//! `Library` (ADR 0085): the cases and stored documents under one storage
//! root, the App Group container on iOS, shared by the app and its Restore
//! extension. Cases are the desktop's encrypted files
//! (`mdoc_store::cases::Store`) under the key the platform passes in; Rust
//! never touches a keychain here.
mod state;
#[cfg(test)]
mod summary_tests;

use crate::CoreError;
use mdoc_pii::restore::RestoreMap;
use mdoc_store::cases::{Case, CaseLink, Store};
pub(crate) use state::Counts;
use state::{DocumentState, Recorded, Stamp};
use std::{
    path::{Path, PathBuf},
    sync::{Arc, Mutex},
};

const CASES: &str = "cases";
const DOCUMENTS: &str = "documents";
/// OCR's finished pages of imports not yet complete (ADR 0085 Long jobs).
const IMPORTS: &str = "imports";
/// The case of the most recently shared document: an id, no names.
const CURRENT_CASE: &str = "current-case.json";
/// Locked while a case is written, in the cases folder.
const LOCK: &str = ".lock";

/// The library's write lock (`Library::lock`).
pub(crate) struct WriteLock<'a> {
    _file: std::fs::File,
    _guard: std::sync::MutexGuard<'a, ()>,
}

#[derive(uniffi::Object)]
pub struct Library {
    root: PathBuf,
    store: Store,
    /// One load, change and save at a time in this process.
    writes: Mutex<()>,
    /// Documents `document_summaries` read to count them.
    #[cfg(test)]
    pub(crate) summary_opens: std::sync::atomic::AtomicUsize,
}

/// The longest file stem a title makes, in bytes: room for " 99.md" in a
/// 255-byte file name.
const MAX_STEM: usize = 240;

/// Characters a file name cannot hold everywhere, and the look-alikes that
/// stand for them, so a title reads back from its file as it was typed. A
/// title that really holds one of the look-alikes (a fullwidth ？＂＜＞, or
/// ∕ ⧵ ꞉ ∗ ∣) reads back with the ASCII character instead; accepted, as
/// those are rare in titles.
const FILE_NAME_STAND_INS: [(char, char); 9] = [
    ('/', '\u{2215}'),  // ∕ DIVISION SLASH
    ('\\', '\u{29F5}'), // ⧵ REVERSE SOLIDUS OPERATOR
    (':', '\u{A789}'),  // ꞉ MODIFIER LETTER COLON
    ('*', '\u{2217}'),  // ∗ ASTERISK OPERATOR
    ('?', '\u{FF1F}'),  // ？ FULLWIDTH QUESTION MARK
    ('"', '\u{FF02}'),  // ＂ FULLWIDTH QUOTATION MARK
    ('<', '\u{FF1C}'),  // ＜ FULLWIDTH LESS-THAN SIGN
    ('>', '\u{FF1E}'),  // ＞ FULLWIDTH GREATER-THAN SIGN
    ('|', '\u{2223}'),  // ∣ DIVIDES
];

/// The case a document moves into.
enum Target {
    Case(String),
    New(String),
}

/// A case as lists show it.
#[derive(Clone, Debug, PartialEq, uniffi::Record)]
pub struct CaseInfo {
    pub id: String,
    pub name: String,
    /// The case colour's hue in degrees (0..360).
    pub hue: u32,
    /// Seconds since the Unix epoch.
    pub created: u64,
    /// Member documents, as absolute paths.
    pub documents: Vec<String>,
    /// Identities that still restore (forgotten ones keep only aliases).
    pub names: u64,
}

/// Every readable case, newest first, and how many case files could not be
/// read (damaged, or sealed with another key).
#[derive(Clone, Debug, PartialEq, uniffi::Record)]
pub struct CaseList {
    pub cases: Vec<CaseInfo>,
    pub unreadable: u64,
}

/// A Markdown document in the library's documents folder.
#[derive(Clone, Debug, PartialEq, uniffi::Record)]
pub struct StoredDocument {
    pub path: String,
    pub title: String,
    pub case_id: Option<String>,
    /// Seconds since the Unix epoch.
    pub modified: u64,
}

/// A document as lists show it (ADR 0104): its review's counts and phase
/// without opening it. Nothing in it names a person beyond the title, which
/// the file name holds already.
#[derive(Clone, Debug, PartialEq, uniffi::Record)]
pub struct DocumentSummary {
    pub path: String,
    pub title: String,
    pub case_id: Option<String>,
    pub phase: crate::document_session::DocumentPhase,
    pub scanned: bool,
    /// Proposals left, as the snapshot counts them.
    pub proposed: u64,
    pub replaced: u64,
    pub kept: u64,
    /// The file's last change, in seconds since the Unix epoch.
    pub modified: u64,
    /// When it was last opened or saved, in seconds since the Unix epoch;
    /// `None` for one not opened since the library began keeping it.
    pub opened: Option<u64>,
}

/// Where `rename_document` left a document.
#[derive(Clone, Debug, PartialEq, uniffi::Record)]
pub struct RenamedDocument {
    /// Its file now; `None` for a document not saved yet.
    pub path: Option<String>,
    /// Its title now: the title asked for, numbered ("Letter 2") when
    /// another document has it, and shortened when it is too long for a
    /// file name.
    pub title: String,
    /// Its case, renamed too when it held only this document and was named
    /// after it.
    pub case: Option<CaseInfo>,
}

/// Where `move_document` left a document.
#[derive(Clone, Debug, PartialEq, uniffi::Record)]
pub struct MovedDocument {
    /// The document's file, which stays where it was.
    pub path: String,
    /// The case it is in now.
    pub case: CaseInfo,
    /// The case it left, as saved after it left, if it was in one. It
    /// stays, even with no documents left. Undo is `move_back(path,
    /// left.id, added)`.
    pub left: Option<CaseInfo>,
    /// Aliases of the identities this move added to `case`: the names it
    /// did not know. `move_back` takes them out again.
    pub added: Vec<String>,
}

/// A byte range in UTF-8 text.
#[derive(Clone, Copy, Debug, PartialEq, Eq, uniffi::Record)]
pub struct TextRange {
    pub start: u64,
    pub end: u64,
}

impl TextRange {
    pub(crate) fn of(range: &std::ops::Range<usize>) -> Self {
        Self {
            start: range.start as u64,
            end: range.end as u64,
        }
    }
}

/// A reply restored in place (ADR 0078): the restored text, where restored
/// values and unresolved alias-shaped tokens sit in it, and the counts for
/// "N values restored · M unrecognised".
#[derive(Clone, Debug, PartialEq, uniffi::Record)]
pub struct RestoredReply {
    pub text: String,
    pub restored: Vec<TextRange>,
    pub unresolved: Vec<TextRange>,
    /// Distinct identities restored.
    pub values: u64,
    /// Distinct unresolved tokens.
    pub unrecognised: u64,
    /// Whether the mapping had identities to restore: Copy and Share are
    /// off without them.
    pub restorable: bool,
}

impl RestoredReply {
    pub(crate) fn of(map: &RestoreMap, reply: &str) -> Self {
        let restored = map.restore_in_place(reply);
        Self {
            restored: restored
                .restored
                .iter()
                .map(|(r, _)| TextRange::of(r))
                .collect(),
            unresolved: restored.unresolved.iter().map(TextRange::of).collect(),
            values: restored.values as u64,
            unrecognised: restored
                .unresolved
                .iter()
                .filter_map(|range| restored.text.get(range.clone()))
                .collect::<std::collections::HashSet<_>>()
                .len() as u64,
            restorable: !map.is_empty(),
            text: restored.text,
        }
    }
}

#[uniffi::export]
impl Library {
    /// The library under `root`, its cases sealed with `case_key` (32 bytes).
    #[uniffi::constructor]
    pub fn new(root: String, case_key: Vec<u8>) -> Result<Arc<Self>, CoreError> {
        let key: [u8; 32] = case_key
            .as_slice()
            .try_into()
            .map_err(|_| CoreError::invalid("The case key must be 32 bytes."))?;
        let root = PathBuf::from(root);
        Ok(Arc::new(Self {
            store: Store::with_key(root.join(CASES), &key),
            root,
            writes: Mutex::new(()),
            #[cfg(test)]
            summary_opens: Default::default(),
        }))
    }

    /// Where case files live.
    pub fn cases_folder(&self) -> String {
        self.root.join(CASES).display().to_string()
    }

