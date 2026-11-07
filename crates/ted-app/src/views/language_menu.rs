//! The language menu of the status bar, as the `ted lenguajes` page of the design: a click on the
//! language of the document opens a small popover above it with a search field and every
//! language ted highlights ([`Language::ALL`]), the current one checked and its extensions at the
//! right. Choosing one highlights the document as that language until it is closed, whatever its
//! file name says (`Syntax::set_language`).
//!
//! The filter and the key moves are plain functions in `state::language_picker`. The rows are
//! rebuilt when the menu opens and when the query changes, never in render.

use gpui_kit::assets::IconName;
use gpui_kit::component::input::{Input, InputEvent, InputState};
use gpui_kit::component::popover::Popover;
use gpui_kit::component::*;
use gpui_kit::prelude::*;
use gpui_kit::*;
use rust_i18n::t;
use ted_syntax::Language;
use telaio_ui::components::ScrollbarOverlay;

use crate::state::language_picker::{self, Choice, Found, Move};
use crate::state::status::language_label;
use crate::theme::{Palette, PaletteExt};

use super::root::AppView;

/// Width of the popover.
const PANEL_WIDTH: f32 = 288.0;
/// Radius of the popover.
const PANEL_RADIUS: f32 = 10.0;
/// Space between the popover and the language button.
const PANEL_GAP: f32 = 8.0;
/// Text size of the rows and the field.
const TEXT_SIZE: f32 = 13.0;
/// Height of the search field.
const SEARCH_HEIGHT: f32 = 32.0;
/// Radius of the search field.
const SEARCH_RADIUS: f32 = 7.0;
/// Height of the list, whatever the number of rows.
const LIST_HEIGHT: f32 = 232.0;
/// Height of a row.
const ROW_HEIGHT: f32 = 28.0;
/// Radius of a highlighted row.
const ROW_RADIUS: f32 = 6.0;
/// Size of the search icon and of the check.
const ICON_SIZE: f32 = 14.0;
/// Text size of the extensions and of the footer.
const SMALL_TEXT_SIZE: f32 = 11.5;
/// Height of the footer with the keys.
const FOOTER_HEIGHT: f32 = 30.0;
/// Side of a key chip of the footer.
const KEY_SIZE: f32 = 16.0;

/// What the menu tells the window.
pub(crate) enum LanguageMenuEvent {
    /// A language was chosen. The menu has closed.
    Choose(Language),
    /// The menu opened or closed, so the status bar redraws its button.
    Toggled,
}

/// The state of the menu, created once per window.
pub(crate) struct LanguageMenu {
    input: Entity<InputState>,
    /// Every language, labelled in the UI language when the menu opened.
    choices: Vec<Choice>,
    current: Option<Language>,
    /// What the query finds, in order.
    found: Vec<Found>,
    /// The highlighted row, an index into `found`.
    highlight: usize,
    scroll: ScrollHandle,
    /// A row to scroll into view once the list has its layout (just opened).
    reveal: Option<usize>,
    /// What had the focus when the menu opened, focused again when it closes.
    previous_focus: Option<FocusHandle>,
    open: bool,
    _input_subscription: Subscription,
}

impl EventEmitter<LanguageMenuEvent> for LanguageMenu {}

impl LanguageMenu {
    fn new(window: &mut Window, cx: &mut Context<Self>) -> Self {
        let input =
            cx.new(|cx| InputState::new(window, cx).placeholder(t!("status.language_search")));
        let subscription = cx.subscribe_in(&input, window, |menu, input, event, _, cx| {
            if matches!(event, InputEvent::Change) {
                let query = input.read(cx).value().to_string();
                menu.found = language_picker::filter(&menu.choices, &query);
                menu.highlight = 0;
                menu.reveal = None;
                menu.scroll.scroll_to_item(0);
                cx.notify();
            }
        });
        Self {
            input,
            choices: Vec::new(),
            current: None,
            found: Vec::new(),
            highlight: 0,
            scroll: ScrollHandle::new(),
            reveal: None,
            previous_focus: None,
            open: false,
            _input_subscription: subscription,
        }
    }

