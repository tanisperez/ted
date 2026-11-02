//! [`Field`]: the text field of the find bar, Go to line and Find in files, from the 0.5.0
//! search design. A `raised` box with a 1 px border and the input in mono. Focused, the border turns `accent` with a 3 px ring
//! of `accent` at 20%; hovered, `border_strong`.
//!
//! The input is gpui-kit's, without its own box (`appearance(false)`), with the app's edit menu
//! and a line height that is a whole number of device pixels (`input_line_height`), so the text
//! does not scroll inside the box at fractional scales.

use gpui_kit::component::input::{Input, InputState};
use gpui_kit::component::*;
use gpui_kit::prelude::*;
use gpui_kit::*;
use telaio_ui::components::{edit_menu, input_line_height};

use crate::theme::PaletteExt;
use crate::theme::metrics::{FIELD_RADIUS, FIELD_RING};

/// Line height of the input inside a [`Field`].
const LINE_HEIGHT: f32 = 20.0;

/// A field: the box and the input.
#[derive(IntoElement)]
pub struct Field {
    state: Entity<InputState>,
    height: f32,
    text_size: f32,
}

impl Field {
    /// A field for `state`, `height` logical pixels high, with its text at `text_size`.
    pub fn new(state: &Entity<InputState>, height: f32, text_size: f32) -> Self {
        Self {
            state: state.clone(),
            height,
            text_size,
        }
    }
}

impl RenderOnce for Field {
    fn render(self, window: &mut Window, cx: &mut App) -> impl IntoElement {
        let palette = cx.palette();
        let focused = self.state.read(cx).focus_handle(cx).is_focused(window);
        let line_height = px(input_line_height(LINE_HEIGHT, window.scale_factor()));
        let mono = cx.theme().mono_font_family.clone();
        let (border, border_strong, accent) =
            (palette.border, palette.border_strong, palette.accent);
        h_flex()
            .id(("field", self.state.entity_id()))
            .flex_1()
            .min_w_0()
            .h(px(self.height))
            .items_center()
            .gap(px(8.0))
            .pl(px(10.0))
            .pr(px(3.0))
            .rounded(px(FIELD_RADIUS))
            .bg(palette.raised)
            .border_1()
            .map(|field| {
                if focused {
                    field.border_color(accent).shadow(vec![
                        BoxShadow::new(px(0.0), px(0.0), accent.opacity(0.2))
                            .spread_radius(px(FIELD_RING)),
                    ])
                } else {
                    field
                        .border_color(border)
                        .hover(move |style| style.border_color(border_strong))
                }
            })
            .child(
                div().flex_1().min_w_0().child(
                    Input::new(&self.state)
                        .appearance(false)
                        .context_menu(edit_menu(&self.state, cx))
                        .px_0()
                        .py_0()
                        .font_family(mono)
                        .text_size(px(self.text_size))
                        .line_height(line_height),
                ),
            )
    }
}
