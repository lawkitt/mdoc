//! Typing in edit mode (ADR 0109): the phone applies plain typing in its
//! text view and sends each committed edit here, where it joins the undo
//! line as a "Typing" step beside review decisions. A run of one-character
//! inserts, each where the last ended, or a run of deletes is one step, as
//! desktop's editor coalesces typing (`crates/mdoc-editor/src/input.rs`
//! `record_edit`); the states inside a run are not retained, so the review
//! composes the run's edits into one (`mdoc_pii::tracking`). Proposals and
//! applied aliases follow the edits through `Review::on_transaction`;
//! discovery waits for the typing pause (`DocumentSession::discover`).
//!
//! The editor core (ADR 0109's first slice) takes the coalescing rule over
//! once it is exported; until then it lives here, its one place.
use super::{Focus, ReviewText, Step, StepKind};
use crate::CoreError;
use mdoc_pii::{DiscoveryInput, DiscoveryResult};
use std::ops::Range;

/// The typing run the next edit may join, as desktop's `EditKind`.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(super) enum Run {
    /// None: the next edit starts a step.
    #[default]
    None,
    /// One-character inserts whose caret ended at this byte.
    Insert(usize),
    /// Deletes.
    Delete,
}

/// What one typed edit did, for the edit view: small, since it comes back
/// on every keystroke (the full snapshot follows the typing pause).
#[derive(Clone, Debug, PartialEq, Eq, uniffi::Record)]
pub struct Typed {
    /// The text's revision after the edit (`ReviewSnapshot::text_revision`).
    pub text_revision: u64,
    /// The text's length in UTF-16 code units: the view compares it with
    /// its own, and sends its whole text again if they differ.
    pub utf16_len: u64,
    /// The edit started a new undo step rather than joining the last one.
    pub new_step: bool,
    /// The edit overlapped an applied alias, which stops tracking it: that
    /// mention won't restore (ADR 0109's one-line hint).
    pub broke_alias: bool,
    pub can_undo: bool,
    pub can_redo: bool,
}

impl Run {
    /// The run an edit of `range` to `text` makes.
    fn of(range: &Range<usize>, text: &str) -> Self {
        if text.is_empty() {
            Self::Delete
        } else if range.is_empty() && text != "\n" && one_character(text) {
            Self::Insert(range.start + text.len())
        } else {
            Self::None
        }
    }

    /// Whether `next`, an edit at `start`, joins this run.
    fn joins(self, next: Self, start: usize) -> bool {
        match (self, next) {
            (Self::Insert(end), Self::Insert(_)) => end == start,
            (Self::Delete, Self::Delete) => true,
            _ => false,
        }
    }
}

/// One character as typed. Desktop counts grapheme clusters; a scalar is
/// close enough for a phone keyboard, where a multi-scalar emoji only
/// becomes a step of its own.
fn one_character(text: &str) -> bool {
    let mut chars = text.chars();
    chars.next().is_some() && chars.next().is_none()
}

impl ReviewText {
    /// Replace `range` with `text`, as typed in edit mode: one "Typing"
    /// step, or part of the last one when it continues its run. An edit
    /// that changes nothing makes no step. Refuses ranges outside the text
    /// or inside a character.
    pub fn type_edit(&mut self, range: Range<usize>, text: &str) -> Result<Typed, CoreError> {
        if range.start > range.end
            || range.end > self.text.len()
            || !self.text.is_char_boundary(range.start)
            || !self.text.is_char_boundary(range.end)
        {
            return Err(CoreError::invalid("The edit is not in the text."));
        }
        let run = Run::of(&range, text);
        let applied = self.review.applied().len();
        let mut new_step = false;
        if self.text.get(range.clone()) != Some(text) {
            let edits = [(range.clone(), text.to_owned())];
            if self.continues_typing(run, range.start) {
                self.sync_ids();
                self.commit_checked(&edits);
                self.edits.clear();
            } else {
                new_step = true;
                let step = Step::new(StepKind::Typing, String::new(), Focus::default());
                self.step(step, |document, _| {
                    document.commit_checked(&edits);
                    Ok(())
                })?;
            }
            self.typing = run;
        }
        Ok(Typed {
            text_revision: self.text_revision,
            utf16_len: self.text.encode_utf16().count() as u64,
            new_step,
            broke_alias: self.review.applied().len() < applied,
            can_undo: self.can_undo(),
            can_redo: self.can_redo(),
        })
    }

