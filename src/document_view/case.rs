//! A tab's link to its saved case (ADR 0041): mapping changes are saved after
//! a short pause, and a reopened member document gets its mapping back.
use super::*;
use crate::pii::restore::ExportedIdentity;

impl DocumentView {
    /// Ask the workspace to save this tab's mapping into its case, once
    /// changes settle.
    pub(crate) fn schedule_case_save(&mut self, cx: &mut Context<Self>) {
        if self.case.is_none() {
            return;
        }
        self.case_save = Some(cx.spawn(async move |this, cx| {
            cx.background_executor()
                .timer(std::time::Duration::from_millis(700))
                .await;
            let _ = this.update(cx, |this, cx| {
                this.case_save = None;
                cx.emit(workspace::TabEvent::CaseChanged);
            });
        }));
    }
    /// Give a freshly loaded member document its case's identities. Alias
    /// tokens in the text become applied replacements that can be restored.
    pub(crate) fn open_case_mapping(
        &mut self,
        link: cases::CaseLink,
        identities: &[ExportedIdentity],
        cx: &mut Context<Self>,
    ) -> usize {
        let text = self.editor.read(cx).text().to_owned();
        let history = self.editor.read(cx).history_id();
        let review = &mut self.pii.review;
        let ids = review.seed(identities, &text);
        let linked = review.relink(&text, identities, &ids, history);
        self.case = Some(link);
        self.sync_pii_theme(cx);
        cx.notify();
        linked
    }
}
