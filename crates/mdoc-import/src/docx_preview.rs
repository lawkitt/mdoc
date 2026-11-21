//! The DOCX preview: docxide draws a document's bytes as a PDF, in memory.
//!
//! On desktop a supervised worker (this executable in its DOCX worker mode)
//! reads the document on stdin and sends the PDF, its comment anchors and
//! warnings back on stdout, so nothing of the document, locked or not, is
//! written anywhere (#1024 decision 3). iOS renders in this process.
use std::{
    borrow::Cow,
    fs,
    io::{Cursor, Read, Write},
    path::Path,
    process::{Command, Stdio},
    sync::{
        Arc, Mutex,
        atomic::{AtomicBool, Ordering},
    },
    thread,
    time::{Duration, Instant},
};

use zip::ZipArchive;

mod comments;
mod fonts;
mod glyph_fallback;
pub use comments::{AnchorRect, COMMENT_SCHEME, CommentAnchor, LinkMark};

use crate::{
    comments::Comment,
    import::{SharedBytes, Source},
};

/// The comments a preview's cards show: `known`, an import's of the same
/// source, else AnyDoc's reading of it as an import reads it. The worker
/// reads none; the cards meet its anchors by comment id.
fn cards(
    source: &Source,
    known: Option<Arc<Vec<Comment>>>,
    cancel: &AtomicBool,
) -> Arc<Vec<Comment>> {
    known.unwrap_or_else(|| Arc::new(crate::import::source_comments_cancellable(source, cancel)))
}

/// Reads the `/Link` annotations whose URI starts with [`COMMENT_SCHEME`]
/// from a rendered PDF's bytes. The platform supplies its PDF reader.
pub type MarkReader = fn(&SharedBytes) -> Result<Vec<LinkMark>, String>;

/// How the worker draws a document.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct RenderOptions {
    /// Draw Word's hidden text in place, with a dotted underline and muted
    /// ink, instead of leaving it out as Word prints it.
    pub show_hidden_text: bool,
}

/// The worker's argument for [`RenderOptions::show_hidden_text`].
pub const SHOW_HIDDEN_ARG: &str = "--show-hidden";

/// The largest document the preview renders.
pub const MAX_INPUT_BYTES: u64 = 50 * 1024 * 1024;
/// The largest PDF the worker may send back: the largest PDF mdoc opens.
const MAX_OUTPUT_BYTES: u64 = crate::import::MAX_PDF_BYTES;
/// The largest anchors and warnings the worker may send back.
const MAX_METADATA_BYTES: u64 = 16 * 1024 * 1024;
const MAX_ENTRIES: usize = 10_000;
const MAX_PART_UNCOMPRESSED_BYTES: u64 = 250 * 1024 * 1024;
const MAX_TOTAL_UNCOMPRESSED_BYTES: u64 = 250 * 1024 * 1024;
const MAX_CONVERSION_TIME: Duration = Duration::from_secs(30);

/// A DOCX drawn as a PDF in memory: nothing of it is on disk.
pub struct DocxPreview {
    /// The PDF, shared without copying (a view opens it from these bytes)
    /// and wiped when its last holder drops it.
    pub pdf: SharedBytes,
    pub warnings: Vec<String>,
    /// The comments its cards show (AnyDoc's; see `cards`).
    pub comments: Arc<Vec<Comment>>,
    /// Where the comments sit on the rendered pages, in document order, by
    /// comment id.
    pub anchors: Arc<Vec<CommentAnchor>>,
    /// Passages of hidden text in the document, drawn or not.
    pub hidden_text: usize,
    /// The options it was drawn with.
    pub options: RenderOptions,
}

// A replacement waits for the obsolete worker to be killed and reaped.
static WORKER: Mutex<()> = Mutex::new(());
static SHUTTING_DOWN: AtomicBool = AtomicBool::new(false);

/// Called off-thread by GPUI's graceful-quit hook after windows are dropped.
pub fn shutdown_workers() {
    SHUTTING_DOWN.store(true, Ordering::Relaxed);
    let _lock = WORKER.lock().unwrap_or_else(|e| e.into_inner());
}
struct Worker(std::process::Child);
impl Drop for Worker {
    fn drop(&mut self) {
        let _ = self.0.kill();
        let _ = self.0.wait();
    }
}

#[derive(Debug, thiserror::Error)]
pub enum PreviewError {
    #[error("DOCX preview supports files up to 50 MiB")]
    InputTooLarge,
    #[error("DOCX preview does not support macro-bearing documents")]
    MacrosNotSupported,
    #[error("DOCX preview does not support encrypted documents")]
    Encrypted,
    #[error("DOCX preview does not support documents with tracked changes")]
    TrackedChangesNotSupported,
    #[error("DOCX preview does not support nested archives")]
    NestedArchive,
    #[error("DOCX preview package is invalid: {0}")]
    Package(String),
    #[error("DOCX preview could not be rendered: {0}")]
    Render(String),
    #[error("DOCX preview renderer rejected this document")]
    Rejected,
    #[error("DOCX preview conversion timed out after 30 seconds")]
    Timeout,
    #[error("DOCX preview conversion was cancelled")]
    Cancelled,
    #[error("DOCX preview could not read the source: {0}")]
    Read(#[from] std::io::Error),
}

impl PreviewError {
    /// The worker's error report: a kind tag, a tab, then the `Display` text,
    /// so the supervisor can tell a damaged document from a failed worker.
    fn report(&self) -> String {
        let kind = match self {
            Self::InputTooLarge => "too-large",
            Self::MacrosNotSupported => "macros",
            Self::Encrypted => "encrypted",
            Self::TrackedChangesNotSupported => "tracked-changes",
            Self::NestedArchive => "nested-archive",
            Self::Package(_) => "package",
            Self::Render(_) => "render",
            Self::Rejected => "rejected",
            Self::Timeout => "timeout",
            Self::Cancelled => "cancelled",
            Self::Read(_) => "read",
        };
        format!("{kind}\t{self}")
    }

