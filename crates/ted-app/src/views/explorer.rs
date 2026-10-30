//! The folder explorer of folder mode: the opened folder's tree, listed one folder at a time in the
//! background, rendered as a virtualized list (only the visible rows are built), and kept live by a
//! watcher on the expanded folders. The tree logic is `state::explorer`; this view lists, watches,
//! renders and takes the keyboard. Opening a file is an [`ExplorerEvent`] for the root view.

use std::collections::HashSet;
use std::path::{Path, PathBuf};
use std::sync::Arc;

use gpui_kit::assets::IconName;
use gpui_kit::component::*;
use gpui_kit::prelude::*;
use gpui_kit::*;
use rust_i18n::t;
use ted_workspace::{ExplorerFilter, FolderWatcher, list_dir};
use telaio_ui::components::{IconButton, SectionLabel, TreeRow};

use crate::state::explorer::{ExplorerTree, Listing};
use crate::state::launch::display_name;
use crate::theme::PaletteExt;
use crate::theme::metrics::{EXPLORER_HEADER_HEIGHT, EXPLORER_LIST_PADDING};

actions!(
    explorer,
    [
        /// Selects the row above.
        SelectPrevious,
        /// Selects the row below.
        SelectNext,
        /// Collapses the selected folder, or selects its parent.
        CollapseOrParent,
        /// Expands the selected folder, or selects its first child.
        ExpandOrChild,
        /// Opens the selected file, or toggles the selected folder.
        OpenSelected,
    ]
);

/// The key context of the explorer, for its bindings.
const KEY_CONTEXT: &str = "Explorer";

/// Registers the explorer's key bindings.
pub fn bind_keys(cx: &mut App) {
    let context = Some(KEY_CONTEXT);
    cx.bind_keys([
        KeyBinding::new("up", SelectPrevious, context),
        KeyBinding::new("down", SelectNext, context),
        KeyBinding::new("left", CollapseOrParent, context),
        KeyBinding::new("right", ExpandOrChild, context),
        KeyBinding::new("enter", OpenSelected, context),
    ]);
}

/// What the explorer tells the root view.
pub enum ExplorerEvent {
    /// A file was picked.
    Open(PathBuf),
}

/// The explorer of one opened folder.
pub struct ExplorerView {
    tree: ExplorerTree,
    filter: Arc<ExplorerFilter>,
    /// `None` when the OS refused a watcher: the tree still works, without live updates.
    watcher: Option<FolderWatcher>,
    watched: HashSet<PathBuf>,
    /// The file to show selected once its folders are listed.
    pending_reveal: Option<PathBuf>,
    scroll: UniformListScrollHandle,
    focus_handle: FocusHandle,
}

impl EventEmitter<ExplorerEvent> for ExplorerView {}

impl ExplorerView {
    /// The explorer of `root`, hiding `excludes` (the `explorer_exclude` setting).
    pub fn new(root: PathBuf, excludes: &[String], cx: &mut Context<Self>) -> Self {
        let filter = Arc::new(ExplorerFilter::new(&root, excludes));
        let watcher = match FolderWatcher::new() {
            Ok((watcher, changes)) => {
                // Sleeps on the channel: an idle explorer does no work.
                cx.spawn(async move |this, cx| {
                    while let Ok(dirs) = changes.recv().await {
                        let result = this.update(cx, |view, cx| {
                            for dir in dirs {
                                if view.tree.is_expanded(&dir) {
                                    view.list(dir, cx);
                                }
                            }
                        });
                        if result.is_err() {
                            break;
                        }
                    }
                })
                .detach();
                Some(watcher)
            }
            Err(error) => {
                log::warn!("the explorer will not follow changes on disk: {error}");
                None
            }
        };
        let mut view = Self {
            tree: ExplorerTree::new(root.clone()),
            filter,
            watcher,
            watched: HashSet::new(),
            pending_reveal: None,
            scroll: UniformListScrollHandle::new(),
            focus_handle: cx.focus_handle(),
        };
        view.list(root, cx);
        view
    }

    /// Hides `excludes` from now on: lists the root and the expanded folders again.
    pub fn set_excludes(&mut self, excludes: &[String], cx: &mut Context<Self>) {
        let root = self.tree.root().to_path_buf();
        self.filter = Arc::new(ExplorerFilter::new(&root, excludes));
        let dirs: Vec<PathBuf> = std::iter::once(root)
            .chain(self.tree.expanded().cloned())
            .collect();
        for dir in dirs {
            self.list(dir, cx);
        }
    }

    /// Shows `path` (the file in the editor) selected, expanding its folders.
    pub fn reveal(&mut self, path: &Path, cx: &mut Context<Self>) {
        self.pending_reveal = Some(path.to_path_buf());
        for dir in self.tree.reveal(path) {
            self.list(dir, cx);
        }
        self.scroll_to_selected();
        self.sync_watcher();
        cx.notify();
    }

    /// Lists `dir` in the background and stores the result.
    fn list(&mut self, dir: PathBuf, cx: &mut Context<Self>) {
        let filter = self.filter.clone();
        cx.spawn(async move |this, cx| {
            let started = std::time::Instant::now();
            let listed_dir = dir.clone();
            let result = cx
                .background_spawn(async move { list_dir(&filter, &listed_dir) })
                .await;
            let _ = this.update(cx, |view, cx| {
                let listing = match result {
                    Ok(entries) => {
                        log::debug!(
                            "listed {} entries in {:?}",
                            entries.len(),
                            started.elapsed()
                        );
                        Listing::Loaded(Arc::new(entries))
                    }
                    Err(error) => {
                        log::warn!("cannot list a folder: {error}");
                        Listing::Failed
                    }
                };
                view.tree.set_children(&dir, listing);
                if let Some(path) = view.pending_reveal.clone()
                    && path.starts_with(&dir)
                {
                    for dir in view.tree.reveal(&path) {
                        view.list(dir, cx);
                    }
                    view.scroll_to_selected();
                }
                view.sync_watcher();
                cx.notify();
            });
        })
        .detach();
    }

