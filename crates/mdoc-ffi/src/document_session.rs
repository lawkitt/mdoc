//! `DocumentSession`: one document from intake to the AI and
//! back. Import and OCR through `mdoc_import`, scans through `mdoc_detect`,
//! the review of `mdoc_pii` as snapshots and transitions with undo and redo,
//! typing in edit mode, Copy for AI and Share text,
//! and the document saved into the Vault with its mapping. Long jobs report their stage to the
//! platform and stop when it cancels; the platform runs them off its main
//! thread.
mod actions;
mod editing;
mod intake;
mod origin;
mod resume;
mod review;
mod saving;
mod scanning;
mod snapshot;
mod vault;

use crate::{
    CoreError,
    setup::{DetectorPaths, OcrPaths},
    text::{RestoredReply, TextRange},
};
pub use actions::{HandOff, ProposedName, ReviewAction, SessionInfo, occurrences};
pub use editing::Typed;
#[cfg(test)]
pub(crate) use intake::BEFORE_KEEPING;
pub(crate) use intake::Source;
use mdoc_pii::restore::RestoreMap;
pub use origin::{DocumentOrigin, first_line_title};
pub(crate) use review::Counts;
use review::ReviewText;
pub use review::{Scope, StepKind};
pub use scanning::Thermal;
pub(crate) use scanning::{PhoneScan, check_free_memory};
pub use snapshot::{
    Category, CategoryChoice, DocumentPhase, HistoryStep, IdentityRow, LinkHint, Mention,
    MentionState, ReviewSnapshot,
};
use std::{
    collections::HashSet,
    sync::{
        Arc, Mutex, MutexGuard,
        atomic::{AtomicBool, Ordering},
    },
};
pub(crate) use vault::vault_counts;
pub use vault::{SavedVaultChange, SavedVaultDocument};

/// Where a long job is; the platform shows it.
#[derive(Clone, Copy, Debug, PartialEq, Eq, uniffi::Enum)]
pub enum JobStage {
    Reading,
    Converting,
    /// OCR: loading the models, then recognizing every page.
    Recognizing,
    Scanning,
    /// A scan paused while the phone is at the critical thermal state.
    CoolingDown,
}

/// The platform's view of a long job, and of the device conditions a scan
/// depends on. Called on the job's thread.
#[uniffi::export(callback_interface)]
pub trait JobProgress: Send + Sync {
    fn stage(&self, stage: JobStage);
    /// How far the current stage is: `done` of `total` units (the
    /// detector's windows while scanning, pages while recognizing). A phone
    /// shows it and passes it to the system, which stops a background job
    /// that reports none.
    fn advanced(&self, done: u64, total: u64);
    /// A scan's estimate of the seconds it has left.
    fn remaining(&self, seconds: u64);
    /// The device's thermal state, asked between a scan's windows.
    fn thermal(&self) -> Thermal;
    /// Bytes the app may still allocate, asked when a scan starts (iOS
    /// `os_proc_available_memory`, Android `ActivityManager.MemoryInfo`);
    /// `None` where the platform cannot tell, such as the Simulator.
    fn free_memory(&self) -> Option<u64>;
}

/// How to import a file.
#[derive(Clone, Default, PartialEq, uniffi::Record)]
pub struct ImportOptions {
    /// Recognize pages without a text layer, and images, with these.
    pub ocr: Option<OcrPaths>,
    /// Take a PDF's text layer only, skipping pages that need OCR.
    pub native_text_only: bool,
    /// A locked DOCX's or PDF's password, for this import only: never kept,
    /// and wiped once the import returns.
    #[uniffi(default = None)]
    pub password: Option<String>,
}

/// Never the password.
impl std::fmt::Debug for ImportOptions {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("ImportOptions")
            .field("ocr", &self.ocr)
            .field("native_text_only", &self.native_text_only)
            .field("password", &self.password.as_ref().map(|_| ".."))
            .finish()
    }
}

impl ImportOptions {
    /// Moves the password out into one that wipes itself when dropped.
    fn take_password(&mut self) -> Option<mdoc_import::import::Password> {
        self.password.take().map(mdoc_import::import::Password::new)
    }
}

/// A candidate found by something other than the core's scan: the parity
/// suites inject them, as the desktop's tests do.
#[derive(Clone, Debug, PartialEq, uniffi::Record)]
pub struct Candidate {
    pub range: TextRange,
    pub category: Category,
    pub score: f32,
}

struct Document {
    title: String,
    note: intake::ImportNote,
    review: ReviewText,
    /// An import hands these bytes to the open iOS document. They are never
    /// part of a saved document or a snapshot.
    #[cfg(not(target_os = "android"))]
    unlocked_docx: Option<mdoc_import::import::Source>,
    /// Categories its Project does not detect; scans drop them.
    skip: HashSet<mdoc_pii::Category>,
    /// Bumped when an import replaces the document, so a late scan of the
    /// one before cannot land on it.
    generation: u64,
    /// Scanned, or names proposed some other way, even if none were found.
    scanned: bool,
    /// `ReviewText::change_key` at the last save: equal now, nothing
    /// changed since, and a save would write what is there.
    saved: Option<((u64, u64, usize), bool)>,
    /// A `save_pending_into_vault` joined the Project again, and no save
    /// has told the app yet (`SavedVaultChange::rejoined`).
    rejoined: bool,
    /// How it began, when that matters later: a new document is named
    /// after its first line, the sample restores.
    origin: DocumentOrigin,
    /// The first line's title a new document last took
    /// (`follow_first_line`), so an unchanged line renames nothing.
    followed: Option<String>,
    /// The file family it came from, which its first Vault save records.
    source_kind: mdoc_vault::SourceKind,
    /// What its first Vault save keeps as its Source Original; gone once
    /// kept.
    source: Option<intake::Source>,
    /// Where it is in the Vault, once it joined a Project there or was
    /// saved or opened there (`vault`); `None` before then.
    vault: Option<vault::Place>,
}

#[derive(uniffi::Object)]
pub struct DocumentSession {
    document: Mutex<Option<Document>>,
    job: Mutex<Jobs>,
    generation: Mutex<u64>,
    /// One save at a time (`lock_saving`).
    saving: Mutex<()>,
    /// This phone finds no names (`set_no_name_finder`): a Vault save
    /// with no proposals left counts as reviewed by hand.
    by_hand: AtomicBool,
}

/// The session's jobs, one at a time: a new job stops the one before. Its
/// lock is held only for a moment, never across a job's work nor while
/// waiting for another lock, so `cancel`, `cancel_job` and `next_job`
/// return at once from any thread.
#[derive(Default)]
struct Jobs {
    /// The latest job's cancel flag.
    flag: Arc<AtomicBool>,
    /// Bumped by every job, so a job that ends knows whether it is the latest.
    id: u64,
    /// The latest job has not ended.
    running: bool,
    /// `cancel_job` named the next job before it started: it starts
    /// cancelled.
    pending: Option<u64>,
}

/// A running job's cancel flag; dropping it ends the job.
struct Job<'a> {
    session: &'a DocumentSession,
    id: u64,
    flag: Arc<AtomicBool>,
}

impl std::ops::Deref for Job<'_> {
    type Target = Arc<AtomicBool>;
    fn deref(&self) -> &Arc<AtomicBool> {
        &self.flag
    }
}

impl Drop for Job<'_> {
    fn drop(&mut self) {
        let mut jobs = self.session.lock_job();
        if jobs.id == self.id {
            jobs.running = false;
        }
    }
}

#[uniffi::export]
impl DocumentSession {
    #[uniffi::constructor]
    pub fn new() -> Arc<Self> {
        Arc::new(Self {
            document: Mutex::new(None),
            job: Mutex::new(Jobs::default()),
            generation: Mutex::new(0),
            saving: Mutex::new(()),
            by_hand: AtomicBool::new(false),
        })
    }

    /// Import `path`: PDF, DOCX (in-process) and the other formats desktop
    /// opens, Markdown and plain text as they are, and JPEG images (camera
    /// scans, photos) through OCR. A PDF with pages that need OCR fails with
    /// `CoreError::NeedsOcr` unless `options` say how to treat them; a
    /// locked DOCX or PDF, with `CoreError::NeedsPassword` or
    /// `WrongPassword` until `options` carry its password.
    pub fn import_file(
        &self,
        path: String,
        mut options: ImportOptions,
        progress: Option<Box<dyn JobProgress>>,
    ) -> Result<ReviewSnapshot, CoreError> {
        let cancel = self.start_job();
        let password = options.take_password();
        let imported = intake::import_file(
            std::path::Path::new(&path),
            &options,
            password.as_ref(),
            &cancel,
            progress.as_deref(),
            None,
        )?;
        self.open(imported, &cancel, None)
    }

