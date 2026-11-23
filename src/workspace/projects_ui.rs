//! Projects in the window: they open with the Vault; a member tab gets its
//! Project's mapping and folds its own back in; a document moves into a
//! Project, into a new one or out to Unfiled with an undoable toast;
//! rename, colour, categories, lists and delete; the Open Project list.
use super::{
    Workspace,
    autosave::{Op, Write},
    document_writer::Target,
    projects::ProjectLink,
};
use crate::{document_view::DocumentView, markdown_search, style, ui};
use gpui::{AnyElement, App, Context, Entity, Focusable, PromptLevel, Window, div, prelude::*, px};
use mdoc_pii::restore::{ExportedIdentity, RestoreMap};
use mdoc_store::{
    document_id::{DocumentId, VaultId},
    lists::Lists,
};
use mdoc_vault::{
    Mapping, MoveUndo, Moved, NewProject, Project, ProjectDocuments, Vault, VaultError,
};
use std::{collections::HashSet, time::Duration};

gpui::actions!(
    projects,
    [
        ConfirmProjectName,
        CancelProjectName,
        AddProjectName,
        ClearProjectName
    ]
);

pub(crate) fn bind_keys(cx: &mut App) {
    let bindings = crate::shortcuts::bindings(crate::shortcuts::Owner::Projects, cx);
    cx.bind_keys(bindings);
}

/// How long the move toast stays, with its Undo.
const TOAST: Duration = Duration::from_secs(10);

/// The toast a move shows: what happened, and how to undo it.
pub(super) struct MoveToast {
    pub(super) text: String,
    /// Undo moves the document back; a Project the move created goes
    /// with it once empty.
    undo: Option<(MoveUndo, Option<String>)>,
    _timeout: gpui::Task<()>,
}

#[derive(Default)]
pub(super) struct ProjectsState {
    pub(super) page: Option<super::project_page::ProjectPage>,
    pub(super) header_menu: Option<(String, gpui::Point<gpui::Pixels>)>,
    pub(super) closing_members: super::project_page::ProjectClose,
    pub(super) renaming: Option<(String, Entity<markdown_search::SearchInput>)>,
    /// Saves the rename when focus leaves its field.
    rename_blur: Option<gpui::Subscription>,
    pub(super) list_open: bool,
    /// The link the Open Project list hangs from; none centers it.
    list_anchor: Option<gpui::Bounds<gpui::Pixels>>,
    /// Where Home's Open a Project… link was last painted.
    pub(super) home_link: std::rc::Rc<std::cell::Cell<Option<gpui::Bounds<gpui::Pixels>>>>,
    /// The Keychain explainer is shown.
    pub(super) explainer: bool,
    /// What runs once the explainer's Continue opened the key.
    then: Option<AfterUnlock>,
    /// Projects changed since their last write was queued.
    unwritten: HashSet<String>,
    pub(super) toast: Option<MoveToast>,
    /// Member loader invocations, including attempts that would fail to read.
    #[cfg(test)]
    pub(super) member_reads: usize,
}

type AfterUnlock = Box<dyn FnOnce(&mut Workspace, &mut Window, &mut Context<Workspace>)>;

impl ProjectsState {
    /// Project `id`'s last write failed: queue it again with the next.
    pub(super) fn mark_unwritten(&mut self, id: &str) {
        self.unwritten.insert(id.to_string());
    }
}

/// What a move shows in its toast, and whether it can be undone.
enum Moving {
    Into,
    New,
    Undo,
}

impl Workspace {
    // ---- The key ---------------------------------------------------------

    /// True when the Vault is open or can't be. While its key is locked,
    /// the Keychain explainer opens and `then` runs after its Continue.
    pub(super) fn vault_key_ready(
        &mut self,
        then: impl FnOnce(&mut Self, &mut Window, &mut Context<Self>) + 'static,
        cx: &mut Context<Self>,
    ) -> bool {
        if !self.vault.needs_key() {
            return true;
        }
        self.project_ui.explainer = true;
        self.explainer_focus_pending = true;
        // An action already waiting still runs, before this one.
        let earlier = self.project_ui.then.take();
        self.project_ui.then = Some(Box::new(move |this, window, cx| {
            if let Some(earlier) = earlier {
                earlier(this, window, cx);
            }
            then(this, window, cx);
        }));
        cx.notify();
        false
    }

    /// Show the explainer while the Vault's key is locked or was declined.
    pub(super) fn unlock_projects(&mut self, _window: &mut Window, cx: &mut Context<Self>) {
        if self.vault.needs_key() {
            self.project_ui.explainer = true;
            self.explainer_focus_pending = true;
            cx.notify();
        }
    }

    /// The explainer's Continue: read the key (macOS may ask now), open the
    /// Vault, then run the waiting action.
    pub(super) fn continue_unlock(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.close_explainer(window, cx);
        let then = self.project_ui.then.take();
        match self.unlock_vault(window, cx) {
            Ok(()) if self.vault.vault().is_some() => {
                if let Some(then) = then {
                    then(self, window, cx);
                }
            }
            // Declined: Try again still owes the user the action.
            Err(_) => {
                self.project_ui.then = then;
                self.carry_out_notices(self.notices.show_locked_projects());
            }
            Ok(()) => self.folder_key_cancelled(),
        }
        cx.notify();
    }

    /// The explainer's Not now: the waiting action is cancelled.
    pub(super) fn cancel_unlock(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.close_explainer(window, cx);
        self.project_ui.then = None;
        self.folder_key_cancelled();
        cx.notify();
    }

    /// Hide the explainer and give focus back to where it was.
    fn close_explainer(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if !self.project_ui.explainer {
            return;
        }
        self.project_ui.explainer = false;
        let focus = self
            .explainer_return
            .take()
            .unwrap_or_else(|| self.focus.clone());
        window.focus(&focus, cx);
    }

    // ---- Loading and members ----------------------------------------------

    /// Read every Project and the user-wide lists, once the Vault is open,
    /// and give loaded member tabs their mapping.
    pub(super) fn load_projects(&mut self, cx: &mut Context<Self>) {
        if self.projects.loaded() {
            return;
        }
        let Some(vault) = self.vault.vault().cloned() else {
            return;
        };
        let mut projects = Vec::new();
        let mut failed = 0;
        for summary in vault.projects() {
            match vault.project(&summary.id) {
                Ok(project) => projects.push(project),
                Err(_) => failed += 1,
            }
        }
        failed += (vault.damaged().iter())
            .filter(|(kind, _)| *kind == mdoc_vault::ObjectKind::Project)
            .count();
        self.projects.finish_load(projects);
        self.user_lists_read(vault.user_lists(), cx);
        if failed > 0 {
            self.notices.post(if failed == 1 {
                "1 Project could not be opened. It is damaged or saved with another key.".into()
            } else {
                format!(
                    "{failed} Projects could not be opened. They are damaged or saved with \
                     another key."
                )
            });
        }
        let ids: Vec<_> = self.tabs.iter().map(|t| t.id()).collect();
        for id in ids {
            self.attach_project(id, cx);
        }
        cx.notify();
    }

