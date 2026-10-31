//! The document tabs of folder mode (telaio's `DocumentTabs`): one per open document, with its name,
//! the dirty dot and the close button. A click shows the file, the cross or a middle click closes
//! it, and dragging reorders. The + after the last tab creates a new document.

use gpui_kit::assets::IconName;
use gpui_kit::component::tooltip::Tooltip;
use gpui_kit::component::{Icon, Sizable};
use gpui_kit::prelude::*;
use gpui_kit::{SharedString, div, px};
use rust_i18n::t;
use telaio_ui::components::{DocumentTab, DocumentTabs};

use crate::actions::NewFile;
use crate::state::launch::shorten_path;
use crate::theme::PaletteExt;
use crate::theme::metrics::{ICON_BUTTON_LG, RADIUS_SM};

use super::root::{AppView, DocumentView};

impl AppView {
    /// Renders the tab bar.
    pub(crate) fn render_tabs(&self, cx: &mut Context<Self>) -> impl IntoElement {
        let weak = cx.weak_entity();
        let home = std::env::home_dir();
        let active = self.documents.active_index();
        let reorder = weak.clone();
        let mut tabs = DocumentTabs::new("document-tabs").on_reorder(move |from, to, _, cx| {
            let _ = reorder.update(cx, |view, cx| {
                view.documents.move_tab(from, to);
                cx.notify();
            });
        });
        for (index, document) in self.documents.items().iter().enumerate() {
            let dirty = document
                .editor()
                .is_some_and(|editor| editor.read(cx).state.has_unsaved());
            let id = document.id;
            let select = weak.clone();
            let close = weak.clone();
            let name = document.name();
            let tooltip = match &document.path {
                Some(path) => shorten_path(path, home.as_deref()),
                None => name.clone(),
            };
            let icon = match &document.view {
                DocumentView::Diff(_) => IconName::GitCompare,
                _ => IconName::File,
            };
            tabs = tabs.item(
                DocumentTab::new(name)
                    .icon(icon)
                    .dirty(dirty)
                    .selected(active == Some(index))
                    .tooltip(tooltip)
                    .on_click(move |window, cx| {
                        let _ = select.update(cx, |view, cx| {
                            if let Some(index) =
                                view.documents.position(|document| document.id == id)
                            {
                                view.activate_document(index, window, cx);
                            }
                        });
                    })
                    .on_close(move |window, cx| {
                        let _ = close.update(cx, |view, cx| view.close_document(id, window, cx));
                    }),
            );
        }
        // A large hover area around a small icon, as Postino's new request button; the bar pads
        // it off the last tab.
        let palette = cx.palette();
        let tooltip = SharedString::from(t!("open.new_file"));
        tabs.suffix(
            div()
                .id("new-document")
                .size(px(ICON_BUTTON_LG))
                .flex()
                .items_center()
                .justify_center()
                .cursor_pointer()
                .rounded(px(RADIUS_SM))
                .text_color(palette.fg_muted)
                .hover(|style| style.bg(palette.hover))
                .active(|style| style.bg(palette.pressed))
                .child(Icon::new(IconName::Plus).small())
                .tooltip(move |window, cx| {
                    Tooltip::new(tooltip.clone())
                        .action(&NewFile, None)
                        .build(window, cx)
                })
                .on_click(cx.listener(|this, _, window, cx| this.new_document(window, cx))),
        )
    }
}
