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

fn seed(app: &mut Workspace, source: &str, cx: &mut Context<Workspace>) -> (u64, u64) {
    app.editor.update(cx, |e, cx| e.set_text(source, cx));
    app.pii.review = Default::default();
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
        .unwrap()
        .id;
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
    let (app, cx) = crate::ui_tests::boot(cx);
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
        app.restore_pii(&PiiRestore, window, cx);
        assert_eq!(
            app.editor.read(cx).text(),
            "CLIENT_1 · Павлова М.С. · CLIENT_1 · EMAIL_1"
        );
    });
}

#[gpui::test]
fn homonym_split_survives_bulk_apply_rescan_undo_and_redo(cx: &mut gpui::TestAppContext) {
    let (app, cx) = crate::ui_tests::boot(cx);
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
    let (app, cx) = crate::ui_tests::boot(cx);
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
            assert!(panel.size.width <= px(340.) && panel.left() > px(0.));
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
    let (app, cx) = crate::ui_tests::boot(cx);
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
    let (app, cx) = crate::ui_tests::boot(cx);
    let source = "Павлова Марина Сергеевна · Павлова М.С. · Павлова М.С.";
    let (_, initials) = app.update(cx, |app, cx| seed(app, source, cx));
    cx.run_until_parked();
    app.update_in(cx, |app, window, cx| {
        let range = app.pii.review.variant(initials).unwrap().mentions[0].clone();
        let annotation = app.pii.review.annotation_id(initials, &range).unwrap();
        app.activate_annotation(annotation, window, cx);
        app.keep_replacement(true, cx);
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
    let (app, cx) = crate::ui_tests::boot(cx);
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
    let (app, cx) = crate::ui_tests::boot(cx);
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
    let (app, cx) = crate::ui_tests::boot(cx);
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
    let (app, cx) = crate::ui_tests::boot(cx);
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
        app.keep_replacement(false, cx);
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
    let (app, cx) = crate::ui_tests::boot(cx);
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
fn panel_click_reveals_exact_word_with_popup_and_retains_workspace(cx: &mut gpui::TestAppContext) {
    let (app, cx) = crate::ui_tests::boot(cx);
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
            let row = cx
                .debug_bounds(Box::leak(format!("identity-{identity}").into_boxed_str()))
                .unwrap();
            cx.simulate_click(row.center(), Default::default());
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
    let (app, cx) = crate::ui_tests::boot(cx);
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
    let (app, cx) = crate::ui_tests::boot(cx);
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
    let (app, cx) = crate::ui_tests::boot(cx);
    let source = "Павлова Марина Сергеевна · Павлова М.С. · marina@example.invalid";
    let (full, initials) = app.update(cx, |app, cx| seed(app, source, cx));
    cx.run_until_parked();
    app.update_in(cx, |app, window, cx| {
        app.reveal_replacement(initials, window, cx);
        app.pii
            .mapping
            .alias
            .update(cx, |input, cx| input.set_value("PERSON_1".into(), cx));
        assert!(!app.confirm_alias(false, false, cx));
        assert_eq!(app.pii.review.variant_identity(initials), Some(initials));
        assert!(app.pii.mapping.field_error.is_some());
        app.link_to_entity(full, cx);
        assert_eq!(app.pii.mapping.alias.read(cx).value(), "PERSON_1");
        assert!(app.pii.popup.is_some());
        app.pii
            .mapping
            .alias
            .update(cx, |input, cx| input.set_value("PERSON_99".into(), cx));
        app.confirm_alias(true, false, cx);
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
    let (app, cx) = crate::ui_tests::boot(cx);
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
    let (app, cx) = crate::ui_tests::boot(cx);
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
