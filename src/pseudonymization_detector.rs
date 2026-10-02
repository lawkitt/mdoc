//! Experimental, offline GLiNER2 integration. Network access belongs only to
//! explicit setup. One bounded scan runs at a time; each scan drops its engine.
use crate::pseudonymization::{Category, Detection};
use gliner2_rs::{
    Chunker, ExecutionMode, InferenceParams, Precision, SchemaTask, SpanConfig, SpanEngine,
    processor::SchemaTransformer,
};
use serde::Deserialize;
use sha2::{Digest, Sha256};
use std::{
    fs::{self, File},
    io::{Read, Write},
    path::{Path, PathBuf},
    sync::{
        Mutex,
        atomic::{AtomicBool, Ordering},
    },
    time::{Duration, Instant},
};

pub const SUPPORTED: bool = crate::ocr::SUPPORTED;
const MAX_SOURCE_BYTES: usize = 2 * 1024 * 1024;
const MAX_INPUT_TOKENS: usize = 512;
const MAX_SCAN_TIME: Duration = Duration::from_secs(120);
static INFERENCE: Mutex<()> = Mutex::new(());
static SETUP: Mutex<()> = Mutex::new(());

const FAMILIES: &[&[(&str, Category)]] = &[
    &[
        ("person", Category::Person),
        ("full_name", Category::Person),
    ],
    &[("organization", Category::Organization)],
    &[
        ("email", Category::Email),
        ("phone_number", Category::Phone),
        ("address", Category::Address),
    ],
    &[
        ("passport_number", Category::Identity),
        ("national_id_number", Category::Identity),
        ("tax_id", Category::Tax),
    ],
    &[
        ("bank_account", Category::Bank),
        ("iban", Category::Bank),
        ("routing_number", Category::Bank),
    ],
];

#[derive(Deserialize)]
struct Manifest {
    files: Vec<Artifact>,
}
#[derive(Deserialize)]
struct Artifact {
    path: String,
    bytes: u64,
    sha256: String,
    url: String,
}
fn manifest() -> Manifest {
    serde_json::from_str(include_str!("../resources/pseudonymization-model.json"))
        .expect("pinned model manifest")
}
fn root() -> Result<PathBuf, String> {
    dirs::data_local_dir()
        .map(|root| root.join("mdoc/pseudonymization/gliner2-pii-fp16-e5948986"))
        .ok_or_else(|| "Could not find local application storage.".into())
}
pub fn download_megabytes() -> u64 {
    // Model + the larger Windows runtime archive, if not already installed.
    (manifest().files.iter().map(|file| file.bytes).sum::<u64>() + 77_086_915).div_ceil(1_000_000)
}
fn verify(path: &Path, artifact: &Artifact) -> Result<(), String> {
    let mut file = File::open(path).map_err(|e| e.to_string())?;
    if file.metadata().map_err(|e| e.to_string())?.len() != artifact.bytes {
        return Err("Model file has an unexpected size. Retry setup.".into());
    }
    let mut digest = Sha256::new();
    let mut buffer = [0; 64 * 1024];
    loop {
        let count = file.read(&mut buffer).map_err(|e| e.to_string())?;
        if count == 0 {
            break;
        }
        digest.update(&buffer[..count]);
    }
    let actual: String = digest
        .finalize()
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect();
    if actual != artifact.sha256 {
        return Err("Model integrity check failed. Retry setup.".into());
    }
    Ok(())
}
fn verify_model(root: &Path) -> Result<(), String> {
    for artifact in manifest().files {
        verify(&root.join(&artifact.path), &artifact)?;
    }
    Ok(())
}

