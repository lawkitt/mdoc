//! Experimental, offline GLiNER2 integration. Network access belongs only to
//! explicit setup. One bounded scan runs at a time; each scan drops its engine.
pub mod recognition;
mod rules;
mod structured;
mod windows;
use gliner2_rs::{
    ExecutionMode, InferenceParams, SchemaTask, SpanConfig, SpanEngine,
    processor::SchemaTransformer,
};
use mdoc_pii::{Category, Detection};
use serde::{Deserialize, Serialize};

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

/// Guess a manual selection's category without a model, correctable in the
/// popup (ADR 0025): a structured rule covering exactly the selection
/// (evaluated on its line, so labels count), else the rule pack's guesses (a
/// date, a phone, a legal form, a name shape for Person), else Other.
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
    rules::pack()
        .guess(&source[range])
        .unwrap_or(Category::Other)
}

/// Whether a model span has its category's shape in the rule pack; the model
/// also tags the words for things ("email", "phone number", "companies") and
/// titles and roles on their own ("Ms", "husband"; issue #302).
fn identifying(text: &str, category: Category) -> bool {
    rules::pack().identifying(text, category)
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
/// The pinned export's id and revision, as its folder is named
/// ("gliner2-pii-fp16-e5948986"); recognition snapshots record it.
pub fn model_name(model: mdoc_store::settings::PiiModel) -> String {
    let manifest = manifest_for(model);
    format!("{}-{}", manifest.id, &manifest.revision[..8])
}
pub fn root_for(model: mdoc_store::settings::PiiModel) -> Result<PathBuf, DetectError> {
    mdoc_store::app_paths::data_local_dir()
        .map(|root| root.join("mdoc/pseudonymization").join(model_name(model)))
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
            .with_intra_threads(intra_threads()),
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
    scan_config_with_progress(source, cancel, config, &|_| {})
}

/// [`scan_config`] that also reports which model windows are done, so a long
/// document can show a count. `progress` runs on the scanning thread: once
/// with no window done when the windows are known, then after each window.
pub fn scan_config_with_progress(
    source: &str,
    cancel: &AtomicBool,
    config: &mdoc_store::settings::PiiConfig,
    progress: &dyn Fn(ScanProgress),
) -> Result<Vec<Detection>, DetectError> {
    let _permit = mdoc_import::model_work::Permit::acquire_for("pseudonymization scanning")?;
    scan_reserved_with(source, cancel, config, progress)
}

/// Model windows a scan has finished, out of all of the source's windows. A
/// window repeated word for word runs and counts once.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ScanProgress {
    pub done: usize,
    pub windows: usize,
}

pub fn scan_reserved(
    source: &str,
    cancel: &AtomicBool,
    config: &mdoc_store::settings::PiiConfig,
) -> Result<Vec<Detection>, DetectError> {
    scan_reserved_with(source, cancel, config, &|_| {})
}

fn scan_reserved_with(
    source: &str,
    cancel: &AtomicBool,
    config: &mdoc_store::settings::PiiConfig,
    progress: &dyn Fn(ScanProgress),
) -> Result<Vec<Detection>, DetectError> {
    if !mdoc_store::settings::unit_interval(config.threshold) {
        return Err(DetectError::InvalidThreshold);
    }
    if !SUPPORTED {
        return Err(DetectError::ScanUnsupported);
    }
    scan_timed(
        source,
        cancel,
        &root_for(config.model)?,
        &mdoc_import::ocr::onnx_runtime(false).map_err(|_| DetectError::NotSetUp)?,
        config,
        progress,
    )
    .map(|(detections, _)| detections)
}

#[cfg(test)]
fn scan_in(
    source: &str,
    cancel: &AtomicBool,
    root: &Path,
    runtime: PathBuf,
    config: &mdoc_store::settings::PiiConfig,
) -> Result<Vec<Detection>, DetectError> {
    scan_timed(source, cancel, root, &runtime, config, &|_| {}).map(|(detections, _)| detections)
}

