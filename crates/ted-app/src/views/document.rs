//! The open documents and their files: saving (`Ctrl+S`, with the save dialog for a new
//! document), asking before unsaved changes are lost
//! (closing a tab, opening something else, closing the window), and noticing when another program changed the
//! file. Every file access runs on the background executor.
//!
//! Saving takes a snapshot of the buffer (cloning a rope is O(1)), computes the whitespace
//! cleanup from it in the background, applies the cleanup as one undo step if nothing was typed
//! meanwhile, and writes the text atomically. The history's checkpoint taken with the text marks
//! the saved step, so undoing back to it makes the document clean again.

use gpui_kit::component::notification::Notification;
use gpui_kit::component::*;
use gpui_kit::*;
use rust_i18n::t;
use ted_workspace::{FileContent, load_file, modified_time, save_file};

use crate::state::launch::{Target, display_name};

use super::choice::{Choice, ChoiceStyle, ask};
use super::editor::EditorView;

use super::root::{AppView, Document};

impl AppView {
    /// Saves the document shown. The task resolves to whether the file was written.
    pub(crate) fn save(&mut self, window: &mut Window, cx: &mut Context<Self>) -> Task<bool> {
        match self.editor().cloned() {
            Some(editor) => self.save_editor(editor, window, cx),
            None => Task::ready(false),
        }
    }

