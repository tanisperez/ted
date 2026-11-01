//! The editor: [`EditorView`], the entity that owns an [`EditorState`] and takes the keyboard, and
//! [`EditorElement`], ted's own gpui element that paints it.
//!
//! The element lays out and paints only the visible lines. Each one is shaped through gpui's text
//! system, whose line layout cache reuses the previous frame's shapes, so scrolling within the
//! same lines costs a hash per line. Lines longer than [`LONG_LINE`] bytes are shaped only around
//! the visible columns, assuming the editor font is monospaced, so a one line 1 MB JSON costs the
//! same as a short line.
//!
//! The cursor does not blink: a blink timer would repaint the window twice a second while idle,
//! which the performance rules forbid.
//!
//! Text comes in through gpui's input handler ([`EntityInputHandler`]), so typing, dead keys and
//! input methods (with their marked text) work as in any native field; editing keys (Enter, Tab,
//! Backspace, the clipboard, undo) are actions bound in the `Editor` key context.

use gpui_kit::component::ActiveTheme;
use gpui_kit::component::input::InputState;
use gpui_kit::component::menu::{ContextMenuExt as _, PopupMenu};
use gpui_kit::prelude::*;
use gpui_kit::*;
use rust_i18n::t;
use std::ops::Range;

use ted_core::{EditKind, Position, Selection};

use crate::state::editor::{
    EditorState, Movement, PADDING_TOP, Viewport, column_for_expanded, expand_tabs, expanded_index,
    next_occurrence,
};
use crate::state::find::Find;
use crate::state::git_gutter::MarkKind;
use crate::state::minimap::{
    EditorMeasures, LINE_HEIGHT as MINIMAP_LINE, MAX_COLUMNS as MINIMAP_COLUMNS, MinimapLayout,
    line_blocks,
};
use crate::state::syntax::{ParseRequest, line_runs};
use crate::theme::PaletteExt;
use crate::theme::metrics::{
    EDITOR_GUTTER_PADDING, EDITOR_GUTTER_RIGHT, EDITOR_SCROLLBAR_WIDTH, EDITOR_TEXT_PADDING,
};
use crate::theme::syntax::SyntaxColors;

/// Lines longer than this, in bytes, are shaped only around the visible columns.
const LONG_LINE: usize = 2_000;
/// Columns shaped on each side of the visible ones on a long line.
const LONG_LINE_MARGIN: usize = 64;
/// Smallest height of the scrollbar thumb, in pixels.
const MIN_THUMB: f32 = 24.0;
/// Width of the text cursor, in pixels.
const CURSOR_WIDTH: f32 = 2.0;
/// The x of the git change marks from the left of the gutter.
const GIT_MARK_LEFT: f32 = 4.0;
/// The width of a git change mark (and the height of a deletion mark).
const GIT_MARK_WIDTH: f32 = 3.0;

actions!(
    editor,
    [
        /// Moves the cursor one character left.
        MoveLeft,
        /// Moves the cursor one character right.
        MoveRight,
        /// Moves the cursor to the start of the previous word.
        MoveWordLeft,
        /// Moves the cursor to the end of the next word.
        MoveWordRight,
        /// Moves the cursor one line up.
        MoveUp,
        /// Moves the cursor one line down.
        MoveDown,
        /// Moves the cursor to the start of its line.
        MoveToLineStart,
        /// Moves the cursor to the end of its line.
        MoveToLineEnd,
        /// Moves the cursor and the view one page up.
        PageUp,
        /// Moves the cursor and the view one page down.
        PageDown,
        /// Moves the cursor to the start of the document.
        MoveToDocumentStart,
        /// Moves the cursor to the end of the document.
        MoveToDocumentEnd,
        /// Extends the selection one character left.
        SelectLeft,
        /// Extends the selection one character right.
        SelectRight,
        /// Extends the selection to the start of the previous word.
        SelectWordLeft,
        /// Extends the selection to the end of the next word.
        SelectWordRight,
        /// Extends the selection one line up.
        SelectUp,
        /// Extends the selection one line down.
        SelectDown,
        /// Extends the selection to the start of the line.
        SelectToLineStart,
        /// Extends the selection to the end of the line.
        SelectToLineEnd,
        /// Extends the selection one page up.
        SelectPageUp,
        /// Extends the selection one page down.
        SelectPageDown,
        /// Extends the selection to the start of the document.
        SelectToDocumentStart,
        /// Extends the selection to the end of the document.
        SelectToDocumentEnd,
        /// Selects the whole document.
        SelectAll,
        /// Deletes the selection or the character before the cursor.
        Backspace,
        /// Deletes the selection or the character after the cursor.
        Delete,
        /// Deletes the selection or back to the start of the previous word.
        DeleteWordLeft,
        /// Deletes the selection or up to the end of the next word.
        DeleteWordRight,
        /// Deletes the selection or back to the start of the line.
        DeleteToLineStart,
        /// Breaks the line, keeping its indentation.
        Newline,
        /// Indents the selected lines, or inserts indentation.
        Indent,
        /// Removes one indentation level from the selected lines.
        Outdent,
        /// Copies the selection, or the whole line without one.
        Copy,
        /// Cuts the selection, or the whole line without one.
        Cut,
        /// Pastes the clipboard's text.
        Paste,
        /// Undoes the last change.
        Undo,
        /// Redoes the last undone change.
        Redo,
        /// Keeps only the primary cursor.
        CollapseCursors,
        /// Selects the word at the cursor, or adds the next occurrence of the selection.
        SelectNextOccurrence,
        /// Removes the last occurrence added.
        UnselectOccurrence,
        /// Duplicates the line, or the selection.
        DuplicateLine,
        /// Deletes the whole line.
        DeleteLine,
        /// Moves the line one line up.
        MoveLineUp,
        /// Moves the line one line down.
        MoveLineDown,
        /// Selects the whole line, or adds the next line to a selection of whole lines.
        SelectLine,
        /// Inserts a line below the cursor's and moves there.
        InsertLineBelow,
        /// Inserts a line above the cursor's and moves there.
        InsertLineAbove,
        /// Indents the whole line, wherever the cursor is.
        IndentLine,
        /// Removes one indentation level from the line.
        OutdentLine,
        /// Comments out the lines, or uncomments them.
        ToggleComment,
        /// Scrolls one line up, the cursor stays.
        ScrollLineUp,
        /// Scrolls one line down, the cursor stays.
        ScrollLineDown,
    ]
);

/// Width of the minimap column, its left padding, and the narrowest editor that shows it.
const MINIMAP_WIDTH: f32 = 100.0;
const MINIMAP_PADDING: f32 = 6.0;
const MINIMAP_MIN_EDITOR_WIDTH: f32 = 480.0;
/// Height of a block in the minimap's 2 px lines, and how strong its color is.
const MINIMAP_BLOCK_HEIGHT: f32 = 1.5;
const MINIMAP_ALPHA: f32 = 0.6;
/// The most bytes the minimap's lines may span for one highlight query over all of them; past
/// it (very long lines) each line is queried on its own start.
const MINIMAP_MAX_RANGE: usize = 256 * 1024;

/// Alpha of the `warning` color behind the find bar's matches, and behind the selected one.
const MATCH_ALPHA: f32 = 0.22;
const CURRENT_MATCH_ALPHA: f32 = 0.5;

/// The key context of the editor, for its bindings.
pub(crate) const KEY_CONTEXT: &str = "Editor";

