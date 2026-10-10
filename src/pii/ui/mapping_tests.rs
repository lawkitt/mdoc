use super::*;

/// Text edits Apply would make for pending mentions.
fn plan_all(review: &Review, source: &str) -> Result<Vec<(Range<usize>, String)>, String> {
    Ok(review
        .pending_plans(source, &Default::default())?
        .into_iter()
        .filter(|plan| plan.before != plan.after)
        .map(|plan| (plan.range, plan.after.to_string()))
        .collect())
}

fn seed(app: &mut DocumentView, source: &str, cx: &mut Context<DocumentView>) -> (u64, u64) {
    app.editor.update(cx, |e, cx| e.set_text(source, cx));
    app.pii.review = Default::default();
    app.pii.mapping.expanded.clear();
    app.pii.reviewing = true;
    let mut detections = Vec::new();
    for (value, category) in [
        ("Павлова Марина Сергеевна", Category::Person),
        ("Павлова М.С.", Category::Organization),
        ("marina@example.invalid", Category::Email),
    ] {
        for (at, _) in source.match_indices(value) {
            detections.push(pii::Detection {
                range: at..at + value.len(),
                category,
                score: 0.9,
                recognizer: pii::Recognizer::Model,
            });
        }
    }
    app.pii.review.ingest(source, detections).unwrap();
    let full = app
        .pii
        .review
        .variants()
        .iter()
        .find(|g| g.original.as_ref() == "Павлова Марина Сергеевна")
        .map_or(0, |g| g.id);
    let initials = app
        .pii
        .review
        .variants()
        .iter()
        .find(|g| g.original.as_ref() == "Павлова М.С.")
        .unwrap()
        .id;
    app.pii.mapping.begin_review();
    app.sync_annotations(cx);
    (full, initials)
}

#[gpui::test]
fn staging_merge_category_owner_apply_rename_and_undo_preserve_exact_originals(
    cx: &mut gpui::TestAppContext,
) {
    let (app, cx) = crate::document_view_tests::boot(cx);
    let source = "Павлова Марина Сергеевна · Павлова М.С. · Павлова М.С. · marina@example.invalid";
    let (full, initials) = app.update(cx, |app, cx| seed(app, source, cx));
    cx.run_until_parked();
    app.update(cx, |app, cx| {
        let full_id = app.pii.review.variant_identity(full).unwrap();
        assert_eq!(app.pii.review.suggestions(initials), vec![full_id]);
        app.change_mapping(MappingAction::AssignVariant(initials, full_id), cx);
        assert_eq!(app.editor.read(cx).text(), source);
        assert_eq!(
            app.pii.review.variant(initials).unwrap().category,
            Category::Person
        );
        let email = app
            .pii
            .review
            .variants()
            .iter()
            .find(|g| g.category == Category::Email)
            .unwrap()
            .identity;
        app.change_mapping(MappingAction::Owner(email, Some(full_id)), cx);
        app.apply_aliases(cx);
        assert_eq!(
            app.editor.read(cx).text(),
            "PERSON_1 · PERSON_1 · PERSON_1 · EMAIL_1"
        );
        assert_eq!(app.pii.review.remaining(), 0);
        assert_eq!(app.pii.review.applied().len(), 4);
        app.change_mapping(MappingAction::Rename(full_id, "CLIENT_1".into()), cx);
        assert_eq!(
            app.editor.read(cx).text(),
            "CLIENT_1 · CLIENT_1 · CLIENT_1 · EMAIL_1"
        );
        assert_eq!(app.pii.review.applied()[1].step.original(), "Павлова М.С.");
    });
    cx.run_until_parked();
    app.update_in(cx, |app, window, cx| {
        app.copy_markdown(&crate::CopyMarkdown, window, cx);
        assert_eq!(
            cx.read_from_clipboard().unwrap().text().unwrap(),
            app.editor.read(cx).text()
        );
        window.focus(&app.editor.read(cx).focus_handle(cx), cx)
    });
    cx.dispatch_action(mdoc_editor::Undo);
    cx.run_until_parked();
    app.update(cx, |app, cx| {
        assert_eq!(
            app.editor.read(cx).text(),
            "PERSON_1 · PERSON_1 · PERSON_1 · EMAIL_1"
        );
        assert_eq!(
            app.pii
                .review
                .identity(app.pii.review.variant_identity(full).unwrap())
                .unwrap()
                .alias,
            "PERSON_1"
        );
    });
    cx.dispatch_action(mdoc_editor::Redo);
    cx.run_until_parked();
    app.update_in(cx, |app, window, cx| {
        assert_eq!(
            app.editor.read(cx).text(),
            "CLIENT_1 · CLIENT_1 · CLIENT_1 · EMAIL_1"
        );
        let selected = app.pii.review.applied()[1].id;
        app.activate_annotation(APPLIED_ID | selected, window, cx);
        app.pii.mapping.set_scope(Scope::Mention);
        app.keep_originals(window, cx);
        assert_eq!(
            app.editor.read(cx).text(),
            "CLIENT_1 · Павлова М.С. · CLIENT_1 · EMAIL_1"
        );
    });
}

#[gpui::test]
fn homonym_split_survives_bulk_apply_rescan_undo_and_redo(cx: &mut gpui::TestAppContext) {
    let (app, cx) = crate::document_view_tests::boot(cx);
    let source = "Павлова Марина Сергеевна · Павлова М.С. · Павлова М.С.";
    let (full, initials) = app.update(cx, |app, cx| seed(app, source, cx));
    cx.run_until_parked();
    app.update(cx, |app, cx| {
        let full_id = app.pii.review.variant_identity(full).unwrap();
        app.change_mapping(MappingAction::AssignVariant(initials, full_id), cx);
        let range = app.pii.review.variant(initials).unwrap().mentions[1].clone();
        app.change_mapping(MappingAction::AssignCandidate(initials, range, None), cx);
        let edits = plan_all(&app.pii.review, source).unwrap();
        assert_eq!(
            edits.iter().map(|(_, a)| a.as_str()).collect::<Vec<_>>(),
            ["PERSON_1", "PERSON_1", "PERSON_2"]
        );
        app.apply_aliases(cx);
        assert_eq!(app.editor.read(cx).text(), "PERSON_1 · PERSON_1 · PERSON_2");
        let detached = app.pii.review.applied()[2].id;
        app.select_occurrence(Selection::Applied(detached), cx);
        app.change_mapping(
            MappingAction::AssignApplied(detached, Some(full_id), false),
            cx,
        );
        assert_eq!(app.editor.read(cx).text(), "PERSON_1 · PERSON_1 · PERSON_1");
        assert_eq!(app.selected_entity(), Some(full_id));
        assert_eq!(app.pii.mapping.alias.read(cx).value(), "PERSON_1");
    });
    cx.run_until_parked();
    app.update_in(cx, |app, window, cx| {
        window.focus(&app.editor.read(cx).focus_handle(cx), cx)
    });
    cx.dispatch_action(mdoc_editor::Undo);
    cx.run_until_parked();
    app.update(cx, |app, cx| {
        assert_eq!(app.editor.read(cx).text(), "PERSON_1 · PERSON_1 · PERSON_2")
    });
    cx.dispatch_action(mdoc_editor::Undo);
    cx.run_until_parked();
    app.update(cx, |app, cx| {
        assert_eq!(app.editor.read(cx).text(), source);
        assert_eq!(plan_all(&app.pii.review, source).unwrap()[2].1, "PERSON_2");
        app.pii.review.ingest(source, Vec::new()).unwrap();
        assert_eq!(plan_all(&app.pii.review, source).unwrap()[2].1, "PERSON_2");
    });
    cx.dispatch_action(mdoc_editor::Undo);
    cx.run_until_parked();
    app.update(cx, |app, _cx| {
        assert_eq!(plan_all(&app.pii.review, source).unwrap()[2].1, "PERSON_1")
    });
    cx.dispatch_action(mdoc_editor::Redo);
    cx.run_until_parked();
    app.update(cx, |app, _cx| {
        assert_eq!(plan_all(&app.pii.review, source).unwrap()[2].1, "PERSON_2")
    });
}

