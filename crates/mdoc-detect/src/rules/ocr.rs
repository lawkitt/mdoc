//! OCR damage: digits read for letters in a name ("A1ice") and words
//! written in look-alike letters of another script («Ha» for «На»).
use super::shapes::bare;
use super::{Pack, Span};
use mdoc_pii::Category;
use regex::Regex;

/// Whether `word` has a run of digits between two letters, as OCR leaves
/// in a name ("A1ice", "M00re"), not at its edge ("A1", "3M").
fn digit_inside(word: &str) -> bool {
    let chars: Vec<char> = word.chars().collect();
    chars.iter().enumerate().any(|(index, c)| {
        c.is_ascii_digit()
            && index > 0
            && chars[index - 1].is_alphabetic()
            && chars[index..]
                .iter()
                .find(|c| !c.is_ascii_digit())
                .is_some_and(|c| c.is_alphabetic())
    })
}

impl Pack {
    /// Whether a person's or an organization's model span `text` at `range`
    /// of `source` is a common Russian word written in another script's
    /// look-alike letters (`lookalike`, #549): «Ha» before a date is «На»,
    /// «Bce» is «Все», «T.π.» is «т.п.». Each word of the span holds only
    /// Cyrillic and look-alike letters (and an abbreviation's dots), one of
    /// them at least a look-alike, and read in Cyrillic it is a `lookalike`
    /// word or a `never` word. The text around is Russian: more Cyrillic
    /// letters than Latin ones within 200 bytes on either side. A name
    /// stays: a word with a letter of its own («Ивaнов», "Hans"), a capital
    /// alone (an initial), a word of look-alikes all in capitals ("BCE",
    /// an abbreviation), a span in quotes («Bce»), one after a person's
    /// title, label or role in its clause («Фамилия: Ha», «гражданка
    /// Республики Корея Ha»: `unless_after`, a `person_role` word or a
    /// person's `trim` word), and a word the document writes beside a
    /// Latin name in any case ("Nguyễn Thị Ha", "HA THI LAN").
    pub(super) fn lookalike_word(
        &self,
        source: &str,
        range: std::ops::Range<usize>,
        text: &str,
    ) -> bool {
        if self.lookalike_letters.is_empty() || self.titled(source, range.start) {
            return false;
        }
        let (mut foreign, mut any) = (Vec::new(), false);
        for word in text.split_whitespace().map(bare).filter(|w| !w.is_empty()) {
            let (mut letters, mut looks, mut capitals) = (0, 0, 0);
            let mut read = String::new();
            for c in word.chars() {
                let cyrillic = if c == '.' {
                    read.push(c);
                    continue;
                } else if cyrillic_letter(c) {
                    c
                } else if let Some(&cyrillic) = self.lookalike_letters.get(&c) {
                    looks += 1;
                    cyrillic
                } else {
                    return false;
                };
                letters += 1;
                capitals += usize::from(c.is_uppercase());
                read.extend(cyrillic.to_lowercase());
            }
            let initial = letters == 1 && capitals == 1;
            let abbreviation = letters > 1 && capitals == letters && looks == letters;
            if looks > 0 && (initial || abbreviation)
                || !(self.lookalike_words.contains(&read) || self.never(&read))
            {
                return false;
            }
            any |= looks > 0;
            if looks == letters {
                foreign.push(word);
            }
        }
        if !any {
            return false;
        }
        // Quoted, it is a name of its own («Bce»), however it reads.
        let (before, after, inner) = (&source[..range.start], &source[range.end..], text.trim());
        if [('«', '»'), ('“', '”'), ('"', '"')]
            .iter()
            .any(|&(open, close)| {
                before.ends_with(open) && after.starts_with(close)
                    || inner.len() > 2 && inner.starts_with(open) && inner.ends_with(close)
            })
        {
            return false;
        }
        // A person's label, role or title in its clause, up to four words
        // before it: «Фамилия: Ha», «Свидетель Ha», «гражданка Республики
        // Корея Ha», «Работник: Ha».
        let clause = before
            .rsplit(['\n', '.', '!', '?', ';', ','])
            .next()
            .unwrap_or_default();
        let person = clause
            .split(|c: char| !c.is_alphabetic() && c != '-')
            .filter(|word| !word.is_empty())
            .rev()
            .take(4)
            .any(|word| {
                let word = word.to_lowercase();
                self.lookalike_after.iter().any(|stem| {
                    word.strip_prefix(stem.as_str())
                        .is_some_and(|ending| ending.chars().count() <= 4)
                }) || self
                    .never
                    .iter()
                    .any(|entry| entry.person_role && entry.has(&word))
                    || self.spans.iter().any(|span| match span {
                        Span::Fix(fix) => {
                            fix.category == Category::Person && fix.trim.contains(&word)
                        }
                        Span::Pattern { .. } => false,
                    })
            });
        if person {
            return false;
        }
        // A Latin name: capitalized, two letters or more of any script but
        // Cyrillic, one of them no look-alike ("Jin" of "Ha Jin", "Thị").
        let latin_name = |word: &str| {
            word.chars().count() > 1
                && word.starts_with(char::is_uppercase)
                && word
                    .chars()
                    .all(|c| c.is_alphabetic() && !cyrillic_letter(c))
                && word
                    .chars()
                    .any(|c| !self.lookalike_letters.contains_key(&c))
        };
        // Beside one anywhere in the document, in any case ("HA THI LAN").
        let beside_name = |word: &str| {
            let Ok(written) = Regex::new(&format!("(?i){}", regex::escape(word))) else {
                return false;
            };
            written.find_iter(source).any(|found| {
                let before = &source[..found.start()];
                let after = &source[found.end()..];
                if before.ends_with(char::is_alphanumeric)
                    || after.starts_with(char::is_alphanumeric)
                {
                    return false;
                }
                let next = after.trim_start_matches([' ', '\t']);
                let previous = before.trim_end_matches([' ', '\t']);
                (next.len() < after.len()
                    && latin_name(
                        next.split(|c: char| !c.is_alphabetic())
                            .next()
                            .unwrap_or(""),
                    ))
                    || (previous.len() < before.len()
                        && latin_name(
                            previous
                                .rsplit(|c: char| !c.is_alphabetic())
                                .next()
                                .unwrap_or(""),
                        ))
            })
        };
        if foreign.iter().any(|word| beside_name(word)) {
            return false;
        }
        let mut start = range.start.saturating_sub(200);
        while !source.is_char_boundary(start) {
            start += 1;
        }
        let mut end = (range.end + 200).min(source.len());
        while !source.is_char_boundary(end) {
            end -= 1;
        }
        let around = source[start..range.start]
            .chars()
            .chain(source[range.end..end].chars());
        let (cyrillic, latin) = around.fold((0, 0), |(cyrillic, latin), c| {
            (
                cyrillic + usize::from(cyrillic_letter(c)),
                latin + usize::from(c.is_ascii_alphabetic()),
            )
        });
        cyrillic > latin
    }

