//! Lines in a file.
use regex::bytes::{NoExpand, Regex};
use smallvec::SmallVec;
use std::borrow::Cow;
use std::cmp::min;
use std::str;
use std::sync::Arc;
use termwiz::cell::{CellAttributes, Intensity};
use termwiz::color::{AnsiColor, ColorAttribute};
use termwiz::escape::csi::{Sgr, CSI};
use termwiz::escape::osc::OperatingSystemCommand;
use termwiz::escape::parser::Parser;
use termwiz::escape::Action;
use termwiz::hyperlink::Hyperlink;
use termwiz::surface::{change::Change, Position};
use unicode_segmentation::UnicodeSegmentation;
use unicode_width::UnicodeWidthStr;

use crate::overstrike;
use crate::search::{trim_trailing_newline, ESCAPE_SEQUENCE};

const LEFT_ARROW: &str = "<";
const RIGHT_ARROW: &str = ">";
const TAB_SPACES: &str = "        ";

/// Represents a single line in a displayed file.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct Line {
    spans: Box<[Span]>,
}

/// Style that is being applied.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum OutputStyle {
    /// The source file's output style.
    File,
    /// Control characters style (inverse video).
    Control,
    /// A search match.
    Match,
    /// The currently selected search match.
    CurrentMatch,
}

/// Tracker of current attributes state.
struct AttributeState {
    /// Current attributes for the file
    attrs: CellAttributes,
    /// Whether the file's attributes have changed
    changed: bool,
    /// What the currently applied style is.
    style: OutputStyle,
}

impl AttributeState {
    /// Create a new color state tracker.
    fn new() -> AttributeState {
        AttributeState {
            attrs: CellAttributes::default(),
            changed: false,
            style: OutputStyle::File,
        }
    }

    /// Apply a sequence of Sgr escape codes onto the attribute state.
    fn apply_sgr_sequence(&mut self, sgr_sequence: &[Sgr]) {
        for sgr in sgr_sequence.iter() {
            match *sgr {
                Sgr::Reset => {
                    // Reset doesn't clear the hyperlink.
                    let hyperlink = self.attrs.hyperlink.take();
                    self.attrs = CellAttributes::default();
                    self.attrs.set_hyperlink(hyperlink);
                }
                Sgr::Intensity(intensity) => {
                    self.attrs.set_intensity(intensity);
                }
                Sgr::Underline(underline) => {
                    self.attrs.set_underline(underline);
                }
                Sgr::Blink(blink) => {
                    self.attrs.set_blink(blink);
                }
                Sgr::Italic(italic) => {
                    self.attrs.set_italic(italic);
                }
                Sgr::Inverse(inverse) => {
                    self.attrs.set_reverse(inverse);
                }
                Sgr::Invisible(invis) => {
                    self.attrs.set_invisible(invis);
                }
                Sgr::StrikeThrough(strike) => {
                    self.attrs.set_strikethrough(strike);
                }
                Sgr::Foreground(color) => {
                    self.attrs.set_foreground(color);
                }
                Sgr::Background(color) => {
                    self.attrs.set_background(color);
                }
                Sgr::Font(_) => {}
            }
        }
        self.changed = true;
    }

    /// Apply a hyperlink escape code onto the attribute state.
    fn apply_hyperlink(&mut self, hyperlink: &Option<Arc<Hyperlink>>) {
        self.attrs.set_hyperlink(hyperlink.clone());
        self.changed = true;
    }

    /// Switch to the given style.  The correct escape color sequences will be emitted.
    fn style(&mut self, style: OutputStyle) -> Result<Option<Change>, std::io::Error> {
        if self.style != style || (style == OutputStyle::File && self.changed) {
            let attrs = match style {
                OutputStyle::File => self.attrs.clone(),
                OutputStyle::Control => CellAttributes::default().set_reverse(true).clone(),
                OutputStyle::Match => CellAttributes::default()
                    .set_foreground(AnsiColor::Black)
                    .set_background(AnsiColor::Olive)
                    .clone(),
                OutputStyle::CurrentMatch => CellAttributes::default()
                    .set_foreground(AnsiColor::Black)
                    .set_background(AnsiColor::Teal)
                    .clone(),
            };
            self.style = style;
            self.changed = false;
            Ok(Some(Change::AllAttributes(attrs)))
        } else {
            Ok(None)
        }
    }
}

