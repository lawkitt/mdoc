//! Intake's actions: a file opened, dropped or chosen in a folder sheet
//! becomes a Vault document, Unfiled or in its Project, through the
//! writer's import job; and opening Vault documents and paths.

use super::state::{
    Admitted, Repeats, Staged, Step, file_name, import_error, needs_conversion, source_kind,
};
use crate::document::OcrState;
use crate::workspace::projects;
use crate::workspace::saving::text;
use crate::workspace::{TabRecord, Workspace};
use gpui::{Context, PromptLevel, Window};
use mdoc_import::import;
use mdoc_store::document_id::{DocumentId, VaultId};
use mdoc_store::{document, session_store};
use mdoc_vault::{DocumentOrigin, NewDocument, OriginalInfo};
use std::path::PathBuf;

/// Read `path` for admission.
pub(super) fn admit_one(path: PathBuf) -> Result<Admitted, (String, String)> {
    let failed = |error: std::io::Error| (file_name(&path), error.to_string());
    let kind = source_kind(&path);
    // Markdown and plain text are their own text, hashed from the same
    // read; any other file converts, and its import checks its hash.
    let (sha256, text) = if needs_conversion(&path) {
        let sha256 = std::fs::File::open(&path)
            .and_then(mdoc_vault::source_sha256)
            .map_err(failed)?;
        (sha256, None)
    } else {
        let bytes = std::fs::read(&path).map_err(failed)?;
        let sha256 = mdoc_vault::source_sha256(&bytes[..]).map_err(failed)?;
        let text = String::from_utf8(bytes)
            .map_err(|error| failed(std::io::Error::new(std::io::ErrorKind::InvalidData, error)))?;
        (sha256, Some(text))
    };
    Ok(Admitted {
        path,
        sha256,
        kind,
        text,
    })
}

impl Workspace {
    /// Admit `paths` (supported files) as Vault documents, Unfiled or in
    /// `project`, after any admission still running: a file the one before
    /// is importing then counts as a repeat.
    pub(in crate::workspace) fn admit(
        &mut self,
        paths: Vec<PathBuf>,
        project: Option<String>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if paths.is_empty() {
            return;
        }
        if self.admitting {
            self.admit_queue.push_back((paths, project));
            return;
        }
        self.admitting = true;
        let task = cx
            .background_executor()
            .spawn(async move { paths.into_iter().map(admit_one).collect::<Vec<_>>() });
        cx.spawn_in(window, async move |this, cx| {
            let admitted = task.await;
            let _ = this.update_in(cx, |this, window, cx| {
                this.admitted(admitted, project, window, cx)
            });
        })
        .detach();
    }

    pub(super) fn admitted(
        &mut self,
        admitted: Vec<Result<Admitted, (String, String)>>,
        project: Option<String>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let Some(vault) = self.vault.vault().cloned() else {
            self.admitted_all(window, cx);
            return;
        };
        let mut files: Vec<Admitted> = Vec::new();
        let mut failed = Vec::new();
        for item in admitted {
            match item {
                // The same bytes twice in one open are one document.
                Ok(item) if files.iter().any(|file| file.sha256 == item.sha256) => {}
                Ok(item) => files.push(item),
                Err(failure) => failed.push(failure),
            }
        }
        self.post_failures(&failed);
        // Open in a tab from the same path, with the same bytes: it only
        // comes to the front. A file replaced since is admitted again, as
        // new bytes are (a repeat of another document asks).
        let mut shown = None;
        files.retain(|file| match self.open_tab_for(&file.path, cx) {
            Some(id) if self.tab_shows_source(id, &file.sha256, &vault, cx) => {
                shown.get_or_insert(id);
                false
            }
            _ => true,
        });
        if files.is_empty() {
            if let Some(id) = shown {
                self.activate(id, window, cx);
            }
            self.admitted_all(window, cx);
            return;
        }
        // Each repeat with the Document it would open: the most recent.
        let existing: Vec<Option<VaultId>> = files
            .iter()
            .map(|file| {
                vault
                    .documents_with_source(&file.sha256)
                    .first()
                    .and_then(|document| VaultId::parse(&document.id))
            })
            .collect();
        let repeats: Vec<String> = files
            .iter()
            .zip(&existing)
            .filter(|(_, existing)| existing.is_some())
            .map(|(file, _)| file_name(&file.path))
            .collect();
        if repeats.is_empty() {
            self.import(files, existing, Repeats::ImportAgain, project, window, cx);
            return;
        }
        let (question, detail) = text::repeats_question(&repeats);
        let answer = window.prompt(
            PromptLevel::Info,
            &question,
            Some(&detail),
            &[
                gpui::PromptButton::new(text::OPEN_EXISTING),
                gpui::PromptButton::new(text::IMPORT_AGAIN),
                gpui::PromptButton::cancel(crate::ui::text::CANCEL),
            ],
            cx,
        );
        cx.spawn_in(window, async move |this, cx| {
            let repeats = match answer.await.ok() {
                Some(0) => Repeats::OpenExisting,
                Some(1) => Repeats::ImportAgain,
                _ => Repeats::Skip,
            };
            let _ = this.update_in(cx, |this, window, cx| {
                this.import(files, existing, repeats, project, window, cx)
            });
        })
        .detach();
    }

