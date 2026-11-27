//! The sidebar's Vault tree: Projects, which start collapsed and expand to
//! all their documents, then Unfiled's most recent documents with Show all.
//! Its rows come from the Vault's summaries and this owner's own state
//! (expanded Projects, Show all, the Unfiled selection), never from what is
//! viewed: what is viewed only marks a row selected. GPUI-free.

use mdoc_vault::{DocumentSummary, ProjectSummary};
use std::collections::HashSet;

/// How many Unfiled documents show until Show all.
pub(in crate::workspace) const UNFILED_RECENT: usize = 7;

/// A row the keyboard and the pointer can land on.
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub(in crate::workspace) enum RowKey {
    Project(String),
    Document(String),
    /// Unfiled's Show all, or Show fewer.
    ShowAll,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(in crate::workspace) enum Row {
    ProjectsLabel,
    Project {
        id: String,
        expanded: bool,
        documents: usize,
    },
    /// A document; `project` is none in Unfiled.
    Document {
        id: String,
        project: Option<String>,
    },
    UnfiledLabel,
    /// Show all (`hidden` documents more), or Show fewer when `hidden` is 0.
    ShowAll {
        hidden: usize,
    },
}

impl Row {
    pub(in crate::workspace) fn key(&self) -> Option<RowKey> {
        match self {
            Self::Project { id, .. } => Some(RowKey::Project(id.clone())),
            Self::Document { id, .. } => Some(RowKey::Document(id.clone())),
            Self::ShowAll { .. } => Some(RowKey::ShowAll),
            Self::ProjectsLabel | Self::UnfiledLabel => None,
        }
    }
}

/// What the window shows, as the tree sees it.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(in crate::workspace) enum Shown {
    /// Home, a page, or a view no row names (a New document before its
    /// first keystroke).
    Nothing,
    ProjectPage(String),
    Document(String),
}

/// What a step of the keyboard did.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(in crate::workspace) enum TreeEffect {
    /// Only the tree looks different.
    Repaint,
    /// The cursor moved to this row; focus follows it.
    Focus(RowKey),
}

#[derive(Default)]
pub(in crate::workspace) struct VaultTree {
    expanded: HashSet<String>,
    show_all: bool,
    /// Unfiled documents chosen for Delete, in the order they were chosen.
    selection: Vec<String>,
    /// Where a range selection starts.
    anchor: Option<String>,
    /// The row the arrows last moved to: the tree's one Tab stop.
    cursor: Option<RowKey>,
}

/// Unfiled's order: newest import first, then by id, neither of which an
/// edit changes, so a row never moves while its document is edited.
fn unfiled(documents: &[DocumentSummary]) -> Vec<&DocumentSummary> {
    let mut unfiled: Vec<_> = documents.iter().filter(|d| d.project.is_none()).collect();
    unfiled.sort_by(|a, b| (b.created.cmp(&a.created)).then_with(|| a.id.cmp(&b.id)));
    unfiled
}

/// A Project's documents, by title.
fn members<'a>(documents: &'a [DocumentSummary], project: &str) -> Vec<&'a DocumentSummary> {
    let mut members: Vec<_> = documents
        .iter()
        .filter(|d| d.project.as_deref() == Some(project))
        .collect();
    members.sort_by(|a, b| {
        (a.title.to_lowercase().cmp(&b.title.to_lowercase())).then_with(|| a.id.cmp(&b.id))
    });
    members
}

impl VaultTree {
    /// The expansion saved with the last session, by id.
    pub(in crate::workspace) fn restore(&mut self, expanded: impl IntoIterator<Item = String>) {
        self.expanded = expanded.into_iter().collect();
    }

    /// The expanded Projects' ids, sorted, for the session.
    pub(in crate::workspace) fn expanded_ids(&self) -> Vec<String> {
        let mut ids: Vec<_> = self.expanded.iter().cloned().collect();
        ids.sort();
        ids
    }

    pub(in crate::workspace) fn expanded(&self, project: &str) -> bool {
        self.expanded.contains(project)
    }

