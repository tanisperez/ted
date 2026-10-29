//! The folder explorer's tree, outside the gpui view: which folders are listed and expanded, the
//! flattened rows the view shows, the selection and the keyboard moves. Plain Rust, so it is unit
//! tested without a window.
//!
//! Folders are listed lazily, one at a time, by `ted_workspace::list_dir` on a background thread:
//! expanding a folder that was never listed asks the view to list it ([`ExplorerTree::toggle`]
//! returns it), and the rows show it as loading until [`ExplorerTree::set_children`] arrives. The
//! rows are rebuilt only when the tree changes, never in a render, and a render only reads the
//! visible slice of them.

use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};
use std::sync::Arc;

use ted_workspace::{DirEntry, EntryKind};

/// What is known of a folder's children.
#[derive(Debug, Clone)]
pub enum Listing {
    /// Asked for, not arrived yet.
    Loading,
    Loaded(Arc<Vec<DirEntry>>),
    /// It could not be listed (permissions, removed meanwhile).
    Failed,
}

/// One visible row of the tree.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Row {
    pub path: PathBuf,
    pub name: String,
    pub depth: usize,
    /// `Some(expanded)` for a folder, `None` for a file.
    pub folder: Option<bool>,
    /// A folder expanded but still being listed.
    pub loading: bool,
}

/// The tree of one opened folder.
#[derive(Debug, Clone)]
pub struct ExplorerTree {
    root: PathBuf,
    listings: HashMap<PathBuf, Listing>,
    expanded: HashSet<PathBuf>,
    rows: Vec<Row>,
    /// The selected row's path: kept by path, so it survives rows being rebuilt.
    selected: Option<PathBuf>,
}

impl ExplorerTree {
    /// The tree of `root`, with the root itself expanded and waiting for its listing: the caller
    /// lists it.
    pub fn new(root: PathBuf) -> Self {
        let mut tree = Self {
            listings: HashMap::from([(root.clone(), Listing::Loading)]),
            expanded: HashSet::from([root.clone()]),
            root,
            rows: Vec::new(),
            selected: None,
        };
        tree.rebuild();
        tree
    }

    pub fn root(&self) -> &Path {
        &self.root
    }

    /// The visible rows, top to bottom.
    pub fn rows(&self) -> &[Row] {
        &self.rows
    }

    /// The expanded folders whose children are shown, for the watcher.
    pub fn expanded(&self) -> impl Iterator<Item = &PathBuf> {
        self.expanded.iter()
    }

    pub fn is_expanded(&self, path: &Path) -> bool {
        self.expanded.contains(path)
    }

    pub fn selected(&self) -> Option<&Path> {
        self.selected.as_deref()
    }

    /// The index of the selected row, if it is visible.
    pub fn selected_index(&self) -> Option<usize> {
        let selected = self.selected.as_deref()?;
        self.rows.iter().position(|row| row.path == selected)
    }

    pub fn select(&mut self, path: Option<PathBuf>) {
        self.selected = path;
    }

    /// Stores a folder's listing and rebuilds the rows. Folders that disappeared from it are
    /// forgotten (with their expansion), so a removed folder that comes back starts collapsed.
    pub fn set_children(&mut self, dir: &Path, listing: Listing) {
        if let Listing::Loaded(entries) = &listing {
            let kept: HashSet<&Path> = entries.iter().map(|entry| entry.path.as_path()).collect();
            let gone: Vec<PathBuf> = self
                .listings
                .keys()
                .chain(self.expanded.iter())
                .filter(|path| path.parent() == Some(dir) && !kept.contains(path.as_path()))
                .cloned()
                .collect();
            for path in gone {
                self.forget(&path);
            }
        }
        self.listings.insert(dir.to_path_buf(), listing);
        self.rebuild();
    }

    /// Forgets `path` and everything under it.
    fn forget(&mut self, path: &Path) {
        self.listings.retain(|key, _| !key.starts_with(path));
        self.expanded.retain(|key| !key.starts_with(path));
    }

    /// Expands or collapses the folder `path`. Returns the folder to list when it was never
    /// listed.
    pub fn toggle(&mut self, path: &Path) -> Option<PathBuf> {
        if self.expanded.contains(path) {
            self.expanded.remove(path);
            self.rebuild();
            None
        } else {
            self.expand(path)
        }
    }

    /// Expands the folder `path`. Returns it when it must be listed.
    pub fn expand(&mut self, path: &Path) -> Option<PathBuf> {
        self.expanded.insert(path.to_path_buf());
        let needs_listing = !matches!(self.listings.get(path), Some(Listing::Loaded(_)));
        if needs_listing {
            self.listings.insert(path.to_path_buf(), Listing::Loading);
        }
        self.rebuild();
        needs_listing.then(|| path.to_path_buf())
    }

