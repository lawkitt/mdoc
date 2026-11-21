//! A password-protected Word 97-2003 file ([MS-DOC] §2.2.6, [MS-OFFCRYPTO]
//! §2.3.5-2.3.7) unlocked in memory into the file an unlocked copy would be,
//! for AnyDoc to convert as it converts any DOC.
//!
//! The password is checked against the file's stored verifier before
//! anything is decrypted. The WordDocument stream (past its first 68 bytes),
//! the Table stream (past the encryption header) and the Data stream are then
//! decrypted where they lie in one copy of the file, and the FIB's
//! `fEncrypted` and `fObfuscated` flags and `lKey` are cleared: no stream
//! changes size, so the compound file around them stays as it was. That copy
//! and every decrypted buffer are wiped when dropped, and nothing is written
//! to disk.
//!
//! The cipher is ported from storytold/craft-libs, `crates/doc-binary/src/
//! crypt.rs` at revision 712b35379b17e442921c03855d4a5c2a2699968e (MIT OR
//! Apache-2.0), written there from the public specifications; craft-doc-binary
//! keeps it private to its reader, which returns a document model rather than
//! the decrypted file (#1007 item 5).

use crate::import::ImportError;
use sha1::{Digest, Sha1};
use zeroize::{Zeroize, Zeroizing};

/// Bytes at the start of the WordDocument stream that are stored untransformed.
const FIB_PLAIN: usize = 68;
/// RC4 re-keys every this many bytes of a stream.
const BLOCK: usize = 512;
/// Longest password considered (the specifications' limit for the RC4 schemes).
const MAX_PASSWORD_CHARS: usize = 255;
/// XOR obfuscation uses at most this many single-byte password characters.
const MAX_XOR_PASSWORD: usize = 15;
/// `FibBase`: `wIdent`, the offset of `flags`, of `lKey`.
const W_IDENT: u16 = 0xA5EC;
const OFF_FLAGS: usize = 0x0A;
const OFF_LKEY: usize = 0x0E;
/// `FibBase.flags`: `fEncrypted`, `fWhichTblStm`, `fObfuscated`.
const F_ENCRYPTED: u16 = 0x0100;
const F_WHICH_TBL_STM: u16 = 0x0200;
const F_OBFUSCATED: u16 = 0x8000;
/// `EncryptionHeaderFlags`: `fCryptoAPI`, `fExternal`, `fAES`.
const H_CRYPTO_API: u32 = 0x04;
const H_EXTERNAL: u32 = 0x10;
const H_AES: u32 = 0x20;
/// Largest stream read, as craft-doc-binary caps it.
const MAX_STREAM: u64 = 1 << 30;

