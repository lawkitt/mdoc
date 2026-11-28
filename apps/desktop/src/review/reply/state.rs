//! The AI reply box's own state: the reply as pasted, its restoration
//! against the tab's mapping, and what each edit asks the tab to do.
use crate::review::text;
use mdoc_pii::restore::{ExportedIdentity, RestoreMap, RestoredText};
use std::ops::Range;

/// What the document view carries out for a reply-box transition.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum ReplyEffect {
    /// The reply changed: its tokens name today's identities.
    Store,
    /// The box holds a changed reply: show the Reply view.
    ShowReply,
    /// The box was cleared: leave the Reply view.
    LeaveReply,
}

/// The tab's AI reply as pasted, and its restoration against the tab's
/// current mapping.
#[derive(Default)]
pub(crate) struct ReplyBox {
    text: String,
    revision: u64,
    /// The reply and review revisions `restored` was made from.
    made_from: Option<(u64, u64)>,
    restored: RestoredText,
    /// The unresolved token Next unrecognised moves to after the last.
    next: usize,
    /// Whether the mapping of the last restoration had identities to restore.
    restorable: bool,
    /// The mapping the reply's alias tokens were last read against, by
    /// identity id, and its review revision: when an identity's alias
    /// changes, the reply's tokens for it follow.
    bound: Vec<(u64, ExportedIdentity)>,
    bound_at: Option<u64>,
    /// The revision of the box's text last saved, and the AI Reply it
    /// saved: none once that reply was deleted, so the text is not saved
    /// again until it changes.
    saved: Option<(Option<String>, u64)>,
}

impl ReplyBox {
    /// The reply as pasted, aliases and all.
    pub(crate) fn text(&self) -> &str {
        &self.text
    }

    /// Whether there is a reply: the Reply segment shows only then.
    pub(crate) fn exists(&self) -> bool {
        !self.text.trim().is_empty()
    }

    /// Bumped on every change of the reply text.
    pub(crate) fn revision(&self) -> u64 {
        self.revision
    }

    /// The user typed, pasted or cleared the box. Any change that leaves a
    /// reply shows the Reply view unless it is already showing, a paste over
    /// an earlier reply included; clearing leaves it.
    pub(crate) fn edited(&mut self, text: &str, showing_reply: bool) -> Vec<ReplyEffect> {
        if text == self.text {
            return Vec::new();
        }
        let had = self.exists();
        self.set(text.to_owned());
        if !self.exists() {
            // A cleared box keeps the reply it saved and starts a new one.
            self.saved = None;
        }
        let mut effects = vec![ReplyEffect::Store];
        if self.exists() {
            if !showing_reply {
                effects.push(ReplyEffect::ShowReply);
            }
        } else if had {
            effects.push(ReplyEffect::LeaveReply);
        }
        effects
    }

    /// A move gave the document's names other aliases: the reply's tokens
    /// follow, as they follow a rename.
    pub(crate) fn realias(&mut self, names: Vec<ExportedIdentity>, aliases: &[Option<String>]) {
        if !self.exists() || aliases.iter().all(Option::is_none) {
            return;
        }
        if let Some(text) = RestoreMap::new(names).realias(&self.text, aliases) {
            self.set_followed(text);
        }
    }

    fn set(&mut self, text: String) {
        self.text = text;
        self.revision += 1;
        self.next = 0;
    }

    /// Rewrite the text's aliases, naming the same people: a reply saved as
    /// it was stays saved, since it restores by identity.
    fn set_followed(&mut self, text: String) {
        let current = self
            .saved
            .as_ref()
            .is_some_and(|(_, at)| *at == self.revision);
        self.set(text);
        if current && let Some((_, at)) = self.saved.as_mut() {
            *at = self.revision;
        }
    }

    /// The saved reply this box holds.
    pub(crate) fn saved_id(&self) -> Option<String> {
        self.saved.as_ref().and_then(|(id, _)| id.clone())
    }

    /// The text and revision to save, when the box holds a reply not
    /// saved as it is now.
    pub(crate) fn unsaved(&self) -> Option<(String, u64)> {
        let current = self
            .saved
            .as_ref()
            .is_some_and(|(_, at)| *at == self.revision);
        (self.exists() && !current).then(|| (self.text.clone(), self.revision))
    }

