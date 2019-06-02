//! Prompts for input.
use failure::Error;
use termwiz::cell::CellAttributes;
use termwiz::color::{AnsiColor, ColorAttribute};
use termwiz::input::KeyEvent;
use termwiz::surface::change::Change;
use termwiz::surface::Position;
use unicode_width::{UnicodeWidthChar, UnicodeWidthStr};

use crate::display::Action;
use crate::screen::Screen;

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
    run: Option<Box<FnMut(&mut Screen, &str) -> Result<Option<Action>, Error>>>,
}

impl Prompt {
    /// Create a new prompt.
    pub(crate) fn new(
        prompt: &str,
        run: Box<FnMut(&mut Screen, &str) -> Result<Option<Action>, Error>>,
    ) -> Prompt {
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
        match (key.modifiers, key.key) {
            (Modifiers::NONE, Enter) => {
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
            (Modifiers::NONE, Escape) => {
                return Ok(Some(Action::Run(Box::new(|screen: &mut Screen| {
                    screen.clear_prompt();
                    Ok(Some(Action::Render))
                }))));
            }
            (Modifiers::NONE, Char(c)) => {
                self.value.insert(self.position, c);
                self.position += 1;
                if self.position == self.value.len() && self.cursor_position() < width - 5 {
                    return Ok(Some(Action::Change(Change::Text(c.to_string()))));
                } else {
                    self.clamp_offset(width);
                    return Ok(Some(Action::RefreshPrompt));
                }
            }
            (Modifiers::NONE, Backspace) => {
                if self.position > 0 {
                    self.value.remove(self.position - 1);
                    self.position -= 1;
                    self.clamp_offset(width);
                    return Ok(Some(Action::RefreshPrompt));
                }
            }
            (Modifiers::NONE, Delete) => {
                if self.position < self.value.len() {
                    self.value.remove(self.position);
                    self.clamp_offset(width);
                    return Ok(Some(Action::RefreshPrompt));
                }
            }
            (Modifiers::NONE, RightArrow) => {
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
            (Modifiers::NONE, LeftArrow) => {
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
            (Modifiers::NONE, End) | (Modifiers::CTRL, Char('E')) => {
                self.position = self.value.len();
                self.clamp_offset(width);
                return Ok(Some(Action::RefreshPrompt));
            }
            (Modifiers::NONE, Home) | (Modifiers::CTRL, Char('A')) => {
                self.position = 0;
                self.clamp_offset(width);
                return Ok(Some(Action::RefreshPrompt));
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
