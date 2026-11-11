//! A person's other name forms the model proposed as an organization, or
//! with a word glued on (#503): a surname the document gives a person
//! ("Ostrander confirmed" beside "Talwen Ostrander"), a titled surname
//! ("Dr. Pellow"), surnames listed after a plural title ("Senators
//! Quillfeather, Brannock and Ostley"), a name with a noun after it ("the
//! Pellow family") and initials after a preposition («к Е. Р.»).
use super::context::CONTEXT_SCORE;
use super::shapes::bare;
use super::{Pack, Span};
use mdoc_pii::{Category, Detection, Recognizer};
use regex::Regex;
use std::collections::HashSet;
use std::ops::Range;
use std::sync::LazyLock;

/// The most name words a relabelled organization holds: a surname alone, a
/// given name and a surname, or one with a middle name.
const NAME_WORDS: usize = 3;

/// The most words a list of surnames after a plural title is read back
/// over, from a span to its title.
const LIST_WORDS: usize = 24;

/// Whether `word` is shaped like a name word: a capital, then letters, a
/// hyphen or an apostrophe ("Ostrander", "STROUD", "Brannock-Vale",
/// "O'Neil").
fn name_word(word: &str) -> bool {
    word.chars().next().is_some_and(char::is_uppercase)
        && word.chars().count() >= 2
        && word
            .chars()
            .all(|c| c.is_alphabetic() || matches!(c, '-' | '\'' | '’'))
}

/// Whether `token` is one of `lists`, a heading's colon aside, as written
/// ("Sens.") or without its period ("Messrs." of "messrs").
fn list_title(token: &str, lists: &[&str]) -> bool {
    let lower = token.trim_end_matches(':').to_lowercase();
    lists.contains(&lower.as_str()) || lists.contains(&lower.trim_end_matches('.'))
}

/// The words of `text` in `source` at `at`, each with its range there.
fn words_at(at: usize, text: &str) -> Vec<(Range<usize>, String)> {
    text.split_whitespace()
        .map(|word| {
            let start = at + (word.as_ptr() as usize - text.as_ptr() as usize);
            (start..start + word.len(), word.to_owned())
        })
        .collect()
}

/// `range` of `source` without whitespace at its ends and a possessive
/// "'s" at its end ("Ostrander’s" is "Ostrander").
fn name_range(source: &str, range: &Range<usize>) -> Range<usize> {
    let text = &source[range.clone()];
    let start = range.start + (text.len() - text.trim_start().len());
    let mut end = range.start + text.trim_end().len();
    if let Some(possessive) = ["'s", "’s", "'S", "’S"]
        .iter()
        .find(|possessive| source[start..end].ends_with(**possessive))
        && end - possessive.len() > start
    {
        end -= possessive.len();
    }
    start..end.max(start)
}

/// Capitalised infinitives that open a court order's operative part,
/// as the PERSON trim before initials in `ru.toml` lists them, and its
/// initials join's `unless_over` in lowercase: «Обязать Е. Р. …». Only
/// these, never any capitalised word in -ть, which may be a surname
/// («Свидетель Локоть Е. Р.»). A test keeps the three lists equal.
pub(super) const OPERATIVE_VERBS: &str = "Обязать|Взыскать|Признать|Отказать|Возложить|Запретить|Освободить|Назначить|Вернуть|Возвратить|Выдать|Выплатить|Возместить|Оплатить|Перечислить|Погасить|Компенсировать|Предоставить|Передать|Восстановить|Прекратить|Приостановить|Возобновить|Расторгнуть|Отменить|Изменить|Удовлетворить|Оставить|Отклонить|Направить|Привлечь|Наложить|Выселить|Вселить|Истребовать|Изъять|Конфисковать|Лишить|Ограничить|Отстранить|Утвердить|Включить|Исключить|Зачесть|Уменьшить|Снизить|Обратить|Разъяснить|Установить|Объявить|Допросить|Вызвать|Уведомить|Предупредить|Разрешить|Определить|Снять|Продлить|Отсрочить|Рассрочить|Произвести|Обеспечить|Поручить|Применить|Избрать|Поместить|Зарегистрировать|Аннулировать|Разделить|Выделить|Присудить|Принять|Устранить";

