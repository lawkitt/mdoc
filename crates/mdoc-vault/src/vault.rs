//! A handle on one Vault folder. Writes are one load-change-save at a
//! time: in the process through a mutex and across processes and handles
//! through `.lock`, held only for the write (#1091 Decision 20). A handle
//! never writes a revision older than the last it wrote.
use crate::{
    ObjectKind, VaultError, is_id,
    manifest::{
        Entry, LoadError, MANIFEST_ID, Manifest, Record, Stamp, document_dir, document_path, load,
        write_dirs,
    },
    new_id,
    records::{Document, DocumentSummary, NewDocument, NewProject, Project, ProjectSummary, now},
    seal::{MAX_FILE, Sealer, TEMP_PREFIX, temp_beside},
};
use std::{
    collections::HashMap,
    fs::File,
    io::{self, Write},
    path::{Path, PathBuf},
    sync::{
        Arc, Mutex, MutexGuard,
        atomic::{AtomicBool, Ordering},
    },
    time::{Duration, Instant},
};

/// A summary change rewrites the manifest at most this often; the rest wait
/// for the next write after it, [`Vault::flush`] or the handle's drop.
const MANIFEST_DEBOUNCE: Duration = Duration::from_secs(2);
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
        Self::open_inner(root.into(), key, None)
    }

    /// [`Self::open`] with a hook run at every write step (tests).
    pub fn open_with_hook(
        root: impl Into<PathBuf>,
        key: &[u8; 32],
        hook: FaultHook,
    ) -> Result<Self, VaultError> {
        Self::open_inner(root.into(), key, Some(hook))
    }

    fn open_inner(
        root: PathBuf,
        key: &[u8; 32],
        hook: Option<FaultHook>,
    ) -> Result<Self, VaultError> {
        std::fs::create_dir_all(&root)?;
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
        };
        vault.opened = {
            let _lock = vault.write_lock()?;
            // Every write holds the lock, so a temp file now is one a dead
            // write left.
            sweep_temp_files(&vault.root);
            let (mut manifest, mut opened) =
                match Manifest::read(&vault.sealer, &vault.manifest_path()) {
                    Ok(Some(manifest)) => (manifest, ManifestCheck::Current),
                    Ok(None) => (Manifest::default(), ManifestCheck::Missing),
                    Err(()) => (Manifest::default(), ManifestCheck::Damaged),
                };
            let checked = manifest.check(&vault.sealer, &vault.root)?;
            if opened != ManifestCheck::Current && checked.opened == 0 && checked.rejected > 0 {
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
        projects.chain(documents).collect()
    }

    /// Pick up what other handles wrote: check the summaries against the
    /// object files again. `true` when anything changed.
    pub fn refresh(&self) -> Result<bool, VaultError> {
        let mut state = self.state();
        let changed = state.manifest.check(&self.sealer, &self.root)?.changed;
        state.dirty |= changed;
        Ok(changed)
    }

    /// Write the manifest now if a summary changed since it was written.
    pub fn flush(&self) -> Result<(), VaultError> {
        let _lock = self.write_lock()?;
        let mut state = self.state();
        if state.dirty {
            self.write_manifest(&mut state)?;
        }
        Ok(())
    }

    pub fn create_project(&self, new: NewProject) -> Result<Project, VaultError> {
        let lock = self.write_lock()?;
        let now = now();
        let project = Project {
            id: new_id()?,
            name: new.name,
            colour: new.colour,
            pinned: new.pinned,
            created: now,
            updated: now,
            revision: 1,
        };
        self.write_record(&lock, &project)?;
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
        let now = now();
        let document = Document {
            id: new_id()?,
            project: new.project,
            title: new.title,
            source_kind: new.source_kind,
            origin: new.origin,
            exclude_from_ai: false,
            created: now,
            updated: now,
            revision: 1,
            markdown: new.markdown,
            scanned: false,
            ready_fingerprint: None,
        };
        self.write_record(&lock, &document)?;
        Ok(document)
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

    /// Save `document` unless this handle wrote, or its file holds, a newer
    /// revision. Its Project stays as stored.
    pub fn save_document(&self, document: &Document) -> Result<Saved, VaultError> {
        self.save(document)
    }

    /// Delete the Document and everything in its folder. Later saves of it
    /// by this handle are skipped.
    pub fn delete_document(&self, id: &str) -> Result<(), VaultError> {
        if !is_id(id) {
            return Err(VaultError::InvalidId);
        }
        let _lock = self.write_lock()?;
        remove_sealed(&document_path(&self.root, id))?;
        match std::fs::remove_dir_all(document_dir(&self.root, id)) {
            Err(e) if e.kind() != io::ErrorKind::NotFound => return Err(e.into()),
            _ => {}
        }
        let mut state = self.state();
        state.written.insert(id.to_string(), Last::Deleted);
        let table = &mut state.manifest.documents;
        if table.ok.remove(id).is_some() | table.damaged.remove(id).is_some() {
            state.dirty = true;
        }
        self.write_manifest_debounced(&mut state)
    }

    fn update<R: Record>(&self, id: &str, change: impl FnOnce(&mut R)) -> Result<R, VaultError> {
        if !is_id(id) {
            return Err(VaultError::InvalidId);
        }
        let lock = self.write_lock()?;
        let stored: R = self.load_record(id)?;
        let last = match self.state().written.get(id).copied() {
            Some(Last::Deleted) => return Err(not_found::<R>(id)),
            Some(Last::Revision(last)) => last,
            None => 0,
        };
        let revision = (stored.revision().max(last).checked_add(1))
            .ok_or_else(|| io::Error::other("A Vault revision overflowed"))?;
        let mut record = stored.clone();
        change(&mut record);
        record.settle(&stored, revision, now());
        self.write_record(&lock, &record)?;
        Ok(record)
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
        let bytes = self.sealer.seal(kind, id, plain)?;
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

    fn write_lock(&self) -> Result<WriteLock<'_>, VaultError> {
        if self.crashed.load(Ordering::SeqCst) {
            return Err(VaultError::Crashed);
        }
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
}

impl Drop for Vault {
    /// Write a manifest the debounce held back.
    fn drop(&mut self) {
        if self.crashed.load(Ordering::SeqCst) || !self.state().dirty {
            return;
        }
        let _ = self.flush();
    }
}

fn not_found<R: Record>(id: &str) -> VaultError {
    VaultError::NotFound {
        kind: R::KIND,
        id: id.to_string(),
    }
}

/// Remove a sealed file as the case store deletes, overwriting it with
/// zeros first. It is renamed to a temp name before that, so a reader
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
