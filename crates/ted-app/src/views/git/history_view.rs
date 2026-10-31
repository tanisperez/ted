//! The history tab: the commit log of the repository with its graph, a search box, and the
//! detail of the selected commit with its files. The log is read in pages of [`PAGE`] commits on
//! the background executor as the list scrolls, and laid out with telaio-git's `GraphLayout`
//! (lanes in `--topo-order`). The rows are a `uniform_list`, so a long history costs only the
//! rows on screen. There is one history tab, opened from the Git panel's header.

use std::sync::Arc;
use std::time::Duration;

use gpui_kit::assets::IconName;
use gpui_kit::component::input::{InputEvent, InputState};
use gpui_kit::component::*;
use gpui_kit::prelude::*;
use gpui_kit::*;
use rust_i18n::t;
use telaio_git::{
    CommitDetail, GitError, GraphLayout, GraphRow, LinePart, LogCommit, LogQuery, RefLabelKind,
    Repo,
};
use telaio_ui::components::{EmptyState, text_field};

use crate::state::git_branches::relative_time;
use crate::state::locale::plural;
use crate::theme::{Palette, PaletteExt};

use super::letter_color;

/// How many commits one page of the log holds.
const PAGE: usize = 200;
/// How close to the end of the list the next page starts loading, in rows.
const PREFETCH: usize = 50;
/// How long the search waits after the last keystroke.
const SEARCH_DELAY: Duration = Duration::from_millis(250);
/// Height of a row of the log.
const ROW_HEIGHT: f32 = 28.0;
/// The x of the first lane's center.
const LANE_ORIGIN: f32 = 14.0;
/// The distance between two lanes' centers.
const LANE_STEP: f32 = 14.0;
/// More lanes than this are drawn on the last one.
const MAX_LANES: usize = 12;
/// The radius of a commit's dot.
const DOT_RADIUS: f32 = 4.0;
/// The width of the lines of the graph.
const STROKE: f32 = 2.0;
/// Width of the author column.
const AUTHOR_WIDTH: f32 = 110.0;
/// Width of the date column.
const DATE_WIDTH: f32 = 96.0;
/// Width of the commit id column.
const OID_WIDTH: f32 = 64.0;
/// Width of the detail panel.
const DETAIL_WIDTH: f32 = 320.0;
/// Height of the toolbar.
const TOOLBAR_HEIGHT: f32 = 44.0;
/// How many colors the graph cycles through.
const GRAPH_COLORS: usize = 6;

/// The x of `lane`'s center.
fn lane_x(lane: usize) -> f32 {
    LANE_ORIGIN + lane.min(MAX_LANES - 1) as f32 * LANE_STEP
}

/// The view of the history tab.
pub(crate) struct HistoryView {
    repo: Repo,
    /// The commits read so far, newest first.
    commits: Vec<LogCommit>,
    /// The graph of each commit, same order. Empty while searching: a filtered log has no graph.
    graph: Vec<Arc<GraphRow>>,
    layout: GraphLayout,
    /// The widest row so far, in lanes.
    lanes: usize,
    /// Whether the last page came back short: nothing more to read.
    complete: bool,
    /// The page being read; replacing it drops a stale one.
    page: Option<Task<()>>,
    /// Bumped on every new query, so a page of an older one is dropped.
    generation: u64,
    /// What the search box held when the log was read.
    text: String,
    search: Entity<InputState>,
    search_task: Option<Task<()>>,
    /// The selected commit and its detail, once read.
    selected: Option<String>,
    detail: Option<Result<Arc<CommitDetail>, String>>,
    detail_task: Option<Task<()>>,
    /// The commit `HEAD` pointed at when the log was read.
    head: Option<String>,
    error: Option<String>,
    scroll: UniformListScrollHandle,
    _subscription: Subscription,
}

