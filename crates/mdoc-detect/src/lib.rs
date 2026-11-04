//! Experimental, offline GLiNER2 integration. Network access belongs only to
//! explicit setup. One bounded scan runs at a time; each scan drops its engine.
pub mod lists;
pub mod recognition;
mod rules;
mod structured;
mod urls;
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
    ops::Range,
    path::{Path, PathBuf},
    sync::{
        Mutex,
        atomic::{AtomicBool, Ordering},
    },
    time::{Duration, Instant},
};

pub const SUPPORTED: bool = mdoc_import::ocr::SUPPORTED;

/// A name column's cell's score, as a `context` rule's value has (#397).
const COLUMN_SCORE: f32 = 0.95;

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
    if urls::whole(&source[range.clone()]) {
        return Category::Url;
    }
    rules::pack()
        .guess(&source[range])
        .unwrap_or(Category::Other)
}

/// Whether a model span has its category's shape in the rule pack; the model
/// also tags the words for things ("email", "phone number", "companies") and
/// titles and roles on their own ("Ms", "husband"; issue #302).
#[cfg(test)]
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
    let mut detections = structured::scan(source);
    detections.extend(urls::scan(source));
    Ok(detections)
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
    scan_at_with_progress(source, cancel, root, runtime, config, &|_| {})
}

