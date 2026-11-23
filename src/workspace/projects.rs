//! The window's Projects: every Project record in the Vault, read when it
//! opens and kept here while the window edits them. A change marks the
//! Project for the window's one writer, which saves the record as it is
//! when its write starts; moves and deletes run through the same writer and
//! hand back the records they wrote. GPUI-free.

use mdoc_vault::{Colour, Project, ProjectSummary};

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
pub(super) struct Projects {
    projects: Vec<Project>,
    loaded: bool,
}

impl Projects {
    /// Every Project was read from the Vault.
    pub(super) fn loaded(&self) -> bool {
        self.loaded
    }

    pub(super) fn finish_load(&mut self, projects: Vec<Project>) {
        self.projects = projects;
        self.loaded = true;
    }

    /// The Vault is gone for this run: no Projects.
    pub(super) fn clear(&mut self) {
        self.projects.clear();
        self.loaded = false;
    }

    pub(super) fn project(&self, id: &str) -> Option<&Project> {
        self.projects.iter().find(|project| project.id == id)
    }

    /// No Project is held.
    pub(super) fn is_empty(&self) -> bool {
        self.projects.is_empty()
    }

    pub(super) fn link(&self, id: &str) -> Option<ProjectLink> {
        self.project(id).map(ProjectLink::of)
    }

    /// Pinned first, then the most recently updated, then by name.
    pub(super) fn summaries(&self) -> Vec<ProjectSummary> {
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
    pub(super) fn edit<R>(
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
    pub(super) fn put(&mut self, project: Project) {
        match self.projects.iter_mut().find(|p| p.id == project.id) {
            Some(kept) => *kept = project,
            None => self.projects.push(project),
        }
    }

    /// A move wrote `project`: take its mapping, keeping what this window
    /// changed in its other fields since it last wrote them.
    pub(super) fn moved(&mut self, project: Project) {
        match self.projects.iter_mut().find(|p| p.id == project.id) {
            Some(kept) => {
                kept.mapping = project.mapping;
                kept.revision = project.revision;
            }
            None => self.projects.push(project),
        }
    }

    pub(super) fn remove(&mut self, id: &str) -> bool {
        let before = self.projects.len();
        self.projects.retain(|project| project.id != id);
        self.projects.len() != before
    }
}

fn now() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_or(0, |since| since.as_secs())
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
