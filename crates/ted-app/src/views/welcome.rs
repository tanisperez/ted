//! The empty state: what the window shows with nothing open. A centered column with the logo,
//! the Open folder, Open file and New file buttons, and the recent folders and files in two
//! columns, scrolled when it does not fit. The recent rows are the design's recents menu rows
//! (30 px, icon 14, name, path in 12 px `fg_subtle`), shown without the menu's card.

use std::path::{Path, PathBuf};

use gpui_kit::assets::IconName as AssetIcon;
use gpui_kit::component::scroll::Scrollbar;
use gpui_kit::component::*;
use gpui_kit::prelude::*;
use gpui_kit::*;
use rust_i18n::t;
use telaio_ui::components::{SecondaryButton, SectionLabel};

use super::logo::logo;

use crate::actions::hints;
use crate::state::launch::{display_name, shorten_path};
use crate::theme::metrics::{
    RADIUS_MD, WELCOME_BUTTON_HEIGHT, WELCOME_COLUMN_GAP, WELCOME_COLUMN_MIN_WIDTH,
    WELCOME_COLUMN_ROW_GAP, WELCOME_LOGO_SIZE, WELCOME_ROW_HEIGHT, WELCOME_SECTION_GAP,
    WELCOME_WIDTH,
};
use crate::theme::{Palette, PaletteExt};

use super::root::AppView;

impl AppView {
    /// Renders the empty state.
    pub(crate) fn render_welcome(&self, cx: &mut Context<Self>) -> AnyElement {
        let palette = cx.palette();
        let home = std::env::home_dir();
        let actions = h_flex()
            .flex_wrap()
            .gap_2()
            .child(self.action_button(
                "open-folder",
                AssetIcon::FolderOpen,
                t!("welcome.open_folder"),
                hints::OPEN_FOLDER,
                |this, window, cx| this.prompt_open(true, window, cx),
                cx,
            ))
            .child(self.action_button(
                "open-file",
                AssetIcon::FileInput,
                t!("welcome.open_file"),
                hints::OPEN_FILE,
                |this, window, cx| this.prompt_open(false, window, cx),
                cx,
            ))
            .child(self.action_button(
                "new-file",
                AssetIcon::FilePlus,
                t!("open.new_file"),
                hints::NEW_FILE,
                |this, window, cx| this.new_document(window, cx),
                cx,
            ));
        // The recents in two columns that wrap to one when the window is narrow, as the
        // design's `repeat(auto-fit, minmax(280px, 1fr))` grid. The rows' own padding lines
        // their text up with the actions above, so the columns reach 8 px past each side.
        let mut recents = h_flex()
            .flex_wrap()
            .items_start()
            .gap_x(px(WELCOME_COLUMN_GAP))
            .gap_y(px(WELCOME_COLUMN_ROW_GAP))
            .mx(px(-8.0));
        if !self.recents.folders.is_empty() {
            recents = recents.child(
                recents_column(t!("welcome.recent_folders")).children(
                    self.recents
                        .folders
                        .iter()
                        .enumerate()
                        .map(|(index, path)| {
                            self.recent_row(("folder", index), true, path, home.as_deref(), cx)
                        }),
                ),
            );
        }
        if !self.recents.files.is_empty() {
            recents = recents.child(recents_column(t!("welcome.recent_files")).children(
                self.recents.files.iter().enumerate().map(|(index, path)| {
                    self.recent_row(("file", index), false, path, home.as_deref(), cx)
                }),
            ));
        }
        let column = v_flex()
            .w_full()
            .max_w(px(WELCOME_WIDTH))
            .gap(px(WELCOME_SECTION_GAP))
            .child(render_header(&palette))
            .child(actions)
            .child(recents);
        let area = div()
            .id("welcome")
            .size_full()
            .overflow_y_scroll()
            .track_scroll(&self.welcome_scroll)
            .flex()
            .justify_center()
            // The column keeps its own height, so it overflows (and scrolls) instead of being
            // stretched to the window and cut.
            .items_start()
            .px_8()
            .pt(px(56.0))
            .pb(px(48.0))
            .child(column);
        // The scrollbar over the area, as Postino's welcome tab. It shows only when the column
        // does not fit.
        div()
            .relative()
            .size_full()
            .child(area)
            .child(
                div()
                    .absolute()
                    .inset_0()
                    .child(Scrollbar::vertical(&self.welcome_scroll)),
            )
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

    /// An action button: a raised 34 px button with the accent icon, the label and the
    /// shortcut hint.
    fn action_button(
        &self,
        id: &'static str,
        icon: AssetIcon,
        label: impl Into<SharedString>,
        hint: &'static str,
        action: impl Fn(&mut Self, &mut Window, &mut Context<Self>) + 'static,
        cx: &mut Context<Self>,
    ) -> Stateful<Div> {
        let palette = cx.palette();
        let (hover_border, hover_bg) = (palette.border_strong, palette.overlay);
        h_flex()
            .id(id)
            .h(px(WELCOME_BUTTON_HEIGHT))
            .px_3()
            .gap_2()
            .items_center()
            .rounded(px(RADIUS_MD))
            .bg(palette.raised)
            .border_1()
            .border_color(palette.border)
            .cursor_pointer()
            .hover(move |style| style.border_color(hover_border).bg(hover_bg))
            .child(Icon::new(icon).text_color(palette.accent_text))
            .child(div().font_weight(FontWeight::MEDIUM).child(label.into()))
            .child(
                div()
                    .ml_1()
                    .font_family(cx.theme().mono_font_family.clone())
                    .text_size(px(11.0))
                    .text_color(palette.fg_subtle)
                    .child(hint),
            )
            .on_click(cx.listener(move |this, _, window, cx| action(this, window, cx)))
    }

    /// A recent folder or file: icon, name and the shortened path (the folder that holds it, for
    /// a file), which opens it when clicked.
    fn recent_row(
        &self,
        id: impl Into<ElementId>,
        folder: bool,
        path: &Path,
        home: Option<&Path>,
        cx: &mut Context<Self>,
    ) -> Stateful<Div> {
        let palette = cx.palette();
        let target: PathBuf = path.to_path_buf();
        let (icon, location) = match folder {
            true => (IconName::Folder, path),
            false => (IconName::FileText, path.parent().unwrap_or(path)),
        };
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
                    .child(shorten_path(location, home)),
            )
            .on_click(cx.listener(move |this, _, window, cx| this.open_path(&target, window, cx)))
    }
}

/// The large logo, the name and the tagline.
fn render_header(palette: &Palette) -> impl IntoElement {
    h_flex().gap(px(14.0)).child(logo(WELCOME_LOGO_SIZE)).child(
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

/// A column of recents: its label, then its rows. It grows to fill its line and wraps below
/// the other one when the two do not fit side by side.
fn recents_column(label: impl Into<SharedString>) -> Div {
    v_flex()
        .flex_grow_1()
        .flex_basis(px(WELCOME_COLUMN_MIN_WIDTH))
        .min_w_0()
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