/// The file a password-protected DOC would be unlocked, decrypted in memory
/// with `password`. A scheme no password opens here is `Encrypted`, whether
/// or not a password is given; then no password is `PasswordRequired` and
/// one that fails the verifier `WrongPassword`. A file whose streams can't
/// be read is `Damaged`.
pub(crate) fn decrypt(
    bytes: &[u8],
    password: Option<&str>,
) -> Result<Zeroizing<Vec<u8>>, ImportError> {
    let limits = craft_cfb::Limits {
        max_stream_size: MAX_STREAM,
        ..craft_cfb::Limits::default()
    };
    let file =
        craft_cfb::CompoundFile::open_with(bytes, limits).map_err(|_| ImportError::Damaged)?;
    // Streams as stored: still encrypted, so their copies need no wiping.
    let stream = |name: &str| -> Option<(usize, Vec<u8>)> {
        let id = file.find(name).ok()?;
        let data = file.read_capped(id, MAX_STREAM).ok()?;
        Some((id, data))
    };
    let (word_id, word) = stream("WordDocument").ok_or(ImportError::Damaged)?;
    if word.len() < FIB_PLAIN || u16_at(&word, 0) != Some(W_IDENT) {
        return Err(ImportError::Damaged);
    }
    let flags = u16_at(&word, OFF_FLAGS).ok_or(ImportError::Damaged)?;
    if flags & F_ENCRYPTED == 0 {
        return Ok(Zeroizing::new(bytes.to_vec()));
    }
    let lkey = u32_at(&word, OFF_LKEY).ok_or(ImportError::Damaged)?;
    // The FIB names the Table stream; files from other writers sometimes
    // name the wrong one, so the other is the fallback, as in craft-doc-binary.
    let (named, other) = if flags & F_WHICH_TBL_STM != 0 {
        ("1Table", "0Table")
    } else {
        ("0Table", "1Table")
    };
    let table = stream(named).or_else(|| stream(other));
    let table_bytes = table.as_ref().map_or(&[][..], |(_, data)| data.as_slice());
    // The scheme is read before the password: an empty one, which Word never
    // encrypts with, tells a scheme mdoc can't open from a wrong password.
    let cipher = match password {
        Some(password) => Cipher::open(flags, lkey, table_bytes, password)?,
        None => {
            return Err(match Cipher::open(flags, lkey, table_bytes, "") {
                Err(ImportError::Encrypted) => ImportError::Encrypted,
                Err(ImportError::Damaged) => ImportError::Damaged,
                _ => ImportError::PasswordRequired,
            });
        }
    };
    let layout = Layout::read(bytes).ok_or(ImportError::Damaged)?;
    let mut unlocked = Zeroizing::new(bytes.to_vec());
    let mut word = Zeroizing::new(word);
    cipher.decrypt(&mut word, FIB_PLAIN);
    let cleared = flags & !(F_ENCRYPTED | F_OBFUSCATED);
    word[OFF_FLAGS..OFF_FLAGS + 2].copy_from_slice(&cleared.to_le_bytes());
    word[OFF_LKEY..OFF_LKEY + 4].fill(0);
    layout.put(&file, word_id, &word, &mut unlocked)?;
    if let Some((table_id, table)) = table {
        let mut table = Zeroizing::new(table);
        cipher.decrypt(&mut table, cipher.table_skip());
        layout.put(&file, table_id, &table, &mut unlocked)?;
    }
    if let Some((data_id, data)) = stream("Data") {
        let mut data = Zeroizing::new(data);
        cipher.decrypt(&mut data, 0);
        layout.put(&file, data_id, &data, &mut unlocked)?;
    }
    Ok(unlocked)
}

// -------------------------------------------------------------------------
// Where a stream's bytes lie in the compound file ([MS-CFB] §2)

const ENDOFCHAIN: u32 = 0xFFFF_FFFE;
/// The header's DIFAT holds the first 109 FAT sector numbers.
const HEADER_DIFAT: usize = 109;

/// The sector tables of a compound file, read from its header.
struct Layout {
    sector: usize,
    mini_sector: usize,
    mini_cutoff: u64,
    fat: Vec<u32>,
    mini_fat: Vec<u32>,
}

impl Layout {
    fn read(bytes: &[u8]) -> Option<Layout> {
        let sector = 1usize.checked_shl(u32::from(u16_at(bytes, 0x1E)?))?;
        let mini_sector = 1usize.checked_shl(u32::from(u16_at(bytes, 0x20)?))?;
        if !matches!(sector, 512 | 4096) || mini_sector != 64 {
            return None;
        }
        let per_sector = sector / 4;
        let fat_sectors = u32_at(bytes, 0x2C)? as usize;
        let mini_cutoff = u64::from(u32_at(bytes, 0x38)?);
        let first_mini_fat = u32_at(bytes, 0x3C)?;
        let mut difat_sector = u32_at(bytes, 0x44)?;
        let sectors_in_file = bytes.len() / sector;
        if fat_sectors > sectors_in_file {
            return None;
        }
        // The FAT's own sectors: 109 in the header, the rest in a DIFAT chain.
        let mut fat_ids: Vec<u32> = (0..HEADER_DIFAT.min(fat_sectors))
            .map(|i| u32_at(bytes, 0x4C + 4 * i))
            .collect::<Option<_>>()?;
        let mut hops = 0;
        while fat_ids.len() < fat_sectors && difat_sector != ENDOFCHAIN {
            hops += 1;
            if hops > sectors_in_file {
                return None;
            }
            let at = Self::offset_of(sector, difat_sector)?;
            for i in 0..per_sector - 1 {
                if fat_ids.len() == fat_sectors {
                    break;
                }
                fat_ids.push(u32_at(bytes, at + 4 * i)?);
            }
            difat_sector = u32_at(bytes, at + 4 * (per_sector - 1))?;
        }
        let mut fat = Vec::with_capacity(fat_ids.len() * per_sector);
        for id in fat_ids {
            let at = Self::offset_of(sector, id)?;
            for i in 0..per_sector {
                fat.push(u32_at(bytes, at + 4 * i)?);
            }
        }
        let mut layout = Layout {
            sector,
            mini_sector,
            mini_cutoff,
            fat,
            mini_fat: Vec::new(),
        };
        let mut mini_fat = Vec::new();
        if first_mini_fat != ENDOFCHAIN {
            for id in layout.chain(first_mini_fat)? {
                let at = Self::offset_of(sector, id)?;
                for i in 0..per_sector {
                    mini_fat.push(u32_at(bytes, at + 4 * i)?);
                }
            }
        }
        layout.mini_fat = mini_fat;
        Some(layout)
    }

