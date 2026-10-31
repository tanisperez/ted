//! The main window's view: the title bar, the content area and the status bar.
//!
//! What it shows depends on what was opened (`state::launch`): nothing (the empty state with the
//! recents, `views/welcome.rs`), one file alone (file mode, the editor fills the window) or a
//! folder (folder mode, with the explorer and one tab per open file). Files load in the
//! background into their document; a load whose document was closed meanwhile is dropped.

use std::cell::RefCell;
use std::path::{Path, PathBuf};
use std::rc::Rc;

use gpui_kit::assets::IconName;
use gpui_kit::component::input::{InputEvent, InputState, TextareaState};
use gpui_kit::component::notification::Notification;
use gpui_kit::component::*;
use gpui_kit::prelude::*;
use gpui_kit::*;
use rust_i18n::t;
use ted_core::Buffer;
use ted_syntax::Language;
use ted_workspace::{FileContent, LoadedFile, load_file, load_head};
use telaio_ui::components::EmptyState;

use crate::actions::{
    CloseTab, NewFile, NextTab, OpenFile, OpenFolder, OpenSettings, PreviousTab, Save,
    ToggleExplorer, ToggleGit,
};
use crate::state::editor::{EditorState, estimate_lines};
use crate::state::git::GitState;
use crate::state::launch::{self, LaunchError, Target};
use crate::state::recents::{self, Recents};
use crate::state::settings::{Settings, SettingsCategory};
use crate::state::syntax::Syntax;
use crate::state::tabs::Tabs;
use crate::theme::PaletteExt;
use crate::theme::metrics::{
    EXPLORER_RESIZE_HANDLE, EXPLORER_WIDTH, EXPLORER_WIDTH_MAX, EXPLORER_WIDTH_MIN,
};

use super::editor::EditorView;
use super::explorer::{ExplorerEvent, ExplorerView};
use super::git::{CheckoutChoice, GitTasks, MenuContext, new_branch_menu};

/// Files at least this big show their first [`HEAD_BYTES`] while the rest loads.
const HEAD_THRESHOLD: u64 = 8 * 1024 * 1024;
/// How much of a big file is read first: many screens, and still under a millisecond.
const HEAD_BYTES: usize = 256 * 1024;
/// The longest first line read for a shebang (`#!/usr/bin/env python3`).
const SHEBANG_MAX: usize = 256;

/// What the content area shows.
enum Content {
    /// Nothing opened: the empty state with the recents.
    Welcome,
    /// Something could not be opened: a title and a detail.
    Message(SharedString, Option<SharedString>),
    /// The open documents: the one file of file mode, or the tabs of folder mode (with none yet,
    /// a placeholder).
    Documents,
}

/// What the panel at the left of folder mode shows, switched from the status bar.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub(crate) enum SidePanel {
    /// The explorer.
    #[default]
    Files,
    /// The Git panel.
    Git,
}

/// What an open document shows.
pub(crate) enum DocumentView {
    /// Its file is being read.
    Loading,
    /// It could not be shown: a title and a detail.
    Message(SharedString, Option<SharedString>),
    Editor(Entity<EditorView>),
}

/// One open file: a tab in folder mode, the whole window in file mode.
pub(crate) struct Document {
    /// Tells the loads apart: a load whose document was closed meanwhile is dropped.
    pub(crate) id: u64,
    /// `None` for a new document, kept in memory until it is saved.
    pub(crate) path: Option<PathBuf>,
    /// The number in the name of a new document ("Untitled 2"), from 1. Unused with a path.
    pub(crate) untitled: usize,
    pub(crate) view: DocumentView,
    /// Keeps the window title in step with the editor (its dirty state).
    _subscription: Option<Subscription>,
}

impl Document {
    /// The name of its tab and of the window title: the file name, or "Untitled".
    pub(crate) fn name(&self) -> String {
        match (&self.path, self.untitled) {
            (Some(path), _) => launch::display_name(path),
            (None, 1) => t!("open.untitled").into_owned(),
            (None, number) => t!("open.untitled_numbered", number = number).into_owned(),
        }
    }

    pub(crate) fn editor(&self) -> Option<&Entity<EditorView>> {
        match &self.view {
            DocumentView::Editor(editor) => Some(editor),
            _ => None,
        }
    }
}

