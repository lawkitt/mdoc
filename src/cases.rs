//! Saved cases (ADR 0041): a named replacement mapping its documents share,
//! stored as one AES-256-GCM file per case in app data. The key is held by the
//! OS (macOS Keychain, Windows data protection); there is no plaintext
//! fallback. Originals never reach the session manifest.
use crate::pii::{Category, restore::ExportedIdentity};
use aes_gcm::{
    Aes256Gcm, KeyInit,
    aead::{Aead, AeadCore, OsRng, Payload, rand_core::RngCore},
};
use serde::{Deserialize, Serialize};
use std::{
    io::{self, Write},
    path::{Path, PathBuf},
    sync::Arc,
};

/// File header: magic and format version, also authenticated with the case id.
const MAGIC: &[u8; 9] = b"MDOCCASE1";
const NONCE: usize = 12;
/// Cases are small; anything larger is not one of ours.
const MAX_FILE: u64 = 64 * 1024 * 1024;

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct StoredIdentity {
    pub alias: String,
    #[serde(default)]
    pub previous: Vec<String>,
    /// The category token, e.g. `PERSON`.
    pub category: String,
    #[serde(default)]
    pub custom: bool,
    /// The primary wording first.
    pub originals: Vec<String>,
}
impl StoredIdentity {
    fn from_exported(e: &ExportedIdentity) -> Self {
        Self {
            alias: e.alias.clone(),
            previous: e.previous.clone(),
            category: e.category.token().into(),
            custom: e.custom,
            originals: e.originals.iter().map(|o| o.to_string()).collect(),
        }
    }
    fn exported(&self) -> Option<ExportedIdentity> {
        let category = *Category::ALL.iter().find(|c| c.token() == self.category)?;
        (!self.originals.is_empty()).then(|| ExportedIdentity {
            alias: self.alias.clone(),
            previous: self.previous.clone(),
            category,
            custom: self.custom,
            originals: self
                .originals
                .iter()
                .map(|o| Arc::from(o.as_str()))
                .collect(),
        })
    }
    pub fn names(&self, alias: &str) -> bool {
        self.alias == alias || self.previous.iter().any(|p| p == alias)
    }
}

