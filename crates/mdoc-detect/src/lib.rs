//! Experimental, offline GLiNER2 integration. Network access belongs only to
//! explicit setup. One bounded scan runs at a time; each scan drops its engine.
mod structured;
mod windows;
use gliner2_rs::{
    ExecutionMode, InferenceParams, SchemaTask, SpanConfig, SpanEngine,
    processor::SchemaTransformer,
};
use mdoc_pii::{Category, Detection};
use serde::Deserialize;

/// Why detection setup, its checks or a scan failed.
#[derive(Debug, thiserror::Error)]
pub enum DetectError {
    #[error(transparent)]
    Permit(#[from] mdoc_import::model_work::PermitError),
    #[error(transparent)]
    Download(#[from] mdoc_import::model_download::DownloadError),
    #[error(transparent)]
    Runtime(#[from] mdoc_import::ocr::SetupError),
    #[error("Could not find local application storage.")]
    NoStorage,
    #[error("Automatic scanning is unavailable on this platform. Manual review is available.")]
    SetupUnsupported,
    #[error("Automatic scanning is unavailable on this platform.")]
    ScanUnsupported,
    #[error("Detection threshold must be finite and between 0 and 1.")]
    InvalidThreshold,
    #[error("Set up the experimental model before scanning.")]
    NotSetUp,
    #[error(
        "Automatic scanning supports up to 2 MiB. Add selections manually in larger documents."
    )]
    TooLarge,
    #[error("Another document is scanning. Retry when it finishes.")]
    Busy,
    #[error("Scan cancelled.")]
    Cancelled,
    #[error("Scan exceeded two minutes. Review a smaller document or add selections manually.")]
    TooSlow,
    #[error(
        "A source word exceeds the model token limit. Add selections manually; no partial scan was accepted."
    )]
    WordTooLong,
    #[error("Detector returned invalid source offsets; scan discarded.")]
    InvalidOffsets,
    /// GLiNER2 or its tokenizer failed, in its words.
    #[error("{0}")]
    Engine(String),
    #[error("{0}")]
    Io(#[from] std::io::Error),
}
use windows::bounded_windows;

use std::{
    fs,
    path::{Path, PathBuf},
    sync::{
        Mutex,
        atomic::{AtomicBool, Ordering},
    },
    time::{Duration, Instant},
};

pub const SUPPORTED: bool = mdoc_import::ocr::SUPPORTED;
/// Company legal forms that mark a manual selection as an organization.
const LEGAL_FORMS: &[&str] = &[
    "ООО", "ОАО", "ЗАО", "ПАО", "АО", "НКО", "ИП", "LLC", "LLP", "Ltd", "Inc", "Corp", "PLC",
    "GmbH", "AG", "SA", "BV",
];

