//! The flows the scenes do not otherwise run, for the old cases folder's
//! check (`old_cases`): a restart on the theme's Vault that imports, saves,
//! moves, exports, deletes and quits, then a launch as the app starts, with
//! the sandbox refusing the OS key. Nothing is captured.
use super::{Driver, Result, Workspace, vault::wait_until};
use gpui::{AppContext as _, px, size};
use mdoc_store::{document_id::VaultId, session_store::Session};
use std::path::Path;

pub(super) fn run(d: &mut Driver, vault: &Path, profile: &Path) -> Result<()> {
    let main = d.window;
    let ran = restart(d, vault, profile).and_then(|()| launch(d));
    d.window = main;
    ran
}

/// A window on `vault` as a restart opens it, with the session written to
/// the profile: import, autosave, move in and out of a Project, export,
/// delete, quit.
fn restart(d: &mut Driver, vault: &Path, profile: &Path) -> Result<()> {
    let session = mdoc_store::app_paths::data_local_dir()
        .ok_or("missing data directory")?
        .join("mdoc/session.json");
    let root = vault.to_path_buf();
    let window =
        d.cx.open_window(
            size(px(super::WIDTH), px(super::HEIGHT)),
            move |window, cx| {
                cx.new(|cx| {
                    let mut workspace = Workspace::empty(window, cx);
                    workspace.use_vault(
                        mdoc_vault::Vault::open(&root, &[3; 32]).expect("the theme's Vault"),
                    );
                    workspace.session_writer.set_path(Some(session));
                    workspace.restore(Session::default(), window, cx);
                    workspace.install_prompts(cx);
                    workspace
                })
            },
        )
        .map_err(|e| e.to_string())?;
    d.window = window.into();
    d.settle()?;

    let folder = profile.join("cut-over");
    std::fs::create_dir_all(&folder).map_err(|e| e.to_string())?;
    let file = folder.join("Cut-over lease.md");
    std::fs::write(
        &file,
        "# Cut-over lease\n\nBetween Jane Roe and Acme Ltd.\n",
    )
    .map_err(|e| e.to_string())?;
    let file = mdoc_store::session_store::identity(&file);
    d.change({
        let file = file.clone();
        move |w, window, cx| w.open_paths(vec![file], window, cx)
    })?;
    let document = |w: &Workspace| -> Option<VaultId> {
        let (tab, _) = w.admitted_from.iter().find(|(_, from)| **from == file)?;
        Some(w.tabs.get(*tab)?.document()?.vault().clone())
    };
    wait_until(d, "the import", |w, _| document(w).is_some())?;
    let id = d
        .change(|w, _, _| document(w))?
        .ok_or("the imported document")?;
    let idle = |w: &Workspace, _: &gpui::Window| !w.document_writer.busy();

    // An edit, saved as a quit or the save shortcut saves it.
    d.change(|w, window, cx| {
        if let Some(view) = w.active_view() {
            view.update(cx, |view, cx| {
                view.editor.update(cx, |editor, cx| {
                    editor.set_text(
                        "# Cut-over lease\n\nBetween Jane Roe and Acme Ltd., signed.\n",
                        cx,
                    )
                });
            });
        }
        w.autosave_all(window, cx);
    })?;
    wait_until(d, "the save", idle)?;

    // Into a new Project, back to Unfiled, and in again.
    d.change({
        let id = id.clone();
        move |w, window, cx| w.move_document_to_new_project(id, window, cx)
    })?;
    let project_of = {
        let id = id.clone();
        move |w: &Workspace| w.project_of(id.as_str())
    };
    wait_until(d, "the move into a new Project", {
        let project_of = project_of.clone();
        move |w, window| idle(w, window) && project_of(w).is_some()
    })?;
    let project = d
        .change({
            let project_of = project_of.clone();
            move |w, _, _| project_of(w)
        })?
        .ok_or("the new Project")?;
    d.change({
        let id = id.clone();
        move |w, window, cx| w.move_document(id, None, window, cx)
    })?;
    wait_until(d, "the move to Unfiled", {
        let project_of = project_of.clone();
        move |w, window| idle(w, window) && project_of(w).is_none()
    })?;
    d.change({
        let (id, project) = (id.clone(), project.clone());
        move |w, window, cx| w.move_document(id, Some(project), window, cx)
    })?;
    wait_until(d, "the move back", {
        let project_of = project_of.clone();
        move |w, window| idle(w, window) && project_of(w).is_some()
    })?;

    // Export Project… and Export Markdown… write what their pickers would
    // have chosen: a folder in the profile.
    let exports = profile.join("exports");
    std::fs::create_dir_all(&exports).map_err(|e| e.to_string())?;
    d.change({
        let (project, exports) = (project.clone(), exports.clone());
        move |w, _, cx| -> Result<()> {
            let vault = w.vault.vault().cloned().ok_or("the Vault")?;
            vault
                .export_project(&project, &exports)
                .map_err(|e| e.to_string())?;
            let text = (w.active_view())
                .map(|view| view.read(cx).editor.read(cx).text().to_string())
                .ok_or("the document's view")?;
            mdoc_store::document::export(&exports.join("Cut-over lease.md"), &text)
                .map_err(|e| e.to_string())
        }
    })??;

    // The Project goes, its document to Unfiled; then the document goes.
    d.change({
        let project = project.clone();
        move |w, window, cx| {
            w.forget_project(
                &project,
                mdoc_vault::ProjectDocuments::MoveToUnfiled,
                window,
                cx,
            )
        }
    })?;
    wait_until(d, "the Project's delete", {
        let project_of = project_of.clone();
        move |w, window| idle(w, window) && project_of(w).is_none()
    })?;
    d.change({
        let id = id.clone();
        move |w, window, cx| w.forget_documents(vec![id], window, cx)
    })?;
    wait_until(d, "the document's delete", {
        let id = id.clone();
        move |w, window| {
            idle(w, window)
                && (w.vault.vault())
                    .is_some_and(|vault| vault.documents().iter().all(|s| s.id != id.as_str()))
        }
    })?;
    quit(d)
}

/// A window as the app opens one: the session from the profile, and the
/// Vault through the OS key, which the sandbox refuses. Then a quit.
fn launch(d: &mut Driver) -> Result<()> {
    let window =
        d.cx.open_window(size(px(super::WIDTH), px(super::HEIGHT)), |window, cx| {
            cx.new(|cx| Workspace::new(window, cx))
        })
        .map_err(|e| e.to_string())?;
    d.window = window.into();
    wait_until(d, "the launch", |w, _| w.ready)?;
    quit(d)
}

/// Quit as File › Quit does, and wait for the window to close.
fn quit(d: &mut Driver) -> Result<()> {
    d.change(|w, window, cx| w.quit(window, cx))?;
    for _ in 0..600 {
        if d.settle().is_err() {
            return Ok(());
        }
        std::thread::sleep(std::time::Duration::from_millis(100));
    }
    Err("timed out waiting for the quit".into())
}
