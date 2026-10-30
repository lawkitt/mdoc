//! Intake (ADR 0085): files become Markdown through `mdoc_import` as on
//! desktop. Markdown and plain text open as they are. A JPEG (a camera scan
//! or a photo the platform saved upright) is wrapped as a one-page PDF so
//! PP-OCR reads it through the same PDF path as a scanned document.
use super::{ImportOptions, JobProgress, JobStage};
use crate::CoreError;
use std::{
    io::Write,
    path::Path,
    sync::atomic::{AtomicBool, Ordering},
};

pub(super) struct Imported {
    pub title: String,
    pub markdown: String,
    pub warning: Option<String>,
}

/// Import `path`; with `kept`, a PDF's OCR keeps its finished pages there
/// and resumes with the rest.
pub(super) fn import_file(
    path: &Path,
    options: &ImportOptions,
    cancel: &AtomicBool,
    progress: Option<&dyn JobProgress>,
    kept: Option<&mdoc_import::resume::KeptPages>,
) -> Result<Imported, CoreError> {
    let stage = |stage| {
        if let Some(progress) = progress {
            progress.stage(stage);
        }
    };
    stage(JobStage::Reading);
    let title = title(path);
    let extension = path
        .extension()
        .map(|e| e.to_string_lossy().to_ascii_lowercase())
        .unwrap_or_default();
    if mdoc_store::document::is_markdown(path) || extension == "txt" {
        let markdown = std::fs::read_to_string(path).map_err(|error| CoreError::Import {
            message: format!("Could not read document: {error}"),
        })?;
        if cancel.load(Ordering::Relaxed) {
            return Err(CoreError::Cancelled);
        }
        return Ok(Imported {
            title,
            markdown,
            warning: None,
        });
    }
    let installed = options
        .ocr
        .as_ref()
        .map(|ocr| ocr.installed())
        .transpose()?;
    let ocr = !options.native_text_only && installed.is_some();
    let imported = if matches!(extension.as_str(), "jpg" | "jpeg") {
        let image = std::fs::read(path).map_err(|error| CoreError::Import {
            message: format!("Could not read image: {error}"),
        })?;
        let dpi = installed.as_ref().map_or(150, |i| i.config.dpi);
        let pdf = jpeg_pdf(&image, dpi)?;
        let wrapped = Scratch::write(&pdf)?;
        stage(if ocr {
            JobStage::Recognizing
        } else {
            JobStage::Converting
        });
        mdoc_import::import::prepare_cancellable(wrapped.path(), installed.as_ref(), false, cancel)?
    } else if matches!(
        extension.as_str(),
        "png" | "heic" | "heif" | "tif" | "tiff" | "gif"
    ) {
        return Err(CoreError::Import {
            message: "Save the image as JPEG to recognize its text.".into(),
        });
    } else {
        stage(if ocr && extension == "pdf" {
            JobStage::Recognizing
        } else {
            JobStage::Converting
        });
        let installed = installed.as_ref().filter(|_| ocr);
        match kept {
            Some(kept) => mdoc_import::resume::prepare_resumable(
                path,
                installed,
                options.native_text_only,
                cancel,
                kept,
                &|_, _| {},
            )?,
            None => mdoc_import::import::prepare_cancellable(
                path,
                installed,
                options.native_text_only,
                cancel,
            )?,
        }
    };
    if cancel.load(Ordering::Relaxed) {
        return Err(CoreError::Cancelled);
    }
    Ok(Imported {
        title,
        markdown: imported.markdown,
        warning: imported.warning,
    })
}

/// A document's title: its file name without the extension.
pub(super) fn title(path: &Path) -> String {
    path.file_stem()
        .map_or_else(|| "Document".into(), |s| s.to_string_lossy().into_owned())
}

