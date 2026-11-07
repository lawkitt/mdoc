//! `DocumentSession` (ADR 0085): one document from intake to the AI and
//! back. Import and OCR through `mdoc_import`, scans through `mdoc_detect`,
//! the review of `mdoc_pii` as snapshots and transitions with undo and redo
//! (ADR 0098), Copy for AI and Share text, and the mapping saved into its
//! case. Long jobs report their
//! stage to the platform and stop when it cancels; the platform runs them
//! off its main thread.
mod actions;
mod intake;
mod review;
mod snapshot;

use crate::{
    CoreError,
    library::{CaseInfo, Library, RenamedDocument, RestoredReply, TextRange},
    setup::{DetectorPaths, OcrPaths},
};
pub use actions::{HandOff, ProposedName, ReviewAction, SessionInfo, occurrences};
use mdoc_pii::restore::RestoreMap;
use review::ReviewText;
pub use review::{Scope, StepKind};
pub use snapshot::{
    Category, CategoryChoice, DocumentPhase, HistoryStep, IdentityRow, LinkHint, Mention,
    MentionState, ReviewSnapshot,
};
use std::{
    collections::HashSet,
    path::{Path, PathBuf},
    sync::{
        Arc, Mutex, MutexGuard,
        atomic::{AtomicBool, Ordering},
    },
};

/// Where a long job is; the platform shows it.
#[derive(Clone, Copy, Debug, PartialEq, Eq, uniffi::Enum)]
pub enum JobStage {
    Reading,
    Converting,
    /// OCR: loading the models, then recognizing every page.
    Recognizing,
    Scanning,
}

/// The platform's view of a long job. Called on the job's thread.
#[uniffi::export(callback_interface)]
pub trait JobProgress: Send + Sync {
    fn stage(&self, stage: JobStage);
    /// How far the current stage is: `done` of `total` units (the
    /// detector's windows while scanning, pages while recognizing). A phone
    /// shows it and passes it to the system, which stops a background job
    /// that reports none.
    fn advanced(&self, done: u64, total: u64);
}

/// How to import a file.
#[derive(Clone, Debug, Default, PartialEq, uniffi::Record)]
pub struct ImportOptions {
    /// Recognize pages without a text layer, and images, with these.
    pub ocr: Option<OcrPaths>,
    /// Take a PDF's text layer only, skipping pages that need OCR.
    pub native_text_only: bool,
}

/// A candidate found by something other than the core's scan: the parity
/// suites inject them, as the desktop's tests do.
#[derive(Clone, Debug, PartialEq, uniffi::Record)]
pub struct Candidate {
    pub range: TextRange,
    pub category: Category,
    pub score: f32,
}

/// Where `save` put the document.
#[derive(Clone, Debug, PartialEq, uniffi::Record)]
pub struct SavedDocument {
    pub path: String,
    pub case: CaseInfo,
    /// The review after saving: joining the case as stored now may have
    /// renumbered aliases another document took meanwhile.
    pub review: ReviewSnapshot,
}

struct Document {
    title: String,
    warning: Option<String>,
    review: ReviewText,
    /// The case the document joined or started; it never changes.
    case: Option<String>,
    /// Categories its case does not detect (ADR 0041); scans drop them.
    skip: HashSet<mdoc_pii::Category>,
    /// Where `save` keeps the Markdown, reserved on the first save.
    path: Option<PathBuf>,
    /// The file as last saved, so a save never overwrites a change made to
    /// it elsewhere (Files, another app).
    stored: mdoc_store::document::Document,
    /// Bumped when an import replaces the document, so a late scan of the
    /// one before cannot land on it.
    generation: u64,
    /// The case's identities as this document last joined or saved them:
    /// while the case still holds exactly these, no other document changed
    /// it, and saving needs no join that could undo this one's own steps.
    synced: Option<Vec<mdoc_store::cases::StoredIdentity>>,
    /// Scanned, or names proposed some other way, even if none were found.
    scanned: bool,
    /// `ReviewText::change_key` at the last save: equal now, nothing
    /// changed since, and a save would write what is there.
    saved: Option<((u64, u64, usize), bool)>,
}

#[derive(uniffi::Object)]
pub struct DocumentSession {
    document: Mutex<Option<Document>>,
    job: Mutex<Jobs>,
    generation: Mutex<u64>,
}

