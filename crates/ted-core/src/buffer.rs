//! [`Buffer`]: the text of one open document, a rope plus what the views need to cache against
//! it (a version counter), what was detected when it was loaded and, on request, the list of its
//! changes ([`TextChange`]) for an incremental parser.

use std::ops::Range;
use std::sync::atomic::{AtomicU64, Ordering};

use ropey::{LineType, Rope, RopeSlice};

use crate::detect::{Indent, LineEnding, SAMPLE_LEN, detect_indent, detect_line_ending};

/// Lines are split on `\n`; a CRLF line keeps its `\r`, which [`Buffer::line`] leaves out.
const LINES: LineType = LineType::LF;

/// The source of every buffer version, one counter for the whole process: two buffers never
/// share a version, so a file reloaded into a new buffer never looks like its old text.
static NEXT_VERSION: AtomicU64 = AtomicU64::new(1);

fn next_version() -> u64 {
    NEXT_VERSION.fetch_add(1, Ordering::Relaxed)
}

/// Most changes kept between two [`Buffer::take_changes`]. Past it the list is dropped and the
/// next take reports that the changes were lost, so nobody keeps an unbounded list.
const MAX_CHANGES: usize = 4096;

/// One change of the text, in the terms an incremental parser (tree-sitter) needs to reuse the
/// tree of the text before it: byte offsets and `(row, column)` points, columns in bytes.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct TextChange {
    pub start_byte: usize,
    pub old_end_byte: usize,
    pub new_end_byte: usize,
    pub start_position: (usize, usize),
    pub old_end_position: (usize, usize),
    pub new_end_position: (usize, usize),
}

impl TextChange {
    /// The change that replaces `range` of `before` with `new_text` (an insertion has an empty
    /// range, a removal an empty text). Reads only the line index of `before`, so it is cheap.
    pub fn new(before: &Buffer, range: Range<usize>, new_text: &str) -> TextChange {
        let point = |offset: usize| {
            let row = before.byte_to_line(offset);
            (row, offset - before.line_to_byte(row))
        };
        let start_position = point(range.start);
        let old_end_position = point(range.end);
        let new_end_position = match new_text.rfind('\n') {
            None => (start_position.0, start_position.1 + new_text.len()),
            Some(last) => (
                start_position.0 + new_text.bytes().filter(|&b| b == b'\n').count(),
                new_text.len() - (last + 1),
            ),
        };
        TextChange {
            start_byte: range.start,
            old_end_byte: range.end,
            new_end_byte: range.start + new_text.len(),
            start_position,
            old_end_position,
            new_end_position,
        }
    }
}

