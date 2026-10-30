//! The state of one editor, outside the gpui element: the buffer, the cursor, the scroll offsets
//! and the viewport math (which lines are visible, how far the document scrolls). Plain Rust, so
//! it is unit tested without a window.
//!
//! The vertical scroll offset is an `f64`: a 100 MB log has more than a million lines, about 27
//! million pixels, and an `f32` loses whole pixels well before that. Everything handed to gpui is
//! relative to the first visible line, so it stays small.

use std::ops::Range;
use std::path::PathBuf;
use std::time::SystemTime;

use ted_core::{Buffer, Edit, EditKind, History, Indent, Position, Selection};

use super::syntax::Syntax;

/// Empty space above the first line, in pixels.
pub const PADDING_TOP: f32 = 10.0;
/// How far the document scrolls past its last line, as a fraction of the viewport height.
pub const SCROLL_PAST_END: f32 = 0.4;
/// Empty space after the longest line, in pixels.
pub const PADDING_RIGHT: f32 = 24.0;
/// Columns a tab advances to the next multiple of. A setting later (#13).
pub const TAB_SIZE: usize = 4;

/// The size of the area the text is painted in and the font metrics, measured by the element on
/// every frame.
#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub struct Viewport {
    /// Width of the text area (without the gutter and the scrollbar), in pixels.
    pub width: f32,
    /// Height of the text area, in pixels.
    pub height: f32,
    /// Height of one line, in pixels.
    pub line_height: f32,
    /// Advance of one character of the editor font, in pixels.
    pub char_width: f32,
}

impl Viewport {
    /// Number of whole lines that fit, at least one.
    pub fn lines_per_page(&self) -> usize {
        if self.line_height <= 0.0 {
            return 1;
        }
        ((self.height / self.line_height).floor() as usize).max(1)
    }
}

/// A cursor movement.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Movement {
    Left,
    Right,
    Up,
    Down,
    LineStart,
    LineEnd,
    PageUp,
    PageDown,
    DocumentStart,
    DocumentEnd,
}

/// One open document in an editor.
#[derive(Debug, Clone, Default)]
pub struct EditorState {
    pub buffer: Buffer,
    /// Where the cursor is.
    pub cursor: Position,
    /// Where the selection started, when there is one. The selection runs from here to the
    /// cursor.
    pub anchor: Option<Position>,
    /// Undo and redo.
    pub history: History,
    /// The display column vertical movements try to keep, set by horizontal ones.
    preferred_column: Option<usize>,
    /// Vertical scroll offset, in pixels from the top of the document.
    pub scroll_y: f64,
    /// Horizontal scroll offset, in pixels.
    pub scroll_x: f32,
    /// The longest line seen on screen so far, in display columns. The horizontal scroll range
    /// grows with it instead of scanning every line of the document.
    pub widest_seen: usize,
    /// The last viewport the element measured.
    pub viewport: Viewport,
    /// While only the start of a big file is loaded: its estimated line count, so the gutter
    /// already has its final width and does not jump when the whole text arrives.
    pub expected_lines: Option<usize>,
    /// The file started with a UTF-8 BOM, which saving restores.
    pub bom: bool,
    /// The file was not valid UTF-8 and was decoded lossily, or only its start is loaded yet:
    /// saving would change its bytes.
    pub read_only: bool,
    /// The file the text comes from.
    pub path: Option<PathBuf>,
    /// The file's modification time when it was loaded or saved, to notice other programs'
    /// changes.
    pub disk_modified: Option<SystemTime>,
    /// The language and the syntax tree of the text.
    pub syntax: Syntax,
}

impl EditorState {
    pub fn new(buffer: Buffer) -> Self {
        Self {
            buffer,
            ..Self::default()
        }
    }

    /// The number of lines the gutter makes room for.
    pub fn gutter_lines(&self) -> usize {
        self.buffer
            .line_count()
            .max(self.expected_lines.unwrap_or(0))
    }

    /// Height of the whole document, padding and scroll past the end included.
    pub fn content_height(&self) -> f64 {
        let viewport = &self.viewport;
        f64::from(PADDING_TOP)
            + self.buffer.line_count() as f64 * f64::from(viewport.line_height)
            + f64::from(viewport.height * SCROLL_PAST_END)
    }

    /// The largest vertical scroll offset.
    pub fn max_scroll_y(&self) -> f64 {
        (self.content_height() - f64::from(self.viewport.height)).max(0.0)
    }

