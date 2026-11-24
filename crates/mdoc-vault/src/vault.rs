//! A handle on one Vault folder. Writes are one load-change-save at a
//! time: in the process through a mutex and across processes and handles
//! through `.lock`, held only for the write (#1091 Decision 20). A handle
//! never writes a revision older than the last it wrote.
mod export;
mod moves;
mod readiness;
mod replies;
mod staging;

pub use export::{Exported, ORIGINALS};
pub use moves::{MoveUndo, Moved, ProjectDocuments};
pub use readiness::Readiness;
pub use replies::ReplyHandle;
pub use staging::StagedImport;

use crate::{
    ObjectKind, VaultError, is_id,
    manifest::{
        Entry, LoadError, MANIFEST_ID, Manifest, Record, Stamp, document_dir, document_path, load,
        original_path, write_dirs,
    },
    mapping::Mapping,
    new_id,
    original::{self, IMPORT_PREFIX, OriginalReader},
    records::{
        Document, DocumentSummary, NewDocument, NewProject, Original, OriginalInfo, Project,
        ProjectSummary, ReplyOwner, UserLists, now,
    },
    seal::{MAX_FILE, Sealer, TEMP_PREFIX, Unseal, read_limited, temp_beside},
};
use mdoc_store::lists::Lists;
use std::{
    collections::HashMap,
    fs::File,
    io::{self, Read, Write},
    path::{Component, Path, PathBuf},
    sync::{
        Arc, Mutex, MutexGuard,
        atomic::{AtomicBool, Ordering},
    },
    time::{Duration, Instant, SystemTime},
};

/// The user-wide lists' id in their authenticated data.
const USER_LISTS_ID: &str = "user-lists";

/// A summary change rewrites the manifest at most this often; the rest wait
/// for the next write after it, [`Vault::flush`] or the handle's drop.
const MANIFEST_DEBOUNCE: Duration = Duration::from_secs(2);
/// A document folder without a record, untouched this long, is what an
/// import that died before its record left.
const ORPHAN_AGE: Duration = Duration::from_secs(24 * 60 * 60);
/// What this handle last wrote of an object.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Last {
    Revision(u64),
    Deleted,
}

/// Where in replacing a file a [`FaultHook`] runs.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Stage {
    /// The sealed temp file is written and synced; the target is untouched.
    BeforeRename,
    /// The target was replaced.
    AfterRename,
}

/// One point in a write, as a [`FaultHook`] sees it.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct WriteStep<'a> {
    pub kind: ObjectKind,
    /// The object's id; `"manifest"` for the manifest.
    pub id: &'a str,
    pub stage: Stage,
}

/// What a [`FaultHook`] makes of a write step.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Fault {
    Continue,
    /// Stop as a crash would: the write goes no further, a temp file it made
    /// stays, and the handle writes nothing more, not even on drop.
    Crash,
}

/// Tests only: runs at every [`WriteStep`] to stand in for a crash between
/// two writes.
pub type FaultHook = Arc<dyn Fn(&WriteStep<'_>) -> Fault + Send + Sync>;

/// How the manifest was found when the handle opened.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ManifestCheck {
    /// It matched every object file.
    Current,
    /// There was none; it was rebuilt from the object files.
    Missing,
    /// It did not open; it was rebuilt from the object files.
    Damaged,
    /// Some object files differed from it; those were read again.
    Stale,
}

/// What a save did.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Saved {
    Written,
    /// This handle already wrote a newer revision, or deleted the object.
    Skipped,
}

pub struct Vault {
    root: PathBuf,
    sealer: Sealer,
    lock_file: Mutex<File>,
    state: Mutex<State>,
    hook: Option<FaultHook>,
    crashed: AtomicBool,
    opened: ManifestCheck,
    /// `false` for a [`ReplyHandle`]: the manifest is checked and kept in
    /// memory but never written, not at open, on flush or on drop.
    writes_manifest: bool,
}

#[derive(Default)]
struct State {
    manifest: Manifest,
    /// The last revision this handle wrote, by object id.
    written: HashMap<String, Last>,
    /// A summary changed since the manifest was last written.
    dirty: bool,
    manifest_written: Option<Instant>,
}

/// `.lock`, held: in this process through the mutex, across handles and
/// processes through the OS lock.
struct WriteLock<'a> {
    file: MutexGuard<'a, File>,
}

impl Drop for WriteLock<'_> {
    fn drop(&mut self) {
        let _ = self.file.unlock();
    }
}

