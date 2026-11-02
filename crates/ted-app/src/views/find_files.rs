//! The Find in files window (`Ctrl+Shift+F`) and Replace in files (`Ctrl+Shift+R`), from the
//! 0.5.0 design: a floating window over the editor, without dimming it, that can be dragged by its
//! header and pinned. At the top the title, the summary, the file mask and the pin; then the
//! query with its toggles (match case, whole words, regular expression), the replacement with
//! preserve case, and the scope (the folder, a directory or the open files). Below, the matches
//! line by line, and the preview of the selected one with the lines around it. Replace shows each
//! change before it is made: the old text struck through and the new one in green.
//!
//! Enter opens the selected match in its file and closes the window (unless pinned); `Ctrl+Enter`
//! leaves the results in a tab (`views/find_results.rs`); the arrows move through the results
//! while the query has the focus; Escape closes it. A click outside closes it too, unless pinned.
//!
//! The search runs in the background, a batch of files at a time, a short while after the last
//! key. The files open in tabs are searched in their buffers, unsaved changes included. The
//! state and the counting live in `state::find_files`; reading, masks and replacing on disk in
//! `ted_workspace::find_files`. The app opens the results and does the replacing
//! ([`FindFilesEvent`]).

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::{Duration, Instant};

use gpui_kit::assets::IconName;
use gpui_kit::component::input::{InputEvent, InputState};
use gpui_kit::component::tooltip::Tooltip;
use gpui_kit::component::*;
use gpui_kit::prelude::*;
use gpui_kit::*;
use rust_i18n::t;
use ted_core::{Buffer, SearchQuery};
use ted_workspace::{ExplorerFilter, FileMask, find_lines, index_files, read_text};
use telaio_ui::components::{CheckState, PrimaryButton, SecondaryButton, TriCheckbox};

use crate::state::find_files::{FileResult, FindFiles, MAX_MATCHES, Scope, row_parts};
use crate::theme::PaletteExt;
use crate::theme::metrics::{
    FIND_FILES_LIST_HEIGHT, FIND_FILES_ROW_HEIGHT, FIND_FILES_TOP, FIND_FILES_WIDTH, RADIUS_DIALOG,
};

use super::field::Field;
use super::find::{ToggleCase, ToggleRegex, ToggleWholeWord, toggle};

actions!(
    find_files,
    [
        /// Opens Find in files (`Ctrl+Shift+F`).
        OpenFindInFiles,
        /// Opens Replace in files (`Ctrl+Shift+R`).
        OpenReplaceInFiles,
        /// Closes the window (Escape in it).
        CloseFindFiles,
        /// Selects the next result (Down in the query).
        NextResult,
        /// Selects the previous result (Up in the query).
        PreviousResult,
    ]
);

/// The key context of the window.
const CONTEXT: &str = "FindFiles";
/// How long after the last key the search starts.
const DEBOUNCE: Duration = Duration::from_millis(120);
/// Files searched by one background batch, at most, before the results go to the window.
const BATCH_FILES: usize = 64;
/// Time one batch may take before it hands its results over.
const BATCH_TIME: Duration = Duration::from_millis(30);
/// The most files of a folder listed for the search.
const MAX_FILES: usize = 200_000;

/// Registers the window's key bindings.
pub fn bind_keys(cx: &mut App) {
    #[cfg(target_os = "macos")]
    let (find, replace) = ("cmd-shift-f", "cmd-shift-r");
    #[cfg(not(target_os = "macos"))]
    let (find, replace) = ("ctrl-shift-f", "ctrl-shift-r");
    cx.bind_keys([
        KeyBinding::new(find, OpenFindInFiles, None),
        KeyBinding::new(replace, OpenReplaceInFiles, None),
        KeyBinding::new("alt-c", ToggleCase, Some(CONTEXT)),
        KeyBinding::new("alt-w", ToggleWholeWord, Some(CONTEXT)),
        KeyBinding::new("alt-x", ToggleRegex, Some(CONTEXT)),
        // Anywhere in the window, also after a click on a result.
        KeyBinding::new("escape", CloseFindFiles, Some(CONTEXT)),
    ]);
    // After gpui-kit's own keys of the field, so these win inside the window.
    cx.bind_keys([
        KeyBinding::new("escape", CloseFindFiles, Some("FindFiles > Input")),
        KeyBinding::new("down", NextResult, Some("FindFiles > Input")),
        KeyBinding::new("up", PreviousResult, Some("FindFiles > Input")),
    ]);
}

