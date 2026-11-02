//! `just import <file>`: the Markdown mdoc's own import makes of a file,
//! on stdout; its warning and errors on stderr. Not shipped.
//!
//! Probes are judged on this text, not on another converter's (ADR 0093).
//! A PDF that needs OCR is recognized as the app would: the recognition
//! languages come from its text layer, file name and keyboards, else the
//! default bundle, with the installed OCR models and runtimes from the app
//! data (read only, never downloaded; `MDOC_OCR_ROOT` points elsewhere).
use mdoc_import::{
    import::{ImportError, Imported, prepare_cancellable},
    ocr, recognition,
};
use mdoc_store::settings::{OcrConfig, OcrModel};
use std::{
    path::{Path, PathBuf},
    process::ExitCode,
    sync::atomic::AtomicBool,
};

fn main() -> ExitCode {
    let mut args = std::env::args_os().skip(1);
    let (Some(path), None) = (args.next(), args.next()) else {
        eprintln!("usage: just import <file>");
        return ExitCode::from(2);
    };
    match convert(Path::new(&path)) {
        Ok(imported) => {
            if let Some(warning) = &imported.warning {
                eprintln!("warning: {warning}");
            }
            print!("{}", imported.markdown);
            ExitCode::SUCCESS
        }
        Err(error) => {
            eprintln!("error: {error}");
            ExitCode::FAILURE
        }
    }
}

fn convert(path: &Path) -> Result<Imported, String> {
    let cancel = AtomicBool::new(false);
    match prepare_cancellable(path, None, false, &cancel) {
        Err(ImportError::NeedsOcr(source, pages, text, page_count)) => {
            let signals = recognition::Signals {
                text,
                file_name: source
                    .file_name()
                    .map(|name| name.to_string_lossy().into_owned())
                    .unwrap_or_default(),
                input_sources: recognition::input_source_languages(),
            };
            let choice = recognition::detect(
                &signals,
                &ocr::offered(),
                ocr::languages(OcrModel::default()),
            );
            let config = OcrConfig {
                model: ocr::plan(&choice.languages).0,
                ..OcrConfig::default()
            };
            let root = match std::env::var_os("MDOC_OCR_ROOT") {
                Some(root) => PathBuf::from(root),
                None => ocr::root().map_err(|e| e.to_string())?,
            };
            let needs = format!(
                "{} of {page_count} pages need OCR ({}, {})",
                pages.len(),
                OcrModel::name(config.model),
                choice.reason
            );
            let installed = ocr::check_reserved_in(&root, &config)
                .map_err(|e| format!("{needs}: {e}"))?
                .ok_or_else(|| {
                    format!(
                        "{needs}, not set up in {}: set it up in mdoc, or set MDOC_OCR_ROOT",
                        root.display()
                    )
                })?;
            eprintln!("{needs}");
            prepare_cancellable(path, Some(&installed), false, &cancel).map_err(|e| e.to_string())
        }
        result => result.map_err(|e| e.to_string()),
    }
}
