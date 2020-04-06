//! A screen displaying a single file.
use anyhow::Error;
use std::cmp::{max, min, Ordering};
use std::sync::Arc;
use termwiz::cell::{CellAttributes, Intensity};
use termwiz::color::{AnsiColor, ColorAttribute};
use termwiz::input::KeyEvent;
use termwiz::surface::change::Change;
use termwiz::surface::{CursorShape, Position};

use crate::command;
use crate::config::Config;
use crate::display::Action;
use crate::display::Capabilities;
use crate::event::EventSender;
use crate::file::File;
use crate::line::Line;
use crate::line_cache::LineCache;
use crate::progress::Progress;
use crate::prompt::Prompt;
use crate::refresh::Refresh;
use crate::ruler::Ruler;
use crate::search::{MatchMotion, Search, SearchKind};
use crate::util::number_width;

const LINE_CACHE_SIZE: usize = 1000;

/// The position on the screen of the file that is being displayed.
#[derive(Clone, Debug, Default, Eq, PartialEq)]
struct ScreenPosition {
    /// The line at the top of the screen.
    top: usize,

    /// The column at the left of the screen.
    left: usize,

    /// The number of columns on screen.
    width: usize,

    /// The number of rows on screen.
    height: usize,
}

/// A screen that is displaying a single file.
pub(crate) struct Screen {
    /// The file being displayed.
    pub(crate) file: File,

    /// An error file potentially being overlayed.
    error_file: Option<File>,

    /// Progress currently being shown.
    progress: Option<Progress>,

    /// The current position of the file that will be displayed on the next render.
    position: ScreenPosition,

    /// The position of the file that was displayed on the previous render.
    rendered_position: ScreenPosition,

    /// The overlay height on the previous render.
    rendered_overlay_height: usize,

    /// The number of lines in the file on the previous render.
    rendered_lines: usize,

    /// The number of searched lines in the file on the previous render.
    rendered_searched_lines: usize,

    /// The number of error lines on the previous render.
    rendered_error_lines: usize,

    /// The height of the rendered progress.
    rendered_progress_height: usize,

    /// Whether we are following the end of the file.  If `true`, we will scroll down to the
    /// end as new input arrives.
    following_end: bool,

    /// Whether line numbers are being displayed.
    line_numbers: bool,

    /// Cache of `Line`s to display.
    line_cache: LineCache,

    /// Cache of `Line`s for the current search.
    search_line_cache: LineCache,

    /// The current error that should be displayed to the user.
    pub(crate) error: Option<String>,

    /// The current prompt that the user is entering a response into.
    prompt: Option<Prompt>,

    /// The current ongoing search.
    search: Option<Search>,

    /// The ruler.
    ruler: Ruler,

    /// Which parts of the screens need to be re-rendered.
    pending_refresh: Refresh,

    /// Configuration set by the top-level `Pager`.
    config: Arc<Config>,
}

impl Screen {
    /// Create a screen that displays a file.
    pub(crate) fn new(file: File, config: Arc<Config>) -> Screen {
        Screen {
            error_file: None,
            progress: None,
            position: ScreenPosition::default(),
            rendered_position: ScreenPosition::default(),
            rendered_overlay_height: 0,
            rendered_lines: 0,
            rendered_searched_lines: 0,
            rendered_error_lines: 0,
            rendered_progress_height: 0,
            following_end: false,
            line_numbers: false,
            line_cache: LineCache::new(LINE_CACHE_SIZE),
            search_line_cache: LineCache::new(LINE_CACHE_SIZE),
            error: None,
            prompt: None,
            search: None,
            ruler: Ruler::new(file.clone()),
            pending_refresh: Refresh::None,
            config,
            file,
        }
    }

    /// Resize the screen
    pub(crate) fn resize(&mut self, width: usize, height: usize) {
        if self.position.width != width || self.position.height != height {
            self.position.width = width;
            self.position.height = height;
            self.pending_refresh = Refresh::All;
        }
    }

    /// Get the screen width
    pub(crate) fn width(&self) -> usize {
        self.position.width
    }

