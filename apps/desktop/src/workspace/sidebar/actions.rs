//! The sidebar's actions: the reveal, dropped files, the resize, and what the
//! tree, navigation and the switcher carry out.
use super::state::{REVEAL_SLIDE, SLIDE, SidebarEffect};
use super::tree_ui::TreeNow;
use crate::workspace::projects::project_page::Place;
use crate::workspace::sidebar::switcher::Switcher;
use crate::workspace::{REVEAL_DELAY, REVEAL_GRACE, Workspace, activate_for_drop};
use crate::workspace::{
    Tab,
    saving::autosave::{Op, Write},
    saving::document_writer::Target,
    saving::live_views::{LIMIT, Weight},
    sidebar::navigation::{Arrival, Stop},
    sidebar::tree::{Row, RowKey, Shown, TreeEffect},
};
use gpui::PromptLevel;
use gpui::{App, Context, Window};
use mdoc_store::document_id::{DocumentId, VaultId};
use std::time::Duration;

impl Workspace {
    /// Track the pointer over the collapsed rail or its reveal.
    pub(in crate::workspace) fn hover_sidebar(
        &mut self,
        panel: bool,
        hovered: bool,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let effect = self.sidebar.hover(panel, hovered);
        self.carry_out_sidebar(effect, window, cx);
    }
    pub(in crate::workspace) fn carry_out_sidebar(
        &mut self,
        effect: Option<SidebarEffect>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        match effect {
            None => {}
            Some(SidebarEffect::Repaint) => cx.notify(),
            Some(SidebarEffect::EndSlide) => {
                cx.spawn(async move |this, cx| {
                    cx.background_executor()
                        .timer(SLIDE + Duration::from_millis(16))
                        .await;
                    let _ = this.update(cx, |_, cx| cx.notify());
                })
                .detach();
            }
            Some(SidebarEffect::Schedule { open, ticket }) => {
                let delay = if open { REVEAL_DELAY } else { REVEAL_GRACE };
                cx.spawn_in(window, async move |this, cx| {
                    cx.background_executor().timer(delay).await;
                    let _ = this.update_in(cx, |this, window, cx| {
                        if !this.sidebar.fire(open, ticket) {
                            return;
                        }
                        if open {
                            this.show_reveal(false, cx);
                        } else if this.reveal_held(window, cx) {
                            let effect = this.sidebar.schedule(false);
                            this.carry_out_sidebar(Some(effect), window, cx);
                        } else {
                            let focused = this.reveal_focus.contains_focused(window, cx);
                            this.dismiss_reveal(window, cx);
                            if focused {
                                this.focus_active(window, cx);
                            }
                        }
                    });
                })
                .detach();
            }
            Some(SidebarEffect::FinishHiding(generation)) => {
                cx.spawn_in(window, async move |this, cx| {
                    cx.background_executor().timer(REVEAL_SLIDE).await;
                    let _ = this.update_in(cx, |this, window, cx| {
                        let effect = this.sidebar.finish_hiding(generation);
                        this.carry_out_sidebar(effect, window, cx);
                    });
                })
                .detach();
                cx.notify();
            }
        }
    }
    /// A tab's or a Project's context menu is open. The sidebar then shows no
    /// tooltips (#310): GPUI keeps a shown tooltip while the pointer rests
    /// on it, draws it over the menu, and shows a new one when the pointer
    /// slips off the menu onto a row. Rendering a control without its
    /// tooltip is GPUI's one way to dismiss it.
    pub(crate) fn context_menu_open(&self) -> bool {
        self.document_menu.is_some() || self.project_ui.header_menu.is_some()
    }
    /// A context menu, drag, the pointer, or keyboard focus keeps the reveal open.
    pub(in crate::workspace) fn reveal_held(&self, window: &Window, cx: &App) -> bool {
        self.document_menu.is_some()
            || cx.has_active_drag()
            || self
                .sidebar
                .holds(|| self.reveal_focus.contains_focused(window, cx))
    }
    pub(in crate::workspace) fn show_reveal(&mut self, keyboard: bool, cx: &mut Context<Self>) {
        self.sidebar.show(keyboard);
        cx.notify();
    }
    /// A reveal slide that started ends after `REVEAL_SLIDE` with a repaint,
    /// so the document beside it is drawn afresh, uncached.
    pub(in crate::workspace) fn end_reveal_slide_later(&mut self, cx: &mut Context<Self>) {
        let Some(generation) = self.sidebar.reveal_slide_to_schedule() else {
            return;
        };
        cx.spawn(async move |this, cx| {
            cx.background_executor().timer(REVEAL_SLIDE).await;
            let _ = this.update(cx, |this, cx| {
                if this.sidebar.end_reveal_slide(generation) {
                    cx.notify();
                }
            });
        })
        .detach();
    }
    /// Slide the reveal back; it unmounts once the animation has finished.
    pub(in crate::workspace) fn dismiss_reveal(
        &mut self,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let effect = self.sidebar.dismiss(cx.reduce_motion());
        self.carry_out_sidebar(effect, window, cx);
    }
    /// Keyboard focus on a rail control reveals the full list at once and moves
    /// focus to the matching control there; focus leaving the reveal closes it.
    pub(in crate::workspace) fn reveal_for_keyboard(
        &mut self,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if self.sidebar.visible() {
            return;
        }
        let may_open = self
            .sidebar
            .keyboard_may_open(|focus| focus.is_focused(window));
        let focused = [
            (&self.rail_toggle_focus, &self.panel_toggle_focus),
            (&self.rail_new_focus, &self.panel_new_focus),
            (&self.rail_home_focus, &self.panel_home_focus),
        ]
        .into_iter()
        .find(|(rail, _)| rail.is_focused(window))
        .map(|(rail, panel)| (rail.clone(), panel.clone()));
        match focused {
            Some((rail, panel)) if may_open && window.last_input_was_keyboard() => {
                self.sidebar.open_for_keyboard(rail);
                cx.notify();
                window.defer(cx, move |window, cx| window.focus(&panel, cx));
            }
            None if self.sidebar.keyboard_reveal() && !self.reveal_held(window, cx) => {
                self.dismiss_reveal(window, cx);
            }
            _ => {}
        }
    }
    pub(in crate::workspace) fn clear_file_drag(
        &mut self,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let effect = self.sidebar.clear_drag();
        self.carry_out_sidebar(effect, window, cx);
        cx.notify();
    }
    /// Import OS files dropped on the tree or the rail into Unfiled.
    pub(in crate::workspace) fn drop_files(
        &mut self,
        paths: &gpui::ExternalPaths,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.clear_file_drag(window, cx);
        // A drop from Finder leaves mdoc in the background; what it opens
        // (a dialog among them) needs the keyboard.
        activate_for_drop(window, cx);
        if !self.quit_flow.quitting() {
            self.open_paths(paths.paths().to_vec(), window, cx);
        }
    }
    /// OS files dropped on a Project header: they are imported into that
    /// Project, and a folder among them offers Add to it.
    pub(in crate::workspace) fn drop_on_project(
        &mut self,
        paths: &gpui::ExternalPaths,
        project: &str,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.clear_file_drag(window, cx);
        activate_for_drop(window, cx);
        if !self.quit_flow.quitting() {
            let paths = paths.paths().to_vec();
            self.open_paths_into(paths, Some(project.to_string()), window, cx);
        }
    }
    /// The sidebar's edge drag ended: keep the width it left in settings.
    pub(in crate::workspace) fn end_sidebar_resize(&mut self, cx: &mut Context<Self>) {
        if let Some(width) = self.sidebar.end_resize() {
            // Kept for this window when the save fails; the error is logged.
            let _ = self.update_general(move |general| general.sidebar_width = Some(width), cx);
        }
        cx.notify();
    }
}