#[gpui::test]
fn live_identity_panel_and_popup_fit_both_themes_and_narrow_windows(cx: &mut gpui::TestAppContext) {
    let (app, cx) = crate::document_view_tests::boot(cx);
    for theme in [crate::Theme::Dark, crate::Theme::Light] {
        for (width, height) in [(1500., 900.), (640., 480.)] {
            cx.simulate_resize(gpui::size(px(width), px(height)));
            app.update_in(cx, |app, window, cx| {
                app.theme.set(theme);
                seed(
                    app,
                    "Павлова Марина Сергеевна · Павлова М.С. · marina@example.invalid",
                    cx,
                );
                app.pii.mapping.open = false;
                app.toggle_replacements_panel(window, cx);
            });
            cx.run_until_parked();
            cx.update(|window, cx| window.draw(cx).clear(cx));
            let panel = cx.debug_bounds("identity-panel").unwrap();
            assert!(
                panel.size.height > px(height * 0.7),
                "vertical panel {panel:?}"
            );
            assert!(panel.size.width <= px(380.) && panel.left() > px(0.));
            assert!(
                panel.left() >= px(0.)
                    && panel.right() <= px(width)
                    && panel.bottom() <= px(height),
                "panel {panel:?}"
            );
            for control in ["close-replacements", "apply-identity-map"] {
                let bounds = cx.debug_bounds(control).unwrap();
                assert!(
                    bounds.left() >= panel.left()
                        && bounds.right() <= panel.right()
                        && bounds.top() >= panel.top()
                        && bounds.bottom() <= panel.bottom(),
                    "{control}: {bounds:?} in {panel:?}"
                );
            }
            assert!(cx.debug_bounds("copy-for-ai").is_none());
            assert!(cx.debug_bounds("pseudonym-review-bar").is_none());
            let apply = cx.debug_bounds("apply-identity-map").unwrap().center();
            cx.simulate_click(apply, Default::default());
            cx.run_until_parked();
            app.update(cx, |app, cx| {
                assert!(app.editor.read(cx).text().contains("PERSON_1"))
            });
        }
    }
}

#[gpui::test]
fn alias_rename_ignores_pasted_lookalikes(cx: &mut gpui::TestAppContext) {
    let (app, cx) = crate::document_view_tests::boot(cx);
    let source = "Павлова Марина Сергеевна · Павлова М.С. · pasted CLIENT_9";
    let (full, initials) = app.update(cx, |app, cx| seed(app, source, cx));
    cx.run_until_parked();
    app.update(cx, |app, cx| {
        let identity = app.pii.review.variant_identity(full).unwrap();
        app.change_mapping(MappingAction::AssignVariant(initials, identity), cx);
        assert_eq!(app.editor.read(cx).text(), source);
        app.apply_aliases(cx);
        assert_eq!(
            app.editor.read(cx).text(),
            "PERSON_1 · PERSON_1 · pasted CLIENT_9"
        );
        app.change_mapping(MappingAction::Rename(identity, "CLIENT_9".into()), cx);
        assert!(app.pii.error.as_ref().unwrap().contains("present"));
        assert_eq!(
            app.editor.read(cx).text(),
            "PERSON_1 · PERSON_1 · pasted CLIENT_9"
        );
        let end = app.editor.read(cx).text().len();
        app.editor
            .update(cx, |e, cx| e.replace_range(end..end, " · PERSON_1", cx));
    });
    cx.run_until_parked();
    app.update(cx, |app, cx| {
        let identity = app.pii.review.variant_identity(full).unwrap();
        app.change_mapping(MappingAction::Rename(identity, "CLIENT_1".into()), cx);
        assert_eq!(
            app.editor.read(cx).text(),
            "CLIENT_1 · CLIENT_1 · pasted CLIENT_9 · PERSON_1"
        );
        assert_eq!(app.pii.review.applied().len(), 2);
    });
}

#[gpui::test]
fn inline_keep_is_metadata_only_and_undoable(cx: &mut gpui::TestAppContext) {
    let (app, cx) = crate::document_view_tests::boot(cx);
    let source = "Павлова Марина Сергеевна · Павлова М.С. · Павлова М.С.";
    let (_, initials) = app.update(cx, |app, cx| seed(app, source, cx));
    cx.run_until_parked();
    app.update_in(cx, |app, window, cx| {
        let range = app.pii.review.variant(initials).unwrap().mentions[0].clone();
        let annotation = app.pii.review.annotation_id(initials, &range).unwrap();
        app.activate_annotation(annotation, window, cx);
        app.pii.mapping.set_scope(Scope::Mention);
        app.keep_originals(window, cx);
        window.focus(&app.editor.read(cx).focus_handle(cx), cx);
        assert_eq!(app.editor.read(cx).text(), source);
        assert_eq!(app.pii.review.remaining(), 2);
    });
    cx.run_until_parked();
    cx.dispatch_action(mdoc_editor::Undo);
    cx.run_until_parked();
    app.update(cx, |app, cx| {
        assert_eq!(app.editor.read(cx).text(), source);
        assert_eq!(app.pii.review.remaining(), 3);
    });
    cx.dispatch_action(mdoc_editor::Redo);
    cx.run_until_parked();
    app.update(cx, |app, cx| {
        assert_eq!(app.editor.read(cx).text(), source);
        assert_eq!(app.pii.review.remaining(), 2);
    });
}

#[gpui::test]
fn bulk_apply_preserves_first_occurrence_split(cx: &mut gpui::TestAppContext) {
    let (app, cx) = crate::document_view_tests::boot(cx);
    let source = "Павлова Марина Сергеевна · Павлова М.С. · Павлова М.С.";
    let (_, initials) = app.update(cx, |app, cx| seed(app, source, cx));
    cx.run_until_parked();
    app.update(cx, |app, cx| {
        let first = app.pii.review.variant(initials).unwrap().mentions[0].clone();
        app.change_mapping(MappingAction::AssignCandidate(initials, first, None), cx);
        let expected: Vec<_> = plan_all(&app.pii.review, source)
            .unwrap()
            .into_iter()
            .map(|(_, alias)| alias)
            .collect();
        app.apply_aliases(cx);
        assert_eq!(app.editor.read(cx).text(), expected.join(" · "));
        for applied in app.pii.review.applied() {
            assert_eq!(
                applied.step.after.as_ref(),
                app.pii
                    .review
                    .identity(applied.step.identity)
                    .unwrap()
                    .alias
            );
        }
    });
}

