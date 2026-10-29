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

use gpui_kit::component::ActiveTheme;
use gpui_kit::prelude::*;
use gpui_kit::*;
use ted_core::Position;

use crate::state::editor::{
    EditorState, Movement, Viewport, column_for_expanded, expand_tabs, expanded_index,
};
use crate::theme::PaletteExt;
use crate::theme::metrics::{
    EDITOR_GUTTER_PADDING, EDITOR_GUTTER_RIGHT, EDITOR_SCROLLBAR_WIDTH, EDITOR_TEXT_PADDING,
};

/// Lines longer than this, in bytes, are shaped only around the visible columns.
const LONG_LINE: usize = 2_000;
/// Columns shaped on each side of the visible ones on a long line.
const LONG_LINE_MARGIN: usize = 64;
/// Smallest height of the scrollbar thumb, in pixels.
const MIN_THUMB: f32 = 24.0;
/// Width of the text cursor, in pixels.
const CURSOR_WIDTH: f32 = 2.0;

actions!(
    editor,
    [
        /// Moves the cursor one character left.
        MoveLeft,
        /// Moves the cursor one character right.
        MoveRight,
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
    ]
);

/// The key context of the editor, for its bindings.
const KEY_CONTEXT: &str = "Editor";

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
    ]);
    #[cfg(target_os = "macos")]
    cx.bind_keys([
        KeyBinding::new("cmd-up", MoveToDocumentStart, context),
        KeyBinding::new("cmd-down", MoveToDocumentEnd, context),
        KeyBinding::new("cmd-left", MoveToLineStart, context),
        KeyBinding::new("cmd-right", MoveToLineEnd, context),
    ]);
    #[cfg(not(target_os = "macos"))]
    cx.bind_keys([
        KeyBinding::new("ctrl-home", MoveToDocumentStart, context),
        KeyBinding::new("ctrl-end", MoveToDocumentEnd, context),
    ]);
}

/// One editor: a document, its cursor and scroll, and the keyboard focus.
pub struct EditorView {
    pub state: EditorState,
    focus_handle: FocusHandle,
    /// The lines painted in the last frame, for mouse hit testing.
    painted: Vec<PaintedLine>,
    /// While the scrollbar thumb is dragged: the distance from the thumb's top to the pointer.
    thumb_grab: Option<f32>,
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
        }
    }

    pub fn focus_handle(&self) -> &FocusHandle {
        &self.focus_handle
    }

    fn move_cursor(&mut self, movement: Movement, cx: &mut Context<Self>) {
        self.state.move_cursor(movement);
        cx.notify();
    }

    /// The document position under a point relative to the text area's top left corner.
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
        div()
            .id("editor")
            .key_context(KEY_CONTEXT)
            .track_focus(&self.focus_handle)
            .size_full()
            .on_action(
                cx.listener(|this, _: &MoveLeft, _, cx| this.move_cursor(Movement::Left, cx)),
            )
            .on_action(
                cx.listener(|this, _: &MoveRight, _, cx| this.move_cursor(Movement::Right, cx)),
            )
            .on_action(cx.listener(|this, _: &MoveUp, _, cx| this.move_cursor(Movement::Up, cx)))
            .on_action(
                cx.listener(|this, _: &MoveDown, _, cx| this.move_cursor(Movement::Down, cx)),
            )
            .on_action(cx.listener(|this, _: &MoveToLineStart, _, cx| {
                this.move_cursor(Movement::LineStart, cx)
            }))
            .on_action(
                cx.listener(|this, _: &MoveToLineEnd, _, cx| {
                    this.move_cursor(Movement::LineEnd, cx)
                }),
            )
            .on_action(
                cx.listener(|this, _: &PageUp, _, cx| this.move_cursor(Movement::PageUp, cx)),
            )
            .on_action(
                cx.listener(|this, _: &PageDown, _, cx| this.move_cursor(Movement::PageDown, cx)),
            )
            .on_action(cx.listener(|this, _: &MoveToDocumentStart, _, cx| {
                this.move_cursor(Movement::DocumentStart, cx)
            }))
            .on_action(cx.listener(|this, _: &MoveToDocumentEnd, _, cx| {
                this.move_cursor(Movement::DocumentEnd, cx)
            }))
            .child(EditorElement {
                editor: cx.entity(),
            })
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
    line_height: Pixels,
    /// The y of the first visible line, relative to the top of the element.
    first_y: Pixels,
    numbers: Vec<ShapedLine>,
    painted: Vec<PaintedLine>,
    cursor: Option<Bounds<Pixels>>,
    current_line: Option<Bounds<Pixels>>,
    text_hitbox: Hitbox,
    gutter_hitbox: Hitbox,
}