    /// Reads `report` back. Anything unrecognized is a failed worker.
    fn from_report(report: &str) -> Self {
        let (kind, text) = report.split_once('\t').unwrap_or(("", report));
        let detail = |prefix: &str| text.strip_prefix(prefix).unwrap_or(text).to_owned();
        match kind {
            "too-large" => Self::InputTooLarge,
            "macros" => Self::MacrosNotSupported,
            "encrypted" => Self::Encrypted,
            "tracked-changes" => Self::TrackedChangesNotSupported,
            "nested-archive" => Self::NestedArchive,
            "package" => Self::Package(detail("DOCX preview package is invalid: ")),
            "rejected" => Self::Rejected,
            "timeout" => Self::Timeout,
            "cancelled" => Self::Cancelled,
            "read" => Self::Read(std::io::Error::other(detail(
                "DOCX preview could not read the source: ",
            ))),
            "render" => Self::Render(detail("DOCX preview could not be rendered: ")),
            _ => Self::Render(text.to_owned()),
        }
    }

    /// Whether trying again can give a different result: a timeout, a worker
    /// that failed, or a file system problem. The document's own content
    /// fails the same way every time.
    pub fn retryable(&self) -> bool {
        matches!(self, Self::Render(_) | Self::Timeout | Self::Read(_))
    }
}

/// The file at `path`, for a preview no import holds the bytes of: refused
/// past the preview's limit before it is read.
pub fn read_source(path: &Path) -> Result<Source, PreviewError> {
    let path = std::path::absolute(path)?;
    let file = fs::File::open(&path)?;
    let metadata = file.metadata()?;
    if metadata.len() > MAX_INPUT_BYTES {
        return Err(PreviewError::InputTooLarge);
    }
    let bytes = read_within_limit(file)?;
    Ok(Source::new(
        path,
        Some(crate::import::SourceStamp::from(&metadata)),
        bytes,
    ))
}

/// At most the preview's limit from `input`, or `InputTooLarge`.
fn read_within_limit(input: impl Read) -> Result<Vec<u8>, PreviewError> {
    let mut bytes = Vec::new();
    input.take(MAX_INPUT_BYTES + 1).read_to_end(&mut bytes)?;
    if bytes.len() as u64 > MAX_INPUT_BYTES {
        return Err(PreviewError::InputTooLarge);
    }
    Ok(bytes)
}

/// Render in this process on iOS and in headless UI tests. The renderer
/// cannot be stopped mid-document, so there is no worker deadline.
pub fn render_in_process(
    source: &Source,
    cancelled: Arc<AtomicBool>,
    read_marks: impl FnOnce(&SharedBytes) -> Result<Vec<LinkMark>, String>,
    options: RenderOptions,
    comments: Option<Arc<Vec<Comment>>>,
) -> Result<DocxPreview, PreviewError> {
    check_cancel(&cancelled)?;
    let rendered = render_marked(source.bytes(), read_marks, options)?;
    let comments = cards(source, comments, &cancelled);
    Ok(rendered.preview(comments, options))
}

/// Render in a supervised worker: this executable, run as the DOCX worker.
/// `comments` are an import's of the same source, if the caller has them.
pub fn render_with_cancel(
    source: &Source,
    cancelled: Arc<AtomicBool>,
    options: RenderOptions,
    comments: Option<Arc<Vec<Comment>>>,
) -> Result<DocxPreview, PreviewError> {
    render_with_executable(
        source,
        cancelled,
        &std::env::current_exe()?,
        MAX_CONVERSION_TIME,
        options,
        comments,
    )
}

fn check_cancel(cancelled: &AtomicBool) -> Result<(), PreviewError> {
    if cancelled.load(Ordering::Relaxed) || SHUTTING_DOWN.load(Ordering::Relaxed) {
        Err(PreviewError::Cancelled)
    } else {
        Ok(())
    }
}

/// Renders `source` in `executable`'s DOCX worker mode, which reads the
/// document's bytes from stdin and answers on stdout: the worker never opens
/// the user's file, and neither side writes anything to disk.
pub fn render_with_executable(
    source: &Source,
    cancelled: Arc<AtomicBool>,
    executable: &Path,
    timeout: Duration,
    options: RenderOptions,
    comments: Option<Arc<Vec<Comment>>>,
) -> Result<DocxPreview, PreviewError> {
    let _lock = loop {
        check_cancel(&cancelled)?;
        if let Ok(lock) = WORKER.try_lock() {
            break lock;
        }
        thread::sleep(Duration::from_millis(10));
    };
    let started = Instant::now();
    let mut command = Command::new(executable);
    command
        .arg("--mdoc-docx-worker")
        .args(options.show_hidden_text.then_some(SHOW_HIDDEN_ARG))
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::null());
    // docxide refuses to convert while a variable with its old prefix is set.
    for (key, _) in std::env::vars_os() {
        if key.to_string_lossy().starts_with("DOCXSIDE_") {
            command.env_remove(key);
        }
    }
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        command.creation_flags(0x08000000); // CREATE_NO_WINDOW
    }
    let mut child = Worker(
        command
            .spawn()
            .map_err(|e| PreviewError::Render(e.to_string()))?,
    );
    // Fed from a thread of its own, so a worker that stops reading cannot
    // stall supervision; the write fails once the worker is gone.
    let stdin = child.0.stdin.take();
    let input = source.shared_bytes();
    thread::Builder::new()
        .spawn(move || {
            if let Some(mut stdin) = stdin {
                let _ = std::io::Write::write_all(&mut stdin, input.as_ref());
            }
        })
        .map_err(|e| PreviewError::Render(format!("no thread to feed the worker: {e}")))?;
    // Read on a thread of its own too, under the output cap: a reply past it
    // closes the pipe, so the worker fails its write and exits.
    let stdout = child.0.stdout.take();
    let reply = thread::Builder::new()
        .spawn(move || match stdout {
            Some(stdout) => read_reply(stdout),
            None => Err(PreviewError::Render("no worker output".into())),
        })
        .map_err(|e| PreviewError::Render(format!("no thread to read the worker: {e}")))?;
    loop {
        check_cancel(&cancelled)?;
        if started.elapsed() >= timeout {
            return Err(PreviewError::Timeout);
        }
        if let Some(status) = child.0.try_wait()? {
            // The worker is gone, so its end of the pipe is closed.
            let reply = reply.join().unwrap_or_else(|_| {
                Err(PreviewError::Render(
                    "the worker's reply was unreadable".into(),
                ))
            });
            let rendered = match (status.success(), reply) {
                (_, Ok(Reply::Failed(error))) => return Err(error),
                (true, Ok(Reply::Rendered(rendered))) => rendered,
                (true, Err(error)) => return Err(error),
                // A reply past the cap: the reader closed the pipe, so the
                // worker failed its write. Retrying can't help.
                (false, Err(error @ PreviewError::Rejected)) => return Err(error),
                (false, _) => {
                    return Err(PreviewError::Render(format!("worker exited with {status}")));
                }
            };
            let comments = cards(source, comments, &cancelled);
            check_cancel(&cancelled)?;
            return Ok(rendered.preview(comments, options));
        }
        thread::sleep(Duration::from_millis(10));
    }
}

