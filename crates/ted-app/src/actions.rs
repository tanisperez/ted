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
        /// Shows or hides the explorer in folder mode, like its status bar toggle (`Alt 1`, `Ctrl 1`
        /// on macOS, as Postino's first panel).
        ToggleExplorer,
        /// Saves the open document (`Ctrl+S`).
        Save,
        /// Opens the Settings modal (`Ctrl+,`).
        OpenSettings,
        /// Closes the active tab in folder mode (`Ctrl+W`).
        CloseTab,
        /// Shows the next tab, wrapping around (`Ctrl+Tab`).
        NextTab,
        /// Shows the previous tab, wrapping around (`Ctrl+Shift+Tab`).
        PreviousTab,
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
        KeyBinding::new("ctrl-1", ToggleExplorer, None),
        KeyBinding::new("cmd-s", Save, None),
        KeyBinding::new("cmd-w", CloseTab, None),
        KeyBinding::new("cmd-,", OpenSettings, None),
    ]);
    #[cfg(not(target_os = "macos"))]
    cx.bind_keys([
        KeyBinding::new("ctrl-o", OpenFile, None),
        KeyBinding::new("ctrl-shift-o", OpenFolder, None),
        KeyBinding::new("alt-1", ToggleExplorer, None),
        KeyBinding::new("ctrl-s", Save, None),
        KeyBinding::new("ctrl-w", CloseTab, None),
        KeyBinding::new("ctrl-,", OpenSettings, None),
    ]);
    // Ctrl+Tab switches tabs on every platform, macOS included, as in Postino. The editor binds
    // only plain `tab` and `shift-tab`, so this still fires while it has the focus.
    cx.bind_keys([
        KeyBinding::new("ctrl-tab", NextTab, None),
        KeyBinding::new("ctrl-shift-tab", PreviousTab, None),
    ]);
}
