//! Conversion boundary for AnyDoc and PDF extraction. Conversion performs no writes.
use std::path::{Path, PathBuf};

#[derive(Clone, Debug)]
pub struct Imported {
    pub source: PathBuf,
    pub markdown: String,
    pub warning: Option<String>,
    pub ocr_failure: Option<String>,
    pub is_pdf: bool,
    pub is_docx: bool,
}

#[derive(Debug)]
pub enum ImportError {
    NeedsOcr(PathBuf),
    Message(String),
    OcrFailed(String),
}

impl From<String> for ImportError {
    fn from(message: String) -> Self {
        Self::Message(message)
    }
}
impl From<&str> for ImportError {
    fn from(message: &str) -> Self {
        Self::Message(message.into())
    }
}

#[cfg(test)]
pub fn convert(path: &Path) -> Result<Imported, String> {
    prepare(path, None, true).map_err(|error| match error {
        ImportError::Message(message) | ImportError::OcrFailed(message) => message,
        ImportError::NeedsOcr(_) => "This PDF requires OCR.".into(),
    })
}

pub fn prepare(
    path: &Path,
    installed: Option<&crate::ocr::Installed>,
    skip_ocr: bool,
) -> Result<Imported, ImportError> {
    let source = std::path::absolute(path).map_err(|e| format!("Could not read document: {e}"))?;
    // Detect PDFs by their header, preserving renamed-file import support.
    // Only PDF conversion reads the complete buffer here; AnyDoc retains its
    // format detection and resource limits for all other inputs.
    let mut file =
        std::fs::File::open(&source).map_err(|e| format!("Could not read document: {e}"))?;
    let mut header = [0; 1024];
    let n = std::io::Read::read(&mut file, &mut header).map_err(|e| e.to_string())?;
    if header[..n].windows(5).any(|bytes| bytes == b"%PDF-") {
        return convert_pdf(source, installed, skip_ocr);
    }
    let converted =
        anydoc::to_markdown_with(&source, anydoc::Options::default().ocr(anydoc::Ocr::Skip))
            .map_err(message)?;
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
    Ok(Imported {
        source,
        markdown: converted.markdown,
        warning,
        ocr_failure: None,
        is_pdf: converted.page_count > 0,
        is_docx,
    })
}

fn convert_pdf(
    source: PathBuf,
    installed: Option<&crate::ocr::Installed>,
    skip_ocr: bool,
) -> Result<Imported, ImportError> {
    use pdf_inspector::vision::{OcrPdfOptions, process_pdf_with_ocr_mem};
    const MAX_PDF_BYTES: u64 = 256 * 1024 * 1024;
    let file = std::fs::File::open(&source).map_err(|e| format!("Could not read document: {e}"))?;
    let mut bytes = Vec::new();
    std::io::Read::read_to_end(
        &mut std::io::Read::take(file, MAX_PDF_BYTES + 1),
        &mut bytes,
    )
    .map_err(|e| e.to_string())?;
    if bytes.len() as u64 > MAX_PDF_BYTES {
        return Err("This PDF exceeds the 256 MiB import limit.".into());
    }
    let native = process_pdf_with_ocr_mem(&bytes, OcrPdfOptions::new()).map_err(|e| {
        format!("Could not import PDF. If password-protected, import an unlocked copy: {e}")
    })?;
    let needs_ocr = !native.pages_recommended_for_ocr.is_empty();
    if needs_ocr && installed.is_none() && !skip_ocr {
        return Err(ImportError::NeedsOcr(source));
    }
    let (result, used_ocr, failure) = if needs_ocr
        && !skip_ocr
        && let Some(installed) = installed
    {
        match process_pdf_with_ocr_mem(&bytes, installed.options()) {
            Ok(result) => (result, true, None),
            Err(error) => (
                native,
                false,
                Some(format!("Local OCR failed: {error}. Retry OCR setup.")),
            ),
        }
    } else {
        (native, false, None)
    };
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
    warning_pages.sort_unstable();
    warning_pages.dedup();
    // Avoid importing punctuation-only failure output as a useful document.
    if !result.markdown.chars().any(char::is_alphanumeric) {
        return Err(match failure {
            Some(error) => ImportError::OcrFailed(error),
            None => "This PDF requires OCR, but no usable text was extracted. Set up OCR and try again, or use a clearer scan.".into(),
        });
    }
    let ocr_failure = failure.clone();
    let warning = if warning_pages.is_empty() {
        failure
    } else {
        let pages = warning_pages
            .iter()
            .map(u32::to_string)
            .collect::<Vec<_>>()
            .join(", ");
        let message = if used_ocr {
            format!(
                "OCR review needed: pages {pages} of {} may be incomplete or inaccurate. Compare with the source PDF.",
                result.page_count
            )
        } else {
            format!(
                "Partial import: pages {pages} of {} require OCR and were skipped.",
                result.page_count
            )
        };
        Some(match failure {
            Some(failure) => format!("{message} {failure}"),
            None => message,
        })
    };
    Ok(Imported {
        source,
        markdown: result.markdown,
        warning,
        ocr_failure,
        is_pdf: true,
        is_docx: false,
    })
}