    /// The largest horizontal scroll offset, from the widest line seen so far.
    pub fn max_scroll_x(&self) -> f32 {
        let viewport = &self.viewport;
        let content = self.widest_seen as f32 * viewport.char_width + PADDING_RIGHT;
        (content - viewport.width).max(0.0)
    }

    /// Sets the viewport and keeps the scroll offsets in range.
    pub fn set_viewport(&mut self, viewport: Viewport) {
        self.viewport = viewport;
        self.clamp_scroll();
    }

    /// Scrolls by a delta in pixels, positive down and right.
    pub fn scroll_by(&mut self, dx: f32, dy: f32) {
        self.scroll_x += dx;
        self.scroll_y += f64::from(dy);
        self.clamp_scroll();
    }

    /// Scrolls so that `fraction` (0 to 1) of the scroll range is above the viewport, for the
    /// scrollbar.
    pub fn scroll_to_fraction(&mut self, fraction: f64) {
        self.scroll_y = fraction.clamp(0.0, 1.0) * self.max_scroll_y();
    }

    fn clamp_scroll(&mut self) {
        self.scroll_y = self.scroll_y.clamp(0.0, self.max_scroll_y());
        self.scroll_x = self.scroll_x.clamp(0.0, self.max_scroll_x());
    }

    /// The lines to paint (partly visible ones included) and the y of the first one relative to
    /// the top of the viewport (zero or negative).
    pub fn visible_lines(&self) -> (Range<usize>, f32) {
        let line_height = f64::from(self.viewport.line_height);
        if line_height <= 0.0 {
            return (0..0, 0.0);
        }
        let top = self.scroll_y - f64::from(PADDING_TOP);
        let first = (top / line_height).floor().max(0.0) as usize;
        let first = first.min(self.buffer.line_count());
        let offset = (first as f64 * line_height - top) as f32;
        let count = (f64::from(self.viewport.height) / line_height).ceil() as usize + 1;
        let end = (first + count).min(self.buffer.line_count());
        (first..end, offset)
    }

    /// The line under a y coordinate relative to the top of the viewport, clamped to the
    /// document.
    pub fn line_at_y(&self, y: f32) -> usize {
        let line_height = f64::from(self.viewport.line_height).max(1.0);
        let document_y = self.scroll_y + f64::from(y) - f64::from(PADDING_TOP);
        let line = (document_y / line_height).floor().max(0.0) as usize;
        line.min(self.buffer.line_count() - 1)
    }

    /// Moves the cursor and scrolls it into view. With `select`, the selection extends to the new
    /// place; without it, a selection collapses (to its start or end for Left and Right).
    pub fn move_cursor(&mut self, movement: Movement, select: bool) {
        self.history.break_group();
        if select {
            self.anchor.get_or_insert(self.cursor);
        } else if let Some(anchor) = self.anchor.take()
            && anchor != self.cursor
            && matches!(movement, Movement::Left | Movement::Right)
        {
            let (start, end) = (anchor.min(self.cursor), anchor.max(self.cursor));
            self.cursor = if movement == Movement::Left {
                start
            } else {
                end
            };
            self.preferred_column = None;
            self.scroll_to_cursor();
            return;
        }
        let buffer = &self.buffer;
        let cursor = self.cursor;
        let page = self.viewport.lines_per_page();
        let vertical = |line: usize, this: &Self| {
            let column = this.preferred_column.unwrap_or_else(|| {
                display_column(&buffer.line(cursor.line).to_string(), cursor.column)
            });
            Position::new(
                line,
                byte_for_display_column(&buffer.line(line).to_string(), column),
            )
        };
        let last_line = buffer.line_count() - 1;
        let (position, keeps_column) = match movement {
            Movement::Left => {
                if cursor.column > 0 {
                    let line = buffer.line(cursor.line);
                    let column = line.floor_char_boundary(cursor.column - 1);
                    (Position::new(cursor.line, column), false)
                } else if cursor.line > 0 {
                    let line = cursor.line - 1;
                    (Position::new(line, buffer.line_len(line)), false)
                } else {
                    (cursor, false)
                }
            }
            Movement::Right => {
                let len = buffer.line_len(cursor.line);
                if cursor.column < len {
                    let line = buffer.line(cursor.line);
                    let column = line.ceil_char_boundary(cursor.column + 1);
                    (Position::new(cursor.line, column), false)
                } else if cursor.line < last_line {
                    (Position::new(cursor.line + 1, 0), false)
                } else {
                    (cursor, false)
                }
            }
            Movement::Up if cursor.line == 0 => (Position::new(0, 0), false),
            Movement::Up => (vertical(cursor.line - 1, self), true),
            Movement::Down if cursor.line == last_line => {
                (Position::new(last_line, buffer.line_len(last_line)), false)
            }
            Movement::Down => (vertical(cursor.line + 1, self), true),
            Movement::PageUp => (vertical(cursor.line.saturating_sub(page), self), true),
            Movement::PageDown => (vertical((cursor.line + page).min(last_line), self), true),
            Movement::LineStart => (Position::new(cursor.line, 0), false),
            Movement::LineEnd => (
                Position::new(cursor.line, buffer.line_len(cursor.line)),
                false,
            ),
            Movement::DocumentStart => (Position::new(0, 0), false),
            Movement::DocumentEnd => (Position::new(last_line, buffer.line_len(last_line)), false),
        };
        if keeps_column {
            if self.preferred_column.is_none() {
                let line = self.buffer.line(cursor.line).to_string();
                self.preferred_column = Some(display_column(&line, cursor.column));
            }
        } else {
            self.preferred_column = None;
        }
        // A page movement also scrolls by a page, so the cursor keeps its place on screen.
        let scroll_lines = match movement {
            Movement::PageUp => -(page as f64),
            Movement::PageDown => page as f64,
            _ => 0.0,
        };
        self.scroll_y += scroll_lines * f64::from(self.viewport.line_height);
        self.clamp_scroll();
        self.cursor = position;
        if self.anchor == Some(position) {
            self.anchor = None;
        }
        self.scroll_to_cursor();
    }

