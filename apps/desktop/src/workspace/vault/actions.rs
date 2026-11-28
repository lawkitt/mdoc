//! The Vault's actions: opening it at launch and unlocking it, the banner's
//! words, refusing intake without it, the recovery key's Show, nudge and
//! entry, and the user-wide lists' reads and writes.
#[cfg(any(
    test,
    all(
        debug_assertions,
        feature = "headless-renders",
        any(target_os = "macos", target_os = "windows")
    )
))]
use super::state::{InjectedKey, KeyStore};
use super::state::{VaultEffect, key_unreadable};
use super::text;
use super::{Opening, WAITS_FOR_RECOVERY_KEY};
use crate::workspace::{Workspace, saving::autosave::Write, saving::document_writer::Target};
use gpui::{Context, Focusable, SharedString, Window};
use mdoc_store::lists::Lists;
use mdoc_store::os_key::{self, KeyError};
use mdoc_vault::{UserLists, Vault, VaultError, recovery};
use zeroize::Zeroizing;

impl Workspace {
    /// Read the key quietly and open the Vault; off the main thread.
    pub(in crate::workspace) fn open_vault_at_launch() -> Opening {
        match os_key::vault_key(true) {
            Ok(Some(key)) => Self::open_vault(&key),
            Ok(None) => Opening::MayAsk,
            Err(error) => key_unreadable(error, os_key::vault_dir()),
        }
    }

    fn open_vault(key: &[u8; 32]) -> Opening {
        let Some(root) = os_key::vault_dir() else {
            return Opening::Key(KeyError::NoAppData);
        };
        match Vault::open(&root, key) {
            Ok(vault) => Opening::Opened(Box::new(vault)),
            Err(mdoc_vault::VaultError::WrongKey) => Opening::WrongKey(root),
            Err(error) => Opening::Vault(error),
        }
    }

