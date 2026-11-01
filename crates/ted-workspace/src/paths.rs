//! Paths as the user writes them. On Windows, `fs::canonicalize` returns verbatim paths
//! (`\\?\C:\work\a.txt`, `\\?\UNC\server\share\a.txt`): they work for IO but must not show in
//! the title, the tabs or the recents. Network shares (`\\server\share`) and the WSL 2
//! distributions (`\\wsl$\Ubuntu`, `\\wsl.localhost\Ubuntu`) are UNC paths and need nothing else.

use std::path::{Path, PathBuf};

/// `path` without the verbatim prefix, when it has one that a plain path can say the same way.
/// Unchanged on other systems.
pub fn plain_path(path: &Path) -> PathBuf {
    if cfg!(windows)
        && let Some(plain) = path.to_str().and_then(strip_verbatim)
    {
        return PathBuf::from(plain);
    }
    path.to_path_buf()
}

/// `\\?\UNC\server\share\x` as `\\server\share\x` and `\\?\C:\x` as `C:\x`. `None` for anything
/// else, including verbatim paths with no plain form (`\\?\Volume{...}`).
fn strip_verbatim(text: &str) -> Option<String> {
    if let Some(rest) = text.strip_prefix(r"\\?\UNC\") {
        return Some(format!(r"\\{rest}"));
    }
    let rest = text.strip_prefix(r"\\?\")?;
    let mut chars = rest.chars();
    let drive = matches!(
        (chars.next(), chars.next(), chars.next()),
        (Some(letter), Some(':'), Some('\\') | None) if letter.is_ascii_alphabetic()
    );
    drive.then(|| rest.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn verbatim_prefixes_go() {
        assert_eq!(
            strip_verbatim(r"\\?\UNC\server\share\notes.md").as_deref(),
            Some(r"\\server\share\notes.md")
        );
        assert_eq!(
            strip_verbatim(r"\\?\UNC\wsl.localhost\Ubuntu\home\me\a.log").as_deref(),
            Some(r"\\wsl.localhost\Ubuntu\home\me\a.log")
        );
        assert_eq!(
            strip_verbatim(r"\\?\C:\work\a.txt").as_deref(),
            Some(r"C:\work\a.txt")
        );
        assert_eq!(strip_verbatim(r"\\?\D:").as_deref(), Some("D:"));
    }

    #[test]
    fn other_paths_stay() {
        for text in [
            r"\\server\share\a.txt",
            r"\\wsl$\Ubuntu\home",
            r"C:\a.txt",
            "/home/me/a.txt",
            r"\\?\Volume{1234}\a.txt",
        ] {
            assert_eq!(strip_verbatim(text), None, "{text}");
        }
        assert_eq!(plain_path(Path::new("/tmp/a")), PathBuf::from("/tmp/a"));
    }
}
