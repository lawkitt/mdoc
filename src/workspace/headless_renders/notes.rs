//! Document notes and Promote to Document (#1091 Decision 3): the
//! Inspector's Notes tab empty and filled, the Reply page's Promote, and the
//! promoted document with its "From AI reply".
use super::{Driver, Result};
use crate::document_view::{DocumentView, Inspector};
use gpui::Entity;

/// The fixture document's Notes tab, empty, then with notes; Review again
/// after, with no notes left.
pub(super) fn scenes(d: &mut Driver, view: &Entity<DocumentView>) -> Result<()> {
    let was_open = d.change(|_, window, cx| {
        view.update(cx, |v, cx| {
            let open = v.replacements_open();
            v.show_notes(window, cx);
            open
        })
    })?;
    d.settle()?;
    d.capture("document-notes-empty")?;
    d.change(|_, window, cx| {
        view.update(cx, |v, cx| {
            v.type_notes(
                "Deadline: reply to the landlord by 14 March.\n\n\
                 - Ask whether the deposit covers the repairs\n\
                 - Check clause 7 against the earlier draft\n\n\
                 Never sent with Copy for AI.",
                window,
                cx,
            )
        })
    })?;
    d.settle()?;
    d.capture("document-notes")?;
    d.change(|_, window, cx| {
        view.update(cx, |v, cx| {
            v.type_notes("", window, cx);
            v.inspector = Inspector::Review;
            if !was_open && v.replacements_open() {
                v.toggle_replacements_panel(window, cx);
            }
            cx.notify();
        })
    })?;
    d.settle()
}

/// The saved reply on its page with Promote to Document focused, then
/// promoted: the new document open, "From AI reply" in its header.
pub(super) fn promoted(d: &mut Driver) -> Result<()> {
    // The fixture made its Project behind the window's back: the window
    // takes it in, as one made in the window would be.
    d.change(|w, _, _| -> Result<()> {
        let vault = w.vault.vault().cloned().ok_or("fixture Vault")?;
        let id = w
            .reply_page
            .as_ref()
            .ok_or("the Reply page")?
            .id()
            .to_string();
        if let mdoc_vault::ReplyOwner::Project(project) =
            vault.reply(&id).map_err(|e| e.to_string())?.owner
        {
            w.projects
                .put(vault.project(&project).map_err(|e| e.to_string())?);
        }
        Ok(())
    })??;
    d.change(|w, window, cx| w.focus_reply_promote(window, cx))?;
    d.settle()?;
    d.capture("reply-page-promote")?;
    d.change(|w, window, cx| w.promote_saved_reply(window, cx))?;
    d.settle()?;
    d.change(|w, _, cx| {
        let view = w.active_view().expect("the promoted document");
        let view = view.read(cx);
        assert!(view.from_reply.is_some(), "the promoted document opened");
        assert!(
            view.pii.review().remaining() > 0,
            "its Project's names are marked again"
        );
    })?;
    d.capture("document-from-reply")
}
