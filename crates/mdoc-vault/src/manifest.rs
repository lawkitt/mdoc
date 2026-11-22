//! The manifest: a cache of every object's summary and its file's stamp,
//! so lists and search open no record (#1091 Decision 15). Object files are
//! the only truth: [`Manifest::check`] compares the cache with the folder and
//! re-reads every file whose stamp differs, so a missing, stale or damaged
//! manifest costs one read of the files it got wrong.
use crate::{
    ObjectKind, is_id,
    records::{Document, DocumentSummary, Project, ProjectSummary},
    seal::{Sealer, Unseal, read_limited},
};
use serde::{Deserialize, Serialize, de::DeserializeOwned};
use std::{
    collections::BTreeMap,
    io,
    path::{Path, PathBuf},
};

/// The manifest's format; another one is rebuilt.
const FORMAT: u32 = 1;

/// A sealed file's header (magic, kind, nonce) and tag: the nonce is new on
/// every write, so a stamp tells two writes apart even within one tick of
/// a coarse file clock (NTFS's 15.6 ms), and the tag changes with damage to
/// it.
const HEAD: usize = crate::seal::MAGIC.len() + 1 + 12;
const TAIL: usize = 16;

/// A file as it is: any write, replacement or copy over it changes this.
/// On Unix its inode and status-change time too, which copying tools
/// cannot set and an atomic save changes; everywhere, the sealed file's
/// nonce and tag.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub(crate) struct Stamp {
    /// Its modification time: seconds and nanoseconds since the Unix epoch.
    modified: u64,
    nanos: u32,
    len: u64,
    inode: u64,
    /// Its status-change time, seconds and nanoseconds.
    changed: i64,
    changed_nanos: i64,
    /// The file's first [`HEAD`] and last [`TAIL`] bytes, in hex.
    ends: String,
}

impl Stamp {
    /// The stamp of a file with `metadata` holding `bytes`; `None` when the
    /// system gives no modification time: such a file is never taken as
    /// unchanged.
    pub fn of(metadata: &std::fs::Metadata, bytes: &[u8]) -> Option<Self> {
        let head = &bytes[..bytes.len().min(HEAD)];
        let tail = &bytes[bytes.len().saturating_sub(TAIL)..];
        Self::from_parts(metadata, head, tail)
    }

    /// The stamp of the file at `path`, reading only its ends.
    pub fn read(path: &Path) -> io::Result<Option<Self>> {
        use std::io::{Read, Seek, SeekFrom};
        let mut file = std::fs::File::open(path)?;
        let metadata = file.metadata()?;
        let mut head = Vec::with_capacity(HEAD);
        (&mut file).take(HEAD as u64).read_to_end(&mut head)?;
        let len = metadata.len();
        file.seek(SeekFrom::Start(len.saturating_sub(TAIL as u64)))?;
        let mut tail = Vec::with_capacity(TAIL);
        file.take(TAIL as u64).read_to_end(&mut tail)?;
        Ok(Self::from_parts(&metadata, &head, &tail))
    }

    fn from_parts(metadata: &std::fs::Metadata, head: &[u8], tail: &[u8]) -> Option<Self> {
        let since = metadata
            .modified()
            .ok()?
            .duration_since(std::time::UNIX_EPOCH)
            .ok()?;
        #[cfg(unix)]
        let (inode, changed, changed_nanos) = {
            use std::os::unix::fs::MetadataExt;
            (metadata.ino(), metadata.ctime(), metadata.ctime_nsec())
        };
        #[cfg(not(unix))]
        let (inode, changed, changed_nanos) = (0, 0, 0);
        Some(Self {
            modified: since.as_secs(),
            nanos: since.subsec_nanos(),
            len: metadata.len(),
            inode,
            changed,
            changed_nanos,
            ends: head
                .iter()
                .chain(tail)
                .map(|b| format!("{b:02x}"))
                .collect(),
        })
    }
}

/// One object's summary and its file as it was when summarized.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub(crate) struct Entry<T> {
    pub summary: T,
    pub stamp: Stamp,
}

