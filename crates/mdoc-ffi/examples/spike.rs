//! The feasibility spike's Mac stand-in (ADR 0085): the same `spike::run`
//! the iOS harness calls, on desktop runtimes. Stage the data first with
//! `scripts/ios-spike-data.sh`, then:
//!
//! cargo run --release -p mdoc-ffi --example spike -- \
//!     target/ios-spike/data <libpdfium.dylib> <libonnxruntime.dylib> <report.json>
use mdoc_ffi::spike::{Runtimes, SpikeInput, report_json, run};
use std::path::{Path, PathBuf};

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let [data, pdfium, onnx, out] = args.as_slice() else {
        eprintln!("usage: spike <data> <pdfium> <onnx> <report.json>");
        std::process::exit(2);
    };
    let data = Path::new(data);
    let report = run(SpikeInput {
        fixtures: data.join("fixtures").display().to_string(),
        ocr_root: data.join("ocr").display().to_string(),
        model_root: model_root(&data.join("pseudonymization"))
            .display()
            .to_string(),
        runtimes: Runtimes {
            pdfium: pdfium.clone(),
            onnx: onnx.clone(),
        },
    })
    .unwrap_or_else(|error| {
        eprintln!("spike: {error}");
        std::process::exit(1);
    });
    std::fs::write(out, report_json(report)).expect("write the report");
}

/// The staged GLiNER2 FP16 folder, `gliner2-pii-fp16-<revision>`.
fn model_root(dir: &Path) -> PathBuf {
    std::fs::read_dir(dir)
        .expect("staged pseudonymization models")
        .filter_map(Result::ok)
        .map(|entry| entry.path())
        .find(|path| {
            path.file_name()
                .is_some_and(|name| name.to_string_lossy().starts_with("gliner2-pii-fp16-"))
        })
        .expect("a gliner2-pii-fp16 folder")
}
