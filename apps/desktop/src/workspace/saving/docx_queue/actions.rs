//! The DOCX queue's actions: starting the next job and taking its end.

use super::DocxEffect;
use crate::workspace::Workspace;
use gpui::{Context, Window};

impl Workspace {
    pub(in crate::workspace) fn pump_docx(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let tabs = &self.tabs;
        let effect = self
            .docx_queue
            .next(|id| tabs.get(id).is_some_and(|tab| tab.view().is_some()));
        if let Some(DocxEffect::Run(job)) = effect
            && let Some(view) = self.tabs.get(job.id).and_then(|tab| tab.view().cloned())
        {
            view.update(cx, |view, cx| {
                view.run_docx(job.path, job.generation, job.cancel, window, cx)
            });
        }
    }

    pub(in crate::workspace) fn docx_finished(
        &mut self,
        id: u64,
        generation: u64,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.docx_queue.finished(id, generation);
        self.pump_docx(window, cx);
    }
}
