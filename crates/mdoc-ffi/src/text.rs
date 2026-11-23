//! Shared records: a byte range in a document's text, and a reply restored
//! in place, which the session and the Vault's replies both give.
use mdoc_pii::restore::RestoreMap;

/// A byte range in UTF-8 text.
#[derive(Clone, Copy, Debug, PartialEq, Eq, uniffi::Record)]
pub struct TextRange {
    pub start: u64,
    pub end: u64,
}

impl TextRange {
    pub(crate) fn of(range: &std::ops::Range<usize>) -> Self {
        Self {
            start: range.start as u64,
            end: range.end as u64,
        }
    }
}

/// A reply restored in place: the restored text, where restored
/// values and unresolved alias-shaped tokens sit in it, and the counts for
/// "N values restored · M unrecognised".
#[derive(Clone, Debug, PartialEq, uniffi::Record)]
pub struct RestoredReply {
    pub text: String,
    pub restored: Vec<TextRange>,
    pub unresolved: Vec<TextRange>,
    /// Distinct identities restored.
    pub values: u64,
    /// Distinct unresolved tokens.
    pub unrecognised: u64,
    /// Whether the mapping had identities to restore: Copy and Share are
    /// off without them.
    pub restorable: bool,
}

impl RestoredReply {
    pub(crate) fn of(map: &RestoreMap, reply: &str) -> Self {
        let restored = map.restore_in_place(reply);
        Self {
            restored: restored
                .restored
                .iter()
                .map(|(r, _)| TextRange::of(r))
                .collect(),
            unresolved: restored.unresolved.iter().map(TextRange::of).collect(),
            values: restored.values as u64,
            unrecognised: restored
                .unresolved
                .iter()
                .filter_map(|range| restored.text.get(range.clone()))
                .collect::<std::collections::HashSet<_>>()
                .len() as u64,
            restorable: !map.is_empty(),
            text: restored.text,
        }
    }
}