    /// Pasted or shared text, taken as Markdown.
    pub fn import_text(&self, text: String, title: String) -> Result<ReviewSnapshot, CoreError> {
        self.import_text_as(text, title, DocumentOrigin::Ordinary)
    }

    /// `import_text` for a document that begins in mdoc: a new
    /// document (`DocumentOrigin::New`, its text empty) is named after its
    /// first line until renamed and is discarded when left empty; the
    /// sample (`DocumentOrigin::Sample`) is the one Try a sample restores.
    /// The origin is kept with the document from its first save.
    pub fn import_text_as(
        &self,
        text: String,
        title: String,
        origin: DocumentOrigin,
    ) -> Result<ReviewSnapshot, CoreError> {
        let cancel = self.start_job();
        let imported = intake::Imported {
            title,
            markdown: text,
            source_kind: mdoc_vault::SourceKind::Text,
            source: None,
            note: intake::ImportNote::default(),
            kept: None,
            #[cfg(not(target_os = "android"))]
            unlocked_docx: None,
        };
        self.open(imported, &cancel, Some(Opened::begun(origin)))
    }

    /// Stop the running import or scan; it fails with `CoreError::Cancelled`
    /// at its next checkpoint (model loads and native calls finish first).
    /// With no job running it does nothing, so a cancel that comes late
    /// never stops a later job; to stop a job that may not have started
    /// yet, use `cancel_job(next_job())`.
    pub fn cancel(&self) {
        let jobs = self.lock_job();
        if jobs.running {
            jobs.flag.store(true, Ordering::Relaxed);
        }
    }

    /// The id the next job (import, open or scan) gets: take it just
    /// before the call, and `cancel_job` stops that job whenever the cancel
    /// comes, before it starts or while it runs (#680).
    pub fn next_job(&self) -> u64 {
        self.lock_job().id + 1
    }

    /// Stop job `job` (`next_job`): the running one now, the next one as
    /// it starts; one that ended, or a later one, is left alone.
    pub fn cancel_job(&self, job: u64) {
        let mut jobs = self.lock_job();
        if job == jobs.id && jobs.running {
            jobs.flag.store(true, Ordering::Relaxed);
        } else if job == jobs.id + 1 {
            jobs.pending = Some(job);
        }
    }

    pub fn snapshot(&self) -> Result<ReviewSnapshot, CoreError> {
        self.with_document(|_| Ok(()))
    }

    /// Minutes a scan of this document takes on this device, rounded, for
    /// "About N minutes on this phone" before a scan expected to take more
    /// than about a minute. `None` for a shorter scan, or before the
    /// device's timed run.
    pub fn scan_estimate(&self, detector: DetectorPaths) -> Option<u32> {
        let chars = {
            let document = self.lock_document();
            document.as_ref()?.review.text.chars().count()
        };
        detector.speed?.shown_minutes(chars)
    }

    /// Scan for candidates: the GLiNER2 detector and the structured rules.
    /// Without `detector` the rules run alone: only UI tests seed candidates
    /// that way, since below the detector floor the apps offer no Find
    /// names. A detector scan has no fixed deadline: it stops
    /// only after a minute without progress, refuses to start without
    /// enough free memory, reports the time it has left and pauses while
    /// the phone is too hot. The
    /// review stays usable meanwhile; a transition that changes the text
    /// (Replace, Keep of a replaced mention) or a new import discards the
    /// scan's result, as an edit does on desktop.
    pub fn scan(
        &self,
        detector: Option<DetectorPaths>,
        progress: Option<Box<dyn JobProgress>>,
    ) -> Result<ReviewSnapshot, CoreError> {
        let (source, generation) = {
            let document = self.lock_document();
            let document = document.as_ref().ok_or_else(no_document)?;
            (document.review.text.clone(), document.generation)
        };
        let cancel = self.start_job();
        let progress = progress.as_deref();
        if let Some(progress) = progress {
            progress.stage(JobStage::Scanning);
        }
        let detections = match &detector {
            Some(paths) => {
                scanning::check_free_memory(progress)?;
                mdoc_detect::scan_at(
                    &source,
                    &cancel,
                    std::path::Path::new(&paths.model),
                    std::path::Path::new(&paths.onnx),
                    &mdoc_store::settings::PiiConfig::default(),
                    mdoc_detect::ScanLimit::PHONE,
                    &scanning::PhoneScan::new(progress, paths.speed),
                )?
                .0
            }
            None => mdoc_detect::scan_rules(&source)?,
        };
        if cancel.load(Ordering::Relaxed) {
            return Err(CoreError::Cancelled);
        }
        self.with_document(|document| {
            if document.generation != generation || document.review.text != source {
                return Err(CoreError::review(
                    "The document changed during the scan. Scan it again.",
                ));
            }
            document.ingest(detections)
        })
    }

    /// The structured rules alone (an email address, a link, a tax or
    /// insurance number), proposed as a scan's finds are, on a phone that
    /// has not downloaded its name finder (#1153). Unlike `scan` it leaves
    /// the document unscanned: no names were looked for, so Find names is
    /// still offered, and a save with no proposals left records it Ready
    /// only where `set_no_name_finder` says this phone finds none.
    pub fn propose_rules(&self) -> Result<ReviewSnapshot, CoreError> {
        self.with_document(|document| {
            let mut detections = mdoc_detect::scan_rules(&document.review.text)?;
            detections.retain(|d| !document.skip.contains(&d.category));
            document.review.ingest(detections)
        })
    }

    /// Candidates found elsewhere, ingested as a scan's are.
    pub fn add_candidates(&self, candidates: Vec<Candidate>) -> Result<ReviewSnapshot, CoreError> {
        let detections = candidates
            .into_iter()
            .map(|c| mdoc_pii::Detection {
                range: c.range.start as usize..c.range.end as usize,
                category: c.category.into(),
                score: c.score,
                recognizer: mdoc_pii::Recognizer::Model,
            })
            .collect();
        self.with_document(|document| document.ingest(detections))
    }

    /// Replace the mention at `range` (a snapshot's mention range), or every
    /// mention of its identity.
    pub fn replace(&self, range: TextRange, scope: Scope) -> Result<ReviewSnapshot, CoreError> {
        self.with_document(|document| document.review.replace(span(range), scope))
    }

    /// Replace every proposal.
    pub fn replace_all(&self) -> Result<ReviewSnapshot, CoreError> {
        self.with_document(|document| document.review.replace_all())
    }

    /// Keep the original at `range`, or every mention of its identity.
    pub fn keep(&self, range: TextRange, scope: Scope) -> Result<ReviewSnapshot, CoreError> {
        self.with_document(|document| document.review.keep(span(range), scope))
    }

    /// Add selected text as a proposal of every exact repeat, in `category`
    /// unless the wording is proposed already.
    pub fn add_selection(
        &self,
        range: TextRange,
        category: Option<Category>,
    ) -> Result<ReviewSnapshot, CoreError> {
        self.with_document(|document| {
            document
                .review
                .add_selection(span(range), category.map(Into::into))
                .map(|_| ())
        })
    }

    /// Link identity `from` into identity `into`, which keeps its alias:
    /// Same as… from a merge hint, or for a whole identity. One undo step.
    pub fn link(&self, from: u64, into: u64) -> Result<ReviewSnapshot, CoreError> {
        self.with_document(|document| document.review.link(from, into))
    }

    /// Undo the last step: a decision, a mark or an identity
    /// change. The snapshot's `redo_step` names what it undid and where.
    /// Nothing to undo leaves the review as it is.
    pub fn undo(&self) -> Result<ReviewSnapshot, CoreError> {
        self.with_document(|document| {
            document.review.undo();
            Ok(())
        })
    }

    /// Redo the last undone step; the snapshot's `undo_step` names it.
    pub fn redo(&self) -> Result<ReviewSnapshot, CoreError> {
        self.with_document(|document| {
            document.review.redo();
            Ok(())
        })
    }

    /// Rename…: the alias of the mention at `range`, or of its identity.
    /// This one splits the mention off under the new alias. An alias another
    /// identity has is refused (`identity_with_alias` names it: Same as is
    /// the way to it), as are retired and malformed ones.
    pub fn rename(
        &self,
        range: TextRange,
        scope: Scope,
        alias: String,
    ) -> Result<ReviewSnapshot, CoreError> {
        self.with_document(|document| document.review.rename(span(range), scope, &alias))
    }

