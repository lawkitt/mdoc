//! Intake: files become Markdown through `mdoc_import` as on
//! desktop. Markdown and plain text open as they are. An image (JPEG, PNG,
//! WebP or GIF: a camera page or a photo) goes through the import's shared
//! image builder, which wraps it as a PDF page in memory, its metadata
//! stripped and its EXIF orientation applied, so PP-OCR reads it through
//! the same PDF path as a scanned document. Photos or camera pages picked
//! together become one document, a page each. HEIC never comes here: each
//! phone's operating system draws it as an upright JPEG first.
use super::{ImportOptions, JobProgress, JobStage};
use crate::CoreError;
use mdoc_import::image_pdf::ImageFormat;
use std::{
    path::{Path, PathBuf},
    sync::atomic::{AtomicBool, Ordering},
};

/// `image` as the copy a phone keeps for its Original: the same pixels
/// without metadata (EXIF with its GPS position, camera and dates, XMP,
/// profiles, comments), its orientation kept. `None` for a format the core
/// doesn't rewrite (HEIC, TIFF) or a file it can't parse, which the phone's
/// operating system then draws as an upright JPEG without metadata, or the
/// phone refuses.
#[uniffi::export]
pub fn image_without_metadata(image: Vec<u8>) -> Option<Vec<u8>> {
    mdoc_import::image_pdf::without_metadata(&image)
}

pub(super) struct Imported {
    pub title: String,
    pub markdown: String,
    pub note: ImportNote,
    /// OCR's kept pages, removed once the session takes the document.
    pub kept: Option<mdoc_import::resume::Completed>,
    #[cfg(not(target_os = "android"))]
    pub unlocked_docx: Option<mdoc_import::import::Source>,
}

/// Import `path`; with `kept`, a PDF's OCR keeps its finished pages there
/// and resumes with the rest.
pub(super) fn import_file(
    path: &Path,
    options: &ImportOptions,
    password: Option<&mdoc_import::import::Password>,
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
            note: ImportNote::default(),
            kept: None,
            #[cfg(not(target_os = "android"))]
            unlocked_docx: None,
        });
    }
    let installed = options
        .ocr
        .as_ref()
        .map(|ocr| ocr.installed())
        .transpose()?;
    let ocr = !options.native_text_only && installed.is_some();
    let image = ImageFormat::from_path(path).is_some();
    stage(if ocr && (extension == "pdf" || image) {
        JobStage::Recognizing
    } else {
        JobStage::Converting
    });
    let installed = installed.as_ref().filter(|_| ocr);
    // A locked PDF opens with its password as a locked DOCX does; its
    // recognized pages are never kept (#1024 decision 7). An image is
    // wrapped in memory: no scratch file (#1021).
    let (imported, completed) = match kept {
        Some(kept) => mdoc_import::resume::prepare_resumable(
            path,
            password,
            installed,
            options.native_text_only,
            cancel,
            kept,
            &|page, pages| advanced(progress, page, pages),
        )?,
        None => (
            mdoc_import::import::prepare_cancellable(
                path,
                password,
                installed,
                options.native_text_only,
                cancel,
            )?,
            None,
        ),
    };
    finish(title, imported, completed, cancel)
}

/// Import `images`, photos or camera pages picked together, as one
/// document titled `title`, a page each in order; with `kept`, OCR keeps
/// its finished pages there and resumes with the rest.
pub(super) fn import_images(
    images: &[PathBuf],
    title: String,
    options: &ImportOptions,
    cancel: &AtomicBool,
    progress: Option<&dyn JobProgress>,
    kept: Option<&mdoc_import::resume::KeptPages>,
) -> Result<Imported, CoreError> {
    if let Some(progress) = progress {
        progress.stage(JobStage::Reading);
    }
    let installed = options
        .ocr
        .as_ref()
        .map(|ocr| ocr.installed())
        .transpose()?
        .filter(|_| !options.native_text_only);
    if let Some(progress) = progress {
        progress.stage(if installed.is_some() {
            JobStage::Recognizing
        } else {
            JobStage::Converting
        });
    }
    let (imported, completed) = match kept {
        Some(kept) => mdoc_import::resume::prepare_resumable_images(
            images,
            installed.as_ref(),
            options.native_text_only,
            cancel,
            kept,
            &|page, pages| advanced(progress, page, pages),
        )?,
        None => (
            mdoc_import::import::prepare_images_cancellable(
                images,
                installed.as_ref(),
                options.native_text_only,
                cancel,
            )?,
            None,
        ),
    };
    finish(title, imported, completed, cancel)
}

