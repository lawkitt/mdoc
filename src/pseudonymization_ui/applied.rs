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
        let category = occurrence.step.category.label();
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
        let matching = if count == 1 {
            "1 matching original".to_owned()
        } else {
            format!("{count} matching originals")
        };
        let anchor = self.editor.read(cx).annotation_bounds(APPLIED_ID | id);
        let position = anchor
            .map(|b| gpui::point(b.left(), b.bottom() + px(4.)))
            .unwrap_or_else(|| self.scroll.bounds().origin + gpui::point(px(24.), px(24.)));
        let palette = self.theme.get().pdf_style();
        Some(deferred(anchored().position(position).snap_to_window().child(
            div().id("applied-pii-popup").key_context("PseudonymReview UiPanel").tab_group().tab_stop(false)
            .track_focus(&self.pseudonymization.focus).track_scroll(&self.pseudonymization.popup_scroll)
            .occlude().w(px(340.)).max_w_full().max_h((window.viewport_size().height-px(40.)).max(px(120.)))
            .overflow_y_scroll().p_3().rounded_md().shadow_md().bg(palette.bg).border_1().border_color(palette.border).text_size(px(13.))
            .on_action(cx.listener(Self::restore_pii)).on_action(cx.listener(Self::restore_all_pii))
            .on_action(cx.listener(Self::close_pseudonym_popup)).on_action(cx.listener(|this,_:&AcceptPseudonymCandidate,window,cx| this.restore_pii(&RestorePii,window,cx)))
            .on_action(cx.listener(|this,_:&ui::NextControl,window,cx| { ui::cycle(window,cx,Some(&this.pseudonymization.focus),false);cx.stop_propagation(); }))
            .on_action(cx.listener(|this,_:&ui::PreviousControl,window,cx| { ui::cycle(window,cx,Some(&this.pseudonymization.focus),true);cx.stop_propagation(); }))
            .on_mouse_down_out(cx.listener(|this,_,_,cx| { this.pseudonymization.popup=None;cx.notify(); }))
            .child(div().flex().items_center().gap_2().child(format!("{category} · {matching}")).child(div().flex_1())
                .child(self.pseudonymization.reveal_popup_control("restore-close",button("×",ClosePseudonymPopup,self.theme.get()),cx)))
            .child(div().py_2().text_color(palette.header_muted).child("Replaced value"))
            .child(div().id("restoration-original").when(cfg!(test),|v|v.debug_selector(||"restoration-original".into())).child(before))
            .child(div().py_2().child(format!("Current: {after}")))
            .when_some(self.pseudonymization.error.clone(),|v,error| v.child(div().py_1().child(error)))
            .child(div().py_2().child(self.pseudonymization.reveal_popup_control("restore-this",button("Restore this occurrence",RestorePii,self.theme.get()),cx)))
            .when(count>1,|v| v.child(div().py_1().child(self.pseudonymization.reveal_popup_control("restore-all",button("Restore all matching originals",RestoreAllPii,self.theme.get()),cx))))
            .child(div().pt_2().text_size(px(11.)).text_color(palette.header_muted).child("Restored values are kept on the next scan. Originals remain available only while this document is open."))
        )).with_priority(2).into_any_element())
    }
}