/// A span of text within a line.
#[derive(Debug, Clone, PartialEq, Eq)]
enum Span {
    /// Ordinary text.
    Text(String),
    /// Text that matches the current search, and the search match index.
    Match(String, usize),
    /// A control character.
    Control(u8),
    /// An invalid UTF-8 byte.
    Invalid(u8),
    /// An unprintable unicode grapheme cluster.
    Unprintable(String),
    /// A sequence of SGR escape codes.
    SgrSequence(SmallVec<[Sgr; 5]>),
    /// A hyperlink escape code.
    Hyperlink(Option<Arc<Hyperlink>>),
    /// Data that should be ignored.
    Ignore(SmallVec<[u8; 20]>),
    /// A tab control character.
    TAB,
    /// A terminating CRLF sequence.
    CRLF,
    /// A terminating LF sequence.
    LF,
}

/// Produce `Change`s to output some text in the given style at the given
/// position, truncated to the start and end columns.
///
/// Returns the new position after the text has been rendered.
fn write_truncated(
    changes: &mut Vec<Change>,
    attr_state: &mut AttributeState,
    style: OutputStyle,
    text: &str,
    start: usize,
    end: usize,
    position: usize,
) -> Result<usize, std::io::Error> {
    let text_width = text.width();
    if position + text_width > start && position < end {
        if let Some(change) = attr_state.style(style)? {
            changes.push(change);
        }
        if position >= start && position + text_width <= end {
            changes.push(Change::Text(text.into()));
        } else {
            let start = start - min(start, position);
            let end = end - position;
            let mut offset = 0;
            let mut start_index = None;
            let mut end_index = None;
            let mut start_gap = 0;
            let mut end_gap = 0;
            for (i, g) in text.grapheme_indices(true) {
                let w = g.width();
                if w != 0 {
                    if offset >= start && start_index.is_none() {
                        start_index = Some(i);
                        start_gap = offset - start;
                    }
                    if offset + w > end && end_index.is_none() {
                        end_index = Some(i);
                        end_gap = end - offset;
                        break;
                    }
                    offset += w;
                }
            }
            let start_index = start_index.unwrap_or_else(|| text.len());
            let end_index = end_index.unwrap_or_else(|| text.len());
            changes.push(Change::Text(format!(
                "{0:1$.1$}{3}{0:2$.2$}",
                "",
                start_gap,
                end_gap,
                &text[start_index..end_index]
            )));
        }
    }
    Ok(position + text_width)
}

impl Span {
    /// Render the span at the given position in the terminal.
    fn render(
        &self,
        changes: &mut Vec<Change>,
        attr_state: &mut AttributeState,
        start: usize,
        end: usize,
        mut position: usize,
        search_index: Option<usize>,
    ) -> Result<usize, std::io::Error> {
        match *self {
            Span::Text(ref t) => {
                position = write_truncated(
                    changes,
                    attr_state,
                    OutputStyle::File,
                    t,
                    start,
                    end,
                    position,
                )?;
            }
            Span::Match(ref t, ref match_index) => {
                let style = if search_index == Some(*match_index) {
                    OutputStyle::CurrentMatch
                } else {
                    OutputStyle::Match
                };
                position = write_truncated(changes, attr_state, style, t, start, end, position)?;
            }
            Span::TAB => {
                let tabchars = 8 - position % 8;
                position = write_truncated(
                    changes,
                    attr_state,
                    OutputStyle::File,
                    &TAB_SPACES[..tabchars],
                    start,
                    end,
                    position,
                )?;
            }
            Span::Control(c) | Span::Invalid(c) => {
                position = write_truncated(
                    changes,
                    attr_state,
                    OutputStyle::Control,
                    &format!("<{:02X}>", c),
                    start,
                    end,
                    position,
                )?;
            }
            Span::Unprintable(ref grapheme) => {
                for c in grapheme.chars() {
                    position = write_truncated(
                        changes,
                        attr_state,
                        OutputStyle::Control,
                        &format!("<U+{:04X}>", c as u32),
                        start,
                        end,
                        position,
                    )?;
                }
            }
            Span::SgrSequence(ref s) => attr_state.apply_sgr_sequence(s),
            Span::Hyperlink(ref l) => attr_state.apply_hyperlink(l),
            _ => {}
        }
        Ok(position)
    }

