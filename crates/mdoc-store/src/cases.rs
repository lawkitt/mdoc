//! Saved cases (ADR 0041): a named replacement mapping its documents share,
//! stored as one AES-256-GCM file per case in app data. The key is held by the
//! OS (macOS Keychain, Windows data protection); there is no plaintext
//! fallback. Originals never reach the session manifest.
use aes_gcm::{
    Aes256Gcm, KeyInit,
    aead::{Aead, Generate, Nonce, Payload},
};
use mdoc_pii::{Category, restore::ExportedIdentity};
use serde::{Deserialize, Serialize};
use std::{
    io::{self, Write},
    path::{Path, PathBuf},
    sync::Arc,
};

#[derive(Debug, thiserror::Error)]
pub enum CaseError {
    /// The user declined the macOS Keychain dialog (ADR 0043).
    #[error("Saved cases need Keychain access.")]
    KeyDenied,
    /// A Keychain call failed; the OS error, as macOS words it.
    #[error("Could not read the case key from the Keychain: {0}")]
    KeychainRead(String),
    #[error("Could not store the case key in the Keychain: {0}")]
    KeychainStore(String),
    #[error("The case key in the Keychain is damaged.")]
    KeychainDamaged,
    /// Windows: the DPAPI-sealed key file.
    #[error("Could not read the case key: {0}")]
    KeyRead(io::Error),
    #[error("Could not store the case key: {0}")]
    KeyWrite(io::Error),
    #[error("The case key is damaged.")]
    KeyDamaged,
    #[error("Windows data protection could not open the case key.")]
    DataProtection,
    /// Linux: no usable Secret Service; its error, as the service words it.
    #[error(
        "Saved cases need a Secret Service keyring (GNOME Keyring or KWallet), which is not available: {0}"
    )]
    KeyringUnavailable(String),
    #[error("Could not store the case key in the keyring: {0}")]
    KeyringStore(String),
    #[error("The case key in the keyring is damaged.")]
    KeyringDamaged,
    #[error(
        "Saved cases need the macOS Keychain, Windows data protection or a Secret Service keyring, so they are not available on this system."
    )]
    NoKeyStore,
    #[error("OS case keys are disabled in headless renders")]
    KeysDisabled,
    #[error("No folder for app data on this system.")]
    NoAppData,
    #[error("Invalid case id.")]
    InvalidId,
    #[error("Could not read the case: {0}")]
    Read(io::Error),
    #[error("The case file is not a case or is damaged.")]
    NotACase,
    #[error("The case file is damaged or was saved with a different key.")]
    WrongKey,
    #[error("The case file is damaged.")]
    Damaged,
}

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

/// The eight case swatches (ADR 0077), in hue order.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum CaseColor {
    Red,
    Orange,
    Amber,
    Green,
    Teal,
    Blue,
    Violet,
    Pink,
}

impl CaseColor {
    pub const ALL: [Self; 8] = [
        Self::Red,
        Self::Orange,
        Self::Amber,
        Self::Green,
        Self::Teal,
        Self::Blue,
        Self::Violet,
        Self::Pink,
    ];

    pub fn name(self) -> &'static str {
        match self {
            Self::Red => "Red",
            Self::Orange => "Orange",
            Self::Amber => "Amber",
            Self::Green => "Green",
            Self::Teal => "Teal",
            Self::Blue => "Blue",
            Self::Violet => "Violet",
            Self::Pink => "Pink",
        }
    }

    pub fn hue(self) -> u32 {
        match self {
            Self::Red => 0,
            Self::Orange => 30,
            Self::Amber => 45,
            Self::Green => 120,
            Self::Teal => 175,
            Self::Blue => 220,
            Self::Violet => 270,
            Self::Pink => 325,
        }
    }

    pub fn for_id(id: &str) -> Self {
        let hue = u32::from_str_radix(id.get(..6).unwrap_or(""), 16).unwrap_or(0) % 360;
        Self::ALL
            .into_iter()
            .min_by_key(|color| {
                let distance = hue.abs_diff(color.hue());
                distance.min(360 - distance)
            })
            .unwrap()
    }
}

/// Non-secret metadata available while cases are locked. No member paths or names.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct CaseSummary {
    pub id: String,
    pub name: String,
    pub documents: usize,
    #[serde(default)]
    pub color: Option<CaseColor>,
}