/// Registers the editor's key bindings.
pub fn bind_keys(cx: &mut App) {
    let context = Some(KEY_CONTEXT);
    cx.bind_keys([
        KeyBinding::new("left", MoveLeft, context),
        KeyBinding::new("right", MoveRight, context),
        KeyBinding::new("up", MoveUp, context),
        KeyBinding::new("down", MoveDown, context),
        KeyBinding::new("home", MoveToLineStart, context),
        KeyBinding::new("end", MoveToLineEnd, context),
        KeyBinding::new("pageup", PageUp, context),
        KeyBinding::new("pagedown", PageDown, context),
        KeyBinding::new("shift-left", SelectLeft, context),
        KeyBinding::new("shift-right", SelectRight, context),
        KeyBinding::new("shift-up", SelectUp, context),
        KeyBinding::new("shift-down", SelectDown, context),
        KeyBinding::new("shift-home", SelectToLineStart, context),
        KeyBinding::new("shift-end", SelectToLineEnd, context),
        KeyBinding::new("shift-pageup", SelectPageUp, context),
        KeyBinding::new("shift-pagedown", SelectPageDown, context),
        KeyBinding::new("backspace", Backspace, context),
        KeyBinding::new("shift-backspace", Backspace, context),
        KeyBinding::new("delete", Delete, context),
        KeyBinding::new("enter", Newline, context),
        KeyBinding::new("shift-enter", Newline, context),
        KeyBinding::new("tab", Indent, context),
        KeyBinding::new("shift-tab", Outdent, context),
        KeyBinding::new("escape", CollapseCursors, context),
    ]);
    #[cfg(target_os = "macos")]
    cx.bind_keys([
        KeyBinding::new("cmd-up", MoveToDocumentStart, context),
        KeyBinding::new("cmd-down", MoveToDocumentEnd, context),
        KeyBinding::new("cmd-left", MoveToLineStart, context),
        KeyBinding::new("cmd-right", MoveToLineEnd, context),
        KeyBinding::new("cmd-shift-up", SelectToDocumentStart, context),
        KeyBinding::new("cmd-shift-down", SelectToDocumentEnd, context),
        KeyBinding::new("cmd-shift-left", SelectToLineStart, context),
        KeyBinding::new("cmd-shift-right", SelectToLineEnd, context),
        KeyBinding::new("alt-left", MoveWordLeft, context),
        KeyBinding::new("alt-right", MoveWordRight, context),
        KeyBinding::new("alt-shift-left", SelectWordLeft, context),
        KeyBinding::new("alt-shift-right", SelectWordRight, context),
        KeyBinding::new("alt-backspace", DeleteWordLeft, context),
        KeyBinding::new("alt-delete", DeleteWordRight, context),
        KeyBinding::new("cmd-backspace", DeleteToLineStart, context),
        KeyBinding::new("cmd-l", SelectLine, context),
        KeyBinding::new("cmd-/", ToggleComment, context),
        KeyBinding::new("cmd-enter", InsertLineBelow, context),
        KeyBinding::new("cmd-shift-enter", InsertLineAbove, context),
        KeyBinding::new("cmd-]", IndentLine, context),
        KeyBinding::new("cmd-[", OutdentLine, context),
        KeyBinding::new("cmd-a", SelectAll, context),
        KeyBinding::new("cmd-c", Copy, context),
        KeyBinding::new("cmd-x", Cut, context),
        KeyBinding::new("cmd-v", Paste, context),
        KeyBinding::new("cmd-z", Undo, context),
        KeyBinding::new("cmd-shift-z", Redo, context),
        // Option+J types a character on macOS, so Control takes its place.
        KeyBinding::new("ctrl-j", SelectNextOccurrence, context),
        KeyBinding::new("ctrl-shift-j", UnselectOccurrence, context),
        KeyBinding::new("cmd-d", DuplicateLine, context),
        KeyBinding::new("cmd-alt-d", DeleteLine, context),
        KeyBinding::new("alt-up", MoveLineUp, context),
        KeyBinding::new("alt-down", MoveLineDown, context),
    ]);
    #[cfg(not(target_os = "macos"))]
    cx.bind_keys([
        KeyBinding::new("ctrl-home", MoveToDocumentStart, context),
        KeyBinding::new("ctrl-end", MoveToDocumentEnd, context),
        KeyBinding::new("ctrl-shift-home", SelectToDocumentStart, context),
        KeyBinding::new("ctrl-shift-end", SelectToDocumentEnd, context),
        KeyBinding::new("ctrl-left", MoveWordLeft, context),
        KeyBinding::new("ctrl-right", MoveWordRight, context),
        KeyBinding::new("ctrl-shift-left", SelectWordLeft, context),
        KeyBinding::new("ctrl-shift-right", SelectWordRight, context),
        KeyBinding::new("ctrl-backspace", DeleteWordLeft, context),
        KeyBinding::new("ctrl-delete", DeleteWordRight, context),
        KeyBinding::new("ctrl-l", SelectLine, context),
        KeyBinding::new("ctrl-/", ToggleComment, context),
        KeyBinding::new("ctrl-enter", InsertLineBelow, context),
        KeyBinding::new("ctrl-shift-enter", InsertLineAbove, context),
        KeyBinding::new("ctrl-]", IndentLine, context),
        KeyBinding::new("ctrl-[", OutdentLine, context),
        KeyBinding::new("ctrl-up", ScrollLineUp, context),
        KeyBinding::new("ctrl-down", ScrollLineDown, context),
        KeyBinding::new("ctrl-a", SelectAll, context),
        // The context menu shows the last binding of each action: the usual ones go last.
        KeyBinding::new("ctrl-insert", Copy, context),
        KeyBinding::new("ctrl-c", Copy, context),
        KeyBinding::new("shift-delete", Cut, context),
        KeyBinding::new("ctrl-x", Cut, context),
        KeyBinding::new("shift-insert", Paste, context),
        KeyBinding::new("ctrl-v", Paste, context),
        KeyBinding::new("ctrl-z", Undo, context),
        KeyBinding::new("ctrl-y", Redo, context),
        KeyBinding::new("ctrl-shift-z", Redo, context),
        KeyBinding::new("alt-j", SelectNextOccurrence, context),
        KeyBinding::new("alt-shift-j", UnselectOccurrence, context),
        KeyBinding::new("ctrl-d", DuplicateLine, context),
        KeyBinding::new("ctrl-alt-d", DeleteLine, context),
        KeyBinding::new("alt-up", MoveLineUp, context),
        KeyBinding::new("alt-down", MoveLineDown, context),
    ]);
}

/// The settings that change how every editor paints, read by the element on every frame. Set
/// with `cx.set_global` at startup and when Settings changes them.
#[derive(Clone, Copy)]
pub struct EditorDisplay {
    pub line_numbers: bool,
    pub current_line: bool,
    pub minimap: bool,
}

impl Default for EditorDisplay {
    fn default() -> Self {
        Self {
            line_numbers: true,
            current_line: true,
            minimap: true,
        }
    }
}

impl Global for EditorDisplay {}

/// One editor: a document, its cursor and scroll, and the keyboard focus.
pub struct EditorView {
    pub state: EditorState,
    focus_handle: FocusHandle,
    /// The lines painted in the last frame, for mouse hit testing.
    painted: Vec<PaintedLine>,
    /// While the scrollbar thumb is dragged: the distance from the thumb's top to the pointer.
    thumb_grab: Option<f32>,
    /// While the horizontal scrollbar thumb is dragged: the distance from its left to the
    /// pointer.
    hthumb_grab: Option<f32>,
    /// While the minimap's slider is dragged.
    minimap_grab: Option<MinimapGrab>,
    /// While the mouse button that started a selection is down.
    selecting: bool,
    /// While Alt+J searches the next occurrence: further presses wait for it.
    searching_occurrence: bool,
    /// While the middle button is down: the line and display column where the column selection
    /// started.
    column_start: Option<(usize, usize)>,
    /// The text an input method is composing, as byte offsets: shown in the document, replaced
    /// as the composition changes and committed when it ends.
    marked: Option<Range<usize>>,
    /// The text area's top left corner and line height in the last frame, for the input method's
    /// candidate window.
    text_origin: Point<Pixels>,
    line_height: Pixels,
    /// The find bar's query and matches (`views/find.rs`).
    pub(crate) find: Find,
    /// Whether the find bar shows.
    pub(crate) find_open: bool,
    /// The find bar's field, made the first time it opens.
    pub(crate) find_input: Option<Entity<InputState>>,
    /// Whether the Go to line bar shows.
    pub(crate) goto_open: bool,
    /// The Go to line field, made the first time the bar opens.
    pub(crate) goto_input: Option<Entity<InputState>>,
    /// Select the first match after the cursor as soon as the search finds one (the query just
    /// changed).
    pub(crate) find_jump: bool,
}

/// A line as painted in the last frame.
struct PaintedLine {
    line: usize,
    /// The line's text, tabs not expanded (the window of it, for a long line).
    text: String,
    /// The byte column of the line where `text` starts (zero unless the line is long).
    start_column: usize,
    shaped: ShapedLine,
    /// The x of `shaped`'s first glyph, relative to the start of the text area, scroll included.
    x: Pixels,
}

impl EditorView {
    pub fn new(state: EditorState, cx: &mut Context<Self>) -> Self {
        Self {
            state,
            focus_handle: cx.focus_handle(),
            painted: Vec::new(),
            thumb_grab: None,
            hthumb_grab: None,
            minimap_grab: None,
            selecting: false,
            column_start: None,
            searching_occurrence: false,
            marked: None,
            text_origin: Point::default(),
            line_height: px(20.0),
            find: Find::default(),
            find_open: false,
            find_input: None,
            goto_open: false,
            goto_input: None,
            find_jump: false,
        }
    }

