//! The title bar: the "t" logo and "ted". The window controls at the far right are drawn by
//! gpui-kit's `TitleBar`.

use gpui_kit::component::*;
use gpui_kit::prelude::*;
use gpui_kit::*;

use crate::theme::metrics::{LOGO_SIZE, TITLE_BAR_HEIGHT};
use crate::theme::{Palette, PaletteExt};

use super::root::AppView;

impl AppView {
    /// Renders the whole title bar.
    pub(crate) fn render_title_bar(
        &mut self,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let palette = cx.palette();
        TitleBar::new()
            .h(px(TITLE_BAR_HEIGHT))
            .child(
                h_flex()
                    .size_full()
                    .items_center()
                    .gap_2()
                    .child(render_logo(&palette))
                    .child(div().font_weight(FontWeight::SEMIBOLD).child("ted")),
            )
            .into_any_element()
    }
}

/// The "t" logo square: `accent` background, `accent_fg` text.
fn render_logo(palette: &Palette) -> impl IntoElement {
    div()
        .size(px(LOGO_SIZE))
        .rounded(px(5.0))
        .bg(palette.accent)
        .text_color(palette.accent_fg)
        .text_size(px(11.0))
        .font_weight(FontWeight::BOLD)
        .flex()
        .items_center()
        .justify_center()
        .child("t")
}
