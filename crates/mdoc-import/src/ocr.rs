//! Explicit, app-managed OCR setup. Only `install` is allowed to use the network.
pub mod manifest;
use std::{
    fs::{self, File},
    io::Read,
    path::{Path, PathBuf},
    sync::Arc,
};

use crate::recognition::{ENGLISH, Language, RUSSIAN};
use mdoc_store::settings::OcrModel;
use pdf_inspector::vision::{
    ModelArtifactKind, ModelDownloadPolicy, ModelStore, ModelStoreError, OarOcrEngine, OcrMode,
    OcrOptions, OcrPdfOptions, PdfiumRenderer,
};
use sha2::{Digest, Sha256};

/// Why OCR setup, its checks or the shared ONNX Runtime failed.
#[derive(Clone, Debug, thiserror::Error)]
pub enum SetupError {
    #[error(transparent)]
    Permit(#[from] crate::model_work::PermitError),
    #[error(transparent)]
    Download(#[from] crate::model_download::DownloadError),
    #[error("Could not find local application storage for OCR.")]
    NoStorage,
    #[error("Local inference supports Apple Silicon macOS and Windows x64.")]
    InferenceUnsupported,
    #[error("Local OCR supports Apple Silicon macOS and Windows x64.")]
    OcrUnsupported,
    /// pdf-inspector's model store, in its words.
    #[error("OCR models are missing or damaged: {0}")]
    Models(Arc<ModelStoreError>),
    #[error("OCR detector is missing.")]
    NoDetector,
    /// PDFium or the OCR engine failed to load, in its words.
    #[error("{0}")]
    Engine(String),
    #[error("Could not load the ONNX Runtime: {0}. Restart mdoc, then retry.")]
    RuntimeInit(String),
    #[error(
        "Could not load the ONNX Runtime: {0}. Install or repair Microsoft Visual C++ Redistributable (x64) from https://aka.ms/vc14/vc_redist.x64.exe, restart mdoc, then retry setup."
    )]
    RuntimeOpenWindows(String),
    #[error("Could not load the ONNX Runtime: {0}")]
    RuntimeOpen(String),
    #[error("{} is not an ONNX Runtime library: {error}. Retry setup.", path.display())]
    NotRuntime { path: PathBuf, error: String },
    #[error("{} failed its integrity check. Retry OCR setup.", .0.display())]
    Integrity(PathBuf),
    #[error("{}: {source}", path.display())]
    Open {
        path: PathBuf,
        source: Arc<std::io::Error>,
    },
    #[error("{0}")]
    Io(Arc<std::io::Error>),
    #[error("{0}")]
    Archive(Arc<zip::result::ZipError>),
    #[error("{0}")]
    Persist(Arc<tempfile::PersistError>),
}
impl SetupError {
    /// Only absent or wrong-size files mean setup has bytes left to download.
    pub fn is_missing(&self) -> bool {
        match self {
            Self::Models(error) => {
                matches!(
                    error.as_ref(),
                    ModelStoreError::MissingArtifact { .. } | ModelStoreError::SizeMismatch { .. }
                ) || matches!(error.as_ref(), ModelStoreError::Io { source, .. }
                    if source.kind() == std::io::ErrorKind::NotFound)
            }
            Self::Open { source, .. } | Self::Io(source) => {
                source.kind() == std::io::ErrorKind::NotFound
            }
            Self::Download(error) => error.is_missing(),
            _ => false,
        }
    }
}
impl From<std::io::Error> for SetupError {
    fn from(error: std::io::Error) -> Self {
        Self::Io(Arc::new(error))
    }
}
impl From<zip::result::ZipError> for SetupError {
    fn from(error: zip::result::ZipError) -> Self {
        Self::Archive(Arc::new(error))
    }
}
impl From<tempfile::PersistError> for SetupError {
    fn from(error: tempfile::PersistError) -> Self {
        Self::Persist(Arc::new(error))
    }
}

// Other desktop targets remain buildable; qualify their actual runtimes before enabling.
pub const SUPPORTED: bool = cfg!(any(
    all(target_os = "macos", target_arch = "aarch64"),
    all(target_os = "windows", target_arch = "x86_64")
));

#[derive(Clone, Debug)]
pub struct Installed {
    pub config: mdoc_store::settings::OcrConfig,
    pub models: PathBuf,
    pub pdfium: PathBuf,
    pub onnx: PathBuf,
}

impl Installed {
    pub fn options(&self) -> OcrPdfOptions {
        OcrPdfOptions {
            model_manifest: manifest::for_model(self.config.model),
            pdfium_library: Some(self.pdfium.clone()),
            onnx_runtime_library: Some(self.onnx.clone()),
            ocr: OcrOptions::new()
                .mode(OcrMode::Force)
                .minimum_confidence(self.config.minimum_confidence)
                .model_directory(&self.models)
                .model_downloads(ModelDownloadPolicy::Offline),
            render: pdf_inspector::vision::RenderOptions::new().dpi(self.config.dpi as f32),
            ..OcrPdfOptions::default()
        }
    }
}

