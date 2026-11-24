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

    /// Measurement only, never the app: `pdf` through [`Self::options`] at
    /// this config's resolution (or without OCR when `ocr` is false), timed,
    /// with the raw recognition kept. The outputs are the files
    /// `tools/ocr-upstream/compare.py` scores.
    pub fn measure(&self, pdf: &[u8], ocr: bool) -> Result<Measured, SetupError> {
        let mut options = OcrPdfOptions {
            retain_recognition: true,
            ..self.options()
        };
        if !ocr {
            options.ocr = options.ocr.mode(OcrMode::Off);
        }
        let start = std::time::Instant::now();
        let result = pdf_inspector::vision::process_pdf_with_ocr_mem(pdf, options)
            .map_err(|e| SetupError::Engine(e.to_string()))?;
        let seconds = start.elapsed().as_secs_f64();
        let page = result
            .pages
            .first()
            .ok_or_else(|| SetupError::Engine("The PDF has no pages.".into()))?;
        Ok(Measured {
            recognition: result
                .recognition
                .iter()
                .flat_map(|page| page.spans.iter())
                .map(|span| span.text.as_str())
                .collect::<Vec<_>>()
                .join("\n"),
            metadata: format!(
                "source={:?}\nrouted={:?}\nrecommended={:?}\nwarnings={:?}\nmodel={:?}\n",
                page.provenance.source,
                result.pages_routed_to_ocr,
                result.pages_recommended_for_ocr,
                page.provenance.warnings,
                result.recognition.first().map(|page| &page.model),
            ),
            pages: result.page_count,
            markdown: result.markdown,
            seconds,
        })
    }
}

/// One [`Installed::measure`] run.
#[derive(Clone, Debug)]
pub struct Measured {
    pub markdown: String,
    /// Every recognized span's text, one per line.
    pub recognition: String,
    /// Routing, provenance and model, as the corpus captures record them.
    pub metadata: String,
    pub pages: u32,
    /// Wall time of the conversion, the engine's loading included.
    pub seconds: f64,
}

/// Plain-language row title for a bundle: the languages it reads,
/// "English & Russian".
pub fn title(model: OcrModel) -> String {
    crate::recognition::label(languages(model))
}
/// The recognition languages the bundle reads.
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

/// A native runtime setup installs as `library`, from the first of `sources`
/// that can be downloaded.
struct Runtime {
    name: &'static str,
    library: &'static str,
    library_sha: &'static str,
    /// Names earlier releases installed this runtime as. One left beside a
    /// missing `library` makes setup an update ([`onnx_runtime_superseded`]);
    /// a version bump adds the outgoing name.
    superseded: &'static [&'static str],
    /// Tried in order; a later source only when an earlier one could not be
    /// downloaded (see [`source_unavailable`]), never after a failed check.
    sources: &'static [Source],
}

/// One pinned archive that holds a runtime's library.
struct Source {
    url: &'static str,
    size: u64,
    archive_sha: &'static str,
    archive_library: &'static str,
    /// The library's digest when this source's build differs from the
    /// runtime's `library_sha`.
    library_sha: Option<&'static str>,
}

impl Runtime {
    /// Bytes of the first source's archive, which setup plans for.
    fn size(&self) -> u64 {
        self.sources[0].size
    }
    /// Every digest an installed library may have, one per distinct build.
    fn library_shas(&self) -> impl Iterator<Item = &'static str> + '_ {
        std::iter::once(self.library_sha).chain(self.sources.iter().filter_map(|s| s.library_sha))
    }
}

impl Source {
    fn library_sha(&self, runtime: &Runtime) -> &'static str {
        self.library_sha.unwrap_or(runtime.library_sha)
    }
    fn is_zip(&self) -> bool {
        self.url.ends_with(".zip") || self.url.ends_with(".nupkg")
    }
}

