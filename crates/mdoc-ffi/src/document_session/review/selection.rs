//! "Replace N and copy" (#1209): a selection's proposals replaced as
//! Replace all replaces them (`mdoc_pii::selection`), as one undo step.
use super::{Focus, ReviewText, Step, StepKind};
use crate::CoreError;
use std::ops::Range;

impl ReviewText {
    /// Replace the proposals in `range` or across its edges, with the
    /// applied aliases there whose identity was renamed since, as one
    /// "Replace in selection" step. Returns the selection in the text
    /// after, widened over a proposal across its edge, and how many
    /// proposals it replaced. With none to replace, no step is made.
    pub fn replace_selection(
        &mut self,
        range: Range<usize>,
    ) -> Result<(Range<usize>, usize), CoreError> {
        self.in_text(&range)?;
        let planned = (self.review)
            .selection_plans(&self.text, range.clone())
            .map_err(CoreError::review)?;
        if planned.plans.is_empty() {
            return Ok((range, 0));
        }
        let subject = planned.proposals.to_string();
        let before = Focus {
            range: Some(range),
            identity: None,
        };
        let mut step = Step::new(StepKind::ReplaceSelection, subject, before);
        step.after = Focus {
            range: Some(planned.after.clone()),
            identity: None,
        };
        self.step(step, |document, _| {
            document.commit_plans(&planned.plans, None)?;
            document.refresh();
            Ok(())
        })?;
        Ok((planned.after, planned.proposals))
    }

    /// What `range` holds (`mdoc_pii::Review::check_selection`).
    pub fn check_selection(
        &self,
        range: Range<usize>,
        scanned: bool,
        known: &mdoc_pii::restore::RestoreMap,
    ) -> Result<mdoc_pii::selection::SelectionCheck, CoreError> {
        self.in_text(&range)?;
        (self.review)
            .check_selection(&self.text, range, scanned, known)
            .map_err(CoreError::review)
    }

    /// Refuses a selection outside the text or inside a character.
    pub(crate) fn in_text(&self, range: &Range<usize>) -> Result<(), CoreError> {
        match self.text.get(range.clone()) {
            Some(_) => Ok(()),
            None => Err(CoreError::invalid("The selection is not in the text.")),
        }
    }
}