    /// Tab `id`'s Vault document, once its view holds it.
    pub(super) fn tab_document(&self, id: u64, cx: &App) -> Option<VaultId> {
        let view = self.tabs.get(id)?.view()?;
        match view.read(cx).session.document.id()? {
            DocumentId::Vault(id) => Some(id),
            DocumentId::File(_) => None,
        }
    }

    /// The Project the Vault files document `id` in, from its summary.
    pub(super) fn project_of(&self, id: &str) -> Option<String> {
        let vault = self.vault.vault()?;
        (vault.documents().into_iter())
            .find(|summary| summary.id == id)
            .and_then(|summary| summary.project)
    }

    /// After a document loads, give it its Project's mapping.
    pub(super) fn attach_project(&mut self, id: u64, cx: &mut Context<Self>) {
        if !self.projects.loaded() {
            self.load_projects(cx);
            if !self.projects.loaded() {
                return;
            }
        }
        let Some(view) = self.tab_view(id) else {
            return;
        };
        let (loaded, linked, restored) = {
            let v = view.read(cx);
            (
                !v.lifecycle.loading() && !v.lifecycle.unavailable() && !v.lifecycle.source_only(),
                v.project.link().is_some(),
                v.session.document.review.is_some(),
            )
        };
        if !loaded || linked {
            return;
        }
        let Some(document) = self.tab_document(id, cx) else {
            return;
        };
        let Some(project) = (self.project_of(document.as_str()))
            .and_then(|project| self.projects.project(&project))
        else {
            return;
        };
        let link = ProjectLink::of(project);
        let identities = project.mapping.exported();
        let reserved = project.mapping.reserved_aliases();
        let skip = project.skipped();
        let lists = project.lists.clone();
        let dismissed = project.dismissed_hints.clone();
        // What it took off as Not a name that the Project keeps for another
        // member stays off.
        let not_names = project.mapping.not_names_of(document.as_str());
        let renames = renames_of(&project.mapping);
        view.update(cx, |view, cx| {
            view.project.set_skip(skip);
            view.project.set_lists(lists);
            view.dismiss_project_hints(&dismissed, cx);
            if restored {
                // Its review came back as saved: it follows what other
                // members renamed since, and proposes the names it lacks.
                view.pii.review_mut().reserve_aliases(&reserved);
                view.project.attach(Some(link));
                view.sync_project_identities(&renames, &identities, cx);
            } else {
                view.open_project_mapping(link, &identities, &reserved, cx);
                view.pii.review_mut().leave_off(&not_names);
            }
        });
        cx.notify();
    }

    pub(super) fn tab_view(&self, id: u64) -> Option<Entity<DocumentView>> {
        self.tabs.get(id)?.view().cloned()
    }

    /// Whether tab `id` can move: a loaded Vault document, converted, and
    /// no move of it waits. An unconverted source keeps its document as
    /// admitted until it converts.
    pub(super) fn can_move(&self, id: u64, cx: &App) -> bool {
        self.vault.vault().is_some()
            && self
                .tab_document(id, cx)
                .is_some_and(|document| !self.moving.contains(&document))
            && self.tab_view(id).is_some_and(|view| {
                let view = view.read(cx);
                view.lifecycle.ready() && !view.lifecycle.loading() && !view.lifecycle.source_only()
            })
    }

    /// A member tab's mapping changed: fold it into its Project.
    pub(super) fn project_changed(&mut self, id: u64, cx: &mut Context<Self>) {
        let Some(view) = self.tab_view(id) else {
            return;
        };
        let Some(member) = self.tab_document(id, cx) else {
            return;
        };
        let member = member.as_str().to_string();
        if let Some(project) = view
            .read(cx)
            .project
            .link()
            .and_then(|link| self.projects.project(&link.id))
        {
            // Undo can restore an alias the Project has irreversibly
            // retired. Reconcile it before saving, rather than silently
            // dropping the member's mapping at the tombstone boundary.
            let reserved = project.mapping.reserved_aliases_for(&member);
            if view
                .read(cx)
                .pii
                .review()
                .export_identities()
                .iter()
                .any(|identity| {
                    reserved.contains(&identity.alias)
                        || identity
                            .previous
                            .iter()
                            .any(|alias| reserved.contains(alias))
                })
            {
                let identities = project.mapping.exported();
                if let Err(error) = view.update(cx, |view, cx| {
                    view.join_project_identities(&identities, &reserved, cx)
                }) {
                    self.notices
                        .post(format!("Could not keep the Project's names: {error}"));
                    return;
                }
            }
        }
        let (link, identities, not_names, kept) = {
            let v = view.read(cx);
            let Some(link) = v.project.link().cloned() else {
                return;
            };
            let review = v.pii.review();
            (
                link,
                review.export_identities(),
                review.not_a_name_pairs(),
                review.kept_pairs(),
            )
        };
        // Its Keeps and manual marks since the last save, for the Project's
        // lists; what other open members still list stays.
        let others: Vec<_> = self
            .tabs
            .iter()
            .filter(|tab| tab.id() != id)
            .filter_map(|tab| tab.view())
            .filter(|other| other.read(cx).project.is(&link.id))
            .map(|other| other.read(cx).project.listed().clone())
            .collect();
        let listed = view
            .update(cx, |view, cx| view.take_list_changes(cx))
            .map(|changes| changes.sparing(&others));
        // What this tab still lists (a homonym's Keep) stays too.
        let own = view.read(cx).project.listed().clone();
        // A Not a name keeps a wording another member's saved text still
        // restores.
        let used = if not_names.is_empty() {
            HashSet::new()
        } else {
            (self.projects.project(&link.id))
                .map(|project| self.restored_in_members(project, &member))
                .unwrap_or_default()
        };
        let Some((renames, added, exported, lists)) = self.projects.edit(&link.id, |project| {
            let before = project.mapping.clone();
            let lists_before = project.lists.clone();
            fold(
                &mut project.mapping,
                &identities,
                &kept,
                &not_names,
                &member,
                |alias| used.contains(alias),
            );
            // A wording taken off leaves both lists, whatever session put it
            // there, unless another open member lists it.
            let mut changes = listed.clone().unwrap_or_default();
            for (_, wording) in &not_names {
                if let Some(entry) = project.lists.always_for(wording).cloned()
                    && own.always_for(wording).is_none()
                    && !others.iter().any(|o| o.always_for(wording).is_some())
                    && !changes.always_removed.contains(&entry)
                {
                    changes.always_removed.push(entry);
                }
                if project.lists.never_has(wording)
                    && !own.never_has(wording)
                    && !others.iter().any(|o| o.never_has(wording))
                    && !changes.never_removed.contains(wording)
                {
                    changes.never_removed.push(wording.clone());
                }
            }
            let lists = (!changes.is_empty() && project.lists.apply(&changes))
                .then(|| project.lists.clone());
            if project.mapping == before && project.lists == lists_before {
                return None;
            }
            // Renamed or recategorized identities, by the alias they had.
            let renames = project.mapping.renames_since(&before);
            let added = project.mapping.identities.len() > before.identities.len();
            Some((renames, added, project.mapping.exported(), lists))
        }) else {
            return;
        };
        view.update(cx, |view, _| view.project.set_names(exported.clone()));
        self.write_project(&link.id, cx);
        if let Some(lists) = lists {
            self.project_lists_changed(&link.id, &lists, cx);
        }
        if renames.is_empty() && !added {
            return;
        }
        // Other open members follow, each as an edit in its own history.
        for tab in &self.tabs {
            if tab.id() == id {
                continue;
            }
            if let Some(view) = tab.view()
                && view.read(cx).project.is(&link.id)
            {
                view.update(cx, |view, cx| {
                    view.sync_project_identities(&renames, &exported, cx)
                });
            }
        }
    }