    pub fn focus_handle(&self) -> &FocusHandle {
        &self.focus_handle
    }

    /// Runs a parse on the background executor and hands its tree to the state, which may ask
    /// for the next one (the text changed meanwhile).
    fn parse(&self, request: ParseRequest, cx: &mut Context<Self>) {
        cx.spawn(async move |this, cx| {
            let started = std::time::Instant::now();
            let tree = cx.background_spawn(async move { request.run() }).await;
            log::debug!("parsed in {:?}", started.elapsed());
            let _ = this.update(cx, |editor, cx| {
                let state = &mut editor.state;
                if let Some(next) = state.syntax.parsed(tree, &mut state.buffer) {
                    editor.parse(next, cx);
                }
                cx.notify();
            });
        })
        .detach();
    }

    fn move_cursor(&mut self, movement: Movement, select: bool, cx: &mut Context<Self>) {
        self.marked = None;
        self.state.move_cursor(movement, select);
        cx.notify();
    }

    /// Runs an edit on the state and repaints.
    fn edit(&mut self, cx: &mut Context<Self>, edit: impl FnOnce(&mut EditorState)) {
        self.marked = None;
        edit(&mut self.state);
        cx.notify();
    }

    fn copy(&mut self, cut: bool, cx: &mut Context<Self>) {
        self.state.select_lines_if_empty();
        let text = if cut {
            self.state.cut()
        } else {
            self.state.copied_text()
        };
        if let Some(text) = text {
            cx.write_to_clipboard(ClipboardItem::new_string(text));
        }
        cx.notify();
    }

    fn paste(&mut self, cx: &mut Context<Self>) {
        let Some(text) = cx.read_from_clipboard().and_then(|item| item.text()) else {
            return;
        };
        // Pasted lines take the document's line ending.
        let text = text.replace("\r\n", "\n");
        let text = text.replace('\n', self.state.buffer.line_ending().as_str());
        self.edit(cx, |state| state.paste(&text));
    }

    /// Alt+J: selects the word at the cursor, or searches the next occurrence of the selection
    /// in the background and adds it if the text has not changed meanwhile.
    fn select_next_occurrence(&mut self, cx: &mut Context<Self>) {
        if self.searching_occurrence {
            return;
        }
        self.marked = None;
        let Some((query, from, selected)) = self.state.occurrence_search() else {
            cx.notify();
            return;
        };
        let rope = self.state.buffer.rope().clone();
        let version = self.state.buffer.version();
        self.searching_occurrence = true;
        cx.spawn(async move |this, cx| {
            let found = cx
                .background_spawn(async move { next_occurrence(&rope, &query, from, &selected) })
                .await;
            let _ = this.update(cx, |editor, cx| {
                editor.searching_occurrence = false;
                if let Some(found) = found
                    && editor.state.buffer.version() == version
                {
                    editor.state.add_occurrence(found);
                    cx.notify();
                }
            });
        })
        .detach();
    }

    /// A UTF-16 range of the platform input as a byte range of the buffer.
    fn bytes_from_utf16(&self, range: &Range<usize>) -> Range<usize> {
        let buffer = &self.state.buffer;
        buffer.utf16_to_byte(range.start)..buffer.utf16_to_byte(range.end)
    }

    fn utf16_from_bytes(&self, range: &Range<usize>) -> Range<usize> {
        let buffer = &self.state.buffer;
        buffer.byte_to_utf16(range.start)..buffer.byte_to_utf16(range.end)
    }

    /// The document position under a point relative to the text area's top left corner.
    /// The line and display column under a point relative to the text area's top left corner,
    /// for the column selection: past the end of a line the column keeps growing.
    fn column_at(&self, point: Point<Pixels>) -> (usize, usize) {
        let line = self.state.line_at_y(f32::from(point.y));
        let char_width = self.state.viewport.char_width.max(1.0);
        let column = ((f32::from(point.x) + self.state.scroll_x) / char_width).round();
        (line, column.max(0.0) as usize)
    }

    fn position_at(&self, point: Point<Pixels>) -> Position {
        let line = self.state.line_at_y(f32::from(point.y));
        let Some(painted) = self.painted.iter().find(|painted| painted.line == line) else {
            return Position::new(line, 0);
        };
        let index = painted.shaped.closest_index_for_x(point.x - painted.x);
        let column = painted.start_column + column_for_expanded(&painted.text, index);
        Position::new(line, column)
    }
}

impl Focusable for EditorView {
    fn focus_handle(&self, _cx: &App) -> FocusHandle {
        self.focus_handle.clone()
    }
}