    fn offset_of(sector: usize, id: u32) -> Option<usize> {
        (id as usize).checked_add(1)?.checked_mul(sector)
    }

    /// A chain of `table` from `start`, refused when it loops or leaves it.
    fn chain_in(table: &[u32], start: u32) -> Option<Vec<u32>> {
        let mut ids = Vec::new();
        let mut cur = start;
        while cur != ENDOFCHAIN {
            if ids.len() >= table.len() {
                return None;
            }
            ids.push(cur);
            cur = *table.get(cur as usize)?;
        }
        Some(ids)
    }

    fn chain(&self, start: u32) -> Option<Vec<u32>> {
        Self::chain_in(&self.fat, start)
    }

    /// The file ranges holding stream `id`'s bytes, in order.
    fn ranges(&self, file: &craft_cfb::CompoundFile<'_>, id: usize) -> Option<Vec<(usize, usize)>> {
        let entry = file.entry(id)?;
        let size = usize::try_from(entry.size).ok()?;
        let mut ranges = Vec::new();
        let mut left = size;
        if entry.size < self.mini_cutoff {
            let mini_stream = self.chain(file.root().start)?;
            for mini in Self::chain_in(&self.mini_fat, entry.start)? {
                if left == 0 {
                    break;
                }
                let at = (mini as usize).checked_mul(self.mini_sector)?;
                let host = *mini_stream.get(at / self.sector)?;
                let offset = Self::offset_of(self.sector, host)?.checked_add(at % self.sector)?;
                let len = left.min(self.mini_sector);
                ranges.push((offset, len));
                left -= len;
            }
        } else {
            for sector in self.chain(entry.start)? {
                if left == 0 {
                    break;
                }
                let len = left.min(self.sector);
                ranges.push((Self::offset_of(self.sector, sector)?, len));
                left -= len;
            }
        }
        (left == 0).then_some(ranges)
    }

    /// Writes `data`, stream `id`'s new bytes of its unchanged size, where
    /// the stream lies in `file_bytes`.
    fn put(
        &self,
        file: &craft_cfb::CompoundFile<'_>,
        id: usize,
        data: &[u8],
        file_bytes: &mut [u8],
    ) -> Result<(), ImportError> {
        let ranges = self.ranges(file, id).ok_or(ImportError::Damaged)?;
        let mut at = 0;
        for (offset, len) in ranges {
            let source = data.get(at..at + len).ok_or(ImportError::Damaged)?;
            file_bytes
                .get_mut(offset..offset + len)
                .ok_or(ImportError::Damaged)?
                .copy_from_slice(source);
            at += len;
        }
        if at == data.len() {
            Ok(())
        } else {
            Err(ImportError::Damaged)
        }
    }
}

// -------------------------------------------------------------------------
// Primitives

