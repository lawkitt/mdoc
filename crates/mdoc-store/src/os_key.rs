//! The OS-held key: the Vault key (#1091 Decisions 21 and 22), read from
//! the Vault key slot first, else the OS key, created on first use (macOS
//! Keychain, Windows data protection). There is no plaintext fallback. The
//! OS key keeps the names an earlier build gave it for saved cases
//! (`case-key`, `cases/key.dpapi`): it is read, never moved or rewritten,
//! and the old cases folder stays untouched (Decision 2).
use aes_gcm::aead::Generate;
use std::{
    io,
    path::{Path, PathBuf},
};

#[derive(Debug, thiserror::Error)]
pub enum KeyError {
    /// The user declined the macOS Keychain dialog.
    #[error("The Vault needs Keychain access.")]
    KeyDenied,
    /// A Keychain call failed; the OS error, as macOS words it.
    #[error("Could not read the Vault key from the Keychain: {0}")]
    KeychainRead(String),
    #[error("Could not store the Vault key in the Keychain: {0}")]
    KeychainStore(String),
    #[error("The Vault key in the Keychain is damaged.")]
    KeychainDamaged,
    /// Windows: the DPAPI-sealed key file.
    #[error("Could not read the Vault key: {0}")]
    KeyRead(io::Error),
    #[error("Could not store the Vault key: {0}")]
    KeyWrite(io::Error),
    #[error("The Vault key is damaged.")]
    KeyDamaged,
    #[error("Windows data protection could not open the Vault key.")]
    DataProtection,
    /// Linux: no usable Secret Service; its error, as the service words it.
    #[error(
        "The Vault needs a Secret Service keyring (GNOME Keyring or KWallet), which is not available: {0}"
    )]
    KeyringUnavailable(String),
    #[error("Could not store the Vault key in the keyring: {0}")]
    KeyringStore(String),
    #[error("The Vault key in the keyring is damaged.")]
    KeyringDamaged,
    #[error(
        "The Vault needs the macOS Keychain, Windows data protection or a Secret Service keyring, so it is not available on this system."
    )]
    NoKeyStore,
    #[error("OS keys are disabled in headless renders")]
    KeysDisabled,
    #[error("No folder for app data on this system.")]
    NoAppData,
}

/// Test builds (feature `deny-os-services`): a call that would ask
/// the OS's security or identity services panics, so a test that reaches one
/// fails at once instead of waiting on macOS (#510). The headless render
/// sandbox is a dev build that asks nothing: there it returns `true`, and the
/// caller answers as a dev build without asking.
#[cfg(feature = "deny-os-services")]
fn deny_os_service(what: &str) -> bool {
    if crate::app_paths::isolated() {
        return true;
    }
    panic!(
        "a test asked the OS for {what}: inject the Vault key and build identity instead \
         (development.md, Tests and OS services)"
    )
}

/// The folder in app data where Windows keeps the OS key, named for the
/// saved cases an earlier build kept there: `cases`, or `cases-dev` for a
/// macOS dev build.
fn key_dir() -> Option<PathBuf> {
    crate::app_paths::data_local_dir().map(|p| p.join("mdoc").join(key::folder()))
}

/// The Vault's folder in app data: `vault` beside the key's folder, or
/// `vault-dev` beside a dev build's, so a dev build never opens the release
/// Vault.
pub fn vault_dir() -> Option<PathBuf> {
    crate::app_paths::data_local_dir().map(|p| p.join("mdoc").join(key::vault_folder()))
}

/// The Vault key: a recovery key kept in the Vault key slot, read first,
/// or else the OS key, created on first use (#1091 Decisions 21
/// and 22). `quiet` reads with dialogs off: `None` when reading would show
/// the macOS password dialog. Elsewhere a quiet read is the real one.
pub fn vault_key(quiet: bool) -> Result<Option<[u8; 32]>, KeyError> {
    if crate::app_paths::isolated() {
        return Err(KeyError::KeysDisabled);
    }
    #[cfg(feature = "deny-os-services")]
    deny_os_service("the Vault key");
    let dir = key_dir().ok_or(KeyError::NoAppData)?;
    let mdoc = dir.parent().ok_or(KeyError::NoAppData)?.to_path_buf();
    slot_first(
        || key::load_vault_slot(&mdoc, quiet),
        || {
            if quiet {
                key::load_quietly(&dir)
            } else {
                key::load_or_create(&dir).map(Some)
            }
        },
    )
}