/// The DOCX worker mode: renders the document on stdin and answers on
/// stdout with the PDF and what it learned besides, or the error, for
/// `render_with_executable`.
pub fn run_worker_reporting(
    read_marks: MarkReader,
    options: RenderOptions,
) -> Result<(), PreviewError> {
    let result = read_within_limit(std::io::stdin().lock())
        .and_then(|input| render_marked(&input, read_marks, options));
    let written = write_reply(&mut std::io::stdout().lock(), &result);
    result?;
    written.map_err(PreviewError::Read)
}

/// The worker's answer starts with this line, so anything else on its
/// stdout is a failed worker, not a document.
const REPLY_MAGIC: &[u8] = b"mdoc-docx-reply 1\n";
const REPLY_RENDERED: u8 = b'+';
const REPLY_FAILED: u8 = b'-';

/// The worker's answer: the magic line, then `+`, the metadata's length
/// (u64, little-endian), the metadata as JSON, the PDF's length and the
/// PDF; or `-` and the error's report.
fn write_reply(
    out: &mut impl Write,
    result: &Result<Rendered, PreviewError>,
) -> std::io::Result<()> {
    out.write_all(REPLY_MAGIC)?;
    match result {
        Ok(rendered) => {
            let metadata = serde_json::to_vec(&rendered.metadata)?;
            out.write_all(&[REPLY_RENDERED])?;
            out.write_all(&(metadata.len() as u64).to_le_bytes())?;
            out.write_all(&metadata)?;
            out.write_all(&(rendered.pdf.as_ref().len() as u64).to_le_bytes())?;
            out.write_all(rendered.pdf.as_ref())?;
        }
        Err(error) => {
            out.write_all(&[REPLY_FAILED])?;
            out.write_all(error.report().as_bytes())?;
        }
    }
    out.flush()
}

/// What the worker answered.
enum Reply {
    Rendered(Rendered),
    Failed(PreviewError),
}

/// Reads `write_reply`'s answer, refusing a PDF past [`MAX_OUTPUT_BYTES`]
/// before reading it: the buffer is the PDF's size, never more.
fn read_reply(mut input: impl Read) -> Result<Reply, PreviewError> {
    let mut magic = [0; REPLY_MAGIC.len()];
    let mut kind = [0];
    if input.read_exact(&mut magic).is_err()
        || magic != REPLY_MAGIC
        || input.read_exact(&mut kind).is_err()
    {
        return Err(PreviewError::Render("worker produced no PDF".into()));
    }
    let broken = |_| PreviewError::Render("the worker's reply was cut short".into());
    match kind[0] {
        REPLY_RENDERED => {
            let length = read_length(&mut input).map_err(broken)?;
            if length > MAX_METADATA_BYTES {
                return Err(PreviewError::Rejected);
            }
            let mut metadata = vec![0; length as usize];
            input.read_exact(&mut metadata).map_err(broken)?;
            let metadata: Metadata = serde_json::from_slice(&metadata)
                .map_err(|e| PreviewError::Render(format!("invalid comment metadata: {e}")))?;
            let length = read_length(&mut input).map_err(broken)?;
            if length > MAX_OUTPUT_BYTES {
                return Err(PreviewError::Rejected);
            }
            // Wiped even if the reply stops halfway.
            let mut pdf = zeroize::Zeroizing::new(vec![0; length as usize]);
            input.read_exact(&mut pdf).map_err(broken)?;
            Ok(Reply::Rendered(Rendered {
                pdf: SharedBytes::from(pdf),
                metadata,
            }))
        }
        REPLY_FAILED => {
            let mut report = String::new();
            input
                .take(64 * 1024)
                .read_to_string(&mut report)
                .map_err(broken)?;
            Ok(Reply::Failed(PreviewError::from_report(&report)))
        }
        _ => Err(PreviewError::Render("worker produced no PDF".into())),
    }
}