/// Plain-language row title for a bundle: the languages it reads,
/// "English & Russian".
pub fn title(model: OcrModel) -> String {
    crate::recognition::label(languages(model))
}
/// The recognition languages the bundle reads (ADR 0065).
pub fn languages(model: OcrModel) -> &'static [Language] {
    match model {
        OcrModel::V6Small => &[ENGLISH],
        OcrModel::Cyrillic => &[ENGLISH, RUSSIAN],
    }
}
/// The recognition-language lists PP-OCR offers, one per bundle,
/// recommended first: English; English & Russian.
pub fn offered() -> Vec<&'static [Language]> {
    OcrModel::ALL.map(languages).to_vec()
}
/// PP-OCR's reading of a recognition-language list: the first bundle
/// that reads all of it, else the one reading most, and the languages
/// that bundle cannot read.
pub fn plan(wanted: &[Language]) -> (OcrModel, Vec<Language>) {
    let list = crate::recognition::closest(wanted, &offered());
    let model = OcrModel::ALL
        .into_iter()
        .find(|m| languages(*m) == list.as_slice())
        .unwrap_or_default();
    let unreadable = wanted
        .iter()
        .copied()
        .filter(|l| !languages(model).contains(l))
        .collect();
    (model, unreadable)
}
/// Attribution for exactly the files setup offers.
pub fn source_and_license(model: OcrModel) -> (String, String) {
    let manifest = manifest::for_model(model);
    (
        format!("GitHub · {}", manifest.repository),
        manifest.license.into(),
    )
}

struct Runtime {
    name: &'static str,
    url: &'static str,
    size: u64,
    archive_sha: &'static str,
    library: &'static str,
    archive_library: &'static str,
    library_sha: &'static str,
}

#[cfg(any(test, not(all(target_os = "windows", target_arch = "x86_64"))))]
const MAC_RUNTIMES: &[Runtime] = &[
    Runtime {
        name: "pdfium",
        url: "https://github.com/firecrawl/pdfium-rs/releases/download/native-v7988/firecrawl-pdfium-mac-arm64.tgz",
        size: 3_460_583,
        archive_sha: "4168356c2e62ad5e79553e2e9162f5c99949759d90cb83876a50311f0c32b9b3",
        library: "libpdfium.dylib",
        archive_library: "lib/libpdfium.dylib",
        library_sha: "fbdec47c3f2eaa80705ed25cf8bed5ac420998ba0f3e786d4d297b6238749064",
    },
    Runtime {
        name: "onnx",
        url: "https://github.com/microsoft/onnxruntime/releases/download/v1.27.0/onnxruntime-osx-arm64-1.27.0.tgz",
        size: 32_485_368,
        archive_sha: "545e81c58152353acb0d1e8bd6ce4b62f830c0961f5b3acfedc790ffd76e477a",
        library: "libonnxruntime.1.27.0.dylib",
        archive_library: "lib/libonnxruntime.1.27.0.dylib",
        library_sha: "299e5a2c6ea00531ecd6bf3217e23798c1fcba1698bb386d313c8e16d7317d60",
    },
];

#[cfg(any(test, all(target_os = "windows", target_arch = "x86_64")))]
const WINDOWS_RUNTIMES: &[Runtime] = &[
    Runtime {
        name: "pdfium",
        url: "https://github.com/firecrawl/pdfium-rs/releases/download/native-v7988/firecrawl-pdfium-win-x64.tgz",
        size: 3_764_191,
        archive_sha: "6f398552d8021a89078f64466557251a204999177287b876be49877eb8750d50",
        library: "pdfium.dll",
        archive_library: "bin/pdfium.dll",
        library_sha: "03cc8de22238ea9ffbbf41703f8ef8aae77faeab735583815481f5c2c70a63c7",
    },
    Runtime {
        name: "onnx",
        url: "https://github.com/microsoft/onnxruntime/releases/download/v1.27.0/onnxruntime-win-x64-1.27.0.zip",
        size: 77_086_915,
        archive_sha: "c5c81710938e68079ff1a192b04897faabe4b43830d48f39f27ecd4e16138bfc",
        library: "onnxruntime.dll",
        archive_library: "lib/onnxruntime.dll",
        library_sha: "fd6dd0a8b1f5562d642abdcbd36bc54251482d2ebaa3f4f88669bfdad92e7525",
    },
];

#[cfg(all(target_os = "windows", target_arch = "x86_64"))]
const RUNTIMES: &[Runtime] = WINDOWS_RUNTIMES;
#[cfg(not(all(target_os = "windows", target_arch = "x86_64")))]
const RUNTIMES: &[Runtime] = MAC_RUNTIMES;

