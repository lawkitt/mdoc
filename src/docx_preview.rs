use std::{
    fs,
    io::{Cursor, Read},
    path::{Path, PathBuf},
    sync::{Arc, Mutex, atomic::AtomicBool},
};
use std::{
    process::{Command, Stdio},
    sync::atomic::Ordering,
    thread,
    time::{Duration, Instant},
};

use tempfile::{TempDir, tempdir};
use zip::ZipArchive;

const MAX_INPUT_BYTES: u64 = 50 * 1024 * 1024;
const MAX_ENTRIES: usize = 10_000;
const MAX_PART_UNCOMPRESSED_BYTES: u64 = 250 * 1024 * 1024;
const MAX_TOTAL_UNCOMPRESSED_BYTES: u64 = 250 * 1024 * 1024;
const MAX_CONVERSION_TIME: Duration = Duration::from_secs(30);

/// The temporary PDF and its directory must live as long as the PDF view.
pub struct DocxPreview {
    pub pdf_path: PathBuf,
    pub warnings: Vec<String>,
    directory: Option<TempDir>,
}

impl Drop for DocxPreview {
    fn drop(&mut self) {
        if let Some(directory) = self.directory.take() {
            CLEANUPS.fetch_add(1, Ordering::Relaxed);
            thread::spawn(move || {
                drop(directory);
                CLEANUPS.fetch_sub(1, Ordering::Release);
            });
        }
    }
}

// A replacement waits for the obsolete worker to be killed and reaped.
static WORKER: Mutex<()> = Mutex::new(());
static SHUTTING_DOWN: AtomicBool = AtomicBool::new(false);
static CLEANUPS: std::sync::atomic::AtomicUsize = std::sync::atomic::AtomicUsize::new(0);

/// Called off-thread by GPUI's graceful-quit hook after windows are dropped.
pub fn shutdown_workers() {
    SHUTTING_DOWN.store(true, Ordering::Relaxed);
    let _lock = WORKER.lock().unwrap_or_else(|e| e.into_inner());
    while CLEANUPS.load(Ordering::Acquire) != 0 {
        thread::sleep(Duration::from_millis(5));
    }
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
    #[error("DOCX preview conversion timed out after 30 seconds")]
    Timeout,
    #[error("DOCX preview conversion was cancelled")]
    Cancelled,
    #[error("DOCX preview could not create temporary storage: {0}")]
    TemporaryStorage(String),
    #[error("DOCX preview could not read the source: {0}")]
    Read(#[from] std::io::Error),
}

// Headless UI tests cannot launch the Rust test harness as an app worker.
// Integration tests exercise the real executable and supervisor separately.
#[cfg(test)]
pub fn render_with_cancel(
    path: &Path,
    cancelled: Arc<AtomicBool>,
) -> Result<DocxPreview, PreviewError> {
    check_cancel(&cancelled)?;
    render_direct(path)
}

#[cfg(not(test))]
pub fn render_with_cancel(
    path: &Path,
    cancelled: Arc<AtomicBool>,
) -> Result<DocxPreview, PreviewError> {
    render_with_executable(
        path,
        cancelled,
        &std::env::current_exe()?,
        MAX_CONVERSION_TIME,
    )
}

fn check_cancel(cancelled: &AtomicBool) -> Result<(), PreviewError> {
    if cancelled.load(Ordering::Relaxed) || SHUTTING_DOWN.load(Ordering::Relaxed) {
        Err(PreviewError::Cancelled)
    } else {
        Ok(())
    }
}

pub fn render_with_executable(
    path: &Path,
    cancelled: Arc<AtomicBool>,
    executable: &Path,
    timeout: Duration,
) -> Result<DocxPreview, PreviewError> {
    let _lock = loop {
        check_cancel(&cancelled)?;
        if let Ok(lock) = WORKER.try_lock() {
            break lock;
        }
        thread::sleep(Duration::from_millis(10));
    };
    let directory = tempdir().map_err(|error| PreviewError::TemporaryStorage(error.to_string()))?;
    let pdf_path = directory.path().join("preview.pdf");
    let started = Instant::now();
    let mut command = Command::new(executable);
    command
        .arg("--mdoc-docx-worker")
        .arg(path)
        .arg(&pdf_path)
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null());
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
    loop {
        check_cancel(&cancelled)?;
        if started.elapsed() >= timeout {
            return Err(PreviewError::Timeout);
        }
        if let Some(status) = child.0.try_wait()? {
            if !status.success() {
                let message = fs::read_to_string(pdf_path.with_extension("error"))
                    .unwrap_or_else(|_| format!("worker exited with {status}"));
                return Err(PreviewError::Render(message));
            }
            if !pdf_path.is_file() {
                return Err(PreviewError::Render("worker produced no PDF".into()));
            }
            let warnings = fs::read_to_string(pdf_path.with_extension("warnings"))?
                .lines()
                .map(str::to_owned)
                .collect();
            return Ok(DocxPreview {
                pdf_path,
                warnings,
                directory: Some(directory),
            });
        }
        thread::sleep(Duration::from_millis(10));
    }
}

