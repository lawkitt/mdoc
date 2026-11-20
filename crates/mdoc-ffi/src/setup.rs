//! `Setup`: the models a device may offer, from the desktop's
//! pinned manifests, installing a file the platform downloaded (SHA-256
//! against the pinned digest, then an atomic move into the model folder),
//! and timing the detector once it is installed. The platform runs the
//! download; nothing here uses the network.
mod speed;

use crate::CoreError;
use mdoc_store::settings::{OcrModel, PiiModel};
pub use speed::DetectorSpeed;
pub(crate) use speed::PAGE_CHARS;
use std::{
    path::{Path, PathBuf},
    sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
    },
};

const MIB: u64 = 1024 * 1024;

/// GLiNER2 FP16's RAM floor: devices sold as 4 GB and up.
/// Devices report a little less than their nominal RAM (a 4 GB iPhone
/// about 3.6 GiB, a 3 GB one under 3 GiB), so the cut sits between the two
/// classes, at 3.25 GiB.
pub const DETECTOR_FLOOR_BYTES: u64 = 13 * 256 * MIB;

/// Memory a scan needs free when it starts: the detector's measured peak in
/// the app (about 1.1 GB on the iPhone 17 Pro) and a margin.
pub const DETECTOR_FREE_BYTES: u64 = 5 * 256 * MIB;

/// Why a device offers no detector.
#[derive(Clone, Copy, Debug, PartialEq, Eq, uniffi::Enum)]
pub enum FloorReason {
    /// Under `DETECTOR_FLOOR_BYTES` of RAM.
    Memory,
    /// Its timed run is past the speed ceiling.
    Speed,
}

/// A downloadable model. Structured rules need none and run everywhere.
#[derive(Clone, Copy, Debug, PartialEq, Eq, uniffi::Enum)]
pub enum ModelId {
    /// GLiNER2 PII, FP16: the detector.
    Gliner2Fp16,
    /// PP-OCR, reading English.
    OcrEnglish,
    /// PP-OCR, reading English and Russian.
    OcrEnglishRussian,
}

impl ModelId {
    pub const ALL: [Self; 3] = [Self::Gliner2Fp16, Self::OcrEnglish, Self::OcrEnglishRussian];

    fn ocr(self) -> Option<OcrModel> {
        match self {
            Self::Gliner2Fp16 => None,
            Self::OcrEnglish => Some(OcrModel::V6Small),
            Self::OcrEnglishRussian => Some(OcrModel::Cyrillic),
        }
    }
}

/// One pinned file of a model.
#[derive(Clone, Debug, PartialEq, uniffi::Record)]
pub struct ModelFile {
    /// Where it goes in the model's folder; `install` names files by it.
    pub path: String,
    pub url: String,
    pub sha256: String,
    pub bytes: u64,
}

#[derive(Clone, Debug, PartialEq, uniffi::Record)]
pub struct ModelInfo {
    pub id: ModelId,
    /// The model's name, as Settings shows it on desktop.
    pub name: String,
    /// What it does for the user: "Detector" or the languages it reads.
    pub title: String,
    pub bytes: u64,
    pub files: Vec<ModelFile>,
    /// Whether this device may offer it.
    pub offered: bool,
    /// Every file is in place at its pinned size.
    pub installed: bool,
    /// Bytes still to download.
    pub missing_bytes: u64,
}

/// What `DocumentSession::scan` runs the detector on.
#[derive(Clone, Debug, PartialEq, uniffi::Record)]
pub struct DetectorPaths {
    /// The GLiNER2 model folder.
    pub model: String,
    pub onnx: String,
    /// This device's timed run, once there is one: the scan's estimates.
    pub speed: Option<DetectorSpeed>,
}

/// What `DocumentSession::import_file` recognizes with.
#[derive(Clone, Debug, PartialEq, uniffi::Record)]
pub struct OcrPaths {
    /// The OCR root, in the desktop layout (`models/<id>/<revision>/…`).
    pub root: String,
    pub model: ModelId,
    pub pdfium: String,
    pub onnx: String,
}