    /// Where the app keeps its Markdown documents.
    pub fn documents_folder(&self) -> String {
        self.documents_dir().display().to_string()
    }

    pub fn cases(&self) -> CaseList {
        let (cases, failed) = self.store.list();
        CaseList {
            cases: cases.iter().map(|case| self.info(case)).collect(),
            unreadable: failed.len() as u64,
        }
    }

    pub fn case_info(&self, id: String) -> Result<CaseInfo, CoreError> {
        Ok(self.info(&self.load(&id)?))
    }

    /// The documents folder's Markdown files, most recently changed first,
    /// each with the case it belongs to.
    pub fn documents(&self) -> Result<Vec<StoredDocument>, CoreError> {
        let (cases, _) = self.store.list();
        let mut documents = Vec::new();
        for (path, metadata) in self.markdown_files()? {
            let member = self.member(&path);
            let modified = Stamp::of(&metadata).modified;
            documents.push(StoredDocument {
                title: title_of(&path),
                case_id: cases
                    .iter()
                    .find(|case| case.members.contains(&member))
                    .map(|case| case.id.clone()),
                path: path.display().to_string(),
                modified,
            });
        }
        documents.sort_by(|a, b| b.modified.cmp(&a.modified).then(a.title.cmp(&b.title)));
        self.prune_document_states();
        Ok(documents)
    }

    /// Every document in the documents folder, most recently changed first,
    /// with its case and its review's counts and phase as of its last save
    /// or open, read from the library's record without opening it. Only a
    /// document whose file changed since (moved between cases, edited
    /// elsewhere) or that has no record (saved before the library kept
    /// one) is read and counted, as opening it would count it; its record
    /// is brought up to date for the next list. The cases are read once.
    pub fn document_summaries(&self) -> Result<Vec<DocumentSummary>, CoreError> {
        let (cases, _) = self.store.list();
        let mut states = self.states();
        let mut changed = Vec::new();
        let mut summaries = Vec::new();
        for (path, metadata) in self.markdown_files()? {
            let member = self.member(&path);
            let case = cases.iter().find(|case| case.members.contains(&member));
            let key = self.key(&path);
            let stamp = Stamp::of(&metadata);
            let state = states.documents.get(&key);
            let (counts, scanned) = match state {
                Some(DocumentState {
                    scanned,
                    summary: Some(recorded),
                    ..
                }) if recorded.stamp == stamp => (recorded.counts, *scanned),
                _ => {
                    #[cfg(test)]
                    self.summary_opens
                        .fetch_add(1, std::sync::atomic::Ordering::Relaxed);
                    let text = match std::fs::read_to_string(&path) {
                        Ok(text) => text,
                        Err(error) if error.kind() == std::io::ErrorKind::NotFound => continue,
                        Err(error) => return Err(error.into()),
                    };
                    let counts = crate::document_session::reopened_counts(text, case);
                    // As `open_document` tells a document without a record.
                    let scanned = state
                        .map_or(counts.proposed + counts.replaced + counts.kept > 0, |s| {
                            s.scanned
                        });
                    let fresh = DocumentState {
                        scanned,
                        summary: Some(Recorded { counts, stamp }),
                        opened: state.and_then(|s| s.opened),
                    };
                    changed.push((key.clone(), fresh));
                    (counts, scanned)
                }
            };
            summaries.push(DocumentSummary {
                title: title_of(&path),
                case_id: case.map(|case| case.id.clone()),
                phase: crate::document_session::DocumentPhase::of(scanned, counts.proposed),
                scanned,
                proposed: counts.proposed,
                replaced: counts.replaced,
                kept: counts.kept,
                modified: stamp.modified,
                opened: states.documents.get(&key).and_then(|s| s.opened),
                path: path.display().to_string(),
            });
        }
        summaries.sort_by(|a, b| b.modified.cmp(&a.modified).then(a.title.cmp(&b.title)));
        // Records brought up to date, and those of files gone dropped;
        // best effort, as a list never fails for them.
        let gone = states
            .documents
            .keys()
            .any(|key| !self.root.join(key).exists());
        if (!changed.is_empty() || gone)
            && let Ok(_lock) = self.lock()
        {
            states = self.states();
            states
                .documents
                .retain(|key, _| self.root.join(key).exists());
            states.documents.extend(changed);
            let _ = self.write_states(&states);
        }
        Ok(summaries)
    }

    /// A new, empty case. Documents usually start their own case when they
    /// are saved (`DocumentSession::save`).
    pub fn create_case(&self, name: String) -> Result<CaseInfo, CoreError> {
        self.create(Case::new(case_name(&name)?, &[], None))
    }

    /// Make the document at `path` a member of case `case_id`. A document
    /// belongs to one case at most (ADR 0041).
    pub fn add_document(&self, case_id: String, path: String) -> Result<CaseInfo, CoreError> {
        let member = self.member(Path::new(&path));
        self.refuse_other_case(&member, &case_id)?;
        self.edit(&case_id, |case| {
            if case.members.contains(&member) {
                return false;
            }
            case.members.push(member);
            true
        })
    }

    /// A document of case `case_id` was shared: the Restore extension uses
    /// this case until another is shared (ADR 0084's current case).
    pub fn note_shared(&self, case_id: String) -> Result<(), CoreError> {
        self.load(&case_id)?;
        let path = self.root.join(CURRENT_CASE);
        let json = serde_json::json!({ "case": case_id }).to_string();
        std::fs::create_dir_all(&self.root)?;
        let partial = path.with_extension("json.part");
        std::fs::write(&partial, json)?;
        std::fs::rename(partial, path)?;
        Ok(())
    }

    /// The case of the most recently shared document, while it still exists.
    pub fn current_case(&self) -> Option<CaseInfo> {
        self.case_info(self.current_case_id()?).ok()
    }

    /// Restore `reply` in place with case `case_id` alone, the Restore
    /// extension's path: aliases repeat across cases, so cases never mix.
    pub fn restore(&self, case_id: String, reply: String) -> Result<RestoredReply, CoreError> {
        let case = self.load(&case_id)?;
        Ok(RestoredReply::of(&RestoreMap::new(case.exported()), &reply))
    }

    /// Keep `reply`, as pasted (aliases, not restored text), as the reply of
    /// `document`, a member of case `case_id` (ADR 0078); a blank reply
    /// removes it.
    pub fn save_reply(
        &self,
        case_id: String,
        document: String,
        reply: String,
    ) -> Result<(), CoreError> {
        let member = self.member(Path::new(&document));
        let text = (!reply.trim().is_empty()).then_some(reply);
        let mut is_member = true;
        self.edit(&case_id, |case| {
            is_member = case.members.contains(&member);
            is_member && case.set_reply(&member, text)
        })?;
        if is_member {
            Ok(())
        } else {
            Err(CoreError::case("This document is not in the case."))
        }
    }

    /// Delete `path`, a document in the documents folder: it leaves its
    /// case, with the reply kept for it (ADR 0078), then its file is
    /// removed, under the library's write lock. The case is saved first, so
    /// a case that cannot be saved leaves the document as it was, and a
    /// file that cannot be removed (or mdoc stopping in between) leaves an
    /// ordinary document in no case, which deleting again removes. A case
    /// that cannot be read for now refuses the delete; one that never will
    /// (another key, damaged) is passed over. The case stays, even with no
    /// members left, as on desktop; cases are deleted on their own. Returns
    /// the case it left.
    pub fn delete_document(&self, path: String) -> Result<Option<CaseInfo>, CoreError> {
        let path = self.library_document(Path::new(&path), "deleted")?;
        let member = self.member(&path);
        let lock = self.lock()?;
        let mut left = None;
        for case in self.readable_cases("deleted")? {
            if !case.members.contains(&member) && case.reply(&member).is_none() {
                continue;
            }
            left = Some(self.edit_locked(&lock, &case.id, |case| {
                let replied = case.set_reply(&member, None);
                let was_member = case.members.contains(&member);
                case.members.retain(|m| *m != member);
                Ok(replied || was_member)
            })?);
        }
        match std::fs::remove_file(&path) {
            Err(error) if error.kind() != std::io::ErrorKind::NotFound => Err(error.into()),
            _ => {
                // Only the entry is left of it; one left behind is harmless.
                let _ = self.set_document_state(&lock, &path, None);
                Ok(left)
            }
        }
    }

