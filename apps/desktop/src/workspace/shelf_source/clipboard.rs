//! Whose names the shelf's Pseudonymize and Restore use: the sources its
//! picker offers (open tabs outside a Project, then every Project), the one
//! used last, and the names each holds, as the window has them.
use super::{Workspace, open_tab};
use crate::shelf::ShelfDocument;
use gpui::App;
use mdoc_pii::restore::{ExportedIdentity, RestoreMap};

/// Whose names the shelf's clipboard rows use: a Project, or an open tab
/// outside one through its live mapping. Never mixed.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) enum ClipboardSource {
    Project {
        id: String,
        name: String,
    },
    /// The tab's title, pseudonymized, without `.md`.
    Tab {
        id: u64,
        title: String,
    },
}

impl ClipboardSource {
    /// The name the row and its picker show.
    pub(crate) fn name(&self) -> &str {
        match self {
            Self::Project { name, .. } => name,
            Self::Tab { title, .. } => title,
        }
    }
}

/// The names a clipboard row may use.
pub(crate) enum ClipboardNames {
    Names(Vec<ExportedIdentity>),
    /// The Vault's key is not open: "Unlock in mdoc…".
    Locked,
    /// The Project or tab is gone.
    Gone,
}

impl Workspace {
    /// Every source the picker offers: open tabs outside a Project that
    /// have a mapping, then every Project.
    pub(crate) fn clipboard_sources(&self, cx: &App) -> Vec<ClipboardSource> {
        let mut sources: Vec<_> = self
            .tabs
            .iter()
            .filter_map(|tab| {
                let view = tab.view()?.read(cx);
                if view.project.link().is_some() || view.pii.review().applied().is_empty() {
                    return None;
                }
                // Only its title is shown: the Vault is not asked.
                let (names, title) = (view.known_identities(), view.display_name());
                let row = ShelfDocument::of_tab(open_tab(tab.id(), view, &title, &names, cx)?)?;
                Some(ClipboardSource::Tab {
                    id: tab.id(),
                    title: row.display_title().to_string(),
                })
            })
            .collect();
        let mut projects = self.projects.summaries();
        projects.sort_by_key(|project| project.name.to_lowercase());
        sources.extend(
            projects
                .into_iter()
                .map(|project| self.project_source(&project.id, &project.name)),
        );
        sources
    }

    /// A Project as a clipboard source, named as the shelf may show it:
    /// with its own names pseudonymized, as a Project is often named after
    /// a person.
    fn project_source(&self, id: &str, name: &str) -> ClipboardSource {
        let name = match self.projects.project(id) {
            Some(project) => {
                RestoreMap::new(project.mapping.exported())
                    .pseudonymize(name)
                    .0
            }
            None => name.to_string(),
        };
        ClipboardSource::Project {
            id: id.to_string(),
            name,
        }
    }

    /// The document used last: the active tab's Project, or its own
    /// mapping.
    pub(crate) fn active_clipboard_source(&self, cx: &App) -> Option<ClipboardSource> {
        let view = self.active_view()?;
        let view = view.read(cx);
        if let Some(link) = view.project.link() {
            return Some(self.project_source(&link.id, &link.name));
        }
        let id = self.tabs.active();
        self.clipboard_sources(cx)
            .into_iter()
            .find(|source| matches!(source, ClipboardSource::Tab { id: tab, .. } if *tab == id))
    }

    /// The names `source` restores or pseudonymizes with. Never reads the
    /// key: while the Vault is locked its Projects are.
    pub(crate) fn clipboard_names(&self, source: &ClipboardSource, cx: &App) -> ClipboardNames {
        match source {
            ClipboardSource::Project { id, .. } => match self.projects.project(id) {
                Some(project) => ClipboardNames::Names(project.mapping.exported()),
                None if self.vault.needs_key() => ClipboardNames::Locked,
                None => ClipboardNames::Gone,
            },
            ClipboardSource::Tab { id, .. } => match self.tabs.get(*id).and_then(|t| t.view()) {
                Some(view) => ClipboardNames::Names(view.read(cx).pii.review().export_identities()),
                None => ClipboardNames::Gone,
            },
        }
    }
}