/// MD5 ([RFC 1321]); the RC4 scheme is defined on it.
fn md5(data: &[u8]) -> [u8; 16] {
    const S: [u32; 64] = [
        7, 12, 17, 22, 7, 12, 17, 22, 7, 12, 17, 22, 7, 12, 17, 22, 5, 9, 14, 20, 5, 9, 14, 20, 5,
        9, 14, 20, 5, 9, 14, 20, 4, 11, 16, 23, 4, 11, 16, 23, 4, 11, 16, 23, 4, 11, 16, 23, 6, 10,
        15, 21, 6, 10, 15, 21, 6, 10, 15, 21, 6, 10, 15, 21,
    ];
    // K[i] = floor(2^32 * abs(sin(i + 1))), as the RFC defines it.
    let k: Vec<u32> = (0..64u32)
        .map(|i| (f64::from(i + 1).sin().abs() * 4_294_967_296.0) as u32)
        .collect();
    let (mut a0, mut b0, mut c0, mut d0) = (
        0x6745_2301u32,
        0xefcd_ab89u32,
        0x98ba_dcfeu32,
        0x1032_5476u32,
    );
    // It holds the password's hash input: wiped when dropped.
    let mut msg = Zeroizing::new(Vec::with_capacity(data.len() + 72));
    msg.extend_from_slice(data);
    msg.push(0x80);
    while msg.len() % 64 != 56 {
        msg.push(0);
    }
    msg.extend_from_slice(&(data.len() as u64).wrapping_mul(8).to_le_bytes());
    for chunk in msg.as_chunks::<64>().0 {
        let mut m = [0u32; 16];
        for (w, b) in m.iter_mut().zip(chunk.as_chunks::<4>().0) {
            *w = u32::from_le_bytes(*b);
        }
        let (mut a, mut b, mut c, mut d) = (a0, b0, c0, d0);
        for i in 0..64usize {
            let (f, g) = match i / 16 {
                0 => ((b & c) | (!b & d), i),
                1 => ((d & b) | (!d & c), (5 * i + 1) % 16),
                2 => (b ^ c ^ d, (3 * i + 5) % 16),
                _ => (c ^ (b | !d), (7 * i) % 16),
            };
            let f = f.wrapping_add(a).wrapping_add(k[i]).wrapping_add(m[g]);
            a = d;
            d = c;
            c = b;
            b = b.wrapping_add(f.rotate_left(S[i]));
        }
        a0 = a0.wrapping_add(a);
        b0 = b0.wrapping_add(b);
        c0 = c0.wrapping_add(c);
        d0 = d0.wrapping_add(d);
    }
    let mut out = [0u8; 16];
    for (o, v) in out.as_chunks_mut::<4>().0.iter_mut().zip([a0, b0, c0, d0]) {
        *o = v.to_le_bytes();
    }
    out
}

/// SHA-1, as [MS-OFFCRYPTO] §2.3.5 specifies for the RC4 CryptoAPI key
/// derivation: part of the file format, not a choice made here.
fn sha1(parts: &[&[u8]]) -> Vec<u8> {
    let mut hash = Sha1::new();
    for part in parts {
        hash.update(part);
    }
    hash.finalize().to_vec()
}

/// The RC4 stream cipher.
struct Rc4 {
    s: [u8; 256],
    i: u8,
    j: u8,
}

impl Rc4 {
    fn new(key: &[u8]) -> Rc4 {
        let mut s = [0u8; 256];
        for (n, v) in s.iter_mut().enumerate() {
            *v = n as u8;
        }
        let kl = key.len().max(1);
        let mut j = 0u8;
        for i in 0..256usize {
            let k = key.get(i % kl).copied().unwrap_or(0);
            j = j.wrapping_add(s[i]).wrapping_add(k);
            s.swap(i, usize::from(j));
        }
        Rc4 { s, i: 0, j: 0 }
    }

    fn next(&mut self) -> u8 {
        self.i = self.i.wrapping_add(1);
        self.j = self.j.wrapping_add(self.s[usize::from(self.i)]);
        self.s.swap(usize::from(self.i), usize::from(self.j));
        self.s[usize::from(self.s[usize::from(self.i)].wrapping_add(self.s[usize::from(self.j)]))]
    }