    /// Renders the part of the screen that has changed.
    pub(crate) fn render(&mut self, caps: &Capabilities) -> Result<Vec<Change>, Error> {
        let mut changes = Vec::new();

        // Hide the cursor while we render things.
        changes.push(Change::CursorShape(CursorShape::Hidden));

        self.ruler.set_position(
            self.position.top,
            self.position.left,
            Some(self.position.top + self.position.height - self.overlay_height())
                .filter(|_| !self.following_end),
        );

        // Work out what needs to be refreshed because we have scrolled.
        let rendered_height = self.rendered_position.height;
        let file_height = rendered_height.saturating_sub(self.rendered_overlay_height);
        if self.pending_refresh != Refresh::All {
            match self.position.top.cmp(&self.rendered_position.top) {
                Ordering::Greater => {
                    if !caps.scroll_up
                        || self.position.top > self.rendered_position.top + file_height
                    {
                        // Can't scroll, or scrolled too far, refresh.
                        self.pending_refresh.add_range(0, file_height);
                    } else {
                        // Moving down the file, so scroll the content up.
                        let scroll_count = self.position.top - self.rendered_position.top;
                        changes.push(Change::ScrollRegionUp {
                            first_row: 0,
                            region_size: file_height,
                            scroll_count,
                        });
                        self.pending_refresh
                            .add_range(file_height - scroll_count, file_height);
                    }
                }
                Ordering::Less => {
                    if !caps.scroll_down
                        || self.position.top + file_height < self.rendered_position.top
                    {
                        // Can't scroll, or scrolled too far, refresh.
                        self.pending_refresh.add_range(0, file_height);
                    } else {
                        // Moving up the file, so scroll the content down.
                        let scroll_count = self.rendered_position.top - self.position.top;
                        changes.push(Change::ScrollRegionDown {
                            first_row: 0,
                            region_size: file_height,
                            scroll_count,
                        });
                        self.pending_refresh.add_range(0, scroll_count);
                    }
                }
                Ordering::Equal => {}
            }
        }

        // Work out what needs to be refreshed because more of the file has loaded.
        let file_lines = self.file.lines();
        if self.pending_refresh != Refresh::All && !self.file.loaded() {
            // Re-render the last rendered line (as it may have been incomplete, and any new lines)
            if self.position.top <= file_lines {
                let start = self.rendered_lines.saturating_sub(self.position.top + 1);
                let end = file_lines - self.position.top;
                self.pending_refresh
                    .add_range(min(start, file_height), min(end, file_height));
            }
        }

        // Work out what needs to be refreshed because the search has progressed.
        let overlay_height = self.overlay_height();
        let searched_lines = match self.search {
            Some(ref search) => {
                let searched_lines = search.searched_lines();
                let start = max(self.position.top, self.rendered_searched_lines);
                let end = min(
                    self.position.top + self.position.height - overlay_height,
                    searched_lines,
                );
                for line in search.matching_lines(start, end).into_iter() {
                    self.pending_refresh
                        .add_range(line - self.position.top, line - self.position.top + 1);
                }
                searched_lines
            }
            None => 0,
        };

        // Work out what needs to be refreshed because the overlay has changed size.
        if self.pending_refresh != Refresh::All && overlay_height != self.rendered_overlay_height {
            self.pending_refresh.add_range(
                rendered_height.saturating_sub(max(overlay_height, self.rendered_overlay_height)),
                rendered_height,
            );
        }

        // Render the lines.
        // TODO: should be possible to do this without collecting the integers,
        // even though the iterator types are different - they all implement
        // `Iterator<Item = usize>`.
        let lines: Vec<usize> = match self.pending_refresh {
            Refresh::None => Vec::new(),
            Refresh::Range(s, e) => (s..e).collect(),
            Refresh::Lines(ref b) => b.iter().collect(),
            Refresh::All => (0..self.position.height).collect(),
        };
        for line in lines.into_iter() {
            if line < self.position.height - overlay_height {
                self.render_line(&mut changes, line)?;
            } else if line < self.position.height {
                self.render_overlay_line(&mut changes, line)?;
            }
        }

        // Set the cursor to the right position and shape.
        if let Some(ref prompt) = self.prompt {
            changes.push(Change::CursorPosition {
                x: Position::Absolute(prompt.cursor_position()),
                y: Position::Absolute(self.prompt_line()),
            });
            changes.push(Change::CursorShape(CursorShape::Default));
        } else {
            changes.push(Change::CursorPosition {
                x: Position::Absolute(0),
                y: Position::Relative(0),
            });
        }

        // Restore attributes to default.
        changes.push(Change::AllAttributes(CellAttributes::default()));

        // Record what we've rendered.
        self.rendered_position.clone_from(&self.position);
        self.rendered_overlay_height = overlay_height;
        self.rendered_lines = file_lines;
        self.rendered_searched_lines = searched_lines;
        self.rendered_error_lines = self.error_file.as_ref().map(|f| f.lines()).unwrap_or(0);
        self.rendered_progress_height = self.progress.as_ref().map(|p| p.lines()).unwrap_or(0);
        self.pending_refresh = Refresh::None;

        Ok(changes)
    }

