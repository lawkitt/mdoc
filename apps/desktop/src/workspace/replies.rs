//! Saved AI Replies: a reply pasted into a document's reply box, or by
//! Paste AI Reply, is saved through the Vault as received
//! (`save_shared_reply`: a Project's document gives the Project's reply, an
//! Unfiled one its own), and the Reply page shows a saved reply restored
//! from the identities it was bound to as they are now. The page is rebuilt
//! from the Vault each time it shows; no restored text is kept.
//!
//! Its actions, its view and its words each have their file.
use gpui::{Entity, FocusHandle};
use mdoc_pii::restore::{ExportedIdentity, RestoredText};
use mdoc_store::document_id::VaultId;
use mdoc_vault::Reply;

mod actions;
mod text;
mod view;

pub(super) use view::reply_detail;

/// A reply on its way into the Vault.
pub(super) enum ReplySave {
    /// The reply in tab `tab`'s box, as the box holds it when the save
    /// runs; it replaces the reply the box saved before. Shown once saved
    /// when `open`.
    Box { tab: u64, open: bool },
    /// Paste AI Reply in tab `tab`: `markdown` as pasted, aliases and all,
    /// to Document `document`. Saved even if the tab closes meanwhile, and
    /// shown once saved.
    Pasted {
        tab: u64,
        document: VaultId,
        title: String,
        markdown: String,
    },
}

/// View state only: never serialized, cleared on any other destination.
pub(super) struct ReplyPage {
    id: String,
    loaded: Result<Reply, String>,
    editor: Entity<mdoc_editor::EditorState>,
    restored: RestoredText,
    /// The identities `restored` was made with.
    made_from: Option<Vec<ExportedIdentity>>,
    /// The unresolved token Next unrecognized moves to after the last.
    next: usize,
    copied: bool,
    scroll: gpui::ScrollHandle,
    copy_focus: FocusHandle,
    next_focus: FocusHandle,
    promote_focus: FocusHandle,
    delete_focus: FocusHandle,
}

impl ReplyPage {
    pub(super) fn id(&self) -> &str {
        &self.id
    }

    /// The restored text as the page shows it.
    #[cfg(test)]
    pub(super) fn restored(&self) -> &RestoredText {
        &self.restored
    }

    #[cfg(test)]
    pub(super) fn test_controls(&self) -> Vec<(&'static str, FocusHandle)> {
        let mut controls = vec![
            ("reply-page-copy", self.copy_focus.clone()),
            ("reply-page-promote", self.promote_focus.clone()),
            ("reply-page-delete", self.delete_focus.clone()),
        ];
        if !self.restored.unresolved.is_empty() {
            controls.push(("reply-page-next", self.next_focus.clone()));
        }
        controls
    }

    /// "N values restored · M unrecognized", as the reply box counts.
    pub(super) fn summary(&self) -> String {
        let text = &self.restored.text;
        let unresolved: std::collections::HashSet<_> = (self.restored.unresolved.iter())
            .filter_map(|range| text.get(range.clone()))
            .collect();
        text::restored(self.restored.values, unresolved.len())
    }
}
