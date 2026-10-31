//! The "Git" category of the Settings modal, as the design draws it and ported from Postino's
//! pane: the git executable (the system's or one picked by hand) with what was detected, the
//! automatic fetch, the confirmation before discarding and what Ctrl+Enter does in the commit
//! message. Changing the executable detects git again.

use std::path::{Path, PathBuf};

use gpui_kit::assets::IconName;
use gpui_kit::component::switch::Switch;
use gpui_kit::component::*;
use gpui_kit::prelude::*;
use gpui_kit::*;
use rust_i18n::t;
use telaio_git::{DetectError, MIN_VERSION};
use telaio_ui::components::{
    GhostButton, LabeledRow, SearchableSelect, SecondaryButton, SectionHeading, SegmentedControl,
    SegmentedItem, SelectHandle, SelectOption, on_pick, pick, select_state, set_options,
    switch_halo,
};

use crate::state::git::GitAvailability;
use crate::state::settings::{GitCommitAction, Settings};
use crate::theme::Palette;

use super::root::AppView;

/// Where git is downloaded from, linked when it is missing.
pub(crate) const INSTALL_GIT_URL: &str = "https://git-scm.com/downloads";

/// The option id of the system's git; any other id is the path itself.
const GIT_AUTO_ID: &str = "";

/// Width of the executable select.
const EXECUTABLE_SELECT_WIDTH: f32 = 220.0;

/// "System", plus the picked executable when there is one, so the box shows it.
fn executable_options(path: Option<&Path>) -> Vec<SelectOption> {
    let mut options = vec![SelectOption::new(GIT_AUTO_ID, t!("settings.git.auto"))];
    if let Some(path) = path {
        let text = path.to_string_lossy().into_owned();
        options.push(SelectOption::new(text.clone(), text));
    }
    options
}

fn executable_id(path: Option<&Path>) -> String {
    path.map(|path| path.to_string_lossy().into_owned())
        .unwrap_or_else(|| GIT_AUTO_ID.to_string())
}

/// "2.30", the oldest git version, for the messages.
fn min_version() -> String {
    format!("{}.{}", MIN_VERSION.major, MIN_VERSION.minor)
}

impl AppView {
    /// Creates the executable select when the modal opens.
    pub(crate) fn create_git_select(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let path = self.settings.git_executable.as_deref();
        let select = select_state(
            executable_options(path),
            Some(&executable_id(path)),
            false,
            window,
            cx,
        );
        let subscription = on_pick(&select, window, cx, |view, id, window, cx| {
            let path = (id != GIT_AUTO_ID).then(|| PathBuf::from(id));
            view.set_git_executable(path, window, cx);
        });
        self.git_select = Some((select, subscription));
    }

    /// Picks the git executable (`None` for the system's) and detects git again.
    pub(crate) fn set_git_executable(
        &mut self,
        path: Option<PathBuf>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if self.settings.git_executable == path {
            return;
        }
        if let Some((select, _)) = &self.git_select {
            set_options(
                select,
                executable_options(path.as_deref()),
                false,
                window,
                cx,
            );
            pick(select, &executable_id(path.as_deref()), window, cx);
        }
        self.settings.git_executable = path;
        self.apply_settings(window, cx);
        self.detect_git(cx);
    }
}

/// The category's sections.
pub(crate) fn git_sections(
    weak: WeakEntity<AppView>,
    palette: &Palette,
    settings: &Settings,
    availability: &GitAvailability,
    select: Option<&SelectHandle>,
) -> Vec<AnyElement> {
    vec![
        executable_section(weak.clone(), palette, availability, select),
        sync_section(weak.clone(), palette, settings),
        changes_section(weak, palette, settings),
    ]
}

/// "Sync": the automatic fetch.
fn sync_section(weak: WeakEntity<AppView>, palette: &Palette, settings: &Settings) -> AnyElement {
    v_flex()
        .gap(px(14.0))
        .child(SectionHeading::new(t!("settings.git.sync_title")))
        .child(
            LabeledRow::new(
                t!("settings.git.fetch"),
                switch(
                    "settings-git-fetch",
                    settings.git_fetch_automatically,
                    palette,
                    move |on, window, cx| {
                        let _ = weak.update(cx, |view, cx| {
                            view.settings.git_fetch_automatically = on;
                            view.apply_settings(window, cx);
                        });
                    },
                ),
            )
            .description(t!("settings.git.fetch_description")),
        )
        .into_any_element()
}