/// The session's jobs, one at a time: a new job stops the one before.
#[derive(Default)]
struct Jobs {
    /// The latest job's cancel flag.
    flag: Arc<AtomicBool>,
    /// Bumped by every job, so a job that ends knows whether it is the latest.
    id: u64,
    /// The latest job has not ended.
    running: bool,
    /// `cancel` came while no job ran: the next job starts cancelled.
    pending: bool,
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
        })
    }

    /// Import `path`: PDF, DOCX (in-process) and the other formats desktop
    /// opens, Markdown and plain text as they are, and JPEG images (camera
    /// scans, photos) through OCR. A PDF with pages that need OCR fails with
    /// `CoreError::NeedsOcr` unless `options` say how to treat them.
    pub fn import_file(
        &self,
        path: String,
        options: ImportOptions,
        progress: Option<Box<dyn JobProgress>>,
    ) -> Result<ReviewSnapshot, CoreError> {
        let cancel = self.start_job();
        let imported = intake::import_file(
            std::path::Path::new(&path),
            &options,
            &cancel,
            progress.as_deref(),
            None,
        )?;
        self.open(imported, &cancel, None)
    }

    /// `import_file`, with a PDF's OCR keeping its finished pages under
    /// `library`'s root (ADR 0085 Long jobs): after a cancel, or mdoc
    /// stopping, importing the same file with the same OCR model again
    /// recognizes only the pages left, and gives the Markdown an
    /// uninterrupted run gives. The kept pages are removed once this
    /// session takes the document, or by `Library::discard_import`.
    pub fn import_file_in(
        &self,
        library: Arc<Library>,
        path: String,
        options: ImportOptions,
        progress: Option<Box<dyn JobProgress>>,
    ) -> Result<ReviewSnapshot, CoreError> {
        let cancel = self.start_job();
        let imported = intake::import_file(
            std::path::Path::new(&path),
            &options,
            &cancel,
            progress.as_deref(),
            Some(&library.kept_pages()),
        )?;
        self.open(imported, &cancel, None)
    }

    /// Pasted or shared text, taken as Markdown.
    pub fn import_text(&self, text: String, title: String) -> Result<ReviewSnapshot, CoreError> {
        let cancel = self.start_job();
        let imported = intake::Imported {
            title,
            markdown: text,
            warning: None,
            kept: None,
        };
        self.open(imported, &cancel, None)
    }

    /// Open `path`, a document the library keeps: it stays that file and
    /// stays in its case, whose names are proposed and whose aliases in the
    /// text count as replaced (desktop's reopened case member).
    pub fn open_document(
        &self,
        library: Arc<Library>,
        path: String,
    ) -> Result<ReviewSnapshot, CoreError> {
        let cancel = self.start_job();
        let stored = mdoc_store::document::Document::open(PathBuf::from(&path))?;
        let member = library.member(Path::new(&path));
        let case = library
            .cases_raw()
            .into_iter()
            .find(|case| case.members.contains(&member));
        let imported = intake::Imported {
            title: crate::library::title_of(Path::new(&path)),
            markdown: stored.saved.clone(),
            warning: None,
            kept: None,
        };
        let opened = Opened {
            path: stored.path.clone(),
            stored,
            case,
        };
        self.open(imported, &cancel, Some(opened))
    }

    /// Stop the running import or scan; it fails with `CoreError::Cancelled`
    /// at its next checkpoint (model loads and native calls finish first).
    /// With no job running, the next job to start stops at once: a cancel
    /// the platform passes on just before the job starts is never lost.
    pub fn cancel(&self) {
        let mut jobs = self.lock_job();
        if jobs.running {
            jobs.flag.store(true, Ordering::Relaxed);
        } else {
            jobs.pending = true;
        }
    }

    pub fn snapshot(&self) -> Result<ReviewSnapshot, CoreError> {
        self.with_document(|_| Ok(()))
    }

    /// Scan for candidates: the GLiNER2 detector and the structured rules.
    /// Without `detector` the rules run alone: only UI tests seed candidates
    /// that way, since below the detector floor the apps offer no Find
    /// names (ADR 0088). The
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
        if let Some(progress) = &progress {
            progress.stage(JobStage::Scanning);
        }
        let detections = match &detector {
            Some(paths) => {
                mdoc_detect::scan_at_with_progress(
                    &source,
                    &cancel,
                    std::path::Path::new(&paths.model),
                    std::path::Path::new(&paths.onnx),
                    &mdoc_store::settings::PiiConfig::default(),
                    &|windows| {
                        if let Some(progress) = &progress {
                            progress.advanced(windows.done as u64, windows.windows as u64);
                        }
                    },
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
        })
    }

    /// Link identity `from` into identity `into`, which keeps its alias:
    /// Same as… from a merge hint, or for a whole identity. One undo step.
    pub fn link(&self, from: u64, into: u64) -> Result<ReviewSnapshot, CoreError> {
        self.with_document(|document| document.review.link(from, into))
    }

    /// Undo the last step (ADR 0098): a decision, a mark or an identity
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
    /// once it saves (ADR 0080). Not an undo step.
    pub fn dismiss_hint(&self, from: u64, into: u64) -> Result<ReviewSnapshot, CoreError> {
        self.with_document(|document| document.review.dismiss_hint(from, into))
    }

    /// Copy for AI: the alias-only note, then the text, byte for byte what
    /// desktop Copy for AI copies.
    pub fn copy_for_ai(&self) -> Result<String, CoreError> {
        self.read_document(|document| document.review.copy_for_ai())
    }

    /// What Share sends: the same text as Copy for AI (ADR 0085).
    pub fn share_text(&self) -> Result<String, CoreError> {
        self.copy_for_ai()
    }

    /// A name for the shared `.md` file: the title with this document's
    /// known originals replaced by their aliases.
    pub fn share_file_name(&self) -> Result<String, CoreError> {
        self.read_document(Document::share_file_name)
    }

    /// Restore an AI reply with this document's mapping, as desktop's reply
    /// box does (ADR 0078).
    pub fn restore_reply(&self, reply: String) -> Result<RestoredReply, CoreError> {
        self.read_document(|document| {
            RestoredReply::of(
                &RestoreMap::new(document.review.review.export_identities()),
                &reply,
            )
        })
    }

    /// Add this document to case `case_id` before saving: its names take
    /// the case's aliases and the case's names are proposed (ADR 0041). A
    /// document belongs to one case; joining another is refused.
    pub fn join_case(
        &self,
        library: Arc<Library>,
        case_id: String,
    ) -> Result<ReviewSnapshot, CoreError> {
        let case = library.load(&case_id)?;
        self.with_document(|document| match &document.case {
            Some(joined) if *joined == case_id => Ok(()),
            Some(_) => Err(CoreError::case("This document is in another case.")),
            None => {
                document
                    .review
                    .join(&case.exported(), &case.reserved_aliases())?;
                document.review.dismiss_pairs(&case.dismissed_hints);
                document.case = Some(case_id.clone());
                document.skip = case.skipped();
                document.synced = Some(case.identities.clone());
                Ok(())
            }
        })
    }

    /// Save the Markdown into the library's documents folder and its
    /// mapping into its case, starting a case named after the document when
    /// it has none (ADR 0085: each prepared document starts a new case).
    /// The document first joins its case as stored now, so names another
    /// document added meanwhile keep their aliases and this one's move to
    /// free numbers; one library write lock covers the whole save.
    pub fn save(&self, library: Arc<Library>) -> Result<SavedDocument, CoreError> {
        let mut guard = self.lock_document();
        guard.as_mut().ok_or_else(no_document)?.save(&library)
    }

    /// Rename…: this document's title, as `Library::rename_document` does
    /// for a saved one, keeping the session on its renamed file with its
    /// review and history. Not saved yet, only the title changes, and its
    /// case with it while the case is still empty and named after it.
    pub fn rename_document(
        &self,
        library: Arc<Library>,
        title: String,
    ) -> Result<RenamedDocument, CoreError> {
        let mut guard = self.lock_document();
        let document = guard.as_mut().ok_or_else(no_document)?;
        let lock = library.lock()?;
        let renamed = match &document.path {
            Some(path) => library.rename_locked(&lock, path, &title)?,
            None => library.rename_unsaved_locked(
                &lock,
                document.case.as_deref(),
                &document.title,
                &title,
            )?,
        };
        if let Some(path) = &renamed.path {
            let path = PathBuf::from(path);
            document.stored.path = Some(std::path::absolute(&path)?);
            document.path = Some(path);
        }
        document.title = renamed.title.clone();
        Ok(renamed)
    }
}