    /// Collapses every folder but the root.
    pub fn collapse_all(&mut self) {
        let root = self.root.clone();
        self.expanded.retain(|path| *path == root);
        self.rebuild();
    }

    /// Expands the folders from the root down to `path`'s parent and selects it. Returns the
    /// folders that must be listed first; call again when they arrive to finish revealing.
    pub fn reveal(&mut self, path: &Path) -> Vec<PathBuf> {
        let Ok(relative) = path.strip_prefix(&self.root) else {
            return Vec::new();
        };
        let mut to_list = Vec::new();
        let mut dir = self.root.clone();
        let components: Vec<_> = relative.components().collect();
        for component in components.iter().take(components.len().saturating_sub(1)) {
            dir.push(component);
            self.expanded.insert(dir.clone());
            if !matches!(
                self.listings.get(&dir),
                Some(Listing::Loaded(_) | Listing::Loading)
            ) {
                self.listings.insert(dir.clone(), Listing::Loading);
                to_list.push(dir.clone());
            }
        }
        self.selected = Some(path.to_path_buf());
        self.rebuild();
        to_list
    }

    /// Moves the selection by `delta` rows, clamped. With nothing selected it starts at the first
    /// row.
    pub fn move_selection(&mut self, delta: isize) {
        if self.rows.is_empty() {
            return;
        }
        let index = match self.selected_index() {
            Some(index) => (index as isize + delta).clamp(0, self.rows.len() as isize - 1) as usize,
            None => 0,
        };
        self.selected = Some(self.rows[index].path.clone());
    }

    /// Left: collapses the selected folder, or selects the parent folder.
    pub fn left(&mut self) {
        let Some(index) = self.selected_index() else {
            return;
        };
        let row = self.rows[index].clone();
        if row.folder == Some(true) {
            self.expanded.remove(&row.path);
            self.rebuild();
        } else if let Some(parent) = row.path.parent()
            && parent != self.root
        {
            self.selected = Some(parent.to_path_buf());
        }
    }

    /// Right: expands the selected folder, or selects its first child when already expanded.
    /// Returns a folder to list.
    pub fn right(&mut self) -> Option<PathBuf> {
        let index = self.selected_index()?;
        let row = self.rows[index].clone();
        match row.folder {
            Some(false) => self.expand(&row.path),
            Some(true) => {
                if let Some(child) = self.rows.get(index + 1)
                    && child.depth > row.depth
                {
                    self.selected = Some(child.path.clone());
                }
                None
            }
            None => None,
        }
    }

    /// Flattens the expanded tree into rows, depth first. The root itself is not a row.
    fn rebuild(&mut self) {
        let mut rows = Vec::with_capacity(self.rows.len());
        self.push_children(&self.root.clone(), 0, &mut rows);
        self.rows = rows;
    }