    fn apply(&mut self, data: &mut [u8]) {
        for b in data {
            *b ^= self.next();
        }
    }
}

/// The key schedule is derived from the password: wiped when dropped.
impl Drop for Rc4 {
    fn drop(&mut self) {
        self.s.zeroize();
    }
}

fn utf16le(password: &str) -> Zeroizing<Vec<u8>> {
    Zeroizing::new(
        password
            .encode_utf16()
            .take(MAX_PASSWORD_CHARS)
            .flat_map(u16::to_le_bytes)
            .collect(),
    )
}

fn u16_at(d: &[u8], at: usize) -> Option<u16> {
    Some(u16::from_le_bytes(
        d.get(at..at.checked_add(2)?)?.try_into().ok()?,
    ))
}

fn u32_at(d: &[u8], at: usize) -> Option<u32> {
    Some(u32::from_le_bytes(
        d.get(at..at.checked_add(4)?)?.try_into().ok()?,
    ))
}

// -------------------------------------------------------------------------
// XOR obfuscation (method 1 key and verifier, method 2 array and transform)

const PAD_ARRAY: [u8; 15] = [
    0xBB, 0xFF, 0xFF, 0xBA, 0xFF, 0xFF, 0xB9, 0x80, 0x00, 0xBE, 0x0F, 0x00, 0xBF, 0x0F, 0x00,
];
const INITIAL_CODE: [u16; 15] = [
    0xE1F0, 0x1D0F, 0xCC9C, 0x84C0, 0x110C, 0x0E10, 0xF1CE, 0x313E, 0x1872, 0xE139, 0xD40F, 0x84F9,
    0x280C, 0xA96A, 0x4EC3,
];
const XOR_MATRIX: [u16; 105] = [
    0xAEFC, 0x4DD9, 0x9BB2, 0x2745, 0x4E8A, 0x9D14, 0x2A09, 0x7B61, 0xF6C2, 0xFDA5, 0xEB6B, 0xC6F7,
    0x9DCF, 0x2BBF, 0x4563, 0x8AC6, 0x05AD, 0x0B5A, 0x16B4, 0x2D68, 0x5AD0, 0x0375, 0x06EA, 0x0DD4,
    0x1BA8, 0x3750, 0x6EA0, 0xDD40, 0xD849, 0xA0B3, 0x5147, 0xA28E, 0x553D, 0xAA7A, 0x44D5, 0x6F45,
    0xDE8A, 0xAD35, 0x4A4B, 0x9496, 0x390D, 0x721A, 0xEB23, 0xC667, 0x9CEF, 0x29FF, 0x53FE, 0xA7FC,
    0x5FD9, 0x47D3, 0x8FA6, 0x0F6D, 0x1EDA, 0x3DB4, 0x7B68, 0xF6D0, 0xB861, 0x60E3, 0xC1C6, 0x93AD,
    0x377B, 0x6EF6, 0xDDEC, 0x45A0, 0x8B40, 0x06A1, 0x0D42, 0x1A84, 0x3508, 0x6A10, 0xAA51, 0x4483,
    0x8906, 0x022D, 0x045A, 0x08B4, 0x1168, 0x76B4, 0xED68, 0xCAF1, 0x85C3, 0x1BA7, 0x374E, 0x6E9C,
    0x3730, 0x6E60, 0xDCC0, 0xA9A1, 0x4363, 0x86C6, 0x1DAD, 0x3331, 0x6662, 0xCCC4, 0x89A9, 0x0373,
    0x06E6, 0x0DCC, 0x1021, 0x2042, 0x4084, 0x8108, 0x1231, 0x2462, 0x48C4,
];

/// `CreateXorKey_Method1`. `pw` is 1 to 15 bytes.
fn xor_key(pw: &[u8]) -> u16 {
    let mut key = INITIAL_CODE
        .get(pw.len().wrapping_sub(1))
        .copied()
        .unwrap_or(0);
    let mut current = 0x68i32;
    for &ch in pw.iter().rev() {
        let mut ch = ch;
        for _ in 0..7 {
            if ch & 0x40 != 0
                && let Some(m) = usize::try_from(current)
                    .ok()
                    .and_then(|c| XOR_MATRIX.get(c))
            {
                key ^= m;
            }
            ch <<= 1;
            current -= 1;
        }
    }
    key
}