impl HistoryView {
    pub(crate) fn new(
        repo: Repo,
        head: Option<String>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Self {
        let search = cx.new(|cx| InputState::new(window, cx).placeholder(t!("git.log.search")));
        let subscription = cx.subscribe(&search, |view, _, event: &InputEvent, cx| {
            if matches!(event, InputEvent::Change) {
                view.search_changed(cx);
            }
        });
        let mut view = HistoryView {
            repo,
            commits: Vec::new(),
            graph: Vec::new(),
            layout: GraphLayout::new(),
            lanes: 1,
            complete: false,
            page: None,
            generation: 0,
            text: String::new(),
            search,
            search_task: None,
            selected: None,
            detail: None,
            detail_task: None,
            head,
            error: None,
            scroll: UniformListScrollHandle::new(),
            _subscription: subscription,
        };
        view.restart(cx);
        view
    }

    /// The tab's name.
    pub(crate) fn title() -> String {
        t!("git.log.tab").into_owned()
    }

    /// Reads the log again from the start when `HEAD` moved (a commit, a checkout, a pull).
    pub(crate) fn head_changed(&mut self, head: Option<String>, cx: &mut Context<Self>) {
        if head != self.head {
            self.head = head;
            self.restart(cx);
        }
    }

    /// Forgets what was read and reads the first page of the current query.
    fn restart(&mut self, cx: &mut Context<Self>) {
        self.generation += 1;
        self.commits.clear();
        self.graph.clear();
        self.layout = GraphLayout::new();
        self.lanes = 1;
        self.complete = false;
        self.error = None;
        self.page = None;
        self.load_page(cx);
        cx.notify();
    }

    /// The search text changed: read the log again once the typing stops.
    fn search_changed(&mut self, cx: &mut Context<Self>) {
        self.search_task = Some(cx.spawn(async move |this, cx| {
            cx.background_executor().timer(SEARCH_DELAY).await;
            let _ = this.update(cx, |view, cx| {
                let text = view.search.read(cx).value().trim().to_string();
                if text != view.text {
                    view.text = text;
                    view.restart(cx);
                }
            });
        }));
    }

    /// Reads the next page, unless one is being read or there is nothing more.
    fn load_page(&mut self, cx: &mut Context<Self>) {
        if self.page.is_some() || self.complete {
            return;
        }
        let repo = self.repo.clone();
        let query = LogQuery {
            text: (!self.text.is_empty()).then(|| self.text.clone()),
            skip: self.commits.len(),
            limit: PAGE,
            ..LogQuery::default()
        };
        let generation = self.generation;
        self.page = Some(cx.spawn(async move |this, cx| {
            let result = cx.background_spawn(async move { repo.log(&query) }).await;
            let _ = this.update(cx, |view, cx| {
                if view.generation != generation {
                    return;
                }
                view.page = None;
                match result {
                    Ok(commits) => view.add_page(commits),
                    Err(error) => {
                        log::warn!("reading the log failed: {error}");
                        view.error = Some(error.to_string());
                        view.complete = true;
                    }
                }
                cx.notify();
            });
        }));
    }

    /// Appends a page and lays out its graph (only without a search: a filtered log has holes).
    fn add_page(&mut self, commits: Vec<LogCommit>) {
        self.complete = commits.len() < PAGE;
        if self.text.is_empty() {
            for commit in &commits {
                let row = self.layout.push(&commit.oid, &commit.parents);
                self.lanes = self.lanes.max(row.width);
                self.graph.push(Arc::new(row));
            }
        }
        self.commits.extend(commits);
    }

    /// Selects a commit and reads its detail.
    fn select(&mut self, oid: String, cx: &mut Context<Self>) {
        if self.selected.as_deref() == Some(oid.as_str()) {
            return;
        }
        self.selected = Some(oid.clone());
        self.detail = None;
        let repo = self.repo.clone();
        self.detail_task = Some(cx.spawn(async move |this, cx| {
            let reading = oid.clone();
            let result = cx
                .background_spawn(async move { repo.commit_detail(&reading) })
                .await;
            let _ = this.update(cx, |view, cx| {
                if view.selected.as_deref() != Some(oid.as_str()) {
                    return;
                }
                view.detail = Some(
                    result
                        .map(Arc::new)
                        .map_err(|error: GitError| error.to_string()),
                );
                cx.notify();
            });
        }));
        cx.notify();
    }

    /// The width of the graph column.
    fn graph_width(&self) -> f32 {
        if self.graph.is_empty() {
            0.0
        } else {
            lane_x(self.lanes.max(1) - 1) + LANE_ORIGIN
        }
    }

    fn render_toolbar(&self, palette: &Palette, cx: &mut Context<Self>) -> impl IntoElement {
        let count = if self.complete {
            plural("git.log.commits", self.commits.len())
        } else {
            plural("git.log.commits_more", self.commits.len())
        };
        h_flex()
            .flex_none()
            .h(px(TOOLBAR_HEIGHT))
            .px_3()
            .gap(px(12.0))
            .items_center()
            .border_b_1()
            .border_color(palette.border)
            .child(div().w(px(280.0)).child(text_field(&self.search, cx)))
            .child(
                div()
                    .text_size(px(12.0))
                    .text_color(palette.fg_subtle)
                    .child(count),
            )
    }

    fn render_detail(&self, palette: &Palette, cx: &mut Context<Self>) -> Option<AnyElement> {
        self.selected.as_ref()?;
        let body = match &self.detail {
            None => div().into_any_element(),
            Some(Err(error)) => div()
                .text_color(palette.danger)
                .child(error.clone())
                .into_any_element(),
            Some(Ok(detail)) => {
                let mono = cx.theme().mono_font_family.clone();
                let now = now_seconds();
                v_flex()
                    .gap(px(10.0))
                    .child(
                        div()
                            .text_size(px(13.0))
                            .whitespace_normal()
                            .child(detail.message.clone()),
                    )
                    .child(
                        v_flex()
                            .gap(px(2.0))
                            .text_size(px(12.0))
                            .text_color(palette.fg_muted)
                            .child(format!("{} <{}>", detail.author_name, detail.author_email))
                            .child(relative_time(detail.date, now))
                            .child(
                                div()
                                    .font_family(mono.clone())
                                    .text_color(palette.fg_subtle)
                                    .child(detail.oid.clone()),
                            ),
                    )
                    .child(
                        div()
                            .pt(px(6.0))
                            .text_size(px(11.0))
                            .font_weight(FontWeight::SEMIBOLD)
                            .text_color(palette.fg_subtle)
                            .child(plural("git.log.files", detail.files.len()).to_uppercase()),
                    )
                    .children(detail.files.iter().map(|file| {
                        let counts = if file.binary {
                            "bin".to_string()
                        } else {
                            format!("+{} \u{2212}{}", file.additions, file.deletions)
                        };
                        h_flex()
                            .gap(px(8.0))
                            .text_size(px(12.0))
                            .child(
                                div()
                                    .flex_none()
                                    .w(px(12.0))
                                    .font_family(mono.clone())
                                    .font_weight(FontWeight::MEDIUM)
                                    .text_color(letter_color(file.letter, palette))
                                    .child(file.letter.to_string()),
                            )
                            .child(div().flex_1().min_w_0().truncate().child(file.path.clone()))
                            .child(
                                div()
                                    .flex_none()
                                    .font_family(mono.clone())
                                    .text_color(palette.fg_subtle)
                                    .child(counts),
                            )
                    }))
                    .into_any_element()
            }
        };
        Some(
            div()
                .id("history-detail")
                .flex_none()
                .w(px(DETAIL_WIDTH))
                .h_full()
                .p(px(14.0))
                .overflow_y_scroll()
                .border_l_1()
                .border_color(palette.border)
                .bg(palette.surface)
                .child(body)
                .into_any_element(),
        )
    }
}

impl Render for HistoryView {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let palette = cx.palette();
        let list = if let Some(error) = &self.error {
            div()
                .flex_1()
                .child(
                    EmptyState::new(t!("git.log.error"))
                        .icon(Icon::new(IconName::CircleAlert))
                        .description(error.clone()),
                )
                .into_any_element()
        } else if self.commits.is_empty() && self.page.is_none() {
            let title = if self.text.is_empty() {
                t!("git.log.empty")
            } else {
                t!("git.log.no_match")
            };
            div()
                .flex_1()
                .child(EmptyState::new(title).icon(Icon::new(IconName::GitGraph)))
                .into_any_element()
        } else {
            uniform_list(
                "history-rows",
                self.commits.len(),
                cx.processor(|view, range: std::ops::Range<usize>, _window, cx| {
                    if range.end + PREFETCH >= view.commits.len() {
                        view.load_page(cx);
                    }
                    let palette = cx.palette();
                    let graph_width = view.graph_width();
                    let now = now_seconds();
                    range
                        .filter_map(|index| view.commits.get(index).map(|commit| (index, commit)))
                        .map(|(index, commit)| {
                            let graph = view.graph.get(index).cloned();
                            let selected = view.selected.as_deref() == Some(commit.oid.as_str());
                            render_row(
                                index,
                                commit,
                                graph,
                                graph_width,
                                selected,
                                now,
                                &palette,
                                cx,
                            )
                        })
                        .collect()
                }),
            )
            .track_scroll(&self.scroll)
            .flex_1()
            .into_any_element()
        };
        v_flex()
            .id("history-view")
            .size_full()
            .bg(palette.bg)
            .child(self.render_toolbar(&palette, cx))
            .child(
                h_flex()
                    .flex_1()
                    .min_h_0()
                    .child(v_flex().flex_1().min_w_0().h_full().child(list))
                    .children(self.render_detail(&palette, cx)),
            )
    }
}