    /// Rename…: give `path`, a document in the documents folder, the title
    /// `title`. Its file is renamed after the title (characters a file name
    /// cannot hold become look-alikes, so the title reads back as typed),
    /// and what cases keep by the file follows it: membership, the reply
    /// kept for it in any case (ADR 0078) and its review fingerprint (ADR
    /// 0084). A case holding only this document and named after it takes
    /// the title too. One write lock covers it; when a case cannot be
    /// saved, the file and the cases saved before it go back as they were.
    /// A document open in a `DocumentSession` is renamed through
    /// `DocumentSession::rename_document`, which keeps the session's file.
    pub fn rename_document(
        &self,
        path: String,
        title: String,
    ) -> Result<RenamedDocument, CoreError> {
        let lock = self.lock()?;
        self.rename_locked(&lock, Path::new(&path), &title)
    }

    /// Rename case `case_id` to `name`; its documents keep their titles.
    pub fn rename_case(&self, case_id: String, name: String) -> Result<CaseInfo, CoreError> {
        let name = case_name(&name)?;
        self.edit(&case_id, |case| {
            let changed = case.name != name;
            case.name = name;
            changed
        })
    }

    /// Move to case: `path`, a document in the documents folder, leaves its
    /// case and joins case `case_id`, as on desktop (ADR 0041 Joining). It
    /// is opened in its case and lined up with the other by
    /// `mdoc_pii::restore::plan_join`: a name the case knows takes its
    /// alias, a clashing alias the case's next unused number, the case's
    /// other names are proposed, and the file is rewritten with the new
    /// aliases. The case it left keeps its identities and the reply kept
    /// for the document, so replies to text already sent still restore with
    /// it, and it stays even with no documents left, as on desktop (a case
    /// is deleted only on its own, confirmed: ADR 0041).
    ///
    /// Undo is `move_back`. Refused for a document already in that case.
    /// The case is saved first, then the file, then the case left; a step
    /// that fails puts the earlier ones back. A document open in a
    /// `DocumentSession` is opened again afterwards
    /// (`DocumentSession::open_document`): its file and case changed.
    pub fn move_document(&self, path: String, case_id: String) -> Result<MovedDocument, CoreError> {
        let lock = self.lock()?;
        self.move_locked(&lock, Path::new(&path), Target::Case(case_id), &[])
    }

    /// Undo of Move to case: `path` moves back into `case_id`, the case it
    /// left (`MovedDocument::left`), as `move_document` moves it. Its names
    /// line up with that case's identities again and take their old aliases
    /// (a name added meanwhile takes a fresh number), and the reply kept
    /// there is the document's again. From the case it leaves now, the
    /// identities the move added (`added`) go again, unless a document still
    /// in that case or a reply kept there uses their alias, or that case is
    /// the current case (a reply to text shared from it may name them).
    pub fn move_back(
        &self,
        path: String,
        case_id: String,
        added: Vec<String>,
    ) -> Result<MovedDocument, CoreError> {
        let lock = self.lock()?;
        self.move_locked(&lock, Path::new(&path), Target::Case(case_id), &added)
    }

    /// Move to case › New case…: `move_document` into a new case named
    /// `name`. A move that fails leaves no new case behind.
    pub fn move_document_to_new_case(
        &self,
        path: String,
        name: String,
    ) -> Result<MovedDocument, CoreError> {
        let name = case_name(&name)?;
        let lock = self.lock()?;
        self.move_locked(&lock, Path::new(&path), Target::New(name), &[])
    }

    /// Remove the OCR pages kept for an import of `source` that has not
    /// completed (`DocumentSession::import_file_in`), when the user drops
    /// it. Call it before removing `source`: kept pages are found by the
    /// file's digest.
    pub fn discard_import(&self, source: String) -> Result<(), CoreError> {
        Ok(self.kept_pages().discard(Path::new(&source))?)
    }

    /// The reply kept for `document` in case `case_id`.
    pub fn reply(&self, case_id: String, document: String) -> Result<Option<String>, CoreError> {
        let member = self.member(Path::new(&document));
        Ok(self.load(&case_id)?.reply(&member).map(str::to_owned))
    }
}

impl Library {
    /// Every readable case.
    pub(crate) fn cases_raw(&self) -> Vec<Case> {
        self.store.list().0
    }

    pub(crate) fn load(&self, id: &str) -> Result<Case, CoreError> {
        Ok(self.store.load(id)?)
    }