    pub(crate) fn is_open(&self) -> bool {
        self.open
    }

    /// Opens the menu with an empty query and `current` highlighted, and focuses the field.
    fn open(&mut self, current: Option<Language>, window: &mut Window, cx: &mut Context<Self>) {
        self.choices = Language::ALL
            .into_iter()
            .map(|language| Choice::new(language, language_label(language)))
            .collect();
        self.current = current;
        self.previous_focus = window.focused(cx);
        let placeholder = t!("status.language_search");
        self.input.update(cx, |input, cx| {
            input.set_placeholder(placeholder, window, cx);
            input.set_value("", window, cx);
            input.focus(window, cx);
        });
        // After `set_value`, whose change event resets the list.
        self.found = language_picker::filter(&self.choices, "");
        self.highlight = current
            .and_then(|current| {
                self.found
                    .iter()
                    .position(|found| self.choices[found.index].language == current)
            })
            .unwrap_or(0);
        // Scrolled to in render, once the list has been laid out.
        self.reveal = Some(self.highlight);
        self.open = true;
        cx.emit(LanguageMenuEvent::Toggled);
        cx.notify();
    }

    /// Closes the menu and gives the focus back to what had it before (the editor).
    fn close(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.open {
            self.open = false;
            if let Some(previous) = self.previous_focus.take() {
                previous.focus(window, cx);
            }
            cx.emit(LanguageMenuEvent::Toggled);
            cx.notify();
        }
    }

    fn move_highlight(&mut self, step: Move, cx: &mut Context<Self>) {
        if let Some(highlight) = language_picker::moved(self.highlight, step, self.found.len()) {
            self.highlight = highlight;
            self.scroll.scroll_to_item(highlight);
            cx.notify();
        }
    }

    fn choose(&mut self, row: usize, window: &mut Window, cx: &mut Context<Self>) {
        let Some(language) = self
            .found
            .get(row)
            .map(|found| self.choices[found.index].language)
        else {
            return;
        };
        self.close(window, cx);
        cx.emit(LanguageMenuEvent::Choose(language));
    }

    /// The keys of the menu, handled before the field sees them.
    fn on_key_down(&mut self, event: &KeyDownEvent, window: &mut Window, cx: &mut Context<Self>) {
        let keystroke = &event.keystroke;
        let jump = keystroke.modifiers.control || keystroke.modifiers.platform;
        let step = match keystroke.key.as_str() {
            "up" => Move::Step(-1),
            "down" => Move::Step(1),
            "pageup" => Move::Page(-1),
            "pagedown" => Move::Page(1),
            "home" if jump => Move::First,
            "end" if jump => Move::Last,
            "enter" => {
                cx.stop_propagation();
                window.prevent_default();
                self.choose(self.highlight, window, cx);
                return;
            }
            "escape" => {
                cx.stop_propagation();
                self.close(window, cx);
                return;
            }
            _ => return,
        };
        cx.stop_propagation();
        window.prevent_default();
        self.move_highlight(step, cx);
    }
}

impl Render for LanguageMenu {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let palette = cx.palette();
        let focused = self.input.read(cx).focus_handle(cx).is_focused(window);
        if let Some(row) = self.reveal {
            if self.scroll.bounds_for_item(row).is_some() {
                self.reveal = None;
                self.scroll.scroll_to_item(row);
            }
            // One more frame: to lay the list out, or to apply the scroll.
            window.request_animation_frame();
        }
        div()
            .w(px(PANEL_WIDTH))
            .text_size(px(TEXT_SIZE))
            .capture_key_down(cx.listener(Self::on_key_down))
            // Clicking the panel must not move the focus out of the field.
            .on_mouse_down(MouseButton::Left, |_, window, _| window.prevent_default())
            .child(
                v_flex()
                    .w_full()
                    .bg(palette.overlay)
                    .border_1()
                    .border_color(palette.border)
                    .rounded(px(PANEL_RADIUS))
                    .shadow(palette.shadow.clone())
                    .overflow_hidden()
                    .child(self.render_search(focused, &palette))
                    .child(self.render_list(&palette, cx))
                    .child(render_footer(&palette)),
            )
    }
}

