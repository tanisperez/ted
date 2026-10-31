//! The status bar at the bottom of the main window. In folder mode it starts with the panel
//! toggles (Files and Git, with its count of changed files), a separator and the folder path (a click
//! opens it in the file manager); with a document open, the cursor position, the indentation, the
//! encoding, the line ending and the language, right aligned as in the design. Inside a git
//! repository the branch comes before the folder path.

use gpui_kit::assets::IconName;
use gpui_kit::component::tooltip::Tooltip;
use gpui_kit::component::*;
use gpui_kit::prelude::*;
use gpui_kit::*;
use rust_i18n::t;

use crate::actions::{ToggleExplorer, ToggleGit};
use crate::state::launch::{Target, shorten_path};
use crate::state::status::DocumentStatus;
use crate::theme::metrics::{
    RADIUS_MD, STATUS_BAR_HEIGHT, STATUS_TOGGLE_HEIGHT, STATUS_TOGGLE_MIN_WIDTH,
};
use crate::theme::{Palette, PaletteExt};

use super::root::{AppView, SidePanel};

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
        let git_badge = Some(self.git.change_count()).filter(|count| *count > 0);
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
                        .child(self.render_panel_toggle(SidePanel::Files, None, &palette, cx))
                        .child(self.render_panel_toggle(SidePanel::Git, git_badge, &palette, cx)),
                )
                .child(div().w(px(1.0)).h(px(14.0)).bg(palette.border))
            })
            .children(self.git.branch_label().map(|branch| {
                h_flex()
                    .id("status-branch")
                    .items_center()
                    .gap(px(6.0))
                    .text_color(palette.accent_text)
                    .child(Icon::new(IconName::GitBranch).with_size(px(13.0)))
                    .child(branch)
            }))
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
                    .child(status.language)
            }))
            .into_any_element()
    }

    /// A panel toggle (Files, Git): shows `panel` at the left, or hides it when it already shows,
    /// as its shortcut. While its panel shows it has an `accent_subtle` background and
    /// `accent_text` icon. `badge` is a count after the icon (the Git panel's changes).
    fn render_panel_toggle(
        &self,
        panel: SidePanel,
        badge: Option<usize>,
        palette: &Palette,
        cx: &mut Context<Self>,
    ) -> impl IntoElement {
        let active = self.explorer_visible && self.side_panel == panel;
        let hover = palette.hover;
        let hover_fg = palette.fg;
        let (id, icon) = match panel {
            SidePanel::Files => ("status-files", IconName::Files),
            SidePanel::Git => ("status-git", IconName::GitBranch),
        };
        // The background of the active state sits on an outer box so the hover tint can layer on
        // top of it, as the design's inset shadow does.
        div()
            .rounded(px(RADIUS_MD - 2.0))
            .when(active, |toggle| toggle.bg(palette.accent_subtle))
            .child(
                div()
                    .id(id)
                    .h(px(STATUS_TOGGLE_HEIGHT))
                    .min_w(px(STATUS_TOGGLE_MIN_WIDTH))
                    .px(px(7.0))
                    .flex()
                    .items_center()
                    .justify_center()
                    .gap(px(5.0))
                    .rounded(px(RADIUS_MD - 2.0))
                    .cursor_pointer()
                    .text_color(if active {
                        palette.accent_text
                    } else {
                        palette.fg_muted
                    })
                    .hover(move |style| style.bg(hover).text_color(hover_fg))
                    .tooltip(move |window, cx| match panel {
                        SidePanel::Files => Tooltip::new(SharedString::from(t!("explorer.title")))
                            .action(&ToggleExplorer, None)
                            .build(window, cx),
                        SidePanel::Git => Tooltip::new(SharedString::from(t!("git.toggle")))
                            .action(&ToggleGit, None)
                            .build(window, cx),
                    })
                    .child(Icon::new(icon).with_size(px(15.0)))
                    .children(badge.map(|count| {
                        div()
                            .font_family(cx.theme().mono_font_family.clone())
                            .text_size(px(11.0))
                            .font_weight(FontWeight::MEDIUM)
                            .child(telaio_app::number::format_integer(count as u64))
                    }))
                    .on_click(cx.listener(move |this, _, _, cx| this.toggle_side_panel(panel, cx))),
            )
    }
}
