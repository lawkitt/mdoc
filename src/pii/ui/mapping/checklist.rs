//! The replacements checklist (ADR 0048): a tick per entity and per mention.
//! Ticked means "will be replaced"; unticking keeps the originals as one undo
//! step. Kept mentions come from the review's Keep decisions, so an entity
//! stays listed, unticked, and undo brings its tick back.
use super::*;

/// What a row's checkbox shows.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub(in crate::pii::ui) enum Tick {
    On,
    Off,
    Mixed,
}

impl DocumentView {
    /// An entity's live mentions: pending (variant, range) and applied ids.
    fn entity_mentions(&self, id: u64, only: Option<u64>) -> (Vec<(u64, Range<usize>)>, Vec<u64>) {
        let review = &self.pii.review;
        let (mut pending, mut applied) = (Vec::new(), Vec::new());
        for (annotation, range) in self.identity_occurrences(id) {
            if only.is_some_and(|a| a != annotation) {
                continue;
            }
            if annotation & APPLIED_ID != 0 {
                applied.push(annotation & !APPLIED_ID);
            } else if let Some(candidate) = review.candidate(annotation) {
                pending.push((candidate.variant, range));
            }
        }
        (pending, applied)
    }

    /// Every kept mention with the entity it belongs to. When the review no
    /// longer links a wording to an entity (all its mentions kept), the
    /// record taken at untick time does, so the row never disappears.
    pub(in crate::pii::ui) fn kept_entries(&self) -> Vec<(Range<usize>, Arc<str>, Option<u64>)> {
        let names = &self.pii.mapping.kept_names;
        self.pii
            .review
            .kept_mentions()
            .into_iter()
            .map(|(range, original, identity)| {
                let identity = identity.or_else(|| names.get(&original).map(|k| k.identity));
                (range, original, identity)
            })
            .collect()
    }

    /// Kept mentions of an entity: (range, wording).
    pub(in crate::pii::ui) fn kept_of(&self, id: u64) -> Vec<(Range<usize>, Arc<str>)> {
        self.kept_entries()
            .into_iter()
            .filter(|(_, _, identity)| *identity == Some(id))
            .map(|(range, original, _)| (range, original))
            .collect()
    }

    /// The checkbox state of an entity row.
    pub(in crate::pii::ui) fn entity_tick(&self, id: u64) -> Tick {
        let live = !self.identity_occurrences(id).is_empty();
        let kept = !self.kept_of(id).is_empty();
        match (live, kept) {
            (true, false) => Tick::On,
            (true, true) => Tick::Mixed,
            _ => Tick::Off,
        }
    }

    /// Untick an entity (or one of its mentions): keep its originals.
    pub(in crate::pii::ui) fn untick(
        &mut self,
        id: u64,
        only: Option<u64>,
        cx: &mut Context<Self>,
    ) {
        let (pending, applied) = self.entity_mentions(id, only);
        // Remember which entity each kept wording belongs to.
        if let Some(identity) = self.pii.review.identity(id).cloned() {
            let review = &self.pii.review;
            let wordings: Vec<Arc<str>> = pending
                .iter()
                .filter_map(|(variant, _)| review.variant(*variant).map(|v| v.original.clone()))
                .chain(applied.iter().filter_map(|a| {
                    review
                        .applied_occurrence(*a)
                        .map(|a| a.step.original_shared().clone())
                }))
                .collect();
            for original in wordings {
                self.pii.mapping.kept_names.insert(
                    original,
                    KeptName {
                        identity: id,
                        alias: identity.alias.clone(),
                        category: identity.category,
                    },
                );
            }
        }
        self.keep_mentions_now(pending, applied, cx);
    }

