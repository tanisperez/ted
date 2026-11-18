//! Moving a file or a folder into another folder, for the explorer's drag and drop (#98).

use std::fs;
use std::io;
use std::path::{Path, PathBuf};

/// Why a move did not happen.
#[derive(Debug, thiserror::Error)]
pub enum MoveError {
    /// The folder already holds an entry with that name; move again with `replace` to overwrite.
    #[error("{0} already exists")]
    Exists(PathBuf),
    /// Moving there makes no sense: into itself, into one of its own folders, or where it is.
    #[error("cannot move {0} there")]
    Pointless(PathBuf),
    #[error("{0}")]
    Io(#[from] io::Error),
}

/// Where `from` would land in `dir`: `dir` joined with its name. `None` for a path with no name.
pub fn move_target(from: &Path, dir: &Path) -> Option<PathBuf> {
    Some(dir.join(from.file_name()?))
}

/// Whether moving `from` into `dir` would do nothing or break the tree: `dir` is `from` itself,
/// one of its folders, or the folder it is already in.
pub fn pointless_move(from: &Path, dir: &Path) -> bool {
    dir.starts_with(from) || from.parent() == Some(dir)
}

/// Moves `from` (a file or a folder) into the folder `dir`, keeping its name, and returns where it
/// is now. An entry of that name already in `dir` is an error unless `replace`, which removes it
/// first. A rename, so it is atomic on one file system.
pub fn move_into(from: &Path, dir: &Path, replace: bool) -> Result<PathBuf, MoveError> {
    if pointless_move(from, dir) {
        return Err(MoveError::Pointless(from.to_path_buf()));
    }
    let Some(target) = move_target(from, dir) else {
        return Err(MoveError::Pointless(from.to_path_buf()));
    };
    if fs::symlink_metadata(&target).is_ok() {
        if !replace {
            return Err(MoveError::Exists(target));
        }
        if target.is_dir() {
            fs::remove_dir_all(&target)?;
        } else {
            fs::remove_file(&target)?;
        }
    }
    fs::rename(from, &target)?;
    Ok(target)
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use super::*;

    #[test]
    fn moves_a_file_and_a_folder() {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path();
        fs::create_dir_all(root.join("notes")).unwrap();
        fs::create_dir_all(root.join("src/deep")).unwrap();
        fs::write(root.join("todo.md"), "a").unwrap();
        fs::write(root.join("src/deep/x.rs"), "x").unwrap();

        let moved = move_into(&root.join("todo.md"), &root.join("notes"), false).unwrap();
        assert_eq!(moved, root.join("notes/todo.md"));
        assert_eq!(fs::read_to_string(&moved).unwrap(), "a");
        assert!(!root.join("todo.md").exists());

        let moved = move_into(&root.join("src"), &root.join("notes"), false).unwrap();
        assert!(moved.join("deep/x.rs").exists());
    }

    #[test]
    fn a_name_clash_asks_and_replace_overwrites() {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path();
        fs::create_dir_all(root.join("notes")).unwrap();
        fs::write(root.join("todo.md"), "new").unwrap();
        fs::write(root.join("notes/todo.md"), "old").unwrap();

        let error = move_into(&root.join("todo.md"), &root.join("notes"), false).unwrap_err();
        assert!(matches!(error, MoveError::Exists(path) if path == root.join("notes/todo.md")));
        let moved = move_into(&root.join("todo.md"), &root.join("notes"), true).unwrap();
        assert_eq!(fs::read_to_string(moved).unwrap(), "new");
    }

    #[test]
    fn never_into_itself_or_where_it_is() {
        let root = Path::new("/w");
        assert!(pointless_move(&root.join("src"), &root.join("src")));
        assert!(pointless_move(&root.join("src"), &root.join("src/deep")));
        assert!(pointless_move(&root.join("src/a.rs"), &root.join("src")));
        assert!(!pointless_move(&root.join("src/a.rs"), root));
        assert!(!pointless_move(&root.join("srcx"), &root.join("src")));
    }
}