pub fn setup() -> Result<(), String> {
    if !SUPPORTED {
        return Err(
            "Automatic scanning is unavailable on this platform. Manual review is available."
                .into(),
        );
    }
    let _setup = SETUP.lock().map_err(|_| "Model setup was interrupted.")?;
    let root = root()?;
    fs::create_dir_all(&root).map_err(|e| e.to_string())?;
    let agent = ureq::Agent::new_with_config(
        ureq::Agent::config_builder()
            .https_only(true)
            .timeout_global(Some(Duration::from_secs(600)))
            .build(),
    );
    for artifact in manifest().files {
        let path = root.join(&artifact.path);
        if verify(&path, &artifact).is_ok() {
            continue;
        }
        let parent = path.parent().ok_or("Invalid model path.")?;
        fs::create_dir_all(parent).map_err(|e| e.to_string())?;
        let mut response = agent
            .get(&artifact.url)
            .call()
            .map_err(|e| format!("Model download failed: {e}"))?;
        let mut file = tempfile::NamedTempFile::new_in(parent).map_err(|e| e.to_string())?;
        std::io::copy(
            &mut response.body_mut().as_reader().take(artifact.bytes + 1),
            &mut file,
        )
        .map_err(|e| e.to_string())?;
        file.flush().map_err(|e| e.to_string())?;
        verify(file.path(), &artifact)?;
        file.as_file().sync_all().map_err(|e| e.to_string())?;
        file.persist(path).map_err(|e| e.to_string())?;
    }
    crate::ocr::onnx_runtime(true)?;
    let licenses = root.join("licenses");
    fs::create_dir_all(&licenses).map_err(|e| e.to_string())?;
    fs::write(
        licenses.join("Apache-2.0.txt"),
        include_str!("../resources/gliner2-license.txt"),
    )
    .map_err(|e| e.to_string())?;
    fs::write(
        licenses.join("gliner2-NOTICE.txt"),
        include_str!("../resources/gliner2-notice.txt"),
    )
    .map_err(|e| e.to_string())?;
    fs::write(
        licenses.join("Microsoft-MIT.txt"),
        include_str!("../resources/pseudonymization-microsoft-license.txt"),
    )
    .map_err(|e| e.to_string())?;
    fs::write(licenses.join("MODEL-NOTICE.txt"), "Experimental unmodified GLiNER2 privacy PII FP16 export.\nExport: Jugaad s.r.l., e594898629d452e8311796f5f329c7edbeda907c, Apache-2.0.\nBase: Fastino GLiNER2 privacy PII, 1cb4166094dc58fa8d836429f060d6c95f62b495, Apache-2.0.\nEncoder lineage: Microsoft mDeBERTa-v3-base, MIT.\nPinned artifact model card is retained in README.md.\nNative ONNX Runtime notices are retained alongside the shared runtime in mdoc/ocr/v1/licenses/onnx.\n").map_err(|e| e.to_string())?;
    verify_model(&root)
}

fn checkpoint(cancel: &AtomicBool, started: Instant) -> Result<(), String> {
    if cancel.load(Ordering::Relaxed) {
        return Err("Scan cancelled.".into());
    }
    if started.elapsed() > MAX_SCAN_TIME {
        return Err(
            "Scan exceeded two minutes. Review a smaller document or add selections manually."
                .into(),
        );
    }
    Ok(())
}

/// Full immutable Markdown source in, verified UTF-8 byte spans out. A scan
/// never writes a document or uses the network, including when setup is absent.
pub fn scan(source: &str, cancel: &AtomicBool) -> Result<Vec<Detection>, String> {
    if !SUPPORTED {
        return Err("Automatic scanning is unavailable on this platform.".into());
    }
    scan_in(
        source,
        cancel,
        &root()?,
        crate::ocr::onnx_runtime(false)
            .map_err(|_| "Set up the experimental model before scanning.".to_string())?,
    )
}

fn scan_in(
    source: &str,
    cancel: &AtomicBool,
    root: &Path,
    runtime: PathBuf,
) -> Result<Vec<Detection>, String> {
    if source.len() > MAX_SOURCE_BYTES {
        return Err(
            "Automatic scanning supports up to 2 MiB. Add selections manually in larger documents."
                .into(),
        );
    }
    let started = Instant::now();
    checkpoint(cancel, started)?;
    let _inference = INFERENCE
        .try_lock()
        .map_err(|_| "Another document is scanning. Retry when it finishes.".to_string())?;
    verify_model(root).map_err(|_| "Set up the experimental model before scanning.".to_string())?;
    checkpoint(cancel, started)?;
    ort::init_from(runtime)
        .map_err(|e| e.to_string())?
        .with_name("mdoc")
        .commit();
    let tasks: Vec<_> = FAMILIES
        .iter()
        .map(|family| {
            SchemaTask::Entities(family.iter().map(|(label, _)| (*label).into()).collect())
        })
        .collect();
    let transformer = SchemaTransformer::from_tokenizer_file(&root.join("fp16_v2/tokenizer.json"))
        .map_err(|e| e.to_string())?;
    let windows = bounded_windows(source, &transformer, &tasks, cancel, started)?;
    let mut engine = SpanEngine::new(
        SpanConfig::new(root)
            .with_precision(Precision::Fp16)
            .with_execution(ExecutionMode::Standard)
            .with_intra_threads(4),
    )
    .map_err(|e| e.to_string())?;
    let params = InferenceParams {
        threshold: 0.5,
        flat_ner: true,
        ..Default::default()
    };
    let mut detections = Vec::new();
    for range in windows {
        checkpoint(cancel, started)?;
        let text = &source[range.clone()];
        let output = engine
            .extract_with(text, &tasks, &params)
            .map_err(|e| e.to_string())?;
        checkpoint(cancel, started)?;
        for entity in output.entities {
            let Some(category) = FAMILIES
                .iter()
                .flat_map(|family| family.iter())
                .find_map(|(label, category)| (*label == entity.label).then_some(*category))
            else {
                continue;
            };
            if text.get(entity.char_start..entity.char_end) != Some(entity.text.as_str())
                || entity.char_start >= entity.char_end
            {
                return Err("Detector returned invalid source offsets; scan discarded.".into());
            }
            detections.push(Detection {
                range: range.start + entity.char_start..range.start + entity.char_end,
                category,
                score: entity.score,
            });
        }
    }
    checkpoint(cancel, started)?;
    Ok(detections)
}

