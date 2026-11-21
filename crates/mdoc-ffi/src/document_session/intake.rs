//! Intake: files become Markdown through `mdoc_import` as on
//! desktop. Markdown and plain text open as they are. A JPEG (a camera scan
//! or a photo) goes through the import's shared image builder, which wraps
//! it as a one-page PDF in memory, its metadata stripped and its EXIF
//! orientation applied, so PP-OCR reads it through the same PDF path as a
//! scanned document.
use super::{ImportOptions, JobProgress, JobStage};
use crate::CoreError;
use std::{
    path::Path,
    sync::atomic::{AtomicBool, Ordering},
};

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
    let (imported, completed) = if matches!(extension.as_str(), "jpg" | "jpeg") {
        stage(if ocr {
            JobStage::Recognizing
        } else {
            JobStage::Converting
        });
        // The import wraps the photo in memory: no scratch file (#1021).
        let imported = mdoc_import::import::prepare_cancellable(
            path,
            None,
            installed.as_ref(),
            false,
            cancel,
        )?;
        (imported, None)
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
        // A locked PDF opens with its password as a locked DOCX does; its
        // recognized pages are never kept (#1024 decision 7).
        match kept {
            Some(kept) => mdoc_import::resume::prepare_resumable(
                path,
                password,
                installed,
                options.native_text_only,
                cancel,
                kept,
                // Each finished page: the bar shows it, and iOS keeps a
                // background job only while it reports progress.
                &|page, pages| {
                    if let Some(progress) = progress {
                        progress.advanced(u64::from(page), u64::from(pages));
                    }
                },
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
        }
    };
    if cancel.load(Ordering::Relaxed) {
        return Err(CoreError::Cancelled);
    }
    Ok(Imported {
        title,
        note: ImportNote::of(&imported),
        markdown: imported.markdown,
        kept: completed,
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
        let mut imported = mdoc_import::import::prepare_cancellable(
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
        imported.decrypted = true;
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

    #[test]
    fn other_images_ask_for_jpeg() {
        let scratch = crate::test_support::Scratch::new();
        let path = scratch.path().join("photo.heic");
        std::fs::write(&path, b"....").unwrap();
        let result = import_file(
            &path,
            &ImportOptions::default(),
            None,
            &AtomicBool::new(false),
            None,
            None,
        );
        assert!(matches!(result, Err(CoreError::Import { .. })));
    }
}
