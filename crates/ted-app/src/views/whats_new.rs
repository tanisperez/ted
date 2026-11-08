//! The What's new tab, as Postino's: a header with the version and a link to its GitHub release,
//! the release notes rendered as Markdown, and the "Show after an update" checkbox.
//!
//! The notes are embedded in the binary (`state::whats_new::NOTES`). They are parsed once, when
//! the tab opens, so render does no Markdown work.

use gpui_kit::assets::IconName;
use gpui_kit::component::scroll::ScrollbarAxis;
use gpui_kit::component::text::{TextView, TextViewState};
use gpui_kit::component::*;
use gpui_kit::prelude::*;
use gpui_kit::*;
use rust_i18n::t;
use telaio_ui::components::{SecondaryButton, labelled_checkbox};

use crate::state::whats_new::{self, NOTES, VERSION};
use crate::theme::PaletteExt;

use super::root::{AppView, DocumentView};
use super::scrollbars::scrollbar_overlay;

/// Widest the column gets, centered in the tab.
const COLUMN_WIDTH: f32 = 720.0;

/// The view of the What's new tab.
pub(crate) struct WhatsNewView {
    app: WeakEntity<AppView>,
    text: Entity<TextViewState>,
    scroll: ScrollHandle,
}

impl WhatsNewView {
    fn new(app: WeakEntity<AppView>, cx: &mut Context<Self>) -> Self {
        let text = match NOTES {
            Some(notes) => whats_new::link_issues(notes, ted_cli::IDENTITY.repository),
            None => t!("whats_new.no_notes").into_owned(),
        };
        Self {
            app,
            text: cx.new(|cx| TextViewState::markdown(&text, cx)),
            scroll: ScrollHandle::new(),
        }
    }
}

impl AppView {
    /// Opens the What's new tab, or brings it to the front.
    pub(crate) fn open_whats_new(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        log::debug!("opening the what's new tab");
        if let Some(index) = self
            .documents
            .position(|document| matches!(document.view, DocumentView::WhatsNew(_)))
        {
            self.activate_document(index, window, cx);
            cx.notify();
            return;
        }
        let app = cx.weak_entity();
        let view = cx.new(|cx| WhatsNewView::new(app, cx));
        self.open_document_view(
            DocumentView::WhatsNew(view),
            Some(t!("whats_new.tab").into_owned()),
            window,
            cx,
        );
    }

    /// Turns the What's new tab after an update on or off, from Settings or the tab's checkbox.
    pub(crate) fn set_show_whats_new(
        &mut self,
        enabled: bool,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.settings.show_whats_new = enabled;
        self.apply_settings(window, cx);
    }
}

impl Render for WhatsNewView {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let palette = cx.palette();
        let release = whats_new::release_url(ted_cli::IDENTITY.repository, VERSION);
        let header = h_flex()
            .items_start()
            .justify_between()
            .gap(px(16.0))
            .pb(px(20.0))
            .border_b_1()
            .border_color(palette.border)
            .child(
                v_flex()
                    .flex_1()
                    .min_w_0()
                    .gap(px(4.0))
                    .child(
                        div()
                            .text_size(px(24.0))
                            .font_weight(FontWeight::SEMIBOLD)
                            .text_color(palette.fg)
                            .child(t!("whats_new.title", version = VERSION)),
                    )
                    .child(
                        div()
                            .text_size(px(13.0))
                            .text_color(palette.fg_muted)
                            .child(t!("whats_new.subtitle")),
                    ),
            )
            .child(
                SecondaryButton::new("whats-new-release", t!("whats_new.release_page"))
                    .icon(Icon::new(IconName::ExternalLink))
                    .on_click(move |_, _, cx| cx.open_url(&release)),
            );
        let notes = div()
            .pt(px(20.0))
            .text_size(px(14.0))
            .text_color(palette.fg)
            .child(TextView::new(&self.text));
        let checked = self
            .app
            .upgrade()
            .is_some_and(|app| app.read(cx).settings.show_whats_new);
        let app = self.app.clone();
        let footer = h_flex()
            .pt(px(16.0))
            .border_t_1()
            .border_color(palette.border)
            .text_size(px(12.5))
            .child(div().flex_1())
            .child(
                labelled_checkbox(
                    "whats-new-show-after-update",
                    checked,
                    t!("whats_new.show_after_update"),
                    &palette,
                )
                .on_change(move |checked, window, cx| {
                    let checked = *checked;
                    let _ = app.update(cx, |view, cx| {
                        view.set_show_whats_new(checked, window, cx);
                    });
                }),
            );
        let column = v_flex()
            .w_full()
            .max_w(px(COLUMN_WIDTH))
            .pt(px(48.0))
            .px(px(40.0))
            .pb(px(32.0))
            .child(header)
            .child(notes)
            .child(div().flex_1().min_h(px(24.0)))
            .child(footer);
        div()
            .relative()
            .size_full()
            .bg(palette.bg)
            .child(
                div()
                    .id("whats-new-tab")
                    .size_full()
                    .overflow_y_scroll()
                    .track_scroll(&self.scroll)
                    .child(
                        h_flex()
                            .w_full()
                            .min_h_full()
                            .items_stretch()
                            .justify_center()
                            .child(column),
                    ),
            )
            .child(scrollbar_overlay(
                &self.scroll,
                ScrollbarAxis::Vertical,
                "whats-new-scrollbar",
                &palette,
            ))
    }
}
