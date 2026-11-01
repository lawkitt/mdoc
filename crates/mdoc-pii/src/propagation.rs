//! Propagation (ADR 0092): the forms of a known full person name elsewhere in
//! the document become proposals. A form with exactly one candidate full
//! name joins that identity; with two or more it keeps an identity of its
//! own (ADR 0068) and Same as… or the merge hint (ADR 0080) offers each.
//!
//! Forms: the surname alone (with a title), the first name alone, first name
//! and patronymic, the surname with initials, Russian case forms of each (a
//! fixed suffix table, not morphology) and nicknames from [`nicknames`].
use super::{
    Category, Recognizer, Review, exact_boundary,
    identities::{TITLES, cyrillic, name_words, split_full_name, surname_forms},
    intersects, safe_span,
    syntax::protected_syntax,
};
use std::{
    collections::{BTreeSet, HashSet},
    ops::Range,
    sync::{Arc, LazyLock},
};

/// Nicknames of a lowercase given name, lowercase. Only names already in the
/// document yield them. Data until the rule pack's `variants` section takes
/// it over (ADR 0092).
pub fn nicknames(given: &str) -> &'static [&'static str] {
    const TABLE: &[(&str, &[&str])] = &[
        // Russian
        ("александр", &["саша", "шура"]),
        ("александра", &["саша", "шура"]),
        ("алексей", &["лёша", "леша"]),
        ("анастасия", &["настя"]),
        ("андрей", &["андрюша"]),
        ("анна", &["аня"]),
        ("владимир", &["володя", "вова"]),
        ("галина", &["галя"]),
        ("дмитрий", &["дима"]),
        ("евгений", &["женя"]),
        ("евгения", &["женя"]),
        ("екатерина", &["катя"]),
        ("елена", &["лена"]),
        ("елизавета", &["лиза"]),
        ("иван", &["ваня"]),
        ("ирина", &["ира"]),
        ("константин", &["костя"]),
        ("ксения", &["ксюша"]),
        ("людмила", &["люда"]),
        ("мария", &["маша", "маруся"]),
        ("марина", &["мариша"]),
        ("михаил", &["миша"]),
        ("надежда", &["надя"]),
        ("наталья", &["наташа"]),
        ("николай", &["коля"]),
        ("ольга", &["оля"]),
        ("павел", &["паша"]),
        ("светлана", &["света"]),
        ("сергей", &["серёжа", "сережа"]),
        ("татьяна", &["таня"]),
        ("юлия", &["юля"]),
        // English
        ("alexander", &["alex"]),
        ("anthony", &["tony"]),
        ("benjamin", &["ben"]),
        ("catherine", &["cathy", "kate", "katie"]),
        ("charles", &["charlie", "chuck"]),
        ("christopher", &["chris"]),
        ("daniel", &["dan", "danny"]),
        ("deborah", &["debbie"]),
        ("edward", &["eddie", "ted", "ned"]),
        ("elizabeth", &["liz", "lizzie", "beth", "betty", "eliza"]),
        ("james", &["jim", "jimmy", "jamie"]),
        ("jennifer", &["jen", "jenny"]),
        ("john", &["jack", "johnny"]),
        ("jonathan", &["jon"]),
        ("joseph", &["joe", "joey"]),
        ("katherine", &["kate", "katie", "kathy"]),
        ("margaret", &["maggie", "meg", "peggy"]),
        ("matthew", &["matt"]),
        ("michael", &["mike", "mick"]),
        ("nicholas", &["nick"]),
        ("patricia", &["pat", "patty", "trish"]),
        ("rebecca", &["becky"]),
        ("richard", &["rick", "rich", "dick"]),
        ("robert", &["bob", "bobby", "rob", "robbie"]),
        ("samuel", &["sam"]),
        ("stephen", &["steve"]),
        ("steven", &["steve"]),
        ("susan", &["sue", "susie"]),
        ("thomas", &["tom", "tommy"]),
        ("victoria", &["vicky", "tori"]),
        ("william", &["bill", "billy", "will", "liam"]),
    ];
    TABLE
        .iter()
        .find(|(name, _)| *name == given)
        .map_or(&[], |(_, nicknames)| nicknames)
}

/// Lowercase forms that are also ordinary words: proposed only capitalised
/// mid-sentence, where the capital marks a name.
fn common_word(form: &str) -> bool {
    const WORDS: &[&str] = &[
        // English given names, nicknames and surnames
        "amber",
        "april",
        "art",
        "baker",
        "banks",
        "bell",
        "best",
        "bill",
        "bishop",
        "black",
        "brown",
        "bush",
        "carter",
        "case",
        "chase",
        "chuck",
        "church",
        "cook",
        "cross",
        "dawn",
        "dick",
        "faith",
        "field",
        "fisher",
        "fox",
        "frank",
        "frost",
        "ginger",
        "grace",
        "grant",
        "green",
        "guy",
        "hazel",
        "hill",
        "holly",
        "hope",
        "hunter",
        "iris",
        "ivy",
        "jack",
        "joy",
        "june",
        "king",
        "knight",
        "lamb",
        "lane",
        "law",
        "little",
        "long",
        "love",
        "mark",
        "mason",
        "may",
        "miles",
        "noble",
        "page",
        "park",
        "pat",
        "pearl",
        "penny",
        "porter",
        "price",
        "rich",
        "rob",
        "rose",
        "ruby",
        "short",
        "smith",
        "snow",
        "stone",
        "sue",
        "summer",
        "ward",
        "white",
        "will",
        "wolf",
        "wood",
        "young",
        // Russian given names and their forms
        "ада",
        "аврора",
        "вера",
        "веры",
        "вере",
        "веру",
        "верой",
        "лада",
        "лев",
        "лиана",
        "лилия",
        "лилии",
        "лилию",
        "любовь",
        "мила",
        "мира",
        "миры",
        "мире",
        "миру",
        "надежда",
        "надежды",
        "надежде",
        "надежду",
        "надеждой",
        "роза",
        "розы",
        "розе",
        "розу",
        "розой",
        "роман",
        "романа",
        "роману",
        "романом",
        "романе",
        "слава",
        "славы",
        "славе",
        "славу",
        "славой",
        "света",
        "свете",
        "свету",
    ];
    WORDS.contains(&form)
}

