//! Explicit, app-managed OCR setup. Only `install` is allowed to use the network.
use std::{
    fs::{self, File},
    io::Read,
    path::{Path, PathBuf},
};

use pdf_inspector::vision::{
    ModelArtifactKind, ModelDownloadPolicy, ModelStore, OarOcrEngine, OcrMode, OcrOptions,
    OcrPdfOptions, PdfiumRenderer,
};
use sha2::{Digest, Sha256};

// Other desktop targets remain buildable; qualify their actual runtimes before enabling.
pub const SUPPORTED: bool = cfg!(any(
    all(target_os = "macos", target_arch = "aarch64"),
    all(target_os = "windows", target_arch = "x86_64")
));

#[derive(Clone, Debug)]
pub struct Installed {
    pub config: crate::settings::OcrConfig,
    pub models: PathBuf,
    pub pdfium: PathBuf,
    pub onnx: PathBuf,
}

impl Installed {
    pub fn options(&self) -> OcrPdfOptions {
        OcrPdfOptions {
            model_manifest: self.config.model.manifest(),
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

/// Bytes OCR setup still needs for `model`: missing model files plus any
/// runtime archive not yet installed. Tests always report a full download.
pub fn pending(model: crate::settings::OcrModel) -> crate::model_download::Pending {
    pending_in(root().ok().filter(|_| !cfg!(test)).as_deref(), model)
}
fn pending_in(
    root: Option<&Path>,
    model: crate::settings::OcrModel,
) -> crate::model_download::Pending {
    let model_root = root.map(|root| model_root(root, model));
    crate::model_download::Pending {
        model: model
            .manifest()
            .artifacts
            .iter()
            .filter(|a| {
                model_root
                    .as_ref()
                    .is_none_or(|dir| crate::model_download::missing(&dir.join(a.filename), a.size))
            })
            .map(|a| a.size)
            .sum(),
        runtime: RUNTIMES
            .iter()
            .filter(|r| root.is_none_or(|root| !root.join(r.library).exists()))
            .map(|r| r.size)
            .sum(),
    }
}
/// Bytes of the shared ONNX Runtime archive when it is not installed yet.
pub(crate) fn onnx_runtime_pending() -> u64 {
    let runtime = &RUNTIMES[1];
    match root() {
        Ok(root) if !cfg!(test) && root.join(runtime.library).exists() => 0,
        _ => runtime.size,
    }
}

pub(crate) fn root() -> Result<PathBuf, String> {
    dirs::data_local_dir()
        .map(|path| path.join("mdoc/ocr/v1"))
        .ok_or_else(|| "Could not find local application storage for OCR.".into())
}

/// No downloads, including when files are missing or invalid.
pub fn check_config(config: &crate::settings::OcrConfig) -> Result<Option<Installed>, String> {
    if !SUPPORTED || cfg!(test) {
        return Ok(None);
    }
    let _permit = crate::model_work::Permit::acquire_for("checking text recognition")?;
    check_reserved(config)
}

/// The caller owns model admission, including for the complete catalog check.
pub fn check_reserved(config: &crate::settings::OcrConfig) -> Result<Option<Installed>, String> {
    if !SUPPORTED || cfg!(test) {
        return Ok(None);
    }
    let root = root()?;
    let model = model_root(&root, config.model);
    if !model.exists() {
        return Ok(None);
    }
    validate_config(&root, config).map(Some)
}

pub fn model_root(root: &Path, model: crate::settings::OcrModel) -> PathBuf {
    let manifest = model.manifest();
    root.join("models")
        .join(manifest.id)
        .join(manifest.revision)
}

#[cfg(test)]
fn check_in(root: &Path) -> Result<Option<Installed>, String> {
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
fn qualification_config() -> crate::settings::OcrConfig {
    crate::settings::OcrConfig {
        model: crate::settings::OcrModel::Cyrillic,
        ..Default::default()
    }
}

#[cfg(test)]
fn validate(root: &Path) -> Result<Installed, String> {
    validate_config(root, &qualification_config())
}

fn validate_config(root: &Path, config: &crate::settings::OcrConfig) -> Result<Installed, String> {
    for runtime in RUNTIMES {
        verify(&root.join(runtime.library), runtime.library_sha)?;
    }
    let models = ModelStore::new(root.join("models"))
        .resolve(config.model.manifest())
        .map_err(|e| format!("OCR models are missing or damaged: {e}"))?;
    let installed = Installed {
        config: config.clone(),
        models: models
            .get(ModelArtifactKind::TextDetection)
            .and_then(Path::parent)
            .ok_or("OCR detector is missing.")?
            .to_path_buf(),
        pdfium: root.join(RUNTIMES[0].library),
        onnx: root.join(RUNTIMES[1].library),
    };
    // Check actual loading before claiming ready. All of this runs off the UI thread.
    PdfiumRenderer::load_from_path(&installed.pdfium).map_err(|e| e.to_string())?;
    OarOcrEngine::from_models_with_runtime(&models, Some(&installed.onnx)).map_err(|e| {
        if cfg!(target_os = "windows")
            && let pdf_inspector::vision::OarOcrError::OnnxRuntimeLoad { source, .. } = &e
        {
            return format!(
                "Could not load the OCR runtime: {source}. Install or repair Microsoft Visual C++ Redistributable (x64) from https://aka.ms/vc14/vc_redist.x64.exe, restart mdoc, then retry OCR setup."
            );
        }
        e.to_string()
    })?;
    Ok(installed)
}

/// Called by a worker that already owns model admission.
pub fn installed_config(config: &crate::settings::OcrConfig) -> Result<Installed, String> {
    validate_config(&root()?, config)
}

/// Share the pinned native runtime without installing OCR models/PDFium.
/// The pseudonymization model has its own setup and never invokes OCR setup.
pub(crate) fn onnx_runtime(install: bool) -> Result<PathBuf, String> {
    if !SUPPORTED {
        return Err("Local inference supports Apple Silicon macOS and Windows x64.".into());
    }
    let root = root()?;
    let runtime = &RUNTIMES[1];
    let path = root.join(runtime.library);
    if verify(&path, runtime.library_sha).is_err() && install {
        fs::create_dir_all(&root).map_err(|e| e.to_string())?;
        install_runtime(&root, runtime)?;
    }
    verify(&path, runtime.library_sha)?;
    Ok(path)
}

pub(crate) fn onnx_runtime_with_progress(
    progress: &crate::model_download::Progress,
) -> Result<PathBuf, String> {
    let root = root()?;
    fs::create_dir_all(&root).map_err(|e| e.to_string())?;
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
fn install_in(root: &Path) -> Result<Installed, String> {
    install_config_in(
        root,
        &qualification_config(),
        &crate::model_download::Progress::default(),
    )
}

pub fn install_config(
    config: &crate::settings::OcrConfig,
    progress: &crate::model_download::Progress,
) -> Result<Installed, String> {
    if !SUPPORTED {
        return Err("Local OCR supports Apple Silicon macOS and Windows x64.".into());
    }
    let _permit = crate::model_work::Permit::acquire_for("installing text recognition")?;
    install_config_in(&root()?, config, progress)
}
fn install_config_in(
    root: &Path,
    config: &crate::settings::OcrConfig,
    progress: &crate::model_download::Progress,
) -> Result<Installed, String> {
    fs::create_dir_all(root).map_err(|e| e.to_string())?;
    for runtime in RUNTIMES {
        progress.check()?;
        if verify(&root.join(runtime.library), runtime.library_sha).is_ok() {
            continue;
        }
        install_runtime_progress(root, runtime, progress)?;
    }
    let manifest = config.model.manifest();
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
    fs::create_dir_all(&notices).map_err(|e| e.to_string())?;
    fs::write(
        notices.join("LICENSE"),
        include_str!("../resources/ocr-model-license.txt"),
    )
    .map_err(|e| e.to_string())?;
    fs::write(notices.join("NOTICE"), format!("{}; {}.\nPaddleOCR / PaddlePaddle authors; unmodified ONNX artifacts from GreatV/oar-ocr.\nhttps://github.com/PaddlePaddle/PaddleOCR\nhttps://github.com/GreatV/oar-ocr\n", manifest.id,manifest.revision)).map_err(|e|e.to_string())?;
    progress.check()?;
    progress.phase("Checking runtime");
    let installed = validate_config(root, config)?;
    progress.check()?;
    Ok(installed)
}

pub fn remove_model(model: crate::settings::OcrModel) -> Result<(), String> {
    let _permit = crate::model_work::Permit::acquire_for("removing text recognition")?;
    remove_model_files(&root()?, model)
}
fn remove_model_files(root: &Path, model: crate::settings::OcrModel) -> Result<(), String> {
    let path = model_root(root, model);
    if path.exists() {
        fs::remove_dir_all(path).map_err(|e| e.to_string())?;
    }
    Ok(())
}

fn verify(path: &Path, expected: &str) -> Result<(), String> {
    let mut file = File::open(path).map_err(|e| format!("{}: {e}", path.display()))?;
    let mut digest = Sha256::new();
    let mut buffer = [0; 64 * 1024];
    loop {
        let count = file.read(&mut buffer).map_err(|e| e.to_string())?;
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
        return Err(format!(
            "{} failed its integrity check. Retry OCR setup.",
            path.display()
        ));
    }
    Ok(())
}

fn install_runtime(root: &Path, runtime: &Runtime) -> Result<(), String> {
    install_runtime_progress(root, runtime, &crate::model_download::Progress::default())
}
fn install_runtime_progress(
    root: &Path,
    runtime: &Runtime,
    progress: &crate::model_download::Progress,
) -> Result<(), String> {
    let temporary = tempfile::tempdir_in(root).map_err(|e| e.to_string())?;
    let path = temporary.path().join("runtime.archive");
    crate::model_download::fetch(
        runtime.url,
        runtime.size,
        runtime.archive_sha,
        &path,
        progress,
    )?;
    let file = File::open(path).map_err(|e| e.to_string())?;
    progress.check()?;
    extract_runtime(root, runtime, &file)?;
    verify(&root.join(runtime.library), runtime.library_sha)
}

fn extract_runtime(root: &Path, runtime: &Runtime, archive: &File) -> Result<(), String> {
    let notices = root.join("licenses").join(runtime.name);
    fs::create_dir_all(&notices).map_err(|e| e.to_string())?;
    if runtime.url.ends_with(".zip") {
        let mut zip = zip::ZipArchive::new(archive).map_err(|e| e.to_string())?;
        for index in 0..zip.len() {
            let mut entry = zip.by_index(index).map_err(|e| e.to_string())?;
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
    for entry in tar.entries().map_err(|e| e.to_string())? {
        let mut entry = entry.map_err(|e| e.to_string())?;
        if !entry.header().entry_type().is_file() {
            continue;
        }
        let path = entry.path().map_err(|e| e.to_string())?.into_owned();
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
) -> Result<(), String> {
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
    let mut file = tempfile::NamedTempFile::new_in(destination.parent().unwrap())
        .map_err(|e| e.to_string())?;
    std::io::copy(entry, &mut file).map_err(|e| e.to_string())?;
    file.as_file().sync_all().map_err(|e| e.to_string())?;
    file.persist(destination).map_err(|e| e.to_string())?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::settings::OcrModel;

    #[test]
    fn removing_a_bundle_preserves_other_bundles_and_shared_runtimes() {
        use crate::settings::OcrModel;
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

    #[test]
    fn pending_counts_only_missing_model_files_and_runtimes() {
        let dir = tempfile::tempdir().unwrap();
        let model = OcrModel::V6Small;
        let full = pending_in(None, model);
        assert_eq!(pending_in(Some(dir.path()), model), full);
        let model_bytes: u64 = model.manifest().artifacts.iter().map(|a| a.size).sum();
        assert_eq!(full.model, model_bytes);
        fs::write(dir.path().join(RUNTIMES[1].library), b"shared-runtime").unwrap();
        assert_eq!(
            pending_in(Some(dir.path()), model).runtime,
            full.runtime - RUNTIMES[1].size
        );
    }

    #[test]
    fn incomplete_or_modified_installations_are_not_ready() {
        let dir = tempfile::tempdir().unwrap();
        assert!(validate(dir.path()).is_err());
        fs::write(dir.path().join(RUNTIMES[0].library), b"corrupt").unwrap();
        assert!(validate(dir.path()).unwrap_err().contains("integrity"));
    }

    #[test]
    fn import_options_never_download_and_select_qualified_model() {
        let installed = Installed {
            config: crate::settings::OcrConfig::default(),
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
        assert!(pending_in(Some(dir.path()), OcrModel::Cyrillic).total() > 0);
        let installed = install_in(dir.path()).unwrap();
        // Second setup verifies a complete installation without network recovery.
        let ready = validate(dir.path()).unwrap();
        assert_eq!(installed.models, ready.models);
        assert_eq!(
            pending_in(Some(dir.path()), OcrModel::Cyrillic),
            Default::default()
        );
        for language in ["english", "russian"] {
            let base =
                Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/ocr-qualification");
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