    /// Return the width of the given span in characters, taking into account
    /// Unicode widths and the expansion that results in displaying control
    /// characters in hex.
    fn width(&self) -> usize {
        match *self {
            Span::Text(ref t) => t.width(),
            Span::Control(_) | Span::Invalid(_) => 4,
            Span::Unprintable(_) => 8,
            _ => 0,
        }
    }
}

/// Parse data into an array of Spans.
fn parse_spans(data: &[u8], match_index: Option<usize>) -> Vec<Span> {
    let mut spans = Vec::new();
    let mut input = &data[..];

    fn parse_unicode_span(data: &str, spans: &mut Vec<Span>, match_index: Option<usize>) {
        let mut text_start = None;
        let mut skip_to = None;
        for (index, grapheme) in data.grapheme_indices(true) {
            let mut span = None;

            // Skip past any escape sequence we've already extracted
            if let Some(end) = skip_to {
                if index < end {
                    continue;
                } else {
                    skip_to = None;
                }
            }

            if grapheme == "\x1B" {
                // Look ahead for an escape sequence
                let mut parser = Parser::new();
                let bytes = data.as_bytes();
                if let Some((actions, len)) = parser.parse_first_as_vec(&bytes[index..]) {
                    // Look at the sequence of actions this parsed to.  We
                    // assume this is one of:
                    //   - A sequence of SGR actions parse from a single SGR
                    //     sequence.
                    //   - A single Cursor or Edit action we want to ignore.
                    //   - A single OSC that contains a hyperlink.
                    //   - Something else that we don't want to parse.
                    let mut actions = actions.into_iter();
                    match actions.next() {
                        Some(Action::CSI(CSI::Sgr(sgr))) => {
                            // Collect all Sgr values
                            let mut sgr_sequence = SmallVec::new();
                            sgr_sequence.push(sgr);
                            for action in actions {
                                if let Action::CSI(CSI::Sgr(sgr)) = action {
                                    sgr_sequence.push(sgr);
                                }
                            }
                            span = Some(Span::SgrSequence(sgr_sequence));
                            skip_to = Some(index + len);
                        }
                        Some(Action::CSI(CSI::Cursor(_))) | Some(Action::CSI(CSI::Edit(_))) => {
                            span = Some(Span::Ignore(SmallVec::from_slice(
                                &bytes[index..index + len],
                            )));
                            skip_to = Some(index + len);
                        }
                        Some(Action::OperatingSystemCommand(osc)) => {
                            if let OperatingSystemCommand::SetHyperlink(hyperlink) = *osc {
                                span = Some(Span::Hyperlink(hyperlink.map(Arc::new)));
                                skip_to = Some(index + len);
                            }
                        }
                        _ => {}
                    }
                }
            }

            if grapheme == "\r\n" {
                span = Some(Span::CRLF);
                skip_to = Some(index + 2);
            }

            if grapheme == "\n" {
                span = Some(Span::LF);
            }

            if grapheme == "\t" {
                span = Some(Span::TAB);
            }

            if span.is_none() && grapheme.len() == 1 {
                if let Some(ch) = grapheme.bytes().next() {
                    if ch < b' ' || ch == b'\x7F' {
                        span = Some(Span::Control(ch));
                    }
                }
            }

            if span.is_none() && grapheme.width() == 0 {
                span = Some(Span::Unprintable(grapheme.to_string()));
            }

            if let Some(span) = span {
                if let Some(start) = text_start {
                    if let Some(match_index) = match_index {
                        spans.push(Span::Match(data[start..index].to_string(), match_index));
                    } else {
                        spans.push(Span::Text(data[start..index].to_string()));
                    }
                    text_start = None;
                }
                spans.push(span);
            } else if text_start.is_none() {
                text_start = Some(index);
            }
        }
        if let Some(start) = text_start {
            if let Some(match_index) = match_index {
                spans.push(Span::Match(data[start..].to_string(), match_index));
            } else {
                spans.push(Span::Text(data[start..].to_string()));
            }
        }
    }

    loop {
        match str::from_utf8(input) {
            Ok(valid) => {
                parse_unicode_span(valid, &mut spans, match_index);
                break;
            }
            Err(error) => {
                let (valid, after_valid) = input.split_at(error.valid_up_to());
                if !valid.is_empty() {
                    unsafe {
                        parse_unicode_span(
                            str::from_utf8_unchecked(valid),
                            &mut spans,
                            match_index,
                        );
                    }
                }
                if let Some(len) = error.error_len() {
                    for byte in &after_valid[..len] {
                        spans.push(Span::Invalid(*byte));
                    }
                    input = &after_valid[len..];
                } else {
                    for byte in &after_valid[..] {
                        spans.push(Span::Invalid(*byte));
                    }
                    break;
                }
            }
        }
    }
    spans
}

