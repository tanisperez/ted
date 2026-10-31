//! The Git panel, shown in place of the explorer (design v2, ported from Postino's Source Control
//! panel): the folder's changed files as a tree with checkboxes that mirror the git index, and the
//! empty states (git missing, not a repository, no changes). The rows come from
//! `state::git::change_rows`, rebuilt by [`AppView::rebuild_git_rows`] when the status changes,
//! never in render. They are a `uniform_list`, so thousands of untracked files cost only the rows
//! on screen.

use std::rc::Rc;

use gpui_kit::assets::IconName;
use gpui_kit::component::*;
use gpui_kit::prelude::*;
use gpui_kit::*;
use rust_i18n::t;
use telaio_ui::components::{
    CheckState, ConfirmDialog, EmptyState, IconButton, PrimaryButton, SecondaryButton,
    SectionLabel, TextArea, TreeRow, TriCheckbox, open_confirm_dialog,
};

use crate::actions::CommitFromMessage;
use crate::state::git_sync::{can_commit, can_write_message};
use crate::state::settings::GitCommitAction;

use crate::state::git::{ChangeGroup, ChangeRow, ChangeRowKind, Check, GitAvailability, RepoState};
use crate::state::locale::plural;
use crate::state::settings::SettingsCategory;
use crate::theme::metrics::{EXPLORER_HEADER_HEIGHT, EXPLORER_LIST_PADDING};
use crate::theme::{Palette, PaletteExt};

use super::super::root::AppView;
use super::super::settings_git::INSTALL_GIT_URL;

/// What a click on a row's checkbox does.
#[derive(Clone, Copy)]
pub(crate) enum IndexAction {
    Stage,
    Unstage,
}

impl AppView {
    /// Folds or unfolds a group or folder of the panel.
    fn toggle_git_row(&mut self, key: String, cx: &mut Context<Self>) {
        if !self.git.collapsed.remove(&key) {
            self.git.collapsed.insert(key);
        }
        self.rebuild_git_rows(cx);
        cx.notify();
    }

    /// Stages or unstages `paths` (repository relative) on the background executor, then reads
    /// the status again.
    pub(crate) fn change_index(
        &mut self,
        action: IndexAction,
        paths: Vec<String>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let Some(repo) = self.git.repo().cloned() else {
            return;
        };
        if paths.is_empty() {
            return;
        }
        self.run_git_write(window, cx, move || match action {
            IndexAction::Stage => repo.stage(&paths),
            IndexAction::Unstage => repo.unstage(&paths),
        });
    }

    /// Discards the changes of `paths`, after a confirmation when the setting asks for one.
    pub(crate) fn discard_paths(
        &mut self,
        label: String,
        paths: Vec<String>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let Some(repo) = self.git.repo().cloned() else {
            return;
        };
        // An empty path would cover the whole repository.
        if paths.is_empty() || paths.iter().any(|path| path.is_empty()) {
            return;
        }
        if !self.settings.git_confirm_discard {
            self.run_git_write(window, cx, move || repo.discard(&paths));
            return;
        }
        let weak = cx.weak_entity();
        open_confirm_dialog(
            window,
            cx,
            ConfirmDialog {
                title: t!("git.discard.title").into(),
                text: t!("git.discard.text", name = label).into(),
                confirm_label: t!("git.discard.confirm").into(),
                destructive: true,
                show_cancel: true,
                on_confirm: Rc::new(move |window, cx| {
                    let repo = repo.clone();
                    let paths = paths.clone();
                    let _ = weak.update(cx, |view, cx| {
                        view.run_git_write(window, cx, move || repo.discard(&paths));
                    });
                }),
            },
        );
    }

    /// Opens Settings on its Git category.
    pub(crate) fn open_git_settings(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.settings_category = SettingsCategory::Git;
        self.open_settings(window, cx);
    }

    /// The header's buttons: refresh, discard every change, list or tree, and the history.
    fn render_git_toolbar(&self, cx: &mut Context<Self>) -> AnyElement {
        let all_paths: Vec<String> = self
            .git
            .rows
            .iter()
            .filter(|row| matches!(row.kind, ChangeRowKind::Group(_)))
            .flat_map(|row| row.repo_paths.iter().cloned())
            .collect();
        let (grouping_icon, grouping_tooltip) = if self.git.flat {
            (IconName::ListTree, t!("git.panel.group_by_folder"))
        } else {
            (IconName::List, t!("git.panel.show_as_list"))
        };
        h_flex()
            .gap(px(2.0))
            .child(
                IconButton::new("git-refresh", IconName::RefreshCw)
                    .tooltip(t!("git.panel.refresh"))
                    .on_click(cx.listener(|view, _, _, cx| view.refresh_git_status(cx))),
            )
            .child(
                IconButton::new("git-discard-all", IconName::Undo2)
                    .tooltip(t!("git.panel.discard_all"))
                    .disabled(all_paths.is_empty())
                    .on_click(cx.listener(move |view, _, window, cx| {
                        let label = t!("git.panel.all_changes").into_owned();
                        view.discard_paths(label, all_paths.clone(), window, cx);
                    })),
            )
            .child(
                IconButton::new("git-grouping", grouping_icon)
                    .tooltip(grouping_tooltip)
                    .on_click(cx.listener(|view, _, _, cx| {
                        view.git.flat = !view.git.flat;
                        view.rebuild_git_rows(cx);
                        cx.notify();
                    })),
            )
            .child(
                IconButton::new("git-history", IconName::GitGraph)
                    .tooltip(t!("git.panel.history"))
                    .on_click(cx.listener(|view, _, window, cx| view.open_git_history(window, cx))),
            )
            .into_any_element()
    }