impl Workspace {
    // ---- What the tree shows ------------------------------------------------

    /// The tree as it stands: the window's Projects and the Vault's
    /// documents, none while the Vault is closed.
    pub(in crate::workspace) fn tree_now(&self) -> TreeNow {
        let (projects, documents) = match self.vault.vault() {
            Some(vault) => (self.projects.summaries(), vault.documents()),
            None => (Vec::new(), Vec::new()),
        };
        let rows = self.tree.rows(&projects, &documents);
        TreeNow {
            projects,
            documents,
            rows,
        }
    }
    /// The Vault document live view `tab` holds, loaded or named.
    pub(in crate::workspace) fn live_document(&self, tab: &Tab, cx: &App) -> Option<VaultId> {
        if let Some(view) = tab.view()
            && let Some(DocumentId::Vault(id)) = view.read(cx).session.document.id()
        {
            return Some(id);
        }
        tab.record().document.clone()
    }
    /// What the window shows, as the tree marks it.
    pub(in crate::workspace) fn tree_shown(&self, cx: &App) -> Shown {
        if let Some(page) = &self.project_ui.page {
            return Shown::ProjectPage(page.id().to_string());
        }
        if self.showing_home(cx) {
            return Shown::Nothing;
        }
        match self
            .tabs
            .active_tab()
            .and_then(|tab| self.live_document(tab, cx))
        {
            Some(id) => Shown::Document(id.as_str().to_string()),
            None => Shown::Nothing,
        }
    }
    /// The tree's rows as keys, in order: what tests and Tab walks compare.
    #[cfg(test)]
    pub(in crate::workspace) fn tree_keys(&self) -> Vec<RowKey> {
        self.tree_now().rows.iter().filter_map(Row::key).collect()
    }
    // ---- Opening and evicting -----------------------------------------------

