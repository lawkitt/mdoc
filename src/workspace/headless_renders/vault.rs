//! The Vault: opening a file it already holds, and Home when there is no
//! key store to keep documents with.
use super::{Driver, Result};
use std::path::Path;

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
    let both = vec![one.clone(), two.clone()];
    d.change({
        let both = both.clone();
        move |w, window, cx| w.open_paths(both, window, cx)
    })?;
    d.settle()?;
    // Closed, they are still in the Vault: opening them again asks.
    d.change({
        let both = both.clone();
        move |w, window, cx| {
            let ids: Vec<u64> = (w.admitted_from.iter())
                .filter(|(_, from)| both.contains(from))
                .map(|(id, _)| *id)
                .collect();
            for id in ids {
                w.close_tab(id, window, cx);
            }
        }
    })?;
    d.settle()?;
    d.change(move |w, window, cx| w.open_paths(vec![one], window, cx))?;
    d.settle()?;
    d.capture("import-repeated")?;
    d.key("escape")?;
    d.change(move |w, window, cx| w.open_paths(both, window, cx))?;
    d.settle()?;
    d.capture("import-repeated-several")?;
    d.key("escape")
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
