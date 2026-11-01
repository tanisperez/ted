//! What a path opens: a file alone (file mode), a folder with the explorer (folder mode), or an
//! error to show instead of a panic. Used for the command line argument, the Open dialogs and the
//! recents.

use std::path::{Path, PathBuf};

/// How the window shows what it opened.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Target {
    /// One file, alone: no explorer, no tabs.
    File(PathBuf),
    /// A folder, with the explorer and the tabs.
    Folder(PathBuf),
}

impl Target {
    pub fn path(&self) -> &Path {
        match self {
            Target::File(path) | Target::Folder(path) => path,
        }
    }
}

/// Why a path cannot be opened.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum LaunchError {
    /// Nothing exists at the path.
    NotFound(PathBuf),
    /// It exists but cannot be read (permissions, a broken link).
    Unreadable(PathBuf, String),
}

/// The target `path` opens, made absolute so it can be remembered in the recents. Reads the
/// path's metadata, nothing more.
pub fn resolve(path: &Path) -> Result<Target, LaunchError> {
    let absolute = std::path::absolute(path).unwrap_or_else(|_| path.to_path_buf());
    match std::fs::metadata(&absolute) {
        Ok(meta) if meta.is_dir() => Ok(Target::Folder(absolute)),
        Ok(_) => Ok(Target::File(absolute)),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
            Err(LaunchError::NotFound(absolute))
        }
        Err(error) => Err(LaunchError::Unreadable(absolute, error.to_string())),
    }
}

/// The name shown for a path in the title bar and the window title: its last component.
pub fn display_name(path: &Path) -> String {
    path.file_name()
        .map(|name| name.to_string_lossy().into_owned())
        .unwrap_or_else(|| path.display().to_string())
}

/// `path` relative to `root`, in the platform's separators, when it is inside it (and is not
/// `root` itself), for Copy relative path.
pub fn relative_path(root: &Path, path: &Path) -> Option<String> {
    let relative = path.strip_prefix(root).ok()?;
    (!relative.as_os_str().is_empty()).then(|| relative.display().to_string())
}

/// `path` with the home folder written as `~`, for the recents and the menus.
pub fn shorten_path(path: &Path, home: Option<&Path>) -> String {
    if let Some(home) = home
        && let Ok(rest) = path.strip_prefix(home)
    {
        if rest.as_os_str().is_empty() {
            return "~".to_string();
        }
        return format!("~{}{}", std::path::MAIN_SEPARATOR, rest.display());
    }
    path.display().to_string()
}

/// Converts the `file://` URLs macOS hands over (Finder's Open with, a drop on the Dock icon)
/// into paths, dropping anything else.
pub fn paths_from_urls(urls: &[String]) -> Vec<PathBuf> {
    urls.iter()
        .filter_map(|url| file_url_to_path(url))
        .collect()
}

/// Converts one `file://` URL (percent-encoded, optional `localhost` host) into a path.
pub fn file_url_to_path(url: &str) -> Option<PathBuf> {
    let rest = url.strip_prefix("file://")?;
    let rest = rest.strip_prefix("localhost").unwrap_or(rest);
    if !rest.starts_with('/') {
        return None;
    }
    let decoded = percent_decode(rest)?;
    // `file:///C:/dir/file` carries a leading slash that is not part of a Windows path.
    let bytes = decoded.as_bytes();
    let path = if bytes.len() > 2 && bytes[2] == b':' && bytes[1].is_ascii_alphabetic() {
        &decoded[1..]
    } else {
        decoded.as_str()
    };
    Some(PathBuf::from(path))
}

/// Decodes `%XX` escapes as UTF-8. `None` when an escape is malformed or the result is not UTF-8.
fn percent_decode(text: &str) -> Option<String> {
    let bytes = text.as_bytes();
    let mut out = Vec::with_capacity(bytes.len());
    let mut index = 0;
    while index < bytes.len() {
        if bytes[index] == b'%' {
            let hex = text.get(index + 1..index + 3)?;
            out.push(u8::from_str_radix(hex, 16).ok()?);
            index += 3;
        } else {
            out.push(bytes[index]);
            index += 1;
        }
    }
    String::from_utf8(out).ok()
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use super::*;
    use pretty_assertions::assert_eq;

    #[test]
    fn relative_paths_need_the_root() {
        let root = Path::new("/home/me/project");
        assert_eq!(
            relative_path(root, Path::new("/home/me/project/src/main.rs")),
            Some(Path::new("src").join("main.rs").display().to_string())
        );
        assert_eq!(relative_path(root, Path::new("/home/me/other.txt")), None);
        assert_eq!(relative_path(root, root), None);
    }

    #[test]
    fn files_folders_and_missing_paths() {
        let dir = tempfile::tempdir().unwrap();
        let file = dir.path().join("notes.txt");
        std::fs::write(&file, "hi").unwrap();
        assert_eq!(resolve(&file), Ok(Target::File(file.clone())));
        assert_eq!(
            resolve(dir.path()),
            Ok(Target::Folder(dir.path().to_path_buf()))
        );
        let missing = dir.path().join("nope.md");
        assert_eq!(resolve(&missing), Err(LaunchError::NotFound(missing)));
    }

    #[test]
    fn relative_paths_become_absolute() {
        let target = resolve(Path::new(".")).unwrap();
        assert!(target.path().is_absolute());
    }

    #[test]
    fn names_and_short_paths() {
        assert_eq!(display_name(Path::new("/home/me/ted")), "ted");
        assert_eq!(display_name(Path::new("/")), "/");
        let home = Path::new("/home/me");
        let sep = std::path::MAIN_SEPARATOR;
        assert_eq!(
            shorten_path(Path::new("/home/me/dev/ted"), Some(home)),
            format!("~{sep}dev/ted")
        );
        assert_eq!(shorten_path(home, Some(home)), "~");
        assert_eq!(
            shorten_path(Path::new("/etc/hosts"), Some(home)),
            "/etc/hosts"
        );
    }

    #[test]
    fn file_urls_are_percent_decoded() {
        assert_eq!(
            file_url_to_path("file:///Users/ana/My%20Notes/to%20do.md"),
            Some(PathBuf::from("/Users/ana/My Notes/to do.md"))
        );
        assert_eq!(
            file_url_to_path("file://localhost/tmp/a.txt"),
            Some(PathBuf::from("/tmp/a.txt"))
        );
        assert_eq!(
            file_url_to_path("file:///tmp/%C3%B1.txt"),
            Some(PathBuf::from("/tmp/\u{f1}.txt"))
        );
    }

    #[test]
    fn non_file_or_malformed_urls_are_dropped() {
        assert_eq!(file_url_to_path("https://example.com/a.txt"), None);
        assert_eq!(file_url_to_path("file:///tmp/%zz"), None);
        assert_eq!(file_url_to_path("file://remote/tmp/a"), None);
        let urls = vec!["ted://x".to_string(), "file:///tmp/a.txt".to_string()];
        assert_eq!(paths_from_urls(&urls), vec![PathBuf::from("/tmp/a.txt")]);
    }
}