/// Guess a manual selection's category without a model, correctable in the
/// popup (ADR 0025): a structured rule covering exactly the selection
/// (evaluated on its line, so labels count), a date, a phone shape, a legal
/// form, a name shape (Person), else Other.
pub fn guess_category(source: &str, range: std::ops::Range<usize>) -> Category {
    let line_start = source[..range.start].rfind('\n').map_or(0, |i| i + 1);
    let line_end = source[range.end..]
        .find('\n')
        .map_or(source.len(), |i| range.end + i);
    let line = &source[line_start..line_end];
    let local = range.start - line_start..range.end - line_start;
    if let Some(detection) = structured::scan(line)
        .into_iter()
        .find(|d| d.range == local)
    {
        return detection.category;
    }
    let text = &source[range];
    if date_shaped(text) {
        return Category::Date;
    }
    let digits = text.chars().filter(char::is_ascii_digit).count();
    if digits >= 7
        && text
            .chars()
            .all(|c| c.is_ascii_digit() || " +()-\u{a0}".contains(c))
    {
        return Category::Phone;
    }
    if text.split(|c: char| !c.is_alphanumeric()).any(|word| {
        LEGAL_FORMS
            .iter()
            .any(|form| form.eq_ignore_ascii_case(word))
    }) {
        return Category::Organization;
    }
    if name_shaped(text) {
        Category::Person
    } else {
        Category::Other
    }
}
/// Numeric, month-name (RU/EN) and blank-fill (`«__» ____ 2026 г.`) dates.
/// Year-only and month-only values are not dates here.
fn date_shaped(text: &str) -> bool {
    static DATE: std::sync::OnceLock<regex::Regex> = std::sync::OnceLock::new();
    let date = DATE.get_or_init(|| {
        const RU: &str = "января|февраля|марта|апреля|мая|июня|июля|августа|сентября|октября|ноября|декабря";
        const EN: &str = "january|february|march|april|may|june|july|august|september|october|november|december";
        let day = r#"[«"]?(?:\d{1,2}|_+)[»"]?"#;
        let year = r"(?:\d{4}|_+)(?:\s*(?:г\.?|года))?";
        regex::Regex::new(&format!(
            r"(?i)^(?:\d{{1,2}}[./]\d{{1,2}}[./]\d{{2,4}}(?:\s*г\.?)?|\d{{4}}-\d{{2}}-\d{{2}}|{day}\s+(?:{RU}|{EN}|_+)\s+{year}|(?:{EN})\s+\d{{1,2}},?\s+\d{{4}})$"
        ))
        .expect("static date regex")
    });
    let normalized = text.replace("\\_", "_").replace('\u{a0}', " ");
    date.is_match(normalized.trim())
}
/// 1–4 words that each start with a capital letter; hyphenated surnames and
/// initials ("М.С.", "J.R.") count.
fn name_shaped(text: &str) -> bool {
    let words: Vec<_> = text.split_whitespace().collect();
    (1..=4).contains(&words.len())
        && words.iter().all(|word| {
            let initials = word
                .split_terminator('.')
                .all(|part| part.chars().count() == 1 && part.chars().all(char::is_uppercase))
                && word.ends_with('.');
            initials
                || word.split('-').all(|part| {
                    let mut chars = part.chars();
                    chars.next().is_some_and(char::is_uppercase)
                        && chars.all(|c| c.is_alphabetic() || c == '\'' || c == '’')
                })
        })
}
const MAX_SOURCE_BYTES: usize = 2 * 1024 * 1024;
const MAX_INPUT_TOKENS: usize = 512;
const MAX_SCAN_TIME: Duration = Duration::from_secs(120);
static INFERENCE: Mutex<()> = Mutex::new(());

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
pub struct Manifest {
    pub id: String,
    pub revision: String,
    pub repository: String,
    pub license: String,
    pub files: Vec<Artifact>,
}
#[derive(Deserialize)]
pub struct Artifact {
    pub path: String,
    pub bytes: u64,
    pub sha256: String,
    pub url: String,
}
pub fn manifest_for(model: mdoc_store::settings::PiiModel) -> Manifest {
    let source = match model {
        mdoc_store::settings::PiiModel::Fp16 => {
            include_str!("../../../resources/pseudonymization-model.json")
        }
        mdoc_store::settings::PiiModel::Fp32 => {
            include_str!("../../../resources/pseudonymization-fp32-model.json")
        }
    };
    serde_json::from_str(source).expect("pinned model manifest")
}
pub fn precision(model: mdoc_store::settings::PiiModel) -> gliner2_rs::Precision {
    match model {
        mdoc_store::settings::PiiModel::Fp16 => gliner2_rs::Precision::Fp16,
        mdoc_store::settings::PiiModel::Fp32 => gliner2_rs::Precision::Fp32,
    }
}
/// Attribution for exactly the files setup offers, including ONNX exports.
pub fn source_and_license(model: mdoc_store::settings::PiiModel) -> (String, String) {
    let manifest = manifest_for(model);
    (
        format!("Hugging Face · {}", manifest.repository),
        manifest.license,
    )
}
pub fn root_for(model: mdoc_store::settings::PiiModel) -> Result<PathBuf, DetectError> {
    mdoc_store::app_paths::data_local_dir()
        .map(|root| {
            root.join(format!(
                "mdoc/pseudonymization/{}-{}",
                manifest_for(model).id,
                &manifest_for(model).revision[..8]
            ))
        })
        .ok_or(DetectError::NoStorage)
}

