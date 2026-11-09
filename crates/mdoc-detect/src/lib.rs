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

/// The score of bare initials after a person's role, and of their repeats
/// (#491), as a `context` rule's value has.
const ROLE_INITIALS_SCORE: f32 = 0.95;

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

/// The table and column of a cell `range` lies in: the offset of its
/// table's first row and the number of unescaped pipes before it on its
/// line, or `None` outside a table row.
fn table_column(source: &str, range: &std::ops::Range<usize>) -> Option<(usize, usize)> {
    let line_start = source[..range.start].rfind('\n').map_or(0, |i| i + 1);
    let row = |line: &str| line.trim_start().starts_with('|');
    if !row(&source[line_start..]) {
        return None;
    }
    let before = &source[line_start..range.start];
    let column = before
        .match_indices('|')
        .filter(|(i, _)| {
            before[..*i]
                .bytes()
                .rev()
                .take_while(|&b| b == b'\\')
                .count()
                % 2
                == 0
        })
        .count();
    // Back over the rows above to the table's first.
    let mut table = line_start;
    while table > 0 {
        let above = source[..table - 1].rfind('\n').map_or(0, |i| i + 1);
        if !row(&source[above..table - 1]) {
            break;
        }
        table = above;
    }
    Some((table, column))
}

/// The model's organizations that are bare initials a column, a reference
/// line or an "initials" label places as a known person's
/// ([`mdoc_pii::placed_initials`]) are that person: "| ЯВР |" under
/// "Исполнитель" beside «Яков Викторович Ростов» (guide rule 1, #387).
/// Not where the document names an organization so: another of the model's
/// organizations holds the word («ООО «ЯВР»»), it is quoted, or the same
/// column holds a model organization that is no known person's initials
/// ("СМУ" beside "ПСК": a column of companies).
fn initials_are_people(source: &str, detections: &mut [Detection]) {
    let organizations = || {
        detections
            .iter()
            .filter(|d| d.category == Category::Organization)
    };
    if organizations().next().is_none() {
        return;
    }
    let people: Vec<&str> = detections
        .iter()
        .filter(|d| d.category == Category::Person)
        .map(|d| &source[d.range.clone()])
        .collect();
    let placed = mdoc_pii::placed_initials(source, &people);
    if placed.is_empty() {
        return;
    }
    let named: Vec<&str> = organizations()
        .filter(|d| !placed.contains(&d.range))
        .map(|d| &source[d.range.clone()])
        .collect();
    let company_columns: Vec<(usize, usize)> = organizations()
        .filter(|d| !placed.contains(&d.range))
        .filter_map(|d| table_column(source, &d.range))
        .collect();
    for detection in detections.iter_mut() {
        if detection.category != Category::Organization || !placed.contains(&detection.range) {
            continue;
        }
        let token = &source[detection.range.clone()];
        let organization = named.iter().any(|name| {
            name.split(|c: char| !c.is_alphanumeric())
                .any(|word| word == token)
        }) || quoted(source, token)
            || table_column(source, &detection.range)
                .is_some_and(|column| company_columns.contains(&column));
        if !organization {
            detection.category = Category::Person;
        }
    }
}

/// Whether `source` writes `word` in quotes, as a company's name is:
/// «ЯВР».
fn quoted(source: &str, word: &str) -> bool {
    ["«{}»", "\"{}\"", "“{}”", "„{}“", "‘{}’", "'{}'"]
        .iter()
        .any(|quoted| source.contains(&quoted.replace("{}", word)))
}

/// A role before bare initials (#491): where it starts, and whether only
/// a person holds it (the rule pack's `initials_after`: «потерпевший»,
/// «ИП») or a company may too (`initials_after_party`: «ответчик»).
#[derive(Clone, Copy, PartialEq)]
struct Cue {
    start: usize,
    person: bool,
}

/// How bare initials in Cyrillic capitals stand in a document (#491).
#[derive(Clone, Copy, PartialEq)]
enum Initials {
    /// On their own in a sentence; `cued` after a role («потерпевший
    /// КЛМ»).
    Lone { cued: Option<Cue> },
    /// As a company's name: after a legal form («АО БАВ», «ООО «БАВ»»),
    /// or before one, in brackets too («БАВ ООО», «КЛМ (ООО)»).
    Company,
    /// Part of a longer name: before a word they qualify, a capitalised
    /// one or other bare initials («КЛМ Групп», «РК БАВ»). The review
    /// would spread a person's letters to such a mention, so the rule adds
    /// none of them where one stands.
    Qualifying,
    /// Neither: in an all-capitals heading, or before a quote («КЛМ
    /// «Кедр»»).
    Other,
}

/// Every whole word of two or three Cyrillic capitals in `source` that is
/// bare initials ([`mdoc_pii::lone_initials`]: no abbreviation such as
/// «ООО», «РФ», «МВД»), no word that never names anyone (the rule pack's
/// «ТКО», «АЗС»), none that is never initials after a role (its
/// `initials_never` and public bodies' words: «ЖКХ», «НЕ», «МЧС») and no
/// mask of «Х»s («гр. ХХХ»), with how it stands. Letters before a word
/// they qualify, a capitalised one or other bare initials («гражданин РК
/// БАВ», «КЛМ Групп»), are part of a longer name. After a
/// citizen's word («гражданин», «гр.») only three letters are cued: two
/// are a country («гражданин РА»).
fn cyrillic_initials(source: &str) -> Vec<(Range<usize>, Initials)> {
    let pack = rules::pack();
    let mut found = Vec::new();
    let mut words = source.char_indices().peekable();
    while let Some((start, c)) = words.next() {
        if !c.is_alphanumeric() {
            continue;
        }
        let mut end = start + c.len_utf8();
        while let Some(&(at, next)) = words.peek() {
            if !next.is_alphanumeric() {
                break;
            }
            end = at + next.len_utf8();
            words.next();
        }
        let range = start..end;
        let word = &source[range.clone()];
        let capitals = word.chars().all(|c| matches!(c, 'А'..='Я' | 'Ё'));
        let mask = word.chars().all(|c| c == 'Х');
        if !capitals
            || mask
            || !mdoc_pii::lone_initials(source, &range)
            || pack.never(word)
            || pack.initials_never(word)
        {
            continue;
        }
        let line_start = source[..start].rfind('\n').map_or(0, |i| i + 1);
        let line_end = source[end..].find('\n').map_or(source.len(), |i| end + i);
        let before = &source[line_start..start];
        let after = source[end..line_end]
            .trim_start_matches(['*', '_'])
            .trim_start();
        let first_word = |text: &str| -> String {
            text.split(|c: char| !c.is_alphanumeric())
                .next()
                .unwrap_or_default()
                .to_owned()
        };
        let next_word = first_word(after);
        let bracketed = after.strip_prefix('(').map(first_word).unwrap_or_default();
        // The next word qualifies the letters where it is a capitalised
        // word («Колос», «России», «Групп») or other bare initials
        // («гражданин РК БАВ»); never an identifier's label («ИНН»), a
        // date mask («ДД.ММ.ГГГГ», «ХХ.ХХ») or «ИЗЪЯТО».
        // `after` runs to the line's end, trimmed at its start only.
        let next_start = line_end - after.len();
        let next_range = next_start..next_start + next_word.len();
        let qualifying = next_word.chars().next().is_some_and(char::is_uppercase)
            && (next_word.chars().any(char::is_lowercase)
                || mdoc_pii::lone_initials(source, &next_range)
                    && !next_word.chars().all(|c| c == 'Х')
                    && !pack.never(&next_word)
                    && !pack.initials_never(&next_word));
        // A heading in capitals has no lowercase letter and some longer
        // word in capitals besides: «ПОТЕРПЕВШИЙ КЛМ».
        let line = &source[line_start..line_end];
        let heading = !line.chars().any(char::is_lowercase)
            && line
                .split(|c: char| !c.is_alphanumeric())
                .any(|w| w.chars().count() > 3 && w.chars().all(char::is_uppercase));
        let standing = if pack.initials_company(before)
            || [&next_word, &bracketed]
                .iter()
                .any(|word| !word.is_empty() && pack.initials_company(word))
        {
            Initials::Company
        } else if qualifying {
            Initials::Qualifying
        } else if heading || after.starts_with(['«', '"', '„', '“']) {
            Initials::Other
        } else {
            Initials::Lone {
                cued: pack
                    .initials_role(before)
                    .filter(|role| !role.three || word.chars().count() == 3)
                    .map(|role| Cue {
                        start: line_start + role.start,
                        person: role.person,
                    }),
            }
        };
        found.push((range, standing));
    }
    found
}

/// Bare initials in a depersonalised act are a person (guide rule 1,
/// #491). After a role only a person has (the rule pack's
/// `initials_after`: «потерпевший КЛМ», «ИП ДРС», «гр. БАВ») the model's
/// organization exactly on the letters, or on the role and them («ИП
/// ДРС»), becomes the person, and where nothing marks them a span of
/// their own does; each lone repeat of the same letters is the person
/// too, the model's organization exactly on one relabelled, a span added
/// where nothing marks it. After a party's role, which a company may hold
/// too (`initials_after_party`: «ответчик КЛМ»), only the model's
/// organization exactly on the letters or a repeat becomes the person,
/// and only where the document gives the letters a person's evidence
/// elsewhere (a person's own role before them, or the model's person on
/// them: «Истец КЛМ … Потерпевший КЛМ»); no span is added. Where the document also writes the letters as a
/// company's (quoted, beside a legal form, inside another organization,
/// defined as one's short name: «ООО «ДРС»», «(далее – КСС)»), or as part
/// of a longer name («КЛМ Групп»), only the
/// model's organization on a mention after a person's own role becomes
/// the person and nothing is added, so the company keeps its name. A
/// mention another span overlaps («КЛМ Групп» as one organization) stays
/// as it is.
fn role_initials_are_people(source: &str, detections: &mut Vec<Detection>) {
    let found = cyrillic_initials(source);
    let cued = |word: &str, person_only: bool| {
        found.iter().any(|(range, standing)| {
            matches!(standing, Initials::Lone { cued: Some(cue) } if !person_only || cue.person)
                && &source[range.clone()] == word
        })
    };
    if !found
        .iter()
        .any(|(_, standing)| matches!(standing, Initials::Lone { cued: Some(_) }))
    {
        return;
    }
    // The lone initials a model organization at `range` is: exactly
    // them, or a role and them («ИП ДРС»).
    let initials_of = |range: &Range<usize>| {
        found.iter().find_map(|(lone, standing)| match standing {
            Initials::Lone { cued } if lone.end == range.end && lone.start >= range.start => {
                let whole =
                    lone.start == range.start || cued.is_some_and(|cue| cue.start <= range.start);
                whole.then(|| lone.clone())
            }
            _ => None,
        })
    };
    let named: Vec<&str> = detections
        .iter()
        .filter(|d| d.category == Category::Organization && initials_of(&d.range).is_none())
        .map(|d| &source[d.range.clone()])
        .collect();
    // The letters the model took for a person somewhere: a person's
    // evidence for a party's role, as a person's own role is.
    let model_people: Vec<&str> = detections
        .iter()
        .filter(|d| d.category == Category::Person && d.recognizer == mdoc_pii::Recognizer::Model)
        .map(|d| &source[d.range.clone()])
        .collect();
    let person = |word: &str| cued(word, true) || model_people.contains(&word);
    let pack = rules::pack();
    let company = |word: &str| {
        quoted(source, word)
            || pack.defined_company(source, word)
            || found.iter().any(|(range, standing)| {
                matches!(standing, Initials::Company | Initials::Qualifying)
                    && &source[range.clone()] == word
            })
            || named.iter().any(|name| {
                name.split(|c: char| !c.is_alphanumeric())
                    .any(|part| part == word)
            })
    };
    for (target, standing) in &found {
        let Initials::Lone { cued: cue } = *standing else {
            continue;
        };
        let word = &source[target.clone()];
        if !cued(word, false)
            || !pack.identifying_at(source, target.clone(), word, Category::Person)
        {
            continue;
        }
        let company = company(word);
        // What may change here: the model's organization, and a span of
        // the rule's own where nothing marks the letters.
        let (relabel, push) = match cue {
            Some(cue) if cue.person => (true, !company),
            Some(_) => (!company && person(word), false),
            None => (!company && person(word), !company && cued(word, true)),
        };
        if !relabel {
            continue;
        }
        // A span on exactly the letters, or after a person's own role on
        // the role and them; any other span over them leaves them be.
        let fits = |range: &Range<usize>| {
            *range == *target
                || cue.is_some_and(|cue| {
                    cue.person
                        && range.end == target.end
                        && cue.start <= range.start
                        && range.start <= target.start
                })
        };
        let overlapping =
            |range: &Range<usize>| range.start < target.end && target.start < range.end;
        if detections
            .iter()
            .any(|d| overlapping(&d.range) && !fits(&d.range))
        {
            continue;
        }
        for detection in detections.iter_mut() {
            if detection.category == Category::Organization && fits(&detection.range) {
                detection.range = target.clone();
                detection.category = Category::Person;
            }
        }
        let marked = detections
            .iter()
            .any(|d| d.category == Category::Person && overlapping(&d.range));
        if push && !marked {
            detections.push(Detection {
                range: target.clone(),
                category: Category::Person,
                score: ROLE_INITIALS_SCORE,
                recognizer: mdoc_pii::Recognizer::Context,
            });
        }
    }
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
    // Establish model company names before filtering reference-looking
    // short names. A trading name needs no legal form (guide rule 2).
    let organizations: Vec<&str> = raw
        .iter()
        .filter(|entity| category_of(&entity.label) == Some(Category::Organization))
        .filter_map(|entity| {
            let text = &view.text[view.view_range(entity.start..entity.end)];
            rules::pack()
                .identifying_at(
                    source,
                    entity.start..entity.end,
                    text,
                    Category::Organization,
                )
                .then_some(text)
        })
        .collect();
    let mut detections: Vec<Detection> = raw
        .iter()
        .filter_map(|entity| {
            let mut category = category_of(&entity.label)?;
            // Read as the model read it: "%41lice" in a link target is "Alice".
            let mut seen = view.view_range(entity.start..entity.end);
            let mut range = entity.start..entity.end;
            // A court's docket guessed as a tax, bank or identity value is
            // a case number (guide rule 5, #480): it stays hidden, and a
            // reference or a cited case around it still removes it.
            if let Some(case) = rules::pack().case_number(&view.text[seen.clone()], category) {
                category = Category::CaseNumber;
                seen = seen.start + case.start..seen.start + case.end;
                range = view.source_range(seen.clone());
            }
            // A company's name with a common noun after it: the model's
            // "Pellam airlines" is "⟦ORG|Pellam⟧ airlines" (guide rule 2,
            // #548), before the shape reads it as a heading's sentence case.
            if category == Category::Organization
                && let Some(end) = rules::pack().org_noun_name(&view.text[seen.clone()])
            {
                seen = seen.start..seen.start + end;
                range = view.source_range(seen.clone());
            }
            let text = &view.text[seen];
            rules::pack()
                .identifying_with_organizations(
                    source,
                    range.clone(),
                    text,
                    category,
                    &organizations,
                )
                .then_some(Detection {
                    range,
                    category,
                    score: entity.score,
                    recognizer: mdoc_pii::Recognizer::Model,
                })
        })
        .collect();
    initials_are_people(source, &mut detections);
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
    let names: Vec<Detection> = detections
        .iter()
        .filter(|d| matches!(d.category, Category::Person | Category::Organization))
        .map(|d| Detection {
            range: view.view_range(d.range.clone()),
            ..d.clone()
        })
        .collect();
    // Each span with the whole match a split one was cut from.
    let spans: Vec<(Detection, Option<Range<usize>>)> = rules::pack()
        .spans_beside(&view.text, &names)
        .into_iter()
        .filter_map(|found| {
            let mut detection = found.detection;
            detection.range = view.source_range(detection.range);
            // One only over an inserted table header names nothing.
            (!detection.range.is_empty())
                .then(|| (detection, found.whole.map(|whole| view.source_range(whole))))
        })
        .collect();
    let rules: Vec<Detection> = rules::pack()
        .context(&view.text)
        .into_iter()
        .chain(structured::scan(&view.text))
        .filter_map(|mut detection| {
            detection.range = view.source_range(detection.range);
            (!detection.range.is_empty()).then_some(detection)
        })
        .collect();
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
    // A case number repeated in another letter case is the same number
    // ("33А-2090/2026" in an act's title after "по делу № 33а-2090/2026",
    // issue #364); the review finds each wording's exact repeats.
    let rules: Vec<Detection> = case_repeats(
        source,
        &rules,
        &precedents,
        detections
            .iter()
            .chain(&rules)
            .chain(spans.iter().map(|(span, _)| span)),
    )
    .into_iter()
    .chain(rules)
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
    trim_at_birth_dates(source, &view, &mut detections, &rules);
    let rest = rest_beside_rule_values(
        source,
        &view,
        &detections,
        rules
            .iter()
            .chain(&addresses)
            .chain(spans.iter().map(|(span, _)| span)),
    );
    detections.extend(rest);
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
    // A model span inside the whole match a split span was cut from is no
    // evidence against it either: "KRENC<<LIDIIA" across a machine-readable
    // zone's filler holds two names, and the review turns it down for its
    // "<" (#495).
    let held = |detection: &Detection, whole: &Option<Range<usize>>| {
        detection.recognizer == mdoc_pii::Recognizer::Model
            && whole.as_ref().is_some_and(|whole| {
                whole.start <= detection.range.start && detection.range.end <= whole.end
            })
    };
    // Span matches stand only where they are the widest evidence.
    let spans: Vec<Detection> = spans
        .into_iter()
        .filter(|(span, whole)| {
            stands(
                detections
                    .iter()
                    .filter(|detection| !absorbed(detection, span) && !held(detection, whole)),
                span,
            )
        })
        .map(|(span, _)| span)
        .collect();
    detections.extend(spans);
    role_initials_are_people(source, &mut detections);
    detections
}

