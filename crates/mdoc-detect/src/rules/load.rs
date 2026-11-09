//! Reading and checking the pack: the TOML entries as written, and
//! `Pack::load`, which compiles and checks them, naming the file and entry
//! of the first problem.
use super::alternation::Alternation;
use super::context::{identifier_field, label_words};
use super::ocr::cyrillic_letter;
use super::spans::numbered;
use super::{
    ConditionalReference, Context, Defined, Label, Never, Pack, Role, Shape, Span, SpanFix, Veto,
};
use mdoc_pii::Category;
use regex::Regex;
use serde::Deserialize;
use std::sync::OnceLock;

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct File {
    /// Role words that are also surnames, kept when capitalized as PERSON.
    #[serde(default)]
    role_surnames: Vec<String>,
    /// Regexes for a role only a person has or a word that introduces one,
    /// after which lone bare initials are that person (#491).
    #[serde(default)]
    initials_after: Vec<String>,
    /// Regexes for a word that introduces a citizen («гражданин», «гр.»),
    /// after which only three letters are a person's initials: two are
    /// almost always a country («гражданин РА») (#491).
    #[serde(default)]
    initials_after_citizen: Vec<String>,
    /// Regexes for a party's role, which a company may hold too: bare
    /// initials after one are a person only where the model named them
    /// (#491).
    #[serde(default)]
    initials_after_party: Vec<String>,
    /// Legal forms, as written, that make bare initials beside them a
    /// company's name ("АО БАВ", #491).
    #[serde(default)]
    initials_companies: Vec<String>,
    /// Words in capitals, as written, that are never bare initials after a
    /// role (#491): abbreviations and short function words.
    #[serde(default)]
    initials_never: Vec<String>,
    /// Each a digit, then the letters OCR reads as it: a capital, a small
    /// letter or both ("1Il", "6б"). A model organization that reads as a
    /// person's name with them is a person (#377).
    #[serde(default)]
    ocr_letters: Vec<String>,
    /// Lowercase words that keep such an organization a company's, as read
    /// back ("legal", "банк").
    #[serde(default)]
    ocr_company_words: Vec<String>,
    /// Regexes for a read-back word, lowercase, that makes such a name a
    /// person's on its own: a patronymic ("ефимовна").
    #[serde(default)]
    ocr_person_words: Vec<String>,
    #[serde(default)]
    never: Vec<NeverEntry>,
    #[serde(default)]
    defined: Vec<DefinedEntry>,
    #[serde(default)]
    context: Vec<ContextEntry>,
    #[serde(default)]
    shape: Vec<ShapeEntry>,
    #[serde(default)]
    pub(super) span: Vec<SpanEntry>,
    #[serde(default)]
    variants: Vec<VariantsEntry>,
    #[serde(default)]
    reference: Vec<ReferenceEntry>,
    #[serde(default)]
    url: Vec<UrlEntry>,
    #[serde(default)]
    private: Vec<PrivateEntry>,
    #[serde(default)]
    label: Vec<LabelEntry>,
    #[serde(default)]
    lookalike: Vec<LookalikeEntry>,
}

/// Letters of another script that read as Cyrillic ones, and the common
/// words they spell in Russian text (#549): a model's name written so
/// («Ha» for «На», «T.π.» for «т.п.») is no name.
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct LookalikeEntry {
    /// Pairs of two letters: the look-alike, then its Cyrillic letter.
    #[serde(default)]
    letters: Vec<String>,
    /// Lowercase Cyrillic words, abbreviations' inner dots kept.
    #[serde(default)]
    words: Vec<String>,
    /// Lowercase Cyrillic stems of a person's labels and roles («фамили»,
    /// «свидетел»), up to four letters of ending: a span after one stays.
    #[serde(default)]
    unless_after: Vec<String>,
}