#[gpui::test]
fn owner_picker_click_links_without_merging_identities(cx: &mut gpui::TestAppContext) {
    let (app, cx) = crate::document_view_tests::boot(cx);
    for theme in [crate::Theme::Dark, crate::Theme::Light] {
        cx.simulate_resize(gpui::size(px(1500.), px(1000.)));
        let (full, email) = app.update_in(cx, |app, window, cx| {
            app.theme.set(theme);
            let (full, initials) = seed(
                app,
                "Павлова Марина Сергеевна · Павлова М.С. · marina@example.invalid",
                cx,
            );
            app.change_mapping(MappingAction::AssignVariant(initials, full), cx);
            app.apply_aliases(cx);
            let email = app.pii.review.applied().last().unwrap().step.identity;
            app.reveal_replacement(email, window, cx);
            (full, email)
        });
        cx.run_until_parked();
        cx.update(|window, cx| window.draw(cx).clear(cx));
        let owner = cx.debug_bounds("direct-owner").unwrap().center();
        cx.simulate_click(owner, Default::default());
        cx.run_until_parked();
        let key = gpui::ElementId::from(gpui::SharedString::from(format!("direct-owner-{full}")));
        app.update_in(cx, |app, window, cx| {
            window.focus(app.pii.popup_controls.borrow().get(&key).unwrap(), cx)
        });
        cx.update(|window, cx| {
            window.refresh();
            window.draw(cx).clear(cx);
        });
        cx.update(|window, cx| {
            window.refresh();
            window.draw(cx).clear(cx);
        });
        let target = cx
            .debug_bounds(Box::leak(format!("direct-owner-{full}").into_boxed_str()))
            .unwrap()
            .center();
        cx.simulate_click(target, Default::default());
        cx.run_until_parked();
        app.update(cx, |app, cx| {
            assert_eq!(app.pii.review.identity(email).unwrap().owner, Some(full));
            assert_eq!(app.editor.read(cx).text(), "PERSON_1 · PERSON_1 · EMAIL_1");
            assert!(app.pii.popup.is_some());
        });
    }
}

#[gpui::test]
fn entity_rename_then_apply_covers_all_normalized_variants(cx: &mut gpui::TestAppContext) {
    let (app, cx) = crate::document_view_tests::boot(cx);
    let source = "Павлова Марина Сергеевна · Павлова М.С. · ПАВЛОВА МАРИНА СЕРГЕЕВНА";
    let (full, _) = app.update(cx, |app, cx| seed(app, source, cx));
    cx.run_until_parked();
    app.update(cx, |app, cx| {
        let start = source.find("ПАВЛОВА").unwrap();
        app.pii
            .review
            .ingest(
                source,
                vec![pii::Detection {
                    range: start..source.len(),
                    category: Category::Person,
                    score: 0.9,
                    recognizer: pii::Recognizer::Model,
                }],
            )
            .unwrap();
        let identity = app.pii.review.variant_identity(full).unwrap();
        app.change_mapping(MappingAction::Rename(identity, "CLIENT_1".into()), cx);
        app.apply_aliases(cx);
        assert_eq!(app.editor.read(cx).text(), "CLIENT_1 · ORG_1 · CLIENT_1");
        for a in app.pii.review.applied() {
            assert_eq!(
                a.step.after.as_ref(),
                app.pii.review.identity(a.step.identity).unwrap().alias
            );
        }
    });
}

#[gpui::test]
fn identity_keep_includes_linked_variants_but_preserves_a_separated_homonym(
    cx: &mut gpui::TestAppContext,
) {
    let (app, cx) = crate::document_view_tests::boot(cx);
    let source = "Павлова Марина Сергеевна · Павлова М.С. · Павлова М.С.";
    app.update_in(cx, |app, window, cx| {
        let (full, initials) = seed(app, source, cx);
        let identity = app.pii.review.variant_identity(full).unwrap();
        app.change_mapping(MappingAction::AssignVariant(initials, identity), cx);
        let detached = app.pii.review.variant(initials).unwrap().mentions[1].clone();
        app.change_mapping(
            MappingAction::AssignCandidate(initials, detached.clone(), None),
            cx,
        );
        let detached_id = app
            .pii
            .review
            .occurrence_identity(initials, &detached)
            .unwrap();
        let revision = app.editor.read(cx).revision();
        let dirty = app.dirty(cx);
        app.select_occurrence(Selection::Entity(identity), cx);
        app.pii.mapping.set_scope(Scope::Entity);
        app.keep_originals(window, cx);
        assert_eq!(app.editor.read(cx).text(), source);
        assert_ne!(
            app.editor.read(cx).revision(),
            revision,
            "Keep invalidates older scan revisions without editing source"
        );
        assert_eq!(app.dirty(cx), dirty);
        assert_eq!(app.pii.review.remaining(), 1);
        assert_eq!(
            app.pii.review.occurrence_identity(initials, &detached),
            Some(detached_id)
        );
        window.focus(&app.editor.read(cx).focus_handle(cx), cx);
    });
    cx.run_until_parked();
    cx.dispatch_action(mdoc_editor::Undo);
    cx.run_until_parked();
    app.update(cx, |app, cx| {
        assert_eq!(app.editor.read(cx).text(), source);
        assert_eq!(app.pii.review.remaining(), 3);
    });
    cx.dispatch_action(mdoc_editor::Redo);
    cx.run_until_parked();
    app.update(cx, |app, cx| {
        assert_eq!(app.pii.review.remaining(), 1);
        app.pii.review.ingest(source, Vec::new()).unwrap();
        assert_eq!(app.pii.review.remaining(), 1);
        app.apply_aliases(cx);
        let text = app.editor.read(cx).text();
        assert!(text.starts_with("Павлова Марина Сергеевна · Павлова М.С. · PERSON_"));
        assert_eq!(app.pii.review.applied().len(), 1);
    });
}

#[gpui::test]
fn single_copy_with_pending_replacements_preserves_exact_source_and_history(
    cx: &mut gpui::TestAppContext,
) {
    let (app, cx) = crate::document_view_tests::boot(cx);
    let source = "# Review\n\nПавлова Марина Сергеевна · Павлова М.С.\n\n[mail](marina@example.invalid)\n<!-- untouched -->\n";
    app.update_in(cx, |app, window, cx| {
        let (full, _) = seed(app, source, cx);
        let identity = app.pii.review.variant_identity(full).unwrap();
        let email = app
            .pii
            .review
            .variants()
            .iter()
            .find(|g| g.category == Category::Email)
            .unwrap()
            .identity;
        app.change_mapping(MappingAction::Owner(email, Some(identity)), cx);
        let revision = app.editor.read(cx).revision();
        let history = app.editor.read(cx).history_id();
        let pending = app.pii.review.remaining();
        app.copy_markdown(&crate::CopyMarkdown, window, cx);
        assert_eq!(
            cx.read_from_clipboard().unwrap().text().as_deref(),
            Some(source)
        );
        assert_eq!(app.editor.read(cx).revision(), revision);
        assert_eq!(app.editor.read(cx).history_id(), history);
        assert_eq!(app.pii.review.remaining(), pending);
        assert!(app.pii.review.applied().is_empty());
    });
}