    /// A document row was clicked: its live view, or a new one, shows.
    pub(in crate::workspace) fn open_from_tree(
        &mut self,
        document: VaultId,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if self.quit_flow.quitting() {
            return;
        }
        self.tree
            .set_cursor(Some(RowKey::Document(document.as_str().to_string())));
        self.open_vault_document(document, window, cx);
    }
    /// How eviction weighs live view `tab`: Home's placeholder and a view
    /// already leaving don't count; a view that works, prompts, waits for a
    /// write or holds text the Vault doesn't (a New document before its first
    /// write) stays.
    fn eviction_weight(&self, tab: &Tab, cx: &App) -> Weight {
        if self.is_placeholder(tab, cx) || self.closing_after_write.contains(&tab.id()) {
            return Weight::Ignored;
        }
        let Some(view) = tab.view() else {
            // Never shown since launch: nothing to lose.
            return Weight::Idle;
        };
        let document = self.live_document(tab, cx);
        let busy = {
            let view = view.read(cx);
            view.import.busy()
                || view.lifecycle.loading()
                || view.lifecycle.prompting()
                || view.import.password_prompt().is_some()
                // Text no document holds yet: a New one before its first
                // write.
                || (document.is_none() && !view.editor.read(cx).text().is_empty())
        };
        if busy
            || document.as_ref().is_some_and(|d| self.moving.contains(d))
            || self.unsaved.contains(&tab.id())
            || self.write_pending_for(tab.id(), cx)
            // The companion's Replace all runs in it.
            || self.ai.replacing_in() == Some(tab.id())
        {
            Weight::Busy
        } else {
            Weight::Idle
        }
    }
    /// A New document left before its first keystroke goes: it never became
    /// a document, so nothing names it. One typed into and emptied again
    /// stays, as does Home's placeholder.
    pub(in crate::workspace) fn drop_if_empty_new(
        &mut self,
        id: u64,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if id == self.tabs.active() && !self.tabs.home() && self.project_ui.page.is_none() {
            return;
        }
        let Some(tab) = self.tabs.get(id) else {
            return;
        };
        let Some(view) = tab.view() else {
            return;
        };
        if tab.record().document.is_some() || self.is_placeholder(tab, cx) {
            return;
        }
        let empty = {
            let view = view.read(cx);
            view.session.document.id().is_none()
                && view.preview.source().is_none()
                && !view.lifecycle.loading()
                && !view.lifecycle.source_only()
                && view.editor.read(cx).text().is_empty()
                && view.lifecycle.edited_at().is_none()
        };
        if empty && !self.write_pending_for(id, cx) {
            self.remove(id, window, cx);
        }
    }
    /// Release the least recently viewed views beyond the live limit, each
    /// once its last text is written.
    pub(in crate::workspace) fn evict(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.quit_flow.quitting() || !self.ready {
            return;
        }
        let evicted = self
            .tabs
            .evictable(LIMIT, |tab| self.eviction_weight(tab, cx));
        for id in evicted {
            self.evicting.insert(id);
            self.close_tab(id, window, cx);
        }
    }
    /// The row whose focus handle holds focus.
    pub(in crate::workspace) fn focused_tree_row(&self, window: &Window) -> Option<RowKey> {
        self.tree_row_focus
            .borrow()
            .iter()
            .find(|(_, focus)| focus.is_focused(window))
            .map(|(key, _)| key.clone())
    }
    /// Where F6 enters the tree: the selected row, else the first.
    pub(in crate::workspace) fn tree_landing(&self, cx: &mut App) -> Option<gpui::FocusHandle> {
        let now = self.tree_now();
        let selected = self
            .tree
            .selected(&now.rows, &self.tree_shown(cx), &now.documents);
        let stop = self.tree.stop(&now.rows, selected.as_ref())?;
        Some(self.tree_focus(&stop, cx))
    }
    fn carry_out_tree(
        &mut self,
        effect: Option<TreeEffect>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        match effect {
            None => {}
            Some(TreeEffect::Repaint) => {
                self.checkpoint(window, cx);
                cx.notify();
            }
            Some(TreeEffect::Focus(key)) => {
                let focus = self.tree_focus(&key, cx);
                window.focus(&focus, cx);
                cx.notify();
            }
        }
    }
    /// ↑ / ↓ in the tree.
    pub(in crate::workspace) fn tree_step(
        &mut self,
        delta: isize,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let Some(from) = self.focused_tree_row(window) else {
            return;
        };
        let rows = self.tree_now().rows;
        let effect = self.tree.step(&rows, &from, delta);
        self.carry_out_tree(effect, window, cx);
    }
    /// ⇧↑ / ⇧↓ on an Unfiled row: extend the selection.
    pub(in crate::workspace) fn tree_extend(
        &mut self,
        delta: isize,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let Some(from) = self.focused_tree_row(window) else {
            return;
        };
        let rows = self.tree_now().rows;
        let unfiled = |key: &RowKey| {
            rows.iter().any(|row| {
                matches!(row, Row::Document { id, project: None } if *key == RowKey::Document(id.clone()))
            })
        };
        if !unfiled(&from) {
            return self.tree_step(delta, window, cx);
        }
        if let RowKey::Document(id) = &from
            && self.tree.selection().is_empty()
        {
            self.tree.choose(id);
        }
        let effect = self.tree.step(&rows, &from, delta);
        if let Some(TreeEffect::Focus(RowKey::Document(to))) = &effect
            && unfiled(&RowKey::Document(to.clone()))
        {
            self.tree.extend_to(&rows, to);
            self.carry_out_tree(effect, window, cx);
        }
    }
    /// → / ← in the tree.
    pub(in crate::workspace) fn tree_fold(
        &mut self,
        open: bool,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let Some(from) = self.focused_tree_row(window) else {
            return;
        };
        let rows = self.tree_now().rows;
        let effect = if open {
            self.tree.right(&rows, &from)
        } else {
            self.tree.left(&rows, &from)
        };
        self.carry_out_tree(effect, window, cx);
    }
    /// ⌫ / Delete on an Unfiled row: ask to delete the selection, or it.
    pub(in crate::workspace) fn tree_delete_focused(
        &mut self,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let Some(RowKey::Document(id)) = self.focused_tree_row(window) else {
            return;
        };
        if self.project_of(&id).is_some() {
            return;
        }
        let documents = self.tree.to_delete(&id);
        self.delete_documents(documents, window, cx);
    }
    /// Esc in the tree clears the Unfiled selection first.
    pub(in crate::workspace) fn clear_tree_selection(&mut self, cx: &mut Context<Self>) -> bool {
        let cleared = self.tree.clear_selection();
        if cleared {
            cx.notify();
        }
        cleared
    }
    // ---- Moves and deletes ----------------------------------------------------

