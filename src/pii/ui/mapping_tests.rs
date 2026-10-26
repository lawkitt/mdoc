use super::{popup::Step, *};
use crate::{
    document_view::DocumentView,
    pii::{
        self, Category, Review,
        ui::{APPLIED_ID, KEPT_ID, PiiApplyAll},
    },
};
use gpui::{Context, Entity, Focusable, px};
use std::{ops::Range, sync::Arc};

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
    app.review_changed(cx);
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
        app.copy_markdown(&crate::document_view::CopyMarkdown, window, cx);
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
    for theme in [crate::style::Theme::Dark, crate::style::Theme::Light] {
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
                // Inspector (ADR 0048, C): the Review tab.
                app.inspector_share = false;
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
            for control in ["replacement-filter", "review-rescan"] {
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
            let apply = cx.debug_bounds("primary-apply").unwrap().center();
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
    for theme in [crate::style::Theme::Dark, crate::style::Theme::Light] {
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
        // Ownership lives in the category menu (ADR 0055).
        let category = cx.debug_bounds("direct-category").unwrap().center();
        cx.simulate_click(category, Default::default());
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
        app.copy_markdown(&crate::document_view::CopyMarkdown, window, cx);
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
    for theme in [crate::style::Theme::Dark, crate::style::Theme::Light] {
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
            // A mention row opens the popup on that word, beside the panel
            // (ADR 0048).
            app.update_in(cx, |app, _, _| {
                assert!(app.pii.popup.is_some());
                assert_eq!(app.active_annotation(), Some(first));
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
    for theme in [crate::style::Theme::Dark, crate::style::Theme::Light] {
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
            let apply = cx.debug_bounds("primary-apply").unwrap();
            cx.simulate_click(apply.center(), Default::default());
            cx.run_until_parked();
            app.update_in(cx, |app, window, cx| {
                assert_eq!(app.editor.read(cx).text(), "CLIENT_7 · ORG_1");
                assert!(app.pii.popup.is_none());
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
        app.pii.mapping.set_scope(Scope::Wording);
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
    for theme in [crate::style::Theme::Light, crate::style::Theme::Dark] {
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
fn chrome_history_fallback_does_not_take_alias_field_keys(cx: &mut gpui::TestAppContext) {
    let (app, cx) = crate::document_view_tests::boot(cx);
    cx.update(|_, cx| crate::bind_app_keys(cx));
    let source = "Павлова Марина Сергеевна · Павлова М.С.";
    let (_, initials) = app.update(cx, |app, cx| seed(app, source, cx));
    cx.run_until_parked();
    app.update_in(cx, |app, window, cx| {
        app.apply_all_pii(&PiiApplyAll, window, cx);
        app.reveal_replacement(initials, window, cx);
        app.pii
            .mapping
            .alias
            .update(cx, |input, cx| input.set_value("DRAFT_ALIAS".into(), cx));
        window.focus(&app.pii.mapping.alias.read(cx).focus_handle(cx), cx);
    });
    cx.run_until_parked();
    let text = cx.update(|_, cx| app.read(cx).editor.read(cx).text().to_owned());
    for chord in if cfg!(target_os = "macos") {
        vec!["cmd-z", "cmd-shift-z"]
    } else {
        vec!["ctrl-z", "ctrl-shift-z", "ctrl-y"]
    } {
        cx.simulate_keystrokes(chord);
        cx.run_until_parked();
        cx.update(|_, cx| {
            assert_eq!(app.read(cx).editor.read(cx).text(), text);
            assert_eq!(
                app.read(cx).pii.mapping.alias.read(cx).value(),
                "DRAFT_ALIAS"
            );
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
fn redo_works_after_an_undo_closes_the_popup(cx: &mut gpui::TestAppContext) {
    let (app, cx) = crate::document_view_tests::boot(cx);
    let source = "Павлова М.С. · marina@example.invalid";
    let (_, initials) = app.update(cx, |app, cx| seed(app, source, cx));
    cx.run_until_parked();
    app.update_in(cx, |app, window, cx| {
        let range = app.pii.review.variant(initials).unwrap().mentions[0].clone();
        let annotation = app.pii.review.annotation_id(initials, &range).unwrap();
        app.activate_annotation(annotation, window, cx);
        window.focus(&app.pii.focus, cx);
    });
    cx.run_until_parked();
    cx.update(|window, cx| window.draw(cx).clear(cx));
    let applied = "ORG_1 · marina@example.invalid";
    cx.simulate_keystrokes("enter");
    cx.run_until_parked();
    app.update(cx, |app, cx| {
        assert_eq!(app.editor.read(cx).text(), applied)
    });
    let (undo, redo) = if cfg!(target_os = "macos") {
        ("cmd-z", "cmd-shift-z")
    } else {
        ("ctrl-z", "ctrl-shift-z")
    };
    cx.update(|window, cx| window.draw(cx).clear(cx));
    cx.simulate_keystrokes(undo);
    cx.run_until_parked();
    app.update(cx, |app, cx| assert_eq!(app.editor.read(cx).text(), source));
    cx.update(|window, cx| window.draw(cx).clear(cx));
    // The popup may have closed with the undo; its redo still runs (#79).
    cx.simulate_keystrokes(redo);
    cx.run_until_parked();
    app.update(cx, |app, cx| {
        assert_eq!(app.editor.read(cx).text(), applied)
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
        // The popup stays on the mention, now kept (ADR 0055).
        assert!(app.pii.popup.is_some() && app.active_is_kept());
        assert_eq!(
            app.active_replacement_range(),
            Some(0.."Павлова М.С.".len())
        );
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
fn kept_homonyms_stay_with_their_entity_before_and_after_apply(cx: &mut gpui::TestAppContext) {
    let (app, cx) = crate::document_view_tests::boot(cx);
    let source = "Павлова М.С. · Павлова М.С.";
    let (first, second) = app.update(cx, |app, cx| {
        let (_, variant) = seed(app, source, cx);
        let first = app.pii.review.variant_identity(variant).unwrap();
        let range = app.pii.review.variant(variant).unwrap().mentions[1].clone();
        app.drop_as_new_entity(
            &PanelDrag::Mention {
                identity: first,
                range: range.clone(),
                original: "Павлова М.С.".into(),
            },
            cx,
        );
        let second = app.pii.review.occurrence_identity(variant, &range).unwrap();
        app.untick(second, None, cx);
        assert!(app.kept_of(first).is_empty());
        assert_eq!(app.kept_of(second)[0].0, range);
        (first, second)
    });
    cx.dispatch_action(mdoc_editor::Undo);
    cx.run_until_parked();
    app.update(cx, |app, cx| {
        assert!(app.kept_entries().is_empty());
        app.apply_aliases(cx);
        app.untick(second, None, cx);
        assert!(app.kept_of(first).is_empty());
        assert_eq!(app.kept_of(second).len(), 1);
        assert_eq!(app.editor.read(cx).text(), "ORG_1 · Павлова М.С.");
        app.untick(first, None, cx);
        assert_eq!(app.kept_of(first).len(), 1);
        assert_eq!(app.kept_of(second).len(), 1);
        assert_eq!(app.editor.read(cx).text(), source);
    });
    cx.dispatch_action(mdoc_editor::Undo);
    cx.run_until_parked();
    app.read_with(cx, |app, _| {
        assert!(app.kept_of(first).is_empty());
        assert_eq!(app.kept_of(second).len(), 1);
    });
    cx.dispatch_action(mdoc_editor::Redo);
    cx.run_until_parked();
    app.read_with(cx, |app, _| {
        assert_eq!(app.kept_of(first).len(), 1);
        assert_eq!(app.kept_of(second).len(), 1);
    });
    app.update(cx, |app, cx| {
        let range = app.kept_of(second)[0].0.clone();
        app.retick_mention(second, range, cx);
        assert_eq!(app.kept_of(first).len(), 1);
        assert!(app.kept_of(second).is_empty());
        let plans = app
            .pii
            .review
            .pending_plans(source, &Default::default())
            .unwrap();
        assert_eq!(plans.len(), 1);
        assert_eq!(plans[0].identity, second);
        app.apply_aliases(cx);
        assert_eq!(app.editor.read(cx).text(), "Павлова М.С. · ORG_2");
        app.retick(first, cx);
        assert!(app.kept_entries().is_empty());
        let plans = app
            .pii
            .review
            .pending_plans(app.editor.read(cx).text(), &Default::default())
            .unwrap();
        assert_eq!(plans.len(), 1);
        assert_eq!(plans[0].identity, first);
    });
    cx.dispatch_action(mdoc_editor::Undo);
    cx.run_until_parked();
    app.read_with(cx, |app, cx| {
        assert_eq!(app.kept_of(first).len(), 1);
        assert!(app.kept_of(second).is_empty());
        assert_eq!(app.editor.read(cx).text(), "Павлова М.С. · ORG_2");
    });
}

#[gpui::test]
fn unticking_an_applied_mention_restores_only_its_original(cx: &mut gpui::TestAppContext) {
    let (app, cx) = crate::document_view_tests::boot(cx);
    for theme in [crate::style::Theme::Dark, crate::style::Theme::Light] {
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
        // The mention row's checkbox keeps this original (ADR 0048).
        let (annotation, identity) = app.update(cx, |app, cx| {
            let annotation = app.active_annotation().unwrap();
            app.panel_select_mention(annotation, cx);
            (annotation, app.selected_entity().unwrap())
        });
        app.update(cx, |app, cx| {
            app.pii.mapping.expanded.insert(identity);
            cx.notify();
        });
        cx.run_until_parked();
        cx.update(|window, cx| window.draw(cx).clear(cx));
        let tick = cx
            .debug_bounds(Box::leak(
                format!("mention-tick-{annotation}").into_boxed_str(),
            ))
            .expect("an applied mention row has a checkbox");
        let panel = cx.debug_bounds("identity-panel").unwrap();
        assert!(tick.left() >= panel.left() && tick.right() <= panel.right());
        cx.simulate_click(tick.center(), Default::default());
        cx.run_until_parked();
        app.update(cx, |app, cx| {
            assert_eq!(app.editor.read(cx).text(), "Павлова М.С. · ORG_1");
            assert_eq!(app.pii.review.applied().len(), 1);
            assert_eq!(app.entity_tick(identity), super::checklist::Tick::Mixed);
        });
    }
}

#[gpui::test]
fn entity_rows_jump_chevrons_expand_and_selection_survives_collapse(cx: &mut gpui::TestAppContext) {
    let (app, cx) = crate::document_view_tests::boot(cx);
    let source = "Павлова Марина Сергеевна · Павлова М.С. · Павлова М.С. · marina@example.invalid";
    let identity = app.update(cx, |app, cx| {
        let (_, initials) = seed(app, source, cx);
        app.pii.review.variant_identity(initials).unwrap()
    });
    cx.run_until_parked();
    let draw = |cx: &mut gpui::VisualTestContext| cx.update(|window, cx| window.draw(cx).clear(cx));
    let click = |cx: &mut gpui::VisualTestContext, selector: String| {
        draw(cx);
        let bounds = cx
            .debug_bounds(Box::leak(selector.into_boxed_str()))
            .unwrap();
        cx.simulate_click(bounds.center(), Default::default());
        cx.run_until_parked();
        draw(cx);
    };
    // An entity row click opens its group and the popup on its first
    // mention (ADR 0048).
    click(cx, format!("identity-{identity}"));
    let first = app.read_with(cx, |app, _| app.identity_occurrences(identity)[0].0);
    app.read_with(cx, |app, _| {
        assert!(app.pii.mapping.expanded.contains(&identity));
        assert_eq!(app.active_annotation(), Some(first));
        assert!(app.pii.popup.is_some());
    });
    // A second click on the open group folds it and closes the popup.
    click(cx, format!("identity-{identity}"));
    app.read_with(cx, |app, _| {
        assert!(!app.pii.mapping.expanded.contains(&identity));
        assert!(app.pii.popup.is_none());
    });
    // The chevron shows the mentions; a mention row opens the popup there.
    click(cx, format!("entity-expand-{identity}"));
    app.read_with(cx, |app, _| {
        assert!(app.pii.mapping.expanded.contains(&identity))
    });
    click(cx, format!("mention-{first}"));
    app.read_with(cx, |app, _| {
        assert_eq!(app.selected_entity(), Some(identity));
        assert_eq!(app.active_annotation(), Some(first));
        assert!(app.pii.popup.is_some());
    });
    // Collapsing leaves the selection alone.
    click(cx, format!("entity-expand-{identity}"));
    app.read_with(cx, |app, _| {
        assert!(!app.pii.mapping.expanded.contains(&identity));
        assert_eq!(app.selected_entity(), Some(identity));
    });
}

#[gpui::test]
fn entity_and_mention_checkboxes_keep_originals_and_tick_again(cx: &mut gpui::TestAppContext) {
    let (app, cx) = crate::document_view_tests::boot(cx);
    let source = "Павлова Марина Сергеевна · Павлова М.С. · Павлова М.С. · marina@example.invalid";
    let identity = app.update(cx, |app, cx| {
        let (_, initials) = seed(app, source, cx);
        app.pii.review.variant_identity(initials).unwrap()
    });
    cx.run_until_parked();
    let draw = |cx: &mut gpui::VisualTestContext| cx.update(|window, cx| window.draw(cx).clear(cx));
    let click = |cx: &mut gpui::VisualTestContext, selector: String| {
        draw(cx);
        let bounds = cx
            .debug_bounds(Box::leak(selector.into_boxed_str()))
            .unwrap();
        cx.simulate_click(bounds.center(), Default::default());
        cx.run_until_parked();
        draw(cx);
    };
    let live = |cx: &mut gpui::VisualTestContext| {
        app.read_with(cx, |app, _| app.identity_occurrences(identity).len())
    };
    let before = live(cx);
    assert!(before > 1);
    // Unticking keeps every original; the row stays, unticked; text is unchanged.
    click(cx, format!("entity-tick-{identity}"));
    assert_eq!(live(cx), 0);
    app.read_with(cx, |app, cx| {
        assert_eq!(app.entity_tick(identity), super::checklist::Tick::Off);
        assert_eq!(app.editor.read(cx).text(), source);
    });
    assert!(
        cx.debug_bounds(Box::leak(format!("identity-{identity}").into_boxed_str()))
            .is_some()
    );
    // Undo brings the tick back; ticking again does the same.
    app.update_in(cx, |app, window, cx| {
        window.focus(&app.editor.read(cx).focus_handle(cx), cx);
    });
    cx.dispatch_action(mdoc_editor::Undo);
    cx.run_until_parked();
    assert_eq!(live(cx), before);
    click(cx, format!("entity-tick-{identity}"));
    click(cx, format!("entity-tick-{identity}"));
    assert_eq!(live(cx), before);
    app.read_with(cx, |app, _| {
        assert_eq!(app.entity_tick(identity), super::checklist::Tick::On)
    });
    // A mention's checkbox keeps only that mention: the entity is mixed.
    click(cx, format!("entity-expand-{identity}"));
    let first = app.read_with(cx, |app, _| app.identity_occurrences(identity)[0].0);
    click(cx, format!("mention-tick-{first}"));
    assert_eq!(live(cx), before - 1);
    app.read_with(cx, |app, _| {
        assert_eq!(app.entity_tick(identity), super::checklist::Tick::Mixed)
    });
    // Ticking a mixed entity proposes the kept mention again.
    click(cx, format!("entity-tick-{identity}"));
    assert_eq!(live(cx), before);
    // Unticked mentions stay listed under their entity, unticked; ticking
    // one brings back that mention only, though its wording is shared.
    let mentions = app.read_with(cx, |app, _| app.identity_occurrences(identity));
    for (annotation, _) in &mentions[..2] {
        click(cx, format!("mention-tick-{annotation}"));
    }
    assert_eq!(live(cx), before - 2);
    let kept = mentions[1].1.start;
    let first_kept = mentions[0].1.start;
    draw(cx);
    assert!(
        cx.debug_bounds(Box::leak(format!("kept-{first_kept}").into_boxed_str()))
            .is_some()
    );
    assert!(
        cx.debug_bounds(Box::leak(format!("kept-{kept}").into_boxed_str()))
            .is_some()
    );
    click(cx, format!("kept-tick-{kept}"));
    assert_eq!(live(cx), before - 1);
    draw(cx);
    assert!(
        cx.debug_bounds(Box::leak(format!("kept-{first_kept}").into_boxed_str()))
            .is_some()
    );
    assert!(
        cx.debug_bounds(Box::leak(format!("kept-{kept}").into_boxed_str()))
            .is_none()
    );
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
fn escape_closes_a_panel_menu_before_the_popup(cx: &mut gpui::TestAppContext) {
    let (app, cx) = crate::document_view_tests::boot(cx);
    // The app binds the generic UiMenu Escape last, so it wins in the panel.
    cx.update(|_, cx| crate::ui::bind_keys(cx));
    let source = "Павлова Марина Сергеевна · Павлова М.С. · marina@example.invalid";
    let (full, initials) = app.update(cx, |app, cx| {
        let (full, initials) = seed(app, source, cx);
        let review = &app.pii.review;
        (full, review.variant_identity(initials).unwrap())
    });
    cx.run_until_parked();
    let right_click = |cx: &mut gpui::VisualTestContext| {
        cx.update(|window, cx| window.draw(cx).clear(cx));
        let row = cx
            .debug_bounds(Box::leak(format!("identity-{initials}").into_boxed_str()))
            .unwrap()
            .center();
        cx.simulate_mouse_down(row, gpui::MouseButton::Right, Default::default());
        cx.simulate_mouse_up(row, gpui::MouseButton::Right, Default::default());
        cx.run_until_parked();
    };
    let escape = |cx: &mut gpui::VisualTestContext| {
        cx.simulate_keystrokes("escape");
        cx.run_until_parked();
    };
    // Without a popup, Escape closes the menu and keeps the panel (#92).
    right_click(cx);
    app.read_with(cx, |app, _| assert!(app.pii.mapping.panel_menu.is_some()));
    escape(cx);
    app.read_with(cx, |app, _| {
        assert!(app.pii.mapping.panel_menu.is_none() && app.pii.mapping.open);
    });
    // Focus tabbed into the menu returns to the list, where keys still work.
    right_click(cx);
    cx.simulate_keystrokes("tab");
    cx.run_until_parked();
    escape(cx);
    app.update_in(cx, |app, window, _| {
        assert!(app.pii.mapping.panel_menu.is_none() && app.pii.mapping.open);
        assert!(app.pii.mapping.focus.is_focused(window));
        app.pii.mapping.cursor = None;
    });
    cx.simulate_keystrokes("down");
    cx.run_until_parked();
    app.read_with(cx, |app, _| assert!(app.pii.mapping.cursor.is_some()));
    // Over the popup and its open picker, the menu closes first, then the
    // picker, then the popup.
    app.update_in(cx, |app, window, cx| {
        let range = app.pii.review.variant(full).unwrap().mentions[0].clone();
        let annotation = app.pii.review.annotation_id(full, &range).unwrap();
        app.activate_annotation(annotation, window, cx);
        app.pii.mapping.pickers.category = true;
    });
    cx.run_until_parked();
    right_click(cx);
    app.read_with(cx, |app, _| {
        assert!(app.pii.mapping.panel_menu.is_some() && app.pii.popup.is_some());
    });
    escape(cx);
    app.read_with(cx, |app, _| {
        assert!(app.pii.mapping.panel_menu.is_none() && app.pii.popup.is_some());
        assert!(app.pii.mapping.pickers.category);
    });
    escape(cx);
    app.read_with(cx, |app, _| {
        assert!(!app.pii.mapping.pickers.category && app.pii.popup.is_some());
    });
    escape(cx);
    app.read_with(cx, |app, _| {
        assert!(app.pii.popup.is_none() && app.pii.mapping.open);
    });
}

#[gpui::test]
fn panel_groups_expand_independently_filter_and_edit_from_rows(cx: &mut gpui::TestAppContext) {
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
    // Groups open independently from their chevrons (ADR 0048).
    click(cx, format!("entity-expand-{full}"));
    click(cx, format!("entity-expand-{initials}"));
    app.read_with(cx, |app, _| {
        assert!(app.pii.mapping.expanded.contains(&full));
        assert!(app.pii.mapping.expanded.contains(&initials));
        assert_eq!(app.selected_entity(), None);
    });
    let email = app.read_with(cx, |app, _| {
        app.pii
            .review
            .active_identities()
            .into_iter()
            .find(|id| *id != full && *id != initials)
            .unwrap()
    });
    click(cx, format!("entity-expand-{email}"));
    app.read_with(cx, |app, cx| {
        assert_eq!(app.pii.mapping.expanded.len(), 3);
        let entries = app.replacement_entries(cx);
        // A category label above each group (ADR 0046): three categories,
        // three entities, four mentions.
        assert_eq!(entries.len(), 3 + 3 + 4);
        let labels: Vec<_> = entries
            .iter()
            .filter_map(|(row, item)| match item {
                super::panel::PanelItem::Category { entities, mentions } => {
                    Some((row.category, *entities, *mentions))
                }
                _ => None,
            })
            .collect();
        assert_eq!(
            labels,
            [
                (Category::Person, 1, 1),
                (Category::Organization, 1, 2),
                (Category::Email, 1, 1)
            ]
        );
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
        assert_eq!(entries.len(), 3);
        assert!(entries[2].1.mention().unwrap() & APPLIED_ID != 0);
    });
    click(cx, "replacement-filter".into());
    click(cx, "replacement-filter-All".into());
    // Header alias: click, type, Enter renames the whole entity.
    click(cx, format!("header-alias-{initials}"));
    cx.simulate_keystrokes("secondary-a");
    cx.simulate_input("CLIENT");
    cx.simulate_keystrokes("enter");
    cx.run_until_parked();
    app.read_with(cx, |app, _| {
        assert_eq!(app.pii.review.identity(initials).unwrap().alias, "CLIENT");
        assert_eq!(app.pii.mapping.header_edit, None);
    });
    // Right-clicking an entity row offers its category (ADR 0048).
    cx.update(|window, cx| window.draw(cx).clear(cx));
    let row = cx
        .debug_bounds(Box::leak(format!("identity-{initials}").into_boxed_str()))
        .unwrap()
        .center();
    cx.simulate_mouse_down(row, gpui::MouseButton::Right, Default::default());
    cx.simulate_mouse_up(row, gpui::MouseButton::Right, Default::default());
    cx.run_until_parked();
    click(cx, "panel-category-PERSON".into());
    app.read_with(cx, |app, _| {
        assert_eq!(
            app.pii.review.identity(initials).unwrap().category,
            Category::Person
        );
    });
}

#[gpui::test]
fn panel_keyboard_ticks_jumps_expands_and_applies(cx: &mut gpui::TestAppContext) {
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
    // ↓ from search enters the list on the first entity (ADR 0048).
    app.update_in(cx, |app, window, cx| {
        window.focus(&app.pii.mapping.search.read(cx).focus_handle(cx), cx);
    });
    key(cx, "down");
    app.read_with(cx, |app, _| {
        assert_eq!(app.pii.mapping.cursor, Some(PanelCursor::Header(full)));
    });
    // Space unticks (keeps the original) and ticks again; the cursor stays.
    key(cx, "space");
    app.read_with(cx, |app, _| {
        assert_eq!(app.pii.review.identity_count(full), 0);
        assert_eq!(app.pii.mapping.cursor, Some(PanelCursor::Header(full)));
    });
    assert_eq!(text(cx), source);
    key(cx, "space");
    app.read_with(cx, |app, _| {
        assert_eq!(app.pii.review.identity_count(full), 1)
    });
    // ⌫ only unticks; ⌘Z in the panel undoes it.
    key(cx, "backspace");
    app.read_with(cx, |app, _| {
        assert_eq!(app.pii.review.identity_count(full), 0)
    });
    key(cx, "secondary-z");
    app.read_with(cx, |app, _| {
        assert_eq!(app.pii.review.identity_count(full), 1)
    });
    // Enter jumps to the mention; ← hides the mentions, → shows them.
    key(cx, "enter");
    app.read_with(cx, |app, _| {
        assert_eq!(app.selected_entity(), Some(full));
        assert!(app.pii.mapping.expanded.contains(&full));
        assert!(app.pii.popup.is_none());
    });
    key(cx, "left");
    app.read_with(cx, |app, _| {
        assert!(!app.pii.mapping.expanded.contains(&full));
        assert_eq!(app.pii.mapping.cursor, Some(PanelCursor::Header(full)));
    });
    key(cx, "right");
    app.read_with(cx, |app, _| {
        assert!(app.pii.mapping.expanded.contains(&full))
    });
    // ⌘↩ applies everything ticked as one step.
    key(cx, "secondary-enter");
    assert!(text(cx).starts_with("PERSON_1 ·"));
    app.read_with(cx, |app, _| {
        assert_eq!(app.pii.review.remaining(), 0);
        assert_eq!(app.pii.review.identity_count(initials), 2);
        // Inspector (ADR 0048, C): applying everything turns to Share.
        assert!(app.inspector_share);
    });
    click_tab(cx, "inspector-review");
    // ↑ to the top, then ↑ again returns to search; Esc there closes.
    key(cx, "secondary-up");
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
    cx.simulate_mouse_down(
        at,
        gpui::MouseButton::Middle,
        gpui::Modifiers::secondary_key(),
    );
    cx.simulate_mouse_up(
        at,
        gpui::MouseButton::Middle,
        gpui::Modifiers::secondary_key(),
    );
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

#[gpui::test]
fn applying_under_the_proposed_filter_keeps_the_cursor_on_a_shown_row(
    cx: &mut gpui::TestAppContext,
) {
    let (app, cx) = crate::document_view_tests::boot(cx);
    let source = "Павлова М.С. · Павлова М.С.";
    app.update_in(cx, |app, window, cx| {
        let (_, initials) = seed(app, source, cx);
        let identity = app.pii.review.variant_identity(initials).unwrap();
        app.pii.mapping.filter = PanelFilter::Proposed;
        app.pii.mapping.expanded.insert(identity);
        let (first, _) = app.identity_occurrences(identity)[0].clone();
        app.panel_select_mention(first, cx);
        app.panel_decide(Scope::Mention, false, window, cx);
        // The applied mention is filtered out; its header takes the cursor.
        assert_eq!(app.pii.review.applied().len(), 1);
        assert_eq!(app.pii.mapping.cursor, Some(PanelCursor::Header(identity)));
        // Applying the last one hides the whole group: no hidden cursor.
        let (second, _) = app.identity_occurrences(identity)[1].clone();
        assert_eq!(second & APPLIED_ID, 0);
        app.panel_select_mention(second, cx);
        app.panel_decide(Scope::Mention, false, window, cx);
        assert_eq!(app.pii.review.applied().len(), 2);
        assert_eq!(app.pii.mapping.cursor, None);
    });
}

/// Click an Inspector tab (ADR 0048 variant C).
fn click_tab(cx: &mut gpui::VisualTestContext, id: &'static str) {
    cx.update(|window, cx| window.draw(cx).clear(cx));
    let at = cx.debug_bounds(id).unwrap().center();
    cx.simulate_click(at, Default::default());
    cx.run_until_parked();
    cx.update(|window, cx| window.draw(cx).clear(cx));
}

#[gpui::test]
fn panel_projection_reuses_rows_and_invalidates_on_keep_history_filter_and_query(
    cx: &mut gpui::TestAppContext,
) {
    use super::panel::PanelItem;
    let (app, cx) = crate::document_view_tests::boot(cx);
    let source = "Павлова Марина Сергеевна · Павлова М.С. · Павлова М.С. · marina@example.invalid";
    let identity = app.update(cx, |app, cx| {
        let (full, _) = seed(app, source, cx);
        let ids = app.pii.review.active_identities();
        app.pii.mapping.expanded.extend(ids);
        app.pii.review.variant_identity(full).unwrap()
    });
    cx.run_until_parked();
    let initial = app.read_with(cx, |app, cx| app.replacement_entries(cx));
    app.update(cx, |app, cx| {
        assert!(Arc::ptr_eq(&initial, &app.replacement_entries(cx)));
        assert_eq!(app.replacement_entity_count(cx), 3);
        assert_eq!(app.kept_mention_count(cx), 0);
        let alias = app.pii.review.identity(identity).unwrap().alias.clone();
        assert!(app.case_alias_is_present(&alias, cx));
        app.sync_pii_theme(cx);
        assert!(
            Arc::ptr_eq(&initial, &app.replacement_entries(cx)),
            "presentation reuses data"
        );
        app.untick(identity, None, cx);
        let kept = app.replacement_entries(cx);
        assert!(!Arc::ptr_eq(&initial, &kept));
        assert_eq!(app.kept_mention_count(cx), 1);
        assert!(!app.case_alias_is_present(&alias, cx));
        assert!(
            kept.iter()
                .any(|(row, item)| row.id == identity && matches!(item, PanelItem::Kept(_)))
        );
    });
    app.update_in(cx, |app, window, cx| {
        app.editor.update(cx, |e, cx| e.undo_step(window, cx))
    });
    cx.run_until_parked();
    app.update(cx, |app, cx| {
        let undone = app.replacement_entries(cx);
        assert_eq!(app.kept_mention_count(cx), 0);
        assert_eq!(undone.len(), initial.len());
        app.pii.mapping.expanded.clear();
        let collapsed = app.replacement_entries(cx);
        assert!(!Arc::ptr_eq(&undone, &collapsed));
        app.pii
            .mapping
            .search
            .update(cx, |input, cx| input.set_value("EMAIL".into(), cx));
        let searched = app.replacement_entries(cx);
        assert_eq!(app.replacement_entity_count(cx), 1);
        assert!(
            searched
                .iter()
                .all(|(row, _)| row.category == Category::Email)
        );
        app.pii.mapping.filter = PanelFilter::Applied;
        assert!(app.replacement_entries(cx).is_empty());
    });
}

#[gpui::test]
#[ignore = "expanded 2 MiB / 20,000 mention panel projection; run serially"]
fn expanded_panel_projection_performance(cx: &mut gpui::TestAppContext) {
    use std::time::Instant;
    let (app, cx) = crate::document_view_tests::boot(cx);
    let names: Vec<_> = (0..64).map(|i| format!("Client{i:04}")).collect();
    let mut source: String = (0..20_000)
        .map(|i| format!("{} agrees to the terms.\n", names[i % names.len()]))
        .collect();
    source.push_str(&" ".repeat(2_097_152 - source.len()));
    app.update(cx, |app, cx| {
        app.editor
            .update(cx, |e, cx| e.set_text(source.clone(), cx));
        let detections = names
            .iter()
            .map(|name| {
                let at = source.find(name).unwrap();
                pii::Detection {
                    range: at..at + name.len(),
                    category: Category::Person,
                    score: 0.9,
                    recognizer: pii::Recognizer::Model,
                }
            })
            .collect();
        app.pii.review.ingest(&source, detections).unwrap();
        app.pii
            .mapping
            .expanded
            .extend(app.pii.review.active_identities());
        app.review_changed(cx);
        let start = Instant::now();
        let entries = app.replacement_entries(cx);
        let cold = start.elapsed().as_secs_f64() * 1000.;
        assert_eq!(entries.len(), 20_000 + 64 + 1);
        let mut warm = Vec::new();
        for _ in 0..100 {
            let start = Instant::now();
            assert!(Arc::ptr_eq(&entries, &app.replacement_entries(cx)));
            warm.push(start.elapsed().as_secs_f64() * 1000.);
        }
        warm.sort_by(f64::total_cmp);
        eprintln!(
            "PANEL_PERF bytes={} mentions=20000 entities=64 cold_ms={cold:.3} warm_median_ms={:.6}",
            source.len(),
            warm[50]
        );
    });
}

/// Propose every "Anna" in `source` as one person.
fn seed_anna(app: &mut DocumentView, source: &str, cx: &mut Context<DocumentView>) -> Vec<u64> {
    app.editor.update(cx, |e, cx| e.set_text(source, cx));
    app.pii.review = Default::default();
    app.pii.reviewing = true;
    let detections = source
        .match_indices("Anna")
        .map(|(at, text)| pii::Detection {
            range: at..at + text.len(),
            category: Category::Person,
            score: 0.9,
            recognizer: pii::Recognizer::Model,
        })
        .collect();
    app.pii.review.ingest(source, detections).unwrap();
    app.pii.mapping.begin_review();
    app.review_changed(cx);
    let mut ids: Vec<_> = app
        .pii
        .review
        .candidates()
        .iter()
        .map(|c| (c.range.start, c.id))
        .collect();
    ids.sort();
    ids.into_iter().map(|(_, id)| id).collect()
}

fn draw_frames(cx: &mut gpui::VisualTestContext) {
    for _ in 0..3 {
        cx.update(|window, cx| {
            window.refresh();
            window.draw(cx).clear(cx);
        });
    }
}

fn assert_popup_in_view(app: &Entity<DocumentView>, cx: &mut gpui::VisualTestContext) {
    let popup = cx.debug_bounds("pseudonym-popup").unwrap();
    app.read_with(cx, |app, cx| {
        let view = app.scroll.bounds();
        let word = app
            .editor
            .read(cx)
            .annotation_bounds(app.active_annotation().unwrap())
            .unwrap();
        assert!(
            word.top() >= view.top() && word.bottom() <= view.bottom(),
            "word {word:?}, view {view:?}"
        );
        assert!(
            popup.top() >= view.top() && popup.bottom() <= view.bottom(),
            "popup {popup:?}, view {view:?}"
        );
        assert!(
            popup.left() >= view.left() && popup.right() <= view.right(),
            "popup {popup:?}, view {view:?}"
        );
    });
}

#[gpui::test]
fn popup_visible_mentions_do_not_scroll_when_stepping_either_way(cx: &mut gpui::TestAppContext) {
    let (app, cx) = crate::document_view_tests::boot(cx);
    cx.simulate_resize(gpui::size(px(1200.), px(800.)));
    let filler = "An ordinary contract paragraph.\n\n".repeat(30);
    let source = format!("{filler}Here Anna signs.\n\nAnna agrees.\n\n{filler}");
    let ids = app.update(cx, |app, cx| seed_anna(app, &source, cx));
    cx.run_until_parked();
    draw_frames(cx);
    // Start partway through the document, with both mentions already visible.
    app.update(cx, |app, cx| {
        let start = app.pii.review.candidate(ids[0]).unwrap().range.start;
        let top = app.editor.read(cx).offset_screen_top(start).unwrap();
        let mut offset = app.scroll.offset();
        offset.y -= top - app.scroll.bounds().top();
        app.scroll.set_offset(offset);
    });
    draw_frames(cx);
    app.update_in(cx, |app, window, cx| {
        app.activate_annotation(ids[0], window, cx)
    });
    draw_frames(cx);
    let before = app.read_with(cx, |app, _| app.scroll.offset());
    assert_popup_in_view(&app, cx);
    for (key, id) in [("right", ids[1]), ("left", ids[0])] {
        press(key, cx);
        draw_frames(cx);
        app.read_with(cx, |app, _| {
            assert_eq!(app.active_annotation(), Some(id));
            assert_eq!(
                app.scroll.offset(),
                before,
                "{key} must leave visible mentions in place"
            );
        });
        assert_popup_in_view(&app, cx);
    }
}

#[gpui::test]
fn popup_enter_to_visible_proposal_does_not_scroll(cx: &mut gpui::TestAppContext) {
    let (app, cx) = crate::document_view_tests::boot(cx);
    cx.simulate_resize(gpui::size(px(1200.), px(800.)));
    let source = format!(
        "Павлова М.С. · Павлова М.С.\n\nmarina@example.invalid\n\n{}",
        "An ordinary contract paragraph.\n\n".repeat(30)
    );
    let (_, initials) = app.update(cx, |app, cx| seed(app, &source, cx));
    cx.run_until_parked();
    open_popup_on(&app, initials, 0, cx);
    draw_frames(cx);
    let before = app.read_with(cx, |app, _| app.scroll.offset());
    press("enter", cx);
    draw_frames(cx);
    app.read_with(cx, |app, cx| {
        assert_eq!(app.pii.review.applied().len(), 2);
        let range = app.active_replacement_range().unwrap();
        assert_eq!(&app.editor.read(cx).text()[range], "marina@example.invalid");
        assert_eq!(
            app.scroll.offset(),
            before,
            "Enter must leave the visible next proposal in place"
        );
    });
    assert_popup_in_view(&app, cx);
}

#[gpui::test]
fn popup_offscreen_mention_scrolls_only_to_the_nearest_edge(cx: &mut gpui::TestAppContext) {
    let (app, cx) = crate::document_view_tests::boot(cx);
    cx.simulate_resize(gpui::size(px(1200.), px(800.)));
    let filler = "An ordinary contract paragraph.\n\n".repeat(30);
    let source = format!("Here Anna signs.\n\n{filler}Anna agrees.\n\n{filler}");
    let ids = app.update(cx, |app, cx| seed_anna(app, &source, cx));
    cx.run_until_parked();
    draw_frames(cx);
    app.update_in(cx, |app, window, cx| {
        app.activate_annotation(ids[0], window, cx)
    });
    draw_frames(cx);
    app.read_with(cx, |app, cx| {
        let range = app.pii.review.candidate(ids[1]).unwrap().range.clone();
        assert!(
            app.editor.read(cx).offset_screen_top(range.start).unwrap()
                > app.scroll.bounds().bottom()
        );
    });
    press("right", cx);
    draw_frames(cx);
    assert_popup_in_view(&app, cx);
    app.read_with(cx, |app, cx| {
        assert_eq!(app.active_annotation(), Some(ids[1]));
        let word = app.editor.read(cx).annotation_bounds(ids[1]).unwrap();
        let expected_bottom = app.scroll.bounds().bottom();
        // Off-screen geometry uses the containing row; painted text is inset.
        assert!(word.bottom() <= expected_bottom
                && expected_bottom - word.bottom() < app.editor.read(cx).base_line_height(),
            "a target below the view should fit at its bottom: {word:?}, expected {expected_bottom:?}");
    });
    press("left", cx);
    draw_frames(cx);
    assert_popup_in_view(&app, cx);
    app.read_with(cx, |app, _| {
        assert_eq!(app.active_annotation(), Some(ids[0]))
    });
}

#[gpui::test]
fn popup_untick_stays_open_and_arrows_and_retick_follow_the_scope(cx: &mut gpui::TestAppContext) {
    let (app, cx) = crate::document_view_tests::boot(cx);
    let source = "Anna met Bob. Anna left early. Then Anna came back.";
    let starts: Vec<_> = source.match_indices("Anna").map(|(at, _)| at).collect();
    app.update_in(cx, |app, window, cx| {
        let ids = seed_anna(app, source, cx);
        app.activate_annotation(ids[0], window, cx);
        // All, the whole entity, is the default scope (ADR 0055).
        assert!(app.pii.mapping.scope() == Scope::Entity);
        app.toggle_replace(window, cx);
        assert_eq!(app.pii.review.remaining(), 0);
        assert_eq!(app.kept_entries().len(), 3);
        assert!(app.pii.popup.is_some() && app.active_is_kept());
        assert_eq!(app.active_replacement_range().unwrap().start, starts[0]);
        assert!(app.pii.focus.is_focused(window));
    });
    draw_frames(cx);
    // Kept originals keep a clickable marker in the text.
    app.update(cx, |app, cx| {
        for start in &starts {
            let marker = KEPT_ID | *start as u64;
            assert!(app.editor.read(cx).annotation_bounds(marker).is_some());
        }
    });
    // → steps through the entity's mentions, kept ones included; ⌘→ ends.
    cx.simulate_keystrokes("right");
    cx.run_until_parked();
    app.update(cx, |app, _| {
        assert_eq!(app.active_replacement_range().unwrap().start, starts[1]);
        assert!(app.active_is_kept());
    });
    cx.simulate_keystrokes("secondary-right");
    cx.run_until_parked();
    app.update(cx, |app, _| {
        assert_eq!(app.active_replacement_range().unwrap().start, starts[2])
    });
    // ↑ / ↓ step like ← / →.
    cx.simulate_keystrokes("up");
    cx.run_until_parked();
    app.update(cx, |app, _| {
        assert_eq!(app.active_replacement_range().unwrap().start, starts[1])
    });
    cx.simulate_keystrokes("down");
    cx.run_until_parked();
    cx.simulate_keystrokes("left");
    cx.run_until_parked();
    // Ticking with This one proposes only this mention again.
    app.update_in(cx, |app, window, cx| {
        assert_eq!(app.active_replacement_range().unwrap().start, starts[1]);
        app.pii.mapping.set_scope(Scope::Mention);
        app.toggle_replace(window, cx);
        assert_eq!(app.pii.review.remaining(), 1);
        assert_eq!(app.kept_entries().len(), 2);
        assert!(app.pii.popup.is_some() && !app.active_is_kept());
        assert_eq!(app.active_replacement_range().unwrap().start, starts[1]);
        // The chosen scope stays while stepping within the entity.
        app.step_identity_occurrence(Step::Next, window, cx);
        assert!(app.pii.mapping.scope() == Scope::Mention);
        app.step_identity_occurrence(Step::Previous, window, cx);
    });
    // Undo from the popup takes it back to kept, and the popup follows.
    cx.simulate_keystrokes("secondary-z");
    cx.run_until_parked();
    app.update(cx, |app, _| {
        assert_eq!(app.kept_entries().len(), 3);
        assert!(app.pii.popup.is_some() && app.active_is_kept());
        assert_eq!(app.active_replacement_range().unwrap().start, starts[1]);
    });
    // Closing resets the next popup to the whole entity.
    cx.simulate_keystrokes("escape");
    cx.run_until_parked();
    app.update_in(cx, |app, window, cx| {
        assert!(app.pii.popup.is_none());
        assert!(app.pii.mapping.scope() == Scope::Entity);
        // A click on a kept marker reopens its controls, unticked.
        app.activate_annotation(KEPT_ID | starts[2] as u64, window, cx);
        assert!(app.pii.popup.is_some() && app.active_is_kept());
        assert_eq!(app.active_replacement_range().unwrap().start, starts[2]);
    });
}

#[gpui::test]
fn stepping_keeps_the_focus_the_popup_returns_to(cx: &mut gpui::TestAppContext) {
    let (app, cx) = crate::document_view_tests::boot(cx);
    let source = "Anna met Bob. Anna left. Anna came back.";
    app.update_in(cx, |app, window, cx| {
        let ids = seed_anna(app, source, cx);
        window.focus(&app.editor.read(cx).focus_handle(cx), cx);
        app.activate_annotation(ids[0], window, cx);
        assert!(app.pii.focus.is_focused(window));
    });
    cx.simulate_keystrokes("right");
    cx.run_until_parked();
    cx.simulate_keystrokes("down");
    cx.run_until_parked();
    cx.simulate_keystrokes("escape");
    cx.run_until_parked();
    app.update_in(cx, |app, window, cx| {
        assert!(app.pii.popup.is_none());
        assert!(app.editor.read(cx).focus_handle(cx).is_focused(window));
    });
}

#[gpui::test]
fn popup_leaves_its_line_and_the_next_visible_and_scrolls_with_it(cx: &mut gpui::TestAppContext) {
    let (app, cx) = crate::document_view_tests::boot(cx);
    cx.simulate_resize(gpui::size(px(1200.), px(800.)));
    let filler = "A line of ordinary contract text.\n\n".repeat(30);
    let source = format!("{filler}Here Anna signs.\n\n{filler}");
    app.update_in(cx, |app, window, cx| {
        let ids = seed_anna(app, &source, cx);
        let start = source.find("Anna").unwrap();
        app.navigate_identity_mention(ids[0], start, window, cx);
    });
    cx.run_until_parked();
    draw_frames(cx);
    // Bring the mention to the middle of the view.
    app.update(cx, |app, cx| {
        let start = app.active_replacement_range().unwrap().start;
        let top = app.editor.read(cx).offset_screen_top(start).unwrap();
        let view = app.scroll.bounds();
        let mut offset = app.scroll.offset();
        offset.y -= top - view.center().y;
        app.scroll.set_offset(offset);
    });
    draw_frames(cx);
    let gap = |cx: &mut gpui::VisualTestContext| {
        let popup = cx.debug_bounds("pseudonym-popup").unwrap();
        app.update(cx, |app, cx| {
            let editor = app.editor.read(cx);
            let word = editor
                .annotation_bounds(app.active_annotation().unwrap())
                .unwrap();
            let line = editor.base_line_height();
            assert!(
                popup.top() >= word.bottom() + line || popup.bottom() <= word.top() - line,
                "popup {popup:?} covers the line next to {word:?}"
            );
            popup.top() - word.top()
        })
    };
    let before = gap(cx);
    // Scrolling moves the bar with its mention in the same frame.
    app.update(cx, |app, _| {
        let mut offset = app.scroll.offset();
        offset.y -= px(37.);
        app.scroll.set_offset(offset);
    });
    cx.update(|window, cx| {
        window.refresh();
        window.draw(cx).clear(cx);
    });
    assert_eq!(gap(cx), before);
}

/// Open the popup on the variant's `index`-th mention, focused for keys.
fn open_popup_on(
    app: &Entity<DocumentView>,
    variant: u64,
    index: usize,
    cx: &mut gpui::VisualTestContext,
) {
    app.update_in(cx, |app, window, cx| {
        let range = app.pii.review.variant(variant).unwrap().mentions[index].clone();
        let annotation = app.pii.review.annotation_id(variant, &range).unwrap();
        app.activate_annotation(annotation, window, cx);
        window.focus(&app.pii.focus, cx);
    });
    cx.run_until_parked();
    cx.update(|window, cx| window.draw(cx).clear(cx));
}

fn press(keys: &str, cx: &mut gpui::VisualTestContext) {
    cx.simulate_keystrokes(keys);
    cx.run_until_parked();
    cx.update(|window, cx| window.draw(cx).clear(cx));
}

#[gpui::test]
fn popup_enter_applies_the_whole_entity_under_all_and_opens_the_next(
    cx: &mut gpui::TestAppContext,
) {
    let (app, cx) = crate::document_view_tests::boot(cx);
    let source = "Павлова М.С. · Павлова М.С. · marina@example.invalid";
    let (_, initials) = app.update(cx, |app, cx| seed(app, source, cx));
    cx.run_until_parked();
    open_popup_on(&app, initials, 0, cx);
    let entity = app.read_with(cx, |app, _| {
        assert_eq!(app.pii.mapping.scope, Scope::Entity);
        app.selected_entity().unwrap()
    });
    press("enter", cx);
    app.read_with(cx, |app, cx| {
        assert_eq!(
            app.editor.read(cx).text(),
            "ORG_1 · ORG_1 · marina@example.invalid"
        );
        // The popup moved to the next entity's proposal, scope All.
        assert!(app.pii.popup.is_some());
        assert_ne!(app.selected_entity(), Some(entity));
        assert!(app.active_annotation().unwrap() & (APPLIED_ID | KEPT_ID) == 0);
        assert_eq!(app.pii.mapping.scope, Scope::Entity);
    });
    // The last proposal applied, nothing is left: the popup closes.
    press("enter", cx);
    app.read_with(cx, |app, cx| {
        assert!(!app.editor.read(cx).text().contains("marina@"));
        assert_eq!(app.pii.review.remaining(), 0);
        assert!(app.pii.popup.is_none());
    });
}

#[gpui::test]
fn popup_enter_under_this_one_applies_one_mention_and_steps_within_the_entity(
    cx: &mut gpui::TestAppContext,
) {
    let (app, cx) = crate::document_view_tests::boot(cx);
    let source = "Павлова М.С. · Павлова М.С. · marina@example.invalid";
    let (_, initials) = app.update(cx, |app, cx| seed(app, source, cx));
    cx.run_until_parked();
    open_popup_on(&app, initials, 0, cx);
    app.update(cx, |app, _| app.pii.mapping.set_scope(Scope::Mention));
    let entity = app.read_with(cx, |app, _| app.selected_entity().unwrap());
    press("enter", cx);
    app.read_with(cx, |app, cx| {
        assert_eq!(
            app.editor.read(cx).text(),
            "ORG_1 · Павлова М.С. · marina@example.invalid"
        );
        // The segment stays This one, on the entity's next proposal.
        assert_eq!(app.pii.mapping.scope, Scope::Mention);
        assert_eq!(app.selected_entity(), Some(entity));
        let range = app.active_replacement_range().unwrap();
        assert_eq!(&app.editor.read(cx).text()[range], "Павлова М.С.");
    });
    // The entity's last proposal: Enter moves to the next entity (All again).
    press("enter", cx);
    app.read_with(cx, |app, cx| {
        assert_eq!(
            app.editor.read(cx).text(),
            "ORG_1 · ORG_1 · marina@example.invalid"
        );
        assert!(app.pii.popup.is_some());
        assert_ne!(app.selected_entity(), Some(entity));
        assert_eq!(app.pii.mapping.scope, Scope::Entity);
    });
}

#[gpui::test]
fn popup_enter_on_an_applied_mention_does_nothing(cx: &mut gpui::TestAppContext) {
    let (app, cx) = crate::document_view_tests::boot(cx);
    let source = "Павлова М.С. · marina@example.invalid";
    let (_, initials) = app.update(cx, |app, cx| seed(app, source, cx));
    cx.run_until_parked();
    open_popup_on(&app, initials, 0, cx);
    app.update(cx, |app, cx| app.apply_scope(cx));
    cx.run_until_parked();
    cx.update(|window, cx| window.draw(cx).clear(cx));
    let (text, history) = app.read_with(cx, |app, cx| {
        assert!(app.active_annotation().unwrap() & APPLIED_ID != 0);
        (
            app.editor.read(cx).text().to_owned(),
            app.editor.read(cx).history_id(),
        )
    });
    press("enter", cx);
    app.read_with(cx, |app, cx| {
        assert_eq!(app.editor.read(cx).text(), text);
        assert_eq!(app.editor.read(cx).history_id(), history);
        assert!(app.pii.popup.is_some());
    });
}

#[gpui::test]
fn apply_all_closes_the_popup(cx: &mut gpui::TestAppContext) {
    let (app, cx) = crate::document_view_tests::boot(cx);
    let source = "Павлова М.С. · Павлова М.С. · marina@example.invalid";
    let (_, initials) = app.update(cx, |app, cx| seed(app, source, cx));
    cx.run_until_parked();
    open_popup_on(&app, initials, 0, cx);
    press("secondary-enter", cx);
    app.read_with(cx, |app, _| {
        assert_eq!(app.pii.review.remaining(), 0);
        assert!(app.pii.popup.is_none());
    });
    // One undo brings every proposal back.
    let undo = if cfg!(target_os = "macos") {
        "cmd-z"
    } else {
        "ctrl-z"
    };
    press(undo, cx);
    app.read_with(cx, |app, cx| {
        assert_eq!(app.editor.read(cx).text(), source);
    });
}

#[gpui::test]
fn popup_keep_keys_leave_the_segment_as_shown(cx: &mut gpui::TestAppContext) {
    let (app, cx) = crate::document_view_tests::boot(cx);
    let source = "Павлова М.С. · Павлова М.С. · marina@example.invalid";
    let (_, initials) = app.update(cx, |app, cx| seed(app, source, cx));
    cx.run_until_parked();
    open_popup_on(&app, initials, 0, cx);
    let entity = app.read_with(cx, |app, _| app.selected_entity().unwrap());
    // ⌫ keeps only this mention; the segment still reads All.
    press("backspace", cx);
    app.read_with(cx, |app, _| {
        assert!(app.active_is_kept());
        assert_eq!(app.pii.review.identity_count(entity), 1);
        assert_eq!(app.pii.mapping.scope, Scope::Entity);
    });
}

/// Add `word` by hand as the user would: select it, then Add (ADR 0024).
fn add_by_hand(app: &Entity<DocumentView>, word: &str, cx: &mut gpui::VisualTestContext) {
    app.update_in(cx, |app, window, cx| {
        let at = app.editor.read(cx).text().find(word).unwrap();
        app.editor
            .update(cx, |e, cx| e.set_selection(at..at + word.len(), cx));
        app.add_pii_candidate(&crate::pii::ui::PiiAddCandidate, window, cx);
        window.focus(&app.pii.focus, cx);
    });
    cx.run_until_parked();
    cx.update(|window, cx| window.draw(cx).clear(cx));
}

#[gpui::test]
fn popup_enter_after_a_manual_addition_stays_and_cancel_reverts_both(
    cx: &mut gpui::TestAppContext,
) {
    let (app, cx) = crate::document_view_tests::boot(cx);
    let source = "Павлова М.С. · Акме · marina@example.invalid";
    for draft in [None, Some("CLIENT_9")] {
        app.update(cx, |app, cx| {
            seed(app, source, cx);
        });
        cx.run_until_parked();
        add_by_hand(&app, "Акме", cx);
        if let Some(draft) = draft {
            // An alias draft is confirmed with the apply, in one step.
            app.update(cx, |app, cx| {
                app.pii
                    .mapping
                    .alias
                    .update(cx, |input, cx| input.set_value(draft.into(), cx))
            });
        }
        press("enter", cx);
        app.read_with(cx, |app, cx| {
            assert!(!app.editor.read(cx).text().contains("Акме"), "{draft:?}");
            assert!(app.pii.popup.is_some(), "{draft:?}");
            assert!(app.active_annotation().unwrap() & APPLIED_ID != 0);
        });
        let cancel = cx.debug_bounds("direct-cancel-addition");
        assert!(cancel.is_some(), "{draft:?}");
        cx.simulate_click(cancel.unwrap().center(), Default::default());
        cx.run_until_parked();
        app.read_with(cx, |app, cx| {
            assert_eq!(app.editor.read(cx).text(), source, "{draft:?}");
        });
    }
}

#[gpui::test]
fn popup_enter_under_this_one_with_an_alias_draft_keeps_the_segment(cx: &mut gpui::TestAppContext) {
    let (app, cx) = crate::document_view_tests::boot(cx);
    let source = "Павлова М.С. · Павлова М.С. · Павлова М.С.";
    let (_, initials) = app.update(cx, |app, cx| seed(app, source, cx));
    cx.run_until_parked();
    open_popup_on(&app, initials, 0, cx);
    app.update(cx, |app, cx| {
        app.pii.mapping.set_scope(Scope::Mention);
        app.pii
            .mapping
            .alias
            .update(cx, |input, cx| input.set_value("CLIENT_1".into(), cx));
    });
    press("enter", cx);
    app.read_with(cx, |app, cx| {
        assert_eq!(
            app.editor.read(cx).text(),
            "CLIENT_1 · Павлова М.С. · Павлова М.С."
        );
        assert_eq!(app.pii.mapping.scope, Scope::Mention);
    });
    // The next Enter still applies one mention.
    press("enter", cx);
    app.read_with(cx, |app, cx| {
        assert_eq!(
            app.editor.read(cx).text(),
            "CLIENT_1 · ORG_1 · Павлова М.С."
        );
        assert_eq!(app.pii.mapping.scope, Scope::Mention);
    });
}

#[gpui::test]
fn popup_enter_wraps_and_skips_entities_without_proposals(cx: &mut gpui::TestAppContext) {
    let (app, cx) = crate::document_view_tests::boot(cx);
    let source = "Павлова М.С. · marina@example.invalid · Павлова М.С. · Павлова Марина Сергеевна";
    let (full, initials) = app.update(cx, |app, cx| seed(app, source, cx));
    cx.run_until_parked();
    // This one from the entity's last proposal wraps to its first.
    open_popup_on(&app, initials, 1, cx);
    app.update(cx, |app, _| app.pii.mapping.set_scope(Scope::Mention));
    let entity = app.read_with(cx, |app, _| app.selected_entity().unwrap());
    press("enter", cx);
    app.read_with(cx, |app, cx| {
        assert_eq!(app.selected_entity(), Some(entity));
        assert_eq!(app.active_replacement_range().unwrap().start, 0);
        assert!(app.editor.read(cx).text().starts_with("Павлова М.С. · "));
    });
    // With the full name applied, Enter on the email goes to the next entity
    // with proposals: the initials, wherever they sit in panel order.
    let person = app.read_with(cx, |app, _| app.pii.review.variant_identity(full).unwrap());
    open_popup_on(&app, full, 0, cx);
    press("enter", cx);
    let order = app.read_with(cx, |app, cx| app.panel_entity_order(cx));
    let email = *order
        .iter()
        .find(|id| ![entity, person].contains(id))
        .unwrap();
    app.update_in(cx, |app, window, cx| {
        let (annotation, _) = app.identity_occurrences(email)[0].clone();
        app.activate_annotation(annotation, window, cx);
        window.focus(&app.pii.focus, cx);
    });
    cx.run_until_parked();
    cx.update(|window, cx| window.draw(cx).clear(cx));
    press("enter", cx);
    app.read_with(cx, |app, _| {
        assert_eq!(app.selected_entity(), Some(entity));
        assert!(app.active_annotation().unwrap() & (APPLIED_ID | KEPT_ID) == 0);
    });
}

#[gpui::test]
fn popup_enter_on_a_kept_mention_and_apply_all_with_nothing_left(cx: &mut gpui::TestAppContext) {
    let (app, cx) = crate::document_view_tests::boot(cx);
    let source = "Павлова М.С. · marina@example.invalid";
    let (_, initials) = app.update(cx, |app, cx| seed(app, source, cx));
    cx.run_until_parked();
    open_popup_on(&app, initials, 0, cx);
    press("space", cx);
    let history = app.read_with(cx, |app, cx| {
        assert!(app.active_is_kept());
        app.editor.read(cx).history_id()
    });
    press("enter", cx);
    app.read_with(cx, |app, cx| {
        assert_eq!(app.editor.read(cx).history_id(), history);
        assert!(app.pii.popup.is_some());
    });
    // Apply the rest, reopen the kept mention: Mod+Enter with nothing left
    // closes the popup without a new step.
    press("secondary-enter", cx);
    app.update_in(cx, |app, window, cx| {
        assert_eq!(app.pii.review.remaining(), 0);
        let kept = app.kept_selection(0).unwrap();
        app.select_occurrence(kept, cx);
        app.pii.keep_selection_popup();
        app.remember_active_replacement(cx);
        window.focus(&app.pii.focus, cx);
    });
    cx.run_until_parked();
    cx.update(|window, cx| window.draw(cx).clear(cx));
    let history = app.read_with(cx, |app, cx| {
        assert!(app.pii.popup.is_some());
        app.editor.read(cx).history_id()
    });
    press("secondary-enter", cx);
    app.read_with(cx, |app, cx| {
        assert!(app.pii.popup.is_none());
        assert_eq!(app.editor.read(cx).history_id(), history);
    });
}

#[gpui::test]
fn popup_wider_keep_keys_leave_the_segment_as_shown(cx: &mut gpui::TestAppContext) {
    let (app, cx) = crate::document_view_tests::boot(cx);
    let source = "Павлова М.С. · Павлова М.С. · Павлова М.С.";
    let (_, initials) = app.update(cx, |app, cx| seed(app, source, cx));
    cx.run_until_parked();
    open_popup_on(&app, initials, 0, cx);
    app.update(cx, |app, _| app.pii.mapping.set_scope(Scope::Mention));
    press("secondary-backspace", cx);
    app.read_with(cx, |app, _| {
        assert!(app.active_is_kept());
        assert_eq!(app.pii.mapping.scope, Scope::Mention);
    });
    let (_, initials) = app.update(cx, |app, cx| seed(app, source, cx));
    cx.run_until_parked();
    open_popup_on(&app, initials, 1, cx);
    app.update(cx, |app, _| app.pii.mapping.set_scope(Scope::Mention));
    press("secondary-shift-backspace", cx);
    app.read_with(cx, |app, _| {
        assert_eq!(app.pii.mapping.scope, Scope::Mention);
    });
}
