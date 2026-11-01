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
fn forgetting_an_identity_preserves_another_homonyms_assigned_mention() {
    let source = "Ann met Ann";
    let mut review = Review::default();
    let group = review.add_manual(source, 0..3, Category::Person).unwrap();
    let first = review.variant_identity(group).unwrap();
    let second = review.new_identity(Category::Person);
    review.commit_assignment(0, 8..11, second);
    let before = review.identity_snapshot();
    review.forget_identity(first);
    assert!(review.identity(first).is_none());
    assert_eq!(review.identity_count(second), 1);
    assert_eq!(review.export_identities().len(), 1);
    assert_eq!(review.export_identities()[0].originals[0].as_ref(), "Ann");
    review.restore_identity_snapshot(before);
    review.refresh(source);
    assert!(review.identity(first).is_none());
    assert_eq!(review.active_identities(), [second]);
}

#[test]
fn forgetting_a_case_alias_also_scrubs_a_pending_document_rename() {
    for case_alias in ["PERSON_1", "CLIENT"] {
        let mut review = Review::default();
        let group = review.add_manual("Ann", 0..3, Category::Person).unwrap();
        let id = review.variant_identity(group).unwrap();
        let initial = review.identity_snapshot();
        review.rename_identity(id, "CLIENT").unwrap();
        review.commit_identity_snapshot(0, 1, initial);
        let previous = review.identity_snapshot();
        // The page can open before the member's debounced save reaches the case.
        review.rename_identity(id, "BUYER").unwrap();
        review.commit_identity_snapshot(1, 2, previous);
        review.forget_case_identity(&[case_alias], &["Ann".into()]);
        assert!(review.identity(id).is_none());
        assert!(review.export_identities().is_empty());
    }
}
#[test]
fn mutations_advance_the_revision_and_a_new_review_never_repeats_one() {
    let source = "Ann met Bob";
    let mut review = Review::default();
    let mut seen = vec![review.revision()];
    let ann = review.add_manual(source, 0..3, Category::Person).unwrap();
    seen.push(review.revision());
    review.keep(ann, None);
    seen.push(review.revision());
    let identity = review.variant_identity(ann).unwrap();
    review.rename_identity(identity, "CLIENT").unwrap();
    seen.push(review.revision());
    let unchanged = review.revision();
    let _ = review.variants();
    assert_eq!(
        review.revision(),
        unchanged,
        "reads leave the revision alone"
    );
    seen.push(Review::default().revision());
    let mut sorted = seen.clone();
    sorted.sort_unstable();
    sorted.dedup();
    assert_eq!(sorted.len(), seen.len(), "{seen:?}");
}
#[test]
fn kept_homonyms_retain_each_occurrences_assigned_identity() {
    let source = "Ann Ann Ann";
    let mut review = Review::default();
    let variant = review.add_manual(source, 0..3, Category::Person).unwrap();
    let first = review.variant_identity(variant).unwrap();
    let second = review.new_identity(Category::Person);
    review.commit_assignment(0, 4..7, second);
    review.keep(variant, Some(4..7));
    review.keep_mentions([(variant, 0..3), (variant, 8..11)]);
    let kept = review.kept_mentions();
    assert_eq!(kept.len(), 3);
    assert_eq!(kept[0].2, Some(first));
    assert_eq!(kept[1].2, Some(second));
    assert_eq!(kept[2].2, Some(first));
    assert_eq!(
        review.propose_kept_mentions(source, std::slice::from_ref(&(4..7))),
        1
    );
    let kept = review.kept_mentions();
    assert_eq!(kept.len(), 2);
    assert!(kept.iter().all(|(_, _, id)| *id == Some(first)));
    let plans = review.pending_plans(source, &HashMap::new()).unwrap();
    assert_eq!(plans.len(), 1);
    assert_eq!(plans[0].identity, second);
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
/// Applies every pending mention of `detections` in `source`.
fn applied(source: &str, detections: Vec<Detection>) -> String {
    let mut review = Review::default();
    review.ingest(source, detections).unwrap();
    let mut changed = source.to_owned();
    for (range, replacement) in plan_all(&review, source).unwrap().iter().rev() {
        changed.replace_range(range.clone(), replacement);
    }
    changed
}
#[test]
fn only_real_markdown_and_html_syntax_hides_a_value() {
    // An email autolink is not a tag (#411), and a tag is protected only
    // when it is a whole one.
    let source = "From: <ann@x.invalid>\nTo: <bob@x.invalid> <i>Bob</i> <b class=\"Bob\">";
    let at = |text: &str| source.find(text).unwrap();
    let email = |text: &str| detection(at(text)..at(text) + text.len(), Category::Email);
    assert_eq!(
        applied(source, vec![email("ann@x.invalid"), email("bob@x.invalid")]),
        "From: <EMAIL_1>\nTo: <EMAIL_2> <i>Bob</i> <b class=\"Bob\">"
    );
    assert_eq!(
        protected_syntax(source),
        [at("i>B")..at("i>B") + 1, at("/i>") + 1..at("/i>") + 2]
            .into_iter()
            .chain([at("b c")..at("b c") + 1, at("class")..at("class") + 5])
            .chain([
                at("\"Bob")..at("\"Bob") + 1,
                at("Bob\">") + 3..at("Bob\">") + 4
            ])
            .collect::<Vec<_>>()
    );
    // Literal `*` and `#` inside a value are text (#423); emphasis is not.
    let source = "Fennel*Orbit Ltd, ID LC#713624, *Ann* Lee.";
    let at = |text: &str| source.find(text).unwrap();
    assert_eq!(
        applied(
            source,
            vec![
                detection(0..16, Category::Organization),
                detection(at("LC#")..at("LC#") + 9, Category::Identity),
                detection(at("Ann*")..at(" Lee") + 4, Category::Person),
            ]
        ),
        "ORG_1, ID IDENTITY_1, *Ann* Lee."
    );
    // A number after a paragraph line is no list unless it is 1 (#428); a
    // bullet is one, so its marker stays and its value is replaced.
    let source = "Паспорт:\n46. 18 731904\nТелефон:\n+ 7 (900) 000-84-31\n\n1. a\n2. b";
    let at = |text: &str| source.find(text).unwrap();
    assert_eq!(
        applied(
            source,
            vec![
                detection(at("46.")..at("46.") + 13, Category::Identity),
                detection(at("+ 7")..at("+ 7") + 19, Category::Phone),
            ]
        ),
        "Паспорт:\nIDENTITY_1\nТелефон:\n+ PHONE_1\n\n1. a\n2. b"
    );
    assert_eq!(
        syntax::list_markers(source),
        [
            at("+ 7")..at("+ 7") + 2,
            at("1. a")..at("1. a") + 2,
            at("2. b")..at("2. b") + 2
        ]
    );
    // Siblings and lazy lines stay in their list; a paragraph after a blank
    // line ends it.
    assert_eq!(syntax::list_markers("1. a\nlazy\n2. b").len(), 2);
    assert_eq!(syntax::list_markers("1. a\n\nText\n2. b").len(), 1);
    assert_eq!(syntax::list_markers("# Title\n2. b").len(), 1);
}
/// #388, #396, #419: a value wrapped over lines is one span; two lines'
/// text, a paragraph break or a hard break is not.
#[test]
fn a_value_wrapped_over_lines_is_one_span() {
    let source = "Signed by Melora Brin-\nsett on 3 May.\nPhone: +1 202\n555 0176\n- Заявитель: Евгения\n  Матвеевна\n  Мельник.\nDepositor: Delva Rosk\nExhibition company: Cinder Ltd\n\nAnn\n\nLee, Bob  \nRay.";
    let span = |text: &str, category| {
        let start = source.find(text).unwrap();
        detection(start..start + text.len(), category)
    };
    assert_eq!(
        applied(
            source,
            vec![
                span("Melora Brin-\nsett", Category::Person),
                span("+1 202\n555 0176", Category::Phone),
                span("Евгения\n  Матвеевна\n  Мельник", Category::Person),
                span("Delva Rosk\nExhibition company", Category::Organization),
                span("Ann\n\nLee", Category::Person),
                span("Bob  \nRay", Category::Person),
            ]
        ),
        "Signed by PERSON_1 on 3 May.\nPhone: PHONE_1\n- Заявитель: PERSON_2.\nDepositor: Delva Rosk\nExhibition company: Cinder Ltd\n\nAnn\n\nLee, Bob  \nRay."
    );
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
    // The bare surname is proposed on its own, offering both namesakes.
    let surname = review
        .variants()
        .iter()
        .find(|g| g.original.as_ref() == "Павлова")
        .unwrap();
    assert_eq!(surname.mentions.len(), 1);
    // Both namesakes, and "Павлова М.С.", which contains it.
    assert_eq!(review.suggestions(surname.id).len(), 3);
    assert_eq!(review.active_identities().len(), 4);
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
fn category_choice_alias_names_only_a_changing_alias() {
    // #179: the popup's category rows show the alias a choice gives.
    let source = "Ann and Ann.";
    let mut review = Review::default();
    let variant = review.add_manual(source, 0..3, Category::Person).unwrap();
    let ann = review.variant_identity(variant).unwrap();
    // The whole entity: its own category keeps the alias, others renumber.
    assert_eq!(
        review.category_choice_alias(ann, Category::Person, true),
        None
    );
    let org = review.category_choice_alias(ann, Category::Organization, true);
    assert_eq!(org.as_deref(), Some("ORG_1"));
    // Some of its mentions split off under a new alias, even in its category.
    let split = review.category_choice_alias(ann, Category::Person, false);
    assert_eq!(split.as_deref(), Some("PERSON_2"));
    // A custom alias stays whatever the category of the whole entity.
    review.rename_identity(ann, "Client").unwrap();
    for category in Category::ALL {
        assert_eq!(review.category_choice_alias(ann, category, true), None);
    }
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

#[test]
fn partial_name_replace_keeps_the_containing_identity_whole() {
    let source = include_str!("../../../docs/product-media/Settlement agreement.md");
    let mut review = Review::default();
    // Added by hand, with "Bennett" and "Maya" they propose kept.
    let mut full = Vec::new();
    for value in ["Maya Bennett", "Daniel Bennett"] {
        let start = source.find(value).unwrap();
        let id = review
            .add_manual(source, start..start + value.len(), Category::Person)
            .unwrap();
        full.push((id, review.variant_identity(id).unwrap()));
    }
    for form in ["Bennett", "Maya"] {
        let id = proposed(&review, form).id;
        review.keep(id, None);
    }
    // Replace "Bennett" selected in "Ms Bennett".
    let start = source.find("Ms Bennett,").unwrap() + 3;
    let selection = review
        .manual_target(source, start..start + "Bennett".len())
        .unwrap();
    let bennett = review
        .add_manual(source, selection, Category::Person)
        .unwrap();
    for (variant, _) in &full {
        assert_eq!(review.variant(*variant).unwrap().mentions.len(), 1);
    }
    let standalone = &review.variant(bennett).unwrap().mentions;
    assert_eq!(
        standalone.len(),
        6,
        "every Ms Bennett, none inside a full name"
    );
    assert!(
        standalone
            .iter()
            .all(|r| source[..r.start].ends_with("Ms "))
    );
    // A marked short form is not linked; Same as… offers both.
    let own = review.variant_identity(bennett).unwrap();
    assert!(full.iter().all(|(_, identity)| *identity != own));
    let suggestions = review.suggestions(bennett);
    assert!(
        full.iter()
            .all(|(_, identity)| suggestions.contains(identity))
    );
    assert_eq!(review.active_identities().len(), 3);
    let edits = plan_all(&review, source).unwrap();
    assert_eq!(edits.len(), 8);
    for value in ["Maya Bennett", "Daniel Bennett"] {
        let start = source.find(value).unwrap();
        assert!(
            edits
                .iter()
                .any(|(r, _)| *r == (start..start + value.len()))
        );
    }
}

#[test]
fn containment_suggests_only_name_identities() {
    let source = "Write to Alice@example.com. Alice agreed.";
    let mut review = Review::default();
    let email = source.find("Alice@").unwrap();
    review
        .ingest(
            source,
            vec![detection(
                email..email + "Alice@example.com".len(),
                Category::Email,
            )],
        )
        .unwrap();
    let start = source.rfind("Alice").unwrap();
    let alice = review
        .add_manual(source, start..start + "Alice".len(), Category::Person)
        .unwrap();
    assert!(review.suggestions(alice).is_empty());
}

fn ingest_names(source: &str, names: &[&str]) -> Review {
    let mut review = Review::default();
    let detections = names
        .iter()
        .map(|value| {
            let start = source.find(value).unwrap();
            detection(start..start + value.len(), Category::Person)
        })
        .collect();
    review.ingest(source, detections).unwrap();
    review
}
fn proposed<'a>(review: &'a Review, original: &str) -> &'a Variant {
    review
        .variants()
        .iter()
        .find(|g| g.original.as_ref() == original)
        .unwrap_or_else(|| panic!("{original} is not proposed"))
}

#[test]
fn a_scan_proposes_title_surname_and_possessive_forms_of_a_full_name() {
    let source = include_str!("../../../docs/product-media/Settlement agreement.md");
    let review = ingest_names(source, &["Maya Bennett", "Daniel Bennett"]);
    let full: Vec<_> = ["Maya Bennett", "Daniel Bennett"]
        .map(|value| {
            review
                .variant_identity(proposed(&review, value).id)
                .unwrap()
        })
        .to_vec();
    // The surname after each title, the title outside the span (guide rule
    // 1): five plain mentions and "Ms Bennett's"; "Ms Maya Bennett" stays
    // whole.
    let short = proposed(&review, "Bennett");
    assert_eq!(short.mentions.len(), 6);
    assert!(
        short
            .mentions
            .iter()
            .any(|r| source[r.end..].starts_with("'s"))
    );
    // Two namesakes: its own identity until Same as… links it to one.
    let own = review.variant_identity(short.id).unwrap();
    assert!(!full.contains(&own));
    let mut expected = full.clone();
    expected.sort_unstable();
    assert_eq!(review.suggestions(short.id), expected);
    // "Maya" alone has one candidate and joins her.
    let maya = proposed(&review, "Maya");
    assert!(maya.recognizer() == Some(Recognizer::Propagation));
    assert_eq!(review.variant_identity(maya.id), Some(full[0]));
    let edits = plan_all(&review, source).unwrap();
    assert_eq!(edits.len(), 9);
    assert!(
        edits
            .iter()
            .all(|(r, _)| !source[..r.start].ends_with("Ms Maya "))
    );
}

#[test]
fn a_surname_or_first_name_with_one_candidate_joins_it() {
    let source = "Priya Raman and James Paul signed. Raman's counsel, Dr. Raman, \
        Priya and Paul agreed with Ms Raman.";
    let review = ingest_names(source, &["Priya Raman", "James Paul"]);
    let raman = review.variant_identity(proposed(&review, "Priya Raman").id);
    for short in ["Raman", "Priya"] {
        let variant = proposed(&review, short);
        assert!(
            variant.recognizer() == Some(Recognizer::Propagation),
            "{short}"
        );
        assert_eq!(review.variant_identity(variant.id), raman, "{short}");
    }
    // "Raman's", and after "Dr." and "Ms", titles outside the span.
    assert_eq!(proposed(&review, "Raman").mentions.len(), 3);
    let paul = review.variant_identity(proposed(&review, "James Paul").id);
    assert_eq!(review.variant_identity(proposed(&review, "Paul").id), paul);
    // Every name takes the next alias; the forms take none.
    assert_eq!(review.active_identities().len(), 2);
    // Titles in any case mark a surname and stay outside it; the verb
    // "miss" is no title.
    let source = "Priya Raman signed. MR RAMAN, mr Raman and DR. Raman \
        did not miss Raman.";
    let review = ingest_names(source, &["Priya Raman"]);
    for (short, count) in [("RAMAN", 1), ("Raman", 3)] {
        assert_eq!(proposed(&review, short).mentions.len(), count, "{short}");
    }
    // A surname that is also another person's first name has two
    // candidates: proposed with its own identity, Same as… offers both.
    let source = "Paul Smith met James Paul. Then Paul left.";
    let review = ingest_names(source, &["Paul Smith", "James Paul"]);
    let paul = proposed(&review, "Paul");
    let own = review.variant_identity(paul.id).unwrap();
    let both = review.suggestions(paul.id);
    assert_eq!(both.len(), 2);
    assert!(!both.contains(&own));
}

#[test]
fn a_kept_name_proposes_no_forms_inside_or_outside_its_mentions() {
    let source = "Priya Raman signed. Priya Raman agreed.";
    let mut review = ingest_names(source, &["Priya Raman"]);
    let full = proposed(&review, "Priya Raman");
    let (id, first) = (full.id, full.mentions[0].clone());
    // One mention kept: "Raman" inside it is not proposed on a rescan.
    review.keep(id, Some(first));
    review.ingest(source, Vec::new()).unwrap();
    assert!(
        review
            .variants()
            .iter()
            .all(|g| g.original.as_ref() != "Raman")
    );
    assert_eq!(review.remaining(), 1);
    // Kept everywhere: its bare surname elsewhere is not proposed either.
    let mut review = Review::default();
    let full = review
        .add_manual("Priya Raman signed.", 0..11, Category::Person)
        .unwrap();
    review.keep(full, None);
    let source = "Priya Raman signed. Raman agreed.";
    review.ingest(source, Vec::new()).unwrap();
    assert!(
        review
            .variants()
            .iter()
            .all(|g| g.original.as_ref() != "Raman")
    );
    assert_eq!(review.remaining(), 0);
}

#[test]
fn a_scan_proposes_russian_case_forms_of_a_surname() {
    let source = "Истец: Павлова Марина Сергеевна. Ответчик: Иван Петрович Чайковский. \
        Свидетель: Приянка Раман. Показания Рамана приобщены. Павловой направлено письмо; \
        Павлову уведомили. Чайковского и Чайковскому не было; Раману ответили, \
        Раманом подписано. Марине и Ивану сообщили.";
    let names = [
        "Павлова Марина Сергеевна",
        "Иван Петрович Чайковский",
        "Приянка Раман",
    ];
    let review = ingest_names(source, &names);
    let identity = |name| review.variant_identity(proposed(&review, name).id).unwrap();
    for (short, full) in [
        ("Павловой", names[0]),
        ("Павлову", names[0]),
        ("Марине", names[0]),
        ("Чайковского", names[1]),
        ("Чайковскому", names[1]),
        ("Ивану", names[1]),
        ("Рамана", names[2]),
        ("Раману", names[2]),
        ("Раманом", names[2]),
    ] {
        assert_eq!(identity(short), identity(full), "{short}");
    }
    // Only inside the full name.
    assert!(
        review
            .variants()
            .iter()
            .all(|g| g.original.as_ref() != "Приянка")
    );
}

#[test]
fn manual_alias_says_what_an_addition_would_get() {
    let source = "Ann met Bob. Acme Ltd paid.";
    let mut review = Review::default();
    review.add_manual(source, 0..3, Category::Person).unwrap();
    // A new wording: the next free alias of each category.
    assert_eq!(review.manual_alias("Bob", Category::Person), "PERSON_2");
    assert_eq!(review.manual_alias("Bob", Category::Organization), "ORG_1");
    // Only new wording takes a pick; a proposed one keeps its category.
    assert_eq!(review.proposed_wording_category("Bob"), None);
    assert_eq!(
        review.proposed_wording_category("Ann"),
        Some(Category::Person)
    );
    // A wording that normalizes to a proposed identity shares its alias in
    // that identity's category, even a custom one.
    assert_eq!(review.manual_alias("ANN", Category::Person), "PERSON_1");
    assert_eq!(review.manual_alias("ANN", Category::Email), "EMAIL_1");
    let ann = review.identity_by_alias("PERSON_1").unwrap();
    review.rename_identity(ann, "Client").unwrap();
    assert_eq!(review.manual_alias("ANN", Category::Person), "Client");
    assert_eq!(review.manual_alias("ANN", Category::Organization), "ORG_1");
}

/// Issue #302: a window that cut "Harrow & Finch Ltd" short proposed
/// "Harrow & Finch", which every occurrence of the full name contains. It
/// must not take ORG_2 and leave the next organization as ORG_3.
#[test]
fn a_wording_found_only_inside_longer_ones_takes_no_alias() {
    let source = "Harrow & Finch Ltd pays. Harrow & Finch Ltd agrees with Elm Row Legal LLP.";
    let full = 0.."Harrow & Finch Ltd".len();
    let second = source.rfind("Harrow").unwrap();
    let elm = source.find("Elm").unwrap();
    let mut review = Review::default();
    review
        .ingest(
            source,
            vec![
                detection(full, Category::Organization),
                detection(
                    second..second + "Harrow & Finch".len(),
                    Category::Organization,
                ),
                detection(elm..elm + "Elm Row Legal LLP".len(), Category::Organization),
            ],
        )
        .unwrap();
    let proposed: Vec<_> = review
        .variants()
        .iter()
        .map(|v| {
            (
                v.original.as_ref(),
                v.replacement.as_str(),
                v.mentions.len(),
            )
        })
        .collect();
    assert_eq!(
        proposed,
        [
            ("Harrow & Finch Ltd", "ORG_1", 2),
            ("Elm Row Legal LLP", "ORG_2", 1)
        ]
    );
}

/// The variants of `identity`, in order, with their pending mention counts.
fn identity_variants(review: &Review, identity: u64) -> Vec<(String, usize)> {
    review
        .variants()
        .iter()
        .filter(|v| review.variant_identity(v.id) == Some(identity))
        .map(|v| (v.original.to_string(), v.mentions.len()))
        .collect()
}

/// U27: the model's six aliases for one person in the Russian demo, and
/// "Марина" it missed, become one identity (ADR 0092).
#[test]
fn the_russian_demo_gives_pavlova_one_identity() {
    let source = include_str!("../../../docs/product-media/Соглашение об урегулировании.md");
    // What the pinned model finds for people there (U27): her full name,
    // first name + patronymic, a nickname, the surname with initials, her
    // husband and her lawyer.
    let detections = [
        "Павлова Марина Сергеевна",
        "Марина Сергеевна, которую",
        "Маришей",
        "Павловой М. С.",
        "Павлову Дмитрию Андреевичу",
        "Шахова Ирина Олеговна",
    ]
    .map(|value| {
        let start = source.find(value).unwrap();
        let name = value.strip_suffix(", которую").unwrap_or(value);
        detection(start..start + name.len(), Category::Person)
    })
    .to_vec();
    let mut review = Review::default();
    review.ingest(source, detections).unwrap();
    let pavlova = review
        .variant_identity(proposed(&review, "Павлова Марина Сергеевна").id)
        .unwrap();
    assert_eq!(
        identity_variants(&review, pavlova),
        [
            ("Павлова Марина Сергеевна".to_string(), 1),
            ("Марина Сергеевна".to_string(), 1),
            ("Маришей".to_string(), 1),
            ("Павловой М. С.".to_string(), 1),
            ("Марина".to_string(), 1),
            ("Павлова".to_string(), 2),
            ("Павловой".to_string(), 1),
        ]
    );
    // "Павлову" is her accusative and her husband's dative: either, so an
    // identity of its own that Same as… offers both.
    let either = proposed(&review, "Павлову").id;
    assert!(
        !identity_variants(&review, pavlova)
            .iter()
            .any(|(w, _)| w == "Павлову")
    );
    assert_eq!(review.suggestions(either).len(), 2);
    assert!(proposed(&review, "Марина").recognizer() == Some(Recognizer::Propagation));
    assert_eq!(
        proposed(&review, "Маришей").recognizer(),
        Some(Recognizer::Model),
        "the model's own"
    );
    // Her husband and her lawyer keep their own identities.
    let others: Vec<_> = ["Павлову Дмитрию Андреевичу", "Шахова Ирина Олеговна"]
        .map(|name| review.variant_identity(proposed(&review, name).id).unwrap())
        .to_vec();
    assert!(!others.contains(&pavlova) && others[0] != others[1]);
    // One alias for her in the text; the next people take the next ones.
    let alias = |id| review.identity(id).unwrap().alias.clone();
    assert_eq!(
        [pavlova, others[0], others[1]].map(alias),
        ["PERSON_1", "PERSON_2", "PERSON_3"]
    );
    let edits = plan_all(&review, source).unwrap();
    assert_eq!(
        edits
            .iter()
            .filter(|(_, alias)| alias == "PERSON_1")
            .count(),
        8
    );
    assert!(
        review.merge_hints().is_empty(),
        "{:?}",
        review.merge_hints()
    );
}

#[test]
fn a_marked_full_name_proposes_its_forms_and_undo_withdraws_them() {
    let source = "Истец Павлова Марина Сергеевна. Позже Марина и Мариша; \
        ответчик Павлову не знал.";
    let mut review = Review::default();
    // A scan found the nickname alone before the full name was marked.
    let nickname = source.find("Мариша").unwrap();
    review
        .ingest(
            source,
            vec![detection(
                nickname..nickname + "Мариша".len(),
                Category::Person,
            )],
        )
        .unwrap();
    let alone = review.variant_identity(proposed(&review, "Мариша").id);
    let before = review.identity_snapshot();
    let start = source.find("Павлова").unwrap();
    let full = review
        .add_manual(
            source,
            start..start + "Павлова Марина Сергеевна".len(),
            Category::Person,
        )
        .unwrap();
    let identity = review.variant_identity(full).unwrap();
    assert_ne!(alone, Some(identity));
    assert_eq!(
        identity_variants(&review, identity),
        [
            ("Мариша".to_string(), 1),
            ("Павлова Марина Сергеевна".to_string(), 1),
            ("Марина".to_string(), 1),
            ("Павлову".to_string(), 1),
        ]
    );
    // Undoing the mark withdraws the forms and the link.
    review.restore_identity_snapshot(before);
    review.refresh(source);
    assert_eq!(
        review.variant_identity(proposed(&review, "Мариша").id),
        alone
    );
    assert_eq!(proposed(&review, "Марина").mentions.len(), 0);
    assert_eq!(proposed(&review, "Павлову").mentions.len(), 0);
}

#[test]
fn an_applied_name_propagates_to_new_text() {
    let source = "Priya Raman signed.";
    let mut review = ingest_names(source, &["Priya Raman"]);
    let text = crate::restore::tests::apply_pending(&mut review, source, 1);
    let identity = review.applied()[0].step.identity;
    let source = format!("{text} Later Raman and Priya agreed.");
    review.refresh(&source);
    review.ingest(&source, Vec::new()).unwrap();
    for form in ["Raman", "Priya"] {
        assert_eq!(
            review.variant_identity(proposed(&review, form).id),
            Some(identity),
            "{form}"
        );
    }
}

#[test]
fn a_joined_form_moves_when_another_person_shares_it() {
    let source = "Maya Bennett signed. Ms Bennett agreed. Daniel Bennett wrote.";
    let mut review = ingest_names(source, &["Maya Bennett"]);
    let maya = review.variant_identity(proposed(&review, "Maya Bennett").id);
    let short = proposed(&review, "Bennett").id;
    assert_eq!(review.variant_identity(short), maya);
    // A second scan finds her husband: "Ms Bennett" may be either.
    review = {
        let start = source.find("Daniel").unwrap();
        review
            .ingest(
                source,
                vec![detection(
                    start..start + "Daniel Bennett".len(),
                    Category::Person,
                )],
            )
            .unwrap();
        review
    };
    let own = review.variant_identity(short).unwrap();
    assert_ne!(Some(own), maya);
    assert_eq!(review.suggestions(short).len(), 2);
    assert_eq!(review.remaining(), 3);
}

#[test]
fn a_later_scan_links_a_form_found_alone_before() {
    let source = "Марина Сергеевна подписала. Павлова Марина Сергеевна, истец.";
    let mut review = ingest_names(source, &["Марина Сергеевна"]);
    let first = proposed(&review, "Марина Сергеевна").id;
    let start = source.find("Павлова").unwrap();
    review
        .ingest(
            source,
            vec![detection(
                start..start + "Павлова Марина Сергеевна".len(),
                Category::Person,
            )],
        )
        .unwrap();
    let full = review.variant_identity(proposed(&review, "Павлова Марина Сергеевна").id);
    assert_eq!(review.variant_identity(first), full);
    assert!(review.merge_hints().is_empty());
}

#[test]
fn an_ordinary_word_counts_only_capitalised_mid_sentence() {
    let source = "Rose Hill signed. We met Rose today. Rose agreed; the rose wilted.";
    let review = ingest_names(source, &["Rose Hill"]);
    let rose = proposed(&review, "Rose");
    let mid = source.find("Rose today").unwrap();
    assert_eq!(rose.mentions, vec![mid..mid + 4]);
}

#[test]
fn forms_inside_link_destinations_and_identifiers_stay_out() {
    let source = "# Contract\n\nAlice Morgan represents **Alice Morgan**. \
        [contact](https://x.invalid/Alice_Morgan) Write to alice_morgan or x/Morgan.\n";
    let review = ingest_names(source, &["Alice Morgan"]);
    let found: Vec<_> = review
        .variants()
        .iter()
        .map(|v| v.original.as_ref())
        .collect();
    assert_eq!(found, ["Alice Morgan"]);
    assert_eq!(review.remaining(), 2);
}

#[test]
fn only_forms_that_join_wait_so_aliases_keep_source_order() {
    let source = "Maya Bennett signed. Ms Bennett, known as Mia, agreed. Maya left. \
        Daniel Bennett wrote.";
    let review = ingest_names(
        source,
        &[
            "Maya Bennett",
            "Ms Bennett",
            "Mia",
            "Maya",
            "Daniel Bennett",
        ],
    );
    let aliases: Vec<_> = review
        .variants()
        .iter()
        .map(|v| (v.original.as_ref(), v.replacement.as_str()))
        .collect();
    assert_eq!(
        aliases,
        [
            ("Maya Bennett", "PERSON_1"),
            ("Ms Bennett", "PERSON_2"),
            ("Mia", "PERSON_3"),
            ("Daniel Bennett", "PERSON_4"),
            ("Maya", "PERSON_1"),
        ]
    );
}

#[test]
#[ignore = "propagation over a 1 MiB document with 50 people; prints its time"]
fn propagation_scales_with_the_document() {
    let mut source = String::new();
    let mut names = Vec::new();
    let letters: Vec<char> = "бвгдклмнпрстфхцж".chars().collect();
    let tag = |n: usize| format!("{}{}", letters[n / 16], letters[n % 16]);
    for n in 0..50 {
        names.push(format!("Ан{}а Петр{}ова Сергеевна", tag(n), tag(n)));
    }
    while source.len() < 1 << 20 {
        for (n, name) in names.iter().enumerate() {
            source.push_str(&format!(
                "{name} подписала. Позже Петр{}овой и Ан{}а Сергеевна, Mr Smith и Rose. ",
                tag(n),
                tag(n)
            ));
        }
    }
    let detections = names
        .iter()
        .map(|name| {
            let start = source.find(name.as_str()).unwrap();
            detection(start..start + name.len(), Category::Person)
        })
        .collect();
    let started = std::time::Instant::now();
    let mut review = Review::default();
    review.ingest(&source, detections).unwrap();
    eprintln!(
        "PROPAGATION bytes={} variants={} remaining={} in {:?}",
        source.len(),
        review.variants().len(),
        review.remaining(),
        started.elapsed()
    );
}

/// Review of #372: a mark's automatic link is not the user's, so a later
/// candidate moves it; undoing the mark restores the link it replaced.
#[test]
fn a_marks_links_stay_automatic_and_follow_its_undo() {
    let source = "Maya agreed. Maya Bennett signed. Maya Jones wrote.";
    let mut review = ingest_names(source, &["Maya"]);
    let maya = proposed(&review, "Maya").id;
    let full = review.add_manual(source, 13..25, Category::Person).unwrap();
    let bennett = review.variant_identity(full);
    assert_eq!(review.variant_identity(maya), bennett);
    // A scan finds another Maya: the lone one may be either.
    let jones = source.find("Maya Jones").unwrap();
    review
        .ingest(
            source,
            vec![detection(
                jones..jones + "Maya Jones".len(),
                Category::Person,
            )],
        )
        .unwrap();
    let alone = review.variant_identity(maya);
    assert_ne!(alone, bennett);
    assert_ne!(
        alone,
        review.variant_identity(proposed(&review, "Maya Jones").id)
    );

    // A mark that adds a second candidate moves a joined form within its
    // own undo step.
    let source = "Maya Bennett signed. Ms Bennett agreed. Daniel Bennett wrote.";
    let mut review = ingest_names(source, &["Maya Bennett"]);
    let short = proposed(&review, "Bennett").id;
    let maya = review.variant_identity(proposed(&review, "Maya Bennett").id);
    assert_eq!(review.variant_identity(short), maya);
    let before = review.identity_snapshot();
    let daniel = source.find("Daniel").unwrap();
    review
        .add_manual(
            source,
            daniel..daniel + "Daniel Bennett".len(),
            Category::Person,
        )
        .unwrap();
    let after = review.identity_snapshot();
    assert_ne!(review.variant_identity(short), maya);
    review.restore_identity_snapshot(before);
    assert_eq!(review.variant_identity(short), maya, "undo");
    review.restore_identity_snapshot(after);
    assert_ne!(review.variant_identity(short), maya, "redo");
}

/// Review of #372: a form that replaced text keeps its link; one that left
/// a name can join it again; a respelled wording follows the form.
#[test]
fn applied_forms_stay_and_detached_forms_can_rejoin() {
    let source = "Maya Bennett signed. Ms Bennett agreed.";
    let mut review = ingest_names(source, &["Maya Bennett"]);
    let maya = review.variant_identity(proposed(&review, "Maya Bennett").id);
    let text = crate::restore::tests::apply_pending(&mut review, source, 1);
    assert_eq!(text, "PERSON_1 signed. Ms PERSON_1 agreed.");
    let source = format!("{text} Ms Bennett and Daniel Bennett wrote.");
    review.refresh(&source);
    let daniel = source.find("Daniel").unwrap();
    review
        .ingest(
            &source,
            vec![detection(
                daniel..daniel + "Daniel Bennett".len(),
                Category::Person,
            )],
        )
        .unwrap();
    let short = proposed(&review, "Bennett").id;
    assert_eq!(
        review.variant_identity(short),
        maya,
        "applied under her alias"
    );

    let source = "Maya Bennett signed. Ms Bennett agreed. Daniel Bennett wrote. MS BENNETT left.";
    let daniel = source.find("Daniel").unwrap();
    let mut review = ingest_names(source, &["Maya Bennett"]);
    review
        .ingest(
            source,
            vec![detection(
                daniel..daniel + "Daniel Bennett".len(),
                Category::Person,
            )],
        )
        .unwrap();
    let short = proposed(&review, "Bennett").id;
    let own = review.variant_identity(short);
    let maya = review.variant_identity(proposed(&review, "Maya Bennett").id);
    assert_ne!(own, maya);
    // The respelled wording moved with it, not to Maya.
    assert_eq!(
        review.variant_identity(proposed(&review, "BENNETT").id),
        own
    );
    // Daniel kept everywhere: Maya is the one candidate again.
    let daniel = proposed(&review, "Daniel Bennett").id;
    review.keep(daniel, None);
    review.ingest(source, Vec::new()).unwrap();
    assert_eq!(review.variant_identity(short), maya);
}

/// Review of #372: marking an ordinary word by hand counts every mention.
#[test]
fn a_manual_mark_lifts_the_mid_sentence_filter() {
    let source = "Rose Hill signed. We met Rose. Rose agreed.";
    let mut review = ingest_names(source, &["Rose Hill"]);
    assert_eq!(proposed(&review, "Rose").mentions.len(), 1);
    let start = source.rfind("Rose").unwrap();
    review
        .add_manual(source, start..start + 4, Category::Person)
        .unwrap();
    assert_eq!(proposed(&review, "Rose").mentions.len(), 2);
}

/// A surname with initials that is her form counts as her, not as a second
/// person: the model's "Павловой" beside "Павловой М. С." joins her in one
/// scan, and spends no alias.
#[test]
fn initials_of_the_one_candidate_do_not_block_her_other_forms() {
    let source = "Павлова Марина Сергеевна, истец. Павловой М. С. выплачено. \
        Ноутбук Павловой вернут. Шахова Ирина Олеговна консультировала.";
    let laptop = source.find("Ноутбук ").unwrap() + "Ноутбук ".len();
    let mut detections: Vec<_> = [
        "Павлова Марина Сергеевна",
        "Павловой М. С.",
        "Шахова Ирина Олеговна",
    ]
    .map(|name| {
        let start = source.find(name).unwrap();
        detection(start..start + name.len(), Category::Person)
    })
    .to_vec();
    detections.push(detection(
        laptop..laptop + "Павловой".len(),
        Category::Person,
    ));
    let mut review = Review::default();
    review.ingest(source, detections).unwrap();
    let full = review.variant_identity(proposed(&review, "Павлова Марина Сергеевна").id);
    for form in ["Павловой М. С.", "Павловой"] {
        assert_eq!(
            review.variant_identity(proposed(&review, form).id),
            full,
            "{form}"
        );
    }
    assert_eq!(
        proposed(&review, "Шахова Ирина Олеговна").replacement,
        "PERSON_2"
    );
}

/// Review of #384: a rescan does not claim a variant the user marked or a
/// case seeded; only a variant the scan creates takes its recognizer.
#[test]
fn a_rescan_keeps_a_marked_or_seeded_variants_recognizer() {
    let source = "Ann met Bob.";
    let mut review = Review::default();
    let ann = review.add_manual(source, 0..3, Category::Person).unwrap();
    review
        .ingest(
            source,
            vec![
                detection(0..3, Category::Person),
                detection(8..11, Category::Person),
            ],
        )
        .unwrap();
    assert_eq!(review.variant(ann).unwrap().recognizer(), None);
    assert_eq!(
        proposed(&review, "Bob").recognizer(),
        Some(Recognizer::Model)
    );
    let mut seeded = Review::default();
    seeded.seed(&review.export_identities(), source);
    seeded
        .ingest(source, vec![detection(0..3, Category::Person)])
        .unwrap();
    assert_eq!(proposed(&seeded, "Ann").recognizer(), None);
}

/// Review of #409: a surname found after a title keeps that context when
/// linked and relinked, and the verb "miss" is no title.
#[test]
fn a_titled_surname_links_as_a_surname_and_miss_is_no_title() {
    let source = "Paul Smith met James Paul. Mr Paul agreed.";
    let mut review = ingest_names(source, &["Paul Smith", "James Paul"]);
    let james = review.variant_identity(proposed(&review, "James Paul").id);
    let paul = proposed(&review, "Paul").id;
    assert_eq!(review.variant_identity(paul), james);
    review.ingest(source, Vec::new()).unwrap();
    assert_eq!(review.variant_identity(paul), james, "rescan");

    let source = "Rose Hill signed. It was a near miss. Rose bushes were damaged.";
    let review = ingest_names(source, &["Rose Hill"]);
    assert!(
        review
            .variants()
            .iter()
            .all(|v| v.original.as_ref() != "Rose")
    );
}

/// #387: signature initials of a known full name join it.
#[test]
fn signature_initials_join_their_full_name() {
    let source = "The assessor, Dr Ysolde Qhenn, signed. She used the initials Y. Q. \
        on the sheet. Recorded by Ceryn Nove.";
    let review = ingest_names(source, &["Ysolde Qhenn", "Ceryn Nove"]);
    let full = review.variant_identity(proposed(&review, "Ysolde Qhenn").id);
    let initials = proposed(&review, "Y. Q.");
    assert_eq!(review.variant_identity(initials.id), full);
    assert_eq!(initials.recognizer(), Some(Recognizer::Propagation));
}

/// #392: a person the document names only in oblique cases still has a
/// surname: the inflected full names are one identity, and the surname
/// alone joins it.
#[test]
fn an_inflected_full_name_propagates_its_surname() {
    let source = "Заказ гражданки Эльзы-Руты Мартыновны Таль-Шнейдер принят. \
        Предмет выдаётся Эльзе-Руте Мартыновне Таль-Шнейдер лично. \
        Получатель Таль-Шнейдер просит вернуть аванс.";
    let review = ingest_names(
        source,
        &[
            "Эльзы-Руты Мартыновны Таль-Шнейдер",
            "Эльзе-Руте Мартыновне Таль-Шнейдер",
        ],
    );
    let genitive =
        review.variant_identity(proposed(&review, "Эльзы-Руты Мартыновны Таль-Шнейдер").id);
    let dative =
        review.variant_identity(proposed(&review, "Эльзе-Руте Мартыновне Таль-Шнейдер").id);
    assert_eq!(genitive, dative, "one person in two cases");
    let surname = proposed(&review, "Таль-Шнейдер");
    assert_eq!(surname.mentions.len(), 1);
    assert_eq!(review.variant_identity(surname.id), genitive);
    assert_eq!(review.active_identities().len(), 1);
}

/// Review of #421: a soft-stem first name in two cases is one person, and a
/// surname shaped like a patronymic still propagates.
#[test]
fn soft_stems_and_patronymic_shaped_surnames_read_back() {
    let source = "Заявление Дарьи Ивановны Петровой принято. Ответ Дарье Ивановне Петровой \
        направлен. Дарья согласна.";
    let review = ingest_names(
        source,
        &["Дарьи Ивановны Петровой", "Дарье Ивановне Петровой"],
    );
    let genitive = review.variant_identity(proposed(&review, "Дарьи Ивановны Петровой").id);
    let dative = review.variant_identity(proposed(&review, "Дарье Ивановне Петровой").id);
    assert_eq!(genitive, dative);
    assert_eq!(
        review.variant_identity(proposed(&review, "Дарья").id),
        genitive
    );

    let source = "Договор с Якубович Анны Сергеевны. Якубович подписала.";
    let review = ingest_names(source, &["Якубович Анны Сергеевны"]);
    let full = review.variant_identity(proposed(&review, "Якубович Анны Сергеевны").id);
    assert_eq!(
        review.variant_identity(proposed(&review, "Якубович").id),
        full
    );
}

/// #398: a known full name repeated in lower case joins it.
#[test]
fn a_lowercase_repeat_of_a_full_name_joins_it() {
    let source = "I, Iara Veldune, appoint Corvin Ashmere. Signed electronically: iara veldune.";
    let review = ingest_names(source, &["Iara Veldune", "Corvin Ashmere"]);
    let full = review.variant_identity(proposed(&review, "Iara Veldune").id);
    let repeat = proposed(&review, "iara veldune");
    assert_eq!(repeat.mentions.len(), 1);
    assert_eq!(review.variant_identity(repeat.id), full);
    assert_eq!(repeat.recognizer(), Some(Recognizer::Propagation));
    assert_eq!(review.active_identities().len(), 2);
}

/// Review of #422: a repeat joins the name's identity after a merge, and a
/// repeat inside a path stays out even once the wording is proposed.
#[test]
fn a_case_repeat_follows_merges_and_stays_out_of_paths() {
    let source = "Iara Veldune signed. Corvin Ashmere witnessed.";
    let mut review = ingest_names(source, &["Iara Veldune", "Corvin Ashmere"]);
    let iara = review
        .variant_identity(proposed(&review, "Iara Veldune").id)
        .unwrap();
    let corvin = review
        .variant_identity(proposed(&review, "Corvin Ashmere").id)
        .unwrap();
    review.merge_identity(iara, corvin).unwrap();
    let source = "Iara Veldune signed. Corvin Ashmere witnessed. Later iara veldune \
        agreed. File /iara veldune/notes.";
    review.ingest(source, Vec::new()).unwrap();
    let repeat = proposed(&review, "iara veldune");
    assert_eq!(review.variant_identity(repeat.id), Some(corvin));
    let prose = source.find("iara veldune").unwrap();
    assert_eq!(repeat.mentions, vec![prose..prose + "iara veldune".len()]);
}

/// #403: a surname's particle stays in its span and its identity.
#[test]
fn a_surname_keeps_its_particle() {
    let source = "Я, Генриетта Яновна де Лаваль, уполномочиваю. Заказ записан на фамилию \
        де Лаваль.";
    let review = ingest_names(source, &["Генриетта Яновна де Лаваль"]);
    let full = review.variant_identity(proposed(&review, "Генриетта Яновна де Лаваль").id);
    let surname = proposed(&review, "де Лаваль");
    assert_eq!(surname.mentions.len(), 1);
    assert_eq!(review.variant_identity(surname.id), full);
    assert!(
        review
            .variants()
            .iter()
            .all(|v| v.original.as_ref() != "Лаваль")
    );
}

/// #416: a known first name that is an ordinary word ("Grant") counts only
/// capitalised mid-sentence, even where the model proposed it.
#[test]
fn an_ordinary_word_name_counts_only_mid_sentence_whoever_proposed_it() {
    let source = "Landlord: Grant Pellover.\n\n## Grant\n\nThe room is let. \
        Grant of storage needs a variation.\n\nThe landlord, Grant, keeps a key.";
    let model = source.find("Grant of").unwrap();
    let full = source.find("Grant Pellover").unwrap();
    let mut review = Review::default();
    review
        .ingest(
            source,
            vec![
                detection(full..full + "Grant Pellover".len(), Category::Person),
                detection(model..model + "Grant".len(), Category::Person),
            ],
        )
        .unwrap();
    let grant = proposed(&review, "Grant");
    let mid = source.find("Grant, keeps").unwrap();
    assert_eq!(grant.mentions, vec![mid..mid + "Grant".len()]);
    // Marked by hand at a sentence start, every mention counts.
    let heading = source.find("## Grant").unwrap() + 3;
    review
        .add_manual(source, heading..heading + 5, Category::Person)
        .unwrap();
    assert_eq!(proposed(&review, "Grant").mentions.len(), 3);
}

/// Review of #440: a case's saved wording keeps every mention, and a mark
/// leaves an existing proposal's mentions as they were, so undo has
/// nothing to restore.
#[test]
fn the_ordinary_word_rule_leaves_saved_wordings_and_marks_alone() {
    let source = "Grant Pellover signed.\nGrant agreed.";
    let mut seeded = Review::default();
    let saved = [crate::restore::ExportedIdentity {
        alias: "PERSON_1".into(),
        previous: vec![],
        category: Category::Person,
        custom: false,
        originals: vec!["Grant Pellover".into(), "Grant".into()],
    }];
    seeded.seed(&saved, source);
    seeded.ingest(source, Vec::new()).unwrap();
    assert_eq!(
        proposed(&seeded, "Grant").mentions.len(),
        1,
        "saved wording"
    );

    let source = "Grant agreed.\nGrant Pellover signed.";
    let mut review = Review::default();
    review
        .ingest(source, vec![detection(0..5, Category::Person)])
        .unwrap();
    // The sentence-start "Grant" the model proposed stays proposed through
    // a mark of the full name and its undo.
    let first = |review: &Review| proposed(review, "Grant").mentions.contains(&(0..5));
    assert!(first(&review));
    let before = review.identity_snapshot();
    let full = source.find("Grant Pellover").unwrap();
    review
        .add_manual(
            source,
            full..full + "Grant Pellover".len(),
            Category::Person,
        )
        .unwrap();
    assert!(first(&review), "a mark");
    review.restore_identity_snapshot(before);
    review.refresh(source);
    assert!(first(&review), "undo");
}