/// A model span running into a date of birth a label proposed keeps its
/// part on the side away from the label (#363): "Иванов Иван Иванович" of
/// "Иванов Иван Иванович, 01.01.1980" before "года рождения", "IVANOV,
/// Ivan" of "61 IVANOV, Ivan" after "DOB 3 Jun". The review takes the date
/// over any span it overlaps, which would drop the whole name. The part
/// stays when it is still of its category's shape; else, or when the label
/// side's part is all there is ("Иванов, дата рождения" of a span to the
/// date), the span stays as it was, for the review to drop.
fn trim_at_birth_dates(
    source: &str,
    view: &mdoc_pii::view::View,
    detections: &mut [Detection],
    rules: &[Detection],
) {
    let dates: Vec<(&Range<usize>, bool)> = rules
        .iter()
        .filter(|rule| rule.category == Category::Date)
        .filter_map(|rule| match rule.recognizer {
            mdoc_pii::Recognizer::Context => Some((&rule.range, false)),
            mdoc_pii::Recognizer::ContextAfter => Some((&rule.range, true)),
            _ => None,
        })
        .collect();
    // Spaces and the punctuation that joins a name to a date: "Иванов,
    // 01.01.1980", "ФИО2 (14 марта"; an initial's dot stays ("П. П.").
    let joint = |c: char| c.is_whitespace() || ",;:(–—-".contains(c);
    for detection in detections
        .iter_mut()
        .filter(|d| d.recognizer == mdoc_pii::Recognizer::Model && d.category != Category::Date)
    {
        let range = detection.range.clone();
        let Some((date, label_after)) = dates.iter().find(|(date, _)| {
            date.start < range.end
                && range.start < date.end
                && (range.start < date.start || date.end < range.end)
        }) else {
            continue;
        };
        let part = if *label_after {
            let before = &source[range.start..date.start.max(range.start)];
            range.start..range.start + before.trim_end_matches(joint).len()
        } else {
            let after = &source[date.end.min(range.end)..range.end];
            range.end - after.trim_start_matches(joint).len()..range.end
        };
        let text = &view.text[view.view_range(part.clone())];
        if !part.is_empty()
            && rules::pack().identifying_at(source, part.clone(), text, detection.category)
        {
            detection.range = part;
        }
    }
}

/// The values a model identifier span holds beside a rule's narrower value
/// in it (#739): "7701234567, р/с 40702810900000012345" after "ИНН", one
/// bank account span, holds the account. The review takes the rule's value
/// over the span it overlaps, which would show the account; each other
/// value becomes a span of its own, its label left outside (guide rule 6).
/// A value just after a field's label (`Pack::field_label`: "р/с", "КПП",
/// "тел.") takes the label's category; one with no word before it the
/// model's, where it has that category's strict shape (`identifier_shaped`);
/// one after any other word (an amount's "на сумму", "индекс", "заказ №")
/// gives nothing. The wide span stays, for the review to drop; where the
/// review turns the rule's value down, the wide span wins over these
/// again. `rules` are the non-model spans, one of an identifier category
/// inside the model span the trigger; they and every other detection the
/// model span overlaps (a lower-scored address) bound the values, so none
/// takes a part of another span.
fn rest_beside_rule_values<'a>(
    source: &str,
    view: &mdoc_pii::view::View,
    detections: &[Detection],
    rules: impl Iterator<Item = &'a Detection>,
) -> Vec<Detection> {
    let rules: Vec<&Detection> = rules.collect();
    let mut found: Vec<Detection> = Vec::new();
    for (index, model) in detections.iter().enumerate() {
        if model.recognizer != mdoc_pii::Recognizer::Model
            || !rules::identifier_field(model.category)
        {
            continue;
        }
        let range = &model.range;
        if !rules.iter().any(|rule| {
            rules::identifier_field(rule.category)
                && range.start <= rule.range.start
                && rule.range.end <= range.end
                && rule.range != *range
        }) {
            continue;
        }
        let mut taken: Vec<&Range<usize>> = rules
            .iter()
            .copied()
            .chain(
                detections
                    .iter()
                    .enumerate()
                    .filter(|(other, _)| *other != index)
                    .map(|(_, other)| other),
            )
            .map(|other| &other.range)
            .filter(|taken| taken.start < range.end && range.start < taken.end)
            .collect();
        taken.sort_by_key(|taken| taken.start);
        let mut gaps = Vec::new();
        let mut at = range.start;
        for taken in taken {
            if at < taken.start {
                gaps.push(at..taken.start);
            }
            at = at.max(taken.end);
        }
        if at < range.end {
            gaps.push(at..range.end);
        }
        for gap in gaps {
            for (value, label) in values_in(&view.text, view.view_range(gap)) {
                let (category, labelled, whole) = match &label {
                    None => (model.category, false, None),
                    Some(label) => match rules::pack().field_label(label) {
                        Some(rules::FieldLabel::Labelled(category, whole)) => {
                            (category, true, whole)
                        }
                        Some(rules::FieldLabel::Strict(category)) => (category, false, None),
                        None => continue,
                    },
                };
                // A phone ends before a year after it ("8 495 123-45-67
                // 2026 г.").
                let value = if category == Category::Phone {
                    value.start..value.start + phone_end(&view.text[value.clone()])
                } else {
                    value
                };
                let part = view.source_range(value.clone());
                let text = &view.text[value];
                if part.is_empty()
                    // A context's own value, its groups' spaces aside (an
                    // IBAN in fours).
                    || whole.is_some_and(|whole| {
                        !whole.is_match(text)
                            && !whole.is_match(&text.split_whitespace().collect::<String>())
                    })
                    || !identifier_shaped(text, category, labelled)
                    || !rules::pack().identifying_at(source, part.clone(), text, category)
                    || detections.iter().chain(&found).any(|d| d.range == part)
                {
                    continue;
                }
                found.push(Detection {
                    range: part,
                    category,
                    ..*model
                });
            }
        }
    }
    found
}

/// The values in `range` of `text`, each with the words just before it
/// (its label: "р/с", "паспорт серии", "Tel."), if any: runs of tokens
/// holding a digit, each line apart, ended by a word, a dotted or ISO date
/// ("01.02.2010", which is no part of a value) or a token's comma or
/// semicolon, and joined by punctuation alone ("№", "-"). A label's words
/// are those since the last value, date, comma or semicolon; a number
/// sign or "No." after them and a connector before them ("и", "and") are
/// left out. A run's edges lose their punctuation, but a phone's "+" and
/// parentheses stay.
fn values_in(text: &str, range: Range<usize>) -> Vec<(Range<usize>, Option<String>)> {
    type Run<'t> = (Range<usize>, Vec<&'t str>);
    static DATE: std::sync::OnceLock<regex::Regex> = std::sync::OnceLock::new();
    let date = DATE.get_or_init(|| {
        regex::Regex::new(r"^(?:\d{1,2}[./]\d{1,2}[./]\d{2,4}(?:г)?|\d{4}-\d{2}-\d{2})$")
            .expect("date token pattern")
    });
    static IBAN_START: std::sync::OnceLock<regex::Regex> = std::sync::OnceLock::new();
    let iban_start = IBAN_START
        .get_or_init(|| regex::Regex::new(r"^[A-Z]{2}\d{2}").expect("IBAN start pattern"));
    let edge = |c: char| ",;:.!?«»\"'“”„*_[]{}".contains(c);
    fn close<'t>(run: &mut Option<Run<'t>>, runs: &mut Vec<Run<'t>>) {
        if let Some(found) = run.take() {
            runs.push(found);
        }
    }
    let mut runs: Vec<Run<'_>> = Vec::new();
    let mut at = range.start;
    for line in text[range].split_inclusive('\n') {
        let mut words: Vec<&str> = Vec::new();
        let mut run: Option<Run<'_>> = None;
        for token in line.split(char::is_whitespace).filter(|t| !t.is_empty()) {
            let start = at + (token.as_ptr() as usize - line.as_ptr() as usize);
            let core = token.trim_matches(edge);
            if date.is_match(core) {
                close(&mut run, &mut runs);
                words.clear();
            } else if core.chars().any(char::is_numeric) {
                let end = start + token.len();
                run = Some(match run.take() {
                    Some((found, label)) => (found.start..end, label),
                    None => (start..end, std::mem::take(&mut words)),
                });
            } else if let Some((found, _)) = run.as_mut().filter(|(found, _)| {
                iban_start.is_match(
                    text[found.clone()].trim_start_matches(|c: char| !c.is_alphanumeric()),
                ) && core
                    .chars()
                    .all(|c| c.is_ascii_uppercase() || c.is_ascii_digit())
            }) {
                // An IBAN's letter group ("GB29 NWBK 6016 …").
                found.end = start + token.len();
            } else if core.chars().any(char::is_alphabetic) {
                if run.is_some() {
                    close(&mut run, &mut runs);
                    words.clear();
                }
                words.push(token);
            }
            if token.ends_with([',', ';']) {
                close(&mut run, &mut runs);
                words.clear();
            }
        }
        close(&mut run, &mut runs);
        at += line.len();
    }
    let connector = |word: &str| {
        ["и", "а", "and", "&", "or", "или"].contains(&rules::label_words(word).as_str())
    };
    let sign = |word: &str| ["no", "n", "nr", "nº"].contains(&rules::label_words(word).as_str());
    runs.into_iter()
        .filter_map(|(found, mut label)| {
            while label.last().is_some_and(|word| sign(word)) {
                label.pop();
            }
            let skip = label.iter().take_while(|word| connector(word)).count();
            let label = (skip < label.len()).then(|| label[skip..].join(" "));
            let value = &text[found.clone()];
            let lead = value.len()
                - value
                    .trim_start_matches(|c: char| !(c.is_alphanumeric() || "+(".contains(c)))
                    .len();
            let trail = value.len()
                - value
                    .trim_end_matches(|c: char| !(c.is_alphanumeric() || c == ')'))
                    .len();
            (found.start + lead < found.end - trail)
                .then(|| (found.start + lead..found.end - trail, label))
        })
        .collect()
}

/// Whether twenty digits are a Russian account's: its currency code, the
/// sixth to eighth digits, is the rouble's or a common currency's
/// ("40702810…"); a payment's УИН ("18810177…") or a budget code
/// ("18210102…") has none.
fn russian_account(value: &str) -> bool {
    let digits: String = value.chars().filter(char::is_ascii_digit).collect();
    ["810", "643", "840", "978", "156", "826", "756"].contains(&&digits[5..8])
}

/// How much of a phone run `value` is the phone: it ends once it has ten
/// or eleven digits and four digits alone come next (a year: "212 555
/// 0142 2026").
fn phone_end(value: &str) -> usize {
    let mut digits = 0;
    for token in value.split(char::is_whitespace).filter(|t| !t.is_empty()) {
        let at = token.as_ptr() as usize - value.as_ptr() as usize;
        if (10..=11).contains(&digits)
            && token.len() == 4
            && token.bytes().all(|b| b.is_ascii_digit())
        {
            return value[..at].trim_end().len();
        }
        digits += token.chars().filter(char::is_ascii_digit).count();
    }
    value.len()
}

/// Whether `value` reads as one value of an identifier `category`: digits
/// with spaces or dashes (#739). After its field's label (`labelled`) an
/// account has 8 to 34 digits, a document's number 6 to 20, a tax number 8
/// to 15 and a phone 7 to 15. With no label only the strict shapes count:
/// a Russian account's twenty digits (`russian_account`) or an IBAN, a
/// passport's series and number, a
/// tax number's ten or twelve digits, a phone with "+" or a bracketed code
/// or ten or eleven digits from 7 or 8 in groups. Thousands groups ("10 000
/// 000"), a date ("2026-03-12"), a span of years or a ZIP code ("2019-2026",
/// "62704-1234"), single digits apart ("1 0 1 1 0"), decimals and dotted
/// dates are none.
fn identifier_shaped(value: &str, category: Category, labelled: bool) -> bool {
    static NOISE: std::sync::OnceLock<regex::Regex> = std::sync::OnceLock::new();
    static IBAN: std::sync::OnceLock<regex::Regex> = std::sync::OnceLock::new();
    static PASSPORT: std::sync::OnceLock<regex::Regex> = std::sync::OnceLock::new();
    static CODE: std::sync::OnceLock<regex::Regex> = std::sync::OnceLock::new();
    let noise = NOISE.get_or_init(|| {
        regex::Regex::new(
            r"^(?:\d{1,3}(?:\s\d{3})+|\d{4}-\d{2}-\d{2}|\d{4,5}[-‑]\d{4}|\d(?:\s+\d)+)$",
        )
        .expect("noise pattern")
    });
    let iban = IBAN.get_or_init(|| {
        regex::Regex::new(r"^[A-Z]{2}\d{2}(?:\s?[A-Z0-9]){11,30}$").expect("IBAN pattern")
    });
    let passport = PASSPORT.get_or_init(|| {
        regex::Regex::new(r"^\d{2}\s?\d{2}\s?(?:№\s?)?\d{6}$").expect("passport pattern")
    });
    let code = CODE.get_or_init(|| {
        regex::Regex::new(r"^(?:\+\d|[78]?[\s-]*\(\d{2,5}\))").expect("phone code pattern")
    });
    let digits = value.chars().filter(char::is_ascii_digit).count();
    let letters = value.chars().filter(char::is_ascii_alphabetic).count();
    let made_of = |extra: &str| {
        value.chars().all(|c| {
            c.is_ascii_alphanumeric()
                || (c.is_whitespace() && c != '\n')
                || "-‑".contains(c)
                || extra.contains(c)
        })
    };
    if noise.is_match(value) {
        return false;
    }
    match category {
        Category::Phone => {
            let groups: Vec<&str> = value
                .split(|c: char| !c.is_ascii_digit())
                .filter(|group| !group.is_empty())
                .collect();
            letters == 0
                && made_of("+()")
                && (7..=15).contains(&digits)
                && (labelled
                    || code.is_match(value)
                    || (10..=11).contains(&digits)
                        && value.starts_with(['7', '8'])
                        && groups.len() >= 3
                        && groups.iter().all(|group| group.len() <= 4))
        }
        Category::Bank => {
            made_of("")
                && (iban.is_match(value)
                    || letters == 0
                        && (digits == 20 && (labelled || russian_account(value))
                            || labelled && (8..=34).contains(&digits)))
        }
        Category::Identity => {
            made_of("№")
                && (passport.is_match(value)
                    || labelled && letters <= 3 && (6..=20).contains(&digits))
        }
        Category::Tax => {
            letters == 0
                && made_of("")
                && (matches!(digits, 10 | 12) || labelled && (8..=15).contains(&digits))
        }
        _ => false,
    }
}

