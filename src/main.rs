//! Stream Pager
//!
//! A pager for command output or large files.
#![warn(missing_docs)]

use clap::ArgMatches;
use failure::{bail, Error};
use std::env;
use std::ffi::{OsStr, OsString};
use std::fmt::Write;
use std::os::unix::io::RawFd;
use std::path::Path;
use std::str::FromStr;
use std::time;
use termwiz::caps::{Capabilities, ProbeHintsBuilder};
use termwiz::input::InputEvent;
use termwiz::istty::IsTty;
use termwiz::surface::{change::Change, Position};
use termwiz::terminal::{SystemTerminal, Terminal};
use vec_map::VecMap;

mod app;
mod buffer;
mod command;
mod display;
mod event;
mod file;
mod line;
mod line_cache;
mod overstrike;
mod progress;
mod prompt;
mod refresh;
mod screen;
mod search;

use event::{Event, EventStream};
use file::File;
use line::Line;
use progress::Progress;

/// Main.
fn main() {
    let (prog_name, spec) = if called_as_spp() {
        ("spp", start_command())
    } else {
        ("sp", open_files(parse_args()))
    };

    let rc = match spec.and_then(run) {
        Ok(()) => 0,
        Err(err) => {
            let mut message = String::new();
            for cause in err.iter_chain() {
                write!(message, ": {}", cause).expect("format write should not fail");
            }
            eprintln!("{}{}", prog_name, message);
            1
        }
    };

    std::process::exit(rc)
}

/// Determine whether the process was called as `spp`.
fn called_as_spp() -> bool {
    if let Some(name) = env::args_os().next() {
        return Path::new(&name).file_name() == Some(OsStr::new("spp"));
    }
    false
}

/// Parse arguments.
fn parse_args() -> ArgMatches<'static> {
    app::app().get_matches()
}

/// A specification of a file to display.
enum FileSpec {
    Stdin,
    Named(OsString),
    Fd(RawFd, String),
    ErrorFd(RawFd, String),
    Command(OsString),
}

/// A specification of what `sp` should do.
struct Spec {
    /// The Terminal.
    term: SystemTerminal,

    /// The Terminal's capabilites.
    caps: Capabilities,

    /// Event Stream to process.
    events: EventStream,

    /// Files to load.
    files: Vec<File>,

    /// Error file mapping.  Maps file indices to the associated error files.
    error_files: VecMap<File>,

    /// Progress indicators to display.
    progress: Option<Progress>,

    /// Whether `sp` should wait to see if enough input is generated to fill
    /// the screen.
    delay_fullscreen: bool,
}

/// Determine terminal capabilities and open the terminal.
fn open_terminal() -> Result<(SystemTerminal, Capabilities), Error> {
    // Get terminal capabilities from the environment, but disable mouse
    // reporting, as we don't want to change the terminal's mouse handling.
    let caps = Capabilities::new_with_hints(
        ProbeHintsBuilder::new_from_env()
            .mouse_reporting(Some(false))
            .build()
            .map_err(failure::err_msg)?,
    )?;
    let mut term = SystemTerminal::new(caps.clone())?;
    term.set_raw_mode()?;
    Ok((term, caps))
}

/// Build a `Spec` for starting a subcommand.  Used when `sp` is called as
/// `spp command and args`.
fn start_command() -> Result<Spec, Error> {
    let (term, caps) = open_terminal()?;
    let events = EventStream::new(term.waker());
    let mut files = Vec::new();
    let mut error_files = VecMap::new();
    let args: Vec<_> = env::args_os().collect();
    if args.len() < 2 {
        bail!("expected command to run")
    }
    let title = &args[1..]
        .iter()
        .map(OsString::as_os_str)
        .map(OsStr::to_string_lossy)
        .collect::<Vec<_>>()
        .join(" ");
    let (out_file, err_file) =
        File::new_command(files.len(), &args[1], &args[2..], &title, events.sender())?;
    error_files.insert(out_file.index(), err_file.clone());
    files.push(out_file);
    files.push(err_file);

    Ok(Spec {
        term,
        caps,
        events,
        files,
        error_files,
        progress: None,
        delay_fullscreen: true,
    })
}

