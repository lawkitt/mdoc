//! The window's AI words: each string a user reads of the AI project, its
//! prompt and activity, deliveries' notifications and badge, Ask and the
//! companion's states is a constant or a function here, with its counts
//! worded only here.
use std::fmt::Display;

// The AI project.

/// The indicator's label, and the Project page's AI line.
pub(crate) const SHARED_WITH_AI_APPS: &str = "Shared with AI apps";
/// The indicator's heading.
pub(crate) const SHARED_WITH_AI_APPS_HEADING: &str = "SHARED WITH AI APPS";
/// The indicator's control that opens Project `name`.
pub(crate) fn open_project(name: impl Display) -> String {
    format!("Open {name}")
}
/// The indicator's and the Project page's control that ends the share.
pub(crate) const STOP_SHARING: &str = "Stop sharing";
/// The indicator with nothing shared.
pub(crate) const NO_PROJECT_SHARED: &str = "No Project shared";
/// The rail icon's label while Project `name` is shared.
pub(crate) fn shared_with_ai_apps(name: impl Display) -> String {
    format!("Shared with AI apps: {name}")
}
/// The rail icon's label with nothing shared.
pub(crate) const NO_PROJECT_SHARED_WITH_AI_APPS: &str = "No Project shared with AI apps";
/// The rail icon's label, `label`, with `count` arrivals not opened yet.
pub(crate) fn with_new(label: impl Display, count: usize) -> String {
    format!("{label}. {count} new, open the newest")
}
/// The notice when AI app `app` asked and there is no Project to share.
pub(crate) fn asked_with_no_project(app: impl Display) -> String {
    format!("{app} asked for documents. Make a Project to share it with AI apps.")
}
/// The notice when sharing failed with `error`.
pub(crate) fn could_not_share(error: impl Display) -> String {
    format!("Could not share the Project with AI apps. {error}")
}

// The share prompt.

/// The prompt's title: AI app `app` asked.
pub(crate) fn asked_for_documents(app: impl Display) -> String {
    format!("{app} asked for documents")
}
/// The prompt's question.
pub(crate) const SHARE_A_PROJECT: &str =
    "Share a Project with AI apps? They read only its Ready documents, pseudonymized.";
/// The prompt's list of Projects.
pub(crate) const PROJECT_TO_SHARE: &str = "Project to share";
/// The prompt's button.
pub(crate) const SHARE: &str = "Share";

// The Project page's AI activity.

/// The activity when it can't be read, with `error`.
pub(crate) fn activity_unreadable(error: impl Display) -> String {
    format!("The AI activity can't be read. {error}")
}
/// The activity with nothing in it.
pub(crate) const NO_ACTIVITY: &str = "No AI app has read this Project yet.";
/// Under the activity when only the newest `shown` of `total` show.
pub(crate) fn showing_newest(shown: usize, total: usize) -> String {
    format!("Showing the newest {shown} of {total}.")
}
/// A title in the activity, quoted.
pub(crate) fn quoted(title: impl Display) -> String {
    format!("\u{201c}{title}\u{201d}")
}
/// Between an activity line's details.
pub(crate) const SEPARATOR: &str = " · ";
/// A document the activity names that is gone.
pub(crate) const A_DELETED_DOCUMENT: &str = "a deleted document";
/// After a read document's title: which part was read.
pub(crate) fn part(part: impl Display) -> String {
    format!(" · part {part}")
}
/// `app` read `document` (`part`, from [`part`], or empty).
pub(crate) fn read(app: impl Display, document: impl Display, part: impl Display) -> String {
    format!("{app} read {document}{part}")
}
/// `app` delivered a document, `title` quoted.
pub(crate) fn delivered_document(app: impl Display, title: impl Display) -> String {
    format!("{app} delivered {title} as a document")
}
/// `app` delivered a document that is gone.
pub(crate) fn delivered_deleted_document(app: impl Display) -> String {
    format!("{app} delivered a deleted document")
}
/// `app` delivered a reply, `title` quoted.
pub(crate) fn delivered_reply(app: impl Display, title: impl Display) -> String {
    format!("{app} delivered a reply {title}")
}
/// `app` delivered a reply that is gone.
pub(crate) fn delivered_deleted_reply(app: impl Display) -> String {
    format!("{app} delivered a deleted reply")
}
/// The documents a delivery was made from: `first`, and `more` others.
pub(crate) fn from_documents(first: impl Display, more: usize) -> String {
    match more {
        0 => format!("from {first}"),
        more => format!("from {first} and {more} more"),
    }
}

// Deliveries.

/// A sender that is not a known product.
pub(crate) const AN_AI_APP: &str = "An AI app";
/// A delivered reply's notification title.
pub(crate) fn sent_a_reply(app: impl Display) -> String {
    format!("{app} sent a reply")
}
/// A delivered reply's notification text.
pub(crate) const OPEN_TO_READ: &str = "Open it in mdoc to read it with the names restored.";
/// A delivered document's notification title.
pub(crate) fn sent_a_document(app: impl Display) -> String {
    format!("{app} sent a document")
}
/// A delivered document's notification text.
pub(crate) const OPEN_TO_REVIEW: &str = "Open it in mdoc to review it.";
/// A review request's notification title, for the document `label`.
pub(crate) fn asked_for(app: impl Display, label: impl Display) -> String {
    format!("{app} asked for \u{201c}{label}\u{201d}")
}
/// A review request's notification text.
pub(crate) const NEEDS_REVIEW_FIRST: &str =
    "It needs review before AI apps can read it. Open it in mdoc to review it.";
/// The notice when a notification's reply is gone.
pub(crate) const REPLY_DELETED: &str = "That AI reply was deleted.";
/// The notice when a notification's document is gone.
pub(crate) const DOCUMENT_DELETED: &str = "That document was deleted.";
/// The badge's label: `count` arrivals not opened yet.
pub(crate) fn new_from_ai_apps(count: usize) -> String {
    format!("{count} new from AI apps, open the newest")
}
/// The badge's text.
pub(crate) fn new_count(count: usize) -> String {
    format!("{count} new")
}

// Ask.

/// The log line when Ask's choice could not be saved.
pub(crate) fn ask_choice_not_saved(error: impl Display) -> String {
    format!("mdoc: could not save Ask's choice: {error}")
}
/// The notice when Ask failed with `error`.
pub(crate) fn could_not_ask(error: impl Display) -> String {
    format!("Could not ask about the document. {error}")
}
/// The Project page's control beside Ask.
pub(crate) const MORE_WAYS_TO_ASK: &str = "More ways to ask";

// The companion.

/// A listed document AI apps can read.
pub(crate) const READY: &str = "Ready";
/// A listed document that needs review, and why (`reason`). The companion
/// reads this prefix back.
pub(crate) fn needs_review(reason: impl Display) -> String {
    format!("Needs review · {reason}")
}