    /// Puts the cursor at `position` (a click), clamped to the text. With `select` (a drag, or a
    /// Shift click), the selection extends to it.
    pub fn set_cursor(&mut self, position: Position, select: bool) {
        self.history.break_group();
        let offset = self.buffer.position_to_offset(position);
        let position = self.buffer.offset_to_position(offset);
        if select {
            self.anchor.get_or_insert(self.cursor);
        } else {
            self.anchor = None;
        }
        self.cursor = position;
        if self.anchor == Some(position) {
            self.anchor = None;
        }
        self.preferred_column = None;
    }

    /// The selection as byte offsets.
    pub fn selection(&self) -> Selection {
        let head = self.buffer.position_to_offset(self.cursor);
        let anchor = self
            .anchor
            .map_or(head, |anchor| self.buffer.position_to_offset(anchor));
        Selection { anchor, head }
    }

    /// Sets the selection from byte offsets.
    pub fn set_selection(&mut self, selection: Selection) {
        self.cursor = self.buffer.offset_to_position(selection.head);
        self.anchor =
            (!selection.is_empty()).then(|| self.buffer.offset_to_position(selection.anchor));
        self.preferred_column = None;
    }

    /// The selected text, `None` when nothing is selected.
    pub fn selected_text(&self) -> Option<String> {
        let selection = self.selection();
        (!selection.is_empty()).then(|| self.buffer.text_in(selection.range()))
    }

    pub fn select_all(&mut self) {
        self.history.break_group();
        self.set_selection(Selection {
            anchor: 0,
            head: self.buffer.len(),
        });
    }

    /// Selects the word under `position` (a double click), or the single character there when it
    /// is not part of a word.
    pub fn select_word_at(&mut self, position: Position) {
        let offset = self.buffer.position_to_offset(position);
        let position = self.buffer.offset_to_position(offset);
        let line = self.buffer.line(position.line).to_string();
        let start_of_line = offset - position.column;
        let (start, end) = word_range(&line, position.column);
        self.history.break_group();
        self.set_selection(Selection {
            anchor: start_of_line + start,
            head: start_of_line + end,
        });
    }

    /// Selects line `line` and its line break (a triple click).
    pub fn select_line(&mut self, line: usize) {
        let line = line.min(self.buffer.line_count() - 1);
        let start = self.buffer.line_to_byte(line);
        let end = self.buffer.line_to_byte(line + 1);
        self.history.break_group();
        self.set_selection(Selection {
            anchor: start,
            head: end,
        });
    }

    /// Replaces the selection (or inserts at the cursor) with `text`, as typing or another edit.
    pub fn insert(&mut self, text: &str, kind: EditKind) {
        let selection = self.selection();
        self.replace(selection.range(), text, kind);
    }