/// The current unix time, in seconds.
fn now_seconds() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_or(0, |elapsed| elapsed.as_secs() as i64)
}

/// The colors the graph's lanes cycle through.
fn graph_colors(palette: &Palette) -> [Hsla; GRAPH_COLORS] {
    [
        palette.accent_text,
        palette.success,
        palette.warning,
        palette.info,
        palette.danger,
        palette.fg_muted,
    ]
}

/// One commit of the log: its graph, its branches and tags, its subject, author, date and id.
#[allow(clippy::too_many_arguments)]
fn render_row(
    index: usize,
    commit: &LogCommit,
    graph: Option<Arc<GraphRow>>,
    graph_width: f32,
    selected: bool,
    now: i64,
    palette: &Palette,
    cx: &mut Context<HistoryView>,
) -> AnyElement {
    let mono = cx.theme().mono_font_family.clone();
    let is_head = commit.refs.iter().any(|r| r.kind == RefLabelKind::Head);
    let badges = commit.refs.iter().map(|label| {
        let (color, icon) = match label.kind {
            RefLabelKind::Head => (palette.accent_text, IconName::GitBranch),
            RefLabelKind::Local => (palette.success, IconName::GitBranch),
            RefLabelKind::Remote => (palette.info, IconName::Cloud),
            RefLabelKind::Tag => (palette.warning, IconName::Tag),
        };
        h_flex()
            .flex_none()
            .h(px(18.0))
            .px(px(6.0))
            .gap(px(4.0))
            .items_center()
            .rounded(px(4.0))
            .border_1()
            .border_color(color.opacity(0.5))
            .text_size(px(11.0))
            .text_color(color)
            .child(Icon::new(icon).with_size(px(11.0)))
            .child(label.name.clone())
    });
    let colors = graph_colors(palette);
    let hollow = if selected {
        palette.list_active
    } else {
        palette.bg
    };
    let graph = graph.map(|row| {
        canvas(
            |_, _, _| (),
            move |bounds, _, window, _| paint_graph(bounds, &row, &colors, is_head, hollow, window),
        )
        .flex_none()
        .w(px(graph_width))
        .h(px(ROW_HEIGHT))
    });
    let oid = commit.oid.clone();
    let hover = palette.hover;
    h_flex()
        .id(("history-row", index))
        .w_full()
        .h(px(ROW_HEIGHT))
        .pr_3()
        .gap(px(12.0))
        .items_center()
        .text_size(px(12.5))
        .cursor_pointer()
        .when(selected, |row| row.bg(palette.list_active))
        .when(!selected, |row| row.hover(move |style| style.bg(hover)))
        .when(graph.is_none(), |row| row.pl_3())
        .children(graph)
        .child(
            h_flex()
                .flex_1()
                .min_w_0()
                .overflow_hidden()
                .gap(px(6.0))
                .items_center()
                .children(badges)
                .child(div().min_w_0().truncate().child(commit.subject.clone())),
        )
        .child(
            div()
                .flex_none()
                .w(px(AUTHOR_WIDTH))
                .truncate()
                .text_color(palette.fg_muted)
                .child(commit.author_name.clone()),
        )
        .child(
            div()
                .flex_none()
                .w(px(DATE_WIDTH))
                .truncate()
                .text_color(palette.fg_subtle)
                .child(relative_time(commit.date, now)),
        )
        .child(
            div()
                .flex_none()
                .w(px(OID_WIDTH))
                .font_family(mono)
                .text_size(px(11.5))
                .text_color(palette.fg_subtle)
                .child(commit.short.clone()),
        )
        .on_click(cx.listener(move |view, _, _, cx| view.select(oid.clone(), cx)))
        .into_any_element()
}