/// A document's membership in a case, as the tab shows it.
#[derive(Clone, Debug, PartialEq)]
pub struct CaseLink {
    pub id: String,
    pub name: String,
}
impl CaseLink {
    pub fn of(case: &Case) -> Self {
        Self {
            id: case.id.clone(),
            name: case.name.clone(),
        }
    }
    /// A stable colour for the case's sidebar dot.
    pub fn color(&self) -> gpui::Hsla {
        let hue = u32::from_str_radix(&self.id[..6], 16).unwrap_or(0) % 360;
        gpui::hsla(hue as f32 / 360., 0.55, 0.55, 1.)
    }
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Case {
    pub id: String,
    pub name: String,
    /// Seconds since the Unix epoch.
    pub created: u64,
    pub identities: Vec<StoredIdentity>,
    /// Member Markdown files, as session identities.
    #[serde(default)]
    pub members: Vec<PathBuf>,
    /// Bumped on every change; a store never writes an older revision.
    #[serde(default)]
    pub revision: u64,
    /// Category tokens scans in member documents skip.
    #[serde(default)]
    pub skip: Vec<String>,
}
impl Case {
    pub fn new(name: String, identities: &[ExportedIdentity], member: Option<PathBuf>) -> Self {
        let mut id = [0u8; 16];
        OsRng.fill_bytes(&mut id);
        let mut case = Self {
            id: id.iter().map(|b| format!("{b:02x}")).collect(),
            name,
            created: std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .map_or(0, |d| d.as_secs()),
            identities: Vec::new(),
            members: member.into_iter().collect(),
            revision: 0,
            skip: Vec::new(),
        };
        case.merge(identities);
        case
    }
    /// The next generated alias of `category` that no identity has ever used.
    pub fn next_alias(&self, category: Category) -> String {
        let used = |alias: &str| self.identities.iter().any(|i| i.names(alias));
        (1..)
            .map(|n| format!("{}_{n}", category.token()))
            .find(|alias| !used(alias))
            .unwrap()
    }
    pub fn skipped(&self) -> std::collections::HashSet<Category> {
        Category::ALL
            .into_iter()
            .filter(|c| self.skip.iter().any(|s| s == c.token()))
            .collect()
    }
    pub fn exported_one(&self, index: usize) -> Option<ExportedIdentity> {
        self.identities.get(index)?.exported()
    }
    pub fn exported(&self) -> Vec<ExportedIdentity> {
        self.identities
            .iter()
            .filter_map(StoredIdentity::exported)
            .collect()
    }
    /// Fold a member document's current identities into the case. Identities
    /// the document no longer uses stay; a renamed alias is kept as an earlier
    /// alias, so old files and replies still restore (ADR 0041 #23).
    pub fn merge(&mut self, exported: &[ExportedIdentity]) {
        for e in exported {
            let incoming = StoredIdentity::from_exported(e);
            // Same alias first, so homonyms stay apart; then a shared original,
            // whatever the category (a category change renames the alias).
            let found = self
                .identities
                .iter()
                .position(|s| {
                    s.names(&incoming.alias) || incoming.previous.iter().any(|p| s.names(p))
                })
                .or_else(|| {
                    self.identities
                        .iter()
                        .position(|s| incoming.originals.iter().any(|o| s.originals.contains(o)))
                });
            // An alias moves to one identity only.
            for (i, other) in self.identities.iter_mut().enumerate() {
                if Some(i) != found && other.alias != incoming.alias {
                    other.previous.retain(|p| *p != incoming.alias);
                }
            }
            match found {
                Some(index) => {
                    let stored = &mut self.identities[index];
                    if stored.alias != incoming.alias && !stored.previous.contains(&stored.alias) {
                        let old = std::mem::take(&mut stored.alias);
                        stored.previous.push(old);
                    }
                    stored.alias = incoming.alias.clone();
                    stored.category = incoming.category.clone();
                    for p in incoming.previous {
                        if !stored.previous.contains(&p) {
                            stored.previous.push(p);
                        }
                    }
                    stored.previous.retain(|p| *p != stored.alias);
                    stored.custom = incoming.custom;
                    let mut originals = incoming.originals;
                    for o in std::mem::take(&mut stored.originals) {
                        if !originals.contains(&o) {
                            originals.push(o);
                        }
                    }
                    stored.originals = originals;
                }
                None => self.identities.push(incoming),
            }
        }
    }
}

/// Encrypted case files in one directory. Writes and deletes are serialized:
/// a write older than the last one, or after a delete, is skipped.
pub struct Store {
    dir: PathBuf,
    cipher: Aes256Gcm,
    /// Last written revision per case; `u64::MAX` once deleted.
    written: std::sync::Mutex<std::collections::HashMap<String, u64>>,
}
impl Store {
    pub fn default_dir() -> Option<PathBuf> {
        dirs::data_local_dir().map(|p| p.join("mdoc/cases"))
    }
    /// Open the store, creating the OS-held key on first use.
    pub fn open(dir: PathBuf) -> Result<Self, String> {
        let key = key::load_or_create(&dir)?;
        Ok(Self::with_key(dir, &key))
    }
    pub fn with_key(dir: PathBuf, key: &[u8; 32]) -> Self {
        Self {
            dir,
            cipher: Aes256Gcm::new(key.into()),
            written: Default::default(),
        }
    }
    fn valid_id(id: &str) -> bool {
        id.len() == 32 && id.bytes().all(|b| b.is_ascii_hexdigit())
    }
    fn path(&self, id: &str) -> PathBuf {
        self.dir.join(format!("{id}.case"))
    }
    fn aad(id: &str) -> Vec<u8> {
        [MAGIC.as_slice(), id.as_bytes()].concat()
    }
    pub fn save(&self, case: &Case) -> io::Result<()> {
        if !Self::valid_id(&case.id) {
            return Err(io::Error::other("Invalid case id"));
        }
        let mut written = self.written.lock().unwrap_or_else(|e| e.into_inner());
        if written
            .get(&case.id)
            .is_some_and(|&last| last > case.revision || last == u64::MAX)
        {
            return Ok(());
        }
        std::fs::create_dir_all(&self.dir)?;
        let plain = serde_json::to_vec(case)?;
        let nonce = Aes256Gcm::generate_nonce(&mut OsRng);
        let sealed = self
            .cipher
            .encrypt(
                &nonce,
                Payload {
                    msg: &plain,
                    aad: &Self::aad(&case.id),
                },
            )
            .map_err(|_| io::Error::other("Could not encrypt the case"))?;
        let mut file = tempfile::NamedTempFile::new_in(&self.dir)?;
        file.write_all(MAGIC)?;
        file.write_all(&nonce)?;
        file.write_all(&sealed)?;
        file.flush()?;
        file.as_file().sync_all()?;
        file.persist(self.path(&case.id))
            .map_err(|error| error.error)?;
        written.insert(case.id.clone(), case.revision);
        Ok(())
    }
    pub fn load(&self, id: &str) -> Result<Case, String> {
        if !Self::valid_id(id) {
            return Err("Invalid case id.".into());
        }
        let path = self.path(id);
        let bytes = read_limited(&path).map_err(|e| format!("Could not read the case: {e}"))?;
        let body = bytes
            .strip_prefix(MAGIC.as_slice())
            .filter(|b| b.len() > NONCE)
            .ok_or("The case file is not a case or is damaged.")?;
        let (nonce, sealed) = body.split_at(NONCE);
        let plain = self
            .cipher
            .decrypt(
                nonce.into(),
                Payload {
                    msg: sealed,
                    aad: &Self::aad(id),
                },
            )
            .map_err(|_| "The case file is damaged or was saved with a different key.")?;
        let case: Case =
            serde_json::from_slice(&plain).map_err(|_| "The case file is damaged.".to_string())?;
        if case.id != id {
            return Err("The case file is damaged.".into());
        }
        Ok(case)
    }
    /// Every readable case, and the ids of files that could not be read.
    pub fn list(&self) -> (Vec<Case>, Vec<String>) {
        let mut cases = Vec::new();
        let mut failed = Vec::new();
        let Ok(entries) = std::fs::read_dir(&self.dir) else {
            return (cases, failed);
        };
        for entry in entries.flatten() {
            let path = entry.path();
            if path.extension().is_none_or(|e| e != "case") {
                continue;
            }
            let Some(id) = path.file_stem().and_then(|s| s.to_str()) else {
                continue;
            };
            match self.load(id) {
                Ok(case) => cases.push(case),
                Err(_) => failed.push(id.to_string()),
            }
        }
        cases.sort_by(|a, b| b.created.cmp(&a.created).then(a.name.cmp(&b.name)));
        (cases, failed)
    }
    /// Forget a case: overwrite its file, then remove it. Markdown is untouched.
    pub fn delete(&self, id: &str) -> io::Result<()> {
        if !Self::valid_id(id) {
            return Err(io::Error::other("Invalid case id"));
        }
        // Later writes of this case, even ones already queued, are skipped.
        let mut written = self.written.lock().unwrap_or_else(|e| e.into_inner());
        written.insert(id.to_string(), u64::MAX);
        let path = self.path(id);
        let len = match std::fs::metadata(&path) {
            Ok(meta) => meta.len(),
            Err(e) if e.kind() == io::ErrorKind::NotFound => return Ok(()),
            Err(e) => return Err(e),
        };
        {
            let mut file = std::fs::OpenOptions::new().write(true).open(&path)?;
            file.write_all(&vec![0; len.min(MAX_FILE) as usize])?;
            file.sync_all()?;
        }
        std::fs::remove_file(path)
    }
}

fn read_limited(path: &Path) -> io::Result<Vec<u8>> {
    use std::io::Read;
    let mut bytes = Vec::new();
    std::fs::File::open(path)?
        .take(MAX_FILE + 1)
        .read_to_end(&mut bytes)?;
    if bytes.len() as u64 > MAX_FILE {
        return Err(io::Error::other("file too large"));
    }
    Ok(bytes)
}

/// The 256-bit case key, created once and held by the operating system.
mod key {
    use super::*;