    /// The identity already called `alias`, if any: Rename shows "PERSON_2
    /// is Maya Bennett" with Same as instead of an error.
    pub fn identity_with_alias(&self, alias: String) -> Result<Option<u64>, CoreError> {
        self.read_document(|document| document.review.identity_with_alias(&alias))
    }

    /// The category menu for the mention at `range` in `scope`: every
    /// category with the alias choosing it gives.
    pub fn category_choices(
        &self,
        range: TextRange,
        scope: Scope,
    ) -> Result<Vec<CategoryChoice>, CoreError> {
        let mut guard = self.lock_document();
        let document = guard.as_mut().ok_or_else(no_document)?;
        let range = span(range);
        let current = document
            .review
            .at(&range)
            .and_then(|at| document.review.entity(&at))
            .and_then(|id| document.review.review.identity(id))
            .map(|i| i.category);
        Ok(document
            .review
            .category_choices(range, scope)?
            .into_iter()
            .map(|(category, alias)| CategoryChoice {
                category: category.into(),
                alias,
                current: Some(category) == current,
            })
            .collect())
    }

    /// Change the category of the mention at `range`, or of its identity;
    /// This one splits the mention off in the new category.
    pub fn set_category(
        &self,
        range: TextRange,
        scope: Scope,
        category: Category,
    ) -> Result<ReviewSnapshot, CoreError> {
        self.with_document(|document| {
            document
                .review
                .set_category(span(range), scope, category.into())
        })
    }

    /// Same as…: All links the identity of the mention at `range` into
    /// `target`, which keeps its alias and restore wording; This one moves
    /// only this mention to `target`. Nothing is replaced by it.
    pub fn same_as(
        &self,
        range: TextRange,
        scope: Scope,
        target: u64,
    ) -> Result<ReviewSnapshot, CoreError> {
        self.with_document(|document| document.review.same_as(span(range), scope, target))
    }

    /// New alias: the mention at `range` split off as an identity of its own.
    pub fn new_alias(&self, range: TextRange) -> Result<ReviewSnapshot, CoreError> {
        self.with_document(|document| document.review.new_alias(span(range)))
    }

    /// Belongs to…: `owner`, a person or organization, owns contact or
    /// identifier `identity`; `None` clears it.
    pub fn set_owner(
        &self,
        identity: u64,
        owner: Option<u64>,
    ) -> Result<ReviewSnapshot, CoreError> {
        self.with_document(|document| document.review.set_owner(identity, owner))
    }

    /// × on a merge hint: hidden from now on, and in the document's case
    /// once it saves. Not an undo step.
    pub fn dismiss_hint(&self, from: u64, into: u64) -> Result<ReviewSnapshot, CoreError> {
        self.with_document(|document| document.review.dismiss_hint(from, into))
    }

    /// The alias-only note, then the session's text, byte for byte what
    /// desktop Copy for AI would copy once checked (the parity tests).
    /// Unchecked against the Project's names, so never what leaves: the
    /// apps' Copy for AI and Share send `Vault::text_for_ai`.
    pub fn copy_for_ai(&self) -> Result<String, CoreError> {
        self.read_document(|document| document.review.copy_for_ai())
    }

    /// The same text as [`Self::copy_for_ai`], unchecked: not what Share
    /// sends, which is `Vault::text_for_ai`.
    pub fn share_text(&self) -> Result<String, CoreError> {
        self.copy_for_ai()
    }

    /// A name for the shared `.md` file: the title with this document's
    /// known originals replaced by their aliases.
    pub fn share_file_name(&self) -> Result<String, CoreError> {
        self.read_document(Document::share_file_name)
    }

    /// Restore an AI reply with this document's mapping, as desktop's reply
    /// box does.
    pub fn restore_reply(&self, reply: String) -> Result<RestoredReply, CoreError> {
        self.read_document(|document| {
            RestoredReply::of(
                &RestoreMap::new(document.review.review.export_identities()),
                &reply,
            )
        })
    }
}

/// What a document opened from the Vault brings, or what a document that
/// begins in mdoc starts with.
struct Opened {
    origin: DocumentOrigin,
    /// A Vault document and its Project; `None` for one just begun.
    vault: Option<vault::FromVault>,
}

impl Opened {
    /// A document that begins in mdoc, not yet saved.
    fn begun(origin: DocumentOrigin) -> Self {
        Self {
            origin,
            vault: None,
        }
    }
}

#[cfg(not(target_os = "android"))]
#[uniffi::export]
impl DocumentSession {
    /// Hand the import's unlock to the open document. The iOS library can
    /// keep a session after its screen is closed, so the session lets go.
    pub fn take_unlocked_docx(&self) -> Option<Arc<crate::original::UnlockedDocx>> {
        self.lock_document()
            .as_mut()?
            .unlocked_docx
            .take()
            .map(crate::original::UnlockedDocx::new)
    }
}

impl Document {
    /// `DocumentSession::share_file_name`.
    fn share_file_name(&self) -> String {
        let map = RestoreMap::new(self.review.review.export_identities());
        let (title, _) = map.pseudonymize(&self.title);
        let title: String = title
            .chars()
            .map(|c| if "/\\:".contains(c) { '-' } else { c })
            .collect();
        format!("{}.md", title.trim())
    }

    /// Its Project in the Vault: what snapshots and `info` call its case
    /// until Case reads Project there too.
    fn project(&self) -> Option<&str> {
        (self.vault.as_ref()).and_then(|place| place.project.as_deref())
    }

    /// Changes when the review does, or when the document is first scanned.
    fn change_key(&self) -> ((u64, u64, usize), bool) {
        (self.review.change_key(), self.scanned)
    }

    fn snapshot(&mut self) -> ReviewSnapshot {
        self.review.sync_ids();
        snapshot::of(
            &self.title,
            &self.note,
            &self.review,
            self.project(),
            self.scanned,
        )
    }

    /// Scan results, minus the categories the Project skips
    /// (`complete_pii_scan`). The document counts as scanned after.
    fn ingest(&mut self, mut detections: Vec<mdoc_pii::Detection>) -> Result<(), CoreError> {
        detections.retain(|d| !self.skip.contains(&d.category));
        self.review.ingest(detections)?;
        self.scanned = true;
        Ok(())
    }
}

impl DocumentSession {
    /// Make `imported` the session's document, unless its job was
    /// cancelled or a newer job started: then the current one stays. The
    /// document is built first, with no lock held: the job lock is only
    /// ever held for a moment, so `cancel` from the platform's main thread
    /// never waits on a job's work.
    fn open(
        &self,
        imported: intake::Imported,
        cancel: &Job,
        opened: Option<Opened>,
    ) -> Result<ReviewSnapshot, CoreError> {
        let generation = {
            let mut generation = self.generation.lock().unwrap_or_else(|e| e.into_inner());
            *generation += 1;
            *generation
        };
        let imported_kept = imported.kept;
        let mut document = Document {
            title: imported.title,
            note: imported.note,
            review: ReviewText::new(imported.markdown),
            #[cfg(not(target_os = "android"))]
            unlocked_docx: imported.unlocked_docx,
            skip: HashSet::new(),
            generation,
            scanned: false,
            saved: None,
            rejoined: false,
            origin: DocumentOrigin::Ordinary,
            followed: None,
            source_kind: imported.source_kind,
            source: imported.source,
            vault: None,
        };
        if let Some(opened) = opened {
            document.origin = opened.origin;
            if let Some(from) = opened.vault {
                document.open_in_vault(from);
            }
        }
        let snapshot = document.snapshot();
        // The document lock, then the job's check: a newer job that starts
        // after the check opens its document after this one, in this order.
        let mut slot = self.lock_document();
        {
            let jobs = self.lock_job();
            if jobs.id != cancel.id || cancel.load(Ordering::Relaxed) {
                return Err(CoreError::Cancelled);
            }
        }
        *slot = Some(document);
        drop(slot);
        // Taken: an import rejected before this point keeps its OCR pages.
        if let Some(kept) = imported_kept {
            kept.remove();
        }
        Ok(snapshot)
    }