/// A one-page PDF showing `jpeg` (baseline or progressive) at its own
/// pixel size when rendered at `dpi`, the OCR render resolution. PDFium
/// decodes the JPEG itself (DCTDecode); nothing is re-encoded.
pub(super) fn jpeg_pdf(jpeg: &[u8], dpi: u16) -> Result<Vec<u8>, CoreError> {
    let (width, height, components) = jpeg_frame(jpeg).ok_or_else(|| CoreError::Import {
        message: "This image is not a JPEG file mdoc can read.".into(),
    })?;
    let color = match components {
        1 => "/DeviceGray",
        3 => "/DeviceRGB",
        4 => "/DeviceCMYK",
        _ => {
            return Err(CoreError::Import {
                message: "This image is not a JPEG file mdoc can read.".into(),
            });
        }
    };
    let points = |pixels: u16| f64::from(pixels) * 72. / f64::from(dpi.max(1));
    let (w, h) = (points(width), points(height));
    let content = format!("q {w:.2} 0 0 {h:.2} 0 0 cm /Im0 Do Q");
    let mut pdf = b"%PDF-1.4\n%\xe2\xe3\xcf\xd3\n".to_vec();
    let mut offsets = Vec::new();
    let mut object = |pdf: &mut Vec<u8>, head: String, stream: Option<&[u8]>| {
        offsets.push(pdf.len());
        pdf.extend_from_slice(format!("{} 0 obj\n{head}", offsets.len()).as_bytes());
        if let Some(stream) = stream {
            pdf.extend_from_slice(b"\nstream\n");
            pdf.extend_from_slice(stream);
            pdf.extend_from_slice(b"\nendstream");
        }
        pdf.extend_from_slice(b"\nendobj\n");
    };
    object(&mut pdf, "<< /Type /Catalog /Pages 2 0 R >>".into(), None);
    object(
        &mut pdf,
        "<< /Type /Pages /Kids [3 0 R] /Count 1 >>".into(),
        None,
    );
    object(
        &mut pdf,
        format!(
            "<< /Type /Page /Parent 2 0 R /MediaBox [0 0 {w:.2} {h:.2}] \
             /Resources << /XObject << /Im0 4 0 R >> >> /Contents 5 0 R >>"
        ),
        None,
    );
    object(
        &mut pdf,
        format!(
            "<< /Type /XObject /Subtype /Image /Width {width} /Height {height} \
             /ColorSpace {color} /BitsPerComponent 8 /Filter /DCTDecode /Length {} >>",
            jpeg.len()
        ),
        Some(jpeg),
    );
    object(
        &mut pdf,
        format!("<< /Length {} >>", content.len()),
        Some(content.as_bytes()),
    );
    let xref = pdf.len();
    pdf.extend_from_slice(
        format!("xref\n0 {}\n0000000000 65535 f \n", offsets.len() + 1).as_bytes(),
    );
    for offset in &offsets {
        pdf.extend_from_slice(format!("{offset:010} 00000 n \n").as_bytes());
    }
    pdf.extend_from_slice(
        format!(
            "trailer\n<< /Size {} /Root 1 0 R >>\nstartxref\n{xref}\n%%EOF\n",
            offsets.len() + 1
        )
        .as_bytes(),
    );
    Ok(pdf)
}

/// Width, height and component count from a JPEG's start-of-frame segment.
fn jpeg_frame(jpeg: &[u8]) -> Option<(u16, u16, u8)> {
    if !jpeg.starts_with(&[0xff, 0xd8]) {
        return None;
    }
    let mut at = 2;
    while at + 4 <= jpeg.len() {
        if jpeg[at] != 0xff {
            return None;
        }
        let marker = jpeg[at + 1];
        // Fill bytes and standalone markers carry no length.
        if marker == 0xff {
            at += 1;
            continue;
        }
        if marker == 0x01 || (0xd0..=0xd7).contains(&marker) {
            at += 2;
            continue;
        }
        let length = usize::from(u16::from_be_bytes([jpeg[at + 2], jpeg[at + 3]]));
        let frame = matches!(marker, 0xc0..=0xcf) && !matches!(marker, 0xc4 | 0xc8 | 0xcc);
        if frame {
            let segment = jpeg.get(at + 4..at + 2 + length)?;
            let height = u16::from_be_bytes([*segment.get(1)?, *segment.get(2)?]);
            let width = u16::from_be_bytes([*segment.get(3)?, *segment.get(4)?]);
            let components = *segment.get(5)?;
            return (width > 0 && height > 0).then_some((width, height, components));
        }
        if marker == 0xda || length < 2 {
            return None;
        }
        at += 2 + length;
    }
    None
}

