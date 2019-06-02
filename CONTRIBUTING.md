# Contributing to streampager

Thanks for considering donating your time and energy!  All contributions are welcome, no
matter how small.

## Documentation

Everything should be documented with rustdoc annotations.  If anything is
missing, contributions to fill in the gaps are welcome.

Contributions for other kinds of documentation are also welcome.

## Quick Tour

*streampager* is divided up into modules as follows:

* `buffer` provides a safe fillable buffer that is used for storing streams in memory.
* `file` contains representations of loaded streams or files.
* `progress` contains the representation of the progress input stream.
* `overstrike` converts ancient overstrike styling into merely old CSI code sequences.
* `line` contains the representation of a single line in a file, including how to render it.
* `line_cache` defines a cache of lines.
* `screen` contains the definition of a screen that displays a single file.
* `refresh` records which parts of the screen need to be refreshed.
* `command` contains the commands the user can invoke, like *search* and *go to line*.
* `prompt` contains the implementation of the prompt for input.
* `search` contains the implementation of searching for text.
* `display` contains the controller of the display.
* `event` contains the event definitions that control how *streampager* handles input events.
* `app` contains the definition of the command line arguments.
* `main` parses the arguments, sets up the input streams, and starts the display.

## Pull Requests

*streampager* is a project that I work on in my spare time.  While I will try to get round
to reviewing pull requests, please remember that I may not have time.

## License Information

I am providing the code in this repository to you under an open source license.
Because this is my personal repository, the license you receive to my code is
from me and not my employer.
