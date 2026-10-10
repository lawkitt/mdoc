//! Offline English/Russian spellcheck with built-in Hunspell dictionaries
//! ([ADR 0036](../../../docs/adr/0036-bundled-spellcheck.md)).
//!
//! The same text and [`Options`] give the same misspellings on every platform.
//! The crate knows nothing about Markdown or PII: the host passes
//! [`Exclusions`] (code, link targets, aliases, entity spans) and gets UTF-8
//! byte ranges back. Two operations, deliberately split:
//! - [`Speller::check`] finds misspelled spans; cheap, and incremental with a
//!   [`LineCache`], so a host can rerun it after each edit.
//! - [`Speller::suggest`] returns replacements for one word; slower (tens of
//!   milliseconds for Russian), so call it lazily, off the UI thread.
//!
//! Dictionaries load lazily and are shared process-wide; [`Speller::load`]
//! blocks while parsing, so call it from a background thread.

mod words;

use std::{
    collections::{HashMap, HashSet, hash_map::DefaultHasher},
    hash::{Hash, Hasher},
    io::Read,
    ops::Range,
    sync::{Arc, OnceLock},
};

use spellbook::Dictionary;
use words::{Script, Token};

/// What to check. Mirrors the app's Settings → Spelling choices.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct Options {
    pub english: bool,
    pub russian: bool,
    /// Check ALL-CAPS words (skipped by default; contract headings can hide
    /// OCR errors, so some users opt in).
    pub check_all_caps: bool,
    /// Check words containing digits, such as OCR's `c1ient` (skipped by default).
    pub check_digits: bool,
}

impl Default for Options {
    fn default() -> Self {
        Self {
            english: true,
            russian: true,
            check_all_caps: false,
            check_digits: false,
        }
    }
}

/// Byte ranges the host keeps out of the dictionary check.
#[derive(Clone, Copy, Debug, Default)]
pub struct Exclusions<'a> {
    /// Never checked: code, link and image targets, raw HTML, aliases.
    pub structural: &'a [Range<usize>],
    /// Detected or proposed PII entities. Skipped by the dictionary, but a
    /// word mixing Latin and Cyrillic letters is still flagged inside them.
    pub entities: &'a [Range<usize>],
}

struct English {
    us: Dictionary,
    gb: Dictionary,
}

static ENGLISH: OnceLock<English> = OnceLock::new();
static RUSSIAN: OnceLock<Dictionary> = OnceLock::new();

macro_rules! bundled {
    ($name:literal) => {
        include_bytes!(concat!(env!("OUT_DIR"), "/", $name, ".deflate"))
    };
}

fn inflate(bytes: &[u8]) -> String {
    let mut text = String::new();
    flate2::read::DeflateDecoder::new(bytes)
        .read_to_string(&mut text)
        .expect("bundled dictionary is valid UTF-8 deflate data");
    // en_GB ships with a byte-order mark that the parser would read as a word.
    match text.strip_prefix('\u{feff}') {
        Some(rest) => rest.to_owned(),
        None => text,
    }
}

/// Parse a bundled dictionary and add a supplement in `.dic` line format.
fn parse(aff: &[u8], dic: &[u8], supplement: &str) -> Dictionary {
    let mut dictionary =
        Dictionary::new(&inflate(aff), &inflate(dic)).expect("bundled dictionary parses");
    for line in supplement.lines().map(str::trim) {
        if line.is_empty() || line.starts_with('#') {
            continue;
        }
        dictionary
            .add(line)
            .unwrap_or_else(|e| panic!("legal word list entry {line:?}: {e:?}"));
    }
    dictionary
}

fn english() -> &'static English {
    ENGLISH.get_or_init(|| English {
        // Legal words use en_US affix flags; a word passes if either variant
        // accepts it, so they need not be repeated for en_GB.
        us: parse(
            bundled!("en_US.aff"),
            bundled!("en_US.dic"),
            include_str!("../dictionaries/legal-en.dic"),
        ),
        gb: parse(bundled!("en_GB.aff"), bundled!("en_GB.dic"), ""),
    })
}

fn russian() -> &'static Dictionary {
    RUSSIAN.get_or_init(|| {
        parse(
            bundled!("ru_RU.aff"),
            bundled!("ru_RU.dic"),
            include_str!("../dictionaries/legal-ru.dic"),
        )
    })
}

