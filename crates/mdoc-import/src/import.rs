//! Conversion boundary for AnyDoc and PDF extraction. Conversion performs no writes.
use crate::comments::Comment;
use std::{
    io::Read,
    path::{Path, PathBuf},
    sync::atomic::{AtomicBool, Ordering},
};

/// Whether a file dragged from the OS looks openable by its extension alone;
/// drag feedback cannot afford `supported_source`'s content reads.
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
/// case. One stat per dragged path.
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
    /// The source's size and modification time as the import opened it:
    /// its comments describe the file only while the file still has them.
    pub stamp: Option<SourceStamp>,
    pub markdown: String,
    /// The notice: a partial import (pages OCR skipped, or may have read
    /// badly).
    pub warning: Option<String>,
    /// What else the Markdown leaves out, counted for the left-out line.
    pub left_out: LeftOut,
    /// The comments AnyDoc read from a DOCX, ODT or RTF, in document order;
    /// the Source rail's cards. Never in the Markdown.
    pub comments: Vec<Comment>,
    /// A PDF's annotations that carry text (notes, FreeText, text markup),
    /// from its native pass whether or not OCR ran. Never in the Markdown.
    pub annotations: Vec<pdf_inspector::PdfAnnotation>,
    pub is_pdf: bool,
    pub is_docx: bool,
}

/// A file's size and modification time: a later reading of the same path
/// is taken to be of the same bytes while its stamp matches.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct SourceStamp {
    len: u64,
    modified: Option<std::time::SystemTime>,
}

impl SourceStamp {
    /// The stamp of the file at `path` now, if it can be read.
    pub fn of(path: &Path) -> Option<Self> {
        std::fs::metadata(path)
            .ok()
            .map(|metadata| Self::from(&metadata))
    }
}

impl From<&std::fs::Metadata> for SourceStamp {
    fn from(metadata: &std::fs::Metadata) -> Self {
        Self {
            len: metadata.len(),
            modified: metadata.modified().ok(),
        }
    }
}

/// What the Markdown of an import leaves out besides skipped pages.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct LeftOut {
    /// Pictures: the Markdown carries their alt text at most, and OCR reads
    /// the text drawn in a picture, not the picture.
    pub pictures: u64,
    /// Passages of hidden-formatted Word text, or stretches of PDF text no
    /// reader sees: invisible (render mode 3, a scan's hidden OCR layer
    /// included), or drawn in its background's colour or covered by a
    /// later fill or image.
    pub hidden_text: u64,
    /// AcroForm field names; the fields' values stay.
    pub field_names: u64,
    /// A PDF's running header and footer rows left out as exact repeats of
    /// a row kept on an earlier page.
    pub repeated_lines: u64,
    /// Comments read as data, which a Source rail can show: `Imported`'s
    /// comments or annotations. Without a rail they count in the line.
    pub comments: u64,
    /// Comments only counted (DOC, PPTX, XLSX, ODS, ODP, and past AnyDoc's
    /// per-document cap): no card shows them, so they always count.
    pub comments_unread: u64,
}

impl LeftOut {
    /// What a PDF's pages hold that its Markdown leaves out, from its native
    /// pass, which OCR doesn't change. Pictures on the `skipped` pages (left
    /// for OCR that didn't run) don't count: the partial-import notice
    /// already reports those pages whole (#949). Once OCR reads a page, its
    /// pictures count again, as OCR keeps their text, not the pictures.
    /// Repeated header lines are not among them: OCR finds its own, so they
    /// come from the result imported ([`repeated_lines`]). Annotations count
    /// as comments, which the Source's rail shows.
    pub(crate) fn of_pdf(native: &pdf_inspector::vision::OcrPdfResult, skipped: &[u32]) -> Self {
        fn sum<T>(pages: &[T], count: impl Fn(&T) -> u32) -> u64 {
            pages.iter().map(|page| u64::from(count(page))).sum()
        }
        let skipped: std::collections::HashSet<u32> = skipped.iter().copied().collect();
        Self {
            pictures: sum(&native.images_by_page, |page| {
                if skipped.contains(&page.page) {
                    0
                } else {
                    page.images
                }
            }),
            hidden_text: sum(&native.invisible_text_by_page, |page| page.stretches)
                + sum(&native.hidden_text_by_page, |page| page.stretches),
            field_names: sum(&native.form_field_names_by_page, |page| page.names),
            repeated_lines: 0,
            comments: native.annotations.len() as u64,
            comments_unread: 0,
        }
    }

