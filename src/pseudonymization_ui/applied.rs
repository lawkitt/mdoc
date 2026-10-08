//! Inspection and restoration of committed occurrences; content prepared on activation.
use super::APPLIED_ID;
use crate::{
    AcceptPseudonymCandidate, ClosePseudonymPopup, RestoreAllPii, RestorePii, Workspace, button, ui,
};
use gpui::{AnyElement, Context, Window, anchored, deferred, div, prelude::*, px};
impl Workspace {
    pub(super) fn applied_popup(
        &self,
        id: u64,
        window: &Window,
        cx: &mut Context<Self>,
    ) -> Option<AnyElement> {
        let occurrence = self.pseudonymization.review.tracking.get(id)?;
        let before = occurrence.step.before.to_string();
        let after = occurrence.step.after.to_string();
        let count = self
            .pseudonymization
            .review
            .tracking
            .applied
            .iter()
            .filter(|o| o.step.before == occurrence.step.before)
            .count();
        let anchor = self.editor.read(cx).annotation_bounds(APPLIED_ID | id);
        let position = anchor
            .map(|b| gpui::point(b.left(), b.bottom() + px(4.)))
            .unwrap_or_else(|| self.scroll.bounds().origin + gpui::point(px(24.), px(24.)));
        let palette = self.theme.get().pdf_style();
        Some(
            deferred(
                anchored().position(position).snap_to_window().child(
                    ui::panel("applied-pii-popup", self.theme.get())
                        .when(cfg!(test), |v| {
                            v.debug_selector(|| "applied-pii-popup".into())
                        })
                        .key_context("PseudonymReview UiPanel")
                        .tab_group()
                        .tab_stop(false)
                        .track_focus(&self.pseudonymization.focus)
                        .track_scroll(&self.pseudonymization.popup_scroll)
                        .w(px(340.))
                        .max_w_full()
                        .max_h((window.viewport_size().height - px(40.)).max(px(120.)))
                        .overflow_y_scroll()
                        .p_3()
                        .on_action(cx.listener(Self::restore_pii))
                        .on_action(cx.listener(Self::restore_all_pii))
                        .on_action(cx.listener(Self::close_pseudonym_popup))
                        .on_action(cx.listener(|_, _: &AcceptPseudonymCandidate, _, cx| {
                            cx.stop_propagation();
                        }))
                        .on_action(cx.listener(|this, _: &ui::NextControl, window, cx| {
                            ui::cycle(window, cx, Some(&this.pseudonymization.focus), false);
                            cx.stop_propagation();
                        }))
                        .on_action(cx.listener(|this, _: &ui::PreviousControl, window, cx| {
                            ui::cycle(window, cx, Some(&this.pseudonymization.focus), true);
                            cx.stop_propagation();
                        }))
                        .on_mouse_down_out(cx.listener(|this, _, window, cx| {
                            this.close_pseudonym_popup(&ClosePseudonymPopup, window, cx)
                        }))
                        .child(
                            div()
                                .flex()
                                .items_center()
                                .gap_2()
                                .child(
                                    div()
                                        .flex_1()
                                        .text_size(px(12.))
                                        .text_color(palette.header_muted)
                                        .child("Replacement"),
                                )
                                .child(
                                    self.pseudonymization.reveal_popup_control(
                                        "restore-close",
                                        button("×", ClosePseudonymPopup, self.theme.get())
                                            .aria_label("Close replacement"),
                                        cx,
                                    ),
                                ),
                        )
                        .child(
                            ui::control(
                                "edit-identity",
                                "Edit replacement…",
                                self.theme.get(),
                                true,
                            )
                            .when(cfg!(test), |v| v.debug_selector(|| "edit-identity".into()))
                            .on_click(
                                cx.listener(|this, _, _, cx| this.edit_annotation_identity(cx)),
                            ),
                        )
                        .child(ui::replacement_transition(
                            div()
                                .id("restoration-original")
                                .when(cfg!(test), |v| {
                                    v.debug_selector(|| "restoration-original".into())
                                })
                                .child(before)
                                .into_any_element(),
                            div()
                                .id("restoration-token")
                                .when(cfg!(test), |v| {
                                    v.debug_selector(|| "restoration-token".into())
                                })
                                .child(after)
                                .into_any_element(),
                            self.theme.get(),
                        ))
                        .when_some(self.pseudonymization.error.clone(), |v, error| {
                            v.child(
                                div()
                                    .py_1()
                                    .text_color(
                                        crate::style::markdown_style(self.theme.get())
                                            .alert_warning,
                                    )
                                    .child(error),
                            )
                        })
                        .child(
                            div()
                                .flex()
                                .flex_wrap()
                                .gap_2()
                                .mt_2()
                                .child(
                                    self.pseudonymization.reveal_popup_control(
                                        "restore-this",
                                        ui::action_control(
                                            "restore-this",
                                            "Restore this",
                                            self.theme.get(),
                                            true,
                                            true,
                                        )
                                        .when(cfg!(test), |v| {
                                            v.debug_selector(|| "restore-this".into())
                                        })
                                        .on_click(
                                            |_, window, cx| {
                                                window.dispatch_action(Box::new(RestorePii), cx)
                                            },
                                        ),
                                        cx,
                                    ),
                                )
                                .when(count > 1, |v| {
                                    v.child(
                                        self.pseudonymization.reveal_popup_control(
                                            "restore-all",
                                            ui::action_control(
                                                "restore-all",
                                                format!("Restore all {count} matches"),
                                                self.theme.get(),
                                                true,
                                                false,
                                            )
                                            .when(cfg!(test), |v| {
                                                v.debug_selector(|| "restore-all".into())
                                            })
                                            .on_click(
                                                |_, window, cx| {
                                                    window.dispatch_action(
                                                        Box::new(RestoreAllPii),
                                                        cx,
                                                    )
                                                },
                                            ),
                                            cx,
                                        ),
                                    )
                                }),
                        ),
                ),
            )
            .with_priority(2)
            .into_any_element(),
        )
    }
}
