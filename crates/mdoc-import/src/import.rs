//! Conversion boundary for AnyDoc and PDF extraction. Conversion performs no writes.
use std::{
    io::Read,
    path::{Path, PathBuf},
    sync::atomic::{AtomicBool, Ordering},
};

/// Whether a file dragged from the OS looks openable by its extension alone;
/// drag feedback cannot afford `supported_source`'s content reads (ADR 0028).
pub fn supported_extension(path: &Path) -> bool {
    mdoc_store::document::is_markdown(path) || anydoc::Format::from_path(path).is_some()
}

/// The formats mdoc opens, as hints name them: one entry per converter
/// family, then Markdown, which opens without conversion. `format_family`
/// keeps this list in step with AnyDoc's formats.
const FORMAT_NAMES: [&str; 9] = [
    "PDF",
    "Word",
    "Excel",
    "PowerPoint",
    "OpenDocument",
    "RTF",
    "EPUB",
    "CSV",
    "Markdown",
];

/// The index in `FORMAT_NAMES` that names `format`. Exhaustive, so a format
/// added to AnyDoc fails the test build until a hint names it.
#[cfg(test)]
fn format_family(format: anydoc::Format) -> usize {
    use anydoc::Format;
    match format {
        Format::Pdf => 0,
        Format::Doc | Format::Docx => 1,
        Format::Excel => 2,
        Format::Ppt | Format::Pptx => 3,
        Format::Odt | Format::Ods | Format::Odp => 4,
        Format::Rtf => 5,
        Format::Epub => 6,
        Format::Csv => 7,
    }
}

/// Every supported format for a hint: "PDF, Word, … or Markdown".
pub fn supported_formats() -> String {
    let (last, rest) = FORMAT_NAMES.split_last().expect("formats");
    format!("{} or {last}", rest.join(", "))
}

/// What a drop target accepts:a supported file, or a folder that opens as a
/// case (ADR 0041 #28). One stat per dragged path.
pub fn droppable(path: &Path) -> bool {
    supported_extension(path) || path.is_dir()
}

/// Admission does not convert documents. Unknown extensions retain AnyDoc's
/// content detection, with bounded reads performed on the background executor.
pub fn supported_source(path: &Path) -> bool {
    if anydoc::Format::from_path(path).is_some() {
        return true;
    }
    let Ok(file) = std::fs::File::open(path) else {
        return false;
    };
    let mut bytes = Vec::new();
    const LIMIT: u64 = 256 * 1024 * 1024;
    file.take(LIMIT + 1).read_to_end(&mut bytes).is_ok()
        && bytes.len() as u64 <= LIMIT
        && anydoc::Format::from_bytes(&bytes).is_some()
}

#[derive(Clone, Debug)]
pub struct Imported {
    pub ocr_configuration: Option<mdoc_store::settings::OcrConfig>,
    pub source: PathBuf,
    pub markdown: String,
    pub warning: Option<String>,
    pub is_pdf: bool,
    pub is_docx: bool,
}

#[derive(Debug, thiserror::Error)]
pub enum ImportError {
    /// Pages needing recognition, the languages the PDF's own text layer
    /// suggests (`recognition::text_languages`), and the PDF's page count.
    #[error("This PDF requires OCR.")]
    NeedsOcr(PathBuf, Vec<u32>, Vec<crate::recognition::Language>, u32),
    #[error("Import cancelled")]
    Cancelled,
    #[error("Could not read document: {0}")]
    Read(std::io::Error),
    #[error("{0}")]
    Io(std::io::Error),
    #[error("This PDF exceeds the 256 MiB import limit.")]
    PdfTooLarge,
    #[error("This PDF is password-protected. Import an unlocked copy.")]
    PdfEncrypted,
    #[error("This document is password-protected. Import an unlocked copy.")]
    Encrypted,
    #[error(
        "This PDF requires OCR. Run text recognition in another application, then import it again."
    )]
    ConverterNeedsOcr,
    #[error("This format is not supported. Choose a {} file.", supported_formats())]
    Unsupported,
    #[error("{DAMAGED}")]
    Damaged,
    #[error(
        "This document exceeds the converter’s resource limits. Try importing a smaller document."
    )]
    ResourceLimit,
    /// Any other native PDF pass failure, in pdf-inspector's words.
    #[error("Could not import PDF: {0}")]
    Pdf(String),
    /// Any other conversion failure, in AnyDoc's words.
    #[error("Could not import document: {0}")]
    Convert(String),
    #[error("No usable native text was extracted. Run OCR or use a clearer source.")]
    NoNativeText,
    /// The OCR runtime or models failed; the shared OCR state needs repair.
    #[error("{0}")]
    OcrFailed(OcrFailure),
    /// OCR ran on this document but recognized nothing usable.
    #[error(
        "Local OCR produced no usable text. Retry with a clearer scan or extract native text only."
    )]
    OcrNoText,
}

