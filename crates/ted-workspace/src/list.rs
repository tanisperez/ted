//! Listing one folder for the explorer: sorted folders first, natural order, without the entries
//! that `.gitignore` or the "Exclude from explorer" patterns hide.

use std::cmp::Ordering;
use std::collections::HashMap;
use std::fs;
use std::io;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};
use std::time::Instant;

use ignore::Match;
use ignore::gitignore::{Gitignore, GitignoreBuilder};

/// What an entry of a listing is. A symlink counts as what it points to.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EntryKind {
    File,
    Dir,
}

/// One visible entry of a folder.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DirEntry {
    pub name: String,
    pub path: PathBuf,
    pub kind: EntryKind,
    /// The entry is a symlink (to a file, a folder or nothing at all). `kind` is its target's.
    pub is_symlink: bool,
}

/// Why a folder could not be listed. Every variant carries the path, for the error message.
#[derive(thiserror::Error, Debug)]
pub enum ListError {
    #[error("{} does not exist", .0.display())]
    NotFound(PathBuf),
    #[error("{} is not a directory", .0.display())]
    NotADirectory(PathBuf),
    #[error("permission denied reading {}", .0.display())]
    PermissionDenied(PathBuf),
    #[error("cannot list {}: {source}", path.display())]
    Io {
        path: PathBuf,
        #[source]
        source: io::Error,
    },
}

impl ListError {
    fn new(path: &Path, source: io::Error) -> Self {
        match source.kind() {
            io::ErrorKind::NotFound => Self::NotFound(path.to_path_buf()),
            io::ErrorKind::NotADirectory => Self::NotADirectory(path.to_path_buf()),
            io::ErrorKind::PermissionDenied => Self::PermissionDenied(path.to_path_buf()),
            _ => Self::Io {
                path: path.to_path_buf(),
                source,
            },
        }
    }
}

/// Decides which entries the explorer hides. Built once per opened root and shared (`Arc`)
/// between background tasks. The `.gitignore` of each folder is read the first time that folder
/// is listed, then cached for the life of the filter.
pub struct ExplorerFilter {
    root: PathBuf,
    /// The user patterns of the setting, relative to the root. Checked first.
    user: Gitignore,
    /// `.git/info/exclude` of the root, lowest of the per-repository sources.
    info_exclude: Gitignore,
    /// The user's global gitignore, lowest priority of all.
    global: Gitignore,
    /// The `.gitignore` of each folder already visited (`None`: it has none).
    gitignores: Mutex<HashMap<PathBuf, Option<Arc<Gitignore>>>>,
}

impl ExplorerFilter {
    /// `exclude_patterns` use the gitignore glob syntax (`target`, `*.log`, `/build`, `!keep`).
    /// An invalid pattern is logged and skipped.
    pub fn new(root: &Path, exclude_patterns: &[String]) -> Self {
        let mut user = GitignoreBuilder::new(root);
        for pattern in exclude_patterns {
            let pattern = pattern.trim();
            if pattern.is_empty() {
                continue;
            }
            if let Err(err) = user.add_line(None, pattern) {
                log::warn!("ignoring explorer exclude pattern {pattern:?}: {err}");
            }
        }
        let user = user.build().unwrap_or_else(|err| {
            log::warn!("cannot build the explorer exclude patterns: {err}");
            Gitignore::empty()
        });

        let mut info = GitignoreBuilder::new(root);
        let info_path = root.join(".git").join("info").join("exclude");
        if info_path.is_file()
            && let Some(err) = info.add(&info_path)
        {
            log::warn!("cannot read {}: {err}", info_path.display());
        }
        let info_exclude = info.build().unwrap_or_else(|_| Gitignore::empty());

        // Reads ~/.gitconfig and one ignore file, a few small reads.
        let (global, err) = Gitignore::global();
        if let Some(err) = err {
            log::debug!("global gitignore not used: {err}");
        }

        Self {
            root: root.to_path_buf(),
            user,
            info_exclude,
            global,
            gitignores: Mutex::new(HashMap::new()),
        }
    }

