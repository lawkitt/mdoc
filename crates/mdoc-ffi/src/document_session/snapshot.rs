//! What the review screen draws, rebuilt after every transition: the text,
//! its mentions with their source byte ranges, identities by category, merge
//! hints and the undo history's ends. Swift and Kotlin keep only view state
//! beside it (ADR 0085, ADR 0098).
use super::review::{Focus, ReviewText, Step, StepKind};
use crate::library::TextRange;
use std::collections::HashMap;

/// `mdoc_pii::Category`, as the platforms see it.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, uniffi::Enum)]
pub enum Category {
    Person,
    Organization,
    Email,
    Url,
    Phone,
    Address,
    Identity,
    Tax,
    Bank,
    CaseNumber,
    Date,
    Other,
}

impl From<mdoc_pii::Category> for Category {
    fn from(category: mdoc_pii::Category) -> Self {
        use mdoc_pii::Category as C;
        match category {
            C::Person => Self::Person,
            C::Organization => Self::Organization,
            C::Email => Self::Email,
            C::Url => Self::Url,
            C::Phone => Self::Phone,
            C::Address => Self::Address,
            C::Identity => Self::Identity,
            C::Tax => Self::Tax,
            C::Bank => Self::Bank,
            C::CaseNumber => Self::CaseNumber,
            C::Date => Self::Date,
            C::Other => Self::Other,
        }
    }
}

