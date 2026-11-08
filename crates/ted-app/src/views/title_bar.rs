//! The title bar: the app icon, "ted" and the switcher with what is open (the folder or the file),
//! whose popover starts with its path (Copy path, Show in file manager), then lists the recent
//! folders and files, each with an X that takes it off the list, and the two Open actions. With a
//! folder or a file open, the search box of the
//! global search sits in the middle of the window. At the right, the settings gear, then the window controls
//! drawn by gpui-kit's `TitleBar`.

use std::path::Path;

use gpui_kit::assets::IconName;
use gpui_kit::component::button::{Button, ButtonVariants as _};
use gpui_kit::component::popover::{Popover, PopoverState};
use gpui_kit::component::scroll::Scrollbar;
use gpui_kit::component::tooltip::Tooltip;
use gpui_kit::component::*;
use gpui_kit::prelude::*;
use gpui_kit::*;
use rust_i18n::t;
use telaio_ui::components::SectionLabel;

use crate::actions::{OpenSettings, hints};
use crate::state::launch::{Target, display_name, shorten_path};
use crate::state::recents;
use crate::theme::metrics::{
    LOGO_SIZE, RADIUS_MD, SEARCH_TRIGGER_HEIGHT, SEARCH_TRIGGER_WIDTH, SETTINGS_GEAR_SIZE,
    TITLE_BAR_HEIGHT,
};
use crate::theme::{Palette, PaletteExt};

use super::location;
use super::logo::logo;
use super::root::AppView;

/// Width of the switcher's popover.
const MENU_WIDTH: f32 = 400.0;
/// Height the switcher's popover leaves to the rest of the viewport at its tallest: the title bar,
/// the popover's padding and border, and the shadow margins of a window with client decorations.
/// With less, gpui moves the popover up over its button to make it fit.
const MENU_WINDOW_MARGIN: f32 = 104.0;
/// Height of a recent row: its name over its path.
const RECENT_ROW_HEIGHT: f32 = 44.0;
/// Side of the X that takes a row off the recents.
const FORGET_SIZE: f32 = 28.0;
/// Height of an action row of the popover.
const ACTION_ROW_HEIGHT: f32 = 30.0;
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
        let bar = TitleBar::new().h(px(TITLE_BAR_HEIGHT)).child(
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
        );
        // Not in the empty state, which lists the recents itself.
        if self.target.is_none() && self.documents.active().is_none() {
            return bar.into_any_element();
        }
        // Centered on the whole window, as in Postino: the bar lays the window controls out
        // beside its content, so centering inside it would be off by half their width. The
        // overlay has no handlers, so it takes no mouse input from the bar around the box.
        let mono_font = cx.theme().mono_font_family.clone();
        div()
            .relative()
            .w_full()
            .flex_none()
            .child(bar)
            .child(
                div()
                    .absolute()
                    .top_0()
                    .left_0()
                    .size_full()
                    .flex()
                    .items_center()
                    .justify_center()
                    .child(no_drag(render_search_trigger(&palette, mono_font, cx))),
            )
            .into_any_element()
    }

    /// The switcher: an icon and the name of what is open (or "Open recent"), and its popover.
    /// The popover is rebuilt from the view's state on every render, so a recent taken off with
    /// its X disappears while it stays open.
    fn render_switcher(&self, palette: &Palette, cx: &mut Context<Self>) -> impl IntoElement {
        let (icon, label) = match &self.target {
            Some(Target::Folder(path)) => (IconName::Folder, display_name(path)),
            Some(Target::File(path)) => (IconName::FileText, display_name(path)),
            // A new document, not saved yet.
            None => match self.documents.active() {
                Some(document) => (IconName::FileText, document.name()),
                None => (IconName::FolderClock, t!("open.recent").into_owned()),
            },
        };
        let trigger = Button::new("switcher")
            .ghost()
            .h(px(SWITCHER_HEIGHT))
            .px_2()
            .rounded(px(RADIUS_MD - 2.0))
            .text_color(palette.fg_muted)
            .icon(Icon::new(icon).small())
            .label(label)
            .dropdown_caret(true);
        let weak = cx.weak_entity();
        let scroll = self.switcher_scroll.clone();
        Popover::new("switcher-popover")
            .w(px(MENU_WIDTH))
            .p(px(6.0))
            .trigger(trigger)
            // It opens at the top, wherever it was left.
            .on_open_change(move |_, _, _| scroll.set_offset(point(px(0.0), px(0.0))))
            .content(move |_, window, cx| render_switcher_menu(weak.clone(), window, cx))
    }

    /// Takes `path` off the recents, never off the disk.
    fn forget_recent(&mut self, path: &Path, cx: &mut Context<Self>) {
        self.recents.remove(path);
        recents::save(&self.recents);
        log::debug!("removed an entry from the recents");
        cx.notify();
    }
}