/// What the window asks of the app.
pub enum FindFilesEvent {
    /// Show `offset` (bytes) of the file at `path`, selected.
    Open {
        path: PathBuf,
        offset: std::ops::Range<usize>,
    },
    /// Put these results in a tab, titled with the query.
    OpenTab {
        query: String,
        files: Vec<FileResult>,
    },
    /// Replace, in each file, the matches that start at the given offsets.
    Replace {
        query: SearchQuery,
        with: String,
        preserve_case: bool,
        files: Vec<(PathBuf, Vec<usize>)>,
        /// Ask first (Replace all).
        confirm: bool,
    },
    /// The window closed.
    Close,
}

impl EventEmitter<FindFilesEvent> for FindFilesView {}

/// The window.
pub struct FindFilesView {
    pub(crate) state: FindFiles,
    /// The open folder, `None` in file mode (then only the open files are searched).
    root: Option<PathBuf>,
    excludes: Vec<String>,
    /// The documents open in tabs: their buffers are searched instead of the files on disk.
    open_buffers: Arc<HashMap<PathBuf, Buffer>>,
    query_input: Entity<InputState>,
    replace_input: Entity<InputState>,
    mask_input: Entity<InputState>,
    list_scroll: UniformListScrollHandle,
    focus_handle: FocusHandle,
    /// How far the window was dragged from its place.
    offset: Point<Pixels>,
    /// While the header is dragged: the pointer's distance from the offset.
    grab: Option<Point<Pixels>>,
    _subscriptions: Vec<Subscription>,
}

impl FindFilesView {
    pub fn new(window: &mut Window, cx: &mut Context<Self>) -> Self {
        let query_input =
            cx.new(|cx| InputState::new(window, cx).placeholder(t!("find.placeholder")));
        let replace_input =
            cx.new(|cx| InputState::new(window, cx).placeholder(t!("find.replace_placeholder")));
        let mask_input =
            cx.new(|cx| InputState::new(window, cx).placeholder(t!("find_files.mask")));
        let subscriptions = vec![
            cx.subscribe_in(
                &query_input,
                window,
                |this, input, event, window, cx| match event {
                    InputEvent::Change => {
                        this.state.text = input.read(cx).value().to_string();
                        this.search(cx);
                    }
                    InputEvent::PressEnter { secondary, .. } => match secondary {
                        true => this.open_tab(cx),
                        false => this.open_selected(window, cx),
                    },
                    _ => {}
                },
            ),
            cx.subscribe_in(
                &replace_input,
                window,
                |this, _, event, _, cx| match event {
                    InputEvent::Change => cx.notify(),
                    InputEvent::PressEnter { .. } => this.replace_selected(cx),
                    _ => {}
                },
            ),
            cx.subscribe_in(&mask_input, window, |this, input, event, _, cx| {
                if let InputEvent::Change = event {
                    this.state.mask = input.read(cx).value().to_string();
                    this.search(cx);
                }
            }),
        ];
        Self {
            state: FindFiles::default(),
            root: None,
            excludes: Vec::new(),
            open_buffers: Arc::new(HashMap::new()),
            query_input,
            replace_input,
            mask_input,
            list_scroll: UniformListScrollHandle::new(),
            focus_handle: cx.focus_handle(),
            offset: Point::default(),
            grab: None,
            _subscriptions: subscriptions,
        }
    }

    /// Shows the window for `root` (the open folder), with `replace` or not, the selected text
    /// in the query when there is one, and the focus in the query. Searches again.
    #[allow(clippy::too_many_arguments)]
    pub fn show(
        &mut self,
        root: Option<PathBuf>,
        excludes: Vec<String>,
        open_buffers: HashMap<PathBuf, Buffer>,
        replace: bool,
        selected: Option<String>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if self.root != root {
            self.state.scope = Scope::default();
        }
        if root.is_none() {
            self.state.scope = Scope::OpenFiles;
        }
        self.root = root;
        self.excludes = excludes;
        self.open_buffers = Arc::new(open_buffers);
        self.state.replace = replace;
        self.query_input.update(cx, |input, cx| {
            if let Some(text) = selected {
                input.set_value(text, window, cx);
            }
            input.focus(window, cx);
            input.select_all(window, cx);
        });
        self.state.text = self.query_input.read(cx).value().to_string();
        self.search(cx);
    }

    /// The buffers of the open documents changed (a replace): searches again with them.
    pub fn refresh(&mut self, open_buffers: HashMap<PathBuf, Buffer>, cx: &mut Context<Self>) {
        self.open_buffers = Arc::new(open_buffers);
        self.search(cx);
    }