impl OcrPaths {
    pub(crate) fn installed(&self) -> Result<mdoc_import::ocr::Installed, CoreError> {
        let model = self
            .model
            .ocr()
            .ok_or_else(|| CoreError::invalid("Choose an OCR model to recognize text."))?;
        Ok(mdoc_import::ocr::installed_at(
            Path::new(&self.root),
            &mdoc_store::settings::OcrConfig {
                model,
                ..Default::default()
            },
            PathBuf::from(&self.pdfium),
            PathBuf::from(&self.onnx),
        )?)
    }
}

/// One [`measure_ocr`] run.
#[derive(Clone, Debug, PartialEq, uniffi::Record)]
pub struct OcrMeasurement {
    pub markdown: String,
    /// Every recognized span's text, one per line.
    pub recognition: String,
    /// Wall time of the conversion, the engine's loading included.
    pub seconds: f64,
}

/// Measurement only, never called by the app: the PDF at `pdf` recognized
/// with `paths` as an import would, but rendered at `dpi` (the iOS
/// resolution test, `OCRResolutionTests`).
#[uniffi::export]
pub fn measure_ocr(paths: OcrPaths, pdf: String, dpi: u16) -> Result<OcrMeasurement, CoreError> {
    let mut installed = paths.installed()?;
    installed.config.dpi = dpi;
    let bytes = std::fs::read(&pdf).map_err(|e| CoreError::invalid(format!("{pdf}: {e}")))?;
    let measured = installed.measure(&bytes, true)?;
    Ok(OcrMeasurement {
        markdown: measured.markdown,
        recognition: measured.recognition,
        seconds: measured.seconds,
    })
}

#[derive(uniffi::Object)]
pub struct Setup {
    root: PathBuf,
    physical_memory: u64,
    /// Set by `cancel_timing`; cleared when a timed run ends.
    timing_cancelled: AtomicBool,
}

#[uniffi::export]
impl Setup {
    /// Models under `models_root`, for a device with `physical_memory_bytes`
    /// of RAM (iOS: `ProcessInfo.physicalMemory`).
    #[uniffi::constructor]
    pub fn new(models_root: String, physical_memory_bytes: u64) -> Arc<Self> {
        Arc::new(Self {
            root: PathBuf::from(models_root),
            physical_memory: physical_memory_bytes,
            timing_cancelled: AtomicBool::new(false),
        })
    }

    /// Every model, offered or not, in setup order.
    pub fn catalog(&self) -> Vec<ModelInfo> {
        ModelId::ALL.into_iter().map(|id| self.info(id)).collect()
    }

    /// Whether this device may offer `model`: OCR everywhere, the detector
    /// above the floor.
    pub fn offers(&self, model: ModelId) -> bool {
        model.ocr().is_some() || self.below_floor().is_none()
    }

    /// Why this device offers no detector: too little RAM, or a timed run
    /// past the speed ceiling. `None` when it offers one.
    pub fn below_floor(&self) -> Option<FloorReason> {
        if self.physical_memory < DETECTOR_FLOOR_BYTES {
            Some(FloorReason::Memory)
        } else if self.detector_speed().is_some_and(|s| s.too_slow()) {
            Some(FloorReason::Speed)
        } else {
            None
        }
    }

    /// This device's timed detector run, once `time_detector` has run for
    /// the installed model.
    pub fn detector_speed(&self) -> Option<DetectorSpeed> {
        speed::read(&self.root, &self.detector_folder_name())
    }

    /// Seconds a scan of 10 pages takes on this device, for Settings.
    pub fn ten_pages_seconds(&self) -> Option<u64> {
        self.detector_speed()
            .map(|s| s.seconds(10 * PAGE_CHARS).round() as u64)
    }

    /// Time one short detector run and keep the result: the platform calls
    /// it right after the detector installs, or before the first scan of a
    /// detector installed untimed, off the main thread. It runs as
    /// a scan does: it needs the memory free, pauses while the phone is too
    /// hot (both through `progress`) and stops at `cancel_timing`. A device
    /// past the speed ceiling counts as below the floor from then on, and
    /// the model it cannot use is removed. `None` when the detector is not
    /// installed or not offered here.
    pub fn time_detector(
        &self,
        runtimes: Runtimes,
        progress: Option<Box<dyn crate::document_session::JobProgress>>,
    ) -> Result<Option<DetectorSpeed>, CoreError> {
        let result = self.timed_run(runtimes, progress.as_deref());
        // A cancel belongs to this run only.
        self.timing_cancelled.store(false, Ordering::Relaxed);
        result
    }