/// Each labelled case number's mentions in `source` in another letter case,
/// as whole values, outside a precedent's citation and outside any value
/// of another category in `others`.
fn case_repeats<'a>(
    source: &str,
    rules: &[Detection],
    precedents: &[Range<usize>],
    others: impl Iterator<Item = &'a Detection>,
) -> Vec<Detection> {
    // An email, an account, an address or a web address may hold the
    // number ("claims+cl-2026-000123@insurer.example", "account
    // gb-2031-4471-0098", "Suite 2a-1234, 12 Harbour Road"): it is theirs,
    // whoever found them.
    let others: Vec<Range<usize>> = others
        .filter(|other| other.category != Category::CaseNumber)
        .map(|other| other.range.clone())
        .chain(urls::scan(source).into_iter().map(|url| url.range))
        .collect();
    let mut seen = std::collections::HashSet::new();
    let mut found = Vec::new();
    for rule in rules {
        let text = &source[rule.range.clone()];
        if rule.category != Category::CaseNumber || !seen.insert(text.to_lowercase()) {
            continue;
        }
        let Ok(pattern) = regex::RegexBuilder::new(&regex::escape(text))
            .case_insensitive(true)
            .build()
        else {
            continue;
        };
        for mention in pattern.find_iter(source) {
            let word = |c: Option<char>| c.is_some_and(char::is_alphanumeric);
            // A separator and a letter or digit beyond it join a longer
            // value, as the review's whole values have it ("+cl-…@", "…-0098").
            let joined = |separator: Option<char>, next: Option<char>| {
                separator.is_some_and(|c| r"-/.:_\@+".contains(c)) && word(next)
            };
            let mut back = source[..mention.start()].chars().rev();
            let mut ahead = source[mention.end()..].chars();
            let (b1, b2) = (back.next(), back.next());
            let (a1, a2) = (ahead.next(), ahead.next());
            if mention.as_str() == text
                || word(b1)
                || word(a1)
                || joined(b1, b2)
                || joined(a1, a2)
                // A value of another category on exactly the repeat's span
                // is the number itself, which the model read as TAX or
                // the like: the repeat makes it CASE.
                || others.iter().any(|other| {
                    other.start < mention.end()
                        && mention.start() < other.end
                        && (other.start, other.end) != (mention.start(), mention.end())
                })
                || precedents
                    .iter()
                    .any(|case| case.start <= mention.start() && mention.end() <= case.end)
            {
                continue;
            }
            found.push(Detection {
                range: mention.range(),
                ..rule.clone()
            });
        }
    }
    found
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
        // #601: a name part's label establishes a role-like surname too when
        // a neighbouring name part's label holds a name.
        for (source, expected) in [
            ("Given name: Edda\nSurname: Trustee\n", ["Edda", "Trustee"]),
            ("Given name: Edda  Surname: Trustee\n", ["Edda", "Trustee"]),
            ("Имя: Лада\nФамилия: Должник\n", ["Лада", "Должник"]),
        ] {
            assert_eq!(people(source), expected, "{source}");
        }
    }

    /// #601: a role word after a lone name part's label is a form's
    /// placeholder, and its other mentions are no names either.
    #[test]
    fn labelled_role_placeholders_stay_roles() {
        for source in [
            "Full name: Claimant\n\nThe Claimant seeks damages from the Defendant.\n",
            "CLAIM FORM\n\nFull name: Claimant\nAddress: ____\n\n\
             Full name: Defendant\nAddress: ____\n\n\
             1. The Claimant claims from the Defendant the sum due.\n",
            "Full name: Claimant\nFull name: Defendant\n",
            "Full name: Director\n\nThe Director signed.\n",
            "Фамилия: Истец\n\nИстец просит взыскать долг с ответчика.\n",
            "ЗАЯВЛЕНИЕ\n\nФамилия: Заявитель\nИмя: ________\n\n\
             Заявитель просит выдать справку.\n",
            "Full name: The Claimant\n",
            "Full name: Applicant Name\nDate: Date\n",
        ] {
            let proposed = recognition::proposals(source, &[]).expect("proposals");
            assert!(proposed.is_empty(), "{source}: {proposed:?}");
        }
    }

    /// #736: a labelled value with the punctuation that ends it, and one
    /// after «ФИО», is a person; a lone role after «ФИО» stays a form's
    /// placeholder (#601), and a label in prose or before a filler gives none.
    #[test]
    fn punctuated_and_fio_labelled_names_are_people() {
        let people = |source: &str| -> Vec<String> {
            correct(source, &[])
                .into_iter()
                .filter(|d| d.category == Category::Person)
                .map(|d| source[d.range].to_owned())
                .collect()
        };
        for (source, expected) in [
            (
                "Surname: Trustee, Given name: Edda.\nФ.И.О.: Ветлин Нил Олегович.\n",
                &["Trustee", "Edda", "Ветлин Нил Олегович"][..],
            ),
            ("Surname: McVellin.\n", &["McVellin"]),
            (
                "Given name: Edda.\nSurname: Trustee.\n",
                &["Edda", "Trustee"],
            ),
            ("ФИО: Ветлин Нил Олегович\n", &["Ветлин Нил Олегович"]),
            ("Ф. И. О.: Ветлин Нил Олегович;\n", &["Ветлин Нил Олегович"]),
            ("Имя: Лада;\nФамилия: Должник.\n", &["Лада", "Должник"]),
            // Two full names are two people: one backs no role beside it.
            (
                "Ф.И.О.: Ветлин Нил Олегович\nФИО: Истец\n",
                &["Ветлин Нил Олегович"],
            ),
            (
                "Full name: Edda Brannock\nFull name: Claimant\n",
                &["Edda Brannock"],
            ),
            // A full name and a name part back each other, as in #601.
            (
                "Full name: Edda Brannock\nSurname: Trustee\n",
                &["Edda Brannock", "Trustee"],
            ),
            (
                "Full name: Edda Brannock    Surname: Trustee\n",
                &["Edda Brannock", "Trustee"],
            ),
            ("Full name: Edda\nSurname: Trustee\n", &["Edda", "Trustee"]),
        ] {
            assert_eq!(people(source), expected, "{source}");
        }
        for source in [
            "ФИО: Истец\n",
            "Ф.И.О.: Истец.\n\nИстец просит взыскать долг с ответчика.\n",
            "ФИО: Истец\nФИО: Ответчик\n",
            "Surname: Trustee.\n",
            "Full name: Claimant.\n\nThe Claimant seeks damages.\n",
            "В графе ФИО: указать фамилию, имя и отчество полностью.\n",
            "In this form the surname: is required.\n",
            "ФИО: не указано.\n",
            "ФИО: ________\n",
            "Surname: N/A.\n",
            "`ФИО: Ветлин Нил Олегович`\n",
            // A form's other fields after a blank label.
            "ФИО: Подпись\n",
            "ФИО: Дата\n",
            "ФИО: Должность\n",
            "ФИО: Полностью\n",
            "ФИО: Прочерк\n",
            "ФИО: Расшифровка\n",
            "Full name: Signature.\n",
            "Full name: Sign\n",
            "Full name: Print Name.\n",
            "Full name: Date.\n",
            "Full name: Title\n",
            "Full name: Position.\n",
        ] {
            let proposed = recognition::proposals(source, &[]).expect("proposals");
            assert!(proposed.is_empty(), "{source}: {proposed:?}");
        }
        // The dot of a last "Jr." or "Sr." is the name's own: no labelled
        // value cuts it, and the model's span stays as it read it.
        let model = |source: &str, text: &str| -> Vec<String> {
            let start = source.find(text).unwrap();
            let raw = [RawEntity {
                start,
                end: start + text.len(),
                label: "person".into(),
                score: 0.9,
                text: text.into(),
            }];
            correct(source, &raw)
                .into_iter()
                .filter(|d| d.category == Category::Person)
                .map(|d| source[d.range].to_owned())
                .collect()
        };
        assert!(people("Full name: Orvel Lanmark Jr.\n").is_empty());
        assert_eq!(
            model("Full name: Orvel Lanmark Jr.\n", "Orvel Lanmark Jr."),
            ["Orvel Lanmark Jr."]
        );
        assert_eq!(
            model("Full name: Orvel Lanmark Sr.\n", "Orvel Lanmark"),
            ["Orvel Lanmark"]
        );
    }

    /// The model's `spans` (each its first occurrence) in `source`.
    fn model_spans(source: &str, spans: &[(&str, &str)]) -> Vec<RawEntity> {
        spans
            .iter()
            .map(|(text, label)| {
                let start = source.find(text).unwrap();
                RawEntity {
                    start,
                    end: start + text.len(),
                    label: (*label).into(),
                    score: 0.9,
                    text: (*text).into(),
                }
            })
            .collect()
    }

    /// What the user is shown for `source` with the model's `spans`, as
    /// (start, category, text) in source order.
    fn shown_with(source: &str, spans: &[(&str, &str)]) -> Vec<(usize, String, String)> {
        recognition::proposals(source, &model_spans(source, spans))
            .unwrap()
            .into_iter()
            .map(|span| {
                (
                    span.range.start,
                    span.category,
                    source[span.range].to_owned(),
                )
            })
            .collect()
    }

    /// #739: a model identifier span from a labelled value into the next
    /// field keeps that field's value, of the model's category and without
    /// its label, where a rule's narrower value in it wins: each row of
    /// the issue. Where no rule marks the first value (an ОКПО or an
    /// invalid ОГРН before #563's rules), the wide span stays as it was; the
    /// rule's value keeps the category the rule gives it alone.
    #[test]
    fn a_wide_identifier_span_keeps_the_field_beside_a_rule_value() {
        for (source, wide, label, rest) in [
            (
                "ИНН 7701234567, р/с 40702810900000012345",
                "7701234567, р/с 40702810900000012345",
                "bank_account",
                &[("BANK", "40702810900000012345")][..],
            ),
            (
                "ИНН 772012345678, паспорт 4512 345678",
                "772012345678, паспорт 4512 345678",
                "passport_number",
                &[("IDENTITY", "4512 345678")],
            ),
            (
                "ОГРН 1027700132195, р/с 40702810900000012345",
                "1027700132195, р/с 40702810900000012345",
                "bank_account",
                &[("BANK", "40702810900000012345")],
            ),
            (
                "ОГРН 1029900000017, р/с 40702810900000012345",
                "1029900000017, р/с 40702810900000012345",
                "bank_account",
                &[("BANK", "40702810900000012345")],
            ),
            (
                "ОКПО 99887766, р/с 40702810900000012345",
                "99887766, р/с 40702810900000012345",
                "bank_account",
                &[("BANK", "40702810900000012345")],
            ),
            (
                "ОГРНИП 329990000000012, паспорт 4512 345678",
                "329990000000012, паспорт 4512 345678",
                "passport_number",
                &[("IDENTITY", "4512 345678")],
            ),
            (
                "ОГРНИП 329990000000019, паспорт серии 45 12 № 345678",
                "329990000000019, паспорт серии 45 12 № 345678",
                "passport_number",
                &[("IDENTITY", "45 12 № 345678")],
            ),
            (
                "ОГРН 1027700132195, тел. 8 (495) 123-45-67",
                "1027700132195, тел. 8 (495) 123-45-67",
                "phone_number",
                &[("PHONE", "8 (495) 123-45-67")],
            ),
            (
                "ИНН 7701234567 и Tel. +7 495 123-45-67.",
                "7701234567 и Tel. +7 495 123-45-67",
                "phone_number",
                &[("PHONE", "+7 495 123-45-67")],
            ),
            // The value before the rule's, too.
            (
                "р/с 40702810900000012345, ИНН 7701234567",
                "40702810900000012345, ИНН 7701234567",
                "bank_account",
                &[("BANK", "40702810900000012345")],
            ),
            // English: a valid tax number after "Tax ID", an account after
            // its label.
            (
                "Tax ID 9900123450, Account 000123456789",
                "9900123450, Account 000123456789",
                "bank_account",
                &[("BANK", "000123456789")],
            ),
        ] {
            let rules = shown_with(source, &[]);
            let start = source.find(wide).unwrap();
            let marked = rules
                .iter()
                .any(|(at, _, text)| start <= *at && at + text.len() <= start + wide.len());
            let mut expected = if marked {
                rules
                    .into_iter()
                    .chain(rest.iter().map(|(category, text)| {
                        (
                            source.find(text).unwrap(),
                            (*category).into(),
                            (*text).into(),
                        )
                    }))
                    .collect()
            } else {
                let category = category_of(label).unwrap().token().to_owned();
                vec![(start, category, wide.to_owned())]
            };
            expected.sort();
            assert_eq!(shown_with(source, &[(wide, label)]), expected, "{source}");
        }
        // On main's rules a labelled ИНН is TAX and a valid ОГРН IDENTITY.
        let pairs = |source: &str, spans: &[(&str, &str)]| -> Vec<(String, String)> {
            shown_with(source, spans)
                .into_iter()
                .map(|(_, category, text)| (category, text))
                .collect()
        };
        let source = "ИНН 7701234567, р/с 40702810900000012345";
        let shown = pairs(
            source,
            &[("7701234567, р/с 40702810900000012345", "bank_account")],
        );
        assert!(
            shown.contains(&("TAX".into(), "7701234567".into())),
            "{shown:?}"
        );
        assert!(
            shown.contains(&("BANK".into(), "40702810900000012345".into())),
            "{shown:?}"
        );
        // The issue's synthetic reproduction: one span from each label's
        // value to the end of its clause.
        let source = "ИНН 7701234567, р/с 40702810900000012345; ОГРНИП 329990000000019, \
                      паспорт 4512 345678";
        let shown: Vec<String> = pairs(
            source,
            &[
                ("7701234567, р/с 40702810900000012345", "bank_account"),
                ("329990000000019, паспорт 4512 345678", "passport_number"),
            ],
        )
        .into_iter()
        .map(|(_, text)| text)
        .collect();
        assert_eq!(
            shown,
            [
                "7701234567",
                "40702810900000012345",
                "329990000000019",
                "4512 345678"
            ]
        );
    }

    /// #739's pre-gate review inputs (target/review-x2, c1–c5), each with
    /// the model's spans and scores, as the user is shown them. Where a
    /// piece would take a lower-scored address, a postcode, an amount, a
    /// date, a ZIP code, an invoice's or an order's number or OCR noise,
    /// what is shown is main's; a field after its own label takes the
    /// label's category («КПП 770101001» TAX, «БИК …» BANK; ОКТМО and КБК
    /// nothing), and a passport before its date of issue stays. The
    /// synthetic reproduction and an ОКПО under a tax span are left to the
    /// tests above and to #563, whose rules change what main shows there.
    #[test]
    fn the_wide_identifier_review_inputs_show_main_or_their_own_fields() {
        for (id, source, model, expected) in [
            (
                "R1 issue row1",
                "ИНН 7701234567, р/с 40702810900000012345",
                &[("7701234567, р/с 40702810900000012345", "bank_account", 0.8)][..],
                "TAX«7701234567» | BANK«40702810900000012345»",
            ),
            (
                "A1 amount space phone",
                "ИНН 7701234567, на сумму 500 000 руб.",
                &[("7701234567, на сумму 500 000", "phone_number", 0.8)][..],
                "TAX«7701234567»",
            ),
            (
                "A2 amount nospace phone",
                "ИНН 7701234567, на сумму 150000000 руб.",
                &[("7701234567, на сумму 150000000", "phone_number", 0.8)][..],
                "TAX«7701234567»",
            ),
            (
                "A3 amount nospace tax",
                "ИНН 7701234567, на сумму 150000000 руб.",
                &[("7701234567, на сумму 150000000", "tax_id", 0.8)][..],
                "TAX«7701234567»",
            ),
            (
                "A4 amount words bank",
                "ИНН 7701234567, на сумму 150000000 (сто пятьдесят миллионов) рублей",
                &[("7701234567, на сумму 150000000", "bank_account", 0.8)][..],
                "TAX«7701234567»",
            ),
            (
                "A5 amount words phone",
                "ИНН 7701234567, на сумму 15000000 (пятнадцать миллионов) рублей",
                &[("7701234567, на сумму 15000000", "phone_number", 0.8)][..],
                "TAX«7701234567»",
            ),
            (
                "A6 в размере",
                "ИНН 7701234567, задолженность в размере 2500000 руб.",
                &[(
                    "7701234567, задолженность в размере 2500000",
                    "bank_account",
                    0.8,
                )][..],
                "TAX«7701234567»",
            ),
            (
                "A7 amount kop",
                "ИНН 7701234567, сумма 2500000 руб. 00 коп.",
                &[("7701234567, сумма 2500000 руб. 00", "tax_id", 0.8)][..],
                "TAX«7701234567»",
            ),
            (
                "C1 case no",
                "ИНН 7701234567, по делу № 12-345/2026",
                &[("7701234567, по делу № 12-345/2026", "tax_id", 0.8)][..],
                "TAX«7701234567» | CASE«12-345/2026»",
            ),
            (
                "C2 case no 2-",
                "ИНН 7701234567, по делу № 2-1234567",
                &[("7701234567, по делу № 2-1234567", "passport_number", 0.8)][..],
                "TAX«7701234567» | CASE«2-1234567»",
            ),
            (
                "C3 case А40",
                "ИНН 7701234567, по делу № А40-123456/2026",
                &[(
                    "7701234567, по делу № А40-123456/2026",
                    "passport_number",
                    0.8,
                )][..],
                "TAX«7701234567» | CASE«А40-123456/2026»",
            ),
            (
                "D1 date words",
                "ИНН 7701234567, выдан 12 марта 2026 г.",
                &[("7701234567, выдан 12 марта 2026", "passport_number", 0.8)][..],
                "TAX«7701234567»",
            ),
            (
                "D2 date iso",
                "ИНН 7701234567, дата 2026-03-12",
                &[("7701234567, дата 2026-03-12", "tax_id", 0.8)][..],
                "TAX«7701234567»",
            ),
            (
                "D3 date iso phone",
                "ИНН 7701234567, дата 2026-03-12",
                &[("7701234567, дата 2026-03-12", "phone_number", 0.8)][..],
                "TAX«7701234567»",
            ),
            (
                "D4 date slash",
                "ИНН 7701234567 от 12/03/2026",
                &[("7701234567 от 12/03/2026", "phone_number", 0.8)][..],
                "TAX«7701234567»",
            ),
            (
                "P1 postcode",
                "ИНН 7701234567, адрес: 123456, г. Энск, ул. Лесная, д. 5",
                &[("7701234567, адрес: 123456", "passport_number", 0.8)][..],
                "TAX«7701234567»",
            ),
            (
                "P2 postcode+addr model",
                "ИНН 7701234567, адрес: 123456, г. Энск, ул. Лесная, д. 5",
                &[
                    ("7701234567, адрес: 123456", "passport_number", 0.9),
                    ("123456, г. Энск, ул. Лесная, д. 5", "address", 0.7),
                ][..],
                "TAX«7701234567» | ADDRESS«123456, г. Энск, ул. Лесная, д. 5»",
            ),
            (
                "P3 postcode+addr model eq",
                "ИНН 7701234567, адрес: 123456, г. Энск, ул. Лесная, д. 5",
                &[
                    ("7701234567, адрес: 123456", "passport_number", 0.8),
                    ("123456, г. Энск, ул. Лесная, д. 5", "address", 0.8),
                ][..],
                "TAX«7701234567» | ADDRESS«123456, г. Энск, ул. Лесная, д. 5»",
            ),
            (
                "O1 oktmo",
                "ИНН 7701234567, ОКТМО 45382000",
                &[("7701234567, ОКТМО 45382000", "tax_id", 0.8)][..],
                "TAX«7701234567»",
            ),
            (
                "O2 kbk",
                "ИНН 7701234567, КБК 18210102010011000110",
                &[("7701234567, КБК 18210102010011000110", "bank_account", 0.8)][..],
                "TAX«7701234567»",
            ),
            (
                "O3 okved phone",
                "ИНН 7701234567, ОКВЭД 47.11",
                &[("7701234567, ОКВЭД 47.11", "phone_number", 0.8)][..],
                "TAX«7701234567»",
            ),
            (
                "O5 okato",
                "ИНН 7701234567, ОКАТО 45286575000",
                &[("7701234567, ОКАТО 45286575000", "tax_id", 0.8)][..],
                "TAX«7701234567»",
            ),
            (
                "O6 knd",
                "ИНН 7701234567, форма по КНД 1111501",
                &[("7701234567, форма по КНД 1111501", "phone_number", 0.8)][..],
                "TAX«7701234567»",
            ),
            (
                "I1 invoice",
                "ИНН 7701234567, оплата по счёту № 12345678 от 01.02.2026",
                &[(
                    "7701234567, оплата по счёту № 12345678",
                    "bank_account",
                    0.8,
                )][..],
                "TAX«7701234567»",
            ),
            (
                "I2 invoice phone",
                "ИНН 7701234567, счёт-фактура № 1234567",
                &[("7701234567, счёт-фактура № 1234567", "phone_number", 0.8)][..],
                "TAX«7701234567»",
            ),
            (
                "I3 contract",
                "ИНН 7701234567, договор № 12345678",
                &[("7701234567, договор № 12345678", "tax_id", 0.8)][..],
                "TAX«7701234567»",
            ),
            (
                "K1 flat",
                "ИНН 7701234567, кв. 15",
                &[("7701234567, кв. 15", "passport_number", 0.8)][..],
                "TAX«7701234567»",
            ),
            (
                "K2 pct",
                "ИНН 7701234567, доля 15%",
                &[("7701234567, доля 15%", "tax_id", 0.8)][..],
                "TAX«7701234567»",
            ),
            (
                "N1 ocr",
                "ИНН 7701234567 1 0 1 1 0 0 1 0",
                &[("7701234567 1 0 1 1 0 0 1 0", "phone_number", 0.8)][..],
                "TAX«7701234567»",
            ),
            (
                "N2 ocr page",
                "ИНН 7701234567 стр. 0 0 2 0 0 1 0",
                &[("7701234567 стр. 0 0 2 0 0 1 0", "phone_number", 0.8)][..],
                "TAX«7701234567»",
            ),
            (
                "N3 kpp slash",
                "ИНН/КПП 7701234567/770101001",
                &[("7701234567/770101001", "tax_id", 0.8)][..],
                "TAX«7701234567/770101001»",
            ),
            (
                "E1 sort",
                "Account No. 12345678, Sort code 12-34-56",
                &[("12345678, Sort code 12-34-56", "bank_account", 0.8)][..],
                "BANK«12345678» | BANK«12-34-56»",
            ),
            (
                "E2 tel ext",
                "Tax ID 9900123450, Tel. 555-0142 ext. 12",
                &[("9900123450, Tel. 555-0142 ext. 12", "phone_number", 0.8)][..],
                "TAX«9900123450» | PHONE«555-0142»",
            ),
            (
                "E3 tax+acct",
                "Tax ID 12-3456789, Account 000123456789",
                &[("12-3456789, Account 000123456789", "bank_account", 0.8)][..],
                "BANK«12-3456789, Account 000123456789»",
            ),
            (
                "E4 ssn amount",
                "SSN 123-45-6789, amount 15000000 USD",
                &[("123-45-6789, amount 15000000", "bank_account", 0.8)][..],
                "BANK«123-45-6789, amount 15000000»",
            ),
            (
                "E5 invoice",
                "Tax ID 9900123450, Invoice No. 12345678",
                &[("9900123450, Invoice No. 12345678", "bank_account", 0.8)][..],
                "TAX«9900123450»",
            ),
            (
                "E6 phone then case",
                "Phone +1 212 555 0142, Case No. 1:24-cv-01234",
                &[(
                    "+1 212 555 0142, Case No. 1:24-cv-01234",
                    "phone_number",
                    0.8,
                )][..],
                "CASE«1:24-cv-01234»",
            ),
            (
                "E7 EIN ZIP",
                "Tax ID 9900123450, Springfield, IL 62704",
                &[("9900123450, Springfield, IL 62704", "passport_number", 0.8)][..],
                "TAX«9900123450»",
            ),
            (
                "X1 rule at end",
                "р/с 40702810900000012345, ИНН 7701234567",
                &[("40702810900000012345, ИНН 7701234567", "bank_account", 0.8)][..],
                "BANK«40702810900000012345» | TAX«7701234567»",
            ),
            (
                "X2 two rules inside",
                "ИНН 7701234567, КПП 770101001, р/с 40702810900000012345, тел. 8 (495) 123-45-67",
                &[(
                    "7701234567, КПП 770101001, р/с 40702810900000012345, тел. 8 (495) 123-45-67",
                    "bank_account",
                    0.8,
                )][..],
                "TAX«7701234567» | TAX«770101001» | BANK«40702810900000012345» | PHONE«8 (495) 123-45-67»",
            ),
            (
                "X3 requisites block",
                "Получатель: ООО «Вега», ИНН 7707083893, КПП 770701001, ОКПО 12345678, р/с 40702810900000001234 в банке, БИК 044525225, к/с 30101810400000000225, ОКТМО 45382000, КБК 18210102010011000110.",
                &[(
                    "7707083893, КПП 770701001, ОКПО 12345678, р/с 40702810900000001234 в банке, БИК 044525225, к/с 30101810400000000225, ОКТМО 45382000, КБК 18210102010011000110",
                    "bank_account",
                    0.8,
                )][..],
                "TAX«7707083893» | TAX«770701001» | TAX«12345678» | BANK«40702810900000001234» | BANK«044525225» | BANK«30101810400000000225»",
            ),
            (
                "X4 CRLF",
                "ИНН 7701234567,\r\nр/с 40702810900000012345\r\n",
                &[(
                    "7701234567,\r\nр/с 40702810900000012345",
                    "bank_account",
                    0.8,
                )][..],
                "TAX«7701234567» | BANK«40702810900000012345»",
            ),
            (
                "X5 cyr before",
                "Счёт получателя № 40702810900000012345 ИНН 7701234567",
                &[(
                    "Счёт получателя № 40702810900000012345 ИНН 7701234567",
                    "bank_account",
                    0.8,
                )][..],
                "TAX«7701234567»",
            ),
            (
                "X6 overlapping models",
                "ИНН 7701234567, р/с 40702810900000012345, паспорт 4512 345678",
                &[
                    ("7701234567, р/с 40702810900000012345", "bank_account", 0.8),
                    (
                        "40702810900000012345, паспорт 4512 345678",
                        "passport_number",
                        0.8,
                    ),
                ][..],
                "TAX«7701234567» | IDENTITY«40702810900000012345, паспорт 4512 345678»",
            ),
            (
                "X7 passport then date",
                "ИНН 772012345678, паспорт 4512 345678 01.02.2010 выдан ОВД",
                &[(
                    "772012345678, паспорт 4512 345678 01.02.2010",
                    "passport_number",
                    0.8,
                )][..],
                "TAX«772012345678» | IDENTITY«4512 345678»",
            ),
            (
                "X8 two phones",
                "тел. 8 (495) 123-45-67, 8 (916) 765-43-21",
                &[("8 (495) 123-45-67, 8 (916) 765-43-21", "phone_number", 0.8)][..],
                "PHONE«8 (495) 123-45-67, 8 (916) 765-43-21»",
            ),
            (
                "X9 phone ext",
                "тел. +7 (495) 123-45-67 доб. 1234",
                &[("+7 (495) 123-45-67 доб. 1234", "phone_number", 0.8)][..],
                "PHONE«+7 (495) 123-45-67 доб. 1234»",
            ),
            (
                "X10 markdown bold",
                "**ИНН** 7701234567, **р/с** 40702810900000012345",
                &[(
                    "7701234567, **р/с** 40702810900000012345",
                    "bank_account",
                    0.8,
                )][..],
                "TAX«7701234567» | BANK«40702810900000012345»",
            ),
            (
                "X11 table cell",
                "| ИНН | 7701234567, р/с 40702810900000012345 |\n|---|---|\n",
                &[("7701234567, р/с 40702810900000012345", "bank_account", 0.8)][..],
                "BANK«7701234567, р/с 40702810900000012345»",
            ),
            (
                "X12 name inside wide",
                "ИНН 7701234567, получатель Иванов Иван Иванович, р/с 40702810900000012345",
                &[(
                    "7701234567, получатель Иванов Иван Иванович, р/с 40702810900000012345",
                    "bank_account",
                    0.8,
                )][..],
                "TAX«7701234567» | BANK«40702810900000012345»",
            ),
            (
                "X12b name model too",
                "ИНН 7701234567, получатель Иванов Иван Иванович, р/с 40702810900000012345",
                &[
                    (
                        "7701234567, получатель Иванов Иван Иванович, р/с 40702810900000012345",
                        "bank_account",
                        0.8,
                    ),
                    ("Иванов Иван Иванович", "person", 0.7),
                ][..],
                "TAX«7701234567» | PERSON«Иванов Иван Иванович» | BANK«40702810900000012345»",
            ),
            (
                "X13 passport wide with code",
                "паспорт 4512 345678, выдан ОВД района Энск 01.02.2010, код подразделения 770-001",
                &[(
                    "4512 345678, выдан ОВД района Энск 01.02.2010, код подразделения 770-001",
                    "passport_number",
                    0.8,
                )][..],
                "IDENTITY«4512 345678, выдан ОВД района Энск 01.02.2010, код подразделения 770-001»",
            ),
            (
                "X14 wide over org",
                "ИНН 7701234567, ООО «Ромашка-2000», р/с 40702810900000012345",
                &[(
                    "7701234567, ООО «Ромашка-2000», р/с 40702810900000012345",
                    "bank_account",
                    0.8,
                )][..],
                "TAX«7701234567» | BANK«40702810900000012345»",
            ),
            (
                "X15 wide phone over address",
                "ИНН 7701234567, 123456, г. Энск, ул. Лесная, д. 5, кв. 7, тел. 8 (495) 123-45-67",
                &[(
                    "7701234567, 123456, г. Энск, ул. Лесная, д. 5, кв. 7, тел. 8 (495) 123-45-67",
                    "phone_number",
                    0.8,
                )][..],
                "TAX«7701234567» | PHONE«8 (495) 123-45-67»",
            ),
            (
                "X15b + address model",
                "ИНН 7701234567, 123456, г. Энск, ул. Лесная, д. 5, кв. 7, тел. 8 (495) 123-45-67",
                &[
                    (
                        "7701234567, 123456, г. Энск, ул. Лесная, д. 5, кв. 7, тел. 8 (495) 123-45-67",
                        "phone_number",
                        0.8,
                    ),
                    ("123456, г. Энск, ул. Лесная, д. 5, кв. 7", "address", 0.6),
                ][..],
                "TAX«7701234567» | ADDRESS«123456, г. Энск, ул. Лесная, д. 5, кв. 7» | PHONE«8 (495) 123-45-67»",
            ),
            (
                "X16 iban",
                "Tax ID 9900123450, IBAN DE89 3704 0044 0532 0130 00",
                &[(
                    "9900123450, IBAN DE89 3704 0044 0532 0130 00",
                    "bank_account",
                    0.8,
                )][..],
                "TAX«9900123450» | BANK«DE89 3704 0044 0532 0130 00»",
            ),
            (
                "X17 duplicate mention",
                "ИНН 7701234567, р/с 40702810900000012345. Повторно: 40702810900000012345.",
                &[("7701234567, р/с 40702810900000012345", "bank_account", 0.8)][..],
                "TAX«7701234567» | BANK«40702810900000012345» | BANK«40702810900000012345»",
            ),
            (
                "X18 rule rejected? ИНН in link",
                "[ИНН 7701234567, р/с 40702810900000012345](https://example.invalid)",
                &[("7701234567, р/с 40702810900000012345", "bank_account", 0.8)][..],
                "TAX«7701234567» | BANK«40702810900000012345» | URL«https://example.invalid»",
            ),
            (
                "X19 nbsp groups",
                "ИНН 7701234567, на сумму 1\u{a0}500\u{a0}000 руб.",
                &[(
                    "7701234567, на сумму 1\u{a0}500\u{a0}000",
                    "phone_number",
                    0.8,
                )][..],
                "TAX«7701234567»",
            ),
            (
                "X20 wide same score diff cat neighbor",
                "ИНН 7701234567, р/с 40702810900000012345",
                &[
                    ("7701234567, р/с 40702810900000012345", "bank_account", 0.8),
                    ("40702810900000012345", "phone_number", 0.85),
                ][..],
                "TAX«7701234567» | BANK«40702810900000012345»",
            ),
            (
                "X21 date model over piece",
                "ИНН 7701234567 от 12345678",
                &[("7701234567 от 12345678", "tax_id", 0.8)][..],
                "TAX«7701234567»",
            ),
            (
                "X22 snils",
                "ИНН 772012345678, СНИЛС 112-233-445 95",
                &[(
                    "772012345678, СНИЛС 112-233-445 95",
                    "national_id_number",
                    0.8,
                )][..],
                "TAX«772012345678» | IDENTITY«112-233-445 95»",
            ),
            (
                "X23 card",
                "ИНН 772012345678, карта 4276 3800 1234 5678",
                &[(
                    "772012345678, карта 4276 3800 1234 5678",
                    "bank_account",
                    0.8,
                )][..],
                "TAX«772012345678»",
            ),
            (
                "Q1 passport rule?",
                "паспорт 4512 345678, выдан ОВД",
                &[][..],
                "",
            ),
            (
                "Q2 passport+addr",
                "ИНН 772012345678, зарегистрирован по адресу: 123456, г. Энск, ул. Лесная, д. 5",
                &[
                    (
                        "772012345678, зарегистрирован по адресу: 123456",
                        "passport_number",
                        0.75,
                    ),
                    ("123456, г. Энск, ул. Лесная, д. 5", "address", 0.6),
                ][..],
                "TAX«772012345678» | ADDRESS«123456, г. Энск, ул. Лесная, д. 5»",
            ),
            (
                "Q3 индекс",
                "ИНН 7701234567, индекс 123456, г. Энск",
                &[("7701234567, индекс 123456", "national_id_number", 0.8)][..],
                "TAX«7701234567»",
            ),
            (
                "Q4 amount words bank",
                "ИНН 7701234567, на сумму 15000000 (пятнадцать миллионов) рублей",
                &[("7701234567, на сумму 15000000", "bank_account", 0.8)][..],
                "TAX«7701234567»",
            ),
            (
                "Q5 amount words tax",
                "ИНН 7701234567, на сумму 15000000 (пятнадцать миллионов) рублей",
                &[("7701234567, на сумму 15000000", "tax_id", 0.8)][..],
                "TAX«7701234567»",
            ),
            (
                "Q6 amount no unit",
                "ИНН 7701234567, задолженность 2500000, пени 125000",
                &[(
                    "7701234567, задолженность 2500000, пени 125000",
                    "phone_number",
                    0.8,
                )][..],
                "TAX«7701234567»",
            ),
            (
                "Q7 iso date tax",
                "ИНН 7701234567, дата 2026-03-12",
                &[("7701234567, дата 2026-03-12", "bank_account", 0.8)][..],
                "TAX«7701234567»",
            ),
            (
                "Q8 contract bank",
                "ИНН 7701234567, договор № 12345678",
                &[("7701234567, договор № 12345678", "bank_account", 0.8)][..],
                "TAX«7701234567»",
            ),
            (
                "Q9 contract phone",
                "ИНН 7701234567, договор № 1234567",
                &[("7701234567, договор № 1234567", "phone_number", 0.8)][..],
                "TAX«7701234567»",
            ),
            (
                "Q10 order no",
                "ИНН 7701234567, заказ № 123456789",
                &[("7701234567, заказ № 123456789", "tax_id", 0.8)][..],
                "TAX«7701234567»",
            ),
            (
                "Q11 year range",
                "ИНН 7701234567, период 2019-2026",
                &[("7701234567, период 2019-2026", "phone_number", 0.8)][..],
                "TAX«7701234567»",
            ),
            (
                "Q12 time",
                "ИНН 7701234567, 12:30 2026",
                &[("7701234567, 12:30 2026", "phone_number", 0.8)][..],
                "TAX«7701234567»",
            ),
            (
                "Q13 pages",
                "ИНН 7701234567, л.д. 123-145",
                &[("7701234567, л.д. 123-145", "phone_number", 0.8)][..],
                "TAX«7701234567»",
            ),
            (
                "Q14 statute",
                "ИНН 7701234567, ст. 1234567 ГК",
                &[("7701234567, ст. 1234567", "phone_number", 0.8)][..],
                "TAX«7701234567»",
            ),
            (
                "Q15 law number",
                "ИНН 7701234567, Федеральный закон от 26.10.2002 № 127-ФЗ",
                &[(
                    "7701234567, Федеральный закон от 26.10.2002 № 127",
                    "phone_number",
                    0.8,
                )][..],
                "TAX«7701234567»",
            ),
            (
                "Q16 ocr letters spaced",
                "ИНН 7701234567 4 5 1 2 3 4 5 6 7 8",
                &[("7701234567 4 5 1 2 3 4 5 6 7 8", "passport_number", 0.8)][..],
                "TAX«7701234567»",
            ),
            (
                "Q17 kadastr",
                "ИНН 7701234567, кадастровый номер 77:01:0001001:1234",
                &[(
                    "7701234567, кадастровый номер 77:01:0001001:1234",
                    "tax_id",
                    0.8,
                )][..],
                "TAX«7701234567» | IDENTITY«77:01:0001001:1234»",
            ),
            (
                "Q18 vin",
                "ИНН 7701234567, VIN XTA21099012345678",
                &[("7701234567, VIN XTA21099012345678", "passport_number", 0.8)][..],
                "TAX«7701234567»",
            ),
            (
                "Q19 reg plate",
                "ИНН 7701234567, гос. номер А123ВС77",
                &[("7701234567, гос. номер А123ВС77", "passport_number", 0.8)][..],
                "TAX«7701234567»",
            ),
            (
                "Q20 EN zip",
                "Tax ID 9900123450, ZIP 62704-1234",
                &[("9900123450, ZIP 62704-1234", "phone_number", 0.8)][..],
                "TAX«9900123450»",
            ),
            (
                "Q21 EN amount",
                "Tax ID 9900123450, amount 15000000 (fifteen million) dollars",
                &[("9900123450, amount 15000000", "bank_account", 0.8)][..],
                "TAX«9900123450»",
            ),
            (
                "Q22 EN date",
                "Tax ID 9900123450, filed 2026-03-12",
                &[("9900123450, filed 2026-03-12", "phone_number", 0.8)][..],
                "TAX«9900123450»",
            ),
            (
                "Q23 EN case",
                "Tax ID 9900123450, Docket 2024-123456",
                &[("9900123450, Docket 2024-123456", "phone_number", 0.8)][..],
                "TAX«9900123450»",
            ),
            (
                "Q24 percent model",
                "ИНН 7701234567, ставка 7,5 %",
                &[("7701234567, ставка 7,5", "phone_number", 0.8)][..],
                "TAX«7701234567»",
            ),
            (
                "Q25 rest overlaps date model",
                "ИНН 7701234567, 2026-03-12 года рождения",
                &[
                    ("7701234567, 2026-03-12", "phone_number", 0.8),
                    ("2026-03-12", "date", 0.6),
                ][..],
                "TAX«7701234567» | DATE«2026-03-12»",
            ),
            (
                "T1 10-digit amount words",
                "ИНН 7701234567, на сумму 1234567890 (один миллиард) рублей",
                &[("7701234567, на сумму 1234567890", "tax_id", 0.8)][..],
                "TAX«7701234567»",
            ),
            (
                "T2 case model overlaps",
                "ИНН 7701234567, дело 2-1234567 рассмотрено",
                &[
                    ("7701234567, дело 2-1234567", "phone_number", 0.8),
                    ("2-1234567", "case_number", 0.6),
                ][..],
                "TAX«7701234567» | CASE«2-1234567»",
            ),
            (
                "T3 date model overlaps",
                "ИНН 7701234567, дата рождения 1985-03-12",
                &[
                    ("7701234567, дата рождения 1985-03-12", "phone_number", 0.8),
                    ("1985-03-12", "date", 0.6),
                ][..],
                "TAX«7701234567» | DATE«1985-03-12»",
            ),
            (
                "T4 addr house model",
                "ИНН 7701234567, адрес: ул. Лесная, д. 1234567",
                &[
                    (
                        "7701234567, адрес: ул. Лесная, д. 1234567",
                        "phone_number",
                        0.8,
                    ),
                    ("ул. Лесная, д. 1234567", "address", 0.6),
                ][..],
                "TAX«7701234567» | ADDRESS«ул. Лесная, д. 1234567»",
            ),
            (
                "T5 EN addr zip model",
                "Tax ID 9900123450, 742 Evergreen Terrace, Springfield, IL 62704-1234",
                &[
                    (
                        "9900123450, 742 Evergreen Terrace, Springfield, IL 62704-1234",
                        "phone_number",
                        0.8,
                    ),
                    (
                        "742 Evergreen Terrace, Springfield, IL 62704-1234",
                        "address",
                        0.6,
                    ),
                ][..],
                "TAX«9900123450» | ADDRESS«742 Evergreen Terrace, Springfield, IL 62704-1234»",
            ),
            (
                "T6 EN addr zip no model addr",
                "Tax ID 9900123450, 742 Evergreen Terrace, Springfield, IL 62704-1234",
                &[(
                    "9900123450, 742 Evergreen Terrace, Springfield, IL 62704-1234",
                    "phone_number",
                    0.8,
                )][..],
                "TAX«9900123450»",
            ),
            (
                "T7 rule rejected keeps wide",
                "ИНН 7701234567, р/с 40702810900000012345",
                &[
                    ("7701234567, р/с 40702810900000012345", "bank_account", 0.8),
                    ("ИНН 7701234567", "organization", 0.99),
                ][..],
                "TAX«7701234567» | BANK«40702810900000012345»",
            ),
            (
                "U1 phone label then amount",
                "ИНН 7701234567, тел. 8 (495) 123-45-67, оплачено 2500000",
                &[(
                    "7701234567, тел. 8 (495) 123-45-67, оплачено 2500000",
                    "phone_number",
                    0.8,
                )][..],
                "TAX«7701234567» | PHONE«8 (495) 123-45-67»",
            ),
            (
                "U2 bank wide then BIK",
                "ИНН 7701234567, р/с 40702810900000012345 в ПАО «Банк», БИК 044525225",
                &[(
                    "7701234567, р/с 40702810900000012345 в ПАО «Банк», БИК 044525225",
                    "bank_account",
                    0.8,
                )][..],
                "TAX«7701234567» | BANK«40702810900000012345» | BANK«044525225»",
            ),
            (
                "U3 passport wide with issue date code",
                "ИНН 772012345678, паспорт 4512 345678, выдан 01.02.2010, код 770-001",
                &[(
                    "772012345678, паспорт 4512 345678, выдан 01.02.2010, код 770-001",
                    "passport_number",
                    0.8,
                )][..],
                "TAX«772012345678» | IDENTITY«4512 345678»",
            ),
            (
                "U4 EN ssn wide",
                "SSN 123-45-6789, DOB 03/12/1985, Phone (212) 555-0142",
                &[(
                    "123-45-6789, DOB 03/12/1985, Phone (212) 555-0142",
                    "phone_number",
                    0.8,
                )][..],
                "DATE«03/12/1985» | PHONE«Phone (212) 555-0142»",
            ),
        ] {
            let raw: Vec<RawEntity> = model
                .iter()
                .map(|(text, label, score)| {
                    let start = source.find(text).unwrap();
                    RawEntity {
                        start,
                        end: start + text.len(),
                        label: (*label).into(),
                        score: *score,
                        text: (*text).into(),
                    }
                })
                .collect();
            let shown: Vec<String> = recognition::proposals(source, &raw)
                .unwrap()
                .into_iter()
                .map(|span| format!("{}«{}»", span.category, &source[span.range]))
                .collect();
            assert_eq!(shown.join(" | "), expected, "{id}: {source}");
        }
    }

    /// #739's delta review: a series is a passport's only after "паспорт" or
    /// alone before a passport's shape; a `context`'s label needs a value
    /// its `value` takes (spaces aside), and "счёт" or "сч." an account's
    /// twenty digits; twenty digits alone need a Russian account's currency
    /// code (no УИН, no КБК); a phone ends before a year; an IBAN keeps its
    /// letter groups.
    #[test]
    fn a_wide_identifier_span_keeps_only_its_own_fields_after_the_delta_review() {
        for (source, wide, label, expected) in [
            // A series.
            (
                "ИНН 772012345678, свидетельство серии 77 № 012345678",
                "772012345678, свидетельство серии 77 № 012345678",
                "passport_number",
                "TAX«772012345678»",
            ),
            (
                "ИНН 772012345678, бланк серии 7701 № 123456",
                "772012345678, бланк серии 7701 № 123456",
                "passport_number",
                "TAX«772012345678»",
            ),
            (
                "ИНН 772012345678, свидетельство серии 77 № 012345678, тел. +7 495 123-45-67",
                "772012345678, свидетельство серии 77 № 012345678, тел. +7 495 123-45-67",
                "phone_number",
                "TAX«772012345678» | PHONE«+7 495 123-45-67»",
            ),
            (
                "ИНН 772012345678, паспорт гражданина РФ серии 45 12 № 345678",
                "772012345678, паспорт гражданина РФ серии 45 12 № 345678",
                "passport_number",
                "TAX«772012345678» | IDENTITY«45 12 № 345678»",
            ),
            (
                "ИНН 772012345678, серия 4512 № 345678",
                "772012345678, серия 4512 № 345678",
                "passport_number",
                "TAX«772012345678» | IDENTITY«4512 № 345678»",
            ),
            (
                "ИНН 772012345678, серия 77 № 0123456",
                "772012345678, серия 77 № 0123456",
                "passport_number",
                "TAX«772012345678»",
            ),
            // A context's value, and "счёт" alone.
            (
                "ИНН 7701234567, счёт № 12345678 от 01.02.2026",
                "7701234567, счёт № 12345678",
                "tax_id",
                "TAX«7701234567»",
            ),
            (
                "ИНН 7701234567, сч. № 12345678",
                "7701234567, сч. № 12345678",
                "tax_id",
                "TAX«7701234567»",
            ),
            (
                "ИНН 7701234567, р/с 40702810900000012345, счёт № 12345678",
                "7701234567, р/с 40702810900000012345, счёт № 12345678",
                "bank_account",
                "TAX«7701234567» | BANK«40702810900000012345»",
            ),
            (
                "ИНН 7701234567, счёт 40702810900000012345",
                "7701234567, счёт 40702810900000012345",
                "bank_account",
                "TAX«7701234567» | BANK«40702810900000012345»",
            ),
            (
                "ИНН 7701234567, кн 12345678",
                "7701234567, кн 12345678",
                "tax_id",
                "TAX«7701234567»",
            ),
            (
                "ИНН 7701234567, кн 12345678, р/с 40702810900000012345",
                "7701234567, кн 12345678, р/с 40702810900000012345",
                "bank_account",
                "TAX«7701234567» | BANK«40702810900000012345»",
            ),
            // Twenty digits alone.
            (
                "ИНН 7701234567, 18810177230000123456",
                "7701234567, 18810177230000123456",
                "bank_account",
                "TAX«7701234567»",
            ),
            (
                "ИНН 7701234567, 18210102010011000110",
                "7701234567, 18210102010011000110",
                "bank_account",
                "TAX«7701234567»",
            ),
            (
                "ИНН 7701234567, 40702978900000012345",
                "7701234567, 40702978900000012345",
                "bank_account",
                "TAX«7701234567» | BANK«40702978900000012345»",
            ),
            // A phone before a year.
            (
                "ИНН 7701234567, тел. 8 495 123-45-67 2026 г.",
                "7701234567, тел. 8 495 123-45-67 2026 г.",
                "phone_number",
                "TAX«7701234567» | PHONE«8 495 123-45-67»",
            ),
            (
                "Tax ID 9900123450, Phone 212 555 0142 2026",
                "9900123450, Phone 212 555 0142 2026",
                "phone_number",
                "TAX«9900123450» | PHONE«212 555 0142»",
            ),
            // An IBAN's letter groups.
            (
                "Tax ID 9900123450, IBAN: GB29 NWBK 6016 1331 9268 19",
                "9900123450, IBAN: GB29 NWBK 6016 1331 9268 19",
                "bank_account",
                "TAX«9900123450» | BANK«GB29 NWBK 6016 1331 9268 19»",
            ),
            (
                "Tax ID 9900123450, IBAN DE89 3704 0044 0532 0130 00",
                "9900123450, IBAN DE89 3704 0044 0532 0130 00",
                "bank_account",
                "TAX«9900123450» | BANK«DE89 3704 0044 0532 0130 00»",
            ),
        ] {
            let shown: Vec<String> = shown_with(source, &[(wide, label)])
                .into_iter()
                .map(|(_, category, text)| format!("{category}«{text}»"))
                .collect();
            assert_eq!(shown.join(" | "), expected, "{source}");
        }
    }

    /// #739's guards: a wide span's rest that is only a label, punctuation
    /// or words, an amount or a date gives nothing; a value the model also
    /// proposed on its own is not proposed twice; a rule's value that is
    /// the whole model span, a name's or a company's span and a wide span
    /// with no rule's value in it are as they were.
    #[test]
    fn a_wide_identifier_span_gives_no_amount_date_or_label() {
        let tax = |source: &str| {
            (
                source.find("7701234567").unwrap(),
                "TAX".to_owned(),
                "7701234567".to_owned(),
            )
        };
        for (source, wide, label) in [
            ("ИНН 7701234567, р/с", "7701234567, р/с", "bank_account"),
            (
                "ИНН 7701234567, паспорт и",
                "7701234567, паспорт и",
                "passport_number",
            ),
            (
                "ИНН 7701234567, на сумму 1 500 000,00 руб.",
                "7701234567, на сумму 1 500 000,00",
                "bank_account",
            ),
            (
                "ИНН 7701234567, на сумму 10 000 000 руб.",
                "7701234567, на сумму 10 000 000",
                "phone_number",
            ),
            (
                "ИНН 7701234567, сумма 150000000 рублей",
                "7701234567, сумма 150000000 рублей",
                "bank_account",
            ),
            (
                "ИНН 7701234567 от 12.03.2031",
                "7701234567 от 12.03.2031",
                "passport_number",
            ),
            (
                "ИНН 7701234567 от 12 марта 2031 г.",
                "7701234567 от 12 марта 2031",
                "tax_id",
            ),
            ("ИНН 7701234567", "7701234567", "tax_id"),
            ("ИНН 7701234567", "7701234567", "bank_account"),
        ] {
            assert_eq!(
                shown_with(source, &[(wide, label)]),
                [tax(source)],
                "{source}"
            );
        }
        // The account the model proposed twice, on its own and in the wide
        // span, is one span.
        let source = "ИНН 7701234567, р/с 40702810900000012345";
        let raw = model_spans(
            source,
            &[
                ("7701234567, р/с 40702810900000012345", "bank_account"),
                ("40702810900000012345", "bank_account"),
            ],
        );
        let account = source.find("40702810900000012345").unwrap();
        let account = account..account + 20;
        assert_eq!(
            correct(source, &raw)
                .iter()
                .filter(|detection| detection.range == account)
                .count(),
            1
        );
        assert_eq!(
            shown_with(source, &[("40702810900000012345", "bank_account")]),
            [
                tax(source),
                (account.start, "BANK".into(), "40702810900000012345".into())
            ]
        );
        // No rule's value inside: the wide span is the model's as before.
        let source = "Tax ID 12-3456789, Account 000123456789";
        let wide = "12-3456789, Account 000123456789";
        assert_eq!(
            shown_with(source, &[(wide, "bank_account")]),
            [(7, "BANK".to_owned(), wide.to_owned())]
        );
        // A company's or a person's span is no identifier's.
        for (wide, label) in [
            (
                "ООО «Ромашка», ИНН 7701234567, р/с 40702810900000012345",
                "organization",
            ),
            (
                "Иванов Иван, ИНН 7701234567, р/с 40702810900000012345",
                "person",
            ),
        ] {
            let source = format!("{wide}.");
            assert!(
                shown_with(&source, &[(wide, label)])
                    .iter()
                    .all(|(_, category, _)| category != "BANK"),
                "{source}"
            );
        }
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

    /// #387: the model's organization "ЯВР" in a column of a known
    /// person's initials is that person; the same letters a company's own
    /// name, or outside a placed position, stay as the model read them.
    #[test]
    fn placed_initials_the_model_took_for_a_company_are_the_person() {
        let shown = |source: &str, model: &[(&str, &str, usize)]| -> Vec<(String, String)> {
            let raw: Vec<RawEntity> = model
                .iter()
                .map(|(text, label, nth)| {
                    let start = source.match_indices(text).nth(*nth).unwrap().0;
                    RawEntity {
                        start,
                        end: start + text.len(),
                        label: (*label).into(),
                        score: 0.9,
                        text: (*text).into(),
                    }
                })
                .collect();
            recognition::proposals(source, &raw)
                .unwrap()
                .into_iter()
                .map(|span| (span.category, source[span.range].to_owned()))
                .collect()
        };
        let pairs = |items: &[(&str, &str)]| -> Vec<(String, String)> {
            items
                .iter()
                .map(|(c, t)| ((*c).to_owned(), (*t).to_owned()))
                .collect()
        };
        let table = "Ответственный исполнитель: Яков Викторович Ростов.\n\n\
            | Этап | Исполнитель |\n|---|---|\n| 1 | ЯВР |\n| 2 | ЯВР |\n";
        let name = ("Яков Викторович Ростов", "person", 0);
        assert_eq!(
            shown(
                table,
                &[name, ("ЯВР", "organization", 0), ("ЯВР", "organization", 1)]
            ),
            pairs(&[
                ("PERSON", "Яков Викторович Ростов"),
                ("PERSON", "ЯВР"),
                ("PERSON", "ЯВР")
            ])
        );
        // A company of that name in the document keeps the letters its own.
        let company = format!("{table}\nПодрядчик: ООО «ЯВР».\n");
        assert_eq!(
            shown(
                &company,
                &[
                    name,
                    ("ЯВР", "organization", 0),
                    ("ЯВР", "organization", 1),
                    ("ООО «ЯВР»", "organization", 0)
                ]
            ),
            pairs(&[
                ("PERSON", "Яков Викторович Ростов"),
                ("ORG", "ЯВР"),
                ("ORG", "ЯВР"),
                ("ORG", "ООО «ЯВР»")
            ])
        );
        // Outside a placed position, or with no person they spell.
        let prose = "Исполнитель Яков Викторович Ростов передал отчёт ЯВР заказчику.";
        assert_eq!(
            shown(prose, &[name, ("ЯВР", "organization", 0)]),
            pairs(&[("PERSON", "Яков Викторович Ростов"), ("ORG", "ЯВР")])
        );
        let nobody = "Исполнитель: Ольга Ивановна Белова.\n\n\
            | Этап | Исполнитель |\n|---|---|\n| 1 | ЯВР |\n";
        assert_eq!(
            shown(
                nobody,
                &[
                    ("Ольга Ивановна Белова", "person", 0),
                    ("ЯВР", "organization", 0)
                ]
            ),
            pairs(&[("PERSON", "Ольга Ивановна Белова"), ("ORG", "ЯВР")])
        );
        // A column the model reads as companies keeps them: "СМУ" spells
        // the person, "ПСК" no one.
        let companies = "Исполнитель работ: Сергей Михайлович Уваров.\n\n\
            | Этап | Исполнитель |\n|---|---|\n| 1 | СМУ |\n| 2 | ПСК |\n";
        assert_eq!(
            shown(
                companies,
                &[
                    ("Сергей Михайлович Уваров", "person", 0),
                    ("СМУ", "organization", 0),
                    ("ПСК", "organization", 0)
                ]
            ),
            pairs(&[
                ("PERSON", "Сергей Михайлович Уваров"),
                ("ORG", "СМУ"),
                ("ORG", "ПСК")
            ])
        );
        let by = "Signed off by Alice B. Carter.\n\n| Step | By |\n|---|---|\n| 1 | ABC |\n| 2 | DLA |\n";
        assert_eq!(
            shown(
                by,
                &[
                    ("Alice B. Carter", "person", 0),
                    ("ABC", "organization", 0),
                    ("DLA", "organization", 0)
                ]
            ),
            pairs(&[
                ("PERSON", "Alice B. Carter"),
                ("ORG", "ABC"),
                ("ORG", "DLA")
            ])
        );
        // The company's name in German or single quotes.
        for quoted in ["„ЯВР“", "‘ЯВР’", "'ЯВР'"] {
            let company = format!("{table}\nПодрядчик: ООО {quoted}.\n");
            let found = shown(
                &company,
                &[name, ("ЯВР", "organization", 0), ("ЯВР", "organization", 1)],
            );
            assert!(
                found
                    .iter()
                    .filter(|(_, text)| text == "ЯВР")
                    .all(|(category, _)| category == "ORG"),
                "{quoted}: {found:?}"
            );
        }
    }

    /// #491: dotless initials after a person's role are that person, and
    /// so are their repeats, though the model took them for companies;
    /// letters the document also writes as a company's, abbreviations,
    /// headings and legal references stay as they were.
    #[test]
    fn initials_after_a_role_the_model_took_for_a_company_are_the_person() {
        // The issue's synthetic reproduction, with the model's
        // organizations on the initials alone and on «ИП ДРС».
        let source = "Потерпевший КЛМ пояснил, что ИП ДРС передал ему товар; КЛМ подтвердил.";
        let people = pairs(&[("PERSON", "КЛМ"), ("PERSON", "ДРС"), ("PERSON", "КЛМ")]);
        for drs in ["ДРС", "ИП ДРС"] {
            assert_eq!(
                shown_for(
                    source,
                    &[
                        ("КЛМ", "organization", 0),
                        (drs, "organization", 0),
                        ("КЛМ", "organization", 1)
                    ]
                ),
                people,
                "{drs}"
            );
        }
        // The model found no one: the role still names them.
        assert_eq!(shown_for(source, &[]), people);
        // Other roles and case forms, emphasis between.
        for (source, word) in [
            ("Гр. БАВ задержан, БАВ отказался от подписи.", "БАВ"),
            (
                "Иск гражданки ЛВГ удовлетворить; ЛВГ вправе обжаловать.",
                "ЛВГ",
            ),
            (
                "**Осуждённому** БАВ назначено наказание. БАВ вину признал.",
                "БАВ",
            ),
            (
                "Законный представитель ЛВГ возражал. Несовершеннолетний ЛВГ просил отложить.",
                "ЛВГ",
            ),
            (
                "Взыскать с индивидуального предпринимателя ЛВГ долг. ЛВГ не явился.",
                "ЛВГ",
            ),
        ] {
            assert_eq!(
                shown_for(
                    source,
                    &[(word, "organization", 0), (word, "organization", 1)]
                ),
                pairs(&[("PERSON", word), ("PERSON", word)]),
                "{source}"
            );
        }
        // A company of the letters keeps its name, and a person beside it
        // stays a person.
        let company = "ООО «КЛМ» и потерпевший Иванов заключили договор.";
        assert_eq!(
            shown_for(
                company,
                &[("ООО «КЛМ»", "organization", 0), ("Иванов", "person", 0)]
            ),
            pairs(&[("ORG", "ООО «КЛМ»"), ("PERSON", "Иванов")])
        );
        // Where the document also writes them as a company's, only the
        // model's organization on the mention after a person's own role is
        // the person: «ИП ДРС» beside «ООО «ДРС»», and «гр. БАВ» beside «АО
        // БАВ». The company keeps its name. The bare repeat the model also
        // took for a company is the review's: it gives the wording one
        // category, the person's here.
        let both = "ИП ДРС передал товар ООО «ДРС», а ДРС оплатило его.";
        assert_eq!(
            shown_for(
                both,
                &[
                    ("ИП ДРС", "organization", 0),
                    ("ООО «ДРС»", "organization", 0),
                    ("ДРС", "organization", 2),
                ],
            ),
            pairs(&[("PERSON", "ДРС"), ("ORG", "ООО «ДРС»"), ("PERSON", "ДРС")])
        );
        let both = "Гр. БАВ работает в АО БАВ.";
        assert_eq!(
            shown_for(
                both,
                &[("БАВ", "organization", 0), ("АО БАВ", "organization", 0)]
            ),
            pairs(&[("PERSON", "БАВ"), ("ORG", "АО БАВ")])
        );
        // Abbreviations after a role, a legal reference, public bodies, a
        // heading in capitals, letters before a quote and a mask name no
        // one.
        for source in [
            "Представитель ООО «Ромашка» и представитель УФК не явились.",
            "Истец ПАО «Ромашка» обратился в суд.",
            "Потерпевший Иванов обратился в ГИБДД МВД по ст. 158 УК РФ.",
            "ПОТЕРПЕВШИЙ КЛМ\n\nОбстоятельства дела.",
            "Свидетель КЛМ «Кедр» дал показания.",
            "Представитель СФР и гр. ХХХ не явились.",
        ] {
            let found = shown_for(source, &[]);
            assert!(
                found.iter().all(|(category, text)| category != "PERSON"
                    || !text.chars().all(char::is_uppercase)),
                "{source}: {found:?}"
            );
        }
        // Letters no role introduces stay as the model read them.
        let alone = "Договор с КЛМ расторгнут.";
        assert_eq!(
            shown_for(alone, &[("КЛМ", "organization", 0)]),
            pairs(&[("ORG", "КЛМ")])
        );
    }

    /// #491's review: what the role path must leave as it was, as main
    /// shows it, and what a party's role changes. Each case is a source,
    /// the model's spans and what the user is shown.
    #[test]
    fn initials_after_a_role_leave_companies_bodies_and_qualifiers_be() {
        type Case<'a> = (
            &'a str,
            Vec<(&'a str, &'a str, usize)>,
            Vec<(&'a str, &'a str)>,
        );
        let org = "organization";
        let cases: Vec<Case> = vec![
            // A wider model organization over the letters keeps them, its
            // own words included.
            (
                "Ответчик КЛМ Групп иск не признал.",
                vec![("КЛМ Групп", org, 0)],
                vec![("ORG", "КЛМ Групп")],
            ),
            (
                "Истец КПК Касса взаимопомощи обратился в суд.",
                vec![("КПК Касса взаимопомощи", org, 0)],
                vec![("ORG", "КПК Касса взаимопомощи")],
            ),
            (
                "Должник НАО ПКБ долг не погасил.",
                vec![("НАО ПКБ", org, 0)],
                vec![("ORG", "НАО ПКБ")],
            ),
            (
                "Потерпевший КЛМ пояснил, а КЛМ Групп поставила товар. КЛМ подтвердил.",
                vec![("КЛМ Групп", org, 0)],
                vec![("ORG", "КЛМ Групп")],
            ),
            // #387's placed initials are as on main: «МТС» is no
            // abbreviation there.
            (
                "Ответственный исполнитель: Мария Тимофеевна Сергеева.\n\n\
                 | Этап | Исполнитель |\n|---|---|\n| 1 | МТС |\n| 2 | МТС |\n",
                vec![
                    ("Мария Тимофеевна Сергеева", "person", 0),
                    ("МТС", org, 0),
                    ("МТС", org, 1),
                ],
                vec![
                    ("PERSON", "Мария Тимофеевна Сергеева"),
                    ("PERSON", "МТС"),
                    ("PERSON", "МТС"),
                ],
            ),
            // Public bodies after a role.
            (
                "Ответчик МЧС России иск не признал. Представитель ФМС возражал.",
                vec![],
                vec![],
            ),
            (
                "Ответчик ТУ Росимущества в Ольховской области иск не признал. \
                 ТУ Росимущества возражало.",
                vec![],
                vec![],
            ),
            (
                "Представитель ДГИ г. Москвы возражал. ДГИ г. Москвы просил отказать.",
                vec![],
                vec![],
            ),
            (
                "Представитель ФТС России и представитель ГСУ СК России не явились.",
                vec![],
                vec![],
            ),
            (
                "Представитель КДН и ЗП администрации района поддержал иск.",
                vec![],
                vec![],
            ),
            (
                "Представитель ОП № 5 УМВД пояснил. Представитель ЦЗН не явился.",
                vec![],
                vec![],
            ),
            ("Свидетель ДПС пояснил.", vec![], vec![]),
            ("Представитель УУП пояснил.", vec![], vec![]),
            ("Ответчик ЦРБ иск не признал.", vec![], vec![]),
            (
                "Представитель АСВ поддержал заявление. Ответчик ГК АСВ возражал.",
                vec![],
                vec![],
            ),
            // A party's role marks nothing of its own, and the model's
            // organization exactly on the letters stays where nothing
            // else says they are a person's.
            (
                "Истец АБВ обратился в суд. АБВ просит взыскать.",
                vec![],
                vec![],
            ),
            (
                "Истец АБВ обратился в суд. АБВ просит взыскать.",
                vec![("АБВ", org, 0), ("АБВ", org, 1)],
                vec![("ORG", "АБВ"), ("ORG", "АБВ")],
            ),
            (
                "Ответчик РСА компенсационную выплату не произвёл. РСА возражал против иска.",
                vec![("РСА", org, 0), ("РСА", org, 1)],
                vec![("ORG", "РСА"), ("ORG", "РСА")],
            ),
            (
                "Ответчик ВСК выплату не произвёл. ВСК возражало.",
                vec![("ВСК", org, 0), ("ВСК", org, 1)],
                vec![("ORG", "ВСК"), ("ORG", "ВСК")],
            ),
            (
                "Ответчик ФКР взносы начислил. Истец УК обслуживает дом. \
                 Ответчик ЕРЦ направил квитанцию.",
                vec![("ФКР", org, 0), ("УК", org, 0), ("ЕРЦ", org, 0)],
                vec![("ORG", "ФКР"), ("ORG", "ЕРЦ")],
            ),
            (
                "Ответчик ГУП иск не признал. Ответчик ДЕЗ возражал. Ответчик ФКУ \
                 возражал. Должник ЖЭК долг не погасил.",
                vec![
                    ("ГУП", org, 0),
                    ("ДЕЗ", org, 0),
                    ("ФКУ", org, 0),
                    ("ЖЭК", org, 0),
                ],
                vec![("ORG", "ДЕЗ"), ("ORG", "ЖЭК")],
            ),
            (
                "Представитель истца КЛМ пояснил. Представитель ответчика ДРС возражал.",
                vec![("КЛМ", org, 0), ("ДРС", org, 0)],
                vec![("ORG", "КЛМ"), ("ORG", "ДРС")],
            ),
            // With a person's evidence elsewhere, a person's own role or
            // the model's person on the letters, it becomes the person.
            (
                "Истец КЛМ обратился с иском. КЛМ просит взыскать. Потерпевший КЛМ пояснил.",
                vec![("КЛМ", org, 0), ("КЛМ", org, 1)],
                vec![("PERSON", "КЛМ"), ("PERSON", "КЛМ"), ("PERSON", "КЛМ")],
            ),
            (
                "Ответчик КЛМ иск не признал. Потерпевший КЛМ пояснил.",
                vec![],
                vec![("PERSON", "КЛМ"), ("PERSON", "КЛМ")],
            ),
            (
                "Истец КЛМ обратился с иском. КЛМ просит взыскать.",
                vec![("КЛМ", "person", 0)],
                vec![("PERSON", "КЛМ"), ("PERSON", "КЛМ")],
            ),
            // The model's person with «гр.» inside is the name alone.
            (
                "Свидетель видела, как гр. БАВ вынес коробку.",
                vec![("гр. БАВ", "person", 0)],
                vec![("PERSON", "БАВ")],
            ),
            (
                "Протокол составлен в отношении гр. Иванов.",
                vec![("гр. Иванов", "person", 0)],
                vec![("PERSON", "Иванов")],
            ),
            // Two letters after a citizen's word are a country.
            (
                "Гражданин РА, ранее не судимый, задержан. По законодательству РА он \
                 совершеннолетний.",
                vec![],
                vec![],
            ),
            (
                "Гражданина РМ допросили. Гражданин США въехал. Гражданка ЧР пояснила. \
                 Гражданин РУ въехал.",
                vec![],
                vec![],
            ),
            // An identifier's label, a date mask or «ИЗЪЯТО» after the
            // letters qualifies nothing.
            (
                "ИП ДРС ИНН 771234567890 передал товар. ДРС товар не вернул.",
                vec![("ИП ДРС", org, 0)],
                vec![
                    ("PERSON", "ДРС"),
                    ("TAX", "771234567890"),
                    ("PERSON", "ДРС"),
                ],
            ),
            (
                "ИП ДРС ИНН 771234567890 передал товар. ДРС товар не вернул.",
                vec![],
                vec![
                    ("PERSON", "ДРС"),
                    ("TAX", "771234567890"),
                    ("PERSON", "ДРС"),
                ],
            ),
            (
                "Подсудимый БАВ ДД.ММ.ГГГГ года рождения ранее не судим.",
                vec![],
                vec![("PERSON", "БАВ")],
            ),
            (
                "Подсудимый БАВ ХХ.ХХ.ХХХХ г.р. ранее не судим.",
                vec![],
                vec![("PERSON", "БАВ")],
            ),
            (
                "Подсудимый БАВ ИЗЪЯТО пояснил.",
                vec![],
                vec![("PERSON", "БАВ")],
            ),
            // A repeat before a word it qualifies is part of a longer
            // name: nothing is spread to it.
            (
                "Потерпевший КЛМ пояснил. КЛМ Групп поставила товар.",
                vec![],
                vec![],
            ),
            (
                "Потерпевший НП пояснил. Председатель НП Содружество КЛМ возражал.",
                vec![],
                vec![],
            ),
            // A definition after a person beside a company names no
            // company.
            (
                "Иванов Иван Иванович, работник ООО «Ромашка» (далее – ИИИ), обратился \
                 в полицию. Потерпевший ИИИ пояснил. ИИИ подтвердил.",
                vec![],
                vec![("PERSON", "ИИИ"), ("PERSON", "ИИИ"), ("PERSON", "ИИИ")],
            ),
            // Letters that qualify the next, capitalised word.
            (
                "Иск гражданина РК БАВ удовлетворить. БАВ прибыл из РК, где действует УК РК.",
                vec![],
                vec![],
            ),
            (
                "Гражданин КНР ЛВГ задержан. ЛВГ въехал из КНР по визе.",
                vec![("ЛВГ", "person", 0), ("ЛВГ", "person", 1)],
                vec![("PERSON", "ЛВГ"), ("PERSON", "ЛВГ")],
            ),
            (
                "Гражданка ДНР ЛВГ и гражданин ФРГ БАВ дали показания.",
                vec![],
                vec![],
            ),
            (
                "Гражданина ОАЭ КЛМ и гражданина КР ДРС допросили.",
                vec![],
                vec![],
            ),
            (
                "Истец КПК Касса взаимопомощи обратился в суд. Ответчик СПК Колос иск признал.",
                vec![],
                vec![],
            ),
            (
                "Истец ДНТ Берёзка обратился в суд с иском. Ответчик НП Содружество возражал.",
                vec![],
                vec![],
            ),
            ("Ответчик ЧОП Щит иск не признал.", vec![], vec![]),
            (
                "Ответчик АКБ Абсолют Банк (ПАО) иск не признал.",
                vec![],
                vec![],
            ),
            (
                "Взыскатель НАО ПКО «Первое клиентское бюро» обратился с заявлением.",
                vec![("НАО ПКО «Первое клиентское бюро»", org, 0)],
                vec![("ORG", "НАО ПКО «Первое клиентское бюро»")],
            ),
            (
                "Взыскатель НАО ПКО «Первое клиентское бюро» обратился с заявлением.",
                vec![],
                vec![],
            ),
            // A company's letters: a party's role yields to the company.
            (
                "Ответчик АО «КЛМ» возражал. Ответчик КЛМ иск не признал. КЛМ просил отказать.",
                vec![("АО «КЛМ»", org, 0), ("КЛМ", org, 1), ("КЛМ", org, 2)],
                vec![("ORG", "АО «КЛМ»"), ("ORG", "КЛМ"), ("ORG", "КЛМ")],
            ),
            (
                "ООО «Кедр Строй Сервис» (далее – КСС) заключило договор. Ответчик КСС \
                 обязательство не исполнил; КСС долг не оплатило.",
                vec![
                    ("ООО «Кедр Строй Сервис»", org, 0),
                    ("КСС", org, 0),
                    ("КСС", org, 1),
                    ("КСС", org, 2),
                ],
                vec![
                    ("ORG", "ООО «Кедр Строй Сервис»"),
                    ("ORG", "КСС"),
                    ("ORG", "КСС"),
                    ("ORG", "КСС"),
                ],
            ),
            (
                "ООО «Кедр Строй Сервис» (далее – КСС) заключило договор. Ответчик КСС \
                 обязательство не исполнил; КСС долг не оплатило.",
                vec![("ООО «Кедр Строй Сервис»", org, 0)],
                vec![("ORG", "ООО «Кедр Строй Сервис»")],
            ),
            (
                "ИП ДРС передал товар ООО «ДРС», а ДРС оплатило его.",
                vec![],
                vec![],
            ),
            (
                "Истец ОАО КЛМ обратился. Ответчик КЛМ возражал.",
                vec![],
                vec![],
            ),
            (
                "Потерпевший КЛМ пояснил. Ранее товар поставило КЛМ (ООО). КЛМ подтвердил.",
                vec![],
                vec![],
            ),
            // Function words in capitals.
            ("Ответчик НЕ явился, о причинах НЕ сообщил.", vec![], vec![]),
        ];
        for (source, model, expected) in cases {
            assert_eq!(shown_for(source, &model), pairs(&expected), "{source}");
        }
    }

    /// What the user is shown for model spans given as `(text, label,
    /// nth)`: the `nth` whole-word occurrence of `text` in `source`.
    fn shown_for(source: &str, model: &[(&str, &str, usize)]) -> Vec<(String, String)> {
        let word = |c: Option<char>| c.is_some_and(char::is_alphanumeric);
        let raw: Vec<RawEntity> = model
            .iter()
            .map(|(text, label, nth)| {
                let start = source
                    .match_indices(text)
                    .map(|(at, _)| at)
                    .filter(|&at| {
                        !word(source[..at].chars().next_back())
                            && !word(source[at + text.len()..].chars().next())
                    })
                    .nth(*nth)
                    .unwrap_or_else(|| panic!("{text} #{nth}"));
                RawEntity {
                    start,
                    end: start + text.len(),
                    label: (*label).into(),
                    score: 0.9,
                    text: (*text).into(),
                }
            })
            .collect();
        recognition::proposals(source, &raw)
            .unwrap()
            .into_iter()
            .map(|span| (span.category, source[span.range].to_owned()))
            .collect()
    }

    fn pairs(items: &[(&str, &str)]) -> Vec<(String, String)> {
        items
            .iter()
            .map(|(c, t)| ((*c).to_owned(), (*t).to_owned()))
            .collect()
    }

    /// #548: a company's short name, a company with a common noun after
    /// it and a team are no people (guide rule 2, #506 items 10 and 11).
    #[test]
    fn a_company_short_name_the_model_took_for_a_person_is_the_company() {
        // The issue's synthetic reproductions.
        let source = "Quillon Freight Corp. (“Quillon”) sued; Defendant Quillon replied. \
                      Pellam airlines and Pellam air fares; the team known as the \
                      Brindle Harriers (BRINDLE HARRIERS LLC).";
        assert_eq!(
            shown_for(
                source,
                &[
                    ("Quillon Freight Corp", "organization", 0),
                    ("Quillon", "person", 1),
                    ("Quillon", "person", 2),
                    ("Pellam airlines", "person", 0),
                    ("Pellam air fares", "person", 0),
                    ("Brindle Harriers", "person", 0),
                    ("BRINDLE HARRIERS LLC", "organization", 0),
                ]
            ),
            pairs(&[
                ("ORG", "Quillon Freight Corp"),
                ("ORG", "Quillon"),
                ("ORG", "Quillon"),
                ("ORG", "Pellam"),
                ("ORG", "Pellam"),
                ("ORG", "Brindle Harriers"),
                ("ORG", "BRINDLE HARRIERS LLC"),
            ])
        );
        // The model may read the noun into a company's span instead; the
        // name before it is a short name then too.
        let source = "Pellam airlines and Pellam air fares; later Pellam flew.";
        assert_eq!(
            shown_for(
                source,
                &[
                    ("Pellam airlines", "organization", 0),
                    ("Pellam air fares", "organization", 0),
                    ("Pellam", "person", 2),
                ]
            ),
            pairs(&[("ORG", "Pellam"), ("ORG", "Pellam"), ("ORG", "Pellam")])
        );
        // Not "Ostrand Medicine" proposed as a person and as a company with
        // no legal form or definition beside it: the model's label alone is
        // no evidence, and the review's conflict rules choose.
        let source = "Pellam Bank sued. The court awarded Pellam attorney’s fees and \
                      Pellam’s costs.";
        assert_eq!(
            shown_for(
                source,
                &[
                    ("Pellam Bank", "organization", 0),
                    ("Pellam", "person", 1),
                    ("Pellam’s", "person", 0),
                ]
            ),
            pairs(&[("ORG", "Pellam Bank"), ("ORG", "Pellam"), ("ORG", "Pellam")])
        );
        // A team's name in capitals elsewhere with its legal form beside it
        // (the model leaves "LLC" out); a short name a firm's definition
        // gives; a Russian one after «далее».
        let source = "BRINDLE HARRIERS LLC sponsor the cup. Brindle Harriers fans attend. \
                      Stewarts Law LLP (“Stewarts”) acted; Stewarts wrote.";
        assert_eq!(
            shown_for(
                source,
                &[
                    ("BRINDLE HARRIERS", "organization", 0),
                    ("Brindle Harriers", "person", 0),
                    ("Stewarts Law LLP", "organization", 0),
                    ("Stewarts", "person", 2),
                ]
            ),
            pairs(&[
                ("ORG", "BRINDLE HARRIERS"),
                ("ORG", "Brindle Harriers"),
                ("ORG", "Stewarts Law LLP"),
                ("ORG", "Stewarts"),
                ("ORG", "Stewarts"),
            ])
        );
        let source = "ООО «Кедр» (далее — «Кедр») поставило товар. Счёт выставил Кедр.";
        assert_eq!(
            shown_for(
                source,
                &[("ООО «Кедр»", "organization", 0), ("Кедр", "person", 2)]
            ),
            pairs(&[("ORG", "ООО «Кедр»"), ("ORG", "Кедр"), ("ORG", "Кедр")])
        );
    }

    /// #548 guards: people who share a company's word stay people.
    #[test]
    fn people_who_share_a_company_word_stay_people() {
        // A title, or a given name, makes the word a person's here.
        let source = "Frost Bank sued. Mr Frost replied, and Frost signed.";
        assert_eq!(
            shown_for(
                source,
                &[
                    ("Frost Bank", "organization", 0),
                    ("Mr Frost", "person", 0),
                    ("Frost", "person", 2),
                ]
            ),
            pairs(&[
                ("ORG", "Frost Bank"),
                ("PERSON", "Frost"),
                ("PERSON", "Frost"),
            ])
        );
        let source = "Pellam Bank sued. Ada Pellam testified; Pellam said the bank erred.";
        assert_eq!(
            shown_for(
                source,
                &[
                    ("Pellam Bank", "organization", 0),
                    ("Ada Pellam", "person", 0),
                    ("Pellam", "person", 2),
                ]
            ),
            pairs(&[
                ("ORG", "Pellam Bank"),
                ("PERSON", "Ada Pellam"),
                ("PERSON", "Pellam"),
            ])
        );
        // A firm named after a partner: "Stewart" is another word than
        // "Stewarts"; a bare "Smith" beside "John Smith" stays as the
        // model gave it.
        let source = "Stewarts Law LLP (“Stewarts”) acted. Mr Stewart signed for Stewarts. \
                      Smith LLP acts for John Smith; later Smith signed.";
        assert_eq!(
            shown_for(
                source,
                &[
                    ("Stewarts Law LLP", "organization", 0),
                    ("Stewarts", "person", 1),
                    ("Mr Stewart", "person", 0),
                    ("Smith LLP", "organization", 0),
                    ("John Smith", "person", 0),
                    ("Smith", "person", 2),
                ]
            ),
            pairs(&[
                ("ORG", "Stewarts Law LLP"),
                ("ORG", "Stewarts"),
                ("PERSON", "Stewart"),
                ("ORG", "Stewarts"),
                ("ORG", "Smith LLP"),
                ("PERSON", "John Smith"),
                ("PERSON", "Smith"),
            ])
        );
        // A defined generic term is no short name; a titled person whose
        // surname is an industry noun's sentence stays; initials stay.
        let source = "Larkfield Supply LLC (the “Company”) hired Ivo Penrow. Ms Ada Bank \
                      and JB signed for JB Holdings Ltd.";
        assert_eq!(
            shown_for(
                source,
                &[
                    ("Larkfield Supply LLC", "organization", 0),
                    ("Ivo Penrow", "person", 0),
                    ("Ada Bank", "person", 0),
                    ("JB", "person", 0),
                    ("JB Holdings Ltd", "organization", 0),
                ]
            ),
            pairs(&[
                ("ORG", "Larkfield Supply LLC"),
                ("PERSON", "Ivo Penrow"),
                ("PERSON", "Ada Bank"),
                ("PERSON", "JB"),
                ("ORG", "JB Holdings Ltd"),
            ])
        );
    }

    /// #548 review inputs (target/review-o2 cases): firms named after
    /// people, a company named after its owner, a model ORG caption, «ИП»,
    /// "Bank" and "Medicine" surnames, "bank" after a person's name, cut
    /// "of medicine" companies and a team name carried on by a lowercase
    /// word all stay as they were: people kept, nothing uncovered. Only
    /// "Frost"/"Pellam" beside "Frost Bank"/"Pellam Bank" with no title or
    /// full name become the bank, as the issue's "Frost" does.
    #[test]
    fn review_inputs_keep_people_who_share_a_company_word() {
        #[allow(clippy::type_complexity)]
        let cases: &[(&str, &str, &[(&str, &str, usize)], &[(&str, &str)])] = &[
            (
                "davis_bare",
                "Davis Polk & Wardwell LLP represented the bank. Davis testified that he signed the note. Davis's affidavit was filed.",
                &[
                    ("Davis Polk & Wardwell LLP", "organization", 0),
                    ("Davis", "person", 1),
                    ("Davis's", "person", 0),
                ],
                &[
                    ("ORG", "Davis Polk & Wardwell LLP"),
                    ("PERSON", "Davis"),
                    ("PERSON", "Davis's"),
                ],
            ),
            (
                "davis_full",
                "Davis Polk & Wardwell LLP represented the bank. Witness John Davis was sworn. Davis testified that he signed the note.",
                &[
                    ("Davis Polk & Wardwell LLP", "organization", 0),
                    ("John Davis", "person", 0),
                    ("Davis", "person", 2),
                ],
                &[
                    ("ORG", "Davis Polk & Wardwell LLP"),
                    ("PERSON", "John Davis"),
                    ("PERSON", "Davis"),
                ],
            ),
            (
                "davis_titled",
                "Davis Polk & Wardwell LLP represented the bank. Mr. Davis was sworn. Davis testified that he signed the note.",
                &[
                    ("Davis Polk & Wardwell LLP", "organization", 0),
                    ("Davis", "person", 1),
                    ("Davis", "person", 2),
                ],
                &[
                    ("ORG", "Davis Polk & Wardwell LLP"),
                    ("PERSON", "Davis"),
                    ("PERSON", "Davis"),
                ],
            ),
            (
                "smith_wesson",
                "Smith & Wesson Brands, Inc. makes firearms. Smith told the jury she bought it. Smith's receipt was lost.",
                &[
                    ("Smith & Wesson Brands, Inc", "organization", 0),
                    ("Smith", "person", 1),
                    ("Smith's", "person", 0),
                ],
                &[
                    ("ORG", "Smith & Wesson Brands, Inc"),
                    ("PERSON", "Smith"),
                    ("PERSON", "Smith's"),
                ],
            ),
            (
                "ford",
                "Ford Motor Co. recalled the car. Ford testified at trial that the brakes failed.",
                &[("Ford Motor Co", "organization", 0), ("Ford", "person", 1)],
                &[("ORG", "Ford Motor Co"), ("PERSON", "Ford")],
            ),
            (
                "ford_cited",
                "Ford testified that the brakes failed. See Davis v. Ford Motor Co., 123 F.3d 456 (5th Cir. 2001). Ford's account was credible.",
                &[
                    ("Ford", "person", 0),
                    ("Davis", "person", 0),
                    ("Ford Motor Co", "organization", 0),
                    ("Ford's", "person", 0),
                ],
                &[
                    ("PERSON", "Ford"),
                    ("ORG", "Ford Motor Co"),
                    ("PERSON", "Ford's"),
                ],
            ),
            (
                "wells",
                "Wells Fargo Bank, N.A. foreclosed on the house. Wells, the borrower, appealed.",
                &[
                    ("Wells Fargo Bank, N.A", "organization", 0),
                    ("Wells", "person", 1),
                ],
                &[("ORG", "Wells Fargo Bank, N.A"), ("PERSON", "Wells")],
            ),
            (
                "goldman",
                "Goldman Sachs & Co. LLC underwrote the issue. Goldman, the plaintiff's expert, opined.",
                &[
                    ("Goldman Sachs & Co. LLC", "organization", 0),
                    ("Goldman", "person", 1),
                ],
                &[("ORG", "Goldman Sachs & Co. LLC"), ("PERSON", "Goldman")],
            ),
            (
                "johnson",
                "Johnson Controls, Inc. hired the plaintiff. Johnson was her supervisor and Johnson fired her.",
                &[
                    ("Johnson Controls, Inc", "organization", 0),
                    ("Johnson", "person", 1),
                    ("Johnson", "person", 2),
                ],
                &[
                    ("ORG", "Johnson Controls, Inc"),
                    ("PERSON", "Johnson"),
                    ("PERSON", "Johnson"),
                ],
            ),
            (
                "kim",
                "Kim & Chang LLC advised the buyer. Kim's affidavit says otherwise.",
                &[
                    ("Kim & Chang LLC", "organization", 0),
                    ("Kim's", "person", 0),
                ],
                &[("ORG", "Kim & Chang LLC"), ("PERSON", "Kim's")],
            ),
            (
                "baker",
                "Baker McKenzie LLP acted for the landlord. Baker signed the lease as tenant.",
                &[
                    ("Baker McKenzie LLP", "organization", 0),
                    ("Baker", "person", 1),
                ],
                &[("ORG", "Baker McKenzie LLP"), ("PERSON", "Baker")],
            ),
            (
                "llc_person",
                "Ada Pellam LLC is owned by Ada Pellam. Ada Pellam testified at trial. Ada said no.",
                &[
                    ("Ada Pellam LLC", "organization", 0),
                    ("Ada Pellam", "person", 1),
                    ("Ada Pellam", "person", 2),
                    ("Ada", "person", 3),
                ],
                &[
                    ("ORG", "Ada Pellam LLC"),
                    ("PERSON", "Ada Pellam"),
                    ("PERSON", "Ada Pellam"),
                    ("PERSON", "Ada"),
                ],
            ),
            (
                "llc_person_titled",
                "Ada Pellam LLC is owned by Ms Ada Pellam. Ada Pellam testified at trial. Ada said no.",
                &[
                    ("Ada Pellam LLC", "organization", 0),
                    ("Ada Pellam", "person", 1),
                    ("Ada Pellam", "person", 2),
                    ("Ada", "person", 3),
                ],
                &[
                    ("ORG", "Ada Pellam LLC"),
                    ("PERSON", "Ada Pellam"),
                    ("PERSON", "Ada Pellam"),
                    ("PERSON", "Ada"),
                ],
            ),
            (
                "org_mislabel_caption",
                "ADA PELLAM, Plaintiff, v. QUILLON FREIGHT CORP, Defendant.\n\nAda Pellam testified. Ada Pellam then left.",
                &[
                    ("ADA PELLAM", "organization", 0),
                    ("QUILLON FREIGHT CORP", "organization", 0),
                    ("Ada Pellam", "person", 0),
                    ("Ada Pellam", "person", 1),
                ],
                &[
                    ("ORG", "ADA PELLAM"),
                    ("ORG", "QUILLON FREIGHT CORP"),
                    ("PERSON", "Ada Pellam"),
                    ("PERSON", "Ada Pellam"),
                ],
            ),
            (
                "ip_ru",
                "Договор между ООО «Кедр» и ИП Иванов. Иванов обязуется поставить товар. Иванов подписал договор.",
                &[
                    ("ООО «Кедр»", "organization", 0),
                    ("ИП Иванов", "organization", 0),
                    ("Иванов", "person", 1),
                    ("Иванов", "person", 2),
                ],
                &[
                    ("ORG", "ООО «Кедр»"),
                    ("ORG", "ИП Иванов"),
                    ("PERSON", "Иванов"),
                    ("PERSON", "Иванов"),
                ],
            ),
            (
                "ip_ru_defined",
                "ИП Иванов (далее — «Иванов») обязуется поставить товар. Иванов подписал договор.",
                &[("ИП Иванов", "organization", 0), ("Иванов", "person", 2)],
                &[
                    ("ORG", "ИП Иванов"),
                    ("PERSON", "Иванов"),
                    ("PERSON", "Иванов"),
                ],
            ),
            (
                "ru_partners",
                "ООО «Иванов и партнёры» представляет истца. Иванов дал показания. Иванова допросили.",
                &[
                    ("ООО «Иванов и партнёры»", "organization", 0),
                    ("Иванов", "person", 1),
                    ("Иванова", "person", 0),
                ],
                &[
                    ("ORG", "ООО «Иванов и партнёры»"),
                    ("PERSON", "Иванов"),
                    ("PERSON", "Иванова"),
                ],
            ),
            (
                "ru_partners_full",
                "ООО «Иванов и партнёры» представляет истца. Иванов И.И. дал показания. Иванов подтвердил.",
                &[
                    ("ООО «Иванов и партнёры»", "organization", 0),
                    ("Иванов И.И.", "person", 0),
                    ("Иванов", "person", 2),
                ],
                &[
                    ("ORG", "ООО «Иванов и партнёры»"),
                    ("PERSON", "Иванов И.И."),
                    ("PERSON", "Иванов"),
                ],
            ),
            (
                "ru_kedr",
                "ООО «Кедр» (далее — «Кедр») поставило товар. Кедров подписал акт.",
                &[("ООО «Кедр»", "organization", 0), ("Кедров", "person", 0)],
                &[("ORG", "ООО «Кедр»"), ("PERSON", "Кедров")],
            ),
            (
                "bank_surname",
                "Melissa Bank testified for the defence. Bank said she saw it. Melissa left early.",
                &[
                    ("Melissa Bank", "person", 0),
                    ("Bank", "person", 1),
                    ("Melissa", "person", 1),
                ],
                &[
                    ("PERSON", "Melissa Bank"),
                    ("PERSON", "Bank"),
                    ("PERSON", "Melissa"),
                ],
            ),
            (
                "pellam_bank_org",
                "Pellam opened an account. Pellam bank statements show deposits. Pellam testified.",
                &[
                    ("Pellam", "person", 0),
                    ("Pellam", "organization", 1),
                    ("Pellam", "person", 2),
                ],
                &[
                    ("PERSON", "Pellam"),
                    ("PERSON", "Pellam"),
                    ("PERSON", "Pellam"),
                ],
            ),
            (
                "pellam_bank_person_span",
                "Pellam testified. The Pellam bank statements were produced. Pellam signed.",
                &[
                    ("Pellam", "person", 0),
                    ("Pellam bank", "person", 0),
                    ("Pellam", "person", 2),
                ],
                &[
                    ("PERSON", "Pellam"),
                    ("PERSON", "Pellam bank"),
                    ("PERSON", "Pellam"),
                ],
            ),
            (
                "ms_pellam_bank",
                "Ms Ada Pellam bank details were filed. Pellam testified.",
                &[("Ada Pellam bank", "person", 0), ("Pellam", "person", 1)],
                &[("PERSON", "Ada Pellam bank"), ("PERSON", "Pellam")],
            ),
            (
                "dr_lee_medicine",
                "Dr Lee medicine was prescribed. Lee medicine again. Lee testified.",
                &[
                    ("Lee medicine", "person", 0),
                    ("Lee medicine", "person", 1),
                    ("Lee", "person", 2),
                ],
                &[
                    ("PERSON", "Lee medicine"),
                    ("PERSON", "Lee medicine"),
                    ("PERSON", "Lee"),
                ],
            ),
            (
                "org_school_of_medicine",
                "She studied at Pellam College of medicine and Ostrand School of medicine.",
                &[
                    ("Pellam College of medicine", "organization", 0),
                    ("Ostrand School of medicine", "organization", 0),
                ],
                &[
                    ("ORG", "Pellam College of medicine"),
                    ("ORG", "Ostrand School of medicine"),
                ],
            ),
            (
                "org_savings_bank",
                "Funds sat in the Pellam savings bank and the Ostrand National bank.",
                &[
                    ("Pellam savings bank", "organization", 0),
                    ("Ostrand National bank", "organization", 0),
                ],
                &[("ORG", "Ostrand National bank")],
            ),
            (
                "team_person",
                "Our captain, the team known as the Ada Pellam fan club, met Ada Pellam.",
                &[("Ada Pellam", "person", 0), ("Ada Pellam", "person", 1)],
                &[("PERSON", "Ada Pellam"), ("PERSON", "Ada Pellam")],
            ),
            (
                "def_person",
                "Pellam Holdings Ltd and Ada Pellam (together “Pellam”) sued. Pellam testified.",
                &[
                    ("Pellam Holdings Ltd", "organization", 0),
                    ("Ada Pellam", "person", 0),
                    ("Pellam", "person", 3),
                ],
                &[
                    ("ORG", "Pellam Holdings Ltd"),
                    ("PERSON", "Ada Pellam"),
                    ("PERSON", "Pellam"),
                    ("PERSON", "Pellam"),
                ],
            ),
            (
                "def_company_shares_word",
                "Ada Pellam, director of Pellam Holdings Ltd (“Pellam”), testified. Pellam's director agreed.",
                &[
                    ("Ada Pellam", "person", 0),
                    ("Pellam Holdings Ltd", "organization", 0),
                    ("Pellam's", "person", 0),
                ],
                &[
                    ("PERSON", "Ada Pellam"),
                    ("ORG", "Pellam Holdings Ltd"),
                    ("PERSON", "Pellam"),
                    ("PERSON", "Pellam's"),
                ],
            ),
            (
                "def_holdings",
                "Quillon Holdings Ltd (“Holdings”) sued. Mrs Holdings testified. Holdings agreed.",
                &[
                    ("Quillon Holdings Ltd", "organization", 0),
                    ("Holdings", "person", 1),
                    ("Holdings", "person", 2),
                ],
                &[
                    ("ORG", "Quillon Holdings Ltd"),
                    ("PERSON", "Holdings"),
                    ("PERSON", "Holdings"),
                    ("PERSON", "Holdings"),
                ],
            ),
            (
                "morgan",
                "Morgan Stanley & Co. LLC advised. Morgan, the claimant, says the advice was wrong. Morgan's loss was large.",
                &[
                    ("Morgan Stanley & Co. LLC", "organization", 0),
                    ("Morgan", "person", 1),
                    ("Morgan's", "person", 0),
                ],
                &[
                    ("ORG", "Morgan Stanley & Co. LLC"),
                    ("PERSON", "Morgan"),
                    ("PERSON", "Morgan's"),
                ],
            ),
            (
                "untitled_pellam_bank",
                "Ada Pellam bank details were filed. Pellam testified. Ada Pellam left.",
                &[
                    ("Ada Pellam bank", "person", 0),
                    ("Pellam", "person", 1),
                    ("Ada Pellam", "person", 1),
                ],
                &[
                    ("PERSON", "Ada Pellam bank"),
                    ("PERSON", "Pellam"),
                    ("PERSON", "Ada Pellam"),
                ],
            ),
            (
                "frost_bank_frost_person_only",
                "Frost Bank sued the guarantor. Frost, the guarantor, testified that Frost never signed.",
                &[
                    ("Frost Bank", "organization", 0),
                    ("Frost", "person", 1),
                    ("Frost", "person", 2),
                ],
                &[("ORG", "Frost Bank"), ("ORG", "Frost"), ("ORG", "Frost")],
            ),
            (
                "jlewis",
                "John Lewis plc employs the claimant. John told the tribunal he was bullied. John Lewis, his manager, denied it.",
                &[
                    ("John Lewis plc", "organization", 0),
                    ("John", "person", 1),
                    ("John Lewis", "person", 1),
                ],
                &[
                    ("ORG", "John Lewis plc"),
                    ("PERSON", "John"),
                    ("PERSON", "John Lewis"),
                ],
            ),
            (
                "airline_person",
                "Ada Pellam airlines tickets were refunded. Pellam testified.",
                &[
                    ("Ada Pellam airlines", "person", 0),
                    ("Pellam", "person", 1),
                ],
                &[("PERSON", "Ada Pellam airlines"), ("PERSON", "Pellam")],
            ),
            (
                "bank_titled_later",
                "Melissa Bank testified for the defence. Ms Bank said she saw it.",
                &[("Melissa Bank", "person", 0), ("Bank", "person", 1)],
                &[("PERSON", "Melissa Bank"), ("PERSON", "Bank")],
            ),
            (
                "bank_model_org",
                "Melissa Bank testified for the defence. Bank said she saw it.",
                &[("Melissa Bank", "organization", 0), ("Bank", "person", 1)],
                &[("ORG", "Melissa Bank")],
            ),
            (
                "bank_titled_first",
                "Ms Melissa Bank testified for the defence. Bank said she saw it.",
                &[("Melissa Bank", "person", 0), ("Bank", "person", 1)],
                &[("PERSON", "Melissa Bank"), ("PERSON", "Bank")],
            ),
            (
                "pellam_bank_bare",
                "Pellam Bank sued. Pellam, its manager, testified. Bank records were produced.",
                &[("Pellam Bank", "organization", 0), ("Pellam", "person", 1)],
                &[("ORG", "Pellam Bank"), ("ORG", "Pellam")],
            ),
            (
                "medicine_surname",
                "Ada Medicine testified. Medicine said she saw it.",
                &[("Ada Medicine", "person", 0), ("Medicine", "person", 1)],
                &[("PERSON", "Ada Medicine"), ("PERSON", "Medicine")],
            ),
            (
                "marks",
                "Marks and Spencer plc dismissed the claimant. Marks, her line manager, gave evidence.",
                &[
                    ("Marks and Spencer plc", "organization", 0),
                    ("Marks", "person", 1),
                ],
                &[("ORG", "Marks and Spencer plc"), ("PERSON", "Marks")],
            ),
            (
                "ru_ip_full_after",
                "ИП Иванов заключил договор. Иванов Иван Иванович подписал. Иванов обязуется.",
                &[
                    ("ИП Иванов", "organization", 0),
                    ("Иванов Иван Иванович", "person", 0),
                    ("Иванов", "person", 2),
                ],
                &[
                    ("ORG", "ИП Иванов"),
                    ("PERSON", "Иванов Иван Иванович"),
                    ("PERSON", "Иванов"),
                ],
            ),
        ];
        for (name, source, model, expected) in cases {
            assert_eq!(shown_for(source, model), pairs(expected), "{name}");
        }
    }

    /// #548 P3-1: only a capitalized name right before a lowercase airline
    /// noun is cut from it.
    #[test]
    fn only_a_name_before_a_lowercase_airline_noun_comes_off() {
        fn name(text: &str) -> Option<&str> {
            rules::pack().org_noun_name(text).map(|end| &text[..end])
        }
        assert_eq!(name("Pellam airlines"), Some("Pellam"));
        assert_eq!(name("Pellam air fares"), Some("Pellam"));
        assert_eq!(name("Pellam Airlines"), None);
        assert_eq!(name("Pellam College of airlines"), None);
        assert_eq!(name("Pellam national airlines"), None);
        assert_eq!(name("Pellam bank"), None);
        assert_eq!(name("Pellam College of medicine"), None);
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

    /// #412: an email address is the whole address, whatever its local part
    /// holds: `_` (in an autolink's brackets too, without them), a leading
    /// `_` that opens no emphasis, quotes and an apostrophe, with or without
    /// the model's span. Prose apostrophes and quotes around the address,
    /// emphasis around it, a mention and a bare `name@` stay out.
    #[test]
    fn email_local_parts_keep_underscores_quotes_and_apostrophes() {
        let emails = |source: &str, spans: &[(&str, &str)]| -> Vec<String> {
            shown_with(source, spans)
                .into_iter()
                .filter(|(_, category, _)| category == "EMAIL")
                .map(|(_, _, text)| text)
                .collect()
        };
        let underscores =
            "Write to quill_and_ink_77@post-box.invalid or <tam_roe@fen-mail.invalid>.";
        for spans in [
            &[][..],
            &[
                ("quill_and_ink_77@post-box.invalid", "email"),
                ("<tam_roe@fen-mail.invalid>", "email"),
            ],
        ] {
            assert_eq!(
                emails(underscores, spans),
                [
                    "quill_and_ink_77@post-box.invalid",
                    "tam_roe@fen-mail.invalid"
                ]
            );
        }
        let quoted = r#"Write to "rhea desk"@ripple-spur.invalid today."#;
        let apostrophe = "O'Venn's email is <eira.o'venn@copper-skein.invalid> now.";
        let leading = "Адрес: _roman@kadry.invalid.";
        for (source, spans, expected) in [
            (quoted, &[][..], r#""rhea desk"@ripple-spur.invalid"#),
            (
                quoted,
                &[("rhea desk", "person"), ("ripple-spur.invalid", "email")],
                r#""rhea desk"@ripple-spur.invalid"#,
            ),
            (apostrophe, &[], "eira.o'venn@copper-skein.invalid"),
            (
                apostrophe,
                &[("venn@copper-skein.invalid", "email")],
                "eira.o'venn@copper-skein.invalid",
            ),
            (leading, &[], "_roman@kadry.invalid"),
            (
                leading,
                &[("roman@kadry.invalid", "email")],
                "_roman@kadry.invalid",
            ),
            (r#"Mail "a_b@host.invalid" now."#, &[], "a_b@host.invalid"),
            (
                "Write to _quill@host.invalid_ now.",
                &[],
                "quill@host.invalid",
            ),
            (
                "Write to __quill@host.invalid__ now.",
                &[],
                "quill@host.invalid",
            ),
            (
                "Some _emph_ then x_y@host.invalid.",
                &[],
                "x_y@host.invalid",
            ),
        ] {
            assert_eq!(emails(source, spans), [expected], "{source}");
        }
        assert_eq!(
            emails("Ping @tam_roe or tam_roe@ today.", &[]),
            Vec::<String>::new()
        );
    }

    /// #746: an escaped `_` (as the importer writes one) is the local
    /// part's, its `\` too, with or without the model's span; so is
    /// emphasis wholly inside the address, whose rendered text it is. In
    /// code and raw HTML the whole run is the address, as a plain one is.
    /// Emphasis around an address, `\_` in prose or a web address, an
    /// escaped `\` and a mention stay as they were.
    #[test]
    fn escaped_and_emphasis_underscores_keep_the_whole_address() {
        let emails = |source: &str, spans: &[(&str, &str)]| -> Vec<String> {
            shown_with(source, spans)
                .into_iter()
                .filter(|(_, category, _)| category == "EMAIL")
                .map(|(_, _, text)| text)
                .collect()
        };
        let escaped = r"Write to snake\_case@kadry.invalid or \_roman@kadry.invalid.";
        for spans in [
            &[][..],
            &[
                ("case@kadry.invalid", "email"),
                ("roman@kadry.invalid", "email"),
            ],
            &[
                (r"_case@kadry.invalid", "email"),
                (r"_roman@kadry.invalid", "email"),
            ],
        ] {
            assert_eq!(
                emails(escaped, spans),
                [r"snake\_case@kadry.invalid", r"\_roman@kadry.invalid"],
                "{spans:?}"
            );
        }
        for (source, spans, expected) in [
            (
                r"- \_first\_last@kadry.invalid",
                &[][..],
                &[r"\_first\_last@kadry.invalid"][..],
            ),
            (
                "Mail __init__@kadry.invalid.",
                &[],
                &["__init__@kadry.invalid"],
            ),
            (
                "Mail __init__@kadry.invalid.",
                &[("init__@kadry.invalid", "email")],
                &["__init__@kadry.invalid"],
            ),
            ("Mail _a_@kadry.invalid.", &[], &["_a_@kadry.invalid"]),
            ("Write to _x@y.invalid_ now.", &[], &["x@y.invalid"]),
            (
                r"An escaped \\_roman@kadry.invalid.",
                &[],
                &["_roman@kadry.invalid"],
            ),
            (r"Use snake\_case names.", &[], &[]),
            (r"See https://wiki.invalid/snake\_case/page.", &[], &[]),
            (r"Ping @snake\_case today.", &[], &[]),
            (
                r"Copy `snake\_case@kadry.invalid` here.",
                &[],
                &[r"snake\_case@kadry.invalid"],
            ),
            (
                r"Copy `snake\_case@kadry.invalid` here.",
                &[("case@kadry.invalid", "email")],
                &[r"snake\_case@kadry.invalid"],
            ),
            (
                "```\nsnake\\_case@kadry.invalid\n```",
                &[],
                &[r"snake\_case@kadry.invalid"],
            ),
            (
                "<!-- snake\\_case@kadry.invalid -->",
                &[],
                &[r"snake\_case@kadry.invalid"],
            ),
            (
                "```\ncode\n\n```\n\\_roman@kadry.invalid now",
                &[],
                &[r"\_roman@kadry.invalid"],
            ),
        ] {
            assert_eq!(emails(source, spans), expected, "{source}");
        }
        // An escaped `_` and a plain one are one address.
        let both = r"Write \_roman@kadry.invalid or [mail](mailto:_roman@kadry.invalid).";
        let mut review = mdoc_pii::Review::default();
        review.ingest(both, correct(both, &[])).unwrap();
        let aliases: Vec<_> = review
            .candidates()
            .iter()
            .map(|c| {
                let variant = review.variant(c.variant).unwrap();
                (&both[c.range.clone()], variant.replacement.clone())
            })
            .collect();
        assert_eq!(
            aliases,
            [
                (r"\_roman@kadry.invalid", "EMAIL_1".to_owned()),
                ("_roman@kadry.invalid", "EMAIL_1".to_owned())
            ]
        );
    }
}
