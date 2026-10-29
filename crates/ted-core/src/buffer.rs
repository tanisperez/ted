//! [`Buffer`]: the text of one open document, a rope plus what the views need to cache against
//! it (a version counter) and what was detected when it was loaded.

use std::ops::Range;

use ropey::{LineType, Rope, RopeSlice};

use crate::detect::{Indent, LineEnding, SAMPLE_LEN, detect_indent, detect_line_ending};

/// Lines are split on `\n`; a CRLF line keeps its `\r`, which [`Buffer::line`] leaves out.
const LINES: LineType = LineType::LF;

/// A place in the text: a zero based line and a byte offset inside that line.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Default, Hash)]
pub struct Position {
    /// Zero based line index.
    pub line: usize,
    /// Byte offset from the start of the line.
    pub column: usize,
}

impl Position {
    pub fn new(line: usize, column: usize) -> Self {
        Self { line, column }
    }
}

/// The text of one document.
///
/// Cloning is cheap (the rope shares its chunks), so a snapshot can be handed to a background
/// task while the original keeps being edited.
#[derive(Debug, Clone)]
pub struct Buffer {
    rope: Rope,
    /// Bumped by every change, so views cache derived data (shaped lines, highlights) under it.
    version: u64,
    line_ending: LineEnding,
    indent: Option<Indent>,
}

impl Default for Buffer {
    fn default() -> Self {
        Self::from_rope(Rope::new())
    }
}

impl Buffer {
    /// A buffer holding `text`.
    pub fn from_text(text: &str) -> Self {
        Self::from_rope(Rope::from_str(text))
    }

    /// A buffer holding `rope`, for a rope built elsewhere (a background loader). Detects the line
    /// ending and the indentation from the start of the text.
    pub fn from_rope(rope: Rope) -> Self {
        let sample = sample(&rope);
        Self {
            line_ending: detect_line_ending(&sample),
            indent: detect_indent(&sample),
            rope,
            version: 0,
        }
    }

    /// The rope itself, for read only work (search, highlighting, saving).
    pub fn rope(&self) -> &Rope {
        &self.rope
    }

    /// Changes on every edit. Two equal versions of the same buffer have the same text.
    pub fn version(&self) -> u64 {
        self.version
    }

    /// The line ending detected at load, used for new lines.
    pub fn line_ending(&self) -> LineEnding {
        self.line_ending
    }

    /// The indentation detected at load, `None` when the text has no indented line.
    pub fn indent(&self) -> Option<Indent> {
        self.indent
    }

    /// Length of the text in bytes.
    pub fn len(&self) -> usize {
        self.rope.len()
    }

    pub fn is_empty(&self) -> bool {
        self.rope.len() == 0
    }

    /// Number of lines. An empty text has one line, and a text ending in a line break has an
    /// empty last line after it, as editors show it.
    pub fn line_count(&self) -> usize {
        self.rope.len_lines(LINES)
    }

