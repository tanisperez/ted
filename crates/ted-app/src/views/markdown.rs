//! The views of a Markdown document, as the design shows them: the editor, the editor with the
//! preview at its right, or the preview alone, chosen with a floating toolbar at the top right or
//! with `Ctrl+Shift+V`; and the preview in a tab of its own, which follows the editor.
//!
//! A Markdown document gets its [`MarkdownPreview`] the first time it shows, with the view of the
//! `markdown_view` setting. A document whose language is changed to something else keeps it but
//! shows only the editor and no toolbar.

use gpui_kit::assets::IconName;
use gpui_kit::component::tooltip::Tooltip;
use gpui_kit::component::*;
use gpui_kit::prelude::*;
use gpui_kit::*;
use rust_i18n::t;
use ted_syntax::Language;

use crate::actions::CycleMarkdownView;
use crate::state::markdown_view::MarkdownView;
use crate::theme::{Palette, PaletteExt};

use super::editor::EditorView;
use super::markdown_preview::MarkdownPreview;
use super::root::{AppView, DocumentView};

/// Size of a toolbar button.
const TOOL_WIDTH: f32 = 26.0;
const TOOL_HEIGHT: f32 = 24.0;

impl AppView {
    /// The preview of the active document when it is a Markdown editor, made the first time.
    pub(crate) fn ensure_markdown_preview(&mut self, cx: &mut Context<Self>) {
        let (view, sync) = (
            self.settings.markdown_view,
            self.settings.markdown_sync_scroll,
        );
        let Some(index) = self.documents.active_index() else {
            return;
        };
        let Some(document) = self.documents.get_mut(index) else {
            return;
        };
        if document.markdown.is_some() {
            return;
        }
        let Some(editor) = document.editor().cloned() else {
            return;
        };
        if editor.read(cx).state.syntax.language != Language::Markdown {
            return;
        }
        document.markdown = Some(cx.new(|cx| MarkdownPreview::new(editor, view, sync, cx)));
    }

    /// The preview of the active document, while it is a Markdown editor.
    pub(crate) fn active_markdown(&self, cx: &App) -> Option<Entity<MarkdownPreview>> {
        let document = self.documents.active()?;
        let editor = document.editor()?;
        if editor.read(cx).state.syntax.language != Language::Markdown {
            return None;
        }
        document.markdown.clone()
    }

    /// The editor of a document: alone, or with its preview and the toolbar for Markdown.
    pub(crate) fn render_editor_document(
        &self,
        editor: &Entity<EditorView>,
        cx: &mut Context<Self>,
    ) -> Div {
        let Some(preview) = self.active_markdown(cx) else {
            return div().flex_1().min_h_0().child(editor.clone());
        };
        let view = preview.read(cx).view();
        let palette = cx.palette();
        h_flex()
            .relative()
            .flex_1()
            .min_h_0()
            .when(view.shows_editor(), |row| {
                row.child(div().flex_1().min_w_0().h_full().child(editor.clone()))
            })
            .when(view.shows_preview(), |row| {
                row.child(
                    div()
                        .flex_1()
                        .min_w_0()
                        .h_full()
                        .when(view == MarkdownView::Split, |pane| {
                            pane.border_l_1().border_color(palette.border)
                        })
                        .child(preview.clone()),
                )
            })
            .child(render_toolbar(&preview, view, &palette, cx))
    }

    /// `Ctrl+Shift+V`: the next view of the active Markdown document.
    pub(crate) fn cycle_markdown_view(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if let Some(preview) = self.active_markdown(cx) {
            let next = preview.read(cx).view().next();
            set_view(&preview, next, window, cx);
        }
    }

    /// Opens the preview of the active Markdown document in a tab, or shows the one already
    /// open.
    pub(crate) fn open_markdown_preview_tab(
        &mut self,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let Some(editor) = self
            .active_markdown(cx)
            .map(|p| p.read(cx).editor().clone())
        else {
            return;
        };
        if let Some(index) = self.documents.position(|document| {
            matches!(&document.view, DocumentView::Preview(p) if *p.read(cx).editor() == editor)
        }) {
            self.activate_document(index, window, cx);
            return;
        }
        let name = self
            .documents
            .active()
            .map(|document| document.name())
            .unwrap_or_default();
        let sync = self.settings.markdown_sync_scroll;
        let preview = cx.new(|cx| MarkdownPreview::new(editor, MarkdownView::Preview, sync, cx));
        self.open_document_view(DocumentView::Preview(preview), Some(name), window, cx);
    }

