//! The Vault: opening a file it already holds, and Home when there is no
//! key store to keep documents with.
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
