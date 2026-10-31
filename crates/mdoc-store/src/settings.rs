//! Application preferences only. No document text, QA results or replacement maps.
//! The user's spelling dictionary is a word list beside the settings file.
//! Models are plain IDs here; their manifests, languages and downloads live
//! with their engines (ADR 0069).
use serde::{Deserialize, Serialize};
use std::{
    cell::RefCell,
    collections::BTreeSet,
    io::{self, Read, Write},
    path::{Path, PathBuf},
    rc::Rc,
    sync::{
        Arc, Mutex,
        atomic::{AtomicU64, Ordering},
    },
};

/// Why settings could not be read or written.
#[derive(Clone, Debug, thiserror::Error)]
pub enum SettingsError {
    #[error(
        "Invalid or obsolete settings. Open Settings and explicitly reset or choose supported values."
    )]
    Invalid,
    #[error("Settings file is too large. Reset explicitly in Settings.")]
    TooLarge,
    #[error("Could not read settings: {0}. Reset explicitly in Settings.")]
    Parse(Arc<serde_json::Error>),
    #[error("Could not find local application storage for settings.")]
    NoStorage,
    /// No preferences were loaded and none chosen yet.
    #[error("Open Settings to select supported preferences.")]
    Unset,
    #[error("Settings path has no parent.")]
    NoParent,
    #[error("{0}")]
    Io(Arc<io::Error>),
    #[error("{0}")]
    Encode(Arc<serde_json::Error>),
    #[error("{0}")]
    Persist(Arc<tempfile::PersistError>),
}
impl From<io::Error> for SettingsError {
    fn from(error: io::Error) -> Self {
        Self::Io(Arc::new(error))
    }
}

