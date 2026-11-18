//! The git commands that change the repository (stage, unstage, discard, init). They run one at
//! a time on the background executor, and the status is read again when each one ends. A failure
//! shows git's own message in a notification.

use gpui_kit::component::WindowExt as _;
use gpui_kit::component::notification::Notification;
use gpui_kit::prelude::*;
use gpui_kit::*;
use rust_i18n::t;
use telaio_git::GitError;

use std::collections::HashSet;

use crate::state::git::{ChangeRowKind, RepoState, change_rows};
use crate::state::git_diff::neighbour;
use crate::state::launch::Target;

use super::super::root::DocumentView;
use super::{DiffView, HistoryView};

use super::super::root::AppView;

impl AppView {
    /// Runs `write` on the background executor after the write in flight, if any, then reads the
    /// status again.
    pub(crate) fn run_git_write(
        &mut self,
        window: &mut Window,
        cx: &mut Context<Self>,
        write: impl FnOnce() -> Result<(), GitError> + Send + 'static,
    ) {
        let previous = self.git_tasks.write.take();
        self.git_tasks.write = Some(cx.spawn_in(window, async move |this, cx| {
            if let Some(previous) = previous {
                previous.await;
            }
            let result = cx.background_spawn(async move { write() }).await;
            let _ = this.update_in(cx, |view, window, cx| {
                if let Err(error) = result {
                    view.show_git_error(&error.to_string(), window, cx);
                }
                view.refresh_git_status(cx);
                cx.notify();
            });
        }));
    }

    /// Shows a failed git command: its own message, verbatim, in a notification.
    pub(crate) fn show_git_error(&self, message: &str, window: &mut Window, cx: &mut App) {
        log::warn!("git command failed: {message}");
        window.push_notification(
            Notification::error(message.to_string()).title(t!("git.error.title").into_owned()),
            cx,
        );
    }

    /// Runs `git init` in the open folder, then discovers the new repository.
    pub(crate) fn init_git_repository(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let (Some(git), Some(Target::Folder(root))) =
            (self.git.availability.git().cloned(), self.target.clone())
        else {
            return;
        };
        if !matches!(self.git.repo, RepoState::NotARepo) {
            return;
        }
        self.git_tasks.write = Some(cx.spawn_in(window, async move |this, cx| {
            let result = cx.background_spawn(async move { git.init(&root) }).await;
            let _ = this.update_in(cx, |view, window, cx| {
                match result {
                    Ok(()) => log::info!("initialized a git repository in the folder"),
                    Err(error) => view.show_git_error(&error.to_string(), window, cx),
                }
                view.discover_git_repo(cx);
                cx.notify();
            });
        }));
    }

    /// Shows the diff of a changed file in the diff tab, opening it or reusing it.
    pub(crate) fn open_git_diff(
        &mut self,
        repo_path: String,
        path: String,
        letter: char,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let Some(repo) = self.git.repo().cloned() else {
            return;
        };
        if let Some(index) = self
            .documents
            .position(|document| matches!(document.view, DocumentView::Diff(_)))
        {
            if let Some(document) = self.documents.get_mut(index)
                && let DocumentView::Diff(diff) = &document.view
            {
                let diff = diff.clone();
                let title = diff.update(cx, |diff, cx| {
                    diff.show(repo, repo_path, path, letter, cx);
                    diff.title()
                });
                document.title = Some(title);
            }
            self.activate_document(index, window, cx);
            cx.notify();
            return;
        }
        let app = cx.weak_entity();
        let diff = cx.new(|cx| DiffView::new(repo, repo_path, path, letter, app, cx));
        let title = diff.read(cx).title();
        self.open_document_view(DocumentView::Diff(diff), Some(title), window, cx);
    }

    /// Shows the history tab, opening it when needed.
    pub(crate) fn open_git_history(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let Some((repo, status)) = self.git.status() else {
            return;
        };
        let (repo, head) = (repo.clone(), status.head_oid.clone());
        if let Some(index) = self
            .documents
            .position(|document| matches!(document.view, DocumentView::History(_)))
        {
            self.activate_document(index, window, cx);
            cx.notify();
            return;
        }
        let history = cx.new(|cx| HistoryView::new(repo, head, window, cx));
        self.open_document_view(
            DocumentView::History(history),
            Some(HistoryView::title()),
            window,
            cx,
        );
    }

    /// Tells the history tab where `HEAD` is now: it reads the log again when it moved.
    pub(crate) fn update_git_history(&mut self, cx: &mut Context<Self>) {
        let head = self
            .git
            .status()
            .and_then(|(_, status)| status.head_oid.clone());
        for document in self.documents.items() {
            if let DocumentView::History(history) = &document.view {
                let head = head.clone();
                history.update(cx, |history, cx| history.head_changed(head, cx));
            }
        }
    }

    /// Reads the diff tab's patch again, after a status change.
    pub(crate) fn reload_git_diff(&mut self, cx: &mut Context<Self>) {
        for document in self.documents.items() {
            if let DocumentView::Diff(diff) = &document.view {
                diff.update(cx, |diff, cx| diff.reload(cx));
            }
        }
    }

    /// Opens a changed file of the panel in its tab. `path` is relative to the open folder.
    pub(crate) fn open_changed_file_at(
        &mut self,
        path: &str,
        line: Option<usize>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let Some(Target::Folder(root)) = &self.target else {
            return;
        };
        let path = root.join(path);
        let open = self
            .documents
            .position(|document| document.path.as_deref() == Some(path.as_path()));
        let loaded = open.and_then(|index| {
            let document = self.documents.items().get(index)?;
            document.editor().cloned().map(|editor| (index, editor))
        });
        match (loaded, line) {
            (Some((index, editor)), Some(line)) => {
                self.activate_document(index, window, cx);
                super::super::find_files_app::cursor_to_line(&editor, line, window, cx);
            }
            (_, line) => {
                self.pending_line = line.map(|line| (path.clone(), line));
                self.open_path(&path, window, cx);
            }
        }
    }

    /// `F7` past the last block, or `Shift+F7` before the first: the diff of the next or previous
    /// changed file in the Git panel's order, folded folders included, landing on its first block
    /// (or its last, going back). Nothing past the first or last file.
    pub(crate) fn open_neighbour_diff(
        &mut self,
        repo_path: &str,
        forward: bool,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let Some((repo, status)) = self.git.status() else {
            return;
        };
        let files: Vec<(String, String, char)> =
            change_rows(status, repo.prefix(), &HashSet::new(), self.git.flat)
                .into_iter()
                .filter_map(|row| match row.kind {
                    ChangeRowKind::File { letter, .. } => {
                        let repo_path = row.repo_paths.first()?.clone();
                        Some((repo_path, row.path, letter))
                    }
                    _ => None,
                })
                .collect();
        let Some(current) = files.iter().position(|(path, _, _)| path == repo_path) else {
            return;
        };
        let Some((repo_path, path, letter)) = neighbour(&files, current, forward).cloned() else {
            return;
        };
        if !forward
            && let Some(diff) =
                self.documents
                    .items()
                    .iter()
                    .find_map(|document| match &document.view {
                        DocumentView::Diff(diff) => Some(diff.clone()),
                        _ => None,
                    })
        {
            diff.update(cx, |diff, _| diff.land_on_last());
        }
        self.open_git_diff(repo_path, path, letter, window, cx);
    }
}
