//! Opt-in same-process OCR + GLiNER2 probe; reuses an installed, offline OCR bundle.
use anyhow::{Context, Result, ensure};
use gliner2_rs::{ExecutionMode, Precision, SchemaTask, SpanConfig, SpanEngine};
use mdoc_pseudonymization_qualification::{model, peak_rss_bytes, sha256_file, verify_model};
use pdf_inspector::vision::{
    ModelArtifactKind, ModelDownloadPolicy, ModelStore, OcrMode, OcrOptions, OcrPdfOptions,
    PP_OCR_CYRILLIC, process_pdf_with_ocr,
};
use std::{
    fs,
    path::{Path, PathBuf},
    time::Instant,
};

fn recognize(ocr: &Path, fixtures: &Path) -> Result<Vec<serde_json::Value>> {
    let mut results = Vec::new();
    let runtime = runtime_path(ocr);
    let models = ModelStore::new(ocr.join("models")).resolve(&PP_OCR_CYRILLIC)?;
    let model_directory = models
        .get(ModelArtifactKind::TextDetection)
        .and_then(Path::parent)
        .context("installed OCR detector missing")?;
    for language in ["english", "russian"] {
        let pdf = fixtures.join(format!("{language}.pdf"));
        let original = sha256_file(&pdf)?;
        let started = Instant::now();
        let output = process_pdf_with_ocr(
            &pdf,
            OcrPdfOptions {
                model_manifest: &PP_OCR_CYRILLIC,
                pdfium_library: Some(ocr.join(if cfg!(windows) {
                    "pdfium.dll"
                } else {
                    "libpdfium.dylib"
                })),
                onnx_runtime_library: Some(runtime.clone()),
                ocr: OcrOptions::new()
                    .mode(OcrMode::Auto)
                    .model_directory(model_directory)
                    .model_downloads(ModelDownloadPolicy::Offline),
                ..Default::default()
            },
        )?;
        let elapsed_ms = started.elapsed().as_secs_f64() * 1000.0;
        let plain = output
            .markdown
            .lines()
            .map(|line| line.trim_start_matches('#').trim())
            .collect::<Vec<_>>()
            .join(" ");
        let expected = fs::read_to_string(fixtures.join(format!("{language}.txt")))?;
        ensure!(
            plain.split_whitespace().collect::<Vec<_>>()
                == expected.split_whitespace().collect::<Vec<_>>(),
            "{language} OCR transcript changed"
        );
        ensure!(sha256_file(&pdf)? == original, "source PDF changed");
        results.push(serde_json::json!({"language":language,"elapsed_ms":elapsed_ms,"matches_expected":true,"source_sha256":original,"source_preserved":true}));
    }
    Ok(results)
}

fn runtime_path(root: &Path) -> PathBuf {
    root.join(if cfg!(windows) {
        "onnxruntime.dll"
    } else {
        "libonnxruntime.1.27.0.dylib"
    })
}

fn main() -> Result<()> {
    let args: Vec<_> = std::env::args_os().skip(1).collect();
    ensure!(args.len() == 4, "CACHE OCR_ROOT OCR_FIXTURES OUTPUT");
    ensure!(
        std::env::var("GLINER2_DEVICE").as_deref() == Ok("cpu"),
        "qualification requires GLINER2_DEVICE=cpu"
    );
    let output = Path::new(&args[3]);
    ensure!(!output.exists(), "output already exists");
    let ocr = Path::new(&args[1]);
    let fixtures = Path::new(&args[2]);
    let runtime = runtime_path(ocr);
    let before = recognize(ocr, fixtures)?;
    let root = verify_model(&model("gliner2-pii-fp16")?, Path::new(&args[0]))?;
    let mut engine = SpanEngine::new(
        SpanConfig::new(root)
            .with_precision(Precision::Fp16)
            .with_execution(ExecutionMode::Standard)
            .with_intra_threads(4),
    )?;
    let schema = [SchemaTask::Entities(vec![
        "person".into(),
        "organization".into(),
        "email".into(),
    ])];
    let mut detected = Vec::new();
    for text in [
        "Alice Morgan represents Northbridge Legal Ltd. Email: alice@example.invalid.",
        "Анна Ёлкина представляет ООО «Ёлка». Почта: anna@example.invalid.",
    ] {
        let inference = engine.extract(text, &schema)?;
        ensure!(
            !inference.entities.is_empty(),
            "no entities in compatibility probe"
        );
        for entity in inference.entities {
            ensure!(
                text.get(entity.char_start..entity.char_end) == Some(entity.text.as_str()),
                "invalid source offset"
            );
            detected.push(serde_json::json!({"text":entity.text,"label":entity.label,"start":entity.char_start,"end":entity.char_end}));
        }
    }
    // OCR's retained engine and the PII engine coexist using the same ort binding.
    let coexist = recognize(ocr, fixtures)?;
    drop(engine);
    let after_drop = recognize(ocr, fixtures)?;
    fs::write(output, serde_json::to_vec_pretty(&serde_json::json!({
        "success":true,"platform":format!("{}-{}",std::env::consts::OS,std::env::consts::ARCH),
        "runtime_sha256":sha256_file(&runtime)?,"pdf_inspector_revision":"620afae42eac4b92fffa2437b89e10a912bb93ee",
        "before_pii":before,"while_pii_loaded":coexist,"after_pii_drop":after_drop,
        "detected":detected,"peak_rss_bytes":peak_rss_bytes(),
    }))?).context("writing compatibility report")?;
    Ok(())
}