/// Words the user accepts beyond the dictionaries: their own dictionary and
/// words ignored in one document. Exact spellings; a lowercase entry also
/// accepts the capitalized and upper-case forms. `generation` identifies the
/// set's contents for [`LineCache`]: change it whenever the words change.
#[derive(Clone, Default)]
pub struct Accepted {
    pub words: Arc<HashSet<String>>,
    pub generation: u64,
}

impl Accepted {
    fn contains(&self, word: &str) -> bool {
        !self.words.is_empty()
            && (self.words.contains(word) || self.words.contains(&word.to_lowercase()))
    }
}

/// The dictionaries for one set of [`Options`]. Cheap to clone once loaded.
#[derive(Clone)]
pub struct Speller {
    options: Options,
    english: Option<&'static English>,
    russian: Option<&'static Dictionary>,
    accepted: Accepted,
}

impl Speller {
    /// Load the dictionaries `options` needs. Blocks while parsing on first
    /// use (about 60 ms for all languages in a release build); languages that
    /// are turned off are never loaded.
    pub fn load(options: Options) -> Self {
        Self {
            options,
            english: options.english.then(english),
            russian: options.russian.then(russian),
            accepted: Accepted::default(),
        }
    }

    /// Also accept the user's own words (see [`Accepted`]).
    pub fn with_accepted(mut self, accepted: Accepted) -> Self {
        self.accepted = accepted;
        self
    }

    /// Whether [`Self::load`] would return without parsing anything.
    pub fn is_loaded(options: Options) -> bool {
        (!options.english || ENGLISH.get().is_some())
            && (!options.russian || RUSSIAN.get().is_some())
    }

    pub fn options(&self) -> Options {
        self.options
    }

    /// Misspelled byte ranges in `text`, in source order.
    pub fn check(&self, text: &str, exclusions: Exclusions) -> Vec<Range<usize>> {
        self.check_cached(text, exclusions, &mut LineCache::default())
    }

    /// Like [`Self::check`], reusing results for lines whose text and
    /// exclusions are unchanged since the previous call with `cache`.
    pub fn check_cached(
        &self,
        text: &str,
        exclusions: Exclusions,
        cache: &mut LineCache,
    ) -> Vec<Range<usize>> {
        let identity = (self.options, self.accepted.generation);
        if cache.identity != Some(identity) {
            cache.lines.clear();
            cache.identity = Some(identity);
        }
        let structural = merged(exclusions.structural);
        let entities = merged(exclusions.entities);
        let mut previous = std::mem::take(&mut cache.lines);
        let mut out = Vec::new();
        let mut start = 0;
        for line in text.split_inclusive('\n') {
            let range = start..start + line.len();
            start = range.end;
            let local_structural = clip(&structural, &range);
            let local_entities = clip(&entities, &range);
            let mut hasher = DefaultHasher::new();
            (line, &local_structural, &local_entities).hash(&mut hasher);
            let key = hasher.finish();
            let found = match previous.remove(&key) {
                Some(found) => found,
                None => self.check_line(line, &local_structural, &local_entities),
            };
            out.extend(
                found
                    .iter()
                    .map(|r| r.start + range.start..r.end + range.start),
            );
            cache.lines.insert(key, found);
        }
        out
    }

    fn check_line(
        &self,
        line: &str,
        structural: &[Range<usize>],
        entities: &[Range<usize>],
    ) -> Vec<Range<usize>> {
        let mut structural = structural.to_vec();
        structural.extend(words::bare_links(line));
        let mut out = Vec::new();
        for token in words::tokens(line) {
            if overlaps(&structural, &token.range) {
                continue;
            }
            let in_entity = overlaps(entities, &token.range);
            self.check_token(line, &token, in_entity, &mut out);
        }
        out
    }