impl Vault {
    /// Open the Vault in `root`, creating the folder if needed. The manifest
    /// is checked against the object files and rebuilt where it differs.
    /// [`VaultError::WrongKey`] when the folder holds objects and none opens
    /// with `key`; nothing is written then.
    pub fn open(root: impl Into<PathBuf>, key: &[u8; 32]) -> Result<Self, VaultError> {
        Self::open_inner(root.into(), key, None, true)
    }

    /// [`Self::open`] with a hook run at every write step (tests).
    pub fn open_with_hook(
        root: impl Into<PathBuf>,
        key: &[u8; 32],
        hook: FaultHook,
    ) -> Result<Self, VaultError> {
        Self::open_inner(root.into(), key, Some(hook), true)
    }

    /// `writes_manifest: false` opens for a [`ReplyHandle`]: an existing
    /// folder only, no temp files swept and no manifest written.
    pub(super) fn open_inner(
        root: PathBuf,
        key: &[u8; 32],
        hook: Option<FaultHook>,
        writes_manifest: bool,
    ) -> Result<Self, VaultError> {
        if writes_manifest {
            std::fs::create_dir_all(&root)?;
        }
        let lock_file = File::options()
            .read(true)
            .write(true)
            .create(true)
            .truncate(false)
            .open(root.join(".lock"))?;
        let mut vault = Self {
            root,
            sealer: Sealer::new(key),
            lock_file: Mutex::new(lock_file),
            state: Mutex::default(),
            hook,
            crashed: AtomicBool::new(false),
            opened: ManifestCheck::Current,
            writes_manifest,
        };
        vault.opened = {
            let _lock = vault.write_lock()?;
            // Every write holds the lock, so a temp file now is one a dead
            // write left.
            // A ReplyHandle removes nothing: the app sweeps.
            if writes_manifest {
                sweep_temp_files(&vault.root);
                sweep_orphans(&vault.root);
                staging::sweep_staging(&vault.root);
            }
            let (mut manifest, mut opened) =
                match Manifest::read(&vault.sealer, &vault.manifest_path()) {
                    Ok(Some(manifest)) => (manifest, ManifestCheck::Current),
                    Ok(None) => (Manifest::default(), ManifestCheck::Missing),
                    Err(()) => (Manifest::default(), ManifestCheck::Damaged),
                };
            let checked = manifest.check(&vault.sealer, &vault.root)?;
            // The user-wide lists are an object too, kept outside the manifest.
            let lists = vault.user_lists_open();
            let (any_opened, any_rejected) = (
                checked.opened > 0 || lists == Some(true),
                checked.rejected > 0 || lists == Some(false),
            );
            if opened != ManifestCheck::Current && !any_opened && any_rejected {
                return Err(VaultError::WrongKey);
            }
            if opened == ManifestCheck::Current && checked.changed {
                opened = ManifestCheck::Stale;
            }
            let mut state = vault.state();
            state.manifest = manifest;
            if opened != ManifestCheck::Current {
                vault.write_manifest(&mut state)?;
            }
            opened
        };
        Ok(vault)
    }

    /// Whether `key` is the key this handle seals with: Settings shows a
    /// recovery key only when it opens these documents. The key itself is
    /// never kept.
    pub fn opens_with(&self, key: &[u8; 32]) -> bool {
        const PROBE: &str = "recovery-key-check";
        self.sealer
            .seal(ObjectKind::Manifest, PROBE, PROBE.as_bytes())
            .is_ok_and(|sealed| {
                Sealer::new(key)
                    .open(ObjectKind::Manifest, PROBE, &sealed)
                    .is_ok()
            })
    }

    pub fn root(&self) -> &Path {
        &self.root
    }

    /// How the manifest was found at open.
    pub fn opened(&self) -> ManifestCheck {
        self.opened
    }

    /// Every Project, pinned first, then most recently updated.
    pub fn projects(&self) -> Vec<ProjectSummary> {
        let mut projects: Vec<_> = (self.state().manifest.projects.ok.values())
            .map(|entry| entry.summary.clone())
            .collect();
        projects.sort_by(|a, b| {
            (b.pinned, b.updated)
                .cmp(&(a.pinned, a.updated))
                .then_with(|| a.name.cmp(&b.name))
        });
        projects
    }

    /// Every Document, most recently updated first.
    pub fn documents(&self) -> Vec<DocumentSummary> {
        let mut documents: Vec<_> = (self.state().manifest.documents.ok.values())
            .map(|entry| entry.summary.clone())
            .collect();
        documents.sort_by(|a, b| {
            b.updated
                .cmp(&a.updated)
                .then_with(|| a.title.cmp(&b.title))
        });
        documents
    }

    /// The Documents of `project`, or Unfiled ones for `None`, most
    /// recently updated first.
    pub fn documents_in(&self, project: Option<&str>) -> Vec<DocumentSummary> {
        let mut documents = self.documents();
        documents.retain(|document| document.project.as_deref() == project);
        documents
    }