    /// Tab `id` shows the Vault document imported from the bytes whose
    /// hash is `sha256`.
    pub(in crate::workspace) fn tab_shows_source(
        &self,
        id: u64,
        sha256: &str,
        vault: &mdoc_vault::Vault,
        cx: &gpui::App,
    ) -> bool {
        let Some(tab) = self.tabs.get(id) else {
            return false;
        };
        let document = (tab.view())
            .and_then(|view| view.read(cx).session.document.vault.clone())
            .or_else(|| tab.record().document.clone());
        document.is_some_and(|document| {
            (vault.documents_with_source(sha256).iter()).any(|d| d.id == document.as_str())
        })
    }

    pub(super) fn post_failures(&mut self, failed: &[(String, String)]) {
        if failed.is_empty() {
            return;
        }
        self.notices.post(text::could_not_import(failed));
    }

    /// Import `files` into the Vault, each as `repeats` says when it is a
    /// repeat of `existing`, then show them. Their originals are sealed
    /// off the main thread and beside the writer, then published as one
    /// job on it (ADR 0130).
    #[allow(clippy::too_many_arguments)]
    pub(super) fn import(
        &mut self,
        files: Vec<Admitted>,
        existing: Vec<Option<VaultId>>,
        repeats: Repeats,
        project: Option<String>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let Some(vault) = self.vault.vault().cloned() else {
            self.admitted_all(window, cx);
            return;
        };
        // In the order opened: a document to import, or an existing one.
        let mut plan = Vec::new();
        for (file, existing) in files.into_iter().zip(existing) {
            match (existing, repeats) {
                (Some(id), Repeats::OpenExisting) => plan.push(Err((id, file.path))),
                (Some(_), Repeats::Skip) => {}
                _ => plan.push(Ok(file)),
            }
        }
        let task = cx.background_executor().spawn(async move {
            (plan.into_iter())
                .map(|step| match step {
                    Err((id, path)) => Step::Existing(id, path),
                    Ok(file) => {
                        // Markdown and plain text: the bytes admission read.
                        // A converted file: read again, and published only
                        // while it hashes as admission hashed it.
                        let staged = match &file.text {
                            Some(text) => vault.stage_import(text.as_bytes()),
                            None => std::fs::File::open(&file.path)
                                .map_err(mdoc_vault::VaultError::from)
                                .and_then(|source| vault.stage_import(source)),
                        };
                        match staged {
                            Ok(staged) => Step::Staged(Box::new((file, staged))),
                            Err(error) => Step::Failed(file_name(&file.path), error.to_string()),
                        }
                    }
                })
                .collect::<Vec<_>>()
        });
        cx.spawn_in(window, async move |this, cx| {
            let steps = task.await;
            let _ = this.update_in(cx, |this, window, cx| {
                // Published before a quit waiting at the head, the quit
                // being the last job: a cancelled quit loses nothing.
                this.next_op += 1;
                let target = crate::workspace::saving::document_writer::Target::Op(this.next_op);
                let staged = Staged { steps, project };
                this.queue_write(
                    target,
                    crate::workspace::saving::autosave::Write::Import(staged),
                    window,
                    cx,
                );
            });
        })
        .detach();
    }