/// Bytes OCR setup still needs for `model` under `root` (the OCR root, see
/// [`root`]): missing model files plus any runtime archive not yet installed.
/// Without a root, everything is still to download.
pub fn pending(
    root: Option<&Path>,
    model: mdoc_store::settings::OcrModel,
) -> crate::model_download::Pending {
    let model_root = root.map(|root| model_root(root, model));
    let artifacts = manifest::for_model(model).artifacts;
    let missing: Vec<u64> = artifacts
        .iter()
        .filter(|a| {
            model_root
                .as_ref()
                .is_none_or(|dir| crate::model_download::missing(&dir.join(a.filename), a.size))
        })
        .map(|a| a.size)
        .collect();
    crate::model_download::Pending {
        full: artifacts.iter().map(|a| a.size).sum::<u64>()
            + RUNTIMES.iter().map(|r| r.size).sum::<u64>(),
        model: missing.iter().sum(),
        runtime: RUNTIMES
            .iter()
            .filter(|r| root.is_none_or(|root| !root.join(r.library).exists()))
            .map(|r| r.size)
            .sum(),
    }
}
/// Bytes of the shared ONNX Runtime archive.
pub fn onnx_runtime_size() -> u64 {
    RUNTIMES[1].size
}
/// Bytes of the shared ONNX Runtime archive when it is not installed under
/// `root` (the OCR root) yet.
pub fn onnx_runtime_pending(root: Option<&Path>) -> u64 {
    let runtime = &RUNTIMES[1];
    match root {
        Some(root) if root.join(runtime.library).exists() => 0,
        _ => runtime.size,
    }
}

pub fn root() -> Result<PathBuf, SetupError> {
    mdoc_store::app_paths::data_local_dir()
        .map(|path| path.join("mdoc/ocr/v1"))
        .ok_or(SetupError::NoStorage)
}

/// Exactly the model and runtime files the reserved check verifies. Metadata
/// callers use this list without reading file contents or loading an engine.
pub fn model_files(root: &Path, model: mdoc_store::settings::OcrModel) -> Vec<PathBuf> {
    let model_root = model_root(root, model);
    manifest::for_model(model)
        .artifacts
        .iter()
        .map(|a| model_root.join(a.filename))
        .chain(RUNTIMES.iter().map(|r| root.join(r.library)))
        .collect()
}

/// Shared runtime required by the detection model's check.
pub fn onnx_runtime_path(root: &Path) -> PathBuf {
    root.join(RUNTIMES[1].library)
}

/// No downloads, including when files are missing or invalid.
pub fn check_config(
    config: &mdoc_store::settings::OcrConfig,
) -> Result<Option<Installed>, SetupError> {
    if !SUPPORTED || cfg!(test) {
        return Ok(None);
    }
    let _permit = crate::model_work::Permit::acquire_for("checking text recognition")?;
    check_reserved(config)
}

/// The caller owns model admission, including for the complete catalog check.
pub fn check_reserved(
    config: &mdoc_store::settings::OcrConfig,
) -> Result<Option<Installed>, SetupError> {
    if !SUPPORTED || cfg!(test) {
        return Ok(None);
    }
    check_reserved_in(&root()?, config)
}

/// The real reserved check with an explicit profile root, also used by isolated tests.
pub fn check_reserved_in(
    root: &Path,
    config: &mdoc_store::settings::OcrConfig,
) -> Result<Option<Installed>, SetupError> {
    let model = model_root(root, config.model);
    match fs::metadata(&model) {
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(None),
        Err(error) => {
            return Err(SetupError::Open {
                path: model,
                source: Arc::new(error),
            });
        }
        Ok(_) => {}
    }
    match validate_config(root, config) {
        Ok(installed) => Ok(Some(installed)),
        Err(error) if error.is_missing() => Ok(None),
        Err(error) => Err(error),
    }
}

pub fn model_root(root: &Path, model: mdoc_store::settings::OcrModel) -> PathBuf {
    let manifest = manifest::for_model(model);
    root.join("models")
        .join(manifest.id)
        .join(manifest.revision)
}

#[cfg(test)]
fn check_in(root: &Path) -> Result<Option<Installed>, SetupError> {
    // Pseudonymization may install only the shared ONNX library. That is not
    // an incomplete OCR installation: OCR still needs its own explicit setup.
    let has_ocr = root.join(RUNTIMES[0].library).exists() || root.join("models").exists();
    if !root.exists() || !has_ocr {
        return Ok(None);
    }
    validate(root).map(Some)
}

/// The English/Russian qualification bundle, independent of the app default.
#[cfg(test)]
fn qualification_config() -> mdoc_store::settings::OcrConfig {
    mdoc_store::settings::OcrConfig {
        model: mdoc_store::settings::OcrModel::Cyrillic,
        ..Default::default()
    }
}

#[cfg(test)]
fn validate(root: &Path) -> Result<Installed, SetupError> {
    validate_config(root, &qualification_config())
}