impl Render for EditorView {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let editor = div()
            .id("editor")
            .key_context(KEY_CONTEXT)
            .track_focus(&self.focus_handle)
            .size_full()
            .on_action(
                cx.listener(|this, _: &MoveLeft, _, cx| {
                    this.move_cursor(Movement::Left, false, cx)
                }),
            )
            .on_action(cx.listener(|this, _: &MoveRight, _, cx| {
                this.move_cursor(Movement::Right, false, cx)
            }))
            .on_action(cx.listener(|this, _: &MoveWordLeft, _, cx| {
                this.move_cursor(Movement::WordLeft, false, cx)
            }))
            .on_action(cx.listener(|this, _: &MoveWordRight, _, cx| {
                this.move_cursor(Movement::WordRight, false, cx)
            }))
            .on_action(
                cx.listener(|this, _: &MoveUp, _, cx| this.move_cursor(Movement::Up, false, cx)),
            )
            .on_action(
                cx.listener(|this, _: &MoveDown, _, cx| {
                    this.move_cursor(Movement::Down, false, cx)
                }),
            )
            .on_action(cx.listener(|this, _: &MoveToLineStart, _, cx| {
                this.move_cursor(Movement::LineStart, false, cx)
            }))
            .on_action(cx.listener(|this, _: &MoveToLineEnd, _, cx| {
                this.move_cursor(Movement::LineEnd, false, cx)
            }))
            .on_action(
                cx.listener(|this, _: &PageUp, _, cx| {
                    this.move_cursor(Movement::PageUp, false, cx)
                }),
            )
            .on_action(cx.listener(|this, _: &PageDown, _, cx| {
                this.move_cursor(Movement::PageDown, false, cx)
            }))
            .on_action(cx.listener(|this, _: &MoveToDocumentStart, _, cx| {
                this.move_cursor(Movement::DocumentStart, false, cx)
            }))
            .on_action(cx.listener(|this, _: &MoveToDocumentEnd, _, cx| {
                this.move_cursor(Movement::DocumentEnd, false, cx)
            }))
            .on_action(
                cx.listener(|this, _: &SelectLeft, _, cx| {
                    this.move_cursor(Movement::Left, true, cx)
                }),
            )
            .on_action(cx.listener(|this, _: &SelectRight, _, cx| {
                this.move_cursor(Movement::Right, true, cx)
            }))
            .on_action(cx.listener(|this, _: &SelectWordLeft, _, cx| {
                this.move_cursor(Movement::WordLeft, true, cx)
            }))
            .on_action(cx.listener(|this, _: &SelectWordRight, _, cx| {
                this.move_cursor(Movement::WordRight, true, cx)
            }))
            .on_action(
                cx.listener(|this, _: &SelectUp, _, cx| this.move_cursor(Movement::Up, true, cx)),
            )
            .on_action(
                cx.listener(|this, _: &SelectDown, _, cx| {
                    this.move_cursor(Movement::Down, true, cx)
                }),
            )
            .on_action(cx.listener(|this, _: &SelectToLineStart, _, cx| {
                this.move_cursor(Movement::LineStart, true, cx)
            }))
            .on_action(cx.listener(|this, _: &SelectToLineEnd, _, cx| {
                this.move_cursor(Movement::LineEnd, true, cx)
            }))
            .on_action(cx.listener(|this, _: &SelectPageUp, _, cx| {
                this.move_cursor(Movement::PageUp, true, cx)
            }))
            .on_action(cx.listener(|this, _: &SelectPageDown, _, cx| {
                this.move_cursor(Movement::PageDown, true, cx)
            }))
            .on_action(cx.listener(|this, _: &SelectToDocumentStart, _, cx| {
                this.move_cursor(Movement::DocumentStart, true, cx)
            }))
            .on_action(cx.listener(|this, _: &SelectToDocumentEnd, _, cx| {
                this.move_cursor(Movement::DocumentEnd, true, cx)
            }))
            .on_action(
                cx.listener(|this, _: &SelectAll, _, cx| this.edit(cx, EditorState::select_all)),
            )
            .on_action(
                cx.listener(|this, _: &Backspace, _, cx| this.edit(cx, EditorState::backspace)),
            )
            .on_action(cx.listener(|this, _: &Delete, _, cx| this.edit(cx, EditorState::delete)))
            .on_action(cx.listener(|this, _: &DeleteWordLeft, _, cx| {
                this.edit(cx, EditorState::delete_word_backward)
            }))
            .on_action(cx.listener(|this, _: &DeleteWordRight, _, cx| {
                this.edit(cx, EditorState::delete_word_forward)
            }))
            .on_action(cx.listener(|this, _: &DeleteToLineStart, _, cx| {
                this.edit(cx, EditorState::delete_to_line_start)
            }))
            .on_action(cx.listener(|this, _: &Newline, _, cx| this.edit(cx, EditorState::newline)))
            .on_action(cx.listener(|this, _: &Indent, _, cx| this.edit(cx, EditorState::indent)))
            .on_action(cx.listener(|this, _: &Outdent, _, cx| this.edit(cx, EditorState::outdent)))
            .on_action(cx.listener(|this, _: &Copy, _, cx| this.copy(false, cx)))
            .on_action(cx.listener(|this, _: &Cut, _, cx| this.copy(true, cx)))
            .on_action(cx.listener(|this, _: &Paste, _, cx| this.paste(cx)))
            .on_action(cx.listener(|this, _: &Undo, _, cx| this.edit(cx, EditorState::undo)))
            .on_action(cx.listener(|this, _: &Redo, _, cx| this.edit(cx, EditorState::redo)))
            .on_action(
                cx.listener(|this, _: &SelectNextOccurrence, _, cx| {
                    this.select_next_occurrence(cx)
                }),
            )
            .on_action(cx.listener(|this, _: &UnselectOccurrence, _, cx| {
                this.edit(cx, EditorState::unselect_occurrence)
            }))
            .on_action(
                cx.listener(|this, _: &DuplicateLine, _, cx| this.edit(cx, EditorState::duplicate)),
            )
            .on_action(
                cx.listener(|this, _: &DeleteLine, _, cx| this.edit(cx, EditorState::delete_lines)),
            )
            .on_action(cx.listener(|this, _: &SelectLine, _, cx| {
                this.edit(cx, EditorState::select_lines_extending)
            }))
            .on_action(cx.listener(|this, _: &InsertLineBelow, _, cx| {
                this.edit(cx, |state| state.insert_line(false))
            }))
            .on_action(cx.listener(|this, _: &InsertLineAbove, _, cx| {
                this.edit(cx, |state| state.insert_line(true))
            }))
            .on_action(
                cx.listener(|this, _: &IndentLine, _, cx| this.edit(cx, EditorState::indent_lines)),
            )
            .on_action(
                cx.listener(|this, _: &OutdentLine, _, cx| this.edit(cx, EditorState::outdent)),
            )
            .on_action(cx.listener(|this, _: &ToggleComment, _, cx| {
                this.edit(cx, EditorState::toggle_comment)
            }))
            .on_action(cx.listener(|this, _: &ScrollLineUp, _, cx| {
                this.state.scroll_lines(-1.0);
                cx.notify();
            }))
            .on_action(cx.listener(|this, _: &ScrollLineDown, _, cx| {
                this.state.scroll_lines(1.0);
                cx.notify();
            }))
            .on_action(cx.listener(|this, _: &MoveLineUp, _, cx| {
                this.edit(cx, |state| state.move_lines(true))
            }))
            .on_action(cx.listener(|this, _: &MoveLineDown, _, cx| {
                this.edit(cx, |state| state.move_lines(false))
            }))
            .on_action(cx.listener(|this, _: &CollapseCursors, _, cx| {
                if this.state.collapse_cursors() {
                    cx.notify();
                } else {
                    cx.propagate();
                }
            }))
            .child(EditorElement {
                editor: cx.entity(),
            });
        let this = cx.entity().downgrade();
        let editor = editor.context_menu(move |menu, _, cx| match this.upgrade() {
            Some(editor) => edit_menu(menu, editor.read(cx)),
            None => menu,
        });
        self.with_find_bar(editor, cx)
    }
}

/// The right-click menu of the editor, with the rules of telaio's `edit_menu`: Cut and Copy need
/// a selection, the edits need a document that is not read only, Undo and Redo need history.
/// Its actions go to the editor, which shows their shortcuts.
fn edit_menu(menu: PopupMenu, editor: &EditorView) -> PopupMenu {
    let state = &editor.state;
    let editable = !state.read_only;
    let selected = state
        .selections()
        .iter()
        .any(|selection| !selection.is_empty());
    menu.action_context(editor.focus_handle.clone())
        .menu_with_disabled(
            t!("editor.menu.undo"),
            Box::new(Undo),
            !(editable && state.history.can_undo()),
        )
        .menu_with_disabled(
            t!("editor.menu.redo"),
            Box::new(Redo),
            !(editable && state.history.can_redo()),
        )
        .separator()
        .menu_with_disabled(
            t!("editor.menu.cut"),
            Box::new(Cut),
            !(editable && selected),
        )
        .menu_with_disabled(t!("editor.menu.copy"), Box::new(Copy), !selected)
        .menu_with_disabled(t!("editor.menu.paste"), Box::new(Paste), !editable)
        .separator()
        .menu(t!("editor.menu.select_all"), Box::new(SelectAll))
        .separator()
        .menu_with_disabled(
            t!("editor.menu.duplicate_line"),
            Box::new(DuplicateLine),
            !editable,
        )
        .menu_with_disabled(
            t!("editor.menu.delete_line"),
            Box::new(DeleteLine),
            !editable,
        )
}

impl EntityInputHandler for EditorView {
    fn text_for_range(
        &mut self,
        range: Range<usize>,
        adjusted_range: &mut Option<Range<usize>>,
        _window: &mut Window,
        _cx: &mut Context<Self>,
    ) -> Option<String> {
        let bytes = self.bytes_from_utf16(&range);
        *adjusted_range = Some(self.utf16_from_bytes(&bytes));
        Some(self.state.buffer.text_in(bytes))
    }

    fn selected_text_range(
        &mut self,
        _ignore_disabled_input: bool,
        _window: &mut Window,
        _cx: &mut Context<Self>,
    ) -> Option<UTF16Selection> {
        let selection = self.state.selection();
        Some(UTF16Selection {
            range: self.utf16_from_bytes(&selection.range()),
            reversed: selection.head < selection.anchor,
        })
    }

    fn marked_text_range(
        &self,
        _window: &mut Window,
        _cx: &mut Context<Self>,
    ) -> Option<Range<usize>> {
        self.marked
            .as_ref()
            .map(|range| self.utf16_from_bytes(range))
    }

    fn unmark_text(&mut self, _window: &mut Window, _cx: &mut Context<Self>) {
        self.marked = None;
    }

    fn replace_text_in_range(
        &mut self,
        range: Option<Range<usize>>,
        text: &str,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let range = range.map(|range| self.bytes_from_utf16(&range));
        let primary = self.state.selection().range();
        match range.or(self.marked.take()) {
            // Plain typing: at every cursor.
            None => self.state.insert(text, EditKind::Typing),
            Some(range) if range == primary => self.state.insert(text, EditKind::Typing),
            Some(range) => self.state.replace(range, text, EditKind::Typing),
        }
        cx.notify();
    }

    fn replace_and_mark_text_in_range(
        &mut self,
        range: Option<Range<usize>>,
        new_text: &str,
        new_selected_range: Option<Range<usize>>,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let range = range
            .map(|range| self.bytes_from_utf16(&range))
            .or(self.marked.clone())
            .unwrap_or_else(|| self.state.selection().range());
        let start = range.start;
        self.state.replace(range, new_text, EditKind::Typing);
        self.marked = (!new_text.is_empty()).then(|| start..start + new_text.len());
        // The selection inside the composition, in UTF-16 units of the new text.
        if let Some(selected) = new_selected_range {
            let to_byte = |utf16: usize| {
                let mut units = 0;
                for (index, ch) in new_text.char_indices() {
                    if units >= utf16 {
                        return index;
                    }
                    units += ch.len_utf16();
                }
                new_text.len()
            };
            self.state.set_selection(Selection {
                anchor: start + to_byte(selected.start),
                head: start + to_byte(selected.end),
            });
        }
        cx.notify();
    }

