//! Organizations: legal forms, company nouns and short names, and names
//! in nested quotes.
use super::defined::{JOINERS, SPELLED_FORMS, definitions, plain_words};
use super::references::soft_break;
use super::shapes::bare;
use super::{Pack, Span};
use mdoc_pii::{Category, Detection, Recognizer};
use regex::Regex;

/// Generic business words, lowercase: after one other word in a company's
/// name they make that word its short name ("Pellam" of "Pellam Bank",
/// "Quillon" of "Quillon Freight Corp"; #548). Not "Motor", "Controls" or
/// "Law": "Ford", "Johnson" and "Stewarts" may be people's names there.
const BUSINESS_WORDS: &[&str] = &[
    "resources",
    "maritime",
    "freight",
    "holdings",
    "group",
    "shipping",
    "bank",
    "airline",
    "airlines",
    "capital",
    "industries",
    "logistics",
    "energy",
    "mining",
];

/// Russian party roles, lowercase: a name before one in a caption is a
/// party's, a person as often as a company ("ИВАНОВ, истец").
const RU_PARTY_ROLES: &[&str] = &["истец", "ответчик", "заявитель", "истца", "ответчика"];

impl Pack {
    /// The `org_before` and `org_after` phrases of the PERSON fixes.
    fn org_phrases(&self) -> (Vec<&String>, Vec<&String>) {
        let fixes = || {
            self.spans.iter().filter_map(|span| match span {
                Span::Fix(fix) if fix.category == Category::Person => Some(fix),
                _ => None,
            })
        };
        (
            fixes().flat_map(|fix| &fix.org_before).collect(),
            fixes().flat_map(|fix| &fix.org_after).collect(),
        )
    }

    /// Where the name ends in `text`, a span that is a capitalized name
    /// and an `org_before` noun written lowercase ("Pellam airlines",
    /// "Pellam air fares"): a company's name with a common noun after it,
    /// which stays outside (guide rule 2, #548). `None` for anything else:
    /// a capitalized noun ("Pellam Airlines"), or a lowercase word or a
    /// joiner before the noun ("Pellam College of airlines").
    pub(crate) fn org_noun_name(&self, text: &str) -> Option<usize> {
        let (nouns, _) = self.org_phrases();
        let words: Vec<(usize, &str)> = text
            .split_whitespace()
            .map(|word| (word.as_ptr() as usize - text.as_ptr() as usize, word))
            .collect();
        let at = nouns.iter().find_map(|noun| {
            let noun: Vec<&str> = noun.split(' ').collect();
            (words.len() > noun.len()
                && words[words.len() - noun.len()..]
                    .iter()
                    .zip(&noun)
                    .all(|((_, word), noun)| bare(word).trim_end_matches('.') == *noun))
            .then(|| words.len() - noun.len())
        })?;
        let (last, word) = words[at - 1];
        let before = bare(word);
        if !before.starts_with(char::is_uppercase)
            || JOINERS.contains(&before.to_lowercase().as_str())
        {
            return None;
        }
        let end = last + word.trim_end_matches([',', ';', ':']).len();
        text.trim_start()
            .starts_with(char::is_uppercase)
            .then_some(end)
    }

