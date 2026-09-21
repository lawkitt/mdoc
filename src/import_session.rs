//! Single pending import/OCR job: busy flags, generation-tagged completions,
//! and the OCR continuation that resumes a conversion after setup.
use crate::import;
use std::path::PathBuf;
use std::sync::{
    Arc,
    atomic::{AtomicBool, Ordering},
};

/// Held by both the tab and its worker, including while OCR dialogs are open.
/// Closing a tab cannot admit new work until its synchronous worker exits.
pub(super) struct ImportPermit(Arc<AtomicBool>);

impl ImportPermit {
    pub fn acquire(busy: &Arc<AtomicBool>) -> Option<Arc<Self>> {
        busy.compare_exchange(false, true, Ordering::AcqRel, Ordering::Relaxed)
            .ok()
            .map(|_| Arc::new(Self(busy.clone())))
    }
}

impl Drop for ImportPermit {
    fn drop(&mut self) {
        self.0.store(false, Ordering::Release);
    }
}

#[derive(Default)]
pub(super) struct ImportSession {
    busy: bool,
    recognizing: bool,
    pending: Option<(u64, Result<import::Imported, import::ImportError>)>,
    ocr_continuation: Option<(u64, PathBuf)>,
}

pub(super) enum ImportCompletion {
    Stale,
    Ready(Result<import::Imported, import::ImportError>),
}

impl ImportSession {
    #[cfg(test)]
    pub fn has_pending(&self) -> bool {
        self.pending.is_some()
    }

    #[cfg(test)]
    pub fn has_ocr_continuation(&self) -> bool {
        self.ocr_continuation.is_some()
    }

    pub fn busy(&self) -> bool {
        self.busy
    }

    pub fn recognizing(&self) -> bool {
        self.recognizing
    }

    pub fn wait_for_ocr(&mut self) {
        self.busy = true;
        self.recognizing = false;
    }

    pub fn defer_for_setup(&mut self, generation: u64, path: PathBuf) {
        self.wait_for_ocr();
        self.ocr_continuation = Some((generation, path));
    }

    /// Installation is global; only resuming the import depends on identity.
    /// Failure calls `finish` instead, retaining the continuation for retry.
    pub fn resume_after_setup(&mut self, generation: u64) -> Option<PathBuf> {
        let (pending_generation, path) = self.ocr_continuation.take()?;
        self.finish();
        (pending_generation == generation).then_some(path)
    }

    /// Admit one job; a second request while busy is refused (existing
    /// import rule, now enforced at the admission point).
    pub fn begin(&mut self, recognizing: bool) -> bool {
        if self.busy {
            return false;
        }
        self.busy = true;
        self.recognizing = recognizing;
        self.pending = None;
        self.ocr_continuation = None;
        true
    }

    pub fn complete(
        &mut self,
        generation: u64,
        result: Result<import::Imported, import::ImportError>,
    ) {
        self.pending = Some((generation, result));
    }

    /// Consume the pending completion while dialogs allow it. Stale
    /// completions (document changed meanwhile) clear busy without mutation.
    pub fn take_ready(&mut self, generation: u64, prompting: bool) -> Option<ImportCompletion> {
        if prompting {
            return None;
        }
        let (pending_generation, result) = self.pending.take()?;
        self.finish();
        Some(if pending_generation == generation {
            ImportCompletion::Ready(result)
        } else {
            ImportCompletion::Stale
        })
    }

    pub fn finish(&mut self) {
        self.busy = false;
        self.recognizing = false;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn worker_keeps_global_slot_until_it_exits_after_tab_closes() {
        let busy = Arc::new(AtomicBool::new(false));
        let tab = ImportPermit::acquire(&busy).unwrap();
        let worker = tab.clone();
        assert!(ImportPermit::acquire(&busy).is_none());
        drop(tab);
        assert!(ImportPermit::acquire(&busy).is_none());
        drop(worker);
        assert!(ImportPermit::acquire(&busy).is_some());
        assert!(!busy.load(Ordering::Relaxed));
    }

    #[test]
    fn completion_waits_for_dialog_and_refuses_second_job() {
        let mut session = ImportSession::default();
        assert!(session.begin(true));
        session.complete(7, Err("conversion failed".into()));
        assert!(!session.begin(false));
        assert!(session.recognizing);
        assert!(session.take_ready(7, true).is_none());
        assert!(session.busy);
        assert!(session.pending.is_some());
        assert!(matches!(
            session.take_ready(7, false),
            Some(ImportCompletion::Ready(Err(_)))
        ));
        assert!(!session.busy);
        assert!(!session.recognizing);
        assert!(session.take_ready(7, false).is_none());
    }

    #[test]
    fn stale_completion_releases_job_without_returning_result() {
        let mut session = ImportSession::default();
        assert!(session.begin(false));
        session.complete(1, Err("old failure".into()));
        assert!(matches!(
            session.take_ready(2, false),
            Some(ImportCompletion::Stale)
        ));
        assert!(!session.busy);
        assert!(session.pending.is_none());
        assert!(session.begin(false));
    }

    #[test]
    fn setup_failure_keeps_retry_continuation_until_next_import() {
        let mut session = ImportSession {
            busy: true,
            ocr_continuation: Some((3, "scan.pdf".into())),
            ..Default::default()
        };
        session.finish();
        assert!(session.ocr_continuation.is_some());
        assert!(session.begin(false));
        assert!(session.ocr_continuation.is_none());
    }

    #[test]
    fn setup_success_consumes_continuation_but_only_resumes_current_document() {
        let mut session = ImportSession::default();
        session.defer_for_setup(3, "scan.pdf".into());
        session.finish(); // Failed installation is retryable.
        assert_eq!(session.resume_after_setup(3), Some("scan.pdf".into()));
        assert!(session.resume_after_setup(3).is_none());
        session.defer_for_setup(4, "old.pdf".into());
        assert!(session.resume_after_setup(5).is_none());
        assert!(!session.busy());
        assert!(session.ocr_continuation.is_none());
    }
}