impl CaseSummary {
    pub fn of(case: &Case) -> Self {
        Self {
            id: case.id.clone(),
            name: case.name.clone(),
            documents: case.members.len(),
            color: case.color,
        }
    }
    pub fn link(&self) -> CaseLink {
        CaseLink {
            id: self.id.clone(),
            name: self.name.clone(),
            color: self.color,
        }
    }
}

/// A document's membership in a case, as the tab shows it.
#[derive(Clone, Debug, PartialEq)]
pub struct CaseLink {
    pub id: String,
    pub name: String,
    pub color: Option<CaseColor>,
}
impl CaseLink {
    pub fn of(case: &Case) -> Self {
        Self {
            id: case.id.clone(),
            name: case.name.clone(),
            color: case.color,
        }
    }
    /// A stable hue in degrees (0..360) for the case's sidebar dot.
    pub fn hue(&self) -> u32 {
        self.color
            .unwrap_or_else(|| CaseColor::for_id(&self.id))
            .hue()
    }
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Case {
    pub id: String,
    pub name: String,
    #[serde(default)]
    pub color: Option<CaseColor>,
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
    /// Each member's AI reply as pasted, aliases and all (ADR 0078).
    #[serde(default)]
    pub replies: Vec<CaseReply>,
    /// Dismissed merge hints, as the identities’ primary wordings (shorter, fuller)
    /// (ADR 0080).
    #[serde(default)]
    pub dismissed_hints: Vec<(String, String)>,
}

/// One member document's AI reply, kept for its Reply view (ADR 0078).
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct CaseReply {
    /// The member, as in [`Case::members`].
    pub member: PathBuf,
    pub text: String,
}
impl Case {
    pub fn new(name: String, identities: &[ExportedIdentity], member: Option<PathBuf>) -> Self {
        let id = <[u8; 16]>::generate();
        let mut case = Self {
            id: id.iter().map(|b| format!("{b:02x}")).collect(),
            name,
            color: None,
            created: std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .map_or(0, |d| d.as_secs()),
            identities: Vec::new(),
            members: member.into_iter().collect(),
            revision: 0,
            skip: Vec::new(),
            replies: Vec::new(),
            dismissed_hints: Vec::new(),
        };
        case.merge(identities);
        case
    }
    /// The AI reply kept for `member`.
    pub fn reply(&self, member: &Path) -> Option<&str> {
        self.replies
            .iter()
            .find(|r| r.member == member)
            .map(|r| r.text.as_str())
    }
    /// Keep `text` as `member`'s reply, or remove it with `None`. Returns
    /// whether the case changed.
    pub fn set_reply(&mut self, member: &Path, text: Option<String>) -> bool {
        let at = self.replies.iter().position(|r| r.member == member);
        match (at, text) {
            (Some(at), None) => {
                self.replies.remove(at);
                true
            }
            (Some(at), Some(text)) if self.replies[at].text != text => {
                self.replies[at].text = text;
                true
            }
            (None, Some(text)) => {
                self.replies.push(CaseReply {
                    member: member.to_path_buf(),
                    text,
                });
                true
            }
            _ => false,
        }
    }
    /// A member renamed from `from` to `to` keeps its reply.
    pub fn move_reply(&mut self, from: &Path, to: &Path) {
        if self.reply(to).is_none()
            && let Some(reply) = self.replies.iter_mut().find(|r| r.member == from)
        {
            reply.member = to.to_path_buf();
        }
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
    /// Alias-only tombstones. Members must reserve these even though their
    /// originals no longer exist and cannot be seeded as live identities.
    pub fn reserved_aliases(&self) -> Vec<String> {
        self.identities
            .iter()
            .filter(|identity| identity.originals.is_empty())
            .flat_map(|identity| {
                std::iter::once(identity.alias.clone()).chain(identity.previous.iter().cloned())
            })
            .collect()
    }
    /// Fold a member document's current identities into the case. Identities
    /// the document no longer uses stay; a renamed alias is kept as an earlier
    /// alias, so old files and replies still restore (ADR 0041 #23).
    pub fn merge(&mut self, exported: &[ExportedIdentity]) {
        for e in exported {
            // A forgotten identity retains only its aliases, reserving them and
            // preventing a stale member's autosave from restoring its originals.
            if self.identities.iter().any(|identity| {
                identity.originals.is_empty()
                    && (identity.names(&e.alias)
                        || e.previous.iter().any(|alias| identity.names(alias)))
            }) {
                continue;
            }
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

    /// Irreversibly discard originals, retaining alias reservations only.
    pub fn forget_identity(&mut self, alias: &str) -> bool {
        let Some(identity) = self
            .identities
            .iter_mut()
            .find(|identity| identity.names(alias))
        else {
            return false;
        };
        if identity.originals.is_empty() {
            return false;
        }
        self.dismissed_hints.retain(|(from, into)| {
            !identity.originals.contains(from) && !identity.originals.contains(into)
        });
        identity.originals.clear();
        true
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
        crate::app_paths::data_local_dir().map(|p| p.join("mdoc").join(key::folder()))
    }
    /// Open the store without any OS dialog: `None` when the key can only be
    /// read after the macOS password dialog (ADR 0043).
    pub fn open_quietly(dir: PathBuf) -> Result<Option<Self>, CaseError> {
        if crate::app_paths::isolated() {
            return Err(CaseError::KeysDisabled);
        }
        Ok(key::load_quietly(&dir)?.map(|key| Self::with_key(dir, &key)))
    }
    /// Open the store, creating the OS-held key on first use.
    pub fn open(dir: PathBuf) -> Result<Self, CaseError> {
        if crate::app_paths::isolated() {
            return Err(CaseError::KeysDisabled);
        }
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
        let nonce = Nonce::<Aes256Gcm>::try_generate()
            .map_err(|_| io::Error::other("Could not generate a nonce"))?;
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
        file.write_all(nonce.as_slice())?;
        file.write_all(&sealed)?;
        file.flush()?;
        file.as_file().sync_all()?;
        file.persist(self.path(&case.id))
            .map_err(|error| error.error)?;
        written.insert(case.id.clone(), case.revision);
        Ok(())
    }
    pub fn load(&self, id: &str) -> Result<Case, CaseError> {
        if !Self::valid_id(id) {
            return Err(CaseError::InvalidId);
        }
        let path = self.path(id);
        let bytes = read_limited(&path).map_err(CaseError::Read)?;
        let body = bytes
            .strip_prefix(MAGIC.as_slice())
            .filter(|b| b.len() > NONCE)
            .ok_or(CaseError::NotACase)?;
        let (nonce, sealed) = body.split_at(NONCE);
        let nonce = <&Nonce<Aes256Gcm>>::try_from(nonce).map_err(|_| CaseError::NotACase)?;
        let plain = self
            .cipher
            .decrypt(
                nonce,
                Payload {
                    msg: sealed,
                    aad: &Self::aad(id),
                },
            )
            .map_err(|_| CaseError::WrongKey)?;
        let case: Case = serde_json::from_slice(&plain).map_err(|_| CaseError::Damaged)?;
        if case.id != id {
            return Err(CaseError::Damaged);
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

/// Public build label, using the same runtime identity as case storage (ADR 0045).
/// This inspects signing metadata only; it never reads the case key.
pub fn build_identity() -> &'static str {
    #[cfg(target_os = "macos")]
    match key::keychain() {
        key::Keychain::Protected => "Protected",
        key::Keychain::Release => "Release",
        key::Keychain::Dev => "Dev",
    }
    #[cfg(not(target_os = "macos"))]
    if cfg!(debug_assertions) {
        "Dev"
    } else {
        "Release"
    }
}

/// The 256-bit case key, created once and held by the operating system.
mod key {
    use super::{CaseError, Generate, Path};
    #[cfg(windows)]
    use std::io::{self, Write};

    /// Which Keychain holds the key, decided once from this build's
    /// signature (ADR 0045).
    #[cfg(target_os = "macos")]
    #[derive(Clone, Copy, Debug, PartialEq)]
    pub enum Keychain {
        /// Developer ID with a provisioning profile: the data protection
        /// keychain, which never shows the password dialog.
        Protected,
        /// Developer ID without a profile (beta.8 and earlier): the login
        /// keychain under the release service.
        Release,
        /// Unsigned, ad-hoc or someone else's signature: the login keychain
        /// under its own service and cases folder, so a dev build never
        /// creates a key that later prompts the release.
        Dev,
    }
    #[cfg(target_os = "macos")]
    const SERVICE: &str = "io.github.lawkitt.mdoc";
    #[cfg(target_os = "macos")]
    const DEV_SERVICE: &str = "io.github.lawkitt.mdoc.dev";
    #[cfg(target_os = "macos")]
    const ACCOUNT: &str = "case-key";
    #[cfg(target_os = "macos")]
    const TEAM: &str = "F33DLM8K53";

    #[cfg(target_os = "macos")]
    pub fn keychain() -> Keychain {
        static KEYCHAIN: std::sync::LazyLock<Keychain> = std::sync::LazyLock::new(|| {
            use security_framework::os::macos::code_signing::{Flags, SecCode, SecRequirement};
            let requirement: Result<SecRequirement, _> =
                format!("anchor apple generic and certificate leaf[subject.OU] = \"{TEAM}\"")
                    .parse();
            let ours = requirement.is_ok_and(|r| {
                SecCode::for_self(Flags::NONE)
                    .and_then(|code| code.check_validity(Flags::NONE, &r))
                    .is_ok()
            });
            // Contents/MacOS/mdoc → Contents/embedded.provisionprofile
            let profile = std::env::current_exe().ok().is_some_and(|exe| {
                exe.parent()
                    .and_then(Path::parent)
                    .is_some_and(|contents| contents.join("embedded.provisionprofile").is_file())
            });
            match (ours, profile) {
                (true, true) => Keychain::Protected,
                (true, false) => Keychain::Release,
                (false, _) => Keychain::Dev,
            }
        });
        *KEYCHAIN
    }

    /// The cases folder name in app data: dev builds keep their own.
    pub fn folder() -> &'static str {
        #[cfg(target_os = "macos")]
        if keychain() == Keychain::Dev {
            return "cases-dev";
        }
        "cases"
    }

    /// Read (or create) the key with Keychain dialogs turned off, so this
    /// never asks: `None` when reading it would show the macOS password
    /// dialog, e.g. for a new build or after "Allow Once" (ADR 0043).
    #[cfg(target_os = "macos")]
    pub fn load_quietly(_: &Path) -> Result<Option<[u8; 32]>, CaseError> {
        load(true)
    }
    #[cfg(not(target_os = "macos"))]
    pub fn load_quietly(dir: &Path) -> Result<Option<[u8; 32]>, CaseError> {
        load_or_create(dir).map(Some)
    }

    #[cfg(target_os = "macos")]
    pub fn load_or_create(_: &Path) -> Result<[u8; 32], CaseError> {
        Ok(load(false)?.expect("only a quiet read is refused"))
    }

    #[cfg(target_os = "macos")]
    fn load(quiet: bool) -> Result<Option<[u8; 32]>, CaseError> {
        match keychain() {
            Keychain::Release => login_or_create(SERVICE, quiet),
            Keychain::Dev => login_or_create(DEV_SERVICE, quiet),
            Keychain::Protected => match protected::read() {
                // Missing entitlement (errSecMissingEntitlement): a profile
                // without keychain access; behave like a build without one.
                Err(-34018) => login_or_create(SERVICE, quiet),
                Err(e) => Err(CaseError::KeychainRead(e.to_string())),
                Ok(Some(key)) => Ok(Some(key)),
                // Not there yet: move the login-keychain key over (the one
                // dialog, behind the explainer), or create a new one only
                // when there is certainly no old key to lose.
                Ok(None) => {
                    let (key, moved) = match login(SERVICE, quiet)? {
                        Login::Key(key) => (key, true),
                        Login::MayAsk => return Ok(None),
                        Login::Missing => (fresh(), false),
                    };
                    protected::write(&key).map_err(|e| CaseError::KeychainStore(e.to_string()))?;
                    if moved {
                        forget_legacy();
                    }
                    Ok(Some(key))
                }
            },
        }
    }

    #[cfg(target_os = "macos")]
    enum Login {
        Key([u8; 32]),
        /// Reading it would show the macOS password dialog.
        MayAsk,
        Missing,
    }

    /// The key in the login keychain. A quiet read runs with Keychain dialogs
    /// off; macOS then refuses a read that would ask with
    /// errSecInteractionNotAllowed or, for an item whose access list does not
    /// trust this app, errSecAuthFailed. Both mean "may ask", never a denial:
    /// treating errSecAuthFailed as denied is how beta.7 fell through to a
    /// loud read at launch (ADR 0045).
    #[cfg(target_os = "macos")]
    fn login(service: &str, quiet: bool) -> Result<Login, CaseError> {
        use security_framework::{
            os::macos::keychain::SecKeychain, passwords::get_generic_password,
        };
        let _quiet = quiet
            .then(SecKeychain::disable_user_interaction)
            .transpose()
            .map_err(|e| CaseError::KeychainRead(e.to_string()))?;
        match get_generic_password(service, ACCOUNT) {
            Ok(bytes) => bytes
                .try_into()
                .map(Login::Key)
                .map_err(|_| CaseError::KeychainDamaged),
            // errSecInteractionNotAllowed, errSecAuthFailed, errSecUserCanceled
            // with dialogs off: reading would show the dialog.
            Err(e) if quiet && matches!(e.code(), -25308 | -25293 | -128) => Ok(Login::MayAsk),
            // errSecUserCanceled, errSecAuthFailed: the dialog was declined.
            Err(e) if matches!(e.code(), -128 | -25293) => Err(CaseError::KeyDenied),
            // errSecItemNotFound
            Err(e) if e.code() == -25300 => Ok(Login::Missing),
            Err(e) => Err(CaseError::KeychainRead(e.to_string())),
        }
    }

    /// The login-keychain key, created on first use; `None` when reading or
    /// creating it would show the dialog.
    #[cfg(target_os = "macos")]
    fn login_or_create(service: &str, quiet: bool) -> Result<Option<[u8; 32]>, CaseError> {
        use security_framework::{
            os::macos::keychain::SecKeychain, passwords::set_generic_password,
        };
        match login(service, quiet)? {
            Login::Key(key) => Ok(Some(key)),
            Login::MayAsk => Ok(None),
            Login::Missing => {
                let _quiet = quiet
                    .then(SecKeychain::disable_user_interaction)
                    .transpose()
                    .map_err(|e| CaseError::KeychainStore(e.to_string()))?;
                let key = fresh();
                match set_generic_password(service, ACCOUNT, &key) {
                    Ok(()) => Ok(Some(key)),
                    // A locked keychain: creating it needs the dialog too.
                    Err(e) if quiet && e.code() == -25308 => Ok(None),
                    Err(e) => Err(CaseError::KeychainStore(e.to_string())),
                }
            }
        }
    }

    /// Delete the moved login-keychain key, with dialogs off: if macOS would
    /// ask, it stays (the same key, still protected) and is never read again.
    #[cfg(target_os = "macos")]
    fn forget_legacy() {
        use security_framework::{
            os::macos::keychain::SecKeychain, passwords::delete_generic_password,
        };
        let Ok(_quiet) = SecKeychain::disable_user_interaction() else {
            return;
        };
        let _ = delete_generic_password(SERVICE, ACCOUNT);
    }

    /// The data protection keychain: access is by the app's Keychain access
    /// group, not a per-item access list, so it never asks.
    #[cfg(target_os = "macos")]
    mod protected {
        use super::{ACCOUNT, SERVICE};
        use security_framework::passwords::{
            PasswordOptions, generic_password, set_generic_password_options,
        };
        fn options() -> PasswordOptions {
            let mut options = PasswordOptions::new_generic_password(SERVICE, ACCOUNT);
            options.use_protected_keychain();
            options
        }
        /// `Ok(None)` when there is no key yet; `Err` is the OSStatus.
        pub fn read() -> Result<Option<[u8; 32]>, i32> {
            match generic_password(options()) {
                Ok(bytes) => bytes.try_into().map(Some).map_err(|_| -26275), // errSecDecode
                Err(e) if e.code() == -25300 => Ok(None),
                Err(e) => Err(e.code()),
            }
        }
        pub fn write(key: &[u8; 32]) -> Result<(), security_framework::base::Error> {
            set_generic_password_options(key, options())
        }
    }

    #[cfg(windows)]
    pub fn load_or_create(dir: &Path) -> Result<[u8; 32], CaseError> {
        let path = dir.join("key.dpapi");
        match std::fs::read(&path) {
            Ok(sealed) => dpapi::unprotect(&sealed)?
                .try_into()
                .map_err(|_| CaseError::KeyDamaged),
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
                    .map_err(CaseError::KeyWrite)?;
                Ok(key)
            }
            Err(e) => Err(CaseError::KeyRead(e)),
        }
    }

    /// Linux: the freedesktop Secret Service (GNOME Keyring, KWallet) over a
    /// pure-Rust D-Bus connection. Without one, cases stay off; there is no
    /// plaintext fallback.
    #[cfg(target_os = "linux")]
    pub fn load_or_create(_: &Path) -> Result<[u8; 32], CaseError> {
        use secret_service::{EncryptionType, blocking::SecretService};
        use std::collections::HashMap;
        let unavailable = |e: secret_service::Error| CaseError::KeyringUnavailable(e.to_string());
        let service = SecretService::connect(EncryptionType::Dh).map_err(unavailable)?;
        let collection = service.get_default_collection().map_err(unavailable)?;
        collection.ensure_unlocked().map_err(unavailable)?;
        let attributes = || {
            HashMap::from([
                ("application", "io.github.lawkitt.mdoc"),
                ("kind", "case-key"),
            ])
        };
        let found = collection.search_items(attributes()).map_err(unavailable)?;
        if let Some(item) = found.first() {
            return item
                .get_secret()
                .map_err(unavailable)?
                .try_into()
                .map_err(|_| CaseError::KeyringDamaged);
        }
        let key = fresh();
        collection
            .create_item(
                "mdoc case key",
                attributes(),
                &key,
                false,
                "application/octet-stream",
            )
            .map_err(|e| CaseError::KeyringStore(e.to_string()))?;
        Ok(key)
    }

    #[cfg(not(any(target_os = "macos", windows, target_os = "linux")))]
    pub fn load_or_create(_: &Path) -> Result<[u8; 32], CaseError> {
        Err(CaseError::NoKeyStore)
    }

    #[cfg(any(target_os = "macos", windows, target_os = "linux"))]
    fn fresh() -> [u8; 32] {
        <[u8; 32]>::generate()
    }

    #[cfg(windows)]
    mod dpapi {
        use super::CaseError;
        use windows_sys::Win32::{
            Foundation::LocalFree,
            Security::Cryptography::{
                CRYPT_INTEGER_BLOB, CRYPTPROTECT_UI_FORBIDDEN, CryptProtectData, CryptUnprotectData,
            },
        };
        fn run(input: &[u8], protect: bool) -> Result<Vec<u8>, CaseError> {
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
                return Err(CaseError::DataProtection);
            }
            // SAFETY: DPAPI returned cbData bytes at pbData.
            let bytes = unsafe {
                std::slice::from_raw_parts(blob_out.pbData, blob_out.cbData as usize).to_vec()
            };
            // SAFETY: the buffer was allocated by DPAPI with LocalAlloc.
            unsafe { LocalFree(blob_out.pbData as _) };
            Ok(bytes)
        }
        pub fn protect(input: &[u8]) -> Result<Vec<u8>, CaseError> {
            run(input, true)
        }
        pub fn unprotect(input: &[u8]) -> Result<Vec<u8>, CaseError> {
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

    /// The case stored in `tests/fixtures/cases/`, written by aes-gcm 0.10.
    fn fixture_case() -> Case {
        let mut case = Case::new(
            "Petrova v Northwind".into(),
            &[
                identity(
                    "PERSON_1",
                    Category::Person,
                    &["Anna Petrova", "A. Petrova"],
                ),
                identity("ORG_1", Category::Organization, &["Northwind Ltd"]),
            ],
            Some("/docs/contract.md".into()),
        );
        case.id = "0123456789abcdef0123456789abcdef".into();
        case.created = 1_760_000_000;
        case.revision = 3;
        case
    }

    #[test]
    fn a_case_written_by_aes_gcm_0_10_still_opens() {
        let fixture = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/cases");
        let dir = tempfile::tempdir().unwrap();
        let name = "0123456789abcdef0123456789abcdef.case";
        std::fs::copy(fixture.join(name), dir.path().join(name)).unwrap();
        let store = store(dir.path());
        let expected = fixture_case();
        assert_eq!(store.load(&expected.id).unwrap(), expected);
        // Rewriting it keeps the format and still opens.
        let mut next = expected.clone();
        next.revision += 1;
        store.save(&next).unwrap();
        assert_eq!(store.load(&next.id).unwrap(), next);
    }

    #[test]
    fn cases_round_trip_encrypted_and_never_in_plaintext() {
        let dir = tempfile::tempdir().unwrap();
        let store = store(dir.path());
        let mut case = Case::new(
            "Petrova v Northwind".into(),
            &[identity("PERSON_1", Category::Person, &["Anna Petrova"])],
            Some("/docs/contract.md".into()),
        );
        case.dismissed_hints
            .push(("Ms Petrova".into(), "Anna Petrova".into()));
        store.save(&case).unwrap();
        assert_eq!(store.load(&case.id).unwrap(), case);
        let bytes = std::fs::read(dir.path().join(format!("{}.case", case.id))).unwrap();
        let text = String::from_utf8_lossy(&bytes);
        for secret in ["Anna", "Petrova", "Ms Petrova", "PERSON_1", "contract.md"] {
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
    fn a_member_reply_is_kept_encrypted_moved_and_removed() {
        let dir = tempfile::tempdir().unwrap();
        let store = store(dir.path());
        let mut case = fixture_case();
        assert_eq!(case.reply(Path::new("/docs/contract.md")), None);
        let reply = "PERSON_1 owes ORG_1 a reply-only sentence.".to_string();
        assert!(case.set_reply(Path::new("/docs/contract.md"), Some(reply.clone())));
        assert!(!case.set_reply(Path::new("/docs/contract.md"), Some(reply.clone())));
        store.save(&case).unwrap();
        let bytes = std::fs::read(dir.path().join(format!("{}.case", case.id))).unwrap();
        assert!(!String::from_utf8_lossy(&bytes).contains("reply-only"));
        let mut loaded = store.load(&case.id).unwrap();
        assert_eq!(
            loaded.reply(Path::new("/docs/contract.md")),
            Some(reply.as_str())
        );
        loaded.move_reply(
            Path::new("/docs/contract.md"),
            Path::new("/docs/renamed.md"),
        );
        assert_eq!(loaded.reply(Path::new("/docs/contract.md")), None);
        assert!(loaded.reply(Path::new("/docs/renamed.md")).is_some());
        assert!(loaded.set_reply(Path::new("/docs/renamed.md"), None));
        assert!(loaded.replies.is_empty());
    }

    #[test]
    fn case_swatches_roundtrip_and_old_cases_snap_to_the_nearest_hue() {
        let dir = tempfile::tempdir().unwrap();
        let store = store(dir.path());
        for color in CaseColor::ALL {
            let mut case = fixture_case();
            case.color = Some(color);
            store.save(&case).unwrap();
            assert_eq!(
                CaseLink::of(&store.load(&case.id).unwrap()).hue(),
                color.hue()
            );
        }
        let mut json = serde_json::to_value(fixture_case()).unwrap();
        json.as_object_mut().unwrap().remove("color");
        let mut old: Case = serde_json::from_value(json).unwrap();
        assert_eq!(old.color, None);
        for (hue, expected) in [
            (359, CaseColor::Red),
            (32, CaseColor::Orange),
            (46, CaseColor::Amber),
            (110, CaseColor::Green),
            (173, CaseColor::Teal),
            (219, CaseColor::Blue),
            (269, CaseColor::Violet),
            (324, CaseColor::Pink),
        ] {
            old.id = format!("{hue:06x}00000000000000000000000000");
            assert_eq!(CaseLink::of(&old).hue(), expected.hue());
        }
    }

    #[test]
    fn forgotten_originals_cannot_return_from_a_stale_member() {
        let mut case = fixture_case();
        let old = case.exported();
        let alias = old[0].alias.clone();
        let wording = old[0].wording().to_string();
        case.dismissed_hints = vec![
            ("Short form".into(), wording.clone()),
            (wording, "Other variant".into()),
            ("Unrelated short".into(), "Unrelated full".into()),
        ];
        assert!(case.forget_identity(&alias));
        assert_eq!(
            case.dismissed_hints,
            [("Unrelated short".into(), "Unrelated full".into())]
        );
        assert!(!case.forget_identity(&alias));
        case.merge(&old);
        assert!(
            !case
                .exported()
                .iter()
                .any(|identity| identity.alias == alias)
        );
        assert!(
            case.identities
                .iter()
                .find(|identity| identity.alias == alias)
                .unwrap()
                .originals
                .is_empty()
        );
        assert_ne!(case.next_alias(old[0].category), alias);
        let dir = tempfile::tempdir().unwrap();
        let store = store(dir.path());
        store.save(&case).unwrap();
        assert_eq!(store.load(&case.id).unwrap(), case);
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