    /// The aliases of `project`'s identities that its members' saved text
    /// restores, `member`'s own included; a member that can't be read
    /// counts as needing them all.
    fn restored_in_members(&self, project: &Project, member: &str) -> HashSet<String> {
        let map = RestoreMap::new(project.mapping.exported());
        let all = || -> HashSet<String> {
            map.identities()
                .iter()
                .map(|identity| identity.alias.clone())
                .collect()
        };
        let Some(vault) = self.vault.vault() else {
            return all();
        };
        let mut members: Vec<String> = (vault.documents_in(Some(&project.id)).into_iter())
            .map(|summary| summary.id)
            .collect();
        if !members.iter().any(|id| id == member) {
            members.push(member.to_string());
        }
        members
            .iter()
            .flat_map(|id| match vault.document(id) {
                Ok(document) => map.restored_aliases(&document.markdown),
                Err(_) => all().into_iter().collect(),
            })
            .collect()
    }

    /// A name the Project should replace before any document mentions it.
    pub(super) fn add_project_term(&mut self, id: u64, term: &str, cx: &mut Context<Self>) {
        let Some(link) = (self.tab_view(id)).and_then(|view| view.read(cx).project.link().cloned())
        else {
            return;
        };
        self.add_project_name(&link.id, term, cx);
    }

    /// A member dismissed a merge hint: the Project keeps it and its other
    /// open members hide it too.
    pub(super) fn dismiss_project_hint(
        &mut self,
        id: u64,
        from: &str,
        into: &str,
        cx: &mut Context<Self>,
    ) {
        let Some(link) = self
            .tab_view(id)
            .and_then(|view| view.read(cx).project.link().cloned())
        else {
            return;
        };
        let pair = (from.to_string(), into.to_string());
        if self
            .projects
            .edit(&link.id, |project| {
                (!project.dismissed_hints.contains(&pair))
                    .then(|| project.dismissed_hints.push(pair.clone()))
            })
            .is_none()
        {
            return;
        }
        self.write_project(&link.id, cx);
        for tab in &self.tabs {
            if let Some(view) = tab.view()
                && view.read(cx).project.is(&link.id)
            {
                let pair = std::slice::from_ref(&pair);
                view.update(cx, |view, cx| view.dismiss_project_hints(pair, cx));
            }
        }
    }

    pub(super) fn add_project_name(
        &mut self,
        project_id: &str,
        term: &str,
        cx: &mut Context<Self>,
    ) {
        let term = term.trim();
        let Some(exported) = self.projects.edit(project_id, |project| {
            if term.is_empty()
                || (project.mapping.identities.iter())
                    .any(|i| i.originals.iter().any(|o| o == term))
            {
                return None;
            }
            let category = mdoc_detect::guess_category(term, 0..term.len());
            let identity = ExportedIdentity {
                alias: project.mapping.next_alias(category),
                previous: Vec::new(),
                category,
                custom: false,
                originals: vec![term.into()],
            };
            project.mapping.merge(&[identity]);
            Some(project.mapping.exported())
        }) else {
            return;
        };
        self.write_project(project_id, cx);
        for tab in &self.tabs {
            if let Some(view) = tab.view()
                && view.read(cx).project.is(project_id)
            {
                view.update(cx, |view, cx| {
                    view.sync_project_identities(&[], &exported, cx)
                });
            }
        }
        cx.notify();
    }

    // ---- Writes -----------------------------------------------------------

    /// Save Project `id` through the writer, as it is when its write runs.
    pub(super) fn write_project(&mut self, id: &str, cx: &mut Context<Self>) {
        if !self.project_ui.unwritten.insert(id.to_string()) {
            return;
        }
        let entity = cx.entity().downgrade();
        let window = self.window_handle;
        cx.defer(move |cx| {
            let _ = window.update(cx, |_, window, cx| {
                let _ = entity.update(cx, |this, cx| this.queue_project_writes(window, cx));
            });
        });
    }

    /// Queue the writes `write_project` asked for.
    pub(super) fn queue_project_writes(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let ids: Vec<_> = self.project_ui.unwritten.drain().collect();
        for id in ids {
            self.queue_write(Target::Project(id), Write::Project, window, cx);
        }
    }

    /// Write the Projects a move reads, as this window has them, before it
    /// runs: a move then never loses a change waiting to be written.
    fn write_now(&mut self, vault: &Vault, ids: &[Option<String>]) -> Result<(), VaultError> {
        for id in ids.iter().flatten() {
            if let Some(project) = self.projects.project(id).cloned() {
                vault.update_project(id, |stored| keep_fields(stored, &project))?;
            }
        }
        Ok(())
    }

    // ---- Moves ------------------------------------------------------------

    /// Move tab `id`'s document into Project `to`, or Unfiled for `None`,
    /// once its last text is written.
    pub(super) fn move_tab(
        &mut self,
        id: u64,
        to: Option<String>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if !self.can_move(id, cx) {
            return;
        }
        let Some(document) = self.tab_document(id, cx) else {
            return;
        };
        if self.project_of(document.as_str()) == to {
            return;
        }
        self.project_changed(id, cx);
        self.queue_op(document, Op::Move { to }, window, cx);
    }

