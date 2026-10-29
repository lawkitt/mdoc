//! Merge hints (ADR 0080): two identities that very likely name the same
//! subject. A hint is shown, never applied; nothing links without a click.
use super::{
    Category, Review,
    identities::{cyrillic, name_words, split_full_name, surname_forms},
};
use std::{
    collections::{HashMap, HashSet},
    sync::{Arc, LazyLock},
};

static SPELLING_SEPARATORS: LazyLock<regex::Regex> =
    LazyLock::new(|| regex::Regex::new(r"[\p{P}\s]").expect("punctuation and whitespace"));

/// One identity as hints see it. Slice order is identity order: of two
/// equally full wordings, the later identity shows the hint.
#[derive(Clone, Debug)]
pub struct HintIdentity {
    pub id: u64,
    pub category: Category,
    /// Original wordings, the primary wording first.
    pub wordings: Vec<Arc<str>>,
}

/// Why two identities look like one subject.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Signal {
    /// An ADR 0068 short or inflected form with exactly one candidate full name.
    ShortForm,
    /// Wordings that differ only in case, whitespace or punctuation.
    Spelling,
    /// The same Russian name words in another order.
    Reversed,
    /// An OCR or typing slip: one edit apart, both names 6+ characters.
    NearDuplicate,
}

#[derive(Clone, Debug, PartialEq)]
pub struct MergeHint {
    /// The shorter (or later) identity, under which the hint shows.
    pub from: u64,
    /// The fuller identity, which keeps its alias and primary wording.
    pub into: u64,
    /// The fuller identity's primary wording: "Looks like …".
    pub wording: Arc<str>,
    /// The identities' primary wordings, `from`'s first. Dismissals hold this
    /// stable pair even when several variants provide matching signals.
    pub pair: (Arc<str>, Arc<str>),
    pub signal: Signal,
}

struct Found {
    from: usize,
    into: usize,
    signal: Signal,
}

type Wording<'a> = (usize, &'a Arc<str>);
type SpellingGroups<'a> = HashMap<(Category, String), Vec<Wording<'a>>>;
type OrderGroups<'a> = HashMap<Vec<String>, Vec<(usize, &'a Arc<str>, Vec<String>)>>;

