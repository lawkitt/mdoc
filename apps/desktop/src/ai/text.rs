//! The words the AI apps' parts show: Ask's menu, prompts and notices, the
//! companion window, its hotkey's refusals, the connector's start and its
//! setup in Settings, each a constant or a function, with their counts and
//! plurals worded here. Also the debug probes' lines and the key codes the
//! hotkey hands the OS, which are no screen words and are never translated.
use super::ask::app;
use super::companion::hotkey::Refused;
use super::connector::setup::{Host, Refusal};
use mdoc_store::settings::{AskWith, Hotkey};
use std::fmt::Display;

// ---- Ask -------------------------------------------------------------------

/// What `with`'s menu row and the button's main part read.
pub(crate) fn label(with: AskWith) -> &'static str {
    match with {
        AskWith::Claude => "Ask Claude",
        AskWith::ChatGpt => "Ask in a ChatGPT thread",
        AskWith::CopyForAi => "Copy for AI",
    }
}

/// The prompt for a document the connector labels `label`, id `doc_id`.
pub(crate) fn document_prompt(label: &str, doc_id: &str) -> String {
    format!("Read \u{201c}{label}\u{201d} in mdoc (doc_id {doc_id}). My question: ")
}

/// The prompt for the shared Project as a whole.
pub(crate) const PROJECT_PROMPT: &str =
    "List my documents in mdoc and read the ones you need. My question: ";

/// The notice after `with`'s app opened, with Project `project` shared.
pub(crate) fn opened(with: AskWith, project: &str) -> String {
    let shared = format!("\u{201c}{project}\u{201d} is shared with AI apps.");
    match with {
        AskWith::ChatGpt => format!(
            "{shared} ChatGPT opened a new local Work thread with the prompt: add your question \
             and send it. Its ordinary chats can't read mdoc."
        ),
        _ => format!(
            "{shared} Claude opened a new chat with the prompt: add your question and send it."
        ),
    }
}

/// The Unfiled card's title, for `with`'s app.
pub(crate) fn unfiled_title(with: AskWith) -> String {
    format!(
        "Ask {} about an Unfiled document?",
        app(with).unwrap_or("an AI app")
    )
}

pub(crate) const UNFILED_TEXT: &str = "AI apps read only the Project you share, never Unfiled. Move it to a new Project and share \
     that?";

pub(crate) const MOVE_AND_SHARE: &str = "Move to a new Project and share";

/// The Unfiled card's text when Project `shared` is shared (#1312).
pub(crate) fn unfiled_shared_text(shared: &str) -> String {
    format!(
        "AI apps read only the Project you share, never Unfiled. Move it to \u{201c}{shared}\u{201d}, \
         the Project you share, or to a new Project?"
    )
}

/// The Unfiled card's first choice when Project `shared` is shared.
pub(crate) fn move_into_and_ask(shared: &str) -> String {
    format!("Move to \u{201c}{shared}\u{201d} and ask")
}

/// Why Ask did nothing for a document excluded from AI.
pub(crate) const EXCLUDED: &str = "This document is excluded from AI apps.";

// ---- The companion ---------------------------------------------------------

/// The companion's heading.
pub(crate) const SHARED_WITH_AI_APPS_HEADING: &str = "SHARED WITH AI APPS";
pub(crate) const SHARED_WITH_AI_APPS: &str = "Shared with AI apps";
pub(crate) const HIDE: &str = "Hide";
pub(crate) const STOP_SHARING: &str = "Stop sharing";
/// Nothing shared while the connector runs.
pub(crate) const NO_PROJECT_SHARED: &str = "No Project shared";
/// Nothing shared while the Vault is locked.
pub(crate) const NO_PROJECT_SHARED_LOCKED: &str = "No Project shared. Unlock mdoc to share one.";