    /// Move a document by id, open or not: the Project page's Remove.
    pub(super) fn move_document(
        &mut self,
        document: VaultId,
        to: Option<String>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if self.moving.contains(&document) {
            return;
        }
        // A tab showing it moves it; a tab not loaded yet loads the moved
        // document when first shown.
        if let Some(tab) = self.find_document(&DocumentId::Vault(document.clone()), None, cx)
            && self.tab_view(tab).is_some()
        {
            self.move_tab(tab, to, window, cx);
            return;
        }
        self.queue_op(document, Op::Move { to }, window, cx);
    }

    /// Move to New Project: a Project named after the document, holding it.
    pub(super) fn move_to_new_project(
        &mut self,
        id: u64,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if !self.can_move(id, cx) {
            return;
        }
        let Some(document) = self.tab_document(id, cx) else {
            return;
        };
        let name = self
            .tab_view(id)
            .map(|view| view.read(cx).display_name())
            .unwrap_or_default();
        let name = name.strip_suffix(".md").unwrap_or(&name).to_string();
        self.project_changed(id, cx);
        let skip = self
            .preferences
            .borrow()
            .current
            .as_ref()
            .map(|p| p.categories.skip.clone())
            .unwrap_or_default();
        let dismissed = self
            .tab_view(id)
            .map(|view| view.read(cx).dismissed_merge_hints())
            .unwrap_or_default();
        self.queue_op(
            document,
            Op::New {
                name,
                skip,
                dismissed,
            },
            window,
            cx,
        );
    }

    /// Move to New Project from a document row: its live view moves it, or
    /// a document not live moves under its title.
    pub(super) fn move_document_to_new_project(
        &mut self,
        document: VaultId,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if self.moving.contains(&document) || self.vault.vault().is_none() {
            return;
        }
        if let Some(tab) = self.find_document(&DocumentId::Vault(document.clone()), None, cx)
            && self.tab_view(tab).is_some()
        {
            self.move_to_new_project(tab, window, cx);
            return;
        }
        let Some(name) = (self.vault.vault().into_iter())
            .flat_map(|vault| vault.documents())
            .find(|summary| summary.id == document.as_str())
            .map(|summary| summary.title)
        else {
            return;
        };
        let name = name.strip_suffix(".md").unwrap_or(&name).to_string();
        let skip = self
            .preferences
            .borrow()
            .current
            .as_ref()
            .map(|p| p.categories.skip.clone())
            .unwrap_or_default();
        self.queue_op(
            document,
            Op::New {
                name,
                skip,
                dismissed: Vec::new(),
            },
            window,
            cx,
        );
    }

    /// The toast's Undo.
    pub(super) fn undo_move(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let Some((undo, made)) = self.project_ui.toast.take().and_then(|toast| toast.undo) else {
            return;
        };
        let Some(document) = VaultId::parse(&undo.document) else {
            return;
        };
        if let Some(tab) = self.find_document(&DocumentId::Vault(document.clone()), None, cx) {
            self.project_changed(tab, cx);
        }
        self.queue_op(document, Op::Undo { undo, made }, window, cx);
        cx.notify();
    }

    /// Hold `document`'s autosaves, write what it shows now, then run `op`
    /// once the writes before it are done.
    fn queue_op(&mut self, document: VaultId, op: Op, window: &mut Window, cx: &mut Context<Self>) {
        if let Some(tab) = self.find_document(&DocumentId::Vault(document.clone()), None, cx) {
            self.autosave(tab, window, cx);
        }
        self.queue_project_writes(window, cx);
        self.moving.insert(document.clone());
        self.next_op += 1;
        let target = Target::Op(self.next_op);
        self.queue_write(target, Write::Op(document, op), window, cx);
    }

    /// Run a move at the head of the writer's queue: nothing else writes
    /// meanwhile. Its Projects are written first; the moved document's tab
    /// loads again.
    pub(super) fn run_op(
        &mut self,
        document: VaultId,
        op: Op,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let Some(vault) = self.vault.vault().cloned() else {
            self.moving.remove(&document);
            return;
        };
        let id = document.as_str();
        // What its tab shows now, typed while the move waited included, is
        // what moves; if it can't be written, nothing moves.
        let tab = self.find_document(&DocumentId::Vault(document.clone()), None, cx);
        if let Some(tab) = tab
            && let Err(error) = self.write_tab_now(tab, &vault, cx)
        {
            self.moving.remove(&document);
            let title =
                (self.tab_view(tab)).map_or_else(String::new, |v| v.read(cx).display_name());
            self.notices.post(format!(
                "Could not move “{title}”: its text could not be saved. {error}"
            ));
            cx.notify();
            return;
        }
        let from = self.project_of(id);
        let before = vault.document(id).ok();
        let title = before.as_ref().map(|d| d.title.clone()).unwrap_or_default();
        let (result, moving, made) = match op {
            Op::Move { to } => (
                self.write_now(&vault, &[from.clone(), to.clone()])
                    .and_then(|()| vault.move_document(id, to.as_deref())),
                Moving::Into,
                None,
            ),
            Op::New {
                name,
                skip,
                dismissed,
            } => {
                let result = self
                    .write_now(&vault, std::slice::from_ref(&from))
                    .and_then(|()| {
                        let new = NewProject {
                            name,
                            colour: None,
                            pinned: false,
                        };
                        let moved = vault.move_document_to_new_project(id, new)?;
                        // Its categories start from Settings › Detection, and
                        // the hints it dismissed stay dismissed.
                        let mut moved = moved;
                        if let Some(project) = &moved.project {
                            moved.project = Some(vault.update_project(&project.id, |project| {
                                project.skip = skip;
                                project.dismissed_hints = dismissed;
                            })?);
                        }
                        Ok(moved)
                    });
                let made = (result.as_ref().ok())
                    .and_then(|moved| moved.project.as_ref().map(|p| p.id.clone()));
                (result, Moving::New, made)
            }
            Op::Undo { undo, made } => {
                let result = self
                    .write_now(&vault, &[from.clone(), undo.project.clone()])
                    .and_then(|()| vault.undo_move(&undo));
                // A Project the move made goes with the undo, once empty.
                if result.is_ok()
                    && let Some(made) = &made
                    && vault.documents_in(Some(made)).is_empty()
                    && vault.delete_project(made, ProjectDocuments::Delete).is_ok()
                {
                    self.projects.remove(made);
                    self.navigation
                        .forget(&super::navigation::Stop::Project(made.clone()));
                }
                (result, Moving::Undo, None)
            }
            Op::Delete { .. } | Op::DeleteDocuments { .. } => {
                unreachable!("deletes run as their own step")
            }
        };
        match result {
            Ok(moved) => {
                let (names, aliases) = (before.as_ref())
                    .and_then(|before| self.alias_changes(before, &moved))
                    .unwrap_or_default();
                let renumbered = before.as_ref().map_or(0, |before| {
                    count_renumbered(&before.markdown, &names, &aliases)
                });
                self.took_move(&moved);
                self.sync_members(&moved, &document, cx);
                let tab = self.find_document(&DocumentId::Vault(document.clone()), None, cx);
                match tab {
                    Some(tab) => {
                        // A reply pasted in the tab names the people it did.
                        if let Some(view) = self.tab_view(tab) {
                            view.update(cx, |view, cx| view.realias_reply(names, &aliases, cx));
                        }
                        self.reload_tab(tab, document, window, cx)
                    }
                    None => {
                        self.moving.remove(&document);
                    }
                }
                let place = match &moved.project {
                    Some(project) => format!("“{}”", project.name),
                    None => "Unfiled".to_string(),
                };
                let text = match moving {
                    Moving::Undo => format!("“{title}” moved back to {place}."),
                    _ if renumbered > 0 => format!(
                        "“{title}” moved to {place}. {renumbered} {} renumbered to fit.",
                        if renumbered == 1 { "alias" } else { "aliases" }
                    ),
                    _ => format!("“{title}” moved to {place}."),
                };
                let undo = (!matches!(moving, Moving::Undo)).then(|| (moved.undo.clone(), made));
                self.show_toast(text, undo, window, cx);
            }
            Err(error) => {
                self.moving.remove(&document);
                self.notices
                    .post(format!("Could not move “{title}”: {error}"));
                if let Some(tab) = self.find_document(&DocumentId::Vault(document), None, cx) {
                    self.autosave(tab, window, cx);
                }
            }
        }
        self.checkpoint(window, cx);
        cx.notify();
    }

