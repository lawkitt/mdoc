#![allow(clippy::single_range_in_vec_init)] // exclusion lists with one range read naturally

use super::*;

fn speller() -> Speller {
    Speller::load(Options::default())
}

/// The flagged words, for readable assertions.
fn flagged(speller: &Speller, text: &str, exclusions: Exclusions) -> Vec<String> {
    speller
        .check(text, exclusions)
        .into_iter()
        .map(|r| text[r].to_owned())
        .collect()
}

fn plain(text: &str) -> Vec<String> {
    flagged(&speller(), text, Exclusions::default())
}

#[test]
fn speller_is_shareable_across_threads() {
    fn assert_send_sync<T: Send + Sync>() {}
    assert_send_sync::<Speller>();
}

#[test]
fn flags_english_and_russian_in_one_document() {
    assert_eq!(
        plain("The parties agre to the termz.\nСтороны обязуютса соблюдать договор."),
        ["agre", "termz", "обязуютса"]
    );
}

#[test]
fn accepts_both_us_and_uk_spellings() {
    assert!(plain("The licence and the license; the judgement and the judgment.").is_empty());
    assert!(plain("Organisation, organization, colour, color.").is_empty());
}

#[test]
fn russian_treats_yo_as_ye_for_lookup() {
    assert!(plain("Ещё ещё еще всё").is_empty());
}

#[test]
fn single_letters_and_abbreviation_parts_are_skipped() {
    assert!(plain("т.е. г. Москва, п. 3, ч. 2; a, I").is_empty());
}

#[test]
fn all_caps_and_digits_follow_options() {
    let text = "THE AGREMENT covers c1ient data.";
    assert!(plain(text).is_empty());
    let strict = Speller::load(Options {
        check_all_caps: true,
        check_digits: true,
        ..Options::default()
    });
    assert_eq!(
        flagged(&strict, text, Exclusions::default()),
        ["AGREMENT", "c1ient"]
    );
    assert!(
        flagged(
            &strict,
            "On the 21st of May, 1990s, item 12.",
            Exclusions::default()
        )
        .is_empty()
    );
}

#[test]
fn mixed_script_words_are_flagged_even_inside_entities() {
    // Cyrillic о in "Ivanоv"; Latin c in "cлово".
    let text = "Mr Ivanоv and Mr Smyth signed. Это cлово.";
    let entity = [
        text.find("Ivan").unwrap()..text.find(" and").unwrap(),
        text.find("Smyth").unwrap()..text.find(" signed").unwrap(),
    ];
    let found = flagged(
        &speller(),
        text,
        Exclusions {
            structural: &[],
            entities: &entity,
        },
    );
    assert_eq!(found, ["Ivanоv", "cлово"]);
}

#[test]
fn mixed_script_is_judged_per_hyphen_part_and_skips_codes() {
    assert!(plain("Отправьте PDF-файл; модель Т-34A.").is_empty());
}

#[test]
fn structural_exclusions_and_identifiers_are_skipped() {
    let text = "Use `fooo` and PERSON_1 and snake_case, see https://exampel.com/pathh or mail me@exampel.com.";
    let code = text.find("`fooo`").unwrap();
    let found = flagged(
        &speller(),
        text,
        Exclusions {
            structural: &[code..code + 6],
            entities: &[],
        },
    );
    assert!(found.is_empty(), "{found:?}");
}

#[test]
fn hyphenated_words_flag_only_the_wrong_part() {
    assert_eq!(plain("A well-knwon non-disclosure clause."), ["knwon"]);
}

#[test]
fn curly_apostrophes_and_possessives() {
    assert!(plain("The Buyer’s rights and the Seller's duties don’t lapse.").is_empty());
}

#[test]
fn turned_off_languages_are_accepted_without_loading() {
    let english_only = Speller::load(Options {
        russian: false,
        ..Options::default()
    });
    assert_eq!(
        flagged(
            &english_only,
            "Стороны обязуютса; the partys",
            Exclusions::default()
        ),
        ["partys"]
    );
}

#[test]
fn ranges_are_utf8_byte_offsets() {
    let text = "Договор — «обязуютса» signd";
    let ranges = speller().check(text, Exclusions::default());
    assert_eq!(ranges.len(), 2);
    assert_eq!(&text[ranges[0].clone()], "обязуютса");
    assert_eq!(&text[ranges[1].clone()], "signd");
}

#[test]
fn line_cache_reuses_unchanged_lines_and_tracks_shifts() {
    let s = speller();
    let mut cache = LineCache::default();
    let first = "Frist line\nsecond lyne\n";
    let found = s.check_cached(first, Exclusions::default(), &mut cache);
    assert_eq!(found, s.check(first, Exclusions::default()));
    // Inserting a line shifts later results; cached lines map to new offsets.
    let second = "New\nFrist line\nsecond lyne\n";
    let found = s.check_cached(second, Exclusions::default(), &mut cache);
    let words: Vec<_> = found.iter().map(|r| &second[r.clone()]).collect();
    assert_eq!(words, ["Frist", "lyne"]);
    assert_eq!(cache.lines.len(), 3);
}

#[test]
fn exclusions_spanning_lines_are_clipped_per_line() {
    let text = "```\nfooo barr\n```\nbazz\n";
    let found = flagged(
        &speller(),
        text,
        Exclusions {
            structural: &[0..text.find("bazz").unwrap()],
            entities: &[],
        },
    );
    assert_eq!(found, ["bazz"]);
}

#[test]
fn suggestions_offer_single_script_spelling_first() {
    let s = speller();
    assert_eq!(
        s.suggest("cлово").first().map(String::as_str),
        Some("слово")
    );
    assert_eq!(s.suggest("іi").first().map(String::as_str), Some("ii"));
    assert!(s.suggest("agre").iter().any(|w| w == "agree"));
    assert!(s.suggest("обязуютса").iter().any(|w| w == "обязуются"));
    assert!(s.suggest("agre").len() <= 8);
}

#[test]
fn accepted_words_skip_flags_and_invalidate_the_cache() {
    let text = "The cessionary Kowalczyk and Ivanоv.\nKowalczyk again; Cessionary first.";
    let mut cache = LineCache::default();
    let plain = speller();
    let found = plain.check_cached(text, Exclusions::default(), &mut cache);
    let words: Vec<_> = found.iter().map(|r| &text[r.clone()]).collect();
    assert_eq!(
        words,
        [
            "cessionary",
            "Kowalczyk",
            "Ivanоv",
            "Kowalczyk",
            "Cessionary"
        ]
    );

    let accepted = Accepted {
        words: Arc::new(
            ["cessionary", "Kowalczyk", "Ivanоv"]
                .map(String::from)
                .into(),
        ),
        generation: 1,
    };
    let with_words = speller().with_accepted(accepted);
    // Same cache: the new generation must not reuse the old results.
    assert!(
        with_words
            .check_cached(text, Exclusions::default(), &mut cache)
            .is_empty()
    );
    // A capitalized entry does not accept a lowercase form.
    assert_eq!(
        flagged(&with_words, "kowalczyk", Exclusions::default()),
        ["kowalczyk"]
    );
}
