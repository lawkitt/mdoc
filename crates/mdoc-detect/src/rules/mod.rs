//! The rule pack (ADR 0092): `rules/{shared,en,ru}.toml`, compiled in, read
//! and checked once. Its `never` words and `shape`s decide which model spans
//! are kept and what a manual selection is guessed to be.
//!
//! One module per layer; the checked pack's types are here:
//!
//! - `load`: the TOML entries, and `Pack::load`, which reads and checks them;
//! - `shapes`: `never` words and `shape`s: which model spans are kept, and guesses;
//! - `public`: courts, public bodies and public sites;
//! - `defined`: a document's defined terms, abbreviations and generic words;
//! - `people`: initials after a role, titles and person documents;
//! - `companies`: legal forms, company nouns and short names;
//! - `references`: legal references and cited cases;
//! - `fixes`: span fixes and `Pack::fix_spans`, which applies every rule to model spans;
//! - `context`: labelled values, table columns and identifier fields' labels;
//! - `ocr`: digits read for letters and look-alike letters;
//! - `spans`: span patterns' whole spans: addresses, machine-readable names.
//!
//! Their tests are in `tests/<layer>.rs`, sharing `test_support`.

mod companies;
mod context;
mod defined;
mod fixes;
mod load;
mod ocr;
mod people;
mod public;
mod references;
mod shapes;
mod spans;
#[cfg(test)]
mod test_support;
#[cfg(test)]
mod tests;

pub(crate) use context::{identifier_field, label_words};
use load::Form;
use mdoc_pii::{Category, Detection};
use regex::Regex;
use std::sync::OnceLock;

/// The pack's files, in the order their rules apply.
const FILES: [(&str, &str); 3] = [
    ("shared.toml", include_str!("../../rules/shared.toml")),
    ("en.toml", include_str!("../../rules/en.toml")),
    ("ru.toml", include_str!("../../rules/ru.toml")),
];

/// The pack, read on first use. Every file is checked by a unit test, so a
/// failure here is a build that skipped its tests.
pub(crate) fn pack() -> &'static Pack {
    static PACK: OnceLock<Pack> = OnceLock::new();
    PACK.get_or_init(|| Pack::load(&FILES).expect("rule pack"))
}

/// A `label` entry's word, as `Pack::field_label` reads it.
struct Label {
    words: String,
    category: Category,
    strict: bool,
    only_after: Vec<String>,
}

/// What a field's label says of the value just after it (#739).
pub(crate) enum FieldLabel<'p> {
    /// A value of the category after its label, which the `context` regex
    /// must take whole where the label is a `context`'s.
    Labelled(Category, Option<&'p Regex>),
    /// A value of the category in its strict, unlabelled shape only.
    Strict(Category),
}

/// The checked pack, merged across files.
pub(crate) struct Pack {
    role_surnames: Vec<String>,
    /// Each `initials_after`, `initials_after_citizen` and
    /// `initials_after_party` regex, anchored before the space that ends a
    /// line's text before bare initials, its role as group `role`, with
    /// the role it reads (its `start` left 0).
    initials_after: Vec<(Regex, Role)>,
    /// `initials_companies`, as written.
    initials_companies: Vec<String>,
    /// `initials_never`, as written.
    initials_never: Vec<String>,
    /// `ocr_letters`: each digit with the letters it may stand for.
    ocr_letters: Vec<(char, Vec<char>)>,
    /// `ocr_company_words`.
    ocr_company_words: std::collections::HashSet<String>,
    /// `ocr_person_words`, each anchored to a whole word.
    ocr_person_words: Vec<Regex>,
    never: Vec<Never>,
    /// Generic nouns a document may define as a term (see `DefinedEntry`).
    defined: Vec<Defined>,
    /// One per category, in the order categories first appear.
    shapes: Vec<Shape>,
    contexts: Vec<Context>,
    spans: Vec<Span>,
    /// Top-level domains a bare domain may end with, lowercase.
    zones: std::collections::HashSet<String>,
    /// Domains never proposed as web addresses, lowercase, with their
    /// subdomains.
    public: std::collections::HashSet<String>,
    /// Legal references: a model span inside a match is never proposed.
    references: Vec<Regex>,
    conditional_references: Vec<ConditionalReference>,
    /// Cited cases, references naming their parties: a model span inside
    /// a match goes unless it names a party of the case at hand.
    cases: Vec<Regex>,
    /// Lowercase words that make the name beside them a party of the case at
    /// hand ("plaintiff").
    party_roles: std::collections::HashSet<String>,
    /// Phrases identifying a cited decision as the case at hand's.
    own_before: Vec<String>,
    /// Bodies never public: a span matching one of an entry's patterns and
    /// none of its `unless` is no court or public body.
    private: Vec<(Vec<Regex>, Vec<Regex>)>,
    /// Identifier fields' `label` words, without their dots.
    labels: Vec<Label>,
    /// `lookalike` letters: each look-alike and its Cyrillic letter.
    lookalike_letters: std::collections::HashMap<char, char>,
    /// `lookalike` words, lowercase Cyrillic.
    lookalike_words: std::collections::HashSet<String>,
    /// `lookalike` `unless_after` stems, lowercase Cyrillic.
    lookalike_after: Vec<String>,
}