/// Whether a buffer lists its changes.
#[derive(Debug, Clone, Default)]
enum Changes {
    #[default]
    Off,
    /// The changes since the last take, in order.
    On(Vec<TextChange>),
    /// More than [`MAX_CHANGES`] since the last take: they were dropped.
    Lost,
}

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
    changes: Changes,
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
            version: next_version(),
            changes: Changes::Off,
        }
    }

    /// The rope itself, for read only work (search, highlighting, saving).
    pub fn rope(&self) -> &Rope {
        &self.rope
    }

    /// Changes on every edit. Versions are unique in the process: two buffers with the same
    /// version have the same text.
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
        self.record(offset..offset, text);
        self.rope.insert(offset, text);
        self.version = next_version();
    }

    /// Removes the bytes in `range`, whose ends must be char boundaries.
    pub fn remove(&mut self, range: Range<usize>) {
        if range.is_empty() {
            return;
        }
        self.record(range.clone(), "");
        self.rope.remove(range);
        self.version = next_version();
    }

    /// Replaces the bytes in `range` with `text`, as one change.
    pub fn replace(&mut self, range: Range<usize>, text: &str) {
        if range.is_empty() && text.is_empty() {
            return;
        }
        self.record(range.clone(), text);
        if !range.is_empty() {
            self.rope.remove(range.clone());
        }
        if !text.is_empty() {
            self.rope.insert(range.start, text);
        }
        self.version = next_version();
    }

    /// Starts listing the changes, for [`Self::take_changes`]. Returns whether it was already
    /// on: when it was not, the changes before this call are unknown.
    pub fn track_changes(&mut self) -> bool {
        if matches!(self.changes, Changes::Off) {
            self.changes = Changes::On(Vec::new());
            return false;
        }
        true
    }

    /// The changes since the last take, in the order they happened, and starts a new list.
    /// `None` when they were not tracked or too many piled up: the caller must start over from
    /// the whole text.
    pub fn take_changes(&mut self) -> Option<Vec<TextChange>> {
        match std::mem::take(&mut self.changes) {
            Changes::Off => None,
            Changes::On(changes) => {
                self.changes = Changes::On(Vec::new());
                Some(changes)
            }
            Changes::Lost => {
                self.changes = Changes::On(Vec::new());
                None
            }
        }
    }

    /// Lists the change about to replace `range` with `text`, when tracking.
    fn record(&mut self, range: Range<usize>, text: &str) {
        let Changes::On(changes) = &self.changes else {
            return;
        };
        if changes.len() >= MAX_CHANGES {
            self.changes = Changes::Lost;
            return;
        }
        let change = TextChange::new(self, range, text);
        if let Changes::On(changes) = &mut self.changes {
            changes.push(change);
        }
    }

    /// The text in `range` (byte offsets, clamped to char boundaries).
    pub fn text_in(&self, range: Range<usize>) -> String {
        let start = self.rope.floor_char_boundary(range.start.min(self.len()));
        let end = self
            .rope
            .floor_char_boundary(range.end.min(self.len()))
            .max(start);
        self.rope.slice(start..end).to_string()
    }

    /// The UTF-16 offset of byte `offset`, for the platform text input.
    pub fn byte_to_utf16(&self, offset: usize) -> usize {
        self.rope.byte_to_utf16_idx(offset.min(self.len()))
    }

    /// The byte offset of UTF-16 offset `offset`, clamped to the text.
    pub fn utf16_to_byte(&self, offset: usize) -> usize {
        self.rope
            .utf16_to_byte_idx(offset.min(self.rope.len_utf16()))
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
    fn edits_change_the_version_and_empty_ones_do_not() {
        let mut buffer = Buffer::from_text("hello");
        let mut seen = vec![buffer.version()];
        buffer.insert(5, " world");
        seen.push(buffer.version());
        buffer.remove(0..1);
        seen.push(buffer.version());
        buffer.replace(0..4, "J");
        seen.push(buffer.version());
        assert_eq!(buffer.to_text(), "J world");
        let unique: std::collections::HashSet<_> = seen.iter().collect();
        assert_eq!(unique.len(), 4);
        let version = buffer.version();
        buffer.insert(0, "");
        buffer.remove(2..2);
        buffer.replace(1..1, "");
        assert_eq!(buffer.version(), version);
    }

    #[test]
    fn two_buffers_never_share_a_version() {
        let first = Buffer::from_text("same");
        let second = Buffer::from_text("same");
        assert_ne!(first.version(), second.version());
    }

    #[test]
    fn changes_are_listed_only_when_tracked() {
        let mut buffer = Buffer::from_text("ab\ncd\n");
        buffer.insert(0, "x");
        assert_eq!(buffer.take_changes(), None);

        let _ = buffer.track_changes();
        buffer.replace(5..6, "x\nyz");
        buffer.remove(0..1);
        let changes = buffer.take_changes().unwrap();
        assert_eq!(changes.len(), 2);
        assert_eq!(changes[0].start_position, (1, 1));
        assert_eq!(changes[0].old_end_position, (1, 2));
        assert_eq!(changes[0].new_end_position, (2, 2));
        assert_eq!(changes[0].new_end_byte, 9);
        assert_eq!(changes[1].old_end_byte, 1);
        assert_eq!(buffer.take_changes(), Some(Vec::new()));
    }

    #[test]
    fn too_many_changes_are_reported_as_lost() {
        let mut buffer = Buffer::from_text("");
        let _ = buffer.track_changes();
        for _ in 0..=MAX_CHANGES {
            buffer.insert(0, "a");
        }
        assert_eq!(buffer.take_changes(), None);
        buffer.insert(0, "b");
        assert_eq!(buffer.take_changes().map(|changes| changes.len()), Some(1));
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
