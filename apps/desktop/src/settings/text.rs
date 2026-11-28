//! The words Settings shows: every string a user reads on its pages is a
//! constant or a function here, and its counts are worded only here.
use super::{Category, Status};
use std::fmt::Display;

// The dialog and its categories.

impl Category {
    pub fn label(self) -> &'static str {
        match self {
            Self::General => "General",
            Self::Detection => "Detection",
            Self::Recognition => "Recognition",
            Self::Spelling => "Spelling",
            Self::Vault => "Vault",
            Self::AiConnector => "AI connector",
            Self::About => "About",
        }
    }
}

pub(super) const SETTINGS: &str = "Settings";
pub(super) const CLOSE_SETTINGS: &str = "Close Settings";
pub(super) const SETTINGS_CATEGORIES: &str = "Settings categories";
pub(super) const RESET: &str = "Reset";
pub(super) const RESET_DESCRIPTION: &str = "Restore this category's defaults.";

/// What a failed save of the choices says.
pub(super) fn not_saved(e: impl Display) -> String {
    format!("Could not save settings: {e}. Previous defaults remain active.")
}

// Buttons several pages share.

pub(super) const REMOVE: &str = "Remove";
pub(super) const REMOVE_ELLIPSIS: &str = "Remove…";
pub(super) const REPAIR: &str = "Repair";
pub(super) const COMPARE: &str = "Compare";
pub(super) const SHOW: &str = "Show";
pub(super) const HIDE: &str = "Hide";
pub(super) const ADD: &str = "Add";
pub(super) const UNDO: &str = "Undo";
pub(super) const OPEN_LINK: &str = "Open ↗";

// General.

pub(super) const APPEARANCE: &str = "Appearance";
pub(super) const APPEARANCE_DESCRIPTION: &str = "Follow the system or choose a theme.";
pub(super) const THEME: &str = "Theme";
pub(super) const SYSTEM: &str = "System";
pub(super) const LIGHT: &str = "Light";
pub(super) const DARK: &str = "Dark";
/// The description beside the shelf's "Show mdoc in the menu bar" switch.
pub(super) const KEEP_RUNNING: &str = "mdoc keeps running after you close its windows.";
pub(super) const KEYBOARD_SHORTCUTS: &str = "Keyboard shortcuts";
pub(super) const KEYBOARD_SHORTCUTS_DESCRIPTION: &str =
    "Browse the keys available in each part of mdoc.";

// General › updates.

pub(super) const AUTOMATIC_UPDATES: &str = "Automatic updates";
pub(super) const AUTOMATIC_UPDATES_DESCRIPTION: &str = "Check when mdoc starts.";
/// The automatic updates switch, as a screen reader names it.
pub(super) const CHECK_WHEN_MDOC_STARTS: &str = "Check when mdoc starts";
pub(super) const INCLUDE_PRERELEASES: &str = "Include pre-releases";
pub(super) const PRERELEASE_BUILD: &str = "This build always includes pre-releases.";
pub(super) const PRERELEASES_DESCRIPTION: &str = "Also offer versions published for testing.";
pub(super) const CHECK_NOW: &str = "Check now";
pub(super) const TRY_AGAIN: &str = "Try again";
pub(super) const DOWNLOAD_AND_INSTALL: &str = "Download and install";
pub(super) const RESTART_NOW: &str = "Restart now";
pub(super) const VIEW_RELEASE: &str = "View release ↗";
pub(super) const CHECKING_FOR_UPDATES: &str = "Checking for updates…";

pub(super) fn downloading_update(version: impl Display, percent: impl Display) -> String {
    format!("Downloading mdoc {version}… {}%", percent)
}

pub(super) fn update_ready(version: impl Display) -> String {
    format!("mdoc {version} is ready. It replaces this version when mdoc restarts or quits.")
}

pub(super) fn update_failed(version: impl Display, message: impl Display) -> String {
    format!("Couldn't update to mdoc {version}. {message}")
}

pub(super) fn update_available(available: impl Display, current: impl Display) -> String {
    format!("mdoc {} is available. You have {}.", available, current)
}