    /// A model person's span that is one capitalized word and an
    /// `org_before` noun, or a name right after an `org_after` phrase that
    /// ends there, is a company's (guide rule 2, #548): "Pellam airlines"
    /// is "Pellam" with a common noun after it, "Pellam Airlines" the
    /// company's whole name, and "Brindle Harriers" in "the team known as
    /// the Brindle Harriers." a team. Never after a person's title, nor two
    /// words before a noun ("Ada Pellam airlines tickets" is a person's),
    /// nor a name a lowercase word carries on ("the team known as the Ada
    /// Pellam fan club"). Returns the one-word names: each is a short name
    /// elsewhere in the document ([`Self::company_short_names`]).
    pub(super) fn company_nouns(
        &self,
        source: &str,
        view: &mdoc_pii::view::View,
        detections: &mut [Detection],
    ) -> Vec<Vec<String>> {
        let (before, after) = self.org_phrases();
        if before.is_empty() && after.is_empty() {
            return Vec::new();
        }
        let text = &view.text;
        let mut named = Vec::new();
        for detection in detections
            .iter_mut()
            .filter(|d| d.category == Category::Person && d.recognizer == Recognizer::Model)
        {
            if self.titled(source, detection.range.start) {
                continue;
            }
            let seen = view.view_range(detection.range.clone());
            let span = &text[seen.clone()];
            // Each word with its offset in the span.
            let words: Vec<(usize, &str)> = span
                .split_whitespace()
                .map(|word| (word.as_ptr() as usize - span.as_ptr() as usize, word))
                .collect();
            let lower: Vec<String> = words
                .iter()
                .map(|(_, word)| bare(word).trim_end_matches('.').to_lowercase())
                .collect();
            // One name word, then the noun.
            let noun = before.iter().any(|phrase| {
                let phrase: Vec<&str> = phrase.split(' ').collect();
                lower.len() == phrase.len() + 1 && lower[1..] == phrase[..]
            });
            let after_phrase = || {
                let before_span: Vec<String> = text[..seen.start]
                    .split_whitespace()
                    .rev()
                    .take(5)
                    .map(str::to_lowercase)
                    .collect();
                // The team's name ends with the span: "(", ",", "." or
                // the line's end, never "fan club".
                let ends = !text[seen.end..]
                    .trim_start_matches([' ', '\t'])
                    .starts_with(char::is_alphanumeric);
                ends && after.iter().any(|phrase| {
                    let phrase: Vec<&str> = phrase.split(' ').rev().collect();
                    before_span.len() >= phrase.len()
                        && before_span.iter().zip(&phrase).all(|(a, b)| a == b)
                })
            };
            let name = if noun {
                match self.org_noun_name(span) {
                    // A lowercase noun stays outside: "⟦ORG|Pellam⟧ airlines".
                    Some(end) => 0..end,
                    None if words[1].1.starts_with(char::is_uppercase) => 0..span.len(),
                    None => continue,
                }
            } else if after_phrase() {
                // "the team known as the Brindle Harriers" read as "the
                // Brindle Harriers": the article stays outside.
                let start = match words.first() {
                    Some((_, "the")) if words.len() > 1 => words[1].0,
                    _ => 0,
                };
                start..span.len()
            } else {
                continue;
            };
            if !span[name.clone()].starts_with(char::is_uppercase) {
                continue;
            }
            let words = plain_words(&span[name.clone()]);
            if words.len() == 1 {
                named.push(words);
            }
            detection.range = view.source_range(seen.start + name.start..seen.start + name.end);
            detection.category = Category::Organization;
        }
        named
    }