    /// Object files that did not open: damaged, or sealed for another id,
    /// kind or key.
    pub fn damaged(&self) -> Vec<(ObjectKind, String)> {
        let state = self.state();
        let projects =
            (state.manifest.projects.damaged.keys()).map(|id| (ObjectKind::Project, id.clone()));
        let documents =
            (state.manifest.documents.damaged.keys()).map(|id| (ObjectKind::Document, id.clone()));
        let replies =
            (state.manifest.replies.damaged.keys()).map(|id| (ObjectKind::Reply, id.clone()));
        projects.chain(documents).chain(replies).collect()
    }

    /// Pick up what other handles wrote: check the summaries against the
    /// object files again. `true` when anything changed.
    pub fn refresh(&self) -> Result<bool, VaultError> {
        let mut state = self.state();
        let changed = state.manifest.check(&self.sealer, &self.root)?.changed;
        // A handle that never writes the manifest never marks it either, so
        // its drop takes no lock.
        state.dirty |= changed && self.writes_manifest;
        Ok(changed)
    }

    /// Write the manifest now if a summary changed since it was written.
    pub fn flush(&self) -> Result<(), VaultError> {
        if !self.writes_manifest {
            return Ok(());
        }
        let _lock = self.write_lock()?;
        let mut state = self.state();
        if state.dirty {
            self.write_manifest(&mut state)?;
        }
        Ok(())
    }

    pub fn create_project(&self, new: NewProject) -> Result<Project, VaultError> {
        let lock = self.write_lock()?;
        self.create_project_locked(&lock, new, |_| {})
    }

    /// [`Self::create_project`], set up by `setup` (its categories, say)
    /// before its first write: one write, so nothing can fail after it.
    pub fn create_project_with(
        &self,
        new: NewProject,
        setup: impl FnOnce(&mut Project),
    ) -> Result<Project, VaultError> {
        let lock = self.write_lock()?;
        self.create_project_locked(&lock, new, setup)
    }

    /// Create a Project from `new`, set up by `setup` before its first
    /// write.
    fn create_project_locked(
        &self,
        lock: &WriteLock<'_>,
        new: NewProject,
        setup: impl FnOnce(&mut Project),
    ) -> Result<Project, VaultError> {
        let now = now();
        let mut project = Project {
            id: new_id()?,
            name: new.name,
            colour: new.colour,
            pinned: new.pinned,
            created: now,
            updated: now,
            revision: 1,
            mapping: Mapping::default(),
            lists: Lists::default(),
            skip: Vec::new(),
            dismissed_hints: Vec::new(),
        };
        setup(&mut project);
        self.write_record(lock, &project)?;
        Ok(project)
    }

    /// The Project `id`, read from its file.
    pub fn project(&self, id: &str) -> Result<Project, VaultError> {
        self.load_record(id)
    }

    /// Load `id`, change it with `change`, and save it as the next revision,
    /// all under the write lock.
    pub fn update_project(
        &self,
        id: &str,
        change: impl FnOnce(&mut Project),
    ) -> Result<Project, VaultError> {
        self.update(id, change)
    }

    /// Save `project` unless this handle wrote, or its file holds, a newer
    /// revision.
    pub fn save_project(&self, project: &Project) -> Result<Saved, VaultError> {
        self.save(project)
    }

    /// A new Document, Unfiled or in an existing Project.
    pub fn create_document(&self, new: NewDocument) -> Result<Document, VaultError> {
        let lock = self.write_lock()?;
        if let Some(project) = &new.project {
            self.load_record::<Project>(project)?;
        }
        let document = new_document(new_id()?, new, None);
        self.write_record(&lock, &document)?;
        Ok(document)
    }

    /// A new Document with its Source Original, Unfiled or in an existing
    /// Project. `source` is sealed into `docs/<id>/original.enc` one chunk
    /// at a time, in bounded memory and outside the write lock; the lock is
    /// taken for the rename and the record only. Until the record is
    /// written there is no Document: a failure removes the folder, and one
    /// a crash leaves is removed by an open a day later.
    pub fn import_document(
        &self,
        new: NewDocument,
        source: impl Read,
        info: OriginalInfo,
    ) -> Result<Document, VaultError> {
        match self.import_of(new, source, info, None)? {
            Some(document) => Ok(document),
            None => unreachable!("only an expected hash refuses the source"),
        }
    }