    /// Render a single line of the overlay.
    fn render_overlay_line(
        &mut self,
        changes: &mut Vec<Change>,
        index: usize,
    ) -> Result<(), Error> {
        let mut line_index = self.position.height - 1;
        if index > line_index {
            // This line is off the bottom of the screen.
            return Ok(());
        }
        if index == line_index && self.overlay_height() == 1 && self.prompt.is_some() {
            // This is the last line of the screen, and the overlay has been collapsed
            // to just the prompt because the screen is too short.
            return self
                .prompt
                .as_mut()
                .unwrap()
                .render(changes, line_index, self.position.width);
        }
        // The remaining parts of the overlay are calculated bottom to top.
        // Progress indicator.
        if let Some(ref progress) = self.progress {
            let progress_height = progress.lines();
            if index >= line_index + 1 - progress_height {
                let progress_line = index - (line_index + 1 - progress_height);
                changes.push(Change::CursorPosition {
                    x: Position::Absolute(0),
                    y: Position::Absolute(index),
                });
                changes.push(Change::AllAttributes(CellAttributes::default()));
                if let Some(line) =
                    progress.with_line(progress_line, |line| Line::new(progress_line, line))
                {
                    line.render(changes, 0, self.position.width, None)?;
                } else {
                    changes.push(Change::ClearToEndOfLine(ColorAttribute::default()));
                }
                return Ok(());
            }
            line_index -= progress_height;
        }
        // Error file.
        if let Some(ref error_file) = self.error_file {
            let error_file_height = self.error_file_height();
            if index >= line_index + 1 - error_file_height {
                let offset = index - (line_index + 1 - error_file_height);
                let error_file_line = error_file.lines() - error_file_height + offset;
                changes.push(Change::CursorPosition {
                    x: Position::Absolute(0),
                    y: Position::Absolute(index),
                });
                changes.push(Change::AllAttributes(CellAttributes::default()));
                let line = error_file
                    .with_line(error_file_line, |line| Line::new(error_file_line, line))
                    .unwrap();
                line.render(changes, 0, self.position.width, None)?;
                return Ok(());
            }
            line_index -= error_file_height;
        }
        // Ruler
        if line_index == index {
            self.ruler
                .bar()
                .render(changes, line_index, self.position.width);
            return Ok(());
        }
        line_index -= 1;
        // Search results.
        if let Some(ref mut search) = self.search {
            if line_index == index {
                return search.render(changes, line_index, self.position.width);
            }
            line_index -= 1;
        }
        // Prompt.
        if let Some(ref mut prompt) = self.prompt {
            if line_index == index {
                return prompt.render(changes, line_index, self.position.width);
            }
            line_index -= 1;
        }
        // Error message.
        if let Some(ref _error) = self.error {
            if line_index == index {
                return self.render_error(changes, line_index);
            }
        }

        Ok(())
    }

