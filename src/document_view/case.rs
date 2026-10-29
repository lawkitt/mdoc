//! A tab's link to its saved case (ADR 0041): mapping changes are saved after
//! a short pause, and a reopened member document gets its mapping back.
use super::{CaseLinkEffect, DocumentView};
use crate::workspace;
use gpui::Context;
use mdoc_pii::restore::ExportedIdentity;
use mdoc_store::cases;

impl DocumentView {
    /// Ask the workspace to save this tab's mapping into its case, once
    /// changes settle.
    pub(crate) fn schedule_case_save(&mut self, cx: &mut Context<Self>) {
        let effect = self.case.mapping_changed();
        self.carry_out_case_link(effect, cx);
    }

    pub(crate) fn carry_out_case_link(
        &mut self,
        effect: Option<CaseLinkEffect>,
        cx: &mut Context<Self>,
    ) {
        match effect {
            None => {}
            Some(CaseLinkEffect::SaveSoon) => {
                self.case_save = Some(cx.spawn(async move |this, cx| {
                    cx.background_executor()
                        .timer(std::time::Duration::from_millis(700))
                        .await;
                    let _ = this.update(cx, |this, cx| {
                        this.case_save = None;
                        cx.emit(workspace::TabEvent::Case(workspace::CaseEvent::Changed));
                    });
                }));
            }
            Some(CaseLinkEffect::CancelSave) => self.case_save = None,
            Some(CaseLinkEffect::Repaint) => cx.notify(),
        }
    }

    /// Leave the case; the tab keeps its live mapping. Returns the case.
    pub(crate) fn leave_case(&mut self, cx: &mut Context<Self>) -> Option<cases::CaseLink> {
        let (link, effect) = self.case.leave();
        self.carry_out_case_link(Some(effect), cx);
        cx.notify();
        link
    }

    /// Case `id` was deleted: a member forgets it.
    pub(crate) fn case_deleted(&mut self, id: &str, cx: &mut Context<Self>) {
        let effect = self.case.case_deleted(id);
        if effect.is_some() {
            self.carry_out_case_link(effect, cx);
            cx.notify();
        }
    }

    /// Whether a mapping change waits to be saved into the case.
    #[cfg(test)]
    pub(crate) fn case_save_pending(&self) -> bool {
        self.case_save.is_some()
    }
    /// Give a freshly loaded member document its case's identities. Alias
    /// tokens in the text become applied replacements that can be restored.
    pub(crate) fn open_case_mapping(
        &mut self,
        link: cases::CaseLink,
        identities: &[ExportedIdentity],
        reserved: &[String],
        cx: &mut Context<Self>,
    ) -> usize {
        let text = self.editor.read(cx).text().to_owned();
        let history = self.editor.read(cx).history_id();
        let review = self.pii.review_mut();
        review.reserve_aliases(reserved);
        let ids = review.seed(identities, &text);
        let linked = review.relink(&text, identities, &ids, history);
        self.case.attach(Some(link));
        self.case.set_names(identities.to_vec());
        self.review_changed(cx);
        cx.notify();
        linked
    }
}
