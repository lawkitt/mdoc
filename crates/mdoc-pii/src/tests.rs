use super::*;
/// Text edits Apply would make for pending mentions of an unchanged source.
pub(crate) fn plan_all(
    review: &Review,
    source: &str,
) -> Result<Vec<(Range<usize>, String)>, String> {
    if source != review.source.as_ref() {
        return Err("The document changed. Review the candidates again.".into());
    }
    Ok(review
        .pending_plans(source, &HashMap::new())?
        .into_iter()
        .filter(|plan| plan.before != plan.after)
        .map(|plan| (plan.range, plan.after.to_string()))
        .collect())
}
fn detection(range: Range<usize>, category: Category) -> Detection {
    Detection {
        range,
        category,
        score: 0.9,
        recognizer: crate::Recognizer::Model,
    }
}
#[test]
fn batch_keeps_exclusions_protects_markdown_and_skips_detected_markers() {
    let source =
        "**Анна** Bob Анна [mail](anna@example.invalid) <span title='Bob'>Bob</span> 2026 100";
    let mut review = Review::default();
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
    let edits = plan_all(&review, source).unwrap();
    let mut result = source.to_owned();
    for (range, replacement) in edits.iter().rev() {
        result.replace_range(range.clone(), replacement);
    }
    assert_eq!(
        result,
        "**Анна** PERSON_2 PERSON_1 [mail](EMAIL_1) <span title='PERSON_2'>PERSON_2</span> 2026 100"
    );
    review.refresh_after_edits(&result, &edits);
    assert_eq!(review.remaining(), 0);
    assert!(plan_all(&review, source).is_err());
    // A detected category marker cannot acquire a new numbered identity.
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
fn unicode_exact_repeats_and_hidden_source_keep_source_syntax() {
    let source = "**Анна** [Анна](https://x.invalid/Анна) `Анна` Аннушка";
    let mut review = Review::default();
    review
        .ingest(source, vec![detection(2..10, Category::Person)])
        .unwrap();
    let group = &review.variants()[0];
    assert_eq!(group.mentions.len(), 4);
    let edits = plan_all(&review, source).unwrap();
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
    assert_eq!(review.variants()[0].replacement, "PERSON_1");
}
#[test]
fn keep_all_survives_rescan_and_single_keep_rebases() {
    let mut review = Review::default();
    review
        .ingest("Ann Ann", vec![detection(0..3, Category::Person)])
        .unwrap();
    let id = review.variants()[0].id;
    review.keep(id, Some(0..3));
    assert_eq!(review.remaining(), 1);
    review.refresh("prefix Ann Ann");
    assert_eq!(review.variants()[0].mentions, vec![11..14]);
    review.keep(id, None);
    review
        .ingest("prefix Ann Ann", vec![detection(7..10, Category::Person)])
        .unwrap();
    assert_eq!(review.remaining(), 0);
}
#[test]
fn rescan_does_not_alias_existing_aliases() {
    let mut review = Review::default();
    let source = "Ann met Bob";
    review
        .ingest(
            source,
            vec![
                detection(0..3, Category::Person),
                detection(8..11, Category::Person),
            ],
        )
        .unwrap();
    let bob = review.variants()[1].identity;
    review.rename_identity(bob, "Witness").unwrap();
    // The detector reads both aliases as people on the rescan.
    let aliased = "PERSON_1 met Witness";
    review.refresh(aliased);
    review
        .ingest(
            aliased,
            vec![
                detection(0..8, Category::Person),
                detection(13..20, Category::Person),
            ],
        )
        .unwrap();
    assert_eq!(review.variants().len(), 2);
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
    let edits = plan_all(&review, source).unwrap();
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
    review.set_replacement(anna, "PERSON_CUSTOM");
    let edits = plan_all(&review, source).unwrap();
    assert_eq!(edits.len(), 3);
    let mut changed = source.to_owned();
    for (range, replacement) in edits.iter().rev() {
        changed.replace_range(range.clone(), replacement);
    }
    assert_eq!(
        changed,
        "**Анна** CLIENT_1 PERSON_CUSTOM [mail](EMAIL_1) Bob"
    );
    assert!(plan_all(&review, "changed").is_err());
    let identity = review.variant_identity(anna).unwrap();
    assert!(review.rename_identity(identity, "invalid token").is_err());
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
    let id = review.variants()[0].id;
    assert!(plan_all(&review, "Anna").is_err());
    let identity = review.variant_identity(id).unwrap();
    assert!(review.rename_identity(identity, "](bad)").is_err());
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
    let group = review.variant(id).unwrap();
    assert_eq!(group.mentions.len(), 6);
    let mut changed = source.to_owned();
    for (range, replacement) in plan_all(&review, source).unwrap().iter().rev() {
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
    review.add_manual(source, 5..19, Category::Phone).unwrap();
    assert_eq!(plan_all(&review, source).unwrap()[0].0, 5..19);
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
    assert_eq!(review.variant(a).unwrap().replacement, "PERSON_2");
    assert!(review.rename_identity(b, "PERSON_1").is_err()); // raw source token
    review.set_replacement(b, "PERSON_2");
    assert_eq!(
        review.variant(a).unwrap().replacement,
        review.variant(b).unwrap().replacement
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
        assert_eq!(review.candidates().len(), 20_000);
        assert_eq!(plan_all(&review, &source).unwrap().len(), 20_000);
        let input = review.discovery_input();
        assert!(Arc::ptr_eq(&review.source, &input.source));
        let matcher = review.candidates.matcher.as_ref().unwrap();
        assert!(Arc::ptr_eq(matcher, input.matcher.as_ref().unwrap()));
        eprintln!(
            "PII_STORAGE bytes={} occurrences=20000 groups={groups} matcher_bytes={} candidate_bytes={} shared_source=true cached_matcher=true",
            source.len(),
            matcher.memory_usage(),
            std::mem::size_of_val(review.candidates())
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
    assert_eq!(review.variant_identity(a), review.variant_identity(b));
    assert_eq!(
        review.variant(b).unwrap().original.as_ref(),
        "ALICE   MORGAN"
    );
    let mut ids = Vec::new();
    for value in ["ООО «Берег»", "ооо \"берег\"", "ИП Берег"] {
        let start = source.find(value).unwrap();
        let group = review
            .add_manual(source, start..start + value.len(), Category::Organization)
            .unwrap();
        ids.push(review.variant_identity(group).unwrap());
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
        .variants()
        .iter()
        .find(|g| g.original.as_ref() == "Павлова М.С.")
        .unwrap();
    assert_eq!(review.suggestions(initials.id).len(), 2);
    assert_eq!(review.active_identities().len(), 3);
    assert!(
        review
            .variants()
            .iter()
            .all(|g| g.original.as_ref() != "Павлова")
    );
}
#[test]
fn manual_selections_trim_edges_but_keep_balanced_pairs_and_initials() {
    use crate::manual::trim_selection;
    let trim = |source: &str| trim_selection(source, 0..source.len()).map(|r| source[r].to_owned());
    assert_eq!(trim(" **Иван Петров**, ").as_deref(), Some("Иван Петров"));
    assert_eq!(trim("«Ромашка».").as_deref(), Some("Ромашка"));
    assert_eq!(trim("ООО «Ромашка»,").as_deref(), Some("ООО «Ромашка»"));
    assert_eq!(trim("(Павлова М.С.)").as_deref(), Some("Павлова М.С."));
    assert_eq!(trim("Acme Ltd.").as_deref(), Some("Acme Ltd."));
    assert_eq!(trim("Петров.").as_deref(), Some("Петров"));
    assert_eq!(
        trim("+7 (495) 123-45-67;").as_deref(),
        Some("+7 (495) 123-45-67")
    );
    assert_eq!(trim("(495) 123").as_deref(), Some("(495) 123"));
    assert_eq!(trim(" .,; "), None);
}
#[test]
fn manual_targets_explain_invalid_selections() {
    let source = "Иван Петров met **Anna** and PERSON_1 in Ивановка.";
    let mut review = Review::default();
    review.add_manual(source, 0..8, Category::Person).unwrap();
    let ivan_petrov = 0..source.find(" met").unwrap();
    // The wider selection may contain the pending "Иван"; it supersedes it.
    assert_eq!(
        review.manual_target(source, ivan_petrov.clone()),
        Ok(ivan_petrov.clone())
    );
    let anna = source.find("**Anna**").unwrap();
    assert_eq!(
        review.manual_target(source, anna..anna + 8),
        Ok(anna + 2..anna + 6)
    );
    let token = source.find("PERSON_1").unwrap();
    assert_eq!(
        review.manual_target(source, token..token + 8).unwrap_err(),
        "Already replaced."
    );
    let village = source.find("Ивановка").unwrap();
    assert_eq!(
        review
            .manual_target(source, village..village + "Иван".len())
            .unwrap_err(),
        "Select whole words."
    );
    assert_eq!(
        review.manual_target(source, 2..4).unwrap_err(),
        "Select whole words."
    );
    let petrov = source.find("Петров").unwrap();
    assert_eq!(
        review.manual_target(source, 4..petrov + 2).unwrap_err(),
        "Select whole words."
    );
    assert_eq!(
        review.manual_target(source, 0..4).unwrap_err(),
        "Select whole words."
    );
    assert_eq!(
        review
            .manual_target(source, 6..petrov + "Петров".len())
            .unwrap_err(),
        "Select whole words."
    );
    let short = "Иван Петров met";
    let mut nested = Review::default();
    let met = short.find(" met").unwrap();
    nested.add_manual(short, 0..met, Category::Person).unwrap();
    assert_eq!(
        nested.manual_target(short, 9..short.len()).unwrap_err(),
        "Select all of the proposed replacement, or none of it."
    );
    assert_eq!(
        nested.manual_target(short, 0..8).unwrap_err(),
        "Already part of a proposed replacement."
    );
}
#[test]
fn a_manual_addition_supersedes_contained_pending_mentions_only_there() {
    let source = "Иван Петров signed. Иван agreed.";
    let mut review = Review::default();
    let ivan = review.add_manual(source, 0..8, Category::Person).unwrap();
    assert_eq!(review.variant(ivan).unwrap().mentions.len(), 2);
    let full = review
        .manual_target(source, 0..source.find(" signed").unwrap())
        .unwrap();
    let id = review
        .add_manual(source, full.clone(), Category::Person)
        .unwrap();
    assert_eq!(review.variant(id).unwrap().mentions, vec![full.clone()]);
    let second = source.rfind("Иван").unwrap();
    assert_eq!(
        review.variant(ivan).unwrap().mentions,
        vec![second..second + 8]
    );
    assert!(review.annotation_id(id, &full).is_some());
}
#[test]
fn a_manual_addition_of_new_wording_adds_pending_mentions() {
    let source = "Анна met Приемка. Приемка again.";
    let mut review = Review::default();
    review.add_manual(source, 0..8, Category::Person).unwrap();
    assert_eq!(review.remaining(), 1);
    let start = source.find("Приемка").unwrap();
    review
        .add_manual(source, start..start + "Приемка".len(), Category::Person)
        .unwrap();
    assert_eq!(review.remaining(), 3);
}
#[test]
fn escaped_and_literal_delimiters_are_text_but_emphasis_is_structure() {
    let source = "Планируемая дата: «\\_\\__» __________ 2026 г.\n\nNext *Anna Lee* here.";
    let mut review = Review::default();
    let value = "«\\_\\__» __________ 2026 г.";
    let start = source.find(value).unwrap();
    let range = start..start + value.len();
    assert_eq!(
        review.manual_target(source, range.clone()),
        Ok(range.clone())
    );
    review
        .add_manual(source, range.clone(), Category::Person)
        .unwrap();
    assert_eq!(
        plan_all(&review, source).unwrap(),
        vec![(range, "PERSON_1".into())]
    );
    let anna = source.find("Anna").unwrap();
    let here = source.find(" here").unwrap();
    assert_eq!(
        review.manual_target(source, anna..here + 5).unwrap_err(),
        "Selection crosses Markdown formatting."
    );
    let lee = source.find("Lee").unwrap();
    assert_eq!(
        review.manual_target(source, anna..lee + 3),
        Ok(anna..lee + 3)
    );
}
#[test]
fn date_and_other_have_their_own_tokens_and_existing_tokens_stay_reserved() {
    let source = "REDACTED_1 on 12.03.2026 about Планируемая дата";
    let mut review = Review::default();
    let date = source.find("12.03.2026").unwrap();
    let d = review
        .add_manual(source, date..date + 10, Category::Date)
        .unwrap();
    let other = source.find("Планируемая").unwrap();
    let o = review
        .add_manual(source, other..source.len(), Category::Other)
        .unwrap();
    assert_eq!(review.variant(d).unwrap().replacement, "DATE_1");
    assert_eq!(review.variant(o).unwrap().replacement, "REDACTED_2");
    assert_eq!(Category::ALL.last(), Some(&Category::Other));
}