    /// Line `index` without its line break (`\n` or `\r\n`). Panics past [`Self::line_count`].
    pub fn line(&self, index: usize) -> RopeSlice<'_> {
        let line = self.rope.line(index, LINES);
        let mut end = line.len();
        if end > 0 && line.byte(end - 1) == b'\n' {
            end -= 1;
            if end > 0 && line.byte(end - 1) == b'\r' {
                end -= 1;
            }
        }
        line.slice(..end)
    }

    /// Length in bytes of line `index`, without its line break.
    pub fn line_len(&self, index: usize) -> usize {
        self.line(index).len()
    }

    /// Byte offset where line `index` starts. `index` may be [`Self::line_count`], which gives the
    /// length of the text.
    pub fn line_to_byte(&self, index: usize) -> usize {
        self.rope.line_to_byte_idx(index, LINES)
    }

    /// The line that holds byte `offset`.
    pub fn byte_to_line(&self, offset: usize) -> usize {
        self.rope.byte_to_line_idx(offset, LINES)
    }

    /// The position of byte `offset`, clamped to the text.
    pub fn offset_to_position(&self, offset: usize) -> Position {
        let offset = self.rope.floor_char_boundary(offset.min(self.len()));
        let line = self.byte_to_line(offset);
        let column = (offset - self.line_to_byte(line)).min(self.line_len(line));
        Position { line, column }
    }

    /// The byte offset of `position`, clamped to the text and to a char boundary of its line.
    pub fn position_to_offset(&self, position: Position) -> usize {
        let line = position.line.min(self.line_count() - 1);
        let start = self.line_to_byte(line);
        let column = position.column.min(self.line_len(line));
        self.rope.floor_char_boundary(start + column)
    }

    /// The column of `position` in characters, one based, as the status bar shows it
    /// (`Col 5`). Counts UTF-16 code units like most editors, so an emoji counts as two.
    pub fn display_column(&self, position: Position) -> usize {
        let line = self.line(position.line.min(self.line_count() - 1));
        let column = line.floor_char_boundary(position.column.min(line.len()));
        line.byte_to_utf16_idx(column) + 1
    }

    /// Inserts `text` at byte `offset`, which must be a char boundary.
    pub fn insert(&mut self, offset: usize, text: &str) {
        if text.is_empty() {
            return;
        }
        self.rope.insert(offset, text);
        self.version += 1;
    }

    /// Removes the bytes in `range`, whose ends must be char boundaries.
    pub fn remove(&mut self, range: Range<usize>) {
        if range.is_empty() {
            return;
        }
        self.rope.remove(range);
        self.version += 1;
    }

    /// Replaces the bytes in `range` with `text`, as one change.
    pub fn replace(&mut self, range: Range<usize>, text: &str) {
        if range.is_empty() && text.is_empty() {
            return;
        }
        if !range.is_empty() {
            self.rope.remove(range.clone());
        }
        if !text.is_empty() {
            self.rope.insert(range.start, text);
        }
        self.version += 1;
    }

    /// The whole text as a `String`. Allocates the full size, so never call it from a render.
    pub fn to_text(&self) -> String {
        self.rope.to_string()
    }
}