    /// [`Self::import_document`], only while `source` hashes to `sha256`,
    /// the bytes the caller checked before: `Ok(None)` when they differ,
    /// with nothing written. The hash is that of the bytes sealed, so the
    /// Document holds exactly what was checked.
    pub fn import_document_expecting(
        &self,
        new: NewDocument,
        source: impl Read,
        info: OriginalInfo,
        sha256: &str,
    ) -> Result<Option<Document>, VaultError> {
        self.import_of(new, source, info, Some(sha256))
    }

    fn import_of(
        &self,
        new: NewDocument,
        source: impl Read,
        info: OriginalInfo,
        expect: Option<&str>,
    ) -> Result<Option<Document>, VaultError> {
        if self.crashed.load(Ordering::SeqCst) {
            return Err(VaultError::Crashed);
        }
        let id = new_id()?;
        let dir = document_dir(&self.root, &id);
        std::fs::create_dir_all(&dir)?;
        let imported = self.import_into(&id, &dir, new, source, info, expect);
        if !matches!(imported, Ok(Some(_)) | Err(VaultError::Crashed)) {
            let _ = std::fs::remove_dir_all(&dir);
        }
        imported
    }

    fn import_into(
        &self,
        id: &str,
        dir: &Path,
        new: NewDocument,
        source: impl Read,
        info: OriginalInfo,
        expect: Option<&str>,
    ) -> Result<Option<Document>, VaultError> {
        let mut temp = tempfile::Builder::new()
            .prefix(IMPORT_PREFIX)
            .tempfile_in(dir)?;
        // Held until the file is renamed: an opening handle never removes a
        // folder whose file is locked, however long the source stalls.
        temp.as_file().lock()?;
        let sealed = original::seal(&self.sealer, id, source, temp.as_file_mut())?;
        if expect.is_some_and(|sha256| sha256 != sealed.sha256) {
            return Ok(None);
        }
        temp.as_file().sync_all()?;
        let lock = self.write_lock()?;
        if let Some(project) = &new.project {
            self.load_record::<Project>(project)?;
        }
        if self.fault(ObjectKind::Original, id, Stage::BeforeRename) {
            let _ = temp.into_temp_path().keep();
            return Err(VaultError::Crashed);
        }
        temp.persist(original_path(&self.root, id))
            .map_err(|error| error.error)?;
        if self.fault(ObjectKind::Original, id, Stage::AfterRename) {
            return Err(VaultError::Crashed);
        }
        let original = Original {
            sha256: sealed.sha256,
            len: sealed.len,
            info,
        };
        let document = new_document(id.to_string(), new, Some(original));
        self.write_record(&lock, &document)?;
        Ok(Some(document))
    }

    /// The Source Original of Document `id`, to read in bounded memory.
    /// [`OriginalReader`] says what an error in the middle of it means. An
    /// original whose record was never written is not there.
    pub fn open_original(&self, id: &str) -> Result<OriginalReader, VaultError> {
        if !is_id(id) {
            return Err(VaultError::InvalidId);
        }
        if !document_path(&self.root, id).is_file() {
            return Err(VaultError::NotFound {
                kind: ObjectKind::Original,
                id: id.to_string(),
            });
        }
        OriginalReader::open(&self.sealer, id, &original_path(&self.root, id))
    }

    /// A viewing copy: Document `id`'s original decrypted into `dir`, a
    /// folder outside the Vault that the caller names and wipes, as
    /// `file_name`, replacing a file of that name. It is written to a temp
    /// file first, so an original found damaged part way leaves nothing in
    /// `dir`.
    pub fn copy_original_to(
        &self,
        id: &str,
        dir: &Path,
        file_name: &str,
    ) -> Result<PathBuf, VaultError> {
        if !is_file_name(file_name) {
            return Err(VaultError::InvalidCopyTarget);
        }
        let mut reader = self.open_original(id)?;
        std::fs::create_dir_all(dir)?;
        // Plaintext never lands in the Vault, nor over one of its files,
        // whatever alias the folder is reached by.
        if std::fs::canonicalize(dir)?.starts_with(std::fs::canonicalize(&self.root)?) {
            return Err(VaultError::InvalidCopyTarget);
        }
        let mut temp = tempfile::Builder::new().prefix(".copy-").tempfile_in(dir)?;
        io::copy(&mut reader, temp.as_file_mut()).map_err(original::read_error)?;
        let path = dir.join(file_name);
        temp.persist(&path).map_err(|error| error.error)?;
        Ok(path)
    }

    /// The Documents whose original has the SHA-256 `sha256` (hex, as
    /// [`crate::source_sha256`] gives it), most recently updated first:
    /// what a repeated import offers to open (#1091 Decision 7).
    pub fn documents_with_source(&self, sha256: &str) -> Vec<DocumentSummary> {
        let mut documents = self.documents();
        documents.retain(|document| {
            (document.source_sha256.as_deref())
                .is_some_and(|hash| hash.eq_ignore_ascii_case(sha256))
        });
        documents
    }