    /// Tick an unticked or mixed entity: propose its kept wordings again, as
    /// one undo step.
    pub(in crate::pii::ui) fn retick(&mut self, id: u64, cx: &mut Context<Self>) {
        if self.pii.scanning() || !self.can_copy_markdown() {
            return;
        }
        let Some(category) = self
            .pii
            .review
            .identity(id)
            .map(|i| i.category)
            .or_else(|| {
                self.pii
                    .mapping
                    .kept_names
                    .values()
                    .find(|k| k.identity == id)
                    .map(|k| k.category)
            })
        else {
            return;
        };
        let source = self.editor.read(cx).text().to_owned();
        let mut seen = std::collections::HashSet::new();
        let ranges: Vec<Range<usize>> = self
            .kept_of(id)
            .into_iter()
            .filter(|(range, original)| {
                source.get(range.clone()) == Some(original.as_ref())
                    && seen.insert(original.clone())
            })
            .map(|(range, _)| range)
            .collect();
        if ranges.is_empty() {
            return;
        }
        let added = self.checkpoint_review(cx, |review| {
            ranges
                .iter()
                .filter(|range| {
                    review
                        .add_manual(&source, (*range).clone(), category)
                        .is_ok()
                })
                .count()
        });
        if added.unwrap_or(0) == 0 {
            self.pii.error = Some("Couldn't propose it again; select the text to add it.".into());
            cx.notify();
            return;
        }
        self.pii.reviewing = true;
        self.pii.error = None;
        self.sync_annotations(cx);
        cx.notify();
    }

    /// Tick one kept mention: propose it again, as one undo step; other kept
    /// mentions of the same wording stay kept.
    pub(in crate::pii::ui) fn retick_mention(
        &mut self,
        id: u64,
        range: Range<usize>,
        cx: &mut Context<Self>,
    ) {
        if self.pii.scanning() || !self.can_copy_markdown() {
            return;
        }
        let Some(category) = self
            .pii
            .review
            .identity(id)
            .map(|i| i.category)
            .or_else(|| {
                self.pii
                    .mapping
                    .kept_names
                    .values()
                    .find(|k| k.identity == id)
                    .map(|k| k.category)
            })
        else {
            return;
        };
        let source = self.editor.read(cx).text().to_owned();
        let Some(original) = source.get(range.clone()).map(Arc::<str>::from) else {
            return;
        };
        // Re-proposing a wording lifts all its Keep decisions; the other kept
        // mentions of that wording are kept again in the same undo step.
        let others: Vec<Range<usize>> = self
            .kept_of(id)
            .into_iter()
            .filter(|(r, o)| *o == original && *r != range)
            .map(|(r, _)| r)
            .collect();
        let added = self.checkpoint_review(cx, |review| {
            let Ok(variant) = review.add_manual(&source, range, category) else {
                return false;
            };
            review.keep_mentions(others.into_iter().map(|r| (variant, r)));
            true
        });
        if added != Some(true) {
            self.pii.error = Some("Couldn't propose it again; select the text to add it.".into());
        } else {
            self.pii.reviewing = true;
            self.pii.error = None;
            self.sync_annotations(cx);
        }
        cx.notify();
    }

    /// A click or Space on an entity row's checkbox.
    pub(in crate::pii::ui) fn toggle_entity(&mut self, id: u64, cx: &mut Context<Self>) {
        match self.entity_tick(id) {
            Tick::On => self.untick(id, None, cx),
            Tick::Off | Tick::Mixed => self.retick(id, cx),
        }
    }

    /// A click or Space on a mention row's checkbox: keep this one original.
    pub(in crate::pii::ui) fn toggle_mention(&mut self, annotation: u64, cx: &mut Context<Self>) {
        let review = &self.pii.review;
        let identity = if annotation & APPLIED_ID != 0 {
            review
                .applied_occurrence(annotation & !APPLIED_ID)
                .map(|a| a.step.identity)
        } else {
            review
                .candidate(annotation)
                .and_then(|c| review.occurrence_identity(c.variant, &c.range))
        };
        if let Some(identity) = identity {
            self.untick(identity, Some(annotation), cx);
        }
    }
}
