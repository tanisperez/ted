//! The diff tab: what changed in one file, from the last commit to the file on disk, as one
//! column with the line numbers of both sides, the added lines in green and the removed ones in
//! red, the changed words emphasized. Each block of changes has Stage (or Unstage) and Revert. The
//! patch is read with the whole file as context and parsed (`telaio_git::parse_file_diff`) on the
//! background executor; the rows are a `uniform_list`, so a long file costs only the rows on
//! screen. Long lines scroll sideways under the line numbers and the block's buttons, which stay
//! in place. There is one diff tab: opening another file's diff reuses it.

use std::cell::Cell;
use std::path::Path;
use std::rc::Rc;
use std::sync::Arc;
use std::time::Duration;

use gpui_kit::assets::IconName;
use gpui_kit::component::button::Button;
use gpui_kit::component::scroll::ScrollbarAxis;
use gpui_kit::component::*;
use gpui_kit::prelude::*;
use gpui_kit::*;
use rust_i18n::t;
use telaio_editor::SyntaxColors;
use telaio_git::{
    ApplyTarget, DiffSource, FileDiff, FileVersion, GitError, LineKind, Repo, block_applies,
    block_patch, parse_file_diff,
};
use telaio_ui::components::{
    ConfirmDialog, EmptyState, GhostButton, NeutralButtonExt, SecondaryButton, open_confirm_dialog,
};

use crate::actions::{NextDiffChange, OpenDiffFile, PreviousDiffChange};
use crate::state::git_diff::{
    HeaderFit, LineSpans, block_at_row, block_new_line, diff_syntax, expand_tabs, header_fit,
    segments,
};
use crate::state::locale::plural;
use crate::theme::{Palette, PaletteExt};

use super::super::root::AppView;
use super::super::scrollbars::scrollbar_overlay;
use super::letter_color;

/// Above this many change blocks the buttons are left out: checking each block against the index
/// costs one git run.
const MAX_BLOCK_ACTIONS: usize = 200;
/// Width of each line number column.
const NUMBER_WIDTH: f32 = 52.0;
/// Width of the column with the sign of a line.
const SIGN_WIDTH: f32 = 16.0;
/// Space after the end of the longest line when scrolled to the right.
const END_PADDING: f32 = 24.0;
/// Height of the header.
const HEADER_HEIGHT: f32 = 40.0;
/// The key context of the diff tab, where `F7`, `Shift+F7` and `F4` are bound (`actions.rs`).
const DIFF_KEY_CONTEXT: &str = "GitDiff";
/// How long the "press again" hint stays under the header.
const EDGE_HINT_TIME: Duration = Duration::from_secs(3);
/// Above this many bytes on a side of the file the diff is shown without syntax colors, as a
/// big response body in Postino.
const HIGHLIGHT_MAX_BYTES: usize = 4 * 1024 * 1024;

/// Whether a block of the diff is in the index already.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum BlockStage {
    /// Not staged: Stage applies it to the index.
    Unstaged,
    /// Staged: Unstage takes it back out.
    Staged,
    /// Partly staged: only Revert is offered.
    Mixed,
}

/// A patch read and parsed.
struct Loaded {
    diff: Arc<FileDiff>,
    patch: Arc<str>,
    /// The columns of the longest line, as drawn (tabs are four columns).
    longest: usize,
    /// Per change block, whether it is staged; `None` without buttons.
    stages: Vec<Option<BlockStage>>,
    /// Per line, its syntax spans; empty without a grammar.
    syntax: Arc<Vec<LineSpans>>,
    additions: usize,
    deletions: usize,
}

