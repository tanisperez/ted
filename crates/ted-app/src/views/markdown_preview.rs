//! The Markdown preview: the document of an editor rendered as the design shows it, a 720 px
//! column with the typography of `docs/design-system.md`. It lives next to the editor (the split
//! view), in its place (the preview view) or in a tab of its own.
//!
//! The text is parsed by `ted_syntax::parse_markdown` on the background executor, a moment after
//! the last edit, and a result for an older text is dropped (a generation counter). The blocks
//! are a gpui `list`, so only those on screen are laid out, whatever the size of the file. In the
//! split view the preview follows the editor: the block that holds the editor's top line is
//! scrolled to the top.

use std::ops::Range;
use std::sync::Arc;
use std::time::Duration;

use gpui_kit::assets::IconName;
use gpui_kit::component::scroll::{Scrollbar, ScrollbarMode};
use gpui_kit::component::tooltip::Tooltip;
use gpui_kit::component::*;
use gpui_kit::prelude::*;
use gpui_kit::*;
use ted_syntax::{
    Block, BlockKind, Inline, InlineStyle, MarkdownDocument, parse_markdown, word_count,
};

use crate::state::markdown_view::MarkdownView;
use crate::theme::syntax::SyntaxColors;
use crate::theme::{Palette, PaletteExt};

use super::editor::EditorView;

/// How long the preview waits after an edit before parsing again.
const REPARSE_DELAY: Duration = Duration::from_millis(150);
/// Width of the text column.
const COLUMN_WIDTH: f32 = 720.0;
/// Space around the column: above the first block, at the sides, under the last.
const PADDING_TOP: f32 = 36.0;
const PADDING_X: f32 = 40.0;
const PADDING_BOTTOM: f32 = 80.0;
/// Text size of paragraphs, lists and quotes.
const BODY_SIZE: f32 = 14.0;
/// Text size of code, inline and in blocks.
const CODE_SIZE: f32 = 12.5;
/// Indentation of a list, where its markers sit.
const LIST_INDENT: f32 = 22.0;

/// The preview of one editor's document.
pub(crate) struct MarkdownPreview {
    editor: Entity<EditorView>,
    /// What the document shows. A preview tab is always `Preview`.
    view: MarkdownView,
    document: Arc<MarkdownDocument>,
    list: ListState,
    /// The buffer version parsed, or waiting to be parsed.
    parsed: Option<u64>,
    generation: u64,
    _parsing: Option<Task<()>>,
    /// A parse runs on the background executor: a newer text waits for it to finish, so a
    /// big file is never parsed twice at once.
    running: bool,
    sync_scroll: bool,
    /// The editor's top line the preview last followed.
    followed: Option<usize>,
    /// Takes the keys while the editor is hidden, so the window's shortcuts still work.
    focus_handle: FocusHandle,
    _observe: Subscription,
}

impl MarkdownPreview {
    pub(crate) fn new(
        editor: Entity<EditorView>,
        view: MarkdownView,
        sync_scroll: bool,
        cx: &mut Context<Self>,
    ) -> Self {
        let observe = cx.observe(&editor, |preview, editor, cx| {
            preview.editor_changed(&editor, cx);
        });
        let mut preview = Self {
            editor: editor.clone(),
            view,
            document: Arc::default(),
            list: ListState::new(0, ListAlignment::Top, px(400.0)),
            parsed: None,
            generation: 0,
            _parsing: None,
            running: false,
            sync_scroll,
            followed: None,
            focus_handle: cx.focus_handle(),
            _observe: observe,
        };
        preview.editor_changed(&editor, cx);
        preview
    }

    pub(crate) fn editor(&self) -> &Entity<EditorView> {
        &self.editor
    }

    pub(crate) fn view(&self) -> MarkdownView {
        self.view
    }

    /// The words of the document, for the status bar.
    pub(crate) fn words(&self) -> usize {
        self.document.words
    }

    pub(crate) fn set_view(&mut self, view: MarkdownView, cx: &mut Context<Self>) {
        if self.view != view {
            let was_shown = self.view.shows_preview();
            self.view = view;
            // The blocks are only built while the preview shows: build them now, at once.
            if view.shows_preview() && !was_shown && !self.running {
                self.parsed = None;
                self.parse(cx);
            }
            self.followed = None;
            self.follow_editor(cx);
            cx.notify();
        }
    }

