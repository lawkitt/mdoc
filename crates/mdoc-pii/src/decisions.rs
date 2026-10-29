//! "What you did" (ADR 0079): the document's pseudonymization decisions, each
//! marking the undo point it created. Memory only: entries name originals, so
//! nothing here is serializable, and the log lives as long as its tab.
use super::{Category, Review};
use mdoc_history::HistoryPath;
use std::sync::Arc;

/// The mentions one decision acted on: a single one is named, a bulk action
/// is counted ("Applied 24 replacements").
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Mentions {
    One(Arc<str>),
    Many(usize),
}

impl Mentions {
    /// The originals of the mentions acted on, one per mention.
    pub fn of(originals: impl IntoIterator<Item = Arc<str>>) -> Self {
        let mut originals = originals.into_iter();
        match (originals.next(), originals.count()) {
            (Some(original), 0) => Self::One(original),
            (Some(_), more) => Self::Many(more + 1),
            (None, _) => Self::Many(0),
        }
    }
}

/// One pseudonymization decision. Ordinary text edits are never decisions.
/// Adding a kind is a variant here and its label below.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Decision {
    Apply(Mentions),
    Keep(Mentions),
    /// Kept mentions proposed for replacement again.
    Repropose(Mentions),
    UndoReplacement(Mentions),
    Rename {
        original: Arc<str>,
        alias: String,
    },
    Category {
        original: Arc<str>,
        category: Category,
    },
    Merge {
        from: Arc<str>,
        into: Arc<str>,
    },
    /// Mentions moved to an identity of their own, in `category` when the
    /// split also changed it.
    Split {
        original: Arc<str>,
        category: Option<Category>,
    },
    Add {
        original: Arc<str>,
    },
    Restore(Mentions),
    JoinCase {
        case: String,
    },
}

fn count(n: usize, one: &str, many: &str) -> String {
    format!("{n} {}", if n == 1 { one } else { many })
}

impl Decision {
    /// The decision's line, without its time.
    pub fn label(&self) -> String {
        use Mentions::{Many, One};
        match self {
            Self::Apply(One(original)) => format!("Applied “{original}”"),
            Self::Apply(Many(n)) => format!("Applied {}", count(*n, "replacement", "replacements")),
            Self::Keep(One(original)) => format!("Kept “{original}”"),
            Self::Keep(Many(n)) => format!("Kept {}", count(*n, "original", "originals")),
            Self::Repropose(One(original)) => format!("Proposed “{original}” again"),
            Self::Repropose(Many(n)) => {
                format!("Proposed {} again", count(*n, "mention", "mentions"))
            }
            Self::UndoReplacement(One(original)) => {
                format!("Undid replacement of “{original}”")
            }
            Self::UndoReplacement(Many(n)) => {
                format!("Undid {}", count(*n, "replacement", "replacements"))
            }
            Self::Rename { original, alias } => format!("Renamed “{original}” to {alias}"),
            Self::Category { original, category } => {
                format!("Changed “{original}” to {}", category.label())
            }
            Self::Merge { from, into } => format!("Merged “{from}” into “{into}”"),
            Self::Split {
                original,
                category: None,
            } => format!("Separated “{original}”"),
            Self::Split {
                original,
                category: Some(category),
            } => format!("Separated “{original}” as {}", category.label()),
            Self::Add { original } => format!("Added “{original}”"),
            Self::Restore(One(original)) => format!("Restored “{original}”"),
            Self::Restore(Many(n)) => format!("Restored {}", count(*n, "name", "names")),
            Self::JoinCase { case } => format!("Joined case “{case}”"),
        }
    }
}

/// The decisions one undo point made, and the history state it created.
#[derive(Clone, Debug)]
pub struct Entry {
    history: u64,
    /// Oldest first; more than one when an action bundles several decisions
    /// on one step (a rename confirmed with Apply).
    decisions: Vec<Decision>,
    time: String,
    /// Its undo point fell past the editor's undo limit.
    past_limit: bool,
}

impl Entry {
    pub fn decisions(&self) -> &[Decision] {
        &self.decisions
    }
    /// The entry's line, without its time: one decision's label, or several
    /// joined ("Renamed “Maya Bennett” to CLIENT · applied 4 replacements").
    pub fn label(&self) -> String {
        let mut label = String::new();
        for decision in &self.decisions {
            let part = decision.label();
            if label.is_empty() {
                label = part;
            } else {
                label.push_str(" · ");
                let mut chars = part.chars();
                label.extend(chars.next().into_iter().flat_map(char::to_lowercase));
                label.push_str(chars.as_str());
            }
        }
        label
    }
    /// Local wall-clock time of the decision, as the host wrote it ("14:02").
    pub fn time(&self) -> &str {
        &self.time
    }
    /// "Kept “Ms Bennett” · 14:02".
    pub fn line(&self) -> String {
        format!("{} · {}", self.label(), self.time)
    }
}

