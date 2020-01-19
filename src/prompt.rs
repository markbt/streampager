//! Prompts for input.
use anyhow::Error;
use termwiz::cell::{CellAttributes, AttributeChange};
use termwiz::color::{AnsiColor, ColorAttribute};
use termwiz::input::KeyEvent;
use termwiz::surface::change::Change;
use termwiz::surface::Position;
use unicode_width::{UnicodeWidthChar, UnicodeWidthStr};

use crate::display::Action;
use crate::screen::Screen;
use crate::util;

type PromptRunFn = dyn FnMut(&mut Screen, &str) -> Result<Option<Action>, Error>;

/// A prompt for input from the user.
pub(crate) struct Prompt {
    /// The text of the prompt to display to the user.
    prompt: String,

    /// The current prompt state,
    state: PromptState,

    /// The closure to run when the user presses Return.  Will only be called once.
    run: Option<Box<PromptRunFn>>,
}

pub(crate) struct PromptState {
    /// The value the user is typing in.
    value: Vec<char>,

    /// The offset within the value that we are displaying from.
    offset: usize,

    /// The cursor position within the value.
    position: usize,
}

impl PromptState {
    pub(crate) fn new() -> PromptState {
        PromptState {
            value: Vec::new(),
            offset: 0,
            position: 0,
        }
    }

    /// Returns the column for the cursor.
    pub(crate) fn cursor_position(&self) -> usize {
        let mut position = 0;
        for c in self.value[self.offset..self.position].iter() {
            position += render_width(*c);
        }
        position
    }

    /// Clamp the offset to values appropriate for the length of the value and
    /// the current cursor position.  Keeps at least 4 characters visible to the
    /// left and right of the value if possible.
    fn clamp_offset(&mut self, width: usize) {
        if self.offset > self.position {
            self.offset = self.position;
        }
        while self.cursor_position() < 5 && self.offset > 0 {
            self.offset -= 1;
        }
        while self.cursor_position() > width - 5 && self.offset < self.position {
            self.offset += 1;
        }
    }

    /// Renders the prompt onto the terminal.
    fn render(
        &mut self,
        changes: &mut Vec<Change>,
        mut position: usize,
        width: usize,
    ) -> Result<(), Error> {
        let mut start = self.offset;
        let mut end = self.offset;
        while end < self.value.len() {
            let c = self.value[end];
            if let Some(render) = special_render(self.value[end]) {
                if end > start {
                    let value: String = self.value[start..end].iter().collect();
                    changes.push(Change::Text(value));
                }
                let render = util::truncate_string(render, 0, width - position);
                position += render.width();
                changes.push(Change::Attribute(AttributeChange::Reverse(true)));
                changes.push(Change::Text(render));
                changes.push(Change::Attribute(AttributeChange::Reverse(false)));
                start = end + 1;
                // Control characters can't compose, so stop if we hit the end.
                if position >= width {
                    break;
                }
            } else {
                let w = c.width().unwrap_or(0);
                if position + w > width {
                    // This character would take us past the end, so stop.
                    break;
                }
                position += w;
            }
            end += 1;
        }
        if end > start {
            let value: String = self.value[start..end].iter().collect();
            changes.push(Change::Text(value));
        }
        if position < width {
            changes.push(Change::ClearToEndOfLine(ColorAttribute::default()));
        }
        Ok(())
    }

    /// Insert a character at the current position.
    fn insert_char(&mut self, c: char, width: usize) -> Option<Action> {
        self.value.insert(self.position, c);
        self.position += 1;
        if self.position == self.value.len() && self.cursor_position() < width - 5 {
            Some(Action::Change(Change::Text(c.to_string())))
        } else {
            Some(Action::RefreshPrompt)
        }
    }

    fn insert_str(&mut self, s: &str) -> Option<Action> {
        let old_len = self.value.len();
        self.value.splice(self.position..self.position, s.chars());
        self.position += self.value.len() - old_len;
        Some(Action::RefreshPrompt)
    }

    /// Delete previous character.
    fn delete_prev_char(&mut self) -> Option<Action> {
        if self.position > 0 {
            self.value.remove(self.position - 1);
            self.position -= 1;
            Some(Action::RefreshPrompt)
        } else {
            None
        }
    }

