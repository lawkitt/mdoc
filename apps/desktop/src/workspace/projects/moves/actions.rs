//! A document's moves: into a Project, into a new one or out to Unfiled,
//! and back with Undo; the job on the writer, the members it reloads, and
//! the toast.

use crate::workspace::projects::state::count_renumbered;
use crate::workspace::projects::{MoveToast, text};
use crate::workspace::{
    Workspace,
    saving::autosave::{Op, Ran, Write},
    saving::document_writer::Target,
};
use gpui::{Context, Window};
use mdoc_pii::restore::ExportedIdentity;
use mdoc_store::document_id::{DocumentId, VaultId};
use mdoc_vault::{MoveUndo, Moved, NewProject, ProjectDocuments, VaultError};
use std::time::Duration;

/// How long the move toast stays, with its Undo.
pub(in crate::workspace::projects) const TOAST: Duration = Duration::from_secs(10);

/// What a move shows in its toast, and whether it can be undone.
pub(in crate::workspace::projects) enum Moving {
    Into,
    New,
    Undo,
}

impl Workspace {
    /// Move tab `id`'s document into Project `to`, or Unfiled for `None`,
    /// once its last text is written.
    pub(in crate::workspace) fn move_tab(
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
        self.queue_op(document, Op::Move { to }, window, cx);
    }

