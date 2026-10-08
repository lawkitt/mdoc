//! Immutable scan jobs and generation/revision/configuration validation.
use super::*;
/// A review scan proposes edits; the explicit Anonymize action applies them.
#[derive(Clone, Copy)]
pub(super) enum ScanIntent {
    Review(Mode),
    Anonymize,
}
impl ScanIntent {
    pub(super) fn mode(self) -> Mode {
        match self {
            Self::Review(mode) => mode,
            Self::Anonymize => Mode::Anonymize,
        }
    }
}
pub(super) struct ScanJob {
    pub(super) cancel: Arc<AtomicBool>,
    pub(super) revision: u64,
    pub(super) generation: u64,
    pub(super) config: settings::PiiConfig,
    pub(super) intent: ScanIntent,
}

impl Workspace {
    pub(super) fn start_pii_scan(&mut self, intent: ScanIntent, cx: &mut Context<Self>) {
        if !self.can_copy_markdown() {
            return;
        }
        self.pseudonymization.cancel();
        self.pseudonymization.error = None;
        self.pseudonymization.completion = None;
        self.pseudonymization.popup = None;
        self.pseudonymization.review.open = true;
        if matches!(intent, ScanIntent::Review(_)) {
            self.pseudonymization.mapping.begin_review();
        }
        let editor = self.editor.read(cx);
        let revision = editor.revision();
        let source = editor.text().to_owned();
        let config = match self.preferences.borrow().snapshot() {
            Ok(p) => p.pseudonymization,
            Err(e) => {
                self.pseudonymization.error = Some(e);
                cx.notify();
                return;
            }
        };
        let generation = self.pseudonymization.generation;
        let identity = self.session.generation;
        let cancel = Arc::new(AtomicBool::new(false));
        self.pseudonymization.job = Some(ScanJob {
            cancel: cancel.clone(),
            revision,
            generation,
            config: config.clone(),
            intent,
        });
        let task = cx
            .background_executor()
            .spawn(async move { detector::scan_config(&source, &cancel, &config) });
        cx.spawn(async move |this, cx| {
            let result = task.await;
            let _ = this.update(cx, |this, cx| {
                this.complete_pseudonym_scan(generation, identity, revision, result, cx)
            });
        })
        .detach();
        cx.notify();
    }
    pub(super) fn complete_pseudonym_scan(
        &mut self,
        generation: u64,
        identity: u64,
        revision: u64,
        result: Result<Vec<pseudonymization::Detection>, String>,
        cx: &mut Context<Self>,
    ) {
        let Some(job) = &self.pseudonymization.job else {
            return;
        };
        if job.generation != generation
            || job.revision != revision
            || self.session.generation != identity
            || self.editor.read(cx).revision() != revision
            || job.intent.mode() != self.pseudonymization.review.mode
            || job.cancel.load(Ordering::Relaxed)
        {
            return;
        }
        let config = job.config.clone();
        let intent = job.intent;
        self.pseudonymization.job = None;
        if matches!(intent, ScanIntent::Review(_)) {
            self.pseudonymization.mapping.open = true;
        }
        let source = self.editor.read(cx).text().to_owned();
        match result.and_then(|detections| self.pseudonymization.review.ingest(&source, detections))
        {
            Ok(()) => {
                self.pseudonymization.error = None;
                self.model_panel.update(cx, |panel, cx| {
                    let model = settings::Model::Pii(config.model);
                    let index = settings::Model::ALL
                        .iter()
                        .position(|m| *m == model)
                        .unwrap();
                    panel.statuses[index] = settings_ui::Status::Ready;
                    cx.notify();
                });
                self.pseudonymization.scans.push(config);
                if matches!(intent, ScanIntent::Anonymize) {
                    match self.commit_all_pii(None, cx) {
                        Ok(count) => {
                            self.pseudonymization.completion =
                                Some((count, self.editor.read(cx).revision()))
                        }
                        Err(error) => self.pseudonymization.error = Some(error),
                    }
                }
                self.sync_annotations(cx);
            }
            Err(error) => {
                self.pseudonymization.error = Some(error);
            }
        }
        cx.notify();
    }
}
