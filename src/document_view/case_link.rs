//! A tab's link to its saved case (ADR 0041): which case the tab belongs
//! to, the case's identities for the panel's "in case" section, the
//! categories the case skips in scans, its Always and Never lists and what
//! this tab put on them, and whether a mapping change waits to be saved
//! into the case. The window's `CaseCoordinator` owns the cases
//! themselves; the document view runs the save timer.

use mdoc_pii::{Category, restore::ExportedIdentity};
use mdoc_store::{
    cases,
    lists::{ListChanges, Lists},
};
use std::collections::HashSet;

/// What the document view carries out for a case-link transition.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum CaseLinkEffect {
    /// The tab's case looks different now.
    Repaint,
    /// Save the mapping into the case once changes settle, replacing a
    /// save already waiting.
    SaveSoon,
    /// A waiting save no longer applies: the tab left its case.
    CancelSave,
}

#[derive(Default)]
pub(crate) struct CaseLink {
    link: Option<cases::CaseLink>,
    /// The case's identities, for the panel's "in case" section.
    names: Vec<ExportedIdentity>,
    /// Categories this tab's case does not detect.
    skip: HashSet<Category>,
    /// The case's Always and Never lists, for scans (ADR 0092).
    lists: Lists,
    /// What this tab's Keeps and manual marks have put on the case's
    /// lists, as last saved into the case.
    listed: Lists,
}

impl CaseLink {
    /// The case this tab belongs to.
    pub(crate) fn link(&self) -> Option<&cases::CaseLink> {
        self.link.as_ref()
    }

    /// Whether this tab belongs to case `id`.
    pub(crate) fn is(&self, id: &str) -> bool {
        self.link.as_ref().is_some_and(|link| link.id == id)
    }

    pub(crate) fn names(&self) -> &[ExportedIdentity] {
        &self.names
    }

    pub(crate) fn skip(&self) -> &HashSet<Category> {
        &self.skip
    }

    pub(crate) fn lists(&self) -> &Lists {
        &self.lists
    }

    /// The case's lists, as the tab joins or as they change.
    pub(crate) fn set_lists(&mut self, lists: Lists) {
        self.lists = lists;
    }

    /// Case `id`'s lists changed; only its members follow.
    pub(crate) fn case_lists_changed(&mut self, id: &str, lists: &Lists) {
        if self.is(id) {
            self.lists = lists.clone();
        }
    }

    /// This tab's contribution to its case's lists is now `listed`: what
    /// changed since the last save into the case. Nothing outside a case,
    /// where the user-wide lists are edited by hand only.
    pub(crate) fn contribute(&mut self, listed: Lists) -> Option<ListChanges> {
        self.link.as_ref()?;
        let changes = ListChanges::between(&self.listed, &listed);
        self.listed = listed;
        (!changes.is_empty()).then_some(changes)
    }

    /// The tab's review started over (a reload): what it put on the lists
    /// stays there, no longer followed.
    pub(crate) fn forget_listed(&mut self) {
        self.listed = Lists::default();
    }

    /// The tab joined `link` (or a reply restored it; none: no case).
    pub(crate) fn attach(&mut self, link: Option<cases::CaseLink>) {
        self.link = link;
    }

    /// The case's identities changed, or the tab joined it.
    pub(crate) fn set_names(&mut self, names: Vec<ExportedIdentity>) {
        self.names = names;
    }

    /// The categories the case skips, as it joins.
    pub(crate) fn set_skip(&mut self, skip: HashSet<Category>) {
        self.skip = skip;
    }

    /// Case `id`'s skipped categories changed; only its members follow.
    pub(crate) fn case_skip_changed(
        &mut self,
        id: &str,
        skip: &HashSet<Category>,
    ) -> Option<CaseLinkEffect> {
        if !self.is(id) {
            return None;
        }
        self.skip = skip.clone();
        Some(CaseLinkEffect::Repaint)
    }

    /// Case `id` was renamed; only its members follow.
    pub(crate) fn case_renamed(&mut self, id: &str, name: &str) -> Option<CaseLinkEffect> {
        let link = self.link.as_mut().filter(|link| link.id == id)?;
        link.name = name.to_string();
        Some(CaseLinkEffect::Repaint)
    }

