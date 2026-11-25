//! Desktop's Done for an edit: once typing pauses, the paragraphs edited
//! since they were scanned are scanned as a fresh document is (the name
//! finder with this tab's lists and categories), so a new name becomes a
//! proposal (`mdoc_pii` `edited`). One scan at a time; an edit cancels it.
//! Copy for AI starts it at once and waits for it.
use crate::document_view::DocumentView;
use gpui::{Context, Task};
use mdoc_store::settings;
use std::{
    sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
    },
    time::Duration,
};

/// How long typing pauses before the edited paragraphs are scanned.
// timing: a pause long enough not to load the model on every word
const PAUSE: Duration = Duration::from_millis(1500);

/// The edit scan waiting for the pause, or running.
#[derive(Default)]
pub(crate) struct EditScan {
    pause: Option<Task<()>>,
    cancel: Option<Arc<AtomicBool>>,
    /// All of the text is owed (`owe_whole_text`): its scan is this tab's
    /// Find names.
    whole: bool,
}

impl EditScan {
    /// Waiting or running: what it finds may still change the review.
    pub(crate) fn pending(&self) -> bool {
        self.pause.is_some() || self.cancel.is_some()
    }

    pub(super) fn cancel(&mut self) {
        self.pause = None;
        if let Some(cancel) = self.cancel.take() {
            cancel.store(true, Ordering::Relaxed);
        }
    }
}

/// What scans the edited text in tests, where no model is set up: the
/// rules alone, unless a test gives its own finds.
#[cfg(test)]
pub(crate) static TEST_SCAN: std::sync::Mutex<Option<TestScan>> = std::sync::Mutex::new(None);

#[cfg(test)]
pub(crate) type TestScan = fn(&str) -> Vec<mdoc_pii::Detection>;

impl DocumentView {
    /// Typing went on: scan the edited paragraphs once it pauses.
    pub(super) fn schedule_edit_scan(&mut self, cx: &mut Context<Self>) {
        self.pii.edit_scan.cancel();
        if !self.pii.review.edits_unscanned() {
            return;
        }
        self.pii.edit_scan.pause = Some(cx.spawn(async move |this, cx| {
            cx.background_executor().timer(PAUSE).await;
            let _ = this.update(cx, |this, cx| {
                this.pii.edit_scan.pause = None;
                this.start_edit_scan(cx);
            });
        }));
    }

    /// A Vault document just loaded, its review or names back: saved
    /// unscanned and not Ready, with a review or aliases (a move clears the
    /// review), it may have been left mid-edit, so it owes a scan of all of
    /// it. One never reviewed waits for Find names as before.
    pub(crate) fn owe_if_left_mid_edit(&mut self, cx: &mut Context<Self>) {
        let stored = &self.session.document;
        let reviewed = stored.review.is_some() || !self.pii.review.applied().is_empty();
        if stored.vault.is_some() && !stored.scanned && !stored.ready && reviewed {
            self.owe_whole_text(cx);
        }
    }

    /// None of the text counts as scanned, and its scan follows the pause.
    fn owe_whole_text(&mut self, cx: &mut Context<Self>) {
        self.pii.review.owe_whole_text();
        self.pii.edit_scan.whole = true;
        self.schedule_edit_scan(cx);
    }

    /// Copy for AI: scan the edited paragraphs now rather than after the
    /// pause.
    pub(crate) fn scan_edits_now(&mut self, cx: &mut Context<Self>) {
        self.pii.edit_scan.pause = None;
        if self.pii.edit_scan.cancel.is_none() {
            self.start_edit_scan(cx);
        }
    }

    /// Discovery or a full scan finding names first: an edit scan whose
    /// pause ended meanwhile starts now.
    pub(super) fn resume_edit_scan(&mut self, cx: &mut Context<Self>) {
        if !self.pii.edit_scan.pending() && self.pii.review.edits_unscanned() {
            self.start_edit_scan(cx);
        }
    }