/// The review card's title once the document is Ready.
pub(crate) fn review_ready(label: impl Display) -> String {
    format!("\u{201c}{}\u{201d} is Ready", label)
}
/// The review card's line once the document is Ready.
pub(crate) fn can_read_it_now(from: impl Display) -> String {
    format!("{} can read it now.", from)
}
/// The review card's title while it asks for a review.
pub(crate) fn asked_for(from: impl Display, label: impl Display) -> String {
    format!("{} asked for \u{201c}{}\u{201d}", from, label)
}
/// How a document's state starts while it needs review, as the window's
/// companion source words it.
pub(crate) const NEEDS_REVIEW: &str = "Needs review · ";
/// The review card's line for what only mdoc decides: why, if the state
/// says.
pub(crate) fn finish_in_mdoc(why: Option<&str>) -> String {
    match why {
        Some(why) => format!("Finish its review in mdoc · {why}"),
        None => "Finish its review in mdoc.".to_string(),
    }
}
/// The review card's line while it lists proposals.
pub(crate) const NEEDS_REVIEW_BEFORE: &str = "It needs review before AI apps can read it.";
pub(crate) const OPEN_IN_MDOC: &str = "Open in mdoc";
pub(crate) const REPLACE_ALL: &str = "Replace all";
pub(crate) const REPLACING: &str = "Replacing…";
/// How many names a review's proposals replace.
pub(crate) fn names_to_replace(count: usize) -> String {
    match count {
        1 => "1 name to replace".to_string(),
        n => format!("{n} names to replace"),
    }
}
/// The proposals' name for a screen reader, while they show.
pub(crate) const PROPOSALS: &str = "Proposals";
/// The proposals' name for a screen reader, while hidden.
pub(crate) fn proposals_hidden(names: impl Display) -> String {
    format!("{names}, hidden. Point at them to see them.")
}
/// The proposals' place while hidden.
pub(crate) fn point_to_see(names: impl Display) -> String {
    format!("{names}. Point here to see them.")
}
/// One proposal for a screen reader: the wording, its alias, its mentions.
pub(crate) fn proposal(original: impl Display, alias: impl Display, mentions: usize) -> String {
    format!(
        "{} becomes {}, {} {}",
        original,
        alias,
        mentions,
        if mentions == 1 { "mention" } else { "mentions" }
    )
}
pub(crate) const DOCUMENTS: &str = "Documents";
/// The documents' name for a screen reader.
pub(crate) const DOCUMENTS_AI_APPS_CAN_ASK_FOR: &str = "Documents AI apps can ask for";
pub(crate) fn documents_not_listed(error: impl Display) -> String {
    format!("The documents can't be listed. {error}")
}
pub(crate) const NO_DOCUMENTS: &str = "No documents in this Project yet.";
/// The reply's name for a screen reader, while it shows.
pub(crate) const REPLY_RESTORED: &str = "Reply with the names restored";
/// The reply's name for a screen reader, while hidden.
pub(crate) const REPLY_HIDDEN: &str =
    "Reply hidden. Point at it to read it with the names restored.";
/// The reply's place while hidden.
pub(crate) const POINT_TO_READ: &str = "Point here to read it with the names restored";
pub(crate) const COPY: &str = "Copy";
pub(crate) const SAVED_AS_DOCUMENT: &str = "Saved as a document";
pub(crate) const SAVING: &str = "Saving…";
pub(crate) const NOT_SAVED: &str = "Not saved. Try again";
pub(crate) const SAVE_AS_DOCUMENT: &str = "Save as document";
/// The reply's heading.
pub(crate) fn latest_reply(from: impl Display) -> String {
    format!("Latest reply · from {}", from)
}
/// The drop target while a Project is shared.
pub(crate) fn drop_to_add(project: impl Display) -> String {
    format!("Drop files here to add them to {}", project)
}
/// The drop target while nothing is shared.
pub(crate) const DROP_TO_OPEN: &str = "Drop files here to open them in mdoc";
pub(crate) const OPEN_MDOC: &str = "Open mdoc";
pub(crate) const SETTINGS: &str = "Settings";
pub(crate) const QUIT: &str = "Quit";

// ---- The companion's hotkey ------------------------------------------------