    /// Whether the model organization at `range` of `document` (the view's
    /// text) is a person's name OCR damaged (#377): two or three words,
    /// each a capitalized name word or initials ("A.", "Н.Е."), one of them
    /// a name word with a digit between two letters, whose digits read
    /// back as `ocr_letters` make a person's name of the PERSON shape. Never
    /// with a legal form or an `ocr_company_words` word, as written or read
    /// back, nor where another of the document's model `organizations`,
    /// read back, starts with it ("M0rgan Stanley" beside "Morgan Stanley
    /// Ltd" or "Morgan Stanley").
    ///
    /// A name of a person's own form is one alone: initials beside a
    /// surname ("J. Sm1th", «Ко3лов И. П.»), a surname, a comma and a name
    /// ("M00re, Alice"), or an `ocr_person_words` word («Ефим0вна»). Plain
    /// capitalized words ("A1ice M0rgan", but also "G0ldman Sachs") need
    /// the document's evidence of a person: a person's title right before
    /// them, an email whose local part holds each of their words
    /// ("alice.morgan@…"), or the model's person (`people`) with the same
    /// words or their last one.
    pub(crate) fn ocr_person(
        &self,
        document: &str,
        range: std::ops::Range<usize>,
        organizations: &[&str],
        people: &[&str],
    ) -> bool {
        if self.ocr_letters.is_empty() {
            return false;
        }
        let text = &document[range.clone()];
        let tokens: Vec<&str> = text
            .trim_matches(|c: char| c.is_whitespace() || matches!(c, ',' | ';' | ':'))
            .split_whitespace()
            .collect();
        if !(2..=3).contains(&tokens.len()) {
            return false;
        }
        let initials = |token: &str| {
            let chars: Vec<char> = token.chars().collect();
            !chars.is_empty()
                && chars.len().is_multiple_of(2)
                && chars
                    .chunks(2)
                    .all(|pair| pair[0].is_uppercase() && pair[1] == '.')
        };
        let mut read = Vec::new();
        // Each name word as written and read back, lowercase.
        let mut names: Vec<(String, String)> = Vec::new();
        let mut damaged = false;
        let mut alone = false;
        let last = tokens.len() - 1;
        for (index, token) in tokens.iter().enumerate() {
            if initials(token) {
                alone = true;
                read.push((*token).to_owned());
                continue;
            }
            // A comma after a surname ("M0rgan, Alice"), a sentence's
            // period after the last word.
            let word = if index == last {
                token.strip_suffix('.').unwrap_or(token)
            } else if let Some(word) = token.strip_suffix(',') {
                alone = true;
                word
            } else {
                token
            };
            let Some(name) = self.ocr_name_word(word) else {
                return false;
            };
            damaged |= digit_inside(word);
            let lower = name.to_lowercase();
            // A patronymic follows a name: «М0сквич Авто» is a car's.
            alone |= index > 0
                && self
                    .ocr_person_words
                    .iter()
                    .any(|pattern| pattern.is_match(&lower));
            names.push((word.to_lowercase(), lower));
            read.push(name);
        }
        if !damaged {
            return false;
        }
        let read = read.join(" ");
        if self.legal_form(text) || self.legal_form(&read) {
            return false;
        }
        let lower = |text: &str| -> Vec<String> {
            text.split_whitespace()
                .map(|word| {
                    let word = bare(word);
                    self.ocr_name_word(word).unwrap_or_else(|| word.to_owned())
                })
                .map(|word| word.to_lowercase())
                .collect()
        };
        let words = lower(&read);
        let company = |word: &String| {
            self.ocr_company_words.contains(word)
                || self
                    .initials_companies
                    .iter()
                    .any(|form| form.to_lowercase() == *word)
        };
        if words.iter().any(company) || lower(text).iter().any(company) {
            return false;
        }
        // Another span of this wording, damaged alike, is no other company.
        let named_company = organizations.iter().any(|other| {
            *other != text && {
                let other = lower(other);
                other.len() >= words.len() && other.starts_with(&words)
            }
        });
        if named_company
            || !self.unlabelled_shape(&read, Category::Person)
            || !self.identifying_named(&read, Category::Person, &|_| false, false)
        {
            return false;
        }
        alone || self.titled(document, range.start) || {
            let surname = &names.last().expect("a damaged name word").1;
            people.iter().any(|person| {
                let person = lower(person);
                person == words || person.contains(surname)
            }) || {
                static EMAIL: std::sync::LazyLock<Regex> = std::sync::LazyLock::new(|| {
                    Regex::new(r"([\p{L}\p{N}._%+-]+)@[\p{L}\p{N}-]+\.")
                        .expect("constant email pattern")
                });
                EMAIL.captures_iter(document).any(|captures| {
                    let local: Vec<String> = captures[1]
                        .split(|c: char| !c.is_alphanumeric())
                        .map(str::to_lowercase)
                        .collect();
                    names
                        .iter()
                        .all(|(written, read)| local.contains(written) || local.contains(read))
                })
            }
        }
    }