    fn start_edit_scan(&mut self, cx: &mut Context<Self>) {
        // Discovery and a full scan go first; each resumes this when done.
        if self.pii.discovery_job.is_some() || self.pii.scanning() {
            return;
        }
        let Some(edited) = self.pii.review.edited_text() else {
            return;
        };
        let Ok(config) = self
            .preferences
            .borrow()
            .snapshot()
            .map(|p| p.pseudonymization)
        else {
            return;
        };
        #[cfg(not(test))]
        if mdoc_detect::SUPPORTED
            && self
                .model_panel
                .read(cx)
                .needs_setup(settings::Model::Pii(config.model))
        {
            // No name finder: the edits stay owed, and the document not Ready.
            return;
        }
        let lists = self.scan_lists(cx);
        let found_as = (self.pii.edit_scan.whole).then(|| config.clone());
        let revision = self.editor.read(cx).revision();
        let identity = self.session.generation;
        let cancel = Arc::new(AtomicBool::new(false));
        self.pii.edit_scan.cancel = Some(cancel.clone());
        let text = edited.text().to_owned();
        let task = cx.background_executor().spawn({
            let cancel = cancel.clone();
            async move {
                scan(&text, &cancel, &config)
                    .map(|found| mdoc_detect::lists::apply(&text, found, &lists))
            }
        });
        cx.spawn(async move |this, cx| {
            let found = task.await;
            let _ = this.update(cx, |this, cx| {
                if !this
                    .pii
                    .edit_scan
                    .cancel
                    .as_ref()
                    .is_some_and(|c| Arc::ptr_eq(c, &cancel))
                {
                    return;
                }
                this.pii.edit_scan.cancel = None;
                this.take_edit_scan(revision, identity, &edited, found, found_as, cx);
                cx.emit(crate::workspace::TabEvent::NamesFound);
            });
        })
        .detach();
    }

    /// The edit scan's finds, unless the text or document changed since.
    fn take_edit_scan(
        &mut self,
        revision: u64,
        identity: u64,
        edited: &mdoc_pii::EditedText,
        found: Result<Vec<mdoc_pii::Detection>, mdoc_detect::DetectError>,
        found_as: Option<settings::PiiConfig>,
        cx: &mut Context<Self>,
    ) {
        if revision != self.editor.read(cx).revision() || identity != self.session.generation {
            return;
        }
        let mut found = match found {
            Ok(found) => found,
            // Another document holds the name finder: try again after the
            // pause.
            Err(mdoc_detect::DetectError::Busy | mdoc_detect::DetectError::Permit(_)) => {
                self.schedule_edit_scan(cx);
                return;
            }
            // Owed still: Copy for AI tries again, and says why it waits.
            Err(error) => {
                self.pii.error = Some(error.to_string());
                cx.notify();
                return;
            }
        };
        let skip = self.scan_skip();
        found.retain(|d| !skip.contains(&d.category));
        let source = self.editor.read(cx).text().to_owned();
        if (self.pii.review)
            .ingest_edited(&source, edited, found)
            .is_ok_and(|taken| taken)
        {
            if let Some(config) = found_as {
                // All of its text scanned, as Find names would.
                self.pii.edit_scan.whole = false;
                self.pii.scans.push(config);
            }
            self.review_changed(cx);
            self.fade_in_candidates(cx);
            self.schedule_autosave(cx);
            cx.notify();
        }
    }
}

/// The name finder over `text`, as Find names runs it.
#[cfg(not(test))]
fn scan(
    text: &str,
    cancel: &AtomicBool,
    config: &settings::PiiConfig,
) -> Result<Vec<mdoc_pii::Detection>, mdoc_detect::DetectError> {
    mdoc_detect::scan_config(text, cancel, config)
}

#[cfg(test)]
fn scan(
    text: &str,
    _: &AtomicBool,
    _: &settings::PiiConfig,
) -> Result<Vec<mdoc_pii::Detection>, mdoc_detect::DetectError> {
    match *TEST_SCAN.lock().unwrap() {
        Some(scan) => Ok(scan(text)),
        None => mdoc_detect::scan_rules(text),
    }
}
