//! Lazy list for multiple hidden fields sharing a containing visual row.
use super::*;
use crate::{NextPiiChoice, OpenPiiChoice, PreviousPiiChoice, ui};
use gpui::{AnyElement, anchored, deferred, div, prelude::*, uniform_list};
impl Workspace {
    pub(crate) fn choose_annotations(
        &mut self,
        ids: Vec<u64>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if ids.is_empty() {
            return;
        }
        self.pseudonymization.popup = Some(Popup::Choose {
            ids: ids.into(),
            selected: 0,
        });
        self.pseudonymization.popup_previous = window.focused(cx);
        self.pseudonymization
            .chooser_scroll
            .scroll_to_item(0, gpui::ScrollStrategy::Top);
        window.focus(&self.pseudonymization.focus, cx);
        cx.notify();
    }
    fn choose_step(&mut self, backwards: bool, cx: &mut Context<Self>) {
        if let Some(Popup::Choose { ids, selected }) = &mut self.pseudonymization.popup {
            *selected = if backwards {
                selected.checked_sub(1).unwrap_or(ids.len() - 1)
            } else {
                (*selected + 1) % ids.len()
            };
            self.pseudonymization
                .chooser_scroll
                .scroll_to_item(*selected, gpui::ScrollStrategy::Nearest);
            cx.notify();
        }
    }
    fn open_choice(&mut self, _: &OpenPiiChoice, window: &mut Window, cx: &mut Context<Self>) {
        let Some(Popup::Choose { ids, selected }) = &self.pseudonymization.popup else {
            return;
        };
        let id = ids[*selected];
        self.activate_annotation(id, window, cx);
    }
    pub(super) fn annotation_chooser(
        &self,
        ids: Arc<[u64]>,
        selected: usize,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let palette = self.theme.get().pdf_style();
        let position = self
            .editor
            .read(cx)
            .annotation_bounds(ids[0])
            .map(|b| gpui::point(b.left(), b.bottom() + px(4.)))
            .unwrap_or_else(|| self.scroll.bounds().origin + gpui::point(px(24.), px(24.)));
        let count = ids.len();
        deferred(
            anchored().position(position).snap_to_window().child(
                ui::panel("pii-field-chooser", self.theme.get())
                    .when(cfg!(test), |v| {
                        v.debug_selector(|| "pii-field-chooser".into())
                    })
                    .key_context("PiiChooser PseudonymReview UiPanel")
                    .tab_group()
                    .tab_stop(false)
                    .track_focus(&self.pseudonymization.focus)
                    .w(px(340.))
                    .max_w_full()
                    .p_2()
                    .on_action(
                        cx.listener(|this, _: &NextPiiChoice, _, cx| this.choose_step(false, cx)),
                    )
                    .on_action(
                        cx.listener(|this, _: &PreviousPiiChoice, _, cx| {
                            this.choose_step(true, cx)
                        }),
                    )
                    .on_action(cx.listener(Self::open_choice))
                    .on_action(cx.listener(Self::close_pseudonym_popup))
                    .on_mouse_down_out(cx.listener(|this, _, window, cx| {
                        this.close_pseudonym_popup(&ClosePseudonymPopup, window, cx)
                    }))
                    .child(
                        div()
                            .flex()
                            .items_center()
                            .gap_2()
                            .pb_2()
                            .child(
                                div()
                                    .flex_1()
                                    .text_size(px(12.))
                                    .text_color(palette.header_muted)
                                    .child(format!("Choose a field · {count}")),
                            )
                            .child(
                                ui::control("chooser-close", "×", self.theme.get(), true)
                                    .aria_label("Close field chooser")
                                    .on_click(|_, window, cx| {
                                        window.dispatch_action(Box::new(ClosePseudonymPopup), cx)
                                    }),
                            ),
                    )
                    .child(
                        uniform_list(
                            "hidden-pii-fields",
                            count,
                            cx.processor(move |this, range: Range<usize>, _, cx| {
                                range
                                    .map(|index| {
                                        let id = ids[index];
                                        let label = if id & APPLIED_ID != 0 {
                                            this.pseudonymization
                                                .review
                                                .tracking
                                                .get(id & !APPLIED_ID)
                                                .map(|o| {
                                                    format!(
                                                        "{} · {}",
                                                        o.step.category.label(),
                                                        o.step.before
                                                    )
                                                })
                                        } else {
                                            this.pseudonymization.review.group(id >> 32).map(|g| {
                                                format!("{} · {}", g.category.label(), g.original)
                                            })
                                        }
                                        .unwrap_or_else(|| "Field no longer available".into());
                                        let tooltip =
                                            crate::style::tooltip(label.clone(), this.theme.get());
                                        ui::control(
                                            gpui::SharedString::from(format!("field-{id}")),
                                            label,
                                            this.theme.get(),
                                            true,
                                        )
                                        .h(px(30.))
                                        .w_full()
                                        .overflow_hidden()
                                        .truncate()
                                        .tooltip(tooltip)
                                        .when(index == selected, |v| v.bg(palette.placeholder_bg))
                                        .on_click(
                                            cx.listener(move |this, _, window, cx| {
                                                this.activate_annotation(id, window, cx)
                                            }),
                                        )
                                    })
                                    .collect()
                            }),
                        )
                        .h(px(240.))
                        .track_scroll(&self.pseudonymization.chooser_scroll),
                    ),
            ),
        )
        .with_priority(2)
        .into_any_element()
    }
}
