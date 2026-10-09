//! Immutable scan jobs and generation/revision/configuration validation.
use super::*;
/// A scan only proposes candidates; Apply edits the document explicitly.
pub(super) struct ScanJob {
    pub(super) cancel: Arc<AtomicBool>,
    pub(super) revision: u64,
    pub(super) generation: u64,
    pub(super) config: settings::PiiConfig,
}

impl Workspace {
    pub(super) fn start_pii_scan(&mut self, cx: &mut Context<Self>) {
        if !self.can_copy_markdown() {
            return;
        }
        self.pii.cancel();
        self.pii.error = None;
        self.pii.dismiss_popup();
        self.pii.reviewing = true;
        self.pii.mapping.begin_review();
        let editor = self.editor.read(cx);
        let revision = editor.revision();
        let source = editor.text().to_owned();
        let config = match self.preferences.borrow().snapshot() {
            Ok(p) => p.pseudonymization,
            Err(e) => {
                self.pii.error = Some(e);
                cx.notify();
                return;
            }
        };
        // Never start a scan that cannot load its model: ask first (ADR 0026).
        if detector::SUPPORTED
            && self
                .model_panel
                .read(cx)
                .needs_setup(settings::Model::Pii(config.model))
        {
            self.pii.setup = true;
            cx.notify();
            return;
        }
        self.pii.setup = false;
        let generation = self.pii.generation;
        let identity = self.session.generation;
        let cancel = Arc::new(AtomicBool::new(false));
        self.pii.job = Some(ScanJob {
            cancel: cancel.clone(),
            revision,
            generation,
            config: config.clone(),
        });
        let task = cx
            .background_executor()
            .spawn(async move { detector::scan_config(&source, &cancel, &config) });
        cx.spawn(async move |this, cx| {
            let result = task.await;
            let _ = this.update(cx, |this, cx| {
                this.complete_pii_scan(generation, identity, revision, result, cx)
            });
        })
        .detach();
        cx.notify();
    }
    pub(super) fn complete_pii_scan(
        &mut self,
        generation: u64,
        identity: u64,
        revision: u64,
        result: Result<Vec<pii::Detection>, String>,
        cx: &mut Context<Self>,
    ) {
        let Some(job) = &self.pii.job else {
            return;
        };
        if job.generation != generation
            || job.revision != revision
            || self.session.generation != identity
            || self.editor.read(cx).revision() != revision
            || job.cancel.load(Ordering::Relaxed)
        {
            return;
        }
        let config = job.config.clone();
        self.pii.job = None;
        self.pii.mapping.show();
        let source = self.editor.read(cx).text().to_owned();
        match result.and_then(|detections| self.pii.review.ingest(&source, detections)) {
            Ok(()) => {
                self.pii.error = None;
                self.model_panel.update(cx, |panel, cx| {
                    let model = settings::Model::Pii(config.model);
                    panel.statuses[settings_ui::Panel::index(model)] = settings_ui::Status::Ready;
                    cx.notify();
                });
                self.pii.scans.push(config);
                self.sync_annotations(cx);
            }
            Err(error) => {
                self.pii.error = Some(error);
            }
        }
        cx.notify();
    }
}
