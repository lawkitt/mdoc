//! A Vault document's Source Original through the mobile core (#1091
//! Decisions 7, 14, 23 and 24, in item 3's format): a document's first
//! save keeps the file it was imported from (`DocumentSession::
//! save_into_vault`), a repeated import is found by its bytes, and the app
//! views it from a copy in a folder of its own, or in memory when small.
use super::{DocumentInfo, Vault};
use crate::{CoreError, document_session::Source};
use std::{io::Read, path::Path};

/// What a document keeps of its Source Original beside its bytes.
#[derive(Clone, Debug, PartialEq, uniffi::Record)]
pub struct OriginalFile {
    /// The file name it was imported from, without its folder.
    pub file_name: String,
    /// Its length in bytes.
    pub length: u64,
    /// Kept locked, as imported (Decision 23): viewing it asks for the
    /// password again.
    pub password_protected: bool,
    /// The import's warning, as shown when it was imported.
    pub import_warning: Option<String>,
    /// What else the Markdown leaves out, counted.
    pub left_out: Option<String>,
}

/// The largest original `original_bytes` hands over in memory; a larger
/// one is viewed from `copy_original` (Decision 24).
const IN_MEMORY: u64 = 16 * 1024 * 1024;

#[uniffi::export]
impl Vault {
    /// The documents already made from the file at `path`, most recently
    /// updated first: a repeated import offers to open one instead
    /// (Decision 7). They are found by the bytes an import keeps (a photo's
    /// without its metadata), whatever the file is called now.
    pub fn documents_from_source(&self, path: String) -> Result<Vec<DocumentInfo>, CoreError> {
        let Some(source) = Source::of(Path::new(&path), false)? else {
            return Ok(Vec::new());
        };
        let sha256 = mdoc_vault::source_sha256(source.reader()?)?;
        Ok(self.infos(self.store().documents_with_source(&sha256)))
    }

    /// What document `id` keeps of its original; `None` for one pasted,
    /// begun in mdoc, or made from pages picked together.
    pub fn original(&self, id: String) -> Result<Option<OriginalFile>, CoreError> {
        Ok(self
            .store()
            .document(&id)?
            .original
            .map(|original| OriginalFile {
                file_name: original.info.file_name,
                length: original.len,
                password_protected: original.info.password_protected,
                import_warning: original.info.import_warning,
                left_out: original.info.left_out,
            }))
    }

    /// Write document `id`'s original, decrypted, into `folder` under the
    /// name it was imported with, and return its path: the viewing copy
    /// the platform viewers need. `folder` is the app's own, private and
    /// out of backups, outside the Vault; the app wipes it when the view
    /// closes and at launch. A damaged original writes nothing.
    pub fn copy_original(&self, id: String, folder: String) -> Result<String, CoreError> {
        let original = (self.store().document(&id)?.original)
            .ok_or_else(|| CoreError::project("This document keeps no original."))?;
        let name = file_name(&original.info.file_name);
        let path = (self.store()).copy_original_to(&id, Path::new(&folder), &name)?;
        Ok(path.display().to_string())
    }

    /// Document `id`'s original, decrypted, in memory: for one of at most
    /// 16 MiB, which the app views without a copy on disk.
    pub fn original_bytes(&self, id: String) -> Result<Vec<u8>, CoreError> {
        let original = (self.store().document(&id)?.original)
            .ok_or_else(|| CoreError::project("This document keeps no original."))?;
        if original.len > IN_MEMORY {
            return Err(CoreError::invalid(
                "This original is too large to hold in memory: copy it to a folder.",
            ));
        }
        let mut bytes = Vec::with_capacity(original.len as usize);
        (self.store().open_original(&id)?.take(IN_MEMORY + 1))
            .read_to_end(&mut bytes)
            .map_err(damaged)?;
        if bytes.len() as u64 > IN_MEMORY {
            return Err(CoreError::invalid(
                "This original is too large to hold in memory: copy it to a folder.",
            ));
        }
        Ok(bytes)
    }
}

/// `name` as a bare file name a viewing copy can take: a name kept from a
/// phone may hold a colon or a backslash, which a copy refuses.
fn file_name(name: &str) -> String {
    let name: String = (name.chars())
        .map(|c| {
            if matches!(c, '/' | '\\' | ':') {
                '-'
            } else {
                c
            }
        })
        .collect();
    match name.trim() {
        "" | "." | ".." => "original".into(),
        name => name.into(),
    }
}

/// A read error, as the Vault's own when the original is damaged.
fn damaged(error: std::io::Error) -> CoreError {
    match (error.get_ref()).and_then(|inner| inner.downcast_ref::<mdoc_vault::VaultError>()) {
        Some(inner) => CoreError::project(inner.to_string()),
        None => error.into(),
    }
}

#[cfg(test)]
mod tests;
