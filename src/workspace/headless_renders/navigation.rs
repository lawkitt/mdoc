//! Home, the document's first open, Back/Forward and the tab menu.
use super::{Driver, Result};
use crate::document_view::DocumentView;
use gpui::{Entity, px};
use std::path::Path;

/// Opens the fixture document and returns its view for later scenes.
pub(super) fn scenes(d: &mut Driver, path: &Path) -> Result<Entity<DocumentView>> {
    d.capture("home")?;
    // Home with no Projects and the header's bell: "Open a Project…" ends
    // under the local note, and the Tab walk starts at Open files… (U25).
    let projects = d.change(|w, _, cx| {
        let projects: Vec<_> = (w.projects.summaries().into_iter())
            .filter_map(|summary| w.projects.project(&summary.id).cloned())
            .collect();
        w.projects.finish_load(Vec::new());
        w.notices.post("Saved elsewhere");
        cx.notify();
        projects
    })?;
    d.settle()?;
    d.change(|w, window, cx| {
        w.title.notices_open.set(false);
        window.focus(&w.focus, cx);
        cx.notify();
    })?;
    d.capture("home-notice")?;
    d.change(|w, _, cx| {
        w.notices.dismiss();
        w.projects.finish_load(projects);
        cx.notify();
    })?;
    // The local note's explanation, as hovering it shows it (U16).
    d.change(|w, _, cx| {
        w.shell.local_note_hover = true;
        cx.notify();
    })?;
    d.capture("home-local-note")?;
    d.change(|w, _, cx| {
        w.shell.local_note_hover = false;
        cx.notify();
    })?;
    d.change(|w, _, cx| {
        w.sidebar.restore(false, Some(false));
        cx.notify();
    })?;
    d.capture("navigation-disabled")?;
    d.change(|w, _, cx| {
        w.sidebar.restore(true, Some(true));
        cx.notify();
    })?;
    let view = document(d, path)?;
    d.capture("document")?;
    d.change(|w, window, cx| {
        w.sidebar.restore(false, Some(false));
        window.focus(&w.focus, cx);
        cx.notify();
    })?;
    d.capture("navigation-enabled")?;
    // The document is in the middle: Home behind it and a second document
    // ahead.
    d.change(|w, window, cx| {
        let vault = w.vault.vault().cloned().expect("the fixture Vault");
        let notes = vault
            .create_document(mdoc_vault::NewDocument {
                project: None,
                title: "Meeting notes".into(),
                source_kind: mdoc_vault::SourceKind::Markdown,
                origin: mdoc_vault::DocumentOrigin::Ordinary,
                markdown: "# Meeting notes
".into(),
            })
            .expect("temporary fixture document");
        let notes = mdoc_store::document_id::VaultId::parse(&notes.id).expect("an id");
        w.open_vault_document(notes, window, cx);
    })?;
    d.settle()?;
    d.change(|w, window, cx| {
        w.navigate(true, window, cx);
        window.focus(&w.focus, cx);
    })?;
    d.capture("navigation-middle")?;
    let (back, forward) = d.change(|w, _, _| (w.back_focus.clone(), w.forward_focus.clone()))?;
    d.focus_pair("navigation-back", back)?;
    d.focus_pair("navigation-forward", forward)?;
    d.change(|w, _, cx| {
        w.sidebar.restore(true, Some(true));
        cx.notify();
    })?;
    document_menu(d)?;
    Ok(view)
}

/// The imported fixture's row menu in Unfiled, opened under its row as a
/// right-click does.
fn document_menu(d: &mut Driver) -> Result<()> {
    d.change(|w, window, cx| {
        let document = w
            .tree_now()
            .documents
            .first()
            .and_then(|summary| mdoc_store::document_id::VaultId::parse(&summary.id))
            .ok_or("no document row")?;
        // Under the first Unfiled row's label.
        let position = gpui::point(px(48.), px(230.));
        w.menu_return = window.focused(cx);
        window.focus(&w.menu_focus, cx);
        w.document_menu = Some((document, position));
        cx.notify();
        Ok::<_, String>(())
    })??;
    d.capture("document-menu")?;
    d.change(|w, window, cx| w.close_document_menu(window, cx))
}

fn document(d: &mut Driver, path: &Path) -> Result<Entity<DocumentView>> {
    // The fixture is imported as a drop imports it.
    d.change(|w, window, cx| {
        let path = mdoc_store::session_store::identity(path);
        w.finish_open(vec![(path, true)], None, window, cx);
    })?;
    d.settle()?;
    d.change(|w, window, cx| {
        window.focus(&w.focus, cx);
        w.active_view().filter(|view| {
            let view = view.read(cx);
            !view.lifecycle.loading() && view.session.document.vault.is_some()
        })
    })?
    .ok_or_else(|| "fixture drop did not open its document".into())
}