#[cfg_attr(not(target_os = "macos"), allow(dead_code))]
impl Refused {
    /// What Settings › AI connector shows under the row.
    pub(crate) fn message(&self, hotkey: Hotkey) -> String {
        let label = hotkey.label();
        match self {
            Refused::Taken => format!(
                "{label} is in use by another app or by macOS. Record another shortcut; the \
                 menu bar icon still opens the companion."
            ),
            Refused::Failed(why) => {
                format!("Couldn't set {label}: {why}. The menu bar icon still opens the companion.")
            }
        }
    }
}

/// Why a recorded key isn't offered.
#[cfg_attr(not(target_os = "macos"), allow(dead_code))]
pub(crate) const NOT_OFFERED: &str = "Use a letter, a digit, a symbol, Space or F1–F20.";
/// Why a recorded key needs a modifier.
#[cfg_attr(not(target_os = "macos"), allow(dead_code))]
pub(crate) const UNMODIFIED: &str = "Hold ⌘, ⌃ or ⌥ with the key, so typing never triggers it.";

// Not words: the W3C key codes `global-hotkey` parses. Never translated.

/// A function key's code, `F1` to `F20`.
#[cfg_attr(not(target_os = "macos"), allow(dead_code))]
pub(crate) fn function_key_code(n: impl Display) -> String {
    format!("F{n}")
}
/// A letter's code.
#[cfg_attr(not(target_os = "macos"), allow(dead_code))]
pub(crate) fn letter_key_code(letter: impl Display) -> String {
    format!("Key{}", letter)
}
/// A digit's code.
#[cfg_attr(not(target_os = "macos"), allow(dead_code))]
pub(crate) fn digit_key_code(c: impl Display) -> String {
    format!("Digit{c}")
}

// ---- The connector and its setup -------------------------------------------

/// The app a connector client names no product for.
pub(crate) const AN_AI_APP: &str = "An AI app";
/// The log line when the connector has nowhere to listen.
pub(crate) const NO_ENDPOINT_LOG: &str = "mdoc: the AI connector has no endpoint: no data folder";
/// Settings' line when the connector has nowhere to listen.
pub(crate) const NO_DATA_FOLDER: &str =
    "Couldn't start the connector: this system gives mdoc no data folder.";
/// The log line when the connector can't listen.
pub(crate) fn cant_listen_log(endpoint: impl Display, error: impl Display) -> String {
    format!("mdoc: the AI connector can't listen at {endpoint}: {error}")
}
/// A failed start in Settings' words (#1220, decided by default on #1091).
pub(crate) fn failed_start(error: &std::io::Error) -> String {
    if error.kind() == std::io::ErrorKind::AddrInUse {
        return "Couldn't start the connector: another copy of mdoc is using it. Quit the other \
                copy, then turn AI connector (preview) off and on."
            .into();
    }
    // A socket's path the system can't take (#1311): no retry helps.
    if error.kind() == std::io::ErrorKind::InvalidFilename {
        return "Couldn't start the connector: the path to its folder is too long for this \
                computer."
            .into();
    }
    let why = error.to_string();
    let why = why.trim_end().trim_end_matches('.');
    format!(
        "Couldn't start the connector: {why}. Turn AI connector (preview) off and on to try \
         again."
    )
}

/// What Test connection asks in the new chat.
pub(crate) const FIRST_PROMPT: &str = "List my documents in mdoc.";