    /// A document row was dropped on a Project, or on Unfiled for `None`.
    pub(in crate::workspace) fn drop_document(
        &mut self,
        document: VaultId,
        to: Option<String>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if self.quit_flow.quitting() || self.project_of(document.as_str()) == to {
            return;
        }
        self.move_document(document, to, window, cx);
    }
    /// Ask, then delete Unfiled `documents` with their originals, names and
    /// replies.
    pub(in crate::workspace) fn delete_documents(
        &mut self,
        documents: Vec<String>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        // Only Unfiled documents are deleted from the tree; one moved into a
        // Project since it was chosen stays.
        let unfiled: std::collections::HashSet<String> = (self.tree_now().documents.into_iter())
            .filter(|summary| summary.project.is_none())
            .map(|summary| summary.id)
            .collect();
        let documents: Vec<VaultId> = (documents.iter())
            .filter(|id| unfiled.contains(id.as_str()))
            .filter_map(|id| VaultId::parse(id))
            .filter(|id| !self.moving.contains(id))
            .collect();
        if documents.is_empty() || self.quit_flow.quitting() {
            return;
        }
        #[cfg(test)]
        {
            self.asked_to_delete.push_back(documents.clone());
        }
        let title = match documents.as_slice() {
            [one] => {
                let name = (self.tree_now().documents.into_iter())
                    .find(|summary| summary.id == one.as_str())
                    .map(|summary| summary.title)
                    .unwrap_or_default();
                super::text::delete_one(name)
            }
            many => super::text::delete_many(many.len()),
        };
        let detail = super::text::delete_detail(documents.len());
        let answer = window.prompt(
            PromptLevel::Warning,
            &title,
            Some(detail),
            &[
                gpui::PromptButton::new(super::text::DELETE),
                gpui::PromptButton::cancel(crate::ui::text::CANCEL),
            ],
            cx,
        );
        cx.spawn_in(window, async move |this, cx| {
            if answer.await.ok() != Some(0) {
                return;
            }
            let _ = this.update_in(cx, |this, window, cx| {
                this.forget_documents(documents, window, cx)
            });
        })
        .detach();
    }
    /// Delete `documents` once the writes queued before are done; nothing
    /// writes them meanwhile.
    pub(in crate::workspace) fn forget_documents(
        &mut self,
        documents: Vec<VaultId>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if documents.is_empty() || self.quit_flow.quitting() {
            return;
        }
        for document in &documents {
            self.moving.insert(document.clone());
        }
        self.next_op += 1;
        let target = Target::Op(self.next_op);
        // Any document id stands in: the delete names its documents itself.
        let id = documents[0].clone();
        self.queue_write(
            target,
            Write::Op(id, Op::DeleteDocuments { documents }),
            window,
            cx,
        );
    }
    /// Run a delete of documents at the head of the writer's queue: their
    /// live views go unwritten, then their Vault objects.
    pub(in crate::workspace) fn run_delete_documents(
        &mut self,
        documents: Vec<VaultId>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let Some(vault) = self.vault.vault().cloned() else {
            for document in &documents {
                self.moving.remove(document);
            }
            return;
        };
        // Only while every one is still Unfiled, compared under the Vault's
        // write lock: one moved into a Project since the question (a folder
        // sheet finishing) stops it, and the question comes again.
        let ids: Vec<String> = (documents.iter())
            .map(|document| document.as_str().to_string())
            .collect();
        let failed = match vault.delete_unfiled_documents(&ids) {
            Ok(false) => {
                for document in &documents {
                    self.moving.remove(document);
                }
                self.notices.post(super::text::MOVED_BEFORE_DELETED);
                self.delete_documents(ids, window, cx);
                cx.notify();
                return;
            }
            Ok(true) => None,
            Err(error) => Some(error),
        };
        for document in &documents {
            // A failure after its record went (its folder's cleanup) still
            // deleted it; one before keeps it, and its live view and edits.
            // Only a record known to be gone counts: one that can't be read
            // for now is missing from the summaries too.
            let gone = failed.is_none()
                || matches!(
                    vault.document(document.as_str()),
                    Err(mdoc_vault::VaultError::NotFound { .. })
                );
            self.moving.remove(document);
            if !gone {
                continue;
            }
            if let Some(tab) = self.find_document(&DocumentId::Vault(document.clone()), None, cx) {
                // Home takes a shown document's page.
                if tab == self.tabs.active() && !self.showing_overview(cx) {
                    self.show_home_as(Arrival::Replace, window, cx);
                }
                self.remove(tab, window, cx);
            }
            self.navigation.forget(&Stop::Document(document.clone()));
        }
        // Their replies went with them: a page showing one closes, and
        // documents promoted from them say so.
        self.drop_gone_reply_page(window, cx);
        self.refresh_saved_replies(cx);
        let now = self.tree_now();
        self.tree.keep_chosen(&now.documents);
        if let Some(error) = failed {
            self.notices.post(super::text::could_not_delete(error));
        }
        self.checkpoint(window, cx);
        cx.notify();
    }
    // ---- A document row's menu ------------------------------------------------

