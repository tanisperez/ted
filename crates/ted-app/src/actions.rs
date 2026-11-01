//! The app wide actions and their key bindings. The editor's own actions live in
//! `views/editor.rs`.
//!
//! `gpui`'s `actions!` macro turns each name into a unit struct that implements `gpui::Action`,
//! bound to keys with `KeyBinding::new` and handled with `on_action`.

use gpui_kit::{App, KeyBinding, actions};

actions!(
    ted,
    [
        /// Creates a new untitled document (`Ctrl+N`).
        NewFile,
        /// Opens a file with the native dialog (`Ctrl+O`).
        OpenFile,
        /// Opens a folder with the native dialog (`Ctrl+Shift+O`).
        OpenFolder,
        /// Shows or hides the explorer in folder mode, like its status bar toggle (`Alt 1`, `Ctrl 1`
        /// on macOS, as Postino's first panel).
        ToggleExplorer,
        /// Shows or hides the Git panel in folder mode, like its status bar toggle (`Alt 2`, `Ctrl 2`
        /// on macOS, as Postino's second panel).
        ToggleGit,
        /// Commits from the Git panel's message (`Ctrl+Enter`), or commits and pushes, as the
        /// setting says.
        CommitFromMessage,
        /// Saves the open document (`Ctrl+S`).
        Save,
        /// Opens the Settings modal (`Ctrl+,`).
        OpenSettings,
        /// Opens the keyboard shortcuts cheat sheet (`F1`, `Cmd+Shift+/` on macOS).
        OpenShortcuts,
        /// Closes the active tab (`Ctrl+W`).
        CloseTab,
        /// Copies the path of the file shown, or of the open folder (`Ctrl+Alt+C`).
        CopyPath,
        /// Shows the file shown, or the open folder, in the system file manager (`Ctrl+Alt+R`).
        RevealInFileManager,
        /// Makes the editor font one point bigger (`Ctrl+=`).
        ZoomIn,
        /// Makes the editor font one point smaller (`Ctrl+-`).
        ZoomOut,
        /// Puts the editor font back to its default size (`Ctrl+0`).
        ZoomReset,
        /// Shows the next tab, wrapping around (`Ctrl+Tab`).
        NextTab,
        /// Shows the previous tab, wrapping around (`Ctrl+Shift+Tab`).
        PreviousTab,
    ]
);

/// The shortcut labels shown next to the actions, as typed on this platform.
#[cfg(target_os = "macos")]
pub mod hints {
    pub const NEW_FILE: &str = "\u{2318}N";
    pub const OPEN_FILE: &str = "\u{2318}O";
    pub const OPEN_FOLDER: &str = "\u{21e7}\u{2318}O";
    pub const COPY_PATH: &str = "\u{2325}\u{2318}C";
    pub const REVEAL: &str = "\u{2325}\u{2318}R";
    pub const CLOSE_TAB: &str = "\u{2318}W";
}

/// The shortcut labels shown next to the actions, as typed on this platform.
#[cfg(not(target_os = "macos"))]
pub mod hints {
    pub const NEW_FILE: &str = "Ctrl+N";
    pub const OPEN_FILE: &str = "Ctrl+O";
    pub const OPEN_FOLDER: &str = "Ctrl+Shift+O";
    pub const COPY_PATH: &str = "Ctrl+Alt+C";
    pub const REVEAL: &str = "Ctrl+Alt+R";
    pub const CLOSE_TAB: &str = "Ctrl+W";
}

/// Registers the app wide key bindings. `None` as the context: anywhere in the window.
pub fn bind_keys(cx: &mut App) {
    #[cfg(target_os = "macos")]
    cx.bind_keys([
        KeyBinding::new("cmd-n", NewFile, None),
        KeyBinding::new("cmd-o", OpenFile, None),
        KeyBinding::new("cmd-shift-o", OpenFolder, None),
        KeyBinding::new("ctrl-1", ToggleExplorer, None),
        KeyBinding::new("ctrl-2", ToggleGit, None),
        KeyBinding::new("cmd-s", Save, None),
        KeyBinding::new("cmd-w", CloseTab, None),
        KeyBinding::new("cmd-,", OpenSettings, None),
        KeyBinding::new("cmd-alt-c", CopyPath, None),
        KeyBinding::new("cmd-alt-r", RevealInFileManager, None),
        KeyBinding::new("cmd-=", ZoomIn, None),
        KeyBinding::new("cmd-+", ZoomIn, None),
        KeyBinding::new("cmd--", ZoomOut, None),
        KeyBinding::new("cmd-0", ZoomReset, None),
    ]);
    #[cfg(not(target_os = "macos"))]
    cx.bind_keys([
        KeyBinding::new("ctrl-n", NewFile, None),
        KeyBinding::new("ctrl-o", OpenFile, None),
        KeyBinding::new("ctrl-shift-o", OpenFolder, None),
        KeyBinding::new("alt-1", ToggleExplorer, None),
        KeyBinding::new("alt-2", ToggleGit, None),
        KeyBinding::new("ctrl-s", Save, None),
        KeyBinding::new("ctrl-w", CloseTab, None),
        KeyBinding::new("ctrl-,", OpenSettings, None),
        KeyBinding::new("ctrl-alt-c", CopyPath, None),
        KeyBinding::new("ctrl-alt-r", RevealInFileManager, None),
        KeyBinding::new("ctrl-=", ZoomIn, None),
        KeyBinding::new("ctrl-+", ZoomIn, None),
        KeyBinding::new("ctrl--", ZoomOut, None),
        KeyBinding::new("ctrl-0", ZoomReset, None),
    ]);
    // Ctrl+Tab switches tabs on every platform, macOS included, as in Postino. The editor binds
    // only plain `tab` and `shift-tab`, so this still fires while it has the focus.
    // The commit message's Ctrl+Enter / Cmd+Enter, bound after the input's own `secondary-enter`
    // (a new line) so it runs first, and only inside the Git panel's message.
    cx.bind_keys([KeyBinding::new(
        "secondary-enter",
        CommitFromMessage,
        Some("CommitMessage > Input"),
    )]);
    cx.bind_keys([
        KeyBinding::new(
            telaio_ui::components::OPEN_SHORTCUTS_KEYSTROKE,
            OpenShortcuts,
            None,
        ),
        KeyBinding::new("ctrl-tab", NextTab, None),
        KeyBinding::new("ctrl-shift-tab", PreviousTab, None),
    ]);
}