/// Model files and the shared runtime, matching `check_reserved`.
pub fn model_files(
    root: &Path,
    ocr_root: &Path,
    model: mdoc_store::settings::PiiModel,
) -> Vec<PathBuf> {
    manifest_for(model)
        .files
        .iter()
        .map(|file| root.join(&file.path))
        .chain(std::iter::once(mdoc_import::ocr::onnx_runtime_path(
            ocr_root,
        )))
        .collect()
}
/// Bytes setup still needs for `model` under `root` (see [`root_for`]),
/// including the shared ONNX Runtime under `ocr_root` (`ocr::root`). Without
/// a root, everything is still to download.
pub fn pending(
    model: mdoc_store::settings::PiiModel,
    root: Option<&Path>,
    ocr_root: Option<&Path>,
) -> mdoc_import::model_download::Pending {
    let files = manifest_for(model).files;
    let missing: Vec<u64> = files
        .iter()
        .filter(|file| {
            root.is_none_or(|root| {
                mdoc_import::model_download::missing(&root.join(&file.path), file.bytes)
            })
        })
        .map(|file| file.bytes)
        .collect();
    mdoc_import::model_download::Pending {
        full: files.iter().map(|file| file.bytes).sum::<u64>()
            + mdoc_import::ocr::onnx_runtime_size(),
        model: missing.iter().sum(),
        runtime: mdoc_import::ocr::onnx_runtime_pending(ocr_root),
    }
}
fn verify_model(root: &Path, model: mdoc_store::settings::PiiModel) -> Result<(), DetectError> {
    for artifact in manifest_for(model).files {
        mdoc_import::model_download::verify(
            &root.join(&artifact.path),
            artifact.bytes,
            &artifact.sha256,
        )?;
    }
    Ok(())
}
pub fn setup_config(
    config: &mdoc_store::settings::PiiConfig,
    progress: &mdoc_import::model_download::Progress,
) -> Result<(), DetectError> {
    if !SUPPORTED {
        return Err(DetectError::SetupUnsupported);
    }
    let _permit =
        mdoc_import::model_work::Permit::acquire_for("installing pseudonymization model")?;
    setup_reserved(config, progress).map(|_| ())
}
/// The caller retains model admission through publishing its setup result.
/// Metadata is captured after downloads, immediately before final validation.
pub fn setup_reserved(
    config: &mdoc_store::settings::PiiConfig,
    progress: &mdoc_import::model_download::Progress,
) -> Result<mdoc_store::model_state::Files, DetectError> {
    if !SUPPORTED {
        return Err(DetectError::SetupUnsupported);
    }
    let root = root_for(config.model)?;
    fs::create_dir_all(&root)?;
    for artifact in manifest_for(config.model).files {
        mdoc_import::model_download::fetch(
            &artifact.url,
            artifact.bytes,
            &artifact.sha256,
            &root.join(&artifact.path),
            progress,
        )?;
    }
    progress.check()?;
    mdoc_import::ocr::onnx_runtime_with_progress(progress)?;
    let licenses = root.join("licenses");
    fs::create_dir_all(&licenses)?;
    for (name, text) in [
        (
            "Apache-2.0.txt",
            include_str!("../../../resources/gliner2-license.txt"),
        ),
        (
            "gliner2-NOTICE.txt",
            include_str!("../../../resources/gliner2-notice.txt"),
        ),
        (
            "Microsoft-MIT.txt",
            include_str!("../../../resources/pseudonymization-microsoft-license.txt"),
        ),
    ] {
        fs::write(licenses.join(name), text)?;
    }
    fs::write(
        licenses.join("MODEL-NOTICE.txt"),
        format!(
            "Experimental unmodified {}.\nExport: Jugaad s.r.l., e594898629d452e8311796f5f329c7edbeda907c, Apache-2.0.\nBase: Fastino GLiNER2 privacy PII, 1cb4166094dc58fa8d836429f060d6c95f62b495, Apache-2.0.\nEncoder: Microsoft mDeBERTa-v3-base, MIT.\nPinned model card retained in README.md. Shared ONNX notices: mdoc/ocr/v1/licenses/onnx.\n",
            config.model.name()
        ),
    )?;
    progress.phase("Checking runtime");
    progress.check()?;
    let before = mdoc_store::model_state::Files::read(&model_files(
        &root,
        &mdoc_import::ocr::root()?,
        config.model,
    ));
    check_engine(&root, config.model)?;
    progress.check()?;
    Ok(before)
}
fn check_engine(root: &Path, model: mdoc_store::settings::PiiModel) -> Result<(), DetectError> {
    check_engine_in(root, &mdoc_import::ocr::root()?, model)
}
fn check_engine_in(
    root: &Path,
    ocr_root: &Path,
    model: mdoc_store::settings::PiiModel,
) -> Result<(), DetectError> {
    verify_model(root, model)?;
    mdoc_import::ocr::load_onnx_runtime(&mdoc_import::ocr::onnx_runtime_in(ocr_root)?)?;
    let _engine = SpanEngine::new(
        SpanConfig::new(root)
            .with_precision(precision(model))
            .with_execution(ExecutionMode::Standard)
            .with_intra_threads(4),
    )
    .map_err(|e| DetectError::Engine(e.to_string()))?;
    Ok(())
}
pub fn check_reserved(model: mdoc_store::settings::PiiModel) -> Result<bool, DetectError> {
    if !SUPPORTED || cfg!(test) {
        return Ok(false);
    }
    check_reserved_in(&root_for(model)?, &mdoc_import::ocr::root()?, model)
}