pub(super) fn latest_version(current: impl Display) -> String {
    format!("mdoc {} is the latest version.", current)
}

pub(super) fn current_version(current: impl Display) -> String {
    format!("You have mdoc {}.", current)
}

// Detection and Recognition.

pub(super) const DETECTION_NOTE: &str =
    "Detection can miss names and identifiers — review the whole document before sharing.";
pub(super) const RECOGNITION_NOTE: &str =
    "Default when a document suggests no language. Each document’s card picks its own.";
/// The Detection model list, as a screen reader names it.
pub(super) const PSEUDONYMIZATION: &str = "Pseudonymization";
/// The Recognition model list, as a screen reader names it.
pub(super) const TEXT_RECOGNITION: &str = "Text recognition";
pub(super) const COMPARE_PSEUDONYMIZATION: &str = "Compare pseudonymization";
pub(super) const COMPARE_TEXT_RECOGNITION: &str = "Compare text recognition";
pub(super) const COMPARE_DESCRIPTION: &str =
    "Compare models on a snapshot without changing your document or defaults.";
pub(super) const RESOLUTION: &str = "Resolution";
pub(super) const RESOLUTION_DESCRIPTION: &str = "Pixels per inch for recognized pages.";

pub(super) fn dpi(dpi: impl Display) -> String {
    format!("{dpi} DPI")
}

pub(super) const DETECTION_THRESHOLD: &str = "Detection threshold";
pub(super) const THRESHOLD_DESCRIPTION: &str =
    "From 0 to 1. Higher values return fewer candidates.";
/// The confidence field, as a screen reader names it.
pub(super) const OCR_MINIMUM_CONFIDENCE: &str = "OCR minimum confidence";
pub(super) const MINIMUM_CONFIDENCE: &str = "Minimum confidence";
pub(super) const CONFIDENCE_DESCRIPTION: &str = "From 0 to 1. Confidence is not measured accuracy.";
/// A number field's placeholder.
pub(super) const NUMBER_PLACEHOLDER: &str = "0–1";
pub(super) const NUMBER_OUT_OF_RANGE: &str =
    "Enter a number from 0 to 1. The previous value is still active.";
pub(super) const CATEGORIES_TO_DETECT: &str = "Categories to detect";
pub(super) const CATEGORIES_TO_DETECT_DESCRIPTION: &str = "Scans of documents outside a Project skip categories that are off, and new \
                 Projects start with this list. Names you mark yourself are still replaced.";

// The models: their rows, checks and setups.

pub(super) fn status_label(status: &Status) -> String {
    match status {
        Status::Unknown => "Not checked",
        Status::Missing => "Not downloaded",
        Status::Ready => "Installed",
        Status::Damaged(_) => "Repair needed",
        Status::Unavailable(_) => "Unavailable",
    }
    .into()
}

pub(super) const CHECKING: &str = "Checking…";
pub(super) const RECOMMENDED: &str = "Recommended";
pub(super) const DOWNLOADS_ON_FIRST_USE: &str = "Downloads on first use";
pub(super) const LESS: &str = "Less ↑";
pub(super) const DETAILS: &str = "Details ↓";
pub(super) const MODEL_CARD: &str = "Model card ↗";
pub(super) const PINNED_FILES: &str = "Pinned files ↗";
pub(super) const REMOVE_MODEL_KEEPS: &str = "Shared runtimes and documents are retained.";
pub(super) const OCR_RUNTIME: &str = "CPU · ONNX Runtime 1.32.0 · PDFium native-v7988\nFixed upstream precision · converter-managed threads\nLimits: 256 MiB PDF · 4-page batches · 256 MiB bitmap/page";
pub(super) const PII_RUNTIME: &str = "CPU · ONNX Runtime 1.32.0 · 4 threads\nLimits: 2 MiB source · 512 tokens/window · 120 s cooperative deadline";
pub(super) const MODEL_FILES_CHANGED: &str = "Model files changed. Retry setup.";
pub(super) const ANOTHER_MODEL_JOB: &str = "Another model job is running. Retry when it finishes.";
/// What other screens say the model slot is busy with while Settings checks.
#[cfg(not(test))]
pub(super) const CHECKING_INSTALLED_MODELS: &str = "checking installed models";
/// The phase mdoc-detect's and mdoc-import's setups report while they check
/// the runtime: a setup that fails in it finds the runtime unavailable.
pub(super) const CHECKING_RUNTIME: &str = "Checking runtime";

