//! `never` words and `shape`s: whether a model span looks like its
//! category and is no role, common word or public body, and what a manual
//! selection is guessed to be.
use super::Pack;
use super::spans::span_ranges;
use mdoc_pii::Category;
use regex::Regex;

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
pub(super) fn bare(word: &str) -> &str {
    let word = word.trim_matches(|c: char| !c.is_alphanumeric() && c != '-');
    ["'s", "’s", "'S", "’S"]
        .iter()
        .find_map(|possessive| word.strip_suffix(possessive))
        .filter(|rest| !rest.is_empty())
        .unwrap_or(word)
}

impl Pack {
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
    pub(super) fn common(&self, word: &str) -> bool {
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
    pub(super) fn identifying_named(
        &self,
        text: &str,
        category: Category,
        named: &dyn Fn(&str) -> bool,
        titled: bool,
    ) -> bool {
        self.identifying_named_after(text, category, named, titled, "")
    }

    pub(super) fn identifying_named_after(
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

    /// Whether a model span has its category's `shape` and is not a role
    /// phrase by the `never` section; public bodies are not checked.
    /// `named` says which common words (lowercase) context makes a person's
    /// name.
    pub(super) fn shaped_named(
        &self,
        text: &str,
        category: Category,
        named: &dyn Fn(&str) -> bool,
    ) -> bool {
        self.shaped_named_after(text, category, named, "")
    }

    pub(super) fn shaped_named_after(
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
                    || self.names(
                        text,
                        words().filter(|w| pattern.is_match(w)),
                        named,
                        category == Category::Person,
                    )
            })
    }

    /// The shape checks a label lifts: `min_code_chars` across all groups
    /// of a value, `lowercase_at_most_digits` and `forbid_unlabelled`,
    /// with the same Unicode predicates as shape regexes.
    pub(super) fn unlabelled_shape(&self, text: &str, category: Category) -> bool {
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
    pub(super) fn role_person_name(&self, text: &str) -> bool {
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
    pub(super) fn determined_role(&self, source: &str, range: std::ops::Range<usize>) -> bool {
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
    /// word is an outline letter: "B. General Terms". For a `person`, a
    /// role or heading modifier before a common word names no one:
    /// "General Program" and "Lead Plan" do not, nor do they make
    /// "Program" or "Plan" a name elsewhere (#500). Alone or after a
    /// common word it may be a name: "General", "B. General", "Christian
    /// Head", "German Chief". A company keeps one ("General Accident").
    fn names<'a>(
        &self,
        text: &str,
        words: impl Iterator<Item = &'a str>,
        named: &dyn Fn(&str) -> bool,
        person: bool,
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
        let (mut name, mut letters, mut common, mut weak) = (false, 0, false, false);
        for (index, word) in words.enumerate() {
            let phrase = self
                .never
                .iter()
                .any(|entry| entry.phrase_only && entry.has(&word.to_lowercase()));
            let modifier = role_head && phrase;
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
                } else if person && phrase && !common {
                    weak = true;
                } else {
                    name = true;
                }
            }
        }
        name || letters > 1 || (letters == 1 || weak) && !common
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
        if matches!(category, Category::Person | Category::Organization)
            && self.lookalike_word(source, range.clone(), text)
        {
            return false;
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