    /// Delete next character.
    fn delete_next_char(&mut self) -> Option<Action> {
        if self.position < self.value.len() {
            self.value.remove(self.position);
            Some(Action::RefreshPrompt)
        } else {
            None
        }
    }

    /// Delete previous word.
    fn delete_prev_word(&mut self) -> Option<Action> {
        let dest = move_word_backwards(&self.value, self.position);
        if dest != self.position {
            self.value.splice(dest..self.position, None);
            self.position = dest;
            Some(Action::RefreshPrompt)
        } else {
            None
        }
    }

    /// Delete next word.
    fn delete_next_word(&mut self) -> Option<Action> {
        let dest = move_word_forwards(&self.value, self.position);
        if dest != self.position {
            self.value.splice(self.position..dest, None);
            Some(Action::RefreshPrompt)
        } else {
            None
        }
    }

    /// Move right one character.
    fn move_next_char(&mut self) -> Option<Action> {
        if self.position < self.value.len() {
            self.position += 1;
            while self.position < self.value.len() {
                let w = render_width(self.value[self.position]);
                if w != 0 {
                    break;
                }
                self.position += 1;
            }
            Some(Action::RefreshPrompt)
        } else {
            None
        }
    }

    /// Move left one character.
    fn move_prev_char(&mut self) -> Option<Action> {
        if self.position > 0 {
            while self.position > 0 {
                self.position -= 1;
                let w = render_width(self.value[self.position]);
                if w != 0 {
                    break;
                }
            }
            Some(Action::RefreshPrompt)
        } else {
            None
        }
    }

    /// Move right one word.
    fn move_next_word(&mut self) -> Option<Action> {
        let dest = move_word_forwards(&self.value, self.position);
        if dest != self.position {
            self.position = dest;
            Some(Action::RefreshPrompt)
        } else {
            None
        }
    }

    /// Move left one word.
    fn move_prev_word(&mut self) -> Option<Action> {
        let dest = move_word_backwards(&self.value, self.position);
        if dest != self.position {
            self.position = dest;
            return Some(Action::RefreshPrompt);
        } else {
            None
        }
    }

    /// Delete to end of line.
    fn delete_to_end(&mut self) -> Option<Action> {
        if self.position < self.value.len() {
            self.value.splice(self.position.., None);
            Some(Action::RefreshPrompt)
        } else {
            None
        }
    }

    /// Delete to start of line.
    fn delete_to_start(&mut self) -> Option<Action> {
        if self.position > 0 {
            self.value.splice(..self.position, None);
            self.position = 0;
            Some(Action::RefreshPrompt)
        } else {
            None
        }
    }

    /// Move to end of line.
    fn move_to_end(&mut self) -> Option<Action> {
        self.position = self.value.len();
        Some(Action::RefreshPrompt)
    }

    /// Move to beginning of line.
    fn move_to_start(&mut self) -> Option<Action> {
        self.position = 0;
        Some(Action::RefreshPrompt)
    }

    /// Transpose characters.
    fn transpose_chars(&mut self) -> Option<Action> {
        if self.position > 0 && self.value.len() > 1 {
            if self.position < self.value.len() {
                self.position += 1;
            }
            self.value.swap(self.position - 2, self.position - 1);
            Some(Action::RefreshPrompt)
        } else {
            None
        }
    }
}

impl Prompt {
    /// Create a new prompt.
    pub(crate) fn new(prompt: &str, run: Box<PromptRunFn>) -> Prompt {
        Prompt {
            prompt: prompt.to_string(),
            state: PromptState::new(),
            run: Some(run),
        }
    }

    /// Returns the column for the cursor.
    pub(crate) fn cursor_position(&self) -> usize {
        self.prompt.width() + 4 + self.state.cursor_position()
    }

    /// Renders the prompt onto the terminal.
    pub(crate) fn render(
        &mut self,
        changes: &mut Vec<Change>,
        row: usize,
        width: usize,
    ) -> Result<(), Error> {
        changes.push(Change::CursorPosition {
            x: Position::Absolute(0),
            y: Position::Absolute(row),
        });
        changes.push(Change::AllAttributes(
            CellAttributes::default()
                .set_foreground(AnsiColor::Black)
                .set_background(AnsiColor::Silver)
                .clone(),
        ));
        changes.push(Change::Text(format!("  {} ", self.prompt)));
        changes.push(Change::AllAttributes(CellAttributes::default()));
        changes.push(Change::Text(" ".into()));
        self.state.render(changes, self.prompt.width() + 4, width)?;
        Ok(())
    }