/// The switcher's popover: where what is open lives, the recent folders and files, and the two
/// Open actions. It scrolls instead of running past the window, as the design's
/// `max-height: calc(100vh - 56px)`.
fn render_switcher_menu(
    weak: WeakEntity<AppView>,
    window: &mut Window,
    cx: &mut Context<PopoverState>,
) -> AnyElement {
    let Some(view) = weak.upgrade() else {
        return div().into_any_element();
    };
    let view = view.read(cx);
    let current = view
        .target
        .as_ref()
        .map(|target| target.path().to_path_buf());
    // The location block at the top: the folder in folder mode, the file shown otherwise.
    let location = match &view.target {
        Some(Target::Folder(root)) => Some((root.clone(), true)),
        _ => view
            .documents
            .active()
            .and_then(|document| document.path.clone())
            .map(|path| (path, false)),
    };
    let folders = view.recents.folders.clone();
    let files = view.recents.files.clone();
    let scroll = view.switcher_scroll.clone();
    let home = std::env::home_dir();
    let palette = cx.palette();
    let mono_font = cx.theme().mono_font_family.clone();

    let mut column = v_flex().text_sm();
    if let Some((path, folder)) = location {
        let (copy, reveal) = (path.display().to_string(), path.clone());
        column = column
            .child(location_heading(&path, folder, &palette, mono_font.clone()))
            .child(
                action_row(
                    "switcher-copy-path",
                    IconName::Copy,
                    t!("open.copy_path"),
                    hints::COPY_PATH,
                    &palette,
                    mono_font.clone(),
                )
                .on_click(cx.listener(move |popover, _, window, cx| {
                    popover.dismiss(window, cx);
                    location::copy_path(copy.clone(), window, cx);
                })),
            )
            .child(
                action_row(
                    "switcher-reveal",
                    IconName::FolderSearch,
                    t!("open.reveal"),
                    hints::REVEAL,
                    &palette,
                    mono_font.clone(),
                )
                .on_click(cx.listener(move |popover, _, window, cx| {
                    popover.dismiss(window, cx);
                    location::reveal(&reveal, folder, cx);
                })),
            )
            .child(separator(&palette));
    }
    for (folder, heading, paths) in [
        (true, t!("welcome.recent_folders"), &folders),
        (false, t!("welcome.recent_files"), &files),
    ] {
        if paths.is_empty() {
            continue;
        }
        column = column.child(
            div()
                .px_2()
                .pt(px(6.0))
                .pb(px(4.0))
                .child(SectionLabel::new(heading)),
        );
        for (index, path) in paths.iter().enumerate() {
            let selected = current.as_deref() == Some(path.as_path());
            let row = recent_row(
                (folder, index),
                path,
                selected,
                home.as_deref(),
                weak.clone(),
                &palette,
                mono_font.clone(),
                cx,
            );
            column = column.child(row);
        }
        column = column.child(separator(&palette));
    }
    let (open_file, open_folder) = (weak.clone(), weak);
    column = column
        .child(
            action_row(
                "switcher-open-file",
                IconName::File,
                t!("welcome.open_file"),
                hints::OPEN_FILE,
                &palette,
                mono_font.clone(),
            )
            .on_click(cx.listener(move |popover, _, window, cx| {
                popover.dismiss(window, cx);
                let _ = open_file.update(cx, |view, cx| view.prompt_open(false, window, cx));
            })),
        )
        .child(
            action_row(
                "switcher-open-folder",
                IconName::FolderOpen,
                t!("welcome.open_folder"),
                hints::OPEN_FOLDER,
                &palette,
                mono_font,
            )
            .on_click(cx.listener(move |popover, _, window, cx| {
                popover.dismiss(window, cx);
                let _ = open_folder.update(cx, |view, cx| view.prompt_open(true, window, cx));
            })),
        );

    div()
        .relative()
        .child(
            div()
                .id("switcher-menu")
                .max_h(window.viewport_size().height - px(MENU_WINDOW_MARGIN))
                .overflow_y_scroll()
                .track_scroll(&scroll)
                .child(column),
        )
        .child(
            div()
                .absolute()
                .inset_0()
                .child(Scrollbar::vertical(&scroll)),
        )
        .into_any_element()
}