/// Why local OCR could not run on a document.
#[derive(Debug, thiserror::Error)]
pub enum OcrFailure {
    #[error(transparent)]
    Permit(#[from] crate::model_work::PermitError),
    #[error(transparent)]
    Setup(#[from] crate::ocr::SetupError),
    /// The OCR pipeline failed, in its words.
    #[error("Local OCR failed: {0}")]
    Engine(String),
}

#[cfg(test)]
pub fn convert(path: &Path) -> Result<Imported, String> {
    prepare(path, None, true).map_err(|error| match error {
        ImportError::NeedsOcr(..) => "This PDF requires OCR.".into(),
        other => other.to_string(),
    })
}

#[cfg(test)]
pub fn prepare(
    path: &Path,
    installed: Option<&crate::ocr::Installed>,
    skip_ocr: bool,
) -> Result<Imported, ImportError> {
    prepare_cancellable(path, installed, skip_ocr, &AtomicBool::new(false))
}

pub fn prepare_cancellable(
    path: &Path,
    installed: Option<&crate::ocr::Installed>,
    skip_ocr: bool,
    cancel: &AtomicBool,
) -> Result<Imported, ImportError> {
    check_cancel(cancel)?;
    let source = std::path::absolute(path).map_err(ImportError::Read)?;
    // Detect PDFs by their header, preserving renamed-file import support.
    // Only PDF conversion reads the complete buffer here; AnyDoc retains its
    // format detection and resource limits for all other inputs.
    let mut file = std::fs::File::open(&source).map_err(ImportError::Read)?;
    let mut header = [0; 1024];
    let n = std::io::Read::read(&mut file, &mut header).map_err(ImportError::Io)?;
    if header[..n].windows(5).any(|bytes| bytes == b"%PDF-") {
        return convert_pdf(source, installed, skip_ocr, cancel);
    }
    let converted =
        anydoc::to_markdown_with(&source, anydoc::Options::default().ocr(anydoc::Ocr::Skip))
            .map_err(convert_error)?;
    check_cancel(cancel)?;
    let warning = (!converted.pages_needing_ocr.is_empty()).then(|| {
        let pages = converted
            .pages_needing_ocr
            .iter()
            .map(u32::to_string)
            .collect::<Vec<_>>()
            .join(", ");
        format!(
            "Partial import: pages {pages} of {} require OCR and were skipped.",
            converted.page_count
        )
    });
    let is_docx = source
        .extension()
        .is_some_and(|extension| extension.eq_ignore_ascii_case("docx"));
    // Word comments never reach the Markdown (AnyDoc reads no comments
    // part); the note says how many were left out.
    let word = matches!(
        anydoc::Format::from_path(&source),
        Some(anydoc::Format::Docx) | None
    );
    let comments = word
        .then(|| crate::hidden_content::docx_comment_count(&source))
        .and_then(crate::hidden_content::comments_note);
    let warning = match (warning, comments) {
        (Some(pages), Some(comments)) => Some(format!("{pages} {comments}")),
        (pages, comments) => pages.or(comments),
    };
    Ok(Imported {
        ocr_configuration: None,
        source,
        markdown: converted.markdown,
        warning,
        is_pdf: converted.page_count > 0,
        is_docx,
    })
}

pub(crate) fn check_cancel(cancel: &AtomicBool) -> Result<(), ImportError> {
    if cancel.load(Ordering::Relaxed) {
        Err(ImportError::Cancelled)
    } else {
        Ok(())
    }
}

fn convert_pdf(
    source: PathBuf,
    installed: Option<&crate::ocr::Installed>,
    skip_ocr: bool,
    cancel: &AtomicBool,
) -> Result<Imported, ImportError> {
    use pdf_inspector::vision::{OcrPdfOptions, process_pdf_with_ocr_mem};
    let bytes = read_pdf(&source)?;
    let native = process_pdf_with_ocr_mem(&bytes, OcrPdfOptions::new()).map_err(pdf_error)?;
    check_cancel(cancel)?;
    let needs_ocr = !native.pages_recommended_for_ocr.is_empty();
    if needs_ocr && installed.is_none() && !skip_ocr {
        return Err(ImportError::NeedsOcr(
            source,
            native.pages_recommended_for_ocr.clone(),
            crate::recognition::text_languages(&native.markdown),
            native.page_count,
        ));
    }
    let (result, used_ocr) = if !skip_ocr && let Some(installed) = installed {
        let _permit = crate::model_work::Permit::acquire_for("text recognition")
            .map_err(|e| ImportError::OcrFailed(e.into()))?;
        crate::ocr::load_onnx_runtime(&installed.onnx)
            .map_err(|e| ImportError::OcrFailed(e.into()))?;
        match process_pdf_with_ocr_mem(&bytes, installed.options()) {
            Ok(result) => (result, true),
            Err(error) => {
                return Err(ImportError::OcrFailed(OcrFailure::Engine(
                    error.to_string(),
                )));
            }
        }
    } else {
        (native, false)
    };
    check_cancel(cancel)?;
    let mut warning_pages = if used_ocr {
        result.pages_recommending_hosted.clone()
    } else {
        result.pages_recommended_for_ocr.clone()
    };
    if used_ocr {
        warning_pages.extend(
            result
                .pages
                .iter()
                .filter(|page| !page.provenance.warnings.is_empty())
                .map(|page| page.page_number),
        );
    }
    let warning = pdf_warning(&result.markdown, warning_pages, result.page_count, used_ocr)?;
    Ok(Imported {
        ocr_configuration: if used_ocr {
            installed.map(|i| i.config.clone())
        } else {
            None
        },
        source,
        markdown: result.markdown,
        warning,
        is_pdf: true,
        is_docx: false,
    })
}

/// A PDF's bytes, within the import limit.
pub(crate) fn read_pdf(source: &Path) -> Result<Vec<u8>, ImportError> {
    const MAX_PDF_BYTES: u64 = 256 * 1024 * 1024;
    let file = std::fs::File::open(source).map_err(ImportError::Read)?;
    let mut bytes = Vec::new();
    std::io::Read::read_to_end(
        &mut std::io::Read::take(file, MAX_PDF_BYTES + 1),
        &mut bytes,
    )
    .map_err(ImportError::Io)?;
    if bytes.len() as u64 > MAX_PDF_BYTES {
        return Err(ImportError::PdfTooLarge);
    }
    Ok(bytes)
}

/// The warning for a converted PDF's `markdown`: the pages OCR may have
/// read badly, or, without OCR, the pages it skipped. Punctuation-only
/// output is refused rather than imported as a useful document.
pub(crate) fn pdf_warning(
    markdown: &str,
    mut warning_pages: Vec<u32>,
    page_count: u32,
    used_ocr: bool,
) -> Result<Option<String>, ImportError> {
    warning_pages.sort_unstable();
    warning_pages.dedup();
    if !markdown.chars().any(char::is_alphanumeric) {
        return Err(if used_ocr {
            ImportError::OcrNoText
        } else {
            ImportError::NoNativeText
        });
    }
    if warning_pages.is_empty() {
        return Ok(None);
    }
    let pages = warning_pages
        .iter()
        .map(u32::to_string)
        .collect::<Vec<_>>()
        .join(", ");
    Ok(Some(if used_ocr {
        format!(
            "OCR review needed: pages {pages} of {page_count} may be incomplete or inaccurate. Compare with the source PDF."
        )
    } else {
        format!("Partial import: pages {pages} of {page_count} require OCR and were skipped.")
    }))
}

/// Why the native text pass could not read a PDF.
pub(crate) fn pdf_error(error: pdf_inspector::vision::OcrPipelineError) -> ImportError {
    use pdf_inspector::{PdfError, vision::OcrPipelineError};
    match error {
        OcrPipelineError::Pdf(PdfError::Encrypted) => ImportError::PdfEncrypted,
        OcrPipelineError::Pdf(PdfError::Io(error)) => ImportError::Read(error),
        OcrPipelineError::Pdf(
            PdfError::Parse(_) | PdfError::InvalidStructure | PdfError::NotAPdf(_),
        ) => ImportError::Damaged,
        other => ImportError::Pdf(other.to_string()),
    }
}

const DAMAGED: &str =
    "This document could not be converted because its contents are damaged or incomplete.";

fn convert_error(error: anydoc::ConvertError) -> ImportError {
    use anydoc::ConvertError;
    match error {
        ConvertError::NeedsOcr { .. } => ImportError::ConverterNeedsOcr,
        ConvertError::Encrypted => ImportError::Encrypted,
        ConvertError::Unsupported(_) => ImportError::Unsupported,
        ConvertError::Malformed { .. } | ConvertError::MissingPart { .. } => ImportError::Damaged,
        ConvertError::ResourceLimit { .. } => ImportError::ResourceLimit,
        ConvertError::Io(error) => ImportError::Read(error),
        other => ImportError::Convert(other.to_string()),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn format_hint_names_every_converter_format() {
        assert_eq!(
            supported_formats(),
            "PDF, Word, Excel, PowerPoint, OpenDocument, RTF, EPUB, CSV or Markdown"
        );
        // Every extension the pinned AnyDoc accepts, with the family a hint
        // names it by.
        for (family, extensions) in [
            ("PDF", &["pdf"][..]),
            ("Word", &["doc", "docx", "docm"]),
            ("Excel", &["xls", "xlsx", "xlsm", "xlsb"]),
            (
                "PowerPoint",
                &["ppt", "pps", "pot", "pptx", "pptm", "ppsx", "ppsm"],
            ),
            ("OpenDocument", &["odt", "ods", "odp"]),
            ("RTF", &["rtf"]),
            ("EPUB", &["epub"]),
            ("CSV", &["csv"]),
        ] {
            for extension in extensions {
                let format = anydoc::Format::from_extension(extension).expect(extension);
                assert_eq!(FORMAT_NAMES[format_family(format)], family, "{extension}");
            }
        }
    }

    #[test]
    fn damaged_documents_get_one_plain_message() {
        let dir = tempfile::tempdir().unwrap();
        let pdf = dir.path().join("damaged.pdf");
        std::fs::write(&pdf, b"%PDF-1.7 not really").unwrap();
        assert_eq!(convert(&pdf).unwrap_err(), DAMAGED);
        assert_eq!(
            convert(&fixture("truncated--errors.docx")).unwrap_err(),
            DAMAGED
        );
    }

    fn fixture(name: &str) -> PathBuf {
        Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../../tests/fixtures/import")
            .join(name)
    }

    #[test]
    fn representative_conversions_match_reviewable_upstream_snapshots() {
        for name in [
            "text.doc",
            "text.docx",
            "handmade-tables.docx",
            "handmade-nested.docx",
            "handmade-checkboxes.docx",
            "handmade-revisions.doc",
            "sheet.xls",
            "sheet.xlsx",
            "text.pdf",
            "handmade-partly-scanned.pdf",
        ] {
            let path = fixture(name);
            let original = std::fs::read(&path).unwrap();
            let imported = convert(&path).unwrap();
            let expected = std::fs::read_to_string(fixture(&format!("{name}.md")))
                .unwrap()
                .replace("\r\n", "\n");
            let markdown = imported.markdown.replace("\r\n", "\n");
            // Insta's text snapshots end in a newline regardless of converter output.
            assert_eq!(
                markdown.trim_end_matches('\n'),
                expected.trim_end_matches('\n'),
                "{name}"
            );
            assert_eq!(std::fs::read(path).unwrap(), original);
            assert_eq!(imported.is_pdf, name.ends_with("pdf"));
            assert_eq!(imported.is_docx, name.ends_with("docx"));
            assert_eq!(
                imported.warning.is_some(),
                name == "handmade-partly-scanned.pdf"
            );
        }
    }

    /// A DOCX whose comments, deleted and moved-from changes and document
    /// properties name people: none of it reaches the Markdown, insertions
    /// are accepted, and the note counts the comments
    /// (recognition grill 2026-11-01, round 3).
    #[test]
    fn hidden_office_content_never_reaches_the_markdown() {
        let path = fixture("handmade-hidden-content.docx");
        let imported = convert(&path).unwrap();
        let expected = std::fs::read_to_string(fixture("handmade-hidden-content.docx.md"))
            .unwrap()
            .replace("\r\n", "\n");
        assert_eq!(imported.markdown.trim_end(), expected.trim_end());
        for hidden in [
            "Holloway",
            "Petrov",
            "555-0142",
            "guarantor",
            "Holloway lease",
        ] {
            assert!(!imported.markdown.contains(hidden), "{hidden}");
        }
        for accepted in ["the Client", "Landlord Ltd", "Moved sentence."] {
            assert!(imported.markdown.contains(accepted), "{accepted}");
        }
        assert_eq!(
            imported.markdown.matches("Moved sentence.").count(),
            1,
            "moved text appears once, where it now is"
        );
        assert_eq!(
            imported.warning.as_deref(),
            Some("2 comments not imported.")
        );
        // Content detection counts them too, for a renamed copy.
        let dir = tempfile::tempdir().unwrap();
        let renamed = dir.path().join("renamed.bin");
        std::fs::copy(&path, &renamed).unwrap();
        assert_eq!(convert(&renamed).unwrap().warning, imported.warning);
    }

    #[test]
    fn partial_pdf_keeps_original_page_numbers_outside_markdown() {
        let imported = convert(&fixture("handmade-partly-scanned.pdf")).unwrap();
        assert_eq!(
            imported.warning.as_deref(),
            Some("Partial import: pages 2, 5 of 5 require OCR and were skipped.")
        );
        assert!(!imported.markdown.contains("OCR"));
        let mixed = convert(&fixture("handmade-mixed.pdf")).unwrap();
        assert_eq!(
            mixed.warning.as_deref(),
            Some("Partial import: pages 2 of 2 require OCR and were skipped.")
        );
    }

    #[test]
    fn failures_are_actionable() {
        assert!(
            convert(&fixture("handmade-scanned.pdf"))
                .unwrap_err()
                .contains("Run OCR")
        );
        assert!(
            convert(&fixture("encrypted--errors.odt"))
                .unwrap_err()
                .contains("password-protected")
        );
        assert!(
            convert(&fixture("truncated--errors.docx"))
                .unwrap_err()
                .contains("damaged or incomplete")
        );
        assert!(
            convert(&fixture("missing.docx"))
                .unwrap_err()
                .contains("Could not read")
        );
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("unknown.bin");
        std::fs::write(&path, b"unknown format").unwrap();
        assert!(convert(&path).unwrap_err().contains("not supported"));
        assert!(
            convert_error(anydoc::ConvertError::ResourceLimit {
                limit: "test",
                detail: String::new()
            })
            .to_string()
            .contains("resource limits")
        );
    }

    #[test]
    fn content_detection_and_csv_extension_fallback_work() {
        let dir = tempfile::tempdir().unwrap();
        let renamed = dir.path().join("renamed.bin");
        std::fs::copy(fixture("text.docx"), &renamed).unwrap();
        assert!(supported_source(&renamed));
        assert_eq!(
            convert(&renamed).unwrap().markdown,
            convert(&fixture("text.docx")).unwrap().markdown
        );
        let csv = dir.path().join("values.CSV");
        std::fs::write(&csv, "Name,Count\nApples,2\n").unwrap();
        assert!(convert(&csv).unwrap().markdown.contains("| Apples | 2 |"));
    }

    #[test]
    fn scan_reports_the_languages_its_text_layer_suggests() {
        use crate::recognition::ENGLISH;
        let Err(ImportError::NeedsOcr(_, pages, text, _)) =
            prepare(&fixture("handmade-partly-scanned.pdf"), None, false)
        else {
            panic!("needs OCR");
        };
        assert_eq!((pages, text), (vec![2, 5], vec![ENGLISH]));
        let raster = Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../../tests/fixtures/ocr-qualification/russian.pdf");
        assert!(matches!(
            prepare(&raster, None, false),
            Err(ImportError::NeedsOcr(_, _, text, _)) if text.is_empty()
        ));
    }

    #[test]
    fn scan_requests_setup_and_explicit_skip_keeps_native_pages() {
        let path = fixture("handmade-partly-scanned.pdf");
        assert!(matches!(
            prepare(&path, None, false),
            Err(ImportError::NeedsOcr(..))
        ));
        let imported = prepare(&path, None, true).unwrap();
        assert!(imported.markdown.contains("Readable page three"));
        assert!(imported.warning.unwrap().contains("2, 5"));
        let dir = tempfile::tempdir().unwrap();
        let renamed = dir.path().join("scan.bin");
        std::fs::copy(fixture("handmade-scanned.pdf"), &renamed).unwrap();
        assert!(matches!(
            prepare(&renamed, None, false),
            Err(ImportError::NeedsOcr(..))
        ));
        assert!(prepare(&renamed, None, true).is_err());
    }

    #[test]
    fn missing_runtime_requires_explicit_fallback() {
        let installed = crate::ocr::Installed {
            config: mdoc_store::settings::OcrConfig::default(),
            models: "missing-models".into(),
            pdfium: "missing-pdfium".into(),
            onnx: "missing-onnx".into(),
        };
        assert!(matches!(
            prepare(
                &fixture("handmade-partly-scanned.pdf"),
                Some(&installed),
                false,
            ),
            Err(ImportError::OcrFailed(_))
        ));
        let skipped = prepare(
            &fixture("handmade-partly-scanned.pdf"),
            Some(&installed),
            true,
        )
        .unwrap();
        assert!(skipped.markdown.contains("Readable page three"));
        assert!(matches!(
            prepare(&fixture("handmade-scanned.pdf"), Some(&installed), false),
            Err(ImportError::OcrFailed(_))
        ));
    }
}