/// The forms of a Russian first name by the surname approach: a fixed table
/// of case endings ("марина" → "марины", "марине", "марину", "мариной").
/// A name in another script stands for itself.
pub(crate) fn given_forms(name: &str) -> Vec<String> {
    let mut forms = vec![name.to_owned()];
    if cyrillic(name) {
        let mut decline = |stem: &str, endings: &[&str]| {
            forms.extend(endings.iter().map(|e| format!("{stem}{e}")));
        };
        if let Some(stem) = name.strip_suffix("ия") {
            decline(stem, &["ии", "ию", "ией"]);
        } else if let Some(stem) = name.strip_suffix('а') {
            // "и" after г, к, х and hissing consonants; "ей" for "Маришей".
            let genitive = if stem.ends_with(['г', 'к', 'х', 'ж', 'ш', 'ч', 'щ']) {
                "и"
            } else {
                "ы"
            };
            decline(stem, &[genitive, "е", "у", "ой", "ей"]);
        } else if let Some(stem) = name.strip_suffix('я') {
            decline(stem, &["и", "е", "ю", "ей"]);
        } else if let Some(stem) = name.strip_suffix("ий") {
            decline(stem, &["ия", "ию", "ием", "ии"]);
        } else if let Some(stem) = name.strip_suffix(['й', 'ь']) {
            decline(stem, &["я", "ю", "ем", "е"]);
        } else if name
            .chars()
            .next_back()
            .is_some_and(|c| !"аеёиоуыэюя-".contains(c))
        {
            decline(name, &["а", "у", "ом", "е"]);
        }
    }
    forms
}

/// A Russian patronymic and its case forms ("сергеевна" → "сергеевны", …).
fn patronymic_forms(word: &str) -> Vec<String> {
    let mut forms = vec![word.to_owned()];
    if let Some(stem) = word.strip_suffix("на") {
        forms.extend(["ны", "не", "ну", "ной"].map(|e| format!("{stem}{e}")));
    } else if word.ends_with("ич") {
        forms.extend(["а", "у", "ем", "е"].map(|e| format!("{word}{e}")));
    }
    forms
}

/// The forms of `surname` that name its own bearer: a feminine surname's
/// masculine forms ("Павлов", "Павловым") name a relative, so they never
/// link to her, though they are still proposed.
fn own_surname_forms(surname: &str) -> Vec<String> {
    let all = surname_forms(surname);
    if !cyrillic(surname) {
        return all;
    }
    let feminine = ["ова", "ева", "ёва", "ина", "ына"]
        .iter()
        .any(|s| surname.ends_with(s));
    if !feminine {
        return all;
    }
    let stem = &surname[..surname.len() - 'а'.len_utf8()];
    all.into_iter()
        .filter(|form| {
            form == surname || *form == format!("{stem}ой") || *form == format!("{stem}у")
        })
        .collect()
}

/// A wording read as one of the forms propagation knows.
#[derive(Debug, PartialEq)]
pub(crate) enum Form {
    /// One name word, with a title before it or not.
    Word { word: String, titled: bool },
    /// First name + patronymic ("Марина Сергеевна").
    Patronymic { first: String, patronymic: String },
    /// A surname with initials before or after it ("Павловой М. С.").
    Initials {
        surname: String,
        initials: Vec<char>,
    },
    /// Two or three initials alone, as a signature ("E. N.", "Я. В."): the
    /// first name's and the patronymic's or middle name's, the surname's, or
    /// all three, in order.
    InitialsOnly(Vec<char>),
}

/// Initials in one word ("м.с", "m"), or `None` for a name word.
fn initials_of(word: &str) -> Option<Vec<char>> {
    let parts: Vec<_> = word.split('.').filter(|p| !p.is_empty()).collect();
    (!parts.is_empty() && parts.iter().all(|p| p.chars().count() == 1))
        .then(|| parts.iter().filter_map(|p| p.chars().next()).collect())
}