/// The real reserved check with explicit roots; never reads the user's profile in tests.
pub fn check_reserved_in(
    root: &Path,
    ocr_root: &Path,
    model: mdoc_store::settings::PiiModel,
) -> Result<bool, DetectError> {
    match fs::metadata(root) {
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(false),
        Err(error) => return Err(error.into()),
        Ok(_) => {}
    }
    let result = check_engine_in(root, ocr_root, model);
    match result {
        Err(DetectError::Download(error)) if error.is_missing() => return Ok(false),
        Err(DetectError::Runtime(error)) if error.is_missing() => return Ok(false),
        other => other?,
    }
    Ok(true)
}
pub fn remove_model(model: mdoc_store::settings::PiiModel) -> Result<(), DetectError> {
    let _permit = mdoc_import::model_work::Permit::acquire_for("removing pseudonymization model")?;
    remove_reserved(model)
}
/// The caller retains model admission through remembering the removal.
pub fn remove_reserved(model: mdoc_store::settings::PiiModel) -> Result<(), DetectError> {
    let root = root_for(model)?;
    if root.exists() {
        fs::remove_dir_all(root)?;
    }
    Ok(())
}

fn checkpoint(cancel: &AtomicBool, started: Instant) -> Result<(), DetectError> {
    if cancel.load(Ordering::Relaxed) {
        return Err(DetectError::Cancelled);
    }
    if started.elapsed() > MAX_SCAN_TIME {
        return Err(DetectError::TooSlow);
    }
    Ok(())
}

/// Full immutable Markdown source in, verified UTF-8 byte spans out. A scan
/// never writes a document or uses the network, including when setup is absent.
pub fn scan_config(
    source: &str,
    cancel: &AtomicBool,
    config: &mdoc_store::settings::PiiConfig,
) -> Result<Vec<Detection>, DetectError> {
    let _permit = mdoc_import::model_work::Permit::acquire_for("pseudonymization scanning")?;
    scan_reserved(source, cancel, config)
}
pub fn scan_reserved(
    source: &str,
    cancel: &AtomicBool,
    config: &mdoc_store::settings::PiiConfig,
) -> Result<Vec<Detection>, DetectError> {
    if !mdoc_store::settings::unit_interval(config.threshold) {
        return Err(DetectError::InvalidThreshold);
    }
    if !SUPPORTED {
        return Err(DetectError::ScanUnsupported);
    }
    scan_in(
        source,
        cancel,
        &root_for(config.model)?,
        mdoc_import::ocr::onnx_runtime(false).map_err(|_| DetectError::NotSetUp)?,
        config,
    )
}

