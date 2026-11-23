//! Intake: a file opened, dropped or chosen in a folder sheet becomes an
//! Unfiled Vault document when it is admitted, before any conversion, with
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

/// Whether a document of `kind` is its own text, with no Source to show.
pub(super) fn is_text(kind: SourceKind) -> bool {
    matches!(kind, SourceKind::Markdown | SourceKind::Text)
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
    // An `.html` file is converted like any source; Markdown and plain text
    // are their own text.
    let text = (is_text(kind) && mdoc_store::document::is_markdown(&path))
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
    /// Admit `paths` (supported files) as Vault documents in new tabs placed
    /// before the tab now at `at`.
    pub(super) fn admit(
        &mut self,
        paths: Vec<PathBuf>,
        at: Option<usize>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if paths.is_empty() {
            return;
        }
        let anchor = at.and_then(|at| self.tabs.at(at).map(|tab| tab.id()));
        let task = cx
            .background_executor()
            .spawn(async move { paths.into_iter().map(admit_one).collect::<Vec<_>>() });
        cx.spawn_in(window, async move |this, cx| {
            let admitted = task.await;
            let _ = this.update_in(cx, |this, window, cx| {
                this.admitted(admitted, anchor, window, cx)
            });
        })
        .detach();
    }

    fn admitted(
        &mut self,
        admitted: Vec<Result<Admitted, (String, String)>>,
        anchor: Option<u64>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let Some(vault) = self.vault.vault().cloned() else {
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
            self.import(files, existing, Repeats::ImportAgain, anchor, window, cx);
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
                this.import(files, existing, repeats, anchor, window, cx)
            });
        })
        .detach();
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
    fn import(
        &mut self,
        files: Vec<Admitted>,
        existing: Vec<Option<VaultId>>,
        repeats: Repeats,
        anchor: Option<u64>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let Some(vault) = self.vault.vault().cloned() else {
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
                            project: None,
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
                                    .map(|id| Ok((id, title, file.kind, file.path)))
                                    .ok_or_else(|| (document.title, "Invalid id.".into()))
                            })
                    }
                })
                .collect::<Vec<_>>()
        });
        cx.spawn_in(window, async move |this, cx| {
            let imported = task.await;
            let _ = this.update_in(cx, |this, window, cx| {
                this.show_imported(imported, anchor, window, cx)
            });
        })
        .detach();
    }

    #[allow(clippy::type_complexity)]
    fn show_imported(
        &mut self,
        imported: Vec<
            Result<
                Result<(VaultId, String, SourceKind, PathBuf), (VaultId, PathBuf)>,
                (String, String),
            >,
        >,
        anchor: Option<u64>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if !self.quit_flow.may_quit() {
            return;
        }
        let mut failed = Vec::new();
        let mut created = Vec::new();
        let mut first = None;
        for step in imported {
            let id = match step {
                Ok(Ok((document, title, kind, path))) => {
                    let mut record = TabRecord {
                        preview_visible: !is_text(kind),
                        ..TabRecord::default()
                    };
                    record.set_document(DocumentId::Vault(document));
                    // Shown until the document loads; never saved.
                    record.name = Some(title);
                    let id = self.push(record);
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
        if anchor.is_some() && !created.is_empty() {
            self.tabs.place(&created, anchor);
        }
        if let Some(first) = first {
            self.show_opened(first, window, cx);
        }
        cx.notify();
    }

    /// Tests: open Markdown files as a case lists them.
    #[cfg(test)]
    pub(crate) fn open_case_files(
        &mut self,
        paths: Vec<PathBuf>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let paths = paths
            .into_iter()
            .map(|path| (mdoc_store::session_store::identity(&path), true))
            .collect();
        self.finish_open(paths, None, true, window, cx);
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
