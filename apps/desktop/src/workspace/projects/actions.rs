//! Projects' actions: the key that opens them, loading them and their
//! members' names, the writes, rename, categories and lists, and the Open
//! Project list's.

use super::state::{fold, keep_fields, renames_of};
use super::text;
use crate::workspace::{
    Workspace, projects::ProjectLink, saving::autosave::Write, saving::document_writer::Target,
};
use crate::{document::DocumentView, document::markdown_search};
use gpui::{App, Context, Entity, Focusable, Window, prelude::*};
use mdoc_pii::restore::{ExportedIdentity, RestoreMap};
use mdoc_store::{document_id::VaultId, lists::Lists};
use mdoc_vault::{Project, Vault, VaultError};
use std::collections::HashSet;

impl Workspace {
    /// True when the Vault is open or can't be. While its key is locked,
    /// the Keychain explainer opens and `then` runs after its Continue.
    pub(in crate::workspace) fn vault_key_ready(
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
    pub(in crate::workspace) fn unlock_projects(
        &mut self,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if self.vault.needs_key() {
            self.project_ui.explainer = true;
            self.explainer_focus_pending = true;
            cx.notify();
        }
    }

    /// The explainer's Continue: read the key (macOS may ask now), open the
    /// Vault, then run the waiting action.
    pub(in crate::workspace) fn continue_unlock(
        &mut self,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
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
    pub(in crate::workspace) fn cancel_unlock(
        &mut self,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.close_explainer(window, cx);
        self.project_ui.then = None;
        self.folder_key_cancelled();
        cx.notify();
    }

    /// Hide the explainer and give focus back to where it was.
    pub(super) fn close_explainer(&mut self, window: &mut Window, cx: &mut Context<Self>) {
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

    /// Read every Project and the user-wide lists, once the Vault is open,
    /// and give loaded member tabs their mapping.
    pub(in crate::workspace) fn load_projects(&mut self, cx: &mut Context<Self>) {
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
            self.notices.post(text::projects_not_opened(failed));
        }
        let ids: Vec<_> = self.tabs.iter().map(|t| t.id()).collect();
        for id in ids {
            self.attach_project(id, cx);
        }
        self.refresh_saved_replies(cx);
        cx.notify();
    }

    /// Tab `id`'s Vault document, once its view holds it.
    pub(in crate::workspace) fn tab_document(&self, id: u64, cx: &App) -> Option<VaultId> {
        let view = self.tabs.get(id)?.view()?;
        view.read(cx).session.document.vault.clone()
    }

    /// The Project the Vault files document `id` in, from its summary.
    pub(in crate::workspace) fn project_of(&self, id: &str) -> Option<String> {
        let vault = self.vault.vault()?;
        (vault.documents().into_iter())
            .find(|summary| summary.id == id)
            .and_then(|summary| summary.project)
    }

    /// After a document loads, give it its Project's mapping.
    pub(in crate::workspace) fn attach_project(&mut self, id: u64, cx: &mut Context<Self>) {
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
                // A member moved mid-edit opens with its review cleared.
                view.owe_if_left_mid_edit(cx);
            }
        });
        // Loaded in the AI project: it entered it (#1164 Decision 5).
        self.scan_on_entry(cx);
        cx.notify();
    }

    pub(in crate::workspace) fn tab_view(&self, id: u64) -> Option<Entity<DocumentView>> {
        self.tabs.get(id)?.view().cloned()
    }

    /// Whether tab `id` can move: a loaded Vault document, converted, and
    /// no move of it waits. An unconverted source keeps its document as
    /// admitted until it converts.
    pub(in crate::workspace) fn can_move(&self, id: u64, cx: &App) -> bool {
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
    pub(in crate::workspace) fn project_changed(&mut self, id: u64, cx: &mut Context<Self>) {
        if let Err(error) = self.fold_into_project(id, cx) {
            self.notices.post(text::names_not_kept(&error));
        }
    }

    /// [`Self::project_changed`]; an error when an alias the Project has
    /// retired could not be reconciled, and nothing was folded.
    pub(in crate::workspace) fn fold_into_project(
        &mut self,
        id: u64,
        cx: &mut Context<Self>,
    ) -> Result<(), String> {
        let Some(view) = self.tab_view(id) else {
            return Ok(());
        };
        let Some(member) = self.tab_document(id, cx) else {
            return Ok(());
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
                view.update(cx, |view, cx| {
                    view.join_project_identities(&identities, &reserved, cx)
                })
                .map_err(|error| error.to_string())?;
            }
        }
        let (link, identities, not_names, kept) = {
            let v = view.read(cx);
            let Some(link) = v.project.link().cloned() else {
                return Ok(());
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
            return Ok(());
        };
        view.update(cx, |view, _| view.project.set_names(exported.clone()));
        self.write_project(&link.id, cx);
        if let Some(lists) = lists {
            self.project_lists_changed(&link.id, &lists, cx);
        }
        // Every open member knows the Project's names now, a new wording of
        // an identity included: a hand copy of it is marked and the shelf
        // counts it as written out (#1209).
        for tab in &self.tabs {
            if let Some(other) = tab.view()
                && tab.id() != id
                && other.read(cx).project.is(&link.id)
            {
                other.update(cx, |other, _| other.project.set_names(exported.clone()));
            }
        }
        if renames.is_empty() && !added {
            return Ok(());
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
        Ok(())
    }

    /// The aliases of `project`'s identities that its members' saved text
    /// restores, `member`'s own included; a member that can't be read
    /// counts as needing them all.
    pub(super) fn restored_in_members(&self, project: &Project, member: &str) -> HashSet<String> {
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
    pub(in crate::workspace) fn add_project_term(
        &mut self,
        id: u64,
        term: &str,
        cx: &mut Context<Self>,
    ) {
        let Some(link) = (self.tab_view(id)).and_then(|view| view.read(cx).project.link().cloned())
        else {
            return;
        };
        self.add_project_name(&link.id, term, cx);
    }

    /// A member dismissed a merge hint: the Project keeps it and its other
    /// open members hide it too.
    pub(in crate::workspace) fn dismiss_project_hint(
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

    pub(in crate::workspace) fn add_project_name(
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

    /// Save Project `id` through the writer, as it is when its write runs.
    pub(in crate::workspace) fn write_project(&mut self, id: &str, cx: &mut Context<Self>) {
        // A reply on show restores with the names as they are now.
        self.refresh_reply_page(cx);
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
    pub(in crate::workspace) fn queue_project_writes(
        &mut self,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let ids: Vec<_> = self.project_ui.unwritten.drain().collect();
        for id in ids {
            self.queue_write(Target::Project(id), Write::Project, window, cx);
        }
    }

    /// Write the Projects a move reads, as this window has them, before it
    /// runs: a move then never loses a change waiting to be written.
    pub(in crate::workspace) fn write_now(
        &mut self,
        vault: &Vault,
        ids: &[Option<String>],
    ) -> Result<(), VaultError> {
        for id in ids.iter().flatten() {
            if let Some(project) = self.projects.project(id).cloned() {
                vault.update_project(id, |stored| keep_fields(stored, &project))?;
            }
        }
        Ok(())
    }

    pub(in crate::workspace) fn start_rename_project(
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
                .with_placeholder(text::PROJECT_NAME);
            input.set_value(name, cx);
            input
        });
        let focus = input.read(cx).focus_handle(cx);
        window.focus(&focus, cx);
        // Leaving the field saves, like Enter (Escape still cancels). The
        // header's field, opened by Move to New Project, stays open while
        // another app is in front: Ask's app comes forward right after it.
        self.project_ui.rename_blur =
            Some(cx.on_focus_out(&focus, window, |this, _, window, cx| {
                let header = (this.tabs.iter()).any(|tab| {
                    tab.view()
                        .is_some_and(|v| v.read(cx).project_name_field.is_some())
                });
                if header && !window.is_window_active() {
                    return;
                }
                this.finish_rename(true, window, cx)
            }));
        self.project_ui.renaming = Some((project_id, input));
        cx.notify();
    }

    pub(in crate::workspace) fn finish_rename(
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
    pub(in crate::workspace) fn end_rename(
        &mut self,
        save: bool,
        cx: &mut Context<Self>,
    ) -> Option<Entity<markdown_search::SearchInput>> {
        let (project_id, input) = self.project_ui.renaming.take()?;
        self.project_ui.rename_blur = None;
        for view in self.tabs.iter().filter_map(|t| t.view().cloned()) {
            if view.read(cx).project_name_field.is_some() {
                view.update(cx, |view, cx| {
                    view.project_name_field = None;
                    cx.notify();
                });
            }
        }
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

    /// Move to New Project's document is shown again (tab `tab`, once it
    /// reloaded) or not live: the Project's name is ready to edit, the
    /// document's name selected. In the header's breadcrumb when its tab is
    /// in view; otherwise on the Project's page.
    pub(in crate::workspace) fn name_moved_project(
        &mut self,
        tab: Option<u64>,
        document: &VaultId,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let Some(project) = self.project_ui.naming.remove(document) else {
            return;
        };
        // A dialog or the recency switcher keeps the keyboard: the name
        // stays the document's, Rename on the Project's page still renames.
        if self.projects.project(&project).is_none()
            || self.quit_flow.quitting()
            || self.project_ui.renaming.is_some()
            || self.dialog_up(window)
            || self.switcher.is_some()
        {
            return;
        }
        let shown = (tab.filter(|tab| *tab == self.tabs.active() && !self.showing_overview(cx)))
            .and_then(|tab| self.tab_view(tab))
            .filter(|view| view.read(cx).project.is(&project));
        match shown {
            Some(view) => {
                self.start_rename_project(project, window, cx);
                let input = (self.project_ui.renaming.as_ref()).map(|(_, input)| input.clone());
                view.update(cx, |view, cx| {
                    view.project_name_field = input;
                    cx.notify();
                });
            }
            None => self.show_project_page(
                &project,
                crate::workspace::projects::project_page::Place::Rename,
                window,
                cx,
            ),
        }
    }

    /// Show the Project page.
    pub(in crate::workspace) fn open_project(
        &mut self,
        project_id: &str,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.show_project_page(
            project_id,
            crate::workspace::projects::project_page::Place::Header,
            window,
            cx,
        );
    }

    pub(in crate::workspace) fn show_project_list(
        &mut self,
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
                this.show_project_list(window, cx);
            }));
            return;
        }
        self.load_projects(cx);
        self.project_ui.list_open = true;
        self.project_list_return = window.focused(cx).filter(|f| *f != self.project_list_focus);
        // After the palette, if it ran this, has restored its own focus.
        let focus = self.project_list_focus.clone();
        window.defer(cx, move |window, cx| window.focus(&focus, cx));
        cx.notify();
    }

    /// Close the Open Project list, focus back where it was opened from.
    pub(in crate::workspace) fn close_project_list(
        &mut self,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.project_ui.list_open = false;
        match self.project_list_return.take() {
            Some(focus) => window.focus(&focus, cx),
            None => self.focus_active(window, cx),
        }
        cx.notify();
    }

    /// Switch a category on or off for a Project and its open members.
    pub(in crate::workspace) fn toggle_project_category(
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
    pub(in crate::workspace) fn project_lists_changed(
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
    pub(in crate::workspace) fn remove_project_list_entry(
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

    /// How many documents Project `id` holds, for the Open Project list.
    pub(super) fn project_document_count(&self, id: &str) -> usize {
        (self.vault.vault()).map_or(0, |vault| vault.documents_in(Some(id)).len())
    }

    /// The Vault waits for its key, so the Open Project list offers Unlock.
    pub(super) fn projects_locked(&self) -> bool {
        self.vault.needs_key()
    }
}