    /// Stops the running `time_detector`, or the next one if none runs yet.
    pub fn cancel_timing(&self) {
        self.timing_cancelled.store(true, Ordering::Relaxed);
    }

    /// Install `downloaded`, the platform's download of `model`'s file
    /// `file`: checked against the pinned size and SHA-256, then moved into
    /// the model folder in one step. A wrong file is refused and left where
    /// it is; nothing in the model folder changes.
    pub fn install(
        &self,
        model: ModelId,
        file: String,
        downloaded: String,
    ) -> Result<ModelInfo, CoreError> {
        let pinned = files(model)
            .into_iter()
            .find(|candidate| candidate.path == file)
            .ok_or_else(|| CoreError::invalid(format!("{file} is not a file of this model.")))?;
        let target = self.folder(model).join(&pinned.path);
        install_file(&pinned, Path::new(&downloaded), &target)?;
        if model.ocr().is_none() {
            // The encoder is re-saved with its weights beside it (#905); a
            // failure keeps the verified file, and the next scan retries.
            if let Err(error) =
                mdoc_detect::convert_at(&self.folder(model), PiiModel::Fp16, &|| Ok(()))
            {
                eprintln!("Could not re-save the detector's encoder: {error}");
            }
        }
        Ok(self.info(model))
    }

    /// The detector's paths once it is installed and offered here.
    pub fn detector_paths(&self, runtimes: Runtimes) -> Option<DetectorPaths> {
        let model = ModelId::Gliner2Fp16;
        (self.offers(model) && self.info(model).installed).then(|| DetectorPaths {
            model: self.folder(model).display().to_string(),
            onnx: runtimes.onnx,
            speed: self.detector_speed(),
        })
    }

    /// OCR's paths for `model` once it is installed.
    pub fn ocr_paths(&self, model: ModelId, runtimes: Runtimes) -> Option<OcrPaths> {
        (model.ocr().is_some() && self.info(model).installed).then(|| OcrPaths {
            root: self.ocr_root().display().to_string(),
            model,
            pdfium: runtimes.pdfium,
            onnx: runtimes.onnx,
        })
    }
}

impl Setup {
    /// `time_detector`'s run, as a scan runs it.
    fn timed_run(
        &self,
        runtimes: Runtimes,
        progress: Option<&dyn crate::document_session::JobProgress>,
    ) -> Result<Option<DetectorSpeed>, CoreError> {
        let Some(paths) = self.detector_paths(runtimes) else {
            return Ok(None);
        };
        crate::document_session::check_free_memory(progress)?;
        let (_, timings) = mdoc_detect::scan_at(
            speed::PROBE,
            &self.timing_cancelled,
            Path::new(&paths.model),
            Path::new(&paths.onnx),
            &mdoc_store::settings::PiiConfig::default(),
            mdoc_detect::ScanLimit::PHONE,
            &crate::document_session::PhoneScan::quiet(progress),
        )?;
        let chars = speed::PROBE.chars().count();
        let inference: f64 = timings
            .windows
            .iter()
            .map(|w| w.inference.as_secs_f64())
            .sum();
        let measured = DetectorSpeed::measured(timings.engine_load.as_secs_f64(), chars, inference);
        speed::write(&self.root, &self.detector_folder_name(), measured)?;
        if measured.too_slow() {
            std::fs::remove_dir_all(self.folder(ModelId::Gliner2Fp16))?;
        }
        Ok(Some(measured))
    }

    /// The desktop's layouts under the models root: `pseudonymization/` as
    /// `mdoc_detect::root_for` names its folder, `ocr/` as OCR's root.
    fn folder(&self, model: ModelId) -> PathBuf {
        match model.ocr() {
            None => self
                .root
                .join("pseudonymization")
                .join(self.detector_folder_name()),
            Some(ocr) => mdoc_import::ocr::model_root(&self.ocr_root(), ocr),
        }
    }