    fn bounds_for_range(
        &mut self,
        range_utf16: Range<usize>,
        _element_bounds: Bounds<Pixels>,
        _window: &mut Window,
        _cx: &mut Context<Self>,
    ) -> Option<Bounds<Pixels>> {
        let start = self.bytes_from_utf16(&range_utf16).start;
        let position = self.state.buffer.offset_to_position(start);
        let (lines, first_y) = self.state.visible_lines();
        if !lines.contains(&position.line) {
            return None;
        }
        let painted = self
            .painted
            .iter()
            .find(|painted| painted.line == position.line)?;
        let column = position.column.checked_sub(painted.start_column)?;
        let x = painted.shaped.x_for_index(expanded_index(
            &painted.text,
            column.min(painted.text.len()),
        ));
        let y = px(first_y) + self.line_height * (position.line - lines.start) as f32;
        Some(Bounds::new(
            self.text_origin + point(painted.x + x, y),
            size(px(CURSOR_WIDTH), self.line_height),
        ))
    }

    fn character_index_for_point(
        &mut self,
        point: Point<Pixels>,
        _window: &mut Window,
        _cx: &mut Context<Self>,
    ) -> Option<usize> {
        let position = self.position_at(point - self.text_origin);
        let offset = self.state.buffer.position_to_offset(position);
        Some(self.state.buffer.byte_to_utf16(offset))
    }
}

/// The element that paints an [`EditorView`].
pub struct EditorElement {
    editor: Entity<EditorView>,
}

impl IntoElement for EditorElement {
    type Element = Self;

    fn into_element(self) -> Self::Element {
        self
    }
}

/// What prepaint measured and shaped for paint.
pub struct EditorLayout {
    gutter: Bounds<Pixels>,
    text: Bounds<Pixels>,
    scrollbar: Option<Scrollbar>,
    /// The horizontal scrollbar, over the bottom of the text, while lines are wider than it.
    hscrollbar: Option<Scrollbar>,
    minimap: Option<Minimap>,
    minimap_hitbox: Option<Hitbox>,
    line_height: Pixels,
    /// The y of the first visible line, relative to the top of the element.
    first_y: Pixels,
    numbers: Vec<ShapedLine>,
    /// The git change marks of the visible lines, in the gutter.
    git_marks: Vec<(Bounds<Pixels>, Hsla)>,
    painted: Vec<PaintedLine>,
    cursors: Vec<Bounds<Pixels>>,
    current_lines: Vec<Bounds<Pixels>>,
    /// The selected parts of the visible lines.
    selection: Vec<Bounds<Pixels>>,
    /// The find bar's matches on the visible lines.
    matches: Vec<Bounds<Pixels>>,
    /// The selected match, painted stronger and over the selection.
    current_match: Vec<Bounds<Pixels>>,
    /// The whole element: the mouse acts on the editor only while this is hovered, so a modal
    /// or a menu drawn over it keeps its clicks and wheel.
    hitbox: Hitbox,
    text_hitbox: Hitbox,
    gutter_hitbox: Hitbox,
}

/// The minimap of a frame: where it is and its blocks with their colors.
pub struct Minimap {
    geometry: MinimapGeometry,
    blocks: Vec<(Bounds<Pixels>, Hsla)>,
}

/// What the mouse handlers need of the minimap.
#[derive(Clone, Copy)]
struct MinimapGeometry {
    area: Bounds<Pixels>,
    slider: Bounds<Pixels>,
    layout: MinimapLayout,
}

/// A drag of the minimap's slider: where it was grabbed and how far it travels.
#[derive(Clone, Copy)]
struct MinimapGrab {
    /// The distance from the slider's top to the pointer.
    grab: f64,
    layout: MinimapLayout,
    /// The minimap's top.
    top: Pixels,
}

/// A scrollbar, vertical or horizontal.
#[derive(Clone, Copy)]
struct Scrollbar {
    track: Bounds<Pixels>,
    thumb: Bounds<Pixels>,
}

impl Element for EditorElement {
    type RequestLayoutState = ();
    type PrepaintState = EditorLayout;

    fn id(&self) -> Option<ElementId> {
        None
    }

    fn source_location(&self) -> Option<&'static core::panic::Location<'static>> {
        None
    }

    fn request_layout(
        &mut self,
        _id: Option<&GlobalElementId>,
        _inspector_id: Option<&InspectorElementId>,
        window: &mut Window,
        cx: &mut App,
    ) -> (LayoutId, Self::RequestLayoutState) {
        let mut style = Style::default();
        style.size.width = relative(1.).into();
        style.size.height = relative(1.).into();
        (window.request_layout(style, [], cx), ())
    }

