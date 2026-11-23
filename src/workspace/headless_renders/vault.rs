//! The Vault: opening a file it already holds, Home when there is no key
//! store to keep documents with, and the recovery key: Settings › Vault,
//! the first-Project nudge and the wrong-key screen.
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
    // Imports run on a background thread: give them real time.
    wait_until(d, "the imports", |w, _| open_tabs(w).len() == 2)?;
    // Closed, they are still in the Vault: opening them again asks.
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

/// Settle until `done` holds, for up to a minute of real time.
fn wait_until(
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
    use crate::settings_ui::Category;
    d.change(|w, window, cx| {
        w.use_key_store(std::sync::Arc::new(std::sync::Mutex::new(
            super::super::recovery_key::InjectedKey {
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
    // The first Project brings the nudge, which opens the bell.
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
