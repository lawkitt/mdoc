use super::*;
use crate::{Detection, Recognizer};
use mdoc_history::{EditorTransaction, HistoryChange, SourceEdit};

fn identity(alias: &str, category: Category, originals: &[&str]) -> ExportedIdentity {
    ExportedIdentity {
        alias: alias.into(),
        previous: Vec::new(),
        category,
        custom: parse_alias(alias).is_none(),
        originals: originals.iter().map(|o| Arc::from(*o)).collect(),
    }
}
fn apply(map: &RestoreMap, text: &str) -> String {
    let mut out = text.to_string();
    for (range, value) in map.restore(text).edits.iter().rev() {
        out.replace_range(range.clone(), value);
    }
    out
}
/// The identities every fixture reply was written against.
fn fixture_map() -> RestoreMap {
    RestoreMap::new(vec![
        identity(
            "PERSON_1",
            Category::Person,
            &["Anna Petrova", "A. Petrova"],
        ),
        identity("PERSON_2", Category::Person, &["Иван Сергеевич Кузнецов"]),
        identity("ORG_1", Category::Organization, &["Northwind Traders Ltd"]),
        identity("ORG_2", Category::Organization, &["ООО «Ромашка»"]),
        identity("EMAIL_1", Category::Email, &["anna@northwind.example"]),
        identity(
            "ADDRESS_1",
            Category::Address,
            &["12 Harbour Street, Leeds"],
        ),
        identity("Seller", Category::Organization, &["Contoso GmbH"]),
    ])
}

#[test]
fn canonical_and_reshaped_generated_aliases_restore() {
    let map = fixture_map();
    for (reply, expected) in [
        ("PERSON_1 signs.", "Anna Petrova signs."),
        ("person_1 signs.", "Anna Petrova signs."),
        ("Person 1 signs.", "Anna Petrova signs."),
        ("PERSON-1 signs.", "Anna Petrova signs."),
        ("PERSON\u{a0}1 signs.", "Anna Petrova signs."),
        ("PERSON_01 signs.", "Anna Petrova signs."),
        ("**PERSON_1** signs.", "**Anna Petrova** signs."),
        ("`PERSON_1` signs.", "`Anna Petrova` signs."),
        ("PERSON_1's notice", "Anna Petrova's notice"),
        ("PERSON_1’s notice", "Anna Petrova’s notice"),
        ("(ORG_1)", "(Northwind Traders Ltd)"),
        ("Write to EMAIL_1.", "Write to anna@northwind.example."),
        (
            "[mail](mailto:EMAIL_1)",
            "[mail](mailto:anna@northwind.example)",
        ),
        (
            "| PERSON_1 | ORG_1 |",
            "| Anna Petrova | Northwind Traders Ltd |",
        ),
    ] {
        assert_eq!(apply(&map, reply), expected, "{reply}");
    }
}

#[test]
fn partial_matches_and_unknown_loose_forms_never_restore() {
    let map = fixture_map();
    for reply in [
        "PERSON_12 signs.",
        "XPERSON_1",
        "PERSON_1x",
        "PERSON_1_draft",
        "PERSONS_1",
        "PERSON__1",
        "PERSON 1a",
        "Tax 2023 return",
        "date-1",
    ] {
        assert_eq!(map.restore(reply).restored, 0, "{reply}");
    }
    // Only the canonical look of an unknown alias is reported.
    assert_eq!(map.restore("PERSON_12 signs.").unresolved, 1);
    assert_eq!(map.restore("Tax 2023, date-1, person 9").unresolved, 0);
}

#[test]
fn russian_inflections_split_from_the_alias() {
    let map = fixture_map();
    assert_eq!(
        apply(&map, "Направить PERSON_2-у копию, а ORG_2у — счёт."),
        "Направить Иван Сергеевич Кузнецов-у копию, а ООО «Ромашка»у — счёт."
    );
    assert_eq!(
        apply(&map, "Договор с «ORG_2» подписан PERSON_2."),
        "Договор с «ООО «Ромашка»» подписан Иван Сергеевич Кузнецов."
    );
}