    /// Reply `id` saved this box's text at `revision`.
    pub(crate) fn mark_saved(&mut self, id: &str, revision: u64) {
        if self.exists() {
            self.saved = Some((Some(id.to_string()), revision));
        }
    }

    /// Reply `id` was deleted: the text as it is stays unsaved, and a
    /// change makes a new reply.
    pub(crate) fn forget_saved(&mut self, id: &str) {
        if let Some((saved, _)) = self.saved.as_mut()
            && saved.as_deref() == Some(id)
        {
            *saved = None;
        }
    }

    /// Read the reply's alias tokens against the mapping at `review` (its
    /// revision) from now on, as when it is pasted or loaded.
    pub(crate) fn bind(&mut self, review: u64, mapping: Vec<(u64, ExportedIdentity)>) {
        self.bound = mapping;
        self.bound_at = Some(review);
    }

    /// The mapping moved on to `review`: rewrite the reply's alias tokens
    /// whose identity now has another alias, so the reply keeps naming the
    /// same people whatever renumbered them (a join, a Project-wide rename, an
    /// undo of either). The reply stays in alias form; nothing restored is
    /// kept. Returns whether the reply changed.
    pub(crate) fn follow(
        &mut self,
        review: u64,
        mapping: impl FnOnce() -> Vec<(u64, ExportedIdentity)>,
    ) -> bool {
        if !self.exists() {
            self.bound.clear();
            self.bound_at = None;
            return false;
        }
        if self.bound_at == Some(review) {
            return false;
        }
        let before = std::mem::take(&mut self.bound);
        self.bind(review, mapping());
        let aliases: Vec<_> = before
            .iter()
            .map(|(id, old)| {
                self.bound
                    .iter()
                    .find(|(now, _)| now == id)
                    .map(|(_, now)| &now.alias)
                    .filter(|alias| **alias != old.alias)
                    .cloned()
            })
            .collect();
        if aliases.iter().all(Option::is_none) {
            return false;
        }
        let map = RestoreMap::new(before.into_iter().map(|(_, e)| e).collect());
        match map.realias(&self.text, &aliases) {
            Some(text) => {
                self.set_followed(text);
                true
            }
            None => false,
        }
    }

    /// Restore against the mapping at `review` (its revision), unless the
    /// reply and mapping are what the last restoration used. Returns whether
    /// the restoration changed.
    pub(crate) fn refresh(&mut self, review: u64, map: impl FnOnce() -> RestoreMap) -> bool {
        let key = Some((self.revision, review));
        if self.made_from == key {
            return false;
        }
        self.made_from = key;
        let restored = if self.exists() {
            let map = map();
            self.restorable = !map.is_empty();
            map.restore_in_place(&self.text)
        } else {
            self.restorable = false;
            RestoredText::default()
        };
        if restored == self.restored {
            return false;
        }
        self.restored = restored;
        self.next = self.next.min(self.restored.unresolved.len());
        true
    }

    /// Whether Copy reply and Open as tab have anything to restore: a reply,
    /// and a mapping with identities (refresh first).
    pub(crate) fn restorable(&self) -> bool {
        self.exists() && self.restorable
    }

    /// The last restoration (refresh first).
    pub(crate) fn restored(&self) -> &RestoredText {
        &self.restored
    }

    /// "N values restored · M unrecognized": distinct identities restored
    /// and distinct unknown tokens, as Anoni counts. Next
    /// unrecognised still visits every occurrence.
    pub(crate) fn summary(&self) -> String {
        let restored = self.restored.values;
        let unresolved = self.unrecognised();
        text::reply_summary(restored, unresolved)
    }

    /// Distinct unresolved tokens in the restored text.
    pub(crate) fn unrecognised(&self) -> usize {
        let text = &self.restored.text;
        let tokens: std::collections::HashSet<_> = self
            .restored
            .unresolved
            .iter()
            .filter_map(|range| text.get(range.clone()))
            .collect();
        tokens.len()
    }

    /// The unresolved token Next unrecognised last moved to.
    #[cfg(test)]
    pub(crate) fn current_unresolved(&self) -> Option<Range<usize>> {
        let at = self.next.checked_sub(1)?;
        self.restored.unresolved.get(at).cloned()
    }

