//! Resumable OCR outside the library (#1109's notes for Vault & Projects
//! item 9): a Vault document's import keeps its finished OCR pages in a
//! folder the app names, beside its intake files, so the old library's
//! `imports` folder stays untouched. The app picks the folder, as it picks
//! the Vault's (Decision 21); the core never does. Android keeps the
//! library's calls (`import_file_in`, `import_images_in`,
//! `Library::discard_import`) until it switches.
use super::{DocumentSession, ImportOptions, JobProgress, ReviewSnapshot, intake};
use crate::CoreError;
use mdoc_import::resume::KeptPages;
use std::path::{Path, PathBuf};

#[uniffi::export]
impl DocumentSession {
    /// `import_file`, with a PDF's OCR keeping its finished pages in
    /// `kept`, a folder of the app's: after a cancel, or mdoc stopping,
    /// importing the same file with the same OCR model again recognizes
    /// only the pages left, and gives the Markdown an uninterrupted run
    /// gives. The kept pages are removed once this session takes the
    /// document, or by `discard_kept_import`.
    pub fn import_file_resuming(
        &self,
        kept: String,
        path: String,
        mut options: ImportOptions,
        progress: Option<Box<dyn JobProgress>>,
    ) -> Result<ReviewSnapshot, CoreError> {
        let cancel = self.start_job();
        let password = options.take_password();
        let imported = intake::import_file(
            Path::new(&path),
            &options,
            password.as_ref(),
            &cancel,
            progress.as_deref(),
            Some(&KeptPages::new(kept.into())),
        )?;
        self.open(imported, &cancel, None)
    }

    /// `import_images_in` with the pages kept in `kept`, as
    /// `import_file_resuming` keeps them; `discard_kept_images` removes
    /// them for the same images.
    pub fn import_images_resuming(
        &self,
        kept: String,
        images: Vec<String>,
        title: String,
        options: ImportOptions,
        progress: Option<Box<dyn JobProgress>>,
    ) -> Result<ReviewSnapshot, CoreError> {
        let cancel = self.start_job();
        let images: Vec<PathBuf> = images.into_iter().map(PathBuf::from).collect();
        let imported = intake::import_images(
            &images,
            title,
            &options,
            &cancel,
            progress.as_deref(),
            Some(&KeptPages::new(kept.into())),
        )?;
        self.open(imported, &cancel, None)
    }
}

/// Remove the OCR pages kept in `kept` for an import of `source` that has
/// not completed (`DocumentSession::import_file_resuming`), when the user
/// drops it. Call it before removing `source`: kept pages are found by the
/// file's digest.
#[uniffi::export]
pub fn discard_kept_import(kept: String, source: String) -> Result<(), CoreError> {
    Ok(KeptPages::new(kept.into()).discard(Path::new(&source))?)
}

