//! Support for `InterfaceMode::Direct` and other modes using `Direct`.

use crate::config::InterfaceMode;
use crate::event::{Event, EventStream};
use crate::file::File;
use crate::line::Line;
use crate::progress::Progress;
use anyhow::{ensure, Result};
use std::time::{Duration, Instant};
use termwiz::input::InputEvent;
use termwiz::surface::change::Change;
use termwiz::surface::Position;
use termwiz::terminal::Terminal;
use vec_map::VecMap;

/// Return value of `direct`.
pub(crate) enum Outcome {
    /// Content is not completely rendered.
    /// A hint to enter full-screen.
    RenderIncomplete,

    /// Content is not rendered at all.
    /// A hint to enter full-screen.
    RenderNothing,

    /// Content is completely rendered.
    RenderComplete,

    /// The user pressed a key to exit.
    Interrupted,
}

/// Streaming content to the terminal without entering full screen.
///
/// Similar to `tail -f`, but with dynamic progress support.
/// Useful for rendering content before entering the full-screen mode.
///
/// Lines are rendered in this order:
/// - Output (append-only)
/// - Error (append-only)
/// - Progress (mutable)
///
/// Return `Outcome::Interrupted` if `q` or `Ctrl+C` is pressed.
/// Otherwise, return values and conditions are as follows:
///
/// | Interface  | Fits Screen | Streams Ended | Return           |
/// |------------|-------------|---------------|------------------|
/// | FullScreen | (any)       | (any)         | RenderNothing    |
/// | Direct     | (any)       | no            | -                |
/// | Direct     | (any)       | yes           | RenderComplete   |
/// | Hybrid     | yes         | no            | -                |
/// | Hybrid     | yes         | yes           | RenderComplete   |
/// | Hybrid     | no          | (any)         | RenderIncomplete |
/// | Delayed    | (any)       | no (time out) | RenderNothing    |
/// | Delayed    | yes         | yes           | RenderComplete   |
/// | Delayed    | no          | yes           | RenderNothing    |
pub(crate) fn direct<T: Terminal>(
    term: &mut T,
    output_files: &[File],
    error_files: &[File],
    progress: Option<&Progress>,
    events: &mut EventStream,
    mode: InterfaceMode,
) -> Result<Outcome> {
    if mode == InterfaceMode::FullScreen {
        return Ok(Outcome::RenderNothing);
    }
    let delayed_deadline = match mode {
        InterfaceMode::Delayed(duration) => Some(Instant::now() + duration),
        _ => None,
    };

    let mut last_read = VecMap::new(); // file index -> line number last read
    let mut collect_unread = |files: &[File], max_lines: usize| -> Vec<Vec<u8>> {
        let mut result = Vec::new();
        for file in files.iter() {
            let index = file.index();
            let mut lines = file.lines();
            let last = last_read.get(index).cloned().unwrap_or(0);
            file.set_needed_lines(last + max_lines);
            // Ignore the incomplete last line if the file is loading.
            if lines > 0
                && !file.loaded()
                && file
                    .with_line(lines - 1, |l| !l.ends_with(b"\n"))
                    .unwrap_or(true)
            {
                lines -= 1;
            }
            if lines >= last {
                let lines = (last + max_lines).min(lines);
                result.reserve(lines - last);
                for i in last..lines {
                    file.with_line(i, |l| result.push(l.to_vec()));
                }
                last_read.insert(index, lines);
            }
        }
        result
    };

    let read_progress_lines = || -> Vec<Vec<u8>> {
        let line_count = progress.map(|p| p.lines()).unwrap_or(0);
        (0..line_count)
            .filter_map(|i| progress.and_then(|p| p.with_line(i, |l| l.to_vec())))
            .collect::<Vec<_>>()
    };

    let mut state = StreamingLines::default();
    let delayed = delayed_deadline.is_some();
    let has_one_screen_limit = match mode {
        InterfaceMode::Direct => false,
        _ => true,
    };
    let mut render = |term: &mut T, h: usize, w: usize| -> Result<Option<Outcome>> {
        let append_output_lines = collect_unread(output_files, h + 2);
        let append_error_lines = collect_unread(error_files, h + 2);
        let progress_lines = read_progress_lines();
        if delayed {
            state.apply_changes(0, &append_output_lines, &append_error_lines, progress_lines);
            if has_one_screen_limit && state.height() >= h {
                return Ok(Some(Outcome::RenderNothing));
            }
        } else {
            let changes = state.render_changes(
                &append_output_lines,
                &append_error_lines,
                progress_lines,
                w,
            )?;
            if has_one_screen_limit && state.height() >= h {
                return Ok(Some(Outcome::RenderIncomplete));
            }
            term.render(&changes)?;
        }
        Ok(None)
    };

    let mut size = term.get_screen_size()?;
    let mut loaded: VecMap<bool> = VecMap::new();
    let mut remaining = output_files.len() + error_files.len();
    while remaining > 0 {
        match events.get(term, Some(Duration::from_millis(10)))? {
            Some(Event::Loaded(i)) => {
                if loaded.get(i) != Some(&true) {
                    loaded.insert(i, true);
                    remaining -= 1;
                }
            }
            Some(Event::Input(InputEvent::Resized { .. })) => {
                size = term.get_screen_size()?;
            }
            Some(Event::Input(InputEvent::Key(key))) => {
                use termwiz::input::{KeyCode::Char, Modifiers};
                match (key.modifiers, key.key) {
                    (Modifiers::NONE, Char('q')) | (Modifiers::CTRL, Char('C')) => {
                        return Ok(Outcome::Interrupted);
                    }
                    (Modifiers::NONE, Char('f')) | (Modifiers::NONE, Char(' ')) => {
                        let outcome = if delayed {
                            Outcome::RenderNothing
                        } else {
                            Outcome::RenderIncomplete
                        };
                        return Ok(outcome);
                    }
                    _ => (),
                }
            }
            _ => (),
        }
        if let Some(deadline) = delayed_deadline {
            if deadline <= Instant::now() {
                return Ok(Outcome::RenderNothing);
            }
        }
        if let Some(outcome) = render(term, size.rows, size.cols)? {
            return Ok(outcome);
        }
    }

    if delayed {
        term.render(&state.render_all(size.cols)?)?;
    }

    Ok(Outcome::RenderComplete)
}