    /// Carry out what the key or the Vault answered.
    pub(in crate::workspace) fn vault_opened(
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
                    let error = text::document_cannot_open(&why);
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
        // Its Projects and the user-wide lists come with it.
        if self.vault.vault().is_some() {
            self.load_projects(cx);
            self.start_ai_connector(window, cx);
        } else if self.vault.unavailable().is_some() {
            self.projects.clear();
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
    pub(in crate::workspace) fn use_vault(&mut self, vault: Vault) {
        self.vault.opened(Opening::Opened(Box::new(vault)));
    }

    /// Headless renders: launch found no key store.
    #[cfg(all(
        debug_assertions,
        feature = "headless-renders",
        any(target_os = "macos", target_os = "windows")
    ))]
    pub(in crate::workspace) fn vault_unavailable_for_render(
        &mut self,
    ) -> Option<std::sync::Arc<Vault>> {
        let vault = self.vault.vault().cloned();
        self.vault.opened(Opening::Key(KeyError::NoKeyStore));
        vault
    }

    /// Headless renders: launch found the Vault sealed with another key.
    #[cfg(all(
        debug_assertions,
        feature = "headless-renders",
        any(target_os = "macos", target_os = "windows")
    ))]
    pub(in crate::workspace) fn vault_wrong_key_for_render(
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
    pub(in crate::workspace) fn restore_vault_for_render(&mut self, vault: std::sync::Arc<Vault>) {
        self.vault.reopen(vault);
    }

    /// Tests: the notice showing.
    #[cfg(test)]
    pub(crate) fn notice_text(&self) -> Option<String> {
        self.notices.message().map(str::to_owned)
    }

    /// Tests: the Vault this window opened.
    #[cfg(test)]
    pub(crate) fn opened_vault(&self) -> Option<std::sync::Arc<Vault>> {
        self.vault.vault().cloned()
    }

    /// A Vault in a new temporary folder that lives as long as the window.
    #[cfg(test)]
    pub(in crate::workspace) fn use_temp_vault(&mut self) {
        let dir = tempfile::tempdir().expect("temporary Vault folder");
        let vault = Vault::open(dir.path().join("vault"), &[3; 32]).expect("temporary Vault");
        self.use_vault(vault);
        self.viewing_root = Some(dir.path().join("viewing"));
        self.test_vault_dir = Some(dir);
    }

    /// Reading the key at launch would ask (as on a Mac that has not allowed
    /// mdoc yet); the explainer's Continue then yields `open`.
    #[cfg(test)]
    pub(in crate::workspace) fn stage_vault_key(&mut self, open: Result<Vault, KeyError>) {
        self.vault.opened(Opening::MayAsk);
        self.staged_vault = Some(open);
    }

    /// The explainer's Continue for a Locked or Denied Vault: read the key,
    /// which may show the macOS dialog, and open the Vault. `Err` only when
    /// the dialog was declined.
    pub(in crate::workspace) fn unlock_vault(
        &mut self,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Result<(), KeyError> {
        #[cfg(test)]
        let opening = match self.staged_vault.take() {
            Some(Ok(vault)) => Opening::Opened(Box::new(vault)),
            Some(Err(error)) => Opening::Key(error),
            None => Opening::Key(KeyError::NoKeyStore),
        };
        #[cfg(not(test))]
        let opening = match os_key::vault_key(false) {
            Ok(Some(key)) => Self::open_vault(&key),
            Ok(None) => Opening::MayAsk,
            Err(error) => key_unreadable(error, os_key::vault_dir()),
        };
        let denied = matches!(opening, Opening::Key(KeyError::KeyDenied));
        self.vault_opened(opening, window, cx);
        if denied {
            Err(KeyError::KeyDenied)
        } else {
            Ok(())
        }
    }

    /// The banner line while documents wait for the key or can't be kept,
    /// with its button.
    pub(in crate::workspace) fn vault_banner(
        &self,
    ) -> Option<(SharedString, Option<&'static str>)> {
        if let Some(why) = self.vault.unavailable() {
            return Some((why.to_owned().into(), None));
        }
        if self.vault.wrong_key_root().is_some() {
            return Some((text::SAVED_WITH_DIFFERENT_KEY.into(), None));
        }
        if self.vault.denied() {
            return Some((text::NEEDS_KEYCHAIN.into(), Some(text::TRY_AGAIN)));
        }
        if self.vault.locked() {
            return Some((text::LOCKED.into(), Some(text::UNLOCK)));
        }
        None
    }

    /// Open, paste, New and drop make Vault documents: while the key is
    /// locked, the explainer offers to unlock it first; with no Vault this
    /// run, say why. Either way nothing is added now.
    pub(in crate::workspace) fn intake_refused(
        &mut self,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> bool {
        if self.vault.needs_key() {
            self.unlock_projects(window, cx);
            return true;
        }
        if self.vault.wrong_key_root().is_some() {
            self.notices.post(WAITS_FOR_RECOVERY_KEY);
            cx.notify();
            return true;
        }
        let Some(why) = self.vault.unavailable() else {
            return false;
        };
        self.notices.post(text::cannot_add(why));
        cx.notify();
        true
    }
}

impl Workspace {
    /// Settings › Vault's Show: the key, when it opens these documents.
    pub(in crate::workspace) fn show_recovery_key(&mut self, cx: &mut Context<Self>) {
        let key = match self.vault.vault() {
            None => Err(text::KEY_SHOWS_ONCE_OPEN.to_string()),
            Some(vault) => self.recovery.store.read().and_then(|key| {
                if vault.opens_with(&key) {
                    Ok(*key)
                } else {
                    Err(text::KEY_DOES_NOT_OPEN.into())
                }
            }),
        };
        if key.is_ok() {
            self.dismiss_recovery_nudge(cx);
        }
        self.settings
            .update(cx, |panel, cx| panel.show_recovery_key(key, cx));
    }

    /// Tests and headless renders: the key store the window reads.
    #[cfg(any(
        test,
        all(
            debug_assertions,
            feature = "headless-renders",
            any(target_os = "macos", target_os = "windows")
        )
    ))]
    pub(crate) fn use_key_store(&mut self, store: std::sync::Arc<std::sync::Mutex<InjectedKey>>) {
        self.recovery.store = KeyStore::Injected(store);
    }

    // ----- The first-Project nudge -----

    /// The nudge shows in the bell once the Vault holds a Project, until
    /// it is closed or the key is shown.
    pub(in crate::workspace) fn recovery_nudge(&self) -> bool {
        let dismissed = (self.preferences.borrow().current.as_ref())
            .is_none_or(|preferences| preferences.general.recovery_key_nudge_dismissed);
        !dismissed && self.vault.vault().is_some() && !self.projects.is_empty()
    }

    pub(in crate::workspace) fn dismiss_recovery_nudge(&mut self, cx: &mut Context<Self>) {
        if !self.recovery_nudge() {
            return;
        }
        if let Err(error) =
            self.update_general(|general| general.recovery_key_nudge_dismissed = true, cx)
        {
            self.notices.post(text::reminder_not_saved(&error));
        }
        cx.notify();
    }

    // ----- The wrong-key screen -----

    /// The Vault turned out sealed with another key: Home shows the screen,
    /// its field focused, over restored tabs that wait for the key.
    pub(in crate::workspace) fn recovery_screen_opened(
        &mut self,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if self.vault.wrong_key_root().is_none() {
            return;
        }
        // At launch, restore shows it once the tabs are back.
        if self.ready {
            self.show_home(window, cx);
        }
        let field = self.recovery_field(window, cx);
        let focus = field.read(cx).focus_handle(cx);
        window.focus(&focus, cx);
    }

    /// Tests: type `text` into the wrong-key screen's field.
    #[cfg(test)]
    pub(crate) fn type_recovery_key(
        &mut self,
        text: &str,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let field = self.recovery_field(window, cx);
        field.update(cx, |field, cx| field.set_value(text.into(), cx));
        let focus = field.read(cx).focus_handle(cx);
        window.focus(&focus, cx);
    }

    /// Headless renders: the wrong-key screen with a key typed and refused.
    #[cfg(all(
        debug_assertions,
        feature = "headless-renders",
        any(target_os = "macos", target_os = "windows")
    ))]
    pub(in crate::workspace) fn refuse_recovery_key_for_render(
        &mut self,
        text: &str,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let field = self.recovery_field(window, cx);
        field.update(cx, |field, cx| field.set_value(text.into(), cx));
        self.recovery.error = Some(refused(&VaultError::WrongKey));
        cx.notify();
    }

    /// Headless renders: the Vault opened again, so the screen goes.
    #[cfg(all(
        debug_assertions,
        feature = "headless-renders",
        any(target_os = "macos", target_os = "windows")
    ))]
    pub(in crate::workspace) fn recovery_screen_closed_for_render(&mut self) {
        self.recovery.field = None;
        self.recovery.error = None;
    }

    /// Open Documents: read the typed key, open the Vault with it off the
    /// main thread and, once it opens, keep it as the Vault key.
    pub(in crate::workspace) fn open_with_recovery_key(
        &mut self,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let (Some(root), Some(field)) = (self.vault.wrong_key_root(), self.recovery.field.clone())
        else {
            return;
        };
        if self.recovery.opening {
            return;
        }
        let key = match recovery::parse(field.read(cx).value()) {
            Ok(key) => Zeroizing::new(key),
            Err(error) => {
                self.recovery.error = Some(error.to_string());
                cx.notify();
                return;
            }
        };
        let root = root.to_path_buf();
        let store = self.recovery.store.clone();
        self.recovery.opening = true;
        self.recovery.error = None;
        cx.notify();
        let task = cx.background_executor().spawn(async move {
            let vault = Vault::open(root, &key)?;
            // Kept only once it has opened the documents.
            Ok::<_, VaultError>((vault, store.store(&key)))
        });
        cx.spawn_in(window, async move |this, cx| {
            let result = task.await;
            let _ = this.update_in(cx, |this, window, cx| {
                this.recovery.opening = false;
                match result {
                    Ok((vault, kept)) => {
                        if let Some(field) = this.recovery.field.take() {
                            field.update(cx, |field, cx| drop(field.take_secret(cx)));
                        }
                        this.recovery.error = None;
                        // Documents can be added now.
                        this.notices
                            .withdraw(crate::workspace::vault::WAITS_FOR_RECOVERY_KEY);
                        if let Err(why) = kept {
                            this.notices.post(text::key_not_kept(&why));
                        }
                        this.vault_opened(Opening::Opened(Box::new(vault)), window, cx);
                    }
                    Err(error) => {
                        this.recovery.error = Some(refused(&error));
                        cx.notify();
                    }
                }
            });
        })
        .detach();
    }
}