pub(crate) fn parse_form(wording: &str) -> Option<Form> {
    let titled = wording.split_whitespace().next().is_some_and(|first| {
        TITLES.contains(
            &first
                .trim_matches(|c: char| !c.is_alphabetic())
                .to_lowercase()
                .as_str(),
        )
    });
    let words = name_words(wording);
    let parsed: Vec<_> = words.iter().map(|w| initials_of(w)).collect();
    let names: Vec<&String> = words
        .iter()
        .zip(&parsed)
        .filter(|(_, i)| i.is_none())
        .map(|(w, _)| w)
        .collect();
    if names.iter().any(|w| {
        !w.chars()
            .all(|c| c.is_alphabetic() || matches!(c, '-' | '\'' | '’'))
    }) {
        return None;
    }
    let initials: Vec<char> = parsed.iter().flatten().flatten().copied().collect();
    match (names.as_slice(), initials.len()) {
        ([word], 0) => Some(Form::Word {
            word: (*word).clone(),
            titled,
        }),
        ([first, second], 0) if !titled && patronymic_like(second) => Some(Form::Patronymic {
            first: (*first).clone(),
            patronymic: (*second).clone(),
        }),
        ([], 2..=3) => Some(Form::InitialsOnly(initials)),
        ([surname], 1..=2)
            if parsed.first().is_some_and(Option::is_none)
                || parsed.last().is_some_and(Option::is_none) =>
        {
            Some(Form::Initials {
                surname: (*surname).clone(),
                initials,
            })
        }
        _ => None,
    }
}

/// A patronymic in any case ("сергеевны", "андреевичу").
pub(crate) fn patronymic_like(word: &str) -> bool {
    let stems = ["овн", "евн", "ичн", "ович", "евич"];
    cyrillic(word)
        && word.char_indices().any(|(i, _)| {
            stems.iter().any(|s| {
                word[i..].starts_with(s)
                    && word[i + s.len()..].chars().count() <= 3
                    && word[..i].chars().count() >= 2
            })
        })
}

/// One person a form may stand for, with the forms of their names.
struct Person {
    owner: u64,
    surname: String,
    /// Given names in order: the first name, then a middle name or patronymic.
    given: Vec<String>,
    /// A full name, so a form may join it; a surname with initials only
    /// blocks ("Петров И.И." beside "Петрова Марина": "Петрову" is either).
    target: bool,
    surname_forms: HashSet<String>,
    own_surname_forms: HashSet<String>,
    /// The first name's forms and its nicknames' (full names only).
    first_forms: HashSet<String>,
    patronymic_forms: HashSet<String>,
}
impl Person {
    fn new(owner: u64, surname: String, given: Vec<String>, target: bool) -> Self {
        let first_forms = match given.first() {
            Some(first) if target => given_forms(first)
                .into_iter()
                .chain(nicknames(first).iter().flat_map(|n| given_forms(n)))
                .collect(),
            _ => HashSet::new(),
        };
        let patronymic_forms = match given.as_slice() {
            [_, second] if target => patronymic_forms(second).into_iter().collect(),
            _ => HashSet::new(),
        };
        Self {
            owner,
            surname_forms: surname_forms(&surname).into_iter().collect(),
            own_surname_forms: own_surname_forms(&surname).into_iter().collect(),
            surname,
            given,
            target,
            first_forms,
            patronymic_forms,
        }
    }
}

/// The people of a document that forms are matched against.
pub(crate) struct People(Vec<Person>);

/// Who a form may stand for.
#[derive(Debug, Default, PartialEq)]
pub(crate) struct Candidates {
    /// Owners the form names in a matching gender and initials.
    pub(crate) link: BTreeSet<u64>,
    /// Some person has it as a form, whether or not it links.
    pub(crate) proposed: bool,
    /// The one full name it joins.
    pub(crate) target: Option<u64>,
}

impl People {
    /// `(owner, wording)` of every known person name: full names are
    /// targets, a surname with initials only a candidate.
    pub(crate) fn new<'a>(names: impl IntoIterator<Item = (u64, &'a str)>) -> Self {
        let mut people = Vec::new();
        for (owner, wording) in names {
            if let Some((surname, given)) = split_full_name(wording) {
                people.push(Person::new(owner, surname, given, true));
            } else if let Some(Form::Initials { surname, initials }) = parse_form(wording) {
                let given = initials.iter().map(char::to_string).collect();
                people.push(Person::new(owner, surname, given, false));
            }
        }
        Self(people)
    }
    pub(crate) fn is_empty(&self) -> bool {
        self.0.is_empty()
    }
    /// Whether `owner` is a full name's identity, which forms may join.
    pub(crate) fn is_target(&self, owner: u64) -> bool {
        self.0.iter().any(|p| p.owner == owner && p.target)
    }
    pub(crate) fn candidates(&self, form: &Form) -> Candidates {
        let mut found = Candidates::default();
        for person in &self.0 {
            // A name with initials does not block its own form.
            if let Form::Initials { surname, initials } = form
                && !person.target
                && person.surname == *surname
                && person
                    .given
                    .iter()
                    .map(|g| g.chars().next())
                    .eq(initials.iter().map(|c| Some(*c)))
            {
                continue;
            }
            let (links, proposes) = match form {
                Form::Word { word, titled } => {
                    let first = !titled && person.first_forms.contains(word);
                    (
                        first || person.own_surname_forms.contains(word),
                        first || person.surname_forms.contains(word),
                    )
                }
                Form::Patronymic { first, patronymic } => {
                    let both = person.first_forms.contains(first)
                        && person.patronymic_forms.contains(patronymic);
                    (both, both)
                }
                Form::Initials { surname, initials } => {
                    let named = person.surname_forms.contains(surname);
                    // Every initial matches, in order; one that matches a
                    // different given name breaks the link.
                    let matching = initials.len() <= person.given.len()
                        && initials
                            .iter()
                            .zip(&person.given)
                            .all(|(i, name)| name.starts_with(*i));
                    (
                        matching && person.own_surname_forms.contains(surname),
                        named,
                    )
                }
                Form::InitialsOnly(initials) => {
                    let first = |name: &String| name.chars().next();
                    let given: Vec<_> = person.given.iter().map(first).collect();
                    let surname = first(&person.surname);
                    let spelled: Vec<_> = initials.iter().map(|c| Some(*c)).collect();
                    // A name with initials ("Y. Quill") matches too, so it
                    // blocks a link, though it never becomes a target.
                    let matching = match spelled.as_slice() {
                        [a, b] => {
                            given.len() >= 2 && [given[0], given[1]] == [*a, *b]
                                || given.first() == Some(a) && surname == *b
                        }
                        [a, b, c] => {
                            given.len() >= 2 && [given[0], given[1], surname] == [*a, *b, *c]
                        }
                        _ => false,
                    };
                    (matching, matching)
                }
            };
            if links {
                found.link.insert(person.owner);
            }
            // A surname with initials alone proposes nothing: only full
            // names propagate.
            found.proposed |= person.target && (proposes || links);
        }
        if let [owner] = found.link.iter().copied().collect::<Vec<_>>()[..] {
            found.target = self
                .0
                .iter()
                .any(|p| p.owner == owner && p.target)
                .then_some(owner);
        }
        found
    }
    /// Whether `wording` is a form of the full name `full`, linked or not.
    pub(crate) fn form_of(wording: &str, full: &str) -> bool {
        parse_form(wording).is_some_and(|form| {
            let people = Self::new([(0, full)]);
            people.0.first().is_some_and(|p| p.target) && people.candidates(&form).proposed
        })
    }
}