    /// Hold the library's write lock until the guard drops: one load,
    /// change and save at a time, across processes too, since the app and
    /// its Restore extension share the folder. Held only for a write, never
    /// across a suspension.
    pub(crate) fn lock(&self) -> Result<WriteLock<'_>, CoreError> {
        let guard = self.writes.lock().unwrap_or_else(|e| e.into_inner());
        let dir = self.root.join(CASES);
        std::fs::create_dir_all(&dir)?;
        let file = std::fs::OpenOptions::new()
            .create(true)
            .truncate(false)
            .write(true)
            .open(dir.join(LOCK))?;
        file.lock()?;
        Ok(WriteLock {
            _file: file,
            _guard: guard,
        })
    }

    /// The write lock if it is free now: for best-effort writes that a
    /// list or an open makes, which never wait on a save in progress.
    pub(crate) fn try_lock(&self) -> Option<WriteLock<'_>> {
        let guard = match self.writes.try_lock() {
            Ok(guard) => guard,
            Err(std::sync::TryLockError::Poisoned(error)) => error.into_inner(),
            Err(std::sync::TryLockError::WouldBlock) => return None,
        };
        let dir = self.root.join(CASES);
        std::fs::create_dir_all(&dir).ok()?;
        let file = std::fs::OpenOptions::new()
            .create(true)
            .truncate(false)
            .write(true)
            .open(dir.join(LOCK))
            .ok()?;
        file.try_lock().ok()?;
        Some(WriteLock {
            _file: file,
            _guard: guard,
        })
    }

    /// Save a new case.
    pub(crate) fn create(&self, case: Case) -> Result<CaseInfo, CoreError> {
        let lock = self.lock()?;
        self.create_locked(&lock, case)
    }

    pub(crate) fn create_locked(&self, _: &WriteLock, case: Case) -> Result<CaseInfo, CoreError> {
        self.store.save(&case)?;
        Ok(self.info(&case))
    }

    /// Change case `id` as stored now; `change` returns whether it changed,
    /// and only then is the next revision saved.
    pub(crate) fn edit(
        &self,
        id: &str,
        change: impl FnOnce(&mut Case) -> bool,
    ) -> Result<CaseInfo, CoreError> {
        let lock = self.lock()?;
        self.edit_locked(&lock, id, |case| Ok(change(case)))
    }

    /// [`Self::edit`] under a lock the caller holds, with a change that may
    /// fail; a failed change saves nothing.
    pub(crate) fn edit_locked(
        &self,
        _: &WriteLock,
        id: &str,
        change: impl FnOnce(&mut Case) -> Result<bool, CoreError>,
    ) -> Result<CaseInfo, CoreError> {
        let mut case = self.load(id)?;
        if change(&mut case)? {
            case.revision += 1;
            self.store.save(&case)?;
        }
        Ok(self.info(&case))
    }

    /// A document belongs to one case at most (ADR 0041): refuse `member`
    /// when a case other than `case_id` has it.
    pub(crate) fn refuse_other_case(&self, member: &Path, case_id: &str) -> Result<(), CoreError> {
        let (cases, _) = self.store.list();
        match cases
            .iter()
            .find(|case| case.id != case_id && case.members.iter().any(|m| m == member))
        {
            Some(other) => Err(CoreError::case(format!(
                "This document is in case “{}” already.",
                other.name
            ))),
            None => Ok(()),
        }
    }

    /// `path`, absolute, when it is a Markdown file in the documents folder;
    /// otherwise refused: only those can be `done`.
    fn library_document(&self, path: &Path, done: &str) -> Result<PathBuf, CoreError> {
        let path = std::path::absolute(path).unwrap_or_else(|_| path.to_path_buf());
        let documents =
            std::path::absolute(self.documents_dir()).unwrap_or_else(|_| self.documents_dir());
        if path.parent() != Some(documents.as_path()) || !mdoc_store::document::is_markdown(&path) {
            return Err(CoreError::invalid(format!(
                "Only the library's documents can be {done}."
            )));
        }
        Ok(path)
    }

    /// Every readable case, for a change that must reach each case keeping
    /// something of a document. A case that cannot be read for now refuses
    /// it (the document is not `done`); one that never will (another key,
    /// damaged) is passed over.
    fn readable_cases(&self, done: &str) -> Result<Vec<Case>, CoreError> {
        let (cases, unreadable) = self.store.list();
        for id in unreadable {
            if let Err(mdoc_store::cases::CaseError::Read(error)) = self.store.load(&id)
                && error.kind() != std::io::ErrorKind::NotFound
            {
                return Err(CoreError::case(format!(
                    "A case could not be read, so the document was not {done}: {error}"
                )));
            }
        }
        Ok(cases)
    }

    /// Save `before` over its case as stored now: a later step failed.
    fn put_back(&self, _: &WriteLock, before: &Case) -> Result<(), CoreError> {
        let mut case = before.clone();
        case.revision = self
            .load(&before.id)
            .map_or(before.revision, |now| now.revision)
            + 1;
        Ok(self.store.save(&case)?)
    }

    /// The id `current-case.json` names, whether or not the case exists.
    fn current_case_id(&self) -> Option<String> {
        let json = std::fs::read_to_string(self.root.join(CURRENT_CASE)).ok()?;
        let value: serde_json::Value = serde_json::from_str(&json).ok()?;
        Some(value.get("case")?.as_str()?.to_owned())
    }

    /// Take out of `case` the identities named by `aliases` (a move added
    /// them) that nothing there uses: no member's text and no kept reply
    /// holds their alias. A member that cannot be read counts as using
    /// them. Returns whether any went.
    fn drop_unused(&self, case: &mut Case, aliases: &[String]) -> bool {
        let candidates: Vec<(usize, mdoc_pii::restore::ExportedIdentity)> =
            (0..case.identities.len())
                .filter(|&i| aliases.contains(&case.identities[i].alias))
                .filter_map(|i| Some((i, case.exported_one(i)?)))
                .collect();
        if candidates.is_empty() {
            return false;
        }
        let map = RestoreMap::new(candidates.iter().map(|(_, e)| e.clone()).collect());
        let mut used = vec![false; candidates.len()];
        let mut mark = |text: &str| {
            for token in map.tokens(text) {
                if let Some(index) = token.identity {
                    used[index] = true;
                }
            }
        };
        for member in &case.members {
            match std::fs::read_to_string(self.root.join(member)) {
                Ok(text) => mark(&text),
                Err(_) => return false,
            }
        }
        for reply in &case.replies {
            mark(&reply.text);
        }
        let unused: Vec<usize> = candidates
            .iter()
            .zip(&used)
            .filter(|(_, used)| !**used)
            .map(|((i, _), _)| *i)
            .collect();
        let mut at = 0;
        case.identities.retain(|_| {
            at += 1;
            !unused.contains(&(at - 1))
        });
        !unused.is_empty()
    }

    /// [`Self::rename_document`] under a lock the caller holds. The file is
    /// renamed first, then each case keeping something of it; a failure puts
    /// the file and the cases already saved back. mdoc stopping between the
    /// file and the cases leaves the document in no case under its new name
    /// (and its case naming a file that is gone). Saving the cases first
    /// would only swap that for the reverse (the cases naming the new file,
    /// the document still under its old name in no case), so neither order
    /// closes the window.
    pub(crate) fn rename_locked(
        &self,
        lock: &WriteLock,
        path: &Path,
        title: &str,
    ) -> Result<RenamedDocument, CoreError> {
        let title = document_title(title)?;
        let path = self.library_document(path, "renamed")?;
        std::fs::metadata(&path)?;
        let cases = self.readable_cases("renamed")?;
        let (renamed, reserved) = self.renamed_path(&path, &title)?;
        if renamed != path
            && let Err(error) = std::fs::rename(&path, &renamed)
        {
            if reserved {
                let _ = std::fs::remove_file(&renamed);
            }
            return Err(error.into());
        }
        let (old, new) = (self.member(&path), self.member(&renamed));
        let mut saved: Vec<Case> = Vec::new();
        let mut info = None;
        for case in cases {
            let member = case.members.contains(&old);
            let rename_case = member
                && case.members.iter().all(|m| *m == old)
                && case.name != title
                && named_after(&case.name, &path);
            let moved = old != new
                && (member || case.reply(&old).is_some() || case.reviewed(&old).is_some());
            if !moved && !rename_case {
                if member {
                    info = Some(self.info(&case));
                }
                continue;
            }
            let edited = self.edit_locked(lock, &case.id, |case| {
                let follow = |m: &mut PathBuf| {
                    if *m == old {
                        *m = new.clone();
                    }
                };
                case.members.iter_mut().for_each(follow);
                case.replies.iter_mut().for_each(|r| follow(&mut r.member));
                case.reviewed.iter_mut().for_each(|r| follow(&mut r.member));
                if rename_case {
                    case.name = title.clone();
                }
                Ok(true)
            });
            match edited {
                Ok(edited) => {
                    if member {
                        info = Some(edited);
                    }
                    saved.push(case);
                }
                Err(error) => {
                    for before in &saved {
                        let _ = self.put_back(lock, before);
                    }
                    if renamed != path {
                        let _ = std::fs::rename(&renamed, &path);
                    }
                    return Err(error);
                }
            }
        }
        // The rename has happened: an entry that cannot follow it only
        // makes the document reopen as it would without one.
        let _ = self.move_document_state(lock, &path, &renamed);
        Ok(RenamedDocument {
            title: title_of(&renamed),
            path: Some(renamed.display().to_string()),
            case: info,
        })
    }

    /// A document not saved yet, in case `case_id` if any, renamed from
    /// `old` to `title`: only an empty case named after it changes.
    pub(crate) fn rename_unsaved_locked(
        &self,
        lock: &WriteLock,
        case_id: Option<&str>,
        old: &str,
        title: &str,
    ) -> Result<RenamedDocument, CoreError> {
        let title = document_title(title)?;
        let case = match case_id {
            Some(id) => Some(self.edit_locked(lock, id, |case| {
                let rename = case.members.is_empty() && case.name == old && case.name != title;
                if rename {
                    case.name = title.clone();
                }
                Ok(rename)
            })?),
            None => None,
        };
        Ok(RenamedDocument {
            path: None,
            title,
            case,
        })
    }

    /// Where a document at `current` titled `title` goes: `current` itself
    /// when the title gives its name (or differs only in letter case on a
    /// file system that ignores it), else a free path, reserved with an
    /// empty file (the second value) so nothing else takes it meanwhile.
    fn renamed_path(&self, current: &Path, title: &str) -> Result<(PathBuf, bool), CoreError> {
        let stem = file_stem(title);
        let dir = current.parent().unwrap_or(current);
        for n in 1.. {
            let candidate = numbered(dir, &stem, n);
            if candidate == current || same_file(&candidate, current) {
                return Ok((candidate, false));
            }
            match std::fs::OpenOptions::new()
                .write(true)
                .create_new(true)
                .open(&candidate)
            {
                Ok(_) => return Ok((candidate, true)),
                Err(error) if error.kind() == std::io::ErrorKind::AlreadyExists => continue,
                Err(error) => return Err(error.into()),
            }
        }
        unreachable!("a free name")
    }

    /// [`Self::move_document`] under a lock the caller holds; `take_back`
    /// is [`Self::move_back`]'s `added`.
    fn move_locked(
        &self,
        lock: &WriteLock,
        path: &Path,
        target: Target,
        take_back: &[String],
    ) -> Result<MovedDocument, CoreError> {
        let path = self.library_document(path, "moved")?;
        let mut stored = mdoc_store::document::Document::open(path.clone())?;
        let cases = self.readable_cases("moved")?;
        // A case made for this move goes again when the move fails.
        let (target, made) = match target {
            Target::Case(id) => (self.load(&id)?, false),
            Target::New(name) => {
                let case = Case::new(name, &[], None);
                self.create_locked(lock, case.clone())?;
                (case, true)
            }
        };
        let moved = self.move_into(lock, &path, &mut stored, &cases, &target, take_back);
        if moved.is_err() && made {
            let _ = self.store.delete(&target.id);
        }
        moved
    }

    /// The move's three writes, each putting the earlier ones back when it
    /// fails. The case files and the document cannot change together, so
    /// mdoc stopping between writes leaves the document a member of both
    /// cases: after the case joined is saved, with its text still in the old
    /// aliases (moving it into the case joined again finishes the move);
    /// after the file is written, in the new ones (moving it into the case
    /// it left undoes it). No order of the writes avoids a moment where the
    /// text and its membership disagree, so none is tried; this one never
    /// leaves the document in no case.
    fn move_into(
        &self,
        lock: &WriteLock,
        path: &Path,
        stored: &mut mdoc_store::document::Document,
        cases: &[Case],
        target: &Case,
        take_back: &[String],
    ) -> Result<MovedDocument, CoreError> {
        let member = self.member(path);
        let from = cases
            .iter()
            .find(|case| case.id != target.id && case.members.contains(&member));
        if from.is_none() && target.members.contains(&member) {
            return Err(CoreError::case("This document is in that case already."));
        }
        let old = stored.saved.clone();
        let (text, exported) = crate::document_session::realias(&old, from, target)?;
        let mut added = Vec::new();
        let case = self.edit_locked(lock, &target.id, |case| {
            let before = case.identities.clone();
            case.merge(&exported);
            added = case
                .identities
                .iter()
                .filter(|i| !before.iter().any(|b| b.names(&i.alias)))
                .map(|i| i.alias.clone())
                .collect();
            if !case.members.contains(&member) {
                case.members.push(member.clone());
            }
            Ok(true)
        })?;
        if text != old
            && let Err(error) = stored.save(path.to_path_buf(), &text)
        {
            let _ = self.put_back(lock, target);
            return Err(error.into());
        }
        let Some(from) = from else {
            return Ok(MovedDocument {
                path: path.display().to_string(),
                case,
                left: None,
                added,
            });
        };
        let shared = self.current_case_id().as_deref() == Some(from.id.as_str());
        let left = match self.edit_locked(lock, &from.id, |case| {
            let changed = case.remove_member(&member);
            let dropped = !shared && self.drop_unused(case, take_back);
            Ok(changed || dropped)
        }) {
            Ok(left) => left,
            Err(error) => {
                if text != old {
                    let _ = stored.save(path.to_path_buf(), &old);
                }
                let _ = self.put_back(lock, target);
                return Err(error);
            }
        };
        Ok(MovedDocument {
            path: path.display().to_string(),
            case,
            left: Some(left),
            added,
        })
    }

    /// The documents folder's Markdown files; none without the folder.
    fn markdown_files(&self) -> Result<Vec<(PathBuf, std::fs::Metadata)>, CoreError> {
        let entries = match std::fs::read_dir(self.documents_dir()) {
            Ok(entries) => entries,
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(Vec::new()),
            Err(error) => return Err(error.into()),
        };
        let mut files = Vec::new();
        for entry in entries {
            let path = entry?.path();
            if !mdoc_store::document::is_markdown(&path) {
                continue;
            }
            // One removed while the folder is read is passed over.
            match std::fs::metadata(&path) {
                Ok(metadata) if metadata.is_file() => files.push((path, metadata)),
                Ok(_) => {}
                Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
                Err(error) => return Err(error.into()),
            }
        }
        Ok(files)
    }

    pub(crate) fn documents_dir(&self) -> PathBuf {
        self.root.join(DOCUMENTS)
    }

    /// Where imports keep their finished OCR pages.
    pub(crate) fn kept_pages(&self) -> mdoc_import::resume::KeptPages {
        mdoc_import::resume::KeptPages::new(self.root.join(IMPORTS))
    }

    /// How a case names a member: relative to the root when inside it, so a
    /// moved container keeps its cases; otherwise the absolute path.
    pub(crate) fn member(&self, path: &Path) -> PathBuf {
        let path = std::path::absolute(path).unwrap_or_else(|_| path.to_path_buf());
        path.strip_prefix(&self.root)
            .map_or_else(|_| path.clone(), Path::to_path_buf)
    }

    /// A new, empty file in the documents folder for a document titled
    /// `title`, created exclusively so two saves never share a path.
    pub(crate) fn reserve_document_path(&self, title: &str) -> Result<PathBuf, CoreError> {
        std::fs::create_dir_all(self.documents_dir())?;
        loop {
            let path = self.new_document_path(title);
            match std::fs::OpenOptions::new()
                .write(true)
                .create_new(true)
                .open(&path)
            {
                Ok(_) => return Ok(path),
                Err(error) if error.kind() == std::io::ErrorKind::AlreadyExists => continue,
                Err(error) => return Err(error.into()),
            }
        }
    }

    /// A free path in the documents folder for a document titled `title`.
    pub(crate) fn new_document_path(&self, title: &str) -> PathBuf {
        let stem = file_stem(title);
        let dir = self.documents_dir();
        (1..)
            .map(|n| numbered(&dir, &stem, n))
            .find(|path| !path.exists())
            .expect("a free name")
    }

    fn info(&self, case: &Case) -> CaseInfo {
        CaseInfo {
            id: case.id.clone(),
            name: case.name.clone(),
            hue: CaseLink::of(case).hue(),
            created: case.created,
            documents: case
                .members
                .iter()
                .map(|member| self.root.join(member).display().to_string())
                .collect(),
            names: case
                .identities
                .iter()
                .filter(|identity| !identity.originals.is_empty())
                .count() as u64,
        }
    }
}

