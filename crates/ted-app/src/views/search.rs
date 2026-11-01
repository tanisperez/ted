//! The global search (`Ctrl+K`, and the title bar's search box in folder mode): a palette at the
//! top of the window that finds the folder's files and the app's actions as the user types.
//!
//! Built on gpui-kit's `Command`, as Postino's command palette, with its own matching turned off
//! (`filterable(false)`): the rows it gets are already ranked by `state::palette`. The palette
//! always has two groups, files and actions; the one with the best match goes first, so a row's
//! `IndexPath.section` says what it is once that order is known.
//!
//! The files come from a [`FileIndex`] walked on the background executor each time the search
//! opens; the previous one shows meanwhile. Ranking them runs in the background too, one task per
//! query, and a result whose query is no longer the current one is dropped (a generation
//! counter). Only the best [`MAX_FILE_RESULTS`] are rendered.

use std::path::{Path, PathBuf};
use std::sync::Arc;

use gpui_kit::assets::IconName as Lucide;
use gpui_kit::component::command::{Command, CommandGroup, CommandItem, CommandState};
use gpui_kit::component::input::{Input, InputEvent, InputState};
use gpui_kit::component::kbd::Kbd;
use gpui_kit::component::*;
use gpui_kit::prelude::*;
use gpui_kit::*;
use rust_i18n::t;
use ted_workspace::{ExplorerFilter, FileIndex, index_files};
use telaio_ui::components::{MODAL_INNER_RADIUS, ModalPlacement, edit_menu, framed_dialog};

use crate::actions;
use crate::state::launch::{Target, shorten_path};
use crate::state::palette::{self, ActionId, MAX_FILE_RESULTS, Match};
use crate::theme::{Palette, PaletteExt};

use super::editor::KEY_CONTEXT as EDITOR_CONTEXT;
use super::root::AppView;

/// Width of the palette.
const PALETTE_WIDTH: f32 = 560.0;
/// Max height of the list under the search field.
const PALETTE_LIST_HEIGHT: f32 = 440.0;
/// Room between the list and the card's rounded bottom corners.
const PALETTE_BOTTOM_PADDING: f32 = 8.0;
/// Height of the search row: `Command`'s own field cannot be resized, so the palette draws one.
const SEARCH_ROW_HEIGHT: f32 = 36.0;
/// Right margin of a row's shortcut, so the list's scrollbar does not cover it.
const SCROLLBAR_CLEARANCE: f32 = 6.0;
/// The most files listed for a folder. Past it the search says only the first ones are searched.
const MAX_INDEXED_FILES: usize = 200_000;

/// The files of the search and their ranking for the current query. One per open palette.
pub(crate) struct SearchResults {
    query: String,
    /// Bumped by every query, so a ranking that finishes late is dropped.
    generation: u64,
    /// The folder's files: `None` until the first listing of this folder ends.
    index: Option<Arc<FileIndex>>,
    /// A listing of the folder runs.
    indexing: bool,
    /// The best files for `query`, indices into `index`.
    files: Vec<Match>,
    /// The ranking in flight; replacing it drops (cancels) the previous one.
    _ranking: Option<Task<()>>,
}

impl SearchResults {
    fn new(index: Option<Arc<FileIndex>>, indexing: bool) -> Self {
        Self {
            query: String::new(),
            generation: 0,
            index,
            indexing,
            files: Vec::new(),
            _ranking: None,
        }
    }

    fn set_query(&mut self, query: String, cx: &mut Context<Self>) {
        if self.query == query {
            return;
        }
        self.query = query;
        self.rank(cx);
    }

    /// A new listing of the folder: the ranking runs again over it.
    pub(crate) fn set_index(&mut self, index: Arc<FileIndex>, cx: &mut Context<Self>) {
        self.index = Some(index);
        self.indexing = false;
        self.rank(cx);
    }

    /// Ranks the files for the current query in the background.
    fn rank(&mut self, cx: &mut Context<Self>) {
        self.generation += 1;
        let generation = self.generation;
        let Some(index) = self.index.clone() else {
            cx.notify();
            return;
        };
        if palette::normalize_query(&self.query).is_empty() {
            self.files.clear();
            self._ranking = None;
            cx.notify();
            return;
        }
        let query = self.query.clone();
        self._ranking = Some(cx.spawn(async move |this, cx| {
            let files = cx
                .background_spawn(async move {
                    let started = std::time::Instant::now();
                    let files = palette::rank_paths(&query, index.iter(), MAX_FILE_RESULTS);
                    log::debug!("ranked {} files in {:?}", index.len(), started.elapsed());
                    files
                })
                .await;
            let _ = this.update(cx, |results, cx| {
                if results.generation == generation {
                    results.files = files;
                    cx.notify();
                }
            });
        }));
    }
}

