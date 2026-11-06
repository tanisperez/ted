//! The find bar of an editor (`Ctrl+F`), from the 0.5.0 design: a full width bar under the tabs,
//! above the text. Its field holds the match case, whole word and regular expression toggles
//! (`Alt+C`, `Alt+W`, `Alt+X`); then the match counter, previous and next, find in selection and
//! close. The chevron on the left, `Ctrl+R` or `Ctrl+H` show the replace row: its field, with the
//! preserve case toggle, Replace and Replace all. Enter and `F3` go to the next match,
//! Shift+Enter and `Shift+F3` to the previous one, Escape closes the bar, from its field or from
//! the text, and gives the focus back to the text. The matches are marked on the scrollbar track
//! (`views/editor.rs`).
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
use telaio_ui::components::{IconButton, SecondaryButton};

use crate::state::find::STEP;
use crate::theme::PaletteExt;
use crate::theme::metrics::{
    FIND_BAR_BUTTON, FIND_BAR_FIELD_HEIGHT, FIND_BAR_FIELD_WIDTH, FIND_COUNT_WIDTH,
};

use super::editor::{EditorView, KEY_CONTEXT};
use super::field::Field;

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
        /// Opens the find bar with the replace row (`Ctrl+R`, `Ctrl+H`).
        OpenReplace,
        /// Match case (`Alt+C` in the bar).
        ToggleCase,
        /// Whole words only (`Alt+W` in the bar).
        ToggleWholeWord,
        /// Regular expression (`Alt+X` in the bar).
        ToggleRegex,
    ]
);

/// The key context of the find bar.
const FIND_CONTEXT: &str = "FindBar";

/// Registers the find bar's key bindings, in the editor and in the bar.
pub fn bind_keys(cx: &mut App) {
    #[cfg(target_os = "macos")]
    let (open, replace) = ("cmd-f", ["cmd-r", "cmd-alt-f"]);
    #[cfg(not(target_os = "macos"))]
    let (open, replace) = ("ctrl-f", ["ctrl-r", "ctrl-h"]);
    for context in [KEY_CONTEXT, FIND_CONTEXT] {
        cx.bind_keys([
            KeyBinding::new(open, OpenFind, Some(context)),
            KeyBinding::new(replace[0], OpenReplace, Some(context)),
            KeyBinding::new(replace[1], OpenReplace, Some(context)),
            KeyBinding::new("f3", FindNext, Some(context)),
            KeyBinding::new("shift-f3", FindPrevious, Some(context)),
        ]);
    }
    cx.bind_keys([
        KeyBinding::new("alt-c", ToggleCase, Some(FIND_CONTEXT)),
        KeyBinding::new("alt-w", ToggleWholeWord, Some(FIND_CONTEXT)),
        KeyBinding::new("alt-x", ToggleRegex, Some(FIND_CONTEXT)),
    ]);
    // After gpui-kit's own Escape of the field, so this one wins inside the bar.
    cx.bind_keys([KeyBinding::new(
        "escape",
        CloseFind,
        Some("FindBar > Input"),
    )]);
}

/// One of the toggles of the bar, as the design draws them: 24 x 22, `accent_subtle` when on.
pub(crate) fn toggle(
    id: &'static str,
    icon: IconName,
    tooltip: SharedString,
    on: bool,
    cx: &App,
) -> Stateful<Div> {
    let palette = cx.palette();
    let hover = palette.hover;
    div()
        .id(id)
        .flex_none()
        .w(px(24.0))
        .h(px(22.0))
        .flex()
        .items_center()
        .justify_center()
        .rounded(px(5.0))
        .cursor_pointer()
        .text_color(if on {
            palette.accent_text
        } else {
            palette.fg_muted
        })
        .when(on, |toggle| toggle.bg(palette.accent_subtle))
        .when(!on, |toggle| toggle.hover(move |style| style.bg(hover)))
        .tooltip(move |window, cx| Tooltip::new(tooltip.clone()).build(window, cx))
        .child(Icon::new(icon).size(px(14.0)))
}

