//! The recently opened folders and files, most recent first, shown in the empty state and the
//! title bar's menu. Stored in `<config dir>/ted/recents.toml` next to the settings, as two lists
//! of absolute paths. Reading or writing it never interrupts the user: errors are only logged.

use std::fs;
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

use super::config::config_dir;
use super::launch::Target;

/// The file, relative to the config directory.
const RECENTS_FILE: &str = "recents.toml";

/// How many folders and how many files are remembered.
pub const MAX_RECENTS: usize = 10;

/// The recent folders and files, most recent first.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct Recents {
    #[serde(default)]
    pub folders: Vec<PathBuf>,
    #[serde(default)]
    pub files: Vec<PathBuf>,
}

impl Recents {
    /// Moves `target` to the front of its list, capped to [`MAX_RECENTS`].
    pub fn add(&mut self, target: &Target) {
        let (list, path) = match target {
            Target::Folder(path) => (&mut self.folders, path),
            Target::File(path) => (&mut self.files, path),
        };
        list.retain(|existing| existing != path);
        list.insert(0, path.clone());
        list.truncate(MAX_RECENTS);
    }

    /// Forgets `path` (it no longer exists).
    pub fn remove(&mut self, path: &Path) {
        self.folders.retain(|existing| existing != path);
        self.files.retain(|existing| existing != path);
    }
}

/// Loads the recents, empty when there are none or the file is unreadable.
pub fn load() -> Recents {
    config_dir().map_or_else(Recents::default, |base| read(&base))
}

/// Saves the recents. Small and rare (once per opened item), so it runs where it is called.
pub fn save(recents: &Recents) {
    if let Some(base) = config_dir() {
        write(&base, recents);
    }
}

fn read(base: &Path) -> Recents {
    let Ok(content) = fs::read_to_string(base.join(RECENTS_FILE)) else {
        return Recents::default();
    };
    match toml::from_str::<Recents>(&content) {
        Ok(mut recents) => {
            recents.folders.truncate(MAX_RECENTS);
            recents.files.truncate(MAX_RECENTS);
            recents
        }
        Err(error) => {
            log::warn!("ignoring {RECENTS_FILE}: {error}");
            Recents::default()
        }
    }
}

fn write(base: &Path, recents: &Recents) {
    let result = toml::to_string(recents)
        .map_err(|error| error.to_string())
        .and_then(|text| {
            fs::create_dir_all(base).map_err(|error| error.to_string())?;
            fs::write(base.join(RECENTS_FILE), text).map_err(|error| error.to_string())
        });
    if let Err(error) = result {
        log::warn!("cannot save {RECENTS_FILE}: {error}");
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use super::*;
    use pretty_assertions::assert_eq;

    #[test]
    fn adding_moves_to_the_front_without_duplicates_and_caps() {
        let mut recents = Recents::default();
        for index in 0..12 {
            recents.add(&Target::File(PathBuf::from(format!("/f{index}"))));
        }
        recents.add(&Target::File(PathBuf::from("/f5")));
        recents.add(&Target::Folder(PathBuf::from("/d")));
        assert_eq!(recents.files.len(), MAX_RECENTS);
        assert_eq!(recents.files[0], PathBuf::from("/f5"));
        assert_eq!(
            recents
                .files
                .iter()
                .filter(|p| **p == Path::new("/f5"))
                .count(),
            1
        );
        assert_eq!(recents.folders, vec![PathBuf::from("/d")]);
        recents.remove(Path::new("/d"));
        assert!(recents.folders.is_empty());
    }

    #[test]
    fn round_trip_through_the_file() {
        let dir = tempfile::tempdir().unwrap();
        assert_eq!(read(dir.path()), Recents::default());
        let mut recents = Recents::default();
        recents.add(&Target::Folder(PathBuf::from("/home/me/ted")));
        recents.add(&Target::File(PathBuf::from("/tmp/a b.txt")));
        write(dir.path(), &recents);
        assert_eq!(read(dir.path()), recents);
    }

    #[test]
    fn a_broken_file_is_ignored() {
        let dir = tempfile::tempdir().unwrap();
        fs::write(dir.path().join(RECENTS_FILE), "folders = 3").unwrap();
        assert_eq!(read(dir.path()), Recents::default());
    }
}