    /// Move a document by id, open or not: the Project page's Remove.
    pub(in crate::workspace) fn move_document(
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
    pub(in crate::workspace) fn move_to_new_project(
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
    pub(in crate::workspace) fn move_document_to_new_project(
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
    pub(in crate::workspace) fn undo_move(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let Some((undo, made)) = self.project_ui.toast.take().and_then(|toast| toast.undo) else {
            return;
        };
        let Some(document) = VaultId::parse(&undo.document) else {
            return;
        };
        // The Project the move made goes: its name field closes unsaved, and
        // one still waiting for the reload never opens.
        if (self.project_ui.renaming.as_ref()).is_some_and(|(id, _)| Some(id) == made.as_ref()) {
            self.finish_rename(false, window, cx);
        }
        self.project_ui.naming.remove(&document);
        self.queue_op(document, Op::Undo { undo, made }, window, cx);
        cx.notify();
    }

    /// Run `op` once the jobs before it are done. What its tab shows is
    /// read when it runs (ADR 0130); until it has, the document counts as
    /// moving, so no second move or delete of it is offered. A quit is the
    /// last job: nothing queues after it.
    pub(in crate::workspace::projects) fn queue_op(
        &mut self,
        document: VaultId,
        op: Op,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if self.quit_flow.quitting() {
            return;
        }
        self.moving.insert(document.clone());
        self.next_op += 1;
        let target = Target::Op(self.next_op);
        self.queue_write(target, Write::Op(document, op), window, cx);
    }

    /// Run a move at the head of the writer's queue: nothing else writes
    /// meanwhile. What its tab shows is written first, its names before its
    /// text, then its Projects; the moved document's tab loads again, and
    /// the job ends once it has.
    pub(in crate::workspace) fn run_op(
        &mut self,
        document: VaultId,
        op: Op,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Ran {
        // An Ask waiting for this move: it asks only once the move landed.
        let ask = self.ai_ask.moved(document.as_str());
        let Some(vault) = self.vault.vault().cloned() else {
            self.moving.remove(&document);
            return Ran::Ended;
        };
        let id = document.as_str();
        // What its tab shows now, typed while the move waited included, is
        // what moves (ADR 0130). A name applied meanwhile joins the source
        // Project's names first: reconciling an alias the Project retired
        // since rewrites the text, which is then written as it shows, and
        // `write_now` writes those names before the move reads them. If any
        // of it fails, nothing moves.
        let tab = self.find_document(&DocumentId::Vault(document.clone()), None, cx);
        if let Some(tab) = tab
            && !self.broken_order()
            && let Err(error) = self.write_tab_now(tab, &vault, cx)
        {
            self.moving.remove(&document);
            let title =
                (self.tab_view(tab)).map_or_else(String::new, |v| v.read(cx).display_name());
            self.notices
                .post(text::could_not_move_curly(&title, &error));
            cx.notify();
            return Ran::Ended;
        }
        let from = self.project_of(id);
        let before = vault.document(id).ok();
        let title = before.as_ref().map(|d| d.title.clone()).unwrap_or_default();
        // The Projects it writes hold every open member's names, a change
        // still waiting for its fold included: the move numbers the
        // document's names against all of them (ADR 0130).
        let places = match &op {
            Op::Move { to } => vec![from.clone(), to.clone()],
            Op::Undo { undo, .. } => vec![from.clone(), undo.project.clone()],
            _ => vec![from.clone()],
        };
        if let Err(error) = self.fold_members(&places, cx) {
            self.moving.remove(&document);
            self.notices.post(text::move_names_not_kept(&title, &error));
            cx.notify();
            return Ran::Ended;
        }
        // The Project an Undo deleted with the move, and whether its page
        // shows.
        let mut gone = None;
        let mut leave_page = false;
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
                        // Its categories start from Settings › Detection, and
                        // the hints it dismissed stay dismissed: written with
                        // the move, so nothing can fail once it is in.
                        vault.move_document_to_new_project_with(id, new, |project| {
                            project.skip = skip;
                            project.dismissed_hints = dismissed;
                        })
                    });
                let made = (result.as_ref().ok())
                    .and_then(|moved| moved.project.as_ref().map(|p| p.id.clone()));
                (result, Moving::New, made)
            }
            Op::Undo { undo, made } => {
                let result = self
                    .write_now(&vault, &[from.clone(), undo.project.clone()])
                    .and_then(|()| vault.undo_move(&undo));
                // A Project the move made goes with the undo, once empty:
                // from the window too (#1337). A delete that failed after
                // its record went counts as done.
                if result.is_ok()
                    && let Some(made) = &made
                    && vault.documents_in(Some(made)).is_empty()
                    && (vault.delete_project(made, ProjectDocuments::Delete).is_ok()
                        || matches!(vault.project(made), Err(VaultError::NotFound { .. })))
                {
                    leave_page =
                        (self.project_ui.page.as_ref()).is_some_and(|page| page.id() == made);
                    self.project_gone(made, cx);
                    gone = Some(made.clone());
                }
                (result, Moving::Undo, None)
            }
            Op::Delete { .. } | Op::DeleteDocuments { .. } => {
                unreachable!("deletes run as their own step")
            }
        };
        match result {
            Ok(moved) => {
                // The order the rule forbids, for the test that must catch
                // it: the tab is read after the move read the Vault.
                if let Some(tab) = tab
                    && self.broken_order()
                {
                    let _ = self.write_tab_now(tab, &vault, cx);
                }
                let (names, aliases) = (before.as_ref())
                    .and_then(|before| self.alias_changes(before, &moved))
                    .unwrap_or_default();
                let renumbered = before.as_ref().map_or(0, |before| {
                    count_renumbered(&before.markdown, &names, &aliases)
                });
                self.took_move(&moved, gone.as_deref());
                self.sync_members(&moved, &document, cx);
                // The Project an Ask waited for: the one made, or the
                // shared one it moved into (#1312).
                let (into, shared) = match moving {
                    Moving::Into => (moved.project.as_ref().map(|p| p.id.clone()), true),
                    _ => (made.clone(), false),
                };
                let asked = ask
                    .zip(into)
                    .map(|(with, into)| (with, into, id.to_string(), shared));
                // The new Project's name is ready to edit once its tab shows
                // the moved document (#1313).
                // Another move of the same document since forgets it.
                match (&moving, &made) {
                    (Moving::New, Some(made)) => self
                        .project_ui
                        .naming
                        .insert(document.clone(), made.clone()),
                    _ => self.project_ui.naming.remove(&document),
                };
                let tab = self.find_document(&DocumentId::Vault(document.clone()), None, cx);
                match tab {
                    Some(tab) => {
                        // A reply pasted in the tab names the people it did.
                        if let Some(view) = self.tab_view(tab) {
                            view.update(cx, |view, cx| view.realias_reply(names, &aliases, cx));
                        }
                        self.reload_tab(tab, document.clone(), window, cx)
                    }
                    None => {
                        self.moving.remove(&document);
                    }
                }
                // The deleted Project's page gives way to the document's tab,
                // else Home, taking its Back/Forward stop. After the reload
                // started, so a tab not loaded yet loads once; a dialog or the
                // switcher keeps the keyboard (#1337).
                if leave_page {
                    let keep = (self.dialog_up(window) || self.switcher.is_some())
                        .then(|| window.focused(cx))
                        .flatten();
                    let arrival = crate::workspace::sidebar::navigation::Arrival::Replace;
                    match tab {
                        Some(tab) => self.activate_as(tab, arrival, window, cx),
                        None => self.show_home_as(arrival, window, cx),
                    }
                    if let Some(keep) = keep {
                        window.focus(&keep, cx);
                    }
                }
                if !self.reloading.contains_key(&document) {
                    let document = document.clone();
                    cx.defer_in(window, move |this, window, cx| {
                        this.name_moved_project(None, &document, window, cx)
                    });
                }
                let place = match &moved.project {
                    Some(project) => text::quoted_name(&project.name),
                    None => text::UNFILED.to_string(),
                };
                let text = match moving {
                    Moving::Undo => text::moved_back(&title, &place),
                    _ => text::moved(&title, &place, renumbered),
                };
                let undo = (!matches!(moving, Moving::Undo)).then(|| (moved.undo.clone(), made));
                self.show_toast(text, undo, window, cx);
                // "Move to a new Project and share" asks now. A move into
                // the shared Project asks only while it still is: a share
                // made since the click is never undone (#1312).
                if let Some((with, made, document, shared)) = asked
                    && (!shared || self.ai_shared().as_deref() == Some(made.as_str()))
                {
                    self.ask_after_move(&document, &made, with, cx);
                }
            }
            Err(error) => {
                self.moving.remove(&document);
                self.notices.post(text::could_not_move(&title, &error));
                if let Some(tab) = self.find_document(&DocumentId::Vault(document), None, cx) {
                    self.autosave(tab, window, cx);
                }
            }
        }
        self.checkpoint(window, cx);
        self.refresh_saved_replies(cx);
        self.refresh_reply_page(cx);
        cx.notify();
        self.hold_for_reloads()
    }

