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
    /// The label opens its line: only whitespace and Markdown marks before
    /// it (a caption's "No." on a line of its own).
    #[serde(default)]
    line_start: bool,
    /// The labels follow the value ("a March 9, 1984 birth date"), with
    /// `between` after the value; the value starts a word.
    #[serde(default)]
    after: bool,
    /// With `after`: words one of which must sit just before the value ("a
    /// March 9, 1984 birth date"); `'s` stands for any possessive ("the
    /// claimant's"). Empty, and any word may.
    #[serde(default)]
    only_after: Vec<String>,
    /// With `after`: what must follow the label ("." after "г.р"; the end of
    /// a phrase, not a noun of a longer one: "birth date records").
    #[serde(default)]
    follow: Option<String>,
    /// Before its value only: no label joined to another by a slash, just
    /// before or after it, spaces aside ("ИНН/ОГРН", "ОКТМО / ОКПО"): such
    /// a list's values form one run the model reads whole (#563).
    #[serde(default)]
    unless_slashed: bool,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct ShapeEntry {
    category: String,
    /// A model value with fewer digits and capital letters together is not
    /// this category ("24", "[021]"); a code's capitals count ("NWBKGB2L").
    #[serde(default)]
    min_code_chars: usize,
    /// A model value with more lowercase letters than digits is not this
    /// category: words around a number ("Alternative Segment 3").
    #[serde(default)]
    lowercase_at_most_digits: bool,
    /// Value-level vetoes like `forbid`, which a label (`unless_in`) lifts.
    #[serde(default)]
    forbid_unlabelled: Vec<String>,
    #[serde(default)]
    require: Vec<String>,
    #[serde(default)]
    forbid: Vec<String>,
    /// What a court's own case number looks like, the whole span: a model
    /// span of this shape is no value of this category but a case number
    /// (`CASE`), its group `case` if it has one ("3 А28-3207/2026" is
    /// "А28-3207/2026"). A reference or a cited case still removes it.
    #[serde(default)]
    case: Vec<String>,
    /// Evidence that this entry's veto does not describe the value: a
    /// house number, or an explicit private-home naming cue before it.
    #[serde(default)]
    forbid_unless: Vec<String>,
    #[serde(default)]
    forbid_unless_after: Vec<String>,
    /// A model span inside one of these matches in its surrounding line
    /// is not this category (an amount's decimals or currency may be outside
    /// the model's span). Labelled values are proposed separately.
    #[serde(default)]
    forbid_in: Vec<String>,
    /// Matches exempting contextual/value vetoes and digit/letter counts:
    /// a labelled identifier stays even beside an amount. Ordinary require
    /// and forbid checks still apply.
    #[serde(default)]
    unless_in: Vec<String>,
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
    /// The last definite address component before a guessed town/recipient
    /// tail. Names and contact fields after this component end the match.
    #[serde(default)]
    continuation_after: Option<String>,
    #[serde(default)]
    continuation_stop: Option<String>,
    /// A regex that cuts a match into its spans: each piece between its
    /// matches is a span of its own (a passport's name fields between
    /// their `<` fillers).
    #[serde(default)]
    split: Option<String>,
    /// The least and most characters of each line of a whole match that is
    /// not blank, `lead` and `tail` included, without the whitespace at its
    /// ends, its backslashes and its `mark` groups.
    #[serde(default)]
    line_length: Option<[usize; 2]>,
    /// A regex: a span in which it matches is not proposed (a masked name,
    /// "XXXXX").
    #[serde(default)]
    skip: Option<String>,
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
    /// A lowercase list marker at a line or list break is no initial.
    #[serde(default)]
    list_letter: bool,
    /// With `extend`: lowercase words a span never grows over.
    #[serde(default)]
    unless_over: Vec<String>,
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
    /// Document evidence for a defined code series. Both this pattern and
    /// each evidence pattern capture `series`; a range may use `series2`.
    #[serde(default)]
    when: Vec<String>,
    /// Conditional references filter only these categories after fixes.
    #[serde(default)]
    categories: Vec<String>,
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
}

/// A cited case in the view's text: its range without `lead` and `tail`,
/// its parties' ranges, and whether a docket number cites it (its `docket`
/// group), which a caption may carry too.
struct Cited {
    range: std::ops::Range<usize>,
    parties: Vec<std::ops::Range<usize>>,
    docket: bool,
}

/// A defined label is not a name, but its shape alone proves nothing.
struct ConditionalReference {
    pattern: Regex,
    when: Vec<Regex>,
    categories: Vec<Category>,
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
        continuation_after: Option<Regex>,
        continuation_stop: Option<Regex>,
        /// Cuts a match into its spans: the pieces between its matches.
        split: Option<Regex>,
        /// How many characters each line of a whole match has.
        line_length: Option<std::ops::RangeInclusive<usize>>,
        /// A span in which it matches is not proposed.
        skip: Option<Regex>,
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
    list_letter: bool,
    /// Lowercase words a span never grows over ("exhibit" in "EXHIBIT A.
    /// WELK").
    unless_over: Vec<String>,
}

impl SpanFix {
    /// `range` without its `trim` words or phrases at either edge (a word's
    /// final period or colon with it; a comma before a trailing one goes
    /// with it), its `trim_start` prefixes and its `strip_end` characters.
    /// An entry with `extend` patterns takes its `trim_start` prefixes off
    /// only a span it `grew`: "HON WAI KEUNG" from the model stays whole.
    fn trimmed(
        &self,
        source: &str,
        mut range: std::ops::Range<usize>,
        grew: bool,
    ) -> std::ops::Range<usize> {
        let trim_start = if self.extend.is_empty() || grew {
            self.trim_start.as_slice()
        } else {
            &[]
        };
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
            if self.list_letter
                && let Some((marker, rest)) = text.split_once(char::is_whitespace)
                && let Some(letter) = marker
                    .strip_prefix('(')
                    .and_then(|s| s.strip_suffix(')'))
                    .or_else(|| marker.strip_suffix(['.', ')']))
                && (letter.len() == 1 && letter.as_bytes()[0].is_ascii_lowercase()
                    || matches!(
                        letter,
                        "ii" | "iii" | "iv" | "vi" | "vii" | "viii" | "ix" | "xi" | "xii"
                    ))
            {
                let line = source[..range.start]
                    .rsplit('\n')
                    .next()
                    .unwrap_or_default();
                let before = line.trim_matches(|c: char| c.is_whitespace() || "#>*_|-".contains(c));
                if before.is_empty() || before.ends_with([':', ';']) {
                    range.start = range.end - rest.len();
                    continue;
                }
            }
            // A `rest` group is the name the prefix must leave: only what
            // is before it comes off.
            if let Some(end) = trim_start
                .iter()
                .filter_map(|pattern| {
                    let found = pattern.captures(text)?;
                    Some(
                        found
                            .name("rest")
                            .map_or(found.get(0)?.end(), |rest| rest.start()),
                    )
                })
                .find(|&end| 0 < end && end < text.len())
            {
                range.start += end;
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
            conditional_references: Vec::new(),
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
                if entry.min_code_chars == 0
                    && !entry.lowercase_at_most_digits
                    && entry.forbid_unlabelled.is_empty()
                    && entry.require.is_empty()
                    && entry.forbid.is_empty()
                    && entry.case.is_empty()
                    && entry.forbid_in.is_empty()
                    && entry.unless_in.is_empty()
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
                let forbid_unlabelled = compile_all(&entry.forbid_unlabelled, false)?;
                let require = compile_all(&entry.require, false)?;
                if entry.forbid.is_empty()
                    && (!entry.forbid_unless.is_empty() || !entry.forbid_unless_after.is_empty())
                {
                    return Err(at(
                        "forbid_unless and forbid_unless_after need forbid".into()
                    ));
                }
                let forbid = Veto {
                    patterns: compile_all(&entry.forbid, false)?,
                    unless: compile_all(&entry.forbid_unless, false)?,
                    unless_after: compile_all(&entry.forbid_unless_after, false)?,
                };
                let case = compile_all(&entry.case, false)?;
                let forbid_in = entry
                    .forbid_in
                    .iter()
                    .map(|pattern| with_parts(pattern, &Default::default()).map_err(at))
                    .collect::<Result<Vec<_>, _>>()?;
                let unless_in = entry
                    .unless_in
                    .iter()
                    .map(|pattern| with_parts(pattern, &Default::default()).map_err(at))
                    .collect::<Result<Vec<_>, _>>()?;
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
                            min_code_chars: 0,
                            lowercase_at_most_digits: false,
                            forbid_unlabelled: Vec::new(),
                            require: Vec::new(),
                            forbid: Vec::new(),
                            case: Vec::new(),
                            forbid_in: Vec::new(),
                            unless_in: Vec::new(),
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
                shape.min_code_chars = shape.min_code_chars.max(entry.min_code_chars);
                shape.lowercase_at_most_digits |= entry.lowercase_at_most_digits;
                shape.forbid_unlabelled.extend(forbid_unlabelled);
                shape.require.extend(require);
                shape.forbid.push(forbid);
                shape.case.extend(case);
                shape.forbid_in.extend(forbid_in);
                shape.unless_in.extend(unless_in);
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
                // Spaces in a label match any run of whitespace; after its
                // value, only spaces on its line ("г. р", not "г.\nР").
                let space = if entry.after { r"[\p{Zs}\t]+" } else { r"\s+" };
                let labels: Vec<String> = entry
                    .labels
                    .iter()
                    .map(|label| {
                        label
                            .split_whitespace()
                            .map(regex::escape)
                            .collect::<Vec<_>>()
                            .join(space)
                    })
                    .collect();
                if entry.after && (entry.line_start || !entry.unless_after.is_empty()) {
                    return Err(at(
                        "a label after its value takes no line_start or unless_after".into(),
                    ));
                }
                if !entry.after && (!entry.only_after.is_empty() || entry.follow.is_some()) {
                    return Err(at(
                        "only a label after its value takes only_after or follow".into(),
                    ));
                }
                if entry.after && entry.unless_slashed {
                    return Err(at(
                        "only a label before its value takes unless_slashed".into()
                    ));
                }
                if let Some(label) = entry
                    .labels
                    .iter()
                    .find(|label| entry.after && **label != label.to_lowercase())
                {
                    return Err(at(format!("{label:?} after its value must be lowercase")));
                }
                let pattern = if entry.after {
                    // The value, what sits after it, then a label, its case
                    // as written: a capitalised one opens a field of its own
                    // ("Signed 12 March 2026 Date of birth: …", "12.03.2026
                    // г. Р.А. Иванов"). Where the value starts and ends,
                    // and what is before it and after the label, is checked
                    // after matching.
                    format!(
                        r"((?:{}))(?:{})\b(?:{})\b",
                        rust_classes(&entry.value),
                        rust_classes(&entry.between),
                        labels.join("|"),
                    )
                } else {
                    format!(
                        // Case is ignored in the label only. Where the value
                        // ends is checked after matching ("1988 г." ends in
                        // a dot). A note marker may follow the label, inside
                        // its emphasis or after it ("ИНН¹:", which the view
                        // reads "ИНН ¹:", or "**ИНН**¹:"; #448).
                        r"(?i)\b(?:{})\b(?-i:(?:[*_]*\p{{Zs}}?[¹²³⁰⁴-⁹]+)?(?:{})((?:{})))",
                        labels.join("|"),
                        rust_classes(&entry.between),
                        rust_classes(&entry.value),
                    )
                };
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
                if let Some(word) = entry.only_after.iter().find(|word| {
                    *word != "'s"
                        && (word.is_empty()
                            || *word != &word.to_lowercase()
                            || !word.chars().all(char::is_alphanumeric))
                }) {
                    return Err(at(format!("{word:?} is not one lowercase word or 's")));
                }
                let follow = entry
                    .follow
                    .as_ref()
                    .map(|follow| Regex::new(&format!("^(?:{})", rust_classes(follow))))
                    .transpose()
                    .map_err(|e| at(e.to_string()))?;
                let unless_slashed = entry
                    .unless_slashed
                    .then(|| {
                        let mut longest = labels.clone();
                        longest.sort_by_key(|label| std::cmp::Reverse(label.len()));
                        Regex::new(&format!(r"(?i)^(?:{})\b", longest.join("|")))
                    })
                    .transpose()
                    .map_err(|e| at(e.to_string()))?;
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
                    line_start: entry.line_start,
                    after: entry.after,
                    only_after: entry.only_after.clone(),
                    follow,
                    unless_slashed,
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
                    || !entry.person_before.is_empty()
                    || entry.list_letter
                    || !entry.unless_over.is_empty();
                if let Some(pattern) = &entry.pattern {
                    if fixes {
                        return Err(at(
                            "a pattern takes no trim, trim_start, extend, strip_end, person_before, list_letter or unless_over".into(),
                        ));
                    }
                    // A sized pattern's `mark` groups are what its lines'
                    // lengths leave out.
                    let groups: &[&str] = if entry.line_length.is_some() {
                        &["lead", "tail", "mark"]
                    } else {
                        &["lead", "tail"]
                    };
                    let pattern = with_groups(pattern, &entry.parts, groups, false).map_err(at)?;
                    let continuation_after = entry
                        .continuation_after
                        .as_ref()
                        .map(|pattern| {
                            let pattern = entry.parts.iter().fold(
                                pattern.clone(),
                                |pattern, (name, part)| {
                                    pattern.replace(&format!("{{{name}}}"), &format!("(?:{part})"))
                                },
                            );
                            Regex::new(&rust_classes(&pattern)).map_err(|e| e.to_string())
                        })
                        .transpose()
                        .map_err(at)?;
                    let continuation_stop = entry
                        .continuation_stop
                        .as_ref()
                        .map(|pattern| {
                            Regex::new(&rust_classes(pattern)).map_err(|e| e.to_string())
                        })
                        .transpose()
                        .map_err(at)?;
                    if continuation_stop.is_some() && continuation_after.is_none() {
                        return Err(at("continuation_stop needs continuation_after".into()));
                    }
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
                    let split = entry
                        .split
                        .as_ref()
                        .map(|split| Regex::new(&rust_classes(split)).map_err(|e| e.to_string()))
                        .transpose()
                        .map_err(at)?;
                    if split.as_ref().is_some_and(|split| split.is_match("")) {
                        return Err(at("split matches an empty piece".into()));
                    }
                    if split.is_some() && continuation_after.is_some() {
                        return Err(at("split takes no continuation".into()));
                    }
                    let line_length = match entry.line_length {
                        Some([least, most]) if least == 0 || most < least => {
                            return Err(at("line_length is [least, most], least 1".into()));
                        }
                        Some([least, most]) => Some(least..=most),
                        None => None,
                    };
                    let skip = entry
                        .skip
                        .as_ref()
                        .map(|skip| Regex::new(&rust_classes(skip)).map_err(|e| e.to_string()))
                        .transpose()
                        .map_err(at)?;
                    if skip.as_ref().is_some_and(|skip| skip.is_match("")) {
                        return Err(at("skip matches every span".into()));
                    }
                    pack.spans.push(Span::Pattern {
                        category,
                        pattern,
                        absorbs,
                        continuation_after,
                        continuation_stop,
                        split,
                        line_length,
                        skip,
                    });
                    continue;
                }
                if !entry.parts.is_empty()
                    || !entry.absorbs.is_empty()
                    || entry.continuation_after.is_some()
                    || entry.continuation_stop.is_some()
                    || entry.split.is_some()
                    || entry.line_length.is_some()
                    || entry.skip.is_some()
                {
                    return Err(at(
                        "parts, absorbs, continuations, split, line_length and skip need a pattern"
                            .into(),
                    ));
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
                if entry.list_letter
                    && !matches!(category, Category::Person | Category::Organization)
                {
                    return Err(at("only PERSON or ORG takes list_letter".into()));
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
                if !entry.unless_over.is_empty() && entry.extend.is_empty() {
                    return Err(at("unless_over needs extend patterns".into()));
                }
                for word in entry
                    .trim
                    .iter()
                    .chain(&entry.person_before)
                    .chain(&entry.unless_over)
                {
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
                        let regex =
                            Regex::new(&rust_classes(pattern)).map_err(|e| at(e.to_string()))?;
                        match regex.capture_names().flatten().find(|g| *g != "lead") {
                            Some(group) => Err(at(format!("extend group {group:?} is not `lead`"))),
                            // Only a person's growth reads a lead: the ORG
                            // checks (`grows_to_company`) take whole matches.
                            None if category != Category::Person
                                && regex.capture_names().flatten().next().is_some() =>
                            {
                                Err(at("only a PERSON extend takes a `lead`".into()))
                            }
                            None => Ok(regex),
                        }
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
                        if let Some(group) = regex.capture_names().flatten().find(|g| *g != "rest")
                        {
                            return Err(at(format!("trim_start group {group:?} is not `rest`")));
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
                    list_letter: entry.list_letter,
                    unless_over: entry.unless_over.clone(),
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
                if !entry.when.is_empty() {
                    if entry.categories.is_empty()
                        || !entry.roles.is_empty()
                        || !entry.own_before.is_empty()
                    {
                        return Err(at(
                            "when needs categories and takes no cited-case fields".into()
                        ));
                    }
                    let pattern = with_groups(
                        &entry.pattern,
                        &entry.parts,
                        &["lead", "tail", "series", "series2"],
                        false,
                    )
                    .map_err(at)?;
                    let when = entry
                        .when
                        .iter()
                        .map(|pattern| {
                            with_groups(pattern, &entry.parts, &["lead", "tail", "series"], false)
                                .map_err(at)
                        })
                        .collect::<Result<Vec<_>, _>>()?;
                    if std::iter::once(&pattern).chain(&when).any(|pattern| {
                        !pattern
                            .capture_names()
                            .flatten()
                            .any(|group| group == "series")
                    }) {
                        return Err(at("when and pattern need a series group".into()));
                    }
                    let categories = entry
                        .categories
                        .iter()
                        .map(|token| self::category(token).map_err(at))
                        .collect::<Result<Vec<_>, _>>()?;
                    pack.conditional_references.push(ConditionalReference {
                        pattern,
                        when,
                        categories,
                    });
                    continue;
                }
                if !entry.categories.is_empty() {
                    return Err(at("categories needs when".into()));
                }
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

    /// Where in `text`, a model span of `category`, a court's case number
    /// lies when the span has one of the category's `case` shapes: the
    /// span is that number (`CASE`), no value of the category (guide rule
    /// 5, #480).
    pub(crate) fn case_number(
        &self,
        text: &str,
        category: Category,
    ) -> Option<std::ops::Range<usize>> {
        let shape = self.shapes.iter().find(|s| s.category == category)?;
        shape.case.iter().find_map(|pattern| {
            let found = pattern.captures(text)?;
            found.name("case").or_else(|| found.get(0)).map(|m| {
                let start = m.start() + (m.as_str().len() - m.as_str().trim_start().len());
                start..m.start() + m.as_str().trim_end().len()
            })
        })
    }

    /// Whether a model span has its category's `shape` and is not a role
    /// phrase or public body by the `never` section. Common words name no
    /// one here: no context says otherwise.
    #[cfg(test)]
    pub(crate) fn identifying(&self, text: &str, category: Category) -> bool {
        self.unlabelled_shape(text, category)
            && self.identifying_named(text, category, &|_| false, false)
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
        self.identifying_named_after(text, category, named, titled, "")
    }

    fn identifying_named_after(
        &self,
        text: &str,
        category: Category,
        named: &dyn Fn(&str) -> bool,
        titled: bool,
        before: &str,
    ) -> bool {
        self.shaped_named_after(text, category, named, before)
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

    /// The UK central bank shares its name with a private US lender.
    /// An article and a bank-specific public relation must occur locally;
    /// a party label takes precedence. Uncertain mentions remain names.
    fn bank_of_england_in_context(&self, text: &str, range: std::ops::Range<usize>) -> bool {
        static PATTERNS: OnceLock<[Regex; 3]> = OnceLock::new();
        let [name, public_before, public_after] = PATTERNS.get_or_init(|| {
            [
                r"(?i)^[\s\p{P}]*(?P<article>the[\p{Zs}\t]+)?bank[\p{Zs}\t]+of[\p{Zs}\t]+england(?:['’]s)?[\s\p{P}]*$",
                r"(?i)\bgovernor[\p{Zs}\t]+of[\p{Zs}\t]+(?:the[\p{Zs}\t]+)?$",
                r"(?i)^(?:['’]s[\p{Zs}\t]+(?:monetary[\p{Zs}\t]+policy[\p{Zs}\t]+committee|prudential[\p{Zs}\t]+regulation[\p{Zs}\t]+authority)\b|[\p{Zs}\t]*,?[\p{Zs}\t]+(?:is[\p{Zs}\t]+)?the[\p{Zs}\t]+(?:uk['’]s|united[\p{Zs}\t]+kingdom['’]s|british)[\p{Zs}\t]+central[\p{Zs}\t]+bank\b)",
            ]
            .map(|pattern| Regex::new(pattern).expect("central bank context pattern"))
        });
        let Some(name) = name.captures(&text[range.clone()]) else {
            return false;
        };
        let boundary = |c| matches!(c, '.' | '!' | '?' | ';' | '\n' | '\r');
        let start = text[..range.start].rfind(boundary).map_or(0, |at| at + 1);
        let end = text[range.end..]
            .find(boundary)
            .map_or(text.len(), |at| range.end + at);
        let before = &text[start..range.start];
        let after = &text[range.end..end];
        let words = |part: &str| {
            part.split(|c: char| !c.is_alphanumeric())
                .filter(|word| !word.is_empty())
                .map(str::to_lowercase)
                .collect::<Vec<_>>()
        };
        let before_words = words(before);
        let labelled = |word: &str| {
            matches!(word, "lender" | "borrower" | "party" | "parties")
                || self.party_roles.contains(word)
                || word.strip_suffix('s').is_some_and(|word| {
                    matches!(word, "lender" | "borrower") || self.party_roles.contains(word)
                })
        };
        // Labels can wrap, include a party letter or precede a quoted
        // article outside the model span. Read the sentence across lines
        // for labels, while public evidence remains confined to the bank's line.
        let label_before = text[..range.start]
            .rsplit(['.', '!', '?', ';'])
            .next()
            .unwrap_or_default();
        let label_after = text[range.end..]
            .split(['.', '!', '?', ';'])
            .next()
            .unwrap_or_default();
        if words(label_before)
            .iter()
            .chain(words(label_after).iter())
            .any(|word| labelled(word) || word == "private")
            || self.beside_role(text, range)
        {
            return false;
        }
        let article =
            name.name("article").is_some() || before_words.last().is_some_and(|word| word == "the");
        article && (public_before.is_match(before) || public_after.is_match(after))
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
        static CUT_NAME: OnceLock<Regex> = OnceLock::new();
        static LAW_END: OnceLock<Regex> = OnceLock::new();
        if self.bank_of_england_in_context(text, range.clone()) {
            return true;
        }
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
        // Only these fixed phrases establish the missing head; a person's
        // name beside a court or an office is not part of that body (#378).
        let cut_name = CUT_NAME.get_or_init(|| {
            Regex::new(r"\b(?:High[ \t]+Court[ \t]+of[ \t]+Justice|Law[ \t]+Enforcement[ \t]+Office|Fire[ \t]+and[ \t]+Police[ \t]+Employees['’][ \t]+Retirement[ \t]+System)\b")
                .expect("cut public name pattern")
        });
        let line_start = text[..range.start].rfind('\n').map_or(0, |at| at + 1);
        let line_end = text[range.end..]
            .find('\n')
            .map_or(text.len(), |at| range.end + at);
        if cut_name.find_iter(&text[line_start..line_end]).any(|m| {
            line_start + m.start() <= range.start
                && range.end <= line_start + m.end()
                && self.public_body(m.as_str(), category, titled)
        }) {
            return true;
        }
        // A surname's lowercase particle opens a name, not a body's tail:
        // "начальник Отдела ⟦де Лаваль⟧".
        let particle = span
            .split(|c: char| c.is_whitespace() || c == '-')
            .next()
            .is_some_and(|word| NAME_PARTICLES.contains(&word));
        if span.chars().next().is_some_and(char::is_lowercase) && !particle {
            // One wrapped line belongs to the name; a paragraph break
            // does not connect a ministry with the following text.
            let before = &text[..range.start];
            let head = before.trim_end_matches([' ', '\t']);
            let head = head
                .strip_suffix('\n')
                .map_or(head, |head| head.trim_end_matches([' ', '\t', '\r']));
            let from = head
                .char_indices()
                .rev()
                .find(|(_, c)| !c.is_alphanumeric() && *c != '-')
                .map_or(0, |(at, c)| at + c.len_utf8());
            let word = &head[from..];
            if head.len() < range.start
                && word.chars().next().is_some_and(char::is_uppercase)
                // A bare office word does not establish a continuation.
                // Require institutional syntax in the phrase patterns,
                // joining only the one permitted wrap for those patterns.
                && self.public_body_by(
                    &text[from..range.end].replace(['\r', '\n'], " "),
                    category,
                    titled,
                    false,
                )
            {
                return true;
            }
        }
        let after = &text[range.end..];
        // A legal head alone may start the next document's heading.
        // Require a supported statutory title pattern, rather than the
        // generic never word "Act", and preserve a labelled signatory.
        let law_end = LAW_END.get_or_init(|| {
            Regex::new(r"^(?:[\p{Zs}\t]+(?:\r?\n[\p{Zs}\t]*)?|\r?\n[\p{Zs}\t]*)(?:Act|Code|Ordinance)(?:[\p{Zs}\t]*(?:[.,;:)]|$)|[\p{Zs}\t]+(?:19|20)\d{2}\b)")
                .expect("cut law title pattern")
        });
        let title_words: Vec<&str> = span.split_whitespace().collect();
        let (label, _) = Self::word_before(text, range.start);
        if title_words.len() >= 2
            && !self.beside_role(text, range.clone())
            && label != "for"
            && !self
                .never
                .iter()
                .any(|entry| entry.person_role && entry.has(&label))
            && title_words.iter().all(|word| {
                word.chars().next().is_some_and(char::is_uppercase)
                    && word
                        .chars()
                        .all(|c| c.is_alphabetic() || ['-', '\'', '’'].contains(&c))
                    || ["of", "the", "and", "for", "on", "&"].contains(word)
            })
            && let Some(found) = law_end.find(after)
            && self.public_body_by(
                &text[range.start..range.end + found.end()].replace(['\r', '\n'], " "),
                category,
                titled,
                false,
            )
        {
            return true;
        }
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
        static PENSION_TITLE: OnceLock<Regex> = OnceLock::new();
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
                r"(\p{Uppercase}(?:[\p{Alphabetic}\p{N}&. \t-]){1,39})[”\x22’']?\)",
            )))
            .expect("abbreviation pattern")
        });
        let pension_title = PENSION_TITLE.get_or_init(|| {
            Regex::new(&rust_classes(concat!(
                r"^(?:(?:\p{Uppercase}[\p{Alphabetic}'’-]*|of|the|and|&)[ \t]+){1,12}",
                r"(?:Pension|Retirement)[ \t]+System(?:[ \t]+of[ \t]+(?:the[ \t]+)?",
                r"(?:City|County)[ \t]+of[ \t]+\p{Uppercase}[\p{Alphabetic}'’-]*",
                r"(?:[ \t]+\p{Uppercase}[\p{Alphabetic}'’-]*){0,2})?$",
            )))
            .expect("public pension title pattern")
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
            let from = (letters.len() >= 2 && !short.as_str().chars().any(char::is_lowercase))
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
            // A public body can define a short name rather than initials
            // ("Brindle County Police Pension System (“Brindle P&P”)").
            // Require a copied name word plus initials in order, and an
            // independently public long form. Unrelated bracketed names
            // and a company's short name remain names (#378).
            let from = from.or_else(|| {
                let parts: Vec<&str> = short
                    .as_str()
                    .split([' ', '\t', '&'])
                    .filter(|p| !p.is_empty())
                    .collect();
                let copied = parts
                    .iter()
                    .any(|part| part.chars().any(char::is_lowercase));
                let acronym = parts
                    .iter()
                    .any(|part| part.chars().all(char::is_uppercase));
                (copied && acronym)
                    .then(|| {
                        (0..words.len()).rev().find(|&first| {
                            let long = &text[words[first].0..title.end()];
                            let mut remaining = words[first..].iter();
                            parts.iter().all(|part| {
                                if part.chars().all(char::is_uppercase) {
                                    part.chars().all(|letter| {
                                        remaining.by_ref().any(|(_, word)| {
                                            word.chars()
                                                .next()
                                                .is_some_and(|c| c.eq_ignore_ascii_case(&letter))
                                        })
                                    })
                                } else {
                                    // A geographic qualifier can be moved to
                                    // the front ("… of the City of Baltimore"
                                    // -> "Baltimore F&P"). It must still be
                                    // copied exactly from the public long form.
                                    words[first..]
                                        .iter()
                                        .any(|(_, word)| word.eq_ignore_ascii_case(part))
                                }
                            }) && pension_title.is_match(long)
                                && self.public_body(long, Category::Organization, false)
                        })
                    })
                    .flatten()
            });
            let short = short.as_str().trim().trim_end_matches('.').to_owned();
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
        self.shaped_named_after(text, category, named, "")
    }

    fn shaped_named_after(
        &self,
        text: &str,
        category: Category,
        named: &dyn Fn(&str) -> bool,
        before: &str,
    ) -> bool {
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
            && !shape.forbid.iter().any(|veto| veto.matches(text, before))
            && !shape.case.iter().any(|pattern| pattern.is_match(text))
            && shape.word.as_ref().is_none_or(|pattern| {
                category == Category::Person && self.role_person_name(text)
                    || self.names(text, words().filter(|w| pattern.is_match(w)), named)
            })
    }

    /// The shape checks a label lifts: `min_code_chars` across all groups
    /// of a value, `lowercase_at_most_digits` and `forbid_unlabelled`,
    /// with the same Unicode predicates as shape regexes.
    fn unlabelled_shape(&self, text: &str, category: Category) -> bool {
        self.shapes
            .iter()
            .find(|shape| shape.category == category)
            .is_none_or(|shape| {
                let digits = text.chars().filter(|c| c.is_numeric()).count();
                let capitals = text.chars().filter(|c| c.is_uppercase()).count();
                let lowercase = text.chars().filter(|c| c.is_lowercase()).count();
                digits + capitals >= shape.min_code_chars
                    && (!shape.lowercase_at_most_digits || lowercase <= digits)
                    && !shape
                        .forbid_unlabelled
                        .iter()
                        .any(|pattern| pattern.is_match(text))
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
        static EMPLOYEE: OnceLock<Regex> = OnceLock::new();
        let employee = EMPLOYEE.get_or_init(|| {
            Regex::new(r"(?i)\b(?:сотрудник|офицер|инспектор)\p{Alphabetic}{0,3}[\p{Zs}\t]+$")
                .expect("employee affiliation pattern")
        });
        let fixes = self.spans.iter().filter_map(|span| match span {
            Span::Fix(fix) => Some(fix),
            Span::Pattern { .. } => None,
        });
        for fix in fixes {
            if !detections.iter().any(|d| d.category == fix.category) {
                continue;
            }
            // A `lead` group is context the grown span leaves out ("Judge "
            // before "K Ashdale"). The next match is searched for from just
            // after this one's grown start, so its lead may be the end of
            // this one ("K Ashdale and L Brown").
            let matches: Vec<(std::ops::Range<usize>, bool)> = fix
                .extend
                .iter()
                .flat_map(|pattern| {
                    if !pattern.capture_names().flatten().any(|g| g == "lead") {
                        return pattern
                            .find_iter(source)
                            .map(|m| (m.range(), false))
                            .collect();
                    }
                    let mut found = Vec::new();
                    let mut at = 0;
                    while at <= source.len()
                        && let Some(captures) = pattern.captures_at(source, at)
                    {
                        let Some(whole) = captures.get(0) else { break };
                        let start = captures
                            .name("lead")
                            .map_or(whole.start(), |lead| lead.end());
                        if start < whole.end() {
                            found.push((start..whole.end(), true));
                        }
                        // Past this match's first grown character at least.
                        at = start + source[start..].chars().next().map_or(1, char::len_utf8);
                    }
                    found
                })
                .filter(|(m, _)| {
                    if !source[m.clone()].contains(['«', '"'])
                        || !self.public_body(&source[m.clone()], fix.category, false)
                    {
                        return true;
                    }
                    let start = source[..m.start]
                        .rfind(['\n', '.', ';'])
                        .map_or(0, |at| at + 1);
                    let before = &source[start..m.start];
                    // An inner agency quotation cannot overrule the
                    // private company enclosing it, in either category.
                    let enclosed = self.legal_form(before)
                        && (before.matches('«').count() > before.matches('»').count()
                            || before.matches('"').count() % 2 == 1);
                    // Even an explicit police department can be an
                    // employee's affiliation before a quoted nickname.
                    !enclosed && !employee.is_match(before)
                })
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
                // A match after a `lead` (a person's, #387) never grows
                // over any part of another category's span either: "ACME
                // LLC" stays the company's. Others may: a person's
                // institution grows over the model's organization in it.
                let crosses = |(m, led): &(std::ops::Range<usize>, bool)| {
                    order.iter().any(|&j| {
                        let other = &detections[j].range;
                        j != i
                            && other.start < m.end
                            && m.start < other.end
                            && !(m.start <= other.start && other.end <= m.end)
                    }) || *led
                        && detections.iter().any(|other| {
                            other.category != fix.category
                                && other.range.start < m.end
                                && m.start < other.range.end
                        })
                };
                // A person never grows back over a role or title before
                // it ("Ответчик" in "Ответчик Н. О." for the model's "Н.
                // О."); after it such a word is a surname ("J. R. Clerk").
                let over_never = |m: &std::ops::Range<usize>| {
                    fix.category == Category::Person
                        && source[m.start..range.start]
                            .split(|c: char| !c.is_alphanumeric() && c != '-')
                            .any(|word| {
                            !word.is_empty()
                                && (self.never(word)
                                    || fix.unless_over.contains(&word.to_lowercase()))
                        })
                        // A supported quoted institution may grow over
                        // its public head; extension patterns must first
                        // establish the institution, not an affiliation.
                        && !(source[m.clone()].contains(['«', '"'])
                            && self.public_body(&source[m.clone()], fix.category, false))
                };
                let grown = matches
                    .iter()
                    .filter(|found| {
                        let m = &found.0;
                        m.start <= range.start
                            && range.end <= m.end
                            && !crosses(found)
                            && !over_never(m)
                    })
                    .map(|(m, _)| m)
                    .max_by_key(|m| m.len())
                    .cloned()
                    .unwrap_or(range.clone());
                let trimmed = fix.trimmed(source, grown.clone(), grown != range);
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
        // Before citations, common-name evidence and propagation: a party
        // code must never become evidence that a real name was mentioned.
        for reference in &self.conditional_references {
            let code = |series: regex::Match<'_>| {
                let end = series.end()
                    + view.text[series.end()..]
                        .bytes()
                        .take_while(u8::is_ascii_digit)
                        .count();
                &view.text[series.start()..end]
            };
            let defined: std::collections::HashSet<&str> = reference
                .when
                .iter()
                .flat_map(|pattern| pattern.captures_iter(&view.text))
                .filter(|captures| {
                    let whole = captures.get(0).expect("group 0 is the match");
                    if !whole.as_str().ends_with(|c: char| c.is_ascii_digit()) {
                        return true;
                    }
                    // A role followed by a code may instead open a company
                    // name: "Claimant C1 Holdings Ltd, trading as C1". The
                    // whole continuation is checked, not just the next
                    // word; an uncertain name keeps its marks.
                    !self.company_continuation(&view.text[whole.end()..])
                })
                .filter_map(|captures| captures.name("series").map(code))
                .collect();
            let series: std::collections::HashSet<_> =
                defined.iter().map(|name| &name[..1]).collect();
            if series.is_empty() {
                continue;
            }
            struct Code<'a> {
                range: std::ops::Range<usize>,
                name: &'a str,
                established: bool,
                compound: bool,
            }
            let codes: Vec<_> = reference
                .pattern
                .captures_iter(&view.text)
                .map(|captures| {
                    let whole = captures.get(0).expect("group 0 is the match");
                    let start = captures.name("lead").map_or(whole.start(), |m| m.end());
                    let end = captures.name("tail").map_or(whole.end(), |m| m.start());
                    Code {
                        range: start..end.max(start),
                        name: code(captures.name("series").expect("checked at load")),
                        established: ["series", "series2"].into_iter().all(|group| {
                            captures
                                .name(group)
                                .is_none_or(|m| series.contains(m.as_str()))
                        }),
                        compound: captures.name("series2").is_some(),
                    }
                })
                .collect();
            // Join only code enumerations. Every member must have an
            // established series; an unknown series or real name keeps
            // the whole model span. Do this before looking for names so
            // "C1 and C2" is not evidence of a company's short form.
            let mut groups: Vec<std::ops::Range<usize>> = Vec::new();
            for (index, current) in codes.iter().enumerate() {
                if let Some(group) = groups.last_mut()
                    && let previous = &codes[group.end - 1]
                    && matches!(
                        view.text[previous.range.end..current.range.start].trim(),
                        "," | "and" | ", and" | ",and"
                    )
                {
                    group.end = index + 1;
                } else {
                    groups.push(index..index + 1);
                }
            }
            // A code that opens a kept name protects its short mentions
            // too (GUIDE rule 2), even if another party defines that code.
            let protected: std::collections::HashSet<_> = codes
                .iter()
                .enumerate()
                .filter(|(index, current)| {
                    let group = groups.iter().find(|g| g.contains(index)).unwrap();
                    self.company_continuation(&view.text[current.range.end..])
                        || detections.iter().any(|d| {
                            if !reference.categories.contains(&d.category) {
                                return false;
                            }
                            let range = view.view_range(d.range.clone());
                            current.range.start <= range.start
                                && range.start < current.range.end
                                && codes[group.end - 1].range.end < range.end
                        })
                })
                .map(|(_, current)| current.name)
                .collect();
            let ranges: Vec<_> = groups
                .into_iter()
                .filter(|group| {
                    codes[group.clone()]
                        .iter()
                        .all(|current| current.established && !protected.contains(current.name))
                        && (group.len() > 1
                            || codes[group.start].compound
                            || defined.contains(codes[group.start].name))
                })
                .map(|group| codes[group.start].range.start..codes[group.end - 1].range.end)
                .collect();
            detections.retain_mut(|d| {
                if !reference.categories.contains(&d.category) {
                    return true;
                }
                let range = view.view_range(d.range.clone());
                if ranges
                    .iter()
                    .any(|found| found.start <= range.start && range.end <= found.end)
                {
                    return false;
                }
                // A model may include the defining label with its real
                // name: "Mrs Penrow (C1)" must keep "Penrow" alone.
                if let Some(found) = ranges.iter().find(|found| {
                    range.start < found.start
                        && found.end == range.end
                        && view.text[(*found).clone()].starts_with('(')
                        && view.text[(*found).clone()].ends_with(')')
                }) {
                    let end = range.start + view.text[range.start..found.start].trim_end().len();
                    d.range.end = view.source_range(range.start..end).end;
                }
                !d.range.is_empty()
            });
        }
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
                && self.identifying_named_after(
                    text,
                    d.category,
                    &|word| {
                        d.category == Category::Person
                            && (titled || name_cells.contains(&d.range) || named.contains(word))
                    },
                    titled,
                    &view.text[..view.view_range(d.range.clone()).start],
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
                // New York's "-against-" too, on a line of its own.
                Regex::new(
                    r"(?:^|[\s|*_])(?P<caption>(?:v|vs)\.?|[Ii]n[\p{Zs}\t]+re|[Ee]x[\p{Zs}\t]+parte|(?:[Ii]n[\p{Zs}\t]+the[\p{Zs}\t]+)?[Mm]atter[\p{Zs}\t]+of)(?:[\s|*_]|$)|(?m:^)[\p{Zs}\t|*_]*(?P<against>-?(?:against|AGAINST)-?)[\p{Zs}\t|*_]*(?m:$)",
                )
                .expect("a pattern")
            })
            .captures_iter(text)
            .filter_map(|found| found.name("caption").or_else(|| found.name("against")))
            // A docket citation is a caption only where it opens its line
            // ("Maren Quill v. Branwick Freight LLC, Index No. …"), never
            // one cited in a sentence ("relies on Grant v. Finecy, Index
            // No. 650001/2030 (Sup. Ct. N.Y. 2030)") or after a signal.
            .find(|found| {
                holder(&found.range()).is_none_or(|case| {
                    cited[case].docket && opens_line(text, cited[case].range.start)
                        || cited_caption(text, &cited[case])
                })
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
        // A related proceeding of the parties cited by its docket alone
        // keeps every name it holds, its other party's too ("Brendt
        // Holloway v. Harrow, Adv. Proc. No. 31-01992 (Bankr. S.D.N.Y.
        // 2031)"): it is no precedent. A reported decision's other party
        // is a reference, as in any precedent.
        let mut i = 0;
        detections.retain(|d| {
            let range = &ranges[i];
            let related = holder(range).is_some_and(|case| earlier[case]);
            let keep = !doubtful[i]
                || if named(d) {
                    ours(&words(range.clone()))
                        || related && holder(range).is_some_and(|case| cited[case].docket)
                } else {
                    related
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
        self.identifying_with_organizations(source, range, text, category, &[])
    }

    /// Like `identifying_at`, with independently accepted model company
    /// names as evidence for a reference-looking short name (#499).
    pub(crate) fn identifying_with_organizations(
        &self,
        source: &str,
        range: std::ops::Range<usize>,
        text: &str,
        category: Category,
        organizations: &[&str],
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
            // A complete company name needs the same protection as one
            // that grows to its legal form. IDs assigned to a person are
            // private even when their value looks like a reference.
            let company = category == Category::Organization
                && (self.legal_form(text) || self.company_short_name(source, text) || {
                    let normalized = |name: &str| {
                        name.split_whitespace()
                            .collect::<Vec<_>>()
                            .join(" ")
                            .to_lowercase()
                    };
                    let short = normalized(text);
                    organizations.iter().any(|name| {
                        let full = normalized(name);
                        full == short
                            || full
                                .strip_prefix(&short)
                                .is_some_and(|tail| tail.starts_with(' '))
                    })
                });
            if !company && !Self::personal_identifier_context(source, range.start) {
                return false;
            }
        }
        let mut shape_exempt = false;
        if let Some(shape) = self.shapes.iter().find(|s| s.category == category)
            && (!shape.forbid_in.is_empty()
                || shape.min_code_chars > 0
                || shape.lowercase_at_most_digits
                || !shape.forbid_unlabelled.is_empty())
        {
            // OCR can put an entire document on one line. Bound both the
            // newline search and the decoded context per span; if an amount
            // needs more distant evidence, keep the possible identifier.
            let mut start = range.start.saturating_sub(300);
            while !source.is_char_boundary(start) {
                start += 1;
            }
            let mut end = range.end.saturating_add(300).min(source.len());
            while !source.is_char_boundary(end) {
                end -= 1;
            }
            start += source[start..range.start].rfind('\n').map_or(0, |i| i + 1);
            end = source[range.end..end]
                .find('\n')
                .map_or(end, |i| range.end + i);
            let view = mdoc_pii::view::View::new(&source[start..end]);
            let span = view.view_range(range.start - start..range.end - start);
            let inside = |pattern: &Regex| {
                span_ranges(pattern, &view.text)
                    .into_iter()
                    .any(|found| found.start <= span.start && span.end <= found.end)
            };
            let forbidden = |pattern: &Regex| {
                span_ranges(pattern, &view.text).into_iter().any(|found| {
                    found.start <= span.start
                        && span.end <= found.end
                        && !joins_numeric_fields(&view.text, found, span.clone())
                })
            };
            shape_exempt = shape.unless_in.iter().any(inside);
            if shape.forbid_in.iter().any(forbidden) && !shape_exempt {
                return false;
            }
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
        let before = mdoc_pii::view::decoded(
            source[..range.start]
                .rsplit('\n')
                .next()
                .unwrap_or_default(),
        );
        (shape_exempt || self.unlabelled_shape(text, category))
            && self.shaped_named_after(text, category, &|_| category == Category::Person, &before)
            && !quoted
    }

    /// An explicit personal-ID label immediately before the value takes
    /// precedence over a reference's shape (guide rule 6, #499).
    fn personal_identifier_context(source: &str, at: usize) -> bool {
        static LABEL: std::sync::LazyLock<Regex> = std::sync::LazyLock::new(|| {
            Regex::new(
                r"(?i)\b(?:badge(?:\s+(?:number|no\.?|id))?|(?:employee|staff)\s+(?:id|number|no\.?|badge)|passport(?:\s+(?:number|no\.?))?)[\s:*_#№.]*$",
            )
            .expect("constant personal identifier label pattern")
        });
        let start = source[..at].rfind('\n').map_or(0, |i| i + 1);
        LABEL.is_match(&mdoc_pii::view::decoded(&source[start..at]))
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
    /// a company. The same suffix comes off an existing person's span in
    /// a document citation ("Ostrand Decl. ¶ 3", #499).
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
            .filter(|d| matches!(d.category, Category::Organization | Category::Person))
        {
            let text = source[detection.range.clone()].trim_end();
            let Some((name, last)) = text.rsplit_once(char::is_whitespace) else {
                continue;
            };
            let last = last.trim_end_matches('.').to_lowercase();
            let name = name.trim_end();
            let wanted = name_words(name);
            // An existing PERSON needs citation punctuation after the
            // abbreviated document word. A surname at a sentence end
            // ("Ivo Decl.") or without the dot stays as the model gave it.
            let after = &source[detection.range.end..];
            let citation_tail = if text.ends_with('.') {
                Some(after)
            } else {
                after.strip_prefix('.')
            }
            .is_some_and(|after| {
                let after = after.trim_start_matches([' ', '\t']);
                let after = after
                    .strip_prefix(',')
                    .unwrap_or(after)
                    .trim_start_matches([' ', '\t']);
                static PAGE_LINE: std::sync::LazyLock<Regex> = std::sync::LazyLock::new(|| {
                    Regex::new(r"^\d+:\d+(?:[-–](?:\d+:)?\d+)?(?:$|[\s),;.\]])")
                        .expect("constant page-line pinpoint pattern")
                });
                // "Ex. 1 to Penrow Decl." explicitly names a document;
                // punctuation plus another span's given name does not.
                static EXHIBIT: std::sync::LazyLock<Regex> = std::sync::LazyLock::new(|| {
                    Regex::new(r"\bEx\.[\p{Zs}\t]+\d+[\p{Zs}\t]+to[\p{Zs}\t]+$")
                        .expect("constant exhibit document pattern")
                });
                static AT_PAGE: std::sync::LazyLock<Regex> = std::sync::LazyLock::new(|| {
                    // A clock time or a street address after "at"
                    // supplies no document pinpoint evidence.
                    Regex::new(r"^at[\p{Zs}\t]+\d+(?:[-–]\d+)?(?:[\p{Zs}\t]*$|[\p{Zs}\t]*[),;.\]])")
                        .expect("constant at-page pinpoint pattern")
                });
                after.starts_with('¶')
                    || PAGE_LINE.is_match(after)
                    || (after.is_empty() || after.starts_with([')', ';']))
                        && EXHIBIT.is_match(&source[..detection.range.start])
                    || AT_PAGE.is_match(after)
            });
            // "Ivo Affleck" corroborates "Ivo Aff", not an affidavit
            // label. Sharing only a given name proves no document label.
            let abbreviated_surname = people.iter().any(|person| {
                person.len() == wanted.len() + 1
                    && wanted
                        .iter()
                        .zip(person)
                        .all(|(a, b)| a.eq_ignore_ascii_case(b))
                    && person.last().is_some_and(|surname| {
                        surname.len() > last.len() && surname.to_lowercase().starts_with(&last)
                    })
            });
            if !words.iter().any(|word| **word == last)
                || wanted.is_empty()
                || abbreviated_surname
                || if detection.category == Category::Person {
                    !citation_tail
                } else {
                    !people
                        .iter()
                        .any(|person| wanted.iter().all(|word| person.contains(word)))
                }
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
                    // A value before its label starts after a space, a
                    // quote or a bracket: "23 March", not "123 March", nor
                    // a number's tail ("AB-12/04/1979", "No.12/04/1979").
                    let starts = !context.after
                        || source[..value.start()]
                            .chars()
                            .next_back()
                            .is_none_or(|c| c.is_whitespace() || "([«\"'“‘„".contains(c));
                    // A word `only_after` lists just before it, on its line
                    // or the one above ("a March 9, 1984 birth date"; a
                    // possessive for "'s": "the claimant's").
                    let led = context.only_after.is_empty() || {
                        let text = &source[..value.start()];
                        let text = text.trim_end_matches([' ', '\t', '\u{a0}']);
                        let text = text.strip_suffix('\n').unwrap_or(text);
                        let text = text.strip_suffix('\r').unwrap_or(text);
                        let text = text.trim_end_matches([' ', '\t', '\u{a0}']);
                        let word = &text[text
                            .trim_end_matches(|c: char| c.is_alphanumeric() || "'’".contains(c))
                            .len()..];
                        let word = word.to_lowercase();
                        context.only_after.contains(&word)
                            || (context.only_after.iter().any(|w| w == "'s")
                                && word.chars().count() > 2
                                && (word.ends_with("'s") || word.ends_with("’s")))
                    };
                    // What follows an `after` label: "г.р." closes with a
                    // dot, "birth date" ends its phrase.
                    let followed = context.follow.as_ref().is_none_or(|follow| {
                        found
                            .get(0)
                            .is_some_and(|whole| follow.is_match(&source[whole.end()..]))
                    });
                    // A date wrapped at a line break before its label ends
                    // its line, or punctuation follows it: "born March
                    // 9,\n1984, in Zenda", never the next line's number
                    // ("4 July\n1600 Pennsylvania Avenue") or words ("14
                    // March\n2026 hearing listed").
                    let wrapped = context.after
                        || !value.as_str().contains('\n')
                        || source[value.end()..]
                            .trim_start_matches([' ', '\t', '\u{a0}'])
                            .chars()
                            .next()
                            .is_none_or(|c| "\r\n.,;:)]»\"'”’".contains(c));
                    let start = found.get(0)?.start();
                    let before = source[..start]
                        .trim_end()
                        .rsplit(|c: char| !c.is_alphanumeric())
                        .next()
                        .unwrap_or_default()
                        .to_lowercase();
                    // Only whitespace and Markdown marks before a line's
                    // label, and no line just above it that ends in a comma:
                    // there a wrapped sentence or citation goes on ("Grant
                    // v. Finecy,\nNo. 02-23-00310-CV, 2031 Tex. App. LEXIS").
                    let opens = || {
                        let mut lines = source[..start].rsplit('\n');
                        lines
                            .next()
                            .unwrap_or_default()
                            .chars()
                            .all(|c| c.is_whitespace() || "#>*_|".contains(c))
                            && !lines
                                .next()
                                .is_some_and(|line| line.trim_end().ends_with(','))
                    };
                    // An initialism's label never continues another one:
                    // "C.A. No." inside "18 U.S.C.A. No. 1234-5".
                    let mut label = source[start..].chars();
                    let mut back = source[..start].chars().rev();
                    let inside = !context.after
                        && label.next().is_some_and(char::is_alphabetic)
                        && label.next() == Some('.')
                        && back.next() == Some('.')
                        && back.next().is_some_and(char::is_alphabetic);
                    // A label joined to another by a slash ("ИНН/ОГРН
                    // 7701234567/1027700132195") leaves the run to the
                    // model: its value may be the other label's.
                    let slashed = context.unless_slashed.as_ref().is_some_and(|label| {
                        let blank = [' ', '\t', '\u{a0}'];
                        let end = label.find(&source[start..]).map_or(0, |found| found.end());
                        source[..start].trim_end_matches(blank).ends_with('/')
                            || source[start + end..]
                                .trim_start_matches(blank)
                                .starts_with('/')
                    });
                    if !ends
                        || !starts
                        || !led
                        || !followed
                        || !wrapped
                        || inside
                        || slashed
                        || context.unless_after.contains(&before)
                        || (context.line_start && !opens())
                        || !self.takes(context, value.as_str())
                    {
                        return None;
                    }
                    Some(Detection {
                        range: value.range(),
                        category: context.category,
                        score: CONTEXT_SCORE,
                        // The review limits a date of birth's repeats by the
                        // label on the same side (#466, #363).
                        recognizer: if context.after {
                            Recognizer::ContextAfter
                        } else {
                            Recognizer::Context
                        },
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
                        .find(|context| !context.after && context.labels.contains(&label))
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

    /// Whether `rest`, the text after a role and its code, carries on a
    /// company or other name, so the code opens a name rather than
    /// defining a party code: more capitalized words ("C1 Holdings"), a
    /// place ("C1 of North Quay"), a legal form within a few words ("C1
    /// international holdings ltd") or a trading name ("C1 trading as").
    fn company_continuation(&self, rest: &str) -> bool {
        // A comma before a legal form belongs to the company, unlike a
        // comma before the next person or clause ("C1, Mrs Penrow"); OCR
        // may leave a space before it ("C1 , Inc").
        if rest
            .trim_start_matches([' ', '\t'])
            .strip_prefix(',')
            .is_some_and(|tail| {
                tail.split_whitespace()
                    .next()
                    .is_some_and(|word| self.legal_form(bare(word)))
            })
        {
            return true;
        }
        let clause = rest
            .split(['.', ',', ';', ':', '!', '?', '\n', '\r'])
            .next()
            .unwrap_or_default();
        let words: Vec<_> = clause
            .split_whitespace()
            .map(bare)
            .filter(|word| !word.is_empty())
            .take(5)
            .collect();
        let code = |word: &str| {
            (2..=3).contains(&word.len())
                && word.starts_with(['C', 'D', 'P', 'R'])
                && word.as_bytes()[1..].iter().all(u8::is_ascii_digit)
        };
        let role = |word: &str| {
            self.party_roles.iter().any(|role| {
                role.eq_ignore_ascii_case(word)
                    || word
                        .strip_suffix('s')
                        .is_some_and(|single| role.eq_ignore_ascii_case(single))
            })
        };
        let trading = |words: &[&str]| {
            words.windows(2).take(3).any(|pair| {
                pair[0].eq_ignore_ascii_case("trading") && pair[1].eq_ignore_ascii_case("as")
            }) || words
                .iter()
                .take(3)
                .any(|word| word.eq_ignore_ascii_case("t/a"))
        };
        // A company's name may wrap onto the next line ("Claimant C1\n
        // Holdings Ltd"): a legal form or trading name there continues it.
        // A role or another code ends the look ("First Claimant\nC1\nSecond
        // Claimant\nC2" is a heading layout).
        let wrapped: Vec<_> = rest
            .split(['.', ',', ';', ':', '!', '?'])
            .next()
            .unwrap_or_default()
            .split_whitespace()
            .map(bare)
            .filter(|word| !word.is_empty())
            .take_while(|word| !role(word) && !code(word))
            .take(5)
            .collect();
        if wrapped[..wrapped.len().min(4)]
            .iter()
            .any(|word| self.legal_form(word))
            || trading(&wrapped)
        {
            return true;
        }
        let capitalized = |word: &&str| {
            word.starts_with(char::is_uppercase)
                && !code(word)
                && !role(word)
                && !self.spans.iter().any(|span| {
                    matches!(span, Span::Fix(fix)
                        if fix.category == Category::Person
                            && fix.trim.iter().any(|title| title.eq_ignore_ascii_case(word)))
                })
        };
        words.first().is_some_and(capitalized)
            || (words.first() == Some(&"of") && words.get(1).is_some_and(capitalized))
            || words[..words.len().min(4)]
                .iter()
                .any(|word| self.legal_form(word))
            || trading(&words)
    }

    /// Whether `text`'s words are all legal forms ("АО", "LLC").
    fn only_legal_form(&self, text: &str) -> bool {
        let mut words = text
            .split(|c: char| !c.is_alphanumeric())
            .filter(|word| !word.is_empty())
            .peekable();
        words.peek().is_some() && words.all(|word| self.legal_form(word))
    }

    /// A reference-looking short name stays when the document also gives
    /// its company's legal form, ignoring case and a soft line wrap. No
    /// model span is required over the full name (guide rule 2).
    fn company_short_name(&self, source: &str, text: &str) -> bool {
        let source = mdoc_pii::view::decoded(source);
        let wording = text
            .split_whitespace()
            .map(regex::escape)
            .collect::<Vec<_>>()
            .join(r"[\p{Zs}\t]+");
        if wording.is_empty() {
            return false;
        }
        // Regex case folding preserves source offsets even for Unicode
        // letters whose lowercase spelling has a different byte length.
        let pattern = Regex::new(&format!(r"(?i){wording}")).expect("escaped company wording");
        pattern.find_iter(&source).any(|found| {
            // Check boundaries outside the matched wording so a name's
            // own final dot ("C.F.R.") remains part of the spelling.
            if source[..found.start()]
                .chars()
                .next_back()
                .is_some_and(char::is_alphanumeric)
            {
                return false;
            }
            let after = &source[found.end()..];
            if !after.starts_with(char::is_whitespace) {
                return false;
            }
            // Permit a soft wrap, but never a paragraph or heading break.
            let end = after.find('\n').map_or(after.len(), |at| {
                let next = &after[at + 1..];
                let next_end = next.find('\n').unwrap_or(next.len());
                let line = source[..found.end()]
                    .rsplit('\n')
                    .next()
                    .unwrap_or_default();
                if soft_break(line, &next[..next_end]) {
                    at + 1 + next_end
                } else {
                    at
                }
            });
            // Only contiguous name words can lead to the legal form;
            // prose or a sentence boundary ends the name.
            for word in after[..end].split_whitespace() {
                let punctuated = word.ends_with(['.', ',', ';', ')', '»', '”', '"']);
                let word = word.trim_end_matches(['.', ',', ';', ')', '»', '”', '"']);
                if self.legal_form(word) {
                    return true;
                }
                if punctuated
                    || word != "&"
                        && !(word
                            .chars()
                            .next()
                            .is_some_and(|c| c.is_uppercase() || c.is_ascii_digit())
                            && word
                                .chars()
                                .all(|c| c.is_alphanumeric() || matches!(c, '-' | '\'' | '’')))
                {
                    break;
                }
            }
            false
        })
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
    #[cfg(test)]
    pub(crate) fn spans(&self, source: &str) -> Vec<Detection> {
        self.spans_beside(source, &[])
            .into_iter()
            .map(|found| found.detection)
            .collect()
    }

    /// An address match `range` of `pattern` beside the `names`, or none
    /// (#362). One a name holds at its front edge ("ПЕТРОВ С. ИВАНОВИЧ, УЛ
    /// ЛЕСНАЯ, Д. 5" with the name found, "ООО КЕДРОВСКИЙ РАЙОН, УЛ …")
    /// starts where the pattern finds it again after the name, so neither
    /// is lost; a name the match absorbs ("Harrow House, 12 …") is its own
    /// part. A lone capital that ends it before a capitalised word is that
    /// word's initial, no house letter ("д. 5 В Петров" ends at "д. 5"). One
    /// that ends at "№ 2" before "-1234/2020" is a case number's head.
    fn address_beside(
        &self,
        source: &str,
        names: &[Detection],
        pattern: &Regex,
        mut range: std::ops::Range<usize>,
    ) -> Option<std::ops::Range<usize>> {
        let front = names
            .iter()
            .filter(|name| {
                matches!(name.category, Category::Person | Category::Organization)
                    && name.range.start <= range.start
                    && range.start < name.range.end
                    && name.range.end < range.end
                    && !self.absorbs(
                        source,
                        Category::Address,
                        range.clone(),
                        name.category,
                        name.range.clone(),
                    )
            })
            .map(|name| name.range.end)
            .max();
        if let Some(after) = front {
            let found = pattern.captures_at(source, after)?;
            let whole = found.get(0).expect("group 0 is the match");
            let start = found.name("lead").map_or(whole.start(), |lead| lead.end());
            let end = found
                .name("tail")
                .map_or(whole.end(), |tail| tail.start())
                .min(range.end);
            if start >= end {
                return None;
            }
            range = start..end;
        }
        let cyrillic_capital = |c: char| ('А'..='Я').contains(&c) || c == 'Ё';
        let mut back = source[range.clone()].chars().rev();
        let after = &source[range.end..];
        if let (Some(letter), Some(space), Some(digit)) = (back.next(), back.next(), back.next())
            && cyrillic_capital(letter)
            && matches!(space, ' ' | '\t')
            && digit.is_ascii_digit()
            && after.starts_with([' ', '\t'])
            && after
                .trim_start_matches([' ', '\t'])
                .starts_with(char::is_uppercase)
        {
            range.end -= letter.len_utf8() + space.len_utf8();
        }
        let text = &source[range.clone()];
        let digits = text.trim_end_matches(|c: char| c.is_ascii_digit());
        let mut ahead = source[range.end..].chars();
        let case_number = digits.len() < text.len()
            && digits.trim_end_matches([' ', '\t']).ends_with('№')
            && matches!(ahead.next(), Some('-' | '/'))
            && ahead.next().is_some_and(|c| c.is_ascii_digit());
        (!case_number && !range.is_empty()).then_some(range)
    }

    /// Matches beside existing evidence, all in view coordinates. Definite
    /// street/building components may contain names; a guessed town tail
    /// never consumes an independent PERSON/ORG or a contact field.
    pub(crate) fn spans_beside(&self, source: &str, names: &[Detection]) -> Vec<SpanMatch> {
        self.spans
            .iter()
            .filter_map(|span| match span {
                Span::Pattern {
                    category,
                    pattern,
                    continuation_after,
                    continuation_stop,
                    split,
                    line_length,
                    skip,
                    ..
                } => Some((
                    *category,
                    pattern,
                    continuation_after,
                    continuation_stop,
                    (split, skip),
                    line_length,
                )),
                Span::Fix(_) => None,
            })
            .flat_map(
                |(
                    category,
                    pattern,
                    continuation_after,
                    continuation_stop,
                    (split, skip),
                    line_length,
                )| {
                    span_ranges_sized(pattern, source, line_length.as_ref())
                        .into_iter()
                        .map(move |(mut range, whole)| {
                            if let Some(core) = continuation_after
                                .as_ref()
                                .and_then(|pattern| pattern.find(&source[range.clone()]))
                            {
                                let tail = range.start + core.end();
                                // A name crossing the definite component's
                                // edge ("Broadway Show") invalidates the
                                // guessed street; never cut it to a fragment.
                                if names.iter().any(|name| {
                                    matches!(
                                        name.category,
                                        Category::Person | Category::Organization
                                    ) && name.range.start < tail
                                        && tail < name.range.end
                                        && range.start < name.range.end
                                }) {
                                    range.end = range.start;
                                    return (range, whole);
                                }
                                let name = names
                                    .iter()
                                    .filter(|name| {
                                        matches!(
                                            name.category,
                                            Category::Person | Category::Organization
                                        ) && tail <= name.range.start
                                            && name.range.start < range.end
                                    })
                                    .map(|name| name.range.start)
                                    .min();
                                let field = continuation_stop
                                    .as_ref()
                                    .and_then(|pattern| pattern.find(&source[tail..range.end]))
                                    .map(|field| tail + field.start());
                                if let Some(end) = name.into_iter().chain(field).min() {
                                    range.end = range.start
                                        + source[range.start..end]
                                            .trim_end_matches(|c: char| {
                                                c.is_whitespace() || [',', ';', ':'].contains(&c)
                                            })
                                            .len();
                                    // Cutting before a recipient must not
                                    // promote a bare street ("Broadway").
                                    if !self.shaped_named_after(
                                        &source[range.clone()],
                                        category,
                                        &|_| false,
                                        &source[..range.start],
                                    ) {
                                        range.end = range.start;
                                    }
                                }
                            }
                            (range, whole)
                        })
                        // A split match's spans are the pieces between the
                        // split's matches ("KRENC<<LIDIIA" is two names); the
                        // whole match goes with each.
                        .flat_map(move |(range, whole)| match split {
                            Some(split) => pieces(split, source, range)
                                .into_iter()
                                .map(|piece| (piece, Some(whole.clone())))
                                .collect(),
                            None => vec![(range, None)],
                        })
                        .filter_map(move |(range, whole)| {
                            if category != Category::Address || range.is_empty() {
                                return Some((range, whole));
                            }
                            self.address_beside(source, names, pattern, range)
                                .map(|range| (range, whole))
                        })
                        // An address match before a colon ends in a field's
                        // label ("12 Oak Road, Email: …"), not the address's
                        // last part; a name before one is a speaker's ("ФИО1
                        // А. Б.: …").
                        .filter(move |(range, _)| {
                            !range.is_empty()
                                && skip
                                    .as_ref()
                                    .is_none_or(|skip| !skip.is_match(&source[range.clone()]))
                                && (category != Category::Address
                                    || !source[range.end..]
                                        .trim_start_matches([' ', '\t'])
                                        .starts_with(':'))
                        })
                        .map(move |(range, whole)| SpanMatch {
                            detection: Detection {
                                range,
                                category,
                                score: CONTEXT_SCORE,
                                recognizer: Recognizer::Span,
                            },
                            whole,
                        })
                },
            )
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

/// Whether a citation at `at` opens its line in `text`: only whitespace
/// and Markdown marks before it, and no citation signal first ("See Grant
/// v. Finecy, …": a sentence's first word joins the party).
fn opens_line(text: &str, at: usize) -> bool {
    let start = text[..at].rfind('\n').map_or(0, |i| i + 1);
    let first = text[at..]
        .split(|c: char| !c.is_alphabetic())
        .next()
        .unwrap_or_default();
    text[start..at]
        .chars()
        .all(|c| c.is_whitespace() || "#>*_|".contains(c))
        && !matches!(
            first,
            "See" | "see" | "Cf" | "cf" | "Accord" | "Compare" | "Contra" | "But" | "Also"
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
    if let Some(group) = pattern.capture_names().flatten().find(|group| {
        !groups.contains(group)
            && !(parties && is_party(group))
            && !(groups.contains(&"mark") && numbered(group, "mark"))
    }) {
        return Err(if parties {
            format!("group {group:?} is not `lead`, `tail`, `docket` or a `party`")
        } else if groups.contains(&"mark") {
            format!("group {group:?} is not `lead`, `tail` or a `mark`")
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

/// A contextual rejection must not join a candidate's digit groups to a
/// neighbouring numeric field through whitespace alone. A decimal touching
/// the candidate still connects an amount's fragments, including OCR-joined
/// decimals ("762,0323 058"), but "770 901 001 500,00" does not make the
/// first three groups money. Ambiguous columns keep the possible identifier.
fn joins_numeric_fields(
    source: &str,
    found: std::ops::Range<usize>,
    span: std::ops::Range<usize>,
) -> bool {
    let value = &source[span.clone()];
    let before = &source[found.start..span.start];
    let after = &source[span.end..found.end];
    // Include two characters on either side so a decimal separator just
    // outside the model span counts, without reading beyond this match.
    let start = span.start
        - before
            .chars()
            .rev()
            .take(2)
            .map(char::len_utf8)
            .sum::<usize>();
    let end = span.end + after.chars().take(2).map(char::len_utf8).sum::<usize>();
    if source.as_bytes()[start..end].windows(3).any(|part| {
        part[0].is_ascii_digit() && matches!(part[1], b',' | b'.') && part[2].is_ascii_digit()
    }) {
        return false;
    }
    (value.starts_with(|c: char| c.is_ascii_digit())
        && before.ends_with(char::is_whitespace)
        && before.trim_end().ends_with(|c: char| c.is_ascii_digit()))
        || (value.ends_with(|c: char| c.is_ascii_digit())
            && after.starts_with(char::is_whitespace)
            && after.trim_start().starts_with(|c: char| c.is_ascii_digit()))
}

/// A span pattern's matches without their `lead` and `tail` groups, the
/// context around a span ("адресу " before a city). Each search starts
/// where the last span ended, so one address's tail ("; ") can be the next
/// one's lead.
fn span_ranges(pattern: &Regex, source: &str) -> Vec<std::ops::Range<usize>> {
    span_ranges_sized(pattern, source, None)
        .into_iter()
        .map(|(range, _)| range)
        .collect()
}

/// [`span_ranges`] of the matches each of whose lines that is not blank,
/// `lead` and `tail` included, has `line_length` characters, without the
/// whitespace at its ends, its backslashes (Markdown's escapes and hard
/// breaks) and its `mark` groups (a blockquote's or a table's marks, a
/// label), each with its whole match.
fn span_ranges_sized(
    pattern: &Regex,
    source: &str,
    line_length: Option<&std::ops::RangeInclusive<usize>>,
) -> Vec<(std::ops::Range<usize>, std::ops::Range<usize>)> {
    let marks: Vec<&str> = pattern
        .capture_names()
        .flatten()
        .filter(|group| numbered(group, "mark"))
        .collect();
    let mut ranges = Vec::new();
    let mut at = 0;
    while at <= source.len()
        && let Some(found) = pattern.captures_at(source, at)
    {
        let whole = found.get(0).expect("group 0 is the match");
        let sized = line_length.is_none_or(|length| {
            let marked: Vec<_> = marks
                .iter()
                .filter_map(|mark| found.name(mark))
                .map(|mark| mark.range())
                .collect();
            let counted: String = whole
                .as_str()
                .char_indices()
                .filter(|(i, c)| {
                    let i = whole.start() + i;
                    *c != '\\' && !marked.iter().any(|mark| mark.contains(&i))
                })
                .map(|(_, c)| c)
                .collect();
            counted
                .lines()
                .map(str::trim)
                .filter(|line| !line.is_empty())
                .all(|line| length.contains(&line.chars().count()))
        });
        let start = found.name("lead").map_or(whole.start(), |lead| lead.end());
        let end = found
            .name("tail")
            .map_or(whole.end(), |tail| tail.start())
            .max(start);
        if !sized {
            at = whole.start()
                + source[whole.start()..]
                    .chars()
                    .next()
                    .map_or(1, char::len_utf8);
            continue;
        }
        if start < end {
            ranges.push((start..end, whole.range()));
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

/// Whether `group` is `base` or `base` with a number (`mark`, `mark2`).
fn numbered(group: &str, base: &str) -> bool {
    group
        .strip_prefix(base)
        .is_some_and(|rest| rest.chars().all(|c| c.is_ascii_digit()))
}

/// The pieces of `range` in `source` between the matches of `split`, none
/// empty: "KRENC", "LIDIIA" and "ARSENEVNA" of "KRENC<<LIDIIA<ARSENEVNA".
fn pieces(
    split: &Regex,
    source: &str,
    range: std::ops::Range<usize>,
) -> Vec<std::ops::Range<usize>> {
    let mut pieces = Vec::new();
    let mut start = range.start;
    for cut in split.find_iter(&source[range.clone()]) {
        pieces.push(start..range.start + cut.start());
        start = range.start + cut.end();
    }
    pieces.push(start..range.end);
    pieces.retain(|piece| !piece.is_empty());
    pieces
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
        assert!(pack.identifying(&"\u{11DE0}".repeat(5), Category::Phone));
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

    /// The case numbers the review shows for `source` without a model.
    fn cases(source: &str) -> Vec<String> {
        crate::recognition::proposals(source, &[])
            .unwrap()
            .into_iter()
            .filter(|span| span.category == "CASE")
            .map(|span| source[span.range].to_owned())
            .collect()
    }

    /// Issue #364: US caption labels give the document's own case number,
    /// the label and a judge's initials outside.
    #[test]
    fn us_caption_labels_propose_their_case_number() {
        for (source, number) in [
            ("§ CIVIL ACTION NO. 5:31-cv-999 §", "5:31-cv-999"),
            ("CIVIL ACTION NO.: 4:31-cv-2999", "4:31-cv-2999"),
            ("Civil Action No. § 4:31-cv-2999", "4:31-cv-2999"),
            ("Civil Action No. 31-2999 (ZZ)", "31-2999"),
            ("C.A. No. 2031-0999-ZZ", "2031-0999-ZZ"),
            ("under Civil Docket No. 1:31-cv-00999.", "1:31-cv-00999"),
            ("**Docket No.:** 1:31-cv-04731-QL", "1:31-cv-04731-QL"),
            ("INDEX NO. 31 CV99999", "31 CV99999"),
            ("Index No. 652134/2031", "652134/2031"),
            ("Trial Court No. Q2031999", "Q2031999"),
            ("Trial Court No. 352-364031-31", "352-364031-31"),
            ("Cause No. 2031-12345", "2031-12345"),
            ("in cause number 09-31-00999-CV.", "09-31-00999-CV"),
            ("No. 09-31-00999-CV\n", "09-31-00999-CV"),
            ("___\n**NO. 09-31-00999-CV**\n___", "09-31-00999-CV"),
            ("| No. 09-31-00999-CV |", "09-31-00999-CV"),
            ("Adv. Pro. No. 31-01992", "31-01992"),
            ("Adversary Proceeding No. 31-01992 (ZZ)", "31-01992"),
            (
                "a Charge of Discrimination (#460-2031-01234) with the EEOC",
                "460-2031-01234",
            ),
            ("EEOC Charge No. 460-2031-01234", "460-2031-01234"),
        ] {
            assert_eq!(cases(source), [number], "{source}");
        }
    }

    /// The new labels never take a statute's, an article's, a docket
    /// entry's, a page's or a date's number, nor a precedent's docket in its
    /// citation (#506 item 3) or a Texas number away from a caption line.
    #[test]
    fn us_case_labels_leave_statutes_entries_pages_dates_and_precedents() {
        for source in [
            "Civil Action No. 12 under 42 U.S.C. § 1983 and Article 12.",
            "a civil action no later than 2031-03-01",
            "See Docket No. 45 and Docket No. 1234 at page 2031.",
            "Docket No. 2031-03-01",
            "Index No. 12, page 2031",
            "a service charge of $1234 and a charge no. 2031",
            "for good cause no 1234 days",
            "In No. 02-23-00310-CV the court held, as in No. 02-23-00311-CV.",
            "Proof of Claim No. 5",
        ] {
            assert!(cases(source).is_empty(), "{source}");
        }
        // A precedent's docket is part of its citation; the same label for
        // the case at hand, or an earlier case of its parties, is CASE.
        let caption = "Maren Quill v. Branwick Freight LLC\n\nIndex No. 652134/2031\n\n";
        for citation in [
            "Grant v. Finecy, Docket No. 2031-0099 (N.J. Super. 2031)",
            "Grant v. Finecy, Index No. 31 CV99999 (Sup. Ct. N.Y. 2031)",
            "Grant v. Finecy, Index No. 650001/2030 (Sup. Ct. N.Y. 2031)",
            "Grant v. Finecy, Cause No. 2031-12345 (Tex. Dist. Ct. 2031)",
            "Grant v. Finecy, Civil Docket No. 1:30-cv-00977, 2031 WL 1234567",
            "Grant v. Finecy, Civil Action No. 1:30-cv-00977 (D. Del. 2031)",
            "Grant v. Finecy, No. 02-23-00310-CV (Tex. App. 2031)",
        ] {
            let source = format!("{caption}The court relied on {citation}.");
            assert_eq!(cases(&source), ["652134/2031"], "{source}");
        }
        let source = format!(
            "{caption}It denied a motion in Quill v. Branwick Freight LLC, Index No. 650001/2030 (Sup. Ct. N.Y. 2030)."
        );
        assert_eq!(cases(&source), ["652134/2031", "650001/2030"]);
    }

    /// The Russian labels of #484 item 7 and an act's header (issue #364):
    /// a case's УИД, an appellate proceeding's number and an administrative
    /// protocol's number are CASE, the label and a date outside.
    #[test]
    fn russian_case_labels_take_uid_appeal_and_protocol_numbers() {
        for (source, number) in [
            (
                "г. Энск Дело № А99-9999-99/2031 01 марта 2031 г.",
                "А99-9999-99/2031",
            ),
            (
                "УИД 77RS0001-02-2031-009999-99",
                "77RS0001-02-2031-009999-99",
            ),
            (
                "**УИД:** 25RS0010-01-2031-001234-56",
                "25RS0010-01-2031-001234-56",
            ),
            (
                "апелляционное производство № 05АП-1234/2031",
                "05АП-1234/2031",
            ),
            (
                "по апелляционному производству №05АП-1234/2031",
                "05АП-1234/2031",
            ),
            (
                "протокол об административном правонарушении № 99 АБ 123456 составлен",
                "99 АБ 123456",
            ),
            (
                "протоколом об административном правонарушении от 01.03.2031 № 18810099310001234567",
                "18810099310001234567",
            ),
            (
                "протоколом № 77 МР 0123456 об административном правонарушении",
                "77 МР 0123456",
            ),
        ] {
            assert_eq!(cases(source), [number], "{source}");
        }
        for source in [
            "протокол № 3 общего собрания участников от 01.03.2031",
            "протоколом № 12/2031 заседания совета",
            "по статье 12.8 КоАП РФ, часть 1 статьи 1234",
            "протокол об административном правонарушении № 12",
            "УИД документа не указан, страница 2031",
        ] {
            assert!(cases(source).is_empty(), "{source}");
        }
    }

    /// #553 item 7 and issue #364: the document's own labelled case number
    /// is CASE wherever it repeats, in any letter case, label or not; a page
    /// or entry number beside it stays outside.
    #[test]
    fn a_labelled_case_number_reaches_its_repeats_in_any_letter_case() {
        assert_eq!(
            cases("АПЕЛЛЯЦИОННОЕ ОПРЕДЕЛЕНИЕ № 33А-2090/2031\n\nпо делу № 33а-2090/2031"),
            ["33А-2090/2031", "33а-2090/2031"]
        );
        assert_eq!(
            cases(
                "CIVIL ACTION NO.: 4:31-cv-2999\n\n4:31-cv-2999 12\n\nCase 4:31-CV-2999 Document 5 Page 3 of 9"
            ),
            ["4:31-cv-2999", "4:31-cv-2999", "4:31-CV-2999"]
        );
        // Not inside a longer number, nor where a precedent cites it.
        assert_eq!(
            cases("Civil Action No. 31-2999\n\nexhibit 31-2999-7 and 131-2999"),
            ["31-2999"]
        );
    }

    /// A case number's repeat in another letter case never takes a part of
    /// an email, an account or an address, which keep their values, the
    /// model's or the rules' (the #364 review, P1-1).
    #[test]
    fn a_case_repeat_leaves_emails_accounts_and_addresses_whole() {
        let email = "Claim No. CL-2026-000123\n\nWrite to claims+cl-2026-000123@insurer.example for updates.";
        let expected = pairs(&[
            ("CL-2026-000123", "CASE"),
            ("claims+cl-2026-000123@insurer.example", "EMAIL"),
        ]);
        assert_eq!(shown(email, &[]), expected);
        assert_eq!(
            shown(email, &[("claims+cl-2026-000123@insurer.example", "email")]),
            expected
        );
        for (source, raw, expected) in [
            (
                "Case No. KL-31-0042\n\nContact kl-31-0042@harrow-law.example.",
                ("kl-31-0042@harrow-law.example", "email"),
                [
                    ("KL-31-0042", "CASE"),
                    ("kl-31-0042@harrow-law.example", "EMAIL"),
                ],
            ),
            (
                "Claim No. GB-2031-4471\n\nPay to account gb-2031-4471-0098 at Brindle Bank.",
                ("gb-2031-4471-0098", "bank_account"),
                [("GB-2031-4471", "CASE"), ("gb-2031-4471-0098", "BANK")],
            ),
            (
                "Case No. 2A-1234\n\nThe office is at Suite 2a-1234, 12 Harbour Road, Brindle, BR1 4QQ.",
                (
                    "Suite 2a-1234, 12 Harbour Road, Brindle, BR1 4QQ",
                    "address",
                ),
                [
                    ("2A-1234", "CASE"),
                    (
                        "Suite 2a-1234, 12 Harbour Road, Brindle, BR1 4QQ",
                        "ADDRESS",
                    ),
                ],
            ),
        ] {
            assert_eq!(shown(source, &[raw]), pairs(&expected), "{source}");
        }
        // Nor a web address's path or an email's local part.
        for source in [
            "Claim No. CL-2026-000123\n\nSee https://portal.insurer.example/claims/cl-2026-000123/status for updates.",
            "по делу № 33а-2090/2031\n\nE-mail: 33А-2090/2031@sud.example",
        ] {
            assert_eq!(cases(source).len(), 1, "{source}");
        }
        // The model reading the repeat itself as TAX makes it no other
        // value: the repeat on exactly that span is CASE.
        let source = "по делу № 33а-9999/2031\n\nДело № 33А-9999/2031 рассмотрено.";
        assert_eq!(
            shown(source, &[("33А-9999/2031", "tax_id")]),
            pairs(&[("33а-9999/2031", "CASE"), ("33А-9999/2031", "CASE")])
        );
    }

    /// A precedent's docket in the standard federal, Delaware and New York
    /// forms stays in its citation: "C.A. No.", "Civ. A. No.", a judge's
    /// initials, a docket entry's pin, a LEXIS or Slip Opinion reporter,
    /// and a table of authorities' wrapped "No." line (the #364 review,
    /// P1-2).
    #[test]
    fn a_precedents_docket_in_its_usual_forms_stays_unmarked() {
        let caption =
            "IVO HARROW, Plaintiff,\n\nv.\n\nQUILLON LTD, Defendant.\n\nCase No. 1:31-cv-00999\n\n";
        for citation in [
            "See Grant v. Finecy, Civil Action No. 21-2873 (ABJ), 2031 WL 1234567, at *3 (D.D.C. Mar. 1, 2031).",
            "See Grant v. Finecy, Index No. 650001/2030, 2031 N.Y. Slip Op. 31234(U) (Sup. Ct. N.Y. Cnty. 2031).",
            "The court relied on Grant v. Finecy, C.A. No. 2019-0123-JRS (Del. Ch. 2031).",
            "See Grant v. Finecy, Civ. A. No. 30-0977 (E.D. Pa. 2031).",
            "See Grant v. Finecy, Civil Action No. 1:30-cv-00977, ECF No. 45 at 3 (D. Del. Mar. 1, 2031).",
            "See Grant v. Finecy, Cause No. 2031-12345, 2031 Tex. App. LEXIS 1234 (Tex. App. 2031).",
            "Grant v. Finecy,\nNo. 02-23-00310-CV, 2031 Tex. App. LEXIS 1234 (Tex. App.—Fort Worth 2031)",
            "See Grant v. Finecy, Civil Action No. 21-2873 (ABJ) (D.D.C. 2031).",
        ] {
            let source = format!("{caption}{citation}");
            assert_eq!(cases(&source), ["1:31-cv-00999"], "{source}");
        }
        // A wrapped sentence's "No." is no caption either, cited or not.
        assert!(cases("as held in the appeal,\nNo. 02-23-00310-CV, the court").is_empty());
    }

    /// A docket's entries and their attachments are no case numbers, nor
    /// is a date, in prose or in a "Docket No" column (the #364 review,
    /// P2-1, P3 item 4); a case number's shapes after "Docket No." are.
    #[test]
    fn docket_entries_and_dates_are_no_docket_numbers() {
        for source in [
            "See the declaration (Docket No. 112-15) and the exhibit (Docket No. 1234-5).",
            "See Declaration of Ivo Harrow, Docket No. 1045-3, at 4.",
            "The motion (Docket Number 45-12) was denied.",
            "(Docket No. 10452)",
            "| Docket No | Filed |\n|---|---|\n| 112-15 | 01/03/2031 |\n| 1234 | 02/03/2031 |",
            "Docket No. 03-01-2031",
        ] {
            assert!(cases(source).is_empty(), "{source}");
        }
        for (source, number) in [
            ("Docket No. A-1234-30", "A-1234-30"),
            ("Docket No. 2031-0099", "2031-0099"),
            ("Docket No. MER-L-001234-31", "MER-L-001234-31"),
            ("Docket No. 09-31-00999-CV", "09-31-00999-CV"),
            ("Docket No. 31 CV 1234", "31 CV 1234"),
        ] {
            assert_eq!(cases(source), [number], "{source}");
        }
    }

    /// New York's "-against-" caption is the case at hand's, so a precedent
    /// cited in a sentence with its "Index No." is none (the #364 review,
    /// P2-2); a one-line caption with its index number still is.
    #[test]
    fn an_against_caption_leaves_a_cited_index_number_unmarked() {
        let source = "MAREN QUILL,\n\nPlaintiff,\n\n-against-\n\nBRANWICK FREIGHT LLC,\n\nDefendant.\n\nIndex No. 652134/2031\n\nPlaintiff relies on Grant v. Finecy, Index No. 650001/2030 (Sup. Ct. N.Y. 2030).";
        let raw = [
            ("MAREN QUILL", "person"),
            ("BRANWICK FREIGHT LLC", "organization"),
            ("Grant", "person"),
            ("Finecy", "person"),
        ];
        assert_eq!(
            shown(source, &raw),
            pairs(&[
                ("MAREN QUILL", "PERSON"),
                ("BRANWICK FREIGHT LLC", "ORG"),
                ("652134/2031", "CASE"),
            ])
        );
        // Without "-against-", a docket citation in a sentence is still no
        // caption.
        let source = source.replace("-against-", "");
        assert_eq!(cases(&source), ["652134/2031"]);
        let source = "Maren Quill v. Branwick Freight LLC, Index No. 652134/2031 (Sup. Ct. N.Y. Cnty. 2031)\n\nThe defendant moves to dismiss.";
        assert_eq!(
            shown(
                source,
                &[
                    ("Maren Quill", "person"),
                    ("Branwick Freight LLC", "organization")
                ]
            ),
            pairs(&[
                ("Maren Quill", "PERSON"),
                ("Branwick Freight LLC", "ORG"),
                ("652134/2031", "CASE"),
            ])
        );
    }

    /// A related proceeding of the parties cited by its docket keeps every
    /// name it holds, its other party's too, with its number (the #364
    /// review, P2-3).
    #[test]
    fn an_earlier_case_keeps_its_other_party() {
        let source = "In re Ivo Harrow\n\nChapter 7\n\nThe trustee sued in Brendt Holloway v. Harrow, Adv. Proc. No. 31-01992 (Bankr. S.D.N.Y. 2031).";
        let shown_here = shown(
            source,
            &[("Ivo Harrow", "person"), ("Brendt Holloway", "person")],
        );
        for expected in pairs(&[("Brendt Holloway", "PERSON"), ("31-01992", "CASE")]) {
            assert!(
                shown_here.contains(&expected),
                "{expected:?} in {shown_here:?}"
            );
        }
        let source = "IVO HARROW, Plaintiff,\n\nv.\n\nQUILLON LTD, Defendant.\n\nIndex No. 652134/2031\n\nHarrow had earlier sued in Harrow v. Pellam Corp., Index No. 650001/2030 (Sup. Ct. N.Y. 2030).";
        let shown_here = shown(
            source,
            &[
                ("IVO HARROW", "person"),
                ("QUILLON LTD", "organization"),
                ("Pellam Corp.", "organization"),
            ],
        );
        for expected in pairs(&[("Pellam Corp", "ORG"), ("650001/2030", "CASE")]) {
            assert!(
                shown_here.contains(&expected),
                "{expected:?} in {shown_here:?}"
            );
        }
    }

    /// The review's other wrong case numbers (#364, P3 items 1-4): a card's
    /// or a phone line's "charge", a "Charge" column of amounts, an OCR
    /// period glued to the value, "C.A." inside "U.S.C.A." and a price
    /// index; and the misses it found that are now proposed.
    #[test]
    fn charges_amounts_statutes_and_price_indexes_are_no_case_numbers() {
        assert_eq!(
            shown(
                "Your card was billed for charge #4532-0151-1283-0366 on 1 March.",
                &[("4532-0151-1283-0366", "bank_account")]
            ),
            pairs(&[("4532-0151-1283-0366", "BANK")])
        );
        assert_eq!(
            shown(
                "For questions about this charge No. 555-201-4477 is open daily.",
                &[("555-201-4477", "phone_number")]
            ),
            pairs(&[("555-201-4477", "PHONE")])
        );
        for source in [
            "| Date | Charge | Payment |\n|---|---|---|\n| 01/03/2031 | 1234.50 | 0 |\n\nTotal due 1234.50.",
            "under 18 U.S.C.A. No. 1234-5 of the code",
            "Consumer Price Index No. 2031-03 for March",
            // The period ends the value, and the review then reads the
            // number as part of "31-2999.The": no wrong "31-2999.The".
            "Civil Action No. 31-2999.The plaintiff filed.",
        ] {
            assert!(cases(source).is_empty(), "{source}");
        }
        for (source, number) in [
            ("Charge No. 460-2031-01234 was dismissed.", "460-2031-01234"),
            ("CIVIL ACTION NO.:\n§ 4:31-cv-2999", "4:31-cv-2999"),
            (
                "протоколом об административном правонарушении от 01 марта 2031 г. № 99 АБ 123456",
                "99 АБ 123456",
            ),
        ] {
            assert_eq!(cases(source), [number], "{source}");
        }
        assert!(cases("CIVIL ACTION NO.\n\n4:31-cv-2999").is_empty());
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

    /// The person proposals for the model's `people` spans in `source`
    /// (each its first occurrence), as the user would be shown them.
    fn proposed_people(source: &str, people: &[&str]) -> Vec<String> {
        let raw: Vec<_> = people
            .iter()
            .map(|text| {
                let start = source.find(text).unwrap();
                crate::RawEntity {
                    start,
                    end: start + text.len(),
                    label: "person".into(),
                    score: 0.9,
                    text: (*text).into(),
                }
            })
            .collect();
        crate::recognition::proposals(source, &raw)
            .unwrap()
            .into_iter()
            .filter(|span| span.category == "PERSON")
            .map(|span| source[span.range].to_owned())
            .collect()
    }

    /// One initial before a surname joins it after a title, "by", "v",
    /// "per", "and" after a word or a judge's "J,", with its period or
    /// without; an all-caps given name joins "X. SURNAME" in a signature or
    /// caption position (#387). A list letter, a section letter, a sentence's
    /// "A", the pronoun "I", a comma, a colon, "to" or "(" before the letter,
    /// a role and a signature's verb do not.
    #[test]
    fn one_initial_or_an_all_caps_given_name_joins_a_surname() {
        for (source, model, expected) in [
            (
                "Before Judge K Ashdale and DORIAN P. WELK, Magistrate.",
                &["Ashdale", "P. WELK"][..],
                &["K Ashdale", "DORIAN P. WELK"][..],
            ),
            (
                "The order of Judge V Davies stands.",
                &["Davies"],
                &["V Davies"],
            ),
            (
                "Heard by Judge K. Ashdale today.",
                &["Ashdale"],
                &["K. Ashdale"],
            ),
            (
                "Signed by K Ashdale on Monday.",
                &["Ashdale"],
                &["K Ashdale"],
            ),
            ("Dear Ms A Brannock,", &["Brannock"], &["A Brannock"]),
            (
                "Smith v A Jones [2020] EWHC 1",
                &["Smith", "Jones"],
                &["Smith", "A Jones"],
            ),
            (
                "per K Ashdale-Brown",
                &["Ashdale-Brown"],
                &["K Ashdale-Brown"],
            ),
            (
                "Before Judge K Ashdale, L Brown and M Green.",
                &["Ashdale", "Brown", "Green"],
                &["K Ashdale", "Brown", "M Green"],
            ),
            (
                "Before: Smith J, K Brown and Green LJ",
                &["Smith", "Brown", "Green"],
                &["Smith", "K Brown", "Green"],
            ),
            (
                "/s/ DORIAN P. WELK\nMagistrate",
                &["P. WELK"],
                &["DORIAN P. WELK"],
            ),
            ("DORIAN P. WELK, Magistrate", &["WELK"], &["DORIAN P. WELK"]),
            ("SIGNED P. WELK\n", &["P. WELK"], &["P. WELK"]),
            // A section, list or answer letter, a sentence's article and the
            // pronoun stay out.
            ("under Part B Smith said", &["Smith"], &["Smith"]),
            ("See Exhibit A Smith for the plan.", &["Smith"], &["Smith"]),
            ("A Smith attended the site.", &["Smith"], &["Smith"]),
            ("It ended. A Smith attended.", &["Smith"], &["Smith"]),
            ("B. Smith\nC. Jones", &["Smith"], &["Smith"]),
            ("Then I Penrow it.", &["Penrow"], &["Penrow"]),
            (
                "See Schedule A and B Jones for the plan.",
                &["Jones"],
                &["Jones"],
            ),
            (
                "In Annex A, B Smith states the terms.",
                &["Smith"],
                &["Smith"],
            ),
            (
                "the holders of Class A and B Smith shares",
                &["Smith"],
                &["Smith"],
            ),
            ("Answer: A Smith attended.", &["Smith"], &["Smith"]),
            ("Amendments to A Jones Trust deed", &["Jones"], &["Jones"]),
            ("the witness ( B Smith ) said", &["Smith"], &["Smith"]),
            ("Q. Who signed it? A. Smith did.", &["Smith"], &["Smith"]),
            (
                "Present: Ivo Penrow, K Ashdale.",
                &["Ivo Penrow", "Ashdale"],
                &["Ivo Penrow", "Ashdale"],
            ),
            // A role, a heading, a caption's party word, a short word, a
            // "V." or a mid-line word before "X. SURNAME".
            ("JUDGE P. WELK presiding.", &["P. WELK"], &["P. WELK"]),
            ("BY P. WELK, counsel.", &["P. WELK"], &["P. WELK"]),
            ("Mixed Case P. WELK here.", &["P. WELK"], &["P. WELK"]),
            ("EXHIBIT A. WELK DECLARATION", &["WELK"], &["WELK"]),
            ("SCHEDULE B. WELK", &["WELK"], &["WELK"]),
            ("COUNT I. WELK", &["WELK"], &["WELK"]),
            ("COMMONWEALTH V. WELK", &["WELK"], &["WELK"]),
            ("PETITIONER J. DOE", &["J. DOE"], &["J. DOE"]),
            (
                "SMITH V. WELK, No. 21-cv-104",
                &["SMITH", "WELK"],
                &["SMITH", "WELK"],
            ),
            ("A note on DORIAN P. WELK here", &["P. WELK"], &["P. WELK"]),
            // A model's own name keeps its start.
            (
                "Witness: HON WAI KEUNG",
                &["HON WAI KEUNG"],
                &["HON WAI KEUNG"],
            ),
            (
                "Witness: Hon Wai Keung",
                &["Hon Wai Keung"],
                &["Hon Wai Keung"],
            ),
            // A judge's title the model took in comes off before an initial
            // and surname or a given name and surname; a role surname or a
            // given name stays.
            (
                "Before Judge K Ashdale and DORIAN P. WELK, Magistrate.",
                &["Judge K Ashdale", "DORIAN P. WELK"],
                &["K Ashdale", "DORIAN P. WELK"],
            ),
            (
                "Heard by Justice Mary Ashdale.",
                &["Justice Mary Ashdale"],
                &["Mary Ashdale"],
            ),
            (
                "Heard by Hon. K Ashdale.",
                &["Hon. K Ashdale"],
                &["K Ashdale"],
            ),
            ("Judge", &["Judge"], &["Judge"]),
            ("Mary Judge agreed.", &["Mary Judge"], &["Mary Judge"]),
            ("Then Mr Judge agreed.", &["Mr Judge"], &["Judge"]),
            (
                "Judge Reinhold agreed.",
                &["Judge Reinhold"],
                &["Judge Reinhold"],
            ),
        ] {
            assert_eq!(proposed_people(source, model), expected, "{source}");
        }
        // A company's span is never grown into, whatever the initial.
        for (source, company) in [
            ("ACME LLC V. WELK, No. 21-cv-104", "ACME LLC"),
            ("ACME CORP V. WELK, No. 21-cv-104", "ACME CORP"),
            ("PELLOW HOLDINGS INC V. WELK", "PELLOW HOLDINGS INC"),
            ("/s/ ACME LLC P. WELK", "ACME LLC"),
            ("/s/ ACMECO P. WELK", "ACMECO"),
        ] {
            let start = source.find(company).unwrap();
            let welk = source.find("WELK").unwrap();
            let raw = [
                crate::RawEntity {
                    start,
                    end: start + company.len(),
                    label: "organization".into(),
                    score: 0.9,
                    text: company.into(),
                },
                crate::RawEntity {
                    start: welk,
                    end: welk + 4,
                    label: "person".into(),
                    score: 0.9,
                    text: "WELK".into(),
                },
            ];
            let shown: Vec<_> = crate::recognition::proposals(source, &raw)
                .unwrap()
                .into_iter()
                .map(|span| (span.category, source[span.range].to_owned()))
                .collect();
            assert_eq!(
                shown,
                [
                    ("ORG".to_owned(), company.to_owned()),
                    ("PERSON".to_owned(), "WELK".to_owned())
                ],
                "{source}"
            );
        }
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

    /// Recheck the outstanding #378 probe shapes against the current pack,
    /// as both categories the model used. Collect failures to see the gaps
    /// together before extending the existing public-body rules.
    #[test]
    fn public_bodies_round_three_reported_names() {
        let mut missed = Vec::new();
        for text in [
            "HEALTH SUBCOMMITTEE",
            "OVERSIGHT SUMCOMMITTEE",
            "Federal Deposit Insurance Corporation",
            "EPA Regional Office",
            "Arizona Game and Fish Department",
            "FWS's Law Enforcement Office",
            "FBI Ashmoor Field Office",
            "Ashmoor State Experiment Station",
            "Northern Regional Research Laboratory",
            "Ashmoor Experiment Station",
            "Iova State Experiment Statione",
            "Georgia Experiment Ststion",
            "Northern Regional Reseerch Laboratory",
            "Fungicide-Herbicide Branch Registration Division",
            "Ashmoor County Dispatch",
            "Ashmoor County Medical Examiner",
            "Agricultural Research Service",
            "Soil Conservation Service",
            "Corps of Engineers",
            "Secretary of Defense",
            "Minister of Defence of the French Republic",
            "NATO Land Group 1",
            "Armaments Cooperation Division",
            "Security Authority",
            "National Archives",
            "Basel Committee",
            "International Monetary Fund",
            "Brindle County Police Pension System",
            "Fire and Police Employees' Retirement System of the City of Brindle",
            "Williamson Cnty. Appraisal Dist.",
            "Texas Health and Safety Code",
            "CAFTA-DR Secretariat",
            "ChD",
            "Арбитражный суд Энской области",
            "Министерства обороны Российской Федерации",
            "Пенсионного фонда Российской Федерации",
            "Фонда пенсионного и социального страхованияРоссийской Федерации",
            "ОПФР",
            "Регистрационно-лицензионной палатой",
        ] {
            for category in [Category::Organization, Category::Person] {
                if !fixed(text, &[(text, category)]).is_empty() {
                    missed.push(format!("{category:?}: {text}"));
                }
            }
        }
        assert!(missed.is_empty(), "still proposed:\n{}", missed.join("\n"));
    }

    /// #378 cut spans and defined short names need the body's context;
    /// adjacent personal names and private bodies must keep their spans.
    #[test]
    fn public_bodies_round_three_fragments() {
        let mut missed = Vec::new();
        for (source, span) in [
            ("High Court of Justice", "Justice"),
            ("the Law Enforcement Office", "Office"),
            (
                "Министерства\nфинансов Российской Федерации",
                "финансов Российской Федерации",
            ),
            (
                "Министерства\r\nфинансов Российской Федерации",
                "финансов Российской Федерации",
            ),
            ("бюджет Фонда", "Фонда"),
            ("МО МВД России «Энский»", "Энский"),
            ("Brindle Freight Safety\nAct", "Brindle Freight Safety"),
            (
                "Brindle County Police Pension System (“Brindle P&P”). Brindle P&P replied.",
                "Brindle P&P",
            ),
            (
                "Fire and Police Employees' Retirement System of the City of Baltimore (“Baltimore F&P”).",
                "Baltimore F&P",
            ),
            (
                "Fire and Police Employees' Retirement System of the City of Brindle",
                "Employees' Retirement System",
            ),
        ] {
            for category in [Category::Organization, Category::Person] {
                if !fixed(source, &[(span, category)]).is_empty() {
                    missed.push(format!("{category:?}: {source} [span {span}]"));
                }
            }
        }
        assert!(missed.is_empty(), "still proposed:\n{}", missed.join("\n"));
    }

    /// The reach decision in #378's last comment (#484 item 4): statutory
    /// corporations, enterprises, laboratories and regulators are bodies;
    /// ownership alone never removes a university or a college.
    #[test]
    fn public_bodies_round_three_statutory_bodies() {
        let mut missed = Vec::new();
        for text in [
            "PBGC",
            "ARRC",
            "Brookhaven National Laboratory",
            "Brindle NHS Trust",
            "Brindle Port Authority",
            "Law Society of Brindle",
            "Росатом",
            "Роскосмос",
            "ГУП «Энский водоканал»",
            "МУП «Энский водоканал»",
        ] {
            for category in [Category::Organization, Category::Person] {
                if !fixed(text, &[(text, category)]).is_empty() {
                    missed.push(format!("{category:?}: {text}"));
                }
            }
        }
        assert!(missed.is_empty(), "still proposed:\n{}", missed.join("\n"));
    }

    /// Gate review of #643: each reported name survives both span fixes
    /// and the proposals the user actually sees, with its category intact.
    #[test]
    fn public_bodies_gate_review_keeps_names() {
        for (source, span, category) in [
            (
                "Employee: John Smith\nAct of acceptance of completed work",
                "John Smith",
                Category::Person,
            ),
            (
                "For National Grid\nAct of acceptance of completed work",
                "National Grid",
                Category::Organization,
            ),
            ("John Smith\nAct", "John Smith", Category::Person),
            (
                "Brindle Freight Safety\nAct of acceptance of completed work",
                "Brindle Freight Safety",
                Category::Organization,
            ),
            (
                "Employee: Brindle Freight Safety\nAct",
                "Brindle Freight Safety",
                Category::Organization,
            ),
            ("Department\njohn Smith", "john Smith", Category::Person),
            ("Ministry\njohn Smith", "john Smith", Category::Person),
            ("Ministry\r\njohn Smith", "john Smith", Category::Person),
            ("Department john Smith", "john Smith", Category::Person),
            (
                "Министерства\nиван Смирнов",
                "иван Смирнов",
                Category::Person,
            ),
            (
                "Сотрудник МВД России «Иван Смирнов» подписал заявление.",
                "Иван Смирнов",
                Category::Person,
            ),
            (
                "Сотрудник МВД России «Сокол» сообщил о встрече.",
                "Сокол",
                Category::Person,
            ),
            (
                "МВД России «Сокол» сообщил о встрече.",
                "Сокол",
                Category::Person,
            ),
            (
                "Сотрудник МО МВД России «Сокол» сообщил о встрече.",
                "Сокол",
                Category::Person,
            ),
            (
                "ООО «МВД России «Страж»» заключило договор.",
                "Страж",
                Category::Organization,
            ),
            (
                "ООО «МО МВД России «Страж»» заключило договор.",
                "Страж",
                Category::Organization,
            ),
        ] {
            assert_eq!(fixed(source, &[(span, category)]), [span], "{source}");
            let start = source.find(span).unwrap();
            let label = match category {
                Category::Person => "person",
                Category::Organization => "organization",
                _ => unreachable!(),
            };
            let proposed = crate::recognition::proposals(
                source,
                &[crate::RawEntity {
                    start,
                    end: start + span.len(),
                    label: label.into(),
                    score: 0.9,
                    text: span.into(),
                }],
            )
            .unwrap();
            assert_eq!(
                proposed,
                [crate::recognition::Span {
                    range: start..start + span.len(),
                    category: if category == Category::Person {
                        "PERSON"
                    } else {
                        "ORG"
                    }
                    .into(),
                }],
                "{source}"
            );
        }
    }

    /// Private and party qualifiers on either side take precedence over
    /// even a direct central-bank relation, in either model category.
    #[test]
    fn bank_of_england_gate_review_keeps_private_names() {
        let name = "Bank of England";
        for category in [Category::Organization, Category::Person] {
            for source in [
                "The Bank of England, a private Arkansas lender, financed a home in London.",
                "The Bank of England (Lender) serves UK customers.",
                "Private Bank of England discussed UK monetary policy.",
                "The Bank of England, a private bank, discussed monetary policy.",
                "Lender: The Bank of England is the British central bank.",
                "The Bank of England (Lender) is the British central bank.",
                "Party A: The Bank of England is the British central bank.",
                "The Bank of England (Party A) is the British central bank.",
                "Private: The Bank of England is the British central bank.",
                "The Bank of England, private, is the British central bank.",
                "The Bank of England\n(Lender) serves UK customers.",
            ] {
                assert_eq!(fixed(source, &[(name, category)]), [name], "{source}");
                let start = source.find(name).unwrap();
                let proposed = crate::recognition::proposals(
                    source,
                    &[crate::RawEntity {
                        start,
                        end: start + name.len(),
                        label: if category == Category::Person {
                            "person"
                        } else {
                            "organization"
                        }
                        .into(),
                        score: 0.9,
                        text: name.into(),
                    }],
                )
                .unwrap();
                assert!(
                    proposed
                        .iter()
                        .any(|span| span.range == (start..start + name.len())),
                    "{source}: {proposed:?}"
                );
            }
        }
    }

    /// Institutional continuations still go when their syntax, rather
    /// than the preceding bare office word, establishes the body's name.
    #[test]
    fn public_body_continuations_need_institutional_syntax() {
        for (source, span) in [
            ("Department\nof Justice", "of Justice"),
            ("Ministry\nof Defence", "of Defence"),
            (
                "Министерства\nфинансов Российской Федерации",
                "финансов Российской Федерации",
            ),
            (
                "Министерства\nобороны Российской Федерации",
                "обороны Российской Федерации",
            ),
            ("Brindle Freight Safety\nAct", "Brindle Freight Safety"),
            (
                "Biometric Information Privacy\r\nAct",
                "Biometric Information Privacy",
            ),
            ("Data Protection\nAct", "Data Protection"),
            ("МО МВД России «Энский»", "Энский"),
        ] {
            for category in [Category::Organization, Category::Person] {
                assert!(fixed(source, &[(span, category)]).is_empty(), "{source}");
            }
        }
    }

    /// #378 Q2: the Arkansas lender shares the central bank's name.
    /// Only an article and a direct institutional relation disambiguate
    /// it; geography and discussion of public policy are insufficient.
    #[test]
    fn bank_of_england_needs_public_context() {
        let name = "Bank of England";
        for category in [Category::Organization, Category::Person] {
            for source in [
                "Lender: Bank of England",
                "Bank of England",
                "The Bank of England supplied the loan",
                "Bank of England discussed Monetary Policy",
                "The Governor spoke. The Bank of England replied",
                "UK policy\n\nThe Bank of England supplied the loan",
                "The Bank of England Services Ltd discussed Monetary Policy",
                "The Bank of England discussed Governor",
                "The Bank of England discussed Monetary Policy",
                "The Bank of England discussed Threadneedle",
                "The Bank of England discussed Prudential Regulation",
                "The Bank of England discussed UK",
                "The Bank of England discussed United Kingdom",
                "The Bank of England discussed British",
                "The Bank of England discussed London",
            ] {
                let span = if source.contains("Services Ltd") {
                    "Bank of England Services Ltd"
                } else {
                    name
                };
                assert_eq!(fixed(source, &[(span, category)]), [span], "{source}");
            }
            for source in [
                "The Governor of the Bank of England replied",
                "The Bank of England's Monetary Policy Committee replied",
                "The Bank of England's Prudential Regulation Authority replied",
                "The Bank of England, the UK's central bank, replied",
                "The Bank of England is the British central bank",
            ] {
                assert!(fixed(source, &[(name, category)]).is_empty(), "{source}");
                for label in [
                    "Lender",
                    "Borrower",
                    "Party",
                    "Plaintiff",
                    "Defendant",
                    "Lenders",
                    "Party A",
                    "Lender (UK)",
                ] {
                    for separator in [": ", ":\n"] {
                        let source = format!("{label}{separator}{source}");
                        // The article may be outside or inside the span.
                        let article = if source.contains("the Bank") {
                            "the"
                        } else {
                            "The"
                        };
                        for span in [name, &format!("{article} Bank of England")] {
                            assert_eq!(fixed(&source, &[(span, category)]), [span], "{source}");
                        }
                    }
                }
            }
        }
    }

    /// #378 Q2: the production company's complete trading name is private;
    /// an office needs its public parent inside the proposed span.
    #[test]
    fn field_office_needs_a_public_parent() {
        for category in [Category::Organization, Category::Person] {
            for (source, span) in [
                ("The Field Office supplied the film", "The Field Office"),
                ("The Field Office supplied the film", "Field Office"),
                ("The Ashmoor Field Office replied", "Ashmoor Field Office"),
                ("Ice Field Office supplied the film", "Ice Field Office"),
                ("The FBI contacted the Field Office", "Field Office"),
                (
                    "FBI Field Office Ltd supplied the film",
                    "FBI Field Office Ltd",
                ),
            ] {
                // An ORG ending in "Office" is a private department (#506)
                // and goes anyway; a legal form keeps it.
                if category == Category::Organization && span.ends_with("Office") {
                    continue;
                }
                assert_eq!(fixed(source, &[(span, category)]), [span], "{source}");
            }
            for span in [
                "FBI Field Office",
                "FBI Ashmoor Field Office",
                "ICE Field Office",
                "USCIS Field Office",
                "Social Security Field Office",
                "Department of Labor Field Office",
                "Ashmoor Agency Field Office",
                "Ashmoor Bureau Field Office",
                "Ashmoor Service Field Office",
            ] {
                assert!(fixed(span, &[(span, category)]).is_empty(), "{span}");
            }
        }
    }

    /// #378 Q2: Foundation and Hospital are parts of NHS trust names.
    #[test]
    fn nhs_foundation_trusts_are_public_bodies() {
        for span in [
            "Oxleas NHS Foundation Trust",
            "Oxleas NHS Trust",
            "Oxleas NHS Foundation Trust Hospital",
            "NHS Foundation Trust",
            "NHS Trust",
            "NHS Foundation Trust Hospital",
            "Ashmoor and Brindle NHS Foundation Trust",
            "Ashmoor & Brindle NHS Foundation Trust",
            "Ashmoor NHS\nFoundation Trust",
            "Ashmoor NHS\r\nFoundation Trust",
            "Ashmoor and\nBrindle NHS Foundation Trust",
            "Ashmoor NHS Foundation\nTrust",
        ] {
            for category in [Category::Organization, Category::Person] {
                assert!(fixed(span, &[(span, category)]).is_empty(), "{span}");
            }
        }
        for private in [
            "Oxleas NHS Foundation Trust Services Ltd",
            "Ashmoor and Brindle NHS Foundation Trust Services Ltd",
            "Ashmoor & Brindle NHS\nFoundation Trust Services Ltd",
            "Ashmoor NHS\n\nFoundation Trust",
            "Ashmoor and Brindle NHS Foundation Trust Consulting",
        ] {
            for category in [Category::Organization, Category::Person] {
                assert_eq!(
                    fixed(private, &[(private, category)]),
                    [private],
                    "{private}"
                );
            }
        }
    }

    /// The privacy side of #378: similar people, companies, universities,
    /// non-state funds and the body's own identifiers remain marked.
    #[test]
    fn public_bodies_round_three_preserves_names_and_details() {
        for text in [
            "Quillon Freight LLC",
            "Bank of England Services Ltd",
            "Security Authority Ltd",
            "Basel Committee Consulting",
            "Research Laboratory Partners",
            "Field Office Supplies",
            "Ashmoor Experiment Station Ltd",
            "Experiment Station Productions",
            "Regional Research Laboratory Services",
            "NATO Logistics",
            "NASA Federal Credit Union",
            "Treaty Oak Capital",
            "Ashmoor County Community College",
            "ФГБОУ ВО «Энский государственный университет»",
            "Негосударственный пенсионный фонд «Кедровый бор»",
            "Благотворительный фонд «Кедровый бор»",
            "ООО «Палата»",
        ] {
            assert_eq!(
                fixed(text, &[(text, Category::Organization)]),
                [text],
                "{text}"
            );
        }
        for (source, span) in [
            ("Ms Justice wrote", "Justice"),
            ("Secretary of Defense Orrin Quill wrote", "Orrin Quill"),
            ("Minister of Defence Orrin Quill wrote", "Orrin Quill"),
            ("начальник Министерства де Лаваль", "де Лаваль"),
            ("Суд Н. О.", "Суд Н. О."),
            ("Иван Судов", "Иван Судов"),
            ("ГУ МВД России: Иван Ветлин", "Иван Ветлин"),
            ("Police met Orrin Quill (“Orrin Q”).", "Orrin Q"),
            (
                "Brindle County Police Pension System; Brindle Paper Partners (“Brindle P&P”).",
                "Brindle P&P",
            ),
        ] {
            assert_eq!(
                fixed(source, &[(span, Category::Person)]),
                [span],
                "{source}"
            );
        }
        // The same short name also defined for a company is ambiguous.
        assert_eq!(
            fixed(
                "Brindle County Police Pension System (“Brindle P&P”); \
                 Brindle Paper Partners LLC (“Brindle P&P”).",
                &[("Brindle P&P", Category::Organization)],
            ),
            ["Brindle P&P"]
        );
        // A line wrap is context; a paragraph break or a sentence is not.
        assert_eq!(
            fixed(
                "Министерства\n\nФинансов Иван",
                &[("Финансов Иван", Category::Person)]
            ),
            ["Финансов Иван"]
        );
        assert_eq!(
            fixed(
                "Министерства\n\nфинансов Российской Федерации",
                &[("финансов Российской Федерации", Category::Organization)]
            ),
            ["финансов Российской Федерации"]
        );
        assert_eq!(
            fixed(
                "Treaty Oak Capital Agreement",
                &[("Treaty Oak Capital", Category::Organization)]
            ),
            ["Treaty Oak Capital"]
        );
        assert_eq!(
            fixed(
                "Quillon Freight LLC\nAct",
                &[("Quillon Freight LLC", Category::Organization)]
            ),
            ["Quillon Freight LLC"]
        );
        for (value, category) in [
            ("41 Larch Road, Ashmoor AS1 9ZZ", Category::Address),
            ("+1 (202) 555-0164", Category::Phone),
            ("office@quarry.invalid", Category::Email),
            ("7701234567", Category::Tax),
            ("40702810900000012345", Category::Bank),
        ] {
            let source = format!("Bureau of Quarry Safety / Министерство финансов: {value}");
            assert_eq!(fixed(&source, &[(value, category)]), [value]);
        }
    }

    /// Exercise the complete correction/review path on the new synthetic
    /// documents with injected model spans; no model snapshot is written.
    #[test]
    fn public_bodies_round_three_fixtures_with_injected_spans() {
        use crate::recognition::{Fixture, proposals};
        for (lang, extra) in [
            (
                "en",
                vec![
                    ("Ashmoor Field Office", Category::Organization),
                    ("Northern Regional Research Laboratory", Category::Person),
                    ("Justice", Category::Person),
                    ("Law Enforcement Office", Category::Organization),
                    ("Brindle P&P", Category::Person),
                    ("Brindle Freight Safety", Category::Organization),
                ],
            ),
            (
                "ru",
                vec![
                    ("МО МВД России «Энский»", Category::Organization),
                    ("Энский»", Category::Person),
                    ("финансов Российской Федерации", Category::Organization),
                    ("МУП «Энский водоканал»", Category::Person),
                ],
            ),
        ] {
            let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
                .join(format!("../../tests/fixtures/recognition/{lang}/judgment/probe-{lang}-r3-public-bodies-r3.md"));
            let fixture = Fixture::read(&path).unwrap();
            fixture.validate().unwrap();
            fixture.validate_layout().unwrap();
            let raw = |range: std::ops::Range<usize>, category: Category| {
                let label = crate::FAMILIES
                    .iter()
                    .flat_map(|family| family.iter())
                    .find(|(_, value)| *value == category)
                    .unwrap()
                    .0;
                crate::RawEntity {
                    start: range.start,
                    end: range.end,
                    label: label.into(),
                    score: 0.9,
                    text: fixture.source[range].to_owned(),
                }
            };
            let mut entities: Vec<_> = fixture
                .gold
                .iter()
                .map(|span| raw(span.range.clone(), category(&span.category).unwrap()))
                .collect();
            for (needle, category) in extra {
                assert!(fixture.source.contains(needle), "{lang}: {needle}");
                for (at, _) in fixture.source.match_indices(needle) {
                    entities.push(raw(at..at + needle.len(), category));
                }
            }
            let mut proposed = proposals(&fixture.source, &entities).unwrap();
            // "Ashmoor Field Office" goes as a private department (#506),
            // as the gold has it.
            let mut gold = fixture.gold;
            proposed.sort();
            gold.sort();
            assert_eq!(proposed, gold, "{lang}");
        }
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
            "Сиреневая аллея, Москва",
            "9-я линия В.О.",
            "The Old Rectory, Nether Compton",
            "The Old Rectory\nNether Compton",
        ] {
            assert!(pack.identifying(street, Category::Address), "{street}");
        }
    }

    /// #362 round two: Russian addresses of court acts and register
    /// extracts are proposed whole, from the index or first element to the
    /// last: flat lists, a second house number, dotless and capital parts,
    /// «им.», settlement types, a house letter, «ГСП», a street with no
    /// house or a box after an index and a place, and the elements around
    /// a masked or blank part.
    #[test]
    fn russian_court_and_register_addresses_are_proposed_whole() {
        let addresses = |source: &str, raw: &[(&str, &str)]| -> Vec<String> {
            shown(source, raw)
                .into_iter()
                .filter(|(_, category)| category == "ADDRESS")
                .map(|(text, _)| text)
                .collect()
        };
        for (source, expected) in [
            // A flat list and a second house number go on; no space is
            // needed after a comma.
            (
                "Квартиры: ул. Кедровая, 10 кв. 1,2; ул. Сосновая, 12 кв. 1, 2, 4; ул. Еловая,16. Далее",
                &[
                    "ул. Кедровая, 10 кв. 1,2",
                    "ул. Сосновая, 12 кв. 1, 2, 4",
                    "ул. Еловая,16",
                ][..],
            ),
            (
                "Дома: ул. Луговая, 6, 9; ул. Озёрная, 25 кв. 1.",
                &["ул. Луговая, 6, 9", "ул. Озёрная, 25 кв. 1"],
            ),
            // From the region or index to the flat, «им.» in the street's
            // name, a district after the city.
            (
                "Адрес: 999999, Энская область, г. Энск, Северный р-н, ул. им. Пахомова, д. 24, кв. 141. Далее",
                &[
                    "999999, Энская область, г. Энск, Северный р-н, ул. им. Пахомова, д. 24, кв. 141",
                ],
            ),
            (
                "проживает: Энская обл., г. Энск, Лесной р-н, ул. им. Пахомова, д. 3, кв. 8.",
                &["Энская обл., г. Энск, Лесной р-н, ул. им. Пахомова, д. 3, кв. 8"],
            ),
            // A register extract in capitals, the country before the index.
            (
                "Юридический адрес: РОССИЯ 999999, ЭНСК, УЛ ЦЕНТРАЛЬНАЯ, Д. 14, К. 4, ПОМЕЩ. 5-Н, ОФ. 1.",
                &["РОССИЯ 999999, ЭНСК, УЛ ЦЕНТРАЛЬНАЯ, Д. 14, К. 4, ПОМЕЩ. 5-Н, ОФ. 1"],
            ),
            (
                "Адрес: РОССИЯ 999999, Г.ЭНСК, ВН.ТЕР.Г. МУНИЦИПАЛЬНЫЙ ОКРУГ КЕДРОВЫЙ, УЛ КЕДРОВАЯ, Д. 10, ПОМЕЩ. 5-Н.",
                &[
                    "РОССИЯ 999999, Г.ЭНСК, ВН.ТЕР.Г. МУНИЦИПАЛЬНЫЙ ОКРУГ КЕДРОВЫЙ, УЛ КЕДРОВАЯ, Д. 10, ПОМЕЩ. 5-Н",
                ],
            ),
            // Settlement types (#482's «ст.» too) and a house letter after a
            // space.
            (
                "Магазин: Республика Энская, Кедровский район, ч. Кедровка, ул. Лесная, д. 43 Г. Далее",
                &["Республика Энская, Кедровский район, ч. Кедровка, ул. Лесная, д. 43 Г"],
            ),
            (
                "Офис: 999999, Энская область, г.о. Кедровск, ул. Лесная, д. 5, стр. 2, оф. 3.",
                &["999999, Энская область, г.о. Кедровск, ул. Лесная, д. 5, стр. 2, оф. 3"],
            ),
            (
                "проживает: 999999, Энский край, ст. Кедровская, ул. Садовая, д. 5; уроженец",
                &["999999, Энский край, ст. Кедровская, ул. Садовая, д. 5"],
            ),
            // Dotless parts.
            (
                "Адрес: ул Лесная, д 31 к 2, кв 69б.",
                &["ул Лесная, д 31 к 2, кв 69б"],
            ),
            // Court headers: an index, «ГСП», a bare city and house, a
            // street with an initial or with its type after its name.
            (
                "Девятый арбитражный апелляционный суд\n999999, Энск, ГСП-4, проезд Соломенной cторожки, 12\nПОСТАНОВЛЕНИЕ",
                &["999999, Энск, ГСП-4, проезд Соломенной cторожки, 12"],
            ),
            (
                "Арбитражный суд Энской области, 999999, г. Энск, ул. Б. Тульская, д. 17, тел. 8 (999) 555-12-34",
                &["999999, г. Энск, ул. Б. Тульская, д. 17"],
            ),
            (
                "Суд: 999999, г. Энск, Смольнинская ул., д. 6.",
                &["999999, г. Энск, Смольнинская ул., д. 6"],
            ),
            (
                "Суд: 999999, Энский край, Энск, ул. Ленина, д. 37.",
                &["999999, Энский край, Энск, ул. Ленина, д. 37"],
            ),
            // After an index and a place, a street with no house, or a box.
            (
                "Адрес: 999999, Энская область, Энск город, Московское (18 км) шоссе. Далее",
                &["999999, Энская область, Энск город, Московское (18 км) шоссе"],
            ),
            (
                "Адрес: 999999, Энская область, г. Энск, а/ 1. Далее",
                &["999999, Энская область, г. Энск, а/ 1"],
            ),
            (
                "Почта: 999999, г. Энск, а/я 15.",
                &["999999, г. Энск, а/я 15"],
            ),
            // Masked or blank parts: the elements around a blank or an
            // ellipsis are one address; no span holds a "<", so one starts
            // after "<...>" (and an ellipsis) at a house with more after it,
            // or at a plot, and an index with its places before a mask is
            // one of its own.
            (
                "(<...> д 31 к 2, кв 69б, ИНН: <***>)",
                &["д 31 к 2, кв 69б"],
            ),
            (
                "участок: Энская область, Кедровский р-н, <...> уч. 109.",
                &["уч. 109"],
            ),
            (
                "Офис: 999999, Энская область, г.о. Кедровск, <...> д. 5, стр. 1, оф. 2.",
                &[
                    "999999, Энская область, г.о. Кедровск",
                    "д. 5, стр. 1, оф. 2",
                ],
            ),
            (
                "Офис: 999999, Энская область, г.о. Кедровск, <...> … д. 5, стр. 1, оф. 2.",
                &[
                    "999999, Энская область, г.о. Кедровск",
                    "д. 5, стр. 1, оф. 2",
                ],
            ),
            (
                "Адрес: г. Энск, ул. Лесная, д. 5, , ком. 1-А. Далее",
                &["г. Энск, ул. Лесная, д. 5, , ком. 1-А"],
            ),
            (
                "Адрес: г. Энск, , д. 5, ком. 1-А. Далее",
                &["г. Энск, , д. 5, ком. 1-А"],
            ),
            (
                "Адрес: г. Энск, ул. <...>, д. 5, кв. 7. Далее",
                &["д. 5, кв. 7"],
            ),
            (
                "Адрес: 999999, г. Энск, ул. …, д. 5, кв. 7. Далее",
                &["999999, г. Энск, ул. …, д. 5, кв. 7"],
            ),
            (
                "Адрес: 999999, г. Энск, …, д. 5, кв. 7. Далее",
                &["999999, г. Энск, …, д. 5, кв. 7"],
            ),
        ] {
            assert_eq!(addresses(source, &[]), expected, "{source}");
        }
        // A model address cut short grows to the whole of it.
        let source =
            "Магазин: Республика Энская, Кедровский район, ч. Кедровка, ул. Лесная, д. 43 Г.";
        assert_eq!(
            addresses(source, &[("ул. Лесная, д. 43", "address")]),
            ["Республика Энская, Кедровский район, ч. Кедровка, ул. Лесная, д. 43 Г"]
        );
    }

    /// #362 round two's guards: a Russian address never takes the name
    /// before it, the next field or a phone after a flat, and an index
    /// alone, prose about «ул.» or a mask before a date is no address. A
    /// model's address that runs into the next label ends before it.
    #[test]
    fn russian_addresses_keep_names_fields_and_prose_outside() {
        assert_eq!(
            shown(
                "Истец Иванов Иван Иванович, 999999, г. Энск, ул. Лесная, д. 5, кв. 7, ИНН 999999999999",
                &[("Иванов Иван Иванович", "person")]
            ),
            pairs(&[
                ("Иванов Иван Иванович", "PERSON"),
                ("999999, г. Энск, ул. Лесная, д. 5, кв. 7", "ADDRESS"),
                ("999999999999", "TAX"),
            ])
        );
        assert_eq!(
            shown(
                "ООО «Кедровый бор»: РОССИЯ 999999, ЭНСК, УЛ ЦЕНТРАЛЬНАЯ, Д. 14, К. 4, ОФ. 1.",
                &[("Кедровый бор", "organization")]
            ),
            pairs(&[
                ("ООО «Кедровый бор»", "ORG"),
                (
                    "РОССИЯ 999999, ЭНСК, УЛ ЦЕНТРАЛЬНАЯ, Д. 14, К. 4, ОФ. 1",
                    "ADDRESS"
                ),
            ])
        );
        // A phone after a flat is no flat list.
        assert_eq!(
            shown("Адрес: ул. Лесная, д. 5, кв. 7, 8 (999) 555-12-34.", &[]),
            pairs(&[("ул. Лесная, д. 5, кв. 7", "ADDRESS")])
        );
        assert_eq!(
            shown("Адрес: ул. Лесная, д. 5, кв. 7, 8-999-555-12-34.", &[]),
            pairs(&[("ул. Лесная, д. 5, кв. 7", "ADDRESS")])
        );
        // A model address that runs into a cadastral label ends before it;
        // a residue of a letter and the label goes.
        let source = "участок по адресу <...>, д. 5, лит. Б, кадастровый № 99:07:0012345:678";
        assert_eq!(
            shown(source, &[("д. 5, лит. Б, кадастровый №", "address")]),
            pairs(&[
                ("д. 5, лит. Б", "ADDRESS"),
                ("99:07:0012345:678", "IDENTITY"),
            ])
        );
        let source = "участок <...> Б, кадастровый № 99:07:0012345:678";
        assert_eq!(
            shown(source, &[("Б, кадастровый №", "address")]),
            pairs(&[("99:07:0012345:678", "IDENTITY")])
        );
        for source in [
            "Индекс 625010, далее по тексту.",
            "625010, ",
            "Сокращение «ул.» означает улицу, а «д.» — дом; ул. Ленина упомянута в тексте.",
            "На ул. Ленина, у дома, стояли люди.",
            "<...> д. 5 мая состоялось заседание",
            "по материалам д. 12, ст. 5 закона",
            "Псков, д. 29, пом. 8",
            "согласно п. 3 ч. 1 ст. 5 закона",
        ] {
            assert!(shown(source, &[]).is_empty(), "{source}");
        }
    }

    /// #362 round two's pre-gate review: every input it gave, with the
    /// model's spans it gave. A name before an address (a surname, a short
    /// company name, initials that read like a settlement's type), a phone,
    /// a date or a passport after a flat list, and a public body's name
    /// keep their spans, and so does the address beside them; a masked
    /// office keeps the index and places before the mask and the house
    /// after it; the brief's shapes stand as they were.
    #[test]
    fn russian_addresses_keep_the_review_inputs_apart() {
        // Each row: the source, the model's spans and what is shown.
        type Spans = &'static [(&'static str, &'static str)];
        let rows: &[(&str, Spans, Spans)] = &[
            // P1-A: a name before the places stays a name; the address after
            // it stands, from its first place.
            (
                "Истец: Иванов, г. Энск, ул. Лесная, д. 5, кв. 7.",
                &[("Иванов", "person")][..],
                &[
                    ("Иванов", "PERSON"),
                    ("г. Энск, ул. Лесная, д. 5, кв. 7", "ADDRESS"),
                ][..],
            ),
            (
                "Истец: Иванов, г. Энск, ул. Лесная, д. 5, кв. 7.",
                &[][..],
                &[("г. Энск, ул. Лесная, д. 5, кв. 7", "ADDRESS")][..],
            ),
            (
                "ответчик Петров, г. Энск, ул. Лесная, д. 5.",
                &[("Петров", "person")][..],
                &[
                    ("Петров", "PERSON"),
                    ("г. Энск, ул. Лесная, д. 5", "ADDRESS"),
                ][..],
            ),
            (
                "общества с ограниченной ответственностью Кедр, г. Энск, ул. Лесная, д. 5.",
                &[("Кедр", "organization")][..],
                &[("Кедр", "ORG"), ("г. Энск, ул. Лесная, д. 5", "ADDRESS")][..],
            ),
            (
                "общества с ограниченной ответственностью Кедр, г. Энск, ул. Лесная, д. 5.",
                &[(
                    "общества с ограниченной ответственностью Кедр",
                    "organization",
                )][..],
                &[
                    ("общества с ограниченной ответственностью Кедр", "ORG"),
                    ("г. Энск, ул. Лесная, д. 5", "ADDRESS"),
                ][..],
            ),
            (
                "Ответчик: Сидоров, город Энск, ул. Лесная, д. 5.",
                &[("Сидоров", "person")][..],
                &[
                    ("Сидоров", "PERSON"),
                    ("город Энск, ул. Лесная, д. 5", "ADDRESS"),
                ][..],
            ),
            (
                "Ответчик: Сидоров, Энская область, г. Энск, ул. Лесная, д. 5.",
                &[("Сидоров", "person")][..],
                &[
                    ("Сидоров", "PERSON"),
                    ("Энская область, г. Энск, ул. Лесная, д. 5", "ADDRESS"),
                ][..],
            ),
            (
                "Ответчик: Петров А.С., г. Энск, ул. Лесная, д. 5.",
                &[("Петров А.С.", "person")][..],
                &[
                    ("Петров А.С.", "PERSON"),
                    ("г. Энск, ул. Лесная, д. 5", "ADDRESS"),
                ][..],
            ),
            (
                "ИП Петров А.С., 999999, г. Энск, ул. Лесная, д. 5.",
                &[("Петров А.С.", "person")][..],
                &[
                    ("Петров А.С.", "PERSON"),
                    ("999999, г. Энск, ул. Лесная, д. 5", "ADDRESS"),
                ][..],
            ),
            (
                "представитель А.С. Петров, 999999, г. Энск, ул. Лесная, д. 5.",
                &[("А.С. Петров", "person")][..],
                &[
                    ("А.С. Петров", "PERSON"),
                    ("999999, г. Энск, ул. Лесная, д. 5", "ADDRESS"),
                ][..],
            ),
            (
                "истца Иванова, Москва, ул. Лесная, д. 5",
                &[("Иванова", "person")][..],
                &[
                    ("Иванова", "PERSON"),
                    ("Москва, ул. Лесная, д. 5", "ADDRESS"),
                ][..],
            ),
            (
                "к обществу Кедр, г. Энск, ул. Лесная, д. 5.",
                &[("Кедр", "organization")][..],
                &[("Кедр", "ORG"), ("г. Энск, ул. Лесная, д. 5", "ADDRESS")][..],
            ),
            (
                "истец Иванов, ул. Лесная, д. 5",
                &[("Иванов", "person")][..],
                &[("Иванов", "PERSON"), ("ул. Лесная, д. 5", "ADDRESS")][..],
            ),
            (
                "ответчику Иванову, Кедровский район, ул. Лесная, д. 5",
                &[("Иванову", "person")][..],
                &[
                    ("Иванову", "PERSON"),
                    ("Кедровский район, ул. Лесная, д. 5", "ADDRESS"),
                ][..],
            ),
            (
                "ответчику Иванову, с. Кедровка, ул. Лесная, д. 5",
                &[("Иванову", "person")][..],
                &[
                    ("Иванову", "PERSON"),
                    ("с. Кедровка, ул. Лесная, д. 5", "ADDRESS"),
                ][..],
            ),
            (
                "Истец: Иванов, 999999, г. Энск, ул. Лесная, д. 5",
                &[("Иванов", "person")][..],
                &[
                    ("Иванов", "PERSON"),
                    ("999999, г. Энск, ул. Лесная, д. 5", "ADDRESS"),
                ][..],
            ),
            (
                "Истец: ООО Кедр, г. Энск, ул. Лесная, д. 5",
                &[("ООО Кедр", "organization")][..],
                &[
                    ("ООО Кедр", "ORG"),
                    ("г. Энск, ул. Лесная, д. 5", "ADDRESS"),
                ][..],
            ),
            (
                "Ответчик: Петров, Кедровский район, с. Кедровка, ул. Ленина, д. 1, ИНН 770708389312",
                &[("Петров", "person")][..],
                &[
                    ("Петров", "PERSON"),
                    ("Кедровский район, с. Кедровка, ул. Ленина, д. 1", "ADDRESS"),
                    ("770708389312", "TAX"),
                ][..],
            ),
            (
                "заявление общества Кедровый Бор, г. Энск, ул. Лесная, д. 5",
                &[("Кедровый Бор", "organization")][..],
                &[
                    ("Кедровый Бор", "ORG"),
                    ("г. Энск, ул. Лесная, д. 5", "ADDRESS"),
                ][..],
            ),
            (
                "Ответчик — Петров, г. Энск, ул. Лесная, д. 5",
                &[("Петров", "person")][..],
                &[
                    ("Петров", "PERSON"),
                    ("г. Энск, ул. Лесная, д. 5", "ADDRESS"),
                ][..],
            ),
            (
                "ИП Сидорова (Петрова, г. Энск, ул. Лесная, д. 5)",
                &[("Петрова", "person")][..],
                &[
                    ("Петрова", "PERSON"),
                    ("г. Энск, ул. Лесная, д. 5", "ADDRESS"),
                ][..],
            ),
            (
                "Истец: Иванов, г. Энск, ул. Лесная, д. 5, кв. 7.",
                &[
                    ("Иванов", "person"),
                    ("г. Энск, ул. Лесная, д. 5, кв. 7", "address"),
                ][..],
                &[
                    ("Иванов", "PERSON"),
                    ("г. Энск, ул. Лесная, д. 5, кв. 7", "ADDRESS"),
                ][..],
            ),
            (
                "Истец: Иванов, г. Энск, ул. Лесная, д. 5, кв. 7.",
                &[("г. Энск, ул. Лесная, д. 5, кв. 7", "address")][..],
                &[("г. Энск, ул. Лесная, д. 5, кв. 7", "ADDRESS")][..],
            ),
            (
                "к индивидуальному предпринимателю Петрову, г. Энск, ул. Лесная, д. 5, о взыскании",
                &[("Петрову", "person")][..],
                &[
                    ("Петрову", "PERSON"),
                    ("г. Энск, ул. Лесная, д. 5", "ADDRESS"),
                ][..],
            ),
            (
                "к индивидуальному предпринимателю Петрову, 999999, г. Энск, ул. Лесная, д. 5",
                &[("Петрову", "person")][..],
                &[
                    ("Петрову", "PERSON"),
                    ("999999, г. Энск, ул. Лесная, д. 5", "ADDRESS"),
                ][..],
            ),
            (
                "Ответчики: Петров, г. Энск, ул. Лесная, д. 5; Сидоров, г. Энск, ул. Садовая, д. 7.",
                &[("Петров", "person"), ("Сидоров", "person")][..],
                &[
                    ("Петров", "PERSON"),
                    ("г. Энск, ул. Лесная, д. 5", "ADDRESS"),
                    ("Сидоров", "PERSON"),
                    ("г. Энск, ул. Садовая, д. 7", "ADDRESS"),
                ][..],
            ),
            (
                "Ответчики: Петров, г. Энск, ул. Лесная, д. 5; Сидоров, г. Энск, ул. Садовая, д. 7.",
                &[][..],
                &[
                    ("г. Энск, ул. Лесная, д. 5", "ADDRESS"),
                    ("г. Энск, ул. Садовая, д. 7", "ADDRESS"),
                ][..],
            ),
            (
                "Иванов И.И. Хабаровский край, Хабаровск, ул. Ленина, д. 5",
                &[("Иванов И.И.", "person")][..],
                &[
                    ("Иванов И.И.", "PERSON"),
                    ("Хабаровск, ул. Ленина, д. 5", "ADDRESS"),
                ][..],
            ),
            (
                "Адрес: Хабаровский край, Хабаровск, ул. Ленина, д. 5",
                &[][..],
                &[("Хабаровский край, Хабаровск, ул. Ленина, д. 5", "ADDRESS")][..],
            ),
            (
                "Истец: Иванов, Энск, ул. Лесная, д. 5",
                &[("Иванов", "person")][..],
                &[("Иванов", "PERSON"), ("Энск, ул. Лесная, д. 5", "ADDRESS")][..],
            ),
            (
                "Адрес: Энск, Северный р-н, ул. Лесная, д. 5",
                &[][..],
                &[("Северный р-н, ул. Лесная, д. 5", "ADDRESS")][..],
            ),
            (
                "ООО «Кедр», Энская область, г. Энск, ул. Лесная, д. 5",
                &[("ООО «Кедр»", "organization")][..],
                &[
                    ("ООО «Кедр»", "ORG"),
                    ("Энская область, г. Энск, ул. Лесная, д. 5", "ADDRESS"),
                ][..],
            ),
            // P1-B: a person's initials are no settlement; a settlement's type
            // in capitals goes only before a name in capitals.
            (
                "представитель А.С. Петров, г. Энск, ул. Лесная, д. 5.",
                &[("А.С. Петров", "person")][..],
                &[
                    ("А.С. Петров", "PERSON"),
                    ("г. Энск, ул. Лесная, д. 5", "ADDRESS"),
                ][..],
            ),
            (
                "представитель А.С. Петров, г. Энск, ул. Лесная, д. 5.",
                &[][..],
                &[("г. Энск, ул. Лесная, д. 5", "ADDRESS")][..],
            ),
            (
                "Истец А. П. Петров, ул. Лесная, д. 5, кв. 7.",
                &[("А. П. Петров", "person")][..],
                &[
                    ("А. П. Петров", "PERSON"),
                    ("ул. Лесная, д. 5, кв. 7", "ADDRESS"),
                ][..],
            ),
            (
                "Истец Д. Петров, ул. Лесная, д. 5, кв. 7.",
                &[("Д. Петров", "person")][..],
                &[
                    ("Д. Петров", "PERSON"),
                    ("ул. Лесная, д. 5, кв. 7", "ADDRESS"),
                ][..],
            ),
            (
                "Ответчик: С. Петрова Ирина, ул. Лесная, д. 5.",
                &[("С. Петрова Ирина", "person")][..],
                &[
                    ("С. Петрова Ирина", "PERSON"),
                    ("ул. Лесная, д. 5", "ADDRESS"),
                ][..],
            ),
            (
                "Ответчик: Иванов И. С., ул. Лесная, д. 5.",
                &[("Иванов И. С.", "person")][..],
                &[("Иванов И. С.", "PERSON"), ("ул. Лесная, д. 5", "ADDRESS")][..],
            ),
            (
                "Ответчик: П. Иванов Иван Иванович, ул. Лесная, д. 5.",
                &[("П. Иванов Иван Иванович", "person")][..],
                &[
                    ("П. Иванов Иван Иванович", "PERSON"),
                    ("ул. Лесная, д. 5", "ADDRESS"),
                ][..],
            ),
            (
                "гражданина С. Петрова, ул. Лесная, д. 5, кв. 7",
                &[("С. Петрова", "person")][..],
                &[
                    ("С. Петрова", "PERSON"),
                    ("ул. Лесная, д. 5, кв. 7", "ADDRESS"),
                ][..],
            ),
            (
                "Истец: И.И. Петров, Энская область, г. Энск, ул. Лесная, д. 5, кв. 7",
                &[("И.И. Петров", "person")][..],
                &[
                    ("И.И. Петров", "PERSON"),
                    (
                        "Энская область, г. Энск, ул. Лесная, д. 5, кв. 7",
                        "ADDRESS",
                    ),
                ][..],
            ),
            (
                "Истец: И.П. Петров, г. Энск, ул. Лесная, д. 5",
                &[("И.П. Петров", "person")][..],
                &[
                    ("И.П. Петров", "PERSON"),
                    ("г. Энск, ул. Лесная, д. 5", "ADDRESS"),
                ][..],
            ),
            (
                "Истец: А.Х. Петров, ул. Лесная, д. 5",
                &[("А.Х. Петров", "person")][..],
                &[("А.Х. Петров", "PERSON"), ("ул. Лесная, д. 5", "ADDRESS")][..],
            ),
            (
                "Истец: А.И. Петров, ул. Лесная, д. 5",
                &[("А.И. Петров", "person")][..],
                &[("А.И. Петров", "PERSON"), ("ул. Лесная, д. 5", "ADDRESS")][..],
            ),
            (
                "Истец: А.С. Петров, ул. Лесная, д. 5",
                &[("Петров", "person")][..],
                &[("А.С. Петров", "PERSON"), ("ул. Лесная, д. 5", "ADDRESS")][..],
            ),
            (
                "ПЕТРОВ С. ИВАНОВИЧ, УЛ ЛЕСНАЯ, Д. 5",
                &[("ПЕТРОВ С. ИВАНОВИЧ", "person")][..],
                &[
                    ("ПЕТРОВ С. ИВАНОВИЧ", "PERSON"),
                    ("УЛ ЛЕСНАЯ, Д. 5", "ADDRESS"),
                ][..],
            ),
            (
                "Истец Г.П. Иванов, ул. Лесная, д. 5, кв. 7",
                &[("Г.П. Иванов", "person")][..],
                &[
                    ("Г.П. Иванов", "PERSON"),
                    ("ул. Лесная, д. 5, кв. 7", "ADDRESS"),
                ][..],
            ),
            (
                "Истец С. П. Иванов, ул. Лесная, д. 5, кв. 7",
                &[("С. П. Иванов", "person")][..],
                &[
                    ("С. П. Иванов", "PERSON"),
                    ("ул. Лесная, д. 5, кв. 7", "ADDRESS"),
                ][..],
            ),
            (
                "представителя Е.Д. Смирновой, г. Энск, ул. Лесная, д. 5",
                &[("Е.Д. Смирновой", "person")][..],
                &[
                    ("Е.Д. Смирновой", "PERSON"),
                    ("г. Энск, ул. Лесная, д. 5", "ADDRESS"),
                ][..],
            ),
            (
                "ООО КЕДРОВЫЙ БОР, РОССИЯ 999999, ЭНСК, УЛ ЦЕНТРАЛЬНАЯ, Д. 14",
                &[("ООО КЕДРОВЫЙ БОР", "organization")][..],
                &[
                    ("ООО КЕДРОВЫЙ БОР", "ORG"),
                    ("РОССИЯ 999999, ЭНСК, УЛ ЦЕНТРАЛЬНАЯ, Д. 14", "ADDRESS"),
                ][..],
            ),
            (
                "АО КЕДР, ЭНСКАЯ ОБЛАСТЬ, Г. ЭНСК, УЛ ЛЕСНАЯ, Д. 5",
                &[("АО КЕДР", "organization")][..],
                &[
                    ("АО КЕДР", "ORG"),
                    ("ЭНСКАЯ ОБЛАСТЬ, Г. ЭНСК, УЛ ЛЕСНАЯ, Д. 5", "ADDRESS"),
                ][..],
            ),
            (
                "ООО КЕДРОВСКИЙ РАЙОН, УЛ ЛЕСНАЯ, Д. 5",
                &[("ООО КЕДРОВСКИЙ РАЙОН", "organization")][..],
                &[
                    ("ООО КЕДРОВСКИЙ РАЙОН", "ORG"),
                    ("УЛ ЛЕСНАЯ, Д. 5", "ADDRESS"),
                ][..],
            ),
            (
                "ПЕТРОВ С. ИВАНОВИЧ, УЛ ЛЕСНАЯ, Д. 5",
                &[][..],
                &[("С. ИВАНОВИЧ, УЛ ЛЕСНАЯ, Д. 5", "ADDRESS")][..],
            ),
            (
                "999999, ЭНСКАЯ ОБЛАСТЬ, С. КЕДРОВКА, УЛ ЛЕСНАЯ, Д. 5",
                &[][..],
                &[(
                    "999999, ЭНСКАЯ ОБЛАСТЬ, С. КЕДРОВКА, УЛ ЛЕСНАЯ, Д. 5",
                    "ADDRESS",
                )][..],
            ),
            (
                "Адрес: Энская область, с. Кедровка, ул. Лесная, д. 5.",
                &[][..],
                &[("Энская область, с. Кедровка, ул. Лесная, д. 5", "ADDRESS")][..],
            ),
            (
                "согласно п. Кедровый, ул. Лесная, д. 5",
                &[][..],
                &[("п. Кедровый, ул. Лесная, д. 5", "ADDRESS")][..],
            ),
            // P2-1: a flat list ends before a phone, a date or a passport.
            (
                "Адрес: ул. Лесная, д. 5, кв. 7, 8 9161234567.",
                &[][..],
                &[("ул. Лесная, д. 5, кв. 7", "ADDRESS")][..],
            ),
            (
                "Адрес: ул. Лесная, д. 5, кв. 7, 8–916–123–45–67.",
                &[][..],
                &[("ул. Лесная, д. 5, кв. 7", "ADDRESS")][..],
            ),
            (
                "Адрес: ул. Лесная, д. 5, кв. 7, 123-45-67.",
                &[][..],
                &[("ул. Лесная, д. 5, кв. 7", "ADDRESS")][..],
            ),
            (
                "проживающий: ул. Лесная, д. 5, кв. 7, 12.03.1985 года рождения",
                &[][..],
                &[
                    ("ул. Лесная, д. 5, кв. 7", "ADDRESS"),
                    ("12.03.1985", "DATE"),
                ][..],
            ),
            (
                "проживающий: ул. Лесная, д. 5, кв. 7, 12.03.1985 года рождения",
                &[("12.03.1985", "date")][..],
                &[
                    ("ул. Лесная, д. 5, кв. 7", "ADDRESS"),
                    ("12.03.1985", "DATE"),
                ][..],
            ),
            (
                "Адрес: ул. Лесная, д. 5, кв. 7, 8, ИНН 770708389312",
                &[][..],
                &[
                    ("ул. Лесная, д. 5, кв. 7, 8", "ADDRESS"),
                    ("770708389312", "TAX"),
                ][..],
            ),
            (
                "Адрес: ул. Лесная, д. 5, кв. 7, 45 06 123456",
                &[][..],
                &[("ул. Лесная, д. 5, кв. 7", "ADDRESS")][..],
            ),
            (
                "Адрес: ул. Лесная, д. 5, кв. 7, р/с 40702810900000012345",
                &[][..],
                &[("ул. Лесная, д. 5, кв. 7", "ADDRESS")][..],
            ),
            (
                "Адрес: ул. Лесная, д. 5, кв. 7, 8–916–123–45–67.",
                &[("8–916–123–45–67", "phone_number")][..],
                &[
                    ("ул. Лесная, д. 5, кв. 7", "ADDRESS"),
                    ("8–916–123–45–67", "PHONE"),
                ][..],
            ),
            (
                "Адрес: ул. Лесная, д. 5, кв. 7, 8 916 123 45 67.",
                &[("8 916 123 45 67", "phone_number")][..],
                &[
                    ("ул. Лесная, д. 5, кв. 7", "ADDRESS"),
                    ("8 916 123 45 67", "PHONE"),
                ][..],
            ),
            (
                "Адрес: ул. Лесная, д. 5, кв. 7, 45 06 123456.",
                &[("45 06 123456", "passport_number")][..],
                &[
                    ("ул. Лесная, д. 5, кв. 7", "ADDRESS"),
                    ("45 06 123456", "IDENTITY"),
                ][..],
            ),
            (
                "Адрес: ул. Лесная, д. 5, кв. 7, 12-34-56.",
                &[("12-34-56", "phone_number")][..],
                &[
                    ("ул. Лесная, д. 5, кв. 7", "ADDRESS"),
                    ("12-34-56", "PHONE"),
                ][..],
            ),
            (
                "ул. Лесная, 5, 8 (999) 555-12-34",
                &[("8 (999) 555-12-34", "phone_number")][..],
                &[("ул. Лесная, 5", "ADDRESS"), ("8 (999) 555-12-34", "PHONE")][..],
            ),
            (
                "ул. Лесная, 5, 770708389312",
                &[][..],
                &[("ул. Лесная, 5", "ADDRESS")][..],
            ),
            (
                "ул. Лесная, д. 5, кв. 7, 555.12.34",
                &[("555.12.34", "phone_number")][..],
                &[
                    ("ул. Лесная, д. 5, кв. 7", "ADDRESS"),
                    ("555.12.34", "PHONE"),
                ][..],
            ),
            (
                "Иванов, проживающий: ул. Лесная, д. 5, кв. 7, 12.03.1985 г.р.",
                &[][..],
                &[
                    ("ул. Лесная, д. 5, кв. 7", "ADDRESS"),
                    ("12.03.1985", "DATE"),
                ][..],
            ),
            (
                "Иванов, проживающий: ул. Лесная, д. 5, кв. 7, 12.03.1985 г.р.",
                &[("12.03.1985", "date")][..],
                &[
                    ("ул. Лесная, д. 5, кв. 7", "ADDRESS"),
                    ("12.03.1985", "DATE"),
                ][..],
            ),
            (
                "проживающий: ул. Лесная, д. 5, кв. 7, 12.03.1985 года рождения, уроженец",
                &[("12.03.1985", "date")][..],
                &[
                    ("ул. Лесная, д. 5, кв. 7", "ADDRESS"),
                    ("12.03.1985", "DATE"),
                ][..],
            ),
            (
                "Адрес: ул. Лесная, д. 5, кв. 7, 12.03.1985 г.р.",
                &[("12.03.1985", "date")][..],
                &[
                    ("ул. Лесная, д. 5, кв. 7", "ADDRESS"),
                    ("12.03.1985", "DATE"),
                ][..],
            ),
            (
                "Адрес: ул. Лесная, д. 5, кв. 7, 8 – 916 – 123.",
                &[][..],
                &[("ул. Лесная, д. 5, кв. 7", "ADDRESS")][..],
            ),
            (
                "ул. Лесная, д. 5, кв. 1, 2",
                &[][..],
                &[("ул. Лесная, д. 5, кв. 1, 2", "ADDRESS")][..],
            ),
            (
                "ул. Кедровая, д. 10, кв. 1, 2\nДалее",
                &[][..],
                &[("ул. Кедровая, д. 10, кв. 1, 2", "ADDRESS")][..],
            ),
            // P2-2: masked parts: the house after a mask and an ellipsis, the
            // index and places before a mask.
            (
                "Офис: 999999, Энская область, г.о. Кедровск, <...> … д. 5, стр. 1, оф. 2.",
                &[][..],
                &[
                    ("999999, Энская область, г.о. Кедровск", "ADDRESS"),
                    ("д. 5, стр. 1, оф. 2", "ADDRESS"),
                ][..],
            ),
            (
                "999999, Энская область, г.о. Кедровск, <...> … д. 5, стр. 1, оф. 2",
                &[][..],
                &[
                    ("999999, Энская область, г.о. Кедровск", "ADDRESS"),
                    ("д. 5, стр. 1, оф. 2", "ADDRESS"),
                ][..],
            ),
            (
                "999999, Энская область, г.о. Кедровск, <...> д. 5, стр. 1, оф. 2",
                &[][..],
                &[
                    ("999999, Энская область, г.о. Кедровск", "ADDRESS"),
                    ("д. 5, стр. 1, оф. 2", "ADDRESS"),
                ][..],
            ),
            ("Адрес: г. Энск, <...>, д. 5", &[][..], &[][..]),
            (
                "Адрес: <...>, д. 5, кв. 7",
                &[][..],
                &[("д. 5, кв. 7", "ADDRESS")][..],
            ),
            (
                "Адрес: г. Энск, ул. ***, д. 5, кв. 7",
                &[][..],
                &[("г. Энск, ул. ***, д. 5, кв. 7", "ADDRESS")][..],
            ),
            (
                "Адрес: г. Энск, ул. <данные изъяты>, д. 5, кв. 7",
                &[][..],
                &[("д. 5, кв. 7", "ADDRESS")][..],
            ),
            (
                "Адрес: 999999, Энская область, г.о. Кедровск, <...>.",
                &[][..],
                &[("999999, Энская область, г.о. Кедровск", "ADDRESS")][..],
            ),
            (
                "Адрес: Россия, 999999, г. Энск, <данные изъяты>",
                &[][..],
                &[("Россия, 999999, г. Энск", "ADDRESS")][..],
            ),
            ("Адрес: 999999, Энск, <...>", &[][..], &[][..]),
            // P2-3: a public body's name stays outside its address.
            (
                "Администрация г. Энска, г. Энск, ул. Ленина, д. 1",
                &[("Администрация г. Энска", "organization")][..],
                &[("г. Энск, ул. Ленина, д. 1", "ADDRESS")][..],
            ),
            (
                "Администрация г. Энска, г. Энск, ул. Ленина, д. 1",
                &[][..],
                &[("г. Энск, ул. Ленина, д. 1", "ADDRESS")][..],
            ),
            (
                "Администрация г. Энска, г. Энск, ул. Ленина, д. 1",
                &[("Администрация", "organization")][..],
                &[("г. Энск, ул. Ленина, д. 1", "ADDRESS")][..],
            ),
            // P3: place names of any Cyrillic or Latin alphabet, a plot that is
            // a year, a case number after «д. №», a lowercase «к» before a
            // time, a house letter before a name.
            (
                "Адрес: г. Київ, ул. Хрещатик, д. 5.",
                &[][..],
                &[("г. Київ, ул. Хрещатик, д. 5", "ADDRESS")][..],
            ),
            (
                "Адрес: 070000, г. Өскемен, ул. Абая, д. 5.",
                &[][..],
                &[("070000, г. Өскемен, ул. Абая, д. 5", "ADDRESS")][..],
            ),
            ("<данные изъяты> уч. 2023 года", &[][..], &[][..]),
            (
                "Энская область, <данные изъяты> уч. 1234.",
                &[][..],
                &[("уч. 1234", "ADDRESS")][..],
            ),
            (
                "по ул. Лесной, д. № 2-1234/2020 рассмотрено",
                &[][..],
                &[][..],
            ),
            (
                "по ул. Лесной, д. № 2-1234/2020 рассмотрено",
                &[("2-1234/2020", "case number")][..],
                &[][..],
            ),
            (
                "по ул. Лесной, д. № 2-1234/2020",
                &[("№ 2-1234/2020", "case")][..],
                &[][..],
            ),
            (
                "Адрес: ул. Лесная, д. 12-1234 далее",
                &[][..],
                &[("ул. Лесная, д. 12", "ADDRESS")][..],
            ),
            (
                "явиться на ул. Лесная, д. 5 к 10 часам",
                &[][..],
                &[("ул. Лесная, д. 5", "ADDRESS")][..],
            ),
            (
                "Адрес: ул. Лесная, д. 5 В Петров И.И. явился",
                &[("Петров И.И.", "person")][..],
                &[("ул. Лесная, д. 5", "ADDRESS"), ("Петров И.И.", "PERSON")][..],
            ),
            (
                "Адрес: ул. Лесная, д. 5 В Петров И.И. явился",
                &[][..],
                &[("ул. Лесная, д. 5", "ADDRESS")][..],
            ),
            (
                "Магазин: ул. Лесная, д. 43 Г. Далее",
                &[][..],
                &[("ул. Лесная, д. 43 Г", "ADDRESS")][..],
            ),
            (
                "Адрес: ул. Лесная, д. 5, лит. Б Петров явился",
                &[][..],
                &[("ул. Лесная, д. 5, лит. Б", "ADDRESS")][..],
            ),
            (
                "Адрес: ул. Лесная, д. 43 Г\nДалее",
                &[][..],
                &[("ул. Лесная, д. 43 Г", "ADDRESS")][..],
            ),
            // The brief's shapes, statute references and model spans as before.
            (
                "согласно п. 3 ч. 1 ст. 5 ГК РФ, ст. 12, д. 5",
                &[][..],
                &[][..],
            ),
            (
                "в соответствии с пп. 1, 2 п. 3 ст. 5 Закона, ч. 2 ст. 15 АПК РФ",
                &[][..],
                &[][..],
            ),
            (
                "в г. 2026, ул. Лесная, д. 5",
                &[][..],
                &[("ул. Лесная, д. 5", "ADDRESS")][..],
            ),
            (
                "Решение от 12.03.2026 г., ул. Лесная, д. 5",
                &[][..],
                &[("ул. Лесная, д. 5", "ADDRESS")][..],
            ),
            (
                "Адрес: 191187, Санкт-Петербург, Литейный проспект, д. 7, офис 12.",
                &[][..],
                &[(
                    "191187, Санкт-Петербург, Литейный проспект, д. 7, офис 12",
                    "ADDRESS",
                )][..],
            ),
            (
                "Адрес: 197101, Санкт-Петербург, Ольховый переулок, д. 41, кв. 3.",
                &[][..],
                &[(
                    "197101, Санкт-Петербург, Ольховый переулок, д. 41, кв. 3",
                    "ADDRESS",
                )][..],
            ),
            (
                "Адрес: Республика Саха (Якутия), г. Якутск, ул. Ленина, д. 5.",
                &[][..],
                &[(
                    "Республика Саха (Якутия), г. Якутск, ул. Ленина, д. 5",
                    "ADDRESS",
                )][..],
            ),
            (
                "Тверская область, г. Ржев, ул. Тихих арок, д. 47, корп. 2, стр. 1, лит. Б, пом. 9",
                &[][..],
                &[(
                    "Тверская область, г. Ржев, ул. Тихих арок, д. 47, корп. 2, стр. 1, лит. Б, пом. 9",
                    "ADDRESS",
                )][..],
            ),
            (
                "Адрес: кв. 9, д. 32, переулок Бумажных лодок, Тверь, 170000",
                &[][..],
                &[(
                    "кв. 9, д. 32, переулок Бумажных лодок, Тверь, 170000",
                    "ADDRESS",
                )][..],
            ),
            (
                "по адресу Псков, ул. Тихих залов, д. 29",
                &[][..],
                &[("Псков, ул. Тихих залов, д. 29", "ADDRESS")][..],
            ),
            (
                "Арбитражный суд Приморского края\n690091, г. Владивосток, ул. Светланская, 54\nРЕШЕНИЕ",
                &[][..],
                &[("690091, г. Владивосток, ул. Светланская, 54", "ADDRESS")][..],
            ),
            (
                "191124, Санкт-Петербург, Суворовский пр., д. 50/52",
                &[][..],
                &[(
                    "191124, Санкт-Петербург, Суворовский пр., д. 50/52",
                    "ADDRESS",
                )][..],
            ),
            (
                "117997, г. Москва, ул. Садовническая, д. 68/70, стр. 1",
                &[][..],
                &[(
                    "117997, г. Москва, ул. Садовническая, д. 68/70, стр. 1",
                    "ADDRESS",
                )][..],
            ),
            (
                "125993, г. Москва, ГСП-3, ул. Тверская, д. 1",
                &[][..],
                &[("125993, г. Москва, ГСП-3, ул. Тверская, д. 1", "ADDRESS")][..],
            ),
            (
                "127994, г. Москва, ГСП-4, проезд Соломенной сторожки, 12",
                &[][..],
                &[(
                    "127994, г. Москва, ГСП-4, проезд Соломенной сторожки, 12",
                    "ADDRESS",
                )][..],
            ),
            (
                "г. Энск, ул. Лесная, д. 5, кв. 7, тел. 8 (999) 555-12-34",
                &[("г. Энск, ул. Лесная, д. 5, кв. 7, тел", "address")][..],
                &[("г. Энск, ул. Лесная, д. 5, кв. 7", "ADDRESS")][..],
            ),
            (
                "дом № 5 по ул. Лесной",
                &[("№ 5 по ул. Лесной", "address")][..],
                &[("5 по ул. Лесной", "ADDRESS")][..],
            ),
            (
                "г. Энск, ул. Лесная, участок № 5",
                &[("г. Энск, ул. Лесная, участок №", "address")][..],
                &[("г. Энск, ул. Лесная, участок № 5", "ADDRESS")][..],
            ),
            (
                "Магазин: Республика Энская, Кедровский район, ч. Кедровка, ул. Лесная, д. 43 Г.",
                &[("ул. Лесная, д. 43", "address")][..],
                &[(
                    "Республика Энская, Кедровский район, ч. Кедровка, ул. Лесная, д. 43 Г",
                    "ADDRESS",
                )][..],
            ),
            (
                "Иванов Иван Иванович, 999999, г. Энск, ул. Лесная, д. 5, кв. 7",
                &[
                    ("Иванов Иван Иванович", "person"),
                    ("г. Энск, ул. Лесная, д. 5, кв. 7", "address"),
                ][..],
                &[
                    ("Иванов Иван Иванович", "PERSON"),
                    ("999999, г. Энск, ул. Лесная, д. 5, кв. 7", "ADDRESS"),
                ][..],
            ),
            (
                "420066, Республика Татарстан, г. Казань, ул. Ново-Песочная, д. 3",
                &[][..],
                &[(
                    "420066, Республика Татарстан, г. Казань, ул. Ново-Песочная, д. 3",
                    "ADDRESS",
                )][..],
            ),
            (
                "410002, г. Саратов, ул. Бабушкин взвоз, д. 1",
                &[][..],
                &[("410002, г. Саратов, ул. Бабушкин взвоз, д. 1", "ADDRESS")][..],
            ),
            (
                "участок по адресу: г. Энск, ул. Лесная, д. 5, кадастровый № 99:07:0012345:678",
                &[("г. Энск, ул. Лесная, д. 5, кадастровый №", "address")][..],
                &[
                    ("г. Энск, ул. Лесная, д. 5", "ADDRESS"),
                    ("99:07:0012345:678", "IDENTITY"),
                ][..],
            ),
            (
                "ООО «Кедровый бор»: Россия 999999, Энск, УЛ ЦЕНТРАЛЬНАЯ, Д. 14, К. 4, ПОМЕЩ. 5-Н, ОФ. 1.",
                &[("Кедровый бор", "organization")][..],
                &[
                    ("ООО «Кедровый бор»", "ORG"),
                    (
                        "Россия 999999, Энск, УЛ ЦЕНТРАЛЬНАЯ, Д. 14, К. 4, ПОМЕЩ. 5-Н, ОФ. 1",
                        "ADDRESS",
                    ),
                ][..],
            ),
            (
                "Адрес: Россия 999999, Энск, … УЛ ЦЕНТРАЛЬНАЯ, Д. 14, К. 4, ПОМЕЩ. 5-Н, ОФ. 1.",
                &[][..],
                &[(
                    "Россия 999999, Энск, … УЛ ЦЕНТРАЛЬНАЯ, Д. 14, К. 4, ПОМЕЩ. 5-Н, ОФ. 1",
                    "ADDRESS",
                )][..],
            ),
            (
                "Адрес: 443000, Самарская область, Самара город, Московское (18 км) шоссе",
                &[][..],
                &[(
                    "443000, Самарская область, Самара город, Московское (18 км) шоссе",
                    "ADDRESS",
                )][..],
            ),
            (
                "Адрес: 445000, Самарская область, г. Тольятти, а/ 1",
                &[][..],
                &[("445000, Самарская область, г. Тольятти, а/ 1", "ADDRESS")][..],
            ),
            (
                "Магазин: Республика Тыва, Дзун-Хемчикский район, ч. Чадан, ул. Ленина, д. 43 Г.",
                &[][..],
                &[(
                    "Республика Тыва, Дзун-Хемчикский район, ч. Чадан, ул. Ленина, д. 43 Г",
                    "ADDRESS",
                )][..],
            ),
            (
                "Квартиры: ул. Кедровая, 10 кв. 1,2; ул. Сосновая, 12 кв. 1, 2, 4; ул. Луговая, 6, 9; ул. Еловая,16; ул. Озёрная, 25 кв. 1.",
                &[][..],
                &[
                    ("ул. Кедровая, 10 кв. 1,2", "ADDRESS"),
                    ("ул. Сосновая, 12 кв. 1, 2, 4", "ADDRESS"),
                    ("ул. Луговая, 6, 9", "ADDRESS"),
                    ("ул. Еловая,16", "ADDRESS"),
                    ("ул. Озёрная, 25 кв. 1", "ADDRESS"),
                ][..],
            ),
            (
                "Энская обл., г. Энск, Северный р-н, ул. им. Пахомова, д. 24, кв. 141",
                &[][..],
                &[(
                    "Энская обл., г. Энск, Северный р-н, ул. им. Пахомова, д. 24, кв. 141",
                    "ADDRESS",
                )][..],
            ),
            (
                "999999, г. Энск, ГСП-4, проезд Соломенной cторожки, 12",
                &[][..],
                &[(
                    "999999, г. Энск, ГСП-4, проезд Соломенной cторожки, 12",
                    "ADDRESS",
                )][..],
            ),
            (
                "680000, Хабаровский край, г. Хабаровск, ул. Ленина, д. 5",
                &[][..],
                &[(
                    "680000, Хабаровский край, г. Хабаровск, ул. Ленина, д. 5",
                    "ADDRESS",
                )][..],
            ),
            (
                "участок <...> Б, кадастровый № 99:07:0012345:678",
                &[("Б, кадастровый №", "address")][..],
                &[("99:07:0012345:678", "IDENTITY")][..],
            ),
            (
                "Московская область, Мытищинский р-н, <...> уч. 109",
                &[][..],
                &[("уч. 109", "ADDRESS")][..],
            ),
            (
                "(<...> д 31 к 2, кв 69б, ИНН:<***>)",
                &[][..],
                &[("д 31 к 2, кв 69б", "ADDRESS")][..],
            ),
            (
                "…, г. Энск, ул. Лесная, д. 5, , ком. 1-А",
                &[][..],
                &[("г. Энск, ул. Лесная, д. 5, , ком. 1-А", "ADDRESS")][..],
            ),
            (
                "а/я 15, г. Энск, 999999",
                &[][..],
                &[("а/я 15, г. Энск, 999999", "ADDRESS")][..],
            ),
            ("625010, ", &[][..], &[][..]),
            (
                "ул Лесная, д 5, кв 1, 2",
                &[][..],
                &[("ул Лесная, д 5, кв 1, 2", "ADDRESS")][..],
            ),
            (
                "Энская область, г.о. Кедровск, ул. Лесная, д. 5",
                &[][..],
                &[("Энская область, г.о. Кедровск, ул. Лесная, д. 5", "ADDRESS")][..],
            ),
            (
                "УЛ ЛЕСНАЯ, Д. 14 Б, К. 4",
                &[][..],
                &[("УЛ ЛЕСНАЯ, Д. 14 Б, К. 4", "ADDRESS")][..],
            ),
            (
                "ул. Лесная, 14 Б",
                &[][..],
                &[("ул. Лесная, 14 Б", "ADDRESS")][..],
            ),
            (
                "ул. Луговая, 6, 9, кв. 3",
                &[][..],
                &[("ул. Луговая, 6, 9, кв. 3", "ADDRESS")][..],
            ),
            (
                "ул. Луговая, 6/2, кв. 3",
                &[][..],
                &[("ул. Луговая, 6/2, кв. 3", "ADDRESS")][..],
            ),
            (
                "РОССИЙСКАЯ ФЕДЕРАЦИЯ, 999999, Г.ЭНСК, ПР-Т ЛЕНИНА, Д. 5, ОФИС 3",
                &[][..],
                &[(
                    "РОССИЙСКАЯ ФЕДЕРАЦИЯ, 999999, Г.ЭНСК, ПР-Т ЛЕНИНА, Д. 5, ОФИС 3",
                    "ADDRESS",
                )][..],
            ),
            (
                "РОССИЯ, 999999, ЭНСК, УЛ ЛЕСНАЯ, Д. 5, ПОМЕЩЕНИЕ I, КОМН. 2",
                &[][..],
                &[(
                    "РОССИЯ, 999999, ЭНСК, УЛ ЛЕСНАЯ, Д. 5, ПОМЕЩЕНИЕ I, КОМН. 2",
                    "ADDRESS",
                )][..],
            ),
            (
                "Адрес: г. Энск, Ленинский пр-т, д. 5, кв. 3",
                &[][..],
                &[("г. Энск, Ленинский пр-т, д. 5, кв. 3", "ADDRESS")][..],
            ),
            (
                "Адрес: г. Энск, ул. имени Пахомова, д. 5",
                &[][..],
                &[("г. Энск, ул. имени Пахомова, д. 5", "ADDRESS")][..],
            ),
            (
                "Адрес: г. Энск, ул. Лесная, д. 5, эт. 2, пом. 3",
                &[][..],
                &[("г. Энск, ул. Лесная, д. 5, эт. 2, пом. 3", "ADDRESS")][..],
            ),
            (
                "999999, ЭНСКАЯ ОБЛАСТЬ, Г.О. КЕДРОВСК, УЛ ЛЕСНАЯ, Д. 5",
                &[][..],
                &[(
                    "999999, ЭНСКАЯ ОБЛАСТЬ, Г.О. КЕДРОВСК, УЛ ЛЕСНАЯ, Д. 5",
                    "ADDRESS",
                )][..],
            ),
            (
                "АРБИТРАЖНЫЙ СУД ГОРОДА МОСКВЫ\n115225, г.Москва, ул. Б. Тульская, 17\nhttp://www.msk.arbitr.ru",
                &[][..],
                &[("115225, г.Москва, ул. Б. Тульская, 17", "ADDRESS")][..],
            ),
            (
                "Девятый арбитражный апелляционный суд 127994, Москва, ГСП-4, проезд Соломенной сторожки, 12, адрес электронной почты",
                &[][..],
                &[(
                    "127994, Москва, ГСП-4, проезд Соломенной сторожки, 12",
                    "ADDRESS",
                )][..],
            ),
            (
                "кв. 9, д. 32, ул. Лесная, г. Энск; ул. Садовая, д. 7, кв. 1",
                &[][..],
                &[
                    ("кв. 9, д. 32, ул. Лесная, г. Энск", "ADDRESS"),
                    ("ул. Садовая, д. 7, кв. 1", "ADDRESS"),
                ][..],
            ),
            (
                "ул. Лесная, д. 5, кв. 7, ул. Садовая, д. 7",
                &[][..],
                &[
                    ("ул. Лесная, д. 5, кв. 7", "ADDRESS"),
                    ("ул. Садовая, д. 7", "ADDRESS"),
                ][..],
            ),
            (
                "Адрес: д. 5, № 7",
                &[("д. 5, №", "address")][..],
                &[("д. 5", "ADDRESS")][..],
            ),
            (
                "<...> д. 5, кв. 7 Петров Иван",
                &[("Петров Иван", "person")][..],
                &[("д. 5, кв. 7", "ADDRESS"), ("Петров Иван", "PERSON")][..],
            ),
            (
                "Адрес: ул. Лесная, д. 5, к. 2, кв. 7.",
                &[][..],
                &[("ул. Лесная, д. 5, к. 2, кв. 7", "ADDRESS")][..],
            ),
            (
                "Адрес: ул. Лесная, д.5 к.2 кв.7.",
                &[][..],
                &[("ул. Лесная, д.5 к.2 кв.7", "ADDRESS")][..],
            ),
            (
                "УЛ ЛЕСНАЯ, Д 5 К 2, КВ 7",
                &[][..],
                &[("УЛ ЛЕСНАЯ, Д 5 К 2, КВ 7", "ADDRESS")][..],
            ),
            (
                "Адрес: ул. Лесная, д. 5-7, кв. 3.",
                &[][..],
                &[("ул. Лесная, д. 5-7, кв. 3", "ADDRESS")][..],
            ),
            (
                "Harrow House, 12 Pellin Street, Bristol BS6 9ZY",
                &[("Harrow House", "organization")][..],
                &[("Harrow House, 12 Pellin Street, Bristol BS6 9ZY", "ADDRESS")][..],
            ),
            (
                "Пахомова ул., д. 5, кв. 3",
                &[("Пахомова", "person")][..],
                &[("Пахомова", "PERSON")][..],
            ),
            (
                "Адрес: ул. им. Пахомова, д. 5",
                &[("Пахомова", "person")][..],
                &[("ул. им. Пахомова, д. 5", "ADDRESS")][..],
            ),
            ("в силу ст. 12, ч. 1 ст. 5 ГК РФ, д. 5", &[][..], &[][..]),
        ];
        for (source, raw, expected) in rows {
            assert_eq!(shown(source, raw), pairs(expected), "{source}");
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
            // postcode alone (#506 item 6).
            "Brindle House, Leeds",
            "High Court, Slocombe WC2A 9ZZ",
            "BRANWICK, CO 81999",
            "Brindlecombe, OH 45999",
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

    /// #498 round two: each separator and street shape ends at its own
    /// city/postcode, with adjacent model names and the next field intact.
    #[test]
    fn round_two_address_shapes_keep_names_and_fields_outside() {
        for address in [
            "103 Pellin Parkway, Suite 305 Slocombe, New Jersey 07999",
            "21495 Quarry Circle, Suite 304A Brindlecombe, Virginia 20999",
            "200 East Long Quarry Road Suite 300 Brindle Hills, Michigan 48999",
            "63 Pellam Street Suite 202 Slocombe, NJ 08999",
            "242A West Harrow Road Ostrand,NJ 07999",
            "1600 Pellin Hill Road, Slocombe, Georgia 30999",
            "500 Pellin Avenue, Unit 16C, New York",
            "500 Pellin Avenue, Unit 16C, New York, New York 10999",
            "224 W 299 St., Bronx, NY 10999",
            "224 W. 299 St., Bronx, NY 10999",
            "One Pellin Plaza, Suite 3335 New York, New York 10999",
            "One Pellin Plaza New York, New York 10999",
            "Two Pellin Plaza, Suite 4, Slocombe, NY 10999",
            "225 Broadmoor, Suite 2700 New York, N.Y. 10999",
            "225 Broadmoor-Suite 2700 New York, N.Y. 10999",
            "1565N. Harrow Expressway, Suite 200, Slocombe, TX 75999",
            "100 N State Line Ave #16, Slocombe, TX 75999",
            "2121 Quillon Ave. N.W., Wexford, DC 20999",
            "2121 N.W. Quillon Ave., Wexford, DC 20999",
            "10th Floor, 42 Pellin Pl., Bldg. 4, Slocombe, TX 75999",
            "42 Pellin Fwy., Floor 3, Slocombe, TX 75999",
            "7528 Rooke Road Penrow Township",
            "24, Pellin Court, Harrow Place, Worcester",
            "20th Street and Quillon Avenue, NW Wexford, DC 20999",
            "P.O. Box 4410, Brindle Falls, OH 45999",
            "P.O. Box 4410 Brindle Falls, OH 45999",
            "P.O. Box 4410, Slocombe",
            "190 South Pellin Street: P.O. Box 351 Slocombe, NY 10999",
            "24 North Street\nP.O. Box 1308\nSlocombe, MA 02999",
            "а/я 4410, Ольховск, 199999",
            "а/я 4410, 199999, Ольховск",
            "а/я 4410, Ольховск",
        ] {
            let source = format!(
                "Larkfield Supply LLC, Jane Quenby, {address}; Email: post@quillon.invalid\nUK\naddress"
            );
            let found: Vec<_> = pack()
                .spans(&source)
                .into_iter()
                .filter(|span| span.category == Category::Address)
                .map(|span| source[span.range].to_owned())
                .collect();
            assert_eq!(found, [address], "{source}");
            assert_eq!(
                shown(
                    &source,
                    &[
                        ("Larkfield Supply LLC", "organization"),
                        ("Jane Quenby", "person"),
                    ]
                ),
                pairs(&[
                    ("Larkfield Supply LLC", "ORG"),
                    ("Jane Quenby", "PERSON"),
                    (address, "ADDRESS"),
                    ("post@quillon.invalid", "EMAIL"),
                ]),
                "{source}"
            );
        }
    }

    /// Decisions #506 item 7 and #553 items 1/3: a body's street is an
    /// address, a bare street or corner is not, even from the model.
    #[test]
    fn address_decisions_keep_bodies_and_bare_streets_outside() {
        for (body, address) in [
            ("Royal Courts of Justice", "Strand, Slocombe, WC2A 9ZZ"),
            (
                "Securities and Exchange Commission",
                "100 F Street, NE, Wexford, DC 20999",
            ),
        ] {
            let source = format!("{body}\n{address}\nUK");
            assert_eq!(
                shown(&source, &[(body, "organization")]),
                pairs(&[(address, "ADDRESS")]),
                "{source}"
            );
        }
        for street in [
            "Cedar Court",
            "Larch Rd",
            "Sunset Boulevard",
            "Mulholland Dr",
            "Cherry Ln",
            "Summerhill Rd. and Richmond Rd.",
            "20th Street and Quillon Avenue",
            "Strand",
        ] {
            assert!(shown(street, &[(street, "address")]).is_empty(), "{street}");
        }
        for source in [
            "The Suite 300 Road plan follows One Long Street discussion.",
            "There are 224 W 299 St. references in the index.",
            "One Pellin Plaza was mentioned in the report.",
            "225 Broadmoor agreed to the Suite 2700 proposal.",
            "P.O. Box 4410 is a label without a destination.",
            "а/я 4410 содержит письма.",
            "The Floor and Bldg. labels describe the Fwy. and Pl. columns.",
            "Summerhill Rd. and Richmond Rd. meet near the park.",
        ] {
            assert!(pack().spans(source).is_empty(), "{source}");
        }
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
            // pattern ends before Chatham House and keeps the street.
            (
                "Lives at 12 Oak Road, Chatham House, 12 March 2026.\n",
                &[("Chatham House", "organization")],
                &[("12 Oak Road", "ADDRESS"), ("Chatham House", "ORG")],
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
            [
                ("12 Oak Road".to_owned(), "ADDRESS".to_owned()),
                ("John Smith".to_owned(), "PERSON".to_owned())
            ]
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
            1
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

    /// A two-digit year after DOB, and a date before "birth date", "date of
    /// birth", «года рождения» or «г.р.», wrapped at a line break or not, are
    /// dates of birth; the label stays out (#363). A bare year, a hearing
    /// date and a date beside another "birth" stay.
    #[test]
    fn dates_of_birth_with_short_years_and_labels_after() {
        let dates = |source: &str| -> Vec<String> {
            crate::recognition::proposals(source, &[])
                .unwrap()
                .into_iter()
                .filter(|span| span.category == "DATE")
                .map(|span| source[span.range].to_owned())
                .collect()
        };
        for (source, date) in [
            ("DOB 3 Jun 61. Nationality: Ruritanian.", "3 Jun 61"),
            ("DOB: 03/06/61; POB: Zenda.", "03/06/61"),
            ("| DOB | 3 Sept. 1961 |", "3 Sept. 1961"),
            ("He gave a March 9, 1984 birth date.", "March 9, 1984"),
            ("He gave a March 9,\n1984 birth date.", "March 9,\n1984"),
            ("a 9 March\r\n1984 birth date", "9 March\r\n1984"),
            ("with a 03/06/1961 date of birth", "03/06/1961"),
            ("born March 9,\n1984, in Zenda", "March 9,\n1984"),
            (
                "Ответчик Иванов И. И., 01.01.1980 года рождения, проживает",
                "01.01.1980",
            ),
            ("Петров П. П., 14 марта 1988 г.р., паспорт", "14 марта 1988"),
            ("Сидоров С. С. (01.02.1990 г. р.)", "01.02.1990"),
            ("Орлова О. О., 01.02.1990 г.р.", "01.02.1990"),
            ("Иванов И. И., 01.01.1980 г.р., проживает", "01.01.1980"),
            ("Иванов И. И., 1 января 1980 г. рождения", "1 января 1980"),
            ("He gave a March 9, 1984 date of birth.", "March 9, 1984"),
            ("He gave a 9 March 1984 date-of-birth.", "9 March 1984"),
            ("He gave a March 9, 1984 birthdate.", "March 9, 1984"),
            ("the claimant's 12/04/1979 date of birth", "12/04/1979"),
            (
                "the visa form records a 12/04/1979 date of birth for his brother",
                "12/04/1979",
            ),
            ("DOB 03-Jun-61; POB Zenda.", "03-Jun-61"),
            ("DOB 03-Jun-1961; POB Zenda.", "03-Jun-1961"),
            ("Birthdate: 03/04/1980", "03/04/1980"),
            // A capitalised label opens the next field, and only its own
            // value is a date of birth (#363 review P1-2).
            (
                "Date of issue 01/02/2020 Date of birth 03/04/1980",
                "03/04/1980",
            ),
            (
                "Date of death: 12 March 2020 Date of birth: 1 January 1950",
                "1 January 1950",
            ),
            (
                "Passport issued 14 March 2015  date of birth  3 June 1961",
                "3 June 1961",
            ),
            ("Hearing date 3 Jun 26 Date of birth 3 Jun 61", "3 Jun 61"),
            (
                "| Issued | 01/02/2020 Date of birth 03/04/1980 |",
                "03/04/1980",
            ),
        ] {
            assert_eq!(dates(source), [date], "{source:?}");
        }
        for source in [
            "The hearing is listed for 3 Jun 26.",
            "Hearing: 12 March 2026\nDate of birth: unknown",
            "Signed 12 March 2026. Date of birth: unknown.",
            "She was born in 1988 in Leeds.",
            "a birth certificate No. 12/03/1988 issued 14 March 1988",
            "On March 9, 1984 birth certificates were reissued.",
            "Rule 123 March 1984 birth date",
            // Over a blank line a date is two paragraphs' text: one span
            // would join them.
            "He gave a March 9,\n\n1984 birth date.",
            "Иванов И. И., 1980 г.р., проживает",
            "Иванов И. И., 1980 года рождения",
            "свидетельство о рождении от 01.01.1980 выдано",
            "решение от 01.01.2026 г. вступило в силу",
            // A label after a date: lowercase, an article or a possessive
            // before the date, and the phrase ends with it (review P1-2).
            "Signed 12 March 2026 Date of birth: unknown",
            "On 12 March 2026 date of birth evidence was filed.",
            "By 1 March 2026 birth date records must be corrected.",
            "the court's 12 March 2026 birth date correction order",
            "He gave a 12 March 2026 date of birth: unknown",
            "3 June 1961 date of birth",
            // «г.р.» lowercase, its dots written, on the date's line: no
            // initial after «г.» and no account (review P2-1).
            "Решением суда от 12.03.2026 г. Р.А. Иванову отказано.",
            "Дата: 12.03.2026 г.\nР. А. Иванов",
            "12.03.2026 г.                Р.А. Иванов",
            "договор от 01.02.2020 Г.Р. Петровой подписан",
            "оплачено 01.02.2020 г. р/с 40702810000000000001",
            // A wrapped year ends its line or punctuation follows (review
            // P2-3).
            "Date of birth: 14 March\n2026 hearing listed.",
            "born on March 9,\n2019 the claimant moved.",
            // A number's tail is no date of its own (review P3).
            "Passport AB-12/04/1979 date of birth unknown",
            "Passport No. 12/04/1979 date of birth",
        ] {
            assert!(dates(source).is_empty(), "{source:?}: {:?}", dates(source));
        }
    }

    /// #363 review P1-1: a date before its label marks its repeats only
    /// before the same label; the date of a hearing, a decision or a
    /// contract that repeats it stays, and so does one at a paragraph's
    /// start. Each source's DATE spans, by where they start.
    #[test]
    fn a_date_before_its_label_repeats_only_before_that_label() {
        let dates = |source: &str| -> Vec<usize> {
            crate::recognition::proposals(source, &[])
                .unwrap()
                .into_iter()
                .filter(|span| span.category == "DATE")
                .map(|span| span.range.start)
                .collect()
        };
        let nth = |source: &str, value: &str, n: usize| -> usize {
            source.match_indices(value).nth(n).unwrap().0
        };
        for source in [
            "Решение от 12.03.2026 вынесено. Заседание 12.03.2026 окончено.\n\n12.03.2026 г.     Р.А. Иванов",
            "The hearing of 3 June 1961 was adjourned to 3 June 1961 at noon.\n\n3 June 1961 date of birth",
            "по договору от 12.03.2026 г. Р.А. Иванов передал. Иск подан по договору от 12.03.2026 и принят.",
        ] {
            assert!(dates(source).is_empty(), "{source:?}: {:?}", dates(source));
        }
        let source = "He gave a 12 March 1984 birth date. He gave a 12 March 1984 address too.";
        assert_eq!(dates(source), [nth(source, "12 March 1984", 0)]);
        let source = "Иванов И. И., 01.01.1980 года рождения. Договор займа от 01.01.1980 подписан.\n\n01.01.1980 г. Иванов";
        assert_eq!(dates(source), [nth(source, "01.01.1980", 0)]);
        // The same label after a repeat: the same date of birth, even where
        // the rule itself would not take it (no article before it).
        let source = "He gave a 12 March 1984 birth date. 12 March 1984 birth date, he said.";
        let both = [
            nth(source, "12 March 1984", 0),
            nth(source, "12 March 1984", 1),
        ];
        assert_eq!(dates(source), both);
        let source = "Иванов И. И., 01.01.1980 г.р. Иск Иванова (01.01.1980 г. р.) удовлетворён.";
        let both = [nth(source, "01.01.1980", 0), nth(source, "01.01.1980", 1)];
        assert_eq!(dates(source), both);
    }

    /// #363 review P2-2, P2-3 and P3: a model's name or address beside a
    /// labelled date keeps its own words, its part away from the label; a
    /// wrapped year never takes the next line's house number; a passport
    /// number stays whole.
    #[test]
    fn names_and_addresses_beside_a_date_of_birth_stay() {
        for (source, raw, expected) in [
            (
                "Иванов Иван Иванович, 01.01.1980 года рождения",
                &[("Иванов Иван Иванович, 01.01.1980", "person")][..],
                &[("Иванов Иван Иванович", "PERSON"), ("01.01.1980", "DATE")][..],
            ),
            (
                "Ответчик Петров П. П., 14 марта 1988 г.р.",
                &[("Петров П. П., 14", "person")],
                &[("Петров П. П.", "PERSON"), ("14 марта 1988", "DATE")],
            ),
            (
                "DOB 3 Jun 61 IVANOV, Ivan",
                &[("61 IVANOV, Ivan", "person")],
                &[("3 Jun 61", "DATE"), ("IVANOV, Ivan", "PERSON")],
            ),
            (
                "DOB 3 Jun 1961 Ivo Harrow",
                &[("1961 Ivo Harrow", "person")],
                &[("3 Jun 1961", "DATE"), ("Ivo Harrow", "PERSON")],
            ),
            // No article before the date: no date of birth, the name as
            // the model gave it.
            (
                "Ivanov Ivan 3 Jun 61 birth date",
                &[("Ivanov Ivan 3", "person")],
                &[("Ivanov Ivan 3", "PERSON")],
            ),
            (
                "He gave a March 9,\n1984 birth date. Ivo Harrow signed.",
                &[("Ivo Harrow", "person")],
                &[("March 9,\n1984", "DATE"), ("Ivo Harrow", "PERSON")],
            ),
            (
                "Date of birth: 4 July\n1600 Pennsylvania Avenue NW, Washington",
                &[("1600 Pennsylvania Avenue NW, Washington", "address")],
                &[("1600 Pennsylvania Avenue NW, Washington", "ADDRESS")],
            ),
            (
                "Date of birth: March 9,\n1984 Harrow Road, Zenda",
                &[("1984 Harrow Road, Zenda", "address")],
                &[("1984 Harrow Road, Zenda", "ADDRESS")],
            ),
            (
                "Ivo Harrow, born March 9,\n1984 Harrow Road, Zenda",
                &[
                    ("Ivo Harrow", "person"),
                    ("1984 Harrow Road, Zenda", "address"),
                ],
                &[
                    ("Ivo Harrow", "PERSON"),
                    ("1984 Harrow Road, Zenda", "ADDRESS"),
                ],
            ),
            (
                "Passport AB-12/04/1979 date of birth unknown",
                &[("AB-12/04/1979", "passport_number")],
                &[("AB-12/04/1979", "IDENTITY")],
            ),
            (
                "Passport No. 12/04/1979 date of birth",
                &[("12/04/1979", "passport_number")],
                &[("12/04/1979", "IDENTITY")],
            ),
        ] {
            assert_eq!(shown(source, raw), pairs(expected), "{source:?}");
        }
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

    /// #482: a settlement's type inside a Russian address keeps the
    /// address whole, from its index to its last part, and a settlement
    /// with a house and no street is an address too.
    #[test]
    fn russian_settlement_types_keep_an_address_whole() {
        // The model's address cut at «ст.» grows to the whole of it.
        let source = "Ответчик проживает: 353999, Энский край, ст. Кедровская, ул. Садовая, д. 5.";
        assert_eq!(
            shown(source, &[("353999, Энский край, ст", "address")]),
            pairs(&[(
                "353999, Энский край, ст. Кедровская, ул. Садовая, д. 5",
                "ADDRESS"
            )]),
        );
        // A workers' settlement keeps the index and the region before it,
        // its dot spaced, unspaced or dropped, and in capitals.
        for address in [
            "431999, Энская область, р. п. Кедровка, улица Садовая, дом 8",
            "431999, Энская область, р.п. Кедровка, улица Садовая, дом 8",
            "431999, Энская область, рп Кедровка, улица Садовая, дом 8",
            "431999, ЭНСКАЯ ОБЛАСТЬ, Р.П. КЕДРОВКА, УЛ САДОВАЯ, Д. 8",
        ] {
            let source = format!("Адрес: {address}.");
            // The model's address from the street on.
            let street = source.find("ул").or_else(|| source.find("УЛ")).unwrap();
            let model = &source[street..source.len() - 1];
            assert_eq!(
                shown(&source, &[(model, "address")]),
                pairs(&[(address, "ADDRESS")]),
                "{source}"
            );
            assert_eq!(
                shown(&source, &[]),
                pairs(&[(address, "ADDRESS")]),
                "{source}"
            );
        }
        // Every settlement type before a street.
        for settlement in [
            "ст. Кедровская",
            "ст-ца Кедровская",
            "станица Кедровская",
            "а. Новый Хутор",
            "аул Новый Хутор",
            "х. Тихий",
            "пгт Кедровый",
            "с. Кедровка",
            "д. Кедровка",
            "п. Кедровый",
            "пос. Кедровый",
            "ч. Кедровка",
        ] {
            let address = format!("353999, Энский край, {settlement}, ул. Садовая, д. 5");
            let source = format!("Ответчик проживает: {address}; уроженец а. Новый Хутор.");
            assert_eq!(
                shown(&source, &[]),
                pairs(&[(&address, "ADDRESS")]),
                "{source}"
            );
        }
        // A settlement with a house and no street, a garrison's officers'
        // house («ДОС») too, with or without an index before it; the
        // model's person in it is the address's.
        for (source, raw, expected) in [
            (
                "Квартиры: п. Кедровый ключ ДОС № 1, кв. 2.",
                &[("п. Кедровый ключ", "person")][..],
                &["п. Кедровый ключ ДОС № 1, кв. 2"][..],
            ),
            (
                "Квартиры: п. Кедровый ключ ДОС № 1, кв. 2; п. Кедровый ключ ДОС-3, кв. 7.",
                &[],
                &[
                    "п. Кедровый ключ ДОС № 1, кв. 2",
                    "п. Кедровый ключ ДОС-3, кв. 7",
                ],
            ),
            (
                "Адрес: 431999, Энская область, с. Кедровка, д. 5.",
                &[],
                &["431999, Энская область, с. Кедровка, д. 5"],
            ),
            (
                "Адрес: Кедровский район, д. Тихая, д. 12, кв. 3.",
                &[],
                &["Кедровский район, д. Тихая, д. 12, кв. 3"],
            ),
        ] {
            let expected: Vec<_> = expected.iter().map(|text| (*text, "ADDRESS")).collect();
            assert_eq!(shown(source, raw), pairs(&expected), "{source}");
        }
        // A bare word before the settlement is a name, outside the address.
        assert_eq!(
            shown(
                "Истец: Иванов, с. Кедровка, д. 5, кв. 2.",
                &[("Иванов", "person")]
            ),
            pairs(&[
                ("Иванов", "PERSON"),
                ("с. Кедровка, д. 5, кв. 2", "ADDRESS")
            ]),
        );
    }

    /// #482 guards: a place alone is no address, and initials or a house
    /// after a person's name make no settlement.
    #[test]
    fn russian_settlements_alone_and_initials_make_no_address() {
        for source in [
            "Ответчик, уроженец а. Новый Хутор, явился.",
            "Решение районного суда г. Ольховск.",
            "Ответчик проживает в с. Кедровка с 2020 года.",
            "Квартиры: п. Кедровый ключ.",
            "согласно п. 3 ч. 2 ст. 5, д. 5 по делу",
        ] {
            assert!(shown(source, &[]).is_empty(), "{source}");
        }
        assert_eq!(
            shown(
                "Свидетель А.С. Петров, д. 5 по делу.",
                &[("А.С. Петров", "person")]
            ),
            pairs(&[("А.С. Петров", "PERSON")]),
        );
        assert_eq!(
            shown(
                "Свидетель С. Петров, д. 5, кв. 2.",
                &[("С. Петров", "person")]
            ),
            pairs(&[("С. Петров", "PERSON")]),
        );
    }

    /// #482: a settlement's type before a place is no person's initial,
    /// after a word that leads a place or with a place's ending.
    #[test]
    fn settlement_types_before_a_place_are_no_initials() {
        for (source, person) in [
            (
                "Ответчик, уроженец а. Новый Хутор, явился.",
                "а. Новый Хутор",
            ),
            ("Ответчик, уроженец а. Новый Хутор, явился.", "Хутор"),
            ("Ответчица, уроженка с. Кедровка, явилась.", "с. Кедровка"),
            ("Письмо из х. Тихий получено.", "х. Тихий"),
            (
                "Решение районного суда г. Ольховск; проведено в г. Ольховск.",
                "г. Ольховск",
            ),
            (
                "Решение Районного суда г. Пёнсан вступило в силу.",
                "г. Пёнсан",
            ),
            (
                "Осмотр проведен в пос. Кедровый ключ.",
                "пос. Кедровый ключ",
            ),
            ("Обыск проведен в ст. Кедровская.", "ст. Кедровская"),
            ("Список: с. Кедровка.", "с. Кедровка"),
            ("Список: г. Ольховск.", "г. Ольховск"),
        ] {
            assert!(
                shown(source, &[(person, "person")]).is_empty(),
                "{source}: {person}"
            );
        }
    }

    /// #482 review: a lowercase «г.» is a title or a year's end, and «с.»,
    /// «ст.» a stray dot, before a real surname, which stays PERSON: a
    /// genitive («-ука», «-ака», «-ка»), an adjective and a verb, a year on
    /// the line before or with its ending («1990-х г.»), an instrumental
    /// after «с.», a capitalised word after an adjective after «в».
    #[test]
    fn surnames_after_lowercase_types_stay_people() {
        for (source, person) in [
            (
                "Постановлением от 12 марта 2020
г. Ковальская признана потерпевшей.",
                "Ковальская",
            ),
            (
                "Постановлением от 12 марта 2020
г. Ковальчука признали потерпевшим.",
                "Ковальчука",
            ),
            (
                "Постановлением от 12 марта 2020
г. Кедровка признана потерпевшей.",
                "Кедровка",
            ),
            ("в 1990-х г. Ковальская переехала.", "Ковальская"),
            ("в 2020-м г. Ковальская работала.", "Ковальская"),
            ("в 2020-м г. Кедровка работала.", "Кедровка"),
            (
                "В судебном заседании г. Ковальская пояснила, что иск не признает.",
                "Ковальская",
            ),
            (
                "В судебном заседании г. Ковальская пояснила, что иск не признает.",
                "г. Ковальская",
            ),
            ("Возражения г. Ковальский поддержал.", "Ковальский"),
            ("Документы получены от г. Ковальчука лично.", "Ковальчука"),
            (
                "Документы получены от г. Ковальчука лично.",
                "г. Ковальчука",
            ),
            ("Подпись г. Новака заверена нотариусом.", "Новака"),
            ("Согласно показаниям г. Бойка установлено.", "Бойка"),
            ("Иск подан в интересах г. Марка.", "Марка"),
            ("Подписал ст. Ковальчука не видел.", "Ковальчука"),
            ("Показания с. Ковальчука оглашены.", "Ковальчука"),
            ("Показания с. Ковальчука оглашены.", "с. Ковальчука"),
            ("Суд с. Ковальчуком согласился.", "Ковальчуком"),
            ("Суд с. Ковальчуком согласился.", "с. Ковальчуком"),
            ("проведено в г. Нижний Ковальчук.", "Ковальчук"),
            // A place alone, with no word that leads one and no address
            // part after it, keeps the model's person: the price of
            // keeping a title's name ("г. Ковальская пояснила").
            ("Список: п. Кедровый ключ.", "п. Кедровый ключ"),
        ] {
            assert_eq!(
                shown(source, &[(person, "person")]),
                pairs(&[(person, "PERSON")]),
                "{source}: {person}"
            );
        }
    }

    /// #482 review: a settlement-and-house address takes no surname after
    /// «с.» or «д.» (a case file), no name after the place and no phone's
    /// digits as its index.
    #[test]
    fn settlement_addresses_take_no_names_or_phones() {
        for (source, person) in [
            ("Договорился с. Петровым, д. 5 и прочее.", "Петровым"),
            ("Договорился с. Петровым, д. 5 и прочее.", "с. Петровым"),
            ("Материалы д. Ковальчука, д. 3 приобщены.", "Ковальчука"),
            (
                "проживает в с. Кедровка Ковальчук Иван, д. 5",
                "Ковальчук Иван",
            ),
            ("Квартиры: п. Кедровый ключ Ковальчук ДОС № 1", "Ковальчук"),
        ] {
            assert_eq!(
                shown(source, &[(person, "person")]),
                pairs(&[(person, "PERSON")]),
                "{source}: {person}"
            );
        }
        assert_eq!(
            shown("Проживает: 8-999-123456, с. Кедровка, д. 5", &[]),
            pairs(&[("с. Кедровка, д. 5", "ADDRESS")]),
        );
        assert_eq!(
            shown("Проживает: 8–999–123456, с. Кедровка, д. 5", &[]),
            pairs(&[("с. Кедровка, д. 5", "ADDRESS")]),
        );
        // A place's name of two capitalised words, or a village's that
        // ends as no surname does, is still the address's.
        for address in [
            "с. Новое Село, д. 5, кв. 2",
            "д. Иваново, д. 5, кв. 2",
            "с. Кедровка, д. 5",
            "а. Новый Хутор, д. 7",
        ] {
            let source = format!("Проживает: {address}.");
            assert_eq!(
                shown(&source, &[]),
                pairs(&[(address, "ADDRESS")]),
                "{source}"
            );
        }
    }

    /// #482 guards: initials, a title's name, a surname after a stray dot
    /// and a name after the place stay PERSON.
    #[test]
    fn initials_beside_settlement_guards_stay_people() {
        for (source, person, expected) in [
            (
                "Ответчик Иванов А. Б. явился.",
                "Иванов А. Б.",
                "Иванов А. Б.",
            ),
            ("Договор подписал А. Петров.", "А. Петров", "А. Петров"),
            ("Заявление подал г-н Петров.", "г-н Петров", "Петров"),
            (
                "Договорился с. Петровым о встрече.",
                "с. Петровым",
                "с. Петровым",
            ),
            (
                "Договорился с П. Петровым о встрече.",
                "П. Петровым",
                "П. Петровым",
            ),
            (
                "Договор подписан в г. Ольховск Ивановым.",
                "Ивановым",
                "Ивановым",
            ),
            (
                "Протокол от 12.03.2026 г. Иванка Петрова подписала.",
                "Иванка",
                "Иванка",
            ),
            (
                "Свидетель А.С. Петров явился.",
                "А.С. Петров",
                "А.С. Петров",
            ),
        ] {
            assert_eq!(
                shown(source, &[(person, "person")]),
                pairs(&[(expected, "PERSON")]),
                "{source}"
            );
        }
    }

    fn pairs(expected: &[(&str, &str)]) -> Vec<(String, String)> {
        expected
            .iter()
            .map(|(text, category)| ((*text).to_owned(), (*category).to_owned()))
            .collect()
    }

    /// #546: defined party labels disappear before the review propagates
    /// names, whether the model calls them people or organizations.
    #[test]
    fn defined_party_codes_are_not_names() {
        let source = "The claimants are Ivo Penrow (“C1”) and Edda Brannock (“C2”); \
            the defendants are Pellam Rooke Ltd (“D1”) and Harrow Ostrand (“D2”).\n\
            a. C1 sued D1; C1's trustees and D2 replied; D3 – D5 took no part.";
        for label in ["person", "organization"] {
            assert_eq!(
                shown(
                    source,
                    &[
                        ("Ivo Penrow", "person"),
                        ("Edda Brannock", "person"),
                        ("Pellam Rooke Ltd", "organization"),
                        ("Harrow Ostrand", "person"),
                        ("C1", label),
                        ("C2", label),
                        ("D1", label),
                        ("D2", label),
                        ("a. C1", label),
                        ("C1's trustees", label),
                        ("D3 – D5", label),
                    ]
                ),
                pairs(&[
                    ("Ivo Penrow", "PERSON"),
                    ("Edda Brannock", "PERSON"),
                    ("Pellam Rooke Ltd", "ORG"),
                    ("Harrow Ostrand", "PERSON")
                ])
            );
        }
    }

    #[test]
    fn party_code_definitions_keep_the_real_name_and_cover_each_series() {
        for (role, code, next) in [
            ("Claimant", "C1", "C12"),
            ("Defendant", "D1", "D12"),
            ("Plaintiff", "P1", "P12"),
            ("Respondent", "R1", "R12"),
        ] {
            for definition in [
                format!("Mrs Penrow ({code})"),
                format!("Pellam Ltd (\"{code}\", as appropriate)"),
                format!("{role}: {code}"),
                format!("{code} – {role}"),
            ] {
                let source = format!("{definition}. {role}: {next}; {next}’s trustees.\nc. {next}");
                assert_eq!(
                    shown(
                        &source,
                        &[
                            (code, "person"),
                            (next, "organization"),
                            (&format!("{next}’s trustees"), "organization"),
                            (&format!("c. {next}"), "person")
                        ]
                    ),
                    [],
                    "{source}"
                );
            }
        }
        assert_eq!(
            shown(
                "Mrs Penrow (C1) called C1.",
                &[("Mrs Penrow", "person"), ("C1", "person")]
            ),
            pairs(&[("Penrow", "PERSON")])
        );
        assert_eq!(
            shown(
                "Mrs Penrow (C1) called C1.",
                &[("Mrs Penrow (C1)", "person")]
            ),
            pairs(&[("Penrow", "PERSON")])
        );
        assert_eq!(
            shown(
                "Pellam Ltd (“D1”) sued D1.",
                &[("Pellam Ltd (“D1”)", "organization")]
            ),
            pairs(&[("Pellam Ltd", "ORG")])
        );
    }

    /// A role before a company name must not suppress its trading name.
    #[test]
    fn party_code_company_continuations_do_not_establish_a_series() {
        for (source, company) in [
            (
                "Claimant C1 Holdings Ltd, trading as C1, paid the invoice.",
                "C1 Holdings Ltd",
            ),
            ("Claimant C1 Holdings paid C1.", "C1 Holdings"),
            ("Claimant C1 Holdings2 paid C1.", "C1 Holdings2"),
            ("Claimant C1 trading as C1 paid the invoice.", "C1"),
            (
                "Claimant C1 international holdings ltd paid C1.",
                "C1 international holdings ltd",
            ),
            ("Claimant C1 of North Quay paid C1.", "C1 of North Quay"),
            ("Claimant C1 t/a C1 paid the invoice.", "C1"),
            (
                "Claimant C1 (international holdings) Ltd, trading as C1, paid the invoice.",
                "C1 (international holdings) Ltd",
            ),
        ] {
            // Use the later occurrence for the trading/short name: the
            // first C1 is inside the company's full name.
            let raw: Vec<_> = [
                (source.find(company).unwrap(), company),
                (source.rfind("C1").unwrap(), "C1"),
            ]
            .into_iter()
            .map(|(start, text)| crate::RawEntity {
                start,
                end: start + text.len(),
                label: "organization".into(),
                score: 0.8,
                text: text.into(),
            })
            .collect();
            let actual: Vec<_> = crate::recognition::proposals(source, &raw)
                .unwrap()
                .into_iter()
                .map(|span| (source[span.range].to_owned(), span.category))
                .collect();
            assert_eq!(
                actual,
                pairs(&[(company, "ORG"), ("C1", "ORG")]),
                "{source}"
            );
        }
        // A company wrapped onto the next line, or with a space before its
        // legal-form comma, still opens with its code: the code marks stay
        // with only the code or short name marked by the model.
        for source in [
            "Claimant C1\nHoldings Ltd, trading as C1, paid.",
            "Claimant C1\r\nHoldings Ltd, trading as C1, paid.",
            "Claimant C1\nLogistics Ltd, trading as C1, paid.",
            "Claimant C1\nInternational Holdings Ltd, trading as C1, paid.",
            "Claimant C1\nholdings ltd paid C1.",
            "Claimant C1\nLtd, trading as C1, paid.",
            "Claimant C1 , Inc, trading as C1, paid.",
            "Claimant C1 , LLC, trading as C1, paid.",
            "Claimant C1, Inc, trading as C1, paid.",
        ] {
            for at in [source.find("C1").unwrap(), source.rfind("C1").unwrap()] {
                let raw = [crate::RawEntity {
                    start: at,
                    end: at + 2,
                    label: "organization".into(),
                    score: 0.8,
                    text: "C1".into(),
                }];
                let actual: Vec<_> = crate::recognition::proposals(source, &raw)
                    .unwrap()
                    .into_iter()
                    .map(|span| source[span.range].to_owned())
                    .collect();
                assert!(actual.iter().any(|text| text == "C1"), "{source} at {at}");
            }
        }
        // A heading layout still defines its codes.
        assert_eq!(
            shown(
                "First Claimant\nC1\nSecond Claimant\nC2\nC1 lost.",
                &[("C1", "organization")]
            ),
            []
        );
        // A role and code before a lowercase clause still define a code.
        assert_eq!(
            shown(
                "Claimant C1 and Defendant D1 called C1.",
                &[("C1", "organization")]
            ),
            []
        );
    }

    #[test]
    fn party_code_definition_removal_consumes_quotes_and_inner_spaces() {
        for bracket in [
            "('C1')",
            "( C1 )",
            "( 'C1' )",
            "(‘C1’)",
            "( ‘C1’ )",
            "(\"C1\")",
            "( \"C1\" )",
            "(“C1”)",
            "( “C1” )",
            "('C1', as appropriate)",
            "( ‘C1’, as appropriate )",
            "(C1, as appropriate)",
            "( C1, as appropriate )",
            "(\t'C1'\t)",
            "(\u{a0}“C1”\u{a0})",
        ] {
            let definition = format!("Mrs Penrow {bracket}");
            let source = format!("{definition} called C1.");
            let expected = pairs(&[("Penrow", "PERSON")]);
            assert_eq!(
                shown(&source, &[(&definition, "person")]),
                expected,
                "{source}"
            );
            for label in ["person", "organization"] {
                assert_eq!(
                    shown(&source, &[("Mrs Penrow", "person"), (bracket, label)]),
                    expected,
                    "{source}: {label}"
                );
            }
            let company = format!("Pellam Ltd {bracket}");
            let source = format!("{company} called C1.");
            assert_eq!(
                shown(&source, &[(&company, "organization")]),
                pairs(&[("Pellam Ltd", "ORG")]),
                "{source}"
            );
        }
    }

    /// Uncertain labels, short real names, other code series and real
    /// identifiers stay: the cost of losing a real value is higher.
    #[test]
    fn unexplained_codes_and_names_beside_party_codes_stay() {
        for source in [
            "C1 met D12 and R2.",
            "The claimant is C1 Ltd.",
            "Penrow (C100) called C1.",
            "The product C1 arrived.",
            "Claimant C1 Ltd called C1.",
            "Defendant C1, Inc called C1.",
        ] {
            assert!(
                shown(source, &[("C1", "organization")])
                    .iter()
                    .any(|(text, _)| text == "C1" || text == "C1 Ltd"),
                "{source}"
            );
        }
        assert_eq!(
            shown("Claimant C1 called C1.", &[("C1", "organization")]),
            []
        );
        let source = "Mrs Penrow (C1). D1 met R2; C100 and C1A attended with C1 Penrow. \
            Staff ID: C123. C1's trustee Ivo Penrow replied.\nМ1. О1.";
        assert_eq!(
            shown(
                source,
                &[
                    ("D1", "person"),
                    ("R2", "organization"),
                    ("C100", "organization"),
                    ("C1A", "organization"),
                    ("C1 Penrow", "person"),
                    ("C1's trustee Ivo Penrow", "person"),
                    ("М1", "person"),
                    ("О1", "person")
                ]
            ),
            pairs(&[
                ("D1", "PERSON"),
                ("R2", "ORG"),
                ("C100", "ORG"),
                ("C1A", "ORG"),
                ("C1 Penrow", "PERSON"),
                ("C123", "IDENTITY"),
                ("C1's trustee Ivo Penrow", "PERSON"),
                ("М1", "PERSON"),
                ("О1", "PERSON")
            ])
        );
        // A range with a series that was never established is uncertain.
        assert_eq!(
            shown("Claimant C1: C3 – R5.", &[("C3 – R5", "organization")]),
            pairs(&[("C3 – R5", "ORG")])
        );
    }

    #[test]
    fn list_letters_come_off_names_only_at_list_breaks() {
        let source = "a. Penrow represents and warrants; b. Brannock will attend; \
            d. Quillon Media Inc agrees to defend.\n(c) Ostrand replied.\ni. Larkfield replied.";
        assert_eq!(
            shown(
                source,
                &[
                    ("a. Penrow", "person"),
                    ("b. Brannock", "person"),
                    ("d. Quillon Media Inc", "organization"),
                    ("(c) Ostrand", "person"),
                    ("i. Larkfield", "person")
                ]
            ),
            pairs(&[
                ("Penrow", "PERSON"),
                ("Brannock", "PERSON"),
                ("Quillon Media Inc", "ORG"),
                ("Ostrand", "PERSON"),
                ("Larkfield", "PERSON")
            ])
        );
        assert_eq!(
            shown(
                "A. Penrow met J. R. Ostrand and van Hoorn.",
                &[
                    ("A. Penrow", "person"),
                    ("J. R. Ostrand", "person"),
                    ("van Hoorn", "person")
                ]
            ),
            pairs(&[
                ("A. Penrow", "PERSON"),
                ("J. R. Ostrand", "PERSON"),
                ("van Hoorn", "PERSON")
            ])
        );
        assert_eq!(
            fixed(
                "Met a. Penrow yesterday.",
                &[("a. Penrow", Category::Person)]
            ),
            ["a. Penrow"]
        );
        assert_eq!(
            fixed("  a. Penrow  ", &[("  a. Penrow  ", Category::Person)]),
            ["Penrow"]
        );
        for ending in [".", "!", "?", "…"] {
            let source = format!("The deed was signed{ending} j. Penrow witnessed it.");
            assert_eq!(
                shown(&source, &[("j. Penrow", "person")]),
                pairs(&[("j. Penrow", "PERSON")]),
                "{source}"
            );
        }
        for marker in ["ii.", "(iv)", "a)", "viii."] {
            for before in ["", "Attending: ", "First; ", "Signed.\n"] {
                for (name, label, category) in [
                    ("Penrow", "person", "PERSON"),
                    ("Quillon Media Inc", "organization", "ORG"),
                ] {
                    let span = format!("{marker} {name}");
                    let source = format!("{before}{span} replied.");
                    assert_eq!(
                        shown(&source, &[(&span, label)]),
                        pairs(&[(name, category)])
                    );
                }
            }
        }
    }

    /// Review #599: a defined series must not hide a real name or an
    /// unexplained standalone code that may itself name an organization.
    #[test]
    fn established_series_keep_names_and_unexplained_members() {
        assert_eq!(
            shown(
                "Pellam Rooke Ltd (D1). The goods went to D3 Logistics Ltd.",
                &[("Pellam Rooke Ltd", "organization"), ("D3", "organization")]
            ),
            pairs(&[("Pellam Rooke Ltd", "ORG"), ("D3", "ORG")])
        );
        let source = "Mrs Penrow (C2). Claimant C1 Holdings Ltd, trading as C1.";
        let raw: Vec<_> = [
            (source.find("Penrow").unwrap(), "Penrow", "person"),
            (
                source.find("C1 Holdings Ltd").unwrap(),
                "C1 Holdings Ltd",
                "organization",
            ),
            (source.rfind("C1").unwrap(), "C1", "organization"),
        ]
        .into_iter()
        .map(|(start, text, label)| crate::RawEntity {
            start,
            end: start + text.len(),
            label: label.into(),
            score: 0.8,
            text: text.into(),
        })
        .collect();
        let actual: Vec<_> = crate::recognition::proposals(source, &raw)
            .unwrap()
            .into_iter()
            .map(|span| (source[span.range].to_owned(), span.category))
            .collect();
        assert_eq!(
            actual,
            pairs(&[
                ("Penrow", "PERSON"),
                ("C1 Holdings Ltd", "ORG"),
                ("C1", "ORG")
            ])
        );
        assert_eq!(
            shown(
                "Mrs Penrow (C1) watched C4.",
                &[("Mrs Penrow", "person"), ("C4", "organization")]
            ),
            pairs(&[("Penrow", "PERSON"), ("C4", "ORG")])
        );
        // Even the very same code can also open a name. Keep both its
        // full form and later short mentions when their meanings collide.
        for (source, name, label) in [
            (
                "Mrs Penrow (C1). C1 Holdings Ltd, trading as C1.",
                "C1 Holdings Ltd",
                "organization",
            ),
            (
                "Mrs Brannock (C1). C1 Penrow replied for C1.",
                "C1 Penrow",
                "person",
            ),
        ] {
            let raw: Vec<_> = [
                (source.find(name).unwrap(), name),
                (source.rfind("C1").unwrap(), "C1"),
            ]
            .into_iter()
            .map(|(start, text)| crate::RawEntity {
                start,
                end: start + text.len(),
                label: label.into(),
                score: 0.8,
                text: text.into(),
            })
            .collect();
            let actual = crate::recognition::proposals(source, &raw).unwrap();
            for entity in raw {
                assert!(
                    actual
                        .iter()
                        .any(|span| span.range == (entity.start..entity.end)),
                    "{source}: {}",
                    entity.text
                );
            }
        }
        // The model may mark only the code at the start of a company,
        // even when that exact code has been defined for another party.
        for name in ["D3 Logistics Ltd", "D3 – Logistics Ltd"] {
            let source = format!("Pellam Rooke Ltd (D3). {name} received the goods.");
            let start = source.rfind("D3").unwrap();
            let raw = [crate::RawEntity {
                start,
                end: start + 2,
                label: "organization".into(),
                score: 0.8,
                text: "D3".into(),
            }];
            assert!(
                crate::recognition::proposals(&source, &raw)
                    .unwrap()
                    .iter()
                    .any(|span| span.range == (start..start + 2)),
                "{source}"
            );
        }
    }

    #[test]
    fn party_code_evidence_stops_at_clauses_and_ignores_titles() {
        for source in [
            "Claimant C1, Mrs Penrow, sued. C1 lost.",
            "First Claimant\nC1\nSecond Claimant\nC2",
            "Claimant C1 Mrs Penrow sued. C1 lost.",
        ] {
            assert!(
                shown(source, &[("C1", "organization")]).is_empty(),
                "{source}"
            );
        }
        assert_eq!(
            shown(
                "Claimant C1, Mrs Penrow, sued. C1 lost.",
                &[("Mrs Penrow", "person"), ("C1", "organization")]
            ),
            pairs(&[("Penrow", "PERSON")])
        );
        assert_eq!(
            shown(
                "Pellam Rooke plc (D1). D1 lost.",
                &[("Pellam Rooke plc", "organization"), ("D1", "organization")]
            ),
            pairs(&[("Pellam Rooke plc", "ORG")])
        );
    }

    #[test]
    fn established_code_pairs_and_lists_go_only_as_whole_codes() {
        for list in ["C1 and C2", "D1, D2 and D3", "D1, D2, and D3", "C1 and D2"] {
            let source = format!("Mrs Penrow (C1). Pellam Ltd (D1). {list} replied.");
            for label in ["person", "organization"] {
                assert_eq!(
                    shown(
                        &source,
                        &[
                            ("Mrs Penrow", "person"),
                            ("Pellam Ltd", "organization"),
                            (list, label)
                        ]
                    ),
                    pairs(&[("Penrow", "PERSON"), ("Pellam Ltd", "ORG")]),
                    "{source}: {label}"
                );
            }
        }
        for list in [
            "C1 and R2",
            "C1 and C2 Penrow",
            "D1, D2 and D3 Logistics Ltd",
            "C1 and Ivo Penrow",
        ] {
            let source = format!("Mrs Penrow (C1). Pellam Ltd (D1). {list} replied.");
            assert!(
                shown(&source, &[(list, "organization")])
                    .iter()
                    .any(|(text, _)| text == list),
                "{source}"
            );
        }
    }

    #[test]
    fn party_codes_use_the_decoded_document_and_keep_other_categories() {
        let source = "Mrs Penrow (&ldquo;C1&rdquo;).\n(a) C1’s trustees; C3—C5; staff C1.";
        assert_eq!(
            shown(
                source,
                &[("(a) C1’s trustees", "organization"), ("C3—C5", "person")]
            ),
            []
        );
        let raw = [crate::RawEntity {
            start: source.rfind("C1").unwrap(),
            end: source.rfind("C1").unwrap() + 2,
            label: "national_id_number".into(),
            score: 0.8,
            text: "C1".into(),
        }];
        assert!(
            crate::correct(source, &raw)
                .iter()
                .any(|d| d.category == Category::Identity)
        );
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

    /// Close variants of the English citations #511 covers are no tax,
    /// bank or identity values either: codes without spaces or dots, state
    /// statutes, the Civil Procedure Rules by rule or part, agencies'
    /// letters and rulings, a running header's entry and page, and years
    /// (guide rules 4 and 8, issue #480).
    #[test]
    fn english_citation_variants_are_never_proposed() {
        let source = "Under 19CFR141.37, 42 USC Section 1981, 42 U.S.C. Sec. 1983, NRS 41.1395, \
            N.R.S. § 41.085, 54 FR 35305, 49FR17754, CPR r71.8, CPR r.38.4, CPR r 71.5, CPR \
            Part 35 and the CPR, Part 19.12 (Release Number 34-52926; FDIC FIL-111-2006; OTS \
            No. 2006-49; CEQ # 20060476; T.D. 84-93; Rev. Rul. 2031-12; PLR 203112001; Fed. R. \
            Civ. P. 12(b)(6)). In Reply Refer To: AESO/SE 22410-2005-F-0506.\n\
            15-49008-wsd Doc 19-1 Filed 06/12/15\nTXSD Page 7 of 8\n\
            For the years 2013 through 2016 and in March 2023 nothing changed.";
        let raw = [
            ("19CFR141.37", "tax_id"),
            ("42 USC Section 1981", "tax_id"),
            ("1983", "bank_account"),
            ("NRS 41.1395", "tax_id"),
            ("41.085", "tax_id"),
            ("54 FR 35305", "tax_id"),
            ("49FR17754", "tax_id"),
            ("CPR r71.8", "national_id_number"),
            ("r.38.4", "tax_id"),
            ("CPR r 71.5", "national_id_number"),
            ("CPR Part 35", "passport_number"),
            ("Part 19.12", "tax_id"),
            ("Release Number 34-52926", "bank_account"),
            ("FIL-111-2006", "tax_id"),
            ("OTS No. 2006-49", "tax_id"),
            ("CEQ # 20060476", "tax_id"),
            ("T.D. 84-93", "tax_id"),
            ("Rev. Rul. 2031-12", "tax_id"),
            ("203112001", "tax_id"),
            ("12(b)(6)", "tax_id"),
            ("AESO/SE 22410-2005-F-0506", "organization"),
            ("15-49008-wsd", "tax_id"),
            ("Doc 19-1", "tax_id"),
            ("TXSD Page 7 of 8", "tax_id"),
            ("2013 through 2016", "tax_id"),
            ("2023", "tax_id"),
        ];
        let failures: Vec<String> = raw
            .iter()
            .filter_map(|span| {
                let actual = shown(source, &[*span]);
                (!actual.is_empty()).then(|| format!("{span:?}: {actual:?}"))
            })
            .collect();
        assert!(failures.is_empty(), "{}", failures.join("\n"));
        // A year alone is no tax number; a real one stays.
        for year in ["2023", " 2013 through 2016 ", "2029 and 2030"] {
            assert!(!pack().identifying(year, Category::Tax), "{year:?}");
        }
        for value in ["98-7654321", "123456789", "123-45-6789", "20231"] {
            assert!(pack().identifying(value, Category::Tax), "{value:?}");
        }
    }

    /// A docket number is the case at hand's after "No." (CASE, with its
    /// repeats), a cited case's inside its citation, and never a tax, bank
    /// or identity value (guide rule 5, #506 item 3, issue #480).
    #[test]
    fn docket_numbers_are_case_numbers_or_nothing() {
        let source = "Ivo Harrow v. Quillon Freight LLC\nNo. 09-31-00999-CV\n\n\
            See Pellam v. Ostrand, No. 09-30-00111-CV (Tex. App. 2030), and 1:31-cv-00999-ZZ \
            (see 02 CV 3651). Index No. 06 CV 15490 is the related action. 31-99999-zzz Doc 7 \
            Filed 06/12/31.\n09-31-00999-CV Page 2 of 3\n";
        let raw = [
            ("Ivo Harrow", "person"),
            ("Quillon Freight LLC", "organization"),
            ("09-31-00999-CV", "tax_id"),
            ("09-30-00111-CV", "tax_id"),
            ("1:31-cv-00999-ZZ", "tax_id"),
            ("02 CV 3651", "bank_account"),
            ("06 CV 15490", "tax_id"),
            ("31-99999-zzz", "national_id_number"),
        ];
        assert_eq!(
            shown(source, &raw),
            pairs(&[
                ("Ivo Harrow", "PERSON"),
                ("Quillon Freight LLC", "ORG"),
                ("09-31-00999-CV", "CASE"),
                ("06 CV 15490", "CASE"),
                ("09-31-00999-CV", "CASE"),
            ])
        );
        // A bankruptcy header's case number is CASE where a label names it,
        // without the entry beside it.
        let source = "Case: 15-49008-wsd\n15-49008-wsd Doc 19-1 Filed 06/12/15\n";
        assert_eq!(
            shown(source, &[("15-49008-wsd Doc 19-1", "tax_id")]),
            pairs(&[("15-49008-wsd", "CASE"), ("15-49008-wsd", "CASE")])
        );
        for (value, category) in [
            ("01-24-00779-CV", Category::Tax),
            ("1:21-cv-00616-RP", Category::Tax),
            ("02 CV 3651", Category::Bank),
            ("15-49008-wsd Doc 19-1", Category::Tax),
            ("01-24-00779-CV", Category::Identity),
        ] {
            assert!(!pack().identifying(value, category), "{value:?}");
        }
    }

    /// Beside each English citation a real value and a party stay: a tax
    /// number, an account, a phone, a passport, a company and a person
    /// (issue #480).
    #[test]
    fn identifiers_beside_citation_variants_stay() {
        let source = "Under 19CFR141.37 Quillon Freight LLC (EIN 98-7654321) paid from account \
            number 12345678; NRS 41.1395 binds Ivo Harrow, tel. +1 (202) 555-0164, passport \
            123456789 (CPR r71.8); Release Number 34-52926, SSN 123-45-6789; for the years \
            2013 through 2016 (TXSD Page 7 of 8) Rowan Pell paid.";
        let raw = [
            ("Quillon Freight LLC", "organization"),
            ("98-7654321", "tax_id"),
            ("12345678", "bank_account"),
            ("Ivo Harrow", "person"),
            ("+1 (202) 555-0164", "phone_number"),
            ("123456789", "passport_number"),
            ("123-45-6789", "tax_id"),
            ("Rowan Pell", "person"),
        ];
        assert_eq!(
            shown(source, &raw),
            pairs(&[
                ("Quillon Freight LLC", "ORG"),
                ("98-7654321", "TAX"),
                ("12345678", "BANK"),
                ("Ivo Harrow", "PERSON"),
                ("+1 (202) 555-0164", "PHONE"),
                ("123456789", "IDENTITY"),
                ("123-45-6789", "TAX"),
                ("Rowan Pell", "PERSON"),
            ])
        );
    }

    /// Close variants of the Russian references #511 covers: a law's number
    /// spaced, reversed or in lower case, an article of a code by its full
    /// name, a municipal act after a quoted name, a letter's number whole or
    /// in pieces, a transfer document with its word, a trading protocol's
    /// number and a date's month (guide rules 4 and 8, issue #480).
    #[test]
    fn russian_reference_variants_are_never_proposed() {
        let source = "Постановление Администрации Энского района «Кедровый» от 25.03.2031 № \
            833-ПА, по УПД № 137 от 06.06.2031, протоколу об определении участников торгов \
            144355-МЭТС/1, Письмо Минфина России от 30.04.2031 N 52н, письмо № \
            02-09-11/05-07-31116 и № 06-18/27628, Закон РФ «О защите прав потребителей» от \
            07.02.1992 № 2300-1, Положение Банка России от 29.06.2031 N 762-П, ст. 428 \
            Гражданского кодекса РФ, ст. 12.8 Кодекса Российской Федерации об \
            административных правонарушениях, Федерального закона № 999 - ФЗ, ФЗ-127 и 44-фз.";
        let raw = [
            ("03", "bank_account"),
            ("833-ПА", "national_id_number"),
            ("УПД № 137", "tax_id"),
            ("144355-МЭТС/1", "tax_id"),
            ("52н", "tax_id"),
            ("05-07-31116", "bank_account"),
            ("06-18/27628", "tax_id"),
            ("2300-1", "tax_id"),
            ("762-П", "national_id_number"),
            ("428 Гражданского кодекса РФ", "tax_id"),
            ("12.8 Кодекса Российской Федерации", "tax_id"),
            ("999 - ФЗ", "tax_id"),
            ("ФЗ-127", "tax_id"),
            ("44-фз", "tax_id"),
        ];
        let failures: Vec<String> = raw
            .iter()
            .filter_map(|span| {
                let actual = shown(source, &[*span]);
                (!actual.is_empty()).then(|| format!("{span:?}: {actual:?}"))
            })
            .collect();
        assert!(failures.is_empty(), "{}", failures.join("\n"));
    }

    /// Beside each Russian reference real values, a party and the case's
    /// own number stay; a page number comes off the case number's repeat,
    /// and a court case's number after "№" is no letter's (issue #480).
    #[test]
    fn identifiers_beside_russian_reference_variants_stay() {
        let source = "Дело № А28-3207/2031. Постановление Администрации Энского района \
            «Кедровый» от 25.03.2031 № 833-ПА получила Лидия Брюн, ИНН 7701234567, КПП \
            770101001, ОГРН 1027700132195, р/с 40702810900000012345 от 30.03.2031, паспорт \
            4512 345678, тел. +7 (495) 123-45-67; по УПД № 137 ООО «Кедровый бор».\n\
            3 А28-3207/2031\n";
        let raw = [
            ("Лидия Брюн", "person"),
            ("7701234567", "tax_id"),
            ("770101001", "tax_id"),
            ("1027700132195", "tax_id"),
            ("40702810900000012345", "bank_account"),
            ("4512 345678", "passport_number"),
            ("+7 (495) 123-45-67", "phone_number"),
            ("ООО «Кедровый бор»", "organization"),
            ("3 А28-3207/2031", "tax_id"),
        ];
        assert_eq!(
            shown(source, &raw),
            pairs(&[
                ("А28-3207/2031", "CASE"),
                ("Лидия Брюн", "PERSON"),
                ("7701234567", "TAX"),
                ("770101001", "TAX"),
                // ОГРН is TAX like ИНН (#484 item 6, #563).
                ("1027700132195", "TAX"),
                ("40702810900000012345", "BANK"),
                ("4512 345678", "IDENTITY"),
                ("+7 (495) 123-45-67", "PHONE"),
                ("ООО «Кедровый бор»", "ORG"),
                ("А28-3207/2031", "CASE"),
            ])
        );
        // A court case's number after "№" is no letter's: the model's span
        // stays, as do a dotted account and an appellate number.
        let source = "Решение суда № 2-1842/2031, счёт 40817.810.12.03.2031, № 09АП-1234/2031";
        assert_eq!(
            shown(
                source,
                &[
                    ("2-1842/2031", "national_id_number"),
                    ("40817.810.12.03.2031", "bank_account"),
                    ("09АП-1234/2031", "tax_id"),
                ]
            ),
            pairs(&[
                ("2-1842/2031", "IDENTITY"),
                ("40817.810.12.03.2031", "BANK"),
                ("09АП-1234/2031", "TAX"),
            ])
        );
    }

    /// A source, the model's spans in it (each wording's `n`th mention)
    /// and what the review should show.
    type Shown<'a> = (
        &'a str,
        &'a [(&'a str, usize, &'a str)],
        &'a [(&'a str, &'a str)],
    );

    /// Each case's proposals against what it should show; the failures,
    /// one line each.
    fn failures(cases: &[Shown<'_>]) -> Vec<String> {
        cases
            .iter()
            .filter_map(|(source, raw, expected)| {
                let actual = shown_at(source, raw);
                (actual != pairs(expected))
                    .then(|| format!("{source:?}: {actual:?}, not {expected:?}"))
            })
            .collect()
    }

    /// A court's docket number the model takes for a tax, bank or identity
    /// value is a case number under any label, in a list or alone, and
    /// never goes unmarked (guide rule 5, #480 review).
    #[test]
    fn docket_numbers_under_any_label_stay_case_numbers() {
        let cases: &[Shown<'_>] = &[
            (
                "UNITED STATES DISTRICT COURT\nIvo Harrow v. Quillon Freight LLC\nDocket: \
                1:21-cv-00616-RP\n",
                &[("1:21-cv-00616-RP", 0, "tax_id")],
                &[("1:21-cv-00616-RP", "CASE")],
            ),
            (
                "Docket Number: 1:21-cv-00616-RP\n",
                &[("1:21-cv-00616-RP", 0, "bank_account")],
                &[("1:21-cv-00616-RP", "CASE")],
            ),
            (
                "NUMBER 13-21-00234-CV\n\nCOURT OF APPEALS\nIvo Harrow, Appellant, v. Quillon \
                Freight LLC, Appellee.\n",
                &[("13-21-00234-CV", 0, "tax_id")],
                &[("13-21-00234-CV", "CASE")],
            ),
            (
                "NUMBER 13-21-00234-CV\n\nIvo Harrow, Appellant, v. Quillon Freight LLC, \
                Appellee.\n\nWe affirm in 13-21-00234-CV.\n",
                &[
                    ("13-21-00234-CV", 0, "tax_id"),
                    ("13-21-00234-CV", 1, "tax_id"),
                ],
                &[("13-21-00234-CV", "CASE"), ("13-21-00234-CV", "CASE")],
            ),
            (
                "NUMBERS 13-21-00234-CV & 13-21-00235-CV\n\nCOURT OF APPEALS\n",
                &[
                    ("13-21-00234-CV", 0, "tax_id"),
                    ("13-21-00235-CV", 0, "tax_id"),
                ],
                &[("13-21-00234-CV", "CASE"), ("13-21-00235-CV", "CASE")],
            ),
            (
                "NOS. 01-24-00779-CV, 01-24-00780-CV\n",
                &[
                    ("01-24-00779-CV", 0, "tax_id"),
                    ("01-24-00780-CV", 0, "tax_id"),
                ],
                &[("01-24-00779-CV", "CASE"), ("01-24-00780-CV", "CASE")],
            ),
            (
                "Nos. 01-24-00779-CV & 01-24-00780-CV\n",
                &[
                    ("01-24-00779-CV", 0, "tax_id"),
                    ("01-24-00780-CV", 0, "tax_id"),
                ],
                &[("01-24-00779-CV", "CASE"), ("01-24-00780-CV", "CASE")],
            ),
            (
                "Case Nos. 1:21-cv-00616-RP, 1:21-cv-00617-RP\n",
                &[
                    ("1:21-cv-00616-RP", 0, "tax_id"),
                    ("1:21-cv-00617-RP", 0, "tax_id"),
                ],
                &[("1:21-cv-00616-RP", "CASE"), ("1:21-cv-00617-RP", "CASE")],
            ),
            (
                "Cause Number 01-24-00779-CV\nIvo Harrow v. Quillon Freight LLC\n",
                &[("01-24-00779-CV", 0, "national_id_number")],
                &[("01-24-00779-CV", "CASE")],
            ),
            (
                "Civil Action 1:21-cv-00616-RP\nIvo Harrow v. Quillon Freight LLC\n",
                &[("1:21-cv-00616-RP", 0, "tax_id")],
                &[("1:21-cv-00616-RP", "CASE")],
            ),
            (
                "Index # 06 CV 15490\nIvo Harrow v. Quillon Freight LLC\n",
                &[("06 CV 15490", 0, "tax_id")],
                &[("06 CV 15490", "CASE")],
            ),
            (
                "In re Ivo Harrow, Debtor.\nBankruptcy No. 15-49008-wsd\nChapter 7\n",
                &[("15-49008-wsd", 0, "tax_id")],
                &[("15-49008-wsd", "CASE")],
            ),
            (
                "Ivo Harrow, Plaintiff, v. Quillon Freight LLC, Defendant.\nAdv. No. \
                15-04123-wsd\n",
                &[("15-04123-wsd", 0, "tax_id")],
                &[("15-04123-wsd", "CASE")],
            ),
            (
                "Adv. Pro. No. 31-01992-wsd\n",
                &[("31-01992-wsd", 0, "tax_id")],
                &[("31-01992-wsd", "CASE")],
            ),
            (
                "In re Ivo Harrow, Debtor. Chapter 7, Bankruptcy 15-49008-wsd\n",
                &[("15-49008-wsd", 0, "tax_id")],
                &[("15-49008-wsd", "CASE")],
            ),
            (
                "No. 01-24-00779-CR\nThe State of Texas v. Ivo Harrow\n",
                &[("01-24-00779-CR", 0, "tax_id")],
                &[("01-24-00779-CR", "CASE")],
            ),
            // "No." inside the model's span stays outside the case number.
            (
                "NO. 14-31-00999-CV\n",
                &[("NO. 14-31-00999-CV", 0, "tax_id")],
                &[("14-31-00999-CV", "CASE")],
            ),
        ];
        let failed = failures(cases);
        assert!(failed.is_empty(), "{}", failed.join("\n"));
        // Russian: a case number under another label or none is CASE too,
        // its page number left out.
        let cases: &[Shown<'_>] = &[
            (
                "Апелляционное производство № 33-1234/2031\nСудья Брюн Л.\n",
                &[("33-1234/2031", 0, "tax_id")],
                &[("33-1234/2031", "CASE")],
            ),
            (
                "Материал № 13-123/2031\n",
                &[("13-123/2031", 0, "tax_id")],
                &[("13-123/2031", "CASE")],
            ),
            (
                "№ 2-1842/2031\nРЕШЕНИЕ\nИменем Российской Федерации\n",
                &[("2-1842/2031", 0, "tax_id")],
                &[("2-1842/2031", "CASE")],
            ),
            (
                "Апелляционное производство № 33-1234/2031\nДело № 2-1842/2031\n",
                &[("33-1234/2031", 0, "tax_id"), ("2-1842/2031", 0, "tax_id")],
                &[("33-1234/2031", "CASE"), ("2-1842/2031", "CASE")],
            ),
            (
                "Определение Арбитражного суда Кировской области от 01.03.2031 № \
                А28-3207/2031\n",
                &[("А28-3207/2031", 0, "tax_id")],
                &[("А28-3207/2031", "CASE")],
            ),
            (
                "3 А28-3207/2031\n",
                &[("3 А28-3207/2031", 0, "tax_id")],
                &[("А28-3207/2031", "CASE")],
            ),
        ];
        let failed = failures(cases);
        assert!(failed.is_empty(), "{}", failed.join("\n"));
    }

    /// A Russian act's, letter's or law's lead before "№" takes no account,
    /// БИК, card, passport or tax number after an author's word, and no
    /// surname that starts like "Закон"; an arbitration court's case number
    /// after an act stays the model's (#480 review).
    #[test]
    fn values_after_an_act_lead_stay() {
        let cases: &[Shown<'_>] = &[
            (
                "Письмо ООО «Ромашка» Счёт N 40702810900000012345\n",
                &[("40702810900000012345", 0, "bank_account")],
                &[("40702810900000012345", "BANK")],
            ),
            (
                "Письмо ПАО Сбербанк БИК № 044525225\n",
                &[("044525225", 0, "bank_account")],
                &[("044525225", "BANK")],
            ),
            (
                "Письмо Брюн Л. Карта № 4276123456789012\n",
                &[("4276123456789012", 0, "bank_account")],
                &[("4276123456789012", "BANK")],
            ),
            (
                "Положение Сотрудника Брюн Л. № 4276123456789012\n",
                &[("4276123456789012", 0, "bank_account")],
                &[("4276123456789012", "BANK")],
            ),
            (
                "Законов Иван Петрович N 4512345678, проживает в г. Кирове.\n",
                &[
                    ("Законов Иван Петрович", 0, "person"),
                    ("4512345678", 0, "passport_number"),
                ],
                &[
                    ("Законов Иван Петрович", "PERSON"),
                    ("4512345678", "IDENTITY"),
                ],
            ),
            (
                "Письмо Минфина России от 30.03.2031 № 7701234567\n",
                &[("7701234567", 0, "tax_id")],
                &[("7701234567", "TAX")],
            ),
            (
                "передаточному документу N 40702810900000012345\n",
                &[("40702810900000012345", 0, "bank_account")],
                &[("40702810900000012345", "BANK")],
            ),
            (
                "Определение Арбитражного суда Кировской области от 01.03.2031 № \
                А28-3207/2031\n",
                &[("А28-3207/2031", 0, "national_id_number")],
                &[("А28-3207/2031", "IDENTITY")],
            ),
        ];
        let failed = failures(cases);
        assert!(failed.is_empty(), "{}", failed.join("\n"));
        // The references themselves still go.
        let source = "Закон РФ от 07.02.1992 № 2300-1, Законом № 999-ФЗ, Письмо Минфина России \
            от 30.03.2031 № 02-09-11/05-03-99999, Определение Верховного Суда РФ от \
            12.03.2031 № 305-ЭС31-1234.";
        for (text, label) in [
            ("2300-1", "tax_id"),
            ("999-ФЗ", "tax_id"),
            ("02-09-11/05-03-99999", "bank_account"),
            ("305-ЭС31-1234", "tax_id"),
        ] {
            assert_eq!(shown(source, &[(text, label)]), Vec::new(), "{text:?}");
        }
        // A date's month after «от» is no account, its year short or not.
        for source in ["письмо от 30.03.15 N 52н.", "письмо от 30.03.2015 N 52н."]
        {
            assert_eq!(
                shown(source, &[("03", "bank_account")]),
                Vec::new(),
                "{source:?}"
            );
        }
    }

    /// A running header's page count, a Treasury Decision, a dotted date,
    /// the Federal Register and a header's entry number each take only
    /// their own shape: a party's name, a phone, a sort code, an address
    /// and an identity number beside a look-alike stay (#480 review).
    #[test]
    fn neighbours_of_citation_look_alikes_stay() {
        let cases: &[Shown<'_>] = &[
            (
                "DECLARATION OF HARROW Page 2 of 8\n",
                &[("HARROW", 0, "person")],
                &[("HARROW", "PERSON")],
            ),
            (
                "Prepared for ACME Page 2 of 8\n",
                &[("ACME", 0, "organization")],
                &[("ACME", "ORG")],
            ),
            (
                "Contacts: Smith, T.D. 555-0164; Jones, A.B. 555-0199.\n",
                &[
                    ("555-0164", 0, "phone_number"),
                    ("555-0199", 0, "phone_number"),
                ],
                &[("555-0164", "PHONE"), ("555-0199", "PHONE")],
            ),
            (
                "Sort code: 20.03.12\nAccount number: 12345678\n",
                &[
                    ("20.03.12", 0, "bank_account"),
                    ("12345678", 0, "bank_account"),
                ],
                &[("20.03.12", "BANK"), ("12345678", "BANK")],
            ),
            (
                "Pay to Ivo Harrow, sort code 20.03.12, account 12345678.\n",
                &[
                    ("20.03.12", 0, "bank_account"),
                    ("12345678", 0, "bank_account"),
                ],
                &[("20.03.12", "BANK"), ("12345678", "BANK")],
            ),
            (
                "Счёт 12.03.31 банк\n",
                &[("12.03.31", 0, "bank_account")],
                &[("12.03.31", "BANK")],
            ),
            (
                "Quillon Freight LLC, Unit 12 FR 75008 Paris\n",
                &[("75008", 0, "address")],
                &[("75008", "ADDRESS")],
            ),
            (
                "Identity Document 12345 Page 2 of the file.\n",
                &[("12345", 0, "national_id_number")],
                &[("12345", "IDENTITY")],
            ),
        ];
        let failed = failures(cases);
        assert!(failed.is_empty(), "{}", failed.join("\n"));
        // The look-alikes themselves still go.
        let source = "TXSD Page 7 of 8; T.D. 84-93; T.D. 9876; 54 FR 35305, 35310; 49FR17754 \
            at 2; Document 12 Filed 06/12/21; от 30.03.2015 N 52н.";
        for (text, label) in [
            ("TXSD Page 7 of 8", "tax_id"),
            ("84-93", "tax_id"),
            ("9876", "tax_id"),
            ("54 FR 35305", "tax_id"),
            ("49FR17754", "tax_id"),
            ("Document 12", "national_id_number"),
            ("03", "bank_account"),
        ] {
            assert_eq!(shown(source, &[(text, label)]), Vec::new(), "{text:?}");
        }
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

    /// Issue #499's follow-up probes: citation parts name nobody, while
    /// the declarant and people following a reporter keep their full names.
    #[test]
    fn citation_parts_follow_up_probes() {
        let mut failures = Vec::new();
        for (source, text, label) in [
            ("See 424 U.S. at 38.", "424 U.S. at 38", "address"),
            (
                "See 904 F.2d 275, 277.",
                "904 F.2d 275, 277",
                "phone_number",
            ),
            ("See 33 A.D.3d 352.", "A.D.", "person"),
            ("Under 42 U.S.C. § 1983.", "S.C.", "person"),
            ("Under I.R.C. § 61.", "I.R.C.", "person"),
            (
                "See n. 14; Tr. 112-14; 123sa.",
                "n. 14",
                "national_id_number",
            ),
            (
                "Under Chapter 7 of the Bankruptcy Code.",
                "Chapter 7",
                "organization",
            ),
            ("Chapter 11 Case", "Chapter 11", "organization"),
            ("R (on the application of Penrow)", "R", "person"),
            ("See at p. 478A.", "478A", "address"),
            ("See 1931 S.C. (H.L.) 13.", "S.C.", "person"),
            ("See 1931 S.C. (H.L.) 13.", "H.L", "person"),
            ("See [2031] All ER (Comm) 9.", "Comm", "organization"),
            (
                "Under National Instrument 45-999.",
                "National Instrument 45-999",
                "national_id_number",
            ),
            ("Under PD 57ZZ.", "PD 57ZZ", "organization"),
            (
                "See 352 DM - Aviation Safety.",
                "352 DM - Aviation Safety",
                "organization",
            ),
            (
                "See AHCP/CCAA p. S-19.",
                "AHCP/CCAA p. S-19",
                "organization",
            ),
            ("See Decl. of Ivo Penrow ¶ 4.", "Decl", "person"),
            ("See Decl. ¶ 4.", "Decl", "person"),
            ("See Dep. of Ivo Penrow at 4.", "Dep", "person"),
            ("See Aff. of Ivo Penrow ¶ 4.", "Aff", "person"),
            ("See Ex. 1 to Penrow Decl.", "Ex. 1", "person"),
            ("Under Tex. R. Civ. P. 91a.", "Tex. R. Civ", "organization"),
            ("Under Tex. R. App. P. 9.", "Tex. R. App", "organization"),
            ("Under Fed. R. Civ. P. 12.", "Fed. R. Civ", "organization"),
            (
                "See (Tex. App.—Slocombe 2031, pet. filed).",
                "Slocombe 2031",
                "address",
            ),
            ("See Section XII.", "Section XII", "organization"),
            (
                "As held in Pellam v. Ostrand, 999 S.W.3d 1 (Tex. App.—Slocombe 2031, pet. filed), No. 09-31-00888-CV.",
                "09-31-00888-CV",
                "national_id_number",
            ),
        ] {
            let actual = shown(source, &[(text, label)]);
            if !actual.is_empty() {
                failures.push(format!("{source:?}: {actual:?}"));
            }
        }
        for (source, text, expected) in [
            ("(Ostrand Decl. ¶ 3)", "Ostrand Decl", "Ostrand"),
            ("(Penrow Dep. at 4)", "Penrow Dep", "Penrow"),
            ("(Penrow Aff. ¶ 4)", "Penrow Aff", "Penrow"),
            (
                "[2031] 1 W.L.R. 12, Ivo Ostrand LJ",
                "Ivo Ostrand",
                "Ivo Ostrand",
            ),
            (
                "[2031] 1 W.L.R. Ivo Ostrand LJ",
                "Ivo Ostrand",
                "Ivo Ostrand",
            ),
        ] {
            let actual = shown(source, &[(text, "person")]);
            let wanted = pairs(&[(expected, "PERSON")]);
            if actual != wanted {
                failures.push(format!("{source:?}: {actual:?}, expected {wanted:?}"));
            }
        }
        assert!(failures.is_empty(), "{}", failures.join("\n"));
    }

    /// A capital S followed by a Roman-looking initial names a person,
    /// including when the model leaves the last period outside its span.
    #[test]
    fn citation_parts_keep_witness_initials() {
        let source = "The witness S. M. testified.";
        for name in ["S. M", "S. M."] {
            assert_eq!(
                shown(source, &[(name, "person")]),
                pairs(&[(name, "PERSON")])
            );
        }
        for reference in ["Section XII", "Sec. XII", "§ XII", "s. 12", "s. 12(a)"] {
            let source = format!("See {reference}.");
            assert!(
                shown(&source, &[(reference, "organization")]).is_empty(),
                "{source}"
            );
        }
    }

    /// A company's full name protects its short name throughout the
    /// document, including when it resembles a chapter reference.
    #[test]
    fn citation_parts_keep_established_company_short_names() {
        let source = "Chapter 11 Ltd supplied the books.\nChapter 11 refused repayment.";
        assert_eq!(
            shown_at(
                source,
                &[
                    ("Chapter 11 Ltd", 0, "organization"),
                    ("Chapter 11", 1, "organization")
                ]
            ),
            pairs(&[("Chapter 11 Ltd", "ORG"), ("Chapter 11", "ORG")])
        );
        for source in [
            source,
            "Chapter 11 refused repayment.\nChapter 11 Ltd supplied the books.",
            "Chapter 11 refused repayment.\nChapter 11 Books Ltd supplied the books.",
            "Under Chapter 11, repayment is due.\nCHAPTER 11 LTD supplied the books.",
            "# Parties\nUnder Chapter 11, repayment is due.\nChapter 11\nLtd supplied the books.",
        ] {
            // The full name is evidence even without a model span over it.
            let at = source.find("Chapter 11").unwrap();
            assert!(
                pack().identifying_at(
                    source,
                    at..at + "Chapter 11".len(),
                    "Chapter 11",
                    Category::Organization
                ),
                "{source}"
            );
        }
        for source in [
            "Under Chapter 11, repayment is due.",
            "SubChapter 11 Ltd supplied the books.\nUnder Chapter 11, repayment is due.",
            "Chapter 11 applies to the debtor Ltd.\nUnder Chapter 11, repayment is due.",
        ] {
            let at = source.rfind("Chapter 11").unwrap();
            assert!(
                !pack().identifying_at(
                    source,
                    at..at + "Chapter 11".len(),
                    "Chapter 11",
                    Category::Organization
                ),
                "{source}"
            );
        }
    }

    /// A comma before a paragraph or a page:line pinpoint supplies the
    /// citation context needed to remove an abbreviated document label.
    #[test]
    fn citation_parts_trim_document_labels_before_pinpoints() {
        for (source, name) in [
            ("(Penrow Decl., ¶ 3)", "Penrow Decl"),
            ("(Penrow Decl., ¶ 3)", "Penrow Decl."),
            ("(Penrow Dep. 12:4–13:2)", "Penrow Dep"),
            ("(Penrow Dep. 12:4–13:2)", "Penrow Dep."),
            ("(Penrow Dep., 12:4-13:2)", "Penrow Dep"),
            ("(Penrow Dep. 12:4-9)", "Penrow Dep"),
        ] {
            assert_eq!(
                shown(source, &[(name, "person")]),
                pairs(&[("Penrow", "PERSON")]),
                "{source}"
            );
        }
        for source in [
            "The witness (Penrow Decl., who testified).",
            "The witness (Penrow Dep. 12:4smith).",
            "The witness (Penrow Dep. 12).",
            "The witness (Penrow Dep. 12:4–).",
        ] {
            let name = if source.contains("Decl") {
                "Penrow Decl"
            } else {
                "Penrow Dep"
            };
            assert_eq!(
                shown(source, &[(name, "person")]),
                pairs(&[(name, "PERSON")]),
                "{source}"
            );
        }
    }

    /// Complete company spans are protected before reference rejection,
    /// and invalid Roman words never become section numbers (g1 finding 1).
    #[test]
    fn citation_parts_keep_complete_companies_and_reject_invalid_romans() {
        for (source, name) in [
            ("The supplier is Section LLC.", "Section LLC"),
            ("The books were supplied by Chapters LLC.", "Chapters LLC"),
            ("See Section LLC.", "Section LLC"),
            ("See Chapters LLC.", "Chapters LLC"),
            ("The supplier is Chapter LLC.", "Chapter LLC"),
        ] {
            assert_eq!(
                shown(source, &[(name, "organization")]),
                pairs(&[(name, "ORG")]),
                "{source}"
            );
        }
        // No ORG/legal-form protection participates in this check: the
        // Roman grammar itself must refuse LLC and other invalid words.
        for roman in ["LLC", "IIV", "VX", "IIII", "MMMM"] {
            let source = format!("See § {roman}.");
            assert!(
                !pack().in_reference(&source, 7..7 + roman.len()),
                "{source}"
            );
        }
        for roman in [
            "I",
            "IV",
            "IX",
            "XII",
            "XL",
            "XC",
            "CD",
            "CM",
            "MDCLXVI",
            "MMMCMXCIX",
        ] {
            let reference = format!("Section {roman}");
            let source = format!("See {reference}.");
            assert!(
                shown(&source, &[(&reference, "organization")]).is_empty(),
                "{source}"
            );
        }
    }

    /// Personal-ID labels win over reference shapes, and ambiguous bare
    /// PD/DM codes need affirmative legal context (g1 finding 2).
    #[test]
    fn citation_parts_keep_labelled_personal_identifiers() {
        for (source, value, label) in [
            ("Employee badge: PD 57ZZ.", "PD 57ZZ", "national_id_number"),
            ("Employee ID: 352 DM.", "352 DM", "national_id_number"),
            ("Employee ID: PD 57ZZ.", "PD 57ZZ", "national_id_number"),
            (
                "The witness presented passport PD 57ZZ.",
                "PD 57ZZ",
                "passport_number",
            ),
            ("Staff number: 352 DM.", "352 DM", "national_id_number"),
            (
                "Badge number: National Instrument 45-999.",
                "National Instrument 45-999",
                "national_id_number",
            ),
            (
                "Employee ID: PD 57ZZ-9842.",
                "PD 57ZZ-9842",
                "national_id_number",
            ),
            ("The value is PD 57ZZ.", "PD 57ZZ", "national_id_number"),
            ("The value is 352 DM.", "352 DM", "national_id_number"),
        ] {
            assert_eq!(
                shown(source, &[(value, label)]),
                pairs(&[(value, "IDENTITY")]),
                "{source}"
            );
        }
        for (source, value) in [
            ("Under PD 57ZZ.", "PD 57ZZ"),
            ("See 352 DM.", "352 DM"),
            ("See 352 DM - Aviation Safety.", "352 DM - Aviation Safety"),
            ("Practice Direction PD 57ZZ.", "PD 57ZZ"),
        ] {
            assert!(
                shown(source, &[(value, "national_id_number")]).is_empty(),
                "{source}"
            );
        }
    }

    /// Case, wrapping and an independently recognized trading name all
    /// protect a company's short name; uncertainty keeps it (g1 finding 3).
    #[test]
    fn citation_parts_keep_company_casing_wraps_and_trading_names() {
        for (source, full, short_n, expected_full) in [
            (
                "CHAPTER 11 LTD supplied the books. Chapter 11 refused a refund.",
                "CHAPTER 11 LTD",
                0,
                "CHAPTER 11 LTD",
            ),
            (
                "Chapter 11 Books supplied the books. Chapter 11 refused a refund.",
                "Chapter 11 Books",
                1,
                "Chapter 11 Books",
            ),
            (
                "Chapter 11\nLtd supplied the books. Chapter 11 refused a refund.",
                "Chapter 11\nLtd",
                1,
                "Chapter 11",
            ),
            (
                "Chapter 11 LLC supplied the books. Chapter 11 refused repayment.",
                "Chapter 11 LLC",
                1,
                "Chapter 11 LLC",
            ),
            // Affirmative reference cues still defer to independent
            // company evidence, including a later model-only trading name.
            (
                "CHAPTER 11 LTD supplied the books. See Chapter 11.",
                "CHAPTER 11 LTD",
                0,
                "CHAPTER 11 LTD",
            ),
            (
                "Chapter 11\nLtd supplied the books. See Chapter 11.",
                "Chapter 11\nLtd",
                1,
                "Chapter 11",
            ),
            (
                "Chapter 11 Books supplied the books. See Chapter 11.",
                "Chapter 11 Books",
                1,
                "Chapter 11 Books",
            ),
        ] {
            assert_eq!(
                shown_at(
                    source,
                    &[
                        (full, 0, "organization"),
                        ("Chapter 11", short_n, "organization")
                    ]
                ),
                pairs(&[(expected_full, "ORG"), ("Chapter 11", "ORG")]),
                "{source}"
            );
        }
        assert_eq!(
            shown(
                "The books were supplied by Chapter 11. Chapter 11 refused a refund.",
                &[("Chapter 11", "organization")]
            ),
            pairs(&[("Chapter 11", "ORG"), ("Chapter 11", "ORG")])
        );
        for source in [
            "The applicant trades as Section IV.",
            "Chapter 11.",
            "Chapter 11 governs repayment.",
        ] {
            let name = if source.contains("Section") {
                "Section IV"
            } else {
                "Chapter 11"
            };
            assert_eq!(
                shown(source, &[(name, "organization")]),
                pairs(&[(name, "ORG")]),
                "{source}"
            );
        }
        let source = "See Chapter 11. Chapter 11 Books supplied the books.";
        assert_eq!(
            shown_at(
                source,
                &[
                    ("Chapter 11", 0, "organization"),
                    ("Chapter 11 Books", 0, "organization")
                ]
            ),
            pairs(&[("Chapter 11", "ORG"), ("Chapter 11 Books", "ORG")])
        );
        let source = "C.F.R. LLC supplied the books. See C.F.R.";
        // Source evidence matching must accept punctuation belonging to
        // the short spelling, independently of category shape filters.
        assert!(pack().company_short_name(source, "C.F.R."));
    }

    /// A shared given name and punctuation prove no document label, and
    /// an attendance time is no pinpoint (g1 finding 4).
    #[test]
    fn citation_parts_keep_abbreviated_surnames_and_attendance_times() {
        for (source, full, abbreviated) in [
            (
                "Ivo Affleck testified (recorded as Ivo Aff.).",
                "Ivo Affleck",
                "Ivo Aff",
            ),
            (
                "Ivo Depp testified (recorded as Ivo Dep.).",
                "Ivo Depp",
                "Ivo Dep",
            ),
            (
                "Ivo Affleck testified (recorded as Ivo Aff.; present).",
                "Ivo Affleck",
                "Ivo Aff",
            ),
            (
                "Ivo Affleck testified; Ivo Aff. ¶ 4.",
                "Ivo Affleck",
                "Ivo Aff",
            ),
        ] {
            assert_eq!(
                shown_at(source, &[(full, 0, "person"), (abbreviated, 1, "person")]),
                pairs(&[(full, "PERSON"), (abbreviated, "PERSON")]),
                "{source}"
            );
        }
        assert_eq!(
            shown(
                "Witness: Ivo Dep. at 09:30; clerk: Orin Vale.",
                &[("Ivo Dep", "person"), ("Orin Vale", "person")]
            ),
            pairs(&[("Ivo Dep", "PERSON"), ("Orin Vale", "PERSON")])
        );
        for (source, name) in [
            ("Ivo Dep. at 09:30 signed the form.", "Ivo Dep"),
            ("Ivo Aff. at 09:30 signed the form.", "Ivo Aff"),
            ("Ivo Decl. at 09:30 signed the form.", "Ivo Decl"),
        ] {
            assert_eq!(
                shown(source, &[(name, "person")]),
                pairs(&[(name, "PERSON")]),
                "{source}"
            );
        }
        assert_eq!(
            shown(
                "Ivo Affleck testified (recorded as Ivo Dep.).",
                &[("Ivo Affleck", "person"), ("Ivo Dep", "person")]
            ),
            pairs(&[("Ivo Affleck", "PERSON"), ("Ivo Dep", "PERSON")])
        );
        assert_eq!(
            shown(
                "See Decl. of Ivo Penrow ¶ 4.",
                &[("Decl. of Ivo Penrow", "person")]
            ),
            pairs(&[("Ivo Penrow", "PERSON")])
        );
    }

    /// A citation-part fix must never hide a real name, company or number
    /// that merely resembles its marker (issue #499).
    #[test]
    fn citation_parts_keep_real_names_and_identifiers() {
        assert_eq!(
            shown(
                "The witness (Ivo Decl.) testified.",
                &[("Ivo Decl", "person")]
            ),
            pairs(&[("Ivo Decl", "PERSON")])
        );
        for name in [
            "Declan Penrow",
            "Ivo Affleck",
            "Ms Decl",
            "Mr Aff",
            "Ivo Decl",
            "Decl",
            "Aff",
            "Ex",
        ] {
            let expected = name
                .strip_prefix("Ms ")
                .or_else(|| name.strip_prefix("Mr "))
                .unwrap_or(name);
            let source = format!("The witness is {name}.");
            assert_eq!(
                shown(&source, &[(name, "person")]),
                pairs(&[(expected, "PERSON")]),
                "{source}"
            );
        }
        for name in [
            "Decl Research Ltd",
            "Aff Analytics LLC",
            "Chapter 11 Ltd",
            "National Instrument 45-999 Ltd",
            "PD 57ZZ Ltd",
            "Section XII LLC",
            "Comm Trading Ltd",
        ] {
            let source = format!("The applicant is {name}.");
            assert_eq!(
                shown(&source, &[(name, "organization")]),
                pairs(&[(name, "ORG")]),
                "{source}"
            );
        }
        for (source, text) in [
            ("See Decl. of Ivo Penrow ¶ 4.", "Decl. of Ivo Penrow"),
            (
                "See Ex. 1 to Ivo Penrow Decl. ¶ 4.",
                "Ex. 1 to Ivo Penrow Decl",
            ),
        ] {
            assert_eq!(
                shown(source, &[(text, "person")]),
                pairs(&[("Ivo Penrow", "PERSON")])
            );
        }
        // The same docket in this action or its caption must stay; only
        // a docket attached to a precedent is suppressed.
        for source in [
            "Case No. 09-31-00888-CV.",
            "Pellam v. Ostrand, 999 S.W.3d 1 (Tex. App.—Slocombe 2031, pet. filed), No. 09-31-00888-CV.",
            "The judgment under appeal is Pellam v. Ostrand, 999 S.W.3d 1 (Tex. App.—Slocombe 2031, pet. filed), No. 09-31-00888-CV.",
        ] {
            let detections = shown(source, &[("09-31-00888-CV", "national_id_number")]);
            assert!(
                detections.iter().any(|(text, _)| text == "09-31-00888-CV"),
                "{source}"
            );
        }
        assert_eq!(
            shown(
                "S.C. Penrow and H.L. Venn testified.",
                &[("S.C. Penrow", "person"), ("H.L. Venn", "person")]
            ),
            pairs(&[("S.C. Penrow", "PERSON"), ("H.L. Venn", "PERSON")])
        );
        assert_eq!(
            shown(
                "R. Penrow lives at 478A Cedar Road, Slocombe 45999.",
                &[
                    ("R. Penrow", "person"),
                    ("478A Cedar Road, Slocombe 45999", "address")
                ]
            ),
            pairs(&[
                ("R. Penrow", "PERSON"),
                ("478A Cedar Road, Slocombe 45999", "ADDRESS")
            ])
        );
        assert_eq!(
            shown(
                "Account 45-999; ID S-19; badge 57ZZ.",
                &[
                    ("45-999", "bank_account"),
                    ("S-19", "national_id_number"),
                    ("57ZZ", "national_id_number")
                ]
            ),
            pairs(&[
                ("45-999", "BANK"),
                ("S-19", "IDENTITY"),
                ("57ZZ", "IDENTITY")
            ])
        );
    }

    /// The real model run of #499's fixture: a Lord Justice's post-nominal
    /// comes off the judge's name, and a badge after its bare label is
    /// one person's identifier even when the model misses it.
    #[test]
    fn citation_parts_trim_lord_justices_and_mark_badges() {
        for (source, raw, expected) in [
            (
                "See [2031] All ER (Comm) 9, per Ivo Ostrand LJ; Ostrand Aff. ¶ 3.",
                "Ivo Ostrand LJ",
                vec![("Ivo Ostrand", "PERSON"), ("Ostrand", "PERSON")],
            ),
            (
                "Per Edda Venn LJJ, the appeal fails.",
                "Edda Venn LJJ",
                vec![("Edda Venn", "PERSON")],
            ),
            // "J" may be an initial and stays.
            (
                "The witness J Ostrand testified.",
                "J Ostrand",
                vec![("J Ostrand", "PERSON")],
            ),
        ] {
            assert_eq!(
                shown(source, &[(raw, "person")]),
                pairs(&expected),
                "{source}"
            );
        }
        for (source, badge) in [
            ("The witness provided badge 88ZZ.", "88ZZ"),
            ("Badge: 4471, issued at the gate.", "4471"),
            ("Badge no. KX42 was returned.", "KX42"),
        ] {
            assert_eq!(
                shown(source, &[]),
                pairs(&[(badge, "IDENTITY")]),
                "{source}"
            );
        }
        for source in [
            "Badge holders may enter.",
            "The badge 12 times failed.",
            "Wear your badge at all times.",
        ] {
            assert!(shown(source, &[]).is_empty(), "{source}");
        }
        // A labelled badge the model gives whole keeps its whole value.
        assert_eq!(
            shown(
                "Employee badge: PD 57ZZ.",
                &[("PD 57ZZ", "national_id_number")]
            ),
            pairs(&[("PD 57ZZ", "IDENTITY")])
        );
    }

    /// Exercise the CC0 fixture through correction and review with the
    /// faulty model boundaries from the probes, without inventing a model
    /// snapshot. The model gate supplies its own real output later (#499).
    #[test]
    fn citation_parts_fixture_keeps_its_declarants() {
        let fixture = crate::recognition::Fixture::parse(
            std::path::Path::new("en/judgment/probe-en-r3-citation-parts.md"),
            include_str!(
                "../../../tests/fixtures/recognition/en/judgment/probe-en-r3-citation-parts.md"
            ),
        )
        .unwrap();
        fixture.validate().unwrap();
        fixture.validate_layout().unwrap();
        let source = fixture.source.as_str();
        let mut raw: Vec<_> = fixture
            .gold
            .iter()
            .map(|span| {
                let mut range = span.range.clone();
                let label = match span.category.as_str() {
                    "PERSON" => "person",
                    "ORG" => "organization",
                    "IDENTITY" => "national_id_number",
                    other => panic!("unexpected category {other}"),
                };
                if label == "person" {
                    for word in [" Decl", " Aff", " Dep"] {
                        if source[range.end..].starts_with(word) {
                            range.end += word.len();
                            break;
                        }
                    }
                }
                crate::RawEntity {
                    start: range.start,
                    end: range.end,
                    label: label.into(),
                    score: 0.8,
                    text: source[range].into(),
                }
            })
            .collect();
        for (text, label) in [
            ("Chapter 11", "organization"),
            ("S.C.", "person"),
            ("H.L", "person"),
            ("Comm", "organization"),
            ("National Instrument 45-999", "national_id_number"),
            ("PD 57ZZ", "organization"),
            ("478A", "address"),
            ("Section XII", "organization"),
            ("352 DM - Aviation Safety", "organization"),
            ("AHCP/CCAA p. S-19", "organization"),
            ("Decl", "person"),
            ("Ex. 1", "person"),
            ("Tex. R. Civ", "organization"),
            ("Pellam", "person"),
            ("Brindle", "person"),
            ("Slocombe 2031", "address"),
            ("09-31-00888-CV", "national_id_number"),
            ("R", "person"),
        ] {
            // R is the judicial-review marker, not a letter in "Research".
            let start = if text == "R" {
                source.find("R (on").unwrap()
            } else {
                source.find(text).unwrap()
            };
            raw.push(crate::RawEntity {
                start,
                end: start + text.len(),
                label: label.into(),
                score: 0.8,
                text: text.into(),
            });
        }
        // The fixture also names Chapter 11 Ltd and Comm Trading Ltd.
        // Their short names stay everywhere under the conservative name
        // policy, even in references the fixture's existing gold omits.
        // Keep this expectation here without changing the fixture.
        let mut expected = fixture.gold.clone();
        for text in ["Chapter 11", "Comm"] {
            let start = source.find(text).unwrap();
            expected.push(crate::recognition::Span {
                range: start..start + text.len(),
                category: "ORG".into(),
            });
        }
        expected.sort();
        assert_eq!(
            crate::recognition::proposals(source, &raw).unwrap(),
            expected
        );
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
            // The title comes off a full name (guide rule 1, #387).
            ("Judge Edda Brannock", "Edda Brannock"),
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
            ("RM6841932", Category::Identity),
            ("GB123456789", Category::Tax),
        ] {
            assert!(keeps(text, category), "{text:?}");
        }
    }

    /// #375 review n2: each review input keeps what origin/main proposed,
    /// its value and every repeat of it.
    #[test]
    fn short_numbers_and_records_keep_real_values() {
        // Source, the model's spans (text, label), what is proposed.
        type Case<'a> = (&'a str, &'a [(&'a str, &'a str)], &'a [(&'a str, &'a str)]);
        let cases: &[Case] = &[
            // P1-1: a sort code in a table, after a newline or a hyphenated label, in prose.
            (
                "| Sort code | 20-00-00 |\n| Account number | 12345678 |",
                &[("20-00-00", "bank_account"), ("12345678", "bank_account")],
                &[("20-00-00", "BANK"), ("12345678", "BANK")],
            ),
            (
                "| Sort code | 20-00-00 |",
                &[("20-00-00", "routing_number")],
                &[("20-00-00", "BANK")],
            ),
            (
                "Sort code: 20-00-00",
                &[("20-00-00", "bank_account")],
                &[("20-00-00", "BANK")],
            ),
            (
                "Sort code:\n20-00-00",
                &[("20-00-00", "bank_account")],
                &[("20-00-00", "BANK")],
            ),
            (
                "Sort-code 20-00-00, account 12345678.",
                &[("20-00-00", "bank_account")],
                &[("20-00-00", "BANK")],
            ),
            (
                "Pay into Mr Penrow's account at Barclays, 20-00-00, 12345678.",
                &[("20-00-00", "bank_account")],
                &[("20-00-00", "BANK")],
            ),
            // P1-2: dotted phones, unlabelled, in a table, after «тел./факс».
            (
                "Связаться с Ивановым: (495) 123.45.67",
                &[("(495) 123.45.67", "phone_number")],
                &[("(495) 123.45.67", "PHONE")],
            ),
            (
                "Иванов И. И., +7 495 123.45.67",
                &[("+7 495 123.45.67", "phone_number")],
                &[("+7 495 123.45.67", "PHONE")],
            ),
            (
                "Иванов И. И., 8.912.345.67.89",
                &[("8.912.345.67.89", "phone_number")],
                &[("8.912.345.67.89", "PHONE")],
            ),
            (
                "| Иванов И. И. | (495) 123.45.67 |",
                &[("(495) 123.45.67", "phone_number")],
                &[("(495) 123.45.67", "PHONE")],
            ),
            (
                "Contact Mme Durand on 01.23.45.67.89.",
                &[("01.23.45.67.89", "phone_number")],
                &[("01.23.45.67.89", "PHONE")],
            ),
            (
                "Mr Penrow, 07700.900.123",
                &[("07700.900.123", "phone_number")],
                &[("07700.900.123", "PHONE")],
            ),
            (
                "Иванов И. И., тел./факс (495) 123.45.67",
                &[("(495) 123.45.67", "phone_number")],
                &[("(495) 123.45.67", "PHONE")],
            ),
            // P2-2: short values reach their repeats, labelled or not.
            (
                "Phone: 6121. Phone: 6121.",
                &[("6121", "phone_number")],
                &[("6121", "PHONE"), ("6121", "PHONE")],
            ),
            (
                "тел. 6121; тел.: 6121",
                &[("6121", "phone_number")],
                &[("6121", "PHONE"), ("6121", "PHONE")],
            ),
            (
                "тел. 6121. Звоните по телефону 6121.",
                &[("6121", "phone_number")],
                &[("6121", "PHONE"), ("6121", "PHONE")],
            ),
            (
                "PIN: 4821. The PIN 4821 was shared.",
                &[("4821", "passport_number")],
                &[("4821", "IDENTITY"), ("4821", "IDENTITY")],
            ),
            (
                "| Удостоверение | 9917 |\n| Подпись адвоката (удостоверение 9917) |",
                &[("9917", "national_id_number")],
                &[("9917", "IDENTITY"), ("9917", "IDENTITY")],
            ),
            (
                "Account No. 1234; account 1234 again",
                &[("1234", "bank_account")],
                &[("1234", "BANK"), ("1234", "BANK")],
            ),
            (
                "ИНН 1234 (опечатка); ИНН 1234",
                &[("1234", "tax_id")],
                &[("1234", "TAX"), ("1234", "TAX")],
            ),
            // P2-3: short phones after ordinary labels; an extension and its repeat.
            (
                "Phone No. 6121",
                &[("6121", "phone_number")],
                &[("6121", "PHONE")],
            ),
            (
                "Tel. No.: 6121",
                &[("6121", "phone_number")],
                &[("6121", "PHONE")],
            ),
            (
                "Telephone extension 4521",
                &[("4521", "phone_number")],
                &[("4521", "PHONE")],
            ),
            (
                "Extension: 4521",
                &[("4521", "phone_number")],
                &[("4521", "PHONE")],
            ),
            (
                "Mr Penrow, ext. 4521",
                &[("4521", "phone_number")],
                &[("4521", "PHONE")],
            ),
            (
                "Звонить по телефону 6121.",
                &[("6121", "phone_number")],
                &[("6121", "PHONE")],
            ),
            (
                "внутренний 6121",
                &[("6121", "phone_number")],
                &[("6121", "PHONE")],
            ),
            (
                "| Телефон | 6121 |",
                &[("6121", "phone_number")],
                &[("6121", "PHONE")],
            ),
            (
                "+7 (495) 123-45-67, доб. 6121",
                &[
                    ("+7 (495) 123-45-67", "phone_number"),
                    ("6121", "phone_number"),
                ],
                &[("+7 (495) 123-45-67", "PHONE"), ("6121", "PHONE")],
            ),
            (
                "Phone: 020 7946 0958 x207; ask for x207 again",
                &[("x207", "phone_number")],
                &[("x207", "PHONE"), ("x207", "PHONE")],
            ),
            // P2-4: SWIFT/BIC codes.
            (
                "SWIFT/BIC: NWBKGB2L",
                &[("NWBKGB2L", "bank_account")],
                &[("NWBKGB2L", "BANK")],
            ),
            (
                "SWIFT code DEUTDEFF500",
                &[("DEUTDEFF500", "bank_account")],
                &[("DEUTDEFF500", "BANK")],
            ),
            // P2-5: masked and partial card and account numbers.
            (
                "The fee was charged to Ms Penrow's card ending 4242.",
                &[("4242", "bank_account")],
                &[("4242", "BANK")],
            ),
            (
                "Visa **** **** **** 4242",
                &[("**** **** **** 4242", "bank_account")],
                &[("**** **** **** 4242", "BANK")],
            ),
            (
                "Account ending ****1234",
                &[("****1234", "bank_account")],
                &[("****1234", "BANK")],
            ),
            (
                "Acct: ****1234",
                &[("****1234", "bank_account")],
                &[("****1234", "BANK")],
            ),
            (
                "Номер карты: **** 1234",
                &[("**** 1234", "bank_account")],
                &[("**** 1234", "BANK")],
            ),
            // P2-6: seven-digit accounts.
            (
                "| Лицевой счёт | 1234567 |",
                &[("1234567", "bank_account")],
                &[("1234567", "BANK")],
            ),
            (
                "| Account | 1234567 |",
                &[("1234567", "bank_account")],
                &[("1234567", "BANK")],
            ),
            (
                "л/с 1234567",
                &[("1234567", "bank_account")],
                &[("1234567", "BANK")],
            ),
            // P2-7: phones beside or inside a Windows path.
            (
                r"Saved as C:\Matters\Penrow\letter.doc - then call Penrow on 020 7946 0958 before sending reply.pdf",
                &[("020 7946 0958", "phone_number")],
                &[("020 7946 0958", "PHONE")],
            ),
            (
                r"Файл D:\Дела\Иванов\иск.docx отправлен Иванову, 8 912 345-67-89, вместе с копией скан.pdf",
                &[("8 912 345-67-89", "phone_number")],
                &[("8 912 345-67-89", "PHONE")],
            ),
            (
                r"W:\Penrow\contacts 020 7946 0958.txt",
                &[("020 7946 0958", "phone_number")],
                &[("020 7946 0958", "PHONE")],
            ),
            // P2-8: a lone "R" or "×" in an address or a passport.
            (
                "Ms Penrow lives at 1500 R Street NW.",
                &[("1500 R Street NW", "address")],
                &[("1500 R Street NW", "ADDRESS")],
            ),
            (
                "Flat 12, Block R 3, Larch Estate, Bristol BS6 9ZY",
                &[(
                    "Flat 12, Block R 3, Larch Estate, Bristol BS6 9ZY",
                    "address",
                )],
                &[(
                    "Flat 12, Block R 3, Larch Estate, Bristol BS6 9ZY",
                    "ADDRESS",
                )],
            ),
            (
                "Apartment 4, Building R 7, Larch Park, London E1 6AN",
                &[(
                    "Apartment 4, Building R 7, Larch Park, London E1 6AN",
                    "address",
                )],
                &[(
                    "Apartment 4, Building R 7, Larch Park, London E1 6AN",
                    "ADDRESS",
                )],
            ),
            (
                "Ms Penrow lives at 1500 R Street NW, Washington, DC 20009.",
                &[("1500 R Street NW, Washington, DC 20009", "address")],
                &[("1500 R Street NW, Washington, DC 20009", "ADDRESS")],
            ),
            (
                "Flat 2, 14 R Road, Bristol BS6 9ZY",
                &[("Flat 2, 14 R Road, Bristol BS6 9ZY", "address")],
                &[("Flat 2, 14 R Road, Bristol BS6 9ZY", "ADDRESS")],
            ),
            (
                "Passport 12 R 345678",
                &[("12 R 345678", "passport_number")],
                &[("12 R 345678", "IDENTITY")],
            ),
            (
                "Lot 7 × 3, Larch Road, Bristol BS6 9ZY",
                &[("Lot 7 × 3, Larch Road, Bristol BS6 9ZY", "address")],
                &[("Lot 7 × 3, Larch Road, Bristol BS6 9ZY", "ADDRESS")],
            ),
            // P2-9: a notary's licence and an administrative-offence protocol.
            (
                "Нотариус Брюн Л. И., лицензия № 0099911",
                &[("0099911", "national_id_number")],
                &[("0099911", "IDENTITY")],
            ),
            // An administrative-offence protocol number is CASE (#484 item 7,
            // #364's label rule).
            (
                "протокол об административном правонарушении № 0199999",
                &[("0199999", "national_id_number")],
                &[("0199999", "CASE")],
            ),
            // P3-1: a list after a record number stops before a phone or an ИНН;
            // a licence is no record.
            (
                "по договорам №№ 15, 16, 89123456789",
                &[("89123456789", "phone_number")],
                &[("89123456789", "PHONE")],
            ),
            (
                "договор № 15,7701234567",
                &[("7701234567", "tax_id")],
                &[("7701234567", "TAX")],
            ),
            (
                "лицензия ИП-7701234567",
                &[("7701234567", "tax_id")],
                &[("7701234567", "TAX")],
            ),
            // P3-2: a local phone after «т.» before a city.
            (
                "Иванов И.И., т. 234567, г. Энск",
                &[("234567", "phone_number")],
                &[("234567", "PHONE")],
            ),
            // P3-3: an IBAN split across lines or table cells.
            (
                "IBAN: GB29 NWBK\n6016 1331 9268 19",
                &[("GB29 NWBK", "iban"), ("6016 1331 9268 19", "iban")],
                &[("GB29 NWBK", "BANK"), ("6016 1331 9268 19", "BANK")],
            ),
            (
                "| GB29 NWBK | 6016 1331 9268 19 |",
                &[("GB29 NWBK", "iban"), ("6016 1331 9268 19", "iban")],
                &[("GB29 NWBK", "BANK"), ("6016 1331 9268 19", "BANK")],
            ),
        ];
        for (source, raw, expected) in cases {
            assert_eq!(shown(source, raw), pairs(expected), "{source}");
        }
    }

    /// #375: the issue's false hits still go where a narrow rule can tell
    /// them from a real value: short line, header and route numbers, plot
    /// ticks, drawing dimensions, a file number in a document path, Russian
    /// record numbers and a postcode before a redacted address.
    #[test]
    fn short_numbers_and_records_are_no_bank_phone_or_tax() {
        // The issue's synthetic reproduction: only the labelled phone stays.
        let source = "[021] Пояснительная записка, тел. 6121. The Brindle Alternative \
                      Segment 3 route is set out in Paragraphs 1 through 24.";
        assert_eq!(
            shown(
                source,
                &[
                    ("021", "phone_number"),
                    ("6121", "phone_number"),
                    ("Brindle Alternative Segment 3", "bank_account"),
                    ("24", "bank_account"),
                ]
            ),
            pairs(&[("6121", "PHONE")])
        );
        for value in [
            "24",
            "300",
            "Eielson Alternative Segment 3",
            "Segments 1 & 2",
            "abcdefghijk12345678",
        ] {
            assert_eq!(shown(value, &[(value, "bank_account")]), [], "{value}");
        }
        for value in ["021", "[021]", "1.981.96", "1.98.96"] {
            assert_eq!(shown(value, &[(value, "phone_number")]), [], "{value}");
        }
        // A label lifts the counts and the plot-tick veto.
        for (source, value, label, category) in [
            ("Phone: 1.981.96", "1.981.96", "phone_number", "PHONE"),
            ("Tel. 021", "021", "phone_number", "PHONE"),
            ("ext. 207", "207", "phone_number", "PHONE"),
            ("доб. 207", "207", "phone_number", "PHONE"),
            ("Account No. 24", "24", "bank_account", "BANK"),
        ] {
            assert_eq!(
                shown(source, &[(value, label)]),
                pairs(&[(value, category)]),
                "{source}"
            );
        }
        // Drawing dimensions, but no number beside a lone "R" or "×".
        for value in ["76×6.4×914", "76 × 6.4 × 914", "6.4 R to inside"] {
            for label in ["bank_account", "phone_number", "tax_id", "address"] {
                assert_eq!(shown(value, &[(value, label)]), [], "{value}: {label}");
            }
        }
        assert_eq!(
            shown(
                "Detail 76×6.4×914; contact 020 7946 0958",
                &[("020 7946 0958", "phone_number")]
            ),
            pairs(&[("020 7946 0958", "PHONE")])
        );
        // A file number in a document path; the path's other text stays.
        for (source, value) in [
            (r"W:\Penrow\09-F-0999 final.doc", "09-F-0999"),
            (r"W:\Penrow\final 09-F-0999.doc", "09-F-0999"),
            (r"W:\Penrow\Drafts\09-F-0999.pdf", "0999"),
        ] {
            assert_eq!(shown(source, &[(value, "phone_number")]), [], "{source}");
        }
        assert_eq!(
            shown(
                "Phone: 555-01-99; file W:\\Penrow\\note.doc",
                &[("555-01-99", "phone_number")]
            ),
            pairs(&[("555-01-99", "PHONE")])
        );
        // An administrative-offence protocol number is no phone but CASE
        // (#484 item 7).
        assert_eq!(
            shown(
                "протокол об административном правонарушении № 0199999",
                &[("0199999", "phone_number")]
            ),
            pairs(&[("0199999", "CASE")])
        );
        // Russian record numbers, list members of the first one's form too.
        for (source, value) in [
            ("Договор № 12345678", "12345678"),
            (
                "договорами об ипотеке № ДИ-999999/2031/00009",
                "ДИ-999999/2031/00009",
            ),
            (
                "договорами об ипотеке № ДИ-999999/2031/00009, ДИ-999998/2031/00010",
                "ДИ-999998/2031/00010",
            ),
            ("договоров от 01.02.2031 № 12345678", "12345678"),
            ("по проектам №№ 1199999, 119999Е", "119999Е"),
            ("по проектам №№ 1152744, 118577Е, 118578Е", "118578Е"),
            ("сообщение № 9999999", "9999999"),
            ("объявление № 13838487", "13838487"),
            ("протокол № 0199999", "0199999"),
            ("по ордеру № 009999", "009999"),
            (
                "свидетельства о депонировании произведения № 023-014748",
                "023-014748",
            ),
            (
                "учетный номер в ЕРКНМ 20999999999999999999",
                "20999999999999999999",
            ),
        ] {
            for label in ["phone_number", "bank_account", "tax_id"] {
                assert_eq!(shown(source, &[(value, label)]), [], "{source}: {label}");
            }
            // A labelled value after the record stays.
            for (tail, pii, label, category) in [
                (
                    "; телефон +7 (999) 555-01-23",
                    "+7 (999) 555-01-23",
                    "phone_number",
                    "PHONE",
                ),
                ("; ИНН 7709999999", "7709999999", "tax_id", "TAX"),
                (
                    "; р/с 40702810900000004711",
                    "40702810900000004711",
                    "bank_account",
                    "BANK",
                ),
            ] {
                let source = format!("{source}{tail}");
                assert!(
                    shown(&source, &[(pii, label)]).contains(&(pii.into(), category.into())),
                    "{source}"
                );
            }
        }
        // A record word cannot reach over a label to a later number sign.
        for (source, value, label, category) in [
            ("договор телефон 6121 № 9", "6121", "phone_number", "PHONE"),
            (
                "свидетельство телефон 6121 № 9",
                "6121",
                "phone_number",
                "PHONE",
            ),
            (
                "по ордеру № 12 удостоверение № 9917",
                "9917",
                "national_id_number",
                "IDENTITY",
            ),
        ] {
            assert!(
                shown(source, &[(value, label)]).contains(&(value.into(), category.into())),
                "{source}"
            );
        }
        // A postcode before a redacted address or «г.»; not after «т.».
        for source in [
            "адрес регистрации: 999999, <...>",
            "999999, г. Энск",
            "Иванов, 234567, г. Энск",
        ] {
            let value = &source[source.find(|c: char| c.is_ascii_digit()).unwrap()..][..6];
            let spans = shown(source, &[(value, "phone_number")]);
            assert!(
                spans.iter().all(|(_, category)| category != "PHONE"),
                "{source}: {spans:?}"
            );
        }
        assert!(
            shown("тел. 999999, г. Энск", &[("999999", "phone_number")])
                .contains(&("999999".into(), "PHONE".into()))
        );
    }

    /// Money, including OCR-joined balance-sheet cells and fragments cut
    /// at their decimal comma, is never TAX, BANK or PHONE (#547).
    #[test]
    fn money_and_its_fragments_are_no_identifiers() {
        for (source, fragment) in [
            ("Остаток: 4 588 762,0323 058 712,74 руб.", "0323 058 712"),
            ("Остаток: 4 588 762,0323 058 712,74 руб.", "762,0323 058"),
            ("Остаток: 4 588 762,0323 058 712,74 руб.", "4 588 762"),
            ("Итого 12 345,67942838 306,12.", "942838 306"),
            ("Итого 12 345,670321 448 312,74.", "0321 448 312"),
            ("Бюджет: 164 353 783 тыс. руб.", "164 353 783"),
            ("Бюджет: 12 345,05 8 443 789 778,00.", "05 8 443 789 778"),
            ("Выплачено 135 130 руб. 86 коп.", "86"),
            ("Выплачено 135 130 руб. 86 коп.", "135 130"),
            ("Итого 714157,31.", "714157"),
            ("Итого 23 707 750,02.", "23 707 750"),
            ("Итого 2903,02.", "2903"),
            ("Итого 714157.31.", "714157"),
            ("Итого 23\u{a0}707\u{202f}750,02.", "23\u{a0}707\u{202f}750"),
            ("Итого 23&nbsp;707&#32;750,02.", "23&nbsp;707&#32;750"),
            ("Итого 164 353 783 млн рублей.", "164 353 783"),
            ("Итого 164 353 783 €.", "164 353 783"),
            ("Сумма, руб.: 164 353 783.", "164 353 783"),
            ("The fund lost $350 billion.", "$350 billion"),
            ("The fund lost $350 billion.", "350"),
            ("The fund lost $11.2 billion a year.", "$11.2 billion"),
            ("The fund lost $11.2 billion a year.", "11.2"),
            ("The fund lost £350 million.", "350 million"),
            ("The fund lost €350 million.", "350"),
            ("The fund lost 350 billion dollars.", "350 billion"),
            ("Tax ID: 999991111 received 350 billion.", "350 billion"),
            ("Пени: ((0,06x159548,05) х2).", "06x159548"),
            ("Пени: (0,05х199999,05) ×2.", "05х199999"),
            ("Пени: (0,05*199999,05) *2.", "05*199999"),
            (
                "Взыскать 1 199999 (один миллион сто девяносто девять тысяч девятьсот девяносто девять) рублей.",
                "1 199999",
            ),
            (
                "Взыскать 1 129731 (один миллион сто двадцать девять тысяч семьсот тридцать один) рубль.",
                "1 129731",
            ),
        ] {
            for label in ["tax_id", "bank_account", "phone_number"] {
                assert_eq!(
                    shown(source, &[(fragment, label)]),
                    [],
                    "{source}: {label} {fragment}"
                );
            }
        }
        // A labelled identifier earlier in the sentence must not exempt
        // a later amount; the context rule still proposes the account.
        assert_eq!(
            shown(
                "Bank account: 12345678 funded 350 million.",
                &[("350 million", "bank_account")]
            ),
            [("12345678".into(), "BANK".into())]
        );
    }

    /// A column gap ends the amount's token group, even without a label (#547).
    #[test]
    fn amount_guards_keep_neighbouring_columns() {
        assert_eq!(
            shown(
                "Account     Balance\n12345678    1000.00",
                &[("12345678", "bank_account"), ("1000.00", "bank_account")]
            ),
            pairs(&[("12345678", "BANK")])
        );
        for gap in ["    ", "  ", "\t", "\u{a0}\u{a0}"] {
            for (value, label, category) in [
                ("12345678", "bank_account", "BANK"),
                ("7709999999", "tax_id", "TAX"),
                ("2025550199", "phone_number", "PHONE"),
            ] {
                for amount in [
                    "1000.00",
                    "714157,31",
                    "350 dollars.",
                    "$350",
                    "350 (триста пятьдесят) рублей.",
                ] {
                    let source = format!("{value}{gap}{amount}");
                    assert_eq!(
                        shown(&source, &[(value, label)]),
                        pairs(&[(value, category)]),
                        "{source:?}"
                    );
                }
                // A currency in the previous column is not this value's cue.
                let source = format!("350 dollars{gap}{value}");
                assert_eq!(
                    shown(&source, &[(value, label)]),
                    pairs(&[(value, category)]),
                    "{source:?}"
                );
            }
        }
    }

    /// An identifier label exempts its own value, not the next column (#547).
    #[test]
    fn amount_label_exemptions_end_at_column_gaps() {
        for gap in ["    ", "  ", "\t", "\u{a0}\u{a0}"] {
            for (prefix, value, amount, fragment, label, category) in [
                (
                    "Bank account: ",
                    "12345678",
                    "350 dollars.",
                    "350",
                    "bank_account",
                    "BANK",
                ),
                (
                    "ИНН ",
                    "7709999999",
                    "714157,31 руб.",
                    "714157",
                    "tax_id",
                    "TAX",
                ),
                (
                    "Tax ID: ",
                    "999991111",
                    "350 dollars.",
                    "350",
                    "tax_id",
                    "TAX",
                ),
                (
                    "Счёт: ",
                    "12345678",
                    "350 руб.",
                    "350",
                    "bank_account",
                    "BANK",
                ),
                (
                    "Phone: ",
                    "2025550199",
                    "350 dollars.",
                    "350",
                    "phone_number",
                    "PHONE",
                ),
                (
                    "Телефон: ",
                    "79990000123",
                    "350 руб.",
                    "350",
                    "phone_number",
                    "PHONE",
                ),
            ] {
                let source = format!("{prefix}{value}{gap}{amount}");
                assert_eq!(
                    shown(&source, &[(value, label), (fragment, label)]),
                    pairs(&[(value, category)]),
                    "{source:?}"
                );
            }
            // Twenty digits in separate columns do not make one bank account.
            let source = format!("12345678{gap}123456789012 dollars.");
            assert_eq!(
                shown(
                    &source,
                    &[
                        ("12345678", "bank_account"),
                        ("123456789012", "bank_account")
                    ]
                ),
                pairs(&[("12345678", "BANK")]),
                "{source:?}"
            );
        }
    }

    /// No amount cue means no reason to discard an identifier. Labels and
    /// twenty-digit accounts win even with monetary context beside them.
    #[test]
    fn amount_guards_keep_identifiers_and_names() {
        for (source, value, label, category) in [
            // Single-space OCR columns must not join identifiers to money.
            (
                "Иванов И.И. 770912345678 15 000 руб.",
                "770912345678",
                "tax_id",
                "TAX",
            ),
            (
                "ООО «Ромашка» 7709999999 1 000,00",
                "7709999999",
                "tax_id",
                "TAX",
            ),
            ("Сумма 1 000,00 7709999999", "7709999999", "tax_id", "TAX"),
            (
                "Петров П.П. 8 916 123 45 67 1 500 руб.",
                "8 916 123 45 67",
                "phone_number",
                "PHONE",
            ),
            (
                "Карта 4276 3801 2345 6789 500 руб.",
                "4276 3801 2345 6789",
                "bank_account",
                "BANK",
            ),
            (
                "Сумма 1 000,00 4081781009991000431",
                "4081781009991000431",
                "bank_account",
                "BANK",
            ),
            (
                "Паспорт 4506 123456 01.02.2020",
                "4506 123456",
                "tax_id",
                "TAX",
            ),
            ("770912345678 01.02.2020", "770912345678", "tax_id", "TAX"),
            (
                "Взыскать 500 руб. 7709999999",
                "7709999999",
                "tax_id",
                "TAX",
            ),
            (
                "Оплачено 500 руб. 8 916 123 45 67",
                "8 916 123 45 67",
                "phone_number",
                "PHONE",
            ),
            (
                "Mobile: 202 555 0199 $500",
                "202 555 0199",
                "phone_number",
                "PHONE",
            ),
            (
                "Call 020 7946 0958 £0.10",
                "020 7946 0958",
                "phone_number",
                "PHONE",
            ),
            ("SSN 123 45 6789 $500", "123 45 6789", "tax_id", "TAX"),
            ("UTR 12345 67890 £500", "12345 67890", "tax_id", "TAX"),
            (
                "IBAN DE89 3704 0044 0532 0130 00 € 500",
                "3704 0044 0532 0130 00",
                "bank_account",
                "BANK",
            ),
            (
                "ИНН/КПП 7709999999/770901001 500 руб.",
                "770901001",
                "tax_id",
                "TAX",
            ),
            (
                "ИНН/КПП 7709999999/770901001 500 руб.",
                "7709999999",
                "tax_id",
                "TAX",
            ),
            (
                "моб. 8 916 123 45 67 500 руб.",
                "8 916 123 45 67",
                "phone_number",
                "PHONE",
            ),
            (
                "Routing 021000021 $500",
                "021000021",
                "bank_account",
                "BANK",
            ),
            // Working labels in the review remain working beside money.
            (
                "тел. 8 (495) 123-45-67 500 руб.",
                "8 (495) 123-45-67",
                "phone_number",
                "PHONE",
            ),
            (
                "Account No. 12 345 678 $500",
                "12 345 678",
                "bank_account",
                "BANK",
            ),
            (
                "р/с 40702810900000004711 500 руб.",
                "40702810900000004711",
                "bank_account",
                "BANK",
            ),
            (
                "Phone 202.555.0199 $500",
                "202.555.0199",
                "phone_number",
                "PHONE",
            ),
            // Dates with a decimal-looking prefix are not amount evidence.
            ("770912345678 01.02,2020", "770912345678", "tax_id", "TAX"),
            // Written-out amounts have the same integer boundaries.
            (
                "770912345678 500 (пятьсот) рублей.",
                "770912345678",
                "tax_id",
                "TAX",
            ),
            ("Номер: 164 353 783.", "164 353 783", "tax_id", "TAX"),
            ("Номер: 942838 306.", "942838 306", "bank_account", "BANK"),
            (
                "Номер: 0321 448 312.",
                "0321 448 312",
                "phone_number",
                "PHONE",
            ),
            (
                "Номер: 1 199999 (основной) рублей не содержит.",
                "1 199999",
                "tax_id",
                "TAX",
            ),
            (
                "Счёт: 40702810900000004711 руб.",
                "40702810900000004711",
                "bank_account",
                "BANK",
            ),
            (
                "40702 810 9 0000 0004711 руб.",
                "40702 810 9 0000 0004711",
                "bank_account",
                "BANK",
            ),
            ("ИНН 7709999999,02 руб.", "7709999999", "tax_id", "TAX"),
            ("ОГРН 1199900000011 руб.", "1199900000011", "tax_id", "TAX"),
            ("КПП 779901001 руб.", "779901001", "tax_id", "TAX"),
            ("БИК 049999999 руб.", "049999999", "bank_account", "BANK"),
            ("account: 12345.67.", "12345.67", "bank_account", "BANK"),
            ("Счёт: 12345.67 руб.", "12345.67", "bank_account", "BANK"),
            (
                "Телефон: 79990000123 руб.",
                "79990000123",
                "phone_number",
                "PHONE",
            ),
            (
                "Phone: 2025550199 dollars.",
                "2025550199",
                "phone_number",
                "PHONE",
            ),
            (
                "Bank account: 12345678 dollars.",
                "12345678",
                "bank_account",
                "BANK",
            ),
            (
                "Tax ID: 999991111,02 dollars.",
                "999991111",
                "tax_id",
                "TAX",
            ),
            (
                "Tax ID: 999 991 111 dollars.",
                "999 991 111",
                "tax_id",
                "TAX",
            ),
            (
                "Bank account: 1234 5678 dollars.",
                "1234 5678",
                "bank_account",
                "BANK",
            ),
            (
                "Phone: +1 (202) 555-0199 dollars.",
                "+1 (202) 555-0199",
                "phone_number",
                "PHONE",
            ),
            ("ИНН 770 999 9999 руб.", "770 999 9999", "tax_id", "TAX"),
            ("Счёт: 1234 5678 руб.", "1234 5678", "bank_account", "BANK"),
            (
                "Телефон: +7 (999) 555-01-23 руб.",
                "+7 (999) 555-01-23",
                "phone_number",
                "PHONE",
            ),
            (
                "ИНН 7709999999; остаток 714157,31 руб.",
                "7709999999",
                "tax_id",
                "TAX",
            ),
            (
                "Остаток 714157,31 руб.; БИК 049999999.",
                "049999999",
                "bank_account",
                "BANK",
            ),
            (
                "Остаток 714157,31 руб.; телефон +7 (999) 555-01-23.",
                "+7 (999) 555-01-23",
                "phone_number",
                "PHONE",
            ),
            (
                "1 199999 рублей перечислены ООО «Кедровый бор».",
                "ООО «Кедровый бор»",
                "organization",
                "ORG",
            ),
            (
                "£350 million paid to Maren Quill.",
                "Maren Quill",
                "person",
                "PERSON",
            ),
        ] {
            assert_eq!(
                shown(source, &[(value, label)]),
                [(value.into(), category.into())],
                "{source}"
            );
        }
    }

    /// A total label cannot establish where an OCR column ends (#547, round 2 P1).
    #[test]
    fn amount_totals_keep_the_next_columns_identifiers() {
        for gap in [" ", "  ", "\t"] {
            for (header, value, label, category) in [
                ("Сумма  КПП", "770 901 001", "tax_id", "TAX"),
                ("Сумма  Routing", "021 000 021", "bank_account", "BANK"),
            ] {
                for source in [
                    format!("{header}\nИтого 500{gap}{value}"),
                    format!("Итого 500{gap}{value}"),
                ] {
                    assert_eq!(
                        shown(&source, &[(value, label)]),
                        pairs(&[(value, category)]),
                        "{source:?}"
                    );
                }
            }
        }
    }

    /// Grouped identifiers cannot borrow a neighbour's amount cue (#547, round 2 P1).
    #[test]
    fn amount_guards_keep_grouped_identifiers_beside_money() {
        for (value, label, category) in [
            ("770 901 001", "tax_id", "TAX"),
            ("1 199 900 000 011", "tax_id", "TAX"),
            ("021 000 021", "bank_account", "BANK"),
            ("202 555 019", "phone_number", "PHONE"),
        ] {
            for source in [
                format!("{value} 500 руб."),
                format!("500 руб. {value}"),
                format!("$500 {value}"),
                format!("{value} 500,00"),
                format!("{value} 500.00"),
                format!("{value} 500 (пятьсот) рублей."),
                format!("{value} 500 €"),
            ] {
                assert_eq!(
                    shown(&source, &[(value, label)]),
                    pairs(&[(value, category)]),
                    "{source:?}"
                );
            }
        }
        // The review's literal OCR rows have header labels on another line.
        for (source, value) in [
            (
                "Организация  КПП  Сумма\nООО «Ромашка» 770 901 001 500 руб.",
                "770 901 001",
            ),
            (
                "Организация  ОГРН  Сумма\nООО «Ромашка» 1 199 900 000 011 500 руб.",
                "1 199 900 000 011",
            ),
        ] {
            assert_eq!(
                shown(source, &[(value, "tax_id")]),
                pairs(&[(value, "TAX")])
            );
        }
    }

    /// Multiword and slash-spaced labels protect only their values (#547, round 2 P2).
    #[test]
    fn amount_guards_keep_multiword_and_paired_labels() {
        for (source, value, label, category) in [
            (
                "Routing number: 021000021 dollars.",
                "021000021",
                "bank_account",
                "BANK",
            ),
            (
                "ABA routing number: 021000021 dollars.",
                "021000021",
                "bank_account",
                "BANK",
            ),
            (
                "Mobile number: 2025550199 dollars.",
                "2025550199",
                "phone_number",
                "PHONE",
            ),
            (
                "ИНН / КПП 7709999999 / 770901001 руб.",
                "770901001",
                "tax_id",
                "TAX",
            ),
            (
                "ИНН / КПП 7709999999 / 770901001 руб.",
                "7709999999",
                "tax_id",
                "TAX",
            ),
        ] {
            assert_eq!(
                shown(source, &[(value, label)]),
                pairs(&[(value, category)]),
                "{source}"
            );
            // Neither a later amount in prose nor another table column is exempt.
            for separator in ["; paid ", "  ", "\t"] {
                let source = format!("{source}{separator}714157,31");
                assert_eq!(
                    shown(&source, &[(value, label), ("714157", label)]),
                    pairs(&[(value, category)]),
                    "{source:?}"
                );
            }
        }
    }

    /// Labels win even when the identifier itself has an amount's shape.
    #[test]
    fn amount_guards_keep_additional_labelled_values() {
        for (source, value, label, category) in [
            // Existing conservative account-label exception, acknowledged in review.
            ("на счёт 500 руб.", "500", "bank_account", "BANK"),
            (
                "ИНН/КПП 7709999999/770901001 руб.",
                "770901001",
                "tax_id",
                "TAX",
            ),
            (
                "моб. 123 456 789 руб.",
                "123 456 789",
                "phone_number",
                "PHONE",
            ),
            (
                "сот. 123 456 789 руб.",
                "123 456 789",
                "phone_number",
                "PHONE",
            ),
            (
                "мобильный: 123 456 789 руб.",
                "123 456 789",
                "phone_number",
                "PHONE",
            ),
            (
                "Mobile: 202 555 019 dollars.",
                "202 555 019",
                "phone_number",
                "PHONE",
            ),
            (
                "Cell: 202 555 019 dollars.",
                "202 555 019",
                "phone_number",
                "PHONE",
            ),
            (
                "Routing 021000021 dollars.",
                "021000021",
                "bank_account",
                "BANK",
            ),
            (
                "ABA: 021000021 dollars.",
                "021000021",
                "bank_account",
                "BANK",
            ),
            ("SSN 123 456 789 dollars.", "123 456 789", "tax_id", "TAX"),
            ("UTR 123 456 789 dollars.", "123 456 789", "tax_id", "TAX"),
            ("Паспорт 4506 123456 руб.", "4506 123456", "tax_id", "TAX"),
        ] {
            assert_eq!(
                shown(source, &[(value, label)]),
                pairs(&[(value, category)]),
                "{source}"
            );
        }
    }

    /// Context searches stay local and preserve UTF-8 boundaries on long OCR lines.
    #[test]
    fn amount_guards_use_bounded_context() {
        for (value, label, category) in [
            ("7709999999", "tax_id", "TAX"),
            ("12345678", "bank_account", "BANK"),
            ("2025550199", "phone_number", "PHONE"),
        ] {
            let source = format!(
                "{} {value} 500 руб. {}",
                "ё ".repeat(1000),
                "ё ".repeat(1000)
            );
            assert_eq!(
                shown(&source, &[(value, label), ("500", label)]),
                pairs(&[(value, category)])
            );
        }
        let pack = Pack::load(&[(
            "test",
            r#"
            # Distant context is insufficient to discard a possible identifier.
            [[shape]]
            category = "TAX"
            forbid_in = ['price: .*\d+ units']
        "#,
        )])
        .unwrap();
        let source = format!("price: {} 123 units", "ё ".repeat(1000));
        let at = source.find("123").unwrap();
        assert!(pack.identifying_at(&source, at..at + 3, "123", Category::Tax));
    }

    /// The contextual shape extension uses the pack's existing match
    /// boundaries and validates its regexes just like span patterns.
    #[test]
    fn contextual_shapes_have_bounded_ranges_and_checked_patterns() {
        let pack = Pack::load(&[(
            "test",
            r#"
            [[shape]]
            category = "TAX"
            forbid_in = ['(?P<lead>price: )\d+(?P<tail> units)']
        "#,
        )])
        .unwrap();
        let source = "price: 123 units; 456";
        assert!(!pack.identifying_at(source, 7..10, "123", Category::Tax));
        assert!(pack.identifying_at(source, 18..21, "456", Category::Tax));
        let pack = Pack::load(&[(
            "test",
            r#"
            [[shape]]
            category = "TAX"
            forbid = ['^666$']
            forbid_in = ['\d+ units']
            unless_in = ['ID: \d+ units']
        "#,
        )])
        .unwrap();
        assert!(pack.identifying_at("ID: 123 units", 4..7, "123", Category::Tax));
        assert!(!pack.identifying_at("ID: 666 units", 4..7, "666", Category::Tax));
        for field in ["forbid_in", "unless_in"] {
            for pattern in ["x*", "(", "(?P<unknown>x)"] {
                let toml = format!("[[shape]]\ncategory = 'TAX'\n{field} = ['{pattern}']");
                assert!(Pack::load(&[("test", &toml)]).is_err(), "{toml}");
            }
        }
    }

    #[test]
    fn mistakes_in_a_rule_file_are_named() {
        for (source, problem) in [
            (
                "[[context]]\ncategory = \"DATE\"\nlabels = [\"birth date\"]\nafter = true\nline_start = true\nbetween = ' '\nvalue = '\\d{4}'",
                "no line_start or unless_after",
            ),
            (
                "[[context]]\ncategory = \"DATE\"\nlabels = [\"dob\"]\nfollow = '\\.'\nbetween = ' '\nvalue = '\\d{4}'",
                "only a label after its value takes only_after or follow",
            ),
            (
                "[[context]]\ncategory = \"DATE\"\nlabels = [\"Birth date\"]\nafter = true\nbetween = ' '\nvalue = '\\d{4}'",
                "after its value must be lowercase",
            ),
            (
                "[[context]]\ncategory = \"DATE\"\nlabels = [\"birth date\"]\nafter = true\nonly_after = [\"The\"]\nbetween = ' '\nvalue = '\\d{4}'",
                "is not one lowercase word or 's",
            ),
            (
                "[[context]]\ncategory = \"DATE\"\nlabels = [\"birth date\"]\nafter = true\nunless_slashed = true\nbetween = ' '\nvalue = '\\d{4}'",
                "only a label before its value takes unless_slashed",
            ),
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
            (
                "[[span]]\ncategory = \"ORG\"\nextend = ['(?P<lead>by )X Ltd']",
                "only a PERSON extend takes a `lead`",
            ),
            (
                "[[span]]\ncategory = \"PERSON\"\nextend = ['(?P<tail>x)y']",
                "is not `lead`",
            ),
            (
                "[[span]]\ncategory = \"PERSON\"\ntrim = [\"ms\"]\nunless_over = [\"part\"]",
                "unless_over needs extend",
            ),
            (
                "[[span]]\ncategory = \"PERSON\"\ntrim_start = ['(?P<name>Dr )x']",
                "is not `rest`",
            ),
            (
                "[[span]]\ncategory = \"BANK\"\nlist_letter = true",
                "only PERSON or ORG",
            ),
            ("[[span]]\ncategory = \"ADDRESS\"", "pattern"),
            (
                "[[span]]\ncategory = \"PERSON\"\nsplit = '<'",
                "need a pattern",
            ),
            (
                "[[span]]\ncategory = \"PERSON\"\npattern = 'x'\nsplit = '<*'",
                "empty piece",
            ),
            (
                "[[span]]\ncategory = \"PERSON\"\npattern = 'x'\nline_length = [5, 4]",
                "line_length",
            ),
            (
                "[[span]]\ncategory = \"PERSON\"\npattern = 'x'\nline_length = [0, 4]",
                "least 1",
            ),
            (
                "[[span]]\ncategory = \"PERSON\"\npattern = 'x'\nskip = 'X*'",
                "every span",
            ),
            (
                "[[span]]\ncategory = \"PERSON\"\nskip = 'X'",
                "need a pattern",
            ),
            (
                "[[span]]\ncategory = \"PERSON\"\npattern = '(?P<mark>a)x'",
                "is not `lead` or `tail`",
            ),
            (
                "[[span]]\ncategory = \"PERSON\"\npattern = '(?P<marker>a)x'\nline_length = [1, 4]",
                "or a `mark`",
            ),
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
                "[[reference]]\npattern = 'x'\ncategories = ['PERSON']",
                "categories needs when",
            ),
            (
                "[[reference]]\npattern = 'x'\nwhen = ['x']",
                "when needs categories",
            ),
            (
                "[[reference]]\npattern = 'x'\nwhen = ['x']\ncategories = ['PERSON']",
                "need a series group",
            ),
            (
                "[[reference]]\npattern = '(?P<series>C)'\nwhen = ['(?P<series>C)']\ncategories = ['NAME']",
                "unknown category",
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

    /// The names the review shows for `source` with the model's `entities`
    /// (`label` and text): what the machine-readable zone rules propose
    /// (#495).
    fn mrz_names_beside(source: &str, entities: &[(&str, &str)]) -> Vec<String> {
        let raw: Vec<crate::RawEntity> = entities
            .iter()
            .map(|(label, text)| {
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
            .filter(|span| span.category == "PERSON")
            .map(|span| source[span.range].to_owned())
            .collect()
    }

    /// The names the review shows for `source` without a model.
    fn mrz_names(source: &str) -> Vec<String> {
        mrz_names_beside(source, &[])
    }

    /// `head` padded with `<` fillers to `length` characters, as an MRZ
    /// line is.
    fn mrz_line(head: &str, length: usize) -> String {
        format!("{head}{}", "<".repeat(length - head.len()))
    }

    /// #495: each name part of a passport's machine-readable line is its
    /// own PERSON span, without the `<` fillers (fixture guide), a
    /// surname of several parts too; the second line stays unmarked.
    #[test]
    fn a_passports_machine_readable_line_proposes_each_name_part() {
        let russian = format!(
            "Машиночитаемая зона:\n{}\n7605318428RUS9104123F3102154<<<<<<<<<<<<<<06\n",
            mrz_line("P<RUSKRENC<<LIDIIA<ARSENEVNA", 44)
        );
        assert_eq!(mrz_names(&russian), ["KRENC", "LIDIIA", "ARSENEVNA"]);
        let foreign = format!(
            "{}\nX0042917<6NOR8807145M3307019<<<<<<<<<<<<<<02",
            mrz_line("P<NORVELLAMO<<TESSA<MARIT", 44)
        );
        assert_eq!(mrz_names(&foreign), ["VELLAMO", "TESSA", "MARIT"]);
        // Germany's "D<<", a surname of three parts, a diplomatic
        // passport's type letter, a United Nations laissez-passer, and no
        // given name at all.
        assert_eq!(
            mrz_names(&mrz_line("P<D<<VAN<DER<BRUGGE<<ORLA", 44)),
            ["VAN", "DER", "BRUGGE", "ORLA"]
        );
        assert_eq!(
            mrz_names(&mrz_line("PDFINHALVARD<<SIGNE", 44)),
            ["HALVARD", "SIGNE"]
        );
        assert_eq!(
            mrz_names(&mrz_line("P<UNOHALVARD<<SIGNE", 44)),
            ["HALVARD", "SIGNE"]
        );
        assert_eq!(mrz_names(&mrz_line("P<NOROSTRANDER<<", 44)), ["OSTRANDER"]);
        // A name too long for the line ends at its edge, with no filler.
        assert_eq!(
            mrz_names("P<NLDVANDERBRUGGEHALVERSTAM<<KORNELIA<ADRIANA"),
            ["VANDERBRUGGEHALVERSTAM", "KORNELIA", "ADRIANA"]
        );
        // Indented, with CRLF line ends.
        assert_eq!(
            mrz_names(&format!(
                "Page 2\r\n   {}  \r\nend",
                mrz_line("P<NORTARN<<EDA", 44)
            )),
            ["TARN", "EDA"]
        );
        // An extra filler between given names (OCR, or the issuer's).
        assert_eq!(
            mrz_names(&mrz_line("P<NORVELLAMO<<TESSA<<MARIT", 44)),
            ["VELLAMO", "TESSA", "MARIT"]
        );
    }

    /// #495: OCR loses or adds some fillers, or the type's; a line of 32 to
    /// 52 characters still reads as a passport's, as in hunt-ru-r7-05 (38),
    /// and a lost type leaves the state whole ("PRUSKRENC" is RUS, KRENC).
    #[test]
    fn a_machine_readable_line_shortened_or_lengthened_by_ocr_still_proposes() {
        assert_eq!(
            mrz_names("P<RUSKRENC<<LIDIIA<ARSENEVNA<<<<<<<<<<"),
            ["KRENC", "LIDIIA", "ARSENEVNA"]
        );
        assert_eq!(mrz_names(&mrz_line("P<NORTARN<<EDA", 50)), ["TARN", "EDA"]);
        assert!(mrz_names(&mrz_line("P<NORTARN<<EDA", 30)).is_empty());
        assert!(mrz_names(&mrz_line("P<NORTARN<<EDA", 54)).is_empty());
        assert_eq!(
            mrz_names(&mrz_line("PRUSKRENC<<LIDIIA<ARSENEVNA", 43)),
            ["KRENC", "LIDIIA", "ARSENEVNA"]
        );
        // A state OCR shortened is no state: nothing, rather than a cut
        // surname ("RENC").
        assert!(mrz_names(&mrz_line("P<RUKRENC<<LIDIIA<ARSENEVNA", 43)).is_empty());
    }

    /// #495: an ID card's names, on a TD2 card's first line (36
    /// characters) and on a TD1 card's third (30, under its two other
    /// lines, in a paragraph each too); the number and date lines stay
    /// unmarked.
    #[test]
    fn an_id_cards_machine_readable_zone_proposes_each_name_part() {
        assert_eq!(
            mrz_names(&format!(
                "{}\nD23145890<NOR8807145F3307019<<<<<<<4",
                mrz_line("I<NORHALDOR<<VESKA<INGRID", 36)
            )),
            ["HALDOR", "VESKA", "INGRID"]
        );
        let lines = [
            mrz_line("I<NORD231458907", 30),
            mrz_line("8807145F3307019NOR", 29) + "6",
            mrz_line("QUILLAN<<ODA<RENATE", 30),
        ];
        let td1 = format!("Оборот карты:\n{}\n", lines.join("\n"));
        assert_eq!(mrz_names(&td1), ["QUILLAN", "ODA", "RENATE"]);
        assert_eq!(mrz_names(&lines.join("\n\n")), ["QUILLAN", "ODA", "RENATE"]);
        assert_eq!(
            mrz_names(&format!("> {}", lines.join("\n> "))),
            ["QUILLAN", "ODA", "RENATE"]
        );
    }

    /// #495: a TD1 card's names need its two other lines, each of 26 to 34
    /// characters: the names line alone, at a card's length or a
    /// passport's, names no one ("PETROVA" would read as a head and cut
    /// "VA", which then marks "the VA hospital").
    #[test]
    fn a_td1_names_line_without_its_card_proposes_nothing() {
        for source in [
            mrz_line("QUILLAN<<ODA<RENATE", 30),
            format!(
                "{}\nThe VA hospital, VA form.",
                mrz_line("PETROVA<<ANNA", 32)
            ),
            mrz_line("CHRISTENSEN<<ANNA", 34),
            format!(
                "{}\n{}\n{}",
                mrz_line("I<NORD231458907", 38),
                mrz_line("8807145F3307019NOR", 29) + "6",
                mrz_line("QUILLAN<<ODA<RENATE", 30)
            ),
            format!(
                "{}\n{}\n{}",
                mrz_line("I<NORD231458907", 30),
                mrz_line("8807145F3307019NOR", 29) + "6",
                mrz_line("QUILLAN<<ODA<RENATE", 24)
            ),
        ] {
            assert!(mrz_names(&source).is_empty(), "{source}");
        }
    }

    /// #495: imported text writes a filler escaped (`\<`, anydoc's RTF
    /// and DOCX output) or as an entity, ends a line in a hard break and
    /// indents with no-break spaces; the names are the same.
    #[test]
    fn a_machine_readable_line_in_imported_text_proposes_each_name_part() {
        let line = mrz_line("P<RUSKRENC<<LIDIIA<ARSENEVNA", 44);
        for source in [
            // anydoc::to_markdown_bytes of an RTF passport copy, as is.
            "Passport copy\n\nP\\<RUSKRENC<\\<LIDIIA\\<ARSENEVNA<<<<<<<<<<<<<<<<<\\\n7605318428RUS9104123F3102154<<<<<<<<<<<<<<06\n".to_owned(),
            line.replace('<', "\\<"),
            line.replace('<', "&lt;"),
            format!("{line}\\\n7605318428RUS9104123F3102154<<<<<<<<<<<<<<06"),
            format!("\u{a0}\u{a0}{line}"),
        ] {
            assert_eq!(
                mrz_names(&source),
                ["KRENC", "LIDIIA", "ARSENEVNA"],
                "{source}"
            );
        }
    }

    /// #495: a machine-readable line in Markdown's structure or after a
    /// label is still one: a blockquote, a list item, a table cell, bold,
    /// inline code, "MRZ: ".
    #[test]
    fn a_machine_readable_line_in_markdown_or_after_a_label_proposes_each_name_part() {
        let line = mrz_line("P<RUSKRENC<<LIDIIA<ARSENEVNA", 44);
        for source in [
            format!("> {line}"),
            format!("- {line}"),
            format!("* {line}"),
            format!("1. {line}"),
            format!("| Field | Value |\n|---|---|\n| MRZ | {line} |"),
            format!("**{line}**"),
            format!("`{line}`"),
            format!("MRZ: {line}"),
            format!("Note: {line}"),
            format!("Нижняя строка: {line}"),
        ] {
            assert_eq!(
                mrz_names(&source),
                ["KRENC", "LIDIIA", "ARSENEVNA"],
                "{source}"
            );
        }
    }

    /// #495: the model's span across a filler ("KRENC<<LIDIIA"), which the
    /// review turns down for its `<`, takes none of the names the rule
    /// cuts from the line it is inside.
    #[test]
    fn a_model_span_across_fillers_leaves_the_machine_readable_names() {
        let source = format!(
            "Holder:\n{}\n",
            mrz_line("P<RUSKRENC<<LIDIIA<ARSENEVNA", 44)
        );
        for span in ["KRENC<<LIDIIA", "LIDIIA<ARSENEVNA", "RUSKRENC"] {
            assert_eq!(
                mrz_names_beside(&source, &[("full_name", span)]),
                ["KRENC", "LIDIIA", "ARSENEVNA"],
                "{span}"
            );
        }
    }

    /// #495: what only looks like a machine-readable line names no one: a
    /// lone `<<` in prose, HTML, code, a row of fillers, capitals without
    /// a state's head (which would cut "N" or "IFIED" off a word and mark
    /// each "N" in the text) or the `<<`, a short line, lower case, a digit
    /// in a name, a visa's line, a masked line, whose "XXXXX" would mark
    /// every mask.
    #[test]
    fn text_with_fillers_that_is_no_machine_readable_zone_proposes_nothing() {
        for source in [
            "The ratio is a << b, and KRENC<<LIDIIA is a typo.".to_owned(),
            "<div class=\"mrz\"><<KRENC<<LIDIIA<<</div>".to_owned(),
            "<p>P<RUSKRENC<<LIDIIA</p>".to_owned(),
            "```\nstd::cout << KRENC << LIDIIA;\ncat <<EOF\nPRINT<<EOF\n```".to_owned(),
            "<".repeat(44),
            mrz_line("P<", 44),
            "PARTIES TO THE AGREEMENT AND RECITALS HEREIN".to_owned(),
            "P<RUSKRENC LIDIIA ARSENEVNA AND OTHERS HEREIN".to_owned(),
            "PRUSSIA<<KING".to_owned(),
            "P<RUSKRENC<<LIDIIA".to_owned(),
            format!(
                "{}\nSection N applies; N = 5.",
                mrz_line("ACTION<<REQUIRED", 44)
            ),
            mrz_line("CLASSIFIED<<NOTICE", 40),
            mrz_line("PAGE<<TWO", 40),
            mrz_line("P<RUSKrenc<<Lidiia", 44),
            mrz_line("V<NORVELLAMO<<TESSA<MARIT", 44),
            mrz_line("P<NORVELL4MO<<TESSA<MARIT", 44),
            format!(
                "{}\nXXXXX and passport XXXXXX.",
                mrz_line("P<RUSXXXXX<<XXXXXX<XXXXXXXX", 44)
            ),
            "AXXXXXXXX<XRUSXXXXXXX<XXXXXXXXX<<<<<<<<<<<<<<".to_owned(),
        ] {
            assert!(mrz_names(&source).is_empty(), "{source}");
        }
    }

    /// #495: the zone rules' states are mdoc-pii's, which reads a
    /// surname's boundary after the same head.
    #[test]
    fn the_machine_readable_zone_rules_list_mdoc_piis_states() {
        let file: File = toml::from_str(FILES[0].1).unwrap();
        let mut expected: Vec<&str> = mdoc_pii::mrz::STATES.to_vec();
        expected.sort_unstable();
        let mut listed = 0;
        for entry in &file.span {
            let Some(states) = entry.parts.get("state") else {
                continue;
            };
            let states = states.replace("\\\\?<", "<");
            let mut states: Vec<&str> = states.split('|').collect();
            states.sort_unstable();
            assert_eq!(states, expected);
            listed += 1;
        }
        assert_eq!(listed, 2);
    }

    /// #563: ОГРН, ОГРНИП and ОКПО are TAX like ИНН, a public body's own
    /// too; the model's IDENTITY or BANK guess on one gives way, and with no
    /// guess the label proposes it. КБК and ОКТМО are budget codes and stay
    /// unmarked; the account and БИК stay BANK (guide rule 6, #484 item 6,
    /// #553 item 1).
    #[test]
    fn state_register_numbers_are_tax_numbers() {
        // The issue's reproduction; its ИНН differs by a digit, as
        // `shown` would find "9900000017" inside "1029900000017".
        let source = "ООО «Север» (ОГРН 1029900000017, ОКПО 99887766), ИП Ветлин Н. О. \
            (ОГРНИП 329990000000012); получатель: УФК по Ольховской области, ИНН \
            9901000017, БИК 049999999, р/с 03100643000000019900, КБК \
            18210102010011000110, ОКТМО 99701000.";
        let expected = pairs(&[
            ("ООО «Север»", "ORG"),
            ("1029900000017", "TAX"),
            ("99887766", "TAX"),
            ("Ветлин Н. О.", "PERSON"),
            ("329990000000012", "TAX"),
            ("9901000017", "TAX"),
            ("049999999", "BANK"),
            ("03100643000000019900", "BANK"),
        ]);
        let common = [
            ("ООО «Север»", "organization"),
            ("Ветлин Н. О.", "person"),
            ("9901000017", "tax_id"),
            ("049999999", "routing_number"),
            ("03100643000000019900", "bank_account"),
            ("18210102010011000110", "bank_account"),
            ("99701000", "tax_id"),
        ];
        for guesses in [
            [
                ("1029900000017", "national_id_number"),
                ("99887766", "bank_account"),
                ("329990000000012", "national_id_number"),
            ],
            [
                ("ОГРН 1029900000017", "bank_account"),
                ("ОКПО 99887766", "national_id_number"),
                ("329990000000012", "passport_number"),
            ],
            [
                ("1029900000017", "bank_account"),
                ("99887766", "passport_number"),
                ("ОГРНИП 329990000000012", "national_id_number"),
            ],
        ] {
            let raw: Vec<_> = common.iter().chain(&guesses).copied().collect();
            assert_eq!(shown(source, &raw), expected, "{guesses:?}");
        }
        // With no model span the labels still propose the numbers.
        assert_eq!(
            shown(source, &[]),
            pairs(&[
                ("1029900000017", "TAX"),
                ("99887766", "TAX"),
                ("Ветлин Н. О.", "PERSON"),
                ("329990000000012", "TAX"),
                ("9901000017", "TAX"),
            ])
        );
        // A valid ОГРН's structured rule says TAX too.
        let source = "ОГРН 1027700132195; OGRN 1027700132195";
        assert_eq!(
            shown(source, &[("1027700132195", "national_id_number")]),
            pairs(&[("1027700132195", "TAX"), ("1027700132195", "TAX")])
        );
    }

    /// #563: the labels' forms, and an invalid or damaged value of one
    /// digit fewer or more, which the guide marks too (rule 6).
    #[test]
    fn state_register_labels_take_their_forms() {
        for (source, values) in [
            ("ОГРН: 1027700132195.", &["1027700132195"][..]),
            ("ОГРНИП № 322213000038573", &["322213000038573"]),
            ("ОГРН ИП 322213000038573", &["322213000038573"]),
            ("ОГРН ИП: 322213000038573", &["322213000038573"]),
            ("(ОГРН:1027700132195)", &["1027700132195"]),
            ("**ОГРН:** 1027700132195", &["1027700132195"]),
            ("ОГРН (1029900000017)", &["1029900000017"]),
            (
                "основной государственный регистрационный номер (ОГРН) 1029900000017",
                &["1029900000017"],
            ),
            (
                "основной государственный регистрационный номер индивидуального \
                    предпринимателя 329990000000012",
                &["329990000000012"],
            ),
            (
                "за основным государственным регистрационным номером 1029900000017",
                &["1029900000017"],
            ),
            // English labels, any case.
            ("OGRN: 1029900000017", &["1029900000017"]),
            ("ogrnip 329990000000012", &["329990000000012"]),
            ("PSRN 1029900000017", &["1029900000017"]),
            ("ОКПО: 99887766", &["99887766"]),
            ("код ОКПО 9988776655", &["9988776655"]),
            ("код по ОКПО 99887766", &["99887766"]),
            // One digit fewer or more, and OCR's letter for a digit.
            ("ОГРН 102990000001", &["102990000001"]),
            ("ОГРН 10299000000170", &["10299000000170"]),
            ("ОГРНИП 3299900000000123", &["3299900000000123"]),
            ("ОГРН 1О29900000017", &["1О29900000017"]),
            ("ОКПО 9988776", &["9988776"]),
            ("ОКПО 998877665", &["998877665"]),
        ] {
            let tax: Vec<(&str, &str)> = values.iter().map(|value| (*value, "TAX")).collect();
            assert_eq!(shown(source, &[]), pairs(&tax), "{source}");
        }
    }

    /// #563: labels joined by a slash leave their run of values to the
    /// model, as before: a span over the whole run keeps its category and
    /// its range, spans over each value stay TAX, and with no span nothing
    /// is proposed. A budget code before "/ОКПО" is never ОКПО's value.
    #[test]
    fn state_register_slash_lists_stay_the_models() {
        for (source, run) in [
            (
                "ИНН/КПП/ОГРН 9901000017/990101001/1029900000017",
                "9901000017/990101001/1029900000017",
            ),
            (
                "ИНН / КПП / ОГРН 9901000017 / 990101001 / 1029900000017",
                "9901000017 / 990101001 / 1029900000017",
            ),
            (
                "ИНН/ОГРН 9901000017/1029900000017",
                "9901000017/1029900000017",
            ),
            (
                "ИНН/ОГРН: 9901000017/1029900000017",
                "9901000017/1029900000017",
            ),
            (
                "ОГРН/ИНН 1029900000017/9901000017",
                "1029900000017/9901000017",
            ),
            (
                "ОГРН/ИНН/КПП 1029900000017/9901000017/990101001",
                "1029900000017/9901000017/990101001",
            ),
            (
                "ОГРН/КПП 1029900000017/990101001",
                "1029900000017/990101001",
            ),
            ("ОГРН/ОКПО 1029900000017/99887766", "1029900000017/99887766"),
            ("ИНН/ОКПО 9901000017/99887766", "9901000017/99887766"),
            (
                "ИНН/КПП/ОКПО 9901000017/990101001/99887766",
                "9901000017/990101001/99887766",
            ),
            (
                "ИНН/ОГРНИП 990100001700/329990000000012",
                "990100001700/329990000000012",
            ),
        ] {
            assert_eq!(
                shown(source, &[(run, "tax_id")]),
                pairs(&[(run, "TAX")]),
                "{source}"
            );
            assert_eq!(shown(source, &[]), pairs(&[]), "{source}");
            let values: Vec<&str> = run.split('/').map(str::trim).collect();
            let raw: Vec<(&str, &str)> = values.iter().map(|value| (*value, "tax_id")).collect();
            let tax: Vec<(&str, &str)> = values.iter().map(|value| (*value, "TAX")).collect();
            assert_eq!(shown(source, &raw), pairs(&tax), "{source}");
        }
        let source = "ОКТМО/ОКПО 45382000/99887766";
        assert_eq!(shown(source, &[]), pairs(&[]));
        assert_eq!(
            shown(source, &[("99887766", "tax_id")]),
            pairs(&[("99887766", "TAX")])
        );
    }

    /// #563 guards: an ИНН stays TAX, a БИК and an account BANK; an amount
    /// or a date beside an ОГРН is no part of it (#600); a run too short or
    /// too long is no ОГРН or ОКПО; КБК, ОКТМО and other classifiers' codes
    /// stay unmarked, whatever the model guessed.
    #[test]
    fn state_register_labels_take_nothing_else() {
        let source = "ИНН 9901000017, ОГРН 1029900000017 от 12.03.2031, БИК 049999999, \
            р/с 03100643000000019900; ОГРН 1199900000011 500 руб.";
        let raw = [
            ("9901000017", "tax_id"),
            ("1029900000017 от 12.03.2031", "national_id_number"),
            ("049999999", "bank_account"),
            ("03100643000000019900", "bank_account"),
            ("1199900000011 500", "tax_id"),
        ];
        assert_eq!(
            shown(source, &raw),
            pairs(&[
                ("9901000017", "TAX"),
                ("1029900000017", "TAX"),
                ("049999999", "BANK"),
                ("03100643000000019900", "BANK"),
                ("1199900000011", "TAX"),
            ])
        );
        for source in [
            "ОГРН 10299000017",
            "ОГРН 10299000000170123",
            "ОКПО 998877",
            "ОКПО 998877665544",
            "ОГРН (при наличии) не указан",
            "номер 1029900000017",
        ] {
            assert_eq!(shown(source, &[]), pairs(&[]), "{source}");
        }
        let source = "КБК: 18210102010011000110, ОКТМО: 99701000, ОКАТО 99401000000, \
            ОКВЭД 47.11; (КБК 18210102010011000110, ОКТМО 99701000)";
        let raw = [
            ("18210102010011000110", "bank_account"),
            ("99701000", "tax_id"),
            ("99401000000", "tax_id"),
            ("47.11", "tax_id"),
            ("КБК 18210102010011000110", "bank_account"),
            ("ОКТМО 99701000", "national_id_number"),
        ];
        assert_eq!(shown(source, &raw), pairs(&[]));
    }
}