    fn prepaint(
        &mut self,
        _id: Option<&GlobalElementId>,
        _inspector_id: Option<&InspectorElementId>,
        bounds: Bounds<Pixels>,
        _request_layout: &mut Self::RequestLayoutState,
        window: &mut Window,
        cx: &mut App,
    ) -> Self::PrepaintState {
        let theme = cx.theme();
        let font = font(theme.mono_font_family.clone());
        let font_size = theme.mono_font_size;
        let palette = cx.palette();
        let line_height = (font_size * 1.6).round();
        let text_system = window.text_system().clone();
        let font_id = text_system.resolve_font(&font);
        let char_width = text_system
            .advance(font_id, font_size, '0')
            .map(|size| size.width)
            .unwrap_or(font_size * 0.6);

        // Gutter: as many digits as the last line number, at least two. Without line numbers,
        // only its right padding stays, so the text does not touch the edge.
        let started = std::time::Instant::now();
        let display = cx
            .try_global::<EditorDisplay>()
            .copied()
            .unwrap_or_default();
        let gutter_width = if display.line_numbers {
            let gutter_lines = self.editor.read(cx).state.gutter_lines();
            let digits = gutter_lines.to_string().len().max(2) as f32;
            px(digits * f32::from(char_width).ceil() + EDITOR_GUTTER_PADDING)
        } else {
            px(EDITOR_GUTTER_RIGHT)
        };
        let gutter = Bounds::new(bounds.origin, size(gutter_width, bounds.size.height));
        let text_left = bounds.left() + gutter_width + px(EDITOR_TEXT_PADDING);
        // The minimap sits between the text and the scrollbar, when the editor is wide enough.
        let text_right = bounds.right() - px(EDITOR_SCROLLBAR_WIDTH);
        let minimap_area = (display.minimap && bounds.size.width > px(MINIMAP_MIN_EDITOR_WIDTH))
            .then(|| {
                Bounds::from_corners(
                    point(text_right - px(MINIMAP_WIDTH), bounds.top()),
                    point(text_right, bounds.bottom()),
                )
            });
        let text = Bounds::from_corners(
            point(text_left, bounds.top()),
            point(
                minimap_area.map_or(text_right, |area| area.left()),
                bounds.bottom(),
            ),
        );

        let viewport = Viewport {
            width: f32::from(text.size.width),
            height: f32::from(bounds.size.height),
            line_height: f32::from(line_height),
            char_width: f32::from(char_width),
        };
        let focused = self.editor.read(cx).focus_handle.is_focused(window);
        let colors = SyntaxColors::new(&cx.theme().highlight_theme);

        self.editor.update(cx, |editor, cx| {
            let state = &mut editor.state;
            if let Some(request) = state.syntax.sync(&mut state.buffer) {
                editor.parse(request, cx);
            }
            // The text changed under an open find bar: count its matches again.
            if editor.find_open && editor.find.version != editor.state.buffer.version() {
                editor.search(cx);
            }
            let state = &mut editor.state;
            state.set_viewport(viewport);
            let (lines, first_y) = state.visible_lines();
            let scroll_x = px(state.scroll_x);
            let text_color = palette.fg;

            let mut painted = Vec::with_capacity(lines.len());
            let mut widest = state.widest_seen;
            for line in lines.clone() {
                let slice = state.buffer.line(line);
                let (start_column, line_text, start_display) = if slice.len() > LONG_LINE {
                    // A window of characters around the visible columns. Tabs count as one
                    // column here: long lines are data (minified JSON, logs), not indented code.
                    let first_visible = (state.scroll_x / viewport.char_width) as usize;
                    let start_char = first_visible.saturating_sub(LONG_LINE_MARGIN);
                    let visible = (viewport.width / viewport.char_width) as usize;
                    let total_chars = slice.len_chars();
                    let end_char = (first_visible + visible + LONG_LINE_MARGIN).min(total_chars);
                    let start_char = start_char.min(end_char);
                    widest = widest.max(total_chars);
                    let start = slice.char_to_byte_idx(start_char);
                    let end = slice.char_to_byte_idx(end_char);
                    (start, slice.slice(start..end).to_string(), start_char)
                } else {
                    (0, slice.to_string(), 0)
                };
                let display = expand_tabs(&line_text);
                widest = widest.max(start_display + display.chars().count());
                let spans = state.syntax.line_spans(
                    &state.buffer,
                    line,
                    start_column..start_column + line_text.len(),
                );
                let runs: Vec<TextRun> = line_runs(&line_text, &spans)
                    .into_iter()
                    .map(|(len, highlight)| TextRun {
                        len,
                        font: font.clone(),
                        color: highlight
                            .and_then(|highlight| colors.get(highlight))
                            .unwrap_or(text_color),
                        background_color: None,
                        underline: None,
                        strikethrough: None,
                    })
                    .collect();
                let shaped = text_system.shape_line(display.into(), font_size, &runs, None);
                painted.push(PaintedLine {
                    line,
                    text: line_text,
                    start_column,
                    shaped,
                    x: char_width * start_display as f32 - scroll_x,
                });
            }
            state.widest_seen = widest;

            let row_top = |line: usize| {
                bounds.top() + px(first_y) + line_height * (line - lines.start) as f32
            };

            // The cursors on the visible lines: the primary one and a binary search of the
            // others, so a column selection over the whole file costs the same per frame.
            let others = state.others_on_lines(lines.clone());
            let visible_cursors: Vec<Position> = std::iter::once(state.cursor)
                .chain(others.iter().map(|caret| caret.cursor))
                .filter(|cursor| lines.contains(&cursor.line))
                .collect();
            let mut cursor_lines: Vec<usize> =
                visible_cursors.iter().map(|cursor| cursor.line).collect();
            cursor_lines.sort_unstable();
            cursor_lines.dedup();
            let current_lines = cursor_lines
                .iter()
                .filter(|_| display.current_line)
                .map(|&line| {
                    Bounds::new(
                        point(bounds.left(), row_top(line)),
                        size(bounds.size.width, line_height),
                    )
                })
                .collect();
            let cursors = visible_cursors
                .iter()
                .filter(|_| focused)
                .filter_map(|cursor| {
                    let painted = painted.iter().find(|painted| painted.line == cursor.line)?;
                    let column = cursor.column.checked_sub(painted.start_column)?;
                    let column = column.min(painted.text.len());
                    let x = painted
                        .shaped
                        .x_for_index(expanded_index(&painted.text, column));
                    Some(Bounds::new(
                        point(text_left + painted.x + x, row_top(cursor.line)),
                        size(px(CURSOR_WIDTH), line_height),
                    ))
                })
                .collect();

            // The selection, one rectangle per visible line it covers. A line whose break is
            // selected shows one more character's width.
            let rects = |range: Range<usize>| {
                let start = state.buffer.offset_to_position(range.start);
                let end = state.buffer.offset_to_position(range.end);
                range_rects(&painted, start, end, char_width, |painted, left, right| {
                    let top = row_top(painted.line);
                    Bounds::from_corners(
                        point(text_left + left, top),
                        point(text_left + right, top + line_height),
                    )
                })
            };
            let selection = state.selection();
            let mut selection_rects = Vec::new();
            for caret in std::iter::once((state.cursor, state.anchor))
                .chain(others.iter().map(|caret| (caret.cursor, caret.anchor)))
            {
                if let (cursor, Some(anchor)) = caret {
                    let range = state.buffer.position_to_offset(anchor.min(cursor))
                        ..state.buffer.position_to_offset(anchor.max(cursor));
                    selection_rects.extend(rects(range));
                }
            }
            // The visible matches of the find bar, searched in the painted part of each line.
            let mut match_rects = Vec::new();
            let mut current_rects = Vec::new();
            let selected = selection.range();
            if let Some(query) = editor
                .find_open
                .then_some(editor.find.query.as_ref())
                .flatten()
            {
                for line in &painted {
                    let start = state.buffer.line_to_byte(line.line) + line.start_column;
                    query.find_in(
                        state.buffer.rope(),
                        start..start + line.text.len(),
                        |found| {
                            if found == selected {
                                current_rects.extend(rects(found));
                            } else {
                                match_rects.extend(rects(found));
                            }
                            true
                        },
                    );
                }
            }

            let numbers = lines
                .clone()
                .filter(|_| display.line_numbers)
                .map(|line| {
                    let label = SharedString::from((line + 1).to_string());
                    let color = if display.current_line && cursor_lines.binary_search(&line).is_ok()
                    {
                        palette.fg_muted
                    } else {
                        palette.fg_subtle
                    };
                    let run = TextRun {
                        len: label.len(),
                        font: font.clone(),
                        color,
                        background_color: None,
                        underline: None,
                        strikethrough: None,
                    };
                    text_system.shape_line(label, font_size, &[run], None)
                })
                .collect();

            // The git change marks, only while the text is the one they were computed for.
            let mut git_marks = Vec::new();
            if let Some(marks) = &state.git_marks
                && marks.version == state.buffer.version()
            {
                let x = gutter.left() + px(GIT_MARK_LEFT);
                for mark in marks.on_lines(lines.clone()) {
                    let (color, top, height) = match mark.kind {
                        MarkKind::Added | MarkKind::Modified => {
                            let first = mark.lines.start.max(lines.start);
                            let last = mark.lines.end.min(lines.end);
                            (
                                if mark.kind == MarkKind::Added {
                                    palette.success
                                } else {
                                    palette.accent
                                },
                                row_top(first),
                                line_height * last.saturating_sub(first) as f32,
                            )
                        }
                        MarkKind::Deleted => {
                            let line = mark.lines.start.clamp(lines.start, lines.end);
                            (
                                palette.danger,
                                row_top(line) - px(GIT_MARK_WIDTH / 2.0),
                                px(GIT_MARK_WIDTH),
                            )
                        }
                    };
                    // A deletion is a short horizontal bar between two lines.
                    let width = if mark.kind == MarkKind::Deleted {
                        GIT_MARK_WIDTH * 3.0
                    } else {
                        GIT_MARK_WIDTH
                    };
                    git_marks.push((Bounds::new(point(x, top), size(px(width), height)), color));
                }
            }

            let minimap = minimap_area.map(|area| {
                let layout = MinimapLayout::new(EditorMeasures {
                    lines: state.buffer.line_count(),
                    line_height: f64::from(viewport.line_height),
                    scroll_y: state.scroll_y,
                    max_scroll_y: state.max_scroll_y(),
                    viewport_height: f64::from(viewport.height),
                });
                let height = f64::from(area.size.height);
                let mut blocks = Vec::new();
                let lines = layout.lines(state.buffer.line_count(), height);
                let max_bytes = MINIMAP_COLUMNS * 4;
                let spans = state.syntax.lines_spans(
                    &state.buffer,
                    lines.clone(),
                    max_bytes,
                    MINIMAP_MAX_RANGE,
                );
                for (line, spans) in lines.zip(spans) {
                    let top = (line as f64 * MINIMAP_LINE - layout.scroll) as f32;
                    let slice = state.buffer.line(line);
                    let end = slice.floor_char_boundary(slice.len().min(max_bytes));
                    let text = slice.slice(..end).to_string();
                    for (columns, highlight) in line_blocks(&text, &spans) {
                        let color = highlight
                            .and_then(|highlight| colors.get(highlight))
                            .unwrap_or(text_color);
                        blocks.push((
                            Bounds::new(
                                point(
                                    area.left() + px(MINIMAP_PADDING + columns.start as f32),
                                    area.top() + px(top),
                                ),
                                size(px(columns.len() as f32), px(MINIMAP_BLOCK_HEIGHT)),
                            ),
                            color.opacity(MINIMAP_ALPHA),
                        ));
                    }
                }
                let slider = Bounds::new(
                    point(area.left(), area.top() + px(layout.slider_top as f32)),
                    size(area.size.width, px(layout.slider_height as f32)),
                );
                Minimap {
                    geometry: MinimapGeometry {
                        area,
                        slider,
                        layout,
                    },
                    blocks,
                }
            });

            let content_height = state.content_height();
            let scrollbar = (content_height > f64::from(viewport.height)).then(|| {
                let track = Bounds::from_corners(
                    point(bounds.right() - px(EDITOR_SCROLLBAR_WIDTH), bounds.top()),
                    bounds.bottom_right(),
                );
                let track_height = f32::from(track.size.height);
                let thumb_height = (f64::from(track_height) * f64::from(viewport.height)
                    / content_height)
                    .max(f64::from(MIN_THUMB)) as f32;
                let fraction = if state.max_scroll_y() > 0.0 {
                    (state.scroll_y / state.max_scroll_y()) as f32
                } else {
                    0.0
                };
                let thumb_top = (track_height - thumb_height) * fraction;
                let thumb = Bounds::new(
                    point(track.left(), track.top() + px(thumb_top)),
                    size(track.size.width, px(thumb_height)),
                );
                Scrollbar { track, thumb }
            });
            // The horizontal one overlays the bottom of the text, as wide as the text area.
            let content_width = state.content_width();
            let hscrollbar = (content_width > viewport.width).then(|| {
                let track = Bounds::from_corners(
                    point(text.left(), bounds.bottom() - px(EDITOR_SCROLLBAR_WIDTH)),
                    point(text.right(), bounds.bottom()),
                );
                let track_width = f32::from(track.size.width);
                let thumb_width =
                    (track_width * viewport.width / content_width).clamp(MIN_THUMB, track_width);
                let fraction = match state.max_scroll_x() {
                    max if max > 0.0 => state.scroll_x / max,
                    _ => 0.0,
                };
                let thumb = Bounds::new(
                    point(
                        track.left() + px((track_width - thumb_width) * fraction),
                        track.top(),
                    ),
                    size(px(thumb_width), track.size.height),
                );
                Scrollbar { track, thumb }
            });

            log::trace!(
                "editor prepaint: {} lines in {:?}",
                painted.len(),
                started.elapsed()
            );
            EditorLayout {
                gutter,
                text,
                scrollbar,
                hscrollbar,
                minimap_hitbox: minimap.as_ref().map(|minimap| {
                    window.insert_hitbox(minimap.geometry.area, HitboxBehavior::Normal)
                }),
                minimap,
                line_height,
                first_y: px(first_y),
                numbers,
                git_marks,
                painted,
                cursors,
                current_lines,
                selection: selection_rects,
                matches: match_rects,
                current_match: current_rects,
                hitbox: window.insert_hitbox(bounds, HitboxBehavior::Normal),
                text_hitbox: window.insert_hitbox(text, HitboxBehavior::Normal),
                gutter_hitbox: window.insert_hitbox(gutter, HitboxBehavior::Normal),
            }
        })
    }

