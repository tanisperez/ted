//! The empty state: what the window shows with nothing open. A centered column with the logo,
//! the two Open actions and the recent folders and files. Its rows are the design's recents menu
//! rows (30 px, icon 14, name, path in 12 px `fg_subtle`), shown without the menu's card.

use std::path::{Path, PathBuf};

use gpui_kit::component::*;
use gpui_kit::prelude::*;
use gpui_kit::*;
use rust_i18n::t;
use telaio_ui::components::{SecondaryButton, SectionLabel};

use crate::actions::hints;
use crate::state::launch::{display_name, shorten_path};
use crate::theme::metrics::{
    RADIUS_MD, WELCOME_LOGO_SIZE, WELCOME_ROW_HEIGHT, WELCOME_SECTION_GAP, WELCOME_WIDTH,
};
use crate::theme::{Palette, PaletteExt};

use super::root::AppView;

impl AppView {
    /// Renders the empty state.
    pub(crate) fn render_welcome(&self, cx: &mut Context<Self>) -> AnyElement {
        let palette = cx.palette();
        let home = std::env::home_dir();
        let mut column = v_flex()
            .w(px(WELCOME_WIDTH))
            .max_w_full()
            .gap(px(WELCOME_SECTION_GAP))
            .child(render_header(&palette))
            .child(
                section(t!("welcome.start"))
                    .child(self.action_row(
                        "open-file",
                        IconName::File,
                        t!("welcome.open_file"),
                        hints::OPEN_FILE,
                        false,
                        cx,
                    ))
                    .child(self.action_row(
                        "open-folder",
                        IconName::FolderOpen,
                        t!("welcome.open_folder"),
                        hints::OPEN_FOLDER,
                        true,
                        cx,
                    )),
            );
        if !self.recents.folders.is_empty() {
            column = column.child(
                section(t!("welcome.recent_folders")).children(
                    self.recents
                        .folders
                        .iter()
                        .enumerate()
                        .map(|(index, path)| {
                            self.recent_row(
                                ("folder", index),
                                IconName::Folder,
                                path,
                                home.as_deref(),
                                cx,
                            )
                        }),
                ),
            );
        }
        if !self.recents.files.is_empty() {
            column = column.child(section(t!("welcome.recent_files")).children(
                self.recents.files.iter().enumerate().map(|(index, path)| {
                    self.recent_row(
                        ("file", index),
                        IconName::FileText,
                        path,
                        home.as_deref(),
                        cx,
                    )
                }),
            ));
        }
        div()
            .id("welcome")
            .size_full()
            .overflow_y_scroll()
            .flex()
            .justify_center()
            .px_4()
            .py(px(64.0))
            .child(column)
            .into_any_element()
    }

    /// The Open file and Open folder buttons of an error state.
    pub(crate) fn render_open_buttons(&self, cx: &mut Context<Self>) -> AnyElement {
        h_flex()
            .gap_2()
            .child(
                SecondaryButton::new("error-open-file", t!("welcome.open_file"))
                    .icon(Icon::new(IconName::File))
                    .on_click(
                        cx.listener(|this, _, window, cx| this.prompt_open(false, window, cx)),
                    ),
            )
            .child(
                SecondaryButton::new("error-open-folder", t!("welcome.open_folder"))
                    .icon(Icon::new(IconName::FolderOpen))
                    .on_click(
                        cx.listener(|this, _, window, cx| this.prompt_open(true, window, cx)),
                    ),
            )
            .into_any_element()
    }

    /// A row that opens a dialog: icon, label and the shortcut hint at the right.
    fn action_row(
        &self,
        id: &'static str,
        icon: IconName,
        label: impl Into<SharedString>,
        hint: &'static str,
        folder: bool,
        cx: &mut Context<Self>,
    ) -> Stateful<Div> {
        let palette = cx.palette();
        row(id, &palette)
            .child(Icon::new(icon).small().text_color(palette.fg_muted))
            .child(div().flex_1().child(label.into()))
            .child(
                div()
                    .font_family(cx.theme().mono_font_family.clone())
                    .text_size(px(11.0))
                    .text_color(palette.fg_subtle)
                    .child(hint),
            )
            .on_click(cx.listener(move |this, _, window, cx| this.prompt_open(folder, window, cx)))
    }

    /// A recent folder or file: icon, name and the shortened path, which opens it when clicked.
    fn recent_row(
        &self,
        id: impl Into<ElementId>,
        icon: IconName,
        path: &Path,
        home: Option<&Path>,
        cx: &mut Context<Self>,
    ) -> Stateful<Div> {
        let palette = cx.palette();
        let target: PathBuf = path.to_path_buf();
        row(id, &palette)
            .child(Icon::new(icon).small().text_color(palette.fg_subtle))
            .child(
                div()
                    .flex_none()
                    .max_w(px(180.0))
                    .truncate()
                    .child(display_name(path)),
            )
            .child(
                div()
                    .flex_1()
                    .min_w_0()
                    .truncate()
                    .text_size(px(12.0))
                    .text_color(palette.fg_subtle)
                    .child(shorten_path(path, home)),
            )
            .on_click(cx.listener(move |this, _, window, cx| this.open_path(&target, window, cx)))
    }
}

/// The large logo, the name and the tagline.
fn render_header(palette: &Palette) -> impl IntoElement {
    h_flex()
        .gap_3()
        .child(
            div()
                .size(px(WELCOME_LOGO_SIZE))
                .flex_none()
                .rounded(px(10.0))
                .bg(palette.accent)
                .text_color(palette.accent_fg)
                .text_size(px(22.0))
                .font_weight(FontWeight::BOLD)
                .flex()
                .items_center()
                .justify_center()
                .child("t"),
        )
        .child(
            v_flex()
                .child(
                    div()
                        .text_size(px(20.0))
                        .font_weight(FontWeight::SEMIBOLD)
                        .child("ted"),
                )
                .child(
                    div()
                        .text_color(palette.fg_muted)
                        .child(t!("welcome.tagline")),
                ),
        )
}

/// A section: its label, then its rows.
fn section(label: impl Into<SharedString>) -> Div {
    v_flex()
        .gap(px(2.0))
        .child(div().px_2().pb_1().child(SectionLabel::new(label)))
}

/// The common row: 30 px, 8 px side padding, `hover` background on hover.
fn row(id: impl Into<ElementId>, palette: &Palette) -> Stateful<Div> {
    let hover = palette.hover;
    h_flex()
        .id(id)
        .h(px(WELCOME_ROW_HEIGHT))
        .px_2()
        .gap(px(10.0))
        .items_center()
        .rounded(px(RADIUS_MD - 2.0))
        .cursor_pointer()
        .hover(move |style| style.bg(hover))
}