impl Refusal {
    /// The sentence the page shows.
    pub(crate) fn message(&self, host: Host) -> String {
        let app = host.name();
        match self {
            Self::NotInstalled => {
                format!("{app} isn't installed on this computer, or hasn't been opened yet.")
            }
            Self::NoShim => {
                "mdoc-mcp isn't installed beside mdoc. Reinstall mdoc to add it.".into()
            }
            Self::NotInPlace => format!(
                "Move mdoc to your Applications folder first: {app} can't start it from where it \
                 runs now."
            ),
            Self::Unparsable => format!(
                "{app}'s settings file can't be read, so mdoc won't change it. Fix or remove it, \
                 then try again."
            ),
            Self::Unexpected => format!(
                "{app}'s settings file isn't in the form mdoc expects, so mdoc won't change it."
            ),
            Self::AlreadyAdded => format!("mdoc is already in {app}'s settings."),
            Self::NotAdded => format!("mdoc isn't in {app}'s settings."),
            Self::Linked => format!(
                "{app}'s settings file is a link to another file, so mdoc won't replace it. \
                 Add the entry where the link points, by hand."
            ),
            Self::Changed => format!(
                "{app}'s settings file changed since mdoc showed it. Nothing was written; look \
                 at the change again."
            ),
            Self::Io {
                writing: false,
                error,
            } => format!("mdoc couldn't read {app}'s settings file: {error}."),
            Self::Io {
                writing: true,
                error,
            } => format!(
                "mdoc couldn't write {app}'s settings file: {error}. If {app} is open, quit it \
                 and try again."
            ),
        }
    }
}

// ---- The debug probes' lines -----------------------------------------------

/// The probes' terminal lines: no screen words, never translated.
#[cfg(all(any(target_os = "macos", windows), debug_assertions, not(test)))]
pub(crate) mod probes {
    use std::fmt::Debug;
    #[cfg(target_os = "macos")]
    use std::fmt::Display;

    /// A step's verdict when a check failed.
    pub(crate) fn failed(found: impl Debug) -> String {
        format!("FAILED {found:?}")
    }
    pub(crate) const DID_NOT_OPEN: &str = "companion probe: the window did not open";

    #[cfg(target_os = "macos")]
    pub(crate) const FRONTMOST_BEFORE_PROBE: &str = "note: mdoc was frontmost before the probe; run it from a terminal with \
                 another app in front";
    #[cfg(target_os = "macos")]
    pub(crate) const NO_ACCESSIBILITY: &str = "hotkey probe: posting key events needs the Accessibility grant of the app \
                 running the probe; nothing was registered";
    #[cfg(target_os = "macos")]
    pub(crate) fn registered_failed(why: impl Display) -> String {
        format!("registered: FAILED {why}")
    }
    #[cfg(target_os = "macos")]
    pub(crate) fn registered_ok(label: impl Display) -> String {
        format!("registered: ok {}", label)
    }
    #[cfg(target_os = "macos")]
    pub(crate) fn presses_not_one(presses: impl Display) -> String {
        format!("{presses} presses reached mdoc, not 1")
    }
    #[cfg(target_os = "macos")]
    pub(crate) fn presses_not_two(presses: impl Display) -> String {
        format!("{presses} presses reached mdoc, not 2")
    }
    #[cfg(target_os = "macos")]
    pub(crate) fn frontmost(frontmost: impl Debug) -> String {
        format!("frontmost: {frontmost:?}")
    }
    #[cfg(target_os = "macos")]
    pub(crate) const FRONTMOST_BEFORE_WINDOW: &str = "note: mdoc was frontmost before the window opened; run the probe from a \
                 terminal with another app in front";

    #[cfg(windows)]
    pub(crate) const WOULD_ACTIVATE: &str = "WM_MOUSEACTIVATE would activate it";
    #[cfg(windows)]
    pub(crate) const IN_FRONT_BEFORE_WINDOW: &str = "note: mdoc was in front before the window opened; run the probe from a \
                 terminal with another app in front";
    #[cfg(windows)]
    pub(crate) fn capture_without_exclusion(share: f32) -> String {
        format!("control capture without the exclusion: {:.1} %", share)
    }
    #[cfg(windows)]
    pub(crate) const CAPTURE_MISSED: &str =
        "the control capture missed it: the capture check proves nothing";
    #[cfg(windows)]
    pub(crate) fn tray_icon(icon: impl Debug) -> String {
        format!("tray icon (added, placed): {icon:?}")
    }
    #[cfg(windows)]
    pub(crate) const TRAY_FAILED: &str = "tray: FAILED";
}