/// Build a `Spec` for opening files or file descriptors (including stdin).
/// Used when `sp` is called normally.
fn open_files(args: ArgMatches) -> Result<Spec, Error> {
    let (term, caps) = open_terminal()?;
    let events = EventStream::new(term.waker());
    let mut specs = VecMap::new();
    let mut files = Vec::new();
    let mut error_files = VecMap::new();
    let mut progress = None;
    let mut delay_fullscreen = false;

    // Collect file specifications from arguments.
    if let (Some(filenames), Some(indices)) = (args.values_of_os("FILE"), args.indices_of("FILE")) {
        for (filename, index) in filenames.zip(indices) {
            specs.insert(index, FileSpec::Named(filename.to_os_string()));
        }
    }

    // Collect file specifications from --fd arguments.
    if let (Some(fds), Some(indices)) = (args.values_of_lossy("fd"), args.indices_of("fd")) {
        for (fd_spec, index) in fds.iter().zip(indices) {
            let (fd, title) = parse_fd_title(fd_spec)?;
            let title = title.unwrap_or(fd_spec);
            specs.insert(index, FileSpec::Fd(fd, title.to_string()));
        }
    }

    // Collect file specifications from --error-fd arguments.
    if let (Some(fds), Some(indices)) = (
        args.values_of_lossy("error_fd"),
        args.indices_of("error_fd"),
    ) {
        for (fd_spec, index) in fds.iter().zip(indices) {
            let (fd, title) = parse_fd_title(&fd_spec)?;
            let title = title.unwrap_or(&fd_spec);
            specs.insert(index, FileSpec::ErrorFd(fd, title.to_string()));
        }
    }

    // Collect file specifications from --command arguments.
    if let (Some(commands), Some(indices)) =
        (args.values_of_os("command"), args.indices_of("command"))
    {
        for (command, index) in commands.zip(indices) {
            specs.insert(index, FileSpec::Command(command.to_os_string()));
        }
    }

    if specs.is_empty() {
        if std::io::stdin().is_tty() {
            bail!("expected filename or piped input");
        }

        // Nothing specified on the command line - page standard streams.
        specs.insert(0, FileSpec::Stdin);

        if let Ok(fd_spec) = env::var("PAGER_ERROR_FD") {
            if let Ok((fd, title)) = parse_fd_title(&fd_spec) {
                let title = title.unwrap_or("STDERR");
                specs.insert(1, FileSpec::ErrorFd(fd, title.to_string()));
            }
        }

        if !args.is_present("force") {
            delay_fullscreen = true;
        }
    }

    if let Some(fd_spec) = env::var("PAGER_PROGRESS_FD")
        .ok()
        .as_ref()
        .map(String::as_ref)
        .or_else(|| args.value_of("progress_fd"))
    {
        if let Ok(fd) = fd_spec.parse::<RawFd>() {
            progress = Some(Progress::new(fd, events.sender()));
        }
    }

    for (_index, spec) in specs.iter() {
        match spec {
            FileSpec::Stdin => {
                let title = env::var("PAGER_TITLE").ok();
                let title = title.as_ref().map(String::as_ref).unwrap_or("");
                files.push(File::new_stdin(files.len(), title, events.sender())?);
            }
            FileSpec::Named(filename) => {
                files.push(File::new_mapped(files.len(), filename, events.sender())?);
            }
            FileSpec::Fd(fd, title) => {
                files.push(File::new_fd(files.len(), *fd, title, events.sender())?);
            }
            FileSpec::ErrorFd(fd, title) => {
                let file = File::new_fd(files.len(), *fd, title, events.sender())?;
                if let Some(last_file) = files.last() {
                    error_files.insert(last_file.index(), file.clone());
                }
                files.push(file);
            }
            FileSpec::Command(command) => {
                let (out_file, err_file) = File::new_command(
                    files.len(),
                    OsStr::new("/bin/sh"),
                    &[OsStr::new("-c"), command],
                    &command.to_string_lossy(),
                    events.sender(),
                )?;
                error_files.insert(out_file.index(), err_file.clone());
                files.push(out_file);
                files.push(err_file);
            }
        }
    }
    Ok(Spec {
        term,
        caps,
        events,
        files,
        error_files,
        progress,
        delay_fullscreen,
    })
}