/// An identifier field's labels (#739): a value just after one, in a wider
/// model span beside a rule's value, is a value of its own, of `category`.
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct LabelEntry {
    category: String,
    words: Vec<String>,
    /// The words name the category only for a value of its strict shape,
    /// as if unlabelled: "счёт" before an invoice's number is none.
    #[serde(default)]
    strict: bool,
    /// Words one of which must come earlier in the field for the label to
    /// count ("паспорт … серии"); alone the label is `strict`, after other
    /// words ("свидетельство серии") it names nothing.
    #[serde(default)]
    only_after: Vec<String>,
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
    /// For a pattern entry with PERSON alone in `anywhere_in`: the span
    /// stays a person where the document names its last word, another
    /// person span with it beside a name or a title before it ("Mary Ann
    /// Project", then "Ms Project").
    #[serde(default)]
    unless_last_named: bool,
}

#[derive(Clone, Copy, Deserialize, PartialEq, Eq, Hash)]
#[serde(rename_all = "lowercase")]
pub(super) enum Form {
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
pub(super) struct SpanEntry {
    category: String,
    /// What a whole span looks like; `{name}` stands for a part. An
    /// entry without one fixes model spans' boundaries instead.
    #[serde(default)]
    pattern: Option<Pattern>,
    /// Named pieces of the pattern, each a regex.
    #[serde(default)]
    pub(super) parts: std::collections::BTreeMap<String, String>,
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
    /// A match that a span of its category already overlaps (the model's,
    /// a name cell's) is not proposed: the pattern only fills gaps.
    #[serde(default)]
    gaps_only: bool,
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
    /// For PERSON: lowercase words or phrases that, ending a span, make
    /// it a company's ("Pellam airlines", "Pellam Bank").
    #[serde(default)]
    org_before: Vec<String>,
    /// For PERSON: lowercase phrases right before a span that make it a
    /// company's ("the team known as the Brindle Harriers").
    #[serde(default)]
    org_after: Vec<String>,
    /// A lowercase list marker at a line or list break is no initial.
    #[serde(default)]
    list_letter: bool,
    /// With `extend`: lowercase words a span never grows over.
    #[serde(default)]
    unless_over: Vec<String>,
    /// For ORG: the legal forms that may stand before a name in nested
    /// quotes («…«…»», "…"…""), each a regex. A span in such a name grows
    /// to where its quotes balance (`SpanFix::nested_names`).
    #[serde(default)]
    nested_quotes: Vec<String>,
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
    pattern: Pattern,
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

/// A `span` or `reference` pattern: one regex, or a list of them that is
/// one alternation of its items in order (`Alternation`), each compiled
/// alone, for a pattern that would come near the regex size limit as one.
/// The parts are every item's.
#[derive(Deserialize)]
#[serde(untagged)]
enum Pattern {
    One(String),
    Any(Vec<String>),
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct VariantsEntry {
    name: String,
    forms: Vec<String>,
}

impl Pack {
    /// Parses and checks `files`, naming the file and entry of the first
    /// problem.
    pub(super) fn load(files: &[(&str, &str)]) -> Result<Self, String> {
        let mut pack = Self {
            role_surnames: Vec::new(),
            initials_after: Vec::new(),
            initials_companies: Vec::new(),
            initials_never: Vec::new(),
            ocr_letters: Vec::new(),
            ocr_company_words: std::collections::HashSet::new(),
            ocr_person_words: Vec::new(),
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
            labels: Vec::new(),
            lookalike_letters: std::collections::HashMap::new(),
            lookalike_words: std::collections::HashSet::new(),
            lookalike_after: Vec::new(),
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
            for (key, patterns, person, three) in [
                ("initials_after", &file.initials_after, true, false),
                (
                    "initials_after_citizen",
                    &file.initials_after_citizen,
                    true,
                    true,
                ),
                (
                    "initials_after_party",
                    &file.initials_after_party,
                    false,
                    false,
                ),
            ] {
                for (index, pattern) in patterns.iter().enumerate() {
                    let at = |problem: String| format!("{name}: {key} #{}: {problem}", index + 1);
                    // The role is a whole word, then emphasis and spaces up
                    // to the initials: "**Потерпевший** КЛМ".
                    let anchored = format!(
                        r"(?:^|[^\p{{Alphabetic}}\p{{N}}])(?P<role>{pattern})[*_]{{0,2}}[\p{{Zs}}\t]+[*_]{{0,2}}$"
                    );
                    let regex =
                        Regex::new(&rust_classes(&anchored)).map_err(|e| at(e.to_string()))?;
                    if regex.is_match(" ") {
                        return Err(at("matches no role".into()));
                    }
                    let role = Role {
                        start: 0,
                        person,
                        three,
                    };
                    pack.initials_after.push((regex, role));
                }
            }
            for (key, words) in [
                ("initials_companies", &file.initials_companies),
                ("initials_never", &file.initials_never),
            ] {
                if words
                    .iter()
                    .any(|word| word.is_empty() || !word.chars().all(char::is_uppercase))
                {
                    return Err(format!("{name}: {key} needs words in capitals"));
                }
            }
            pack.initials_companies.extend(file.initials_companies);
            pack.initials_never.extend(file.initials_never);
            for entry in &file.ocr_letters {
                let mut chars = entry.chars();
                let digit = chars.next().filter(char::is_ascii_digit);
                let letters: Vec<char> = chars.collect();
                let letters_fit = (1..=2).contains(&letters.len())
                    && letters.iter().all(|c| c.is_alphabetic())
                    && letters.iter().filter(|c| c.is_uppercase()).count() <= 1
                    && letters.iter().filter(|c| c.is_lowercase()).count() <= 1;
                let Some(digit) = digit.filter(|_| letters_fit) else {
                    return Err(format!(
                        "{name}: ocr_letters {entry:?} needs a digit, then a capital, a small letter or both"
                    ));
                };
                pack.ocr_letters.push((digit, letters));
            }
            if file.ocr_company_words.iter().any(|word| {
                word.is_empty()
                    || *word != word.to_lowercase()
                    || !word.chars().all(char::is_alphabetic)
            }) {
                return Err(format!("{name}: ocr_company_words needs lowercase words"));
            }
            pack.ocr_company_words.extend(file.ocr_company_words);
            for pattern in &file.ocr_person_words {
                let regex = Regex::new(&rust_classes(&format!("^(?:{pattern})$")))
                    .map_err(|e| format!("{name}: ocr_person_words {pattern:?}: {e}"))?;
                pack.ocr_person_words.push(regex);
            }
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
                if entry.unless_last_named
                    && (entry.form != Form::Pattern || entry.anywhere_in != ["PERSON"])
                {
                    return Err(at(
                        "unless_last_named needs a pattern with anywhere_in PERSON alone".into(),
                    ));
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
                    unless_last_named: entry.unless_last_named,
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
                        unless_last_named: false,
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
            for (index, entry) in file.lookalike.into_iter().enumerate() {
                let at = |problem: &str| format!("{name}: lookalike #{}: {problem}", index + 1);
                if entry.letters.is_empty()
                    && entry.words.is_empty()
                    && entry.unless_after.is_empty()
                {
                    return Err(at("needs letters, words or unless_after"));
                }
                if entry.unless_after.iter().any(|stem| {
                    stem.is_empty()
                        || !stem
                            .chars()
                            .all(|c| cyrillic_letter(c) && !c.is_uppercase())
                }) {
                    return Err(at("unless_after needs lowercase Cyrillic stems"));
                }
                pack.lookalike_after.extend(entry.unless_after);
                for pair in &entry.letters {
                    let (look, cyrillic) = match pair.chars().collect::<Vec<_>>()[..] {
                        [look, cyrillic] => (look, cyrillic),
                        _ => return Err(at(&format!("{pair:?} is no pair of two letters"))),
                    };
                    if !look.is_alphabetic() || cyrillic_letter(look) || !cyrillic_letter(cyrillic)
                    {
                        return Err(at(&format!(
                            "{pair:?} needs another script's letter, then a Cyrillic one"
                        )));
                    }
                    if pack
                        .lookalike_letters
                        .insert(look, cyrillic)
                        .is_some_and(|before| before != cyrillic)
                    {
                        return Err(at(&format!("{look:?} reads as two letters")));
                    }
                }
                for word in entry.words {
                    let letters = word.split('.').all(|part| {
                        !part.is_empty()
                            && part
                                .chars()
                                .all(|c| cyrillic_letter(c) && !c.is_uppercase())
                    });
                    if !letters {
                        return Err(at(&format!(
                            "{word:?} needs lowercase Cyrillic letters, dots only inside"
                        )));
                    }
                    pack.lookalike_words.insert(word);
                }
            }
            for (index, entry) in file.label.into_iter().enumerate() {
                let at = |problem: String| format!("{name}: label #{}: {problem}", index + 1);
                let category = category(&entry.category).map_err(at)?;
                if !identifier_field(category) {
                    return Err(at("needs TAX, BANK, IDENTITY or PHONE".into()));
                }
                if entry.words.is_empty() {
                    return Err(at("needs words".into()));
                }
                if let Some(word) = entry.words.iter().chain(&entry.only_after).find(|word| {
                    word.is_empty() || **word != word.to_lowercase() || *word != &label_words(word)
                }) {
                    return Err(at(format!(
                        "{word:?} needs lowercase words, single-spaced, without dots"
                    )));
                }
                if let Some(word) = entry.words.iter().find(|word| {
                    pack.labels
                        .iter()
                        .any(|label| label.words == **word && label.category != category)
                }) {
                    return Err(at(format!("{word:?} is another category's label too")));
                }
                pack.labels
                    .extend(entry.words.into_iter().map(|words| Label {
                        words,
                        category,
                        strict: entry.strict,
                        only_after: entry.only_after.clone(),
                    }));
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
                    || !entry.org_before.is_empty()
                    || !entry.org_after.is_empty()
                    || entry.list_letter
                    || !entry.unless_over.is_empty()
                    || !entry.nested_quotes.is_empty();
                if let Some(pattern) = &entry.pattern {
                    if fixes {
                        return Err(at(
                            "a pattern takes no trim, trim_start, extend, strip_end, person_before, org_before, org_after, list_letter, unless_over or nested_quotes".into(),
                        ));
                    }
                    // A sized pattern's `mark` groups are what its lines'
                    // lengths leave out.
                    let groups: &[&str] = if entry.line_length.is_some() {
                        &["lead", "tail", "mark"]
                    } else {
                        &["lead", "tail"]
                    };
                    let pattern = alternation(pattern, &entry.parts, groups, false).map_err(at)?;
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
                        gaps_only: entry.gaps_only,
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
                    || entry.gaps_only
                {
                    return Err(at(
                        "parts, absorbs, continuations, split, line_length, skip and gaps_only need a pattern"
                            .into(),
                    ));
                }
                if !fixes {
                    return Err(at(
                        "needs a pattern, trim words, trim_start, extend patterns, strip_end, person_before, org_before or org_after"
                            .into(),
                    ));
                }
                if !entry.person_before.is_empty() && category != Category::Organization {
                    return Err(at("only an ORG fix takes person_before".into()));
                }
                if !entry.nested_quotes.is_empty() && category != Category::Organization {
                    return Err(at("only an ORG fix takes nested_quotes".into()));
                }
                if (!entry.org_before.is_empty() || !entry.org_after.is_empty())
                    && category != Category::Person
                {
                    return Err(at("only a PERSON fix takes org_before or org_after".into()));
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
                for word in entry
                    .trim
                    .iter()
                    .chain(&entry.org_before)
                    .chain(&entry.org_after)
                {
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
                    .chain(&entry.org_before)
                    .chain(&entry.org_after)
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
                        match regex
                            .capture_names()
                            .flatten()
                            .find(|g| *g != "lead" && *g != "tail")
                        {
                            Some(group) => Err(at(format!(
                                "extend group {group:?} is not `lead` or `tail`"
                            ))),
                            // Only a person's growth reads a lead or a tail:
                            // the ORG checks (`grows_to_company`) take whole
                            // matches.
                            None if category != Category::Person
                                && regex.capture_names().flatten().next().is_some() =>
                            {
                                Err(at("only a PERSON extend takes a `lead` or a `tail`".into()))
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
                // Each form, ending where the outer quote opens (spaces
                // between), and as a whole word anywhere.
                let nested_quotes = entry
                    .nested_quotes
                    .iter()
                    .map(|pattern| {
                        let regex = |end: &str| {
                            Regex::new(&rust_classes(&format!("(?:{pattern}){end}")))
                                .map_err(|e| at(e.to_string()))
                        };
                        let before = regex(r"[\p{Zs}\t]*\z")?;
                        if before.is_match("") {
                            return Err(at("nested_quotes matches an empty form".into()));
                        }
                        if before.capture_names().flatten().next().is_some() {
                            return Err(at("nested_quotes takes no groups".into()));
                        }
                        Ok((before, regex(r"\b")?))
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
                    org_before: entry.org_before.clone(),
                    org_after: entry.org_after.clone(),
                    list_letter: entry.list_letter,
                    unless_over: entry.unless_over.clone(),
                    nested_quotes,
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
                    let Pattern::One(pattern) = &entry.pattern else {
                        return Err(at("when takes one pattern, no list".into()));
                    };
                    let pattern = with_groups(
                        pattern,
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
                let pattern = alternation(
                    &entry.pattern,
                    &entry.parts,
                    &["lead", "tail", "docket"],
                    true,
                )
                .map_err(at)?;
                let cited = pattern.capture_names().flatten().any(is_party);
                if pattern
                    .0
                    .iter()
                    .any(|form| form.capture_names().flatten().any(is_party) != cited)
                {
                    return Err(at(
                        "a list's forms all name a cited case's parties, or none does".into(),
                    ));
                }
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

/// Whether a reference's group names a cited case's party: `party`,
/// `party2`, ….
pub(super) fn is_party(group: &str) -> bool {
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

/// [`with_groups`] for a `pattern` that may be a list: each part is used by
/// some item and replaced in every item, and each item is compiled and
/// checked alone.
fn alternation(
    pattern: &Pattern,
    parts: &std::collections::BTreeMap<String, String>,
    groups: &[&str],
    parties: bool,
) -> Result<Alternation, String> {
    let mut items = match pattern {
        Pattern::One(pattern) => {
            return with_groups(pattern, parts, groups, parties).map(|one| Alternation(vec![one]));
        }
        Pattern::Any(items) if items.len() < 2 => {
            return Err("a list of patterns has two at least".into());
        }
        Pattern::Any(items) => items.clone(),
    };
    for (part, regex) in parts {
        let name = format!("{{{part}}}");
        if !items.iter().any(|item| item.contains(&name)) {
            return Err(format!("part {part:?} is never used"));
        }
        for item in &mut items {
            *item = item.replace(&name, &format!("(?:{regex})"));
        }
    }
    items
        .iter()
        .map(|item| with_groups(item, &Default::default(), groups, parties))
        .collect::<Result<_, _>>()
        .map(Alternation)
}

/// `pattern` with `\p{Uppercase}`, `\p{Alphabetic}` and `\p{N}` meaning
/// exactly Rust's `char::is_uppercase`, `is_alphabetic` and `is_numeric`:
/// `regex`'s Unicode tables can be a version behind the compiler's, and the
/// rest of detection splits and trims words with Rust's.
pub(super) fn rust_classes(pattern: &str) -> String {
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
pub(super) fn category(token: &str) -> Result<Category, String> {
    Category::ALL
        .into_iter()
        .find(|category| category.token() == token)
        .ok_or_else(|| format!("unknown category {token:?}"))
}
