//! Conversion boundary for AnyDoc and PDF extraction. Conversion performs no writes.
use crate::comments::Comment;
use std::{
    io::Read,
    path::{Path, PathBuf},
    sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
    },
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
    /// The bytes this import converted: the DOCX preview renders these.
    pub source: Source,
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
    /// The file was password-protected and this import unlocked it in
    /// memory. No rendering shows it (that would write it to disk), so the
    /// left-out line counts its comments.
    pub decrypted: bool,
}

/// A document's bytes, read from its file once: the import converts them,
/// and its comments and the DOCX preview read them too, never the file
/// again, so bytes that differ from the file's (an unlocked copy of a
/// locked one) never have to be written anywhere.
#[derive(Clone)]
pub struct Source {
    /// Where the bytes came from (absolute when an import read them); its
    /// extension names the format when the content does not.
    pub path: PathBuf,
    /// The file's size and modification time when it was read: what the
    /// bytes say about the file holds only while the stamp still matches.
    pub stamp: Option<SourceStamp>,
    bytes: SharedBytes,
    /// The bytes are a locked file's, unlocked in memory: they differ from
    /// the file's and must never be written anywhere.
    unlocked: bool,
}

/// A document's bytes, shared rather than copied, and wiped when the last
/// holder drops them: they may be a locked document's, unlocked. A view that
/// opens them from memory takes a clone as `Arc<dyn AsRef<[u8]> + Send +
/// Sync>`, with no copy.
#[derive(Clone)]
pub struct SharedBytes(Arc<zeroize::Zeroizing<Vec<u8>>>);

impl AsRef<[u8]> for SharedBytes {
    fn as_ref(&self) -> &[u8] {
        &self.0
    }
}

impl Source {
    /// The file at `path`, read as an import reads it: a PDF (by its header,
    /// so a renamed one counts) within the import limit, anything else
    /// whole, since AnyDoc applies its own limits.
    pub fn read(path: &Path) -> Result<Self, ImportError> {
        let path = std::path::absolute(path).map_err(ImportError::Read)?;
        let file = std::fs::File::open(&path).map_err(ImportError::Read)?;
        let stamp = file
            .metadata()
            .ok()
            .map(|metadata| SourceStamp::from(&metadata));
        let mut file = file.take(MAX_PDF_BYTES + 1);
        let mut bytes = Vec::new();
        file.read_to_end(&mut bytes).map_err(ImportError::Io)?;
        if is_pdf(&bytes) {
            if bytes.len() as u64 > MAX_PDF_BYTES {
                return Err(ImportError::PdfTooLarge);
            }
        } else {
            file.into_inner()
                .read_to_end(&mut bytes)
                .map_err(ImportError::Io)?;
        }
        Ok(Self::new(path, stamp, bytes))
    }

    /// `bytes`, said to come from `path` as `stamp` stood.
    pub fn new(path: PathBuf, stamp: Option<SourceStamp>, bytes: Vec<u8>) -> Self {
        Self {
            path,
            stamp,
            bytes: SharedBytes(Arc::new(zeroize::Zeroizing::new(bytes))),
            unlocked: false,
        }
    }

    pub fn bytes(&self) -> &[u8] {
        self.bytes.as_ref()
    }

    /// The bytes, shared rather than copied: for the workers and for views
    /// that open a document from memory.
    pub fn shared_bytes(&self) -> SharedBytes {
        self.bytes.clone()
    }

    /// The bytes are a password-protected file's, unlocked in memory: no
    /// copy of them may be written, so the DOCX preview, which renders to a
    /// temporary file, leaves them alone.
    pub fn unlocked(&self) -> bool {
        self.unlocked
    }

    /// The bytes are a PDF's, as the import detects one.
    pub fn is_pdf(&self) -> bool {
        is_pdf(self.bytes())
    }

    /// The file at `path` is still the one these bytes were read from: its
    /// stamp matches the one taken then.
    pub fn is_current(&self) -> bool {
        self.stamp.is_some() && SourceStamp::of(&self.path) == self.stamp
    }

    /// The bytes as an import converts them: a password-protected DOCX,
    /// XLSX, PPTX or DOC unlocked in memory with `password`, anything else
    /// as it is (a password it doesn't need is ignored). Without a password
    /// a locked file is `PasswordRequired`, so the caller knows to ask; a
    /// scheme no password opens here is `EncryptionUnsupported` (DOCX, XLSX,
    /// PPTX) or `Encrypted` (DOC).
    pub fn unlock(self, password: Option<&Password>) -> Result<Self, ImportError> {
        if self.unlocked {
            return Ok(self);
        }
        if craft_doc_binary::is_encrypted(self.bytes()) {
            let document = unlock_doc(self.bytes(), password.map(Password::expose))?;
            return Ok(Self {
                unlocked: true,
                ..Self::new(self.path, self.stamp, document)
            });
        }
        if !craft_offcrypto::is_encrypted(self.bytes()) {
            return Ok(self);
        }
        let package = craft_offcrypto::decrypt(self.bytes(), password.map(Password::expose))
            .map_err(|error| match error {
                craft_offcrypto::Error::PasswordRequired => ImportError::PasswordRequired,
                craft_offcrypto::Error::WrongPassword => ImportError::WrongPassword,
                craft_offcrypto::Error::Unsupported(scheme) => {
                    ImportError::EncryptionUnsupported(scheme)
                }
                // Malformed, TooLarge, and whatever a later revision adds.
                _ => ImportError::Damaged,
            })?;
        Ok(Self {
            unlocked: true,
            ..Self::new(self.path, self.stamp, package)
        })
    }

    fn extension(&self) -> Option<&str> {
        self.path
            .extension()
            .and_then(|extension| extension.to_str())
    }
}

/// Never the bytes: they are the user's document.
impl std::fmt::Debug for Source {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Source")
            .field("path", &self.path)
            .field("stamp", &self.stamp)
            .field("len", &self.bytes().len())
            .field("unlocked", &self.unlocked)
            .finish()
    }
}

/// A password-protected DOC (XOR obfuscation, RC4 or RC4 CryptoAPI)
/// unlocked in memory with `password`, as the file AnyDoc converts: its
/// scheme is checked first, so a file no password opens here is `Encrypted`
/// before any password is asked for; then a missing password is
/// `PasswordRequired` and one that fails the file's verifier `WrongPassword`.
/// The unlocked bytes go straight into the source, whose wiping covers them.
fn unlock_doc(bytes: &[u8], password: Option<&str>) -> Result<Vec<u8>, ImportError> {
    let mut unlocked = crate::doc_crypt::decrypt(bytes, password)?;
    Ok(std::mem::take(&mut *unlocked))
}

/// The password of a password-protected document. It lives in memory only:
/// for one import, or as a tab's [`Unlock`] of a locked PDF. Never stored,
/// logged or shown; clones share one copy, wiped when the last one drops.
#[derive(Clone)]
pub struct Password(Arc<zeroize::Zeroizing<String>>);

impl Password {
    /// Takes `text` without copying it.
    pub fn new(text: String) -> Self {
        Self::from(zeroize::Zeroizing::new(text))
    }

    /// The text, for the call that opens the document with it and nothing
    /// else: it is never kept, logged or shown.
    pub fn expose(&self) -> &str {
        &self.0
    }
}

/// For a view that opens a PDF with it from memory, as `Arc<dyn AsRef<str>
/// + Send + Sync>`: the text is shared, never copied.
impl AsRef<str> for Password {
    fn as_ref(&self) -> &str {
        self.expose()
    }
}

impl From<zeroize::Zeroizing<String>> for Password {
    fn from(text: zeroize::Zeroizing<String>) -> Self {
        Self(Arc::new(text))
    }
}

/// Never the password.
impl std::fmt::Debug for Password {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("Password(..)")
    }
}

/// What opens a password-protected source again without asking, for as
/// long as the tab that unlocked it is open (#1024 decision 6): the
/// unlocked bytes of a locked DOCX, XLSX, PPTX or DOC, or the password of a
/// locked PDF, which every PDF call takes. It lives in memory only, never in
/// the session, a log or a file, and is wiped when its last holder drops;
/// reopening the document asks again. An import takes it and hands back
/// what opens the source from then on ([`prepare_unlocking`]).
#[derive(Clone)]
pub enum Unlock {
    /// A password for the file at `path`: typed on the conversion card and
    /// not tried yet, or a locked PDF's once it opened the file.
    Password { path: PathBuf, password: Password },
    /// A locked Office document's bytes, unlocked in memory. Conversions and
    /// the preview read these while the file is unchanged, never the file.
    Unlocked(Source),
}