    /// Expand or collapse a Project's row.
    pub(in crate::workspace) fn set_expanded(&mut self, project: &str, expanded: bool) -> bool {
        if expanded {
            self.expanded.insert(project.to_string())
        } else {
            self.expanded.remove(project)
        }
    }

    pub(in crate::workspace) fn toggle(&mut self, project: &str) {
        let expanded = !self.expanded(project);
        self.set_expanded(project, expanded);
    }

    /// A Project was deleted.
    pub(in crate::workspace) fn forget_project(&mut self, project: &str) {
        self.expanded.remove(project);
        if self.cursor == Some(RowKey::Project(project.to_string())) {
            self.cursor = None;
        }
    }

    pub(in crate::workspace) fn toggle_show_all(&mut self) {
        self.show_all = !self.show_all;
    }

    /// The rows in order. `projects` come sorted (pinned, then recently
    /// updated); `documents` in any order.
    pub(in crate::workspace) fn rows(
        &self,
        projects: &[ProjectSummary],
        documents: &[DocumentSummary],
    ) -> Vec<Row> {
        let mut rows = vec![Row::ProjectsLabel];
        for project in projects {
            let members = members(documents, &project.id);
            let expanded = self.expanded(&project.id);
            rows.push(Row::Project {
                id: project.id.clone(),
                expanded,
                documents: members.len(),
            });
            if expanded {
                rows.extend(members.into_iter().map(|d| Row::Document {
                    id: d.id.clone(),
                    project: Some(project.id.clone()),
                }));
            }
        }
        rows.push(Row::UnfiledLabel);
        let unfiled = unfiled(documents);
        let total = unfiled.len();
        let shown = if self.show_all {
            total
        } else {
            total.min(UNFILED_RECENT)
        };
        rows.extend(unfiled.into_iter().take(shown).map(|d| Row::Document {
            id: d.id.clone(),
            project: None,
        }));
        if total > UNFILED_RECENT {
            rows.push(Row::ShowAll {
                hidden: total - shown,
            });
        }
        rows
    }

    /// The row marked selected for what is shown: the document's row; its
    /// collapsed Project's row; Show all while Unfiled hides it; the
    /// Project's row while its page shows.
    pub(in crate::workspace) fn selected(
        &self,
        rows: &[Row],
        shown: &Shown,
        documents: &[DocumentSummary],
    ) -> Option<RowKey> {
        let keys: Vec<RowKey> = rows.iter().filter_map(Row::key).collect();
        let found = |key: RowKey| keys.contains(&key).then_some(key);
        match shown {
            Shown::Nothing => None,
            Shown::ProjectPage(id) => found(RowKey::Project(id.clone())),
            Shown::Document(id) => {
                found(RowKey::Document(id.clone())).or_else(|| {
                    match documents.iter().find(|d| &d.id == id)?.project.clone() {
                        Some(project) => found(RowKey::Project(project)),
                        None => found(RowKey::ShowAll),
                    }
                })
            }
        }
    }

    // ---- The keyboard -------------------------------------------------------

    /// The tree's one Tab stop: the cursor's row while it is shown, else the
    /// selected row, else the first.
    pub(in crate::workspace) fn stop(
        &self,
        rows: &[Row],
        selected: Option<&RowKey>,
    ) -> Option<RowKey> {
        let keys: Vec<RowKey> = rows.iter().filter_map(Row::key).collect();
        (self.cursor.clone().filter(|key| keys.contains(key)))
            .or_else(|| selected.filter(|key| keys.contains(key)).cloned())
            .or_else(|| keys.first().cloned())
    }

    pub(in crate::workspace) fn set_cursor(&mut self, key: Option<RowKey>) {
        self.cursor = key;
    }

    /// ↑ / ↓ from `from`: the neighbouring row, stopping at the ends.
    pub(in crate::workspace) fn step(
        &mut self,
        rows: &[Row],
        from: &RowKey,
        delta: isize,
    ) -> Option<TreeEffect> {
        let keys: Vec<RowKey> = rows.iter().filter_map(Row::key).collect();
        let index = keys.iter().position(|key| key == from)?;
        let next = index.saturating_add_signed(delta).min(keys.len() - 1);
        (next != index).then(|| self.focus(keys[next].clone()))
    }

