//! Finding and creating the `.editorconfig` file that applies to a document (#119).

use std::io;
use std::path::{Path, PathBuf};

/// The file's name.
pub const EDITORCONFIG: &str = ".editorconfig";

/// The nearest `.editorconfig` from `dir` up to `stop` (included), or to the filesystem root when
/// `stop` is `None` or not above `dir`.
pub fn find_editorconfig(dir: &Path, stop: Option<&Path>) -> Option<PathBuf> {
    let stop = stop.filter(|stop| dir.starts_with(stop));
    let mut current = Some(dir);
    while let Some(folder) = current {
        let candidate = folder.join(EDITORCONFIG);
        if candidate.is_file() {
            return Some(candidate);
        }
        if stop == Some(folder) {
            break;
        }
        current = folder.parent();
    }
    None
}

/// Creates `dir/.editorconfig` with `content`, never replacing one that exists. Returns its path.
pub fn create_editorconfig(dir: &Path, content: &str) -> io::Result<PathBuf> {
    use std::io::Write;
    let path = dir.join(EDITORCONFIG);
    let mut file = std::fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(&path)?;
    file.write_all(content.as_bytes())?;
    Ok(path)
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use super::*;
    use std::fs;

    #[test]
    fn finds_the_nearest_up_to_the_stop() {
        let tmp = tempfile::tempdir().unwrap();
        let root = tmp.path();
        let deep = root.join("a").join("b");
        fs::create_dir_all(&deep).unwrap();
        assert_eq!(find_editorconfig(&deep, Some(root)), None);
        fs::write(root.join(EDITORCONFIG), "root = true\n").unwrap();
        assert_eq!(
            find_editorconfig(&deep, Some(root)),
            Some(root.join(EDITORCONFIG))
        );
        fs::write(root.join("a").join(EDITORCONFIG), "").unwrap();
        assert_eq!(
            find_editorconfig(&deep, Some(root)),
            Some(root.join("a").join(EDITORCONFIG))
        );
        // Not past the stop.
        assert_eq!(find_editorconfig(&deep, Some(&deep)), None);
    }

    #[test]
    fn creates_once() {
        let tmp = tempfile::tempdir().unwrap();
        let path = create_editorconfig(tmp.path(), "root = true\n").unwrap();
        assert_eq!(fs::read_to_string(&path).unwrap(), "root = true\n");
        assert!(create_editorconfig(tmp.path(), "x").is_err());
    }
}