    /// Replaces `range` with `text` and puts the cursor after it. Does nothing on a read only
    /// document.
    pub fn replace(&mut self, range: Range<usize>, text: &str, kind: EditKind) {
        if self.read_only {
            return;
        }
        let before = self.selection();
        let after = Selection::cursor(range.start + text.len());
        let edit = Edit {
            start: range.start,
            old_text: self.buffer.text_in(range),
            new_text: text.to_string(),
        };
        self.history
            .edit(&mut self.buffer, edit, kind, before, after);
        self.set_selection(after);
        self.scroll_to_cursor();
    }

    /// Enter: a line break in the document's line ending, then the current line's indentation.
    pub fn newline(&mut self) {
        let line = self.buffer.line(self.cursor.line).to_string();
        let indent_len = line.len() - line.trim_start_matches([' ', '\t']).len();
        let column = self.cursor.column.min(indent_len);
        let text = format!("{}{}", self.buffer.line_ending().as_str(), &line[..column]);
        self.insert(&text, EditKind::Other);
    }

    /// Backspace: deletes the selection, or the character (or line break) before the cursor.
    pub fn backspace(&mut self) {
        let selection = self.selection();
        if !selection.is_empty() {
            return self.replace(selection.range(), "", EditKind::Deleting);
        }
        let end = selection.head;
        if end == 0 {
            return;
        }
        let rope = self.buffer.rope();
        let mut start = rope.floor_char_boundary(end - 1);
        if rope.byte(start) == b'\n' && start > 0 && rope.byte(start - 1) == b'\r' {
            start -= 1;
        }
        self.replace(start..end, "", EditKind::Deleting);
    }

    /// Delete: deletes the selection, or the character (or line break) after the cursor.
    pub fn delete(&mut self) {
        let selection = self.selection();
        if !selection.is_empty() {
            return self.replace(selection.range(), "", EditKind::Deleting);
        }
        let start = selection.head;
        let rope = self.buffer.rope();
        if start >= rope.len() {
            return;
        }
        let mut end = rope.ceil_char_boundary(start + 1);
        if rope.byte(start) == b'\r' && end < rope.len() && rope.byte(end) == b'\n' {
            end += 1;
        }
        self.replace(start..end, "", EditKind::Deleting);
    }

    /// The text of one indentation level: the document's own, spaces by default.
    fn indent_unit(&self) -> String {
        match self.buffer.indent() {
            Some(Indent::Tabs) => "\t".to_string(),
            Some(Indent::Spaces(count)) => " ".repeat(usize::from(count)),
            None => " ".repeat(TAB_SIZE),
        }
    }

    /// Tab: indents the selected lines when the selection spans lines, otherwise inserts
    /// indentation up to the next stop.
    pub fn indent(&mut self) {
        let unit = self.indent_unit();
        let selection = self.selection();
        let range = selection.range();
        let first = self.buffer.byte_to_line(range.start);
        let last = self.buffer.byte_to_line(range.end);
        if first == last {
            let text = if unit == "\t" {
                unit
            } else {
                let line = self.buffer.line(self.cursor.line).to_string();
                let column = display_column(&line, self.cursor.column);
                " ".repeat(unit.len() - column % unit.len())
            };
            return self.insert(&text, EditKind::Other);
        }
        self.change_lines(first, last, |line| {
            if line.is_empty() {
                line.to_string()
            } else {
                format!("{unit}{line}")
            }
        });
    }

    /// Shift+Tab: removes one indentation level from the lines of the selection or the cursor.
    pub fn outdent(&mut self) {
        let unit = self.indent_unit();
        let range = self.selection().range();
        let first = self.buffer.byte_to_line(range.start);
        let last = self.buffer.byte_to_line(range.end);
        self.change_lines(first, last, |line| {
            if let Some(rest) = line.strip_prefix(unit.as_str()) {
                rest.to_string()
            } else {
                line.trim_start_matches([' ', '\t']).to_string()
            }
        });
    }