/// The root view of the main window.
pub struct AppView {
    /// The settings in effect, read once at startup.
    pub settings: Settings,
    /// The recently opened folders and files.
    pub(crate) recents: Recents,
    /// What is open: `None` in the empty state.
    pub(crate) target: Option<Target>,
    content: Content,
    /// The open files, in tab order.
    pub(crate) documents: Tabs<Document>,
    /// The id of the next document opened.
    next_document: u64,
    focus_handle: FocusHandle,
    /// The explorer of folder mode, with the subscription to its events.
    pub(crate) explorer: Option<(Entity<ExplorerView>, Subscription)>,
    /// Whether the side panel (the explorer or the Git panel) is shown.
    pub(crate) explorer_visible: bool,
    /// Which panel the side panel shows.
    pub(crate) side_panel: SidePanel,
    /// The scroll of the Git panel's rows.
    pub(crate) git_scroll: UniformListScrollHandle,
    /// The explorer panel's width, changed by dragging its edge.
    explorer_width: f32,
    /// While the explorer's edge is dragged.
    resizing_explorer: bool,
    /// The window title last set, so it is only set again when it changes.
    last_title: String,
    /// The user chose to close the window without saving: it may close now.
    pub(crate) closing: bool,
    /// The category the Settings modal shows.
    pub(crate) settings_category: SettingsCategory,
    /// The exclude patterns field of the Settings modal, made when it opens.
    pub(crate) exclude_input: Option<Entity<InputState>>,
    /// The git executable select of the Settings modal, made when it opens.
    pub(crate) git_select: Option<(telaio_ui::components::SelectHandle, Subscription)>,
    /// The scroll of the empty state, for its scrollbar.
    pub(crate) welcome_scroll: ScrollHandle,
    /// Git: availability, the folder's repository and its status, read by render.
    pub(crate) git: GitState,
    /// The git work in flight.
    pub(crate) git_tasks: GitTasks,
    /// The Git panel's commit message.
    pub(crate) commit_input: Entity<TextareaState>,
    /// The branch menu of the status bar.
    pub(crate) branch_menu: Entity<telaio_ui::components::SearchableMenuState>,
    /// What the branch menu's submenus and create row need to know, updated on every rebuild.
    pub(crate) branch_menu_context: Rc<RefCell<MenuContext>>,
    /// The choice of the checkout dialog while it is open.
    pub(crate) checkout_choice: CheckoutChoice,
    /// Checks the open file on disk when the window gets the focus back.
    _activation: Subscription,
}

impl AppView {
    pub fn new(
        settings: Settings,
        paths: Vec<PathBuf>,
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
        let commit_input = cx.new(|cx| {
            TextareaState::new(window, cx)
                .placeholder(t!("git.commit.placeholder"))
                .auto_grow(3, 10)
        });
        // The commit buttons are enabled by the message, so its edits repaint the panel.
        cx.subscribe(&commit_input, |_, _, event: &InputEvent, cx| {
            if matches!(event, InputEvent::Change) {
                cx.notify();
            }
        })
        .detach();
        let branch_menu_context = Rc::new(RefCell::new(MenuContext::default()));
        let branch_menu = new_branch_menu(Rc::clone(&branch_menu_context), window, cx);
        let mut view = Self {
            settings,
            recents: recents::load(),
            target: None,
            content: Content::Welcome,
            documents: Tabs::default(),
            next_document: 0,
            focus_handle,
            explorer: None,
            explorer_visible: true,
            side_panel: SidePanel::default(),
            git_scroll: UniformListScrollHandle::new(),
            explorer_width: EXPLORER_WIDTH,
            resizing_explorer: false,
            last_title: String::new(),
            closing: false,
            settings_category: SettingsCategory::default(),
            exclude_input: None,
            git_select: None,
            welcome_scroll: ScrollHandle::new(),
            git: GitState::default(),
            git_tasks: GitTasks::default(),
            commit_input,
            branch_menu,
            branch_menu_context,
            checkout_choice: CheckoutChoice::default(),
            _activation: activation,
        };
        view.detect_git(cx);
        view.update_title(window, cx);
        view.open_paths(&paths, window, cx);
        view.apply_debug_open(window, cx);
        view
    }