/// What a file row opens.
#[derive(Clone)]
struct FileRow {
    path: PathBuf,
    /// The path shown, relative to the folder (or shortened, for a recent file outside it).
    shown: String,
    /// The matched char indices of `shown`.
    positions: Vec<usize>,
}

/// One action row, with its shortcut worked out when the palette opened.
#[derive(Clone)]
struct ActionRow {
    action: ActionId,
    label: String,
    shortcut: Option<SharedString>,
}

impl AppView {
    /// Opens the global search. A no-op while another dialog shows, so `Ctrl+K` never stacks a
    /// second one.
    pub(crate) fn open_search(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if window.has_active_dialog(cx) {
            return;
        }
        let root = match &self.target {
            Some(Target::Folder(root)) => Some(root.clone()),
            _ => None,
        };
        // A listing of another folder is useless here.
        if self.file_index.as_ref().map(|index| index.root()) != root.as_deref() {
            self.file_index = None;
        }
        if let Some(root) = &root {
            self.refresh_file_index(root.clone(), cx);
        }
        let indexing = root.is_some() && self.indexing == root;
        let results = cx.new(|_| SearchResults::new(self.file_index.clone(), indexing));
        self.search = Some(results.downgrade());

        let recent = recent_rows(&self.recents.files, root.as_deref());
        let editor = self.editor().cloned();
        let context = palette::Context {
            folder: root.is_some(),
            document: self.documents.active().is_some(),
            editor: editor.is_some(),
        };
        let actions: Vec<ActionRow> = ActionId::available(context)
            .into_iter()
            .map(|action| ActionRow {
                action,
                label: action.label(),
                shortcut: shortcut_of(action, window),
            })
            .collect();

        let weak = cx.weak_entity();
        let command_state = cx.new(|cx| CommandState::new(window, cx));
        let search_input = cx.new(|cx| InputState::new(window, cx).placeholder(t!("search.hint")));
        // Typing drives both `Command`'s query (it resets the selection) and the file ranking.
        let query_subscription = window.subscribe(&search_input, cx, {
            let command_state = command_state.clone();
            let results = results.clone();
            move |input, event: &InputEvent, window, cx| {
                if matches!(event, InputEvent::Change) {
                    let query = input.read(cx).value();
                    results.update(cx, |results, cx| results.set_query(query.to_string(), cx));
                    command_state.update(cx, |state, cx| state.set_query(query, window, cx));
                }
            }
        });
        let focus_input = search_input.clone();
        window.open_dialog(cx, move |dialog, window, _cx| {
            let _ = &query_subscription;
            let weak = weak.clone();
            let command_state = command_state.clone();
            let search_input = search_input.clone();
            let results = results.clone();
            let recent = recent.clone();
            let actions = actions.clone();
            let editor = editor.clone();
            framed_dialog(
                dialog,
                PALETTE_WIDTH,
                None,
                ModalPlacement::Top {
                    fraction: 0.14,
                    min: 48.0,
                },
                window,
                move |_window, cx| {
                    div()
                        .pb(px(PALETTE_BOTTOM_PADDING))
                        .child(render_palette(
                            &weak,
                            &command_state,
                            &search_input,
                            &results,
                            &recent,
                            &actions,
                            editor.as_ref(),
                            cx,
                        ))
                        .into_any_element()
                },
            )
        });
        // After `open_dialog`, which keeps the focus it finds to give it back on close.
        focus_input.update(cx, |state, cx| state.focus(window, cx));
    }

    /// Lists the folder's files again in the background, unless a listing of it already runs.
    /// The result replaces the previous one and goes to the open search.
    fn refresh_file_index(&mut self, root: PathBuf, cx: &mut Context<Self>) {
        if self.indexing.as_ref() == Some(&root) {
            return;
        }
        self.indexing = Some(root.clone());
        let excludes = self.settings.explorer_exclude.clone();
        cx.spawn(async move |this, cx| {
            let walked_root = root.clone();
            let index = cx
                .background_spawn(async move {
                    let filter = ExplorerFilter::new(&walked_root, &excludes);
                    index_files(&filter, &walked_root, MAX_INDEXED_FILES)
                })
                .await;
            let _ = this.update(cx, |view, cx| {
                if view.indexing.as_ref() == Some(&root) {
                    view.indexing = None;
                }
                // The folder was closed or replaced meanwhile.
                if !matches!(&view.target, Some(Target::Folder(open)) if *open == root) {
                    return;
                }
                let index = Arc::new(index);
                view.file_index = Some(Arc::clone(&index));
                if let Some(results) = view.search.as_ref().and_then(WeakEntity::upgrade) {
                    results.update(cx, |results, cx| results.set_index(index, cx));
                }
            });
        })
        .detach();
    }
}

