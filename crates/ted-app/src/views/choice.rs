//! A small modal question with several answers ("Save the changes?": Save, Don't save, Cancel),
//! in telaio's shared modal frame, so it looks like the rest of the app instead of the platform's
//! plain prompt. The answer comes back through a channel, for code that awaits it.

use gpui_kit::component::{WindowExt as _, v_flex};
use gpui_kit::prelude::*;
use gpui_kit::*;
use telaio_ui::components::{
    DangerButton, ModalPlacement, PrimaryButton, SecondaryButton, framed_dialog, modal_body,
    modal_footer, modal_header,
};

use crate::theme::PaletteExt;
use crate::theme::metrics::MODAL_WIDTH_DEFAULT;

/// How an answer's button looks.
#[derive(Clone, Copy, PartialEq, Eq)]
pub enum ChoiceStyle {
    /// The suggested answer, also chosen with Enter. One per question.
    Primary,
    Secondary,
    /// An answer that loses something (Don't save).
    Danger,
}

/// One answer: its label and its look.
#[derive(Clone)]
pub struct Choice {
    pub label: SharedString,
    pub style: ChoiceStyle,
}

impl Choice {
    pub fn new(label: impl Into<SharedString>, style: ChoiceStyle) -> Self {
        Self {
            label: label.into(),
            style,
        }
    }
}

/// Opens the question. The receiver gets the index of the answer picked, in `choices`' order;
/// closing the dialog any other way (Escape, the close button) closes the channel, which callers
/// treat as Cancel.
pub fn ask(
    window: &mut Window,
    cx: &mut App,
    title: impl Into<SharedString>,
    text: impl Into<SharedString>,
    choices: Vec<Choice>,
) -> async_channel::Receiver<usize> {
    let (sender, receiver) = async_channel::bounded(1);
    let title = title.into();
    let text = text.into();
    let primary = choices
        .iter()
        .position(|choice| choice.style == ChoiceStyle::Primary);
    window.open_dialog(cx, move |dialog, window, _cx| {
        let on_ok = sender.clone();
        let dialog = dialog.overlay_closable(false).on_ok(move |_, _, _| {
            if let Some(index) = primary {
                let _ = on_ok.try_send(index);
            }
            true
        });
        let (title, text, choices, sender) =
            (title.clone(), text.clone(), choices.clone(), sender.clone());
        framed_dialog(
            dialog,
            MODAL_WIDTH_DEFAULT,
            None,
            ModalPlacement::DEFAULT,
            window,
            move |_window, cx| {
                let palette = cx.palette();
                let buttons = choices.iter().enumerate().map(|(index, choice)| {
                    let sender = sender.clone();
                    let on_click = move |_: &ClickEvent, window: &mut Window, cx: &mut App| {
                        let _ = sender.try_send(index);
                        window.close_dialog(cx);
                    };
                    let id = ("choice", index);
                    match choice.style {
                        ChoiceStyle::Primary => PrimaryButton::new(id, choice.label.clone())
                            .on_click(on_click)
                            .into_any_element(),
                        ChoiceStyle::Secondary => SecondaryButton::new(id, choice.label.clone())
                            .on_click(on_click)
                            .into_any_element(),
                        ChoiceStyle::Danger => DangerButton::new(id, choice.label.clone())
                            .on_click(on_click)
                            .into_any_element(),
                    }
                });
                v_flex()
                    .child(modal_header(title.clone(), cx))
                    .child(
                        modal_body().child(div().text_color(palette.fg_muted).child(text.clone())),
                    )
                    .child(modal_footer(cx).children(buttons))
                    .into_any_element()
            },
        )
    });
    receiver
}
