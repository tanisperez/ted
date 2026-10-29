//! The app wide actions and their key bindings. The editor's own actions live in
//! `views/editor.rs`.
//!
//! `gpui`'s `actions!` macro turns each name into a unit struct that implements `gpui::Action`,
//! bound to keys with `KeyBinding::new` and handled with `on_action`.

use gpui_kit::{App, KeyBinding, actions};

actions!(
    ted,
    [
        /// Opens a file with the native dialog (`Ctrl+O`).
        OpenFile,
        /// Opens a folder with the native dialog (`Ctrl+Shift+O`).
        OpenFolder,
        /// Shows or hides the explorer in folder mode (`Ctrl+B`).
        ToggleExplorer,
    ]
);

/// The shortcut labels shown next to the actions, as typed on this platform.
#[cfg(target_os = "macos")]
pub mod hints {
    pub const OPEN_FILE: &str = "\u{2318}O";
    pub const OPEN_FOLDER: &str = "\u{21e7}\u{2318}O";
}

/// The shortcut labels shown next to the actions, as typed on this platform.
#[cfg(not(target_os = "macos"))]
pub mod hints {
    pub const OPEN_FILE: &str = "Ctrl+O";
    pub const OPEN_FOLDER: &str = "Ctrl+Shift+O";
}

/// Registers the app wide key bindings. `None` as the context: anywhere in the window.
pub fn bind_keys(cx: &mut App) {
    #[cfg(target_os = "macos")]
    cx.bind_keys([
        KeyBinding::new("cmd-o", OpenFile, None),
        KeyBinding::new("cmd-shift-o", OpenFolder, None),
        KeyBinding::new("cmd-b", ToggleExplorer, None),
    ]);
    #[cfg(not(target_os = "macos"))]
    cx.bind_keys([
        KeyBinding::new("ctrl-o", OpenFile, None),
        KeyBinding::new("ctrl-shift-o", OpenFolder, None),
        KeyBinding::new("ctrl-b", ToggleExplorer, None),
    ]);
}
