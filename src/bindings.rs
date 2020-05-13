//! Key bindings.
use std::collections::HashMap;

use anyhow::{anyhow, Context, Result};
use indexmap::IndexMap;
use termwiz::input::{KeyCode, Modifiers};

/// A key binding category.
///
/// Key bindings are listed by category in the help screen.
#[derive(Copy, Clone, Debug, PartialEq, Eq)]
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

impl Category {
    pub(crate) fn categories() -> impl Iterator<Item = Category> {
        [
            Category::General,
            Category::Navigation,
            Category::Presentation,
            Category::Searching,
            Category::None,
        ]
        .iter()
        .cloned()
    }
}

impl std::fmt::Display for Category {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match *self {
            Category::None => f.write_str("Other"),
            Category::General => f.write_str("General"),
            Category::Navigation => f.write_str("Navigation"),
            Category::Presentation => f.write_str("Presentation"),
            Category::Searching => f.write_str("Searching"),
        }
    }
}

/// An action that may be bound to a key.
#[derive(Clone, Debug, Hash, PartialEq, Eq)]
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

    /// Parse a keybinding identifier and list of parameters into a key binding.
    pub fn parse(ident: String, params: Vec<String>) -> Result<Self> {
        use Binding::*;

        let param_usize = |index| -> Result<usize> {
            let value: &String = params
                .get(index)
                .ok_or_else(|| anyhow!("{}: missing parameter {}", ident, index))?;
            let value = value
                .parse::<usize>()
                .with_context(|| format!("{}: parameter {}", ident, index))?;
            Ok(value)
        };

        let binding = match ident.as_str() {
            "Quit" => Quit,
            "Refresh" => Refresh,
            "Help" => Help,
            "Cancel" => Cancel,
            "PreviousFile" => PreviousFile,
            "NextFile" => NextFile,
            "ScrollUpLines" => ScrollUpLines(param_usize(0)?),
            "ScrollDownLines" => ScrollDownLines(param_usize(0)?),
            "ScrollUpScreenFraction" => ScrollUpScreenFraction(param_usize(0)?),
            "ScrollDownScreenFraction" => ScrollDownScreenFraction(param_usize(0)?),
            "ScrollToTop" => ScrollToTop,
            "ScrollToBottom" => ScrollToBottom,
            "ScrollLeftColumns" => ScrollLeftColumns(param_usize(0)?),
            "ScrollRightColumns" => ScrollRightColumns(param_usize(0)?),
            "ScrollLeftScreenFraction" => ScrollLeftScreenFraction(param_usize(0)?),
            "ScrollRightScreenFraction" => ScrollRightScreenFraction(param_usize(0)?),
            "ToggleLineNumbers" => ToggleLineNumbers,
            "ToggleLineWrapping" => ToggleLineWrapping,
            "PromptGoToLine" => PromptGoToLine,
            "PromptSearchFromStart" => PromptSearchFromStart,
            "PromptSearchForwards" => PromptSearchForwards,
            "PromptSearchBackwards" => PromptSearchBackwards,
            "PreviousMatch" => PreviousMatch,
            "NextMatch" => NextMatch,
            "PreviousMatchLine" => PreviousMatchLine,
            "NextMatchLine" => NextMatchLine,
            "FirstMatch" => FirstMatch,
            "LastMatch" => LastMatch,
            _ => Unrecognized(ident),
        };

        Ok(binding)
    }
}