/// Every hint between `identities`, at most one per pair (the strongest
/// signal), ordered by the identity that shows it.
pub fn merge_hints(identities: &[HintIdentity]) -> Vec<MergeHint> {
    let mut found: HashMap<(usize, usize), Found> = HashMap::new();
    let mut record = |from: (usize, &Arc<str>), into: (usize, &Arc<str>), signal| {
        if from.0 != into.0 {
            let key = (from.0.min(into.0), from.0.max(into.0));
            let (from, into) = orient(identities, from.0, into.0);
            found.entry(key).or_insert(Found { from, into, signal });
        }
    };
    let wordings = || {
        identities.iter().enumerate().flat_map(|(i, identity)| {
            identity
                .wordings
                .iter()
                .filter(move |w| identity.category != Category::Person || !has_initials(w))
                .map(move |w| (i, identity.category, w))
        })
    };

    // Short and inflected forms: the full names each surname form stands for.
    let mut forms: HashMap<String, Vec<(usize, &Arc<str>)>> = HashMap::new();
    let mut given = HashSet::new();
    for (i, category, wording) in wordings() {
        if category == Category::Person
            && let Some((surname, names)) = split_full_name(wording)
        {
            given.extend(names);
            for form in surname_forms(&surname) {
                forms.entry(form).or_default().push((i, wording));
            }
        }
    }
    forms.retain(|form, _| !given.contains(form));
    for (i, category, wording) in wordings() {
        let words = name_words(wording);
        if category != Category::Person || words.len() != 1 {
            continue;
        }
        let Some(fulls) = forms.get(&words[0]) else {
            continue;
        };
        // Relatives sharing a surname give two candidates: no hint. A form
        // of the identity's own full name is linked already.
        let owners: HashSet<usize> = fulls.iter().map(|(owner, _)| *owner).collect();
        if owners.len() == 1 && !owners.contains(&i) {
            record((i, wording), fulls[0], Signal::ShortForm);
        }
    }

    // Case, whitespace and punctuation, in any category.
    let mut spellings = SpellingGroups::new();
    for (i, category, wording) in wordings() {
        let key = SPELLING_SEPARATORS.replace_all(wording, "").to_lowercase();
        if !key.is_empty() {
            spellings
                .entry((category, key))
                .or_default()
                .push((i, wording));
        }
    }
    for group in spellings.values() {
        for (n, a) in group.iter().enumerate() {
            // Identical wordings in two identities were split on purpose.
            for b in group[n + 1..].iter().filter(|b| b.1 != a.1) {
                record(*a, *b, Signal::Spelling);
            }
        }
    }

    // Russian names in reversed order: "Павлова Марина" / "Марина Павлова".
    let mut orders = OrderGroups::new();
    for (i, category, wording) in wordings() {
        let words = name_words(wording);
        if category == Category::Person
            && (2..=3).contains(&words.len())
            && words
                .iter()
                .all(|w| cyrillic(w) && w.chars().filter(|c| c.is_alphabetic()).count() > 1)
        {
            let mut key = words.clone();
            key.sort_unstable();
            orders.entry(key).or_default().push((i, wording, words));
        }
    }
    for group in orders.values() {
        for (n, a) in group.iter().enumerate() {
            for b in group[n + 1..].iter().filter(|b| b.2 != a.2) {
                record((a.0, a.1), (b.0, b.1), Signal::Reversed);
            }
        }
    }

    // Near-duplicates: wordings one deletion apart share a key, so only
    // those are compared.
    let mut deletions = SpellingGroups::new();
    for (i, category, wording) in wordings() {
        let chars: Vec<char> = wording.to_lowercase().chars().collect();
        if wording.chars().count() < 6 {
            continue;
        }
        for skip in 0..=chars.len() {
            let key = chars
                .iter()
                .enumerate()
                .filter(|(k, _)| *k != skip)
                .map(|(_, c)| c)
                .collect();
            deletions
                .entry((category, key))
                .or_default()
                .push((i, wording));
        }
    }
    let mut compared = HashSet::new();
    for group in deletions.values() {
        for (n, a) in group.iter().enumerate() {
            for b in &group[n + 1..] {
                if a.0 != b.0
                    && compared.insert((a.0, a.1.clone(), b.0, b.1.clone()))
                    && near_duplicate(a.1, b.1, identities[a.0].category)
                {
                    record(*a, *b, Signal::NearDuplicate);
                }
            }
        }
    }

    let mut hints: Vec<_> = found.into_values().collect();
    hints.sort_by_key(|f| (f.from, f.into));
    hints
        .into_iter()
        .map(|f| MergeHint {
            from: identities[f.from].id,
            into: identities[f.into].id,
            wording: identities[f.into].wordings[0].clone(),
            pair: (
                identities[f.from].wordings[0].clone(),
                identities[f.into].wordings[0].clone(),
            ),
            signal: f.signal,
        })
        .collect()
}

/// (shorter, fuller): compare whole identities, not the variants providing
/// the signal. Equal primary lengths keep the earlier identity.
fn orient(identities: &[HintIdentity], a: usize, b: usize) -> (usize, usize) {
    let length = |i: usize| identities[i].wordings[0].chars().count();
    let (la, lb) = (length(a), length(b));
    if la > lb || (la == lb && a < b) {
        (b, a)
    } else {
        (a, b)
    }
}

fn has_initials(wording: &str) -> bool {
    name_words(wording).iter().any(|word| {
        word.chars().filter(|c| c.is_alphabetic()).count() == 1
            || (word.contains('.') && word.split('.').any(|part| part.chars().count() == 1))
    })
}