    /// The Projects a move wrote, in this window.
    fn took_move(&mut self, moved: &Moved) {
        for project in moved.project.iter().chain(&moved.left) {
            self.projects.moved(project.clone());
        }
    }

    /// The other open members of the Projects a move wrote take their names
    /// as they are now: a name the move brought is known, and its alias is
    /// taken, before any of them can hand that alias to someone else.
    fn sync_members(&mut self, moved: &Moved, document: &VaultId, cx: &mut Context<Self>) {
        for project in moved.project.iter().chain(&moved.left) {
            let Some(project) = self.projects.project(&project.id) else {
                continue;
            };
            let exported = project.mapping.exported();
            let reserved = project.mapping.reserved_aliases();
            for tab in self.tabs.iter() {
                let Some(view) = tab.view() else {
                    continue;
                };
                let member = view.read(cx).project.is(&project.id)
                    && !view
                        .read(cx)
                        .session
                        .document
                        .is(&DocumentId::Vault(document.clone()));
                if member {
                    view.update(cx, |view, cx| {
                        view.pii.review_mut().reserve_aliases(&reserved);
                        view.sync_project_identities(&[], &exported, cx);
                    });
                }
            }
        }
    }

    /// The names `before` had where it was, each beside the alias the move
    /// gave it, where that differs: a moved document's names keep their
    /// ids. Read before the window takes the move's Projects.
    fn alias_changes(
        &self,
        before: &mdoc_vault::Document,
        moved: &Moved,
    ) -> Option<(Vec<ExportedIdentity>, Vec<Option<String>>)> {
        let from = match &before.project {
            Some(id) => &self.projects.project(id)?.mapping,
            None => &before.mapping,
        };
        let to = match &moved.project {
            Some(project) => &project.mapping,
            None => &moved.document.mapping,
        };
        Some(
            (from.identities.iter())
                .filter_map(|identity| Some((identity, identity.exported()?)))
                .map(|(identity, exported)| {
                    let now = (to.bound(&identity.id))
                        .map(|now| now.alias.clone())
                        .filter(|alias| *alias != exported.alias);
                    (exported, now)
                })
                .unzip(),
        )
    }

    /// Load tab `id`'s document again from the Vault: a move rewrote it.
    fn reload_tab(
        &mut self,
        id: u64,
        document: VaultId,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let Some(view) = self.tab_view(id) else {
            self.moving.remove(&document);
            return;
        };
        let mut record = self
            .tabs
            .get(id)
            .map(|tab| tab.record().clone())
            .unwrap_or_default();
        view.update(cx, |view, cx| {
            record.caret = view.editor.read(cx).cursor();
            let (_, effect) = view.project.leave();
            view.carry_out_project_link(Some(effect), cx);
        });
        self.reloading.insert(document.clone());
        self.load_document(id, DocumentId::Vault(document), record, window, cx);
    }

    fn show_toast(
        &mut self,
        text: String,
        undo: Option<(MoveUndo, Option<String>)>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let timeout = cx.spawn_in(window, async move |this, cx| {
            cx.background_executor().timer(TOAST).await;
            let _ = this.update(cx, |this, cx| {
                this.project_ui.toast = None;
                cx.notify();
            });
        });
        self.project_ui.toast = Some(MoveToast {
            text,
            undo,
            _timeout: timeout,
        });
    }

    pub(super) fn dismiss_toast(&mut self, cx: &mut Context<Self>) {
        self.project_ui.toast = None;
        cx.notify();
    }

    /// The move toast, over the bottom of the content.
    pub(super) fn move_toast(&self, cx: &mut Context<Self>) -> Option<AnyElement> {
        let toast = self.project_ui.toast.as_ref()?;
        let theme = self.theme.get();
        let palette = theme.pdf_style();
        let panel = ui::panel("move-toast", theme)
            .when(cfg!(test), |v| v.debug_selector(|| "move-toast".into()))
            .flex()
            .items_center()
            .gap_3()
            .px_3()
            .py_2()
            .max_w(px(520.))
            .text_size(px(13.))
            .role(gpui::Role::Status)
            .child(div().flex_1().min_w_0().child(toast.text.clone()))
            .when(toast.undo.is_some(), |v| {
                v.child(
                    ui::control("move-toast-undo", "Undo", theme, true)
                        .when(cfg!(test), |v| {
                            v.debug_selector(|| "move-toast-undo".into())
                        })
                        .on_click(cx.listener(|this, _, window, cx| this.undo_move(window, cx))),
                )
            })
            .child(
                ui::control("move-toast-close", "×", theme, true)
                    .aria_label("Dismiss")
                    .text_color(palette.header_muted)
                    .on_click(cx.listener(|this, _, _, cx| this.dismiss_toast(cx))),
            );
        Some(
            gpui::deferred(
                div()
                    .absolute()
                    .bottom(px(24.))
                    .left_0()
                    .right_0()
                    .flex()
                    .justify_center()
                    .child(panel),
            )
            .with_priority(1)
            .into_any_element(),
        )
    }

