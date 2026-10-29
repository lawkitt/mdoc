//! What you did (ADR 0079): each pseudonymization decision is listed on the
//! undo point it created, and travelling to an entry presses ⌘Z or ⇧⌘Z that
//! many times. The log is memory only and never reaches the session or case.
use super::{ReviewUi, mapping::MappingAction};
use crate::document_view::DocumentView;
use gpui::{App, Context, Window};
use mdoc_pii::decisions::{Decision, DecisionLog};
use std::{collections::HashSet, ops::Range, sync::Arc};

/// The entry's time, as the clock on this computer shows it.
pub(super) fn local_time() -> String {
    chrono::Local::now().format("%H:%M").to_string()
}

impl ReviewUi {
    pub(crate) fn decisions(&self) -> &DecisionLog {
        &self.decisions
    }
    /// Renders and tests write a fixed time.
    #[cfg(any(
        test,
        all(
            debug_assertions,
            feature = "headless-renders",
            any(target_os = "macos", target_os = "windows")
        )
    ))]
    pub(crate) fn set_decision_clock(&mut self, clock: fn() -> String) {
        self.clock = clock;
    }
}

impl DocumentView {
    /// List `decision` as the step from history state `before` to now.
    /// Nothing is listed when it made no step.
    pub(in crate::pii::ui) fn record_decision(
        &mut self,
        before: u64,
        decision: Decision,
        cx: &App,
    ) {
        let path = self.editor.read(cx).history_path();
        let time = (self.pii.clock)();
        self.pii.decisions.record(before, decision, time, &path);
    }

    /// Travel to row `index` of What you did: Undo back to just after it, or
    /// Redo up to it. Each press is its own ⌘Z / ⇧⌘Z, handled in full before
    /// the next, so travel equals pressing the keys that many times.
    pub(crate) fn travel_to_decision(
        &mut self,
        index: usize,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let path = self.editor.read(cx).history_path();
        if let Some(steps) = self.pii.decisions.steps_to(index, &path) {
            self.press_history(steps < 0, steps.unsigned_abs(), window, cx);
        }
    }
    fn press_history(
        &mut self,
        undo: bool,
        presses: usize,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if presses == 0 {
            return;
        }
        self.editor.update(cx, |editor, cx| {
            if undo {
                editor.undo_step(window, cx)
            } else {
                editor.redo_step(window, cx)
            }
        });
        // Deferred behind the press's own events, as the next key would be.
        cx.defer_in(window, move |this, window, cx| {
            this.press_history(undo, presses - 1, window, cx)
        });
    }

    /// The original wording of the mention at `range`: a proposal's variant
    /// or an applied mention's original.
    fn mention_original(&self, range: &Range<usize>) -> Option<Arc<str>> {
        let review = &self.pii.review;
        review
            .candidates()
            .iter()
            .find(|c| c.range == *range)
            .and_then(|c| review.variant(c.variant))
            .map(|v| v.original.clone())
            .or_else(|| {
                review
                    .applied()
                    .iter()
                    .find(|a| a.range == *range)
                    .map(|a| a.step.original_shared().clone())
            })
    }

    /// The originals of these applied mentions, one per mention.
    pub(in crate::pii::ui) fn applied_originals(&self, ids: &HashSet<u64>) -> Vec<Arc<str>> {
        self.pii
            .review
            .applied()
            .iter()
            .filter(|a| ids.contains(&a.id))
            .map(|a| a.step.original_shared().clone())
            .collect()
    }

    /// The decision a mapping change is, named before it happens. Setting an
    /// owner is no listed decision.
    pub(in crate::pii::ui) fn mapping_decision(&self, action: &MappingAction) -> Option<Decision> {
        let review = &self.pii.review;
        let entity = |id: u64| {
            review
                .identity_wording(id)
                .or_else(|| review.identity(id).map(|i| i.alias.as_str().into()))
        };
        let variant = |group: u64| review.variant(group).map(|v| v.original.clone());
        let applied = |id: u64| {
            review
                .applied_occurrence(id)
                .map(|a| a.step.original_shared().clone())
        };
        Some(match action {
            MappingAction::Rename(id, alias) => Decision::Rename {
                original: entity(*id)?,
                alias: alias.clone(),
            },
            MappingAction::Category(id, category) => Decision::Category {
                original: entity(*id)?,
                category: *category,
            },
            MappingAction::Owner(..) => return None,
            MappingAction::Merge(from, into) => Decision::Merge {
                from: entity(*from)?,
                into: entity(*into)?,
            },
            MappingAction::AssignVariant(group, into) => Decision::Merge {
                from: variant(*group)?,
                into: entity(*into)?,
            },
            MappingAction::AssignCandidate(group, _, Some(into)) => Decision::Merge {
                from: variant(*group)?,
                into: entity(*into)?,
            },
            MappingAction::AssignCandidate(group, _, None) => Decision::Split {
                original: variant(*group)?,
            },
            MappingAction::AssignApplied(id, Some(into), _) => Decision::Merge {
                from: applied(*id)?,
                into: entity(*into)?,
            },
            MappingAction::AssignApplied(id, None, _) => Decision::Split {
                original: applied(*id)?,
            },
            MappingAction::Scoped {
                identity,
                ranges,
                target,
                alias,
                category,
            } => {
                let original = ranges
                    .first()
                    .and_then(|range| self.mention_original(range))
                    .or_else(|| entity(*identity))?;
                match (target, alias, category) {
                    (Some(into), _, _) => Decision::Merge {
                        from: original,
                        into: entity(*into)?,
                    },
                    (None, Some(alias), _) => Decision::Rename {
                        original,
                        alias: alias.clone(),
                    },
                    (None, None, Some(category)) => Decision::Category {
                        original,
                        category: *category,
                    },
                    (None, None, None) => Decision::Split { original },
                }
            }
        })
    }
}