/// "Changes": the confirmation before discarding and what Ctrl+Enter does in the message.
fn changes_section(
    weak: WeakEntity<AppView>,
    palette: &Palette,
    settings: &Settings,
) -> AnyElement {
    let mut action = SegmentedControl::new("settings-git-commit-action");
    for choice in GitCommitAction::ALL {
        let weak = weak.clone();
        action = action.item(
            SegmentedItem::new(choice.label())
                .selected(choice == settings.git_commit_action)
                .on_click(move |window, cx| {
                    let _ = weak.update(cx, |view, cx| {
                        view.settings.git_commit_action = choice;
                        view.apply_settings(window, cx);
                    });
                }),
        );
    }
    v_flex()
        .gap(px(14.0))
        .child(SectionHeading::new(t!("settings.git.changes_title")))
        .child(
            LabeledRow::new(
                t!("settings.git.confirm_discard"),
                switch(
                    "settings-git-confirm-discard",
                    settings.git_confirm_discard,
                    palette,
                    {
                        let weak = weak.clone();
                        move |on, window, cx| {
                            let _ = weak.update(cx, |view, cx| {
                                view.settings.git_confirm_discard = on;
                                view.apply_settings(window, cx);
                            });
                        }
                    },
                ),
            )
            .description(t!("settings.git.confirm_discard_description")),
        )
        .child(
            LabeledRow::new(t!("settings.git.commit_action"), action)
                .description(t!("settings.git.commit_action_description")),
        )
        .into_any_element()
}

/// A switch in its halo.
fn switch(
    id: &'static str,
    checked: bool,
    palette: &Palette,
    on_change: impl Fn(bool, &mut Window, &mut App) + 'static,
) -> AnyElement {
    switch_halo(
        SharedString::from(format!("{id}-halo")),
        Switch::new(id)
            .checked(checked)
            .on_click(move |checked, window, cx| on_change(*checked, window, cx)),
        palette,
    )
    .into_any_element()
}

/// "Executable": the select, Browse and Detect again, and what was found.
fn executable_section(
    weak: WeakEntity<AppView>,
    palette: &Palette,
    availability: &GitAvailability,
    select: Option<&SelectHandle>,
) -> AnyElement {
    let browse = weak.clone();
    let detect = weak;
    let controls = h_flex()
        .gap(px(8.0))
        .children(select.map(|select| {
            SearchableSelect::new("settings-git-executable", select).width(EXECUTABLE_SELECT_WIDTH)
        }))
        .child(
            SecondaryButton::new("settings-git-browse", t!("settings.git.browse"))
                .on_click(move |_, _, cx| pick_executable(browse.clone(), cx)),
        )
        .child(
            SecondaryButton::new("settings-git-detect", t!("settings.git.detect_again"))
                .icon(Icon::new(IconName::RefreshCw))
                .on_click(move |_, _, cx| {
                    let _ = detect.update(cx, |view, cx| view.detect_git(cx));
                }),
        );
    let (icon, color, text) = match availability {
        GitAvailability::Detecting => (
            IconName::RefreshCw,
            palette.fg_muted,
            t!("settings.git.detecting").into_owned(),
        ),
        GitAvailability::Ready(git) => (
            IconName::CircleCheck,
            palette.success,
            t!(
                "settings.git.found",
                version = git.version().to_string(),
                path = git.path().display().to_string()
            )
            .into_owned(),
        ),
        GitAvailability::Missing(error) => (
            IconName::TriangleAlert,
            palette.danger,
            detect_error_text(error),
        ),
    };
    let missing = matches!(availability, GitAvailability::Missing(_));
    let status = h_flex()
        .items_center()
        .gap(px(8.0))
        .text_size(px(12.0))
        .text_color(color)
        .child(Icon::new(icon).small())
        .child(div().min_w_0().child(text));
    v_flex()
        .gap(px(10.0))
        .child(
            SectionHeading::new(t!("settings.git.title"))
                .description(t!("settings.git.description")),
        )
        .child(LabeledRow::new(t!("settings.git.executable"), controls))
        .child(status)
        .when(missing, |section| {
            section.child(
                h_flex().child(
                    GhostButton::new("settings-git-install", t!("settings.git.install"))
                        .icon(Icon::new(IconName::ExternalLink))
                        .on_click(|_, _, cx| cx.open_url(INSTALL_GIT_URL)),
                ),
            )
        })
        .into_any_element()
}

/// The translated message for a failed detection.
pub(crate) fn detect_error_text(error: &DetectError) -> String {
    match error {
        DetectError::NotFound => t!("settings.git.not_found").into_owned(),
        DetectError::XcodeToolsMissing => t!("settings.git.xcode_missing").into_owned(),
        DetectError::TooOld { version, .. } => t!(
            "settings.git.too_old",
            version = version.to_string(),
            min = min_version()
        )
        .into_owned(),
        DetectError::Failed(message) => {
            t!("settings.git.failed", error = message.as_str()).into_owned()
        }
    }
}

/// Opens the native file picker and, once a file is picked, makes it the git executable.
fn pick_executable(weak: WeakEntity<AppView>, cx: &mut App) {
    let receiver = cx.prompt_for_paths(PathPromptOptions {
        files: true,
        directories: false,
        multiple: false,
        prompt: None,
    });
    cx.spawn(async move |cx| {
        let Ok(Ok(Some(mut paths))) = receiver.await else {
            return;
        };
        let Some(path) = paths.pop() else {
            return;
        };
        cx.update(|cx| {
            let Some(window) = cx.active_window() else {
                return;
            };
            let _ = window.update(cx, |_, window, cx| {
                let _ = weak.update(cx, |view, cx| {
                    view.set_git_executable(Some(path), window, cx)
                });
            });
        });
    })
    .detach();
}