/// `discard_kept_import` for images imported together
/// (`DocumentSession::import_images_resuming`), in the same order.
#[uniffi::export]
pub fn discard_kept_images(kept: String, images: Vec<String>) -> Result<(), CoreError> {
    let images: Vec<PathBuf> = images.into_iter().map(PathBuf::from).collect();
    Ok(KeptPages::new(kept.into()).discard_images(&images)?)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{setup::OcrPaths, test_support::Scratch};
    use std::sync::atomic::{AtomicBool, Ordering};

    fn fixture(name: &str) -> String {
        Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../../tests/fixtures")
            .join(name)
            .display()
            .to_string()
    }

    /// Without OCR the resuming imports give what the plain ones give,
    /// keep nothing (the folder is not even made), and discarding is a
    /// no-op.
    #[test]
    fn an_import_without_ocr_keeps_nothing_in_the_apps_folder() {
        let scratch = Scratch::new();
        let kept = scratch.path().join("intake/ocr");
        let folder = || kept.display().to_string();
        for (name, options) in [
            ("import/text.docx", ImportOptions::default()),
            (
                "import/handmade-partly-scanned.pdf",
                ImportOptions {
                    native_text_only: true,
                    ..ImportOptions::default()
                },
            ),
        ] {
            let resuming = DocumentSession::new()
                .import_file_resuming(folder(), fixture(name), options.clone(), None)
                .unwrap();
            let plain = DocumentSession::new()
                .import_file(fixture(name), options, None)
                .unwrap();
            assert_eq!(resuming, plain, "{name}");
        }
        assert!(matches!(
            DocumentSession::new().import_file_resuming(
                folder(),
                fixture("import/handmade-partly-scanned.pdf"),
                ImportOptions::default(),
                None
            ),
            Err(CoreError::NeedsOcr { .. })
        ));
        let photos = vec![
            fixture("images/gps-rotated.jpg"),
            fixture("images/gps-rotated.jpg"),
        ];
        let result = DocumentSession::new().import_images_resuming(
            folder(),
            photos.clone(),
            "Lease".into(),
            ImportOptions::default(),
            None,
        );
        assert!(
            matches!(
                result,
                Err(CoreError::NeedsOcr { ref pages, page_count: 2 }) if pages == &[1, 2]
            ),
            "{result:?}"
        );
        assert!(!kept.exists(), "nothing kept");
        discard_kept_import(folder(), fixture("import/handmade-partly-scanned.pdf")).unwrap();
        discard_kept_images(folder(), photos).unwrap();
        assert!(!kept.exists());
        // Nothing beside the app's folder either: no library folders.
        assert_eq!(std::fs::read_dir(scratch.path()).unwrap().count(), 0);
    }

    /// With the OCR runtime: an import stopped after two pages keeps them
    /// in the app's folder and resumes with the pages left, giving an
    /// uninterrupted import's document; a dropped import's pages go.
    #[test]
    #[ignore = "downloads OCR runtimes/models; requires Apple Silicon macOS or Windows x64"]
    #[cfg(any(
        all(target_os = "macos", target_arch = "aarch64"),
        all(target_os = "windows", target_arch = "x86_64")
    ))]
    fn a_stopped_ocr_import_resumes_from_the_apps_folder() {
        let scratch = Scratch::new();
        let models = scratch.path().join("models");
        let installed = mdoc_import::ocr::install_config_in(
            &models,
            &mdoc_store::settings::OcrConfig {
                model: mdoc_store::settings::OcrModel::Cyrillic,
                ..Default::default()
            },
            &mdoc_import::model_download::Progress::default(),
        )
        .unwrap();
        let paths = OcrPaths {
            root: models.display().to_string(),
            model: crate::setup::ModelId::OcrEnglishRussian,
            pdfium: installed.pdfium.display().to_string(),
            onnx: installed.onnx.display().to_string(),
        };
        let options = ImportOptions {
            ocr: Some(paths.clone()),
            ..ImportOptions::default()
        };
        let kept = scratch.path().join("intake/ocr");
        let folder = kept.display().to_string();
        let pdf = fixture("import/handmade-partly-scanned.pdf");
        let uninterrupted = DocumentSession::new()
            .import_file(pdf.clone(), options.clone(), None)
            .unwrap();
        let kept_pages = || std::fs::read_dir(&kept).map_or(0, |dir| dir.count());
        // Stop after page 2, as a cancel or mdoc stopping would.
        let stop = || {
            let cancel = AtomicBool::new(false);
            let stopped = mdoc_import::resume::prepare_resumable(
                Path::new(&pdf),
                None,
                Some(&paths.installed().unwrap()),
                false,
                &cancel,
                &KeptPages::new(kept.clone()),
                &|page, _| {
                    if page == 2 {
                        cancel.store(true, Ordering::Relaxed);
                    }
                },
            );
            assert!(matches!(
                stopped,
                Err(mdoc_import::import::ImportError::Cancelled)
            ));
            assert_eq!(kept_pages(), 1);
        };
        stop();
        let resumed = DocumentSession::new()
            .import_file_resuming(folder.clone(), pdf.clone(), options, None)
            .unwrap();
        assert_eq!(resumed, uninterrupted);
        assert_eq!(kept_pages(), 0);
        stop();
        discard_kept_import(folder, pdf).unwrap();
        assert_eq!(kept_pages(), 0);
        assert!(!scratch.path().join("imports").exists());
    }
}