    fn detector_folder_name(&self) -> String {
        let manifest = mdoc_detect::manifest_for(PiiModel::Fp16);
        format!("{}-{}", manifest.id, &manifest.revision[..8])
    }

    fn ocr_root(&self) -> PathBuf {
        self.root.join("ocr")
    }

    fn info(&self, id: ModelId) -> ModelInfo {
        let files = files(id);
        let folder = self.folder(id);
        let missing_bytes = match id.ocr() {
            // An encoder counts in either layout (#905).
            None => mdoc_detect::manifest_for(PiiModel::Fp16)
                .files
                .iter()
                .filter(|f| mdoc_detect::artifact_missing(&folder, f))
                .map(|f| f.bytes)
                .sum(),
            Some(_) => files
                .iter()
                .filter(|f| mdoc_import::model_download::missing(&folder.join(&f.path), f.bytes))
                .map(|f| f.bytes)
                .sum(),
        };
        let (name, title) = match id.ocr() {
            None => (PiiModel::Fp16.name().into(), "Detector".into()),
            Some(ocr) => (ocr.name().into(), mdoc_import::ocr::title(ocr)),
        };
        ModelInfo {
            id,
            name,
            title,
            bytes: files.iter().map(|f| f.bytes).sum(),
            offered: self.offers(id),
            installed: missing_bytes == 0,
            missing_bytes,
            files,
        }
    }
}

/// Check `downloaded` against `pinned`, then move it to `target` in one
/// step; a refused file stays where it is.
fn install_file(pinned: &ModelFile, downloaded: &Path, target: &Path) -> Result<(), CoreError> {
    mdoc_import::model_download::verify(downloaded, pinned.bytes, &pinned.sha256)?;
    let parent = target.parent().expect("a model file has a folder");
    std::fs::create_dir_all(parent)?;
    if std::fs::rename(downloaded, target).is_err() {
        // Another volume: copy beside the target, then rename in place.
        let partial = target.with_extension("part");
        std::fs::copy(downloaded, &partial)?;
        std::fs::File::open(&partial)?.sync_all()?;
        std::fs::rename(&partial, target)?;
        std::fs::remove_file(downloaded)?;
    }
    Ok(())
}

/// The pinned files: the detector's manifest, or the OCR bundle's. The
/// runtimes are linked into the app, so none is downloaded.
fn files(model: ModelId) -> Vec<ModelFile> {
    match model.ocr() {
        None => mdoc_detect::manifest_for(PiiModel::Fp16)
            .files
            .into_iter()
            .map(|f| ModelFile {
                path: f.path,
                url: f.url,
                sha256: f.sha256,
                bytes: f.bytes,
            })
            .collect(),
        Some(ocr) => mdoc_import::ocr::manifest::for_model(ocr)
            .model
            .artifacts
            .iter()
            .map(|a| ModelFile {
                path: a.filename.into(),
                url: a.url.into(),
                sha256: a.sha256.into(),
                bytes: a.size,
            })
            .collect(),
    }
}

/// The native libraries the core loads.
#[derive(Clone, Debug, serde::Serialize, uniffi::Record)]
pub struct Runtimes {
    pub pdfium: String,
    pub onnx: String,
}

