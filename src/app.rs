//! Command Line Definitions.
use clap::{App, Arg};

pub(crate) fn app() -> App<'static, 'static> {
    App::new("sp")
        .version(env!("CARGO_PKG_VERSION"))
        .about("Stream Pager")
        .arg(
            Arg::with_name("FILE")
                .help("Displays the contents of this file")
                .multiple(true),
        )
        .arg(
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
            Arg::with_name("command")
                .long("command").short("c")
                .value_name("\"COMMAND ARGS...\"")
                .help("Runs the command in a subshell and displays its output and error streams")
                .multiple(true),
        )
        .arg(
            Arg::with_name("progress_fd")
                .long("progress-fd")
                .value_name("FD")
                .help("Displays pages from this file descriptor as progress indicators"),
        )
        .arg(
            Arg::with_name("force")
                .long("force")
                .help("Start paging immediately, don't wait to see if input is short"),
        )
}