    /// A new job: the one before stops at its next checkpoint, and a
    /// `cancel_job` that named this one before it started stops it.
    fn start_job(&self) -> Job<'_> {
        let mut jobs = self.lock_job();
        jobs.flag.store(true, Ordering::Relaxed);
        jobs.id += 1;
        let cancelled = jobs.pending.take() == Some(jobs.id);
        jobs.flag = Arc::new(AtomicBool::new(cancelled));
        jobs.running = true;
        Job {
            session: self,
            id: jobs.id,
            flag: jobs.flag.clone(),
        }
    }

    fn lock_job(&self) -> MutexGuard<'_, Jobs> {
        self.job.lock().unwrap_or_else(|e| e.into_inner())
    }

    fn lock_document(&self) -> MutexGuard<'_, Option<Document>> {
        self.document.lock().unwrap_or_else(|e| e.into_inner())
    }

    /// Run a transition and return the snapshot after it.
    fn with_document(
        &self,
        change: impl FnOnce(&mut Document) -> Result<(), CoreError>,
    ) -> Result<ReviewSnapshot, CoreError> {
        let mut guard = self.lock_document();
        let document = guard.as_mut().ok_or_else(no_document)?;
        change(document)?;
        Ok(document.snapshot())
    }

    fn read_document<R>(&self, read: impl FnOnce(&Document) -> R) -> Result<R, CoreError> {
        let guard = self.lock_document();
        Ok(read(guard.as_ref().ok_or_else(no_document)?))
    }
}

/// The review as a Vault document record keeps it (ROADMAP Vault & Projects
/// item 7 stores it). Rust only: the platforms never see a saved review.
impl DocumentSession {
    /// The open document's review, saved: what reopening it needs besides
    /// its text and whether it was scanned.
    pub fn saved_review(&self) -> Result<mdoc_pii::saved::SavedReview, CoreError> {
        self.read_document(|document| document.review.saved_review())
    }

    /// Reopen the open document's review as `saved` from this same text,
    /// scanned as `scanned` says, with no rescan. Undo starts afresh; a
    /// document that had nothing to save still has nothing to save.
    pub fn reopen_review(
        &self,
        saved: mdoc_pii::saved::SavedReview,
        scanned: bool,
    ) -> Result<ReviewSnapshot, CoreError> {
        self.with_document(|document| {
            let as_saved = document.saved == Some(document.change_key());
            document.review.reopen(saved)?;
            document.scanned = scanned;
            if as_saved {
                document.saved = Some(document.change_key());
            }
            Ok(())
        })
    }
}

#[cfg(test)]
impl DocumentSession {
    /// Make `source` what the open document's first Vault save keeps, as an
    /// import would: for sources a test cannot import without OCR.
    pub(crate) fn keep_as_original(&self, source: intake::Source) {
        if let Some(document) = self.lock_document().as_mut() {
            document.source = Some(source);
        }
    }
}

fn no_document() -> CoreError {
    CoreError::invalid("Import a document first.")
}