#[gpui::test]
fn panel_click_reveals_exact_word_and_editor_popup_retains_workspace(
    cx: &mut gpui::TestAppContext,
) {
    let (app, cx) = crate::document_view_tests::boot(cx);
    for theme in [crate::Theme::Dark, crate::Theme::Light] {
        for (width, height, preview) in [
            (1500., 900., false),
            (1100., 760., true),
            (640., 480., true),
        ] {
            cx.simulate_resize(gpui::size(px(width), px(height)));
            let identity = app.update(cx, |app, cx| {
                app.theme.set(theme);
                app.preview.visible = preview;
                app.original_selected = preview;
                let (full, _) = seed(
                    app,
                    "Павлова Марина Сергеевна · Павлова М.С. · marina@example.invalid",
                    cx,
                );
                app.pii
                    .mapping
                    .search
                    .update(cx, |input, cx| input.set_value("Павлова".into(), cx));
                app.pii.review.variant_identity(full).unwrap()
            });
            cx.run_until_parked();
            cx.update(|window, cx| window.draw(cx).clear(cx));
            // The header expands its group; the mention row selects (ADR 0033).
            let row = cx
                .debug_bounds(Box::leak(format!("identity-{identity}").into_boxed_str()))
                .unwrap();
            cx.simulate_click(row.center(), Default::default());
            cx.run_until_parked();
            cx.update(|window, cx| window.draw(cx).clear(cx));
            let first = app.read_with(cx, |app, _| app.identity_occurrences(identity)[0].0);
            let mention = cx
                .debug_bounds(Box::leak(format!("mention-{first}").into_boxed_str()))
                .unwrap();
            cx.simulate_click(mention.center(), Default::default());
            cx.run_until_parked();
            // A panel selection reveals the word without a popup; the editor
            // path then opens the popup beside the panel.
            app.update_in(cx, |app, window, cx| {
                assert!(app.pii.popup.is_none());
                assert_eq!(app.active_annotation(), Some(first));
                let start = app.active_replacement_range().unwrap().start;
                app.navigate_identity_mention(first, start, window, cx);
            });
            cx.run_until_parked();
            for _ in 0..3 {
                cx.update(|window, cx| {
                    window.refresh();
                    window.draw(cx).clear(cx);
                });
            }
            let panel = cx.debug_bounds("identity-panel").unwrap();
            let popup = cx.debug_bounds("pseudonym-popup").unwrap();
            assert!(panel.size.height > px(height * 0.7));
            if let Some(divider) = cx.debug_bounds("preview-divider") {
                assert!(
                    panel.right() <= divider.left(),
                    "replacements sit before Original: panel {panel:?} divider {divider:?}"
                );
            }
            assert!(
                popup.right() <= panel.left(),
                "popup {popup:?} panel {panel:?}"
            );
            app.update_in(cx, |app, window, cx| {
                assert_eq!(app.selected_entity(), Some(identity));
                assert!(app.pii.popup.is_some());
                let annotation = app.active_annotation().unwrap();
                let word = app.editor.read(cx).annotation_bounds(annotation).unwrap();
                assert!(
                    word.top() >= app.scroll.bounds().top()
                        && word.bottom() <= app.scroll.bounds().bottom()
                );
                assert!(
                    popup.top() >= word.bottom() || popup.bottom() <= word.top(),
                    "popup {popup:?} overlaps active word {word:?}"
                );
                assert!(
                    popup.left() >= px(0.)
                        && popup.right() <= px(width)
                        && popup.bottom() <= px(height)
                );
                assert!(panel.top() >= px(0.) && panel.bottom() <= px(height));
                assert_eq!(app.pii.mapping.search.read(cx).value(), "Павлова");
                app.close_replacements(window, cx);
                assert!(app.pii.popup.is_none());
                assert!(app.editor.read(cx).focus_handle(cx).is_focused(window));
            });
        }
    }
}

#[gpui::test]
fn visible_alias_draft_and_apply_share_one_undo_step(cx: &mut gpui::TestAppContext) {
    let (app, cx) = crate::document_view_tests::boot(cx);
    for theme in [crate::Theme::Dark, crate::Theme::Light] {
        for (width, height) in [(1500., 900.), (640., 480.)] {
            cx.simulate_resize(gpui::size(px(width), px(height)));
            let source = "Павлова Марина Сергеевна · Павлова М.С.";
            app.update_in(cx, |app, window, cx| {
                app.theme.set(theme);
                let (full, _) = seed(app, source, cx);
                let identity = app.pii.review.variant_identity(full).unwrap();
                app.reveal_replacement(identity, window, cx);
                window.focus(&app.pii.mapping.alias.read(cx).focus_handle(cx), cx);
            });
            cx.simulate_input("CLIENT_7");
            cx.run_until_parked();
            cx.update(|window, cx| window.draw(cx).clear(cx));
            assert!(cx.debug_bounds("rename-identity").is_none());
            let apply = cx.debug_bounds("apply-identity-map").unwrap();
            cx.simulate_click(apply.center(), Default::default());
            cx.run_until_parked();
            app.update_in(cx, |app, window, cx| {
                assert_eq!(app.editor.read(cx).text(), "CLIENT_7 · ORG_1");
                assert!(app.pii.popup.is_some());
                assert!(app.pii.mapping.field_error.is_none());
                window.focus(&app.editor.read(cx).focus_handle(cx), cx);
            });
            cx.dispatch_action(mdoc_editor::Undo);
            cx.run_until_parked();
            app.update(cx, |app, cx| {
                assert_eq!(app.editor.read(cx).text(), source);
                assert!(
                    app.pii.review.active_identities().into_iter().all(|id| app
                        .pii
                        .review
                        .identity(id)
                        .unwrap()
                        .alias
                        != "CLIENT_7")
                );
            });
            cx.dispatch_action(mdoc_editor::Redo);
            cx.run_until_parked();
            assert_eq!(
                app.read_with(cx, |app, cx| app.editor.read(cx).text().to_owned()),
                "CLIENT_7 · ORG_1"
            );
        }
    }
}

#[gpui::test]
fn same_wording_links_preserve_detached_homonyms_before_and_after_apply(
    cx: &mut gpui::TestAppContext,
) {
    let (app, cx) = crate::document_view_tests::boot(cx);
    let source = "Павлова Марина Сергеевна · Павлова М.С. · Павлова М.С. · Павлова М.С.";
    let (full, initials) = app.update(cx, |app, cx| seed(app, source, cx));
    cx.run_until_parked();
    app.update_in(cx, |app, window, cx| {
        let ranges = app.pii.review.variant(initials).unwrap().mentions.clone();
        app.change_mapping(
            MappingAction::AssignCandidate(initials, ranges[1].clone(), None),
            cx,
        );
        let detached = app
            .pii
            .review
            .occurrence_identity(initials, &ranges[1])
            .unwrap();
        let annotation = app.pii.review.annotation_id(initials, &ranges[0]).unwrap();
        app.activate_annotation(annotation, window, cx);
        assert_eq!(app.scoped_ranges(Scope::Wording).len(), 2);
        app.link_to_entity(full, cx);
        assert_eq!(
            app.pii.review.occurrence_identity(initials, &ranges[1]),
            Some(detached)
        );
        assert_eq!(app.editor.read(cx).text(), source);
        app.apply_replacements(cx);
        assert_eq!(
            app.editor.read(cx).text(),
            "PERSON_1 · PERSON_1 · ORG_2 · PERSON_1"
        );
        assert!(app.pii.popup.is_some());
        assert_eq!(app.scoped_ranges(Scope::Wording).len(), 2);
        let anchor = app.active_replacement_range().unwrap().start;
        app.separate_with_new_alias(cx);
        assert_eq!(app.active_replacement_range().unwrap().start, anchor);
        assert_eq!(
            app.editor.read(cx).text(),
            "PERSON_1 · PERSON_2 · ORG_2 · PERSON_2"
        );
        assert_eq!(app.pii.review.applied()[2].step.identity, detached);
        assert_eq!(app.pii.review.applied()[1].step.original(), "Павлова М.С.");
    });
    cx.run_until_parked();
    app.update_in(cx, |app, window, cx| {
        window.focus(&app.editor.read(cx).focus_handle(cx), cx)
    });
    cx.dispatch_action(mdoc_editor::Undo);
    cx.run_until_parked();
    assert_eq!(
        app.read_with(cx, |app, cx| app.editor.read(cx).text().to_owned()),
        "PERSON_1 · PERSON_1 · ORG_2 · PERSON_1"
    );
}