    /// → on a row: expand a collapsed Project, else move into it.
    pub(in crate::workspace) fn right(
        &mut self,
        rows: &[Row],
        from: &RowKey,
    ) -> Option<TreeEffect> {
        let RowKey::Project(id) = from else {
            return None;
        };
        if !self.expanded(id) {
            self.expanded.insert(id.clone());
            return Some(TreeEffect::Repaint);
        }
        let first = rows.iter().find_map(|row| match row {
            Row::Document {
                id: document,
                project: Some(project),
            } if project == id => Some(document.clone()),
            _ => None,
        })?;
        Some(self.focus(RowKey::Document(first)))
    }

    /// ← on a row: collapse an expanded Project, or move from a member to
    /// its Project's row.
    pub(in crate::workspace) fn left(&mut self, rows: &[Row], from: &RowKey) -> Option<TreeEffect> {
        match from {
            RowKey::Project(id) if self.expanded(id) => {
                self.expanded.remove(id);
                Some(TreeEffect::Repaint)
            }
            RowKey::Document(id) => {
                let project = rows.iter().find_map(|row| match row {
                    Row::Document {
                        id: document,
                        project,
                    } if document == id => project.clone(),
                    _ => None,
                })?;
                Some(self.focus(RowKey::Project(project)))
            }
            _ => None,
        }
    }

    fn focus(&mut self, key: RowKey) -> TreeEffect {
        self.cursor = Some(key.clone());
        TreeEffect::Focus(key)
    }

    // ---- The Unfiled selection ---------------------------------------------

    pub(in crate::workspace) fn selection(&self) -> &[String] {
        &self.selection
    }

    pub(in crate::workspace) fn is_chosen(&self, document: &str) -> bool {
        self.selection.iter().any(|id| id == document)
    }

    /// A plain click on Unfiled row `document`: it alone is chosen.
    pub(in crate::workspace) fn choose(&mut self, document: &str) {
        self.selection = vec![document.to_string()];
        self.anchor = Some(document.to_string());
    }

    /// ⌘-click (Ctrl-click): add or remove one row.
    pub(in crate::workspace) fn toggle_chosen(&mut self, document: &str) {
        if let Some(index) = self.selection.iter().position(|id| id == document) {
            self.selection.remove(index);
        } else {
            self.selection.push(document.to_string());
        }
        self.anchor = Some(document.to_string());
    }

    /// ⇧-click or ⇧↑/↓: every Unfiled row shown from the anchor to
    /// `document`.
    pub(in crate::workspace) fn extend_to(&mut self, rows: &[Row], document: &str) {
        let shown: Vec<&String> = rows
            .iter()
            .filter_map(|row| match row {
                Row::Document { id, project: None } => Some(id),
                _ => None,
            })
            .collect();
        let Some(to) = shown.iter().position(|id| *id == document) else {
            return;
        };
        let from = (self.anchor.as_ref())
            .and_then(|anchor| shown.iter().position(|id| *id == anchor))
            .unwrap_or(to);
        let (low, high) = (from.min(to), from.max(to));
        self.selection = shown[low..=high].iter().map(|id| (*id).clone()).collect();
        if self.anchor.is_none() {
            self.anchor = Some(document.to_string());
        }
    }

    /// Esc, or a click outside Unfiled.
    pub(in crate::workspace) fn clear_selection(&mut self) -> bool {
        self.anchor = None;
        !std::mem::take(&mut self.selection).is_empty()
    }

    /// Keep only chosen documents still in Unfiled.
    pub(in crate::workspace) fn keep_chosen(&mut self, documents: &[DocumentSummary]) {
        let unfiled: HashSet<&str> = documents
            .iter()
            .filter(|d| d.project.is_none())
            .map(|d| d.id.as_str())
            .collect();
        self.selection.retain(|id| unfiled.contains(id.as_str()));
        if (self.anchor.as_deref()).is_some_and(|anchor| !unfiled.contains(anchor)) {
            self.anchor = None;
        }
    }

