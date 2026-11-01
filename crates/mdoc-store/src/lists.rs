//! The user's lists (ADR 0092): wordings scans always propose (Always) or
//! never propose (Never). Each case keeps its own, from Keep and manual
//! marks in its members; Settings › Detection keeps user-wide ones, edited
//! by hand. Both can name clients, so they are stored only in the
//! keychain-protected case store, never in `settings.json`.
use mdoc_pii::Category;
use serde::{Deserialize, Serialize};

/// The wording lists compare by: lowercase, with each run of whitespace one
/// space and none at either end. Matching is whole-word in this form.
pub fn key(wording: &str) -> String {
    wording
        .split_whitespace()
        .map(str::to_lowercase)
        .collect::<Vec<_>>()
        .join(" ")
}

/// A wording scans always propose, in the category a manual mark gave it.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct AlwaysEntry {
    pub wording: String,
    /// The category token, e.g. `PERSON`.
    pub category: String,
}

impl AlwaysEntry {
    pub fn category(&self) -> Category {
        Category::ALL
            .into_iter()
            .find(|c| c.token() == self.category)
            .unwrap_or(Category::Other)
    }
}

/// One Always and one Never list. A wording is on at most one of them: the
/// later addition moves it.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct Lists {
    #[serde(default)]
    pub always: Vec<AlwaysEntry>,
    #[serde(default)]
    pub never: Vec<String>,
}

impl Lists {
    pub fn is_empty(&self) -> bool {
        self.always.is_empty() && self.never.is_empty()
    }
    /// Whether `wording` is on Never.
    pub fn never_has(&self, wording: &str) -> bool {
        let key = key(wording);
        self.never.iter().any(|w| self::key(w) == key)
    }
    /// The Always entry for `wording`.
    pub fn always_for(&self, wording: &str) -> Option<&AlwaysEntry> {
        let key = key(wording);
        self.always.iter().find(|e| self::key(&e.wording) == key)
    }
    /// Put `wording` on Never, taking it off Always. Returns whether the
    /// lists changed.
    pub fn add_never(&mut self, wording: &str) -> bool {
        let wording = wording.trim();
        if key(wording).is_empty() || self.never_has(wording) {
            return false;
        }
        self.remove_always(wording);
        self.never.push(wording.to_string());
        true
    }
    /// Put `wording` on Always in `category`, taking it off Never; an entry
    /// already there takes the new category. Returns whether the lists
    /// changed.
    pub fn add_always(&mut self, wording: &str, category: Category) -> bool {
        let wording = wording.trim();
        if key(wording).is_empty() {
            return false;
        }
        if self
            .always_for(wording)
            .is_some_and(|e| e.category == category.token())
        {
            return false;
        }
        self.remove_always(wording);
        self.remove_never(wording);
        self.always.push(AlwaysEntry {
            wording: wording.to_string(),
            category: category.token().into(),
        });
        true
    }
    pub fn remove_never(&mut self, wording: &str) -> bool {
        let key = key(wording);
        let before = self.never.len();
        self.never.retain(|w| self::key(w) != key);
        self.never.len() != before
    }
    pub fn remove_always(&mut self, wording: &str) -> bool {
        let key = key(wording);
        let before = self.always.len();
        self.always.retain(|e| self::key(&e.wording) != key);
        self.always.len() != before
    }
    /// What a scan uses: a case's lists over the user-wide ones, so a case's
    /// Never hides a user-wide Always wording and the other way round. A
    /// document outside a case passes no case and gets the user-wide lists.
    pub fn effective(case: Option<&Lists>, user: &Lists) -> Lists {
        let Some(case) = case else {
            return user.clone();
        };
        let mut lists = case.clone();
        for wording in &user.never {
            if lists.always_for(wording).is_none() && !lists.never_has(wording) {
                lists.never.push(wording.clone());
            }
        }
        for entry in &user.always {
            if !lists.never_has(&entry.wording) && lists.always_for(&entry.wording).is_none() {
                lists.always.push(entry.clone());
            }
        }
        lists
    }
    /// Carry out `changes`, removals first. Returns whether the lists
    /// changed.
    pub fn apply(&mut self, changes: &ListChanges) -> bool {
        let mut changed = false;
        for wording in &changes.never_removed {
            changed |= self.remove_never(wording);
        }
        for entry in &changes.always_removed {
            // Only the entry this tab added: a category since changed by
            // another member stays.
            if self.always_for(&entry.wording) == Some(entry) {
                changed |= self.remove_always(&entry.wording);
            }
        }
        for wording in &changes.never_added {
            changed |= self.add_never(wording);
        }
        for entry in &changes.always_added {
            changed |= self.add_always(&entry.wording, entry.category());
        }
        changed
    }
}

