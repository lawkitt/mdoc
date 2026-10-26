//! Recognition languages (ADR 0065): the list of languages OCR is asked to
//! read, and the default list for one document with the reason it was chosen.
//! Engines map the list to what they read (`ocr::plan` for PP-OCR).

/// A document language, by ISO 639-1 code. Lists of these, never pairs or
/// per-language switches, choose and report recognition (ADR 0065).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct Language(pub &'static str);

pub const ENGLISH: Language = Language("en");
pub const RUSSIAN: Language = Language("ru");

impl Language {
    pub fn name(self) -> &'static str {
        match self.0 {
            "en" => "English",
            "ru" => "Russian",
            code => code,
        }
    }
    /// The language a letter's script suggests: Cyrillic suggests Russian,
    /// the only qualified Cyrillic language (ADR 0065).
    fn of_letter(c: char) -> Option<Self> {
        match c {
            'a'..='z' | 'A'..='Z' => Some(ENGLISH),
            '\u{0400}'..='\u{04FF}' if c.is_alphabetic() => Some(RUSSIAN),
            _ => None,
        }
    }
    /// Words a file name may use to name this language.
    fn names(self) -> &'static [&'static str] {
        match self.0 {
            "en" => &["en", "eng", "english", "англ", "английский"],
            "ru" => &["ru", "rus", "russian", "рус", "русский"],
            _ => &[],
        }
    }
}

/// "English", "English & Russian", "English, French & Russian".
pub fn label(languages: &[Language]) -> String {
    join(languages, "&")
}

/// "Russian", "French or Russian".
fn label_or(languages: &[Language]) -> String {
    join(languages, "or")
}

fn join(languages: &[Language], last_word: &str) -> String {
    let names: Vec<_> = languages.iter().map(|l| l.name()).collect();
    match names.split_last() {
        None => String::new(),
        Some((last, [])) => (*last).into(),
        Some((last, rest)) => format!("{} {last_word} {last}", rest.join(", ")),
    }
}

/// What decided a document's recognition languages.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Basis {
    /// Letters in the PDF's own text (pages that need no recognition).
    TextLayer,
    /// Non-Latin letters in the file name ("договор.pdf").
    FileNameScript,
    /// A word in the file name naming a language ("contract_ru.pdf").
    FileNameWord,
    /// A keyboard input source set up on this computer.
    InputSource,
    /// Nothing suggested a language: the Settings OCR choice applies.
    Settings,
    /// Picked on the card for this document.
    User,
}

/// The recognition languages for one document and why they were chosen.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Choice {
    pub languages: Vec<Language>,
    pub source: Basis,
    /// The card's one line, e.g. "English & Russian: the file name is in
    /// Russian".
    pub reason: String,
}

impl Choice {
    pub fn picked(languages: Vec<Language>) -> Self {
        let reason = format!("{}: picked for this document", label(&languages));
        Self {
            languages,
            source: Basis::User,
            reason,
        }
    }
}

/// What detection may look at, gathered when the card first appears.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Signals {
    /// Languages the text layer's letters suggest (`text_languages`).
    pub text: Vec<Language>,
    pub file_name: String,
    /// Languages of this computer's keyboard input sources.
    pub input_sources: Vec<Language>,
}

/// Fewer letters than this say nothing: page numbers, stray marks.
const MIN_LETTERS: usize = 3;

/// Languages a text's letters suggest, most letters first. A few stray
/// letters of a script are ignored.
pub fn text_languages(text: &str) -> Vec<Language> {
    letter_languages(text, MIN_LETTERS)
}

/// Languages with at least `min` letters in `text`, most letters first.
fn letter_languages(text: &str, min: usize) -> Vec<Language> {
    let mut counts: Vec<(Language, usize)> = Vec::new();
    for language in text.chars().filter_map(Language::of_letter) {
        match counts.iter_mut().find(|(l, _)| *l == language) {
            Some((_, count)) => *count += 1,
            None => counts.push((language, 1)),
        }
    }
    counts.retain(|(_, count)| *count >= min);
    counts.sort_by_key(|(_, count)| std::cmp::Reverse(*count));
    counts.into_iter().map(|(l, _)| l).collect()
}

/// Languages a file name names in words such as "ru" or "english".
fn named_languages(file_name: &str) -> Vec<Language> {
    let mut found = Vec::new();
    for word in file_name
        .split(|c: char| !c.is_alphanumeric())
        .map(str::to_lowercase)
    {
        for language in [ENGLISH, RUSSIAN] {
            if language.names().contains(&word.as_str()) && !found.contains(&language) {
                found.push(language);
            }
        }
    }
    found
}

