//! Explicit, app-managed OCR setup. Only `install` is allowed to use the network.
use std::{
    fs::{self, File},
    io::{Read, Seek, SeekFrom, Write},
    path::{Path, PathBuf},
};

use pdf_inspector::vision::{
    HttpModelDownloader, ModelArtifactKind, ModelDownloadPolicy, ModelStore, OarOcrEngine, OcrMode,
    OcrOptions, OcrPdfOptions, PP_OCR_CYRILLIC, PdfiumRenderer,
};
use sha2::{Digest, Sha256};

// Other desktop targets remain buildable; qualify their actual runtimes before enabling.
pub const SUPPORTED: bool = cfg!(any(
    all(target_os = "macos", target_arch = "aarch64"),
    all(target_os = "windows", target_arch = "x86_64")
));

#[derive(Clone, Debug)]
pub struct Installed {
    pub models: PathBuf,
    pub pdfium: PathBuf,
    pub onnx: PathBuf,
}

impl Installed {
    pub fn options(&self) -> OcrPdfOptions {
        OcrPdfOptions {
            model_manifest: &PP_OCR_CYRILLIC,
            pdfium_library: Some(self.pdfium.clone()),
            onnx_runtime_library: Some(self.onnx.clone()),
            ocr: OcrOptions::new()
                .mode(OcrMode::Auto)
                .model_directory(&self.models)
                .model_downloads(ModelDownloadPolicy::Offline),
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

pub fn download_megabytes() -> u64 {
    let runtimes: u64 = RUNTIMES.iter().map(|runtime| runtime.size).sum();
    let models: u64 = PP_OCR_CYRILLIC
        .artifacts
        .iter()
        .map(|model| model.size)
        .sum();
    (runtimes + models).div_ceil(1_000_000)
}

fn root() -> Result<PathBuf, String> {
    dirs::data_local_dir()
        .map(|path| path.join("mdoc/ocr/v1"))
        .ok_or_else(|| "Could not find local application storage for OCR.".into())
}

/// No downloads, including when files are missing or invalid.
pub fn check() -> Result<Option<Installed>, String> {
    if !SUPPORTED || cfg!(test) {
        return Ok(None);
    }
    let root = root()?;
    if !root.exists() {
        return Ok(None);
    }
    validate(&root).map(Some)
}

fn validate(root: &Path) -> Result<Installed, String> {
    for runtime in RUNTIMES {
        verify(&root.join(runtime.library), runtime.library_sha)?;
    }
    let models = ModelStore::new(root.join("models"))
        .resolve(&PP_OCR_CYRILLIC)
        .map_err(|e| format!("OCR models are missing or damaged: {e}"))?;
    let installed = Installed {
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

pub fn install() -> Result<Installed, String> {
    if !SUPPORTED {
        return Err("Local OCR supports Apple Silicon macOS and Windows x64.".into());
    }
    install_in(&root()?)
}

fn install_in(root: &Path) -> Result<Installed, String> {
    fs::create_dir_all(root).map_err(|e| e.to_string())?;
    for runtime in RUNTIMES {
        if verify(&root.join(runtime.library), runtime.library_sha).is_ok() {
            continue;
        }
        install_runtime(root, runtime)
            .map_err(|e| format!("{} setup failed: {e}", runtime.name))?;
    }
    ModelStore::new(root.join("models"))
        .resolve_or_download(
            &PP_OCR_CYRILLIC,
            ModelDownloadPolicy::IfMissing,
            &HttpModelDownloader::default(),
        )
        .map_err(|e| format!("OCR model download failed. Retry setup: {e}"))?;
    let notices = root.join("licenses/models");
    fs::create_dir_all(&notices).map_err(|e| e.to_string())?;
    fs::write(
        notices.join("LICENSE"),
        include_str!("../resources/ocr-model-license.txt"),
    )
    .map_err(|e| e.to_string())?;
    fs::write(notices.join("NOTICE"), "PP-OCRv6 Small detection and PP-OCRv5 Cyrillic mobile recognition.\nPaddleOCR / PaddlePaddle authors; ONNX artifacts distributed by GreatV/oar-ocr.\nhttps://github.com/PaddlePaddle/PaddleOCR\nhttps://github.com/GreatV/oar-ocr\nModel files are unmodified; artifact versions and checksums are pinned by pdf-inspector.\n").map_err(|e| e.to_string())?;
    validate(root)
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
    let agent = ureq::Agent::new_with_config(
        ureq::Agent::config_builder()
            .https_only(true)
            .timeout_global(Some(std::time::Duration::from_secs(300)))
            .build(),
    );
    let mut response = agent.get(runtime.url).call().map_err(|e| e.to_string())?;
    let mut archive = tempfile::NamedTempFile::new_in(root).map_err(|e| e.to_string())?;
    let size = std::io::copy(
        &mut response.body_mut().as_reader().take(runtime.size + 1),
        &mut archive,
    )
    .map_err(|e| e.to_string())?;
    if size != runtime.size {
        return Err(
            "Runtime download was incomplete or had an unexpected size. Retry setup.".into(),
        );
    }
    archive.flush().map_err(|e| e.to_string())?;
    verify(archive.path(), runtime.archive_sha)?;
    archive
        .seek(SeekFrom::Start(0))
        .map_err(|e| e.to_string())?;
    extract_runtime(root, runtime, archive.as_file())?;
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
            models: "models".into(),
            pdfium: "pdfium".into(),
            onnx: "onnx".into(),
        };
        let options = installed.options();
        assert_eq!(options.ocr.model_downloads, ModelDownloadPolicy::Offline);
        assert_eq!(options.model_manifest.id, PP_OCR_CYRILLIC.id);
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
        let installed = install_in(dir.path()).unwrap();
        // Second setup verifies a complete installation without network recovery.
        let ready = validate(dir.path()).unwrap();
        assert_eq!(installed.models, ready.models);
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
            assert!(imported.ocr_failure.is_none());
            assert_eq!(fs::read(pdf).unwrap(), original);
        }
    }
}