/// The text of a document moving from case `from` (if any) into case `to`,
/// and the identities it brings: opened in `from` as a reopened member is,
/// then joined to `to` (`plan_join`), so names `to` knows take its aliases,
/// clashing aliases its next free numbers, and the text is rewritten with
/// them. The identities carry no earlier aliases: those name `from`'s
/// history, which can mean other people in `to`.
pub(crate) fn realias(
    text: &str,
    from: Option<&mdoc_store::cases::Case>,
    to: &mdoc_store::cases::Case,
) -> Result<(String, Vec<mdoc_pii::restore::ExportedIdentity>), CoreError> {
    let mut review = ReviewText::new(text.to_owned());
    if let Some(from) = from {
        review.open_case(&from.exported(), &from.reserved_aliases());
    }
    review.join(&to.exported(), &to.reserved_aliases())?;
    let mut exported = review.review.export_identities();
    for identity in &mut exported {
        identity.previous.clear();
    }
    Ok((review.text, exported))
}

/// What a document the library keeps brings when it is opened.
struct Opened {
    path: Option<PathBuf>,
    stored: mdoc_store::document::Document,
    case: Option<mdoc_store::cases::Case>,
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

    /// Changes when the review does, or when the document is first scanned.
    fn change_key(&self) -> ((u64, u64, usize), bool) {
        (self.review.change_key(), self.scanned)
    }

    fn snapshot(&mut self) -> ReviewSnapshot {
        self.review.sync_ids();
        snapshot::of(
            &self.title,
            self.warning.as_deref(),
            &self.review,
            self.case.as_deref(),
            self.scanned,
        )
    }

    /// Scan results, minus the categories the case skips
    /// (`complete_pii_scan`). The document counts as scanned after.
    fn ingest(&mut self, mut detections: Vec<mdoc_pii::Detection>) -> Result<(), CoreError> {
        detections.retain(|d| !self.skip.contains(&d.category));
        self.review.ingest(detections)?;
        self.scanned = true;
        Ok(())
    }

    /// `DocumentSession::save`.
    fn save(&mut self, library: &Library) -> Result<SavedDocument, CoreError> {
        let lock = library.lock()?;
        let path = match &self.path {
            Some(path) => path.clone(),
            None => {
                let path = library.reserve_document_path(&self.title)?;
                self.path = Some(path.clone());
                path
            }
        };
        let member = library.member(&path);
        library.refuse_other_case(&member, self.case.as_deref().unwrap_or_default())?;
        let case = match self.case.clone() {
            Some(id) => library.edit_locked(&lock, &id, |case| {
                // Only a case another document changed since is joined
                // again: joining against this document's own last save would
                // take back what Undo just reverted.
                if self.synced.as_ref() != Some(&case.identities) {
                    self.review
                        .join(&case.exported(), &case.reserved_aliases())?;
                    self.review.dismiss_pairs(&case.dismissed_hints);
                }
                self.stored.save(path.clone(), &self.review.text)?;
                let before = case.clone();
                case.merge(&self.review.review.export_identities());
                if !case.members.contains(&member) {
                    case.members.push(member.clone());
                }
                for (a, b) in self.review.dismissed_pairs() {
                    let pair = (a, b);
                    let flipped = (pair.1.clone(), pair.0.clone());
                    if !case.dismissed_hints.contains(&pair)
                        && !case.dismissed_hints.contains(&flipped)
                    {
                        case.dismissed_hints.push(pair);
                    }
                }
                self.synced = Some(case.identities.clone());
                Ok(*case != before)
            })?,
            None => {
                self.stored.save(path.clone(), &self.review.text)?;
                let mut case = mdoc_store::cases::Case::new(
                    self.title.clone(),
                    &self.review.review.export_identities(),
                    Some(member),
                );
                // Hints dismissed before the case existed stay dismissed.
                case.dismissed_hints = self.review.dismissed_pairs();
                self.synced = Some(case.identities.clone());
                library.create_locked(&lock, case)?
            }
        };
        self.case = Some(case.id.clone());
        self.saved = Some(self.change_key());
        Ok(SavedDocument {
            path: path.display().to_string(),
            case,
            review: self.snapshot(),
        })
    }
}