    /// The Document `id`, read from its file.
    pub fn document(&self, id: &str) -> Result<Document, VaultError> {
        self.load_record(id)
    }

    /// Load `id`, change it with `change`, and save it as the next revision,
    /// all under the write lock. Its Project stays as stored.
    pub fn update_document(
        &self,
        id: &str,
        change: impl FnOnce(&mut Document),
    ) -> Result<Document, VaultError> {
        self.update(id, change)
    }

    /// Replace Document `id`'s private notes (#1091 Decision 3) under the
    /// write lock; its text, review and place stay as stored.
    pub fn set_notes(&self, id: &str, notes: String) -> Result<Document, VaultError> {
        self.update(id, |document: &mut Document| document.notes = notes)
    }

    /// Save `document` unless this handle wrote, or its file holds, a newer
    /// revision. Its Project stays as stored.
    pub fn save_document(&self, document: &Document) -> Result<Saved, VaultError> {
        self.save(document)
    }

    /// Delete the Document, everything in its folder and the AI Replies
    /// saved for it. Later saves of it by this handle are skipped.
    pub fn delete_document(&self, id: &str) -> Result<(), VaultError> {
        if !is_id(id) {
            return Err(VaultError::InvalidId);
        }
        let lock = self.write_lock()?;
        self.delete_document_locked(&lock, id)
    }

    /// [`Self::delete_document`] of each of `ids`, only while every one
    /// that still exists is Unfiled: checked under the write lock, so a
    /// Document moved into a Project since the user was asked is never
    /// deleted. `Ok(false)` when one was moved, and nothing is deleted.
    /// One that can't be read (another key, damaged) was never moved by
    /// this Vault and is deleted.
    pub fn delete_unfiled_documents(&self, ids: &[String]) -> Result<bool, VaultError> {
        if !ids.iter().all(|id| is_id(id)) {
            return Err(VaultError::InvalidId);
        }
        let lock = self.write_lock()?;
        self.refresh()?;
        for id in ids {
            match self.load_record::<Document>(id) {
                Ok(document) if document.project.is_some() => return Ok(false),
                Err(VaultError::Io(error)) => return Err(VaultError::Io(error)),
                _ => {}
            }
        }
        for id in ids {
            self.delete_document_locked(&lock, id)?;
        }
        Ok(true)
    }

    fn delete_document_locked(&self, lock: &WriteLock<'_>, id: &str) -> Result<(), VaultError> {
        self.check_alive()?;
        // Replies first: a delete stopped part way is finished by deleting
        // again.
        self.delete_replies_of(lock, &ReplyOwner::Document(id.to_string()))?;
        remove_sealed(&document_path(&self.root, id))?;
        match std::fs::remove_dir_all(document_dir(&self.root, id)) {
            Err(e) if e.kind() != io::ErrorKind::NotFound => return Err(e.into()),
            _ => {}
        }
        self.forget_deleted::<Document>(id)
    }

    /// Note `id` deleted: later saves of it by this handle are skipped,
    /// and it leaves the cached summaries.
    fn forget_deleted<R: Record>(&self, id: &str) -> Result<(), VaultError> {
        let mut state = self.state();
        state.written.insert(id.to_string(), Last::Deleted);
        let table = R::table(&mut state.manifest);
        if table.ok.remove(id).is_some() | table.damaged.remove(id).is_some() {
            state.dirty = true;
        }
        self.write_manifest_debounced(&mut state)
    }

    /// The user-wide Always and Never lists; empty ones when none were
    /// saved.
    pub fn user_lists(&self) -> Result<UserLists, VaultError> {
        let path = self.user_lists_path();
        let damaged = || VaultError::Damaged {
            kind: ObjectKind::UserLists,
            id: USER_LISTS_ID.into(),
        };
        let mut file = match File::open(&path) {
            Ok(file) => file,
            Err(e) if e.kind() == io::ErrorKind::NotFound => return Ok(UserLists::default()),
            Err(e) => return Err(e.into()),
        };
        let bytes = match read_limited(&mut file) {
            Ok(bytes) => bytes,
            Err(Unseal::Io(e)) => return Err(e.into()),
            Err(_) => return Err(damaged()),
        };
        let plain = (self.sealer)
            .open(ObjectKind::UserLists, USER_LISTS_ID, &bytes)
            .map_err(|_| damaged())?;
        serde_json::from_slice(&plain).map_err(|_| damaged())
    }

