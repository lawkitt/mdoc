//! The folder sheet's actions: queueing dropped folders, the rows' status
//! and ticks, and what the buttons do. New files are imported into the
//! Project; open Unfiled ones move into it.

use super::{FolderProject, SheetFocus};
use crate::workspace::projects::text;
use crate::workspace::{
    Workspace,
    projects::folder_sheet::{CAP, DroppedFolder, Pending, Sheet, Status, Target},
};
use gpui::{Context, KeyDownEvent, ScrollHandle, Window};
use mdoc_vault::NewProject;
use std::path::{Path, PathBuf};

impl Workspace {
    /// A dropped folder: a notice when it has nothing to open, else a sheet
    /// once the ones before it are answered.
    pub(in crate::workspace) fn queue_folder(
        &mut self,
        pending: Pending,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if pending.folder.files.is_empty() {
            self.notices.post(pending.folder.empty_notice());
            cx.notify();
            return;
        }
        self.folder_sheets.push(pending);
        self.show_next_folder(window, cx);
    }

    /// Show the next waiting folder's sheet, if none shows.
    pub(super) fn show_next_folder(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let Some(pending) = self.folder_sheets.next() else {
            return;
        };
        let statuses = pending
            .folder
            .files
            .iter()
            .map(|path| self.folder_file_status(path, cx))
            .collect();
        let target = self.folder_target(&pending.folder, pending.project.as_deref(), cx);
        let sheet = Sheet::new(&pending.folder, statuses, target);
        let rows = sheet
            .rows()
            .iter()
            .map(|_| cx.focus_handle().tab_stop(true))
            .collect::<Vec<_>>();
        let return_focus = window.focused(cx);
        if let Some(first) = rows.first() {
            window.focus(first, cx);
        }
        self.folder_sheets.show(sheet);
        self.folder_sheet_focus = Some(SheetFocus {
            group: cx.focus_handle(),
            rows,
            scroll: ScrollHandle::new(),
            return_focus,
        });
        cx.notify();
    }

    /// The tab a file already has: the Vault document imported from it, or
    /// the tab converting it.
    pub(in crate::workspace) fn open_tab_for(&self, path: &Path, cx: &gpui::App) -> Option<u64> {
        // A Vault document opened from `path` this session.
        let admitted = || {
            self.admitted_from
                .iter()
                .find(|(_, from)| from.as_path() == path)
                .map(|(id, _)| *id)
        };
        admitted().or_else(|| {
            self.tabs
                .iter()
                .find(|tab| {
                    let source = tab
                        .view()
                        .and_then(|view| {
                            let view = view.read(cx);
                            view.session
                                .source
                                .as_ref()
                                .or(view.preview.attachment())
                                .or(view.preview.source())
                        })
                        .or(tab.record().attachment.as_ref());
                    source.map(PathBuf::as_path) == Some(path)
                })
                .map(|tab| tab.id())
        })
    }

    pub(super) fn folder_file_status(&self, path: &Path, cx: &gpui::App) -> Status {
        let Some(tab) = self.open_tab_for(path, cx).and_then(|id| self.tabs.get(id)) else {
            return Status::New;
        };
        match self.sidebar_project_link(tab, cx) {
            Some(link) => Status::InProject(link.name),
            None => Status::Open,
        }
    }

    /// The Project the folder goes into: the header it was dropped on, else
    /// the one holding most of its open files.
    pub(super) fn folder_target(
        &self,
        folder: &DroppedFolder,
        header: Option<&str>,
        cx: &gpui::App,
    ) -> Option<Target> {
        let target = |link: crate::workspace::ProjectLink| Target {
            id: link.id,
            name: link.name,
        };
        if let Some(link) = header.and_then(|id| self.projects.link(id)) {
            return Some(target(link));
        }
        let inside = |path: &Path| path.parent() == Some(folder.path.as_path());
        let mut counts: Vec<(crate::workspace::ProjectLink, usize)> = Vec::new();
        for path in folder.files.iter().filter(|path| inside(path)) {
            let Some(link) = (self.open_tab_for(path, cx))
                .and_then(|id| self.tabs.get(id))
                .and_then(|tab| self.sidebar_project_link(tab, cx))
            else {
                continue;
            };
            match counts.iter_mut().find(|(l, _)| l.id == link.id) {
                Some((_, count)) => *count += 1,
                None => counts.push((link, 1)),
            }
        }
        let mut best: Option<(crate::workspace::ProjectLink, usize)> = None;
        for (link, count) in counts {
            if best.as_ref().is_none_or(|(_, most)| count > *most) {
                best = Some((link, count));
            }
        }
        best.map(|(link, _)| target(link))
    }