    /// Starts the search over, after [`DEBOUNCE`].
    fn search(&mut self, cx: &mut Context<Self>) {
        let (generation, compiled) = self.state.restart();
        cx.notify();
        let Some((query, mask)) = compiled else {
            return;
        };
        let scope = self.state.scope.clone();
        let root = self.root.clone();
        let excludes = self.excludes.clone();
        let open = Arc::clone(&self.open_buffers);
        cx.spawn(async move |this, cx| {
            cx.background_executor().timer(DEBOUNCE).await;
            let stale = |this: &WeakEntity<Self>, cx: &mut AsyncApp| {
                !matches!(
                    this.read_with(cx, |view, _| view.state.generation == generation),
                    Ok(true)
                )
            };
            if stale(&this, cx) {
                return;
            }
            let started = Instant::now();
            let list_open = Arc::clone(&open);
            let files = cx
                .background_spawn(async move {
                    list_files(&scope, root.as_deref(), &excludes, &list_open)
                })
                .await;
            let total = files.len();
            let files = Arc::new(files);
            let mut next = 0;
            let mut room = MAX_MATCHES;
            while next < total && room > 0 {
                if stale(&this, cx) {
                    return;
                }
                let (batch_files, batch_open) = (Arc::clone(&files), Arc::clone(&open));
                let (batch_query, batch_mask) = (query.clone(), mask.clone());
                let from = next;
                let (results, end) = cx
                    .background_spawn(async move {
                        search_batch(
                            &batch_query,
                            &batch_mask,
                            &batch_files,
                            from,
                            &batch_open,
                            room,
                        )
                    })
                    .await;
                next = end;
                let added = this.update(cx, |view, cx| {
                    let room = view.state.add(generation, results);
                    cx.notify();
                    room
                });
                match added {
                    Ok(left) => room = left,
                    Err(_) => return,
                }
            }
            let _ = this.update(cx, |view, cx| {
                view.state.finish(generation);
                cx.notify();
            });
            log::debug!(
                "searched {next} of {total} files in {:?}",
                started.elapsed()
            );
        })
        .detach();
    }

    fn select_by(&mut self, delta: isize, cx: &mut Context<Self>) {
        self.state.select_by(delta);
        if let Some(row) = self.state.selected {
            self.list_scroll
                .scroll_to_item(row, ScrollStrategy::Nearest);
        }
        cx.notify();
    }

    /// Enter: opens the selected match; the window closes unless pinned.
    fn open_selected(&mut self, _window: &mut Window, cx: &mut Context<Self>) {
        let Some((file, matched)) = self.state.current() else {
            return;
        };
        cx.emit(FindFilesEvent::Open {
            path: file.path.clone(),
            offset: matched.offset.clone(),
        });
        if !self.state.pinned {
            cx.emit(FindFilesEvent::Close);
        }
    }

    /// `Ctrl+Enter`: the results in a tab, and the window closes.
    fn open_tab(&mut self, cx: &mut Context<Self>) {
        if self.state.files().is_empty() {
            return;
        }
        cx.emit(FindFilesEvent::OpenTab {
            query: self.state.text.clone(),
            files: self.state.files().to_vec(),
        });
        cx.emit(FindFilesEvent::Close);
    }

    fn replacement(&self, cx: &App) -> String {
        self.replace_input.read(cx).value().to_string()
    }

    /// Replace: the selected match alone.
    fn replace_selected(&mut self, cx: &mut Context<Self>) {
        let (Some(query), Some((file, matched))) = (self.state.query.clone(), self.state.current())
        else {
            return;
        };
        let files = vec![(file.path.clone(), vec![matched.offset.start])];
        cx.emit(FindFilesEvent::Replace {
            query,
            with: self.replacement(cx),
            preserve_case: self.state.preserve_case,
            files,
            confirm: false,
        });
    }

    /// Replace all: every match found, after a question.
    fn replace_all(&mut self, cx: &mut Context<Self>) {
        let Some(query) = self.state.query.clone() else {
            return;
        };
        let files = self
            .state
            .files()
            .iter()
            .map(|file| {
                let starts = file.matches.iter().map(|m| m.offset.start).collect();
                (file.path.clone(), starts)
            })
            .collect::<Vec<_>>();
        if files.is_empty() {
            return;
        }
        cx.emit(FindFilesEvent::Replace {
            query,
            with: self.replacement(cx),
            preserve_case: self.state.preserve_case,
            files,
            confirm: true,
        });
    }

