//! The main window's view: the title bar, the content area and the status bar.
//!
//! What it shows depends on what was opened (`state::launch`): nothing (the empty state with the
//! recents, `views/welcome.rs`), one file alone (file mode, the editor fills the window) or a
//! folder (folder mode; the explorer and the tabs come with #6 and #8). Files load in the
//! background; a generation counter drops the result of a load that a newer one replaced.

use std::path::{Path, PathBuf};

use gpui_kit::component::*;
use gpui_kit::prelude::*;
use gpui_kit::*;
use rust_i18n::t;
use ted_workspace::{FileContent, LoadedFile, load_file, load_head};
use telaio_ui::components::EmptyState;

use crate::actions::{OpenFile, OpenFolder, Save, ToggleExplorer};
use crate::state::editor::{EditorState, estimate_lines};
use crate::state::launch::{self, LaunchError, Target};
use crate::state::recents::{self, Recents};
use crate::state::settings::Settings;
use crate::theme::PaletteExt;
use crate::theme::metrics::{
    EXPLORER_RESIZE_HANDLE, EXPLORER_WIDTH, EXPLORER_WIDTH_MAX, EXPLORER_WIDTH_MIN,
};

use super::editor::EditorView;
use super::explorer::{ExplorerEvent, ExplorerView};

/// Files at least this big show their first [`HEAD_BYTES`] while the rest loads.
const HEAD_THRESHOLD: u64 = 8 * 1024 * 1024;
/// How much of a big file is read first: many screens, and still under a millisecond.
const HEAD_BYTES: usize = 256 * 1024;

/// What the content area shows.
enum Content {
    /// Nothing opened: the empty state with the recents.
    Welcome,
    /// A file is being read.
    Loading,
    /// A folder is open and no file yet.
    Folder,
    /// Something could not be opened or shown: a title and a detail.
    Message(SharedString, Option<SharedString>),
    Editor(Entity<EditorView>),
}

/// The root view of the main window.
pub struct AppView {
    /// The settings in effect, read once at startup.
    pub settings: Settings,
    /// The recently opened folders and files.
    pub(crate) recents: Recents,
    /// What is open: `None` in the empty state.
    pub(crate) target: Option<Target>,
    /// The file shown in folder mode, `None` until one is opened.
    pub(crate) file: Option<PathBuf>,
    content: Content,
    /// Bumped by every load, so a slow load cannot replace a newer one.
    load_generation: u64,
    focus_handle: FocusHandle,
    /// The explorer of folder mode, with the subscription to its events.
    pub(crate) explorer: Option<(Entity<ExplorerView>, Subscription)>,
    /// Whether the explorer panel is shown (`Ctrl+B`).
    pub(crate) explorer_visible: bool,
    /// The explorer panel's width, changed by dragging its edge.
    explorer_width: f32,
    /// While the explorer's edge is dragged.
    resizing_explorer: bool,
    /// Keeps the window title in step with the editor (its dirty state).
    editor_subscription: Option<Subscription>,
    /// The window title last set, so it is only set again when it changes.
    last_title: String,
    /// The user chose to close the window without saving: it may close now.
    pub(crate) closing: bool,
    /// Checks the open file on disk when the window gets the focus back.
    _activation: Subscription,
}

