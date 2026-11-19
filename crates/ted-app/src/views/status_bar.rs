//! The status bar at the bottom of the main window. In folder mode it starts with the panel
//! toggles (Files and Git, with its count of changed files), a separator and, inside a git
//! repository, the branch; with a document open, the cursor position, the indentation, the
//! encoding, the line ending and the language, right aligned as in the design, and the keyboard
//! button of the shortcuts cheat sheet. The folder's path lives in the title bar's switcher
//! menu.

use gpui_kit::assets::IconName;
use gpui_kit::component::button::Button;
use gpui_kit::component::tooltip::Tooltip;
use gpui_kit::component::*;
use gpui_kit::prelude::*;
use gpui_kit::*;
use rust_i18n::t;
use telaio_ui::components::{NeutralButtonExt, SearchableMenu};

use crate::actions::{OpenShortcuts, ToggleExplorer, ToggleGit};
use crate::state::launch::Target;
use crate::state::status::DocumentStatus;
use crate::state::update::ManualUpdate;
use crate::theme::metrics::{
    RADIUS_MD, STATUS_BAR_HEIGHT, STATUS_TOGGLE_HEIGHT, STATUS_TOGGLE_MIN_WIDTH,
};
use crate::theme::{Palette, PaletteExt};

use super::language_menu::language_menu_popover;
use super::root::{AppView, SidePanel};