/// Keep `key`, a recovery key that opened the Vault, as the Vault key: in
/// the Vault key slot of the OS key store, never in a plain file. The OS
/// key and the old cases folder are left as they are (#1091 Decision 2).
pub fn store_vault_key(key: &[u8; 32]) -> Result<(), KeyError> {
    if crate::app_paths::isolated() {
        return Err(KeyError::KeysDisabled);
    }
    #[cfg(feature = "deny-os-services")]
    deny_os_service("the Vault key, to store it");
    let mdoc = crate::app_paths::data_local_dir()
        .ok_or(KeyError::NoAppData)?
        .join("mdoc");
    key::store_vault_slot(&mdoc, key)
}

/// What the Vault key slot holds.
#[derive(Debug, PartialEq)]
enum Slot {
    Key([u8; 32]),
    /// No recovery key was ever kept: the OS key is the Vault key.
    Missing,
    /// Reading it would show the macOS password dialog.
    #[cfg_attr(not(target_os = "macos"), allow(dead_code))]
    MayAsk,
}

/// The slot's key when it holds one; the OS key only when it is empty.
fn slot_first(
    slot: impl FnOnce() -> Result<Slot, KeyError>,
    os_key: impl FnOnce() -> Result<Option<[u8; 32]>, KeyError>,
) -> Result<Option<[u8; 32]>, KeyError> {
    match slot()? {
        Slot::Key(key) => Ok(Some(key)),
        Slot::MayAsk => Ok(None),
        Slot::Missing => os_key(),
    }
}

/// Public build label, using the same runtime identity as the Vault key.
/// This inspects signing metadata only; it never reads the key.
pub fn build_identity() -> &'static str {
    #[cfg(feature = "deny-os-services")]
    if deny_os_service("the build identity") {
        return "Dev";
    }
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

/// The 256-bit OS key, created once and held by the operating system.
mod key {
    #[cfg(any(target_os = "macos", windows, target_os = "linux"))]
    use super::Generate;
    use super::{KeyError, Path};
    #[cfg(windows)]
    use std::io::{self, Write};

