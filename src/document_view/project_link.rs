//! A tab's link to its saved Project: which Project the tab belongs
//! to, the Project's identities for the panel's "in Project" section, the
//! categories the Project skips in scans, its Always and Never lists and what
//! this tab put on them, and whether a mapping change waits to be saved
//! into the Project. The window's `Projects` owns the Projects
//! themselves; the document view runs the save timer.

use crate::workspace::ProjectLink;
use mdoc_pii::{Category, restore::ExportedIdentity};
use mdoc_store::lists::{ListChanges, Lists};
use std::collections::HashSet;

/// What the document view carries out for a Project transition.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum ProjectEffect {
    /// The tab's Project looks different now.
    Repaint,
    /// Save the mapping into the Project once changes settle, replacing a
    /// save already waiting.
    SaveSoon,
    /// A waiting save no longer applies: the tab left its Project.
    CancelSave,
}

#[derive(Default)]
pub(crate) struct ProjectState {
    link: Option<ProjectLink>,
    /// The Project's identities, for the panel's "in Project" section.
    names: Vec<ExportedIdentity>,
    /// Categories this tab's Project does not detect.
    skip: HashSet<Category>,
    /// The Project's Always and Never lists, for scans.
    lists: Lists,
    /// What this tab's Keeps and manual marks have put on the Project's
    /// lists, as last saved into the Project.
    listed: Lists,
}

impl ProjectState {
    /// The Project this tab belongs to.
    pub(crate) fn link(&self) -> Option<&ProjectLink> {
        self.link.as_ref()
    }

    /// Whether this tab belongs to Project `id`.
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

    /// What this tab has put on its Project's lists.
    pub(crate) fn listed(&self) -> &Lists {
        &self.listed
    }

    /// The Project's lists, as the tab joins or as they change.
    pub(crate) fn set_lists(&mut self, lists: Lists) {
        self.lists = lists;
    }

    /// Project `id`'s lists changed; only its members follow.
    pub(crate) fn project_lists_changed(&mut self, id: &str, lists: &Lists) {
        if self.is(id) {
            self.lists = lists.clone();
        }
    }

    /// This tab's contribution to its Project's lists is now `listed`: what
    /// changed since the last save into the Project. Nothing outside a Project,
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

    /// The tab joined `link` (or a reply restored it; none: no Project).
    pub(crate) fn attach(&mut self, link: Option<ProjectLink>) {
        self.link = link;
    }

    /// The Project's identities changed, or the tab joined it.
    pub(crate) fn set_names(&mut self, names: Vec<ExportedIdentity>) {
        self.names = names;
    }

    /// The categories the Project skips, as it joins.
    pub(crate) fn set_skip(&mut self, skip: HashSet<Category>) {
        self.skip = skip;
    }

    /// Project `id`'s skipped categories changed; only its members follow.
    pub(crate) fn project_skip_changed(
        &mut self,
        id: &str,
        skip: &HashSet<Category>,
    ) -> Option<ProjectEffect> {
        if !self.is(id) {
            return None;
        }
        self.skip = skip.clone();
        Some(ProjectEffect::Repaint)
    }

    /// Project `id` was renamed; only its members follow.
    pub(crate) fn project_renamed(&mut self, id: &str, name: &str) -> Option<ProjectEffect> {
        let link = self.link.as_mut().filter(|link| link.id == id)?;
        link.name = name.to_string();
        Some(ProjectEffect::Repaint)
    }

    /// The mapping changed: save it into the Project once changes settle.
    pub(crate) fn mapping_changed(&self) -> Option<ProjectEffect> {
        self.link.is_some().then_some(ProjectEffect::SaveSoon)
    }

    /// The tab leaves its Project, keeping its live mapping.
    /// Returns the Project it left.
    pub(crate) fn leave(&mut self) -> (Option<ProjectLink>, ProjectEffect) {
        self.skip.clear();
        self.lists = Lists::default();
        self.listed = Lists::default();
        (self.link.take(), ProjectEffect::CancelSave)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn link(id: &str) -> ProjectLink {
        ProjectLink {
            id: id.into(),
            colour: None,
            name: "Acme".into(),
        }
    }

    #[test]
    fn a_member_saves_mapping_changes_and_follows_its_project() {
        let mut project = ProjectState::default();
        assert_eq!(
            project.mapping_changed(),
            None,
            "no Project, nothing to save"
        );
        project.attach(Some(link("a")));
        assert!(project.is("a") && !project.is("b"));
        assert_eq!(project.mapping_changed(), Some(ProjectEffect::SaveSoon));
        assert_eq!(project.project_renamed("b", "Other"), None);
        assert_eq!(
            project.project_renamed("a", "Acme Ltd"),
            Some(ProjectEffect::Repaint)
        );
        assert_eq!(project.link().unwrap().name, "Acme Ltd");
        let skip = HashSet::from([Category::Person]);
        assert_eq!(project.project_skip_changed("b", &skip), None);
        assert!(project.skip().is_empty());
        assert_eq!(
            project.project_skip_changed("a", &skip),
            Some(ProjectEffect::Repaint)
        );
        assert_eq!(project.skip(), &skip);
    }

    #[test]
    fn leaving_the_project_cancels_a_waiting_save() {
        let mut project = ProjectState::default();
        project.attach(Some(link("a")));
        project.set_skip(HashSet::from([Category::Person]));
        let (left, effect) = project.leave();
        assert_eq!(left.map(|l| l.id), Some("a".to_string()));
        assert_eq!(effect, ProjectEffect::CancelSave);
        assert!(project.link().is_none() && project.skip().is_empty());
    }

    #[test]
    fn a_member_contributes_what_changed_and_forgets_it_on_leaving() {
        let mut project = ProjectState::default();
        let mut listed = Lists::default();
        listed.add_never("Acme");
        assert_eq!(
            project.contribute(listed.clone()),
            None,
            "no Project, no lists"
        );
        project.attach(Some(link("a")));
        let changes = project.contribute(listed.clone()).unwrap();
        assert_eq!(changes.never_added, ["Acme"]);
        assert_eq!(project.contribute(listed.clone()), None, "nothing new");
        let changes = project.contribute(Lists::default()).unwrap();
        assert_eq!(changes.never_removed, ["Acme"]);
        // A reload forgets the contribution without taking it back.
        project.contribute(listed.clone());
        project.forget_listed();
        assert_eq!(project.contribute(Lists::default()), None);
        let mut lists = Lists::default();
        lists.add_always("Maya Bennett", Category::Person);
        project.project_lists_changed("b", &lists);
        assert!(project.lists().is_empty());
        project.project_lists_changed("a", &lists);
        assert_eq!(project.lists(), &lists);
        project.leave();
        assert!(project.lists().is_empty());
    }
}