    /// Rewrites lines `first..=last` (without their line breaks) as one undo step, and selects
    /// them whole when they were more than one.
    fn change_lines(&mut self, first: usize, last: usize, change: impl Fn(&str) -> String) {
        if self.read_only {
            return;
        }
        let start = self.buffer.line_to_byte(first);
        let end = self.buffer.line_to_byte(last) + self.buffer.line_len(last);
        let old_text = self.buffer.text_in(start..end);
        let new_text = old_text
            .split('\n')
            .map(|line| {
                let (body, cr) = match line.strip_suffix('\r') {
                    Some(body) => (body, "\r"),
                    None => (line, ""),
                };
                format!("{}{cr}", change(body))
            })
            .collect::<Vec<_>>()
            .join("\n");
        if new_text == old_text {
            return;
        }
        let before = self.selection();
        let after = if first == last {
            let delta = new_text.len() as isize - old_text.len() as isize;
            let head = (before.head as isize + delta).max(start as isize) as usize;
            Selection::cursor(head)
        } else {
            Selection {
                anchor: start,
                head: start + new_text.len(),
            }
        };
        let edit = Edit {
            start,
            old_text,
            new_text,
        };
        self.history
            .edit(&mut self.buffer, edit, EditKind::Other, before, after);
        self.set_selection(after);
        self.scroll_to_cursor();
    }

    /// Cut: removes the selection and returns it.
    pub fn cut(&mut self) -> Option<String> {
        if self.read_only {
            return None;
        }
        let text = self.selected_text()?;
        let range = self.selection().range();
        self.replace(range, "", EditKind::Other);
        Some(text)
    }

    /// Whether the text differs from the file on disk.
    pub fn is_dirty(&self) -> bool {
        self.history.is_dirty()
    }

    /// Applies the save cleanup (`ted_core::cleanup_edits`) as one undo step, keeping the cursor
    /// on its line.
    pub fn apply_cleanup(&mut self, edits: Vec<Edit>) {
        if edits.is_empty() || self.read_only {
            return;
        }
        let before = self.selection();
        let shift = |offset: usize| {
            let mut shifted = offset as isize;
            for edit in &edits {
                if edit.start >= offset {
                    break;
                }
                let removed_end = (edit.start + edit.old_text.len()).min(offset);
                shifted -= (removed_end - edit.start) as isize;
                if edit.start + edit.old_text.len() <= offset {
                    shifted += edit.new_text.len() as isize;
                }
            }
            shifted.max(0) as usize
        };
        let after = Selection {
            anchor: shift(before.anchor),
            head: shift(before.head),
        };
        self.history
            .edit_many(&mut self.buffer, edits, before, after);
        self.set_selection(after);
    }

    /// Replaces the text with the file's new content (changed by another program), keeping the
    /// cursor and the scroll where they can stay. The undo history starts again.
    pub fn reload(&mut self, buffer: Buffer, modified: Option<SystemTime>) {
        let cursor = self.cursor;
        self.buffer = buffer;
        self.history = History::default();
        self.anchor = None;
        self.disk_modified = modified;
        self.set_cursor(cursor, false);
        let viewport = self.viewport;
        self.set_viewport(viewport);
    }

    pub fn undo(&mut self) {
        if let Some(selection) = self.history.undo(&mut self.buffer) {
            self.set_selection(selection);
            self.scroll_to_cursor();
        }
    }

    pub fn redo(&mut self) {
        if let Some(selection) = self.history.redo(&mut self.buffer) {
            self.set_selection(selection);
            self.scroll_to_cursor();
        }
    }

    /// Scrolls the least needed to show the cursor's line and column.
    pub fn scroll_to_cursor(&mut self) {
        let viewport = self.viewport;
        let line_height = f64::from(viewport.line_height);
        let top = f64::from(PADDING_TOP) + self.cursor.line as f64 * line_height;
        let bottom = top + line_height;
        if top - f64::from(PADDING_TOP) < self.scroll_y {
            self.scroll_y = top - f64::from(PADDING_TOP);
        } else if bottom > self.scroll_y + f64::from(viewport.height) {
            self.scroll_y = bottom - f64::from(viewport.height);
        }
        let line = self.buffer.line(self.cursor.line).to_string();
        let column = display_column(&line, self.cursor.column);
        self.widest_seen = self.widest_seen.max(column);
        let x = column as f32 * viewport.char_width;
        let margin = viewport.char_width * 4.0;
        if x < self.scroll_x + margin {
            self.scroll_x = x - margin;
        } else if x + margin > self.scroll_x + viewport.width {
            self.scroll_x = x + margin - viewport.width;
        }
        self.clamp_scroll();
    }
}

/// The display column of byte `column` of `line`: characters before it, with tabs advancing to
/// the next multiple of [`TAB_SIZE`].
pub fn display_column(line: &str, column: usize) -> usize {
    let mut display = 0;
    for (index, ch) in line.char_indices() {
        if index >= column {
            break;
        }
        display = advance(display, ch);
    }
    display
}

