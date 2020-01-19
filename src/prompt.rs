//! Prompts for input.
use anyhow::Error;
use termwiz::cell::CellAttributes;
use termwiz::color::{AnsiColor, ColorAttribute};
use termwiz::input::KeyEvent;
use termwiz::surface::change::Change;
use termwiz::surface::Position;
use unicode_width::{UnicodeWidthChar, UnicodeWidthStr};

use crate::display::Action;
use crate::screen::Screen;

type PromptRunFn = dyn FnMut(&mut Screen, &str) -> Result<Option<Action>, Error>;

/// A prompt for input from the user.
pub(crate) struct Prompt {
    /// The text of the prompt to display to the user.
    prompt: String,

    /// The value the user is typing in.
    value: Vec<char>,

    /// The offset within the value that we are displaying from.
    offset: usize,

    /// The cursor position within the value.
    position: usize,

    /// The closure to run when the user presses Return.  Will only be called once.
    run: Option<Box<PromptRunFn>>,
}

impl Prompt {
    /// Create a new prompt.
    pub(crate) fn new(prompt: &str, run: Box<PromptRunFn>) -> Prompt {
        Prompt {
            prompt: prompt.to_string(),
            value: Vec::new(),
            offset: 0,
            position: 0,
            run: Some(run),
        }
    }

    /// Clamp the offset to values appropriate for the length of the value and
    /// the current cursor position.  Keeps at least 4 characters visible to the
    /// left and right of the value if possible.
    fn clamp_offset(&mut self, width: usize) {
        if self.offset > self.position {
            self.offset = self.position;
        }
        let prompt_width = self.prompt.width() + 4;
        while self.cursor_position() < prompt_width + 5 && self.offset > 0 {
            self.offset -= 1;
        }
        while self.cursor_position() > width - 5 && self.offset < self.position {
            self.offset += 1;
        }
    }

    /// Returns the column for the cursor.
    pub(crate) fn cursor_position(&self) -> usize {
        let mut position = self.prompt.width() + 4;
        for c in self.value[self.offset..self.position].iter() {
            position += c.width().unwrap_or(0);
        }
        position
    }