/// State for calculating how to incrementally render streaming changes.
///
/// +----------------------------+
/// | past output (never redraw) |
/// +----------------------------+
/// | new output (just received) |
/// +----------------------------+
/// | error (always redraw)      |
/// +----------------------------+
/// | progress (always redraw)   |
/// +----------------------------+
#[derive(Default)]
struct StreamingLines {
    past_output_line_count: usize,
    past_output_lines: Vec<Vec<u8>>,
    error_lines: Vec<Vec<u8>>,
    progress_lines: Vec<Vec<u8>>,
}

impl StreamingLines {
    fn apply_changes(
        &mut self,
        past_output_line_count: usize,
        append_output_lines: &[Vec<u8>],
        append_error_lines: &[Vec<u8>],
        replace_progress_lines: Vec<Vec<u8>>,
    ) {
        self.past_output_line_count += past_output_line_count;
        self.past_output_lines
            .extend_from_slice(append_output_lines);
        self.error_lines.extend_from_slice(append_error_lines);
        self.progress_lines = replace_progress_lines;
    }

    fn render_changes(
        &mut self,
        append_output_lines: &[Vec<u8>],
        append_error_lines: &[Vec<u8>],
        replace_progress_lines: Vec<Vec<u8>>,
        terminal_width: usize,
    ) -> Result<Vec<Change>> {
        // Fast path: nothing changed?
        if append_output_lines.is_empty()
            && append_error_lines.is_empty()
            && replace_progress_lines == self.progress_lines
        {
            return Ok(Vec::new());
        }

        let mut changes = Vec::with_capacity(
            // *2: Every line needs at least 2 `Change`s: Text, and CursorPosition.
            // +2: 2 Changes for erasing existing lines.
            (append_output_lines.len() + append_error_lines.len() + replace_progress_lines.len())
                * 2
                + 2,
        );

        // Step 1: Erase progress, and error.
        let erase_line_count = self.progress_lines.len() + self.error_lines.len();
        if erase_line_count > 0 {
            // XXX: This is a workaround to a bug in termwiz. It is only correct
            // with the buggy termwiz! See https://github.com/wez/wezterm/pull/109.
            // let dy = -(erase_line_count as isize);
            let dy = (0xffff_ffff_0000_0000u64 | (erase_line_count as u64)) as isize;
            changes.push(Change::CursorPosition {
                x: Position::NoChange,
                y: Position::Relative(dy),
            });
            changes.push(Change::ClearToEndOfScreen(Default::default()));
        }

        // Step 2: Render new output + error + progress
        for line in append_output_lines
            .iter()
            .chain(self.error_lines.iter())
            .chain(append_error_lines.iter())
            .chain(replace_progress_lines.iter())
        {
            let line = Line::new(0, line);
            line.render(&mut changes, 0, terminal_width, None)?;
            changes.push(Change::CursorPosition {
                x: Position::Absolute(0),
                y: Position::Relative(1),
            });
        }

        // Step 3: Update internal state.
        self.apply_changes(
            append_output_lines.len(),
            &[],
            append_error_lines,
            replace_progress_lines,
        );

        Ok(changes)
    }

    fn render_all(&self, terminal_width: usize) -> Result<Vec<Change>> {
        ensure!(
            self.past_output_line_count == 0,
            "bug: render_all() does not support past_output_line_count > 0"
        );
        let mut changes = Vec::with_capacity(self.height() * 2);
        for line in self
            .past_output_lines
            .iter()
            .chain(self.error_lines.iter())
            .chain(self.progress_lines.iter())
        {
            let line = Line::new(0, line);
            line.render(&mut changes, 0, terminal_width, None)?;
            changes.push(Change::CursorPosition {
                x: Position::Absolute(0),
                y: Position::Relative(1),
            });
        }
        Ok(changes)
    }

    fn height(&self) -> usize {
        self.past_output_line_count
            + self.past_output_lines.len()
            + self.error_lines.len()
            + self.progress_lines.len()
    }
}