    /// Renders a line of the file on the screen.
    fn render_line(&mut self, changes: &mut Vec<Change>, index: usize) -> Result<(), Error> {
        changes.push(Change::CursorPosition {
            x: Position::Absolute(0),
            y: Position::Absolute(index),
        });
        changes.push(Change::AllAttributes(CellAttributes::default()));
        let line_index = self.position.top + index;

        let line = match self.search {
            Some(ref search) if search.line_matches(line_index) => self
                .search_line_cache
                .get_or_create(&self.file, line_index, Some(search.regex())),
            _ => self.line_cache.get_or_create(&self.file, line_index, None),
        };

        let match_index = self
            .search
            .as_ref()
            .and_then(|ref search| search.current_match())
            .and_then(|(match_line_index, match_index)| {
                if match_line_index == line_index {
                    Some(match_index)
                } else {
                    None
                }
            });

        if let Some(line) = line {
            let start = self.position.left;
            let mut end = self.position.left + self.position.width;
            if self.line_numbers {
                let lw = number_width(self.file.lines());
                if lw + 2 < self.position.width {
                    changes.push(Change::AllAttributes(
                        CellAttributes::default()
                            .set_foreground(AnsiColor::Black)
                            .set_background(AnsiColor::Silver)
                            .clone(),
                    ));
                    let s: String = format!(" {:>1$} ", line_index + 1, lw);
                    changes.push(Change::Text(s));
                    changes.push(Change::AllAttributes(CellAttributes::default()));
                    end -= lw + 2;
                }
            }
            line.render(changes, start, end, match_index)?;
        } else {
            changes.push(Change::AllAttributes(
                CellAttributes::default()
                    .set_foreground(AnsiColor::Navy)
                    .set_intensity(Intensity::Bold)
                    .clone(),
            ));
            changes.push(Change::Text("~".into()));
            changes.push(Change::ClearToEndOfLine(ColorAttribute::default()));
        }
        Ok(())
    }

    /// Renders the error message at the bottom of the screen.
    fn render_error(&mut self, changes: &mut Vec<Change>, row: usize) -> Result<(), Error> {
        if let Some(ref error) = self.error {
            changes.push(Change::CursorPosition {
                x: Position::Absolute(0),
                y: Position::Absolute(row),
            });
            changes.push(Change::AllAttributes(
                CellAttributes::default()
                    .set_foreground(AnsiColor::Black)
                    .set_background(AnsiColor::Maroon)
                    .clone(),
            ));
            changes.push(Change::Text(format!("  {}  ", error)));
            changes.push(Change::AllAttributes(CellAttributes::default()));
            changes.push(Change::ClearToEndOfLine(ColorAttribute::default()));
        }
        Ok(())
    }

    /// Refreshes the ruler on the next render.
    pub(crate) fn refresh_ruler(&mut self) {
        let ruler_offset = self.error_file_height() + self.progress_height() + 1;
        if self.position.height > ruler_offset {
            let ruler_line = self.position.height - ruler_offset;
            self.pending_refresh.add_range(ruler_line, ruler_line + 1);
        }
    }

    /// Refreshes the prompt on the next render.
    pub(crate) fn refresh_prompt(&mut self) {
        if self.prompt.is_some() {
            let prompt_line = self.prompt_line();
            self.pending_refresh.add_range(prompt_line, prompt_line + 1);
        }
    }

    /// Refreshes the overlay on the next render.
    pub(crate) fn refresh_overlay(&mut self) {
        let overlay_height = max(self.overlay_height(), self.rendered_overlay_height);
        let start = self.position.height.saturating_sub(overlay_height);
        let end = self.position.height;
        self.pending_refresh.add_range(start, end);
    }

    /// Refreshes the progress section on the next render.
    pub(crate) fn refresh_progress(&mut self) {
        let progress_height = self.progress_height();
        if progress_height != self.rendered_progress_height {
            // Progress height has changed, must re-render the whole overlay.
            self.refresh_overlay();
        } else {
            let progress_height = min(self.position.height, progress_height);
            let start = self.position.height - progress_height;
            let end = self.position.height;
            self.pending_refresh.add_range(start, end);
        }
    }

