//! Track screen refresh regions.
use bit_set::BitSet;
use std::cmp::{max, min};

/// Tracks which parts of the screen need to be refreshed.
#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) enum Refresh {
    /// Nothing to render.
    None,

    /// The range of lines from `start`..`end` must be rendered.
    Range(usize, usize),

    /// The lines in the bitset must be rendered.
    Lines(BitSet),

    /// The whole screen must be rendered.
    All,
}

impl Refresh {
    /// Add a range of lines to the lines that must be rendered.
    pub(crate) fn add_range(&mut self, start: usize, end: usize) {
        match *self {
            Refresh::None => *self = Refresh::Range(start, end),
            Refresh::Range(s, e) => {
                if start > e || s > end {
                    let mut b = BitSet::new();
                    b.extend(s..e);
                    b.extend(start..end);
                    *self = Refresh::Lines(b);
                } else {
                    *self = Refresh::Range(min(start, s), max(end, e));
                }
            }
            Refresh::Lines(ref mut b) => {
                b.extend(start..end);
            }
            Refresh::All => {}
        }
    }

    /// Rotate the range of lines upwards (towards 0).  Lines that roll past
    /// 0 are dropped.
    pub(crate) fn rotate_range_up(&mut self, step: usize) {
        match *self {
            Refresh::None | Refresh::All => {}
            Refresh::Range(s, e) => {
                if step > e {
                    *self = Refresh::None;
                } else {
                    *self = Refresh::Range(s.saturating_sub(step), e - step);
                }
            }
            Refresh::Lines(ref b) => {
                let mut new_b = BitSet::new();
                for line in b.iter() {
                    if line >= step {
                        new_b.insert(line - step);
                    }
                }
                if new_b.is_empty() {
                    *self = Refresh::None;
                } else {
                    *self = Refresh::Lines(new_b)
                }
            }
        }
    }

    /// Rotate the range of lines downwards (away from 0).
    pub(crate) fn rotate_range_down(&mut self, step: usize) {
        match *self {
            Refresh::None | Refresh::All => {}
            Refresh::Range(s, e) => {
                *self = Refresh::Range(s + step, e + step);
            }
            Refresh::Lines(ref b) => {
                let mut new_b = BitSet::new();
                for line in b.iter() {
                    new_b.insert(line + step);
                }
                *self = Refresh::Lines(new_b)
            }
        }
    }
}
