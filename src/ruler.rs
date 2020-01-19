//! The Ruler
use std::cmp::{max, min};
use std::fmt::Write;
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::sync::Arc;
use std::time::Instant;
use termwiz::surface::change::Change;
use unicode_width::UnicodeWidthStr;

use crate::bar::{Bar, BarItem, BarString, BarStyle};
use crate::file::File;
use crate::util;

pub(crate) struct Ruler {
    position: Arc<PositionIndicator>,
    loading: Arc<LoadingIndicator>,
    ruler_bar: Bar,
}

impl Ruler {
    pub(crate) fn new(file: File) -> Self {
        let title = Arc::new(BarString::new(file.title().to_string()));
        let file_info = Arc::new(FileInfo::new(file.clone()));
        let position = Arc::new(PositionIndicator::new(file.clone()));
        let loading = Arc::new(LoadingIndicator::new(file));

        let mut ruler_bar = Bar::new(BarStyle::Normal);
        ruler_bar.add_left_item(title);
        ruler_bar.add_right_item(file_info);
        ruler_bar.add_right_item(position.clone());
        ruler_bar.add_right_item(loading.clone());

        Ruler {
            position,
            loading,
            ruler_bar,
        }
    }

    pub(crate) fn bar(&self) -> &Bar {
        &self.ruler_bar
    }

    pub(crate) fn set_position(&self, top: usize, left: usize, bottom: Option<usize>) {
        self.position.top.store(top, Ordering::SeqCst);
        self.position.left.store(left, Ordering::SeqCst);
        let (bottom, following_end) = match bottom {
            Some(bottom) => (bottom, false),
            None => (0, true),
        };
        self.position.bottom.store(bottom, Ordering::SeqCst);
        self.loading
            .following_end
            .store(following_end, Ordering::SeqCst);
    }
}

/// Shows the file's additional info.
struct FileInfo {
    file: File,
}

impl FileInfo {
    fn new(file: File) -> Self {
        FileInfo { file }
    }
}

impl BarItem for FileInfo {
    fn width(&self) -> usize {
        self.file.info().as_str().width()
    }

    fn render(&self, changes: &mut Vec<Change>, width: usize) {
        changes.push(Change::Text(util::truncate_string(
            self.file.info(),
            0,
            width,
        )));
    }
}

/// Indicates the current position within the file.
struct PositionIndicator {
    file: File,
    top: AtomicUsize,
    left: AtomicUsize,
    bottom: AtomicUsize,
}

impl PositionIndicator {
    pub(crate) fn new(file: File) -> Self {
        PositionIndicator {
            file,
            top: AtomicUsize::new(0),
            left: AtomicUsize::new(0),
            bottom: AtomicUsize::new(0),
        }
    }
}

impl BarItem for PositionIndicator {
    fn width(&self) -> usize {
        let top = self.top.load(Ordering::SeqCst);
        let left = self.left.load(Ordering::SeqCst);
        let bottom = self.bottom.load(Ordering::SeqCst);
        let mut width = 0;
        let file_lines = self.file.lines();
        let nw = max(3, util::number_width(max(file_lines, max(bottom, top + 1))));

        // Indicate horizontal position as "+N" if we are not at the very left.
        if left > 1 {
            width += util::number_width(left + 1) + 3;
        }

        if top > file_lines {
            // We are past end of the file, show as "line NNN/NNN".
            width += 2 * nw + 6;
        } else {
            // We are displaying normally, show as "lines NNN-NNN/NNN".
            width += 3 * nw + 8;
        }

        width
    }

    fn render(&self, changes: &mut Vec<Change>, width: usize) {
        let top = self.top.load(Ordering::SeqCst);
        let left = self.left.load(Ordering::SeqCst);
        let bottom = self.bottom.load(Ordering::SeqCst);
        let file_lines = self.file.lines();
        let mut out = String::new();
        let nw = max(3, util::number_width(max(file_lines, max(bottom, top + 1))));

        if left > 0 {
            write!(out, "{:+}  ", left + 1,).expect("writes to strings should not fail");
        }

        if top > file_lines {
            write!(out, "line {1:0}/{2:0$}", nw, top + 1, file_lines)
        } else if bottom > 0 {
            write!(
                out,
                "lines {1:0$}-{2:0$}/{3:0$.0$}",
                nw,
                top + 1,
                min(bottom, file_lines),
                file_lines,
            )
        } else {
            write!(
                out,
                "lines {1:0$}-{2:0$}/{3:0$.0$}",
                nw,
                top + 1,
                "END",
                file_lines,
            )
        }
        .expect("writes to strings can't fail");

        changes.push(Change::Text(util::truncate_string(&out, 0, width)));
    }
}

/// Shows whether or not the file is loading.
struct LoadingIndicator {
    file: File,
    following_end: AtomicBool,
    animation_start: Instant,
}

impl LoadingIndicator {
    fn new(file: File) -> Self {
        LoadingIndicator {
            file,
            following_end: AtomicBool::new(false),
            animation_start: Instant::now(),
        }
    }

    fn content(&self) -> Option<&'static str> {
        if self.file.loaded() {
            None
        } else if self.file.paused() && !self.following_end.load(Ordering::SeqCst) {
            Some("[loading paused]")
        } else {
            let frame_index = (self.animation_start.elapsed().subsec_millis() / 200) as usize;
            let frame = [
                "[loading •     ]",
                "[loading  •    ]",
                "[loading   •   ]",
                "[loading    •  ]",
                "[loading     • ]",
            ][frame_index];
            Some(frame)
        }
    }
}

impl BarItem for LoadingIndicator {
    fn width(&self) -> usize {
        if self.file.loaded() {
            0
        } else {
            16
        }
    }

    fn render(&self, changes: &mut Vec<Change>, width: usize) {
        if let Some(content) = self.content() {
            changes.push(Change::Text(util::truncate_string(content, 0, width)));
        }
    }
}