    /// The mapping changed: save it into the case once changes settle.
    pub(crate) fn mapping_changed(&self) -> Option<CaseLinkEffect> {
        self.link.is_some().then_some(CaseLinkEffect::SaveSoon)
    }

    /// The tab leaves its case, keeping its live mapping (ADR 0041 #25).
    /// Returns the case it left.
    pub(crate) fn leave(&mut self) -> (Option<cases::CaseLink>, CaseLinkEffect) {
        self.skip.clear();
        self.lists = Lists::default();
        self.listed = Lists::default();
        (self.link.take(), CaseLinkEffect::CancelSave)
    }

    /// Case `id` was deleted: a member forgets it.
    pub(crate) fn case_deleted(&mut self, id: &str) -> Option<CaseLinkEffect> {
        if !self.is(id) {
            return None;
        }
        self.link = None;
        self.skip.clear();
        self.lists = Lists::default();
        self.listed = Lists::default();
        Some(CaseLinkEffect::CancelSave)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn link(id: &str) -> cases::CaseLink {
        cases::CaseLink {
            id: id.into(),
            color: None,
            name: "Acme".into(),
        }
    }

    #[test]
    fn a_member_saves_mapping_changes_and_follows_its_case() {
        let mut case = CaseLink::default();
        assert_eq!(case.mapping_changed(), None, "no case, nothing to save");
        case.attach(Some(link("a")));
        assert!(case.is("a") && !case.is("b"));
        assert_eq!(case.mapping_changed(), Some(CaseLinkEffect::SaveSoon));
        assert_eq!(case.case_renamed("b", "Other"), None);
        assert_eq!(
            case.case_renamed("a", "Acme Ltd"),
            Some(CaseLinkEffect::Repaint)
        );
        assert_eq!(case.link().unwrap().name, "Acme Ltd");
        let skip = HashSet::from([Category::Person]);
        assert_eq!(case.case_skip_changed("b", &skip), None);
        assert!(case.skip().is_empty());
        assert_eq!(
            case.case_skip_changed("a", &skip),
            Some(CaseLinkEffect::Repaint)
        );
        assert_eq!(case.skip(), &skip);
    }

    #[test]
    fn leaving_or_deleting_the_case_cancels_a_waiting_save() {
        let mut case = CaseLink::default();
        case.attach(Some(link("a")));
        case.set_skip(HashSet::from([Category::Person]));
        let (left, effect) = case.leave();
        assert_eq!(left.map(|l| l.id), Some("a".to_string()));
        assert_eq!(effect, CaseLinkEffect::CancelSave);
        assert!(case.link().is_none() && case.skip().is_empty());
        case.attach(Some(link("a")));
        case.set_skip(HashSet::from([Category::Person]));
        assert_eq!(case.case_deleted("b"), None);
        assert_eq!(case.case_deleted("a"), Some(CaseLinkEffect::CancelSave));
        assert!(case.link().is_none() && case.skip().is_empty());
    }

    #[test]
    fn a_member_contributes_what_changed_and_forgets_it_on_leaving() {
        let mut case = CaseLink::default();
        let mut listed = Lists::default();
        listed.add_never("Acme");
        assert_eq!(case.contribute(listed.clone()), None, "no case, no lists");
        case.attach(Some(link("a")));
        let changes = case.contribute(listed.clone()).unwrap();
        assert_eq!(changes.never_added, ["Acme"]);
        assert_eq!(case.contribute(listed.clone()), None, "nothing new");
        let changes = case.contribute(Lists::default()).unwrap();
        assert_eq!(changes.never_removed, ["Acme"]);
        // A reload forgets the contribution without taking it back.
        case.contribute(listed.clone());
        case.forget_listed();
        assert_eq!(case.contribute(Lists::default()), None);
        let mut lists = Lists::default();
        lists.add_always("Maya Bennett", Category::Person);
        case.case_lists_changed("b", &lists);
        assert!(case.lists().is_empty());
        case.case_lists_changed("a", &lists);
        assert_eq!(case.lists(), &lists);
        case.leave();
        assert!(case.lists().is_empty());
    }
}
