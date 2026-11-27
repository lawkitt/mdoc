//! The Vault: opening a file it already holds, Home when there is no key
//! store to keep documents with, and the recovery key: Settings › Vault,
//! the first-Project nudge and the wrong-key screen.
pub(super) mod cut_over;
pub(super) mod old_cases;
use super::{Driver, Result, Workspace};
use std::path::{Path, PathBuf};

/// The repeated-import question, for one file and for several; each is
/// cancelled. The files are imported and closed first, beside the fixture.
pub(super) fn repeated_import(d: &mut Driver, folder: &Path) -> Result<()> {
    let one = folder.join("Engagement letter.md");
    let two = folder.join("Board minutes.md");
    for (path, text) in [
        (
            &one,
            "# Engagement letter
",
        ),
        (
            &two,
            "# Board minutes
",
        ),
    ] {
        std::fs::write(path, text).map_err(|e| e.to_string())?;
    }
    // Opened files are known by their normalized path: on macOS a temporary
    // folder's /var is /private/var.
    let both: Vec<PathBuf> = [&one, &two]
        .into_iter()
        .map(|path| mdoc_store::session_store::identity(path))
        .collect();
    let open_tabs = {
        let both = both.clone();
        move |w: &Workspace| -> Vec<u64> {
            (w.admitted_from.iter())
                .filter(|(_, from)| both.contains(from))
                .map(|(id, _)| *id)
                .collect()
        }
    };
    d.change({
        let both = both.clone();
        move |w, window, cx| w.open_paths(both, window, cx)
    })?;
    // Imports run on a background thread: give them real time. The second
    // may be evicted at once: it was never viewed.
    wait_until(d, "the imports", |w, _| {
        let titles: Vec<String> = (w.vault.vault().into_iter())
            .flat_map(|vault| vault.documents())
            .map(|summary| summary.title)
            .collect();
        !w.admitting
            && ["Engagement letter", "Board minutes"]
                .iter()
                .all(|title| titles.iter().filter(|t| t == title).count() == 1)
    })?;
    // Released, they are still in the Vault: opening them again asks.
    d.change({
        let open_tabs = open_tabs.clone();
        move |w, window, cx| {
            for id in open_tabs(w) {
                w.close_tab(id, window, cx);
            }
        }
    })?;
    wait_until(d, "the closes", |w, _| open_tabs(w).is_empty())?;
    d.change({
        let one = both[0].clone();
        move |w, window, cx| w.open_paths(vec![one], window, cx)
    })?;
    wait_until(d, "the question", |_, window| window.has_active_prompt())?;
    d.capture("import-repeated")?;
    d.key("escape")?;
    wait_until(d, "the answer", |_, window| !window.has_active_prompt())?;
    d.change(move |w, window, cx| w.open_paths(both, window, cx))?;
    wait_until(d, "the question", |_, window| window.has_active_prompt())?;
    d.capture("import-repeated-several")?;
    d.key("escape")
}

/// A converted document whose original is gone: its saved Markdown opens,
/// and the Source pane beside it says why it shows nothing. The tab closes
/// after.
pub(super) fn original_missing(d: &mut Driver) -> Result<()> {
    let id = d.change(|w, _, _| -> Result<mdoc_store::document_id::VaultId> {
        let vault = w.vault.vault().ok_or("the theme's Vault")?.clone();
        let new = mdoc_vault::NewDocument {
            project: None,
            title: "Lease extension".into(),
            source_kind: mdoc_vault::SourceKind::Pdf,
            origin: mdoc_vault::DocumentOrigin::Ordinary,
            markdown: "# Lease extension\n\nThe term is extended to 31 December, on the \
                       terms of the original lease.\n"
                .into(),
        };
        let info = mdoc_vault::OriginalInfo {
            file_name: "Lease extension.pdf".into(),
            ..mdoc_vault::OriginalInfo::default()
        };
        let document = (vault.import_document(new, &b"%PDF-1.4\n%%EOF\n"[..], info))
            .map_err(|e| e.to_string())?;
        // Gone, as a damaged or deleted file would be.
        let original = (vault.root().join("docs").join(&document.id)).join("original.enc");
        std::fs::remove_file(original).map_err(|e| e.to_string())?;
        mdoc_store::document_id::VaultId::parse(&document.id).ok_or("the document's id".into())
    })??;
    d.change({
        let id = id.clone();
        move |w, window, cx| w.open_vault_document(id, window, cx)
    })?;
    // Loaded off the main thread: give it real time.
    let mut loaded = false;
    for _ in 0..600 {
        d.settle()?;
        loaded = d.change(|w, _, cx| {
            (w.active_view()).is_some_and(|view| {
                let view = view.read(cx);
                view.lifecycle.ready() && view.preview.failed()
            })
        })?;
        if loaded {
            break;
        }
        std::thread::sleep(std::time::Duration::from_millis(100));
    }
    if !loaded {
        return Err("timed out waiting for the document".into());
    }
    d.capture("document-original-missing")?;
    d.change(|w, window, cx| {
        let tab = w.tabs.active();
        w.close_tab(tab, window, cx);
    })
}