fn bounded_windows(
    source: &str,
    transformer: &SchemaTransformer,
    tasks: &[SchemaTask],
    cancel: &AtomicBool,
    started: Instant,
) -> Result<Vec<std::ops::Range<usize>>, String> {
    let mut pending: Vec<_> = Chunker::new(128, 32)
        .map_err(|e| e.to_string())?
        .split(source)
        .map_err(|e| e.to_string())?
        .into_iter()
        .map(|chunk| chunk.byte_start..chunk.byte_end)
        .collect();
    let mut windows = Vec::new();
    while let Some(range) = pending.pop() {
        checkpoint(cancel, started)?;
        let record = transformer
            .transform(&source[range.clone()], tasks)
            .map_err(|e| e.to_string())?;
        if record.input_ids.len() <= MAX_INPUT_TOKENS {
            windows.push(range);
            continue;
        }
        // Subdivide using the model's own word splitter, and verify actual
        // schema+text tokens again. Reject a single pathological word rather
        // than truncate it or silently deliver partial detection coverage.
        let words = record.num_words();
        if words < 2 {
            return Err("A source word exceeds the model token limit. Add selections manually; no partial scan was accepted.".into());
        }
        let size = words.div_ceil(2);
        let overlap = (size / 4).min(size - 1);
        for chunk in Chunker::new(size, overlap)
            .map_err(|e| e.to_string())?
            .split(&source[range.clone()])
            .map_err(|e| e.to_string())?
        {
            pending.push(range.start + chunk.byte_start..range.start + chunk.byte_end);
        }
    }
    windows.sort_by_key(|range| range.start);
    Ok(windows)
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    #[ignore = "requires preverified qualification cache and installed native runtime; no downloads"]
    fn experimental_model_offline_scan() {
        let root =
            Path::new(env!("CARGO_MANIFEST_DIR")).join(".qualification/models/gliner2-pii-fp16");
        let source = "Alice Morgan represents Northbridge Legal Ltd. Анна Ёлкина подписала договор. Contact alice@example.invalid.\n".repeat(64);
        let started = Instant::now();
        let detected = scan_in(
            &source,
            &AtomicBool::new(false),
            &root,
            crate::ocr::onnx_runtime(false).unwrap(),
        )
        .unwrap();
        assert!(!detected.is_empty());
        assert!(
            detected
                .iter()
                .all(|detection| source.get(detection.range.clone()).is_some())
        );
        assert!(
            detected
                .iter()
                .any(|detection| detection.range.start > source.len() / 2)
        );
        eprintln!(
            "offline experimental scan: {} bytes, {} spans, {:.2}s",
            source.len(),
            detected.len(),
            started.elapsed().as_secs_f64()
        );
        let cancel = AtomicBool::new(false);
        let cancelled_at = Instant::now();
        std::thread::scope(|scope| {
            scope.spawn(|| {
                std::thread::sleep(Duration::from_secs(2));
                cancel.store(true, Ordering::Relaxed);
            });
            let result = scan_in(
                &source,
                &cancel,
                &root,
                crate::ocr::onnx_runtime(false).unwrap(),
            );
            assert_eq!(result.unwrap_err(), "Scan cancelled.");
        });
        eprintln!(
            "active scan cancellation returned after {:.2}s; no partial result accepted",
            cancelled_at.elapsed().as_secs_f64()
        );
        assert!(
            scan_in(
                "Alice",
                &AtomicBool::new(true),
                &root,
                crate::ocr::onnx_runtime(false).unwrap()
            )
            .is_err()
        );
    }
    #[test]
    fn pinned_artifacts_reject_partial_and_same_size_corruption() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("model");
        let artifact = Artifact {
            path: "model".into(),
            bytes: 3,
            sha256: Sha256::digest(b"abc")
                .iter()
                .map(|byte| format!("{byte:02x}"))
                .collect(),
            url: String::new(),
        };
        assert!(verify(&path, &artifact).is_err());
        fs::write(&path, b"ab").unwrap();
        assert!(verify(&path, &artifact).is_err());
        fs::write(&path, b"bad").unwrap();
        assert!(verify(&path, &artifact).is_err());
        fs::write(&path, b"abc").unwrap();
        verify(&path, &artifact).unwrap();
    }
    #[test]
    fn cancellation_and_deadline_reject_results() {
        assert!(checkpoint(&AtomicBool::new(true), Instant::now()).is_err());
        assert!(
            checkpoint(
                &AtomicBool::new(false),
                Instant::now() - Duration::from_secs(121)
            )
            .is_err()
        );
    }
}