    /// Renders the Git panel.
    pub(crate) fn render_git_panel(&mut self, cx: &mut Context<Self>) -> AnyElement {
        let palette = cx.palette();
        let header = h_flex()
            .h(px(EXPLORER_HEADER_HEIGHT))
            .flex_none()
            .px_3()
            .items_center()
            .justify_between()
            .child(SectionLabel::new(t!("git.panel.title")))
            .children(self.git.repo().map(|_| self.render_git_toolbar(cx)));
        v_flex()
            .id("git-panel")
            .size_full()
            .bg(palette.surface)
            .child(header)
            .child(self.render_git_body(&palette, cx))
            .children(self.render_commit_box(&palette, cx))
            .into_any_element()
    }

    fn render_git_body(&self, palette: &Palette, cx: &mut Context<Self>) -> AnyElement {
        match &self.git.availability {
            GitAvailability::Detecting => return div().flex_1().into_any_element(),
            GitAvailability::Missing(_) => return render_git_missing(palette, cx),
            GitAvailability::Ready(_) => {}
        }
        match &self.git.repo {
            RepoState::Unknown | RepoState::Repo { status: None, .. } => {
                div().flex_1().into_any_element()
            }
            RepoState::NotARepo => panel_empty(
                EmptyState::new(t!("git.empty.not_a_repo"))
                    .icon(Icon::new(IconName::GitBranch))
                    .description(t!("git.empty.not_a_repo_text"))
                    .action(
                        PrimaryButton::new("git-init", t!("git.empty.init"))
                            .icon(Icon::new(IconName::Plus))
                            .on_click(cx.listener(|view, _, window, cx| {
                                view.init_git_repository(window, cx)
                            })),
                    ),
            ),
            RepoState::Error(message) => panel_empty(
                EmptyState::new(t!("git.empty.error"))
                    .icon(Icon::new(IconName::CircleAlert))
                    .description(message.clone()),
            ),
            RepoState::Repo {
                status: Some(status),
                ..
            } => {
                if self.git.rows.is_empty() {
                    let description = if status.ahead > 0 {
                        plural("git.empty.ahead", status.ahead as usize)
                    } else {
                        t!("git.empty.clean").into_owned()
                    };
                    return v_flex()
                        .flex_1()
                        .min_h_0()
                        .children(self.render_outside_notice(palette, cx))
                        .child(panel_empty(
                            EmptyState::new(t!("git.empty.no_changes"))
                                .icon(Icon::new(IconName::CircleCheck))
                                .description(description),
                        ))
                        .into_any_element();
                }
                let rows = uniform_list(
                    "git-panel-rows",
                    self.git.rows.len(),
                    cx.processor(|view, range: std::ops::Range<usize>, _window, cx| {
                        let palette = cx.palette();
                        let rows = view.git.rows.clone();
                        range
                            .filter_map(|index| rows.get(index).map(|row| (index, row)))
                            .map(|(index, row)| render_change_row(index, row, &palette, cx))
                            .collect()
                    }),
                )
                .track_scroll(&self.git_scroll)
                .flex_1()
                .px(px(EXPLORER_LIST_PADDING))
                .pb_2();
                v_flex()
                    .flex_1()
                    .min_h_0()
                    .children(self.render_outside_notice(palette, cx))
                    .child(rows)
                    .into_any_element()
            }
        }
    }