    fn paint(
        &mut self,
        _id: Option<&GlobalElementId>,
        _inspector_id: Option<&InspectorElementId>,
        bounds: Bounds<Pixels>,
        _request_layout: &mut Self::RequestLayoutState,
        layout: &mut Self::PrepaintState,
        window: &mut Window,
        cx: &mut App,
    ) {
        let palette = cx.palette();
        window.set_cursor_style(CursorStyle::IBeam, &layout.text_hitbox);
        window.set_cursor_style(CursorStyle::Arrow, &layout.gutter_hitbox);

        window.paint_quad(fill(bounds, palette.bg));
        // A half visible row at the top or the bottom must not spill over the title bar or the
        // status bar, so everything per row is clipped to the editor.
        let editor_mask = ContentMask { bounds };
        window.with_content_mask(Some(editor_mask), |window| {
            for current_line in &layout.current_lines {
                window.paint_quad(fill(*current_line, palette.hover));
            }
        });

        let line_height = layout.line_height;
        let top = bounds.top() + layout.first_y;
        window.with_content_mask(
            Some(ContentMask {
                bounds: layout.text,
            }),
            |window| {
                for rect in &layout.matches {
                    window.paint_quad(fill(*rect, palette.warning.opacity(MATCH_ALPHA)));
                }
                for rect in &layout.selection {
                    window.paint_quad(fill(*rect, palette.accent_subtle));
                }
                for rect in &layout.current_match {
                    window.paint_quad(fill(*rect, palette.warning.opacity(CURRENT_MATCH_ALPHA)));
                }
                for (row, painted) in layout.painted.iter().enumerate() {
                    let origin = point(
                        layout.text.left() + painted.x,
                        top + line_height * row as f32,
                    );
                    let _ = painted.shaped.paint(
                        origin,
                        line_height,
                        TextAlign::Left,
                        None,
                        window,
                        cx,
                    );
                }
                for cursor in &layout.cursors {
                    window.paint_quad(fill(*cursor, palette.accent));
                }
            },
        );

        // Line numbers, right aligned, never scrolled horizontally.
        let number_right = layout.gutter.right() - px(EDITOR_GUTTER_RIGHT);
        window.with_content_mask(Some(editor_mask), |window| {
            for (rect, color) in &layout.git_marks {
                window.paint_quad(fill(*rect, *color));
            }
            for (row, number) in layout.numbers.iter().enumerate() {
                let origin = point(number_right - number.width, top + line_height * row as f32);
                let _ = number.paint(origin, line_height, TextAlign::Left, None, window, cx);
            }
        });

        if let Some(minimap) = &layout.minimap {
            if let Some(hitbox) = &layout.minimap_hitbox {
                window.set_cursor_style(CursorStyle::Arrow, hitbox);
            }
            let dragging = self.editor.read(cx).minimap_grab.is_some();
            window.with_content_mask(
                Some(ContentMask {
                    bounds: minimap.geometry.area,
                }),
                |window| {
                    for (rect, color) in &minimap.blocks {
                        window.paint_quad(fill(*rect, *color));
                    }
                    let slider = if dragging {
                        palette.pressed
                    } else {
                        palette.hover
                    };
                    window.paint_quad(fill(minimap.geometry.slider, slider));
                },
            );
        }

        // The thumbs are inset 2 px in their 10 px tracks, as the design's scrollbar.
        for scrollbar in layout.scrollbar.iter().chain(&layout.hscrollbar) {
            let thumb = Bounds::from_corners(
                scrollbar.thumb.origin + point(px(2.0), px(2.0)),
                scrollbar.thumb.bottom_right() - point(px(2.0), px(2.0)),
            );
            window.paint_quad(fill(thumb, palette.border_strong).corner_radii(px(3.0)));
        }

        let text_origin = layout.text.origin;
        self.editor.update(cx, |editor, _| {
            editor.painted = std::mem::take(&mut layout.painted);
            editor.text_origin = text_origin;
            editor.line_height = line_height;
        });
        let focus_handle = self.editor.read(cx).focus_handle.clone();
        window.handle_input(
            &focus_handle,
            ElementInputHandler::new(bounds, self.editor.clone()),
            cx,
        );

        self.register_mouse_handlers(layout, window);
    }
}