/// `wording` as a form; `surname`: found only after a title, so a word is
/// a surname, never a first name.
fn form_in_context(wording: &str, surname: bool) -> Option<Form> {
    match parse_form(wording)? {
        Form::Word { word, .. } if surname => Some(Form::Word { word, titled: true }),
        form => Some(form),
    }
}

static TOKEN: LazyLock<regex::Regex> =
    LazyLock::new(|| regex::Regex::new(r"\p{L}+(?:['’-]\p{Lu}\p{L}*)*").expect("name token"));

/// Whether the text before `start` on its line ends a sentence: a line
/// start, a list marker, a table cell or sentence punctuation.
/// A courtesy title before a surname, in any case; lowercase "miss" is the
/// verb ("a near miss").
fn is_title(word: &str) -> bool {
    word != "miss" && TITLES.contains(&word.to_lowercase().as_str())
}

pub(crate) fn sentence_start(source: &str, start: usize) -> bool {
    let line = &source[source[..start].rfind('\n').map_or(0, |i| i + 1)..start];
    let before = line.trim_end_matches(|c: char| c.is_whitespace() || "*_\"'«“„‘([".contains(c));
    // "Mr. Hill": a title's period ends no sentence.
    let titled = before.strip_suffix('.').is_some_and(|b| {
        let word = b
            .rsplit(|c: char| !c.is_alphabetic())
            .next()
            .unwrap_or_default();
        is_title(word)
    });
    if titled {
        return false;
    }
    let marker = before.trim_start();
    marker.is_empty()
        || matches!(marker, "-" | "+" | ">")
        || marker.trim_start_matches('#').is_empty()
        || marker
            .strip_suffix(['.', ')'])
            .is_some_and(|n| !n.is_empty() && n.bytes().all(|b| b.is_ascii_digit()))
        || before.ends_with(['.', '!', '?', '…', '|'])
}

