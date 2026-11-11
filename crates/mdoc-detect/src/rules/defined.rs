//! Defined terms and generic words: a document's own terms («далее –
//! «Покупатель»», "the Distributor") and abbreviations are no company's
//! names unless a real name defines them.
use super::Pack;
use super::load::rust_classes;
use super::shapes::bare;
use mdoc_pii::Category;
use regex::Regex;
use std::sync::OnceLock;

/// English legal forms spelled out, lowercase: a company's name ends in
/// one ("Contract Corporation", "Acme Holdings Limited").
pub(super) const SPELLED_FORMS: &[&str] = &["corporation", "company", "incorporated", "limited"];

/// Russian words of a legal form spelled out, lowercase: «Общество с
/// ограниченной ответственностью», «Акционерное общество».
const SPELLED_FORMS_RU: &[&str] = &["ответственностью", "акционерное"];

/// The lowercase words of `text` as generic words are listed: `bare`, and
/// without a final period ("Def.'s" is "def").
pub(super) fn plain_words(text: &str) -> Vec<String> {
    text.split_whitespace()
        .map(|word| bare(word).trim_end_matches('.').to_lowercase())
        .filter(|word| !word.is_empty())
        .collect()
}

/// Words that join the generic words of a defined term ("Terms of
/// Business", "Покупатель и Продавец").
pub(super) const JOINERS: &[&str] = &["of", "and", "&", "the", "for", "in", "и", "для"];

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
pub(super) struct Definition<'a> {
    pub(super) term: &'a str,
    pub(super) subject: &'a str,
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
pub(super) fn definitions(text: &str) -> Vec<Definition<'_>> {
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

impl Pack {
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
    pub(super) fn unnamed_abbreviations(&self, text: &str) -> std::collections::HashSet<String> {
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
    pub(super) fn company_form(&self, text: &str) -> bool {
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
    pub(super) fn defined_terms(&self, text: &str) -> std::collections::HashSet<(usize, usize)> {
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

    /// The terms `text` defines, as their lowercase words ("sponsor
    /// group" of "(the “Sponsor Group”)").
    pub(super) fn defined_phrases(text: &str) -> std::collections::HashSet<Vec<String>> {
        definitions(text)
            .iter()
            .map(|definition| plain_words(definition.term))
            .collect()
    }

    /// Whether `text`, an organization or person span, is wholly generic
    /// words, one of them a term the document `defined` (#390):
    /// "Distributor", "the Sponsor’s", "RESELLER AGREEMENT", «Положение».
    /// A word that is not generic, a legal form included, makes it a name.
    /// So does a spelled-out legal form that ends it right after a word
    /// other than a joiner ("The Venue Company", "Purchaser Corporation"),
    /// unless the document defines that very term (`phrases`); "Group" is
    /// no legal form, so a defined party's group goes ("the Purchaser
    /// Group"), and so do joined parties ("Seller and the Company"). In a
    /// `person`'s span a stem's surname ending makes a name too
    /// («Подрядчиков», #450).
    pub(super) fn defined_generic(
        &self,
        text: &str,
        defined: &std::collections::HashSet<(usize, usize)>,
        phrases: &std::collections::HashSet<Vec<String>>,
        person: bool,
    ) -> bool {
        let mut known = false;
        let words = plain_words(text);
        let named = words
            .iter()
            .position(|word| !JOINERS.contains(&word.as_str()));
        if let (Some(first), [.., before, last]) = (named, words.as_slice())
            && words.len() - first > 1
            && !JOINERS.contains(&before.as_str())
            && SPELLED_FORMS.contains(&last.as_str())
            && !phrases.contains(&words[first..].to_vec())
        {
            return false;
        }
        !words.is_empty()
            && words.iter().all(|word| match self.defined_word(word) {
                Some((entry, at)) if person && self.person_surname(word, entry, at) => false,
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

    /// Whether lowercase `word`, the `at`th word of `defined` entry
    /// `entry`, has one of the entry's `person_not_endings` after its
    /// stem: a surname in a person's span («Подрядчиков», «Агентов»).
    fn person_surname(&self, word: &str, entry: usize, at: usize) -> bool {
        let defined = &self.defined[entry];
        word.strip_prefix(defined.list.words[at].as_str())
            .is_some_and(|ending| defined.person_not_endings.iter().any(|not| not == ending))
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
}
