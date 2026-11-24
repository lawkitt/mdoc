//! The Vault tree in the window: its rows drawn from the Vault's summaries,
//! opening a document from its row (the ninth live view is evicted), the
//! keyboard inside the tree, dragging a document onto a Project or Unfiled,
//! a document row's menu, and Unfiled's multi-select delete.
use super::{
    Tab, Workspace,
    autosave::{Op, Write},
    document_writer::Target,
    live_views::{LIMIT, Weight},
    navigation::{Arrival, Stop},
    tree::{Row, RowKey, Shown, TreeEffect},
};
use crate::{style, ui};
use gpui::{AnyElement, App, Context, PromptLevel, Window, div, prelude::*, px};
use mdoc_store::document_id::{DocumentId, VaultId};
use mdoc_vault::{DocumentOrigin, DocumentSummary, ProjectSummary, SourceKind};

/// A document row being dragged onto a Project or Unfiled.
#[derive(Clone)]
pub(super) struct DocumentDrag {
    pub(super) id: VaultId,
    title: String,
}

impl Render for DocumentDrag {
    fn render(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
        div().p_2().child(self.title.clone())
    }
}

/// The tree's rows and what they are drawn from.
pub(super) struct TreeNow {
    pub(super) projects: Vec<ProjectSummary>,
    pub(super) documents: Vec<DocumentSummary>,
    pub(super) rows: Vec<Row>,
}

/// The short kind label a document row shows.
pub(super) fn kind_label(document: &DocumentSummary) -> &'static str {
    match document.source_kind {
        SourceKind::Pdf => "PDF",
        SourceKind::Word => "DOC",
        SourceKind::Spreadsheet => "XLS",
        SourceKind::Presentation => "PPT",
        SourceKind::Ebook => "EPUB",
        SourceKind::Image => "IMG",
        SourceKind::Scan => "SCAN",
        SourceKind::Markdown => "MD",
        SourceKind::Text if document.origin == DocumentOrigin::New => "MD",
        SourceKind::Text => "TXT",
    }
}

impl Workspace {
    // ---- What the tree shows ------------------------------------------------

