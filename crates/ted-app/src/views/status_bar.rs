//! The status bar at the bottom of the main window. In folder mode it starts with the panel
//! toggles (Files for now, Git joins it with its panel), a separator and the folder path (a click
//! opens it in the file manager); with a document open, the cursor position, the indentation, the
//! encoding and the line ending, right aligned as in the design. The language joins them with
//! highlighting (#9), the branch with the Git panel.

use gpui_kit::assets::IconName;
use gpui_kit::component::tooltip::Tooltip;
use gpui_kit::component::*;
use gpui_kit::prelude::*;
use gpui_kit::*;
use rust_i18n::t;

use crate::actions::ToggleExplorer;
use crate::state::launch::{Target, shorten_path};
use crate::state::status::DocumentStatus;
use crate::theme::metrics::{
    RADIUS_MD, STATUS_BAR_HEIGHT, STATUS_TOGGLE_HEIGHT, STATUS_TOGGLE_MIN_WIDTH,
};
use crate::theme::{Palette, PaletteExt};

use super::root::AppView;

impl AppView {
    /// Renders the status bar.
    pub(crate) fn render_status_bar(&self, cx: &mut Context<Self>) -> AnyElement {
        let palette = cx.palette();
        let status = self
            .editor()
            .map(|editor| DocumentStatus::of(&editor.read(cx).state));
        let folder = match &self.target {
            Some(Target::Folder(root)) => Some(root.clone()),
            _ => None,
        };
        h_flex()
            .flex_none()
            .h(px(STATUS_BAR_HEIGHT))
            .items_center()
            .gap(px(16.0))
            .pl(px(4.0))
            .pr_3()
            .bg(palette.surface)
            .border_t_1()
            .border_color(palette.border)
            .text_size(px(11.5))
            .text_color(palette.fg_subtle)
            .when(folder.is_some(), |bar| {
                bar.child(
                    h_flex()
                        .items_center()
                        .gap(px(2.0))
                        .mr(px(-4.0))
                        .child(self.render_files_toggle(&palette, cx)),
                )
                .child(div().w(px(1.0)).h(px(14.0)).bg(palette.border))
            })
            .children(folder.map(|root| {
                let label = shorten_path(&root, std::env::home_dir().as_deref());
                h_flex()
                    .id("status-folder")
                    .items_center()
                    .gap(px(6.0))
                    .cursor_pointer()
                    .hover(|style| style.underline())
                    .child(Icon::new(IconName::HardDrive).small())
                    .child(label)
                    .on_click(move |_, _, cx| cx.open_with_system(&root))
            }))
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

    /// The Files toggle: shows or hides the explorer, as `Alt 1`. While the explorer shows it has
    /// an `accent_subtle` background and `accent_text` icon.
    fn render_files_toggle(&self, palette: &Palette, cx: &mut Context<Self>) -> impl IntoElement {
        let active = self.explorer_visible;
        let hover = palette.hover;
        let hover_fg = palette.fg;
        // The background of the active state sits on an outer box so the hover tint can layer on
        // top of it, as the design's inset shadow does.
        div()
            .rounded(px(RADIUS_MD - 2.0))
            .when(active, |toggle| toggle.bg(palette.accent_subtle))
            .child(
                div()
                    .id("status-files")
                    .h(px(STATUS_TOGGLE_HEIGHT))
                    .min_w(px(STATUS_TOGGLE_MIN_WIDTH))
                    .px(px(7.0))
                    .flex()
                    .items_center()
                    .justify_center()
                    .rounded(px(RADIUS_MD - 2.0))
                    .cursor_pointer()
                    .text_color(if active {
                        palette.accent_text
                    } else {
                        palette.fg_muted
                    })
                    .hover(move |style| style.bg(hover).text_color(hover_fg))
                    .tooltip(|window, cx| {
                        Tooltip::new(SharedString::from(t!("explorer.title")))
                            .action(&ToggleExplorer, None)
                            .build(window, cx)
                    })
                    .child(Icon::new(IconName::Files).with_size(px(15.0)))
                    .on_click(cx.listener(|this, _, _, cx| {
                        this.explorer_visible = !this.explorer_visible;
                        cx.notify();
                    })),
            )
    }
}