/// Initials in Cyrillic after lowercase words or an operative verb
/// ([`OPERATIVE_VERBS`]), the whole of a model organization: «к Е. Р.»,
/// «Обязать Е. Р.» are the initials, the word outside (guide rule 1,
/// #793). Latin initials are left alone: "of U.S." is a country.
static GLUED_INITIALS: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(&format!(
        r"^(?:(?:\p{{Ll}}+|(?:{OPERATIVE_VERBS}))[\p{{Zs}}\t]+)+(?P<initials>[А-ЯЁ]\.(?:[\p{{Zs}}\t]?[А-ЯЁ]\.){{1,2}})$",
    ))
    .expect("constant glued initials pattern")
});

/// A Russian name of two initials and a surname, in either order: the
/// initials after the surname («Верхов-Залесский Р.Р.», `after`) or before
/// it, glued or spaced («Е. Р. Тулину», «Т.Т.Ольшанская», `before`); a
/// hyphenated surname too (guide rule 1: initials are a person's).
static INITIALS_NAME: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(
        r"^(?:(?:[А-ЯЁ][\x27’])?[А-ЯЁ][а-яё]+(?:-[А-ЯЁ][а-яё]+)*[\p{Zs}\t]+(?P<after>[А-ЯЁ]\.[\p{Zs}\t]?[А-ЯЁ]\.)|(?P<before>[А-ЯЁ]\.[\p{Zs}\t]?[А-ЯЁ]\.)[\p{Zs}\t]*(?:[А-ЯЁ][\x27’])?[А-ЯЁ][а-яё]+(?:-[А-ЯЁ][а-яё]+)*)$",
    )
    .expect("constant initials name pattern")
});

/// Two Cyrillic initials, spaced or not («Е. Р.», «Е.Р.»).
static BARE_INITIALS: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"[А-ЯЁ]\.[\p{Zs}\t]?[А-ЯЁ]\.").expect("constant bare initials pattern")
});

/// Quotes that set off a company's name («ООО «Р.Р. Групп»»).
const QUOTES: [char; 6] = ['«', '»', '"', '“', '”', '„'];

/// Spaces on a line, between a name and its quotes or its neighbours.
const BLANKS: [char; 3] = [' ', '\t', '\u{a0}'];

/// More initials after a comma or «и»: a list of letters («А.Б., В.Г.»,
/// «А.Б. и В.Г.»), no person's name.
static MORE_INITIALS: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"^[\p{Zs}\t]*(?:,|и[\p{Zs}\t])[\p{Zs}\t]*[А-ЯЁ]\.")
        .expect("constant more initials pattern")
});

/// What the document says of people, read from the model's person spans.
struct People {
    /// Each person span's surname, lowercase: the word before a comma
    /// ("Penrow, J. L."), or else the last name word ("Talwen Ostrander",
    /// «Иванов И. И.»).
    surnames: HashSet<String>,
    /// Each person span's name words, lowercase, single-spaced, a title
    /// or post-nominal left out ("talwen ostrander").
    names: HashSet<String>,
}

impl Pack {
    /// The PERSON fixes' `person_titles`, `person_lists`, `person_nouns`
    /// and `trim` words, in that order.
    fn person_words(&self) -> [Vec<&str>; 4] {
        let mut words: [Vec<&str>; 4] = Default::default();
        for span in &self.spans {
            if let Span::Fix(fix) = span
                && fix.category == Category::Person
            {
                for (list, found) in words.iter_mut().zip([
                    &fix.person_titles,
                    &fix.person_lists,
                    &fix.person_nouns,
                    &fix.trim,
                ]) {
                    list.extend(found.iter().map(String::as_str));
                }
            }
        }
        words
    }