impl std::fmt::Display for Binding {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        use Binding::*;
        match *self {
            Quit => write!(f, "Quit"),
            Refresh => write!(f, "Refresh the screen"),
            Help => write!(f, "Show this help"),
            Cancel => write!(f, "Close help or any open prompt"),
            PreviousFile => write!(f, "Switch to the previous file"),
            NextFile => write!(f, "Switch to the next file"),
            ScrollUpLines(1) => write!(f, "Scroll up"),
            ScrollUpLines(n) => write!(f, "Scroll up {} lines", n),
            ScrollDownLines(1) => write!(f, "Scroll down"),
            ScrollDownLines(n) => write!(f, "Scroll down {} lines", n),
            ScrollUpScreenFraction(1) => write!(f, "Scroll up one screen"),
            ScrollUpScreenFraction(n) => write!(f, "Scroll up 1/{} screen", n),
            ScrollDownScreenFraction(1) => write!(f, "Scroll down one screen"),
            ScrollDownScreenFraction(n) => write!(f, "Scroll down 1/{} screen", n),
            ScrollToTop => write!(f, "Move to the start of the file"),
            ScrollToBottom => write!(f, "Move to and follow the end of the file"),
            ScrollLeftColumns(1) => write!(f, "Scroll left"),
            ScrollLeftColumns(n) => write!(f, "Scroll left {} columns", n),
            ScrollRightColumns(1) => write!(f, "Scroll right"),
            ScrollRightColumns(n) => write!(f, "Scroll right {} columns", n),
            ScrollLeftScreenFraction(1) => write!(f, "Scroll left one screen"),
            ScrollLeftScreenFraction(n) => write!(f, "Scroll left 1/{} screen", n),
            ScrollRightScreenFraction(1) => write!(f, "Scroll right one screen"),
            ScrollRightScreenFraction(n) => write!(f, "Scroll right 1/{} screen", n),
            ToggleLineNumbers => write!(f, "Toggle line numbers"),
            ToggleLineWrapping => write!(f, "Cycle through line wrapping modes"),
            PromptGoToLine => write!(f, "Go to position in file"),
            PromptSearchFromStart => write!(f, "Search from the start of the file"),
            PromptSearchForwards => write!(f, "Search forwards"),
            PromptSearchBackwards => write!(f, "Search backwards"),
            PreviousMatch => write!(f, "Move to the previous match"),
            NextMatch => write!(f, "Move to the next match"),
            PreviousMatchLine => write!(f, "Move to the previous matching line"),
            NextMatchLine => write!(f, "Move the the next matching line"),
            FirstMatch => write!(f, "Move to the first match"),
            LastMatch => write!(f, "Move to the last match"),
            Unrecognized(ref s) => write!(f, "Unrecognized binding ({})", s),
        }
    }
}

/// A binding to a key.
#[derive(Clone, Debug)]
pub struct BindingConfig {
    /// The binding.
    pub binding: Binding,

    /// Whether this binding is visible in the help screen.
    pub visible: bool,
}

impl BindingConfig {
    /// Create new binding config.
    pub fn new(binding: Binding, visible: bool) -> Self {
        Self { binding, visible }
    }
}

/// A collection of key bindings.
#[derive(PartialEq, Eq)]
pub struct Keymap {
    /// Map of bindings from keys.
    bindings: HashMap<(Modifiers, KeyCode), Binding>,

    /// Map of visible keys from bindings.
    keys: IndexMap<Binding, Vec<(Modifiers, KeyCode)>>,
}

impl<'a, I: IntoIterator<Item = &'a ((Modifiers, KeyCode), BindingConfig)>> From<I> for Keymap {
    fn from(iter: I) -> Keymap {
        let iter = iter.into_iter();
        let size_hint = iter.size_hint();
        let mut bindings = HashMap::with_capacity(size_hint.0);
        let mut keys = IndexMap::with_capacity(size_hint.0);
        for &((modifiers, keycode), ref binding_config) in iter {
            bindings.insert((modifiers, keycode), binding_config.binding.clone());
            if binding_config.visible {
                keys.entry(binding_config.binding.clone())
                    .or_insert_with(Vec::new)
                    .push((modifiers, keycode));
            }
        }
        Keymap { bindings, keys }
    }
}

impl Keymap {
    /// Create a new, empty, keymap.
    pub fn new() -> Self {
        Keymap {
            bindings: HashMap::new(),
            keys: IndexMap::new(),
        }
    }

    /// Get the binding associated with a key combination.
    pub fn get(&self, modifiers: Modifiers, keycode: KeyCode) -> Option<&Binding> {
        self.bindings.get(&(modifiers, keycode))
    }

    /// Bind (or unbind) a key combination.
    pub fn bind(
        &mut self,
        modifiers: Modifiers,
        keycode: KeyCode,
        binding_config: Option<BindingConfig>,
    ) -> &mut Self {
        if let Some(old_binding) = self.bindings.remove(&(modifiers, keycode)) {
            if let Some(keys) = self.keys.get_mut(&old_binding) {
                keys.retain(|&item| item != (modifiers, keycode));
            }
        }
        if let Some(binding_config) = binding_config {
            self.bindings
                .insert((modifiers, keycode), binding_config.binding.clone());
            if binding_config.visible {
                self.keys
                    .entry(binding_config.binding)
                    .or_insert_with(Vec::new)
                    .push((modifiers, keycode));
            }
        }
        self
    }

    pub(crate) fn iter_keys(&self) -> impl Iterator<Item = (&Binding, &Vec<(Modifiers, KeyCode)>)> {
        self.keys.iter()
    }
}

impl Default for Keymap {
    fn default() -> Self {
        Keymap::from(crate::keymaps::default::KEYMAP.iter())
    }
}

impl std::fmt::Debug for Keymap {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_tuple("Keymap")
            .field(&format!("<{} keys bound>", self.bindings.len()))
            .finish()
    }
}