#[gpui::test]
fn category_origin_existing_alias_and_draft_lifecycle_are_explicit(cx: &mut gpui::TestAppContext) {
    let (app, cx) = crate::document_view_tests::boot(cx);
    let source = "Павлова Марина Сергеевна · Павлова М.С. · marina@example.invalid";
    let (full, initials) = app.update(cx, |app, cx| seed(app, source, cx));
    cx.run_until_parked();
    app.update_in(cx, |app, window, cx| {
        app.reveal_replacement(initials, window, cx);
        app.pii
            .mapping
            .alias
            .update(cx, |input, cx| input.set_value("PERSON_1".into(), cx));
        assert!(!app.confirm_alias(false, Applying::Nothing, cx));
        assert_eq!(app.pii.review.variant_identity(initials), Some(initials));
        assert!(app.pii.mapping.field_error.is_some());
        app.link_to_entity(full, cx);
        assert_eq!(app.pii.mapping.alias.read(cx).value(), "PERSON_1");
        assert!(app.pii.popup.is_some());
        app.pii
            .mapping
            .alias
            .update(cx, |input, cx| input.set_value("PERSON_99".into(), cx));
        app.confirm_alias(true, Applying::Nothing, cx);
        assert!(app.pii.review.identity(full).unwrap().custom_alias);
        app.pii.mapping.scope = Scope::Entity;
        app.correct_category(Category::Organization, cx);
        assert_eq!(
            app.pii.review.identity(full).unwrap().alias,
            "PERSON_99",
            "user-entered generated-looking name is still custom"
        );
        app.pii
            .mapping
            .alias
            .update(cx, |input, cx| input.set_value("DISCARDED".into(), cx));
        let email = app
            .pii
            .review
            .candidates()
            .iter()
            .find(|c| app.pii.review.variant(c.variant).unwrap().category == Category::Email)
            .unwrap()
            .id;
        app.navigate_identity_mention(email, source.find("marina@").unwrap(), window, cx);
        assert_eq!(app.pii.mapping.alias.read(cx).value(), "EMAIL_1");
        assert_eq!(app.editor.read(cx).text(), source);
    });
}

#[gpui::test]
fn table_occurrence_popup_uses_exact_painted_word_in_both_themes(cx: &mut gpui::TestAppContext) {
    let (app, cx) = crate::document_view_tests::boot(cx);
    for theme in [crate::Theme::Light, crate::Theme::Dark] {
        cx.simulate_resize(gpui::size(px(1500.), px(900.)));
        app.update_in(cx, |app, window, cx| {
            app.theme.set(theme);
            let (_, initials) = seed(app, "Павлова Марина Сергеевна\n\n| Заказчик | Исполнитель |\n| --- | --- |\n| **ИП**<br>Павлова М.С. | marina@example.invalid |\n", cx);
            app.reveal_replacement(initials, window, cx);
        });
        cx.run_until_parked();
        for _ in 0..3 {
            cx.update(|window, cx| {
                window.refresh();
                window.draw(cx).clear(cx);
            });
        }
        let popup = cx.debug_bounds("pseudonym-popup").unwrap();
        app.update(cx, |app, cx| {
            let annotation = app.active_annotation().unwrap();
            let range = app.active_replacement_range().unwrap();
            let word = app.editor.read(cx).annotation_bounds(annotation).unwrap();
            assert_eq!(&app.editor.read(cx).text()[range.clone()], "Павлова М.С.");
            assert!(word.size.width > px(20.) && word.size.width < px(400.));
            assert!(popup.top() >= word.bottom() || popup.bottom() <= word.top());
            let context = app.readable_mention(&range, cx);
            assert!(!format!("{}{}{}", context.0, context.1, context.2).contains("<br>"));
            assert!(app.pii.mapping.open && app.pii.popup.is_some());
        });
    }
}

#[gpui::test]
fn external_source_edit_discards_unconfirmed_mapping_draft(cx: &mut gpui::TestAppContext) {
    let (app, cx) = crate::document_view_tests::boot(cx);
    let source = "Павлова Марина Сергеевна · Павлова М.С.";
    let (_, initials) = app.update(cx, |app, cx| seed(app, source, cx));
    cx.run_until_parked();
    app.update_in(cx, |app, window, cx| {
        app.reveal_replacement(initials, window, cx);
        app.pii
            .mapping
            .alias
            .update(cx, |input, cx| input.set_value("UNCONFIRMED".into(), cx));
        app.editor
            .update(cx, |editor, cx| editor.replace_range(0..0, "Edited: ", cx));
    });
    cx.run_until_parked();
    app.update(cx, |app, cx| {
        assert!(app.pii.popup.is_none());
        assert!(app.selected_entity().is_none());
        assert!(app.pii.mapping.alias.read(cx).value().is_empty());
        assert!(app.pii.mapping.open);
        assert!(
            !app.pii.review.active_identities().into_iter().any(|id| app
                .pii
                .review
                .identity(id)
                .unwrap()
                .alias
                == "UNCONFIRMED")
        );
    });
}

#[gpui::test]
fn popup_apply_undo_and_keep_follow_scope_and_stay_on_the_mention(cx: &mut gpui::TestAppContext) {
    let (app, cx) = crate::document_view_tests::boot(cx);
    let source = "Павлова М.С. · Павлова М.С. · marina@example.invalid";
    let (_, initials) = app.update(cx, |app, cx| seed(app, source, cx));
    cx.run_until_parked();
    app.update_in(cx, |app, window, cx| {
        let range = app.pii.review.variant(initials).unwrap().mentions[0].clone();
        let annotation = app.pii.review.annotation_id(initials, &range).unwrap();
        app.activate_annotation(annotation, window, cx);
        // Same wording is the default scope; the email stays proposed.
        app.apply_scope(cx);
        assert_eq!(
            app.editor.read(cx).text(),
            "ORG_1 · ORG_1 · marina@example.invalid"
        );
        assert_eq!(app.pii.review.remaining(), 1);
        assert!(app.pii.popup.is_some());
        assert!(app.active_annotation().unwrap() & APPLIED_ID != 0);
        window.focus(&app.editor.read(cx).focus_handle(cx), cx);
    });
    cx.dispatch_action(mdoc_editor::Undo);
    cx.run_until_parked();
    app.update(cx, |app, cx| {
        assert_eq!(app.editor.read(cx).text(), source, "Apply is one undo step");
    });
    cx.dispatch_action(mdoc_editor::Redo);
    cx.run_until_parked();
    app.update_in(cx, |app, window, cx| {
        let id = app.pii.review.applied()[0].id;
        app.activate_annotation(APPLIED_ID | id, window, cx);
        app.pii.mapping.set_scope(Scope::Mention);
        let (_, applied) = app.scoped_mentions();
        app.undo_replacements(applied.into_iter().collect(), cx);
        assert_eq!(
            app.editor.read(cx).text(),
            "Павлова М.С. · ORG_1 · marina@example.invalid"
        );
        // Back to proposed with the same alias, and the popup stays on it.
        assert_eq!(app.pii.review.remaining(), 2);
        assert!(app.pii.popup.is_some());
        let annotation = app.active_annotation().unwrap();
        assert_eq!(annotation & APPLIED_ID, 0);
        let candidate = app.pii.review.candidate(annotation).unwrap();
        assert_eq!(candidate.range, 0.."Павлова М.С.".len());
        assert_eq!(
            app.pii
                .review
                .identity(app.selected_entity().unwrap())
                .unwrap()
                .alias,
            "ORG_1"
        );
        // Same wording now spans one proposal and one applied mention.
        let (candidates, applied) = app.scoped_mentions();
        assert_eq!((candidates.len(), applied.len()), (1, 1));
        app.keep_originals(window, cx);
        assert_eq!(
            app.editor.read(cx).text(),
            "Павлова М.С. · Павлова М.С. · marina@example.invalid"
        );
        assert_eq!(app.pii.review.remaining(), 1);
        assert!(app.pii.popup.is_none());
        let history = app.editor.read(cx).history_id();
        assert_eq!(app.pii.mapping.kept_at(history), Some(2));
    });
    cx.dispatch_action(mdoc_editor::Undo);
    cx.run_until_parked();
    app.update(cx, |app, cx| {
        assert_eq!(
            app.editor.read(cx).text(),
            "Павлова М.С. · ORG_1 · marina@example.invalid",
            "Keep original across both states is one undo step"
        );
        let history = app.editor.read(cx).history_id();
        assert_eq!(app.pii.mapping.kept_at(history), None);
        app.pii.review.refresh(app.editor.read(cx).text());
        assert_eq!(app.pii.review.remaining(), 2);
    });
}

