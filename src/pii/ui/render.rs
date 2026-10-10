//! PII toolbar and popup routing. State stays in the parent controller.
use super::{Popup, ReviewUi};
use crate::{DocumentView, ui};
use gpui::{AnyElement, App, Context, Window, div, prelude::*, px};

impl ReviewUi {
    pub(super) fn reveal_popup_control(
        &self,
        id: impl Into<gpui::ElementId>,
        control: gpui::Stateful<gpui::Div>,
        cx: &App,
    ) -> gpui::Stateful<gpui::Div> {
        let focus = self.popup_control_focus(id, cx);
        ui::reveal_focus(
            control.track_focus(&focus),
            focus,
            self.popup_scroll.clone(),
        )
    }
    pub(super) fn popup_control_focus(
        &self,
        id: impl Into<gpui::ElementId>,
        cx: &App,
    ) -> gpui::FocusHandle {
        self.popup_controls
            .borrow_mut()
            .entry(id.into())
            .or_insert_with(|| cx.focus_handle().tab_stop(true))
            .clone()
    }
}

impl DocumentView {
    pub(crate) fn pii_toolbar_control(&self, cx: &mut Context<Self>) -> AnyElement {
        let theme = self.theme.get();
        let enabled = self.can_copy_markdown();
        div()
            .id("pii-toolbar")
            .flex()
            .items_center()
            .child(
                ui::icon_control(
                    "Pseudonymize",
                    "Pseudonymize",
                    ui::Icon::Anonymous,
                    theme,
                    enabled,
                )
                .on_click(cx.listener(move |this, _, window, cx| {
                    if enabled {
                        this.toggle_pii_review(window, cx);
                    }
                })),
            )
            .children(self.scan_status(cx))
            .into_any_element()
    }
    /// While a scan runs: a spinner and a stage label beside Pseudonymize that
    /// crossfades through the scan's stages (ADR 0032). The scan reports no
    /// progress, so stages advance on a timer and settle on the last ones.
    fn scan_status(&self, cx: &mut Context<Self>) -> Option<AnyElement> {
        const STAGES: [&str; 4] = [
            "Reading…",
            "Analyzing…",
            "Finding names…",
            "Linking mentions…",
        ];
        const SLOT: f32 = 1.8;
        if !self.pii.scanning() {
            return None;
        }
        let theme = self.theme.get();
        let elapsed = self.pii.scan_started.elapsed().as_secs_f32();
        let slot = (elapsed / SLOT) as usize;
        // After one pass, alternate between the last two stages.
        let stage = if slot < STAGES.len() {
            slot
        } else {
            STAGES.len() - 2 + (slot - STAGES.len()) % 2
        };
        let label = div()
            .id(("scan-stage", stage))
            .when(cfg!(test), |v| v.debug_selector(|| "scan-status".into()))
            // Wide enough for the longest stage, so the status keeps its size.
            .w(px(112.))
            .whitespace_nowrap()
            .text_size(px(12.))
            .text_color(theme.pdf_style().header_muted)
            .child(STAGES[stage]);
        // Each stage fades in, holds, and fades out before the next. The
        // spinner redraws every frame, so the fade follows the clock.
        let t = (elapsed % SLOT) / SLOT;
        let fade = 0.12;
        let label = if cx.reduce_motion() {
            label
        } else {
            label.opacity((t / fade).min((1. - t) / fade).clamp(0., 1.))
        };
        let palette = theme.pdf_style();
        // The status is also the way out: hovering swaps it for "Cancel"
        // in the same place; a click stops the scan.
        Some(
            div()
                .id("cancel-pii-scan")
                .when(cfg!(test), |v| {
                    v.debug_selector(|| "cancel-pii-scan".into())
                })
                .group("scan-status")
                .role(gpui::Role::Button)
                .aria_label(format!("{} Cancel pseudonymization", STAGES[stage]))
                .tab_index(0)
                .relative()
                .ml_1()
                .h(px(24.))
                .px(px(6.))
                .rounded_md()
                .cursor_pointer()
                .hover(move |s| s.bg(palette.placeholder_bg))
                .focus_visible(move |s| s.bg(palette.placeholder_bg))
                .on_click(cx.listener(|this, _, window, cx| this.stop_pii_scan(window, cx)))
                .child(
                    div()
                        .h_full()
                        .flex()
                        .items_center()
                        .gap(px(6.))
                        .group_hover("scan-status", |s| s.opacity(0.))
                        .child(ui::spinner("scan-spinner", theme))
                        .child(label),
                )
                .child(
                    div()
                        .absolute()
                        .inset_0()
                        .flex()
                        .items_center()
                        .justify_center()
                        .text_size(px(12.))
                        .font_weight(gpui::FontWeight::MEDIUM)
                        .text_color(palette.header_fg)
                        .opacity(0.)
                        .group_hover("scan-status", |s| s.opacity(1.))
                        .child("Cancel"),
                )
                .into_any_element(),
        )
    }
    pub(crate) fn pii_popup(&self, window: &Window, cx: &mut Context<Self>) -> Option<AnyElement> {
        if let Popup::Choose { ids, selected } = self.pii.popup.as_ref()? {
            return Some(self.annotation_chooser(ids.clone(), *selected, cx));
        }
        self.replacement_popup(window, cx)
    }
}