#[test]
fn custom_aliases_restore_only_exactly() {
    let map = fixture_map();
    assert_eq!(
        apply(&map, "The Seller warrants"),
        "The Contoso GmbH warrants"
    );
    for reply in [
        "The seller warrants",
        "Sellers warrant",
        "Seller_1",
        "ReSeller",
    ] {
        assert_eq!(map.restore(reply).restored, 0, "{reply}");
    }
}

#[test]
fn summary_counts_mentions_values_and_unresolved_tokens() {
    let map = fixture_map();
    let result = map.restore("PERSON_1 and person 1 met ORG_1; PERSON_9 and ORG_7 did not.");
    assert_eq!(
        (result.restored, result.values, result.unresolved),
        (3, 2, 2)
    );
    let text = "PERSON_9 and ORG_7, PERSON_1";
    assert_eq!(map.unresolved(text), vec![0..8, 13..18]);
}

#[test]
fn previous_aliases_still_restore_after_a_rename() {
    let mut renamed = identity("BUYER", Category::Person, &["Anna Petrova"]);
    renamed.previous = vec!["PERSON_1".into()];
    let map = RestoreMap::new(vec![renamed]);
    assert_eq!(apply(&map, "BUYER, PERSON_1"), "Anna Petrova, Anna Petrova");
}

/// Replies modelled on how chat assistants reshape placeholders: Markdown
/// emphasis, headings, tables, lists, possessives, Russian case endings,
/// renumbered and invented tokens. Synthetic, not captured from a live model.
#[test]
fn fixture_replies_restore_as_expected() {
    let map = fixture_map();
    for (name, reply, expected, unresolved) in [
        (
            "en-summary",
            include_str!("../fixtures/restore/en-summary.reply.md"),
            include_str!("../fixtures/restore/en-summary.expected.md"),
            0,
        ),
        (
            "en-table",
            include_str!("../fixtures/restore/en-table.reply.md"),
            include_str!("../fixtures/restore/en-table.expected.md"),
            1,
        ),
        (
            "ru-letter",
            include_str!("../fixtures/restore/ru-letter.reply.md"),
            include_str!("../fixtures/restore/ru-letter.expected.md"),
            0,
        ),
        (
            "mangled",
            include_str!("../fixtures/restore/mangled.reply.md"),
            include_str!("../fixtures/restore/mangled.expected.md"),
            3,
        ),
    ] {
        assert_eq!(apply(&map, reply), expected, "{name}");
        assert_eq!(map.restore(reply).unresolved, unresolved, "{name}");
    }
}

fn apply_pending(review: &mut Review, source: &str, revision: u64) -> String {
    let plans = review.pending_plans(source, &HashMap::new()).unwrap();
    let added = review.prepare_replacements(&plans);
    let mut text = source.to_string();
    for plan in plans.iter().rev() {
        text.replace_range(plan.range.clone(), &plan.after);
    }
    review.on_transaction(
        &EditorTransaction {
            revision,
            changes: vec![HistoryChange {
                before: revision - 1,
                after: revision,
                edits: plans
                    .iter()
                    .map(|p| SourceEdit {
                        range: p.range.clone(),
                        new_len: p.after.len(),
                    })
                    .collect(),
            }],
            retained: vec![revision - 1, revision],
        },
        &text,
    );
    review.commit_replacements(revision, added);
    text
}