/// Spans of forms in `source`, outside `covered` and protected syntax, with
/// whether each is an ordinary word too. Longer forms win at each place.
fn find_forms(
    source: &str,
    people: &People,
    covered: &[Range<usize>],
) -> Vec<(Range<usize>, bool, bool)> {
    let protected = protected_syntax(source);
    let tokens: Vec<Range<usize>> = TOKEN.find_iter(source).map(|m| m.range()).collect();
    let capital = |k: usize| {
        tokens.get(k).is_some_and(|r| {
            source[r.clone()]
                .chars()
                .next()
                .is_some_and(char::is_uppercase)
        })
    };
    let blank = |from: usize, to: usize| {
        from <= to && source[from..to].chars().all(|c| c == ' ' || c == '\t')
    };
    // An initial: one capital letter and its period.
    let initial = |k: usize| {
        capital(k)
            && source[tokens[k].clone()].chars().count() == 1
            && source[tokens[k].end..].starts_with('.')
    };
    // A name word: a capital and more letters.
    let word = |k: usize| capital(k) && source[tokens[k].clone()].chars().count() > 1;
    let mut found = Vec::new();
    let mut i = 0;
    while i < tokens.len() {
        let start = tokens[i].start;
        // Candidate spans at `i`, longest first, each with the token after it.
        // `true`: a surname after a title, which stays outside the span.
        let mut tries: Vec<(Range<usize>, usize, bool)> = Vec::new();
        if initial(i) {
            // Initials before the surname: "М. С. Павлова", "M. Bennett".
            let last = if initial(i + 1) && blank(tokens[i].end + 1, tokens[i + 1].start) {
                i + 1
            } else {
                i
            };
            if word(last + 1) && blank(tokens[last].end + 1, tokens[last + 1].start) {
                tries.push((start..tokens[last + 1].end, last + 2, false));
            }
            // Initials alone, as a signature: "E. N.", "Я. В.". The whole
            // run or nothing: "J. S. X." is not "J. S." with a stray "X.".
            let mut run = i;
            while run < i + 3
                && initial(run + 1)
                && blank(tokens[run].end + 1, tokens[run + 1].start)
            {
                run += 1;
            }
            // Right after a name word they are that name's initials
            // ("Ветлин Н. О."), not a signature of their own.
            let after_name = i > 0
                && word(i - 1)
                && blank(tokens[i - 1].end, start)
                && (intersects(covered, &tokens[i - 1])
                    || parse_form(&source[tokens[i - 1].clone()])
                        .is_some_and(|form| people.candidates(&form).proposed));
            if (1..=2).contains(&(run - i)) && !after_name {
                tries.push((start..tokens[run].end + 1, run + 1));
            }
        } else {
            let title = &source[tokens[i].clone()];
            // Lowercase "miss" is the verb.
            let titled = is_title(title);
            // Initials after it: "Павловой М. С.", "Павлова М.С.".
            if word(i)
                && initial(i + 1)
                && tokens[i].end < tokens[i + 1].start
                && blank(tokens[i].end, tokens[i + 1].start)
            {
                if initial(i + 2) && blank(tokens[i + 1].end + 1, tokens[i + 2].start) {
                    tries.push((start..tokens[i + 2].end + 1, i + 3, false));
                }
                tries.push((start..tokens[i + 1].end + 1, i + 2, false));
            }
            // A title in any case and a surname ("Ms Raman", "dr. Raman"):
            // the surname alone, the title outside the span as the guide
            // keeps it; or first name + patronymic.
            if (titled || word(i)) && word(i + 1) {
                let gap =
                    tokens[i].end + usize::from(titled && source[tokens[i].end..].starts_with('.'));
                if gap < tokens[i + 1].start && blank(gap, tokens[i + 1].start) {
                    if titled {
                        tries.push((tokens[i + 1].clone(), i + 2, true));
                    } else {
                        tries.push((start..tokens[i + 1].end, i + 2, false));
                    }
                }
            }
            if word(i) {
                tries.push((tokens[i].clone(), i + 1, false));
            }
        }
        let accepted = tries.into_iter().find_map(|(range, next, after_title)| {
            let text = &source[range.clone()];
            // After a title the word is a surname, never a first name.
            let form = match parse_form(text)? {
                Form::Word { word, .. } if after_title => Form::Word { word, titled: true },
                _ if after_title => return None,
                form => form,
            };
            // Touching `_`, `/`, `@` or `\` it is part of an identifier, a
            // path or an address ("x.invalid/Alice_Morgan").
            let glued = |c: Option<char>| c.is_some_and(|c| "_/@\\".contains(c));
            let usable = !glued(source[..range.start].chars().next_back())
                && !glued(source[range.end..].chars().next())
                && safe_span(text)
                && !intersects(&protected, &range)
                && !intersects(covered, &range)
                && exact_boundary(source, &range, text)
                && people.candidates(&form).proposed;
            let common = matches!(&form, Form::Word { word, .. } if common_word(word));
            (usable && !(common && !after_title && sentence_start(source, range.start)))
                .then_some((range, common, next, after_title))
        });
        match accepted {
            Some((range, common, next, after_title)) => {
                found.push((range, common, after_title));
                i = next;
            }
            None => i += 1,
        }
    }
    found
}