impl Unlock {
    /// The file this unlock opens.
    pub fn path(&self) -> &Path {
        match self {
            Self::Password { path, .. } => path,
            Self::Unlocked(source) => &source.path,
        }
    }

    /// A locked PDF's password (or one typed on the card): what every call
    /// that opens the PDF takes.
    pub fn password(&self) -> Option<&Password> {
        match self {
            Self::Password { password, .. } => Some(password),
            Self::Unlocked(_) => None,
        }
    }

    /// A locked Office document's unlocked bytes, while its file is still
    /// the one they were read from.
    pub fn current_source(&self) -> Option<&Source> {
        match self {
            Self::Unlocked(source) if source.is_current() => Some(source),
            _ => None,
        }
    }

    /// It opens the file at `path`.
    pub fn opens(&self, path: &Path) -> bool {
        std::path::absolute(self.path())
            .is_ok_and(|own| std::path::absolute(path).is_ok_and(|path| own == path))
    }
}

/// Never the password or the bytes.
impl std::fmt::Debug for Unlock {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Password { path, .. } => f
                .debug_struct("Password")
                .field("path", path)
                .finish_non_exhaustive(),
            Self::Unlocked(source) => f.debug_tuple("Unlocked").field(source).finish(),
        }
    }
}

/// Every PDF goes to pdf-inspector: AnyDoc, built without its `pdf`
/// feature, would refuse one, and detects a PDF by the same window.
pub(crate) fn is_pdf(bytes: &[u8]) -> bool {
    bytes[..bytes.len().min(1024)]
        .windows(5)
        .any(|window| window == b"%PDF-")
}