    /// Whether a person's title stands right before `at` in `source`, a
    /// space between and on the same line: a courtesy title or a
    /// post-nominal by the person fixes' `trim` ([`Self::titled`]), or a
    /// `person_titles` word ("Senator Quillfeather", "Sen.
    /// Quillfeather").
    fn person_titled(&self, source: &str, at: usize, titles: &[&str]) -> bool {
        let line = source[..at].rsplit('\n').next().unwrap_or_default();
        let before = line.trim_end_matches([' ', '\t']);
        if before.len() == line.len() {
            return false;
        }
        if self.titled(source, at) {
            return true;
        }
        let lower = before.to_lowercase();
        titles.iter().any(|title| {
            lower.strip_suffix(title).is_some_and(|rest| {
                !rest
                    .chars()
                    .next_back()
                    .is_some_and(|c| c.is_alphanumeric() || c == '-')
            })
        })
    }

    /// Whether `source` writes `word` as a whole word after a person's
    /// title anywhere ([`Self::person_titled`]).
    fn titled_anywhere(&self, source: &str, word: &str, titles: &[&str]) -> bool {
        let inside = |c: Option<char>| c.is_some_and(|c| c.is_alphanumeric() || c == '-');
        !word.is_empty()
            && source.match_indices(word).any(|(at, _)| {
                !inside(source[..at].chars().next_back())
                    && !inside(source[at + word.len()..].chars().next())
                    && self.person_titled(source, at, titles)
            })
    }

    /// The surnames and names of the person spans in `detections` that
    /// name a person beyond doubt: two words or more, initials included
    /// ("Talwen Ostrander", "J. L. Penrow"), or a title before the span or
    /// in it ("Mr Davis"). A lone word the model called a person may be a
    /// company's short name ("Pellam" beside "Pellam airlines", #548), so it
    /// proves nothing alone.
    fn people(
        &self,
        source: &str,
        detections: &[Detection],
        titles: &[&str],
        trim: &[&str],
    ) -> People {
        let mut people = People {
            surnames: HashSet::new(),
            names: HashSet::new(),
        };
        for detection in detections.iter().filter(|d| d.category == Category::Person) {
            let range = name_range(source, &detection.range);
            let text = &source[range.clone()];
            if text.split_whitespace().count() < 2
                && !self.person_titled(source, range.start, titles)
            {
                continue;
            }
            let name = |part: &str| -> Vec<String> {
                part.split_whitespace()
                    .map(bare)
                    .filter(|word| name_word(word))
                    .map(str::to_lowercase)
                    .filter(|word| !trim.contains(&word.as_str()) && !self.never(word))
                    .collect()
            };
            let words = name(text);
            let surname = match text.split_once(',') {
                Some((first, _)) => name(first).last().cloned(),
                None => words.last().cloned(),
            };
            if let Some(surname) = surname {
                people.surnames.insert(surname);
            }
            if words.len() > 1 {
                people.names.insert(words.join(" "));
            }
        }
        people
    }

    /// Lowercase name words of the model's organizations that are a
    /// company's or a body's by their own words: a legal form ("Ostrander
    /// Holdings Ltd") or a body word (`ocr_company_words`: "Ostrander
    /// Foundation"). A bare surname among them may be that company's short
    /// name (guide rule 2, #506 item 10), so it stays as the model gave it.
    fn company_words(&self, source: &str, detections: &[Detection]) -> HashSet<String> {
        detections
            .iter()
            .filter(|d| d.category == Category::Organization)
            .map(|d| &source[d.range.clone()])
            .filter(|text| self.company_named(text))
            .flat_map(|text| {
                text.split_whitespace()
                    .map(|word| bare(word).to_lowercase())
                    .collect::<Vec<_>>()
            })
            .collect()
    }

    /// Whether `text` names a company or a body by its own words: a legal
    /// form, a body word (`ocr_company_words`) or a public body.
    fn company_named(&self, text: &str) -> bool {
        self.own_legal_form(text)
            || self.legal_form(text)
            || text
                .split_whitespace()
                .any(|word| self.ocr_company_words.contains(&bare(word).to_lowercase()))
            || self.public_body(text, Category::Organization, false)
    }