fn read_length(input: &mut impl Read) -> std::io::Result<u64> {
    let mut length = [0; 8];
    input.read_exact(&mut length)?;
    Ok(u64::from_le_bytes(length))
}

/// A document drawn and its comments placed, in memory: what the worker
/// sends back.
struct Rendered {
    pdf: SharedBytes,
    metadata: Metadata,
}

impl Rendered {
    fn preview(self, comments: Arc<Vec<Comment>>, options: RenderOptions) -> DocxPreview {
        DocxPreview {
            pdf: self.pdf,
            warnings: self.metadata.warnings,
            comments,
            anchors: Arc::new(self.metadata.anchors),
            hidden_text: self.metadata.hidden,
            options,
        }
    }
}

/// What the worker learns about a document besides its pages: where
/// comments sit, by id, never what they say.
#[derive(serde::Serialize, serde::Deserialize)]
struct Metadata {
    anchors: Vec<CommentAnchor>,
    hidden: usize,
    warnings: Vec<String>,
}

/// `bytes` drawn, with its comments placed by `read_marks` reading the PDF.
fn render_marked(
    bytes: &[u8],
    read_marks: impl FnOnce(&SharedBytes) -> Result<Vec<LinkMark>, String>,
    options: RenderOptions,
) -> Result<Rendered, PreviewError> {
    let Drawn {
        pdf,
        warnings,
        hidden,
        placing,
    } = draw(bytes, options)?;
    if pdf.len() as u64 > MAX_OUTPUT_BYTES {
        return Err(PreviewError::Rejected);
    }
    let pdf = SharedBytes::from(pdf);
    let anchors = placing.anchors(|| read_marks(&pdf))?;
    Ok(Rendered {
        pdf,
        metadata: Metadata {
            anchors,
            hidden,
            warnings,
        },
    })
}

/// A document drawn as a PDF, before its comments are placed.
struct Drawn {
    pdf: zeroize::Zeroizing<Vec<u8>>,
    warnings: Vec<String>,
    hidden: usize,
    placing: Placing,
}

/// The comment markers the render copy carries, by comment id.
struct Placing {
    ids: Vec<String>,
    markers: Vec<comments::Marker>,
}

impl Placing {
    /// The comments' anchors, from the marks `read` finds in the PDF; it
    /// isn't called for a document without comments.
    fn anchors(
        &self,
        read: impl FnOnce() -> Result<Vec<LinkMark>, String>,
    ) -> Result<Vec<CommentAnchor>, PreviewError> {
        if self.markers.is_empty() {
            return Ok(Vec::new());
        }
        let marks = read().map_err(PreviewError::Render)?;
        Ok(comments::anchors(&marks, &self.markers, &self.ids))
    }
}

// Validation and rendering share one bounded snapshot inside the terminable
// worker. The deadline therefore includes package inflation and validation.
fn draw(bytes: &[u8], options: RenderOptions) -> Result<Drawn, PreviewError> {
    if bytes.len() as u64 > MAX_INPUT_BYTES {
        return Err(PreviewError::InputTooLarge);
    }
    let warnings = inspect_package(bytes)?;
    let comments::Prepared {
        render,
        ids,
        markers,
        hidden,
    } = comments::prepare(bytes, options.show_hidden_text)?;
    let mut render_input = render.map_or(Cow::Borrowed(bytes), Cow::Owned);
    if let Some(fallback) = glyph_fallback::platform_fallback()
        && let Some(copy) = glyph_fallback::prepare(&render_input, fallback)?
    {
        render_input = Cow::Owned(copy);
    }
    let pdf = docxide_pdf::convert_docx_bytes_to_pdf_bytes_with(
        &render_input,
        &fonts::options(&render_input),
    )
    .map_err(|error| match error {
        // Nothing is written, so an I/O error is the renderer's own.
        docxide_pdf::Error::Io(error) => PreviewError::Render(error.to_string()),
        _ => PreviewError::Rejected,
    })?;
    Ok(Drawn {
        pdf: zeroize::Zeroizing::new(pdf),
        warnings,
        hidden,
        placing: Placing { ids, markers },
    })
}

fn word(node: roxmltree::Node<'_, '_>, name: &str) -> bool {
    node.tag_name().name() == name
        && matches!(
            node.tag_name().namespace(),
            Some(
                "http://schemas.openxmlformats.org/wordprocessingml/2006/main"
                    | "http://purl.oclc.org/ooxml/wordprocessingml/main"
            )
        )
}

