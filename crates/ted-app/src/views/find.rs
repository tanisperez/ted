//! The find bar of an editor (`Ctrl+F`): a field, the match counter, the case toggle, previous
//! and next, and close. `Ctrl+H` adds a second row with the replacement, Replace and Replace all. It floats at the top right of the editor in the `overlay` look of the
//! design's floating toolbars. Enter and `F3` go to the next match, Shift+Enter and `Shift+F3` to
//! the previous one, Escape closes it and gives the focus back to the text.
//!
//! The bar is a sibling of the editor's key context, not inside it, so keys typed in its field
//! never reach the editor's bindings (Tab, the arrows). The state and the counting rules live in
//! `state::find`; the whole text is searched in the background, one [`STEP`] at a time.

use gpui_kit::assets::IconName;
use gpui_kit::component::input::{InputEvent, InputState};
use gpui_kit::component::tooltip::Tooltip;
use gpui_kit::component::*;
use gpui_kit::prelude::*;
use gpui_kit::*;
use rust_i18n::t;
use ted_core::Selection;
use telaio_ui::components::{IconButton, text_field};

use crate::state::find::STEP;
use crate::theme::PaletteExt;
use crate::theme::metrics::{FIND_BAR_FIELD_WIDTH, FIND_COUNT_WIDTH, RADIUS_MD};

use super::editor::{EditorView, KEY_CONTEXT};

actions!(
    find,
    [
        /// Opens the find bar, or focuses its field (`Ctrl+F`).
        OpenFind,
        /// Selects the next match (`F3`).
        FindNext,
        /// Selects the previous match (`Shift+F3`).
        FindPrevious,
        /// Closes the find bar (Escape in it).
        CloseFind,
        /// Opens the find bar with the replace row (`Ctrl+H`).
        OpenReplace,
    ]
);

/// The key context of the find bar.
const FIND_CONTEXT: &str = "FindBar";

/// Registers the find bar's key bindings, in the editor and in the bar.
pub fn bind_keys(cx: &mut App) {
    #[cfg(target_os = "macos")]
    let (open, replace) = ("cmd-f", "cmd-alt-f");
    #[cfg(not(target_os = "macos"))]
    let (open, replace) = ("ctrl-f", "ctrl-h");
    for context in [KEY_CONTEXT, FIND_CONTEXT] {
        cx.bind_keys([
            KeyBinding::new(open, OpenFind, Some(context)),
            KeyBinding::new(replace, OpenReplace, Some(context)),
            KeyBinding::new("f3", FindNext, Some(context)),
            KeyBinding::new("shift-f3", FindPrevious, Some(context)),
        ]);
    }
    // After gpui-kit's own Escape of the field, so this one wins inside the bar.
    cx.bind_keys([KeyBinding::new(
        "escape",
        CloseFind,
        Some("FindBar > Input"),
    )]);
}

impl EditorView {
    /// Wraps the editor with the find bar's actions and, when open, the bar on top of it.
    pub(crate) fn with_find_bar(
        &mut self,
        editor: impl IntoElement,
        cx: &mut Context<Self>,
    ) -> impl IntoElement {
        div()
            .relative()
            .size_full()
            .on_action(cx.listener(|this, _: &OpenFind, window, cx| {
                this.replace_open = false;
                this.open_find(window, cx)
            }))
            .on_action(
                cx.listener(|this, _: &OpenReplace, window, cx| this.open_replace(window, cx)),
            )
            .on_action(cx.listener(|this, _: &FindNext, _, cx| this.find_step(true, cx)))
            .on_action(cx.listener(|this, _: &FindPrevious, _, cx| this.find_step(false, cx)))
            .on_action(cx.listener(|this, _: &CloseFind, window, cx| this.close_find(window, cx)))
            .on_action(
                cx.listener(|this, _: &super::goto::OpenGoto, window, cx| {
                    this.open_goto(window, cx)
                }),
            )
            .child(editor)
            .when(self.find_open, |this| this.child(self.render_find_bar(cx)))
            .when(self.goto_open, |this| this.child(self.render_goto_bar(cx)))
    }

    /// Opens the bar with the selected text (when it is on one line) and focuses its field.
    fn open_find(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.goto_open = false;
        let selected = self
            .state
            .selected_text()
            .filter(|text| !text.contains('\n'));
        let input =
            match &self.find_input {
                Some(input) => input.clone(),
                None => {
                    let input = cx
                        .new(|cx| InputState::new(window, cx).placeholder(t!("find.placeholder")));
                    cx.subscribe_in(&input, window, |this, input, event: &InputEvent, _, cx| {
                        match event {
                            InputEvent::Change => {
                                this.find.text = input.read(cx).value().to_string();
                                this.find_jump = true;
                                this.search(cx);
                            }
                            InputEvent::PressEnter { shift, .. } => this.find_step(!shift, cx),
                            _ => {}
                        }
                    })
                    .detach();
                    self.find_input = Some(input.clone());
                    input
                }
            };
        input.update(cx, |input, cx| {
            if let Some(text) = selected {
                input.set_value(text, window, cx);
            }
            input.focus(window, cx);
            input.select_all(window, cx);
        });
        if !self.find_open {
            self.find_open = true;
            self.find.text = input.read(cx).value().to_string();
            self.search(cx);
        }
        cx.notify();
    }