    /// Whether `user-lists.enc` opens with this key: `None` when there is
    /// none, or it is not a sealed file at all.
    fn user_lists_open(&self) -> Option<bool> {
        let mut file = File::open(self.user_lists_path()).ok()?;
        let bytes = read_limited(&mut file).ok()?;
        match (self.sealer).open(ObjectKind::UserLists, USER_LISTS_ID, &bytes) {
            Ok(_) => Some(true),
            Err(Unseal::Rejected) => Some(false),
            Err(_) => None,
        }
    }

    /// Load the user-wide lists, change them with `change`, and save them
    /// as the next revision, all under the write lock.
    pub fn update_user_lists(
        &self,
        change: impl FnOnce(&mut Lists),
    ) -> Result<UserLists, VaultError> {
        let lock = self.write_lock()?;
        let stored = self.user_lists()?;
        let last = match self.state().written.get(USER_LISTS_ID).copied() {
            Some(Last::Revision(last)) => last,
            _ => 0,
        };
        let mut lists = stored.clone();
        change(&mut lists.lists);
        lists.revision = (stored.revision.max(last).checked_add(1))
            .ok_or_else(|| io::Error::other("A Vault revision overflowed"))?;
        self.write_user_lists(&lock, &lists)?;
        Ok(lists)
    }

    /// Save `lists` unless this handle wrote, or the file holds, a newer
    /// revision.
    pub fn save_user_lists(&self, lists: &UserLists) -> Result<Saved, VaultError> {
        let lock = self.write_lock()?;
        let last = match self.state().written.get(USER_LISTS_ID).copied() {
            Some(Last::Revision(last)) => last,
            _ => 0,
        };
        if last > lists.revision || self.user_lists()?.revision > lists.revision {
            return Ok(Saved::Skipped);
        }
        self.write_user_lists(&lock, lists)?;
        Ok(Saved::Written)
    }

    fn write_user_lists(&self, _lock: &WriteLock<'_>, lists: &UserLists) -> Result<(), VaultError> {
        let json = serde_json::to_vec(lists).map_err(io::Error::from)?;
        let path = self.user_lists_path();
        self.write_object(ObjectKind::UserLists, USER_LISTS_ID, &path, &json)?;
        (self.state().written).insert(USER_LISTS_ID.into(), Last::Revision(lists.revision));
        Ok(())
    }

    fn update<R: Record>(&self, id: &str, change: impl FnOnce(&mut R)) -> Result<R, VaultError> {
        if !is_id(id) {
            return Err(VaultError::InvalidId);
        }
        let lock = self.write_lock()?;
        self.update_locked(&lock, id, change)
    }

    fn update_locked<R: Record>(
        &self,
        lock: &WriteLock<'_>,
        id: &str,
        change: impl FnOnce(&mut R),
    ) -> Result<R, VaultError> {
        let stored: R = self.load_record(id)?;
        let mut record = stored.clone();
        change(&mut record);
        record.settle(&stored, self.next_revision(&stored)?, now());
        self.write_record(lock, &record)?;
        Ok(record)
    }

    /// The revision a write over `stored` takes: past the file's and past
    /// the last this handle wrote.
    fn next_revision<R: Record>(&self, stored: &R) -> Result<u64, VaultError> {
        let last = match self.state().written.get(stored.id()).copied() {
            Some(Last::Deleted) => return Err(not_found::<R>(stored.id())),
            Some(Last::Revision(last)) => last,
            None => 0,
        };
        Ok((stored.revision().max(last).checked_add(1))
            .ok_or_else(|| io::Error::other("A Vault revision overflowed"))?)
    }

    fn save<R: Record>(&self, record: &R) -> Result<Saved, VaultError> {
        let id = record.id();
        if !is_id(id) {
            return Err(VaultError::InvalidId);
        }
        let lock = self.write_lock()?;
        let last = self.state().written.get(id).copied();
        if (last).is_some_and(|last| match last {
            Last::Deleted => true,
            Last::Revision(last) => last > record.revision(),
        }) {
            return Ok(Saved::Skipped);
        }
        let stored: R = self.load_record(id)?;
        // Another handle wrote a newer revision: the caller's copy is stale.
        if stored.revision() > record.revision() {
            return Ok(Saved::Skipped);
        }
        let mut record = record.clone();
        record.settle(&stored, record.revision(), now());
        self.write_record(&lock, &record)?;
        Ok(Saved::Written)
    }

    fn load_record<R: Record>(&self, id: &str) -> Result<R, VaultError> {
        if !is_id(id) {
            return Err(VaultError::InvalidId);
        }
        match load::<R>(&self.sealer, &R::path(&self.root, id), id) {
            Ok((record, _)) => Ok(record),
            Err(LoadError::Io(e)) if e.kind() == io::ErrorKind::NotFound => Err(not_found::<R>(id)),
            Err(LoadError::Io(e)) => Err(e.into()),
            Err(LoadError::Rejected | LoadError::Damaged) => Err(VaultError::Damaged {
                kind: R::KIND,
                id: id.to_string(),
            }),
        }
    }