    // ---- Delete -----------------------------------------------------------

    /// Ask, then delete the Project: a non-empty one asks whether its
    /// documents move to Unfiled or are deleted with it.
    pub(super) fn delete_project(
        &mut self,
        project_id: String,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let Some(name) = self.projects.project(&project_id).map(|p| p.name.clone()) else {
            return;
        };
        let count =
            (self.vault.vault()).map_or(0, |vault| vault.documents_in(Some(&project_id)).len());
        let answer = if count == 0 {
            window.prompt(
                PromptLevel::Warning,
                &format!("Delete the Project “{name}”?"),
                Some(
                    "Its names and its AI replies are deleted from this computer. This cannot \
                     be undone.",
                ),
                &["Delete", "Cancel"],
                cx,
            )
        } else {
            let documents = if count == 1 {
                "Its document".to_string()
            } else {
                format!("Its {count} documents")
            };
            window.prompt(
                PromptLevel::Warning,
                &format!("Delete the Project “{name}”?"),
                Some(&format!(
                    "{documents} can move to Unfiled, keeping the names their text uses, or be \
                     deleted with it. The Project's own AI replies are deleted either way. This \
                     cannot be undone."
                )),
                &[
                    gpui::PromptButton::new("Move to Unfiled"),
                    gpui::PromptButton::new("Delete Documents"),
                    gpui::PromptButton::cancel("Cancel"),
                ],
                cx,
            )
        };
        cx.spawn_in(window, async move |this, cx| {
            // An empty Project has no documents to move.
            let documents = match answer.await.ok() {
                Some(0) => ProjectDocuments::MoveToUnfiled,
                Some(1) if count > 0 => ProjectDocuments::Delete,
                _ => return,
            };
            let _ = this.update_in(cx, |this, window, cx| {
                this.forget_project(&project_id, documents, window, cx)
            });
        })
        .detach();
    }

    /// Delete Project `project_id` now, its documents going as `documents`
    /// says. Its open members load again as Unfiled, or close unwritten.
    pub(super) fn forget_project(
        &mut self,
        project_id: &str,
        documents: ProjectDocuments,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let Some(vault) = self.vault.vault().cloned() else {
            return;
        };
        let members: Vec<VaultId> = (vault.documents_in(Some(project_id)).into_iter())
            .filter_map(|summary| VaultId::parse(&summary.id))
            .collect();
        // Their last text is written first; then nothing writes them.
        for member in &members {
            if let Some(tab) = self.find_document(&DocumentId::Vault(member.clone()), None, cx) {
                self.project_changed(tab, cx);
                self.autosave(tab, window, cx);
            }
        }
        for member in &members {
            self.moving.insert(member.clone());
        }
        self.next_op += 1;
        let op = Op::Delete {
            project: project_id.to_string(),
            documents,
            members,
        };
        let target = Target::Op(self.next_op);
        // Any document id stands in: a delete names its members itself.
        let id = VaultId::parse(&"0".repeat(32)).expect("a valid id");
        self.queue_write(target, Write::Op(id, op), window, cx);
    }

    /// Run a Project delete at the head of the writer's queue.
    pub(super) fn run_delete(
        &mut self,
        project_id: String,
        documents: ProjectDocuments,
        members: Vec<VaultId>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let Some(vault) = self.vault.vault().cloned() else {
            return;
        };
        // Only what was confirmed is deleted: a move since that changed the
        // Project's documents stops it.
        let mut now: Vec<String> = (vault.documents_in(Some(&project_id)).into_iter())
            .map(|summary| summary.id)
            .collect();
        let mut confirmed: Vec<String> = members.iter().map(|m| m.as_str().to_string()).collect();
        now.sort();
        confirmed.sort();
        if now != confirmed {
            for member in &members {
                self.moving.remove(member);
            }
            self.notices.post(
                "The Project's documents changed before it was deleted, so nothing was \
                 deleted. Delete it again to choose for them as they are now.",
            );
            cx.notify();
            return;
        }
        let result = vault.delete_project(&project_id, documents);
        let tabs: Vec<(u64, VaultId)> = (members.iter())
            .filter_map(|member| {
                let tab = self.find_document(&DocumentId::Vault(member.clone()), None, cx)?;
                Some((tab, member.clone()))
            })
            .collect();
        match result {
            Ok(()) => {
                if (self.project_ui.page.as_ref()).is_some_and(|page| page.id() == project_id) {
                    // Home takes the deleted Project page's stop.
                    self.show_home_as(super::navigation::Arrival::Replace, window, cx);
                }
                self.navigation
                    .forget(&super::navigation::Stop::Project(project_id.clone()));
                self.projects.remove(&project_id);
                self.tree.forget_project(&project_id);
                for (tab, member) in tabs {
                    match documents {
                        ProjectDocuments::MoveToUnfiled => self.reload_tab(tab, member, window, cx),
                        ProjectDocuments::Delete => {
                            self.moving.remove(&member);
                            self.remove(tab, window, cx);
                        }
                    }
                }
                for member in &members {
                    if self
                        .find_document(&DocumentId::Vault(member.clone()), None, cx)
                        .is_none()
                    {
                        self.moving.remove(member);
                    }
                }
            }
            Err(error) => {
                // What it moved or deleted before the failure is as the
                // Vault holds it now.
                if let Ok(project) = vault.project(&project_id) {
                    self.projects.put(project);
                }
                for (tab, member) in tabs {
                    self.reload_tab(tab, member, window, cx);
                }
                for member in &members {
                    if self
                        .find_document(&DocumentId::Vault(member.clone()), None, cx)
                        .is_none()
                    {
                        self.moving.remove(member);
                    }
                }
                self.notices
                    .post(format!("Could not delete the Project: {error}"));
            }
        }
        self.checkpoint(window, cx);
        cx.notify();
    }

    // ---- Rename, categories, lists ----------------------------------------

    pub(super) fn start_rename_project(
        &mut self,
        project_id: String,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let Some(name) = self.projects.project(&project_id).map(|p| p.name.clone()) else {
            return;
        };
        let input = cx.new(|cx| {
            let mut input = markdown_search::SearchInput::new(cx)
                .with_theme(self.theme.clone())
                .with_key_context("ProjectName")
                .with_placeholder("Project name");
            input.set_value(name, cx);
            input
        });
        let focus = input.read(cx).focus_handle(cx);
        window.focus(&focus, cx);
        // Leaving the field saves, like Enter (Escape still cancels).
        self.project_ui.rename_blur =
            Some(cx.on_focus_out(&focus, window, |this, _, window, cx| {
                this.finish_rename(true, window, cx)
            }));
        self.project_ui.renaming = Some((project_id, input));
        cx.notify();
    }