#[gpui::test]
fn undo_replacement_keeps_a_separated_homonym_on_its_identity(cx: &mut gpui::TestAppContext) {
    let (app, cx) = crate::document_view_tests::boot(cx);
    let source = "Павлова М.С. · Павлова М.С.";
    let (_, initials) = app.update(cx, |app, cx| seed(app, source, cx));
    cx.run_until_parked();
    app.update_in(cx, |app, window, cx| {
        let second = app.pii.review.variant(initials).unwrap().mentions[1].clone();
        app.change_mapping(
            MappingAction::AssignCandidate(initials, second.clone(), None),
            cx,
        );
        let separated = app
            .pii
            .review
            .occurrence_identity(initials, &second)
            .unwrap();
        app.apply_aliases(cx);
        assert_eq!(app.editor.read(cx).text(), "ORG_1 · ORG_2");
        let id = app.pii.review.applied()[1].id;
        app.activate_annotation(APPLIED_ID | id, window, cx);
        app.undo_replacements([id].into(), cx);
        assert_eq!(app.editor.read(cx).text(), "ORG_1 · Павлова М.С.");
        let candidate = app.pii.review.candidates()[0].clone();
        assert_eq!(
            app.pii
                .review
                .occurrence_identity(candidate.variant, &candidate.range),
            Some(separated)
        );
        app.apply_aliases(cx);
        assert_eq!(app.editor.read(cx).text(), "ORG_1 · ORG_2");
    });
}

#[gpui::test]
fn panel_mention_undo_icon_reverts_only_that_mention(cx: &mut gpui::TestAppContext) {
    let (app, cx) = crate::document_view_tests::boot(cx);
    for theme in [crate::Theme::Dark, crate::Theme::Light] {
        let source = "Павлова М.С. · Павлова М.С.";
        app.update_in(cx, |app, window, cx| {
            app.theme.set(theme);
            let (_, initials) = seed(app, source, cx);
            let range = app.pii.review.variant(initials).unwrap().mentions[0].clone();
            let annotation = app.pii.review.annotation_id(initials, &range).unwrap();
            app.activate_annotation(annotation, window, cx);
            app.apply_scope(cx);
            assert_eq!(app.editor.read(cx).text(), "ORG_1 · ORG_1");
        });
        cx.run_until_parked();
        cx.update(|window, cx| window.draw(cx).clear(cx));
        let annotation = app.update(cx, |app, _| app.active_annotation().unwrap());
        let selector = Box::leak(format!("mention-undo-{annotation}").into_boxed_str());
        let undo = cx
            .debug_bounds(selector)
            .expect("the active applied mention row offers undo");
        let panel = cx.debug_bounds("identity-panel").unwrap();
        assert!(undo.left() >= panel.left() && undo.right() <= panel.right());
        cx.simulate_click(undo.center(), Default::default());
        cx.run_until_parked();
        app.update(cx, |app, cx| {
            assert_eq!(app.editor.read(cx).text(), "Павлова М.С. · ORG_1");
            assert_eq!(app.pii.review.applied().len(), 1);
        });
    }
}

#[gpui::test]
fn group_rows_toggle_and_scope_outlines_follow_group_chip_and_hover(cx: &mut gpui::TestAppContext) {
    let (app, cx) = crate::document_view_tests::boot(cx);
    let source = "Павлова Марина Сергеевна · Павлова М.С. · Павлова М.С. · marina@example.invalid";
    let identity = app.update(cx, |app, cx| {
        let (_, initials) = seed(app, source, cx);
        app.pii.review.variant_identity(initials).unwrap()
    });
    cx.run_until_parked();
    let selector = Box::leak(format!("identity-{identity}").into_boxed_str());
    let click_row = |cx: &mut gpui::VisualTestContext| {
        cx.update(|window, cx| window.draw(cx).clear(cx));
        let row = cx.debug_bounds(selector).unwrap();
        cx.simulate_click(row.center(), Default::default());
        cx.run_until_parked();
        cx.update(|window, cx| window.draw(cx).clear(cx));
    };
    let outlines = |cx: &mut gpui::VisualTestContext| {
        app.read_with(cx, |app, cx| app.editor.read(cx).outlines().len())
    };
    // A header click only expands: nothing is selected (ADR 0033).
    click_row(cx);
    app.read_with(cx, |app, _| {
        assert!(app.pii.mapping.expanded.contains(&identity));
        assert_eq!(app.selected_entity(), None);
        assert!(app.pii.popup.is_none());
    });
    let first = app.read_with(cx, |app, _| app.identity_occurrences(identity)[0].0);
    let mention = cx
        .debug_bounds(Box::leak(format!("mention-{first}").into_boxed_str()))
        .unwrap();
    cx.simulate_click(mention.center(), Default::default());
    cx.run_until_parked();
    cx.update(|window, cx| window.draw(cx).clear(cx));
    // A panel selection shows inline controls instead of the popup; its
    // scope starts at this mention and the selector widens the outline.
    app.read_with(cx, |app, _| {
        assert_eq!(app.selected_entity(), Some(identity));
        assert!(app.pii.popup.is_none());
        assert!(app.pii.mapping.panel_selected);
    });
    assert!(cx.debug_bounds("inline-controls").is_some());
    assert_eq!(outlines(cx), 1);
    let same = cx.debug_bounds("inline-scope-same").unwrap().center();
    cx.simulate_click(same, Default::default());
    cx.run_until_parked();
    cx.update(|window, cx| window.draw(cx).clear(cx));
    // Same wording: both initials.
    assert_eq!(outlines(cx), 2);
    // Collapsing leaves the selection alone.
    click_row(cx);
    app.read_with(cx, |app, _| {
        assert!(!app.pii.mapping.expanded.contains(&identity));
        assert_eq!(app.selected_entity(), Some(identity));
    });
}

#[gpui::test]
fn panel_drops_link_one_mention_merge_entities_and_split_new_entities(
    cx: &mut gpui::TestAppContext,
) {
    let (app, cx) = crate::document_view_tests::boot(cx);
    let source = "Павлова Марина Сергеевна · Павлова М.С. · Павлова М.С. · marina@example.invalid";
    app.update(cx, |app, cx| {
        let (full, initials) = seed(app, source, cx);
        let review = &app.pii.review;
        let full_id = review.variant_identity(full).unwrap();
        let initials_id = review.variant_identity(initials).unwrap();
        let first = review.variant(initials).unwrap().mentions[0].clone();
        let second = review.variant(initials).unwrap().mentions[1].clone();
        // A mention dropped on another group links only that mention.
        app.drop_on_entity(
            &PanelDrag::Mention {
                identity: initials_id,
                range: first.clone(),
                original: "Павлова М.С.".into(),
            },
            full_id,
            cx,
        );
        // A panel drop does not open the popup.
        assert!(app.pii.popup.is_none());
        let review = &app.pii.review;
        assert_eq!(review.occurrence_identity(initials, &first), Some(full_id));
        assert_eq!(
            review.occurrence_identity(initials, &second),
            Some(initials_id)
        );
        // Dropping on its own group does nothing.
        app.drop_on_entity(
            &PanelDrag::Mention {
                identity: full_id,
                range: first.clone(),
                original: "Павлова М.С.".into(),
            },
            full_id,
            cx,
        );
        assert_eq!(
            app.pii.review.occurrence_identity(initials, &first),
            Some(full_id)
        );
        // "New entity" separates a mention from a multi-mention entity.
        app.drop_as_new_entity(
            &PanelDrag::Mention {
                identity: full_id,
                range: first.clone(),
                original: "Павлова М.С.".into(),
            },
            cx,
        );
        let split = app
            .pii
            .review
            .occurrence_identity(initials, &first)
            .unwrap();
        assert!(split != full_id && split != initials_id);
        // A group dropped on a group merges the entities.
        app.drop_on_entity(
            &PanelDrag::Entity {
                identity: split,
                original: "Павлова М.С.".into(),
            },
            initials_id,
            cx,
        );
        assert_eq!(
            app.pii.review.occurrence_identity(initials, &first),
            Some(initials_id)
        );
        assert_eq!(app.editor.read(cx).text(), source);
    });
}

