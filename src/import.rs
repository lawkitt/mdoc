//! The only module that knows AnyDoc. Conversion performs no writes.
use std::path::{Path, PathBuf};

#[derive(Clone, Debug)]
pub struct Imported {
    pub source: PathBuf,
    pub markdown: String,
    pub warning: Option<String>,
    pub is_pdf: bool,
    pub is_docx: bool,
}

pub fn convert(path: &Path) -> Result<Imported, String> {
    let source = std::path::absolute(path).map_err(|e| format!("Could not read document: {e}"))?;
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
        is_pdf: converted.page_count > 0,
        is_docx,
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
}