/// The largest PDF an import reads.
const MAX_PDF_BYTES: u64 = 256 * 1024 * 1024;

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
    /// A spreadsheet's hidden worksheets (XLSX, XLSB, XLS, ODS), each
    /// whether or not it holds anything.
    pub hidden_sheets: u64,
    /// Hidden rows of a spreadsheet's visible sheets that hold a value.
    pub hidden_rows: u64,
    /// Hidden columns of a spreadsheet's visible sheets that hold a value.
    pub hidden_columns: u64,
    /// AcroForm field names; the fields' values stay.
    pub field_names: u64,
    /// Files attached to a PDF (its embedded files, a portfolio's files,
    /// file attachment annotations), each once. Only the count: their names
    /// are personal data too, and their content is never read.
    pub attached_files: u64,
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
            hidden_sheets: 0,
            hidden_rows: 0,
            hidden_columns: 0,
            field_names: sum(&native.form_field_names_by_page, |page| page.names),
            attached_files: native.attached_files.len() as u64,
            repeated_lines: 0,
            comments: native.annotations.len() as u64,
            comments_unread: 0,
        }
    }

    /// The note's lines for the counts other than comments, in order.
    fn counts(&self) -> [Option<String>; 8] {
        [
            pictures_note(self.pictures),
            hidden_text_note(self.hidden_text),
            left_out_note(self.hidden_sheets, "hidden sheet", "hidden sheets"),
            left_out_note(self.hidden_rows, "hidden row", "hidden rows"),
            left_out_note(self.hidden_columns, "hidden column", "hidden columns"),
            left_out_note(self.field_names, "form field name", "form field names"),
            left_out_note(self.attached_files, "attached file", "attached files"),
            repeated_lines_note(self.repeated_lines),
        ]
    }

    /// The left-out line, if anything is left out. Comments read as data
    /// count only where no rendering of the document shows them
    /// (`comments_shown` false); comments only counted always do.
    pub fn line(&self, comments_shown: bool) -> Option<String> {
        let [
            pictures,
            hidden,
            sheets,
            rows,
            columns,
            field_names,
            attached,
            repeated,
        ] = self.counts();
        let comments = self.comments_unread + if comments_shown { 0 } else { self.comments };
        join_notes([
            pictures,
            hidden,
            sheets,
            rows,
            columns,
            field_names,
            attached,
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
        let [
            pictures,
            hidden,
            sheets,
            rows,
            columns,
            field_names,
            attached,
            repeated,
        ] = self.left_out.counts();
        join_notes([
            self.warning.clone(),
            left_out_note(
                self.left_out.comments + self.left_out.comments_unread,
                "comment",
                "comments",
            ),
            pictures,
            hidden,
            sheets,
            rows,
            columns,
            field_names,
            attached,
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

/// The comments of the office document `source`, read as its import reads
/// them: the Source's cards for a document this session did not import.
/// None when AnyDoc cannot convert it.
pub fn source_comments(source: &Source) -> Vec<Comment> {
    source_comments_cancellable(source, &AtomicBool::new(false))
}

pub(crate) fn source_comments_cancellable(source: &Source, cancel: &AtomicBool) -> Vec<Comment> {
    convert_document(source, cancel)
        .map(|converted| converted.comments)
        .unwrap_or_default()
}

/// The left-out line's phrase for the comments of the office document
/// `source` that its rendering would show: for an original that could not
/// render, whose import counted them there instead of in the line.
pub fn source_comments_not_shown(source: &Source) -> Option<String> {
    not_shown_note(source_comments(source).len() as u64)
}

/// The annotations of the PDF `source` that carry text, read by the same
/// native pass its import makes, opened with the tab's `password` when it
/// is locked: the Source's cards for a PDF this session did not import.
/// `None` when the pass cannot read it (past the import's size limit,
/// locked without the right password, or damaged).
pub fn source_annotations(
    source: &Source,
    password: Option<&Password>,
) -> Option<Vec<pdf_inspector::PdfAnnotation>> {
    use pdf_inspector::vision::{OcrPdfOptions, process_pdf_with_ocr_mem};
    if source.bytes().len() as u64 > MAX_PDF_BYTES {
        return None;
    }
    process_pdf_with_ocr_mem(
        source.bytes(),
        with_password(OcrPdfOptions::new(), password),
    )
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
    /// A locked PDF whose encryption no password opens here: a security
    /// handler other than the standard one (public-key encryption among
    /// them), or an algorithm it doesn't know.
    #[error("This PDF uses encryption mdoc can’t open. Import an unlocked copy.")]
    PdfEncryptionUnsupported,
    /// A password-protected file mdoc can't unlock: an ODT, or a DOC whose
    /// encryption no password opens here.
    #[error("This document is password-protected. Import an unlocked copy.")]
    Encrypted,
    /// A locked DOCX, XLSX, PPTX or DOC and no password: the app asks for
    /// one.
    #[error("This document is password-protected. Enter its password to import it.")]
    PasswordRequired,
    #[error("The password is incorrect. Check it and try again.")]
    WrongPassword,
    /// A locked DOCX, XLSX or PPTX whose encryption no password opens here,
    /// named to complete "uses {0} encryption" (`RC4`, `rights-management`).
    #[error("This document uses {0} encryption, which mdoc can’t unlock. Import an unlocked copy.")]
    EncryptionUnsupported(String),
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
    prepare_cancellable(path, None, installed, skip_ocr, &AtomicBool::new(false))
}

/// Imports the file at `path`. `password` unlocks a password-protected
/// DOCX, XLSX, PPTX, DOC or PDF in memory for this import only; without it
/// such a file is `PasswordRequired`, and a wrong one `WrongPassword`.
pub fn prepare_cancellable(
    path: &Path,
    password: Option<&Password>,
    installed: Option<&crate::ocr::Installed>,
    skip_ocr: bool,
    cancel: &AtomicBool,
) -> Result<Imported, ImportError> {
    let mut unlock = password.map(|password| Unlock::Password {
        path: path.to_path_buf(),
        password: password.clone(),
    });
    prepare_unlocking(path, &mut unlock, installed, skip_ocr, cancel)
}

/// [`prepare_cancellable`] of `source`, already read: a caller that
/// decides something from the bytes (a format's policy) converts those same
/// bytes.
pub fn prepare_source(
    source: Source,
    password: Option<&Password>,
    installed: Option<&crate::ocr::Installed>,
    skip_ocr: bool,
    cancel: &AtomicBool,
) -> Result<Imported, ImportError> {
    check_cancel(cancel)?;
    let source = source.unlock(password)?;
    check_cancel(cancel)?;
    import_source(source, password, installed, skip_ocr, cancel)
}

/// Imports the file at `path` with a tab's `unlock`, then leaves in it what
/// opens the file from now on. Unlocked bytes of the file as it still is
/// convert again without reading or decrypting it; a password unlocks it.
/// Afterwards `unlock` holds a locked Office document's unlocked bytes, or
/// a PDF's password, which every later call that opens the PDF takes (its
/// decrypted bytes are never kept); nothing when the file isn't locked,
/// the password was missing or wrong, the PDF's encryption is unsupported,
/// or an Office file changed since it was unlocked (so the card asks
/// again). An unlock of another file is ignored and dropped.
pub fn prepare_unlocking(
    path: &Path,
    unlock: &mut Option<Unlock>,
    installed: Option<&crate::ocr::Installed>,
    skip_ocr: bool,
    cancel: &AtomicBool,
) -> Result<Imported, ImportError> {
    check_cancel(cancel)?;
    let path = std::path::absolute(path).map_err(ImportError::Read)?;
    let held = unlock.take().filter(|held| held.opens(&path));
    let password = held.as_ref().and_then(Unlock::password).cloned();
    let source = match held.as_ref().and_then(Unlock::current_source) {
        Some(unlocked) => unlocked.clone(),
        None => Source::read(&path)?.unlock(password.as_ref())?,
    };
    drop(held);
    *unlock = source.unlocked().then(|| Unlock::Unlocked(source.clone()));
    check_cancel(cancel)?;
    // A PDF's password stays only once it has opened the file: a typed one
    // a cancel or a refusal stopped before then proves nothing.
    let opened = std::cell::Cell::new(false);
    let imported = import_opening(
        source,
        password.as_ref(),
        installed,
        skip_ocr,
        cancel,
        &opened,
    );
    if opened.get()
        && let Some(password) = password
    {
        *unlock = Some(Unlock::Password { path, password });
    }
    imported
}

/// Imports `source`'s bytes: the one conversion every import makes after it
/// has read them. `password` opens a locked PDF; other formats arrive
/// unlocked already ([`Source::unlock`]).
pub(crate) fn import_source(
    source: Source,
    password: Option<&Password>,
    installed: Option<&crate::ocr::Installed>,
    skip_ocr: bool,
    cancel: &AtomicBool,
) -> Result<Imported, ImportError> {
    let opened = std::cell::Cell::new(false);
    import_opening(source, password, installed, skip_ocr, cancel, &opened)
}

/// [`import_source`], setting `opened` once a PDF's native pass has opened
/// it with `password`: then the password is known to open the file, however
/// the import ends.
fn import_opening(
    source: Source,
    password: Option<&Password>,
    installed: Option<&crate::ocr::Installed>,
    skip_ocr: bool,
    cancel: &AtomicBool,
    opened: &std::cell::Cell<bool>,
) -> Result<Imported, ImportError> {
    if is_pdf(source.bytes()) {
        return convert_pdf(source, password, installed, skip_ocr, cancel, opened);
    }
    let converted = convert_document(&source, cancel)?;
    check_cancel(cancel)?;
    let is_docx = source
        .path
        .extension()
        .is_some_and(|extension| extension.eq_ignore_ascii_case("docx"));
    let decrypted = source.unlocked();
    // Comments never reach the Markdown: AnyDoc reports them beside it, read
    // (DOCX, ODT, RTF) or only counted (DOC, PPTX, XLSX, ODS, ODP), and the
    // left-out line says how many.
    let comments = converted.comments;
    let read = comments.len() as u64;
    let comments_unread = u64::from(converted.comments_left_out).saturating_sub(read);
    // Nor do Word's pictures: embedded ones render as their alt text at
    // most (ROADMAP Vision item 1).
    let format = anydoc::Format::from_path(&source.path);
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
            // Nor do a spreadsheet's hidden sheets, rows and columns
            // (XLSX, XLSB, XLS and ODS; 0 elsewhere).
            hidden_sheets: u64::from(converted.hidden_sheets_left_out),
            hidden_rows: u64::from(converted.hidden_rows_left_out),
            hidden_columns: u64::from(converted.hidden_columns_left_out),
            field_names: 0,
            attached_files: 0,
            repeated_lines: 0,
            comments: read,
            comments_unread,
        },
        comments,
        annotations: Vec::new(),
        is_pdf: false,
        is_docx,
        decrypted,
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
    pub hidden_sheets_left_out: u32,
    pub hidden_rows_left_out: u32,
    pub hidden_columns_left_out: u32,
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
            hidden_sheets_left_out: converted.hidden_sheets_left_out,
            hidden_rows_left_out: converted.hidden_rows_left_out,
            hidden_columns_left_out: converted.hidden_columns_left_out,
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
fn convert_document(source: &Source, cancel: &AtomicBool) -> Result<Converted, ImportError> {
    let (bytes, extension) = (source.bytes(), source.extension());
    #[cfg(not(any(target_os = "ios", target_os = "android")))]
    if let Some(executable) = crate::conversion_worker::installed() {
        return crate::conversion_worker::convert_with_executable(
            source.shared_bytes(),
            extension,
            cancel,
            executable,
            CONVERSION_TIMEOUT,
        );
    }
    check_cancel(cancel)?;
    contained(|| convert_bytes(bytes, extension))
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
    source: Source,
    password: Option<&Password>,
    installed: Option<&crate::ocr::Installed>,
    skip_ocr: bool,
    cancel: &AtomicBool,
    opened: &std::cell::Cell<bool>,
) -> Result<Imported, ImportError> {
    use pdf_inspector::vision::{OcrPdfOptions, process_pdf_with_ocr_mem};
    let bytes = pdf_bytes(&source)?;
    let mut native = process_pdf_with_ocr_mem(bytes, with_password(OcrPdfOptions::new(), password))
        .map_err(|error| pdf_error(error, bytes, password))?;
    opened.set(true);
    check_cancel(cancel)?;
    let needs_ocr = !native.pages_recommended_for_ocr.is_empty();
    if needs_ocr && installed.is_none() && !skip_ocr {
        return Err(ImportError::NeedsOcr(
            source.path,
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
        match process_pdf_with_ocr_mem(bytes, with_password(installed.options(), password)) {
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
        markdown: result.markdown,
        warning,
        left_out,
        comments: Vec::new(),
        annotations,
        is_pdf: true,
        is_docx: false,
        decrypted: false,
    })
}

/// A PDF's bytes, within the import limit.
pub(crate) fn pdf_bytes(source: &Source) -> Result<&[u8], ImportError> {
    if source.bytes().len() as u64 > MAX_PDF_BYTES {
        return Err(ImportError::PdfTooLarge);
    }
    Ok(source.bytes())
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

/// `options` opening the PDF with `password`, when there is one: every pass
/// over a locked PDF (text, OCR's PDFium render, comment hiding) takes it.
/// pdf-inspector keeps its own copy of the text for the pass.
pub(crate) fn with_password(
    options: pdf_inspector::vision::OcrPdfOptions,
    password: Option<&Password>,
) -> pdf_inspector::vision::OcrPdfOptions {
    match password {
        Some(password) => options.password(password.expose()),
        None => options,
    }
}

/// Why the native text pass could not read the PDF `bytes`, opened with
/// `password`. A locked one is `PasswordRequired` without a password and
/// `WrongPassword` with one, unless its encryption is one no password opens
/// here (`PdfEncryptionUnsupported`). A PDF locked only against editing
/// opens without a password, so it never gets here.
pub(crate) fn pdf_error(
    error: pdf_inspector::vision::OcrPipelineError,
    bytes: &[u8],
    password: Option<&Password>,
) -> ImportError {
    use pdf_inspector::{PdfError, vision::OcrPipelineError};
    match error {
        OcrPipelineError::Pdf(PdfError::Encrypted) => locked_pdf_error(bytes, password),
        OcrPipelineError::Pdf(PdfError::Io(error)) => ImportError::Read(error),
        OcrPipelineError::Pdf(
            PdfError::Parse(_) | PdfError::InvalidStructure | PdfError::NotAPdf(_),
        ) => ImportError::Damaged,
        other => ImportError::Pdf(other.to_string()),
    }
}

/// What a locked PDF's import says: the card asks for its password, says the
/// one given is wrong, or the PDF can't be opened at all.
fn locked_pdf_error(bytes: &[u8], password: Option<&Password>) -> ImportError {
    if unsupported_pdf_encryption(bytes, password) {
        ImportError::PdfEncryptionUnsupported
    } else if password.is_some() {
        ImportError::WrongPassword
    } else {
        ImportError::PasswordRequired
    }
}

/// The PDF `bytes`, which pdf-inspector refused as encrypted with
/// `password`, use encryption no password opens here: a security handler
/// other than the standard one, or a version, revision or algorithm it
/// doesn't know. pdf-inspector reports those as it reports a missing or
/// wrong password, so lopdf tells them apart (gpui-pdf's 0.47; pdf-inspector
/// reads with 0.45, so a file this lopdf opens counts as one no password
/// opens here): the handler first, whatever was typed, then what the
/// password meets. Only an
/// import that already failed asks; a document lopdf opens is dropped at
/// once, and nothing of it is kept or logged.
fn unsupported_pdf_encryption(bytes: &[u8], password: Option<&Password>) -> bool {
    use lopdf::{Error, LoadOptions, encryption::DecryptionError};
    let unsupported = |error: &Error| {
        matches!(
            error,
            Error::UnsupportedSecurityHandler(_)
                | Error::Decryption(
                    DecryptionError::UnsupportedEncryption
                        | DecryptionError::UnsupportedVersion
                        | DecryptionError::UnsupportedRevision,
                )
        )
    };
    let load = |options| lopdf::Document::load_mem_with_options(bytes, options);
    match load(LoadOptions::default()) {
        // Locked: a password can open it when its handler is the standard one.
        Ok(document) if document.is_encrypted() => {
            if !standard_security_handler(&document) {
                return true;
            }
        }
        // Opened where pdf-inspector's reader couldn't: no password it
        // takes opens it.
        Ok(_) => return true,
        Err(error) => return unsupported(&error),
    }
    let Some(password) = password else {
        return false;
    };
    match load(LoadOptions::with_password(password.expose())) {
        Ok(document) => !document.is_encrypted(),
        Err(error) => unsupported(&error),
    }
}

/// The encrypted `document`'s `/Encrypt` dictionary names the standard
/// (password) security handler at a version (`/V` 1, 2, 4 or 5) and
/// revision (`/R` 2 to 6) it defines: one a password can open. lopdf loads
/// another version or revision as still encrypted and calls any password
/// for it invalid, so it is told apart here.
fn standard_security_handler(document: &lopdf::Document) -> bool {
    use lopdf::Object;
    let dictionary = match document.trailer.get(b"Encrypt") {
        Ok(Object::Reference(id)) => document.get_dictionary(*id).ok(),
        Ok(Object::Dictionary(dictionary)) => Some(dictionary),
        _ => None,
    };
    let Some(dictionary) = dictionary else {
        return false;
    };
    let number = |key: &[u8]| {
        dictionary
            .get(key)
            .ok()
            .and_then(|value| value.as_i64().ok())
    };
    dictionary
        .get(b"Filter")
        .ok()
        .and_then(|filter| filter.as_name().ok())
        == Some(b"Standard".as_slice())
        && number(b"V").is_some_and(|version| matches!(version, 1 | 2 | 4 | 5))
        && number(b"R").is_some_and(|revision| (2..=6).contains(&revision))
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
            "hidden.xlsx",
            "handmade-hidden.xlsb",
            "hidden.xls",
            "hidden.ods",
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
                "hidden.xlsx" | "handmade-hidden.xlsb" | "hidden.xls" | "hidden.ods" => Some(
                    "1 hidden sheet not imported. 1 hidden row not imported. \
                     1 hidden column not imported.",
                ),
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

    /// A password-protected DOCX, XLSX or PPTX, agile (Office 2010 and
    /// later) or standard (Office 2007): no password asks for one, a wrong
    /// one is refused, and the right one imports what the unlocked file
    /// imports. Nothing but the locked file is ever in its folder.
    #[test]
    fn a_locked_office_document_imports_only_with_its_password() {
        const PASSWORD: &str = "Пароль-2026";
        for name in ["text.docx", "sheet.xlsx", "handmade-comments.pptx"] {
            let unlocked = convert(&fixture(name)).unwrap();
            assert!(!unlocked.decrypted, "{name}");
            let plain = std::fs::read(fixture(name)).unwrap();
            for (scheme, locked) in [
                (
                    "agile",
                    craft_offcrypto::encrypt_with_spin_count(&plain, PASSWORD, 1000).unwrap(),
                ),
                (
                    "standard",
                    craft_offcrypto::encrypt_standard(&plain, PASSWORD, 128).unwrap(),
                ),
            ] {
                let case = format!("{name}, {scheme}");
                let dir = tempfile::tempdir().unwrap();
                let path = dir.path().join(format!("locked-{name}"));
                std::fs::write(&path, locked).unwrap();
                let import = |password: Option<&str>| {
                    let password = password.map(|text| Password::new(text.into()));
                    prepare_cancellable(
                        &path,
                        password.as_ref(),
                        None,
                        true,
                        &AtomicBool::new(false),
                    )
                };
                assert!(
                    matches!(import(None), Err(ImportError::PasswordRequired)),
                    "{case}"
                );
                assert!(
                    matches!(import(Some("пароль-2026")), Err(ImportError::WrongPassword)),
                    "{case}"
                );
                let imported = import(Some(PASSWORD)).unwrap();
                assert!(imported.decrypted && imported.source.unlocked(), "{case}");
                assert_eq!(imported.markdown, unlocked.markdown, "{case}");
                assert_eq!(imported.left_out, unlocked.left_out, "{case}");
                assert_eq!(imported.comments, unlocked.comments, "{case}");
                assert_eq!(imported.is_docx, unlocked.is_docx, "{case}");
                let left: Vec<_> = std::fs::read_dir(dir.path())
                    .unwrap()
                    .map(|entry| entry.unwrap().file_name())
                    .collect();
                assert_eq!(left, [path.file_name().unwrap()], "{case}");
            }
        }
    }

    /// Files locked by real Word, Excel, PowerPoint (agile encryption) and
    /// LibreOffice (standard encryption) import only with their password,
    /// as their unlocked twins do, and the folder never gains a file. Their
    /// DOCs (RC4 CryptoAPI from Word, RC4 from LibreOffice) ask for the
    /// password and refuse a wrong one (#1007 item 5).
    #[test]
    fn office_files_locked_by_word_excel_powerpoint_and_libreoffice_import() {
        // The fixtures' published test password (their README).
        const PASSWORD: &str = "Petrov-2026";
        let folder =
            Path::new(env!("CARGO_MANIFEST_DIR")).join("../../tests/fixtures/locked-office");
        let files = || {
            let mut names: Vec<_> = std::fs::read_dir(&folder)
                .unwrap()
                .map(|entry| entry.unwrap().file_name())
                .collect();
            names.sort();
            names
        };
        let before = files();
        let import = |name: &str, password: Option<&str>| {
            let password = password.map(|text| Password::new(text.into()));
            prepare_cancellable(
                &folder.join(name),
                password.as_ref(),
                None,
                true,
                &AtomicBool::new(false),
            )
        };
        for (app, extension) in [
            ("word", "docx"),
            ("excel", "xlsx"),
            ("powerpoint", "pptx"),
            ("libreoffice", "docx"),
            ("libreoffice", "xlsx"),
            ("libreoffice", "pptx"),
        ] {
            let locked = format!("{app}-locked.{extension}");
            assert!(
                matches!(import(&locked, None), Err(ImportError::PasswordRequired)),
                "{locked}"
            );
            assert!(
                matches!(
                    import(&locked, Some("petrov-2026")),
                    Err(ImportError::WrongPassword)
                ),
                "{locked}"
            );
            let imported = import(&locked, Some(PASSWORD)).unwrap();
            let open = import(&format!("{app}-open.{extension}"), None).unwrap();
            assert!(imported.decrypted && !open.decrypted, "{locked}");
            assert!(
                imported.markdown.contains("Petrov"),
                "{locked}: {}",
                imported.markdown
            );
            assert_eq!(imported.markdown, open.markdown, "{locked}");
            assert_eq!(imported.left_out, open.left_out, "{locked}");
        }
        for app in ["word", "libreoffice"] {
            let doc = format!("{app}-locked.doc");
            assert!(
                matches!(import(&doc, None), Err(ImportError::PasswordRequired)),
                "{doc}"
            );
            assert!(
                matches!(
                    import(&doc, Some("petrov-2026")),
                    Err(ImportError::WrongPassword)
                ),
                "{doc}"
            );
            let imported = import(&doc, Some(PASSWORD)).unwrap();
            let open = import(&format!("{app}-open.doc"), None).unwrap();
            assert!(imported.decrypted && !open.decrypted, "{doc}");
            assert!(
                imported.markdown.contains("Petrov"),
                "{doc}: {}",
                imported.markdown
            );
            assert_eq!(imported.markdown, open.markdown, "{doc}");
            assert_eq!(imported.left_out, open.left_out, "{doc}");
        }
        assert_eq!(files(), before);
    }

    /// A password given for a file that isn't locked changes nothing.
    #[test]
    fn a_password_an_unlocked_document_does_not_need_is_ignored() {
        for name in ["text.docx", "text.pdf", "text.doc"] {
            let password = Password::new("unused".into());
            let imported = prepare_cancellable(
                &fixture(name),
                Some(&password),
                None,
                true,
                &AtomicBool::new(false),
            )
            .unwrap();
            assert!(!imported.decrypted, "{name}");
            assert_eq!(
                imported.markdown,
                convert(&fixture(name)).unwrap().markdown,
                "{name}"
            );
        }
    }

    /// A locked package whose encryption no password opens here is named,
    /// before any password is asked for or tried.
    #[test]
    fn a_locked_office_document_names_encryption_it_cannot_unlock() {
        let plain = std::fs::read(fixture("text.docx")).unwrap();
        let mut locked = craft_offcrypto::encrypt_standard(&plain, "secret", 128).unwrap();
        // EncryptionInfo's version: 3.2 (standard, with its CryptoAPI and
        // AES flags) becomes 4.3, the extensible scheme.
        let at = locked
            .windows(8)
            .position(|bytes| bytes == [3, 0, 2, 0, 0x24, 0, 0, 0])
            .unwrap();
        locked[at..at + 4].copy_from_slice(&[4, 0, 3, 0]);
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("locked.docx");
        std::fs::write(&path, locked).unwrap();
        for password in [None, Some(Password::new("secret".into()))] {
            let error = prepare_cancellable(
                &path,
                password.as_ref(),
                None,
                true,
                &AtomicBool::new(false),
            )
            .unwrap_err();
            assert!(
                matches!(&error, ImportError::EncryptionUnsupported(scheme) if scheme == "Extensible"),
                "{error:?}"
            );
            assert_eq!(
                error.to_string(),
                "This document uses Extensible encryption, which mdoc can’t unlock. Import an unlocked copy."
            );
        }
    }

    /// A password never shows: not in its own `Debug`, nor in a source's.
    #[test]
    fn a_password_never_shows() {
        let password = Password::new("Секрет".into());
        assert_eq!(format!("{password:?}"), "Password(..)");
        let source = Source::new("locked.docx".into(), None, "Секрет".as_bytes().to_vec());
        assert!(!format!("{source:?}").contains("Секрет"));
    }

    /// A tab's unlock (#1024 decision 6): the right password leaves the
    /// unlocked bytes in it, and the next conversion (OCR, Try again) reads
    /// those, with no password and without decrypting the file again, and
    /// keeps them. Cancelled before it starts, it keeps them too. Once the
    /// file changes, or for another file, they are dropped and a locked
    /// file asks again.
    #[test]
    fn a_tab_s_unlock_converts_again_without_its_password() {
        const PASSWORD: &str = "Пароль-2026";
        for name in ["text.docx", "sheet.xlsx"] {
            let expected = convert(&fixture(name)).unwrap().markdown;
            let plain = std::fs::read(fixture(name)).unwrap();
            let dir = tempfile::tempdir().unwrap();
            let path = dir.path().join(format!("locked-{name}"));
            let locked = craft_offcrypto::encrypt_with_spin_count(&plain, PASSWORD, 1000).unwrap();
            std::fs::write(&path, &locked).unwrap();
            let run = |unlock: &mut Option<Unlock>, path: &Path| {
                prepare_unlocking(path, unlock, None, true, &AtomicBool::new(false))
            };
            let mut unlock = Some(Unlock::Password {
                path: path.clone(),
                password: Password::new(PASSWORD.into()),
            });
            assert_eq!(
                run(&mut unlock, &path).unwrap().markdown,
                expected,
                "{name}"
            );
            let held = unlock.clone().expect(name);
            let bytes = held.current_source().expect(name).bytes();
            assert_eq!(bytes, plain.as_slice(), "{name}");
            assert!(held.password().is_none(), "{name}: no password is kept");
            for _ in 0..2 {
                let imported = run(&mut unlock, &path).unwrap();
                assert_eq!(imported.markdown, expected, "{name}");
                assert!(imported.decrypted, "{name}");
                let again = unlock
                    .as_ref()
                    .and_then(Unlock::current_source)
                    .expect(name);
                assert_eq!(
                    again.bytes().as_ptr(),
                    bytes.as_ptr(),
                    "{name}: the same bytes"
                );
            }
            let cancelled =
                prepare_unlocking(&path, &mut unlock, None, true, &AtomicBool::new(true));
            assert!(matches!(cancelled, Err(ImportError::Cancelled)), "{name}");
            assert!(unlock.is_some(), "{name}: a cancelled run keeps it");

            let other = dir.path().join(format!("open-{name}"));
            std::fs::write(&other, &plain).unwrap();
            let mut elsewhere = unlock.clone();
            assert_eq!(
                run(&mut elsewhere, &other).unwrap().markdown,
                expected,
                "{name}"
            );
            assert!(elsewhere.is_none(), "{name}: another file drops it");

            std::fs::File::options()
                .write(true)
                .open(&path)
                .unwrap()
                .set_modified(std::time::SystemTime::now() + std::time::Duration::from_secs(60))
                .unwrap();
            assert!(
                matches!(run(&mut unlock, &path), Err(ImportError::PasswordRequired)),
                "{name}: a changed file asks again"
            );
            assert!(unlock.is_none(), "{name}");
            let left: Vec<_> = std::fs::read_dir(dir.path()).unwrap().collect();
            assert_eq!(left.len(), 2, "{name}: nothing unlocked is written");
        }
    }

    /// The password and an unlocked document's bytes go to a view from
    /// memory as shared trait objects: the same text and bytes, not copies.
    #[test]
    fn a_view_shares_the_password_and_bytes_without_copying() {
        let password = Password::new("Секрет".into());
        let text: Arc<dyn AsRef<str> + Send + Sync> = Arc::new(password.clone());
        let shown: &str = (*text).as_ref();
        assert_eq!(shown.as_ptr(), password.expose().as_ptr());
        let source = Source::new("locked.docx".into(), None, b"bytes".to_vec());
        let bytes: Arc<dyn AsRef<[u8]> + Send + Sync> = Arc::new(source.shared_bytes());
        let read: &[u8] = (*bytes).as_ref();
        assert_eq!(read.as_ptr(), source.bytes().as_ptr());
    }

    /// An unlock never shows its password or bytes in its `Debug`.
    #[test]
    fn an_unlock_never_shows_what_it_holds() {
        let password = Unlock::Password {
            path: "locked.pdf".into(),
            password: Password::new("Секрет".into()),
        };
        let unlocked = Unlock::Unlocked(Source::new(
            "locked.docx".into(),
            None,
            "Тайна".as_bytes().to_vec(),
        ));
        for unlock in [password, unlocked] {
            let shown = format!("{unlock:?}");
            assert!(
                !shown.contains("Секрет") && !shown.contains("Тайна"),
                "{shown}"
            );
            assert!(shown.contains("locked."), "{shown}");
        }
    }

    /// An import converts the bytes it holds: a document and a PDF held
    /// under a name no file has import as their files do, comments and
    /// annotations included.
    #[test]
    fn an_import_converts_held_bytes_without_their_file() {
        for name in ["handmade-comments.docx", "text_annotations.pdf"] {
            let from_file = convert(&fixture(name)).unwrap();
            let held = Source::new(
                format!("never-written-{name}").into(),
                None,
                std::fs::read(fixture(name)).unwrap(),
            );
            let imported = import_source(held, None, None, true, &AtomicBool::new(false)).unwrap();
            assert_eq!(imported.markdown, from_file.markdown, "{name}");
            assert_eq!(imported.left_out, from_file.left_out, "{name}");
            assert_eq!(imported.comments, from_file.comments, "{name}");
            assert_eq!(imported.annotations, from_file.annotations, "{name}");
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

    /// A workbook's hidden sheets, rows and columns never reach the Markdown,
    /// and the note counts each kind in a sentence of its own (#1019), the
    /// same for XLSX, XLSB, XLS and ODS; so does a renamed copy.
    #[test]
    fn the_note_counts_hidden_spreadsheet_content_left_out() {
        for name in [
            "hidden.xlsx",
            "handmade-hidden.xlsb",
            "hidden.xls",
            "hidden.ods",
        ] {
            let imported = convert(&fixture(name)).unwrap();
            assert!(!imported.markdown.contains("HIDDEN"), "{name}");
            assert!(imported.markdown.contains("Cora Penhallow"), "{name}");
            assert_eq!(
                imported.note().as_deref(),
                Some(
                    "1 hidden sheet not imported. 1 hidden row not imported. \
                     1 hidden column not imported."
                ),
                "{name}"
            );
            let dir = tempfile::tempdir().unwrap();
            let renamed = dir.path().join("renamed.bin");
            std::fs::copy(fixture(name), &renamed).unwrap();
            assert_eq!(convert(&renamed).unwrap().left_out, imported.left_out);
        }
        // A workbook with nothing hidden counts nothing.
        for name in ["sheet.xlsx", "sheet.xls"] {
            assert_eq!(convert(&fixture(name)).unwrap().note(), None, "{name}");
        }
        let plural = LeftOut {
            hidden_sheets: 1,
            hidden_rows: 3,
            hidden_columns: 2,
            ..LeftOut::default()
        };
        assert_eq!(
            plural.line(true).as_deref(),
            Some(
                "1 hidden sheet not imported. 3 hidden rows not imported. \
                 2 hidden columns not imported."
            )
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

    /// #1004's PDF: one line outside any layer, one in layer L1, which the
    /// default configuration turns off (`/D /OFF`), and one in layer L2,
    /// which it turns on; and an attached file. Fictional names.
    fn pdf_with_an_off_layer() -> Vec<u8> {
        fn stream(dictionary: &str, data: &[u8]) -> Vec<u8> {
            let mut object =
                format!("<< {dictionary}/Length {} >>\nstream\n", data.len()).into_bytes();
            object.extend_from_slice(data);
            object.extend_from_slice(b"\nendstream");
            object
        }
        let content = b"BT /F1 14 Tf 72 720 Td (Visible paragraph on the page.) Tj ET\n\
/OC /L1 BDC\n\
BT /F1 14 Tf 72 690 Td (Off layer: Ivan Petrov, 12 Baker Street) Tj ET\n\
EMC\n\
/OC /L2 BDC\n\
BT /F1 14 Tf 72 660 Td (On layer: second visible line.) Tj ET\n\
EMC\n";
        let attachment = b"Client notes: Ivan Petrov phoned on Monday.\n";
        let objects: Vec<Vec<u8>> = vec![
            b"<< /Type /Catalog /Pages 2 0 R /OCProperties << /OCGs [6 0 R 7 0 R] \
              /D << /Order [6 0 R 7 0 R] /ON [7 0 R] /OFF [6 0 R] >> >> \
              /Names << /EmbeddedFiles << /Names [(client-notes.txt) 8 0 R] >> >> >>"
                .to_vec(),
            b"<< /Type /Pages /Kids [3 0 R] /Count 1 >>".to_vec(),
            b"<< /Type /Page /Parent 2 0 R /MediaBox [0 0 612 792] /Contents 4 0 R \
              /Resources << /Font << /F1 5 0 R >> /Properties << /L1 6 0 R /L2 7 0 R >> >> >>"
                .to_vec(),
            stream("", content),
            b"<< /Type /Font /Subtype /Type1 /BaseFont /Helvetica >>".to_vec(),
            b"<< /Type /OCG /Name (Hidden notes) >>".to_vec(),
            b"<< /Type /OCG /Name (Visible layer) >>".to_vec(),
            b"<< /Type /Filespec /F (client-notes.txt) /UF (client-notes.txt) /EF << /F 9 0 R >> >>"
                .to_vec(),
            stream("/Type /EmbeddedFile ", attachment),
        ];
        let mut pdf = b"%PDF-1.7\n%\xe2\xe3\xcf\xd3\n".to_vec();
        let mut offsets = Vec::new();
        for (index, object) in objects.iter().enumerate() {
            offsets.push(pdf.len());
            pdf.extend_from_slice(format!("{} 0 obj\n", index + 1).as_bytes());
            pdf.extend_from_slice(object);
            pdf.extend_from_slice(b"\nendobj\n");
        }
        let xref = pdf.len();
        pdf.extend_from_slice(
            format!("xref\n0 {}\n0000000000 65535 f \n", objects.len() + 1).as_bytes(),
        );
        for offset in offsets {
            pdf.extend_from_slice(format!("{offset:010} 00000 n \n").as_bytes());
        }
        pdf.extend_from_slice(
            format!(
                "trailer\n<< /Size {} /Root 1 0 R >>\nstartxref\n{xref}\n%%EOF\n",
                objects.len() + 1
            )
            .as_bytes(),
        );
        pdf
    }

    /// Text in a PDF layer the default configuration turns off is in no
    /// reader: it stays out of the Markdown and counts with the other
    /// hidden passages, while the lines outside any layer and in a layer
    /// that is on stay (#1004).
    #[test]
    fn text_in_an_off_pdf_layer_is_left_out_and_counted_as_hidden() {
        let dir = tempfile::tempdir().unwrap();
        let pdf = dir.path().join("layers.pdf");
        std::fs::write(&pdf, pdf_with_an_off_layer()).unwrap();
        let imported = convert(&pdf).unwrap();
        for hidden in ["Off layer", "Ivan Petrov", "Baker Street"] {
            assert!(!imported.markdown.contains(hidden), "{}", imported.markdown);
        }
        for kept in [
            "Visible paragraph on the page.",
            "On layer: second visible line.",
        ] {
            assert!(imported.markdown.contains(kept), "{}", imported.markdown);
        }
        assert_eq!(imported.warning, None);
        assert_eq!(imported.left_out.hidden_text, 1);
        let note = imported.note().unwrap_or_default();
        assert!(note.contains("1 hidden passage not imported."), "{note}");
        assert!(!note.contains("Ivan Petrov"), "{note}");
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

    /// A PDF from `objects`, object 1 its catalog, with a cross-reference
    /// table.
    fn write_pdf(objects: &[String]) -> Vec<u8> {
        let mut pdf = b"%PDF-1.7\n".to_vec();
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
                "trailer\n<< /Size {} /Root 1 0 R >>\nstartxref\n{xref}\n%%EOF\n",
                objects.len() + 1
            )
            .as_bytes(),
        );
        pdf
    }

    fn stream_object(dict: &str, content: &str) -> String {
        format!(
            "<< {dict} /Length {} >>\nstream\n{content}\nendstream",
            content.len()
        )
    }

    /// Files attached to a PDF are counted in the note, each once, and
    /// neither their names nor their content reach the Markdown, the note or
    /// the left-out line (#1005): #1004's PDF with `client-notes.txt` in its
    /// embedded files, a file attachment annotation, and a portfolio.
    #[test]
    fn the_note_counts_attached_files_and_never_names_them() {
        const NOTE: &str = "Client notes: Ivan Petrov phoned on Monday.";
        let font = "<< /Type /Font /Subtype /Type1 /BaseFont /Helvetica >>".to_owned();
        let page_text = "BT /F1 14 Tf 72 720 Td (Visible paragraph on the page.) Tj ET";
        // A paperclip on the page carrying its own file, and a second one
        // pointing at the same file specification, which adds nothing.
        let annotated = write_pdf(&[
            "<< /Type /Catalog /Pages 2 0 R >>".to_owned(),
            "<< /Type /Pages /Kids [3 0 R] /Count 1 >>".to_owned(),
            "<< /Type /Page /Parent 2 0 R /MediaBox [0 0 612 792] /Contents 4 0 R \
             /Resources << /Font << /F1 5 0 R >> >> /Annots [6 0 R 9 0 R] >>"
                .to_owned(),
            stream_object("", page_text),
            font.clone(),
            "<< /Type /Annot /Subtype /FileAttachment /Rect [72 600 90 618] /FS 7 0 R \
             /Contents (witness-statement.txt) >>"
                .to_owned(),
            "<< /Type /Filespec /F (witness-statement.txt) /EF << /F 8 0 R >> >>".to_owned(),
            stream_object("/Type /EmbeddedFile", NOTE),
            "<< /Type /Annot /Subtype /FileAttachment /Rect [100 600 118 618] /FS 7 0 R >>"
                .to_owned(),
        ]);
        // A portfolio keeps its files in the name tree, here across kids.
        let portfolio = write_pdf(&[
            "<< /Type /Catalog /Pages 2 0 R /Collection << /Type /Collection /View /D >> \
             /Names << /EmbeddedFiles 6 0 R >> >>"
                .to_owned(),
            "<< /Type /Pages /Kids [3 0 R] /Count 1 >>".to_owned(),
            "<< /Type /Page /Parent 2 0 R /MediaBox [0 0 612 792] /Contents 4 0 R \
             /Resources << /Font << /F1 5 0 R >> >> >>"
                .to_owned(),
            stream_object("", page_text),
            font,
            "<< /Kids [7 0 R 8 0 R] >>".to_owned(),
            "<< /Limits [(a) (a)] /Names [(exhibit-a.pdf) 9 0 R] >>".to_owned(),
            "<< /Limits [(b) (c)] /Names [(exhibit-b.xlsx) 10 0 R (exhibit-c.docx) 11 0 R] >>"
                .to_owned(),
            "<< /Type /Filespec /UF (exhibit-a.pdf) /EF << /F 12 0 R >> >>".to_owned(),
            "<< /Type /Filespec /UF (exhibit-b.xlsx) /EF << /F 13 0 R >> >>".to_owned(),
            "<< /Type /Filespec /UF (exhibit-c.docx) /EF << /F 14 0 R >> >>".to_owned(),
            stream_object("/Type /EmbeddedFile", NOTE),
            stream_object("/Type /EmbeddedFile", NOTE),
            stream_object("/Type /EmbeddedFile", NOTE),
        ]);
        let dir = tempfile::tempdir().unwrap();
        // #1004's PDF also has a line in an off layer: one hidden passage.
        let embedded_note = "1 hidden passage not imported. 1 attached file not imported.";
        for (name, pdf, files, expected) in [
            ("embedded.pdf", pdf_with_an_off_layer(), 1, embedded_note),
            (
                "annotated.pdf",
                annotated,
                1,
                "1 attached file not imported.",
            ),
            (
                "portfolio.pdf",
                portfolio,
                3,
                "3 attached files not imported.",
            ),
        ] {
            let path = dir.path().join(name);
            std::fs::write(&path, pdf).unwrap();
            let imported = convert(&path).unwrap();
            assert_eq!(imported.left_out.attached_files, files, "{name}");
            assert!(
                imported.markdown.contains("Visible paragraph on the page."),
                "{name}: {}",
                imported.markdown
            );
            let note = imported.note().unwrap();
            let line = imported.left_out.line(false).unwrap();
            assert_eq!(note, expected, "{name}");
            assert_eq!(line, expected, "{name}");
            for text in [&imported.markdown, &note, &line] {
                for leaked in [
                    "client-notes",
                    "witness-statement",
                    "exhibit-",
                    "phoned on Monday",
                ] {
                    assert!(!text.contains(leaked), "{name}: {leaked} in {text}");
                }
            }
        }
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
        assert_eq!(source_comments(&read(&path)), imported.comments);
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
        assert!(source_comments(&read(&fixture("text_annotations.pdf"))).is_empty());
        assert!(source_comments(&damaged("damaged.docx")).is_empty());
    }

    /// A Source shown without an import reads the annotations the import
    /// would: the same native pass.
    #[test]
    fn source_annotations_match_the_import() {
        let path = fixture("text_annotations.pdf");
        let imported = convert(&path).unwrap();
        assert_eq!(imported.annotations.len(), 8);
        assert_eq!(
            source_annotations(&read(&path), None),
            Some(imported.annotations)
        );
        assert_eq!(source_annotations(&damaged("damaged.pdf"), None), None);
        assert_eq!(source_annotations(&read(&fixture("text.docx")), None), None);
    }

    fn read(path: &Path) -> Source {
        Source::read(path).unwrap()
    }

    /// The PDF Source fixtures: `comments.pdf` and its encrypted twins.
    fn pdf_source(name: &str) -> PathBuf {
        Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../../tests/fixtures/pdf-source")
            .join(name)
    }

    /// The encrypted twins' user password, a published test value.
    const PDF_PASSWORD: &str = "Petrov-2026";

    fn import_with(path: &Path, password: Option<&str>) -> Result<Imported, ImportError> {
        let password = password.map(|text| Password::new(text.into()));
        prepare_cancellable(path, password.as_ref(), None, true, &AtomicBool::new(false))
    }

    /// The AES-256 twin with its standard security handler swapped for one
    /// no reader knows, the same length so every offset still holds.
    fn unknown_handler(dir: &Path) -> PathBuf {
        unknown_encryption(dir, b"/Standard", b"/Nonstand")
    }

    /// The AES-256 twin with `from` in its encryption dictionary swapped
    /// for `to`, the same length: an unknown handler, version or revision.
    fn unknown_encryption(dir: &Path, from: &[u8], to: &[u8]) -> PathBuf {
        assert_eq!(from.len(), to.len());
        let mut bytes = std::fs::read(pdf_source("comments-aes256.pdf")).unwrap();
        let at = bytes
            .windows(from.len())
            .position(|window| window == from)
            .expect("the encryption dictionary's entry");
        bytes[at..at + to.len()].copy_from_slice(to);
        let path = dir.join(format!(
            "unknown-{}.pdf",
            String::from_utf8_lossy(&to[1..2])
        ));
        std::fs::write(&path, bytes).unwrap();
        path
    }

    /// A locked PDF (#1024 decision 5) imports with its password as its open
    /// twin does: the same Markdown, counts and annotations. Without one the
    /// card asks, a wrong one is refused, and encryption no password opens
    /// says so whatever is typed. A PDF locked only against editing opens
    /// with no password at all.
    #[test]
    fn a_locked_pdf_imports_only_with_its_password() {
        let open = import_with(&pdf_source("comments.pdf"), None).unwrap();
        for name in ["comments-aes128.pdf", "comments-aes256.pdf"] {
            let path = pdf_source(name);
            assert!(
                matches!(import_with(&path, None), Err(ImportError::PasswordRequired)),
                "{name}"
            );
            assert!(
                matches!(
                    import_with(&path, Some("petrov-2026")),
                    Err(ImportError::WrongPassword)
                ),
                "{name}"
            );
            let imported = import_with(&path, Some(PDF_PASSWORD)).unwrap();
            assert_eq!(imported.markdown, open.markdown, "{name}");
            assert_eq!(imported.left_out, open.left_out, "{name}");
            assert_eq!(imported.annotations, open.annotations, "{name}");
            assert!(imported.is_pdf && !imported.decrypted, "{name}");
        }
        let owner_only = import_with(&pdf_source("comments-owner-only.pdf"), None).unwrap();
        assert_eq!(owner_only.markdown, open.markdown);
        assert_eq!(owner_only.annotations, open.annotations);
        let dir = tempfile::tempdir().unwrap();
        // An unknown handler, version and revision, whatever is typed.
        for unknown in [
            unknown_handler(dir.path()),
            unknown_encryption(dir.path(), b"/V 5/R 6", b"/V 7/R 6"),
            unknown_encryption(dir.path(), b"/V 5/R 6", b"/V 5/R 8"),
        ]
        .iter()
        .flat_map(|path| [None, Some(PDF_PASSWORD), Some("wrong")].map(|p| (path.clone(), p)))
        {
            let (unknown, password) = unknown;
            let error = import_with(&unknown, password).unwrap_err();
            assert!(
                matches!(error, ImportError::PdfEncryptionUnsupported),
                "{}: {password:?}: {error:?}",
                unknown.display()
            );
            assert_eq!(
                error.to_string(),
                "This PDF uses encryption mdoc can’t open. Import an unlocked copy."
            );
        }
    }

    /// A tab keeps a locked PDF's password, never its decrypted bytes, and
    /// every later run opens the PDF with it: one stopped for OCR consent
    /// too. A wrong password, unsupported encryption or another file leave
    /// nothing, so the card asks again.
    #[test]
    fn a_tab_keeps_a_locked_pdf_s_password_for_every_later_run() {
        let path = std::path::absolute(pdf_source("comments-aes256.pdf")).unwrap();
        let expected = import_with(&pdf_source("comments.pdf"), None)
            .unwrap()
            .markdown;
        let run = |unlock: &mut Option<Unlock>, path: &Path| {
            prepare_unlocking(path, unlock, None, true, &AtomicBool::new(false))
        };
        let mut unlock = Some(Unlock::Password {
            path: path.clone(),
            password: Password::new(PDF_PASSWORD.into()),
        });
        for _ in 0..2 {
            assert_eq!(run(&mut unlock, &path).unwrap().markdown, expected);
            let held = unlock.as_ref().expect("the tab keeps the password");
            assert_eq!(held.password().map(Password::expose), Some(PDF_PASSWORD));
            assert!(held.current_source().is_none(), "never the bytes");
            assert!(held.opens(&path));
        }
        let cancelled = prepare_unlocking(&path, &mut unlock, None, true, &AtomicBool::new(true));
        assert!(matches!(cancelled, Err(ImportError::Cancelled)));
        assert!(unlock.is_some(), "a cancelled run keeps it");

        let mut wrong = Some(Unlock::Password {
            path: path.clone(),
            password: Password::new("wrong".into()),
        });
        assert!(matches!(
            run(&mut wrong, &path),
            Err(ImportError::WrongPassword)
        ));
        assert!(wrong.is_none(), "a wrong password is dropped");
        let dir = tempfile::tempdir().unwrap();
        let unknown = unknown_handler(dir.path());
        let mut unsupported = Some(Unlock::Password {
            path: unknown.clone(),
            password: Password::new(PDF_PASSWORD.into()),
        });
        assert!(run(&mut unsupported, &unknown).is_err());
        assert!(unsupported.is_none());
        let other = pdf_source("comments.pdf");
        let mut elsewhere = unlock.clone();
        assert_eq!(run(&mut elsewhere, &other).unwrap().markdown, expected);
        assert!(elsewhere.is_none(), "another file drops it");
        // A PDF that opens without one keeps none.
        let mut none = None;
        run(&mut none, &other).unwrap();
        assert!(none.is_none());
    }

    /// `bytes` encrypted with AES-256 (V5/R6) under `password`, as the
    /// encrypted fixtures are (gpui-pdf's `encrypted_tests`).
    fn encrypted(bytes: &[u8], password: &str) -> Vec<u8> {
        use lopdf::encryption::crypt_filters::{Aes256CryptFilter, CryptFilter};
        let mut document = lopdf::Document::load_mem(bytes).unwrap();
        let filter: std::sync::Arc<dyn CryptFilter> = std::sync::Arc::new(Aes256CryptFilter);
        // A fixed key: the copy protects nothing.
        let key: Vec<u8> = (0u8..32).map(|i| i.wrapping_mul(37) ^ 0x5a).collect();
        let state = lopdf::EncryptionState::try_from(lopdf::EncryptionVersion::V5 {
            encrypt_metadata: true,
            crypt_filters: std::collections::BTreeMap::from([(b"StdCF".to_vec(), filter)]),
            file_encryption_key: &key,
            stream_filter: b"StdCF".to_vec(),
            string_filter: b"StdCF".to_vec(),
            owner_password: "owner-2026",
            user_password: password,
            permissions: lopdf::Permissions::all(),
        })
        .unwrap();
        document.encrypt(&state).unwrap();
        let mut out = Vec::new();
        document.save_to(&mut out).unwrap();
        out
    }

    /// A locked, partly scanned PDF stops to ask for OCR consent with the
    /// same pages as its open copy, and the tab keeps the password that
    /// opened it for the OCR run that follows. A password typed for a run
    /// that never opened the file isn't kept.
    #[test]
    fn a_locked_pdf_stopped_for_ocr_consent_keeps_its_password() {
        let plain = std::fs::read(fixture("handmade-partly-scanned.pdf")).unwrap();
        let dir = tempfile::tempdir().unwrap();
        let path = std::path::absolute(dir.path().join("locked scan.pdf")).unwrap();
        std::fs::write(&path, encrypted(&plain, PDF_PASSWORD)).unwrap();
        let typed = |password: &str| {
            Some(Unlock::Password {
                path: path.clone(),
                password: Password::new(password.into()),
            })
        };
        let open = prepare(&fixture("handmade-partly-scanned.pdf"), None, false);
        let Err(ImportError::NeedsOcr(_, open_pages, ..)) = open else {
            panic!("the open copy asks for OCR");
        };
        let mut unlock = typed(PDF_PASSWORD);
        let stopped = prepare_unlocking(&path, &mut unlock, None, false, &AtomicBool::new(false));
        let Err(ImportError::NeedsOcr(_, pages, ..)) = stopped else {
            panic!("the locked copy asks for OCR: {:?}", stopped.map(|_| ()));
        };
        assert_eq!(pages, open_pages);
        let held = unlock.expect("the tab keeps the password for the OCR run");
        assert_eq!(held.password().map(Password::expose), Some(PDF_PASSWORD));
        let mut wrong = typed("wrong");
        let refused = prepare_unlocking(&path, &mut wrong, None, false, &AtomicBool::new(false));
        assert!(matches!(refused, Err(ImportError::WrongPassword)));
        assert!(wrong.is_none());
    }

    /// A locked PDF's annotations read with the tab's password, as its
    /// import reads them; without it, or with a wrong one, none.
    #[test]
    fn a_locked_pdf_s_annotations_read_with_its_password() {
        let open = source_annotations(&read(&pdf_source("comments.pdf")), None).unwrap();
        assert!(!open.is_empty());
        for name in ["comments-aes128.pdf", "comments-aes256.pdf"] {
            let locked = read(&pdf_source(name));
            let password = Password::new(PDF_PASSWORD.into());
            assert_eq!(
                source_annotations(&locked, Some(&password)),
                Some(open.clone())
            );
            assert_eq!(source_annotations(&locked, None), None, "{name}");
            let wrong = Password::new("wrong".into());
            assert_eq!(source_annotations(&locked, Some(&wrong)), None, "{name}");
        }
    }

    /// Bytes no converter reads, named `name`.
    fn damaged(name: &str) -> Source {
        Source::new(name.into(), None, b"%PDF-1.7 not a document".to_vec())
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
            hidden_sheets: 2,
            hidden_rows: 3,
            hidden_columns: 2,
            field_names: 3,
            attached_files: 5,
            repeated_lines: 4,
            comments: 2,
            comments_unread: 0,
        };
        assert_eq!(
            all.line(false).as_deref(),
            Some(
                "1 picture not imported. 2 hidden passages not imported. \
                 2 hidden sheets not imported. 3 hidden rows not imported. \
                 2 hidden columns not imported. \
                 3 form field names not imported. 5 attached files not imported. \
                 4 repeated header and footer lines removed. 2 comments not shown."
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
                 2 hidden passages not imported. 2 hidden sheets not imported. \
                 3 hidden rows not imported. 2 hidden columns not imported. \
                 3 form field names not imported. \
                 5 attached files not imported. 4 repeated header and footer lines removed."
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

    /// A CSV in a legacy encoding without a BOM imports as its UTF-8 twin
    /// does (#1023): Russian from Windows-1251, KOI8-R or UTF-8 with a BOM,
    /// Western European from Windows-1252.
    #[test]
    fn legacy_encoded_csvs_import_as_their_utf8_twins() {
        for (twin, legacy) in [
            (
                "russian-utf8.csv",
                &[
                    "russian-utf8-bom.csv",
                    "russian-windows-1251.csv",
                    "russian-koi8-r.csv",
                ][..],
            ),
            ("western-utf8.csv", &["western-windows-1252.csv"][..]),
        ] {
            let expected = convert(&fixture(twin)).unwrap();
            for name in legacy {
                let imported = convert(&fixture(name)).unwrap();
                assert_eq!(imported.markdown, expected.markdown, "{name}");
                assert_eq!(imported.note(), expected.note(), "{name}");
            }
        }
        let russian = convert(&fixture("russian-koi8-r.csv")).unwrap().markdown;
        assert!(
            russian.contains("| Белозёрова Анна Викторовна | Москва | бухгалтер | 1 250,00 |"),
            "{russian}"
        );
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