    /// Whether the span at `start` of `source` follows a plural title and
    /// the surnames listed after it so far, on its line: "Senators " before
    /// "Quillfeather", "Senators Quillfeather, Brannock and " before
    /// "Ostley" (`person_lists`).
    fn listed(&self, source: &str, start: usize, lists: &[&str]) -> bool {
        if lists.is_empty() {
            return false;
        }
        let line = source[..start].rsplit('\n').next().unwrap_or_default();
        if !line.ends_with([' ', '\t']) {
            return false;
        }
        let tokens = line.split_whitespace().rev().take(LIST_WORDS);
        let title = |token: &str| list_title(token, lists);
        let name = |token: &str| name_word(token) && !self.never(token);
        // After "and" a name is due; after a name and its comma, the title
        // or the next name with its comma.
        let mut due = false;
        for token in tokens {
            if due {
                if !name(token.trim_end_matches(',')) {
                    return false;
                }
                due = false;
            } else if title(token) {
                return true;
            } else if token == "and" || token == "&" {
                due = true;
            } else if !token.strip_suffix(',').is_some_and(name) {
                return false;
            }
        }
        false
    }

    /// The names of a list span after a plural title, each with its
    /// range: "Quillfeather", "Brannock" and "Ostley" of the model's one
    /// span "Quillfeather, Brannock and Ostley" (`person_lists`), and the
    /// name alone of a span the title opens ("Senators Quillfeather").
    /// `None` unless every piece between a comma, "and" or "&" is one or
    /// two name words, and there are two pieces at least or the title
    /// opens the span. A person's span of two pieces with only a comma
    /// between may be a surname-first name ("Brannock, Ivo"), so it needs
    /// "and" or "&" (`person`).
    fn list_pieces(
        &self,
        source: &str,
        range: &Range<usize>,
        lists: &[&str],
        person: bool,
    ) -> Option<Vec<Range<usize>>> {
        static SEPARATOR: LazyLock<Regex> = LazyLock::new(|| {
            Regex::new(r",[\p{Zs}\t]*(?:(?:and|&)[\p{Zs}\t]+)?|[\p{Zs}\t]+(?:and|&)[\p{Zs}\t]+")
                .expect("constant list separator pattern")
        });
        let range = name_range(source, range);
        let mut start = range.start;
        // The title itself may open the span.
        let text = &source[range.clone()];
        let opened = text
            .split_once([' ', '\t'])
            .is_some_and(|(first, _)| list_title(first, lists));
        if opened {
            start += text.len() - text.trim_start_matches(|c: char| !c.is_whitespace()).len();
            start += source[start..range.end].len() - source[start..range.end].trim_start().len();
        } else if !self.listed(source, range.start, lists) {
            return None;
        }
        let text = &source[start..range.end];
        let mut cuts: Vec<Range<usize>> = SEPARATOR.find_iter(text).map(|m| m.range()).collect();
        let joined = cuts.iter().any(|cut| {
            let separator = &text[cut.clone()];
            separator.contains("and") || separator.contains('&')
        });
        if person && !opened && cuts.len() == 1 && !joined {
            return None;
        }
        cuts.push(text.len()..text.len());
        let mut pieces = Vec::new();
        let mut at = 0;
        for cut in cuts {
            let words: Vec<&str> = text[at..cut.start].split_whitespace().collect();
            if words.is_empty()
                || words.len() > 2
                || !words
                    .iter()
                    .all(|word| name_word(word) && !self.never(word))
            {
                return None;
            }
            pieces.push(start + at..start + cut.start);
            at = cut.end;
        }
        (opened || pieces.len() > 1).then_some(pieces)
    }

    /// The initials of a Russian name of initials and a surname at `range`
    /// of `source` ([`INITIALS_NAME`]), letters only: "ЕР" of «Е. Р.
    /// Тулину» and of «Тулина Е.Р.». `None` for other text and for a
    /// company's: a legal form, a body word or a public body in it, or
    /// quotes in it or around it («ООО «Р.Р. Групп»»).
    fn initials_name(&self, source: &str, range: &Range<usize>) -> Option<String> {
        let text = &source[range.clone()];
        let found = INITIALS_NAME.captures(text)?;
        if text.contains(QUOTES) || self.quoted(source, range) || self.company_named(text) {
            return None;
        }
        let initials = found.name("after").or_else(|| found.name("before"))?;
        Some(
            initials
                .as_str()
                .chars()
                .filter(|c| c.is_alphabetic())
                .collect(),
        )
    }

