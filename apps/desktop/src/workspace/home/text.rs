//! Home's words: each string a user reads on Home is a constant or a
//! function here, with its counts and ages worded only here.

/// The header's title.
pub(crate) const HOME: &str = "Home";
/// The greeting over the drop zone.
pub(crate) const HELLO: &str = "Hello";
/// The line under the greeting.
pub(crate) const PREPARE_FOR_AI: &str = "What would you like to prepare for AI?";

/// The banner after an update installed.
pub(crate) fn installed(version: impl std::fmt::Display) -> String {
    format!("mdoc {version} is installed.")
}
/// The banner's link to the release notes.
pub(crate) const WHATS_NEW: &str = "What's new";

/// The drop zone's title with no Vault this run.
pub(crate) const DOCUMENTS_UNAVAILABLE: &str = "Documents are unavailable";
/// The drop zone's title while nothing dragged over it can be opened.
pub(crate) const CANT_BE_OPENED: &str = "These files can't be opened";
/// The drop zone's title while one droppable file is dragged over it.
pub(crate) const DROP_TO_OPEN: &str = "Drop to open";
/// The drop zone's title while several droppable files are dragged over it.
pub(crate) fn drop_files(n: usize) -> String {
    format!("Drop {n} files")
}
/// The drop zone's title at rest.
pub(crate) const DROP_DOCUMENTS_HERE: &str = "Drop documents here";
/// The drop zone's button.
pub(crate) const OPEN_FILES: &str = "Open files…";
/// The drop zone's line under the supported formats.
pub(crate) const SEVERAL_AT_ONCE: &str = "Several at once each become a document in Unfiled";

/// Where documents stay, on macOS.
pub(crate) const THIS_MAC: &str = "this Mac";
/// Where documents stay, elsewhere.
pub(crate) const THIS_COMPUTER: &str = "this computer";
/// The local note's tooltip, on `machine`.
pub(crate) fn local_detail(machine: &str) -> String {
    format!(
        "Detection, conversion and Projects run on {machine}. mdoc goes online only \
         to check for updates and to download models you choose."
    )
}
/// The local note, as a screen reader reads it with its tooltip.
pub(crate) fn local_label(machine: &str, detail: &str) -> String {
    format!("Documents stay on {machine}. {detail}")
}
/// The local note.
pub(crate) fn local_note(machine: &str) -> String {
    format!("Documents stay on {machine}")
}

/// The quick actions.
pub(crate) const NEW_DOCUMENT: &str = "New document";
pub(crate) const PASTE_TEXT: &str = "Paste text";
pub(crate) const PASTE_AI_REPLY: &str = "Paste AI reply";
pub(crate) const PSEUDONYMIZE_CLIPBOARD: &str = "Pseudonymize clipboard";

/// The Continue strip's label.
pub(crate) const CONTINUE: &str = "Continue";
/// The strip's count with cards shown and none needing review.
pub(crate) const NOTHING_NEEDS_REVIEW: &str = "Nothing needs review";
/// The strip's count of documents needing review beyond those shown.
pub(crate) fn need_review(n: usize) -> String {
    match n {
        1 => "1 needs review".to_string(),
        n => format!("{n} need review"),
    }
}
/// The strip with no card to show.
pub(crate) const NOTHING_NEEDS_REVIEW_SENTENCE: &str = "Nothing needs review.";
/// A card's place for a document in no Project.
pub(crate) const UNFILED: &str = "Unfiled";
/// A document card's status: how many proposals wait.
pub(crate) fn to_review(n: usize) -> String {
    match n {
        1 => "1 to review".to_string(),
        n => format!("{n} to review"),
    }
}
/// A document card's status before its first review.
pub(crate) const NOT_REVIEWED: &str = "Not reviewed";
/// A reply card's badge.
pub(crate) const REPLY: &str = "REPLY";
/// A reply card's status.
pub(crate) const SAVED_REPLY: &str = "Saved reply";
/// A card as a screen reader reads it.
pub(crate) fn card_label(title: &str, place: &str, status: &str, age: &str) -> String {
    format!("{title}, {place}, {status}, {age}")
}
/// A card's status line.
pub(crate) fn card_status(status: &str, age: &str) -> String {
    format!("{status} · {age}")
}

/// How long ago, under an hour, a day and a week.
pub(crate) const JUST_NOW: &str = "just now";
pub(crate) fn minutes_ago(minutes: u64) -> String {
    format!("{minutes} min ago")
}
pub(crate) fn hours_ago(hours: u64) -> String {
    format!("{hours} h ago")
}
pub(crate) const YESTERDAY: &str = "yesterday";
pub(crate) fn days_ago(days: u64) -> String {
    format!("{days} days ago")
}
/// An older date, this year and earlier (chrono's format).
pub(crate) const DATE_THIS_YEAR: &str = "%-d %b";
pub(crate) const DATE_OTHER_YEAR: &str = "%-d %b %Y";

/// The notice when Paste text finds no text.
pub(crate) const NO_TEXT_TO_PASTE: &str = "The clipboard has no text to paste.";
/// The notice when a clipboard action needs a document and none is open.
pub(crate) fn needs_a_document(action: &str) -> String {
    format!("{action} works from a pseudonymized document. Open one, or open a Project, first.")
}