/// The runtimes that ship inside the app. iOS: linked into it. Android: the
/// APK's own `libpdfium.so` and `libonnxruntime.so` beside this library in
/// the app's native library folder, which needs the APK to extract its
/// libraries (`useLegacyPackaging`): PDFium is opened by file path. Elsewhere
/// there are none, and callers pass the desktop's
/// downloaded libraries instead.
#[uniffi::export]
pub fn linked_runtimes() -> Result<Runtimes, CoreError> {
    #[cfg(target_os = "ios")]
    {
        let linked = mdoc_import::ocr::linked_runtimes()?;
        Ok(Runtimes {
            pdfium: linked.pdfium.display().to_string(),
            onnx: linked.onnx.display().to_string(),
        })
    }
    #[cfg(target_os = "android")]
    {
        let dir = android::library_dir().ok_or_else(|| CoreError::Ocr {
            message: "The app's native libraries are not extracted.".into(),
        })?;
        let runtimes = Runtimes {
            pdfium: dir.join("libpdfium.so").display().to_string(),
            onnx: dir.join("libonnxruntime.so").display().to_string(),
        };
        for library in [&runtimes.pdfium, &runtimes.onnx] {
            if !Path::new(library).is_file() {
                return Err(CoreError::Ocr {
                    message: format!(
                        "{library} is missing: the app's native libraries are not extracted."
                    ),
                });
            }
        }
        Ok(runtimes)
    }
    #[cfg(not(any(target_os = "ios", target_os = "android")))]
    Err(CoreError::Ocr {
        message: "Runtimes ship inside the app only on iOS and Android.".into(),
    })
}

/// Where this library was loaded from, as the dynamic linker reports it.
#[cfg(target_os = "android")]
#[allow(unsafe_code)] // dladdr (#897).
mod android {
    use std::{
        ffi::{CStr, c_char, c_int, c_void},
        path::PathBuf,
    };

    #[repr(C)]
    struct DlInfo {
        fname: *const c_char,
        fbase: *mut c_void,
        sname: *const c_char,
        saddr: *mut c_void,
    }

    unsafe extern "C" {
        fn dladdr(address: *const c_void, info: *mut DlInfo) -> c_int;
    }