    fn toggle_option(
        &mut self,
        flip: impl FnOnce(&mut ted_core::SearchOptions),
        cx: &mut Context<Self>,
    ) {
        flip(&mut self.state.options);
        self.search(cx);
    }

    fn set_scope(&mut self, scope: Scope, cx: &mut Context<Self>) {
        self.state.scope = scope;
        self.search(cx);
    }

    /// "Directory…": asks for a directory and searches under it.
    fn pick_directory(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let receiver = cx.prompt_for_paths(PathPromptOptions {
            files: false,
            directories: true,
            multiple: false,
            prompt: None,
        });
        cx.spawn_in(window, async move |this, cx| {
            let Ok(Ok(Some(mut paths))) = receiver.await else {
                return;
            };
            let Some(path) = paths.pop() else { return };
            let _ = this.update(cx, |view, cx| view.set_scope(Scope::Directory(path), cx));
        })
        .detach();
    }

    fn close(&mut self, cx: &mut Context<Self>) {
        // Stops a running search: its next batch sees another generation.
        self.state.generation += 1;
        self.state.searching = false;
        cx.emit(FindFilesEvent::Close);
    }
}

/// The files to search for `scope`, with the path each one shows: relative to `root` when it is
/// inside it. Honors `.gitignore` and the exclude patterns, as the explorer does.
fn list_files(
    scope: &Scope,
    root: Option<&Path>,
    excludes: &[String],
    open: &HashMap<PathBuf, Buffer>,
) -> Vec<(PathBuf, String)> {
    let shown = |path: &Path| -> String {
        match root.and_then(|root| path.strip_prefix(root).ok()) {
            Some(relative) => relative
                .components()
                .map(|part| part.as_os_str().to_string_lossy())
                .collect::<Vec<_>>()
                .join("/"),
            None => path.display().to_string(),
        }
    };
    let walk = |dir: &Path| -> Vec<(PathBuf, String)> {
        let filter = ExplorerFilter::new(dir, excludes);
        let index = index_files(&filter, dir, MAX_FILES);
        (0..index.len())
            .map(|i| {
                let path = index.absolute(i);
                let relative = shown(&path);
                (path, relative)
            })
            .collect()
    };
    match (scope, root) {
        (Scope::Folder, Some(root)) => walk(root),
        (Scope::Directory(dir), _) => walk(dir),
        _ => {
            let mut files: Vec<(PathBuf, String)> = open
                .keys()
                .map(|path| (path.clone(), shown(path)))
                .collect();
            files.sort_by(|a, b| a.1.cmp(&b.1));
            files
        }
    }
}

/// Searches `files` from `from` on, until [`BATCH_FILES`] files, [`BATCH_TIME`] or `room`
/// matches. Returns the files with matches and where the next batch starts.
fn search_batch(
    query: &SearchQuery,
    mask: &FileMask,
    files: &[(PathBuf, String)],
    from: usize,
    open: &HashMap<PathBuf, Buffer>,
    room: usize,
) -> (Vec<FileResult>, usize) {
    let started = Instant::now();
    let mut results = Vec::new();
    let mut found = 0;
    let mut next = from;
    while next < files.len()
        && next - from < BATCH_FILES
        && started.elapsed() < BATCH_TIME
        && found < room
    {
        let (path, relative) = &files[next];
        next += 1;
        if !mask.matches(relative) {
            continue;
        }
        let matches = match open.get(path) {
            Some(buffer) => find_lines(query, buffer, room - found),
            None => match read_text(path) {
                Some(buffer) => find_lines(query, &buffer, room - found),
                None => continue,
            },
        };
        if matches.is_empty() {
            continue;
        }
        found += matches.len();
        results.push(FileResult {
            path: path.clone(),
            relative: relative.clone(),
            matches,
        });
    }
    (results, next)
}

impl Render for FindFilesView {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let palette = cx.palette();
        let replace = self.state.replace;
        let pinned = self.state.pinned;
        let mono = cx.theme().mono_font_family.clone();
        let hover = palette.hover;

