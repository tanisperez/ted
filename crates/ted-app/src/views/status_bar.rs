//! The status bar at the bottom of the main window: with a document open, the cursor position,
//! the indentation, the encoding and the line ending, right aligned as in the design. The
//! language joins them with highlighting (#9), the folder path and the branch with folder mode.

use gpui_kit::component::*;
use gpui_kit::prelude::*;
use gpui_kit::*;

use crate::state::status::DocumentStatus;
use crate::theme::PaletteExt;
use crate::theme::metrics::STATUS_BAR_HEIGHT;

use super::root::AppView;

impl AppView {
    /// Renders the status bar.
    pub(crate) fn render_status_bar(&self, cx: &Context<Self>) -> AnyElement {
        let palette = cx.palette();
        let status = self
            .editor()
            .map(|editor| DocumentStatus::of(&editor.read(cx).state));
        h_flex()
            .flex_none()
            .h(px(STATUS_BAR_HEIGHT))
            .items_center()
            .gap(px(16.0))
            .px_3()
            .bg(palette.surface)
            .border_t_1()
            .border_color(palette.border)
            .text_size(px(11.5))
            .text_color(palette.fg_subtle)
            .child(div().flex_1())
            .children(status.map(|status| {
                h_flex()
                    .gap(px(16.0))
                    .child(status.position)
                    .children(status.indent)
                    .child(status.encoding)
                    .child(status.line_ending)
            }))
            .into_any_element()
    }
}