/// The recent files, shown while the query is empty: in folder mode the ones inside the folder,
/// relative to it; otherwise all of them, shortened.
fn recent_rows(files: &[PathBuf], root: Option<&Path>) -> Vec<FileRow> {
    let home = std::env::home_dir();
    files
        .iter()
        .filter_map(|path| {
            let shown = match root {
                Some(root) => relative_shown(path.strip_prefix(root).ok()?),
                None => shorten_path(path, home.as_deref()),
            };
            Some(FileRow {
                path: path.clone(),
                shown,
                positions: Vec::new(),
            })
        })
        .collect()
}

/// A relative path as the index spells it, `/` separated.
fn relative_shown(relative: &Path) -> String {
    relative
        .components()
        .map(|part| part.as_os_str().to_string_lossy())
        .collect::<Vec<_>>()
        .join("/")
}

/// The keys of `action` as this platform types them (`Ctrl+N`, `⌘N`), read from the key bindings
/// so they never drift from them. Editor actions are looked up in the editor's key context.
fn shortcut_of(action: ActionId, window: &Window) -> Option<SharedString> {
    let boxed = boxed_action(action)?;
    let context = match is_editor_action(action) {
        true => KeyContext::parse(EDITOR_CONTEXT).ok()?,
        false => KeyContext::default(),
    };
    let binding = window.highest_precedence_binding_for_action_in_context(&*boxed, context)?;
    let keystroke = binding.keystrokes().first()?;
    Some(Kbd::format(keystroke.as_keystroke()).into())
}

fn is_editor_action(action: ActionId) -> bool {
    matches!(
        action,
        ActionId::Find | ActionId::Replace | ActionId::GoToLine | ActionId::ToggleWrap
    )
}

/// The gpui action an [`ActionId`] dispatches.
fn boxed_action(action: ActionId) -> Option<Box<dyn Action>> {
    use ActionId::*;
    Some(match action {
        NewFile => Box::new(actions::NewFile),
        OpenFile => Box::new(actions::OpenFile),
        OpenFolder => Box::new(actions::OpenFolder),
        Save => Box::new(actions::Save),
        CloseTab => Box::new(actions::CloseTab),
        OpenSettings => Box::new(actions::OpenSettings),
        OpenShortcuts => Box::new(actions::OpenShortcuts),
        ToggleExplorer => Box::new(actions::ToggleExplorer),
        ToggleGit => Box::new(actions::ToggleGit),
        ZoomIn => Box::new(actions::ZoomIn),
        ZoomOut => Box::new(actions::ZoomOut),
        ZoomReset => Box::new(actions::ZoomReset),
        CopyPath => Box::new(actions::CopyPath),
        Reveal => Box::new(actions::RevealInFileManager),
        Find => Box::new(super::find::OpenFind),
        Replace => Box::new(super::find::OpenReplace),
        GoToLine => Box::new(super::goto::OpenGoto),
        ToggleWrap => Box::new(super::editor::ToggleWrap),
    })
}