        let header = h_flex()
            .id("find-files-header")
            .items_center()
            .gap(px(10.0))
            .pt(px(12.0))
            .pb(px(8.0))
            .pl(px(16.0))
            .pr(px(12.0))
            .cursor_move()
            .on_mouse_down(
                MouseButton::Left,
                cx.listener(|this, event: &MouseDownEvent, _, cx| {
                    this.grab = Some(event.position - this.offset);
                    cx.notify();
                }),
            )
            .child(
                div()
                    .font_weight(FontWeight::SEMIBOLD)
                    .child(match replace {
                        true => t!("find_files.title_replace"),
                        false => t!("find_files.title"),
                    }),
            )
            .child(
                div()
                    .text_size(px(12.0))
                    .text_color(if self.state.invalid.is_some() {
                        palette.danger
                    } else {
                        palette.fg_subtle
                    })
                    .child(self.state.summary()),
            )
            .child(div().flex_1())
            .child(
                div()
                    .id("find-files-mask")
                    .w(px(170.0))
                    .on_mouse_down(MouseButton::Left, |_, _, cx| cx.stop_propagation())
                    .tooltip(|window, cx| {
                        Tooltip::new(SharedString::from(t!("find_files.mask_tooltip")))
                            .build(window, cx)
                    })
                    .child(
                        h_flex()
                            .child(Field::new(&self.mask_input, 26.0, 12.0).icon(IconName::Funnel)),
                    ),
            )
            .child(
                div()
                    .id("find-files-pin")
                    .size(px(26.0))
                    .flex()
                    .items_center()
                    .justify_center()
                    .rounded(px(6.0))
                    .cursor_pointer()
                    .on_mouse_down(MouseButton::Left, |_, _, cx| cx.stop_propagation())
                    .text_color(if pinned {
                        palette.accent_text
                    } else {
                        palette.fg_muted
                    })
                    .when(pinned, |button| button.bg(palette.accent_subtle))
                    .when(!pinned, |button| button.hover(move |style| style.bg(hover)))
                    .tooltip(move |window, cx| {
                        let label = match pinned {
                            true => t!("find_files.unpin"),
                            false => t!("find_files.pin"),
                        };
                        Tooltip::new(SharedString::from(label)).build(window, cx)
                    })
                    .child(Icon::new(IconName::Pin).size(px(14.0)))
                    .on_click(cx.listener(|this, _, _, cx| {
                        this.state.pinned = !this.state.pinned;
                        cx.notify();
                    })),
            )
            .child(
                div()
                    .id("find-files-close")
                    .size(px(26.0))
                    .flex()
                    .items_center()
                    .justify_center()
                    .rounded(px(6.0))
                    .cursor_pointer()
                    .on_mouse_down(MouseButton::Left, |_, _, cx| cx.stop_propagation())
                    .text_color(palette.fg_muted)
                    .hover(move |style| style.bg(hover))
                    .tooltip(|window, cx| {
                        Tooltip::new(SharedString::from(t!("find.close"))).build(window, cx)
                    })
                    .child(Icon::new(IconName::X).size(px(15.0)))
                    .on_click(cx.listener(|this, _, _, cx| this.close(cx))),
            );

        let options = self.state.options;
        let query_field = Field::new(&self.query_input, 32.0, 13.0)
            .icon(IconName::Search)
            .suffix(
                toggle(
                    "find-files-case",
                    IconName::CaseSensitive,
                    t!("find.case").into(),
                    options.case_sensitive,
                    cx,
                )
                .on_click(cx.listener(|this, _, _, cx| {
                    this.toggle_option(|options| options.case_sensitive ^= true, cx)
                })),
            )
            .suffix(
                toggle(
                    "find-files-word",
                    IconName::WholeWord,
                    t!("find.whole_word").into(),
                    options.whole_word,
                    cx,
                )
                .on_click(cx.listener(|this, _, _, cx| {
                    this.toggle_option(|options| options.whole_word ^= true, cx)
                })),
            )
            .suffix(
                toggle(
                    "find-files-regex",
                    IconName::Regex,
                    t!("find.regex").into(),
                    options.regex,
                    cx,
                )
                .on_click(cx.listener(|this, _, _, cx| {
                    this.toggle_option(|options| options.regex ^= true, cx)
                })),
            );
        let replace_field = replace.then(|| {
            Field::new(&self.replace_input, 32.0, 13.0)
                .icon(IconName::Replace)
                .suffix(
                    toggle(
                        "find-files-preserve-case",
                        IconName::CaseUpper,
                        t!("find.preserve_case").into(),
                        self.state.preserve_case,
                        cx,
                    )
                    .on_click(cx.listener(|this, _, _, cx| {
                        this.state.preserve_case = !this.state.preserve_case;
                        cx.notify();
                    })),
                )
        });
        let fields = v_flex()
            .gap(px(6.0))
            .px(px(12.0))
            .child(h_flex().child(query_field))
            .children(replace_field.map(|field| h_flex().child(field)));

