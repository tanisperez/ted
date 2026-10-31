//! The document tabs of folder mode (telaio's `DocumentTabs`): one per open document, with its name,
//! the dirty dot and the close button. A click shows the file, the cross or a middle click closes
//! it, and dragging reorders.

use gpui_kit::assets::IconName;
use gpui_kit::prelude::*;
use telaio_ui::components::{DocumentTab, DocumentTabs};

use crate::state::launch::shorten_path;

use super::root::AppView;

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
            tabs = tabs.item(
                DocumentTab::new(name)
                    .icon(IconName::File)
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
        tabs
    }
}