/// The palette's content: the search row and the two groups, rebuilt on every repaint from the
/// current query and ranking.
#[allow(clippy::too_many_arguments)]
fn render_palette(
    weak: &WeakEntity<AppView>,
    command_state: &Entity<CommandState>,
    search_input: &Entity<InputState>,
    results: &Entity<SearchResults>,
    recent: &[FileRow],
    actions: &[ActionRow],
    editor: Option<&Entity<super::editor::EditorView>>,
    cx: &mut App,
) -> AnyElement {
    let palette = cx.palette();
    let mono_font = cx.theme().mono_font_family.clone();
    let state = results.read(cx);
    let query_empty = palette::normalize_query(&state.query).is_empty();

    let files: Vec<FileRow> = match (&state.index, query_empty) {
        (_, true) => recent.to_vec(),
        (Some(index), false) => state
            .files
            .iter()
            .filter(|found| found.index < index.len())
            .map(|found| FileRow {
                path: index.absolute(found.index),
                shown: index.get(found.index).to_string(),
                positions: found.positions.clone(),
            })
            .collect(),
        (None, false) => Vec::new(),
    };
    let labels: Vec<String> = actions.iter().map(|row| row.label.clone()).collect();
    let ranked_actions: Vec<(ActionRow, Vec<usize>, i32)> =
        palette::rank_labels(&state.query, &labels)
            .into_iter()
            .map(|found| (actions[found.index].clone(), found.positions, found.score))
            .collect();
    let note = match (&state.index, state.indexing) {
        (None, true) if !query_empty => Some(t!("search.indexing").into_owned()),
        (Some(index), _) if index.truncated() => Some(
            t!(
                "search.truncated",
                count = telaio_app::number::format_integer(index.len() as u64)
            )
            .into_owned(),
        ),
        _ => None,
    };

    let file_heading = match query_empty {
        true => t!("search.recent"),
        false => t!("search.files"),
    };
    let mut file_group = CommandGroup::new().label(file_heading.into_owned());
    for row in &files {
        let row = row.clone();
        let row_palette = palette.clone();
        file_group = file_group
            .item(CommandItem::new().child(move |_, _| render_file_row(&row, &row_palette)));
    }
    let mut action_group = CommandGroup::new().label(t!("search.actions").into_owned());
    for (row, positions, _) in &ranked_actions {
        let row = row.clone();
        let positions = positions.clone();
        let row_palette = palette.clone();
        let row_font = mono_font.clone();
        action_group = action_group.item(CommandItem::new().child(move |_, _| {
            render_action_row(&row, &positions, &row_palette, row_font.clone())
        }));
    }

    // The actions first when one matches better than every file: `go line` runs Go to line
    // before it opens `command-line.html`.
    let best_action = ranked_actions.first().map(|(_, _, score)| *score);
    let best_file = (!query_empty)
        .then(|| state.files.first().map(|found| found.score))
        .flatten();
    let actions_first = match (best_action, best_file) {
        (Some(action), Some(file)) => action > file,
        (Some(_), None) => !query_empty,
        _ => false,
    };
    let file_paths: Vec<PathBuf> = files.into_iter().map(|row| row.path).collect();
    let action_ids: Vec<ActionId> = ranked_actions
        .iter()
        .map(|(row, _, _)| row.action)
        .collect();
    let (files_section, groups) = match actions_first {
        true => (1, [action_group, file_group]),
        false => (0, [file_group, action_group]),
    };
    let weak = weak.clone();
    let editor = editor.cloned();
    let search_input = search_input.clone();
    let footer_palette = palette.clone();
    let command = Command::new(command_state)
        .bordered(false)
        .filterable(false)
        .searchable(false)
        .rounded_t(px(MODAL_INNER_RADIUS))
        .max_h(px(PALETTE_LIST_HEIGHT))
        .header(move |_, _, cx| render_search_row(&search_input, cx))
        .empty(|_, _, _| {
            div()
                .py_4()
                .flex()
                .justify_center()
                .child(t!("search.empty").into_owned())
        })
        .when_some(note, |command, note| {
            command.footer(move |_, _, _| {
                div()
                    .px_3()
                    .pt_2()
                    .text_size(px(12.0))
                    .text_color(footer_palette.fg_subtle)
                    .child(note.clone())
            })
        });
    groups
        .into_iter()
        .fold(command, Command::group)
        .on_confirm(move |index_path, window, cx| {
            let confirmed = match index_path.section == files_section {
                true => file_paths.get(index_path.row).cloned().map(Confirmed::File),
                false => action_ids
                    .get(index_path.row)
                    .copied()
                    .map(Confirmed::Action),
            };
            let Some(confirmed) = confirmed else {
                return;
            };
            window.close_dialog(cx);
            run(confirmed, weak.clone(), editor.clone(), window, cx);
        })
        .into_any_element()
}

enum Confirmed {
    File(PathBuf),
    Action(ActionId),
}

/// Opens a file, or runs an action as its key would, from where the focus was before.
fn run(
    confirmed: Confirmed,
    weak: WeakEntity<AppView>,
    editor: Option<Entity<super::editor::EditorView>>,
    window: &mut Window,
    cx: &mut App,
) {
    match confirmed {
        Confirmed::File(path) => {
            let _ = weak.update(cx, |view, cx| view.open_path(&path, window, cx));
        }
        Confirmed::Action(action) => {
            // The editor actions only reach a focused editor.
            if is_editor_action(action)
                && let Some(editor) = &editor
            {
                let focus = editor.read(cx).focus_handle().clone();
                focus.focus(window, cx);
            }
            if let Some(boxed) = boxed_action(action) {
                // On the next turn, once the dialog is gone from the dispatch tree.
                window.defer(cx, move |window, cx| window.dispatch_action(boxed, cx));
            }
        }
    }
}