/// One edit apart inside one word, both wordings 6+ characters. Initials,
/// numbers and a feminine surname ending ("Петров" / "Петрова", relatives
/// as often as not) are not slips.
fn near_duplicate(a: &str, b: &str, category: Category) -> bool {
    if a.chars().count() < 6 || b.chars().count() < 6 {
        return false;
    }
    let words = |s: &str| -> Vec<String> { s.split_whitespace().map(str::to_lowercase).collect() };
    let (wa, wb) = (words(a), words(b));
    if wa.len() != wb.len() {
        return false;
    }
    let mut differing = wa.iter().zip(&wb).filter(|(x, y)| x != y);
    let (Some((x, y)), None) = (differing.next(), differing.next()) else {
        return false;
    };
    let letters = |w: &str| w.chars().filter(|c| c.is_alphabetic()).count();
    let numeric = |w: &str| w.chars().any(char::is_numeric);
    let feminine = |short: &str, long: &str| {
        long.strip_suffix(['а', 'a'])
            .is_some_and(|stem| stem == short)
    };
    letters(x) >= 2
        && letters(y) >= 2
        && !(numeric(x) && numeric(y))
        && !(category == Category::Person && (feminine(x, y) || feminine(y, x)))
        && one_edit(x, y)
}

/// Exactly one insertion, deletion or substitution apart.
fn one_edit(a: &str, b: &str) -> bool {
    let a: Vec<char> = a.chars().collect();
    let b: Vec<char> = b.chars().collect();
    let (short, long) = if a.len() <= b.len() {
        (&a, &b)
    } else {
        (&b, &a)
    };
    if long.len() - short.len() > 1 {
        return false;
    }
    let prefix = short.iter().zip(long).take_while(|(x, y)| x == y).count();
    if short.len() == long.len() {
        prefix < short.len() && short[prefix + 1..] == long[prefix + 1..]
    } else {
        short[prefix..] == long[prefix + 1..]
    }
}