/// Settle until `done` holds, for up to a minute of real time.
pub(super) fn wait_until(
    d: &mut Driver,
    what: &str,
    done: impl Fn(&Workspace, &gpui::Window) -> bool,
) -> Result<()> {
    for _ in 0..600 {
        d.settle()?;
        if d.change(|w, window, _| done(w, window))? {
            return Ok(());
        }
        std::thread::sleep(std::time::Duration::from_millis(100));
    }
    Err(format!("timed out waiting for {what}"))
}

/// Home and its banner with the Vault Unavailable; the Vault opens again
/// after.
pub(super) fn unavailable_home(d: &mut Driver) -> Result<()> {
    let vault = d.change(|w, window, cx| {
        let vault = w.vault_unavailable_for_render();
        w.show_home(window, cx);
        cx.notify();
        vault
    })?;
    d.capture("home-vault-unavailable")?;
    d.change(|w, _, cx| {
        w.title.notices_open.set(true);
        cx.notify();
    })?;
    d.capture("home-vault-unavailable-notice")?;
    d.change(|w, window, cx| {
        w.close_notices(window, cx);
        if let Some(vault) = vault {
            w.restore_vault_for_render(vault);
        }
        cx.notify();
    })
}

/// Settings › Vault before and with the recovery key, the first-Project
/// nudge, and Home asking for the recovery key, empty and refused. The key
/// is the injected test key; the Vault is open again after.
pub(super) fn recovery_key(d: &mut Driver) -> Result<()> {
    use crate::settings::Category;
    d.change(|w, window, cx| {
        w.use_key_store(std::sync::Arc::new(std::sync::Mutex::new(
            crate::workspace::vault::recovery_key::InjectedKey {
                key: Some([3; 32]),
                ..Default::default()
            },
        )));
        w.settings
            .update(cx, |p, cx| p.show_category(Category::Vault, window, cx));
    })?;
    d.capture("settings-vault")?;
    d.change(|w, _, cx| w.show_recovery_key(cx))?;
    d.capture("settings-vault-recovery-key")?;
    // The first Project brings the nudge, counted on the bell, which the
    // scene opens: the nudge doesn't open it by itself (#1286).
    let project = d.change(|w, window, cx| {
        w.settings.update(cx, |p, cx| p.close(window, cx));
        w.show_home(window, cx);
        let vault = w.vault.vault().cloned().ok_or("no Vault")?;
        let project = vault
            .create_project(mdoc_vault::NewProject {
                name: "Smith v. Jones".into(),
                ..Default::default()
            })
            .map_err(|e| e.to_string())?;
        let id = project.id.clone();
        w.projects.put(project);
        // Every other scene renders with the nudge closed.
        w.update_general(|g| g.recovery_key_nudge_dismissed = false, cx)
            .map_err(|e| e.to_string())?;
        w.title.notices_open.set(true);
        cx.notify();
        Ok::<_, String>((vault, id))
    })??;
    d.settle()?;
    d.capture("vault-recovery-nudge")?;
    d.change(move |w, window, cx| {
        let (vault, id) = project;
        let _ = vault.delete_project(&id, mdoc_vault::ProjectDocuments::MoveToUnfiled);
        w.projects.remove(&id);
        let _ = w.update_general(|g| g.recovery_key_nudge_dismissed = true, cx);
        w.close_notices(window, cx);
        cx.notify();
    })?;
    // Sealed with another key: Home asks for the recovery key.
    let vault = d.change(|w, window, cx| {
        let vault = w.vault_wrong_key_for_render(window, cx);
        cx.notify();
        vault
    })?;
    d.capture("home-vault-wrong-key")?;
    d.change(|w, window, cx| {
        let typed = mdoc_vault::recovery::format(&[7; 32]);
        w.refuse_recovery_key_for_render(&typed, window, cx);
    })?;
    d.capture("home-vault-wrong-key-error")?;
    d.change(|w, _, cx| {
        if let Some(vault) = vault {
            w.restore_vault_for_render(vault);
        }
        w.recovery_screen_closed_for_render();
        cx.notify();
    })
}
