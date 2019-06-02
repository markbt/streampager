# streampager (sp)

A pager for command output or large files.

*streampager* accepts streamed input on stdin and presents it for browsing
one page at a time.  It can accept input from additional streams for separate
presentation of the output of multiple commands and their error streams.  It
can also display the contents of files.

## Basic Usage

If invoked with no arguments, *streampager* reads from stdin, expecting to be
invoked as the final command in a pipeline:

    my_command | sp

The default paging behaviour depends on how much data is received:

* For programs that produce less than a screenful of output quickly and then
  exit, *streampager* prints the output to the terminal without paging and also exits.
* For programs that produce output slowly, *streampager* will wait for two seconds
  to see if the program will stop with less than a screenful of output.
  After two seconds *streampager* stops waiting and starts paging.

This behaviour can be disabled, forcing *streampager* to always page the input,
with the `--force` option.

An animated indicator the bottom left of the screen indicates if the input pipe
is still connected.

*streampager* can also be used to display files, and efficiently loads large
files by memory mapping them.

Press **h** or **?** from within *streampager* to display the keyboard shortcuts.
Press **q** to exit.

## Additional Streams

*streampager* can page multiple input streams from different file descriptors
on separate screens.  These additional streams can be passed in using the
`--fd` option.

## Error Streams and Progress Indicators

Input streams that are the error output for a stream can also be provided using
the `--error-fd` option.  As well as being shown on their own screen, the last
8 lines of an error stream are also shown at the bottom of the screen belonging
to the corresponding main stream.

An additional stream for progress indicators can be provided with the
`--progress-fd` option.  This input stream expects to receive progress updates
(e.g. progress bars) terminated by ASCII form-feed characters (`\f` or `x0C`).
*streampager* will display the most recently received progress indicator
at the bottom of the screen.

Progress indicator pages should not contain control codes that are used for
moving the cursor or clearing parts of the display.  Control codes that affect
the color or style of output are accepted and passed through to the terminal.

Calling processes that are using *streampager* to page their own output can
also provide the file descriptor for these streams by setting the
`PAGER_ERROR_FD` and `PAGER_PROGRESS_FD` environment variables.

## Invoking Commands

The `-c` option causes *streampager* to invoke the specified command, and capture
its standard output and standard error streams as separate streams.

For example:

    sp -c "grep -r foo /path"

will run *grep*, and page its output.  Errors from *grep* will
be paged separately from the main output.

The `-c` option can be specified multiple times to run multiple commands
and page all of their outputs as separate streams.

If `sp` is invoked as `spp` then it runs the rest of its command line
arguments as a single command.  For example:

    spp grep -r foo /path

is equivalent to the previous example.

## Keyboard Shortcuts

### General

* **`q`**: Quit.
* **`h`** or **`?`**: Show the help screen.
* **`Esc`**: Close help or any open prompt.

### Navigation

* **Cursor Keys**: Move one line or four columns.
* **`Shift` + Cursor Keys**: Move one quarter of the screen.
* **`Page Up`** and **`Page Down`**: Move a full screen up or down.
* **`Backspace`** and **`Space`**: Move a full screen up or down.
* **`Home`** and **`End`**: Move to the top or bottom of the file.
* **`:`**: Go to a line number or percentage through the file.
* **`[`** and **`]`**: Switch to the previous or next file.

### Presentation

* **`#`**: Toggle display of line numbers.

### Searching

* **`/`**: Search from the top of the file.
* **`<`** and **`>`**: Search backwards or forwards from the current screen.
* **`.`** and **`,`**: Move to the next or previous match.
* **`n`** and **`p`**: Move to the next or previous matching line.
* **`(`** and **`)`**: Move to the first or last match.

## Things Left To Do

* [ ] Toggle line wrapping (Key: **`\`**)
* [ ] Search history and go-to-line history.
* [ ] Handle non-printable characters in prompt input.
* [ ] Wrapping searches and searching backwards.
* [ ] Following the end of files on disk (like `tail -f`).
* [ ] Line ending detection and handling (display `<CR>` in files with mixed line
  endings).
* [ ] Support composing character sequences (e.g. "لآ")
* [ ] Saving content to a file on disk (Key: **`=`**)