/// Run Stream Pager.
fn run(mut spec: Spec) -> Result<(), Error> {
    // If we are delaying fullsceeen (e.g. because wre are paging stdin without --force)
    // then wait for up to two seconds to see if this is a small amount of
    // output that we don't need to page.
    if spec.delay_fullscreen {
        let load_delay = time::Duration::from_millis(2000);
        if wait_for_screenful(&spec.files, &mut spec.term, &mut spec.events, load_delay)? {
            // The input streams have all completed and they fit on a single
            // screen, just write them out and stop.
            let mut changes = Vec::new();
            for file in spec.files.iter() {
                for i in 0..file.lines() {
                    if let Some(line) = file.with_line(i, |line| Line::new(i, line)) {
                        line.render_full(&mut changes)?;
                        changes.push(Change::CursorPosition {
                            x: Position::Absolute(0),
                            y: Position::Relative(1),
                        });
                    }
                }
            }
            spec.term.render(changes.as_slice())?;
            return Ok(());
        }
    }

    display::start(
        spec.term,
        spec.caps,
        spec.events,
        spec.files,
        spec.error_files,
        spec.progress,
    )
}

/// Parse a file description and title specification.
///
/// Parses `FD[=TITLE]` and returns the FD and the optional title.
fn parse_fd_title(fd_spec: &str) -> Result<(RawFd, Option<&str>), <RawFd as FromStr>::Err> {
    if let Some(eq) = fd_spec.find('=') {
        Ok((fd_spec[..eq].parse::<RawFd>()?, Some(&fd_spec[eq + 1..])))
    } else {
        Ok((fd_spec.parse::<RawFd>()?, None))
    }
}

/// Poll the event stream, waiting until either the file has finished loading,
/// the file definitely doesn't fit on the screen, or the load delay has passed.
///
/// If the file has finished loading and the file fits on the screen, returns
/// true.  Otherwise, returns false.
fn wait_for_screenful<T: Terminal>(
    files: &[File],
    term: &mut T,
    events: &mut EventStream,
    load_delay: time::Duration,
) -> Result<bool, Error> {
    let load_start = time::Instant::now();
    let mut size = term.get_screen_size()?;
    let mut loaded: Vec<bool> = files.iter().map(|_| false).collect();
    while load_start.elapsed() < load_delay {
        match events.get(term, Some(time::Duration::from_millis(50)))? {
            Some(Event::Loaded(i)) => {
                loaded[i] = true;
                if loaded.iter().all(|l| *l) {
                    if files_fit(files, size.cols, size.rows) {
                        return Ok(true);
                    }
                    break;
                }
            }
            Some(Event::Input(InputEvent::Resized { .. })) => {
                size = term.get_screen_size()?;
            }
            Some(Event::Input(InputEvent::Key(_))) => break,
            _ => {}
        }
        if !files_fit(files, size.cols, size.rows) {
            break;
        }
    }
    Ok(false)
}

/// Returns true if the given files fit on a screen of dimensions `w` x `h`.
fn files_fit(files: &[File], w: usize, h: usize) -> bool {
    let mut wrapped_lines = 0;
    for file in files.iter() {
        let lines = file.lines();
        if wrapped_lines + lines > h {
            return false;
        }
        for i in 0..lines {
            wrapped_lines += file
                .with_line(i, |line| {
                    let line = Line::new(i, line);
                    line.height(w)
                })
                .unwrap_or(0);
            if wrapped_lines > h {
                return false;
            }
        }
    }
    true
}
