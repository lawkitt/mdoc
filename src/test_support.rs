//! Fixtures shared by the app's tests (ADR 0069).
use mdoc_import::import::{self, Imported};
use std::{
    path::{Path, PathBuf},
    sync::atomic::AtomicBool,
    time::Duration,
};

/// A finished conversion of `source` with one heading and a partial-import
/// warning, as if its pages needing OCR were skipped.
pub fn converted(source: PathBuf) -> Imported {
    Imported {
        ocr_configuration: None,
        source,
        markdown: "# Imported\n".into(),
        warning: Some("Partial import: pages 2 of 2 require OCR and were skipped.".into()),
        is_pdf: false,
        is_docx: false,
    }
}

/// Import `path` without OCR, as a document open would before OCR consent.
pub fn convert(path: &Path) -> Result<Imported, import::ImportError> {
    import::prepare_cancellable(path, None, true, &AtomicBool::new(false))
}

/// Whether a closed preview's file is gone, allowing for the cleanup thread
/// (slow on Windows CI runners).
pub fn removed(path: &Path) -> bool {
    let deadline = std::time::Instant::now() + Duration::from_secs(10);
    while path.exists() && std::time::Instant::now() < deadline {
        std::thread::sleep(Duration::from_millis(20));
    }
    !path.exists()
}
