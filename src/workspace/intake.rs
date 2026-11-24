//! Intake: a file opened, dropped or chosen in a folder sheet becomes a
//! Vault document, Unfiled or in the Project it was dropped on, when it is
//! admitted, before any conversion, with
//! its bytes kept as its Source Original. A file the Vault already holds
//! asks once, for every such file in the open, whether to open the existing
//! document or import it again.
use super::{TabRecord, Workspace};
use gpui::{Context, PromptLevel, Window};
use mdoc_store::document_id::{DocumentId, VaultId};
use mdoc_vault::{DocumentOrigin, NewDocument, OriginalInfo, SourceKind};
use std::path::{Path, PathBuf};

/// A file read for admission: its source's hash, and the text of a
/// Markdown or plain text file.
pub(super) struct Admitted {
    path: PathBuf,
    sha256: String,
    kind: SourceKind,
    text: Option<String>,
}

/// The family a file belongs to, from its name, or its content for an
/// image under another name. Desktop never writes `scan`: an OCR'd PDF
/// stays a PDF.
pub(super) fn source_kind(path: &Path) -> SourceKind {
    let extension = path
        .extension()
        .and_then(|e| e.to_str())
        .map(str::to_ascii_lowercase)
        .unwrap_or_default();
    match extension.as_str() {
        "pdf" => SourceKind::Pdf,
        "doc" | "docx" | "docm" | "dot" | "dotx" | "dotm" | "odt" | "rtf" => SourceKind::Word,
        "xls" | "xlsx" | "xlsm" | "xlsb" | "ods" | "csv" => SourceKind::Spreadsheet,
        "ppt" | "pptx" | "pptm" | "odp" => SourceKind::Presentation,
        "epub" => SourceKind::Ebook,
        "md" | "markdown" | "mdown" => SourceKind::Markdown,
        "txt" | "html" | "htm" => SourceKind::Text,
        _ if mdoc_import::image_pdf::ImageFormat::from_path(path).is_some()
            || mdoc_import::image_pdf::format_of_file(path).is_ok_and(|f| f.is_some()) =>
        {
            SourceKind::Image
        }
        _ => SourceKind::Text,
    }
}

/// Whether the file at `path` is converted, with a Source to show, rather
/// than being its own text (Markdown and plain text).
pub(super) fn needs_conversion(path: &Path) -> bool {
    !mdoc_store::document::is_markdown(path)
}

fn file_name(path: &Path) -> String {
    path.file_name()
        .map(|name| name.to_string_lossy().into_owned())
        .unwrap_or_default()
}

/// Read `path` for admission.
fn admit_one(path: PathBuf) -> Result<Admitted, (String, String)> {
    let failed = |error: std::io::Error| (file_name(&path), error.to_string());
    let sha256 = std::fs::File::open(&path)
        .and_then(mdoc_vault::source_sha256)
        .map_err(failed)?;
    let kind = source_kind(&path);
    // Markdown and plain text are their own text; any other file converts.
    let text = (!needs_conversion(&path))
        .then(|| std::fs::read_to_string(&path))
        .transpose()
        .map_err(failed)?;
    Ok(Admitted {
        path,
        sha256,
        kind,
        text,
    })
}

/// What the repeated-import question decided for the repeats.
#[derive(Clone, Copy, PartialEq, Eq)]
enum Repeats {
    OpenExisting,
    ImportAgain,
    Skip,
}

impl Workspace {
    /// Admit `paths` (supported files) as Vault documents, Unfiled or in
    /// `project`, after any admission still running: a file the one before
    /// is importing then counts as a repeat.
    pub(super) fn admit(
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

    fn admitted(
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
        let (question, detail) = match repeats.as_slice() {
            [one] => (
                format!("“{one}” is already in mdoc"),
                "Open the document you imported before, or import the file again as a new \
                 document."
                    .to_string(),
            ),
            many => (
                format!("{} files are already in mdoc", many.len()),
                format!(
                    "{}\n\nOpen the documents you imported before, or import the files again \
                     as new documents. Other files open either way.",
                    many.join("\n")
                ),
            ),
        };
        let answer = window.prompt(
            PromptLevel::Info,
            &question,
            Some(&detail),
            &[
                gpui::PromptButton::new("Open Existing"),
                gpui::PromptButton::new("Import Again"),
                gpui::PromptButton::cancel("Cancel"),
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
    pub(super) fn tab_shows_source(
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

    fn post_failures(&mut self, failed: &[(String, String)]) {
        if failed.is_empty() {
            return;
        }
        let names: Vec<_> = failed
            .iter()
            .map(|(name, error)| format!("{name} ({error})"))
            .collect();
        self.notices
            .post(format!("Could not import: {}", names.join(", ")));
    }

    /// Import `files` into the Vault, each as `repeats` says when it is a
    /// repeat of `existing`, then show them.
    #[allow(clippy::too_many_arguments)]
    fn import(
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
            plan.into_iter()
                .map(|step| match step {
                    Err(existing) => Ok(Err(existing)),
                    Ok(file) => {
                        let name = file_name(&file.path);
                        let title = file
                            .path
                            .file_stem()
                            .map(|stem| stem.to_string_lossy().into_owned())
                            .unwrap_or_else(|| name.clone());
                        let new = NewDocument {
                            project: project.clone(),
                            title: title.clone(),
                            source_kind: file.kind,
                            origin: DocumentOrigin::Ordinary,
                            markdown: file.text.unwrap_or_default(),
                        };
                        let info = OriginalInfo {
                            file_name: name.clone(),
                            ..OriginalInfo::default()
                        };
                        std::fs::File::open(&file.path)
                            .map_err(mdoc_vault::VaultError::from)
                            .and_then(|source| vault.import_document(new, source, info))
                            .map_err(|error| (name, error.to_string()))
                            .and_then(|document| {
                                VaultId::parse(&document.id)
                                    .map(|id| Ok((id, title, file.path)))
                                    .ok_or_else(|| (document.title, "Invalid id.".into()))
                            })
                    }
                })
                .collect::<Vec<_>>()
        });
        cx.spawn_in(window, async move |this, cx| {
            let imported = task.await;
            let _ = this.update_in(cx, |this, window, cx| {
                this.show_imported(imported, window, cx)
            });
        })
        .detach();
    }

    #[allow(clippy::type_complexity)]
    fn show_imported(
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
                    crate::document_view::LastFolder::note(&path, cx);
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
    fn admitted_all(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.admitting = false;
        if let Some((paths, project)) = self.admit_queue.pop_front() {
            self.admit(paths, project, window, cx);
        }
    }

    /// Show Vault document `document`: its tab, or a new one.
    pub(super) fn open_vault_document(
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
    pub(super) fn name_from_vault(&mut self, id: u64) {
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
}
