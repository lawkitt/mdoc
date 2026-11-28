//! The Vault's state: the window's hold on it, opened at launch with the
//! OS-held key read quietly, Locked while that read would ask, Denied after
//! the macOS dialog, WrongKey for a Vault sealed with another key; where the
//! key is read and kept; and which tabs wait for it. No GPUI.
use super::text;
use super::text::{key_unavailable, vault_unavailable};
use mdoc_store::os_key::KeyError;
use mdoc_vault::{Vault, VaultError};
use std::{
    path::{Path, PathBuf},
    sync::Arc,
};
use zeroize::Zeroizing;

/// What reading the key and opening the Vault gave.
pub(in crate::workspace) enum Opening {
    Opened(Box<Vault>),
    /// Reading the key would show the macOS password dialog.
    MayAsk,
    Key(KeyError),
    /// The Vault in this folder was sealed with another key.
    WrongKey(PathBuf),
    Vault(VaultError),
}

/// What the workspace view carries out after a transition.
#[derive(Debug, PartialEq, Eq)]
pub(in crate::workspace) enum VaultEffect {
    /// Load these tabs' documents from the Vault.
    Load(Vec<u64>),
    /// Show these tabs that their document can't open, and why.
    Fail(Vec<u64>, String),
    None,
}

enum Access {
    /// Not read yet: launch has not finished.
    Unopened,
    Locked,
    Denied,
    Open(Arc<Vault>),
    /// Sealed with another key: the recovery key for the Vault in this
    /// folder opens it.
    WrongKey(PathBuf),
    /// Why documents can't be kept, in the user's words.
    Unavailable(String),
}

pub(in crate::workspace) struct VaultState {
    access: Access,
    /// Tabs whose document waits for the key.
    waiting: Vec<u64>,
}

impl Default for VaultState {
    fn default() -> Self {
        Self {
            access: Access::Unopened,
            waiting: Vec::new(),
        }
    }
}

impl VaultState {
    pub(in crate::workspace) fn vault(&self) -> Option<&Arc<Vault>> {
        match &self.access {
            Access::Open(vault) => Some(vault),
            _ => None,
        }
    }

    /// Why documents can't be kept, when the Vault is Unavailable.
    pub(in crate::workspace) fn unavailable(&self) -> Option<&str> {
        match &self.access {
            Access::Unavailable(why) => Some(why),
            _ => None,
        }
    }

    pub(in crate::workspace) fn locked(&self) -> bool {
        matches!(self.access, Access::Locked)
    }

    pub(in crate::workspace) fn denied(&self) -> bool {
        matches!(self.access, Access::Denied)
    }

    /// The Vault's folder, while it waits for its recovery key.
    pub(in crate::workspace) fn wrong_key_root(&self) -> Option<&Path> {
        match &self.access {
            Access::WrongKey(root) => Some(root),
            _ => None,
        }
    }

    /// Unlocking would read the key: Locked or Denied.
    pub(in crate::workspace) fn needs_key(&self) -> bool {
        matches!(self.access, Access::Locked | Access::Denied)
    }

    /// The key or the Vault answered.
    pub(in crate::workspace) fn opened(&mut self, opening: Opening) -> VaultEffect {
        self.access = match opening {
            Opening::Opened(vault) => Access::Open(Arc::from(vault)),
            Opening::MayAsk => Access::Locked,
            Opening::Key(KeyError::KeyDenied) => Access::Denied,
            Opening::Key(error) => Access::Unavailable(key_unavailable(&error)),
            Opening::WrongKey(root) => Access::WrongKey(root),
            Opening::Vault(error) => Access::Unavailable(vault_unavailable(&error)),
        };
        match &self.access {
            Access::Open(_) => VaultEffect::Load(std::mem::take(&mut self.waiting)),
            Access::Unavailable(why) => {
                VaultEffect::Fail(std::mem::take(&mut self.waiting), why.clone())
            }
            _ => VaultEffect::None,
        }
    }