/// A temporary file removed when dropped.
struct Scratch(std::path::PathBuf);

impl Scratch {
    fn write(bytes: &[u8]) -> Result<Self, CoreError> {
        let nanos = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map_or(0, |d| d.as_nanos());
        let path =
            std::env::temp_dir().join(format!("mdoc-image-{}-{nanos}.pdf", std::process::id()));
        let mut file = std::fs::File::create(&path)?;
        let scratch = Self(path);
        file.write_all(bytes)?;
        Ok(scratch)
    }

    fn path(&self) -> &Path {
        &self.0
    }
}

impl Drop for Scratch {
    fn drop(&mut self) {
        let _ = std::fs::remove_file(&self.0);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The smallest baseline JPEG: an 8×8 grey frame (SOI, DQT, SOF0, DHT,
    /// SOS, EOI), enough for the frame reader and PDF structure.
    fn tiny_jpeg(width: u16, height: u16) -> Vec<u8> {
        let mut jpeg = vec![0xff, 0xd8];
        // An APP0 segment first, as cameras write.
        jpeg.extend_from_slice(&[0xff, 0xe0, 0x00, 0x04, 0x00, 0x00]);
        jpeg.extend_from_slice(&[0xff, 0xc0, 0x00, 0x0b, 0x08]);
        jpeg.extend_from_slice(&height.to_be_bytes());
        jpeg.extend_from_slice(&width.to_be_bytes());
        jpeg.extend_from_slice(&[0x01, 0x01, 0x11, 0x00]);
        jpeg.extend_from_slice(&[0xff, 0xd9]);
        jpeg
    }

    #[test]
    fn the_frame_gives_the_image_size() {
        assert_eq!(jpeg_frame(&tiny_jpeg(640, 480)), Some((640, 480, 1)));
        assert_eq!(jpeg_frame(b"\x89PNG\r\n"), None);
        assert_eq!(jpeg_frame(&[0xff, 0xd8, 0xff, 0xda, 0x00, 0x02]), None);
    }

    #[test]
    fn a_jpeg_becomes_one_page_at_its_ocr_resolution() {
        let jpeg = tiny_jpeg(1500, 3000);
        let pdf = jpeg_pdf(&jpeg, 150).unwrap();
        let text = String::from_utf8_lossy(&pdf);
        assert!(text.starts_with("%PDF-1.4"));
        assert!(text.contains("/MediaBox [0 0 720.00 1440.00]"));
        assert!(text.contains("/Width 1500 /Height 3000 /ColorSpace /DeviceGray"));
        assert!(pdf.windows(jpeg.len()).any(|w| w == jpeg));
        // Each xref offset points at its object.
        let xref = text.rfind("xref\n").unwrap();
        for (n, line) in text[xref..].lines().skip(3).take(5).enumerate() {
            let offset: usize = line[..10].parse().unwrap();
            assert!(text[offset..].starts_with(&format!("{} 0 obj", n + 1)));
        }
    }

    /// pdf-inspector opens the wrapper and finds one page without text,
    /// so the import asks for OCR, as for a scanned PDF.
    #[test]
    fn a_wrapped_image_asks_for_ocr_like_a_scan() {
        let scratch = crate::test_support::Scratch::new();
        let path = scratch.path().join("scan.jpg");
        std::fs::write(&path, tiny_jpeg(16, 16)).unwrap();
        let result = import_file(
            &path,
            &ImportOptions::default(),
            &AtomicBool::new(false),
            None,
            None,
        );
        match result {
            Err(CoreError::NeedsOcr { page_count, .. }) => assert_eq!(page_count, 1),
            Err(other) => panic!("{other}"),
            Ok(_) => panic!("a blank image has no text"),
        }
    }

    #[test]
    fn other_images_ask_for_jpeg() {
        let scratch = crate::test_support::Scratch::new();
        let path = scratch.path().join("photo.heic");
        std::fs::write(&path, b"....").unwrap();
        let result = import_file(
            &path,
            &ImportOptions::default(),
            &AtomicBool::new(false),
            None,
            None,
        );
        assert!(matches!(result, Err(CoreError::Import { .. })));
    }
}
