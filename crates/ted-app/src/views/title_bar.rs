//! The title bar: the app icon, "ted" and the switcher with what is open (the folder or the file),
//! whose menu lists the recent folders and files and the two Open actions. At the right, the
//! settings gear, then the window controls drawn by gpui-kit's `TitleBar`.

use gpui_kit::assets::IconName;
use gpui_kit::component::button::{Button, ButtonVariants as _};
use gpui_kit::component::menu::{DropdownMenu as _, PopupMenuItem};
use gpui_kit::component::tooltip::Tooltip;
use gpui_kit::component::*;
use gpui_kit::prelude::*;
use gpui_kit::*;
use rust_i18n::t;
use telaio_ui::components::{menu_shortcut_item, rich_menu_width};

use crate::actions::{OpenSettings, hints};
use crate::state::launch::{Target, display_name, shorten_path};
use crate::theme::metrics::{
    LOGO_SIZE, RADIUS_MD, RICH_MENU_ROW_HEIGHT, RICH_MENU_TEXT_SIZE, SETTINGS_GEAR_SIZE,
    TITLE_BAR_HEIGHT,
};
use crate::theme::{Palette, PaletteExt};

use super::logo::logo;
use super::root::AppView;

/// Width of the switcher's menu.
const MENU_WIDTH: f32 = 360.0;
/// Height of the switcher button.
const SWITCHER_HEIGHT: f32 = 26.0;

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
                    .child(logo(LOGO_SIZE))
                    .child(div().font_weight(FontWeight::SEMIBOLD).child("ted"))
                    .child(div().w(px(1.0)).h(px(16.0)).mx(px(4.0)).bg(palette.border))
                    .child(no_drag(self.render_switcher(&palette, cx)))
                    .child(div().flex_1())
                    .child(no_drag(render_settings_gear(&palette, cx))),
            )
            .into_any_element()
    }

    /// The switcher: an icon and the name of what is open (or "Open recent"), and its menu.
    fn render_switcher(&self, palette: &Palette, cx: &mut Context<Self>) -> impl IntoElement {
        let (icon, label) = match &self.target {
            Some(Target::Folder(path)) => (IconName::Folder, display_name(path)),
            Some(Target::File(path)) => (IconName::FileText, display_name(path)),
            None => (IconName::FolderClock, t!("open.recent").into_owned()),
        };
        let current = self
            .target
            .as_ref()
            .map(|target| target.path().to_path_buf());
        let folders = self.recents.folders.clone();
        let files = self.recents.files.clone();
        let weak = cx.weak_entity();
        let home = std::env::home_dir();

        Button::new("switcher")
            .ghost()
            .h(px(SWITCHER_HEIGHT))
            .px_2()
            .rounded(px(RADIUS_MD - 2.0))
            .text_color(palette.fg_muted)
            .icon(Icon::new(icon).small())
            .label(label)
            .dropdown_caret(true)
            .dropdown_menu(move |menu, _, _| {
                let mut menu = rich_menu_width(menu, MENU_WIDTH);
                for (group, paths) in [(IconName::Folder, &folders), (IconName::FileText, &files)] {
                    if paths.is_empty() {
                        continue;
                    }
                    for path in paths {
                        let name = display_name(path);
                        let detail = shorten_path(path, home.as_deref());
                        let selected = current.as_deref() == Some(path.as_path());
                        let target = path.clone();
                        let weak = weak.clone();
                        menu = menu.item(recent_item(group, name, detail, selected).on_click(
                            move |_, window, cx| {
                                let _ =
                                    weak.update(cx, |view, cx| view.open_path(&target, window, cx));
                            },
                        ));
                    }
                    menu = menu.separator();
                }
                let open_file = weak.clone();
                let open_folder = weak.clone();
                menu.item(menu_shortcut_item(
                    IconName::File,
                    t!("welcome.open_file"),
                    hints::OPEN_FILE,
                    move |window, cx| {
                        let _ =
                            open_file.update(cx, |view, cx| view.prompt_open(false, window, cx));
                    },
                ))
                .item(menu_shortcut_item(
                    IconName::FolderOpen,
                    t!("welcome.open_folder"),
                    hints::OPEN_FOLDER,
                    move |window, cx| {
                        let _ =
                            open_folder.update(cx, |view, cx| view.prompt_open(true, window, cx));
                    },
                ))
            })
    }
}

/// A recent folder or file in the switcher's menu: its icon, its name, the shortened path at the
/// right and a check on the one that is open.
fn recent_item(icon: IconName, name: String, detail: String, selected: bool) -> PopupMenuItem {
    PopupMenuItem::element(move |_, cx| {
        let palette = cx.palette();
        h_flex()
            .flex_1()
            .min_w_0()
            .h(px(RICH_MENU_ROW_HEIGHT))
            .px(px(4.0))
            .text_size(px(RICH_MENU_TEXT_SIZE))
            .gap(px(8.0))
            .items_center()
            .child(Icon::new(icon).small().text_color(palette.fg_subtle))
            .child(
                div()
                    .flex_none()
                    .max_w(px(140.0))
                    .truncate()
                    .child(name.clone()),
            )
            .child(
                div()
                    .flex_1()
                    .min_w_0()
                    .truncate()
                    .text_size(px(12.0))
                    .text_color(palette.fg_subtle)
                    .child(detail.clone()),
            )
            .children(selected.then(|| {
                Icon::new(IconName::Check)
                    .small()
                    .text_color(palette.accent_text)
            }))
    })
}

/// The settings gear: opens the Settings modal, as `Ctrl+,`.
fn render_settings_gear(palette: &Palette, cx: &mut Context<AppView>) -> impl IntoElement {
    let (hover, pressed) = (palette.hover, palette.pressed);
    div()
        .id("title-bar-settings")
        .size(px(SETTINGS_GEAR_SIZE))
        .mr(px(4.0))
        .flex()
        .items_center()
        .justify_center()
        .cursor_pointer()
        .rounded(px(RADIUS_MD - 2.0))
        .text_color(palette.fg_muted)
        .hover(move |style| style.bg(hover))
        .active(move |style| style.bg(pressed))
        .tooltip(|window, cx| {
            Tooltip::new(SharedString::from(t!("settings.open")))
                .action(&OpenSettings, None)
                .build(window, cx)
        })
        .child(Icon::new(IconName::Settings).with_size(px(16.0)))
        .on_click(cx.listener(|view, _, window, cx| view.open_settings(window, cx)))
}

/// Wraps an interactive title bar control so the mouse reaches it: `TitleBar` marks the whole bar
/// as a window drag area, and on Windows the system would eat the control's clicks (see Postino's
/// `views/title_bar.rs`).
fn no_drag(control: impl IntoElement) -> impl IntoElement {
    div().occlude().child(control)
}
