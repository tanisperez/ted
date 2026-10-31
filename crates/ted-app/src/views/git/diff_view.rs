//! The diff tab: what changed in one file, from the last commit to the file on disk, as one
//! column with the line numbers of both sides, the added lines in green and the removed ones in
//! red, the changed words emphasized. Each block of changes has Stage (or Unstage) and Revert. The
//! patch is read with the whole file as context and parsed (`telaio_git::parse_file_diff`) on the
//! background executor; the rows are a `uniform_list`, so a long file costs only the rows on
//! screen. There is one diff tab: opening another file's diff reuses it.

use std::rc::Rc;
use std::sync::Arc;

use gpui_kit::assets::IconName;
use gpui_kit::component::*;
use gpui_kit::prelude::*;
use gpui_kit::*;
use rust_i18n::t;
use telaio_git::{
    ApplyTarget, DiffSource, FileDiff, GitError, LineKind, Repo, block_patch, parse_file_diff,
};
use telaio_ui::components::{
    ConfirmDialog, EmptyState, GhostButton, SecondaryButton, open_confirm_dialog,
};

use crate::state::locale::plural;
use crate::theme::{Palette, PaletteExt};

use super::super::root::AppView;
use super::letter_color;

/// Above this many change blocks the buttons are left out: checking each block against the index
/// costs one git run.
const MAX_BLOCK_ACTIONS: usize = 200;
/// Width of each line number column.
const NUMBER_WIDTH: f32 = 52.0;
/// Height of the header.
const HEADER_HEIGHT: f32 = 40.0;

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
    /// Per change block, whether it is staged; `None` without buttons.
    stages: Vec<Option<BlockStage>>,
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
        let with_actions = diff.blocks.len() <= MAX_BLOCK_ACTIONS;
        let stages = (0..diff.blocks.len())
            .map(|index| {
                with_actions
                    .then(|| block_patch(&patch, &diff, index))
                    .flatten()
                    .map(|block| block_stage(repo, &block))
            })
            .collect();
        Ok(Loaded {
            diff: Arc::new(diff),
            patch: patch.into(),
            stages,
            additions,
            deletions,
        })
    }
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
        };
        view.reload(cx);
        view
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
        }
        (self.repo, self.repo_path, self.path, self.letter) = (repo, repo_path, path, letter);
        self.reload(cx);
    }

    /// Reads the patch again (after a status change). The rows stay until the new ones arrive.
    pub(crate) fn reload(&mut self, cx: &mut Context<Self>) {
        let repo = self.repo.clone();
        let repo_path = self.repo_path.clone();
        self.task = Some(cx.spawn(async move |this, cx| {
            let result = cx
                .background_spawn(async move { Loaded::read(&repo, &repo_path) })
                .await;
            let _ = this.update(cx, |view, cx| {
                view.loaded = Some(result.map_err(|error| {
                    log::warn!("reading a diff failed: {error}");
                    error.to_string()
                }));
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
        let path = self.path.clone();
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
            .child(
                h_flex()
                    .min_w_0()
                    .child(div().text_color(palette.fg_subtle).child(folder))
                    .child(div().child(name)),
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
                    .child(
                        div()
                            .text_color(palette.fg_subtle)
                            .child(plural("git.diff.blocks", blocks)),
                    )
            }))
            .child(div().flex_1())
            .child(
                SecondaryButton::new("diff-open-file", t!("git.diff.open_file"))
                    .icon(Icon::new(IconName::FileText))
                    .on_click(cx.listener(move |view, _, window, cx| {
                        let path = path.clone();
                        let _ = view.app.update(cx, |app, cx| {
                            app.open_changed_file(&path, window, cx);
                        });
                    })),
            )
    }
}

impl Render for DiffView {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
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
                uniform_list(
                    "diff-lines",
                    count,
                    cx.processor(|view, range: std::ops::Range<usize>, _window, cx| {
                        let palette = cx.palette();
                        let Some(Ok(loaded)) = &view.loaded else {
                            return Vec::new();
                        };
                        let diff = loaded.diff.clone();
                        let stages = loaded.stages.clone();
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
                                render_line(index, line, actions, &palette, cx)
                            })
                            .collect()
                    }),
                )
                .track_scroll(&self.scroll)
                .flex_1()
                .into_any_element()
            }
        };
        v_flex()
            .id("diff-view")
            .size_full()
            .bg(palette.bg)
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
fn render_line(
    index: usize,
    line: &telaio_git::DiffLine,
    actions: Option<AnyElement>,
    palette: &Palette,
    cx: &App,
) -> AnyElement {
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
    // Tabs are drawn as four spaces; the emphasis ranges are byte offsets of the original text,
    // so a line with tabs is drawn without them.
    let text: SharedString = line.text.replace('\t', "    ").into();
    let highlights: Vec<_> = if line.text.contains('\t') {
        Vec::new()
    } else {
        line.emphasis
            .iter()
            .filter(|range| range.end <= text.len())
            .map(|range| {
                (
                    range.clone(),
                    HighlightStyle {
                        background_color: Some(emphasis),
                        ..Default::default()
                    },
                )
            })
            .collect()
    };
    h_flex()
        .id(("diff-line", index))
        .w_full()
        .h(px(f32::from(size) * 1.6))
        .items_center()
        .font_family(mono)
        .text_size(size)
        .when_some(tint, |row, tint| row.bg(tint))
        .child(number(line.old_number))
        .child(number(line.new_number))
        .child(
            div()
                .flex_none()
                .w(px(16.0))
                .text_color(sign_color)
                .child(sign),
        )
        .child(
            div()
                .flex_1()
                .min_w_0()
                .overflow_hidden()
                .whitespace_nowrap()
                .child(StyledText::new(text).with_highlights(highlights)),
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