impl Loaded {
    /// Reads `repo_path`'s patch from `HEAD` to the work tree with the whole file as context,
    /// parses it and finds out whether each change block is staged.
    fn read(repo: &Repo, repo_path: &str) -> Result<Self, GitError> {
        let patch = repo.diff_full(repo_path, &DiffSource::HeadToWorkTree)?;
        let diff = parse_file_diff(&patch);
        let count = |kind: LineKind| diff.lines.iter().filter(|line| line.kind == kind).count();
        let (additions, deletions) = (count(LineKind::Added), count(LineKind::Removed));
        let longest = diff
            .lines
            .iter()
            .map(|line| {
                line.text
                    .chars()
                    .map(|ch| if ch == '\t' { 4 } else { 1 })
                    .sum()
            })
            .max()
            .unwrap_or(0);
        let with_actions = diff.blocks.len() <= MAX_BLOCK_ACTIONS;
        // The index once, instead of one or two `git apply --check` per block (Postino #285).
        let index = with_actions
            .then(|| repo.file_at(repo_path, &FileVersion::Index).ok().flatten())
            .flatten();
        let stages = (0..diff.blocks.len())
            .map(|block| {
                if !with_actions {
                    return None;
                }
                index
                    .as_deref()
                    .and_then(|index| stage_in_index(&patch, &diff, block, index))
                    .or_else(|| {
                        block_patch(&patch, &diff, block).map(|hunk| block_stage(repo, &hunk))
                    })
            })
            .collect();
        let syntax = diff_syntax(
            &diff,
            telaio_syntax::Language::detect(Path::new(repo_path)),
            HIGHLIGHT_MAX_BYTES,
        );
        Ok(Loaded {
            syntax: Arc::new(syntax),
            diff: Arc::new(diff),
            patch: patch.into(),
            longest,
            stages,
            additions,
            deletions,
        })
    }
}

/// [`block_stage`] worked out from the `index` text instead of git, `None` when
/// [`block_applies`] cannot tell.
fn stage_in_index(patch: &str, diff: &FileDiff, block: usize, index: &str) -> Option<BlockStage> {
    if block_applies(patch, diff, block, index, false)? {
        return Some(BlockStage::Unstaged);
    }
    Some(if block_applies(patch, diff, block, index, true)? {
        BlockStage::Staged
    } else {
        BlockStage::Mixed
    })
}

/// Whether `block` (from HEAD to the work tree) is in the index: it applies to the index forward
/// when it is not, backward when it is.
fn block_stage(repo: &Repo, block: &str) -> BlockStage {
    if repo
        .can_apply(block, ApplyTarget::Index, false)
        .unwrap_or(false)
    {
        BlockStage::Unstaged
    } else if repo
        .can_apply(block, ApplyTarget::Index, true)
        .unwrap_or(false)
    {
        BlockStage::Staged
    } else {
        BlockStage::Mixed
    }
}

/// The view of the diff tab.
pub(crate) struct DiffView {
    repo: Repo,
    /// The file, relative to the repository root, `/` separated.
    repo_path: String,
    /// The file, relative to the open folder: what the header shows and what Open file opens.
    path: String,
    /// Its status letter, as in the Git panel.
    letter: char,
    app: WeakEntity<AppView>,
    /// The last read, `None` while the first one runs.
    loaded: Option<Result<Loaded, String>>,
    task: Option<Task<()>>,
    scroll: UniformListScrollHandle,
    /// The sideways scroll of the lines' text, kept by an empty element as wide as the longest
    /// line, under the list.
    text_scroll: ScrollHandle,
    /// Where `F7`, `Shift+F7` and `F4` act.
    focus: FocusHandle,
    /// Focus the tab on the next render: a file was just opened in it.
    focus_pending: bool,
    /// The change block the counter, `F7` and `F4` start from.
    current: usize,
    /// Scroll to [`Self::current`] on the next render, where the layout is known.
    scroll_pending: bool,
    /// Land on the last block of the next file read (`Shift+F7` past the first block).
    land_last: bool,
    /// The "press again" hint while it shows: `true` for the next file, `false` for the previous.
    edge_hint: Option<bool>,
    /// Hides the hint after [`EDGE_HINT_TIME`], a one-shot timer.
    hint_task: Option<Task<()>>,
    /// The width of the diff area in the last frame, for what the header fits.
    area_width: Rc<Cell<f32>>,
}