    fn check_token(&self, line: &str, token: &Token, in_entity: bool, out: &mut Vec<Range<usize>>) {
        let text = &line[token.range.clone()];
        if text.contains('_') {
            return; // identifiers and generated aliases (`PERSON_1`)
        }
        if self.accepted.contains(text) {
            return; // the user's word, even if mixed-script or in an entity
        }
        let parts = words::hyphen_parts(text, token.range.start);
        let has_digit = text.chars().any(|c| c.is_ascii_digit());
        // Mixed scripts are judged per hyphen part: "PDF-файл" is fine.
        let mut mixed = false;
        for part in &parts {
            let word = &line[part.clone()];
            if !word.chars().any(|c| c.is_ascii_digit()) && words::script(word) == Script::Mixed {
                out.push(part.clone());
                mixed = true;
            }
        }
        if mixed || in_entity || words::letter_count(text) < 2 {
            return;
        }
        if has_digit && !self.options.check_digits {
            return;
        }
        if !self.options.check_all_caps && words::is_all_caps(text) {
            return;
        }
        if !has_digit && self.accepts(text) {
            return;
        }
        for part in parts {
            let word = &line[part.clone()];
            if words::letter_count(word) < 2 && !word.chars().any(|c| c.is_ascii_digit()) {
                continue; // `т.е`, `г.`, `1-й`: single letters say nothing
            }
            if word.chars().any(|c| c.is_ascii_digit()) {
                if !words::is_number_like(word) {
                    out.push(part);
                }
            } else if !self.accepts(word) {
                out.push(part);
            }
        }
    }

    /// Dictionary lookup for one letters-only word, routed by script. A word
    /// in a language that is turned off is accepted.
    fn accepts(&self, word: &str) -> bool {
        if self.accepted.contains(word) {
            return true;
        }
        let word = words::lookup_form(word);
        match words::script(&word) {
            Script::Cyrillic => self.russian.is_none_or(|ru| {
                ru.check(&word) || (word.contains(['ё', 'Ё']) && ru.check(&words::yo_to_ye(&word)))
            }),
            Script::Latin => self
                .english
                .is_none_or(|en| en.us.check(&word) || en.gb.check(&word)),
            Script::Mixed => false,
            Script::Other => true,
        }
    }

    /// Replacements for `word`, best first, at most eight. A mixed-script
    /// word first offers its single-script spellings, even ones the
    /// dictionary doesn't know (names).
    pub fn suggest(&self, word: &str) -> Vec<String> {
        const LIMIT: usize = 8;
        let mut out: Vec<String> = words::single_script_forms(word);
        // A form the dictionary knows beats the majority-script guess.
        out.sort_by_key(|form| !self.accepts(form));
        let base = out.first().cloned().unwrap_or_else(|| word.to_owned());
        let lookup = words::lookup_form(&base);
        let mut found = Vec::new();
        match words::script(&lookup) {
            Script::Cyrillic => {
                if let Some(ru) = self.russian {
                    ru.suggest(&lookup, &mut found);
                }
            }
            Script::Latin => {
                if let Some(en) = self.english {
                    en.us.suggest(&lookup, &mut found);
                    let mut gb = Vec::new();
                    en.gb.suggest(&lookup, &mut gb);
                    found.extend(gb);
                }
            }
            Script::Mixed | Script::Other => {}
        }
        for suggestion in found {
            if suggestion != word && !out.contains(&suggestion) {
                out.push(suggestion);
            }
        }
        out.truncate(LIMIT);
        out
    }
}

/// Per-document results by line, so an edit rechecks only changed lines.
/// Entries for lines that no longer exist are dropped on each check.
#[derive(Default)]
pub struct LineCache {
    identity: Option<(Options, u64)>,
    lines: HashMap<u64, Vec<Range<usize>>>,
}

/// Sorted, non-overlapping copy of `ranges`.
fn merged(ranges: &[Range<usize>]) -> Vec<Range<usize>> {
    let mut sorted: Vec<_> = ranges.iter().filter(|r| r.start < r.end).cloned().collect();
    sorted.sort_by_key(|r| r.start);
    let mut out: Vec<Range<usize>> = Vec::with_capacity(sorted.len());
    for range in sorted {
        match out.last_mut() {
            Some(last) if range.start <= last.end => last.end = last.end.max(range.end),
            _ => out.push(range),
        }
    }
    out
}

/// `ranges` (from [`merged`]) intersected with `within`, relative to its start.
fn clip(ranges: &[Range<usize>], within: &Range<usize>) -> Vec<Range<usize>> {
    let first = ranges.partition_point(|r| r.end <= within.start);
    ranges[first..]
        .iter()
        .take_while(|r| r.start < within.end)
        .filter(|r| r.end > within.start)
        .map(|r| r.start.max(within.start) - within.start..r.end.min(within.end) - within.start)
        .collect()
}

fn overlaps(ranges: &[Range<usize>], range: &Range<usize>) -> bool {
    ranges
        .iter()
        .any(|r| r.start < range.end && range.start < r.end)
}

#[cfg(test)]
mod tests;
