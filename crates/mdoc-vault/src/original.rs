//! Source Originals (#1091 Decisions 14 and 23, ADR 0125): the format of
//! `docs/<id>/original.enc`, written and read a chunk at a time.
//!
//! ```text
//! header  the file's random 256-bit key, sealed as a record of kind
//!         Original for the document id (seal.rs): 69 bytes
//! chunks  1 MiB of the source (the last 0 ..= 1 MiB) and a 16-byte tag,
//!         under the file key; nonce 0u32 | i as u64, authenticated data
//!         i as u64 | the final-chunk flag, both big-endian
//! ```
//!
//! The file key is unique, so a counter nonce never repeats under it. The
//! flag stops truncation, the counter stops reordering, and the document
//! id bound into the header stops a file opening under another document.
//! An empty source is one empty final chunk. The stream is the source's
//! bytes only; what else is known of it is in the Document record.
use crate::{
    ObjectKind, VaultError,
    seal::{MAGIC, NONCE, Sealer, Unseal},
};
use aes_gcm::{
    Aes256Gcm, KeyInit,
    aead::{AeadInOut, Generate, Nonce, Tag},
};
use sha2::{Digest, Sha256};
use std::{
    fs::File,
    io::{self, Read, Write},
    path::Path,
};

/// Source bytes per chunk.
pub(crate) const CHUNK: usize = 1 << 20;
const TAG: usize = 16;
const KEY: usize = 32;
/// The sealed file key at the head of the file.
pub(crate) const HEADER: usize = MAGIC.len() + 1 + NONCE + KEY + TAG;
/// The temp file an import streams into. Not the record writes' prefix,
/// which an opening handle sweeps while an import may still be running.
pub(crate) const IMPORT_PREFIX: &str = ".import-";

fn nonce(counter: u64) -> Nonce<Aes256Gcm> {
    let mut nonce = [0; NONCE];
    nonce[NONCE - 8..].copy_from_slice(&counter.to_be_bytes());
    nonce.into()
}

fn aad(counter: u64, last: bool) -> [u8; 9] {
    let mut aad = [0; 9];
    aad[..8].copy_from_slice(&counter.to_be_bytes());
    aad[8] = u8::from(last);
    aad
}

fn hex(bytes: &[u8]) -> String {
    bytes.iter().map(|b| format!("{b:02x}")).collect()
}

/// Read from `source` until `buf` is full or the source ends; how many
/// bytes were read.
fn fill(source: &mut impl Read, buf: &mut [u8]) -> io::Result<usize> {
    let mut filled = 0;
    while filled < buf.len() {
        match source.read(&mut buf[filled..]) {
            Ok(0) => break,
            Ok(n) => filled += n,
            Err(e) if e.kind() == io::ErrorKind::Interrupted => {}
            Err(e) => return Err(e),
        }
    }
    Ok(filled)
}

/// The SHA-256, in lowercase hex, of everything `source` yields: what a
/// Document records of its original, so an import can look for it first.
pub fn source_sha256(mut source: impl Read) -> io::Result<String> {
    let mut hasher = Sha256::new();
    let mut buf = vec![0; 64 * 1024];
    loop {
        let n = fill(&mut source, &mut buf)?;
        hasher.update(&buf[..n]);
        if n < buf.len() {
            return Ok(hex(&hasher.finalize()));
        }
    }
}

/// What sealing a source found of it.
pub(crate) struct Sealed {
    /// Its SHA-256 in lowercase hex.
    pub sha256: String,
    pub len: u64,
}

/// Seal `source` as document `id`'s original into `out`, one chunk at a
/// time, hashing it on the way.
pub(crate) fn seal(
    sealer: &Sealer,
    id: &str,
    mut source: impl Read,
    out: &mut impl Write,
) -> io::Result<Sealed> {
    let key =
        <[u8; KEY]>::try_generate().map_err(|_| io::Error::other("Could not generate a key"))?;
    out.write_all(&sealer.seal(ObjectKind::Original, id, &key)?)?;
    let cipher = Aes256Gcm::new(&key.into());
    let mut hasher = Sha256::new();
    // A chunk and the byte after it, which tells whether it is the last.
    let mut buf = vec![0; CHUNK + 1];
    let (mut have, mut len, mut counter) = (0, 0u64, 0u64);
    loop {
        have += fill(&mut source, &mut buf[have..])?;
        let last = have <= CHUNK;
        let chunk = &mut buf[..have.min(CHUNK)];
        hasher.update(&*chunk);
        len += chunk.len() as u64;
        let tag = cipher
            .encrypt_inout_detached(&nonce(counter), &aad(counter, last), (&mut *chunk).into())
            .map_err(|_| io::Error::other("Could not encrypt an original"))?;
        out.write_all(chunk)?;
        out.write_all(&tag)?;
        if last {
            return Ok(Sealed {
                sha256: hex(&hasher.finalize()),
                len,
            });
        }
        buf[0] = buf[CHUNK];
        have = 1;
        counter += 1;
    }
}

