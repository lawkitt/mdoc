//! The Vault: Home when there is no key store to keep documents with.
use super::{Driver, Result};

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
