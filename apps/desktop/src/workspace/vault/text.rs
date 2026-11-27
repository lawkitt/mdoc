//! The Vault's words: each string a user reads about the Vault, its key,
//! the Keychain explainer, the recovery key and the user-wide lists is a
//! constant or a function here.

use mdoc_store::os_key::KeyError;
use mdoc_vault::VaultError;

// ---- The Keychain explainer -----------------------------------------------

/// The explainer's title, also its name for a screen reader.
pub(crate) const EXPLAINER_TITLE: &str = "mdoc will ask for Keychain access";
/// Its three lines.
pub(crate) const EXPLAINER_ENCRYPTED: &str = "Documents and Projects are encrypted, and the key lives in your \
                                         Mac’s Keychain.";
pub(crate) const EXPLAINER_PASSWORD: &str = "macOS will ask for your Mac login password. mdoc never \
                                         sees it.";
pub(crate) const EXPLAINER_ALWAYS_ALLOW: &str = "Choose Always Allow so it isn’t asked again.";
/// The preview's caption and its name for a screen reader.
pub(crate) const PREVIEW_CAPTION: &str = "After Continue, macOS shows this:";
pub(crate) const PREVIEW_LABEL: &str = "Preview of the macOS Keychain dialog";
/// The explainer's button.
pub(crate) const CONTINUE: &str = "Continue";
/// What the drawn macOS dialog says, as macOS words it.
pub(crate) const PREVIEW_ASKS: &str = "mdoc wants to use your confidential information stored \
                                     in “io.github.lawkitt.mdoc” in your keychain.";
pub(crate) const PREVIEW_PASSWORD_LINE: &str =
    "To allow this, enter the “login” keychain password.";
pub(crate) const PREVIEW_PASSWORD: &str = "Password:";
pub(crate) const PREVIEW_ALWAYS_ALLOW: &str = "Always Allow";
pub(crate) const PREVIEW_DENY: &str = "Deny";
pub(crate) const PREVIEW_ALLOW: &str = "Allow";

// ---- The recovery key -----------------------------------------------------

/// Why the key store gave no key.
pub(crate) const KEY_NEEDS_ASKING: &str = "mdoc could not read the key without asking.";
pub(crate) const NO_KEY_HELD: &str = "No key is held.";
pub(crate) const KEY_STORE_REFUSED: &str = "The key store refused it.";
/// Settings › Vault's Show, before the documents open or with the wrong key.
pub(crate) const KEY_SHOWS_ONCE_OPEN: &str =
    "Your recovery key shows here once your documents are open.";
pub(crate) const KEY_DOES_NOT_OPEN: &str = "The key on this computer doesn't open these documents. Quit mdoc \
                         and open it again to enter your recovery key.";
/// The nudge's dismissal could not be saved.
pub(crate) fn reminder_not_saved(error: impl std::fmt::Display) -> String {
    format!("Could not save that the reminder was closed: {error}")
}
/// The first-Project nudge and its buttons.
pub(crate) const NUDGE: &str = "Save your recovery key. It's the only way to open your documents \
                             if this computer's key is lost, as after reinstalling the system.";
pub(crate) const NOT_NOW: &str = "Not Now";
pub(crate) const SHOW_RECOVERY_KEY: &str = "Show Recovery Key";
/// The recovery panel on Home.
pub(crate) const ENTER_RECOVERY_KEY: &str = "Enter your recovery key";
pub(crate) const RECOVERY_DETAIL: &str = "Your documents were saved with a different key, as after reinstalling \
                         the system or copying them from another computer. The recovery key \
                         from Settings › Vault on that computer opens them, and this computer \
                         keeps it.";
pub(crate) const OPEN_DOCUMENTS: &str = "Open Documents";
/// The recovery field's placeholder and label.
pub(crate) const RECOVERY_PLACEHOLDER: &str = "ABCD EFGH IJKL …";
pub(crate) const RECOVERY_KEY: &str = "Recovery key";
/// The documents opened, but the key could not be kept.
pub(crate) fn key_not_kept(why: impl std::fmt::Display) -> String {
    format!(
        "Your documents are open, but this computer couldn't keep the \
                                 recovery key, so mdoc asks for it at the next launch. {why}"
    )
}
/// A recovery key that did not open the documents.
pub(crate) const WRONG_RECOVERY_KEY: &str =
    "That recovery key doesn't open these documents. Check each group and try again.";
pub(crate) fn could_not_open_with_it(error: impl std::fmt::Display) -> String {
    format!("mdoc could not open the documents with it. {error}")
}

// ---- The user-wide lists --------------------------------------------------

pub(crate) fn lists_not_opened(error: impl std::fmt::Display) -> String {
    format!("Your Always and Never lists could not be opened: {error}")
}
pub(crate) const LISTS_UNAVAILABLE: &str =
    "Could not save your Always and Never lists: documents are unavailable.";
pub(crate) fn lists_not_saved(error: impl std::fmt::Display) -> String {
    format!("Could not save your Always and Never lists: {error}")
}

// ---- Opening the Vault ----------------------------------------------------

/// Intake refused while the Vault waits for its recovery key. The key
/// opening the Vault withdraws it (#1161).
pub(crate) const WAITS_FOR_RECOVERY_KEY: &str = "Documents can't be added until your recovery \
                                                 key opens the documents you have. Enter it on \
                                                 Home.";
/// A tab that waited for the Vault, when it can't be had.
pub(crate) fn document_cannot_open(why: impl std::fmt::Display) -> String {
    format!("This document can't open. {why}")
}
/// Intake with no Vault this run.
pub(crate) fn cannot_add(why: impl std::fmt::Display) -> String {
    format!("Documents can't be added. {why}")
}

/// Why no key could be read.
pub(crate) fn key_unavailable(error: &KeyError) -> String {
    match error {
        KeyError::NoKeyStore => "mdoc keeps documents encrypted with a key held by the macOS \
            Keychain, Windows data protection or a Secret Service keyring, and this system has \
            none."
            .into(),
        KeyError::KeyringUnavailable(why) => format!(
            "mdoc keeps documents encrypted with a key in a Secret Service keyring (GNOME \
             Keyring or KWallet), which is not available: {why}"
        ),
        error => format!("mdoc could not read the key that encrypts documents. {error}"),
    }
}

/// Why the Vault could not be opened.
pub(crate) fn vault_unavailable(error: &VaultError) -> String {
    match error {
        VaultError::WrongKey => {
            "mdoc's documents were encrypted with a different key, so they can't be opened.".into()
        }
        error => format!("mdoc could not open its documents. {error}"),
    }
}

// ---- The banner -----------------------------------------------------------

/// The banner while documents wait for the recovery key, Keychain access
/// or the key, with its button.
pub(crate) const SAVED_WITH_DIFFERENT_KEY: &str = "Your documents were saved with a different key. Enter your recovery key on \
                 Home to open them.";
pub(crate) const NEEDS_KEYCHAIN: &str = "Your documents need Keychain access to open.";
pub(crate) const TRY_AGAIN: &str = "Try again…";
pub(crate) const LOCKED: &str = "Your documents are locked. Unlock them to open them.";
pub(crate) const UNLOCK: &str = "Unlock…";