        let scope_item = |id: &'static str, label: SharedString, on: bool| {
            div()
                .id(id)
                .h(px(24.0))
                .flex()
                .items_center()
                .px(px(10.0))
                .rounded(px(6.0))
                .text_size(px(12.5))
                .cursor_pointer()
                .text_color(if on { palette.fg } else { palette.fg_muted })
                .when(on, |item| item.bg(palette.raised))
                .when(!on, |item| item.hover(move |style| style.bg(hover)))
                .child(label)
        };
        let directory_label: SharedString = match &self.state.scope {
            Scope::Directory(dir) => dir
                .file_name()
                .map(|name| format!("{}/", name.to_string_lossy()))
                .unwrap_or_else(|| dir.display().to_string())
                .into(),
            _ => t!("find_files.scope_directory").into(),
        };
        let has_folder = self.root.is_some();
        let scopes = h_flex()
            .items_center()
            .gap(px(2.0))
            .px(px(12.0))
            .py(px(8.0))
            .when(has_folder, |row| {
                row.child(
                    scope_item(
                        "find-files-scope-folder",
                        t!("find_files.scope_folder").into(),
                        self.state.scope == Scope::Folder,
                    )
                    .on_click(cx.listener(|this, _, _, cx| this.set_scope(Scope::Folder, cx))),
                )
            })
            .child(
                scope_item(
                    "find-files-scope-directory",
                    directory_label,
                    matches!(self.state.scope, Scope::Directory(_)),
                )
                .on_click(cx.listener(|this, _, window, cx| this.pick_directory(window, cx))),
            )
            .child(
                scope_item(
                    "find-files-scope-open",
                    t!("find_files.scope_open").into(),
                    self.state.scope == Scope::OpenFiles,
                )
                .on_click(cx.listener(|this, _, _, cx| this.set_scope(Scope::OpenFiles, cx))),
            );

        let rows = self.state.row_count();
        let list = uniform_list(
            "find-files-rows",
            rows,
            cx.processor(|view, range: std::ops::Range<usize>, _window, cx| {
                range.map(|row| view.render_row(row, cx)).collect()
            }),
        )
        .track_scroll(&self.list_scroll)
        .size_full();
        let results = div()
            .h(px(FIND_FILES_LIST_HEIGHT))
            .border_t_1()
            .border_color(palette.border)
            .py(px(4.0))
            .child(list);

        let preview = self.render_preview(&mono, cx);

        let footer = h_flex()
            .items_center()
            .gap(px(10.0))
            .px(px(16.0))
            .py(px(10.0))
            .border_t_1()
            .border_color(palette.border)
            .child(
                TriCheckbox::new(
                    "find-files-new-tab",
                    if self.state.new_tab {
                        CheckState::Checked
                    } else {
                        CheckState::Unchecked
                    },
                )
                .label(t!("find_files.new_tab"))
                .on_click(cx.listener(|this, _, _, cx| {
                    this.state.new_tab = !this.state.new_tab;
                    cx.notify();
                })),
            )
            .child(div().flex_1())
            .map(|footer| match replace {
                true => footer
                    .child(
                        SecondaryButton::new("find-files-replace", t!("find_files.replace"))
                            .height(28.0)
                            .disabled(self.state.current().is_none())
                            .on_click(cx.listener(|this, _, _, cx| this.replace_selected(cx))),
                    )
                    .child(
                        PrimaryButton::new(
                            "find-files-replace-all",
                            t!(
                                "find_files.replace_all",
                                count = telaio_app::number::format_integer(
                                    self.state.match_count() as u64
                                )
                            ),
                        )
                        .height(28.0)
                        .disabled(self.state.match_count() == 0)
                        .on_click(cx.listener(|this, _, _, cx| this.replace_all(cx))),
                    ),
                false => footer
                    .child(
                        div()
                            .text_size(px(12.0))
                            .text_color(palette.fg_subtle)
                            .child(OPEN_TAB_HINT),
                    )
                    .child(
                        SecondaryButton::new("find-files-open-tab", t!("find_files.open_tab"))
                            .height(28.0)
                            .disabled(self.state.files().is_empty())
                            .on_click(cx.listener(|this, _, _, cx| this.open_tab(cx))),
                    ),
            });

        let panel = v_flex()
            .id("find-files")
            .key_context(CONTEXT)
            .track_focus(&self.focus_handle)
            .occlude()
            .relative()
            .left(self.offset.x)
            .top(self.offset.y)
            .w(px(FIND_FILES_WIDTH))
            .max_w_full()
            .bg(palette.overlay)
            .border_1()
            .border_color(palette.border_strong)
            .rounded(px(RADIUS_DIALOG))
            .shadow(palette.shadow.clone())
            .overflow_hidden()
            .text_size(px(13.0))
            .on_action(cx.listener(|this, _: &CloseFindFiles, _, cx| this.close(cx)))
            .on_action(cx.listener(|this, _: &NextResult, _, cx| this.select_by(1, cx)))
            .on_action(cx.listener(|this, _: &PreviousResult, _, cx| this.select_by(-1, cx)))
            .on_action(cx.listener(|this, _: &ToggleCase, _, cx| {
                this.toggle_option(|options| options.case_sensitive ^= true, cx)
            }))
            .on_action(cx.listener(|this, _: &ToggleWholeWord, _, cx| {
                this.toggle_option(|options| options.whole_word ^= true, cx)
            }))
            .on_action(cx.listener(|this, _: &ToggleRegex, _, cx| {
                this.toggle_option(|options| options.regex ^= true, cx)
            }))
            .on_mouse_down_out(cx.listener(|this, _, _, cx| {
                if !this.state.pinned && this.grab.is_none() {
                    this.close(cx);
                }
            }))
            .child(header)
            .child(fields)
            .child(scopes)
            .child(results)
            .child(preview)
            .child(footer);

        let row = h_flex()
            .absolute()
            .top(px(FIND_FILES_TOP))
            .left_0()
            .right_0()
            .justify_center()
            .child(panel);
        let _ = window;
        // While the header is dragged, a layer over the whole window follows the pointer.
        match self.grab {
            Some(_) => div()
                .absolute()
                .inset_0()
                .occlude()
                .cursor_move()
                .on_mouse_move(cx.listener(|this, event: &MouseMoveEvent, _, cx| {
                    if let Some(grab) = this.grab {
                        this.offset = event.position - grab;
                        cx.notify();
                    }
                }))
                .on_mouse_up(
                    MouseButton::Left,
                    cx.listener(|this, _, _, cx| {
                        this.grab = None;
                        cx.notify();
                    }),
                )
                .child(row)
                .into_any_element(),
            None => row.into_any_element(),
        }
    }
}

