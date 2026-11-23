//! The Vault in the window: opened at launch, unlocked through the Keychain
//! explainer, and why documents can't be added when there is none.
use super::{
    Workspace,
    vault_state::{Opening, VaultEffect},
};
use gpui::{Context, SharedString, Window};
use mdoc_store::cases::{self, CaseError, Store};
use mdoc_vault::Vault;
use std::sync::Arc;

impl Workspace {
    /// Read the key quietly and open the Vault; off the main thread. The key
    /// comes back too, so the case store needs no second read.
    pub(super) fn open_vault_at_launch() -> (Opening, Option<[u8; 32]>) {
        match cases::vault_key(true) {
            Ok(Some(key)) => (Self::open_vault(&key), Some(key)),
            Ok(None) => (Opening::MayAsk, None),
            Err(error) => (Opening::Key(error), None),
        }
    }

    fn open_vault(key: &[u8; 32]) -> Opening {
        let Some(root) = cases::vault_dir() else {
            return Opening::Key(CaseError::NoAppData);
        };
        match Vault::open(&root, key) {
            Ok(vault) => Opening::Opened(Box::new(vault)),
            Err(mdoc_vault::VaultError::WrongKey) => Opening::WrongKey(root),
            Err(error) => Opening::Vault(error),
        }
    }

    /// The key was read for the Vault: saved cases use it too.
    pub(super) fn key_read_at_launch(&mut self, key: Option<[u8; 32]>) {
        if let Some(key) = key
            && self.case_coordinator.store().is_none()
            && let Some(dir) = Store::default_dir()
        {
            self.case_coordinator
                .open(Arc::new(Store::with_key(dir, &key)));
        }
    }

    /// Carry out what the key or the Vault answered.
    pub(super) fn vault_opened(
        &mut self,
        opening: Opening,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        match self.vault.opened(opening) {
            VaultEffect::Load(tabs) => {
                for id in tabs {
                    let Some(tab) = self.tabs.get(id) else {
                        continue;
                    };
                    let record = tab.record().clone();
                    if let Some(document) = tab.document().cloned().or_else(|| record.document()) {
                        self.load_document(id, document, record, window, cx);
                    }
                }
            }
            VaultEffect::Fail(tabs, why) => {
                for view in tabs
                    .into_iter()
                    .filter_map(|id| self.tabs.get(id)?.view().cloned())
                {
                    let error = format!("This document can't open. {why}");
                    view.update(cx, |view, cx| {
                        let generation = view.lifecycle.begin_load();
                        view.lifecycle.finish_load(generation, false);
                        view.error = Some(error);
                        cx.notify();
                    });
                }
            }
            VaultEffect::None => {}
        }
        self.recovery_screen_opened(window, cx);
        cx.notify();
    }

