sp(1)
=====

Name
----
sp - a pager for command output or large files

Synopsis
--------

*command* | *sp* [_OPTIONS_]

*sp* [_OPTIONS_] [_FILE_...]

*sp* [_OPTIONS_] *-c* "_COMMAND_ _ARGS_..."]

*spp* _COMMAND_ _ARGS_...

*sp* *--help*

*sp* *--version*

Description
-----------

streampager (*sp*) accepts streamed input on stdin and presents it for browsing
one page at a time.  It can accept input from additional streams for separate
presentation of the output of multiple commands and their error streams.  It
can also display the contents of files.

Positional Arugments
--------------------

_FILE_::
  A file to display instead of the streamed input.

Options
-------

*--force*::
  Start paging immediately, don't wait to see if input is short.

*-c*, *--command* "_COMMAND_ _ARGS_..."::
  Runs the command in a subshell and displays its output and error streams.

*--fd* _FD_[=_TITLE_]::
  Displays the contents of this file descriptor

*--error-fd* _FD_[=_TITLE_]::
  Displays the contents of this file descriptor as the error stream of the previous file or file descriptor.

*--progress-fd* _FD_::
  Displays pages from this file descriptor as progress indicators.

Basic Usage
-----------

If invoked with no arguments, *streampager* reads from stdin, expecting to be
invoked as the final command in a pipeline:

    command | sp

The default paging behaviour depends on how much data is received:

* For programs that produce less than a screenful of output quickly and then
  exit, *streampager* prints the output to the terminal without paging and also exits.
* For programs that produce output slowly, *streampager* will wait for two seconds
  to see if the program will stop with less than a screenful of output.
  After two seconds *streampager* stops waiting and starts paging.

This behaviour can be disabled, forcing *streampager* to always page the input,
with the *--force* option.

An animated indicator the bottom left of the screen indicates if the input pipe
is still connected.

*streampager* can also be used to display files, and efficiently loads large
files by memory mapping them.

Press *h* or *?* from within *streampager* to display the keyboard shortcuts.
Press *q* to exit.

Additional Streams
------------------

*streampager* can page multiple input streams from different file descriptors
on separate screens.  The file descriptors for these additional streams can be
passed in using the *--fd* option.

Error Streams and Progress Indicators
-------------------------------------

Input streams that are the error output for a stream can also be provided using
the *--error-fd* option.  As well as being shown on their own screen, the last
8 lines of an error stream are also shown at the bottom of the screen belonging
to the corresponding main stream.

An additional stream for progress indicators can be provided with the
*--progress-fd* option.  This input stream expects to receive progress updates
(e.g. progress bars) terminated by ASCII form-feed characters (*\f* or *\x0C*).
*streampager* will display the most recently received progress indicator
at the bottom of the screen.

Progress indicator pages should not contain control codes that are used for
moving the cursor or clearing parts of the display.  Control codes that affect
the color or style of output are accepted and passed through to the terminal.

Calling processes that are using *streampager* to page their own output can
also provide the file descriptor for these streams by setting the
_PAGER_ERROR_FD_ and _PAGER_PROGRESS_FD_ environment variables.

Invoking Commands
-----------------

The *-c* option causes *streampager* to invoke the specified command, and capture
its standard output and standard error streams as separate streams.

For example:

    sp -c "grep -r foo /path"

will run *grep*, and page its output.  Errors from *grep* will be paged
separately from the main output.

The *-c* option can be specified multiple times to run multiple commands
and page all of their outputs as separate streams.

If *streampager* is invoked as *spp* then it runs the rest of its command line
arguments as a single command.  For example:

    spp grep -r foo /path

is equivalent to the previous example.

Version
-------
{VERSION}

Homepage
--------
http://github.com/markbt/streampager

Authors
-------
Mark Thomas <markbt@efaref.net>
