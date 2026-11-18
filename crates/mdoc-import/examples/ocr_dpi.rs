//! `ocr_dpi ROOT DPI OUTPUT PDF…`: each PDF through the app's OCR options at
//! DPI with the English/Russian bundle and confidence 0, writing the
//! `<stem>.prepared.md`, `.recognition.txt` and `.metadata.txt` files
//! `tools/ocr-upstream/compare.py` scores, and one `stem<TAB>seconds` line
//! per PDF on stdout. A `*.native.pdf` is extracted without OCR. Measurement
//! tooling, not shipped: `tools/ocr-upstream/dpi.py` drives it.
//!
//! ROOT is throwaway OCR storage: `ocr_dpi --setup ROOT` downloads the
//! pinned runtimes and models there, never into the app's data. A measuring
//! run only resolves them, so its peak memory is the conversion's.
use mdoc_import::{model_download::Progress, ocr};
use mdoc_store::settings::{OcrConfig, OcrModel};
use std::{fs, path::PathBuf, process::ExitCode};

fn main() -> ExitCode {
    match run() {
        Ok(()) => ExitCode::SUCCESS,
        Err(error) => {
            eprintln!("error: {error}");
            ExitCode::FAILURE
        }
    }
}

fn run() -> Result<(), String> {
    let mut args = std::env::args_os().skip(1).peekable();
    let usage = "usage: ocr_dpi --setup ROOT | ocr_dpi ROOT DPI OUTPUT PDF…";
    if args.next_if(|arg| arg == "--setup").is_some() {
        let root = PathBuf::from(args.next().ok_or(usage)?);
        let config = OcrConfig {
            model: OcrModel::Cyrillic,
            ..OcrConfig::default()
        };
        ocr::install_config_in(&root, &config, &Progress::default()).map_err(|e| e.to_string())?;
        return Ok(());
    }
    let root = PathBuf::from(args.next().ok_or(usage)?);
    let dpi: u16 = args
        .next()
        .and_then(|dpi| dpi.to_str()?.parse().ok())
        .ok_or(usage)?;
    let output = PathBuf::from(args.next().ok_or(usage)?);
    let pdfs: Vec<PathBuf> = args.map(PathBuf::from).collect();
    if pdfs.is_empty() {
        return Err(usage.into());
    }
    let config = OcrConfig {
        model: OcrModel::Cyrillic,
        dpi,
        minimum_confidence: 0.,
    };
    let installed = ocr::installed_at(
        &root,
        &config,
        ocr::pdfium_path(&root),
        ocr::onnx_runtime_path(&root),
    )
    .map_err(|e| format!("{e}; run ocr_dpi --setup first"))?;
    fs::create_dir_all(&output).map_err(|e| e.to_string())?;
    for pdf in pdfs {
        let stem = pdf
            .file_stem()
            .ok_or("missing file stem")?
            .to_string_lossy()
            .into_owned();
        let bytes = fs::read(&pdf).map_err(|e| format!("{}: {e}", pdf.display()))?;
        let measured = installed
            .measure(&bytes, !stem.ends_with(".native"))
            .map_err(|e| format!("{stem}: {e}"))?;
        for (suffix, text) in [
            ("prepared.md", &measured.markdown),
            ("recognition.txt", &measured.recognition),
            ("metadata.txt", &measured.metadata),
        ] {
            fs::write(output.join(format!("{stem}.{suffix}")), text).map_err(|e| e.to_string())?;
        }
        println!("{stem}\t{:.3}", measured.seconds);
    }
    Ok(())
}