/// The vertical scrollbar.
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

        // Gutter: as many digits as the last line number, at least two.
        let started = std::time::Instant::now();
        let gutter_lines = self.editor.read(cx).state.gutter_lines();
        let digits = gutter_lines.to_string().len().max(2) as f32;
        let gutter_width = px(digits * f32::from(char_width).ceil() + EDITOR_GUTTER_PADDING);
        let gutter = Bounds::new(bounds.origin, size(gutter_width, bounds.size.height));
        let text_left = bounds.left() + gutter_width + px(EDITOR_TEXT_PADDING);
        let text = Bounds::from_corners(
            point(text_left, bounds.top()),
            point(bounds.right() - px(EDITOR_SCROLLBAR_WIDTH), bounds.bottom()),
        );

        let viewport = Viewport {
            width: f32::from(text.size.width),
            height: f32::from(bounds.size.height),
            line_height: f32::from(line_height),
            char_width: f32::from(char_width),
        };
        let focused = self.editor.read(cx).focus_handle.is_focused(window);

        self.editor.update(cx, |editor, _| {
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
                let run = TextRun {
                    len: display.len(),
                    font: font.clone(),
                    color: text_color,
                    background_color: None,
                    underline: None,
                    strikethrough: None,
                };
                let shaped = text_system.shape_line(display.into(), font_size, &[run], None);
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

            let cursor_line = state.cursor.line;
            let current_line = lines.contains(&cursor_line).then(|| {
                Bounds::new(
                    point(bounds.left(), row_top(cursor_line)),
                    size(bounds.size.width, line_height),
                )
            });
            let cursor = painted
                .iter()
                .find(|painted| painted.line == cursor_line && focused)
                .and_then(|painted| {
                    let column = state.cursor.column.checked_sub(painted.start_column)?;
                    let column = column.min(painted.text.len());
                    let x = painted
                        .shaped
                        .x_for_index(expanded_index(&painted.text, column));
                    Some(Bounds::new(
                        point(text_left + painted.x + x, row_top(cursor_line)),
                        size(px(CURSOR_WIDTH), line_height),
                    ))
                });

            let numbers = lines
                .clone()
                .map(|line| {
                    let label = SharedString::from((line + 1).to_string());
                    let color = if line == cursor_line {
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

            log::trace!(
                "editor prepaint: {} lines in {:?}",
                painted.len(),
                started.elapsed()
            );
            EditorLayout {
                gutter,
                text,
                scrollbar,
                line_height,
                first_y: px(first_y),
                numbers,
                painted,
                cursor,
                current_line,
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
        if let Some(current_line) = layout.current_line {
            window.paint_quad(fill(current_line, palette.hover));
        }

        let line_height = layout.line_height;
        let top = bounds.top() + layout.first_y;
        window.with_content_mask(
            Some(ContentMask {
                bounds: layout.text,
            }),
            |window| {
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
                if let Some(cursor) = layout.cursor {
                    window.paint_quad(fill(cursor, palette.accent));
                }
            },
        );

        // Line numbers, right aligned, never scrolled horizontally.
        let number_right = layout.gutter.right() - px(EDITOR_GUTTER_RIGHT);
        for (row, number) in layout.numbers.iter().enumerate() {
            let origin = point(number_right - number.width, top + line_height * row as f32);
            let _ = number.paint(origin, line_height, TextAlign::Left, None, window, cx);
        }

        if let Some(scrollbar) = layout.scrollbar {
            // The thumb is inset 2 px in its 10 px track, as the design's scrollbar.
            let thumb = Bounds::from_corners(
                scrollbar.thumb.origin + point(px(2.0), px(2.0)),
                scrollbar.thumb.bottom_right() - point(px(2.0), px(2.0)),
            );
            window.paint_quad(fill(thumb, palette.border_strong).corner_radii(px(3.0)));
        }

        self.editor.update(cx, |editor, _| {
            editor.painted = std::mem::take(&mut layout.painted);
        });

        self.register_mouse_handlers(bounds, layout, window);
    }
}

impl EditorElement {
    fn register_mouse_handlers(
        &self,
        bounds: Bounds<Pixels>,
        layout: &EditorLayout,
        window: &mut Window,
    ) {
        let line_height = layout.line_height;
        let text = layout.text;
        let scrollbar = layout.scrollbar;

        let editor = self.editor.clone();
        window.on_mouse_event(move |event: &ScrollWheelEvent, phase, _window, cx| {
            if !phase.bubble() || !bounds.contains(&event.position) {
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

        let editor = self.editor.clone();
        window.on_mouse_event(move |event: &MouseDownEvent, phase, window, cx| {
            if !phase.bubble() || event.button != MouseButton::Left {
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
            if !bounds.contains(&event.position) {
                return;
            }
            editor.update(cx, |editor, cx| {
                window.focus(&editor.focus_handle, cx);
                let local = point(
                    event.position.x - text.left(),
                    event.position.y - text.top(),
                );
                let position = editor.position_at(local);
                editor.state.set_cursor(position);
                cx.notify();
            });
            cx.stop_propagation();
        });

        let editor = self.editor.clone();
        window.on_mouse_event(move |event: &MouseMoveEvent, phase, _window, cx| {
            if !phase.bubble() {
                return;
            }
            let Some(scrollbar) = scrollbar else { return };
            editor.update(cx, |editor, cx| {
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
                editor.update(cx, |editor, _| editor.thumb_grab = None);
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