#[gpui::test]
fn dragging_a_mention_over_free_panel_space_offers_the_next_alias(cx: &mut gpui::TestAppContext) {
    let (app, cx) = crate::document_view_tests::boot(cx);
    let source = "Павлова Марина Сергеевна · Павлова М.С. · Павлова М.С. · marina@example.invalid";
    let (initials, identity) = app.update(cx, |app, cx| {
        let (_, initials) = seed(app, source, cx);
        let identity = app.pii.review.variant_identity(initials).unwrap();
        app.select_occurrence(Selection::Entity(identity), cx);
        (initials, identity)
    });
    cx.run_until_parked();
    cx.update(|window, cx| window.draw(cx).clear(cx));
    let (annotation, second) =
        app.read_with(cx, |app, _| app.identity_occurrences(identity)[1].clone());
    let row = cx
        .debug_bounds(Box::leak(format!("mention-{annotation}").into_boxed_str()))
        .unwrap();
    assert!(cx.debug_bounds("replacement-new-entity").is_none());
    let left = gpui::MouseButton::Left;
    cx.simulate_mouse_down(row.center(), left, Default::default());
    let panel = cx.debug_bounds("identity-panel").unwrap();
    let free = gpui::point(panel.center().x, panel.bottom() - px(160.));
    for y in [row.center().y + px(6.), row.center().y + px(40.), free.y] {
        cx.simulate_mouse_move(gpui::point(row.center().x, y), left, Default::default());
        cx.run_until_parked();
        cx.update(|window, cx| window.draw(cx).clear(cx));
    }
    cx.simulate_mouse_move(free, left, Default::default());
    cx.update(|window, cx| window.draw(cx).clear(cx));
    let zone = cx
        .debug_bounds("replacement-new-entity")
        .expect("the free panel space offers a new alias while dragging");
    let expected = app.read_with(cx, |app, _| {
        app.pii.review.next_alias(Category::Organization)
    });
    assert_eq!(expected, "ORG_2");
    // The area fills the free space under the rows.
    assert!(zone.size.height > px(72.));
    cx.simulate_mouse_move(zone.center(), left, Default::default());
    cx.simulate_mouse_up(zone.center(), left, Default::default());
    cx.run_until_parked();
    app.read_with(cx, |app, _| {
        let review = &app.pii.review;
        let split = review.occurrence_identity(initials, &second).unwrap();
        assert_ne!(split, identity);
        assert_eq!(review.identity(split).unwrap().alias, "ORG_2");
        assert!(app.pii.popup.is_none());
    });
    cx.update(|window, cx| window.draw(cx).clear(cx));
    assert!(cx.debug_bounds("replacement-new-entity").is_none());
}

#[gpui::test]
fn panel_groups_expand_independently_filter_and_edit_from_headers(cx: &mut gpui::TestAppContext) {
    let (app, cx) = crate::document_view_tests::boot(cx);
    let source = "Павлова Марина Сергеевна · Павлова М.С. · Павлова М.С. · marina@example.invalid";
    let (full, initials) = app.update(cx, |app, cx| {
        let (full, initials) = seed(app, source, cx);
        let review = &app.pii.review;
        (
            review.variant_identity(full).unwrap(),
            review.variant_identity(initials).unwrap(),
        )
    });
    cx.run_until_parked();
    let click = |cx: &mut gpui::VisualTestContext, selector: String| {
        cx.update(|window, cx| window.draw(cx).clear(cx));
        let at = cx
            .debug_bounds(Box::leak(selector.into_boxed_str()))
            .unwrap()
            .center();
        cx.simulate_click(at, Default::default());
        cx.run_until_parked();
    };
    // Two groups stay open together.
    click(cx, format!("identity-{full}"));
    click(cx, format!("identity-{initials}"));
    app.read_with(cx, |app, _| {
        assert!(app.pii.mapping.expanded.contains(&full));
        assert!(app.pii.mapping.expanded.contains(&initials));
        assert_eq!(app.selected_entity(), None);
    });
    // The e-mail group is still collapsed, so the button expands all; then
    // nothing is collapsed and it collapses every group.
    click(cx, "replacement-expand-all".into());
    app.read_with(cx, |app, _| assert_eq!(app.pii.mapping.expanded.len(), 3));
    click(cx, "replacement-expand-all".into());
    app.read_with(cx, |app, _| assert!(app.pii.mapping.expanded.is_empty()));
    click(cx, "replacement-expand-all".into());
    app.read_with(cx, |app, cx| {
        assert_eq!(app.pii.mapping.expanded.len(), 3);
        assert_eq!(app.replacement_entries(cx).len(), 3 + 4);
    });
    // Apply one mention; the Applied filter shows only it and its group.
    app.update(cx, |app, cx| {
        let range = app.pii.review.variant(full).unwrap().mentions[0].clone();
        let annotation = app.pii.review.annotation_id(full, &range).unwrap();
        app.sync_replacement_annotation(annotation, cx);
        app.pii.mapping.set_scope(Scope::Mention);
        app.apply_scope(cx);
    });
    cx.run_until_parked();
    click(cx, "replacement-filter".into());
    click(cx, "replacement-filter-Applied".into());
    app.read_with(cx, |app, cx| {
        let entries = app.replacement_entries(cx);
        assert_eq!(entries.len(), 2);
        assert!(entries[1].1.mention().unwrap() & APPLIED_ID != 0);
    });
    click(cx, "replacement-filter".into());
    click(cx, "replacement-filter-All".into());
    // Header alias: click, type, Enter renames the whole entity.
    click(cx, format!("header-alias-{initials}"));
    cx.simulate_keystrokes("cmd-a");
    cx.simulate_input("CLIENT");
    cx.simulate_keystrokes("enter");
    cx.run_until_parked();
    app.read_with(cx, |app, _| {
        assert_eq!(app.pii.review.identity(initials).unwrap().alias, "CLIENT");
        assert_eq!(app.pii.mapping.header_edit, None);
    });
    // Header category: the dropdown recategorizes the entity.
    click(cx, format!("header-category-{initials}"));
    click(cx, "panel-category-PERSON".into());
    app.read_with(cx, |app, _| {
        assert_eq!(
            app.pii.review.identity(initials).unwrap().category,
            Category::Person
        );
    });
}