/// The default recognition languages for a document. The first signal that
/// says anything decides, document signals before the computer's: the text
/// layer, the file name's letters, words in the file name, then a keyboard
/// input source; otherwise `fallback` (the Settings OCR choice).
///
/// `offered` lists what the engine can be asked for; the result is the
/// first offered list that holds every suggested language, or the one
/// holding most of them.
pub fn detect(signals: &Signals, offered: &[&'static [Language]], fallback: &[Language]) -> Choice {
    let stem = std::path::Path::new(&signals.file_name)
        .file_stem()
        .map(|s| s.to_string_lossy().into_owned())
        .unwrap_or_default();
    // Only scripts other than Latin say something in a file name: Latin
    // names are as often transliterated ("dogovor.pdf") as English.
    // Any such letter counts: a file name is short ("РФ.pdf").
    let script: Vec<_> = letter_languages(&stem, 1)
        .into_iter()
        .filter(|l| *l != ENGLISH)
        .collect();
    // The first offered list is what every computer reads (English).
    let base = offered.first().copied().unwrap_or_default();
    // The keyboard only ever adds a language: an English keyboard is on
    // nearly every computer and says nothing about the document.
    let keyboard: Vec<_> = signals
        .input_sources
        .iter()
        .copied()
        .filter(|l| !base.contains(l))
        .collect();
    let (suggested, source) = [
        (signals.text.clone(), Basis::TextLayer),
        (script, Basis::FileNameScript),
        (named_languages(&stem), Basis::FileNameWord),
        (keyboard, Basis::InputSource),
    ]
    .into_iter()
    .find(|(languages, _)| !languages.is_empty())
    .unwrap_or_else(|| (fallback.to_vec(), Basis::Settings));
    let languages = closest(&suggested, offered);
    let mut others: Vec<Language> = offered
        .iter()
        .flat_map(|list| list.iter().copied())
        .filter(|l| !languages.contains(l))
        .collect();
    others.sort_unstable();
    others.dedup();
    let news: Vec<_> = suggested
        .iter()
        .copied()
        .filter(|l| !base.contains(l))
        .collect();
    let because = match source {
        Basis::TextLayer if !news.is_empty() => {
            format!("the text layer has {} text", label(&news))
        }
        Basis::TextLayer if !others.is_empty() => {
            format!("the text layer has no {} text", label_or(&others))
        }
        Basis::TextLayer => format!("the text layer is in {}", label(&languages)),
        Basis::FileNameScript => format!("the file name is in {}", label(&suggested)),
        Basis::FileNameWord => format!("the file name mentions {}", label(&suggested)),
        Basis::InputSource => format!("this computer has a {} keyboard", label(&suggested)),
        Basis::Settings if languages == base && !others.is_empty() => format!(
            "no sign of {} in the text, file name or keyboard",
            label_or(&others)
        ),
        Basis::Settings | Basis::User => "your default in Settings".into(),
    };
    Choice {
        reason: format!("{}: {because}", label(&languages)),
        languages,
        source,
    }
}

/// The first offered list holding every language in `wanted`, else the one
/// holding most of them (earlier lists win ties).
pub fn closest(wanted: &[Language], offered: &[&[Language]]) -> Vec<Language> {
    let covered = |list: &[Language]| wanted.iter().filter(|l| list.contains(l)).count();
    offered
        .iter()
        .find(|list| covered(list) == wanted.len())
        .or_else(|| offered.iter().rev().max_by_key(|list| covered(list)))
        .map(|list| list.to_vec())
        .unwrap_or_else(|| wanted.to_vec())
}

/// Languages of the keyboard input sources set up on this computer. The
/// document view takes this as an injectable function so tests never read
/// the machine's keyboards.
pub fn input_source_languages() -> Vec<Language> {
    let mut languages = Vec::new();
    for code in platform::input_source_codes() {
        let primary = code.split(['-', '_']).next().unwrap_or_default();
        let language = match primary {
            "en" => ENGLISH,
            "ru" => RUSSIAN,
            _ => continue,
        };
        if !languages.contains(&language) {
            languages.push(language);
        }
    }
    languages
}

#[cfg(target_os = "macos")]
mod platform {
    //! Text Input Sources (Carbon): each enabled keyboard source's primary
    //! language, e.g. "ru" for Russian – PC.
    use std::ffi::{c_char, c_void};

    type CFTypeRef = *const c_void;
    type CFIndex = isize;

    #[link(name = "Carbon", kind = "framework")]
    unsafe extern "C" {
        static kTISPropertyInputSourceLanguages: CFTypeRef;
        static kTISPropertyInputSourceCategory: CFTypeRef;
        static kTISCategoryKeyboardInputSource: CFTypeRef;
        fn TISCreateInputSourceList(properties: CFTypeRef, include_all: u8) -> CFTypeRef;
        fn TISGetInputSourceProperty(source: CFTypeRef, key: CFTypeRef) -> CFTypeRef;
    }

    #[link(name = "CoreFoundation", kind = "framework")]
    unsafe extern "C" {
        fn CFArrayGetCount(array: CFTypeRef) -> CFIndex;
        fn CFArrayGetValueAtIndex(array: CFTypeRef, index: CFIndex) -> CFTypeRef;
        fn CFEqual(a: CFTypeRef, b: CFTypeRef) -> u8;
        fn CFStringGetCString(
            string: CFTypeRef,
            buffer: *mut c_char,
            size: CFIndex,
            encoding: u32,
        ) -> u8;
        fn CFRelease(value: CFTypeRef);
    }

    const UTF8: u32 = 0x0800_0100;

    pub fn input_source_codes() -> Vec<String> {
        let mut codes = Vec::new();
        // SAFETY: the Create call's array is released below; Get calls
        // return borrowed values that live as long as the array.
        unsafe {
            let sources = TISCreateInputSourceList(std::ptr::null(), 0);
            if sources.is_null() {
                return codes;
            }
            for index in 0..CFArrayGetCount(sources) {
                let source = CFArrayGetValueAtIndex(sources, index);
                let category = TISGetInputSourceProperty(source, kTISPropertyInputSourceCategory);
                if category.is_null() || CFEqual(category, kTISCategoryKeyboardInputSource) == 0 {
                    continue;
                }
                let languages = TISGetInputSourceProperty(source, kTISPropertyInputSourceLanguages);
                if languages.is_null() || CFArrayGetCount(languages) == 0 {
                    continue;
                }
                let primary = CFArrayGetValueAtIndex(languages, 0);
                let mut buffer = [0 as c_char; 32];
                if CFStringGetCString(primary, buffer.as_mut_ptr(), buffer.len() as CFIndex, UTF8)
                    != 0
                {
                    codes.push(
                        std::ffi::CStr::from_ptr(buffer.as_ptr())
                            .to_string_lossy()
                            .into_owned(),
                    );
                }
            }
            CFRelease(sources);
        }
        codes
    }
}

#[cfg(windows)]
mod platform {
    //! The installed keyboard layouts' languages (LANGID primary language).
    use windows_sys::Win32::UI::Input::KeyboardAndMouse::{GetKeyboardLayoutList, HKL};

    pub fn input_source_codes() -> Vec<String> {
        // SAFETY: a null buffer asks for the count; the second call fills
        // at most `layouts.len()` handles.
        let (mut layouts, filled) = unsafe {
            let count = GetKeyboardLayoutList(0, std::ptr::null_mut());
            let mut layouts: Vec<HKL> = vec![std::ptr::null_mut(); count.max(0) as usize];
            let filled = GetKeyboardLayoutList(layouts.len() as i32, layouts.as_mut_ptr());
            (layouts, filled)
        };
        layouts.truncate(filled.max(0) as usize);
        layouts
            .into_iter()
            .filter_map(|layout| match (layout as usize & 0x3ff) as u16 {
                0x09 => Some("en".to_owned()),
                0x19 => Some("ru".to_owned()),
                _ => None,
            })
            .collect()
    }
}

#[cfg(not(any(target_os = "macos", windows)))]
mod platform {
    pub fn input_source_codes() -> Vec<String> {
        Vec::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const OFFERED: &[&[Language]] = &[&[ENGLISH], &[ENGLISH, RUSSIAN]];

    fn signals(text: &str, file_name: &str, input_sources: &[Language]) -> Signals {
        Signals {
            text: text_languages(text),
            file_name: file_name.into(),
            input_sources: input_sources.to_vec(),
        }
    }

    fn reason(signals: &Signals) -> String {
        detect(signals, OFFERED, &[ENGLISH]).reason
    }

    /// Reads this computer's keyboards, so it only runs by hand.
    #[test]
    #[ignore = "reads this computer's keyboard input sources"]
    fn input_sources_are_read_without_crashing() {
        let codes = platform::input_source_codes();
        eprintln!("input sources: {codes:?} -> {:?}", input_source_languages());
        if cfg!(any(target_os = "macos", windows)) {
            assert!(!codes.is_empty());
        }
    }

    #[test]
    fn labels_are_lists() {
        assert_eq!(label(&[ENGLISH]), "English");
        assert_eq!(label(&[ENGLISH, RUSSIAN]), "English & Russian");
        assert_eq!(
            label(&[ENGLISH, Language("fr"), RUSSIAN]),
            "English, fr & Russian"
        );
    }

    #[test]
    fn text_layer_letters_suggest_languages_ignoring_strays() {
        assert_eq!(text_languages("Договор подписан"), [RUSSIAN]);
        assert_eq!(
            text_languages("Agreement dated 1 May — Договор"),
            [ENGLISH, RUSSIAN]
        );
        assert!(text_languages("12 / 34 — 5. Я").is_empty());
        assert_eq!(text_languages("Page 1 · ab"), [ENGLISH]);
    }

    #[test]
    fn each_signal_explains_the_choice_in_one_line() {
        assert_eq!(
            reason(&signals("Этот документ", "scan.pdf", &[])),
            "English & Russian: the text layer has Russian text"
        );
        assert_eq!(
            reason(&signals("", "договор аренды.pdf", &[])),
            "English & Russian: the file name is in Russian"
        );
        for short in ["РФ.pdf", "Я scan.pdf"] {
            assert_eq!(
                reason(&signals("", short, &[])),
                "English & Russian: the file name is in Russian",
                "{short}"
            );
        }
        assert_eq!(
            reason(&signals("", "russian.pdf", &[])),
            "English & Russian: the file name mentions Russian"
        );
        assert_eq!(
            reason(&signals("", "contract_RU.pdf", &[])),
            "English & Russian: the file name mentions Russian"
        );
        assert_eq!(
            reason(&signals("", "scan.pdf", &[ENGLISH, RUSSIAN])),
            "English & Russian: this computer has a Russian keyboard"
        );
        assert_eq!(
            reason(&signals("", "scan.pdf", &[ENGLISH])),
            "English: no sign of Russian in the text, file name or keyboard"
        );
        assert_eq!(
            detect(&signals("", "scan.pdf", &[]), OFFERED, &[ENGLISH, RUSSIAN]).reason,
            "English & Russian: your default in Settings"
        );
    }

    #[test]
    fn document_signals_outrank_the_keyboard() {
        // An English text layer or file name stays English on a computer
        // with a Russian keyboard.
        let english = detect(
            &signals("Readable page three", "scan.pdf", &[RUSSIAN]),
            OFFERED,
            &[ENGLISH],
        );
        assert_eq!(english.languages, [ENGLISH]);
        assert_eq!(english.source, Basis::TextLayer);
        assert_eq!(
            english.reason,
            "English: the text layer has no Russian text"
        );
        let named = detect(&signals("", "english.pdf", &[RUSSIAN]), OFFERED, &[ENGLISH]);
        assert_eq!(named.languages, [ENGLISH]);
        assert_eq!(named.reason, "English: the file name mentions English");
        // A Latin file name alone is not evidence: it may be transliterated.
        let latin = detect(&signals("", "dogovor.pdf", &[RUSSIAN]), OFFERED, &[ENGLISH]);
        assert_eq!(latin.source, Basis::InputSource);
        // Text outranks the file name.
        let text = detect(&signals("Договор", "english.pdf", &[]), OFFERED, &[ENGLISH]);
        assert_eq!(text.languages, [ENGLISH, RUSSIAN]);
        assert_eq!(text.source, Basis::TextLayer);
    }

    #[test]
    fn suggestions_map_to_the_closest_offered_list() {
        // Russian alone maps to the list that reads it.
        let russian = detect(&signals("Только русский", "", &[]), OFFERED, &[ENGLISH]);
        assert_eq!(russian.languages, [ENGLISH, RUSSIAN]);
        // A language no list offers keeps the list holding the rest.
        assert_eq!(
            closest(&[Language("de"), RUSSIAN], OFFERED),
            [ENGLISH, RUSSIAN]
        );
        assert_eq!(closest(&[Language("de")], OFFERED), [ENGLISH]);
    }
}