    /// Seal `record` over its file, then note it in the cached summaries.
    fn write_record<R: Record>(&self, _lock: &WriteLock<'_>, record: &R) -> Result<(), VaultError> {
        let json = serde_json::to_vec(record).map_err(io::Error::from)?;
        let id = record.id();
        let stamp = self.write_object(R::KIND, id, &R::path(&self.root, id), &json)?;
        let mut state = self.state();
        state
            .written
            .insert(id.to_string(), Last::Revision(record.revision()));
        let table = R::table(&mut state.manifest);
        table.damaged.remove(id);
        let summary = record.summary();
        let before = table.ok.insert(
            id.to_string(),
            Entry {
                summary: summary.clone(),
                stamp,
            },
        );
        if before.is_none_or(|entry| entry.summary != summary) {
            state.dirty = true;
        }
        self.write_manifest_debounced(&mut state)
    }

    /// Replace `path` with `plain` sealed for `kind` and `id`; the new
    /// file's stamp.
    fn write_object(
        &self,
        kind: ObjectKind,
        id: &str,
        path: &Path,
        plain: &[u8],
    ) -> Result<Stamp, VaultError> {
        self.check_alive()?;
        let bytes = self.sealer.seal(kind, id, plain)?;
        if bytes.len() as u64 > MAX_FILE {
            return Err(VaultError::TooLarge {
                kind,
                id: id.to_string(),
            });
        }
        let temp = temp_beside(path, &bytes)?;
        if self.fault(kind, id, Stage::BeforeRename) {
            let _ = temp.into_temp_path().keep();
            return Err(VaultError::Crashed);
        }
        temp.persist(path).map_err(|error| error.error)?;
        if self.fault(kind, id, Stage::AfterRename) {
            return Err(VaultError::Crashed);
        }
        let metadata = std::fs::metadata(path)?;
        Ok(Stamp::of(&metadata, &bytes).unwrap_or_default())
    }

    /// The manifest is a cache: a failed write leaves it to the next check.
    /// Only an injected crash is passed on.
    fn write_manifest_debounced(&self, state: &mut State) -> Result<(), VaultError> {
        let due = (state.manifest_written).is_none_or(|at| at.elapsed() >= MANIFEST_DEBOUNCE);
        if !(state.dirty && due) {
            return Ok(());
        }
        match self.write_manifest(state) {
            Err(VaultError::Crashed) => Err(VaultError::Crashed),
            Ok(()) | Err(_) => Ok(()),
        }
    }

    /// Check the cache against the folder, taking in other handles' writes,
    /// and seal it to `manifest.enc`. The caller holds the write lock.
    fn write_manifest(&self, state: &mut State) -> Result<(), VaultError> {
        if !self.writes_manifest {
            return Ok(());
        }
        state.manifest.check(&self.sealer, &self.root)?;
        let json = state.manifest.to_json()?;
        self.write_object(
            ObjectKind::Manifest,
            MANIFEST_ID,
            &self.manifest_path(),
            &json,
        )?;
        state.dirty = false;
        state.manifest_written = Some(Instant::now());
        Ok(())
    }

    /// `true` when the hook makes this step a crash.
    fn fault(&self, kind: ObjectKind, id: &str, stage: Stage) -> bool {
        let Some(hook) = &self.hook else {
            return false;
        };
        let crash = hook(&WriteStep { kind, id, stage }) == Fault::Crash;
        if crash {
            self.crashed.store(true, Ordering::SeqCst);
        }
        crash
    }

    /// [`VaultError::Crashed`] once a fault hook stopped a write: a crashed
    /// handle writes nothing more, not even to put back a failed step.
    fn check_alive(&self) -> Result<(), VaultError> {
        if self.crashed.load(Ordering::SeqCst) {
            return Err(VaultError::Crashed);
        }
        Ok(())
    }

    fn write_lock(&self) -> Result<WriteLock<'_>, VaultError> {
        self.check_alive()?;
        self.held_lock()
    }

    /// The write lock, held for a read that must see one moment's records
    /// (#1198); a crashed handle still reads, as it did without it.
    fn held_lock(&self) -> Result<WriteLock<'_>, VaultError> {
        let file = self.lock_file.lock().unwrap_or_else(|e| e.into_inner());
        file.lock()?;
        Ok(WriteLock { file })
    }

    fn state(&self) -> MutexGuard<'_, State> {
        self.state.lock().unwrap_or_else(|e| e.into_inner())
    }

    fn manifest_path(&self) -> PathBuf {
        self.root.join("manifest.enc")
    }

    fn user_lists_path(&self) -> PathBuf {
        self.root.join("user-lists.enc")
    }
}