impl EditorView {
    /// Wraps the editor with the find bar's actions and, when open, the bar above it. The Go to
    /// line dialog floats over the text.
    pub(crate) fn with_find_bar(
        &mut self,
        editor: impl IntoElement,
        cx: &mut Context<Self>,
    ) -> impl IntoElement {
        v_flex()
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
            .when(self.find_open, |this| this.child(self.render_find_bar(cx)))
            .child(
                div()
                    .relative()
                    .flex_1()
                    .min_h_0()
                    .w_full()
                    .child(editor)
                    .when(self.goto_open, |this| this.child(self.render_goto_bar(cx))),
            )
    }

    /// Opens the bar with the selected text (when it is on one line) and focuses its field. A
    /// selection over several lines turns on find in selection instead.
    fn open_find(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.goto_open = false;
        let selected = self.state.selected_text();
        let (one_line, several) = match selected {
            Some(text) if text.contains('\n') => (None, true),
            Some(text) => (Some(text), false),
            None => (None, false),
        };
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
            if let Some(text) = one_line {
                input.set_value(text, window, cx);
            }
            input.focus(window, cx);
            input.select_all(window, cx);
        });
        let scope = several.then(|| self.state.selection().range());
        if !self.find_open || scope.is_some() {
            if scope.is_some() {
                self.find.scope = scope;
            }
            self.find_open = true;
            self.find.text = input.read(cx).value().to_string();
            self.search(cx);
        }
        cx.notify();
    }

    /// Opens the bar with its replace row, the focus in the find field as `Ctrl+F` leaves it.
    fn open_replace(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.ensure_replace_input(window, cx);
        self.replace_open = true;
        self.open_find(window, cx);
    }

    /// Creates the replace field the first time the replace row shows.
    fn ensure_replace_input(&mut self, window: &mut Window, cx: &mut Context<Self>) {
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
    }

    /// The chevron: shows or hides the replace row.
    fn toggle_replace_row(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.replace_open {
            self.replace_open = false;
            cx.notify();
        } else {
            self.open_replace(window, cx);
        }
    }

    /// Flips one option of the query and searches again.
    fn toggle_option(
        &mut self,
        flip: impl FnOnce(&mut ted_core::SearchOptions),
        cx: &mut Context<Self>,
    ) {
        flip(&mut self.find.options);
        self.find_jump = true;
        self.search(cx);
    }

    /// Find in selection: limits the search to the selection, or to the whole text again.
    fn toggle_in_selection(&mut self, cx: &mut Context<Self>) {
        let selection = self.state.selection().range();
        self.find.scope = match self.find.scope {
            Some(_) => None,
            None if !selection.is_empty() => Some(selection),
            None => return,
        };
        self.search(cx);
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
            let matched = self.state.buffer.text_in(selection.clone());
            let text = query.replacement(&matched, &self.replacement(cx), self.find.preserve_case);
            if let Some(scope) = &mut self.find.scope {
                scope.end = (scope.end + text.len()).saturating_sub(matched.len());
            }
            self.state.insert(&text, ted_core::EditKind::Other);
            // The edit restarts the search on the next frame; its first match after the
            // cursor gets selected.
            self.find_jump = true;
            cx.notify();
        } else {
            self.find_step(true, cx);
        }
    }

    /// Replace all: every match of the text (or of the selection searched), searched in the
    /// background, replaced as one undo step if the text did not change meanwhile.
    fn replace_all(&mut self, cx: &mut Context<Self>) {
        if self.state.read_only {
            return;
        }
        let Some(query) = self.find.query.clone() else {
            return;
        };
        let text = self.replacement(cx);
        let preserve_case = self.find.preserve_case;
        let rope = self.state.buffer.rope().clone();
        let range = self.find.range(rope.len());
        let version = self.state.buffer.version();
        cx.spawn(async move |this, cx| {
            let (ranges, query) = cx
                .background_spawn(async move {
                    let mut ranges = Vec::new();
                    query.find_in(&rope, range, |found| {
                        ranges.push(found);
                        true
                    });
                    (ranges, query)
                })
                .await;
            let _ = this.update(cx, |editor, cx| {
                if editor.state.buffer.version() == version {
                    log::debug!("replacing {} matches", ranges.len());
                    let before = editor.state.buffer.len();
                    editor.state.replace_each(&ranges, |matched| {
                        query.replacement(matched, &text, preserve_case)
                    });
                    let after = editor.state.buffer.len();
                    if let Some(scope) = &mut editor.find.scope {
                        scope.end = (scope.end + after).saturating_sub(before);
                    }
                    cx.notify();
                }
            });
        })
        .detach();
    }

    /// Closes the bar and gives the focus back to the text.
    pub(crate) fn close_find(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.find_open = false;
        self.find.scope = None;
        // Stops a running search: its next step sees another generation.
        self.find.restart(self.state.buffer.version());
        self.focus_handle().clone().focus(window, cx);
        cx.notify();
    }

    /// Searches the text (or the selection searched) again, in the background, for the current
    /// query.
    pub(crate) fn search(&mut self, cx: &mut Context<Self>) {
        let generation = self.find.restart(self.state.buffer.version());
        cx.notify();
        let Some(query) = self.find.query.clone() else {
            return;
        };
        let rope = self.state.buffer.rope().clone();
        let range = self.find.range(rope.len());
        cx.spawn(async move |this, cx| {
            let started = std::time::Instant::now();
            let len = range.end;
            let mut offset = range.start;
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
                        log::debug!(
                            "searched {} bytes in {:?}",
                            len - range.start,
                            started.elapsed()
                        );
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

    /// The bar: the find row and, when shown, the replace row.
    fn render_find_bar(&self, cx: &mut Context<Self>) -> impl IntoElement {
        let palette = cx.palette();
        let Some(input) = &self.find_input else {
            return div().into_any_element();
        };
        let selection = self.state.selection().range();
        let label = self.find.label(Some(&selection));
        let options = self.find.options;
        let in_selection = self.find.scope.is_some();
        let (hover, accent_subtle) = (palette.hover, palette.accent_subtle);
        let chevron = div()
            .id("find-replace-toggle")
            .flex_none()
            .w(px(24.0))
            .h(px(28.0))
            .flex()
            .items_center()
            .justify_center()
            .rounded(px(6.0))
            .cursor_pointer()
            .text_color(palette.fg_muted)
            .hover(move |style| style.bg(hover))
            .tooltip(|window, cx| {
                Tooltip::new(SharedString::from(t!("find.toggle_replace"))).build(window, cx)
            })
            .child(
                Icon::new(if self.replace_open {
                    IconName::ChevronDown
                } else {
                    IconName::ChevronRight
                })
                .size(px(14.0)),
            )
            .on_click(cx.listener(|this, _, window, cx| this.toggle_replace_row(window, cx)));
        let field = Field::new(input, FIND_BAR_FIELD_HEIGHT, 12.5)
            .icon(IconName::Search)
            .suffix(
                toggle(
                    "find-case",
                    IconName::CaseSensitive,
                    t!("find.case").into(),
                    options.case_sensitive,
                    cx,
                )
                .on_click(cx.listener(|this, _, _, cx| {
                    this.toggle_option(|options| options.case_sensitive ^= true, cx)
                })),
            )
            .suffix(
                toggle(
                    "find-word",
                    IconName::WholeWord,
                    t!("find.whole_word").into(),
                    options.whole_word,
                    cx,
                )
                .on_click(cx.listener(|this, _, _, cx| {
                    this.toggle_option(|options| options.whole_word ^= true, cx)
                })),
            )
            .suffix(
                toggle(
                    "find-regex",
                    IconName::Regex,
                    t!("find.regex").into(),
                    options.regex,
                    cx,
                )
                .on_click(cx.listener(|this, _, _, cx| {
                    this.toggle_option(|options| options.regex ^= true, cx)
                })),
            );
        let invalid = self.find.invalid.is_some();
        let find_row = h_flex()
            .items_center()
            .gap(px(6.0))
            .child(chevron)
            .child(h_flex().w(px(FIND_BAR_FIELD_WIDTH)).child(field))
            .child(
                div()
                    .flex_none()
                    .min_w(px(FIND_COUNT_WIDTH))
                    .px(px(4.0))
                    .text_size(px(12.0))
                    .text_color(if invalid {
                        palette.danger
                    } else {
                        palette.fg_muted
                    })
                    .text_center()
                    .child(label),
            )
            .child(
                h_flex()
                    .gap(px(2.0))
                    .child(
                        IconButton::new("find-previous", IconName::ArrowUp)
                            .box_size(FIND_BAR_BUTTON)
                            .tooltip(t!("find.previous"))
                            .on_click(cx.listener(|this, _, _, cx| this.find_step(false, cx))),
                    )
                    .child(
                        IconButton::new("find-next", IconName::ArrowDown)
                            .box_size(FIND_BAR_BUTTON)
                            .tooltip(t!("find.next"))
                            .on_click(cx.listener(|this, _, _, cx| this.find_step(true, cx))),
                    )
                    .child(
                        div()
                            .id("find-in-selection")
                            .size(px(FIND_BAR_BUTTON))
                            .flex()
                            .items_center()
                            .justify_center()
                            .rounded(px(6.0))
                            .cursor_pointer()
                            .text_color(if in_selection {
                                palette.accent_text
                            } else {
                                palette.fg_muted
                            })
                            .when(in_selection, |button| button.bg(accent_subtle))
                            .when(!in_selection, |button| {
                                button.hover(move |style| style.bg(hover))
                            })
                            .tooltip(|window, cx| {
                                Tooltip::new(SharedString::from(t!("find.in_selection")))
                                    .build(window, cx)
                            })
                            .child(Icon::new(IconName::SquareDashedText).size(px(14.0)))
                            .on_click(cx.listener(|this, _, _, cx| this.toggle_in_selection(cx))),
                    ),
            )
            .child(div().flex_1())
            .child(
                IconButton::new("find-close", IconName::X)
                    .box_size(FIND_BAR_BUTTON)
                    .tooltip(t!("find.close"))
                    .on_click(cx.listener(|this, _, window, cx| this.close_find(window, cx))),
            );
        let replace_row = self
            .replace_input
            .as_ref()
            .filter(|_| self.replace_open)
            .map(|input| {
                let editable = !self.state.read_only;
                let preserve = self.find.preserve_case;
                let field = Field::new(input, FIND_BAR_FIELD_HEIGHT, 12.5)
                    .icon(IconName::Replace)
                    .suffix(
                        toggle(
                            "find-preserve-case",
                            IconName::CaseUpper,
                            t!("find.preserve_case").into(),
                            preserve,
                            cx,
                        )
                        .on_click(cx.listener(|this, _, _, cx| {
                            this.find.preserve_case = !this.find.preserve_case;
                            cx.notify();
                        })),
                    );
                h_flex()
                    .items_center()
                    .gap(px(6.0))
                    .pl(px(30.0))
                    .child(h_flex().w(px(FIND_BAR_FIELD_WIDTH)).child(field))
                    .child(div().flex_none().w(px(FIND_COUNT_WIDTH)))
                    .child(
                        SecondaryButton::new("find-replace", t!("find.replace"))
                            .height(FIND_BAR_BUTTON)
                            .disabled(!editable)
                            .on_click(cx.listener(|this, _, _, cx| this.replace_one(cx))),
                    )
                    .child(
                        SecondaryButton::new("find-replace-all", t!("find.replace_all"))
                            .height(FIND_BAR_BUTTON)
                            .disabled(!editable)
                            .on_click(cx.listener(|this, _, _, cx| this.replace_all(cx))),
                    )
            });
        v_flex()
            .id("find-bar")
            .key_context(FIND_CONTEXT)
            // A click on the bar keeps the focus where it is, in its field or the text: otherwise
            // it moves to the root of the window, where neither typing nor Escape reach the bar.
            .on_mouse_down(MouseButton::Left, |_, window, _| window.prevent_default())
            .flex_none()
            .w_full()
            .gap(px(6.0))
            .pt(px(8.0))
            .pb(px(8.0))
            .pl(px(8.0))
            .pr(px(12.0))
            .bg(palette.surface)
            .border_b_1()
            .border_color(palette.border)
            .on_action(cx.listener(|this, _: &ToggleCase, _, cx| {
                this.toggle_option(|options| options.case_sensitive ^= true, cx)
            }))
            .on_action(cx.listener(|this, _: &ToggleWholeWord, _, cx| {
                this.toggle_option(|options| options.whole_word ^= true, cx)
            }))
            .on_action(cx.listener(|this, _: &ToggleRegex, _, cx| {
                this.toggle_option(|options| options.regex ^= true, cx)
            }))
            .child(find_row)
            .children(replace_row)
            .into_any_element()
    }
}
