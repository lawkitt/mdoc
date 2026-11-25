//! Ask (#1164 Decisions 2, 3 and 22; Milestone 2 item 10): the document
//! header's and the Project page's Ask Claude, Ask ChatGPT and Copy for
//! AI, the prompt each opens the AI app with, and what waits for a move.
//! GPUI-free; the window carries it out (`workspace/ai_ask_ui.rs`).
//!
//! - Ask shares the document's Project, in place of any other, as part of
//!   the click: the share never follows the view.
//! - The prompt names a document only by its AI label, from the
//!   connector's own list, and its doc_id: never a title or its text. A
//!   label can change before the app reads it (a document reviewed since
//!   the click is named by its heading), the doc_id doesn't.
//! - Unfiled never leaves: Ask on an Unfiled document offers "Move to a
//!   new Project and share", which asks once the move has landed.
//! - Copy for AI is today's Copy for AI; it shares nothing.
use crate::ai_links;
use mdoc_store::settings::AskWith;

/// Every choice, in the menu's order.
pub(crate) const CHOICES: [AskWith; 3] = [AskWith::Claude, AskWith::ChatGpt, AskWith::CopyForAi];

/// What `with`'s menu row and the button's main part read.
pub(crate) fn label(with: AskWith) -> &'static str {
    match with {
        AskWith::Claude => "Ask Claude",
        AskWith::ChatGpt => "Ask ChatGPT",
        AskWith::CopyForAi => "Copy for AI",
    }
}

/// The AI app `with` opens, as the window names it.
pub(crate) fn app(with: AskWith) -> Option<&'static str> {
    match with {
        AskWith::Claude => Some("Claude"),
        AskWith::ChatGpt => Some("ChatGPT"),
        AskWith::CopyForAi => None,
    }
}

/// What the Project page's button repeats: Copy for AI copies one
/// document, so the page offers only the apps, and Claude in its place.
pub(crate) fn on_project_page(last: AskWith) -> AskWith {
    match last {
        AskWith::CopyForAi => AskWith::Claude,
        with => with,
    }
}

/// The prompt for a document the connector labels `label`, id `doc_id`.
pub(crate) fn document_prompt(label: &str, doc_id: &str) -> String {
    format!("Read \u{201c}{label}\u{201d} in mdoc (doc_id {doc_id}). My question: ")
}

/// The prompt for the shared Project as a whole.
pub(crate) const PROJECT_PROMPT: &str =
    "List my documents in mdoc and read the ones you need. My question: ";

/// The link that opens `with`'s app with `prompt` filled in, not sent;
/// `None` for Copy for AI.
pub(crate) fn link(with: AskWith, prompt: &str) -> Option<String> {
    match with {
        AskWith::Claude => Some(ai_links::claude(prompt)),
        AskWith::ChatGpt => Some(ai_links::chatgpt(prompt)),
        AskWith::CopyForAi => None,
    }
}

/// The notice after `with`'s app opened, with Project `project` shared.
pub(crate) fn opened(with: AskWith, project: &str) -> String {
    let shared = format!("\u{201c}{project}\u{201d} is shared with AI apps.");
    match with {
        AskWith::ChatGpt => format!(
            "{shared} ChatGPT opened a new thread with the prompt: add your question and send \
             it. Only Work threads can read mdoc."
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

/// Why Ask did nothing for a document excluded from AI.
pub(crate) const EXCLUDED: &str = "This document is excluded from AI apps.";

/// An Ask waiting for its document's move to a new Project.
#[derive(Default)]
pub(crate) struct Waiting {
    after_move: Option<(String, AskWith)>,
}

impl Waiting {
    /// Document `document` moves to a new Project for `with`: once it has,
    /// that Project is shared and `with`'s app opens. Replaces any other.
    pub(crate) fn after_move(&mut self, document: &str, with: AskWith) {
        self.after_move = Some((document.to_string(), with));
    }

    /// Document `document`'s move ended: what waited for it, once.
    pub(crate) fn moved(&mut self, document: &str) -> Option<AskWith> {
        let (waiting, _) = self.after_move.as_ref()?;
        if waiting != document {
            return None;
        }
        self.after_move.take().map(|(_, with)| with)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_prompt_names_a_document_by_its_label_and_id_alone() {
        assert_eq!(
            document_prompt("Lease of PERSON_1's flat", "0a1b"),
            "Read “Lease of PERSON_1's flat” in mdoc (doc_id 0a1b). My question: "
        );
        assert_eq!(
            PROJECT_PROMPT,
            "List my documents in mdoc and read the ones you need. My question: "
        );
        assert_eq!(
            link(AskWith::Claude, "Hi there").as_deref(),
            Some("claude://claude.ai/new?q=Hi%20there")
        );
        assert_eq!(
            link(AskWith::ChatGpt, "Hi there").as_deref(),
            Some("codex://new?prompt=Hi%20there")
        );
        assert_eq!(link(AskWith::CopyForAi, "Hi"), None);
    }

    #[test]
    fn the_project_page_repeats_an_app_never_copy() {
        assert_eq!(on_project_page(AskWith::CopyForAi), AskWith::Claude);
        assert_eq!(on_project_page(AskWith::ChatGpt), AskWith::ChatGpt);
        assert_eq!(
            CHOICES.map(label),
            ["Ask Claude", "Ask ChatGPT", "Copy for AI"]
        );
    }

    #[test]
    fn notices_name_the_app_and_the_shared_project() {
        assert_eq!(
            opened(AskWith::Claude, "Lease"),
            "“Lease” is shared with AI apps. Claude opened a new chat with the prompt: add your \
             question and send it."
        );
        assert!(opened(AskWith::ChatGpt, "Lease").ends_with("Only Work threads can read mdoc."));
        assert_eq!(
            unfiled_title(AskWith::ChatGpt),
            "Ask ChatGPT about an Unfiled document?"
        );
    }

    #[test]
    fn an_ask_waits_for_its_own_documents_move_once() {
        let mut waiting = Waiting::default();
        assert_eq!(waiting.moved("a"), None);
        waiting.after_move("a", AskWith::ChatGpt);
        assert_eq!(waiting.moved("b"), None);
        assert_eq!(waiting.moved("a"), Some(AskWith::ChatGpt));
        assert_eq!(waiting.moved("a"), None);
        // A later Ask replaces one still waiting.
        waiting.after_move("a", AskWith::Claude);
        waiting.after_move("b", AskWith::ChatGpt);
        assert_eq!(waiting.moved("a"), None);
        assert_eq!(waiting.moved("b"), Some(AskWith::ChatGpt));
    }
}
