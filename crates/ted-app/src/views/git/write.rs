//! The git commands that change the repository (stage, unstage, discard, init). They run one at
//! a time on the background executor, and the status is read again when each one ends. A failure
//! shows git's own message in a notification.

use gpui_kit::component::WindowExt as _;
use gpui_kit::component::notification::Notification;
use gpui_kit::prelude::*;
use gpui_kit::*;
use rust_i18n::t;
use telaio_git::GitError;

use crate::state::git::RepoState;
use crate::state::launch::Target;

use super::super::root::DocumentView;
use super::DiffView;

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

    /// Reads the diff tab's patch again, after a status change.
    pub(crate) fn reload_git_diff(&mut self, cx: &mut Context<Self>) {
        for document in self.documents.items() {
            if let DocumentView::Diff(diff) = &document.view {
                diff.update(cx, |diff, cx| diff.reload(cx));
            }
        }
    }

    /// Opens a changed file of the panel in its tab. `path` is relative to the open folder.
    pub(crate) fn open_changed_file(
        &mut self,
        path: &str,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let Some(Target::Folder(root)) = &self.target else {
            return;
        };
        let path = root.join(path);
        self.open_path(&path, window, cx);
    }
}
