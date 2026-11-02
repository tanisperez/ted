//! What the app does for Find in files (`views/find_files.rs`): opens the window with the open
//! folder and the buffers of the open documents, shows a match in its file (also one that is
//! still loading), leaves the results in a tab and does the replacing: in the buffer of a file
//! open in a tab (one undo step per file, not saved), on disk for the others (atomically).

use std::collections::HashMap;
use std::ops::Range;
use std::path::PathBuf;

use gpui_kit::component::WindowExt as _;
use gpui_kit::component::notification::Notification;
use gpui_kit::prelude::*;
use gpui_kit::*;
use rust_i18n::t;
use ted_core::{Buffer, SearchQuery, Selection};
use ted_workspace::replace_in_file;

use crate::state::launch::Target;
use crate::state::locale::plural;

use super::choice::{Choice, ChoiceStyle, ask};
use super::find_files::{FindFilesEvent, FindFilesView};
use super::find_results::{FindResultsEvent, FindResultsView};
use super::root::{AppView, DocumentView};

impl AppView {
    /// `Ctrl+Shift+F` and `Ctrl+Shift+R`: shows the window, with the selected text (on one line)
    /// in its query.
    pub(crate) fn open_find_in_files(
        &mut self,
        replace: bool,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if window.has_active_dialog(cx) {
            return;
        }
        let root = match &self.target {
            Some(Target::Folder(root)) => Some(root.clone()),
            _ => None,
        };
        let selected = self.editor().and_then(|editor| {
            editor
                .read(cx)
                .state
                .selected_text()
                .filter(|text| !text.contains('\n'))
        });
        let view = match &self.find_files {
            Some((view, _)) => view.clone(),
            None => {
                let view = cx.new(|cx| FindFilesView::new(window, cx));
                let subscription = cx.subscribe_in(&view, window, Self::on_find_files_event);
                self.find_files = Some((view.clone(), subscription));
                view
            }
        };
        let excludes = self.settings.explorer_exclude.clone();
        let buffers = self.open_buffers(cx);
        view.update(cx, |view, cx| {
            view.show(root, excludes, buffers, replace, selected, window, cx)
        });
        self.find_files_open = true;
        cx.notify();
    }

    /// The buffers of the documents open in tabs, by path.
    fn open_buffers(&self, cx: &App) -> HashMap<PathBuf, Buffer> {
        self.documents
            .items()
            .iter()
            .filter_map(|document| {
                let path = document.path.clone()?;
                let editor = document.editor()?;
                Some((path, editor.read(cx).state.buffer.clone()))
            })
            .collect()
    }

    fn on_find_files_event(
        &mut self,
        _: &Entity<FindFilesView>,
        event: &FindFilesEvent,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        match event {
            FindFilesEvent::Open { path, offset } => {
                self.reveal(path.clone(), offset.clone(), window, cx)
            }
            FindFilesEvent::OpenTab { query, files } => {
                let new_tab = self
                    .find_files
                    .as_ref()
                    .is_some_and(|(view, _)| view.read(cx).state.new_tab);
                self.open_results_tab(query.clone(), files.clone(), new_tab, window, cx);
            }
            FindFilesEvent::Replace {
                query,
                with,
                preserve_case,
                files,
                confirm,
            } => {
                let job = ReplaceJob {
                    query: query.clone(),
                    with: with.clone(),
                    preserve_case: *preserve_case,
                    files: files.clone(),
                };
                match confirm {
                    true => self.confirm_replace(job, window, cx),
                    false => self.replace_in_files(job, window, cx),
                }
            }
            FindFilesEvent::Close => {
                self.find_files_open = false;
                match self.editor() {
                    Some(editor) => {
                        let focus = editor.read(cx).focus_handle().clone();
                        focus.focus(window, cx);
                    }
                    None => self.focus_handle.focus(window, cx),
                }
                cx.notify();
            }
        }
    }

    /// Shows `offset` of the file at `path`, selected: in its tab when it is open, otherwise once
    /// it has loaded.
    pub(crate) fn reveal(
        &mut self,
        path: PathBuf,
        offset: Range<usize>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let open = self
            .documents
            .position(|document| document.path.as_deref() == Some(path.as_path()));
        let loaded = open.and_then(|index| {
            let document = self.documents.items().get(index)?;
            document.editor().cloned().map(|editor| (index, editor))
        });
        match loaded {
            Some((index, editor)) => {
                self.activate_document(index, window, cx);
                select_range(&editor, offset, window, cx);
            }
            None => {
                self.pending_reveal = Some((path.clone(), offset));
                self.open_path(&path, window, cx);
            }
        }
    }

    /// After a document loaded: selects the match [`Self::reveal`] was waiting to show in it.
    pub(crate) fn reveal_pending(
        &mut self,
        path: &std::path::Path,
        editor: &Entity<super::editor::EditorView>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if self
            .pending_reveal
            .as_ref()
            .is_some_and(|(pending, _)| pending == path)
            && let Some((_, offset)) = self.pending_reveal.take()
        {
            select_range(editor, offset, window, cx);
        }
    }