/// Where one scan's time went. Only measurements read it (the mobile
/// feasibility spike, ADR 0085); scanning behaves the same without it.
#[derive(Clone, Debug, Default)]
pub struct ScanTimings {
    /// Checking the model files against their pinned digests.
    pub verify: Duration,
    /// Loading ONNX Runtime into ort (instant after the first time).
    pub runtime: Duration,
    pub tokenizer: Duration,
    /// Splitting the source into token-bounded windows.
    pub windowing: Duration,
    /// Building the GLiNER2 sessions, which every scan does (ADR 0002).
    pub engine_load: Duration,
    /// One entry per distinct model window, in source order.
    pub windows: Vec<WindowTiming>,
    /// The correction stage ([`correct`]): the shape filter and the
    /// structured rules over the whole source.
    pub structured: Duration,
    pub total: Duration,
}

#[derive(Clone, Debug)]
pub struct WindowTiming {
    pub bytes: usize,
    /// Schema and text tokens, at most 512.
    pub tokens: usize,
    pub inference: Duration,
}

/// The structured rules alone (emails, phones, IBAN, INN/SNILS and the
/// rest), with no model: the mobile UI tests seed candidates with it. No
/// device scans with the rules alone (ADR 0088). Full scans add these same
/// detections.
pub fn scan_rules(source: &str) -> Result<Vec<Detection>, DetectError> {
    if source.len() > MAX_SOURCE_BYTES {
        return Err(DetectError::TooLarge);
    }
    Ok(structured::scan(source))
}

/// [`scan_reserved`] on explicit model and runtime paths, with its timings.
/// The mobile core passes the runtime linked into the app
/// (`mdoc_import::ocr::linked_runtimes`). Takes the model work permit.
pub fn scan_at(
    source: &str,
    cancel: &AtomicBool,
    root: &Path,
    runtime: &Path,
    config: &mdoc_store::settings::PiiConfig,
) -> Result<(Vec<Detection>, ScanTimings), DetectError> {
    if !mdoc_store::settings::unit_interval(config.threshold) {
        return Err(DetectError::InvalidThreshold);
    }
    let _permit = mdoc_import::model_work::Permit::acquire_for("pseudonymization scanning")?;
    scan_timed(source, cancel, root, runtime, config, &|_| {})
}

fn scan_timed(
    source: &str,
    cancel: &AtomicBool,
    root: &Path,
    runtime: &Path,
    config: &mdoc_store::settings::PiiConfig,
    progress: &dyn Fn(ScanProgress),
) -> Result<(Vec<Detection>, ScanTimings), DetectError> {
    let started = Instant::now();
    let (raw, mut timings) = model_stage(source, cancel, started, root, runtime, config, progress)?;
    checkpoint(cancel, started)?;
    let lap = Instant::now();
    let detections = correct(source, &raw);
    timings.structured = lap.elapsed();
    checkpoint(cancel, started)?;
    timings.total = started.elapsed();
    Ok((detections, timings))
}

/// One span the model proposed, in document byte offsets, before any
/// correction (ADR 0092): its label from the schema, its score at the
/// configured threshold and its text. The model reads the source's
/// [`View`](mdoc_pii::view::View) (character references decoded); its
/// spans are mapped back to the source's offsets and text. Recognition
/// snapshots are these (ADR 0093).
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct RawEntity {
    pub start: usize,
    pub end: usize,
    pub label: String,
    pub score: f32,
    pub text: String,
}

/// `raw`, found in `view`, in `source`'s offsets and text.
fn to_source(source: &str, view: &mdoc_pii::view::View, mut raw: Vec<RawEntity>) -> Vec<RawEntity> {
    if !view.is_source() {
        for entity in &mut raw {
            let range = view.source_range(entity.start..entity.end);
            entity.text = source[range.clone()].to_owned();
            (entity.start, entity.end) = (range.start, range.end);
        }
    }
    raw
}

/// The correction stage (ADR 0092): the model's raw entities through the
/// shape filter (`identifying`), then the rule pack's `context` rules and
/// the structured rules over the whole source, all reading it as the model
/// does. Rules win the review's conflicts with the model. Pure: the same
/// source and entities give the same detections, in the order a scan
/// returns them.
pub fn correct(source: &str, raw: &[RawEntity]) -> Vec<Detection> {
    let view = mdoc_pii::view::View::new(source);
    let mut detections: Vec<Detection> = raw
        .iter()
        .filter_map(|entity| {
            let category = category_of(&entity.label)?;
            identifying(&mdoc_pii::view::decoded(&entity.text), category).then_some(Detection {
                range: entity.start..entity.end,
                category,
                score: entity.score,
                recognizer: mdoc_pii::Recognizer::Model,
            })
        })
        .collect();
    let rules = rules::pack()
        .context(&view.text)
        .into_iter()
        .chain(structured::scan(&view.text));
    detections.extend(rules.map(|mut detection| {
        detection.range = view.source_range(detection.range);
        detection
    }));
    detections
}