    /// A model person whose words are a company's short name in the same
    /// document is that company (guide rule 2, #506 items 10 and 11, #548),
    /// a possessive included ("Pellam’s"), only on strong evidence:
    /// - a name the document defines for a company right after it, sharing
    ///   a word with it ("Quillon Freight Corp. (“Quillon”)"; a generic
    ///   term such as “the Company” shares none);
    /// - a company's name before its legal form ("Quillon Freight" of
    ///   "Quillon Freight Corp"), unless a person's span has those words
    ///   ("Ada Pellam" of "Ada Pellam LLC");
    /// - its first word where the rest is a [`BUSINESS_WORDS`] word and no
    ///   joiner ("Pellam" of "Pellam Bank", "Quillon" of "Quillon Freight
    ///   Corp"; not "Davis" of "Davis Polk & Wardwell LLP", "Ford" of "Ford
    ///   Motor Co", nor "Ada" of "Ada Pellam LLC");
    /// - a company's whole name of two words or more in any letter case
    ///   where that mention has a legal form, a definition, an `org_before`
    ///   noun or an `org_after` phrase beside it ("Brindle Harriers" beside
    ///   "BRINDLE HARRIERS LLC"), never one a party role follows ("ADA
    ///   PELLAM, Plaintiff");
    /// - a one-word company's name before an `org_before` noun ("Pellam"
    ///   of "Pellam airlines"), or one [`Self::company_nouns`] `named`.
    ///
    /// A company in a cited precedent gives no short name; «ИП» is no legal
    /// form here (guide rule 1). Never where the words are a person's too:
    /// after a title ("Mr Pellam"), or in a longer person's span beside
    /// another name ("Ada Pellam", "John Smith" beside "Smith LLP"), which
    /// a title always makes a person's; then each span of them stays as
    /// the model gave it. Nor bare capitals of three letters or fewer no
    /// definition names, which may be a person's initials (#387).
    pub(super) fn company_short_names(
        &self,
        source: &str,
        view: &mdoc_pii::view::View,
        detections: &mut [Detection],
        named: &[Vec<String>],
    ) {
        if !detections.iter().any(|d| d.category == Category::Person) {
            return;
        }
        let text = &view.text;
        let written = |text: &str| -> Vec<String> {
            text.split_whitespace()
                .map(|word| bare(word).trim_end_matches('.').to_owned())
                .filter(|word| !word.is_empty())
                .collect()
        };
        let lower =
            |words: &[String]| -> Vec<String> { words.iter().map(|w| w.to_lowercase()).collect() };
        let (nouns, after) = self.org_phrases();
        let nouns: Vec<Vec<&str>> = nouns.iter().map(|noun| noun.split(' ').collect()).collect();
        let people: Vec<(usize, bool, Vec<String>)> = detections
            .iter()
            .filter(|d| d.category == Category::Person)
            .map(|d| {
                let words = written(&text[view.view_range(d.range.clone())]);
                (d.range.start, self.titled(source, d.range.start), words)
            })
            .collect();
        let a_person = |words: &[String]| {
            let words = lower(words);
            people.iter().any(|(_, _, person)| lower(person) == words)
        };
        // Each short name as written, and whether a definition gives it.
        let mut known: Vec<(Vec<String>, bool)> =
            named.iter().map(|name| (name.clone(), false)).collect();
        // A span the model gave both categories ("Grant Pellover" as a
        // person and a company) is no company's evidence: the review's
        // conflict rules choose there. Nor is an individual entrepreneur
        // («ИП Иванов») a company (guide rule 1).
        let entrepreneur = |words: &[String]| {
            let words = lower(words);
            words.iter().any(|word| word == "ип")
                || words.windows(2).any(|pair| {
                    pair[0] == "индивидуальный" && pair[1].starts_with("предпринимател")
                })
        };
        let cited: Vec<std::ops::Range<usize>> = self
            .cited(text)
            .into_iter()
            .map(|case| case.range)
            .collect();
        struct Company {
            seen: std::ops::Range<usize>,
            words: Vec<String>,
            cited: bool,
        }
        let organizations: Vec<Company> = detections
            .iter()
            .filter(|d| {
                d.category == Category::Organization
                    && !detections
                        .iter()
                        .any(|other| other.category == Category::Person && other.range == d.range)
            })
            .map(|d| {
                let seen = view.view_range(d.range.clone());
                let words = written(&text[seen.clone()]);
                let cited = cited
                    .iter()
                    .any(|case| case.start <= seen.start && seen.end <= case.end);
                Company { seen, words, cited }
            })
            .filter(|company| !company.words.is_empty() && !entrepreneur(&company.words))
            .collect();
        let form = |word: &String| {
            self.legal_form(word) || SPELLED_FORMS.contains(&word.to_lowercase().as_str())
        };
        // Definitions right after a company: (its words, the term).
        let mut defined: Vec<(&Company, Vec<String>)> = Vec::new();
        for definition in definitions(text) {
            // Where the subject ends in `text`; an empty one may be no
            // slice of it.
            let Some(end) = (definition.subject.as_ptr() as usize)
                .checked_sub(text.as_ptr() as usize)
                .map(|start| start + definition.subject.len())
                .filter(|end| *end <= text.len() && !definition.subject.is_empty())
            else {
                continue;
            };
            let Some(company) = organizations.iter().find(|company| {
                company.seen.end <= end
                    && text[company.seen.end..end]
                        .chars()
                        .all(|c| c.is_whitespace() || c == '.' || c == ',')
            }) else {
                continue;
            };
            let mut term = written(definition.term);
            if term
                .first()
                .is_some_and(|word| word.eq_ignore_ascii_case("the"))
            {
                term.remove(0);
            }
            if entrepreneur(&term) {
                continue;
            }
            let words = lower(&company.words);
            if lower(&term).iter().any(|word| words.contains(word)) {
                defined.push((company, term));
            }
        }
        for company in &organizations {
            let words = &company.words;
            let rest = &text[company.seen.end..];
            let next: Vec<String> = rest
                .split_whitespace()
                .take(2)
                .map(|word| bare(word).trim_end_matches('.').to_lowercase())
                .collect();
            let spaced = rest.starts_with(char::is_whitespace);
            // "Pellam" before "airlines": a company's name with its noun
            // outside ([`Self::org_noun_name`]).
            let noun = spaced
                && nouns.iter().any(|noun| {
                    noun.len() <= next.len() && next.iter().zip(noun).all(|(a, b)| a == b)
                });
            if noun && words.len() == 1 {
                known.push((words.clone(), false));
            }
            let definition = defined
                .iter()
                .any(|(defining, _)| std::ptr::eq(*defining, company));
            // The whole name in another mention, on evidence beside this
            // one; a party role after it is a caption's person.
            let role = rest
                .trim_start_matches(|c: char| c.is_whitespace() || c == ',')
                .split(|c: char| !c.is_alphanumeric())
                .next()
                .map(str::to_lowercase)
                .is_some_and(|word| {
                    self.party_roles.contains(&word) || RU_PARTY_ROLES.contains(&word.as_str())
                });
            let after_phrase = || {
                let before: Vec<String> = text[..company.seen.start]
                    .split_whitespace()
                    .rev()
                    .take(5)
                    .map(str::to_lowercase)
                    .collect();
                after.iter().any(|phrase| {
                    let phrase: Vec<&str> = phrase.split(' ').rev().collect();
                    before.len() >= phrase.len() && before.iter().zip(&phrase).all(|(a, b)| a == b)
                })
            };
            let legal = words.iter().any(form)
                || spaced && next.first().is_some_and(|word| self.legal_form(word));
            if words.len() >= 2 && !role && (legal || definition || noun || after_phrase()) {
                known.push((words.clone(), false));
            }
            // A precedent's company gives its whole name only.
            if company.cited {
                continue;
            }
            let mut core = &words[..];
            while let Some((first, rest)) = core.split_first()
                && (first.eq_ignore_ascii_case("the") || form(first))
            {
                core = rest;
            }
            while let Some((last, rest)) = core.split_last()
                && !rest.is_empty()
                && form(last)
            {
                core = rest;
            }
            if core.is_empty() {
                continue;
            }
            // Before its legal form, unless a person's span has those words
            // and no definition makes them the company's.
            let owner = core.len() >= 2 && a_person(core) && !definition;
            if core.len() < words.len() && !owner {
                known.push((core.to_vec(), false));
            }
            // The first word, where the rest is a business word: no
            // joiner, as in a firm of partners' names.
            let joined = text[company.seen.clone()].contains(['&', '+'])
                || lower(core)
                    .iter()
                    .any(|word| ["and", "и", "&", "+"].contains(&word.as_str()));
            if core.len() >= 2
                && !owner
                && !joined
                && lower(&core[1..])
                    .iter()
                    .all(|word| BUSINESS_WORDS.contains(&word.as_str()))
            {
                known.push((core[..1].to_vec(), false));
            }
        }
        known.extend(defined.into_iter().map(|(_, term)| (term, true)));
        // A name's words alone: no title, role or joiner only, a capital
        // first, never a person's initials unless the document defines them.
        known.retain(|(words, defined)| {
            let name = |word: &String| {
                !self.never(word) && !JOINERS.contains(&word.to_lowercase().as_str())
            };
            let initials =
                |word: &String| word.chars().count() <= 3 && word.chars().all(char::is_uppercase);
            words.first().is_some_and(|first| {
                first.starts_with(char::is_uppercase) && first.chars().count() >= 2
            }) && words.iter().any(name)
                && (*defined || words.len() > 1 || !initials(&words[0]))
        });
        let known: Vec<Vec<String>> = known.into_iter().map(|(words, _)| lower(&words)).collect();
        if known.is_empty() {
            return;
        }
        // A person's word in this document: a span of it after a title, or
        // a longer person's span with it and another name; a longer span
        // that is a company's name counts only after a title.
        let personal = |name: &Vec<String>| {
            people.iter().any(|(_, titled, words)| {
                let lowered = lower(words);
                if lowered == *name {
                    return *titled;
                }
                lowered.len() > name.len()
                    && (*titled || !known.contains(&lowered))
                    && lowered.windows(name.len()).any(|run| run == &name[..])
                    && words.iter().zip(&lowered).any(|(word, lowered)| {
                        !name.contains(lowered)
                            && word.starts_with(char::is_uppercase)
                            && !self.never(word)
                    })
            })
        };
        let companies: Vec<&Vec<String>> = known.iter().filter(|name| !personal(name)).collect();
        for detection in detections
            .iter_mut()
            .filter(|d| d.category == Category::Person && d.recognizer == Recognizer::Model)
        {
            let words = lower(&written(&text[view.view_range(detection.range.clone())]));
            if !companies.iter().any(|name| **name == words) {
                continue;
            }
            // The possessive stays outside, as a person's does.
            let span = &source[detection.range.clone()];
            if let Some(possessive) = ["'s", "’s", "'S", "’S"]
                .into_iter()
                .find(|possessive| span.len() > possessive.len() && span.ends_with(possessive))
            {
                detection.range.end -= possessive.len();
            }
            detection.category = Category::Organization;
        }
    }