impl DocumentSession {
    /// Make `imported` the session's document, unless its job was
    /// cancelled or a newer job started: then the current one stays.
    fn open(
        &self,
        imported: intake::Imported,
        cancel: &Job,
        opened: Option<Opened>,
    ) -> Result<ReviewSnapshot, CoreError> {
        let job = self.lock_job();
        if job.id != cancel.id || cancel.load(Ordering::Relaxed) {
            return Err(CoreError::Cancelled);
        }
        let generation = {
            let mut generation = self.generation.lock().unwrap_or_else(|e| e.into_inner());
            *generation += 1;
            *generation
        };
        let imported_kept = imported.kept;
        let mut document = Document {
            title: imported.title,
            warning: imported.warning,
            review: ReviewText::new(imported.markdown),
            case: None,
            skip: HashSet::new(),
            path: None,
            stored: mdoc_store::document::Document::default(),
            generation,
            synced: None,
            scanned: false,
            saved: None,
        };
        if let Some(opened) = opened {
            document.path = opened.path;
            document.stored = opened.stored;
            if let Some(case) = opened.case {
                document
                    .review
                    .open_case(&case.exported(), &case.reserved_aliases());
                document.review.dismiss_pairs(&case.dismissed_hints);
                document.skip = case.skipped();
                document.synced = Some(case.identities.clone());
                document.case = Some(case.id);
            }
        }
        let snapshot = document.snapshot();
        *self.lock_document() = Some(document);
        // Taken: an import rejected before this point keeps its OCR pages.
        if let Some(kept) = imported_kept {
            kept.remove();
        }
        drop(job);
        Ok(snapshot)
    }

