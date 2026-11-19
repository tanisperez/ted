//! The folder explorer of folder mode: the opened folder's tree, listed one folder at a time in the
//! background, rendered as a virtualized list (only the visible rows are built), and kept live by a
//! watcher on the expanded folders. The tree logic is `state::explorer`; this view lists, watches,
//! renders and takes the keyboard. Opening a file is an [`ExplorerEvent`] for the root view. A
//! right click on a row, or below the rows for the root, copies its path or shows it in the file
//! manager.

use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};
use std::sync::Arc;

use gpui_kit::assets::IconName;
use gpui_kit::component::menu::ContextMenuExt as _;
use gpui_kit::component::scroll::ScrollbarAxis;
use gpui_kit::component::*;
use gpui_kit::prelude::*;
use gpui_kit::*;
use rust_i18n::t;
use ted_workspace::{ExplorerFilter, FolderWatcher, list_dir, pointless_move};
use telaio_ui::components::{IconButton, SectionLabel, TreeRow};

use crate::state::explorer::{ExplorerTree, Listing};
use crate::state::file_icon::{FileKind, file_kind};
use crate::state::launch::display_name;
use crate::theme::Palette;
use crate::theme::PaletteExt;
use crate::theme::metrics::{EXPLORER_HEADER_HEIGHT, EXPLORER_LIST_PADDING};
use crate::views::git::letter_color;
use crate::views::location;
use crate::views::scrollbars::{SCROLLBAR_GUTTER, scrollbar_overlay};

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
    /// A file or a folder was dragged onto a folder: move `from` into `to_dir` (#98).
    Move { from: PathBuf, to_dir: PathBuf },
    /// The Select opened file button: reveal the file of the active tab (#121).
    RevealActive,
}

/// A file or a folder of the explorer while it is dragged, drawn as a chip under the pointer.
#[derive(Clone)]
pub struct DraggedPath {
    pub path: PathBuf,
    folder: bool,
    name: SharedString,
}