/// The objects of one kind, by id: those that opened, and those that did
/// not with their file's stamp, so a damaged file is read again only once
/// it changes.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub(crate) struct Table<T> {
    pub ok: BTreeMap<String, Entry<T>>,
    pub damaged: BTreeMap<String, Stamp>,
}

impl<T> Default for Table<T> {
    fn default() -> Self {
        Self {
            ok: BTreeMap::new(),
            damaged: BTreeMap::new(),
        }
    }
}

/// What a check found.
#[derive(Debug, Default)]
pub(crate) struct Checked {
    /// Anything differs from the cache, stamps included.
    pub changed: bool,
    /// Files the key opened in this check.
    pub opened: usize,
    /// Files whose authentication failed in this check.
    pub rejected: usize,
}

impl Checked {
    fn add(&mut self, other: Self) {
        self.changed |= other.changed;
        self.opened += other.opened;
        self.rejected += other.rejected;
    }
}

/// A record kind stored as its own sealed file and summarized in the
/// manifest. Later kinds (AI Replies) implement it too.
pub(crate) trait Record: Clone + Serialize + DeserializeOwned {
    type Summary: Clone + PartialEq;
    const KIND: ObjectKind;
    fn id(&self) -> &str;
    fn revision(&self) -> u64;
    fn summary(&self) -> Self::Summary;
    fn path(root: &Path, id: &str) -> PathBuf;
    fn table(manifest: &mut Manifest) -> &mut Table<Self::Summary>;
    /// Before a write over `stored`: keep the fields only the Vault
    /// changes, and stamp `revision` and `now`.
    fn settle(&mut self, stored: &Self, revision: u64, now: u64);
}

impl Record for Project {
    type Summary = ProjectSummary;
    const KIND: ObjectKind = ObjectKind::Project;
    fn id(&self) -> &str {
        &self.id
    }
    fn revision(&self) -> u64 {
        self.revision
    }
    fn summary(&self) -> ProjectSummary {
        ProjectSummary::of(self)
    }
    fn path(root: &Path, id: &str) -> PathBuf {
        project_path(root, id)
    }
    fn table(manifest: &mut Manifest) -> &mut Table<ProjectSummary> {
        &mut manifest.projects
    }
    fn settle(&mut self, stored: &Self, revision: u64, now: u64) {
        self.id.clone_from(&stored.id);
        self.created = stored.created;
        self.revision = revision;
        self.updated = now;
    }
}

impl Record for Document {
    type Summary = DocumentSummary;
    const KIND: ObjectKind = ObjectKind::Document;
    fn id(&self) -> &str {
        &self.id
    }
    fn revision(&self) -> u64 {
        self.revision
    }
    fn summary(&self) -> DocumentSummary {
        DocumentSummary::of(self)
    }
    fn path(root: &Path, id: &str) -> PathBuf {
        document_path(root, id)
    }
    fn table(manifest: &mut Manifest) -> &mut Table<DocumentSummary> {
        &mut manifest.documents
    }
    /// A Document's Project changes only by a move (item 4), so a stale
    /// copy saved after a move cannot undo it.
    fn settle(&mut self, stored: &Self, revision: u64, now: u64) {
        self.id.clone_from(&stored.id);
        self.project.clone_from(&stored.project);
        self.created = stored.created;
        self.revision = revision;
        self.updated = now;
    }
}

/// Why a record did not load.
pub(crate) enum LoadError {
    Io(io::Error),
    /// Authentication failed: damaged, another key, or another id or kind.
    Rejected,
    /// Not sealed, or its JSON is not this record.
    Damaged,
}