    pub(crate) fn set_sync_scroll(&mut self, sync_scroll: bool, cx: &mut Context<Self>) {
        self.sync_scroll = sync_scroll;
        self.followed = None;
        self.follow_editor(cx);
    }

    /// The editor notified: its text may have changed (parse again) or it scrolled (follow it).
    fn editor_changed(&mut self, editor: &Entity<EditorView>, cx: &mut Context<Self>) {
        let version = editor.read(cx).state.buffer.version();
        if self.parsed != Some(version) && !self.running {
            self.parse(cx);
        }
        self.follow_editor(cx);
    }

    /// Parses the editor's text in the background, at once the first time and after a short
    /// delay afterwards, so typing fast parses once.
    fn parse(&mut self, cx: &mut Context<Self>) {
        let first = self.parsed.is_none();
        self.parsed = Some(self.editor.read(cx).state.buffer.version());
        self.generation += 1;
        let generation = self.generation;
        self._parsing = Some(cx.spawn(async move |this, cx| {
            if !first {
                cx.background_executor().timer(REPARSE_DELAY).await;
            }
            // The text as it is after the delay, the latest edits included.
            let Ok((buffer, blocks)) = this.update(cx, |preview, cx| {
                preview.running = true;
                let buffer = preview.editor.read(cx).state.buffer.clone();
                preview.parsed = Some(buffer.version());
                (buffer, preview.view.shows_preview())
            }) else {
                return;
            };
            let document = cx
                .background_spawn(async move {
                    let started = std::time::Instant::now();
                    let text = buffer.to_text();
                    // With the preview hidden only the words are needed, for the status bar.
                    let document = match blocks {
                        true => parse_markdown(&text),
                        false => MarkdownDocument {
                            blocks: Vec::new(),
                            words: word_count(&text),
                        },
                    };
                    log::debug!(
                        "parsed the Markdown preview ({} blocks) in {:?}",
                        document.blocks.len(),
                        started.elapsed()
                    );
                    document
                })
                .await;
            let _ = this.update(cx, |preview, cx| {
                preview.running = false;
                if preview.generation != generation {
                    return;
                }
                // Edited while it ran, or the preview showed meanwhile and this parse counted
                // only the words: parse again, once this task is done.
                let edited =
                    preview.editor.read(cx).state.buffer.version() != preview.parsed.unwrap_or(0);
                let blocks_missing = preview.view.shows_preview() && !blocks;
                if edited || blocks_missing {
                    if blocks_missing {
                        preview.parsed = None;
                    }
                    let weak = cx.entity().downgrade();
                    cx.defer(move |cx| {
                        let _ = weak.update(cx, |preview, cx| preview.parse(cx));
                    });
                }
                let old = preview.document.blocks.len();
                let new = document.blocks.len();
                preview.document = Arc::new(document);
                // Keeps the scroll position on the same block while typing.
                preview.list.splice(0..old, new);
                preview.followed = None;
                preview.follow_editor(cx);
                cx.notify();
            });
        }));
    }

    /// In the split view, scrolls the block that holds the editor's top line to the top.
    fn follow_editor(&mut self, cx: &mut Context<Self>) {
        if !self.sync_scroll || self.view != MarkdownView::Split {
            return;
        }
        let top = self.editor.read(cx).state.line_at_y(0.0);
        if self.followed == Some(top) {
            return;
        }
        self.followed = Some(top);
        let Some((item, into)) = self.document.block_at_line(top) else {
            return;
        };
        let height = self
            .list
            .bounds_for_item(item)
            .map(|bounds| bounds.size.height)
            .unwrap_or_default();
        self.list.scroll_to(ListOffset {
            item_ix: item,
            offset_in_item: height * into,
        });
        cx.notify();
    }

