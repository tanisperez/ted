//! ted's folder inside the OS config directory, where the settings live.

use std::path::PathBuf;

/// `<config dir>/ted`, `None` when the OS does not report the config directory.
pub fn config_dir() -> Option<PathBuf> {
    telaio_app::paths::config_dir(&ted_cli::IDENTITY)
}