/// A library document's title: its file name without the extension, with
/// the look-alikes a file name holds read back as the characters they stand
/// for (`file_stem`).
pub(crate) fn title_of(path: &Path) -> String {
    path.file_stem().map_or_else(String::new, |stem| {
        stem.to_string_lossy()
            .chars()
            .map(|c| {
                FILE_NAME_STAND_INS
                    .iter()
                    .find(|(_, stand_in)| *stand_in == c)
                    .map_or(c, |(original, _)| *original)
            })
            .collect()
    })
}

/// The file stem for a document titled `title`: characters a file name
/// cannot hold on every platform become look-alikes (`title_of` reads them
/// back), control characters `-`; trimmed, without leading dots, at most
/// `MAX_STEM` bytes, and "Document" when nothing is left.
fn file_stem(title: &str) -> String {
    let stem: String = title
        .chars()
        .map(|c| {
            FILE_NAME_STAND_INS
                .iter()
                .find(|(original, _)| *original == c)
                .map_or(if c.is_control() { '-' } else { c }, |(_, stand_in)| {
                    *stand_in
                })
        })
        .collect();
    let mut stem = stem.trim().trim_start_matches('.');
    if stem.len() > MAX_STEM {
        let mut end = MAX_STEM;
        while !stem.is_char_boundary(end) {
            end -= 1;
        }
        stem = stem[..end].trim_end();
    }
    if stem.is_empty() {
        "Document".into()
    } else {
        stem.into()
    }
}

/// The file stem the core gives a document titled `title` (`file_stem`), so
/// the apps name the files they keep beside it, such as a source PDF, by
/// the same rule.
#[uniffi::export]
pub fn file_name_stem(title: String) -> String {
    file_stem(&title)
}

/// The stem documents were saved under before `file_stem`: every character
/// a file name cannot hold became `-`.
fn legacy_file_stem(title: &str) -> String {
    let stem: String = title
        .chars()
        .map(|c| {
            if "/\\:*?\"<>|".contains(c) || c.is_control() {
                '-'
            } else {
                c
            }
        })
        .collect();
    let stem = stem.trim().trim_start_matches('.');
    if stem.is_empty() {
        "Document".into()
    } else {
        stem.into()
    }
}

/// The `n`th name for `stem` in `dir`: `stem.md`, then `stem 2.md`, …
fn numbered(dir: &Path, stem: &str, n: u32) -> PathBuf {
    match n {
        1 => dir.join(format!("{stem}.md")),
        n => dir.join(format!("{stem} {n}.md")),
    }
}

/// Whether the case named `name` is named after the document at `path`: the
/// document took its file name from that name (or the document's title is
/// that name), as a document saved into a new case does, under the current
/// or the earlier file naming, numbered or not.
fn named_after(name: &str, path: &Path) -> bool {
    let Some(stem) = path.file_stem().map(|s| s.to_string_lossy()) else {
        return false;
    };
    [file_stem(name), legacy_file_stem(name)]
        .iter()
        .any(|base| {
            stem == base.as_str()
                || stem
                    .strip_prefix(base.as_str())
                    .and_then(|rest| rest.strip_prefix(' '))
                    .and_then(|n| n.parse::<u32>().ok())
                    .is_some_and(|n| n >= 2)
        })
}