/// NuGet's Microsoft.ML.OnnxRuntime 1.32.0, the only source of ONNX Runtime
/// 1.32.0 that can be pinned (#925). GitHub's release
/// (github.com/microsoft/onnxruntime) has returned 404 since 2026-11-17, so
/// its v1.32.0 assets could not be downloaded or hashed; once they can, they
/// go first in each `sources` list with their own archive and library pins.
/// One package holds every platform's library: 184 MB.
const fn onnx_runtime_nuget(archive_library: &'static str) -> Source {
    Source {
        url: "https://api.nuget.org/v3-flatcontainer/microsoft.ml.onnxruntime/1.32.0/microsoft.ml.onnxruntime.1.32.0.nupkg",
        size: 184_391_411,
        archive_sha: "a9150bc2992cbbb6722070da9910e1ffa989a8bfc67bdd5bb8e745ca755641db",
        archive_library,
        library_sha: None,
    }
}

#[cfg(any(test, not(all(target_os = "windows", target_arch = "x86_64"))))]
const MAC_RUNTIMES: &[Runtime] = &[
    Runtime {
        name: "pdfium",
        library: "libpdfium.dylib",
        library_sha: "fbdec47c3f2eaa80705ed25cf8bed5ac420998ba0f3e786d4d297b6238749064",
        superseded: &[],
        sources: &[Source {
            url: "https://github.com/firecrawl/pdfium-rs/releases/download/native-v7988/firecrawl-pdfium-mac-arm64.tgz",
            size: 3_460_583,
            archive_sha: "4168356c2e62ad5e79553e2e9162f5c99949759d90cb83876a50311f0c32b9b3",
            archive_library: "lib/libpdfium.dylib",
            library_sha: None,
        }],
    },
    Runtime {
        name: "onnx",
        library: "libonnxruntime.1.32.0.dylib",
        // Signed with Microsoft's Developer ID (team UBF8T346G9).
        library_sha: "21ff304871400e846a30c8a1165d15173add56598afb45f98227e2dfc716a497",
        // 0.4.0-rc.3 and earlier.
        superseded: &["libonnxruntime.1.27.0.dylib"],
        sources: &[onnx_runtime_nuget(
            "runtimes/osx-arm64/native/libonnxruntime.dylib",
        )],
    },
];

#[cfg(any(test, all(target_os = "windows", target_arch = "x86_64")))]
const WINDOWS_RUNTIMES: &[Runtime] = &[
    Runtime {
        name: "pdfium",
        library: "pdfium.dll",
        library_sha: "03cc8de22238ea9ffbbf41703f8ef8aae77faeab735583815481f5c2c70a63c7",
        superseded: &[],
        sources: &[Source {
            url: "https://github.com/firecrawl/pdfium-rs/releases/download/native-v7988/firecrawl-pdfium-win-x64.tgz",
            size: 3_764_191,
            archive_sha: "6f398552d8021a89078f64466557251a204999177287b876be49877eb8750d50",
            archive_library: "bin/pdfium.dll",
            library_sha: None,
        }],
    },
    Runtime {
        name: "onnx",
        // Named by version, like the macOS library, so the 1.27.0
        // `onnxruntime.dll` an earlier setup left reads as a runtime still to
        // download (OCR's and pseudonymization's setup cards count it), not
        // as a damaged one. Nothing links to the DLL by name: it is loaded
        // by full path.
        library: "onnxruntime.1.32.0.dll",
        library_sha: "348c3d29d6995eee37e3451825eaddb7ea7400be3afda6ca2286b12bc775f042",
        // 1.27.0, from 0.4.0-rc.3 and earlier.
        superseded: &["onnxruntime.dll"],
        sources: &[onnx_runtime_nuget(
            "runtimes/win-x64/native/onnxruntime.dll",
        )],
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
            + RUNTIMES.iter().map(Runtime::size).sum::<u64>(),
        model: missing.iter().sum(),
        runtime: RUNTIMES
            .iter()
            .filter(|r| root.is_none_or(|root| !root.join(r.library).exists()))
            .map(Runtime::size)
            .sum(),
        replaces_runtime: onnx_runtime_superseded(root),
    }
}
/// Bytes of the shared ONNX Runtime archive.
pub fn onnx_runtime_size() -> u64 {
    RUNTIMES[1].size()
}
/// Bytes of the shared ONNX Runtime archive when it is not installed under
/// `root` (the OCR root) yet.
pub fn onnx_runtime_pending(root: Option<&Path>) -> u64 {
    let runtime = &RUNTIMES[1];
    match root {
        Some(root) if root.join(runtime.library).exists() => 0,
        _ => runtime.size(),
    }
}