    /// A new job: the one before stops at its next checkpoint, and a
    /// cancel that came while none ran stops this one.
    fn start_job(&self) -> Job<'_> {
        let mut jobs = self.lock_job();
        jobs.flag.store(true, Ordering::Relaxed);
        jobs.flag = Arc::new(AtomicBool::new(std::mem::take(&mut jobs.pending)));
        jobs.id += 1;
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

fn no_document() -> CoreError {
    CoreError::invalid("Import a document first.")
}

fn span(range: TextRange) -> std::ops::Range<usize> {
    range.start as usize..range.end as usize
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::test_support::Scratch;

    fn library(scratch: &Scratch) -> Arc<Library> {
        Library::new(scratch.path().display().to_string(), vec![3; 32]).unwrap()
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

    /// Two documents of one case reviewed side by side both give PERSON_1
    /// to their own person; the later save moves its person to a free
    /// number instead of merging the two people.
    #[test]
    fn stale_sessions_never_merge_different_people() {
        let scratch = Scratch::new();
        let library = library(&scratch);
        let case = library.create_case("Case".into()).unwrap();
        let ann = session("Ann Lee signed.", "Ann");
        let bob = session("Bob Hart signed.", "Bob");
        for (session, name) in [(&ann, "Ann Lee"), (&bob, "Bob Hart")] {
            session.join_case(library.clone(), case.id.clone()).unwrap();
            select(session, name);
            session.replace_all().unwrap();
        }
        assert_eq!(ann.snapshot().unwrap().text, "PERSON_1 signed.");
        assert_eq!(bob.snapshot().unwrap().text, "PERSON_1 signed.");
        ann.save(library.clone()).unwrap();
        let saved = bob.save(library.clone()).unwrap();
        assert_eq!(saved.review.text, "PERSON_2 signed.");
        assert_eq!(
            std::fs::read_to_string(&saved.path).unwrap(),
            "PERSON_2 signed."
        );
        let reply = "PERSON_1 and PERSON_2 agree.";
        assert_eq!(
            library.restore(case.id, reply.into()).unwrap().text,
            "Ann Lee and Bob Hart agree."
        );
    }

    #[test]
    fn concurrent_saves_of_one_title_get_their_own_files_and_cases() {
        let scratch = Scratch::new();
        let library = library(&scratch);
        let saves: Vec<_> = ["Ann Lee", "Bob Hart"]
            .into_iter()
            .map(|name| {
                let library = library.clone();
                let session = session(&format!("{name} signed."), "Document");
                select(&session, name);
                session.replace_all().unwrap();
                std::thread::spawn(move || session.save(library).unwrap())
            })
            .collect();
        let saved: Vec<_> = saves.into_iter().map(|s| s.join().unwrap()).collect();
        assert_ne!(saved[0].path, saved[1].path);
        assert_ne!(saved[0].case.id, saved[1].case.id);
        for saved in &saved {
            assert_eq!(saved.case.documents, std::slice::from_ref(&saved.path));
        }
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

    /// #680: a cancel the platform passes on just before the job starts
    /// stops it, for an import and for a scan; the job after it runs.
    #[test]
    fn a_cancel_just_before_a_job_stops_it() {
        let scratch = Scratch::new();
        let path = scratch.path().join("old.md");
        std::fs::write(&path, "Ann Lee, ann@example.com.").unwrap();
        let session = session("Current", "Current");
        session.cancel();
        let result =
            session.import_file(path.display().to_string(), ImportOptions::default(), None);
        assert!(matches!(result, Err(CoreError::Cancelled)), "{result:?}");
        assert_eq!(session.snapshot().unwrap().text, "Current");
        session
            .import_file(path.display().to_string(), ImportOptions::default(), None)
            .unwrap();
        session.cancel();
        let result = session.scan(None, None);
        assert!(matches!(result, Err(CoreError::Cancelled)), "{result:?}");
        assert!(session.snapshot().unwrap().mentions.is_empty());
        assert_eq!(session.scan(None, None).unwrap().mentions.len(), 1);
    }

    /// Save → reopen from the library → share → restore → save: the file
    /// and its case stay the same, and its aliases count as replaced.
    #[test]
    fn a_saved_document_reopens_in_its_case() {
        let scratch = Scratch::new();
        let library = library(&scratch);
        let first = session("Ann Lee wrote to Bob Hart.", "Letter");
        select(&first, "Ann Lee");
        first.replace_all().unwrap();
        let saved = first.save(library.clone()).unwrap();
        let copied = first.copy_for_ai().unwrap();

        let reopened = DocumentSession::new();
        let snapshot = reopened
            .open_document(library.clone(), saved.path.clone())
            .unwrap();
        assert_eq!(snapshot.case_id, Some(saved.case.id.clone()));
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
        let again = reopened.save(library.clone()).unwrap();
        assert_eq!(again.path, saved.path);
        assert_eq!(again.case.id, saved.case.id);
        assert_eq!(library.documents().unwrap().len(), 1);
        assert_eq!(
            std::fs::read_to_string(&saved.path).unwrap(),
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

    /// Without OCR, importing into a library is `import_file`, and keeps
    /// nothing.
    #[test]
    fn importing_into_a_library_without_ocr_is_import_file() {
        let scratch = Scratch::new();
        let library = library(&scratch);
        for (name, options) in [
            ("import/text.docx", ImportOptions::default()),
            (
                "import/handmade-partly-scanned.pdf",
                ImportOptions {
                    native_text_only: true,
                    ..ImportOptions::default()
                },
            ),
        ] {
            let kept = DocumentSession::new()
                .import_file_in(library.clone(), fixture(name), options.clone(), None)
                .unwrap();
            let plain = DocumentSession::new()
                .import_file(fixture(name), options, None)
                .unwrap();
            assert_eq!(kept, plain, "{name}");
        }
        assert!(matches!(
            DocumentSession::new().import_file_in(
                library.clone(),
                fixture("import/handmade-partly-scanned.pdf"),
                ImportOptions::default(),
                None
            ),
            Err(CoreError::NeedsOcr { .. })
        ));
        assert!(!scratch.path().join("imports").exists());
        // Nothing kept: discarding is a no-op.
        library
            .discard_import(fixture("import/handmade-partly-scanned.pdf"))
            .unwrap();
    }

    /// With the OCR runtime: an import stopped after two pages resumes
    /// through the session with the pages left, and gives an uninterrupted
    /// import's document; a dropped import's pages are discarded.
    #[test]
    #[ignore = "downloads OCR runtimes/models; requires Apple Silicon macOS or Windows x64"]
    #[cfg(any(
        all(target_os = "macos", target_arch = "aarch64"),
        all(target_os = "windows", target_arch = "x86_64")
    ))]
    fn a_stopped_ocr_import_resumes_with_the_pages_left() {
        let scratch = Scratch::new();
        let library = library(&scratch);
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
        let paths = OcrPaths {
            root: models.display().to_string(),
            model: crate::setup::ModelId::OcrEnglishRussian,
            pdfium: installed.pdfium.display().to_string(),
            onnx: installed.onnx.display().to_string(),
        };
        let options = ImportOptions {
            ocr: Some(paths.clone()),
            native_text_only: false,
        };
        let pdf = fixture("import/handmade-partly-scanned.pdf");
        let uninterrupted = DocumentSession::new()
            .import_file(pdf.clone(), options.clone(), None)
            .unwrap();
        let imports = scratch.path().join("imports");
        let kept_pages = || std::fs::read_dir(&imports).map_or(0, |dir| dir.count());
        // Stop after page 2, as a cancel or mdoc stopping would.
        let stop = || {
            let cancel = AtomicBool::new(false);
            let stopped = mdoc_import::resume::prepare_resumable(
                Path::new(&pdf),
                Some(&paths.installed().unwrap()),
                false,
                &cancel,
                &library.kept_pages(),
                &|page, _| {
                    if page == 2 {
                        cancel.store(true, Ordering::Relaxed);
                    }
                },
            );
            assert!(matches!(
                stopped,
                Err(mdoc_import::import::ImportError::Cancelled)
            ));
            assert_eq!(kept_pages(), 1);
        };
        stop();
        let resumed = DocumentSession::new()
            .import_file_in(library.clone(), pdf.clone(), options.clone(), None)
            .unwrap();
        assert_eq!(resumed, uninterrupted);
        assert_eq!(kept_pages(), 0);
        stop();
        library.discard_import(pdf).unwrap();
        assert_eq!(kept_pages(), 0);
    }

    fn range_of(snapshot: &ReviewSnapshot, wording: &str, nth: usize) -> TextRange {
        let start = snapshot.text.match_indices(wording).nth(nth).unwrap().0 as u64;
        TextRange {
            start,
            end: start + wording.len() as u64,
        }
    }

    /// Each change saves (as the app does): saving joins the case only when
    /// another document changed it, so an undone rename stays undone.
    #[test]
    fn an_undo_after_a_save_stays_undone() {
        let scratch = Scratch::new();
        let library = library(&scratch);
        let session = session("Ann Lee wrote to Ann Lee.", "Letter");
        select(&session, "Ann Lee");
        let replaced = session.replace_all().unwrap();
        session.save(library.clone()).unwrap();
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
        session.save(library.clone()).unwrap();
        let undone = session.undo().unwrap();
        assert_eq!(undone.text, "PERSON_1 wrote to PERSON_1.");
        assert_eq!(undone.redo_step.unwrap().kind, StepKind::Rename);
        let saved = session.save(library.clone()).unwrap();
        assert_eq!(saved.review.text, "PERSON_1 wrote to PERSON_1.");
        assert!(saved.review.can_redo);
        assert_eq!(
            library
                .restore(saved.case.id.clone(), "PERSON_1 agreed.".into())
                .unwrap()
                .text,
            "Ann Lee agreed."
        );
        let redone = session.redo().unwrap();
        assert_eq!(redone.text, "CLIENT wrote to CLIENT.");
        assert!(!redone.can_redo);
    }

    /// Same as saved into the case, then undone and saved again: the
    /// document has two people again and restores each to its own. The case
    /// keeps the merge, as on desktop: a member's save never reverses a case
    /// merge (ADR 0041, `unmerged_members_cannot_reverse_a_case_merge`).
    #[test]
    fn an_undone_merge_is_two_people_in_the_document_again() {
        let scratch = Scratch::new();
        let library = library(&scratch);
        let session = session("Ann Lee wrote to Mia Fox.", "Letter");
        select(&session, "Ann Lee");
        select(&session, "Mia Fox");
        let replaced = session.replace_all().unwrap();
        assert_eq!(replaced.text, "PERSON_1 wrote to PERSON_2.");
        session.save(library.clone()).unwrap();
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
        session.save(library.clone()).unwrap();
        assert_eq!(session.undo().unwrap().text, "PERSON_1 wrote to PERSON_2.");
        let saved = session.save(library.clone()).unwrap();
        assert_eq!(saved.review.text, "PERSON_1 wrote to PERSON_2.");
        assert_eq!(saved.review.identities.len(), 2);
        assert_eq!(
            session
                .restore_reply("PERSON_1 and PERSON_2.".into())
                .unwrap()
                .text,
            "Ann Lee and Mia Fox."
        );
        let case = library.load(&saved.case.id).unwrap();
        assert_eq!(case.identities.len(), 1, "the case keeps the merge");
    }

    /// × on a merge hint is kept in the case: the document reopened, or
    /// another member, no longer shows it (ADR 0080).
    #[test]
    fn a_dismissed_hint_is_kept_in_the_case() {
        let scratch = Scratch::new();
        let library = library(&scratch);
        let session = session("Maya Bennett met Ms Bennett.", "Letter");
        select(&session, "Maya Bennett");
        select(&session, "Ms Bennett");
        let mut snapshot = session.snapshot().unwrap();
        if snapshot.identities.len() == 1 {
            // Propagation joined the short form (ADR 0092): split it off.
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
        let saved = session.save(library.clone()).unwrap();
        assert_eq!(
            library.load(&saved.case.id).unwrap().dismissed_hints.len(),
            1
        );
        let reopened = DocumentSession::new()
            .open_document(library.clone(), saved.path)
            .unwrap();
        assert_eq!(reopened.identities.len(), 2);
        assert!(reopened.link_hints.is_empty());
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

    /// `text` with `names` replaced, saved into a case of its own.
    fn saved(library: &Arc<Library>, text: &str, names: &[&str], title: &str) -> SavedDocument {
        let session = session(text, title);
        for name in names {
            select(&session, name);
        }
        session.replace_all().unwrap();
        session.save(library.clone()).unwrap()
    }

    fn restored(library: &Library, case: &str, reply: &str) -> String {
        library.restore(case.into(), reply.into()).unwrap().text
    }

    /// Moving re-aliases the document for the case it joins (`plan_join`):
    /// a shared name takes the case's alias, a clashing one the next free
    /// number, and the case's other names are proposed. The case it left
    /// keeps its names and the reply kept for it. Moving back gives the
    /// names their old aliases and takes out of the case it leaves the
    /// names the move added there.
    #[test]
    fn a_moved_document_takes_the_cases_aliases() {
        let scratch = Scratch::new();
        let library = library(&scratch);
        let letter = saved(
            &library,
            "Ann Lee wrote to Bob Hart about Carl Diaz.",
            &["Ann Lee", "Bob Hart"],
            "Letter",
        );
        assert_eq!(
            std::fs::read_to_string(&letter.path).unwrap(),
            "PERSON_1 wrote to PERSON_2 about Carl Diaz."
        );
        let reply = "PERSON_1 thanks PERSON_2.";
        library
            .save_reply(letter.case.id.clone(), letter.path.clone(), reply.into())
            .unwrap();
        let memo = saved(
            &library,
            "Bob Hart met Carl Diaz.",
            &["Bob Hart", "Carl Diaz"],
            "Memo",
        );
        let moved = library
            .move_document(letter.path.clone(), memo.case.id.clone())
            .unwrap();
        assert_eq!(moved.path, letter.path);
        assert_eq!(moved.case.id, memo.case.id);
        assert_eq!(
            moved.case.documents,
            [memo.path.clone(), letter.path.clone()]
        );
        assert_eq!(moved.added, ["PERSON_3"], "Ann; Bob and Carl were known");
        let left = moved.left.unwrap();
        assert_eq!(left.id, letter.case.id);
        assert!(left.documents.is_empty());
        assert_eq!(
            std::fs::read_to_string(&letter.path).unwrap(),
            "PERSON_3 wrote to PERSON_1 about Carl Diaz."
        );
        assert_eq!(
            restored(&library, &memo.case.id, "PERSON_3, PERSON_1, PERSON_2."),
            "Ann Lee, Bob Hart, Carl Diaz."
        );
        // The case left restores replies to what was sent before, and keeps
        // the reply; the document has none in its new case.
        assert_eq!(
            restored(&library, &letter.case.id, reply),
            "Ann Lee thanks Bob Hart."
        );
        let stored = library.load(&letter.case.id).unwrap();
        assert_eq!(stored.replies.len(), 1);
        assert_eq!(
            library
                .reply(memo.case.id.clone(), letter.path.clone())
                .unwrap(),
            None
        );
        // Reopened, it is in its new case, with the case's other name proposed.
        let reopened = DocumentSession::new()
            .open_document(library.clone(), letter.path.clone())
            .unwrap();
        assert_eq!(reopened.case_id, Some(memo.case.id.clone()));
        assert_eq!(reopened.replaced, 2);
        assert!(
            reopened
                .mentions
                .iter()
                .any(|m| m.original == "Carl Diaz" && m.alias.as_deref() == Some("PERSON_2"))
        );
        assert_eq!(
            library
                .documents()
                .unwrap()
                .iter()
                .find(|d| d.path == letter.path)
                .unwrap()
                .case_id,
            Some(memo.case.id.clone())
        );
        // Undo: moving back gives the names their old aliases, the reply is
        // the document's again, and Memo's case forgets Ann.
        let back = library
            .move_back(
                letter.path.clone(),
                letter.case.id.clone(),
                moved.added.clone(),
            )
            .unwrap();
        assert_eq!(back.case.id, letter.case.id);
        assert_eq!(
            back.left.unwrap().documents,
            std::slice::from_ref(&memo.path)
        );
        assert_eq!(
            std::fs::read_to_string(&letter.path).unwrap(),
            "PERSON_1 wrote to PERSON_2 about Carl Diaz."
        );
        assert_eq!(
            library
                .reply(letter.case.id.clone(), letter.path.clone())
                .unwrap(),
            Some(reply.into())
        );
        assert_eq!(
            restored(&library, &letter.case.id, "PERSON_1, PERSON_2."),
            "Ann Lee, Bob Hart."
        );
        let memo_case = library.load(&memo.case.id).unwrap();
        assert!(memo_case.identities.iter().all(|i| i.alias != "PERSON_3"));
        assert_eq!(memo_case.identities.len(), 2);
        let memo_reopened = DocumentSession::new()
            .open_document(library.clone(), memo.path.clone())
            .unwrap();
        assert!(
            memo_reopened
                .identities
                .iter()
                .all(|i| i.alias != "PERSON_3")
        );
    }

    /// The case a document leaves stays, even empty (ADR 0041: a case is
    /// deleted only on its own). Moving back keeps the names the move added
    /// where something still uses them: another document of that case, or
    /// a reply to text shared from it (the current case).
    #[test]
    fn a_case_left_stays_and_moving_back_keeps_names_in_use() {
        let scratch = Scratch::new();
        let library = library(&scratch);
        let letter = saved(&library, "Ann Lee wrote.", &["Ann Lee"], "Letter");
        let memo = saved(&library, "Bob Hart wrote.", &["Bob Hart"], "Memo");
        let moved = library
            .move_document(letter.path.clone(), memo.case.id.clone())
            .unwrap();
        assert_eq!(moved.added, ["PERSON_2"]);
        assert!(moved.left.unwrap().documents.is_empty());
        assert_eq!(library.cases().cases.len(), 2);
        assert_eq!(restored(&library, &letter.case.id, "PERSON_1"), "Ann Lee");
        assert_eq!(
            std::fs::read_to_string(&letter.path).unwrap(),
            "PERSON_2 wrote."
        );
        // Another document of Memo's case names Ann now.
        let note = self::session("Ann Lee called.", "Note");
        note.join_case(library.clone(), memo.case.id.clone())
            .unwrap();
        select(&note, "Ann Lee");
        note.replace_all().unwrap();
        let note = note.save(library.clone()).unwrap();
        assert_eq!(
            std::fs::read_to_string(&note.path).unwrap(),
            "PERSON_2 called."
        );
        let back = library
            .move_back(letter.path.clone(), letter.case.id.clone(), moved.added)
            .unwrap();
        assert_eq!(back.case.id, letter.case.id);
        assert_eq!(
            std::fs::read_to_string(&letter.path).unwrap(),
            "PERSON_1 wrote."
        );
        assert_eq!(restored(&library, &memo.case.id, "PERSON_2"), "Ann Lee");
        // Moved into a case shared since: a reply may name Ann there.
        let other = saved(&library, "Dan Roe wrote.", &["Dan Roe"], "Other");
        let moved = library
            .move_document(letter.path.clone(), other.case.id.clone())
            .unwrap();
        assert_eq!(moved.added, ["PERSON_2"]);
        library.note_shared(other.case.id.clone()).unwrap();
        library
            .move_back(letter.path.clone(), letter.case.id.clone(), moved.added)
            .unwrap();
        assert_eq!(restored(&library, &other.case.id, "PERSON_2"), "Ann Lee");
        // Moving back into the case it is in is refused, as a move is.
        assert!(matches!(
            library.move_back(letter.path.clone(), letter.case.id.clone(), Vec::new()),
            Err(CoreError::Case { .. })
        ));
    }

    #[test]
    fn a_move_into_its_own_or_a_missing_case_is_refused() {
        let scratch = Scratch::new();
        let library = library(&scratch);
        let letter = saved(&library, "Ann Lee wrote.", &["Ann Lee"], "Letter");
        assert!(matches!(
            library.move_document(letter.path.clone(), letter.case.id.clone()),
            Err(CoreError::Case { .. })
        ));
        for id in ["0".repeat(32), "not an id".into()] {
            assert!(matches!(
                library.move_document(letter.path.clone(), id),
                Err(CoreError::Case { .. })
            ));
        }
        let outside = scratch.path().join("outside.md");
        std::fs::write(&outside, "Ann Lee").unwrap();
        assert!(matches!(
            library.move_document(outside.display().to_string(), letter.case.id.clone()),
            Err(CoreError::Invalid { .. })
        ));
        assert!(matches!(
            library.move_document_to_new_case(letter.path.clone(), " ".into()),
            Err(CoreError::Invalid { .. })
        ));
        assert_eq!(
            std::fs::read_to_string(&letter.path).unwrap(),
            "PERSON_1 wrote."
        );
        assert_eq!(
            library.case_info(letter.case.id).unwrap().documents,
            std::slice::from_ref(&letter.path)
        );
    }

    /// New case…: the document's names keep their aliases in a case of
    /// their own; the case it left stays.
    #[test]
    fn a_document_moves_into_a_new_case() {
        let scratch = Scratch::new();
        let library = library(&scratch);
        let letter = saved(&library, "Ann Lee wrote.", &["Ann Lee"], "Letter");
        let moved = library
            .move_document_to_new_case(letter.path.clone(), " Ivanov ".into())
            .unwrap();
        assert_eq!(moved.case.name, "Ivanov");
        assert_eq!(moved.case.names, 1);
        assert_eq!(moved.added, ["PERSON_1"]);
        assert_eq!(
            std::fs::read_to_string(&letter.path).unwrap(),
            "PERSON_1 wrote."
        );
        assert_eq!(restored(&library, &moved.case.id, "PERSON_1"), "Ann Lee");
        assert_eq!(library.cases().cases.len(), 2);
        assert!(
            library
                .case_info(letter.case.id)
                .unwrap()
                .documents
                .is_empty()
        );
    }

    /// A file that cannot be rewritten leaves the document in the case it
    /// was in, and the case it was joining as it was.
    #[test]
    #[cfg(unix)]
    fn a_move_whose_file_cannot_be_written_changes_nothing() {
        use std::os::unix::fs::PermissionsExt;
        let scratch = Scratch::new();
        let library = library(&scratch);
        let letter = saved(&library, "Ann Lee wrote.", &["Ann Lee"], "Letter");
        let memo = saved(&library, "Bob Hart wrote.", &["Bob Hart"], "Memo");
        let before = library.load(&memo.case.id).unwrap();
        let documents = library.documents_dir();
        let mode = |mode| {
            std::fs::set_permissions(&documents, std::fs::Permissions::from_mode(mode)).unwrap()
        };
        mode(0o555);
        let result = library.move_document(letter.path.clone(), memo.case.id.clone());
        mode(0o755);
        assert!(matches!(result, Err(CoreError::Io { .. })), "{result:?}");
        assert_eq!(
            std::fs::read_to_string(&letter.path).unwrap(),
            "PERSON_1 wrote."
        );
        let after = library.load(&memo.case.id).unwrap();
        assert_eq!(
            (&after.identities, &after.members),
            (&before.identities, &before.members)
        );
        assert_eq!(
            library.case_info(letter.case.id).unwrap().documents,
            std::slice::from_ref(&letter.path)
        );
    }

    /// Renaming an open document keeps its review and history; its next
    /// save goes to the renamed file. Not saved yet, its case follows.
    #[test]
    fn an_open_document_is_renamed_in_place() {
        let scratch = Scratch::new();
        let library = library(&scratch);
        let session = session("Ann Lee wrote to Bob Hart.", "Sample: services agreement");
        select(&session, "Ann Lee");
        session.replace_all().unwrap();
        let first = session.save(library.clone()).unwrap();
        assert_eq!(first.case.name, "Sample: services agreement");
        assert_eq!(
            library.documents().unwrap()[0].title,
            "Sample: services agreement"
        );
        let renamed = session
            .rename_document(library.clone(), "Services: final".into())
            .unwrap();
        assert_eq!(renamed.case.unwrap().name, "Services: final");
        let snapshot = session.snapshot().unwrap();
        assert_eq!(snapshot.title, "Services: final");
        assert!(snapshot.can_undo);
        select(&session, "Bob Hart");
        session.replace_all().unwrap();
        let again = session.save(library.clone()).unwrap();
        assert_eq!(Some(again.path.clone()), renamed.path);
        assert_eq!(
            std::fs::read_to_string(&again.path).unwrap(),
            "PERSON_1 wrote to PERSON_2."
        );
        assert!(!Path::new(&first.path).exists());
        assert_eq!(library.documents().unwrap().len(), 1);
        // Before its first save: the title, and its empty case named after it.
        let case = library.create_case("Draft".into()).unwrap();
        let draft = self::session("Text.", "Draft");
        draft.join_case(library.clone(), case.id.clone()).unwrap();
        let renamed = draft
            .rename_document(library.clone(), "Draft: two".into())
            .unwrap();
        assert_eq!(renamed.path, None);
        assert_eq!(renamed.case.unwrap().name, "Draft: two");
        let saved = draft.save(library.clone()).unwrap();
        assert!(saved.path.ends_with("Draft\u{A789} two.md"));
    }

    #[test]
    fn a_save_never_overwrites_a_change_made_elsewhere() {
        let scratch = Scratch::new();
        let library = library(&scratch);
        let session = session("Ann Lee wrote.", "Letter");
        select(&session, "Ann Lee");
        session.replace_all().unwrap();
        let saved = session.save(library.clone()).unwrap();
        std::fs::write(&saved.path, "Edited in Files.").unwrap();
        assert!(matches!(
            session.save(library.clone()),
            Err(CoreError::Io { .. })
        ));
        assert_eq!(
            std::fs::read_to_string(&saved.path).unwrap(),
            "Edited in Files."
        );
    }
}