pub(super) fn download(megabytes: impl Display) -> String {
    format!("Download · {} MB", megabytes)
}

/// A page's note, with the shared runtime's size while it isn't downloaded.
pub(super) fn with_runtime(text: &str, megabytes: impl Display) -> String {
    format!(
        "{text} The first download adds a {} MB shared runtime.",
        megabytes
    )
}

/// A model row's second line: what it is, its size and status, and a note.
pub(super) fn model_line(
    summary: impl Display,
    megabytes: impl Display,
    status_label: &str,
    note: Option<&str>,
) -> String {
    match note {
        Some(note) => format!("{} · {} MB · {status_label} · {note}", summary, megabytes),
        None => format!("{} · {} MB · {status_label}", summary, megabytes),
    }
}

pub(super) fn model_size(name: impl Display, megabytes: impl Display) -> String {
    format!("{} · {} MB", name, megabytes)
}

pub(super) fn languages(languages: impl Display) -> String {
    format!("Languages: {}", languages)
}

pub(super) fn model_facts(revision: &str, license: &str, storage: impl Display) -> String {
    format!(
        "Revision: {revision}\nLicenses: {license}\nStorage: {}",
        storage
    )
}

pub(super) fn remove_model_files(name: impl Display) -> String {
    format!("Remove {} model files?", name)
}

/// Logged when the record of a setup could not be written.
pub(super) fn setup_not_remembered(error: impl Display) -> String {
    format!("Could not remember model setup: {error}")
}

/// Logged when the record of a check could not be written.
pub(super) fn checks_not_remembered(error: impl Display) -> String {
    format!("Could not remember model checks: {error}")
}

// Detection's Always and Never lists.

pub(super) const ADD_ALWAYS_WORDING: &str = "Add a wording to always replace";
pub(super) const ADD_NEVER_WORDING: &str = "Add a wording to never replace";
pub(super) const ALWAYS_AND_NEVER: &str = "Always and never replace";
pub(super) const LISTS_DESCRIPTION: &str = "Scans in every document propose these wordings, or never do. A Project's own \
                 lists, from what you keep and mark in it, come first.";
pub(super) const LISTS_LOCKED: &str = "Your lists are kept with your documents, which are locked.";
pub(super) const UNLOCK: &str = "Unlock…";
pub(super) const ALWAYS_REPLACE: &str = "Always replace";
pub(super) const NEVER_REPLACE: &str = "Never replace";
pub(super) const NO_WORDINGS: &str = "No wordings yet.";
pub(super) const NO_NEVER_WORDINGS: &str =
    "No wordings yet. Courts and public bodies are left as they are unless you add them to Always.";
pub(super) const ADD_TO_ALWAYS: &str = "Add to Always replace";
pub(super) const ADD_TO_NEVER: &str = "Add to Never replace";

pub(super) fn remove_wording(wording: &str) -> String {
    format!("Remove “{wording}”")
}

// Spelling and the dictionary.

pub(super) const CHECK_SPELLING: &str = "Check spelling";
pub(super) const CHECK_SPELLING_DESCRIPTION: &str =
    "Flags likely typos and recognition errors. Code, links and detected names are skipped.";
pub(super) const ENGLISH: &str = "English";
pub(super) const ENGLISH_DESCRIPTION: &str = "Check words against the bundled English dictionary.";
pub(super) const RUSSIAN: &str = "Russian";
pub(super) const RUSSIAN_DESCRIPTION: &str = "Check words against the bundled Russian dictionary.";
pub(super) const CHECK_ALL_CAPS: &str = "Check ALL-CAPS words";
pub(super) const CHECK_ALL_CAPS_DESCRIPTION: &str = "Include words written entirely in capitals.";
pub(super) const CHECK_DIGITS: &str = "Check words with digits";
pub(super) const CHECK_DIGITS_DESCRIPTION: &str = "Include words that contain numbers.";
pub(super) const YOUR_DICTIONARY: &str = "Your dictionary";
pub(super) const BACK_TO_SPELLING: &str = "← Spelling";
pub(super) const MANAGE: &str = "Manage…";
pub(super) const SEARCH_OR_ADD_WORD: &str = "Search or add a word";
pub(super) const NO_WORDS_YET: &str =
    "No words yet. Add them with “Add to dictionary” in the editor.";
