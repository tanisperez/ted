//! Where a file or a folder lives: copy its path, or show it in the system file manager. Used by
//! the title bar's switcher menu, the explorer's right-click menu and the `Ctrl+Alt+C` and
//! `Ctrl+Alt+R` shortcuts.

use std::path::{Path, PathBuf};

use gpui_kit::assets::IconName;
use gpui_kit::component::WindowExt as _;
use gpui_kit::component::menu::PopupMenu;
use gpui_kit::component::notification::Notification;
use gpui_kit::prelude::*;
use gpui_kit::*;
use rust_i18n::t;
use telaio_ui::components::menu_shortcut_item;

use crate::actions::{CopyPath, RevealInFileManager, hints};
use crate::state::launch::{Target, relative_path};

use super::root::AppView;

/// Shows `path` in the system file manager: a folder (`folder`) opens, a file opens its folder
/// with the file selected where the platform allows it.
pub(crate) fn reveal(path: &Path, folder: bool, cx: &App) {
    log::debug!("revealing {}", path.display());
    if folder {
        cx.open_with_system(path);
    } else {
        cx.reveal_path(path);
    }
}

/// Copies `text` (a path) to the clipboard and says so.
pub(crate) fn copy_path(text: String, window: &mut Window, cx: &mut App) {
    cx.write_to_clipboard(ClipboardItem::new_string(text));
    window.push_notification(Notification::info(t!("open.path_copied").into_owned()), cx);
}

/// Adds Copy path, Copy relative path (when `root` is given and holds `path`) and Show in file
/// manager for `path` (a folder when `folder`) to `menu`.
pub(crate) fn path_items(
    menu: PopupMenu,
    path: &Path,
    folder: bool,
    root: Option<&Path>,
) -> PopupMenu {
    let full = path.display().to_string();
    let mut menu = menu.item(menu_shortcut_item(
        IconName::Copy,
        t!("open.copy_path"),
        hints::COPY_PATH,
        move |window, cx| copy_path(full.clone(), window, cx),
    ));
    if let Some(relative) = root.and_then(|root| relative_path(root, path)) {
        menu = menu.item(menu_shortcut_item(
            IconName::ClipboardCopy,
            t!("open.copy_relative_path"),
            "",
            move |window, cx| copy_path(relative.clone(), window, cx),
        ));
    }
    let target = path.to_path_buf();
    menu.item(menu_shortcut_item(
        IconName::FolderSearch,
        t!("open.reveal"),
        hints::REVEAL,
        move |_, cx| reveal(&target, folder, cx),
    ))
}

impl AppView {
    /// What the path shortcuts act on: the file shown, or the open folder when no file is, and
    /// whether it is a folder.
    pub(crate) fn current_location(&self) -> Option<(PathBuf, bool)> {
        let file = self
            .documents
            .active()
            .and_then(|document| document.path.clone());
        match (file, &self.target) {
            (Some(file), _) => Some((file, false)),
            (None, Some(Target::Folder(root))) => Some((root.clone(), true)),
            _ => None,
        }
    }

    /// `Ctrl+Alt+C`: copies the path of the file shown, or of the open folder.
    pub(crate) fn copy_current_path(
        &mut self,
        _: &CopyPath,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if let Some((path, _)) = self.current_location() {
            copy_path(path.display().to_string(), window, cx);
        }
    }

    /// `Ctrl+Alt+R`: shows the file shown, or the open folder, in the file manager.
    pub(crate) fn reveal_current(
        &mut self,
        _: &RevealInFileManager,
        _: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if let Some((path, folder)) = self.current_location() {
            reveal(&path, folder, cx);
        }
    }
}
