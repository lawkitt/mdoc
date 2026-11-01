//! The user-wide Always and Never lists of Settings › Detection (ADR 0092):
//! read with the saved cases, since they share the OS-held key, and saved
//! beside them on every change, one revision after another.
use super::Workspace;
use gpui::Context;
use mdoc_store::{cases::UserLists, lists::Lists};

impl Workspace {
    /// The lists as read with the cases; an unreadable file leaves Settings
    /// showing them as locked rather than empty, so no save overwrites it.
    pub(super) fn user_lists_read(
        &mut self,
        read: Result<UserLists, mdoc_store::cases::CaseError>,
        cx: &mut Context<Self>,
    ) {
        let lists = match read {
            Ok(saved) => {
                self.cases.user_lists_revision = saved.revision;
                Some(saved.lists)
            }
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
    /// Save the lists Settings changed, opening the key first if needed
    /// (on a Mac, after the Keychain explainer).
    pub(super) fn save_user_lists(&mut self, lists: Lists, cx: &mut Context<Self>) {
        let retry = lists.clone();
        if !self.case_key_ready(move |this, _, cx| this.save_user_lists(retry, cx), cx) {
            return;
        }
        let store = match self.case_store() {
            Ok(store) => store,
            Err(error) => {
                self.notices.post(format!(
                    "Could not save your Always and Never lists: {error}"
                ));
                cx.notify();
                return;
            }
        };
        self.cases.user_lists_revision += 1;
        let saved = UserLists {
            revision: self.cases.user_lists_revision,
            lists,
        };
        let task = cx
            .background_executor()
            .spawn(async move { store.save_lists(&saved) });
        cx.spawn(async move |this, cx| {
            if let Err(error) = task.await {
                let _ = this.update(cx, |this, cx| {
                    this.notices.post(format!(
                        "Could not save your Always and Never lists: {error}"
                    ));
                    cx.notify();
                });
            }
        })
        .detach();
    }
}