    /// The folder holding `libmdoc_ffi.so`; `None` when the linker reports
    /// none, or a path inside the APK (libraries not extracted).
    pub fn library_dir() -> Option<PathBuf> {
        let mut info = DlInfo {
            fname: std::ptr::null(),
            fbase: std::ptr::null_mut(),
            sname: std::ptr::null(),
            saddr: std::ptr::null_mut(),
        };
        let address = library_dir as *const c_void;
        // SAFETY: dladdr only reads the address and fills `info`; fname, when
        // set, points to the linker's own NUL-terminated string.
        if unsafe { dladdr(address, &mut info) } == 0 || info.fname.is_null() {
            return None;
        }
        // SAFETY: checked non-null above; the linker keeps it alive.
        let file = PathBuf::from(unsafe { CStr::from_ptr(info.fname) }.to_str().ok()?);
        file.is_file()
            .then(|| file.parent().map(PathBuf::from))
            .flatten()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::test_support::Scratch;

    const GIB: u64 = 1024 * MIB;

    fn runtimes() -> Runtimes {
        Runtimes {
            pdfium: "pdfium".into(),
            onnx: "onnx".into(),
        }
    }

    #[test]
    fn the_catalog_sizes_come_from_the_pinned_manifests() {
        let scratch = Scratch::new();
        let setup = Setup::new(scratch.path().display().to_string(), 12 * GIB);
        let catalog = setup.catalog();
        assert_eq!(catalog.len(), 3);
        let detector = &catalog[0];
        let manifest = mdoc_detect::manifest_for(PiiModel::Fp16);
        assert_eq!(
            detector.bytes,
            manifest.files.iter().map(|f| f.bytes).sum::<u64>()
        );
        assert_eq!(detector.files.len(), manifest.files.len());
        for model in &catalog {
            assert!(model.bytes > 0 && model.files.iter().all(|f| f.sha256.len() == 64));
            assert!(!model.installed);
            assert_eq!(model.missing_bytes, model.bytes);
        }
        assert_eq!(
            catalog[2].title,
            mdoc_import::ocr::title(OcrModel::Cyrillic)
        );
    }

    #[test]
    fn the_detector_needs_a_four_gigabyte_device_and_ocr_runs_everywhere() {
        let offered = |ram: u64| {
            Setup::new("models".into(), ram)
                .catalog()
                .into_iter()
                .filter(|m| m.offered)
                .map(|m| m.id)
                .collect::<Vec<_>>()
        };
        let ocr = [ModelId::OcrEnglish, ModelId::OcrEnglishRussian];
        // A 3 GB phone (the iPhone SE 2nd generation) reports under 3 GiB.
        assert_eq!(offered(2 * GIB + 800 * MIB), ocr);
        assert_eq!(offered(3 * GIB), ocr);
        // A 4 GB phone reports about 3.6 GiB; a 6 GB iPhone 12 Pro 5.6 GiB.
        assert_eq!(offered(3 * GIB + 600 * MIB).len(), 3);
        assert_eq!(offered(5 * GIB + 600 * MIB).len(), 3);
        assert_eq!(
            Setup::new("models".into(), 3 * GIB).below_floor(),
            Some(FloorReason::Memory)
        );
    }

    /// A timed run past the ceiling puts the device below the floor;
    /// another model's record does not count.
    #[test]
    fn a_device_past_the_speed_ceiling_offers_no_detector() {
        let scratch = Scratch::new();
        let root = scratch.path().join("models");
        let setup = Setup::new(root.display().to_string(), 6 * GIB);
        assert_eq!(setup.below_floor(), None);
        assert_eq!(setup.ten_pages_seconds(), None);
        let slow = DetectorSpeed {
            load_seconds: 5.,
            seconds_per_thousand_chars: 70.,
        };
        speed::write(&root, "another-model", slow).unwrap();
        assert_eq!(setup.below_floor(), None);
        speed::write(&root, &setup.detector_folder_name(), slow).unwrap();
        assert_eq!(setup.below_floor(), Some(FloorReason::Speed));
        assert!(!setup.offers(ModelId::Gliner2Fp16));
        assert!(setup.offers(ModelId::OcrEnglish));
        // 10 pages are 30,000 characters.
        assert_eq!(setup.ten_pages_seconds(), Some(2105));
        let fast = DetectorSpeed {
            load_seconds: 2.,
            seconds_per_thousand_chars: 2.,
        };
        speed::write(&root, &setup.detector_folder_name(), fast).unwrap();
        assert!(setup.offers(ModelId::Gliner2Fp16));
        assert_eq!(setup.ten_pages_seconds(), Some(62));
    }

    #[test]
    fn timing_needs_an_installed_detector_and_a_cancel_lasts_one_run() {
        let scratch = Scratch::new();
        let setup = Setup::new(scratch.path().display().to_string(), 6 * GIB);
        setup.cancel_timing();
        assert_eq!(setup.time_detector(runtimes(), None).unwrap(), None);
        assert!(
            !setup.timing_cancelled.load(Ordering::Relaxed),
            "the next run starts uncancelled"
        );
    }

    /// A phone short of free memory is not timed: the timed run would load
    /// the detector as a scan does.
    #[test]
    fn timing_needs_the_memory_a_scan_needs() {
        use crate::document_session::{JobProgress, JobStage, Thermal};
        struct Short;
        impl JobProgress for Short {
            fn stage(&self, _: JobStage) {}
            fn advanced(&self, _: u64, _: u64) {}
            fn remaining(&self, _: u64) {}
            fn thermal(&self) -> Thermal {
                Thermal::Nominal
            }
            fn free_memory(&self) -> Option<u64> {
                Some(DETECTOR_FREE_BYTES / 2)
            }
        }
        let scratch = Scratch::new();
        let setup = Setup::new(scratch.path().join("models").display().to_string(), 6 * GIB);
        let folder = setup.folder(ModelId::Gliner2Fp16);
        for file in &mdoc_detect::manifest_for(PiiModel::Fp16).files {
            let path = folder.join(&file.path);
            std::fs::create_dir_all(path.parent().unwrap()).unwrap();
            std::fs::File::create(&path)
                .unwrap()
                .set_len(file.bytes)
                .unwrap();
        }
        assert!(setup.info(ModelId::Gliner2Fp16).installed);
        let refused = setup.time_detector(runtimes(), Some(Box::new(Short)));
        assert!(
            matches!(&refused, Err(CoreError::Detect { message }) if message.contains("memory")),
            "{refused:?}"
        );
        assert_eq!(setup.detector_speed(), None);
    }

    #[test]
    fn a_wrong_file_is_refused_and_left_where_it_is() {
        let scratch = Scratch::new();
        let setup = Setup::new(scratch.path().join("models").display().to_string(), 0);
        let model = ModelId::OcrEnglish;
        let pinned = files(model).remove(0);
        let download = scratch.path().join("download.bin");
        // A wrong file of the right size is refused and stays where it was.
        std::fs::write(&download, vec![0u8; pinned.bytes as usize]).unwrap();
        let refused = setup.install(model, pinned.path.clone(), download.display().to_string());
        assert!(
            matches!(refused, Err(CoreError::Setup { .. })),
            "{refused:?}"
        );
        assert!(download.exists());
        let folder = setup.folder(model);
        assert!(!folder.join(&pinned.path).exists());
        // A short file is refused for its size before hashing.
        std::fs::write(&download, b"short").unwrap();
        assert!(matches!(
            setup.install(model, pinned.path.clone(), download.display().to_string()),
            Err(CoreError::Setup { .. })
        ));
        // A file the model does not have is refused outright.
        assert!(matches!(
            setup.install(model, "other.onnx".into(), download.display().to_string()),
            Err(CoreError::Invalid { .. })
        ));
        assert_eq!(setup.ocr_paths(model, runtimes()), None);
    }

    /// The detector reads as installed with its encoder in either layout:
    /// the upstream file, or the re-saved model and data pair (#905).
    #[test]
    fn the_detector_is_installed_with_either_encoder_layout() {
        let scratch = Scratch::new();
        let setup = Setup::new(
            scratch.path().join("models").display().to_string(),
            12 * GIB,
        );
        let folder = setup.folder(ModelId::Gliner2Fp16);
        let manifest = mdoc_detect::manifest_for(PiiModel::Fp16);
        // Sparse files at the pinned sizes: presence is checked by size.
        let sized = |path: &Path, bytes: u64| {
            std::fs::create_dir_all(path.parent().unwrap()).unwrap();
            std::fs::File::create(path).unwrap().set_len(bytes).unwrap();
        };
        let mut encoder = None;
        for file in &manifest.files {
            match &file.external_data {
                None => sized(&folder.join(&file.path), file.bytes),
                Some(pins) => encoder = Some((file, pins)),
            }
        }
        let (encoder, pins) = encoder.expect("the encoder has a re-saved layout");
        let info = setup.info(ModelId::Gliner2Fp16);
        assert_eq!(info.missing_bytes, encoder.bytes);
        let path = folder.join(&encoder.path);
        sized(&path, pins.model.bytes);
        // The model without its data file is still to download.
        assert_eq!(
            setup.info(ModelId::Gliner2Fp16).missing_bytes,
            encoder.bytes
        );
        sized(&mdoc_detect::external::data_path(&path), pins.data.bytes);
        assert!(setup.info(ModelId::Gliner2Fp16).installed);
        assert!(setup.detector_paths(runtimes()).is_some());
        sized(&path, encoder.bytes);
        assert!(setup.info(ModelId::Gliner2Fp16).installed);
    }

    /// A file matching its pin moves into the folder the desktop layout
    /// names, replacing a stale copy, and counts as present.
    #[test]
    fn a_verified_file_moves_into_the_desktop_layout() {
        let scratch = Scratch::new();
        let setup = Setup::new(scratch.path().join("models").display().to_string(), 0);
        let content = b"pinned bytes";
        let mut pinned = files(ModelId::OcrEnglish).remove(0);
        pinned.bytes = content.len() as u64;
        pinned.sha256 = "977d59924a4dfd9d4e93c74467f3253786327c8c28e69a75f30cf0429dab77c7".into();
        let target = setup.folder(ModelId::OcrEnglish).join(&pinned.path);
        std::fs::create_dir_all(target.parent().unwrap()).unwrap();
        std::fs::write(&target, b"stale").unwrap();
        let download = scratch.path().join("download.bin");
        std::fs::write(&download, content).unwrap();
        install_file(&pinned, &download, &target).unwrap();
        assert!(!download.exists());
        assert_eq!(std::fs::read(&target).unwrap(), content);
        assert!(target.starts_with(scratch.path().join("models").join("ocr").join("models")));
        assert!(!mdoc_import::model_download::missing(&target, pinned.bytes));
    }
}