/// `CreatePasswordVerifier_Method1`.
fn verifier_method1(pw: &[u8]) -> u16 {
    let mut v = 0u16;
    for &b in pw.iter().rev().chain(std::iter::once(&(pw.len() as u8))) {
        let i1 = u16::from(v & 0x4000 != 0);
        let i2 = v.wrapping_mul(2) & 0x7FFF;
        v = (i1 | i2) ^ u16::from(b);
    }
    v ^ 0xCE4B
}

/// `CreatePasswordVerifier_Method2`: the 32-bit value stored in `FibBase.lKey`.
fn xor_verifier(pw: &[u8]) -> u32 {
    (u32::from(xor_key(pw)) << 16) | u32::from(verifier_method1(pw))
}

/// `CreateXorArray_Method2`.
fn xor_array(pw: &[u8]) -> [u8; 16] {
    let key = (xor_verifier(pw) >> 16) as u16;
    let (high, low) = ((key >> 8) as u8, key as u8);
    let mut arr = [0u8; 16];
    for (i, slot) in arr.iter_mut().enumerate() {
        *slot = pw.get(i).copied().unwrap_or_else(|| {
            PAD_ARRAY
                .get(i.saturating_sub(pw.len()))
                .copied()
                .unwrap_or(0)
        });
    }
    for pair in arr.as_chunks_mut::<2>().0 {
        pair[0] = (pair[0] ^ low).rotate_right(1);
        pair[1] = (pair[1] ^ high).rotate_right(1);
    }
    arr
}

/// The single-byte forms of a password the XOR scheme tries: Latin-1, and
/// "low byte, or high byte when the low one is zero". Each is cut to 15.
fn xor_candidates(password: &str) -> Vec<Vec<u8>> {
    let mut out: Vec<Vec<u8>> = Vec::new();
    let latin: Option<Vec<u8>> = password
        .chars()
        .take(MAX_XOR_PASSWORD)
        .map(|c| u8::try_from(u32::from(c)).ok())
        .collect();
    if let Some(latin) = latin {
        out.push(latin);
    }
    let folded: Vec<u8> = password
        .encode_utf16()
        .take(MAX_XOR_PASSWORD)
        .map(|u| {
            if u & 0xFF == 0 {
                (u >> 8) as u8
            } else {
                u as u8
            }
        })
        .collect();
    if !out.contains(&folded) {
        out.push(folded);
    }
    out.retain(|p| !p.is_empty());
    out
}

// -------------------------------------------------------------------------
// The cipher

/// How the streams of one file are transformed, once the password is verified.
enum Cipher {
    Xor {
        array: [u8; 16],
    },
    Rc4 {
        base: Zeroizing<Vec<u8>>,
        crypto_api: bool,
        key_len: usize,
        table_skip: usize,
    },
}

impl Cipher {
    /// Checks `password` against the file's verifier. `flags` and `lkey`
    /// come from the FIB base, `table` is the still encrypted Table stream.
    fn open(flags: u16, lkey: u32, table: &[u8], password: &str) -> Result<Cipher, ImportError> {
        if flags & F_OBFUSCATED != 0 {
            for pw in xor_candidates(password) {
                if xor_verifier(&pw) == lkey {
                    return Ok(Cipher::Xor {
                        array: xor_array(&pw),
                    });
                }
            }
            return Err(ImportError::WrongPassword);
        }
        let major = u16_at(table, 0).ok_or(ImportError::Damaged)?;
        let minor = u16_at(table, 2).ok_or(ImportError::Damaged)?;
        match (major, minor) {
            (1, 1) => Self::open_rc4(table, password),
            (2..=4, 2) => Self::open_crypto_api(table, password),
            _ => Err(ImportError::Encrypted),
        }
    }

