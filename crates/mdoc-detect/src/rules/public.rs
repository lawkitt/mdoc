//! Courts, public bodies and public sites: a span that is one, or is cut
//! out of one, is no name, and a public site's address is no web address.
use super::load::{Form, rust_classes};
use super::shapes::bare;
use super::{Never, Pack};
use mdoc_pii::Category;
use regex::Regex;
use std::sync::OnceLock;

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

impl Pack {
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

    /// Whether a span is a court or other public body by the `never`
    /// section's `anywhere_in` words. A legal form makes it a company,
    /// whatever else it says ("Garden Court Ltd"), and a word in quotes is
    /// a name ("Банк «Город»"). The correction stage asks this only after
    /// the span fixes, so a company's inner name ("Северная область Строй")
    /// first grows to its legal form.
    pub(super) fn public_body(&self, text: &str, category: Category, titled: bool) -> bool {
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
                    entry.anywhere_in.contains(&category)
                        && !(titled && entry.unless_titled)
                        && !entry.unless_last_named
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
    pub(super) fn cut_from_public_body(
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
        if self.head_after_public_name(span, after, category, titled) {
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

    /// Whether a span of capitalized words is a local body's name cut
    /// before its head, read with the whole run of capitalized words after
    /// it on the line (#788): "⟦Elko⟧ County Sheriff's Office", "⟦Ashby⟧
    /// Police Department". The run must open with a local body's head and
    /// be a public body by the `never` patterns as a whole, so "Harlow
    /// County Bank" keeps its "Harlow".
    fn head_after_public_name(
        &self,
        span: &str,
        after: &str,
        category: Category,
        titled: bool,
    ) -> bool {
        static HEAD: OnceLock<Regex> = OnceLock::new();
        static RUN: OnceLock<Regex> = OnceLock::new();
        let head = HEAD.get_or_init(|| {
            Regex::new(r"^[\p{Zs}\t]+(?:County|Parish|Borough|Township|Police|Sheriff['’]s|Fire[\p{Zs}\t]+Department|(?:City|Town|Borough|District|Parish)[\p{Zs}\t]+Council|(?:Housing|Transit)[\p{Zs}\t]+Authority|(?:School|Water)[\p{Zs}\t]+District)\b")
                .expect("local body head pattern")
        });
        let run = RUN.get_or_init(|| {
            Regex::new(r"^(?:[\p{Zs}\t]+(?:\p{Uppercase}[\p{Alphabetic}'’.-]*|of|and|&|the)){1,8}")
                .expect("capitalized run pattern")
        });
        let words: Vec<&str> = span.split_whitespace().collect();
        if words.is_empty()
            || words.len() > 3
            || self.legal_form(span)
            || !words.iter().all(|word| {
                word.chars().next().is_some_and(char::is_uppercase)
                    && word.chars().all(|c| c.is_alphabetic() || c == '-')
            })
            || !head.is_match(after)
        {
            return false;
        }
        let Some(found) = run.find(after) else {
            return false;
        };
        let mut tail = found.as_str();
        while let Some(rest) = ["of", "and", "&", "the"].iter().find_map(|joiner| {
            tail.strip_suffix(joiner)
                .filter(|rest| rest.ends_with([' ', '\t']))
        }) {
            tail = rest.trim_end();
        }
        self.public_body_by(&format!("{span}{tail}"), category, titled, false)
    }
}

/// `text` without what it quotes in closed pairs («…», "…", “…”), «» nested:
/// "ГКУ «Дирекция района»" is "ГКУ ". An unclosed quote stays as it is.
pub(super) fn unquoted(text: &str) -> String {
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
