//! The language menu of the status bar: a click on the language of the document opens telaio's
//! searchable menu with every language ted highlights ([`Language::ALL`]), the current one
//! checked. Choosing one highlights the document as that language until it is closed, whatever
//! its file name says (`Syntax::set_language`). The rows are rebuilt when the menu opens, never in
//! render.

use gpui_kit::assets::IconName;
use gpui_kit::prelude::*;
use gpui_kit::*;
use rust_i18n::t;
use ted_syntax::Language;
use telaio_ui::components::{MenuGroup, MenuRow, SearchableMenuEvent, SearchableMenuState};

use crate::state::status::language_label;
use crate::theme::PaletteExt;

use super::root::AppView;

/// Builds the menu's state, once per window.
pub(crate) fn new_language_menu(
    window: &mut Window,
    cx: &mut Context<AppView>,
) -> Entity<SearchableMenuState> {
    let menu = cx.new(|cx| {
        SearchableMenuState::new(window, cx)
            .placeholder(t!("status.language_search"))
            .empty_message(t!("status.language_empty"))
    });
    cx.subscribe_in(
        &menu,
        window,
        |view, _, event: &SearchableMenuEvent, _, cx| {
            if let SearchableMenuEvent::Activate(id) = event
                && let Some(language) = Language::ALL
                    .into_iter()
                    .find(|language| language.name() == id.as_ref())
            {
                view.set_document_language(language, cx);
            }
        },
    )
    .detach();
    // The rows follow the active document: rebuilt each time the menu opens.
    cx.observe(&menu, |view, menu, cx| {
        let open = menu.read(cx).is_open();
        if open && !view.language_menu_open {
            view.language_menu_open = true;
            view.rebuild_language_menu(cx);
        } else if !open {
            view.language_menu_open = false;
        }
    })
    .detach();
    menu
}

impl AppView {
    /// Fills the menu with every language, the active document's one checked.
    fn rebuild_language_menu(&mut self, cx: &mut Context<Self>) {
        let palette = cx.palette();
        let current = self
            .editor()
            .map(|editor| editor.read(cx).state.syntax.language);
        let rows = Language::ALL.into_iter().map(|language| {
            let checked = Some(language) == current;
            let row = MenuRow::new(
                language.name(),
                language_label(language),
                if checked {
                    IconName::Check
                } else {
                    IconName::FileCode
                },
            )
            .search_text(language_label(language));
            if checked {
                row.icon_color(palette.accent_text)
                    .title_color(palette.accent_text)
            } else {
                row
            }
        });
        let groups = vec![MenuGroup::new(t!("status.languages"), rows)];
        self.language_menu
            .update(cx, |menu, cx| menu.set_groups(groups, cx));
    }

    /// Highlights the active document as `language`.
    fn set_document_language(&mut self, language: Language, cx: &mut Context<Self>) {
        if let Some(editor) = self.editor() {
            editor.update(cx, |editor, cx| {
                log::debug!("highlighting the document as {}", language.name());
                editor.state.syntax.set_language(language);
                cx.notify();
            });
            cx.notify();
        }
    }
}