/// Whether the shared ONNX Runtime under `root` (the OCR root) is missing
/// while an earlier release's is installed: its download is an update. The
/// pinned library is named by its version, so the earlier one is found by
/// name; nothing is read.
pub fn onnx_runtime_superseded(root: Option<&Path>) -> bool {
    let runtime = &RUNTIMES[1];
    root.is_some_and(|root| {
        !root.join(runtime.library).exists()
            && runtime.superseded.iter().any(|old| root.join(old).exists())
    })
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

/// The PDFium library OCR setup installs under `root`.
pub fn pdfium_path(root: &Path) -> PathBuf {
    root.join(RUNTIMES[0].library)
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
        verify_library(root, runtime)?;
    }
    let (installed, models) = resolve_models(
        root,
        config,
        root.join(RUNTIMES[0].library),
        root.join(RUNTIMES[1].library),
    )?;
    // Check actual loading before claiming ready. All of this runs off the UI thread.
    PdfiumRenderer::load_from_path(&installed.pdfium)
        .map_err(|e| SetupError::Engine(e.to_string()))?;
    load_onnx_runtime(&installed.onnx)?;
    OarOcrEngine::from_models_with_runtime(&models, Some(&installed.onnx))
        .map_err(|e| SetupError::Engine(e.to_string()))?;
    Ok(installed)
}

/// OCR whose models sit under `root/models` in the desktop layout, with the
/// given PDFium and ONNX Runtime libraries: the mobile core passes the
/// runtimes linked into the app ([`linked_runtimes`]). Checks the models
/// against the pinned manifest; nothing is downloaded, and the runtimes load
/// on first recognition.
pub fn installed_at(
    root: &Path,
    config: &mdoc_store::settings::OcrConfig,
    pdfium: PathBuf,
    onnx: PathBuf,
) -> Result<Installed, SetupError> {
    resolve_models(root, config, pdfium, onnx).map(|(installed, _)| installed)
}

fn resolve_models(
    root: &Path,
    config: &mdoc_store::settings::OcrConfig,
    pdfium: PathBuf,
    onnx: PathBuf,
) -> Result<(Installed, pdf_inspector::vision::ModelPaths), SetupError> {
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
        pdfium,
        onnx,
    };
    Ok((installed, models))
}

/// The PDFium and ONNX Runtime that ship inside the iOS app: no
/// runtime is downloaded there.
#[cfg(target_os = "ios")]
#[derive(Clone, Debug)]
pub struct LinkedRuntimes {
    /// The embedded `pdfium.framework` beside the app executable.
    pub pdfium: PathBuf,
    /// The image that holds this code: ONNX Runtime is statically linked
    /// into it, so ort's loader finds `OrtGetApiBase` there. The app must
    /// export that symbol and keep it through stripping.
    pub onnx: PathBuf,
}

#[cfg(target_os = "ios")]
#[allow(unsafe_code)] // dladdr (#897).
pub fn linked_runtimes() -> Result<LinkedRuntimes, SetupError> {
    use std::ffi::{CStr, c_char, c_int, c_void};
    #[repr(C)]
    struct DlInfo {
        fname: *const c_char,
        fbase: *mut c_void,
        sname: *const c_char,
        saddr: *mut c_void,
    }
    unsafe extern "C" {
        fn OrtGetApiBase() -> *const c_void;
        fn dladdr(address: *const c_void, info: *mut DlInfo) -> c_int;
    }
    // Taking the address makes the app's link pull ONNX Runtime in.
    let symbol = std::hint::black_box(OrtGetApiBase as *const c_void);
    let mut info = DlInfo {
        fname: std::ptr::null(),
        fbase: std::ptr::null_mut(),
        sname: std::ptr::null(),
        saddr: std::ptr::null_mut(),
    };
    // SAFETY: `symbol` is a function in a loaded image and `info` is a valid
    // Dl_info; dladdr only writes into it.
    if unsafe { dladdr(symbol, &mut info) } == 0 || info.fname.is_null() {
        return Err(SetupError::RuntimeOpen(
            "ONNX Runtime is not linked into the app".into(),
        ));
    }
    // SAFETY: dladdr returned a NUL-terminated path owned by dyld.
    let onnx = PathBuf::from(
        unsafe { CStr::from_ptr(info.fname) }
            .to_string_lossy()
            .into_owned(),
    );
    let app = std::env::current_exe()?;
    let pdfium = app
        .parent()
        .ok_or_else(|| SetupError::RuntimeOpen("the app has no bundle folder".into()))?
        .join("Frameworks/pdfium.framework/pdfium");
    Ok(LinkedRuntimes { pdfium, onnx })
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

#[allow(unsafe_code)] // Loads the runtime to check it (#897).
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
    if verify_library(&root, runtime).is_err() && install {
        fs::create_dir_all(&root)?;
        install_runtime(&root, runtime)?;
    }
    verify_library(&root, runtime)?;
    Ok(path)
}

