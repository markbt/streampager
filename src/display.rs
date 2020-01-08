//! Manage the Display.
use anyhow::Error;
use scopeguard::guard;
use std::sync::Arc;
use std::time::Duration;
use termwiz::caps::Capabilities as TermCapabilities;
use termwiz::cell::CellAttributes;
use termwiz::input::InputEvent;
use termwiz::surface::change::Change;
use termwiz::surface::{CursorShape, Position};
use termwiz::terminal::Terminal;
use vec_map::VecMap;

use crate::command;
use crate::config::Config;
use crate::direct;
use crate::event::{Event, EventStream, UniqueInstance};
use crate::file::File;
use crate::progress::Progress;
use crate::screen::Screen;
use crate::search::SearchKind;

/// Capabilities of the terminal that we care about.
#[derive(Default)]
pub(crate) struct Capabilities {
    pub(crate) scroll_up: bool,
    pub(crate) scroll_down: bool,
}

impl Capabilities {
    fn new(term_caps: TermCapabilities) -> Capabilities {
        use terminfo::capability as cap;
        let mut caps = Capabilities::default();
        if let Some(db) = term_caps.terminfo_db() {
            if db.get::<cap::ChangeScrollRegion>().is_some() {
                caps.scroll_up = db.get::<cap::ParmIndex>().is_some()
                    || (db.get::<cap::CursorAddress>().is_some()
                        && db.get::<cap::ScrollForward>().is_some());
                caps.scroll_down = db.get::<cap::ParmRindex>().is_some()
                    || (db.get::<cap::CursorAddress>().is_some()
                        && db.get::<cap::ScrollReverse>().is_some());
            }
        }
        caps
    }
}

/// An action that affects the display.
pub(crate) enum Action {
    /// Run a function.  The function may return a new action to run next.
    Run(Box<dyn FnMut(&mut Screen) -> Result<Option<Action>, Error>>),

    /// Change the terminal.
    Change(Change),

    /// Render the parts of the screen that have changed.
    Render,

    /// Render the whole screen.
    Refresh,

    /// Render the prompt.
    RefreshPrompt,

    /// Move to the next file.
    NextFile,

    /// Move to the previous file.
    PreviousFile,

    /// Show the help screen.
    ShowHelp,

    /// Clear the overlay.
    ClearOverlay,

    /// Close the program.
    Quit,
}

/// Container for all screens.
struct Screens {
    /// The loaded files.
    screens: Vec<Screen>,

    /// An overlaid screen (e.g. the help screen).
    overlay: Option<Screen>,

    /// The currently active screen.
    current_index: usize,

    /// The file index of the overlay.  While overlays aren't part of the
    /// screens vector, we still need a file index so that the file loader can
    /// report loading completion and the search thread can report search
    /// matches.  Use an index starting after the loaded files for this purpose.
    /// Each time a new overlay is added, this index is incremented, so that
    /// each overlay gets a unique index.
    overlay_index: usize,
}

impl Screens {
    /// Create a new screens container for the given files.
    fn new(
        files: Vec<File>,
        mut error_files: VecMap<File>,
        progress: Option<Progress>,
        config: Arc<Config>,
    ) -> Screens {
        let count = files.len();
        let mut screens = Vec::new();
        for file in files.into_iter() {
            let index = file.index();
            let mut screen = Screen::new(file, config.clone());
            screen.set_progress(progress.clone());
            screen.set_error_file(error_files.remove(index));
            screens.push(screen);
        }
        Screens {
            screens,
            overlay: None,
            current_index: 0,
            overlay_index: count,
        }
    }

    /// Get the current screen.
    fn current(&mut self) -> &mut Screen {
        if let Some(ref mut screen) = self.overlay {
            screen
        } else {
            &mut self.screens[self.current_index]
        }
    }

    /// True if the given index is the index of the currently visible screen.
    fn is_current_index(&self, index: usize) -> bool {
        match self.overlay {
            Some(_) => index == self.overlay_index,
            None => index == self.current_index,
        }
    }

    /// Get the screen with the given index.
    fn get(&mut self, index: usize) -> Option<&mut Screen> {
        if index == self.overlay_index {
            self.overlay.as_mut()
        } else if index < self.screens.len() {
            Some(&mut self.screens[index])
        } else {
            None
        }
    }
}