fn validate_config(
    root: &Path,
    config: &mdoc_store::settings::OcrConfig,
) -> Result<Installed, SetupError> {
    for runtime in RUNTIMES {
        verify(&root.join(runtime.library), runtime.library_sha)?;
    }
    let models = ModelStore::new(root.join("models"))
        .resolve(manifest::for_model(config.model))
        .map_err(|e| SetupError::Models(Arc::new(e)))?;
    let installed = Installed {
        config: config.clone(),
        models: models
            .get(ModelArtifactKind::TextDetection)
            .and_then(Path::parent)
            .ok_or(SetupError::NoDetector)?
            .to_path_buf(),
        pdfium: root.join(RUNTIMES[0].library),
        onnx: root.join(RUNTIMES[1].library),
    };
    // Check actual loading before claiming ready. All of this runs off the UI thread.
    PdfiumRenderer::load_from_path(&installed.pdfium)
        .map_err(|e| SetupError::Engine(e.to_string()))?;
    load_onnx_runtime(&installed.onnx)?;
    OarOcrEngine::from_models_with_runtime(&models, Some(&installed.onnx))
        .map_err(|e| SetupError::Engine(e.to_string()))?;
    Ok(installed)
}

/// The outcome of the one `ort::init_from` this process makes.
static ORT_LOADED: std::sync::Mutex<Option<Result<(), SetupError>>> = std::sync::Mutex::new(None);

/// Load the shared ONNX Runtime into ort. Call before any ort, gliner2 or OCR
/// engine use, and stop on `Err`.
///
/// ort rc.13 marks its library slot loaded even when `init_from` fails, so a
/// second `init_from` returns `Ok` and the next session panics on a null
/// handle, which aborts the release build. A library that cannot be opened
/// (missing, damaged, refused by library validation) therefore never reaches
/// ort and stays retryable; a failure inside ort lasts until restart.
pub fn load_onnx_runtime(path: &Path) -> Result<(), SetupError> {
    let mut loaded = ORT_LOADED.lock().unwrap_or_else(|e| e.into_inner());
    if let Some(result) = loaded.as_ref() {
        return result.clone();
    }
    probe_onnx_runtime(path)?;
    let result = ort::init_from(path)
        .map(|builder| {
            builder.with_name("mdoc").commit();
        })
        .map_err(|e| SetupError::RuntimeInit(e.to_string()));
    *loaded = Some(result.clone());
    result
}

fn probe_onnx_runtime(path: &Path) -> Result<(), SetupError> {
    // SAFETY: the pinned, digest-verified runtime; loading it runs its initializers.
    let library = unsafe { libloading::Library::new(path) }.map_err(|e| {
        if cfg!(target_os = "windows") {
            SetupError::RuntimeOpenWindows(e.to_string())
        } else {
            SetupError::RuntimeOpen(e.to_string())
        }
    })?;
    // SAFETY: only the symbol's presence is checked; it is never called here.
    unsafe { library.get::<unsafe extern "C" fn()>(b"OrtGetApiBase") }.map_err(|e| {
        SetupError::NotRuntime {
            path: path.to_path_buf(),
            error: e.to_string(),
        }
    })?;
    Ok(())
}

/// Called by a worker that already owns model admission.
pub fn installed_config(config: &mdoc_store::settings::OcrConfig) -> Result<Installed, SetupError> {
    validate_config(&root()?, config)
}

/// Share the pinned native runtime without installing OCR models/PDFium.
/// The pseudonymization model has its own setup and never invokes OCR setup.
pub fn onnx_runtime(install: bool) -> Result<PathBuf, SetupError> {
    if !SUPPORTED {
        return Err(SetupError::InferenceUnsupported);
    }
    let root = root()?;
    let runtime = &RUNTIMES[1];
    let path = root.join(runtime.library);
    if verify(&path, runtime.library_sha).is_err() && install {
        fs::create_dir_all(&root)?;
        install_runtime(&root, runtime)?;
    }
    verify(&path, runtime.library_sha)?;
    Ok(path)
}

/// Verify the shared runtime without downloading or consulting platform paths.
pub fn onnx_runtime_in(root: &Path) -> Result<PathBuf, SetupError> {
    let runtime = &RUNTIMES[1];
    let path = root.join(runtime.library);
    verify(&path, runtime.library_sha)?;
    Ok(path)
}

pub fn onnx_runtime_with_progress(
    progress: &crate::model_download::Progress,
) -> Result<PathBuf, SetupError> {
    let root = root()?;
    fs::create_dir_all(&root)?;
    let runtime = &RUNTIMES[1];
    let path = root.join(runtime.library);
    if verify(&path, runtime.library_sha).is_err() {
        install_runtime_progress(&root, runtime, progress)?;
    }
    verify(&path, runtime.library_sha)?;
    Ok(path)
}

/// Only the qualification smoke test installs, on the supported targets.
#[cfg(all(
    test,
    any(
        all(target_os = "macos", target_arch = "aarch64"),
        all(target_os = "windows", target_arch = "x86_64")
    )
))]
fn install_in(root: &Path) -> Result<Installed, SetupError> {
    install_config_in(
        root,
        &qualification_config(),
        &crate::model_download::Progress::default(),
    )
}

