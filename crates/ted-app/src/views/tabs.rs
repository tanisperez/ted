//! The document tabs (telaio's `DocumentTabs`): one per open document, with its name, the dirty
//! dot and the close button. A click shows the file, the cross or a middle click closes it,
//! dragging reorders, and a right click opens telaio's standard tab menu (the Close items and the
//! path items, #95). The + after the last tab creates a new document.

use gpui_kit::assets::IconName;
use gpui_kit::component::tooltip::Tooltip;
use gpui_kit::component::{Icon, Sizable};
use gpui_kit::prelude::*;
use gpui_kit::{SharedString, div, px};
use rust_i18n::t;
use telaio_ui::components::{DocumentTab, DocumentTabs};

use gpui_kit::WeakEntity;
use gpui_kit::component::menu::PopupMenu;
use telaio_ui::components::{PathItems, TabMenuTarget, path_items, tab_close_items};

use crate::actions::{CloseTab, CopyPath, NewFile, RevealInFileManager};
use crate::state::launch::relative_path;
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
        let dirty_flags = self.dirty_flags(cx);
        let reorder = weak.clone();
        let mut tabs = DocumentTabs::new("document-tabs")
            .selected_bounds(self.tab_bounds.clone())
            .on_reorder(move |from, to, _, cx| {
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
            let menu_weak = weak.clone();
            let menu_target = TabMenuTarget::new(index, &dirty_flags);
            let menu_path = document.path.clone();
            let menu_root = match &self.target {
                Some(crate::state::launch::Target::Folder(root)) => Some(root.clone()),
                _ => None,
            };
            let tooltip = match &document.path {
                Some(path) => shorten_path(path, home.as_deref()),
                None => name.clone(),
            };
            let icon = match &document.view {
                DocumentView::Diff(_) => IconName::GitCompare,
                DocumentView::History(_) => IconName::GitGraph,
                DocumentView::FindResults(_) => IconName::Search,
                DocumentView::Preview(_) => IconName::Eye,
                DocumentView::WhatsNew(_) => IconName::Sparkles,
                _ => IconName::File,
            };
            let mut tab = DocumentTab::new(name);
            if matches!(document.view, DocumentView::Preview(_)) {
                tab = tab.detail(t!("markdown.tab_detail"));
            }
            tabs = tabs.item(
                tab.icon(icon)
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
                    })
                    .context_menu(move |menu, _, _| {
                        tab_menu(
                            menu,
                            id,
                            menu_target,
                            &menu_weak,
                            menu_path.as_deref(),
                            menu_root.as_deref(),
                        )
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

/// The right-click menu of a tab (telaio's standard one, as Postino's): Close, Close others,
/// to the right, to the left, saved and all, then the path items of a file.
pub(crate) fn tab_menu(
    menu: PopupMenu,
    id: u64,
    target: TabMenuTarget,
    weak: &WeakEntity<AppView>,
    path: Option<&std::path::Path>,
    root: Option<&std::path::Path>,
) -> PopupMenu {
    let close = weak.clone();
    let menu = tab_close_items(menu, &target, Some(&CloseTab), move |scope, window, cx| {
        let _ = close.update(cx, |view, cx| view.close_scope(id, scope, window, cx));
    });
    match path {
        Some(path) => path_items(
            menu.separator(),
            PathItems {
                path: path.to_path_buf(),
                relative: root.and_then(|root| relative_path(root, path)),
                folder: false,
                copy_action: Some(Box::new(CopyPath)),
                reveal_action: Some(Box::new(RevealInFileManager)),
            },
        ),
        None => menu,
    }
}