    fn render_item(&self, item: usize, window: &mut Window, cx: &mut App) -> AnyElement {
        let Some(block) = self.document.blocks.get(item) else {
            return div().into_any_element();
        };
        let style = PreviewStyle::new(window, cx);
        let last = item + 1 == self.document.blocks.len();
        let mut ids = 0;
        div()
            .w_full()
            .flex()
            .justify_center()
            .px(px(PADDING_X))
            .when(item == 0, |this| this.pt(px(PADDING_TOP)))
            .when(last, |this| this.pb(px(PADDING_BOTTOM)))
            .child(
                div()
                    .w_full()
                    .max_w(px(COLUMN_WIDTH))
                    .child(render_block(block, item, &mut ids, &style)),
            )
            .into_any_element()
    }
}

impl Focusable for MarkdownPreview {
    fn focus_handle(&self, _cx: &App) -> FocusHandle {
        self.focus_handle.clone()
    }
}

impl Render for MarkdownPreview {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let palette = cx.palette();
        let rows = list(
            self.list.clone(),
            cx.processor(|this, item, window, cx| this.render_item(item, window, cx)),
        )
        .size_full();
        let track = palette.bg.opacity(0.0);
        div()
            .track_focus(&self.focus_handle)
            .relative()
            .size_full()
            .bg(palette.bg)
            .text_color(palette.fg)
            .child(rows)
            .child(
                div().absolute().inset_0().child(
                    Scrollbar::vertical(&self.list)
                        .id("markdown-preview-scrollbar")
                        .mode(ScrollbarMode::Always)
                        .styles(move |styles| {
                            styles
                                .track(|style| style.bg(track).border_color(track))
                                .track_hover(|style| style.bg(track).border_color(track))
                                .track_active(|style| style.bg(track).border_color(track))
                        }),
                ),
            )
    }
}

/// What every block needs to paint itself: the palette, the fonts and the code colors.
struct PreviewStyle {
    palette: Palette,
    ui_font: Font,
    mono_font: Font,
    syntax: SyntaxColors,
}

impl PreviewStyle {
    fn new(window: &Window, cx: &App) -> Self {
        let palette = (*cx.palette()).clone();
        let ui_font = window.text_style().font();
        let mono_font = font(cx.theme().mono_font_family.clone());
        let syntax = SyntaxColors::new(&cx.theme().highlight_theme, &palette);
        Self {
            palette,
            ui_font,
            mono_font,
            syntax,
        }
    }
}

/// A unique id for an element of block `item`.
fn next_id(item: usize, ids: &mut usize) -> ElementId {
    *ids += 1;
    ElementId::from(SharedString::from(format!("md-{item}-{ids}")))
}

