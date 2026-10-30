//! `DocumentSession` (ADR 0085): one document from intake to the AI and
//! back. Import and OCR through `mdoc_import`, scans through `mdoc_detect`,
//! the review of `mdoc_pii` as snapshots and transitions, Copy for AI and
//! Share text, and the mapping saved into its case. Long jobs report their
//! stage to the platform and stop when it cancels; the platform runs them
//! off its main thread.
mod intake;
mod review;
mod snapshot;

use crate::{
    CoreError,
    library::{CaseInfo, Library, RestoredReply, TextRange},
    setup::{DetectorPaths, OcrPaths},
};
use mdoc_pii::restore::RestoreMap;
use review::ReviewText;
pub use review::Scope;
pub use snapshot::{Category, IdentityRow, LinkHint, Mention, MentionState, ReviewSnapshot};
use std::{
    collections::HashSet,
    path::PathBuf,
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
}

struct Document {
    title: String,
    warning: Option<String>,
    review: ReviewText,
    /// The case the document joined or started; it never changes.
    case: Option<String>,
    /// Categories its case does not detect (ADR 0041); scans drop them.
    skip: HashSet<mdoc_pii::Category>,
    /// Where `save` keeps the Markdown.
    path: Option<PathBuf>,
    /// Bumped when an import replaces the document, so a late scan of the
    /// one before cannot land on it.
    generation: u64,
}

#[derive(uniffi::Object)]
pub struct DocumentSession {
    document: Mutex<Option<Document>>,
    /// The running job's cancel flag.
    job: Mutex<Arc<AtomicBool>>,
    generation: Mutex<u64>,
}

#[uniffi::export]
impl DocumentSession {
    #[uniffi::constructor]
    pub fn new() -> Arc<Self> {
        Arc::new(Self {
            document: Mutex::new(None),
            job: Mutex::new(Arc::new(AtomicBool::new(false))),
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
        )?;
        Ok(self.open(imported))
    }

    /// Pasted or shared text, taken as Markdown.
    pub fn import_text(&self, text: String, title: String) -> ReviewSnapshot {
        self.start_job();
        self.open(intake::Imported {
            title,
            markdown: text,
            warning: None,
        })
    }

    /// Stop the running import or scan; it fails with `CoreError::Cancelled`
    /// at its next checkpoint (model loads and native calls finish first).
    pub fn cancel(&self) {
        self.lock_job().store(true, Ordering::Relaxed);
    }

    pub fn snapshot(&self) -> Result<ReviewSnapshot, CoreError> {
        self.with_document(|_| Ok(()))
    }

    /// Scan for candidates: the GLiNER2 detector and the structured rules,
    /// or the rules alone on devices without the detector (ADR 0085). The
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
                mdoc_detect::scan_at(
                    &source,
                    &cancel,
                    std::path::Path::new(&paths.model),
                    std::path::Path::new(&paths.onnx),
                    &mdoc_store::settings::PiiConfig::default(),
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

    /// Link identity `from` into identity `into`, which keeps its alias.
    pub fn link(&self, from: u64, into: u64) -> Result<ReviewSnapshot, CoreError> {
        self.with_document(|document| document.review.link(from, into))
    }

    /// Copy for AI: the alias-only note, then the text, byte for byte what
    /// desktop Copy for AI copies.
    pub fn copy_for_ai(&self) -> Result<String, CoreError> {
        self.read_document(|document| document.review.review.copy_for_ai(&document.review.text))
    }

    /// What Share sends: the same text as Copy for AI (ADR 0085).
    pub fn share_text(&self) -> Result<String, CoreError> {
        self.copy_for_ai()
    }

    /// A name for the shared `.md` file: the title with this document's
    /// known originals replaced by their aliases.
    pub fn share_file_name(&self) -> Result<String, CoreError> {
        self.read_document(|document| {
            let map = RestoreMap::new(document.review.review.export_identities());
            let (title, _) = map.pseudonymize(&document.title);
            let title: String = title
                .chars()
                .map(|c| if "/\\:".contains(c) { '-' } else { c })
                .collect();
            format!("{}.md", title.trim())
        })
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
                document.case = Some(case_id.clone());
                document.skip = case.skipped();
                Ok(())
            }
        })
    }

    /// Save the Markdown into the library's documents folder and its
    /// mapping into its case, starting a case named after the document when
    /// it has none (ADR 0085: each prepared document starts a new case).
    pub fn save(&self, library: Arc<Library>) -> Result<SavedDocument, CoreError> {
        let mut guard = self.lock_document();
        let document = guard.as_mut().ok_or_else(no_document)?;
        let path = match &document.path {
            Some(path) => path.clone(),
            None => {
                std::fs::create_dir_all(library.documents_dir())?;
                library.new_document_path(&document.title)
            }
        };
        let member = library.member(&path);
        let exported = document.review.review.export_identities();
        if let Some(id) = &document.case {
            library.refuse_other_case(&member, id)?;
        }
        mdoc_store::document::Document::default().save(path.clone(), &document.review.text)?;
        // A failed case write keeps this file for the retry.
        document.path = Some(path.clone());
        let case = match &document.case {
            Some(id) => library.edit(id, |case| {
                let before = case.clone();
                case.merge(&exported);
                if !case.members.contains(&member) {
                    case.members.push(member.clone());
                }
                *case != before
            })?,
            None => library.create(mdoc_store::cases::Case::new(
                document.title.clone(),
                &exported,
                Some(member),
            ))?,
        };
        document.case = Some(case.id.clone());
        Ok(SavedDocument {
            path: path.display().to_string(),
            case,
        })
    }
}

impl Document {
    /// Scan results, minus the categories the case skips
    /// (`complete_pii_scan`).
    fn ingest(&mut self, mut detections: Vec<mdoc_pii::Detection>) -> Result<(), CoreError> {
        detections.retain(|d| !self.skip.contains(&d.category));
        self.review.ingest(detections)
    }
}

impl DocumentSession {
    fn open(&self, imported: intake::Imported) -> ReviewSnapshot {
        let generation = {
            let mut generation = self.generation.lock().unwrap_or_else(|e| e.into_inner());
            *generation += 1;
            *generation
        };
        let document = Document {
            title: imported.title,
            warning: imported.warning,
            review: ReviewText::new(imported.markdown),
            case: None,
            skip: HashSet::new(),
            path: None,
            generation,
        };
        let snapshot = snapshot::of(
            &document.title,
            document.warning.as_deref(),
            &document.review,
            None,
        );
        *self.lock_document() = Some(document);
        snapshot
    }

    /// A new job: the one before stops at its next checkpoint.
    fn start_job(&self) -> Arc<AtomicBool> {
        let mut job = self.lock_job();
        job.store(true, Ordering::Relaxed);
        *job = Arc::new(AtomicBool::new(false));
        job.clone()
    }

    fn lock_job(&self) -> MutexGuard<'_, Arc<AtomicBool>> {
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
        Ok(snapshot::of(
            &document.title,
            document.warning.as_deref(),
            &document.review,
            document.case.as_deref(),
        ))
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
