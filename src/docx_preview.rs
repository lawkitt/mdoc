use std::{
    fs,
    io::{Cursor, Read},
    path::{Path, PathBuf},
    sync::{Arc, atomic::AtomicBool},
};
#[cfg(not(test))]
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
#[cfg(not(test))]
const MAX_CONVERSION_TIME: Duration = Duration::from_secs(30);

/// The temporary PDF and its directory must live as long as the PDF view.
pub struct DocxPreview {
    pub pdf_path: PathBuf,
    pub warnings: Vec<String>,
    _directory: TempDir,
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
    #[cfg(not(test))]
    Timeout,
    #[error("DOCX preview conversion was cancelled")]
    #[cfg(not(test))]
    Cancelled,
    #[error("DOCX preview could not create temporary storage: {0}")]
    TemporaryStorage(String),
    #[error("DOCX preview could not read the source: {0}")]
    Read(#[from] std::io::Error),
}

#[cfg(test)]
pub fn render_with_cancel(
    path: &Path,
    _cancelled: Arc<AtomicBool>,
) -> Result<DocxPreview, PreviewError> {
    render_direct(path)
}

#[cfg(not(test))]
pub fn render_with_cancel(
    path: &Path,
    cancelled: Arc<AtomicBool>,
) -> Result<DocxPreview, PreviewError> {
    let metadata = fs::metadata(path)?;
    if metadata.len() > MAX_INPUT_BYTES {
        return Err(PreviewError::InputTooLarge);
    }
    let bytes = fs::read(path)?;
    let warnings = inspect_package(&bytes)?;
    let directory = tempdir().map_err(|error| PreviewError::TemporaryStorage(error.to_string()))?;
    let pdf_path = directory.path().join("preview.pdf");
    let mut child = Command::new(std::env::current_exe()?)
        .arg("--mdoc-docx-worker")
        .arg(path)
        .arg(&pdf_path)
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::piped())
        .spawn()
        .map_err(|error| PreviewError::Render(error.to_string()))?;
    let started = Instant::now();
    loop {
        if let Some(status) = child
            .try_wait()
            .map_err(|error| PreviewError::Render(error.to_string()))?
        {
            if status.success() {
                return Ok(DocxPreview {
                    pdf_path,
                    warnings,
                    _directory: directory,
                });
            }
            let mut message = String::new();
            if let Some(mut stderr) = child.stderr.take() {
                let _ = stderr.read_to_string(&mut message);
            }
            return Err(PreviewError::Render(if message.trim().is_empty() {
                format!("worker exited with {status}")
            } else {
                message.trim().to_owned()
            }));
        }
        if started.elapsed() >= MAX_CONVERSION_TIME {
            let _ = child.kill();
            let _ = child.wait();
            return Err(PreviewError::Timeout);
        }
        if cancelled.load(Ordering::Relaxed) {
            let _ = child.kill();
            let _ = child.wait();
            return Err(PreviewError::Cancelled);
        }
        thread::sleep(Duration::from_millis(20));
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
    let metadata = fs::metadata(path)?;
    if metadata.len() > MAX_INPUT_BYTES {
        return Err(PreviewError::InputTooLarge);
    }
    let bytes = fs::read(path)?;
    let warnings = inspect_package(&bytes)?;
    let directory = tempdir().map_err(|error| PreviewError::TemporaryStorage(error.to_string()))?;
    let pdf_path = directory.path().join("preview.pdf");
    render_bytes(&bytes, &pdf_path)?;
    Ok(DocxPreview {
        pdf_path,
        warnings,
        _directory: directory,
    })
}

fn render_direct_to(path: &Path, output: &Path) -> Result<(), PreviewError> {
    let metadata = fs::metadata(path)?;
    if metadata.len() > MAX_INPUT_BYTES {
        return Err(PreviewError::InputTooLarge);
    }
    let bytes = fs::read(path)?;
    inspect_package(&bytes)?;
    render_bytes(&bytes, output)
}

fn render_bytes(bytes: &[u8], output: &Path) -> Result<(), PreviewError> {
    docxide_pdf::convert_docx_bytes_to_pdf(bytes, output)
        .map_err(|error| PreviewError::Render(error.to_string()))?;
    Ok(())
}

fn inspect_package(bytes: &[u8]) -> Result<Vec<String>, PreviewError> {
    if bytes.starts_with(&[0xD0, 0xCF, 0x11, 0xE0, 0xA1, 0xB1, 0x1A, 0xE1]) {
        return Err(PreviewError::Encrypted);
    }
    let mut archive = ZipArchive::new(Cursor::new(bytes))
        .map_err(|error| PreviewError::Package(error.to_string()))?;
    if archive.len() > MAX_ENTRIES {
        return Err(PreviewError::Package("too many ZIP entries".into()));
    }
    let mut total = 0_u64;
    let mut warnings = Vec::new();
    let mut tracked_changes = false;
    for index in 0..archive.len() {
        let mut entry = archive
            .by_index(index)
            .map_err(|error| PreviewError::Package(error.to_string()))?;
        if entry.encrypted() {
            return Err(PreviewError::Encrypted);
        }
        let name = entry.name().replace('\\', "/");
        if name.ends_with(".zip") || name.ends_with(".docx") || name.ends_with(".docm") {
            return Err(PreviewError::NestedArchive);
        }
        if name.ends_with("vbaProject.bin") || name.contains("/vbaProject.bin") {
            return Err(PreviewError::MacrosNotSupported);
        }
        if (name.contains("/embeddings/")
            || name.contains("/activeX/")
            || name.contains("oleObject"))
            && !warnings
                .iter()
                .any(|warning| warning == "Embedded objects were omitted.")
        {
            warnings.push("Embedded objects were omitted.".into());
        }
        let expanded = entry.size();
        let Some(new_total) = total.checked_add(expanded) else {
            return Err(PreviewError::Package(
                "expanded content exceeds the preview limit".into(),
            ));
        };
        if expanded > MAX_PART_UNCOMPRESSED_BYTES || new_total > MAX_TOTAL_UNCOMPRESSED_BYTES {
            return Err(PreviewError::Package(
                "expanded content exceeds the preview limit".into(),
            ));
        }
        total = new_total;
        let is_xml = name.ends_with(".xml") || name.ends_with(".rels");
        if is_xml {
            let mut contents = Vec::new();
            entry
                .read_to_end(&mut contents)
                .map_err(PreviewError::Read)?;
            let text = String::from_utf8_lossy(&contents);
            if text.contains("<w:ins") || text.contains("<w:del") {
                tracked_changes = true;
            }
            if name.ends_with(".rels") && text.contains("TargetMode=\"External\"") {
                warnings.push("External resources were ignored.".into());
            }
            if name.contains("comments") {
                warnings.push("Comments are omitted from the preview.".into());
            }
        }
    }
    if tracked_changes {
        return Err(PreviewError::TrackedChangesNotSupported);
    }
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
    fn rejects_macro_and_tracked_change_packages_before_layout() {
        assert!(matches!(
            inspect_package(&package(&[("word/vbaProject.bin", b"macro")])),
            Err(PreviewError::MacrosNotSupported)
        ));
        assert!(matches!(
            inspect_package(&package(&[("word/document.xml", b"<w:ins>text</w:ins>")])),
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
            ("word/_rels/document.xml.rels", br#"TargetMode="External""#),
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
    fn renders_representative_docx_to_a_owned_pdf() {
        let source = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/import/text.docx");
        let preview = render_for_test(&source).unwrap();
        assert!(preview.pdf_path.is_file());
        assert!(std::fs::metadata(&preview.pdf_path).unwrap().len() > 0);
    }
}