impl LanguageMenu {
    fn render_search(&self, focused: bool, palette: &Palette) -> impl IntoElement {
        div().pt(px(8.0)).px(px(8.0)).child(
            h_flex()
                .h(px(SEARCH_HEIGHT))
                .items_center()
                .gap(px(8.0))
                .px(px(10.0))
                .rounded(px(SEARCH_RADIUS))
                .bg(palette.raised)
                .border_1()
                .border_color(if focused {
                    palette.accent
                } else {
                    palette.border
                })
                .child(
                    Icon::new(IconName::Search)
                        .with_size(px(ICON_SIZE))
                        .text_color(palette.fg_muted),
                )
                .child(
                    div().flex_1().min_w_0().child(
                        Input::new(&self.input)
                            .appearance(false)
                            .p_0()
                            .text_size(px(TEXT_SIZE))
                            .text_color(palette.fg),
                    ),
                ),
        )
    }

    fn render_list(&self, palette: &Palette, cx: &mut Context<Self>) -> AnyElement {
        if self.found.is_empty() {
            let query = self.input.read(cx).value().trim().to_string();
            return div()
                .h(px(LIST_HEIGHT))
                .pt(px(28.0))
                .px(px(8.0))
                .text_center()
                .text_color(palette.fg_muted)
                .child(t!("status.language_empty", query = query).into_owned())
                .into_any_element();
        }
        let rows: Vec<AnyElement> = self
            .found
            .iter()
            .enumerate()
            .map(|(row, found)| self.render_row(row, found, palette, cx))
            .collect();
        div()
            .relative()
            .h(px(LIST_HEIGHT))
            .child(
                v_flex()
                    .id("language-menu-rows")
                    .size_full()
                    .overflow_y_scroll()
                    .track_scroll(&self.scroll)
                    .px(px(6.0))
                    .py(px(8.0))
                    .gap(px(1.0))
                    .children(rows),
            )
            .child(ScrollbarOverlay::new(&self.scroll))
            .into_any_element()
    }

    fn render_row(
        &self,
        row: usize,
        found: &Found,
        palette: &Palette,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let choice = &self.choices[found.index];
        let highlighted = row == self.highlight;
        let checked = Some(choice.language) == self.current;
        let label = StyledText::new(choice.label.clone()).with_highlights(
            found.matched.clone().map(|range| {
                (
                    range,
                    HighlightStyle {
                        color: Some(palette.accent_text),
                        font_weight: Some(FontWeight::SEMIBOLD),
                        ..HighlightStyle::default()
                    },
                )
            }),
        );
        h_flex()
            .id(("language-menu-row", row))
            .flex_none()
            .h(px(ROW_HEIGHT))
            .items_center()
            .gap(px(8.0))
            .pl(px(6.0))
            .pr(px(8.0))
            .rounded(px(ROW_RADIUS))
            .cursor_pointer()
            .text_color(palette.fg)
            .when(highlighted, |this| this.bg(palette.accent_subtle))
            .on_mouse_move(cx.listener(move |menu, _, _, cx| {
                if menu.highlight != row {
                    menu.highlight = row;
                    cx.notify();
                }
            }))
            .on_click(cx.listener(move |menu, _, window, cx| menu.choose(row, window, cx)))
            .child(div().w(px(ICON_SIZE)).flex_none().when(checked, |this| {
                this.child(
                    Icon::new(IconName::Check)
                        .with_size(px(ICON_SIZE))
                        .text_color(palette.accent_text),
                )
            }))
            .child(div().flex_1().min_w_0().truncate().child(label))
            .child(
                div()
                    .flex_none()
                    .text_size(px(SMALL_TEXT_SIZE))
                    .text_color(palette.fg_muted)
                    .child(choice.extensions.clone()),
            )
            .into_any_element()
    }
}