/// A recent folder or file: its name over its path (the folder that holds it, for a file). The
/// open one has a check and says so; any other opens on click and has the X that takes it off the
/// list.
#[allow(clippy::too_many_arguments)]
fn recent_row(
    (folder, index): (bool, usize),
    path: &Path,
    selected: bool,
    home: Option<&Path>,
    weak: WeakEntity<AppView>,
    palette: &Palette,
    mono_font: SharedString,
    cx: &mut Context<PopoverState>,
) -> impl IntoElement + use<> {
    let id = if folder { "folder" } else { "file" };
    let (icon, location) = match folder {
        true => (IconName::Folder, path),
        false => (IconName::FileText, path.parent().unwrap_or(path)),
    };
    let leading = if selected {
        Icon::new(IconName::Check)
            .small()
            .text_color(palette.accent_text)
    } else {
        Icon::new(icon).small().text_color(palette.fg_muted)
    };
    let trailing = if selected {
        div()
            .flex_none()
            .px_2()
            .text_size(px(11.0))
            .text_color(palette.fg_subtle)
            .child(t!("open.current"))
            .into_any_element()
    } else {
        let (fg, hover, pressed) = (palette.fg, palette.hover, palette.pressed);
        let tooltip = SharedString::from(t!("open.remove_recent").into_owned());
        let (forget_weak, forget_path) = (weak.clone(), path.to_path_buf());
        div()
            .id(SharedString::from(format!("forget-{id}-{index}")))
            .flex_none()
            .size(px(FORGET_SIZE))
            .flex()
            .items_center()
            .justify_center()
            .rounded(px(RADIUS_MD - 2.0))
            .cursor_pointer()
            .text_color(palette.fg_subtle)
            .hover(move |style| style.bg(hover).text_color(fg))
            .active(move |style| style.bg(pressed).text_color(fg))
            .child(Icon::new(IconName::X).with_size(px(14.0)))
            .tooltip(move |window, cx| Tooltip::new(tooltip.clone()).build(window, cx))
            .on_click(move |_, _, cx| {
                // The row under the button opens the recent on click: keep it out.
                cx.stop_propagation();
                let _ = forget_weak.update(cx, |view, cx| view.forget_recent(&forget_path, cx));
            })
            .into_any_element()
    };
    let (hover, pressed) = (palette.hover, palette.pressed);
    let target = path.to_path_buf();
    h_flex()
        .id(SharedString::from(format!("recent-{id}-{index}")))
        .h(px(RECENT_ROW_HEIGHT))
        .items_center()
        .gap(px(10.0))
        .pl_2()
        .pr_1()
        .rounded(px(RADIUS_MD - 2.0))
        .cursor_pointer()
        .when(selected, |row| row.bg(palette.list_active))
        .when(!selected, |row| {
            row.hover(move |style| style.bg(hover))
                .active(move |style| style.bg(pressed))
        })
        .child(leading)
        .child(
            v_flex()
                .flex_1()
                .min_w_0()
                .gap(px(2.0))
                .child(
                    div()
                        .truncate()
                        .text_color(palette.fg)
                        .child(display_name(path)),
                )
                .child(
                    div()
                        .truncate()
                        .font_family(mono_font)
                        .text_size(px(11.0))
                        .text_color(palette.fg_subtle)
                        .child(shorten_path(location, home)),
                ),
        )
        .child(trailing)
        .on_click(cx.listener(move |popover, _, window, cx| {
            popover.dismiss(window, cx);
            if !selected {
                let _ = weak.update(cx, |view, cx| view.open_path(&target, window, cx));
            }
        }))
}