    /// The companies in `text` that a `nested_quotes` legal form and its
    /// quoted name make, one inner pair of quotes at most, in order and none
    /// over another: "ПАО «Банк»" and "ООО «Ромашка»" of "в ПАО «Банк» для
    /// ООО «Ромашка», к/с" (#381).
    pub(crate) fn quoted_companies(&self, text: &str) -> Vec<std::ops::Range<usize>> {
        let mut found: Vec<std::ops::Range<usize>> = self
            .spans
            .iter()
            .filter_map(|span| match span {
                Span::Fix(fix) if fix.category == Category::Organization => Some(fix),
                _ => None,
            })
            .flat_map(|fix| &fix.nested_quotes)
            .flat_map(|(_, anywhere)| anywhere.find_iter(text))
            .filter_map(|form| {
                let open = form.end()
                    + (text[form.end()..].len()
                        - text[form.end()..]
                            .trim_start_matches([' ', '\t', '\u{a0}'])
                            .len());
                let mut chars = text[open..].char_indices();
                let (_, quote) = chars.next()?;
                let closing = if quote == '«' { '»' } else { quote };
                if !matches!(quote, '«' | '"') {
                    return None;
                }
                // The inner pair open, a line's end or a second inner pair
                // ends the search.
                let mut inner = 0;
                for (at, c) in chars {
                    match c {
                        '\n' => return None,
                        '«' if quote == '«' => {
                            inner += 1;
                            if inner > 1 {
                                return None;
                            }
                        }
                        c if c == closing && quote == '«' && inner == 1 => inner = 2,
                        c if c == closing && quote == '«' && inner == 2 => {
                            return Some(form.start()..open + at + c.len_utf8());
                        }
                        c if c == closing && (quote == '"' || inner == 0) => {
                            return Some(form.start()..open + at + c.len_utf8());
                        }
                        _ => {}
                    }
                }
                None
            })
            .collect();
        // The earliest first, the longest of those starting together.
        found.sort_by_key(|range| (range.start, std::cmp::Reverse(range.end)));
        let mut end = 0;
        found.retain(|range| {
            let free = end <= range.start;
            if free {
                end = range.end;
            }
            free
        });
        found
    }