    /// The matchers of `dir` and its ancestors up to the root, deepest first.
    fn levels(&self, dir: &Path) -> Vec<Arc<Gitignore>> {
        let mut dirs: Vec<&Path> = if dir.starts_with(&self.root) {
            dir.ancestors()
                .take_while(|d| d.starts_with(&self.root))
                .collect()
        } else {
            vec![dir]
        };
        // `ancestors` is already deepest first.
        let mut out = Vec::with_capacity(dirs.len());
        let mut cache = self
            .gitignores
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        for d in dirs.drain(..) {
            let entry = cache.entry(d.to_path_buf()).or_insert_with(|| {
                let path = d.join(".gitignore");
                let mut builder = GitignoreBuilder::new(d);
                match builder.add(&path) {
                    // A missing file is the common case and not an error.
                    Some(err) if !is_not_found(&err) => {
                        log::warn!("cannot read {}: {err}", path.display());
                    }
                    _ => {}
                }
                match builder.build() {
                    Ok(gi) if !gi.is_empty() => Some(Arc::new(gi)),
                    _ => None,
                }
            });
            if let Some(gi) = entry {
                out.push(Arc::clone(gi));
            }
        }
        out
    }

    fn is_hidden(&self, levels: &[Arc<Gitignore>], path: &Path, is_dir: bool) -> bool {
        if matches!(self.user.matched(path, is_dir), Match::Ignore(_)) {
            return true;
        }
        for gi in levels {
            match gi.matched(path, is_dir) {
                Match::Ignore(_) => return true,
                Match::Whitelist(_) => return false,
                Match::None => {}
            }
        }
        for gi in [&self.info_exclude, &self.global] {
            if gi.is_empty() {
                continue;
            }
            match gi.matched(path, is_dir) {
                Match::Ignore(_) => return true,
                Match::Whitelist(_) => return false,
                Match::None => {}
            }
        }
        false
    }
}

fn is_not_found(err: &ignore::Error) -> bool {
    match err {
        ignore::Error::Io(e) => e.kind() == io::ErrorKind::NotFound,
        ignore::Error::WithPath { err, .. } => is_not_found(err),
        _ => false,
    }
}

/// Lists the visible children of `dir`: folders first, then files, each group in natural order.
/// Blocking. Reads the folder once; the only extra `stat` is for symlinks, to know if they point
/// to a folder. An entry that vanishes or cannot be read while listing is skipped.
pub fn list_dir(filter: &ExplorerFilter, dir: &Path) -> Result<Vec<DirEntry>, ListError> {
    let started = Instant::now();
    let read = fs::read_dir(dir).map_err(|err| ListError::new(dir, err))?;
    let levels = filter.levels(dir);

    let mut keyed: Vec<(String, DirEntry)> = Vec::new();
    for item in read {
        let item = match item {
            Ok(item) => item,
            Err(err) => {
                log::debug!("skipping an entry of {}: {err}", dir.display());
                continue;
            }
        };
        let Ok(file_type) = item.file_type() else {
            continue;
        };
        let path = item.path();
        let is_symlink = file_type.is_symlink();
        let is_dir = if is_symlink {
            fs::metadata(&path).map(|m| m.is_dir()).unwrap_or(false)
        } else {
            file_type.is_dir()
        };
        if filter.is_hidden(&levels, &path, is_dir) {
            continue;
        }
        let name = item.file_name().to_string_lossy().into_owned();
        keyed.push((
            name.to_lowercase(),
            DirEntry {
                name,
                path,
                kind: if is_dir {
                    EntryKind::Dir
                } else {
                    EntryKind::File
                },
                is_symlink,
            },
        ));
    }

    keyed.sort_unstable_by(|(ka, a), (kb, b)| {
        (a.kind == EntryKind::File)
            .cmp(&(b.kind == EntryKind::File))
            .then_with(|| natural_cmp(ka, kb))
            // Stable tie break for names that differ only in case.
            .then_with(|| a.name.cmp(&b.name))
    });
    let entries: Vec<DirEntry> = keyed.into_iter().map(|(_, e)| e).collect();
    log::debug!(
        "listed {} ({} entries) in {:?}",
        dir.display(),
        entries.len(),
        started.elapsed()
    );
    Ok(entries)
}

/// Compares two strings with their digit runs as numbers: `file2` before `file10`.
fn natural_cmp(a: &str, b: &str) -> Ordering {
    let (mut a, mut b) = (a.as_bytes(), b.as_bytes());
    while let (Some(&ca), Some(&cb)) = (a.first(), b.first()) {
        if ca.is_ascii_digit() && cb.is_ascii_digit() {
            let (na, rest_a) = split_digits(a);
            let (nb, rest_b) = split_digits(b);
            let (ta, tb) = (strip_zeros(na), strip_zeros(nb));
            let ord = ta.len().cmp(&tb.len()).then_with(|| ta.cmp(tb));
            if ord != Ordering::Equal {
                return ord;
            }
            // Same value: fewer leading zeros first, so the order stays total.
            let ord = na.len().cmp(&nb.len());
            if ord != Ordering::Equal {
                return ord;
            }
            (a, b) = (rest_a, rest_b);
        } else {
            if ca != cb {
                return ca.cmp(&cb);
            }
            (a, b) = (&a[1..], &b[1..]);
        }
    }
    a.len().cmp(&b.len())
}