    /// Publish what an import sealed, at the head of the writer's queue:
    /// each file only while its bytes are the ones admission read and its
    /// Project still exists, both checked under the Vault's write lock.
    /// A file changed since is admitted again; the rest open.
    pub(in crate::workspace) fn run_import(
        &mut self,
        staged: Staged,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let Some(vault) = self.vault.vault().cloned() else {
            self.admitted_all(window, cx);
            return;
        };
        let Staged { steps, project } = staged;
        // Converted files whose bytes changed since admission: admitted
        // again, as they are now.
        let mut changed = Vec::new();
        let imported = (steps.into_iter())
            .filter_map(|step| match step {
                Step::Existing(id, path) => Some(Ok(Err((id, path)))),
                Step::Failed(name, error) => Some(Err((name, error))),
                Step::Staged(staged) => {
                    let (file, staged) = *staged;
                    let name = file_name(&file.path);
                    let title = file
                        .path
                        .file_stem()
                        .map(|stem| stem.to_string_lossy().into_owned())
                        .unwrap_or_else(|| name.clone());
                    let info = OriginalInfo {
                        file_name: name.clone(),
                        ..OriginalInfo::default()
                    };
                    let expect = file.text.is_none().then_some(file.sha256.as_str());
                    let new = NewDocument {
                        project: project.clone(),
                        title: title.clone(),
                        source_kind: file.kind,
                        origin: DocumentOrigin::Ordinary,
                        markdown: file.text.clone().unwrap_or_default(),
                    };
                    match vault.publish_import(staged, new, info, expect) {
                        Ok(None) => {
                            changed.push(file.path);
                            None
                        }
                        Ok(Some(document)) => Some(
                            VaultId::parse(&document.id)
                                .map(|id| Ok((id, title, file.path)))
                                .ok_or_else(|| (document.title, text::INVALID_ID.into())),
                        ),
                        Err(error) => Some(Err((name, import_error(&error)))),
                    }
                }
            })
            .collect();
        if !changed.is_empty() {
            self.admit_queue.push_front((changed, project));
        }
        self.show_imported(imported, window, cx);
    }

    #[allow(clippy::type_complexity)]
    pub(super) fn show_imported(
        &mut self,
        imported: Vec<
            Result<Result<(VaultId, String, PathBuf), (VaultId, PathBuf)>, (String, String)>,
        >,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if !self.quit_flow.may_quit() {
            self.admitted_all(window, cx);
            return;
        }
        let mut failed = Vec::new();
        let mut created = Vec::new();
        let mut first = None;
        for step in imported {
            let id = match step {
                Ok(Ok((document, title, path))) => {
                    let mut record = TabRecord {
                        preview_visible: needs_conversion(&path),
                        ..TabRecord::default()
                    };
                    record.set_document(DocumentId::Vault(document));
                    // Shown until the document loads; never saved.
                    record.name = Some(title);
                    let id = self.push(record);
                    crate::document::LastFolder::note(&path, cx);
                    // The path now finds the newest import of it.
                    self.admitted_from.retain(|_, from| *from != path);
                    self.admitted_from.insert(id, path);
                    created.push(id);
                    id
                }
                Ok(Err((document, path))) => {
                    let document = DocumentId::Vault(document);
                    match self.find_document(&document, None, cx) {
                        Some(id) => id,
                        None => {
                            let mut record = TabRecord::default();
                            record.set_document(document);
                            let id = self.push(record);
                            self.name_from_vault(id);
                            // Not imported: the last folder stays as it was.
                            self.admitted_from.insert(id, path);
                            created.push(id);
                            id
                        }
                    }
                }
                Err(failure) => {
                    failed.push(failure);
                    continue;
                }
            };
            first.get_or_insert(id);
        }
        self.post_failures(&failed);
        if let Some(first) = first {
            self.show_opened(first, window, cx);
        }
        self.admitted_all(window, cx);
        cx.notify();
    }

