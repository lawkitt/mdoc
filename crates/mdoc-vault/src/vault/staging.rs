//! An import in two steps (ADR 0130): the slow part, sealing the Source
//! Original, runs outside the write lock into `staging/`, a folder no
//! reader lists; the short part, making it a Document, runs under the lock
//! and checks there what the caller knew before (the bytes' hash, the
//! Project). A desktop window seals beside its writer queue and publishes
//! as one of its jobs, so a large file never holds the queue up.
use super::{Vault, new_document};
use crate::{
    ObjectKind, VaultError,
    manifest::{document_dir, original_path},
    new_id,
    original::{self, IMPORT_PREFIX},
    records::{Document, NewDocument, Original, OriginalInfo, Project},
};
use std::{fs::File, io::Read, path::Path};

/// The folder sealed imports wait in until they are published.
pub(super) const STAGING: &str = "staging";

/// An import's Source Original, sealed into the Vault's `staging/` folder
/// and not yet a Document: nothing lists or reads it until
/// [`Vault::publish_import`] makes it one. Its file is locked while this
/// value lives and removed when it is dropped unpublished; one a crash
/// left is unlocked, and the next open removes it.
pub struct StagedImport {
    id: String,
    file: tempfile::NamedTempFile,
    sha256: String,
    len: u64,
}

impl StagedImport {
    /// The SHA-256 of the bytes sealed, in lowercase hex.
    pub fn sha256(&self) -> &str {
        &self.sha256
    }
}

impl std::fmt::Debug for StagedImport {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("StagedImport")
            .field("id", &self.id)
            .field("sha256", &self.sha256)
            .field("len", &self.len)
            .finish_non_exhaustive()
    }
}

impl Vault {
    /// Seal `source` as a future Document's original into `staging/`, one
    /// chunk at a time and without the write lock.
    pub fn stage_import(&self, source: impl Read) -> Result<StagedImport, VaultError> {
        let id = new_id()?;
        let dir = self.root.join(STAGING);
        let mut file = {
            // Made and locked under the write lock, which an opening handle
            // sweeps under: it never sees the file before its lock.
            let _lock = self.write_lock()?;
            std::fs::create_dir_all(&dir)?;
            let file = tempfile::Builder::new()
                .prefix(IMPORT_PREFIX)
                .tempfile_in(&dir)?;
            // Held while staged: an opening handle never removes it.
            file.as_file().lock()?;
            file
        };
        let sealed = original::seal(&self.sealer, &id, source, file.as_file_mut())?;
        file.as_file().sync_all()?;
        Ok(StagedImport {
            id,
            file,
            sha256: sealed.sha256,
            len: sealed.len,
        })
    }

    /// Make `staged` a Document, Unfiled or in an existing Project, checked
    /// under the write lock: only while its bytes hash to `expect`, when
    /// given, and its Project still exists. `Ok(None)` for another hash,
    /// with nothing written. Whatever stops it, nothing of it stays but
    /// what a crash leaves.
    pub fn publish_import(
        &self,
        staged: StagedImport,
        new: NewDocument,
        info: OriginalInfo,
        expect: Option<&str>,
    ) -> Result<Option<Document>, VaultError> {
        if expect.is_some_and(|sha256| !sha256.eq_ignore_ascii_case(&staged.sha256)) {
            return Ok(None);
        }
        let lock = self.write_lock()?;
        if let Some(project) = &new.project {
            self.load_record::<Project>(project)?;
        }
        let StagedImport {
            id,
            file,
            sha256,
            len,
        } = staged;
        if self.fault(ObjectKind::Original, &id, super::Stage::BeforeRename) {
            // As a crash leaves it: in staging, for the next open.
            let _ = file.into_temp_path().keep();
            return Err(VaultError::Crashed);
        }
        let dir = document_dir(&self.root, &id);
        let published = (|| {
            std::fs::create_dir_all(&dir)?;
            file.persist(original_path(&self.root, &id))
                .map_err(|error| error.error)?;
            if self.fault(ObjectKind::Original, &id, super::Stage::AfterRename) {
                return Err(VaultError::Crashed);
            }
            let document = new_document(id.clone(), new, Some(Original { sha256, len, info }));
            self.write_record(&lock, &document)?;
            Ok(document)
        })();
        if !matches!(published, Ok(_) | Err(VaultError::Crashed)) {
            let _ = std::fs::remove_dir_all(&dir);
        }
        published.map(Some)
    }
}

/// Remove the staged imports no handle holds: what a crash left. The
/// caller holds the write lock.
pub(super) fn sweep_staging(root: &Path) {
    let Ok(entries) = std::fs::read_dir(root.join(STAGING)) else {
        return;
    };
    for entry in entries.flatten() {
        let held = File::open(entry.path()).map_or(true, |file| {
            matches!(file.try_lock(), Err(std::fs::TryLockError::WouldBlock))
        });
        if !held {
            let _ = std::fs::remove_file(entry.path());
        }
    }
}