    /// Which Keychain holds the key, decided once from this build's
    /// signature.
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
        /// under its own service and key folder, so a dev build never
        /// creates a key that later prompts the release.
        Dev,
    }
    #[cfg(target_os = "macos")]
    const SERVICE: &str = "io.github.lawkitt.mdoc";
    #[cfg(target_os = "macos")]
    const DEV_SERVICE: &str = "io.github.lawkitt.mdoc.dev";
    #[cfg(target_os = "macos")]
    /// The account an earlier build named for saved cases; kept, so the
    /// key is the same.
    const ACCOUNT: &str = "case-key";
    #[cfg(target_os = "macos")]
    const TEAM: &str = "F33DLM8K53";

    #[cfg(target_os = "macos")]
    pub fn keychain() -> Keychain {
        #[cfg(feature = "deny-os-services")]
        if super::deny_os_service("the build's code signature") {
            return Keychain::Dev;
        }
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

    /// The key's folder name in app data, as an earlier build named it for
    /// saved cases: dev builds keep their own.
    pub fn folder() -> &'static str {
        #[cfg(target_os = "macos")]
        if keychain() == Keychain::Dev {
            return "cases-dev";
        }
        "cases"
    }

    /// The Vault's folder name in app data, picked as [`folder`] is.
    pub fn vault_folder() -> &'static str {
        if folder() == "cases-dev" {
            "vault-dev"
        } else {
            "vault"
        }
    }

    /// Read (or create) the key with Keychain dialogs turned off, so this
    /// never asks: `None` when reading it would show the macOS password
    /// dialog, e.g. for a new build or after "Allow Once".
    #[cfg(target_os = "macos")]
    pub fn load_quietly(_: &Path) -> Result<Option<[u8; 32]>, KeyError> {
        load(true)
    }
    #[cfg(not(target_os = "macos"))]
    pub fn load_quietly(dir: &Path) -> Result<Option<[u8; 32]>, KeyError> {
        load_or_create(dir).map(Some)
    }

    #[cfg(target_os = "macos")]
    pub fn load_or_create(_: &Path) -> Result<[u8; 32], KeyError> {
        Ok(load(false)?.expect("only a quiet read is refused"))
    }

    #[cfg(target_os = "macos")]
    fn load(quiet: bool) -> Result<Option<[u8; 32]>, KeyError> {
        match keychain() {
            Keychain::Release => login_or_create(SERVICE, quiet),
            Keychain::Dev => login_or_create(DEV_SERVICE, quiet),
            Keychain::Protected => match protected::read() {
                // Missing entitlement (errSecMissingEntitlement): a profile
                // without keychain access; behave like a build without one.
                Err(-34018) => login_or_create(SERVICE, quiet),
                Err(e) => Err(KeyError::KeychainRead(e.to_string())),
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
                    protected::write(&key).map_err(|e| KeyError::KeychainStore(e.to_string()))?;
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
    /// loud read at launch.
    #[cfg(target_os = "macos")]
    fn login(service: &str, quiet: bool) -> Result<Login, KeyError> {
        login_account(service, ACCOUNT, quiet)
    }

    /// [`login`] for any account: the OS key's or the Vault key slot's.
    #[cfg(target_os = "macos")]
    fn login_account(service: &str, account: &str, quiet: bool) -> Result<Login, KeyError> {
        use security_framework::{
            os::macos::keychain::SecKeychain, passwords::get_generic_password,
        };
        let _quiet = quiet
            .then(SecKeychain::disable_user_interaction)
            .transpose()
            .map_err(|e| KeyError::KeychainRead(e.to_string()))?;
        match get_generic_password(service, account) {
            Ok(bytes) => bytes
                .try_into()
                .map(Login::Key)
                .map_err(|_| KeyError::KeychainDamaged),
            // errSecInteractionNotAllowed, errSecAuthFailed, errSecUserCanceled
            // with dialogs off: reading would show the dialog.
            Err(e) if quiet && matches!(e.code(), -25308 | -25293 | -128) => Ok(Login::MayAsk),
            // errSecUserCanceled, errSecAuthFailed: the dialog was declined.
            Err(e) if matches!(e.code(), -128 | -25293) => Err(KeyError::KeyDenied),
            // errSecItemNotFound
            Err(e) if e.code() == -25300 => Ok(Login::Missing),
            Err(e) => Err(KeyError::KeychainRead(e.to_string())),
        }
    }

    /// The login-keychain key, created on first use; `None` when reading or
    /// creating it would show the dialog.
    #[cfg(target_os = "macos")]
    fn login_or_create(service: &str, quiet: bool) -> Result<Option<[u8; 32]>, KeyError> {
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
                    .map_err(|e| KeyError::KeychainStore(e.to_string()))?;
                let key = fresh();
                match set_generic_password(service, ACCOUNT, &key) {
                    Ok(()) => Ok(Some(key)),
                    // A locked keychain: creating it needs the dialog too.
                    Err(e) if quiet && e.code() == -25308 => Ok(None),
                    Err(e) => Err(KeyError::KeychainStore(e.to_string())),
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
        fn options(account: &str) -> PasswordOptions {
            let mut options = PasswordOptions::new_generic_password(SERVICE, account);
            options.use_protected_keychain();
            options
        }
        /// `Ok(None)` when there is no key yet; `Err` is the OSStatus.
        pub fn read() -> Result<Option<[u8; 32]>, i32> {
            read_account(ACCOUNT)
        }
        pub fn read_account(account: &str) -> Result<Option<[u8; 32]>, i32> {
            match generic_password(options(account)) {
                Ok(bytes) => bytes.try_into().map(Some).map_err(|_| -26275), // errSecDecode
                Err(e) if e.code() == -25300 => Ok(None),
                Err(e) => Err(e.code()),
            }
        }
        pub fn write(key: &[u8; 32]) -> Result<(), security_framework::base::Error> {
            write_account(ACCOUNT, key)
        }
        pub fn write_account(
            account: &str,
            key: &[u8; 32],
        ) -> Result<(), security_framework::base::Error> {
            set_generic_password_options(key, options(account))
        }
    }

    #[cfg(windows)]
    pub fn load_or_create(dir: &Path) -> Result<[u8; 32], KeyError> {
        let path = dir.join("key.dpapi");
        match std::fs::read(&path) {
            Ok(sealed) => dpapi::unprotect(&sealed)?
                .try_into()
                .map_err(|_| KeyError::KeyDamaged),
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
                    .map_err(KeyError::KeyWrite)?;
                Ok(key)
            }
            Err(e) => Err(KeyError::KeyRead(e)),
        }
    }

    /// Linux: the freedesktop Secret Service (GNOME Keyring, KWallet) over a
    /// pure-Rust D-Bus connection. Without one, the Vault is unavailable; there is no
    /// plaintext fallback.
    #[cfg(target_os = "linux")]
    pub fn load_or_create(_: &Path) -> Result<[u8; 32], KeyError> {
        use secret_service::{EncryptionType, blocking::SecretService};
        use std::collections::HashMap;
        let unavailable = |e: secret_service::Error| KeyError::KeyringUnavailable(e.to_string());
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
                .map_err(|_| KeyError::KeyringDamaged);
        }
        let key = fresh();
        collection
            .create_item(
                "mdoc Vault key",
                attributes(),
                &key,
                false,
                "application/octet-stream",
            )
            .map_err(|e| KeyError::KeyringStore(e.to_string()))?;
        Ok(key)
    }

    #[cfg(not(any(target_os = "macos", windows, target_os = "linux")))]
    pub fn load_or_create(_: &Path) -> Result<[u8; 32], KeyError> {
        Err(KeyError::NoKeyStore)
    }

    /// The Vault key slot's account (macOS) or kind (Linux): written only
    /// by a recovery key, read before the OS key.
    #[cfg(any(target_os = "macos", target_os = "linux"))]
    const VAULT_ACCOUNT: &str = "vault-key";

    /// Windows: the slot's DPAPI-sealed file in mdoc's app data folder,
    /// beside the Vault's folder and never inside it, so a copied Vault
    /// carries no key sealed to another computer.
    #[cfg(windows)]
    fn vault_slot_file(mdoc: &Path) -> std::path::PathBuf {
        mdoc.join(format!("{}-key.dpapi", vault_folder()))
    }

    #[cfg(windows)]
    pub fn load_vault_slot(mdoc: &Path, _quiet: bool) -> Result<super::Slot, KeyError> {
        match std::fs::read(vault_slot_file(mdoc)) {
            Ok(sealed) => dpapi::unprotect(&sealed)?
                .try_into()
                .map(super::Slot::Key)
                .map_err(|_| KeyError::KeyDamaged),
            Err(e) if e.kind() == io::ErrorKind::NotFound => Ok(super::Slot::Missing),
            Err(e) => Err(KeyError::KeyRead(e)),
        }
    }

    #[cfg(windows)]
    pub fn store_vault_slot(mdoc: &Path, key: &[u8; 32]) -> Result<(), KeyError> {
        let sealed = dpapi::protect(key)?;
        std::fs::create_dir_all(mdoc)
            .and_then(|_| {
                let mut file = tempfile::NamedTempFile::new_in(mdoc)?;
                file.write_all(&sealed)?;
                file.as_file().sync_all()?;
                file.persist(vault_slot_file(mdoc)).map_err(|e| e.error)?;
                Ok(())
            })
            .map_err(KeyError::KeyWrite)
    }

    /// macOS: the same Keychain as the OS key, under the slot's account.
    #[cfg(target_os = "macos")]
    pub fn load_vault_slot(_: &Path, quiet: bool) -> Result<super::Slot, KeyError> {
        let login = |service: &str| -> Result<super::Slot, KeyError> {
            Ok(match login_account(service, VAULT_ACCOUNT, quiet)? {
                Login::Key(key) => super::Slot::Key(key),
                Login::MayAsk => super::Slot::MayAsk,
                Login::Missing => super::Slot::Missing,
            })
        };
        match keychain() {
            Keychain::Release => login(SERVICE),
            Keychain::Dev => login(DEV_SERVICE),
            Keychain::Protected => match protected::read_account(VAULT_ACCOUNT) {
                // Missing entitlement: as a build without a profile.
                Err(-34018) => login(SERVICE),
                Err(e) => Err(KeyError::KeychainRead(e.to_string())),
                Ok(Some(key)) => Ok(super::Slot::Key(key)),
                Ok(None) => Ok(super::Slot::Missing),
            },
        }
    }

    #[cfg(target_os = "macos")]
    pub fn store_vault_slot(_: &Path, key: &[u8; 32]) -> Result<(), KeyError> {
        use security_framework::passwords::set_generic_password;
        let store = |e: security_framework::base::Error| KeyError::KeychainStore(e.to_string());
        match keychain() {
            Keychain::Release => set_generic_password(SERVICE, VAULT_ACCOUNT, key).map_err(store),
            Keychain::Dev => set_generic_password(DEV_SERVICE, VAULT_ACCOUNT, key).map_err(store),
            Keychain::Protected => match protected::write_account(VAULT_ACCOUNT, key) {
                // Missing entitlement: as a build without a profile.
                Err(e) if e.code() == -34018 => {
                    set_generic_password(SERVICE, VAULT_ACCOUNT, key).map_err(store)
                }
                result => result.map_err(store),
            },
        }
    }

    /// Linux: a second Secret Service item, `kind=vault-key`.
    #[cfg(target_os = "linux")]
    pub fn load_vault_slot(_: &Path, _quiet: bool) -> Result<super::Slot, KeyError> {
        use secret_service::{EncryptionType, blocking::SecretService};
        let unavailable = |e: secret_service::Error| KeyError::KeyringUnavailable(e.to_string());
        let service = SecretService::connect(EncryptionType::Dh).map_err(unavailable)?;
        let collection = service.get_default_collection().map_err(unavailable)?;
        collection.ensure_unlocked().map_err(unavailable)?;
        let found = collection
            .search_items(vault_attributes())
            .map_err(unavailable)?;
        match found.first() {
            Some(item) => item
                .get_secret()
                .map_err(unavailable)?
                .try_into()
                .map(super::Slot::Key)
                .map_err(|_| KeyError::KeyringDamaged),
            None => Ok(super::Slot::Missing),
        }
    }

    #[cfg(target_os = "linux")]
    pub fn store_vault_slot(_: &Path, key: &[u8; 32]) -> Result<(), KeyError> {
        use secret_service::{EncryptionType, blocking::SecretService};
        let unavailable = |e: secret_service::Error| KeyError::KeyringUnavailable(e.to_string());
        let service = SecretService::connect(EncryptionType::Dh).map_err(unavailable)?;
        let collection = service.get_default_collection().map_err(unavailable)?;
        collection.ensure_unlocked().map_err(unavailable)?;
        collection
            .create_item(
                "mdoc Vault key",
                vault_attributes(),
                key,
                true,
                "application/octet-stream",
            )
            .map_err(|e| KeyError::KeyringStore(e.to_string()))?;
        Ok(())
    }

    #[cfg(target_os = "linux")]
    fn vault_attributes() -> std::collections::HashMap<&'static str, &'static str> {
        std::collections::HashMap::from([
            ("application", "io.github.lawkitt.mdoc"),
            ("kind", VAULT_ACCOUNT),
        ])
    }

    #[cfg(not(any(target_os = "macos", windows, target_os = "linux")))]
    pub fn load_vault_slot(_: &Path, _quiet: bool) -> Result<super::Slot, KeyError> {
        Ok(super::Slot::Missing)
    }

    #[cfg(not(any(target_os = "macos", windows, target_os = "linux")))]
    pub fn store_vault_slot(_: &Path, _: &[u8; 32]) -> Result<(), KeyError> {
        Err(KeyError::NoKeyStore)
    }

    #[cfg(any(target_os = "macos", windows, target_os = "linux"))]
    fn fresh() -> [u8; 32] {
        <[u8; 32]>::generate()
    }

    #[cfg(windows)]
    #[allow(unsafe_code)] // DPAPI through Win32 (#897).
    mod dpapi {
        use super::KeyError;
        use windows_sys::Win32::{
            Foundation::LocalFree,
            Security::Cryptography::{
                CRYPT_INTEGER_BLOB, CRYPTPROTECT_UI_FORBIDDEN, CryptProtectData, CryptUnprotectData,
            },
        };
        fn run(input: &[u8], protect: bool) -> Result<Vec<u8>, KeyError> {
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
                return Err(KeyError::DataProtection);
            }
            // SAFETY: DPAPI returned cbData bytes at pbData.
            let bytes = unsafe {
                std::slice::from_raw_parts(blob_out.pbData, blob_out.cbData as usize).to_vec()
            };
            // SAFETY: the buffer was allocated by DPAPI with LocalAlloc.
            unsafe { LocalFree(blob_out.pbData as _) };
            Ok(bytes)
        }
        pub fn protect(input: &[u8]) -> Result<Vec<u8>, KeyError> {
            run(input, true)
        }
        pub fn unprotect(input: &[u8]) -> Result<Vec<u8>, KeyError> {
            run(input, false)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// An OS key store in memory: the OS key, the Vault key slot, and
    /// every read and write of each.
    #[derive(Default)]
    struct FakeKeys {
        os: Option<[u8; 32]>,
        slot: Option<[u8; 32]>,
        os_reads: usize,
        os_writes: usize,
    }

    impl FakeKeys {
        fn vault_key(&mut self, may_ask: bool) -> Result<Option<[u8; 32]>, KeyError> {
            let slot = match (self.slot, may_ask) {
                (_, true) => Slot::MayAsk,
                (Some(key), false) => Slot::Key(key),
                (None, false) => Slot::Missing,
            };
            let os = &mut self.os;
            let (reads, writes) = (&mut self.os_reads, &mut self.os_writes);
            slot_first(
                || Ok(slot),
                || {
                    *reads += 1;
                    // Created on first use, as the OS store does.
                    if os.is_none() {
                        *os = Some([9; 32]);
                        *writes += 1;
                    }
                    Ok(*os)
                },
            )
        }
    }

    #[test]
    fn the_vault_key_slot_is_read_before_the_os_key() {
        let mut keys = FakeKeys {
            os: Some([1; 32]),
            ..FakeKeys::default()
        };
        // No recovery key kept: the OS key is the Vault key.
        assert_eq!(keys.vault_key(false).unwrap(), Some([1; 32]));
        assert_eq!(keys.os_reads, 1);
        // A recovery key kept in the slot wins, and the OS key is not
        // even read.
        keys.slot = Some([2; 32]);
        assert_eq!(keys.vault_key(false).unwrap(), Some([2; 32]));
        assert_eq!(keys.os_reads, 1);
        // A slot that would ask asks first: nothing is read behind it.
        assert_eq!(keys.vault_key(true).unwrap(), None);
        assert_eq!(keys.os_reads, 1);
        // The OS key was never written, and is still the old one.
        assert_eq!((keys.os, keys.os_writes), (Some([1; 32]), 0));
    }

    #[test]
    fn a_slot_error_is_reported_and_never_falls_back_to_the_os_key() {
        let mut read = false;
        let result = slot_first(
            || Err(KeyError::KeyDamaged),
            || {
                read = true;
                Ok(Some([1; 32]))
            },
        );
        assert!(matches!(result, Err(KeyError::KeyDamaged)));
        assert!(!read);
    }

    /// The guard covers the slot: a test that reaches for the OS Vault key,
    /// to read or to keep one, fails at once.
    #[cfg(feature = "deny-os-services")]
    #[test]
    #[should_panic(expected = "a test asked the OS for the Vault key, to store it")]
    fn storing_a_vault_key_in_a_test_panics() {
        let _ = store_vault_key(&[1; 32]);
    }

    #[cfg(feature = "deny-os-services")]
    #[test]
    #[should_panic(expected = "a test asked the OS for the Vault key")]
    fn reading_the_vault_key_in_a_test_panics() {
        let _ = vault_key(true);
    }
}
