//! The in-app confirm dialog every `window.prompt` uses (ADR 0064). The
//! platform prompt titles itself from its level on Windows ("Warning",
//! "Information"); this one is a dialog named by its question on every
//! platform, keeps Tab inside, and gives focus back where it was.
//!
//! Callers list the default button first; the dialog lays the buttons out in
//! the platform's order and fills the default (ADR 0064).
use crate::{style::Theme, ui};
use gpui::{
    App, Context, EventEmitter, FocusHandle, Focusable, KeyDownEvent, PromptButton, PromptResponse,
    SharedString, Window, actions, div, prelude::*, px,
};
use std::{cell::Cell, rc::Rc};

actions!(confirm_dialog, [Discard]);

/// The dialog's key context.
pub(crate) const DIALOG: &str = "ConfirmDialog";

/// ⌘D / Alt+D answers Don't Save, from the shortcuts table (ADR 0073).
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
        // A system quit's questions, and any failure while it saves, come
        // to the front: mdoc may be in the background.
        #[cfg(any(target_os = "macos", test))]
        if crate::system_quit::waiting(cx) {
            cx.activate(true);
            window.activate_window();
        }
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

    /// The buttons in drawing order, before and after the gap.
    fn order(&self) -> (Vec<usize>, Vec<usize>) {
        let labels: Vec<&str> = self
            .buttons
            .iter()
            .map(|(button, _)| button.label().as_ref())
            .collect();
        platform_order(&labels, self.cancel_index(), cfg!(target_os = "macos"))
    }

    /// The default is filled; the others are outlined.
    fn button(&self, index: usize, theme: Theme, cx: &mut Context<Self>) -> ui::Control {
        let (button, focus) = &self.buttons[index];
        let label = button.label().clone();
        let control = if index == 0 {
            ui::filled_button(("confirm-button", index), label, theme, true)
        } else {
            // As tall as the filled default, so the row lines up.
            ui::action_control(("confirm-button", index), label, theme, true, false)
                .h(px(28.))
                .flex()
                .items_center()
        };
        control
            .paint_in_place()
            .when(cfg!(test), |v| {
                let label = button.label().to_string();
                v.debug_selector(move || format!("confirm-{label}"))
            })
            .track_focus(focus)
            .on_click(cx.listener(move |this, _, _, cx| this.answer(index, cx)))
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
            .bg(theme.scrim())
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
                    // ⌘D / Alt+D: Don't Save, as in the platform's save sheets.
                    .on_action(cx.listener(|this, _: &Discard, _, cx| {
                        match this.position("Don't Save") {
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
                    .child({
                        let (leading, trailing) = self.order();
                        let leading: Vec<_> = leading
                            .into_iter()
                            .map(|i| self.button(i, theme, cx))
                            .collect();
                        let trailing: Vec<_> = trailing
                            .into_iter()
                            .map(|i| self.button(i, theme, cx))
                            .collect();
                        div()
                            .flex()
                            .justify_end()
                            .items_center()
                            .gap_2()
                            .pt_1()
                            .children(leading)
                            .when(cfg!(target_os = "macos"), |v| v.child(div().flex_1()))
                            .children(trailing)
                    })
                    .text_color(palette.header_fg),
            )
    }
}

/// Buttons that drop work: on macOS they sit apart, at the left.
fn drops_work(label: &str) -> bool {
    matches!(label, "Don't Save" | "Discard All" | "Close") || label.starts_with("Quit without")
}

/// The platform's button order (ADR 0064), as indices into `labels`, whose
/// first entry is the default: the buttons before the gap, then after it.
///
/// macOS: buttons that drop work at the left; then Cancel, the other
/// choices, and the default at the right (Don't Save … Cancel · Save · Save
/// as Case). Windows: in the caller's order, default first, Cancel last.
fn platform_order(labels: &[&str], cancel: Option<usize>, mac: bool) -> (Vec<usize>, Vec<usize>) {
    if !mac {
        return (Vec::new(), (0..labels.len()).collect());
    }
    let cancel = cancel.filter(|&i| i != 0 && !drops_work(labels[i]));
    let leading = (1..labels.len())
        .filter(|&i| drops_work(labels[i]))
        .collect();
    let others = (1..labels.len())
        .rev()
        .filter(|&i| !drops_work(labels[i]) && Some(i) != cancel);
    let trailing = cancel
        .into_iter()
        .chain(others)
        .chain((!labels.is_empty()).then_some(0))
        .collect();
    (leading, trailing)
}

#[cfg(test)]
mod tests {
    use super::platform_order;

    type Order<'a> = (Vec<&'a str>, Vec<&'a str>);

    fn order<'a>(labels: &[&'a str], cancel: usize, mac: bool) -> Order<'a> {
        let (leading, trailing) = platform_order(labels, Some(cancel), mac);
        let names = |indices: Vec<usize>| indices.into_iter().map(|i| labels[i]).collect();
        (names(leading), names(trailing))
    }

    #[test]
    fn macos_puts_dont_save_left_and_the_default_right() {
        assert_eq!(
            order(&["Save", "Don't Save", "Cancel"], 2, true),
            (vec!["Don't Save"], vec!["Cancel", "Save"])
        );
        assert_eq!(
            order(&["Save as Case", "Save", "Don't Save", "Cancel"], 3, true),
            (vec!["Don't Save"], vec!["Cancel", "Save", "Save as Case"])
        );
        assert_eq!(
            order(&["Review…", "Discard All", "Cancel"], 2, true),
            (vec!["Discard All"], vec!["Cancel", "Review…"])
        );
        assert_eq!(
            order(&["Save as Case", "Close", "Cancel"], 2, true),
            (vec!["Close"], vec!["Cancel", "Save as Case"])
        );
    }

    #[test]
    fn macos_puts_cancel_left_of_the_default() {
        assert_eq!(
            order(&["Remove", "Cancel"], 1, true),
            (vec![], vec!["Cancel", "Remove"])
        );
        // Cancel is the default here: it alone sits at the right.
        assert_eq!(
            order(&["Cancel", "Quit without updating session"], 0, true),
            (vec!["Quit without updating session"], vec!["Cancel"])
        );
        assert_eq!(
            order(
                &["Retry", "Cancel", "Quit without saving mappings"],
                1,
                true
            ),
            (
                vec!["Quit without saving mappings"],
                vec!["Cancel", "Retry"]
            )
        );
    }

    #[test]
    fn windows_keeps_the_callers_order_default_first() {
        let labels = ["Save as Case", "Save", "Don't Save", "Cancel"];
        assert_eq!(order(&labels, 3, false), (vec![], labels.to_vec()));
    }
}