/// Each finished page: the bar shows it, and iOS keeps a background job
/// only while it reports progress.
fn advanced(progress: Option<&dyn JobProgress>, page: u32, pages: u32) {
    if let Some(progress) = progress {
        progress.advanced(u64::from(page), u64::from(pages));
    }
}

fn finish(
    title: String,
    imported: mdoc_import::import::Imported,
    kept: Option<mdoc_import::resume::Completed>,
    cancel: &AtomicBool,
) -> Result<Imported, CoreError> {
    if cancel.load(Ordering::Relaxed) {
        return Err(CoreError::Cancelled);
    }
    Ok(Imported {
        title,
        note: ImportNote::of(&imported),
        markdown: imported.markdown,
        kept,
        #[cfg(not(target_os = "android"))]
        unlocked_docx: (imported.is_docx && imported.source.unlocked()).then_some(imported.source),
    })
}

impl ImportNote {
    fn of(imported: &mdoc_import::import::Imported) -> Self {
        Self {
            warning: imported.note(),
            notice: imported.warning.clone(),
            // A Word file's comments show on its rendered original, which
            // iOS has (`crate::original`) and Android not yet; every other
            // format's comments (a PDF's annotations included) count here.
            left_out: imported
                .left_out
                .line(cfg!(not(target_os = "android")) && imported.is_docx),
        }
    }
}

/// What an import says it left out.
#[derive(Clone, Debug, Default, PartialEq)]
pub(super) struct ImportNote {
    /// The notice and the counts as one note, as the apps show it today.
    pub warning: Option<String>,
    /// The notice alone: a partial import (pages OCR skipped).
    pub notice: Option<String>,
    /// The left-out line: what else the Markdown leaves out, counted.
    pub left_out: Option<String>,
}