/// A one-line action of the popover: an icon, its label and its shortcut.
fn action_row(
    id: &'static str,
    icon: IconName,
    label: impl Into<SharedString>,
    hint: &'static str,
    palette: &Palette,
    mono_font: SharedString,
) -> Stateful<Div> {
    let (hover, pressed) = (palette.hover, palette.pressed);
    h_flex()
        .id(id)
        .h(px(ACTION_ROW_HEIGHT))
        .items_center()
        .gap(px(10.0))
        .px_2()
        .rounded(px(RADIUS_MD - 2.0))
        .cursor_pointer()
        .text_color(palette.fg)
        .hover(move |style| style.bg(hover))
        .active(move |style| style.bg(pressed))
        .child(Icon::new(icon).small().text_color(palette.fg_muted))
        .child(div().flex_1().min_w_0().truncate().child(label.into()))
        .child(
            div()
                .flex_none()
                .font_family(mono_font)
                .text_size(px(11.0))
                .text_color(palette.fg_subtle)
                .child(hint),
        )
}

/// The line between the groups of the popover.
fn separator(palette: &Palette) -> impl IntoElement + use<> {
    div().h(px(1.0)).my(px(5.0)).mx(px(2.0)).bg(palette.border)
}

/// The top of the switcher's popover: Folder or File and the full path in mono, truncated.
fn location_heading(
    path: &Path,
    folder: bool,
    palette: &Palette,
    mono_font: SharedString,
) -> impl IntoElement + use<> {
    let heading = match folder {
        true => t!("open.location_folder"),
        false => t!("open.location_file"),
    };
    v_flex()
        .px_2()
        .py(px(6.0))
        .gap(px(3.0))
        .child(
            div()
                .text_size(px(11.0))
                .font_weight(FontWeight::SEMIBOLD)
                .text_color(palette.fg_subtle)
                .child(heading),
        )
        .child(
            div()
                .truncate()
                .font_family(mono_font)
                .text_size(px(11.5))
                .text_color(palette.fg_muted)
                .child(path.display().to_string()),
        )
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
/// The `group` of the search box, so its key hint follows the box's hover.
const SEARCH_TRIGGER_GROUP: &str = "title-bar-search";

/// The search box: opens the global search (`views/search.rs`), as `Ctrl+K` does.
fn render_search_trigger(
    palette: &Palette,
    mono_font: SharedString,
    cx: &mut Context<AppView>,
) -> impl IntoElement {
    let accent = palette.accent;
    let accent_text = palette.accent_text;
    let fg_muted = palette.fg_muted;
    h_flex()
        .id("title-bar-search")
        .group(SEARCH_TRIGGER_GROUP)
        .w(px(SEARCH_TRIGGER_WIDTH))
        .h(px(SEARCH_TRIGGER_HEIGHT))
        .items_center()
        .gap_2()
        .px(px(10.0))
        .cursor_pointer()
        .rounded(px(RADIUS_MD - 1.0))
        .bg(palette.raised)
        .border_1()
        .border_color(palette.border)
        .text_color(palette.fg_subtle)
        // As the design: an accent border, a soft glow and a 3 px ring.
        .hover(move |style| {
            style.border_color(accent).text_color(fg_muted).shadow(vec![
                BoxShadow::new(px(0.0), px(2.0), accent.opacity(0.22)).blur_radius(px(8.0)),
                BoxShadow::new(px(0.0), px(0.0), accent.opacity(0.16)).spread_radius(px(3.0)),
            ])
        })
        // An `svg`, not an `Icon`: an `Icon` takes its color when built, so a hover never reaches it.
        .child(
            svg()
                .path(IconName::Search.path())
                .flex_none()
                .size_3p5()
                .text_color(palette.fg_subtle)
                .group_hover(SEARCH_TRIGGER_GROUP, move |style| {
                    style.text_color(accent_text)
                }),
        )
        .child(div().flex_1().truncate().child(t!("search.hint")))
        .child(
            div()
                .font_family(mono_font)
                .text_size(px(11.0))
                .border_1()
                .border_color(palette.border)
                .group_hover(SEARCH_TRIGGER_GROUP, move |style| {
                    style.border_color(accent).text_color(accent_text)
                })
                .rounded(px(4.0))
                .px(px(4.0))
                .child(hints::SEARCH),
        )
        .on_click(cx.listener(|view, _, window, cx| view.open_search(window, cx)))
}

fn no_drag(control: impl IntoElement) -> impl IntoElement {
    div().occlude().child(control)
}
