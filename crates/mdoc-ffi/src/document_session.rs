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
            None,
        )?;
        self.open(imported, &cancel, None)
    }

    /// `import_file`, with a PDF's OCR keeping its finished pages under
    /// `library`'s root (ADR 0085 Long jobs): after a cancel, or mdoc
    /// stopping, importing the same file with the same OCR model again
    /// recognizes only the pages left, and gives the Markdown an
    /// uninterrupted run gives. The kept pages are removed when the import
    /// completes, or by `Library::discard_import`.
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
            title: intake::title(Path::new(&path)),
            markdown: stored.saved.clone(),
            warning: None,
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
        self.read_document(|document| document.review.copy_for_ai())
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
    /// The document first joins its case as stored now, so names another
    /// document added meanwhile keep their aliases and this one's move to
    /// free numbers; one library write lock covers the whole save.
    pub fn save(&self, library: Arc<Library>) -> Result<SavedDocument, CoreError> {
        let mut guard = self.lock_document();
        let document = guard.as_mut().ok_or_else(no_document)?;
        let lock = library.lock()?;
        let path = match &document.path {
            Some(path) => path.clone(),
            None => {
                let path = library.reserve_document_path(&document.title)?;
                document.path = Some(path.clone());
                path
            }
        };
        let member = library.member(&path);
        library.refuse_other_case(&member, document.case.as_deref().unwrap_or_default())?;
        let case = match document.case.clone() {
            Some(id) => library.edit_locked(&lock, &id, |case| {
                document
                    .review
                    .join(&case.exported(), &case.reserved_aliases())?;
                document.stored.save(path.clone(), &document.review.text)?;
                let before = case.clone();
                case.merge(&document.review.review.export_identities());
                if !case.members.contains(&member) {
                    case.members.push(member.clone());
                }
                Ok(*case != before)
            })?,
            None => {
                document.stored.save(path.clone(), &document.review.text)?;
                let case = mdoc_store::cases::Case::new(
                    document.title.clone(),
                    &document.review.review.export_identities(),
                    Some(member),
                );
                library.create_locked(&lock, case)?
            }
        };
        document.case = Some(case.id.clone());
        Ok(SavedDocument {
            path: path.display().to_string(),
            case,
            review: document.snapshot(),
        })
    }
}

/// What a document the library keeps brings when it is opened.
struct Opened {
    path: Option<PathBuf>,
    stored: mdoc_store::document::Document,
    case: Option<mdoc_store::cases::Case>,
}

impl Document {
    fn snapshot(&self) -> ReviewSnapshot {
        snapshot::of(
            &self.title,
            self.warning.as_deref(),
            &self.review,
            self.case.as_deref(),
        )
    }

    /// Scan results, minus the categories the case skips
    /// (`complete_pii_scan`).
    fn ingest(&mut self, mut detections: Vec<mdoc_pii::Detection>) -> Result<(), CoreError> {
        detections.retain(|d| !self.skip.contains(&d.category));
        self.review.ingest(detections)
    }
}

impl DocumentSession {
    /// Make `imported` the session's document, unless its job was
    /// cancelled or a newer job started: then the current one stays.
    fn open(
        &self,
        imported: intake::Imported,
        cancel: &Arc<AtomicBool>,
        opened: Option<Opened>,
    ) -> Result<ReviewSnapshot, CoreError> {
        let job = self.lock_job();
        if !Arc::ptr_eq(&job, cancel) || cancel.load(Ordering::Relaxed) {
            return Err(CoreError::Cancelled);
        }
        let generation = {
            let mut generation = self.generation.lock().unwrap_or_else(|e| e.into_inner());
            *generation += 1;
            *generation
        };
        let mut document = Document {
            title: imported.title,
            warning: imported.warning,
            review: ReviewText::new(imported.markdown),
            case: None,
            skip: HashSet::new(),
            path: None,
            stored: mdoc_store::document::Document::default(),
            generation,
        };
        if let Some(opened) = opened {
            document.path = opened.path;
            document.stored = opened.stored;
            if let Some(case) = opened.case {
                document
                    .review
                    .open_case(&case.exported(), &case.reserved_aliases());
                document.skip = case.skipped();
                document.case = Some(case.id);
            }
        }
        let snapshot = document.snapshot();
        *self.lock_document() = Some(document);
        drop(job);
        Ok(snapshot)
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