/// The first [`SAMPLE_LEN`] bytes of `rope` (cut at a char boundary), for the detectors.
fn sample(rope: &Rope) -> String {
    let end = rope.floor_char_boundary(rope.len().min(SAMPLE_LEN));
    rope.slice(..end).to_string()
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use super::*;
    use pretty_assertions::assert_eq;

    fn lines(buffer: &Buffer) -> Vec<String> {
        (0..buffer.line_count())
            .map(|index| buffer.line(index).to_string())
            .collect()
    }

    #[test]
    fn an_empty_text_has_one_empty_line() {
        let buffer = Buffer::from_text("");
        assert!(buffer.is_empty());
        assert_eq!(lines(&buffer), vec![""]);
        assert_eq!(buffer.line_ending(), LineEnding::Lf);
        assert_eq!(buffer.indent(), None);
    }

    #[test]
    fn a_trailing_newline_makes_an_empty_last_line() {
        assert_eq!(lines(&Buffer::from_text("a\nb\n")), vec!["a", "b", ""]);
        assert_eq!(lines(&Buffer::from_text("a\nb")), vec!["a", "b"]);
    }

    #[test]
    fn crlf_lines_leave_out_the_carriage_return() {
        let buffer = Buffer::from_text("one\r\ntwo\r\n");
        assert_eq!(lines(&buffer), vec!["one", "two", ""]);
        assert_eq!(buffer.line_ending(), LineEnding::CrLf);
        assert_eq!(buffer.line_len(0), 3);
        assert_eq!(buffer.line_to_byte(1), 5);
    }

    #[test]
    fn a_lone_carriage_return_is_text() {
        assert_eq!(lines(&Buffer::from_text("a\rb\n")), vec!["a\rb", ""]);
    }

    #[test]
    fn positions_and_offsets_round_trip() {
        let buffer = Buffer::from_text("año\nñu\n");
        assert_eq!(buffer.offset_to_position(0), Position::new(0, 0));
        assert_eq!(buffer.offset_to_position(4), Position::new(0, 4));
        assert_eq!(buffer.offset_to_position(5), Position::new(1, 0));
        for offset in [0, 1, 3, 4, 5, 7, 8, 9] {
            let position = buffer.offset_to_position(offset);
            assert_eq!(buffer.position_to_offset(position), offset, "{offset}");
        }
    }

    #[test]
    fn offsets_are_clamped_to_char_boundaries_and_the_text() {
        let buffer = Buffer::from_text("año\nb");
        // Byte 2 is inside the `ñ`.
        assert_eq!(buffer.offset_to_position(2), Position::new(0, 1));
        assert_eq!(buffer.position_to_offset(Position::new(0, 2)), 1);
        assert_eq!(buffer.position_to_offset(Position::new(0, 99)), 4);
        assert_eq!(buffer.position_to_offset(Position::new(9, 0)), 5);
        assert_eq!(buffer.offset_to_position(999), Position::new(1, 1));
    }

    #[test]
    fn display_columns_count_utf16_units_from_one() {
        let buffer = Buffer::from_text("a😀ñb");
        assert_eq!(buffer.display_column(Position::new(0, 0)), 1);
        assert_eq!(buffer.display_column(Position::new(0, 1)), 2);
        assert_eq!(buffer.display_column(Position::new(0, 5)), 4);
        assert_eq!(buffer.display_column(Position::new(0, 7)), 5);
    }

    #[test]
    fn edits_bump_the_version_and_empty_ones_do_not() {
        let mut buffer = Buffer::from_text("hello");
        buffer.insert(5, " world");
        buffer.remove(0..1);
        buffer.replace(0..4, "J");
        assert_eq!(buffer.to_text(), "J world");
        assert_eq!(buffer.version(), 3);
        buffer.insert(0, "");
        buffer.remove(2..2);
        buffer.replace(1..1, "");
        assert_eq!(buffer.version(), 3);
    }

    #[test]
    fn clones_are_independent_snapshots() {
        let mut buffer = Buffer::from_text("abc");
        let snapshot = buffer.clone();
        buffer.insert(3, "d");
        assert_eq!(snapshot.to_text(), "abc");
        assert_eq!(buffer.to_text(), "abcd");
    }

    /// Random edits against a plain `String`: the line view must always agree with `str::split`.
    #[test]
    fn random_edits_match_a_string_model() {
        let mut rng = fastrand::Rng::with_seed(7);
        let pieces = ["a", "ñ", "\n", "\r\n", "😀", "xyz", ""];
        let mut buffer = Buffer::default();
        let mut model = String::new();
        for _ in 0..2_000 {
            let at = floor(&model, rng.usize(..=model.len()));
            if rng.bool() || model.is_empty() {
                let text = pieces[rng.usize(..pieces.len())];
                buffer.insert(at, text);
                model.insert_str(at, text);
            } else {
                let end = floor(&model, rng.usize(at..=model.len()));
                buffer.remove(at..end);
                model.replace_range(at..end, "");
            }
            assert_eq!(buffer.len(), model.len());
        }
        assert_eq!(buffer.to_text(), model);
        let expected: Vec<String> = model
            .split('\n')
            .map(|line| line.strip_suffix('\r').unwrap_or(line).to_string())
            .collect();
        assert_eq!(lines(&buffer), expected);
    }

    fn floor(text: &str, mut index: usize) -> usize {
        while !text.is_char_boundary(index) {
            index -= 1;
        }
        index
    }

    /// Building a big buffer: about 100 MB of log lines. Run with
    /// `cargo test --release -p ted-core -- --ignored --nocapture big_buffer` to see the time.
    #[test]
    #[ignore = "slow in debug, a timing to read by hand in release"]
    fn big_buffer() {
        let line = "2026-10-29T20:41:17.961+01:00 INFO  ted::workspace: opened folder in 12 ms\n";
        let text = line.repeat(100_000_000 / line.len());
        let start = std::time::Instant::now();
        let buffer = Buffer::from_text(&text);
        let built = start.elapsed();
        let start = std::time::Instant::now();
        let middle = buffer.line(buffer.line_count() / 2).len();
        let lookup = start.elapsed();
        println!(
            "{} MB, {} lines: built in {built:?}, middle line ({middle} bytes) in {lookup:?}",
            text.len() / 1_000_000,
            buffer.line_count()
        );
        assert_eq!(middle, line.len() - 1);
    }
}