impl DiffView {
    pub(crate) fn new(
        repo: Repo,
        repo_path: String,
        path: String,
        letter: char,
        app: WeakEntity<AppView>,
        cx: &mut Context<Self>,
    ) -> Self {
        let mut view = DiffView {
            repo,
            repo_path,
            path,
            letter,
            app,
            loaded: None,
            task: None,
            scroll: UniformListScrollHandle::new(),
            text_scroll: ScrollHandle::new(),
            focus: cx.focus_handle(),
            focus_pending: true,
            current: 0,
            scroll_pending: false,
            land_last: false,
            edge_hint: None,
            hint_task: None,
            area_width: Rc::new(Cell::new(f32::MAX)),
        };
        view.reload(cx);
        view
    }

    /// Where the diff's keys act, for the root to focus when the tab is shown again.
    pub(crate) fn focus_handle(&self) -> &FocusHandle {
        &self.focus
    }

    /// Makes the next file read land on its last block, for `Shift+F7` from the previous file.
    pub(crate) fn land_on_last(&mut self) {
        self.land_last = true;
    }

    fn block_count(&self) -> usize {
        match &self.loaded {
            Some(Ok(loaded)) => loaded.diff.blocks.len(),
            _ => 0,
        }
    }

    /// `F7` and `Shift+F7`: the next or previous block, or, pressed again at the last or first
    /// one, the next or previous changed file (Postino #224).
    fn step(&mut self, forward: bool, window: &mut Window, cx: &mut Context<Self>) {
        let count = self.block_count();
        let next = if forward {
            (self.current + 1 < count).then_some(self.current + 1)
        } else {
            self.current.checked_sub(1).filter(|_| count > 0)
        };
        if let Some(next) = next {
            self.current = next;
            self.scroll_pending = true;
            self.edge_hint = None;
            cx.notify();
            return;
        }
        if self.edge_hint == Some(forward) {
            self.edge_hint = None;
            let (app, repo_path) = (self.app.clone(), self.repo_path.clone());
            // Deferred: opening the file shows it in this same view.
            window.defer(cx, move |window, cx| {
                let _ = app.update(cx, |app, cx| {
                    app.open_neighbour_diff(&repo_path, forward, window, cx)
                });
            });
            cx.notify();
            return;
        }
        self.edge_hint = Some(forward);
        self.hint_task = Some(cx.spawn(async move |this, cx| {
            cx.background_executor().timer(EDGE_HINT_TIME).await;
            let _ = this.update(cx, |view, cx| {
                view.edge_hint = None;
                cx.notify();
            });
        }));
        cx.notify();
    }

    /// A click on a changed line makes its block the current one.
    fn select_row(&mut self, row: usize, cx: &mut Context<Self>) {
        let Some(Ok(loaded)) = &self.loaded else {
            return;
        };
        if let Some(block) = block_at_row(&loaded.diff, row) {
            self.current = block;
            self.edge_hint = None;
            cx.notify();
        }
    }

    /// `F4` and Open file: the file in the editor, at the line of the current change.
    fn open_file(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.letter == 'D' {
            return;
        }
        let line = match &self.loaded {
            Some(Ok(loaded)) => block_new_line(&loaded.diff, self.current),
            _ => None,
        };
        let (app, path) = (self.app.clone(), self.path.clone());
        window.defer(cx, move |window, cx| {
            let _ = app.update(cx, |app, cx| {
                app.open_changed_file_at(&path, line, window, cx)
            });
        });
    }

    /// The tab's name: the file name.
    pub(crate) fn title(&self) -> String {
        let name = self.path.rsplit('/').next().unwrap_or(&self.path);
        t!("git.diff.tab", name = name).into_owned()
    }

    /// Shows another file, keeping the tab.
    pub(crate) fn show(
        &mut self,
        repo: Repo,
        repo_path: String,
        path: String,
        letter: char,
        cx: &mut Context<Self>,
    ) {
        if self.repo_path != repo_path {
            self.loaded = None;
            self.scroll = UniformListScrollHandle::new();
            self.text_scroll.set_offset(point(px(0.0), px(0.0)));
        }
        self.focus_pending = true;
        self.edge_hint = None;
        (self.repo, self.repo_path, self.path, self.letter) = (repo, repo_path, path, letter);
        self.reload(cx);
    }