/// The key that leaves the results in a tab, as the footer shows it.
#[cfg(target_os = "macos")]
const OPEN_TAB_HINT: &str = "\u{2318}\u{21b5}";
#[cfg(not(target_os = "macos"))]
const OPEN_TAB_HINT: &str = "Ctrl+Enter";

impl FindFilesView {
    /// One row of the results: the line of the match, its file and its line number.
    fn render_row(&self, row: usize, cx: &mut Context<Self>) -> AnyElement {
        let palette = cx.palette();
        let Some((file, matched)) = self.state.row(row) else {
            return div().h(px(FIND_FILES_ROW_HEIGHT)).into_any_element();
        };
        let selected = self.state.selected == Some(row);
        let replacement = self.preview_replacement(&matched.text[matched.range.clone()], cx);
        let text = match_text(
            &matched.text,
            &matched.range,
            true,
            replacement.as_deref(),
            selected,
            &palette,
        );
        let hover = palette.hover;
        h_flex()
            .id(("find-files-row", row))
            .w_full()
            .h(px(FIND_FILES_ROW_HEIGHT))
            .items_center()
            .gap(px(16.0))
            .px(px(16.0))
            .cursor_pointer()
            .when(selected, |row| row.bg(palette.accent_subtle))
            .when(!selected, |row| row.hover(move |style| style.bg(hover)))
            .child(
                div()
                    .flex_1()
                    .min_w_0()
                    .truncate()
                    .font_family(cx.theme().mono_font_family.clone())
                    .text_size(px(12.5))
                    .child(text),
            )
            .child(
                h_flex()
                    .flex_none()
                    .gap(px(6.0))
                    .text_size(px(12.0))
                    .child(
                        div()
                            .text_color(palette.fg_muted)
                            .child(file.name().to_string()),
                    )
                    .child(
                        div()
                            .text_color(palette.fg_subtle)
                            .font_family(cx.theme().mono_font_family.clone())
                            .child((matched.line + 1).to_string()),
                    ),
            )
            .on_click(cx.listener(move |this, event: &ClickEvent, window, cx| {
                this.state.selected = Some(row);
                // The keys (arrows, Enter, Escape) keep working after a click.
                this.query_input
                    .update(cx, |input, cx| input.focus(window, cx));
                cx.notify();
                if event.click_count() > 1 {
                    this.open_selected(window, cx);
                }
            }))
            .into_any_element()
    }

