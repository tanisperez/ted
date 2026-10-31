//! Keeps the git status current when git runs outside ted (a terminal `git commit`, `git add` or
//! `git checkout`). A `notify` watcher on the repository's `.git` folder, not recursive, plus its
//! `refs`, sees `HEAD`, the index and the branches move; the status is read again once the events
//! stop for [`DEBOUNCE`]. The work tree itself is not watched: a recursive watch costs one OS
//! watch per folder and a walk of the whole tree, which a folder of 100,000 files cannot afford.
//! Files changed by other programs are caught when the window gets the focus back, and ted's own
//! saves read the status at once. While nothing changes nothing runs: the watcher thread blocks
//! in the OS and the task waits on the channel.

use std::path::{Component, Path};
use std::time::Duration;

use gpui_kit::prelude::*;
use gpui_kit::*;
use notify::{Event, EventKind, RecursiveMode, Watcher};

use super::super::root::AppView;

/// How long the events must stop before the status is read again. A commit or a checkout writes
/// several files at once; one read covers them all.
const DEBOUNCE: Duration = Duration::from_millis(300);

/// The live watcher and the task that turns its events into status reads. Dropping it stops
/// both.
pub(crate) struct GitWatcher {
    _watcher: notify::RecommendedWatcher,
    _task: Task<()>,
}

impl AppView {
    /// Starts watching the open repository's `.git` folder, replacing the previous watcher. With
    /// no repository it only stops the previous one. A watcher that cannot start is logged; the
    /// status is still read on focus and after ted's own writes.
    pub(crate) fn watch_git(&mut self, cx: &mut Context<Self>) {
        self.git_tasks.watcher = None;
        let Some(repo) = self.git.repo() else {
            return;
        };
        let git_dir = repo.root().join(".git");
        // A worktree's `.git` is a file pointing elsewhere: its changes are only seen on focus.
        if !git_dir.is_dir() {
            return;
        }
        let (sender, receiver) = async_channel::unbounded::<()>();
        let watcher = notify::recommended_watcher(move |result: notify::Result<Event>| {
            if let Ok(event) = result
                && is_relevant(&event)
            {
                let _ = sender.try_send(());
            }
        });
        let mut watcher = match watcher {
            Ok(watcher) => watcher,
            Err(error) => {
                log::warn!("cannot watch the repository: {error}");
                return;
            }
        };
        if let Err(error) = watcher.watch(&git_dir, RecursiveMode::NonRecursive) {
            log::warn!("cannot watch {}: {error}", git_dir.display());
            return;
        }
        let refs = git_dir.join("refs");
        if refs.is_dir()
            && let Err(error) = watcher.watch(&refs, RecursiveMode::Recursive)
        {
            log::warn!("cannot watch {}: {error}", refs.display());
        }
        let task = cx.spawn(async move |this, cx| {
            while receiver.recv().await.is_ok() {
                // Wait until the burst is over, then read the status once.
                loop {
                    cx.background_executor().timer(DEBOUNCE).await;
                    if receiver.is_empty() {
                        break;
                    }
                    while receiver.try_recv().is_ok() {}
                }
                if this
                    .update(cx, |view, cx| view.refresh_git_status(cx))
                    .is_err()
                {
                    break;
                }
            }
        });
        self.git_tasks.watcher = Some(GitWatcher {
            _watcher: watcher,
            _task: task,
        });
    }
}

/// Whether `event` may change `git status`: a create, modify or remove that is not a lock file
/// (git writes those constantly while it works) nor in git's object store or logs.
fn is_relevant(event: &Event) -> bool {
    if matches!(event.kind, EventKind::Access(_) | EventKind::Other) {
        return false;
    }
    event.paths.iter().any(|path| !is_git_noise(path))
}

fn is_git_noise(path: &Path) -> bool {
    if path
        .extension()
        .is_some_and(|extension| extension == "lock")
    {
        return true;
    }
    let mut components = path
        .components()
        .skip_while(|c| *c != Component::Normal(".git".as_ref()));
    if components.next().is_none() {
        return false;
    }
    matches!(
        components.next(),
        Some(Component::Normal(name)) if name == "objects" || name == "logs"
    )
}

#[cfg(test)]
mod tests {
    // Not `super::*`: gpui's own `test` attribute would shadow the standard one.
    use super::{is_git_noise, is_relevant};
    use notify::{Event, EventKind};
    use std::path::{Path, PathBuf};

    #[test]
    fn git_internals_are_noise_but_refs_and_index_are_not() {
        assert!(is_git_noise(Path::new("/repo/.git/objects/ab/cdef")));
        assert!(is_git_noise(Path::new("/repo/.git/logs/HEAD")));
        assert!(is_git_noise(Path::new("/repo/.git/index.lock")));
        assert!(!is_git_noise(Path::new("/repo/.git/index")));
        assert!(!is_git_noise(Path::new("/repo/.git/HEAD")));
        assert!(!is_git_noise(Path::new("/repo/.git/refs/heads/main")));
    }

    #[test]
    fn access_events_are_ignored() {
        let path = PathBuf::from("/repo/.git/index");
        let access =
            Event::new(EventKind::Access(notify::event::AccessKind::Any)).add_path(path.clone());
        let modify = Event::new(EventKind::Modify(notify::event::ModifyKind::Any)).add_path(path);
        assert!(!is_relevant(&access));
        assert!(is_relevant(&modify));
    }
}