    /// The next typed edit starts a step of its own: the caret moved, or
    /// the view committed something other than a keystroke.
    pub fn end_typing(&mut self) {
        self.typing = Run::None;
    }

    /// The edit joins the current step: the last step is typing, its run
    /// goes on here, and nothing was undone since.
    fn continues_typing(&self, run: Run, start: usize) -> bool {
        self.typing.joins(run, start)
            && self.can_undo()
            && self.redo.is_empty()
            && self
                .step
                .as_ref()
                .is_some_and(|s| s.kind == StepKind::Typing)
    }

    /// Discovery after a typing pause (desktop's `schedule_pii_discovery`):
    /// more mentions of the names under review, in what was typed. Returns
    /// whether the review changed.
    #[cfg(test)]
    pub fn discover(&mut self) -> bool {
        let Some(input) = self.discovery_input() else {
            return false;
        };
        input
            .run(&std::sync::atomic::AtomicBool::new(false))
            .is_some_and(|result| self.apply_discovery(result))
    }

    /// What discovery reads, while edits wait for it; it runs without the
    /// session's lock, so typing goes on meanwhile.
    pub fn discovery_input(&self) -> Option<DiscoveryInput> {
        self.review
            .awaiting_discovery()
            .then(|| self.review.discovery_input())
    }

