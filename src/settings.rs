//! Application preferences only. No document text, QA results or replacement maps.
//! The user's spelling dictionary is a word list beside the settings file.
use pdf_inspector::vision::{ModelManifest, PP_OCR_CYRILLIC, PP_OCR_V6_SMALL};
use serde::{Deserialize, Serialize};
use std::{
    cell::RefCell,
    collections::BTreeSet,
    io::{Read, Write},
    path::{Path, PathBuf},
    rc::Rc,
    sync::{
        Arc, Mutex,
        atomic::{AtomicU64, Ordering},
    },
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
/// Spellcheck choices (ADR 0036). The per-document switch is session-only
/// and lives in the document view, not here.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SpellingConfig {
    pub enabled: bool,
    pub english: bool,
    pub russian: bool,
    pub check_all_caps: bool,
    pub check_digits: bool,
}
impl Default for SpellingConfig {
    fn default() -> Self {
        Self {
            enabled: true,
            english: true,
            russian: true,
            check_all_caps: false,
            check_digits: false,
        }
    }
}
impl SpellingConfig {
    /// What to check, or `None` when nothing would be checked.
    pub fn options(self) -> Option<mdoc_spell::Options> {
        (self.enabled && (self.english || self.russian)).then_some(mdoc_spell::Options {
            english: self.english,
            russian: self.russian,
            check_all_caps: self.check_all_caps,
            check_digits: self.check_digits,
        })
    }
}
/// Update check choices (ADR 0038). Pre-release builds include pre-releases
/// regardless of `include_prereleases`.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct UpdatesConfig {
    pub check_automatically: bool,
    pub include_prereleases: bool,
}
impl Default for UpdatesConfig {
    fn default() -> Self {
        Self {
            check_automatically: true,
            include_prereleases: false,
        }
    }
}
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Preferences {
    pub version: u32,
    pub ocr: OcrConfig,
    pub pseudonymization: PiiConfig,
    /// Absent in settings saved before spellcheck existed.
    #[serde(default)]
    pub spelling: SpellingConfig,
    /// Absent in settings saved before the update check existed.
    #[serde(default)]
    pub updates: UpdatesConfig,
}
impl Default for Preferences {
    fn default() -> Self {
        Self {
            version: 1,
            ocr: OcrConfig::default(),
            pseudonymization: PiiConfig::default(),
            spelling: SpellingConfig::default(),
            updates: UpdatesConfig::default(),
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
    pub words: UserWords,
}

/// Words added with "Add to dictionary": exact spellings, one per line in
/// `spelling-words.txt` beside the settings file, shared by all documents.
pub struct UserWords {
    pub words: Arc<BTreeSet<String>>,
    /// Bumped on every change, so spellcheck caches know to recheck.
    pub generation: u64,
    pub path: Option<PathBuf>,
    pub error: Option<String>,
}

pub const MAX_WORDS: usize = 10_000;
const MAX_WORD_CHARS: usize = 64;

/// A single word worth storing: no whitespace or control characters, at most
/// 64 characters, and at least one letter.
pub fn valid_word(word: &str) -> bool {
    !word.is_empty()
        && word.chars().count() <= MAX_WORD_CHARS
        && !word.chars().any(|c| c.is_whitespace() || c.is_control())
        && word.chars().any(char::is_alphabetic)
}

pub fn load_words(path: &Path) -> Result<BTreeSet<String>, String> {
    let file = match std::fs::File::open(path) {
        Ok(f) => f,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(BTreeSet::new()),
        Err(e) => return Err(e.to_string()),
    };
    let mut text = String::new();
    file.take(1 << 20)
        .read_to_string(&mut text)
        .map_err(|e| format!("Could not read your spelling dictionary: {e}"))?;
    Ok(text
        .lines()
        .map(str::trim)
        .filter(|w| valid_word(w))
        .take(MAX_WORDS)
        .map(str::to_owned)
        .collect())
}

pub fn save_words(path: &Path, words: &BTreeSet<String>) -> Result<(), String> {
    let parent = path.parent().ok_or("Dictionary path has no parent.")?;
    std::fs::create_dir_all(parent).map_err(|e| e.to_string())?;
    let mut file = tempfile::NamedTempFile::new_in(parent).map_err(|e| e.to_string())?;
    for word in words {
        writeln!(file, "{word}").map_err(|e| e.to_string())?;
    }
    file.as_file().sync_all().map_err(|e| e.to_string())?;
    file.persist(path).map_err(|e| e.to_string())?;
    Ok(())
}

/// Serializes dictionary writes; a write for an older generation is skipped.
static WORDS_WRITE: Mutex<()> = Mutex::new(());
static WORDS_LATEST: AtomicU64 = AtomicU64::new(0);

impl Store {
    /// Replace the user's words and save them in the background. Returns
    /// `false` (changing nothing) when the list is full.
    pub fn set_words(shared: &Shared, words: BTreeSet<String>, cx: &mut gpui::App) -> bool {
        if words.len() > MAX_WORDS {
            shared.borrow_mut().words.error =
                Some(format!("Your dictionary is full ({MAX_WORDS} words)."));
            return false;
        }
        let (path, words, generation) = {
            let mut store = shared.borrow_mut();
            store.words.words = Arc::new(words);
            store.words.generation += 1;
            (
                store.words.path.clone(),
                store.words.words.clone(),
                store.words.generation,
            )
        };
        let Some(path) = path else {
            return true;
        };
        WORDS_LATEST.store(generation, Ordering::SeqCst);
        let task = cx.background_executor().spawn(async move {
            let _guard = WORDS_WRITE.lock().unwrap_or_else(|e| e.into_inner());
            if WORDS_LATEST.load(Ordering::SeqCst) != generation {
                return Ok(());
            }
            save_words(&path, &words)
        });
        let shared = shared.clone();
        cx.spawn(async move |_| {
            let result = task.await;
            shared.borrow_mut().words.error = result
                .err()
                .map(|e| format!("Could not save your spelling dictionary: {e}"));
        })
        .detach();
        true
    }
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
        let words_path = path
            .as_ref()
            .map(|p| p.with_file_name("spelling-words.txt"));
        let (words, words_error) = match words_path.as_deref().map(load_words) {
            Some(Ok(words)) => (words, None),
            Some(Err(e)) => (BTreeSet::new(), Some(e)),
            None => (BTreeSet::new(), None),
        };
        Rc::new(RefCell::new(Self {
            current,
            error,
            path,
            words: UserWords {
                words: Arc::new(words),
                generation: 0,
                path: words_path,
                error: words_error,
            },
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
    fn user_words_roundtrip_and_skip_invalid_lines() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("spelling-words.txt");
        assert!(load_words(&path).unwrap().is_empty());
        let words: BTreeSet<String> = ["цессионарий", "Kowalczyk", "co-signer"]
            .map(String::from)
            .into();
        save_words(&path, &words).unwrap();
        assert_eq!(load_words(&path).unwrap(), words);
        std::fs::write(&path, "good\n\ntwo words\n123\n  trimmed  \n").unwrap();
        let loaded: Vec<_> = load_words(&path).unwrap().into_iter().collect();
        assert_eq!(loaded, ["good", "trimmed"]);
        assert!(!valid_word(&"a".repeat(65)));
    }
    #[test]
    fn spelling_defaults_apply_to_older_files_and_choices_roundtrip() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("settings.json");
        let mut older = serde_json::to_value(Preferences::default()).unwrap();
        older.as_object_mut().unwrap().remove("spelling");
        std::fs::write(&path, serde_json::to_vec(&older).unwrap()).unwrap();
        let loaded = load(&path).unwrap();
        assert_eq!(loaded.spelling, SpellingConfig::default());
        assert!(loaded.spelling.enabled && !loaded.spelling.check_all_caps);

        let mut prefs = Preferences::default();
        prefs.spelling.russian = false;
        prefs.spelling.check_digits = true;
        save(&path, &prefs).unwrap();
        assert_eq!(load(&path).unwrap(), prefs);
        prefs.spelling.english = false;
        assert_eq!(prefs.spelling.options(), None);
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