    pub(super) fn finish_rename(
        &mut self,
        save: bool,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let Some(input) = self.end_rename(save, cx) else {
            return;
        };
        // A click on another control has already moved focus there; keep it.
        if input.read(cx).focus_handle(cx).contains_focused(window, cx) {
            self.focus_active(window, cx);
        }
        cx.notify();
    }

    /// End a rename in progress, saving a non-empty name when `save`.
    /// Leaving the Project page for a tab, Home or another Project saves, as
    /// a click away does.
    pub(super) fn end_rename(
        &mut self,
        save: bool,
        cx: &mut Context<Self>,
    ) -> Option<Entity<markdown_search::SearchInput>> {
        let (project_id, input) = self.project_ui.renaming.take()?;
        self.project_ui.rename_blur = None;
        let name = input.read(cx).value().trim().to_string();
        if save && !name.is_empty() {
            if self
                .projects
                .edit(&project_id, |project| {
                    (project.name != name).then(|| project.name = name.clone())
                })
                .is_some()
            {
                self.write_project(&project_id, cx);
            }
            for view in self.tabs.iter().filter_map(|t| t.view().cloned()) {
                view.update(cx, |view, cx| {
                    let effect = view.project.project_renamed(&project_id, &name);
                    view.carry_out_project_link(effect, cx);
                });
            }
        }
        Some(input)
    }

    /// Show the Project page.
    pub(super) fn open_project(
        &mut self,
        project_id: &str,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.show_project_page(project_id, super::project_page::Place::Header, window, cx);
    }

    pub(super) fn show_project_list(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.show_project_list_at(None, window, cx);
    }

    /// Open the list below `anchor`, the link it was opened from (#307).
    pub(super) fn show_project_list_at(
        &mut self,
        anchor: Option<gpui::Bounds<gpui::Pixels>>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        // An existing modal keeps focus; the list waits for it.
        if self.project_ui.explainer {
            let earlier = self.project_ui.then.take();
            self.project_ui.then = Some(Box::new(move |this, window, cx| {
                if let Some(earlier) = earlier {
                    earlier(this, window, cx);
                }
                this.show_project_list_at(anchor, window, cx);
            }));
            return;
        }
        self.load_projects(cx);
        self.project_ui.list_open = true;
        self.project_ui.list_anchor = anchor;
        self.project_list_return = window.focused(cx).filter(|f| *f != self.project_list_focus);
        // After the palette, if it ran this, has restored its own focus.
        let focus = self.project_list_focus.clone();
        window.defer(cx, move |window, cx| window.focus(&focus, cx));
        cx.notify();
    }

    /// Close the Open Project list, focus back where it was opened from.
    pub(super) fn close_project_list(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.project_ui.list_open = false;
        match self.project_list_return.take() {
            Some(focus) => window.focus(&focus, cx),
            None => self.focus_active(window, cx),
        }
        cx.notify();
    }

    /// Switch a category on or off for a Project and its open members.
    pub(super) fn toggle_project_category(
        &mut self,
        project_id: &str,
        category: mdoc_pii::Category,
        cx: &mut Context<Self>,
    ) {
        let Some(skip) = self.projects.edit(project_id, |project| {
            let token = category.token().to_string();
            if project.skip.contains(&token) {
                project.skip.retain(|s| *s != token);
            } else {
                project.skip.push(token);
            }
            Some(project.skipped())
        }) else {
            return;
        };
        self.write_project(project_id, cx);
        for view in self.tabs.iter().filter_map(|t| t.view().cloned()) {
            view.update(cx, |view, cx| {
                let effect = view.project.project_skip_changed(project_id, &skip);
                view.carry_out_project_link(effect, cx);
            });
        }
        cx.notify();
    }

    /// Project `project_id`'s lists changed: its open members scan with
    /// them.
    pub(super) fn project_lists_changed(
        &mut self,
        project_id: &str,
        lists: &Lists,
        cx: &mut Context<Self>,
    ) {
        for view in self.tabs.iter().filter_map(|t| t.view().cloned()) {
            view.update(cx, |view, _| {
                view.project.project_lists_changed(project_id, lists)
            });
        }
        cx.notify();
    }

    /// The Project page's Remove on an Always (`always`) or Never entry.
    pub(super) fn remove_project_list_entry(
        &mut self,
        project_id: &str,
        always: bool,
        wording: &str,
        cx: &mut Context<Self>,
    ) {
        let Some(lists) = self.projects.edit(project_id, |project| {
            let removed = if always {
                project.lists.remove_always(wording)
            } else {
                project.lists.remove_never(wording)
            };
            removed.then(|| project.lists.clone())
        }) else {
            return;
        };
        self.write_project(project_id, cx);
        self.project_lists_changed(project_id, &lists, cx);
    }