    fn push_children(&self, dir: &Path, depth: usize, rows: &mut Vec<Row>) {
        let Some(Listing::Loaded(entries)) = self.listings.get(dir) else {
            return;
        };
        for entry in entries.iter() {
            let is_dir = entry.kind == EntryKind::Dir;
            let expanded = is_dir && self.expanded.contains(&entry.path);
            rows.push(Row {
                path: entry.path.clone(),
                name: entry.name.clone(),
                depth,
                folder: is_dir.then_some(expanded),
                loading: expanded
                    && matches!(
                        self.listings.get(&entry.path),
                        Some(Listing::Loading) | None
                    ),
            });
            if expanded {
                self.push_children(&entry.path, depth + 1, rows);
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use pretty_assertions::assert_eq;

    fn entry(path: &str, dir: bool) -> DirEntry {
        let path = PathBuf::from(path);
        DirEntry {
            name: path
                .file_name()
                .map(|n| n.to_string_lossy().into_owned())
                .unwrap_or_default(),
            path,
            kind: if dir { EntryKind::Dir } else { EntryKind::File },
            is_symlink: false,
        }
    }

    fn loaded(entries: Vec<DirEntry>) -> Listing {
        Listing::Loaded(Arc::new(entries))
    }

    fn names(tree: &ExplorerTree) -> Vec<String> {
        tree.rows()
            .iter()
            .map(|row| format!("{}{}", "  ".repeat(row.depth), row.name))
            .collect()
    }

    fn sample() -> ExplorerTree {
        let mut tree = ExplorerTree::new(PathBuf::from("/r"));
        tree.set_children(
            Path::new("/r"),
            loaded(vec![entry("/r/src", true), entry("/r/README.md", false)]),
        );
        tree
    }

    #[test]
    fn the_root_lists_its_children_flat() {
        let tree = sample();
        assert_eq!(names(&tree), vec!["src", "README.md"]);
        assert_eq!(tree.rows()[0].folder, Some(false));
        assert_eq!(tree.rows()[1].folder, None);
    }

    #[test]
    fn expanding_asks_for_a_listing_once() {
        let mut tree = sample();
        assert_eq!(
            tree.toggle(Path::new("/r/src")),
            Some(PathBuf::from("/r/src"))
        );
        assert!(tree.rows()[0].loading);
        tree.set_children(
            Path::new("/r/src"),
            loaded(vec![entry("/r/src/main.rs", false)]),
        );
        assert_eq!(names(&tree), vec!["src", "  main.rs", "README.md"]);
        assert_eq!(tree.toggle(Path::new("/r/src")), None);
        assert_eq!(names(&tree), vec!["src", "README.md"]);
        // Listed already: expanding again needs no listing.
        assert_eq!(tree.toggle(Path::new("/r/src")), None);
        assert_eq!(tree.rows().len(), 3);
    }

    #[test]
    fn keyboard_moves() {
        let mut tree = sample();
        tree.move_selection(1);
        assert_eq!(tree.selected(), Some(Path::new("/r/src")));
        assert_eq!(tree.right(), Some(PathBuf::from("/r/src")));
        tree.set_children(
            Path::new("/r/src"),
            loaded(vec![entry("/r/src/main.rs", false)]),
        );
        assert_eq!(tree.right(), None);
        assert_eq!(tree.selected(), Some(Path::new("/r/src/main.rs")));
        tree.left();
        assert_eq!(tree.selected(), Some(Path::new("/r/src")));
        tree.left();
        assert_eq!(names(&tree), vec!["src", "README.md"]);
        tree.move_selection(5);
        assert_eq!(tree.selected(), Some(Path::new("/r/README.md")));
        tree.move_selection(-9);
        assert_eq!(tree.selected(), Some(Path::new("/r/src")));
    }

    #[test]
    fn revealing_expands_the_ancestors() {
        let mut tree = sample();
        let to_list = tree.reveal(Path::new("/r/src/a/b.rs"));
        assert_eq!(
            to_list,
            vec![PathBuf::from("/r/src"), PathBuf::from("/r/src/a")]
        );
        tree.set_children(Path::new("/r/src"), loaded(vec![entry("/r/src/a", true)]));
        tree.set_children(
            Path::new("/r/src/a"),
            loaded(vec![entry("/r/src/a/b.rs", false)]),
        );
        assert_eq!(names(&tree), vec!["src", "  a", "    b.rs", "README.md"]);
        assert_eq!(tree.selected_index(), Some(2));
        // Outside the root: nothing happens.
        assert!(tree.reveal(Path::new("/elsewhere/x")).is_empty());
    }

    #[test]
    fn a_removed_folder_is_forgotten() {
        let mut tree = sample();
        tree.toggle(Path::new("/r/src"));
        tree.set_children(Path::new("/r/src"), loaded(vec![]));
        tree.set_children(Path::new("/r"), loaded(vec![entry("/r/README.md", false)]));
        assert!(!tree.is_expanded(Path::new("/r/src")));
        tree.set_children(
            Path::new("/r"),
            loaded(vec![entry("/r/src", true), entry("/r/README.md", false)]),
        );
        assert_eq!(tree.rows()[0].folder, Some(false));
    }

    #[test]
    fn collapse_all_keeps_the_root() {
        let mut tree = sample();
        tree.toggle(Path::new("/r/src"));
        tree.set_children(Path::new("/r/src"), loaded(vec![entry("/r/src/x", false)]));
        tree.collapse_all();
        assert_eq!(names(&tree), vec!["src", "README.md"]);
    }

    #[test]
    fn a_big_folder_flattens_quickly() {
        let mut tree = ExplorerTree::new(PathBuf::from("/r"));
        let entries = (0..100_000)
            .map(|index| entry(&format!("/r/f{index}"), false))
            .collect();
        let start = std::time::Instant::now();
        tree.set_children(Path::new("/r"), loaded(entries));
        assert_eq!(tree.rows().len(), 100_000);
        // Generous even for a debug build; it is about 10 ms in release.
        assert!(start.elapsed().as_secs() < 5);
    }
}
