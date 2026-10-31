//! The Git integration's glue (milestone 0.3.0, ported from Postino): detecting git, discovering
//! the opened folder's repository and reading its status, always on the background executor. The
//! results land in [`AppView::git`] (`state::git`), which render reads.

use std::sync::Arc;

use gpui_kit::prelude::*;
use gpui_kit::*;
use telaio_git::{DiffSource, Executable};

use crate::state::git::{GitAvailability, RepoState, change_rows, explorer_marks, staged_outside};
use crate::state::git_sync::CommitStats;
use crate::state::launch::Target;

use super::root::AppView;

mod branch_menu;
mod branches;
mod panel;
mod sync;
mod watch;
mod write;

pub(crate) use branch_menu::{MenuContext, new_branch_menu};
pub(crate) use branches::CheckoutChoice;
pub(crate) use panel::letter_color;

/// The git work in flight. Replacing a task drops the previous one, so only the latest result is
/// applied.
#[derive(Default)]
pub(crate) struct GitTasks {
    detect: Option<Task<()>>,
    discover: Option<Task<()>>,
    status: Option<Task<()>>,
    /// The write in flight (`write.rs`); the next one waits for it.
    write: Option<Task<()>>,
    /// The commit box's summary line being read.
    commit_stats: Option<Task<()>>,
    /// The branches being read for the menu (`branches.rs`).
    branches: Option<Task<()>>,
    /// The watcher of the open repository's `.git` folder (`watch.rs`).
    watcher: Option<watch::GitWatcher>,
}

impl AppView {
    /// The executable the settings ask for.
    fn git_executable(&self) -> Executable {
        match &self.settings.git_executable {
            Some(path) => Executable::Path(path.clone()),
            None => Executable::Auto,
        }
    }

    /// Detects git again (at startup and when its setting changes), then discovers the open
    /// folder's repository.
    pub(crate) fn detect_git(&mut self, cx: &mut Context<Self>) {
        let executable = self.git_executable();
        self.git.availability = GitAvailability::Detecting;
        self.git_tasks.detect = Some(cx.spawn(async move |this, cx| {
            let result = cx
                .background_spawn(async move { telaio_git::detect(&executable) })
                .await;
            let _ = this.update(cx, |view, cx| {
                view.git.availability = match result {
                    Ok(git) => {
                        log::info!("git {} at {}", git.version(), git.path().display());
                        GitAvailability::Ready(git)
                    }
                    Err(error) => {
                        log::warn!("git is not available: {error}");
                        GitAvailability::Missing(Arc::new(error))
                    }
                };
                view.discover_git_repo(cx);
                cx.notify();
            });
        }));
    }

    /// Finds the repository of the open folder, then reads its status. Called when a folder
    /// opens or closes and after git is detected again.
    pub(crate) fn discover_git_repo(&mut self, cx: &mut Context<Self>) {
        self.git.repo = RepoState::Unknown;
        self.git.branches = None;
        // A new explorer starts without marks.
        self.git.marks = Arc::default();
        self.git_tasks.status = None;
        self.git.collapsed.clear();
        self.git.sync.last_fetch = None;
        self.git_tasks.watcher = None;
        self.rebuild_git_rows(cx);
        let root = match &self.target {
            Some(Target::Folder(root)) if self.explorer.is_some() => root.clone(),
            _ => {
                self.git_tasks.discover = None;
                return;
            }
        };
        let Some(git) = self.git.availability.git().cloned() else {
            self.git_tasks.discover = None;
            return;
        };
        self.git_tasks.discover = Some(cx.spawn(async move |this, cx| {
            let result = cx
                .background_spawn(async move { git.discover(&root) })
                .await;
            let _ = this.update(cx, |view, cx| {
                view.git.repo = match result {
                    Ok(Some(repo)) => {
                        log::debug!(
                            "git repository at {}, folder prefix {:?}",
                            repo.root().display(),
                            repo.prefix()
                        );
                        RepoState::Repo { repo, status: None }
                    }
                    Ok(None) => RepoState::NotARepo,
                    Err(error) => {
                        log::warn!("git repository discovery failed: {error}");
                        RepoState::Error(error.to_string())
                    }
                };
                view.watch_git(cx);
                view.refresh_git_status(cx);
                view.load_branches(cx);
                cx.notify();
            });
        }));
    }

    /// Reads `git status` again. The view is notified only when the status changed, so calling
    /// this on every focus or file change costs no frame while nothing changes.
    pub(crate) fn refresh_git_status(&mut self, cx: &mut Context<Self>) {
        let Some(repo) = self.git.repo().cloned() else {
            return;
        };
        self.git_tasks.status = Some(cx.spawn(async move |this, cx| {
            let reading = repo.clone();
            let result = cx
                .background_spawn(async move {
                    let started = std::time::Instant::now();
                    let status = reading.status();
                    log::debug!("git status read in {:?}", started.elapsed());
                    status
                })
                .await;
            let _ = this.update(cx, |view, cx| {
                // The folder may have changed while git ran.
                let same_repo = view
                    .git
                    .repo()
                    .is_some_and(|current| current.root() == repo.root());
                if !same_repo {
                    return;
                }
                match result {
                    Ok(status) => {
                        if let RepoState::Repo {
                            status: current, ..
                        } = &mut view.git.repo
                            && current.as_deref() != Some(&status)
                        {
                            *current = Some(Arc::new(status));
                            view.rebuild_git_rows(cx);
                            view.load_commit_stats(cx);
                            // A commit, checkout or fetch made anywhere moves the branches too.
                            view.load_branches(cx);
                            cx.notify();
                        }
                    }
                    Err(error) => {
                        log::warn!("git status failed: {error}");
                        view.git.repo = RepoState::Error(error.to_string());
                        view.rebuild_git_rows(cx);
                        cx.notify();
                    }
                }
            });
        }));
    }

    /// Reads what the next commit would include, for the commit box's summary line.
    fn load_commit_stats(&mut self, cx: &mut Context<Self>) {
        let Some(repo) = self.git.repo().cloned() else {
            return;
        };
        self.git_tasks.commit_stats = Some(cx.spawn(async move |this, cx| {
            let result = cx
                .background_spawn(async move { repo.numstat(&DiffSource::HeadToIndex) })
                .await;
            let _ = this.update(cx, |view, cx| match result {
                Ok(numstat) => {
                    let stats = CommitStats::from_numstat(&numstat);
                    if view.git.commit.stats != stats {
                        view.git.commit.stats = stats;
                        cx.notify();
                    }
                }
                Err(error) => log::warn!("reading the staged changes failed: {error}"),
            });
        }));
    }

    /// Rebuilds the panel's rows from the last status and the foldings. Never called in render.
    pub(crate) fn rebuild_git_rows(&mut self, cx: &mut App) {
        let (rows, outside, marks) = match self.git.status() {
            Some((repo, status)) => (
                change_rows(status, repo.prefix(), &self.git.collapsed, self.git.flat),
                staged_outside(status, repo.prefix()),
                explorer_marks(status, repo.prefix()),
            ),
            None => (Vec::new(), Vec::new(), Default::default()),
        };
        self.git.rows = Arc::new(rows);
        self.git.staged_outside = Arc::new(outside);
        let marks = Arc::new(marks);
        if *marks != *self.git.marks {
            self.git.marks = marks.clone();
            if let Some((explorer, _)) = &self.explorer {
                explorer.update(cx, |explorer, cx| explorer.set_git_marks(marks, cx));
            }
        }
    }
}