/// Read, open and parse the `R` with `id` at `path`, with the stamp of the
/// file that was read.
pub(crate) fn load<R: Record>(
    sealer: &Sealer,
    path: &Path,
    id: &str,
) -> Result<(R, Option<Stamp>), LoadError> {
    let mut file = std::fs::File::open(path).map_err(LoadError::Io)?;
    // Stamped from the handle that is read, so the stamp is of these bytes.
    let metadata = file.metadata().ok();
    let bytes = read_limited(&mut file).map_err(unseal_error)?;
    let stamp = metadata.and_then(|metadata| Stamp::of(&metadata, &bytes));
    let plain = sealer.open(R::KIND, id, &bytes).map_err(unseal_error)?;
    let record: R = serde_json::from_slice(&plain).map_err(|_| LoadError::Damaged)?;
    if record.id() != id {
        return Err(LoadError::Damaged);
    }
    Ok((record, stamp))
}

fn unseal_error(error: Unseal) -> LoadError {
    match error {
        Unseal::Io(e) => LoadError::Io(e),
        Unseal::NotSealed => LoadError::Damaged,
        Unseal::Rejected => LoadError::Rejected,
    }
}

impl<T: Clone + PartialEq> Table<T> {
    /// Bring the table in line with `found`, the files of its kind now in
    /// the folder: keep entries whose stamp matches, read the rest (damaged
    /// ones always), drop entries whose file is gone.
    fn check<R: Record<Summary = T>>(
        &mut self,
        sealer: &Sealer,
        found: Vec<(String, PathBuf)>,
    ) -> Checked {
        let mut checked = Checked::default();
        let present: std::collections::HashSet<&str> =
            found.iter().map(|(id, _)| id.as_str()).collect();
        let before = (self.ok.len(), self.damaged.len());
        self.ok.retain(|id, _| present.contains(id.as_str()));
        self.damaged.retain(|id, _| present.contains(id.as_str()));
        checked.changed |= before != (self.ok.len(), self.damaged.len());
        for (id, path) in &found {
            let stamp = Stamp::read(path).ok().flatten();
            if stamp.is_some() && self.ok.get(id).map(|e| &e.stamp) == stamp.as_ref() {
                continue;
            }
            let loaded = load::<R>(sealer, path, id);
            checked.changed |= !matches!(
                (&loaded, self.damaged.get(id)),
                (Err(LoadError::Rejected | LoadError::Damaged), Some(before)) if Some(before) == stamp.as_ref()
            );
            match loaded {
                Ok((record, stamp)) => {
                    checked.opened += 1;
                    self.damaged.remove(id);
                    self.ok.insert(
                        id.clone(),
                        Entry {
                            summary: record.summary(),
                            stamp: stamp.unwrap_or_default(),
                        },
                    );
                }
                Err(LoadError::Io(e)) if e.kind() == io::ErrorKind::NotFound => {
                    self.ok.remove(id);
                    self.damaged.remove(id);
                }
                Err(error) => {
                    if matches!(error, LoadError::Rejected) {
                        checked.rejected += 1;
                    }
                    self.ok.remove(id);
                    // An unreadable file gets the default stamp, which no
                    // file has, so it is tried again at the next check.
                    let stamp = match error {
                        LoadError::Io(_) => Stamp::default(),
                        _ => stamp.unwrap_or_default(),
                    };
                    self.damaged.insert(id.clone(), stamp);
                }
            }
        }
        checked
    }
}

/// The cache, as sealed in `manifest.enc`.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub(crate) struct Manifest {
    format: u32,
    pub projects: Table<ProjectSummary>,
    pub documents: Table<DocumentSummary>,
}

impl Default for Manifest {
    fn default() -> Self {
        Self {
            format: FORMAT,
            projects: Table::default(),
            documents: Table::default(),
        }
    }
}

/// The manifest's id in its authenticated data.
pub(crate) const MANIFEST_ID: &str = "manifest";

impl Manifest {
    /// The manifest at `path`: `Ok(None)` when there is none, `Err(())`
    /// when it is damaged, sealed under another key or of another format.
    pub fn read(sealer: &Sealer, path: &Path) -> Result<Option<Self>, ()> {
        let mut file = match std::fs::File::open(path) {
            Ok(file) => file,
            Err(e) if e.kind() == io::ErrorKind::NotFound => return Ok(None),
            Err(_) => return Err(()),
        };
        let bytes = read_limited(&mut file).map_err(|_| ())?;
        let plain = sealer
            .open(ObjectKind::Manifest, MANIFEST_ID, &bytes)
            .map_err(|_| ())?;
        let manifest: Self = serde_json::from_slice(&plain).map_err(|_| ())?;
        if manifest.format != FORMAT {
            return Err(());
        }
        Ok(Some(manifest))
    }