    #[cfg(target_os = "macos")]
    pub fn load_or_create(_: &Path) -> Result<[u8; 32], String> {
        use security_framework::passwords::{get_generic_password, set_generic_password};
        const SERVICE: &str = "io.github.lawkitt.mdoc";
        const ACCOUNT: &str = "case-key";
        match get_generic_password(SERVICE, ACCOUNT) {
            Ok(bytes) => bytes
                .try_into()
                .map_err(|_| "The case key in the Keychain is damaged.".to_string()),
            Err(e) if e.code() == -25300 => {
                // errSecItemNotFound: first use.
                let key = fresh();
                set_generic_password(SERVICE, ACCOUNT, &key)
                    .map_err(|e| format!("Could not store the case key in the Keychain: {e}"))?;
                Ok(key)
            }
            Err(e) => Err(format!(
                "Could not read the case key from the Keychain: {e}"
            )),
        }
    }

    #[cfg(windows)]
    pub fn load_or_create(dir: &Path) -> Result<[u8; 32], String> {
        let path = dir.join("key.dpapi");
        match std::fs::read(&path) {
            Ok(sealed) => dpapi::unprotect(&sealed)?
                .try_into()
                .map_err(|_| "The case key is damaged.".to_string()),
            Err(e) if e.kind() == io::ErrorKind::NotFound => {
                let key = fresh();
                let sealed = dpapi::protect(&key)?;
                std::fs::create_dir_all(dir)
                    .and_then(|_| {
                        let mut file = tempfile::NamedTempFile::new_in(dir)?;
                        file.write_all(&sealed)?;
                        file.as_file().sync_all()?;
                        file.persist(&path).map_err(|e| e.error)?;
                        Ok(())
                    })
                    .map_err(|e| format!("Could not store the case key: {e}"))?;
                Ok(key)
            }
            Err(e) => Err(format!("Could not read the case key: {e}")),
        }
    }