pub(super) const ALREADY_IN_DICTIONARY: &str = "Already in your dictionary";
pub(super) const NOT_A_WORD: &str =
    "Enter a single word with at least one letter, up to 64 characters.";
pub(super) const DICTIONARY_EMPTY: &str = "Your dictionary is empty. Type a word above to add it, or use “Add to dictionary” in the editor.";
pub(super) const NO_MATCHING_WORDS: &str = "No matching words.";

/// How many words the dictionary holds.
pub(super) fn words(n: usize) -> String {
    match n {
        1 => "1 word".to_string(),
        n => format!("{n} words"),
    }
}

/// How many of the dictionary's words match the search.
pub(super) fn words_of(shown: usize, all: usize) -> String {
    format!("{} of {}", shown, all)
}

pub(super) fn add_word(word: &str) -> String {
    format!("Add “{word}”")
}

pub(super) fn remove_from_dictionary(word: &str) -> String {
    format!("Remove {word} from your dictionary")
}

pub(super) fn removed_word(word: &str) -> String {
    format!("Removed “{word}”")
}

// Vault.

pub(super) const RECOVERY_KEY: &str = "Recovery key";
pub(super) const SHOW_RECOVERY_KEY: &str = "Show Recovery Key";
pub(super) const RECOVERY_KEY_DESCRIPTION: &str = "Opens your documents if this computer's key is lost, for example after \
             reinstalling the system or on a new computer. Write it down or scan the QR code, \
             and keep it somewhere safe: anyone with it and a copy of your documents can read \
             them.";
pub(super) const RECOVERY_KEY_NOTE: &str = "mdoc never saves the recovery key. With it, a copy of the vault folder \
                         opens on another computer: mdoc asks for the key there.";
pub(super) const RECOVERY_QR: &str = "QR code of the recovery key";

pub(super) fn qr_failed(e: impl Display) -> String {
    format!("The QR code could not be made: {e}")
}

// AI connector.

pub(super) const AI_CONNECTOR_PREVIEW: &str = "AI connector (preview)";
pub(super) const AI_CONNECTOR_DESCRIPTION: &str = "Lets Claude Desktop or ChatGPT read the Ready documents of a Project you share, \
             and send its answers back to mdoc. Only pseudonymized text leaves; mdoc restores \
             the names.";
pub(super) const OPEN_DELIVERIES: &str = "Open deliveries when they arrive";
pub(super) const OPEN_DELIVERIES_DESCRIPTION: &str =
    "A reply or document an AI app sends opens in front, unless a dialog is open.";
pub(super) const CONNECTOR_PROGRAM: &str = "Connector program: ";
pub(super) const COPY_PATH: &str = "Copy path";
pub(super) const COMPANION_SHORTCUT: &str = "Companion shortcut";
pub(super) const TYPE_A_SHORTCUT: &str = "Type a shortcut…";
pub(super) const HOTKEY_DESCRIPTION: &str =
    "Shows or hides the companion from any app, without leaving it.";
/// Added to the shortcut's description while the menu bar item is off.
pub(super) const HOTKEY_NEEDS_MENU_BAR: &str =
    " Works while “Show mdoc in the menu bar” is on, in General.";
pub(super) const PRESS_NEW_SHORTCUT: &str = "Press the new shortcut with ⌘, ⌃ or ⌥. Esc cancels.";
pub(super) const NOT_ADDED: &str = "Not added.";
pub(super) const ADDED_CLAUDE: &str = "Added. Quit and reopen Claude Desktop to load it.";
pub(super) const ADDED_OTHER: &str = "Added, but pointing at another copy of mdoc-mcp.";
pub(super) const ADD_ELLIPSIS: &str = "Add…";
pub(super) const UPDATE_ELLIPSIS: &str = "Update…";
pub(super) const TEST_CONNECTION: &str = "Test connection";
pub(super) const CHANGES_ONE_FILE: &str = "mdoc will change one file:";
pub(super) const TAKES_OUT_ENTRY: &str = "It takes out this entry:";
/// Added to the consent card's closing line for ChatGPT's file.
pub(super) const CODEX_READS_TOO: &str = " Codex reads this file too.";

