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
    /// The notice: a partial import (pages OCR skipped, or may have read
    /// badly).
    pub warning: Option<String>,
    /// What else the Markdown leaves out, counted for the left-out line.
    pub left_out: LeftOut,
    pub is_pdf: bool,
    pub is_docx: bool,
}

/// What the Markdown of an import leaves out besides skipped pages.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct LeftOut {
    /// Pictures: the Markdown carries their alt text at most, and OCR reads
    /// the text drawn in a picture, not the picture.
    pub pictures: u64,
    /// Passages of hidden-formatted Word text, or stretches of invisible PDF
    /// text (render mode 3, a scan's hidden OCR layer included).
    pub hidden_text: u64,
    /// AcroForm field names; the fields' values stay.
    pub field_names: u64,
    /// A Word or ODT document's comments, which only a rendering shows.
    pub comments: crate::hidden_content::Comments,
}

impl LeftOut {
    /// What a PDF's pages hold that its Markdown leaves out, from its native
    /// pass, which OCR doesn't change.
    pub(crate) fn of_pdf(native: &pdf_inspector::vision::OcrPdfResult) -> Self {
        fn sum<T>(pages: &[T], count: impl Fn(&T) -> u32) -> u64 {
            pages.iter().map(|page| u64::from(count(page))).sum()
        }
        Self {
            pictures: sum(&native.images_by_page, |page| page.images),
            hidden_text: sum(&native.invisible_text_by_page, |page| page.stretches),
            field_names: sum(&native.form_field_names_by_page, |page| page.names),
            comments: Default::default(),
        }
    }

    /// The note's lines for the counts other than comments, in order.
    fn counts(&self) -> [Option<String>; 3] {
        [
            pictures_note(self.pictures),
            hidden_text_note(self.hidden_text),
            left_out_note(self.field_names, "form field name", "form field names"),
        ]
    }

    /// The left-out line, if anything is left out. Comments count only where
    /// no rendering of the document shows them (`comments_shown` false).
    pub fn line(&self, comments_shown: bool) -> Option<String> {
        let [pictures, hidden, field_names] = self.counts();
        join_notes([
            pictures,
            hidden,
            field_names,
            (!comments_shown)
                .then(|| self.comments.not_shown())
                .flatten(),
        ])
    }
}