impl EditorElement {
    fn register_mouse_handlers(&self, layout: &EditorLayout, window: &mut Window) {
        let line_height = layout.line_height;
        let text = layout.text;
        let scrollbar = layout.scrollbar;
        let hscrollbar = layout.hscrollbar;
        let minimap = layout.minimap.as_ref().map(|minimap| minimap.geometry);
        let hitbox = layout.hitbox.clone();

        let editor = self.editor.clone();
        let wheel_hitbox = hitbox.clone();
        window.on_mouse_event(move |event: &ScrollWheelEvent, phase, window, cx| {
            if !phase.bubble() || !wheel_hitbox.is_hovered(window) {
                return;
            }
            let delta = event.delta.pixel_delta(line_height);
            let (mut dx, dy) = (f32::from(delta.x), f32::from(delta.y));
            // Shift turns a plain wheel into horizontal scrolling.
            let dy = if event.modifiers.shift && dx == 0.0 {
                dx = dy;
                0.0
            } else {
                dy
            };
            editor.update(cx, |editor, cx| {
                let before = (editor.state.scroll_x, editor.state.scroll_y);
                editor.state.scroll_by(-dx, -dy);
                if before != (editor.state.scroll_x, editor.state.scroll_y) {
                    cx.notify();
                }
            });
            cx.stop_propagation();
        });

        // A right click outside the selections moves the cursor there first, as other editors
        // do. The event goes on to the context menu, which opens on it.
        let editor = self.editor.clone();
        let right_hitbox = hitbox.clone();
        window.on_mouse_event(move |event: &MouseDownEvent, phase, window, cx| {
            if !phase.bubble()
                || event.button != MouseButton::Right
                || !right_hitbox.is_hovered(window)
                || !text.contains(&event.position)
            {
                return;
            }
            editor.update(cx, |editor, cx| {
                window.focus(&editor.focus_handle, cx);
                editor.marked = None;
                let position = editor.position_at(event.position - text.origin);
                let offset = editor.state.buffer.position_to_offset(position);
                if !editor.state.is_selected(offset) {
                    editor.state.set_cursor(position, false);
                }
                cx.notify();
            });
        });

        let editor = self.editor.clone();
        let middle_hitbox = hitbox.clone();
        window.on_mouse_event(move |event: &MouseDownEvent, phase, window, cx| {
            // The middle button starts a column selection, as in IntelliJ. It never pastes the
            // primary selection.
            if !phase.bubble()
                || event.button != MouseButton::Middle
                || !middle_hitbox.is_hovered(window)
                || !text.contains(&event.position)
            {
                return;
            }
            editor.update(cx, |editor, cx| {
                window.focus(&editor.focus_handle, cx);
                editor.marked = None;
                let start = editor.column_at(event.position - text.origin);
                editor.column_start = Some(start);
                editor.state.select_columns(start, start);
                cx.notify();
            });
            cx.stop_propagation();
        });

        let editor = self.editor.clone();
        window.on_mouse_event(move |event: &MouseDownEvent, phase, window, cx| {
            if !phase.bubble() || event.button != MouseButton::Left || !hitbox.is_hovered(window) {
                return;
            }
            if let Some(minimap) = minimap
                && minimap.area.contains(&event.position)
            {
                editor.update(cx, |editor, cx| {
                    if minimap.slider.contains(&event.position) {
                        editor.minimap_grab = Some(MinimapGrab {
                            grab: f64::from(event.position.y - minimap.slider.top()),
                            layout: minimap.layout,
                            top: minimap.area.top(),
                        });
                    } else {
                        // A click outside the slider centers that line.
                        let y = f64::from(event.position.y - minimap.area.top());
                        let line = minimap.layout.line_at(y);
                        let state = &mut editor.state;
                        let line_height = f64::from(state.viewport.line_height);
                        let top = f64::from(PADDING_TOP) + line as f64 * line_height;
                        let centered = top - f64::from(state.viewport.height) / 2.0;
                        state.scroll_y = centered.clamp(0.0, state.max_scroll_y());
                    }
                    cx.notify();
                });
                cx.stop_propagation();
                return;
            }
            if let Some(scrollbar) = scrollbar
                && scrollbar.track.contains(&event.position)
            {
                editor.update(cx, |editor, cx| {
                    let grab = if scrollbar.thumb.contains(&event.position) {
                        event.position.y - scrollbar.thumb.top()
                    } else {
                        scrollbar.thumb.size.height / 2.0
                    };
                    editor.thumb_grab = Some(f32::from(grab));
                    drag_thumb(editor, scrollbar, event.position.y);
                    cx.notify();
                });
                cx.stop_propagation();
                return;
            }
            if let Some(hscrollbar) = hscrollbar
                && hscrollbar.track.contains(&event.position)
            {
                editor.update(cx, |editor, cx| {
                    let grab = if hscrollbar.thumb.contains(&event.position) {
                        event.position.x - hscrollbar.thumb.left()
                    } else {
                        hscrollbar.thumb.size.width / 2.0
                    };
                    editor.hthumb_grab = Some(f32::from(grab));
                    drag_hthumb(editor, hscrollbar, event.position.x);
                    cx.notify();
                });
                cx.stop_propagation();
                return;
            }
            editor.update(cx, |editor, cx| {
                window.focus(&editor.focus_handle, cx);
                editor.marked = None;
                let local = point(
                    event.position.x - text.left(),
                    event.position.y - text.top(),
                );
                let position = editor.position_at(local);
                match event.click_count {
                    // Alt+click (Option+click on macOS) adds or removes a cursor.
                    1 if event.modifiers.alt && !event.modifiers.shift => {
                        editor.state.toggle_cursor(position);
                    }
                    2 => editor.state.select_word_at(position),
                    3 => editor.state.select_line(position.line),
                    _ => {
                        editor.state.set_cursor(position, event.modifiers.shift);
                        editor.selecting = true;
                    }
                }
                cx.notify();
            });
            cx.stop_propagation();
        });

        let editor = self.editor.clone();
        window.on_mouse_event(move |event: &MouseMoveEvent, phase, _window, cx| {
            if !phase.bubble() {
                return;
            }
            editor.update(cx, |editor, cx| {
                if let Some(start) = editor.column_start {
                    if event.pressed_button == Some(MouseButton::Middle) {
                        let end = editor.column_at(event.position - text.origin);
                        editor.state.select_columns(start, end);
                        editor.state.scroll_to_cursor();
                        cx.notify();
                    } else {
                        editor.column_start = None;
                    }
                    return;
                }
                if editor.selecting {
                    if event.pressed_button == Some(MouseButton::Left) {
                        let local = point(
                            event.position.x - text.left(),
                            event.position.y - text.top(),
                        );
                        let position = editor.position_at(local);
                        editor.state.set_cursor(position, true);
                        editor.state.scroll_to_cursor();
                        cx.notify();
                    } else {
                        editor.selecting = false;
                    }
                    return;
                }
                if let Some(grab) = editor.minimap_grab {
                    if event.pressed_button == Some(MouseButton::Left) {
                        let slider_top = f64::from(event.position.y - grab.top) - grab.grab;
                        let fraction = grab.layout.fraction_for_slider(slider_top);
                        editor.state.scroll_to_fraction(fraction);
                    } else {
                        editor.minimap_grab = None;
                    }
                    cx.notify();
                    return;
                }
                if let Some(hscrollbar) = hscrollbar
                    && editor.hthumb_grab.is_some()
                {
                    if event.pressed_button == Some(MouseButton::Left) {
                        drag_hthumb(editor, hscrollbar, event.position.x);
                    } else {
                        editor.hthumb_grab = None;
                    }
                    cx.notify();
                    return;
                }
                let Some(scrollbar) = scrollbar else { return };
                if editor.thumb_grab.is_some() {
                    if event.pressed_button == Some(MouseButton::Left) {
                        drag_thumb(editor, scrollbar, event.position.y);
                    } else {
                        editor.thumb_grab = None;
                    }
                    cx.notify();
                }
            });
        });

        let editor = self.editor.clone();
        window.on_mouse_event(move |_: &MouseUpEvent, phase, _window, cx| {
            if phase.bubble() {
                editor.update(cx, |editor, cx| {
                    editor.thumb_grab = None;
                    editor.hthumb_grab = None;
                    editor.selecting = false;
                    editor.column_start = None;
                    if editor.minimap_grab.take().is_some() {
                        cx.notify();
                    }
                });
            }
        });
    }
}

/// Scrolls so the thumb's top sits at the pointer minus where it was grabbed.
fn drag_thumb(editor: &mut EditorView, scrollbar: Scrollbar, pointer_y: Pixels) {
    let grab = editor.thumb_grab.unwrap_or(0.0);
    let range = f32::from(scrollbar.track.size.height - scrollbar.thumb.size.height);
    if range <= 0.0 {
        return;
    }
    let top = f32::from(pointer_y - scrollbar.track.top()) - grab;
    editor.state.scroll_to_fraction(f64::from(top / range));
}

/// Scrolls so the horizontal thumb's left sits at the pointer minus where it was grabbed.
fn drag_hthumb(editor: &mut EditorView, scrollbar: Scrollbar, pointer_x: Pixels) {
    let grab = editor.hthumb_grab.unwrap_or(0.0);
    let range = f32::from(scrollbar.track.size.width - scrollbar.thumb.size.width);
    if range <= 0.0 {
        return;
    }
    let left = f32::from(pointer_x - scrollbar.track.left()) - grab;
    editor.state.scroll_to_fraction_x(left / range);
}

/// The rectangles that cover the text from `start` to `end` on the painted lines, one per line,
/// built by `rect` from a line and its left and right x. A line whose break is covered shows
/// one more character's width; an empty range shows half a character.
fn range_rects(
    painted: &[PaintedLine],
    start: Position,
    end: Position,
    char_width: Pixels,
    rect: impl Fn(&PaintedLine, Pixels, Pixels) -> Bounds<Pixels>,
) -> Vec<Bounds<Pixels>> {
    let mut rects = Vec::new();
    for line in painted {
        if line.line < start.line || line.line > end.line {
            continue;
        }
        let x_at = |column: usize| {
            let column = column.saturating_sub(line.start_column);
            let column = column.min(line.text.len());
            line.x + line.shaped.x_for_index(expanded_index(&line.text, column))
        };
        let left = if line.line == start.line {
            x_at(start.column)
        } else {
            line.x
        };
        let mut right = if line.line == end.line {
            x_at(end.column)
        } else {
            x_at(usize::MAX) + char_width
        };
        if right <= left {
            right = left + char_width * 0.5;
        }
        rects.push(rect(line, left, right));
    }
    rects
}