impl Drop for Vault {
    /// Write a manifest the debounce held back.
    fn drop(&mut self) {
        if !self.writes_manifest || self.crashed.load(Ordering::SeqCst) || !self.state().dirty {
            return;
        }
        let _ = self.flush();
    }
}

fn new_document(id: String, new: NewDocument, original: Option<Original>) -> Document {
    let now = now();
    let unconverted = original.is_some() && new.markdown.is_empty();
    Document {
        id,
        project: new.project,
        title: new.title,
        source_kind: new.source_kind,
        origin: new.origin,
        exclude_from_ai: false,
        created: now,
        updated: now,
        revision: 1,
        markdown: new.markdown,
        unconverted,
        scanned: false,
        ready_fingerprint: None,
        mapping: Mapping::default(),
        review: None,
        original,
        counts: None,
        notes: String::new(),
        from_reply: None,
    }
}

/// A bare file name: one plain component, no folder, drive or stream.
fn is_file_name(name: &str) -> bool {
    let mut components = Path::new(name).components();
    let plain = matches!(
        (components.next(), components.next()),
        (Some(Component::Normal(component)), None) if component == name
    );
    plain && !name.contains(['/', '\\', ':'])
}

fn not_found<R: Record>(id: &str) -> VaultError {
    VaultError::NotFound {
        kind: R::KIND,
        id: id.to_string(),
    }
}

/// Remove a sealed file, overwriting it with zeros first. It is renamed to a temp name before that, so a reader
/// without the lock sees the whole file or none, and a crash leaves a temp
/// file the next open sweeps.
fn remove_sealed(path: &Path) -> io::Result<()> {
    let (Some(dir), true) = (path.parent(), path.try_exists()?) else {
        return Ok(());
    };
    let doomed = tempfile::Builder::new()
        .prefix(TEMP_PREFIX)
        .tempfile_in(dir)?
        .into_temp_path();
    std::fs::rename(path, &doomed)?;
    let len = std::fs::metadata(&doomed)?.len();
    {
        let mut file = File::options().write(true).open(&doomed)?;
        file.write_all(&vec![0; len.min(MAX_FILE) as usize])?;
        file.sync_all()?;
    }
    doomed.close()
}

/// Remove the document folders without a record that have not changed for
/// [`ORPHAN_AGE`]: what an import that died before its record left. An
/// import still running writes into its folder all the time.
fn sweep_orphans(root: &Path) {
    let Ok(entries) = std::fs::read_dir(root.join("docs")) else {
        return;
    };
    for entry in entries.flatten() {
        let name = entry.file_name();
        let Some(id) = name.to_str().filter(|id| is_id(id)) else {
            continue;
        };
        let dir = entry.path();
        if !dir.is_dir() || !matches!(document_path(root, id).try_exists(), Ok(false)) {
            continue;
        }
        let old =
            last_change(&dir).is_some_and(|at| at.elapsed().is_ok_and(|age| age > ORPHAN_AGE));
        if old && !in_use(&dir) {
            let _ = std::fs::remove_dir_all(&dir);
        }
    }
}

/// A file in `dir` is locked: an import is still writing it.
fn in_use(dir: &Path) -> bool {
    let Ok(entries) = std::fs::read_dir(dir) else {
        return true;
    };
    entries.flatten().any(|entry| {
        File::open(entry.path()).map_or(true, |file| {
            matches!(file.try_lock(), Err(std::fs::TryLockError::WouldBlock))
        })
    })
}

/// When anything in `dir` last changed: its newest file's modification
/// time, or the folder's own when it holds none; `None` when unreadable.
fn last_change(dir: &Path) -> Option<SystemTime> {
    let mut newest = None;
    for entry in std::fs::read_dir(dir).ok()? {
        let modified = entry.ok()?.metadata().ok()?.modified().ok()?;
        newest = newest.max(Some(modified));
    }
    newest.or_else(|| std::fs::metadata(dir).ok()?.modified().ok())
}

/// Remove the temp files dead writes left.
fn sweep_temp_files(root: &Path) {
    for dir in write_dirs(root) {
        let Ok(entries) = std::fs::read_dir(&dir) else {
            continue;
        };
        for entry in entries.flatten() {
            let is_temp = entry
                .file_name()
                .to_str()
                .is_some_and(|name| name.starts_with(TEMP_PREFIX));
            if is_temp {
                let _ = std::fs::remove_file(entry.path());
            }
        }
    }
}
