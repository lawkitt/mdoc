//! The user-wide Always and Never lists of Settings › Detection: read with
//! the Projects when the Vault opens, and saved into it on every change by
//! a job on the window's writer, one revision after another.
use crate::workspace::{Workspace, saving::autosave::Write, saving::document_writer::Target};
use gpui::{Context, Window};
use mdoc_store::lists::Lists;
use mdoc_vault::{UserLists, VaultError};

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
            self.notices
                .post("Could not save your Always and Never lists: documents are unavailable.");
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
            self.notices.post(format!(
                "Could not save your Always and Never lists: {error}"
            ));
            cx.notify();
        }
    }
}
