//! ted's document icon on the files it opens by default, on Linux and the other freedesktop
//! systems (Settings, "Files", `document_icons`). The work is `ted_workspace::sync_mime_icons`,
//! always on the background executor; this file decides when it runs:
//!
//! 1. at startup, so an icon follows a default chosen while ted was closed, and leftovers are
//!    removed if the setting was turned off by hand;
//! 2. when the setting changes, from any control of Settings or "Reset values";
//! 3. while the setting is on, when a file of the config folder changes, so the icon follows
//!    "Remember" in Dolphin's Open With without a restart.
//!
//! Errors are logged, never shown, and never block the UI.

use gpui_kit::*;
use ted_workspace::{FolderWatcher, XdgDirs, sync_mime_icons};

use super::root::AppView;

/// Whether this platform has a freedesktop MIME database.
pub const SUPPORTED: bool = cfg!(all(unix, not(target_os = "macos")));

/// What the view keeps for the document icons.
#[derive(Default)]
pub struct DocumentIcons {
    /// The setting the watcher follows, `None` before the first sync.
    applied: Option<bool>,
    /// Watches the config folder while the setting is on.
    watcher: Option<FolderWatcher>,
    /// The last sync, which the next one waits for, so they never overlap or finish out of
    /// order.
    task: Option<Task<()>>,
}

impl AppView {
    /// Makes the icons follow `settings.document_icons` when it changed since the last call.
    /// Called at startup and by every change of the settings.
    pub(crate) fn follow_document_icons(&mut self, cx: &mut Context<Self>) {
        if !SUPPORTED {
            return;
        }
        let enabled = self.settings.document_icons;
        if self.document_icons.applied == Some(enabled) {
            return;
        }
        self.document_icons.applied = Some(enabled);
        let Some(dirs) = XdgDirs::from_env() else {
            log::warn!("document icons: the home folder is unknown");
            return;
        };
        self.document_icons.watcher = if enabled {
            self.watch_mime_defaults(&dirs, cx)
        } else {
            None
        };
        self.sync_document_icons(dirs, enabled, cx);
    }

    /// Starts watching the config folder, where file managers write `mimeapps.list`. Any change
    /// there syncs again: reading a few small files is cheap, and the database is only rebuilt
    /// when ted's package changes.
    fn watch_mime_defaults(
        &mut self,
        dirs: &XdgDirs,
        cx: &mut Context<Self>,
    ) -> Option<FolderWatcher> {
        let (mut watcher, changes) = match FolderWatcher::new() {
            Ok(watcher) => watcher,
            Err(error) => {
                log::warn!("document icons will not follow new defaults until restart: {error}");
                return None;
            }
        };
        watcher.watch(&dirs.config_home);
        let dirs = dirs.clone();
        // Sleeps on the channel: no work while nothing changes. Ends when the watcher is
        // dropped, which closes the channel.
        cx.spawn(async move |this, cx| {
            while changes.recv().await.is_ok() {
                let dirs = dirs.clone();
                let result = this.update(cx, |view, cx| {
                    let enabled = view.settings.document_icons;
                    view.sync_document_icons(dirs, enabled, cx)
                });
                if result.is_err() {
                    break;
                }
            }
        })
        .detach();
        Some(watcher)
    }

    /// Runs one sync in the background, after the one before it.
    fn sync_document_icons(&mut self, dirs: XdgDirs, enabled: bool, cx: &mut Context<Self>) {
        let previous = self.document_icons.task.take();
        let executor = cx.background_executor().clone();
        self.document_icons.task = Some(cx.spawn(async move |_, _| {
            if let Some(previous) = previous {
                previous.await;
            }
            let result = executor
                .spawn(async move { sync_mime_icons(&dirs, enabled) })
                .await;
            match result {
                Ok(sync) if sync.package_changed && sync.types.is_empty() => {
                    log::info!("document icons: removed");
                }
                Ok(sync) if sync.package_changed => {
                    log::info!(
                        "document icons: {} file types show ted's icon ({})",
                        sync.types.len(),
                        sync.types.iter().copied().collect::<Vec<_>>().join(", ")
                    );
                }
                Ok(_) => {}
                Err(error) => log::warn!("document icons: {error}"),
            }
        }));
    }
}