/// Start displaying files.
pub(crate) fn start(
    mut term: impl Terminal,
    term_caps: TermCapabilities,
    mut events: EventStream,
    files: Vec<File>,
    error_files: VecMap<File>,
    progress: Option<Progress>,
    config: Config,
) -> Result<(), Error> {
    let outcome = {
        // Only take the first output and error. This emulates the behavior that
        // the main pager can only display one stream at a time.
        let output_files = &files[0..1.min(files.len())];
        let error_files = match error_files.iter().nth(0) {
            None => Vec::new(),
            Some((_i, file)) => vec![file.clone()],
        };
        direct::direct(
            &mut term,
            output_files,
            &error_files[..],
            progress.as_ref(),
            &mut events,
            config.interface_mode,
        )?
    };
    match outcome {
        direct::Outcome::RenderComplete | direct::Outcome::Interrupted => return Ok(()),
        direct::Outcome::RenderIncomplete => (),
        direct::Outcome::RenderNothing => term.enter_alternate_screen()?,
    }

    let mut term = guard(term, |mut term| {
        // Clean up when exiting.  Most of this should be achieved by exiting
        // the alternate screen, but just in case it isn't, move to the
        // bottom of the screen and reset all attributes.
        let size = term.get_screen_size().unwrap();
        term.render(&[
            Change::CursorShape(CursorShape::Default),
            Change::AllAttributes(CellAttributes::default()),
            Change::ScrollRegionUp {
                first_row: 0,
                region_size: size.rows,
                scroll_count: 1,
            },
            Change::CursorPosition {
                x: Position::Absolute(0),
                y: Position::Absolute(size.rows),
            },
        ])
        .unwrap();
    });
    let config = Arc::new(config);
    let caps = Capabilities::new(term_caps);
    let mut screens = Screens::new(files, error_files, progress, config.clone());
    let event_sender = events.sender();
    let render_unique = UniqueInstance::new();
    let refresh_unique = UniqueInstance::new();
    {
        let screen = screens.current();
        let size = term.get_screen_size()?;
        screen.resize(size.cols, size.rows);
        term.render(&screen.render(&caps)?)?;
    }
    loop {
        // Listen for an event or input.  If we are animating, put a timeout on the wait.
        let timeout = if screens.current().animate() {
            Some(Duration::from_millis(100))
        } else {
            None
        };
        let event = events.get(&mut *term, timeout)?;

        // Dispatch the event and receive an action to take.
        let mut action = {
            let screen = screens.current();
            match event {
                None => screen.dispatch_animation()?,
                Some(Event::Render) => {
                    term.render(&screen.render(&caps)?)?;
                    None
                }
                Some(Event::Input(InputEvent::Resized { .. })) => {
                    let size = term.get_screen_size()?;
                    screen.resize(size.cols, size.rows);
                    term.render(&screen.render(&caps)?)?;
                    None
                }
                Some(Event::Refresh) => {
                    let size = term.get_screen_size()?;
                    screen.resize(size.cols, size.rows);
                    screen.refresh();
                    term.render(&screen.render(&caps)?)?;
                    None
                }
                Some(Event::Progress) => {
                    screen.refresh_progress();
                    term.render(&screen.render(&caps)?)?;
                    None
                }
                Some(Event::Input(InputEvent::Key(key))) => {
                    let width = screen.width();
                    if let Some(prompt) = screen.prompt() {
                        prompt.dispatch_key(key, width)?
                    } else {
                        screen.dispatch_key(key, &event_sender)?
                    }
                }
                Some(Event::Input(InputEvent::Paste(ref text))) => {
                    let width = screen.width();
                    screen
                        .prompt()
                        .get_or_insert_with(|| {
                            // Assume the user wanted to search for what they're pasting.
                            command::search(SearchKind::First, event_sender.clone())
                        })
                        .paste(text, width)?
                }
                Some(Event::Loaded(index)) if screens.is_current_index(index) => {
                    Some(Action::Refresh)
                }
                Some(Event::SearchFirstMatch(index)) => screens
                    .get(index)
                    .and_then(|screen| screen.search_first_match()),
                Some(Event::SearchFinished(index)) => screens
                    .get(index)
                    .and_then(|screen| screen.search_finished()),
                _ => None,
            }
        };

        // Process the action.  We may get new actions in return from the action.
        while let Some(current_action) = action.take() {
            match current_action {
                Action::Run(mut f) => action = f(screens.current())?,
                Action::Change(c) => {
                    term.render(&[c])?;
                }
                Action::Render => event_sender.send_unique(Event::Render, &render_unique)?,
                Action::Refresh => event_sender.send_unique(Event::Refresh, &refresh_unique)?,
                Action::RefreshPrompt => {
                    screens.current().refresh_prompt();
                    event_sender.send_unique(Event::Render, &render_unique)?;
                }
                Action::NextFile => {
                    screens.overlay = None;
                    if screens.current_index < screens.screens.len() - 1 {
                        screens.current_index += 1;
                        let screen = screens.current();
                        let size = term.get_screen_size()?;
                        screen.resize(size.cols, size.rows);
                        screen.refresh();
                        term.render(&screen.render(&caps)?)?;
                    }
                }
                Action::PreviousFile => {
                    screens.overlay = None;
                    if screens.current_index > 0 {
                        screens.current_index -= 1;
                        let screen = screens.current();
                        let size = term.get_screen_size()?;
                        screen.resize(size.cols, size.rows);
                        screen.refresh();
                        term.render(&screen.render(&caps)?)?;
                    }
                }
                Action::ShowHelp => {
                    let overlay_index = screens.overlay_index + 1;
                    let mut screen = Screen::new(
                        File::new_static(
                            overlay_index,
                            "HELP",
                            include_bytes!("help.txt"),
                            event_sender.clone(),
                        )?,
                        config.clone(),
                    );
                    let size = term.get_screen_size()?;
                    screen.resize(size.cols, size.rows);
                    screen.refresh();
                    term.render(&screen.render(&caps)?)?;
                    screens.overlay = Some(screen);
                    screens.overlay_index = overlay_index;
                }
                Action::ClearOverlay => {
                    screens.overlay = None;
                    let screen = screens.current();
                    let size = term.get_screen_size()?;
                    screen.resize(size.cols, size.rows);
                    screen.refresh();
                    term.render(&screen.render(&caps)?)?;
                }
                Action::Quit => {
                    return Ok(());
                }
            }
        }
    }
}