impl Review {
    /// Merge hints between the identities this document still replaces, in
    /// identity order (ADR 0080).
    pub fn merge_hints(&self) -> Vec<MergeHint> {
        let identities: Vec<_> = self
            .export_identities_with_ids()
            .into_iter()
            .map(|(id, e)| HintIdentity {
                id,
                category: e.category,
                wordings: e.originals,
            })
            .collect();
        merge_hints(&identities)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{Detection, Recognizer};

    fn identities(list: &[(Category, &[&str])]) -> Vec<HintIdentity> {
        list.iter()
            .enumerate()
            .map(|(i, (category, wordings))| HintIdentity {
                id: i as u64 + 1,
                category: *category,
                wordings: wordings.iter().map(|w| Arc::from(*w)).collect(),
            })
            .collect()
    }
    fn people(names: &[&str]) -> Vec<HintIdentity> {
        let list: Vec<(Category, &[&str])> = names
            .iter()
            .map(|n| (Category::Person, std::slice::from_ref(n)))
            .collect();
        identities(&list)
    }
    /// (from, into, signal) of every hint.
    fn hints(list: &[HintIdentity]) -> Vec<(u64, u64, Signal)> {
        merge_hints(list)
            .into_iter()
            .map(|h| (h.from, h.into, h.signal))
            .collect()
    }

    #[test]
    fn a_short_form_with_one_full_name_hints_at_it() {
        let list = people(&["Maya Bennett", "Ms Bennett"]);
        let all = merge_hints(&list);
        assert_eq!(all.len(), 1);
        let hint = &all[0];
        assert_eq!((hint.from, hint.into), (2, 1));
        assert_eq!(hint.signal, Signal::ShortForm);
        assert_eq!(hint.wording.as_ref(), "Maya Bennett");
        assert_eq!(
            (hint.pair.0.as_ref(), hint.pair.1.as_ref()),
            ("Ms Bennett", "Maya Bennett")
        );
        // Russian inflected forms, the full name in either order.
        assert_eq!(
            hints(&people(&["Павлова Марина Сергеевна", "Павловой"])),
            [(2, 1, Signal::ShortForm)]
        );
        assert_eq!(
            hints(&people(&["Рамана", "Иван Раман"])),
            [(1, 2, Signal::ShortForm)],
            "the shorter identity shows the hint, even when it came first"
        );
    }

    #[test]
    fn relatives_sharing_a_surname_give_no_hint() {
        assert!(hints(&people(&["Maya Bennett", "Daniel Bennett", "Ms Bennett"])).is_empty());
        assert!(hints(&people(&["Петров Иван", "Петрова Марина", "Петрова"])).is_empty());
        // A form of the identity's own full name is already linked.
        let list = identities(&[
            (Category::Person, &["Maya Bennett", "Ms Bennett"]),
            (Category::Person, &["Daniel Bennett"]),
        ]);
        assert!(hints(&list).is_empty());
        assert!(hints(&people(&["Paul Smith", "James Paul", "Paul"])).is_empty());
    }

    #[test]
    fn case_whitespace_and_punctuation_hint_in_any_category() {
        let list = identities(&[
            (Category::Organization, &["Northwind Ltd"]),
            (Category::Organization, &["Northwind Ltd."]),
            (Category::Email, &["Maya@Example.com"]),
            (Category::Email, &["maya@example.com"]),
            (Category::Phone, &["+7 900 123-45-67"]),
            (Category::Phone, &["+79001234567"]),
        ]);
        assert_eq!(
            hints(&list),
            [
                (1, 2, Signal::Spelling),
                (4, 3, Signal::Spelling),
                (6, 5, Signal::Spelling),
            ],
            "the shorter wording, or of equals the later identity, shows it"
        );
        // Another category is another subject; identical wordings were split
        // on purpose.
        let list = identities(&[
            (Category::Organization, &["Northwind"]),
            (Category::Person, &["NORTHWIND"]),
            (Category::Person, &["Maya Bennett"]),
            (Category::Person, &["Maya Bennett"]),
        ]);
        assert!(hints(&list).is_empty());
        let symbols = identities(&[
            (Category::Organization, &["Acme™"]),
            (Category::Organization, &["Acme"]),
        ]);
        assert!(hints(&symbols).is_empty(), "symbols are not punctuation");
    }

    #[test]
    fn one_slip_in_a_long_name_hints_but_not_in_a_short_one() {
        assert_eq!(
            hints(&people(&["Maya Bennett", "Maya Benett"])),
            [(2, 1, Signal::NearDuplicate)]
        );
        let list = identities(&[
            (Category::Organization, &["N0rthwind"]),
            (Category::Organization, &["Northwind"]),
        ]);
        assert_eq!(hints(&list), [(2, 1, Signal::NearDuplicate)], "OCR 0 for O");
        // Five letters: "Smith" / "Smyth" are different people often enough.
        assert!(hints(&people(&["Smith", "Smyth"])).is_empty());
        assert!(hints(&people(&["Brown", "Brawn"])).is_empty());
        // Two edits, numbers and initials are not slips.
        assert!(hints(&people(&["Maya Bennett", "Maia Benett"])).is_empty());
        let list = identities(&[
            (Category::Bank, &["40702810900000000001"]),
            (Category::Bank, &["40702810900000000002"]),
            (Category::Address, &["221B Baker Street"]),
            (Category::Address, &["221C Baker Street"]),
        ]);
        assert!(hints(&list).is_empty());
        assert!(hints(&people(&["A. Bennett", "B. Bennett"])).is_empty());
        // A feminine ending names a relative as often as a slip.
        assert!(hints(&people(&["Петров И.И.", "Петрова И.И."])).is_empty());
    }

    #[test]
    fn reversed_russian_name_order_hints() {
        assert_eq!(
            hints(&people(&["Павлова Марина", "Марина Павлова"])),
            [(2, 1, Signal::Reversed)]
        );
        assert_eq!(
            hints(&people(&[
                "Павлова Марина Сергеевна",
                "Марина Сергеевна Павлова"
            ])),
            [(2, 1, Signal::Reversed)]
        );
        // Latin names keep their order meaning; different words are others.
        assert!(hints(&people(&["Bennett Maya", "Maya Bennett"])).is_empty());
        assert!(hints(&people(&["Павлова Марина", "Марина Петрова"])).is_empty());
    }

    #[test]
    fn initials_and_transliteration_are_not_signals() {
        assert!(hints(&people(&["Павлова Марина Сергеевна", "Павлова М.С."])).is_empty());
        assert!(hints(&people(&["Maya Bennett", "M. Bennett"])).is_empty());
        assert!(hints(&people(&["M. Bennett", "M. Benett"])).is_empty());
        assert!(hints(&people(&["M. Bennett", "M Bennett"])).is_empty());
        assert!(hints(&people(&["Павлова М.С.", "М.С. Павлова"])).is_empty());
        assert_eq!(
            hints(&people(&["Maya O'Neil", "Maya O’Neil"])),
            [(2, 1, Signal::Spelling)]
        );

        assert!(hints(&people(&["Марина Павлова", "Marina Pavlova"])).is_empty());
        assert!(hints(&people(&["Павлова", "Marina Pavlova"])).is_empty());
    }

    #[test]
    fn every_hint_shows_once_on_its_own_identity() {
        // A chain: each shorter identity points at a fuller one, once.
        let list = identities(&[
            (Category::Organization, &["Northwind Traders Ltd."]),
            (Category::Organization, &["Northwind Traders Ltd"]),
            (Category::Person, &["Maya Bennett"]),
            (Category::Person, &["Ms Bennett"]),
        ]);
        assert_eq!(
            hints(&list),
            [(2, 1, Signal::Spelling), (4, 3, Signal::ShortForm)]
        );
        // The strongest signal names a pair that matches several ways.
        let list = people(&["Maya Bennett.", "maya bennett"]);
        assert_eq!(hints(&list), [(2, 1, Signal::Spelling)]);
    }

    #[test]
    fn matching_a_short_variant_still_keeps_the_fuller_identity() {
        let list = identities(&[
            (Category::Organization, &["Northwind Traders Ltd", "NW Ltd"]),
            (Category::Organization, &["NW Ltd."]),
        ]);
        let hint = merge_hints(&list).remove(0);
        assert_eq!((hint.from, hint.into), (2, 1));
        assert_eq!(hint.wording.as_ref(), "Northwind Traders Ltd");
        assert_eq!(
            hint.pair,
            (Arc::from("NW Ltd."), Arc::from("Northwind Traders Ltd"))
        );
    }

    #[test]
    fn dismissal_pair_is_stable_across_matching_variants_and_recomputations() {
        let list = identities(&[
            (Category::Person, &["Maya Bennett", "Ms Bennett"]),
            (Category::Person, &["Maya Bennett.", "Ms Bennett."]),
        ]);
        let expected = merge_hints(&list);
        for _ in 0..32 {
            assert_eq!(merge_hints(&list), expected);
        }
        assert_eq!(
            expected[0].pair,
            (Arc::from("Maya Bennett"), Arc::from("Maya Bennett."))
        );
    }

    #[test]
    fn a_review_hints_at_a_proposed_short_form_until_merged() {
        let source = "Maya Bennett signed. Later Ms Bennett agreed.";
        let mut review = Review::default();
        review
            .ingest(
                source,
                vec![Detection {
                    range: 0..12,
                    category: Category::Person,
                    score: 0.9,
                    recognizer: Recognizer::Model,
                }],
            )
            .unwrap();
        let all = review.merge_hints();
        assert_eq!(all.len(), 1, "{all:?}");
        let hint = &all[0];
        assert_eq!(hint.wording.as_ref(), "Maya Bennett");
        assert_eq!(hint.pair.0.as_ref(), "Ms Bennett");
        let full = review.identity(hint.into).unwrap().alias.clone();
        review.merge_identity(hint.from, hint.into).unwrap();
        assert!(review.merge_hints().is_empty());
        let variants: Vec<_> = review
            .variants()
            .iter()
            .map(|v| (v.original.to_string(), v.replacement.clone()))
            .collect();
        assert_eq!(
            variants,
            [
                ("Maya Bennett".to_string(), full.clone()),
                ("Ms Bennett".to_string(), full)
            ]
        );
    }
}