fn category_of(label: &str) -> Option<Category> {
    FAMILIES
        .iter()
        .flat_map(|family| family.iter())
        .find_map(|(name, category)| (*name == label).then_some(*category))
}

/// The model stage: windows, inference and offsets checked, with no
/// correction. `started` is the scan's start, for its deadline.
#[allow(clippy::too_many_arguments)]
fn model_stage(
    source: &str,
    cancel: &AtomicBool,
    started: Instant,
    root: &Path,
    runtime: &Path,
    config: &mdoc_store::settings::PiiConfig,
    progress: &dyn Fn(ScanProgress),
) -> Result<(Vec<RawEntity>, ScanTimings), DetectError> {
    if source.len() > MAX_SOURCE_BYTES {
        return Err(DetectError::TooLarge);
    }
    let mut timings = ScanTimings::default();
    let mut lap = Instant::now();
    let mut split = || {
        let now = Instant::now();
        let elapsed = now - lap;
        lap = now;
        elapsed
    };
    checkpoint(cancel, started)?;
    let _inference = INFERENCE.try_lock().map_err(|_| DetectError::Busy)?;
    split();
    verify_model(root, config.model).map_err(|_| DetectError::NotSetUp)?;
    timings.verify = split();
    checkpoint(cancel, started)?;
    mdoc_import::ocr::load_onnx_runtime(runtime)?;
    timings.runtime = split();
    let tasks = schema_tasks();
    let transformer = transformer(root, config.model)?;
    timings.tokenizer = split();
    let view = mdoc_pii::view::View::new(source);
    let windows = distinct_windows(&view.text, &transformer, &tasks, cancel, started)?;
    timings.windowing = split();
    let count = windows.len();
    progress(ScanProgress {
        done: 0,
        windows: count,
    });
    let mut engine = load_engine(root, config.model)?;
    timings.engine_load = split();
    let params = inference_params(config);
    let mut raw = Vec::new();
    for (index, (text, tokens, starts)) in windows.into_iter().enumerate() {
        checkpoint(cancel, started)?;
        split();
        run_window(&mut engine, text, &starts, &tasks, &params, &mut raw)?;
        timings.windows.push(WindowTiming {
            bytes: text.len(),
            tokens,
            inference: split(),
        });
        checkpoint(cancel, started)?;
        progress(ScanProgress {
            done: index + 1,
            windows: count,
        });
    }
    Ok((to_source(source, &view, raw), timings))
}

fn transformer(
    root: &Path,
    model: mdoc_store::settings::PiiModel,
) -> Result<SchemaTransformer, DetectError> {
    SchemaTransformer::from_tokenizer_file(&root.join(model.folder()).join("tokenizer.json"))
        .map_err(|e| DetectError::Engine(e.to_string()))
}

fn inference_params(config: &mdoc_store::settings::PiiConfig) -> InferenceParams {
    InferenceParams {
        threshold: config.threshold,
        flat_ner: true,
        ..Default::default()
    }
}

/// A distinct window's text, its token count and every start it occurs at.
type Window<'a> = (&'a str, usize, Vec<usize>);

/// The source's model windows, each distinct text once with its token count
/// and every start it occurs at.
fn distinct_windows<'a>(
    source: &'a str,
    transformer: &SchemaTransformer,
    tasks: &[SchemaTask],
    cancel: &AtomicBool,
    started: Instant,
) -> Result<Vec<Window<'a>>, DetectError> {
    // A window's proposals depend only on its text, so a repeated paragraph
    // runs once and its results land on every repeat.
    let mut windows: Vec<(&str, usize, Vec<usize>)> = Vec::new();
    let mut seen = std::collections::HashMap::new();
    for (range, tokens) in bounded_windows(source, transformer, tasks, cancel, started)? {
        let text = &source[range.clone()];
        let index = *seen.entry(text).or_insert_with(|| {
            windows.push((text, tokens, Vec::new()));
            windows.len() - 1
        });
        windows[index].2.push(range.start);
    }
    Ok(windows)
}