    /// The document menu from Share, the ⋯ menu or the menu bar: its moves
    /// alone, at `at` (#1312).
    pub(in crate::workspace) fn open_move_menu(
        &mut self,
        tab: u64,
        at: gpui::Point<gpui::Pixels>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        // As the move itself checks: a loaded, converted Vault document
        // not already moving.
        if self.quit_flow.quitting() || !self.can_move(tab, cx) {
            return;
        }
        let Some(document) = self.tab_document(tab, cx) else {
            return;
        };
        self.menu_return = window.focused(cx);
        self.document_menu = Some((document, at));
        self.move_menu = Some(tab);
        window.focus(&self.menu_focus, cx);
        cx.notify();
    }
    /// The Project menu's Move to Project… (the move menu under the
    /// header's ⋯) and Move to New Project, for the document shown.
    pub(in crate::workspace) fn move_from_menu_bar(
        &mut self,
        new: bool,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if self.showing_overview(cx) {
            return;
        }
        let tab = self.tabs.active();
        let Some(view) = self.tab_view(tab) else {
            return;
        };
        if new {
            self.move_to_new_project(tab, window, cx);
        } else {
            let at = view.read(cx).more_bounds.get().bottom_left();
            self.open_move_menu(tab, at, window, cx);
        }
    }
    pub(in crate::workspace) fn close_document_menu(
        &mut self,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.document_menu = None;
        self.move_menu = None;
        if let Some(focus) = self.menu_return.take() {
            window.focus(&focus, cx);
        }
        cx.notify();
    }
}