/// An entry still reached by Undo (done) or only by Redo (undone), or one
/// whose undo point the editor's undo limit dropped: still listed, no longer
/// reached.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum EntryState {
    Done,
    Undone,
    PastLimit,
}

/// The decisions of one document, oldest first, each on its undo point.
/// Travelling to an entry is exactly that many ⌘Z or ⇧⌘Z presses.
#[derive(Default)]
pub struct DecisionLog {
    entries: Vec<Entry>,
}

impl DecisionLog {
    /// Record `decision` as the step from `before` to the editor's current
    /// state. A decision that changed nothing is not listed; one recorded on
    /// the step an entry already has (an action bundling several) joins it.
    pub fn record(&mut self, before: u64, decision: Decision, time: String, path: &HistoryPath) {
        self.follow(path);
        let history = path.current;
        if history == before {
            return;
        }
        match self.entries.last_mut() {
            Some(last) if last.history == history && !last.past_limit => {
                if !last.decisions.contains(&decision) {
                    last.decisions.push(decision);
                }
            }
            _ => self.entries.push(Entry {
                history,
                decisions: vec![decision],
                time,
                past_limit: false,
            }),
        }
    }
    /// Follow the editor's history. Undone entries go once a new decision or
    /// edit clears Redo. Ones the undo limit dropped stay, marked: the limit
    /// evicts the oldest states, so an entry older than every state Undo
    /// still reaches fell past it. A reload clears the log ([`Self::clear`]).
    pub fn follow(&mut self, path: &HistoryPath) {
        let oldest = path.undo.first().copied();
        self.entries.retain_mut(|entry| {
            if !entry.past_limit && path.steps_to(entry.history).is_none() {
                entry.past_limit = oldest.is_some_and(|oldest| entry.history < oldest);
                return entry.past_limit;
            }
            true
        });
    }
    pub fn clear(&mut self) {
        self.entries.clear();
    }
    /// Every listed entry, oldest first, with its state.
    pub fn rows<'a>(
        &'a self,
        path: &'a HistoryPath,
    ) -> impl Iterator<Item = (&'a Entry, EntryState)> + 'a {
        self.entries.iter().filter_map(|entry| {
            if entry.past_limit {
                return Some((entry, EntryState::PastLimit));
            }
            let steps = path.steps_to(entry.history)?;
            Some((
                entry,
                if steps > 0 {
                    EntryState::Undone
                } else {
                    EntryState::Done
                },
            ))
        })
    }
    /// The presses that travel to row `index` of [`Self::rows`]: negative
    /// for Undo, positive for Redo, `None` past the undo limit. Undoing goes
    /// back to just after the entry, redoing up to it.
    pub fn steps_to(&self, index: usize, path: &HistoryPath) -> Option<isize> {
        let (entry, state) = self.rows(path).nth(index)?;
        if state == EntryState::PastLimit {
            return None;
        }
        path.steps_to(entry.history)
    }
}

impl Review {
    /// The wording an entry names for identity `id`: its longest original,
    /// applied or proposed.
    pub fn identity_wording(&self, id: u64) -> Option<Arc<str>> {
        let applied = self
            .applied()
            .iter()
            .filter(|a| a.step.identity == id)
            .map(|a| a.step.original_shared().clone());
        let proposed = self
            .variants()
            .iter()
            .filter(|v| self.variant_identity(v.id) == Some(id))
            .map(|v| v.original.clone());
        applied
            .chain(proposed)
            .max_by_key(|original| original.chars().count())
    }
}

#[cfg(test)]
mod tests {
    use super::{Decision, DecisionLog, EntryState, Mentions};
    use crate::Category;
    use mdoc_history::HistoryPath;

    fn path(undo: &[u64], current: u64, redo: &[u64]) -> HistoryPath {
        HistoryPath {
            undo: undo.to_vec(),
            current,
            redo: redo.to_vec(),
        }
    }
    fn kept(name: &str) -> Decision {
        Decision::Keep(Mentions::One(name.into()))
    }

    #[test]
    fn labels_name_one_mention_and_count_a_bulk_action() {
        assert_eq!(
            Decision::Apply(Mentions::of(["a".into(), "b".into()])).label(),
            "Applied 2 replacements"
        );
        assert_eq!(kept("Ms Bennett").label(), "Kept “Ms Bennett”");
        assert_eq!(
            Decision::Category {
                original: "Acme".into(),
                category: Category::Organization
            }
            .label(),
            "Changed “Acme” to Organization"
        );
        let mut log = DecisionLog::default();
        log.record(1, kept("Ms Bennett"), "14:02".into(), &path(&[1], 2, &[]));
        let p = path(&[1], 2, &[]);
        let (entry, _) = log.rows(&p).next().unwrap();
        assert_eq!(entry.line(), "Kept “Ms Bennett” · 14:02");
    }