    /// Headless renders: the Vault is open again.
    #[cfg(all(
        debug_assertions,
        feature = "headless-renders",
        any(target_os = "macos", target_os = "windows")
    ))]
    pub(in crate::workspace) fn reopen(&mut self, vault: Arc<Vault>) {
        self.access = Access::Open(vault);
    }

    /// Tab `id`'s document waits for the key.
    pub(in crate::workspace) fn wait(&mut self, id: u64) {
        if !self.waiting.contains(&id) {
            self.waiting.push(id);
        }
    }

    /// Tab `id` closed.
    pub(in crate::workspace) fn forget(&mut self, id: u64) {
        self.waiting.retain(|&tab| tab != id);
    }
}

/// Where the window reads and keeps the Vault key.
#[derive(Clone, Default)]
pub(crate) enum KeyStore {
    /// The OS key store, through mdoc-store's guarded key path.
    #[default]
    Os,
    /// Tests and headless renders: a key in memory.
    #[cfg(any(
        test,
        all(
            debug_assertions,
            feature = "headless-renders",
            any(target_os = "macos", target_os = "windows")
        )
    ))]
    Injected(std::sync::Arc<std::sync::Mutex<InjectedKey>>),
}

/// An injected key store: what it holds, and what was stored in it.
#[cfg(any(
    test,
    all(
        debug_assertions,
        feature = "headless-renders",
        any(target_os = "macos", target_os = "windows")
    )
))]
#[derive(Default)]
pub(crate) struct InjectedKey {
    pub key: Option<[u8; 32]>,
    /// Every key stored, in order.
    pub stored: Vec<[u8; 32]>,
    /// Storing fails, as when the OS refuses.
    pub refuse: bool,
}

impl KeyStore {
    /// The Vault key, read the way Unlock reads it (it may ask on macOS).
    pub(super) fn read(&self) -> Result<Zeroizing<[u8; 32]>, String> {
        match self {
            Self::Os => match mdoc_store::os_key::vault_key(false) {
                Ok(Some(key)) => Ok(Zeroizing::new(key)),
                Ok(None) => Err(text::KEY_NEEDS_ASKING.into()),
                Err(error) => Err(error.to_string()),
            },
            #[cfg(any(
                test,
                all(
                    debug_assertions,
                    feature = "headless-renders",
                    any(target_os = "macos", target_os = "windows")
                )
            ))]
            Self::Injected(store) => (store.lock().unwrap().key)
                .map(Zeroizing::new)
                .ok_or_else(|| text::NO_KEY_HELD.into()),
        }
    }

    /// Keep `key` as the Vault key.
    pub(super) fn store(&self, key: &[u8; 32]) -> Result<(), String> {
        match self {
            Self::Os => mdoc_store::os_key::store_vault_key(key).map_err(|e| e.to_string()),
            #[cfg(any(
                test,
                all(
                    debug_assertions,
                    feature = "headless-renders",
                    any(target_os = "macos", target_os = "windows")
                )
            ))]
            Self::Injected(store) => {
                let mut store = store.lock().unwrap();
                if store.refuse {
                    return Err(text::KEY_STORE_REFUSED.into());
                }
                store.key = Some(*key);
                store.stored.push(*key);
                Ok(())
            }
        }
    }
}

/// A key that is there but can't be opened (a slot or key file sealed by
/// another Windows installation, or damaged) leaves a Vault that holds
/// documents to its recovery key, which then replaces the slot. Any other
/// failure leaves the Vault unavailable.
pub(super) fn key_unreadable(error: KeyError, root: Option<std::path::PathBuf>) -> Opening {
    let unreadable = matches!(
        error,
        KeyError::DataProtection
            | KeyError::KeyDamaged
            | KeyError::KeychainDamaged
            | KeyError::KeyringDamaged
    );
    match root {
        Some(root) if unreadable && holds_objects(&root) => Opening::WrongKey(root),
        _ => Opening::Key(error),
    }
}