    /// The notice over the rows when files outside the folder are staged: they would go into the
    /// next commit although the panel does not list them.
    fn render_outside_notice(&self, palette: &Palette, cx: &mut Context<Self>) -> Option<Div> {
        let outside = self.git.staged_outside.clone();
        if outside.is_empty() {
            return None;
        }
        Some(
            v_flex()
                .flex_none()
                .mx(px(8.0))
                .mb(px(6.0))
                .p(px(8.0))
                .gap(px(6.0))
                .rounded(px(6.0))
                .bg(palette.warning.opacity(0.10))
                .text_size(px(12.0))
                .text_color(palette.fg_muted)
                .child(plural("git.panel.staged_outside", outside.len()))
                .child(
                    div().child(
                        SecondaryButton::new("git-unstage-outside", t!("git.panel.unstage_them"))
                            .on_click(cx.listener(move |view, _, window, cx| {
                                let paths = outside.to_vec();
                                view.change_index(IndexAction::Unstage, paths, window, cx);
                            })),
                    ),
                ),
        )
    }
}

impl AppView {
    /// The commit box under the rows: the message, Amend, the summary line and the two commit
    /// buttons.
    fn render_commit_box(&self, palette: &Palette, cx: &mut Context<Self>) -> Option<AnyElement> {
        self.git.status()?;
        let running = self.git.sync.running.is_some();
        let stats = self.git.commit.stats;
        let amend = self.git.commit.amend;
        let message = self.commit_input.read(cx).value();
        let enabled = can_commit(&message, stats, amend, running);

        let message_box = div()
            .key_context("CommitMessage")
            .on_action(cx.listener(|view, _: &CommitFromMessage, window, cx| {
                let push = view.settings.git_commit_action == GitCommitAction::CommitAndPush;
                view.commit_from_box(push, window, cx);
            }))
            .child(TextArea::new(&self.commit_input).disabled(!can_write_message(stats, amend)));
        let amend_box = TriCheckbox::new(
            "git-amend",
            if amend {
                CheckState::Checked
            } else {
                CheckState::Unchecked
            },
        )
        .label(t!("git.commit.amend"))
        .disabled(running)
        .on_click(cx.listener(|view, _, window, cx| view.toggle_amend(window, cx)));
        let mono = cx.theme().mono_font_family.clone();
        let summary = h_flex()
            .gap(px(4.0))
            .font_family(mono)
            .text_size(px(11.5))
            .text_color(palette.fg_subtle)
            .child(format!(
                "{} \u{b7}",
                plural("git.commit.files", stats.files)
            ))
            .child(div().text_color(palette.success).child(format!(
                "+{}",
                telaio_app::number::format_integer(stats.additions)
            )))
            .child(div().text_color(palette.danger).child(format!(
                "\u{2212}{}",
                telaio_app::number::format_integer(stats.deletions)
            )));
        let commit = PrimaryButton::new("git-commit", t!("git.commit.commit"))
            .full_width()
            .disabled(!enabled)
            .on_click(cx.listener(|view, _, window, cx| view.commit_from_box(false, window, cx)));
        let commit_and_push =
            SecondaryButton::new("git-commit-push", t!("git.commit.commit_and_push"))
                .disabled(!enabled)
                .on_click(
                    cx.listener(|view, _, window, cx| view.commit_from_box(true, window, cx)),
                );
        Some(
            v_flex()
                .flex_none()
                .gap(px(8.0))
                .p(px(10.0))
                .border_t_1()
                .border_color(palette.border)
                .child(message_box)
                .child(
                    h_flex()
                        .flex_wrap()
                        .items_center()
                        .gap_x(px(14.0))
                        .gap_y(px(4.0))
                        .text_size(px(12.0))
                        .text_color(palette.fg_muted)
                        .child(div().flex_1().child(amend_box))
                        .child(summary),
                )
                .child(
                    h_flex()
                        .gap(px(6.0))
                        .child(div().flex_1().min_w_0().child(commit))
                        .child(div().flex_none().child(commit_and_push)),
                )
                .into_any_element(),
        )
    }
}

/// Git is not installed, too old or failed to run.
fn render_git_missing(palette: &Palette, cx: &mut Context<AppView>) -> AnyElement {
    panel_empty(
        EmptyState::new(t!("git.empty.missing"))
            .icon(Icon::new(IconName::CircleAlert))
            .description(t!("git.empty.missing_text"))
            .action(
                v_flex()
                    .items_center()
                    .gap_2()
                    .child(
                        SecondaryButton::new("git-open-settings", t!("git.empty.open_settings"))
                            .icon(Icon::new(IconName::Settings))
                            .on_click(cx.listener(|view, _, window, cx| {
                                view.open_git_settings(window, cx)
                            })),
                    )
                    .child(
                        div()
                            .id("git-install-link")
                            .text_size(px(12.0))
                            .text_color(palette.accent_text)
                            .cursor_pointer()
                            .hover(|style| style.underline())
                            .child(t!("settings.git.install"))
                            .on_click(|_, _, cx| cx.open_url(INSTALL_GIT_URL)),
                    ),
            ),
    )
}

/// An empty state laid out inside the panel, with side padding so its text wraps.
fn panel_empty(state: EmptyState) -> AnyElement {
    div()
        .flex_1()
        .min_h_0()
        .px(px(16.0))
        .child(state)
        .into_any_element()
}