    #[test]
    fn entries_follow_undo_and_a_new_decision_drops_undone_ones() {
        let mut log = DecisionLog::default();
        log.record(1, kept("A"), "t".into(), &path(&[1], 2, &[]));
        log.record(2, kept("B"), "t".into(), &path(&[1, 2], 3, &[]));
        // A text edit between decisions is no entry.
        log.record(3, kept("C"), "t".into(), &path(&[1, 2, 3, 4], 5, &[]));
        let now = path(&[1, 2, 3, 4], 5, &[]);
        assert_eq!(log.steps_to(0, &now), Some(-3));
        assert_eq!(log.steps_to(2, &now), Some(0));
        // Travel to B: C is undone and redo reaches it.
        let back = path(&[1, 2], 3, &[4, 5]);
        let states: Vec<_> = log.rows(&back).map(|(_, s)| s).collect();
        assert_eq!(
            states,
            [EntryState::Done, EntryState::Done, EntryState::Undone]
        );
        assert_eq!(log.steps_to(2, &back), Some(2));
        // A new decision clears Redo, and C with it.
        log.record(3, kept("D"), "t".into(), &path(&[1, 2, 3], 6, &[]));
        let labels: Vec<_> = log
            .rows(&path(&[1, 2, 3], 6, &[]))
            .map(|(e, _)| e.label())
            .collect();
        assert_eq!(labels, ["Kept “A”", "Kept “B”", "Kept “D”"]);
    }

    #[test]
    fn a_no_op_is_not_listed_and_decisions_on_one_step_share_its_entry() {
        let mut log = DecisionLog::default();
        log.record(1, kept("A"), "t".into(), &path(&[], 1, &[]));
        assert_eq!(log.rows(&path(&[], 1, &[])).count(), 0);
        log.record(
            1,
            Decision::Rename {
                original: "A".into(),
                alias: "CLIENT".into(),
            },
            "t".into(),
            &path(&[1], 2, &[]),
        );
        log.record(
            1,
            Decision::Apply(Mentions::Many(3)),
            "t".into(),
            &path(&[1], 2, &[]),
        );
        let labels: Vec<_> = log
            .rows(&path(&[1], 2, &[]))
            .map(|(e, _)| e.label())
            .collect();
        assert_eq!(labels, ["Renamed “A” to CLIENT · applied 3 replacements"]);
        // A reload clears the history, and the log with it.
        log.clear();
        assert_eq!(log.rows(&path(&[], 9, &[])).count(), 0);
    }

    #[test]
    fn entries_past_the_undo_limit_stay_listed_but_unreachable() {
        let mut log = DecisionLog::default();
        log.record(1, kept("A"), "t".into(), &path(&[1], 2, &[]));
        log.record(4, kept("B"), "t".into(), &path(&[1, 2, 3, 4], 5, &[]));
        // The limit evicted 1 and 2: A is older than every state Undo reaches.
        let capped = path(&[3, 4, 5, 6], 7, &[]);
        log.follow(&capped);
        let states: Vec<_> = log.rows(&capped).map(|(_, s)| s).collect();
        assert_eq!(states, [EntryState::PastLimit, EntryState::Done]);
        assert_eq!(log.steps_to(0, &capped), None);
        assert_eq!(log.steps_to(1, &capped), Some(-2));
        // Undo back past B, then type: B is dropped with Redo, A stays.
        log.follow(&path(&[3], 4, &[5, 6, 7]));
        let branched = path(&[3, 4], 8, &[]);
        log.follow(&branched);
        let labels: Vec<_> = log.rows(&branched).map(|(e, _)| e.label()).collect();
        assert_eq!(labels, ["Kept “A”"]);
        // A later decision never joins an entry past the limit.
        log.record(8, kept("C"), "t".into(), &path(&[3, 4, 8], 9, &[]));
        assert_eq!(log.rows(&path(&[3, 4, 8], 9, &[])).count(), 2);
    }

    #[test]
    fn split_and_repropose_labels() {
        let split = |category| Decision::Split {
            original: "Maya".into(),
            category,
        };
        assert_eq!(split(None).label(), "Separated “Maya”");
        assert_eq!(
            split(Some(Category::Organization)).label(),
            "Separated “Maya” as Organization"
        );
        assert_eq!(
            Decision::Repropose(Mentions::One("Maya".into())).label(),
            "Proposed “Maya” again"
        );
        assert_eq!(
            Decision::Repropose(Mentions::Many(2)).label(),
            "Proposed 2 mentions again"
        );
    }
}