    /// Reads the patch again (after a status change). The rows stay until the new ones arrive.
    pub(crate) fn reload(&mut self, cx: &mut Context<Self>) {
        let repo = self.repo.clone();
        let repo_path = self.repo_path.clone();
        // A new file starts on its first block (or its last, from Shift+F7); a reload of the same
        // one keeps the current block.
        let fresh = self.loaded.is_none();
        self.task = Some(cx.spawn(async move |this, cx| {
            let result = cx
                .background_spawn(async move { Loaded::read(&repo, &repo_path) })
                .await;
            let _ = this.update(cx, |view, cx| {
                view.loaded = Some(result.map_err(|error| {
                    log::warn!("reading a diff failed: {error}");
                    error.to_string()
                }));
                let count = view.block_count();
                if fresh {
                    let last = std::mem::take(&mut view.land_last);
                    view.current = if last { count.saturating_sub(1) } else { 0 };
                    view.scroll_pending = count > 0;
                } else {
                    view.current = view.current.min(count.saturating_sub(1));
                }
                cx.notify();
            });
        }));
    }

    /// The patch of change block `index` and whether it is staged.
    fn block(&self, index: usize) -> Option<(Repo, String, Option<BlockStage>)> {
        let Some(Ok(loaded)) = &self.loaded else {
            return None;
        };
        let patch = block_patch(&loaded.patch, &loaded.diff, index)?;
        let stage = loaded.stages.get(index).copied().flatten();
        Some((self.repo.clone(), patch, stage))
    }

    /// Stages change block `index`, or unstages it when `reverse`.
    fn stage_block(&mut self, index: usize, reverse: bool, window: &mut Window, cx: &mut App) {
        let Some((repo, block, _)) = self.block(index) else {
            return;
        };
        let _ = self.app.update(cx, |app, cx| {
            app.run_git_write(window, cx, move || {
                repo.apply(&block, ApplyTarget::Index, reverse)
            });
        });
    }

    /// Puts change block `index` back as it was in the last commit, in the work tree and, when it
    /// is staged, in the index too. Asks first when the setting says so.
    fn revert_block(&mut self, index: usize, window: &mut Window, cx: &mut App) {
        let Some((repo, block, stage)) = self.block(index) else {
            return;
        };
        let target = if stage == Some(BlockStage::Staged) {
            ApplyTarget::Both
        } else {
            ApplyTarget::WorkTree
        };
        let app = self.app.clone();
        let confirm = app
            .upgrade()
            .is_some_and(|app| app.read(cx).settings.git_confirm_discard);
        let revert = move |window: &mut Window, cx: &mut App| {
            let (repo, block) = (repo.clone(), block.clone());
            let _ = app.update(cx, |app, cx| {
                app.run_git_write(window, cx, move || repo.apply(&block, target, true));
            });
        };
        if !confirm {
            revert(window, cx);
            return;
        }
        open_confirm_dialog(
            window,
            cx,
            ConfirmDialog {
                title: t!("git.diff.revert_title").into(),
                text: t!("git.diff.revert_text").into(),
                confirm_label: t!("git.diff.revert").into(),
                destructive: true,
                show_cancel: true,
                icon: None,
                on_confirm: Rc::new(revert),
            },
        );
    }

