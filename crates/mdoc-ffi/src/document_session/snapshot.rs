//! What the review screen draws, rebuilt after every transition: the text,
//! its mentions with their source byte ranges, and identities by category.
//! Swift and Kotlin keep only view state beside it (ADR 0085).
use super::review::ReviewText;
use crate::library::TextRange;
use std::collections::HashMap;

/// `mdoc_pii::Category`, as the platforms see it.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, uniffi::Enum)]
pub enum Category {
    Person,
    Organization,
    Email,
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
    pub link_hints: Vec<LinkHint>,
    pub proposed: u64,
    pub replaced: u64,
    pub kept: u64,
    pub case_id: Option<String>,
}

pub(super) fn of(
    title: &str,
    warning: Option<&str>,
    document: &ReviewText,
    case_id: Option<&str>,
) -> ReviewSnapshot {
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
        });
    }
    mentions.sort_by_key(|m| (m.range.start, m.range.end));

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
            let proposed = mentions
                .iter()
                .filter(|m| m.identity == Some(id) && m.state == MentionState::Proposed)
                .count() as u64;
            Some(IdentityRow {
                id,
                alias: identity.alias.clone(),
                category: identity.category.into(),
                originals,
                mentions: review.identity_count(id) as u64,
                proposed,
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
    let count = |state| mentions.iter().filter(|m| m.state == state).count() as u64;
    ReviewSnapshot {
        title: title.to_owned(),
        warning: warning.map(str::to_owned),
        text: document.text.clone(),
        link_hints: review
            .merge_hints()
            .into_iter()
            .map(|hint| LinkHint {
                from: hint.from,
                into: hint.into,
                wording: hint.wording.to_string(),
            })
            .collect(),
        proposed: count(MentionState::Proposed),
        replaced: count(MentionState::Replaced),
        kept: count(MentionState::Kept),
        identities,
        mentions,
        case_id: case_id.map(str::to_owned),
    }
}
