//! Projects' state: every Project record in the Vault, read when it opens
//! and kept here while the window edits them, and the pure work on their
//! names. GPUI-free.

use mdoc_pii::restore::{ExportedIdentity, RestoreMap};
use mdoc_vault::{Colour, Mapping, Project, ProjectSummary};
use std::collections::HashSet;

/// What a tab, a reply and the shelf know of a Project.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct ProjectLink {
    pub(crate) id: String,
    pub(crate) name: String,
    pub(crate) colour: Option<Colour>,
}

impl ProjectLink {
    pub(crate) fn of(project: &Project) -> Self {
        Self {
            id: project.id.clone(),
            name: project.name.clone(),
            colour: project.colour,
        }
    }

    /// Its colour's hue: the one picked, else the one its id gives.
    pub(crate) fn hue(&self) -> u32 {
        self.colour
            .unwrap_or_else(|| Colour::for_id(&self.id))
            .hue()
    }
}

#[derive(Default)]
pub(in crate::workspace) struct Projects {
    projects: Vec<Project>,
    loaded: bool,
    /// How many there are, for the documents' Share tab (#1312).
    count: std::rc::Rc<std::cell::Cell<usize>>,
    /// Documents a move or a delete is rewriting: their autosaves wait
    /// until they load again.
    pub(in crate::workspace) moving: HashSet<mdoc_store::document_id::VaultId>,
    /// Of those, the ones whose tab loads them again now, with that load's
    /// generation: an older load ending is not the reload.
    pub(in crate::workspace) reloading:
        std::collections::HashMap<mdoc_store::document_id::VaultId, u64>,
    /// Tests only: the documents each delete question asked about, in the
    /// order the questions show.
    #[cfg(test)]
    pub(in crate::workspace) asked_to_delete:
        std::collections::VecDeque<Vec<mdoc_store::document_id::VaultId>>,
    /// Tests only: what each export read, as stored then.
    #[cfg(test)]
    pub(in crate::workspace) exports_seen: Vec<super::export_project::ExportSeen>,
}

impl Projects {
    /// Every Project was read from the Vault.
    pub(in crate::workspace) fn loaded(&self) -> bool {
        self.loaded
    }

    pub(in crate::workspace) fn finish_load(&mut self, projects: Vec<Project>) {
        self.projects = projects;
        self.loaded = true;
        self.counted();
    }

    /// How many Projects there are, kept current as they change.
    pub(in crate::workspace) fn count(&self) -> std::rc::Rc<std::cell::Cell<usize>> {
        self.count.clone()
    }

    pub(super) fn counted(&self) {
        self.count.set(self.projects.len());
    }

    /// The Vault is gone for this run: no Projects.
    pub(in crate::workspace) fn clear(&mut self) {
        self.projects.clear();
        self.loaded = false;
        self.counted();
    }

    pub(in crate::workspace) fn project(&self, id: &str) -> Option<&Project> {
        self.projects.iter().find(|project| project.id == id)
    }

    /// No Project is held.
    pub(in crate::workspace) fn is_empty(&self) -> bool {
        self.projects.is_empty()
    }

    pub(in crate::workspace) fn link(&self, id: &str) -> Option<ProjectLink> {
        self.project(id).map(ProjectLink::of)
    }

    /// Pinned first, then the most recently updated, then by name.
    pub(in crate::workspace) fn summaries(&self) -> Vec<ProjectSummary> {
        let mut summaries: Vec<_> = self.projects.iter().map(ProjectSummary::of).collect();
        summaries.sort_by(|a, b| {
            (b.pinned.cmp(&a.pinned))
                .then(b.updated.cmp(&a.updated))
                .then_with(|| a.name.to_lowercase().cmp(&b.name.to_lowercase()))
        });
        summaries
    }

    /// Change Project `id`; `None` from `change` means nothing changed.
    /// The caller queues its write.
    pub(in crate::workspace) fn edit<R>(
        &mut self,
        id: &str,
        change: impl FnOnce(&mut Project) -> Option<R>,
    ) -> Option<R> {
        let project = self.projects.iter_mut().find(|project| project.id == id)?;
        let result = change(project)?;
        project.updated = now();
        Some(result)
    }

    /// Keep `project` as the Vault stored it: new, or written by a move.
    pub(in crate::workspace) fn put(&mut self, project: Project) {
        match self.projects.iter_mut().find(|p| p.id == project.id) {
            Some(kept) => *kept = project,
            None => self.projects.push(project),
        }
        self.counted();
    }

    /// A move wrote `project`: take its mapping, keeping what this window
    /// changed in its other fields since it last wrote them.
    pub(in crate::workspace) fn moved(&mut self, project: Project) {
        match self.projects.iter_mut().find(|p| p.id == project.id) {
            Some(kept) => {
                kept.mapping = project.mapping;
                kept.revision = project.revision;
            }
            None => self.projects.push(project),
        }
        self.counted();
    }