    fn render_header(&self, palette: &Palette, cx: &mut Context<Self>) -> impl IntoElement {
        let name = self
            .path
            .rsplit('/')
            .next()
            .unwrap_or(&self.path)
            .to_string();
        let folder = self
            .path
            .rsplit_once('/')
            .map(|(folder, _)| format!("{folder}/"))
            .unwrap_or_default();
        let summary = match &self.loaded {
            Some(Ok(loaded)) => {
                Some((loaded.additions, loaded.deletions, loaded.diff.blocks.len()))
            }
            _ => None,
        };
        let mono = cx.theme().mono_font_family.clone();
        let full = header_fit(self.area_width.get()) == HeaderFit::Full;
        let navigation = summary
            .filter(|(_, _, blocks)| *blocks > 0)
            .map(|(_, _, blocks)| self.render_navigation(blocks, full, palette, cx));
        let open_file = (self.letter != 'D').then(|| {
            let open = cx.listener(|view, _: &ClickEvent, window, cx| view.open_file(window, cx));
            if full {
                SecondaryButton::new("diff-open-file", t!("git.diff.open_file"))
                    .icon(Icon::new(IconName::FileText))
                    .key_hint("F4")
                    .on_click(open)
                    .into_any_element()
            } else {
                Button::new("diff-open-file")
                    .neutral_bordered(cx, palette.raised, palette.border_strong, palette.fg_muted)
                    .size(px(28.0))
                    .rounded(px(7.0))
                    .icon(Icon::new(IconName::FileText).size(px(13.0)))
                    .tooltip_with_action(
                        t!("git.diff.open_file"),
                        &OpenDiffFile,
                        Some(DIFF_KEY_CONTEXT),
                    )
                    .on_click(open)
                    .into_any_element()
            }
        });
        h_flex()
            .flex_none()
            .h(px(HEADER_HEIGHT))
            .px_3()
            .gap(px(10.0))
            .items_center()
            .border_b_1()
            .border_color(palette.border)
            .child(
                div()
                    .font_family(mono.clone())
                    .text_size(px(11.5))
                    .font_weight(FontWeight::MEDIUM)
                    .text_color(letter_color(self.letter, palette))
                    .child(self.letter.to_string()),
            )
            // The folder gives way first, then the name: nothing overlaps (Postino #289).
            .child(
                h_flex()
                    .min_w_0()
                    .flex_shrink(1.0)
                    .overflow_hidden()
                    .whitespace_nowrap()
                    .child(
                        div()
                            .min_w_0()
                            .flex_shrink(1.0)
                            .overflow_hidden()
                            .text_ellipsis()
                            .text_color(palette.fg_subtle)
                            .child(folder),
                    )
                    .child(
                        div()
                            .flex_shrink_0()
                            .overflow_hidden()
                            .text_ellipsis()
                            .child(name),
                    ),
            )
            .children(summary.map(|(additions, deletions, blocks)| {
                h_flex()
                    .gap(px(6.0))
                    .font_family(mono)
                    .text_size(px(11.5))
                    .child(
                        div()
                            .text_color(palette.success)
                            .child(format!("+{additions}")),
                    )
                    .child(
                        div()
                            .text_color(palette.danger)
                            .child(format!("\u{2212}{deletions}")),
                    )
                    .when(blocks == 0, |summary| {
                        summary.child(
                            div()
                                .text_color(palette.fg_subtle)
                                .child(plural("git.diff.blocks", blocks)),
                        )
                    })
                    .flex_none()
                    .whitespace_nowrap()
            }))
            .child(div().flex_1())
            .children(navigation)
            .children(open_file)
    }

