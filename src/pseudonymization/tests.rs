use super::*;
fn detection(range: Range<usize>, category: Category) -> Detection {
    Detection {
        range,
        category,
        score: 0.9,
        recognizer: crate::pseudonymization::Recognizer::Model,
    }
}
#[test]
fn anonymization_uses_shared_markers_keeps_exclusions_and_protects_markdown() {
    let source =
        "**Анна** Bob Анна [mail](anna@example.invalid) <span title='Bob'>Bob</span> 2026 100";
    let mut review = Review::default();
    review.set_mode(Mode::Anonymize, source);
    let anna = review.add_manual(source, 2..10, Category::Person).unwrap();
    let bob = source.find("Bob").unwrap();
    review
        .add_manual(source, bob..bob + 3, Category::Person)
        .unwrap();
    let email = source.find("anna@example.invalid").unwrap();
    review
        .add_manual(source, email..email + 20, Category::Email)
        .unwrap();
    review.keep(anna, Some(2..10));
    let edits = review.plan_all(source, None).unwrap();
    let mut result = source.to_owned();
    for (range, replacement) in edits.iter().rev() {
        result.replace_range(range.clone(), replacement);
    }
    assert_eq!(
        result,
        "**Анна** PERSON PERSON [mail](EMAIL) <span title='PERSON'>PERSON</span> 2026 100"
    );
    review.refresh_after_edits(&result, &edits);
    assert_eq!(review.remaining(), 0);
    assert!(review.plan(&result, anna, None, "PERSON_1").is_err());
    // Detected shared markers cannot acquire new numbered identities.
    review.set_mode(Mode::Pseudonymize, &result);
    let marker = result.find("PERSON").unwrap();
    review
        .ingest(
            &result,
            vec![detection(marker..marker + 6, Category::Person)],
        )
        .unwrap();
    assert_eq!(review.remaining(), 0);
}
#[test]
fn anonymization_converts_only_known_accepted_pseudonyms_and_restores_proposals() {
    let mut review = Review::default();
    let original = "Alice Bob CLIENT_OTHER";
    let alice = review.add_manual(original, 0..5, Category::Person).unwrap();
    let bob = review.add_manual(original, 6..9, Category::Person).unwrap();
    assert!(review.rename_identity(bob, "CLIENT_OTHER").is_err()); // raw token collision
    review.set_replacement(bob, "CLIENT_B"); // proposed, never accepted
    let edits = review.plan(original, alice, None, "CLIENT_A").unwrap();
    let changed = "CLIENT_A Bob CLIENT_OTHER";
    let added =
        review
            .tracking
            .prepare(&[(0..5, "Alice".into(), "CLIENT_A".into(), Category::Person)]);
    review
        .tracking
        .on_transaction(&mdoc_editor::EditorTransaction {
            revision: 1,
            changes: vec![mdoc_editor::HistoryChange {
                before: 0,
                after: 1,
                edits: vec![mdoc_editor::SourceEdit {
                    range: 0..5,
                    new_len: 8,
                }],
            }],
            retained: vec![0, 1],
        });
    review.tracking.commit(1, added);
    review.record_acceptance(alice, "CLIENT_A");
    review.refresh_after_edits(changed, &edits);
    review.set_mode(Mode::Anonymize, changed);
    assert_eq!(
        review.plan_all(changed, None).unwrap(),
        vec![(0..8, "PERSON".into()), (9..12, "PERSON".into())]
    );
    assert_eq!(review.mappings().len(), 2);
    review.set_mode(Mode::Pseudonymize, changed);
    assert_eq!(
        review.plan_all(changed, None).unwrap(),
        vec![(9..12, "CLIENT_B".into())]
    );
    let mut reopened = Review::default();
    reopened.set_mode(Mode::Anonymize, changed);
    assert!(reopened.plan_all(changed, None).unwrap().is_empty());
    // Explicit manual selection works for tokens whose mappings are gone.
    reopened
        .add_manual(changed, 0..8, Category::Person)
        .unwrap();
    assert_eq!(
        reopened.plan_all(changed, None).unwrap(),
        vec![(0..8, "PERSON".into())]
    );
}
#[test]
fn unicode_exact_repeats_and_hidden_source_keep_source_syntax() {
    let source = "**Анна** [Анна](https://x.invalid/Анна) `Анна` Аннушка";
    let mut review = Review::default();
    review
        .ingest(source, vec![detection(2..10, Category::Person)])
        .unwrap();
    let group = &review.groups[0];
    assert_eq!(group.mentions.len(), 4);
    let edits = review.plan(source, group.id, None, "PERSON_1").unwrap();
    let mut changed = source.to_string();
    for (range, replacement) in edits.iter().rev() {
        changed.replace_range(range.clone(), replacement);
    }
    assert_eq!(
        changed,
        "**PERSON_1** [PERSON_1](https://x.invalid/PERSON_1) `PERSON_1` Аннушка"
    );
    review.refresh(&changed);
    assert_eq!(review.remaining(), 0);
    review.refresh(source); // undo
    assert_eq!(review.remaining(), 4);
    assert_eq!(review.groups[0].replacement, "PERSON_1");
}
#[test]
fn keep_all_survives_rescan_and_single_keep_rebases() {
    let mut review = Review::default();
    review
        .ingest("Ann Ann", vec![detection(0..3, Category::Person)])
        .unwrap();
    let id = review.groups[0].id;
    review.keep(id, Some(0..3));
    assert_eq!(review.remaining(), 1);
    review.refresh("prefix Ann Ann");
    assert_eq!(review.groups[0].mentions, vec![11..14]);
    review.keep(id, None);
    review
        .ingest("prefix Ann Ann", vec![detection(7..10, Category::Person)])
        .unwrap();
    assert_eq!(review.remaining(), 0);
}
#[test]
fn batch_refresh_preserves_a_kept_mention_between_replacements() {
    let source = "Ann Acme Ann Bob";
    let mut review = Review::default();
    let ann = review.add_manual(source, 0..3, Category::Person).unwrap();
    review
        .add_manual(source, 4..8, Category::Organization)
        .unwrap();
    review.add_manual(source, 13..16, Category::Person).unwrap();
    review.keep(ann, Some(9..12));
    let edits = review.plan_all(source, None).unwrap();
    let mut changed = source.to_owned();
    for (range, replacement) in edits.iter().rev() {
        changed.replace_range(range.clone(), replacement);
    }
    review.refresh_after_edits(&changed, &edits);
    assert_eq!(changed, "PERSON_1 ORG_1 Ann PERSON_2");
    assert_eq!(review.remaining(), 0);
}
#[test]
fn batch_plan_preserves_kept_mentions_custom_tokens_and_hidden_source() {
    let source = "**Анна** Acme Анна [mail](anna@example.invalid) Bob";
    let mut review = Review::default();
    let anna = review.add_manual(source, 2..10, Category::Person).unwrap();
    let acme = source.find("Acme").unwrap();
    let org = review
        .add_manual(source, acme..acme + 4, Category::Organization)
        .unwrap();
    let email = source.find("anna@example.invalid").unwrap();
    review
        .add_manual(source, email..email + 20, Category::Email)
        .unwrap();
    let bob = source.find("Bob").unwrap();
    let kept = review
        .add_manual(source, bob..bob + 3, Category::Person)
        .unwrap();
    review.keep(anna, Some(2..10));
    review.keep(kept, None);
    review.set_replacement(org, "CLIENT_1");
    let edits = review
        .plan_all(source, Some((anna, "PERSON_CUSTOM")))
        .unwrap();
    assert_eq!(edits.len(), 3);
    let mut changed = source.to_owned();
    for (range, replacement) in edits.iter().rev() {
        changed.replace_range(range.clone(), replacement);
    }
    assert_eq!(
        changed,
        "**Анна** CLIENT_1 PERSON_CUSTOM [mail](EMAIL_1) Bob"
    );
    assert!(review.plan_all("changed", None).is_err());
    assert!(
        review
            .plan_all(source, Some((anna, "invalid token")))
            .is_err()
    );
    assert_eq!(review.remaining(), 3);
}
#[test]
fn plans_refuse_stale_or_invalid_ranges_and_syntax_replacements() {
    let mut review = Review::default();
    assert!(
        review
            .ingest("Ё", vec![detection(1..2, Category::Person)])
            .is_err()
    );
    review
        .ingest("Ann", vec![detection(0..3, Category::Person)])
        .unwrap();
    let id = review.groups[0].id;
    assert!(review.plan("Anna", id, None, "PERSON_1").is_err());
    assert!(review.plan("Ann", id, Some(1..3), "PERSON_1").is_err());
    assert!(review.plan("Ann", id, None, "](bad)").is_err());
    assert!(
        review
            .add_manual("[Ann](url)", 0..10, Category::Person)
            .is_err()
    );
}
#[test]
fn hidden_values_are_replaceable_but_html_names_quotes_and_list_prefixes_are_protected() {
    let source = "1. Anna\n\n[Anna](https://x.invalid/Anna) ![photo](Anna.png) <Anna Anna=\"Anna\">Anna</Anna>";
    let mut review = Review::default();
    let id = review.add_manual(source, 3..7, Category::Person).unwrap();
    let group = review.group(id).unwrap();
    assert_eq!(group.mentions.len(), 6);
    let mut changed = source.to_owned();
    for (range, replacement) in review
        .plan(source, id, None, "PERSON_1")
        .unwrap()
        .iter()
        .rev()
    {
        changed.replace_range(range.clone(), replacement);
    }
    assert_eq!(
        changed,
        "1. PERSON_1\n\n[PERSON_1](https://x.invalid/PERSON_1) ![photo](PERSON_1.png) <Anna Anna=\"PERSON_1\">PERSON_1</Anna>"
    );
    assert!(review.add_manual(source, 0..2, Category::Identity).is_err());
    let tag = source.find("<Anna").unwrap() + 1;
    assert!(
        review
            .add_manual(source, tag..tag + 4, Category::Person)
            .is_err()
    );
    let quote = source.find("\"Anna\"").unwrap();
    assert!(
        review
            .add_manual(source, quote + 1..quote + 6, Category::Person)
            .is_err()
    );
    assert!(!valid_replacement("---"));
    assert!(!valid_replacement("1"));
    assert!(!valid_replacement("PERSON_"));
}
#[test]
fn parenthesized_phone_values_do_not_consume_link_delimiters() {
    let source = "Call (202) 555-0101. [contact](tel:(202)555-0101)";
    let mut review = Review::default();
    let id = review.add_manual(source, 5..19, Category::Phone).unwrap();
    assert_eq!(
        review.plan(source, id, None, "PHONE_1").unwrap()[0].0,
        5..19
    );
    let end = source.len();
    assert!(
        review
            .add_manual(source, end - 13..end, Category::Phone)
            .is_err()
    );
}
#[test]
fn mappings_link_variants_explicitly_and_never_collide_with_source_tokens() {
    let mut review = Review::default();
    let source = "PERSON_1 Анна Анны";
    let a = review.add_manual(source, 9..17, Category::Person).unwrap();
    let b = review.add_manual(source, 18..26, Category::Person).unwrap();
    assert_eq!(review.group(a).unwrap().replacement, "PERSON_2");
    review.set_replacement(b, "PERSON_2");
    assert_eq!(
        review.group(a).unwrap().replacement,
        review.group(b).unwrap().replacement
    );
}