impl Workspace {
    /// The place on screen, as a stop.
    pub(in crate::workspace) fn place(&self, cx: &App) -> Stop {
        if let Some(page) = &self.reply_page {
            Stop::Reply(page.id().into())
        } else if let Some(page) = &self.project_ui.page {
            Stop::Project(page.id().into())
        } else if self.showing_home(cx) {
            Stop::Home
        } else if let Some(document) =
            (self.tabs.active_tab()).and_then(|tab| self.live_document(tab, cx))
        {
            Stop::Document(document)
        } else {
            Stop::Tab(self.tabs.active())
        }
    }
    pub(in crate::workspace) fn note_destination(&mut self, arrival: Arrival, cx: &App) {
        let stop = self.place(cx);
        self.note_ai_viewed(&stop);
        self.navigation.arrive(stop, arrival);
    }
    /// The one liveness rule for the buttons, their tooltips and the
    /// actions: Home, a document the Vault holds, a live view that isn't
    /// Home's placeholder, a Project that still exists.
    fn live_stops<'a>(&'a self, cx: &'a App) -> impl Fn(&Stop) -> bool + 'a {
        let projects: Vec<String> = self
            .projects
            .summaries()
            .into_iter()
            .map(|project| project.id)
            .collect();
        let documents: std::collections::HashSet<String> = (self.vault.vault().into_iter())
            .flat_map(|vault| vault.documents())
            .map(|summary| summary.id)
            .collect();
        move |stop| match stop {
            Stop::Home => true,
            Stop::Document(id) => documents.contains(id.as_str()),
            Stop::Tab(id) => self
                .tabs
                .get(*id)
                .is_some_and(|tab| !self.is_placeholder(tab, cx)),
            Stop::Project(id) => projects.contains(id),
            Stop::Reply(id) => self.reply_exists(id),
        }
    }
    pub(in crate::workspace) fn navigation_target(
        &self,
        backwards: bool,
        cx: &App,
    ) -> Option<&Stop> {
        self.navigation
            .target(backwards, &self.place(cx), self.live_stops(cx))
    }
    pub(in crate::workspace) fn navigation_label(&self, backwards: bool, cx: &App) -> String {
        let direction = if backwards {
            super::text::BACK
        } else {
            super::text::FORWARD
        };
        let destination = match self.navigation_target(backwards, cx) {
            Some(Stop::Home) => Some(super::text::HOME.into()),
            Some(Stop::Tab(id)) => self
                .tabs
                .get(*id)
                .map(|tab| crate::workspace::tab_label(tab, cx).name),
            Some(Stop::Document(id)) => (self.vault.vault().into_iter())
                .flat_map(|vault| vault.documents())
                .find(|summary| summary.id == id.as_str())
                .map(|summary| summary.title),
            Some(Stop::Project(id)) => self
                .projects
                .summaries()
                .into_iter()
                .find(|project| &project.id == id)
                .map(|project| super::text::project_stop(&project.name)),
            Some(Stop::Reply(id)) => self.reply_title(id).map(|title| format!("“{title}”")),
            None => None,
        };
        match destination {
            Some(destination) => super::text::stop_toward(direction, destination),
            None => direction.into(),
        }
    }
    pub(in crate::workspace) fn navigate(
        &mut self,
        backwards: bool,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        // Navigation must preserve the restrictions of stacked modal surfaces.
        if self.settings.read(cx).open
            || self.project_ui.list_open
            || self.project_ui.explainer
            || self.folder_sheets.open()
            || self.new_project.is_some()
            || self.palette.palette.is_some()
            || window.has_active_prompt()
            || self.quit_flow.quitting()
        {
            return;
        }
        let here = self.place(cx);
        let Some(index) = self
            .navigation
            .target_index(backwards, &here, self.live_stops(cx))
        else {
            return;
        };
        let stop = self.navigation.go(index);
        self.close_shortcuts(window, cx);
        // A menu belongs to the page it was opened over.
        self.document_menu = None;
        self.project_ui.header_menu = None;
        self.project_ui.ask_menu = None;
        self.menu_return = None;
        // The cursor is already on this stop, so the destination's own
        // visit is a no-op and preserves the forward branch.
        match stop {
            Stop::Home => self.show_home(window, cx),
            Stop::Tab(id) => self.activate(id, window, cx),
            Stop::Document(id) => self.open_vault_document(id, window, cx),
            Stop::Project(id) => self.show_project_page(&id, Place::Header, window, cx),
            Stop::Reply(id) => self.show_reply_page(&id, Arrival::Visit, window, cx),
        }
    }
}