    /// The counter and the Previous and Next buttons, in one bordered box, with the "press
    /// again" hint under them.
    fn render_navigation(
        &self,
        blocks: usize,
        full: bool,
        palette: &Palette,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let step = |id: &'static str,
                    icon: IconName,
                    label: Option<String>,
                    tooltip: String,
                    action: &dyn Action,
                    forward: bool,
                    cx: &mut Context<Self>| {
            Button::new(id)
                .neutral_ghost(cx, palette.fg_muted)
                .h(px(26.0))
                .px(px(if label.is_some() { 8.0 } else { 6.0 }))
                .rounded(px(0.0))
                .icon(Icon::new(icon).size(px(14.0)))
                .when_some(label, |button, label| button.label(label))
                .tooltip_with_action(tooltip, action, Some(DIFF_KEY_CONTEXT))
                .on_click(cx.listener(move |view, _, window, cx| view.step(forward, window, cx)))
        };
        let previous = step(
            "diff-previous",
            IconName::ChevronUp,
            full.then(|| t!("git.diff.previous").into_owned()),
            t!("git.diff.previous_tooltip").into_owned(),
            &PreviousDiffChange,
            false,
            cx,
        );
        let next = step(
            "diff-next",
            IconName::ChevronDown,
            full.then(|| t!("git.diff.next").into_owned()),
            t!("git.diff.next_tooltip").into_owned(),
            &NextDiffChange,
            true,
            cx,
        );
        let (current, total) = (self.current + 1, blocks);
        let counter = if full && total == 1 {
            t!("git.diff.position_one", current = current, total = total)
        } else if full {
            t!("git.diff.position", current = current, total = total)
        } else {
            t!("git.diff.position_short", current = current, total = total)
        };
        // Deferred, so it paints over the lines below the header.
        let hint = self.edge_hint.map(|forward| {
            let text = if forward {
                t!("git.diff.hint_next")
            } else {
                t!("git.diff.hint_previous")
            };
            deferred(
                div()
                    .absolute()
                    .top(px(34.0))
                    .right_0()
                    .px(px(8.0))
                    .py(px(5.0))
                    .rounded(px(6.0))
                    .bg(palette.raised)
                    .border_1()
                    .border_color(palette.border_strong)
                    .shadow_md()
                    .text_size(px(12.0))
                    .text_color(palette.fg)
                    .whitespace_nowrap()
                    .child(text.into_owned()),
            )
            .with_priority(1)
        });
        h_flex()
            .relative()
            .flex_none()
            .items_center()
            .gap(px(8.0))
            .child(
                div()
                    .text_size(px(12.0))
                    .text_color(palette.fg_subtle)
                    .whitespace_nowrap()
                    .child(counter.into_owned()),
            )
            .child(
                h_flex()
                    .h(px(28.0))
                    .items_center()
                    .rounded(px(7.0))
                    .border_1()
                    .border_color(palette.border_strong)
                    .bg(palette.raised)
                    .overflow_hidden()
                    .child(previous)
                    .child(div().w(px(1.0)).h_full().bg(palette.border_strong))
                    .child(next),
            )
            .children(hint)
            .into_any_element()
    }
}