/// One window's entities with a known label, at each of its starts. Offsets
/// that don't round-trip to the entity's text reject the whole scan.
fn run_window(
    engine: &mut SpanEngine,
    text: &str,
    starts: &[usize],
    tasks: &[SchemaTask],
    params: &InferenceParams,
    raw: &mut Vec<RawEntity>,
) -> Result<(), DetectError> {
    let output = engine
        .extract_with(text, tasks, params)
        .map_err(|e| DetectError::Engine(e.to_string()))?;
    for entity in output.entities {
        if category_of(&entity.label).is_none() {
            continue;
        }
        if text.get(entity.char_start..entity.char_end) != Some(entity.text.as_str())
            || entity.char_start >= entity.char_end
        {
            return Err(DetectError::InvalidOffsets);
        }
        raw.extend(starts.iter().map(|start| RawEntity {
            start: start + entity.char_start,
            end: start + entity.char_end,
            label: entity.label.clone(),
            score: entity.score,
            text: entity.text.clone(),
        }));
    }
    Ok(())
}

/// The model stage loaded once for many sources: the recognition tool's
/// snapshots and the detection gate (ADR 0093). The app scans through
/// [`scan_config`], which builds its engine per scan (ADR 0002). Holds the
/// model work permit while it lives.
pub struct ModelSession {
    engine: SpanEngine,
    transformer: SchemaTransformer,
    tasks: Vec<SchemaTask>,
    params: InferenceParams,
    _inference: std::sync::MutexGuard<'static, ()>,
    _permit: mdoc_import::model_work::Permit,
}

impl ModelSession {
    /// Verifies the model under `root` and loads it with the ONNX Runtime
    /// library at `runtime`; never downloads.
    pub fn load(
        root: &Path,
        runtime: &Path,
        config: &mdoc_store::settings::PiiConfig,
    ) -> Result<Self, DetectError> {
        if !mdoc_store::settings::unit_interval(config.threshold) {
            return Err(DetectError::InvalidThreshold);
        }
        let permit = mdoc_import::model_work::Permit::acquire_for("pseudonymization scanning")?;
        let inference = INFERENCE.try_lock().map_err(|_| DetectError::Busy)?;
        verify_model(root, config.model).map_err(|_| DetectError::NotSetUp)?;
        mdoc_import::ocr::load_onnx_runtime(runtime)?;
        Ok(Self {
            transformer: transformer(root, config.model)?,
            engine: load_engine(root, config.model)?,
            tasks: schema_tasks(),
            params: inference_params(config),
            _inference: inference,
            _permit: permit,
        })
    }

    /// The model stage over one source, as a scan runs it, with the scan's
    /// size limit and deadline.
    pub fn entities(&mut self, source: &str) -> Result<Vec<RawEntity>, DetectError> {
        if source.len() > MAX_SOURCE_BYTES {
            return Err(DetectError::TooLarge);
        }
        let cancel = AtomicBool::new(false);
        let started = Instant::now();
        let view = mdoc_pii::view::View::new(source);
        let windows =
            distinct_windows(&view.text, &self.transformer, &self.tasks, &cancel, started)?;
        let mut raw = Vec::new();
        for (text, _, starts) in windows {
            checkpoint(&cancel, started)?;
            run_window(
                &mut self.engine,
                text,
                &starts,
                &self.tasks,
                &self.params,
                &mut raw,
            )?;
        }
        Ok(to_source(source, &view, raw))
    }
}

fn schema_tasks() -> Vec<SchemaTask> {
    FAMILIES
        .iter()
        .map(|family| {
            SchemaTask::Entities(family.iter().map(|(label, _)| (*label).into()).collect())
        })
        .collect()
}

/// ONNX Runtime's threads for the detector: 4, unless `MDOC_DETECT_THREADS`
/// names another count. The Android spike times other counts with it
/// (ADR 0089); nothing else sets it.
fn intra_threads() -> usize {
    std::env::var("MDOC_DETECT_THREADS")
        .ok()
        .and_then(|value| value.parse().ok())
        .filter(|&threads| threads > 0)
        .unwrap_or(4)
}

