//! The main window's view: the title bar, the content area and the status bar. The content is
//! empty until the editor, the explorer and the empty state land.

use gpui_kit::component::*;
use gpui_kit::prelude::*;
use gpui_kit::*;

use crate::state::settings::Settings;
use crate::theme::PaletteExt;

/// The root view of the main window.
pub struct AppView {
    /// The settings in effect, read once at startup.
    pub settings: Settings,
}

impl AppView {
    pub fn new(settings: Settings) -> Self {
        Self { settings }
    }
}

impl Render for AppView {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        log::trace!("render AppView");
        let palette = cx.palette();
        v_flex()
            .size_full()
            .bg(palette.bg)
            .text_color(palette.fg)
            .child(self.render_title_bar(window, cx))
            .child(div().flex_1())
            .child(self.render_status_bar(cx))
    }
}