impl Render for DiffView {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let palette = cx.palette();
        let body = match &self.loaded {
            None => div().flex_1().into_any_element(),
            Some(Err(error)) => div()
                .flex_1()
                .child(
                    EmptyState::new(t!("git.diff.error"))
                        .icon(Icon::new(IconName::CircleAlert))
                        .description(error.clone()),
                )
                .into_any_element(),
            Some(Ok(loaded)) if loaded.diff.binary => div()
                .flex_1()
                .child(EmptyState::new(t!("git.diff.binary")).icon(Icon::new(IconName::File)))
                .into_any_element(),
            Some(Ok(loaded)) if loaded.diff.lines.is_empty() => div()
                .flex_1()
                .child(
                    EmptyState::new(t!("git.diff.no_changes"))
                        .icon(Icon::new(IconName::CircleCheck)),
                )
                .into_any_element(),
            Some(Ok(loaded)) => {
                let count = loaded.diff.lines.len();
                let theme = cx.theme();
                let text_system = window.text_system();
                let char_width = text_system
                    .advance(
                        text_system.resolve_font(&font(theme.mono_font_family.clone())),
                        theme.mono_font_size,
                        '0',
                    )
                    .map_or(theme.mono_font_size * 0.6, |advance| advance.width);
                let text_width = char_width * loaded.longest as f32
                    + px(NUMBER_WIDTH * 2.0 + SIGN_WIDTH + END_PADDING);
                let mut lines =
                    uniform_list(
                        "diff-lines",
                        count,
                        cx.processor(|view, range: std::ops::Range<usize>, _window, cx| {
                            let palette = cx.palette();
                            let colors = SyntaxColors::new(&cx.theme().highlight_theme, &palette);
                            let Some(Ok(loaded)) = &view.loaded else {
                                return Vec::new();
                            };
                            let diff = loaded.diff.clone();
                            let stages = loaded.stages.clone();
                            let syntax = loaded.syntax.clone();
                            let current = diff.blocks.get(view.current).map(|block| {
                                block.first_line..block.first_line + block.removed + block.added
                            });
                            range
                                .filter_map(|index| diff.lines.get(index).map(|line| (index, line)))
                                .map(|(index, line)| {
                                    let block = diff
                                        .blocks
                                        .iter()
                                        .position(|block| block.first_line == index);
                                    let actions = block.and_then(|block| {
                                        stages
                                            .get(block)
                                            .copied()
                                            .flatten()
                                            .map(|stage| render_block_actions(block, stage, cx))
                                    });
                                    let scroll_x = view.text_scroll.offset().x;
                                    let spans = syntax.get(index).map_or(&[][..], Vec::as_slice);
                                    let style = LineStyle {
                                        palette: &palette,
                                        colors: &colors,
                                        current: current
                                            .as_ref()
                                            .is_some_and(|rows| rows.contains(&index)),
                                    };
                                    div()
                                        .id(("diff-row", index))
                                        .on_click(cx.listener(move |view, _, _, cx| {
                                            view.select_row(index, cx)
                                        }))
                                        .child(render_line(
                                            index, line, spans, actions, scroll_x, style, cx,
                                        ))
                                        .into_any_element()
                                })
                                .collect()
                        }),
                    )
                    .track_scroll(&self.scroll)
                    .size_full();
                // A sideways wheel moves the text, not the lines.
                lines.style().restrict_scroll_to_axis = Some(true);
                if std::mem::take(&mut self.scroll_pending)
                    && let Some(block) = loaded.diff.blocks.get(self.current)
                {
                    self.scroll
                        .scroll_to_item(block.first_line, ScrollStrategy::Center);
                }
                div()
                    .relative()
                    .flex_1()
                    .min_h_0()
                    .child(
                        div()
                            .id("diff-text-scroll")
                            .absolute()
                            .inset_0()
                            .overflow_x_scroll()
                            .restrict_scroll_to_axis()
                            .track_scroll(&self.text_scroll)
                            .child(div().h_full().w(text_width)),
                    )
                    .child(lines)
                    .child(scrollbar_overlay(
                        &self.scroll,
                        ScrollbarAxis::Vertical,
                        "diff-vertical-scrollbar",
                        &palette,
                    ))
                    .child(scrollbar_overlay(
                        &self.text_scroll,
                        ScrollbarAxis::Horizontal,
                        "diff-horizontal-scrollbar",
                        &palette,
                    ))
                    .into_any_element()
            }
        };
        if std::mem::take(&mut self.focus_pending) {
            self.focus.focus(window, cx);
        }
        // The area's width, for the header of the next frame; a change renders again.
        let width = self.area_width.clone();
        let id = cx.entity_id();
        let measure = canvas(
            move |bounds, _, cx| {
                let now = f32::from(bounds.size.width);
                if (width.get() - now).abs() > 0.5 {
                    width.set(now);
                    cx.notify(id);
                }
            },
            |_, _, _, _| {},
        )
        .absolute()
        .inset_0();
        v_flex()
            .id("diff-view")
            .relative()
            .size_full()
            .bg(palette.bg)
            .track_focus(&self.focus)
            .key_context(DIFF_KEY_CONTEXT)
            .on_action(
                cx.listener(|view, _: &NextDiffChange, window, cx| view.step(true, window, cx)),
            )
            .on_action(
                cx.listener(|view, _: &PreviousDiffChange, window, cx| {
                    view.step(false, window, cx)
                }),
            )
            .on_action(cx.listener(|view, _: &OpenDiffFile, window, cx| view.open_file(window, cx)))
            .child(measure)
            .child(self.render_header(&palette, cx))
            .child(body)
    }
}