    /// Opens the bar with its replace row, the focus in the find field as `Ctrl+F` leaves it.
    fn open_replace(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.replace_input.is_none() {
            let input = cx
                .new(|cx| InputState::new(window, cx).placeholder(t!("find.replace_placeholder")));
            cx.subscribe_in(&input, window, |this, _, event: &InputEvent, _, cx| {
                if let InputEvent::PressEnter { .. } = event {
                    this.replace_one(cx);
                }
            })
            .detach();
            self.replace_input = Some(input);
        }
        self.replace_open = true;
        self.open_find(window, cx);
    }

    /// The replace field's text.
    fn replacement(&self, cx: &App) -> String {
        self.replace_input
            .as_ref()
            .map(|input| input.read(cx).value().to_string())
            .unwrap_or_default()
    }

    /// Replace: when the selection is a match, puts the replacement in its place, then selects
    /// the next match; otherwise selects the next match first.
    fn replace_one(&mut self, cx: &mut Context<Self>) {
        if self.state.read_only {
            return;
        }
        let Some(query) = self.find.query.clone() else {
            return;
        };
        let selection = self.state.selection().range();
        let mut is_match = false;
        if !selection.is_empty() {
            query.find_in(self.state.buffer.rope(), selection.clone(), |found| {
                is_match = found == selection;
                false
            });
        }
        if is_match {
            let text = self.replacement(cx);
            self.state.insert(&text, ted_core::EditKind::Other);
            // The edit restarts the search on the next frame; its first match after the
            // cursor gets selected.
            self.find_jump = true;
            cx.notify();
        } else {
            self.find_step(true, cx);
        }
    }

    /// Replace all: every match of the whole text, searched in the background, replaced as one
    /// undo step if the text did not change meanwhile.
    fn replace_all(&mut self, cx: &mut Context<Self>) {
        if self.state.read_only {
            return;
        }
        let Some(query) = self.find.query.clone() else {
            return;
        };
        let text = self.replacement(cx);
        let rope = self.state.buffer.rope().clone();
        let version = self.state.buffer.version();
        cx.spawn(async move |this, cx| {
            let ranges = cx
                .background_spawn(async move {
                    let mut ranges = Vec::new();
                    query.find_in(&rope, 0..rope.len(), |found| {
                        ranges.push(found);
                        true
                    });
                    ranges
                })
                .await;
            let _ = this.update(cx, |editor, cx| {
                if editor.state.buffer.version() == version {
                    log::debug!("replacing {} matches", ranges.len());
                    editor.state.replace_ranges(&ranges, &text);
                    cx.notify();
                }
            });
        })
        .detach();
    }

    /// Closes the bar and gives the focus back to the text.
    fn close_find(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.find_open = false;
        // Stops a running search: its next step sees another generation.
        self.find.restart(self.state.buffer.version());
        self.focus_handle().clone().focus(window, cx);
        cx.notify();
    }

    /// Searches the whole text again, in the background, for the current query.
    pub(crate) fn search(&mut self, cx: &mut Context<Self>) {
        let generation = self.find.restart(self.state.buffer.version());
        cx.notify();
        let Some(query) = self.find.query.clone() else {
            return;
        };
        let rope = self.state.buffer.rope().clone();
        cx.spawn(async move |this, cx| {
            let started = std::time::Instant::now();
            let len = rope.len();
            let mut offset = 0;
            loop {
                let end = (offset + STEP).min(len);
                let (step_query, step_rope) = (query.clone(), rope.clone());
                let (matches, next) = cx
                    .background_spawn(async move {
                        let mut matches = Vec::new();
                        step_query.find_in(&step_rope, offset..end, |found| {
                            matches.push(found);
                            true
                        });
                        // A match may end past the step: the next one starts after it.
                        let next = matches.last().map_or(end, |last| last.end.max(end));
                        (matches, next)
                    })
                    .await;
                let done = next >= len;
                let alive = this.update(cx, |editor, cx| {
                    if editor.find.generation != generation {
                        return false;
                    }
                    editor.find.add(generation, matches, done);
                    if editor.find_jump {
                        editor.jump_to_match(cx);
                    }
                    cx.notify();
                    true
                });
                if !matches!(alive, Ok(true)) || done {
                    if done {
                        log::debug!("searched {len} bytes in {:?}", started.elapsed());
                    }
                    break;
                }
                offset = next;
            }
        })
        .detach();
    }