    /// Renders the prompt onto the terminal.
    pub(crate) fn render(
        &mut self,
        changes: &mut Vec<Change>,
        row: usize,
        width: usize,
    ) -> Result<(), Error> {
        let start = self.offset;
        let mut end = self.offset;
        let mut position = self.prompt.width() + 3;
        while end < self.value.len() {
            position += self.value[end].width().unwrap_or(0);
            if position > width {
                break;
            }
            end += 1;
        }
        let value: String = self.value[start..end].iter().collect();
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
        changes.push(Change::Text(value));
        if position < width {
            changes.push(Change::ClearToEndOfLine(ColorAttribute::default()));
        }
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
        match (key.modifiers, key.key) {
            (NONE, Enter) | (CTRL, Char('J')) | (CTRL, Char('M')) => {
                // Finish.
                let mut run = self.run.take();
                let value: String = self.value[..].iter().collect();
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
            (NONE, Char(c)) => {
                // Insert a character.
                self.value.insert(self.position, c);
                self.position += 1;
                if self.position == self.value.len() && self.cursor_position() < width - 5 {
                    return Ok(Some(Action::Change(Change::Text(c.to_string()))));
                } else {
                    self.clamp_offset(width);
                    return Ok(Some(Action::RefreshPrompt));
                }
            }
            (NONE, Backspace) | (CTRL, Char('H')) => {
                // Delete character to left.
                if self.position > 0 {
                    self.value.remove(self.position - 1);
                    self.position -= 1;
                    self.clamp_offset(width);
                    return Ok(Some(Action::RefreshPrompt));
                }
            }
            (NONE, Delete) | (CTRL, Char('D')) => {
                // Delete character to right.
                if self.position < self.value.len() {
                    self.value.remove(self.position);
                    self.clamp_offset(width);
                    return Ok(Some(Action::RefreshPrompt));
                }
            }
            (CTRL, Char('W')) | (ALT, Backspace) => {
                // Delete previous word.
                let dest = move_word_backwards(&self.value, self.position);
                if dest != self.position {
                    self.value.splice(dest..self.position, None);
                    self.position = dest;
                    self.clamp_offset(width);
                    return Ok(Some(Action::RefreshPrompt));
                }
            }
            (ALT, Char('d')) => {
                // Delete next word.
                let dest = move_word_forwards(&self.value, self.position);
                if dest != self.position {
                    self.value.splice(self.position..dest, None);
                    self.clamp_offset(width);
                    return Ok(Some(Action::RefreshPrompt));
                }
            }
            (NONE, RightArrow) | (CTRL, Char('F')) => {
                // Move right one character.
                if self.position < self.value.len() {
                    self.position += 1;
                    while self.position < self.value.len() {
                        let w = self.value[self.position].width().unwrap_or(0);
                        if w != 0 {
                            break;
                        }
                        self.position += 1;
                    }
                    self.clamp_offset(width);
                    return Ok(Some(Action::RefreshPrompt));
                }
            }
            (NONE, LeftArrow) | (CTRL, Char('B')) => {
                // Move left one character.
                if self.position > 0 {
                    while self.position > 0 {
                        self.position -= 1;
                        let w = self.value[self.position].width().unwrap_or(0);
                        if w != 0 {
                            break;
                        }
                    }
                    self.clamp_offset(width);
                    return Ok(Some(Action::RefreshPrompt));
                }
            }
            (CTRL, RightArrow) | (ALT, Char('f')) => {
                // Move right one word.
                let dest = move_word_forwards(&self.value, self.position);
                if dest != self.position {
                    self.position = dest;
                    self.clamp_offset(width);
                    return Ok(Some(Action::RefreshPrompt));
                }
            }
            (CTRL, LeftArrow) | (ALT, Char('b')) => {
                // Move left one word.
                let dest = move_word_backwards(&self.value, self.position);
                if dest != self.position {
                    self.position = dest;
                    self.clamp_offset(width);
                    return Ok(Some(Action::RefreshPrompt));
                }
            }
            (CTRL, Char('K')) => {
                // Delete to end of line.
                if self.position < self.value.len() {
                    self.value.splice(self.position.., None);
                    self.clamp_offset(width);
                    return Ok(Some(Action::RefreshPrompt));
                }
            }
            (CTRL, Char('U')) => {
                // Delete to start of line.
                if self.position> 0 {
                    self.value.splice(..self.position, None);
                    self.position = 0;
                    self.clamp_offset(width);
                    return Ok(Some(Action::RefreshPrompt));
                }
            }
            (NONE, End) | (CTRL, Char('E')) => {
                // Move to end of line.
                self.position = self.value.len();
                self.clamp_offset(width);
                return Ok(Some(Action::RefreshPrompt));
            }
            (NONE, Home) | (CTRL, Char('A')) => {
                // Move to beginning of line.
                self.position = 0;
                self.clamp_offset(width);
                return Ok(Some(Action::RefreshPrompt));
            }
            (CTRL, Char('T')) => {
                // Transpose characters.
                if self.position > 0 && self.value.len() > 1 {
                    if self.position < self.value.len() {
                        self.position += 1;
                    }
                    self.value.swap(self.position - 2, self.position - 1);
                    self.clamp_offset(width);
                    return Ok(Some(Action::RefreshPrompt));
                }
            }
            _ => {}
        }
        Ok(None)
    }

    /// Paste some text into the prompt.
    pub(crate) fn paste(&mut self, text: &str, width: usize) -> Result<Option<Action>, Error> {
        let old_len = self.value.len();
        self.value
            .splice(self.position..self.position, text.chars());
        self.position += self.value.len() - old_len;
        self.clamp_offset(width);
        Ok(Some(Action::RefreshPrompt))
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