/// How one document's contribution to its case's lists changed since it
/// was last saved into the case.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct ListChanges {
    pub never_added: Vec<String>,
    pub never_removed: Vec<String>,
    pub always_added: Vec<AlwaysEntry>,
    pub always_removed: Vec<AlwaysEntry>,
}

impl ListChanges {
    /// From `before` to `after`, comparing wordings by [`key`].
    pub fn between(before: &Lists, after: &Lists) -> Self {
        let never = |from: &Lists, to: &Lists| -> Vec<String> {
            from.never
                .iter()
                .filter(|w| !to.never_has(w))
                .cloned()
                .collect()
        };
        let always = |from: &Lists, to: &Lists| -> Vec<AlwaysEntry> {
            from.always
                .iter()
                .filter(|e| to.always_for(&e.wording) != Some(*e))
                .cloned()
                .collect()
        };
        Self {
            never_added: never(after, before),
            never_removed: never(before, after),
            always_added: always(after, before),
            always_removed: always(before, after),
        }
    }
    /// Keep the wordings `others` (other open members' contributions) still
    /// list: one member's undo leaves another's Keep or mark in place.
    pub fn sparing<'a>(mut self, others: impl IntoIterator<Item = &'a Lists>) -> Self {
        for other in others {
            self.never_removed.retain(|w| !other.never_has(w));
            self.always_removed
                .retain(|e| other.always_for(&e.wording).is_none());
        }
        self
    }
    pub fn is_empty(&self) -> bool {
        self.never_added.is_empty()
            && self.never_removed.is_empty()
            && self.always_added.is_empty()
            && self.always_removed.is_empty()
    }
}

#[cfg(test)]
mod tests {
    use super::{ListChanges, Lists, key};
    use mdoc_pii::Category;

    #[test]
    fn wordings_compare_ignoring_case_and_whitespace() {
        assert_eq!(key("  Acme \n  LTD "), "acme ltd");
        assert_eq!(key("Марина  Павлова"), "марина павлова");
        let mut lists = Lists::default();
        assert!(lists.add_never("Acme Ltd"));
        assert!(!lists.add_never("acme   ltd"));
        assert!(lists.never_has("ACME\tLtd"));
        assert!(!lists.add_never("   "));
    }

    #[test]
    fn a_wording_is_on_one_list_the_later_addition_wins() {
        let mut lists = Lists::default();
        lists.add_never("Maya Bennett");
        assert!(lists.add_always("maya bennett", Category::Person));
        assert!(!lists.never_has("Maya Bennett"));
        assert_eq!(lists.always.len(), 1);
        assert!(!lists.add_always("Maya Bennett", Category::Person));
        assert!(lists.add_always("Maya Bennett", Category::Organization));
        assert_eq!(lists.always.len(), 1);
        assert!(lists.add_never("MAYA BENNETT"));
        assert!(lists.always.is_empty());
    }

    #[test]
    fn a_case_over_the_user_wide_lists() {
        let mut user = Lists::default();
        user.add_never("Supreme Court");
        user.add_always("Acme", Category::Organization);
        user.add_always("Riverside", Category::Address);
        let mut case = Lists::default();
        case.add_always("supreme court", Category::Organization);
        case.add_never("ACME");
        let lists = Lists::effective(Some(&case), &user);
        assert!(!lists.never_has("Supreme Court"));
        assert!(lists.never_has("Acme"));
        assert!(lists.always_for("Acme").is_none());
        assert!(lists.always_for("Supreme Court").is_some());
        assert!(lists.always_for("Riverside").is_some());
        // Outside a case: the user-wide lists alone.
        assert_eq!(Lists::effective(None, &user), user);
    }

    #[test]
    fn changes_between_contributions_apply_to_a_case() {
        let mut before = Lists::default();
        before.add_never("Acme");
        before.add_always("Maya Bennett", Category::Person);
        let mut after = Lists::default();
        after.add_never("Riverside");
        after.add_always("Maya Bennett", Category::Person);
        let changes = ListChanges::between(&before, &after);
        assert_eq!(changes.never_added, ["Riverside"]);
        assert_eq!(changes.never_removed, ["Acme"]);
        assert!(changes.always_added.is_empty() && changes.always_removed.is_empty());
        let mut case = before.clone();
        case.add_never("Other member's wording");
        assert!(case.apply(&changes));
        assert_eq!(case.never, ["Other member's wording", "Riverside"]);
        assert!(!case.apply(&ListChanges::default()));
        assert!(ListChanges::between(&after, &after).is_empty());
        // Another member still keeps Acme: only its own removal is spared.
        let mut other = Lists::default();
        other.add_never("ACME");
        let spared = ListChanges::between(&before, &after).sparing([&other]);
        assert!(spared.never_removed.is_empty());
        assert_eq!(spared.never_added, ["Riverside"]);
    }
}