fn refused(error: &VaultError) -> String {
    match error {
        VaultError::WrongKey => text::WRONG_RECOVERY_KEY.into(),
        error => text::could_not_open_with_it(error),
    }
}

impl Workspace {
    /// The lists as the Vault holds them; an unreadable file leaves Settings
    /// showing them as locked rather than empty, so no save overwrites it.
    pub(in crate::workspace) fn user_lists_read(
        &mut self,
        read: Result<UserLists, VaultError>,
        cx: &mut Context<Self>,
    ) {
        let lists = match read {
            Ok(saved) => Some(saved.lists),
            Err(error) => {
                self.notices.post(text::lists_not_opened(&error));
                None
            }
        };
        self.settings
            .update(cx, |panel, cx| panel.set_user_lists(lists, cx));
    }

    /// Save the lists Settings changed, after the Keychain explainer while
    /// the Vault is locked: a job on the writer, the latest lists replacing
    /// a save still waiting, and the quit after it.
    pub(in crate::workspace) fn save_user_lists(
        &mut self,
        lists: Lists,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let retry = lists.clone();
        if !self.vault_key_ready(
            move |this, window, cx| this.save_user_lists(retry, window, cx),
            cx,
        ) {
            return;
        }
        if self.vault.vault().is_none() {
            self.notices.post(text::LISTS_UNAVAILABLE);
            cx.notify();
            return;
        }
        self.queue_write(Target::UserLists, Write::UserLists(lists), window, cx);
    }

    /// Write the user-wide lists at the head of the writer's queue.
    pub(in crate::workspace) fn run_user_lists(&mut self, lists: Lists, cx: &mut Context<Self>) {
        let Some(vault) = self.vault.vault().cloned() else {
            return;
        };
        if let Err(error) = vault.update_user_lists(|saved| *saved = lists) {
            self.notices.post(text::lists_not_saved(&error));
            cx.notify();
        }
    }
}