    /// Refresh a file line.
    pub(crate) fn refresh_file_line(&mut self, file_line_index: usize) {
        if file_line_index >= self.position.top
            && file_line_index < self.position.top + self.position.height - self.overlay_height()
        {
            let line_index = file_line_index - self.position.top;
            self.pending_refresh.add_range(line_index, line_index + 1);
        }
    }

    /// Refresh the line with the current match (if any).
    pub(crate) fn refresh_matched_line(&mut self) {
        if let Some(ref search) = self.search {
            if let Some((line_index, _match_index)) = search.current_match() {
                self.refresh_file_line(line_index);
            }
        }
    }

    /// Refresh all lines with any matches.
    pub(crate) fn refresh_matched_lines(&mut self) {
        if let Some(ref search) = self.search {
            for line in search
                .matching_lines(
                    self.position.top,
                    self.position.top + self.position.height - self.overlay_height(),
                )
                .into_iter()
            {
                self.refresh_file_line(line);
            }
        }
    }

    /// Triggers a full refresh on the next render.
    pub(crate) fn refresh(&mut self) {
        self.pending_refresh = Refresh::All;
    }

    /// Returns the number of overlay rows.
    fn overlay_height(&self) -> usize {
        let overlay_height = 1
            + self.prompt.is_some() as usize
            + self.search.is_some() as usize
            + self.error.is_some() as usize
            + self.error_file_height()
            + self.progress_height();
        if overlay_height < self.position.height {
            overlay_height
        } else {
            // The screen is too short to display the overlay, so just show the
            // prompt.
            self.prompt.is_some() as usize
        }
    }

    /// Returns the line number of the prompt.
    fn prompt_line(&self) -> usize {
        if self.overlay_height() > 1 {
            self.position.height
                - self.error_file_height()
                - self.progress_height()
                - 2
                - self.search.is_some() as usize
        } else {
            self.position.height - 1
        }
    }

    /// Returns the number of lines of the error file to display
    fn error_file_height(&self) -> usize {
        self.error_file
            .as_ref()
            .map(|f| min(f.lines(), 8))
            .unwrap_or(0)
    }

    /// Returns the number of progress lines to display
    fn progress_height(&self) -> usize {
        self.progress.as_ref().map(|f| f.lines()).unwrap_or(0)
    }

    /// Scrolls to the given line number.
    pub(crate) fn scroll_to(&mut self, line: usize) {
        let half_height = (self.position.height - self.overlay_height() - 1) / 2;
        if line < half_height {
            self.scroll_up(self.position.top);
        } else if self.position.top > line - half_height {
            self.scroll_up(self.position.top - (line - half_height));
        } else if self.position.top < line - half_height {
            self.scroll_down(line - half_height - self.position.top);
        }
    }

    /// Scroll the screen `step` characters up.
    fn scroll_up(&mut self, step: usize) {
        let mut step = step;
        self.following_end = false;
        if self.position.top > step {
            self.position.top -= step;
        } else {
            step = self.position.top;
            self.position.top = 0;
        }
        self.pending_refresh.rotate_range_down(step);
        self.refresh_overlay();
    }

    /// Scroll the screen `step` characters down.
    fn scroll_down(&mut self, step: usize) {
        self.following_end = false;
        let mut lines = self.file.lines();
        if !self.config.scroll_past_eof {
            let view_height = self.rendered_position.height - self.rendered_overlay_height;
            lines = lines.max(view_height) - view_height;
        } else {
            // Keep at least one line on screen.
            lines = lines.max(1) - 1;
        }
        let step = step.min(lines.max(self.position.top) - self.position.top);
        self.position.top += step;
        self.pending_refresh.rotate_range_up(step);
        self.refresh_overlay();
    }

    /// Scroll the screen `step` characters to the left.
    fn scroll_left(&mut self, step: usize) {
        let mut step = step;
        if self.position.left > step {
            self.position.left -= step;
        } else {
            step = self.position.left;
            self.position.left = 0;
        }
        if step != 0 {
            self.refresh();
        }
    }