    /// Hide the sheet and give focus back where it was.
    pub(super) fn close_folder_sheet(
        &mut self,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Option<Sheet> {
        let sheet = self.folder_sheets.take()?;
        if let Some(focus) = self.folder_sheet_focus.take() {
            match focus.return_focus {
                Some(focus) => window.focus(&focus, cx),
                None => self.focus_active(window, cx),
            }
        }
        cx.notify();
        Some(sheet)
    }

    /// Cancel: nothing opens; the next folder's sheet shows.
    pub(in crate::workspace) fn cancel_folder_sheet(
        &mut self,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if self.folder_sheets.waiting() {
            return;
        }
        self.close_folder_sheet(window, cx);
        self.show_next_folder(window, cx);
    }

    /// Open files: the ticked files open Unfiled.
    pub(in crate::workspace) fn open_folder_files(
        &mut self,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if self.folder_sheets.waiting()
            || self
                .folder_sheets
                .sheet()
                .is_none_or(|s| s.ticked_count() == 0)
        {
            return;
        }
        if let Some(sheet) = self.close_folder_sheet(window, cx) {
            self.open_ticked(&sheet, None, window, cx);
        }
        self.show_next_folder(window, cx);
    }

    /// Open the ticked files: those not open yet are imported, Unfiled or
    /// into `project`; the cap holds against the tabs open now.
    pub(super) fn open_ticked(
        &mut self,
        sheet: &Sheet,
        project: Option<String>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let mut new = 0;
        let paths = sheet
            .ticked()
            .into_iter()
            .filter(|path| {
                self.open_tab_for(path, cx).is_some() || {
                    new += 1;
                    new <= CAP
                }
            })
            .map(|path| (path, true))
            .collect();
        self.finish_open(paths, project, window, cx);
    }

    /// Open as New Project / Add to “X”: the ticked files go into that
    /// Project. A file open Unfiled moves into it; one in another Project
    /// stays there. While the Vault's key is locked, the Keychain explainer
    /// comes first, and Not now returns to the sheet.
    pub(in crate::workspace) fn folder_primary(
        &mut self,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if self.folder_sheets.waiting()
            || self
                .folder_sheets
                .sheet()
                .is_none_or(|s| s.ticked_count() == 0)
        {
            return;
        }
        let id = self.folder_sheets.sheet().map_or(0, Sheet::id);
        if !self.vault_key_ready(
            move |this, window, cx| this.folder_project_now(id, window, cx),
            cx,
        ) {
            self.folder_sheets.set_waiting(true);
            return;
        }
        self.folder_project_now(id, window, cx);
    }

    pub(super) fn folder_project_now(
        &mut self,
        id: u64,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        // Only for the sheet that asked: a later sheet is not answered.
        if self.folder_sheets.sheet().is_none_or(|s| s.id() != id) {
            return;
        }
        self.folder_sheets.set_waiting(false);
        if self.vault.vault().is_none() {
            return;
        }
        let Some(sheet) = self.close_folder_sheet(window, cx) else {
            return;
        };
        self.load_projects(cx);
        let target = sheet
            .target()
            .filter(|t| self.projects.project(&t.id).is_some())
            .map(|t| t.id.clone());
        match target {
            Some(project) => self.fill_folder_project(sheet, project, window, cx),
            // Made by a job on the writer, like every Vault write (ADR 0130).
            None => {
                let skip = self
                    .preferences
                    .borrow()
                    .current
                    .as_ref()
                    .map(|p| p.categories.skip.clone())
                    .unwrap_or_default();
                self.next_op += 1;
                let target = crate::workspace::saving::document_writer::Target::Op(self.next_op);
                let write =
                    crate::workspace::saving::autosave::Write::FolderProject(FolderProject {
                        sheet,
                        skip,
                    });
                self.queue_write(target, write, window, cx);
            }
        }
    }

    /// Make a dropped folder's new Project at the head of the writer's
    /// queue, its categories with it in one write, then fill it.
    pub(in crate::workspace) fn run_new_project(
        &mut self,
        project: FolderProject,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let FolderProject { sheet, skip } = project;
        let Some(vault) = self.vault.vault().cloned() else {
            return;
        };
        let new = NewProject {
            name: sheet.name().to_string(),
            colour: None,
            pinned: false,
        };
        match vault.create_project_with(new, |project| project.skip = skip) {
            Ok(project) => {
                let id = project.id.clone();
                self.projects.put(project);
                self.fill_folder_project(sheet, id, window, cx);
            }
            Err(error) => {
                self.notices
                    .post(text::could_not_become_project(sheet.name(), &error));
                self.open_ticked(&sheet, None, window, cx);
                self.show_next_folder(window, cx);
            }
        }
    }

    /// The sheet's ticked files go into Project `project`.
    pub(super) fn fill_folder_project(
        &mut self,
        sheet: Sheet,
        project: String,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let Some(vault) = self.vault.vault().cloned() else {
            return;
        };
        // Open Unfiled files move in, once hashed: a file replaced since its
        // tab imported it is new bytes, imported into it with the others.
        let open: Vec<(u64, PathBuf)> = (sheet.ticked().into_iter())
            .filter_map(|path| Some((self.open_tab_for(&path, cx)?, path)))
            .collect();
        let hashed = cx.background_executor().spawn(async move {
            (open.into_iter())
                .map(|(tab, path)| {
                    let sha256 = std::fs::File::open(&path).and_then(mdoc_vault::source_sha256);
                    (tab, sha256.ok())
                })
                .collect::<Vec<_>>()
        });
        cx.spawn_in(window, async move |this, cx| {
            let hashed = hashed.await;
            let _ = this.update_in(cx, |this, window, cx| {
                for (tab, sha256) in hashed {
                    let unchanged = sha256
                        .is_some_and(|sha256| this.tab_shows_source(tab, &sha256, &vault, cx));
                    let document = (this.tabs.get(tab))
                        .filter(|tab| unchanged && this.sidebar_project_link(tab, cx).is_none())
                        .and_then(|tab| tab.document().map(|d| d.vault().clone()));
                    if let Some(document) = document {
                        this.move_document(document, Some(project.clone()), window, cx);
                    }
                }
                this.open_ticked(&sheet, Some(project), window, cx);
                this.checkpoint(window, cx);
                cx.notify();
                this.show_next_folder(window, cx);
            });
        })
        .detach();
    }

    /// The explainer's Not now: back to the sheet.
    pub(in crate::workspace) fn folder_key_cancelled(&mut self) {
        self.folder_sheets.set_waiting(false);
    }

    pub(in crate::workspace) fn toggle_folder_row(
        &mut self,
        index: usize,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if let Some(sheet) = self.folder_sheets.sheet_mut() {
            sheet.toggle(index);
        }
        self.focus_folder_row(index, window, cx);
    }

    pub(super) fn focus_folder_row(
        &mut self,
        index: usize,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if let Some(focus) = &self.folder_sheet_focus
            && let Some(row) = focus.rows.get(index)
        {
            window.focus(row, cx);
            focus.scroll.scroll_to_item(index);
        }
        cx.notify();
    }

    /// The focused row, if focus is on one.
    pub(in crate::workspace) fn focused_folder_row(&self, window: &Window) -> Option<usize> {
        self.folder_sheet_focus
            .as_ref()?
            .rows
            .iter()
            .position(|row| row.is_focused(window))
    }

    /// ↑ / ↓ between rows, Space ticks, Enter is the primary button, Esc
    /// cancels. A focused button takes Enter and Space itself.
    pub(super) fn folder_sheet_key(
        &mut self,
        event: &KeyDownEvent,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let keystroke = &event.keystroke;
        if keystroke.modifiers.modified() {
            return;
        }
        let row = self.focused_folder_row(window);
        let on_group = self
            .folder_sheet_focus
            .as_ref()
            .is_some_and(|f| f.group.is_focused(window));
        match keystroke.key.as_str() {
            "escape" => self.cancel_folder_sheet(window, cx),
            "enter" if row.is_some() || on_group => self.folder_primary(window, cx),
            "space" if row.is_some() => self.toggle_folder_row(row.unwrap_or(0), window, cx),
            "up" | "down" if row.is_some() => {
                let next = self.folder_sheets.sheet_mut().and_then(|sheet| {
                    sheet.set_cursor(row.unwrap_or(0));
                    sheet.step(keystroke.key == "down")
                });
                if let Some(next) = next {
                    self.focus_folder_row(next, window, cx);
                }
            }
            _ => return,
        }
        cx.stop_propagation();
    }
}