fn split_digits(s: &[u8]) -> (&[u8], &[u8]) {
    let end = s
        .iter()
        .position(|c| !c.is_ascii_digit())
        .unwrap_or(s.len());
    s.split_at(end)
}

fn strip_zeros(s: &[u8]) -> &[u8] {
    let start = s.iter().position(|&c| c != b'0').unwrap_or(s.len());
    &s[start..]
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use super::*;
    use std::fs;
    use std::path::Path;

    fn defaults() -> Vec<String> {
        [".git", "target", "node_modules"]
            .map(String::from)
            .to_vec()
    }

    fn touch(root: &Path, rel: &str) {
        let path = root.join(rel);
        fs::create_dir_all(path.parent().unwrap()).unwrap();
        fs::write(path, "x").unwrap();
    }

    fn names(filter: &ExplorerFilter, dir: &Path) -> Vec<String> {
        list_dir(filter, dir)
            .unwrap()
            .into_iter()
            .map(|e| e.name)
            .collect()
    }

    #[test]
    fn natural_order_is_case_insensitive() {
        let tmp = tempfile::tempdir().unwrap();
        for n in [
            "file10.txt",
            "file2.txt",
            "File1.txt",
            "b.txt",
            "A.txt",
            "file02.txt",
        ] {
            touch(tmp.path(), n);
        }
        let f = ExplorerFilter::new(tmp.path(), &[]);
        assert_eq!(
            names(&f, tmp.path()),
            [
                "A.txt",
                "b.txt",
                "File1.txt",
                "file2.txt",
                "file02.txt",
                "file10.txt"
            ]
        );
    }

    #[test]
    fn case_only_difference_is_stable() {
        let tmp = tempfile::tempdir().unwrap();
        touch(tmp.path(), "a");
        touch(tmp.path(), "A");
        let f = ExplorerFilter::new(tmp.path(), &[]);
        assert_eq!(names(&f, tmp.path()), ["A", "a"]);
    }

    #[test]
    fn folders_first() {
        let tmp = tempfile::tempdir().unwrap();
        touch(tmp.path(), "a.txt");
        touch(tmp.path(), "zdir/inner.txt");
        touch(tmp.path(), "bdir/inner.txt");
        let f = ExplorerFilter::new(tmp.path(), &[]);
        let list = list_dir(&f, tmp.path()).unwrap();
        let got: Vec<_> = list.iter().map(|e| (e.name.as_str(), e.kind)).collect();
        assert_eq!(
            got,
            [
                ("bdir", EntryKind::Dir),
                ("zdir", EntryKind::Dir),
                ("a.txt", EntryKind::File)
            ]
        );
        assert_eq!(list[0].path, tmp.path().join("bdir"));
    }

    #[test]
    fn root_and_nested_gitignore() {
        let tmp = tempfile::tempdir().unwrap();
        let r = tmp.path();
        fs::write(r.join(".gitignore"), "*.log\nbuild/\n").unwrap();
        touch(r, "a.log");
        touch(r, "a.txt");
        touch(r, "build/x");
        touch(r, "sub/b.log");
        touch(r, "sub/c.tmp");
        touch(r, "sub/keep.txt");
        fs::write(r.join("sub/.gitignore"), "*.tmp\n").unwrap();
        let f = ExplorerFilter::new(r, &[]);
        assert_eq!(names(&f, r), ["sub", ".gitignore", "a.txt"]);
        assert_eq!(names(&f, &r.join("sub")), [".gitignore", "keep.txt"]);
    }

    #[test]
    fn negations_and_nested_override() {
        let tmp = tempfile::tempdir().unwrap();
        let r = tmp.path();
        fs::write(r.join(".gitignore"), "*.log\n!important.log\n").unwrap();
        touch(r, "a.log");
        touch(r, "important.log");
        touch(r, "sub/x.log");
        touch(r, "sub/y.log");
        // A nested file re-includes what the root ignores.
        fs::write(r.join("sub/.gitignore"), "!y.log\n").unwrap();
        let f = ExplorerFilter::new(r, &[]);
        assert_eq!(names(&f, r), ["sub", ".gitignore", "important.log"]);
        assert_eq!(names(&f, &r.join("sub")), [".gitignore", "y.log"]);
    }

    #[test]
    fn exclude_patterns_and_git_hidden_by_default() {
        let tmp = tempfile::tempdir().unwrap();
        let r = tmp.path();
        touch(r, ".git/HEAD");
        touch(r, "target/x");
        touch(r, "node_modules/x");
        touch(r, "src/target/x");
        touch(r, "src/main.rs");
        touch(r, "notes.bak");
        let mut patterns = defaults();
        patterns.push("*.bak".into());
        let f = ExplorerFilter::new(r, &patterns);
        assert_eq!(names(&f, r), ["src"]);
        assert_eq!(names(&f, &r.join("src")), ["main.rs"]);
        // Without patterns nothing is hidden.
        let f = ExplorerFilter::new(r, &[]);
        assert_eq!(names(&f, r).len(), 5);
    }

    #[test]
    fn git_info_exclude() {
        let tmp = tempfile::tempdir().unwrap();
        let r = tmp.path();
        touch(r, ".git/HEAD");
        fs::create_dir_all(r.join(".git/info")).unwrap();
        fs::write(r.join(".git/info/exclude"), "secret.txt\n").unwrap();
        touch(r, "secret.txt");
        touch(r, "open.txt");
        let f = ExplorerFilter::new(r, &defaults());
        assert_eq!(names(&f, r), ["open.txt"]);
    }

    #[cfg(unix)]
    #[test]
    fn symlinks() {
        use std::os::unix::fs::symlink;
        let tmp = tempfile::tempdir().unwrap();
        let r = tmp.path();
        touch(r, "real/x");
        touch(r, "file.txt");
        symlink(r.join("real"), r.join("link_dir")).unwrap();
        symlink(r.join("file.txt"), r.join("link_file")).unwrap();
        symlink(r.join("missing"), r.join("link_broken")).unwrap();
        let f = ExplorerFilter::new(r, &[]);
        let list = list_dir(&f, r).unwrap();
        let get = |n: &str| list.iter().find(|e| e.name == n).unwrap().clone();
        assert_eq!(
            (get("link_dir").kind, get("link_dir").is_symlink),
            (EntryKind::Dir, true)
        );
        assert_eq!(get("link_file").kind, EntryKind::File);
        assert!(get("link_file").is_symlink);
        assert_eq!(get("link_broken").kind, EntryKind::File);
        assert!(!get("real").is_symlink);
        // Folders first: real and link_dir before the files.
        assert_eq!(list[0].kind, EntryKind::Dir);
        assert_eq!(list[1].kind, EntryKind::Dir);
    }

    #[test]
    fn errors() {
        let tmp = tempfile::tempdir().unwrap();
        touch(tmp.path(), "f.txt");
        let f = ExplorerFilter::new(tmp.path(), &[]);
        assert!(matches!(
            list_dir(&f, &tmp.path().join("nope")),
            Err(ListError::NotFound(_))
        ));
        assert!(matches!(
            list_dir(&f, &tmp.path().join("f.txt")),
            Err(ListError::NotADirectory(_))
        ));
    }

    #[test]
    fn filter_is_send_sync() {
        fn check<T: Send + Sync>() {}
        check::<ExplorerFilter>();
    }

    /// Timings for docs/performance.md. Needs `make bench-files`; run with
    /// `cargo test -p ted-workspace --release -- --ignored --nocapture list_bench`.
    #[test]
    #[ignore]
    fn list_bench() {
        let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../target/bench-files/tree");
        let filter = ExplorerFilter::new(&root, &defaults());
        let (mut widest, mut count) = (root.clone(), 0);
        for entry in fs::read_dir(&root).unwrap().flatten() {
            let n = fs::read_dir(entry.path()).map(|d| d.count()).unwrap_or(0);
            if n > count {
                (widest, count) = (entry.path(), n);
            }
        }
        for (label, dir, f) in [
            ("widest folder", widest, &filter),
            (
                "ignored target/",
                root.join("target"),
                &ExplorerFilter::new(&root, &[]),
            ),
        ] {
            let t = Instant::now();
            let list = list_dir(f, &dir).unwrap();
            println!(
                "{label} {}: {} entries (cold filter) in {:?}",
                dir.display(),
                list.len(),
                t.elapsed()
            );
            let t = Instant::now();
            let list = list_dir(f, &dir).unwrap();
            println!(
                "{label}: {} entries (warm) in {:?}",
                list.len(),
                t.elapsed()
            );
        }
        // The root listing hides the ignored target/ and dirs through the root .gitignore.
        let t = Instant::now();
        let list = list_dir(&filter, &root).unwrap();
        println!("root: {} entries in {:?}", list.len(), t.elapsed());
    }
}
