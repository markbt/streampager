//! Stream Pager Process
//!
//! A pager for command output.
#![warn(missing_docs)]

use std::env;
use std::ffi::{OsStr, OsString};
use std::fmt::Write;

use anyhow::{bail, Error};

use streampager::Pager;

/// Main.
fn main() {
    let rc = match start_command() {
        Ok(()) => 0,
        Err(err) => {
            let mut message = String::new();
            for cause in err.chain() {
                write!(message, ": {}", cause).expect("format write should not fail");
            }
            eprintln!("spp{}", message);
            1
        }
    };

    std::process::exit(rc)
}

/// Start a command and page the output.
fn start_command() -> Result<(), Error> {
    let mut pager = Pager::new_using_system_terminal()?;
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
    pager.add_subprocess(&args[1], &args[2..], &title)?;
    pager.run()?;
    Ok(())
}