/// The byte column of `line` closest to display column `target`, never inside a tab or a
/// character.
pub fn byte_for_display_column(line: &str, target: usize) -> usize {
    let mut display = 0;
    for (index, ch) in line.char_indices() {
        let next = advance(display, ch);
        if next > target {
            // Past the middle of a wide tab, the cursor goes after it.
            return if target - display > (next - display) / 2 {
                index + ch.len_utf8()
            } else {
                index
            };
        }
        display = next;
    }
    line.len()
}

/// `line` as painted: tabs expanded to spaces.
pub fn expand_tabs(line: &str) -> String {
    if !line.contains('\t') {
        return line.to_string();
    }
    let mut out = String::with_capacity(line.len() + 8);
    let mut display = 0;
    for ch in line.chars() {
        let next = advance(display, ch);
        if ch == '\t' {
            out.extend(std::iter::repeat_n(' ', next - display));
        } else {
            out.push(ch);
        }
        display = next;
    }
    out
}

/// The byte index in [`expand_tabs`]'s output of byte `column` of `line`.
pub fn expanded_index(line: &str, column: usize) -> usize {
    let mut out = 0;
    let mut display = 0;
    for (index, ch) in line.char_indices() {
        if index >= column {
            break;
        }
        let next = advance(display, ch);
        out += if ch == '\t' {
            next - display
        } else {
            ch.len_utf8()
        };
        display = next;
    }
    out
}

/// The byte column of `line` for byte `index` of [`expand_tabs`]'s output.
pub fn column_for_expanded(line: &str, index: usize) -> usize {
    let mut out = 0;
    let mut display = 0;
    for (column, ch) in line.char_indices() {
        let next = advance(display, ch);
        let width = if ch == '\t' {
            next - display
        } else {
            ch.len_utf8()
        };
        if out + width > index {
            return if ch == '\t' && index - out > width / 2 {
                column + 1
            } else {
                column
            };
        }
        out += width;
        display = next;
    }
    line.len()
}

/// The byte range of the word around byte `column` of `line`: letters, digits and `_`. Anything
/// else selects that one character; whitespace selects the run of whitespace.
pub fn word_range(line: &str, column: usize) -> (usize, usize) {
    let column = column.min(line.len());
    let class = |ch: char| {
        if ch.is_alphanumeric() || ch == '_' {
            0
        } else if ch.is_whitespace() {
            1
        } else {
            2
        }
    };
    let Some(ch) = line[column..]
        .chars()
        .next()
        .or_else(|| line[..column].chars().next_back())
    else {
        return (column, column);
    };
    let at = if column == line.len() {
        column - ch.len_utf8()
    } else {
        column
    };
    let target = class(ch);
    if target == 2 {
        return (at, at + ch.len_utf8());
    }
    let start = line[..at]
        .char_indices()
        .rev()
        .take_while(|(_, ch)| class(*ch) == target)
        .last()
        .map_or(at, |(index, _)| index);
    let end = line[at..]
        .char_indices()
        .find(|(_, ch)| class(*ch) != target)
        .map_or(line.len(), |(index, _)| at + index);
    (start, end)
}

/// The display column after `ch` at display column `display`.
pub(crate) fn advance(display: usize, ch: char) -> usize {
    if ch == '\t' {
        (display / TAB_SIZE + 1) * TAB_SIZE
    } else {
        display + 1
    }
}

/// The line count of a file of `file_size` bytes, estimated from the average line of a start of
/// it that has `head_lines` lines in `head_len` bytes.
pub fn estimate_lines(file_size: u64, head_len: usize, head_lines: usize) -> usize {
    if head_len == 0 {
        return head_lines;
    }
    (file_size as f64 * head_lines as f64 / head_len as f64).ceil() as usize
}

#[cfg(test)]
mod tests {
    use super::*;
    use pretty_assertions::assert_eq;

    fn editor(text: &str) -> EditorState {
        let mut state = EditorState::new(Buffer::from_text(text));
        state.set_viewport(Viewport {
            width: 400.0,
            height: 200.0,
            line_height: 20.0,
            char_width: 8.0,
        });
        state
    }

    #[test]
    fn the_scroll_range_has_padding_and_scroll_past_the_end() {
        let state = editor(&"x\n".repeat(99));
        // 10 + 100 lines * 20 + 40% of 200.
        assert_eq!(state.content_height(), 2090.0);
        assert_eq!(state.max_scroll_y(), 1890.0);
    }