/// Verify the shared runtime without downloading or consulting platform paths.
pub fn onnx_runtime_in(root: &Path) -> Result<PathBuf, SetupError> {
    let runtime = &RUNTIMES[1];
    let path = root.join(runtime.library);
    verify_library(root, runtime)?;
    Ok(path)
}

pub fn onnx_runtime_with_progress(
    progress: &crate::model_download::Progress,
) -> Result<PathBuf, SetupError> {
    let root = root()?;
    fs::create_dir_all(&root)?;
    let runtime = &RUNTIMES[1];
    let path = root.join(runtime.library);
    if verify_library(&root, runtime).is_err() {
        install_runtime_progress(&root, runtime, progress)?;
    }
    verify_library(&root, runtime)?;
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
pub(crate) fn install_in(root: &Path) -> Result<Installed, SetupError> {
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
        if verify_library(root, runtime).is_ok() {
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
    verify_any(path, std::iter::once(expected))
}

/// The installed library matches the build of any of its sources.
fn verify_library(root: &Path, runtime: &Runtime) -> Result<(), SetupError> {
    verify_any(&root.join(runtime.library), runtime.library_shas())
}

fn verify_any<'a>(
    path: &Path,
    expected: impl IntoIterator<Item = &'a str>,
) -> Result<(), SetupError> {
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
    let actual = digest
        .finalize()
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect::<String>();
    if !expected.into_iter().any(|sha| sha == actual) {
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
    install_runtime_with(root, runtime, progress, crate::model_download::fetch)
}

/// Download, check and install `runtime` from the first source that can be
/// downloaded. `fetch` is [`crate::model_download::fetch`] outside tests.
fn install_runtime_with(
    root: &Path,
    runtime: &Runtime,
    progress: &crate::model_download::Progress,
    fetch: impl Fn(
        &str,
        u64,
        &str,
        &Path,
        &crate::model_download::Progress,
    ) -> Result<(), crate::model_download::DownloadError>,
) -> Result<(), SetupError> {
    let temporary = tempfile::tempdir_in(root)?;
    let path = temporary.path().join("runtime.archive");
    let mut sources = runtime.sources.iter().peekable();
    while let Some(source) = sources.next() {
        match fetch(source.url, source.size, source.archive_sha, &path, progress) {
            Ok(()) => {
                let file = File::open(&path)?;
                progress.check()?;
                extract_runtime(root, runtime, source, &file)?;
                return verify(&root.join(runtime.library), source.library_sha(runtime));
            }
            Err(error) if source_unavailable(&error) => {
                let Some(next) = sources.peek() else {
                    return Err(error.into());
                };
                eprintln!(
                    "OCR setup: {} runtime unavailable from {} ({error}); trying {}",
                    runtime.name, source.url, next.url
                );
                // The bundle grows by the difference between the archives.
                let mut state = progress.state.lock().unwrap();
                if state.planned > 0 {
                    state.planned = (state.planned + next.size).saturating_sub(source.size);
                }
            }
            Err(error) => return Err(error.into()),
        }
    }
    unreachable!("every runtime has a source")
}

/// The source could not deliver its archive, so the next one may. A wrong
/// digest or size cap, a cancel or a local error stops setup instead.
fn source_unavailable(error: &crate::model_download::DownloadError) -> bool {
    use crate::model_download::DownloadError;
    matches!(
        error,
        DownloadError::Unreachable { .. }
            | DownloadError::Tls { .. }
            | DownloadError::Status { .. }
            | DownloadError::Other { .. }
            // A transfer that ended early.
            | DownloadError::SizeMismatch
    )
}

fn extract_runtime(
    root: &Path,
    runtime: &Runtime,
    source: &Source,
    archive: &File,
) -> Result<(), SetupError> {
    let notices = root.join("licenses").join(runtime.name);
    fs::create_dir_all(&notices)?;
    if source.is_zip() {
        let mut zip = zip::ZipArchive::new(archive)?;
        for index in 0..zip.len() {
            let mut entry = zip.by_index(index)?;
            if !entry.is_file() || entry.is_symlink() {
                continue;
            }
            let Some(path) = entry.enclosed_name() else {
                continue;
            };
            copy_runtime_entry(root, runtime, source, &path, entry.size(), &mut entry)?;
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
        copy_runtime_entry(root, runtime, source, &path, entry.size(), &mut entry)?;
    }
    Ok(())
}

fn copy_runtime_entry(
    root: &Path,
    runtime: &Runtime,
    source: &Source,
    path: &Path,
    size: u64,
    entry: &mut impl Read,
) -> Result<(), SetupError> {
    let notices = root.join("licenses").join(runtime.name);
    let Some(name) = path.file_name().and_then(|n| n.to_str()) else {
        return Ok(());
    };
    let destination = if path.ends_with(source.archive_library) && size < 100_000_000 {
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
            full.runtime - RUNTIMES[1].size()
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
        extract_runtime(
            dir.path(),
            &MAC_RUNTIMES[0],
            &MAC_RUNTIMES[0].sources[0],
            &archive,
        )
        .unwrap();
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
            // Laid out like NuGet's Microsoft.ML.OnnxRuntime package.
            for (name, contents) in [
                (
                    "runtimes/win-x64/native/onnxruntime.dll",
                    b"runtime".as_slice(),
                ),
                (
                    "runtimes/win-x64/native/onnxruntime.lib",
                    b"import".as_slice(),
                ),
                (
                    "runtimes/win-x64/native/onnxruntime_providers_shared.dll",
                    b"provider".as_slice(),
                ),
                (
                    "runtimes/win-arm64/native/onnxruntime.dll",
                    b"arm".as_slice(),
                ),
                ("debug/onnxruntime.dll", b"wrong".as_slice()),
                ("LICENSE", b"license".as_slice()),
                ("ThirdPartyNotices.txt", b"notice".as_slice()),
                (
                    "../runtimes/win-x64/native/onnxruntime.dll",
                    b"traversal".as_slice(),
                ),
            ] {
                zip.start_file(name, options).unwrap();
                zip.write_all(contents).unwrap();
            }
            zip.add_symlink(
                "link/runtimes/win-x64/native/onnxruntime.dll",
                "/outside",
                options,
            )
            .unwrap();
            zip.finish().unwrap();
        }
        archive.rewind().unwrap();
        extract_runtime(
            dir.path(),
            &WINDOWS_RUNTIMES[1],
            &WINDOWS_RUNTIMES[1].sources[0],
            &archive,
        )
        .unwrap();
        assert_eq!(
            fs::read(dir.path().join(WINDOWS_RUNTIMES[1].library)).unwrap(),
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
                &WINDOWS_RUNTIMES[0].sources[0],
                Path::new(path),
                bytes.len() as u64,
                &mut bytes,
            )
            .unwrap();
        }
        assert_eq!(fs::read(dir.path().join("pdfium.dll")).unwrap(), b"runtime");
        assert_eq!(fs::read_dir(dir.path()).unwrap().count(), 2);
    }

    fn sha(bytes: &[u8]) -> &'static str {
        let hex = Sha256::digest(bytes)
            .iter()
            .map(|byte| format!("{byte:02x}"))
            .collect::<String>();
        Box::leak(hex.into_boxed_str())
    }

    fn tgz(entries: &[(&str, &[u8])]) -> Vec<u8> {
        let mut tar = tar::Builder::new(flate2::write::GzEncoder::new(
            Vec::new(),
            flate2::Compression::fast(),
        ));
        for (name, contents) in entries {
            let mut header = tar::Header::new_gnu();
            header.set_size(contents.len() as u64);
            header.set_mode(0o644);
            header.set_cksum();
            tar.append_data(&mut header, name, *contents).unwrap();
        }
        tar.into_inner().unwrap().finish().unwrap()
    }

    fn zip(entries: &[(&str, &[u8])]) -> Vec<u8> {
        let mut zip = zip::ZipWriter::new(std::io::Cursor::new(Vec::new()));
        for (name, contents) in entries {
            zip.start_file(*name, zip::write::SimpleFileOptions::default())
                .unwrap();
            zip.write_all(contents).unwrap();
        }
        zip.finish().unwrap().into_inner()
    }

    const GITHUB: &str = "https://github.test/onnxruntime-osx-arm64-1.32.0.tgz";
    const NUGET: &str = "https://nuget.test/microsoft.ml.onnxruntime.1.32.0.nupkg";

    /// A runtime pinned like the real ONNX Runtime entry: a GitHub tarball
    /// first, then a NuGet package whose library is a separate build.
    fn two_source_runtime(github: &[u8], nuget: &[u8]) -> Runtime {
        let sources = vec![
            Source {
                url: GITHUB,
                size: github.len() as u64,
                archive_sha: sha(github),
                archive_library: "lib/libonnxruntime.1.32.0.dylib",
                library_sha: None,
            },
            Source {
                url: NUGET,
                size: nuget.len() as u64,
                archive_sha: sha(nuget),
                archive_library: "runtimes/osx-arm64/native/libonnxruntime.dylib",
                library_sha: Some(sha(b"nuget build")),
            },
        ];
        Runtime {
            name: "onnx",
            library: "libonnxruntime.1.32.0.dylib",
            library_sha: sha(b"github build"),
            superseded: &[],
            sources: Vec::leak(sources),
        }
    }

    /// Serves `served` by URL like `model_download::fetch`: it writes the
    /// bytes, then checks them against the pinned size and digest.
    fn fake_fetch<'a>(
        served: &'a [(
            &'a str,
            Result<Vec<u8>, crate::model_download::DownloadError>,
        )],
        calls: &'a std::cell::RefCell<Vec<String>>,
    ) -> impl Fn(
        &str,
        u64,
        &str,
        &Path,
        &crate::model_download::Progress,
    ) -> Result<(), crate::model_download::DownloadError>
    + 'a {
        move |url, size, sha, path, _| {
            calls.borrow_mut().push(url.into());
            let (_, response) = served.iter().find(|(u, _)| *u == url).unwrap();
            fs::write(path, response.clone()?)?;
            crate::model_download::verify(path, size, sha)
        }
    }

    fn github_archive() -> Vec<u8> {
        tgz(&[
            (
                "onnxruntime/lib/libonnxruntime.1.32.0.dylib",
                b"github build",
            ),
            ("onnxruntime/LICENSE", b"license"),
        ])
    }

    fn nuget_package() -> Vec<u8> {
        zip(&[
            ("runtimes/win-x64/native/onnxruntime.dll", b"windows build"),
            (
                "runtimes/osx-arm64/native/libonnxruntime.dylib",
                b"nuget build",
            ),
            (
                "runtimes/linux-x64/native/libonnxruntime.so",
                b"linux build",
            ),
            ("runtimes/android/native/onnxruntime.aar", b"android build"),
            ("LICENSE", b"license\r\n"),
            ("ThirdPartyNotices.txt", b"notices"),
            ("Microsoft.ML.OnnxRuntime.nuspec", b"<package/>"),
        ])
    }

    fn not_found() -> crate::model_download::DownloadError {
        crate::model_download::DownloadError::Status {
            host: "github.test".into(),
            code: 404,
        }
    }

    #[test]
    fn runtime_installs_from_github_without_trying_nuget() {
        let dir = tempfile::tempdir().unwrap();
        let (github, nuget) = (github_archive(), nuget_package());
        let runtime = two_source_runtime(&github, &nuget);
        let served = [(GITHUB, Ok(github)), (NUGET, Ok(nuget))];
        let calls = Default::default();
        install_runtime_with(
            dir.path(),
            &runtime,
            &Default::default(),
            fake_fetch(&served, &calls),
        )
        .unwrap();
        assert_eq!(calls.take(), [GITHUB]);
        assert_eq!(
            fs::read(dir.path().join(runtime.library)).unwrap(),
            b"github build"
        );
        verify_library(dir.path(), &runtime).unwrap();
    }

    #[test]
    fn unavailable_github_release_falls_back_to_the_nuget_package() {
        let dir = tempfile::tempdir().unwrap();
        let (github, nuget) = (github_archive(), nuget_package());
        let runtime = two_source_runtime(&github, &nuget);
        let served = [(GITHUB, Err(not_found())), (NUGET, Ok(nuget.clone()))];
        let calls = Default::default();
        let progress = crate::model_download::Progress::default();
        progress.state.lock().unwrap().planned = 1_000 + github.len() as u64;
        install_runtime_with(dir.path(), &runtime, &progress, fake_fetch(&served, &calls)).unwrap();
        assert_eq!(calls.take(), [GITHUB, NUGET]);
        // Only the platform's library, under the same installed name.
        assert_eq!(
            fs::read(dir.path().join(runtime.library)).unwrap(),
            b"nuget build"
        );
        assert_eq!(
            fs::read(dir.path().join("licenses/onnx/LICENSE")).unwrap(),
            b"license\r\n"
        );
        let mut files = fs::read_dir(dir.path())
            .unwrap()
            .map(|entry| entry.unwrap().file_name().into_string().unwrap())
            .collect::<Vec<_>>();
        files.sort();
        assert_eq!(files, ["libonnxruntime.1.32.0.dylib", "licenses"]);
        // Setup's checks accept the NuGet build from now on.
        verify_library(dir.path(), &runtime).unwrap();
        // The planned bundle now counts the larger package.
        assert_eq!(
            progress.state.lock().unwrap().planned,
            1_000 + nuget.len() as u64
        );
    }

    #[test]
    fn a_damaged_nuget_package_is_an_error() {
        let dir = tempfile::tempdir().unwrap();
        let (github, nuget) = (github_archive(), nuget_package());
        let runtime = two_source_runtime(&github, &nuget);
        let mut damaged = nuget.clone();
        *damaged.last_mut().unwrap() ^= 1;
        let served = [(GITHUB, Err(not_found())), (NUGET, Ok(damaged))];
        let calls = Default::default();
        let error = install_runtime_with(
            dir.path(),
            &runtime,
            &Default::default(),
            fake_fetch(&served, &calls),
        )
        .unwrap_err();
        assert!(
            matches!(
                error,
                SetupError::Download(crate::model_download::DownloadError::Integrity)
            ),
            "{error}"
        );
        assert!(!dir.path().join(runtime.library).exists());
    }

    #[test]
    fn a_damaged_github_archive_never_falls_back() {
        let dir = tempfile::tempdir().unwrap();
        let (github, nuget) = (github_archive(), nuget_package());
        let runtime = two_source_runtime(&github, &nuget);
        let mut damaged = github.clone();
        *damaged.last_mut().unwrap() ^= 1;
        let served = [(GITHUB, Ok(damaged)), (NUGET, Ok(nuget))];
        let calls = Default::default();
        let error = install_runtime_with(
            dir.path(),
            &runtime,
            &Default::default(),
            fake_fetch(&served, &calls),
        )
        .unwrap_err();
        assert!(matches!(error, SetupError::Download(_)), "{error}");
        assert_eq!(calls.take(), [GITHUB]);
        assert!(!dir.path().join(runtime.library).exists());
    }

    #[test]
    fn a_package_library_is_checked_against_its_own_source_pin() {
        let dir = tempfile::tempdir().unwrap();
        let github = github_archive();
        // A correctly pinned package that holds the GitHub build: the NuGet
        // source's own library pin refuses it.
        let nuget = zip(&[(
            "runtimes/osx-arm64/native/libonnxruntime.dylib",
            b"github build",
        )]);
        let runtime = two_source_runtime(&github, &nuget);
        let served = [(GITHUB, Err(not_found())), (NUGET, Ok(nuget))];
        let calls = Default::default();
        let error = install_runtime_with(
            dir.path(),
            &runtime,
            &Default::default(),
            fake_fetch(&served, &calls),
        )
        .unwrap_err();
        assert!(matches!(error, SetupError::Integrity(_)), "{error}");
    }

    #[test]
    fn only_unreachable_sources_fall_back() {
        use crate::model_download::DownloadError;
        let host = || "github.com".to_string();
        for error in [
            DownloadError::Unreachable { host: host() },
            DownloadError::Tls { host: host() },
            not_found(),
            DownloadError::Other {
                host: host(),
                message: "redirect loop".into(),
            },
            DownloadError::SizeMismatch,
        ] {
            assert!(source_unavailable(&error), "{error}");
        }
        for error in [
            DownloadError::Integrity,
            DownloadError::Oversize,
            DownloadError::Cancelled,
            DownloadError::NetworkDisabled,
            DownloadError::InvalidPath,
        ] {
            assert!(!source_unavailable(&error), "{error}");
        }
    }

    #[test]
    fn every_runtime_source_is_pinned() {
        for runtime in MAC_RUNTIMES.iter().chain(WINDOWS_RUNTIMES) {
            assert!(!runtime.sources.is_empty(), "{}", runtime.name);
            for source in runtime.sources {
                assert!(source.url.starts_with("https://"), "{}", source.url);
                assert!(source.size > 0, "{}", source.url);
                for pin in [source.archive_sha, source.library_sha(runtime)] {
                    assert_eq!(pin.len(), 64, "{}", source.url);
                    assert!(pin.bytes().all(|b| b.is_ascii_hexdigit()), "{}", source.url);
                }
            }
        }
    }

    /// 1.27.0's libraries, left by an earlier setup, are a runtime still to
    /// download: setup cards count it and checks read "not set up".
    #[test]
    fn an_earlier_onnx_runtime_reads_as_missing() {
        let dir = tempfile::tempdir().unwrap();
        for old in ["libonnxruntime.1.27.0.dylib", "onnxruntime.dll"] {
            fs::write(dir.path().join(old), b"ONNX Runtime 1.27.0").unwrap();
        }
        assert_eq!(onnx_runtime_pending(Some(dir.path())), onnx_runtime_size());
        assert_eq!(
            pending(Some(dir.path()), OcrModel::Cyrillic).runtime,
            RUNTIMES.iter().map(Runtime::size).sum::<u64>()
        );
        assert!(onnx_runtime_in(dir.path()).unwrap_err().is_missing());
    }

    /// An earlier release's ONNX Runtime in place of the pinned one makes
    /// its download an update; none, or the pinned one beside it, does not.
    #[test]
    fn an_earlier_onnx_runtime_is_superseded() {
        let dir = tempfile::tempdir().unwrap();
        assert!(!onnx_runtime_superseded(None));
        assert!(!onnx_runtime_superseded(Some(dir.path())));
        assert!(!pending(Some(dir.path()), OcrModel::Cyrillic).replaces_runtime);
        for runtime in [&MAC_RUNTIMES[1], &WINDOWS_RUNTIMES[1]] {
            assert!(!runtime.superseded.is_empty());
            assert!(!runtime.superseded.contains(&runtime.library));
            for old in runtime.superseded {
                fs::write(dir.path().join(old), b"ONNX Runtime 1.27.0").unwrap();
            }
        }
        assert!(onnx_runtime_superseded(Some(dir.path())));
        assert!(pending(Some(dir.path()), OcrModel::Cyrillic).replaces_runtime);
        assert_eq!(onnx_runtime_pending(Some(dir.path())), onnx_runtime_size());
        fs::write(onnx_runtime_path(dir.path()), b"ONNX Runtime 1.32.0").unwrap();
        assert!(!onnx_runtime_superseded(Some(dir.path())));
    }

    /// A version bump changes every ONNX Runtime source and both installed
    /// library names together, so an earlier version's library reads as
    /// missing rather than damaged.
    #[test]
    fn onnx_runtime_sources_match_the_installed_version() {
        let version = MAC_RUNTIMES[1]
            .library
            .strip_prefix("libonnxruntime.")
            .and_then(|name| name.strip_suffix(".dylib"))
            .unwrap();
        assert_eq!(
            WINDOWS_RUNTIMES[1].library,
            format!("onnxruntime.{version}.dll")
        );
        for runtime in [&MAC_RUNTIMES[1], &WINDOWS_RUNTIMES[1]] {
            assert_eq!(runtime.name, "onnx");
            for source in runtime.sources {
                assert!(
                    source.url.contains(&format!("/{version}/"))
                        || source.url.contains(&format!("/v{version}/")),
                    "{} is not ONNX Runtime {version}",
                    source.url
                );
            }
        }
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
