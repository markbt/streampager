//! Key bindings.
use std::collections::HashMap;

use termwiz::input::{KeyCode, Modifiers};

/// A key binding category.
///
/// Key bindings are listed by category in the help screen.
#[derive(Copy, Clone, Debug)]
pub enum Category {
    /// Uncategorized actions.
    None,

    /// Actions for controlling the pager.
    General,

    /// Actions for moving around the file.
    Navigation,

    /// Actions that affect the presentation of the file.
    Presentation,

    /// Actions that initiate or modify searches.
    Searching,
}

/// An action that may be bound to a key.
#[derive(Clone, Debug)]
pub enum Binding {
    /// Quit the pager.
    Quit,

    /// Refresh the screen.
    Refresh,

    /// Show the help screen.
    Help,

    /// Cancel the current action.
    Cancel,

    /// Switch to the previous file.
    PreviousFile,

    /// Switch to the next file.
    NextFile,

    /// Scroll up *n* lines.
    ScrollUpLines(usize),

    /// Scroll down *n* lines.
    ScrollDownLines(usize),

    /// Scroll up 1/*n* of the screen height.
    ScrollUpScreenFraction(usize),

    /// Scroll down 1/*n* of the screen height.
    ScrollDownScreenFraction(usize),

    /// Scroll to the top of the file.
    ScrollToTop,

    /// Scroll to the bottom of the file, and start following it.
    ScrollToBottom,

    /// Scroll left *n* columns.
    ScrollLeftColumns(usize),

    /// Scroll right *n* columns.
    ScrollRightColumns(usize),

    /// Scroll left 1/*n* of the screen width.
    ScrollLeftScreenFraction(usize),

    /// Scroll right 1/*n* of the screen width.
    ScrollRightScreenFraction(usize),

    /// Toggle display of line numbers.
    ToggleLineNumbers,

    /// Toggle line wrapping mode.
    ToggleLineWrapping,

    /// Prompt the user for a line to move to.
    PromptGoToLine,

    /// Prompt the user for a search term.  The search will start at the beginning of the file.
    PromptSearchFromStart,

    /// Prompt the user for a search term.  The search will start at the top of the screen.
    PromptSearchForwards,

    /// Prompt the user for a search term.  The search will start from the bottom of the screen and
    /// proceed backwards.
    PromptSearchBackwards,

    /// Move to the previous match.
    PreviousMatch,

    /// Move to the next match.
    NextMatch,

    /// Move the previous line that contains a match.
    PreviousMatchLine,

    /// Move to the next line that contains a match.
    NextMatchLine,

    /// Move to the first match.
    FirstMatch,

    /// Move to the last match.
    LastMatch,

    /// An unrecognised binding.
    Unrecognized(String),
}

impl Binding {
    pub(crate) fn category(&self) -> Category {
        use Binding::*;
        match self {
            Quit | Refresh | Help | Cancel => Category::General,
            PreviousFile
            | NextFile
            | ScrollUpLines(_)
            | ScrollDownLines(_)
            | ScrollUpScreenFraction(_)
            | ScrollDownScreenFraction(_)
            | ScrollToTop
            | ScrollToBottom
            | ScrollLeftColumns(_)
            | ScrollRightColumns(_)
            | ScrollLeftScreenFraction(_)
            | ScrollRightScreenFraction(_)
            | PromptGoToLine => Category::Navigation,
            ToggleLineNumbers | ToggleLineWrapping => Category::Presentation,
            PromptSearchFromStart
            | PromptSearchForwards
            | PromptSearchBackwards
            | NextMatch
            | PreviousMatch
            | NextMatchLine
            | PreviousMatchLine
            | FirstMatch
            | LastMatch => Category::Searching,
            Unrecognized(_) => Category::None,
        }
    }
}

pub(crate) type Keymap = HashMap<(Modifiers, KeyCode), Binding>;
