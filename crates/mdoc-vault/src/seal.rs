//! The sealed record format (#1091 Decision 13): magic, a kind byte, a
//! 12-byte random nonce, then the AES-256-GCM ciphertext and tag. The
//! authenticated data is magic, kind and id, so a file renamed to another
//! id or placed as another kind fails to open. Files are replaced whole:
//! written to a temp file beside the target, synced, renamed over it.
use crate::ObjectKind;
use aes_gcm::{
    Aes256Gcm, KeyInit,
    aead::{Aead, Generate, Nonce, Payload},
};
use std::{
    fs::File,
    io::{self, Read, Write},
    path::Path,
};

/// File header and format version.
pub(crate) const MAGIC: &[u8; 8] = b"MDOCVLT1";
const NONCE: usize = 12;
/// Records are small; anything larger is not one of ours.
pub(crate) const MAX_FILE: u64 = 64 * 1024 * 1024;
/// Temp files a write leaves only when it dies before its rename.
pub(crate) const TEMP_PREFIX: &str = ".tmp-";

/// Why a sealed file did not open.
#[derive(Debug)]
pub(crate) enum Unseal {
    Io(io::Error),
    /// Not our header, too short, or too large.
    NotSealed,
    /// Authentication failed: damaged, another key, or another id or kind.
    Rejected,
}

pub(crate) struct Sealer {
    cipher: Aes256Gcm,
}

impl Sealer {
    pub fn new(key: &[u8; 32]) -> Self {
        Self {
            cipher: Aes256Gcm::new(key.into()),
        }
    }

    fn aad(kind: ObjectKind, id: &str) -> Vec<u8> {
        [MAGIC.as_slice(), &[kind.byte()], id.as_bytes()].concat()
    }

    /// The whole file for `plain`.
    pub fn seal(&self, kind: ObjectKind, id: &str, plain: &[u8]) -> io::Result<Vec<u8>> {
        let nonce = Nonce::<Aes256Gcm>::try_generate()
            .map_err(|_| io::Error::other("Could not generate a nonce"))?;
        let aad = Self::aad(kind, id);
        let sealed = self
            .cipher
            .encrypt(
                &nonce,
                Payload {
                    msg: plain,
                    aad: &aad,
                },
            )
            .map_err(|_| io::Error::other("Could not encrypt a Vault record"))?;
        let mut out = Vec::with_capacity(MAGIC.len() + 1 + NONCE + sealed.len());
        out.extend_from_slice(MAGIC);
        out.push(kind.byte());
        out.extend_from_slice(nonce.as_slice());
        out.extend_from_slice(&sealed);
        Ok(out)
    }

    /// The plaintext of a file [`Self::seal`] made for `kind` and `id`.
    pub fn open(&self, kind: ObjectKind, id: &str, bytes: &[u8]) -> Result<Vec<u8>, Unseal> {
        let body = bytes
            .strip_prefix(MAGIC.as_slice())
            .and_then(|b| b.strip_prefix(&[kind.byte()]))
            .filter(|b| b.len() > NONCE)
            .ok_or(Unseal::NotSealed)?;
        let (nonce, sealed) = body.split_at(NONCE);
        let nonce = <&Nonce<Aes256Gcm>>::try_from(nonce).map_err(|_| Unseal::NotSealed)?;
        let aad = Self::aad(kind, id);
        self.cipher
            .decrypt(
                nonce,
                Payload {
                    msg: sealed,
                    aad: &aad,
                },
            )
            .map_err(|_| Unseal::Rejected)
    }
}

/// Read an open file whole, refusing one over [`MAX_FILE`].
pub(crate) fn read_limited(file: &mut File) -> Result<Vec<u8>, Unseal> {
    let mut bytes = Vec::new();
    file.take(MAX_FILE + 1)
        .read_to_end(&mut bytes)
        .map_err(Unseal::Io)?;
    if bytes.len() as u64 > MAX_FILE {
        return Err(Unseal::NotSealed);
    }
    Ok(bytes)
}

/// A synced temp file holding `bytes` in `path`'s folder, ready to rename
/// over it.
pub(crate) fn temp_beside(path: &Path, bytes: &[u8]) -> io::Result<tempfile::NamedTempFile> {
    let dir = path
        .parent()
        .ok_or_else(|| io::Error::other("A Vault file has no folder"))?;
    std::fs::create_dir_all(dir)?;
    let mut file = tempfile::Builder::new()
        .prefix(TEMP_PREFIX)
        .tempfile_in(dir)?;
    file.write_all(bytes)?;
    file.flush()?;
    file.as_file().sync_all()?;
    Ok(file)
}