    /// Tests and headless renders: use `vault`, as if launch opened it.
    #[cfg(any(
        test,
        all(
            debug_assertions,
            feature = "headless-renders",
            any(target_os = "macos", target_os = "windows")
        )
    ))]
    pub(super) fn use_vault(&mut self, vault: Vault) {
        self.vault.opened(Opening::Opened(Box::new(vault)));
    }

    /// Headless renders: launch found no key store.
    #[cfg(all(
        debug_assertions,
        feature = "headless-renders",
        any(target_os = "macos", target_os = "windows")
    ))]
    pub(super) fn vault_unavailable_for_render(&mut self) -> Option<Arc<Vault>> {
        let vault = self.vault.vault().cloned();
        self.vault.opened(Opening::Key(CaseError::NoKeyStore));
        vault
    }

    /// Headless renders: launch found the Vault sealed with another key.
    #[cfg(all(
        debug_assertions,
        feature = "headless-renders",
        any(target_os = "macos", target_os = "windows")
    ))]
    pub(super) fn vault_wrong_key_for_render(
        &mut self,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Option<std::sync::Arc<Vault>> {
        let vault = self.vault.vault().cloned();
        let root = vault.as_ref()?.root().to_path_buf();
        self.vault_opened(Opening::WrongKey(root), window, cx);
        vault
    }

    /// Headless renders: the Vault opened again.
    #[cfg(all(
        debug_assertions,
        feature = "headless-renders",
        any(target_os = "macos", target_os = "windows")
    ))]
    pub(super) fn restore_vault_for_render(&mut self, vault: Arc<Vault>) {
        self.vault.reopen(vault);
    }

    /// Tests: the notice showing.
    #[cfg(test)]
    pub(crate) fn notice_text(&self) -> Option<String> {
        self.notices.message().map(str::to_owned)
    }

    /// Tests: the Vault this window opened.
    #[cfg(test)]
    pub(crate) fn opened_vault(&self) -> Option<Arc<Vault>> {
        self.vault.vault().cloned()
    }

    /// A Vault in a new temporary folder that lives as long as the window.
    #[cfg(test)]
    pub(super) fn use_temp_vault(&mut self) {
        let dir = tempfile::tempdir().expect("temporary Vault folder");
        let vault = Vault::open(dir.path().join("vault"), &[3; 32]).expect("temporary Vault");
        self.use_vault(vault);
        self.viewing_root = Some(dir.path().join("viewing"));
        self.test_vault_dir = Some(dir);
    }

    /// Reading the key at launch would ask (as on a Mac that has not allowed
    /// mdoc yet); the explainer's Continue then yields `open`.
    #[cfg(test)]
    pub(super) fn stage_vault_key(&mut self, open: Result<Vault, CaseError>) {
        self.vault.opened(Opening::MayAsk);
        self.staged_vault = Some(open);
    }

    /// The explainer's Continue for a Locked or Denied Vault: read the key,
    /// which may show the macOS dialog, and open the Vault. Saved cases get
    /// the same key. `Err` only when the dialog was declined.
    pub(super) fn unlock_vault(
        &mut self,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Result<(), CaseError> {
        #[cfg(test)]
        let opening = match self.staged_vault.take() {
            Some(Ok(vault)) => Opening::Opened(Box::new(vault)),
            Some(Err(error)) => Opening::Key(error),
            None => Opening::Key(CaseError::NoKeyStore),
        };
        #[cfg(not(test))]
        let opening = match cases::vault_key(false) {
            Ok(Some(key)) => {
                self.key_read_at_launch(Some(key));
                Self::open_vault(&key)
            }
            Ok(None) => Opening::MayAsk,
            Err(error) => Opening::Key(error),
        };
        let denied = matches!(opening, Opening::Key(CaseError::KeyDenied));
        self.vault_opened(opening, window, cx);
        if denied {
            Err(CaseError::KeyDenied)
        } else {
            Ok(())
        }
    }

    /// The banner line while documents wait for the key or can't be kept,
    /// with its button.
    pub(super) fn vault_banner(&self) -> Option<(SharedString, Option<&'static str>)> {
        if let Some(why) = self.vault.unavailable() {
            return Some((why.to_owned().into(), None));
        }
        if self.vault.wrong_key_root().is_some() {
            return Some((
                "Your documents were saved with a different key. Enter your recovery key on \
                 Home to open them."
                    .into(),
                None,
            ));
        }
        if self.vault.denied() {
            return Some((
                "Your documents need Keychain access to open.".into(),
                Some("Try again…"),
            ));
        }
        if self.vault.locked() {
            return Some((
                "Your documents are locked. Unlock them to open them.".into(),
                Some("Unlock…"),
            ));
        }
        None
    }

    /// Open, paste, New and drop make Vault documents: while the key is
    /// locked, the explainer offers to unlock it first; with no Vault this
    /// run, say why. Either way nothing is added now.
    pub(super) fn intake_refused(&mut self, window: &mut Window, cx: &mut Context<Self>) -> bool {
        if self.vault.needs_key() {
            self.unlock_cases(window, cx);
            return true;
        }
        if self.vault.wrong_key_root().is_some() {
            self.notices.post(
                "Documents can't be added until your recovery key opens the documents you \
                 have. Enter it on Home.",
            );
            cx.notify();
            return true;
        }
        let Some(why) = self.vault.unavailable() else {
            return false;
        };
        self.notices
            .post(format!("Documents can't be added. {why}"));
        cx.notify();
        true
    }
}
