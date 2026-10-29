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

/// `path` with the home folder written as `~`, for the recents and the status bar.
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

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use super::*;
    use pretty_assertions::assert_eq;

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
}