    fn open_rc4(table: &[u8], password: &str) -> Result<Cipher, ImportError> {
        let salt = table.get(4..20).ok_or(ImportError::Damaged)?;
        let enc_verifier = table.get(20..36).ok_or(ImportError::Damaged)?;
        let enc_hash = table.get(36..52).ok_or(ImportError::Damaged)?;
        let h0 = md5(&utf16le(password));
        let mut buf = Zeroizing::new(Vec::with_capacity(336));
        for _ in 0..16 {
            buf.extend_from_slice(&h0[..5]);
            buf.extend_from_slice(salt);
        }
        let base = Zeroizing::new(md5(&buf)[..5].to_vec());
        let cipher = Cipher::Rc4 {
            base,
            crypto_api: false,
            key_len: 16,
            table_skip: 52,
        };
        let mut rc4 = Rc4::new(&cipher.block_key(0));
        let mut verifier = enc_verifier.to_vec();
        let mut hash = enc_hash.to_vec();
        rc4.apply(&mut verifier);
        rc4.apply(&mut hash);
        if md5(&verifier)[..] != hash[..] {
            return Err(ImportError::WrongPassword);
        }
        Ok(cipher)
    }

    fn open_crypto_api(table: &[u8], password: &str) -> Result<Cipher, ImportError> {
        let hflags = u32_at(table, 4).ok_or(ImportError::Damaged)?;
        // An external provider, or AES, which [MS-OFFCRYPTO] defines only for
        // the newer encrypted packages: no public scheme to follow.
        if hflags & (H_EXTERNAL | H_AES) != 0 || hflags & H_CRYPTO_API == 0 {
            return Err(ImportError::Encrypted);
        }
        let hsize =
            usize::try_from(u32_at(table, 8).ok_or(ImportError::Damaged)?).unwrap_or(usize::MAX);
        // EncryptionHeader: flags, sizeExtra, algId, algIdHash, keySize,
        // providerType, 2 reserved words and the CSP name, from byte 12.
        if hsize < 32 || hsize > table.len() {
            return Err(ImportError::Damaged);
        }
        let alg_id = u32_at(table, 20).ok_or(ImportError::Damaged)?;
        let alg_hash = u32_at(table, 24).ok_or(ImportError::Damaged)?;
        if !matches!(alg_id, 0 | 0x6801) || !matches!(alg_hash, 0 | 0x8004) {
            return Err(ImportError::Encrypted);
        }
        let mut bits = u32_at(table, 28).ok_or(ImportError::Damaged)?;
        if bits == 0 {
            bits = 40;
        }
        if !(40..=128).contains(&bits) || bits % 8 != 0 {
            return Err(ImportError::Encrypted);
        }
        let key_len = (bits / 8) as usize;
        // EncryptionVerifier: saltSize, salt[16], encryptedVerifier[16],
        // verifierHashSize, encryptedVerifierHash[20].
        let v = 12 + hsize;
        if u32_at(table, v) != Some(16) {
            return Err(ImportError::Damaged);
        }
        let salt = table.get(v + 4..v + 20).ok_or(ImportError::Damaged)?;
        let enc_verifier = table.get(v + 20..v + 36).ok_or(ImportError::Damaged)?;
        if u32_at(table, v + 36) != Some(20) {
            return Err(ImportError::Encrypted);
        }
        let enc_hash = table.get(v + 40..v + 60).ok_or(ImportError::Damaged)?;
        let base = Zeroizing::new(sha1(&[salt, &utf16le(password)]));
        let cipher = Cipher::Rc4 {
            base,
            crypto_api: true,
            key_len,
            table_skip: v + 60,
        };
        let mut rc4 = Rc4::new(&cipher.block_key(0));
        let mut verifier = enc_verifier.to_vec();
        let mut hash = enc_hash.to_vec();
        rc4.apply(&mut verifier);
        rc4.apply(&mut hash);
        if sha1(&[&verifier]) != hash {
            return Err(ImportError::WrongPassword);
        }
        Ok(cipher)
    }

