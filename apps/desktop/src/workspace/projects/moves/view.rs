//! A move's toast: what happened, with Undo.

use crate::ui;
use crate::workspace::Workspace;
use crate::workspace::projects::text;
use gpui::{AnyElement, Context, div, prelude::*, px};

impl Workspace {
    /// The move toast, over the bottom of the content.
    pub(in crate::workspace) fn move_toast(&self, cx: &mut Context<Self>) -> Option<AnyElement> {
        let toast = self.project_ui.toast.as_ref()?;
        let theme = self.theme.get();
        let palette = theme.pdf_style();
        let panel = ui::panel("move-toast", theme)
            .when(cfg!(test), |v| v.debug_selector(|| "move-toast".into()))
            .flex()
            .items_center()
            .gap_3()
            .px_3()
            .py_2()
            .max_w(px(520.))
            .text_size(px(13.))
            .role(gpui::Role::Status)
            .child(div().flex_1().min_w_0().child(toast.text.clone()))
            .when(toast.undo.is_some(), |v| {
                v.child(
                    ui::control("move-toast-undo", text::UNDO, theme, true)
                        .when(cfg!(test), |v| {
                            v.debug_selector(|| "move-toast-undo".into())
                        })
                        .on_click(cx.listener(|this, _, window, cx| this.undo_move(window, cx))),
                )
            })
            .child(
                ui::control("move-toast-close", "×", theme, true)
                    .aria_label(crate::ui::text::DISMISS)
                    .text_color(palette.header_muted)
                    .on_click(cx.listener(|this, _, _, cx| this.dismiss_toast(cx))),
            );
        Some(
            gpui::deferred(
                div()
                    .absolute()
                    .bottom(px(24.))
                    .left_0()
                    .right_0()
                    .flex()
                    .justify_center()
                    .child(panel),
            )
            .with_priority(1)
            .into_any_element(),
        )
    }
}
