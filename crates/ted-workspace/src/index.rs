//! The list of every file of a folder, for the global search. Walked once per request on the
//! background executor, honoring the same `.gitignore` files and exclude patterns as the explorer
//! ([`ExplorerFilter`]). The paths are kept relative to the root, `/` separated, in one string
//! with their end offsets, so a folder with 100 000 files costs two allocations, not 100 000.

use std::fs;
use std::path::{Path, PathBuf};
use std::time::Instant;

use crate::list::{ExplorerFilter, natural_cmp};

/// Every file under a root, in the explorer's order: each folder's files before its subfolders',
/// both in natural order.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct FileIndex {
    root: PathBuf,
    /// The relative paths, one after the other.
    text: String,
    /// The end of each path in `text`.
    ends: Vec<u32>,
    /// More files than the limit: the rest was left out.
    truncated: bool,
}

impl FileIndex {
    /// The folder the paths are relative to.
    pub fn root(&self) -> &Path {
        &self.root
    }

    pub fn len(&self) -> usize {
        self.ends.len()
    }

    pub fn is_empty(&self) -> bool {
        self.ends.is_empty()
    }

    /// Whether the folder has more files than the limit given to [`index_files`].
    pub fn truncated(&self) -> bool {
        self.truncated
    }

    /// The relative path of file `index`, `/` separated (`src/main.rs`).
    ///
    /// # Panics
    ///
    /// When `index` is not below [`FileIndex::len`].
    pub fn get(&self, index: usize) -> &str {
        let start = match index {
            0 => 0,
            _ => self.ends[index - 1] as usize,
        };
        &self.text[start..self.ends[index] as usize]
    }

    /// The relative paths, in order.
    pub fn iter(&self) -> impl Iterator<Item = &str> {
        (0..self.len()).map(|index| self.get(index))
    }

    /// The full path of file `index`.
    pub fn absolute(&self, index: usize) -> PathBuf {
        let mut path = self.root.clone();
        path.extend(self.get(index).split('/'));
        path
    }

    /// Adds the path `prefix` + `name`.
    fn push(&mut self, prefix: &str, name: &str) {
        let start = self.text.len();
        self.text.push_str(prefix);
        self.text.push_str(name);
        // The limit keeps the text far below 4 GiB; a path past it is dropped, never wrapped.
        match u32::try_from(self.text.len()) {
            Ok(end) => self.ends.push(end),
            Err(_) => {
                self.text.truncate(start);
                self.truncated = true;
            }
        }
    }
}

