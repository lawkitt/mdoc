//! `Library` (ADR 0085): the cases and stored documents under one storage
//! root, the App Group container on iOS, shared by the app and its Restore
//! extension. Cases are the desktop's encrypted files
//! (`mdoc_store::cases::Store`) under the key the platform passes in; Rust
//! never touches a keychain here.
use crate::CoreError;
use mdoc_pii::restore::RestoreMap;
use mdoc_store::cases::{Case, CaseLink, Store};
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
        let entries = match std::fs::read_dir(self.documents_dir()) {
            Ok(entries) => entries,
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(Vec::new()),
            Err(error) => return Err(error.into()),
        };
        let mut documents = Vec::new();
        for entry in entries {
            let path = entry?.path();
            if !mdoc_store::document::is_markdown(&path) || !path.is_file() {
                continue;
            }
            let member = self.member(&path);
            let modified = std::fs::metadata(&path)?
                .modified()
                .ok()
                .and_then(|time| time.duration_since(std::time::UNIX_EPOCH).ok())
                .map_or(0, |since| since.as_secs());
            documents.push(StoredDocument {
                title: title(&path),
                case_id: cases
                    .iter()
                    .find(|case| case.members.contains(&member))
                    .map(|case| case.id.clone()),
                path: path.display().to_string(),
                modified,
            });
        }
        documents.sort_by(|a, b| b.modified.cmp(&a.modified).then(a.title.cmp(&b.title)));
        Ok(documents)
    }

    /// A new, empty case. Documents usually start their own case when they
    /// are saved (`DocumentSession::save`).
    pub fn create_case(&self, name: String) -> Result<CaseInfo, CoreError> {
        let name = name.trim();
        if name.is_empty() {
            return Err(CoreError::invalid("A case needs a name."));
        }
        self.create(Case::new(name.into(), &[], None))
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
        let json = std::fs::read_to_string(self.root.join(CURRENT_CASE)).ok()?;
        let value: serde_json::Value = serde_json::from_str(&json).ok()?;
        self.case_info(value.get("case")?.as_str()?.to_owned()).ok()
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
        let path = std::path::absolute(&path).unwrap_or_else(|_| PathBuf::from(&path));
        let documents =
            std::path::absolute(self.documents_dir()).unwrap_or_else(|_| self.documents_dir());
        if path.parent() != Some(documents.as_path()) || !mdoc_store::document::is_markdown(&path) {
            return Err(CoreError::invalid(
                "Only the library's documents can be deleted.",
            ));
        }
        let member = self.member(&path);
        let lock = self.lock()?;
        let (cases, unreadable) = self.store.list();
        for id in unreadable {
            if let Err(mdoc_store::cases::CaseError::Read(error)) = self.store.load(&id)
                && error.kind() != std::io::ErrorKind::NotFound
            {
                return Err(CoreError::case(format!(
                    "A case could not be read, so the document was not deleted: {error}"
                )));
            }
        }
        let mut left = None;
        for case in cases {
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
            _ => Ok(left),
        }
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
        let stem = if stem.is_empty() { "Document" } else { stem };
        let dir = self.documents_dir();
        (1..)
            .map(|n| match n {
                1 => dir.join(format!("{stem}.md")),
                n => dir.join(format!("{stem} {n}.md")),
            })
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

fn title(path: &Path) -> String {
    path.file_stem()
        .map_or_else(String::new, |stem| stem.to_string_lossy().into_owned())
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
        assert!(path.ends_with("documents/Letter- to-Ann.md"));
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
        assert_eq!(documents[0].title, "Letter- to-Ann");
        assert_eq!(documents[0].case_id, Some(first.id));
        assert_eq!(
            library.new_document_path("Letter: to/Ann"),
            path.with_file_name("Letter- to-Ann 2.md")
        );
        // Members are stored relative to the root, so a moved root keeps them.
        assert_eq!(
            library.member(&path),
            Path::new("documents").join("Letter- to-Ann.md")
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
