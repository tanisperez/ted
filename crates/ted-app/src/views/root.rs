//! The main window's view: the title bar, the content area and the status bar. The content is the
//! editor of the file given on the command line, loaded in the background; the explorer, the tabs
//! and the empty state come later (#6, #7, #8).

use std::path::PathBuf;

use gpui_kit::component::*;
use gpui_kit::prelude::*;
use gpui_kit::*;
use rust_i18n::t;
use ted_workspace::{FileContent, LoadedFile, load_file, load_head};

use crate::state::editor::{EditorState, estimate_lines};
use crate::state::settings::Settings;
use crate::theme::PaletteExt;

use super::editor::EditorView;

/// Files at least this big show their first [`HEAD_BYTES`] while the rest loads.
const HEAD_THRESHOLD: u64 = 8 * 1024 * 1024;
/// How much of a big file is read first: many screens, and still under a millisecond.
const HEAD_BYTES: usize = 256 * 1024;

/// What the content area shows.
enum Content {
    /// Nothing to show.
    Empty,
    /// The file is being read.
    Loading,
    /// The file could not be shown.
    Message(SharedString),
    Editor(Entity<EditorView>),
}

/// The root view of the main window.
pub struct AppView {
    /// The settings in effect, read once at startup.
    pub settings: Settings,
    content: Content,
}

impl AppView {
    pub fn new(
        settings: Settings,
        file: Option<PathBuf>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Self {
        let mut view = Self {
            settings,
            content: Content::Empty,
        };
        if let Some(path) = file {
            view.open_file(path, window, cx);
        }
        view
    }

    /// Loads `path` in the background and shows it. A big file shows its start first, then the
    /// whole text replaces it, keeping the cursor and the scroll.
    fn open_file(&mut self, path: PathBuf, window: &mut Window, cx: &mut Context<Self>) {
        self.content = Content::Loading;
        cx.spawn_in(window, async move |this, cx| {
            let size = std::fs::metadata(&path).map(|meta| meta.len()).unwrap_or(0);
            if size >= HEAD_THRESHOLD {
                let head_path = path.clone();
                let head = cx
                    .background_spawn(async move { load_head(&head_path, HEAD_BYTES) })
                    .await;
                let _ = this.update_in(cx, |view, window, cx| view.show(head, window, cx));
            }
            let loaded = cx.background_spawn(async move { load_file(&path) }).await;
            let _ = this.update_in(cx, |view, window, cx| view.show(loaded, window, cx));
        })
        .detach();
    }

    /// Shows a loaded file: a new editor, or the full text in the editor of its head.
    fn show(
        &mut self,
        loaded: Result<LoadedFile, ted_workspace::LoadError>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let loaded = match loaded {
            Ok(loaded) => loaded,
            Err(error) => {
                log::warn!("cannot open a file: {error}");
                self.content = Content::Message(error.to_string().into());
                cx.notify();
                return;
            }
        };
        let file_size = loaded.size;
        let buffer = match loaded.content {
            FileContent::Text { buffer, .. } => buffer,
            FileContent::Binary => {
                self.content = Content::Message(t!("editor.binary").into());
                cx.notify();
                return;
            }
        };
        match &self.content {
            Content::Editor(editor) => editor.update(cx, |editor, cx| {
                editor.state.buffer = buffer;
                editor.state.expected_lines = None;
                cx.notify();
            }),
            _ => {
                // Only the start of the file: make room in the gutter for all its lines.
                let expected_lines = (file_size > buffer.len() as u64)
                    .then(|| estimate_lines(file_size, buffer.len(), buffer.line_count()));
                let mut state = EditorState::new(buffer);
                state.expected_lines = expected_lines;
                let editor = cx.new(|cx| EditorView::new(state, cx));
                let focus = editor.read(cx).focus_handle().clone();
                focus.focus(window, cx);
                self.content = Content::Editor(editor);
            }
        }
        cx.notify();
    }
}

impl Render for AppView {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        log::trace!("render AppView");
        let palette = cx.palette();
        let content = match &self.content {
            Content::Editor(editor) => div().flex_1().min_h_0().child(editor.clone()),
            Content::Loading | Content::Empty => div().flex_1(),
            Content::Message(message) => div()
                .flex_1()
                .flex()
                .items_center()
                .justify_center()
                .text_color(palette.fg_muted)
                .child(message.clone()),
        };
        v_flex()
            .size_full()
            .bg(palette.bg)
            .text_color(palette.fg)
            .child(self.render_title_bar(window, cx))
            .child(content)
            .child(self.render_status_bar(cx))
    }
}
