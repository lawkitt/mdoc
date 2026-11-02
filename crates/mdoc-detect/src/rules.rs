//! The rule pack (ADR 0092): `rules/{shared,en,ru}.toml`, compiled in, read
//! and checked once. Its `never` words and `shape`s decide which model spans
//! are kept and what a manual selection is guessed to be.
use mdoc_pii::{Category, Detection, Recognizer};
use regex::Regex;
use serde::Deserialize;
use std::sync::OnceLock;

/// The pack's files, in the order their rules apply.
const FILES: [(&str, &str); 3] = [
    ("shared.toml", include_str!("../rules/shared.toml")),
    ("en.toml", include_str!("../rules/en.toml")),
    ("ru.toml", include_str!("../rules/ru.toml")),
];

/// A labelled value's score: rule evidence, like the structured rules'.
const CONTEXT_SCORE: f32 = 0.95;

/// The pack, read on first use. Every file is checked by a unit test, so a
/// failure here is a build that skipped its tests.
pub(crate) fn pack() -> &'static Pack {
    static PACK: OnceLock<Pack> = OnceLock::new();
    PACK.get_or_init(|| Pack::load(&FILES).expect("rule pack"))
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct File {
    #[serde(default)]
    never: Vec<NeverEntry>,
    #[serde(default)]
    context: Vec<ContextEntry>,
    #[serde(default)]
    shape: Vec<ShapeEntry>,
    #[serde(default)]
    span: Vec<SpanEntry>,
    #[serde(default)]
    variants: Vec<VariantsEntry>,
    #[serde(default)]
    reference: Vec<ReferenceEntry>,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct NeverEntry {
    #[serde(rename = "match")]
    form: Form,
    /// For `stem`: how many letters may follow the stem.
    ending: Option<usize>,
    words: Vec<String>,
    /// Categories whose model span ending in one of these words is a role,
    /// not a name ("Technical Reviewer", "Costs Officer").
    #[serde(default)]
    last_word_of: Vec<String>,
    /// Categories whose model span with one of these words anywhere is
    /// never proposed: courts and public bodies ("County Court").
    #[serde(default)]
    anywhere_in: Vec<String>,
    /// For `stem`: endings that never count, because they make a surname
    /// of the stem ("Подрядчиков", "Директоров").
    #[serde(default)]
    not_endings: Vec<String>,
}

#[derive(Clone, Copy, Deserialize, PartialEq, Eq, Hash)]
#[serde(rename_all = "lowercase")]
enum Form {
    /// The word itself.
    Whole,
    /// The word, or the word and an `s`.
    Plural,
    /// The stem and up to `ending` letters.
    Stem,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct ContextEntry {
    category: String,
    labels: Vec<String>,
    /// What may sit between a label and its value (`:`, `№`, Markdown bold).
    between: String,
    value: String,
    /// Fewer digits in the value and it is not proposed: the review finds a
    /// wording everywhere, so a short one would mark "article 12" too.
    #[serde(default)]
    min_digits: usize,
    /// Words that, just before the label, make it a common word ("in case").
    #[serde(default)]
    unless_after: Vec<String>,
    /// Categories whose guesses, taking the value, rule it out (a date).
    #[serde(default)]
    unless_guessed: Vec<String>,
    /// Categories one of whose guesses must take the value (a date of birth
    /// must look like a date).
    #[serde(default)]
    only_guessed: Vec<String>,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct ShapeEntry {
    category: String,
    #[serde(default)]
    require: Vec<String>,
    #[serde(default)]
    forbid: Vec<String>,
    word: Option<String>,
    #[serde(default)]
    guess: Vec<String>,
    #[serde(default)]
    guess_words: Vec<String>,
    /// Quote pairs ("«»"): a model span wrapped exactly in one is a title,
    /// not this category.
    #[serde(default)]
    forbid_quotes: Vec<String>,
    /// Lowercase stems: with them, a quoted span is a title only after a
    /// word written in lowercase that is one with a noun's ending
    /// ("повести «…»", not "Роман «…»" or "рассказал «…»").
    #[serde(default)]
    quotes_after: Vec<String>,
    /// Words as written: with them, a quoted span is a title or product
    /// after one of these words too ("ПО «…»", "АСУ «…»").
    #[serde(default)]
    quotes_after_words: Vec<String>,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct SpanEntry {
    category: String,
    /// What a whole span looks like; `{name}` stands for a part. An
    /// entry without one fixes model spans' boundaries instead.
    #[serde(default)]
    pattern: Option<String>,
    /// Named pieces of the pattern, each a regex.
    #[serde(default)]
    parts: std::collections::BTreeMap<String, String>,
    #[serde(default)]
    trim: Vec<String>,
    #[serde(default)]
    extend: Vec<String>,
    #[serde(default)]
    strip_end: String,
    #[serde(default)]
    person_before: Vec<String>,
}

/// A legal reference or other marker no model span inside is (guide rule 8):
/// "424 U.S. at 38", "26 U.S.C. § 6212", "127-ФЗ".
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct ReferenceEntry {
    /// What the reference looks like; `{name}` stands for a part, and a
    /// group named `lead` or `tail` is context it leaves out.
    pattern: String,
    #[serde(default)]
    parts: std::collections::BTreeMap<String, String>,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct VariantsEntry {
    name: String,
    forms: Vec<String>,
}

/// The checked pack, merged across files.
pub(crate) struct Pack {
    never: Vec<Never>,
    /// One per category, in the order categories first appear.
    shapes: Vec<Shape>,
    contexts: Vec<Context>,
    spans: Vec<Span>,
    /// Legal references: a model span inside a match is never proposed.
    references: Vec<Regex>,
}

/// A `span` entry: a whole span by its pattern, or a boundary fix.
enum Span {
    /// Proposes each match: an address from its first element to its
    /// postcode or city.
    Pattern {
        category: Category,
        pattern: Regex,
    },
    Fix(SpanFix),
}

/// A boundary fix for one category's spans.
struct SpanFix {
    category: Category,
    /// Lowercase words that come off either edge ("ms", "kc").
    trim: Vec<String>,
    /// A span inside a match grows to the whole match.
    extend: Vec<Regex>,
    /// Characters that come off the end (a sentence's period).
    strip_end: String,
    /// Lowercase words that, ending an organization span, make it a
    /// person's document: "Ostrand Decl." is the declaration of a person
    /// named Ostrand.
    person_before: Vec<String>,
}

impl SpanFix {
    /// `range` without its `trim` words at either edge (a comma before a
    /// trailing one goes with it) and its `strip_end` characters.
    fn trimmed(&self, source: &str, mut range: std::ops::Range<usize>) -> std::ops::Range<usize> {
        let listed = |word: &str| {
            let word = word.strip_suffix('.').unwrap_or(word).to_lowercase();
            self.trim.contains(&word)
        };
        loop {
            let text = &source[range.clone()];
            let lead = text.len() - text.trim_start().len();
            let tail = text.len() - text.trim_end().len();
            range = range.start + lead..range.end - tail;
            let text = &source[range.clone()];
            if let Some((first, rest)) = text.split_once(char::is_whitespace)
                && listed(first)
            {
                range.start = range.end - rest.len();
                continue;
            }
            if let Some((rest, last)) = text.rsplit_once(char::is_whitespace)
                && listed(last)
            {
                range.end = range.start + rest.trim_end_matches([',', ' ']).len();
                continue;
            }
            if let Some(c) = text.chars().next_back()
                && text.len() > c.len_utf8()
                && self.strip_end.contains(c)
                && !(c == '.' && ends_with_initial(&text[..text.len() - 1]))
            {
                range.end -= c.len_utf8();
                continue;
            }
            return range;
        }
    }
}

/// What may follow a `quotes_after` stem: a noun's case endings ("повести",
/// "романом"), never a verb's ("рассказал", "играл").
const NOUN_ENDINGS: &[&str] = &[
    "", "а", "я", "у", "ю", "е", "и", "ы", "о", "ь", "ом", "ем", "ой", "ей", "ою", "ью", "ах",
    "ях", "ам", "ям", "ов", "ев", "ий", "ия", "ие", "ию", "ии", "ая", "ое", "ые", "й", "ами",
    "ями",
];

/// Whether `text` ends with a single capital letter on its own, an
/// initial whose period belongs to the name ("Белявская О. М", "Smith J").
fn ends_with_initial(text: &str) -> bool {
    let mut chars = text.chars().rev();
    chars.next().is_some_and(char::is_uppercase)
        && chars.next().is_none_or(|c| c.is_whitespace() || c == '.')
}

/// A label then its value: the value's capture group 1 is proposed.
struct Context {
    category: Category,
    pattern: Regex,
    /// The labels, lowercase, single-spaced: a table column headed by one
    /// takes the value in each of its cells.
    labels: Vec<String>,
    /// The value alone, whole.
    value: Regex,
    min_digits: usize,
    unless_after: Vec<String>,
    unless_guessed: Vec<Category>,
    only_guessed: Vec<Category>,
}

struct Never {
    form: Form,
    ending: usize,
    words: Vec<String>,
    last_word_of: Vec<Category>,
    anywhere_in: Vec<Category>,
    not_endings: Vec<String>,
}

impl Never {
    /// Whether lowercase `word` is one of this entry's words.
    fn has(&self, word: &str) -> bool {
        self.words.iter().any(|listed| match self.form {
            Form::Whole => word == listed,
            Form::Plural => word == listed || word.strip_suffix('s') == Some(listed),
            Form::Stem => word.strip_prefix(listed.as_str()).is_some_and(|ending| {
                ending.chars().count() <= self.ending
                    && ending.chars().all(char::is_alphabetic)
                    && !self.not_endings.iter().any(|not| not == ending)
            }),
        })
    }
}

struct Shape {
    category: Category,
    require: Vec<Regex>,
    forbid: Vec<Regex>,
    word: Option<Regex>,
    guess: Vec<Regex>,
    guess_words: Vec<String>,
    quotes: Vec<(char, char)>,
    quotes_after: Vec<String>,
    quotes_after_words: Vec<String>,
}

impl Pack {
    /// Parses and checks `files`, naming the file and entry of the first
    /// problem.
    fn load(files: &[(&str, &str)]) -> Result<Self, String> {
        let mut pack = Self {
            never: Vec::new(),
            shapes: Vec::new(),
            contexts: Vec::new(),
            spans: Vec::new(),
            references: Vec::new(),
        };
        let mut never_seen = std::collections::HashSet::new();
        for (name, source) in files {
            let file: File = toml::from_str(source).map_err(|e| format!("{name}: {e}"))?;
            for (index, entry) in file.never.into_iter().enumerate() {
                let at = |problem: String| format!("{name}: never #{}: {problem}", index + 1);
                let ending = match (entry.form, entry.ending) {
                    (Form::Stem, Some(ending)) if ending > 0 => ending,
                    (Form::Stem, _) => return Err(at("a stem needs an ending above 0".into())),
                    (_, Some(_)) => return Err(at("only a stem takes an ending".into())),
                    (_, None) => 0,
                };
                if entry.form != Form::Stem && !entry.not_endings.is_empty() {
                    return Err(at("only a stem takes not_endings".into()));
                }
                if entry.words.is_empty() {
                    return Err(at("no words".into()));
                }
                for word in &entry.words {
                    if word.is_empty() || word.chars().any(char::is_whitespace) {
                        return Err(at(format!("{word:?} is not one word")));
                    }
                    if *word != word.to_lowercase() {
                        return Err(at(format!("{word:?} is not lowercase")));
                    }
                    if !never_seen.insert((entry.form, word.clone())) {
                        return Err(at(format!("{word:?} is listed twice")));
                    }
                }
                let tokens = |tokens: &[String]| {
                    tokens
                        .iter()
                        .map(|token| category(token))
                        .collect::<Result<Vec<_>, _>>()
                        .map_err(at)
                };
                pack.never.push(Never {
                    form: entry.form,
                    ending,
                    words: entry.words,
                    last_word_of: tokens(&entry.last_word_of)?,
                    anywhere_in: tokens(&entry.anywhere_in)?,
                    not_endings: entry.not_endings,
                });
            }
            for (index, entry) in file.shape.into_iter().enumerate() {
                let at = |problem: String| format!("{name}: shape #{}: {problem}", index + 1);
                let category = category(&entry.category).map_err(at)?;
                let compile = |pattern: &str, whole: bool| {
                    let pattern = rust_classes(pattern);
                    let pattern = if whole {
                        format!("^(?:{pattern})$")
                    } else {
                        pattern
                    };
                    Regex::new(&pattern).map_err(|e| at(e.to_string()))
                };
                let compile_all = |patterns: &[String], whole: bool| {
                    patterns
                        .iter()
                        .map(|pattern| compile(pattern, whole))
                        .collect::<Result<Vec<_>, _>>()
                };
                if entry.require.is_empty()
                    && entry.forbid.is_empty()
                    && entry.word.is_none()
                    && entry.guess.is_empty()
                    && entry.guess_words.is_empty()
                    && entry.forbid_quotes.is_empty()
                    && entry.quotes_after.is_empty()
                    && entry.quotes_after_words.is_empty()
                {
                    return Err(at("no rules".into()));
                }
                let quotes = entry
                    .forbid_quotes
                    .iter()
                    .map(|pair| {
                        let mut chars = pair.chars();
                        match (chars.next(), chars.next(), chars.next()) {
                            (Some(open), Some(close), None) => Ok((open, close)),
                            _ => Err(at(format!(
                                "{pair:?} is not an opening and a closing quote"
                            ))),
                        }
                    })
                    .collect::<Result<Vec<_>, _>>()?;
                if !entry.quotes_after.is_empty() && quotes.is_empty() {
                    return Err(at("quotes_after needs forbid_quotes".into()));
                }
                if !entry.quotes_after_words.is_empty() && quotes.is_empty() {
                    return Err(at("quotes_after_words needs forbid_quotes".into()));
                }
                if let Some(word) = entry
                    .quotes_after_words
                    .iter()
                    .find(|word| word.is_empty() || !word.chars().all(char::is_alphanumeric))
                {
                    return Err(at(format!("{word:?} is not one word")));
                }
                if let Some(stem) = entry.quotes_after.iter().find(|stem| {
                    stem.is_empty()
                        || !stem.chars().all(char::is_alphanumeric)
                        || **stem != stem.to_lowercase()
                }) {
                    return Err(at(format!("{stem:?} is not a lowercase stem")));
                }
                if let Some(word) = entry
                    .guess_words
                    .iter()
                    .find(|word| word.is_empty() || !word.chars().all(char::is_alphanumeric))
                {
                    return Err(at(format!("{word:?} is not one word")));
                }
                let require = compile_all(&entry.require, false)?;
                let forbid = compile_all(&entry.forbid, false)?;
                let word = entry
                    .word
                    .as_deref()
                    .map(|word| compile(word, false))
                    .transpose()?;
                let guess = compile_all(&entry.guess, true)?;
                let shape = match pack.shapes.iter_mut().find(|s| s.category == category) {
                    Some(shape) => shape,
                    None => {
                        pack.shapes.push(Shape {
                            category,
                            require: Vec::new(),
                            forbid: Vec::new(),
                            word: None,
                            guess: Vec::new(),
                            guess_words: Vec::new(),
                            quotes: Vec::new(),
                            quotes_after: Vec::new(),
                            quotes_after_words: Vec::new(),
                        });
                        pack.shapes.last_mut().expect("just pushed")
                    }
                };
                if word.is_some() && shape.word.is_some() {
                    return Err(at(format!("a second word rule for {}", entry.category)));
                }
                shape.require.extend(require);
                shape.forbid.extend(forbid);
                shape.word = shape.word.take().or(word);
                shape.guess.extend(guess);
                shape.guess_words.extend(entry.guess_words);
                shape.quotes.extend(quotes);
                shape.quotes_after.extend(entry.quotes_after);
                shape.quotes_after_words.extend(entry.quotes_after_words);
            }
            for (index, entry) in file.context.iter().enumerate() {
                let at = |problem: String| format!("{name}: context #{}: {problem}", index + 1);
                let category = category(&entry.category).map_err(at)?;
                if entry.labels.is_empty() {
                    return Err(at("needs labels".into()));
                }
                // Whole words: a label starts and ends with a letter or digit.
                let edge = |c: Option<char>| c.is_some_and(char::is_alphanumeric);
                if let Some(label) = entry
                    .labels
                    .iter()
                    .find(|label| !edge(label.chars().next()) || !edge(label.chars().next_back()))
                {
                    return Err(at(format!(
                        "{label:?} must start and end with a letter or digit"
                    )));
                }
                // Spaces in a label match any run of whitespace.
                let labels: Vec<String> = entry
                    .labels
                    .iter()
                    .map(|label| {
                        label
                            .split_whitespace()
                            .map(regex::escape)
                            .collect::<Vec<_>>()
                            .join(r"\s+")
                    })
                    .collect();
                let pattern = format!(
                    // Case is ignored in the label only. Where the value
                    // ends is checked after matching ("1988 г." ends in a dot).
                    r"(?i)\b(?:{})\b(?-i:(?:{})((?:{})))",
                    labels.join("|"),
                    rust_classes(&entry.between),
                    rust_classes(&entry.value),
                );
                let pattern = Regex::new(&pattern).map_err(|e| at(e.to_string()))?;
                let value = Regex::new(&format!("^(?:{})$", rust_classes(&entry.value)))
                    .map_err(|e| at(e.to_string()))?;
                if pattern.captures_len() != 2 {
                    return Err(at("between and value must not capture".into()));
                }
                if let Some(word) = entry.unless_after.iter().find(|word| {
                    word.is_empty()
                        || *word != &word.to_lowercase()
                        || !word.chars().all(char::is_alphanumeric)
                }) {
                    return Err(at(format!("{word:?} is not one lowercase word")));
                }
                let unless_guessed = entry
                    .unless_guessed
                    .iter()
                    .map(|token| self::category(token))
                    .collect::<Result<Vec<_>, _>>()
                    .map_err(at)?;
                let only_guessed = entry
                    .only_guessed
                    .iter()
                    .map(|token| self::category(token))
                    .collect::<Result<Vec<_>, _>>()
                    .map_err(at)?;
                pack.contexts.push(Context {
                    category,
                    pattern,
                    labels: entry
                        .labels
                        .iter()
                        .map(|label| {
                            label
                                .split_whitespace()
                                .collect::<Vec<_>>()
                                .join(" ")
                                .to_lowercase()
                        })
                        .collect(),
                    value,
                    min_digits: entry.min_digits,
                    unless_after: entry.unless_after.clone(),
                    unless_guessed,
                    only_guessed,
                });
            }
            for (index, entry) in file.span.iter().enumerate() {
                let at = |problem: String| format!("{name}: span #{}: {problem}", index + 1);
                let category = category(&entry.category).map_err(at)?;
                let fixes = !entry.trim.is_empty()
                    || !entry.extend.is_empty()
                    || !entry.strip_end.is_empty()
                    || !entry.person_before.is_empty();
                if let Some(pattern) = &entry.pattern {
                    if fixes {
                        return Err(at(
                            "a pattern takes no trim, extend, strip_end or person_before".into(),
                        ));
                    }
                    let pattern = with_parts(pattern, &entry.parts).map_err(at)?;
                    pack.spans.push(Span::Pattern { category, pattern });
                    continue;
                }
                if !entry.parts.is_empty() {
                    return Err(at("parts need a pattern".into()));
                }
                if !fixes {
                    return Err(at(
                        "needs a pattern, trim words, extend patterns, strip_end or person_before"
                            .into(),
                    ));
                }
                if !entry.person_before.is_empty() && category != Category::Organization {
                    return Err(at("only an ORG fix takes person_before".into()));
                }
                for word in entry.trim.iter().chain(&entry.person_before) {
                    if word.is_empty() || word.chars().any(char::is_whitespace) {
                        return Err(at(format!("{word:?} is not one word")));
                    }
                    if *word != word.to_lowercase() {
                        return Err(at(format!("{word:?} is not lowercase")));
                    }
                }
                if entry.strip_end.chars().any(char::is_alphanumeric) {
                    return Err(at("strip_end takes punctuation only".into()));
                }
                let extend = entry
                    .extend
                    .iter()
                    .map(|pattern| {
                        Regex::new(&rust_classes(pattern)).map_err(|e| at(e.to_string()))
                    })
                    .collect::<Result<Vec<_>, _>>()?;
                pack.spans.push(Span::Fix(SpanFix {
                    category,
                    trim: entry.trim.clone(),
                    extend,
                    strip_end: entry.strip_end.clone(),
                    person_before: entry.person_before.clone(),
                }));
            }
            for (index, entry) in file.reference.iter().enumerate() {
                let at = |problem: String| format!("{name}: reference #{}: {problem}", index + 1);
                pack.references
                    .push(with_parts(&entry.pattern, &entry.parts).map_err(at)?);
            }
            for (index, entry) in file.variants.iter().enumerate() {
                let at = |problem: String| format!("{name}: variants #{}: {problem}", index + 1);
                if entry.name.trim().is_empty() {
                    return Err(at("no name".into()));
                }
                if entry.forms.is_empty()
                    || entry
                        .forms
                        .iter()
                        .any(|form| form.trim().is_empty() || *form == entry.name)
                {
                    return Err(at("needs forms, none blank or the name itself".into()));
                }
            }
        }
        Ok(pack)
    }

    /// Whether `word` is a title, role, determiner or other word that never
    /// names anyone on its own, ignoring case.
    pub(crate) fn never(&self, word: &str) -> bool {
        let word = word.to_lowercase();
        self.never
            .iter()
            // Public-body words only veto spans that contain them: "Суднев"
            // alone is a surname, not a court.
            .filter(|entry| entry.anywhere_in.is_empty())
            .any(|entry| entry.has(&word))
    }

    /// Whether a model span has its category's `shape` and is not a role
    /// phrase or public body by the `never` section.
    pub(crate) fn identifying(&self, text: &str, category: Category) -> bool {
        let words = || {
            text.split_whitespace()
                .map(|word| word.trim_matches(|c: char| !c.is_alphanumeric() && c != '-'))
                .filter(|word| !word.is_empty())
        };
        let lowercase: Vec<String> = words().map(str::to_lowercase).collect();
        // A legal form makes it a company, whatever else it says ("Garden
        // Court Ltd").
        let company = self.legal_form(text);
        let vetoed = self.never.iter().any(|entry| {
            (!company
                && entry.anywhere_in.contains(&category)
                && lowercase.iter().any(|w| entry.has(w)))
                || (entry.last_word_of.contains(&category)
                    && lowercase.last().is_some_and(|w| entry.has(w)))
        });
        if vetoed {
            return false;
        }
        let Some(shape) = self.shapes.iter().find(|s| s.category == category) else {
            return true;
        };
        shape.require.iter().all(|pattern| pattern.is_match(text))
            && !shape.forbid.iter().any(|pattern| pattern.is_match(text))
            && shape.word.as_ref().is_none_or(|pattern| {
                words().any(|word| pattern.is_match(word) && !self.never(word))
            })
    }

    /// The `span` fixes on `detections` in `source`: each grows to an
    /// `extend` match it lies inside, loses `trim` words at either edge and
    /// `strip_end` characters at its end. A span left without its shape
    /// goes, read as `view` reads it.
    pub(crate) fn fix_spans(
        &self,
        source: &str,
        view: &mdoc_pii::view::View,
        detections: &mut Vec<Detection>,
    ) {
        let fixes = self.spans.iter().filter_map(|span| match span {
            Span::Fix(fix) => Some(fix),
            Span::Pattern { .. } => None,
        });
        for fix in fixes {
            if !detections.iter().any(|d| d.category == fix.category) {
                continue;
            }
            let matches: Vec<_> = fix
                .extend
                .iter()
                .flat_map(|pattern| pattern.find_iter(source).map(|m| m.range()))
                .collect();
            // In reading order: a span never grows into part of another
            // span of its category, so in "Ветлин Н. О. Брюн Л. И." the
            // initials after "Ветлин" stay his.
            let mut order: Vec<usize> = (0..detections.len())
                .filter(|&i| detections[i].category == fix.category)
                .collect();
            order.sort_by_key(|&i| detections[i].range.start);
            for &i in &order {
                let range = detections[i].range.clone();
                let crosses = |m: &std::ops::Range<usize>| {
                    order.iter().any(|&j| {
                        let other = &detections[j].range;
                        j != i
                            && other.start < m.end
                            && m.start < other.end
                            && !(m.start <= other.start && other.end <= m.end)
                    })
                };
                // A person never grows back over a role or title before
                // it ("Ответчик" in "Ответчик Н. О." for the model's "Н.
                // О."); after it such a word is a surname ("J. R. Clerk").
                let over_never = |m: &std::ops::Range<usize>| {
                    fix.category == Category::Person
                        && source[m.start..range.start]
                            .split(|c: char| !c.is_alphanumeric() && c != '-')
                            .any(|word| !word.is_empty() && self.never(word))
                };
                let grown = matches
                    .iter()
                    .filter(|m| {
                        m.start <= range.start
                            && range.end <= m.end
                            && !crosses(m)
                            && !over_never(m)
                    })
                    .max_by_key(|m| m.len())
                    .cloned()
                    .unwrap_or(range);
                let trimmed = fix.trimmed(source, grown.clone());
                // A trim never leaves a company's legal form alone: "АО ТТН"
                // stays whole, "АЗС ООО «Бета-Ойл»" loses "АЗС".
                detections[i].range =
                    if trimmed != grown && self.only_legal_form(&source[trimmed.clone()]) {
                        grown
                    } else {
                        trimmed
                    };
            }
        }
        self.person_documents(source, detections);
        detections.retain(|d| {
            !d.range.is_empty()
                && self.identifying(&view.text[view.view_range(d.range.clone())], d.category)
        });
    }

    /// Whether a model span at `range` of `source` has its category's
    /// `shape`, its surroundings included: wrapped exactly in a forbidden
    /// quote pair, or holding one, after a `quotes_after` or
    /// `quotes_after_words` word, it is a title or a product ("повести
    /// «Кедровый свет»", "ПО «Контур»"); a name quoted elsewhere stays
    /// ("назван «Иван Петров»").
    /// `text` is the span as the shape reads it (entities decoded).
    pub(crate) fn identifying_at(
        &self,
        source: &str,
        range: std::ops::Range<usize>,
        text: &str,
        category: Category,
    ) -> bool {
        if source.get(range.clone()).is_none() {
            return false;
        }
        // Checked once grown: "АЗС" in "ООО «АЗС»" is a company's name, not
        // the abbreviation (#481), and so is "C.F.R." in "ООО «C.F.R.»".
        if self.grows_to_company(source, range.clone(), category) {
            return true;
        }
        if self.in_reference(source, range.clone()) {
            return false;
        }
        let quoted = self
            .shapes
            .iter()
            .find(|s| s.category == category)
            .is_some_and(|shape| {
                // Both sides as detection reads them: references decoded,
                // invisible characters gone ("&laquo;", "свет\u{200b}»").
                let mut from = range.start.saturating_sub(96);
                while !source.is_char_boundary(from) {
                    from += 1;
                }
                let mut to = (range.end + 32).min(source.len());
                while !source.is_char_boundary(to) {
                    to -= 1;
                }
                let head = mdoc_pii::view::decoded(&source[from..range.start]);
                let tail = mdoc_pii::view::decoded(&source[range.end..to]);
                // The span may hold its own quotes ("«Контур»" after "ПО").
                let inner = text.trim();
                let own = shape.quotes.iter().any(|(open, close)| {
                    inner.len() > open.len_utf8() + close.len_utf8()
                        && inner.starts_with(*open)
                        && inner.ends_with(*close)
                });
                let before = if own {
                    inner.chars().next()
                } else {
                    head.chars().next_back()
                };
                let Some(before) = before else {
                    return false;
                };
                let after = if own {
                    inner.chars().next_back()
                } else {
                    tail.chars().next()
                };
                let lead = if own {
                    &head[..]
                } else {
                    &head[..head.len() - before.len_utf8()]
                };
                let written = lead
                    .trim_end()
                    .rsplit(|c: char| !c.is_alphanumeric())
                    .next()
                    .unwrap_or_default();
                // A cue is a common noun: "Роман «Сокол»" names a person
                // called Роман, whose nickname stays.
                let word = written.to_lowercase();
                let cue = word == written;
                shape
                    .quotes
                    .iter()
                    .any(|(open, close)| before == *open && after == Some(*close))
                    && (shape.quotes_after.is_empty() && shape.quotes_after_words.is_empty()
                        // An abbreviation as written: "ПО «Контур»", not "по «…»".
                        || shape.quotes_after_words.iter().any(|listed| *listed == written)
                        || cue
                            && shape.quotes_after.iter().any(|stem| {
                                word.strip_prefix(stem.as_str())
                                    .is_some_and(|ending| NOUN_ENDINGS.contains(&ending))
                            }))
            });
        self.identifying(text, category) && !quoted
    }

    /// Whether `range` of `source` lies inside a `reference` match, a legal
    /// reference ("424 U.S. at 38", "статьи 431 ГК РФ"): no identifier,
    /// address or name (guide rule 8). The span's lines are searched as
    /// detection reads them ("&sect;" is "§"), and again with the line on
    /// each side joined where a citation may wrap onto it ("904 F.2d\n275,
    /// 277"): a soft break between two lines of running text.
    fn in_reference(&self, source: &str, range: std::ops::Range<usize>) -> bool {
        if self.references.is_empty() {
            return false;
        }
        let line_start = |at: usize| source[..at].rfind('\n').map_or(0, |i| i + 1);
        let line_end = |at: usize| source[at..].find('\n').map_or(source.len(), |i| at + i);
        let start = line_start(range.start);
        let end = line_end(range.end);
        let inside = |from: usize, text: &str| {
            let view = mdoc_pii::view::View::new(text);
            let span = view.view_range(range.start - from..range.end - from);
            self.references.iter().any(|pattern| {
                span_ranges(pattern, &view.text)
                    .into_iter()
                    .any(|found| found.start <= span.start && span.end <= found.end)
            })
        };
        if inside(start, &source[start..end]) {
            return true;
        }
        let mut wide = start..end;
        if start > 0 {
            let before = line_start(start - 1);
            if soft_break(&source[before..start - 1], &source[start..end]) {
                wide.start = before;
            }
        }
        if end < source.len() {
            let after = line_end(end + 1);
            if soft_break(&source[line_start(end)..end], &source[end + 1..after]) {
                wide.end = after;
            }
        }
        // One space for each line break: offsets stay the source's.
        let lines = &source[wide.clone()];
        lines.contains('\n') && inside(wide.start, &lines.replace(['\r', '\n'], " "))
    }

    /// An organization span ending in a `person_before` word ("Ostrand
    /// Decl.") becomes the person before it ("Ostrand") when a person's span
    /// holds each of that name's words: a declaration is a person's, never
    /// a company.
    fn person_documents(&self, source: &str, detections: &mut [Detection]) {
        let words: Vec<&String> = self
            .spans
            .iter()
            .filter_map(|span| match span {
                Span::Fix(fix) => Some(&fix.person_before),
                Span::Pattern { .. } => None,
            })
            .flatten()
            .collect();
        if words.is_empty() {
            return;
        }
        let name_words = |text: &str| -> Vec<String> {
            text.split_whitespace()
                .map(|word| word.trim_matches(|c: char| !c.is_alphanumeric() && c != '-'))
                .filter(|word| !word.is_empty())
                .map(str::to_owned)
                .collect()
        };
        let people: Vec<Vec<String>> = detections
            .iter()
            .filter(|d| d.category == Category::Person)
            .map(|d| name_words(&source[d.range.clone()]))
            .collect();
        for detection in detections
            .iter_mut()
            .filter(|d| d.category == Category::Organization)
        {
            let text = source[detection.range.clone()].trim_end();
            let Some((name, last)) = text.rsplit_once(char::is_whitespace) else {
                continue;
            };
            let last = last.trim_end_matches('.').to_lowercase();
            let name = name.trim_end();
            let wanted = name_words(name);
            if !words.iter().any(|word| **word == last)
                || wanted.is_empty()
                || !people
                    .iter()
                    .any(|person| wanted.iter().all(|word| person.contains(word)))
            {
                continue;
            }
            let at = detection.range.start + (text.len() - text.trim_start().len());
            let name = name.trim_start();
            detection.range = at..at + name.len();
            detection.category = Category::Person;
        }
    }

    /// Values beside their labels in `source`, as `context` rules propose
    /// them: the value only, without its label.
    pub(crate) fn context(&self, source: &str) -> Vec<Detection> {
        let mut found: Vec<Detection> = self
            .contexts
            .iter()
            .flat_map(|context| {
                context.pattern.captures_iter(source).filter_map(|found| {
                    let value = found.get(1)?;
                    // The value ends where a word does ("1234_5678" is one word).
                    let ends = !source[value.end()..]
                        .chars()
                        .next()
                        .is_some_and(|c| c.is_alphanumeric() || c == '_');
                    let before = source[..found.get(0)?.start()]
                        .trim_end()
                        .rsplit(|c: char| !c.is_alphanumeric())
                        .next()
                        .unwrap_or_default()
                        .to_lowercase();
                    if !ends
                        || context.unless_after.contains(&before)
                        || !self.takes(context, value.as_str())
                    {
                        return None;
                    }
                    Some(Detection {
                        range: value.range(),
                        category: context.category,
                        score: CONTEXT_SCORE,
                        recognizer: Recognizer::Context,
                    })
                })
            })
            .collect();
        found.extend(self.table_columns(source));
        found
    }

    /// Whether `context` proposes `value`: enough digits, no guess that rules
    /// it out (a date), and the guess it needs if it names one (a date of
    /// birth must look like a date).
    fn takes(&self, context: &Context, value: &str) -> bool {
        let guessed = self.guess(value);
        value.chars().filter(char::is_ascii_digit).count() >= context.min_digits
            && !guessed.is_some_and(|guessed| context.unless_guessed.contains(&guessed))
            && (context.only_guessed.is_empty()
                || guessed.is_some_and(|guessed| context.only_guessed.contains(&guessed)))
    }

    /// Values in Markdown table columns whose header cell is a label ("| ИНН
    /// | СНИЛС |"): each cell that is a whole value of that label's rule.
    fn table_columns(&self, source: &str) -> Vec<Detection> {
        let mut found = Vec::new();
        let mut lines = Vec::new();
        let mut at = 0;
        for line in source.split_inclusive('\n') {
            lines.push((at, line.trim_end_matches(['\n', '\r'])));
            at += line.len();
        }
        let mut row = 0;
        while row + 1 < lines.len() {
            let (header_at, header) = lines[row];
            if !is_table_row(header) || !is_delimiter_row(lines[row + 1].1) {
                row += 1;
                continue;
            }
            // The context rule each column's header names, if any.
            let columns: Vec<Option<&Context>> = cells(header, header_at)
                .into_iter()
                .map(|cell| {
                    let label = source[cell]
                        .trim_matches(|c: char| {
                            c == '*' || c == '_' || c == ':' || c.is_whitespace()
                        })
                        .split_whitespace()
                        .collect::<Vec<_>>()
                        .join(" ")
                        .to_lowercase();
                    self.contexts
                        .iter()
                        .find(|context| context.labels.contains(&label))
                })
                .collect();
            row += 2;
            // Up to the next table's header, which a delimiter row follows.
            while row < lines.len()
                && is_table_row(lines[row].1)
                && !lines
                    .get(row + 1)
                    .is_some_and(|(_, next)| is_delimiter_row(next))
            {
                let (line_at, line) = lines[row];
                for (cell, context) in cells(line, line_at).into_iter().zip(&columns) {
                    if let Some(context) = context
                        && context.value.is_match(&source[cell.clone()])
                        && self.takes(context, &source[cell.clone()])
                    {
                        found.push(Detection {
                            range: cell,
                            category: context.category,
                            score: CONTEXT_SCORE,
                            recognizer: Recognizer::Context,
                        });
                    }
                }
                row += 1;
            }
        }
        found
    }

    /// Whether `text` has an organization's legal form (the ORG shape's
    /// `guess_words`: Ltd, LLC, ООО…).
    fn legal_form(&self, text: &str) -> bool {
        self.shapes
            .iter()
            .filter(|shape| shape.category == Category::Organization)
            .any(|shape| {
                text.split(|c: char| !c.is_alphanumeric()).any(|word| {
                    shape
                        .guess_words
                        .iter()
                        .any(|form| form.eq_ignore_ascii_case(word))
                })
            })
    }

    /// Whether `text`'s words are all legal forms ("АО", "LLC").
    fn only_legal_form(&self, text: &str) -> bool {
        let mut words = text
            .split(|c: char| !c.is_alphanumeric())
            .filter(|word| !word.is_empty())
            .peekable();
        words.peek().is_some() && words.all(|word| self.legal_form(word))
    }

    /// Whether an organization `span` fix grows the span at `range` of
    /// `source` to a company's whole name ("АЗС" in "ООО «АЗС»", "ЭДО" in
    /// "АНО «ЭДО»"; every ORG `extend` is a name with its legal form), so
    /// its shape is checked on the grown span (`fix_spans`). Fixes stay on
    /// one line, so only the span's lines are searched.
    fn grows_to_company(
        &self,
        source: &str,
        range: std::ops::Range<usize>,
        category: Category,
    ) -> bool {
        if category != Category::Organization {
            return false;
        }
        let start = source[..range.start].rfind('\n').map_or(0, |at| at + 1);
        let end = source[range.end..]
            .find('\n')
            .map_or(source.len(), |at| range.end + at);
        self.spans
            .iter()
            .filter_map(|span| match span {
                Span::Fix(fix) if fix.category == category => Some(fix),
                _ => None,
            })
            .flat_map(|fix| &fix.extend)
            .flat_map(|pattern| pattern.find_iter(&source[start..end]))
            .any(|found| {
                let found_range = start + found.start()..start + found.end();
                found_range.start <= range.start
                    && range.end <= found_range.end
                    && found_range.len() > range.len()
            })
    }

    /// Whole spans the `span` patterns find in `source`: an address from
    /// its first element to its postcode or city (ADR 0092).
    pub(crate) fn spans(&self, source: &str) -> Vec<Detection> {
        self.spans
            .iter()
            .filter_map(|span| match span {
                Span::Pattern { category, pattern } => Some((*category, pattern)),
                Span::Fix(_) => None,
            })
            .flat_map(|(category, pattern)| {
                span_ranges(pattern, source)
                    .into_iter()
                    // A match before a colon ends in a field's label ("12
                    // Oak Road, Email: …"), not the address's last part.
                    .filter(|range| {
                        !source[range.end..]
                            .trim_start_matches([' ', '\t'])
                            .starts_with(':')
                    })
                    .map(move |range| Detection {
                        range,
                        category,
                        score: CONTEXT_SCORE,
                        recognizer: Recognizer::Span,
                    })
            })
            .collect()
    }

    /// The first category, in pack order, whose guesses take a manual
    /// selection.
    pub(crate) fn guess(&self, text: &str) -> Option<Category> {
        self.shapes
            .iter()
            .find(|shape| {
                shape.guess.iter().any(|pattern| pattern.is_match(text))
                    || (!shape.guess_words.is_empty()
                        && text.split(|c: char| !c.is_alphanumeric()).any(|word| {
                            shape
                                .guess_words
                                .iter()
                                .any(|listed| listed.eq_ignore_ascii_case(word))
                        }))
            })
            .map(|shape| shape.category)
    }
}

/// `pattern` with each `{name}` replaced by `parts.name`, compiled: every
/// part used, no empty match (an empty span is no detection: the review
/// would discard the whole scan) and no group but `lead` and `tail`.
fn with_parts(
    pattern: &str,
    parts: &std::collections::BTreeMap<String, String>,
) -> Result<Regex, String> {
    let mut pattern = pattern.to_owned();
    for (part, regex) in parts {
        let name = format!("{{{part}}}");
        if !pattern.contains(&name) {
            return Err(format!("part {part:?} is never used"));
        }
        pattern = pattern.replace(&name, &format!("(?:{regex})"));
    }
    let pattern = Regex::new(&rust_classes(&pattern)).map_err(|e| e.to_string())?;
    if pattern.is_match("") {
        return Err("the pattern matches an empty span".into());
    }
    if let Some(group) = pattern
        .capture_names()
        .flatten()
        .find(|group| !["lead", "tail"].contains(group))
    {
        return Err(format!("group {group:?} is not `lead` or `tail`"));
    }
    Ok(pattern)
}

/// A span pattern's matches without their `lead` and `tail` groups, the
/// context around a span ("адресу " before a city). Each search starts
/// where the last span ended, so one address's tail ("; ") can be the next
/// one's lead.
fn span_ranges(pattern: &Regex, source: &str) -> Vec<std::ops::Range<usize>> {
    let mut ranges = Vec::new();
    let mut at = 0;
    while at <= source.len()
        && let Some(found) = pattern.captures_at(source, at)
    {
        let whole = found.get(0).expect("group 0 is the match");
        let start = found.name("lead").map_or(whole.start(), |lead| lead.end());
        let end = found
            .name("tail")
            .map_or(whole.end(), |tail| tail.start())
            .max(start);
        if start < end {
            ranges.push(start..end);
        }
        at = if end > at {
            end
        } else {
            whole.start()
                + source[whole.start()..]
                    .chars()
                    .next()
                    .map_or(1, char::len_utf8)
        };
    }
    ranges
}

/// Whether the break between lines `first` and `second` may wrap running
/// text: neither is blank, a heading or a table row, and the second opens
/// no quote, list item or numbered item.
fn soft_break(first: &str, second: &str) -> bool {
    let (first, second) = (first.trim(), second.trim());
    let numbered = second
        .trim_start_matches(|c: char| c.is_ascii_digit())
        .starts_with(['.', ')'])
        && second.starts_with(|c: char| c.is_ascii_digit());
    !first.is_empty()
        && !second.is_empty()
        && !first.starts_with(['|', '#'])
        && !second.starts_with(['|', '#', '>', '-', '*', '+'])
        && !numbered
}

fn is_table_row(line: &str) -> bool {
    line.trim_start().starts_with('|')
}

/// "| --- | :-: |": the row under a table's header.
fn is_delimiter_row(line: &str) -> bool {
    let line = line.trim();
    line.starts_with('|')
        && line.contains('-')
        && line
            .chars()
            .all(|c| matches!(c, '|' | '-' | ':' | ' ' | '\t'))
}

/// The byte ranges of a table row's cells in the source, each trimmed of
/// spaces; an escaped `\|` stays inside its cell.
fn cells(line: &str, at: usize) -> Vec<std::ops::Range<usize>> {
    let mut cells = Vec::new();
    let mut start = None;
    let mut escaped = false;
    for (i, c) in line.char_indices() {
        if c == '|' && !escaped {
            if let Some(from) = start {
                let text: &str = &line[from..i];
                let lead = text.len() - text.trim_start().len();
                let trimmed = text.trim();
                cells.push(at + from + lead..at + from + lead + trimmed.len());
            }
            start = Some(i + 1);
        }
        escaped = c == '\\' && !escaped;
    }
    // A trailing pipe is optional: the rest of the line is the last cell.
    if let Some(from) = start {
        let text = &line[from..];
        let trimmed = text.trim();
        if !trimmed.is_empty() {
            let lead = text.len() - text.trim_start().len();
            cells.push(at + from + lead..at + from + lead + trimmed.len());
        }
    }
    cells
}

/// `pattern` with `\p{Uppercase}`, `\p{Alphabetic}` and `\p{N}` meaning
/// exactly Rust's `char::is_uppercase`, `is_alphabetic` and `is_numeric`:
/// `regex`'s Unicode tables can be a version behind the compiler's, and the
/// rest of detection splits and trims words with Rust's.
fn rust_classes(pattern: &str) -> String {
    static CLASSES: OnceLock<[(&str, String); 3]> = OnceLock::new();
    let classes = CLASSES.get_or_init(|| {
        [
            (r"\p{Uppercase}", class(char::is_uppercase)),
            (r"\p{Alphabetic}", class(char::is_alphabetic)),
            (r"\p{N}", class(char::is_numeric)),
        ]
    });
    classes
        .iter()
        .fold(pattern.to_owned(), |pattern, (name, class)| {
            pattern.replace(name, class)
        })
}

/// A bracketed class of every scalar value `predicate` takes; it nests
/// inside another class too.
fn class(predicate: fn(char) -> bool) -> String {
    let mut class = String::from("[");
    let mut run: Option<(char, char)> = None;
    let flush = |run: (char, char), class: &mut String| {
        class.push_str(&format!(
            "\\x{{{:X}}}-\\x{{{:X}}}",
            run.0 as u32, run.1 as u32
        ));
    };
    for c in (0..=char::MAX as u32)
        .filter_map(char::from_u32)
        .filter(|&c| predicate(c))
    {
        run = match run {
            Some((start, end)) if end as u32 + 1 == c as u32 => Some((start, c)),
            Some(done) => {
                flush(done, &mut class);
                Some((c, c))
            }
            None => Some((c, c)),
        };
    }
    if let Some(done) = run {
        flush(done, &mut class);
    }
    class.push(']');
    class
}

/// A category by its token (`PERSON`, `ORG`, …).
fn category(token: &str) -> Result<Category, String> {
    Category::ALL
        .into_iter()
        .find(|category| category.token() == token)
        .ok_or_else(|| format!("unknown category {token:?}"))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_rule_file_loads_and_each_entry_is_commented() {
        for file in FILES {
            Pack::load(&[file]).unwrap_or_else(|problem| panic!("{problem}"));
            // Each entry and section opens under a comment saying what it is for.
            let lines: Vec<&str> = file.1.lines().collect();
            for (index, line) in lines.iter().enumerate() {
                let opens = line.starts_with("[[")
                    || ["never", "context", "shape", "span", "variants", "reference"]
                        .iter()
                        .any(|section| line.starts_with(&format!("{section} =")));
                if opens {
                    assert!(
                        index > 0 && lines[index - 1].starts_with('#'),
                        "{}:{}: {line} needs a comment above it",
                        file.0,
                        index + 1
                    );
                }
            }
        }
        let pack = pack();
        // Manual selections try dates before phones (2026-03-12 has eight
        // digits) and legal forms before name shapes ("Acme Ltd").
        let order: Vec<_> = pack.shapes.iter().map(|s| s.category).collect();
        let position = |category| order.iter().position(|&c| c == category).unwrap();
        assert!(position(Category::Date) < position(Category::Phone));
        assert!(position(Category::Organization) < position(Category::Person));
    }

    /// The pack's Unicode classes follow the compiler's tables, which can be
    /// newer than `regex`'s: U+A7CE is uppercase and U+11DE0 a digit in
    /// Unicode 17.
    #[test]
    fn letters_and_digits_mean_what_rust_says() {
        assert!('\u{A7CE}'.is_uppercase() && '\u{11DE0}'.is_numeric());
        let pack = pack();
        assert!(pack.identifying("\u{A7CE}a", Category::Person));
        assert_eq!(pack.guess("\u{A7CE}a"), Some(Category::Person));
        assert!(pack.identifying("\u{11DE0}", Category::Phone));
        for (name, predicate) in [
            (r"\p{Uppercase}", char::is_uppercase as fn(char) -> bool),
            (r"\p{Alphabetic}", char::is_alphabetic),
            (r"\p{N}", char::is_numeric),
        ] {
            let class = Regex::new(&format!("^{}$", rust_classes(name))).unwrap();
            for c in (0..=char::MAX as u32).filter_map(char::from_u32) {
                assert_eq!(
                    class.is_match(c.encode_utf8(&mut [0; 4])),
                    predicate(c),
                    "{name} {c:?}"
                );
            }
        }
        // Shapes split words and trim with `\s`, as Rust does with
        // `split_whitespace` and `trim`.
        let space = Regex::new(r"^\s$").unwrap();
        for c in (0..=char::MAX as u32).filter_map(char::from_u32) {
            assert_eq!(
                space.is_match(c.encode_utf8(&mut [0; 4])),
                c.is_whitespace(),
                "{c:?}"
            );
        }
    }

    /// Claim and case numbers after their label, the value only (ADR 0092);
    /// statute and article numbers and "case" as a common word stay out.
    #[test]
    fn labelled_case_numbers_are_proposed() {
        let found = |source: &str| -> Vec<(String, Category)> {
            pack()
                .context(source)
                .into_iter()
                .map(|d| (source[d.range].to_owned(), d.category))
                .collect()
        };
        for (source, value) in [
            ("**Claim number:** 6001842/2026 (Employment", "6001842/2026"),
            ("Claim No. 1234 was settled", "1234"),
            ("Case No. 1:26-cv-01234-JAB, filed", "1:26-cv-01234-JAB"),
            ("Claim No CL-2026-000123.", "CL-2026-000123"),
            ("Case No. 2026/314A; see", "2026/314A"),
            ("Дело № 1234", "1234"),
            ("**Дело №** 2-1842/2026", "2-1842/2026"),
            ("proceedings are numbered 2026/314.", "2026/314"),
            ("case number 7707083894.", "7707083894"),
            ("Formula x + 2; case 202-555-0169.", "202-555-0169"),
            ("**Номер дела:** 2-1842/2026", "2-1842/2026"),
            ("дело № А40-123456/2026.", "А40-123456/2026"),
            ("Article 309; дело А40-987654/2026; цена", "А40-987654/2026"),
        ] {
            let found = found(source);
            assert!(!found.is_empty(), "{source}");
            for (text, category) in found {
                assert_eq!((text.as_str(), category), (value, Category::CaseNumber));
            }
        }
        for source in [
            "Under section 4.2 and article 309, the claim above is settled.",
            "in case 30 days pass",
            "in this case. 12345 people",
            "case abc12345",
            "по делу 15 свидетелей",
            "showcase 2026/314",
            "Claim No. 12. Under article 12 and section 12, payment is due.",
            "In case 2026-03-12 is a non-business day, payment falls due.",
            "Case No. 12.03.2026",
            "Дело № 12",
            "необходимо архивное дело 58239 из фонда",
            "личное дело № 4521 работника",
            "Case No. 1234_5678. See section 1234.",
        ] {
            assert!(found(source).is_empty(), "{source}");
        }
    }

    /// Russian identifiers take their label's category whatever their
    /// checksum, beside the label or under it as a table column's header.
    #[test]
    fn labelled_russian_identifiers_take_their_category() {
        let found = |source: &str| -> Vec<(String, Category)> {
            let mut found: Vec<_> = pack()
                .context(source)
                .into_iter()
                .map(|d| (source[d.range].to_owned(), d.category))
                .collect();
            found.sort_by(|a, b| a.0.cmp(&b.0));
            found
        };
        let tax = |v: &str| (v.to_owned(), Category::Tax);
        let id = |v: &str| (v.to_owned(), Category::Identity);
        assert_eq!(
            found("ИНН: 7701234567; СНИЛС: 123-456-789 00; ИНН: 77O1234567."),
            [id("123-456-789 00"), tax("7701234567"), tax("77O1234567")]
        );
        assert_eq!(found("**СНИЛС:** 000-000-000 00"), [id("000-000-000 00")]);
        assert_eq!(
            found("Код карточки: CARD-918264<br>Счёт"),
            [id("CARD-918264")]
        );
        assert_eq!(
            found(
                "| ИНН | **СНИЛС** | Цена |\n| --- | :-: | --- |\n| 5401000039 | 345-678-912 26 | 1234567890 |\n| нет | — | 12 |\n"
            ),
            [id("345-678-912 26"), tax("5401000039")]
        );
        for source in [
            "Договор № 5401000039, цена 660100000231 рублей.",
            "ИНН: 123; СНИЛС: 12-34",
            "| Номер | Цена |\n| --- | --- |\n| 5401000039 | 345-678-912 26 |\n",
            "ИНН\n\n| 5401000039 |",
            // An adjacent table's columns are its own.
            "| ИНН |\n| --- |\n| 7701234567 |\n| Цена |\n| --- |\n| 1234567890 |\n",
        ] {
            let found = found(source);
            assert!(
                found
                    .iter()
                    .all(|(v, _)| v != "1234567890" && v != "5401000039"),
                "{source}: {found:?}"
            );
        }
        // No trailing pipe: the last cell still counts.
        assert_eq!(
            found("| Заказчик | ИНН\n| --- | ---\n| ООО | 7701234567\n"),
            [tax("7701234567")]
        );
        for source in [
            "Договор № 5401000039, цена 660100000231 рублей.",
            "ИНН: 123; СНИЛС: 12-34",
        ] {
            assert!(found(source).is_empty(), "{source}");
        }
    }

    /// Digits that are not the model's category: long runs and bank codes
    /// are no phones, bare ten- or eleven-digit numbers no bank details, a
    /// rate or a writ series no tax identifier, a host without a dot no
    /// email. Real ones stay.
    #[test]
    fn shapes_drop_numbers_that_are_not_their_category() {
        let keeps = |text: &str, category| pack().identifying(text, category);
        for (text, category) in [
            ("123456789012345678901234567890", Category::Phone),
            ("+1234567890123456789012345", Category::Phone),
            ("044525593", Category::Phone),
            ("12345678900", Category::Bank),
            ("5401000039", Category::Bank),
            ("20 percent tax", Category::Tax),
            ("НДС 20%", Category::Tax),
            ("ФС 086731924", Category::Tax),
            ("not-a-mail@localhost", Category::Email),
        ] {
            assert!(!keeps(text, category), "{text}");
        }
        for (text, category) in [
            ("+44 7700 900461", Category::Phone),
            ("8 (495) 123-45-67", Category::Phone),
            ("40702810900000001234", Category::Bank),
            ("GB28NWBK60161331926819", Category::Bank),
            ("044525225", Category::Bank),
            ("12345", Category::Bank),
            ("7707083893", Category::Tax),
            ("GB 123 4567 89", Category::Tax),
            ("maya.bennett@example.com", Category::Email),
        ] {
            assert!(keeps(text, category), "{text}");
        }
    }

    /// An English organization in sentence case is a heading, a document
    /// kind or a team; title case, legal forms and Russian names stay.
    #[test]
    fn english_sentence_case_is_no_organization() {
        let org = |text: &str| pack().identifying(text, Category::Organization);
        for text in [
            "Distribution register",
            "Lasting power of attorney",
            "Financial authority",
            "Accounts team",
        ] {
            assert!(!org(text), "{text}");
        }
        for text in [
            "Harrow & Finch Ltd",
            "Copper Kite Metrology LLP",
            // One title-case word; Ofcom is a public body, never marked (#378).
            "Vantora",
            "Elm Row Legal LLP",
            "Переплётная орбита",
            "Sable Chime Music Ltd",
        ] {
            assert!(org(text), "{text}");
        }
    }

    /// English labels decide a bank account (a slash between sort code and
    /// number is fine) and a staff ID, even one of letters only.
    #[test]
    fn english_labels_decide_bank_accounts_and_staff_ids() {
        let found = |source: &str| -> Vec<(String, Category)> {
            pack()
                .context(source)
                .into_iter()
                .map(|d| (source[d.range].to_owned(), d.category))
                .collect()
        };
        assert_eq!(
            found("**Bank account:** 47-29-16/00318572"),
            [("47-29-16/00318572".to_owned(), Category::Bank)]
        );
        assert_eq!(
            found("<td>Bank account: 62-18-43/00492716<br>GBP 12</td>"),
            [("62-18-43/00492716".to_owned(), Category::Bank)]
        );
        assert_eq!(
            found("IBAN: GB28NWBK60161331926819."),
            [("GB28NWBK60161331926819".to_owned(), Category::Bank)]
        );
        assert_eq!(
            found("**Staff ID:** VELK-QUEN"),
            [("VELK-QUEN".to_owned(), Category::Identity)]
        );
        assert_eq!(
            found("Employee ID: TERN-WISP, signed"),
            [("TERN-WISP".to_owned(), Category::Identity)]
        );
        for source in [
            "the bank account was closed in 2026",
            "Staff ID: pending",
            "Employee ID: Maya Bennett",
            "account number 12345",
        ] {
            assert!(found(source).is_empty(), "{source}");
        }
    }

    /// A name wrapped exactly in «» is a title, not a person; one beside
    /// quotes, or in straight quotes, stays.
    #[test]
    fn a_name_in_guillemets_is_a_title() {
        let person = |source: &str, name: &str| {
            let start = source.find(name).unwrap();
            pack().identifying_at(source, start..start + name.len(), name, Category::Person)
        };
        assert!(!person(
            "рукопись повести «Кедровый свет».",
            "Кедровый свет"
        ));
        for source in [
            "рукопись повести &laquo;Кедровый свет&raquo;.",
            "рукопись повести &#171;Кедровый свет&#187;.",
            "рукопись повести &#xAB;Кедровый свет&#xBB;.",
        ] {
            assert!(!person(source, "Кедровый свет"), "{source}");
        }
        assert!(person("Истец «Иван Петров» и", "Петров"));
        assert!(person("Истец &laquo;Иван Петров", "Петров"));
        // A real name in «» elsewhere stays a person, quotes written out or
        // as references.
        for source in [
            "В заявлении потерпевший назван «Иван Петров».",
            "В заявлении потерпевший назван &laquo;Иван Петров&raquo;.",
            "по прозвищу «Лёша Гром» в",
            "Игрок «Лёша Гром» дал показания",
            "Свидетель записал имя коллеги «Иван Петров».",
            "товарищ «Иван Петров» пришёл",
            "программист «Иван Петров» в",
            "Свидетель Роман «Сокол» дал показания",
            "Об этом рассказал «Иван Петров».",
            "в команде играл «Иван Петров» и",
            "его друг Марк «Сокол» в",
            // A company named after a person is the ORG span's; the person
            // span stays rather than leave the name unproposed.
            "ООО «Вера Снегирь» в",
            "коллегии «Вера Снегирь» в",
        ] {
            let name = ["Лёша Гром", "Сокол", "Вера Снегирь"]
                .into_iter()
                .find(|name| source.contains(name))
                .unwrap_or("Иван Петров");
            assert!(person(source, name), "{source}");
        }
        // A work, a product or a company named in «» is no person.
        for (source, name) in [
            ("роман «Анна Каренина» в", "Анна Каренина"),
            ("журнал «Вера Снегирь» за", "Вера Снегирь"),
            ("товарный знак «Иван Царевич» в", "Иван Царевич"),
            ("повести&nbsp;«Кедровый свет» в", "Кедровый свет"),
            (
                "повести&#160;&laquo;Кедровый свет&raquo; в",
                "Кедровый свет",
            ),
            ("в игре «Иван Царевич» за", "Иван Царевич"),
            ("в его рассказе «Иван Петров» есть", "Иван Петров"),
            ("над романом «Анна Каренина» в", "Анна Каренина"),
            ("сценарий «Иван Царевич» для", "Иван Царевич"),
            ("повести «Кедровый свет\u{200b}» в", "Кедровый свет"),
            ("повести \u{200b}«Кедровый свет» в", "Кедровый свет"),
        ] {
            assert!(!person(source, name), "{source}");
        }
        assert!(person("Истец Иван Петров.", "Иван Петров"));
        assert!(person("name=\"Alice Morgan\"", "Alice Morgan"));
    }

    /// What the review shows: a case number's other mentions join it, and a
    /// labelled number too short to propose never spreads to statute and
    /// clause numbers (the review finds a wording everywhere).
    #[test]
    fn a_case_number_reaches_its_repeats_and_never_a_statute_number() {
        let shown = |source: &str| -> Vec<String> {
            crate::recognition::proposals(source, &[])
                .unwrap()
                .into_iter()
                .filter(|span| span.category == "CASE")
                .map(|span| source[span.range].to_owned())
                .collect()
        };
        assert!(shown("Claim No. 12. Under article 12 and section 12, payment is due.").is_empty());
        assert_eq!(
            shown("Claim No 6001842/2026. The claim 6001842/2026 is settled under article 309."),
            ["6001842/2026", "6001842/2026"]
        );
    }

    fn fixed(source: &str, spans: &[(&str, Category)]) -> Vec<String> {
        let mut detections: Vec<Detection> = spans
            .iter()
            .map(|(text, category)| {
                let start = source.find(text).unwrap();
                Detection {
                    range: start..start + text.len(),
                    category: *category,
                    score: 0.9,
                    recognizer: Recognizer::Model,
                }
            })
            .collect();
        pack().fix_spans(source, &mdoc_pii::view::View::new(source), &mut detections);
        detections
            .into_iter()
            .map(|d| source[d.range].to_owned())
            .collect()
    }

    #[test]
    fn span_fixes_trim_titles_keep_legal_forms_and_end_before_the_period() {
        // Titles and post-nominals off a person (#382); initials keep theirs.
        assert_eq!(
            fixed(
                "Ms Bennett met Dr. Ysolde Qhenn, KC and Jorin P.",
                &[
                    ("Ms Bennett", Category::Person),
                    ("Dr. Ysolde Qhenn, KC", Category::Person),
                    ("Jorin P.", Category::Person),
                ]
            ),
            ["Bennett", "Ysolde Qhenn", "Jorin P."]
        );
        assert_eq!(
            fixed(
                "Г-жа Павлова пришла.",
                &[("Г-жа Павлова", Category::Person)]
            ),
            ["Павлова"]
        );
        // A title alone keeps no span.
        assert!(fixed("Dear Ms,", &[("Ms", Category::Person)]).is_empty());
        // An organization's legal form and quotes (#374), not the period.
        assert_eq!(
            fixed(
                "Истец: ООО «Северный Вектор», ответчик АО \"Ёлка\". Signed by Acme Ltd.",
                &[
                    ("Северный Вектор", Category::Organization),
                    ("Ёлка", Category::Organization),
                    ("Acme Ltd.", Category::Organization),
                ]
            ),
            ["ООО «Северный Вектор»", "АО \"Ёлка\"", "Acme Ltd"]
        );
        // Quotes without a legal form, and a person, stay as they are.
        assert_eq!(
            fixed(
                "Роман «Кедровый свет» и «Вера Снегирь»",
                &[("Вера Снегирь", Category::Person)]
            ),
            ["Вера Снегирь"]
        );
        assert_eq!(
            fixed("журнал «Север»", &[("Север", Category::Organization)]),
            ["Север"]
        );
    }

    #[test]
    fn an_initial_keeps_its_period_a_sentence_loses_it() {
        assert_eq!(
            fixed(
                "Исполнитель: ИП Белявская О. М. Заказчик: ООО «Ромашка».",
                &[
                    ("ИП Белявская О. М.", Category::Organization),
                    ("ООО «Ромашка».", Category::Organization),
                ]
            ),
            ["ИП Белявская О. М.", "ООО «Ромашка»"]
        );
        assert_eq!(
            fixed(
                "Исполнитель: ИП Белявская О.М.. Подрядчик: Альфа-Строй.",
                &[
                    ("ИП Белявская О.М..", Category::Organization),
                    ("Альфа-Строй.", Category::Organization),
                ]
            ),
            ["ИП Белявская О.М.", "Альфа-Строй"]
        );
        assert_eq!(
            fixed(
                "Prepared by Smith J. for Northbridge Legal Ltd.",
                &[
                    ("Smith J.", Category::Organization),
                    ("Northbridge Legal Ltd.", Category::Organization),
                ]
            ),
            ["Smith J.", "Northbridge Legal Ltd"]
        );
    }

    /// A surname takes in two initials beside it (#391); a sentence's
    /// period, a comma, a line or one initial alone keeps them apart.
    #[test]
    fn initials_beside_a_surname_join_it() {
        for (source, model, expected) in [
            ("Ответчик Ветлин Н. О. возражал", "Ветлин", "Ветлин Н. О."),
            (
                "Заявитель: Ветлин Н. О.. Объяснения",
                "Ветлин",
                "Ветлин Н. О.",
            ),
            ("Подписал Ветлин Н.О., истец", "Ветлин", "Ветлин Н.О."),
            ("Нотариус Л. И. Брюн удостоверила", "Брюн", "Л. И. Брюн"),
            ("Подпись: О’Шеннон А. П.", "О’Шеннон", "О’Шеннон А. П."),
            ("Истец Ли-Вельт Л. А. пришла", "Ли-Вельт", "Ли-Вельт Л. А."),
            ("Signed by J. R. Quill on Monday", "Quill", "J. R. Quill"),
            (
                "Signed by J.R. O’Brien-Hale",
                "O’Brien-Hale",
                "J.R. O’Brien-Hale",
            ),
            // A sentence's period, a comma and a line end the name.
            ("Ветлин. Н. О. Иванов", "Ветлин", "Ветлин"),
            ("Ветлин, Н. О. Иванов", "Ветлин", "Ветлин"),
            ("Ветлин\nН. О. Иванов", "Ветлин", "Ветлин"),
            // One initial is not enough to join.
            ("Ветлин Н. пришёл", "Ветлин", "Ветлин"),
            ("under Part A. Smith said", "Smith", "Smith"),
            // A judge's "J" after an English surname is a title.
            ("Smith J. R. held", "Smith", "Smith"),
            // A role word before the initials does not take them.
            ("Ответчик Н. О. Ветлин", "Ветлин", "Н. О. Ветлин"),
        ] {
            assert_eq!(
                fixed(source, &[(model, Category::Person)]),
                [expected],
                "{source}"
            );
        }
        // Two people side by side keep their own spans.
        assert_eq!(
            fixed(
                "Ветлин Н. О. Брюн Л. И.",
                &[("Ветлин", Category::Person), ("Брюн", Category::Person)]
            ),
            ["Ветлин Н. О.", "Брюн Л. И."]
        );
        // Initials the model gave apart join the surname after them, not a
        // role word before them.
        assert_eq!(
            fixed(
                "Ответчик Н. О. Ветлин",
                &[("Н. О.", Category::Person), ("Ветлин", Category::Person)]
            ),
            ["Н. О. Ветлин", "Н. О. Ветлин"]
        );
        assert_eq!(
            fixed("Истец Н. О.", &[("Н. О.", Category::Person)]),
            ["Н. О."]
        );
        // After the initials a role word is a surname.
        assert_eq!(
            fixed("Signed by J. R. Clerk.", &[("J. R.", Category::Person)]),
            ["J. R. Clerk"]
        );
        // An organization is not a person.
        assert_eq!(
            fixed("ИП Ветлин Н. О.", &[("ИП Ветлин", Category::Organization)]),
            ["ИП Ветлин"]
        );
    }

    /// Roles, role phrases, category words, defined terms and public bodies
    /// are never names (#378, #379, #385, #390); names beside them still are.
    #[test]
    fn roles_category_words_and_public_bodies_are_never_names() {
        let pack = pack();
        for (text, category) in [
            ("Owner", Category::Person),
            ("Assignee", Category::Person),
            ("Technical Reviewer", Category::Person),
            ("Эксперту", Category::Person),
            ("Секретарь", Category::Person),
            ("Costs Officer", Category::Organization),
            ("Administrative Team", Category::Organization),
            ("Provider", Category::Organization),
            ("Общество", Category::Organization),
            ("Email", Category::Organization),
            ("Bank", Category::Organization),
            ("СНИЛС", Category::Organization),
            ("ГК РФ", Category::Organization),
            ("Address", Category::Address),
            ("County Court", Category::Organization),
            ("Ministry of Justice", Category::Organization),
            ("Employment Tribunal", Category::Organization),
            ("Ofcom", Category::Organization),
            ("Верховный Суд Российской Федерации", Category::Organization),
            ("Министерства юстиции", Category::Organization),
            ("Addresses", Category::Address),
            ("Technical Secretaries", Category::Person),
            ("Ministries of Justice", Category::Organization),
            ("Managing Director", Category::Person),
            ("Независимый эксперт", Category::Person),
        ] {
            assert!(!pack.identifying(text, category), "{text}");
        }
        for (text, category) in [
            ("Peter Lord", Category::Person),
            ("Иван Суднев", Category::Person),
            ("Barclays Bank", Category::Organization),
            ("Acme Company", Category::Organization),
            (
                "Общество с ограниченной ответственностью «Ромашка»",
                Category::Organization,
            ),
            ("14 Fennel Court, Lincoln", Category::Address),
            ("Garden Court Ltd", Category::Organization),
            ("Northbridge Department Store Ltd", Category::Organization),
            ("Cedar Arts Council", Category::Organization),
            ("ООО «Судак»", Category::Organization),
            ("John Clerk", Category::Person),
            ("Суднев", Category::Person),
            ("Банков", Category::Person),
            ("Компаниец", Category::Person),
            ("Подрядчиков", Category::Person),
            ("Супругов", Category::Person),
            ("Директоров", Category::Person),
            ("Истцова", Category::Person),
            ("Banks", Category::Person),
            ("Адресат", Category::Organization),
        ] {
            assert!(pack.identifying(text, category), "{text}");
        }
    }

    /// Date masks (#487), common abbreviations (#481), the Internet,
    /// software and site domains (#488) are no names; companies that share
    /// their words keep them.
    #[test]
    fn masks_abbreviations_the_internet_and_products_are_no_names() {
        let pack = pack();
        for (text, category) in [
            ("ДД.ММ.ГГГГ", Category::Organization),
            ("ДД.ММ.ГГГГ", Category::Person),
            ("ДД.ММ.ГГГГ", Category::Date),
            ("ДД.ММ.ГГ", Category::Organization),
            ("ХХ.ХХ.ХХ", Category::Person),
            ("XX.XX.XXXX", Category::Organization),
            ("дд/мм/гггг", Category::Organization),
            ("«ДД.ММ.ГГГГ»", Category::Organization),
            ("ДД.ММ.ГГГГ г.", Category::Organization),
            ("ХХ.ХХ.ХХ №№", Category::Organization),
            ("ДД.ММ.ГГГГ №-осн", Category::Organization),
            ("ДД.ММ.ГГГГ уроженца", Category::Person),
            ("ТКО", Category::Organization),
            ("ТН", Category::Organization),
            ("ТТН", Category::Organization),
            ("АЗС", Category::Organization),
            ("СОУЭ", Category::Organization),
            ("АПС", Category::Organization),
            ("МЧД", Category::Organization),
            ("КЭП", Category::Organization),
            ("ЭП", Category::Organization),
            ("ЕИО", Category::Organization),
            ("ЭДО", Category::Organization),
            ("МЧД", Category::Person),
            ("Интернет", Category::Organization),
            ("«Интернет»", Category::Organization),
            ("Интернета", Category::Organization),
            ("Интернете", Category::Organization),
            ("сети «Интернет»", Category::Organization),
            (
                "информационно-телекоммуникационной сети «Интернет»",
                Category::Organization,
            ),
            (
                "информационно-телекоммуникационная сеть Интернет",
                Category::Organization,
            ),
            ("КонсультантПлюс", Category::Organization),
            ("{КонсультантПлюс}", Category::Organization),
            ("Ватсап", Category::Organization),
            ("Ватсапе", Category::Organization),
            ("WhatsApp", Category::Organization),
            ("Телеграм", Category::Organization),
            ("ПО «Контур»", Category::Organization),
            ("АСУ «Экспресс»", Category::Organization),
            ("программы «Сетка»", Category::Organization),
            ("NALOG.GOV.RU", Category::Organization),
            ("M4D.NALOG.GOV.RU", Category::Organization),
            ("example.invalid", Category::Organization),
            ("https://www.sait-primer.ru/", Category::Organization),
            ("сайт.рф", Category::Organization),
            ("«EXAMPLE.GOV.RU»", Category::Organization),
        ] {
            assert!(!pack.identifying(text, category), "{text}");
        }
        for (text, category) in [
            ("ООО «Контур»", Category::Organization),
            ("АО «ТТН-Логистик»", Category::Organization),
            ("ООО «АЗС»", Category::Organization),
            ("АЗС ООО «Бета-Ойл»", Category::Organization),
            ("ООО «КонсультантПлюс»", Category::Organization),
            ("ООО «Интернет Решения»", Category::Organization),
            ("Интернет Решения", Category::Organization),
            ("Сети «Ромашка»", Category::Organization),
            ("Сбербанк Онлайн", Category::Organization),
            ("Яндекс.Такси", Category::Organization),
            ("E.ON", Category::Organization),
            ("Alfa.Bank", Category::Organization),
            ("J.P.Morgan", Category::Organization),
            ("М.Г.", Category::Person),
            ("Х. М. Гусейнов", Category::Person),
            ("Дмитрий Дымов", Category::Person),
            ("12.03.2026", Category::Date),
            ("Тнеева", Category::Person),
        ] {
            assert!(pack.identifying(text, category), "{text}");
        }
    }

    /// In context: a product's name in «» after the word for it is no
    /// company, inside the quotes or with them (#488); a company named
    /// like an abbreviation or product keeps its name (#481).
    #[test]
    fn a_product_in_guillemets_is_no_company_and_a_company_stays() {
        let corrected = |source: &str, spans: &[&str]| -> Vec<String> {
            let mut detections: Vec<Detection> = spans
                .iter()
                .filter_map(|text| {
                    let start = source.find(text).unwrap();
                    let range = start..start + text.len();
                    pack()
                        .identifying_at(source, range.clone(), text, Category::Organization)
                        .then_some(Detection {
                            range,
                            category: Category::Organization,
                            score: 0.9,
                            recognizer: Recognizer::Model,
                        })
                })
                .collect();
            pack().fix_spans(source, &mdoc_pii::view::View::new(source), &mut detections);
            detections
                .into_iter()
                .map(|d| source[d.range].to_owned())
                .collect()
        };
        let none: [String; 0] = [];
        for (source, span) in [
            ("расчёт выполнен посредством ПО «Сетка» и", "Сетка"),
            ("расчёт выполнен посредством ПО «Сетка» и", "«Сетка»"),
            ("билеты оформлены в АСУ «Экспресс» и", "Экспресс"),
            ("в программе «Сетка» за", "Сетка"),
            ("в группах мессенджера «Ватсап» и", "Ватсап"),
            ("данные системы «Гарант» и", "«Гарант»"),
            ("размещено в сети «Интернет» на", "Интернет"),
            ("расчёт выполнен в ПО \"Сетка\" и", "Сетка"),
            ("расчёт выполнен в ПО \"Сетка\" и", "ПО \"Сетка\""),
            ("Система «Сетка» считает", "Сетка"),
        ] {
            assert_eq!(corrected(source, &[span]), none, "{source}: {span}");
        }
        for (source, span, whole) in [
            ("договор с ООО «Контур» от", "Контур", "ООО «Контур»"),
            (
                "на АЗС ООО «Бета-Ойл» в",
                "АЗС ООО «Бета-Ойл»",
                "ООО «Бета-Ойл»",
            ),
            ("товар передан ООО «АЗС» в", "АЗС", "ООО «АЗС»"),
            ("оператор АНО «ЭДО» в", "ЭДО", "АНО «ЭДО»"),
            // A trim never leaves a legal form alone.
            ("перевозчик АО ТТН в", "АО ТТН", "АО ТТН"),
            ("carrier ТТН LLC in", "ТТН LLC", "ТТН LLC"),
            (
                "перевозчик АО «ТТН-Логистик» в",
                "ТТН-Логистик",
                "АО «ТТН-Логистик»",
            ),
            (
                "лицензия ООО «КонсультантПлюс» на",
                "КонсультантПлюс",
                "ООО «КонсультантПлюс»",
            ),
            // "по" is a preposition, not ПО.
            ("по «Сетке» и", "Сетке", "Сетке"),
            // An app's name waits for the guide (#484).
            (
                "в приложении «Сбербанк Онлайн» и",
                "Сбербанк Онлайн",
                "Сбербанк Онлайн",
            ),
        ] {
            assert_eq!(corrected(source, &[span]), [whole], "{source}: {span}");
        }
        // A person's name in its own quotes after a work's word is a title.
        assert!(!pack().identifying_at(
            "рукопись повести «Кедровый свет».",
            32..61,
            "«Кедровый свет»",
            Category::Person
        ));
    }

    /// One pack holds both kinds of `span` entry: a pattern proposes whole
    /// spans, an entry without one fixes model spans' boundaries, and
    /// neither does the other's work.
    #[test]
    fn span_patterns_and_span_fixes_load_from_one_pack() {
        let pack = Pack::load(&[(
            "test.toml",
            "[[span]]\ncategory = \"ADDRESS\"\npattern = '{street}, {town}'\n\
             parts.street = '\\d+ \\p{Uppercase}\\p{Alphabetic}+ Road'\n\
             parts.town = '\\p{Uppercase}\\p{Alphabetic}+'\n\
             [[span]]\ncategory = \"ORG\"\nstrip_end = \".\"\n\
             [[span]]\ncategory = \"PERSON\"\ntrim = [\"ms\"]\n",
        )])
        .unwrap();
        let source = "Ms Bennett of Acme Ltd. lives at 41 Larch Road, Bristol.";
        let proposed: Vec<_> = pack
            .spans(source)
            .into_iter()
            .map(|d| (source[d.range].to_owned(), d.category, d.recognizer))
            .collect();
        assert_eq!(
            proposed,
            [(
                "41 Larch Road, Bristol".to_owned(),
                Category::Address,
                Recognizer::Span
            )]
        );
        let mut detections: Vec<Detection> = [
            ("Ms Bennett", Category::Person),
            ("Acme Ltd.", Category::Organization),
            ("Larch Road, Bristol.", Category::Address),
        ]
        .into_iter()
        .map(|(text, category)| {
            let start = source.find(text).unwrap();
            Detection {
                range: start..start + text.len(),
                category,
                score: 0.9,
                recognizer: Recognizer::Model,
            }
        })
        .collect();
        pack.fix_spans(source, &mdoc_pii::view::View::new(source), &mut detections);
        let fixed: Vec<_> = detections
            .iter()
            .map(|d| &source[d.range.clone()])
            .collect();
        assert_eq!(fixed, ["Bennett", "Acme Ltd", "Larch Road, Bristol."]);
    }

    /// Addresses are found whole, from the first element to the postcode or
    /// city (#361, #362); a city alone or a numbered court is no address
    /// (#386).
    #[test]
    fn addresses_are_found_whole_and_cities_alone_are_not() {
        let pack = pack();
        let found = |source: &str| -> Vec<String> {
            pack.spans(source)
                .into_iter()
                .filter(|d| d.category == Category::Address)
                .map(|d| source[d.range].to_owned())
                .collect()
        };
        for (source, address) in [
            (
                "of Flat 3, 41 Larch Road, Bristol BS6 9ZY (the Employee).",
                "Flat 3, 41 Larch Road, Bristol BS6 9ZY",
            ),
            (
                "at 42 Cedar Lane, Boston, MA 02110.",
                "42 Cedar Lane, Boston, MA 02110",
            ),
            (
                "is at 18 Brindle Mews, Leeds. Access",
                "18 Brindle Mews, Leeds",
            ),
            (
                "| 27 Wisp Lane<br>Norwich NR2 8ZZ |",
                "27 Wisp Lane<br>Norwich NR2 8ZZ",
            ),
            ("Address: 42 Cedar Lane, B0ston", "42 Cedar Lane, B0ston"),
            (
                "<span>Unit 6, 92 Fable Street, Exeter</span>",
                "Unit 6, 92 Fable Street, Exeter",
            ),
            (
                "адрес: 191187, Санкт-Петербург, Литейный проспект, д. 7, офис 12 (Работодатель)",
                "191187, Санкт-Петербург, Литейный проспект, д. 7, офис 12",
            ),
            (
                "Адрес: г. Москва, ул. Кедровая, д. 42, кв. 7.",
                "г. Москва, ул. Кедровая, д. 42, кв. 7",
            ),
            (
                "Адрес получателя: ул. Тихих фонарей, д. 83, кв. 16. Электронная",
                "ул. Тихих фонарей, д. 83, кв. 16",
            ),
            (
                "по адресу ул. Переплётных огней, д. 74, помещение 6, а",
                "ул. Переплётных огней, д. 74, помещение 6",
            ),
            (
                "по адресу: Тверская область, г. Ржев, ул. Тихих арок, д. 47, корп. 2, стр. 1, лит. Б, пом. 9. Ключи",
                "Тверская область, г. Ржев, ул. Тихих арок, д. 47, корп. 2, стр. 1, лит. Б, пом. 9",
            ),
            (
                "Адрес: г. М0сква, ул. Кедровая, д. 42",
                "г. М0сква, ул. Кедровая, д. 42",
            ),
            // A flat after the street is no town; an index goes with a
            // prefixed city; a house keeps a hyphenated letter.
            (
                "Mail to 123 Main Street, Apt 4, Boston, MA 02110 by Friday.",
                "123 Main Street, Apt 4, Boston, MA 02110",
            ),
            ("at 12 Oak Road, Flat 2. Keys", "12 Oak Road, Flat 2"),
            (
                "at 12 Oak Road, St. Albans AL1 1AA today",
                "12 Oak Road, St. Albans AL1 1AA",
            ),
            (
                "at 12 Oak Road, Newcastle upon Tyne NE1 4AA today",
                "12 Oak Road, Newcastle upon Tyne NE1 4AA",
            ),
            ("at 12 Oak Road, Leeds on Monday", "12 Oak Road, Leeds"),
            ("at 12 Oak Road, M1 1AA today", "12 Oak Road, M1 1AA"),
            (
                "at 12 Oak Road, Little Snoring, Norfolk NR21 0AA today",
                "12 Oak Road, Little Snoring, Norfolk NR21 0AA",
            ),
            (
                "at 12 Oak Road, Leeds, Yorkshire today",
                "12 Oak Road, Leeds",
            ),
            (
                "Адрес: г. Москва, ул. Кедровая, д. 42-44, кв. 7.",
                "г. Москва, ул. Кедровая, д. 42-44, кв. 7",
            ),
            (
                "at 12 Oak Road, Salt Lake City, UT 84101 today",
                "12 Oak Road, Salt Lake City, UT 84101",
            ),
            (
                "at 12 Oak Road, Bury St Edmunds IP33 1AA today",
                "12 Oak Road, Bury St Edmunds IP33 1AA",
            ),
            (
                "at 8 Sunset Blvd, Los Angeles today",
                "8 Sunset Blvd, Los Angeles",
            ),
            (
                "Flat 3, 41 Larch Rd., Bristol BS6 9ZY.",
                "Flat 3, 41 Larch Rd., Bristol BS6 9ZY",
            ),
            ("at 7 Elm St., Leeds today", "7 Elm St., Leeds"),
            ("at 9 Cedar Ave. LS1 4AB today", "9 Cedar Ave. LS1 4AB"),
            ("at 12 Oak Road, W1A 1AA today", "12 Oak Road, W1A 1AA"),
            (
                "Flat 2, 12 Oak Road, M1 1AA today",
                "Flat 2, 12 Oak Road, M1 1AA",
            ),
            (
                "Адрес: ул. Кедровая, д. 42, корп. 1-А, кв. 7Б.",
                "ул. Кедровая, д. 42, корп. 1-А, кв. 7Б",
            ),
            (
                "Адрес: 197101, г. Москва, ул. Кедровая, д. 42, кв. 7.",
                "197101, г. Москва, ул. Кедровая, д. 42, кв. 7",
            ),
            (
                "Адрес: г. Москва, ул. Кедровая, д. 42-А, кв. 7.",
                "г. Москва, ул. Кедровая, д. 42-А, кв. 7",
            ),
            (
                "Адрес: ул. Кедровая, д. 42/1Б, кв. 7.",
                "ул. Кедровая, д. 42/1Б, кв. 7",
            ),
            // A city without "г." leads where its field starts (#362).
            (
                "Адрес для оригиналов: Псков, ул. Тихих залов, д. 29, пом. 8. Почта",
                "Псков, ул. Тихих залов, д. 29, пом. 8",
            ),
            (
                "| Псков, ул. Тихих залов, д. 29 |",
                "Псков, ул. Тихих залов, д. 29",
            ),
            (
                "Псковская область, Псков, ул. Тихих залов, д. 29",
                "Псковская область, Псков, ул. Тихих залов, д. 29",
            ),
            (
                "по адресу Ростов-на-Дону, пр. Тихий, д. 3, кв. 1, а",
                "Ростов-на-Дону, пр. Тихий, д. 3, кв. 1",
            ),
            (
                "Адрес: переулок Бумажных лодок, д. 32, кв. 9.",
                "переулок Бумажных лодок, д. 32, кв. 9",
            ),
            // Mid-name, a word is no city.
            ("Иван Иванович, ул. Ленина, д. 5", "ул. Ленина, д. 5"),
            // Of a list of cities, only the one before the street joins.
            ("Москва, Тверь, ул. Ленина, д. 5", "Тверь, ул. Ленина, д. 5"),
            // In reverse: the flat first and the city last, a city without
            // "г." where its field ends (#362).
            (
                "| кв. 9, д. 32, переулок Бумажных лодок, Тверь |",
                "кв. 9, д. 32, переулок Бумажных лодок, Тверь",
            ),
            (
                "| офис 6, д. 17, улица Лунных папок, Псков |",
                "офис 6, д. 17, улица Лунных папок, Псков",
            ),
            (
                "вернуть на д. 17, улица Лунных папок, г. Псков, до",
                "д. 17, улица Лунных папок, г. Псков",
            ),
            (
                "пом. 2, корп. 1, д. 4, ул. Кедровая. Ключи",
                "пом. 2, корп. 1, д. 4, ул. Кедровая",
            ),
            (
                "кв. 9, д. 32, ул. Кедровая, Иван Петров примет",
                "кв. 9, д. 32, ул. Кедровая",
            ),
        ] {
            assert_eq!(found(source), [address], "{source}");
        }
        // A reverse address ends at its index; one address's end can lead
        // the next; a city of several words takes its "г.".
        for (source, addresses) in [
            (
                "| кв. 9, д. 32, ул. Ленина, Тверь, 170000 |",
                &["кв. 9, д. 32, ул. Ленина, Тверь, 170000"][..],
            ),
            (
                "кв. 9, д. 5, ул. Ленина, г. Нижний Новгород, 603000.",
                &["кв. 9, д. 5, ул. Ленина, г. Нижний Новгород, 603000"],
            ),
            (
                "д. 1, ул. Ленина, Псков; Тверь, ул. Мира, д. 2",
                &["д. 1, ул. Ленина, Псков", "Тверь, ул. Мира, д. 2"],
            ),
            (
                "Адрес: г. Нижний Новгород, ул. Ленина, д. 5.",
                &["г. Нижний Новгород, ул. Ленина, д. 5"],
            ),
            ("Нижний Новгород, ул. Ленина, д. 5", &["ул. Ленина, д. 5"]),
        ] {
            assert_eq!(found(source), addresses, "{source}");
        }
        // A town ends with its line: the name below is no part of it.
        assert_eq!(
            found("12 Oak Road, Bristol\nJohn Smith will collect it."),
            ["12 Oak Road, Bristol"]
        );
        assert_eq!(
            found("ул. Кедровая, д. 42\nкв. 7 сдана"),
            ["ул. Кедровая, д. 42"]
        );
        for source in [
            "Claim 12 High Court was listed.",
            "under section 4.2 of the Road Traffic Act 1988",
            "Иванов, ул. Ленина",
            "the hearing in Cardiff on 3 May",
            // Cities alone, a list of them, "д." of a case or a debt, an
            // area in square metres: no street, no address.
            "Москва, Тверь, Псков",
            "Доставка: Москва, Тверь, Псков.",
            "Псков, д. 29, пом. 8",
            "кв. 9, д. 32, Тверь",
            "по материалам д. 12, ст. 5 закона",
            "остаток д. 120, Петров",
            "площадь 52 кв. м, д. 3",
        ] {
            assert!(found(source).is_empty(), "{source}");
        }
        for city in ["Cardiff", "Москва", "Казань"] {
            assert!(!pack.identifying(city, Category::Address), "{city}");
        }
        for street in [
            "ул. Ленина",
            "пер. Кленовый",
            "наб. Лесная",
            "б-р Цветной",
            "Cedar Court",
            "Larch Rd",
            "Sunset Boulevard",
            "Mulholland Dr",
            "Cherry Ln",
            "Сиреневая аллея, Москва",
            "9-я линия В.О.",
            "The Old Rectory, Nether Compton",
            "The Old Rectory\nNether Compton",
        ] {
            assert!(pack.identifying(street, Category::Address), "{street}");
        }
    }

    /// Rules win the review's conflicts, but a span pattern's match gives
    /// way wherever it is not the widest evidence, so it never cuts what the
    /// model or another rule found; a model address inside it grows to it.
    #[test]
    fn a_model_address_is_not_cut_by_a_shorter_pattern_match() {
        let proposed = |source: &str, model: &[(&str, &str)]| -> Vec<(String, String)> {
            let raw: Vec<_> = model
                .iter()
                .map(|(text, label)| {
                    let start = source.find(text).unwrap();
                    crate::RawEntity {
                        start,
                        end: start + text.len(),
                        label: (*label).into(),
                        score: 0.9,
                        text: (*text).into(),
                    }
                })
                .collect();
            crate::recognition::proposals(source, &raw)
                .unwrap()
                .into_iter()
                .map(|span| (source[span.range].to_owned(), span.category))
                .collect()
        };
        let shown = |source: &str, model: &str| -> Vec<String> {
            proposed(source, &[(model, "address")])
                .into_iter()
                .filter(|(_, category)| category == "ADDRESS")
                .map(|(text, _)| text)
                .collect()
        };
        // A person the model found after the street is no town: the match
        // took a name, so it gives way and the person stands.
        assert_eq!(
            proposed(
                "Deliver to 12 Oak Road, John Smith will collect it.",
                &[("John Smith", "person")]
            ),
            [("John Smith".to_owned(), "PERSON".to_owned())]
        );
        // Nor is a match cut down to its house number by a span over its
        // street, which would propose the number and its repeats.
        assert_eq!(
            proposed(
                "Ship to 12 Oak Road, Leeds. Clause 12 remains applicable.",
                &[("Oak Road, Leeds", "person")]
            ),
            [("Oak Road, Leeds".to_owned(), "PERSON".to_owned())]
        );
        // Unless the model's own address holds that name: it stays whole.
        assert_eq!(
            proposed(
                "Ship to 12 Oak Road, Bath today.",
                &[("12 Oak Road, Bath", "address"), ("Bath", "person")]
            ),
            [("12 Oak Road, Bath".to_owned(), "ADDRESS".to_owned())]
        );
        // A valid email inside the model's address still stands, at its
        // end or within it.
        let source = "12 Oak Road, Bristol BS6 9ZY. Email: jane@example.invalid";
        assert!(
            proposed(source, &[(source, "address")])
                .contains(&("jane@example.invalid".to_owned(), "EMAIL".to_owned()))
        );
        let source = "12 Oak Road, Bristol BS6 9ZY. Email: jane@example.invalid. Contact us.";
        assert!(
            proposed(source, &[(source, "address")])
                .contains(&("jane@example.invalid".to_owned(), "EMAIL".to_owned()))
        );
        // A field's label after the street is no town: the model's address
        // stands.
        assert_eq!(
            proposed(
                "Address: 12 Oak Road, Email: jane@example.invalid",
                &[("12 Oak Road", "address")]
            ),
            [
                ("12 Oak Road".to_owned(), "ADDRESS".to_owned()),
                ("jane@example.invalid".to_owned(), "EMAIL".to_owned()),
            ]
        );
        // So does an email another rule found.
        assert_eq!(
            proposed(
                "Deliver to 12 Oak Road, Jane.Doe@example.invalid today.",
                &[]
            ),
            [("Jane.Doe@example.invalid".to_owned(), "EMAIL".to_owned())]
        );
        // And a name the model split: both its parts stand.
        assert_eq!(
            proposed(
                "Deliver to 12 Oak Road, John Smith will collect it. John will call.",
                &[("John", "person"), ("Smith", "person")]
            )
            .into_iter()
            .filter(|(_, category)| category == "ADDRESS")
            .count(),
            0
        );
        // A model name inside the address is the street's: the address stays.
        assert_eq!(
            proposed(
                "Адрес: г. Москва, ул. Ленина, д. 5, кв. 2. Почта",
                &[("Ленина", "person")]
            ),
            [(
                "г. Москва, ул. Ленина, д. 5, кв. 2".to_owned(),
                "ADDRESS".to_owned()
            )]
        );
        let source = "Адрес: Псков, ул. Тихих залов, д. 29, пом. 8. Почта";
        let whole = "Псков, ул. Тихих залов, д. 29, пом. 8";
        assert_eq!(shown(source, whole), [whole]);
        let source = "Deliver to 12 Oak Road, Little Snoring, Norfolk NR21 0AA today.";
        let whole = "12 Oak Road, Little Snoring, Norfolk NR21 0AA";
        assert_eq!(shown(source, whole), [whole]);
        // The match starts before the model's span and ends inside it: the
        // model's address, which holds the county, stands.
        let source = "at Flat 3, 12 Oak Road, Little Snoring, Norfolk today.";
        assert_eq!(
            shown(source, "12 Oak Road, Little Snoring, Norfolk"),
            ["12 Oak Road, Little Snoring, Norfolk"]
        );
        // A match that holds the model's span grows it to the whole address.
        let source = "at Flat 3, 12 Oak Road, Little Snoring, Norfolk NR21 0AA today.";
        assert_eq!(
            shown(source, "12 Oak Road, Little Snoring, Norfolk NR21 0AA"),
            ["Flat 3, 12 Oak Road, Little Snoring, Norfolk NR21 0AA"]
        );
        // An overlong model address does not lend the match its reach over a
        // person: the two compete as without the rule, by the model's scores.
        let source = "12 Oak Road, Leeds. Contact John Smith today.";
        let raw = [
            crate::RawEntity {
                start: 0,
                end: source.len(),
                label: "address".into(),
                score: 0.6,
                text: source.into(),
            },
            crate::RawEntity {
                start: source.find("John").unwrap(),
                end: source.find(" today").unwrap(),
                label: "person".into(),
                score: 0.95,
                text: "John Smith".into(),
            },
        ];
        let found: Vec<_> = crate::recognition::proposals(source, &raw)
            .unwrap()
            .into_iter()
            .map(|span| (source[span.range].to_owned(), span.category))
            .collect();
        assert_eq!(found, [("John Smith".to_owned(), "PERSON".to_owned())]);
        let source = "of Flat 3, 41 Larch Road, Bristol BS6 9ZY (the Employee).";
        assert_eq!(
            shown(source, "Larch Road, Bristol"),
            ["Flat 3, 41 Larch Road, Bristol BS6 9ZY"]
        );
    }

    /// A date in a personal context is proposed as Date, the label left out
    /// (#363); every other date stays (ADR 0092).
    #[test]
    fn dates_of_birth_are_proposed_and_other_dates_stay() {
        let dates = |source: &str| -> Vec<String> {
            pack()
                .context(source)
                .into_iter()
                .filter(|d| d.category == Category::Date)
                .map(|d| source[d.range].to_owned())
                .collect()
        };
        for (source, date) in [
            (
                "Ms Maya Bennett, born 14 March 1988, National",
                "14 March 1988",
            ),
            ("born on 2 May 1990.", "2 May 1990"),
            ("**DOB:** 23 September 1991. That", "23 September 1991"),
            ("Date of birth: March 4, 1979;", "March 4, 1979"),
            ("дата рождения 14 марта 1988 г., СНИЛС", "14 марта 1988 г."),
            (
                "Дата рождения: 19 февраля 1987 года. В",
                "19 февраля 1987 года",
            ),
            ("Иванов И. И., род. 14.03.1988, паспорт", "14.03.1988"),
            ("д.р. 01.02.1990", "01.02.1990"),
            ("| DOB | 14/03/1988 |", "14/03/1988"),
            ("| Дата рождения | 14.03.1988 г. |", "14.03.1988 г."),
            ("DOB:\n14/03/1988", "14/03/1988"),
            (
                "Дата рождения: «14» марта 1988 г., паспорт",
                "«14» марта 1988 г.",
            ),
            ("| DOB | 14/03/1988 |", "14/03/1988"),
            ("| Дата рождения | 14.03.1988 г. |", "14.03.1988 г."),
            (
                "DOB:
14/03/1988",
                "14/03/1988",
            ),
            (
                "Дата рождения: «14» марта 1988 г., паспорт",
                "«14» марта 1988 г.",
            ),
        ] {
            assert_eq!(dates(source), [date], "{source}");
        }
        for source in [
            "Signed on 14 March 2026 in Bristol.",
            "The hearing is listed for 12.03.2026.",
            "She was born in Leeds in 1988.",
            "born 14 Marchmont 1988",
            "Род деятельности: 2026 год",
            "Adobe 14 March 1988",
        ] {
            assert!(dates(source).is_empty(), "{source}");
        }
    }

    /// A birth date's repeat elsewhere is another date and stays, like any
    /// other date (#466 option 1, GUIDE rule 4); a labelled repeat counts.
    #[test]
    fn a_birth_dates_repeat_elsewhere_stays_unmarked() {
        let source = "DOB: 14/03/1988. Signed 14/03/1988, due 01/04/2026. DOB 14/03/1988.";
        let dates: Vec<_> = crate::recognition::proposals(source, &[])
            .unwrap()
            .into_iter()
            .filter(|span| span.category == "DATE")
            .map(|span| source[span.range].to_owned())
            .collect();
        assert_eq!(dates, ["14/03/1988", "14/03/1988"]);
    }

    /// What the user is shown for the model's `raw` spans (text and label,
    /// each found at its first place): the correction stage and the review.
    fn shown(source: &str, raw: &[(&str, &str)]) -> Vec<(String, String)> {
        let raw: Vec<crate::RawEntity> = raw
            .iter()
            .map(|(text, label)| {
                let start = source
                    .find(text)
                    .unwrap_or_else(|| panic!("{text:?} not in the source"));
                crate::RawEntity {
                    start,
                    end: start + text.len(),
                    label: (*label).into(),
                    score: 0.8,
                    text: (*text).into(),
                }
            })
            .collect();
        crate::recognition::proposals(source, &raw)
            .unwrap()
            .into_iter()
            .map(|span| (source[span.range].to_owned(), span.category))
            .collect()
    }

    fn pairs(expected: &[(&str, &str)]) -> Vec<(String, String)> {
        expected
            .iter()
            .map(|(text, category)| ((*text).to_owned(), (*category).to_owned()))
            .collect()
    }

    /// US and UK citations are no identifiers, addresses or names: reporter,
    /// Westlaw and law-report citations, codes and their abbreviations,
    /// sections, the record and a form's outline letter (guide rule 8,
    /// issues #499 and #480).
    #[test]
    fn english_legal_references_are_never_proposed() {
        let source = "As held at 912 F.3d 455, 460 and 77 N.E.2d 310, and 424 U.S. at 38 \
            (527 U.S. 706; 33 A.D.3d 352; 904 F.2d 275, 277), under 26 U.S.C. § 6212, \
            31 U.S.C. 3903, I.R.C. § 61, 48 CFR 52.212-1, FAR 52.212-1, 34 CFR §300.8, \
            E.O. 13526, Release No. 34-98765, 2031 WL 1234567, [2031] STC 412 and ICR 801 \
            (Tr. 112-14; Tr. 435-36; ECF Doc. #12-3; 123sa; see n. 14; Id. at 41; \
            ROA.977), for tax year 2029/30, the U.S. District Court ruled.\n\nJ. Additional Questions\n";
        let raw = [
            ("912 F.3d 455, 460 and 77 N.E.2d 310", "address"),
            ("424 U.S. at 38", "address"),
            ("U.S.", "organization"),
            ("A.D.", "person"),
            ("904 F.2d 275, 277", "phone_number"),
            ("6212", "tax_id"),
            ("31 U.S.C. 3903", "bank_account"),
            ("S.C.", "person"),
            ("R.C.", "person"),
            ("48 CFR 52.212-1", "tax_id"),
            ("FAR 52.212-1", "tax_id"),
            ("300.8", "tax_id"),
            ("13526", "tax_id"),
            ("34-98765", "tax_id"),
            ("2031 WL 1234567", "tax_id"),
            ("STC 412", "tax_id"),
            ("ICR 801", "bank_account"),
            ("Tr. 112-14", "tax_id"),
            ("435-36", "tax_id"),
            ("#12-3", "tax_id"),
            ("123sa", "tax_id"),
            ("n. 14", "passport_number"),
            ("Id. at 41", "passport_number"),
            ("ROA.977", "national_id_number"),
            ("2029/30", "tax_id"),
            ("J.", "person"),
        ];
        assert_eq!(shown(source, &raw), []);
        assert_eq!(
            shown("Before the U.S. District Court.", &[("U.S.", "person")]),
            []
        );
        // The same abbreviation as a span alone, whatever its neighbours.
        for text in ["U.S.", "U.K.", " U.S.A. "] {
            assert!(!pack().identifying(text, Category::Organization), "{text}");
            assert!(!pack().identifying(text, Category::Person), "{text}");
        }
    }

    /// A real identifier beside a citation stays: a tax number, a case
    /// number after its label, a phone, a company, a person, a university.
    #[test]
    fn identifiers_beside_a_citation_stay() {
        let source = "Under 26 U.S.C. § 6212 (EIN 98-7654321), Case No. 1:26-cv-01234 \
            (citing 424 U.S. at 38), call +1 (202) 555-0164 (904 F.2d 275, 277). \
            U.S. Bank paid; USC agreed; GB 123 4567 89 is the VAT number.\n\n\
            J. Ostrand\n";
        let raw = [
            ("98-7654321", "tax_id"),
            ("+1 (202) 555-0164", "phone_number"),
            ("U.S. Bank", "organization"),
            ("USC", "organization"),
            ("GB 123 4567 89", "tax_id"),
            ("J. Ostrand", "person"),
        ];
        assert_eq!(
            shown(source, &raw),
            pairs(&[
                ("98-7654321", "TAX"),
                ("1:26-cv-01234", "CASE"),
                ("+1 (202) 555-0164", "PHONE"),
                ("U.S. Bank", "ORG"),
                ("USC", "ORG"),
                ("GB 123 4567 89", "TAX"),
                ("J. Ostrand", "PERSON"),
            ])
        );
        // A tax year is no tax number; a number in another shape stays.
        assert!(!pack().identifying("2014/15", Category::Tax));
        assert!(!pack().identifying("2014-2015", Category::Tax));
        assert!(pack().identifying("98-7654321", Category::Tax));
        assert!(pack().identifying("123-45-6789", Category::Tax));
    }

    /// Russian law, article and court-act numbers are no tax numbers; the
    /// INN beside them stays (guide rule 8, issue #480).
    #[test]
    fn russian_legal_references_are_never_proposed() {
        let source = "на основании Федерального закона от 26.10.2002 № 127-ФЗ, Федерального \
            закона № 999-ФЗ и статьи 431 ГК РФ, ст. 428 ГК РФ, п. 2 ст. 450 ГК РФ, \
            Определения Верховного Суда РФ от 12.03.2031 № 305-ЭС31-1234 и УПД № 412, \
            ИНН 7701234567, тел. +7 (495) 123-45-67";
        let raw = [
            ("127-ФЗ", "tax_id"),
            ("999-ФЗ", "tax_id"),
            ("431 ГК РФ", "tax_id"),
            ("428 ГК РФ", "tax_id"),
            ("450", "tax_id"),
            ("305-ЭС31-1234", "tax_id"),
            ("412", "tax_id"),
            ("7701234567", "tax_id"),
            ("+7 (495) 123-45-67", "phone_number"),
        ];
        assert_eq!(
            shown(source, &raw),
            pairs(&[("7701234567", "TAX"), ("+7 (495) 123-45-67", "PHONE")])
        );
        // A court's own words may sit between the act and its number.
        let source = "Определение судебной коллегии по экономическим спорам Верховного Суда РФ \
            от 12.03.2031 № 305-ЭС31-1234, Приказ Минфина России от 01.02.2031 № 12н";
        assert_eq!(
            shown(source, &[("305-ЭС31-1234", "tax_id"), ("12н", "tax_id")]),
            []
        );
        // A value between an act and a "№" stays, and so does a number
        // after "№" that is no act's.
        let source = "Постановление в отношении ООО «Вега», ИНН 7701234567, № 17";
        assert_eq!(
            shown(source, &[("7701234567", "tax_id")]),
            pairs(&[("7701234567", "TAX")])
        );
        let source = "Постановление направлено владельцу паспорта № 4512345678.";
        assert_eq!(
            shown(source, &[("4512345678", "passport_number")]),
            pairs(&[("4512345678", "IDENTITY")])
        );
    }

    /// A citation is found as detection reads it: through character
    /// references and across a soft line break. A company named like a
    /// code keeps its name; a blank line, a table row or a list item ends
    /// the citation (#480, #499).
    #[test]
    fn citations_wrapped_or_encoded_are_found_and_neighbours_stay() {
        for source in [
            "under 26 U.S.C. &sect; 6212 the notice",
            "under 26 U.S.C.\n6212 the notice",
            "under 26 U.S.C.\r\n6212 the notice",
        ] {
            assert_eq!(shown(source, &[("6212", "tax_id")]), [], "{source:?}");
        }
        assert_eq!(
            shown(
                "see 904 F.2d\n275, 277, and",
                &[("904 F.2d\n275, 277", "phone_number")]
            ),
            []
        );
        assert_eq!(
            shown("ООО «C.F.R.» paid.", &[("C.F.R.", "organization")]),
            pairs(&[("ООО «C.F.R.»", "ORG")])
        );
        for source in [
            "Under 26 U.S.C.\n\n123-45-6789 is the number.",
            "Under 26 U.S.C.\r\n\r\n123-45-6789 is the number.",
            "## Under 26 U.S.C.\n123-45-6789 is the number.",
            "Under 26 U.S.C.\n1. 123-45-6789 is the number.",
        ] {
            assert_eq!(
                shown(source, &[("123-45-6789", "tax_id")]),
                pairs(&[("123-45-6789", "TAX")]),
                "{source:?}"
            );
        }
    }

    /// "& Anor" and "& Ors" come off a party and name no one; a declaration
    /// is its declarant's, a person's (issue #499).
    #[test]
    fn citation_words_and_declarations_are_no_companies() {
        let source = "The claim by Pellow Holdings Ltd & Anor and Vey Ltd & Ors fails. Mr Ivo \
            Ostrand confirms it (Ostrand Decl. ¶ 3; Ruel Decl. ¶ 4).";
        let raw = [
            ("Pellow Holdings Ltd & Anor", "organization"),
            ("Vey Ltd", "organization"),
            ("Ors", "organization"),
            ("Ivo Ostrand", "person"),
            ("Ostrand Decl", "organization"),
            ("Ruel Decl", "organization"),
        ];
        // "Ruel" is no known person: the span stays as the model gave it.
        assert_eq!(
            shown(source, &raw),
            pairs(&[
                ("Pellow Holdings Ltd", "ORG"),
                ("Vey Ltd", "ORG"),
                ("Ivo Ostrand", "PERSON"),
                ("Ostrand", "PERSON"),
                ("Ruel Decl", "ORG"),
            ])
        );
        assert!(!pack().identifying("Anor", Category::Person));
        assert!(!pack().identifying("Ors", Category::Organization));
    }

    #[test]
    fn mistakes_in_a_rule_file_are_named() {
        for (source, problem) in [
            (
                "[[never]]\nmatch = \"plural\"\nwords = [\"Ms\"]",
                "not lowercase",
            ),
            ("[[never]]\nmatch = \"stem\"\nwords = [\"истц\"]", "ending"),
            (
                "[[shape]]\ncategory = \"PERSON\"\nforbid_quotes = [\"«\"]",
                "not an opening and a closing quote",
            ),
            (
                "[[shape]]\ncategory = \"PERSON\"\nquotes_after = [\"повест\"]",
                "needs forbid_quotes",
            ),
            (
                "[[shape]]\ncategory = \"PERSON\"\nforbid_quotes = [\"«»\"]\nquotes_after = [\"Повест\"]",
                "lowercase stem",
            ),
            (
                "[[never]]\nmatch = \"whole\"\nwords = [\"the\", \"the\"]",
                "twice",
            ),
            (
                "[[shape]]\ncategory = \"NAME\"\nrequire = ['x']",
                "unknown category",
            ),
            ("[[shape]]\ncategory = \"DATE\"\nguess = ['(']", "shape #1"),
            ("[[shape]]\ncategory = \"DATE\"", "no rules"),
            (
                "[[shape]]\ncategory = \"PERSON\"\nword = 'a'\n[[shape]]\ncategory = \"PERSON\"\nword = 'b'",
                "second word rule",
            ),
            (
                "[[shape]]\ncategory = \"ORG\"\nguess_words = [\"S.A.\"]",
                "one word",
            ),
            (
                "[[context]]\ncategory = \"DATE\"\nlabels = []\nbetween = ' '\nvalue = '\\d+'",
                "labels",
            ),
            (
                "[[context]]\ncategory = \"CASE\"\nlabels = [\"дело №\"]\nbetween = ' '\nvalue = '\\d+'",
                "start and end",
            ),
            (
                "[[context]]\ncategory = \"CASE\"\nlabels = [\"case\"]\nbetween = ' '\nvalue = '(\\d+)'",
                "capture",
            ),
            ("[[span]]\ncategory = \"PERSON\"", "trim"),
            (
                "[[span]]\ncategory = \"PERSON\"\ntrim = [\"Ms\"]",
                "not lowercase",
            ),
            (
                "[[span]]\ncategory = \"ORG\"\nstrip_end = \"a.\"",
                "punctuation only",
            ),
            ("[[span]]\ncategory = \"ORG\"\nextend = ['(']", "span #1"),
            ("[[span]]\ncategory = \"ADDRESS\"", "pattern"),
            (
                "[[span]]\ncategory = \"ADDRESS\"\npattern = '\\d+'\nparts = { street = 'x' }",
                "never used",
            ),
            (
                "[[span]]\ncategory = \"ADDRESS\"\npattern = ''",
                "empty span",
            ),
            (
                "[[span]]\ncategory = \"ADDRESS\"\npattern = 'x*'",
                "empty span",
            ),
            (
                "[[span]]\ncategory = \"ADDRESS\"\npattern = '(?P<city>\\w+), \\d+'",
                "is not `lead` or `tail`",
            ),
            (
                "[[span]]\ncategory = \"ADDRESS\"\npattern = '\\d+'\ntrim = [\"ms\"]",
                "takes no trim",
            ),
            (
                "[[span]]\ncategory = \"ORG\"\nstrip_end = \".\"\nparts = { street = 'x' }",
                "need a pattern",
            ),
            (
                "[[span]]\ncategory = \"PERSON\"\nperson_before = [\"decl\"]",
                "only an ORG fix",
            ),
            (
                "[[span]]\ncategory = \"ORG\"\nperson_before = [\"Decl\"]",
                "not lowercase",
            ),
            (
                "[[reference]]\npattern = 'x*'",
                "reference #1: the pattern matches an empty span",
            ),
            (
                "[[reference]]\npattern = '\\d+'\nparts = { code = 'x' }",
                "never used",
            ),
            (
                "[[reference]]\npattern = '(?P<code>x)'",
                "is not `lead` or `tail`",
            ),
            ("[[variants]]\nname = \"Liz\"\nforms = [\"Liz\"]", "forms"),
            (
                "[[never]]\nmatch = \"whole\"\nwords = [\"a\"]\ntypo = 1",
                "typo",
            ),
        ] {
            let Err(error) = Pack::load(&[("test.toml", source)]) else {
                panic!("accepted {source:?}");
            };
            assert!(error.contains(problem), "{error}");
            assert!(error.starts_with("test.toml"), "{error}");
        }
    }
}