fn scan_in(
    source: &str,
    cancel: &AtomicBool,
    root: &Path,
    runtime: PathBuf,
    config: &mdoc_store::settings::PiiConfig,
) -> Result<Vec<Detection>, DetectError> {
    if source.len() > MAX_SOURCE_BYTES {
        return Err(DetectError::TooLarge);
    }
    let started = Instant::now();
    checkpoint(cancel, started)?;
    let _inference = INFERENCE.try_lock().map_err(|_| DetectError::Busy)?;
    verify_model(root, config.model).map_err(|_| DetectError::NotSetUp)?;
    checkpoint(cancel, started)?;
    mdoc_import::ocr::load_onnx_runtime(&runtime)?;
    let tasks: Vec<_> = FAMILIES
        .iter()
        .map(|family| {
            SchemaTask::Entities(family.iter().map(|(label, _)| (*label).into()).collect())
        })
        .collect();
    let transformer = SchemaTransformer::from_tokenizer_file(
        &root.join(config.model.folder()).join("tokenizer.json"),
    )
    .map_err(|e| DetectError::Engine(e.to_string()))?;
    let windows = bounded_windows(source, &transformer, &tasks, cancel, started)?;
    let mut engine = SpanEngine::new(
        SpanConfig::new(root)
            .with_precision(precision(config.model))
            .with_execution(ExecutionMode::Standard)
            .with_intra_threads(4),
    )
    .map_err(|e| DetectError::Engine(e.to_string()))?;
    let params = InferenceParams {
        threshold: config.threshold,
        flat_ner: true,
        ..Default::default()
    };
    let mut detections = Vec::new();
    for range in windows {
        checkpoint(cancel, started)?;
        let text = &source[range.clone()];
        let output = engine
            .extract_with(text, &tasks, &params)
            .map_err(|e| DetectError::Engine(e.to_string()))?;
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
                return Err(DetectError::InvalidOffsets);
            }
            detections.push(Detection {
                range: range.start + entity.char_start..range.start + entity.char_end,
                category,
                score: entity.score,
                recognizer: mdoc_pii::Recognizer::Model,
            });
        }
    }
    checkpoint(cancel, started)?;
    detections.extend(structured::scan(source));
    checkpoint(cancel, started)?;
    Ok(detections)
}