#[gpui::test]
fn panel_keyboard_moves_decides_and_triages(cx: &mut gpui::TestAppContext) {
    let (app, cx) = crate::document_view_tests::boot(cx);
    let source = "Павлова Марина Сергеевна · Павлова М.С. · Павлова М.С. · marina@example.invalid";
    let (full, initials) = app.update(cx, |app, cx| {
        let (full, initials) = seed(app, source, cx);
        let review = &app.pii.review;
        (
            review.variant_identity(full).unwrap(),
            review.variant_identity(initials).unwrap(),
        )
    });
    cx.run_until_parked();
    let text = |cx: &mut gpui::VisualTestContext| {
        app.read_with(cx, |app, cx| app.editor.read(cx).text().to_owned())
    };
    let key = |cx: &mut gpui::VisualTestContext, keys: &str| {
        cx.simulate_keystrokes(keys);
        cx.run_until_parked();
        cx.update(|window, cx| window.draw(cx).clear(cx));
    };
    // ↓ from search enters the list on the first header; Enter opens it.
    app.update_in(cx, |app, window, cx| {
        window.focus(&app.pii.mapping.search.read(cx).focus_handle(cx), cx);
    });
    key(cx, "down");
    app.read_with(cx, |app, _| {
        assert_eq!(app.pii.mapping.cursor, Some(PanelCursor::Header(full)));
    });
    key(cx, "enter");
    app.read_with(cx, |app, _| {
        assert!(app.pii.mapping.expanded.contains(&full))
    });
    // ↓ selects the mention: inline controls, no popup.
    key(cx, "down");
    app.read_with(cx, |app, _| {
        assert!(matches!(
            app.pii.mapping.cursor,
            Some(PanelCursor::Mention(_))
        ));
        assert!(app.pii.popup.is_none());
    });
    assert!(cx.debug_bounds("inline-controls").is_some());
    // Enter applies this mention; triage moves to the next undecided one,
    // opening the initials group.
    key(cx, "enter");
    assert!(text(cx).starts_with("PERSON_1 ·"));
    app.read_with(cx, |app, _| {
        assert!(app.pii.mapping.expanded.contains(&initials));
        assert_eq!(app.selected_entity(), Some(initials));
    });
    // ⌘⌫ keeps both initials (same text) in one step; triage reaches e-mail.
    key(cx, "cmd-backspace");
    app.read_with(cx, |app, cx| {
        assert_eq!(app.pii.review.identity_count(initials), 0);
        // The triage-opened initials group closed once decided.
        assert!(!app.pii.mapping.expanded.contains(&initials));
        assert!(app.selected_entity().is_some());
        assert_eq!(app.editor.read(cx).text(), text_of(source));
    });
    // ⌘Z in the panel undoes the keep.
    key(cx, "cmd-z");
    app.read_with(cx, |app, _| {
        assert_eq!(app.pii.review.identity_count(initials), 2)
    });
    // ↑ to the top, then ↑ again returns to search; Esc there closes.
    key(cx, "cmd-up");
    key(cx, "up");
    app.update_in(cx, |app, window, cx| {
        assert!(
            app.pii
                .mapping
                .search
                .read(cx)
                .focus_handle(cx)
                .is_focused(window)
        );
    });
    key(cx, "escape");
    app.read_with(cx, |app, _| assert!(!app.pii.mapping.open));
}

/// The source after the first full-name mention became `PERSON_1`.
fn text_of(source: &str) -> String {
    source.replacen("Павлова Марина Сергеевна", "PERSON_1", 1)
}

#[gpui::test]
fn keyboard_move_wheel_and_middle_click(cx: &mut gpui::TestAppContext) {
    let (app, cx) = crate::document_view_tests::boot(cx);
    let source = "Павлова Марина Сергеевна · Павлова М.С. · Павлова М.С. · marina@example.invalid";
    let (full, initials) = app.update(cx, |app, cx| {
        let (full, initials) = seed(app, source, cx);
        let review = &app.pii.review;
        (
            review.variant_identity(full).unwrap(),
            review.variant_identity(initials).unwrap(),
        )
    });
    cx.run_until_parked();
    let draw = |cx: &mut gpui::VisualTestContext| {
        cx.run_until_parked();
        cx.update(|window, cx| window.draw(cx).clear(cx));
    };
    // ⌥↓ from the initials header targets the next group; releasing ⌥
    // merges, Esc while held cancels.
    app.update_in(cx, |app, window, cx| {
        window.focus(&app.pii.mapping.focus, cx);
        app.pii.mapping.cursor = Some(PanelCursor::Header(initials));
        cx.notify();
    });
    draw(cx);
    cx.simulate_keystrokes("alt-up");
    draw(cx);
    app.read_with(cx, |app, _| {
        assert!(app.pii.mapping.key_moving);
        assert_eq!(app.pii.mapping.drop_target, Some(DropTarget::Entity(full)));
    });
    cx.simulate_keystrokes("alt-escape");
    draw(cx);
    app.read_with(cx, |app, _| {
        assert!(!app.pii.mapping.key_moving);
        assert_eq!(app.pii.review.identity_count(initials), 2);
    });
    cx.simulate_keystrokes("alt-up");
    cx.simulate_modifiers_change(gpui::Modifiers::none());
    draw(cx);
    app.read_with(cx, |app, _| {
        assert_eq!(app.pii.review.identity_count(full), 3);
        assert_eq!(app.pii.review.identity_count(initials), 0);
    });
    cx.dispatch_action(mdoc_editor::Undo);
    draw(cx);
    app.read_with(cx, |app, _| {
        assert_eq!(app.pii.review.identity_count(initials), 2)
    });
    // A mouse wheel steps the keyboard row; a trackpad does not.
    let list = cx.debug_bounds("identity-panel").unwrap().center();
    app.update(cx, |app, cx| {
        app.pii.mapping.cursor = None;
        cx.notify();
    });
    draw(cx);
    let wheel = |cx: &mut gpui::VisualTestContext, delta| {
        cx.simulate_event(gpui::ScrollWheelEvent {
            position: list,
            delta,
            modifiers: Default::default(),
            touch_phase: gpui::TouchPhase::Moved,
        });
        cx.run_until_parked();
    };
    wheel(cx, gpui::ScrollDelta::Lines(gpui::point(0., -1.)));
    let first = app.read_with(cx, |app, _| app.pii.mapping.cursor);
    assert!(first.is_some());
    wheel(cx, gpui::ScrollDelta::Lines(gpui::point(0., -1.)));
    let second = app.read_with(cx, |app, _| app.pii.mapping.cursor);
    assert_ne!(first, second);
    wheel(cx, gpui::ScrollDelta::Pixels(gpui::point(px(0.), px(-30.))));
    assert_eq!(app.read_with(cx, |app, _| app.pii.mapping.cursor), second);
    // Middle click on a panel mention keeps its original; ⌘ widens it.
    app.update(cx, |app, cx| {
        app.pii.mapping.expanded.insert(initials);
        cx.notify();
    });
    draw(cx);
    let mention = app.read_with(cx, |app, _| app.identity_occurrences(initials)[0].0);
    let at = cx
        .debug_bounds(Box::leak(format!("mention-{mention}").into_boxed_str()))
        .unwrap()
        .center();
    cx.simulate_mouse_down(at, gpui::MouseButton::Middle, gpui::Modifiers::command());
    cx.simulate_mouse_up(at, gpui::MouseButton::Middle, gpui::Modifiers::command());
    draw(cx);
    app.read_with(cx, |app, _| {
        assert_eq!(app.pii.review.identity_count(initials), 0)
    });
    // The same gesture on an editor chip keeps that mention.
    let chip = app.read_with(cx, |app, cx| {
        let annotation = app.identity_occurrences(full)[0].0;
        app.editor
            .read(cx)
            .annotation_bounds(annotation)
            .unwrap()
            .center()
    });
    cx.simulate_mouse_down(chip, gpui::MouseButton::Middle, Default::default());
    cx.simulate_mouse_up(chip, gpui::MouseButton::Middle, Default::default());
    draw(cx);
    app.read_with(cx, |app, _| {
        assert_eq!(app.pii.review.identity_count(full), 0)
    });
}