    /// Dispatch a key press to the prompt.
    pub(crate) fn dispatch_key(
        &mut self,
        key: KeyEvent,
        width: usize,
    ) -> Result<Option<Action>, Error> {
        use termwiz::input::{KeyCode::*, Modifiers};
        const CTRL: Modifiers = Modifiers::CTRL;
        const NONE: Modifiers = Modifiers::NONE;
        const ALT: Modifiers = Modifiers::ALT;
        let value_width = width - self.prompt.width() - 4;
        let action = match (key.modifiers, key.key) {
            (NONE, Enter) | (CTRL, Char('J')) | (CTRL, Char('M')) => {
                // Finish.
                let mut run = self.run.take();
                let value: String = self.state.value[..].iter().collect();
                return Ok(Some(Action::Run(Box::new(move |screen: &mut Screen| {
                    screen.clear_prompt();
                    if let Some(ref mut run) = run {
                        run(screen, &value)
                    } else {
                        Ok(Some(Action::Render))
                    }
                }))));
            }
            (NONE, Escape) => {
                // Cancel.
                return Ok(Some(Action::Run(Box::new(|screen: &mut Screen| {
                    screen.clear_prompt();
                    Ok(Some(Action::Render))
                }))));
            }
            (NONE, Char(c)) => self.state.insert_char(c, value_width),
            (NONE, Backspace) | (CTRL, Char('H')) => self.state.delete_prev_char(),
            (NONE, Delete) | (CTRL, Char('D')) => self.state.delete_next_char(),
            (CTRL, Char('W')) | (ALT, Backspace) => self.state.delete_prev_word(),
            (ALT, Char('d')) => self.state.delete_next_word(),
            (NONE, RightArrow) | (CTRL, Char('F')) => self.state.move_next_char(),
            (NONE, LeftArrow) | (CTRL, Char('B')) => self.state.move_prev_char(),
            (CTRL, RightArrow) | (ALT, Char('f')) => self.state.move_next_word(),
            (CTRL, LeftArrow) | (ALT, Char('b')) => self.state.move_prev_word(),
            (CTRL, Char('K')) => self.state.delete_to_end(),
            (CTRL, Char('U')) => self.state.delete_to_start(),
            (NONE, End) | (CTRL, Char('E')) => self.state.move_to_end(),
            (NONE, Home) | (CTRL, Char('A')) => self.state.move_to_start(),
            (CTRL, Char('T')) => self.state.transpose_chars(),
            _ => return Ok(None),
        };
        self.state.clamp_offset(value_width);
        Ok(action)
    }

    /// Paste some text into the prompt.
    pub(crate) fn paste(&mut self, text: &str, width: usize) -> Result<Option<Action>, Error> {
        let value_width = width - self.prompt.width() - 4;
        let action = self.state.insert_str(text);
        self.state.clamp_offset(value_width);
        Ok(action)
    }
}

fn move_word_forwards(value: &Vec<char>, mut position: usize) -> usize {
    let len = value.len();
    while position < len && value[position].is_whitespace() {
        position += 1;
    }
    while position < len && !value[position].is_whitespace() {
        position += 1;
    }
    position
}

fn move_word_backwards(value: &Vec<char>, mut position: usize) -> usize {
    while position > 0 {
        position -= 1;
        if !value[position].is_whitespace() {
            break;
        }
    }
    while position > 0 {
        if value[position].is_whitespace() {
            position += 1;
            break;
        }
        position -= 1;
    }
    position
}

/// Determine the rendering width for a character.
fn render_width(c: char) -> usize {
    if c < ' ' || c == '\x7F' {
        // Render as <XX>
        4
    } else if let Some(w) = c.width() {
        // Render as the character itself
        w
    } else {
        // Render as <U+XXXX>
        8
    }
}

/// Determine the special rendering for a character, if any.
fn special_render(c: char) -> Option<String> {
    if c < ' ' || c == '\x7F' {
        Some(format!("<{:02X}>", c as u8))
    } else if c.width().is_none() {
        Some(format!("<U+{:04X}>", c as u32))
    } else {
        None
    }
}