#[test]
fn export_takes_applied_aliases_and_the_fullest_wording() {
    let source = "Anna Petrova and A. Petrova met Northwind Traders Ltd.";
    let mut review = Review::default();
    review
        .ingest(
            source,
            vec![
                Detection {
                    range: 0..12,
                    category: Category::Person,
                    score: 0.9,
                    recognizer: Recognizer::Model,
                },
                Detection {
                    range: 32..53,
                    category: Category::Organization,
                    score: 0.9,
                    recognizer: Recognizer::Model,
                },
            ],
        )
        .unwrap();
    let short = review.add_manual(source, 17..27, Category::Person).unwrap();
    let anna = review.variant_identity(review.variants()[0].id).unwrap();
    review.assign_variant(short, anna).unwrap();
    let applied = apply_pending(&mut review, source, 1);
    assert_eq!(applied, "PERSON_1 and PERSON_1 met ORG_1.");
    let exported = review.export_identities();
    assert_eq!(exported.len(), 2);
    let person = exported.iter().find(|e| e.alias == "PERSON_1").unwrap();
    assert_eq!(person.wording(), "Anna Petrova");
    assert_eq!(person.originals.len(), 2);
    let map = RestoreMap::new(exported.clone());
    assert_eq!(
        apply(&map, &applied),
        "Anna Petrova and Anna Petrova met Northwind Traders Ltd."
    );

    // A new document seeded with these identities proposes the same aliases.
    let restored = "Anna Petrova wrote to Northwind Traders Ltd and A. Petrova.";
    let mut seeded = Review::default();
    seeded.seed(&exported, restored);
    let plans = seeded.pending_plans(restored, &HashMap::new()).unwrap();
    let aliases: Vec<_> = plans.iter().map(|p| p.after.as_ref()).collect();
    assert_eq!(aliases, ["PERSON_1", "ORG_1", "PERSON_1"]);
    // New identities continue numbering after the seeded aliases.
    assert_eq!(seeded.next_alias(Category::Person), "PERSON_2");
}

#[test]
fn a_wording_with_a_full_stop_does_not_double_the_reply_s() {
    let map = RestoreMap::new(vec![identity(
        "ORG_1",
        Category::Organization,
        &["Northwind Ltd."],
    )]);
    assert_eq!(apply(&map, "Buy from ORG_1."), "Buy from Northwind Ltd.");
    assert_eq!(apply(&map, "ORG_1 sells"), "Northwind Ltd. sells");
}

#[test]
fn choices_offer_the_token_category_first() {
    let map = fixture_map();
    let people: Vec<_> = map.choices("PERSON_9")[..2].to_vec();
    assert!(
        people
            .iter()
            .all(|&i| map.identities()[i].category == Category::Person)
    );
    assert_eq!(map.choices("ORG-4")[0], 2);
    assert_eq!(map.choices("PERSON_9").len(), map.identities().len());
}

#[test]
fn a_generated_looking_custom_alias_matches_exactly() {
    let mut chosen = identity("PERSON_9", Category::Person, &["Anna Petrova"]);
    chosen.custom = true;
    chosen.previous = vec!["PERSON_1".into()];
    let map = RestoreMap::new(vec![chosen]);
    assert_eq!(
        apply(&map, "PERSON_9 and PERSON_1"),
        "Anna Petrova and Anna Petrova"
    );
    for reply in ["person 9", "PERSON-9", "PERSON_09"] {
        assert_eq!(map.restore(reply).restored, 0, "{reply}");
    }
    assert_eq!(
        map.restore("person 1").restored,
        1,
        "earlier generated aliases stay lenient"
    );
}

#[test]
fn restored_ranges_follow_the_applied_edits() {
    let map = RestoreMap::new(vec![
        identity("PERSON_1", Category::Person, &["John Smith"]),
        identity("PERSON_2", Category::Person, &["John Smith"]),
    ]);
    let reply = "PERSON_1 met PERSON_2.";
    let restoration = map.restore(reply);
    let text = apply(&map, reply);
    assert_eq!(text, "John Smith met John Smith.");
    let ranges = restoration.restored_ranges();
    assert_eq!(ranges, [(0..10, 0), (15..25, 1)]);
    assert!(ranges.iter().all(|(r, _)| &text[r.clone()] == "John Smith"));
}
