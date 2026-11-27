//! Notifications and where a reply lands (ADR 0126; #1164 Decisions 4 and
//! 6; Milestone 2 item 9), all of it only while "AI connector (preview)"
//! is on:
//!
//! - a delivery, and a request for a document that needs review, each
//!   raise one OS notification; clicking it brings the window forward and
//!   opens the Reply page, or the document for review (a delivered
//!   Document opens for review too);
//! - the AI card's badge counts what arrived and is not opened yet, and
//!   opens the newest;
//! - mdoc never takes focus by itself unless "Open deliveries when they
//!   arrive" is on: then a delivery opens where it lands, in front;
//! - the companion window (item 14) opens by itself for each, without
//!   taking focus, and shows the latest reply and review request.
//!
//! A notification names only the AI app and the AI label of the document
//! asked for: never a title, a restored name or document text. Its tag
//! holds ids only. GPUI posts it (`App::show_system_notification`); tests
//! and headless renders inject a [`Notifier`] that records instead.
//!
//! Its actions and its view (the badge) each have their file.
mod actions;
mod view;

use super::text;
use crate::workspace::ai::project::Arrived;
use gpui::App;
use std::rc::Rc;

/// The app's identity for notifications: the packager's identifier
/// (`[package.metadata.packager]`), the Windows AppUserModelID.
pub(in crate::workspace) const APP_ID: &str = "io.github.lawkitt.mdoc";

/// The longest AI label a notification quotes, in characters.
const LABEL_CHARS: usize = 80;

/// One notification, as mdoc words it.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(in crate::workspace) struct Note {
    /// Ids only: [`Arrived::tag`].
    pub(in crate::workspace) tag: String,
    pub(in crate::workspace) title: String,
    pub(in crate::workspace) body: String,
}

impl Note {
    /// `item` from AI app `app`, as it sent itself; `label`, a review
    /// request's AI label.
    pub(in crate::workspace) fn of(item: &Arrived, app: &str, label: &str) -> Self {
        // A product's name, never the client's own free text.
        let app = crate::ai::connector::product_name(app).unwrap_or(text::AN_AI_APP);
        let (title, body) = match item {
            Arrived::Reply(_) => (text::sent_a_reply(app), text::OPEN_TO_READ.to_string()),
            Arrived::Document(_) => (text::sent_a_document(app), text::OPEN_TO_REVIEW.to_string()),
            Arrived::Review(_) => {
                let label: String = (label.chars())
                    .map(|c| if c.is_control() { ' ' } else { c })
                    .take(LABEL_CHARS)
                    .collect();
                (
                    text::asked_for(app, label.trim()),
                    text::NEEDS_REVIEW_FIRST.to_string(),
                )
            }
        };
        Self {
            tag: item.tag(),
            title,
            body,
        }
    }
}

/// What posts a note.
type Post = Rc<dyn Fn(&Note, &mut App)>;

/// Where the window's notifications go: the OS, by default.
#[derive(Clone)]
pub(in crate::workspace) struct Notifier {
    post: Post,
    /// The OS's: the app takes its identity when the connector starts.
    system: bool,
}

impl Default for Notifier {
    fn default() -> Self {
        Self {
            post: Rc::new(|note, cx| {
                cx.show_system_notification(gpui::SystemNotification {
                    tag: note.tag.clone().into(),
                    title: note.title.clone().into(),
                    body: note.body.clone().into(),
                    actions: Vec::new(),
                })
            }),
            system: !mdoc_store::app_paths::isolated(),
        }
    }
}

impl Notifier {
    /// One that keeps each note in the list it returns, and posts none.
    #[cfg(any(
        test,
        all(
            debug_assertions,
            feature = "headless-renders",
            any(target_os = "macos", target_os = "windows")
        )
    ))]
    pub(in crate::workspace) fn recording() -> (Self, Rc<std::cell::RefCell<Vec<Note>>>) {
        let notes = Rc::new(std::cell::RefCell::new(Vec::new()));
        let kept = notes.clone();
        let notifier = Self {
            post: Rc::new(move |note, _| kept.borrow_mut().push(note.clone())),
            system: false,
        };
        (notifier, notes)
    }

    /// Renders: the OS's notifier posts nothing there.
    fn posts(&self) -> bool {
        !self.system || !mdoc_store::app_paths::isolated()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Notification text names the app and the AI label only.
    #[test]
    fn notes_name_the_app_and_the_label_only() {
        let reply = Note::of(&Arrived::Reply("r1".into()), "claude-ai", "");
        assert_eq!(
            reply,
            Note {
                tag: "ai-reply-r1".into(),
                title: "Claude sent a reply".into(),
                body: "Open it in mdoc to read it with the names restored.".into(),
            }
        );
        let document = Note::of(&Arrived::Document("d1".into()), "openai-chatgpt", "");
        assert_eq!(document.title, "ChatGPT sent a document");
        assert_eq!(document.body, "Open it in mdoc to review it.");
        assert_eq!(document.tag, "ai-document-d1");
        // A name the client made up is not shown.
        let other = Note::of(&Arrived::Reply("r".into()), "Maria Lopez's lease", "");
        assert_eq!(other.title, "An AI app sent a reply");
        let review = Note::of(&Arrived::Review("d2".into()), "", "Document\n3");
        assert_eq!(
            review.title,
            "An AI app asked for \u{201c}Document 3\u{201d}"
        );
        assert_eq!(
            review.body,
            "It needs review before AI apps can read it. Open it in mdoc to review it."
        );
        let long = Note::of(&Arrived::Review("d".into()), "claude-ai", &"x".repeat(200));
        assert!(long.title.chars().count() < 120, "{}", long.title);
    }
}