    /// Selects the first match at or after the start of the selection, once one is known.
    fn jump_to_match(&mut self, cx: &mut Context<Self>) {
        let from = self.state.selection().range().start;
        if let Some(found) = self.find.step(self.state.buffer.rope(), from, true) {
            self.find_jump = false;
            self.select_match(found, cx);
        }
    }

    /// Selects the next match after the selection, or the previous one before it.
    fn find_step(&mut self, forward: bool, cx: &mut Context<Self>) {
        if self.find.version != self.state.buffer.version() {
            return;
        }
        let range = self.state.selection().range();
        let from = if forward {
            // Past the selected match, not onto it again.
            if range.is_empty() {
                range.start
            } else {
                range.start + 1
            }
        } else {
            range.start
        };
        if let Some(found) = self.find.step(self.state.buffer.rope(), from, forward) {
            self.find_jump = false;
            self.select_match(found, cx);
        }
    }

    fn select_match(&mut self, found: std::ops::Range<usize>, cx: &mut Context<Self>) {
        self.state.set_selection(Selection {
            anchor: found.start,
            head: found.end,
        });
        self.state.scroll_to_cursor();
        cx.notify();
    }

    /// The bar: field, counter, case toggle, previous, next and close.
    fn render_find_bar(&self, cx: &mut Context<Self>) -> impl IntoElement {
        let palette = cx.palette();
        let Some(input) = &self.find_input else {
            return div().into_any_element();
        };
        let selection = self.state.selection().range();
        let label = self.find.label(Some(&selection));
        let case = self.find.case_sensitive;
        let (hover, accent_subtle) = (palette.hover, palette.accent_subtle);
        let find_row = h_flex()
            .items_center()
            .gap(px(4.0))
            .child(
                div()
                    .w(px(FIND_BAR_FIELD_WIDTH))
                    .child(text_field(input, cx)),
            )
            .child(
                div()
                    .w(px(FIND_COUNT_WIDTH))
                    .text_size(px(12.0))
                    .text_color(palette.fg_muted)
                    .text_center()
                    .truncate()
                    .child(label),
            )
            .child(
                div()
                    .id("find-case")
                    .size(px(24.0))
                    .flex()
                    .items_center()
                    .justify_center()
                    .rounded(px(RADIUS_MD - 2.0))
                    .cursor_pointer()
                    .text_color(if case {
                        palette.accent_text
                    } else {
                        palette.fg_muted
                    })
                    .when(case, |toggle| toggle.bg(accent_subtle))
                    .when(!case, |toggle| toggle.hover(move |style| style.bg(hover)))
                    .tooltip(|window, cx| {
                        Tooltip::new(SharedString::from(t!("find.case"))).build(window, cx)
                    })
                    .child(Icon::new(IconName::CaseSensitive).small())
                    .on_click(cx.listener(|this, _, _, cx| {
                        this.find.case_sensitive = !this.find.case_sensitive;
                        this.find_jump = true;
                        this.search(cx);
                    })),
            )
            .child(
                IconButton::new("find-previous", IconName::ChevronUp)
                    .tooltip(t!("find.previous"))
                    .on_click(cx.listener(|this, _, _, cx| this.find_step(false, cx))),
            )
            .child(
                IconButton::new("find-next", IconName::ChevronDown)
                    .tooltip(t!("find.next"))
                    .on_click(cx.listener(|this, _, _, cx| this.find_step(true, cx))),
            )
            .child(
                IconButton::new("find-close", IconName::Close)
                    .tooltip(t!("find.close"))
                    .on_click(cx.listener(|this, _, window, cx| this.close_find(window, cx))),
            );
        let replace_row = self
            .replace_input
            .as_ref()
            .filter(|_| self.replace_open)
            .map(|input| {
                let editable = !self.state.read_only;
                h_flex()
                    .items_center()
                    .gap(px(4.0))
                    .child(
                        div()
                            .w(px(FIND_BAR_FIELD_WIDTH))
                            .child(text_field(input, cx)),
                    )
                    .child(div().w(px(FIND_COUNT_WIDTH)))
                    .child(
                        IconButton::new("find-replace", IconName::Replace)
                            .tooltip(t!("find.replace"))
                            .disabled(!editable)
                            .on_click(cx.listener(|this, _, _, cx| this.replace_one(cx))),
                    )
                    .child(
                        IconButton::new("find-replace-all", IconName::ReplaceAll)
                            .tooltip(t!("find.replace_all"))
                            .disabled(!editable)
                            .on_click(cx.listener(|this, _, _, cx| this.replace_all(cx))),
                    )
            });
        v_flex()
            .id("find-bar")
            .key_context(FIND_CONTEXT)
            .absolute()
            .top(px(10.0))
            .right(px(22.0))
            .occlude()
            .gap(px(4.0))
            .p(px(4.0))
            .bg(palette.overlay)
            .border_1()
            .border_color(palette.border)
            .rounded(px(RADIUS_MD))
            .shadow(palette.shadow.clone())
            .child(find_row)
            .children(replace_row)
            .into_any_element()
    }
}
