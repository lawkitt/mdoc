//! The Vault tree: Projects collapsed and expanded, Unfiled with Show all
//! and a multi-select with its delete question, the recency switcher, and
//! Home after Close.
use crate::workspace::headless_renders::{Driver, Result};

const UNFILED: [&str; 9] = [
    "Board minutes",
    "Disclosure letter",
    "Engagement terms",
    "Expert report",
    "Invoice summary",
    "Lease schedule",
    "Hearing notes",
    "Settlement draft",
    "Witness outline",
];

/// Runs with the fixture Project `project` holding the fixture document.
pub(in crate::workspace::headless_renders) fn scenes(d: &mut Driver, project: &str) -> Result<()> {
    let shown = d.change(|w, _, _| w.tabs.active())?;
    d.change(|w, window, cx| {
        w.tree.set_expanded(project, false);
        w.sidebar.restore(true, Some(true));
        window.focus(&w.focus, cx);
        cx.notify();
    })?;
    d.capture("tree-projects-collapsed")?;
    d.change(|w, _, cx| {
        w.tree.set_expanded(project, true);
        cx.notify();
    })?;
    d.capture("tree-project-expanded")?;
    // Unfiled with more than its newest seven, all shown.
    let added = d.change(|w, _, cx| {
        let vault = w.vault.vault().cloned().expect("the fixture Vault");
        let added: Vec<String> = UNFILED
            .iter()
            .map(|title| {
                vault
                    .create_document(mdoc_vault::NewDocument {
                        project: None,
                        title: (*title).into(),
                        source_kind: mdoc_vault::SourceKind::Markdown,
                        origin: mdoc_vault::DocumentOrigin::Ordinary,
                        markdown: format!("# {title}\n"),
                    })
                    .expect("temporary fixture document")
                    .id
            })
            .collect();
        w.tree.set_expanded(project, false);
        w.tree.toggle_show_all();
        cx.notify();
        added
    })?;
    d.capture("tree-unfiled-show-all")?;
    d.change(|w, _, cx| {
        w.tree.choose(&added[1]);
        w.tree.toggle_chosen(&added[3]);
        w.tree.toggle_chosen(&added[4]);
        cx.notify();
    })?;
    d.capture("tree-unfiled-selected")?;
    d.change(|w, window, cx| {
        let chosen = w.tree.selection().to_vec();
        w.delete_documents(chosen, window, cx);
    })?;
    d.capture("confirm-delete-documents")?;
    d.key("escape")?;
    d.change(|w, _, cx| {
        w.tree.clear_selection();
        w.tree.toggle_show_all();
        let vault = w.vault.vault().cloned().expect("the fixture Vault");
        for id in &added {
            vault
                .delete_document(id)
                .expect("temporary fixture document");
        }
        cx.notify();
    })?;
    // The switcher over the live views, held open as ⌃ is: the Project's
    // other document was viewed just before this one.
    d.change(|w, window, cx| {
        let vault = w.vault.vault().cloned().expect("the fixture Vault");
        let other = (vault.documents_in(Some(project)).into_iter())
            .find(|summary| summary.title == "Witness statement")
            .and_then(|summary| mdoc_store::document_id::VaultId::parse(&summary.id))
            .expect("the Project's other document");
        w.open_vault_document(other, window, cx);
    })?;
    d.settle()?;
    d.change(|w, window, cx| w.activate(shown, window, cx))?;
    d.settle()?;
    d.change(|w, window, cx| w.switch_document(false, window, cx))?;
    d.capture("recency-switcher")?;
    d.change(|w, window, cx| w.cancel_switcher(window, cx))?;
    // Close returns to Home; the document stays live behind it.
    d.change(|_, window, cx| window.dispatch_action(Box::new(crate::workspace::Close), cx))?;
    d.capture("home-after-close")?;
    d.change(|w, window, cx| w.activate(shown, window, cx))?;
    d.settle()
}