/// The footer: the keys of the menu.
fn render_footer(palette: &Palette) -> impl IntoElement {
    let hint = |keys: &[&'static str], meaning: String| {
        h_flex()
            .items_center()
            .gap(px(5.0))
            .children(keys.iter().map(|key| render_key(key, palette)))
            .child(meaning)
    };
    h_flex()
        .flex_none()
        .h(px(FOOTER_HEIGHT))
        .items_center()
        .gap(px(14.0))
        .px(px(12.0))
        .border_t_1()
        .border_color(palette.border)
        .text_size(px(SMALL_TEXT_SIZE))
        .text_color(palette.fg_muted)
        .child(hint(
            &["↑", "↓"],
            t!("status.language_hint_move").into_owned(),
        ))
        .child(hint(&["↵"], t!("status.language_hint_choose").into_owned()))
        .child(hint(
            &["Esc"],
            t!("status.language_hint_close").into_owned(),
        ))
}

/// A key chip of the footer.
fn render_key(key: &'static str, palette: &Palette) -> impl IntoElement {
    div()
        .flex()
        .items_center()
        .justify_center()
        .min_w(px(KEY_SIZE))
        .h(px(KEY_SIZE))
        .px(px(4.0))
        .border_1()
        .border_color(palette.border_strong)
        .rounded(px(4.0))
        .text_size(px(10.5))
        .line_height(px(KEY_SIZE - 2.0))
        .child(key)
}

/// The popover around the language button, open while the menu is.
pub(crate) fn language_menu_popover(
    menu: &Entity<LanguageMenu>,
    current: Option<Language>,
    trigger: impl Selectable + IntoElement + 'static,
    cx: &App,
) -> impl IntoElement {
    let (open, focus) = {
        let state = menu.read(cx);
        (state.is_open(), state.input.read(cx).focus_handle(cx))
    };
    let changed = menu.clone();
    let content = menu.clone();
    Popover::new("status-language-menu")
        .anchor(Anchor::BottomRight)
        .offset(px(PANEL_GAP))
        .appearance(false)
        .open(open)
        .track_focus(&focus)
        .on_open_change(move |open, window, cx| {
            changed.update(cx, |menu, cx| {
                if *open {
                    menu.open(current, window, cx);
                } else {
                    menu.close(window, cx);
                }
            });
        })
        .content(move |_, _, _| content.clone())
        .trigger(trigger)
}

/// Builds the menu, once per window, and chooses the language it reports.
pub(crate) fn new_language_menu(
    window: &mut Window,
    cx: &mut Context<AppView>,
) -> Entity<LanguageMenu> {
    let menu = cx.new(|cx| LanguageMenu::new(window, cx));
    cx.subscribe(
        &menu,
        |view, _, event: &LanguageMenuEvent, cx| match event {
            LanguageMenuEvent::Choose(language) => view.set_document_language(*language, cx),
            LanguageMenuEvent::Toggled => cx.notify(),
        },
    )
    .detach();
    menu
}

impl AppView {
    /// Opens the language menu, as a click on the language of the status bar does (`Alt+L`).
    /// Only with a document in an editor, the one whose language the status bar shows, and
    /// never over a dialog.
    pub(crate) fn open_language_menu(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if window.has_active_dialog(cx) {
            return;
        }
        let Some(current) = self
            .editor()
            .map(|editor| editor.read(cx).state.syntax.language)
        else {
            return;
        };
        self.language_menu.update(cx, |menu, cx| {
            if !menu.is_open() {
                menu.open(Some(current), window, cx);
            }
        });
    }

    /// Highlights the active document as `language`.
    fn set_document_language(&mut self, language: Language, cx: &mut Context<Self>) {
        if let Some(editor) = self.editor() {
            editor.update(cx, |editor, cx| {
                log::debug!("highlighting the document as {}", language.name());
                editor.state.syntax.set_language(language);
                cx.notify();
            });
            cx.notify();
        }
    }
}