#[test]
#[ignore = "2 MiB / 20,000 UTF-8 occurrence matcher/storage matrix"]
fn utf8_matcher_storage_matrix() {
    for groups in [1, 1_000, 20_000] {
        let mut source = String::new();
        let mut detections = Vec::new();
        for n in 0..20_000 {
            let start = source.len();
            source.push_str(&format!("Анна{:05}", n % groups));
            detections.push(Detection {
                range: start..source.len(),
                category: Category::Person,
                score: 0.9,
                recognizer: Recognizer::Model,
            });
            source.push_str(" обычный текст.\n");
        }
        source.push_str(&" ".repeat(2_097_152 - source.len()));
        let mut review = Review::default();
        review.ingest(&source, detections).unwrap();
        assert_eq!(review.candidates.len(), 20_000);
        assert_eq!(review.plan_all(&source, None).unwrap().len(), 20_000);
        let input = review.discovery_input();
        assert!(Arc::ptr_eq(&review.source, &input.source));
        let matcher = review.matcher.as_ref().unwrap();
        assert!(Arc::ptr_eq(matcher, input.matcher.as_ref().unwrap()));
        eprintln!(
            "PII_STORAGE bytes={} occurrences=20000 groups={groups} matcher_bytes={} candidate_capacity_bytes={} shared_source=true cached_matcher=true",
            source.len(),
            matcher.memory_usage(),
            review.candidates.capacity() * std::mem::size_of::<CandidateOccurrence>()
        );
    }
}