/// A document's title: its file name without the extension, read as a
/// library document's is (`library::title_of`), so a file the platform
/// named after a title gives that title back.
pub(super) fn title(path: &Path) -> String {
    match crate::library::title_of(path) {
        title if title.is_empty() => "Document".into(),
        title => title,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A DOCX’s comments count only on phones without an Original pane.
    #[test]
    fn a_docx_counts_its_comments_when_the_app_has_no_original_pane() {
        let path = Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../../tests/fixtures/import/handmade-comments.docx");
        let imported = mdoc_import::import::prepare_cancellable(
            &path,
            None,
            None,
            true,
            &AtomicBool::new(false),
        )
        .unwrap();
        assert!(!imported.comments.is_empty());
        let counted = |line: Option<String>| line.is_some_and(|line| line.contains("not shown"));
        assert_eq!(
            counted(ImportNote::of(&imported).left_out),
            cfg!(target_os = "android")
        );
    }

    /// pdf-inspector opens the wrapper and finds one page without text,
    /// so the import asks for OCR, as for a scanned PDF; nothing is written
    /// beside the photo or to the temporary folder.
    #[test]
    fn a_wrapped_image_asks_for_ocr_like_a_scan() {
        let scratch = crate::test_support::Scratch::new();
        let path = scratch.path().join("scan.jpg");
        std::fs::copy(
            Path::new(env!("CARGO_MANIFEST_DIR"))
                .join("../../tests/fixtures/images/gps-rotated.jpg"),
            &path,
        )
        .unwrap();
        let result = import_file(
            &path,
            &ImportOptions::default(),
            None,
            &AtomicBool::new(false),
            None,
            None,
        );
        match result {
            Err(CoreError::NeedsOcr { page_count, .. }) => assert_eq!(page_count, 1),
            Err(other) => panic!("{other}"),
            Ok(_) => panic!("a photo without text has none"),
        }
        let left: Vec<_> = std::fs::read_dir(scratch.path())
            .unwrap()
            .map(|entry| entry.unwrap().file_name())
            .collect();
        assert_eq!(left, ["scan.jpg"]);
    }

    /// The whole note stays as the apps show it today; the notice and the
    /// left-out line come apart beside it, comments counted on the rendered
    /// original instead (only Android, with none, counts them as not shown).
    #[test]
    fn the_note_comes_whole_and_split() {
        let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../../tests/fixtures/import/handmade-hidden-content.docx");
        let imported = import_file(
            &path,
            &ImportOptions::default(),
            None,
            &AtomicBool::new(false),
            None,
            None,
        )
        .unwrap();
        assert_eq!(
            imported.note,
            ImportNote {
                warning: Some("2 comments not imported.".into()),
                notice: None,
                left_out: None,
            }
        );
    }

    /// Phones get the desktop's counts of the comments the forks report:
    /// read ones (RTF, a PDF's annotations) and counted ones (PPTX), all
    /// not shown, and none in the Markdown.
    #[test]
    fn the_left_out_line_counts_every_formats_comments() {
        let fixtures =
            std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../tests/fixtures/import");
        for (name, count) in [
            ("handmade-comments.rtf", 7),
            ("text_annotations.pdf", 8),
            ("handmade-comments.pptx", 4),
        ] {
            let imported = import_file(
                &fixtures.join(name),
                &ImportOptions::default(),
                None,
                &AtomicBool::new(false),
                None,
                None,
            )
            .unwrap();
            assert_eq!(
                imported.note.left_out,
                Some(format!("{count} comments not shown.")),
                "{name}"
            );
            assert!(!imported.markdown.contains("Alice"), "{name}");
        }
    }

    /// A 1×1 PNG, as the phones' screenshots and saved images arrive.
    const PNG: [u8; 70] = [
        0x89, 0x50, 0x4e, 0x47, 0x0d, 0x0a, 0x1a, 0x0a, 0x00, 0x00, 0x00, 0x0d, 0x49, 0x48, 0x44,
        0x52, 0x00, 0x00, 0x00, 0x01, 0x00, 0x00, 0x00, 0x01, 0x08, 0x06, 0x00, 0x00, 0x00, 0x1f,
        0x15, 0xc4, 0x89, 0x00, 0x00, 0x00, 0x0d, 0x49, 0x44, 0x41, 0x54, 0x78, 0xda, 0x63, 0x64,
        0x60, 0xf8, 0x5f, 0x0f, 0x00, 0x02, 0x87, 0x01, 0x80, 0xeb, 0x47, 0xba, 0x92, 0x00, 0x00,
        0x00, 0x00, 0x49, 0x45, 0x4e, 0x44, 0xae, 0x42, 0x60, 0x82,
    ];

    fn fixture(name: &str) -> PathBuf {
        Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../../tests/fixtures")
            .join(name)
    }

    fn names(dir: &Path) -> Vec<String> {
        let mut names: Vec<_> = std::fs::read_dir(dir)
            .unwrap()
            .map(|entry| entry.unwrap().file_name().to_string_lossy().into_owned())
            .collect();
        names.sort();
        names
    }

    /// Every image format goes through the core, never "save it as JPEG"
    /// (#1024 decision 9); a renamed one is known by its content.
    #[test]
    fn every_image_format_is_wrapped_like_a_scan() {
        let scratch = crate::test_support::Scratch::new();
        for (name, bytes) in [("scan.png", PNG.as_slice()), ("scan.bin", PNG.as_slice())] {
            let path = scratch.path().join(name);
            std::fs::write(&path, bytes).unwrap();
            let result = import_file(
                &path,
                &ImportOptions::default(),
                None,
                &AtomicBool::new(false),
                None,
                None,
            );
            match result {
                Err(CoreError::NeedsOcr { page_count, .. }) => assert_eq!(page_count, 1),
                Err(other) => panic!("{name}: {other}"),
                Ok(_) => panic!("{name}: an image without text has none"),
            }
        }
        assert_eq!(names(scratch.path()), ["scan.bin", "scan.png"]);
    }

    /// Photos picked together are one document, a page each, titled as the
    /// phone chose; nothing is written beside them.
    #[test]
    fn photos_picked_together_are_one_document() {
        let scratch = crate::test_support::Scratch::new();
        let photo = scratch.path().join("page-1.jpg");
        std::fs::copy(fixture("images/gps-rotated.jpg"), &photo).unwrap();
        let screenshot = scratch.path().join("page-2.png");
        std::fs::write(&screenshot, PNG).unwrap();
        let result = import_images(
            &[photo, screenshot],
            "Lease".into(),
            &ImportOptions::default(),
            &AtomicBool::new(false),
            None,
            None,
        );
        match result {
            Err(CoreError::NeedsOcr { pages, page_count }) => {
                assert_eq!((pages, page_count), (vec![1, 2], 2));
            }
            Err(other) => panic!("{other}"),
            Ok(_) => panic!("photos without text have none"),
        }
        assert_eq!(names(scratch.path()), ["page-1.jpg", "page-2.png"]);
    }

    /// The copy a phone keeps loses the photo's GPS position and camera but
    /// keeps its orientation; HEIC is left to the phone's operating system.
    #[test]
    fn the_kept_copy_has_no_metadata() {
        let photo = std::fs::read(fixture("images/gps-rotated.jpg")).unwrap();
        let kept = image_without_metadata(photo.clone()).unwrap();
        assert!(kept.len() < photo.len());
        let contains = |needle: &[u8]| kept.windows(needle.len()).any(|w| w == needle);
        assert!(contains(b"Exif\0\0"));
        assert!(!contains(b"FictionalCam"));
        let heic = std::fs::read(fixture("heic/page-gps.heic")).unwrap();
        assert_eq!(image_without_metadata(heic), None);
    }
}