    /// The replacement of `matched` while Replace in files shows, for the preview of the change.
    fn preview_replacement(&self, matched: &str, cx: &App) -> Option<String> {
        if !self.state.replace {
            return None;
        }
        let query = self.state.query.as_ref()?;
        Some(query.replacement(matched, &self.replacement(cx), self.state.preserve_case))
    }

    /// The preview of the selected match: its file and the lines around it.
    fn render_preview(&self, mono: &SharedString, cx: &mut Context<Self>) -> AnyElement {
        let palette = cx.palette();
        let Some((file, matched)) = self.state.current() else {
            return div()
                .h(px(30.0 + 21.0 * 5.0 + 12.0))
                .flex()
                .items_center()
                .justify_center()
                .border_t_1()
                .border_color(palette.border)
                .bg(palette.bg)
                .text_size(px(12.0))
                .text_color(palette.fg_subtle)
                .child(t!("find_files.preview_empty"))
                .into_any_element();
        };
        let replacement = self.preview_replacement(&matched.text[matched.range.clone()], cx);
        let lines = matched.context.iter().map(|(number, text)| {
            let current = *number == matched.line;
            let content = match current {
                true => match_text(
                    &matched.text,
                    &matched.range,
                    false,
                    replacement.as_deref(),
                    true,
                    &palette,
                ),
                false => StyledText::new(text.clone()).into_any_element(),
            };
            h_flex()
                .h(px(21.0))
                .when(current, |line| line.bg(palette.hover))
                .child(
                    div()
                        .flex_none()
                        .w(px(52.0))
                        .pr(px(14.0))
                        .text_right()
                        .text_color(if current {
                            palette.fg
                        } else {
                            palette.fg_subtle
                        })
                        .child((number + 1).to_string()),
                )
                .child(div().flex_1().min_w_0().truncate().child(content))
        });
        v_flex()
            .child(
                h_flex()
                    .h(px(30.0))
                    .items_center()
                    .gap(px(8.0))
                    .px(px(16.0))
                    .border_t_1()
                    .border_color(palette.border)
                    .bg(palette.surface)
                    .text_size(px(12.0))
                    .child(
                        div()
                            .font_weight(FontWeight::MEDIUM)
                            .child(file.name().to_string()),
                    )
                    .child(
                        div()
                            .min_w_0()
                            .truncate()
                            .text_color(palette.fg_subtle)
                            .child(file.dir().to_string()),
                    ),
            )
            .child(
                v_flex()
                    .h(px(21.0 * 5.0 + 12.0))
                    .py(px(6.0))
                    .bg(palette.bg)
                    .font_family(mono.clone())
                    .text_size(px(12.5))
                    .whitespace_nowrap()
                    .children(lines),
            )
            .into_any_element()
    }
}

/// The text of a match's line: the match tinted (strong when `strong`), or, with a
/// `replacement`, the old text struck through and the new one in green. `trim` leaves the
/// indentation out, for the rows of the list.
pub(crate) fn match_text(
    text: &str,
    range: &std::ops::Range<usize>,
    trim: bool,
    replacement: Option<&str>,
    strong: bool,
    palette: &crate::theme::Palette,
) -> AnyElement {
    let (before, matched, after) = match trim {
        true => row_parts(text, range),
        false => (
            text[..range.start.min(text.len())].to_string(),
            text[range.start.min(text.len())..range.end.min(text.len())].to_string(),
            text[range.end.min(text.len())..].to_string(),
        ),
    };
    let mut shown = before.clone();
    let mut highlights = Vec::new();
    let start = shown.len();
    shown.push_str(&matched);
    match replacement {
        Some(new) => {
            highlights.push((
                start..shown.len(),
                HighlightStyle {
                    color: Some(palette.fg_subtle),
                    strikethrough: Some(StrikethroughStyle {
                        thickness: px(1.0),
                        color: Some(palette.fg_subtle),
                    }),
                    ..Default::default()
                },
            ));
            let new_start = shown.len();
            shown.push_str(new);
            highlights.push((
                new_start..shown.len(),
                HighlightStyle {
                    background_color: Some(palette.success.opacity(0.2)),
                    ..Default::default()
                },
            ));
        }
        None => highlights.push((
            start..shown.len(),
            HighlightStyle {
                background_color: Some(if strong {
                    palette.accent.opacity(0.45)
                } else {
                    palette.warning.opacity(0.24)
                }),
                ..Default::default()
            },
        )),
    }
    shown.push_str(&after);
    let highlights: Vec<_> = highlights
        .into_iter()
        .filter(|(range, _)| !range.is_empty())
        .collect();
    StyledText::new(shown)
        .with_highlights(highlights)
        .into_any_element()
}