/// Paints the lanes of `row` and its dot inside `bounds` (one row of the list). Lines are 2 px
/// strokes: straight when the lane stays, a cubic curve between the edge of the row and the dot
/// when it moves. The dot of the commit `HEAD` points at is hollow.
fn paint_graph(
    bounds: Bounds<Pixels>,
    row: &GraphRow,
    colors: &[Hsla; GRAPH_COLORS],
    is_head: bool,
    hollow: Hsla,
    window: &mut Window,
) {
    let top = bounds.origin.y;
    let middle = top + bounds.size.height / 2.0;
    let bottom = top + bounds.size.height;
    let x = |lane: usize| bounds.origin.x + px(lane_x(lane));
    let color_of = |index: usize| colors[index % GRAPH_COLORS];
    for line in &row.lines {
        let (start, end) = match line.part {
            LinePart::Top => (point(x(line.from), top), point(x(line.to), middle)),
            LinePart::Bottom => (point(x(line.from), middle), point(x(line.to), bottom)),
        };
        let mut path = PathBuilder::stroke(px(STROKE));
        path.move_to(start);
        if line.from == line.to {
            path.line_to(end);
        } else {
            let reach = (end.y - start.y) * 0.6;
            path.cubic_bezier_to(
                end,
                point(start.x, start.y + reach),
                point(end.x, end.y - reach),
            );
        }
        if let Ok(path) = path.build() {
            window.paint_path(path, color_of(line.color));
        }
    }
    let radius = px(DOT_RADIUS);
    let color = color_of(row.color);
    window.paint_quad(quad(
        Bounds {
            origin: point(x(row.lane) - radius, middle - radius),
            size: size(radius * 2.0, radius * 2.0),
        },
        radius,
        if is_head { hollow } else { color },
        px(STROKE),
        color,
        BorderStyle::Solid,
    ));
}
