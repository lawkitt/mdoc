//! New Project: the tree's Projects header with its "+", the Open Project
//! list's hint with no Projects, the name sheet, the empty Project's page
//! it opens, and Move to New Project's name field in the header.
use super::{Driver, Result};

/// Runs with the sidebar open; leaves the Vault's Projects as it found them.
pub(super) fn scenes(d: &mut Driver) -> Result<()> {
    let shown = d.change(|w, _, _| w.tabs.active())?;
    d.change(|w, window, cx| {
        w.sidebar.restore(true, Some(true));
        window.focus(&w.focus, cx);
        cx.notify();
    })?;
    d.capture("tree-projects-header")?;
    // No Projects: the window's copy only, put back after.
    let kept = d.change(|w, window, cx| {
        let kept: Vec<_> = (w.projects.summaries().into_iter())
            .filter_map(|summary| w.projects.project(&summary.id).cloned())
            .collect();
        w.projects.finish_load(Vec::new());
        w.show_project_list(window, cx);
        kept
    })?;
    d.settle()?;
    d.capture("open-project-empty")?;
    d.change(|w, window, cx| w.close_project_list(window, cx))?;
    d.capture("tree-no-projects")?;
    d.change(|w, _, cx| {
        w.projects.finish_load(kept);
        cx.notify();
    })?;
    // The name sheet, from the "+", with a name typed.
    d.change(|_, window, cx| window.dispatch_action(Box::new(super::super::NewProject), cx))?;
    d.settle()?;
    d.change(|w, _, cx| w.type_new_project_name("Estate of Novak", cx))?;
    d.capture("new-project-sheet")?;
    // Create: the capture's Tab walk left focus on a button.
    d.change(|w, window, cx| w.confirm_new_project(window, cx))?;
    d.settle()?;
    // A fixed creation date, as the Project page scene has.
    d.change(|w, _, cx| {
        if let Some(id) = w.project_ui.page.as_ref().map(|page| page.id().to_string()) {
            w.projects.edit(&id, |project| {
                project.created = 1791763200;
                Some(())
            });
        }
        cx.notify();
    })?;
    d.capture("new-project-page")?;
    // Gone again, so later scenes see the fixture's Projects only.
    d.change(|w, window, cx| -> Result<()> {
        let id = (w.project_ui.page.as_ref())
            .map(|page| page.id().to_string())
            .ok_or("the new Project's page")?;
        let vault = w.vault.vault().cloned().ok_or("the fixture Vault")?;
        vault
            .delete_project(&id, mdoc_vault::ProjectDocuments::Delete)
            .map_err(|e| e.to_string())?;
        w.projects.remove(&id);
        w.navigation
            .forget(&crate::workspace::sidebar::navigation::Stop::Project(id));
        w.activate(shown, window, cx);
        Ok(())
    })??;
    d.settle()?;
    move_to_new_project(d, shown)
}

/// Move to New Project (#1313): an Unfiled document moves at once, and the
/// new Project's name is ready to edit in the header's breadcrumb, the
/// document's name selected. Undone after, the document deleted.
fn move_to_new_project(d: &mut Driver, shown: u64) -> Result<()> {
    let letter = d.change(|w, window, cx| -> Result<_> {
        let vault = w.vault.vault().cloned().ok_or("the fixture Vault")?;
        let letter = vault
            .create_document(mdoc_vault::NewDocument {
                project: None,
                title: "Lease letter".into(),
                source_kind: mdoc_vault::SourceKind::Markdown,
                origin: mdoc_vault::DocumentOrigin::Ordinary,
                markdown: "# Lease letter\n\nThe deposit is returned in full.\n".into(),
            })
            .map_err(|e| e.to_string())?;
        let id = mdoc_store::document_id::VaultId::parse(&letter.id).ok_or("its id")?;
        w.open_vault_document(id.clone(), window, cx);
        Ok(id)
    })??;
    d.settle()?;
    d.change(|w, window, cx| {
        let tab = w.tabs.active();
        w.move_to_new_project(tab, window, cx);
    })?;
    d.settle()?;
    d.capture("move-to-new-project-name")?;
    d.change(|w, window, cx| {
        w.finish_rename(false, window, cx);
        w.undo_move(window, cx);
    })?;
    d.settle()?;
    d.change(|w, window, cx| -> Result<()> {
        // Undo's own toast goes too, so later scenes show none.
        w.dismiss_toast(cx);
        let tab = w.tabs.active();
        w.close_tab(tab, window, cx);
        w.activate(shown, window, cx);
        Ok(())
    })??;
    d.settle()?;
    d.change(|w, _, cx| -> Result<()> {
        let vault = w.vault.vault().cloned().ok_or("the fixture Vault")?;
        vault
            .delete_document(letter.as_str())
            .map_err(|e| e.to_string())?;
        cx.notify();
        Ok(())
    })??;
    d.settle()
}