    /// `TED_OPEN=settings` or `settings-about` opens the Settings modal at startup, to check or
    /// screenshot it. Not a feature: unknown values are ignored.
    fn apply_debug_open(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let category = match std::env::var("TED_OPEN").as_deref() {
            Ok("settings") => SettingsCategory::Appearance,
            Ok("settings-about") => SettingsCategory::About,
            _ => return,
        };
        self.settings_category = category;
        // A dialog needs the window's `Root`, which does not exist yet while this view is being
        // built: wait one tick, as Postino does.
        let weak = cx.weak_entity();
        window.defer(cx, move |window, cx| {
            let _ = weak.update(cx, |view, cx| view.open_settings(window, cx));
        });
    }

    /// Shows `panel` at the left, or hides the side panel when it already shows it. Only in
    /// folder mode.
    pub(crate) fn toggle_side_panel(&mut self, panel: SidePanel, cx: &mut Context<Self>) {
        if self.explorer.is_none() {
            return;
        }
        if self.explorer_visible && self.side_panel == panel {
            self.explorer_visible = false;
        } else {
            self.explorer_visible = true;
            self.side_panel = panel;
            if panel == SidePanel::Git {
                self.refresh_git_status(cx);
            }
        }
        cx.notify();
    }

    /// The editor shown, if any.
    pub(crate) fn editor(&self) -> Option<&Entity<EditorView>> {
        match self.content {
            Content::Documents => self.documents.active().and_then(Document::editor),
            _ => None,
        }
    }

    /// The editors of every open document, for the questions about unsaved changes.
    pub(crate) fn editors(&self) -> Vec<Entity<EditorView>> {
        self.documents
            .items()
            .iter()
            .filter_map(|document| document.editor().cloned())
            .collect()
    }

    /// Whether the tab bar shows: always in folder mode with a file open, and in file mode once
    /// more than one file is open (`ted a.txt b.txt`).
    pub(crate) fn shows_tabs(&self) -> bool {
        match self.explorer {
            Some(_) => !self.documents.is_empty(),
            None => self.documents.items().len() > 1,
        }
    }

    /// Opens what the command line (or Finder) names, in order: the first folder with the
    /// explorer, then every file in its own tab. Without a folder the files open alone, in tabs.
    /// More folders than one are not opened.
    pub(crate) fn open_paths(
        &mut self,
        paths: &[PathBuf],
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let folder = paths.iter().position(|path| path.is_dir());
        if let Some(index) = folder {
            self.open_path(&paths[index], window, cx);
        }
        for (index, path) in paths.iter().enumerate() {
            if Some(index) == folder {
                continue;
            }
            if path.is_dir() {
                log::warn!("only the first folder given is opened");
                continue;
            }
            match (&self.target, launch::resolve(path)) {
                // A file after the first one joins the open ones as a tab.
                (Some(_), Ok(Target::File(file))) => {
                    self.recents.add(&Target::File(file.clone()));
                    recents::save(&self.recents);
                    self.open_document(file, window, cx);
                }
                _ => self.open_path(path, window, cx),
            }
        }
    }

    /// Opens a file or a folder, from the command line, a dialog, the explorer or the recents. A
    /// file opened in folder mode gets a tab; anything else replaces what is open, asking first
    /// when that has unsaved changes.
    pub(crate) fn open_path(&mut self, path: &Path, window: &mut Window, cx: &mut Context<Self>) {
        match launch::resolve(path) {
            Ok(Target::File(file)) if matches!(self.target, Some(Target::Folder(_))) => {
                log::info!("opening a file");
                self.recents.add(&Target::File(file.clone()));
                recents::save(&self.recents);
                self.open_document(file, window, cx);
            }
            Ok(target) => {
                let editors = self.editors();
                self.confirm_discard(editors, window, cx, move |this, window, cx| {
                    this.open_target(target, window, cx)
                });
            }
            Err(error) => {
                let (title, path) = match &error {
                    LaunchError::NotFound(path) => (t!("open.not_found"), path),
                    LaunchError::Unreadable(path, _) => (t!("open.unreadable"), path),
                };
                log::warn!("cannot open a path: {error:?}");
                self.recents.remove(path);
                recents::save(&self.recents);
                let detail = path.display().to_string();
                if self.documents.is_empty() {
                    self.content = Content::Message(title.into_owned().into(), Some(detail.into()));
                } else {
                    // Keep the open documents: say it in a notification instead.
                    window.push_notification(
                        Notification::warning(detail).title(title.into_owned()),
                        cx,
                    );
                }
                self.update_title(window, cx);
                cx.notify();
            }
        }
    }

