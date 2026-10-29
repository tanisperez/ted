//! The activity rail of folder mode, left of the side panel: the Explorer button for now (Git and
//! Settings join it with their features). The active panel's button has a 2 px `accent` bar on
//! its left edge.

use gpui_kit::assets::IconName;
use gpui_kit::component::tooltip::Tooltip;
use gpui_kit::component::*;
use gpui_kit::prelude::*;
use gpui_kit::*;
use rust_i18n::t;

use crate::theme::PaletteExt;
use crate::theme::metrics::{
    RAIL_BUTTON_HEIGHT, RAIL_INDICATOR_INSET, RAIL_INDICATOR_WIDTH, RAIL_WIDTH,
};

use super::root::AppView;

impl AppView {
    /// Renders the rail.
    pub(crate) fn render_rail(&self, cx: &mut Context<Self>) -> impl IntoElement {
        let palette = cx.palette();
        let active = self.explorer_visible;
        let color = if active {
            palette.fg
        } else {
            palette.fg_subtle
        };
        let hover = palette.fg;
        v_flex()
            .flex_none()
            .w(px(RAIL_WIDTH))
            .h_full()
            .bg(palette.surface)
            .border_r_1()
            .border_color(palette.border)
            .child(
                div()
                    .id("rail-explorer")
                    .relative()
                    .w_full()
                    .h(px(RAIL_BUTTON_HEIGHT))
                    .flex()
                    .items_center()
                    .justify_center()
                    .cursor_pointer()
                    .text_color(color)
                    .hover(move |style| style.text_color(hover))
                    .tooltip(|window, cx| {
                        Tooltip::new(SharedString::from(t!("explorer.title"))).build(window, cx)
                    })
                    .when(active, |button| {
                        button.child(
                            div()
                                .absolute()
                                .left_0()
                                .top(px(RAIL_INDICATOR_INSET))
                                .bottom(px(RAIL_INDICATOR_INSET))
                                .w(px(RAIL_INDICATOR_WIDTH))
                                .bg(palette.accent),
                        )
                    })
                    .child(Icon::new(IconName::Files).size(px(20.0)))
                    .on_click(cx.listener(|this, _, _, cx| {
                        this.explorer_visible = !this.explorer_visible;
                        cx.notify();
                    })),
            )
    }
}
