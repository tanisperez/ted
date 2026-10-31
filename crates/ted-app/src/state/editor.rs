//! The state of one editor, outside the gpui element: the buffer, the cursor, the scroll offsets
//! and the viewport math (which lines are visible, how far the document scrolls). Plain Rust, so
//! it is unit tested without a window.
//!
//! The vertical scroll offset is an `f64`: a 100 MB log has more than a million lines, about 27
//! million pixels, and an `f32` loses whole pixels well before that. Everything handed to gpui is
//! relative to the first visible line, so it stays small.

use std::ops::Range;
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::time::SystemTime;

use ted_core::{Buffer, Edit, EditKind, History, Indent, Position, Rope, SearchQuery, Selection};

use super::syntax::Syntax;

/// Empty space above the first line, in pixels.
pub const PADDING_TOP: f32 = 10.0;
/// How far the document scrolls past its last line, as a fraction of the viewport height.
pub const SCROLL_PAST_END: f32 = 0.4;
/// Empty space after the longest line, in pixels.
pub const PADDING_RIGHT: f32 = 24.0;
/// Columns a tab advances to the next multiple of (the `tab_size` setting). Process wide, like
/// the settings, and an atomic because the layout helpers below read it once per character.
static TAB_SIZE: AtomicUsize = AtomicUsize::new(4);
/// Whether Tab inserts spaces where the file's indentation is unknown (`insert_spaces`).
static INSERT_SPACES: AtomicBool = AtomicBool::new(true);

/// Applies the `tab_size` and `insert_spaces` settings to every editor. The caller repaints.
pub fn set_indentation(tab_size: usize, insert_spaces: bool) {
    TAB_SIZE.store(tab_size.max(1), Ordering::Relaxed);
    INSERT_SPACES.store(insert_spaces, Ordering::Relaxed);
}

/// The current tab size.
pub fn tab_size() -> usize {
    TAB_SIZE.load(Ordering::Relaxed)
}

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

/// A cursor other than the primary one, with its selection: the same fields the primary cursor
/// has in [`EditorState`].
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Caret {
    pub cursor: Position,
    pub anchor: Option<Position>,
    preferred_column: Option<usize>,
}

impl Caret {
    fn at(position: Position) -> Self {
        Self {
            cursor: position,
            anchor: None,
            preferred_column: None,
        }
    }
}

/// One open document in an editor.
#[derive(Debug, Clone, Default)]
pub struct EditorState {
    pub buffer: Buffer,
    /// Where the primary cursor is: the one the view scrolls to and the status bar shows.
    pub cursor: Position,
    /// Where the selection started, when there is one. The selection runs from here to the
    /// cursor.
    pub anchor: Option<Position>,
    /// The other cursors (Alt+click, the column selection), sorted by position. No two cursors
    /// or selections overlap.
    pub others: Vec<Caret>,
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

    /// Moves every cursor and scrolls the primary one into view. With `select`, the selections
    /// extend to the new places; without it, they collapse (to their start or end for Left and
    /// Right).
    pub fn move_cursor(&mut self, movement: Movement, select: bool) {
        self.history.break_group();
        let others = std::mem::take(&mut self.others);
        if !others.is_empty() {
            let primary = self.caret();
            let mut moved = Vec::with_capacity(others.len());
            for caret in others {
                self.set_caret(caret);
                self.move_one(movement, select, false);
                moved.push(self.caret());
            }
            self.set_caret(primary);
            self.others = moved;
        }
        self.move_one(movement, select, true);
        self.normalize();
    }

    /// Moves the primary cursor; with `primary`, the view follows it.
    fn move_one(&mut self, movement: Movement, select: bool, primary: bool) {
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
            if primary {
                self.scroll_to_cursor();
            }
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
        self.cursor = position;
        if self.anchor == Some(position) {
            self.anchor = None;
        }
        if !primary {
            return;
        }
        // A page movement also scrolls by a page, so the cursor keeps its place on screen.
        let scroll_lines = match movement {
            Movement::PageUp => -(page as f64),
            Movement::PageDown => page as f64,
            _ => 0.0,
        };
        self.scroll_y += scroll_lines * f64::from(self.viewport.line_height);
        self.clamp_scroll();
        self.scroll_to_cursor();
    }