/// Stage or Unstage, and Revert, for change block `block`.
fn render_block_actions(block: usize, stage: BlockStage, cx: &mut Context<DiffView>) -> AnyElement {
    let stage_button = match stage {
        BlockStage::Unstaged => Some((t!("git.diff.stage"), false)),
        BlockStage::Staged => Some((t!("git.diff.unstage"), true)),
        BlockStage::Mixed => None,
    };
    h_flex()
        .gap(px(4.0))
        .children(stage_button.map(|(label, reverse)| {
            GhostButton::new(("diff-stage", block), label).on_click(
                cx.listener(move |view, _, window, cx| {
                    view.stage_block(block, reverse, window, cx)
                }),
            )
        }))
        .child(
            GhostButton::new(("diff-revert", block), t!("git.diff.revert")).on_click(
                cx.listener(move |view, _, window, cx| view.revert_block(block, window, cx)),
            ),
        )
        .into_any_element()
}

/// One line of the diff: the old and new line numbers, the sign, the text with its changed words
/// emphasized, and the block's buttons on its first line.
/// The colors a line is drawn with, looked up once per frame.
#[derive(Clone, Copy)]
struct LineStyle<'a> {
    palette: &'a Palette,
    colors: &'a SyntaxColors,
    /// Whether the line is in the current block: an accent bar on its left edge.
    current: bool,
}

fn render_line(
    index: usize,
    line: &telaio_git::DiffLine,
    syntax: &[(std::ops::Range<usize>, telaio_syntax::Highlight)],
    actions: Option<AnyElement>,
    scroll_x: Pixels,
    style: LineStyle,
    cx: &App,
) -> AnyElement {
    let palette = style.palette;
    let (tint, emphasis, sign, sign_color) = match line.kind {
        LineKind::Added => (
            Some(palette.success.opacity(0.10)),
            palette.success.opacity(0.28),
            "+",
            palette.success,
        ),
        LineKind::Removed => (
            Some(palette.danger.opacity(0.10)),
            palette.danger.opacity(0.28),
            "\u{2212}",
            palette.danger,
        ),
        LineKind::Context => (None, palette.hover, " ", palette.fg_subtle),
    };
    let theme = cx.theme();
    let mono = theme.mono_font_family.clone();
    let size = theme.mono_font_size;
    let number = |number: Option<u32>| {
        div()
            .flex_none()
            .w(px(NUMBER_WIDTH))
            .pr(px(10.0))
            .text_right()
            .text_color(palette.fg_subtle)
            .child(number.map(|n| n.to_string()).unwrap_or_default())
    };
    // Tabs are drawn as four spaces; `segments` moves the ranges to match.
    let text: SharedString = expand_tabs(&line.text).into();
    let highlights: Vec<_> = segments(&line.text, syntax, &line.emphasis)
        .into_iter()
        .map(|segment| {
            (
                segment.range,
                HighlightStyle {
                    color: segment
                        .highlight
                        .and_then(|highlight| style.colors.get(highlight)),
                    background_color: segment.emphasis.then_some(emphasis),
                    ..Default::default()
                },
            )
        })
        .collect();
    h_flex()
        .id(("diff-line", index))
        .w_full()
        .h(px(f32::from(size) * 1.6))
        .items_center()
        .font_family(mono)
        .text_size(size)
        .when_some(tint, |row, tint| row.bg(tint))
        .relative()
        .when(style.current, |row| {
            row.child(
                div()
                    .absolute()
                    .left_0()
                    .top_0()
                    .bottom_0()
                    .w(px(2.0))
                    .bg(palette.accent),
            )
        })
        .child(number(line.old_number))
        .child(number(line.new_number))
        .child(
            div()
                .flex_none()
                .w(px(SIGN_WIDTH))
                .text_color(sign_color)
                .child(sign),
        )
        .child(
            div()
                .flex_1()
                .min_w_0()
                .overflow_hidden()
                .whitespace_nowrap()
                .child(
                    div()
                        .ml(scroll_x)
                        .child(StyledText::new(text).with_highlights(highlights)),
                ),
        )
        .children(actions.map(|actions| {
            div()
                .flex_none()
                .pr_2()
                .font_family(theme.font_family.clone())
                .text_size(theme.font_size)
                .child(actions)
        }))
        .into_any_element()
}