    /// Saves the document of `editor`, asking where first when it has no file yet. The task
    /// resolves to whether the file was written.
    fn save_editor(
        &mut self,
        editor: Entity<EditorView>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Task<bool> {
        match editor.read(cx).state.path.clone() {
            Some(path) => self.write(editor, path, window, cx),
            None => self.save_as(editor, window, cx),
        }
    }

    /// Asks with the save dialog where to write a new document (in folder mode, starting at the
    /// folder), then writes it there and turns it into a normal file.
    fn save_as(
        &mut self,
        editor: Entity<EditorView>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Task<bool> {
        let directory = match &self.target {
            Some(Target::Folder(root)) => root.clone(),
            _ => std::env::home_dir().unwrap_or_default(),
        };
        let name = self.document_name(&editor);
        let receiver = cx.prompt_for_new_path(&directory, Some(&name));
        cx.spawn_in(window, async move |this, cx| {
            let Ok(Ok(Some(path))) = receiver.await else {
                return false;
            };
            let Ok(writing) = this.update_in(cx, |this, window, cx| {
                this.write(editor.clone(), path.clone(), window, cx)
            }) else {
                return false;
            };
            if !writing.await {
                return false;
            }
            let _ = this.update_in(cx, |this, window, cx| {
                this.saved_as(&editor, path, window, cx)
            });
            true
        })
    }

    /// The name of the document of `editor`, as its tab shows it.
    fn document_name(&self, editor: &Entity<EditorView>) -> String {
        self.documents
            .items()
            .iter()
            .find(|document| document.editor() == Some(editor))
            .map(Document::name)
            .unwrap_or_default()
    }

    /// Writes the text of `editor` to `path`, after the whitespace cleanup. The task resolves to
    /// whether the file was written.
    fn write(
        &mut self,
        editor: Entity<EditorView>,
        path: std::path::PathBuf,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Task<bool> {
        let state = &editor.read(cx).state;
        if state.read_only {
            window.push_notification(Notification::warning(t!("save.read_only").into_owned()), cx);
            return Task::ready(false);
        }
        let known = state.disk_modified;
        let snapshot = state.buffer.clone();
        let (trim, final_newline) = (
            self.settings.trim_trailing_whitespace,
            self.settings.insert_final_newline,
        );
        cx.spawn_in(window, async move |this, cx| {
            // Another program changed the file since it was loaded: ask before overwriting.
            let stat_path = path.clone();
            let on_disk = cx
                .background_spawn(async move { modified_time(&stat_path) })
                .await;
            if on_disk.is_some() && known.is_some() && on_disk != known {
                let answer = this.update_in(cx, |_, window, cx| {
                    ask(
                        window,
                        cx,
                        t!("save.changed_on_disk", name = display_name(&path)),
                        t!("save.changed_on_disk_detail"),
                        vec![
                            Choice::new(t!("common.cancel"), ChoiceStyle::Secondary),
                            Choice::new(t!("save.overwrite"), ChoiceStyle::Primary),
                        ],
                    )
                });
                let Ok(answer) = answer else { return false };
                if answer.recv().await != Ok(1) {
                    return false;
                }
            }

            let version = snapshot.version();
            let edits = cx
                .background_spawn(
                    async move { ted_core::cleanup_edits(&snapshot, trim, final_newline) },
                )
                .await;
            let taken = editor.update(cx, |editor, cx| {
                let state = &mut editor.state;
                if state.buffer.version() == version {
                    state.apply_cleanup(edits);
                    cx.notify();
                }
                let checkpoint = state.history.checkpoint();
                (state.buffer.rope().clone(), state.bom, checkpoint)
            });
            let (rope, bom, checkpoint) = taken;
            let save_path = path.clone();
            let result = cx
                .background_spawn(async move { save_file(&save_path, &rope, bom) })
                .await;
            let saved = result.is_ok();
            let _ = this.update_in(cx, |this, window, cx| {
                match result {
                    Ok(modified) => {
                        editor.update(cx, |editor, cx| {
                            editor.state.history.set_saved(checkpoint);
                            editor.state.disk_modified = modified;
                            cx.notify();
                        });
                        this.refresh_git_status(cx);
                        this.update_gutter(&editor, cx);
                    }
                    Err(error) => {
                        log::warn!("{error}");
                        window.push_notification(Notification::error(error.to_string()), cx);
                    }
                }
                this.update_title(window, cx);
            });
            saved
        })
    }

    /// Runs `then` at once when none of `editors` has unsaved changes. Otherwise asks, one
    /// document at a time, to save, not save or cancel, and runs it once every one was saved (if
    /// the save worked) or discarded. Cancel stops there.
    pub(crate) fn confirm_discard(
        &mut self,
        editors: Vec<Entity<EditorView>>,
        window: &mut Window,
        cx: &mut Context<Self>,
        then: impl FnOnce(&mut Self, &mut Window, &mut Context<Self>) + 'static,
    ) {
        let mut dirty: Vec<_> = editors
            .into_iter()
            .filter(|editor| editor.read(cx).state.has_unsaved())
            .collect();
        if dirty.is_empty() {
            return then(self, window, cx);
        }
        let editor = dirty.remove(0);
        let name = self.document_name(&editor);
        let answer = ask(
            window,
            cx,
            t!("save.unsaved", name = name),
            t!("save.unsaved_detail"),
            vec![
                Choice::new(t!("save.dont_save"), ChoiceStyle::Danger),
                Choice::new(t!("common.cancel"), ChoiceStyle::Secondary),
                Choice::new(t!("save.save"), ChoiceStyle::Primary),
            ],
        );
        cx.spawn_in(window, async move |this, cx| match answer.recv().await {
            Ok(2) => {
                let Ok(saving) =
                    this.update_in(cx, |this, window, cx| this.save_editor(editor, window, cx))
                else {
                    return;
                };
                if saving.await {
                    let _ = this.update_in(cx, |this, window, cx| {
                        this.confirm_discard(dirty, window, cx, then)
                    });
                }
            }
            Ok(0) => {
                let _ = this.update_in(cx, |this, window, cx| {
                    this.confirm_discard(dirty, window, cx, then)
                });
            }
            _ => {}
        })
        .detach();
    }

    /// Whether the window may close now. With unsaved changes it asks first and closes the window
    /// itself once they are saved or discarded.
    pub fn should_close(&mut self, window: &mut Window, cx: &mut Context<Self>) -> bool {
        let editors = self.editors();
        let dirty = editors
            .iter()
            .any(|editor| editor.read(cx).state.has_unsaved());
        if self.closing || !dirty {
            return true;
        }
        self.confirm_discard(editors, window, cx, |this, window, _| {
            this.closing = true;
            window.remove_window();
        });
        false
    }

    /// Looks at the open file on disk (the window got the focus back). Changed by another
    /// program: reloaded at once when there is nothing to lose, otherwise after asking.
    pub(crate) fn check_disk(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let Some(editor) = self.editor().cloned() else {
            return;
        };
        let state = &editor.read(cx).state;
        let (Some(path), Some(known)) = (state.path.clone(), state.disk_modified) else {
            return;
        };
        cx.spawn_in(window, async move |this, cx| {
            let stat_path = path.clone();
            let on_disk = cx
                .background_spawn(async move { modified_time(&stat_path) })
                .await;
            // Removed meanwhile: keep the text, saving writes it again.
            let Some(on_disk) = on_disk else { return };
            if on_disk == known {
                return;
            }
            let dirty = editor.read_with(cx, |editor, _| editor.state.is_dirty());
            if dirty {
                let answer = this.update_in(cx, |_, window, cx| {
                    ask(
                        window,
                        cx,
                        t!("save.changed_on_disk", name = display_name(&path)),
                        t!("save.reload_detail"),
                        vec![
                            Choice::new(t!("save.keep_mine"), ChoiceStyle::Secondary),
                            Choice::new(t!("save.reload"), ChoiceStyle::Primary),
                        ],
                    )
                });
                let Ok(answer) = answer else { return };
                if answer.recv().await != Ok(1) {
                    // Keep mine: do not ask again for this change.
                    editor.update(cx, |editor, _| {
                        editor.state.disk_modified = Some(on_disk);
                    });
                    return;
                }
            }
            log::info!("reloading a file changed on disk");
            let load_path = path.clone();
            let loaded = cx
                .background_spawn(async move { load_file(&load_path) })
                .await;
            let Ok(loaded) = loaded else { return };
            let FileContent::Text { buffer, .. } = loaded.content else {
                return;
            };
            editor.update(cx, |editor, cx| {
                editor.state.reload(buffer, loaded.modified);
                cx.notify();
            });
            let _ = this.update_in(cx, |this, window, cx| this.update_title(window, cx));
        })
        .detach();
    }
}