    /// Whether the text at `range` of `source` stands in a company's
    /// quotes or after its legal form: a quote right before or after it,
    /// spaces between allowed («ООО « Д.А. Сухов »», «ООО «Д.А.»»), or a
    /// legal form as the word before it, an opening quote between
    /// allowed.
    fn quoted(&self, source: &str, range: &Range<usize>) -> bool {
        let before = source[..range.start].trim_end_matches(BLANKS);
        let after = source[range.end..].trim_start_matches(BLANKS);
        let quote = |c: Option<char>| c.is_some_and(|c| QUOTES.contains(&c));
        if quote(before.chars().next_back()) || quote(after.chars().next()) {
            return true;
        }
        before
            .trim_end_matches(|c: char| BLANKS.contains(&c) || QUOTES.contains(&c))
            .rsplit(|c: char| c.is_whitespace())
            .next()
            .is_some_and(|word| !word.is_empty() && self.legal_form(word))
    }

    /// Bare Cyrillic initials the model left unmarked are a known person's
    /// other name form (#503) where a person span of the document is those
    /// initials and a surname: «Требования к Е. Р.» beside «Е. Р. Тулину».
    /// Only two initials after a lowercase word and a space (a
    /// preposition, a verb, a conjunction: no colon or other mark between,
    /// so a label's «код страны: Р.Ф.» stays): no letter, digit or period
    /// glued on, no capitalised word after them (a surname, the next
    /// initial), no more initials after a comma or «и» (a list of letters,
    /// «А.Б., В.Г.»), no quote beside them or legal form before them (a
    /// company's «ООО «Д.А.»»), and no span over them. Initials no person
    /// of the document has stay unmarked.
    fn bare_initials(&self, source: &str, detections: &mut Vec<Detection>) {
        let known: HashSet<String> = detections
            .iter()
            .filter(|d| d.category == Category::Person)
            .filter_map(|d| self.initials_name(source, &name_range(source, &d.range)))
            .collect();
        if known.is_empty() {
            return;
        }
        let mut added = Vec::new();
        for found in BARE_INITIALS.find_iter(source) {
            let range = found.range();
            let after = &source[range.end..];
            if source[..range.start]
                .chars()
                .next_back()
                .is_some_and(|c| c.is_alphanumeric() || c == '.')
                || after.starts_with(char::is_alphanumeric)
            {
                continue;
            }
            let line = source[..range.start]
                .rsplit('\n')
                .next()
                .unwrap_or_default();
            let before = line.trim_end_matches(BLANKS);
            let lowercase = before.len() < line.len()
                && before
                    .rsplit(char::is_whitespace)
                    .next()
                    .is_some_and(|word| !word.is_empty() && word.chars().all(char::is_lowercase));
            if !lowercase
                || after
                    .trim_start_matches(BLANKS)
                    .starts_with(char::is_uppercase)
                || MORE_INITIALS.is_match(after)
                || self.quoted(source, &range)
            {
                continue;
            }
            let letters: String = found
                .as_str()
                .chars()
                .filter(|c| c.is_alphabetic())
                .collect();
            if !known.contains(&letters)
                || detections
                    .iter()
                    .any(|d| d.range.start < range.end && range.start < d.range.end)
            {
                continue;
            }
            added.push(Detection {
                range,
                category: Category::Person,
                score: CONTEXT_SCORE,
                recognizer: Recognizer::Span,
            });
        }
        detections.extend(added);
    }

    /// The wording of a model organization's list (`pieces`, by
    /// [`Self::list_pieces`]), from its first name to its last, a heading
    /// or title before it left out, if it is a company's by its whole
    /// wording: a legal form or a body word in it, or the wording written
    /// with a legal form after it ("Pellam and Quenby" of "Pellam and
    /// Quenby LLP" or "Pellam and Quenby, LLP", its short name). Then the
    /// list is no list of people: it stays one organization, without the
    /// heading ("Signatories: Pellam and Quenby" is "Pellam and Quenby").
    fn company_list(&self, source: &str, pieces: &[Range<usize>]) -> Option<Range<usize>> {
        let wording = pieces.first()?.start..pieces.last()?.end;
        let text = &source[wording.clone()];
        (self.company_named(text) || self.company_short_name(source, text)).then_some(wording)
    }