impl AppView {
    pub fn new(
        settings: Settings,
        path: Option<PathBuf>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Self {
        let focus_handle = cx.focus_handle();
        focus_handle.focus(window, cx);
        let activation = cx.observe_window_activation(window, |this, window, cx| {
            if window.is_window_active() {
                this.check_disk(window, cx);
            }
        });
        let mut view = Self {
            settings,
            recents: recents::load(),
            target: None,
            file: None,
            content: Content::Welcome,
            load_generation: 0,
            focus_handle,
            explorer: None,
            explorer_visible: true,
            explorer_width: EXPLORER_WIDTH,
            resizing_explorer: false,
            editor_subscription: None,
            last_title: String::new(),
            closing: false,
            _activation: activation,
        };
        view.update_title(window, cx);
        if let Some(path) = path {
            view.open_path(&path, window, cx);
        }
        view
    }

    /// The editor shown, if any.
    pub(crate) fn editor(&self) -> Option<&Entity<EditorView>> {
        match &self.content {
            Content::Editor(editor) => Some(editor),
            _ => None,
        }
    }

    /// Opens a file or a folder, from the command line, a dialog or the recents, asking first
    /// when the open document has unsaved changes.
    pub(crate) fn open_path(&mut self, path: &Path, window: &mut Window, cx: &mut Context<Self>) {
        let path = path.to_path_buf();
        self.confirm_discard(window, cx, move |this, window, cx| {
            this.open_path_now(&path, window, cx)
        });
    }

    /// Opens a file or a folder. A file opened while a folder is open is shown inside folder
    /// mode.
    fn open_path_now(&mut self, path: &Path, window: &mut Window, cx: &mut Context<Self>) {
        match launch::resolve(path) {
            Ok(Target::File(file)) => {
                log::info!("opening a file");
                self.recents.add(&Target::File(file.clone()));
                if matches!(self.target, Some(Target::Folder(_))) {
                    self.file = Some(file.clone());
                    if let Some((explorer, _)) = &self.explorer {
                        explorer.update(cx, |explorer, cx| explorer.reveal(&file, cx));
                    }
                } else {
                    self.explorer = None;
                    self.target = Some(Target::File(file.clone()));
                    self.file = None;
                }
                self.load(file, window, cx);
            }
            Ok(target @ Target::Folder(_)) => {
                log::info!("opening a folder");
                self.recents.add(&target);
                let explorer = cx.new(|cx| ExplorerView::new(target.path().to_path_buf(), cx));
                let subscription = cx.subscribe_in(
                    &explorer,
                    window,
                    |this, _, event: &ExplorerEvent, window, cx| match event {
                        ExplorerEvent::Open(path) => this.open_path(path, window, cx),
                    },
                );
                self.explorer = Some((explorer, subscription));
                self.explorer_visible = true;
                self.target = Some(target);
                self.file = None;
                self.load_generation += 1;
                self.content = Content::Folder;
            }
            Err(error) => {
                let (title, path) = match &error {
                    LaunchError::NotFound(path) => (t!("open.not_found"), path),
                    LaunchError::Unreadable(path, _) => (t!("open.unreadable"), path),
                };
                log::warn!("cannot open a path: {error:?}");
                self.recents.remove(path);
                self.content = Content::Message(
                    title.into_owned().into(),
                    Some(path.display().to_string().into()),
                );
            }
        }
        recents::save(&self.recents);
        self.update_title(window, cx);
        cx.notify();
    }

    /// Asks for a file or a folder with the native dialog and opens it.
    pub(crate) fn prompt_open(
        &mut self,
        folder: bool,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let receiver = cx.prompt_for_paths(PathPromptOptions {
            files: !folder,
            directories: folder,
            multiple: false,
            prompt: None,
        });
        cx.spawn_in(window, async move |this, cx| {
            let Ok(Ok(Some(mut paths))) = receiver.await else {
                return;
            };
            let Some(path) = paths.pop() else { return };
            let _ = this.update_in(cx, |view, window, cx| view.open_path(&path, window, cx));
        })
        .detach();
    }

    /// The window title: the file, the folder, or the app name alone.
    pub(crate) fn update_title(&mut self, window: &mut Window, cx: &App) {
        let dirty = self
            .editor()
            .is_some_and(|editor| editor.read(cx).state.is_dirty());
        let name = self
            .file
            .as_deref()
            .or(self.target.as_ref().map(Target::path))
            .map(launch::display_name);
        let title = match name {
            Some(name) if dirty => format!("\u{25cf} {name} \u{b7} ted"),
            Some(name) => format!("{name} \u{b7} ted"),
            None => "ted".to_string(),
        };
        if title != self.last_title {
            window.set_window_title(&title);
            self.last_title = title;
        }
    }

    /// Loads `path` in the background and shows it. A big file shows its start first, then the
    /// whole text replaces it, keeping the cursor and the scroll.
    fn load(&mut self, path: PathBuf, window: &mut Window, cx: &mut Context<Self>) {
        self.load_generation += 1;
        let generation = self.load_generation;
        self.content = Content::Loading;
        cx.spawn_in(window, async move |this, cx| {
            let size = std::fs::metadata(&path).map(|meta| meta.len()).unwrap_or(0);
            if size >= HEAD_THRESHOLD {
                let head_path = path.clone();
                let head = cx
                    .background_spawn(async move { load_head(&head_path, HEAD_BYTES) })
                    .await;
                let _ = this.update_in(cx, |view, window, cx| {
                    view.show(generation, head, window, cx)
                });
            }
            let loaded = cx.background_spawn(async move { load_file(&path) }).await;
            let _ = this.update_in(cx, |view, window, cx| {
                view.show(generation, loaded, window, cx)
            });
        })
        .detach();
    }

    /// Shows a loaded file: a new editor, or the full text in the editor of its head.
    fn show(
        &mut self,
        generation: u64,
        loaded: Result<LoadedFile, ted_workspace::LoadError>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if generation != self.load_generation {
            return;
        }
        let loaded = match loaded {
            Ok(loaded) => loaded,
            Err(error) => {
                log::warn!("cannot open a file: {error}");
                self.content = Content::Message(
                    t!("open.unreadable").into_owned().into(),
                    Some(error.to_string().into()),
                );
                cx.notify();
                return;
            }
        };
        let file_size = loaded.size;
        let (path, modified) = (loaded.path.clone(), loaded.modified);
        let (buffer, bom, lossy) = match loaded.content {
            FileContent::Text { buffer, bom, lossy } => (buffer, bom, lossy),
            FileContent::Binary => {
                self.content = Content::Message(t!("editor.binary").into_owned().into(), None);
                cx.notify();
                return;
            }
        };
        match &self.content {
            Content::Editor(editor) => editor.update(cx, |editor, cx| {
                editor.state.buffer = buffer;
                editor.state.expected_lines = None;
                editor.state.read_only = lossy;
                editor.state.disk_modified = modified;
                cx.notify();
            }),
            _ => {
                // Only the start of the file: make room in the gutter for all its lines.
                let expected_lines = (file_size > buffer.len() as u64)
                    .then(|| estimate_lines(file_size, buffer.len(), buffer.line_count()));
                let mut state = EditorState::new(buffer);
                state.expected_lines = expected_lines;
                state.bom = bom;
                // Only the start of a big file: no editing until the whole text replaces it.
                state.read_only = lossy || expected_lines.is_some();
                state.path = Some(path);
                state.disk_modified = modified;
                let editor = cx.new(|cx| EditorView::new(state, cx));
                self.editor_subscription =
                    Some(cx.observe_in(&editor, window, |this, _, window, cx| {
                        this.update_title(window, cx)
                    }));
                let focus = editor.read(cx).focus_handle().clone();
                focus.focus(window, cx);
                self.content = Content::Editor(editor);
            }
        }
        cx.notify();
    }
}

impl AppView {
    /// The invisible strip on the explorer's right edge that resizes it.
    fn render_resize_handle(&self, cx: &mut Context<Self>) -> impl IntoElement {
        div()
            .id("explorer-resize")
            .flex_none()
            .h_full()
            .w(px(EXPLORER_RESIZE_HANDLE))
            .ml(px(-EXPLORER_RESIZE_HANDLE))
            .cursor_col_resize()
            .on_mouse_down(
                MouseButton::Left,
                cx.listener(|this, _, _, cx| {
                    this.resizing_explorer = true;
                    cx.stop_propagation();
                }),
            )
    }
}

impl Render for AppView {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        log::trace!("render AppView");
        let palette = cx.palette();
        let content = match &self.content {
            Content::Editor(editor) => div().flex_1().min_h_0().child(editor.clone()),
            Content::Welcome => div().flex_1().min_h_0().child(self.render_welcome(cx)),
            Content::Loading => div().flex_1(),
            Content::Folder => div().flex_1().child(
                EmptyState::new(t!("open.folder_placeholder"))
                    .icon(Icon::new(IconName::FolderOpen))
                    .hint(crate::actions::hints::OPEN_FILE),
            ),
            Content::Message(title, detail) => {
                let mut state = EmptyState::new(title.clone())
                    .icon(Icon::new(IconName::TriangleAlert))
                    .action(self.render_open_buttons(cx));
                if let Some(detail) = detail {
                    state = state.description(detail.clone());
                }
                div().flex_1().child(state)
            }
        };
        let content = match &self.explorer {
            Some((explorer, _)) => h_flex()
                .flex_1()
                .min_h_0()
                .when(self.explorer_visible, |row| {
                    row.child(
                        div()
                            .flex_none()
                            .h_full()
                            .w(px(self.explorer_width))
                            .border_r_1()
                            .border_color(palette.border)
                            .child(explorer.clone()),
                    )
                    .child(self.render_resize_handle(cx))
                })
                .child(content.min_w_0().h_full()),
            None => content,
        };
        v_flex()
            .id("app")
            .key_context("App")
            .track_focus(&self.focus_handle)
            .on_action(
                cx.listener(|this, _: &OpenFile, window, cx| this.prompt_open(false, window, cx)),
            )
            .on_action(
                cx.listener(|this, _: &OpenFolder, window, cx| this.prompt_open(true, window, cx)),
            )
            .on_action(cx.listener(|this, _: &Save, window, cx| {
                this.save(window, cx).detach();
            }))
            .on_action(cx.listener(|this, _: &ToggleExplorer, _, cx| {
                if this.explorer.is_some() {
                    this.explorer_visible = !this.explorer_visible;
                    cx.notify();
                }
            }))
            .on_mouse_move(cx.listener(|this, event: &MouseMoveEvent, _, cx| {
                if this.resizing_explorer {
                    let width = f32::from(event.position.x);
                    this.explorer_width = width.clamp(EXPLORER_WIDTH_MIN, EXPLORER_WIDTH_MAX);
                    cx.notify();
                }
            }))
            .on_mouse_up(
                MouseButton::Left,
                cx.listener(|this, _, _, _| this.resizing_explorer = false),
            )
            .size_full()
            .bg(palette.bg)
            .text_color(palette.fg)
            .child(self.render_title_bar(window, cx))
            .child(content)
            .child(self.render_status_bar(cx))
    }
}
