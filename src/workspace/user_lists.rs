//! The user-wide Always and Never lists of Settings › Detection: read with
//! the Projects when the Vault opens, and saved into it at once on every
//! change, one revision after another.
use super::Workspace;
use gpui::Context;
use mdoc_store::lists::Lists;
use mdoc_vault::{UserLists, VaultError};

impl Workspace {
    /// The lists as the Vault holds them; an unreadable file leaves Settings
    /// showing them as locked rather than empty, so no save overwrites it.
    pub(super) fn user_lists_read(
        &mut self,
        read: Result<UserLists, VaultError>,
        cx: &mut Context<Self>,
    ) {
        let lists = match read {
            Ok(saved) => Some(saved.lists),
            Err(error) => {
                self.notices.post(format!(
                    "Your Always and Never lists could not be opened: {error}"
                ));
                None
            }
        };
        self.settings
            .update(cx, |panel, cx| panel.set_user_lists(lists, cx));
    }

    /// Save the lists Settings changed, after the Keychain explainer while
    /// the Vault is locked.
    pub(super) fn save_user_lists(&mut self, lists: Lists, cx: &mut Context<Self>) {
        let retry = lists.clone();
        if !self.vault_key_ready(move |this, _, cx| this.save_user_lists(retry, cx), cx) {
            return;
        }
        let Some(vault) = self.vault.vault().cloned() else {
            self.notices
                .post("Could not save your Always and Never lists: documents are unavailable.");
            cx.notify();
            return;
        };
        // A small file, written at once: a quit right after an edit never
        // outruns it.
        if let Err(error) = vault.update_user_lists(|saved| *saved = lists) {
            self.notices.post(format!(
                "Could not save your Always and Never lists: {error}"
            ));
            cx.notify();
        }
    }
}