    /// The tree as it stands: the window's Projects and the Vault's
    /// documents, none while the Vault is closed.
    pub(super) fn tree_now(&self) -> TreeNow {
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
    pub(super) fn live_document(&self, tab: &Tab, cx: &App) -> Option<VaultId> {
        if let Some(view) = tab.view()
            && let Some(DocumentId::Vault(id)) = view.read(cx).session.document.id()
        {
            return Some(id);
        }
        tab.record().document.clone()
    }

    /// What the window shows, as the tree marks it.
    pub(super) fn tree_shown(&self, cx: &App) -> Shown {
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
    pub(super) fn tree_keys(&self) -> Vec<RowKey> {
        self.tree_now().rows.iter().filter_map(Row::key).collect()
    }

    // ---- Opening and evicting -----------------------------------------------

    /// A document row was clicked: its live view, or a new one, shows.
    pub(super) fn open_from_tree(
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
        {
            Weight::Busy
        } else {
            Weight::Idle
        }
    }

    /// A New document left before its first keystroke goes: it never became
    /// a document, so nothing names it. One typed into and emptied again
    /// stays, as does Home's placeholder.
    pub(super) fn drop_if_empty_new(
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
    pub(super) fn evict(&mut self, window: &mut Window, cx: &mut Context<Self>) {
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

    // ---- The keyboard --------------------------------------------------------

    /// The focus handle of row `key`, made on first use.
    pub(super) fn tree_focus(&self, key: &RowKey, cx: &mut App) -> gpui::FocusHandle {
        self.tree_row_focus
            .borrow_mut()
            .entry(key.clone())
            .or_insert_with(|| cx.focus_handle().tab_stop(true))
            .clone()
    }

    /// The row whose focus handle holds focus.
    pub(super) fn focused_tree_row(&self, window: &Window) -> Option<RowKey> {
        self.tree_row_focus
            .borrow()
            .iter()
            .find(|(_, focus)| focus.is_focused(window))
            .map(|(key, _)| key.clone())
    }

    /// Where F6 enters the tree: the selected row, else the first.
    pub(super) fn tree_landing(&self, cx: &mut App) -> Option<gpui::FocusHandle> {
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
    pub(super) fn tree_step(&mut self, delta: isize, window: &mut Window, cx: &mut Context<Self>) {
        let Some(from) = self.focused_tree_row(window) else {
            return;
        };
        let rows = self.tree_now().rows;
        let effect = self.tree.step(&rows, &from, delta);
        self.carry_out_tree(effect, window, cx);
    }

    /// ⇧↑ / ⇧↓ on an Unfiled row: extend the selection.
    pub(super) fn tree_extend(
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
    pub(super) fn tree_fold(&mut self, open: bool, window: &mut Window, cx: &mut Context<Self>) {
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
    pub(super) fn tree_delete_focused(&mut self, window: &mut Window, cx: &mut Context<Self>) {
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
    pub(super) fn clear_tree_selection(&mut self, cx: &mut Context<Self>) -> bool {
        let cleared = self.tree.clear_selection();
        if cleared {
            cx.notify();
        }
        cleared
    }

    // ---- Moves and deletes ----------------------------------------------------

    /// A document row was dropped on a Project, or on Unfiled for `None`.
    pub(super) fn drop_document(
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
    pub(super) fn delete_documents(
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
        let title = match documents.as_slice() {
            [one] => {
                let name = (self.tree_now().documents.into_iter())
                    .find(|summary| summary.id == one.as_str())
                    .map(|summary| summary.title)
                    .unwrap_or_default();
                format!("Delete “{name}”?")
            }
            many => format!("Delete {} documents?", many.len()),
        };
        let detail = if documents.len() == 1 {
            "Its text, original, names and AI replies are deleted from this computer. This \
             cannot be undone."
        } else {
            "Their text, originals, names and AI replies are deleted from this computer. This \
             cannot be undone."
        };
        let answer = window.prompt(
            PromptLevel::Warning,
            &title,
            Some(detail),
            &[
                gpui::PromptButton::new("Delete"),
                gpui::PromptButton::cancel("Cancel"),
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
    pub(super) fn forget_documents(
        &mut self,
        documents: Vec<VaultId>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
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
    pub(super) fn run_delete_documents(
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
        let mut failed = None;
        for document in &documents {
            // A failure after its record went (its folder's cleanup) still
            // deleted it; one before keeps it, and its live view and edits.
            let result = vault.delete_document(document.as_str());
            let gone = result.is_ok()
                || !(vault.documents().iter()).any(|summary| summary.id == document.as_str());
            if let Err(error) = result {
                failed = Some(error);
            }
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
        let now = self.tree_now();
        self.tree.keep_chosen(&now.documents);
        if let Some(error) = failed {
            self.notices
                .post(format!("Could not delete every document: {error}"));
        }
        self.checkpoint(window, cx);
        cx.notify();
    }

    // ---- Drawing ----------------------------------------------------------------

    /// The tree's rows for the expanded sidebar or the rail's reveal.
    pub(super) fn tree_view(&self, cx: &mut Context<Self>) -> Vec<AnyElement> {
        let theme = self.theme.get();
        let palette = theme.pdf_style();
        if self.vault.vault().is_none() {
            return vec![
                self.section_label("Projects"),
                div()
                    .px_2()
                    .py_1()
                    .text_color(palette.header_muted)
                    .child("Documents show here once the Vault opens.")
                    .into_any_element(),
            ];
        }
        let now = self.tree_now();
        let shown = self.tree_shown(cx);
        let selected = self.tree.selected(&now.rows, &shown, &now.documents);
        let stop = self.tree.stop(&now.rows, selected.as_ref());
        let mut elements = Vec::with_capacity(now.rows.len());
        for row in &now.rows {
            let element = match row {
                Row::ProjectsLabel => {
                    let label = self.section_label("Projects");
                    if now.projects.is_empty() {
                        elements.push(label);
                        div()
                            .px_2()
                            .pb_1()
                            .text_size(px(12.))
                            .text_color(palette.header_muted)
                            .child("No Projects yet")
                            .into_any_element()
                    } else {
                        label
                    }
                }
                Row::Project {
                    id,
                    expanded,
                    documents,
                } => {
                    let Some(summary) = now.projects.iter().find(|p| &p.id == id) else {
                        continue;
                    };
                    self.tree_project_row(summary, *expanded, *documents, &selected, &stop, cx)
                }
                Row::Document { id, project } => {
                    let Some(summary) = now.documents.iter().find(|d| &d.id == id) else {
                        continue;
                    };
                    self.tree_document_row(summary, project.is_some(), &selected, &stop, cx)
                }
                Row::UnfiledLabel => {
                    let chosen = (self.tree.selection().iter())
                        .filter(|id| {
                            now.documents
                                .iter()
                                .any(|d| &d.id == *id && d.project.is_none())
                        })
                        .count();
                    self.tree_unfiled_label(chosen, cx)
                }
                Row::ShowAll { hidden } => self.tree_show_all(*hidden, &selected, &stop, cx),
            };
            elements.push(element);
        }
        elements
    }

    /// The common shape of a focusable tree row.
    fn tree_row(
        &self,
        key: &RowKey,
        element_id: String,
        label: String,
        selected: &Option<RowKey>,
        stop: &Option<RowKey>,
        cx: &mut Context<Self>,
    ) -> ui::Control {
        let theme = self.theme.get();
        let palette = theme.pdf_style();
        let focus = self
            .tree_focus(key, cx)
            .tab_stop(stop.as_ref() == Some(key));
        let is_selected = selected.as_ref() == Some(key);
        let menu_key = key.clone();
        let bounds = std::rc::Rc::new(std::cell::Cell::new(gpui::Bounds::default()));
        let drawn = bounds.clone();
        let selector = element_id.clone();
        ui::focus_ring(element_id, theme)
            .when(cfg!(test), move |v| {
                v.debug_selector(move || selector.clone())
            })
            .map(|v| ui::reveal_focus(v.track_focus(&focus), focus, self.sidebar_scroll.clone()))
            .key_context("UiControl SidebarRow")
            .tab_index(0)
            .tab_stop(stop.as_ref() == Some(key))
            .role(gpui::Role::TreeItem)
            .aria_selected(is_selected)
            .aria_label(label)
            .relative()
            .child(
                gpui::canvas(move |b, _, _| drawn.set(b), |_, _, _, _| {})
                    .absolute()
                    .inset_0(),
            )
            .on_action(cx.listener(move |this, _: &ui::SidebarMenu, window, cx| {
                this.menu_return = window.focused(cx);
                let at = bounds.get().bottom_left();
                match &menu_key {
                    RowKey::Document(id) => {
                        if let Some(id) = VaultId::parse(id) {
                            this.document_menu = Some((id, at));
                        }
                    }
                    RowKey::Project(id) => this.project_ui.header_menu = Some((id.clone(), at)),
                    RowKey::ShowAll => return,
                }
                window.focus(&this.menu_focus, cx);
                cx.notify();
            }))
            .flex()
            .items_center()
            .min_w_0()
            .gap_1()
            .px_2()
            .min_h(px(30.))
            .mb(px(1.))
            .rounded_md()
            .when(is_selected, |v| v.bg(theme.sidebar_selected()))
            .hover(move |v| v.bg(palette.placeholder_bg))
            .cursor_pointer()
    }

    fn tree_project_row(
        &self,
        project: &ProjectSummary,
        expanded: bool,
        documents: usize,
        selected: &Option<RowKey>,
        stop: &Option<RowKey>,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let theme = self.theme.get();
        let palette = theme.pdf_style();
        let id = project.id.clone();
        let key = RowKey::Project(id.clone());
        let hue = project
            .colour
            .unwrap_or_else(|| mdoc_vault::Colour::for_id(&project.id))
            .hue();
        let menu_open = self.context_menu_open();
        self.tree_row(
            &key,
            format!("tree-project-{id}"),
            format!("Project {}, {documents} documents", project.name),
            selected,
            stop,
            cx,
        )
        .aria_expanded(expanded)
        .child(
            ui::control(
                gpui::SharedString::from(format!("tree-chevron-{id}")),
                if expanded { "▾" } else { "▸" },
                theme,
                true,
            )
            .when(cfg!(test), {
                let id = id.clone();
                move |v| v.debug_selector(move || format!("tree-chevron-{id}"))
            })
            .tab_stop(false)
            .aria_label(if expanded {
                "Collapse Project"
            } else {
                "Expand Project"
            })
            .px_1()
            .py_0()
            .on_click(cx.listener({
                let id = id.clone();
                move |this, _, window, cx| {
                    cx.stop_propagation();
                    this.tree.toggle(&id);
                    this.checkpoint(window, cx);
                    cx.notify();
                }
            })),
        )
        .child(
            div()
                .size(px(8.))
                .flex_shrink_0()
                .rounded_full()
                .bg(style::project_color(hue, theme)),
        )
        .child(
            div()
                .flex_1()
                .min_w_0()
                .truncate()
                .child(project.name.clone()),
        )
        .child(
            div()
                .text_size(px(11.))
                .text_color(palette.header_muted)
                .child(documents.to_string()),
        )
        .when(!menu_open, |v| {
            v.tooltip(style::tooltip(
                format!(
                    "Project “{}” · its names are kept in the Vault",
                    project.name
                ),
                theme,
            ))
        })
        .on_click(cx.listener({
            let id = id.clone();
            move |this, _, window, cx| {
                if this.quit_flow.quitting() {
                    return;
                }
                this.tree.set_cursor(Some(RowKey::Project(id.clone())));
                this.open_project(&id, window, cx);
            }
        }))
        .on_mouse_down(
            gpui::MouseButton::Right,
            cx.listener({
                let id = id.clone();
                move |this, event: &gpui::MouseDownEvent, window, cx| {
                    this.menu_return = window.focused(cx);
                    // The row keeps no focus of its own from this press.
                    window.prevent_default();
                    window.focus(&this.menu_focus, cx);
                    this.project_ui.header_menu = Some((id.clone(), event.position));
                    cx.notify();
                }
            }),
        )
        .drag_over::<DocumentDrag>(move |v, _, _, _| v.bg(theme.sidebar_selected()))
        .on_drop(cx.listener({
            let id = id.clone();
            move |this, drag: &DocumentDrag, window, cx| {
                this.drop_document(drag.id.clone(), Some(id.clone()), window, cx)
            }
        }))
        // OS files dropped here are imported into this Project.
        .drag_over::<gpui::ExternalPaths>(move |v, _, _, _| v.bg(theme.sidebar_selected()))
        .can_drop(super::can_drop_files)
        .on_drop(cx.listener({
            let id = id.clone();
            move |this, paths: &gpui::ExternalPaths, window, cx| {
                cx.stop_propagation();
                this.drop_on_project(paths, &id, window, cx)
            }
        }))
        .into_any_element()
    }

    fn tree_document_row(
        &self,
        document: &DocumentSummary,
        member: bool,
        selected: &Option<RowKey>,
        stop: &Option<RowKey>,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let theme = self.theme.get();
        let palette = theme.pdf_style();
        let Some(id) = VaultId::parse(&document.id) else {
            return div().into_any_element();
        };
        let key = RowKey::Document(document.id.clone());
        let live = self
            .find_document(&DocumentId::Vault(id.clone()), None, cx)
            .and_then(|tab| self.tabs.get(tab))
            .and_then(|tab| tab.view())
            .map(|view| view.read(cx));
        let running =
            live.is_some_and(|v| v.lifecycle.loading() || v.import.busy() || v.preview.loading());
        let busy = running || live.is_some_and(|v| v.preview.queued());
        let error = live.is_some_and(|v| {
            (v.error.is_some() && !v.conversion_cancelled())
                || v.preview.failed()
                || v.import.ocr_required().is_some()
                || v.import.password_prompt().is_some()
        });
        let pseudonymized = live.is_some_and(|v| v.pseudonymized());
        let recognized = live.is_some_and(|v| v.recognized());
        let chosen = !member && self.tree.is_chosen(&document.id);
        let title = document.title.clone();
        let tooltip = format!(
            "{title}{}{}{}{}",
            if recognized {
                "\nRecognized with OCR"
            } else {
                ""
            },
            if pseudonymized { "\nPseudonymized" } else { "" },
            if busy { "\nProcessing…" } else { "" },
            if error { "\nNeeds attention" } else { "" }
        );
        let menu_open = self.context_menu_open();
        let unfiled = !member;
        self.tree_row(
            &key,
            format!("tree-document-{}", document.id),
            title.clone(),
            selected,
            stop,
            cx,
        )
        .when(member, |v| v.pl(px(26.)))
        .when(chosen, |v| {
            v.bg(gpui::Hsla {
                a: 0.18,
                ..theme.search_accent()
            })
        })
        .child(
            div()
                .min_w(px(32.))
                .text_size(px(10.))
                .text_color(palette.header_muted)
                .flex_shrink_0()
                .child(kind_label(document)),
        )
        .child(
            div()
                .flex_1()
                .min_w_0()
                .child(super::middle_truncated(&title)),
        )
        .when(recognized, |v| {
            v.child(super::tab_badge(ui::Icon::Ocr, palette.header_muted))
        })
        .when(pseudonymized, |v| {
            v.child(super::tab_badge(ui::Icon::Anonymous, palette.header_muted))
        })
        .when(busy || error, |v| {
            v.child(
                div()
                    .text_color(if error {
                        style::markdown_style(theme).alert_warning
                    } else {
                        palette.header_muted
                    })
                    .child(if running && !error {
                        ui::spinner(
                            gpui::SharedString::from(format!("tree-activity-{}", document.id)),
                            theme,
                        )
                    } else {
                        div()
                            .child(if error { "!" } else { "…" })
                            .into_any_element()
                    }),
            )
        })
        .when(!menu_open, |v| v.tooltip(style::tooltip(tooltip, theme)))
        .on_click(cx.listener({
            let id = id.clone();
            move |this, event: &gpui::ClickEvent, window, cx| {
                let modifiers = event.modifiers();
                if unfiled && modifiers.secondary(cx) {
                    this.tree.toggle_chosen(id.as_str());
                    cx.notify();
                    return;
                }
                if unfiled && modifiers.shift {
                    let rows = this.tree_now().rows;
                    this.tree.extend_to(&rows, id.as_str());
                    cx.notify();
                    return;
                }
                if unfiled {
                    this.tree.choose(id.as_str());
                } else {
                    this.tree.clear_selection();
                }
                this.open_from_tree(id.clone(), window, cx);
            }
        }))
        .on_mouse_down(
            gpui::MouseButton::Right,
            cx.listener({
                let id = id.clone();
                move |this, event: &gpui::MouseDownEvent, window, cx| {
                    this.menu_return = window.focused(cx);
                    // The row keeps no focus of its own from this press.
                    window.prevent_default();
                    window.focus(&this.menu_focus, cx);
                    this.document_menu = Some((id.clone(), event.position));
                    cx.notify();
                }
            }),
        )
        .on_drag(
            DocumentDrag {
                id: id.clone(),
                title,
            },
            |drag, _, _, cx| {
                cx.stop_propagation();
                cx.new(|_| drag.clone())
            },
        )
        .when(unfiled, |v| {
            v.drag_over::<DocumentDrag>(move |v, _, _, _| v.bg(theme.sidebar_selected()))
                .on_drop(cx.listener(move |this, drag: &DocumentDrag, window, cx| {
                    this.drop_document(drag.id.clone(), None, window, cx)
                }))
        })
        .into_any_element()
    }

    /// Unfiled's label: a drop target for documents, New, and Delete N…
    /// while several rows are chosen.
    fn tree_unfiled_label(&self, chosen: usize, cx: &mut Context<Self>) -> AnyElement {
        let theme = self.theme.get();
        let palette = theme.pdf_style();
        div()
            .id("tree-unfiled")
            .when(cfg!(test), |v| v.debug_selector(|| "tree-unfiled".into()))
            .flex()
            .items_center()
            .gap_1()
            .px_2()
            .pt_3()
            .pb_1()
            .text_size(px(11.))
            .text_color(palette.header_muted)
            .child(div().flex_1().child("Unfiled"))
            .when(chosen > 1, |v| {
                v.child(
                    ui::control(
                        "tree-delete-chosen",
                        format!("Delete {chosen}…"),
                        theme,
                        true,
                    )
                    .when(cfg!(test), |v| {
                        v.debug_selector(|| "tree-delete-chosen".into())
                    })
                    .aria_label(format!("Delete {chosen} documents"))
                    .px_1()
                    .py_0()
                    .text_color(style::markdown_style(theme).alert_caution)
                    .on_click(cx.listener(|this, _, window, cx| {
                        let documents = this.tree.selection().to_vec();
                        this.delete_documents(documents, window, cx);
                    })),
                )
            })
            .child(
                ui::header_icon_untipped("tree-new", "New document", ui::Icon::Plus, theme, false)
                    .track_focus(&self.panel_new_focus)
                    .when(!self.context_menu_open(), |v| {
                        v.tooltip(style::tooltip(
                            crate::shortcuts::tip("New document", &super::New),
                            theme,
                        ))
                    })
                    .when(cfg!(test), |v| v.debug_selector(|| "tree-new".into()))
                    .size(px(22.))
                    .on_click(|_, window, cx| window.dispatch_action(Box::new(super::New), cx)),
            )
            .drag_over::<DocumentDrag>(move |v, _, _, _| v.bg(theme.sidebar_selected()))
            .on_drop(cx.listener(|this, drag: &DocumentDrag, window, cx| {
                this.drop_document(drag.id.clone(), None, window, cx)
            }))
            .into_any_element()
    }

    fn tree_show_all(
        &self,
        hidden: usize,
        selected: &Option<RowKey>,
        stop: &Option<RowKey>,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let palette = self.theme.get().pdf_style();
        let label = if hidden > 0 {
            format!("Show all ({hidden} more)")
        } else {
            "Show fewer".to_string()
        };
        self.tree_row(
            &RowKey::ShowAll,
            "tree-show-all".into(),
            label.clone(),
            selected,
            stop,
            cx,
        )
        .text_color(palette.header_muted)
        .child(div().pl(px(36.)).child(label))
        .on_click(cx.listener(|this, _, _, cx| {
            this.tree.set_cursor(Some(RowKey::ShowAll));
            this.tree.toggle_show_all();
            cx.notify();
        }))
        .into_any_element()
    }

    // ---- A document row's menu ------------------------------------------------

    /// Move to each other Project, Move to New Project, Move to Unfiled for a
    /// member, and Delete… on an Unfiled row.
    pub(super) fn document_menu_items(
        &self,
        document: &VaultId,
        cx: &mut Context<Self>,
    ) -> Vec<AnyElement> {
        let theme = self.theme.get();
        let palette = theme.pdf_style();
        let row = |id: String, label: String| {
            let selector = id.clone();
            ui::focus_ring(gpui::SharedString::from(id), theme)
                .role(gpui::Role::MenuItem)
                .aria_label(label.clone())
                .key_context("UiControl")
                .tab_index(0)
                .when(cfg!(test), move |v| {
                    let selector = selector.clone();
                    v.debug_selector(move || selector.clone())
                })
                .px_3()
                .py_1()
                .rounded_sm()
                .cursor_pointer()
                .hover(move |v| v.bg(palette.placeholder_bg))
                .max_w(px(320.))
                .truncate()
                .child(label)
        };
        let project = self.project_of(document.as_str());
        let movable = self.vault.vault().is_some() && !self.moving.contains(document);
        let mut items = Vec::new();
        if movable {
            for p in (self.projects.summaries().into_iter())
                .filter(|p| project.as_ref() != Some(&p.id))
                .take(5)
            {
                let (to, document) = (p.id.clone(), document.clone());
                items.push(
                    row(
                        format!("context-move-{}", p.id),
                        format!("Move to “{}”", p.name),
                    )
                    .when(cfg!(test), |v| {
                        v.debug_selector(|| "context-move-project".into())
                    })
                    .on_click(cx.listener(move |this, _, window, cx| {
                        this.close_document_menu(window, cx);
                        this.drop_document(document.clone(), Some(to.clone()), window, cx);
                    }))
                    .into_any_element(),
                );
            }
            let new = document.clone();
            items.push(
                row("context-new-project".into(), "Move to New Project".into())
                    .on_click(cx.listener(move |this, _, window, cx| {
                        this.close_document_menu(window, cx);
                        this.move_document_to_new_project(new.clone(), window, cx);
                    }))
                    .into_any_element(),
            );
            if project.is_some() {
                let leave = document.clone();
                items.push(
                    row("context-leave-project".into(), "Move to Unfiled".into())
                        .on_click(cx.listener(move |this, _, window, cx| {
                            this.close_document_menu(window, cx);
                            this.drop_document(leave.clone(), None, window, cx);
                        }))
                        .into_any_element(),
                );
            }
        }
        if project.is_none() && !self.moving.contains(document) {
            let chosen = self.tree.to_delete(document.as_str());
            let label = if chosen.len() > 1 {
                format!("Delete {} Documents…", chosen.len())
            } else {
                "Delete Document…".to_string()
            };
            if !items.is_empty() {
                items.push(ui::menu_divider(theme).into_any_element());
            }
            items.push(
                row("context-delete-document".into(), label)
                    .text_color(style::markdown_style(theme).alert_caution)
                    .on_click(cx.listener(move |this, _, window, cx| {
                        this.close_document_menu(window, cx);
                        this.delete_documents(chosen.clone(), window, cx);
                    }))
                    .into_any_element(),
            );
        }
        items
    }

    pub(super) fn close_document_menu(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.document_menu = None;
        if let Some(focus) = self.menu_return.take() {
            window.focus(&focus, cx);
        }
        cx.notify();
    }
}