    #[cfg(not(any(target_os = "macos", windows)))]
    pub fn load_or_create(_: &Path) -> Result<[u8; 32], String> {
        Err(
            "Saved cases need the macOS Keychain or Windows data protection, \
             so they are not available on this system."
                .into(),
        )
    }

    #[cfg(any(target_os = "macos", windows))]
    fn fresh() -> [u8; 32] {
        let mut key = [0u8; 32];
        OsRng.fill_bytes(&mut key);
        key
    }

    #[cfg(windows)]
    mod dpapi {
        use windows_sys::Win32::{
            Foundation::LocalFree,
            Security::Cryptography::{
                CRYPT_INTEGER_BLOB, CRYPTPROTECT_UI_FORBIDDEN, CryptProtectData, CryptUnprotectData,
            },
        };
        fn run(input: &[u8], protect: bool) -> Result<Vec<u8>, String> {
            let blob_in = CRYPT_INTEGER_BLOB {
                cbData: input.len() as u32,
                pbData: input.as_ptr() as *mut u8,
            };
            let mut blob_out = CRYPT_INTEGER_BLOB {
                cbData: 0,
                pbData: std::ptr::null_mut(),
            };
            // SAFETY: valid input blob; DPAPI allocates the output with
            // LocalAlloc, which is copied and then freed below.
            let ok = unsafe {
                if protect {
                    CryptProtectData(
                        &blob_in,
                        std::ptr::null(),
                        std::ptr::null(),
                        std::ptr::null(),
                        std::ptr::null(),
                        CRYPTPROTECT_UI_FORBIDDEN,
                        &mut blob_out,
                    )
                } else {
                    CryptUnprotectData(
                        &blob_in,
                        std::ptr::null_mut(),
                        std::ptr::null(),
                        std::ptr::null(),
                        std::ptr::null(),
                        CRYPTPROTECT_UI_FORBIDDEN,
                        &mut blob_out,
                    )
                }
            };
            if ok == 0 || blob_out.pbData.is_null() {
                return Err("Windows data protection could not open the case key.".into());
            }
            // SAFETY: DPAPI returned cbData bytes at pbData.
            let bytes = unsafe {
                std::slice::from_raw_parts(blob_out.pbData, blob_out.cbData as usize).to_vec()
            };
            // SAFETY: the buffer was allocated by DPAPI with LocalAlloc.
            unsafe { LocalFree(blob_out.pbData as _) };
            Ok(bytes)
        }
        pub fn protect(input: &[u8]) -> Result<Vec<u8>, String> {
            run(input, true)
        }
        pub fn unprotect(input: &[u8]) -> Result<Vec<u8>, String> {
            run(input, false)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn identity(alias: &str, category: Category, originals: &[&str]) -> ExportedIdentity {
        ExportedIdentity {
            alias: alias.into(),
            previous: Vec::new(),
            category,
            custom: false,
            originals: originals.iter().map(|o| Arc::from(*o)).collect(),
        }
    }
    fn store(dir: &Path) -> Store {
        Store::with_key(dir.to_path_buf(), &[7; 32])
    }

    #[test]
    fn cases_round_trip_encrypted_and_never_in_plaintext() {
        let dir = tempfile::tempdir().unwrap();
        let store = store(dir.path());
        let case = Case::new(
            "Petrova v Northwind".into(),
            &[identity("PERSON_1", Category::Person, &["Anna Petrova"])],
            Some("/docs/contract.md".into()),
        );
        store.save(&case).unwrap();
        assert_eq!(store.load(&case.id).unwrap(), case);
        let bytes = std::fs::read(dir.path().join(format!("{}.case", case.id))).unwrap();
        let text = String::from_utf8_lossy(&bytes);
        for secret in ["Anna", "Petrova", "PERSON_1", "contract.md"] {
            assert!(
                !text.contains(secret),
                "{secret} must not be stored in plaintext"
            );
        }
        let (listed, failed) = store.list();
        assert_eq!(listed, std::slice::from_ref(&case));
        assert!(failed.is_empty());
        assert_eq!(case.exported()[0].wording(), "Anna Petrova");
    }

    #[test]
    fn tampered_misnamed_or_foreign_key_files_are_rejected() {
        let dir = tempfile::tempdir().unwrap();
        let store = store(dir.path());
        let case = Case::new("A".into(), &[], None);
        store.save(&case).unwrap();
        let path = dir.path().join(format!("{}.case", case.id));
        let mut bytes = std::fs::read(&path).unwrap();
        let last = bytes.len() - 1;
        bytes[last] ^= 1;
        std::fs::write(&path, &bytes).unwrap();
        assert!(store.load(&case.id).is_err(), "tampering");
        store.save(&case).unwrap();
        let other = Store::with_key(dir.path().into(), &[8; 32]);
        assert!(other.load(&case.id).is_err(), "different key");
        // A file renamed to another id fails authentication.
        let renamed = "0".repeat(32);
        std::fs::copy(&path, dir.path().join(format!("{renamed}.case"))).unwrap();
        assert!(store.load(&renamed).is_err());
        assert_eq!(store.list().1, [renamed]);
        assert!(store.load("../escape").is_err());
    }

    #[test]
    fn stale_writes_and_writes_after_delete_are_skipped() {
        let dir = tempfile::tempdir().unwrap();
        let store = store(dir.path());
        let mut case = Case::new("A".into(), &[], None);
        case.revision = 2;
        store.save(&case).unwrap();
        let mut older = case.clone();
        older.revision = 1;
        older.name = "old".into();
        store.save(&older).unwrap();
        assert_eq!(store.load(&case.id).unwrap().name, "A");
        store.delete(&case.id).unwrap();
        case.revision = 3;
        store.save(&case).unwrap();
        assert!(
            store.load(&case.id).is_err(),
            "a queued save cannot resurrect it"
        );
    }

    #[test]
    fn deleting_wipes_the_file() {
        let dir = tempfile::tempdir().unwrap();
        let store = store(dir.path());
        let case = Case::new("A".into(), &[], None);
        store.save(&case).unwrap();
        store.delete(&case.id).unwrap();
        assert!(store.load(&case.id).is_err());
        assert!(store.list().0.is_empty());
        store.delete(&case.id).unwrap();
    }

    #[test]
    fn merging_keeps_unused_identities_and_renamed_aliases() {
        let mut case = Case::new(
            "A".into(),
            &[
                identity("PERSON_1", Category::Person, &["Anna Petrova"]),
                identity("ORG_1", Category::Organization, &["Northwind Ltd"]),
            ],
            None,
        );
        // The document renamed PERSON_1, learned a variant and dropped ORG_1.
        let mut renamed = identity("BUYER", Category::Person, &["Anna Petrova", "A. Petrova"]);
        renamed.custom = true;
        case.merge(&[
            renamed,
            identity("PERSON_2", Category::Person, &["Ivan Ivanov"]),
        ]);
        assert_eq!(case.identities.len(), 3);
        let anna = &case.identities[0];
        assert_eq!(anna.alias, "BUYER");
        assert_eq!(anna.previous, ["PERSON_1"]);
        assert_eq!(anna.originals, ["Anna Petrova", "A. Petrova"]);
        assert_eq!(case.identities[1].alias, "ORG_1");
        assert_eq!(case.identities[2].alias, "PERSON_2");
        // Renaming back moves the alias, never duplicating it.
        case.merge(&[identity("PERSON_1", Category::Person, &["Anna Petrova"])]);
        let anna = &case.identities[0];
        assert_eq!(
            (anna.alias.as_str(), anna.previous.as_slice()),
            ("PERSON_1", &["BUYER".to_string()][..])
        );
    }
}