    /// Opens a file alone (file mode) or a folder, replacing what was open.
    fn open_target(&mut self, target: Target, window: &mut Window, cx: &mut Context<Self>) {
        self.recents.add(&target);
        recents::save(&self.recents);
        self.documents.clear();
        self.content = Content::Documents;
        match target {
            Target::File(file) => {
                log::info!("opening a file");
                self.explorer = None;
                self.target = Some(Target::File(file.clone()));
                self.discover_git_repo(cx);
                self.open_document(file, window, cx);
            }
            Target::Folder(root) => {
                log::info!("opening a folder");
                let excludes = self.settings.explorer_exclude.clone();
                let explorer = cx.new(|cx| ExplorerView::new(root.clone(), &excludes, cx));
                let subscription = cx.subscribe_in(
                    &explorer,
                    window,
                    |this, _, event: &ExplorerEvent, window, cx| match event {
                        ExplorerEvent::Open(path) => this.open_path(path, window, cx),
                    },
                );
                self.explorer = Some((explorer, subscription));
                self.explorer_visible = true;
                self.side_panel = SidePanel::Files;
                self.target = Some(Target::Folder(root));
                self.discover_git_repo(cx);
                self.update_title(window, cx);
                cx.notify();
            }
        }
    }

    /// Shows `path`: its tab when it is already open, otherwise a new one that loads it.
    fn open_document(&mut self, path: PathBuf, window: &mut Window, cx: &mut Context<Self>) {
        self.content = Content::Documents;
        if let Some(index) = self
            .documents
            .position(|document| document.path.as_deref() == Some(path.as_path()))
        {
            self.activate_document(index, window, cx);
            return;
        }
        let id = self.next_document;
        self.next_document += 1;
        self.documents.open(Document {
            id,
            path: Some(path.clone()),
            untitled: 0,
            view: DocumentView::Loading,
            _subscription: None,
        });
        self.load(id, path, window, cx);
        self.document_shown(window, cx);
    }

    /// Creates a new untitled document (`Ctrl+N`): a new tab in folder mode, otherwise the editor
    /// alone, replacing what is open (asking first when that has unsaved changes).
    pub(crate) fn new_document(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.explorer.is_some() {
            return self.open_untitled(window, cx);
        }
        let editors = self.editors();
        self.confirm_discard(editors, window, cx, |this, window, cx| {
            this.documents.clear();
            this.target = None;
            this.open_untitled(window, cx);
        });
    }

    /// Opens an empty document with no file, named with the lowest number no other one uses.
    fn open_untitled(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        log::info!("creating a new document");
        let mut untitled = 1;
        while self
            .documents
            .position(|document| document.path.is_none() && document.untitled == untitled)
            .is_some()
        {
            untitled += 1;
        }
        let mut state = EditorState::new(Buffer::default());
        state.syntax = Syntax::new(Language::PlainText, self.settings.highlight_max_bytes());
        let editor = cx.new(|cx| EditorView::new(state, cx));
        let subscription = cx.observe_in(&editor, window, |this, _, window, cx| {
            this.update_title(window, cx)
        });
        let id = self.next_document;
        self.next_document += 1;
        self.content = Content::Documents;
        self.documents.open(Document {
            id,
            path: None,
            untitled,
            view: DocumentView::Editor(editor),
            _subscription: Some(subscription),
        });
        self.document_shown(window, cx);
    }

    /// After a new document was saved as `path`: it becomes a normal file, with its name, its
    /// language and its place in the recents.
    pub(crate) fn saved_as(
        &mut self,
        editor: &Entity<EditorView>,
        path: PathBuf,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let max_bytes = self.settings.highlight_max_bytes();
        editor.update(cx, |editor, cx| {
            let state = &mut editor.state;
            state.syntax = Syntax::new(detect_language(&path, &state.buffer), max_bytes);
            state.path = Some(path.clone());
            cx.notify();
        });
        if let Some(index) = self
            .documents
            .position(|document| document.editor() == Some(editor))
            && let Some(document) = self.documents.get_mut(index)
        {
            document.path = Some(path.clone());
        }
        self.recents.add(&Target::File(path));
        recents::save(&self.recents);
        self.document_shown(window, cx);
    }