    /// Discovery's result, unless the text changed since its input: then
    /// the next pause runs it again. Returns whether the review changed.
    pub fn apply_discovery(&mut self, result: DiscoveryResult) -> bool {
        self.review.apply_discovery(result)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use mdoc_pii::{Category, Detection, Recognizer};

    fn document(text: &str, names: &[&str]) -> ReviewText {
        let mut document = ReviewText::new(text.to_owned());
        let detections = names
            .iter()
            .flat_map(|name| {
                text.match_indices(name).map(|(start, _)| Detection {
                    range: start..start + name.len(),
                    category: Category::Person,
                    score: 1.0,
                    recognizer: Recognizer::Model,
                })
            })
            .collect();
        document.ingest(detections).unwrap();
        document
    }

    /// Types `text` one character at a time from byte `at`.
    fn type_at(document: &mut ReviewText, mut at: usize, text: &str) -> Vec<Typed> {
        text.chars()
            .map(|c| {
                let typed = document.type_edit(at..at, &c.to_string()).unwrap();
                at += c.len_utf8();
                typed
            })
            .collect()
    }

    fn proposed(document: &ReviewText) -> Vec<String> {
        document
            .review
            .candidates()
            .iter()
            .map(|c| document.text[c.range.clone()].to_owned())
            .collect()
    }

    #[test]
    fn a_run_of_typing_is_one_step() {
        let mut document = document("Hello.", &[]);
        let typed = type_at(&mut document, 5, " there");
        assert_eq!(document.text, "Hello there.");
        assert!(typed[0].new_step);
        assert!(typed[1..].iter().all(|t| !t.new_step));
        assert_eq!(document.undo.len(), 1);
        assert_eq!(document.undo_step().unwrap().0.kind, StepKind::Typing);
        assert!(document.undo());
        assert_eq!(document.text, "Hello.");
        assert!(document.redo());
        assert_eq!(document.text, "Hello there.");
    }

    #[test]
    fn deletes_coalesce_and_breaks_start_new_steps() {
        let mut document = document("abcdef", &[]);
        // Backspace three times from the end.
        for end in [6, 5, 4] {
            document.type_edit(end - 1..end, "").unwrap();
        }
        assert_eq!(document.text, "abc");
        assert_eq!(document.undo.len(), 1);
        // Typing after deleting is a new step, as is a newline, a paste,
        // and an insert away from where the run ended.
        type_at(&mut document, 3, "x");
        assert!(document.type_edit(4..4, "\n").unwrap().new_step);
        assert!(document.type_edit(5..5, "pasted").unwrap().new_step);
        assert!(document.type_edit(0..0, "y").unwrap().new_step);
        // The caret moved: the run ends even where it would continue.
        document.end_typing();
        assert!(document.type_edit(1..1, "z").unwrap().new_step);
        assert_eq!(document.undo.len(), 6);
        while document.undo() {}
        assert_eq!(document.text, "abcdef");
    }

    #[test]
    fn an_edit_that_changes_nothing_is_no_step() {
        let mut document = document("abc", &[]);
        let typed = document.type_edit(1..2, "b").unwrap();
        assert!(!typed.new_step && !typed.can_undo);
        #[allow(clippy::reversed_empty_ranges)]
        let backwards = 2..1;
        assert!(document.type_edit(backwards, "").is_err());
        assert!(document.type_edit(0..9, "").is_err());
        let mut accented = ReviewText::new("é".into());
        assert!(accented.type_edit(1..1, "x").is_err());
    }

    #[test]
    fn undo_runs_across_typing_and_decisions() {
        let mut document = document("Ann Lee signed.", &["Ann Lee"]);
        let range = document.review.candidates()[0].range.clone();
        document.replace(range, super::super::Scope::This).unwrap();
        assert_eq!(document.text, "PERSON_1 signed.");
        let end = document.text.len() - 1;
        type_at(&mut document, end, " today");
        assert_eq!(document.text, "PERSON_1 signed today.");
        // Typing, then the decision, then forward again.
        assert!(document.undo());
        assert_eq!(document.redo_step().unwrap().0.kind, StepKind::Typing);
        assert_eq!(document.text, "PERSON_1 signed.");
        assert!(document.undo());
        assert_eq!(document.redo_step().unwrap().0.kind, StepKind::Replace);
        assert_eq!(document.text, "Ann Lee signed.");
        assert!(document.redo() && document.redo());
        assert_eq!(document.text, "PERSON_1 signed today.");
        assert_eq!(document.review.applied().len(), 1);
        // A decision after typing ends the run: typing on is a new step.
        let undo = document.undo.len();
        let end = document.text.len() - 1;
        assert!(type_at(&mut document, end, "!")[0].new_step);
        assert_eq!(document.undo.len(), undo + 1);
    }

    #[test]
    fn breaking_an_applied_alias_stops_tracking_it() {
        let mut document = document("Ann Lee met Ann Lee.", &["Ann Lee"]);
        document.replace_all().unwrap();
        assert_eq!(document.text, "PERSON_1 met PERSON_1.");
        assert_eq!(document.review.applied().len(), 2);
        // Typing beside an alias leaves it tracked.
        assert!(!document.type_edit(0..0, ">").unwrap().broke_alias);
        assert_eq!(document.review.applied().len(), 2);
        // Typing inside one breaks it; the other mention stays.
        let typed = document.type_edit(4..5, "").unwrap();
        assert!(typed.broke_alias);
        assert_eq!(document.text, ">PERON_1 met PERSON_1.");
        assert_eq!(document.review.applied().len(), 1);
        assert!(document.undo());
        assert_eq!(document.review.applied().len(), 2);
    }

    #[test]
    fn proposals_follow_typing_and_discovery_finds_new_mentions() {
        let mut document = document("Ann Lee signed.", &["Ann Lee"]);
        type_at(&mut document, 0, "Today ");
        assert_eq!(proposed(&document), ["Ann Lee"]);
        assert_eq!(document.review.candidates()[0].range, 6..13);
        let end = document.text.len();
        type_at(&mut document, end, " Ann Lee agreed.");
        assert!(document.review.awaiting_discovery());
        assert!(document.discover());
        assert!(!document.review.awaiting_discovery());
        assert_eq!(proposed(&document), ["Ann Lee", "Ann Lee"]);
        assert!(!document.discover());
        // An edit inside a proposal drops it; discovery may find it again.
        document.type_edit(8..9, "").unwrap();
        document.discover();
        assert_eq!(proposed(&document), ["Ann Lee"]);
    }

    #[test]
    fn utf16_length_follows_the_text() {
        let mut document = document("a", &[]);
        let typed = document.type_edit(1..1, "😀").unwrap();
        assert_eq!(typed.utf16_len, 3);
        assert_eq!(typed.text_revision, document.text_revision);
    }
}