    /// Watches exactly the expanded folders.
    fn sync_watcher(&mut self) {
        let Some(watcher) = self.watcher.as_mut() else {
            return;
        };
        let expanded: HashSet<PathBuf> = self.tree.expanded().cloned().collect();
        for dir in self.watched.difference(&expanded) {
            watcher.unwatch(dir);
        }
        for dir in expanded.difference(&self.watched) {
            watcher.watch(dir);
        }
        self.watched = expanded;
    }

    fn scroll_to_selected(&self) {
        if let Some(index) = self.tree.selected_index() {
            self.scroll.scroll_to_item(index, ScrollStrategy::Nearest);
        }
    }

    /// A click or Enter on a row: a folder toggles, a file opens.
    fn activate(&mut self, path: PathBuf, folder: bool, cx: &mut Context<Self>) {
        self.tree.select(Some(path.clone()));
        if folder {
            if let Some(dir) = self.tree.toggle(&path) {
                self.list(dir, cx);
            }
            self.sync_watcher();
        } else {
            cx.emit(ExplorerEvent::Open(path));
        }
        cx.notify();
    }

    pub fn collapse_all(&mut self, cx: &mut Context<Self>) {
        self.tree.collapse_all();
        self.sync_watcher();
        cx.notify();
    }

    fn move_selection(&mut self, delta: isize, cx: &mut Context<Self>) {
        self.tree.move_selection(delta);
        self.scroll_to_selected();
        cx.notify();
    }

    fn open_selected(&mut self, cx: &mut Context<Self>) {
        let Some(index) = self.tree.selected_index() else {
            return;
        };
        let row = &self.tree.rows()[index];
        let (path, folder) = (row.path.clone(), row.folder.is_some());
        self.activate(path, folder, cx);
    }
}

impl Focusable for ExplorerView {
    fn focus_handle(&self, _cx: &App) -> FocusHandle {
        self.focus_handle.clone()
    }
}

impl Render for ExplorerView {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let palette = cx.palette();
        let root_name = display_name(self.tree.root());
        let row_count = self.tree.rows().len();
        let list = uniform_list(
            "explorer-rows",
            row_count,
            cx.processor(|view, range: std::ops::Range<usize>, _window, cx| {
                let palette = cx.palette();
                let selected = view.tree.selected().map(Path::to_path_buf);
                range
                    .filter_map(|index| view.tree.rows().get(index).map(|row| (index, row)))
                    .map(|(index, row)| {
                        let icon = match row.folder {
                            Some(true) => IconName::FolderOpen,
                            Some(false) => IconName::Folder,
                            None => IconName::File,
                        };
                        let path = row.path.clone();
                        let folder = row.folder.is_some();
                        let mut item = TreeRow::new(("row", index), row.name.clone())
                            .depth(row.depth)
                            .selected(selected.as_deref() == Some(row.path.as_path()))
                            .leading(Icon::new(icon).small().text_color(palette.fg_subtle))
                            .on_click(cx.listener(move |view, _, window, cx| {
                                window.focus(&view.focus_handle, cx);
                                view.activate(path.clone(), folder, cx);
                            }));
                        if let Some(expanded) = row.folder {
                            item = item.folder(expanded);
                        }
                        if row.loading {
                            item = item.detail("…");
                        }
                        item
                    })
                    .collect()
            }),
        )
        .track_scroll(&self.scroll)
        .flex_1()
        .px(px(EXPLORER_LIST_PADDING))
        .pb_2();

        v_flex()
            .id("explorer")
            .key_context(KEY_CONTEXT)
            .track_focus(&self.focus_handle)
            .size_full()
            .bg(palette.surface)
            .on_action(cx.listener(|view, _: &SelectPrevious, _, cx| view.move_selection(-1, cx)))
            .on_action(cx.listener(|view, _: &SelectNext, _, cx| view.move_selection(1, cx)))
            .on_action(cx.listener(|view, _: &CollapseOrParent, _, cx| {
                view.tree.left();
                view.sync_watcher();
                view.scroll_to_selected();
                cx.notify();
            }))
            .on_action(cx.listener(|view, _: &ExpandOrChild, _, cx| {
                if let Some(dir) = view.tree.right() {
                    view.list(dir, cx);
                }
                view.sync_watcher();
                view.scroll_to_selected();
                cx.notify();
            }))
            .on_action(cx.listener(|view, _: &OpenSelected, _, cx| view.open_selected(cx)))
            .child(
                h_flex()
                    .h(px(EXPLORER_HEADER_HEIGHT))
                    .flex_none()
                    .px_3()
                    .items_center()
                    .justify_between()
                    .child(SectionLabel::new(root_name))
                    .child(
                        IconButton::new("collapse-all", IconName::ListChevronsDownUp)
                            .tooltip(t!("explorer.collapse_all"))
                            .on_click(cx.listener(|view, _, _, cx| view.collapse_all(cx))),
                    ),
            )
            .child(list)
    }
}