/// The package with the parts at the given entry indexes replaced.
fn replace_parts(
    archive: &mut ZipArchive<Cursor<&[u8]>>,
    replacements: &std::collections::HashMap<usize, String>,
) -> Result<Vec<u8>, PreviewError> {
    use std::io::Write;
    let invalid = |e: zip::result::ZipError| PreviewError::Package(e.to_string());
    let mut output = zip::ZipWriter::new(Cursor::new(Vec::new()));
    for i in 0..archive.len() {
        let entry = archive.by_index(i).map_err(invalid)?;
        if let Some(xml) = replacements.get(&i) {
            output
                .start_file(
                    entry.name().map_err(invalid)?,
                    zip::write::SimpleFileOptions::default()
                        .compression_method(zip::CompressionMethod::Deflated),
                )
                .map_err(invalid)?;
            output.write_all(xml.as_bytes())?;
        } else {
            output.raw_copy_file(entry).map_err(invalid)?;
        }
    }
    Ok(output.finish().map_err(invalid)?.into_inner())
}

// Auxiliary OOXML parts (for example customXml) can legally use UTF-16.
// Decode them for policy checks even when the renderer does not consume them.
fn xml_text(bytes: &[u8]) -> Result<std::borrow::Cow<'_, str>, PreviewError> {
    if bytes.starts_with(&[0xff, 0xfe]) || bytes.starts_with(&[0xfe, 0xff]) {
        if !bytes.len().is_multiple_of(2) {
            return Err(PreviewError::Package("invalid UTF-16 XML".into()));
        }
        let little_endian = bytes[0] == 0xff;
        let units = bytes[2..]
            .as_chunks::<2>()
            .0
            .iter()
            .map(|b| {
                if little_endian {
                    u16::from_le_bytes(*b)
                } else {
                    u16::from_be_bytes(*b)
                }
            })
            .collect::<Vec<_>>();
        String::from_utf16(&units)
            .map(std::borrow::Cow::Owned)
            .map_err(|_| PreviewError::Package("invalid UTF-16 XML".into()))
    } else {
        std::str::from_utf8(bytes)
            .map(std::borrow::Cow::Borrowed)
            .map_err(|_| PreviewError::Package("unsupported XML encoding".into()))
    }
}

