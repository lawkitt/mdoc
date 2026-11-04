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

/// A surname's lowercase particles ("де Лаваль", "van Hoorn", "аль-Хадид"):
/// a span they open is a name, never a public body's cut tail.
const NAME_PARTICLES: &[&str] = &[
    "де",
    "ди",
    "да",
    "дю",
    "дель",
    "делла",
    "дер",
    "ден",
    "ла",
    "ле",
    "фон",
    "ван",
    "оглы",
    "кызы",
    "ибн",
    "бен",
    "бин",
    "аль",
    "эль",
    "de",
    "di",
    "da",
    "du",
    "del",
    "della",
    "der",
    "den",
    "la",
    "le",
    "von",
    "van",
    "bin",
    "ibn",
    "ben",
    "al",
    "el",
    "ter",
    "ten",
    "dos",
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
    /// Role words that are also surnames, kept when capitalized as PERSON.
    #[serde(default)]
    role_surnames: Vec<String>,
    #[serde(default)]
    never: Vec<NeverEntry>,
    #[serde(default)]
    defined: Vec<DefinedEntry>,
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
    #[serde(default)]
    url: Vec<UrlEntry>,
    #[serde(default)]
    private: Vec<PrivateEntry>,
}

/// Bodies that are never public, whoever owns them: a span that matches
/// one of the `patterns`, quotes included, is no court or public body
/// ("ФГБОУ ВО «… университет»", "Негосударственный пенсионный фонд «…»"),
/// unless it matches one of `unless` too ("Electoral College").
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct PrivateEntry {
    patterns: Vec<String>,
    #[serde(default)]
    unless: Vec<String>,
}

/// Web addresses' data (ADR 0094): which bare domains are addresses and
/// which domains are public or legal, never proposed.
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct UrlEntry {
    #[serde(default)]
    zones: Vec<String>,
    #[serde(default)]
    public: Vec<String>,
}