#[test]
fn normalized_identities_preserve_bytes_and_legal_forms_without_guessing_relationships() {
    let source = "Alice Morgan · ALICE   MORGAN · ООО «Берег» · ооо \"берег\" · ИП Берег";
    let mut review = Review::default();
    let a = review.add_manual(source, 0..12, Category::Person).unwrap();
    let at = source.find("ALICE").unwrap();
    let b = review
        .add_manual(source, at..at + "ALICE   MORGAN".len(), Category::Person)
        .unwrap();
    assert_eq!(review.group_identity(a), review.group_identity(b));
    assert_eq!(review.group(b).unwrap().original.as_ref(), "ALICE   MORGAN");
    let mut ids = Vec::new();
    for value in ["ООО «Берег»", "ооо \"берег\"", "ИП Берег"] {
        let start = source.find(value).unwrap();
        let group = review
            .add_manual(source, start..start + value.len(), Category::Organization)
            .unwrap();
        ids.push(review.group_identity(group).unwrap());
    }
    assert_eq!(ids[0], ids[1]);
    assert_ne!(ids[0], ids[2]);
    assert!(
        review
            .active_identities()
            .iter()
            .all(|id| review.identity(*id).unwrap().owner.is_none())
    );
}

#[test]
fn initials_discovery_keeps_ambiguous_links_as_suggestions() {
    let source = "Павлова Марина Сергеевна · Павлова Мария Степановна · Павлова М.С. · Павлова";
    let mut review = Review::default();
    let mut detections = Vec::new();
    for value in ["Павлова Марина Сергеевна", "Павлова Мария Степановна"]
    {
        let start = source.find(value).unwrap();
        detections.push(detection(start..start + value.len(), Category::Person));
    }
    review.ingest(source, detections).unwrap();
    let initials = review
        .groups
        .iter()
        .find(|g| g.original.as_ref() == "Павлова М.С.")
        .unwrap();
    assert_eq!(review.suggestions(initials.id).len(), 2);
    assert_eq!(review.active_identities().len(), 3);
    assert!(
        review
            .groups
            .iter()
            .all(|g| g.original.as_ref() != "Павлова")
    );
}