/// A role before bare initials, as [`Pack::initials_role`] reads it
/// (#491): where it starts in the text before them, whether only a person
/// holds it (`initials_after`, `initials_after_citizen`) or a company may
/// too (`initials_after_party`), and whether only three letters after it
/// are initials (`initials_after_citizen`).
#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) struct Role {
    pub(crate) start: usize,
    pub(crate) person: bool,
    pub(crate) three: bool,
}

/// A defined label is not a name, but its shape alone proves nothing.
struct ConditionalReference {
    pattern: Regex,
    when: Vec<Regex>,
    categories: Vec<Category>,
}

/// A `span` entry: a whole span by its pattern, or a boundary fix.
enum Span {
    /// Proposes each match: an address from its first element to its
    /// postcode or city.
    Pattern {
        category: Category,
        pattern: Regex,
        /// A model span of the category, read from its start by the regex
        /// (anchored there) as its `span` group, is the match's own part
        /// where the match holds it: a building's name in an address is no
        /// company ("Harrow House" of "Harrow House, 12 Pellin Street").
        absorbs: Vec<(Category, Regex)>,
        continuation_after: Option<Regex>,
        continuation_stop: Option<Regex>,
        /// Cuts a match into its spans: the pieces between its matches.
        split: Option<Regex>,
        /// How many characters each line of a whole match has.
        line_length: Option<std::ops::RangeInclusive<usize>>,
        /// A span in which it matches is not proposed.
        skip: Option<Regex>,
        /// A match a name span of the category overlaps is not proposed.
        gaps_only: bool,
    },
    Fix(SpanFix),
}

/// A `span` pattern's span, and for a split one the whole match it was cut
/// from, `lead` and `tail` included: a model span inside that match is no
/// evidence against its pieces ("KRENC<<LIDIIA" holds two of them).
pub(crate) struct SpanMatch {
    pub(crate) detection: Detection,
    pub(crate) whole: Option<std::ops::Range<usize>>,
}