/// [`scan_at`] that also reports the model windows done, as
/// [`scan_config_with_progress`] does: a phone passes them on to the
/// system, which stops a background job that shows no progress.
pub fn scan_at_with_progress(
    source: &str,
    cancel: &AtomicBool,
    root: &Path,
    runtime: &Path,
    config: &mdoc_store::settings::PiiConfig,
    progress: &dyn Fn(ScanProgress),
) -> Result<(Vec<Detection>, ScanTimings), DetectError> {
    if !mdoc_store::settings::unit_interval(config.threshold) {
        return Err(DetectError::InvalidThreshold);
    }
    let _permit = mdoc_import::model_work::Permit::acquire_for("pseudonymization scanning")?;
    scan_timed(source, cancel, root, runtime, config, progress)
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

/// `L.L.C.` spends six of the pinned export's eight model words per span.
/// Spaces in place of its periods leave room for the company name, keeping
/// the same UTF-8 byte offsets. The whole law-firm form `P.L.L.C.` reads the
/// same way, and so do `L.L.C` without its final dot and lower-case
/// `l.l.c.`; a form glued to a letter, digit or dot before it
/// ("N.L.L.C.") is left alone. Other initialisms, including legal references
/// and person initials, keep their punctuation. This is model input only:
/// it returns the ranges of the rewritten forms (without a final dot), and
/// `to_source` restores the original text.
fn model_text(text: &str) -> (std::borrow::Cow<'_, str>, Vec<Range<usize>>) {
    static DOTTED_LLC: std::sync::LazyLock<regex::Regex> = std::sync::LazyLock::new(|| {
        regex::Regex::new(r"(?i)\b(?:P\.)?L\.L\.C(?:\.|\b)").expect("constant legal-form pattern")
    });
    let mut normalized = None;
    let mut forms = Vec::new();
    for found in DOTTED_LLC.find_iter(text) {
        let glued = text[..found.start()]
            .chars()
            .next_back()
            .is_some_and(|before| before == '.' || before.is_alphanumeric());
        if glued {
            continue;
        }
        let bytes = normalized.get_or_insert_with(|| text.as_bytes().to_vec());
        for byte in &mut bytes[found.range()] {
            if *byte == b'.' {
                *byte = b' ';
            }
        }
        let end = found.end() - usize::from(found.as_str().ends_with('.'));
        forms.push(found.start()..end);
    }
    let input = match normalized {
        None => std::borrow::Cow::Borrowed(text),
        Some(bytes) => {
            std::borrow::Cow::Owned(String::from_utf8(bytes).expect("ASCII spaces preserve UTF-8"))
        }
    };
    (input, forms)
}

/// `raw`, found in the model input, in `source`'s offsets and original text.
/// A span that starts or ends inside a rewritten legal form (`forms`, from
/// [`model_text`]) takes the whole form: "Center L L" is "Center L.L.C",
/// never a broken "Center L.L"; the final dot stays outside.
fn to_source(
    source: &str,
    view: &mdoc_pii::view::View,
    forms: &[Range<usize>],
    mut raw: Vec<RawEntity>,
) -> Vec<RawEntity> {
    for entity in &mut raw {
        for form in forms {
            if form.start < entity.start && entity.start < form.end {
                entity.start = form.start;
            }
            if form.start < entity.end && entity.end < form.end {
                entity.end = form.end;
            }
        }
        let range = view.source_range(entity.start..entity.end);
        entity.text = source[range.clone()].to_owned();
        (entity.start, entity.end) = (range.start, range.end);
    }
    // A span only over a table header the view inserted names no source text.
    raw.retain(|entity| entity.start < entity.end);
    raw
}

/// The correction stage (ADR 0092): the model's raw entities through the
/// shape filter (`identifying`) and the rule pack's `span` fixes, then its
/// `context` rules and
/// the structured rules over the whole source, all reading it as the model
/// does, and web addresses as the source writes them (ADR 0094). Rules win the review's conflicts with the model. Pure: the same
/// source and entities give the same detections, in the order a scan
/// returns them.
pub fn correct(source: &str, raw: &[RawEntity]) -> Vec<Detection> {
    let view = mdoc_pii::view::View::new(source);
    let mut detections: Vec<Detection> = raw
        .iter()
        .filter_map(|entity| {
            let category = category_of(&entity.label)?;
            // Read as the model read it: "%41lice" in a link target is "Alice".
            let text = &view.text[view.view_range(entity.start..entity.end)];
            rules::pack()
                .identifying_at(source, entity.start..entity.end, text, category)
                .then_some(Detection {
                    range: entity.start..entity.end,
                    category,
                    score: entity.score,
                    recognizer: mdoc_pii::Recognizer::Model,
                })
        })
        .collect();
    // A cell under a name part's header, or a value after a name part's
    // label (#397): before the span fixes, so their filters judge it as
    // they judge the model's names.
    detections.extend(
        mdoc_pii::columns::name_cells(source)
            .into_iter()
            .chain(mdoc_pii::columns::labelled_names(source))
            .filter(|range| {
                let text = &view.text[view.view_range(range.clone())];
                rules::pack().identifying_at(source, range.clone(), text, Category::Person)
            })
            .map(|range| Detection {
                range,
                category: Category::Person,
                score: COLUMN_SCORE,
                recognizer: mdoc_pii::Recognizer::Context,
            }),
    );
    let precedents = rules::pack().fix_spans(source, &view, &mut detections);
    // After the span fixes, so each spelling copies the fixed wording.
    let respelt = respelt(source, &view, &detections);
    detections.extend(respelt);
    let rules = rules::pack()
        .context(&view.text)
        .into_iter()
        .chain(rules::pack().spans(&view.text))
        .chain(structured::scan(&view.text));
    let (spans, rules): (Vec<Detection>, Vec<Detection>) = rules
        .filter_map(|mut detection| {
            detection.range = view.source_range(detection.range);
            // One only over an inserted table header names nothing.
            (!detection.range.is_empty()).then_some(detection)
        })
        .partition(|detection| detection.recognizer == mdoc_pii::Recognizer::Span);
    // A precedent's docket number is part of its citation, no case number
    // of the case at hand (guide rule 8, #506 item 3): "Grant v. Finecy,
    // Case No. 4:20-cv-1234 (N.D. Tex. 2024)".
    let rules: Vec<Detection> = rules
        .into_iter()
        .filter(|rule| {
            rule.category != Category::CaseNumber
                || !precedents
                    .iter()
                    .any(|case| case.start <= rule.range.start && rule.range.end <= case.end)
        })
        .collect();
    // A web address inside a labelled case number is part of it (guide rule
    // 8), and a domain inside an email is the email's
    // ("ismay&#64;crane-pith.invalid"); any other address wins over what is
    // inside it, a labelled value included.
    let addresses: Vec<Detection> = urls::scan(source)
        .into_iter()
        .filter(|url| {
            !rules.iter().any(|rule| {
                let inside = rule.range.start <= url.range.start && url.range.end <= rule.range.end;
                inside && matches!(rule.category, Category::CaseNumber | Category::Email)
            })
        })
        .collect();
    detections.extend(rules);
    detections.extend(addresses);
    // A model span a match holds as its own part ("Harrow House" of an
    // address) is no evidence against it. It stays for the review, which
    // takes the match over it, or it where the review turns the match down.
    let absorbed = |detection: &Detection, span: &Detection| {
        detection.recognizer == mdoc_pii::Recognizer::Model
            && span.range.start <= detection.range.start
            && detection.range.end <= span.range.end
            && rules::pack().absorbs(
                &view.text,
                span.category,
                view.view_range(span.range.clone()),
                detection.category,
                view.view_range(detection.range.clone()),
            )
    };
    // Span matches stand only where they are the widest evidence.
    let spans: Vec<Detection> = spans
        .into_iter()
        .filter(|span| {
            stands(
                detections
                    .iter()
                    .filter(|detection| !absorbed(detection, span)),
                span,
            )
        })
        .collect();
    detections.extend(spans);
    detections
}

/// The model's wordings where the source spells them otherwise and the view
/// reads them the same: "Liora%20Nesket" in a link target, "Alice_Morgan"
/// in a file name, for a detected "Liora Nesket" (ADR 0092). Each is a
/// wording of its own, so Copy for AI replaces it as written.
fn respelt(source: &str, view: &mdoc_pii::view::View, model: &[Detection]) -> Vec<Detection> {
    if view.is_source() {
        return Vec::new();
    }
    // Each wording once per category: a wording the model gave two
    // categories gets both, and the review's conflict rules choose, as they
    // do where it is visible.
    let mut seen = std::collections::HashSet::new();
    let proposed: std::collections::HashSet<_> =
        model.iter().map(|d| (d.range.start, d.range.end)).collect();
    let mut found = Vec::new();
    for detection in model {
        let text = view.text[view.view_range(detection.range.clone())].to_owned();
        if !seen.insert((text.clone(), detection.category)) {
            continue;
        }
        for (at, _) in view.text.match_indices(&text) {
            let end = at + text.len();
            let word = |c: Option<char>| c.is_some_and(char::is_alphanumeric);
            if word(view.text[..at].chars().next_back()) || word(view.text[end..].chars().next()) {
                continue;
            }
            let range = view.source_range(at..end);
            if !range.is_empty()
                && source[range.clone()] != text
                && !proposed.contains(&(range.start, range.end))
            {
                found.push(Detection {
                    range,
                    ..*detection
                });
            }
        }
    }
    found
}

/// Whether a span pattern's match stands beside the other detections. Rules
/// win the review's conflicts, so the match gives way, and the others
/// compete as they would without it, wherever it is not the widest
/// evidence: a model span of its category that reaches past it (the model
/// found more of the address), a span of another category across one of
/// its edges ("John Smith" in "12 Oak Road, John Smith"), or another
/// rule's span anywhere in it (a valid email). A model span inside the
/// match ("Ленина" in "ул. Ленина, д. 5", "Larch Road" of a whole address)
/// loses to it.
fn stands<'a>(mut others: impl Iterator<Item = &'a Detection>, rule: &Detection) -> bool {
    let range = &rule.range;
    !others.any(|span| {
        let overlaps = span.range.start < range.end && range.start < span.range.end;
        let inside = range.start <= span.range.start && span.range.end <= range.end;
        let strictly_inside = range.start < span.range.start && span.range.end < range.end;
        overlaps
            && if span.recognizer != mdoc_pii::Recognizer::Model {
                true
            } else if span.category == rule.category {
                !inside
            } else {
                !strictly_inside
            }
    })
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
    let (input, forms) = model_text(&view.text);
    let windows = distinct_windows(&input, &transformer, &tasks, cancel, started)?;
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
    Ok((to_source(source, &view, &forms, raw), timings))
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
        let (input, forms) = model_text(&view.text);
        let windows = distinct_windows(&input, &self.transformer, &self.tasks, &cancel, started)?;
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
        Ok(to_source(source, &view, &forms, raw))
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
    let (input, _) = model_text(text);
    let text = input.as_ref();
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
    fn dotted_legal_forms_fit_the_model_without_changing_other_punctuation() {
        let source = "Pellam Company of Ohio L.L.C.; A. Rowan v. Penrow, Inc. 3.14";
        let (input, forms) = model_text(source);
        assert_eq!(
            input,
            "Pellam Company of Ohio L L C ; A. Rowan v. Penrow, Inc. 3.14"
        );
        assert_eq!(forms.len(), 1);
        assert_eq!(&source[forms[0].clone()], "L.L.C");
        let splitter = gliner2_rs::processor::WhitespaceTokenSplitter::new().unwrap();
        assert_eq!(
            splitter
                .split_with_offsets("Pellam Company of Ohio L.L.C.")
                .len(),
            10
        );
        assert_eq!(
            splitter
                .split_with_offsets("Pellam Company of Ohio L L C")
                .len(),
            7
        );
        let (input, forms) = model_text("A. Rowan v. Penrow, Inc. 3.14; U.S.C.; N.A.; R.C. Tile");
        assert!(matches!(input, std::borrow::Cow::Borrowed(_)));
        assert!(forms.is_empty());
        assert_eq!(
            model_text("United Federation of Glass, Kiln & Ceramic Workers").0,
            "United Federation of Glass, Kiln & Ceramic Workers"
        );
    }

    #[test]
    fn whole_variants_of_the_dotted_legal_form_are_normalized_and_glued_ones_left_alone() {
        for (source, input, forms) in [
            // The law-firm form reads whole, never as "P.L L C".
            (
                "Smith & Jones, P.L.L.C.",
                "Smith & Jones, P L L C ",
                &["P.L.L.C"][..],
            ),
            (
                "Ashmoor Center L.L.C, as landlord",
                "Ashmoor Center L L C, as landlord",
                &["L.L.C"],
            ),
            ("ashmoor center l.l.c.", "ashmoor center l l c ", &["l.l.c"]),
            (
                "Ashmoor Center L.L.C.; Pellam Company of Ohio L.L.C.",
                "Ashmoor Center L L C ; Pellam Company of Ohio L L C ",
                &["L.L.C", "L.L.C"],
            ),
            // Glued to a dot, letter or digit before it: another initialism.
            (
                "Rowan N.L.L.C. and X.P.L.L.C.",
                "Rowan N.L.L.C. and X.P.L.L.C.",
                &[],
            ),
            (
                "Rowan 3L.L.C. and AL.L.C.",
                "Rowan 3L.L.C. and AL.L.C.",
                &[],
            ),
            ("Rowan L.L.Cx", "Rowan L.L.Cx", &[]),
        ] {
            let (normalized, ranges) = model_text(source);
            assert_eq!(normalized, input, "{source}");
            assert_eq!(normalized.len(), source.len());
            let found: Vec<&str> = ranges.iter().map(|range| &source[range.clone()]).collect();
            assert_eq!(found, forms, "{source}");
        }
    }

    #[test]
    fn normalized_model_spans_restore_original_punctuation_and_source_offsets() {
        // (source, the spans the model proposes in its input, the source
        // text each becomes)
        for (source, proposed, expected) in [
            (
                "Орг: Pellam Company of Ohio L.L.C.;",
                &["Pellam Company of Ohio L L C"][..],
                &["Pellam Company of Ohio L.L.C"][..],
            ),
            (
                "Орг: Pellam&nbsp;Company of Ohio L.L.C.;",
                &["Pellam\u{a0}Company of Ohio L L C"],
                &["Pellam&nbsp;Company of Ohio L.L.C"],
            ),
            // A span ending inside the form takes all of it.
            (
                "Landlord: Taubman Creek Shopping Center L.L.C.;",
                &["Taubman Creek Shopping Center L L"],
                &["Taubman Creek Shopping Center L.L.C"],
            ),
            (
                "Counsel: Smith & Jones, P.L.L.C.",
                &["Smith & Jones, P L L"],
                &["Smith & Jones, P.L.L.C"],
            ),
            // So does a span starting inside it.
            ("Landlord: Ashmoor L.L.C. sued", &["L C"], &["L.L.C"]),
            // A period the Markdown view decodes.
            (
                "Landlord: Ashmoor Center L&#46;L.C.;",
                &["Ashmoor Center L L"],
                &["Ashmoor Center L&#46;L.C"],
            ),
            // Two forms in one window.
            (
                "Landlords: Ashmoor Center L.L.C.; Pellam Company of Ohio L.L.C.",
                &["Ashmoor Center L L", "Pellam Company of Ohio L L C"],
                &["Ashmoor Center L.L.C", "Pellam Company of Ohio L.L.C"],
            ),
        ] {
            let view = mdoc_pii::view::View::new(source);
            let (input, forms) = model_text(&view.text);
            assert_eq!(input.len(), view.text.len());
            let raw = proposed
                .iter()
                .map(|text| {
                    let start = input.find(text).unwrap();
                    RawEntity {
                        start,
                        end: start + text.len(),
                        label: "organization".into(),
                        score: 0.9,
                        text: (*text).into(),
                    }
                })
                .collect();
            let restored = to_source(source, &view, &forms, raw);
            let texts: Vec<&str> = restored.iter().map(|entity| entity.text.as_str()).collect();
            assert_eq!(texts, expected, "{source}");
            for entity in &restored {
                assert_eq!(&source[entity.start..entity.end], entity.text);
                // The final dot stays outside the span.
                assert_eq!(&source[entity.end..entity.end + 1], ".", "{source}");
            }
        }
    }

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
    /// #378, #478: through the whole correction stage, a company's inner
    /// name grows to its legal form before the public-body words could drop
    /// it, while courts, institutions and places go.
    #[test]
    fn correction_keeps_companies_and_drops_public_bodies_and_places() {
        let source = "Именем Российской Федерации Арбитражный суд Северной области                       рассмотрел иск ООО «Северная область Строй» к ООО «Судак»                       с участием ГБУ Северной области «Центр снабжения»; платёж                       прошёл через Банк «Город».";
        let raw: Vec<RawEntity> = [
            "Российской Федерации",
            "Северной области",
            "Северная область Строй",
            "Судак",
            "Центр снабжения",
            "Банк «Город»",
        ]
        .into_iter()
        .map(|text| {
            let start = source.find(text).unwrap();
            RawEntity {
                start,
                end: start + text.len(),
                label: "organization".into(),
                score: 0.9,
                text: text.into(),
            }
        })
        .collect();
        let found: Vec<_> = correct(source, &raw)
            .into_iter()
            .map(|d| (&source[d.range], d.category))
            .collect();
        assert_eq!(
            found,
            [
                ("ООО «Северная область Строй»", Category::Organization),
                ("ООО «Судак»", Category::Organization),
                ("Банк «Город»", Category::Organization),
            ]
        );
    }

    /// #378, #478 in English: through the whole correction stage, public
    /// bodies, laws, their defined abbreviations and places go, as
    /// organizations or people, while companies with a public or place word
    /// in their name stay whole.
    #[test]
    fn correction_drops_english_public_bodies_and_places_and_keeps_companies() {
        let source = "The Ashmoor County Probation Department, HMRC and the 9th Cir. applied \
                      the Gaming Act 2031 (“GA”) to Quillon Freight LLC of Boston. The State \
                      of Ashmoria and the Town of Brindle (Illinois) sued Georgia-Quillon \
                      Paper LLC, CMA CGM, Commission Junction and State Street Quillon \
                      Corp; GA binds them.";
        let raw: Vec<RawEntity> = [
            ("Ashmoor County Probation Department", "organization"),
            ("HMRC", "person"),
            ("9th Cir", "organization"),
            ("Gaming Act 2031", "person"),
            ("Quillon Freight LLC", "organization"),
            ("Boston", "organization"),
            ("State of Ashmoria", "organization"),
            ("Brindle", "organization"),
            ("Illinois", "organization"),
            ("Georgia-Quillon Paper LLC", "organization"),
            ("CMA CGM", "organization"),
            ("Commission Junction", "organization"),
            ("State Street Quillon Corp", "organization"),
            // The abbreviation where it is used, not where it is defined.
            ("GA", "organization"),
        ]
        .into_iter()
        .map(|(text, label)| {
            let start = source.rfind(text).unwrap();
            RawEntity {
                start,
                end: start + text.len(),
                label: label.into(),
                score: 0.9,
                text: text.into(),
            }
        })
        .collect();
        let found: Vec<_> = correct(source, &raw)
            .into_iter()
            .map(|d| (&source[d.range], d.category))
            .collect();
        assert_eq!(
            found,
            [
                ("Quillon Freight LLC", Category::Organization),
                ("Georgia-Quillon Paper LLC", Category::Organization),
                ("CMA CGM", Category::Organization),
                ("Commission Junction", Category::Organization),
                ("State Street Quillon Corp", Category::Organization),
            ]
        );
    }

    /// #397: the model reads an identifier column's cells with their
    /// header; a span over the header and the value maps to the cell, one
    /// only over the header goes, and a label rule finds the value.
    #[test]
    fn identifier_columns_map_back_to_their_cells() {
        let source = "| Фамилия | Паспорт | ИНН |\n| --- | --- | --- |\n| Кий | 46 82 731609 | 508614729031 |\n";
        let view = mdoc_pii::view::View::new(source);
        // The first `text` in the view from `from` on.
        let entity = |from: &str, text: &str, label: &str| {
            let start = view.text.find(from).unwrap();
            RawEntity {
                start,
                end: start + text.len(),
                label: label.into(),
                score: 0.9,
                text: text.into(),
            }
        };
        let raw = vec![
            entity("Кий", "Кий", "person"),
            entity("Паспорт: 4", "Паспорт: 46 82 731609", "passport"),
            entity("ИНН: 5", "ИНН", "tax"),
        ];
        let found: Vec<_> = to_source(source, &view, &[], raw)
            .into_iter()
            .map(|entity| (entity.text, entity.label))
            .collect();
        assert_eq!(
            found,
            [
                ("Кий".into(), "person".into()),
                ("46 82 731609".into(), "passport".into())
            ]
        );
        let found: Vec<_> = correct(source, &[])
            .into_iter()
            .map(|d| (&source[d.range], d.category))
            .collect();
        // The surname by its column (#397), the INN by its label's.
        assert_eq!(
            found,
            [("Кий", Category::Person), ("508614729031", Category::Tax)]
        );
    }

    /// ADR 0094: a web address reaches the review whole, after a field's
    /// colon or an attribute's "=", with its own "_", brackets and port, and
    /// wins over a labelled value inside it.
    #[test]
    fn a_web_address_reaches_the_review_whole() {
        for (source, address) in [
            (
                "[profile](https://private.invalid/staff%20ID:AB1234)",
                "https://private.invalid/staff%20ID:AB1234",
            ),
            (
                "Website:https://private.invalid/Alice",
                "https://private.invalid/Alice",
            ),
            (
                "<a href=https://private.invalid/Alice>profile</a>",
                "https://private.invalid/Alice",
            ),
            (
                "[file](https://share.invalid/token_)",
                "https://share.invalid/token_",
            ),
            (
                "See https://private.invalid/?ids[]=123 now.",
                "https://private.invalid/?ids[]=123",
            ),
            (
                "Visit site.invalid:8080/client/Alice today.",
                "site.invalid:8080/client/Alice",
            ),
            (
                "Profile https://site.invalid/u/alice_r today.",
                "https://site.invalid/u/alice_r",
            ),
            (
                "See https://site.invalid/~alice/page#top_section.",
                "https://site.invalid/~alice/page#top_section",
            ),
            (
                "Or www.site.invalid/u/alice_r.",
                "www.site.invalid/u/alice_r",
            ),
            (
                "Form: site.invalid/?email=alice@mail.invalid",
                "site.invalid/?email=alice@mail.invalid",
            ),
            (
                "_https://private.invalid/Alice_ wrote.",
                "https://private.invalid/Alice",
            ),
            (
                "__https://private.invalid/Alice__ wrote.",
                "https://private.invalid/Alice",
            ),
            (
                "Profile: <https://private.invalid/alice_r>.",
                "https://private.invalid/alice_r",
            ),
            (
                "- <https://private.invalid/u/alice_r>",
                "https://private.invalid/u/alice_r",
            ),
            (
                "Profile:\n<https://private.invalid/u/alice_r>",
                "https://private.invalid/u/alice_r",
            ),
            (
                "<https://private.invalid/u/alice_r>\n\nNext.",
                "https://private.invalid/u/alice_r",
            ),
            (
                "Old: ~~https://private.invalid/x~~ gone.",
                "https://private.invalid/x",
            ),
            (
                "See https://private.invalid/it's here.",
                "https://private.invalid/it's",
            ),
            (
                "[file](<https://private.invalid/My File.pdf>)",
                "https://private.invalid/My File.pdf",
            ),
            (
                "[file](https://share.invalid/receipt\\).pdf)",
                "https://share.invalid/receipt\\).pdf",
            ),
        ] {
            let mut review = mdoc_pii::Review::default();
            review.ingest(source, correct(source, &[])).unwrap();
            let proposed: Vec<_> = review
                .variants()
                .iter()
                .map(|v| (v.original.to_string(), v.category))
                .collect();
            assert_eq!(proposed, [(address.to_owned(), Category::Url)], "{source}");
            assert_eq!(review.candidates().len(), 1, "{source}");
        }
    }

    /// #401, #376: a detected name spelt otherwise in a link target is
    /// proposed there too, as written.
    #[test]
    fn a_detected_name_is_found_in_its_hidden_spellings() {
        let source = "Client: Liora Nesket. [a](https://b.invalid/Liora%20Nesket/x) ![s](Liora_Nesket.png) LioraNesket";
        let raw = [RawEntity {
            start: 8,
            end: 20,
            label: "person".into(),
            score: 0.9,
            text: "Liora Nesket".into(),
        }];
        // The web address around a spelling is proposed too, and wins in the
        // review (ADR 0094); the spellings stand for files and paths.
        let found: Vec<_> = correct(source, &raw)
            .into_iter()
            .filter(|d| d.category != Category::Url)
            .map(|d| &source[d.range])
            .collect();
        assert_eq!(found, ["Liora Nesket", "Liora%20Nesket", "Liora_Nesket"]);
    }

    /// A link target's spellings: a name only there is read decoded, an
    /// underscore before an encoded letter is a space too, and an email
    /// address keeps its underscores.
    #[test]
    fn link_targets_read_names_decoded_and_emails_whole() {
        let person = |source: &str, text: &str| {
            let start = source.find(text).unwrap();
            RawEntity {
                start,
                end: start + text.len(),
                label: "person".into(),
                score: 0.9,
                text: text.into(),
            }
        };
        let found = |source: &str, raw: &[RawEntity]| -> Vec<String> {
            correct(source, raw)
                .into_iter()
                .filter(|d| d.category != Category::Url)
                .map(|d| source[d.range].to_owned())
                .collect()
        };
        let source = "See [x](https://b.invalid/%41lice%20Morgan).";
        assert_eq!(
            found(source, &[person(source, "%41lice%20Morgan")]),
            ["%41lice%20Morgan"]
        );
        let source = "Client: Liora Nesket. [x](https://b.invalid/Liora_%4Eesket)";
        assert_eq!(
            found(source, &[person(source, "Liora Nesket")]),
            ["Liora Nesket", "Liora_%4Eesket"]
        );
        let source = "Write to [mail](mailto:alice_morgan@example.invalid?subject=a_b).";
        assert_eq!(found(source, &[]), ["alice_morgan@example.invalid"]);
        let source = "Write to [mail](mailto:alice_morgan&#64;example.invalid).";
        assert_eq!(found(source, &[]), ["alice_morgan&#64;example.invalid"]);
        let source = "Write to [mail](mailto:alice_morgan&commat;example.invalid).";
        assert_eq!(found(source, &[]), ["alice_morgan&commat;example.invalid"]);
        let source = "Write to [mail](mailto:alice_morgan＠example.invalid).";
        assert_eq!(found(source, &[]), ["alice_morgan＠example.invalid"]);
        // Spaces may come before a destination.
        let source = "Client: Liora Nesket. [x]( https://b.invalid/Liora%20Nesket)";
        assert_eq!(
            found(source, &[person(source, "Liora Nesket")]),
            ["Liora Nesket", "Liora%20Nesket"]
        );
    }

    /// Full-width digits read as ASCII, a referenced one too (#405).
    #[test]
    fn full_width_digits_read_as_ascii() {
        for source in [
            "ИНН ７７０７０８３８９３.",
            "ИНН &#xFF17;７０７０８３８９３.",
        ] {
            // The context rule and the structured INN rule both propose it.
            let mut found: Vec<_> = correct(source, &[])
                .into_iter()
                .map(|d| (&source[d.range], d.category))
                .collect();
            found.dedup();
            assert_eq!(
                found,
                [(&source[7..source.len() - 1], Category::Tax)],
                "{source}"
            );
        }
    }

    /// #448: a letter-spaced label, one in emphasis with a footnote and
    /// one with a note marker keep their value's category.
    #[test]
    fn spaced_and_footnoted_labels_keep_their_value() {
        for (source, value) in [
            ("И Н Н: 508641732966\n", "508641732966"),
            ("И Н Н¹: 508641732966", "508641732966"),
            ("**ИНН**[^1]: 508614372930\n\n[^1]: Note.", "508614372930"),
            ("__ИНН__[^1]: 508614372930", "508614372930"),
            ("**ИНН[^tax]:** 508614372930", "508614372930"),
            ("ИНН¹: 508614372930", "508614372930"),
            ("**ИНН**¹: 508614372930", "508614372930"),
        ] {
            let found: Vec<_> = correct(source, &[])
                .into_iter()
                .filter(|d| d.recognizer == mdoc_pii::Recognizer::Context)
                .map(|d| (&source[d.range], d.category))
                .collect();
            assert_eq!(found, [(value, Category::Tax)], "{source}");
        }
    }

    /// #397: a cell under a name part's header is a person without the
    /// model. The header establishes a name even for role-like surnames.
    #[test]
    fn name_columns_propose_their_cells() {
        let people = |source: &str| -> Vec<String> {
            correct(source, &[])
                .into_iter()
                .filter(|d| d.category == Category::Person)
                .map(|d| source[d.range].to_owned())
                .collect()
        };
        let source = "| Фамилия | Имя | Отчество | Паспорт |\n\
                      | --- | --- | --- | --- |\n\
                      | Кий | Ева | Оскаровна | 46 82 731609 |\n\
                      | Истец | Ответчик | Директор | 46 83 862710 |\n";
        assert_eq!(
            people(source),
            ["Кий", "Ева", "Оскаровна", "Истец", "Ответчик", "Директор"]
        );
        let source = "| Full name | Role |\n| --- | --- |\n| Claimant | Witness |\n";
        assert_eq!(people(source), ["Claimant"]);
        let source = "| Name | Type |\n| --- | --- |\n| Hearing Bundle | Bundle |\n";
        assert!(people(source).is_empty());
        for (source, expected) in [
            (
                "| First name | Surname |\n|---|---|\n| Edda | Trustee |\n| Ivo | Agent |",
                ["Edda", "Trustee", "Ivo", "Agent"],
            ),
            (
                "| Имя | Фамилия |\n|---|---|\n| Лада | Должник |\n| Иво | Руководитель |",
                ["Лада", "Должник", "Иво", "Руководитель"],
            ),
        ] {
            assert_eq!(people(source), expected, "{source}");
        }
        let source = "Given name: Alvora        Given name: Nerian\n\
                      Surname: McVellin        Surname: O’Kell–Venn\n";
        assert_eq!(
            people(source),
            ["Alvora", "Nerian", "McVellin", "O’Kell–Venn"]
        );
        assert!(people("Фамилия: Истец\n").is_empty());
    }

    /// A wording the model gave two categories is copied to its other
    /// spellings with both, whichever came first; the review chooses.
    #[test]
    fn a_respelling_keeps_every_category_the_model_gave() {
        let source = "Amber. [x](%41mber)";
        let raw = [("person", 0.6), ("organization", 0.9)].map(|(label, score)| RawEntity {
            start: 0,
            end: 5,
            label: label.into(),
            score,
            text: "Amber".into(),
        });
        let mut found: Vec<_> = correct(source, &raw)
            .into_iter()
            .filter(|d| &source[d.range.clone()] == "%41mber")
            .map(|d| (d.category, d.score))
            .collect();
        found.sort_by(|a, b| a.1.total_cmp(&b.1));
        assert_eq!(
            found,
            [(Category::Person, 0.6), (Category::Organization, 0.9)]
        );
    }
    /// #500 and #492's English reproductions: pronouns, "In", headings,
    /// religions, group nouns, nationalities and a place's second word are
    /// no one; a person named by such a word keeps it beside a title or
    /// their full name.
    #[test]
    fn correction_drops_common_words_and_keeps_people_named_so() {
        let source = "He appeals. In this case the Tribunal accepted his Christian faith, \
                      and Others agree. It relied on the Program Announcement. B. General \
                      Terms. The Maltese lawyer told all American drivers and United States \
                      Citizens. Mr Ossian Draycott lives in New York. Mr. General and Jane \
                      Federal met Dawn French; French signed.";
        let raw: Vec<RawEntity> = [
            ("He appeals", "He", "person"),
            ("In this", "In", "person"),
            ("Christian faith", "Christian", "person"),
            ("Others agree", "Others", "person"),
            ("It relied", "It", "organization"),
            ("Program Announcement", "Program", "person"),
            ("B. General", "B. General", "person"),
            ("Maltese lawyer", "Maltese lawyer", "person"),
            ("American drivers", "American", "person"),
            ("United States", "United States Citizens", "person"),
            ("Mr Ossian Draycott", "Ossian Draycott", "person"),
            ("New York", "York", "person"),
            ("Mr. General", "Mr. General", "person"),
            ("Jane", "Jane Federal", "person"),
            ("Dawn French", "Dawn French", "person"),
            ("French signed", "French", "person"),
        ]
        .into_iter()
        .map(|(anchor, text, label)| {
            let start = source.find(anchor).unwrap() + anchor.find(&text[..2]).unwrap();
            RawEntity {
                start,
                end: start + text.len(),
                label: label.into(),
                score: 0.9,
                text: text.into(),
            }
        })
        .collect();
        let found: Vec<_> = correct(source, &raw)
            .into_iter()
            .map(|d| (&source[d.range], d.category))
            .collect();
        assert_eq!(
            found,
            [
                // The partial model span is also an initial and surname;
                // preserve it under the ambiguous-name policy.
                ("B. General", Category::Person),
                ("Ossian Draycott", Category::Person),
                ("General", Category::Person),
                ("Jane Federal", Category::Person),
                ("Dawn French", Category::Person),
                ("French", Category::Person),
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