    /// The note's lines for the counts other than comments, in order.
    fn counts(&self) -> [Option<String>; 4] {
        [
            pictures_note(self.pictures),
            hidden_text_note(self.hidden_text),
            left_out_note(self.field_names, "form field name", "form field names"),
            repeated_lines_note(self.repeated_lines),
        ]
    }

    /// The left-out line, if anything is left out. Comments read as data
    /// count only where no rendering of the document shows them
    /// (`comments_shown` false); comments only counted always do.
    pub fn line(&self, comments_shown: bool) -> Option<String> {
        let [pictures, hidden, field_names, repeated] = self.counts();
        let comments = self.comments_unread + if comments_shown { 0 } else { self.comments };
        join_notes([
            pictures,
            hidden,
            field_names,
            repeated,
            not_shown_note(comments),
        ])
    }

    /// The left-out line's count of the comments read as data, for a
    /// document whose Source shows none: "3 comments not shown."
    pub fn comments_not_shown(&self) -> Option<String> {
        not_shown_note(self.comments)
    }
}

impl Imported {
    /// The notice and the counts as one note, as the apps show it until
    /// they show the left-out line (ROADMAP Comments item 4): the order the
    /// note had before the split.
    pub fn note(&self) -> Option<String> {
        let [pictures, hidden, field_names, repeated] = self.left_out.counts();
        join_notes([
            self.warning.clone(),
            left_out_note(
                self.left_out.comments + self.left_out.comments_unread,
                "comment",
                "comments",
            ),
            pictures,
            hidden,
            field_names,
            repeated,
        ])
    }
}

/// The left-out line's phrase for `count` comments no rendering shows.
fn not_shown_note(count: u64) -> Option<String> {
    match count {
        0 => None,
        1 => Some("1 comment not shown.".into()),
        n => Some(format!("{n} comments not shown.")),
    }
}

/// The comments of the office document at `path`, read as its import reads
/// them: the Source's cards for a document this session did not import.
/// None when AnyDoc cannot convert it.
pub fn source_comments(path: &Path) -> Vec<Comment> {
    source_comments_cancellable(path, &AtomicBool::new(false))
}

pub(crate) fn source_comments_cancellable(path: &Path, cancel: &AtomicBool) -> Vec<Comment> {
    convert_document(path, cancel)
        .map(|converted| converted.comments)
        .unwrap_or_default()
}

/// The left-out line's phrase for the comments of the office document at
/// `path` that its rendering would show: for an original that could not
/// render, whose import counted them there instead of in the line.
pub fn source_comments_not_shown(path: &Path) -> Option<String> {
    not_shown_note(source_comments(path).len() as u64)
}