impl Workspace {
    /// `⌃Tab` / `⌃⇧Tab`: open the switcher, or move its pick.
    pub(in crate::workspace) fn switch_document(
        &mut self,
        backwards: bool,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if self.quit_flow.quitting() {
            return;
        }
        if let Some(switcher) = &mut self.switcher {
            switcher.step(if backwards { -1 } else { 1 });
            cx.notify();
            return;
        }
        if self.dialog_up(window)
            || self.settings.read(cx).open
            || self.palette.palette.is_some()
            || self.project_ui.list_open
        {
            return;
        }
        let views = self.tabs.recent(|tab| {
            if self.is_placeholder(tab, cx) {
                crate::workspace::saving::live_views::Weight::Ignored
            } else {
                crate::workspace::saving::live_views::Weight::Idle
            }
        });
        let shown = !self.showing_overview(cx);
        let Some(switcher) = Switcher::open(views, shown, backwards) else {
            return;
        };
        self.switcher = Some(switcher);
        self.switcher_return = window.focused(cx);
        window.focus(&self.switcher_focus, cx);
        cx.notify();
    }
    /// Show the switcher's pick.
    pub(in crate::workspace) fn confirm_switcher(
        &mut self,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let Some(switcher) = self.switcher.take() else {
            return;
        };
        self.switcher_return = None;
        let picked = switcher.picked();
        if self.tabs.contains(picked) {
            self.activate(picked, window, cx);
        }
        cx.notify();
    }
    /// The live views as the Window menu lists them: by title, in the
    /// order they opened (#1160). Which one is shown is left out, so
    /// switching documents leaves the menu bar as it is.
    pub(in crate::workspace) fn live_view_items(
        &self,
        cx: &App,
    ) -> Vec<crate::commands::menus::LiveViewItem> {
        let mut views: Vec<_> = (self.tabs.iter())
            .filter(|tab| !self.is_placeholder(tab, cx))
            .map(|tab| crate::commands::menus::LiveViewItem {
                id: tab.id(),
                title: crate::workspace::tab_label(tab, cx).name.into(),
            })
            .collect();
        views.sort_by_key(|view| view.id);
        views
    }
    /// Window › a live view's title: the window comes forward and shows
    /// it, as Back and the switcher would. Nothing moves under a dialog,
    /// sheet, Settings, the palette or the Project list, or while
    /// quitting; one no longer live does nothing.
    pub(in crate::workspace) fn show_live_view(
        &mut self,
        id: u64,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if self.quit_flow.quitting()
            || self.modal_up(window, cx)
            || self.palette.palette.is_some()
            || self.project_ui.list_open
            || self.switcher.is_some()
            || !self.tabs.contains(id)
        {
            return;
        }
        crate::shelf::menu_bar::reveal_window(window, cx);
        self.activate(id, window, cx);
        cx.notify();
    }
    /// Esc or a click outside: nothing changes.
    pub(in crate::workspace) fn cancel_switcher(
        &mut self,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if self.switcher.take().is_none() {
            return;
        }
        if let Some(focus) = self.switcher_return.take() {
            window.focus(&focus, cx);
        }
        cx.notify();
    }
}

#[cfg(test)]
impl Workspace {
    /// Show the next most recent live view, as a quick `⌃Tab` does.
    pub(in crate::workspace) fn show_next_recent(
        &mut self,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if let Some(next) = self.tabs.iter().nth(1).map(|tab| tab.id()) {
            self.activate(next, window, cx);
        }
    }
}

impl Workspace {
    // ---- What the tree's views ask ------------------------------------------

    /// Whether a Vault is open, so the tree has Projects to show.
    pub(super) fn tree_has_vault(&self) -> bool {
        self.vault.vault().is_some()
    }

    /// Whether the document menu offers moves: a Vault is open and the
    /// document isn't moving already.
    pub(super) fn document_movable(&self, document: &VaultId) -> bool {
        self.vault.vault().is_some() && !self.moving.contains(document)
    }

    /// A Project's chevron folds or unfolds it, and the session is saved.
    pub(super) fn toggle_project(
        &mut self,
        project: &str,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.tree.toggle(project);
        self.checkpoint(window, cx);
        cx.notify();
    }
}