impl From<Category> for mdoc_pii::Category {
    fn from(category: Category) -> Self {
        match category {
            Category::Person => Self::Person,
            Category::Organization => Self::Organization,
            Category::Email => Self::Email,
            Category::Url => Self::Url,
            Category::Phone => Self::Phone,
            Category::Address => Self::Address,
            Category::Identity => Self::Identity,
            Category::Tax => Self::Tax,
            Category::Bank => Self::Bank,
            Category::CaseNumber => Self::CaseNumber,
            Category::Date => Self::Date,
            Category::Other => Self::Other,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, uniffi::Enum)]
pub enum MentionState {
    /// A candidate waiting for Replace or Keep.
    Proposed,
    /// Replaced: the text holds the alias.
    Replaced,
    /// Kept: the text holds the original.
    Kept,
}

/// One mention in the text, in source order.
#[derive(Clone, Debug, PartialEq, uniffi::Record)]
pub struct Mention {
    /// Byte range in `ReviewSnapshot::text`; transitions name mentions by it.
    pub range: TextRange,
    pub state: MentionState,
    /// The original wording; for a replaced mention, what the alias stands for.
    pub original: String,
    /// The alias it has or would get; none for a kept mention without an identity.
    pub alias: Option<String>,
    pub category: Category,
    pub identity: Option<u64>,
    /// Stays the same while the mention does, across Replace, Keep, edits
    /// elsewhere, Undo and Redo (ADR 0104); transitions by id take it.
    pub id: u64,
}

/// Where a document's review stands, as lists and the review bar show it.
#[derive(Clone, Copy, Debug, PartialEq, Eq, uniffi::Enum)]
pub enum DocumentPhase {
    /// Never scanned, nor proposed names some other way.
    Unscanned,
    /// Scanned, with proposals waiting.
    Reviewing,
    /// Scanned, nothing waiting: ready for Copy for AI.
    Ready,
}

impl DocumentPhase {
    pub(crate) fn of(scanned: bool, proposed: u64) -> Self {
        match (scanned, proposed) {
            (false, _) => Self::Unscanned,
            (true, 0) => Self::Ready,
            (true, _) => Self::Reviewing,
        }
    }
}

/// One identity: an alias and the wordings it stands for.
#[derive(Clone, Debug, PartialEq, uniffi::Record)]
pub struct IdentityRow {
    pub id: u64,
    pub alias: String,
    pub category: Category,
    /// The primary wording first, the one restore puts back.
    pub originals: Vec<String>,
    /// Proposed and replaced mentions.
    pub mentions: u64,
    pub proposed: u64,
    /// Belongs to…: the person or organization a contact or identifier
    /// belongs to.
    pub owner: Option<u64>,
    /// Proposed while any mention is; kept when every mention is kept;
    /// replaced otherwise.
    pub state: MentionState,
    /// What the identity list counts: `mentions`, or every mention of it in
    /// the text when more (kept ones included).
    pub mention_count: u64,
}

/// One row of the category menu: choosing `category` gives `alias`, or
/// keeps the alias as it is when `None` (desktop's #179).
#[derive(Clone, Debug, PartialEq, uniffi::Record)]
pub struct CategoryChoice {
    pub category: Category,
    pub alias: Option<String>,
    /// The mention's identity is in this category already.
    pub current: bool,
}

/// An undo step Undo or Redo reaches (ADR 0098): the toast names it, and
/// the bar moves to where it acted.
#[derive(Clone, Debug, PartialEq, uniffi::Record)]
pub struct HistoryStep {
    pub kind: StepKind,
    /// "Replace · Riverside Holdings", "Same as · Maya Bennett".
    pub label: String,
    /// What the label names after the kind: a wording, an alias, a
    /// category, a count; empty for none.
    pub subject: String,
    /// The mention it acted on, in this snapshot's text: for the step Undo
    /// reverts, where it is now; for the one Redo makes, where it is before.
    pub range: Option<TextRange>,
    /// The identity it acted on, in this snapshot.
    pub identity: Option<u64>,
}

/// Two identities that look like one subject (ADR 0080); linking them is
/// the user's explicit choice.
#[derive(Clone, Debug, PartialEq, uniffi::Record)]
pub struct LinkHint {
    pub from: u64,
    pub into: u64,
    /// The fuller identity's primary wording: "Looks like …".
    pub wording: String,
}

#[derive(Clone, Debug, PartialEq, uniffi::Record)]
pub struct ReviewSnapshot {
    pub title: String,
    /// An import's partial-result warning.
    pub warning: Option<String>,
    /// The Markdown source; mention ranges index it.
    pub text: String,
    pub mentions: Vec<Mention>,
    /// In category order (Person, Organization, …), then by first mention.
    pub identities: Vec<IdentityRow>,
    /// Merge hints, without the ones dismissed here or in the case.
    pub link_hints: Vec<LinkHint>,
    pub proposed: u64,
    pub replaced: u64,
    pub kept: u64,
    pub case_id: Option<String>,
    pub can_undo: bool,
    pub can_redo: bool,
    /// The step Undo reverts; after a redo, the step it made again.
    pub undo_step: Option<HistoryStep>,
    /// The step Redo makes again; after an undo, the step it undid.
    pub redo_step: Option<HistoryStep>,
    /// Scanned, or names proposed some other way, even if none were found;
    /// kept with the document.
    pub scanned: bool,
    pub phase: DocumentPhase,
    /// Names kept everywhere: no identity in `identities` any more, but
    /// their kept mentions still name them, so the list shows them as kept.
    /// In order of first mention.
    pub kept_identities: Vec<IdentityRow>,
    /// Changes when `text` does, and only then: an app keeps what it built
    /// from the text (its Markdown styling) while this stays.
    pub text_revision: u64,
}

/// The snapshot of `document`, whose ids are synced (`ReviewText::sync_ids`).
pub(super) fn of(
    title: &str,
    warning: Option<&str>,
    document: &ReviewText,
    case_id: Option<&str>,
    scanned: bool,
) -> ReviewSnapshot {
    let id = |range: &std::ops::Range<usize>| {
        let id = document.ids.id(range);
        debug_assert!(id.is_some(), "mention ids are synced");
        id.unwrap_or_default()
    };
    let review = &document.review;
    let alias = |identity: Option<u64>| {
        identity
            .and_then(|id| review.identity(id))
            .map(|i| i.alias.clone())
    };
    let mut mentions = Vec::new();
    for candidate in review.candidates() {
        let Some(variant) = review.variant(candidate.variant) else {
            continue;
        };
        let identity = review.occurrence_identity(candidate.variant, &candidate.range);
        mentions.push(Mention {
            range: TextRange::of(&candidate.range),
            state: MentionState::Proposed,
            original: variant.original.to_string(),
            alias: alias(identity),
            category: identity
                .and_then(|id| review.identity(id))
                .map_or(variant.category, |i| i.category)
                .into(),
            identity,
            id: id(&candidate.range),
        });
    }
    for applied in review.applied() {
        mentions.push(Mention {
            range: TextRange::of(&applied.range),
            state: MentionState::Replaced,
            original: applied.step.original().to_owned(),
            alias: Some(applied.step.after.to_string()),
            category: applied.step.category.into(),
            identity: Some(applied.step.identity).filter(|id| *id != 0),
            id: id(&applied.range),
        });
    }
    let kept = review.kept_mentions();
    for (range, original, identity) in &kept {
        mentions.push(Mention {
            range: TextRange::of(range),
            state: MentionState::Kept,
            original: original.to_string(),
            alias: alias(*identity),
            category: identity
                .and_then(|id| review.identity(id))
                .map_or(mdoc_pii::Category::Other, |i| i.category)
                .into(),
            identity: *identity,
            id: id(range),
        });
    }
    mentions.sort_by_key(|m| (m.range.start, m.range.end));
    // Each identity's mentions: how many, and in which states.
    #[derive(Default)]
    struct Tally {
        count: u64,
        proposed: u64,
        kept: u64,
    }
    let mut tallies: HashMap<u64, Tally> = HashMap::new();
    for m in &mentions {
        if let Some(identity) = m.identity {
            let tally = tallies.entry(identity).or_default();
            tally.count += 1;
            tally.proposed += u64::from(m.state == MentionState::Proposed);
            tally.kept += u64::from(m.state == MentionState::Kept);
        }
    }
    let state = |tally: Option<&Tally>| match tally {
        Some(t) if t.proposed > 0 => MentionState::Proposed,
        Some(t) if t.kept == t.count => MentionState::Kept,
        _ => MentionState::Replaced,
    };

    let exported: HashMap<_, _> = review.export_identities_with_ids().into_iter().collect();
    let first: HashMap<u64, u64> = mentions.iter().rev().fold(HashMap::new(), |mut first, m| {
        if let Some(id) = m.identity {
            first.insert(id, m.range.start);
        }
        first
    });
    let mut identities: Vec<_> = review
        .active_identities()
        .into_iter()
        .filter_map(|id| {
            let identity = review.identity(id)?;
            let originals = exported.get(&id).map_or_else(
                || {
                    review
                        .identity_wording(id)
                        .into_iter()
                        .map(|w| w.to_string())
                        .collect()
                },
                |e| e.originals.iter().map(|o| o.to_string()).collect(),
            );
            let tally = tallies.get(&id);
            // `Review::identity_count`, from the mentions already found: it
            // walks every mention for each identity.
            let mentions = tally.map_or(0, |t| t.count - t.kept);
            Some(IdentityRow {
                id,
                alias: identity.alias.clone(),
                category: identity.category.into(),
                originals,
                mentions,
                proposed: tally.map_or(0, |t| t.proposed),
                owner: identity.owner,
                state: state(tally),
                mention_count: mentions.max(tally.map_or(0, |t| t.count)),
            })
        })
        .collect();
    let order = |category: Category| {
        mdoc_pii::Category::ALL
            .iter()
            .position(|c| Category::from(*c) == category)
            .unwrap_or(usize::MAX)
    };
    identities.sort_by_key(|row| {
        (
            order(row.category),
            first.get(&row.id).copied().unwrap_or(u64::MAX),
        )
    });
    // Names kept everywhere, which no active identity lists.
    let listed: std::collections::HashSet<u64> = identities.iter().map(|row| row.id).collect();
    let mut kept_identities: Vec<IdentityRow> = Vec::new();
    for m in &mentions {
        let Some(id) = m.identity.filter(|id| !listed.contains(id)) else {
            continue;
        };
        if m.state != MentionState::Kept || m.alias.is_none() {
            continue;
        }
        match kept_identities.iter_mut().find(|row| row.id == id) {
            Some(row) => {
                if !row.originals.contains(&m.original) {
                    row.originals.push(m.original.clone());
                }
                row.mentions += 1;
                row.mention_count += 1;
            }
            None => kept_identities.push(IdentityRow {
                id,
                alias: m.alias.clone().unwrap_or_default(),
                category: m.category,
                originals: vec![m.original.clone()],
                mentions: 1,
                proposed: 0,
                owner: None,
                state: MentionState::Kept,
                mention_count: 1,
            }),
        }
    }
    let count = |state| mentions.iter().filter(|m| m.state == state).count() as u64;
    let proposed = count(MentionState::Proposed);
    let step = |(step, focus): (&Step, &Focus)| {
        let mention = focus
            .range
            .as_ref()
            .and_then(|r| mentions.iter().find(|m| m.range == TextRange::of(r)));
        HistoryStep {
            kind: step.kind,
            label: step.label(),
            subject: step.subject.clone(),
            range: focus.range.as_ref().map(TextRange::of),
            identity: mention.and_then(|m| m.identity).or(focus.identity),
        }
    };
    let undo_step = document.undo_step().map(step);
    let redo_step = document.redo_step().map(step);
    ReviewSnapshot {
        can_undo: document.can_undo(),
        can_redo: document.can_redo(),
        undo_step,
        redo_step,
        title: title.to_owned(),
        warning: warning.map(str::to_owned),
        text: document.text.clone(),
        link_hints: document
            .merge_hints()
            .into_iter()
            .map(|hint| LinkHint {
                from: hint.from,
                into: hint.into,
                wording: hint.wording.to_string(),
            })
            .collect(),
        proposed,
        replaced: count(MentionState::Replaced),
        kept: count(MentionState::Kept),
        identities,
        mentions,
        case_id: case_id.map(str::to_owned),
        scanned,
        phase: DocumentPhase::of(scanned, proposed),
        kept_identities,
        text_revision: document.text_revision,
    }
}
