//! One running candidate refresh and one replaceable latest request.
use super::*;
pub(super) struct DiscoveryJob {
    pub cancel: Arc<AtomicBool>,
    pub ticket: u64,
}
impl Workspace {
    pub(super) fn schedule_pii_discovery(&mut self, cx: &mut Context<Self>) {
        if self.pii.review.variants().is_empty() {
            return;
        }
        if let Some(job) = &self.pii.discovery_job {
            job.cancel.store(true, Ordering::Relaxed);
            self.pii.discovery_pending = true;
            return;
        }
        self.pii.discovery_pending = false;
        let input = self.pii.review.discovery_input();
        let revision = self.editor.read(cx).revision();
        let identity = self.session.generation;
        let ticket = self.pii.generation;
        let cancel = Arc::new(AtomicBool::new(false));
        self.pii.discovery_job = Some(DiscoveryJob {
            cancel: cancel.clone(),
            ticket,
        });
        let task = cx
            .background_executor()
            .spawn(async move { input.run(&cancel) });
        cx.spawn(async move |this, cx| {
            let result = task.await;
            let _ = this.update(cx, |this, cx| {
                if !this
                    .pii
                    .discovery_job
                    .as_ref()
                    .is_some_and(|j| j.ticket == ticket)
                {
                    return;
                }
                this.pii.discovery_job = None;
                if revision == this.editor.read(cx).revision()
                    && identity == this.session.generation
                    && let Some(result) = result
                {
                    this.pii.review.apply_discovery(result);
                    this.sync_annotations(cx);
                    cx.notify();
                }
                if this.pii.discovery_pending {
                    this.schedule_pii_discovery(cx);
                }
            });
        })
        .detach();
    }
}