fn render_block(block: &Block, item: usize, ids: &mut usize, style: &PreviewStyle) -> AnyElement {
    let palette = &style.palette;
    match &block.kind {
        BlockKind::Heading { level, text } => {
            let (size, top, bottom) = match level {
                1 => (26.0, 0.0, 14.0),
                2 => (19.0, 28.0, 10.0),
                _ => (15.0, 22.0, 8.0),
            };
            div()
                .mt(px(top))
                .mb(px(bottom))
                .text_size(px(size))
                .line_height(relative(1.3))
                .when(*level == 1, |this| {
                    this.pb(px(10.0)).border_b_1().border_color(palette.border)
                })
                .child(render_inline(
                    text,
                    FontWeight::SEMIBOLD,
                    palette.fg,
                    item,
                    ids,
                    style,
                ))
                .into_any_element()
        }
        BlockKind::Paragraph(text) => div()
            .mb(px(14.0))
            .text_size(px(BODY_SIZE))
            .line_height(relative(1.65))
            .child(render_inline(
                text,
                FontWeight::NORMAL,
                palette.fg,
                item,
                ids,
                style,
            ))
            .into_any_element(),
        BlockKind::List(list) => {
            let items = list.items.iter().enumerate().map(|(index, list_item)| {
                let marker: AnyElement = match list_item.checked {
                    Some(done) => Icon::new(if done {
                        IconName::SquareCheck
                    } else {
                        IconName::Square
                    })
                    .with_size(px(14.0))
                    .text_color(if done {
                        palette.accent_text
                    } else {
                        palette.fg_muted
                    })
                    .into_any_element(),
                    None if list.ordered => div()
                        .text_color(palette.fg_muted)
                        .child(format!("{}.", list.start as usize + index))
                        .into_any_element(),
                    None => div()
                        .text_color(palette.fg_muted)
                        .child("•")
                        .into_any_element(),
                };
                let blocks = list_item
                    .blocks
                    .iter()
                    .map(|inner| render_list_block(inner, palette.fg, item, ids, style))
                    .collect::<Vec<_>>();
                h_flex()
                    .items_start()
                    .my(px(2.0))
                    .child(
                        div()
                            .flex_none()
                            .w(px(LIST_INDENT))
                            .h(px(BODY_SIZE * 1.65))
                            .flex()
                            .items_center()
                            .child(marker),
                    )
                    .child(v_flex().flex_1().min_w_0().children(blocks))
            });
            v_flex()
                .mb(px(14.0))
                .text_size(px(BODY_SIZE))
                .line_height(relative(1.65))
                .children(items)
                .into_any_element()
        }
        BlockKind::Quote(blocks) => div()
            .mb(px(14.0))
            .py(px(2.0))
            .pl(px(14.0))
            .border_l(px(3.0))
            .border_color(palette.border_strong)
            .text_color(palette.fg_muted)
            .children(
                blocks
                    .iter()
                    .map(|inner| render_list_block(inner, palette.fg_muted, item, ids, style)),
            )
            .into_any_element(),
        BlockKind::Code { text, spans, .. } => {
            let runs = code_runs(text, spans, style);
            let shown: SharedString = if text.is_empty() {
                " ".into()
            } else {
                text.clone().into()
            };
            let runs = if text.is_empty() {
                vec![run(1, style.mono_font.clone(), palette.fg)]
            } else {
                runs
            };
            div()
                .id(next_id(item, ids))
                .mb(px(16.0))
                .py(px(12.0))
                .px(px(14.0))
                .bg(palette.raised)
                .border_1()
                .border_color(palette.border)
                .rounded(px(8.0))
                .overflow_x_scroll()
                .text_size(px(CODE_SIZE))
                .line_height(relative(1.6))
                .whitespace_nowrap()
                .child(StyledText::new(shown).with_runs(runs))
                .into_any_element()
        }
        BlockKind::Table(table) => {
            let header = h_flex()
                .bg(palette.surface)
                .children(table.head.iter().map(|cell| {
                    div()
                        .flex_1()
                        .min_w_0()
                        .py(px(7.0))
                        .px(px(12.0))
                        .border_b_1()
                        .border_color(palette.border)
                        .text_size(px(12.5))
                        .text_color(palette.fg_muted)
                        .child(render_inline(
                            cell,
                            FontWeight::MEDIUM,
                            palette.fg_muted,
                            item,
                            ids,
                            style,
                        ))
                }));
            let columns = table.head.len().max(1);
            let rows = table.rows.iter().enumerate().map(|(index, row)| {
                let last = index + 1 == table.rows.len();
                h_flex().children((0..columns).map(|column| {
                    let cell = row.get(column).cloned().unwrap_or_default();
                    div()
                        .flex_1()
                        .min_w_0()
                        .py(px(7.0))
                        .px(px(12.0))
                        .when(!last, |this| this.border_b_1().border_color(palette.border))
                        .text_size(px(13.0))
                        .child(render_inline(
                            &cell,
                            FontWeight::NORMAL,
                            palette.fg,
                            item,
                            ids,
                            style,
                        ))
                }))
            });
            v_flex()
                .mb(px(16.0))
                .border_1()
                .border_color(palette.border)
                .rounded(px(8.0))
                .overflow_hidden()
                .child(header)
                .children(rows)
                .into_any_element()
        }
        BlockKind::Rule => div()
            .my(px(24.0))
            .h(px(1.0))
            .bg(palette.border)
            .into_any_element(),
    }
}

/// A block inside a list item or a quote: a paragraph there has no space under it, so the
/// items of a tight list stay together.
fn render_list_block(
    block: &Block,
    color: Hsla,
    item: usize,
    ids: &mut usize,
    style: &PreviewStyle,
) -> AnyElement {
    match &block.kind {
        BlockKind::Paragraph(text) => div()
            .child(render_inline(
                text,
                FontWeight::NORMAL,
                color,
                item,
                ids,
                style,
            ))
            .into_any_element(),
        _ => render_block(block, item, ids, style),
    }
}

