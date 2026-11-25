//! Edit mode: typed edits from the phone's text view, as
//! "Typing" steps in the one undo line (`review/typing.rs`), discovery
//! once the typing pauses, and at Done a scan of the paragraphs the edits
//! changed (`mdoc_pii` `edited`). Each edit returns a small `Typed`, never the
//! whole snapshot: the view already shows the text. The save that follows
//! is `save_pending`'s, as after a review tap.
pub use super::review::Typed;
use super::{DocumentSession, JobProgress, JobStage, ReviewSnapshot, no_document, span};
use crate::{CoreError, editor::EditorEdit, setup::DetectorPaths, text::TextRange};
use std::sync::atomic::{AtomicBool, Ordering};

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

    /// At Done, after `end_typing`, and before Copy for AI or Share if the
    /// snapshot's `edits_unscanned` says so: discovery, then the paragraphs
    /// edits changed since they were scanned, scanned as a fresh document
    /// is: by the name finder with `detector` (its memory check and pause
    /// for heat as `scan`'s), or by the rules alone without one, below the
    /// floor. A new name becomes a proposal, so the document is Ready again
    /// only once it is decided; an edit with no new name is Ready again with
    /// no click. Nothing owed returns the snapshot as it is. The text
    /// changing meanwhile leaves the paragraphs owed, for the next call.
    pub fn scan_edits(
        &self,
        detector: Option<DetectorPaths>,
        progress: Option<Box<dyn JobProgress>>,
    ) -> Result<ReviewSnapshot, CoreError> {
        let snapshot = self.discover()?;
        let (edited, generation) = {
            let guard = self.lock_document();
            let document = guard.as_ref().ok_or_else(no_document)?;
            match document.review.edited_text() {
                Some(edited) => (edited, document.generation),
                None => return Ok(snapshot),
            }
        };
        let cancel = self.start_job();
        let progress = progress.as_deref();
        if let Some(progress) = progress {
            progress.stage(JobStage::Scanning);
        }
        let mut detections = match &detector {
            Some(paths) => {
                super::scanning::check_free_memory(progress)?;
                mdoc_detect::scan_at(
                    edited.text(),
                    &cancel,
                    std::path::Path::new(&paths.model),
                    std::path::Path::new(&paths.onnx),
                    &mdoc_store::settings::PiiConfig::default(),
                    mdoc_detect::ScanLimit::PHONE,
                    &super::scanning::PhoneScan::new(progress, paths.speed),
                )?
                .0
            }
            None => mdoc_detect::scan_rules(edited.text())?,
        };
        if cancel.load(Ordering::Relaxed) {
            return Err(CoreError::Cancelled);
        }
        self.with_document(|document| {
            if document.generation == generation {
                detections.retain(|d| !document.skip.contains(&d.category));
                document.review.ingest_edited(&edited, detections)?;
            }
            Ok(())
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        document_session::{MentionState, ReviewAction},
        test_support::Scratch,
        vault::{Readiness, Vault},
    };
    use std::sync::Arc;

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

    /// Typing saves through `save_pending_into_vault` and reopens as typed;
    /// undo and redo in the snapshot name the typing step.
    #[test]
    fn typing_saves_and_undoes_through_the_session() {
        let scratch = Scratch::new();
        let vault = Vault::open(scratch.path().display().to_string(), vec![3; 32]).unwrap();
        let session = DocumentSession::new();
        session
            .import_text("Ann Lee signed.".into(), "Typed".into())
            .unwrap();
        session.save_into_vault(vault.clone()).unwrap();
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
        let saved = session
            .save_pending_into_vault(vault.clone())
            .unwrap()
            .unwrap();
        let reopened = DocumentSession::new();
        let opened = reopened.open_from_vault(vault.clone(), saved.id).unwrap();
        assert_eq!(opened.text, "Ann Lee signed today.");
        let undone = session.review(ReviewAction::Undo).unwrap();
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
        assert_eq!(session.review(ReviewAction::Undo).unwrap().text, "due now");
    }

    /// A document Ready after its scan, its one name replaced, and its Vault.
    fn ready_letter(scratch: &Scratch) -> (Arc<DocumentSession>, Arc<Vault>, String) {
        let vault = Vault::open(scratch.path().display().to_string(), vec![3; 32]).unwrap();
        let session = DocumentSession::new();
        let text = "Ann Lee signed the lease.\n\nThe term ends in May.";
        session.import_text(text.into(), "Lease".into()).unwrap();
        session
            .add_candidates(vec![super::super::Candidate {
                range: at(text, "Ann Lee"),
                category: super::super::Category::Person,
                score: 0.9,
            }])
            .unwrap();
        session.replace_all().unwrap();
        let id = session.save_into_vault(vault.clone()).unwrap().id;
        assert_eq!(vault.readiness(id.clone()).unwrap(), Readiness::Ready);
        (session, vault, id)
    }

    /// Types `typed` at the end of the text, a character at a time, and
    /// saves as the apps do after each keystroke.
    fn type_at_end(session: &DocumentSession, vault: &Arc<Vault>, typed: &str) {
        for c in typed.chars() {
            let end = session.snapshot().unwrap().text.len();
            session.type_text(caret(end), c.to_string()).unwrap();
        }
        session.save_pending_into_vault(vault.clone()).unwrap();
    }

    /// Below the floor, a new name typed into a Ready document is found at
    /// Done by the rules, as a fresh document's would be: a proposal, and
    /// the document not Ready until it is decided. Until Done it is not
    /// Ready either, and nothing leaves with the name as typed.
    #[test]
    fn a_typed_new_name_is_proposed_at_done() {
        let scratch = Scratch::new();
        let (session, vault, id) = ready_letter(&scratch);
        session.set_no_name_finder(true);
        type_at_end(&session, &vault, " Write to bob@example.com.");
        let typed = session.discover().unwrap();
        assert!(typed.edits_unscanned && typed.proposed == 0);
        assert_eq!(vault.readiness(id.clone()).unwrap(), Readiness::NotReviewed);
        assert!(vault.text_for_ai(id.clone()).is_err());

        session.end_typing();
        let done = session.scan_edits(None, None).unwrap();
        assert!(!done.edits_unscanned);
        let proposed: Vec<_> = (done.mentions.iter())
            .filter(|m| m.state == MentionState::Proposed)
            .map(|m| m.original.as_str())
            .collect();
        assert_eq!(proposed, ["bob@example.com"]);
        session.save_pending_into_vault(vault.clone()).unwrap();
        assert_eq!(vault.readiness(id.clone()).unwrap(), Readiness::NotReviewed);

        session.replace_all().unwrap();
        session.save_pending_into_vault(vault.clone()).unwrap();
        let sent = vault.text_for_ai(id).unwrap();
        assert!(sent.contains("EMAIL_1") && !sent.contains("bob@"), "{sent}");
        assert!(!sent.contains("Ann Lee"));
    }

    /// An edit that adds no name is Ready again at Done, with no click.
    #[test]
    fn an_edit_with_no_new_name_is_ready_again_at_done() {
        let scratch = Scratch::new();
        let (session, vault, id) = ready_letter(&scratch);
        type_at_end(&session, &vault, " It renews.");
        assert_ne!(vault.readiness(id.clone()).unwrap(), Readiness::Ready);
        session.end_typing();
        let done = session.scan_edits(None, None).unwrap();
        assert!(!done.edits_unscanned && done.proposed == 0);
        assert!(
            session
                .save_pending_into_vault(vault.clone())
                .unwrap()
                .is_some()
        );
        assert_eq!(vault.readiness(id.clone()).unwrap(), Readiness::Ready);
        assert!(
            vault
                .text_for_ai(id)
                .unwrap()
                .ends_with("in May. It renews.")
        );
        // Nothing owed: the snapshot as it is.
        assert_eq!(session.scan_edits(None, None).unwrap(), done);
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