/// The companion's recorder, as a screen reader names it.
pub(super) fn record_hotkey(shortcut: impl Display) -> String {
    format!("Companion shortcut, {}. Click to record another", shortcut)
}

pub(super) fn added_chatgpt(prompt: impl Display) -> String {
    format!(
        "Added. Quit and reopen ChatGPT, then in a Work thread ask: “{}” ChatGPT \
                     asks before it uses each mdoc tool. Codex reads this setting too.",
        prompt
    )
}

/// What the last Add or Remove did.
pub(super) fn changed(removed: bool, app: impl Display) -> String {
    format!(
        "{}. Quit and reopen {} to {} mdoc.",
        if removed { "Removed" } else { "Added" },
        app,
        if removed { "unload" } else { "load" },
    )
}

pub(super) fn add_to(app: impl Display) -> String {
    format!("Add to {app}")
}

pub(super) fn remove_from(app: impl Display) -> String {
    format!("Remove from {app}")
}

/// What Claude's Test connection opened.
pub(super) fn test_opened(prompt: impl Display) -> String {
    format!(
        "A new chat opened with “{}” filled in. Send it; Claude asks before it uses \
                     each mdoc tool.",
        prompt
    )
}

pub(super) fn add_title(app: impl Display) -> String {
    format!("Add mdoc to {app}?")
}

pub(super) fn add_says(servers: impl Display) -> String {
    format!("It adds this entry under {servers}:")
}

pub(super) fn update_title(app: impl Display) -> String {
    format!("Update mdoc in {app}?")
}

pub(super) fn update_says(servers: impl Display) -> String {
    format!("It replaces mdoc's entry under {servers} with this one:")
}

pub(super) fn update_app(app: impl Display) -> String {
    format!("Update {app}")
}

pub(super) fn remove_title(app: impl Display) -> String {
    format!("Remove mdoc from {app}?")
}

pub(super) fn remove_says(servers: impl Display) -> String {
    format!("It removes this entry under {servers}:")
}

/// Added to the consent card's closing line when the file is kept as it was.
pub(super) fn keeps_backup(name: impl Display) -> String {
    format!(" mdoc keeps the file as it was beside it, as {}.", name)
}

/// The consent card's closing line.
pub(super) fn everything_else(
    backup: &str,
    shared: &str,
    app: impl Display,
    remove: bool,
) -> String {
    let reopen = if remove { "unload" } else { "load" };
    format!(
        "Everything else in the file stays as it is.{backup}{shared} Quit and reopen \
                 {app} to {reopen} mdoc."
    )
}

// About.

pub(super) const APP_NAME: &str = "mdoc";
/// About's build line in test builds, which are never signed.
pub(super) const DEV_BUILD: &str = "Dev";
pub(super) const HELP: &str = "Help";
pub(super) const HELP_DESCRIPTION: &str = "Learn how to prepare and review documents.";
pub(super) const WEBSITE: &str = "Website";
pub(super) const WEBSITE_DESCRIPTION: &str = "Visit the mdoc project.";
pub(super) const REPORT_ISSUE: &str = "Report an issue";
pub(super) const REPORT_ISSUE_DESCRIPTION: &str = "Describe a problem or suggest an improvement.";
pub(super) const APP_LICENSE: &str = "App license";
pub(super) const APP_LICENSE_DESCRIPTION: &str = "GNU General Public License, version 3 or later.";
pub(super) const PRIVACY_NOTE: &str = "Your documents stay on this computer. Processing is local; no document text is sent to an AI service. Update checks send only the app version. Models and updates download only on request.";

pub(super) fn version(version: impl Display, build: impl Display) -> String {
    format!("Version {} · {}", version, build)
}