    /// Fold the names of every open member of `projects` whose change waits
    /// for its fold, for a job about to write those Projects.
    pub(in crate::workspace) fn fold_members(
        &mut self,
        projects: &[Option<String>],
        cx: &mut Context<Self>,
    ) -> Result<(), String> {
        let tabs: Vec<u64> = (self.tabs.iter())
            .filter(|tab| {
                tab.view().is_some_and(|view| {
                    let view = view.read(cx);
                    view.project_save_pending()
                        && (view.project.link())
                            .is_some_and(|link| projects.iter().flatten().any(|p| *p == link.id))
                })
            })
            .map(|tab| tab.id())
            .collect();
        for tab in tabs {
            if let Some(view) = self.tab_view(tab) {
                view.update(cx, |view, _| view.take_project_save());
            }
            self.fold_into_project(tab, cx)?;
        }
        Ok(())
    }

    /// A job that started reloads holds the writer until they end: no
    /// later job reads a view a reload is about to replace (ADR 0130).
    pub(in crate::workspace) fn hold_for_reloads(&mut self) -> Ran {
        if self.reloading.is_empty() {
            return Ran::Ended;
        }
        self.reloads_hold_writer = true;
        Ran::Holds
    }

    /// Tests only: run moves in the order ADR 0130 forbids, reading the tab
    /// after the move read the Vault, so the interleaving test can show it
    /// catches that.
    pub(in crate::workspace::projects) fn broken_order(&self) -> bool {
        #[cfg(test)]
        return self.broken_order;
        #[cfg(not(test))]
        false
    }

    /// The Projects a move wrote, in this window.
    /// The Project it left is not taken back when the move deleted it:
    /// `gone`, the one Undo took with it (#1337).
    pub(in crate::workspace::projects) fn took_move(&mut self, moved: &Moved, gone: Option<&str>) {
        for project in moved.project.iter().chain(&moved.left) {
            if Some(project.id.as_str()) != gone {
                self.projects.moved(project.clone());
            }
        }
    }

    /// Project `id` was deleted: the window forgets it, its Back/Forward
    /// stop, its AI share and its row's state. A page showing it is left
    /// first, by the caller.
    pub(in crate::workspace::projects) fn project_gone(
        &mut self,
        id: &str,
        cx: &mut Context<Self>,
    ) {
        self.navigation
            .forget(&crate::workspace::sidebar::navigation::Stop::Project(
                id.to_string(),
            ));
        self.projects.remove(id);
        self.ai_project_gone(id, cx);
        self.tree.forget_project(id);
    }

    /// The other open members of the Projects a move wrote take their names
    /// as they are now: a name the move brought is known, and its alias is
    /// taken, before any of them can hand that alias to someone else.
    pub(in crate::workspace::projects) fn sync_members(
        &mut self,
        moved: &Moved,
        document: &VaultId,
        cx: &mut Context<Self>,
    ) {
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
    pub(in crate::workspace::projects) fn alias_changes(
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
    pub(in crate::workspace::projects) fn reload_tab(
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
        match self.load_document(id, DocumentId::Vault(document.clone()), record, window, cx) {
            Some(generation) => {
                self.reloading.insert(document, generation);
            }
            None => {
                self.moving.remove(&document);
            }
        }
    }

    pub(in crate::workspace::projects) fn show_toast(
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

    pub(in crate::workspace) fn dismiss_toast(&mut self, cx: &mut Context<Self>) {
        self.project_ui.toast = None;
        cx.notify();
    }

    /// A load of `document` ends, done or with its tab gone. When it is a
    /// move's reload (its `generation`, not an older load's), and the last
    /// of its job's reloads, the next job may read the view once it holds
    /// the moved text.
    pub(in crate::workspace) fn move_reload_ended(
        &mut self,
        document: &mdoc_store::document_id::VaultId,
        generation: u64,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if self.reloading.get(document) != Some(&generation) {
            return;
        }
        self.reloading.remove(document);
        self.moving.remove(document);
        if self.reloading.is_empty() && std::mem::take(&mut self.reloads_hold_writer) {
            cx.defer_in(window, |this, window, cx| this.advance_writer(window, cx));
        }
    }
}