    pub(in crate::workspace) fn remove(&mut self, id: &str) -> bool {
        let before = self.projects.len();
        self.projects.retain(|project| project.id != id);
        self.counted();
        self.projects.len() != before
    }
}

pub(super) fn now() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_or(0, |since| since.as_secs())
}

/// The renames a member that closed before them has not followed: each
/// earlier alias of an identity, as (old alias, identity now).
pub(super) fn renames_of(mapping: &Mapping) -> Vec<(String, ExportedIdentity)> {
    (mapping.identities.iter())
        .filter_map(|identity| Some((identity, identity.exported()?)))
        .flat_map(|(identity, exported)| {
            (identity.previous.iter()).map(move |old| (old.clone(), exported.clone()))
        })
        .collect()
}

/// Fold a document's names into `mapping`, the Project's or its own while
/// Unfiled: its identities, what an undo of its Not a name gave back, and
/// its Not a name wordings, which stay where `in_use` finds their alias
/// restored in another document.
pub(in crate::workspace) fn fold(
    mapping: &mut Mapping,
    identities: &[ExportedIdentity],
    kept: &[(String, String)],
    not_names: &[(String, String)],
    document: &str,
    in_use: impl Fn(&str) -> bool,
) {
    mapping.merge_from(identities, Some(document));
    mapping.refill(kept, document);
    mapping.drop_wordings(not_names, document, |identity| in_use(&identity.alias));
}

/// Copy what this window keeps of a Project over `stored`: everything a
/// move doesn't own.
pub(in crate::workspace) fn keep_fields(stored: &mut Project, project: &Project) {
    stored.name.clone_from(&project.name);
    stored.colour = project.colour;
    stored.pinned = project.pinned;
    stored.mapping.clone_from(&project.mapping);
    stored.lists.clone_from(&project.lists);
    stored.skip.clone_from(&project.skip);
    stored.dismissed_hints.clone_from(&project.dismissed_hints);
}

/// How many of `names` that `text` uses took another alias.
pub(super) fn count_renumbered(
    text: &str,
    names: &[ExportedIdentity],
    aliases: &[Option<String>],
) -> usize {
    let map = RestoreMap::new(names.to_vec());
    let mut seen = HashSet::new();
    (map.tokens(text).into_iter())
        .filter_map(|token| token.identity)
        .filter(|index| seen.insert(*index))
        .filter(|index| aliases[*index].is_some())
        .count()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn project(id: char, name: &str, pinned: bool, updated: u64) -> Project {
        let vault = tempfile::tempdir().unwrap();
        let vault = mdoc_vault::Vault::open(vault.path(), &[1; 32]).unwrap();
        let mut project = vault
            .create_project(mdoc_vault::NewProject {
                name: name.into(),
                colour: None,
                pinned,
            })
            .unwrap();
        project.id = id.to_string().repeat(32);
        project.updated = updated;
        project
    }

    #[test]
    fn projects_list_pinned_then_recent_and_edits_stamp_them() {
        let mut projects = Projects::default();
        assert!(!projects.loaded());
        projects.finish_load(vec![
            project('a', "Older", false, 10),
            project('b', "Pinned", true, 1),
            project('c', "Newer", false, 20),
        ]);
        assert!(projects.loaded());
        let names: Vec<_> = projects.summaries().into_iter().map(|s| s.name).collect();
        assert_eq!(names, ["Pinned", "Newer", "Older"]);
        let id = "a".repeat(32);
        assert_eq!(projects.edit(&id, |_| None::<()>), None);
        assert_eq!(
            projects.project(&id).unwrap().updated,
            10,
            "nothing changed"
        );
        projects.edit(&id, |p| {
            p.name = "Renamed".into();
            Some(())
        });
        assert!(projects.project(&id).unwrap().updated > 20);
        assert_eq!(projects.link(&id).unwrap().name, "Renamed");
    }

    #[test]
    fn a_move_brings_its_mapping_and_keeps_the_windows_other_changes() {
        let mut projects = Projects::default();
        let mut kept = project('a', "Mine", false, 1);
        kept.skip = vec!["PERSON".into()];
        projects.finish_load(vec![kept.clone()]);
        let mut written = kept.clone();
        written.name = "Stale".into();
        written.skip.clear();
        written.revision = 7;
        written.mapping.not_names.push(mdoc_vault::NotName {
            document: "d".repeat(32),
            alias: "PERSON_1".into(),
            wording: "Anna".into(),
        });
        projects.moved(written.clone());
        let now = projects.project(&kept.id).unwrap();
        assert_eq!(now.name, "Mine");
        assert_eq!(now.skip, ["PERSON"]);
        assert_eq!(now.mapping, written.mapping);
        assert_eq!(now.revision, 7);
        assert!(projects.remove(&kept.id));
        assert!(!projects.remove(&kept.id));
        projects.clear();
        assert!(!projects.loaded() && projects.summaries().is_empty());
    }
}
