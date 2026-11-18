//! Moving a file or a folder dragged in the explorer (#98): asks before replacing an entry of
//! the same name, moves it on the background executor, and keeps the tabs of the files inside it
//! open on their new paths.

use std::path::{Path, PathBuf};

use gpui_kit::component::WindowExt as _;
use gpui_kit::component::notification::Notification;
use gpui_kit::prelude::*;
use gpui_kit::*;
use rust_i18n::t;
use ted_workspace::{move_into, move_target, pointless_move};

use super::choice::{Choice, ChoiceStyle, ask};
use super::root::AppView;
use crate::state::launch::display_name;

/// Where a path inside `from` lands once `from` moved to `to`, `None` for a path outside it.
pub(crate) fn moved_path(path: &Path, from: &Path, to: &Path) -> Option<PathBuf> {
    let rest = path.strip_prefix(from).ok()?;
    // `join("")` would end the path in a separator.
    Some(if rest.as_os_str().is_empty() {
        to.to_path_buf()
    } else {
        to.join(rest)
    })
}

impl AppView {
    /// Moves `from` into the folder `to_dir`, asking first when `to_dir` already has an entry of
    /// that name. Dropping it where it is, or into itself, does nothing.
    pub(crate) fn move_entry(
        &mut self,
        from: PathBuf,
        to_dir: PathBuf,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if pointless_move(&from, &to_dir) {
            return;
        }
        let Some(target) = move_target(&from, &to_dir) else {
            return;
        };
        if std::fs::symlink_metadata(&target).is_err() {
            self.run_move(from, to_dir, false, window, cx);
            return;
        }
        let name = display_name(&from);
        let folder = display_name(&to_dir);
        let answer = ask(
            window,
            cx,
            t!("explorer.move.replace_title", name = name),
            t!("explorer.move.replace_detail", name = name, folder = folder),
            vec![
                Choice::new(t!("common.cancel"), ChoiceStyle::Secondary),
                Choice::new(t!("explorer.move.replace"), ChoiceStyle::Danger),
            ],
        );
        cx.spawn_in(window, async move |this, cx| {
            if answer.recv().await == Ok(1) {
                let _ = this.update_in(cx, |this, window, cx| {
                    this.run_move(from, to_dir, true, window, cx)
                });
            }
        })
        .detach();
    }

    /// Moves on the background executor, then retargets the open tabs.
    fn run_move(
        &mut self,
        from: PathBuf,
        to_dir: PathBuf,
        replace: bool,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        log::debug!("moving {} into {}", from.display(), to_dir.display());
        let task = cx.background_spawn({
            let (from, to_dir) = (from.clone(), to_dir.clone());
            async move { move_into(&from, &to_dir, replace) }
        });
        cx.spawn_in(window, async move |this, cx| {
            let result = task.await;
            let _ = this.update_in(cx, |this, window, cx| match result {
                Ok(to) => this.retarget_documents(&from, &to, window, cx),
                Err(error) => {
                    log::warn!("moving {} failed: {error}", from.display());
                    let name = display_name(&from);
                    window.push_notification(
                        Notification::error(
                            t!(
                                "explorer.move.failed",
                                name = name,
                                error = error.to_string()
                            )
                            .into_owned(),
                        ),
                        cx,
                    );
                }
            });
        })
        .detach();
    }

    /// The tabs of the files inside `from` follow it to `to`.
    fn retarget_documents(
        &mut self,
        from: &Path,
        to: &Path,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let moved: Vec<_> = self
            .documents
            .items()
            .iter()
            .filter_map(|document| {
                let path = document.path.as_deref()?;
                let new = moved_path(path, from, to)?;
                Some((document.editor().cloned(), new))
            })
            .collect();
        for (editor, new) in moved {
            if let Some(editor) = editor {
                self.saved_as(&editor, new, window, cx);
            }
        }
        cx.notify();
    }
}

#[cfg(test)]
mod tests {
    use std::path::{Path, PathBuf};

    use super::moved_path;

    #[test]
    fn paths_inside_follow_the_move() {
        let (from, to) = (Path::new("/w/src"), Path::new("/w/lib/src"));
        assert_eq!(
            moved_path(Path::new("/w/src/a/b.rs"), from, to),
            Some(PathBuf::from("/w/lib/src/a/b.rs"))
        );
        // The moved file itself, with no separator at the end.
        assert_eq!(
            moved_path(from, from, to).map(|path| path.display().to_string()),
            Some("/w/lib/src".to_string())
        );
        assert_eq!(moved_path(Path::new("/w/srcx/a.rs"), from, to), None);
    }
}