    /// An admission ended: the next one waiting starts.
    pub(super) fn admitted_all(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.admitting = false;
        if let Some((paths, project)) = self.admit_queue.pop_front() {
            self.admit(paths, project, window, cx);
        }
    }

    /// Show Vault document `document`: its tab, or a new one.
    pub(in crate::workspace) fn open_vault_document(
        &mut self,
        document: VaultId,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let id = DocumentId::Vault(document);
        let tab = match self.find_document(&id, None, cx) {
            Some(tab) => tab,
            None => {
                let mut record = TabRecord::default();
                record.set_document(id);
                let tab = self.push(record);
                self.name_from_vault(tab);
                tab
            }
        };
        self.show_opened(tab, window, cx);
    }

    /// A Vault tab not loaded yet is named as its document is.
    pub(in crate::workspace) fn name_from_vault(&mut self, id: u64) {
        let Some(document) = self
            .tabs
            .get(id)
            .and_then(|tab| tab.record().document.clone())
        else {
            return;
        };
        let title = self.vault.vault().and_then(|vault| {
            vault
                .documents()
                .into_iter()
                .find(|summary| summary.id == document.as_str())
                .map(|summary| summary.title)
        });
        self.tabs.set_name(id, title);
    }

    #[cfg(test)]
    pub fn open_path(&mut self, path: PathBuf, window: &mut Window, cx: &mut Context<Self>) {
        self.open_paths(vec![path], window, cx);
    }

    pub fn open_paths(&mut self, paths: Vec<PathBuf>, window: &mut Window, cx: &mut Context<Self>) {
        self.open_paths_into(paths, None, window, cx);
    }

    /// Open what arrived while a confirm dialog was up, once it is answered.
    pub(in crate::workspace) fn open_after_prompt(
        &mut self,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if self.awaiting_prompt {
            return;
        }
        self.awaiting_prompt = true;
        cx.spawn_in(window, async move |this, cx| {
            loop {
                cx.background_executor()
                    .timer(std::time::Duration::from_millis(100))
                    .await;
                let open = this.update_in(cx, |this, window, cx| {
                    if this.dialog_up(window) {
                        return false;
                    }
                    this.awaiting_prompt = false;
                    for (paths, project) in std::mem::take(&mut this.pending_open) {
                        this.open_paths_into(paths, project, window, cx);
                    }
                    true
                });
                if open.unwrap_or(true) {
                    break;
                }
            }
        })
        .detach();
    }

    /// Open paths. Plain files open at once; each folder among them shows
    /// its sheet in turn, offering to add to `project` when it was dropped on
    /// that Project's row.
    pub(in crate::workspace) fn open_paths_into(
        &mut self,
        paths: Vec<PathBuf>,
        project: Option<String>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if !self.ready {
            self.pending_open.push((paths, project));
            return;
        }
        if self.intake_refused(window, cx) {
            return;
        }
        // Opening activates a tab and takes focus; a confirm dialog keeps it
        // until answered, and the paths open after. Checked before
        // quitting, so a cancelled quit still opens them.
        if self.dialog_up(window) {
            self.pending_open.push((paths, project));
            self.open_after_prompt(window, cx);
            return;
        }
        if !self.quit_flow.may_quit() {
            return;
        }
        let requested = paths.clone();
        let task = cx.background_executor().spawn(async move {
            let supported = |path: &std::path::Path| {
                !path.is_dir() && (document::is_markdown(path) || import::supported_source(path))
            };
            let mut files = Vec::new();
            let mut folders = Vec::new();
            for path in paths {
                if let Some(folder) = projects::folder_sheet::DroppedFolder::read(&path, supported)
                {
                    folders.push(folder);
                } else {
                    let ok = supported(&path);
                    files.push((session_store::identity(&path), ok));
                }
            }
            (files, folders)
        });
        cx.spawn_in(window, async move |this, cx| {
            let (paths, folders) = task.await;
            let _ = this.update_in(cx, |this, window, cx| {
                // A dialog that came up while the paths resolved keeps focus.
                if this.dialog_up(window) {
                    this.pending_open.push((requested, project));
                    this.open_after_prompt(window, cx);
                    return;
                }
                this.finish_open(paths, project.clone(), window, cx);
                for folder in folders {
                    let pending = projects::folder_sheet::Pending {
                        folder,
                        project: project.clone(),
                    };
                    this.queue_folder(pending, window, cx);
                }
            });
        })
        .detach();
    }