    /// Whether one name of a model organization's list is a company's, as
    /// the relabelling of a lone surname checks: its surname a company of
    /// the document holds (`companies`, by [`Self::company_words`]), or its
    /// wording written with a legal form after it. Only that name stays an
    /// organization; the others of the list are people ("Senators Hartrow
    /// and Vensel" beside "Hartrow Holdings Ltd": "Vensel" is a person).
    fn company_piece(&self, source: &str, piece: &str, companies: &HashSet<String>) -> bool {
        piece
            .split_whitespace()
            .next_back()
            .is_some_and(|surname| companies.contains(&bare(surname).to_lowercase()))
            || self.company_short_name(source, piece)
    }

    /// A person's other name forms the model proposed as an organization,
    /// or with a word glued on, are the person (guide rule 1, #503, #793),
    /// before the span fixes, so a title left in comes off as any person's
    /// does:
    ///
    /// - an organization whose words are a person span's surname alone
    ///   ("Ostrander confirmed" beside "Talwen Ostrander") or name, in any
    ///   letter case ("STROUD"), or a surname the document writes after a
    ///   person's title anywhere ("Senator Quillfeather");
    /// - an organization of a person's title and a name ("Dr. Pellow",
    ///   "HHJ Penrow"): the name;
    /// - surnames after a plural title (`person_lists`: "Senators
    ///   Quillfeather, Brannock and Ostley"), each its own span, and the
    ///   model's one span over the list cut into them;
    /// - a name and a `person_nouns` word ("the Pellow family", "Ostrander
    ///   Lab"): the name, the noun and a "the" before it outside, as an
    ///   organization only where the document names the person so;
    /// - Cyrillic initials after a lowercase word or a verb («к Е. Р.»):
    ///   the initials;
    /// - a Russian surname with two initials («Верхов-Залесский Р.Р.») the
    ///   model gave as a person and an organization, and bare initials of a
    ///   person the document names with them and a surname («к Е. Р.»
    ///   beside «Е. Р. Тулину»).
    ///
    /// Never an organization with a legal form or a body word ("Ostrander
    /// Holdings Ltd", "Ostrander Foundation"), one whose surname such a
    /// company of the document holds or the document writes with a legal
    /// form after it (its short name, #506 item 10), nor one the model also
    /// gave as a person (the review chooses) unless it is a surname with
    /// initials; a programme or prize named
    /// after a person ("Pellow Prize") is no surname alone, so it stays.
    pub(super) fn person_forms(&self, source: &str, detections: &mut Vec<Detection>) {
        if !detections
            .iter()
            .any(|d| matches!(d.category, Category::Organization | Category::Person))
        {
            return;
        }
        let [titles, lists, nouns, trim] = self.person_words();
        let people = self.people(source, detections, &titles, &trim);
        // A wording the model gave both categories stays both: the review
        // chooses between them.
        let both: HashSet<Range<usize>> = detections
            .iter()
            .filter(|d| d.category == Category::Person)
            .map(|d| d.range.clone())
            .collect();
        let companies = self.company_words(source, detections);
        let mut added = Vec::new();
        for detection in detections.iter_mut() {
            if !matches!(
                detection.category,
                Category::Organization | Category::Person
            ) {
                continue;
            }
            // A list after a plural title: one span per surname, unless the
            // model's organization is a company's name or short name, which
            // stays one organization, its heading outside; a name of it a
            // company holds stays an organization alone.
            let person = detection.category == Category::Person;
            if let Some(pieces) = self.list_pieces(source, &detection.range, &lists, person) {
                if !person && let Some(wording) = self.company_list(source, &pieces) {
                    detection.range = wording;
                    continue;
                }
                let category = |piece: &Range<usize>| {
                    if !person && self.company_piece(source, &source[piece.clone()], &companies) {
                        Category::Organization
                    } else {
                        Category::Person
                    }
                };
                added.extend(pieces[1..].iter().map(|piece| Detection {
                    range: piece.clone(),
                    category: category(piece),
                    ..detection.clone()
                }));
                detection.category = category(&pieces[0]);
                detection.range = pieces[0].clone();
                continue;
            }
            let range = name_range(source, &detection.range);
            let text = &source[range.clone()];
            let mut words = words_at(range.start, text);
            if words
                .first()
                .is_some_and(|(_, word)| word.eq_ignore_ascii_case("the"))
            {
                words.remove(0);
            }
            // A name and a noun after it: the name alone.
            let noun = words.len() > 1
                && words.last().is_some_and(|(_, word)| {
                    nouns.contains(&bare(word).trim_end_matches('.').to_lowercase().as_str())
                });
            if noun {
                let name = &words[..words.len() - 1];
                let name_range = name[0].0.start..name[name.len() - 1].0.end;
                // A body's name ends with the noun: "Brookhaven National
                // Laboratory" is a public body's whole name.
                let shaped = name.len() <= NAME_WORDS
                    && name.iter().all(|(_, word)| name_word(word))
                    && !self.company_named(text)
                    && !self.public_body(text, Category::Person, false);
                let surname = bare(&name[name.len() - 1].1).to_lowercase();
                let known = detection.category == Category::Person
                    || people.surnames.contains(&surname)
                    || self.titled_anywhere(source, bare(&name[name.len() - 1].1), &titles);
                if shaped && known {
                    detection.range = name_range;
                    detection.category = Category::Person;
                }
                continue;
            }
            // A Russian surname with initials the model gave as a person and
            // an organization at once is the person: initials are a
            // person's (guide rule 1, #503). An organization alone stays as
            // the model gave it: an entrepreneur's rule decides that.
            if detection.category == Category::Organization
                && both.contains(&detection.range)
                && self.initials_name(source, &range).is_some()
            {
                detection.range = range;
                detection.category = Category::Person;
                continue;
            }
            if detection.category != Category::Organization || both.contains(&detection.range) {
                continue;
            }
            if let Some(found) = GLUED_INITIALS.captures(text)
                && let Some(initials) = found.name("initials")
            {
                detection.range = range.start + initials.start()..range.start + initials.end();
                detection.category = Category::Person;
                continue;
            }
            if self.company_named(text) || words.is_empty() {
                continue;
            }
            // A person's titles, then the name.
            let titled = words
                .iter()
                .take_while(|(_, word)| {
                    let lower = word.to_lowercase();
                    titles.contains(&lower.as_str()) || trim.contains(&lower.trim_end_matches('.'))
                })
                .count();
            let name = &words[titled..];
            if name.is_empty()
                || name.len() > NAME_WORDS
                || !name
                    .iter()
                    .all(|(_, word)| name_word(word) && !self.never(word))
            {
                continue;
            }
            let wording: Vec<String> = name.iter().map(|(_, word)| word.to_lowercase()).collect();
            let surname = &wording[wording.len() - 1];
            // A name the document writes with a legal form after it is a
            // company's short name ("BRINDLE HARRIERS" of "BRINDLE HARRIERS
            // LLC").
            let wording_text = &source[name[0].0.start..name[name.len() - 1].0.end];
            let person = titled > 0
                || !companies.contains(surname)
                    && !self.company_short_name(source, wording_text)
                    && (name.len() == 1
                        && (people.surnames.contains(surname)
                            || self.listed(source, range.start, &lists)
                            || self.titled_anywhere(source, &name[0].1, &titles))
                        || name.len() > 1 && people.names.contains(&wording.join(" ")));
            if person {
                detection.range = name[0].0.start..name[name.len() - 1].0.end;
                detection.category = Category::Person;
            }
        }
        detections.extend(added);
        // A span relabelled to the person the model also gave is one span.
        let mut seen = HashSet::new();
        detections.retain(|d| seen.insert((d.range.clone(), d.category)));
        self.bare_initials(source, detections);
    }
}
