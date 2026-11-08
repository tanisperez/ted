//! The scrollbars laid over the scrolling lists: always shown, with a transparent track so only
//! the thumb stands out, as the Markdown preview's. Nothing is drawn while the content fits.

use gpui_kit::component::scroll::{Scrollbar, ScrollbarAxis, ScrollbarHandle, ScrollbarMode};
use gpui_kit::prelude::*;
use gpui_kit::*;

use crate::theme::Palette;

/// A scrollbar of `axis` for the area scrolled with `handle`, to put next to the scrolling element
/// inside a `relative()` wrapper. `id` tells apart the scrollbars of one view.
pub(crate) fn scrollbar_overlay<H: ScrollbarHandle + Clone>(
    handle: &H,
    axis: ScrollbarAxis,
    id: &'static str,
    palette: &Palette,
) -> Div {
    let track = palette.overlay.opacity(0.0);
    let scrollbar = match axis {
        ScrollbarAxis::Horizontal => Scrollbar::horizontal(handle),
        _ => Scrollbar::vertical(handle),
    };
    div()
        .absolute()
        .inset_0()
        .child(
            scrollbar
                .id(id)
                .mode(ScrollbarMode::Always)
                .styles(move |styles| {
                    styles
                        .track(|style| style.bg(track).border_color(track))
                        .track_hover(|style| style.bg(track).border_color(track))
                        .track_active(|style| style.bg(track).border_color(track))
                }),
        )
}
