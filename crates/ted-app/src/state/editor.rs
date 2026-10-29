//! The state of one editor, outside the gpui element: the buffer, the cursor, the scroll offsets
//! and the viewport math (which lines are visible, how far the document scrolls). Plain Rust, so
//! it is unit tested without a window.
//!
//! The vertical scroll offset is an `f64`: a 100 MB log has more than a million lines, about 27
//! million pixels, and an `f32` loses whole pixels well before that. Everything handed to gpui is
//! relative to the first visible line, so it stays small.

use std::ops::Range;

use ted_core::{Buffer, Position};

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
    /// The file was not valid UTF-8 and was decoded lossily: saving would change its bytes.
    pub read_only: bool,
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

    /// Moves the cursor and scrolls it into view.
    pub fn move_cursor(&mut self, movement: Movement) {
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
        self.scroll_to_cursor();
    }

    /// Puts the cursor at `position` (a click), clamped to the text.
    pub fn set_cursor(&mut self, position: Position) {
        let offset = self.buffer.position_to_offset(position);
        self.cursor = self.buffer.offset_to_position(offset);
        self.preferred_column = None;
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

fn advance(display: usize, ch: char) -> usize {
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
        state.move_cursor(Movement::Right);
        assert_eq!(state.cursor, Position::new(0, 3));
        state.move_cursor(Movement::Right);
        state.move_cursor(Movement::Right);
        assert_eq!(state.cursor, Position::new(1, 0));
        state.move_cursor(Movement::Left);
        assert_eq!(state.cursor, Position::new(0, 4));
        state.move_cursor(Movement::Left);
        assert_eq!(state.cursor, Position::new(0, 3));
        state.move_cursor(Movement::Left);
        assert_eq!(state.cursor, Position::new(0, 1));
    }

    #[test]
    fn vertical_moves_keep_the_column() {
        let mut state = editor("long line\nab\nlonger line");
        state.cursor = Position::new(0, 7);
        state.move_cursor(Movement::Down);
        assert_eq!(state.cursor, Position::new(1, 2));
        state.move_cursor(Movement::Down);
        assert_eq!(state.cursor, Position::new(2, 7));
        state.move_cursor(Movement::Down);
        assert_eq!(state.cursor, Position::new(2, 11));
        state.move_cursor(Movement::Up);
        assert_eq!(state.cursor, Position::new(1, 2));
    }

    #[test]
    fn document_moves_scroll_the_cursor_into_view() {
        let mut state = editor(&"x\n".repeat(99));
        state.move_cursor(Movement::DocumentEnd);
        assert_eq!(state.cursor, Position::new(99, 0));
        // Line 99's bottom is 10 + 100 * 20 = 2010, at the bottom of a 200 px viewport.
        assert_eq!(state.scroll_y, 1810.0);
        state.move_cursor(Movement::DocumentStart);
        assert_eq!(state.scroll_y, 0.0);
    }

    #[test]
    fn page_moves_scroll_by_a_page() {
        let mut state = editor(&"x\n".repeat(99));
        state.move_cursor(Movement::PageDown);
        assert_eq!(state.cursor.line, 10);
        assert_eq!(state.scroll_y, 200.0);
        state.move_cursor(Movement::PageUp);
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