/// The search row: the icon and a borderless field at [`SEARCH_ROW_HEIGHT`], inside `Command`'s
/// key context, so Up, Down, Enter and Escape typed in it still move, confirm and cancel.
fn render_search_row(search_input: &Entity<InputState>, cx: &App) -> AnyElement {
    h_flex()
        .flex_none()
        .h(px(SEARCH_ROW_HEIGHT))
        .px_3()
        .gap_2()
        .items_center()
        .border_b_1()
        .border_color(cx.theme().border)
        .child(Icon::new(IconName::Search).text_color(cx.theme().muted_foreground))
        .child(
            Input::new(search_input)
                .context_menu(edit_menu(search_input, cx))
                .appearance(false)
                .flex_1()
                .p_0(),
        )
        .into_any_element()
}

/// A file row: the file icon, its name and, after it, its folder, the matched characters of both
/// in `accent_text`.
fn render_file_row(row: &FileRow, palette: &Palette) -> AnyElement {
    let name_start = row.shown.rfind('/').map_or(0, |slash| slash + 1);
    let name_start_char = row.shown[..name_start].chars().count();
    let name = &row.shown[name_start..];
    let folder = row.shown[..name_start].trim_end_matches('/');
    let name_positions: Vec<usize> = row
        .positions
        .iter()
        .filter(|&&position| position >= name_start_char)
        .map(|position| position - name_start_char)
        .collect();
    h_flex()
        .w_full()
        .items_center()
        .gap(px(8.0))
        .child(
            Icon::new(Lucide::File)
                .small()
                .text_color(palette.fg_subtle),
        )
        .child(
            div()
                .flex_none()
                .max_w(px(PALETTE_WIDTH * 0.5))
                .truncate()
                .child(highlighted(name, &name_positions, palette.accent_text)),
        )
        .when(!folder.is_empty(), |row_element| {
            row_element.child(
                div()
                    .flex_1()
                    .min_w_0()
                    .truncate()
                    .text_size(px(12.0))
                    .text_color(palette.fg_subtle)
                    .child(highlighted(folder, &row.positions, palette.accent_text)),
            )
        })
        .into_any_element()
}

/// An action row: its icon, the label with the matched characters in `accent_text` and the
/// shortcut at the right.
fn render_action_row(
    row: &ActionRow,
    positions: &[usize],
    palette: &Palette,
    mono_font: SharedString,
) -> AnyElement {
    h_flex()
        .w_full()
        .items_center()
        .gap(px(8.0))
        .child(
            Icon::new(action_icon(row.action))
                .small()
                .text_color(palette.fg_muted),
        )
        .child(div().flex_1().min_w_0().truncate().child(highlighted(
            &row.label,
            positions,
            palette.accent_text,
        )))
        .children(row.shortcut.clone().map(|shortcut| {
            div()
                .flex_none()
                .mr(px(SCROLLBAR_CLEARANCE))
                .font_family(mono_font)
                .text_size(px(11.0))
                .text_color(palette.fg_subtle)
                .child(shortcut)
        }))
        .into_any_element()
}

fn action_icon(action: ActionId) -> Lucide {
    use ActionId::*;
    match action {
        NewFile => Lucide::FilePlus,
        OpenFile => Lucide::FileInput,
        OpenFolder => Lucide::FolderOpen,
        Save => Lucide::Save,
        CloseTab => Lucide::X,
        OpenSettings => Lucide::Settings,
        OpenShortcuts => Lucide::Keyboard,
        ToggleExplorer => Lucide::Files,
        ToggleGit => Lucide::GitBranch,
        Find | Replace => Lucide::Search,
        GoToLine => Lucide::ArrowDownToLine,
        ToggleWrap => Lucide::TextWrap,
        ZoomIn => Lucide::ZoomIn,
        ZoomOut => Lucide::ZoomOut,
        ZoomReset => Lucide::RotateCcw,
        CopyPath => Lucide::Copy,
        Reveal => Lucide::FolderSearch,
    }
}

/// `text` with the chars at `positions` (char indices, ignoring those past its end) colored
/// `color`.
fn highlighted(text: &str, positions: &[usize], color: Hsla) -> AnyElement {
    let runs: Vec<(std::ops::Range<usize>, HighlightStyle)> = text
        .char_indices()
        .enumerate()
        .filter(|(char_index, _)| positions.contains(char_index))
        .map(|(_, (start, character))| {
            (
                start..start + character.len_utf8(),
                HighlightStyle {
                    color: Some(color),
                    ..Default::default()
                },
            )
        })
        .collect();
    StyledText::new(text.to_string())
        .with_highlights(runs)
        .into_any_element()
}
