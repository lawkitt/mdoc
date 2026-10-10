//! Application preferences only. No document text, QA results or replacement maps.
use pdf_inspector::vision::{ModelManifest, PP_OCR_CYRILLIC, PP_OCR_V6_SMALL};
use serde::{Deserialize, Serialize};
use std::{
    cell::RefCell,
    io::{Read, Write},
    path::{Path, PathBuf},
    rc::Rc,
};

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub enum OcrModel {
    Cyrillic,
    /// English-first default (ADR 0026).
    #[default]
    V6Small,
}
impl OcrModel {
    /// Recommended first.
    pub const ALL: [Self; 2] = [Self::V6Small, Self::Cyrillic];
    pub fn manifest(self) -> &'static ModelManifest {
        match self {
            Self::Cyrillic => &PP_OCR_CYRILLIC,
            Self::V6Small => &PP_OCR_V6_SMALL,
        }
    }
    pub fn name(self) -> &'static str {
        match self {
            Self::Cyrillic => "PP-OCRv5 Cyrillic + v6 detector",
            Self::V6Small => "PP-OCRv6 Small",
        }
    }
    /// Plain-language row title.
    pub fn title(self) -> &'static str {
        match self {
            Self::V6Small => "English",
            Self::Cyrillic => "English & Russian",
        }
    }
    /// Languages the bundle recognizes, for setup hints.
    pub fn languages(self) -> &'static str {
        match self {
            Self::V6Small => "English",
            Self::Cyrillic => "English and Russian",
        }
    }
    pub fn summary(self) -> &'static str {
        match self {
            Self::V6Small => "Exact on English test scans",
            Self::Cyrillic => "For documents with Russian text",
        }
    }
    pub fn evidence(self) -> &'static str {
        match self {
            Self::Cyrillic => {
                "EN/RU synthetic transcripts passed on macOS and Windows; real scans require review."
            }
            Self::V6Small => {
                "QA alternative: English synthetic transcript passed; Russian recognition failed."
            }
        }
    }
}
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub enum PiiModel {
    #[default]
    Fp16,
    Fp32,
}
impl PiiModel {
    pub const ALL: [Self; 2] = [Self::Fp16, Self::Fp32];
    pub fn name(self) -> &'static str {
        match self {
            Self::Fp16 => "GLiNER2 PII FP16",
            Self::Fp32 => "GLiNER2 PII FP32",
        }
    }
    pub fn folder(self) -> &'static str {
        match self {
            Self::Fp16 => "fp16_v2",
            Self::Fp32 => "fp32_v2",
        }
    }
    pub fn precision(self) -> gliner2_rs::Precision {
        match self {
            Self::Fp16 => gliner2_rs::Precision::Fp16,
            Self::Fp32 => gliner2_rs::Precision::Fp32,
        }
    }
    pub fn engine(self) -> &'static str {
        "gliner2-rs 0.9.6"
    }
    pub fn languages(self) -> &'static str {
        "English, French, Spanish, German, Italian, Portuguese, Dutch; Russian is exploratory"
    }
    pub fn description(self) -> &'static str {
        match self {
            Self::Fp16 => {
                "Flexible GLiNER2 span detector. Half-precision weights; current reference model."
            }
            Self::Fp32 => {
                "The same GLiNER2 checkpoint with full-precision weights. Larger download and memory use; not established as more accurate."
            }
        }
    }
    pub fn title(self) -> &'static str {
        match self {
            Self::Fp16 => "Standard",
            Self::Fp32 => "Full precision",
        }
    }
    pub fn summary(self) -> &'static str {
        match self {
            Self::Fp16 => "About 2 GB of memory while scanning",
            Self::Fp32 => "Slower, more memory; not shown to be more accurate",
        }
    }
    pub fn evidence(self) -> &'static str {
        "Known EN/RU misses; neither precision is qualified. Review the complete document."
    }
    pub fn license(self) -> &'static str {
        "Apache-2.0; encoder MIT"
    }
    pub fn hugging_face_url(self) -> &'static str {
        "https://huggingface.co/fastino/gliner2-privacy-filter-PII-multi"
    }
}
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum Model {
    Ocr(OcrModel),
    Pii(PiiModel),
}
impl Model {
    pub const ALL: [Self; 4] = [
        Self::Ocr(OcrModel::V6Small),
        Self::Ocr(OcrModel::Cyrillic),
        Self::Pii(PiiModel::Fp16),
        Self::Pii(PiiModel::Fp32),
    ];
    pub fn name(self) -> &'static str {
        match self {
            Self::Ocr(m) => m.name(),
            Self::Pii(m) => m.name(),
        }
    }
    pub fn title(self) -> &'static str {
        match self {
            Self::Ocr(m) => m.title(),
            Self::Pii(m) => m.title(),
        }
    }
    pub fn summary(self) -> &'static str {
        match self {
            Self::Ocr(m) => m.summary(),
            Self::Pii(m) => m.summary(),
        }
    }
    /// The bundle's application default, shown as Recommended.
    pub fn recommended(self) -> bool {
        match self {
            Self::Ocr(m) => m == OcrModel::default(),
            Self::Pii(m) => m == PiiModel::default(),
        }
    }
    /// How setup prompts name the bundle, e.g. "English OCR".
    pub fn short_name(self) -> String {
        match self {
            Self::Ocr(m) => format!("{} OCR", m.title()),
            Self::Pii(m) => format!("{} model", m.title()),
        }
    }
    pub fn pending(self) -> crate::model_download::Pending {
        match self {
            Self::Ocr(m) => crate::ocr::pending(m),
            Self::Pii(m) => crate::pii::detector::pending(m),
        }
    }
}
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct OcrConfig {
    pub model: OcrModel,
    pub dpi: u16,
    pub minimum_confidence: f32,
}
impl Default for OcrConfig {
    fn default() -> Self {
        Self {
            model: OcrModel::default(),
            dpi: 150,
            minimum_confidence: 0.,
        }
    }
}
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PiiConfig {
    pub model: PiiModel,
    pub threshold: f32,
}
impl Default for PiiConfig {
    fn default() -> Self {
        Self {
            model: PiiModel::default(),
            threshold: 0.5,
        }
    }
}
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Preferences {
    pub version: u32,
    pub ocr: OcrConfig,
    pub pseudonymization: PiiConfig,
}
impl Default for Preferences {
    fn default() -> Self {
        Self {
            version: 1,
            ocr: OcrConfig::default(),
            pseudonymization: PiiConfig::default(),
        }
    }
}
pub fn unit_interval(value: f32) -> bool {
    value.is_finite() && (0. ..=1.).contains(&value)
}
impl Preferences {
    pub fn validate(&self) -> Result<(), String> {
        if self.version != 1
            || ![150, 200, 300].contains(&self.ocr.dpi)
            || !unit_interval(self.ocr.minimum_confidence)
            || !unit_interval(self.pseudonymization.threshold)
        {
            return Err("Invalid or obsolete settings. Open Settings and explicitly reset or choose supported values.".into());
        }
        Ok(())
    }
}
pub type Shared = Rc<RefCell<Store>>;
pub struct Store {
    pub current: Option<Preferences>,
    pub error: Option<String>,
    pub path: Option<PathBuf>,
}
impl Store {
    pub fn new() -> Shared {
        let path = if cfg!(test) {
            None
        } else {
            dirs::data_local_dir().map(|p| p.join("mdoc/settings.json"))
        };
        let loaded = match &path {
            Some(path) => load(path),
            None if cfg!(test) => Ok(Preferences::default()),
            None => Err("Could not find local application storage for settings.".into()),
        };
        let (current, error) = match loaded {
            Ok(p) => (Some(p), None),
            Err(e) => (None, Some(e)),
        };
        Rc::new(RefCell::new(Self {
            current,
            error,
            path,
        }))
    }
    pub fn snapshot(&self) -> Result<Preferences, String> {
        self.current.clone().ok_or_else(|| {
            self.error
                .clone()
                .unwrap_or_else(|| "Open Settings to select supported preferences.".into())
        })
    }
}
pub fn load(path: &Path) -> Result<Preferences, String> {
    let file = match std::fs::File::open(path) {
        Ok(f) => f,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(Preferences::default()),
        Err(e) => return Err(e.to_string()),
    };
    let mut bytes = Vec::new();
    file.take(65537)
        .read_to_end(&mut bytes)
        .map_err(|e| e.to_string())?;
    if bytes.len() > 65536 {
        return Err("Settings file is too large. Reset explicitly in Settings.".into());
    }
    let prefs: Preferences = serde_json::from_slice(&bytes)
        .map_err(|e| format!("Could not read settings: {e}. Reset explicitly in Settings."))?;
    prefs.validate()?;
    Ok(prefs)
}
pub fn save(path: &Path, prefs: &Preferences) -> Result<(), String> {
    prefs.validate()?;
    let parent = path.parent().ok_or("Settings path has no parent.")?;
    std::fs::create_dir_all(parent).map_err(|e| e.to_string())?;
    let mut file = tempfile::NamedTempFile::new_in(parent).map_err(|e| e.to_string())?;
    serde_json::to_writer_pretty(&mut file, prefs).map_err(|e| e.to_string())?;
    file.flush().map_err(|e| e.to_string())?;
    file.as_file().sync_all().map_err(|e| e.to_string())?;
    file.persist(path).map_err(|e| e.to_string())?;
    Ok(())
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn settings_roundtrip_and_invalid_values_preserve_previous_file() {
        let d = tempfile::tempdir().unwrap();
        let path = d.path().join("settings.json");
        assert_eq!(load(&path).unwrap(), Preferences::default());
        let mut p = Preferences::default();
        p.ocr.model = OcrModel::V6Small;
        p.ocr.dpi = 300;
        save(&path, &p).unwrap();
        assert_eq!(load(&path).unwrap(), p);
        p.pseudonymization.threshold = f32::NAN;
        assert!(save(&path, &p).is_err());
        assert_eq!(load(&path).unwrap().ocr.dpi, 300);
        std::fs::write(&path, b"{\"version\":9}").unwrap();
        assert!(load(&path).is_err());
    }
    #[test]
    fn defaults_are_english_first_and_stored_choices_are_kept() {
        let defaults = Preferences::default();
        assert_eq!(defaults.ocr.model, OcrModel::V6Small);
        assert_eq!(defaults.pseudonymization.model, PiiModel::Fp16);
        assert!(Model::Ocr(OcrModel::V6Small).recommended());
        assert!(!Model::Pii(PiiModel::Fp32).recommended());
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("settings.json");
        let mut stored = Preferences::default();
        stored.ocr.model = OcrModel::Cyrillic;
        save(&path, &stored).unwrap();
        assert_eq!(load(&path).unwrap().ocr.model, OcrModel::Cyrillic);
    }
    #[test]
    fn every_pii_model_roundtrips_without_changing_existing_defaults() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("settings.json");
        for model in PiiModel::ALL {
            let mut prefs = Preferences::default();
            prefs.pseudonymization.model = model;
            save(&path, &prefs).unwrap();
            assert_eq!(load(&path).unwrap(), prefs);
        }
        assert_eq!(
            serde_json::from_str::<PiiModel>("\"Fp16\"").unwrap(),
            PiiModel::default()
        );
        assert_eq!(
            serde_json::from_str::<PiiModel>("\"Fp32\"").unwrap(),
            PiiModel::Fp32
        );
    }
    #[test]
    fn removed_models_require_explicit_reset_without_rewriting_preferences() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("settings.json");
        for removed in ["HorizonSmallQ8", "NymBaseCompressed", "OpenAiPrivacyQ4"] {
            let mut stored = serde_json::to_value(Preferences::default()).unwrap();
            stored["pseudonymization"]["model"] = removed.into();
            let bytes = serde_json::to_vec(&stored).unwrap();
            std::fs::write(&path, &bytes).unwrap();
            let error = load(&path).unwrap_err();
            assert!(error.contains("Reset explicitly in Settings"));
            assert_eq!(std::fs::read(&path).unwrap(), bytes);

            save(&path, &Preferences::default()).unwrap();
            assert_eq!(load(&path).unwrap(), Preferences::default());
        }
    }
}
