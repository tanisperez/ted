//! ted's folder inside the OS config directory, where the settings live, and the last version
//! that ran, for the What's new tab (`state::whats_new`).

use std::fs;
use std::path::{Path, PathBuf};

use super::whats_new::LastSeen;

/// The file with the last stable version that ran, relative to the config directory.
const LAST_VERSION_FILE: &str = "last-version.txt";
/// Files of an earlier run: with one of them and no [`LAST_VERSION_FILE`], ted was updated from
/// a version that did not record itself. Without any, this is the first run.
const EARLIER_RUN_FILES: [&str; 3] = ["settings.toml", "recents.toml", "window.toml"];

/// `<config dir>/ted`, `None` when the OS does not report the config directory.
pub fn config_dir() -> Option<PathBuf> {
    telaio_app::paths::config_dir(&ted_cli::IDENTITY)
}

/// The last version that ran.
pub fn last_seen_version() -> LastSeen {
    config_dir().map_or(LastSeen::FirstRun, |base| last_seen_version_at(&base))
}

/// Records `version` as the last one that ran, when it differs from the recorded one. Errors are
/// only logged.
pub fn record_version(version: &str) {
    if let Some(base) = config_dir() {
        record_version_at(&base, version);
    }
}

fn last_seen_version_at(base: &Path) -> LastSeen {
    if let Ok(content) = fs::read_to_string(base.join(LAST_VERSION_FILE)) {
        return LastSeen::Version(content.trim().to_string());
    }
    if EARLIER_RUN_FILES
        .iter()
        .any(|file| base.join(file).exists())
    {
        LastSeen::Unrecorded
    } else {
        LastSeen::FirstRun
    }
}

fn record_version_at(base: &Path, version: &str) {
    if last_seen_version_at(base) == LastSeen::Version(version.to_string()) {
        return;
    }
    if let Err(error) = fs::create_dir_all(base) {
        log::warn!("could not create {}: {error}", base.display());
        return;
    }
    let path = base.join(LAST_VERSION_FILE);
    if let Err(error) = fs::write(&path, format!("{version}\n")) {
        log::warn!("could not save {}: {error}", path.display());
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use super::*;
    use pretty_assertions::assert_eq;

    #[test]
    fn the_last_version_goes_from_first_run_to_recorded() {
        let dir = tempfile::tempdir().unwrap();
        assert_eq!(last_seen_version_at(dir.path()), LastSeen::FirstRun);
        fs::write(dir.path().join("recents.toml"), "").unwrap();
        assert_eq!(last_seen_version_at(dir.path()), LastSeen::Unrecorded);
        record_version_at(dir.path(), "0.6.0");
        assert_eq!(
            last_seen_version_at(dir.path()),
            LastSeen::Version("0.6.0".to_string())
        );
    }
}