#[cfg(test)]
mod tests {
    use super::*;
    use sha2::{Digest, Sha256};
    #[test]
    fn manual_selections_guess_categories_without_a_model() {
        let guess = |source: &str, value: &str| {
            let start = source.find(value).unwrap();
            guess_category(source, start..start + value.len())
        };
        assert_eq!(
            guess("Write to anna@example.invalid.", "anna@example.invalid"),
            Category::Email
        );
        assert_eq!(guess("ИНН 500100732259", "500100732259"), Category::Tax);
        assert_eq!(
            guess("Call +7 (495) 123-45-67", "+7 (495) 123-45-67"),
            Category::Phone
        );
        assert_eq!(guess("Signed 12.03.2026", "12.03.2026"), Category::Date);
        assert_eq!(guess("On 2026-03-12", "2026-03-12"), Category::Date);
        assert_eq!(
            guess("От 12 марта 2026 г. до", "12 марта 2026 г."),
            Category::Date
        );
        assert_eq!(guess("On March 12, 2026", "March 12, 2026"), Category::Date);
        assert_eq!(
            guess(
                "Дата: «\\_\\__» __________ 2026 г.",
                "«\\_\\__» __________ 2026 г."
            ),
            Category::Date
        );
        assert_eq!(
            guess("В «12» марта 2026", "«12» марта 2026"),
            Category::Date
        );
        assert_eq!(guess("В 2026 г.", "2026 г."), Category::Other);
        assert_eq!(
            guess("Планируемая дата", "Планируемая дата"),
            Category::Other
        );
        assert_eq!(
            guess("Мария Петрова-Иванова", "Мария Петрова-Иванова"),
            Category::Person
        );
        assert_eq!(guess("J.R. Smith", "J.R. Smith"), Category::Person);
        assert_eq!(
            guess("ООО «Ромашка» agreed", "ООО «Ромашка»"),
            Category::Organization
        );
        assert_eq!(
            guess("Acme Ltd. agreed", "Acme Ltd."),
            Category::Organization
        );
        assert_eq!(
            guess("Павлова М.С. agreed", "Павлова М.С."),
            Category::Person
        );
    }
    fn verify(path: &Path, artifact: &Artifact) -> Result<(), String> {
        mdoc_import::model_download::verify(path, artifact.bytes, &artifact.sha256)
            .map_err(|e| e.to_string())
    }
    #[test]
    fn resource_guards_reject_before_opening_any_model_and_deadlines_reject_partial_work() {
        let source = "x".repeat(MAX_SOURCE_BYTES + 1);
        for model in mdoc_store::settings::PiiModel::ALL {
            let result = scan_in(
                &source,
                &AtomicBool::new(false),
                Path::new("missing-test-model"),
                PathBuf::from("missing-runtime"),
                &mdoc_store::settings::PiiConfig {
                    model,
                    ..Default::default()
                },
            );
            assert!(result.unwrap_err().to_string().contains("2 MiB"));
        }
        assert!(
            checkpoint(
                &AtomicBool::new(false),
                Instant::now() - Duration::from_secs(121)
            )
            .unwrap_err()
            .to_string()
            .contains("two minutes")
        );
        assert!(matches!(
            checkpoint(&AtomicBool::new(true), Instant::now()),
            Err(DetectError::Cancelled)
        ));
    }
    #[test]
    #[ignore = "requires preverified qualification cache and installed native runtime; no downloads"]
    fn experimental_model_offline_scan() {
        let root = Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../../.qualification/models/gliner2-pii-fp16");
        let source = "Alice Morgan represents Northbridge Legal Ltd. Анна Ёлкина подписала договор. Contact alice@example.invalid.\n".repeat(64);
        let started = Instant::now();
        let detected = scan_in(
            &source,
            &AtomicBool::new(false),
            &root,
            mdoc_import::ocr::onnx_runtime(false).unwrap(),
            &mdoc_store::settings::PiiConfig::default(),
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
                mdoc_import::ocr::onnx_runtime(false).unwrap(),
                &mdoc_store::settings::PiiConfig::default(),
            );
            assert!(matches!(result, Err(DetectError::Cancelled)));
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
                mdoc_import::ocr::onnx_runtime(false).unwrap(),
                &mdoc_store::settings::PiiConfig::default(),
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
    fn tracked_files_match_verified_artifacts_and_shared_runtime() {
        let dir = tempfile::tempdir().unwrap();
        let runtime = dir.path().join("ocr");
        for model in mdoc_store::settings::PiiModel::ALL {
            let paths = model_files(dir.path(), &runtime, model);
            let manifest = manifest_for(model);
            let expected = manifest
                .files
                .iter()
                .map(|a| dir.path().join(&a.path))
                .chain(std::iter::once(mdoc_import::ocr::onnx_runtime_path(
                    &runtime,
                )))
                .collect::<Vec<_>>();
            assert_eq!(paths, expected);
            assert_eq!(paths.len(), manifest.files.len() + 1);
            assert!(
                matches!(verify_model(dir.path(), model), Err(DetectError::Download(mdoc_import::model_download::DownloadError::Io(error))) if error.kind() == std::io::ErrorKind::NotFound)
            );
            assert!(!check_reserved_in(dir.path(), &runtime, model).unwrap());
            fs::create_dir_all(paths[0].parent().unwrap()).unwrap();
            fs::write(&paths[0], b"wrong size").unwrap();
            assert!(!check_reserved_in(dir.path(), &runtime, model).unwrap());
        }
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
