//! Installs both OCR bundles the feasibility spike reads (English PP-OCRv6
//! Small and Cyrillic) into a folder, through the desktop's pinned, verified
//! downloads, so `scripts/android-spike-data.sh` can stage them without
//! touching anyone's mdoc profile (ADR 0089):
//!
//! cargo run --release -p mdoc-ffi --example spike_ocr_models -- <ocr-root>
use mdoc_import::{model_download::Progress, ocr::install_config_in};
use mdoc_store::settings::{OcrConfig, OcrModel};
use std::path::Path;

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let [root] = args.as_slice() else {
        eprintln!("usage: spike_ocr_models <ocr-root>");
        std::process::exit(2);
    };
    for model in [OcrModel::V6Small, OcrModel::Cyrillic] {
        let config = OcrConfig {
            model,
            ..Default::default()
        };
        if let Err(error) = install_config_in(Path::new(root), &config, &Progress::default()) {
            eprintln!("spike_ocr_models: {}: {error}", model.name());
            std::process::exit(1);
        }
    }
}