/// Generic common nouns a document may define as a term for a party, a
/// thing or a filing ("the Distributor", "the Territory", «Положение»; #390).
/// Unlike `never` words they name no one only where the document itself
/// defines the term (or a term made of generic words, one of them this
/// word): a company that happens to share one is still proposed elsewhere.
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct DefinedEntry {
    #[serde(rename = "match")]
    form: Form,
    /// For `stem`: how many letters may follow the stem.
    ending: Option<usize>,
    #[serde(default)]
    words: Vec<String>,
    /// For `stem`: endings that never count, because they make a surname
    /// of the stem.
    #[serde(default)]
    not_endings: Vec<String>,
    /// Names of filings and contract documents ("Statement", "Motion"): a
    /// long form of generic words with one among them has an abbreviation
    /// that names no one ("Def.'s Statement of Material Facts (“DSMF”)").
    #[serde(default)]
    document: bool,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct NeverEntry {
    #[serde(rename = "match")]
    form: Form,
    /// For `stem`: how many letters may follow the stem.
    ending: Option<usize>,
    #[serde(default)]
    words: Vec<String>,
    /// For `pattern`: regexes; a span matching one outside quotes is a
    /// court, public body, law or place ("Department of Labor").
    #[serde(default)]
    patterns: Vec<String>,
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
    /// For a `stem` with PERSON in `anywhere_in`: in a person's span the
    /// stem counts only with one of these endings, the body word's own
    /// forms ("суда", "Судебной"); any other makes a surname of it
    /// ("Судине", "Судник", "Приставко").
    #[serde(default)]
    person_endings: Vec<String>,
    /// For a stem with `person_endings`: those of them that make a surname
    /// of a capitalized word all the same ("Судов", but "судов общей
    /// юрисдикции").
    #[serde(default)]
    surname_endings: Vec<String>,
    /// Common words that name no one alone ("He", "Program", "Christian")
    /// but stay a person's name in context: after a title ("Mr He") or
    /// where another person span has them beside a name ("Dawn French").
    #[serde(default)]
    unless_named: bool,
    /// Initial plus capitalized role may be a name.
    #[serde(default)]
    person_role: bool,
    /// A role modifier counts only inside a multiword phrase, never alone.
    #[serde(default)]
    phrase_only: bool,
    /// A person's title before a person's span keeps it, a surname that
    /// reads like the phrase or word ("Ms St. Louis", "г-н Суд"); for
    /// words an initial in the span does too ("Суд Н. О.").
    #[serde(default)]
    unless_titled: bool,
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
    /// No words: `patterns` that match a phrase in the span.
    Pattern,
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
    /// By category, what a model span the pattern's match holds looks like
    /// with what follows it, its own text the `span` group: such a span is
    /// part of the match, not a span of its own. The parts work here too.
    #[serde(default)]
    absorbs: std::collections::BTreeMap<String, String>,
    #[serde(default)]
    trim: Vec<String>,
    /// ORG role trims need an explicit separator and an identifying remainder.
    #[serde(default)]
    role_trim: bool,
    /// Case-sensitive regexes removed only from the start of a span.
    #[serde(default)]
    trim_start: Vec<String>,
    #[serde(default)]
    extend: Vec<String>,
    #[serde(default)]
    strip_end: String,
    #[serde(default)]
    person_before: Vec<String>,
}

/// A legal reference or other marker no model span inside is (guide rule 8):
/// "424 U.S. at 38", "26 U.S.C. § 6212", "127-ФЗ". One whose pattern names
/// parties (groups `party`, `party2`, …) is a cited case ("Pellam v.
/// Ostrand, 912 F.3d 455"): a name inside it stays where it names a party
/// of the case at hand.
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct ReferenceEntry {
    /// What the reference looks like; `{name}` stands for a part, and a
    /// group named `lead` or `tail` is context it leaves out.
    pattern: String,
    #[serde(default)]
    parts: std::collections::BTreeMap<String, String>,
    /// For a cited case: lowercase words that, beside a name, make it a
    /// party of the case at hand ("Plaintiff Ivo Harrow", "Quillon Ltd,
    /// Defendant"), for every cited case in the pack.
    #[serde(default)]
    roles: Vec<String>,
    /// Lowercase phrases in the 60 characters before a cited case that
    /// identify it as the case at hand's decision.
    #[serde(default)]
    own_before: Vec<String>,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct VariantsEntry {
    name: String,
    forms: Vec<String>,
}

/// The checked pack, merged across files.
pub(crate) struct Pack {
    role_surnames: Vec<String>,
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
}

/// A cited case in the view's text: its range without `lead` and `tail`,
/// its parties' ranges, and whether a docket number cites it (its `docket`
/// group), which a caption may carry too.
struct Cited {
    range: std::ops::Range<usize>,
    parties: Vec<std::ops::Range<usize>>,
    docket: bool,
}

/// Where `at` in `text` starts once the emphasis just before it is left
/// out: "Mr *Pellam*" reads as "Mr Pellam".
fn unemphasised(text: &str, at: usize) -> usize {
    text[..at].trim_end_matches(['*', '_']).len()
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
    },
    Fix(SpanFix),
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
}

impl SpanFix {
    /// `range` without its `trim` words or phrases at either edge (a word's
    /// final period or colon with it; a comma before a trailing one goes
    /// with it), its `trim_start` prefixes and its `strip_end` characters.
    fn trimmed(&self, source: &str, mut range: std::ops::Range<usize>) -> std::ops::Range<usize> {
        let plain = |word: &str| word.trim_end_matches(['.', ':', ',']).to_lowercase();
        let listed = |word: &str| self.trim.contains(&plain(word));
        let phrases: Vec<Vec<&str>> = self
            .trim
            .iter()
            .filter(|entry| entry.contains(' '))
            .map(|entry| entry.split(' ').collect())
            .collect();
        loop {
            let text = &source[range.clone()];
            let lead = text.len() - text.trim_start().len();
            let tail = text.len() - text.trim_end().len();
            range = range.start + lead..range.end - tail;
            let text = &source[range.clone()];
            if let Some(prefix) = self
                .trim_start
                .iter()
                .filter_map(|pattern| pattern.find(text))
                .find(|prefix| 0 < prefix.end() && prefix.end() < text.len())
            {
                range.start += prefix.end();
                continue;
            }
            // A phrase at either edge, with a word left beside it: "In re
            // Quill" is "Quill".
            let words: Vec<(usize, &str)> = text
                .split(|c: char| c.is_whitespace() || [',', ':', '–', '—'].contains(&c))
                .filter(|word| !word.is_empty())
                .map(|word| (word.as_ptr() as usize - text.as_ptr() as usize, word))
                .collect();
            let is = |slice: &[(usize, &str)], phrase: &[&str]| {
                slice.len() == phrase.len()
                    && slice.windows(2).all(|pair| {
                        text[pair[0].0 + pair[0].1.len()..pair[1].0]
                            .chars()
                            .all(char::is_whitespace)
                    })
                    && slice
                        .iter()
                        .zip(phrase)
                        .all(|((_, word), listed)| plain(word) == *listed)
            };
            // An ORG role must be external to the name. Whitespace alone
            // cannot distinguish "Notary Public Underwriters, Inc".
            let after_role = |end: usize| {
                let rest = text[end..].trim_start();
                let separated = rest.trim_start_matches([',', ':', '-', '–', '—']);
                if separated.len() != rest.len() {
                    return Some(text.len() - separated.trim_start().len());
                }
                if rest.get(..3).is_some_and(|s| s.eq_ignore_ascii_case("for"))
                    && rest[3..].starts_with(char::is_whitespace)
                {
                    let mut name = rest[3..].trim_start();
                    if let Some((first, rest)) = name.split_once(char::is_whitespace)
                        && ["the", "a", "an", "these", "those", "such"]
                            .contains(&first.to_lowercase().as_str())
                    {
                        name = rest.trim_start();
                    }
                    return Some(text.len() - name.len());
                }
                None
            };
            if let Some(phrase) = phrases
                .iter()
                .find(|phrase| words.len() > phrase.len() && is(&words[..phrase.len()], phrase))
            {
                let (_, last) = words[phrase.len() - 1];
                let end = words[phrase.len() - 1].0 + last.trim_end_matches([',', ':']).len();
                if !self.role_trim {
                    range.start += words[phrase.len()].0;
                    continue;
                } else if let Some(start) = after_role(end) {
                    range.start += start;
                    continue;
                }
            }
            if let Some(phrase) = phrases.iter().find(|phrase| {
                words.len() > phrase.len() && is(&words[words.len() - phrase.len()..], phrase)
            }) {
                let rest = &text[..words[words.len() - phrase.len()].0];
                if self.role_trim && !rest.trim_end().ends_with([',', ':', '-', '–', '—']) {
                    return range;
                }
                range.end = range.start
                    + rest
                        .trim_end_matches(|c: char| {
                            [',', ':', '-', '–', '—'].contains(&c) || c.is_whitespace()
                        })
                        .len();
                continue;
            }
            if let Some((first, rest)) = text.split_once(char::is_whitespace)
                && listed(first)
            {
                if !self.role_trim {
                    range.start = range.end - rest.len();
                    continue;
                } else if let Some(start) = after_role(first.trim_end_matches([',', ':']).len()) {
                    range.start += start;
                    continue;
                }
            }
            if let Some((rest, last)) = text.rsplit_once(char::is_whitespace)
                && listed(last)
                && (!self.role_trim || rest.trim_end().ends_with([',', ':', '-', '–', '—']))
            {
                range.end = range.start
                    + rest
                        .trim_end_matches(|c: char| {
                            c.is_whitespace() || [',', ':', '-', '–', '—'].contains(&c)
                        })
                        .len();
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

/// A span's `word` as never words are listed: without the punctuation
/// around it (a hyphen stays) or a possessive "'s" in either case ("the
/// LEA's IEP", "DISTRIBUTOR’S").
fn bare(word: &str) -> &str {
    let word = word.trim_matches(|c: char| !c.is_alphanumeric() && c != '-');
    ["'s", "’s", "'S", "’S"]
        .iter()
        .find_map(|possessive| word.strip_suffix(possessive))
        .filter(|rest| !rest.is_empty())
        .unwrap_or(word)
}

/// English legal forms spelled out, lowercase: a company's name ends in
/// one ("Contract Corporation", "Acme Holdings Limited").
const SPELLED_FORMS: &[&str] = &["corporation", "company", "incorporated", "limited"];

/// Russian words of a legal form spelled out, lowercase: «Общество с
/// ограниченной ответственностью», «Акционерное общество».
const SPELLED_FORMS_RU: &[&str] = &["ответственностью", "акционерное"];

/// The lowercase words of `text` as generic words are listed: `bare`, and
/// without a final period ("Def.'s" is "def").
fn plain_words(text: &str) -> Vec<String> {
    text.split_whitespace()
        .map(|word| bare(word).trim_end_matches('.').to_lowercase())
        .filter(|word| !word.is_empty())
        .collect()
}

/// Words that join the generic words of a defined term ("Terms of
/// Business", "Покупатель и Продавец").
const JOINERS: &[&str] = &["of", "and", "&", "the", "for", "in", "и", "для"];

/// Words that may stand around a quoted defined term in its brackets:
/// "(hereinafter referred to as the “Distributor”)", «(далее по тексту –
/// «Общество»)».
const DEFINING: &[&str] = &[
    "hereinafter",
    "hereafter",
    "herein",
    "referred",
    "to",
    "as",
    "the",
    "a",
    "an",
    "each",
    "collectively",
    "individually",
    "together",
    "called",
    "and",
    "or",
    "jointly",
    "severally",
    "defined",
    "being",
    "also",
    "respectively",
    "singular",
    "plural",
    "now",
    "sometimes",
    "далее",
    "по",
    "тексту",
    "в",
    "дальнейшем",
    "именуемое",
    "именуемый",
    "именуемая",
    "именуемые",
    "также",
    "сокращенно",
    "сокращённо",
    "или",
    "и",
];

/// A defined term and the adjacent clause that identifies what it names.
struct Definition<'a> {
    term: &'a str,
    subject: &'a str,
}

/// Keep the adjacent clause, bounded even when extracted text has no punctuation.
fn definition_subject(text: &str, before: bool) -> &str {
    let boundary = [';', '!', '?', '(', ')', '[', ']'];
    if before {
        let clause = text
            .rsplit(boundary)
            .next()
            .unwrap_or_default()
            .rsplit("\n\n")
            .next()
            .unwrap_or_default()
            .rsplit("\r\n\r\n")
            .next()
            .unwrap_or_default();
        let start = clause.char_indices().rev().nth(199).map_or(0, |(at, _)| at);
        &clause[start..]
    } else {
        let clause = text
            .split(boundary)
            .next()
            .unwrap_or_default()
            .split(". ")
            .next()
            .unwrap_or_default()
            .split("\n\n")
            .next()
            .unwrap_or_default()
            .split("\r\n\r\n")
            .next()
            .unwrap_or_default();
        let end = clause
            .char_indices()
            .nth(200)
            .map_or(clause.len(), |(at, _)| at);
        &clause[..end]
    }
}

/// The terms `text` defines, as written: a quoted term in brackets after
/// "hereinafter", "the" or nothing ("(the “Distributor”)", "(“Townhouse”)",
/// «(далее – «Общество»)»), a bare capitalised one after "hereinafter" or
/// «далее» ("(hereinafter the Distributor)", «(далее – Положение)»), a
/// party named «именуемое в дальнейшем «Поставщик»», and a quoted term its
/// own definition opens with (“Territory” means …, in bold too).
fn definitions(text: &str) -> Vec<Definition<'_>> {
    static PATTERNS: OnceLock<[Regex; 6]> = OnceLock::new();
    let [bracket, quoted, hereinafter, further, named, means] = PATTERNS.get_or_init(|| {
        let compile = |pattern: &str| Regex::new(&rust_classes(pattern)).expect("definition");
        let quotes = r#"“([^“”]{2,60})”|"([^"]{2,60})"|«([^«»]{2,60})»|‘([^‘’]{2,60})’|'([^']{2,60})'"#;
        [
            compile(r"[(\[]([^()\[\]]{2,200})[)\]]"),
            compile(quotes),
            compile(concat!(
                r"^\s*[Hh]ereinafter\b[\s,:]*(?:(?:referred\s+to\s+as|called)\b[\s,:]*)?(?:the\s+)?",
                r"(\p{Uppercase}[\p{Alphabetic}'’-]*(?:\s+\p{Uppercase}[\p{Alphabetic}'’-]*){0,3})\s*$",
            )),
            compile(concat!(
                r"^\s*(?:[Дд]алее(?:\s+по\s+тексту)?|[Вв]\s+дальнейшем(?:\s+по\s+тексту)?)",
                r"\s*[-–—:,]?\s*(\p{Uppercase}\p{Alphabetic}+(?:\s+\p{Alphabetic}+){0,2})\s*$",
            )),
            compile(&format!(r"{}\s*[-–—,:]?\s*(?:{quotes})",
                r"(?i:именуем)\p{Alphabetic}*\s+(?i:в\s+дальнейшем|далее)(?:\s+по\s+тексту)?",
            )),
            compile(&format!(concat!(
                r"(?:{})\*{{0,2}}\s+",
                r"(?:shall\s+)?(?:means?|has\s+the\s+meaning|refers?\s+to)\b",
            ), quotes)),
        ]
    });
    let mut terms = Vec::new();
    for found in bracket.captures_iter(text) {
        let inside = found.get(1).expect("definition contents").as_str();
        let subject = definition_subject(
            &text[..found.get(0).expect("definition bracket").start()],
            true,
        );
        let quotes: Vec<&str> = quoted
            .captures_iter(inside)
            .filter_map(|quote| {
                quote
                    .iter()
                    .skip(1)
                    .flatten()
                    .next()
                    .map(|term| term.as_str())
            })
            .collect();
        if quotes.is_empty() {
            for pattern in [hereinafter, further] {
                if let Some(term) = pattern.captures(inside).and_then(|c| c.get(1)) {
                    terms.push(Definition {
                        term: term.as_str(),
                        subject,
                    });
                }
            }
            continue;
        }
        let lead = quoted.replace_all(inside, " ");
        if lead
            .split(|c: char| !c.is_alphabetic())
            .filter(|word| !word.is_empty())
            .all(|word| DEFINING.contains(&word.to_lowercase().as_str()))
        {
            terms.extend(quotes.into_iter().map(|term| Definition { term, subject }));
        }
    }
    for (pattern, follows) in [(named, false), (means, true)] {
        for found in pattern.captures_iter(text) {
            if let Some(term) = found.iter().skip(1).flatten().next() {
                let range = found.get(0).expect("definition").range();
                let subject = if follows {
                    &text[range.end..]
                } else {
                    &text[..range.start]
                };
                terms.push(Definition {
                    term: term.as_str(),
                    subject: definition_subject(subject, !follows),
                });
            }
        }
    }
    terms
}

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
            role_surnames: Vec::new(),
            never: Vec::new(),
            defined: Vec::new(),
            shapes: Vec::new(),
            contexts: Vec::new(),
            spans: Vec::new(),
            zones: std::collections::HashSet::new(),
            public: std::collections::HashSet::new(),
            references: Vec::new(),
            cases: Vec::new(),
            party_roles: std::collections::HashSet::new(),
            own_before: Vec::new(),
            private: Vec::new(),
        };
        let mut never_seen = std::collections::HashSet::new();
        let mut defined_seen = std::collections::HashSet::new();
        for (name, source) in files {
            let file: File = toml::from_str(source).map_err(|e| format!("{name}: {e}"))?;
            if file.role_surnames.iter().any(|word| {
                word.is_empty()
                    || *word != word.to_lowercase()
                    || !word.chars().all(char::is_alphabetic)
            }) {
                return Err(format!("{name}: role_surnames needs lowercase words"));
            }
            pack.role_surnames.extend(file.role_surnames);
            for (index, entry) in file.never.into_iter().enumerate() {
                let at = |problem: String| format!("{name}: never #{}: {problem}", index + 1);
                let ending = match (entry.form, entry.ending) {
                    (Form::Stem, Some(ending)) if ending > 0 => ending,
                    (Form::Stem, _) => return Err(at("a stem needs an ending above 0".into())),
                    (_, Some(_)) => return Err(at("only a stem takes an ending".into())),
                    (_, None) => 0,
                };
                if entry.form != Form::Stem
                    && !(entry.not_endings.is_empty() && entry.person_endings.is_empty())
                {
                    return Err(at("only a stem takes not_endings or person_endings".into()));
                }
                if !entry.person_endings.is_empty()
                    && !entry.anywhere_in.iter().any(|c| c == "PERSON")
                {
                    return Err(at("person_endings needs PERSON in anywhere_in".into()));
                }
                if let Some(ending) = entry
                    .surname_endings
                    .iter()
                    .find(|ending| !entry.person_endings.contains(ending))
                {
                    return Err(at(format!(
                        "surname ending {ending:?} is no person_endings"
                    )));
                }
                if entry.form == Form::Pattern {
                    if entry.patterns.is_empty() || !entry.words.is_empty() {
                        return Err(at("a pattern entry needs patterns and no words".into()));
                    }
                    if entry.anywhere_in.is_empty() || !entry.last_word_of.is_empty() {
                        return Err(at("a pattern entry needs anywhere_in alone".into()));
                    }
                } else if !entry.patterns.is_empty() {
                    return Err(at("only a pattern entry takes patterns".into()));
                } else if entry.words.is_empty() {
                    return Err(at("no words".into()));
                }
                // A common word may end an organization's role phrase
                // ("Bright Paths Program"), never a person's: context names
                // a person by it ("Paul Program").
                if entry.unless_named
                    && (!entry.anywhere_in.is_empty()
                        || entry.last_word_of.iter().any(|token| token == "PERSON"))
                {
                    return Err(at(
                        "unless_named takes no anywhere_in or last_word_of PERSON".into(),
                    ));
                }
                if entry.unless_titled
                    && if entry.form == Form::Pattern {
                        entry.anywhere_in != ["PERSON"]
                    } else {
                        !entry.anywhere_in.iter().any(|c| c == "PERSON")
                    }
                {
                    return Err(at(if entry.form == Form::Pattern {
                        "unless_titled needs anywhere_in PERSON alone".into()
                    } else {
                        "unless_titled needs PERSON in anywhere_in".into()
                    }));
                }
                if entry.person_role && !entry.unless_named {
                    return Err(at("person_role needs unless_named".into()));
                }
                if entry.phrase_only
                    && (!entry.unless_named
                        || entry.person_role
                        || !entry.anywhere_in.is_empty()
                        || !entry.last_word_of.is_empty())
                {
                    return Err(at("phrase_only needs unless_named alone".into()));
                }
                let patterns = entry
                    .patterns
                    .iter()
                    .map(|pattern| Regex::new(&rust_classes(pattern)).map_err(|e| e.to_string()))
                    .collect::<Result<Vec<_>, _>>()
                    .map_err(at)?;
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
                    patterns,
                    last_word_of: tokens(&entry.last_word_of)?,
                    anywhere_in: tokens(&entry.anywhere_in)?,
                    not_endings: entry.not_endings,
                    person_endings: entry.person_endings,
                    surname_endings: entry.surname_endings,
                    unless_named: entry.unless_named,
                    person_role: entry.person_role,
                    phrase_only: entry.phrase_only,
                    unless_titled: entry.unless_titled,
                });
            }
            for (index, entry) in file.defined.into_iter().enumerate() {
                let at = |problem: String| format!("{name}: defined #{}: {problem}", index + 1);
                let ending = match (entry.form, entry.ending) {
                    (Form::Stem, Some(ending)) if ending > 0 => ending,
                    (Form::Stem, _) => return Err(at("a stem needs an ending above 0".into())),
                    (Form::Pattern, _) => return Err(at("a pattern entry takes no words".into())),
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
                    if !defined_seen.insert((entry.form, word.clone())) {
                        return Err(at(format!("{word:?} is listed twice")));
                    }
                }
                pack.defined.push(Defined {
                    list: Never {
                        form: entry.form,
                        ending,
                        words: entry.words,
                        patterns: Vec::new(),
                        last_word_of: Vec::new(),
                        anywhere_in: Vec::new(),
                        not_endings: entry.not_endings,
                        person_endings: Vec::new(),
                        surname_endings: Vec::new(),
                        unless_named: false,
                        person_role: false,
                        phrase_only: false,
                        unless_titled: false,
                    },
                    document: entry.document,
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
                    // A note marker may follow the label, inside its
                    // emphasis or after it ("ИНН¹:", which the view reads
                    // "ИНН ¹:", or "**ИНН**¹:"; #448).
                    r"(?i)\b(?:{})\b(?-i:(?:[*_]*\p{{Zs}}?[¹²³⁰⁴-⁹]+)?(?:{})((?:{})))",
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
                    || entry.role_trim
                    || !entry.trim_start.is_empty()
                    || !entry.extend.is_empty()
                    || !entry.strip_end.is_empty()
                    || !entry.person_before.is_empty();
                if let Some(pattern) = &entry.pattern {
                    if fixes {
                        return Err(at(
                            "a pattern takes no trim, trim_start, extend, strip_end or person_before".into(),
                        ));
                    }
                    let pattern = with_parts(pattern, &entry.parts).map_err(at)?;
                    let mut absorbs = Vec::new();
                    for (name, regex) in &entry.absorbs {
                        let absorbed = self::category(name).map_err(at)?;
                        let regex =
                            entry
                                .parts
                                .iter()
                                .fold(regex.to_owned(), |regex, (part, piece)| {
                                    regex.replace(&format!("{{{part}}}"), &format!("(?:{piece})"))
                                });
                        let regex = Regex::new(&rust_classes(&format!(r"\A(?:{regex})")))
                            .map_err(|e| at(e.to_string()))?;
                        if !regex.capture_names().flatten().any(|group| group == "span") {
                            return Err(at(format!("absorbs.{name} has no `span` group")));
                        }
                        absorbs.push((absorbed, regex));
                    }
                    pack.spans.push(Span::Pattern {
                        category,
                        pattern,
                        absorbs,
                    });
                    continue;
                }
                if !entry.parts.is_empty() || !entry.absorbs.is_empty() {
                    return Err(at("parts and absorbs need a pattern".into()));
                }
                if !fixes {
                    return Err(at(
                        "needs a pattern, trim words, trim_start, extend patterns, strip_end or person_before"
                            .into(),
                    ));
                }
                if !entry.person_before.is_empty() && category != Category::Organization {
                    return Err(at("only an ORG fix takes person_before".into()));
                }
                if entry.role_trim && (category != Category::Organization || entry.trim.is_empty())
                {
                    return Err(at("role_trim needs ORG and trim words".into()));
                }
                // A trim may be a phrase of single-spaced words ("in re").
                for word in &entry.trim {
                    if word
                        .split(' ')
                        .any(|part| part.is_empty() || part.chars().any(char::is_whitespace))
                    {
                        return Err(at(format!("{word:?} is not words single-spaced")));
                    }
                }
                for word in &entry.person_before {
                    if word.is_empty() || word.chars().any(char::is_whitespace) {
                        return Err(at(format!("{word:?} is not one word")));
                    }
                }
                for word in entry.trim.iter().chain(&entry.person_before) {
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
                let trim_start = entry
                    .trim_start
                    .iter()
                    .map(|pattern| {
                        let regex = Regex::new(&rust_classes(&format!(r"\A(?:{pattern})")))
                            .map_err(|e| at(e.to_string()))?;
                        if regex.is_match("") {
                            return Err(at("trim_start matches an empty prefix".into()));
                        }
                        Ok(regex)
                    })
                    .collect::<Result<Vec<_>, _>>()?;
                pack.spans.push(Span::Fix(SpanFix {
                    category,
                    trim: entry.trim.clone(),
                    role_trim: entry.role_trim,
                    trim_start,
                    extend,
                    strip_end: entry.strip_end.clone(),
                    person_before: entry.person_before.clone(),
                }));
            }
            for (index, entry) in file.url.iter().enumerate() {
                let at = |problem: String| format!("{name}: url #{}: {problem}", index + 1);
                if entry.zones.is_empty() && entry.public.is_empty() {
                    return Err(at("needs zones or public domains".into()));
                }
                for (list, domains, dotted) in [
                    (&mut pack.zones, &entry.zones, false),
                    (&mut pack.public, &entry.public, true),
                ] {
                    for domain in domains {
                        let labels_ok = domain.split('.').all(|label| {
                            !label.is_empty()
                                && label.chars().all(|c| c.is_alphanumeric() || c == '-')
                        });
                        if !labels_ok || (!dotted && domain.contains('.')) {
                            return Err(at(format!("{domain:?} is not a domain")));
                        }
                        if *domain != domain.to_lowercase() {
                            return Err(at(format!("{domain:?} is not lowercase")));
                        }
                        if !list.insert(domain.clone()) {
                            return Err(at(format!("{domain:?} is listed twice")));
                        }
                    }
                }
            }
            for (index, entry) in file.private.iter().enumerate() {
                let at = |problem: String| format!("{name}: private #{}: {problem}", index + 1);
                if entry.patterns.is_empty() {
                    return Err(at("no patterns".into()));
                }
                let compile = |patterns: &[String]| {
                    patterns
                        .iter()
                        .map(|pattern| Regex::new(&rust_classes(pattern)))
                        .collect::<Result<Vec<_>, _>>()
                        .map_err(|e| at(e.to_string()))
                };
                pack.private
                    .push((compile(&entry.patterns)?, compile(&entry.unless)?));
            }
            for (index, entry) in file.reference.iter().enumerate() {
                let at = |problem: String| format!("{name}: reference #{}: {problem}", index + 1);
                let pattern = with_groups(
                    &entry.pattern,
                    &entry.parts,
                    &["lead", "tail", "docket"],
                    true,
                )
                .map_err(at)?;
                let cited = pattern.capture_names().flatten().any(is_party);
                if !cited
                    && (!entry.roles.is_empty()
                        || !entry.own_before.is_empty()
                        || pattern.capture_names().flatten().any(|g| g == "docket"))
                {
                    return Err(at(
                        "only a cited case (a `party` group) takes roles, own_before or a docket"
                            .into(),
                    ));
                }
                let lowercase = |word: &String| {
                    if word.is_empty() || *word != word.to_lowercase() {
                        Err(at(format!("{word:?} is not a lowercase word")))
                    } else {
                        Ok(word.clone())
                    }
                };
                for role in &entry.roles {
                    pack.party_roles.insert(lowercase(role)?);
                }
                for phrase in &entry.own_before {
                    pack.own_before.push(lowercase(phrase)?);
                }
                if cited {
                    pack.cases.push(pattern);
                } else {
                    pack.references.push(pattern);
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
        // A public domain's subdomains are public already ("gov" holds
        // "pravo.gov.ru"); listing one twice hides which entry decides.
        for domain in &pack.public {
            let mut parent = domain.as_str();
            while let Some((_, rest)) = parent.split_once('.') {
                if pack.public.contains(rest) {
                    return Err(format!(
                        "url: {domain:?} is inside {rest:?}, listed already"
                    ));
                }
                parent = rest;
            }
        }
        Ok(pack)
    }

    /// Whether a bare domain may end with top-level domain `zone`.
    pub(crate) fn web_zone(&self, zone: &str) -> bool {
        self.zones.contains(&zone.to_lowercase())
    }

    /// Whether `host` is a public or legal site's, or a subdomain of one
    /// (ADR 0094): "kad.arbitr.ru", "www.legislation.gov.uk".
    pub(crate) fn public_host(&self, host: &str) -> bool {
        let host = host.trim_end_matches('.').to_lowercase();
        let mut rest = host.as_str();
        loop {
            if self.public.contains(rest) {
                return true;
            }
            match rest.split_once('.') {
                Some((_, parent)) => rest = parent,
                None => return false,
            }
        }
    }

    /// Whether `word` is a title, role, determiner or other word that never
    /// names anyone on its own, ignoring case.
    pub(crate) fn never(&self, word: &str) -> bool {
        let word = word.to_lowercase();
        self.never
            .iter()
            // Public-body words only veto spans that contain them: "Суднев"
            // alone is a surname, not a court.
            .filter(|entry| entry.anywhere_in.is_empty() && !entry.phrase_only)
            .any(|entry| entry.has(&word))
    }

    /// Whether `word` is a never word only by an `unless_named` entry: a
    /// common word that may be a person's name in context ("He", "French").
    fn common(&self, word: &str) -> bool {
        let word = word.to_lowercase();
        let mut listing = self
            .never
            .iter()
            .filter(|entry| entry.anywhere_in.is_empty() && !entry.phrase_only && entry.has(&word))
            .peekable();
        listing.peek().is_some() && listing.all(|entry| entry.unless_named)
    }

    /// Whether a model span has its category's `shape` and is not a role
    /// phrase or public body by the `never` section. Common words name no
    /// one here: no context says otherwise.
    #[cfg(test)]
    pub(crate) fn identifying(&self, text: &str, category: Category) -> bool {
        self.identifying_named(text, category, &|_| false, false)
    }

    /// `identifying`, where `named` says which common words (lowercase)
    /// context makes a person's name, and `titled` whether a person's
    /// title stands before the span.
    fn identifying_named(
        &self,
        text: &str,
        category: Category,
        named: &dyn Fn(&str) -> bool,
        titled: bool,
    ) -> bool {
        self.shaped_named(text, category, named)
            && (category == Category::Person && self.role_person_name(text)
                || !self.public_body(text, category, titled))
    }

    /// Whether a span is a court or other public body by the `never`
    /// section's `anywhere_in` words. A legal form makes it a company,
    /// whatever else it says ("Garden Court Ltd"), and a word in quotes is
    /// a name ("Банк «Город»"). The correction stage asks this only after
    /// the span fixes, so a company's inner name ("Северная область Строй")
    /// first grows to its legal form.
    fn public_body(&self, text: &str, category: Category, titled: bool) -> bool {
        self.public_body_by(text, category, titled, true)
    }

    /// `public_body`, by the `never` patterns alone unless `words`.
    fn public_body_by(&self, text: &str, category: Category, titled: bool, words: bool) -> bool {
        // A university or a non-state fund is no public body, its quoted
        // name included (#484).
        let private = self.private.iter().any(|(patterns, unless)| {
            patterns.iter().any(|pattern| pattern.is_match(text))
                && !unless.iter().any(|pattern| pattern.is_match(text))
        });
        if self.legal_form(text) || private {
            return false;
        }
        // Quotes around the whole span hide nothing: "«Арбитражный суд»".
        let outside = unquoted(text);
        let outside = if outside.chars().any(char::is_alphanumeric) {
            outside
        } else {
            text.to_owned()
        };
        let person = category == Category::Person;
        // Initials make a person of a surname that is a public body's word
        // ("Суд Н. О.", "СУД Н. О.", "Н.О. Судова"), where the span is a
        // name alone: initials and capitalized words, no lowercase word
        // ("ГБУ имени Н. О. Ветлина" is an institution); an abbreviation
        // like "ГБУ" is no stem, so initials never spare it.
        let initialed = person && {
            let initial = |word: &str| {
                let mut parts = word.split('.').filter(|part| !part.is_empty()).peekable();
                word.contains('.')
                    && parts.peek().is_some()
                    && parts.all(|part| {
                        part.chars().count() == 1 && part.chars().all(char::is_uppercase)
                    })
            };
            let name = |word: &str| {
                word.chars().next().is_some_and(char::is_uppercase)
                    && word.chars().all(|c| c.is_alphabetic() || c == '-')
            };
            let words: Vec<&str> = outside
                .split_whitespace()
                .map(|word| word.trim_matches(|c: char| !c.is_alphanumeric() && c != '.'))
                .filter(|word| !word.is_empty())
                .collect();
            words.iter().any(|word| initial(word))
                && words.iter().all(|word| initial(word) || name(word))
        };
        // A title keeps any entry's word that is `unless_titled`; initials
        // only a stem's, a surname's ("Суд Н. О.", not "Мэрия Н. О.").
        let spared = |entry: &Never, word: &str, capitalized: bool| {
            person
                && (entry.unless_titled && (titled || initialed && entry.form == Form::Stem)
                    || entry.surname(word, capitalized))
        };
        words
            && outside
                .split_whitespace()
                .map(|word| {
                    let word = bare(word);
                    (
                        word.to_lowercase(),
                        word.chars().next().is_some_and(char::is_uppercase),
                    )
                })
                .any(|(word, capitalized)| {
                    self.never.iter().any(|entry| {
                        entry.anywhere_in.contains(&category)
                            && entry.has(&word)
                            && !spared(entry, &word, capitalized)
                    })
                })
            || self
                .never
                .iter()
                .filter(|entry| {
                    entry.anywhere_in.contains(&category) && !(titled && entry.unless_titled)
                })
                .flat_map(|entry| &entry.patterns)
                .any(|pattern| pattern.is_match(&outside))
    }

    /// Whether a model span at `range` of `text` is cut out of a court's,
    /// public body's or law's name, read with the words beside it (#378):
    /// a lowercase tail after the name's capitalized head ("Министерства
    /// ⟦финансов …⟧"), a span cut at a possessive's apostrophe
    /// ("⟦Sheriff’⟧s Office"), or a head before "of" ("⟦Bureau⟧ of
    /// Oceans"; by the patterns alone, so "⟦Quill⟧ of the Department"
    /// stays).
    fn cut_from_public_body(
        &self,
        text: &str,
        range: std::ops::Range<usize>,
        category: Category,
        titled: bool,
    ) -> bool {
        static TAIL: OnceLock<Regex> = OnceLock::new();
        static OF: OnceLock<Regex> = OnceLock::new();
        let tail = TAIL.get_or_init(|| {
            Regex::new(&rust_classes(
                r"^s?[\p{Zs}\t]+\p{Uppercase}[\p{Alphabetic}\x27’-]*",
            ))
            .expect("possessive pattern")
        });
        let of = OF.get_or_init(|| {
            Regex::new(&rust_classes(
                r"^[\p{Zs}\t]+(?:of|for)[\p{Zs}\t]+(?:the[\p{Zs}\t]+)?\p{Uppercase}[\p{Alphabetic}\x27’-]*",
            ))
            .expect("of pattern")
        });
        let span = &text[range.clone()];
        // A surname's lowercase particle opens a name, not a body's tail:
        // "начальник Отдела ⟦де Лаваль⟧".
        let particle = span
            .split(|c: char| c.is_whitespace() || c == '-')
            .next()
            .is_some_and(|word| NAME_PARTICLES.contains(&word));
        if span.chars().next().is_some_and(char::is_lowercase) && !particle {
            let head = text[..range.start].trim_end_matches([' ', '\t']);
            let from = head
                .char_indices()
                .rev()
                .find(|(_, c)| !c.is_alphanumeric() && *c != '-')
                .map_or(0, |(at, c)| at + c.len_utf8());
            let word = &head[from..];
            if head.len() < range.start
                && word.chars().next().is_some_and(char::is_uppercase)
                && self.public_body(&text[from..range.end], category, titled)
            {
                return true;
            }
        }
        let after = &text[range.end..];
        if span.ends_with(['’', '\''])
            && let Some(found) = tail.find(after)
            && self.public_body(
                &text[range.start..range.end + found.end()],
                category,
                titled,
            )
        {
            return true;
        }
        of.find(after).is_some_and(|found| {
            self.public_body_by(
                &text[range.start..range.end + found.end()],
                category,
                titled,
                false,
            )
        })
    }

    /// Abbreviations `text` defines in brackets beside a long form that
    /// names no one: capitals whose letters are the initials, in order, of
    /// the words just before the bracket. Each is what its long form is,
    /// wherever it stands: a court, public body or law ("Biometric Privacy
    /// Act (“BPA”)", "Department of Justice (DOJ)"; #378), or a programme,
    /// standard, condition or common noun phrase ("National Reading Access
    /// Standard (NRAS)", "Electronic funds transfer (EFT)", "Acute Lung
    /// Injury (ALI)"; #481, #501): a long form the ORG shape and never
    /// words refuse. A long form with a legal form ("Quillon Freight LLC
    /// (“QF”)") or a word of its own ("International Business Machines
    /// (IBM)", "Ossian Draycott (“OD”)") is a name and its abbreviation
    /// stays; so does one the text also defines beside anything else,
    /// which is ambiguous.
    fn unnamed_abbreviations(&self, text: &str) -> std::collections::HashSet<String> {
        static DEFINED: OnceLock<Regex> = OnceLock::new();
        let defined = DEFINED.get_or_init(|| {
            Regex::new(&rust_classes(concat!(
                // The words before: any case, across a line break too.
                r"((?:[\p{Alphabetic}\p{N}&][\p{Alphabetic}\p{N}'’.&-]*(?:[\p{Zs}\t]+|[\p{Zs}\t]*",
                r"\r?\n[\p{Zs}\t]*))*[\p{Alphabetic}\p{N}&][\p{Alphabetic}\p{N}'’.&-]*)",
                // A citation may stand between, piece by piece: ", 740 ILCS
                // 14/1", ", 29 U.S.C. §§ 201, et seq.", ", 42 Stat. 123":
                // numbers, initials, capitals, section signs and a few
                // citation words, never prose ("Act, 2026, applies to …")
                // or a sentence's end ("Act, 2026. ASHMOOR …").
                r"(?:,[\p{Zs}\t]*\d[\p{N}/:–-]*(?:(?:[\p{Zs}\t]+|,[\p{Zs}\t]*)",
                r"(?:\d[\p{N}/:–-]*|(?:\p{Uppercase}\.){1,4}|\p{Uppercase}{2,6}",
                r"|\p{Uppercase}\p{Lowercase}{1,5}\.|§{1,2}|et[\p{Zs}\t]+seq\.?",
                r"|(?:c|ch|ss?|art|pt|para)\.?)){0,12})?\s*\(",
                // "hereinafter", "hereinafter referred to as", "the".
                r"(?:hereinafter(?:[\p{Zs}\t]+(?:referred[\p{Zs}\t]+to[\p{Zs}\t]+as|called))?",
                r"[\p{Zs}\t]*[,:]?[\p{Zs}\t]*)?(?:the[\p{Zs}\t]+)?[“\x22‘']?",
                r"(\p{Uppercase}(?:[\p{Uppercase}\p{N}&.-]){1,11})[”\x22’']?\)",
            )))
            .expect("abbreviation pattern")
        });
        let initials = |word: &str| -> Vec<char> {
            word.split('-')
                .filter_map(|part| part.chars().find(|c| c.is_alphabetic()))
                .flat_map(char::to_lowercase)
                .collect()
        };
        let mut unnamed = std::collections::HashSet::new();
        let mut other = std::collections::HashSet::new();
        for found in defined.captures_iter(text) {
            let (Some(title), Some(short)) = (found.get(1), found.get(2)) else {
                continue;
            };
            let letters: Vec<char> = short
                .as_str()
                .chars()
                .filter(|c| c.is_alphabetic())
                .flat_map(char::to_lowercase)
                .collect();
            let words: Vec<(usize, &str)> = title
                .as_str()
                .split_whitespace()
                .map(|word| (word.as_ptr() as usize - text.as_ptr() as usize, word))
                .collect();
            // The long form: the fewest words before the bracket whose
            // initials spell the abbreviation, the first one its first
            // letter ("Electronic funds transfer", not "pay by electronic
            // funds transfer").
            let from = (letters.len() >= 2)
                .then(|| {
                    (0..words.len()).rev().find(|&first| {
                        let mut spelt = words[first..].iter().flat_map(|(_, w)| initials(w));
                        spelt.next() == Some(letters[0])
                            && letters[1..]
                                .iter()
                                .all(|letter| spelt.by_ref().any(|initial| initial == *letter))
                    })
                })
                .flatten();
            let short = short.as_str().trim_end_matches('.').to_owned();
            let Some(from) = from else {
                other.insert(short);
                continue;
            };
            let long = &text[words[from].0..title.end()];
            // A company's long form names it, whatever its other words
            // ("Contract Corporation (“CC”)"); a filing's names no one.
            let name = !self.public_body(long, Category::Organization, false)
                && (self.company_form(long)
                    || !self.generic_filing(long)
                        && self.shaped_named(long, Category::Organization, &|_| false));
            if name {
                other.insert(short);
            } else {
                unnamed.insert(short);
            }
        }
        unnamed.retain(|short| !other.contains(short));
        unnamed
    }

    /// The `defined` word that lowercase `word` is, as the entry's and the
    /// word's indexes. An -ies or -es plural is its singular's word
    /// ("Subsidiaries" is "Subsidiary", "Businesses" "Business").
    fn defined_word(&self, word: &str) -> Option<(usize, usize)> {
        let listed = |word: &str| {
            self.defined
                .iter()
                .enumerate()
                .find_map(|(entry, defined)| defined.list.listed(word).map(|at| (entry, at)))
        };
        let singular = word
            .strip_suffix("ies")
            .map(|stem| format!("{stem}y"))
            .or_else(|| word.strip_suffix("es").map(str::to_owned))
            .filter(|singular| singular.chars().count() > 1);
        singular
            .and_then(|singular| listed(&singular))
            .or_else(|| listed(word))
    }

    /// Whether `text` has a company's legal form, abbreviated ("Ltd",
    /// "ООО") or spelled out: an English one its last words are
    /// ("Contract Corporation", "Acme Company Limited"), a Russian one
    /// anywhere.
    fn company_form(&self, text: &str) -> bool {
        let words = plain_words(text);
        self.legal_form(text)
            || words
                .last()
                .is_some_and(|word| SPELLED_FORMS.contains(&word.as_str()))
            || words
                .iter()
                .any(|word| SPELLED_FORMS_RU.contains(&word.as_str()))
    }

    /// Whether lowercase `word` is generic wherever it stands: a joiner, or
    /// a role or category `never` word that no person's name is ("Party",
    /// "Buyer", "Bank"; not "French" or "Program").
    fn generic_anywhere(&self, word: &str) -> bool {
        JOINERS.contains(&word) || self.never(word) && !self.common(word)
    }

    /// The `defined` words `text` defines as terms (#390): each word of a
    /// term whose every word is generic ("the “Distributor”", "the
    /// “Franchised Business”", «(далее – Положение)»). A term with a coined
    /// or real word ("the “Kew Project”", “Quillon”, “Fox”) defines none.
    fn defined_terms(&self, text: &str) -> std::collections::HashSet<(usize, usize)> {
        let mut found = std::collections::HashSet::new();
        let mut named = std::collections::HashSet::new();
        if self.defined.is_empty() {
            return found;
        }
        for definition in definitions(text) {
            let mut listed = Vec::new();
            let words = plain_words(definition.term);
            let generic = !words.is_empty()
                && words.iter().all(|word| match self.defined_word(word) {
                    Some(at) => {
                        listed.push(at);
                        true
                    }
                    None => self.generic_anywhere(word),
                });
            if generic {
                // A generic-looking word can be the company's own short
                // name: Network International Ltd (“Network”), ООО
                // «Контракт» (далее — «Контракт»). Name evidence wins even
                // if another definition uses the same word generically.
                let subject: Vec<String> = definition
                    .subject
                    .split(|c: char| !c.is_alphanumeric() && c != '-')
                    .filter(|word| !word.is_empty())
                    .map(str::to_lowercase)
                    .collect();
                let name_words: Vec<String> = definition
                    .term
                    .split(|c: char| !c.is_alphanumeric() && c != '-')
                    .filter(|word| !word.is_empty())
                    .map(str::to_lowercase)
                    .collect();
                let form_only = name_words
                    .iter()
                    .all(|word| SPELLED_FORMS.contains(&word.as_str()) || self.legal_form(word));
                if self.company_form(definition.subject)
                    && !form_only
                    && subject
                        .windows(name_words.len())
                        .any(|part| part == name_words)
                {
                    named.extend(listed);
                } else {
                    found.extend(listed);
                }
            }
        }
        found.retain(|at| !named.contains(at));
        found
    }

    /// Whether `text`, an organization span, is wholly generic words, one
    /// of them a term the document `defined` (#390): "Distributor",
    /// "the Sponsor’s", "RESELLER AGREEMENT", «Положение». A word that is
    /// not generic, a legal form included, makes it a name.
    fn defined_generic(
        &self,
        text: &str,
        defined: &std::collections::HashSet<(usize, usize)>,
    ) -> bool {
        let mut known = false;
        let words = plain_words(text);
        !words.is_empty()
            && words.iter().all(|word| match self.defined_word(word) {
                Some(at) if defined.contains(&at) => {
                    known = true;
                    true
                }
                // Another listed word is generic beside a defined term: a
                // document's name ("RESELLER AGREEMENT"), a companion
                // ("Sponsor Business").
                Some(_) => true,
                None => self.generic_anywhere(word),
            })
            && known
    }

    /// Whether `long`, the words before an abbreviation's bracket, names a
    /// filing or document in generic words only ("Statement of Uncontested
    /// Material Fact", "Motion for Summary Judgment"): a `document` word
    /// among them, and no word that is not generic.
    fn generic_filing(&self, long: &str) -> bool {
        let words = plain_words(long);
        words.iter().any(|word| {
            self.defined_word(word)
                .is_some_and(|(entry, _)| self.defined[entry].document)
        }) && words
            .iter()
            .all(|word| self.defined_word(word).is_some() || self.generic_anywhere(word))
    }

    /// Whether a model span has its category's `shape` and is not a role
    /// phrase by the `never` section; public bodies are not checked.
    /// `named` says which common words (lowercase) context makes a person's
    /// name.
    fn shaped_named(&self, text: &str, category: Category, named: &dyn Fn(&str) -> bool) -> bool {
        let words = || {
            text.split_whitespace()
                .map(bare)
                .filter(|word| !word.is_empty())
        };
        let lowercase: Vec<String> = words().map(str::to_lowercase).collect();
        // A company's legal form keeps a common head word its name ends in
        // ("ООО «Bright Paths Program»", "ООО «Acme Office»").
        let formed = self.legal_form(text);
        let vetoed = self.never.iter().any(|entry| {
            entry.last_word_of.contains(&category)
                && !formed
                && lowercase.last().is_some_and(|w| entry.has(w))
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
                category == Category::Person && self.role_person_name(text)
                    || self.names(text, words().filter(|w| pattern.is_match(w)), named)
            })
    }

    /// A role is weak evidence against a model's name-shaped PERSON.
    /// Do not apply this to ORG or to whole role phrases/headings.
    fn role_person_name(&self, text: &str) -> bool {
        let words: Vec<_> = text.split_whitespace().map(bare).collect();
        let (word, initialed) = match words.as_slice() {
            [word] => (*word, false),
            [initial, word]
                if initial.chars().count() == 1 && initial.chars().all(char::is_uppercase) =>
            {
                (*word, true)
            }
            _ => return false,
        };
        if !word.chars().next().is_some_and(char::is_uppercase) {
            return false;
        }
        let lower = word.to_lowercase();
        let role = self
            .never
            .iter()
            .any(|entry| entry.person_role && entry.has(&lower));
        (initialed && role) || self.role_surnames.contains(&lower)
    }

    /// A determiner immediately before a single role span resolves the
    /// plural ambiguity even when the model omitted it: "the Masters".
    fn determined_role(&self, source: &str, range: std::ops::Range<usize>) -> bool {
        let text = &source[range.clone()];
        let lower = bare(text).to_lowercase();
        if text.split_whitespace().count() != 1
            || !self
                .never
                .iter()
                .any(|entry| entry.person_role && entry.has(&lower))
        {
            return false;
        }
        let before = &source[..range.start];
        let word = before
            .split_whitespace()
            .next_back()
            .unwrap_or_default()
            .to_lowercase();
        before.ends_with(char::is_whitespace)
            && ["the", "these", "those", "such", "данный", "указанный"].contains(&word.as_str())
    }

    /// Whether `words`, those a shape's `word` takes, name someone: one is
    /// no never word, or a common word context names. Initials alone name
    /// someone ("Ms B.", "J. R. French"), but one letter before a common
    /// word is an outline letter: "B. General Terms".
    fn names<'a>(
        &self,
        text: &str,
        words: impl Iterator<Item = &'a str>,
        named: &dyn Fn(&str) -> bool,
    ) -> bool {
        let initial = |word: &str| {
            word.split('.')
                .all(|part| part.chars().count() == 1 && part.chars().all(char::is_uppercase))
        };
        // Initials do not make a role phrase: "R. General" may be a name,
        // while "B. General Terms" is still a heading. Count the whole
        // phrase for role heads, including lowercase ones ("Независимый
        // эксперт") that the name-word shape does not take. Modifiers
        // alone may name a company ("General Accident", "Head First").
        let role_head = text.split_whitespace().map(bare).any(|word| {
            self.never
                .iter()
                .any(|entry| entry.person_role && entry.has(&word.to_lowercase()))
        });
        let (mut name, mut letters, mut common) = (false, 0, false);
        for (index, word) in words.enumerate() {
            let modifier = role_head
                && self
                    .never
                    .iter()
                    .any(|entry| entry.phrase_only && entry.has(&word.to_lowercase()));
            if self.common(word) || modifier {
                if named(&word.to_lowercase()) {
                    name = true;
                } else if index == 1 && letters == 1 {
                    return false;
                } else {
                    common = true;
                }
            } else if !self.never(word) {
                if initial(word) {
                    letters += word.split('.').count();
                } else {
                    name = true;
                }
            }
        }
        name || letters > 1 || letters == 1 && !common
    }

    /// The `span` fixes on `detections` in `source`: each grows to an
    /// `extend` match it lies inside, loses `trim` words at either edge and
    /// `strip_end` characters at its end. A span left without its shape
    /// goes, read as `view` reads it, and so does one a cited case holds
    /// (`cited_cases`). Returns the precedents' ranges in `source`: cited
    /// cases naming no party of the case at hand.
    pub(crate) fn fix_spans(
        &self,
        source: &str,
        view: &mdoc_pii::view::View,
        detections: &mut Vec<Detection>,
    ) -> Vec<std::ops::Range<usize>> {
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
                // Nor may a job phrase consume a titled person's whole
                // name: "Mr Team Leader" must not become just "Mr". The
                // title fix can then remove Mr and establish the name.
                detections[i].range = if trimmed != grown
                    && (self.only_legal_form(&source[trimmed.clone()])
                        || fix.category == Category::Person
                            && self.role_person_name(&source[grown.clone()])
                            && !self.role_person_name(&source[trimmed.clone()])
                        || fix.role_trim
                            && !self.identifying_named(
                                &source[trimmed.clone()],
                                Category::Organization,
                                &|_| false,
                                false,
                            )
                        || fix.category == Category::Person
                            && self.titled(source, trimmed.end)
                            && !self.titled(source, trimmed.start)
                            && !self.shaped_named(
                                &source[trimmed.clone()],
                                Category::Person,
                                &|_| false,
                            )) {
                    grown
                } else {
                    trimmed
                };
            }
        }
        self.person_documents(source, detections);
        let precedents = self.cited_cases(view, detections);
        let abbreviations = self.unnamed_abbreviations(&view.text);
        let defined = self.defined_terms(&view.text);
        let named = self.named_words(view, detections);
        // A name-part header establishes the cell's name even when the
        // spelling is also a role ("Trustee", "Должник"; #575).
        let name_cells = mdoc_pii::columns::name_cells(source);
        detections.retain(|d| {
            let text = &view.text[view.view_range(d.range.clone())];
            let titled = d.category == Category::Person && self.titled(source, d.range.start);
            !d.range.is_empty()
                && !(d.category == Category::Person
                    && self.determined_role(source, d.range.clone()))
                && !(matches!(d.category, Category::Organization | Category::Person)
                    && self.cut_from_public_body(
                        &view.text,
                        view.view_range(d.range.clone()),
                        d.category,
                        titled,
                    ))
                && self.identifying_named(
                    text,
                    d.category,
                    &|word| {
                        d.category == Category::Person
                            && (titled || name_cells.contains(&d.range) || named.contains(word))
                    },
                    titled,
                )
                && !field_code(source, d)
                && !(d.category == Category::Organization
                    && !defined.is_empty()
                    && self.defined_generic(text, &defined))
                && !(matches!(d.category, Category::Organization | Category::Person)
                    && abbreviations.contains(
                        text.trim_matches(|c: char| !c.is_alphanumeric())
                            .trim_end_matches("’s")
                            .trim_end_matches("'s"),
                    ))
        });
        precedents
    }

    /// A precedent's parties before a full or reporter short citation are
    /// legal references. Names outside citations, beside a role or title,
    /// or in the caption protect the case at hand's parties everywhere.
    /// The caption is the first accepted case introducer standing alone
    /// or introducing a judgment, even with a wrapped reporter citation.
    /// A needless mark costs one untick, so uncertain names stay (#561).
    fn cited_cases(
        &self,
        view: &mdoc_pii::view::View,
        detections: &mut Vec<Detection>,
    ) -> Vec<std::ops::Range<usize>> {
        let cited = self.cited(&view.text);
        if cited.is_empty() {
            return Vec::new();
        }
        let text = &view.text;
        let words = |range: std::ops::Range<usize>| -> Vec<String> {
            text[range]
                .split(|c: char| !c.is_alphanumeric())
                .filter(|word| !word.is_empty())
                .map(str::to_lowercase)
                .collect()
        };
        // Whether `outer` holds `inner`'s words in a row.
        let holds = |outer: &[String], inner: &[String]| {
            !inner.is_empty() && outer.windows(inner.len()).any(|run| run == inner)
        };
        let named = |d: &Detection| matches!(d.category, Category::Person | Category::Organization);
        let ranges: Vec<std::ops::Range<usize>> = detections
            .iter()
            .map(|d| view.view_range(d.range.clone()))
            .collect();
        let holder = |range: &std::ops::Range<usize>| {
            cited
                .iter()
                .position(|case| case.range.start <= range.start && range.end <= case.range.end)
        };
        // The caption's lines, including the whole matched citation when
        // either party or its reporter wraps.
        static CAPTION: OnceLock<Regex> = OnceLock::new();
        let mut own: Vec<Vec<String>> = CAPTION
            .get_or_init(|| {
                Regex::new(
                    r"(?:^|[\s|*_])(?P<caption>(?:v|vs)\.?|[Ii]n[\p{Zs}\t]+re|[Ee]x[\p{Zs}\t]+parte|(?:[Ii]n[\p{Zs}\t]+the[\p{Zs}\t]+)?[Mm]atter[\p{Zs}\t]+of)(?:[\s|*_]|$)",
                )
                .expect("a pattern")
            })
            .captures_iter(text)
            .filter_map(|found| found.name("caption"))
            .find(|found| {
                holder(&found.range())
                    .is_none_or(|case| cited[case].docket || cited_caption(text, &cited[case]))
            })
            .map(|found| {
                let mut start = text[..found.start()].rfind('\n').map_or(0, |i| i + 1);
                let mut end = text[found.end()..]
                    .find('\n')
                    .map_or(text.len(), |i| found.end() + i);
                if text[start..end].trim_matches(|c: char| c.is_whitespace() || "#>*_|".contains(c))
                    == found.as_str()
                {
                    start = text[..start]
                        .trim_end_matches(['\r', '\n'])
                        .rfind('\n')
                        .map_or(0, |i| i + 1);
                    if end < text.len() {
                        end = text[end + 1..]
                            .find('\n')
                            .map_or(text.len(), |i| end + 1 + i);
                    }
                }
                if let Some(case) = holder(&found.range()) {
                    start = start.min(cited[case].range.start);
                    end = end.max(cited[case].range.end);
                }
                words(start..end)
            })
            .into_iter()
            .collect();
        for case in &cited {
            let before = &text[..case.range.start];
            let start = before
                .char_indices()
                .rev()
                .take(60)
                .last()
                .map_or(0, |(at, _)| at);
            let context = before[start..].to_lowercase();
            if self
                .own_before
                .iter()
                .any(|phrase| context.contains(phrase))
            {
                own.extend(case.parties.iter().map(|party| words(party.clone())));
            }
        }
        let mut doubtful = vec![false; detections.len()];
        for (i, d) in detections.iter().enumerate() {
            let range = ranges[i].clone();
            let beside = named(d)
                && (self.beside_role(text, range.clone())
                    || self.titled(text, unemphasised(text, range.start)));
            // A permissive party pattern must never swallow an attribution
            // after a model name ("Ivo Harrow for Pellam"). Keep the name
            // even if a rule pack later admits such prose again.
            let prefix = named(d)
                && holder(&range).is_some_and(|case| {
                    cited[case].parties.iter().any(|party| {
                        range.start == party.start
                            && range.end < party.end
                            && text[range.end..party.end]
                                .split(|c: char| !c.is_alphabetic())
                                .any(|word| {
                                    !word.is_empty()
                                        && word.chars().all(char::is_lowercase)
                                        && !party_connector(word)
                                })
                    })
                });
            if holder(&range).is_some() && !beside && !prefix {
                doubtful[i] = true;
            } else if named(d) {
                own.push(words(range));
            }
        }
        let ours = |name: &[String]| {
            own.iter()
                .any(|other| holds(other, name) || holds(name, other))
        };
        // A cited case naming a party of the case at hand is its own earlier
        // decision, whose docket number is the case at hand's too.
        let earlier: Vec<bool> = cited
            .iter()
            .map(|case| case.parties.iter().any(|party| ours(&words(party.clone()))))
            .collect();
        let mut i = 0;
        detections.retain(|d| {
            let range = &ranges[i];
            let keep = !doubtful[i]
                || if named(d) {
                    ours(&words(range.clone()))
                } else {
                    holder(range).is_some_and(|case| earlier[case])
                };
            i += 1;
            keep
        });
        cited
            .iter()
            .zip(&earlier)
            .filter(|(_, earlier)| !**earlier)
            .map(|(case, _)| view.source_range(case.range.clone()))
            .collect()
    }

    /// The cited cases in `text`, each `cases` pattern's matches.
    fn cited(&self, text: &str) -> Vec<Cited> {
        let mut cited = Vec::new();
        for pattern in &self.cases {
            for found in pattern.captures_iter(text) {
                let whole = found.get(0).expect("group 0 is the match");
                let mut start = found.name("lead").map_or(whole.start(), |lead| lead.end());
                let end = found.name("tail").map_or(whole.end(), |tail| tail.start());
                let mut parties: Vec<_> = pattern
                    .capture_names()
                    .flatten()
                    .filter(|group| is_party(group))
                    .filter_map(|group| found.name(group))
                    .map(|party| party.range())
                    .collect();
                let mut uncertain = false;
                for party in &mut parties {
                    // A party may have consumed a sentence's final
                    // abbreviation ("Ivo Harrow Jr."). A period before
                    // another capitalised word is an uncertain boundary.
                    let mut after_boundary = party.start;
                    for (at, _) in text[party.clone()].match_indices('.') {
                        let rest = &text[party.start + at + 1..party.end];
                        let next = rest.trim_start_matches(char::is_whitespace);
                        if next.len() < rest.len()
                            && next.chars().next().is_some_and(char::is_uppercase)
                        {
                            after_boundary = party.end - next.len();
                        }
                    }
                    if after_boundary > party.start {
                        if party.start == start {
                            start = after_boundary;
                            party.start = start;
                        } else {
                            // A boundary after "v." or "In re" breaks the
                            // citation's shape: keep all its names.
                            uncertain = true;
                            break;
                        }
                    }
                }
                if !uncertain && start < end {
                    cited.push(Cited {
                        range: start..end,
                        parties,
                        docket: found.name("docket").is_some(),
                    });
                }
            }
        }
        cited
    }

    /// The lowercase word just before `at` in `text`, emphasis and the
    /// punctuation after it skipped, and where it starts.
    fn word_before(text: &str, at: usize) -> (String, usize) {
        let before = text[..unemphasised(text, at)]
            .trim_end()
            .trim_end_matches([',', ':', '.', '*', '_'])
            .trim_end();
        let start = before.rfind(|c: char| !c.is_alphanumeric()).map_or(0, |i| {
            i + before[i..].chars().next().map_or(1, char::len_utf8)
        });
        (before[start..].to_lowercase(), start)
    }

    /// Whether a party's role ("Plaintiff", "Defendants") stands beside
    /// `range` of `text`: before it, a person's title between them or not
    /// ("Plaintiff Ivo Harrow", "the Defendant, Mr *Quill*"), or after it
    /// ("IVO HARROW, Plaintiff", "Quillon Ltd (Defendant)").
    fn beside_role(&self, text: &str, range: std::ops::Range<usize>) -> bool {
        let role = |word: &str| {
            self.party_roles.contains(word)
                || word
                    .strip_suffix('s')
                    .is_some_and(|single| self.party_roles.contains(single))
        };
        let after = text[range.end..].trim_start_matches(|c: char| {
            c.is_whitespace() || matches!(c, ',' | '(' | '-' | '–' | '*' | '_')
        });
        let next = after
            .split(|c: char| !c.is_alphanumeric())
            .next()
            .unwrap_or_default()
            .to_lowercase();
        if role(&next) {
            return true;
        }
        let mut at = range.start;
        for _ in 0..2 {
            let (word, start) = Self::word_before(text, at);
            if role(&word) {
                return true;
            }
            if word.is_empty() || !self.titled(text, unemphasised(text, at)) {
                return false;
            }
            at = start;
        }
        false
    }

    /// Lowercase words of the person spans in `detections` that name
    /// someone without context ("Dawn French", not "New York" or "Federal
    /// Election Commission"): a common word among them is that person's
    /// name wherever it stands alone ("French").
    fn named_words(
        &self,
        view: &mdoc_pii::view::View,
        detections: &[Detection],
    ) -> std::collections::HashSet<String> {
        detections
            .iter()
            .filter(|d| d.category == Category::Person && !d.range.is_empty())
            .filter(|d| !self.determined_role(&view.text, view.view_range(d.range.clone())))
            .map(|d| &view.text[view.view_range(d.range.clone())])
            .filter(|text| self.identifying_named(text, Category::Person, &|_| false, false))
            .flat_map(|text| {
                text.split_whitespace()
                    .map(|word| bare(word).to_lowercase())
                    .collect::<Vec<_>>()
            })
            .collect()
    }

    /// Whether a person's title stands just before `at` in `source` ("Mr
    /// He", "Dr. General", "г-н Суд"): the person fixes' `trim` words.
    fn titled(&self, source: &str, at: usize) -> bool {
        let before = source[..at].trim_end();
        let before = before.strip_suffix('.').unwrap_or(before);
        let word = before
            .rsplit(|c: char| !c.is_alphanumeric() && c != '-')
            .next()
            .unwrap_or_default()
            .to_lowercase();
        !word.is_empty()
            && self.spans.iter().any(|span| match span {
                Span::Fix(fix) => fix.category == Category::Person && fix.trim.contains(&word),
                Span::Pattern { .. } => false,
            })
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
        // Public bodies wait for the span fixes (`fix_spans`): "Судак" in
        // "ООО «Судак»" grows to its legal form before a court's stem in it
        // could drop it. So do a person's common words, whose context the
        // fixes and the other spans settle ("Mr He", "Dawn French").
        self.shaped_named(text, category, &|_| category == Category::Person) && !quoted
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

    /// Whether a model span of `category` at `at` of `text`, inside a
    /// `span` pattern's match of `matched` at `within`, is that match's own
    /// part (its `absorbs`), read within the match alone: "Harrow House"
    /// before ", 12 Pellin Street" is the address's building, not a company;
    /// a number after the match ("…, Chatham House, 12 March") is none of it.
    pub(crate) fn absorbs(
        &self,
        text: &str,
        matched: Category,
        within: std::ops::Range<usize>,
        category: Category,
        at: std::ops::Range<usize>,
    ) -> bool {
        self.spans.iter().any(|span| match span {
            Span::Pattern {
                category: of,
                absorbs,
                ..
            } if *of == matched => absorbs.iter().any(|(absorbed, regex)| {
                *absorbed == category
                    && regex
                        .captures(&text[at.start..within.end])
                        .and_then(|found| found.name("span"))
                        .is_some_and(|span| span.range() == (0..at.len()))
            }),
            _ => false,
        })
    }

    /// Whole spans the `span` patterns find in `source`: an address from
    /// its first element to its postcode or city (ADR 0092).
    pub(crate) fn spans(&self, source: &str) -> Vec<Detection> {
        self.spans
            .iter()
            .filter_map(|span| match span {
                Span::Pattern {
                    category, pattern, ..
                } => Some((*category, pattern)),
                Span::Fix(_) => None,
            })
            .flat_map(|(category, pattern)| {
                span_ranges(pattern, source)
                    .into_iter()
                    // An address match before a colon ends in a field's
                    // label ("12 Oak Road, Email: …"), not the address's
                    // last part; a name before one is a speaker's ("ФИО1
                    // А. Б.: …").
                    .filter(move |range| {
                        category != Category::Address
                            || !source[range.end..]
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
    with_groups(pattern, parts, &["lead", "tail"], false)
}

/// Lowercase words admitted between the capitalised words of a party.
/// `ex rel.` is the only supported multiword connector.
fn party_connector(word: &str) -> bool {
    matches!(
        word,
        "of" | "the" | "and" | "de" | "du" | "van" | "von" | "der" | "la" | "le" | "ex" | "rel"
    )
}

/// A citation may be the document's own caption. Require its own line
/// (allowing Markdown and the court after a citation), or a judgment
/// label in the next three nonempty lines. Prose before/after a citation
/// alone, such as "Motion under Pellam ...", does not establish a caption.
fn cited_caption(text: &str, case: &Cited) -> bool {
    let start = text[..case.range.start].rfind('\n').map_or(0, |i| i + 1);
    let end = text[case.range.end..]
        .find('\n')
        .map_or(text.len(), |i| case.range.end + i);
    let unmarked = |s: &str| {
        s.trim_matches(|c: char| c.is_whitespace() || "#*_".contains(c))
            .to_owned()
    };
    static COURT: OnceLock<Regex> = OnceLock::new();
    let standalone = unmarked(&text[start..case.range.start]).is_empty()
        && COURT
            .get_or_init(|| Regex::new(r"^(?:\s*\([^()\n]*\))*[.,;:]?$").expect("a pattern"))
            .is_match(&unmarked(&text[case.range.end..end]));
    standalone
        || text[end..]
            .lines()
            .map(unmarked)
            .filter(|line| !line.is_empty())
            .take(3)
            .any(|line| {
                matches!(
                    line.as_str(),
                    "Judgment"
                        | "JUDGMENT"
                        | "Approved Judgment"
                        | "Opinion"
                        | "OPINION"
                        | "Order"
                        | "ORDER"
                ) || line.starts_with("Before:")
            })
}

/// Whether a reference's group names a cited case's party: `party`,
/// `party2`, ….
fn is_party(group: &str) -> bool {
    group
        .strip_prefix("party")
        .is_some_and(|rest| rest.chars().all(|c| c.is_ascii_digit()))
}

/// [`with_parts`] allowing `groups`, and party groups where `parties`.
fn with_groups(
    pattern: &str,
    parts: &std::collections::BTreeMap<String, String>,
    groups: &[&str],
    parties: bool,
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
        .find(|group| !groups.contains(group) && !(parties && is_party(group)))
    {
        return Err(if parties {
            format!("group {group:?} is not `lead`, `tail`, `docket` or a `party`")
        } else {
            format!("group {group:?} is not `lead` or `tail`")
        });
    }
    Ok(pattern)
}

/// `text` without what it quotes in closed pairs («…», "…", “…”), «» nested:
/// "ГКУ «Дирекция района»" is "ГКУ ". An unclosed quote stays as it is.
fn unquoted(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    let mut rest = text;
    while let Some(open) = rest.find(['«', '"', '“']) {
        let quote = rest[open..].chars().next().expect("found");
        let after = &rest[open + quote.len_utf8()..];
        let close = match quote {
            '«' => {
                let mut depth = 1;
                after.char_indices().find_map(|(at, c)| {
                    match c {
                        '«' => depth += 1,
                        '»' => depth -= 1,
                        _ => {}
                    }
                    (depth == 0).then_some(at)
                })
            }
            '"' => after.find('"'),
            _ => after.find('”'),
        };
        out.push_str(&rest[..open]);
        match close {
            Some(at) => {
                out.push(' ');
                let closing = after[at..].chars().next().expect("found");
                rest = &after[at + closing.len_utf8()..];
            }
            None => {
                out.push(quote);
                rest = after;
            }
        }
    }
    out.push_str(rest);
    out
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

/// Whether an organization or person span is a field code heading a
/// Markdown table's column ("| FLDA | FLDB |" over a delimiter row): a
/// whole header cell of capitals, digits and underscores, up to ten, in a
/// header of two cells or more that are all such codes, a list of fields,
/// not of companies (guide rule 8, issue #501). A header with any other
/// cell keeps its spans ("| HSBC | Quillon Freight LLC |", "| IBM | Price
/// |").
fn field_code(source: &str, detection: &Detection) -> bool {
    let code = |text: &str| {
        (2..=10).contains(&text.len())
            && text.starts_with(|c: char| c.is_ascii_uppercase())
            && text
                .chars()
                .all(|c| c.is_ascii_uppercase() || c.is_ascii_digit() || c == '_')
    };
    let range = detection.range.clone();
    let text = source[range.clone()].trim();
    if !matches!(
        detection.category,
        Category::Organization | Category::Person
    ) || !code(text)
    {
        return false;
    }
    let start = source[..range.start].rfind('\n').map_or(0, |at| at + 1);
    let end = source[range.end..]
        .find('\n')
        .map_or(source.len(), |at| range.end + at);
    let line = &source[start..end];
    let next = source[end..]
        .strip_prefix('\n')
        .map(|rest| rest.split('\n').next().unwrap_or_default());
    let header: Vec<(std::ops::Range<usize>, &str)> = cells(line, start)
        .into_iter()
        .map(|cell| {
            let text = source[cell.clone()].trim_matches(['*', '`', ' ', '\r']);
            (cell, text)
        })
        .filter(|(_, text)| !text.is_empty())
        .collect();
    is_table_row(line)
        && next.is_some_and(is_delimiter_row)
        && header.len() >= 2
        && header.iter().all(|(_, cell)| code(cell))
        && header.iter().any(|(cell, cell_text)| {
            cell.start <= range.start && range.end <= cell.end && *cell_text == text
        })
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
                "Исполнитель: Студия Белявская О. М. Заказчик: ООО «Ромашка».",
                &[
                    ("Студия Белявская О. М.", Category::Organization),
                    ("ООО «Ромашка».", Category::Organization),
                ]
            ),
            ["Студия Белявская О. М.", "ООО «Ромашка»"]
        );
        assert_eq!(
            fixed(
                "Исполнитель: Студия Белявская О.М.. Подрядчик: Альфа-Строй.",
                &[
                    ("Студия Белявская О.М..", Category::Organization),
                    ("Альфа-Строй.", Category::Organization),
                ]
            ),
            ["Студия Белявская О.М.", "Альфа-Строй"]
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

    /// Courts, public bodies, state institutions and laws in their case
    /// forms and abbreviations are no organizations (#378), nor is a
    /// country, region or city alone or as a body's tail (#478); companies,
    /// banks and a company named after the country stay.
    #[test]
    fn public_bodies_laws_and_places_are_no_organizations() {
        let pack = pack();
        for text in [
            // Courts.
            "Арбитражный суд Северной области",
            "Арбитражного суда Северо-Западного округа",
            "Верховного Суда Российской Федерации",
            "АС Северной области",
            "ФАС Восточного округа",
            "14 ААС",
            "Пленума ВС РФ",
            "ВС РФ",
            "Судебная коллегия",
            "Коллегия",
            // Ministries, services, agencies and their territorial units.
            "ФНС России",
            "МИФНС России № 9",
            "Федеральной налоговой службы",
            "ГУ МВД России по Северной области",
            "Отдел МВД России по Ольховскому району",
            "ГИАЗ ОМВД России",
            "Ольховского ГОСП УФССП",
            "отдела судебных приставов по Центральному округу",
            "Минтруда России",
            "Управлением Росреестра по Северной области",
            "Правительства Северной области",
            "Мэрию муниципального образования",
            "ГБ МСЭ",
            // State institutions.
            "ГКУ города Ольховска",
            "Федерального казенного учреждения",
            "государственного бюджетного учреждения",
            // Codes and laws.
            "Гражданского кодекса Российской Федерации",
            "Арбитражного процессуального кодекса",
            "Кодекса Российской Федерации об административных правонарушениях",
            "Гражданский кодекс РФ",
            "АПК РФ",
            "КоАП РФ",
            "Федерального закона",
            "Федеральным законом",
            "Конституции Российской Федерации",
            // Places, alone or as a tail.
            "РОССИЙСКОЙ ФЕДЕРАЦИИ",
            "Российской Федерации",
            "Российская Федерация",
            "России",
            "субъектов Российской Федерации",
            "Именем Российской Федерации",
            "Северной области",
            "Кедровской Республики",
            "Ольховскому району",
            "Ольховскому краю",
            "городу Ольховску",
            "г. Ольховска",
            "ПГТ Ольховка",
            "Ольховского муниципального района Северной области",
        ] {
            assert!(!pack.identifying(text, Category::Organization), "{text}");
        }
        for text in [
            "ООО «Россия-Трейд»",
            "ООО «Россия»",
            "Банк «Россия»",
            "Сбербанк России",
            "ПАО Сбербанк",
            "АО «Северный банк»",
            "Российский союз автостраховщиков",
            "ООО «Северная область Строй»",
            "ИП Ветлин, г. Ольховск",
            "Федерация футбола России",
            "коллегия адвокатов «Елагин и партнёры»",
            "ООО «Отделстрой»",
            "ООО «Судак»",
            "Ромашка-Плюс",
            // A name in quotes is a name, whatever words it has.
            "Банк «Город»",
            "Банк «Северный край»",
        ] {
            assert!(pack.identifying(text, Category::Organization), "{text}");
        }
        for text in [
            "ФКЗ РФ",
            "ФЗ",
            "ФЗ № 127-ФЗ",
            "Федерального закона от 12.03.2026 № 418-ФЗ",
            "ГКУ «Дирекция парков»",
            "Суд «Северный»",
            "«Арбитражный суд Северной области»",
        ] {
            assert!(!pack.identifying(text, Category::Organization), "{text}");
        }
        assert_eq!(
            unquoted("ГКУ «Центр «Северный» снабжения» и суд"),
            "ГКУ   и суд"
        );
        assert_eq!(unquoted("Банк \"Город\", суд «Север"), "Банк  , суд «Север");
        // Gone whole, wherever the model's span starts: an institution's
        // quoted name with it, a court's tail.
        assert!(
            fixed(
                "Ответчик: ГКУ города Ольховска «Дирекция парков».",
                &[
                    ("ГКУ города Ольховска", Category::Organization),
                    ("Дирекция парков", Category::Organization),
                ]
            )
            .is_empty()
        );
        for source in [
            "третье лицо — Федеральное казенное учреждение «Центр снабжения».",
            "третье лицо — ГБУ Северной области «Центр снабжения».",
            "третье лицо — ГБУЗ Республики Сосновия «Центр снабжения».",
            "третье лицо — ГКУ г. Ольховска «Центр снабжения».",
            "третье лицо — ГБУ города Нижнего Ольховска «Центр снабжения».",
            "третье лицо — ГКУ Северо-Ольховского автономного округа «Центр снабжения».",
        ] {
            assert!(
                fixed(source, &[("Центр снабжения", Category::Organization)]).is_empty(),
                "{source}"
            );
        }
        // Never across another name: a company's legal form ends the
        // institution's reach.
        assert_eq!(
            fixed(
                "ГКУ и ООО «Ромашка-Плюс»",
                &[("Ромашка-Плюс", Category::Organization)]
            ),
            ["ООО «Ромашка-Плюс»"]
        );
        assert_eq!(
            fixed(
                "Именем Российской Федерации Арбитражный суд Северной области \
                 рассмотрел иск ООО «Ромашка-Плюс».",
                &[
                    ("Российской Федерации", Category::Organization),
                    ("Северной области", Category::Organization),
                    ("Ромашка-Плюс", Category::Organization),
                ]
            ),
            ["ООО «Ромашка-Плюс»"]
        );
    }

    /// English courts, agencies, departments, offices, laws and codes are
    /// no organizations and no people (#378), nor is a country, state or
    /// city (#478); companies with their legal form, and private bodies that
    /// share a word with them, stay.
    #[test]
    fn english_public_bodies_laws_and_places_are_no_organizations() {
        let pack = pack();
        let org = |text: &str| pack.identifying(text, Category::Organization);
        let person = |text: &str| pack.identifying(text, Category::Person);
        for text in [
            // Departments, offices, agencies and bureaus.
            "Department of Justice",
            "U.S. Department of Labor",
            "Dep't of Labor",
            "Department for Work and Pensions",
            "Ashmoor County Probation Department",
            "Office of the Attorney General",
            "Home Office",
            "Clerk's Office",
            "Agency for International Development",
            "National Security Agency",
            "Bureau of Prisons",
            "Census Bureau",
            // Commissions, legislatures, services and councils.
            "Federal Election Commission",
            "Securities and Exchange Commission",
            "Commission on Civil Rights",
            "National Labor Relations Board",
            "Board of Education",
            "Ashmoor County Board of Supervisors",
            "Congress",
            "the Legislature",
            "Senate Judiciary Committee",
            "Social Security Administration",
            "Internal Revenue Service",
            "Crown Prosecution Service",
            "Ashmoor City Council",
            "HM Treasury",
            "HM Land Registry",
            "His Majesty's Revenue and Customs",
            // Short names.
            "IRS",
            "the IRS",
            "U.S. FDA",
            "CMA",
            "EEOC",
            "OHRP",
            "SEC",
            "EAT",
            "the ET",
            // Prosecutors and ministers.
            "Attorney General",
            "Attorney-General's Office",
            "U.S. Attorney",
            "District Attorney",
            "Secretary of State",
            // Courts by circuit, district and division.
            "1st Cir",
            "9th Cir.",
            "Ninth Circuit",
            "Federal Circuit",
            "Northern District of Ashmoria",
            "Chancery Division",
            "King's Bench Division",
            // Laws, codes and rules.
            "Gaming Act 2031",
            "Biometric Information Privacy Act",
            "the First Amendment",
            "Data Protection Regulations",
            "Bankruptcy Code",
            "Code of Federal Regulations",
            "18 U.S.C.",
            "Civil Procedure Rules",
            "Federal Rules of Evidence",
            // Places alone, together, and after "State of" and the like.
            "Pakistan",
            "North Macedonia",
            "Republic of Korea",
            "Illinois",
            "Massachusetts",
            "United States",
            "United States of America",
            "the United Kingdom",
            "U.S.",
            "U.S",
            "USA",
            "Boston",
            "New York",
            "State",
            "States",
            "Federal",
            "Georgia v. Alabama",
            "Washington, D.C.",
            "State of Ashmoria",
            "STATE OF ASHMORIA",
            "Commonwealth of Brindlemoor",
            "People of the State of Ashmoria",
            "Town of Brindle",
            "District of Columbia",
            "Ashmoor County",
        ] {
            assert!(!org(text), "{text}");
        }
        for text in [
            // A legal form keeps a company, whatever public or place word
            // it has; none cuts it.
            "Bank of America, N.A.",
            "State Street Corporation",
            "Georgia-Pacific LLC",
            "Georgia-Pacific",
            "Federal Express Corp.",
            "Federal Express",
            "Quillon Freight LLC",
            "Brindle Court Ltd",
            "Ashmoor Department Store Ltd",
            "State of Ashmoria Holdings LLC",
            "Congress Street Partners LLP",
            // Private bodies that share a word with a public one.
            "Northbridge Department Store",
            "Acme Talent Agency",
            "Cedar Arts Council",
            "Law Offices of Quill & Venn",
            "New York Life",
            "Boston Scientific",
            "Ohio Valley Freight",
            "United Airlines",
            "Washington Post",
            "Delaware North",
            // Short names and words of public bodies that companies share
            // (review of #515).
            "CMA CGM",
            "SEC Newgate",
            "Commission Junction",
            "The Republic of Tea",
        ] {
            assert!(org(text), "{text}");
        }
        for text in [
            "HMRC",
            "Gaming Act 1968",
            "Biometric Information Privacy Act",
            "Ashmoor County Probation Department",
            "Department of Justice",
            "Federal Election Commission",
            "High Court of Justice",
            "State of Ashmoria",
            "Town of Plymouth",
            "Ninth Circuit",
            "Chancery Division",
            "Attorney General",
            "Bankruptcy Code",
            "National Labor Relations Board",
            "IRS",
        ] {
            assert!(!person(text), "{text}");
        }
        for text in [
            "Georgia Quill",
            "Virginia Ashby",
            "Washington Vale",
            "Sec. Brindle",
            "Justice Brindle",
            "Attorney General Brindle",
            "Pakistan",
        ] {
            assert!(person(text), "{text}");
        }
        // A place's name after "Town of" grows to the place and goes, as an
        // organization or a person; the other side of a caption stays.
        assert_eq!(
            fixed(
                "The Town of Brindle (Illinois) sued; Town of Plymouth v. Orrin Quill.",
                &[
                    ("Brindle", Category::Organization),
                    ("Illinois", Category::Organization),
                    ("Plymouth", Category::Person),
                    ("Orrin Quill", Category::Person),
                ]
            ),
            ["Orrin Quill"]
        );
        // An abbreviation defined beside a law or a public body is the law
        // or the body wherever it stands; a company's stays.
        assert_eq!(
            fixed(
                "Under the Biometric Information Privacy Act, 740 ILCS 14/1 (“BIPA”), \
                 and the Equal Employment Opportunity Commission (EEOC) rules, \
                 Quillon Freight LLC (“QF”) answered. BIPA applies; the EEOC \
                 agreed; BIPA’s terms bind QF.",
                &[
                    ("BIPA", Category::Organization),
                    ("EEOC", Category::Person),
                    ("BIPA’s", Category::Person),
                    ("QF", Category::Organization),
                ]
            ),
            ["QF"]
        );
        // A title wrapped across a line, LF or CRLF.
        for source in [
            "the Biometric Information
Privacy Act (“BIPA”). BIPA applies.",
            "the Biometric Information
Privacy Act (“BIPA”). BIPA applies.",
        ] {
            let at = source.rfind("BIPA").unwrap();
            let mut detections = vec![Detection {
                range: at..at + 4,
                category: Category::Organization,
                score: 0.9,
                recognizer: Recognizer::Model,
            }];
            pack.fix_spans(source, &mdoc_pii::view::View::new(source), &mut detections);
            assert!(detections.is_empty(), "{source:?}");
        }
        // An abbreviation the text also defines for something else is
        // ambiguous and stays.
        let source = "the Data Protection Act (“DPA”); DPA Microphones (“DPA”) supplied                       it. DPA invoiced us.";
        let at = source.rfind("DPA").unwrap();
        let mut detections = vec![Detection {
            range: at..at + 3,
            category: Category::Organization,
            score: 0.9,
            recognizer: Recognizer::Model,
        }];
        pack.fix_spans(source, &mdoc_pii::view::View::new(source), &mut detections);
        assert_eq!(detections.len(), 1);
    }

    /// Probe round 2 of #378 in Russian: courts and public bodies go as
    /// people too, and so do the pension and social funds, their offices,
    /// the central bank and more short forms; a surname with a body's
    /// stem, a titled or initialled person and companies stay.
    #[test]
    fn russian_public_bodies_are_no_people_and_funds_no_organizations() {
        let pack = pack();
        let org = |text: &str| pack.identifying(text, Category::Organization);
        let person = |text: &str| pack.identifying(text, Category::Person);
        for text in [
            "Арбитражный суд Республики Сосновия",
            "Арбитражный суд Северной области",
            "Министерства обороны Российской Федерации",
            "Министерства",
            "Администрации г. Ольховска",
            "Государственное учреждение - Управление Пенсионного фонда \
             Российской Федерации по Ольховскому району",
            "Минобороны России",
            "Минфина России",
            "Президента Российской Федерации",
            "Гражданского кодекса Российской Федерации",
            "Суд",
        ] {
            assert!(!person(text), "{text}");
        }
        for text in [
            "Пенсионный фонд Российской Федерации",
            "Пенсионного фонда РФ",
            "Фонда пенсионного и социального страхования Российской Федерации",
            "Фонда социального страхования",
            "Фонда социального страхованияРоссийской Федерации",
            "Социальный фонд России",
            "бюджет Фонда",
            "Фонда",
            "УПФР",
            "ОПФР по Северной области",
            "ОСФР",
            "УФК по Северной области",
            "ГУ",
            "Регистрационно-лицензионной палатой",
            "Президента Российской Федерации",
            "ЦБ РФ",
            "Банк России",
            "Центрального банка Российской Федерации",
            "ВАС РФ",
            "Президиума ВАС РФ",
            "АС Ольховска и Северной обл",
            "АС Северной области",
            "ЛК РФ",
            "Депнедра",
            "ФГАУ «Жилкомплекс»",
            "Вооруженных Сил Российской Федерации",
            "Налоговые органы",
            "гражданским законодательством Российской Федерации",
        ] {
            assert!(!org(text), "{text}");
        }
        for text in [
            "Судов",
            "Суднев",
            "Судина",
            "Приставко",
            "Приставкина",
            "Законов",
            "Палатова",
            "Суд Н. О.",
            "Н.О. Суд",
            "Гу",
            "Иван Суднев",
        ] {
            assert!(person(text), "{text}");
        }
        for text in [
            "ООО «Фонд Север»",
            "Фонд «Север»",
            "Благотворительный фонд «Север»",
            "Сбербанк России",
            "Банк «Россия»",
            "АС Групп",
            "ООО «Палата»",
        ] {
            assert!(org(text), "{text}");
        }
        // A title keeps a surname that is a court's word.
        assert_eq!(
            fixed("Г-н Суд пояснил.", &[("Суд", Category::Person)]),
            ["Суд"]
        );
        // A state institution's quoted name, as a person, goes with it.
        assert!(
            fixed(
                "жалобу ГБУ СО «Кедровый дом-интернат»",
                &[("Кедровый дом-интернат", Category::Person)]
            )
            .is_empty()
        );
    }

    /// Probe round 2 of #378: a span cut out of a public body's name goes,
    /// read with the words beside it; a person beside a body's word stays.
    #[test]
    fn a_span_cut_out_of_a_public_body_goes() {
        for (source, span, category) in [
            (
                "жалобу Министерства финансов Российской Федерации",
                "финансов Российской Федерации",
                Category::Organization,
            ),
            (
                "жалобу Министерства обороны Российской Федерации",
                "обороны Российской Федерации",
                Category::Person,
            ),
            (
                "the Ashmoor County Sheriff’s Office arrested him",
                "Ashmoor County Sheriff’",
                Category::Organization,
            ),
            (
                "the Bureau of Oceans, Environment, and Science",
                "Bureau",
                Category::Organization,
            ),
            (
                "the Office of the Attorney General",
                "Office",
                Category::Person,
            ),
        ] {
            assert!(fixed(source, &[(span, category)]).is_empty(), "{source}");
        }
        for (source, span, category) in [
            (
                "начальник Отдела Ветлина Н. О. пояснила",
                "Ветлина Н. О.",
                Category::Person,
            ),
            (
                "Ms Quill of the Department of Labor",
                "Quill",
                Category::Person,
            ),
            ("Quill’s Office", "Quill", Category::Person),
            (
                "Quillon Freight LLC of the Bureau of Prisons",
                "Quillon Freight LLC",
                Category::Organization,
            ),
        ] {
            assert_eq!(fixed(source, &[(span, category)]), [span], "{source}");
        }
    }

    /// The review of probe round 2 of #378: a surname with a body's stem,
    /// a particle after a body's word, a non-state fund, a university, a
    /// private name with an agency's letters and a company's abbreviation
    /// after a law stay; an institution named after a person goes.
    #[test]
    fn names_and_private_bodies_beside_public_ones_stay() {
        let pack = pack();
        let org = |text: &str| pack.identifying(text, Category::Organization);
        let person = |text: &str| pack.identifying(text, Category::Person);
        // A surname with a court's stem, whatever its ending; the court's
        // own forms still go.
        for text in ["Николае Судине", "Иван Судник", "Судине", "Судник"]
        {
            assert!(person(text), "{text}");
        }
        for text in ["Суда", "Судом", "Судебной коллегии", "Законом", "Палатой"]
        {
            assert!(!person(text), "{text}");
        }
        // A lowercase particle opens the person's name, not the body's.
        assert_eq!(
            fixed(
                "начальник Отдела де Лаваль пояснил",
                &[("де Лаваль", Category::Person)]
            ),
            ["де Лаваль"]
        );
        // Non-state funds, universities and colleges are no public bodies
        // (#484), whoever owns them.
        for text in [
            "Негосударственный пенсионный фонд «Кедр»",
            "Негосударственного пенсионного фонда «Кедр»",
            "НПФ «Кедр»",
            "ФГБОУ ВО «Северный государственный университет»",
            "Северный государственный университет",
            "ГБПОУ «Ольховский колледж»",
            "Ashmoor State University",
            "University of Ashmoor",
            "Ashmoor County Community College",
        ] {
            assert!(org(text), "{text}");
        }
        assert_eq!(
            fixed(
                "ректор ФГБОУ ВО «Северный государственный университет» сообщил",
                &[(
                    "Северный государственный университет",
                    Category::Organization
                )]
            ),
            ["ФГБОУ ВО «Северный государственный университет»"]
        );
        // An agency's letters or a grand jury inside a private name.
        assert!(person("Nato Venn"));
        assert!(org("NATO Logistics"));
        assert!(org("Grand Jury Games"));
        assert!(!org("the Ashmoor County Grand Jury"));
        // Prose after a law's year is no citation: the company's own
        // abbreviation stays.
        assert_eq!(
            fixed(
                "Ashmoor Freight Safety Act, 2026, applies to Ashmoor Freight Services \
                 (hereinafter “AFS”). AFS wrote.",
                &[("AFS", Category::Organization)]
            ),
            ["AFS"]
        );
        // Only a name of initials and capitalized words is a person's: an
        // institution named after one, or a lone period, spares nothing.
        assert!(pack.public_body("ГБУ имени Н. О. Ветлина", Category::Person, false));
        assert!(pack.public_body("Суд .", Category::Person, false));
        assert!(!pack.public_body("Суд Н. О.", Category::Person, false));
        // A body's own address and phone are marked like anyone's (#553).
        let source = "Арбитражный суд Северной области: 190000, г. Ольховск, ул. Лесная, \
                      д. 1, тел. +7 (812) 555-01-01";
        assert_eq!(
            fixed(
                source,
                &[
                    ("190000, г. Ольховск, ул. Лесная, д. 1", Category::Address),
                    ("+7 (812) 555-01-01", Category::Phone),
                ]
            ),
            [
                "190000, г. Ольховск, ул. Лесная, д. 1",
                "+7 (812) 555-01-01"
            ]
        );
    }

    /// The second review of probe round 2 of #378: a sentence's end stops
    /// a citation, an uppercase surname keeps its initials, only a
    /// university or college itself is private, a body's word keeps all
    /// its case forms, and a grand jury and an agency's organ go with
    /// what follows them.
    #[test]
    fn second_review_of_public_bodies_round_two() {
        let pack = pack();
        let org = |text: &str| pack.identifying(text, Category::Organization);
        let person = |text: &str| pack.identifying(text, Category::Person);
        // A sentence's end is no citation, and neither is "Act." before
        // the next sentence's capitals: the company keeps its "AFS".
        for source in [
            "Ashmoor Freight Safety Act, 2026. ASHMOOR FREIGHT SERVICES \
             (hereinafter “AFS”). AFS wrote.",
            "Ashmoor Freight Safety Act. ASHMOOR FREIGHT SERVICES (hereinafter \
             “AFS”). AFS wrote.",
        ] {
            assert_eq!(
                fixed(source, &[("AFS", Category::Organization)]),
                ["AFS"],
                "{source}"
            );
        }
        // An uppercase surname with initials is a person; an institution
        // abbreviation with initials is not.
        assert_eq!(
            fixed("Подпись: СУД Н. О.", &[("СУД Н. О.", Category::Person)]),
            ["СУД Н. О."]
        );
        assert!(pack.public_body("ГБУ Н. О.", Category::Person, false));
        // Only a span that names a university or college itself is
        // private; the Electoral College is public.
        for text in [
            "Ministry of Colleges and Universities",
            "ГБУ «Университетская больница»",
            "Министерство науки и университетов",
            "Electoral College",
            "the Electoral College",
            "College of Policing",
        ] {
            assert!(!org(text), "{text}");
        }
        for text in [
            "ФГБОУ ВО «Северный государственный университет»",
            "Северного государственного университета",
            "University of Ashmoor",
            "Royal College of Surgeons",
            "Негосударственный пенсионный фонд «Кедр»",
        ] {
            assert!(org(text), "{text}");
        }
        // A body's word goes as a person in its case forms; a capitalized
        // surname with "-ов" stays.
        for text in [
            "Министерство финансов",
            "Министерства финансов",
            "Министерству финансов",
            "Правительство",
            "Правительства Северной области",
            "Правительством",
            "Управление",
            "Администрацией",
            "Прокуратуру",
            "Прокуратурою",
            "Росимущество",
            "Минстрой",
            "Законодательство",
            "Конституцию",
            "Кодексом",
        ] {
            assert!(!person(text), "{text}");
        }
        assert!(pack.public_body("судов общей юрисдикции", Category::Person, false));
        for text in ["Судов", "Иван Судов", "Законов", "Приставов"] {
            assert!(person(text), "{text}");
        }
        // A grand jury with its jurisdiction, and an agency's organ with
        // an adjective, go; private names stay.
        for text in [
            "Grand Jury of Ashmoor County",
            "the Grand Jury for the Southern District of Ashmoor",
            "NATO Parliamentary Assembly",
            "IMF Executive Board",
        ] {
            assert!(!org(text), "{text}");
        }
        for text in [
            "Grand Jury Games",
            "NATO Logistics",
            "Grand Jury Games of Ashmoor LLC",
        ] {
            assert!(org(text), "{text}");
        }
    }

    /// Probe round 2 of #378 in English: more agencies and their units,
    /// treaties, ordinances, rules and laws whose head is "Law" go, as an
    /// organization or a person; companies and people stay.
    #[test]
    fn english_agencies_treaties_and_laws_of_probe_round_two() {
        let pack = pack();
        let org = |text: &str| pack.identifying(text, Category::Organization);
        let person = |text: &str| pack.identifying(text, Category::Person);
        for text in [
            "Customs",
            "U.S. Customs",
            "Customs and Border Protection",
            "Northvale Constabulary",
            "Elko County Jail",
            "TEXARKANA DIVISION",
            "Utah Division of Wildlife Resources",
            "the Board",
            "Central Office",
            "Secretary of State for the Home Department",
            "Director of Public Prosecutions",
            "Grand Jury",
            "Ashmoor District Registry",
            "ICAO",
            "ICAO Council",
            "NASA",
            "the IMF",
            "NATO",
            "USFWS",
            "Court’s",
            "CAFTA-DR",
            "Environmental Cooperation Agreement",
            "Dunmere Trade Agreement",
            "Environment Chapter",
            "Brussels I",
            "Lugano Conventions",
            "Treaty of Rome",
            "Philadelphia Fair Practices Ordinance",
            "Federal Rules of Bankruptcy Procedure",
            "Tex. R. App. P.",
            "Ashmoor Executive Law",
            "New York State Criminal Procedure Law",
            "Debtor and Creditor Law",
            "the FDCPA",
            "TCPA",
            "FLSA",
            "SB 9",
        ] {
            assert!(!org(text), "{text}");
        }
        for text in [
            "BLM",
            "Customs",
            "Northvale Constabulary",
            "Grand Jury",
            "Brussels I",
        ] {
            assert!(!person(text), "{text}");
        }
        for text in [
            "Customs Brokers Inc",
            "Treaty Oak Capital LLC",
            "Treaty Oak Capital",
            "Ashmoor Convention Center",
            "Agreement Solutions LLC",
            "Brussels Airlines",
            "Quill Tax Law",
            "Board Street Partners LLP",
            "NASA Federal Credit Union",
        ] {
            assert!(org(text), "{text}");
        }
        for text in ["Nasa Quill", "Treaty Quill"] {
            assert!(person(text), "{text}");
        }
        // An abbreviation defined after "hereinafter", or after a citation
        // with commas, is the law wherever it stands.
        assert_eq!(
            fixed(
                "the Ashmoor Freight Safety Act, 12 A.S.C. § 41, et seq. (hereinafter, \
                 the “AFSA”) binds Quillon Freight LLC (“QF”). AFSA applies to QF.",
                &[
                    ("AFSA", Category::Organization),
                    ("QF", Category::Organization),
                ]
            ),
            ["QF"]
        );
    }

    /// Pronouns, common words, religions and nationalities name no one
    /// alone (#500, #492); with any other word they stay a name.
    #[test]
    fn common_words_alone_are_no_names() {
        let pack = pack();
        for (text, category) in [
            ("He", Category::Person),
            ("She", Category::Person),
            ("It", Category::Organization),
            ("We", Category::Organization),
            ("In", Category::Person),
            ("Others", Category::Person),
            ("Program", Category::Person),
            ("Programme", Category::Person),
            ("Judgment", Category::Person),
            ("Federal", Category::Person),
            ("Law School", Category::Organization),
            ("Christian", Category::Person),
            ("Christians", Category::Person),
            ("Islam", Category::Organization),
            ("Muslims", Category::Person),
            ("Americans", Category::Person),
            ("Maltese lawyer", Category::Person),
            ("American licensed drivers", Category::Person),
            ("United States Citizens", Category::Person),
            ("Citizenry", Category::Person),
            ("the French", Category::Person),
            ("New York", Category::Person),
            ("San Francisco", Category::Person),
            ("St. Louis", Category::Person),
            ("North Carolina", Category::Person),
            // An outline letter before a heading's common word, and whole
            // headings (review of #528).
            ("B. General Terms", Category::Person),
            ("Approved Judgment", Category::Person),
            ("American End Products", Category::Person),
        ] {
            assert!(!pack.identifying(text, category), "{text}");
        }
        for (text, category) in [
            ("General", Category::Person),
            ("B. General", Category::Person),
            ("He Jiankui", Category::Person),
            ("Jane Federal", Category::Person),
            ("Paul Program", Category::Person),
            ("John York", Category::Person),
            ("Sarah In", Category::Person),
            ("Christian Bale", Category::Person),
            ("Nazrul Islam", Category::Person),
            ("Dawn French", Category::Person),
            ("Georgia Quill", Category::Person),
            ("Virginia", Category::Person),
            ("Carolina", Category::Person),
            ("J. R. Clerk", Category::Person),
            ("J. R. French", Category::Person),
            ("J.R. French", Category::Person),
            ("B.", Category::Person),
            ("General Motors", Category::Organization),
            ("Harvard Law School", Category::Organization),
            ("British Airways", Category::Organization),
            ("Islamic Relief", Category::Organization),
            ("State Street", Category::Organization),
            ("New York Life", Category::Organization),
        ] {
            assert!(pack.identifying(text, category), "{text}");
        }
    }

    /// In context: a common word after a title, or one a person span in
    /// the document has beside a name, is that person's name (#500); the
    /// second word of a two-word place grows to the place and goes; a
    /// caption's "In re" comes off the party.
    #[test]
    fn common_words_in_context_and_place_tails_and_captions() {
        let source = "Mr He met Dr. General. He appeals. Dawn French and Sarah In \
                      wrote; French and In agree, Christian too. Mr Lane lives in New \
                      York, John York in San Jose. In re Ossian Draycott; In the \
                      Matter of Quill; Ex parte Vell; In re Quillon Freight LLC. The \
                      Federal Election Commission met; Federal agreed. Ms St. Louis \
                      signed in St. Louis.";
        let at = |anchor: &str, text: &str| {
            let start = source.find(anchor).unwrap() + anchor.find(text).unwrap();
            start..start + text.len()
        };
        let mut detections: Vec<Detection> = [
            (at("Mr He", "He"), Category::Person),
            (at("Dr. General", "Dr. General"), Category::Person),
            (at("He appeals", "He"), Category::Person),
            (at("Dawn French", "Dawn French"), Category::Person),
            (at("Sarah In", "Sarah In"), Category::Person),
            (at("French and In", "French"), Category::Person),
            (at("French and In", "In"), Category::Person),
            (at("Christian too", "Christian"), Category::Person),
            (at("New York", "York"), Category::Person),
            (at("John York", "John York"), Category::Person),
            (at("San Jose", "Jose"), Category::Person),
            (
                at("In re Ossian Draycott", "In re Ossian Draycott"),
                Category::Person,
            ),
            (
                at("In the Matter of Quill", "In the Matter of Quill"),
                Category::Person,
            ),
            (at("Ex parte Vell", "Ex parte Vell"), Category::Person),
            (
                at("In re Quillon Freight LLC", "In re Quillon Freight LLC"),
                Category::Organization,
            ),
            // A public body names no one, so its words stay common.
            (
                at("Federal Election Commission", "Federal Election Commission"),
                Category::Person,
            ),
            (at("Federal agreed", "Federal"), Category::Person),
            // A title makes a place a surname; the place itself goes.
            (at("Ms St. Louis", "St. Louis"), Category::Person),
            (at("in St. Louis", "St. Louis"), Category::Person),
        ]
        .into_iter()
        .map(|(range, category)| Detection {
            range,
            category,
            score: 0.9,
            recognizer: Recognizer::Model,
        })
        .collect();
        let pack = pack();
        pack.fix_spans(source, &mdoc_pii::view::View::new(source), &mut detections);
        let found: Vec<&str> = detections
            .iter()
            .map(|d| &source[d.range.clone()])
            .collect();
        assert_eq!(
            found,
            [
                "He",
                "General",
                "Dawn French",
                "Sarah In",
                "French",
                "In",
                "John York",
                "Ossian Draycott",
                "Quill",
                "Vell",
                "Quillon Freight LLC",
                "St. Louis",
            ]
        );
    }

    /// Every whole-word occurrence of each text in `spans`, as a model
    /// would tag them (earlier texts first, none inside another), after
    /// the span fixes.
    fn fixed_every(source: &str, spans: &[(&str, Category)]) -> Vec<String> {
        let mut detections: Vec<Detection> = Vec::new();
        for (text, category) in spans {
            for (start, _) in source.match_indices(text) {
                let before = source[..start].chars().next_back();
                let after = source[start + text.len()..].chars().next();
                let end = start + text.len();
                if before.is_some_and(char::is_alphanumeric)
                    || after.is_some_and(char::is_alphanumeric)
                    || detections
                        .iter()
                        .any(|d| d.range.start < end && start < d.range.end)
                {
                    continue;
                }
                detections.push(Detection {
                    range: start..start + text.len(),
                    category: *category,
                    score: 0.9,
                    recognizer: Recognizer::Model,
                });
            }
        }
        detections.sort_by_key(|d| d.range.start);
        pack().fix_spans(source, &mdoc_pii::view::View::new(source), &mut detections);
        detections
            .into_iter()
            .map(|d| source[d.range].to_owned())
            .collect()
    }

    /// The terms a document defines, in every form of definition (#390).
    #[test]
    fn definitions_are_found_in_brackets_after_hereinafter_and_in_bold() {
        for (source, terms) in [
            (
                "Larkfield LLC (the “Distributor”) sells",
                vec!["Distributor"],
            ),
            (
                "Larkfield LLC (the \"Distributor\") sells",
                vec!["Distributor"],
            ),
            ("the land (“Townhouse”) is let", vec!["Townhouse"]),
            (
                "Larkfield LLC (hereinafter referred to as the “Buyer”)",
                vec!["Buyer"],
            ),
            ("Larkfield LLC (hereinafter, “Buyer”)", vec!["Buyer"]),
            (
                "Larkfield LLC and Quillon LLC (collectively, the “Parties”)",
                vec!["Parties"],
            ),
            ("Larkfield LLC (each a “Party”)", vec!["Party"]),
            (
                "the Kew Project (the “Kew Project” or the “Project”)",
                vec!["Kew Project", "Project"],
            ),
            (
                "Larkfield LLC (hereinafter the Distributor)",
                vec!["Distributor"],
            ),
            (
                "**“Franchised Business”** means the shops",
                vec!["Franchised Business"],
            ),
            ("“Territory” has the meaning in clause 2", vec!["Territory"]),
            ("ООО «Север» (далее – «Общество»)", vec!["Общество"]),
            (
                "ООО «Север» (далее по тексту – Положение)",
                vec!["Положение"],
            ),
            (
                "ООО «Север», именуемое в дальнейшем «Поставщик», и",
                vec!["Поставщик"],
            ),
            // Not a definition: a reference, a plain bracket, a quotation.
            ("as stated (see “Exhibit A”)", vec![]),
            ("the network (Network) runs", vec![]),
            ("he said “Distributor” twice", vec![]),
        ] {
            assert_eq!(
                definitions(source)
                    .iter()
                    .map(|d| d.term)
                    .collect::<Vec<_>>(),
                terms,
                "{source}"
            );
        }
    }

    #[test]
    fn generic_vocabulary_keeps_a_companys_defined_short_name() {
        for (source, names, expected) in [
            (
                "Network International Ltd (“Network”). Network shall pay…",
                vec!["Network International Ltd", "Network"],
                vec!["Network International Ltd", "Network", "Network"],
            ),
            (
                "ООО «Контракт» (далее — «Контракт»). Контракт платит.",
                vec!["ООО «Контракт»", "Контракт"],
                vec!["ООО «Контракт»", "Контракт", "Контракт"],
            ),
            (
                "Larkfield LLC (the “Network”). Network International Ltd (“Network”). Network pays.",
                vec!["Network"],
                vec!["Network", "Network", "Network", "Network"],
            ),
            (
                "“Network” means Network International Ltd. Network pays.",
                vec!["Network International Ltd", "Network"],
                vec!["Network", "Network International Ltd", "Network"],
            ),
        ] {
            assert_eq!(
                fixed_every(
                    source,
                    &names
                        .into_iter()
                        .map(|name| (name, Category::Organization))
                        .collect::<Vec<_>>()
                ),
                expected,
                "{source}",
            );
        }
        // A role assigned to a different company is still generic.
        assert!(
            fixed_every(
                "Larkfield LLC (the “Network”). Network shall pay…",
                &[("Network", Category::Organization)],
            )
            .is_empty()
        );
    }

    #[test]
    fn wrapped_definitions_suppress_generic_organization_proposals() {
        for source in [
            "Larkfield LLC (the\n“Distributor”). The Distributor pays.",
            "Larkfield LLC (the\r\n“Distributor”). The Distributor pays.",
            "Larkfield LLC (hereinafter\nreferred to as the “Distributor”). The Distributor pays.",
            "Larkfield LLC (the “Franchised\nBusiness”). Franchised Business pays.",
            "Банки (далее\n— «Кредиторы»). Кредиторы платят.",
            "Quillon LLC (the\n“Distributor”). Distributor pays.",
            "ООО «Север»\n(далее —\n«Кредитор»). Кредитор платит.",
        ] {
            assert!(
                fixed_every(
                    source,
                    &[
                        ("Distributor", Category::Organization),
                        ("Franchised\nBusiness", Category::Organization),
                        ("Franchised Business", Category::Organization),
                        ("Кредиторы", Category::Organization),
                        ("Кредитор", Category::Organization),
                    ]
                )
                .is_empty(),
                "{source}"
            );
        }
        // Wrapping is bounded by the same 200-character bracket limit.
        let source = format!(
            "Larkfield LLC (the{}“Distributor”). The Distributor pays.",
            "\n".repeat(200)
        );
        assert_eq!(
            fixed_every(&source, &[("Distributor", Category::Organization)]),
            ["Distributor", "Distributor"]
        );
    }

    #[test]
    fn single_quoted_definitions_suppress_generic_organization_proposals() {
        for source in [
            "Larkfield Ltd (the ‘Distributor’). The Distributor pays.",
            "Larkfield Ltd (the 'Distributor'). The Distributor pays.",
            "‘Distributor’ means Larkfield Ltd. The Distributor pays.",
            "'Distributor' means Larkfield Ltd. The Distributor pays.",
            "**‘Distributor’** shall mean Larkfield Ltd. The Distributor pays.",
            "ООО «Север», именуемое в дальнейшем ‘Кредитор’. Кредитор платит.",
        ] {
            assert!(
                fixed_every(
                    source,
                    &[
                        ("Distributor", Category::Organization),
                        ("Кредитор", Category::Organization),
                    ]
                )
                .is_empty(),
                "{source}"
            );
        }
        // An unmatched quotation is insufficient evidence to drop a name.
        assert!(definitions("Larkfield Ltd (the ‘Distributor\").").is_empty());
        assert!(definitions("Larkfield Ltd (the 'Distributor’).").is_empty());
    }

    #[test]
    fn russian_defined_nouns_include_genitive_plurals_but_keep_people() {
        let source = "Банки (далее — «Кредиторы»). Согласие Кредиторов обязательно.";
        assert!(
            fixed_every(
                source,
                &[
                    ("Кредиторы", Category::Organization),
                    ("Кредиторов", Category::Organization),
                ]
            )
            .is_empty()
        );
        // The noun's case forms do not change the PERSON surname guards.
        let source = "Банки (далее — «Кредиторы»). Кредиторов И. П. и г-н Кредиторов согласны.";
        assert_eq!(
            fixed_every(source, &[("Кредиторов", Category::Person)]),
            ["Кредиторов И. П.", "Кредиторов"]
        );
        assert_eq!(
            fixed_every(
                "Банки (далее — «Кредиторы»). Кредиторин согласен.",
                &[("Кредиторин", Category::Organization)],
            ),
            ["Кредиторин"]
        );
        assert!(
            fixed_every(
                "Компании (далее — «Должники»). Согласие Должников обязательно.",
                &[
                    ("Должники", Category::Organization),
                    ("Должников", Category::Organization)
                ],
            )
            .is_empty()
        );
    }

    /// A term the document defines in generic words is no organization,
    /// wherever it stands, in any of its forms; the parties' names and a
    /// term with a coined word stay (#390).
    #[test]
    fn defined_terms_of_generic_words_are_no_organizations() {
        let source = "This agreement is between Larkfield Supply LLC (the “Distributor”) \
                      and Quillon Freight LLC (the “Sponsor”). The Distributor sells in \
                      the Territory (the “Territory”); the Sponsor’s Franchised Business \
                      (the “Franchised Business”) funds the Kew Project (the “Project”). \
                      The Project ends when the Distributor leaves the Territory. \
                      Quillon (the “Quillon Group”) agrees.";
        assert_eq!(
            fixed_every(
                source,
                &[
                    ("Larkfield Supply LLC", Category::Organization),
                    ("Quillon Freight LLC", Category::Organization),
                    ("Distributor", Category::Organization),
                    ("Sponsor’s", Category::Organization),
                    ("Sponsor", Category::Organization),
                    ("Territory", Category::Organization),
                    ("Franchised Business", Category::Organization),
                    ("Kew Project", Category::Organization),
                    ("Project", Category::Organization),
                    ("Quillon", Category::Organization),
                ]
            ),
            [
                "Larkfield Supply LLC",
                "Quillon Freight LLC",
                "Quillon",
                "Quillon"
            ]
        );
        // Bold quotes, other terms and the title made of them.
        let source = "RESELLER AGREEMENT\n\nLarkfield Supply LLC (the “Reseller”) buys from \
                      Quillon Freight LLC. The Reseller pays. **“Concessionaire”** means a \
                      shop; each Concessionaire reports to the Reseller. The Estate (the \
                      “Estate”) is let by the Reseller’s Affiliates.";
        assert_eq!(
            fixed_every(
                source,
                &[
                    ("RESELLER AGREEMENT", Category::Organization),
                    ("Larkfield Supply LLC", Category::Organization),
                    ("Quillon Freight LLC", Category::Organization),
                    ("Reseller", Category::Organization),
                    ("Concessionaire", Category::Organization),
                    ("Estate", Category::Organization),
                    ("Affiliates", Category::Organization),
                ]
            ),
            // "Affiliates" is a role word too (#385), so it goes as well.
            ["Larkfield Supply LLC", "Quillon Freight LLC"]
        );
        // The same words without a definition are proposed as before.
        assert_eq!(
            fixed_every(
                "The Distributor sells in the Territory.",
                &[("Distributor", Category::Organization)]
            ),
            ["Distributor"]
        );
        // A person's span is never dropped for it.
        assert_eq!(
            fixed_every(
                "Larkfield LLC (the “Estate”) pays Mr Estate.",
                &[("Estate", Category::Person)]
            ),
            ["Estate", "Estate"]
        );
    }

    /// A defined short form of a real name stays an organization: a coined
    /// word, a ticker, a surname firm, a company's own name; and a generic
    /// word with a name beside it or a legal form is a name (#390, #506).
    #[test]
    fn real_short_names_stay_when_defined() {
        let source = "Quillon Freight LLC (“Quillon”), Harbour Chase Bank Ltd (the “Chase”),                       Berkshire Hills Bancorp (“BHLB”), Twentieth Century Fox (“Fox”) and                       Vell & Ostrand LLP (the “Vell”) met Larkfield Supply LLC (the                       “Distributor”). Quillon, Chase, BHLB, Fox and Vell sued the                       Distributor. Distributor Quillon and Distributor LLC agreed.";
        let names = [
            "Distributor Quillon",
            "Distributor LLC",
            "Quillon Freight LLC",
            "Harbour Chase Bank Ltd",
            "Twentieth Century Fox",
            "Vell & Ostrand LLP",
            "Larkfield Supply LLC",
            "Quillon",
            "Chase",
            "BHLB",
            "Fox",
            "Vell",
            "Distributor",
        ];
        let found = fixed_every(source, &names.map(|name| (name, Category::Organization)));
        let count = |name: &str| found.iter().filter(|f| *f == name).count();
        // Each short form is proposed where it is defined and where it is
        // used; the defined generic word nowhere alone.
        for (name, times) in [
            ("Quillon Freight LLC", 1),
            ("Harbour Chase Bank Ltd", 1),
            ("Twentieth Century Fox", 1),
            ("Vell & Ostrand LLP", 1),
            ("Larkfield Supply LLC", 1),
            ("Distributor Quillon", 1),
            ("Distributor LLC", 1),
            ("Quillon", 2),
            ("Chase", 2),
            ("BHLB", 2),
            ("Fox", 2),
            ("Vell", 2),
            ("Distributor", 0),
        ] {
            assert_eq!(count(name), times, "{name}: {found:?}");
        }
    }

    /// The Mac hub's review of #580 (#390): a company's long form keeps its
    /// abbreviation, irregular plurals are their singular's term, listed
    /// companions go beside a defined term, uppercase possessives go.
    #[test]
    fn defined_terms_review_cases() {
        // A long form with a legal form, spelled out or abbreviated, is a
        // company's, never a filing's.
        for (source, short) in [
            ("Contract Corporation (“CC”). CC supplies goods.", "CC"),
            ("Contract Company (“CC”). CC supplies goods.", "CC"),
            ("Contract Incorporated (“CI”). CI supplies goods.", "CI"),
            (
                "Contract Holdings Limited (“CHL”). CHL supplies goods.",
                "CHL",
            ),
            ("Contract Corp (“CC”). CC supplies goods.", "CC"),
            (
                "Agreement Statement LLC (“ASL”). ASL supplies goods.",
                "ASL",
            ),
        ] {
            assert_eq!(
                fixed_every(source, &[(short, Category::Organization)]),
                [short, short],
                "{source}"
            );
        }
        // A company's short name defined from a spelled-out form stays.
        assert_eq!(
            fixed_every(
                "Network Corporation (“Network”). Network pays.",
                &[("Network", Category::Organization)]
            ),
            ["Network", "Network", "Network"]
        );
        // A filing still names no one.
        assert!(
            fixed_every(
                "Corporation Statement of Material Fact (“CSMF”). CSMF ¶ 2.",
                &[("CSMF", Category::Organization)]
            )
            .is_empty()
        );
        // Irregular plurals, both directions.
        for (singular, plural) in [
            ("Subsidiary", "Subsidiaries"),
            ("Territory", "Territories"),
            ("Business", "Businesses"),
        ] {
            for (defined, used) in [(singular, plural), (plural, singular)] {
                let source = format!("Larkfield LLC (the “{defined}”). The {used} pay.");
                assert!(
                    fixed_every(
                        &source,
                        &[
                            (defined, Category::Organization),
                            (used, Category::Organization)
                        ]
                    )
                    .is_empty(),
                    "{source}"
                );
            }
        }
        // Listed generic words go beside a defined one; a real word keeps
        // the span.
        assert_eq!(
            fixed_every(
                "Quillon LLC (the “Sponsor”). The Sponsor Business and Sponsor Quillon pay.",
                &[
                    ("Sponsor Business", Category::Organization),
                    ("Sponsor Quillon", Category::Organization),
                ]
            ),
            ["Sponsor Quillon"]
        );
        // Uppercase possessives, straight and curly.
        assert!(
            fixed_every(
                "Quillon LLC (the “Distributor”). DISTRIBUTOR’S and DISTRIBUTOR'S duties.",
                &[
                    ("DISTRIBUTOR’S", Category::Organization),
                    ("DISTRIBUTOR'S", Category::Organization),
                ]
            )
            .is_empty()
        );
    }

    /// A filing's abbreviation, defined after a long form of generic words,
    /// names no one; a company's does (#390).
    #[test]
    fn abbreviations_of_generic_filings_are_no_organizations() {
        let source = "Def.’s Statement of Uncontested Material Fact (“DSUMF”) ¶ 2 and \
                      Pl.’s Motion for Summary Judgment (“PMSJ”) ¶ 3. DSUMF ¶ 4; PMSJ. \
                      Quillon Freight LLC (“QF”) and Berkshire Hills Bancorp (“BHB”) \
                      met; QF and BHB agreed.";
        assert_eq!(
            fixed_every(
                source,
                &[
                    ("DSUMF", Category::Organization),
                    ("PMSJ", Category::Organization),
                    ("QF", Category::Organization),
                    ("BHB", Category::Organization),
                ]
            ),
            ["QF", "BHB", "QF", "BHB"]
        );
    }

    /// The Russian shape: «(далее – <Слово>)», «именуемое в дальнейшем
    /// «Поставщик»», any case form; a short name stays (#390).
    #[test]
    fn russian_defined_terms_are_no_organizations() {
        let source = "Утверждено Положение о закупках (далее – Положение); ООО «Кедровое» \
                      (далее – Учреждение). Учреждение возражает против Положения, \
                      Учреждению отказано. ООО «Север» (далее – «Север»), именуемое в \
                      дальнейшем «Займодавец», передало заём. Займодавца известили, \
                      Займодавцу выплатили; Север согласен.";
        assert_eq!(
            fixed_every(
                source,
                &[
                    ("Положение", Category::Organization),
                    ("Положения", Category::Organization),
                    ("Учреждение", Category::Organization),
                    ("Учреждению", Category::Organization),
                    ("Займодавец", Category::Organization),
                    ("Займодавца", Category::Organization),
                    ("Займодавцу", Category::Organization),
                    ("ООО «Кедровое»", Category::Organization),
                    ("ООО «Север»", Category::Organization),
                    ("Север", Category::Organization),
                ]
            ),
            ["ООО «Кедровое»", "ООО «Север»", "Север", "Север"]
        );
        // Not defined: a company's name with the same word stays.
        assert_eq!(
            fixed_every(
                "Положение утверждено ООО «Север».",
                &[("Положение", Category::Organization)]
            ),
            ["Положение"]
        );
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

    /// US and UK addresses from PDF and text layers (#498): the "City, ST
    /// ZIP" line under the street, suites, floors and buildings, compass
    /// parts, spelled-out states and capitals, whole from the first element
    /// to the postcode, never the country after it.
    #[test]
    fn us_and_uk_addresses_over_two_lines_are_found_whole() {
        let pack = pack();
        let found = |source: &str| -> Vec<String> {
            pack.spans(source)
                .into_iter()
                .filter(|d| d.category == Category::Address)
                .map(|d| source[d.range].to_owned())
                .collect()
        };
        // The issue's reproduction: two addresses, the country left out.
        assert_eq!(
            found(
                "Notices to Larkfield Supply LLC, 4410 W. Quarry Parkway, Suite 210\n\
                 Brindlecombe, OH 45999, and to Harrow House, 12 Pellin Street\n\
                 Slocombe SL9 9ZZ\nUK."
            ),
            [
                "4410 W. Quarry Parkway, Suite 210\nBrindlecombe, OH 45999",
                "Harrow House, 12 Pellin Street\nSlocombe SL9 9ZZ",
            ]
        );
        for (source, address) in [
            // A line break, with no comma, before the "City, ST ZIP" line;
            // capitals, a compass part, two spaces before the ZIP.
            (
                "Contractor:\n1800 S RIO VERDE AVENUE\nBRANWICK, CO  81999\nPhone",
                "1800 S RIO VERDE AVENUE\nBRANWICK, CO  81999",
            ),
            (
                "1625 L. Street, NW\r\nWexford, DC 20999-5687\r\n",
                "1625 L. Street, NW\r\nWexford, DC 20999-5687",
            ),
            (
                "Wrenfield LLP\n  1909 K St. N.W., Suite 600  \n  Wexford D.C. 20999\n",
                "1909 K St. N.W., Suite 600  \n  Wexford D.C. 20999",
            ),
            (
                "1023 15th Street, N.W., 10th Floor\nWexford, DC  20995",
                "1023 15th Street, N.W., 10th Floor\nWexford, DC  20995",
            ),
            // A Markdown hard break.
            (
                "12 Oak Road\\\nBrindlecombe, OH 45999",
                "12 Oak Road\\\nBrindlecombe, OH 45999",
            ),
            // Dotted compass parts and the street words #361 lacked.
            (
                "at 2121 Quillon Ave. N.W., Wexford, DC 20997 since",
                "2121 Quillon Ave. N.W., Wexford, DC 20997",
            ),
            (
                "at 8001 Harl Tansey Fwy., Brindlecombe, TX 75999.",
                "8001 Harl Tansey Fwy., Brindlecombe, TX 75999",
            ),
            (
                "at 6910 Network Pl., Slocombe, IN 46999.",
                "6910 Network Pl., Slocombe, IN 46999",
            ),
            (
                "at 2251 Quarry Blvd. SE, Bldg 499, Kestle AFB, NM 87999-2773 by",
                "2251 Quarry Blvd. SE, Bldg 499, Kestle AFB, NM 87999-2773",
            ),
            // A state spelled out or dotted before its ZIP.
            (
                "at 1600 Pellin Hill Road, Brindlecombe, Georgia 30999.",
                "1600 Pellin Hill Road, Brindlecombe, Georgia 30999",
            ),
            (
                "Room 640, 500 Larch Street, New York, N.Y. 10999.",
                "Room 640, 500 Larch Street, New York, N.Y. 10999",
            ),
            (
                "at 12 Oak Road, Slocombe, New Hampshire 03999.",
                "12 Oak Road, Slocombe, New Hampshire 03999",
            ),
            // A ZIP wrapped onto the next line; "Ft." before a town.
            (
                "at 151 Nob Hill Road, Suite 321, Ft. Brindle, FL\n33999 (you)",
                "151 Nob Hill Road, Suite 321, Ft. Brindle, FL\n33999",
            ),
            // A building name, on the street's line or its own, and a street
            // without a number after it.
            (
                "Harrow House\n12 Pellin Street\nSlocombe SL9 9ZZ\nEngland",
                "Harrow House\n12 Pellin Street\nSlocombe SL9 9ZZ",
            ),
            (
                "Brindle House, Brindle Road\nSlocombe SL9 7ZZ, UK",
                "Brindle House, Brindle Road\nSlocombe SL9 7ZZ",
            ),
            (
                "Brindle House\nBrindle Road\nSlocombe SL9 7ZZ",
                "Brindle House\nBrindle Road\nSlocombe SL9 7ZZ",
            ),
            (
                "Ardley Building, Suite 400\n1750 Quillon Avenue, NW\nWexford, DC 20996",
                "Ardley Building, Suite 400\n1750 Quillon Avenue, NW\nWexford, DC 20996",
            ),
            (
                "7th Floor\n31 St. Brin Avenue\nBrindlecombe, MA 02999",
                "7th Floor\n31 St. Brin Avenue\nBrindlecombe, MA 02999",
            ),
            // A street without a number before its postcode.
            (
                "Royal Courts of Justice\nStrand, Slocombe, WC2A 9ZZ\n",
                "Strand, Slocombe, WC2A 9ZZ",
            ),
            (
                "at One Pellin Place, Brindlecombe, GA 30999.",
                "One Pellin Place, Brindlecombe, GA 30999",
            ),
            // A country after the postcode stays out.
            (
                "12 Oak Road, Slocombe SL9 9ZZ, United Kingdom",
                "12 Oak Road, Slocombe SL9 9ZZ",
            ),
            // A surname that is also a building word is no building.
            ("Jane Chambers, 12 Oak Road, Leeds", "12 Oak Road, Leeds"),
            (
                "Brindle Medical Center\n3939 Harl Blvd.\nSlocombe, UT 84999",
                "3939 Harl Blvd.\nSlocombe, UT 84999",
            ),
            // A suite on its own line after the street.
            (
                "12 Oak Road\nSuite 210\nLeeds LS1 4AB\n",
                "12 Oak Road\nSuite 210\nLeeds LS1 4AB",
            ),
        ] {
            assert_eq!(found(source), [address], "{source}");
        }
        for source in [
            // After a line break a town needs its postcode.
            "12 Oak Road\nLeeds",
            "12 Oak Road\nJohn Smith AB will call",
            // No street: a building and a town, a court, a town and its
            // postcode alone (#506), a P.O. box (#506).
            "Brindle House, Leeds",
            "High Court, Slocombe WC2A 9ZZ",
            "BRANWICK, CO 81999",
            "Brindlecombe, OH 45999",
            "P.O. Box 36060, Brindlecombe, CA 94999",
            // A street without a number needs its postcode.
            "Brindle Road, Leeds",
            "Strand, Leeds",
        ] {
            assert!(found(source).is_empty(), "{source}");
        }
        // A building, room or mail-stop part is no company, bank account or
        // identity document; a company named with one of their words stays
        // a company.
        for (text, category) in [
            ("Bldg 499", Category::Organization),
            ("Suite 210", Category::Organization),
            ("10th Floor", Category::Organization),
            ("Suite 210, 3rd Floor", Category::Organization),
            ("Building 179A", Category::Organization),
            ("Mail Stop K9-24", Category::Organization),
            ("Room 2.104", Category::Bank),
            ("Building 179A", Category::Bank),
            ("Mail Stop K9-24", Category::Identity),
            ("Apt 4B", Category::Identity),
        ] {
            assert!(!pack.identifying(text, category), "{text}");
        }
        for name in [
            "Brindle Building Society",
            "Pellin House",
            "Harrow Mail Ltd",
            "Harrow Floor Services",
            "Suite Seven Studios",
        ] {
            assert!(pack.identifying(name, Category::Organization), "{name}");
        }
        // So a model's building part no longer costs the whole address.
        let source = "Building 179A, 12 Oak Road\nLeeds LS1 4AB\n";
        let start = source.find("Building").unwrap();
        let raw = [crate::RawEntity {
            start,
            end: start + "Building 179A".len(),
            label: "organization".into(),
            score: 0.9,
            text: "Building 179A".into(),
        }];
        let shown: Vec<_> = crate::recognition::proposals(source, &raw)
            .unwrap()
            .into_iter()
            .map(|span| (source[span.range].to_owned(), span.category))
            .collect();
        assert_eq!(
            shown,
            [(
                "Building 179A, 12 Oak Road\nLeeds LS1 4AB".to_owned(),
                "ADDRESS".to_owned()
            )]
        );
        // A model's address loses the country and the word "address" at
        // its edges.
        assert_eq!(
            fixed(
                "Harrow House, 12 Pellin Street\nSlocombe SL9 9ZZ\nUK.",
                &[(
                    "Harrow House, 12 Pellin Street\nSlocombe SL9 9ZZ\nUK",
                    Category::Address
                )]
            ),
            ["Harrow House, 12 Pellin Street\nSlocombe SL9 9ZZ"]
        );
        assert_eq!(
            fixed(
                "at 12 Pellin Street address.",
                &[("12 Pellin Street address", Category::Address)]
            ),
            ["12 Pellin Street"]
        );
        // A country of several words, too; "United" leading a building's
        // name goes, but the span pattern's match holds what is left.
        assert_eq!(
            fixed(
                "12 Oak Road, Slocombe SL9 9ZZ, United Kingdom",
                &[(
                    "12 Oak Road, Slocombe SL9 9ZZ, United Kingdom",
                    Category::Address
                )]
            ),
            ["12 Oak Road, Slocombe SL9 9ZZ"]
        );
        assert_eq!(
            fixed(
                "of 4 Oak Road, Brindlecombe, OH 45999, United States of America.",
                &[(
                    "of 4 Oak Road, Brindlecombe, OH 45999, United States of America",
                    Category::Address
                )]
            ),
            ["4 Oak Road, Brindlecombe, OH 45999"]
        );
        let source = "United House, 12 Pellin Street\nSlocombe SL9 9ZZ\n";
        let raw = [crate::RawEntity {
            start: 0,
            end: source.trim_end().len(),
            label: "address".into(),
            score: 0.9,
            text: source.trim_end().into(),
        }];
        let shown: Vec<_> = crate::recognition::proposals(source, &raw)
            .unwrap()
            .into_iter()
            .map(|span| source[span.range].to_owned())
            .collect();
        assert_eq!(shown, [source.trim_end()]);
    }

    /// #498: the review keeps an address the pattern found over its own
    /// lines with the sentence going on after it, but only when it ends in
    /// its number before a word apart (`mdoc_pii::view::wrapped_value`).
    #[test]
    fn an_address_over_lines_is_proposed_with_text_after_it() {
        let shown = |source: &str| -> Vec<String> {
            crate::recognition::proposals(source, &[])
                .unwrap()
                .into_iter()
                .filter(|span| span.category == "ADDRESS")
                .map(|span| source[span.range].to_owned())
                .collect()
        };
        let found = |source: &str| -> Vec<String> {
            pack()
                .spans(source)
                .into_iter()
                .filter(|d| d.category == Category::Address)
                .map(|d| source[d.range].to_owned())
                .collect()
        };
        assert_eq!(
            shown(
                "Notices to 4410 W. Quarry Parkway, Suite 210\nBrindlecombe, OH 45999, and to \
                 Harrow House, 12 Pellin Street\nSlocombe SL9 9ZZ\nUK."
            ),
            [
                "4410 W. Quarry Parkway, Suite 210\nBrindlecombe, OH 45999",
                "Harrow House, 12 Pellin Street\nSlocombe SL9 9ZZ",
            ]
        );
        // The pattern reads a next field's first word as a town, or stops
        // inside a room number: the review keeps neither (review of #508).
        let source = "Lives at 12 Oak Road,\nDate of birth: 14/03/1988.";
        assert_eq!(found(source), ["12 Oak Road,\nDate"]);
        assert!(shown(source).is_empty());
        let source = "Lives at 12 Oak Road\nRoom 2/3 with Ann.";
        assert_eq!(found(source), ["12 Oak Road\nRoom 2"]);
        assert!(shown(source).is_empty());
    }

    /// #498 on the real model: it reads the building's name before a house
    /// number as a company. That company is the address's own part, and the
    /// address stands whole; a company named like a building with no street
    /// after it, or not ending in a building word, stays a company.
    #[test]
    fn a_building_name_before_its_street_is_the_address_not_a_company() {
        let shown = |source: &str, model: &[(&str, &str)]| -> Vec<(String, String)> {
            let raw: Vec<_> = model
                .iter()
                .map(|(text, label)| {
                    let start = source.find(text).unwrap();
                    crate::RawEntity {
                        start,
                        end: start + text.len(),
                        label: (*label).into(),
                        score: 0.98,
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
        let pair = |text: &str, category: &str| (text.to_owned(), category.to_owned());
        // The fixture's snapshot: the building a company, the rest an
        // address with its country.
        assert_eq!(
            shown(
                "Notices to Larkfield Supply LLC and to Harrow House, 12 Pellin Street\n\
                 Slocombe SL9 9ZZ\nUK.\n",
                &[
                    ("Larkfield Supply LLC", "organization"),
                    ("Harrow House", "organization"),
                    ("12 Pellin Street\nSlocombe SL9 9ZZ\nUK", "address"),
                ]
            ),
            [
                pair("Larkfield Supply LLC", "ORG"),
                pair(
                    "Harrow House, 12 Pellin Street\nSlocombe SL9 9ZZ",
                    "ADDRESS"
                ),
            ]
        );
        // On its own line, or with a suite before the street; the model
        // finding no address at all.
        for (source, building, address) in [
            (
                "Harrow House\n12 Pellin Street\nSlocombe SL9 9ZZ\n",
                "Harrow House",
                "Harrow House\n12 Pellin Street\nSlocombe SL9 9ZZ",
            ),
            (
                "Ardley Building, Suite 400\n1750 Quillon Avenue, NW\nWexford, DC 20996\n",
                "Ardley Building",
                "Ardley Building, Suite 400\n1750 Quillon Avenue, NW\nWexford, DC 20996",
            ),
        ] {
            assert_eq!(
                shown(source, &[(building, "organization")]),
                [pair(address, "ADDRESS")],
                "{source}"
            );
        }
        // A company named like a building: with its legal form, with no
        // street after it, not ending in a building word, or before a
        // number that is no street.
        for (source, model, proposed) in [
            (
                "Harrow House Ltd agreed to pay.\n",
                &[("Harrow House Ltd", "organization")][..],
                &[("Harrow House Ltd", "ORG")][..],
            ),
            (
                "Harrow House agreed to pay.\n",
                &[("Harrow House", "organization")],
                &[("Harrow House", "ORG")],
            ),
            (
                "Notices to Harrow House Ltd, 12 Pellin Street\nSlocombe SL9 9ZZ\n",
                &[("Harrow House Ltd", "organization")],
                &[
                    ("Harrow House Ltd", "ORG"),
                    ("12 Pellin Street\nSlocombe SL9 9ZZ", "ADDRESS"),
                ],
            ),
            (
                "Notices to Tower Records, 12 Pellin Street\nSlocombe SL9 9ZZ\n",
                &[("Tower Records", "organization")],
                &[
                    ("Tower Records", "ORG"),
                    ("12 Pellin Street\nSlocombe SL9 9ZZ", "ADDRESS"),
                ],
            ),
            (
                "Harrow House, 12 March 2026, Slocombe.\n",
                &[("Harrow House", "organization")],
                &[("Harrow House", "ORG")],
            ),
            // A number after the match is none of it (review of #508): the
            // pattern's "12 Oak Road, Chatham House" gives way as before.
            (
                "Lives at 12 Oak Road, Chatham House, 12 March 2026.\n",
                &[("Chatham House", "organization")],
                &[("Chatham House", "ORG")],
            ),
            // An address the review turns down (text after its town on its
            // last line) leaves the company standing (review of #508).
            (
                "Chatham House\n10 St James's Square, London, UK.\n",
                &[("Chatham House", "organization")],
                &[("Chatham House", "ORG")],
            ),
        ] {
            let proposed: Vec<_> = proposed.iter().map(|(t, c)| pair(t, c)).collect();
            assert_eq!(shown(source, model), proposed, "{source}");
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
            ("EIN 98-7654321", "tax_id"),
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

    /// [`shown`] for spans each at the `n`th place of its text (from 0).
    fn shown_at(source: &str, raw: &[(&str, usize, &str)]) -> Vec<(String, String)> {
        let raw: Vec<crate::RawEntity> = raw
            .iter()
            .map(|(text, n, label)| {
                let (start, _) = source
                    .match_indices(text)
                    .nth(*n)
                    .unwrap_or_else(|| panic!("{text:?} #{n} not in the source"));
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

    /// A cited case's parties are a legal reference, both sides, "In re"
    /// too, and so is its reporter short cite; a party of the case at hand
    /// stays marked in a citation (guide rule 8, #506 item 2, issue #561).
    #[test]
    fn cited_cases_parties_are_references_and_the_case_at_hands_stay() {
        // The issue's reproduction.
        let source = "Pellam v. Ostrand, 912 F.3d 455 (9th Cir. 2031), and In re Draycott, \
            77 B.R. 310, apply. Plaintiff Ivo Harrow cites Harrow v. Quillon Ltd, 2031 WL \
            1234567.";
        let raw = [
            ("Pellam", 0, "organization"),
            ("Ostrand", 0, "person"),
            ("Draycott", 0, "person"),
            ("Ivo Harrow", 0, "person"),
            ("Harrow", 1, "person"),
            ("Quillon Ltd", 0, "organization"),
        ];
        assert_eq!(
            shown_at(source, &raw),
            pairs(&[("Ivo Harrow", "PERSON"), ("Harrow", "PERSON")])
        );
        // Both sides in one span, "Ex parte", a legal form after a comma,
        // English reports and a precedent's docket number with its court.
        let source = "Ivo Harrow v. Quillon Ltd\n\nSee Easter v. Powell, 467 F.3d 459 (5th Cir. 2031); Ex parte Murrin \
            Bros. 1885, Ltd, 12 So. 3d 4; News Group Newspapers Ltd v Brindle [2031] EWHC 12 \
            (Ch); Grant v. Finecy, Case No. 4:20-cv-01234 (N.D. Tex. 2031).";
        let raw = [
            ("Easter v. Powell", 0, "person"),
            ("Murrin Bros. 1885, Ltd", 0, "organization"),
            ("News Group Newspapers Ltd", 0, "organization"),
            ("Brindle", 0, "person"),
            ("Grant", 0, "person"),
            ("Finecy", 0, "organization"),
        ];
        assert_eq!(shown_at(source, &raw), []);
        // The caption names the case at hand's parties: its earlier
        // decision's citation keeps them and its case number, a precedent
        // loses its parties, and the caption's own case number stays.
        let source = "IVO HARROW v. QUILLON LTD, Case No. 1:31-cv-00412\n\nThe claimant \
            relies on Harrow v. Quillon Ltd, Case No. 1:30-cv-00977 (N.D. Cal. 2030), and on \
            Pellam v. Ostrand Holdings Ltd, 2031 WL 1234567.";
        let raw = [
            ("IVO HARROW", 0, "person"),
            ("QUILLON LTD", 0, "organization"),
            ("Harrow", 0, "person"),
            ("Quillon Ltd", 0, "organization"),
            ("Pellam", 0, "person"),
            ("Ostrand Holdings Ltd", 0, "organization"),
        ];
        assert_eq!(
            shown_at(source, &raw),
            pairs(&[
                ("IVO HARROW", "PERSON"),
                ("QUILLON LTD", "ORG"),
                ("1:31-cv-00412", "CASE"),
                ("Harrow", "PERSON"),
                ("Quillon Ltd", "ORG"),
                ("1:30-cv-00977", "CASE"),
            ])
        );
        // A party named only after its role is the case at hand's too; a
        // short citation's name goes like the full one's.
        let source = "The Defendant, Mr Quill, relies on Quill v. Ostrand, 912 F.3d 455, and \
            Pellam, 912 F.3d at 460; Pellam, 2031 WL 1234567, at *3, agrees.";
        let raw = [
            ("Quill", 0, "person"),
            ("Quill", 1, "person"),
            ("Ostrand", 0, "person"),
            ("Pellam", 0, "person"),
            ("Pellam", 1, "person"),
        ];
        assert_eq!(
            shown_at(source, &raw),
            pairs(&[("Quill", "PERSON"), ("Quill", "PERSON")])
        );
        // A name with no citation after it is no cited case's.
        let source = "Ivo Harrow v. Quillon Ltd was heard on 3 May.";
        assert_eq!(
            shown(
                source,
                &[("Ivo Harrow", "person"), ("Quillon Ltd", "organization")]
            ),
            pairs(&[("Ivo Harrow", "PERSON"), ("Quillon Ltd", "ORG")])
        );
    }

    /// "Id." and "Ibid." cite authority without naming anyone (#561).
    #[test]
    fn id_and_ibid_are_references() {
        assert_eq!(
            shown("Id. Ibid.", &[("Id", "person"), ("Ibid", "person")]),
            []
        );
    }

    /// The case at hand's people stay beside a cited case: a sentence's end
    /// is no party's, a docket number's citation ends with its court, a
    /// title or a role names the case at hand's party, emphasis included,
    /// and the caption below a citation in a heading is still the caption
    /// (#561).
    #[test]
    fn cited_cases_leave_the_case_at_hands_people() {
        /// A source, the model's spans and what is shown.
        type Case<'a> = (
            &'a str,
            &'a [(&'a str, usize, &'a str)],
            &'a [(&'a str, &'a str)],
        );
        let cases: [Case; 6] = [
            (
                "The witness was Ivo Harrow. Pellam v. Ostrand, 912 F.3d 455, controls.",
                &[("Ivo Harrow", 0, "person"), ("Pellam", 0, "person")],
                &[("Ivo Harrow", "PERSON")],
            ),
            (
                "In Pellam v. Ostrand, No. 22-1234, counsel was Ivo Harrow (admitted 2020).",
                &[("Ivo Harrow", 0, "person"), ("Ostrand", 0, "person")],
                &[("Ostrand", "PERSON"), ("Ivo Harrow", "PERSON")],
            ),
            (
                "Mr Pellam signed the agreement. Pellam v. Ostrand, 912 F.3d 455, controls.",
                &[
                    ("Pellam", 0, "person"),
                    ("Pellam", 1, "person"),
                    ("Ostrand", 0, "person"),
                ],
                &[("Pellam", "PERSON"), ("Pellam", "PERSON")],
            ),
            (
                "Plaintiff Ivo Harrow v. Defendant Quillon Ltd, Case No. 1:31-cv-00412 (N.D. \
                 Cal. 2031).",
                &[
                    ("Ivo Harrow", 0, "person"),
                    ("Quillon Ltd", 0, "organization"),
                ],
                &[
                    ("Ivo Harrow", "PERSON"),
                    ("Quillon Ltd", "ORG"),
                    ("1:31-cv-00412", "CASE"),
                ],
            ),
            (
                "Plaintiff _Pellam_ signed. Pellam v. Ostrand, 912 F.3d 455, controls.",
                &[
                    ("Pellam", 0, "person"),
                    ("Pellam", 1, "person"),
                    ("Ostrand", 0, "person"),
                ],
                &[("Pellam", "PERSON"), ("Pellam", "PERSON")],
            ),
            (
                "# Motion under Pellam v. Ostrand, 912 F.3d 455\n\n## Pellam v. Quillon Ltd\n",
                &[
                    ("Pellam", 0, "person"),
                    ("Ostrand", 0, "person"),
                    ("Pellam", 1, "person"),
                    ("Quillon Ltd", 0, "organization"),
                ],
                &[
                    ("Pellam", "PERSON"),
                    ("Pellam", "PERSON"),
                    ("Quillon Ltd", "ORG"),
                ],
            ),
        ];
        for (source, raw, expected) in cases {
            assert_eq!(shown_at(source, raw), pairs(expected), "{source:?}");
        }
        // A cited case in emphasis is one too.
        assert_eq!(
            shown(
                "*Pellam v. Ostrand*, 912 F.3d 455, controls; *Pellam*, 912 F.3d at 460.",
                &[("Pellam", "person"), ("Ostrand", "person")]
            ),
            []
        );
    }

    /// Where nothing settles that a name is only a precedent's, it stays: a
    /// legal form's period ends a party, a caption with its own case number
    /// and court is the caption wherever it stands, a full name in prose is
    /// the case at hand's, the caption's words count undetected, and a title
    /// counts through emphasis (#561, second review).
    #[test]
    fn uncertain_cited_names_stay() {
        let preamble = "Counsel line\n".repeat(30);
        let late_caption =
            format!("{preamble}Pellam v. Quillon Ltd\n\nSee Pellam v. Ostrand, 912 F.3d 455.\n");
        type Case<'a> = (
            &'a str,
            &'a [(&'a str, usize, &'a str)],
            &'a [(&'a str, &'a str)],
        );
        let cases: [Case; 6] = [
            (
                "The supplier was Quillon Ltd. Pellam v. Ostrand, 912 F.3d 455, controls.",
                &[("Quillon Ltd", 0, "organization"), ("Pellam", 0, "person")],
                &[("Quillon Ltd", "ORG")],
            ),
            (
                "Ivo Harrow v. Quillon Ltd, Case No. 1:31-cv-00412 (N.D. Cal. 2031).\n\nThe \
                 court considers the motion.",
                &[
                    ("Ivo Harrow", 0, "person"),
                    ("Quillon Ltd", 0, "organization"),
                ],
                &[
                    ("Ivo Harrow", "PERSON"),
                    ("Quillon Ltd", "ORG"),
                    ("1:31-cv-00412", "CASE"),
                ],
            ),
            (
                "Our client Ivo Pellam signed the agreement. Ivo Pellam v. Ostrand, 912 F.3d \
                 455, is his earlier case.",
                &[
                    ("Ivo Pellam", 0, "person"),
                    ("Ivo Pellam", 1, "person"),
                    ("Ostrand", 0, "person"),
                ],
                &[("Ivo Pellam", "PERSON"), ("Ivo Pellam", "PERSON")],
            ),
            (
                "Ivo Harrow v. Quillon Ltd\n\nEarlier: Harrow v. Quillon Ltd, 2031 WL 1234567.",
                &[
                    ("Ivo Harrow", 0, "person"),
                    ("Harrow", 1, "person"),
                    ("Quillon Ltd", 1, "organization"),
                ],
                &[
                    ("Ivo Harrow", "PERSON"),
                    ("Quillon Ltd", "ORG"),
                    ("Harrow", "PERSON"),
                    ("Quillon Ltd", "ORG"),
                ],
            ),
            (
                &late_caption,
                &[("Pellam", 0, "person"), ("Pellam", 1, "person")],
                &[("Pellam", "PERSON"), ("Pellam", "PERSON")],
            ),
            (
                "Mr *Pellam* signed the agreement. Pellam v. Ostrand, 912 F.3d 455, controls.",
                &[
                    ("Pellam", 0, "person"),
                    ("Pellam", 1, "person"),
                    ("Ostrand", 0, "person"),
                ],
                &[("Pellam", "PERSON"), ("Pellam", "PERSON")],
            ),
        ];
        for (source, raw, expected) in cases {
            assert_eq!(shown_at(source, raw), pairs(expected), "{source:?}");
        }
    }

    /// A suffix or abbreviation can end the previous sentence (#561, r4).
    #[test]
    fn cited_parties_start_after_the_last_sentence_boundary() {
        for suffix in ["Jr.", "Sr.", "II.", "III.", "Gen.", "J."] {
            let name = format!("Ivo Harrow {suffix}");
            for citation in [
                "Pellam v. Ostrand, 912 F.3d 455, controls.",
                "Pellam, 912 F.3d at 460, controls.",
            ] {
                let source = format!("The witness was {name} {citation}");
                let cases = pack().cited(&source);
                assert_eq!(cases.len(), 1, "{source}");
                assert_eq!(cases[0].range.start, source.find("Pellam").unwrap());
                assert_eq!(
                    shown_at(
                        &source,
                        &[("Ivo Harrow", 0, "person"), ("Pellam", 0, "person")]
                    ),
                    pairs(&[("Ivo Harrow", "PERSON")]),
                    "{source}"
                );
            }
        }
        // The exact reported example keeps the model's suffix too.
        let source = "The witness was Ivo Harrow Jr. Pellam v. Ostrand, 912 F.3d 455, controls.";
        assert_eq!(
            shown(
                source,
                &[("Ivo Harrow Jr.", "person"), ("Pellam", "person")]
            ),
            pairs(&[("Ivo Harrow Jr.", "PERSON")])
        );
        // A boundary inside the other party or after "In re" cannot
        // establish a full citation, so its preceding name stays too.
        for source in [
            "Pellam v. Ivo Harrow Jr. Ostrand, 912 F.3d 455, controls.",
            "In re Ivo Harrow Jr. Pellam, 912 F.3d 455, controls.",
        ] {
            assert_eq!(
                shown(source, &[("Ivo Harrow", "person")]),
                pairs(&[("Ivo Harrow", "PERSON")]),
                "{source}"
            );
        }
    }

    /// Unicode spaces and line breaks cannot hide a sentence boundary (r5).
    #[test]
    fn cited_sentence_boundaries_accept_unicode_whitespace() {
        for space in [
            " ", "\u{00a0}", "\u{1680}", "\u{2000}", "\u{2001}", "\u{2002}", "\u{2003}",
            "\u{2004}", "\u{2005}", "\u{2006}", "\u{2007}", "\u{2008}", "\u{2009}", "\u{200a}",
            "\u{202f}", "\u{205f}", "\u{3000}", "\t", "\n", "\r\n",
        ] {
            for citation in [
                "Pellam v. Ostrand, 912 F.3d 455, controls.",
                "Pellam, 912 F.3d at 460, controls.",
            ] {
                let source = format!("The witness was Ivo Harrow Jr.{space}{citation}");
                assert_eq!(
                    shown(&source, &[("Ivo Harrow", "person"), ("Pellam", "person")]),
                    pairs(&[("Ivo Harrow", "PERSON")]),
                    "{source:?}"
                );
            }
        }
        let source = "The witness was Ivo Harrow Jr.\u{00a0}Pellam v. Ostrand, 912 F.3d 455.";
        assert_eq!(
            shown(source, &[("Ivo Harrow", "person")]),
            pairs(&[("Ivo Harrow", "PERSON")])
        );
    }

    /// Attribution words separate the preparer from a cited party (r5).
    #[test]
    fn cited_parties_do_not_absorb_attributions() {
        for connector in ["for", "by", "to", "with", "from"] {
            for citation in [
                "Pellam v. Ostrand, 912 F.3d 455.",
                "Pellam, 912 F.3d at 460.",
            ] {
                let source = format!("Brief prepared by Ivo Harrow {connector} {citation}");
                let cases = pack().cited(&source);
                assert_eq!(cases.len(), 1, "{source}");
                assert_eq!(cases[0].range.start, source.find("Pellam").unwrap());
                assert_eq!(
                    shown(&source, &[("Ivo Harrow", "person"), ("Pellam", "person")]),
                    pairs(&[("Ivo Harrow", "PERSON")]),
                    "{source}"
                );
            }
        }
    }

    /// Even an overbroad rule pack cannot discard a model's name prefix.
    #[test]
    fn uncertain_party_prefixes_stay_even_with_a_permissive_pattern() {
        let pack = Pack::load(&[(
            "test.toml",
            r"[[reference]]
pattern = '(?P<party>Ivo Harrow (?:for|by|to|with|from|of) Pellam) v\. Ostrand, 912 F\.3d 455'",
        )])
        .unwrap();
        for connector in ["for", "by", "to", "with", "from", "of"] {
            let source = format!(
                "Brief prepared by Ivo Harrow {connector} Pellam v. Ostrand, 912 F.3d 455."
            );
            let start = source.find("Ivo Harrow").unwrap();
            let mut detections = vec![Detection {
                range: start..start + "Ivo Harrow".len(),
                category: Category::Person,
                score: 0.8,
                recognizer: Recognizer::Model,
            }];
            pack.cited_cases(&mdoc_pii::view::View::new(&source), &mut detections);
            assert_eq!(detections.len(), usize::from(connector != "of"), "{source}");
        }
    }

    /// A caption protects both parties when a party or reporter wraps (r7).
    #[test]
    fn wrapped_citation_bearing_captions_keep_both_parties() {
        for caption in [
            "Ivo Harrow v.\nQuillon Ltd, 912 F.3d 455 (9th Cir. 2031)",
            "Ivo Harrow\nv. Quillon Ltd, 912 F.3d 455 (9th Cir. 2031)",
            "Ivo Harrow\nv.\nQuillon Ltd, 912 F.3d 455 (9th Cir. 2031)",
            "Ivo Harrow v. Quillon Ltd\n[2024] EWHC 12 (Ch)",
        ] {
            let source = format!("{caption}\n\nOpinion");
            assert_eq!(
                shown(
                    &source,
                    &[("Ivo Harrow", "person"), ("Quillon Ltd", "organization")]
                ),
                pairs(&[("Ivo Harrow", "PERSON"), ("Quillon Ltd", "ORG")]),
                "{source}"
            );
        }
    }

    /// Caption discovery accepts all of the citation rule's introducers (r7).
    #[test]
    fn citation_bearing_captions_accept_every_introducer() {
        for introducer in ["Ex parte", "Matter of", "In the Matter of", "In re"] {
            let source = format!("{introducer} Ivo Harrow, 12 So. 3d 4 (Ala. 2024)\n\nOpinion");
            assert_eq!(
                shown(&source, &[("Ivo Harrow", "person")]),
                pairs(&[("Ivo Harrow", "PERSON")]),
                "{source}"
            );
        }
        for introducer in ["v.", "vs."] {
            let source =
                format!("Ivo Harrow {introducer} Quillon Ltd, 12 So. 3d 4 (Ala. 2024)\n\nOpinion");
            assert_eq!(
                shown(
                    &source,
                    &[("Ivo Harrow", "person"), ("Quillon Ltd", "organization")]
                ),
                pairs(&[("Ivo Harrow", "PERSON"), ("Quillon Ltd", "ORG")]),
                "{source}"
            );
        }
    }

    /// Prose identifying the decision under review protects all its parties (r7).
    #[test]
    fn decisions_under_review_keep_the_case_at_hands_parties() {
        let parties = &[("Ivo Harrow", "person"), ("Quillon Ltd", "organization")];
        let expected = pairs(&[("Ivo Harrow", "PERSON"), ("Quillon Ltd", "ORG")]);
        let citation = "Ivo Harrow v. Quillon Ltd\n[2024] EWHC 12 (Ch).";
        let source = format!("The judgment under appeal is {citation}");
        assert_eq!(shown(&source, parties), expected, "{source}");
        for phrase in [
            "under appeal",
            "appealed from",
            "appeal from",
            "the decision below",
            "the judgment below",
            "the order below",
            "on appeal from",
            "the court below",
            "this appeal",
            "the present case",
            "the instant case",
            "this case",
            "this action",
        ] {
            for phrase in [phrase.to_owned(), phrase.to_uppercase()] {
                let source = format!("We identify {phrase} as {citation}");
                assert_eq!(shown(&source, parties), expected, "{source}");
            }
        }
        // The window counts characters, including multibyte ones, and
        // includes a phrase starting exactly 60 characters before the case.
        let source = format!("under appeal{} {citation}", "é".repeat(47));
        assert_eq!(shown(&source, parties), expected, "{source}");
        // A phrase outside that window does not establish the case at hand.
        let source = format!("under appeal{} {citation}", "é".repeat(48));
        assert_eq!(shown(&source, parties), [], "{source}");
    }

    /// A document's citation-bearing caption protects its parties everywhere.
    #[test]
    fn citation_bearing_captions_keep_the_case_at_hands_parties() {
        for caption in [
            "# Ivo Harrow v. Quillon Ltd [2031] EWHC 12 (Ch)",
            "Ivo Harrow v. Quillon Ltd, 912 F.3d 455 (9th Cir. 2031)",
            "**Ivo Harrow v. Quillon Ltd [2031] EWHC 12 (Ch)**",
            "# *Ivo Harrow v. Quillon Ltd*, 912 F.3d 455",
        ] {
            let source =
                format!("{caption}\n\nJudgment\n\nEarlier: Harrow v. Quillon Ltd, 912 F.3d 455.");
            assert_eq!(
                shown_at(
                    &source,
                    &[
                        ("Ivo Harrow", 0, "person"),
                        ("Quillon Ltd", 0, "organization"),
                        ("Harrow", 1, "person"),
                        ("Quillon Ltd", 1, "organization"),
                    ]
                ),
                pairs(&[
                    ("Ivo Harrow", "PERSON"),
                    ("Quillon Ltd", "ORG"),
                    ("Harrow", "PERSON"),
                    ("Quillon Ltd", "ORG"),
                ]),
                "{source}"
            );
        }
        // A standalone caption needs no judgment label or detected parties.
        let source =
            "**In re Quillon Ltd**, 77 B.R. 310\n\nEarlier: In re Quillon Ltd, 77 B.R. 310.";
        assert_eq!(
            shown_at(source, &[("Quillon Ltd", 1, "organization")]),
            pairs(&[("Quillon Ltd", "ORG"), ("Quillon Ltd", "ORG")])
        );
        for label in [
            "Judgment",
            "JUDGMENT",
            "Approved Judgment",
            "Opinion",
            "OPINION",
            "Order",
            "ORDER",
            "Before: Judge Quill",
        ] {
            // The label is the third nonempty line after a prose-wrapped
            // caption, which cannot qualify by standing alone.
            let source = format!(
                "Case of Ivo Harrow v. Quillon Ltd [2031] EWHC 12 (Ch), heard today\n\nCourt\n\nCivil Division\n\n{label}"
            );
            assert_eq!(
                shown(
                    &source,
                    &[("Ivo Harrow", "person"), ("Quillon Ltd", "organization")]
                ),
                pairs(&[("Ivo Harrow", "PERSON"), ("Quillon Ltd", "ORG")]),
                "{source}"
            );
        }
        // A label beyond the third nonempty line does not establish a caption.
        let source = "Motion under Ivo Harrow v. Quillon Ltd [2031] EWHC 12 (Ch), cited\nOne\nTwo\nThree\nJudgment";
        assert_eq!(shown(source, &[("Ivo Harrow", "person")]), []);
    }

    /// A docket with court/year does not disqualify the caption, even
    /// after a reporter precedent, and "In re" is a caption too (#561, r4).
    #[test]
    fn docket_only_captions_follow_reporter_precedents() {
        for precedent in [
            "Motion under Pellam v. Ostrand, 912 F.3d 455",
            "Motion under In re Draycott, 77 B.R. 310",
            "Motion under Pellam v. Ostrand, No. 22-5678, 2031 WL 1234567",
        ] {
            for caption in [
                "Ivo Harrow v. Quillon Ltd, Case No. 1:31-cv-00412 (N.D. Cal. 2031).",
                "Ivo Harrow vs. Quillon Ltd, Case No. 1:31-cv-00412 (N.D. Cal. 2031).",
                "In re Quillon Ltd, Case No. 22-1234 (Bankr. D. Mass. 2031).",
                "**In re Quillon Ltd**, Case No. 22-1234 (Bankr. D. Mass. 2031).",
                "In re Quillon Ltd",
            ] {
                let source = format!("{precedent}\n\n{caption}");
                let mut raw = vec![("Quillon Ltd", 0, "organization")];
                if caption.starts_with("Ivo") {
                    raw.push(("Ivo Harrow", 0, "person"));
                }
                if precedent.contains("Pellam") {
                    raw.push(("Pellam", 0, "person"));
                    raw.push(("Ostrand", 0, "person"));
                } else {
                    raw.push(("Draycott", 0, "person"));
                }
                let mut expected = Vec::new();
                if caption.starts_with("Ivo") {
                    expected.push(("Ivo Harrow", "PERSON"));
                }
                expected.push(("Quillon Ltd", "ORG"));
                if caption.contains("1:31") {
                    expected.push(("1:31-cv-00412", "CASE"));
                } else if caption.contains("22-1234") {
                    expected.push(("22-1234", "CASE"));
                }
                assert_eq!(shown_at(&source, &raw), pairs(&expected), "{source}");
            }
        }
        // A wrapped caption still protects undetected party words.
        let source = "Ivo Harrow\nv.\nQuillon Ltd\n\nEarlier: Harrow v. Quillon Ltd, 912 F.3d 455.";
        assert_eq!(
            shown_at(
                source,
                &[("Harrow", 1, "person"), ("Quillon Ltd", 1, "organization")]
            ),
            pairs(&[
                ("Harrow", "PERSON"),
                ("Quillon Ltd", "ORG"),
                ("Harrow", "PERSON"),
                ("Quillon Ltd", "ORG"),
            ])
        );
        // Undetected caption words protect a name in an earlier decision.
        let source = "In re Quillon Ltd\n\nEarlier: In re Quillon Ltd, 77 B.R. 310.";
        assert_eq!(
            shown_at(source, &[("Quillon Ltd", 1, "organization")]),
            pairs(&[("Quillon Ltd", "ORG"), ("Quillon Ltd", "ORG")])
        );
    }

    #[test]
    fn entrepreneur_prefix_trim_preserves_the_surname_ip() {
        for source in [
            "Свидетель Ип Ман подписал заявление",
            "Свидетель Ип Ман дал показания.",
        ] {
            assert_eq!(
                shown(source, &[("Ип Ман", "person")]),
                pairs(&[("Ип Ман", "PERSON")]),
                "{source}"
            );
        }
        // Only the uppercase status at the start comes off. A surname at
        // the end and a name after the status must both survive.
        for (source, name) in [
            ("ИП Ип Ман", "Ип Ман"),
            ("ИП Ман Ип", "Ман Ип"),
            ("Ман ИП", "Ман ИП"),
            // A role phrase at the end comes off too (#385).
            ("Ман индивидуальный предприниматель", "Ман"),
            ("ИП Ветлин", "Ветлин"),
            ("индивидуального предпринимателя Ип Ман", "Ип Ман"),
            ("ИНДИВИДУАЛЬНЫЙ ПРЕДПРИНИМАТЕЛЬ Ип Ман", "Ип Ман"),
            ("от ИП Ип Ман", "Ип Ман"),
            ("от Ип Ман", "от Ип Ман"),
        ] {
            let raw = crate::RawEntity {
                start: 0,
                end: source.len(),
                label: "person".into(),
                score: 0.8,
                text: source.into(),
            };
            let corrected: Vec<_> = crate::correct(source, &[raw])
                .into_iter()
                .map(|d| (source[d.range].to_owned(), d.category.token().to_owned()))
                .collect();
            assert_eq!(corrected, pairs(&[(name, "PERSON")]), "{source}");
        }
    }

    #[test]
    fn entrepreneur_name_only_org_yields_to_person() {
        for (source, name) in [
            ("ИП Ветлин Н. О.", "Ветлин Н. О."),
            (
                "индивидуальный предприниматель Ветлин Н. О.",
                "Ветлин Н. О.",
            ),
            (
                "индивидуальному предпринимателю Ветлину Н.О.",
                "Ветлину Н.О.",
            ),
            (
                "индивидуальный предприниматель Брюн Лидия Игоревна",
                "Брюн Лидия Игоревна",
            ),
        ] {
            assert_eq!(
                shown(source, &[(name, "organization")]),
                pairs(&[(name, "PERSON")]),
                "{source}"
            );
        }
        // With no entrepreneur status, this rule has no evidence to
        // override an organization. Quoted trading names stay as well.
        assert_eq!(
            shown("Ветлин Н. О.", &[("Ветлин Н. О.", "organization")]),
            pairs(&[("Ветлин Н. О.", "ORG")])
        );
        let source = "ИП Ветлин Н. О., магазин «Кедрач»";
        assert_eq!(
            shown(
                source,
                &[
                    ("Ветлин Н. О.", "organization"),
                    ("«Кедрач»", "organization")
                ]
            ),
            pairs(&[("Ветлин Н. О.", "PERSON"), ("«Кедрач»", "ORG")])
        );
    }

    #[test]
    fn entrepreneur_full_names_in_capitals_are_people() {
        for (source, name) in [
            ("ИП МОХОВ ЛЕВ РОМАНОВИЧ", "МОХОВ ЛЕВ РОМАНОВИЧ"),
            ("ИП ЛЕВ РОМАНОВИЧ МОХОВ", "ЛЕВ РОМАНОВИЧ МОХОВ"),
            ("ИП МОХОВА ЛИДИЯ ИГОРЕВНА", "МОХОВА ЛИДИЯ ИГОРЕВНА"),
            ("ИП МОХОВ-БРЮН ЛЕВ РОМАНОВИЧ", "МОХОВ-БРЮН ЛЕВ РОМАНОВИЧ"),
            (
                "индивидуального предпринимателя МОХОВА ЛЬВА РОМАНОВИЧА",
                "МОХОВА ЛЬВА РОМАНОВИЧА",
            ),
            (
                "ИНДИВИДУАЛЬНЫЙ ПРЕДПРИНИМАТЕЛЬ МОХОВ ЛЕВ РОМАНОВИЧ",
                "МОХОВ ЛЕВ РОМАНОВИЧ",
            ),
        ] {
            assert!(
                !pack().identifying(source, Category::Organization),
                "{source}"
            );
            for raw in [
                vec![],
                vec![(source, "organization")],
                vec![(name, "organization")],
                vec![(source, "person")],
            ] {
                assert_eq!(
                    shown(source, &raw),
                    pairs(&[(name, "PERSON")]),
                    "{source}: {raw:?}"
                );
            }
        }
    }

    /// A surname in capitals before a name and patronymic in title case,
    /// and the name in every case form, «-ичом» and «-ичем» among them
    /// (the Mac hub's review of #582, issue #562).
    #[test]
    fn entrepreneur_mixed_case_and_case_forms_are_people() {
        for (source, name) in [
            ("ИП МОХОВ Лев Романович", "МОХОВ Лев Романович"),
            ("ИП Лев Романович МОХОВ", "Лев Романович МОХОВ"),
            (
                "индивидуальным предпринимателем Моховым Ильёй Ильичом",
                "Моховым Ильёй Ильичом",
            ),
            (
                "индивидуальным предпринимателем МОХОВЫМ ИЛЬЁЙ ИЛЬИЧОМ",
                "МОХОВЫМ ИЛЬЁЙ ИЛЬИЧОМ",
            ),
            (
                "индивидуальным предпринимателем Моховым Львом Романовичем",
                "Моховым Львом Романовичем",
            ),
            (
                "индивидуальным предпринимателем Ильёй Ильичом Моховым",
                "Ильёй Ильичом Моховым",
            ),
            (
                "индивидуальному предпринимателю Мохову Илье Ильичу",
                "Мохову Илье Ильичу",
            ),
            (
                "об индивидуальном предпринимателе Мохове Илье Ильиче",
                "Мохове Илье Ильиче",
            ),
            (
                "индивидуального предпринимателя Мохова Ильи Ильича",
                "Мохова Ильи Ильича",
            ),
            (
                "индивидуальным предпринимателем Моховой Лидией Игоревной",
                "Моховой Лидией Игоревной",
            ),
            (
                "индивидуальным предпринимателем Брюн Анной Сергеевной",
                "Брюн Анной Сергеевной",
            ),
            (
                "индивидуальным предпринимателем Моховой Ниной Ильиничной",
                "Моховой Ниной Ильиничной",
            ),
            (
                "индивидуального предпринимателя Моховой Нины Ильиничны",
                "Моховой Нины Ильиничны",
            ),
            (
                "индивидуальному предпринимателю Моховой Лидии Игоревне",
                "Моховой Лидии Игоревне",
            ),
            (
                "на индивидуального предпринимателя Мохову Лидию Игоревну",
                "Мохову Лидию Игоревну",
            ),
        ] {
            assert!(
                !pack().identifying(source, Category::Organization),
                "{source}"
            );
            for raw in [
                vec![],
                vec![(source, "organization")],
                vec![(name, "organization")],
                vec![(source, "person")],
            ] {
                assert_eq!(
                    shown(source, &raw),
                    pairs(&[(name, "PERSON")]),
                    "{source}: {raw:?}"
                );
            }
        }
    }

    /// An individual entrepreneur is a person, «ИП» or «индивидуальный
    /// предприниматель» outside the name, in any case form; a trading name
    /// in quotes stays an organization, and placeholders stay no one (guide
    /// rule 1, the maintainer's decision on #484 items 5 and 8, issue #562).
    #[test]
    fn individual_entrepreneurs_are_people() {
        let pack = pack();
        for text in [
            "ИП Мохов Л. Р.",
            "ИП Мохов Л.Р.",
            "ИП Мохов Л. Р.,",
            "ИП МОХОВ Л.Р.",
            "ИП Л. Р. Мохов",
            "ИП Ветлину Н.О.",
            "ответчик ИП Мохов Л. Р.",
            "индивидуальный предприниматель Мохов Лев Романович",
            "Индивидуальный предприниматель Брюн Лидия Игоревна",
            "индивидуального предпринимателя Ветлина Н. О.",
            "индивидуальным предпринимателем Мохов Лев Романович",
            "ИП Лев Романович Мохов",
            "ИП Мохова Лидия Игоревна",
            "ИП Мохов-Брюн Л. Р.",
        ] {
            assert!(!pack.identifying(text, Category::Organization), "{text}");
        }
        for text in [
            "ИП Мохов Л. Р. магазин «Кедрач»",
            "ИП «Кедрач»",
            "ИП Мохов",
            "ИП Мохов и партнёры",
            "ООО «Мохов Л. Р.»",
            "Мохов Л. Р.",
        ] {
            assert!(pack.identifying(text, Category::Organization), "{text}");
        }
        let source = "Истец ИП Ветлин Н. О. и индивидуальный предприниматель Брюн Лидия Игоревна заключили договор с ООО «Север».";
        let expected = pairs(&[
            ("Ветлин Н. О.", "PERSON"),
            ("Брюн Лидия Игоревна", "PERSON"),
            ("ООО «Север»", "ORG"),
        ]);
        // The model's organizations, or nothing for the names.
        assert_eq!(
            shown(
                source,
                &[
                    ("ИП Ветлин Н. О.", "organization"),
                    (
                        "индивидуальный предприниматель Брюн Лидия Игоревна",
                        "organization"
                    ),
                    ("ООО «Север»", "organization"),
                ]
            ),
            expected
        );
        assert_eq!(shown(source, &[("ООО «Север»", "organization")]), expected);
        // Or as persons with the status inside the span.
        assert_eq!(
            shown(
                source,
                &[
                    ("ИП Ветлин Н. О.", "person"),
                    (
                        "индивидуальный предприниматель Брюн Лидия Игоревна",
                        "person"
                    ),
                    ("ООО «Север»", "organization"),
                ]
            ),
            expected
        );
        // Or the model had the names right.
        assert_eq!(
            shown(
                source,
                &[
                    ("Ветлин Н. О.", "person"),
                    ("Брюн Лидия Игоревна", "person"),
                    ("ООО «Север»", "organization"),
                ]
            ),
            expected
        );
        // Case forms, and the initials without a space.
        let source = "Иск к индивидуальному предпринимателю Ветлину Н. О. и ИП Ветлину Н.О. отклонён; от индивидуального предпринимателя Мохова Льва Романовича поступил отзыв.";
        assert_eq!(
            shown(
                source,
                &[
                    (
                        "индивидуальному предпринимателю Ветлину Н. О.",
                        "organization"
                    ),
                    ("ИП Ветлину Н.О.", "organization"),
                    (
                        "индивидуального предпринимателя Мохова Льва Романовича",
                        "organization"
                    ),
                ]
            ),
            pairs(&[
                ("Ветлину Н. О.", "PERSON"),
                ("Ветлину Н.О.", "PERSON"),
                ("Мохова Льва Романовича", "PERSON"),
            ])
        );
        // A shop sign in quotes is an organization; only the name changes.
        let source = "Исполнитель: ИП Мохов Л. Р., магазин «Кедрач».";
        assert_eq!(
            shown(
                source,
                &[
                    ("ИП Мохов Л. Р.", "organization"),
                    ("«Кедрач»", "organization")
                ]
            ),
            pairs(&[("Мохов Л. Р.", "PERSON"), ("«Кедрач»", "ORG")])
        );
        // The status alone, a company and a role are no names.
        let source =
            "Индивидуальный предприниматель вправе заключить договор. ИП Ромашка не явился.";
        assert_eq!(shown(source, &[]), pairs(&[]));
        // Placeholders stay no one, their real initials PERSON alone.
        let source = "Истец ИП ФИО1 и ИП ФИО3 А-Х.О. подали иск.";
        assert_eq!(
            shown(
                source,
                &[
                    ("ИП ФИО1", "organization"),
                    ("ИП ФИО3 А-Х.О.", "organization")
                ]
            ),
            pairs(&[("А-Х.О.", "PERSON")])
        );
    }

    /// Placeholders for a party are never marked, whatever the model
    /// guessed; a real name beside one, and real names and companies called
    /// alike, stay (guide rule 8, #484 item 1).
    #[test]
    fn placeholders_are_no_names() {
        let pack = pack();
        for (text, category) in [
            ("ФИО1", Category::Organization),
            ("ФИО1", Category::Person),
            ("ФИО1", Category::Identity),
            ("ФИО 2", Category::Person),
            ("«ФИО12»", Category::Organization),
            ("ИП ФИО1", Category::Organization),
            ("ИП ФИО1", Category::Person),
            ("ФИО1 и ФИО2", Category::Person),
            ("ФИО1, ФИО2", Category::Organization),
            ("John Doe", Category::Person),
            ("Jane Does 1-25", Category::Person),
            ("John Does l-25", Category::Person),
            ("Richard Roe", Category::Person),
            ("DOE SUPERVISORS I-X", Category::Organization),
            ("DOE SUPERVISORS", Category::Organization),
            ("DOE MEDICAL STAFF I-X", Category::Organization),
            ("ROE ENTITIES", Category::Organization),
            ("ROE CORPORATIONS I through X", Category::Organization),
            ("DOES 1 through 10", Category::Person),
            ("Does 1-10", Category::Person),
            ("Doe 3", Category::Person),
            ("DOE", Category::Person),
            ("DOE", Category::Organization),
            ("ROE", Category::Organization),
            ("Does", Category::Person),
            ("I-X", Category::Person),
            ("I-X", Category::Organization),
            ("1-25", Category::Organization),
            ("ФИО1", Category::Tax),
            ("ФИО1", Category::Phone),
            ("ФИО1", Category::Bank),
            // A role before it, a dash between two, a role-number
            // placeholder (#484 item 1, extended).
            ("конкурсного управляющего ФИО1", Category::Person),
            ("ФИО3 – ФИО6", Category::Organization),
            ("ФИО3-ФИО6", Category::Person),
            ("Потерпевший №1", Category::Person),
            ("потерпевшего № 2", Category::Organization),
            ("представителя потерпевшего №1", Category::Person),
            ("Свидетель №3", Category::Person),
            ("Потерпевший №1", Category::Identity),
            // Real initials after one come back alone by the span pattern.
            ("ИП ФИО3 А-Х.О.", Category::Organization),
            ("ФИО3 А-Х.О.", Category::Person),
            ("ФИО2 В. С.", Category::Person),
        ] {
            assert!(!pack.identifying(text, category), "{text}");
        }
        for (text, category) in [
            ("ФИО3 Иванов", Category::Person),
            ("Потерпевший Ветлин", Category::Person),
            ("ИП Мохов Л. Р. магазин «Кедрач»", Category::Organization),
            ("ИП «Кедрач»", Category::Organization),
            ("Мирон Павлович Грендель", Category::Person),
            ("Samuel Doe", Category::Person),
            ("Jane Doerr", Category::Person),
            ("John Roebuck", Category::Person),
            ("Doe Run Company", Category::Organization),
            ("Roe Holdings Ltd", Category::Organization),
            ("Ivo X", Category::Person),
            ("J. Doe", Category::Person),
            ("A. Roe", Category::Person),
            ("Doe", Category::Person),
        ] {
            assert!(pack.identifying(text, category), "{text}");
        }
        // In context: "ФИО:" is a label, the name after it a person; real
        // initials after a placeholder are PERSON alone, whether the model
        // took them with it, as an organization, or not at all. "Doe"
        // alone is a person's after a title or beside a real first name.
        let source = "ФИО: Мирон Грендель. Истец ФИО1 и ИП ФИО2 подали иск к ИП ФИО3 А-Х.О. \
            и к ФИО4 В. С., Потерпевший №1 возражал.";
        let raw = [
            ("Мирон Грендель", "person"),
            ("ФИО1", "organization"),
            ("ИП ФИО2", "organization"),
            ("ИП ФИО3 А-Х.О.", "organization"),
            ("Потерпевший №1", "person"),
        ];
        assert_eq!(
            shown(source, &raw),
            pairs(&[
                ("Мирон Грендель", "PERSON"),
                ("А-Х.О.", "PERSON"),
                ("В. С.", "PERSON")
            ])
        );
        // After a role-number placeholder too, and before a speaker's
        // colon.
        let source = "Потерпевший №2 Л. И. пояснил. ФИО5 Р. Т.: «Возражаю».";
        let raw = [("Потерпевший №2 Л. И.", "person"), ("ФИО5 Р. Т.", "person")];
        assert_eq!(
            shown(source, &raw),
            pairs(&[("Л. И.", "PERSON"), ("Р. Т.", "PERSON")])
        );
        let source = "Plaintiffs sue John Does 1-25, DOE SUPERVISORS I-X and ROE ENTITIES; \
            DOE agreed.";
        let raw = [
            ("John Does 1-25", "person"),
            ("DOE SUPERVISORS", "organization"),
            ("I-X", "person"),
            ("ROE ENTITIES", "organization"),
            ("DOE", "person"),
        ];
        assert_eq!(shown(source, &raw), pairs(&[]));
        let source = "Witness J. Doe testified.";
        assert_eq!(
            shown(source, &[("J. Doe", "person")]),
            pairs(&[("J. Doe", "PERSON")])
        );
        let source = "Mr Doe testified.";
        assert_eq!(
            shown(source, &[("Doe", "person")]),
            pairs(&[("Doe", "PERSON")])
        );
        let source = "Doe left early. Samuel Doe agreed.";
        assert_eq!(
            shown(source, &[("Doe", "person"), ("Samuel Doe", "person")]),
            pairs(&[("Doe", "PERSON"), ("Samuel Doe", "PERSON")])
        );
    }

    /// Cadastral and registration numbers and numbers assigned to one person
    /// are IDENTITY, the value only; a cadastral quarter, a notarial act's
    /// entry and other numbers stay (#484 items 2 and 3, #483).
    #[test]
    fn cadastral_and_personal_register_numbers_are_identity() {
        let found = |source: &str| -> Vec<(String, Category)> {
            let mut found: Vec<(String, Category)> = pack()
                .context(source)
                .into_iter()
                .chain(pack().spans(source))
                .filter(|d| d.category == Category::Identity)
                .map(|d| (source[d.range].to_owned(), d.category))
                .collect();
            found.dedup();
            found
        };
        let id = |text: &str| vec![(text.to_owned(), Category::Identity)];
        for (source, value) in [
            ("кадастровый номер 99:07:0012345:678", "99:07:0012345:678"),
            (
                "с кадастровым номером 99:07:0012345:678.",
                "99:07:0012345:678",
            ),
            (
                "Кадастровый номер земельного участка: 99:07:0012345:678",
                "99:07:0012345:678",
            ),
            ("КН 99:07:0012345:6789", "99:07:0012345:6789"),
            ("участок 99:07:001234:5 продан", "99:07:001234:5"),
            (
                "номер регистрации права 99:07:0012345:678-99/011/2031-4",
                "99:07:0012345:678-99/011/2031-4",
            ),
            (
                "номер государственной регистрации от 12.03.2031 № 99-99/011-99/011/001/2031-123/2",
                "99-99/011-99/011/001/2031-123/2",
            ),
            (
                "регистрационный номер в сводном реестре 99887, e-mail",
                "99887",
            ),
            (
                "регистрационный номер в сводном государственном реестре арбитражных управляющих 12345",
                "12345",
            ),
            (
                "регистрационный номер в реестре адвокатов Ольховской области 99/1234",
                "99/1234",
            ),
            ("рег. номер в реестре нотариусов: 99/123-н", "99/123-н"),
            ("удостоверение адвоката № 9917", "9917"),
            ("удостоверением № 99012", "99012"),
            ("Ohio State Bar No. 0099911", "0099911"),
            ("(Pa. Bar # 99911)", "99911"),
            ("Bar No.: 12345", "12345"),
            ("Texas Bar No. 24099911", "24099911"),
            ("SBN 299911", "299911"),
            ("ROWAN PELL, ESQ. (RP9911)", "RP9911"),
            ("Officer Ivo Ostrand, Shield No. 9912.", "9912"),
            ("реестровый номер адвоката 99/1234", "99/1234"),
            (
                "| Bar No |
|---|
| 0099911 |
",
                "0099911",
            ),
            (
                "| Регистрационный номер в реестре адвокатов |
|---|
| 99/1234 |
",
                "99/1234",
            ),
        ] {
            assert_eq!(found(source), id(value), "{source}");
        }
        for source in [
            "кадастровый квартал 99:07:0012345",
            "реестровый номер 99/123-н/99-2031-1-456",
            "| Номер |
|---|
| 12345 |
",
            "кадастровая стоимость 1234567 рублей",
            "зарегистрировано в реестре № 99/123-н/99-2031-1-456",
            "в реестре нотариусов не значится",
            "время 12:30:45",
            "Bar association met in 2031",
            "the bar is 2 miles away",
        ] {
            assert!(found(source).is_empty(), "{source}");
        }
        // The model's tax, phone or bank guesses on a cadastral number go,
        // and the number is IDENTITY.
        for label in ["tax_id", "phone_number", "bank_account"] {
            let source = "Участок с кадастровым номером 99:07:0012345:678 передан.";
            assert_eq!(
                shown(source, &[("99:07:0012345:678", label)]),
                pairs(&[("99:07:0012345:678", "IDENTITY")]),
                "{label}"
            );
            let source = "Участок 99:07:0012345:678 передан.";
            assert_eq!(
                shown(source, &[("99:07:0012345:678", label)]),
                pairs(&[("99:07:0012345:678", "IDENTITY")]),
                "{label}"
            );
        }
        // A partial tax guess on a cadastral number gives way to the whole.
        let source = "Участок 99:07:0012345:678 продан.";
        assert_eq!(
            shown(source, &[("99:07:0012345", "tax_id")]),
            pairs(&[("99:07:0012345:678", "IDENTITY")])
        );
        // A bar number swallowed into an organization is the number alone.
        let source = "Counsel: Rowan Pell, Texas State Bar No. 24099911, for Quillon Freight LLC.";
        assert_eq!(
            shown(
                source,
                &[
                    ("Rowan Pell", "person"),
                    ("Texas State Bar No. 24099911", "organization"),
                    ("Quillon Freight LLC", "organization"),
                ]
            ),
            pairs(&[
                ("Rowan Pell", "PERSON"),
                ("24099911", "IDENTITY"),
                ("Quillon Freight LLC", "ORG"),
            ])
        );
        // So is a bar's name before its number.
        let source = "Ohio State Bar No. 0099911";
        assert_eq!(
            shown(source, &[("Ohio State Bar", "organization")]),
            pairs(&[("0099911", "IDENTITY")])
        );
    }

    /// A public-law company is a public body, by its form or its name
    /// (guide rule 8, #484 item 4); companies stay.
    #[test]
    fn a_public_law_company_is_a_public_body() {
        let source = "Сведения предоставила публично-правовая компания «Роскадастр», договор \
            заключён с ППК «Фонд развития территорий», филиал Роскадастра ответил, а \
            ООО «Кадастр-Сервис» и АО «Фонд Сибири» подписали акт.";
        let raw = [
            ("Роскадастр", "organization"),
            ("Фонд развития территорий", "organization"),
            ("Роскадастра", "organization"),
            ("Кадастр-Сервис", "organization"),
            ("Фонд Сибири", "organization"),
        ];
        assert_eq!(
            shown(source, &raw),
            pairs(&[("ООО «Кадастр-Сервис»", "ORG"), ("АО «Фонд Сибири»", "ORG")])
        );
        for (text, category) in [
            ("ППК «Роскадастр»", Category::Organization),
            (
                "публично-правовой компании «Фонд развития территорий»",
                Category::Organization,
            ),
            ("Роскадастру", Category::Person),
        ] {
            assert!(!pack().identifying(text, category), "{text}");
        }
        // The company's quoted name as a person is no one either.
        let source = "Договор заключён с ППК «Фонд развития территорий».";
        assert_eq!(
            shown(source, &[("Фонд развития территорий", "person")]),
            pairs(&[])
        );
    }

    /// An account's label comes off its span, a colon with it, and a label
    /// the trim takes no part of still gives its value ("acct"); a value's
    /// own final letters stay, and a case number wrapped over lines is no
    /// account (guide rule 6, issue #486).
    #[test]
    fn account_labels_stay_outside_bank_spans() {
        let source = "Pay to acct 1234567890 or IBAN: GB47 KELD 6012 4483 7196 02;             ref LU12 3456 7890 1234 ACCT. Claim No.
6001842
/
2026 is pending.
";
        let raw = [
            ("acct 1234567890", "bank_account"),
            ("IBAN: GB47 KELD 6012 4483 7196 02", "iban"),
            ("LU12 3456 7890 1234 ACCT", "bank_account"),
            (
                "6001842
/
2026",
                "bank_account",
            ),
        ];
        assert_eq!(
            shown(source, &raw),
            pairs(&[
                ("1234567890", "BANK"),
                ("GB47 KELD 6012 4483 7196 02", "BANK"),
                ("LU12 3456 7890 1234 ACCT", "BANK"),
            ])
        );
        let source = "Плательщик перечисляет на р/с: 40702810900000004711.
";
        let raw = [("р/с: 40702810900000004711", "bank_account")];
        assert_eq!(
            shown(source, &raw),
            pairs(&[("40702810900000004711", "BANK")])
        );
    }

    /// A dash may stand between a classifier's code and its value, as a
    /// form's table writes it (issue #486).
    #[test]
    fn classifier_codes_after_a_dash_are_no_identifiers() {
        let source = "Коды: ОКФС - 61, ОКВЭД – 84.30; ИНН 7707083893.
";
        let raw = [
            ("61", "tax_id"),
            ("84.30", "tax_id"),
            ("7707083893", "tax_id"),
        ];
        assert_eq!(shown(source, &raw), pairs(&[("7707083893", "TAX")]));
    }

    /// Programmes, standards, conditions, treatises, a legal form with no
    /// name, generic words, common abbreviations, and software, sites and
    /// app platforms are no companies (#501, #481, #488); companies with
    /// their legal form or a name of their own keep it.
    #[test]
    fn programmes_abbreviations_and_products_are_no_companies() {
        let pack = pack();
        for (text, category) in [
            // Programmes, services, plans, standards, projects (#501).
            ("Bright Paths Program", Category::Organization),
            ("the Bright Paths Programme", Category::Organization),
            ("VR services", Category::Organization),
            ("National Reading Access Standard", Category::Organization),
            (
                "Brindle Backwater Pumping Plant project",
                Category::Organization,
            ),
            ("Quality Assurance", Category::Organization),
            ("Phase II Trial", Category::Organization),
            ("Federal Employment", Category::Organization),
            ("Acute Kidney Strain", Category::Organization),
            ("Acute Lung Injury", Category::Organization),
            ("Respiratory Distress Syndrome", Category::Organization),
            // A legal form with no name, and a treatise.
            (
                "a Wyoming limited liability company",
                Category::Organization,
            ),
            (
                "New Jersey limited liability company",
                Category::Organization,
            ),
            ("a Delaware corporation", Category::Organization),
            ("an English private limited company", Category::Organization),
            ("4 Brindle on Insolvency", Category::Organization),
            ("Vell and Ostrand on Contracts", Category::Organization),
            // Generic words alone.
            ("Crew", Category::Organization),
            ("The Crew", Category::Organization),
            ("Crew", Category::Person),
            ("Operations", Category::Organization),
            ("Assignment", Category::Organization),
            ("Facilities", Category::Organization),
            ("Human Resources", Category::Organization),
            ("Services", Category::Organization),
            ("F.O.B. DESTINATION", Category::Organization),
            // Common abbreviations (#481).
            ("VAT", Category::Organization),
            ("EFT", Category::Organization),
            ("F.O.B.", Category::Organization),
            ("OEM", Category::Organization),
            ("SICAV", Category::Organization),
            ("NOBO", Category::Organization),
            ("LEA", Category::Organization),
            ("the LEA's", Category::Organization),
            ("IEP", Category::Organization),
            ("RNG", Category::Organization),
            ("MMS", Category::Person),
            ("K-12", Category::Organization),
            ("FOBTs", Category::Organization),
            // Sites, software, portals and app platforms (#488).
            ("Timber Trade Online", Category::Organization),
            ("FileFlow Manager", Category::Organization),
            ("Application Manager", Category::Organization),
            ("Case Management System", Category::Organization),
            ("the grants portal", Category::Organization),
            ("Brindle Jobs Portal", Category::Organization),
            ("AppStore", Category::Organization),
            ("the App Store", Category::Organization),
            ("Google Play", Category::Organization),
            ("GooglePlay", Category::Organization),
            ("USAJOBS", Category::Organization),
            ("Grants.gov", Category::Organization),
            ("CM/ECF", Category::Organization),
            ("myHMCTS", Category::Organization),
            ("OASys", Category::Organization),
            ("WHO.INT", Category::Organization),
            // Capitals only for the abbreviations that are names too.
            ("the SEA", Category::Organization),
            ("FIRs", Category::Organization),
            // A head word ends a name with no legal form after it: the
            // issue's choice (#488, review of #543).
            ("America Online", Category::Organization),
        ] {
            assert!(!pack.identifying(text, category), "{text}");
        }
        for (text, category) in [
            // A legal form keeps a company, whatever its words.
            ("Northvale Ledger Services LLC", Category::Organization),
            ("Acme Program Ltd", Category::Organization),
            ("Harrow Services Ltd", Category::Organization),
            ("Timber Trade Online LLC", Category::Organization),
            ("Crew Ltd", Category::Organization),
            (
                "Quillon Freight LLC, a Wyoming limited liability company",
                Category::Organization,
            ),
            ("Copper Dusk Facilities Ltd", Category::Organization),
            // A name of its own keeps one without a legal form.
            ("Northvale Ledger Services", Category::Organization),
            ("Harrow Floor Services", Category::Organization),
            ("Plan International", Category::Organization),
            ("Standard Chartered", Category::Organization),
            ("Acme Systems", Category::Organization),
            ("Acme Corporation", Category::Organization),
            ("Brindle Trust", Category::Organization),
            ("Crew Clothing", Category::Organization),
            ("Acme Fund SICAV", Category::Organization),
            ("VAT Advisers Quill", Category::Organization),
            ("Apple", Category::Organization),
            ("Google", Category::Organization),
            ("Brindle Online Group", Category::Organization),
            ("Bramble Purl Surveys Ltd", Category::Organization),
            // People.
            ("Ossian Crew", Category::Person),
            ("Lea Brindle", Category::Person),
            ("Paul Program", Category::Person),
            ("Collier", Category::Person),
            // Review of #543: a legal form keeps a head word, a subject that
            // is no field of law keeps a name, a name before a legal form in
            // words stays, and so does a word that only starts like an app
            // platform; "Lea" is a name.
            ("ООО «Bright Paths Program»", Category::Organization),
            ("Meals on Wheels America", Category::Organization),
            ("Meals on Wheels", Category::Organization),
            (
                "Northvale Ledger limited liability company",
                Category::Organization,
            ),
            ("App Storehouse", Category::Organization),
            ("Lea", Category::Person),
            ("Lea", Category::Organization),
        ] {
            assert!(pack.identifying(text, category), "{text}");
        }
    }

    /// #501, #481 and #488's reproductions as the user is shown them: only
    /// the company is proposed; abbreviations defined beside a programme,
    /// a standard or a condition go wherever they stand, a company's and a
    /// person's stay, and a field code heading a table's column is no
    /// company.
    #[test]
    fn generic_names_and_their_abbreviations_are_not_shown() {
        let source = "Northvale Ledger Services LLC, a Wyoming limited liability company, \
            runs the Bright Paths Program under the National Reading Access Standard \
            (NRAS) for patients with Acute Kidney Strain (AKS); see 4 Brindle on \
            Insolvency ¶ 12.03. The Crew reports to Assignment. NRAS applies; AKS is \
            rare. VAT on EFT payments, F.O.B. destination; the LEA's IEP and the RNG \
            used by Quillon Freight LLC (“QF”). Apply through Timber Trade Online and \
            FileFlow Manager; the app is on the AppStore. Ossian Draycott (“OD”) \
            and QF agree; OD signs.";
        let raw = [
            ("Northvale Ledger Services LLC", "organization"),
            ("Wyoming limited liability company", "organization"),
            ("Bright Paths Program", "organization"),
            ("National Reading Access Standard", "organization"),
            ("NRAS", "organization"),
            ("Acute Kidney Strain", "organization"),
            ("AKS", "person"),
            ("Brindle on Insolvency", "organization"),
            ("Crew", "organization"),
            ("Assignment", "organization"),
            ("VAT", "organization"),
            ("EFT", "organization"),
            ("F.O.B. destination", "organization"),
            ("LEA's", "organization"),
            ("IEP", "organization"),
            ("RNG", "organization"),
            ("Quillon Freight LLC", "organization"),
            ("QF", "organization"),
            ("Timber Trade Online", "organization"),
            ("FileFlow Manager", "organization"),
            ("AppStore", "organization"),
            ("Ossian Draycott", "person"),
            ("OD", "person"),
        ];
        assert_eq!(
            shown(source, &raw),
            pairs(&[
                ("Northvale Ledger Services LLC", "ORG"),
                ("Quillon Freight LLC", "ORG"),
                ("QF", "ORG"),
                ("Ossian Draycott", "PERSON"),
                ("OD", "PERSON"),
                ("QF", "ORG"),
                ("OD", "PERSON"),
            ])
        );
        // Field codes heading the columns are no companies, and a code in a
        // body row stays; a header with any other cell keeps its spans
        // (review of #543).
        let source = "| FLDA | FLDB |\n|---|---|\n| FLDC | 12 |\n\n\
            | HSBC | Quillon Freight LLC |\n| :-- | --: |\n| 1 | 2 |\n";
        assert_eq!(
            shown(
                source,
                &[
                    ("FLDA", "organization"),
                    ("FLDB", "organization"),
                    ("FLDC", "organization"),
                    ("HSBC", "organization"),
                    ("Quillon Freight LLC", "organization"),
                ]
            ),
            pairs(&[
                ("FLDC", "ORG"),
                ("HSBC", "ORG"),
                ("Quillon Freight LLC", "ORG"),
            ])
        );
        // A company's initials stay where the model missed its long form,
        // which has a word of its own (review of #543).
        let source = "International Business Machines (IBM) supplied it; IBM invoiced.";
        assert_eq!(
            shown(source, &[("IBM", "organization")]),
            pairs(&[("IBM", "ORG"), ("IBM", "ORG")])
        );
    }

    /// Common words keep a person named so beside a title or their full
    /// name (#501, #481): "Ms Crew", "Lea Brindle" then "Lea".
    #[test]
    fn generic_words_stay_a_persons_name_in_context() {
        let source = "Ms Crew met Lea Brindle; Lea signed.";
        // Each anchor's first word, at the anchor's place.
        let entities: Vec<crate::RawEntity> = [
            ("Ms Crew", "Crew", "person"),
            ("Lea Brindle", "Lea Brindle", "person"),
            ("Lea signed", "Lea", "person"),
        ]
        .into_iter()
        .map(|(anchor, text, label)| {
            let start = source.find(anchor).unwrap() + anchor.find(text).unwrap();
            crate::RawEntity {
                start,
                end: start + text.len(),
                label: label.into(),
                score: 0.8,
                text: text.into(),
            }
        })
        .collect();
        let found: Vec<(String, String)> = crate::recognition::proposals(source, &entities)
            .unwrap()
            .into_iter()
            .map(|span| (source[span.range].to_owned(), span.category))
            .collect();
        assert_eq!(
            found,
            pairs(&[
                ("Crew", "PERSON"),
                ("Lea Brindle", "PERSON"),
                ("Lea", "PERSON"),
            ])
        );
    }

    /// Probe rounds 1 and 2 (#385): only complete roles go, in either
    /// model category. Do not use an arbitrary job head as a span veto.
    #[test]
    fn english_probe_roles_are_no_names() {
        for role in [
            "Debtor",
            "Trustee",
            "Chapter 7 Trustee",
            "Chapter 11 Trustee",
            "First Defendant",
            "Defendants",
            "Second Claimant",
            "Claimant",
            "Party",
            "Bankrupt Party",
            "Plaintiff(s)",
            "Affiliate",
            "Agents",
            "Lead Agent",
            "Dealer Covered Person",
            "Sponsor Staff",
            "Operating Principal",
            "Recipient",
            "Discloser",
            "Franchisee",
            "Subscriber",
            "Judges",
            "Chairmen",
            "Deputies",
            "Secretaries",
            "Masters",
            "Recorders",
            "Circuit Judges",
            "Circuit Judge(s)",
            "UNITED STATES DISTRICT JUDGE",
            "Sergeant",
            "SHERIFF",
            "UNDERSHERIFF",
            "Monsieur",
            "King of Sweden",
            "Esquire",
            "Team Leader",
            "Chief Investigator",
            "Bureau Safety Manager",
            "Chair",
            "Director",
            "Accident Investigation Advisor",
            "Program Manager",
            "Vice President of Infrastructure",
            "CxO",
            "CEO",
            "Supervisors",
            "Fire Marshal",
            "Officer",
            "TREASURER",
            "Participant",
            "Participants",
            "other Participant",
            "originating Participants",
            "host Participant's",
            "Head of Delegation",
            "HOD",
            "Chairman",
            "INTERNATIONAL AGREEMENTS SPECIALIST",
            "Appellees",
            "Appellants",
            "Attorneys for Plaintiffs",
            "Magistrate Judge",
            "Notary Public",
            "User",
            "Division Administrator",
            "Administrative Adviser",
        ] {
            for label in ["person", "organization"] {
                // Listed capitalized surnames are deliberately
                // retained as PERSON; their lowercase role readings still go.
                let person_name =
                    ["Masters", "Sergeant", "SHERIFF", "Chair", "Chairman"].contains(&role);
                let expected = if label == "person" && person_name {
                    pairs(&[(role, "PERSON")])
                } else {
                    Vec::new()
                };
                assert_eq!(shown(role, &[(role, label)]), expected, "{label}: {role}");
                let lower = role.to_lowercase();
                assert!(
                    shown(&lower, &[(&lower, label)]).is_empty(),
                    "{label}: {lower}"
                );
            }
        }
    }

    /// Russian roles include oblique cases, plurals and multiword offices
    /// (#385), never a wildcard that would also take an officer's name.
    #[test]
    fn russian_probe_roles_are_no_names() {
        for role in [
            "Собрание кредиторов",
            "Истец",
            "Истца",
            "Ответчику",
            "Заемщика",
            "Заёмщиком",
            "Заимодавец",
            "Заимодавца",
            "Заимодавцу",
            "Заимодавцем",
            "Заимодавце",
            "Займодавец",
            "Лицо",
            "Лица",
            "Лицами",
            "Несовершеннолетний ребенок",
            "Несовершеннолетнего ребёнка",
            "Несовершеннолетнему ребёнку",
            "Несовершеннолетним ребенком",
            "Руководитель",
            "Руководителя",
            "Руководителю",
            "Руководителем",
            "Руководителе",
            "Заместитель руководителя",
            "Заместителя главного бухгалтера",
            "Заместителю главного бухгалтера",
            "Уполномоченный",
            "Уполномоченного",
            "Предприниматель",
            "Предпринимателя",
            "Предпринимателю",
            "Предпринимателем",
            "Предпринимателе",
            "Правообладатель",
            "Правообладателя",
            "Правообладателю",
            "Правообладателем",
            "Правообладателе",
            "Должник",
            "Должника",
            "Должнику",
            "Должником",
            "Должнике",
            "Должник холост",
            "Кассатор",
            "Кассатора",
            "Кассатором",
            "Заявитель",
            "Заявителя",
            "Заявителю",
            "Заявителем",
            "Заявителе",
            "конкурсный управляющий",
            "конкурсного управляющего",
            "конкурсному управляющему",
            "финансовому управляющему",
            "финансовым управляющим",
            "арбитражного управляющего",
            "арбитражном управляющем",
            "Уполномоченный по защите прав предпринимателей",
            "Уполномоченному по защите прав предпринимателей",
            "Индивидуальный предприниматель",
            "ИП",
        ] {
            for label in ["person", "organization"] {
                assert!(shown(role, &[(role, label)]).is_empty(), "{label}: {role}");
            }
        }
    }

    /// Complete edge phrases are safe to trim; a role-like surname on
    /// either edge is retained. ИП is outside an entrepreneur's name
    /// (guide rule 1, #484 item 5).
    #[test]
    fn probe_role_edges_leave_the_name() {
        for (source, expected) in [
            ("Chief Investigator Ms Edda Brannock", "Edda Brannock"),
            ("Team Leader Edda Brannock", "Edda Brannock"),
            ("Edda Brannock, Division Administrator", "Edda Brannock"),
            (
                "Vice President of Infrastructure Edda Brannock",
                "Edda Brannock",
            ),
            ("Monsieur Ivo Penrow", "Ivo Penrow"),
            ("Chapter 7 Trustee Ivo Penrow", "Ivo Penrow"),
            ("First Defendant Edda Brannock", "Edda Brannock"),
            ("Attorneys for Plaintiffs Ivo Penrow", "Ivo Penrow"),
            (
                "Уполномоченный по защите прав предпринимателей Лада Брюн",
                "Лада Брюн",
            ),
            ("ИП Пахомцев И. В.", "Пахомцев И. В."),
            (
                "Индивидуальный предприниматель Пахомцев И. В.",
                "Пахомцев И. В.",
            ),
            (
                "конкурсного управляющего Пахомцева И. В.",
                "Пахомцева И. В.",
            ),
            (
                "финансовому управляющему Пахомцеву И. В.",
                "Пахомцеву И. В.",
            ),
            (
                "Пахомцевым И. В., арбитражным управляющим",
                "Пахомцевым И. В.",
            ),
            ("Заместитель главного бухгалтера Лада Брюн", "Лада Брюн"),
        ] {
            assert_eq!(
                shown(source, &[(source, "person")]),
                pairs(&[(expected, "PERSON")]),
                "{source}"
            );
        }
        for (source, expected) in [
            (
                "Team Leader, Brannock Services Ltd",
                "Brannock Services Ltd",
            ),
            (
                "Brannock Services Ltd, Chief Investigator",
                "Brannock Services Ltd",
            ),
            ("конкурсного управляющего: ООО «Листолёт»", "ООО «Листолёт»"),
        ] {
            assert_eq!(
                shown(source, &[(source, "organization")]),
                pairs(&[(expected, "ORG")]),
                "{source}"
            );
        }
    }

    #[test]
    fn role_surnames_and_initials_survive_without_other_mentions() {
        for name in [
            "R. Masters",
            "Masters",
            "Stewards",
            "Master",
            "Steward",
            "Judge",
            "Marshal",
            "Sheriff",
            "Bishop",
            "Chancellor",
            "Clerk",
            "Mayor",
            "Dean",
            "Sergeant",
            "Banker",
            "Cook",
            "Porter",
            "Baker",
            "Taylor",
            "Mason",
            "Carter",
            "Fowler",
            "Судья",
            "Староста",
            "Голова",
            "Кузнец",
            "Бухгалтер",
        ] {
            assert_eq!(
                shown(name, &[(name, "person")]),
                pairs(&[(name, "PERSON")]),
                "{name}"
            );
        }
        // Exercise every role word/stem, rather than protecting only Masters.
        for entry in pack().never.iter().filter(|entry| entry.person_role) {
            for word in &entry.words {
                let mut chars = word.chars();
                let capitalized =
                    chars.next().unwrap().to_uppercase().collect::<String>() + chars.as_str();
                for surname in [capitalized.clone(), format!("{capitalized}s")]
                    .into_iter()
                    .take(if entry.form == Form::Plural { 2 } else { 1 })
                {
                    let name = format!("R. {surname}");
                    assert_eq!(
                        shown(&name, &[(&name, "person")]),
                        pairs(&[(&name, "PERSON")]),
                        "{name}"
                    );
                }
            }
        }
        for role in ["masters", "judges", "Judges", "the Masters", "the Judges"] {
            assert!(shown(role, &[(role, "person")]).is_empty(), "{role}");
        }
        for (source, detected) in [("the Masters", "Masters"), ("these Judges", "Judges")] {
            assert!(
                shown(source, &[(detected, "person")]).is_empty(),
                "{source}"
            );
        }
    }

    #[test]
    fn standalone_role_modifiers_keep_person_and_organization_names() {
        for modifier in [
            "VICE",
            "Deputy",
            "Acting",
            "Assistant",
            "Senior",
            "Chief",
            "Lead",
            "Head",
            "General",
            "Principal",
            "Executive",
            "First",
            "Technical",
            "Managing",
            "Junior",
            "Independent",
            "Administrative",
            "Заместитель",
            "Главный",
            "Старший",
            "Ведущий",
            "Генеральный",
            "Независимый",
            "Конкурсный",
            "Финансовый",
            "Арбитражный",
            "Несовершеннолетний",
        ] {
            assert!(!pack().never(modifier), "{modifier}");
            for (label, category) in [("person", "PERSON"), ("organization", "ORG")] {
                assert_eq!(
                    shown(modifier, &[(modifier, label)]),
                    pairs(&[(modifier, category)]),
                    "{label}: {modifier}"
                );
            }
            let initialed = format!("R. {modifier}");
            assert_eq!(
                shown(&initialed, &[(&initialed, "person")]),
                pairs(&[(&initialed, "PERSON")]),
                "{initialed}"
            );
        }
        for role in [
            "Vice President",
            "Deputy Director",
            "Acting Director",
            "Assistant Director",
            "Senior Officer",
            "Chief Investigator",
            "Lead Agent",
            "Head of Delegation",
            "General Counsel",
            "Principal Investigator",
            "Executive Director",
            "Заместитель руководителя",
            "Главный бухгалтер",
            "Генеральный директор",
        ] {
            for label in ["person", "organization"] {
                assert!(shown(role, &[(role, label)]).is_empty(), "{label}: {role}");
            }
        }
    }

    #[test]
    fn explicit_capacity_definitions_are_roles_but_principal_alone_stays() {
        for source in [
            "The terms Principal and Attorney describe capacities in the specimen.",
            "The term Principal describes a capacity.",
        ] {
            assert!(
                shown(source, &[("Principal", "person")]).is_empty(),
                "{source}"
            );
        }
        for source in [
            "Principal",
            "The terms Principal and Attorney identify two people.",
            "The terms Principal and Penrow describe capacities.",
        ] {
            assert_eq!(
                shown(source, &[("Principal", "person")]),
                pairs(&[("Principal", "PERSON")]),
                "{source}"
            );
        }
    }

    #[test]
    fn role_surnames_after_said_and_possessives_survive() {
        // Only Masters is supplied: no other name seed can rescue it.
        for source in [
            "Willis said Masters did not author these two notes",
            "Willis testified that Masters did not sign.",
        ] {
            assert_eq!(
                shown(source, &[("Masters", "person")]),
                pairs(&[("Masters", "PERSON")]),
                "{source}"
            );
        }
        for before in [
            "said", "her", "his", "their", "its", "our", "my", "your", "a", "an", "this", "that",
            "этот",
        ] {
            let source = format!("{before} Masters");
            assert_eq!(
                shown(&source, &[("Masters", "person")]),
                pairs(&[("Masters", "PERSON")]),
                "{source}"
            );
        }
        for before in ["the", "these", "those", "such", "данный", "указанный"] {
            let source = format!("{before} Masters");
            assert!(
                shown(&source, &[("Masters", "person")]).is_empty(),
                "{source}"
            );
            for separator in [", ", ": ", " — "] {
                let source = format!("{before}{separator}Masters");
                assert_eq!(
                    shown(&source, &[("Masters", "person")]),
                    pairs(&[("Masters", "PERSON")]),
                    "{source}"
                );
            }
            let source = format!("{before} spoke to Masters");
            assert_eq!(
                shown(&source, &[("Masters", "person")]),
                pairs(&[("Masters", "PERSON")]),
                "{source}"
            );
        }
    }

    #[test]
    fn of_inside_an_organization_name_does_not_separate_a_role() {
        for company in [
            "Notary Public of America, Inc",
            "Notary Public of Quillon Ltd",
        ] {
            assert_eq!(
                shown(company, &[(company, "organization")]),
                pairs(&[(company, "ORG")]),
                "{company}"
            );
        }
    }

    #[test]
    fn organization_roles_need_separators_and_a_name_remainder() {
        for company in [
            "Notary Public Underwriters, Inc",
            "Team Leader Brannock Services Ltd",
            "конкурсного управляющего ООО «Листолёт»",
            "Notary Public, LLC",
        ] {
            assert_eq!(
                shown(company, &[(company, "organization")]),
                pairs(&[(company, "ORG")]),
                "{company}"
            );
        }
        for form in [
            "Inc", "LLC", "Ltd", "Corp", "Co.", "PLC", "LLP", "ООО", "АО",
        ] {
            let company = format!("Notary Public Underwriters, {form}");
            let expected = company.trim_end_matches('.');
            assert_eq!(
                shown(&company, &[(&company, "organization")]),
                pairs(&[(expected, "ORG")]),
                "{company}"
            );
        }
        for (source, name) in [
            ("Trustee for Quillon Ltd", "Quillon Ltd"),
            (
                "Trustee for the Brannock Family Trust",
                "Brannock Family Trust",
            ),
            ("Chief Investigator, Quillon Ltd", "Quillon Ltd"),
            ("Notary Public — Quillon Ltd", "Quillon Ltd"),
            ("Notary Public - Quillon Ltd", "Quillon Ltd"),
            ("Notary Public: Quillon Ltd", "Quillon Ltd"),
            ("Quillon Ltd, Notary Public", "Quillon Ltd"),
            ("Quillon Ltd — Notary Public", "Quillon Ltd"),
        ] {
            assert_eq!(
                shown(source, &[(source, "organization")]),
                pairs(&[(name, "ORG")]),
                "{source}"
            );
        }
    }

    #[test]
    fn comma_after_leading_person_role_comes_off_with_the_role() {
        for source in [
            "Chief Investigator, John Smith",
            "Chief Investigator,John Smith",
            "Team Leader, John Smith",
            "Заместитель руководителя, Иван Петров",
        ] {
            let name = source.split_once(',').unwrap().1.trim_start();
            assert_eq!(
                shown(source, &[(source, "person")]),
                pairs(&[(name, "PERSON")]),
                "{source}"
            );
        }
    }

    /// A role can also be a surname. Titles, initials, full names and
    /// later surname-only mentions must keep that real name (#385).
    #[test]
    fn probe_role_surnames_stay_names() {
        for name in [
            "Edda Leader",
            "Ivo Chair",
            "Edda Trustee",
            "Ivo Esquire",
            "Edda Investigator",
            "Ivo Participant",
            "Edda Clerk",
            "Лада Должник",
            "Иво Кассатор",
            "Лада Руководитель",
            "Иво Уполномоченный",
            "Лада Предприниматель",
            "Иво Правообладатель",
            "Подрядчиков",
            "Директоров",
            "Кредиторов",
            "Руководителев",
        ] {
            assert_eq!(
                shown(name, &[(name, "person")]),
                pairs(&[(name, "PERSON")]),
                "{name}"
            );
        }
        for (source, name) in [
            ("Mr Leader", "Leader"),
            ("Ms Trustee", "Trustee"),
            ("Dr. Chair", "Chair"),
            ("Ms Participant", "Participant"),
            ("Mr Team Leader", "Team Leader"),
            ("Ms Chief Investigator", "Chief Investigator"),
            ("господин Должник", "Должник"),
            ("госпожа Руководитель", "Руководитель"),
            ("господин Предприниматель", "Предприниматель"),
            ("госпожа Кредиторов", "Кредиторов"),
        ] {
            for detected in [source, name] {
                assert_eq!(
                    shown(source, &[(detected, "person")]),
                    pairs(&[(name, "PERSON")]),
                    "{detected}"
                );
            }
        }
        for (source, full, surname) in [
            ("Edda Leader met Leader", "Edda Leader", "Leader"),
            ("Иво Должник встретил Должник", "Иво Должник", "Должник"),
        ] {
            let raw: Vec<crate::RawEntity> = [
                (source.find(full).unwrap(), full),
                (source.rfind(surname).unwrap(), surname),
            ]
            .into_iter()
            .map(|(start, text)| crate::RawEntity {
                start,
                end: start + text.len(),
                label: "person".into(),
                score: 0.8,
                text: text.into(),
            })
            .collect();
            let found: Vec<(String, String)> = crate::recognition::proposals(source, &raw)
                .unwrap()
                .into_iter()
                .map(|span| (source[span.range].to_owned(), span.category))
                .collect();
            assert_eq!(
                found,
                pairs(&[(full, "PERSON"), (surname, "PERSON")]),
                "{source}"
            );
        }
        for name in ["J. R. Leader", "Должник И. В."] {
            assert_eq!(
                shown(name, &[(name, "person")]),
                pairs(&[(name, "PERSON")]),
                "{name}"
            );
        }
        // A role inside a longer model span cannot veto the whole name.
        for name in ["Trustee Edda Brannock", "Руководитель Лада Брюн"] {
            assert_eq!(
                shown(name, &[(name, "person")]),
                pairs(&[(name, "PERSON")]),
                "{name}"
            );
        }
        for name in ["Brannock Leader", "Trustee Brannock Ltd"] {
            assert_eq!(
                shown(name, &[(name, "organization")]),
                pairs(&[(name, "ORG")]),
                "{name}"
            );
        }
    }

    #[test]
    fn role_modifiers_without_a_role_head_keep_organization_names() {
        for name in ["General Accident", "Head First", "First General"] {
            assert_eq!(
                shown(name, &[(name, "organization")]),
                pairs(&[(name, "ORG")]),
                "{name}"
            );
        }
    }

    #[test]
    fn unlisted_capitalized_roles_and_plurals_are_not_surnames() {
        for (source, role) in [
            ("Defendants filed a motion.", "Defendants"),
            ("Plaintiffs", "Plaintiffs"),
            ("Tenants", "Tenants"),
            ("Directors", "Directors"),
            ("Specialist", "Specialist"),
            ("Notary", "Notary"),
            ("Esquire", "Esquire"),
            ("Recorder", "Recorder"),
            ("Treasurer", "Treasurer"),
            ("President", "President"),
        ] {
            assert!(shown(source, &[(role, "person")]).is_empty(), "{source}");
        }
        for name in ["Masters", "Stewards", "Judge", "Clerk", "Chair"] {
            assert_eq!(
                shown(name, &[(name, "person")]),
                pairs(&[(name, "PERSON")]),
                "{name}"
            );
        }
    }

    #[test]
    fn private_departments_are_not_organizations_but_company_forms_and_people_stay() {
        for name in [
            "Legal Team",
            "Sales Team",
            "Data Protection Officer",
            "Planning Inspector",
            "Acme Legal Team",
            "Acme Office",
            "Acme Department",
            "Acme Committee",
            "Acme Board",
            "Acme Unit",
            "Acme Division",
            "Acme Desk",
            "Ashmoor Advisory Board",
        ] {
            assert!(shown(name, &[(name, "organization")]).is_empty(), "{name}");
        }
        for name in [
            "Officer & Co",
            "Leader Group",
            "Trustee Ltd",
            "Acme Legal Team Ltd",
            "ООО «Acme Office»",
        ] {
            assert_eq!(
                shown(name, &[(name, "organization")]),
                pairs(&[(name, "ORG")]),
                "{name}"
            );
        }
        let name = "Brannock Officer";
        assert_eq!(shown(name, &[(name, "person")]), pairs(&[(name, "PERSON")]));
    }

    #[test]
    fn public_bodies_misclassified_as_people_still_go_after_role_changes() {
        for body in [
            "Министерство финансов",
            "Арбитражный суд города Москвы",
            "Department of Justice",
        ] {
            assert!(shown(body, &[(body, "person")]).is_empty(), "{body}");
        }
    }

    #[test]
    fn cross_provider_review_verified_safe_names_stay() {
        for name in [
            "Vice Media",
            "VICE",
            "Officer & Co",
            "Trustee Ltd",
            "Leader Group",
            "General Electric",
            "Master Builders Ltd",
            "ООО «Директор»",
            "ООО «Кредитор»",
            "ООО «Руководитель»",
            "Trustees of Princeton University",
            "Chief Executive Officer of Acme Ltd",
        ] {
            assert_eq!(
                shown(name, &[(name, "organization")]),
                pairs(&[(name, "ORG")]),
                "{name}"
            );
        }
        for (source, name) in [
            ("Судья", "Судья"),
            ("Начальник", "Начальник"),
            ("Кузнецов", "Кузнецов"),
            ("Должников", "Должников"),
            ("Judge Edda Brannock", "Judge Edda Brannock"),
            ("Sheriff Brannock", "Sheriff Brannock"),
            ("John Smith, Chief", "John Smith, Chief"),
            ("ИП ФИО3 К-Л.М.", "К-Л.М."),
        ] {
            assert_eq!(
                shown(source, &[(source, "person")]),
                pairs(&[(name, "PERSON")]),
                "{source}"
            );
        }
    }

    /// Russian form numbers, classifier codes, standards, a form's barcode
    /// line and waybill numbers are no identifiers; bank details, tax,
    /// registration and passport numbers and phones beside them stay
    /// (guide rules 6 and 8, issue #486).
    #[test]
    fn russian_form_numbers_and_codes_are_no_identifiers() {
        let source = "   7000  2034\nИНН\n\n2990  2026  КПП  Стр. 0 0 2\n\n\
            Форма по КНД 1111501   Форма № Р11001\n\n\
            Платёжное поручение, форма по ОКУД 0401060, ОКВЭД 47.11, 47.19, \
            ОКТМО 45382000, КБК 18210102010011000110, код ТН ВЭД 8471300000.\n\n\
            Помещение соответствует СП 2.13130.2020, ГОСТ Р 7.0.97-2016 и СНиП 2.01.07-85*.\n\n\
            Груз принят по ТН № 47 от 03.02.2026, ТТН № 112 и товарной накладной № 58, \
            счёт-фактура № 123/4.\n\n\
            Получатель: ООО «Вега», ИНН 7707083893, КПП 770701001, ОКПО 12345678, \
            р/с 40702810900000001234 в банке, БИК 044525225, к/с 30101810400000000225, \
            расчётный счёт № 40702810500000009876, паспорт 4510 123456, \
            тел. 8-800-555-35-35, +7 (495) 123-45-67.\n";
        let raw = [
            ("7000  2034", "tax_id"),
            ("2990  2026", "bank_account"),
            ("1111501", "tax_id"),
            ("Р11001", "tax_id"),
            ("0401060", "bank_account"),
            ("47.11", "tax_id"),
            ("45382000", "tax_id"),
            ("18210102010011000110", "bank_account"),
            ("8471300000", "tax_id"),
            ("2.13130.2020", "bank_account"),
            ("7.0.97-2016", "tax_id"),
            ("2.01.07-85", "tax_id"),
            ("ТН № 47", "bank_account"),
            ("112", "bank_account"),
            ("58", "bank_account"),
            ("123/4", "tax_id"),
            ("7707083893", "tax_id"),
            ("770701001", "tax_id"),
            ("12345678", "tax_id"),
            ("р/с 40702810900000001234", "bank_account"),
            ("044525225", "bank_account"),
            ("к/с 30101810400000000225", "bank_account"),
            ("расчётный счёт № 40702810500000009876", "bank_account"),
            ("4510 123456", "passport_number"),
            ("8-800-555-35-35", "phone_number"),
            ("+7 (495) 123-45-67", "phone_number"),
        ];
        assert_eq!(
            shown(source, &raw),
            pairs(&[
                ("7707083893", "TAX"),
                ("770701001", "TAX"),
                ("12345678", "TAX"),
                ("40702810900000001234", "BANK"),
                ("044525225", "BANK"),
                ("30101810400000000225", "BANK"),
                ("40702810500000009876", "BANK"),
                ("4510 123456", "IDENTITY"),
                ("8-800-555-35-35", "PHONE"),
                ("+7 (495) 123-45-67", "PHONE"),
            ])
        );
        // A joint venture's name after «СП» stays; so does a number after
        // a barcode-like group that is no barcode line.
        assert_eq!(
            shown(
                "Арендатор: СП «Восход», тел. 4951 2345 678.",
                &[
                    ("СП «Восход»", "organization"),
                    ("4951 2345 678", "phone_number")
                ]
            ),
            pairs(&[("СП «Восход»", "ORG"), ("4951 2345 678", "PHONE")])
        );
        // An account typed with one space on its own line is no barcode,
        // in either language.
        for source in [
            "Account number:\n1234 5678\n",
            "Номер счёта:\n1234 5678\nИНН\n",
        ] {
            assert_eq!(
                shown(source, &[("1234 5678", "bank_account")]),
                pairs(&[("1234 5678", "BANK")]),
                "{source:?}"
            );
        }
    }

    /// English patent and trademark schedules, labelled patent and form
    /// numbers, industry codes, a link's query token, a page footer and a
    /// toll-free prefix are no identifiers; an IBAN, a VAT number, a
    /// company number, a toll-free number and a phone stay (guide rules 6
    /// and 8, issue #486).
    #[test]
    fn english_patent_form_and_page_numbers_are_no_identifiers() {
        let source = "COUNTRY  APP NO.  FILING DATE  PATENT NUMBER  ISSUE DATE\n\
            AU  2099241803  30-Apr-29  2099241803  26-Sep-33\n\
            DE  DE 60 2090 031 448.0  29-Nov-31  2 424 911  23-Mar-36\n\
            EP  9739191.6  30-Apr-29  2286018  11-Jun-34\n\
            US  12/432,845  30-Apr-29  8,420,710  16-Apr-33\n\
            US  SIGNATURE SCRAPE  Registered  86920111  25-Feb-36  5187924\n\
            CA  SIGNATURE SCRAPE  Published  1769731  26-Feb-36\n\n\
            Patent US 99/123,456 (AU 2099000111), trademark Reg. No. 9876543, \
            form STD009X.UK, NAICS 999999; see https://example.invalid/p?srsltid=AbCd_123xYz \
            or call the toll-free \"800\" number.\n\nFinal PLZ\n\n   -12-\n\nDraft QRT\n   -13-\n\n\
            NAICS:\n335931\n\n\
            IBAN DE89 3704 0044 0532 0130 00, VAT No. GB 123 4567 89, \
            Company Registration No. 01234567, toll-free 1-800-555-0199, \
            phone +1 (202) 555-0164.\n";
        let raw = [
            ("AU", "organization"),
            ("2099241803", "tax_id"),
            ("DE 60 2090 031 448.0", "bank_account"),
            ("2 424 911", "tax_id"),
            ("9739191.6", "tax_id"),
            ("12/432,845", "tax_id"),
            ("8,420,710", "tax_id"),
            ("86920111", "national_id_number"),
            ("1769731", "national_id_number"),
            ("99/123,456", "tax_id"),
            ("2099000111", "tax_id"),
            ("9876543", "national_id_number"),
            ("STD009X.UK", "national_id_number"),
            ("999999", "tax_id"),
            ("AbCd_123xYz", "tax_id"),
            ("800", "phone_number"),
            ("PLZ\n\n   -12-", "phone_number"),
            ("QRT\n   -13-", "phone_number"),
            ("-12-", "phone_number"),
            ("335931", "tax_id"),
            ("IBAN DE89 3704 0044 0532 0130 00", "bank_account"),
            ("GB 123 4567 89", "tax_id"),
            ("01234567", "national_id_number"),
            ("1-800-555-0199", "phone_number"),
            ("+1 (202) 555-0164", "phone_number"),
        ];
        assert_eq!(
            shown(source, &raw),
            pairs(&[
                ("https://example.invalid/p?srsltid=AbCd_123xYz", "URL"),
                ("DE89 3704 0044 0532 0130 00", "BANK"),
                ("GB 123 4567 89", "TAX"),
                ("01234567", "IDENTITY"),
                ("1-800-555-0199", "PHONE"),
                ("+1 (202) 555-0164", "PHONE"),
            ])
        );
        // Out of a schedule's row, a company's table (a status column too)
        // and a sentence opening "US" keep their values, and so do a query's
        // values of digits alone, of one case or without digits (inside a
        // whole link they are its URL's, rule 7). A row ends on its own
        // line: the line after it keeps its name and passport.
        let source = "| UK | Acme Holdings Ltd | 01234567 | 01/02/2030 |\n\
            | UK | Bryn Holdings Ltd | Registered | 07654321 | 01/03/2030 |\n\n\
            US law applies to Vela Quist from 30-Apr-29, SSN 123-45-6789.\n\n\
            Query: inn=7799000011&doc=AB1234567&who=JaneMorgan.\n\n\
            AU  2099241803  30-Apr-29   \n\
            Contact: Jane Morgan, passport RM6841932, acct 2099000111223.\n";
        let raw = [
            ("Acme Holdings Ltd", "organization"),
            ("01234567", "national_id_number"),
            ("Bryn Holdings Ltd", "organization"),
            ("07654321", "national_id_number"),
            ("Vela Quist", "person"),
            ("123-45-6789", "tax_id"),
            ("7799000011", "tax_id"),
            ("AB1234567", "passport_number"),
            ("JaneMorgan", "person"),
            ("2099241803", "tax_id"),
            ("Jane Morgan", "person"),
            ("RM6841932", "passport_number"),
            ("2099000111223", "bank_account"),
        ];
        assert_eq!(
            shown(source, &raw),
            pairs(&[
                ("Acme Holdings Ltd", "ORG"),
                ("01234567", "IDENTITY"),
                ("Bryn Holdings Ltd", "ORG"),
                ("07654321", "IDENTITY"),
                ("Vela Quist", "PERSON"),
                ("123-45-6789", "TAX"),
                ("7799000011", "TAX"),
                ("AB1234567", "IDENTITY"),
                ("JaneMorgan", "PERSON"),
                ("Jane Morgan", "PERSON"),
                ("RM6841932", "IDENTITY"),
                ("2099000111223", "BANK"),
            ])
        );
    }

    /// The number shapes that are no tax, bank, identity or phone value
    /// alone, and the real values beside them (issue #486).
    #[test]
    fn grouped_numbers_guids_and_tokens_are_no_identifiers() {
        let keeps = |text: &str, category| pack().identifying(text, category);
        for (text, category) in [
            ("8,420,710", Category::Tax),
            ("12/432,845", Category::Bank),
            ("1,250,000.00", Category::Identity),
            ("2,000", Category::Phone),
            ("b539f4a9-4e5f-4abb-bec6-aae430c254f2", Category::Tax),
            ("B539F4A9-4E5F-4ABB-BEC6-AAE430C254F2", Category::Bank),
            (
                "AfmBOorJnJ4Ys9vL_zXDXNKRcwv2xdd4zUKdwjsQuHUWB4mbpigOS4O3",
                Category::Tax,
            ),
            ("AbcD_123xyzQ", Category::Phone),
            ("STD001Y2K.UK", Category::Identity),
            ("PLZ\n\n   -44-", Category::Phone),
            ("-44-", Category::Phone),
            ("– 12 –", Category::Phone),
            ("4510\r\n\r\n123456", Category::Identity),
        ] {
            assert!(!keeps(text, category), "{text:?}");
        }
        for (text, category) in [
            ("12345.67", Category::Bank),
            ("4510 123456", Category::Identity),
            ("GB29 NWBK 6016 1331 9268 19", Category::Bank),
            ("77l12345O7", Category::Tax),
            ("4510\n123456", Category::Identity),
            ("555-12-34", Category::Phone),
            ("8-800-555-35-35", Category::Phone),
            ("42", Category::Phone),
            ("RM6841932", Category::Identity),
            ("GB123456789", Category::Tax),
        ] {
            assert!(keeps(text, category), "{text:?}");
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
                "[[never]]\nmatch = \"whole\"\nwords = [\"vice\"]\nphrase_only = true",
                "phrase_only needs unless_named alone",
            ),
            (
                "[[never]]\nmatch = \"whole\"\nwords = [\"vice\"]\nunless_named = true\nphrase_only = true\nperson_role = true",
                "phrase_only needs unless_named alone",
            ),
            (
                "[[never]]\nmatch = \"whole\"\nwords = [\"vice\"]\nunless_named = true\nphrase_only = true\nlast_word_of = [\"ORG\"]",
                "phrase_only needs unless_named alone",
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
                "[[span]]\ncategory = \"PERSON\"\ntrim_start = ['x*']",
                "empty prefix",
            ),
            (
                "[[span]]\ncategory = \"PERSON\"\ntrim_start = ['(']",
                "span #1",
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
                "[[span]]\ncategory = \"ORG\"\nstrip_end = \".\"\nabsorbs = { ORG = '(?P<span>x)' }",
                "need a pattern",
            ),
            (
                "[[span]]\ncategory = \"ADDRESS\"\npattern = '\\d+'\nabsorbs = { ORG = 'x' }",
                "no `span` group",
            ),
            (
                "[[span]]\ncategory = \"ADDRESS\"\npattern = '\\d+'\nabsorbs = { NOPE = '(?P<span>x)' }",
                "span #1",
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
                "is not `lead`, `tail`, `docket` or a `party`",
            ),
            (
                "[[reference]]\npattern = 'x'\nroles = [\"plaintiff\"]",
                "only a cited case",
            ),
            (
                "[[reference]]\npattern = '(?P<party>x)'\nroles = [\"Plaintiff\"]",
                "not a lowercase word",
            ),
            (
                "[[reference]]\npattern = 'x'\nown_before = [\"this case\"]",
                "only a cited case",
            ),
            (
                "[[reference]]\npattern = '(?P<party>x)'\nown_before = [\"This case\"]",
                "not a lowercase word",
            ),
            ("[[variants]]\nname = \"Liz\"\nforms = [\"Liz\"]", "forms"),
            (
                "[[never]]\nmatch = \"whole\"\nwords = [\"a\"]\ntypo = 1",
                "typo",
            ),
            ("[[url]]", "needs zones or public domains"),
            ("[[url]]\nzones = [\"co.uk\"]", "is not a domain"),
            ("[[url]]\npublic = [\"Gov.ru\"]", "is not lowercase"),
            ("[[url]]\npublic = [\"gov\", \"gov\"]", "listed twice"),
        ] {
            let Err(error) = Pack::load(&[("test.toml", source)]) else {
                panic!("accepted {source:?}");
            };
            assert!(error.contains(problem), "{error}");
            assert!(error.starts_with("test.toml"), "{error}");
        }
    }

    /// ADR 0094: a public domain's subdomain is never listed beside it, in
    /// one file or across files.
    #[test]
    fn a_public_domain_inside_another_is_rejected() {
        let Err(error) = Pack::load(&[
            ("a.toml", "[[url]]\npublic = [\"gov.ru\"]"),
            ("b.toml", "[[url]]\npublic = [\"pravo.gov.ru\"]"),
        ]) else {
            panic!("accepted a listed parent's subdomain");
        };
        assert!(
            error.contains("\"pravo.gov.ru\" is inside \"gov.ru\""),
            "{error}"
        );
    }
}
