//! Intake's state: a file admitted into the Vault, its kind and whether it
//! needs conversion, the import's staged files and steps, and what an
//! import error says. No GPUI.

use crate::workspace::saving::text;
use mdoc_store::document_id::VaultId;
use mdoc_vault::SourceKind;
use std::path::{Path, PathBuf};

/// A file read for admission: its source's hash, and the text of a
/// Markdown or plain text file, from one read of its bytes: an import
/// seals those same bytes as its original.
pub(in crate::workspace) struct Admitted {
    pub(super) path: PathBuf,
    pub(super) sha256: String,
    pub(super) kind: SourceKind,
    pub(super) text: Option<String>,
}

/// The family a file belongs to, from its name, or its content for an
/// image under another name. Desktop never writes `scan`: an OCR'd PDF
/// stays a PDF.
pub(in crate::workspace) fn source_kind(path: &Path) -> SourceKind {
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
pub(in crate::workspace) fn needs_conversion(path: &Path) -> bool {
    !mdoc_store::document::is_markdown(path)
}

pub(super) fn file_name(path: &Path) -> String {
    path.file_name()
        .map(|name| name.to_string_lossy().into_owned())
        .unwrap_or_default()
}

/// An import's files as sealed beside the writer, in the order opened,
/// waiting to be published by a job on it.
pub(in crate::workspace) struct Staged {
    pub(super) steps: Vec<Step>,
    pub(super) project: Option<String>,
}

/// One file of an import.
pub(super) enum Step {
    /// A repeat that opens the document imported before.
    Existing(VaultId, PathBuf),
    /// Sealed, with what admission read of it.
    Staged(Box<(Admitted, mdoc_vault::StagedImport)>),
    /// Its name, and why it could not be sealed.
    Failed(String, String),
}

/// What the repeated-import question decided for the repeats.
#[derive(Clone, Copy, PartialEq, Eq)]
pub(super) enum Repeats {
    OpenExisting,
    ImportAgain,
    Skip,
}

/// Why an import into the Vault failed, in words: a Project gone since the
/// import began is never named by its id (#1337).
pub(super) fn import_error(error: &mdoc_vault::VaultError) -> String {
    match error {
        mdoc_vault::VaultError::NotFound {
            kind: mdoc_vault::ObjectKind::Project,
            ..
        } => text::PROJECT_GONE.into(),
        error => error.to_string(),
    }
}