/// Lists every file under `root` that the explorer would show, up to `max_files`. Blocking: call
/// it on the background executor. Folders are read one by one, deepest last; `.git` folders are
/// always skipped, symlinks to folders are not followed (they could loop) and names that are not
/// valid UTF-8 are left out. A folder that cannot be read is skipped.
pub fn index_files(filter: &ExplorerFilter, root: &Path, max_files: usize) -> FileIndex {
    let started = Instant::now();
    let mut index = FileIndex {
        root: root.to_path_buf(),
        ..FileIndex::default()
    };
    // (folder, its path relative to the root with a trailing `/`, or empty for the root).
    let mut pending: Vec<(PathBuf, String)> = vec![(root.to_path_buf(), String::new())];
    let mut files: Vec<String> = Vec::new();
    let mut dirs: Vec<(String, PathBuf)> = Vec::new();
    'walk: while let Some((dir, prefix)) = pending.pop() {
        let Ok(read) = fs::read_dir(&dir) else {
            log::debug!("index: cannot read {}", dir.display());
            continue;
        };
        let levels = filter.levels(&dir);
        files.clear();
        dirs.clear();
        for item in read.flatten() {
            let Ok(file_type) = item.file_type() else {
                continue;
            };
            let Ok(name) = item.file_name().into_string() else {
                continue;
            };
            let path = item.path();
            let is_dir = file_type.is_dir();
            if is_dir && name == ".git" {
                continue;
            }
            if file_type.is_symlink() && fs::metadata(&path).is_ok_and(|meta| meta.is_dir()) {
                continue;
            }
            if filter.is_hidden(&levels, &path, is_dir) {
                continue;
            }
            match is_dir {
                true => dirs.push((name, path)),
                false => files.push(name),
            }
        }
        files.sort_unstable_by(|a, b| natural_cmp(&a.to_lowercase(), &b.to_lowercase()));
        for name in &files {
            if index.len() >= max_files {
                index.truncated = true;
                break 'walk;
            }
            index.push(&prefix, name);
        }
        // Reversed, so the first folder in natural order is popped first.
        dirs.sort_unstable_by(|(a, _), (b, _)| natural_cmp(&b.to_lowercase(), &a.to_lowercase()));
        for (name, path) in dirs.drain(..) {
            pending.push((path, format!("{prefix}{name}/")));
        }
    }
    log::info!(
        "indexed {} files under {} in {:?}{}",
        index.len(),
        root.display(),
        started.elapsed(),
        if index.truncated { " (truncated)" } else { "" }
    );
    index
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use super::*;
    use pretty_assertions::assert_eq;

    fn touch(root: &Path, rel: &str) {
        let path = root.join(rel);
        fs::create_dir_all(path.parent().unwrap()).unwrap();
        fs::write(path, "x").unwrap();
    }

    fn paths(index: &FileIndex) -> Vec<&str> {
        index.iter().collect()
    }

    #[test]
    fn lists_every_file_in_the_explorer_order() {
        let tmp = tempfile::tempdir().unwrap();
        for rel in [
            "b.txt",
            "a10.txt",
            "a2.txt",
            "src/main.rs",
            "src/lib/x.rs",
            "docs/y.md",
        ] {
            touch(tmp.path(), rel);
        }
        let filter = ExplorerFilter::new(tmp.path(), &[]);
        let index = index_files(&filter, tmp.path(), 100);
        assert_eq!(
            paths(&index),
            vec![
                "a2.txt",
                "a10.txt",
                "b.txt",
                "docs/y.md",
                "src/main.rs",
                "src/lib/x.rs"
            ]
        );
        assert!(!index.truncated());
        assert_eq!(
            index.absolute(5),
            tmp.path().join("src").join("lib").join("x.rs")
        );
    }

    #[test]
    fn honors_gitignore_and_the_exclude_patterns() {
        let tmp = tempfile::tempdir().unwrap();
        fs::write(tmp.path().join(".gitignore"), "*.log\n").unwrap();
        for rel in [
            "keep.rs",
            "out.log",
            "target/debug/ted",
            "node_modules/a/b.js",
            ".git/HEAD",
        ] {
            touch(tmp.path(), rel);
        }
        let filter = ExplorerFilter::new(tmp.path(), &["target".into(), "node_modules".into()]);
        let index = index_files(&filter, tmp.path(), 100);
        assert_eq!(paths(&index), vec![".gitignore", "keep.rs"]);
    }

    #[test]
    fn skips_git_folders_even_when_not_excluded() {
        let tmp = tempfile::tempdir().unwrap();
        touch(tmp.path(), ".git/HEAD");
        touch(tmp.path(), "a.txt");
        let filter = ExplorerFilter::new(tmp.path(), &[]);
        assert_eq!(paths(&index_files(&filter, tmp.path(), 100)), vec!["a.txt"]);
    }

    #[test]
    fn stops_at_the_limit() {
        let tmp = tempfile::tempdir().unwrap();
        for number in 0..10 {
            touch(tmp.path(), &format!("f{number}.txt"));
        }
        let filter = ExplorerFilter::new(tmp.path(), &[]);
        let index = index_files(&filter, tmp.path(), 4);
        assert_eq!(index.len(), 4);
        assert!(index.truncated());
    }

    #[cfg(unix)]
    #[test]
    fn does_not_follow_symlinked_folders() {
        let tmp = tempfile::tempdir().unwrap();
        touch(tmp.path(), "real/a.txt");
        std::os::unix::fs::symlink(tmp.path().join("real"), tmp.path().join("loop")).unwrap();
        let filter = ExplorerFilter::new(tmp.path(), &[]);
        assert_eq!(
            paths(&index_files(&filter, tmp.path(), 100)),
            vec!["real/a.txt"]
        );
    }
}