    /// Makes the tab at `index` the one shown.
    pub(crate) fn activate_document(
        &mut self,
        index: usize,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if self.documents.activate(index) {
            self.document_shown(window, cx);
            self.check_disk(window, cx);
        }
    }

    /// After another document became the active one: focus its editor, reveal it in the
    /// explorer and retitle the window.
    pub(crate) fn document_shown(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        // Without a folder, the title bar's switcher names the file shown.
        if self.explorer.is_none()
            && let Some(document) = self.documents.active()
        {
            self.target = document.path.clone().map(Target::File);
        }
        match self.documents.active() {
            Some(document) => {
                if let Some(editor) = document.editor() {
                    let focus = editor.read(cx).focus_handle().clone();
                    focus.focus(window, cx);
                }
                if let (Some((explorer, _)), Some(path)) = (&self.explorer, &document.path) {
                    let path = path.clone();
                    explorer.update(cx, |explorer, cx| explorer.reveal(&path, cx));
                }
            }
            None => self.focus_handle.focus(window, cx),
        }
        self.update_title(window, cx);
        cx.notify();
    }

    /// Closes the document with `id`, asking first when it has unsaved changes.
    pub(crate) fn close_document(&mut self, id: u64, window: &mut Window, cx: &mut Context<Self>) {
        let Some(index) = self.documents.position(|document| document.id == id) else {
            return;
        };
        let editors = self.documents.items()[index].editor().cloned();
        self.confirm_discard(
            editors.into_iter().collect(),
            window,
            cx,
            move |this, window, cx| {
                let Some(index) = this.documents.position(|document| document.id == id) else {
                    return;
                };
                let was_active = this.documents.active_index() == Some(index);
                this.documents.close(index);
                if was_active {
                    this.document_shown(window, cx);
                } else {
                    cx.notify();
                }
            },
        );
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
            .is_some_and(|editor| editor.read(cx).state.has_unsaved());
        let name = self.documents.active().map(Document::name).or(self
            .target
            .as_ref()
            .map(|target| launch::display_name(target.path())));
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

    /// Loads `path` in the background into the document `id`. A big file shows its start first,
    /// then the whole text replaces it, keeping the cursor and the scroll.
    fn load(&mut self, id: u64, path: PathBuf, window: &mut Window, cx: &mut Context<Self>) {
        cx.spawn_in(window, async move |this, cx| {
            let size = std::fs::metadata(&path).map(|meta| meta.len()).unwrap_or(0);
            if size >= HEAD_THRESHOLD {
                let head_path = path.clone();
                let head = cx
                    .background_spawn(async move { load_head(&head_path, HEAD_BYTES) })
                    .await;
                let _ = this.update_in(cx, |view, window, cx| view.show(id, head, window, cx));
            }
            let loaded = cx.background_spawn(async move { load_file(&path) }).await;
            let _ = this.update_in(cx, |view, window, cx| view.show(id, loaded, window, cx));
        })
        .detach();
    }

    /// Shows a loaded file in its document: a new editor, or the full text in the editor of its
    /// head. Dropped when the document was closed meanwhile.
    fn show(
        &mut self,
        id: u64,
        loaded: Result<LoadedFile, ted_workspace::LoadError>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let Some(index) = self.documents.position(|document| document.id == id) else {
            return;
        };
        let active = self.documents.active_index() == Some(index);
        let Some(document) = self.documents.get_mut(index) else {
            return;
        };
        let loaded = match loaded {
            Ok(loaded) => loaded,
            Err(error) => {
                log::warn!("cannot open a file: {error}");
                document.view = DocumentView::Message(
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
                document.view =
                    DocumentView::Message(t!("editor.binary").into_owned().into(), None);
                cx.notify();
                return;
            }
        };
        match &document.view {
            DocumentView::Editor(editor) => editor.update(cx, |editor, cx| {
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
                // A file over the limit is never parsed, not even the start shown first.
                let max_bytes = self.settings.highlight_max_bytes();
                let max_bytes = if file_size > max_bytes as u64 {
                    0
                } else {
                    max_bytes
                };
                state.syntax = Syntax::new(detect_language(&path, &state.buffer), max_bytes);
                state.path = Some(path);
                state.disk_modified = modified;
                let editor = cx.new(|cx| EditorView::new(state, cx));
                document._subscription =
                    Some(cx.observe_in(&editor, window, |this, _, window, cx| {
                        this.update_title(window, cx)
                    }));
                if active {
                    let focus = editor.read(cx).focus_handle().clone();
                    focus.focus(window, cx);
                }
                document.view = DocumentView::Editor(editor);
            }
        }
        cx.notify();
    }
}

/// The language of a file: from its name, or for a script without an extension, from the
/// shebang in its first line.
fn detect_language(path: &Path, buffer: &Buffer) -> Language {
    match Language::detect(path) {
        Language::PlainText if buffer.line_len(0) <= SHEBANG_MAX => {
            let first_line = buffer.line(0).to_string();
            Language::from_shebang(&first_line).unwrap_or(Language::PlainText)
        }
        language => language,
    }
}

impl AppView {
    /// Something that cannot be shown: its title and detail, and the Open buttons.
    fn render_message(
        &self,
        title: &SharedString,
        detail: Option<&SharedString>,
        cx: &mut Context<Self>,
    ) -> Div {
        let mut state = EmptyState::new(title.clone())
            .icon(Icon::new(IconName::TriangleAlert))
            .action(self.render_open_buttons(cx));
        if let Some(detail) = detail {
            state = state.description(detail.clone());
        }
        div().flex_1().child(state)
    }

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
            Content::Welcome => div().flex_1().min_h_0().child(self.render_welcome(cx)),
            Content::Message(title, detail) => self.render_message(title, detail.as_ref(), cx),
            Content::Documents => {
                let document = match self.documents.active().map(|document| &document.view) {
                    Some(DocumentView::Editor(editor)) => {
                        div().flex_1().min_h_0().child(editor.clone())
                    }
                    Some(DocumentView::Loading) => div().flex_1(),
                    Some(DocumentView::Message(title, detail)) => {
                        self.render_message(title, detail.as_ref(), cx)
                    }
                    None => div().flex_1().child(
                        EmptyState::new(t!("open.folder_placeholder"))
                            .icon(Icon::new(IconName::FolderOpen))
                            .hint(crate::actions::hints::OPEN_FILE),
                    ),
                };
                match self.shows_tabs() {
                    true => v_flex()
                        .flex_1()
                        .min_h_0()
                        .child(self.render_tabs(cx))
                        .child(document),
                    false => document,
                }
            }
        };
        let explorer = self.explorer.as_ref().map(|(explorer, _)| explorer.clone());
        let content = match explorer {
            Some(explorer) => h_flex()
                .flex_1()
                .min_h_0()
                .when(self.explorer_visible, |row| {
                    let panel = match self.side_panel {
                        SidePanel::Files => explorer.into_any_element(),
                        SidePanel::Git => self.render_git_panel(cx),
                    };
                    row.child(
                        div()
                            .flex_none()
                            .h_full()
                            .w(px(self.explorer_width))
                            .border_r_1()
                            .border_color(palette.border)
                            .child(panel),
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
            .on_action(cx.listener(|this, _: &NewFile, window, cx| this.new_document(window, cx)))
            .on_action(
                cx.listener(|this, _: &OpenFile, window, cx| this.prompt_open(false, window, cx)),
            )
            .on_action(
                cx.listener(|this, _: &OpenFolder, window, cx| this.prompt_open(true, window, cx)),
            )
            .on_action(cx.listener(|this, _: &Save, window, cx| {
                this.save(window, cx).detach();
            }))
            .on_action(
                cx.listener(|this, _: &OpenSettings, window, cx| this.open_settings(window, cx)),
            )
            .on_action(cx.listener(|this, _: &CloseTab, window, cx| {
                if this.shows_tabs()
                    && let Some(document) = this.documents.active()
                {
                    this.close_document(document.id, window, cx);
                }
            }))
            .on_action(cx.listener(|this, _: &NextTab, window, cx| {
                if this.documents.cycle(true) {
                    this.document_shown(window, cx);
                }
            }))
            .on_action(cx.listener(|this, _: &PreviousTab, window, cx| {
                if this.documents.cycle(false) {
                    this.document_shown(window, cx);
                }
            }))
            .on_action(cx.listener(|this, _: &ToggleExplorer, _, cx| {
                this.toggle_side_panel(SidePanel::Files, cx)
            }))
            .on_action(
                cx.listener(|this, _: &ToggleGit, _, cx| {
                    this.toggle_side_panel(SidePanel::Git, cx)
                }),
            )
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
