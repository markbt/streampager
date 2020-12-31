//! Commands
//!
//! Commands the user can invoke.

use crate::display::DisplayAction;
use crate::error::Error;
use crate::event::EventSender;
use crate::file::FileInfo;
use crate::prompt::Prompt;
use crate::screen::Screen;
use crate::search::{MatchMotion, Search, SearchKind};

/// Go to a line (Shortcut: ':')
///
/// Prompts the user for a line number or percentage within the file and jumps
/// to that position.  Negative numbers can be used to refer to locations
/// relative to the end of the file.
pub(crate) fn goto() -> Prompt {
    Prompt::new(
        "goto",
        "Go to line:",
        Box::new(
            |screen: &mut Screen, value: &str| -> Result<Option<DisplayAction>, Error> {
                match value {
                    // Let vi users quit with `:q` muscle memory.
                    "q" => return Ok(Some(DisplayAction::Quit)),
                    "" => return Ok(Some(DisplayAction::Render)),
                    _ => {}
                }
                let lines = screen.file.lines() as isize;
                if value.ends_with('%') {
                    // Percentage
                    match str::parse::<isize>(&value[..value.len() - 1]) {
                        Ok(value_percent) => {
                            let value_percent = if value_percent <= -100 {
                                0
                            } else if value_percent > 100 {
                                100
                            } else if value_percent < 0 {
                                100 + value_percent
                            } else {
                                value_percent
                            };
                            let value = value_percent * (lines - 1) / 100;
                            screen.scroll_to(value as usize);
                        }
                        Err(e) => {
                            screen.error = Some(e.to_string());
                        }
                    }
                } else {
                    // Absolute
                    match str::parse::<isize>(value) {
                        Ok(value) => {
                            let value = if value < -lines || value == 0 {
                                0
                            } else if value > lines {
                                lines - 1
                            } else if value < 0 {
                                lines + value - 1
                            } else {
                                value - 1
                            };
                            screen.scroll_to(value as usize);
                        }
                        Err(e) => {
                            screen.error = Some(e.to_string());
                        }
                    }
                }
                Ok(Some(DisplayAction::Render))
            },
        ),
    )
}

/// Search for text (Shortcuts: '/', '<', '>')
///
/// Prompts the user for text to search.
pub(crate) fn search(kind: SearchKind, event_sender: EventSender) -> Prompt {
    Prompt::new(
        "search",
        "Search:",
        Box::new(
            move |screen: &mut Screen, value: &str| -> Result<Option<DisplayAction>, Error> {
                screen.refresh_matched_lines();
                if value.is_empty() {
                    match kind {
                        SearchKind::First | SearchKind::FirstAfter(_) => {
                            screen.move_match(MatchMotion::NextLine)
                        }
                        SearchKind::FirstBefore(_) => screen.move_match(MatchMotion::PreviousLine),
                    }
                } else {
                    screen.set_search(
                        Search::new(&screen.file, value, kind, event_sender.clone()).ok(),
                    );
                }
                Ok(Some(DisplayAction::Render))
            },
        ),
    )
}