/// A Source Original's bytes, read from its sealed file one chunk at a
/// time, in bounded memory.
///
/// Each chunk is authenticated before any of it is returned, but damage or
/// truncation is found only when the read reaches it: after an error,
/// discard everything read so far. A damaged file's error has the kind
/// [`io::ErrorKind::InvalidData`] and wraps [`VaultError::Damaged`]; every
/// read after an error fails too.
pub struct OriginalReader {
    file: File,
    id: String,
    cipher: Aes256Gcm,
    counter: u64,
    /// A sealed chunk and the byte after it.
    buf: Vec<u8>,
    /// The byte after the last chunk read: the next chunk's first.
    carry: Option<u8>,
    /// The current chunk's plaintext is `buf[pos..end]`.
    pos: usize,
    end: usize,
    /// The final chunk was read.
    done: bool,
    /// The kind of the error that stopped the read.
    failed: Option<io::ErrorKind>,
}

impl OriginalReader {
    /// Open `path` as document `id`'s original: its header must unseal.
    pub(crate) fn open(sealer: &Sealer, id: &str, path: &Path) -> Result<Self, VaultError> {
        let damaged = || VaultError::Damaged {
            kind: ObjectKind::Original,
            id: id.to_string(),
        };
        let mut file = match File::open(path) {
            Ok(file) => file,
            Err(e) if e.kind() == io::ErrorKind::NotFound => {
                return Err(VaultError::NotFound {
                    kind: ObjectKind::Original,
                    id: id.to_string(),
                });
            }
            Err(e) => return Err(e.into()),
        };
        let mut header = [0; HEADER];
        if fill(&mut file, &mut header)? < HEADER {
            return Err(damaged());
        }
        let key = match sealer.open(ObjectKind::Original, id, &header) {
            Ok(key) => key,
            Err(Unseal::Io(e)) => return Err(e.into()),
            Err(Unseal::NotSealed | Unseal::Rejected) => return Err(damaged()),
        };
        let key = <[u8; KEY]>::try_from(key).map_err(|_| damaged())?;
        Ok(Self {
            file,
            id: id.to_string(),
            cipher: Aes256Gcm::new(&key.into()),
            counter: 0,
            buf: vec![0; CHUNK + TAG + 1],
            carry: None,
            pos: 0,
            end: 0,
            done: false,
            failed: None,
        })
    }

    fn damaged(&self) -> io::Error {
        io::Error::new(
            io::ErrorKind::InvalidData,
            VaultError::Damaged {
                kind: ObjectKind::Original,
                id: self.id.clone(),
            },
        )
    }

    /// Read and open the next chunk into `buf[..end]`.
    fn next_chunk(&mut self) -> io::Result<()> {
        let mut have = 0;
        if let Some(byte) = self.carry.take() {
            self.buf[0] = byte;
            have = 1;
        }
        have += fill(&mut self.file, &mut self.buf[have..])?;
        let sealed = CHUNK + TAG;
        // A full chunk with more after it is not the last; whatever ends
        // the file is, and its flag must say so.
        let last = have <= sealed;
        let n = have.min(sealed);
        if n < TAG {
            return Err(self.damaged());
        }
        let (body, tag) = self.buf[..n].split_at_mut(n - TAG);
        let opened = <&Tag<Aes256Gcm>>::try_from(&*tag).is_ok_and(|tag| {
            (self.cipher)
                .decrypt_inout_detached(
                    &nonce(self.counter),
                    &aad(self.counter, last),
                    body.into(),
                    tag,
                )
                .is_ok()
        });
        if !opened {
            return Err(self.damaged());
        }
        (self.pos, self.end) = (0, n - TAG);
        if last {
            self.done = true;
        } else {
            self.carry = Some(self.buf[sealed]);
            self.counter += 1;
        }
        Ok(())
    }
}

impl Read for OriginalReader {
    fn read(&mut self, out: &mut [u8]) -> io::Result<usize> {
        if let Some(kind) = self.failed {
            return Err(io::Error::new(
                kind,
                "An earlier read of the original failed",
            ));
        }
        while self.pos == self.end {
            if self.done || out.is_empty() {
                return Ok(0);
            }
            if let Err(e) = self.next_chunk() {
                self.failed = Some(e.kind());
                return Err(e);
            }
        }
        let n = out.len().min(self.end - self.pos);
        out[..n].copy_from_slice(&self.buf[self.pos..self.pos + n]);
        self.pos += n;
        Ok(n)
    }
}

/// The [`VaultError`] an [`OriginalReader`]'s error stands for.
pub(crate) fn read_error(error: io::Error) -> VaultError {
    let damaged = (error.get_ref())
        .and_then(|inner| inner.downcast_ref::<VaultError>())
        .and_then(|inner| match inner {
            VaultError::Damaged { kind, id } => Some(VaultError::Damaged {
                kind: *kind,
                id: id.clone(),
            }),
            _ => None,
        });
    damaged.unwrap_or(VaultError::Io(error))
}
