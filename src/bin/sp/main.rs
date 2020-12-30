//! Stream Pager
//!
//! A pager for command output or large files.
#![warn(missing_docs)]

use std::env;
use std::ffi::{OsStr, OsString};
use std::fmt::Write;
#[cfg(unix)]
use std::os::unix::io::{FromRawFd, RawFd};
#[cfg(unix)]
use std::str::FromStr;
use std::time::Duration;

use anyhow::{bail, Error};
use clap::ArgMatches;
use termwiz::istty::IsTty;
use vec_map::VecMap;

use streampager::{config::InterfaceMode, config::WrappingMode, Pager};

mod app;

/// Main.
fn main() {
    let args = app::app().get_matches();
    let rc = match open_files(args) {
        Ok(()) => 0,
        Err(err) => {
            let mut message = String::new();
            for cause in err.chain() {
                write!(message, ": {}", cause).expect("format write should not fail");
            }
            eprintln!("sp{}", message);
            1
        }
    };

    std::process::exit(rc)
}

/// A specification of a file to display.
enum FileSpec {
    Stdin,
    Named(OsString),
    #[cfg(unix)]
    Fd(RawFd, String),
    #[cfg(unix)]
    ErrorFd(RawFd, String),
    Command(OsString),
}

/// Run the pager, opening files or file descriptors (including stdin).
fn open_files(args: ArgMatches) -> Result<(), Error> {
    let mut pager = Pager::new_using_system_terminal()?;
    if args.is_present("no_alternate") {
        pager.set_interface_mode(InterfaceMode::Hybrid);
    } else if args.is_present("fullscreen") {
        pager.set_interface_mode(InterfaceMode::FullScreen);
    } else if let Some(delay) = args.value_of("delayed") {
        let delay = delay.parse::<u64>()?;
        if delay == 0 {
            pager.set_interface_mode(InterfaceMode::FullScreen);
        } else {
            pager.set_interface_mode(InterfaceMode::Delayed(Duration::from_secs(delay)));
        }
    }

    if args.is_present("no_alternate") {
        pager.set_wrapping_mode(WrappingMode::GraphemeBoundary);
    }

    let mut specs = VecMap::new();

    // Collect file specifications from arguments.
    if let (Some(filenames), Some(indices)) = (args.values_of_os("FILE"), args.indices_of("FILE")) {
        for (filename, index) in filenames.zip(indices) {
            specs.insert(index, FileSpec::Named(filename.to_os_string()));
        }
    }

    #[cfg(unix)]
    {
        // Collect file specifications from --fd arguments.
        if let (Some(fds), Some(indices)) = (args.values_of_lossy("fd"), args.indices_of("fd")) {
            for (fd_spec, index) in fds.iter().zip(indices) {
                let (fd, title) = parse_fd_title(&fd_spec)?;
                let title = title.unwrap_or(&fd_spec);
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

        #[cfg(unix)]
        {
            if let Ok(fd_spec) = env::var("PAGER_ERROR_FD") {
                if let Ok((fd, title)) = parse_fd_title(&fd_spec) {
                    let title = title.unwrap_or("STDERR");
                    specs.insert(1, FileSpec::ErrorFd(fd, title.to_string()));
                }
            }
        }
    }

    #[cfg(unix)]
    {
        if let Some(fd_spec) = env::var("PAGER_PROGRESS_FD")
            .ok()
            .as_ref()
            .map(String::as_ref)
            .or_else(|| args.value_of("progress_fd"))
        {
            if let Ok(fd) = fd_spec.parse::<RawFd>() {
                let file = unsafe { std::fs::File::from_raw_fd(fd) };
                pager.set_progress_stream(file);
            }
        }
    }

    for (_index, spec) in specs.iter() {
        match spec {
            FileSpec::Stdin => {
                let title = env::var("PAGER_TITLE").ok();
                let title = title.as_ref().map(String::as_ref).unwrap_or("");
                pager.add_output_stream(std::io::stdin(), title)?;
            }
            FileSpec::Named(filename) => {
                pager.add_output_file(filename)?;
            }
            #[cfg(unix)]
            FileSpec::Fd(fd, title) => {
                let stream = unsafe { std::fs::File::from_raw_fd(*fd) };
                pager.add_output_stream(stream, title)?;
            }
            #[cfg(unix)]
            FileSpec::ErrorFd(fd, title) => {
                let stream = unsafe { std::fs::File::from_raw_fd(*fd) };
                pager.add_error_stream(stream, title)?;
            }
            FileSpec::Command(command) => {
                let (shell, flag) = if cfg!(windows) {
                    ("cmd.exe", "/C")
                } else {
                    ("/bin/sh", "-c")
                };
                pager.add_subprocess(
                    OsStr::new(shell),
                    &[OsStr::new(flag), command],
                    &command.to_string_lossy(),
                )?;
            }
        }
    }
    pager.run()?;
    Ok(())
}

#[cfg(unix)]
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
