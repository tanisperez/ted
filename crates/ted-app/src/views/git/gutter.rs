//! Fills the editors' git change marks (`state::git_gutter`). Only a file the status lists as
//! changed costs a `git diff`; a clean file gets no marks without running git. The diff runs on
//! the background executor and its marks are kept only while the buffer is still the text that
//! was on disk: an edit hides them until the next save.

use std::path::Path;
use std::sync::Arc;

use gpui_kit::prelude::*;
use gpui_kit::*;
use telaio_git::{Change, DiffSource, parse_file_diff};

use crate::state::git_gutter::{GitMarks, gutter_marks};
use crate::state::launch::Target;

use super::super::editor::EditorView;
use super::super::root::AppView;

impl AppView {
    /// Reads the marks of every open editor again, after a status change.
    pub(crate) fn update_all_gutters(&mut self, cx: &mut Context<Self>) {
        for editor in self.editors() {
            self.update_gutter(&editor, cx);
        }
    }

    /// Reads the marks of `editor` again, after it was loaded or saved.
    pub(crate) fn update_gutter(&mut self, editor: &Entity<EditorView>, cx: &mut Context<Self>) {
        let state = &editor.read(cx).state;
        let version = state.buffer.version();
        let path = state.path.clone();
        let dirty = state.has_unsaved();
        let had_marks = state.git_marks.is_some();
        let target = match (&path, self.repo_path_of(path.as_deref())) {
            (Some(_), Some(repo_path)) if !dirty => Some(repo_path),
            _ => None,
        };
        let Some((repo, repo_path)) = target.and_then(|repo_path| {
            let (repo, status) = self.git.status()?;
            let changed = status.entries.iter().any(|entry| {
                entry.path == repo_path && !entry.untracked && entry.worktree != Change::Deleted
            });
            changed.then(|| (repo.clone(), repo_path))
        }) else {
            if had_marks {
                editor.update(cx, |editor, cx| {
                    editor.state.git_marks = None;
                    cx.notify();
                });
            }
            return;
        };
        let editor = editor.downgrade();
        cx.spawn(async move |_, cx| {
            let marks = cx
                .background_spawn(async move {
                    repo.diff(&repo_path, &DiffSource::HeadToWorkTree)
                        .map(|patch| gutter_marks(&parse_file_diff(&patch)))
                })
                .await;
            let marks = match marks {
                Ok(marks) => marks,
                Err(error) => {
                    log::warn!("reading the gutter marks failed: {error}");
                    return;
                }
            };
            let _ = editor.update(cx, |editor, cx| {
                if editor.state.buffer.version() == version {
                    editor.state.git_marks = Some(GitMarks {
                        version,
                        marks: Arc::new(marks),
                    });
                    cx.notify();
                }
            });
        })
        .detach();
    }

    /// The path of `path` relative to the repository root, `/` separated, when it is inside the
    /// open folder's repository.
    fn repo_path_of(&self, path: Option<&Path>) -> Option<String> {
        let (repo, Some(Target::Folder(root))) = (self.git.repo()?, &self.target) else {
            return None;
        };
        let relative = path?.strip_prefix(root).ok()?;
        let mut parts: Vec<String> = Vec::new();
        if !repo.prefix().is_empty() {
            parts.push(repo.prefix().to_string());
        }
        parts.extend(
            relative
                .components()
                .map(|part| part.as_os_str().to_string_lossy().into_owned()),
        );
        Some(parts.join("/"))
    }
}