/// The annotations of the PDF at `path` that carry text, read by the same
/// native pass its import makes: the Source's cards for a PDF this session
/// did not import. `None` when the pass cannot read it (unreadable, past
/// the import's size limit, encrypted or damaged).
pub fn source_annotations(path: &Path) -> Option<Vec<pdf_inspector::PdfAnnotation>> {
    use pdf_inspector::vision::{OcrPdfOptions, process_pdf_with_ocr_mem};
    let bytes = read_pdf(path).ok()?;
    process_pdf_with_ocr_mem(&bytes, OcrPdfOptions::new())
        .ok()
        .map(|native| native.annotations)
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
    /// The converter crashed, panicked or sent no usable result; the detail
    /// is for diagnosis, not the message.
    #[error(
        "The converter stopped unexpectedly on this document. It may be damaged or too complex to convert; mdoc kept running and nothing was imported."
    )]
    ConverterFailed(String),
    #[error(
        "Converting this document took longer than {} minutes and was stopped. It may be too large or complex to convert.",
        CONVERSION_TIMEOUT.as_secs() / 60
    )]
    ConverterTimeout,
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
    let stamp = file
        .metadata()
        .ok()
        .map(|metadata| SourceStamp::from(&metadata));
    let mut header = [0; 1024];
    let n = std::io::Read::read(&mut file, &mut header).map_err(ImportError::Io)?;
    // Every PDF goes to pdf-inspector: AnyDoc, built without its `pdf`
    // feature, would refuse one, and detects a PDF by the same window.
    if header[..n].windows(5).any(|bytes| bytes == b"%PDF-") {
        return convert_pdf(source, stamp, installed, skip_ocr, cancel);
    }
    let converted = convert_document(&source, cancel)?;
    check_cancel(cancel)?;
    let is_docx = source
        .extension()
        .is_some_and(|extension| extension.eq_ignore_ascii_case("docx"));
    // Comments never reach the Markdown: AnyDoc reports them beside it, read
    // (DOCX, ODT, RTF) or only counted (DOC, PPTX, XLSX, ODS, ODP), and the
    // left-out line says how many.
    let comments = converted.comments;
    let read = comments.len() as u64;
    let comments_unread = u64::from(converted.comments_left_out).saturating_sub(read);
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
        stamp,
        markdown: converted.markdown,
        // Only a PDF's import can be partial.
        warning: None,
        left_out: LeftOut {
            pictures,
            // Nor does hidden-formatted text (DOCX, RTF and DOC; 0
            // elsewhere).
            hidden_text: u64::from(converted.hidden_text_left_out),
            field_names: 0,
            repeated_lines: 0,
            comments: read,
            comments_unread,
        },
        comments,
        annotations: Vec::new(),
        is_pdf: false,
        is_docx,
    })
}

/// What converting a document other than a PDF gives the import: AnyDoc's
/// Markdown, comments and the counts the import note reads. The supervised
/// worker sends it back as is (`conversion_worker`), so it holds data only.
#[derive(Clone, Debug, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct Converted {
    pub markdown: String,
    pub images_left_out: u32,
    pub hidden_text_left_out: u32,
    pub comments: Vec<Comment>,
    /// Total comments left out of the Markdown, including those read as data.
    pub comments_left_out: u32,
}

impl From<anydoc::Conversion> for Converted {
    fn from(converted: anydoc::Conversion) -> Self {
        Self {
            markdown: converted.markdown,
            images_left_out: converted.images_left_out,
            hidden_text_left_out: converted.hidden_text_left_out,
            comments: converted.comments.into_iter().map(Comment::from).collect(),
            comments_left_out: converted.comments_left_out,
        }
    }
}

/// How long a non-PDF conversion may run in the supervised worker before
/// it is stopped. AnyDoc's own resource limits end most runaway documents
/// long before this.
pub const CONVERSION_TIMEOUT: std::time::Duration = std::time::Duration::from_secs(120);

/// Converts `source`, which is not a PDF: in the supervised worker when the
/// app installed one (desktop, `conversion_worker::supervise`), otherwise in
/// this process with a converter panic contained (the phones, whose
/// profile unwinds, and tests).
fn convert_document(source: &Path, cancel: &AtomicBool) -> Result<Converted, ImportError> {
    let bytes = std::fs::read(source).map_err(ImportError::Read)?;
    let extension = source.extension().and_then(|extension| extension.to_str());
    #[cfg(not(any(target_os = "ios", target_os = "android")))]
    if let Some(executable) = crate::conversion_worker::installed() {
        return crate::conversion_worker::convert_with_executable(
            bytes,
            extension,
            cancel,
            executable,
            CONVERSION_TIMEOUT,
        );
    }
    check_cancel(cancel)?;
    contained(|| convert_bytes(&bytes, extension))
}

/// Converts a document other than a PDF from its bytes, as the worker and
/// the in-process path both do. The format comes from the content, then the
/// file's `extension`, as `anydoc::to_markdown_with` picks it.
pub fn convert_bytes(bytes: &[u8], extension: Option<&str>) -> Result<Converted, ImportError> {
    let format = anydoc::Format::from_bytes(bytes)
        .or_else(|| extension.and_then(anydoc::Format::from_extension))
        .ok_or(ImportError::Unsupported)?;
    anydoc::to_markdown_bytes_with(bytes, format, anydoc::Options::default())
        .map(Converted::from)
        .map_err(convert_error)
}

