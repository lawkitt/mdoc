//! The Vault in the window: the window's hold on it, opening and unlocking
//! it, the Keychain explainer, the recovery key and the user-wide lists.
//! The wrong-key screen's struct is here; the state, view, actions and
//! words each have their file.
mod actions;
mod state;
mod text;
mod view;

use crate::document::markdown_search::SearchInput;
use gpui::{Entity, actions};
#[cfg(any(
    test,
    all(
        debug_assertions,
        feature = "headless-renders",
        any(target_os = "macos", target_os = "windows")
    )
))]
pub(crate) use state::InjectedKey;
use state::KeyStore;
pub(in crate::workspace) use state::{Opening, VaultState};
pub(in crate::workspace) use text::WAITS_FOR_RECOVERY_KEY;

actions!(workspace, [OpenWithRecoveryKey]);

/// The key context of the wrong-key screen's field.
pub(crate) const RECOVERY_FIELD: &str = "RecoveryKey";

/// The wrong-key screen's state.
#[derive(Default)]
pub(in crate::workspace) struct Recovery {
    pub(in crate::workspace) store: KeyStore,
    pub(in crate::workspace) field: Option<Entity<SearchInput>>,
    /// Why the last key typed didn't open the Vault.
    pub(in crate::workspace) error: Option<String>,
    /// A typed key is being tried.
    opening: bool,
}