    #[test]
    fn visible_lines_follow_the_scroll() {
        let mut state = editor(&"x\n".repeat(99));
        assert_eq!(state.visible_lines(), (0..11, 10.0));
        state.scroll_by(0.0, 35.0);
        // The document y of the viewport's top is 25: line 1 starts 5 px above it.
        assert_eq!(state.visible_lines(), (1..12, -5.0));
        state.scroll_by(0.0, 1e9);
        assert_eq!(state.scroll_y, 1890.0);
        assert_eq!(state.visible_lines().0.end, 100);
    }

    #[test]
    fn a_huge_document_keeps_subpixel_offsets() {
        let mut state = editor("");
        state.buffer = Buffer::from_text(&"x\n".repeat(2_000_000));
        state.set_viewport(state.viewport);
        state.scroll_to_fraction(1.0);
        state.scroll_by(0.0, -1.5);
        let (lines, offset) = state.visible_lines();
        assert!(lines.start > 1_999_000);
        assert!((offset.fract().abs() - 0.5).abs() < 1e-3, "{offset}");
    }

    #[test]
    fn horizontal_moves_cross_lines_and_characters() {
        let mut state = editor("añb\ncd");
        state.cursor = Position::new(0, 1);
        state.move_cursor(Movement::Right, false);
        assert_eq!(state.cursor, Position::new(0, 3));
        state.move_cursor(Movement::Right, false);
        state.move_cursor(Movement::Right, false);
        assert_eq!(state.cursor, Position::new(1, 0));
        state.move_cursor(Movement::Left, false);
        assert_eq!(state.cursor, Position::new(0, 4));
        state.move_cursor(Movement::Left, false);
        assert_eq!(state.cursor, Position::new(0, 3));
        state.move_cursor(Movement::Left, false);
        assert_eq!(state.cursor, Position::new(0, 1));
    }

    #[test]
    fn vertical_moves_keep_the_column() {
        let mut state = editor("long line\nab\nlonger line");
        state.cursor = Position::new(0, 7);
        state.move_cursor(Movement::Down, false);
        assert_eq!(state.cursor, Position::new(1, 2));
        state.move_cursor(Movement::Down, false);
        assert_eq!(state.cursor, Position::new(2, 7));
        state.move_cursor(Movement::Down, false);
        assert_eq!(state.cursor, Position::new(2, 11));
        state.move_cursor(Movement::Up, false);
        assert_eq!(state.cursor, Position::new(1, 2));
    }

    #[test]
    fn document_moves_scroll_the_cursor_into_view() {
        let mut state = editor(&"x\n".repeat(99));
        state.move_cursor(Movement::DocumentEnd, false);
        assert_eq!(state.cursor, Position::new(99, 0));
        // Line 99's bottom is 10 + 100 * 20 = 2010, at the bottom of a 200 px viewport.
        assert_eq!(state.scroll_y, 1810.0);
        state.move_cursor(Movement::DocumentStart, false);
        assert_eq!(state.scroll_y, 0.0);
    }

    #[test]
    fn page_moves_scroll_by_a_page() {
        let mut state = editor(&"x\n".repeat(99));
        state.move_cursor(Movement::PageDown, false);
        assert_eq!(state.cursor.line, 10);
        assert_eq!(state.scroll_y, 200.0);
        state.move_cursor(Movement::PageUp, false);
        assert_eq!(state.cursor.line, 0);
        assert_eq!(state.scroll_y, 0.0);
    }

    #[test]
    fn the_gutter_makes_room_for_the_expected_lines() {
        let mut state = editor("a\nb\n");
        assert_eq!(state.gutter_lines(), 3);
        state.expected_lines = Some(estimate_lines(1_000_000, 100, 10));
        assert_eq!(state.gutter_lines(), 100_000);
        assert_eq!(estimate_lines(5, 0, 1), 1);
    }

    fn at(state: &mut EditorState, line: usize, column: usize) {
        state.set_cursor(Position::new(line, column), false);
    }

    #[test]
    fn enter_keeps_the_indentation_and_the_line_ending() {
        let mut state = editor("    let a = 1;\r\n");
        at(&mut state, 0, 14);
        state.newline();
        assert_eq!(state.buffer.to_text(), "    let a = 1;\r\n    \r\n");
        assert_eq!(state.cursor, Position::new(1, 4));
    }