impl Review {
    /// Full person names that propagate, as `(identity, wording)`: proposed
    /// (not kept) or applied. With a surname and initials, which only block.
    pub(super) fn known_people(&self) -> Vec<(u64, Arc<str>)> {
        let mut names: Vec<(u64, Arc<str>)> = self
            .variants()
            .iter()
            .filter(|v| v.category == Category::Person && !v.kept && !v.mentions.is_empty())
            .filter_map(|v| Some((self.variant_identity(v.id)?, v.original.clone())))
            .collect();
        names.extend(self.applied().iter().filter_map(|a| {
            let identity = a.step.identity;
            (self.identity(identity)?.category == Category::Person)
                .then(|| (identity, a.step.original_shared().clone()))
        }));
        let mut seen = HashSet::new();
        names.retain(|(id, wording)| seen.insert((*id, wording.clone())));
        names
    }
    fn people(&self) -> People {
        let names = self.known_people();
        People::new(names.iter().map(|(id, w)| (*id, w.as_ref())))
    }
    /// The identity a person wording joins: its one candidate full name.
    pub(super) fn propagation_target(&self, wording: &str) -> Option<u64> {
        let form = parse_form(wording)?;
        self.people().candidates(&form).target
    }
    /// Seed a form found by propagation: in its one candidate's identity,
    /// or an identity of its own.
    pub(super) fn seed_form(&mut self, wording: &str, target: Option<u64>) -> u64 {
        let id = self.add_seed_linked(wording, Category::Person, target);
        let seeded = self.variant(id).map(|v| v.identity);
        if let Some(variant) = self.candidates.variant_mut(id) {
            variant.managed = true;
            variant.own = (seeded == Some(id)).then_some(id);
        }
        id
    }
    /// Propose the forms of every known full name found outside existing
    /// mentions, and link earlier ones again. `marked`: a manual mark of a
    /// full name did it, as one undo step that withdraws the new forms and
    /// the links. Returns the variants added.
    pub(super) fn propagate(&mut self, source: &str, marked: bool) -> Vec<u64> {
        // Mentions of the seeds so far, so a form inside a longer wording
        // stays part of it.
        self.refresh(source);
        let people = self.people();
        self.relink_forms(&people, marked);
        if people.is_empty() {
            return Vec::new();
        }
        let mut spans: Vec<_> = self
            .candidates()
            .iter()
            .map(|c| c.range.clone())
            .chain(self.applied().iter().map(|a| a.range.clone()))
            .chain(self.tracking.exclusions.iter().map(|e| e.range.clone()))
            .collect();
        spans.sort_by_key(|r| r.start);
        let mut covered: Vec<Range<usize>> = Vec::with_capacity(spans.len());
        for span in spans {
            match covered.last_mut() {
                Some(last) if span.start < last.end => last.end = last.end.max(span.end),
                _ => covered.push(span),
            }
        }
        // Each wording once; a surname when every place it was found
        // follows a title ("Mr Paul": James Paul, not Paul Smith).
        let mut found: Vec<(&str, bool, bool)> = Vec::new();
        for (range, common, after_title) in find_forms(source, &people, &covered) {
            let wording = &source[range];
            match found.iter_mut().find(|(w, ..)| *w == wording) {
                Some((_, _, surname)) => *surname &= after_title,
                None => found.push((wording, common, after_title)),
            }
        }
        let mut added = Vec::new();
        for (wording, common, surname) in found {
            if self.candidates.by_original(wording).is_some() {
                continue;
            }
            let target =
                form_in_context(wording, surname).and_then(|form| people.candidates(&form).target);
            let id = self.seed_form(wording, target);
            if let Some(variant) = self.candidates.variant_mut(id) {
                variant.recognizer = Some(Recognizer::Propagation);
                variant.mid_sentence = common;
                variant.surname = surname;
                variant.keep_default = marked;
            }
            if marked {
                self.set_kept(id, false);
            }
            added.push(id);
        }
        if marked {
            self.refresh(source);
        }
        added
    }
    /// Link forms again as names come and go: a form joins its one
    /// candidate full name, and with none (another person shares it, or the
    /// name is gone) returns to an identity of its own. A scan's lone
    /// proposal that is a form joins too. The user's links stay, and so does
    /// a form that replaced text already. `marked`: as one undo step.
    fn relink_forms(&mut self, people: &People, marked: bool) {
        let moves: Vec<(u64, Option<u64>)> = self
            .variants()
            .iter()
            .filter(|v| {
                v.category == Category::Person
                    && !v.kept
                    && !self.identities.reassigned(v.id)
                    && (v.managed || self.loose(v))
                    && !self
                        .applied()
                        .iter()
                        .any(|a| a.step.original_shared().as_ref() == v.original.as_ref())
            })
            .filter_map(|v| {
                let target = form_in_context(&v.original, v.surname)
                    .and_then(|f| people.candidates(&f).target);
                let current = self.variant_identity(v.id)?;
                let desired = match target {
                    Some(target) => Some(target),
                    // A managed form leaves a full name it no longer fits.
                    None if v.managed && people.is_target(current) => None,
                    None => Some(current),
                };
                (desired != Some(current)).then_some((v.id, desired))
            })
            .collect();
        for (id, desired) in moves {
            let Some(variant) = self.variant(id) else {
                continue;
            };
            // Its own identity, or the one a respelling of it ("MS BENNETT")
            // left for already.
            let own = variant
                .own
                .or((variant.identity == variant.id).then_some(variant.id))
                .filter(|own| !people.is_target(*own))
                .or_else(|| {
                    self.identities
                        .normalized_identity(&variant.original, Category::Person)
                        .filter(|own| !people.is_target(*own))
                });
            let desired = match desired.or(own) {
                Some(identity) => identity,
                None => self.new_identity(Category::Person),
            };
            let Some(variant) = self.candidates.variant_mut(id) else {
                continue;
            };
            variant.managed = true;
            if !people.is_target(desired) {
                variant.own = Some(desired);
            }
            let (seed, original) = (variant.identity, variant.original.clone());
            if marked || self.identities.linked_automatically(id) {
                self.identities
                    .link_automatically(id, (desired != seed).then_some(desired));
            } else if let Some(variant) = self.candidates.variant_mut(id) {
                variant.identity = desired;
            }
            self.identities
                .remember_normalized(&original, Category::Person, desired);
        }
        self.sync_identity_variants();
    }
    /// A scan's person proposal with an identity of its own and nothing
    /// else in it: no other wording, no applied mention.
    fn loose(&self, v: &super::Variant) -> bool {
        v.detected
            && !v.managed
            && v.priority == 0
            && v.identity == v.id
            && !self.tracking.uses_identity(v.identity)
            && self.variants().iter().all(|other| {
                other.id == v.id || self.variant_identity(other.id) != Some(v.identity)
            })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn form(wording: &str) -> Form {
        parse_form(wording).unwrap_or_else(|| panic!("{wording} is no form"))
    }
    fn people(names: &[&str]) -> People {
        People::new(names.iter().enumerate().map(|(i, n)| (i as u64 + 1, *n)))
    }
    /// The full name a form joins, by its 1-based place in `names`.
    fn target(names: &[&str], wording: &str) -> Option<u64> {
        people(names).candidates(&form(wording)).target
    }

    #[test]
    fn nicknames_come_from_a_small_table() {
        assert_eq!(nicknames("марина"), ["мариша"]);
        assert_eq!(nicknames("elizabeth")[0], "liz");
        assert!(nicknames("maya").is_empty());
        assert!(nicknames("Марина").is_empty(), "lowercase only");
    }

    #[test]
    fn first_names_and_patronymics_decline_by_suffix() {
        let has = |name: &str, form: &str| given_forms(name).iter().any(|f| f == form);
        for form in ["марины", "марине", "марину", "мариной"] {
            assert!(has("марина", form), "{form}");
        }
        assert!(has("мариша", "маришей"));
        assert!(has("мариша", "мариши"), "и after ш");
        assert!(has("мария", "марией"));
        assert!(has("иван", "ивану"));
        assert!(has("дмитрий", "дмитрию"));
        assert!(has("сергей", "сергеем"));
        assert!(has("игорь", "игоря"));
        assert_eq!(given_forms("maya"), ["maya"]);
        assert!(patronymic_forms("сергеевна").contains(&"сергеевной".into()));
        assert!(patronymic_forms("андреевич").contains(&"андреевичу".into()));
        assert!(patronymic_like("андреевичу") && patronymic_like("сергеевны"));
        assert!(!patronymic_like("марина") && !patronymic_like("павлова"));
    }

    #[test]
    fn wordings_read_as_forms() {
        assert_eq!(
            form("Ms Bennett"),
            Form::Word {
                word: "bennett".into(),
                titled: true
            }
        );
        assert_eq!(
            form("Марина Сергеевна"),
            Form::Patronymic {
                first: "марина".into(),
                patronymic: "сергеевна".into()
            }
        );
        let initials = |surname: &str, initials: &[char]| Form::Initials {
            surname: surname.into(),
            initials: initials.to_vec(),
        };
        assert_eq!(form("Павловой М. С."), initials("павловой", &['м', 'с']));
        assert_eq!(form("Павлова М.С."), initials("павлова", &['м', 'с']));
        assert_eq!(form("М.С. Павлова"), initials("павлова", &['м', 'с']));
        assert_eq!(form("M. Bennett"), initials("bennett", &['m']));
        // Two names without a patronymic, or initials between names, are not
        // forms: a full name is one.
        assert_eq!(parse_form("Maya Bennett"), None);
        assert_eq!(parse_form("Павлова Марина Сергеевна"), None);
        assert_eq!(parse_form("John F. Kennedy"), None);
    }

    #[test]
    fn a_form_with_one_candidate_joins_it() {
        let names = ["Павлова Марина Сергеевна", "Шахова Ирина Олеговна"];
        for wording in [
            "Павлова",
            "Павловой",
            "Павлову",
            "Марина",
            "Марине",
            "Маришей",
            "Марина Сергеевна",
            "Марины Сергеевны",
            "Павловой М. С.",
            "М.С. Павлова",
            "Павлова М.",
        ] {
            assert_eq!(target(&names, wording), Some(1), "{wording}");
        }
        assert_eq!(target(&names, "Ирину"), Some(2));
        let names = ["Maya Bennett"];
        for wording in ["Bennett", "Ms Bennett", "Maya", "M. Bennett"] {
            assert_eq!(target(&names, wording), Some(1), "{wording}");
        }
        assert_eq!(target(&["Elizabeth Moore"], "Liz"), Some(1));
        // A title goes with a surname, not a first name.
        assert_eq!(target(&names, "Ms Maya"), None);
    }

    #[test]
    fn two_candidates_or_another_gender_keep_a_form_apart() {
        // Relatives sharing a surname.
        let family = ["Maya Bennett", "Daniel Bennett"];
        let both = people(&family).candidates(&form("Ms Bennett"));
        assert_eq!(
            (both.link.len(), both.target, both.proposed),
            (2, None, true)
        );
        // "Петрова" is her name and his genitive.
        assert_eq!(target(&["Петров Иван", "Петрова Марина"], "Петрова"), None);
        // A masculine form names her relative: proposed, never linked to her.
        let he = people(&["Павлова Марина Сергеевна"]).candidates(&form("Павлов"));
        assert_eq!((he.target, he.proposed), (None, true));
        assert_eq!(target(&["Павлова Марина Сергеевна"], "Павловым"), None);
        // A first name two people share; a surname that is another's first name.
        assert_eq!(target(&["Paul Smith", "Paul Jones"], "Paul"), None);
        assert_eq!(target(&["Paul Smith", "James Paul"], "Paul"), None);
        // A relative named by initials is a candidate, never a target.
        assert_eq!(
            target(&["Петров И.И.", "Петрова Марина Сергеевна"], "Петрову"),
            None
        );
        assert_eq!(target(&["Петров И.И."], "Петрову"), None);
    }

    #[test]
    fn every_initial_must_match_and_a_different_one_breaks_the_link() {
        let names = ["Павлова Марина Сергеевна"];
        let other = people(&names).candidates(&form("Павлова А. С."));
        assert_eq!(
            (other.target, other.proposed),
            (None, true),
            "proposed apart"
        );
        assert_eq!(target(&names, "Павлова М. А."), None);
        assert_eq!(
            target(&names, "Павлова С."),
            None,
            "the first initial first"
        );
        // Namesakes with the same initials.
        let names = ["Павлова Марина Сергеевна", "Павлова Мария Степановна"];
        assert_eq!(target(&names, "Павлова М.С."), None);
        assert_eq!(target(&names, "Павлова М."), None);
        assert_eq!(
            target(&["Maya Bennett", "Daniel Bennett"], "D. Bennett"),
            Some(2)
        );
    }

    #[test]
    fn initials_alone_name_the_one_person_they_fit() {
        assert_eq!(form("E. N."), Form::InitialsOnly(vec!['e', 'n']));
        assert_eq!(form("Я.В."), Form::InitialsOnly(vec!['я', 'в']));
        assert_eq!(parse_form("E."), None, "one initial is too little");
        // First name and surname, first name and patronymic, or all three.
        assert_eq!(target(&["Ysolde Qhenn"], "Y. Q."), Some(1));
        let names = ["Яромир Всеволодович Фьорд-Лукин", "Шахова Ирина Олеговна"];
        assert_eq!(target(&names, "Я. В."), Some(1));
        assert_eq!(target(&names, "Я. Ф."), Some(1));
        assert_eq!(target(&names, "Я. В. Ф."), Some(1));
        assert_eq!(target(&names, "И. О."), Some(2));
        // Out of order or another person's: nothing.
        assert_eq!(target(&names, "В. Я."), None);
        let other = people(&names).candidates(&form("Y. Q."));
        assert_eq!((other.target, other.proposed), (None, false));
        // Two people with the same initials, or one named by initials
        // only: proposed apart.
        let both = people(&["Yara Quill", "Ysolde Qhenn"]).candidates(&form("Y. Q."));
        assert_eq!((both.target, both.proposed), (None, true));
        let abbreviated = people(&["Ysolde Qhenn", "Y. Quill"]).candidates(&form("Y. Q."));
        assert_eq!((abbreviated.target, abbreviated.proposed), (None, true));
        // A third initial that does not fit breaks the match.
        assert_eq!(target(&["John Smith"], "J. S. X."), None);
    }

    #[test]
    fn initials_alone_are_found_with_their_last_period() {
        let names = ["Ysolde Qhenn", "Яромир Всеволодович Фьорд-Лукин"];
        let source = "Signed Y. Q. on the sheet; подпись Я. В. в журнале; \
            clause A. B. and Y. alone stay; so do Y. Q. X. and Y. Q. Z. W.";
        let found: Vec<_> = find_forms(source, &people(&names), &[])
            .into_iter()
            .map(|(r, _)| &source[r])
            .collect();
        assert_eq!(found, ["Y. Q.", "Я. В."]);
        // After a surname they are its initials, not a signature.
        let names = ["Надир Оскарович Ветлин"];
        let source = "Заявитель: Ветлин Н. О.";
        let covered = [Range {
            start: source.find("Ветлин").unwrap(),
            end: source.find(" Н.").unwrap(),
        }];
        assert!(find_forms(source, &people(&names), &covered).is_empty());
    }

    #[test]
    fn ordinary_words_count_only_capitalised_mid_sentence() {
        let names = ["Rose Hill"];
        let found = |source: &str| -> Vec<String> {
            find_forms(source, &people(&names), &[])
                .into_iter()
                .map(|(r, ..)| source[r].to_owned())
                .collect()
        };
        assert_eq!(found("We met Rose. Rose agreed; a rose."), ["Rose"]);
        assert_eq!(
            found("- Rose left\n1. Rose\n| Rose |"),
            Vec::<String>::new()
        );
        // A title marks a surname wherever it stands, and stays outside the
        // span (guide rule 1); a title's period ends no sentence.
        assert_eq!(found("Call Ms Hill. Hill agreed."), ["Hill"]);
        assert_eq!(found("Mr. Hill agreed."), ["Hill"]);
        assert!(!sentence_start("Mr. Hill", 4));
        assert!(sentence_start("**Rose**", 2));
        assert!(!sentence_start("said “Rose", 6 + "“".len()));
        // A name that is no ordinary word counts anywhere.
        let names = ["Павлова Марина Сергеевна"];
        let source = "Марина ушла. Мира нет.";
        let found: Vec<_> = find_forms(source, &people(&names), &[])
            .into_iter()
            .map(|(r, common, _)| (&source[r], common))
            .collect();
        assert_eq!(found, [("Марина", false)]);
    }

    #[test]
    fn longer_forms_win_at_each_place() {
        let names = ["Павлова Марина Сергеевна"];
        let source = "Павловой М. С. и М.С. Павлова; Марина Сергеевна, Марине и Павловой.";
        let found: Vec<_> = find_forms(source, &people(&names), &[])
            .into_iter()
            .map(|(r, ..)| &source[r])
            .collect();
        assert_eq!(
            found,
            [
                "Павловой М. С.",
                "М.С. Павлова",
                "Марина Сергеевна",
                "Марине",
                "Павловой"
            ]
        );
    }
}