fn span(range: TextRange) -> std::ops::Range<usize> {
    range.start as usize..range.end as usize
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        test_support::Scratch,
        vault::{SharedFrom, Vault},
    };
    use std::path::Path;

    fn vault(scratch: &Scratch) -> Arc<Vault> {
        Vault::open(
            scratch.path().join("vault").display().to_string(),
            vec![3; 32],
        )
        .unwrap()
    }

    /// `reply` restored as one to text shared from Vault document `id`.
    fn restored(vault: &Vault, id: &str, reply: &str) -> String {
        let from = SharedFrom::Document { id: id.into() };
        vault.restore_text(from, reply.into()).unwrap().text
    }

    /// Vault document `id`'s own names (Unfiled), as stored.
    fn stored_names(vault: &Vault, id: &str) -> usize {
        vault.store().document(id).unwrap().mapping.identities.len()
    }

    /// The OCR pages a resuming import keeps, in a folder the test names.
    fn kept_folder(scratch: &Scratch) -> String {
        scratch.path().join("ocr").display().to_string()
    }

    fn session(text: &str, title: &str) -> Arc<DocumentSession> {
        let session = DocumentSession::new();
        session.import_text(text.into(), title.into()).unwrap();
        session
    }

    fn select(session: &DocumentSession, wording: &str) {
        let text = session.snapshot().unwrap().text;
        let start = text.find(wording).unwrap() as u64;
        session
            .add_selection(
                TextRange {
                    start,
                    end: start + wording.len() as u64,
                },
                Some(Category::Person),
            )
            .unwrap();
    }

    /// What a reopened document shows the same: all but the session's own
    /// mention ids and undo history.
    fn reopened_view(mut snapshot: ReviewSnapshot) -> ReviewSnapshot {
        for mention in &mut snapshot.mentions {
            mention.id = 0;
        }
        snapshot.can_undo = false;
        snapshot.can_redo = false;
        snapshot.undo_step = None;
        snapshot.redo_step = None;
        snapshot
    }

    /// Vault & Projects item 2: a review holding every kind of decision —
    /// replaced, proposed, kept, Not a name, a mention split off to its own
    /// identity, a renamed alias — saves through JSON and reopens over its
    /// text to the same snapshot, with no rescan, and carries on as it would
    /// have.
    #[test]
    fn a_saved_review_reopens_as_it_was() {
        let text =
            "Ann Lee met Bob Hart. Ann Lee left. Carl Fox stayed. Dana Kim came. Ann Lee rang.";
        let at = |text: &str, wording: &str, nth: usize| {
            let start = text.match_indices(wording).nth(nth).unwrap().0 as u64;
            TextRange {
                start,
                end: start + wording.len() as u64,
            }
        };
        let session = session(text, "Letter");
        let candidates = ["Ann Lee", "Bob Hart", "Carl Fox", "Dana Kim"]
            .map(|wording| Candidate {
                range: at(text, wording, 0),
                category: Category::Person,
                score: 0.9,
            })
            .to_vec();
        session.add_candidates(candidates).unwrap();
        session
            .replace(at(text, "Bob Hart", 0), Scope::This)
            .unwrap();
        let text = session.snapshot().unwrap().text;
        let bob = session.snapshot().unwrap().mentions[1].range;
        session.rename(bob, Scope::All, "WITNESS".into()).unwrap();
        let text_now = session.snapshot().unwrap().text;
        assert_ne!(text, text_now);
        let text = text_now;
        session.keep(at(&text, "Carl Fox", 0), Scope::This).unwrap();
        let dana = session
            .snapshot()
            .unwrap()
            .mentions
            .into_iter()
            .find(|mention| mention.original == "Dana Kim")
            .unwrap()
            .id;
        session
            .review(ReviewAction::NotAName {
                mention: dana,
                scope: Scope::This,
            })
            .unwrap();
        session.new_alias(at(&text, "Ann Lee", 1)).unwrap();
        let before = session.snapshot().unwrap();
        let states: Vec<_> = before.mentions.iter().map(|m| m.state).collect();
        assert_eq!(
            states,
            [
                MentionState::Proposed,
                MentionState::Replaced,
                MentionState::Proposed,
                MentionState::Kept,
                MentionState::Proposed,
            ],
            "Dana Kim is shown nowhere: {:?}",
            before.mentions
        );
        assert_ne!(
            before.mentions[0].identity, before.mentions[2].identity,
            "the second Ann Lee is an identity of its own"
        );

        let saved = session.saved_review().unwrap();
        let json = serde_json::to_string(&saved).unwrap();
        let reopened = DocumentSession::new();
        reopened
            .import_text(before.text.clone(), "Letter".into())
            .unwrap();
        let after = reopened
            .reopen_review(serde_json::from_str(&json).unwrap(), true)
            .unwrap();
        assert_eq!(reopened_view(after), reopened_view(before.clone()));
        assert_eq!(reopened.saved_review().unwrap(), saved);
        assert!(!reopened.snapshot().unwrap().can_undo);

        // Both carry on alike: the replacement reverts at once, then the
        // rest is replaced.
        for each in [&session, &reopened] {
            each.keep(before.mentions[1].range, Scope::This).unwrap();
        }
        assert_eq!(
            reopened_view(reopened.snapshot().unwrap()),
            reopened_view(session.snapshot().unwrap())
        );
        assert!(reopened.snapshot().unwrap().text.contains("Bob Hart"));
        session.replace_all().unwrap();
        reopened.replace_all().unwrap();
        assert_eq!(
            reopened_view(reopened.snapshot().unwrap()),
            reopened_view(session.snapshot().unwrap())
        );
    }

    /// Cancelling from the progress callback, or starting a newer import
    /// there, leaves the import out: the current document stays.
    #[test]
    fn a_cancelled_or_superseded_import_never_replaces_the_document() {
        struct Then(Arc<DocumentSession>, bool);
        impl JobProgress for Then {
            fn stage(&self, _: JobStage) {
                if self.1 {
                    self.0.cancel();
                } else {
                    self.0.import_text("Newer".into(), "Newer".into()).unwrap();
                }
            }
            fn advanced(&self, _: u64, _: u64) {}
            fn remaining(&self, _: u64) {}
            fn thermal(&self) -> Thermal {
                Thermal::Nominal
            }
            fn free_memory(&self) -> Option<u64> {
                None
            }
        }
        let scratch = Scratch::new();
        let path = scratch.path().join("old.md");
        std::fs::write(&path, "Older").unwrap();
        let session = session("Current", "Current");
        for cancel in [true, false] {
            let result = session.import_file(
                path.display().to_string(),
                ImportOptions::default(),
                Some(Box::new(Then(session.clone(), cancel))),
            );
            assert!(matches!(result, Err(CoreError::Cancelled)), "{result:?}");
        }
        assert_eq!(session.snapshot().unwrap().text, "Newer");
        // A cancel during that job stopped it, and only it.
        let next = session.import_file(path.display().to_string(), ImportOptions::default(), None);
        assert_eq!(next.unwrap().text, "Older");
    }

    /// #680: a cancel for the job about to start stops it, for an import
    /// and for a scan; the job after it runs.
    #[test]
    fn a_cancel_just_before_a_job_stops_it() {
        let scratch = Scratch::new();
        let path = scratch.path().join("old.md");
        std::fs::write(&path, "Ann Lee, ann@example.com.").unwrap();
        let import = |session: &DocumentSession| {
            session.import_file(path.display().to_string(), ImportOptions::default(), None)
        };
        let session = session("Current", "Current");
        session.cancel_job(session.next_job());
        let result = import(&session);
        assert!(matches!(result, Err(CoreError::Cancelled)), "{result:?}");
        assert_eq!(session.snapshot().unwrap().text, "Current");
        import(&session).unwrap();
        session.cancel_job(session.next_job());
        let result = session.scan(None, None);
        assert!(matches!(result, Err(CoreError::Cancelled)), "{result:?}");
        assert!(session.snapshot().unwrap().mentions.is_empty());
        assert_eq!(session.scan(None, None).unwrap().mentions.len(), 1);
    }

    /// A job blocked in its progress callback, until released.
    struct Blocked {
        entered: Mutex<Option<std::sync::mpsc::Sender<()>>>,
        release: Mutex<std::sync::mpsc::Receiver<()>>,
    }

    impl JobProgress for Blocked {
        fn stage(&self, _: JobStage) {
            if let Some(entered) = self.entered.lock().unwrap().take() {
                entered.send(()).unwrap();
                self.release.lock().unwrap().recv().unwrap();
            }
        }
        fn advanced(&self, _: u64, _: u64) {}
        fn remaining(&self, _: u64) {}
        fn thermal(&self) -> Thermal {
            Thermal::Nominal
        }
        fn free_memory(&self) -> Option<u64> {
            None
        }
    }

    /// What the platforms call from their main thread returns at once
    /// while a job runs: Android cancels from it, and a blocked main thread
    /// misses the system's deadlines.
    #[test]
    fn main_thread_calls_never_wait_for_a_running_job() {
        use std::{
            sync::mpsc,
            time::{Duration, Instant},
        };
        let scratch = Scratch::new();
        let path = scratch.path().join("old.md");
        std::fs::write(&path, "Ann Lee wrote.").unwrap();
        let session = session("Current", "Current");
        let (entered, on_enter) = mpsc::channel();
        let (release, on_release) = mpsc::channel();
        let progress = Blocked {
            entered: Mutex::new(Some(entered)),
            release: Mutex::new(on_release),
        };
        let job = {
            let session = session.clone();
            let path = path.display().to_string();
            std::thread::spawn(move || {
                session.import_file(path, ImportOptions::default(), Some(Box::new(progress)))
            })
        };
        on_enter.recv_timeout(Duration::from_secs(10)).unwrap();
        let began = Instant::now();
        session.cancel();
        let next = session.next_job();
        session.cancel_job(next - 1);
        assert_eq!(session.snapshot().unwrap().text, "Current");
        assert_eq!(session.info().unwrap().title, "Current");
        let took = began.elapsed();
        assert!(took < Duration::from_millis(500), "{took:?}");
        release.send(()).unwrap();
        let result = job.join().unwrap();
        assert!(matches!(result, Err(CoreError::Cancelled)), "{result:?}");
        assert_eq!(session.snapshot().unwrap().text, "Current");
    }

    /// The cancel calls never wait for the document, which a save or a
    /// scan's last step holds.
    #[test]
    fn cancel_never_waits_for_the_document() {
        use std::{sync::mpsc, time::Duration};
        let session = session("Current", "Current");
        let held = session.lock_document();
        let (done, finished) = mpsc::channel();
        let other = session.clone();
        std::thread::spawn(move || {
            other.cancel();
            other.cancel_job(other.next_job());
            done.send(()).unwrap();
        });
        finished.recv_timeout(Duration::from_secs(5)).unwrap();
        drop(held);
    }

    /// may while the app saves, never stops the next job.
    #[test]
    fn a_cancel_after_a_job_ended_leaves_the_next_one() {
        let session = DocumentSession::new();
        let job = session.next_job();
        session
            .import_text("Ann Lee, ann@example.com.".into(), "Letter".into())
            .unwrap();
        session.cancel_job(job);
        session.cancel();
        assert_eq!(session.scan(None, None).unwrap().mentions.len(), 1);
        // A ticket for a job two ahead is no ticket.
        session.cancel_job(session.next_job() + 1);
        session.scan(None, None).unwrap();
    }

    /// Save → reopen from the Vault → share → restore → save: the record
    /// stays the same, and its aliases count as replaced.
    #[test]
    fn a_saved_document_reopens_as_saved() {
        let scratch = Scratch::new();
        let vault = vault(&scratch);
        let first = session("Ann Lee wrote to Bob Hart.", "Letter");
        select(&first, "Ann Lee");
        first.replace_all().unwrap();
        let saved = first.save_into_vault(vault.clone()).unwrap();
        let copied = first.copy_for_ai().unwrap();

        let reopened = DocumentSession::new();
        let snapshot = reopened
            .open_from_vault(vault.clone(), saved.id.clone())
            .unwrap();
        assert_eq!(snapshot.case_id, None, "Unfiled");
        assert_eq!(snapshot.replaced, 1);
        assert_eq!(reopened.copy_for_ai().unwrap(), copied);
        assert_eq!(
            reopened
                .restore_reply("PERSON_1 replied.".into())
                .unwrap()
                .text,
            "Ann Lee replied."
        );
        select(&reopened, "Bob Hart");
        reopened.replace_all().unwrap();
        let again = reopened.save_into_vault(vault.clone()).unwrap();
        assert_eq!(again.id, saved.id);
        assert_eq!(vault.documents().len(), 1);
        assert_eq!(
            vault.store().document(&saved.id).unwrap().markdown,
            "PERSON_1 wrote to PERSON_2."
        );
    }

    fn fixture(name: &str) -> String {
        Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../../tests/fixtures")
            .join(name)
            .display()
            .to_string()
    }

    /// The phones' import note counts hidden-formatted text and repeated
    /// header lines left out, as the desktop's does; the hidden text itself
    /// never reaches the document.
    #[test]
    fn the_import_note_counts_hidden_text() {
        for (name, note) in [
            (
                "import/handmade-hidden.docx",
                "7 hidden passages not imported.",
            ),
            (
                "import/handmade-hidden.rtf",
                "6 hidden passages not imported.",
            ),
            (
                "import/handmade-running-header.pdf",
                "1 hidden passage not imported. 3 repeated header and footer lines removed.",
            ),
            // The header's sentence in a sparse page's body stays and isn't
            // counted (#925 M16).
            (
                "import/handmade-sparse-page-header.pdf",
                "4 repeated header and footer lines removed.",
            ),
            // The same on a sparse page after Letter and Legal pages, the
            // sentence where no full page has the header (#925 M17).
            (
                "import/handmade-mixed-size-header.pdf",
                "6 repeated header and footer lines removed.",
            ),
            // The same right under the header on a sparse page: only the
            // match closest to the header's place goes (#925 M18).
            (
                "import/handmade-sparse-page-closest-header.pdf",
                "5 repeated header and footer lines removed.",
            ),
        ] {
            let snapshot = DocumentSession::new()
                .import_file(fixture(name), ImportOptions::default(), None)
                .unwrap();
            assert_eq!(snapshot.warning.as_deref(), Some(note), "{name}");
            assert!(!snapshot.text.contains("SECRET"), "{name}");
            if name.contains("sparse-page") || name.contains("mixed-size") {
                // Page 1's header and the sparse last page's body line.
                let header = "Annual Report of the Board 2026 Confidential";
                assert_eq!(snapshot.text.matches(header).count(), 2, "{name}");
            }
        }
    }

    /// Comments are context for the original, never part of the document:
    /// no comment's text or author (an annotation's, for a PDF) reaches the
    /// text, Copy for AI, Share or the saved file.
    #[test]
    fn comments_never_reach_the_saved_copied_or_shared_text() {
        let scratch = Scratch::new();
        let vault = vault(&scratch);
        for name in [
            "import/handmade-comments.docx",
            "import/handmade-comments.odt",
            "import/handmade-comments.rtf",
            "import/text_annotations.pdf",
        ] {
            let path = fixture(name);
            let source = mdoc_import::import::Source::read(Path::new(&path)).unwrap();
            let comments: Vec<_> = mdoc_import::import::source_comments(&source)
                .into_iter()
                .chain(
                    mdoc_import::import::source_annotations(&source, None)
                        .unwrap_or_default()
                        .iter()
                        .enumerate()
                        .map(|(i, a)| mdoc_import::comments::Comment::from_pdf(i, a)),
                )
                .collect();
            assert!(comments.len() >= 6, "{name}");
            let session = DocumentSession::new();
            let snapshot = session
                .import_file(path, ImportOptions::default(), None)
                .unwrap();
            let saved = session.save_into_vault(vault.clone()).unwrap();
            let carried = [
                snapshot.text,
                session.copy_for_ai().unwrap(),
                session.share_text().unwrap(),
                vault.store().document(&saved.id).unwrap().markdown,
            ];
            for comment in &comments {
                for words in [comment.text.trim(), comment.author.trim()] {
                    if words.is_empty() {
                        continue;
                    }
                    for text in &carried {
                        assert!(!text.contains(words), "{name}: {words:?} was carried");
                    }
                }
            }
        }
    }

    /// Every file under `root`, sorted.
    fn files(root: &Path) -> Vec<std::path::PathBuf> {
        let mut found = Vec::new();
        for entry in std::fs::read_dir(root).unwrap() {
            let path = entry.unwrap().path();
            if path.is_dir() {
                found.extend(files(&path));
            } else {
                found.push(path);
            }
        }
        found.sort();
        found
    }

    /// The encrypted PDF fixtures' user password, a published test value.
    const PDF_PASSWORD: &str = "Petrov-2026";

    /// A locked PDF on the phones (#1024 item 11), AES-128 and AES-256: no
    /// password asks for one, a wrong one asks again, and the right one
    /// gives what its open twin gives. Encryption no password opens says so
    /// whatever is typed, and a PDF locked only against editing opens with
    /// none. Nothing is written beside it or into the app's OCR folder: no
    /// kept pages, no unlocked copy.
    #[test]
    fn a_locked_pdf_imports_with_its_password_only() {
        let scratch = Scratch::new();
        let copy = |name: &str| {
            let path = scratch.path().join(name);
            std::fs::copy(fixture(&format!("pdf-source/{name}")), &path).unwrap();
            path.display().to_string()
        };
        let import = |path: &str, password: Option<&str>| {
            DocumentSession::new().import_file_resuming(
                kept_folder(&scratch),
                path.into(),
                ImportOptions {
                    password: password.map(Into::into),
                    ..ImportOptions::default()
                },
                None,
            )
        };
        let open = DocumentSession::new()
            .import_file(
                fixture("pdf-source/comments.pdf"),
                ImportOptions::default(),
                None,
            )
            .unwrap();
        // The AES-256 copy with its standard security handler swapped for
        // one no reader knows, the same length so every offset still holds.
        let unknown = scratch.path().join("unknown-handler.pdf");
        let mut bytes = std::fs::read(fixture("pdf-source/comments-aes256.pdf")).unwrap();
        let at = bytes
            .windows(b"/Standard".len())
            .position(|window| window == b"/Standard")
            .unwrap();
        bytes[at..at + b"/Nonstand".len()].copy_from_slice(b"/Nonstand");
        std::fs::write(&unknown, bytes).unwrap();
        let unknown = unknown.display().to_string();
        let locked = ["comments-aes128.pdf", "comments-aes256.pdf"].map(copy);
        let owner_only = copy("comments-owner-only.pdf");
        let before = files(scratch.path());
        for path in &locked {
            assert!(
                matches!(import(path, None), Err(CoreError::NeedsPassword)),
                "{path}"
            );
            assert!(
                matches!(
                    import(path, Some("petrov-2026")),
                    Err(CoreError::WrongPassword)
                ),
                "{path}"
            );
            let unlocked = import(path, Some(PDF_PASSWORD)).unwrap();
            assert_eq!(unlocked.text, open.text, "{path}");
            assert_eq!(unlocked.warning, open.warning, "{path}");
            assert_eq!(unlocked.left_out, open.left_out, "{path}");
        }
        for password in [None, Some(PDF_PASSWORD), Some("wrong")] {
            match import(&unknown, password) {
                Err(CoreError::Import { message }) => assert_eq!(
                    message,
                    "This PDF uses encryption mdoc can’t open. Import an unlocked copy."
                ),
                other => panic!("{password:?}: {:?}", other.map(|_| ())),
            }
        }
        let owner_only = import(&owner_only, None).unwrap();
        assert_eq!(owner_only.text, open.text);
        assert_eq!(owner_only.left_out, open.left_out);
        assert_eq!(files(scratch.path()), before);
    }

    /// With the OCR runtime: a locked, partly scanned PDF recognized through
    /// the phones' resumable import keeps no page (#1024 decision 7). A
    /// cancelled run leaves nothing to resume, and the next one, with its
    /// password, gives its open twin's document.
    #[test]
    #[ignore = "downloads OCR runtimes/models; requires Apple Silicon macOS or Windows x64"]
    #[cfg(any(
        all(target_os = "macos", target_arch = "aarch64"),
        all(target_os = "windows", target_arch = "x86_64")
    ))]
    fn a_locked_scan_keeps_no_pages_on_the_phones() {
        struct CancelAtFirstPage(Arc<DocumentSession>);
        impl JobProgress for CancelAtFirstPage {
            fn stage(&self, _: JobStage) {}
            fn advanced(&self, _: u64, _: u64) {
                self.0.cancel();
            }
            fn remaining(&self, _: u64) {}
            fn thermal(&self) -> Thermal {
                Thermal::Nominal
            }
            fn free_memory(&self) -> Option<u64> {
                None
            }
        }
        let scratch = Scratch::new();
        let models = scratch.path().join("models");
        let installed = mdoc_import::ocr::install_config_in(
            &models,
            &mdoc_store::settings::OcrConfig {
                model: mdoc_store::settings::OcrModel::Cyrillic,
                ..Default::default()
            },
            &mdoc_import::model_download::Progress::default(),
        )
        .unwrap();
        let ocr = OcrPaths {
            root: models.display().to_string(),
            model: crate::setup::ModelId::OcrEnglishRussian,
            pdfium: installed.pdfium.display().to_string(),
            onnx: installed.onnx.display().to_string(),
        };
        let options = |password: Option<&str>| ImportOptions {
            ocr: Some(ocr.clone()),
            password: password.map(Into::into),
            ..ImportOptions::default()
        };
        let locked = fixture("pdf-source/partly-scanned-aes256.pdf");
        let session = DocumentSession::new();
        let stopped = session.import_file_resuming(
            kept_folder(&scratch),
            locked.clone(),
            options(Some(PDF_PASSWORD)),
            Some(Box::new(CancelAtFirstPage(session.clone()))),
        );
        assert!(
            matches!(stopped, Err(CoreError::Cancelled)),
            "{:?}",
            stopped.map(|_| ())
        );
        let imports = Path::new(&kept_folder(&scratch)).to_owned();
        assert!(!imports.exists(), "nothing kept");
        let recognized = DocumentSession::new()
            .import_file_resuming(
                kept_folder(&scratch),
                locked,
                options(Some(PDF_PASSWORD)),
                None,
            )
            .unwrap();
        let open = DocumentSession::new()
            .import_file(
                fixture("import/handmade-partly-scanned.pdf"),
                options(None),
                None,
            )
            .unwrap();
        assert_eq!(recognized.text, open.text);
        assert!(!imports.exists(), "nothing kept");
    }

    const PASSWORD: &str = "Petrov-2026";

    /// A locked DOCX, saved by Word and by LibreOffice: no password asks for
    /// one, a wrong one asks again, and the right one gives what its
    /// unlocked twin gives. Nothing is written beside it or into the app's
    /// OCR folder.
    #[test]
    fn a_locked_docx_imports_with_its_password_only() {
        let scratch = Scratch::new();
        for app in ["word", "libreoffice"] {
            let locked = scratch.path().join(format!("{app}-locked.docx"));
            std::fs::copy(
                fixture(&format!("locked-office/{app}-locked.docx")),
                &locked,
            )
            .unwrap();
            let before = files(scratch.path());
            let import = |password: Option<&str>| {
                DocumentSession::new().import_file_resuming(
                    kept_folder(&scratch),
                    locked.display().to_string(),
                    ImportOptions {
                        password: password.map(Into::into),
                        ..ImportOptions::default()
                    },
                    None,
                )
            };
            assert!(
                matches!(import(None), Err(CoreError::NeedsPassword)),
                "{app}"
            );
            assert!(
                matches!(import(Some("petrov-2026")), Err(CoreError::WrongPassword)),
                "{app}"
            );
            let unlocked = import(Some(PASSWORD)).unwrap();
            let open = DocumentSession::new()
                .import_file(
                    fixture(&format!("locked-office/{app}-open.docx")),
                    ImportOptions::default(),
                    None,
                )
                .unwrap();
            assert_eq!(unlocked.text, open.text, "{app}");
            assert_eq!(unlocked.warning, open.warning, "{app}");
            assert_eq!(unlocked.left_out, open.left_out, "{app}");
            assert_eq!(files(scratch.path()), before, "{app}");
        }
    }

    /// The session hands its unlock to the open document. Both that path
    /// and reopening with a password render without writing to TMPDIR.
    #[cfg(not(target_os = "android"))]
    #[test]
    fn a_locked_docx_original_leaves_the_temp_folder_empty() {
        use crate::original::{CommentMark, CommentMarkReader, DocxInput, DocxOriginal, PageRect};

        const CHILD: &str = "MDOC_FFI_LOCKED_ORIGINAL_TEST";
        const FIXTURE: &str = "MDOC_FFI_LOCKED_ORIGINAL_FIXTURE";
        if std::env::var_os(CHILD).is_some() {
            struct Marks;
            impl CommentMarkReader for Marks {
                fn comment_marks(&self, pdf: Vec<u8>) -> Vec<CommentMark> {
                    assert!(pdf.starts_with(b"%PDF-"));
                    vec![CommentMark {
                        page: 0,
                        rect: PageRect {
                            x: 0.1,
                            y: 0.1,
                            w: 0.2,
                            h: 0.02,
                        },
                        target: "0".into(),
                    }]
                }
            }

            assert_eq!(
                std::env::temp_dir(),
                Path::new(&std::env::var_os("TMPDIR").unwrap())
            );
            let fixture = std::env::var(FIXTURE).unwrap();
            let check_original = |original: &DocxOriginal| {
                assert!(original.pdf_bytes().starts_with(b"%PDF-"));
                let anchor = &original.anchors()[0];
                assert_eq!(anchor.id, "0");
                assert_eq!(anchor.page, 0);
                assert_eq!(anchor.rects[0].x, 0.1);
                assert!(
                    std::fs::read_dir(std::env::temp_dir())
                        .unwrap()
                        .next()
                        .is_none()
                );
            };
            let session = DocumentSession::new();
            session
                .import_file(
                    fixture.clone(),
                    ImportOptions {
                        password: Some(PASSWORD.into()),
                        ..ImportOptions::default()
                    },
                    None,
                )
                .unwrap();
            let source = session
                .take_unlocked_docx()
                .expect("the import hands over the unlock");
            assert!(session.take_unlocked_docx().is_none());
            let original =
                DocxOriginal::render(DocxInput::Unlocked { source }, Box::new(Marks)).unwrap();
            check_original(&original);
            assert!(original.unlocked_source().is_none());

            let render_reopened = |password: Option<String>| {
                DocxOriginal::render(
                    DocxInput::File {
                        bytes: std::fs::read(&fixture).unwrap(),
                        password,
                    },
                    Box::new(Marks),
                )
            };
            assert!(matches!(
                render_reopened(None),
                Err(CoreError::NeedsPassword)
            ));
            assert!(matches!(
                render_reopened(Some("wrong".into())),
                Err(CoreError::WrongPassword)
            ));
            let original = render_reopened(Some(PASSWORD.into())).unwrap();
            check_original(&original);
            assert!(original.unlocked_source().is_some());
            return;
        }

        let input = Scratch::new();
        let encrypted = input.path().join("comments.docx");
        let plain = std::fs::read(fixture("docx-preview/comments.docx")).unwrap();
        std::fs::write(
            &encrypted,
            craft_offcrypto::encrypt_with_spin_count(&plain, PASSWORD, 1000).unwrap(),
        )
        .unwrap();
        let empty = Scratch::new();
        let output = std::process::Command::new(std::env::current_exe().unwrap())
            .args([
                "--exact",
                "document_session::tests::a_locked_docx_original_leaves_the_temp_folder_empty",
                "--nocapture",
            ])
            .env(CHILD, "1")
            .env(FIXTURE, encrypted)
            .env("TMPDIR", empty.path())
            .env("TEMP", empty.path())
            .env("TMP", empty.path())
            .output()
            .unwrap();
        let stdout = String::from_utf8_lossy(&output.stdout);
        let stderr = String::from_utf8_lossy(&output.stderr);
        assert!(
            output.status.success() && stdout.contains("1 passed"),
            "{stdout}\n{stderr}"
        );
        assert!(std::fs::read_dir(empty.path()).unwrap().next().is_none());
    }

    /// Import options never show their password.
    #[test]
    fn import_options_never_show_the_password() {
        let options = ImportOptions {
            password: Some("Petrov-2026".into()),
            ..ImportOptions::default()
        };
        let shown = format!("{options:?} {options:#?}");
        assert!(!shown.contains("Petrov"), "{shown}");
    }

    /// A partial import's notice belongs to the import: a saved document
    /// reopens without it, so the phones' notice card closes for good.
    #[test]
    fn a_reopened_document_has_no_import_notice() {
        let scratch = Scratch::new();
        let vault = vault(&scratch);
        let session = DocumentSession::new();
        let imported = session
            .import_file(
                fixture("import/handmade-partly-scanned.pdf"),
                ImportOptions {
                    native_text_only: true,
                    ..ImportOptions::default()
                },
                None,
            )
            .unwrap();
        assert!(
            imported
                .notice
                .as_deref()
                .is_some_and(|n| n.starts_with("Partial import")),
            "{:?}",
            imported.notice
        );
        let saved = session.save_into_vault(vault.clone()).unwrap();
        let reopened = DocumentSession::new()
            .open_from_vault(vault, saved.id)
            .unwrap();
        assert_eq!(reopened.notice, None);
        assert_eq!(reopened.warning, None);
        assert_eq!(reopened.left_out, None);
    }

    fn range_of(snapshot: &ReviewSnapshot, wording: &str, nth: usize) -> TextRange {
        let start = snapshot.text.match_indices(wording).nth(nth).unwrap().0 as u64;
        TextRange {
            start,
            end: start + wording.len() as u64,
        }
    }

    /// Each change saves (as the app does): saving joins the Project only
    /// when another document changed it, so an undone rename stays undone.
    #[test]
    fn an_undo_after_a_save_stays_undone() {
        let scratch = Scratch::new();
        let vault = vault(&scratch);
        let session = session("Ann Lee wrote to Ann Lee.", "Letter");
        select(&session, "Ann Lee");
        let replaced = session.replace_all().unwrap();
        session.save_into_vault(vault.clone()).unwrap();
        let renamed = session
            .rename(
                range_of(&replaced, "PERSON_1", 0),
                Scope::All,
                "CLIENT".into(),
            )
            .unwrap();
        assert_eq!(renamed.text, "CLIENT wrote to CLIENT.");
        let step = renamed.undo_step.clone().unwrap();
        assert_eq!(step.label, "Rename · CLIENT");
        assert_eq!(step.range, Some(range_of(&renamed, "CLIENT", 0)));
        session.save_into_vault(vault.clone()).unwrap();
        let undone = session.undo().unwrap();
        assert_eq!(undone.text, "PERSON_1 wrote to PERSON_1.");
        assert_eq!(undone.redo_step.unwrap().kind, StepKind::Rename);
        let saved = session.save_into_vault(vault.clone()).unwrap();
        assert_eq!(saved.review.text, "PERSON_1 wrote to PERSON_1.");
        assert!(saved.review.can_redo);
        assert_eq!(
            restored(&vault, &saved.id, "PERSON_1 agreed."),
            "Ann Lee agreed."
        );
        let redone = session.redo().unwrap();
        assert_eq!(redone.text, "CLIENT wrote to CLIENT.");
        assert!(!redone.can_redo);
    }

    /// Same as saved, then undone and saved again: the document has two
    /// people again and restores each to its own. Its mapping keeps the
    /// merge, as on desktop: a save never reverses a merge
    /// (`unmerged_members_cannot_reverse_a_case_merge`).
    #[test]
    fn an_undone_merge_is_two_people_in_the_document_again() {
        let scratch = Scratch::new();
        let vault = vault(&scratch);
        let session = session("Ann Lee wrote to Mia Fox.", "Letter");
        select(&session, "Ann Lee");
        select(&session, "Mia Fox");
        let replaced = session.replace_all().unwrap();
        assert_eq!(replaced.text, "PERSON_1 wrote to PERSON_2.");
        session.save_into_vault(vault.clone()).unwrap();
        let (ann, mia) = (
            session
                .identity_with_alias("PERSON_1".into())
                .unwrap()
                .unwrap(),
            session
                .identity_with_alias("PERSON_2".into())
                .unwrap()
                .unwrap(),
        );
        session.link(mia, ann).unwrap();
        session.save_into_vault(vault.clone()).unwrap();
        assert_eq!(session.undo().unwrap().text, "PERSON_1 wrote to PERSON_2.");
        let saved = session.save_into_vault(vault.clone()).unwrap();
        assert_eq!(saved.review.text, "PERSON_1 wrote to PERSON_2.");
        assert_eq!(saved.review.identities.len(), 2);
        assert_eq!(
            session
                .restore_reply("PERSON_1 and PERSON_2.".into())
                .unwrap()
                .text,
            "Ann Lee and Mia Fox."
        );
        assert_eq!(
            stored_names(&vault, &saved.id),
            1,
            "the mapping keeps the merge"
        );
    }

    /// × on a merge hint is kept in the Project: the document reopened, or
    /// another of its documents, no longer shows it.
    #[test]
    fn a_dismissed_hint_is_kept_in_the_project() {
        let scratch = Scratch::new();
        let vault = vault(&scratch);
        let project = vault.create_project("Case".into()).unwrap();
        let session = session("Maya Bennett met Ms Bennett.", "Letter");
        session
            .join_project(vault.clone(), project.id.clone())
            .unwrap();
        select(&session, "Maya Bennett");
        select(&session, "Ms Bennett");
        let mut snapshot = session.snapshot().unwrap();
        if snapshot.identities.len() == 1 {
            // Propagation joined the short form: split it off.
            snapshot = session
                .new_alias(range_of(&snapshot, "Ms Bennett", 0))
                .unwrap();
        }
        assert_eq!(snapshot.link_hints.len(), 1);
        let hint = snapshot.link_hints[0].clone();
        let can_undo = snapshot.can_undo;
        let dismissed = session.dismiss_hint(hint.from, hint.into).unwrap();
        assert!(dismissed.link_hints.is_empty());
        assert_eq!(dismissed.can_undo, can_undo);
        session.replace_all().unwrap();
        let saved = session.save_into_vault(vault.clone()).unwrap();
        assert_eq!(
            vault
                .store()
                .project(&project.id)
                .unwrap()
                .dismissed_hints
                .len(),
            1
        );
        let reopened = DocumentSession::new()
            .open_from_vault(vault.clone(), saved.id)
            .unwrap();
        assert_eq!(reopened.identities.len(), 2);
        assert!(reopened.link_hints.is_empty());
    }

    /// A merge hint only suggests: replacing, saving and reopening leave
    /// the two people apart until the user merges them.
    #[test]
    fn a_merge_hint_never_merges_on_its_own() {
        let scratch = Scratch::new();
        let vault = vault(&scratch);
        let session = session("Maya Bennett met Ms Bennett.", "Letter");
        select(&session, "Maya Bennett");
        select(&session, "Ms Bennett");
        let mut snapshot = session.snapshot().unwrap();
        if snapshot.identities.len() == 1 {
            // Propagation joined the short form: split it off.
            snapshot = session
                .new_alias(range_of(&snapshot, "Ms Bennett", 0))
                .unwrap();
        }
        assert_eq!(snapshot.link_hints.len(), 1);
        let replaced = session.replace_all().unwrap();
        assert_eq!(replaced.identities.len(), 2);
        let saved = session.save_into_vault(vault.clone()).unwrap();
        assert_eq!(saved.review.identities.len(), 2);
        assert_eq!(stored_names(&vault, &saved.id), 2);
        let reopened = DocumentSession::new()
            .open_from_vault(vault.clone(), saved.id)
            .unwrap();
        assert_eq!(reopened.identities.len(), 2);
        assert_eq!(reopened.link_hints.len(), 1);
    }

    /// Undo history belongs to one document: opening another in the same
    /// session leaves nothing to undo, so Undo never reaches the first
    /// document's step.
    #[test]
    fn opening_another_document_starts_its_own_undo_history() {
        let scratch = Scratch::new();
        let vault = vault(&scratch);
        let saved = |text: &str, name: &str, title: &str| {
            let session = session(text, title);
            select(&session, name);
            session.replace_all().unwrap();
            session.save_into_vault(vault.clone()).unwrap().id
        };
        let first = saved("Ann Lee wrote.", "Ann Lee", "First");
        let second = saved("Mia Fox replied.", "Mia Fox", "Second");

        let session = DocumentSession::new();
        session
            .open_from_vault(vault.clone(), first.clone())
            .unwrap();
        select(&session, "wrote");
        assert!(session.snapshot().unwrap().can_undo);
        let opened = session.open_from_vault(vault.clone(), second).unwrap();
        assert!(!opened.can_undo);
        assert_eq!(session.undo().unwrap().text, opened.text);
        let first_again = DocumentSession::new()
            .open_from_vault(vault.clone(), first)
            .unwrap();
        assert_eq!(first_again.text, "PERSON_1 wrote.");
    }

    /// The identity tools through the session: the category menu, owner,
    /// the alias lookup Rename's Same as uses.
    #[test]
    fn the_identity_tools_reach_the_review() {
        let session = session("Acme Ltd, info@acme.example.", "Letter");
        select(&session, "Acme Ltd");
        let text = session.snapshot().unwrap().text;
        let start = text.find("info@acme.example").unwrap() as u64;
        let email = TextRange {
            start,
            end: start + "info@acme.example".len() as u64,
        };
        session.add_selection(email, Some(Category::Email)).unwrap();
        let snapshot = session.snapshot().unwrap();
        let acme = range_of(&snapshot, "Acme Ltd", 0);
        let choices = session.category_choices(acme, Scope::All).unwrap();
        assert!(
            choices
                .iter()
                .any(|c| c.current && c.category == Category::Person)
        );
        let org = choices
            .iter()
            .find(|c| c.category == Category::Organization)
            .unwrap();
        assert_eq!(org.alias.as_deref(), Some("ORG_1"));
        let snapshot = session
            .set_category(acme, Scope::All, Category::Organization)
            .unwrap();
        let company = session
            .identity_with_alias("ORG_1".into())
            .unwrap()
            .unwrap();
        let mailbox = session
            .identity_with_alias("EMAIL_1".into())
            .unwrap()
            .unwrap();
        assert_eq!(
            session.identity_with_alias("PERSON_1".into()).unwrap(),
            None
        );
        assert_eq!(snapshot.undo_step.unwrap().identity, Some(company));
        let owned = session.set_owner(mailbox, Some(company)).unwrap();
        let row = owned.identities.iter().find(|i| i.id == mailbox).unwrap();
        assert_eq!(row.owner, Some(company));
        let undone = session.undo().unwrap();
        let row = undone.identities.iter().find(|i| i.id == mailbox).unwrap();
        assert_eq!(row.owner, None);
        assert_eq!(undone.redo_step.unwrap().kind, StepKind::Owner);
    }
}
