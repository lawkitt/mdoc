//! The in-app confirm dialog every `window.prompt` uses (ADR 0064). The
//! platform prompt titles itself from its level on Windows ("Warning",
//! "Information"); this one is a dialog named by its question on every
//! platform, keeps Tab inside, and gives focus back where it was.
use crate::{style::Theme, ui};
use gpui::{
    App, Context, EventEmitter, FocusHandle, Focusable, KeyDownEvent, PromptButton, PromptResponse,
    SharedString, Window, actions, div, prelude::*, px,
};
use std::{cell::Cell, rc::Rc};

actions!(confirm_dialog, [Discard]);

/// The dialog's key context.
pub(crate) const DIALOG: &str = "ConfirmDialog";

/// ⌘D / Alt+D answers Discard, from the shortcuts table (ADR 0073).
pub fn bind_keys(cx: &mut App) {
    cx.bind_keys(crate::shortcuts::bindings(crate::shortcuts::Owner::Dialog));
}

/// Replace the platform prompt for the rest of the app's life.
pub fn install(theme: Rc<Cell<Theme>>, cx: &mut App) {
    cx.set_prompt_builder(move |_, message, detail, buttons, handle, window, cx| {
        let view = cx.new(|cx| ConfirmDialog::new(theme.get(), message, detail, buttons, cx));
        let first = view
            .read(cx)
            .buttons
            .first()
            .map(|(_, focus)| focus.clone());
        let handle = handle.with_view(view, window, cx);
        // Focus starts on the first button, the default: Enter takes it.
        if let Some(first) = first {
            window.focus(&first, cx);
        }
        handle
    });
}

pub struct ConfirmDialog {
    theme: Theme,
    message: SharedString,
    detail: Option<SharedString>,
    buttons: Vec<(PromptButton, FocusHandle)>,
    focus: FocusHandle,
}

impl ConfirmDialog {
    fn new(
        theme: Theme,
        message: &str,
        detail: Option<&str>,
        buttons: &[PromptButton],
        cx: &mut Context<Self>,
    ) -> Self {
        Self {
            theme,
            message: message.to_owned().into(),
            detail: detail.map(|detail| detail.to_owned().into()),
            buttons: buttons
                .iter()
                .map(|button| (button.clone(), cx.focus_handle().tab_stop(true)))
                .collect(),
            focus: cx.focus_handle(),
        }
    }

    fn answer(&mut self, index: usize, cx: &mut Context<Self>) {
        cx.emit(PromptResponse(index));
    }

    /// Esc answers Cancel: the button marked so, else one labelled so, else
    /// the last.
    fn cancel_index(&self) -> Option<usize> {
        self.buttons
            .iter()
            .position(|(button, _)| button.is_cancel())
            .or_else(|| self.position("Cancel"))
            .or(self.buttons.len().checked_sub(1))
    }

    fn position(&self, label: &str) -> Option<usize> {
        self.buttons
            .iter()
            .position(|(button, _)| button.label().as_ref() == label)
    }

    fn key_down(&mut self, event: &KeyDownEvent, window: &mut Window, cx: &mut Context<Self>) {
        let keystroke = &event.keystroke;
        let plain = !keystroke.modifiers.modified();
        let index = match keystroke.key.as_str() {
            "escape" => self.cancel_index(),
            // A focused button takes Enter itself; with focus on the dialog
            // (after a click on its text) Enter is the default, the first.
            "enter" if plain && self.focus.is_focused(window) => {
                (!self.buttons.is_empty()).then_some(0)
            }
            _ => None,
        };
        if let Some(index) = index {
            cx.stop_propagation();
            self.answer(index, cx);
        }
    }
}

impl EventEmitter<PromptResponse> for ConfirmDialog {}

impl Focusable for ConfirmDialog {
    fn focus_handle(&self, _: &App) -> FocusHandle {
        self.focus.clone()
    }
}

impl Render for ConfirmDialog {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let theme = self.theme;
        let palette = theme.pdf_style();
        let size = window.viewport_size();
        div()
            .id("confirm-overlay")
            .w(size.width)
            .h(size.height)
            .flex()
            .items_start()
            .justify_center()
            .pt(px(96.))
            .bg(gpui::hsla(0., 0., 0., 0.25))
            // A modal: nothing behind it takes the pointer.
            .occlude()
            .child(
                ui::panel("confirm-dialog", theme)
                    .when(cfg!(test), |v| v.debug_selector(|| "confirm-dialog".into()))
                    .role(gpui::Role::AlertDialog)
                    .aria_label(self.message.clone())
                    .when_some(self.detail.clone(), |v, detail| v.aria_description(detail))
                    .w(px(420.))
                    .p_5()
                    .flex()
                    .flex_col()
                    .gap_3()
                    .key_context(format!("{DIALOG} UiPanel").as_str())
                    // ⌘D / Alt+D: Discard, as in the platform's save sheets.
                    .on_action(cx.listener(|this, _: &Discard, _, cx| {
                        match this
                            .position("Discard")
                            .or_else(|| this.position("Don't Save"))
                        {
                            Some(index) => this.answer(index, cx),
                            None => cx.propagate(),
                        }
                    }))
                    .track_focus(&self.focus)
                    .tab_group()
                    .tab_stop(false)
                    .on_action(cx.listener(|this, _: &ui::NextControl, window, cx| {
                        ui::cycle(window, cx, Some(&this.focus), false);
                        cx.stop_propagation();
                    }))
                    .on_action(cx.listener(|this, _: &ui::PreviousControl, window, cx| {
                        ui::cycle(window, cx, Some(&this.focus), true);
                        cx.stop_propagation();
                    }))
                    .on_key_down(cx.listener(Self::key_down))
                    .child(ui::card_title(self.message.clone()))
                    .when_some(self.detail.clone(), |v, detail| {
                        v.child(ui::card_text(detail, theme))
                    })
                    .child(
                        div().flex().justify_end().gap_2().pt_1().children(
                            self.buttons
                                .iter()
                                .enumerate()
                                .map(|(index, (button, focus))| {
                                    ui::action_control(
                                        ("confirm-button", index),
                                        button.label().clone(),
                                        theme,
                                        true,
                                        index == 0,
                                    )
                                    .when(cfg!(test), |v| {
                                        let label = button.label().to_string();
                                        v.debug_selector(move || format!("confirm-{label}"))
                                    })
                                    .track_focus(focus)
                                    .on_click(
                                        cx.listener(move |this, _, _, cx| this.answer(index, cx)),
                                    )
                                }),
                        ),
                    )
                    .text_color(palette.header_fg),
            )
    }
}