/// The status letter's color: modified amber, added and untracked green, deleted and conflicts
/// red, renamed blue. Shared with the explorer.
pub(crate) fn letter_color(letter: char, palette: &Palette) -> Hsla {
    match letter {
        'A' | 'U' => palette.success,
        'D' | '!' => palette.danger,
        'R' => palette.info,
        _ => palette.warning,
    }
}

/// The checkbox state of a [`Check`].
fn check_state(check: Check) -> CheckState {
    match check {
        Check::On => CheckState::Checked,
        Check::Off => CheckState::Unchecked,
        Check::Partial => CheckState::Mixed,
    }
}

/// One row of the panel: a group header, a folder or a file.
fn render_change_row(
    index: usize,
    row: &ChangeRow,
    palette: &Palette,
    cx: &mut Context<AppView>,
) -> AnyElement {
    let check = row.check;
    let paths = row.repo_paths.clone();
    let checkbox = {
        let paths = paths.clone();
        TriCheckbox::new(("git-check", index), check_state(check)).on_click(cx.listener(
            move |view, _, window, cx| {
                cx.stop_propagation();
                let action = match check {
                    Check::On => IndexAction::Unstage,
                    Check::Off | Check::Partial => IndexAction::Stage,
                };
                view.change_index(action, paths.clone(), window, cx);
            },
        ))
    };
    let discard_label = match &row.kind {
        ChangeRowKind::Group(_) => t!("git.panel.all_changes").into_owned(),
        _ => row.path.clone(),
    };
    let discard = IconButton::new(("git-discard", index), IconName::Undo2)
        .box_size(22.0)
        .tooltip(t!("git.panel.discard"))
        .on_click(cx.listener(move |view, _, window, cx| {
            cx.stop_propagation();
            view.discard_paths(discard_label.clone(), paths.clone(), window, cx);
        }));
    let mono = cx.theme().mono_font_family.clone();
    let count = |count: usize| {
        div()
            .font_family(mono.clone())
            .text_size(px(11.0))
            .text_color(palette.fg_subtle)
            .child(telaio_app::number::format_integer(count as u64))
    };

    match &row.kind {
        ChangeRowKind::Group(group) => {
            let label = match group {
                ChangeGroup::Changes => t!("git.panel.changes"),
                ChangeGroup::Unversioned => t!("git.panel.unversioned"),
            };
            let key = row.key.clone();
            TreeRow::new(("git-row", index), label)
                .folder(row.expanded)
                .leading(checkbox)
                .trailing(discard)
                .suffix(count(row.count))
                .on_click(cx.listener(move |view, _, _, cx| view.toggle_git_row(key.clone(), cx)))
                .into_any_element()
        }
        ChangeRowKind::Folder => {
            let key = row.key.clone();
            TreeRow::new(("git-row", index), row.label.clone())
                .depth(row.depth)
                .folder(row.expanded)
                .leading(
                    h_flex().gap(px(6.0)).items_center().child(checkbox).child(
                        Icon::new(IconName::Folder)
                            .small()
                            .text_color(palette.fg_subtle),
                    ),
                )
                .trailing(discard)
                .suffix(count(row.count))
                .on_click(cx.listener(move |view, _, _, cx| view.toggle_git_row(key.clone(), cx)))
                .into_any_element()
        }
        ChangeRowKind::File { letter, deleted } => {
            let letter_element = div()
                .w(px(12.0))
                .font_family(mono.clone())
                .text_size(px(11.5))
                .font_weight(FontWeight::MEDIUM)
                .text_color(letter_color(*letter, palette))
                .child(letter.to_string());
            let leading = h_flex().gap(px(6.0)).items_center().child(checkbox).child(
                Icon::new(IconName::File)
                    .small()
                    .text_color(palette.fg_subtle),
            );
            let path = row.path.clone();
            let repo_path = row.repo_paths.first().cloned().unwrap_or_default();
            let (letter, deleted) = (*letter, *deleted);
            // A flat list (depth 1) names the folder each file is in.
            let folder = (row.depth == 1)
                .then(|| {
                    row.path
                        .rsplit_once('/')
                        .map(|(folder, _)| folder.to_string())
                })
                .flatten();
            TreeRow::new(("git-row", index), row.label.clone())
                .depth(row.depth)
                .when_some(folder, |row, folder| row.detail(folder))
                .strikethrough(deleted)
                .leading(leading)
                .trailing(discard)
                .suffix(letter_element)
                .on_click(cx.listener(move |view, _, window, cx| {
                    view.open_git_diff(repo_path.clone(), path.clone(), letter, window, cx);
                }))
                .into_any_element()
        }
    }
}