/// Text with its styles: bold, italic, code, struck through, links (clickable, with their target
/// in a tooltip) and the text of images.
fn render_inline(
    inline: &Inline,
    weight: FontWeight,
    color: Hsla,
    item: usize,
    ids: &mut usize,
    style: &PreviewStyle,
) -> AnyElement {
    let mut base = style.ui_font.clone();
    base.weight = weight;
    let mut runs = Vec::new();
    let mut links: Vec<(Range<usize>, String)> = Vec::new();
    let mut at = 0;
    for span in &inline.spans {
        if span.range.start > at {
            runs.push(run(span.range.start - at, base.clone(), color));
        }
        runs.push(styled_run(
            span.range.len(),
            &span.style,
            &base,
            color,
            style,
        ));
        if let Some(url) = &span.style.link {
            links.push((span.range.clone(), url.clone()));
        }
        at = span.range.end;
    }
    if inline.text.len() > at {
        runs.push(run(inline.text.len() - at, base, color));
    }
    if inline.text.is_empty() {
        return div().into_any_element();
    }
    let text = StyledText::new(SharedString::from(inline.text.clone())).with_runs(runs);
    if links.is_empty() {
        return text.into_any_element();
    }
    let hints = links.clone();
    let (ranges, urls): (Vec<_>, Vec<_>) = links.into_iter().unzip();
    InteractiveText::new(next_id(item, ids), text)
        .on_click(ranges, move |index, _, cx| {
            // Only web and mail links: a relative link has no file to open here.
            if let Some(url) = urls.get(index)
                && (url.starts_with("http://")
                    || url.starts_with("https://")
                    || url.starts_with("mailto:"))
            {
                cx.open_url(url);
            }
        })
        .tooltip(move |index, window, cx| {
            let (_, url) = hints.iter().find(|(range, _)| range.contains(&index))?;
            Some(Tooltip::new(SharedString::from(url.clone())).build(window, cx))
        })
        .into_any_element()
}

fn styled_run(
    len: usize,
    inline: &InlineStyle,
    base: &Font,
    plain: Hsla,
    style: &PreviewStyle,
) -> TextRun {
    let palette = &style.palette;
    let mut font = if inline.code {
        style.mono_font.clone()
    } else {
        base.clone()
    };
    if inline.strong {
        font.weight = FontWeight::SEMIBOLD;
    }
    if inline.emphasis || inline.image {
        font.style = FontStyle::Italic;
    }
    let color = if inline.link.is_some() {
        palette.accent_text
    } else if inline.image {
        palette.fg_muted
    } else {
        plain
    };
    TextRun {
        len,
        font,
        color,
        background_color: inline.code.then_some(palette.raised),
        underline: None,
        strikethrough: inline.strikethrough.then_some(StrikethroughStyle {
            thickness: px(1.0),
            color: Some(color),
        }),
    }
}

/// The runs of a code block: its highlighted ranges in their syntax colors, the rest plain.
fn code_runs(
    text: &str,
    spans: &[ted_syntax::HighlightSpan],
    style: &PreviewStyle,
) -> Vec<TextRun> {
    let plain = style.palette.fg;
    let mut runs = Vec::new();
    let mut at = 0;
    for span in spans {
        let range = span.range.start.max(at)..span.range.end.min(text.len());
        if range.start >= range.end {
            continue;
        }
        if range.start > at {
            runs.push(run(range.start - at, style.mono_font.clone(), plain));
        }
        let color = style.syntax.get(span.highlight).unwrap_or(plain);
        runs.push(run(range.len(), style.mono_font.clone(), color));
        at = range.end;
    }
    if text.len() > at {
        runs.push(run(text.len() - at, style.mono_font.clone(), plain));
    }
    runs
}

fn run(len: usize, font: Font, color: Hsla) -> TextRun {
    TextRun {
        len,
        font,
        color,
        background_color: None,
        underline: None,
        strikethrough: None,
    }
}
