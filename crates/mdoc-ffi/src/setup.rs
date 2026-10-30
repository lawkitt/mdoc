//! `Setup` (ADR 0085): the models a device may offer, from the desktop's
//! pinned manifests, and installing a file the platform downloaded: SHA-256
//! against the pinned digest, then an atomic move into the model folder.
//! The platform runs the download; nothing here uses the network.
use crate::{CoreError, spike::Runtimes};
use mdoc_store::settings::{OcrModel, PiiModel};
use std::{
    path::{Path, PathBuf},
    sync::Arc,
};

/// GLiNER2 FP16's RAM floor: devices sold as 8 GB and up (ADR 0085's floor
/// rule, until the spike's device rows tune it). Devices report a little
/// less than their nominal RAM (an 8 GB iPhone about 7.5 GiB, a 6 GB one
/// about 5.6 GiB), so the cut sits between the two classes.
pub const DETECTOR_FLOOR_BYTES: u64 = 7 * 1024 * 1024 * 1024;

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

#[derive(uniffi::Object)]
pub struct Setup {
    root: PathBuf,
    physical_memory: u64,
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
        })
    }

    /// Every model, offered or not, in setup order.
    pub fn catalog(&self) -> Vec<ModelInfo> {
        ModelId::ALL.into_iter().map(|id| self.info(id)).collect()
    }

    /// Whether this device may offer `model` (ADR 0085's floor rule).
    pub fn offers(&self, model: ModelId) -> bool {
        model.ocr().is_some() || self.physical_memory >= DETECTOR_FLOOR_BYTES
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
        Ok(self.info(model))
    }

    /// The detector's paths once it is installed and offered here.
    pub fn detector_paths(&self, runtimes: Runtimes) -> Option<DetectorPaths> {
        let model = ModelId::Gliner2Fp16;
        (self.offers(model) && self.info(model).installed).then(|| DetectorPaths {
            model: self.folder(model).display().to_string(),
            onnx: runtimes.onnx,
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
    /// The desktop's layouts under the models root: `pseudonymization/` as
    /// `mdoc_detect::root_for` names its folder, `ocr/` as OCR's root.
    fn folder(&self, model: ModelId) -> PathBuf {
        match model.ocr() {
            None => {
                let manifest = mdoc_detect::manifest_for(PiiModel::Fp16);
                self.root.join("pseudonymization").join(format!(
                    "{}-{}",
                    manifest.id,
                    &manifest.revision[..8]
                ))
            }
            Some(ocr) => mdoc_import::ocr::model_root(&self.ocr_root(), ocr),
        }
    }

    fn ocr_root(&self) -> PathBuf {
        self.root.join("ocr")
    }

    fn info(&self, id: ModelId) -> ModelInfo {
        let files = files(id);
        let folder = self.folder(id);
        let missing_bytes = files
            .iter()
            .filter(|f| mdoc_import::model_download::missing(&folder.join(&f.path), f.bytes))
            .map(|f| f.bytes)
            .sum();
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
/// runtimes are linked into the app, so none is downloaded (ADR 0085).
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::test_support::Scratch;

    const GIB: u64 = 1024 * 1024 * 1024;

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
    fn the_detector_needs_an_eight_gigabyte_device_and_ocr_runs_everywhere() {
        let offered = |ram: u64| {
            Setup::new("models".into(), ram)
                .catalog()
                .into_iter()
                .filter(|m| m.offered)
                .map(|m| m.id)
                .collect::<Vec<_>>()
        };
        let ocr = [ModelId::OcrEnglish, ModelId::OcrEnglishRussian];
        // An iPhone 12 Pro (6 GB) reports about 5.6 GiB; a 4 GB phone less.
        assert_eq!(offered(3 * GIB), ocr);
        assert_eq!(offered(5 * GIB + 600 * 1024 * 1024), ocr);
        // An 8 GB iPhone reports about 7.5 GiB; a 12 GB one about 11.
        assert_eq!(offered(7 * GIB + GIB / 2)[0], ModelId::Gliner2Fp16);
        assert_eq!(offered(11 * GIB).len(), 3);
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