/// Whether `a` and `b` name the same existing file (a letter-case change
/// on a file system that ignores letter case).
fn same_file(a: &Path, b: &Path) -> bool {
    #[cfg(unix)]
    {
        use std::os::unix::fs::MetadataExt;
        match (std::fs::metadata(a), std::fs::metadata(b)) {
            (Ok(a), Ok(b)) => a.dev() == b.dev() && a.ino() == b.ino(),
            _ => false,
        }
    }
    #[cfg(not(unix))]
    {
        a.exists() && a.to_string_lossy().to_lowercase() == b.to_string_lossy().to_lowercase()
    }
}

/// A case name as typed, trimmed; a blank one is refused.
fn case_name(name: &str) -> Result<String, CoreError> {
    let name = name.trim();
    if name.is_empty() {
        return Err(CoreError::invalid("A case needs a name."));
    }
    Ok(name.into())
}

/// A document title as typed, trimmed; a blank one is refused.
fn document_title(title: &str) -> Result<String, CoreError> {
    let title = title.trim();
    if title.is_empty() {
        return Err(CoreError::invalid("A document needs a title."));
    }
    Ok(title.into())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::test_support::Scratch;
    use mdoc_pii::{Category, restore::ExportedIdentity};

    const KEY: [u8; 32] = [7; 32];

    fn library(scratch: &Scratch) -> Arc<Library> {
        Library::new(scratch.path().display().to_string(), KEY.to_vec()).unwrap()
    }

    fn person(alias: &str, originals: &[&str]) -> ExportedIdentity {
        ExportedIdentity {
            alias: alias.into(),
            previous: Vec::new(),
            category: Category::Person,
            custom: false,
            originals: originals.iter().map(|o| Arc::from(*o)).collect(),
        }
    }

    #[test]
    fn a_key_of_the_wrong_length_is_refused() {
        let scratch = Scratch::new();
        let error = Library::new(scratch.path().display().to_string(), vec![1; 16]);
        assert!(matches!(error, Err(CoreError::Invalid { .. })));
    }

    #[test]
    fn cases_list_newest_first_and_another_key_cannot_read_them() {
        let scratch = Scratch::new();
        let library = library(&scratch);
        assert!(library.create_case("  ".into()).is_err());
        let ivanov = library.create_case("Ivanov".into()).unwrap();
        assert_eq!(ivanov.name, "Ivanov");
        assert!(ivanov.documents.is_empty());
        assert_eq!(library.case_info(ivanov.id.clone()).unwrap(), ivanov);
        let list = library.cases();
        assert_eq!((list.cases.len(), list.unreadable), (1, 0));
        let stranger =
            Library::new(scratch.path().display().to_string(), [8; 32].to_vec()).unwrap();
        let list = stranger.cases();
        assert_eq!((list.cases.len(), list.unreadable), (0, 1));
        assert!(matches!(
            stranger.case_info(ivanov.id),
            Err(CoreError::Case { .. })
        ));
    }

    #[test]
    fn a_document_joins_one_case_and_lists_with_it() {
        let scratch = Scratch::new();
        let library = library(&scratch);
        let first = library.create_case("First".into()).unwrap();
        let second = library.create_case("Second".into()).unwrap();
        let path = library.new_document_path("Letter: to/Ann");
        assert!(path.ends_with("documents/Letter\u{A789} to\u{2215}Ann.md"));
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        std::fs::write(&path, "PERSON_1 wrote.").unwrap();
        let document = path.display().to_string();
        let joined = library
            .add_document(first.id.clone(), document.clone())
            .unwrap();
        assert_eq!(joined.documents, std::slice::from_ref(&document));
        // Adding it again changes nothing; another case refuses it.
        assert_eq!(
            library
                .add_document(first.id.clone(), document.clone())
                .unwrap(),
            joined
        );
        assert!(matches!(
            library.add_document(second.id, document.clone()),
            Err(CoreError::Case { .. })
        ));
        let documents = library.documents().unwrap();
        assert_eq!(documents.len(), 1);
        assert_eq!(documents[0].title, "Letter: to/Ann");
        assert_eq!(documents[0].case_id, Some(first.id));
        assert_eq!(
            library.new_document_path("Letter: to/Ann"),
            path.with_file_name("Letter\u{A789} to\u{2215}Ann 2.md")
        );
        // Members are stored relative to the root, so a moved root keeps them.
        assert_eq!(
            library.member(&path),
            Path::new("documents").join("Letter\u{A789} to\u{2215}Ann.md")
        );
    }

    /// A title reads back from its file as typed; the file name holds none
    /// of the characters some file systems refuse.
    #[test]
    fn a_title_reads_back_as_typed() {
        let scratch = Scratch::new();
        let library = library(&scratch);
        for title in [
            "Sample: services agreement",
            r#"A/B\C:D*E?F"G<H>I|J"#,
            "Договор № 5: аренда",
        ] {
            let path = library.new_document_path(title);
            let name = path.file_name().unwrap().to_string_lossy().into_owned();
            assert!(!name.contains(['/', '\\', ':', '*', '?', '"', '<', '>', '|']));
            assert_eq!(title_of(&path), title);
        }
        assert_eq!(title_of(&library.new_document_path(" .. ")), "Document");
        assert_eq!(
            title_of(&library.new_document_path("Tab\there")),
            "Tab-here"
        );
        // A title too long for a file name is shortened at a character.
        let long = "Ж".repeat(200);
        let path = library.new_document_path(&long);
        assert!(path.file_name().unwrap().len() <= 255);
        assert!(long.starts_with(&title_of(&path)));
        // A case is named after a document under the earlier naming too.
        let legacy = library
            .documents_dir()
            .join("Sample- services agreement 2.md");
        assert!(named_after("Sample: services agreement", &legacy));
        assert!(named_after(
            "Sample: services agreement",
            &path.with_file_name("Sample\u{A789} services agreement.md")
        ));
        assert!(!named_after("Sample", &legacy));
    }

    /// The exported rule is the one the library names documents by.
    #[test]
    fn the_exported_file_name_stem_is_the_librarys() {
        let scratch = Scratch::new();
        let library = library(&scratch);
        let long = "ж".repeat(200);
        for title in [
            "Claim: Bennett",
            ".hidden",
            "Tab\there",
            "  ",
            long.as_str(),
        ] {
            let path = library.new_document_path(title);
            let stem = path.file_stem().unwrap().to_string_lossy().into_owned();
            assert_eq!(file_name_stem(title.into()), stem);
        }
        assert_eq!(file_name_stem(".hidden".into()), "hidden");
        assert_eq!(file_name_stem(long).len(), 240);
    }

    /// Rename takes everything a case keeps by the file along: membership
    /// and the reply; a case with other documents keeps its name.
    #[test]
    fn a_renamed_document_keeps_its_case_and_reply() {
        let scratch = Scratch::new();
        let library = library(&scratch);
        let case = library.create_case("Ivanov".into()).unwrap();
        let letter = member_with_reply(&library, &case, "Letter");
        let memo = member_with_reply(&library, &case, "Memo");
        let renamed = library
            .rename_document(letter.clone(), "  Letter: final ".into())
            .unwrap();
        assert_eq!(renamed.title, "Letter: final");
        let path = renamed.path.clone().unwrap();
        assert!(!Path::new(&letter).exists());
        assert_eq!(
            std::fs::read_to_string(&path).unwrap(),
            "Letter: PERSON_1 wrote."
        );
        let info = renamed.case.unwrap();
        assert_eq!(info.name, "Ivanov");
        assert_eq!(info.documents, [path.clone(), memo.clone()]);
        assert_eq!(
            library.reply(case.id.clone(), path.clone()).unwrap(),
            Some("Thanks, PERSON_1.".into())
        );
        assert_eq!(library.reply(case.id.clone(), letter).unwrap(), None);
        let titles: Vec<_> = library
            .documents()
            .unwrap()
            .into_iter()
            .map(|d| (d.title, d.case_id))
            .collect();
        assert!(titles.contains(&("Letter: final".into(), Some(case.id.clone()))));
        // Another document's title is numbered; its own title changes nothing.
        let numbered = library
            .rename_document(memo, "Letter: final".into())
            .unwrap();
        assert_eq!(numbered.title, "Letter: final 2");
        let same = library
            .rename_document(path.clone(), "Letter: final".into())
            .unwrap();
        assert_eq!(same.path, Some(path.clone()));
        // A change of letter case alone renames the one file.
        let lower = library
            .rename_document(path.clone(), "letter: final".into())
            .unwrap();
        assert_eq!(lower.title, "letter: final");
        assert_eq!(library.documents().unwrap().len(), 2);
        let path = lower.path.unwrap();
        // A blank title, or a file outside the documents folder, is refused.
        assert!(matches!(
            library.rename_document(path, " ".into()),
            Err(CoreError::Invalid { .. })
        ));
        let outside = scratch.path().join("outside.md");
        std::fs::write(&outside, "Keep me.").unwrap();
        assert!(matches!(
            library.rename_document(outside.display().to_string(), "In".into()),
            Err(CoreError::Invalid { .. })
        ));
    }

    /// The case of a document saved into a new case carries its name: it
    /// follows the document's renames while the document is its only one.
    #[test]
    fn a_case_named_after_its_only_document_is_renamed_with_it() {
        let scratch = Scratch::new();
        let library = library(&scratch);
        let case = library
            .create_case("Sample: services agreement".into())
            .unwrap();
        let sample = member_with_reply(&library, &case, "Sample: services agreement");
        let renamed = library
            .rename_document(sample, "Services: final".into())
            .unwrap();
        assert_eq!(renamed.case.unwrap().name, "Services: final");
        // Saved under the earlier naming ("Sample- services agreement").
        let legacy = library.create_case("Lease: draft".into()).unwrap();
        let path = library.documents_dir().join("Lease- draft.md");
        std::fs::write(&path, "Text.").unwrap();
        let path = path.display().to_string();
        library
            .add_document(legacy.id.clone(), path.clone())
            .unwrap();
        let renamed = library.rename_document(path, "Lease".into()).unwrap();
        assert_eq!(renamed.case.unwrap().name, "Lease");
        // A case named otherwise, or with other documents, keeps its name.
        let other = library.create_case("Ivanov".into()).unwrap();
        let alone = member_with_reply(&library, &other, "Alone");
        let renamed = library.rename_document(alone, "Elsewhere".into()).unwrap();
        assert_eq!(renamed.case.unwrap().name, "Ivanov");
        let shared = library.create_case("Pair".into()).unwrap();
        let first = member_with_reply(&library, &shared, "Pair");
        member_with_reply(&library, &shared, "Second");
        let renamed = library.rename_document(first, "Single".into()).unwrap();
        assert_eq!(renamed.case.unwrap().name, "Pair");
        // A case can be renamed on its own; a blank name is refused.
        assert_eq!(
            library
                .rename_case(shared.id.clone(), " Pair: two ".into())
                .unwrap()
                .name,
            "Pair: two"
        );
        assert!(matches!(
            library.rename_case(shared.id, "".into()),
            Err(CoreError::Invalid { .. })
        ));
        assert!(library.rename_case("0".repeat(32), "Gone".into()).is_err());
    }

    /// When a case cannot be saved, the file goes back to its name and the
    /// case still has it there.
    #[test]
    fn a_rename_a_case_refuses_leaves_the_document_as_it_was() {
        let scratch = Scratch::new();
        let library = library(&scratch);
        let case = library.create_case("Case".into()).unwrap();
        let letter = member_with_reply(&library, &case, "Letter");
        let held = Held::file(&case_file(&library, &case.id), true);
        let result = library.rename_document(letter.clone(), "Memo".into());
        drop(held);
        assert!(result.is_err(), "{result:?}");
        assert!(Path::new(&letter).exists());
        assert!(!library.documents_dir().join("Memo.md").exists());
        assert_eq!(
            library.case_info(case.id.clone()).unwrap().documents,
            std::slice::from_ref(&letter)
        );
        assert_eq!(
            library.reply(case.id, letter).unwrap(),
            Some("Thanks, PERSON_1.".into())
        );
    }

    #[test]
    fn the_current_case_follows_the_last_share_while_it_exists() {
        let scratch = Scratch::new();
        let library = library(&scratch);
        assert_eq!(library.current_case(), None);
        let first = library.create_case("First".into()).unwrap();
        let second = library.create_case("Second".into()).unwrap();
        library.note_shared(first.id.clone()).unwrap();
        assert_eq!(library.current_case(), Some(first.clone()));
        library.note_shared(second.id.clone()).unwrap();
        assert_eq!(
            library.current_case().map(|c| c.id),
            Some(second.id.clone())
        );
        assert!(library.note_shared("0".repeat(32)).is_err());
        assert_eq!(
            library.current_case().map(|c| c.id),
            Some(second.id.clone())
        );
        // Another process (the extension) sees the same current case.
        let extension = self::library(&scratch);
        assert_eq!(
            extension.current_case().map(|c| c.id),
            Some(second.id.clone())
        );
        library.store.delete(&second.id).unwrap();
        assert_eq!(library.current_case(), None);
    }

    #[test]
    fn a_reply_restores_with_its_own_case_only() {
        let scratch = Scratch::new();
        let library = library(&scratch);
        let ivanov = library
            .create(Case::new(
                "Ivanov".into(),
                &[person("PERSON_1", &["Ivan Ivanov"])],
                None,
            ))
            .unwrap();
        let petrov = library
            .create(Case::new(
                "Petrov".into(),
                &[person("PERSON_1", &["Petr Petrov"])],
                None,
            ))
            .unwrap();
        let reply = "PERSON_1 agrees; PERSON_2 does not.";
        let restored = library.restore(ivanov.id.clone(), reply.into()).unwrap();
        assert_eq!(restored.text, "Ivan Ivanov agrees; PERSON_2 does not.");
        assert_eq!(restored.restored, [TextRange { start: 0, end: 11 }]);
        assert_eq!(restored.unresolved, [TextRange { start: 20, end: 28 }]);
        assert_eq!((restored.values, restored.unrecognised), (1, 1));
        assert!(restored.restorable);
        // Unrecognised counts distinct tokens; every occurrence is listed.
        let twice = library
            .restore(ivanov.id.clone(), "PERSON_2, PERSON_2 and ORG_4.".into())
            .unwrap();
        assert_eq!((twice.unresolved.len(), twice.unrecognised), (3, 2));
        let restored = library.restore(petrov.id, reply.into()).unwrap();
        assert_eq!(restored.text, "Petr Petrov agrees; PERSON_2 does not.");
        let empty = library.create_case("Empty".into()).unwrap();
        assert!(!library.restore(empty.id, reply.into()).unwrap().restorable);
    }

    /// The app and its extension are two libraries on one folder: their
    /// overlapping writes all land.
    #[test]
    fn writes_from_two_libraries_never_lose_each_other() {
        let scratch = Scratch::new();
        let case = library(&scratch).create_case("Shared".into()).unwrap();
        let writers: Vec<_> = (0..2)
            .map(|writer| {
                let library = library(&scratch);
                let id = case.id.clone();
                std::thread::spawn(move || {
                    for n in 0..15 {
                        library
                            .edit(&id, |case| {
                                case.members.push(format!("{writer}-{n}.md").into());
                                true
                            })
                            .unwrap();
                    }
                })
            })
            .collect();
        for writer in writers {
            writer.join().unwrap();
        }
        assert_eq!(
            library(&scratch)
                .case_info(case.id)
                .unwrap()
                .documents
                .len(),
            30
        );
    }

    /// A document in `case` with a kept reply, as `save` leaves one.
    fn member_with_reply(library: &Library, case: &CaseInfo, title: &str) -> String {
        let path = library.reserve_document_path(title).unwrap();
        std::fs::write(&path, format!("{title}: PERSON_1 wrote.")).unwrap();
        let document = path.display().to_string();
        library
            .add_document(case.id.clone(), document.clone())
            .unwrap();
        library
            .save_reply(
                case.id.clone(),
                document.clone(),
                "Thanks, PERSON_1.".into(),
            )
            .unwrap();
        document
    }

    /// The document leaves its case with its reply and its file goes; the
    /// case and its other member stay, and a case left empty stays too.
    #[test]
    fn a_deleted_document_leaves_its_case_with_its_reply() {
        let scratch = Scratch::new();
        let library = library(&scratch);
        let case = library
            .create(Case::new(
                "Ivanov".into(),
                &[person("PERSON_1", &["Ivan Ivanov"])],
                None,
            ))
            .unwrap();
        let letter = member_with_reply(&library, &case, "Letter");
        let memo = member_with_reply(&library, &case, "Memo");
        let left = library.delete_document(letter.clone()).unwrap().unwrap();
        assert_eq!(left.id, case.id);
        assert_eq!(left.documents, std::slice::from_ref(&memo));
        assert!(!Path::new(&letter).exists());
        let stored = library.load(&case.id).unwrap();
        assert_eq!(stored.reply(&library.member(Path::new(&letter))), None);
        assert_eq!(
            library.reply(case.id.clone(), memo.clone()).unwrap(),
            Some("Thanks, PERSON_1.".into())
        );
        assert_eq!(library.documents().unwrap().len(), 1);
        // The case still restores, and stays when its last member goes.
        library.delete_document(memo.clone()).unwrap();
        let empty = library.case_info(case.id.clone()).unwrap();
        assert!(empty.documents.is_empty());
        assert_eq!(empty.names, 1);
        assert_eq!(
            library.restore(case.id, "PERSON_1".into()).unwrap().text,
            "Ivan Ivanov"
        );
        assert!(library.documents().unwrap().is_empty());
        assert!(
            std::fs::read_dir(library.documents_dir())
                .unwrap()
                .next()
                .is_none()
        );
    }

    #[test]
    fn a_document_in_no_case_or_already_gone_is_deleted_quietly() {
        let scratch = Scratch::new();
        let library = library(&scratch);
        let path = library.reserve_document_path("Loose").unwrap();
        assert_eq!(
            library.delete_document(path.display().to_string()).unwrap(),
            None
        );
        assert!(!path.exists());
        // Removed elsewhere (Files): it still leaves its case.
        let case = library.create_case("Case".into()).unwrap();
        let gone = member_with_reply(&library, &case, "Gone");
        std::fs::remove_file(&gone).unwrap();
        let left = library.delete_document(gone).unwrap().unwrap();
        assert!(left.documents.is_empty());
    }

    #[test]
    fn only_the_documents_folder_is_deleted_from() {
        let scratch = Scratch::new();
        let library = library(&scratch);
        let outside = scratch.path().join("outside.md");
        std::fs::write(&outside, "Keep me.").unwrap();
        let elsewhere = library.documents_dir().join("notes.pdf");
        std::fs::create_dir_all(library.documents_dir()).unwrap();
        std::fs::write(&elsewhere, "%PDF-").unwrap();
        for path in [&outside, &elsewhere, &library.documents_dir()] {
            assert!(matches!(
                library.delete_document(path.display().to_string()),
                Err(CoreError::Invalid { .. })
            ));
        }
        assert!(outside.exists() && elsewhere.exists());
    }

    /// Open `path` so that it still reads (unless `readable` is false) but
    /// cannot be replaced or removed, until the guard drops.
    struct Held {
        #[cfg(windows)]
        _file: std::fs::File,
        #[cfg(unix)]
        restore: (PathBuf, u32),
    }

    impl Held {
        fn file(path: &Path, readable: bool) -> Self {
            #[cfg(windows)]
            {
                use std::os::windows::fs::OpenOptionsExt;
                const FILE_SHARE_READ: u32 = 1;
                Self {
                    _file: std::fs::OpenOptions::new()
                        .read(true)
                        .share_mode(if readable { FILE_SHARE_READ } else { 0 })
                        .open(path)
                        .unwrap(),
                }
            }
            #[cfg(unix)]
            {
                // Unreadable: the file itself; unremovable: its folder.
                let target = if readable {
                    path.parent().unwrap().to_path_buf()
                } else {
                    path.to_path_buf()
                };
                Self::mode(&target, if readable { 0o555 } else { 0o000 }, 0o755)
            }
        }

        #[cfg(unix)]
        fn mode(path: &Path, mode: u32, back: u32) -> Self {
            use std::os::unix::fs::PermissionsExt;
            std::fs::set_permissions(path, std::fs::Permissions::from_mode(mode)).unwrap();
            Self {
                restore: (path.to_path_buf(), back),
            }
        }
    }

    #[cfg(unix)]
    impl Drop for Held {
        fn drop(&mut self) {
            use std::os::unix::fs::PermissionsExt;
            let (path, mode) = &self.restore;
            let _ = std::fs::set_permissions(path, std::fs::Permissions::from_mode(*mode));
        }
    }

    fn case_file(library: &Library, id: &str) -> PathBuf {
        std::fs::read_dir(library.cases_folder())
            .unwrap()
            .map(|entry| entry.unwrap().path())
            .find(|path| path.file_stem().is_some_and(|n| n.to_string_lossy() == id))
            .unwrap()
    }

    /// A case that cannot be read for now refuses the delete, and nothing
    /// changes; a case sealed with another key never blocks it.
    #[test]
    fn a_case_unreadable_for_now_refuses_the_delete() {
        let scratch = Scratch::new();
        let library = library(&scratch);
        let case = library.create_case("Case".into()).unwrap();
        let letter = member_with_reply(&library, &case, "Letter");
        let held = Held::file(&case_file(&library, &case.id), false);
        assert!(matches!(
            library.delete_document(letter.clone()),
            Err(CoreError::Case { .. })
        ));
        drop(held);
        assert!(Path::new(&letter).exists());
        assert_eq!(
            library.case_info(case.id.clone()).unwrap().documents,
            std::slice::from_ref(&letter)
        );
        let stranger =
            Library::new(scratch.path().display().to_string(), [8; 32].to_vec()).unwrap();
        stranger.create_case("Elsewhere".into()).unwrap();
        assert_eq!(library.cases().unreadable, 1);
        assert_eq!(
            library.delete_document(letter.clone()).unwrap().unwrap().id,
            case.id
        );
        assert!(!Path::new(&letter).exists());
    }

    /// A file that cannot be removed after its case was saved stays an
    /// ordinary document in no case, and deleting it again removes it.
    #[test]
    fn a_file_that_cannot_be_removed_stays_a_document_in_no_case() {
        let scratch = Scratch::new();
        let library = library(&scratch);
        let case = library.create_case("Case".into()).unwrap();
        let letter = member_with_reply(&library, &case, "Letter");
        let held = Held::file(Path::new(&letter), true);
        assert!(matches!(
            library.delete_document(letter.clone()),
            Err(CoreError::Io { .. })
        ));
        drop(held);
        let documents = library.documents().unwrap();
        assert_eq!(documents.len(), 1);
        assert_eq!(documents[0].case_id, None);
        assert!(library.case_info(case.id).unwrap().documents.is_empty());
        assert_eq!(library.delete_document(letter.clone()).unwrap(), None);
        assert!(library.documents().unwrap().is_empty());
    }

    /// When its case cannot be saved, the document stays a member and its
    /// file stays where it was.
    #[test]
    fn a_case_that_cannot_be_saved_keeps_the_document() {
        let scratch = Scratch::new();
        let library = library(&scratch);
        let case = library.create_case("Case".into()).unwrap();
        let letter = member_with_reply(&library, &case, "Letter");
        // The case still reads, but cannot be replaced.
        let held = Held::file(&case_file(&library, &case.id), true);
        let result = library.delete_document(letter.clone());
        drop(held);
        assert!(result.is_err(), "{result:?}");
        assert_eq!(
            std::fs::read_to_string(&letter).unwrap(),
            "Letter: PERSON_1 wrote."
        );
        assert_eq!(
            library.case_info(case.id.clone()).unwrap().documents,
            std::slice::from_ref(&letter)
        );
        assert_eq!(
            library.reply(case.id, letter).unwrap(),
            Some("Thanks, PERSON_1.".into())
        );
    }

    #[test]
    fn reserved_paths_are_never_shared() {
        let scratch = Scratch::new();
        let library = library(&scratch);
        let first = library.reserve_document_path("Document").unwrap();
        let second = library.reserve_document_path("Document").unwrap();
        assert_ne!(first, second);
        assert!(first.exists() && second.exists());
    }

    #[test]
    fn a_reply_is_kept_with_its_member_document() {
        let scratch = Scratch::new();
        let library = library(&scratch);
        let case = library.create_case("Case".into()).unwrap();
        let document = library.new_document_path("Letter").display().to_string();
        assert!(matches!(
            library.save_reply(case.id.clone(), document.clone(), "PERSON_1".into()),
            Err(CoreError::Case { .. })
        ));
        library
            .add_document(case.id.clone(), document.clone())
            .unwrap();
        library
            .save_reply(
                case.id.clone(),
                document.clone(),
                "Thanks, PERSON_1.".into(),
            )
            .unwrap();
        assert_eq!(
            library.reply(case.id.clone(), document.clone()).unwrap(),
            Some("Thanks, PERSON_1.".into())
        );
        library
            .save_reply(case.id.clone(), document.clone(), " \n".into())
            .unwrap();
        assert_eq!(library.reply(case.id, document).unwrap(), None);
    }
}