fn inspect_package(bytes: &[u8]) -> Result<Vec<String>, PreviewError> {
    if bytes.starts_with(&[0xD0, 0xCF, 0x11, 0xE0, 0xA1, 0xB1, 0x1A, 0xE1]) {
        return Err(PreviewError::Encrypted);
    }
    let mut archive = ZipArchive::new(Cursor::new(bytes))
        .map_err(|_| PreviewError::Package("invalid ZIP".into()))?;
    if archive.len() > MAX_ENTRIES {
        return Err(PreviewError::Package("too many ZIP entries".into()));
    }
    let mut total = 0_u64;
    let mut warnings = Vec::new();
    let mut names = std::collections::HashSet::new();
    for index in 0..archive.len() {
        if archive
            .by_index_raw(index)
            .map_err(|_| PreviewError::Package("invalid entry".into()))?
            .encrypted()
        {
            return Err(PreviewError::Encrypted);
        }
        let mut entry = archive
            .by_index(index)
            .map_err(|_| PreviewError::Package("unreadable entry".into()))?;
        let name = entry
            .name()
            .map_err(|_| PreviewError::Package("unreadable entry name".into()))?
            .replace('\\', "/");
        if name.starts_with('/')
            || name.contains(':')
            || name.split('/').any(|p| p == "..")
            || !names.insert(name.clone())
        {
            return Err(PreviewError::Package(
                "unsafe or duplicate entry name".into(),
            ));
        }
        let lower = name.to_ascii_lowercase();
        if lower.ends_with("vbaproject.bin") {
            return Err(PreviewError::MacrosNotSupported);
        }
        if [".zip", ".docx", ".docm", ".xlsx", ".pptx", ".odt", ".epub"]
            .iter()
            .any(|ext| lower.ends_with(ext))
        {
            return Err(PreviewError::NestedArchive);
        }
        if lower.contains("/embeddings/") || lower.contains("/activex/") {
            warnings.push("Embedded objects were omitted.".into());
        }
        let remaining = MAX_TOTAL_UNCOMPRESSED_BYTES - total;
        if entry.size() > remaining || entry.size() > MAX_PART_UNCOMPRESSED_BYTES {
            return Err(PreviewError::Package(
                "expanded content exceeds the preview limit".into(),
            ));
        }
        // Check actual inflation too, including binary parts and their CRCs.
        let mut contents = Vec::new();
        entry
            .by_ref()
            .take(remaining + 1)
            .read_to_end(&mut contents)?;
        total += contents.len() as u64;
        if total > MAX_TOTAL_UNCOMPRESSED_BYTES {
            return Err(PreviewError::Package(
                "expanded content exceeds the preview limit".into(),
            ));
        }
        if contents.starts_with(b"PK\x03\x04") || contents.starts_with(b"PK\x05\x06") {
            return Err(PreviewError::NestedArchive);
        }
        if lower.ends_with(".xml") || lower.ends_with(".rels") {
            let text = xml_text(&contents)?;
            let xml = roxmltree::Document::parse(&text)
                .map_err(|_| PreviewError::Package("invalid XML".into()))?;
            if xml.descendants().filter(|n| n.is_text()).filter_map(|n| n.text()).any(|text| text.chars().any(|c| matches!(c as u32, 0x0590..=0x08ff | 0x2e80..=0x9fff | 0xac00..=0xd7af | 0xfb1d..=0xfdff | 0xfe70..=0xfeff | 0x20000..=0x3134f))) {
                warnings.push("CJK and right-to-left text rendering has not been qualified.".into());
            }
            for node in xml.descendants().filter(|n| n.is_element()) {
                let tag = node.tag_name();
                let ns = tag.namespace().unwrap_or_default();
                let word = ns == "http://schemas.openxmlformats.org/wordprocessingml/2006/main"
                    || ns == "http://purl.oclc.org/ooxml/wordprocessingml/main";
                if word
                    && (matches!(
                        tag.name(),
                        "ins"
                            | "del"
                            | "delText"
                            | "delInstrText"
                            | "moveFrom"
                            | "moveTo"
                            | "cellIns"
                            | "cellDel"
                            | "cellMerge"
                            | "numberingChange"
                    ) || tag.name().ends_with("Change")
                        || tag.name().starts_with("moveFromRange")
                        || tag.name().starts_with("moveToRange"))
                {
                    return Err(PreviewError::TrackedChangesNotSupported);
                }
                if node.attributes().any(|a| {
                    matches!(a.name(), "ContentType" | "Type")
                        && (a.value().to_ascii_lowercase().contains("macroenabled")
                            || a.value().to_ascii_lowercase().contains("vbaproject"))
                }) {
                    return Err(PreviewError::MacrosNotSupported);
                }
                // The pinned renderer reads resources only from ZIP entries.
                // Hyperlinks remain usable and are not omitted resources.
                if tag.name() == "Relationship"
                    && node.attribute("TargetMode") == Some("External")
                    && !node
                        .attribute("Type")
                        .is_some_and(|t| t.ends_with("/hyperlink"))
                {
                    warnings.push("External resources were ignored.".into());
                }
                if word && matches!(tag.name(), "object" | "control") {
                    warnings.push("Embedded objects were omitted.".into());
                }
                // Word writes math settings (`m:mathPr`) into every
                // settings.xml; only an equation itself counts.
                if (ns.contains("/math") && matches!(tag.name(), "oMath" | "oMathPara"))
                    || ns.contains("/chart")
                    || ns.contains("/diagram")
                    || (word
                        && matches!(
                            tag.name(),
                            "altChunk"
                                | "embedRegular"
                                | "embedBold"
                                | "embedItalic"
                                | "embedBoldItalic"
                        ))
                {
                    warnings.push("Equations, charts, SmartArt, embedded fonts, or alternate content may render incompletely.".into());
                }
            }
        }
    }
    warnings.sort();
    warnings.dedup();
    Ok(warnings)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn worker_reports_keep_the_error_kind() {
        for error in [
            PreviewError::InputTooLarge,
            PreviewError::MacrosNotSupported,
            PreviewError::Encrypted,
            PreviewError::TrackedChangesNotSupported,
            PreviewError::NestedArchive,
            PreviewError::Package("invalid ZIP".into()),
            PreviewError::Render("worker crashed".into()),
            PreviewError::Rejected,
            PreviewError::Timeout,
            PreviewError::Read(std::io::Error::other("gone")),
        ] {
            let back = PreviewError::from_report(&error.report());
            assert_eq!(back.to_string(), error.to_string());
            assert_eq!(back.retryable(), error.retryable(), "{error}");
            assert_eq!(
                std::mem::discriminant(&back),
                std::mem::discriminant(&error)
            );
        }
        assert!(matches!(
            PreviewError::from_report("an older worker's text"),
            PreviewError::Render(text) if text == "an older worker's text"
        ));
        assert!(!PreviewError::Package("invalid ZIP".into()).retryable());
        assert!(PreviewError::Timeout.retryable());
    }
    use std::io::Write;
    use zip::write::SimpleFileOptions;

    pub(super) fn package(entries: &[(&str, &[u8])]) -> Vec<u8> {
        let mut output = Cursor::new(Vec::new());
        let mut zip = zip::ZipWriter::new(&mut output);
        for (name, contents) in entries {
            zip.start_file(*name, SimpleFileOptions::default()).unwrap();
            zip.write_all(contents).unwrap();
        }
        zip.finish().unwrap();
        output.into_inner()
    }

    #[test]
    fn cancelled_job_never_starts_a_worker() {
        let result = render_with_executable(
            &Source::new("unused".into(), None, Vec::new()),
            Arc::new(AtomicBool::new(true)),
            Path::new("no-such-executable"),
            MAX_CONVERSION_TIME,
            RenderOptions::default(),
            None,
        );
        assert!(matches!(result, Err(PreviewError::Cancelled)));
    }

    /// A locked DOCX draws from the bytes unlocked in memory.
    #[test]
    fn unlocked_bytes_render_in_memory() {
        let plain = std::fs::read(
            Path::new(env!("CARGO_MANIFEST_DIR")).join("../../tests/fixtures/import/text.docx"),
        )
        .unwrap();
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("locked.docx");
        std::fs::write(
            &path,
            craft_offcrypto::encrypt_with_spin_count(&plain, "fictional-pass", 1000).unwrap(),
        )
        .unwrap();
        let locked = Source::read(&path).unwrap();
        assert!(locked.locked());
        let unlocked = locked
            .unlock(Some(&crate::import::Password::new("fictional-pass".into())))
            .unwrap();
        assert!(unlocked.unlocked() && !unlocked.locked());
        let preview = render_in_process(
            &unlocked,
            Arc::new(AtomicBool::new(false)),
            |_| Ok(Vec::new()),
            RenderOptions::default(),
            None,
        )
        .unwrap();
        assert!(preview.pdf.as_ref().starts_with(b"%PDF-"));
        assert_eq!(std::fs::read_dir(dir.path()).unwrap().count(), 1);
    }

    /// The worker's answer reads back as it was written: the PDF and what
    /// it learned, or the error with its kind.
    #[test]
    fn the_worker_s_reply_reads_back() {
        let rendered = Rendered {
            pdf: SharedBytes::from(zeroize::Zeroizing::new(b"%PDF-1.7 pages".to_vec())),
            metadata: Metadata {
                anchors: vec![CommentAnchor {
                    id: "0".into(),
                    page: 1,
                    rects: vec![AnchorRect {
                        x: 0.1,
                        y: 0.2,
                        w: 0.3,
                        h: 0.04,
                    }],
                    point: false,
                }],
                hidden: 2,
                warnings: vec!["Embedded objects were omitted.".into()],
            },
        };
        let mut reply = Vec::new();
        write_reply(&mut reply, &Ok(rendered)).unwrap();
        let Ok(Reply::Rendered(back)) = read_reply(reply.as_slice()) else {
            panic!("a rendering");
        };
        assert_eq!(back.pdf.as_ref(), b"%PDF-1.7 pages");
        let preview = back.preview(Arc::new(Vec::new()), RenderOptions::default());
        assert_eq!(preview.anchors[0].id, "0");
        assert_eq!(preview.anchors[0].rects[0].w, 0.3);
        assert_eq!(preview.hidden_text, 2);
        assert_eq!(preview.warnings, ["Embedded objects were omitted."]);

        let mut reply = Vec::new();
        write_reply(&mut reply, &Err(PreviewError::TrackedChangesNotSupported)).unwrap();
        assert!(matches!(
            read_reply(reply.as_slice()),
            Ok(Reply::Failed(PreviewError::TrackedChangesNotSupported))
        ));
    }

    /// A reply that isn't one, or is cut short, is a failed worker; a PDF
    /// past the cap is refused from its length, before any of it is read.
    #[test]
    fn a_broken_or_oversized_reply_is_refused() {
        for reply in [
            &b""[..],
            b"%PDF-1.7",
            b"mdoc-docx-reply 1\n",
            b"mdoc-docx-reply 1\n?",
        ] {
            assert!(matches!(read_reply(reply), Err(PreviewError::Render(_))));
        }
        let mut cut = Vec::new();
        write_reply(
            &mut cut,
            &Ok(Rendered {
                pdf: SharedBytes::from(zeroize::Zeroizing::new(vec![b'x'; 100])),
                metadata: Metadata {
                    anchors: Vec::new(),
                    hidden: 0,
                    warnings: Vec::new(),
                },
            }),
        )
        .unwrap();
        cut.truncate(cut.len() - 1);
        assert!(matches!(
            read_reply(cut.as_slice()),
            Err(PreviewError::Render(_))
        ));
        let metadata = br#"{"anchors":[],"hidden":0,"warnings":[]}"#;
        let mut oversized = REPLY_MAGIC.to_vec();
        oversized.push(REPLY_RENDERED);
        oversized.extend((metadata.len() as u64).to_le_bytes());
        oversized.extend(metadata);
        oversized.extend((MAX_OUTPUT_BYTES + 1).to_le_bytes());
        assert!(matches!(
            read_reply(oversized.as_slice()),
            Err(PreviewError::Rejected)
        ));
    }

    #[test]
    fn utf16_auxiliary_parts_are_validated_without_ignoring_revisions() {
        for little_endian in [true, false] {
            let encode = |text: &str| {
                let mut bytes = if little_endian {
                    vec![0xff, 0xfe]
                } else {
                    vec![0xfe, 0xff]
                };
                for c in text.encode_utf16() {
                    bytes.extend(if little_endian {
                        c.to_le_bytes()
                    } else {
                        c.to_be_bytes()
                    });
                }
                bytes
            };
            let clean = encode("<?xml version='1.0' encoding='UTF-16'?><data/>");
            assert!(inspect_package(&package(&[("customXml/item1.xml", &clean)])).is_ok());
            let changed = encode(
                "<x:ins xmlns:x='http://schemas.openxmlformats.org/wordprocessingml/2006/main'/>",
            );
            assert!(matches!(
                inspect_package(&package(&[("customXml/item1.xml", &changed)])),
                Err(PreviewError::TrackedChangesNotSupported)
            ));
        }
    }

    #[test]
    fn rejects_macro_and_tracked_change_packages_before_layout() {
        assert!(matches!(
            inspect_package(&package(&[("word/vbaProject.bin", b"macro")])),
            Err(PreviewError::MacrosNotSupported)
        ));
        assert!(matches!(
            inspect_package(&package(&[("word/document.xml", br#"<x:ins xmlns:x="http://schemas.openxmlformats.org/wordprocessingml/2006/main">text</x:ins>"#)])),
            Err(PreviewError::TrackedChangesNotSupported)
        ));
    }

    #[test]
    fn rejects_nested_archives() {
        assert!(matches!(
            inspect_package(&package(&[("word/nested.docx", b"archive")])),
            Err(PreviewError::NestedArchive)
        ));
    }

    #[test]
    fn rejects_ole_encrypted_packages() {
        assert!(matches!(
            inspect_package(&[0xD0, 0xCF, 0x11, 0xE0, 0xA1, 0xB1, 0x1A, 0xE1]),
            Err(PreviewError::Encrypted)
        ));
    }

    #[test]
    fn reports_omitted_external_and_embedded_content() {
        let warnings = inspect_package(&package(&[
            ("word/_rels/document.xml.rels", br#"<Relationships><Relationship TargetMode='External' Type='image'/></Relationships>"#),
            ("word/embeddings/oleObject1.bin", b"object"),
        ]))
        .unwrap();
        assert!(
            warnings
                .iter()
                .any(|warning| warning == "External resources were ignored.")
        );
        assert!(
            warnings
                .iter()
                .any(|warning| warning == "Embedded objects were omitted.")
        );
    }

    #[test]
    fn complex_content_notice_needs_an_equation_not_math_settings() {
        const NOTICE: &str = "Equations, charts, SmartArt, embedded fonts, or alternate content may render incompletely.";
        let math = "http://schemas.openxmlformats.org/officeDocument/2006/math";
        let word = "http://schemas.openxmlformats.org/wordprocessingml/2006/main";
        // Every Word document carries math settings.
        let settings = format!(
            r#"<w:settings xmlns:w="{word}" xmlns:m="{math}"><m:mathPr><m:mathFont m:val="Cambria Math"/></m:mathPr></w:settings>"#
        );
        let plain =
            inspect_package(&package(&[("word/settings.xml", settings.as_bytes())])).unwrap();
        assert!(!plain.iter().any(|warning| warning == NOTICE));
        let equation = format!(
            r#"<w:document xmlns:w="{word}" xmlns:m="{math}"><w:body><w:p><m:oMath><m:r><m:t>x</m:t></m:r></m:oMath></w:p></w:body></w:document>"#
        );
        let with_equation = inspect_package(&package(&[
            ("word/settings.xml", settings.as_bytes()),
            ("word/document.xml", equation.as_bytes()),
        ]))
        .unwrap();
        assert!(with_equation.iter().any(|warning| warning == NOTICE));
    }

    #[test]
    fn rejects_encrypted_entries_and_entry_count_limit() {
        let mut encrypted = package(&[("data", b"x")]);
        let central = encrypted
            .windows(4)
            .position(|v| v == b"PK\x01\x02")
            .unwrap();
        encrypted[central + 8] |= 1;
        encrypted[6] |= 1;
        assert!(matches!(
            inspect_package(&encrypted),
            Err(PreviewError::Encrypted)
        ));
        let names = (0..=MAX_ENTRIES)
            .map(|i| format!("part{i}"))
            .collect::<Vec<_>>();
        let entries = names
            .iter()
            .map(|name| (name.as_str(), b"".as_slice()))
            .collect::<Vec<_>>();
        assert!(matches!(
            inspect_package(&package(&entries)),
            Err(PreviewError::Package(_))
        ));
    }

    #[test]
    fn policy_uses_xml_namespaces_and_content_types() {
        for tag in ["ins", "del", "moveFrom", "rPrChange", "cellMerge"] {
            let xml = format!(
                "<x:{tag} xmlns:x='http://schemas.openxmlformats.org/wordprocessingml/2006/main'/>"
            );
            assert!(matches!(
                inspect_package(&package(&[("word/document.xml", xml.as_bytes())])),
                Err(PreviewError::TrackedChangesNotSupported)
            ));
        }
        let clean = br#"<w:document xmlns:w="http://schemas.openxmlformats.org/wordprocessingml/2006/main"><w:p w:rsidR="123"/><w:instrText>plain</w:instrText><!-- <w:ins/> --></w:document>"#;
        assert!(inspect_package(&package(&[("word/document.xml", clean)])).is_ok());
        let macros = br#"<Types><Override ContentType="application/vnd.ms-word.document.macroEnabled.main+xml"/></Types>"#;
        assert!(matches!(
            inspect_package(&package(&[("[Content_Types].xml", macros)])),
            Err(PreviewError::MacrosNotSupported)
        ));
        let links = br#"<Relationships><Relationship TargetMode = 'External' Type='http://schemas.openxmlformats.org/officeDocument/2006/relationships/hyperlink' Target='https://example.com'/></Relationships>"#;
        assert!(
            inspect_package(&package(&[("word/_rels/document.xml.rels", links)]))
                .unwrap()
                .is_empty()
        );
    }

    #[test]
    fn rejects_traversal_disguised_archives_and_inflation_limits() {
        assert!(matches!(
            inspect_package(&package(&[("../outside", b"x")])),
            Err(PreviewError::Package(_))
        ));
        let nested = package(&[("nested", b"x")]);
        assert!(matches!(
            inspect_package(&package(&[("word/embeddings/data.bin", &nested)])),
            Err(PreviewError::NestedArchive)
        ));
        // A forged central-directory size must be rejected before inflation.
        let mut oversized = package(&[("word/media/image.bin", b"x")]);
        let central = oversized
            .windows(4)
            .position(|v| v == b"PK\x01\x02")
            .unwrap();
        oversized[central + 24..central + 28]
            .copy_from_slice(&((MAX_TOTAL_UNCOMPRESSED_BYTES + 1) as u32).to_le_bytes());
        assert!(matches!(
            inspect_package(&oversized),
            Err(PreviewError::Package(_))
        ));
    }

    #[test]
    fn renders_representative_docx_to_a_pdf_in_memory() {
        let source =
            Path::new(env!("CARGO_MANIFEST_DIR")).join("../../tests/fixtures/import/text.docx");
        let preview = render_for_test(&source).unwrap();
        assert!(preview.pdf.as_ref().starts_with(b"%PDF-"));
    }

    /// The file at `path`, rendered in process; this crate can't read the
    /// marks, so no anchors.
    pub(super) fn render_for_test(path: &Path) -> Result<DocxPreview, PreviewError> {
        render_in_process(
            &read_source(path)?,
            Arc::new(AtomicBool::new(false)),
            |_| Ok(Vec::new()),
            RenderOptions::default(),
            None,
        )
    }
}