    /// `word` with its digits read back as `ocr_letters` letters, when that
    /// makes a capitalized name word: each part between hyphens a capital
    /// and small letters, two at least, an apostrophe's next letter either
    /// ("O'Br1en"). A digit opening a part reads as a capital, any other
    /// as a small letter, of the word's own script where it has one.
    fn ocr_name_word(&self, word: &str) -> Option<String> {
        let cyrillic = |c: char| ('\u{0400}'..='\u{04FF}').contains(&c);
        let in_cyrillic = word.chars().any(cyrillic);
        let mut read = String::with_capacity(word.len());
        for part in word.split('-') {
            if !read.is_empty() {
                read.push('-');
            }
            let mut letters = 0;
            let mut after_apostrophe = false;
            for (index, c) in part.chars().enumerate() {
                let capital = index == 0;
                let letter = if c.is_ascii_digit() {
                    let fits = |l: &char| {
                        if capital {
                            l.is_uppercase()
                        } else {
                            l.is_lowercase() || after_apostrophe && l.is_uppercase()
                        }
                    };
                    let options: Vec<char> = self
                        .ocr_letters
                        .iter()
                        .filter(|(digit, _)| *digit == c)
                        .flat_map(|(_, letters)| letters.iter().copied())
                        .filter(fits)
                        .collect();
                    options
                        .iter()
                        .find(|l| cyrillic(**l) == in_cyrillic)
                        .or(options.first())
                        .copied()?
                } else {
                    c
                };
                let fits = if letter == '\'' || letter == '’' {
                    !capital
                } else if capital {
                    letter.is_uppercase()
                } else {
                    letter.is_lowercase() || after_apostrophe && letter.is_uppercase()
                };
                if !fits {
                    return None;
                }
                after_apostrophe = letter == '\'' || letter == '’';
                letters += usize::from(letter.is_alphabetic());
                read.push(letter);
            }
            if letters < 2 || after_apostrophe {
                return None;
            }
        }
        Some(read)
    }
}

/// Whether `c` is a letter of the Cyrillic block (U+0400–U+04FF).
pub(super) fn cyrillic_letter(c: char) -> bool {
    ('\u{0400}'..='\u{04FF}').contains(&c) && c.is_alphabetic()
}