#[cfg(test)]
pub fn render_for_test(path: &Path) -> Result<DocxPreview, PreviewError> {
    render_direct(path)
}

pub fn run_worker(input: &Path, output: &Path) -> Result<(), PreviewError> {
    render_direct_to(input, output)
}

#[cfg(test)]
fn render_direct(path: &Path) -> Result<DocxPreview, PreviewError> {
    let directory = tempdir().map_err(|error| PreviewError::TemporaryStorage(error.to_string()))?;
    let pdf_path = directory.path().join("preview.pdf");
    run_worker(path, &pdf_path)?;
    let warnings = fs::read_to_string(pdf_path.with_extension("warnings"))?
        .lines()
        .map(str::to_owned)
        .collect();
    Ok(DocxPreview {
        pdf_path,
        warnings,
        directory: Some(directory),
    })
}

// Validation and rendering share one bounded snapshot inside the terminable
// worker. The deadline therefore includes package inflation and validation.
fn render_direct_to(path: &Path, output: &Path) -> Result<(), PreviewError> {
    let file = fs::File::open(path)?;
    if file.metadata()?.len() > MAX_INPUT_BYTES {
        return Err(PreviewError::InputTooLarge);
    }
    let mut bytes = Vec::new();
    file.take(MAX_INPUT_BYTES + 1).read_to_end(&mut bytes)?;
    if bytes.len() as u64 > MAX_INPUT_BYTES {
        return Err(PreviewError::InputTooLarge);
    }
    let warnings = inspect_package(&bytes)?;
    render_bytes(&bytes, output)?;
    fs::write(output.with_extension("warnings"), warnings.join("\n"))?;
    Ok(())
}

fn render_bytes(bytes: &[u8], output: &Path) -> Result<(), PreviewError> {
    docxide_pdf::convert_docx_bytes_to_pdf(bytes, output)
        .map_err(|_| PreviewError::Render("the renderer rejected this document".into()))?;
    Ok(())
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
        let name = entry.name().replace('\\', "/");
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
            let text = std::str::from_utf8(&contents)
                .map_err(|_| PreviewError::Package("XML must be UTF-8".into()))?;
            let xml = roxmltree::Document::parse(text)
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
                if word
                    && matches!(
                        tag.name(),
                        "comment" | "commentRangeStart" | "commentReference"
                    )
                {
                    warnings.push("Comments are omitted from the preview.".into());
                }
                if word && matches!(tag.name(), "object" | "control") {
                    warnings.push("Embedded objects were omitted.".into());
                }
                if ns.contains("/math")
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
    use std::io::Write;
    use zip::write::SimpleFileOptions;

    fn package(entries: &[(&str, &[u8])]) -> Vec<u8> {
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
            Path::new("unused"),
            Arc::new(AtomicBool::new(true)),
            Path::new("no-such-executable"),
            MAX_CONVERSION_TIME,
        );
        assert!(matches!(result, Err(PreviewError::Cancelled)));
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
    fn renders_representative_docx_to_a_owned_pdf() {
        let source = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/import/text.docx");
        let preview = render_for_test(&source).unwrap();
        assert!(preview.pdf_path.is_file());
        assert!(std::fs::metadata(&preview.pdf_path).unwrap().len() > 0);
    }
}