/// The Vault's folder holds sealed objects (#1091 Decision 12's layout).
pub(super) fn holds_objects(root: &std::path::Path) -> bool {
    root.join("manifest.enc").is_file()
        || ["docs", "projects", "replies"].iter().any(|dir| {
            std::fs::read_dir(root.join(dir)).is_ok_and(|mut entries| entries.next().is_some())
        })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn vault() -> (tempfile::TempDir, Vault) {
        let dir = tempfile::tempdir().unwrap();
        let vault = Vault::open(dir.path().join("vault"), &[3; 32]).unwrap();
        (dir, vault)
    }

    #[test]
    fn tabs_waiting_on_a_locked_key_load_once_it_opens() {
        let mut state = VaultState::default();
        assert_eq!(state.opened(Opening::MayAsk), VaultEffect::None);
        assert!(state.locked() && state.needs_key() && state.vault().is_none());
        state.wait(4);
        state.wait(4);
        state.wait(7);
        state.forget(7);
        assert_eq!(
            state.opened(Opening::Key(KeyError::KeyDenied)),
            VaultEffect::None
        );
        assert!(state.denied() && state.needs_key());
        let (_dir, vault) = vault();
        assert_eq!(
            state.opened(Opening::Opened(Box::new(vault))),
            VaultEffect::Load(vec![4])
        );
        assert!(state.vault().is_some() && !state.needs_key());
    }

    #[test]
    fn no_key_store_leaves_documents_unavailable_and_says_why() {
        let mut state = VaultState::default();
        state.wait(2);
        let VaultEffect::Fail(tabs, why) = state.opened(Opening::Key(KeyError::NoKeyStore)) else {
            panic!("unavailable");
        };
        assert_eq!(tabs, vec![2]);
        assert_eq!(state.unavailable(), Some(why.as_str()));
        assert!(why.contains("Windows data protection"));
        assert!(!state.needs_key() && state.vault().is_none());
        state.opened(Opening::Vault(VaultError::WrongKey));
        assert!(state.unavailable().unwrap().contains("different key"));
    }

    #[test]
    fn a_vault_sealed_with_another_key_keeps_tabs_waiting_for_its_recovery_key() {
        let mut state = VaultState::default();
        state.wait(3);
        let root = PathBuf::from("vault");
        assert_eq!(
            state.opened(Opening::WrongKey(root.clone())),
            VaultEffect::None
        );
        assert_eq!(state.wrong_key_root(), Some(root.as_path()));
        assert!(state.unavailable().is_none() && !state.needs_key());
        let (_dir, vault) = vault();
        assert_eq!(
            state.opened(Opening::Opened(Box::new(vault))),
            VaultEffect::Load(vec![3])
        );
        assert!(state.wrong_key_root().is_none());
    }

    #[test]
    fn a_key_that_cannot_be_opened_offers_the_recovery_key_for_a_vault_with_documents() {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path().join("vault");
        // No Vault yet: nothing to recover.
        let opening = key_unreadable(KeyError::DataProtection, Some(root.clone()));
        assert!(matches!(opening, Opening::Key(KeyError::DataProtection)));
        let vault = Vault::open(&root, &[1; 32]).unwrap();
        vault
            .create_project(mdoc_vault::NewProject::default())
            .unwrap();
        vault.flush().unwrap();
        for error in [KeyError::DataProtection, KeyError::KeyDamaged] {
            assert!(matches!(
                key_unreadable(error, Some(root.clone())),
                Opening::WrongKey(found) if found == root
            ));
        }
        // No key store at all is not a key to recover.
        assert!(matches!(
            key_unreadable(KeyError::NoKeyStore, Some(root.clone())),
            Opening::Key(KeyError::NoKeyStore)
        ));
    }
}