    /// Scroll the screen `step` characters to the right.
    fn scroll_right(&mut self, step: usize) {
        self.position.left += step;
        if step != 0 {
            self.refresh();
        }
    }

    /// Scroll down (screen / n) lines. Negative `n` means scrolling up.
    fn scroll_down_screen(&mut self, n: isize) {
        let lines = (self.rendered_position.height - self.rendered_overlay_height) as isize / n;
        if n >= 0 {
            self.scroll_down((lines as usize).max(1));
        } else {
            self.scroll_up((lines.abs() as usize).max(1));
        }
    }

    /// Dispatch a keypress to navigate the displayed file.
    pub(crate) fn dispatch_key(
        &mut self,
        key: KeyEvent,
        event_sender: &EventSender,
    ) -> Result<Option<Action>, Error> {
        use termwiz::input::{KeyCode::*, Modifiers};
        const CTRL: Modifiers = Modifiers::CTRL;
        const NONE: Modifiers = Modifiers::NONE;
        const SHIFT: Modifiers = Modifiers::SHIFT;
        match (key.modifiers, key.key) {
            (NONE, Char('q')) | (CTRL, Char('C')) => {
                return Ok(Some(Action::Quit));
            }
            (NONE, Escape) => {
                self.error_file = None;
                self.set_search(None);
                self.error = None;
                self.refresh();
                return Ok(Some(Action::ClearOverlay));
            }

            // line
            (NONE, UpArrow) | (NONE, Char('k')) => self.scroll_up(1),
            (NONE, DownArrow) | (NONE, Enter) | (NONE, Char('j')) => self.scroll_down(1),

            // 1/4 screen
            (SHIFT, UpArrow) | (NONE, ApplicationUpArrow) => self.scroll_down_screen(-4),
            (SHIFT, DownArrow) | (NONE, ApplicationDownArrow) => self.scroll_down_screen(4),

            // 1/2 screen
            (CTRL, Char('D')) => self.scroll_down_screen(2),
            (CTRL, Char('U')) => self.scroll_down_screen(-2),

            // 1 screen
            (NONE, PageDown) | (NONE, Char(' ')) | (CTRL, Char('F')) => self.scroll_down_screen(1),
            (NONE, PageUp) | (NONE, Backspace) | (NONE, Char('b')) | (CTRL, Char('B')) => {
                self.scroll_down_screen(-1)
            }

            (NONE, End) | (NONE, Char('G')) => self.following_end = true,
            (NONE, Home) | (NONE, Char('g')) => self.scroll_up(self.position.top),

            (NONE, LeftArrow) => self.scroll_left(4),
            (NONE, RightArrow) => self.scroll_right(4),

            (SHIFT, LeftArrow) | (NONE, ApplicationLeftArrow) => {
                self.scroll_left(self.position.width / 4)
            }
            (SHIFT, RightArrow) | (NONE, ApplicationRightArrow) => {
                self.scroll_right(self.position.width / 4)
            }

            (NONE, Char('[')) => return Ok(Some(Action::PreviousFile)),
            (NONE, Char(']')) => return Ok(Some(Action::NextFile)),

            (NONE, Char('?')) | (NONE, Char('h')) => return Ok(Some(Action::ShowHelp)),
            (NONE, Char('#')) => {
                self.line_numbers = !self.line_numbers;
                return Ok(Some(Action::Refresh));
            }
            (NONE, Char(':')) => self.prompt = Some(command::goto()),
            (NONE, Char('/')) => {
                self.prompt = Some(command::search(SearchKind::First, event_sender.clone()));
            }
            (NONE, Char('>')) => {
                self.prompt = Some(command::search(
                    SearchKind::FirstAfter(self.position.top),
                    event_sender.clone(),
                ));
            }
            (NONE, Char('<')) => {
                self.prompt = Some(command::search(
                    SearchKind::FirstBefore(
                        self.position.top + self.position.height - self.overlay_height(),
                    ),
                    event_sender.clone(),
                ));
            }
            (NONE, Char(',')) => self.move_match(MatchMotion::Previous),
            (NONE, Char('.')) => self.move_match(MatchMotion::Next),
            (NONE, Char('p')) | (NONE, Char('N')) => self.move_match(MatchMotion::PreviousLine),
            (NONE, Char('n')) => self.move_match(MatchMotion::NextLine),
            (NONE, Char('(')) => self.move_match(MatchMotion::First),
            (NONE, Char(')')) => self.move_match(MatchMotion::Last),
            _ => {}
        }
        Ok(Some(Action::Render))
    }