impl AppView {
    /// Renders the status bar.
    pub(crate) fn render_status_bar(&self, cx: &mut Context<Self>) -> AnyElement {
        let palette = cx.palette();
        let status = self
            .editor()
            .map(|editor| DocumentStatus::of(&editor.read(cx).state));
        let language = self
            .editor()
            .map(|editor| editor.read(cx).state.syntax.language);
        let language_menu_open = self.language_menu.read(cx).is_open();
        // A Markdown document shows its words, and no cursor while only the preview shows.
        let markdown = self
            .active_markdown(cx)
            .map(|preview| (preview.read(cx).words(), preview.read(cx).view()));
        let words = markdown.map(|(words, _)| match words {
            1 => t!("markdown.word").into_owned(),
            count => t!(
                "markdown.words",
                count = telaio_app::number::format_integer(count as u64)
            )
            .into_owned(),
        });
        let shows_cursor = markdown.is_none_or(|(_, view)| view.shows_editor());
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
                let (ahead, behind) = self
                    .git
                    .status()
                    .map_or((0, 0), |(_, status)| (status.ahead, status.behind));
                let count = |icon, count: u32| {
                    (count > 0).then(|| {
                        h_flex()
                            .items_center()
                            .gap(px(2.0))
                            .child(Icon::new(icon).with_size(px(11.0)))
                            .child(telaio_app::number::format_integer(u64::from(count)))
                    })
                };
                let content = h_flex()
                    .items_center()
                    .gap(px(6.0))
                    .text_color(palette.accent_text)
                    .child(Icon::new(IconName::GitBranch).with_size(px(13.0)))
                    .child(branch)
                    .child(Icon::new(IconName::ChevronUp).with_size(px(11.0)))
                    .children(count(IconName::ArrowUp, ahead))
                    .children(count(IconName::ArrowDown, behind));
                SearchableMenu::new("status-branch-menu", &self.branch_menu).trigger(
                    Button::new("status-branch")
                        .neutral_ghost(cx, palette.accent_text)
                        .compact()
                        .h(px(22.0))
                        .px(px(6.0))
                        .mx(px(-6.0))
                        .rounded(px(RADIUS_MD - 2.0))
                        .text_size(px(11.5))
                        .tooltip(t!("git.menu.tooltip"))
                        .child(content),
                )
            }))
            .children(self.git.sync.running.map(|op| {
                h_flex()
                    .items_center()
                    .gap(px(6.0))
                    .child(Icon::new(IconName::LoaderCircle).with_size(px(13.0)))
                    .child(op.progress_label())
            }))
            .child(div().flex_1())
            .children(self.render_update_item(&palette, cx))
            .children(status.map(|status| {
                h_flex()
                    .gap(px(16.0))
                    .children(words)
                    .when(shows_cursor, |row| {
                        row.child(self.render_position(status.position, &palette, cx))
                    })
                    .child(self.render_wrap_toggle(status.wrap, &palette, cx))
                    .children(status.indent)
                    .child(status.encoding)
                    .child(status.line_ending)
                    .child(language_menu_popover(
                        &self.language_menu,
                        language,
                        Button::new("status-language")
                            .neutral_ghost(cx, palette.fg_subtle)
                            .compact()
                            .h(px(22.0))
                            .px(px(6.0))
                            .mx(px(-6.0))
                            .rounded(px(RADIUS_MD - 2.0))
                            .text_size(px(11.5))
                            // The open menu keeps the button lit, as in the design.
                            .when(language_menu_open, |button| {
                                button.bg(palette.hover).text_color(palette.fg)
                            })
                            .tooltip(t!("status.language_tooltip"))
                            .child(status.language),
                        cx,
                    ))
            }))
            .child(self.render_shortcuts_button(&palette, cx))
            .into_any_element()
    }

    /// The keyboard button at the right end: opens the shortcuts cheat sheet, as `F1` does.
    fn render_shortcuts_button(
        &self,
        palette: &Palette,
        cx: &mut Context<Self>,
    ) -> impl IntoElement {
        let hover = palette.hover;
        let hover_fg = palette.fg;
        div()
            .id("status-shortcuts")
            .size(px(20.0))
            .mr(px(-6.0))
            .flex()
            .items_center()
            .justify_center()
            .rounded(px(RADIUS_MD - 2.0))
            .cursor_pointer()
            .text_color(palette.fg_muted)
            .hover(move |style| style.bg(hover).text_color(hover_fg))
            .tooltip(|window, cx| {
                Tooltip::new(SharedString::from(t!("shortcuts.button")))
                    .action(&OpenShortcuts, None)
                    .build(window, cx)
            })
            .child(Icon::new(IconName::Keyboard).with_size(px(14.0)))
            .on_click(cx.listener(|this, _, window, cx| this.open_shortcuts(window, cx)))
    }

    /// The Wrap toggle: the wrap-text icon and "Wrap", `accent_subtle` and `accent_text` while
    /// the document wraps. A click does what Alt+Z does.
    /// `Ln 12, Col 5`: a click opens Go to line, as `Ctrl+G` (#117).
    fn render_position(
        &self,
        position: String,
        palette: &Palette,
        cx: &mut Context<Self>,
    ) -> impl IntoElement {
        let hover = palette.hover;
        h_flex()
            .id("status-position")
            .h(px(22.0))
            .px(px(6.0))
            .mx(px(-6.0))
            .items_center()
            .rounded(px(RADIUS_MD - 2.0))
            .cursor_pointer()
            .hover(move |style| style.bg(hover))
            .tooltip(|window, cx| {
                Tooltip::new(SharedString::from(t!("status.position_tooltip"))).build(window, cx)
            })
            .child(position)
            .on_click(cx.listener(|this, _, window, cx| {
                let Some(editor) = this.editor().cloned() else {
                    return;
                };
                editor.read(cx).focus_handle().clone().focus(window, cx);
                window.defer(cx, |window, cx| {
                    window.dispatch_action(Box::new(telaio_editor::views::goto::OpenGoto), cx)
                });
            }))
    }

    fn render_wrap_toggle(
        &self,
        wrap: bool,
        palette: &Palette,
        cx: &mut Context<Self>,
    ) -> impl IntoElement {
        let hover = palette.hover;
        h_flex()
            .id("status-wrap")
            .h(px(22.0))
            .px(px(6.0))
            .mx(px(-6.0))
            .gap(px(5.0))
            .items_center()
            .rounded(px(RADIUS_MD - 2.0))
            .cursor_pointer()
            .when(wrap, |toggle| {
                toggle
                    .bg(palette.accent_subtle)
                    .text_color(palette.accent_text)
            })
            .hover(move |style| style.bg(hover))
            .tooltip(move |window, cx| {
                let label = match wrap {
                    true => t!("status.wrap_on"),
                    false => t!("status.wrap_off"),
                };
                Tooltip::new(SharedString::from(label)).build(window, cx)
            })
            .child(Icon::new(IconName::TextWrap).with_size(px(14.0)))
            .child(t!("status.wrap"))
            .on_click(cx.listener(|this, _, _, cx| {
                if let Some(editor) = this.editor().cloned() {
                    editor.update(cx, |editor, cx| {
                        editor.state.toggle_wrap();
                        cx.notify();
                    });
                }
            }))
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

impl AppView {
    /// The discreet update item (#99): "ready, restart" with the release notes, or, for a copy
    /// that downloads nothing, "available" with what to do. Built from the stored update, no work
    /// beyond a few small clones.
    fn render_update_item(&self, palette: &Palette, cx: &mut Context<Self>) -> Option<AnyElement> {
        let ready = self.update.ready()?;
        let version = ready.version.as_str();
        let notes_url = ready.notes_url.clone();
        if ready.is_manual_download() {
            let manual = self.manual_update();
            let label = if manual == ManualUpdate::InstallScript {
                t!("update.status.available_script", version = version)
            } else {
                t!("update.status.available", version = version)
            };
            return Some(
                h_flex()
                    .id("status-bar-update-download")
                    .items_center()
                    .gap(px(6.0))
                    .cursor_pointer()
                    .text_color(palette.accent_text)
                    .hover(|style| style.underline())
                    .child(Icon::new(IconName::Download).with_size(px(13.0)))
                    .child(label.into_owned())
                    .on_click(cx.listener(move |view, _, window, cx| {
                        if manual == ManualUpdate::InstallScript {
                            view.copy_update_command(window, cx);
                        } else {
                            cx.open_url(&notes_url);
                        }
                    }))
                    .into_any_element(),
            );
        }
        Some(
            h_flex()
                .items_center()
                .gap(px(12.0))
                .child(
                    h_flex()
                        .id("status-bar-update-restart")
                        .items_center()
                        .gap(px(6.0))
                        .cursor_pointer()
                        .text_color(palette.accent_text)
                        .hover(|style| style.underline())
                        .child(Icon::new(IconName::RefreshCw).with_size(px(13.0)))
                        .child(t!("update.status.ready", version = version).into_owned())
                        .on_click(cx.listener(|view, _, window, cx| {
                            view.request_update_restart(window, cx)
                        })),
                )
                .child(
                    div()
                        .id("status-bar-update-notes")
                        .cursor_pointer()
                        .hover(|style| style.underline())
                        .child(t!("update.status.notes").into_owned())
                        .on_click(move |_, _, cx| cx.open_url(&notes_url)),
                )
                .into_any_element(),
        )
    }
}
