//! Edit mode (ADR 0109): typed edits from the phone's text view, as
//! "Typing" steps in the one undo line (`review/typing.rs`), and discovery
//! once the typing pauses. Each edit returns a small `Typed`, never the
//! whole snapshot: the view already shows the text. The save that follows
//! is `save_pending`'s, as after a review tap.
pub use super::review::Typed;
use super::{DocumentSession, ReviewSnapshot, no_document, span};
use crate::{CoreError, editor::EditorEdit, library::TextRange};
use std::sync::atomic::AtomicBool;

#[uniffi::export]
impl DocumentSession {
    /// Replace `range` (UTF-8 offsets into the text) with `text`, as typed
    /// in edit mode: a new "Typing" step, or part of the last one while the
    /// run of typing goes on. The platform sends committed text only, never
    /// marked (IME) text.
    pub fn type_text(&self, range: TextRange, text: String) -> Result<Typed, CoreError> {
        let mut guard = self.lock_document();
        let document = guard.as_mut().ok_or_else(no_document)?;
        document.review.type_edit(span(range), &text)
    }

    /// Apply one edit-mode command's edits (a toolbar or formatting-keyboard
    /// button, ⌘B, the link sheet: `editor_*`'s answer as the view applied
    /// it) as a "Formatting" undo step of its own. The edits are sorted and
    /// in UTF-8 offsets of the text before any of them.
    pub fn format_text(&self, edits: Vec<EditorEdit>) -> Result<Typed, CoreError> {
        let edits: Vec<_> = edits
            .into_iter()
            .map(|edit| (span(edit.range), edit.text))
            .collect();
        let mut guard = self.lock_document();
        let document = guard.as_mut().ok_or_else(no_document)?;
        document.review.format_edit(&edits)
    }

    /// The next typed edit starts an undo step of its own: the caret moved
    /// away, or edit mode ended.
    pub fn end_typing(&self) {
        if let Some(document) = self.lock_document().as_mut() {
            document.review.end_typing();
        }
    }

    /// After a typing pause: discovery finds more mentions of the names
    /// under review in what was typed, as desktop's background discovery
    /// does, without holding the document while it runs. The snapshot
    /// after, which a typed edit meanwhile leaves for the next pause.
    pub fn discover(&self) -> Result<ReviewSnapshot, CoreError> {
        let input = {
            let guard = self.lock_document();
            guard
                .as_ref()
                .ok_or_else(no_document)?
                .review
                .discovery_input()
        };
        let result = input.and_then(|input| input.run(&AtomicBool::new(false)));
        let mut guard = self.lock_document();
        let document = guard.as_mut().ok_or_else(no_document)?;
        if let Some(result) = result {
            document.review.apply_discovery(result);
        }
        Ok(document.snapshot())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{document_session::ReviewAction, library::Library, test_support::Scratch};

    fn at(text: &str, needle: &str) -> TextRange {
        let start = text.find(needle).unwrap() as u64;
        TextRange {
            start,
            end: start + needle.len() as u64,
        }
    }

    fn caret(offset: usize) -> TextRange {
        TextRange {
            start: offset as u64,
            end: offset as u64,
        }
    }

    /// Typing saves through `save_pending` and reopens as typed; undo and
    /// redo in the snapshot name the typing step.
    #[test]
    fn typing_saves_and_undoes_through_the_session() {
        let scratch = Scratch::new();
        let library = Library::new(scratch.path().display().to_string(), vec![3; 32]).unwrap();
        let session = DocumentSession::new();
        session
            .import_text("Ann Lee signed.".into(), "Typed".into())
            .unwrap();
        session.save(library.clone()).unwrap();
        let text = session.snapshot().unwrap().text;
        let end = text.len() - 1;
        for (i, c) in " today".chars().enumerate() {
            let typed = session.type_text(caret(end + i), c.to_string()).unwrap();
            assert_eq!(typed.new_step, i == 0);
        }
        let snapshot = session.discover().unwrap();
        assert_eq!(snapshot.text, "Ann Lee signed today.");
        assert_eq!(
            snapshot.undo_step.unwrap().kind,
            super::super::StepKind::Typing
        );
        let saved = session.save_pending(library.clone()).unwrap().unwrap();
        let reopened = DocumentSession::new();
        let opened = reopened.open_document(library.clone(), saved.path).unwrap();
        assert_eq!(opened.text, "Ann Lee signed today.");
        let undone = session.review(ReviewAction::Undo, None).unwrap();
        assert_eq!(undone.text, "Ann Lee signed.");
        assert_eq!(undone.redo_step.unwrap().label, "Typing");
    }

    /// A mention typed after the names were proposed is proposed once
    /// discovery runs.
    #[test]
    fn discovery_proposes_typed_mentions() {
        let session = DocumentSession::new();
        let text = "Ann Lee signed.";
        session.import_text(text.into(), "Typed".into()).unwrap();
        session
            .add_selection(at(text, "Ann Lee"), Some(super::super::Category::Person))
            .unwrap();
        session
            .type_text(caret(text.len()), " Ann Lee too.".into())
            .unwrap();
        assert_eq!(session.snapshot().unwrap().proposed, 1);
        assert_eq!(session.discover().unwrap().proposed, 2);
    }

    /// A command is one Formatting step, named by Undo.
    #[test]
    fn a_command_formats_as_one_step() {
        let session = DocumentSession::new();
        session
            .import_text("due now".into(), "Typed".into())
            .unwrap();
        let change = crate::editor::editor_toggle_format(
            "due now".into(),
            caret(1),
            crate::editor::EditorInlineFormat::Bold,
        )
        .unwrap();
        let typed = session.format_text(change.edits).unwrap();
        assert!(typed.new_step);
        let snapshot = session.snapshot().unwrap();
        assert_eq!(snapshot.text, "**due** now");
        assert_eq!(snapshot.undo_step.unwrap().label, "Formatting");
        assert_eq!(
            session.review(ReviewAction::Undo, None).unwrap().text,
            "due now"
        );
    }

    #[test]
    fn edits_outside_the_text_are_refused() {
        let session = DocumentSession::new();
        assert!(session.type_text(caret(0), "x".into()).is_err());
        session.import_text("abc".into(), "Typed".into()).unwrap();
        assert!(session.type_text(caret(9), "x".into()).is_err());
        session.end_typing();
    }
}