    /// Set the search for this file.
    pub(crate) fn set_search(&mut self, search: Option<Search>) {
        self.search = search;
        self.search_line_cache.clear();
    }

    /// Set the error file for this file.
    pub(crate) fn set_error_file(&mut self, error_file: Option<File>) {
        self.error_file = error_file;
    }

    /// Set the progress indicator for this file.
    pub(crate) fn set_progress(&mut self, progress: Option<Progress>) {
        self.progress = progress;
    }

    /// Returns true if this screen is currently animating for any reason.
    pub(crate) fn animate(&self) -> bool {
        self.error_file.is_some()
            || (!self.file.loaded() && !self.file.paused())
            || self.following_end
            || self
                .search
                .as_ref()
                .map(|search| !search.finished())
                .unwrap_or(false)
    }

    /// Dispatch an animation timeout, updating for the next animation frame.
    pub(crate) fn dispatch_animation(&mut self) -> Result<Option<Action>, Error> {
        if self.following_end {
            let lines = self.file.lines();
            let new_top = if lines <= self.rendered_position.height - self.rendered_overlay_height {
                0
            } else {
                lines - (self.rendered_position.height - self.rendered_overlay_height)
            };
            if self.position.top != new_top {
                self.position.top = new_top;
                self.refresh_ruler();
            }
        }
        if !self.file.loaded() {
            self.refresh_ruler();
        }
        if self
            .search
            .as_ref()
            .map(|search| !search.finished())
            .unwrap_or(false)
        {
            self.refresh_overlay();
        }
        if let Some(ref error_file) = self.error_file {
            if error_file.lines() != self.rendered_error_lines {
                self.refresh_overlay();
            }
        }
        match &self.pending_refresh {
            Refresh::None => Ok(None),
            _ => Ok(Some(Action::Render)),
        }
    }

    pub(crate) fn prompt(&mut self) -> &mut Option<Prompt> {
        &mut self.prompt
    }

    /// Clears the prompt from the screen.
    pub(crate) fn clear_prompt(&mut self) {
        // Refresh the prompt before we remove it, so that we know which line to refresh.
        self.refresh_prompt();
        self.prompt = None;
    }

    /// Called when a search finds its first match in order to scroll to that match.
    pub(crate) fn search_first_match(&mut self) -> Option<Action> {
        let current_match = self
            .search
            .as_ref()
            .and_then(|ref search| search.current_match());
        if let Some((line_index, _match_index)) = current_match {
            self.scroll_to(line_index);
            self.refresh_matched_lines();
            self.refresh_overlay();
            return Some(Action::Render);
        }
        None
    }

    /// Called when a search completes.
    pub(crate) fn search_finished(&mut self) -> Option<Action> {
        self.refresh_matched_lines();
        self.refresh_overlay();
        Some(Action::Render)
    }

    /// Move the currently selected match to a new match.
    pub(crate) fn move_match(&mut self, motion: MatchMotion) {
        self.refresh_matched_line();
        if let Some(ref mut search) = self.search {
            search.move_match(motion);
            if let Some((line_index, _match_index)) = search.current_match() {
                self.scroll_to(line_index);
            }
            self.refresh_matched_line();
        }
    }

    pub(crate) fn flush_line_caches(&mut self) {
        self.line_cache.clear();
        self.search_line_cache.clear();
    }

    /// Load more lines from a stream.
    pub(crate) fn maybe_load_more(&mut self) {
        // Fetch 1 screen + config.read_ahead_lines.
        let needed_lines =
            self.position.top + self.position.height * 2 + self.config.read_ahead_lines;
        self.file.set_needed_lines(needed_lines);
    }
}