/// A boundary fix for one category's spans.
struct SpanFix {
    category: Category,
    /// Lowercase words that come off either edge ("ms", "kc"), or phrases
    /// of them ("in re").
    trim: Vec<String>,
    role_trim: bool,
    /// Anchored, case-sensitive prefixes, with some name left after them.
    trim_start: Vec<Regex>,
    /// A span inside a match grows to the whole match.
    extend: Vec<Regex>,
    /// Characters that come off the end (a sentence's period).
    strip_end: String,
    /// Lowercase words that, ending an organization span, make it a
    /// person's document: "Ostrand Decl." is the declaration of a person
    /// named Ostrand.
    person_before: Vec<String>,
    /// Lowercase words or phrases that, ending a person's span, make it a
    /// company's: the name before a lowercase one ("Pellam" of "Pellam
    /// airlines"), the whole span before a capitalized one ("Pellam
    /// Bank"). They also end a company's name after its short name.
    org_before: Vec<String>,
    /// Lowercase phrases right before a person's span that make it a
    /// company's: "team known as the" before "Brindle Harriers".
    org_after: Vec<String>,
    list_letter: bool,
    /// Lowercase words a span never grows over ("exhibit" in "EXHIBIT A.
    /// WELK").
    unless_over: Vec<String>,
    /// Legal forms before a name in nested quotes (`nested_names`): each
    /// anchored to end where the name's outer quote opens, and as a whole
    /// word anywhere.
    nested_quotes: Vec<(Regex, Regex)>,
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
    line_start: bool,
    /// The labels follow the value; the pattern starts with it.
    after: bool,
    /// Words one of which sits just before a value its `after` label takes.
    only_after: Vec<String>,
    /// What follows an `after` label, anchored at its end.
    follow: Option<Regex>,
    /// With `unless_slashed`: the labels, longest first, anchored at a
    /// match's start, to find where its label ends.
    unless_slashed: Option<Regex>,
}

/// A `defined` entry: its words, as a `never` entry holds them.
struct Defined {
    list: Never,
    document: bool,
}

struct Never {
    form: Form,
    ending: usize,
    words: Vec<String>,
    patterns: Vec<Regex>,
    last_word_of: Vec<Category>,
    anywhere_in: Vec<Category>,
    not_endings: Vec<String>,
    person_endings: Vec<String>,
    surname_endings: Vec<String>,
    unless_named: bool,
    person_role: bool,
    phrase_only: bool,
    unless_titled: bool,
    unless_last_named: bool,
}

impl Never {
    /// Whether lowercase `word` is one of this entry's words.
    fn has(&self, word: &str) -> bool {
        self.listed(word).is_some()
    }

    /// The index of the entry's word that lowercase `word` is, or a form of.
    fn listed(&self, word: &str) -> Option<usize> {
        self.words.iter().position(|listed| match self.form {
            Form::Whole => word == listed,
            Form::Plural => word == listed || word.strip_suffix('s') == Some(listed),
            Form::Stem => word.strip_prefix(listed.as_str()).is_some_and(|ending| {
                ending.chars().count() <= self.ending
                    && ending.chars().all(char::is_alphabetic)
                    && !self.not_endings.iter().any(|not| not == ending)
            }),
            Form::Pattern => false,
        })
    }

    /// Whether lowercase `word`, one of this entry's words, is a surname
    /// in a person's span: a stem with an ending that is none of its
    /// `person_endings` ("судине", "судник" after "суд"), or, written
    /// `capitalized`, one of its `surname_endings` ("Судов").
    fn surname(&self, word: &str, capitalized: bool) -> bool {
        let own = |endings: &[String]| {
            self.words.iter().any(|listed| {
                word.strip_prefix(listed.as_str())
                    .is_some_and(|ending| endings.iter().any(|own| own == ending))
            })
        };
        self.form == Form::Stem
            && !self.person_endings.is_empty()
            && (!own(&self.person_endings) || capitalized && own(&self.surname_endings))
    }
}

struct Shape {
    category: Category,
    min_code_chars: usize,
    lowercase_at_most_digits: bool,
    forbid_unlabelled: Vec<Regex>,
    require: Vec<Regex>,
    forbid: Vec<Veto>,
    case: Vec<Regex>,
    forbid_in: Vec<Regex>,
    unless_in: Vec<Regex>,
    word: Option<Regex>,
    guess: Vec<Regex>,
    guess_words: Vec<String>,
    quotes: Vec<(char, char)>,
    quotes_after: Vec<String>,
    quotes_after_words: Vec<String>,
}

struct Veto {
    patterns: Vec<Regex>,
    unless: Vec<Regex>,
    unless_after: Vec<Regex>,
}

impl Veto {
    fn matches(&self, text: &str, before: &str) -> bool {
        self.patterns.iter().any(|pattern| pattern.is_match(text))
            && !self.unless.iter().any(|pattern| pattern.is_match(text))
            && !self
                .unless_after
                .iter()
                .any(|pattern| pattern.is_match(before))
    }
}