/// Why the spelling dictionary could not be read or written.
#[derive(Clone, Debug, thiserror::Error)]
pub enum DictionaryError {
    #[error("{0}")]
    Open(Arc<io::Error>),
    #[error("Could not read your spelling dictionary: {0}")]
    Read(Arc<io::Error>),
    #[error("Your dictionary is full ({MAX_WORDS} words).")]
    Full,
    #[error("Dictionary path has no parent.")]
    NoParent,
    #[error("{0}")]
    Io(Arc<io::Error>),
    #[error("{0}")]
    Persist(Arc<tempfile::PersistError>),
    #[error("Could not save your spelling dictionary: {0}")]
    Save(Box<DictionaryError>),
}
impl From<io::Error> for DictionaryError {
    fn from(error: io::Error) -> Self {
        Self::Io(Arc::new(error))
    }
}

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
    pub fn name(self) -> &'static str {
        match self {
            Self::Cyrillic => "PP-OCRv5 Cyrillic + v6 detector",
            Self::V6Small => "PP-OCRv6 Small",
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
/// Saved-case choices (ADR 0041).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CasesConfig {
    /// Closing a tab with an unsaved replacement mapping asks to save a case.
    pub ask_before_closing: bool,
}
impl Default for CasesConfig {
    fn default() -> Self {
        Self {
            ask_before_closing: true,
        }
    }
}
/// Settings › General choices with no section of their own.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct GeneralConfig {
    /// "Show mdoc in the menu bar" (ADR 0084, macOS): the shelf, and mdoc
    /// keeps running after its last window closes. On by default; the
    /// default, the first-close note and this switch are the consent to
    /// keep running.
    #[serde(default = "on")]
    pub menu_bar: bool,
    /// The first-close note ("mdoc is still in the menu bar.") has shown;
    /// it never shows again.
    #[serde(default)]
    pub menu_bar_note_shown: bool,
    /// A shelf drag has dropped, so the hint "Drag a preview into any app"
    /// is gone for good.
    #[serde(default)]
    pub shelf_drag_hint_shown: bool,
    /// The expanded sidebar's width in points, from dragging its edge;
    /// none: the default. The window clamps it to its range.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub sidebar_width: Option<u16>,
}
fn on() -> bool {
    true
}
impl Default for GeneralConfig {
    fn default() -> Self {
        Self {
            menu_bar: true,
            menu_bar_note_shown: false,
            shelf_drag_hint_shown: false,
            sidebar_width: None,
        }
    }
}
impl GeneralConfig {
    fn is_default(&self) -> bool {
        *self == Self::default()
    }
}
/// Settings › Detection › Categories to detect (ADR 0076): scans of
/// documents without a case skip these, and a new case's list starts here.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CategoriesConfig {
    /// Category tokens scans skip, as in a case's `skip`.
    pub skip: Vec<String>,
}
impl CategoriesConfig {
    fn is_default(&self) -> bool {
        self.skip.is_empty()
    }
    pub fn skipped(&self) -> std::collections::HashSet<mdoc_pii::Category> {
        mdoc_pii::Category::ALL
            .into_iter()
            .filter(|c| self.skip.iter().any(|s| s == c.token()))
            .collect()
    }
    /// Whether scans of documents without a case detect `category`.
    pub fn detects(&self, category: mdoc_pii::Category) -> bool {
        !self.skip.iter().any(|s| s == category.token())
    }
    /// Tick or untick `category`; unticking appends it once.
    pub fn set(&mut self, category: mdoc_pii::Category, detect: bool) {
        let token = category.token();
        if detect {
            self.skip.retain(|s| s != token);
        } else if self.detects(category) {
            self.skip.push(token.to_string());
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
    /// Absent in settings saved before saved cases existed.
    #[serde(default)]
    pub cases: CasesConfig,
    /// Absent while every category is detected, so older files and the
    /// default file stay as they were.
    #[serde(default, skip_serializing_if = "CategoriesConfig::is_default")]
    pub categories: CategoriesConfig,
    /// Absent while every choice is the default, so older files and the
    /// default file stay as they were.
    #[serde(default, skip_serializing_if = "GeneralConfig::is_default")]
    pub general: GeneralConfig,
}
impl Default for Preferences {
    fn default() -> Self {
        Self {
            version: 1,
            ocr: OcrConfig::default(),
            pseudonymization: PiiConfig::default(),
            spelling: SpellingConfig::default(),
            updates: UpdatesConfig::default(),
            cases: CasesConfig::default(),
            categories: CategoriesConfig::default(),
            general: GeneralConfig::default(),
        }
    }
}
pub fn unit_interval(value: f32) -> bool {
    value.is_finite() && (0. ..=1.).contains(&value)
}
impl Preferences {
    pub fn validate(&self) -> Result<(), SettingsError> {
        if self.version != 1
            || ![150, 200, 300].contains(&self.ocr.dpi)
            || !unit_interval(self.ocr.minimum_confidence)
            || !unit_interval(self.pseudonymization.threshold)
        {
            return Err(SettingsError::Invalid);
        }
        Ok(())
    }
}
pub type Shared = Rc<RefCell<Store>>;
pub struct Store {
    pub current: Option<Preferences>,
    pub error: Option<SettingsError>,
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
    pub error: Option<DictionaryError>,
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

pub fn load_words(path: &Path) -> Result<BTreeSet<String>, DictionaryError> {
    let file = match std::fs::File::open(path) {
        Ok(f) => f,
        Err(e) if e.kind() == io::ErrorKind::NotFound => return Ok(BTreeSet::new()),
        Err(e) => return Err(DictionaryError::Open(Arc::new(e))),
    };
    let mut text = String::new();
    file.take(1 << 20)
        .read_to_string(&mut text)
        .map_err(|e| DictionaryError::Read(Arc::new(e)))?;
    Ok(text
        .lines()
        .map(str::trim)
        .filter(|w| valid_word(w))
        .take(MAX_WORDS)
        .map(str::to_owned)
        .collect())
}

pub fn save_words(path: &Path, words: &BTreeSet<String>) -> Result<(), DictionaryError> {
    let parent = path.parent().ok_or(DictionaryError::NoParent)?;
    std::fs::create_dir_all(parent)?;
    let mut file = tempfile::NamedTempFile::new_in(parent)?;
    for word in words {
        writeln!(file, "{word}")?;
    }
    file.as_file().sync_all()?;
    file.persist(path)
        .map_err(|e| DictionaryError::Persist(Arc::new(e)))?;
    Ok(())
}

/// Serializes dictionary writes; a write for an older generation is skipped.
static WORDS_WRITE: Mutex<()> = Mutex::new(());
static WORDS_LATEST: AtomicU64 = AtomicU64::new(0);

/// A dictionary save for the caller to run off the UI thread
/// ([`WordsWrite::run`]); a write for an older generation does nothing.
#[must_use]
pub struct WordsWrite {
    path: PathBuf,
    words: Arc<BTreeSet<String>>,
    generation: u64,
}
impl WordsWrite {
    pub fn run(self) -> Result<(), DictionaryError> {
        let _guard = WORDS_WRITE.lock().unwrap_or_else(|e| e.into_inner());
        if WORDS_LATEST.load(Ordering::SeqCst) != self.generation {
            return Ok(());
        }
        save_words(&self.path, &self.words).map_err(|e| DictionaryError::Save(Box::new(e)))
    }
}

impl Store {
    /// Replace the user's words; the returned write saves them (none without
    /// a dictionary file). When the list is full nothing changes and the
    /// error is kept for the dictionary view.
    pub fn set_words(
        shared: &Shared,
        words: BTreeSet<String>,
    ) -> Result<Option<WordsWrite>, DictionaryError> {
        if words.len() > MAX_WORDS {
            shared.borrow_mut().words.error = Some(DictionaryError::Full);
            return Err(DictionaryError::Full);
        }
        let mut store = shared.borrow_mut();
        store.words.words = Arc::new(words);
        store.words.generation += 1;
        let Some(path) = store.words.path.clone() else {
            return Ok(None);
        };
        WORDS_LATEST.store(store.words.generation, Ordering::SeqCst);
        Ok(Some(WordsWrite {
            path,
            words: store.words.words.clone(),
            generation: store.words.generation,
        }))
    }
}
impl Store {
    /// The settings file under app data, if the system has a folder for it.
    pub fn default_path() -> Option<PathBuf> {
        crate::app_paths::data_local_dir().map(|p| p.join("mdoc/settings.json"))
    }
    /// Preferences and dictionary read from `path` (see [`Store::default_path`]);
    /// without one, the store reports that settings have nowhere to live.
    pub fn open(path: Option<PathBuf>) -> Shared {
        let loaded = match &path {
            Some(path) => load(path),
            None => Err(SettingsError::NoStorage),
        };
        Self::with(path, loaded)
    }
    /// Default preferences that are never read from or saved to disk.
    pub fn in_memory() -> Shared {
        Self::with(None, Ok(Preferences::default()))
    }
    fn with(path: Option<PathBuf>, loaded: Result<Preferences, SettingsError>) -> Shared {
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
    pub fn snapshot(&self) -> Result<Preferences, SettingsError> {
        self.current
            .clone()
            .ok_or_else(|| self.error.clone().unwrap_or(SettingsError::Unset))
    }
}
pub fn load(path: &Path) -> Result<Preferences, SettingsError> {
    let file = match std::fs::File::open(path) {
        Ok(f) => f,
        Err(e) if e.kind() == io::ErrorKind::NotFound => return Ok(Preferences::default()),
        Err(e) => return Err(e.into()),
    };
    let mut bytes = Vec::new();
    file.take(65537).read_to_end(&mut bytes)?;
    if bytes.len() > 65536 {
        return Err(SettingsError::TooLarge);
    }
    let prefs: Preferences =
        serde_json::from_slice(&bytes).map_err(|e| SettingsError::Parse(Arc::new(e)))?;
    prefs.validate()?;
    Ok(prefs)
}
/// Serializes settings writes made in order; a write older than the latest
/// one started is skipped, since the later one holds newer choices.
static PREFS_WRITE: Mutex<()> = Mutex::new(());
static PREFS_LATEST: AtomicU64 = AtomicU64::new(0);

/// A settings write's place in line: take it when the choice is made (on
/// the UI thread), then [`save_in_order`] it, perhaps on another thread.
pub fn next_write() -> u64 {
    PREFS_LATEST.fetch_add(1, Ordering::SeqCst) + 1
}

/// Whether `write` of [`next_write`] is still the latest started: a write
/// that is not must not publish its choices, even in memory.
pub fn is_latest_write(write: u64) -> bool {
    PREFS_LATEST.load(Ordering::SeqCst) == write
}

/// Save `prefs` as write `write` of [`next_write`], unless a later write
/// was started meanwhile; `Ok(false)` when skipped. An older Settings save
/// still running can then never land over a newer choice (the menu bar
/// shelf's notes save while the panel may be saving, ADR 0084).
pub fn save_in_order(path: &Path, prefs: &Preferences, write: u64) -> Result<bool, SettingsError> {
    let _guard = PREFS_WRITE.lock().unwrap_or_else(|e| e.into_inner());
    if PREFS_LATEST.load(Ordering::SeqCst) != write {
        return Ok(false);
    }
    save(path, prefs).map(|()| true)
}

pub fn save(path: &Path, prefs: &Preferences) -> Result<(), SettingsError> {
    prefs.validate()?;
    let parent = path.parent().ok_or(SettingsError::NoParent)?;
    std::fs::create_dir_all(parent)?;
    let mut file = tempfile::NamedTempFile::new_in(parent)?;
    serde_json::to_writer_pretty(&mut file, prefs)
        .map_err(|e| SettingsError::Encode(Arc::new(e)))?;
    file.flush()?;
    file.as_file().sync_all()?;
    file.persist(path)
        .map_err(|e| SettingsError::Persist(Arc::new(e)))?;
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
    fn default_categories_detect_everything_and_unticked_ones_roundtrip() {
        use mdoc_pii::Category;
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("settings.json");
        let defaults = Preferences::default();
        assert!(
            Category::ALL
                .into_iter()
                .all(|c| defaults.categories.detects(c))
        );
        let mut prefs = Preferences::default();
        prefs.categories.set(Category::Address, false);
        prefs.categories.set(Category::Email, false);
        prefs.categories.set(Category::Address, false);
        assert_eq!(prefs.categories.skip, ["ADDRESS", "EMAIL"]);
        save(&path, &prefs).unwrap();
        let loaded = load(&path).unwrap();
        assert_eq!(loaded, prefs);
        assert_eq!(
            loaded.categories.skipped(),
            [Category::Address, Category::Email].into()
        );
        prefs.categories.set(Category::Address, true);
        assert_eq!(prefs.categories.skip, ["EMAIL"]);
    }
    #[test]
    fn a_settings_write_older_than_the_latest_started_is_skipped() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("settings.json");
        let older = next_write();
        let newer = next_write();
        let mut off = Preferences::default();
        off.general.menu_bar = false;
        assert!(save_in_order(&path, &off, newer).unwrap());
        // The older write finishes last: it does not land.
        assert!(!save_in_order(&path, &Preferences::default(), older).unwrap());
        assert_eq!(load(&path).unwrap(), off);
    }
    #[test]
    fn the_menu_bar_is_on_by_default_and_absent_from_the_file_while_default() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("settings.json");
        assert!(Preferences::default().general.menu_bar);
        save(&path, &Preferences::default()).unwrap();
        assert!(!std::fs::read_to_string(&path).unwrap().contains("general"));
        // Turned off, or a note shown: written, and read back.
        let mut prefs = Preferences::default();
        prefs.general.menu_bar = false;
        prefs.general.menu_bar_note_shown = true;
        save(&path, &prefs).unwrap();
        assert_eq!(load(&path).unwrap(), prefs);
        // A file from before the flags keeps its choice; a missing switch
        // is on.
        let mut stored = serde_json::to_value(Preferences::default()).unwrap();
        stored["general"] = serde_json::json!({ "menu_bar": false });
        std::fs::write(&path, serde_json::to_vec(&stored).unwrap()).unwrap();
        let loaded = load(&path).unwrap().general;
        assert!(!loaded.menu_bar && !loaded.menu_bar_note_shown);
        stored["general"] = serde_json::json!({ "shelf_drag_hint_shown": true });
        std::fs::write(&path, serde_json::to_vec(&stored).unwrap()).unwrap();
        let loaded = load(&path).unwrap().general;
        assert!(loaded.menu_bar && loaded.shelf_drag_hint_shown);
        assert_eq!(loaded.sidebar_width, None);
    }
    #[test]
    fn a_dragged_sidebar_width_is_saved_and_absent_while_default() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("settings.json");
        let mut prefs = Preferences::default();
        prefs.general.sidebar_width = Some(300);
        save(&path, &prefs).unwrap();
        assert_eq!(load(&path).unwrap().general.sidebar_width, Some(300));
        // Another choice saved leaves the width out.
        prefs.general.sidebar_width = None;
        prefs.general.menu_bar = false;
        save(&path, &prefs).unwrap();
        assert!(
            !std::fs::read_to_string(&path)
                .unwrap()
                .contains("sidebar_width")
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
            assert!(error.to_string().contains("Reset explicitly in Settings"));
            assert_eq!(std::fs::read(&path).unwrap(), bytes);

            save(&path, &Preferences::default()).unwrap();
            assert_eq!(load(&path).unwrap(), Preferences::default());
        }
    }
    #[test]
    fn default_settings_file_is_unchanged_byte_for_byte() {
        // The file format predates mdoc-store; moving the types must not
        // change a single byte of what is written.
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("settings.json");
        save(&path, &Preferences::default()).unwrap();
        assert_eq!(
            std::fs::read_to_string(&path).unwrap(),
            r#"{
  "version": 1,
  "ocr": {
    "model": "V6Small",
    "dpi": 150,
    "minimum_confidence": 0.0
  },
  "pseudonymization": {
    "model": "Fp16",
    "threshold": 0.5
  },
  "spelling": {
    "enabled": true,
    "english": true,
    "russian": true,
    "check_all_caps": false,
    "check_digits": false
  },
  "updates": {
    "check_automatically": true,
    "include_prereleases": false
  },
  "cases": {
    "ask_before_closing": true
  }
}"#
        );
    }
    #[test]
    fn full_dictionary_changes_nothing_and_writes_skip_older_generations() {
        let dir = tempfile::tempdir().unwrap();
        let shared = Store::open(Some(dir.path().join("settings.json")));
        let full: BTreeSet<String> = (0..=MAX_WORDS).map(|i| format!("w{i}")).collect();
        assert!(matches!(
            Store::set_words(&shared, full),
            Err(DictionaryError::Full)
        ));
        assert!(shared.borrow().words.words.is_empty());
        let first = Store::set_words(&shared, ["one".to_string()].into())
            .unwrap()
            .unwrap();
        let second = Store::set_words(&shared, ["two".to_string()].into())
            .unwrap()
            .unwrap();
        first.run().unwrap();
        let path = dir.path().join("spelling-words.txt");
        assert!(!path.exists());
        second.run().unwrap();
        assert_eq!(load_words(&path).unwrap(), ["two".to_string()].into());
        assert!(
            Store::set_words(&Store::in_memory(), BTreeSet::new())
                .unwrap()
                .is_none()
        );
    }
}
