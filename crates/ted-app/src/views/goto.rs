//! The Go to line bar of an editor (`Ctrl+G`): one field that takes `line` or `line:column`. It
//! floats where the find bar does, in the same `overlay` look, and replaces it while open. Enter
//! goes there and closes it, Escape closes it; both give the focus back to the text. The parsing
//! lives in `state::goto`.

use gpui_kit::component::input::{InputEvent, InputState};
use gpui_kit::prelude::*;
use gpui_kit::*;
use rust_i18n::t;
use telaio_ui::components::text_field;

use crate::state::goto::parse_target;
use crate::theme::PaletteExt;
use crate::theme::metrics::{FIND_BAR_FIELD_WIDTH, RADIUS_MD};

use super::editor::{EditorView, KEY_CONTEXT};

actions!(
    goto,
    [
        /// Opens the Go to line bar (`Ctrl+G`).
        OpenGoto,
        /// Closes it (Escape in it).
        CloseGoto,
    ]
);

/// The key context of the bar.
const GOTO_CONTEXT: &str = "GotoBar";

/// Registers the bar's key bindings.
pub fn bind_keys(cx: &mut App) {
    // Ctrl+G on every platform: Cmd+G is Find next on macOS.
    cx.bind_keys([KeyBinding::new("ctrl-g", OpenGoto, Some(KEY_CONTEXT))]);
    // After gpui-kit's own Escape of the field, so this one wins inside the bar.
    cx.bind_keys([KeyBinding::new(
        "escape",
        CloseGoto,
        Some("GotoBar > Input"),
    )]);
}

impl EditorView {
    /// Opens the bar, empty, with the focus in its field. The find bar closes.
    pub(crate) fn open_goto(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let input = match &self.goto_input {
            Some(input) => input.clone(),
            None => {
                let input = cx.new(|cx| InputState::new(window, cx));
                cx.subscribe_in(
                    &input,
                    window,
                    |this, input, event: &InputEvent, window, cx| {
                        if let InputEvent::PressEnter { .. } = event {
                            let text = input.read(cx).value().to_string();
                            if let Some(target) = parse_target(&text) {
                                this.state
                                    .go_to(target.line - 1, target.column.unwrap_or(1) - 1);
                                this.close_goto(window, cx);
                            }
                        }
                    },
                )
                .detach();
                self.goto_input = Some(input.clone());
                input
            }
        };
        let lines = self.state.buffer.line_count();
        input.update(cx, |input, cx| {
            input.set_placeholder(t!("goto.placeholder", count = lines), window, cx);
            input.set_value("", window, cx);
            input.focus(window, cx);
        });
        self.find_open = false;
        self.goto_open = true;
        cx.notify();
    }

    /// Closes the bar and gives the focus back to the text.
    pub(crate) fn close_goto(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.goto_open = false;
        self.focus_handle().clone().focus(window, cx);
        cx.notify();
    }

    /// The bar: the field alone.
    pub(crate) fn render_goto_bar(&self, cx: &mut Context<Self>) -> AnyElement {
        let palette = cx.palette();
        let Some(input) = &self.goto_input else {
            return div().into_any_element();
        };
        div()
            .id("goto-bar")
            .key_context(GOTO_CONTEXT)
            .absolute()
            .top(px(10.0))
            .right(px(22.0))
            .occlude()
            .p(px(4.0))
            .bg(palette.overlay)
            .border_1()
            .border_color(palette.border)
            .rounded(px(RADIUS_MD))
            .shadow(palette.shadow.clone())
            .on_action(cx.listener(|this, _: &CloseGoto, window, cx| this.close_goto(window, cx)))
            .child(
                div()
                    .w(px(FIND_BAR_FIELD_WIDTH))
                    .child(text_field(input, cx)),
            )
            .into_any_element()
    }
}