    /// Whether `text` has an organization's legal form (the ORG shape's
    /// `guess_words`: Ltd, LLC, ООО…).
    pub(super) fn legal_form(&self, text: &str) -> bool {
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

    /// Whether `text` holds a company's legal form by any of the pack's
    /// rules: an abbreviated or spelled-out form ([`Self::company_form`]:
    /// "Ltd", "Corporation", «общество»), a name with its form as an ORG
    /// `extend` reads it ("АНО «Кедр»", "Harbor Lane Bank, N.A"), or a form
    /// before nested quotes («АНО», «коллегия адвокатов»). A span with one
    /// is a whole company, never a piece of the next one's name (#502).
    pub(super) fn own_legal_form(&self, text: &str) -> bool {
        self.company_form(text)
            || self.spans.iter().any(|span| match span {
                Span::Fix(fix) if fix.category == Category::Organization => {
                    fix.extend.iter().any(|pattern| pattern.is_match(text))
                        || fix
                            .nested_quotes
                            .iter()
                            .any(|(_, anywhere)| anywhere.is_match(text))
                }
                _ => false,
            })
    }

    /// Whether `rest`, the text after a role and its code, carries on a
    /// company or other name, so the code opens a name rather than
    /// defining a party code: more capitalized words ("C1 Holdings"), a
    /// place ("C1 of North Quay"), a legal form within a few words ("C1
    /// international holdings ltd") or a trading name ("C1 trading as").
    pub(super) fn company_continuation(&self, rest: &str) -> bool {
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
    pub(super) fn only_legal_form(&self, text: &str) -> bool {
        let mut words = text
            .split(|c: char| !c.is_alphanumeric())
            .filter(|word| !word.is_empty())
            .peekable();
        words.peek().is_some() && words.all(|word| self.legal_form(word))
    }

    /// A reference-looking short name stays when the document also gives
    /// its company's legal form, ignoring case and a soft line wrap. No
    /// model span is required over the full name (guide rule 2).
    pub(super) fn company_short_name(&self, source: &str, text: &str) -> bool {
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
    /// one line, so only the span's lines are searched. A pattern with a
    /// `tail` grows only a span it starts with, to where the tail starts
    /// ("QUILLAN" of "QUILLAN, INC").
    pub(super) fn grows_to_company(
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
            .any(|pattern| {
                let line = &source[start..end];
                if pattern
                    .capture_names()
                    .flatten()
                    .any(|group| group == "tail")
                {
                    return pattern
                        .captures_at(line, range.start - start)
                        .and_then(|found| Some((found.get(0)?.start(), found.name("tail")?)))
                        .is_some_and(|(at, tail)| {
                            start + at == range.start && range.end < start + tail.start()
                        });
                }
                pattern.find_iter(line).any(|found| {
                    let found_range = start + found.start()..start + found.end();
                    found_range.start <= range.start
                        && range.end <= found_range.end
                        && found_range.len() > range.len()
                })
            })
    }
}
