//! `mdoc --mdoc-self-test <pdf> <expected.txt>`: the packaged-app release gate
//! (ADR 0037, `just smoke-package`).
//!
//! Runs inside the shipped binary, so it sees what users get: the release
//! profile and, on macOS, the hardened-runtime signature that once refused the
//! downloaded PDFium and ONNX Runtime libraries while every test build passed.
//! Installs the default OCR model into throwaway storage, then recognizes
//! `<pdf>` through the app's own import path and compares it with
//! `<expected.txt>` word for word.

use std::path::Path;

pub fn run(pdf: &Path, expected: &Path) -> Result<(), String> {
    if !mdoc_import::ocr::SUPPORTED {
        return Err("The self-test needs Apple Silicon macOS or Windows x64.".into());
    }
    let storage = tempfile::tempdir().map_err(|e| e.to_string())?;
    let installed = mdoc_import::ocr::install_config_in(
        storage.path(),
        &mdoc_store::settings::OcrConfig::default(),
        &mdoc_import::model_download::Progress::default(),
    )
    .map_err(|e| e.to_string())?;
    let cancel = std::sync::atomic::AtomicBool::new(false);
    let imported = mdoc_import::import::prepare_cancellable(pdf, Some(&installed), false, &cancel)
        .map_err(|e| match e {
            mdoc_import::import::ImportError::NeedsOcr(..) => "OCR did not run.".into(),
            other => other.to_string(),
        })?;
    if imported.ocr_configuration.is_none() {
        return Err("The document converted without OCR.".into());
    }
    let expected = std::fs::read_to_string(expected).map_err(|e| e.to_string())?;
    let got = words(&imported.markdown);
    if got != expected.split_whitespace().collect::<Vec<_>>() {
        return Err(format!("Recognized text differs:\n{}", imported.markdown));
    }
    Ok(())
}

/// Markdown headings recognized from a scan carry `#` marks the fixture lacks.
fn words(markdown: &str) -> Vec<&str> {
    markdown
        .lines()
        .flat_map(|line| line.trim_start_matches('#').split_whitespace())
        .collect()
}