    /// The next unresolved token, wrapping after the last.
    pub(crate) fn next_unresolved(&mut self) -> Option<Range<usize>> {
        let tokens = &self.restored.unresolved;
        if tokens.is_empty() {
            return None;
        }
        let at = self.next % tokens.len();
        self.next = at + 1;
        Some(tokens[at].clone())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use mdoc_pii::{Category, restore::ExportedIdentity};

    fn map() -> RestoreMap {
        RestoreMap::new(vec![ExportedIdentity {
            alias: "PERSON_1".into(),
            previous: vec![],
            category: Category::Person,
            custom: false,
            originals: vec!["Anna Petrova".into()],
        }])
    }

    #[test]
    fn filling_shows_the_reply_and_clearing_leaves_it() {
        let mut reply = ReplyBox::default();
        assert!(!reply.exists());
        assert_eq!(
            reply.edited("PERSON_1", false),
            [ReplyEffect::Store, ReplyEffect::ShowReply]
        );
        // Typing on while the Reply view shows only stores.
        assert_eq!(reply.edited("PERSON_1 left", true), [ReplyEffect::Store]);
        assert_eq!(reply.edited("PERSON_1 left", false), []);
        // A different reply pasted over this one while on Edit shows it.
        assert_eq!(
            reply.edited("PERSON_1 came back", false),
            [ReplyEffect::Store, ReplyEffect::ShowReply]
        );
        assert_eq!(reply.text(), "PERSON_1 came back");
        assert_eq!(
            reply.edited("  ", true),
            [ReplyEffect::Store, ReplyEffect::LeaveReply]
        );
        assert!(!reply.exists(), "a blank box keeps nothing");
        assert_eq!(reply.edited("", false), [ReplyEffect::Store]);
    }

    #[test]
    fn the_reply_follows_its_identities_through_a_swap_and_back() {
        let person = |alias: &str, name: &str| ExportedIdentity {
            alias: alias.into(),
            previous: vec![],
            category: Category::Person,
            custom: false,
            originals: vec![name.into()],
        };
        let mapping =
            |anna: &str, boris: &str| vec![(1, person(anna, "Anna")), (2, person(boris, "Boris"))];
        let mut reply = ReplyBox::default();
        reply.edited("PERSON_1 owes PERSON_2.", false);
        reply.bind(1, mapping("PERSON_1", "PERSON_2"));
        assert!(!reply.follow(1, || unreachable!("same revision")));
        assert!(!reply.follow(2, || mapping("PERSON_1", "PERSON_2")));
        assert!(reply.follow(3, || mapping("PERSON_2", "PERSON_1")));
        assert_eq!(reply.text(), "PERSON_2 owes PERSON_1.");
        assert!(reply.follow(4, || mapping("PERSON_1", "PERSON_2")));
        assert_eq!(reply.text(), "PERSON_1 owes PERSON_2.");
        // A forgotten identity's tokens stay as they are.
        assert!(!reply.follow(5, || vec![(2, person("PERSON_2", "Boris"))]));
        assert_eq!(reply.text(), "PERSON_1 owes PERSON_2.");
    }

    #[test]
    fn the_summary_counts_distinct_values_and_next_unrecognised_wraps() {
        let mut reply = ReplyBox::default();
        reply.edited(
            "PERSON_1 and PERSON_1 met PERSON_8, PERSON_9 and PERSON_8.",
            false,
        );
        assert!(reply.refresh(1, map));
        assert!(!reply.refresh(1, || unreachable!("cached")));
        // One identity twice is one value; PERSON_8 twice is one unknown.
        assert_eq!(reply.summary(), "1 value restored · 2 unrecognized");
        let text = reply.restored().text.clone();
        let mut seen = Vec::new();
        for _ in 0..4 {
            let range = reply.next_unresolved().unwrap();
            seen.push(text[range].to_string());
        }
        // Next unrecognised visits every occurrence, then wraps.
        assert_eq!(seen, ["PERSON_8", "PERSON_9", "PERSON_8", "PERSON_8"]);
        reply.edited("PERSON_1", false);
        reply.refresh(1, map);
        assert_eq!(reply.summary(), "1 value restored · 0 unrecognized");
        assert_eq!(reply.next_unresolved(), None);
    }
}