    /// The RC4 key of stream block `block`.
    fn block_key(&self, block: u32) -> Zeroizing<Vec<u8>> {
        Zeroizing::new(match self {
            Cipher::Xor { .. } => Vec::new(),
            Cipher::Rc4 {
                base,
                crypto_api: false,
                ..
            } => md5(&[base.as_slice(), &block.to_le_bytes()].concat()).to_vec(),
            Cipher::Rc4 {
                base,
                crypto_api: true,
                key_len,
                ..
            } => {
                let mut k = sha1(&[base, &block.to_le_bytes()]);
                k.truncate(*key_len);
                // A 40-bit key is zero-padded to 128 bits.
                if *key_len == 5 {
                    k.resize(16, 0);
                }
                k
            }
        })
    }

    /// Where the Table stream's transformed bytes start.
    fn table_skip(&self) -> usize {
        match self {
            Cipher::Xor { .. } => 0,
            Cipher::Rc4 { table_skip, .. } => *table_skip,
        }
    }

    /// Decrypts `data` in place; its first `skip` bytes are untransformed.
    fn decrypt(&self, data: &mut [u8], skip: usize) {
        match self {
            Cipher::Xor { array } => {
                for (i, b) in data.iter_mut().enumerate().skip(skip) {
                    let a = array[i % 16];
                    // Zero bytes, and bytes equal to the array element, were
                    // left as they were.
                    if *b != 0 && *b != a {
                        *b ^= a;
                    }
                }
            }
            Cipher::Rc4 { .. } => {
                for (n, chunk) in data.chunks_mut(BLOCK).enumerate() {
                    let Ok(block) = u32::try_from(n) else {
                        return;
                    };
                    let mut rc4 = Rc4::new(&self.block_key(block));
                    let start = n * BLOCK;
                    // The keystream runs over the untransformed bytes too.
                    let plain = skip.saturating_sub(start).min(chunk.len());
                    let head = chunk[..plain].to_vec();
                    rc4.apply(chunk);
                    chunk[..plain].copy_from_slice(&head);
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn hex(b: &[u8]) -> String {
        b.iter().map(|x| format!("{x:02x}")).collect()
    }

    #[test]
    fn md5_matches_the_rfc_1321_vectors() {
        assert_eq!(hex(&md5(b"")), "d41d8cd98f00b204e9800998ecf8427e");
        assert_eq!(hex(&md5(b"abc")), "900150983cd24fb0d6963f7d28e17f72");
        assert_eq!(
            hex(&md5(
                b"12345678901234567890123456789012345678901234567890123456789012345678901234567890"
            )),
            "57edf4a22be3c955ac49da2e2107b67a"
        );
    }

    /// Word's RC4 CryptoAPI and LibreOffice's RC4: the file is decrypted
    /// where it lies, so it keeps its size, and no longer reads as locked;
    /// no password and a wrong one decrypt nothing.
    #[test]
    fn a_locked_doc_is_decrypted_in_place() {
        for app in ["word", "libreoffice"] {
            let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join(format!(
                "../../tests/fixtures/locked-office/{app}-locked.doc"
            ));
            let locked = std::fs::read(path).unwrap();
            assert!(craft_doc_binary::is_encrypted(&locked), "{app}");
            assert!(
                matches!(decrypt(&locked, None), Err(ImportError::PasswordRequired)),
                "{app}"
            );
            assert!(
                matches!(
                    decrypt(&locked, Some("petrov-2026")),
                    Err(ImportError::WrongPassword)
                ),
                "{app}"
            );
            let unlocked = decrypt(&locked, Some("Petrov-2026")).unwrap();
            assert_eq!(unlocked.len(), locked.len(), "{app}");
            assert_ne!(*unlocked, locked, "{app}");
            assert!(!craft_doc_binary::is_encrypted(&unlocked), "{app}");
        }
    }

    /// A DOC that isn't encrypted comes back as it was.
    #[test]
    fn an_unlocked_doc_is_returned_unchanged() {
        let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../../tests/fixtures/locked-office/word-open.doc");
        let bytes = std::fs::read(path).unwrap();
        assert_eq!(*decrypt(&bytes, Some("anything")).unwrap(), bytes);
    }
}