fn load_engine(
    root: &Path,
    model: mdoc_store::settings::PiiModel,
) -> Result<SpanEngine, DetectError> {
    SpanEngine::new(
        SpanConfig::new(root)
            .with_precision(precision(model))
            .with_execution(ExecutionMode::Standard)
            .with_intra_threads(intra_threads()),
    )
    .map_err(|e| DetectError::Engine(e.to_string()))
}

/// One full model window timed on its own: the longest whole-word prefix of
/// `text` that fits 512 schema+text tokens.
#[derive(Clone, Debug)]
pub struct WindowProbe {
    pub tokens: usize,
    pub bytes: usize,
    /// Loading ONNX Runtime into ort; the first call in a process pays it.
    pub runtime: Duration,
    /// Building the sessions: cold on the first engine in a process.
    pub engine_load: Duration,
    /// Each timed inference over the window.
    pub runs: Vec<Duration>,
}

/// Measures [`WindowProbe`] `runs` times on explicit paths, as [`scan_at`]
/// loads them. Takes the model work permit.
pub fn probe_window(
    text: &str,
    root: &Path,
    runtime: &Path,
    config: &mdoc_store::settings::PiiConfig,
    runs: usize,
) -> Result<WindowProbe, DetectError> {
    let _permit = mdoc_import::model_work::Permit::acquire_for("pseudonymization scanning")?;
    let _inference = INFERENCE.try_lock().map_err(|_| DetectError::Busy)?;
    verify_model(root, config.model).map_err(|_| DetectError::NotSetUp)?;
    let started = Instant::now();
    mdoc_import::ocr::load_onnx_runtime(runtime)?;
    let runtime_load = started.elapsed();
    let tasks = schema_tasks();
    let transformer = SchemaTransformer::from_tokenizer_file(
        &root.join(config.model.folder()).join("tokenizer.json"),
    )
    .map_err(|e| DetectError::Engine(e.to_string()))?;
    let tokens = |end: usize| -> Result<usize, DetectError> {
        transformer
            .transform(&text[..end], &tasks)
            .map(|record| record.input_ids.len())
            .map_err(|e| DetectError::Engine(e.to_string()))
    };
    // Word ends, then the last one whose prefix still fits.
    let ends: Vec<usize> = text
        .char_indices()
        .filter(|(_, c)| c.is_whitespace())
        .map(|(i, _)| i)
        .chain(std::iter::once(text.len()))
        .filter(|&end| end > 0)
        .collect();
    let (mut low, mut high) = (0, ends.len());
    while low < high {
        let mid = (low + high).div_ceil(2);
        if tokens(ends[mid - 1])? <= MAX_INPUT_TOKENS {
            low = mid;
        } else {
            high = mid - 1;
        }
    }
    let end = *ends
        .get(low.checked_sub(1).ok_or(DetectError::WordTooLong)?)
        .ok_or(DetectError::WordTooLong)?;
    let window = &text[..end];
    let count = tokens(end)?;
    let started = Instant::now();
    let mut engine = load_engine(root, config.model)?;
    let engine_load = started.elapsed();
    let params = InferenceParams {
        threshold: config.threshold,
        flat_ner: true,
        ..Default::default()
    };
    let mut timed = Vec::with_capacity(runs);
    for _ in 0..runs {
        let started = Instant::now();
        engine
            .extract_with(window, &tasks, &params)
            .map_err(|e| DetectError::Engine(e.to_string()))?;
        timed.push(started.elapsed());
    }
    Ok(WindowProbe {
        tokens: count,
        bytes: window.len(),
        runtime: runtime_load,
        engine_load,
        runs: timed,
    })
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
    /// #367: the correction stage reads character references decoded, and
    /// what it finds maps back to whole references in the source.
    #[test]
    fn correction_reads_character_references_decoded() {
        let source = "Email: ismay&#64;crane-pith.invalid. Dear Ism&#97;y O&#39;Verren,";
        let name = source.find("Ism&").unwrap()..source.len() - 1;
        let raw = [RawEntity {
            start: name.start,
            end: name.end,
            label: "person".into(),
            score: 0.9,
            text: source[name.clone()].into(),
        }];
        let found: Vec<_> = correct(source, &raw)
            .into_iter()
            .map(|d| (&source[d.range], d.category))
            .collect();
        assert_eq!(
            found,
            [
                ("Ism&#97;y O&#39;Verren", Category::Person),
                ("ismay&#64;crane-pith.invalid", Category::Email)
            ]
        );
    }
    #[test]
    fn titles_role_and_category_words_are_never_proposed() {
        let person = |text| identifying(text, Category::Person);
        let organization = |text| identifying(text, Category::Organization);
        for text in [
            "Ms",
            "Mr.",
            "adviser",
            "husband",
            "her husband",
            "Husband",
            "The Employer",
            "Parties",
            "г-жа",
            "Заказчику",
            "Исполнителя",
            "maya",
            "-",
            "People",
        ] {
            assert!(!person(text), "{text}");
        }
        for text in [
            "Ms Bennett",
            "Maya",
            "A. Morgan",
            "Daniel Bennett",
            "Анна Ёлкина",
            "Drew",
            "Sonia",
            "Peter Lord",
        ] {
            assert!(person(text), "{text}");
        }
        for text in [
            "Employer",
            "the Employer",
            "Company",
            "Сторона",
            "companies",
        ] {
            assert!(!organization(text), "{text}");
        }
        for text in [
            "Harrow & Finch Ltd",
            "Elm Row Legal LLP",
            "eBay",
            "ООО «Ромашка»",
        ] {
            assert!(organization(text), "{text}");
        }
        for (text, category, shaped) in [
            ("email", Category::Email, false),
            ("example.com", Category::Email, false),
            ("maya.bennett@example.invalid", Category::Email, true),
            ("phone number", Category::Phone, false),
            ("+1 (2O2) 555-Ol47", Category::Phone, true),
            ("addresses", Category::Address, false),
            ("Flat 3, 41 Larch Road", Category::Address, true),
            ("ул. Ленина", Category::Address, true),
            ("passport", Category::Identity, false),
            ("QQ 12 34 56 C", Category::Identity, true),
            ("ИНН", Category::Tax, false),
            ("account", Category::Bank, false),
            ("6001842/2026", Category::Bank, false),
            ("GB29 NWBK 6016 1331 9268 19", Category::Bank, true),
            ("husband", Category::Other, true),
        ] {
            assert_eq!(identifying(text, category), shaped, "{text}");
        }
    }
    /// Issue #302: a section scans the same alone and repeated 40 times.
    #[test]
    #[ignore = "requires preverified qualification cache and installed native runtime; no downloads"]
    fn a_long_document_proposes_what_its_sections_do() {
        let root = Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../../.qualification/models/gliner2-pii-fp16");
        let runtime = std::env::var_os("MDOC_ONNX_RUNTIME")
            .map(PathBuf::from)
            .unwrap_or_else(|| mdoc_import::ocr::onnx_runtime(false).unwrap());
        let scan = |source: &str| {
            let (detected, timings) = scan_timed(
                source,
                &AtomicBool::new(false),
                &root,
                &runtime,
                &mdoc_store::settings::PiiConfig::default(),
                &|_| {},
            )
            .unwrap();
            let found: std::collections::BTreeSet<_> = detected
                .iter()
                .map(|d| (source[d.range.clone()].to_owned(), d.category.token()))
                .collect();
            (found, detected.len(), timings)
        };
        let section = windows::tests::settlement_section();
        let (alone, alone_count, _) = scan(&section);
        let long = section.repeat(40);
        let (repeated, repeated_count, timings) = scan(&long);
        eprintln!(
            "{} bytes: {} model windows, {:.2}s",
            long.len(),
            timings.windows.len(),
            timings.total.as_secs_f64()
        );
        assert_eq!(repeated, alone);
        assert_eq!(repeated_count, 40 * alone_count);
        for (text, _) in &alone {
            assert!(
                !["Ms", "adviser", "husband"].contains(&text.as_str()),
                "{text}"
            );
        }
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
        let reports = std::cell::RefCell::new(Vec::new());
        let (detected, timings) = scan_timed(
            &source,
            &AtomicBool::new(false),
            &root,
            &mdoc_import::ocr::onnx_runtime(false).unwrap(),
            &mdoc_store::settings::PiiConfig::default(),
            &|p| reports.borrow_mut().push(p),
        )
        .unwrap();
        // The count starts at none done and steps once per window.
        let windows = timings.windows.len();
        assert!(windows > 1, "a long source spans several windows");
        let expected: Vec<_> = (0..=windows)
            .map(|done| ScanProgress { done, windows })
            .collect();
        assert_eq!(reports.into_inner(), expected);
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