    /// Leaves results in a tab: the last results tab, replaced, or a new one with `new_tab`.
    fn open_results_tab(
        &mut self,
        query: String,
        files: Vec<crate::state::find_files::FileResult>,
        new_tab: bool,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let view = cx.new(|cx| FindResultsView::new(query, files, cx));
        let subscription = cx.subscribe_in(
            &view,
            window,
            |this, _, event: &FindResultsEvent, window, cx| match event {
                FindResultsEvent::Open { path, offset } => {
                    this.reveal(path.clone(), offset.clone(), window, cx)
                }
            },
        );
        let title = view.read(cx).title();
        let existing = (!new_tab)
            .then(|| {
                self.documents
                    .position(|document| matches!(document.view, DocumentView::FindResults(_)))
            })
            .flatten();
        match existing {
            Some(index) => {
                if let Some(document) = self.documents.get_mut(index) {
                    document.view = DocumentView::FindResults(view);
                    document.title = Some(title);
                    document._subscription = Some(subscription);
                }
                self.activate_document(index, window, cx);
            }
            None => {
                self.open_document_view(DocumentView::FindResults(view), Some(title), window, cx);
                if let Some(index) = self.documents.active_index()
                    && let Some(document) = self.documents.get_mut(index)
                {
                    document._subscription = Some(subscription);
                }
            }
        }
        cx.notify();
    }

    /// Replace all asks first: files that are not open are saved at once.
    fn confirm_replace(&mut self, job: ReplaceJob, window: &mut Window, cx: &mut Context<Self>) {
        let matches = job.files.iter().map(|(_, starts)| starts.len()).sum();
        let detail = t!(
            "find_files.confirm_detail",
            matches = plural("find_files.matches", matches),
            files = plural("find_files.files", job.files.len())
        );
        let answer = ask(
            window,
            cx,
            t!("find_files.confirm_title"),
            detail,
            vec![
                Choice::new(t!("common.cancel"), ChoiceStyle::Secondary),
                Choice::new(t!("find_files.replace"), ChoiceStyle::Primary),
            ],
        );
        cx.spawn_in(window, async move |this, cx| {
            if let Ok(1) = answer.recv().await {
                let _ = this.update_in(cx, |view, window, cx| {
                    view.replace_in_files(job, window, cx)
                });
            }
        })
        .detach();
    }

    /// Replaces the job's matches: in the buffers of open files at once, on disk for the others
    /// in the background. Then the window searches again.
    fn replace_in_files(&mut self, job: ReplaceJob, window: &mut Window, cx: &mut Context<Self>) {
        let mut in_buffers = 0;
        let mut files_changed = 0;
        let mut on_disk = Vec::new();
        for (path, starts) in job.files {
            let editor = self
                .documents
                .items()
                .iter()
                .find(|document| document.path.as_deref() == Some(path.as_path()))
                .and_then(|document| document.editor().cloned());
            match editor {
                Some(editor) => {
                    let replaced = editor.update(cx, |editor, cx| {
                        let state = &mut editor.state;
                        if state.read_only {
                            return 0;
                        }
                        let mut ranges = Vec::new();
                        job.query
                            .find_in(state.buffer.rope(), 0..state.buffer.len(), |found| {
                                if starts.contains(&found.start) {
                                    ranges.push(found);
                                }
                                true
                            });
                        state.replace_each(&ranges, |matched| {
                            job.query.replacement(matched, &job.with, job.preserve_case)
                        });
                        cx.notify();
                        ranges.len()
                    });
                    if replaced > 0 {
                        in_buffers += replaced;
                        files_changed += 1;
                    }
                }
                None => on_disk.push((path, starts)),
            }
        }
        let (query, with, preserve_case) = (job.query, job.with, job.preserve_case);
        cx.spawn_in(window, async move |this, cx| {
            let (replaced, files, failed) = cx
                .background_spawn(async move {
                    let (mut replaced, mut files, mut failed) = (0, 0, 0);
                    for (path, starts) in on_disk {
                        match replace_in_file(&path, &query, &with, preserve_case, Some(&starts)) {
                            Ok(0) => {}
                            Ok(count) => {
                                replaced += count;
                                files += 1;
                            }
                            Err(error) => {
                                log::warn!("cannot replace in a file: {error}");
                                failed += 1;
                            }
                        }
                    }
                    (replaced, files, failed)
                })
                .await;
            let _ = this.update_in(cx, |view, window, cx| {
                let matches = plural("find_files.matches", in_buffers + replaced);
                let files_label = plural("find_files.files", files_changed + files);
                log::info!(
                    "replaced {} matches in {} files",
                    in_buffers + replaced,
                    files_changed + files
                );
                let message = t!(
                    "find_files.replaced",
                    matches = matches,
                    files = files_label
                );
                let notification = match failed {
                    0 => Notification::success(message.into_owned()),
                    _ => Notification::warning(message.into_owned()),
                };
                window.push_notification(notification, cx);
                let buffers = view.open_buffers(cx);
                if let Some((find, _)) = &view.find_files {
                    find.update(cx, |find, cx| find.refresh(buffers, cx));
                }
            });
        })
        .detach();
    }
}

/// What Replace and Replace all ask for.
struct ReplaceJob {
    query: SearchQuery,
    with: String,
    preserve_case: bool,
    files: Vec<(PathBuf, Vec<usize>)>,
}

/// Selects `offset` in `editor`, scrolled into view, and gives it the focus.
fn select_range(
    editor: &Entity<super::editor::EditorView>,
    offset: Range<usize>,
    window: &mut Window,
    cx: &mut App,
) {
    editor.update(cx, |editor, cx| {
        let len = editor.state.buffer.len();
        editor.state.set_selection(Selection {
            anchor: offset.start.min(len),
            head: offset.end.min(len),
        });
        editor.state.center_cursor();
        cx.notify();
    });
    let focus = editor.read(cx).focus_handle().clone();
    focus.focus(window, cx);
}