fn message(error: anydoc::ConvertError) -> String {
    use anydoc::ConvertError;
    match error {
        ConvertError::NeedsOcr { .. } => "This PDF requires OCR. Run text recognition in another application, then import it again.".into(),
        ConvertError::Encrypted => "This document is password-protected. Import an unlocked copy.".into(),
        ConvertError::Unsupported(_) => "This format is not supported. Choose a PDF, Office, OpenDocument, RTF, EPUB, or CSV file.".into(),
        ConvertError::Malformed { .. } | ConvertError::MissingPart { .. } => "This document could not be converted because its contents are damaged or incomplete.".into(),
        ConvertError::ResourceLimit { .. } => "This document exceeds the converter’s resource limits. Try importing a smaller document.".into(),
        ConvertError::Io(error) => format!("Could not read document: {error}"),
        other => format!("Could not import document: {other}"),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn fixture(name: &str) -> PathBuf {
        Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("tests/fixtures/import")
            .join(name)
    }

    #[test]
    fn representative_conversions_match_reviewable_upstream_snapshots() {
        for name in [
            "text.doc",
            "text.docx",
            "handmade-tables.docx",
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
            assert_eq!(
                imported.warning.is_some(),
                name == "handmade-partly-scanned.pdf"
            );
        }
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
                .contains("requires OCR")
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
            message(anydoc::ConvertError::ResourceLimit {
                limit: "test",
                detail: String::new()
            })
            .contains("resource limits")
        );
    }

    #[test]
    fn content_detection_and_csv_extension_fallback_work() {
        let dir = tempfile::tempdir().unwrap();
        let renamed = dir.path().join("renamed.bin");
        std::fs::copy(fixture("text.docx"), &renamed).unwrap();
        assert_eq!(
            convert(&renamed).unwrap().markdown,
            convert(&fixture("text.docx")).unwrap().markdown
        );
        let csv = dir.path().join("values.CSV");
        std::fs::write(&csv, "Name,Count\nApples,2\n").unwrap();
        assert!(convert(&csv).unwrap().markdown.contains("| Apples | 2 |"));
    }

    #[test]
    fn scan_requests_setup_and_explicit_skip_keeps_native_pages() {
        let path = fixture("handmade-partly-scanned.pdf");
        assert!(matches!(
            prepare(&path, None, false),
            Err(ImportError::NeedsOcr(_))
        ));
        let imported = prepare(&path, None, true).unwrap();
        assert!(imported.markdown.contains("Readable page three"));
        assert!(imported.warning.unwrap().contains("2, 5"));
        let dir = tempfile::tempdir().unwrap();
        let renamed = dir.path().join("scan.bin");
        std::fs::copy(fixture("handmade-scanned.pdf"), &renamed).unwrap();
        assert!(matches!(
            prepare(&renamed, None, false),
            Err(ImportError::NeedsOcr(_))
        ));
        assert!(prepare(&renamed, None, true).is_err());
    }

    #[test]
    fn missing_runtime_preserves_native_content_and_reports_setup_failure() {
        let installed = crate::ocr::Installed {
            models: "missing-models".into(),
            pdfium: "missing-pdfium".into(),
            onnx: "missing-onnx".into(),
        };
        let partial = prepare(
            &fixture("handmade-partly-scanned.pdf"),
            Some(&installed),
            false,
        )
        .unwrap();
        assert!(partial.markdown.contains("Readable page three"));
        assert!(partial.ocr_failure.is_some());
        assert!(partial.warning.unwrap().contains("Local OCR failed"));
        let skipped = prepare(
            &fixture("handmade-partly-scanned.pdf"),
            Some(&installed),
            true,
        )
        .unwrap();
        assert!(
            skipped.ocr_failure.is_none(),
            "explicit skip must not load a runtime"
        );
        assert!(matches!(
            prepare(&fixture("handmade-scanned.pdf"), Some(&installed), false),
            Err(ImportError::OcrFailed(_))
        ));
    }
}