pub fn install_config(
    config: &mdoc_store::settings::OcrConfig,
    progress: &crate::model_download::Progress,
) -> Result<Installed, SetupError> {
    if !SUPPORTED {
        return Err(SetupError::OcrUnsupported);
    }
    let _permit = crate::model_work::Permit::acquire_for("installing text recognition")?;
    install_reserved(config, progress).map(|(installed, _)| installed)
}

/// The caller retains model admission through publishing its setup result.
/// Metadata is captured after downloads, immediately before final validation.
pub fn install_reserved(
    config: &mdoc_store::settings::OcrConfig,
    progress: &crate::model_download::Progress,
) -> Result<(Installed, mdoc_store::model_state::Files), SetupError> {
    if !SUPPORTED {
        return Err(SetupError::OcrUnsupported);
    }
    install_checked_in(&root()?, config, progress)
}
pub fn install_config_in(
    root: &Path,
    config: &mdoc_store::settings::OcrConfig,
    progress: &crate::model_download::Progress,
) -> Result<Installed, SetupError> {
    install_checked_in(root, config, progress).map(|(installed, _)| installed)
}

fn install_checked_in(
    root: &Path,
    config: &mdoc_store::settings::OcrConfig,
    progress: &crate::model_download::Progress,
) -> Result<(Installed, mdoc_store::model_state::Files), SetupError> {
    fs::create_dir_all(root)?;
    for runtime in RUNTIMES {
        progress.check()?;
        if verify(&root.join(runtime.library), runtime.library_sha).is_ok() {
            continue;
        }
        install_runtime_progress(root, runtime, progress)?;
    }
    let manifest = manifest::for_model(config.model);
    let model_root = model_root(root, config.model);
    for artifact in manifest.artifacts {
        crate::model_download::fetch(
            artifact.url,
            artifact.size,
            artifact.sha256,
            &model_root.join(artifact.filename),
            progress,
        )?;
    }
    let notices = model_root.join("licenses");
    fs::create_dir_all(&notices)?;
    fs::write(
        notices.join("LICENSE"),
        include_str!("../../../resources/ocr-model-license.txt"),
    )?;
    fs::write(
        notices.join("NOTICE"),
        format!(
            "{}; {}.\nPaddleOCR / PaddlePaddle authors; unmodified ONNX artifacts from GreatV/oar-ocr.\nhttps://github.com/PaddlePaddle/PaddleOCR\nhttps://github.com/GreatV/oar-ocr\n",
            manifest.id, manifest.revision
        ),
    )?;
    progress.check()?;
    progress.phase("Checking runtime");
    let before = mdoc_store::model_state::Files::read(&model_files(root, config.model));
    let installed = validate_config(root, config)?;
    progress.check()?;
    Ok((installed, before))
}

pub fn remove_model(model: mdoc_store::settings::OcrModel) -> Result<(), SetupError> {
    let _permit = crate::model_work::Permit::acquire_for("removing text recognition")?;
    remove_reserved(model)
}
/// The caller retains model admission through remembering the removal.
pub fn remove_reserved(model: mdoc_store::settings::OcrModel) -> Result<(), SetupError> {
    remove_model_files(&root()?, model)
}
fn remove_model_files(
    root: &Path,
    model: mdoc_store::settings::OcrModel,
) -> Result<(), SetupError> {
    let path = model_root(root, model);
    if path.exists() {
        fs::remove_dir_all(path)?;
    }
    Ok(())
}

fn verify(path: &Path, expected: &str) -> Result<(), SetupError> {
    let mut file = File::open(path).map_err(|e| SetupError::Open {
        path: path.to_path_buf(),
        source: Arc::new(e),
    })?;
    let mut digest = Sha256::new();
    let mut buffer = [0; 64 * 1024];
    loop {
        let count = file.read(&mut buffer)?;
        if count == 0 {
            break;
        }
        digest.update(&buffer[..count]);
    }
    if digest
        .finalize()
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect::<String>()
        != expected
    {
        return Err(SetupError::Integrity(path.to_path_buf()));
    }
    Ok(())
}

fn install_runtime(root: &Path, runtime: &Runtime) -> Result<(), SetupError> {
    install_runtime_progress(root, runtime, &crate::model_download::Progress::default())
}
fn install_runtime_progress(
    root: &Path,
    runtime: &Runtime,
    progress: &crate::model_download::Progress,
) -> Result<(), SetupError> {
    let temporary = tempfile::tempdir_in(root)?;
    let path = temporary.path().join("runtime.archive");
    crate::model_download::fetch(
        runtime.url,
        runtime.size,
        runtime.archive_sha,
        &path,
        progress,
    )?;
    let file = File::open(path)?;
    progress.check()?;
    extract_runtime(root, runtime, &file)?;
    verify(&root.join(runtime.library), runtime.library_sha)
}