impl Line {
    pub(crate) fn new(_index: usize, data: impl AsRef<[u8]>) -> Line {
        let data = data.as_ref();
        let data = overstrike::convert_overstrike(&data[..]);
        let spans = parse_spans(&data[..], None).into_boxed_slice();
        Line { spans }
    }

    pub(crate) fn new_search(_index: usize, data: impl AsRef<[u8]>, regex: &Regex) -> Line {
        let data = data.as_ref();
        let data = overstrike::convert_overstrike(&data[..]);
        let len = trim_trailing_newline(&data[..]);
        let mut spans = Vec::new();
        let mut start = 0;
        let (data_without_escapes, convert_offset) = if ESCAPE_SEQUENCE.is_match(&data[..len]) {
            let mut escape_ranges = Vec::new();
            for match_range in ESCAPE_SEQUENCE.find_iter(&data[..len]) {
                escape_ranges.push((match_range.start(), match_range.end()));
            }
            (
                ESCAPE_SEQUENCE.replace_all(&data[..len], NoExpand(b"")),
                Some(move |offset| {
                    let mut original_offset = 0;
                    let mut remaining_offset = offset;
                    for (escape_start, escape_end) in escape_ranges.iter() {
                        if original_offset + remaining_offset < *escape_start {
                            break;
                        } else {
                            remaining_offset -= escape_start - original_offset;
                            original_offset = *escape_end;
                        }
                    }
                    original_offset + remaining_offset
                }),
            )
        } else {
            (Cow::Borrowed(&data[..len]), None)
        };
        for (match_index, match_range) in regex.find_iter(&data_without_escapes[..]).enumerate() {
            let (match_start, match_end) = if let Some(ref convert) = convert_offset {
                (convert(match_range.start()), convert(match_range.end()))
            } else {
                (match_range.start(), match_range.end())
            };
            if start < match_start {
                spans.append(&mut parse_spans(&data[start..match_start], None));
            }
            spans.append(&mut parse_spans(
                &data[match_start..match_end],
                Some(match_index),
            ));
            start = match_end;
        }
        if start < data.len() {
            spans.append(&mut parse_spans(&data[start..], None));
        }
        let spans = spans.into_boxed_slice();
        Line { spans }
    }

    pub(crate) fn width(&self) -> usize {
        self.spans.iter().map(|s| s.width()).sum()
    }

    pub(crate) fn height(&self, width: usize) -> usize {
        (self.width() + width - 1) / width
    }

    /// Produce the `Change`s needed to render a slice of the line on a terminal.
    pub(crate) fn render(
        &self,
        changes: &mut Vec<Change>,
        start: usize,
        end: usize,
        search_index: Option<usize>,
    ) -> Result<(), std::io::Error> {
        let mut start = start;
        let mut attr_state = AttributeState::new();
        let mut position = 0;
        if start > 0 {
            changes.push(Change::AllAttributes(
                CellAttributes::default()
                    .set_foreground(AnsiColor::Navy)
                    .set_intensity(Intensity::Bold)
                    .clone(),
            ));
            changes.push(LEFT_ARROW.into());
            changes.push(Change::AllAttributes(CellAttributes::default()));
            start += 1;
        }
        for span in self.spans.iter() {
            position = span.render(changes, &mut attr_state, start, end, position, search_index)?;
        }
        if position > end {
            // There is more text after the end of the line, so we need to
            // render the right arrow.
            //
            // The cursor should be in the final column of the line.  However,
            // we need to work around strange terminal behaviour when setting
            // styles at the end of the line by backspacing and then moving
            // forwards.
            changes.push(Change::Text("\x08".into()));
            changes.push(Change::CursorPosition {
                x: Position::Relative(1),
                y: Position::NoChange,
            });
            changes.push(Change::AllAttributes(
                CellAttributes::default()
                    .set_foreground(AnsiColor::Navy)
                    .set_intensity(Intensity::Bold)
                    .clone(),
            ));
            changes.push(RIGHT_ARROW.into());
            changes.push(Change::AllAttributes(CellAttributes::default()));
        } else if position == end {
            changes.push(Change::AllAttributes(CellAttributes::default()));
        } else {
            changes.push(Change::ClearToEndOfLine(ColorAttribute::default()));
        }
        Ok(())
    }

