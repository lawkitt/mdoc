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
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct NeverEntry {
    #[serde(rename = "match")]
    form: Form,
    /// For `stem`: how many letters may follow the stem.
    ending: Option<usize>,
    words: Vec<String>,
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
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct SpanEntry {
    category: String,
    /// What a whole span looks like; `{name}` stands for a part.
    pattern: Option<String>,
    /// Named pieces of the pattern, each a regex.
    #[serde(default)]
    parts: std::collections::BTreeMap<String, String>,
    /// A model span's fixes: text just before it and just after it that it
    /// takes in, both or neither ("ООО «" and "»").
    before: Option<String>,
    after: Option<String>,
    /// What a model span drops at its start or end ("Ms ", a final dot):
    /// the first capture group, or the whole match without one.
    trim_start: Option<String>,
    trim_end: Option<String>,
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
    fixes: Vec<Fix>,
}

/// A whole span by its pattern: an address from its first element to its
/// postcode or city.
struct Span {
    category: Category,
    pattern: Regex,
}

/// A boundary fix for a model span of `category`.
struct Fix {
    category: Category,
    /// Anchored at the span's start and end.
    before: Option<Regex>,
    after: Option<Regex>,
    trim_start: Option<Regex>,
    trim_end: Option<Regex>,
}

/// A label then its value: the value's capture group 1 is proposed.
struct Context {
    category: Category,
    pattern: Regex,
    min_digits: usize,
    unless_after: Vec<String>,
    unless_guessed: Vec<Category>,
    only_guessed: Vec<Category>,
}

struct Never {
    form: Form,
    ending: usize,
    words: Vec<String>,
}

struct Shape {
    category: Category,
    require: Vec<Regex>,
    forbid: Vec<Regex>,
    word: Option<Regex>,
    guess: Vec<Regex>,
    guess_words: Vec<String>,
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
            fixes: Vec::new(),
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
                pack.never.push(Never {
                    form: entry.form,
                    ending,
                    words: entry.words,
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
                {
                    return Err(at("no rules".into()));
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
                    min_digits: entry.min_digits,
                    unless_after: entry.unless_after.clone(),
                    unless_guessed,
                    only_guessed,
                });
            }
            for (index, entry) in file.span.iter().enumerate() {
                let at = |problem: String| format!("{name}: span #{}: {problem}", index + 1);
                let category = category(&entry.category).map_err(at)?;
                let fixes = [
                    &entry.before,
                    &entry.after,
                    &entry.trim_start,
                    &entry.trim_end,
                ];
                let Some(pattern) = entry.pattern.clone() else {
                    if !entry.parts.is_empty() || fixes.iter().all(|f| f.is_none()) {
                        return Err(at("needs a pattern, or fixes without parts".into()));
                    }
                    let anchored = |regex: &Option<String>, start: bool| {
                        regex
                            .as_deref()
                            .map(|regex| {
                                let regex = rust_classes(regex);
                                let regex = if start {
                                    format!("^(?:{regex})")
                                } else {
                                    format!("(?:{regex})$")
                                };
                                Regex::new(&regex).map_err(|e| at(e.to_string()))
                            })
                            .transpose()
                    };
                    let fix = Fix {
                        category,
                        before: anchored(&entry.before, false)?,
                        after: anchored(&entry.after, true)?,
                        trim_start: anchored(&entry.trim_start, true)?,
                        trim_end: anchored(&entry.trim_end, false)?,
                    };
                    if [&fix.trim_start, &fix.trim_end]
                        .iter()
                        .any(|r| r.as_ref().is_some_and(|r| r.captures_len() > 2))
                    {
                        return Err(at("a trim has at most one group".into()));
                    }
                    pack.fixes.push(fix);
                    continue;
                };
                if fixes.iter().any(|f| f.is_some()) {
                    return Err(at("a pattern takes no fixes".into()));
                }
                let mut pattern = pattern;
                for (part, regex) in &entry.parts {
                    let name = format!("{{{part}}}");
                    if !pattern.contains(&name) {
                        return Err(at(format!("part {part:?} is never used")));
                    }
                    pattern = pattern.replace(&name, &format!("(?:{regex})"));
                }
                let pattern = Regex::new(&rust_classes(&pattern)).map_err(|e| at(e.to_string()))?;
                pack.spans.push(Span { category, pattern });
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
        self.never.iter().any(|entry| {
            entry.words.iter().any(|listed| match entry.form {
                Form::Whole => word == *listed,
                Form::Plural => word == *listed || word.strip_suffix('s') == Some(listed),
                Form::Stem => word.strip_prefix(listed.as_str()).is_some_and(|ending| {
                    ending.chars().count() <= entry.ending
                        && ending.chars().all(char::is_alphabetic)
                }),
            })
        })
    }

    /// Whether a model span has its category's `shape`.
    pub(crate) fn identifying(&self, text: &str, category: Category) -> bool {
        let Some(shape) = self.shapes.iter().find(|s| s.category == category) else {
            return true;
        };
        shape.require.iter().all(|pattern| pattern.is_match(text))
            && !shape.forbid.iter().any(|pattern| pattern.is_match(text))
            && shape.word.as_ref().is_none_or(|pattern| {
                text.split_whitespace()
                    .map(|word| word.trim_matches(|c: char| !c.is_alphanumeric() && c != '-'))
                    .filter(|word| !word.is_empty())
                    .any(|word| pattern.is_match(word) && !self.never(word))
            })
    }

    /// Values beside their labels in `source`, as `context` rules propose
    /// them: the value only, without its label.
    pub(crate) fn context(&self, source: &str) -> Vec<Detection> {
        self.contexts
            .iter()
            .flat_map(|context| {
                context.pattern.captures_iter(source).filter_map(|found| {
                    let value = found.get(1)?;
                    let digits = value.as_str().chars().filter(char::is_ascii_digit).count();
                    // The value ends where a word does.
                    let ends = !source[value.end()..]
                        .chars()
                        .next()
                        .is_some_and(char::is_alphanumeric);
                    let guessed = self.guess(value.as_str());
                    let before = source[..found.get(0)?.start()]
                        .trim_end()
                        .rsplit(|c: char| !c.is_alphanumeric())
                        .next()
                        .unwrap_or_default()
                        .to_lowercase();
                    if !ends
                        || digits < context.min_digits
                        || context.unless_after.contains(&before)
                        || guessed.is_some_and(|g| context.unless_guessed.contains(&g))
                        || (!context.only_guessed.is_empty()
                            && !guessed.is_some_and(|g| context.only_guessed.contains(&g)))
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
            .collect()
    }

    /// Whole spans the `span` patterns find in `source`: an address from
    /// its first element to its postcode or city (ADR 0092).
    pub(crate) fn spans(&self, source: &str) -> Vec<Detection> {
        self.spans
            .iter()
            .flat_map(|span| {
                span.pattern.find_iter(source).map(|found| Detection {
                    range: found.range(),
                    category: span.category,
                    score: CONTEXT_SCORE,
                    recognizer: Recognizer::Span,
                })
            })
            .collect()
    }

    /// A model span of `category` with the `span` section's boundary fixes:
    /// a legal form and its quotes taken in, a title or a final dot dropped.
    pub(crate) fn fix(
        &self,
        source: &str,
        mut range: std::ops::Range<usize>,
        category: Category,
    ) -> std::ops::Range<usize> {
        // Text beside a span is read this far, not to the document's ends.
        const NEAR: usize = 128;
        for fix in self.fixes.iter().filter(|f| f.category == category) {
            let window = source.floor_char_boundary(range.start.saturating_sub(NEAR));
            let before = fix.before.as_ref().map(|r| {
                r.find(&source[window..range.start])
                    .map(|m| window + m.start())
            });
            let after_end = source.ceil_char_boundary((range.end + NEAR).min(source.len()));
            let after = fix.after.as_ref().map(|r| {
                r.find(&source[range.end..after_end])
                    .map(|m| range.end + m.end())
            });
            if (before.is_some() || after.is_some())
                && before.is_none_or(|b| b.is_some())
                && after.is_none_or(|a| a.is_some())
            {
                range =
                    before.flatten().unwrap_or(range.start)..after.flatten().unwrap_or(range.end);
            }
            let trimmed = |regex: &Option<Regex>, text: &str| {
                regex.as_ref().and_then(|r| {
                    r.captures(text)
                        .and_then(|c| c.get(1).or_else(|| c.get(0)))
                        .map(|m| m.range())
                })
            };
            if let Some(cut) = trimmed(&fix.trim_start, &source[range.clone()])
                && range.start + cut.end < range.end
            {
                range.start += cut.end;
            }
            if let Some(cut) = trimmed(&fix.trim_end, &source[range.clone()])
                && cut.start > 0
            {
                range.end = range.start + cut.start;
            }
        }
        range
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
                    || ["never", "context", "shape", "span", "variants"]
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
        ] {
            assert!(found(source).is_empty(), "{source}");
        }
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
                "Адрес: г. М0сква, ул. Кедровая, д. 42",
                "г. М0сква, ул. Кедровая, д. 42",
            ),
        ] {
            assert_eq!(found(source), [address], "{source}");
        }
        for source in [
            "Claim 12 High Court was listed.",
            "under section 4.2 of the Road Traffic Act 1988",
            "Иванов, ул. Ленина",
            "the hearing in Cardiff on 3 May",
        ] {
            assert!(found(source).is_empty(), "{source}");
        }
        for city in ["Cardiff", "Москва", "Казань"] {
            assert!(!pack.identifying(city, Category::Address), "{city}");
        }
        assert!(pack.identifying("ул. Ленина", Category::Address));
    }

    /// Model spans get the guide's boundaries (#374, #382, #391): a legal
    /// form with its quotes taken in, the abbreviation dot and a title left
    /// out, initials after a surname taken in.
    #[test]
    fn model_spans_get_the_guides_boundaries() {
        let pack = pack();
        let fixed = |source: &str, model: &str, category: Category| {
            let start = source.find(model).unwrap();
            source[pack.fix(source, start..start + model.len(), category)].to_owned()
        };
        for (source, model, category, expected) in [
            (
                "deposit from Moth Lantern Events Ltd. The booking",
                "Moth Lantern Events Ltd.",
                Category::Organization,
                "Moth Lantern Events Ltd",
            ),
            (
                "Ответчик: ООО «Северный Вектор», ИНН",
                "Северный Вектор",
                Category::Organization,
                "ООО «Северный Вектор»",
            ),
            (
                "адвокат коллегия адвокатов «Елагин и партнёры» (Представитель)",
                "Елагин и партнёры",
                Category::Organization,
                "коллегия адвокатов «Елагин и партнёры»",
            ),
            (
                "журнал «Северный Вектор» сообщил",
                "Северный Вектор",
                Category::Organization,
                "Северный Вектор",
            ),
            (
                "Signed by Ms Bennett today",
                "Ms Bennett",
                Category::Person,
                "Bennett",
            ),
            (
                "Dr Ysolde Qhenn heard it",
                "Dr Ysolde Qhenn",
                Category::Person,
                "Ysolde Qhenn",
            ),
            (
                "Peter Lord signed",
                "Peter Lord",
                Category::Person,
                "Peter Lord",
            ),
            ("Ms", "Ms", Category::Person, "Ms"),
            (
                "Ответчик Ветлин Н. О. возражал",
                "Ветлин",
                Category::Person,
                "Ветлин Н. О.",
            ),
            ("Ветлин. Н. Иванов", "Ветлин", Category::Person, "Ветлин"),
        ] {
            assert_eq!(fixed(source, model, category), expected, "{source}");
        }
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
            ("[[span]]\ncategory = \"ADDRESS\"", "pattern"),
            (
                "[[span]]\ncategory = \"ADDRESS\"\npattern = '\\d+'\nparts = { street = 'x' }",
                "never used",
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