impl Render for DraggedPath {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let palette = cx.palette();
        let icon = if self.folder {
            IconName::Folder
        } else {
            IconName::File
        };
        h_flex()
            .h(px(26.0))
            .px(px(10.0))
            .gap(px(6.0))
            .items_center()
            .rounded(px(6.0))
            .bg(palette.overlay)
            .border_1()
            .border_color(palette.border_strong)
            .shadow_md()
            .text_size(px(12.5))
            .text_color(palette.fg)
            .child(Icon::new(icon).small().text_color(palette.fg_subtle))
            .child(self.name.clone())
    }
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
    /// The git status letters, by path relative to the root (`state::git::explorer_marks`).
    git_marks: Arc<HashMap<String, char>>,
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
            git_marks: Arc::default(),
        };
        view.list(root, cx);
        view
    }

    /// Shows the git status letters `marks` from now on.
    pub fn set_git_marks(&mut self, marks: Arc<HashMap<String, char>>, cx: &mut Context<Self>) {
        self.git_marks = marks;
        cx.notify();
    }

    /// Hides `excludes` from now on: lists the root and the expanded folders again.
    pub fn set_excludes(&mut self, excludes: &[String], cx: &mut Context<Self>) {
        let root = self.tree.root().to_path_buf();
        self.filter = Arc::new(ExplorerFilter::new(&root, excludes));
        self.reload(cx);
    }

    /// Lists the root and the expanded folders again, keeping what is expanded and selected
    /// (#123). The `.gitignore` files are read again too.
    pub fn reload(&mut self, cx: &mut Context<Self>) {
        let root = self.tree.root().to_path_buf();
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
        // Every expanded folder, not only the new ones: `watch` skips the folders already watched
        // and adds again those deleted and created anew since (#124).
        for dir in &expanded {
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
        let root_path = self.tree.root().to_path_buf();
        let row_count = self.tree.rows().len();
        let list = uniform_list(
            "explorer-rows",
            row_count,
            cx.processor(|view, range: std::ops::Range<usize>, window, cx| {
                let palette = cx.palette();
                let focused = view.focus_handle.is_focused(window);
                let selected = view.tree.selected().map(Path::to_path_buf);
                let root = view.tree.root().to_path_buf();
                let marks = view.git_marks.clone();
                range
                    .filter_map(|index| view.tree.rows().get(index).map(|row| (index, row)))
                    .map(|(index, row)| {
                        let (icon, icon_color) = match row.folder {
                            Some(true) => (IconName::FolderOpen, palette.fg_subtle),
                            Some(false) => (IconName::Folder, palette.fg_subtle),
                            None => file_icon(file_kind(&row.name), &palette),
                        };
                        let path = row.path.clone();
                        let folder = row.folder.is_some();
                        let mut item = TreeRow::new(("row", index), row.name.clone())
                            .depth(row.depth)
                            .selected(selected.as_deref() == Some(row.path.as_path()))
                            .neutral_selection(focused)
                            .leading(Icon::new(icon).small().text_color(icon_color))
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
                        let menu_path = row.path.clone();
                        let menu_root = root.clone();
                        if let Some(letter) = git_mark(&marks, &root, &row.path) {
                            // A folder shows a dot in the color of the change under it.
                            let text = if folder { '\u{25cf}' } else { letter };
                            item = item.suffix(
                                div()
                                    .w(px(12.0))
                                    .font_family(cx.theme().mono_font_family.clone())
                                    .text_size(px(if folder { 8.0 } else { 11.5 }))
                                    .font_weight(FontWeight::MEDIUM)
                                    .text_color(letter_color(letter, &palette))
                                    .child(text.to_string()),
                            );
                        }
                        // Dropped on a folder, into it; on a file, into the file's folder.
                        let drop_dir = if folder {
                            Some(row.path.clone())
                        } else {
                            row.path.parent().map(Path::to_path_buf)
                        };
                        let dragged = DraggedPath {
                            path: row.path.clone(),
                            folder,
                            name: row.name.clone().into(),
                        };
                        let target_dir = drop_dir.clone();
                        let (accent, accent_subtle) = (palette.accent, palette.accent_subtle);
                        // Its own right-click menu, which wins over the list's.
                        div()
                            .id(("row-menu", index))
                            .rounded(px(5.0))
                            .on_drag(dragged, |dragged, _, _, cx| cx.new(|_| dragged.clone()))
                            .when_some(drop_dir, |row, dir| {
                                row.drag_over::<DraggedPath>(move |style, dragged, _, _| {
                                    if folder && !pointless_move(&dragged.path, &dir) {
                                        style.bg(accent_subtle).border_1().border_color(accent)
                                    } else {
                                        style
                                    }
                                })
                            })
                            .on_drop(cx.listener(move |_, dragged: &DraggedPath, _, cx| {
                                if let Some(dir) = target_dir.clone() {
                                    cx.emit(ExplorerEvent::Move {
                                        from: dragged.path.clone(),
                                        to_dir: dir,
                                    });
                                }
                                cx.stop_propagation();
                            }))
                            .child(item)
                            .context_menu(move |menu, _, _| {
                                location::path_items(menu, &menu_path, folder, Some(&menu_root))
                            })
                            .into_any_element()
                    })
                    .collect()
            }),
        )
        .track_scroll(&self.scroll)
        .size_full()
        .pl(px(EXPLORER_LIST_PADDING))
        .pr(px(SCROLLBAR_GUTTER))
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
                        h_flex()
                            .gap(px(2.0))
                            .child(
                                IconButton::new("reveal-active", IconName::LocateFixed)
                                    .tooltip(t!("explorer.reveal_active"))
                                    .on_click(cx.listener(|_, _, _, cx| {
                                        cx.emit(ExplorerEvent::RevealActive)
                                    })),
                            )
                            .child(
                                IconButton::new("reload", IconName::RefreshCw)
                                    .tooltip(t!("explorer.reload"))
                                    .on_click(cx.listener(|view, _, _, cx| view.reload(cx))),
                            )
                            .child(
                                IconButton::new("collapse-all", IconName::ListChevronsDownUp)
                                    .tooltip(t!("explorer.collapse_all"))
                                    .on_click(cx.listener(|view, _, _, cx| view.collapse_all(cx))),
                            ),
                    ),
            )
            .child(
                div()
                    .relative()
                    .flex_1()
                    .min_h_0()
                    .child(list)
                    .child(scrollbar_overlay(
                        &self.scroll,
                        ScrollbarAxis::Vertical,
                        "explorer-scrollbar",
                        &palette,
                    )),
            )
            // Dropped below the rows: into the root folder.
            .on_drop(cx.listener({
                let root = root_path.clone();
                move |_, dragged: &DraggedPath, _, cx| {
                    cx.emit(ExplorerEvent::Move {
                        from: dragged.path.clone(),
                        to_dir: root.clone(),
                    });
                }
            }))
            // The empty area below the rows: the root folder's menu.
            .context_menu(move |menu, _, _| location::path_items(menu, &root_path, true, None))
    }
}

/// The icon of a file of `kind` and its color, as in the design (#108): code in `warning`, text
/// in `info`, data and configuration in `accent_text`, scripts in `success`, images in `danger`,
/// the rest in `fg_subtle`.
fn file_icon(kind: FileKind, palette: &Palette) -> (IconName, Hsla) {
    match kind {
        FileKind::Code => (IconName::FileCode, palette.warning),
        FileKind::Web => (IconName::FileCode, palette.warning),
        FileKind::Text => (IconName::FileText, palette.info),
        FileKind::Data => (IconName::FileCog, palette.accent_text),
        FileKind::Script => (IconName::FileTerminal, palette.success),
        FileKind::Image => (IconName::FileImage, palette.danger),
        FileKind::Archive => (IconName::FileArchive, palette.fg_muted),
        FileKind::Lock => (IconName::FileLock, palette.fg_subtle),
        FileKind::Other => (IconName::File, palette.fg_subtle),
    }
}

/// The git status letter of `path` (a row of the explorer of `root`), if it has one.
fn git_mark(marks: &HashMap<String, char>, root: &Path, path: &Path) -> Option<char> {
    if marks.is_empty() {
        return None;
    }
    let relative = path.strip_prefix(root).ok()?;
    let key: Vec<String> = relative
        .components()
        .map(|part| part.as_os_str().to_string_lossy().into_owned())
        .collect();
    marks.get(&key.join("/")).copied()
}
