//! PII toolbar and popup routing. State stays in the parent controller.
use super::{Popup, ReviewUi};
use crate::{document_view::DocumentView, ui};
use gpui::{AnyElement, App, Context, Window, div, prelude::*, px};

impl ReviewUi {
    pub(super) fn reveal_popup_control(
        &self,
        id: impl Into<gpui::ElementId>,
        control: ui::Control,
        cx: &App,
    ) -> ui::Control {
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
    /// While a scan runs: a spinner and a stage label beside Pseudonymize that
    /// crossfades through the scan's stages (ADR 0032). A scan of several
    /// model windows counts them instead once it knows how many (#309).
    pub(crate) fn scan_status(&self, cx: &mut Context<Self>) -> Option<AnyElement> {
        let job = self.pii.job.as_ref()?;
        let theme = self.theme.get();
        let elapsed = self.pii.scan_started.elapsed().as_secs_f32();
        let stage = scan_stage(elapsed, job.progress.get());
        let label = div()
            .id(("scan-stage", stage.key))
            .when(cfg!(test), |v| v.debug_selector(|| "scan-status".into()))
            // Wide enough for the longest stage, so the status keeps its
            // size; a count can be wider.
            .map(|v| match stage.fade {
                Some(_) => v.w(px(112.)),
                None => v.min_w(px(112.)),
            })
            .whitespace_nowrap()
            .text_size(px(12.))
            .text_color(theme.pdf_style().header_muted)
            .child(stage.text.clone());
        // Each timed stage fades in, holds, and fades out before the next.
        // The spinner redraws every frame, so the fade follows the clock.
        let label = match stage.fade {
            Some(opacity) if !cx.reduce_motion() => label.opacity(opacity),
            _ => label,
        };
        let palette = theme.pdf_style();
        // The status is also the way out: hovering swaps it for "Cancel"
        // in the same place; a click stops the scan.
        Some(
            crate::ui::focus_ring("cancel-pii-scan", theme)
                .when(cfg!(test), |v| {
                    v.debug_selector(|| "cancel-pii-scan".into())
                })
                .group("scan-status")
                .role(gpui::Role::Button)
                .aria_label(format!("{} Cancel pseudonymization", stage.text))
                .tab_index(0)
                .relative()
                .ml_1()
                .h(px(24.))
                .px(px(6.))
                .rounded_md()
                // The faint fill alone vanishes on a light header, so the
                // revealed Cancel also gets an outline; the border is always
                // there (transparent) so hovering does not shift the layout.
                .border_1()
                .border_color(gpui::transparent_black())
                .cursor_pointer()
                .hover(move |s| s.bg(palette.placeholder_bg).border_color(palette.border))
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
    pub(crate) fn pii_popup(&self, cx: &mut Context<Self>) -> Option<AnyElement> {
        if let Some(popup) = self.unresolved_popup(cx) {
            return Some(popup);
        }
        if let Popup::Choose { ids, selected } = self.pii.popup.as_ref()? {
            return Some(self.annotation_chooser(ids.clone(), *selected, cx));
        }
        None
    }
    /// The mention bar, laid out beside the editor inside the scrolled
    /// content; `layer` records where that content sits (ADR 0055).
    pub(crate) fn mention_popup_layer(
        &self,
        window: &Window,
        cx: &mut Context<Self>,
    ) -> [AnyElement; 2] {
        let layer = self.pii.popup_layer.clone();
        let scroll = self.scroll.clone();
        [
            gpui::canvas(
                move |bounds, _, _| layer.set(Some((bounds.origin, scroll.offset()))),
                |_, _, _, _| {},
            )
            .absolute()
            .inset_0()
            .into_any_element(),
            // Anchored at the editor's top-left, in its coordinates.
            div()
                .absolute()
                .top_0()
                .left_0()
                .children(self.replacement_popup(window, cx))
                .children(self.selection_menu(window, cx))
                .into_any_element(),
        ]
    }
}

/// What the toolbar says about a running scan.
#[derive(Debug, PartialEq)]
pub(super) struct ScanStage {
    /// Changes with the text, so each stage is its own element.
    pub(super) key: usize,
    pub(super) text: gpui::SharedString,
    /// A timed stage's opacity on its crossfade; a count holds still.
    pub(super) fade: Option<f32>,
}

/// The scan's stage after `elapsed` seconds, from its `(done, windows)`
/// count. Until a scan of several windows has split the source, stages
/// advance on a timer and settle on the last ones (ADR 0032); then the label
/// names the window being read, "Finding names… 3 of 12" (#309).
pub(super) fn scan_stage(elapsed: f32, (done, windows): (usize, usize)) -> ScanStage {
    const STAGES: [&str; 4] = [
        "Reading…",
        "Analyzing…",
        "Finding names…",
        "Linking mentions…",
    ];
    const SLOT: f32 = 1.8;
    if windows > 1 {
        return ScanStage {
            key: STAGES.len(),
            text: format!("Finding names… {} of {windows}", (done + 1).min(windows)).into(),
            fade: None,
        };
    }
    let slot = (elapsed / SLOT) as usize;
    // After one pass, alternate between the last two stages.
    let key = if slot < STAGES.len() {
        slot
    } else {
        STAGES.len() - 2 + (slot - STAGES.len()) % 2
    };
    let t = (elapsed % SLOT) / SLOT;
    let fade = 0.12;
    ScanStage {
        key,
        text: STAGES[key].into(),
        fade: Some((t / fade).min((1. - t) / fade).clamp(0., 1.)),
    }
}