    /// Puts the cursor at `position` (a click), clamped to the text, and drops the other cursors.
    /// With `select` (a drag, or a Shift click), the selection extends to it.
    pub fn set_cursor(&mut self, position: Position, select: bool) {
        self.history.break_group();
        self.others.clear();
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

    /// The primary selection as byte offsets.
    pub fn selection(&self) -> Selection {
        self.caret_selection(&self.caret())
    }

    /// Sets the selection from byte offsets, dropping the other cursors.
    pub fn set_selection(&mut self, selection: Selection) {
        self.others.clear();
        self.set_caret(self.caret_at(selection));
    }

    /// Every selection as byte offsets, the primary one first, then the others in order.
    pub fn selections(&self) -> Vec<Selection> {
        std::iter::once(self.caret())
            .chain(self.others.iter().copied())
            .map(|caret| self.caret_selection(&caret))
            .collect()
    }

    /// Sets every selection from byte offsets, the primary one first. Overlapping ones merge.
    pub fn set_selections(&mut self, selections: &[Selection]) {
        let Some((primary, others)) = selections.split_first() else {
            return;
        };
        self.set_selection(*primary);
        self.others = others
            .iter()
            .map(|&selection| self.caret_at(selection))
            .collect();
        self.normalize();
    }

    /// Escape: keeps only the primary cursor. Returns whether there were others.
    pub fn collapse_cursors(&mut self) -> bool {
        let had_others = !self.others.is_empty();
        self.others.clear();
        had_others
    }

    /// Whether byte `offset` is inside one of the selections, for a right click that keeps them.
    pub fn is_selected(&self, offset: usize) -> bool {
        self.selections()
            .iter()
            .any(|selection| selection.range().contains(&offset))
    }

    /// Alt+click: adds a cursor at `position`, which becomes the primary one, or removes the
    /// cursor already there unless it is the last one.
    pub fn toggle_cursor(&mut self, position: Position) {
        self.history.break_group();
        let offset = self.buffer.position_to_offset(position);
        let position = self.buffer.offset_to_position(offset);
        let at = Caret::at(position);
        if let Some(index) = self
            .others
            .iter()
            .position(|caret| caret.cursor == position && caret.anchor.is_none())
        {
            self.others.remove(index);
            return;
        }
        if self.cursor == position && self.anchor.is_none() {
            if let Some(last) = self.others.pop() {
                self.set_caret(last);
            }
            return;
        }
        self.others.push(self.caret());
        self.set_caret(at);
        self.normalize();
    }

    /// The column selection (a middle button drag): on every line from `from.0` to `to.0`, the
    /// text between display columns `from.1` and `to.1`, clamped to each line. The primary
    /// cursor is on the line of `to`, where the pointer is.
    pub fn select_columns(&mut self, from: (usize, usize), to: (usize, usize)) {
        self.history.break_group();
        let last_line = self.buffer.line_count() - 1;
        let (from_line, to_line) = (from.0.min(last_line), to.0.min(last_line));
        let caret = |line: usize| {
            let text = self.buffer.line(line).to_string();
            let anchor = Position::new(line, byte_for_display_column(&text, from.1));
            let cursor = Position::new(line, byte_for_display_column(&text, to.1));
            Caret {
                cursor,
                anchor: (anchor != cursor).then_some(anchor),
                preferred_column: None,
            }
        };
        let lines = from_line.min(to_line)..=from_line.max(to_line);
        self.others = lines.filter(|&line| line != to_line).map(caret).collect();
        self.set_caret(caret(to_line));
        self.normalize();
    }

    /// Alt+J, first half: without a selection, selects the word at the primary cursor and
    /// returns `None`. With one, returns the query for its text and where the search starts:
    /// the next occurrence is searched by [`next_occurrence`], off the main thread.
    pub fn occurrence_search(&mut self) -> Option<(SearchQuery, usize, Vec<Range<usize>>)> {
        let primary = self.selection();
        if primary.is_empty() {
            let line = self.buffer.line(self.cursor.line).to_string();
            let start_of_line = primary.head - self.cursor.column;
            let (start, end) = word_range(&line, self.cursor.column);
            if start < end {
                self.history.break_group();
                self.set_caret(self.caret_at(Selection {
                    anchor: start_of_line + start,
                    head: start_of_line + end,
                }));
                self.normalize();
            }
            return None;
        }
        let query = SearchQuery::new(&self.buffer.text_in(primary.range()), true)?;
        let mut selected: Vec<Range<usize>> = self
            .selections()
            .iter()
            .map(|selection| selection.range())
            .collect();
        selected.sort_by_key(|range| range.start);
        Some((query, primary.range().end, selected))
    }

    /// Alt+J, second half: adds `found` as a selection, which becomes the primary one, and
    /// scrolls to it.
    pub fn add_occurrence(&mut self, found: Range<usize>) {
        self.history.break_group();
        self.others.push(self.caret());
        self.set_caret(self.caret_at(Selection {
            anchor: found.start,
            head: found.end,
        }));
        self.normalize();
        self.scroll_to_cursor();
    }

    /// Alt+Shift+J: removes the primary selection, the last occurrence added, and makes the one
    /// before it (in the document, wrapping around) the primary one.
    pub fn unselect_occurrence(&mut self) {
        if self.others.is_empty() {
            return;
        }
        self.history.break_group();
        let cursor = self.cursor;
        let index = self
            .others
            .iter()
            .rposition(|caret| caret.cursor < cursor)
            .unwrap_or(self.others.len() - 1);
        let previous = self.others.remove(index);
        self.set_caret(previous);
        self.scroll_to_cursor();
    }

    fn caret(&self) -> Caret {
        Caret {
            cursor: self.cursor,
            anchor: self.anchor,
            preferred_column: self.preferred_column,
        }
    }

    fn set_caret(&mut self, caret: Caret) {
        self.cursor = caret.cursor;
        self.anchor = caret.anchor;
        self.preferred_column = caret.preferred_column;
    }

    fn caret_selection(&self, caret: &Caret) -> Selection {
        let head = self.buffer.position_to_offset(caret.cursor);
        let anchor = caret
            .anchor
            .map_or(head, |anchor| self.buffer.position_to_offset(anchor));
        Selection { anchor, head }
    }

    fn caret_at(&self, selection: Selection) -> Caret {
        Caret {
            cursor: self.buffer.offset_to_position(selection.head),
            anchor: (!selection.is_empty())
                .then(|| self.buffer.offset_to_position(selection.anchor)),
            preferred_column: None,
        }
    }

    /// Sorts the other cursors and merges the ones that overlap or meet (a merged one that held
    /// the primary cursor stays the primary). Linear in the number of cursors.
    fn normalize(&mut self) {
        if self.others.is_empty() {
            return;
        }
        let primary = self.caret();
        let mut carets: Vec<(Range<usize>, Caret, bool)> = std::iter::once((primary, true))
            .chain(
                std::mem::take(&mut self.others)
                    .into_iter()
                    .map(|caret| (caret, false)),
            )
            .map(|(caret, is_primary)| (self.caret_selection(&caret).range(), caret, is_primary))
            .collect();
        carets.sort_by_key(|(range, _, _)| (range.start, range.end));
        let mut merged: Vec<(Range<usize>, Caret, bool)> = Vec::with_capacity(carets.len());
        for (range, caret, is_primary) in carets {
            if let Some((last, last_caret, last_primary)) = merged.last_mut() {
                let meets = range.start < last.end
                    || range.start == last.start
                    || (range.start == last.end
                        && (range.start == range.end || last.start == last.end));
                if meets {
                    if range.end > last.end {
                        last.end = last.end.max(range.end);
                        *last_caret = self.caret_at(Selection {
                            anchor: last.start,
                            head: last.end,
                        });
                    }
                    *last_primary |= is_primary;
                    continue;
                }
            }
            merged.push((range, caret, is_primary));
        }
        let primary_index = merged
            .iter()
            .position(|(_, _, is_primary)| *is_primary)
            .unwrap_or(0);
        let (_, primary, _) = merged.remove(primary_index);
        self.set_caret(primary);
        self.others = merged.into_iter().map(|(_, caret, _)| caret).collect();
    }

    /// The other cursors on lines `lines`, for painting: a binary search, not a scan.
    pub fn others_on_lines(&self, lines: Range<usize>) -> &[Caret] {
        let line_of = |caret: &Caret| caret.anchor.unwrap_or(caret.cursor).min(caret.cursor);
        let start = self.others.partition_point(|caret| {
            caret.cursor.max(caret.anchor.unwrap_or(caret.cursor)).line < lines.start
        });
        let end = self
            .others
            .partition_point(|caret| line_of(caret).line < lines.end);
        &self.others[start..end.max(start)]
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

    /// Replaces every selection (or inserts at every cursor) with `text`, as typing or another
    /// edit.
    pub fn insert(&mut self, text: &str, kind: EditKind) {
        self.edit_each(kind, |_, _, selection| {
            Some((selection.range(), text.to_string()))
        });
    }

    /// Pastes `text`: when it has as many lines as there are cursors, one line at each,
    /// otherwise all of it at every cursor.
    pub fn paste(&mut self, text: &str) {
        let count = self.others.len() + 1;
        let ending = self.buffer.line_ending().as_str();
        let body = text.strip_suffix(ending).unwrap_or(text);
        let lines: Vec<&str> = body.split(ending).collect();
        if count > 1 && lines.len() == count {
            let mut lines = lines.into_iter();
            self.edit_each(EditKind::Other, |_, _, selection| {
                Some((selection.range(), lines.next()?.to_string()))
            });
        } else {
            self.insert(text, EditKind::Other);
        }
    }

    /// Replaces `range` with `text` and puts the only cursor after it (an input method). Does
    /// nothing on a read only document.
    pub fn replace(&mut self, range: Range<usize>, text: &str, kind: EditKind) {
        if self.read_only {
            return;
        }
        let before = self.selections();
        let after = Selection::cursor(range.start + text.len());
        let edit = Edit {
            start: range.start,
            old_text: self.buffer.text_in(range),
            new_text: text.to_string(),
        };
        self.history
            .edit_many(&mut self.buffer, vec![edit], kind, before, vec![after]);
        self.set_selection(after);
        self.scroll_to_cursor();
    }

    /// The edit at every cursor, as one undo step: `edit` gets the buffer, the cursor and its
    /// selection (in document order) and returns the range to replace and its new text, or
    /// `None` to leave that cursor alone. Every edited cursor ends after its new text. Does
    /// nothing on a read only document.
    fn edit_each(
        &mut self,
        kind: EditKind,
        mut edit: impl FnMut(&Buffer, &Caret, Selection) -> Option<(Range<usize>, String)>,
    ) {
        self.edit_each_to(kind, |buffer, caret, selection| {
            edit(buffer, caret, selection).map(|(range, text)| (range, text, None))
        });
    }

    /// As [`Self::edit_each`], but `edit` may also say where the cursor's selection ends: its
    /// anchor and head relative to the start of the edit, in the text after it.
    #[allow(clippy::type_complexity)]
    fn edit_each_to(
        &mut self,
        kind: EditKind,
        mut edit: impl FnMut(
            &Buffer,
            &Caret,
            Selection,
        ) -> Option<(Range<usize>, String, Option<(isize, isize)>)>,
    ) {
        if self.read_only {
            return;
        }
        let before = self.selections();
        let carets: Vec<Caret> = std::iter::once(self.caret())
            .chain(self.others.iter().copied())
            .collect();
        let mut order: Vec<usize> = (0..before.len()).collect();
        order.sort_by_key(|&index| before[index].range().start);
        let mut edits = Vec::with_capacity(order.len());
        let mut after = before.clone();
        // How much the edits so far moved the text after them.
        let mut shift = 0isize;
        let mut edited_end = 0;
        let moved = |offset: usize, shift: isize| (offset as isize + shift) as usize;
        for index in order {
            let selection = before[index];
            let change = edit(&self.buffer, &carets[index], selection)
                .filter(|(range, _, _)| range.start >= edited_end || edits.is_empty());
            let Some((range, text, to)) = change else {
                after[index] = Selection {
                    anchor: moved(selection.anchor, shift),
                    head: moved(selection.head, shift),
                };
                continue;
            };
            let start = moved(range.start, shift);
            after[index] = match to {
                Some((anchor, head)) => Selection {
                    anchor: moved(start, anchor),
                    head: moved(start, head),
                },
                None => Selection::cursor(start + text.len()),
            };
            shift += text.len() as isize - range.len() as isize;
            edited_end = range.end;
            edits.push(Edit {
                start: range.start,
                old_text: self.buffer.text_in(range),
                new_text: text,
            });
        }
        if edits.is_empty() {
            return;
        }
        self.history
            .edit_many(&mut self.buffer, edits, kind, before, after.clone());
        self.set_selections(&after);
        self.scroll_to_cursor();
    }

    /// Enter: a line break in the document's line ending, then the current line's indentation.
    pub fn newline(&mut self) {
        let ending = self.buffer.line_ending().as_str();
        self.edit_each(EditKind::Other, |buffer, caret, selection| {
            let line = buffer.line(caret.cursor.line).to_string();
            let indent_len = line.len() - line.trim_start_matches([' ', '\t']).len();
            let column = caret.cursor.column.min(indent_len);
            Some((selection.range(), format!("{ending}{}", &line[..column])))
        });
    }

    /// Backspace: deletes the selection, or the character (or line break) before the cursor.
    pub fn backspace(&mut self) {
        self.edit_each(EditKind::Deleting, |buffer, _, selection| {
            if !selection.is_empty() {
                return Some((selection.range(), String::new()));
            }
            let end = selection.head;
            if end == 0 {
                return None;
            }
            let rope = buffer.rope();
            let mut start = rope.floor_char_boundary(end - 1);
            if rope.byte(start) == b'\n' && start > 0 && rope.byte(start - 1) == b'\r' {
                start -= 1;
            }
            Some((start..end, String::new()))
        });
    }

    /// Delete: deletes the selection, or the character (or line break) after the cursor.
    pub fn delete(&mut self) {
        self.edit_each(EditKind::Deleting, |buffer, _, selection| {
            if !selection.is_empty() {
                return Some((selection.range(), String::new()));
            }
            let start = selection.head;
            let rope = buffer.rope();
            if start >= rope.len() {
                return None;
            }
            let mut end = rope.ceil_char_boundary(start + 1);
            if rope.byte(start) == b'\r' && end < rope.len() && rope.byte(end) == b'\n' {
                end += 1;
            }
            Some((start..end, String::new()))
        });
    }

    /// The text of one indentation level: the document's own, spaces by default.
    fn indent_unit(&self) -> String {
        match self.buffer.indent() {
            Some(Indent::Tabs) => "\t".to_string(),
            Some(Indent::Spaces(count)) => " ".repeat(usize::from(count)),
            None if INSERT_SPACES.load(Ordering::Relaxed) => " ".repeat(tab_size()),
            None => "\t".to_string(),
        }
    }

    /// The lines each selection touches, first and last.
    fn selected_lines(&self) -> Vec<(usize, usize)> {
        self.selections()
            .iter()
            .map(|selection| {
                let range = selection.range();
                (
                    self.buffer.byte_to_line(range.start),
                    self.buffer.byte_to_line(range.end),
                )
            })
            .collect()
    }

    /// Tab: indents the selected lines when a selection spans lines, otherwise inserts
    /// indentation up to the next stop at every cursor.
    pub fn indent(&mut self) {
        let unit = self.indent_unit();
        let lines = self.selected_lines();
        if lines.iter().all(|(first, last)| first == last) {
            return self.edit_each(EditKind::Other, |buffer, caret, selection| {
                let text = if unit == "\t" {
                    unit.clone()
                } else {
                    let line = buffer.line(caret.cursor.line).to_string();
                    let column = display_column(&line, caret.cursor.column);
                    " ".repeat(unit.len() - column % unit.len())
                };
                Some((selection.range(), text))
            });
        }
        self.change_lines(&lines, |line| {
            if line.is_empty() {
                line.to_string()
            } else {
                format!("{unit}{line}")
            }
        });
    }

    /// Shift+Tab: removes one indentation level from the lines of the selections or the cursors.
    pub fn outdent(&mut self) {
        let unit = self.indent_unit();
        let lines = self.selected_lines();
        self.change_lines(&lines, |line| {
            if let Some(rest) = line.strip_prefix(unit.as_str()) {
                rest.to_string()
            } else {
                line.trim_start_matches([' ', '\t']).to_string()
            }
        });
    }

    /// Rewrites the lines in `lines` (first and last of each selection, without their line
    /// breaks) as one undo step. With one cursor and more than one line, selects them whole;
    /// otherwise the selections follow the text.
    fn change_lines(&mut self, lines: &[(usize, usize)], change: impl Fn(&str) -> String) {
        if self.read_only {
            return;
        }
        let mut numbers: Vec<usize> = lines
            .iter()
            .flat_map(|&(first, last)| first..=last)
            .collect();
        numbers.sort_unstable();
        numbers.dedup();
        let edits: Vec<Edit> = numbers
            .iter()
            .filter_map(|&line| {
                let start = self.buffer.line_to_byte(line);
                let old_text = self
                    .buffer
                    .text_in(start..start + self.buffer.line_len(line));
                let new_text = change(&old_text);
                (new_text != old_text).then(|| trimmed_edit(start, &old_text, &new_text))
            })
            .collect();
        if edits.is_empty() {
            return;
        }
        let before = self.selections();
        let after: Vec<Selection> = match lines {
            [(first, last)] if first != last => {
                let start = self.buffer.line_to_byte(*first);
                let end = self.buffer.line_to_byte(*last) + self.buffer.line_len(*last);
                let delta: isize = edits
                    .iter()
                    .map(|edit| edit.new_text.len() as isize - edit.old_text.len() as isize)
                    .sum();
                vec![Selection {
                    anchor: start,
                    head: (end as isize + delta) as usize,
                }]
            }
            _ => before
                .iter()
                .map(|selection| Selection {
                    anchor: shift_offset(selection.anchor, &edits),
                    head: shift_offset(selection.head, &edits),
                })
                .collect(),
        };
        self.history.edit_many(
            &mut self.buffer,
            edits,
            EditKind::Other,
            before,
            after.clone(),
        );
        self.set_selections(&after);
        self.scroll_to_cursor();
    }

    /// Ctrl+D: duplicates the selection right after it, or, with no selection or one that spans
    /// lines, the whole lines below them. The cursor or the selection moves to the copy.
    pub fn duplicate(&mut self) {
        let ending = self.buffer.line_ending().as_str();
        // Cursors are visited in document order: two on the same lines duplicate them once.
        let mut done_until = None;
        self.edit_each_to(EditKind::Other, |buffer, _, selection| {
            let range = selection.range();
            let first = buffer.byte_to_line(range.start);
            let last = buffer.byte_to_line(range.end);
            if !selection.is_empty() && first == last {
                let text = buffer.text_in(range.clone());
                let len = text.len() as isize;
                let to = if selection.head < selection.anchor {
                    (len, 0)
                } else {
                    (0, len)
                };
                return Some((range.end..range.end, text, Some(to)));
            }
            if done_until.is_some_and(|done| first <= done) {
                return None;
            }
            done_until = Some(last);
            let start = buffer.line_to_byte(first);
            let end = buffer.line_to_byte(last) + buffer.line_len(last);
            let text = format!("{ending}{}", buffer.text_in(start..end));
            // The copy starts after the inserted line break.
            let copy = |offset: usize| (ending.len() + offset - start) as isize;
            let to = (copy(selection.anchor), copy(selection.head));
            Some((end..end, text, Some(to)))
        });
    }

    /// Ctrl+Alt+D: deletes the whole lines of every cursor and selection, line breaks included.
    /// Each cursor stays at its column on the line that takes their place.
    pub fn delete_lines(&mut self) {
        if self.read_only {
            return;
        }
        let before = self.selections();
        let line_count = self.buffer.line_count();
        // The lines of each selection, first and last, and its cursor's column. A selection
        // that ends at the start of a line leaves that line alone.
        let mut blocks: Vec<(usize, usize, usize, bool)> = before
            .iter()
            .enumerate()
            .map(|(index, selection)| {
                let range = selection.range();
                let first = self.buffer.byte_to_line(range.start);
                let mut last = self.buffer.byte_to_line(range.end);
                if last > first && range.end == self.buffer.line_to_byte(last) {
                    last -= 1;
                }
                let head = self.buffer.offset_to_position(selection.head);
                (first, last, head.column, index == 0)
            })
            .collect();
        blocks.sort_by_key(|&(first, _, _, _)| first);
        let mut merged: Vec<(usize, usize, usize, bool)> = Vec::with_capacity(blocks.len());
        for block in blocks {
            match merged.last_mut() {
                Some(last) if block.0 <= last.1 + 1 => {
                    last.1 = last.1.max(block.1);
                    if block.3 {
                        last.2 = block.2;
                        last.3 = true;
                    }
                }
                _ => merged.push(block),
            }
        }
        let mut edits = Vec::with_capacity(merged.len());
        let mut after = Vec::with_capacity(merged.len());
        let mut shift = 0isize;
        for &(first, last, column, primary) in &merged {
            let mut start = self.buffer.line_to_byte(first);
            let end = self.buffer.line_to_byte(last + 1);
            // The last lines of a file without a final line break: the break before them goes.
            let (cursor_line, cursor_line_start) = if last + 1 == line_count && first > 0 {
                start = self.buffer.line_to_byte(first - 1) + self.buffer.line_len(first - 1);
                (first - 1, self.buffer.line_to_byte(first - 1))
            } else {
                (last + 1, end)
            };
            let cursor_column = if cursor_line < line_count {
                column.min(self.buffer.line_len(cursor_line))
            } else {
                0
            };
            let new_start = (start as isize + shift) as usize;
            let cursor = if cursor_line_start >= end {
                new_start + cursor_column
            } else {
                (cursor_line_start as isize + shift) as usize + cursor_column
            };
            shift -= (end - start) as isize;
            edits.push(Edit {
                start,
                old_text: self.buffer.text_in(start..end),
                new_text: String::new(),
            });
            let selection = Selection::cursor(cursor);
            if primary {
                after.insert(0, selection);
            } else {
                after.push(selection);
            }
        }
        self.history.edit_many(
            &mut self.buffer,
            edits,
            EditKind::Other,
            before,
            after.clone(),
        );
        self.set_selections(&after);
        self.scroll_to_cursor();
    }

    /// Selects the whole line of every cursor when nothing is selected, for Copy and Cut
    /// without a selection, as most editors do.
    pub fn select_lines_if_empty(&mut self) {
        let selections = self.selections();
        if selections.iter().any(|selection| !selection.is_empty()) {
            return;
        }
        let lines: Vec<Selection> = selections
            .iter()
            .map(|selection| {
                let line = self.buffer.byte_to_line(selection.head);
                Selection {
                    anchor: self.buffer.line_to_byte(line),
                    head: self.buffer.line_to_byte(line + 1),
                }
            })
            .collect();
        self.history.break_group();
        self.set_selections(&lines);
    }

    /// The selected text of every cursor in document order, for the clipboard: one per line,
    /// unless a selection already ends its line. `None` when nothing is selected.
    pub fn copied_text(&self) -> Option<String> {
        let mut selections = self.selections();
        selections.retain(|selection| !selection.is_empty());
        selections.sort_by_key(|selection| selection.range().start);
        let ending = self.buffer.line_ending().as_str();
        let mut text = String::new();
        for (index, selection) in selections.iter().enumerate() {
            if index > 0 && !text.ends_with('\n') {
                text.push_str(ending);
            }
            text.push_str(&self.buffer.text_in(selection.range()));
        }
        (!selections.is_empty()).then_some(text)
    }

    /// Cut: removes every selection and returns their text.
    pub fn cut(&mut self) -> Option<String> {
        if self.read_only {
            return None;
        }
        let text = self.copied_text()?;
        self.edit_each(EditKind::Other, |_, _, selection| {
            (!selection.is_empty()).then(|| (selection.range(), String::new()))
        });
        Some(text)
    }

    /// Whether the text differs from the file on disk.
    pub fn is_dirty(&self) -> bool {
        self.history.is_dirty()
    }

    /// Whether closing it loses something: the changes since the file was saved, or any text at
    /// all in a new document that has no file yet.
    pub fn has_unsaved(&self) -> bool {
        match self.path {
            Some(_) => self.is_dirty(),
            None => !self.buffer.is_empty(),
        }
    }

    /// Applies the save cleanup (`ted_core::cleanup_edits`) as one undo step, keeping the cursors
    /// on their lines.
    pub fn apply_cleanup(&mut self, edits: Vec<Edit>) {
        if edits.is_empty() || self.read_only {
            return;
        }
        let before = self.selections();
        let after: Vec<Selection> = before
            .iter()
            .map(|selection| Selection {
                anchor: keep_offset(selection.anchor, &edits),
                head: keep_offset(selection.head, &edits),
            })
            .collect();
        self.history.edit_many(
            &mut self.buffer,
            edits,
            EditKind::Other,
            before,
            after.clone(),
        );
        self.set_selections(&after);
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
        if let Some(selections) = self.history.undo(&mut self.buffer) {
            self.set_selections(&selections);
            self.scroll_to_cursor();
        }
    }

    pub fn redo(&mut self) {
        if let Some(selections) = self.history.redo(&mut self.buffer) {
            self.set_selections(&selections);
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

/// The first match of `query` from `from` to the end of `rope`, then from its start, that is not
/// one of the `selected` ranges (sorted) nor overlaps them. `None` when every match is selected.
/// It may read the whole text, so it runs on the background executor.
pub fn next_occurrence(
    rope: &Rope,
    query: &SearchQuery,
    from: usize,
    selected: &[Range<usize>],
) -> Option<Range<usize>> {
    let free = |found: &Range<usize>| {
        let index = selected.partition_point(|range| range.end <= found.start);
        selected
            .get(index)
            .is_none_or(|range| range.start >= found.end)
    };
    let mut result = None;
    for range in [from..rope.len(), 0..from] {
        query.find_in(rope, range, |found| {
            if free(&found) {
                result = Some(found);
                return false;
            }
            true
        });
        if result.is_some() {
            break;
        }
    }
    result
}

/// The edit that turns `old` (at `start`) into `new`, without the text both share at their start
/// and end, so the cursors outside the changed part keep their place.
fn trimmed_edit(start: usize, old: &str, new: &str) -> Edit {
    let prefix = old
        .char_indices()
        .zip(new.chars())
        .find(|((_, a), b)| a != b)
        .map_or(old.len().min(new.len()), |((index, _), _)| index);
    let (old_rest, new_rest) = (&old[prefix..], &new[prefix..]);
    let suffix = old_rest
        .chars()
        .rev()
        .zip(new_rest.chars().rev())
        .take_while(|(a, b)| a == b)
        .map(|(a, _)| a.len_utf8())
        .sum::<usize>();
    Edit {
        start: start + prefix,
        old_text: old_rest[..old_rest.len() - suffix].to_string(),
        new_text: new_rest[..new_rest.len() - suffix].to_string(),
    }
}

/// Where `offset` lands after `edits` (sorted, in the coordinates of the text before them): an
/// offset inside a replaced range goes to its start, one at or after its end moves with it. Text
/// inserted at the offset itself stays after it, so the save cleanup's final line break does not
/// move a cursor at the end.
fn keep_offset(offset: usize, edits: &[Edit]) -> usize {
    let mut shifted = offset as isize;
    for edit in edits {
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
}

/// As [`keep_offset`], but text inserted at the offset itself goes before it, so a cursor at the
/// start of a line moves with the indentation added there.
fn shift_offset(offset: usize, edits: &[Edit]) -> usize {
    let mut shifted = offset as isize;
    for edit in edits {
        let end = edit.start + edit.old_text.len();
        if edit.start > offset || (edit.start == offset && !edit.old_text.is_empty()) {
            break;
        }
        let removed_end = end.min(offset);
        shifted -= (removed_end - edit.start) as isize;
        if end <= offset {
            shifted += edit.new_text.len() as isize;
        }
    }
    shifted.max(0) as usize
}

/// The display column of byte `column` of `line`: characters before it, with tabs advancing to
/// the next multiple of the tab size.
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
        let size = tab_size();
        (display / size + 1) * size
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
    fn a_new_document_has_unsaved_text_only_when_it_has_text() {
        let mut state = editor("");
        assert!(!state.has_unsaved());
        state.insert("hi", EditKind::Typing);
        assert!(state.has_unsaved());
        state.backspace();
        state.backspace();
        // Typed and deleted: nothing to lose, although the history moved.
        assert!(state.is_dirty());
        assert!(!state.has_unsaved());
        state.path = Some("notes.txt".into());
        assert!(state.has_unsaved());
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

    fn cursors(state: &EditorState) -> Vec<usize> {
        let mut offsets: Vec<usize> = state
            .selections()
            .iter()
            .map(|selection| selection.head)
            .collect();
        offsets.sort_unstable();
        offsets
    }

    #[test]
    fn alt_click_adds_and_removes_cursors() {
        let mut state = editor("one\ntwo\nthree");
        at(&mut state, 0, 1);
        state.toggle_cursor(Position::new(1, 1));
        state.toggle_cursor(Position::new(2, 1));
        assert_eq!(state.cursor, Position::new(2, 1));
        assert_eq!(cursors(&state), vec![1, 5, 9]);
        // On a cursor again: it goes, and the primary one is never left without a cursor.
        state.toggle_cursor(Position::new(1, 1));
        assert_eq!(cursors(&state), vec![1, 9]);
        state.toggle_cursor(Position::new(2, 1));
        assert_eq!(cursors(&state), vec![1]);
        state.toggle_cursor(Position::new(0, 1));
        assert_eq!(cursors(&state), vec![1]);
        // A plain click drops the others.
        state.toggle_cursor(Position::new(1, 0));
        at(&mut state, 2, 0);
        assert_eq!(cursors(&state), vec![8]);
    }

    #[test]
    fn typing_edits_every_cursor_and_undoes_at_once() {
        let mut state = editor("ab\ncd\nef");
        at(&mut state, 0, 1);
        state.toggle_cursor(Position::new(1, 1));
        state.toggle_cursor(Position::new(2, 2));
        for ch in ["x", "y"] {
            state.insert(ch, EditKind::Typing);
        }
        assert_eq!(state.buffer.to_text(), "axyb\ncxyd\nefxy");
        state.backspace();
        assert_eq!(state.buffer.to_text(), "axb\ncxd\nefx");
        state.newline();
        assert_eq!(state.buffer.to_text(), "ax\nb\ncx\nd\nefx\n");
        state.undo();
        state.undo();
        state.undo();
        assert_eq!(state.buffer.to_text(), "ab\ncd\nef");
        assert_eq!(cursors(&state), vec![1, 4, 8]);
        assert_eq!(state.cursor, Position::new(2, 2));
    }

    #[test]
    fn cursors_that_meet_merge() {
        let mut state = editor("abc");
        at(&mut state, 0, 1);
        state.toggle_cursor(Position::new(0, 2));
        state.move_cursor(Movement::Left, false);
        assert_eq!(cursors(&state), vec![0, 1]);
        state.move_cursor(Movement::Left, false);
        assert_eq!(cursors(&state), vec![0]);
        assert!(state.others.is_empty());
    }

    #[test]
    fn the_column_selection_spans_lines_and_clamps_to_short_ones() {
        let mut state = editor("abcdef\nab\nabcdef\n\tx");
        state.select_columns((0, 1), (3, 4));
        let selected: Vec<String> = {
            let mut selections = state.selections();
            selections.sort_by_key(|selection| selection.range().start);
            selections
                .iter()
                .map(|selection| state.buffer.text_in(selection.range()))
                .collect()
        };
        // The tab advances to column 4, so the last line has nothing between columns 1 and 4
        // but the tab itself.
        assert_eq!(selected, vec!["bcd", "b", "bcd", "\t"]);
        assert_eq!(state.cursor.line, 3);
        // Upwards, from the end of a line: one cursor per line.
        state.select_columns((2, 6), (0, 6));
        assert_eq!(cursors(&state), vec![6, 9, 16]);
        assert_eq!(state.cursor, Position::new(0, 6));
        assert_eq!(state.others_on_lines(1..2).len(), 1);
        assert!(state.collapse_cursors());
        assert!(!state.collapse_cursors());
    }

    #[test]
    fn copy_and_paste_with_several_cursors() {
        let mut state = editor("one\ntwo\n");
        state.select_columns((0, 0), (1, 3));
        assert_eq!(state.copied_text().as_deref(), Some("one\ntwo"));
        // As many lines as cursors: one each.
        state.move_cursor(Movement::LineEnd, false);
        state.paste("1\n2");
        assert_eq!(state.buffer.to_text(), "one1\ntwo2\n");
        // Otherwise all of it at every cursor.
        state.paste("-");
        assert_eq!(state.buffer.to_text(), "one1-\ntwo2-\n");
        // Cut without a selection takes every cursor's line.
        state.select_lines_if_empty();
        assert_eq!(state.cut().as_deref(), Some("one1-\ntwo2-\n"));
        assert_eq!(state.buffer.to_text(), "");
    }

    #[test]
    fn tab_and_shift_tab_at_several_cursors() {
        let mut state = editor("a\n    b\n");
        at(&mut state, 0, 1);
        state.toggle_cursor(Position::new(1, 5));
        state.outdent();
        assert_eq!(state.buffer.to_text(), "a\nb\n");
        assert_eq!(cursors(&state), vec![1, 3]);
        state.indent();
        assert_eq!(state.buffer.to_text(), "a   \nb   \n");
    }

    /// Random typing, Backspace and Delete at random cursors against the same edits on a
    /// `String`, then undo everything back to the start.
    #[test]
    fn several_cursors_match_a_string_model() {
        let mut rng = fastrand::Rng::with_seed(20);
        let start = "fn main() {\n    let ñ = 1;\n}\n";
        let mut state = editor(start);
        for _ in 0..300 {
            let mut model = state.buffer.to_text();
            if rng.u8(..4) == 0 {
                at(&mut state, 0, 0);
                for _ in 0..rng.usize(1..4) {
                    let offset = model.floor_char_boundary(rng.usize(..=model.len()));
                    let position = state.buffer.offset_to_position(offset);
                    state.toggle_cursor(position);
                }
            }
            let mut offsets = cursors(&state);
            match rng.u8(..3) {
                0 => {
                    let text = ["x", "ñ", " ", "\n"][rng.usize(..4)];
                    state.insert(text, EditKind::Typing);
                    for &offset in offsets.iter().rev() {
                        model.insert_str(offset, text);
                    }
                }
                1 => {
                    state.backspace();
                    offsets.dedup();
                    for &offset in offsets.iter().rev() {
                        if offset > 0 {
                            let start = model.floor_char_boundary(offset - 1);
                            model.replace_range(start..offset, "");
                        }
                    }
                }
                _ => {
                    state.delete();
                    let mut last_end = usize::MAX;
                    for &offset in offsets.iter().rev() {
                        if offset < model.len() && offset < last_end {
                            let end = model.ceil_char_boundary(offset + 1);
                            model.replace_range(offset..end, "");
                            last_end = offset;
                        }
                    }
                }
            }
            assert_eq!(state.buffer.to_text(), model);
        }
        while state.history.can_undo() {
            state.undo();
        }
        assert_eq!(state.buffer.to_text(), start);
    }

    fn occurrence(state: &mut EditorState) -> bool {
        match state.occurrence_search() {
            None => true,
            Some((query, from, selected)) => {
                match next_occurrence(state.buffer.rope(), &query, from, &selected) {
                    Some(found) => {
                        state.add_occurrence(found);
                        true
                    }
                    None => false,
                }
            }
        }
    }

    #[test]
    fn alt_j_selects_the_word_then_its_next_occurrences() {
        let mut state = editor("foo bar foo\nfoo");
        at(&mut state, 0, 9);
        assert!(occurrence(&mut state));
        assert_eq!(state.selected_text().as_deref(), Some("foo"));
        assert_eq!(state.others.len(), 0);
        // After the last one, the search wraps to the start.
        assert!(occurrence(&mut state));
        assert_eq!(state.cursor, Position::new(1, 3));
        assert!(occurrence(&mut state));
        assert_eq!(state.cursor, Position::new(0, 3));
        assert_eq!(cursors(&state), vec![3, 11, 15]);
        // Every occurrence is selected: nothing more.
        assert!(!occurrence(&mut state));
        // Typing replaces all of them.
        state.insert("x", EditKind::Typing);
        assert_eq!(state.buffer.to_text(), "x bar x\nx");
    }

    #[test]
    fn alt_shift_j_unselects_the_last_occurrence() {
        let mut state = editor("ab ab ab");
        state.set_selection(Selection { anchor: 0, head: 2 });
        occurrence(&mut state);
        occurrence(&mut state);
        assert_eq!(state.cursor, Position::new(0, 8));
        state.unselect_occurrence();
        assert_eq!(state.cursor, Position::new(0, 5));
        state.unselect_occurrence();
        assert_eq!(state.cursor, Position::new(0, 2));
        assert!(state.others.is_empty());
        state.unselect_occurrence();
        assert_eq!(state.selected_text().as_deref(), Some("ab"));
    }

    #[test]
    #[allow(clippy::expect_used)]
    fn occurrences_are_case_sensitive_and_skip_overlaps() {
        let rope = Rope::from_str("aaa Aa aa");
        let query = SearchQuery::new("aa", true).expect("a query");
        // "aa" at 1 overlaps the selected one at 0, and "Aa" is another case.
        assert_eq!(next_occurrence(&rope, &query, 1, &[0..2, 9..9]), Some(7..9));
        assert_eq!(next_occurrence(&rope, &query, 9, &[0..2, 7..9]), None);
    }

    #[test]
    fn ctrl_d_duplicates_the_line_or_the_selection() {
        let mut state = editor("one\r\ntwo");
        at(&mut state, 0, 2);
        state.duplicate();
        assert_eq!(state.buffer.to_text(), "one\r\none\r\ntwo");
        assert_eq!(state.cursor, Position::new(1, 2));
        // A selection in a line: the copy right after it, selected.
        state.set_selection(Selection { anchor: 6, head: 8 });
        state.duplicate();
        assert_eq!(state.buffer.to_text(), "one\r\nonene\r\ntwo");
        assert_eq!(state.selected_text().as_deref(), Some("ne"));
        assert_eq!(
            state.selection(),
            Selection {
                anchor: 8,
                head: 10
            }
        );
        // A selection over lines: the whole lines, the selection on the copy.
        let mut state = editor("a\nbc\nd");
        state.set_selection(Selection { anchor: 0, head: 3 });
        state.duplicate();
        assert_eq!(state.buffer.to_text(), "a\nbc\na\nbc\nd");
        assert_eq!(state.selection(), Selection { anchor: 5, head: 8 });
        state.undo();
        assert_eq!(state.buffer.to_text(), "a\nbc\nd");
    }

    #[test]
    fn ctrl_d_at_several_cursors_duplicates_each_line_once() {
        let mut state = editor("a\nb\n");
        at(&mut state, 0, 0);
        state.toggle_cursor(Position::new(0, 1));
        state.toggle_cursor(Position::new(1, 1));
        state.duplicate();
        assert_eq!(state.buffer.to_text(), "a\na\nb\nb\n");
    }

    #[test]
    fn ctrl_alt_d_deletes_whole_lines() {
        let mut state = editor("one\ntwo\nthree");
        at(&mut state, 0, 2);
        state.delete_lines();
        assert_eq!(state.buffer.to_text(), "two\nthree");
        assert_eq!(state.cursor, Position::new(0, 2));
        // The last line takes the line break before it.
        at(&mut state, 1, 4);
        state.delete_lines();
        assert_eq!(state.buffer.to_text(), "two");
        assert_eq!(state.cursor, Position::new(0, 3));
        state.delete_lines();
        assert_eq!(state.buffer.to_text(), "");
        state.undo();
        state.undo();
        state.undo();
        assert_eq!(state.buffer.to_text(), "one\ntwo\nthree");
    }

    #[test]
    fn ctrl_alt_d_at_several_cursors_and_selections() {
        let mut state = editor("1\n2\n3\n4\n5\n");
        // A selection over lines 1 and 2 that ends at the start of line 3, and a cursor on
        // line 4.
        state.set_selection(Selection { anchor: 2, head: 6 });
        state.toggle_cursor(Position::new(4, 1));
        state.delete_lines();
        assert_eq!(state.buffer.to_text(), "1\n4\n");
        assert_eq!(cursors(&state), vec![2, 4]);
    }
}
