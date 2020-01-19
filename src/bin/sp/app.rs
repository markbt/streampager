//! Command line definition for sp.

use clap::{App, Arg};

pub(crate) fn app() -> App<'static, 'static> {
    let app = App::new("sp")
        .version(env!("CARGO_PKG_VERSION"))
        .about("Stream Pager")
        .arg(
            Arg::with_name("FILE")
                .help("Displays the contents of this file")
                .multiple(true),
        )
        .arg(
            Arg::with_name("command")
                .long("command")
                .short("c")
                .value_name("\"COMMAND ARGS...\"")
                .help("Runs the command in a subshell and displays its output and error streams")
                .multiple(true),
        )
        .arg(
            Arg::with_name("fullscreen")
                .long("fullscreen")
                .short("F")
                .overrides_with_all(&["delayed", "no_alternate"])
                .help("Enter full screen immediately")
        )
        .arg(
            Arg::with_name("delayed")
                .long("delayed")
                .short("D")
                .value_name("SEC")
                .overrides_with_all(&["fullscreen", "no_alternate"])
                .help("Enter full screen after SEC seconds without waiting for content to fill one screen."),
        )
        .arg(
            Arg::with_name("no_alternate")
                .long("no-alternate")
                .short("X")
                .overrides_with_all(&["fullscreen", "delayed"])
                .help("Disables using the alternate screen. Enables streaming output before full screen."),
        );
    if cfg!(unix) {
        app.arg(
            Arg::with_name("fd")
                .long("fd")
                .value_name("FD[=TITLE]")
                .help("Displays the contents of this file descriptor")
                .multiple(true),
        )
        .arg(
            Arg::with_name("error_fd")
                .long("error-fd")
                .value_name("FD[=TITLE]")
                .help("Displays the contents of this file descriptor as the error stream of the previous file or file descriptor")
                .multiple(true),
        )
        .arg(
            Arg::with_name("progress_fd")
                .long("progress-fd")
                .value_name("FD")
                .help("Displays pages from this file descriptor as progress indicators"),
        )
    } else {
        app
    }
}