    /// File → Open Project…: every Project, pinned then most recent.
    pub(super) fn project_list(&self, cx: &mut Context<Self>) -> Option<AnyElement> {
        if !self.project_ui.list_open || self.project_ui.explainer {
            return None;
        }
        let theme = self.theme.get();
        let palette = theme.pdf_style();
        let open: HashSet<String> = self
            .tabs
            .iter()
            .filter_map(|t| t.view()?.read(cx).project.link().map(|l| l.id.clone()))
            .collect();
        let rows: Vec<AnyElement> = self
            .projects
            .summaries()
            .into_iter()
            .map(|summary| {
                let link = ProjectLink {
                    id: summary.id.clone(),
                    name: summary.name.clone(),
                    colour: summary.colour,
                };
                let open_id = summary.id.clone();
                let delete_id = summary.id.clone();
                let documents = (self.vault.vault())
                    .map_or(0, |vault| vault.documents_in(Some(&summary.id)).len());
                let names = (self.projects.project(&summary.id))
                    .map_or(0, |project| project.mapping.exported().len());
                div()
                    .id(gpui::SharedString::from(format!(
                        "project-row-{}",
                        summary.id
                    )))
                    .flex()
                    .items_center()
                    .gap_2()
                    .px_2()
                    .py_1()
                    .rounded_md()
                    .hover(move |v| v.bg(palette.placeholder_bg))
                    .child(
                        div()
                            .size(px(8.))
                            .flex_shrink_0()
                            .rounded_full()
                            .bg(style::project_color(link.hue(), theme)),
                    )
                    .child(
                        crate::ui::focus_ring(
                            gpui::SharedString::from(format!("open-project-{}", summary.id)),
                            theme,
                        )
                        .when(cfg!(test), |v| {
                            v.debug_selector(|| "open-project-row".into())
                        })
                        // Enter or Space opens it, as a click does.
                        .key_context("UiControl")
                        .tab_index(0)
                        .tab_stop(true)
                        .role(gpui::Role::Button)
                        .aria_label(format!("Open {}", summary.name))
                        .rounded_sm()
                        .flex_1()
                        .min_w_0()
                        .cursor_pointer()
                        .flex()
                        .flex_col()
                        .child(div().truncate().child(summary.name.clone()))
                        .child(
                            div()
                                .text_size(px(11.))
                                .text_color(palette.header_muted)
                                .child(format!(
                                    "{names} {} · {documents} {}{}",
                                    if names == 1 { "name" } else { "names" },
                                    if documents == 1 {
                                        "document"
                                    } else {
                                        "documents"
                                    },
                                    if open.contains(&summary.id) {
                                        " · open"
                                    } else {
                                        ""
                                    }
                                )),
                        )
                        .on_click(cx.listener(
                            move |this, _, window, cx| this.open_project(&open_id, window, cx),
                        )),
                    )
                    .child(
                        ui::control(
                            gpui::SharedString::from(format!("delete-project-{}", summary.id)),
                            "Delete…",
                            theme,
                            true,
                        )
                        .on_click(cx.listener(
                            move |this, _, window, cx| {
                                this.close_project_list(window, cx);
                                this.show_project_page(
                                    &delete_id,
                                    super::project_page::Place::Delete,
                                    window,
                                    cx,
                                )
                            },
                        )),
                    )
                    .into_any_element()
            })
            .collect();
        let locked = self.vault.needs_key();
        let panel = ui::panel("project-list", theme)
            .when(cfg!(test), |v| v.debug_selector(|| "project-list".into()))
            .track_focus(&self.project_list_focus)
            .key_context("UiPanel UiMenu")
            .on_action(cx.listener(|this, _: &ui::NextControl, window, cx| {
                ui::cycle(window, cx, Some(&this.project_list_focus), false);
                cx.stop_propagation();
            }))
            .on_action(cx.listener(|this, _: &ui::PreviousControl, window, cx| {
                ui::cycle(window, cx, Some(&this.project_list_focus), true);
                cx.stop_propagation();
            }))
            .on_action(cx.listener(|this, _: &ui::CloseMenu, window, cx| {
                this.close_project_list(window, cx);
                cx.stop_propagation();
            }))
            .w(px(420.))
            .max_h(px(480.))
            .overflow_y_scroll()
            .p_2()
            .flex()
            .flex_col()
            .gap_1()
            .on_mouse_down_out(cx.listener(|this, _, window, cx| {
                this.project_ui.list_open = false;
                // Unless the click focused something, focus goes back where
                // the list was opened from.
                let list = this.project_list_focus.clone();
                let back = this.project_list_return.take();
                let weak = cx.entity().downgrade();
                window.defer(cx, move |window, cx| {
                    if list.contains_focused(window, cx) {
                        match back {
                            Some(focus) => window.focus(&focus, cx),
                            None => {
                                let _ = weak.update(cx, |this, cx| this.focus_active(window, cx));
                            }
                        }
                    }
                });
                cx.notify();
            }))
            .child(
                div()
                    .flex()
                    .items_center()
                    .px_2()
                    .pb_1()
                    .child(div().flex_1().child("Open Project"))
                    .child(
                        ui::control("close-project-list", "×", theme, true)
                            .aria_label("Close Project list")
                            .on_click(cx.listener(|this, _, window, cx| {
                                this.close_project_list(window, cx);
                            })),
                    ),
            )
            .when(rows.is_empty() && !locked, |v| {
                v.child(
                    div()
                        .when(cfg!(test), |v| {
                            v.debug_selector(|| "project-list-empty".into())
                        })
                        .px_2()
                        .py_2()
                        .text_size(px(12.))
                        .text_color(palette.header_muted)
                        .child(
                            "No Projects yet. Right-click a tab, or open Share, and choose \
                             Move to New Project.",
                        ),
                )
            })
            .children(rows)
            .when(locked, |v| {
                v.child(
                    div()
                        .when(cfg!(test), |v| {
                            v.debug_selector(|| "project-list-locked".into())
                        })
                        .px_2()
                        .py_2()
                        .text_size(px(12.))
                        .text_color(palette.header_muted)
                        .child("Your documents are locked."),
                )
                .child(
                    ui::control("project-list-unlock", "Unlock…", theme, true)
                        .when(cfg!(test), |v| {
                            v.debug_selector(|| "project-list-unlock".into())
                        })
                        .on_click(
                            cx.listener(|this, _, window, cx| this.unlock_projects(window, cx)),
                        ),
                )
            });
        // Below Home's link when opened from it; centered otherwise.
        let placed = match self.project_ui.list_anchor {
            Some(link) => gpui::anchored()
                .position(gpui::point(link.right(), link.bottom() + px(4.)))
                .anchor(gpui::Anchor::TopRight)
                .snap_to_window_with_margin(px(8.))
                .child(panel)
                .into_any_element(),
            None => div()
                .absolute()
                .inset_0()
                .flex()
                .items_start()
                .justify_center()
                .pt(px(80.))
                .child(panel)
                .into_any_element(),
        };
        Some(gpui::deferred(placed).with_priority(2).into_any_element())
    }
}

/// The renames a member that closed before them has not followed: each
/// earlier alias of an identity, as (old alias, identity now).
fn renames_of(mapping: &Mapping) -> Vec<(String, ExportedIdentity)> {
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
pub(super) fn fold(
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
pub(super) fn keep_fields(stored: &mut Project, project: &Project) {
    stored.name.clone_from(&project.name);
    stored.colour = project.colour;
    stored.pinned = project.pinned;
    stored.mapping.clone_from(&project.mapping);
    stored.lists.clone_from(&project.lists);
    stored.skip.clone_from(&project.skip);
    stored.dismissed_hints.clone_from(&project.dismissed_hints);
}

/// How many of `names` that `text` uses took another alias.
fn count_renumbered(text: &str, names: &[ExportedIdentity], aliases: &[Option<String>]) -> usize {
    let map = RestoreMap::new(names.to_vec());
    let mut seen = HashSet::new();
    (map.tokens(text).into_iter())
        .filter_map(|token| token.identity)
        .filter(|index| seen.insert(*index))
        .filter(|index| aliases[*index].is_some())
        .count()
}