    /// Open `paths`, supported or not, as Vault documents: Unfiled, or in
    /// Project `project`. A file open already, unchanged, comes to the
    /// front.
    pub(in crate::workspace) fn finish_open(
        &mut self,
        paths: Vec<(PathBuf, bool)>,
        project: Option<String>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if !self.quit_flow.may_quit() || self.vault.vault().is_none() {
            return;
        }
        let (supported, unsupported): (Vec<_>, Vec<_>) =
            paths.into_iter().partition(|(_, supported)| *supported);
        self.post_unsupported(unsupported.into_iter().map(|(path, _)| path));
        // Each is hashed first: a file open in a tab comes to the front only
        // while its bytes are the ones that tab imported (`intake`).
        let paths = supported.into_iter().map(|(path, _)| path).collect();
        self.admit(paths, project, window, cx);
        cx.notify();
    }

    /// Unsupported files are skipped, by name: the notice is no place for
    /// the full path.
    pub(in crate::workspace) fn post_unsupported(
        &mut self,
        paths: impl IntoIterator<Item = PathBuf>,
    ) {
        let names: Vec<_> = paths
            .into_iter()
            .map(|path| {
                path.file_name()
                    .unwrap_or(path.as_os_str())
                    .to_string_lossy()
                    .into_owned()
            })
            .collect();
        if !names.is_empty() {
            self.notices.post(text::unsupported_skipped(&names));
        }
    }

    /// Activate `first`, newly opened, and drop the placeholder tabs it
    /// replaces.
    pub(in crate::workspace) fn show_opened(
        &mut self,
        first: u64,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        {
            let disposable: Vec<_> = self
                .tabs
                .iter()
                .filter(|tab| {
                    tab.id() != first
                        && tab.view().map_or(tab.record().blank_disposable, |view| {
                            let view = view.read(cx);
                            view.lifecycle.blank_disposable()
                                && view.editor.read(cx).text().is_empty()
                                && view.preview.source().is_none()
                        })
                })
                .map(|tab| tab.id())
                .collect();
            self.activate(first, window, cx);
            for id in disposable {
                self.remove(id, window, cx);
            }
        }
    }

    pub fn refresh_import(&self, window: &mut Window, cx: &mut Context<Self>) {
        if self.resolving_close() || self.quit_flow.finishing() {
            return;
        }
        for tab in &self.tabs {
            if let Some(view) = tab.view() {
                view.update(cx, |view, cx| {
                    view.try_auto_convert(window, cx);
                    cx.notify();
                });
            }
        }
        cx.notify();
    }

    pub(crate) fn set_ocr(&mut self, state: OcrState, cx: &mut Context<Self>) {
        self.ocr_state = state.clone();
        for tab in &self.tabs {
            if let Some(view) = tab.view() {
                view.update(cx, |view, cx| {
                    view.ocr_state = state.clone();
                    cx.notify();
                });
            }
        }
        cx.notify();
    }

    /// Files opened from Finder or another app: a window hidden by the
    /// menu bar item shows them.
    pub(crate) fn open_paths_from_os(
        &mut self,
        paths: Vec<PathBuf>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        crate::shelf::menu_bar::reveal_if_hidden(window, cx);
        self.open_paths(paths, window, cx);
    }
}
