//! The rule pack (ADR 0092): `rules/{shared,en,ru}.toml`, compiled in, read
//! and checked once. Its `never` words and `shape`s decide which model spans
//! are kept and what a manual selection is guessed to be.
use mdoc_pii::Category;
use regex::Regex;
use serde::Deserialize;
use std::sync::OnceLock;

/// The pack's files, in the order their rules apply.
const FILES: [(&str, &str); 3] = [
    ("shared.toml", include_str!("../rules/shared.toml")),
    ("en.toml", include_str!("../rules/en.toml")),
    ("ru.toml", include_str!("../rules/ru.toml")),
];

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
    #[serde(default)]
    trim: Vec<String>,
    #[serde(default)]
    extend: Vec<String>,
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
                category(&entry.category).map_err(at)?;
                if entry.labels.is_empty() || entry.labels.iter().any(|l| l.trim().is_empty()) {
                    return Err(at("needs labels, none blank".into()));
                }
            }
            for (index, entry) in file.span.iter().enumerate() {
                let at = |problem: String| format!("{name}: span #{}: {problem}", index + 1);
                category(&entry.category).map_err(at)?;
                if entry.trim.is_empty() && entry.extend.is_empty() {
                    return Err(at("needs trim words or extend patterns".into()));
                }
                if entry.trim.iter().any(|word| word.trim().is_empty()) {
                    return Err(at("a blank trim word".into()));
                }
                for pattern in &entry.extend {
                    Regex::new(pattern).map_err(|e| at(e.to_string()))?;
                }
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
            ("[[context]]\ncategory = \"DATE\"\nlabels = []", "labels"),
            ("[[span]]\ncategory = \"PERSON\"", "trim"),
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