    #[test]
    fn typing_replaces_the_selection_and_undoes_in_one_step() {
        let mut state = editor("hello world");
        state.set_selection(Selection { anchor: 0, head: 5 });
        for ch in ["H", "i"] {
            state.insert(ch, EditKind::Typing);
        }
        assert_eq!(state.buffer.to_text(), "Hi world");
        state.undo();
        assert_eq!(state.buffer.to_text(), "hello world");
        assert_eq!(state.selection(), Selection { anchor: 0, head: 5 });
        state.redo();
        assert_eq!(state.buffer.to_text(), "Hi world");
    }

    #[test]
    fn backspace_and_delete_take_a_crlf_whole() {
        let mut state = editor("a\r\nb");
        at(&mut state, 1, 0);
        state.backspace();
        assert_eq!(state.buffer.to_text(), "ab");
        let mut state = editor("añ\r\nb");
        at(&mut state, 0, 3);
        state.delete();
        assert_eq!(state.buffer.to_text(), "añb");
        state.backspace();
        assert_eq!(state.buffer.to_text(), "ab");
    }

    #[test]
    fn tab_inserts_to_the_next_stop_or_indents_lines() {
        let mut state = editor("ab\ncd\n");
        at(&mut state, 0, 1);
        state.indent();
        assert_eq!(state.buffer.to_text(), "a   b\ncd\n");
        state.select_all();
        state.indent();
        assert_eq!(state.buffer.to_text(), "    a   b\n    cd\n");
        state.outdent();
        assert_eq!(state.buffer.to_text(), "a   b\ncd\n");
    }

    #[test]
    fn words_and_lines_select_with_clicks() {
        assert_eq!(word_range("let foo_bar = 1;", 6), (4, 11));
        assert_eq!(word_range("let foo_bar = 1;", 11), (11, 12));
        assert_eq!(word_range("let foo_bar = 1;", 12), (12, 13));
        assert_eq!(word_range("a, b", 1), (1, 2));
        assert_eq!(word_range("ñandú", 5), (0, 7));
        let mut state = editor("one two\nthree\n");
        state.select_word_at(Position::new(0, 5));
        assert_eq!(state.selected_text().as_deref(), Some("two"));
        state.select_line(0);
        assert_eq!(state.selected_text().as_deref(), Some("one two\n"));
    }

    #[test]
    fn cut_removes_the_selection() {
        let mut state = editor("one two");
        state.set_selection(Selection { anchor: 3, head: 7 });
        assert_eq!(state.cut().as_deref(), Some(" two"));
        assert_eq!(state.buffer.to_text(), "one");
    }

    #[test]
    fn shift_moves_extend_and_plain_moves_collapse() {
        let mut state = editor("abcdef");
        at(&mut state, 0, 1);
        state.move_cursor(Movement::Right, true);
        state.move_cursor(Movement::Right, true);
        assert_eq!(state.selected_text().as_deref(), Some("bc"));
        state.move_cursor(Movement::Left, false);
        assert_eq!(state.cursor, Position::new(0, 1));
        assert_eq!(state.selected_text(), None);
    }

    #[test]
    fn the_save_cleanup_keeps_the_cursor_and_undoes_at_once() {
        let mut state = editor("a  \nbc  \nd");
        at(&mut state, 1, 2);
        let edits = ted_core::cleanup_edits(&state.buffer, true, true);
        state.apply_cleanup(edits);
        assert_eq!(state.buffer.to_text(), "a\nbc\nd\n");
        assert_eq!(state.cursor, Position::new(1, 2));
        assert!(state.is_dirty());
        state.undo();
        assert_eq!(state.buffer.to_text(), "a  \nbc  \nd");
        assert!(!state.is_dirty());
    }

    #[test]
    fn a_read_only_document_does_not_change() {
        let mut state = editor("text");
        state.read_only = true;
        state.insert("x", EditKind::Typing);
        state.backspace();
        state.select_all();
        assert_eq!(state.cut(), None);
        assert_eq!(state.buffer.to_text(), "text");
    }

    #[test]
    fn tabs_advance_to_the_next_stop() {
        assert_eq!(expand_tabs("a\tb"), "a   b");
        assert_eq!(display_column("a\tb", 2), 4);
        assert_eq!(byte_for_display_column("a\tb", 4), 2);
        assert_eq!(byte_for_display_column("a\tb", 2), 1);
        assert_eq!(byte_for_display_column("a\tb", 3), 2);
        assert_eq!(expanded_index("a\tñb", 3), 6);
        assert_eq!(column_for_expanded("a\tñb", 6), 4);
        assert_eq!(column_for_expanded("a\tb", 1), 1);
        assert_eq!(column_for_expanded("a\tb", 3), 2);
    }
}