    /// Produce changes to render the full line.
    pub(crate) fn render_full(&self, changes: &mut Vec<Change>) -> Result<(), std::io::Error> {
        self.render(changes, 0, !0, None)
    }
}

#[cfg(test)]
mod test {
    use super::Span::*;
    use super::*;
    #[test]
    fn test_parse_spans() {
        assert_eq!(parse_spans(b"hello", None), vec![Text("hello".to_string())]);
        assert_eq!(
            parse_spans("Wíth Únícódé".as_bytes(), None),
            vec![Text("Wíth Únícódé".to_string())]
        );
        assert_eq!(
            parse_spans(b"Truncated\xE0", None),
            vec![Text("Truncated".to_string()), Invalid(224)]
        );
        assert_eq!(
            parse_spans(b"Truncated\xE0\x80", None),
            vec![Text("Truncated".to_string()), Invalid(224), Invalid(128)]
        );
        assert_eq!(
            parse_spans(b"Internal\xE0Error", None),
            vec![
                Text("Internal".to_string()),
                Invalid(224),
                Text("Error".to_string())
            ]
        );
        assert_eq!(
            parse_spans(b"\x84StartingError", None),
            vec![Invalid(132), Text("StartingError".to_string())]
        );
        assert_eq!(
            parse_spans(b"Internal\xE0\x80Error", None),
            vec![
                Text("Internal".to_string()),
                Invalid(224),
                Invalid(128),
                Text("Error".to_string())
            ]
        );
        assert_eq!(
            parse_spans(b"TerminatingControl\x1F", None),
            vec![Text("TerminatingControl".to_string()), Control(31)]
        );
        assert_eq!(
            parse_spans(b"Internal\x02Control", None),
            vec![
                Text("Internal".to_string()),
                Control(2),
                Text("Control".to_string())
            ]
        );
        assert_eq!(
            parse_spans(b"\x1AStartingControl", None),
            vec![Control(26), Text("StartingControl".to_string())]
        );
        assert_eq!(
            parse_spans(b"\x1B[1mBold!\x1B[m", None),
            vec![
                SgrSequence(SmallVec::from_slice(&[27, 91, 49, 109])),
                Text("Bold!".to_string()),
                SgrSequence(SmallVec::from_slice(&[27, 91, 109]))
            ]
        );
        assert_eq!(
            parse_spans(
                b"Multi\x1B[31;7m-colored \x1B[36;1mtext\x1B[42;1m line",
                None
            ),
            vec![
                Text("Multi".to_string()),
                SgrSequence(SmallVec::from_slice(&[27, 91, 51, 49, 59, 55, 109])),
                Text("-colored ".to_string()),
                SgrSequence(SmallVec::from_slice(&[27, 91, 51, 54, 59, 49, 109])),
                Text("text".to_string()),
                SgrSequence(SmallVec::from_slice(&[27, 91, 52, 50, 59, 49, 109])),
                Text(" line".to_string())
            ]
        );
        assert_eq!(
            parse_spans(b"Terminating LF\n", None),
            vec![Text("Terminating LF".to_string()), LF]
        );
        assert_eq!(
            parse_spans(b"Terminating CRLF\r\n", None),
            vec![Text("Terminating CRLF".to_string()), CRLF]
        );

        assert_eq!(
            parse_spans(b"Terminating CR\r", None),
            vec![Text("Terminating CR".to_string()), Control(13)]
        );

        assert_eq!(
            parse_spans(b"Internal\rCR", None),
            vec![
                Text("Internal".to_string()),
                Control(13),
                Text("CR".to_string())
            ]
        );
        assert_eq!(
            parse_spans(b"Internal\nLF", None),
            vec![Text("Internal".to_string()), LF, Text("LF".to_string())]
        );
        assert_eq!(
            parse_spans(b"Internal\r\nCRLF", None),
            vec![Text("Internal".to_string()), CRLF, Text("CRLF".to_string())]
        );
    }
}
