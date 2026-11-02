//! The Go to line dialog of an editor (`Ctrl+G`), as in IntelliJ: a small window centered at the
//! top of the editor, with a title, one field that takes `line` or `line:column`, a hint and the
//! Cancel and Go buttons. The field opens with the cursor's place selected, so typing replaces
//! it. Enter (or Go) goes there and closes it, Escape (or Cancel) closes it; both give the focus
//! back to the text. It replaces the find bar while open. The parsing lives in `state::goto`.

use gpui_kit::assets::IconName;
use gpui_kit::component::input::{InputEvent, InputState};
use gpui_kit::component::*;
use gpui_kit::prelude::*;
use gpui_kit::*;
use rust_i18n::t;
use telaio_ui::components::{IconButton, PrimaryButton, SecondaryButton};

use crate::state::goto::parse_target;
use crate::theme::PaletteExt;
use crate::theme::metrics::{
    GOTO_BUTTON_HEIGHT, GOTO_FIELD_HEIGHT, GOTO_TOP, GOTO_WIDTH, RADIUS_DIALOG,
};

use super::editor::{EditorView, KEY_CONTEXT};
use super::field::Field;

actions!(
    goto,
    [
        /// Opens the Go to line dialog (`Ctrl+G`).
        OpenGoto,
        /// Closes it (Escape in it).
        CloseGoto,
    ]
);

/// The key context of the dialog.
const GOTO_CONTEXT: &str = "GotoBar";

/// Registers the dialog's key bindings.
pub fn bind_keys(cx: &mut App) {
    // Ctrl+G on every platform: Cmd+G is Find next on macOS.
    cx.bind_keys([KeyBinding::new("ctrl-g", OpenGoto, Some(KEY_CONTEXT))]);
    // After gpui-kit's own Escape of the field, so this one wins inside the dialog.
    cx.bind_keys([KeyBinding::new(
        "escape",
        CloseGoto,
        Some("GotoBar > Input"),
    )]);
}

impl EditorView {
    /// Opens the dialog with the cursor's place selected in its field. The find bar closes.
    pub(crate) fn open_goto(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let input = match &self.goto_input {
            Some(input) => input.clone(),
            None => {
                let input = cx.new(|cx| InputState::new(window, cx));
                cx.subscribe_in(&input, window, |this, _, event: &InputEvent, window, cx| {
                    if let InputEvent::PressEnter { .. } = event {
                        this.go_to_typed(window, cx);
                    }
                })
                .detach();
                self.goto_input = Some(input.clone());
                input
            }
        };
        let here = format!(
            "{}:{}",
            self.state.cursor.line + 1,
            self.state.buffer.display_column(self.state.cursor)
        );
        input.update(cx, |input, cx| {
            input.set_value(here, window, cx);
            input.focus(window, cx);
            input.select_all(window, cx);
        });
        self.find_open = false;
        self.goto_open = true;
        cx.notify();
    }

    /// Goes to the place typed in the field and closes the dialog. Nothing happens while the
    /// text is not a place.
    fn go_to_typed(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let Some(input) = &self.goto_input else {
            return;
        };
        let text = input.read(cx).value().to_string();
        if let Some(target) = parse_target(&text) {
            self.state
                .go_to(target.line - 1, target.column.unwrap_or(1) - 1);
            self.close_goto(window, cx);
        }
    }

    /// Closes the dialog and gives the focus back to the text.
    pub(crate) fn close_goto(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.goto_open = false;
        self.focus_handle().clone().focus(window, cx);
        cx.notify();
    }

    /// The dialog, centered at the top of the editor. The row that holds it lets the pointer
    /// through; only the dialog itself takes it.
    pub(crate) fn render_goto_bar(&self, cx: &mut Context<Self>) -> AnyElement {
        let palette = cx.palette();
        let Some(input) = &self.goto_input else {
            return div().into_any_element();
        };
        let lines = self.state.buffer.line_count();
        let dialog = v_flex()
            .id("goto-dialog")
            .key_context(GOTO_CONTEXT)
            .occlude()
            .w(px(GOTO_WIDTH))
            .p(px(14.0))
            .gap(px(12.0))
            .bg(palette.overlay)
            .border_1()
            .border_color(palette.border_strong)
            .rounded(px(RADIUS_DIALOG))
            .shadow(palette.shadow.clone())
            .on_action(cx.listener(|this, _: &CloseGoto, window, cx| this.close_goto(window, cx)))
            .child(
                h_flex()
                    .items_center()
                    .gap(px(8.0))
                    .child(
                        Icon::new(IconName::ArrowDownToLine)
                            .size(px(14.0))
                            .text_color(palette.fg_subtle),
                    )
                    .child(
                        div()
                            .flex_1()
                            .font_weight(FontWeight::SEMIBOLD)
                            .child(t!("goto.title")),
                    )
                    .child(
                        IconButton::new("goto-close", IconName::X)
                            .tooltip(t!("find.close"))
                            .on_click(
                                cx.listener(|this, _, window, cx| this.close_goto(window, cx)),
                            ),
                    ),
            )
            .child(h_flex().child(Field::new(input, GOTO_FIELD_HEIGHT, 13.0)))
            .child(
                div()
                    .text_size(px(12.0))
                    .line_height(relative(1.45))
                    .text_color(palette.fg_subtle)
                    .child(t!("goto.hint", count = lines)),
            )
            .child(
                h_flex()
                    .justify_end()
                    .gap(px(8.0))
                    .child(
                        SecondaryButton::new("goto-cancel", t!("common.cancel"))
                            .height(GOTO_BUTTON_HEIGHT)
                            .on_click(
                                cx.listener(|this, _, window, cx| this.close_goto(window, cx)),
                            ),
                    )
                    .child(
                        PrimaryButton::new("goto-go", t!("goto.go"))
                            .height(GOTO_BUTTON_HEIGHT)
                            .on_click(
                                cx.listener(|this, _, window, cx| this.go_to_typed(window, cx)),
                            ),
                    ),
            );
        h_flex()
            .absolute()
            .top(px(GOTO_TOP))
            .left_0()
            .right_0()
            .justify_center()
            .child(dialog)
            .into_any_element()
    }
}