/// `convert`'s result, or an import error if it panics. The phones' profile
/// unwinds (`profile.mobile`), so this keeps a converter panic from reaching
/// UniFFI as an internal error; the desktop release profile aborts, which is
/// why the desktop app converts in the supervised worker instead.
pub(crate) fn contained<T>(
    convert: impl FnOnce() -> Result<T, ImportError> + std::panic::UnwindSafe,
) -> Result<T, ImportError> {
    std::panic::catch_unwind(convert).unwrap_or_else(|panic| {
        let detail = panic
            .downcast_ref::<&str>()
            .map(|text| (*text).to_owned())
            .or_else(|| panic.downcast_ref::<String>().cloned())
            .unwrap_or_else(|| "a panic".into());
        Err(ImportError::ConverterFailed(format!(
            "the converter panicked: {detail}"
        )))
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
    stamp: Option<SourceStamp>,
    installed: Option<&crate::ocr::Installed>,
    skip_ocr: bool,
    cancel: &AtomicBool,
) -> Result<Imported, ImportError> {
    use pdf_inspector::vision::{OcrPdfOptions, process_pdf_with_ocr_mem};
    let bytes = read_pdf(&source)?;
    let mut native = process_pdf_with_ocr_mem(&bytes, OcrPdfOptions::new()).map_err(pdf_error)?;
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
    // Without OCR the pages it would read are skipped, and the notice
    // reports them whole: their pictures don't count again (#949).
    let mut left_out = if !skip_ocr && installed.is_some() {
        LeftOut::of_pdf(&native, &[])
    } else {
        LeftOut::of_pdf(&native, &native.pages_recommended_for_ocr)
    };
    // Annotations come from the native pass, and OCR renders the pages it
    // reads without them, so their text never reaches the Markdown.
    let annotations = std::mem::take(&mut native.annotations);
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
    left_out.repeated_lines = repeated_lines(&result);
    Ok(Imported {
        ocr_configuration: if used_ocr {
            installed.map(|i| i.config.clone())
        } else {
            None
        },
        source,
        stamp,
        markdown: result.markdown,
        warning,
        left_out,
        comments: Vec::new(),
        annotations,
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

/// The running header and footer rows `result`'s Markdown left out as exact
/// repeats of a row kept on an earlier page.
pub(crate) fn repeated_lines(result: &pdf_inspector::vision::OcrPdfResult) -> u64 {
    result
        .repeated_lines_by_page
        .iter()
        .map(|page| u64::from(page.lines))
        .sum()
}

/// The import note's line for repeated header and footer rows removed, if
/// any: "3 repeated header and footer lines removed."
pub(crate) fn repeated_lines_note(count: u64) -> Option<String> {
    match count {
        0 => None,
        1 => Some("1 repeated header or footer line removed.".to_owned()),
        n => Some(format!("{n} repeated header and footer lines removed.")),
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
                "handmade-partly-scanned.pdf" => {
                    Some("Partial import: pages 2, 5 of 5 require OCR and were skipped.")
                }
                "handmade-hidden.docx" => Some("7 hidden passages not imported."),
                "handmade-hidden.rtf" => Some("6 hidden passages not imported."),
                "handmade-hidden.doc" => Some("3 hidden passages not imported."),
                _ => None,
            };
            assert_eq!(imported.note().as_deref(), expected_note, "{name}");
        }
    }

    /// Every non-PDF import fixture: converting from bytes, as the worker
    /// and the phones do, gives what the file path gives AnyDoc,
    /// including its comments, counts and errors.
    #[test]
    fn converting_bytes_matches_converting_the_file() {
        let dir = tempfile::tempdir().unwrap();
        let csv = dir.path().join("values.CSV");
        std::fs::write(&csv, "Name,Count\nApples,2\n").unwrap();
        let unknown = dir.path().join("unknown.bin");
        std::fs::write(&unknown, b"unknown format").unwrap();
        let mut paths = vec![csv, unknown];
        for entry in std::fs::read_dir(fixture("")).unwrap() {
            let path = entry.unwrap().path();
            let extension = path.extension().and_then(|e| e.to_str()).unwrap_or("");
            if !["md", "pdf", "py", "anydoc"].contains(&extension) {
                paths.push(path);
            }
        }
        assert!(paths.len() >= 15, "{paths:?}");
        for path in paths {
            let by_path = anydoc::to_markdown_with(&path, anydoc::Options::default())
                .map(Converted::from)
                .map_err(|error| convert_error(error).to_string());
            let extension = path.extension().and_then(|e| e.to_str());
            let by_bytes = convert_bytes(&std::fs::read(&path).unwrap(), extension)
                .map_err(|error| error.to_string());
            assert_eq!(by_bytes, by_path, "{}", path.display());
        }
    }

    /// A converter panic is an import error where the profile unwinds (the
    /// phones and tests), never a crash.
    #[test]
    fn a_converter_panic_is_an_import_error() {
        let result = contained::<()>(|| panic!("parser bug"));
        assert!(
            matches!(&result, Err(ImportError::ConverterFailed(detail)) if detail.contains("parser bug")),
            "{result:?}"
        );
        assert!(
            result
                .unwrap_err()
                .to_string()
                .starts_with("The converter stopped unexpectedly")
        );
        assert_eq!(contained(|| Ok(7)).unwrap(), 7);
        assert!(matches!(
            contained::<()>(|| Err(ImportError::Damaged)),
            Err(ImportError::Damaged)
        ));
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

    /// Two pages: text with one picture, then a page that is only a picture
    /// (a scan). Without OCR the scan page is skipped and its picture is in
    /// the notice, not counted again; the other page's picture still is
    /// (#949).
    #[test]
    fn pictures_on_skipped_pages_are_not_counted_twice() {
        fn image(width: usize, height: usize) -> Vec<u8> {
            let data: Vec<u8> = (0..width * height * 3)
                .map(|i| (i * 7 % 251) as u8)
                .collect();
            let mut object = format!(
                "<< /Type /XObject /Subtype /Image /Width {width} /Height {height} \
                 /ColorSpace /DeviceRGB /BitsPerComponent 8 /Length {} >>\nstream\n",
                data.len()
            )
            .into_bytes();
            object.extend(data);
            object.extend(b"\nendstream");
            object
        }
        fn stream(content: &str) -> Vec<u8> {
            format!(
                "<< /Length {} >>\nstream\n{content}\nendstream",
                content.len()
            )
            .into_bytes()
        }
        let text = "BT /F1 12 Tf 72 700 Td (The supplier delivers the goods within ten days.) Tj ET\n\
             BT /F1 12 Tf 72 680 Td (The buyer pays within thirty days of delivery.) Tj ET\n\
             q 120 0 0 90 72 500 cm /Im1 Do Q";
        let objects: Vec<Vec<u8>> = vec![
            b"<< /Type /Catalog /Pages 2 0 R >>".to_vec(),
            b"<< /Type /Pages /Kids [3 0 R 4 0 R] /Count 2 >>".to_vec(),
            b"<< /Type /Page /Parent 2 0 R /MediaBox [0 0 612 792] /Resources << /Font << /F1 5 0 R >> \
              /XObject << /Im1 6 0 R >> >> /Contents 8 0 R >>"
                .to_vec(),
            b"<< /Type /Page /Parent 2 0 R /MediaBox [0 0 612 792] /Resources << /XObject << /Im2 7 0 R >> >> \
              /Contents 9 0 R >>"
                .to_vec(),
            b"<< /Type /Font /Subtype /Type1 /BaseFont /Helvetica >>".to_vec(),
            image(64, 48),
            image(200, 260),
            stream(text),
            stream("q 612 0 0 792 0 0 cm /Im2 Do Q"),
        ];
        let mut pdf = b"%PDF-1.4\n".to_vec();
        let mut offsets = Vec::new();
        for (index, object) in objects.iter().enumerate() {
            offsets.push(pdf.len());
            pdf.extend(format!("{} 0 obj\n", index + 1).as_bytes());
            pdf.extend(object);
            pdf.extend(b"\nendobj\n");
        }
        let xref = pdf.len();
        pdf.extend(format!("xref\n0 {}\n0000000000 65535 f \n", offsets.len() + 1).as_bytes());
        for offset in offsets {
            pdf.extend(format!("{offset:010} 00000 n \n").as_bytes());
        }
        pdf.extend(
            format!(
                "trailer\n<< /Size {} /Root 1 0 R >>\nstartxref\n{xref}\n%%EOF",
                objects.len() + 1
            )
            .as_bytes(),
        );
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("scan-and-picture.pdf");
        std::fs::write(&path, pdf).unwrap();
        let imported = convert(&path).unwrap();
        assert_eq!(
            imported.warning.as_deref(),
            Some("Partial import: pages 2 of 2 require OCR and were skipped.")
        );
        assert_eq!(imported.left_out.pictures, 1, "{:?}", imported.note());
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

    /// A running header stays on its first page and its exact repeats are
    /// dropped and counted; text drawn in its background's colour is left
    /// out and counted with the other hidden passages (#925 M4, M5).
    #[test]
    fn the_note_counts_repeated_header_lines_and_text_in_its_background_colour() {
        let imported = convert(&fixture("handmade-running-header.pdf")).unwrap();
        assert_eq!(
            imported.markdown.matches("Case No. 2-123/2026").count(),
            1,
            "{}",
            imported.markdown
        );
        assert!(
            !imported.markdown.contains("SECRET"),
            "{}",
            imported.markdown
        );
        assert!(
            imported.markdown.contains("Clause 4.11"),
            "{}",
            imported.markdown
        );
        assert_eq!(imported.warning, None);
        assert_eq!(
            imported.left_out.line(true).as_deref(),
            Some("1 hidden passage not imported. 3 repeated header and footer lines removed.")
        );
        assert_eq!(repeated_lines_note(0), None);
        assert_eq!(
            repeated_lines_note(1).as_deref(),
            Some("1 repeated header or footer line removed.")
        );
    }

    /// On a sparse last page, the header's sentence in the body is body
    /// text: it stays and isn't counted, while the header and footer still
    /// go from pages 2 and 3 (#925 M16, lawkitt/pdf-inspector#22).
    #[test]
    fn a_header_sentence_in_a_sparse_pages_body_stays() {
        let imported = convert(&fixture("handmade-sparse-page-header.pdf")).unwrap();
        let header = "Annual Report of the Board 2026 Confidential";
        let (pages_1_to_3, page_4) = imported.markdown.split_once("Item 4.1.").unwrap();
        assert_eq!(
            pages_1_to_3.matches(header).count(),
            1,
            "{}",
            imported.markdown
        );
        assert_eq!(page_4.matches(header).count(), 1, "{}", imported.markdown);
        assert_eq!(
            imported
                .markdown
                .matches("Prepared for the shareholders")
                .count(),
            1,
            "{}",
            imported.markdown
        );
        assert_eq!(imported.warning, None);
        assert_eq!(
            imported.left_out.line(true).as_deref(),
            Some("4 repeated header and footer lines removed.")
        );
    }

    /// Letter pages and a Legal page keep the header at one height, a steady
    /// distance from the bottom edge only. On the sparse last page the
    /// header's sentence 96 pt below the top, the full pages' average
    /// distance from the top, is body text: it stays and isn't counted
    /// (#925 M17, lawkitt/pdf-inspector#23).
    #[test]
    fn a_header_sentence_on_a_mixed_size_sparse_page_stays() {
        let imported = convert(&fixture("handmade-mixed-size-header.pdf")).unwrap();
        let header = "Annual Report of the Board 2026 Confidential";
        // Page 5's sentence is above its body line, so page 5 starts after
        // page 4's last item.
        let (pages_1_to_4, page_5) = imported.markdown.split_once("Item 4.12.").unwrap();
        assert_eq!(
            pages_1_to_4.matches(header).count(),
            1,
            "{}",
            imported.markdown
        );
        assert_eq!(page_5.matches(header).count(), 1, "{}", imported.markdown);
        assert_eq!(
            imported
                .markdown
                .matches("Prepared for the shareholders")
                .count(),
            1,
            "{}",
            imported.markdown
        );
        assert_eq!(imported.warning, None);
        assert_eq!(
            imported.left_out.line(true).as_deref(),
            Some("6 repeated header and footer lines removed.")
        );
    }

    /// On a sparse last page with the header where the full pages have it
    /// and its sentence again 24 pt lower, inside the position tolerance,
    /// only the match closest to the header's place goes: the sentence
    /// stays and isn't counted (#925 M18, lawkitt/pdf-inspector#24).
    #[test]
    fn a_sentence_right_under_a_sparse_pages_header_stays() {
        let imported = convert(&fixture("handmade-sparse-page-closest-header.pdf")).unwrap();
        let header = "Annual Report of the Board 2026 Confidential";
        let (pages_1_to_3, page_4) = imported.markdown.split_once("Item 3.12.").unwrap();
        assert_eq!(
            pages_1_to_3.matches(header).count(),
            1,
            "{}",
            imported.markdown
        );
        assert_eq!(page_4.matches(header).count(), 1, "{}", imported.markdown);
        assert_eq!(
            imported
                .markdown
                .matches("Prepared for the shareholders")
                .count(),
            1,
            "{}",
            imported.markdown
        );
        assert_eq!(imported.warning, None);
        assert_eq!(
            imported.left_out.line(true).as_deref(),
            Some("5 repeated header and footer lines removed.")
        );
    }

    /// Comments never reach the Markdown, whatever the format: the import
    /// equals the fork's reviewed snapshot, and no comment's text or author
    /// (an annotation's, for a PDF) is in it. The import carries them beside
    /// it instead.
    #[test]
    fn comments_and_annotations_never_reach_the_markdown() {
        for name in [
            "handmade-comments.docx",
            "handmade-comments.odt",
            "handmade-comments.rtf",
            "text_annotations.pdf",
        ] {
            let imported = convert(&fixture(name)).unwrap();
            let expected = std::fs::read_to_string(fixture(&format!("{name}.md")))
                .unwrap()
                .replace("\r\n", "\n");
            let markdown = imported.markdown.replace("\r\n", "\n");
            assert_eq!(
                markdown.trim_end_matches('\n'),
                expected.trim_end_matches('\n'),
                "{name}"
            );
            let said: Vec<(&str, &str)> = imported
                .comments
                .iter()
                .map(|c| (c.text.as_str(), c.author.as_str()))
                .chain(
                    imported
                        .annotations
                        .iter()
                        .map(|a| (a.text.as_str(), a.author.as_deref().unwrap_or_default())),
                )
                .collect();
            assert!(said.len() >= 6, "{name}: {said:?}");
            for (text, author) in said {
                for words in [text.trim(), author] {
                    assert!(
                        words.is_empty() || !markdown.contains(words),
                        "{name}: {words:?} reached the Markdown"
                    );
                }
            }
        }
    }

    /// The left-out line counts what the forks report: comments read as
    /// data (DOCX, ODT, RTF; a PDF's annotations) only without a rendering
    /// to show them, comments only counted (PPTX, XLSX) always. A renamed
    /// copy counts the same, as AnyDoc reads the content.
    #[test]
    fn the_left_out_line_counts_the_forks_comments() {
        for (name, read, unread) in [
            ("handmade-comments.docx", 7, 0),
            ("handmade-comments.odt", 7, 0),
            ("handmade-comments.rtf", 7, 0),
            ("handmade-comments.pptx", 0, 4),
            ("handmade-comments.xlsx", 0, 3),
            ("text_annotations.pdf", 8, 0),
        ] {
            let imported = convert(&fixture(name)).unwrap();
            let left_out = imported.left_out;
            assert_eq!(
                (left_out.comments, left_out.comments_unread),
                (read, unread),
                "{name}"
            );
            assert_eq!(
                imported.comments.len() + imported.annotations.len(),
                read as usize,
                "{name}"
            );
            let all = format!("{} comments not shown.", read + unread);
            assert_eq!(left_out.line(false), Some(all), "{name}");
            let only_counted = (unread > 0).then(|| format!("{unread} comments not shown."));
            assert_eq!(left_out.line(true), only_counted, "{name}");
            let note = format!("{} comments not imported.", read + unread);
            assert_eq!(imported.note(), Some(note), "{name}");
            let dir = tempfile::tempdir().unwrap();
            let renamed = dir.path().join("renamed.bin");
            std::fs::copy(fixture(name), &renamed).unwrap();
            let again = convert(&renamed).unwrap();
            assert_eq!(again.left_out, left_out, "{name}");
            assert_eq!(again.comments, imported.comments, "{name}");
        }
        assert_eq!(not_shown_note(1).as_deref(), Some("1 comment not shown."));
        assert_eq!(not_shown_note(0), None);
    }

    /// The Source's cards and the note come from the same reading: AnyDoc's
    /// comments, with author, date, text, passage, parent and resolved
    /// state, as `source_comments` gives them for a Source opened without
    /// an import.
    #[test]
    fn source_cards_read_what_the_import_counted() {
        let path = fixture("handmade-comments.docx");
        let imported = convert(&path).unwrap();
        assert_eq!(source_comments(&path), imported.comments);
        let row = |id: &str| {
            let c = imported.comments.iter().find(|c| c.id == id).unwrap();
            (
                c.author.as_str(),
                c.text.as_str(),
                c.anchor_text.as_str(),
                c.parent.as_deref(),
                c.resolved,
            )
        };
        assert_eq!(
            row("0"),
            (
                "Alice Reviewer",
                "Check this range.",
                "commented words",
                None,
                false
            )
        );
        assert_eq!(
            row("3"),
            (
                "Bob Builder",
                "Agreed.",
                "commented words",
                Some("0"),
                false
            )
        );
        assert_eq!(
            row("4"),
            (
                "Bob Builder",
                "Fixed, resolving.",
                "resolved words",
                None,
                true
            )
        );
        assert_eq!(imported.comments[0].date, "2026-11-18T09:30:00Z");
        // A PDF has no office comments; one AnyDoc can't convert has none.
        assert!(source_comments(&fixture("text_annotations.pdf")).is_empty());
        assert!(source_comments(&fixture("missing.docx")).is_empty());
    }

    /// A Source shown without an import reads the annotations the import
    /// would: the same native pass.
    #[test]
    fn source_annotations_match_the_import() {
        let path = fixture("text_annotations.pdf");
        let imported = convert(&path).unwrap();
        assert_eq!(imported.annotations.len(), 8);
        assert_eq!(source_annotations(&path), Some(imported.annotations));
        assert_eq!(source_annotations(&fixture("missing.pdf")), None);
        assert_eq!(source_annotations(&fixture("text.docx")), None);
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
            Some("Partial import: pages 2, 5 of 5 require OCR and were skipped.")
        );
        // A picture the package holds but the document never shows is not
        // left out of anything.
        let comments = Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../../tests/fixtures/docx-preview/comments.docx");
        assert_eq!(
            convert(&comments).unwrap().note().as_deref(),
            Some("6 comments not imported. 1 hidden passage not imported.")
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
        // Its two pictures are on the skipped pages, which the notice
        // reports whole (#949).
        assert_eq!(partly.left_out.line(true), None);
        let comments = convert(
            &Path::new(env!("CARGO_MANIFEST_DIR"))
                .join("../../tests/fixtures/docx-preview/comments.docx"),
        )
        .unwrap();
        assert_eq!(comments.warning, None);
        assert_eq!(
            comments.left_out.line(true).as_deref(),
            Some("1 hidden passage not imported.")
        );
        assert_eq!(
            comments.left_out.line(false).as_deref(),
            Some("1 hidden passage not imported. 6 comments not shown.")
        );
        let all = LeftOut {
            pictures: 1,
            hidden_text: 2,
            field_names: 3,
            repeated_lines: 4,
            comments: 2,
            comments_unread: 0,
        };
        assert_eq!(
            all.line(false).as_deref(),
            Some(
                "1 picture not imported. 2 hidden passages not imported. \
                 3 form field names not imported. 4 repeated header and footer lines removed. \
                 2 comments not shown."
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
                "2 comments not imported. 1 picture not imported. \
                 2 hidden passages not imported. 3 form field names not imported. \
                 4 repeated header and footer lines removed."
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
        assert_eq!(imported.left_out.comments, 2);
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
            Some("Partial import: pages 2, 5 of 5 require OCR and were skipped.")
        );
        assert!(!imported.markdown.contains("OCR"));
        let mixed = convert(&fixture("handmade-mixed.pdf")).unwrap();
        assert_eq!(
            mixed.note().as_deref(),
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
