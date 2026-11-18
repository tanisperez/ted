//! Context menus opened from the keyboard, as Postino's (#232): `Shift+F10` and the menu key open
//! the menu a right click would on the active tab, under it.
//!
//! gpui-component's `context_menu` only opens on a right click, at the pointer. This builds the
//! same menu with the same builder (`tabs::tab_menu`) as a `PopupMenu` of its own, anchored under
//! the tab whose bounds the last frame recorded ([`AppView::tab_bounds`]). The menu takes the
//! focus, so the arrow keys, Enter and Escape work in it, and gives it back to what had it when it
//! closes.

use gpui_kit::component::WindowExt as _;
use gpui_kit::component::menu::PopupMenu;
use gpui_kit::prelude::*;
use gpui_kit::*;
use telaio_ui::components::TabMenuTarget;

use super::root::AppView;
use super::tabs::tab_menu;
use crate::actions::OpenContextMenu;
use crate::state::launch::Target;

/// Drawn over everything else in the window, like gpui-component's own popups.
const MENU_PRIORITY: usize = 1;

/// A context menu opened from the keyboard, while it is open.
pub(crate) struct KeyboardMenu {
    menu: Entity<PopupMenu>,
    position: Point<Pixels>,
    _dismiss: Subscription,
}

impl AppView {
    /// Handles `Shift+F10` and the menu key (see `actions.rs`).
    pub(crate) fn on_open_context_menu(
        &mut self,
        _: &OpenContextMenu,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if window.has_active_dialog(cx) {
            return;
        }
        let (Some(index), Some(bounds)) = (self.documents.active_index(), self.tab_bounds.get())
        else {
            return;
        };
        let Some(document) = self.documents.items().get(index) else {
            return;
        };
        let (id, path) = (document.id, document.path.clone());
        let root = match &self.target {
            Some(Target::Folder(root)) => Some(root.clone()),
            _ => None,
        };
        let target = TabMenuTarget::new(index, &self.dirty_flags(cx));
        let weak = cx.weak_entity();
        self.open_keyboard_menu(
            bounds.bottom_left(),
            move |menu, _, _| tab_menu(menu, id, target, &weak, path.as_deref(), root.as_deref()),
            window,
            cx,
        );
    }

    /// Opens the menu `build` makes at `position`, with the focus in it.
    fn open_keyboard_menu(
        &mut self,
        position: Point<Pixels>,
        build: impl FnOnce(PopupMenu, &mut Window, &mut Context<PopupMenu>) -> PopupMenu + 'static,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        // Where the focus goes back when the menu closes. Without it nothing would be focused,
        // and every window shortcut would stop working until the next click.
        let previous = window
            .focused(cx)
            .unwrap_or_else(|| self.focus_handle.clone());
        let menu = PopupMenu::build(window, cx, move |menu, window, cx| {
            build(menu, window, cx).action_context(previous)
        });
        menu.focus_handle(cx).focus(window, cx);
        let dismiss = cx.subscribe_in(&menu, window, |view, _, _: &DismissEvent, _, cx| {
            view.keyboard_menu = None;
            cx.notify();
        });
        self.keyboard_menu = Some(KeyboardMenu {
            menu,
            position,
            _dismiss: dismiss,
        });
        cx.notify();
    }

    /// The open keyboard menu, over the window.
    pub(crate) fn render_keyboard_menu(&self) -> Option<AnyElement> {
        let open = self.keyboard_menu.as_ref()?;
        Some(
            deferred(
                anchored()
                    .position(open.position)
                    .snap_to_window_with_margin(px(8.0))
                    .child(open.menu.clone()),
            )
            .with_priority(MENU_PRIORITY)
            .into_any_element(),
        )
    }
}