    /// What Delete on Unfiled row `document` deletes: the selection when
    /// the row is in it, else that row alone.
    pub(in crate::workspace) fn to_delete(&self, document: &str) -> Vec<String> {
        if self.is_chosen(document) {
            self.selection.clone()
        } else {
            vec![document.to_string()]
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use mdoc_vault::{DocumentOrigin, SourceKind};

    fn id(c: char) -> String {
        c.to_string().repeat(32)
    }

    fn project(c: char, name: &str) -> ProjectSummary {
        ProjectSummary {
            id: id(c),
            name: name.into(),
            colour: None,
            pinned: false,
            created: 0,
            updated: 0,
        }
    }

    fn document(c: char, title: &str, project: Option<char>, created: u64) -> DocumentSummary {
        DocumentSummary {
            id: id(c),
            project: project.map(id),
            title: title.into(),
            source_kind: SourceKind::Text,
            origin: DocumentOrigin::Ordinary,
            exclude_from_ai: false,
            scanned: false,
            created,
            updated: created,
            source_sha256: None,
            counts: None,
        }
    }

    fn keys(rows: &[Row]) -> Vec<RowKey> {
        rows.iter().filter_map(Row::key).collect()
    }

    #[test]
    fn projects_start_collapsed_and_expand_to_their_documents_by_title() {
        let mut tree = VaultTree::default();
        let projects = [project('a', "Lease"), project('b', "Smith")];
        let documents = [
            document('1', "Schedule", Some('a'), 1),
            document('2', "Agreement", Some('a'), 2),
            document('3', "Notes", None, 3),
        ];
        let rows = tree.rows(&projects, &documents);
        assert_eq!(
            rows,
            [
                Row::ProjectsLabel,
                Row::Project {
                    id: id('a'),
                    expanded: false,
                    documents: 2
                },
                Row::Project {
                    id: id('b'),
                    expanded: false,
                    documents: 0
                },
                Row::UnfiledLabel,
                Row::Document {
                    id: id('3'),
                    project: None
                },
            ]
        );
        tree.toggle(&id('a'));
        assert_eq!(
            keys(&tree.rows(&projects, &documents))[1..3],
            [RowKey::Document(id('2')), RowKey::Document(id('1'))]
        );
        assert_eq!(tree.expanded_ids(), [id('a')]);
        let mut restored = VaultTree::default();
        restored.restore(tree.expanded_ids());
        assert_eq!(
            restored.rows(&projects, &documents),
            tree.rows(&projects, &documents)
        );
        tree.forget_project(&id('a'));
        assert!(tree.expanded_ids().is_empty());
    }

    #[test]
    fn unfiled_shows_its_seven_newest_until_show_all() {
        let mut tree = VaultTree::default();
        let documents: Vec<_> = "123456789"
            .chars()
            .enumerate()
            .map(|(i, c)| document(c, &format!("Doc {c}"), None, i as u64))
            .collect();
        let rows = tree.rows(&[], &documents);
        let shown = keys(&rows);
        assert_eq!(shown.len(), UNFILED_RECENT + 1);
        assert_eq!(shown[0], RowKey::Document(id('9')), "newest first");
        assert_eq!(rows.last(), Some(&Row::ShowAll { hidden: 2 }));
        tree.toggle_show_all();
        let rows = tree.rows(&[], &documents);
        assert_eq!(keys(&rows).len(), 10);
        assert_eq!(rows.last(), Some(&Row::ShowAll { hidden: 0 }));
        // Seven or fewer: no Show all.
        assert!(
            !tree
                .rows(&[], &documents[..7])
                .contains(&Row::ShowAll { hidden: 0 })
        );
    }

    #[test]
    fn viewing_marks_a_row_and_never_changes_the_rows() {
        let mut tree = VaultTree::default();
        let projects = [project('a', "Lease")];
        let documents: Vec<_> = [
            document('1', "Member", Some('a'), 1),
            document('2', "Old", None, 0),
        ]
        .into_iter()
        .chain(
            (3..10).map(|i| document(char::from_digit(i, 10).unwrap(), "New", None, 10 + i as u64)),
        )
        .collect();
        let rows = tree.rows(&projects, &documents);
        let selected =
            |tree: &VaultTree, rows: &[Row], shown| tree.selected(rows, &shown, &documents);
        // A collapsed Project's member marks the Project; a hidden Unfiled
        // document marks Show all.
        assert_eq!(
            selected(&tree, &rows, Shown::Document(id('1'))),
            Some(RowKey::Project(id('a')))
        );
        assert_eq!(
            selected(&tree, &rows, Shown::Document(id('2'))),
            Some(RowKey::ShowAll)
        );
        assert_eq!(
            selected(&tree, &rows, Shown::Document(id('9'))),
            Some(RowKey::Document(id('9')))
        );
        assert_eq!(
            selected(&tree, &rows, Shown::ProjectPage(id('a'))),
            Some(RowKey::Project(id('a')))
        );
        assert_eq!(selected(&tree, &rows, Shown::Nothing), None);
        assert_eq!(tree.rows(&projects, &documents), rows, "nothing moved");
        tree.toggle(&id('a'));
        let rows = tree.rows(&projects, &documents);
        assert_eq!(
            selected(&tree, &rows, Shown::Document(id('1'))),
            Some(RowKey::Document(id('1')))
        );
    }

    #[test]
    fn arrows_walk_rows_and_left_right_fold_projects() {
        let mut tree = VaultTree::default();
        let projects = [project('a', "Lease")];
        let documents = [
            document('1', "Member", Some('a'), 1),
            document('2', "Loose", None, 2),
        ];
        let rows = tree.rows(&projects, &documents);
        let a = RowKey::Project(id('a'));
        assert_eq!(tree.stop(&rows, None), Some(a.clone()));
        assert_eq!(
            tree.step(&rows, &a, 1),
            Some(TreeEffect::Focus(RowKey::Document(id('2'))))
        );
        assert_eq!(tree.stop(&rows, None), Some(RowKey::Document(id('2'))));
        assert_eq!(tree.step(&rows, &RowKey::Document(id('2')), 1), None);
        assert_eq!(tree.left(&rows, &a), None, "already collapsed");
        assert_eq!(tree.right(&rows, &a), Some(TreeEffect::Repaint));
        let rows = tree.rows(&projects, &documents);
        assert_eq!(
            tree.right(&rows, &a),
            Some(TreeEffect::Focus(RowKey::Document(id('1'))))
        );
        assert_eq!(
            tree.left(&rows, &RowKey::Document(id('1'))),
            Some(TreeEffect::Focus(a.clone()))
        );
        assert_eq!(tree.left(&rows, &RowKey::Document(id('2'))), None);
        assert_eq!(tree.left(&rows, &a), Some(TreeEffect::Repaint));
        assert!(!tree.expanded(&id('a')));
    }

    #[test]
    fn unfiled_selection_toggles_extends_and_drops_what_left() {
        let mut tree = VaultTree::default();
        let mut documents: Vec<_> = (1..=5)
            .map(|i| document(char::from_digit(i, 10).unwrap(), "D", None, i as u64))
            .collect();
        let rows = tree.rows(&[], &documents);
        // Shown newest first: 5 4 3 2 1.
        tree.choose(&id('4'));
        tree.extend_to(&rows, &id('2'));
        assert_eq!(tree.selection(), [id('4'), id('3'), id('2')]);
        tree.toggle_chosen(&id('3'));
        assert_eq!(tree.selection(), [id('4'), id('2')]);
        tree.toggle_chosen(&id('5'));
        assert_eq!(tree.to_delete(&id('2')), [id('4'), id('2'), id('5')]);
        assert_eq!(tree.to_delete(&id('1')), [id('1')]);
        // A moved document leaves the selection.
        documents[1].project = Some(id('a'));
        tree.keep_chosen(&documents);
        assert_eq!(tree.selection(), [id('4'), id('5')]);
        assert!(tree.clear_selection());
        assert!(!tree.clear_selection());
        // No anchor: a range starts at the row itself.
        tree.extend_to(&rows, &id('3'));
        assert_eq!(tree.selection(), [id('3')]);
    }
}
