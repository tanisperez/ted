//! The results of Find in files left in a tab (`Ctrl+Enter` in the window): every file with its
//! matches, a header row per file and a row per match with its line number. A click on a match
//! opens it in its file. The rows are a virtual list, so ten thousand matches cost what the
//! screen shows.

use std::path::PathBuf;

use gpui_kit::assets::IconName;
use gpui_kit::component::*;
use gpui_kit::prelude::*;
use gpui_kit::*;
use rust_i18n::t;

use crate::state::find_files::FileResult;
use crate::state::locale::plural;
use crate::theme::PaletteExt;
use crate::theme::metrics::FIND_FILES_ROW_HEIGHT;

use super::find_files::match_text;

/// A row of the list: a file, or one of its matches.
#[derive(Clone, Copy)]
enum Row {
    File(usize),
    Match(usize, usize),
}

/// What the tab asks of the app.
pub enum FindResultsEvent {
    /// Show `offset` (bytes) of the file at `path`, selected.
    Open {
        path: PathBuf,
        offset: std::ops::Range<usize>,
    },
}

impl EventEmitter<FindResultsEvent> for FindResultsView {}

/// The tab.
pub struct FindResultsView {
    query: String,
    files: Vec<FileResult>,
    rows: Vec<Row>,
    scroll: UniformListScrollHandle,
    focus_handle: FocusHandle,
}

impl FindResultsView {
    pub fn new(query: String, files: Vec<FileResult>, cx: &mut Context<Self>) -> Self {
        let rows = files
            .iter()
            .enumerate()
            .flat_map(|(file, result)| {
                std::iter::once(Row::File(file))
                    .chain((0..result.matches.len()).map(move |line| Row::Match(file, line)))
            })
            .collect();
        Self {
            query,
            files,
            rows,
            scroll: UniformListScrollHandle::new(),
            focus_handle: cx.focus_handle(),
        }
    }

    /// The tab's title: `Find: query`.
    pub fn title(&self) -> String {
        t!("find_files.tab_title", query = self.query.as_str()).into_owned()
    }

    fn render_row(&self, index: usize, cx: &mut Context<Self>) -> AnyElement {
        let palette = cx.palette();
        let hover = palette.hover;
        match self.rows.get(index).copied() {
            Some(Row::File(file)) => {
                let result = &self.files[file];
                h_flex()
                    .w_full()
                    .h(px(FIND_FILES_ROW_HEIGHT))
                    .items_center()
                    .gap(px(8.0))
                    .px(px(16.0))
                    .text_size(px(12.5))
                    .child(
                        Icon::new(IconName::File)
                            .size(px(13.0))
                            .text_color(palette.fg_subtle),
                    )
                    .child(
                        div()
                            .font_weight(FontWeight::MEDIUM)
                            .child(result.name().to_string()),
                    )
                    .child(
                        div()
                            .min_w_0()
                            .truncate()
                            .text_color(palette.fg_subtle)
                            .child(result.dir().to_string()),
                    )
                    .child(
                        div()
                            .text_color(palette.fg_subtle)
                            .child(plural("find_files.matches", result.matches.len())),
                    )
                    .into_any_element()
            }
            Some(Row::Match(file, line)) => {
                let result = &self.files[file];
                let matched = &result.matches[line];
                let (path, offset) = (result.path.clone(), matched.offset.clone());
                h_flex()
                    .id(("find-result", index))
                    .w_full()
                    .h(px(FIND_FILES_ROW_HEIGHT))
                    .items_center()
                    .gap(px(12.0))
                    .pl(px(38.0))
                    .pr(px(16.0))
                    .cursor_pointer()
                    .hover(move |style| style.bg(hover))
                    .font_family(cx.theme().mono_font_family.clone())
                    .text_size(px(12.5))
                    .child(
                        div()
                            .flex_none()
                            .w(px(48.0))
                            .text_right()
                            .text_color(palette.fg_subtle)
                            .child((matched.line + 1).to_string()),
                    )
                    .child(div().flex_1().min_w_0().truncate().child(match_text(
                        &matched.text,
                        &matched.range,
                        true,
                        None,
                        false,
                        &palette,
                    )))
                    .on_click(cx.listener(move |_, _, _, cx| {
                        cx.emit(FindResultsEvent::Open {
                            path: path.clone(),
                            offset: offset.clone(),
                        });
                    }))
                    .into_any_element()
            }
            None => div().into_any_element(),
        }
    }
}

impl Focusable for FindResultsView {
    fn focus_handle(&self, _cx: &App) -> FocusHandle {
        self.focus_handle.clone()
    }
}

impl Render for FindResultsView {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let palette = cx.palette();
        let list = uniform_list(
            "find-results-rows",
            self.rows.len(),
            cx.processor(|view, range: std::ops::Range<usize>, _window, cx| {
                range.map(|row| view.render_row(row, cx)).collect()
            }),
        )
        .track_scroll(&self.scroll)
        .size_full();
        div()
            .track_focus(&self.focus_handle)
            .size_full()
            .py(px(6.0))
            .bg(palette.bg)
            .child(list)
    }
}