impl Imported {
    /// The notice and the counts as one note, as the apps show it until
    /// they show the left-out line (ROADMAP Comments item 4): the order the
    /// note had before the split.
    pub fn note(&self) -> Option<String> {
        let [pictures, hidden, field_names] = self.left_out.counts();
        join_notes([
            self.warning.clone(),
            self.left_out.comments.note(),
            pictures,
            hidden,
            field_names,
        ])
    }
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
    // Every PDF goes to pdf-inspector: AnyDoc, built without its `pdf`
    // feature, would refuse one, and detects a PDF by the same window.
    if header[..n].windows(5).any(|bytes| bytes == b"%PDF-") {
        return convert_pdf(source, installed, skip_ocr, cancel);
    }
    let converted =
        anydoc::to_markdown_with(&source, anydoc::Options::default()).map_err(convert_error)?;
    check_cancel(cancel)?;
    let is_docx = source
        .extension()
        .is_some_and(|extension| extension.eq_ignore_ascii_case("docx"));
    // Comments never reach the Markdown (AnyDoc reads no Word comments part
    // and skips ODT annotations); the left-out line says how many.
    // AnyDoc picks the parser by content, so both counters look at the
    // package whatever its extension; each counts nothing in the other's.
    let comments = crate::hidden_content::docx_comments(&source)
        .and(crate::hidden_content::odt_comments(&source));
    // Nor do Word's pictures: embedded ones render as their alt text at
    // most (ROADMAP Vision item 1).
    let format = anydoc::Format::from_path(&source);
    let pictures = if matches!(format, Some(anydoc::Format::Docx) | None) {
        u64::from(converted.images_left_out)
    } else {
        0
    };
    Ok(Imported {
        ocr_configuration: None,
        source,
        markdown: converted.markdown,
        // Only a PDF's import can be partial.
        warning: None,
        left_out: LeftOut {
            pictures,
            // Nor does hidden-formatted text (DOCX, RTF and DOC; 0
            // elsewhere).
            hidden_text: u64::from(converted.hidden_text_left_out),
            field_names: 0,
            comments,
        },
        is_pdf: false,
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
    let left_out = LeftOut::of_pdf(&native);
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
        left_out,
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

/// The import note's line for `count` things that did not reach the
/// Markdown, if any: "3 pictures not imported."
fn left_out_note(count: u64, one: &str, many: &str) -> Option<String> {
    match count {
        0 => None,
        1 => Some(format!("1 {one} not imported.")),
        n => Some(format!("{n} {many} not imported.")),
    }
}

/// The import note's line for pictures that did not reach the Markdown, if
/// any.
pub(crate) fn pictures_note(count: u64) -> Option<String> {
    left_out_note(count, "picture", "pictures")
}

/// The import note's line for hidden or invisible text left out, counted in
/// stretches (passages), if any.
pub(crate) fn hidden_text_note(count: u64) -> Option<String> {
    left_out_note(count, "hidden passage", "hidden passages")
}

/// An import note from its lines, in order: none when every line is.
pub(crate) fn join_notes<const N: usize>(lines: [Option<String>; N]) -> Option<String> {
    let lines: Vec<String> = lines.into_iter().flatten().collect();
    (!lines.is_empty()).then(|| lines.join(" "))
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

    /// Office documents through AnyDoc, and PDFs through pdf-inspector's
    /// native pass, against their reviewed snapshots.
    #[test]
    fn representative_conversions_match_reviewable_upstream_snapshots() {
        for name in [
            "text.doc",
            "text.docx",
            "handmade-tables.docx",
            "handmade-nested.docx",
            "handmade-checkboxes.docx",
            "handmade-revisions.doc",
            "handmade-hidden.docx",
            "handmade-hidden.rtf",
            "handmade-hidden.doc",
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
            let expected_note = match name {
                "text.docx" | "text.pdf" => Some("1 picture not imported."),
                "handmade-partly-scanned.pdf" => Some(
                    "Partial import: pages 2, 5 of 5 require OCR and were skipped. 2 pictures not imported.",
                ),
                "handmade-hidden.docx" => Some("7 hidden passages not imported."),
                "handmade-hidden.rtf" => Some("6 hidden passages not imported."),
                "handmade-hidden.doc" => Some("3 hidden passages not imported."),
                _ => None,
            };
            assert_eq!(imported.note().as_deref(), expected_note, "{name}");
        }
    }

    /// AnyDoc is built without its PDF converter: every PDF goes to
    /// pdf-inspector, so AnyDoc's own PDF path is not even in the build.
    #[test]
    fn anydoc_never_converts_a_pdf() {
        for name in ["text.pdf", "handmade-partly-scanned.pdf"] {
            assert!(
                matches!(
                    anydoc::to_markdown_with(fixture(name), anydoc::Options::default()),
                    Err(anydoc::ConvertError::Unsupported(_))
                ),
                "{name}"
            );
            assert!(convert(&fixture(name)).unwrap().is_pdf, "{name}");
        }
    }

    /// Hidden-formatted Word text never reaches the Markdown and the note
    /// counts its stretches (#925 decision 9); so does a renamed copy.
    #[test]
    fn the_note_counts_hidden_text_left_out() {
        for (name, count) in [
            ("handmade-hidden.docx", 7),
            ("handmade-hidden.rtf", 6),
            ("handmade-hidden.doc", 3),
        ] {
            let imported = convert(&fixture(name)).unwrap();
            assert!(!imported.markdown.contains("SECRET"), "{name}");
            let note = format!("{count} hidden passages not imported.");
            assert_eq!(imported.note().as_deref(), Some(note.as_str()), "{name}");
            let dir = tempfile::tempdir().unwrap();
            let renamed = dir.path().join("renamed.bin");
            std::fs::copy(fixture(name), &renamed).unwrap();
            assert_eq!(convert(&renamed).unwrap().left_out, imported.left_out);
        }
        assert_eq!(hidden_text_note(0), None);
        assert_eq!(
            hidden_text_note(1).as_deref(),
            Some("1 hidden passage not imported.")
        );
    }

    /// One page with visible text, an invisible (render mode 3) passage, a
    /// text field named `txtClientName` and a checked box named `chkAgree`.
    fn pdf_with_hidden_text_and_fields() -> Vec<u8> {
        let content = "BT /F1 12 Tf 72 700 Td (Client:) Tj ET\n\
             BT /F1 12 Tf 3 Tr 72 680 Td (SECRET-LAYER) Tj ET\n\
             BT /F1 12 Tf 0 Tr 72 660 Td (Visible clause.) Tj ET";
        let objects = [
            "<< /Type /Catalog /Pages 2 0 R /AcroForm << /Fields [5 0 R 6 0 R] >> >>".to_owned(),
            "<< /Type /Pages /Kids [3 0 R] /Count 1 >>".to_owned(),
            "<< /Type /Page /Parent 2 0 R /MediaBox [0 0 612 792] \
             /Resources << /Font << /F1 4 0 R >> >> /Contents 7 0 R /Annots [5 0 R 6 0 R] >>"
                .to_owned(),
            "<< /Type /Font /Subtype /Type1 /BaseFont /Helvetica >>".to_owned(),
            "<< /Type /Annot /Subtype /Widget /FT /Tx /T (txtClientName) \
             /V (Ivanov Ivan) /Rect [130 695 330 715] /P 3 0 R >>"
                .to_owned(),
            "<< /Type /Annot /Subtype /Widget /FT /Btn /T (chkAgree) \
             /V /Yes /Rect [72 620 84 632] /P 3 0 R >>"
                .to_owned(),
            format!(
                "<< /Length {} >>\nstream\n{content}\nendstream",
                content.len()
            ),
        ];
        let mut pdf = b"%PDF-1.4\n".to_vec();
        let mut offsets = Vec::new();
        for (index, object) in objects.iter().enumerate() {
            offsets.push(pdf.len());
            pdf.extend_from_slice(format!("{} 0 obj\n{object}\nendobj\n", index + 1).as_bytes());
        }
        let xref = pdf.len();
        pdf.extend_from_slice(
            format!("xref\n0 {}\n0000000000 65535 f \n", offsets.len() + 1).as_bytes(),
        );
        for offset in offsets {
            pdf.extend_from_slice(format!("{offset:010} 00000 n \n").as_bytes());
        }
        pdf.extend_from_slice(
            format!(
                "trailer\n<< /Size {} /Root 1 0 R >>\nstartxref\n{xref}\n%%EOF",
                objects.len() + 1
            )
            .as_bytes(),
        );
        pdf
    }

    /// Invisible PDF text and form field names never reach the Markdown;
    /// the note counts them after the pictures (#925 decision 9). The
    /// fields' values stay.
    #[test]
    fn the_note_counts_invisible_pdf_text_and_form_field_names() {
        let dir = tempfile::tempdir().unwrap();
        let pdf = dir.path().join("form.pdf");
        std::fs::write(&pdf, pdf_with_hidden_text_and_fields()).unwrap();
        let imported = convert(&pdf).unwrap();
        for hidden in ["SECRET", "txtClientName", "chkAgree"] {
            assert!(!imported.markdown.contains(hidden), "{}", imported.markdown);
        }
        for kept in ["Client:", "Ivanov Ivan", "Visible clause."] {
            assert!(imported.markdown.contains(kept), "{}", imported.markdown);
        }
        assert_eq!(imported.warning, None);
        assert_eq!(
            imported.note().as_deref(),
            Some("1 hidden passage not imported. 2 form field names not imported.")
        );
        let left_out = LeftOut {
            pictures: 3,
            hidden_text: 2,
            field_names: 1,
            ..LeftOut::default()
        };
        assert_eq!(
            left_out.line(true).as_deref(),
            Some(
                "3 pictures not imported. 2 hidden passages not imported. 1 form field name not imported."
            )
        );
        assert_eq!(LeftOut::default().line(false), None);
    }

    /// ODT comments are left out of the Markdown (AnyDoc skips
    /// `office:annotation`), so the note counts them as it does Word's.
    #[test]
    fn the_note_counts_odt_comments() {
        use std::io::Write;
        let content = r#"<?xml version="1.0" encoding="UTF-8"?>
<office:document-content xmlns:office="urn:oasis:names:tc:opendocument:xmlns:office:1.0" xmlns:text="urn:oasis:names:tc:opendocument:xmlns:text:1.0" xmlns:dc="http://purl.org/dc/elements/1.1/"><office:body><office:text>
<text:p>Lamb<office:annotation office:name="a"><dc:creator>Margaret Holloway</dc:creator><text:p>COMMENTTEXT</text:p></office:annotation>da<office:annotation-end office:name="a"/>word</text:p>
<text:p>Second<office:annotation><text:p>Another</text:p></office:annotation> paragraph.</text:p>
</office:text></office:body></office:document-content>"#;
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("comments.odt");
        let mut zip = zip::ZipWriter::new(std::fs::File::create(&path).unwrap());
        let stored = zip::write::SimpleFileOptions::default()
            .compression_method(zip::CompressionMethod::Stored);
        zip.start_file("mimetype", stored).unwrap();
        zip.write_all(b"application/vnd.oasis.opendocument.text")
            .unwrap();
        zip.start_file("content.xml", zip::write::SimpleFileOptions::default())
            .unwrap();
        zip.write_all(content.as_bytes()).unwrap();
        zip.finish().unwrap();
        let imported = convert(&path).unwrap();
        assert_eq!(imported.markdown.trim(), "Lambdaword\n\nSecond paragraph.");
        assert_eq!(imported.note().as_deref(), Some("2 comments not imported."));
        // No rendering shows an ODT's comments.
        assert_eq!(
            imported.left_out.line(false).as_deref(),
            Some("2 comments not shown.")
        );
        // AnyDoc reads the content, whatever the extension says; so does
        // the count.
        for name in ["renamed.bin", "renamed.docx", "renamed.rtf"] {
            let renamed = dir.path().join(name);
            std::fs::copy(&path, &renamed).unwrap();
            let converted = convert(&renamed).unwrap();
            assert_eq!(converted.markdown, imported.markdown, "{name}");
            assert_eq!(converted.left_out, imported.left_out, "{name}");
        }
    }

    /// Pictures never reach the Markdown, so the note counts them, for PDF
    /// and Word (ROADMAP Vision item 1); a document without any gets no
    /// line. A scanned page's image counts even though OCR may read it.
    #[test]
    fn the_note_counts_pictures_left_out() {
        let note = |name: &str| convert(&fixture(name)).unwrap().note();
        assert_eq!(
            note("text.docx").as_deref(),
            Some("1 picture not imported.")
        );
        assert_eq!(note("handmade-tables.docx"), None);
        // `text.docx` saved as PDF keeps its picture; the DocLayNet page
        // draws none.
        assert_eq!(note("text.pdf").as_deref(), Some("1 picture not imported."));
        assert_eq!(note("../ocr-upstream/en-doclaynet-law-4.native.pdf"), None);
        assert_eq!(
            note("handmade-partly-scanned.pdf").as_deref(),
            Some(
                "Partial import: pages 2, 5 of 5 require OCR and were skipped. 2 pictures not imported."
            )
        );
        // A picture the package holds but the document never shows is not
        // left out of anything.
        let comments = Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../../tests/fixtures/docx-preview/comments.docx");
        assert_eq!(
            convert(&comments).unwrap().note().as_deref(),
            Some("6 comments not imported.")
        );
    }

    /// The notice keeps only a partial import; the counts are the left-out
    /// line, where comments count only without a rendering to show them.
    #[test]
    fn the_note_splits_into_the_notice_and_the_left_out_line() {
        let partly = convert(&fixture("handmade-partly-scanned.pdf")).unwrap();
        assert_eq!(
            partly.warning.as_deref(),
            Some("Partial import: pages 2, 5 of 5 require OCR and were skipped.")
        );
        assert_eq!(
            partly.left_out.line(true).as_deref(),
            Some("2 pictures not imported.")
        );
        let comments = convert(
            &Path::new(env!("CARGO_MANIFEST_DIR"))
                .join("../../tests/fixtures/docx-preview/comments.docx"),
        )
        .unwrap();
        assert_eq!(comments.warning, None);
        assert_eq!(comments.left_out.line(true), None);
        assert_eq!(
            comments.left_out.line(false).as_deref(),
            Some("6 comments not shown.")
        );
        let all = LeftOut {
            pictures: 1,
            hidden_text: 2,
            field_names: 3,
            comments: crate::hidden_content::Comments {
                count: 2,
                at_least: false,
            },
        };
        assert_eq!(
            all.line(false).as_deref(),
            Some(
                "1 picture not imported. 2 hidden passages not imported. \
                 3 form field names not imported. 2 comments not shown."
            )
        );
        // Hidden text is left out of the Markdown whether or not the Source
        // shows it.
        let hidden = convert(&fixture("handmade-hidden.docx")).unwrap();
        assert_eq!(hidden.warning, None);
        assert_eq!(
            hidden.left_out.line(true).as_deref(),
            Some("7 hidden passages not imported.")
        );
        // The whole note the apps show keeps its old order: comments first.
        let whole = Imported {
            left_out: all,
            ..comments
        };
        assert_eq!(
            whole.note().as_deref(),
            Some(
                "2 comments not imported. 1 picture not imported.                  2 hidden passages not imported. 3 form field names not imported."
            )
        );
    }

    #[test]
    fn picture_note_names_the_count() {
        assert_eq!(pictures_note(0), None);
        assert_eq!(pictures_note(1).as_deref(), Some("1 picture not imported."));
        assert_eq!(
            pictures_note(3).as_deref(),
            Some("3 pictures not imported.")
        );
        assert_eq!(join_notes::<2>([None, None]), None);
        assert_eq!(
            join_notes([Some("A.".into()), None, Some("B.".into())]).as_deref(),
            Some("A. B.")
        );
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
        assert_eq!(imported.note().as_deref(), Some("2 comments not imported."));
        assert_eq!(imported.left_out.comments.count, 2);
        // Content detection counts them too, for a renamed copy.
        let dir = tempfile::tempdir().unwrap();
        for name in ["renamed.bin", "renamed.odt"] {
            let renamed = dir.path().join(name);
            std::fs::copy(&path, &renamed).unwrap();
            assert_eq!(convert(&renamed).unwrap().left_out, imported.left_out);
        }
    }

    #[test]
    fn partial_pdf_keeps_original_page_numbers_outside_markdown() {
        let imported = convert(&fixture("handmade-partly-scanned.pdf")).unwrap();
        assert_eq!(
            imported.note().as_deref(),
            Some(
                "Partial import: pages 2, 5 of 5 require OCR and were skipped. 2 pictures not imported."
            )
        );
        assert!(!imported.markdown.contains("OCR"));
        let mixed = convert(&fixture("handmade-mixed.pdf")).unwrap();
        assert_eq!(
            mixed.note().as_deref(),
            Some(
                "Partial import: pages 2 of 2 require OCR and were skipped. 1 picture not imported."
            )
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