    pub fn to_json(&self) -> io::Result<Vec<u8>> {
        Ok(serde_json::to_vec(self)?)
    }

    /// Bring the whole cache in line with the object files under `root`.
    /// A folder that can't be listed leaves the cache as it was.
    pub fn check(&mut self, sealer: &Sealer, root: &Path) -> io::Result<Checked> {
        let (projects, documents) = (project_files(root)?, document_files(root)?);
        let mut checked = self.projects.check::<Project>(sealer, projects);
        checked.add(self.documents.check::<Document>(sealer, documents));
        Ok(checked)
    }
}

pub(crate) fn project_path(root: &Path, id: &str) -> PathBuf {
    root.join("projects").join(format!("{id}.enc"))
}

pub(crate) fn document_dir(root: &Path, id: &str) -> PathBuf {
    root.join("docs").join(id)
}

pub(crate) fn document_path(root: &Path, id: &str) -> PathBuf {
    document_dir(root, id).join("doc.enc")
}

/// The entries of `dir`; none when it does not exist.
fn entries(dir: &Path) -> io::Result<Vec<std::fs::DirEntry>> {
    match std::fs::read_dir(dir) {
        Ok(entries) => entries.collect(),
        Err(e) if e.kind() == io::ErrorKind::NotFound => Ok(Vec::new()),
        Err(e) => Err(e),
    }
}

/// `projects/<id>.enc`, by id.
fn project_files(root: &Path) -> io::Result<Vec<(String, PathBuf)>> {
    Ok(entries(&root.join("projects"))?
        .into_iter()
        .filter_map(|entry| {
            let name = entry.file_name();
            let id = name.to_str()?.strip_suffix(".enc")?;
            is_id(id).then(|| (id.to_string(), entry.path()))
        })
        .collect())
}

/// `docs/<id>/doc.enc`, by id, for every document folder holding one.
fn document_files(root: &Path) -> io::Result<Vec<(String, PathBuf)>> {
    Ok(entries(&root.join("docs"))?
        .into_iter()
        .filter_map(|entry| {
            let name = entry.file_name();
            let id = name.to_str().filter(|id| is_id(id))?;
            let path = entry.path().join("doc.enc");
            path.is_file().then(|| (id.to_string(), path))
        })
        .collect())
}

/// Every folder a write may leave a temp file in.
pub(crate) fn write_dirs(root: &Path) -> Vec<PathBuf> {
    let mut dirs = vec![root.to_path_buf(), root.join("projects")];
    if let Ok(entries) = std::fs::read_dir(root.join("docs")) {
        dirs.extend(
            entries
                .flatten()
                .map(|entry| entry.path())
                .filter(|p| p.is_dir()),
        );
    }
    dirs
}

#[cfg(test)]
mod tests {
    use super::Stamp;

    /// Where the file clock is coarse and there is no inode (Windows), two
    /// writes of one length in one tick still differ by their nonce and tag.
    #[test]
    fn equal_length_writes_in_one_tick_have_different_stamps() {
        let temp = tempfile::tempdir().unwrap();
        let file = temp.path().join("a.enc");
        std::fs::write(&file, [1u8; 64]).unwrap();
        let metadata = std::fs::metadata(&file).unwrap();
        let mut other = [1u8; 64];
        other[15] = 2;
        let (a, b) = (
            Stamp::of(&metadata, &[1u8; 64]),
            Stamp::of(&metadata, &other),
        );
        assert!(a.is_some() && a != b);
        let mut damaged_tag = [1u8; 64];
        damaged_tag[63] = 0;
        assert_ne!(a, Stamp::of(&metadata, &damaged_tag));
        assert_eq!(a, Stamp::read(&file).unwrap());
    }
}