    /// Closes the preview tabs of `editor`, whose document was closed.
    pub(crate) fn close_previews_of(&mut self, editor: &Entity<EditorView>, cx: &App) {
        while let Some(index) = self.documents.position(|document| {
            matches!(&document.view, DocumentView::Preview(p) if p.read(cx).editor() == editor)
        }) {
            self.documents.close(index);
        }
    }

    /// The settings changed: the previews follow the editor or stop.
    pub(crate) fn apply_markdown_settings(&mut self, cx: &mut Context<Self>) {
        let sync = self.settings.markdown_sync_scroll;
        let previews: Vec<Entity<MarkdownPreview>> = self
            .documents
            .items()
            .iter()
            .filter_map(|document| match &document.view {
                DocumentView::Preview(preview) => Some(preview.clone()),
                _ => document.markdown.clone(),
            })
            .collect();
        for preview in previews {
            preview.update(cx, |preview, cx| preview.set_sync_scroll(sync, cx));
        }
    }
}

/// Shows `view` and moves the focus where the keys should go: the preview when the editor is
/// hidden, the editor otherwise.
fn set_view(
    preview: &Entity<MarkdownPreview>,
    view: MarkdownView,
    window: &mut Window,
    cx: &mut App,
) {
    preview.update(cx, |preview, cx| preview.set_view(view, cx));
    let focus = match view {
        MarkdownView::Preview => preview.read(cx).focus_handle(cx),
        _ => preview.read(cx).editor().read(cx).focus_handle().clone(),
    };
    focus.focus(window, cx);
}

/// The floating toolbar at the top right: the three views and "open the preview in a tab".
fn render_toolbar(
    preview: &Entity<MarkdownPreview>,
    current: MarkdownView,
    palette: &Palette,
    cx: &mut Context<AppView>,
) -> impl IntoElement {
    let hover = palette.hover;
    let tools = MarkdownView::ALL.into_iter().map(|view| {
        let selected = view == current;
        let icon = match view {
            MarkdownView::Editor => IconName::Code,
            MarkdownView::Split => IconName::Columns2,
            MarkdownView::Preview => IconName::Eye,
        };
        let tooltip = SharedString::from(view.tooltip());
        let preview = preview.clone();
        tool(
            SharedString::from(format!("markdown-view-{view:?}")),
            icon,
            if selected {
                palette.accent_text
            } else {
                palette.fg_muted
            },
            hover,
        )
        .when(selected, |button| button.bg(palette.accent_subtle))
        .tooltip(move |window, cx| {
            Tooltip::new(tooltip.clone())
                .action(&CycleMarkdownView, None)
                .build(window, cx)
        })
        .on_click(move |_, window, cx| set_view(&preview, view, window, cx))
    });
    let open_tab = SharedString::from(t!("markdown.open_in_tab"));
    div()
        .absolute()
        .top(px(10.0))
        .right(px(22.0))
        .flex()
        .items_center()
        .gap(px(2.0))
        .p(px(3.0))
        .rounded(px(8.0))
        .bg(palette.overlay)
        .border_1()
        .border_color(palette.border)
        .shadow(vec![
            BoxShadow::new(px(0.0), px(4.0), gpui_kit::black().opacity(0.22)).blur_radius(px(14.0)),
        ])
        .children(tools)
        .child(div().w(px(1.0)).h(px(16.0)).mx(px(3.0)).bg(palette.border))
        .child(
            tool(
                "markdown-open-tab".into(),
                IconName::SquareArrowOutUpRight,
                palette.fg_muted,
                hover,
            )
            .tooltip(move |window, cx| Tooltip::new(open_tab.clone()).build(window, cx))
            .on_click(
                cx.listener(|view, _, window, cx| view.open_markdown_preview_tab(window, cx)),
            ),
        )
}

/// One toolbar button.
fn tool(id: SharedString, icon: IconName, color: Hsla, hover: Hsla) -> Stateful<Div> {
    div()
        .id(id)
        .w(px(TOOL_WIDTH))
        .h(px(TOOL_HEIGHT))
        .flex()
        .items_center()
        .justify_center()
        .rounded(px(5.0))
        .cursor_pointer()
        .text_color(color)
        .hover(move |style| style.bg(hover))
        .child(Icon::new(icon).with_size(px(14.0)).text_color(color))
}