fn extract_runtime(root: &Path, runtime: &Runtime, archive: &File) -> Result<(), SetupError> {
    let notices = root.join("licenses").join(runtime.name);
    fs::create_dir_all(&notices)?;
    if runtime.url.ends_with(".zip") {
        let mut zip = zip::ZipArchive::new(archive)?;
        for index in 0..zip.len() {
            let mut entry = zip.by_index(index)?;
            if !entry.is_file() || entry.is_symlink() {
                continue;
            }
            let Some(path) = entry.enclosed_name() else {
                continue;
            };
            copy_runtime_entry(root, runtime, &path, entry.size(), &mut entry)?;
        }
        return Ok(());
    }
    let mut tar = tar::Archive::new(flate2::read::GzDecoder::new(archive));
    for entry in tar.entries()? {
        let mut entry = entry?;
        if !entry.header().entry_type().is_file() {
            continue;
        }
        let path = entry.path()?.into_owned();
        copy_runtime_entry(root, runtime, &path, entry.size(), &mut entry)?;
    }
    Ok(())
}

fn copy_runtime_entry(
    root: &Path,
    runtime: &Runtime,
    path: &Path,
    size: u64,
    entry: &mut impl Read,
) -> Result<(), SetupError> {
    let notices = root.join("licenses").join(runtime.name);
    let Some(name) = path.file_name().and_then(|n| n.to_str()) else {
        return Ok(());
    };
    let destination = if path.ends_with(runtime.archive_library) && size < 100_000_000 {
        root.join(runtime.library)
    } else if (name == "LICENSE"
        || name == "ThirdPartyNotices.txt"
        || path.components().any(|c| c.as_os_str() == "licenses"))
        && size < 2_000_000
    {
        notices.join(name)
    } else {
        return Ok(());
    };
    // Never unpack paths from the archive. Copy only selected regular files
    // into our own flat destinations, then atomically replace complete files.
    let mut file = tempfile::NamedTempFile::new_in(destination.parent().unwrap())?;
    std::io::copy(entry, &mut file)?;
    file.as_file().sync_all()?;
    file.persist(destination)?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use mdoc_store::settings::OcrModel;

    #[test]
    fn removing_a_bundle_preserves_other_bundles_and_shared_runtimes() {
        use mdoc_store::settings::OcrModel;
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path();
        for model in OcrModel::ALL {
            let path = model_root(root, model);
            fs::create_dir_all(&path).unwrap();
            fs::write(path.join("weights.onnx"), b"pinned fixture").unwrap();
        }
        for runtime in RUNTIMES {
            fs::write(root.join(runtime.library), b"shared fixture").unwrap();
        }
        remove_model_files(root, OcrModel::V6Small).unwrap();
        assert!(!model_root(root, OcrModel::V6Small).exists());
        assert!(
            model_root(root, OcrModel::Cyrillic)
                .join("weights.onnx")
                .exists()
        );
        for runtime in RUNTIMES {
            assert!(root.join(runtime.library).exists());
        }
    }
    use std::io::{Seek, Write};

    #[test]
    fn shared_inference_runtime_does_not_claim_or_break_ocr_setup() {
        let dir = tempfile::tempdir().unwrap();
        fs::write(dir.path().join(RUNTIMES[1].library), b"shared-runtime").unwrap();
        assert!(check_in(dir.path()).unwrap().is_none());
        fs::write(dir.path().join(RUNTIMES[0].library), b"corrupt-pdfium").unwrap();
        assert!(check_in(dir.path()).is_err());
    }

    /// ort rc.13 reported a second `init_from` of an unloadable runtime as
    /// loaded, so the next session panicked and the release build aborted.
    #[test]
    fn unloadable_onnx_runtime_stays_an_error_without_reaching_ort() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join(RUNTIMES[1].library);
        fs::write(&path, b"refused by library validation").unwrap();
        for _ in 0..2 {
            let error = load_onnx_runtime(&path).unwrap_err();
            assert!(
                error
                    .to_string()
                    .starts_with("Could not load the ONNX Runtime"),
                "{error}"
            );
        }
        // A library without the ONNX Runtime entry point is refused the same way.
        #[cfg(target_os = "macos")]
        let other = Path::new("/usr/lib/libz.1.dylib");
        #[cfg(target_os = "windows")]
        let other = Path::new("kernel32.dll");
        #[cfg(any(target_os = "macos", target_os = "windows"))]
        for _ in 0..2 {
            let error = load_onnx_runtime(other).unwrap_err();
            assert!(
                error.to_string().contains("not an ONNX Runtime library"),
                "{error}"
            );
        }
        assert!(ORT_LOADED.lock().unwrap().is_none());
    }

    #[test]
    fn pending_counts_only_missing_model_files_and_runtimes() {
        let dir = tempfile::tempdir().unwrap();
        let model = OcrModel::V6Small;
        let full = pending(None, model);
        assert_eq!(pending(Some(dir.path()), model), full);
        let model_bytes: u64 = manifest::for_model(model)
            .artifacts
            .iter()
            .map(|a| a.size)
            .sum();
        assert_eq!(full.model, model_bytes);
        fs::write(dir.path().join(RUNTIMES[1].library), b"shared-runtime").unwrap();
        assert_eq!(
            pending(Some(dir.path()), model).runtime,
            full.runtime - RUNTIMES[1].size
        );
        // A runtime already here: the card says what is left, not "download".
        assert!(!full.resumed());
        assert!(full.download_label().ends_with(" MB download"));
        let partial = pending(Some(dir.path()), model);
        assert!(partial.resumed());
        assert!(partial.download_label().ends_with(" MB left to download"));
    }

    #[test]
    fn incomplete_or_modified_installations_are_not_ready() {
        let dir = tempfile::tempdir().unwrap();
        assert!(validate(dir.path()).is_err());
        fs::write(dir.path().join(RUNTIMES[0].library), b"corrupt").unwrap();
        assert!(
            validate(dir.path())
                .unwrap_err()
                .to_string()
                .contains("integrity")
        );
    }

    #[test]
    fn tracked_files_match_model_store_and_runtime_verification() {
        let dir = tempfile::tempdir().unwrap();
        for model in OcrModel::ALL {
            let paths = model_files(dir.path(), model);
            let manifest = manifest::for_model(model);
            let expected = manifest
                .artifacts
                .iter()
                .map(|artifact| {
                    dir.path()
                        .join("models")
                        .join(manifest.id)
                        .join(manifest.revision)
                        .join(artifact.filename)
                })
                .chain(
                    RUNTIMES
                        .iter()
                        .map(|runtime| dir.path().join(runtime.library)),
                )
                .collect::<Vec<_>>();
            assert_eq!(paths, expected);
            assert_eq!(paths.len(), manifest.artifacts.len() + 2);
            let error = ModelStore::new(dir.path().join("models"))
                .resolve(manifest)
                .unwrap_err();
            assert!(
                matches!(error, ModelStoreError::MissingArtifact { path, .. } if path == paths[0])
            );
        }
    }

    #[test]
    fn only_missing_and_wrong_size_errors_mean_not_downloaded() {
        let path = PathBuf::from("model.onnx");
        assert!(
            SetupError::Models(Arc::new(ModelStoreError::MissingArtifact {
                path: path.clone(),
                download_url: "https://example.test/model"
            }))
            .is_missing()
        );
        assert!(
            SetupError::Models(Arc::new(ModelStoreError::SizeMismatch {
                path: path.clone(),
                expected: 4,
                actual: 2
            }))
            .is_missing()
        );
        assert!(!SetupError::Integrity(path.clone()).is_missing());
        assert!(!SetupError::RuntimeOpenWindows("VC++ dependency missing".into()).is_missing());
        for kind in [
            std::io::ErrorKind::NotFound,
            std::io::ErrorKind::PermissionDenied,
        ] {
            let error = SetupError::Open {
                path: path.clone(),
                source: Arc::new(std::io::Error::from(kind)),
            };
            assert_eq!(error.is_missing(), kind == std::io::ErrorKind::NotFound);
        }
    }

    #[test]
    fn import_options_never_download_and_select_qualified_model() {
        let installed = Installed {
            config: mdoc_store::settings::OcrConfig::default(),
            models: "models".into(),
            pdfium: "pdfium".into(),
            onnx: "onnx".into(),
        };
        let options = installed.options();
        assert_eq!(options.ocr.model_downloads, ModelDownloadPolicy::Offline);
        assert_eq!(
            options.model_manifest.id,
            pdf_inspector::vision::PP_OCR_V6_SMALL.id
        );
        assert_eq!(options.ocr.model_directory, Some(installed.models));
        assert_eq!(options.pdfium_library, Some(installed.pdfium));
    }

    #[test]
    fn runtime_extraction_ignores_debug_files_and_symlinks() {
        let dir = tempfile::tempdir().unwrap();
        let mut archive = tempfile::tempfile().unwrap();
        {
            let gzip = flate2::write::GzEncoder::new(&mut archive, flate2::Compression::fast());
            let mut tar = tar::Builder::new(gzip);
            for (name, contents) in [
                ("package/lib/libpdfium.dylib", b"runtime".as_slice()),
                ("debug/DWARF/libpdfium.dylib", b"debug".as_slice()),
                ("package/LICENSE", b"notice".as_slice()),
            ] {
                let mut header = tar::Header::new_gnu();
                header.set_size(contents.len() as u64);
                header.set_mode(0o644);
                header.set_cksum();
                tar.append_data(&mut header, name, contents).unwrap();
            }
            let mut link = tar::Header::new_gnu();
            link.set_entry_type(tar::EntryType::Symlink);
            link.set_size(0);
            link.set_mode(0o777);
            tar.append_link(&mut link, "lib/libpdfium.dylib", "/outside")
                .unwrap();
            tar.into_inner().unwrap().finish().unwrap();
        }
        archive.rewind().unwrap();
        extract_runtime(dir.path(), &MAC_RUNTIMES[0], &archive).unwrap();
        assert_eq!(
            fs::read(dir.path().join("libpdfium.dylib")).unwrap(),
            b"runtime"
        );
        assert_eq!(
            fs::read(dir.path().join("licenses/pdfium/LICENSE")).unwrap(),
            b"notice"
        );
        assert!(!dir.path().join("debug").exists());
    }

    #[test]
    fn windows_zip_extracts_only_runtime_and_notices() {
        let dir = tempfile::tempdir().unwrap();
        let mut archive = tempfile::tempfile().unwrap();
        {
            let mut zip = zip::ZipWriter::new(&mut archive);
            let options = zip::write::SimpleFileOptions::default();
            for (name, contents) in [
                ("package/lib/onnxruntime.dll", b"runtime".as_slice()),
                ("package/lib/onnxruntime.pdb", b"debug".as_slice()),
                ("debug/onnxruntime.dll", b"wrong".as_slice()),
                ("package/LICENSE", b"license".as_slice()),
                ("package/ThirdPartyNotices.txt", b"notice".as_slice()),
                ("../lib/onnxruntime.dll", b"traversal".as_slice()),
            ] {
                zip.start_file(name, options).unwrap();
                zip.write_all(contents).unwrap();
            }
            zip.add_symlink("link/lib/onnxruntime.dll", "/outside", options)
                .unwrap();
            zip.finish().unwrap();
        }
        archive.rewind().unwrap();
        extract_runtime(dir.path(), &WINDOWS_RUNTIMES[1], &archive).unwrap();
        assert_eq!(
            fs::read(dir.path().join("onnxruntime.dll")).unwrap(),
            b"runtime"
        );
        assert_eq!(
            fs::read(dir.path().join("licenses/onnx/LICENSE")).unwrap(),
            b"license"
        );
        assert_eq!(
            fs::read(dir.path().join("licenses/onnx/ThirdPartyNotices.txt")).unwrap(),
            b"notice"
        );
        assert_eq!(fs::read_dir(dir.path()).unwrap().count(), 2);
    }

    #[test]
    fn windows_pdfium_uses_bin_directory_and_replaces_damaged_library() {
        let dir = tempfile::tempdir().unwrap();
        fs::create_dir_all(dir.path().join("licenses/pdfium")).unwrap();
        fs::write(dir.path().join("pdfium.dll"), b"damaged").unwrap();
        for (path, mut bytes) in [
            ("bin/pdfium.dll", b"runtime".as_slice()),
            ("lib/pdfium.dll.lib", b"import library".as_slice()),
            ("lib/pdfium.dll", b"wrong directory".as_slice()),
        ] {
            copy_runtime_entry(
                dir.path(),
                &WINDOWS_RUNTIMES[0],
                Path::new(path),
                bytes.len() as u64,
                &mut bytes,
            )
            .unwrap();
        }
        assert_eq!(fs::read(dir.path().join("pdfium.dll")).unwrap(), b"runtime");
        assert_eq!(fs::read_dir(dir.path()).unwrap().count(), 2);
    }

    /// Explicit qualification only: downloads pinned artifacts to throwaway storage.
    #[test]
    #[ignore = "downloads OCR runtimes/models; requires Apple Silicon macOS or Windows x64"]
    #[cfg(any(
        all(target_os = "macos", target_arch = "aarch64"),
        all(target_os = "windows", target_arch = "x86_64")
    ))]
    fn setup_and_offline_english_russian_smoke() {
        let dir = tempfile::tempdir().unwrap();
        assert!(pending(Some(dir.path()), OcrModel::Cyrillic).total() > 0);
        let installed = install_in(dir.path()).unwrap();
        // Second setup verifies a complete installation without network recovery.
        let ready = validate(dir.path()).unwrap();
        assert_eq!(installed.models, ready.models);
        assert_eq!(pending(Some(dir.path()), OcrModel::Cyrillic).total(), 0);
        for language in ["english", "russian"] {
            let base = Path::new(env!("CARGO_MANIFEST_DIR"))
                .join("../../tests/fixtures/ocr-qualification");
            let pdf = base.join(format!("{language}.pdf"));
            let original = fs::read(&pdf).unwrap();
            let imported = crate::import::prepare(&pdf, Some(&installed), false).unwrap();
            let plain = imported
                .markdown
                .lines()
                .map(|line| line.trim_start_matches('#').trim())
                .collect::<Vec<_>>()
                .join(" ");
            let expected = fs::read_to_string(base.join(format!("{language}.txt"))).unwrap();
            assert_eq!(
                plain.split_whitespace().collect::<Vec<_>>(),
                expected.split_whitespace().collect::<Vec<_>>(),
                "{language}"
            );
            assert_eq!(fs::read(pdf).unwrap(), original);
        }
    }
}
