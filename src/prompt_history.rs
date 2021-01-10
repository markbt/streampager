//! Prompt History.

use std::fs::File;
use std::io::{BufRead, BufReader, Write};

use tempfile::NamedTempFile;

use crate::display::DisplayAction;
use crate::error::Error;
use crate::prompt::PromptState;

const HISTORY_LENGTH: usize = 1000;

struct HistoryEntry {
    /// The stored state of the history entry.
    stored: Option<String>,

    /// The active state of the history entry.
    state: Option<PromptState>,
}

impl HistoryEntry {
    fn new() -> Self {
        HistoryEntry {
            stored: None,
            state: Some(PromptState::new()),
        }
    }

    fn load(data: String) -> Self {
        HistoryEntry {
            stored: Some(data),
            state: None,
        }
    }

    fn save(&self) -> Option<String> {
        self.state.as_ref().map(|state| state.save())
    }

    fn activate(&mut self) {
        if self.state.is_none() {
            if let Some(stored) = &self.stored {
                self.state = Some(PromptState::load(stored));
            } else {
                self.state = Some(PromptState::new());
            }
        }
    }

    fn state(&self) -> &PromptState {
        self.state.as_ref().expect("state should exist")
    }

    fn state_mut(&mut self) -> &mut PromptState {
        self.state.as_mut().expect("state should exist")
    }
}

pub(crate) struct PromptHistory {
    ident: String,

    entries: Vec<HistoryEntry>,

    active_index: usize,
}

impl PromptHistory {
    pub(crate) fn open(ident: impl Into<String>) -> Self {
        let ident = ident.into();
        let mut entries = Vec::new();
        if let Some(mut path) = dirs::data_dir() {
            path.push("streampager");
            path.push("history");
            path.push(format!("{}.history", ident));
            if let Ok(file) = File::open(path) {
                let file = BufReader::new(file);
                entries = file
                    .lines()
                    .filter_map(|entry| entry.map(HistoryEntry::load).ok())
                    .collect();
            }
        }
        let active_index = entries.len();
        entries.push(HistoryEntry::new());
        PromptHistory {
            ident,
            entries,
            active_index,
        }
    }

    pub(crate) fn state(&self) -> &PromptState {
        self.entries[self.active_index].state()
    }

    pub(crate) fn state_mut(&mut self) -> &mut PromptState {
        self.entries[self.active_index].state_mut()
    }

    pub(crate) fn previous(&mut self) -> Option<DisplayAction> {
        if self.active_index > 0 {
            self.active_index -= 1;
            self.entries[self.active_index].activate();
            Some(DisplayAction::RefreshPrompt)
        } else {
            None
        }
    }

    pub(crate) fn next(&mut self) -> Option<DisplayAction> {
        if self.active_index < self.entries.len() - 1 {
            self.active_index += 1;
            self.entries[self.active_index].activate();
            Some(DisplayAction::RefreshPrompt)
        } else {
            None
        }
    }

    pub(crate) fn save(&mut self) -> Result<(), Error> {
        if let Some(data) = self.entries[self.active_index].save() {
            if data.is_empty() {
                return Ok(());
            }
            if self.entries.len() > 1 {
                if let Some(previous_data) = &self.entries[self.entries.len() - 2].stored {
                    if data == *previous_data {
                        return Ok(());
                    }
                }
            }
            if let Some(mut path) = dirs::data_dir() {
                path.push("streampager");
                path.push("history");
                std::fs::create_dir_all(&path)?;
                let mut new_file = NamedTempFile::new_in(&path)?;
                path.push(format!("{}.history", &self.ident));
                if let Ok(file) = File::open(&path) {
                    let file = BufReader::new(file);
                    for line in file
                        .lines()
                        .skip(self.entries.len().saturating_sub(HISTORY_LENGTH))
                    {
                        writeln!(new_file, "{}", line?)?;
                    }
                }
                writeln!(new_file, "{}", data)?;
                new_file.persist(&path)?;
            }
        }
        Ok(())
    }
}
