use super::*;
use crate::{
    document_view::{CopyMarkdown, DocumentView},
    pii::ui::{PiiAddCandidate, PiiApplyAll, PiiConfirm, Pseudonymize},
    settings_ui,
    style::Theme,
};
use gpui::{App, Context, Entity, Focusable, Window, px};
use mdoc_pii::{Category, Review};
use mdoc_store::{document::Document, settings};
use std::{
    ops::Range,
    sync::{Arc, atomic::AtomicBool},
};

fn install_scan(app: &mut DocumentView, cx: &mut Context<DocumentView>) -> (u64, u64, u64) {
    let revision = app.editor.read(cx).revision();
    let generation = app.pii.generation;
    let identity = app.session.generation;
    app.pii.reviewing = true;
    app.pii.job = Some(ScanJob {
        cancel: Arc::new(AtomicBool::new(false)),
        revision,
        generation,
        config: settings::PiiConfig::default(),
        progress: Default::default(),
    });
    (generation, identity, revision)
}

#[gpui::test]
fn forgetting_a_case_identity_rejects_an_older_scan_result(cx: &mut gpui::TestAppContext) {
    let (app, cx) = crate::document_view::tests::boot(cx);
    app.update(cx, |app, cx| {
        app.editor
            .update(cx, |editor, cx| editor.set_text("Anna", cx))
    });
    cx.run_until_parked();
    app.update(cx, |app, cx| {
        app.pii
            .review
            .add_manual("Anna", 0..4, Category::Person)
            .unwrap();
        let (generation, identity, revision) = install_scan(app, cx);
        let cancel = app.pii.job.as_ref().unwrap().cancel.clone();
        app.forget_case_identity(&["PERSON_1"], &["Anna".into()], &[], cx);
        assert!(cancel.load(std::sync::atomic::Ordering::Relaxed));
        app.complete_pii_scan(
            generation,
            identity,
            revision,
            Ok(vec![mdoc_pii::Detection {
                range: 0..4,
                category: Category::Person,
                score: 0.9,
                recognizer: mdoc_pii::Recognizer::Model,
            }]),
            cx,
        );
        assert!(app.pii.review.export_identities().is_empty());
        assert_eq!(app.editor.read(cx).text(), "Anna");
    });
}
#[gpui::test]
fn scan_proposes_without_applying_until_explicit_apply(cx: &mut gpui::TestAppContext) {
    let (app, cx) = crate::document_view::tests::boot(cx);
    app.update(cx, |app, cx| {
        app.editor
            .update(cx, |editor, cx| editor.set_text("Alice", cx));
    });
    cx.run_until_parked();
    app.update(cx, |app, cx| {
        let (generation, identity, revision) = install_scan(app, cx);
        assert!(!app.pseudonymized(), "a running scan is not a result");
        app.complete_pii_scan(
            generation,
            identity,
            revision,
            Ok(vec![mdoc_pii::Detection {
                range: 0..5,
                category: Category::Person,
                score: 0.9,
                recognizer: mdoc_pii::Recognizer::Model,
            }]),
            cx,
        );
        assert_eq!(app.editor.read(cx).text(), "Alice");
        assert_eq!(app.editor.read(cx).revision(), revision);
        assert_eq!(app.pii.review.remaining(), 1);
        assert!(app.pii.mapping.open);
        assert!(app.pseudonymized());
    });
    cx.run_until_parked();
    app.update_in(cx, |app, window, cx| {
        let id = app.pii.review.variants()[0].id;
        let range = app.pii.review.variant(id).unwrap().mentions[0].clone();
        let annotation = app.pii.review.annotation_id(id, &range).unwrap();
        app.activate_annotation(annotation, window, cx);
        app.apply_replacements(cx);
        assert_eq!(app.editor.read(cx).text(), "PERSON_1");
        assert_eq!(app.pii.review.remaining(), 0);
        window.focus(&app.editor.read(cx).focus_handle(cx), cx);
    });
    cx.dispatch_action(mdoc_editor::Undo);
    cx.run_until_parked();
    app.update(cx, |app, cx| {
        assert_eq!(app.editor.read(cx).text(), "Alice");
        assert_eq!(app.pii.review.remaining(), 1);
    });
}

#[gpui::test]
fn a_failed_scan_still_offers_manual_replace(cx: &mut gpui::TestAppContext) {
    let (app, cx) = crate::document_view::tests::boot(cx);
    app.update(cx, |app, cx| {
        app.editor
            .update(cx, |editor, cx| editor.set_text("Alice Morgan signs", cx));
    });
    cx.run_until_parked();
    app.update(cx, |app, cx| {
        // Selected before the scan finishes: Replace appears once it fails.
        app.editor
            .update(cx, |editor, cx| editor.set_selection(0..12, cx));
        let (generation, identity, revision) = install_scan(app, cx);
        app.complete_pii_scan(
            generation,
            identity,
            revision,
            Err("Automatic scanning is unavailable on this platform.".into()),
            cx,
        );
        assert!(app.pii.scans.is_empty());
        app.editor
            .update(cx, |editor, cx| editor.set_selection(0..12, cx));
        assert!(
            matches!(app.manual_target(cx), Some(Ok(_))),
            "the panel tells the user to select text and choose Replace"
        );
        assert!(
            app.editor.read(cx).selection_action().is_some(),
            "the pill is refreshed without a new selection"
        );
    });
}

#[gpui::test]
#[ignore = "requires installed verified GLiNER2 FP16 and ONNX Runtime; no downloads"]
fn installed_model_offline_smoke(cx: &mut gpui::TestAppContext) {
    let (app, cx) = crate::document_view::tests::boot(cx);
    let source = "Alice Morgan represents Northbridge Legal Ltd. Contact alice@example.invalid.\n";
    // Tests report every model as missing; this smoke uses the installed one.
    app.update(cx, |app, cx| {
        app.model_panel
            .update(cx, |panel, _| panel.pending.fill(Default::default()));
    });
    app.update(cx, |app, cx| {
        app.editor
            .update(cx, |editor, cx| editor.set_text(source, cx));
    });
    cx.run_until_parked();
    app.update_in(cx, |app, window, cx| {
        app.pseudonymize(&Pseudonymize, window, cx)
    });
    cx.run_until_parked();
    app.update(cx, |app, cx| {
        assert!(app.pii.error.is_none(), "{:?}", app.pii.error);
        let count = app.pii.review.remaining();
        assert!(count > 0);
        assert_eq!(app.editor.read(cx).text(), source);
        app.apply_replacements(cx);
        let result = app.editor.read(cx).text();
        assert!(result.contains("PERSON_"));
        assert!(result.contains("EMAIL_"));
        eprintln!("installed offline pseudonymization: {count} replacements; {result}");
    });
}
#[gpui::test]
fn scan_then_apply_is_one_undo_step_and_copy_is_explicit(cx: &mut gpui::TestAppContext) {
    let (app, cx) = crate::document_view::tests::boot(cx);
    let source = "**Анна** Bob Анна [mail](anna@example.invalid)";
    let applied = "**Анна** PERSON_2 PERSON_1 [mail](EMAIL_1)";
    let dir = tempfile::tempdir().unwrap();
    let original = dir.path().join("original.md");
    std::fs::write(&original, source).unwrap();
    let app = crate::document_view::tests::open_document(&app, original.clone(), cx);
    app.update(cx, |app, cx| {
        app.session.warning = Some("Review extraction".into());
        let kept = app
            .pii
            .review
            .add_manual(source, 2..10, Category::Person)
            .unwrap();
        app.pii.review.keep(kept, Some(2..10));
        cx.write_to_clipboard(gpui::ClipboardItem::new_string("keep clipboard".into()));
        let (generation, identity, revision) = install_scan(app, cx);
        let bob = source.find("Bob").unwrap();
        let mail = source.find("anna@example.invalid").unwrap();
        app.complete_pii_scan(
            generation,
            identity,
            revision,
            Ok(vec![
                mdoc_pii::Detection {
                    range: 2..10,
                    category: Category::Person,
                    score: 0.9,
                    recognizer: mdoc_pii::Recognizer::Model,
                },
                mdoc_pii::Detection {
                    range: bob..bob + 3,
                    category: Category::Person,
                    score: 0.9,
                    recognizer: mdoc_pii::Recognizer::Model,
                },
                mdoc_pii::Detection {
                    range: mail..mail + 20,
                    category: Category::Email,
                    score: 0.9,
                    recognizer: mdoc_pii::Recognizer::Model,
                },
            ]),
            cx,
        );
        assert_eq!(app.editor.read(cx).text(), source);
        assert_eq!(app.pii.review.remaining(), 3);
        app.apply_replacements(cx);
        assert_eq!(app.editor.read(cx).text(), applied);
        assert_eq!(
            cx.read_from_clipboard().unwrap().text().as_deref(),
            Some("keep clipboard")
        );
        assert!(app.dirty(cx));
        assert!(app.session.warning.is_some());
    });
    cx.run_until_parked();
    cx.update(|window, cx| window.draw(cx).clear(cx));
    assert!(cx.debug_bounds("identity-panel").is_some());
    app.update_in(cx, |app, window, cx| {
        app.copy_markdown(&CopyMarkdown, window, cx);
        assert_eq!(
            cx.read_from_clipboard().unwrap().text().as_deref(),
            Some(applied)
        );
        let result = app.editor.read(cx).text().to_owned();
        app.session
            .save(dir.path().join("prepared.md"), &result)
            .unwrap();
        window.focus(&app.editor.read(cx).focus_handle(cx), cx);
    });
    assert_eq!(std::fs::read_to_string(original).unwrap(), source);
    cx.dispatch_action(mdoc_editor::Undo);
    cx.run_until_parked();
    app.update(cx, |app, cx| {
        assert_eq!(app.editor.read(cx).text(), source);
        assert_eq!(app.pii.review.remaining(), 3);
    });
    cx.dispatch_action(mdoc_editor::Redo);
    cx.run_until_parked();
    assert_eq!(
        app.read_with(cx, |app, cx| app.editor.read(cx).text().to_owned()),
        applied
    );
}
#[gpui::test]
fn failed_cancelled_stale_and_superseded_scans_do_not_edit(cx: &mut gpui::TestAppContext) {
    let (app, cx) = crate::document_view::tests::boot(cx);
    for case in 0..6 {
        app.update(cx, |app, cx| {
            app.editor
                .update(cx, |editor, cx| editor.set_text("Alice", cx));
        });
        cx.run_until_parked();
        app.update(cx, |app, cx| {
            let (generation, identity, revision) = install_scan(app, cx);
            let mut result = Ok(vec![mdoc_pii::Detection {
                range: 0..5,
                category: Category::Person,
                score: 0.9,
                recognizer: mdoc_pii::Recognizer::Model,
            }]);
            match case {
                0 => result = Err("Partial scan rejected".into()),
                1 => result.as_mut().unwrap()[0].range = 0..99,
                2 => app.pii.cancel(),
                3 => {
                    app.editor
                        .update(cx, |editor, cx| editor.replace_range(0..5, "Betty", cx));
                }
                4 => app.reset_pii(cx),
                _ => app.session.replace(Document::default()),
            }
            app.complete_pii_scan(generation, identity, revision, result, cx);
            assert_eq!(
                app.editor.read(cx).text(),
                if case == 3 { "Betty" } else { "Alice" }
            );
            assert_eq!(app.pii.review.remaining(), 0);
            assert!(!app.pseudonymized(), "rejected scans do not count");
            app.pii.cancel();
        });
        cx.run_until_parked();
    }
}
#[gpui::test]
fn manual_replacements_count_without_a_scan_or_open_review(cx: &mut gpui::TestAppContext) {
    let (app, cx) = crate::document_view::tests::boot(cx);
    app.update(cx, |app, cx| {
        app.editor
            .update(cx, |editor, cx| editor.set_text("Alice", cx));
    });
    cx.run_until_parked();
    app.update(cx, |app, cx| {
        app.pii
            .review
            .add_manual("Alice", 0..5, Category::Person)
            .unwrap();
        assert!(app.pii.scans.is_empty());
        assert!(!app.pii.reviewing);
        assert!(app.pseudonymized(), "a manual proposal is replacement data");
        app.apply_replacements(cx);
        assert!(!app.pii.review.applied().is_empty());
        assert!(app.pseudonymized(), "applied aliases retain the status");
    });
}
#[gpui::test]
fn header_pseudonymize_scans_once_and_the_panel_toggle_hides_the_review(
    cx: &mut gpui::TestAppContext,
) {
    let (app, cx) = crate::document_view::tests::boot(cx);
    cx.simulate_resize(gpui::size(px(720.), px(480.)));
    // An empty document has nothing to pseudonymize (ADR 0048).
    cx.update(|window, cx| window.draw(cx).clear(cx));
    assert!(cx.debug_bounds("Pseudonymize").is_none());
    app.update(cx, |app, cx| {
        app.editor
            .update(cx, |editor, cx| editor.set_text("Notes", cx))
    });
    let click = |cx: &mut gpui::VisualTestContext, id: &'static str| {
        cx.update(|window, cx| {
            window.refresh();
            window.draw(cx).clear(cx);
        });
        let bounds = cx.debug_bounds(id).unwrap();
        cx.simulate_click(bounds.center(), Default::default());
        cx.run_until_parked();
    };
    for theme in [Theme::Dark, Theme::Light] {
        app.update(cx, |app, cx| {
            app.theme.set(theme);
            cx.notify();
        });
        cx.update(|window, cx| {
            window.refresh();
            window.draw(cx).clear(cx);
        });
        assert!(cx.debug_bounds("Pseudonymize").is_some());
        assert!(cx.debug_bounds("Anonymize").is_none());
    }
    // Invalid preferences exercise the actual icon without loading a model
    // or downloading anything in the UI test.
    app.update(cx, |app, cx| {
        app.preferences.borrow_mut().current = None;
        cx.notify();
    });
    click(cx, "Pseudonymize");
    app.update(cx, |app, _| {
        assert!(app.pii.error.is_some());
        assert!(app.pii.reviewing);
        assert!(app.pii.mapping.open);
    });
    click(cx, "header-panel");
    assert!(app.read_with(cx, |app, _| !app.pii.mapping.open));
    click(cx, "Pseudonymize");
    app.update(cx, |app, _| {
        assert!(app.pii.mapping.open);
        assert!(!app.pii.scanning());
    });
    app.update_in(cx, |app, window, cx| {
        app.editor
            .update(cx, |editor, cx| editor.set_text("Alice", cx));
        let id = app
            .pii
            .review
            .add_manual("Alice", 0..5, Category::Person)
            .unwrap();
        app.review_changed(cx);
        let range = app.pii.review.variant(id).unwrap().mentions[0].clone();
        let annotation = app.pii.review.annotation_id(id, &range).unwrap();
        app.activate_annotation(annotation, window, cx);
    });
    cx.run_until_parked();
    cx.update(|window, cx| window.draw(cx).clear(cx));
    assert!(cx.debug_bounds("pseudonym-popup").is_some());
    assert!(cx.debug_bounds("direct-alias").is_some());
    assert_eq!(
        app.read_with(cx, |app, cx| app.editor.read(cx).text().to_owned()),
        "Alice"
    );
    let next = crate::document_view::tests::new_document(&app, cx);
    next.read_with(cx, |app, _| {
        assert!(!app.pii.reviewing);
        assert!(!app.pii.mapping.open);
    });
}
#[gpui::test]
fn accept_all_button_applies_pending_replacements_as_one_undo_step(cx: &mut gpui::TestAppContext) {
    let (app, cx) = crate::document_view::tests::boot(cx);
    let source = "**Анна** Acme Анна [mail](anna@example.invalid) Bob";
    app.update(cx, |app, cx| {
        app.editor
            .update(cx, |editor, cx| editor.set_text(source, cx));
    });
    cx.run_until_parked();
    app.update_in(cx, |app, window, cx| {
        app.pii.mapping.open = true;
        app.pii.reviewing = true;
        let review = &mut app.pii.review;
        let anna = review.add_manual(source, 2..10, Category::Person).unwrap();
        let acme = source.find("Acme").unwrap();
        review
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
        app.review_changed(cx);
        let range = app.pii.review.variant(anna).unwrap().mentions[0].clone();
        let annotation = app.pii.review.annotation_id(anna, &range).unwrap();
        app.activate_annotation(annotation, window, cx);
        app.pii
            .mapping
            .alias
            .update(cx, |input, cx| input.set_value("PERSON_CUSTOM".into(), cx));
        window.focus(&app.focus, cx);
        cx.notify();
    });
    cx.run_until_parked();
    cx.update(|window, cx| window.draw(cx).clear(cx));
    let bounds = cx.debug_bounds("primary-apply").unwrap();
    cx.simulate_click(bounds.center(), gpui::Modifiers::none());
    cx.run_until_parked();
    app.update(cx, |app, cx| {
        assert_eq!(
            app.editor.read(cx).text(),
            "**Анна** ORG_1 PERSON_CUSTOM [mail](EMAIL_1) Bob"
        );
        assert_eq!(app.pii.review.remaining(), 0);
        // Nothing is left to review, so Apply closes the popup (ADR 0055).
        assert!(app.pii.popup.is_none());
        assert!(app.dirty(cx));
        assert!(app.pii.error.is_none());
    });
    // With no pending suggestions, another click must not add an undo step.
    let revision = app.read_with(cx, |app, cx| app.editor.read(cx).revision());
    cx.update(|window, cx| window.draw(cx).clear(cx));
    assert!(cx.debug_bounds("primary-apply").is_none());
    app.update_in(cx, |app, window, cx| {
        assert_eq!(app.editor.read(cx).revision(), revision);
        window.focus(&app.editor.read(cx).focus_handle(cx), cx);
    });
    cx.dispatch_action(mdoc_editor::Undo);
    cx.run_until_parked();
    app.update(cx, |app, cx| {
        assert_eq!(app.editor.read(cx).text(), source);
        assert_eq!(app.pii.review.remaining(), 3);
    });
    cx.dispatch_action(mdoc_editor::Redo);
    cx.run_until_parked();
    app.update(cx, |app, cx| {
        assert_eq!(
            app.editor.read(cx).text(),
            "**Анна** ORG_1 PERSON_CUSTOM [mail](EMAIL_1) Bob"
        );
        assert_eq!(app.pii.review.remaining(), 0);
    });
}
#[gpui::test]
fn keyboard_search_and_navigation_reach_virtualized_alias_targets(cx: &mut gpui::TestAppContext) {
    let (app, cx) = crate::document_view::tests::boot(cx);
    cx.simulate_resize(gpui::size(px(640.), px(480.)));
    let source = (0..30)
        .map(|i| format!("Name{i:02}"))
        .collect::<Vec<_>>()
        .join(" ");
    app.update_in(cx, |app, window, cx| {
        app.editor
            .update(cx, |editor, cx| editor.set_text(&source, cx));
        app.pii.review = Review::default();
        app.pii.reviewing = true;
        for i in 0..30 {
            app.pii
                .review
                .add_manual(&source, i * 7..i * 7 + 6, Category::Person)
                .unwrap();
        }
        app.review_changed(cx);
        let annotation = app.pii.review.candidates()[0].id;
        app.activate_annotation(annotation, window, cx);
        app.pii.mapping.show_alias_choices();
        window.focus(&app.pii.mapping.alias.read(cx).focus_handle(cx), cx);
    });
    cx.run_until_parked();
    for _ in 0..29 {
        cx.simulate_keystrokes("down");
        cx.run_until_parked();
    }
    for _ in 0..3 {
        cx.update(|window, cx| {
            window.refresh();
            window.draw(cx).clear(cx);
        });
    }
    app.update(cx, |app, cx| {
        assert_eq!(app.pii.mapping.target_index, Some(28));
        assert_eq!(app.editor.read(cx).text(), source);
    });
    let last = cx
        .debug_bounds("target-30")
        .expect("keyboard-selected target must be painted");
    // Alias targets float in their own menu over the popup (ADR 0032).
    let menu = cx.debug_bounds("direct-alias-menu").unwrap();
    assert!(
        last.top() >= menu.top() && last.bottom() <= menu.bottom(),
        "target {last:?} menu {menu:?}"
    );
    cx.simulate_keystrokes("enter");
    cx.run_until_parked();
    app.update(cx, |app, cx| {
        assert_eq!(app.pii.mapping.alias.read(cx).value(), "PERSON_30");
        assert_eq!(app.editor.read(cx).text(), source);
        assert!(app.pii.popup.is_some());
    });
}

#[gpui::test]
fn accept_all_refuses_pending_scans_and_invalid_alias_drafts(cx: &mut gpui::TestAppContext) {
    let (app, cx) = crate::document_view::tests::boot(cx);
    let source = "Alice Acme";
    app.update(cx, |app, cx| {
        app.editor
            .update(cx, |editor, cx| editor.set_text(source, cx));
    });
    cx.run_until_parked();
    app.update_in(cx, |app, window, cx| {
        app.pii.reviewing = true;
        let alice = app
            .pii
            .review
            .add_manual(source, 0..5, Category::Person)
            .unwrap();
        app.pii
            .review
            .add_manual(source, 6..10, Category::Organization)
            .unwrap();
        let revision = app.editor.read(cx).revision();
        app.pii.job = Some(ScanJob {
            cancel: Arc::new(AtomicBool::new(false)),
            revision,
            generation: 1,
            config: settings::PiiConfig::default(),
            progress: Default::default(),
        });
        app.apply_all_pii(&PiiApplyAll, window, cx);
        assert_eq!(app.editor.read(cx).revision(), revision);
        assert_eq!(app.editor.read(cx).text(), source);
        app.pii.cancel();
        let range = app.pii.review.variant(alice).unwrap().mentions[0].clone();
        let annotation = app.pii.review.annotation_id(alice, &range).unwrap();
        app.activate_annotation(annotation, window, cx);
        app.pii
            .mapping
            .alias
            .update(cx, |input, cx| input.set_value("invalid token".into(), cx));
        app.apply_all_pii(&PiiApplyAll, window, cx);
        assert_eq!(app.editor.read(cx).revision(), revision);
        assert_eq!(app.editor.read(cx).text(), source);
        assert_eq!(app.pii.review.remaining(), 2);
    });
}
#[gpui::test]
fn missing_model_asks_first_and_resumes_only_after_this_setup(cx: &mut gpui::TestAppContext) {
    let (app, cx) = crate::document_view::tests::boot(cx);
    cx.simulate_resize(gpui::size(px(1000.), px(700.)));
    let panel = cx.update(|_, cx| app.read(cx).model_panel.clone());
    let fp16 = settings::Model::Pii(settings::PiiModel::Fp16);
    app.update(cx, |app, cx| {
        app.editor
            .update(cx, |editor, cx| editor.set_text("Alice", cx));
    });
    let draw = |cx: &mut gpui::VisualTestContext| {
        cx.update(|window, cx| {
            window.refresh();
            window.draw(cx).clear(cx);
        })
    };
    draw(cx);
    let bounds = cx.debug_bounds("Pseudonymize").unwrap();
    cx.simulate_click(bounds.center(), Default::default());
    cx.run_until_parked();
    draw(cx);
    // Tests always report the model as missing: ask, never start a scan.
    app.read_with(cx, |app, _| {
        assert!(app.pii.setup);
        assert!(app.pii.mapping.open);
        assert!(!app.pii.scanning());
    });
    assert!(cx.debug_bounds("pii-setup-card").is_some());
    assert!(cx.debug_bounds("replacement-commands").is_none());
    cx.update(|_, cx| assert!(panel.read(cx).working.is_none()));

    // Every precision shows the export repository (not just the base model)
    // and its manifest license on the consent card, without opening Settings.
    for model in settings::PiiModel::ALL {
        app.update(cx, |app, cx| {
            app.preferences
                .borrow_mut()
                .current
                .as_mut()
                .unwrap()
                .pseudonymization
                .model = model;
            cx.notify();
        });
        draw(cx);
        let model = settings::Model::Pii(model);
        let label = format!(
            "{}: {}",
            model.name(),
            crate::models::attribution_label(model)
        );
        let bounds = cx
            .debug_bounds(Box::leak(label.into_boxed_str()))
            .expect("PII source and license");
        let card = cx.debug_bounds("pii-setup-card").unwrap();
        assert!(bounds.top() >= card.top() && bounds.bottom() <= card.bottom());
    }
    app.update(cx, |app, _| {
        app.preferences
            .borrow_mut()
            .current
            .as_mut()
            .unwrap()
            .pseudonymization
            .model = settings::PiiModel::Fp16;
    });

    // A download finished elsewhere (Settings) never scans by itself.
    panel.update(cx, |panel, cx| {
        panel.pending.fill(Default::default());
        panel.statuses[settings_ui::Panel::index(fp16)] = settings_ui::Status::Ready;
        cx.emit(settings_ui::Event::Finished(fp16, Ok(None)));
    });
    app.read_with(cx, |app, _| {
        assert!(app.pii.setup);
        assert!(!app.pii.scanning());
    });

    // Failure keeps the card with the error for Retry.
    app.update_in(cx, |app, window, cx| app.watch_pii_setup(fp16, window, cx));
    panel.update(cx, |_, cx| {
        cx.emit(settings_ui::Event::Finished(
            fp16,
            Err("Download cancelled.".into()),
        ))
    });
    app.read_with(cx, |app, _| {
        assert!(app.pii.setup);
        assert_eq!(app.pii.error.as_deref(), Some("Download cancelled."));
    });

    // Download & scan resumes this document once.
    app.update_in(cx, |app, window, cx| app.watch_pii_setup(fp16, window, cx));
    panel.update(cx, |_, cx| {
        cx.emit(settings_ui::Event::Finished(fp16, Ok(None)))
    });
    app.read_with(cx, |app, _| {
        assert!(!app.pii.setup);
        assert!(app.pii.scanning());
    });
    app.update(cx, |app, _| app.pii.cancel());
    cx.run_until_parked();

    // Closing the panel discards the card.
    app.update_in(cx, |app, window, cx| {
        app.pii.setup = true;
        app.close_replacements(window, cx);
        assert!(!app.pii.setup);
    });
}
#[gpui::test]
fn cancelled_edited_and_replaced_document_results_are_rejected(cx: &mut gpui::TestAppContext) {
    let (app, cx) = crate::document_view::tests::boot(cx);
    app.update(cx, |app, cx| {
        app.editor
            .update(cx, |editor, cx| editor.set_text("Alice", cx));
        let result = || {
            Ok(vec![mdoc_pii::Detection {
                range: 0..5,
                category: Category::Person,
                score: 0.9,
                recognizer: mdoc_pii::Recognizer::Model,
            }])
        };
        let identity = app.session.generation;
        let revision = app.editor.read(cx).revision();
        let install_job = |app: &mut DocumentView| {
            app.pii.job = Some(ScanJob {
                cancel: Arc::new(AtomicBool::new(false)),
                revision,
                generation: 7,
                config: settings::PiiConfig::default(),
                progress: Default::default(),
            });
        };
        install_job(app);
        app.pii.cancel();
        app.complete_pii_scan(7, identity, revision, result(), cx);
        assert!(app.pii.review.variants().is_empty());
        install_job(app);
        app.editor
            .update(cx, |editor, cx| editor.replace_range(0..5, "Betty", cx));
        app.complete_pii_scan(7, identity, revision, result(), cx);
        assert!(app.pii.review.variants().is_empty());
        app.editor
            .update(cx, |editor, cx| editor.set_text("Alice", cx));
        let revision = app.editor.read(cx).revision();
        app.pii.job = Some(ScanJob {
            cancel: Arc::new(AtomicBool::new(false)),
            revision,
            generation: 9,
            config: settings::PiiConfig::default(),
            progress: Default::default(),
        });
        app.session.replace(Document::default());
        app.complete_pii_scan(9, identity, revision, result(), cx);
        assert!(app.pii.review.variants().is_empty());
        app.pii.cancel();
    });
}

fn prepare_applied(app: &mut DocumentView, source: &str, cx: &mut Context<DocumentView>) {
    app.editor.update(cx, |e, cx| e.set_text(source, cx));
    app.pii.reviewing = true;
    let detections: Vec<_> = source
        .match_indices("Anna")
        .chain(source.match_indices("Bob"))
        .map(|(at, text)| mdoc_pii::Detection {
            range: at..at + text.len(),
            category: Category::Person,
            score: 0.9,
            recognizer: mdoc_pii::Recognizer::Model,
        })
        .collect();
    app.pii.review.ingest(source, detections).unwrap();
    app.apply_replacements(cx);
    assert_eq!(app.pii.review.remaining(), 0);
    app.review_changed(cx);
}
#[gpui::test]
fn applied_highlights_restore_one_or_matching_originals_and_keep_survives_rescan(
    cx: &mut gpui::TestAppContext,
) {
    let (app, cx) = crate::document_view::tests::boot(cx);
    app.update(cx, |app, cx| prepare_applied(app, "Anna Bob Anna", cx));
    cx.run_until_parked();
    app.update_in(cx, |app, window, cx| {
        assert_eq!(app.editor.read(cx).text(), "PERSON_1 PERSON_2 PERSON_1");
        let id = app.pii.review.applied()[0].id;
        app.activate_annotation(APPLIED_ID | id, window, cx);
    });
    cx.run_until_parked();
    cx.update(|window, cx| window.draw(cx).clear(cx));
    assert!(cx.debug_bounds("pseudonym-original").is_some());
    app.update_in(cx, |app, window, cx| {
        app.pii.mapping.set_scope(mapping::Scope::Mention);
        app.keep_originals(window, cx)
    });
    cx.run_until_parked();
    app.update(cx, |app, cx| {
        assert_eq!(app.editor.read(cx).text(), "Anna PERSON_2 PERSON_1");
        app.pii
            .review
            .ingest(
                "Anna PERSON_2 PERSON_1",
                vec![mdoc_pii::Detection {
                    range: 0..4,
                    category: Category::Person,
                    score: 0.9,
                    recognizer: mdoc_pii::Recognizer::Model,
                }],
            )
            .unwrap();
        assert_eq!(app.pii.review.remaining(), 0);
    });
    cx.dispatch_action(mdoc_editor::Undo);
    cx.run_until_parked();
    app.update_in(cx, |app, window, cx| {
        assert_eq!(app.editor.read(cx).text(), "PERSON_1 PERSON_2 PERSON_1");
        let id = app.pii.review.applied()[0].id;
        app.activate_annotation(APPLIED_ID | id, window, cx);
        app.pii.mapping.set_scope(mapping::Scope::Entity);
        app.keep_originals(window, cx);
    });
    cx.run_until_parked();
    app.update(cx, |app, cx| {
        assert_eq!(app.editor.read(cx).text(), "Anna PERSON_2 Anna");
        assert_eq!(app.pii.review.applied().len(), 1);
        assert_eq!(app.pii.review.applied()[0].step.before.as_ref(), "Bob");
    });
    cx.dispatch_action(mdoc_editor::Undo);
    cx.run_until_parked();
    cx.dispatch_action(mdoc_editor::Redo);
    cx.run_until_parked();
    app.update(cx, |app, cx| {
        assert_eq!(app.editor.read(cx).text(), "Anna PERSON_2 Anna")
    });
}

#[gpui::test]
fn compact_restoration_buttons_preserve_matching_scope_in_both_themes(
    cx: &mut gpui::TestAppContext,
) {
    let (app, cx) = crate::document_view::tests::boot(cx);
    for theme in [Theme::Dark, Theme::Light] {
        for (width, height) in [(1100., 760.), (640., 480.)] {
            cx.simulate_resize(gpui::size(px(width), px(height)));
            app.update(cx, |app, cx| {
                app.theme.set(theme);
                app.pii.review = Default::default();
                prepare_applied(app, "Anna Bob Anna", cx);
            });
            cx.run_until_parked();
            app.update_in(cx, |app, window, cx| {
                let id = app.pii.review.applied()[0].id;
                app.activate_annotation(APPLIED_ID | id, window, cx);
            });
            cx.run_until_parked();
            cx.update(|window, cx| window.draw(cx).clear(cx));
            let panel = cx.debug_bounds("pseudonym-popup").unwrap();
            for id in ["pseudonym-original", "direct-alias", "direct-keep"] {
                let control = cx
                    .debug_bounds(id)
                    .unwrap_or_else(|| panic!("missing {id}"));
                assert!(
                    control.left() >= panel.left()
                        && control.right() <= panel.right()
                        && control.top() >= panel.top()
                        && control.bottom() <= panel.bottom(),
                    "{id} at {width}x{height} {theme:?}: control={control:?}, panel={panel:?}"
                );
            }
            assert!(panel.bottom() <= px(height) && panel.right() <= px(width));
            // Neutral popup focus must never turn Enter into restoration.
            cx.simulate_keystrokes("enter");
            cx.run_until_parked();
            app.update(cx, |app, cx| {
                assert_eq!(app.editor.read(cx).text(), "PERSON_1 PERSON_2 PERSON_1");
                assert!(app.pii.popup.is_some());
            });
            // Click the actual secondary action, rather than calling its handler.
            let restore_all = cx.debug_bounds("direct-keep").unwrap().center();
            cx.simulate_click(restore_all, Default::default());
            cx.run_until_parked();
            app.update(cx, |app, cx| {
                assert_eq!(app.editor.read(cx).text(), "Anna PERSON_2 Anna");
                assert!(app.pii.popup.is_some() && app.active_is_kept());
            });
            cx.simulate_keystrokes("escape");
            cx.run_until_parked();
            app.update_in(cx, |app, window, cx| {
                let id = app.pii.review.applied()[0].id;
                app.activate_annotation(APPLIED_ID | id, window, cx);
            });
            cx.run_until_parked();
            cx.update(|window, cx| window.draw(cx).clear(cx));
            assert!(cx.debug_bounds("direct-applied").is_some());
            cx.simulate_keystrokes("escape");
            cx.run_until_parked();
            app.update_in(cx, |app, window, cx| {
                assert!(app.pii.popup.is_none());
                assert!(app.editor.read(cx).focus_handle(cx).is_focused(window));
            });
        }
    }
}

#[gpui::test]
fn long_original_enter_applies_this_one_and_alias_enter_only_confirms(
    cx: &mut gpui::TestAppContext,
) {
    let (app, cx) = crate::document_view::tests::boot(cx);
    cx.simulate_resize(gpui::size(px(640.), px(480.)));
    let original = "Индивидуальный предприниматель Анна Александровна ".repeat(5);
    for theme in [Theme::Dark, Theme::Light] {
        app.update_in(cx, |app, window, cx| {
            app.theme.set(theme);
            app.editor.update(cx, |e, cx| e.set_text(&original, cx));
            app.pii.review = Review::default();
            app.pii.reviewing = true;
            app.pii
                .review
                .add_manual(&original, 0..original.len(), Category::Person)
                .unwrap();
            app.review_changed(cx);
            app.activate_annotation(app.pii.review.candidates()[0].id, window, cx);
        });
        cx.run_until_parked();
        // Enter applies this mention (ADR 0033), as one undo step.
        cx.simulate_keystrokes("enter");
        cx.run_until_parked();
        assert_eq!(
            app.read_with(cx, |app, cx| app.editor.read(cx).text().to_owned()),
            "PERSON_1"
        );
        app.update_in(cx, |app, window, cx| {
            app.editor.update(cx, |e, cx| e.undo_step(window, cx))
        });
        cx.run_until_parked();
        assert_eq!(
            app.read_with(cx, |app, cx| app.editor.read(cx).text().to_owned()),
            original
        );
        app.update_in(cx, |app, window, cx| {
            let candidate = app.pii.review.candidates()[0].id;
            app.activate_annotation(candidate, window, cx);
            app.pii
                .mapping
                .alias
                .update(cx, |input, cx| input.set_value("CLIENT_1".into(), cx));
            window.focus(&app.pii.mapping.alias.read(cx).focus_handle(cx), cx);
        });
        cx.simulate_keystrokes("enter");
        cx.run_until_parked();
        assert_eq!(
            app.read_with(cx, |app, cx| app.editor.read(cx).text().to_owned()),
            original
        );
        cx.update(|window, cx| window.draw(cx).clear(cx));
        let apply = cx.debug_bounds("primary-apply").unwrap();
        cx.simulate_click(apply.center(), Default::default());
        cx.run_until_parked();
        app.update_in(cx, |app, window, cx| {
            assert_eq!(app.editor.read(cx).text(), "CLIENT_1");
            assert!(app.pii.popup.is_none());
            // Enter on the applied mention changes nothing.
            let applied = app.pii.review.applied()[0].id;
            app.activate_annotation(APPLIED_ID | applied, window, cx);
            window.focus(&app.pii.focus, cx);
        });
        cx.run_until_parked();
        cx.update(|window, cx| window.draw(cx).clear(cx));
        cx.simulate_keystrokes("enter");
        cx.run_until_parked();
        assert_eq!(
            app.read_with(cx, |app, cx| app.editor.read(cx).text().to_owned()),
            "CLIENT_1"
        );
    }
}

#[gpui::test]
fn editing_alias_invalidates_provenance_undo_recovers_it_and_paste_creates_none(
    cx: &mut gpui::TestAppContext,
) {
    let (app, cx) = crate::document_view::tests::boot(cx);
    app.update(cx, |app, cx| prepare_applied(app, "Anna", cx));
    cx.run_until_parked();
    app.update(cx, |app, cx| {
        app.editor
            .update(cx, |e, cx| e.replace_range(0..0, "😀 ", cx))
    });
    cx.run_until_parked();
    let id = app.read_with(cx, |app, _| app.pii.review.applied()[0].id);
    app.update(cx, |app, cx| {
        assert_eq!(app.pii.review.applied_occurrence(id).unwrap().range, 5..13);
        app.editor
            .update(cx, |e, cx| e.replace_range(6..6, "X", cx));
    });
    cx.run_until_parked();
    assert!(app.read_with(cx, |app, _| { app.pii.review.applied().is_empty() }));
    cx.dispatch_action(mdoc_editor::Undo);
    cx.run_until_parked();
    app.update(cx, |app, cx| {
        assert_eq!(app.pii.review.applied_occurrence(id).unwrap().range, 5..13);
        app.editor
            .update(cx, |e, cx| e.replace_range(13..13, " PERSON_1", cx));
    });
    cx.run_until_parked();
    app.update(cx, |app, cx| {
        assert_eq!(app.editor.read(cx).text(), "😀 PERSON_1 PERSON_1");
        assert_eq!(app.pii.review.applied().len(), 1);
    });
}
#[gpui::test]
fn dense_hidden_fields_choose_and_restore_by_keyboard(cx: &mut gpui::TestAppContext) {
    let (app, cx) = crate::document_view::tests::boot(cx);
    app.update(cx, |app, cx| {
        prepare_applied(app, "intro\n\n[link](https://x.invalid/Anna/Bob)", cx)
    });
    cx.run_until_parked();
    cx.update(|window, cx| window.draw(cx).clear(cx));
    let bounds = app.read_with(cx, |app, cx| {
        let applied = &app.pii.review.applied();
        let a = app
            .editor
            .read(cx)
            .annotation_bounds(APPLIED_ID | applied[0].id)
            .unwrap();
        let b = app
            .editor
            .read(cx)
            .annotation_bounds(APPLIED_ID | applied[1].id)
            .unwrap();
        assert_eq!(a, b);
        a
    });
    cx.simulate_click(bounds.center(), Default::default());
    cx.run_until_parked();
    cx.update(|window, cx| window.draw(cx).clear(cx));
    assert!(cx.debug_bounds("pii-field-chooser").is_some());
    cx.simulate_keystrokes("down enter");
    cx.run_until_parked();
    app.update_in(cx, |app, window, cx| {
        app.pii.mapping.set_scope(mapping::Scope::Mention);
        app.keep_originals(window, cx)
    });
    cx.run_until_parked();
    app.update(cx, |app, cx| {
        assert_eq!(
            app.editor.read(cx).text(),
            "intro\n\n[link](https://x.invalid/PERSON_1/Bob)"
        )
    });
}

#[gpui::test]
fn hidden_annotations_follow_visible_wrapped_rows(cx: &mut gpui::TestAppContext) {
    let (app, cx) = crate::document_view::tests::boot(cx);
    let source = format!(
        "{}\n\nend",
        "Пример [visible_label_with_words](https://x.invalid/Anna/Bob) ".repeat(1000)
    );
    let annotations: Vec<_> = source
        .match_indices("Anna")
        .enumerate()
        .map(|(id, (at, _))| mdoc_editor::SourceAnnotation {
            id: id as u64,
            range: at..at + 4,
            color: gpui::rgba(0xffaa0022).into(),
            active_color: gpui::rgba(0xffaa0055).into(),
            border: gpui::transparent_black(),
            text_color: None,
            underline: None,
            dotted: false,
        })
        .collect();
    app.update(cx, |app, cx| {
        app.editor.update(cx, |editor, cx| {
            editor.set_text(source.clone(), cx);
            editor.set_cursor(source.len(), cx);
            editor.set_annotations(editor.revision(), annotations, cx);
        })
    });
    for _ in 0..2 {
        cx.update(|window, cx| window.draw(cx).clear(cx));
    }
    app.update(cx, |app, _| {
        app.scroll
            .set_offset(gpui::point(px(0.), -app.scroll.max_offset().y / 2.))
    });
    for _ in 0..2 {
        cx.update(|window, cx| window.draw(cx).clear(cx));
    }
    app.read_with(cx, |app, cx| {
        let viewport = app.scroll.bounds();
        let editor = app.editor.read(cx);
        let visible: Vec<_> = (0..1000)
            .filter_map(|id| editor.annotation_bounds(id).map(|bounds| (id, bounds)))
            .collect();
        assert!(!visible.is_empty() && visible.len() < 300);
        assert!(
            visible
                .iter()
                .any(|(_, bounds)| bounds.top() >= viewport.top()
                    && bounds.bottom() <= viewport.bottom()),
            "hidden markers must follow their visible wrap rows"
        );
        assert!(
            visible
                .iter()
                .all(|(id, _)| editor.annotation_is_hidden(*id))
        );
        assert!(
            editor.annotation_bounds(0).is_none(),
            "offscreen hidden spans are not gutter markers"
        );
    });
}
#[gpui::test]
fn selection_replace_supersedes_partial_proposals_and_enter_applies(cx: &mut gpui::TestAppContext) {
    let (app, cx) = crate::document_view::tests::boot(cx);
    let source = "Contact Иван Петров today. Иван agreed.";
    app.update(cx, |app, cx| {
        app.editor
            .update(cx, |editor, cx| editor.set_text(source, cx));
    });
    cx.run_until_parked();
    let first = source.find("Иван").unwrap();
    let full = first..source.find(" today").unwrap();
    let select = |range: Range<usize>, cx: &mut gpui::VisualTestContext| {
        app.update(cx, |app, cx| {
            app.editor.update(cx, |e, cx| e.set_selection(range, cx))
        });
        cx.run_until_parked();
        app.read_with(cx, |app, cx| {
            app.editor.read(cx).selection_action().cloned()
        })
    };
    // Not offered before Pseudonymize has run.
    assert!(select(full.clone(), cx).is_none());
    app.update(cx, |app, cx| {
        let (generation, identity, revision) = install_scan(app, cx);
        app.complete_pii_scan(
            generation,
            identity,
            revision,
            Ok(vec![mdoc_pii::Detection {
                range: first..first + "Иван".len(),
                category: Category::Person,
                score: 0.9,
                recognizer: mdoc_pii::Recognizer::Model,
            }]),
            cx,
        );
        assert_eq!(app.pii.review.remaining(), 2);
    });
    cx.run_until_parked();
    // A partial word explains itself instead of adding.
    let action = select(first..first + 4, cx).unwrap();
    assert_eq!(action.disabled.as_deref(), Some("Select whole words."));
    // Edge whitespace and punctuation are trimmed; the pill offers Replace.
    let action = select(first - 1..full.end + 1, cx).unwrap();
    assert_eq!(action.disabled, None);
    assert_eq!(action.label.as_ref(), "Replace");
    app.update_in(cx, |app, window, cx| {
        app.add_pii_candidate(&PiiAddCandidate, window, cx);
        let review = &app.pii.review;
        let added = review
            .variants()
            .iter()
            .find(|v| v.original.as_ref() == "Иван Петров")
            .unwrap();
        assert_eq!(added.category, Category::Person);
        assert_eq!(added.mentions, vec![full.clone()]);
        assert_eq!(review.remaining(), 2);
        assert!(app.pii.popup.is_some() && app.pii.enter_applies);
        assert!(app.pii.mapping.open);
        assert!(app.pii.focus.is_focused(window));
    });
    cx.dispatch_action(PiiConfirm);
    cx.run_until_parked();
    app.update(cx, |app, cx| {
        // Marking the full name linked the scan's lone "Иван" to it (ADR
        // 0092), so Apply replaces both with its alias.
        let text = app.editor.read(cx).text().to_owned();
        assert_eq!(text, "Contact PERSON_2 today. PERSON_2 agreed.");
        assert_eq!(app.pii.review.remaining(), 0);
    });
    // Apply and the addition are separate undo steps.
    app.update_in(cx, |app, window, cx| {
        window.focus(&app.editor.read(cx).focus_handle(cx), cx)
    });
    cx.dispatch_action(mdoc_editor::Undo);
    cx.run_until_parked();
    app.update(cx, |app, cx| {
        assert_eq!(app.editor.read(cx).text(), source);
        assert_eq!(app.pii.review.candidates()[0].range, full);
    });
    cx.dispatch_action(mdoc_editor::Undo);
    cx.run_until_parked();
    app.update(cx, |app, cx| {
        assert_eq!(app.editor.read(cx).text(), source);
        let first = &app.pii.review.candidates()[0].range;
        assert_eq!(first.len(), "Иван".len());
    });
}
#[gpui::test]
fn unknown_wording_is_other_cued_and_cancel_reverts_addition_and_apply(
    cx: &mut gpui::TestAppContext,
) {
    let (app, cx) = crate::document_view::tests::boot(cx);
    let source = "Планируемая дата поступления. Анна agreed.";
    app.update(cx, |app, cx| {
        app.editor
            .update(cx, |editor, cx| editor.set_text(source, cx));
    });
    cx.run_until_parked();
    let anna = source.find("Анна").unwrap();
    app.update(cx, |app, cx| {
        let (generation, identity, revision) = install_scan(app, cx);
        app.complete_pii_scan(
            generation,
            identity,
            revision,
            Ok(vec![mdoc_pii::Detection {
                range: anna..anna + "Анна".len(),
                category: Category::Person,
                score: 0.9,
                recognizer: mdoc_pii::Recognizer::Model,
            }]),
            cx,
        );
    });
    cx.run_until_parked();
    let date = 0.."Планируемая дата".len();
    app.update(cx, |app, cx| {
        app.editor
            .update(cx, |e, cx| e.set_selection(date.clone(), cx))
    });
    cx.run_until_parked();
    app.update_in(cx, |app, window, cx| {
        app.add_pii_candidate(&PiiAddCandidate, window, cx);
        let added = app
            .pii
            .review
            .variants()
            .iter()
            .find(|v| v.original.as_ref() == "Планируемая дата")
            .unwrap();
        assert_eq!(added.category, Category::Other);
        assert_eq!(added.replacement, "REDACTED_1");
        // A fallback guess cues the alias and the category chip.
        assert!(matches!(app.pii.mapping.cue, Some((_, true))));
        let history = app.editor.read(cx).history_id();
        assert_eq!(app.pii.mapping.added_at(history).map(|(_, n)| n), Some(1));
    });
    cx.dispatch_action(PiiConfirm);
    cx.run_until_parked();
    app.update_in(cx, |app, window, cx| {
        assert!(
            app.editor
                .read(cx)
                .text()
                .starts_with("REDACTED_1 поступления.")
        );
        let history = app.editor.read(cx).history_id();
        assert_eq!(app.pii.mapping.added_at(history).map(|(_, n)| n), Some(2));
        app.cancel_addition(window, cx);
        assert!(app.pii.popup.is_none());
    });
    cx.run_until_parked();
    app.update(cx, |app, cx| {
        assert_eq!(app.editor.read(cx).text(), source);
        assert_eq!(app.pii.review.remaining(), 1);
        let history = app.editor.read(cx).history_id();
        assert!(app.pii.mapping.added_at(history).is_none());
    });
}

#[gpui::test]
fn decision_card_fits_the_mention_and_apply_settles_in(cx: &mut gpui::TestAppContext) {
    let (app, cx) = crate::document_view::tests::boot(cx);
    let source = "Планируемая дата поступления. Анна agreed.";
    app.update(cx, |app, cx| {
        app.editor
            .update(cx, |editor, cx| editor.set_text(source, cx));
    });
    cx.run_until_parked();
    let anna = source.find("Анна").unwrap();
    app.update(cx, |app, cx| {
        install_scan(app, cx);
    });
    // The toolbar stage label shows only while the scan runs (ADR 0032).
    cx.update(|window, cx| window.draw(cx).clear(cx));
    assert!(cx.debug_bounds("scan-status").is_some());
    app.update(cx, |app, cx| {
        let job = app.pii.job.as_ref().unwrap();
        let (generation, revision) = (job.generation, job.revision);
        let identity = app.session.generation;
        app.complete_pii_scan(
            generation,
            identity,
            revision,
            Ok(vec![mdoc_pii::Detection {
                range: anna..anna + "Анна".len(),
                category: Category::Person,
                score: 0.9,
                recognizer: mdoc_pii::Recognizer::Model,
            }]),
            cx,
        );
        // Proposals fade in as the sweep stops.
        let id = app.pii.review.candidates()[0].id;
        assert!(app.editor.read(cx).is_flashing(id));
    });
    cx.update(|window, cx| window.draw(cx).clear(cx));
    assert!(cx.debug_bounds("scan-status").is_none());
    app.update(cx, |app, cx| {
        app.editor
            .update(cx, |e, cx| e.set_selection(0.."Планируемая дата".len(), cx))
    });
    cx.run_until_parked();
    app.update_in(cx, |app, window, cx| {
        app.add_pii_candidate(&PiiAddCandidate, window, cx)
    });
    cx.update(|window, cx| window.draw(cx).clear(cx));
    // A fresh addition is backed out with Cancel, not kept; one mention needs
    // neither navigation nor a scope, and Other has no owner.
    assert!(cx.debug_bounds("direct-cancel-addition").is_some());
    assert!(cx.debug_bounds("direct-keep").is_none());
    assert!(cx.debug_bounds("direct-prev").is_none());
    assert!(cx.debug_bounds("scope-0").is_none());
    assert!(cx.debug_bounds("direct-owner").is_none());
    cx.dispatch_action(PiiConfirm);
    cx.run_until_parked();
    app.update(cx, |app, cx| {
        let applied = app.pii.review.applied()[0].id;
        assert!(app.editor.read(cx).is_flashing(APPLIED_ID | applied));
        // The words roll from the original into the alias.
        assert!(app.editor.read(cx).is_rolling_text());
    });
    cx.update(|window, cx| window.draw(cx).clear(cx));
    assert!(cx.debug_bounds("direct-applied").is_some());
    assert!(cx.debug_bounds("direct-cancel-addition").is_some());
    // Undo eases the restored proposal back to amber.
    app.update(cx, |app, cx| {
        let applied = app.pii.review.applied()[0].id;
        app.undo_replacements([applied].into(), cx);
        let proposal = app
            .pii
            .review
            .candidates()
            .iter()
            .find(|c| c.range.start == 0)
            .unwrap()
            .id;
        assert!(app.editor.read(cx).is_flashing(proposal));
    });
    // Keep original leaves a fading ghost where the chip was.
    app.update_in(cx, |app, window, cx| {
        let proposal = app
            .pii
            .review
            .candidates()
            .iter()
            .find(|c| c.range.start == 0)
            .unwrap()
            .id;
        app.activate_annotation(proposal, window, cx);
        app.keep_originals(window, cx);
        assert!(app.editor.read(cx).is_fading_ranges());
    });
}
#[gpui::test]
fn toolbar_status_cancels_a_running_scan(cx: &mut gpui::TestAppContext) {
    let (app, cx) = crate::document_view::tests::boot(cx);
    app.update(cx, |app, cx| {
        app.editor
            .update(cx, |editor, cx| editor.set_text("Alice", cx));
    });
    cx.run_until_parked();
    app.update(cx, |app, cx| {
        install_scan(app, cx);
    });
    cx.update(|window, cx| window.draw(cx).clear(cx));
    let stop = cx.debug_bounds("cancel-pii-scan").unwrap().center();
    cx.simulate_click(stop, gpui::Modifiers::none());
    cx.run_until_parked();
    app.update(cx, |app, cx| {
        assert!(!app.pii.scanning());
        assert_eq!(app.editor.read(cx).text(), "Alice");
        // Nothing was found, so the next Pseudonymize scans afresh.
        assert!(!app.pii.reviewing && !app.pii.mapping.open);
    });
    cx.update(|window, cx| window.draw(cx).clear(cx));
    assert!(cx.debug_bounds("scan-status").is_none());
}

#[gpui::test]
fn alias_click_shows_a_caret_and_new_alias_below_the_card_applies(cx: &mut gpui::TestAppContext) {
    let source = "Tebriz Tagiev and Daniel Okafor. Agreement. Daniel Okafor at +44 20 7946 0958.";
    for scope in [
        mapping::Scope::Mention,
        mapping::Scope::Wording,
        mapping::Scope::Entity,
    ] {
        let (app, cx) = crate::document_view::tests::boot(cx);
        app.update(cx, |app, cx| {
            app.editor.update(cx, |e, cx| e.set_text(source, cx))
        });
        cx.run_until_parked();
        app.update(cx, |app, cx| {
            let (generation, identity, revision) = install_scan(app, cx);
            let det = |text: &str, nth: usize, category| {
                let at = source.match_indices(text).nth(nth).unwrap().0;
                mdoc_pii::Detection {
                    range: at..at + text.len(),
                    category,
                    score: 0.9,
                    recognizer: mdoc_pii::Recognizer::Model,
                }
            };
            app.complete_pii_scan(
                generation,
                identity,
                revision,
                Ok(vec![
                    det("Tebriz Tagiev", 0, Category::Person),
                    det("Daniel Okafor", 0, Category::Person),
                    det("Agreement", 0, Category::Person),
                    det("Daniel Okafor", 1, Category::Person),
                    det("+44 20 7946 0958", 0, Category::Phone),
                ]),
                cx,
            );
        });
        cx.run_until_parked();
        app.update_in(cx, |app, window, cx| {
            let daniel = source.find("Daniel").unwrap();
            let candidate = app
                .pii
                .review
                .candidates()
                .iter()
                .find(|c| c.range.start == daniel)
                .unwrap()
                .id;
            app.activate_annotation(candidate, window, cx);
            app.pii.mapping.set_scope(scope);
        });
        cx.run_until_parked();
        cx.update(|window, cx| window.draw(cx).clear(cx));
        // Clicking the alias leaves a caret, not a selection: typing appends.
        let field = cx.debug_bounds("direct-alias").unwrap();
        cx.simulate_click(
            gpui::point(field.right() - px(4.), field.center().y),
            Default::default(),
        );
        cx.run_until_parked();
        cx.simulate_input("x");
        cx.run_until_parked();
        app.update(cx, |app, cx| {
            assert_eq!(app.pii.mapping.alias.read(cx).value(), "PERSON_2x");
        });
        cx.simulate_keystrokes("backspace");
        cx.run_until_parked();
        // The menu hangs below the card; press and release it like a real
        // pointer, with a frame between, so focus may not shift the menu.
        cx.update(|window, cx| window.draw(cx).clear(cx));
        let card = cx.debug_bounds("pseudonym-popup").unwrap();
        let new_alias = cx.debug_bounds("direct-new-alias").unwrap().center();
        assert!(new_alias.y > card.bottom());
        cx.simulate_mouse_down(new_alias, gpui::MouseButton::Left, Default::default());
        cx.run_until_parked();
        cx.update(|window, cx| window.draw(cx).clear(cx));
        cx.simulate_mouse_up(new_alias, gpui::MouseButton::Left, Default::default());
        cx.run_until_parked();
        app.update(cx, |app, cx| {
            assert!(app.pii.popup.is_some());
            assert_eq!(app.pii.mapping.alias.read(cx).value(), "PERSON_4");
        });
    }
}

#[gpui::test]
fn renaming_an_alias_back_frees_its_old_name(cx: &mut gpui::TestAppContext) {
    let source = "Tebriz Tagiev and Daniel Okafor of Acme. Daniel Okafor at +44 20 7946 0958.";
    for scope in [
        mapping::Scope::Mention,
        mapping::Scope::Wording,
        mapping::Scope::Entity,
    ] {
        let (app, cx) = crate::document_view::tests::boot(cx);
        app.update(cx, |app, cx| {
            app.editor.update(cx, |e, cx| e.set_text(source, cx))
        });
        cx.run_until_parked();
        app.update(cx, |app, cx| {
            let (generation, identity, revision) = install_scan(app, cx);
            let det = |text: &str, nth: usize, category| {
                let at = source.match_indices(text).nth(nth).unwrap().0;
                mdoc_pii::Detection {
                    range: at..at + text.len(),
                    category,
                    score: 0.9,
                    recognizer: mdoc_pii::Recognizer::Model,
                }
            };
            app.complete_pii_scan(
                generation,
                identity,
                revision,
                Ok(vec![
                    det("Tebriz Tagiev", 0, Category::Person),
                    det("Daniel Okafor", 0, Category::Person),
                    det("Daniel Okafor", 1, Category::Person),
                    det("+44 20 7946 0958", 0, Category::Phone),
                ]),
                cx,
            );
        });
        cx.run_until_parked();
        let rename = |app: &mut DocumentView,
                      nth: usize,
                      alias: &str,
                      window: &mut Window,
                      cx: &mut Context<DocumentView>| {
            let at = source.match_indices("Daniel").nth(nth).unwrap().0;
            let candidate = app
                .pii
                .review
                .candidates()
                .iter()
                .find(|c| c.range.start == at)
                .unwrap()
                .id;
            app.activate_annotation(candidate, window, cx);
            app.pii.mapping.set_scope(scope);
            app.pii
                .mapping
                .alias
                .update(cx, |input, cx| input.set_value(alias.into(), cx));
            app.confirm_alias(false, mapping::Applying::Nothing, cx)
        };
        app.update_in(cx, |app, window, cx| {
            assert!(rename(app, 0, "PERSON_3", window, cx));
            if scope == mapping::Scope::Mention {
                // The other mention joins the renamed entity.
                let target = app
                    .pii
                    .review
                    .active_identities()
                    .into_iter()
                    .find(|&i| app.pii.review.identity(i).unwrap().alias == "PERSON_3")
                    .unwrap();
                let at = source.match_indices("Daniel").nth(1).unwrap().0;
                let candidate = app
                    .pii
                    .review
                    .candidates()
                    .iter()
                    .find(|c| c.range.start == at)
                    .unwrap()
                    .id;
                app.activate_annotation(candidate, window, cx);
                app.link_to_entity(target, cx);
            }
        });
        cx.run_until_parked();
        app.update_in(cx, |app, window, cx| {
            let renamed = rename(app, 0, "PERSON_2", window, cx);
            let aliases: Vec<_> = app
                .pii
                .review
                .active_identities()
                .into_iter()
                .map(|i| app.pii.review.identity(i).unwrap().alias.clone())
                .collect();
            assert!(renamed, "scope {}: {aliases:?}", scope as u8);
            let mut unique = aliases.clone();
            unique.sort();
            unique.dedup();
            assert_eq!(unique.len(), aliases.len(), "{aliases:?}");
            assert!(aliases.contains(&"PERSON_2".to_string()));
        });
    }
}

#[gpui::test]
fn panel_undo_redo_and_after_action_notice(cx: &mut gpui::TestAppContext) {
    let (app, cx) = crate::document_view::tests::boot(cx);
    app.update(cx, |app, cx| {
        app.editor
            .update(cx, |e, cx| e.set_text("Anna Bob Anna", cx))
    });
    cx.run_until_parked();
    app.update(cx, |app, cx| {
        let (generation, identity, revision) = install_scan(app, cx);
        let det = |range| mdoc_pii::Detection {
            range,
            category: Category::Person,
            score: 0.9,
            recognizer: mdoc_pii::Recognizer::Model,
        };
        app.complete_pii_scan(
            generation,
            identity,
            revision,
            Ok(vec![det(0..4), det(5..8), det(9..13)]),
            cx,
        );
    });
    cx.run_until_parked();
    cx.update(|window, cx| window.draw(cx).clear(cx));
    assert!(cx.debug_bounds("review-rescan").is_some());
    let apply = cx.debug_bounds("primary-apply").unwrap().center();
    cx.simulate_click(apply, Default::default());
    cx.run_until_parked();
    cx.update(|window, cx| window.draw(cx).clear(cx));
    app.update(cx, |app, cx| {
        assert_eq!(app.editor.read(cx).text(), "PERSON_1 PERSON_2 PERSON_1");
        // Inspector (ADR 0048, C): back to Review for the notice.
        app.inspector_share = false;
        cx.notify();
    });
    cx.update(|window, cx| window.draw(cx).clear(cx));
    // The notice offers Undo, then times out.
    let notice = cx.debug_bounds("replacement-notice-undo").unwrap().center();
    cx.simulate_click(notice, Default::default());
    cx.run_until_parked();
    app.update(cx, |app, cx| {
        assert_eq!(app.editor.read(cx).text(), "Anna Bob Anna")
    });
    // Redo and undo by keyboard while the panel, not the editor, has focus.
    app.update_in(cx, |app, window, cx| {
        window.focus(&app.pii.mapping.focus, cx);
    });
    cx.simulate_keystrokes("secondary-shift-z");
    cx.run_until_parked();
    app.update(cx, |app, cx| {
        assert_eq!(app.editor.read(cx).text(), "PERSON_1 PERSON_2 PERSON_1")
    });
    cx.simulate_keystrokes("secondary-z");
    cx.run_until_parked();
    app.update(cx, |app, cx| {
        assert_eq!(app.editor.read(cx).text(), "Anna Bob Anna")
    });
    cx.executor()
        .advance_clock(std::time::Duration::from_secs(7));
    cx.run_until_parked();
    cx.update(|window, cx| window.draw(cx).clear(cx));
    assert!(cx.debug_bounds("replacement-notice-undo").is_none());
}

#[gpui::test]
fn popup_delete_keeps_but_alias_field_backspace_edits(cx: &mut gpui::TestAppContext) {
    let (app, cx) = crate::document_view::tests::boot(cx);
    app.update_in(cx, |app, window, cx| {
        app.editor.update(cx, |e, cx| e.set_text("Anna Bob", cx));
        app.pii.review = Review::default();
        app.pii.reviewing = true;
        app.pii
            .review
            .ingest(
                "Anna Bob",
                vec![mdoc_pii::Detection {
                    range: 0..4,
                    category: Category::Person,
                    score: 0.9,
                    recognizer: mdoc_pii::Recognizer::Model,
                }],
            )
            .unwrap();
        app.review_changed(cx);
        app.activate_annotation(app.pii.review.candidates()[0].id, window, cx);
        window.focus(&app.pii.mapping.alias.read(cx).focus_handle(cx), cx);
    });
    cx.run_until_parked();
    cx.simulate_keystrokes("backspace");
    cx.run_until_parked();
    app.update_in(cx, |app, window, cx| {
        // The selected alias text was deleted; nothing was kept.
        assert_eq!(app.pii.mapping.alias.read(cx).value(), "");
        assert_eq!(app.pii.review.remaining(), 1);
        let alias = "PERSON_1".to_string();
        app.pii
            .mapping
            .alias
            .update(cx, |input, cx| input.set_value(alias, cx));
        window.focus(&app.pii.focus, cx);
    });
    cx.simulate_keystrokes("backspace");
    cx.run_until_parked();
    app.update(cx, |app, cx| {
        assert_eq!(app.pii.review.remaining(), 0);
        assert_eq!(app.editor.read(cx).text(), "Anna Bob");
    });
}

#[gpui::test]
fn unrecognized_placeholders_are_assigned_or_left_never_guessed(cx: &mut gpui::TestAppContext) {
    use mdoc_pii::restore::ExportedIdentity;
    let (app, cx) = crate::document_view::tests::boot(cx);
    let reply = restore::RestoredReply {
        text: "PERSON_1 and PERSON_2 met PERSON_2 and ORG_5.".into(),
        identities: vec![ExportedIdentity {
            alias: "PERSON_1".into(),
            previous: Vec::new(),
            category: Category::Person,
            custom: false,
            originals: vec!["Anna Petrova".into()],
        }],
        source_name: "contract".into(),
        case: None,
    };
    app.update_in(cx, |app, window, cx| app.start_restored(&reply, window, cx));
    cx.run_until_parked();
    app.update_in(cx, |app, window, cx| {
        assert_eq!(
            app.editor.read(cx).text(),
            "Anna Petrova and PERSON_2 met PERSON_2 and ORG_5."
        );
        let tokens = app.unresolved_tokens(cx);
        assert_eq!(tokens.len(), 3);
        app.activate_annotation(restore::UNRESOLVED_ID | tokens[0].start as u64, window, cx);
    });
    // The picker keeps the focus it shares with the mention popup (#79).
    cx.update(|window, cx| window.draw(cx).clear(cx));
    app.update_in(cx, |app, window, cx| {
        assert!(app.pii.focus.is_focused(window));
        app.assign_unresolved(0, window, cx);
    });
    cx.run_until_parked();
    app.update_in(cx, |app, window, cx| {
        assert_eq!(
            app.editor.read(cx).text(),
            "Anna Petrova and Anna Petrova met PERSON_2 and ORG_5.",
            "only the chosen token changes; nothing is guessed for its repeat"
        );
        // The paste's restore and the one assigned token: two decisions.
        let path = app.editor.read(cx).history_path();
        let labels: Vec<_> = app
            .pii
            .decisions()
            .rows(&path)
            .map(|(e, _)| e.label())
            .collect();
        assert_eq!(
            labels,
            ["Restored “Anna Petrova”", "Restored “Anna Petrova”"]
        );
        assert_eq!(
            app.pii.review.remaining(),
            2,
            "both now proposed as PERSON_1"
        );
        let tokens = app.unresolved_tokens(cx);
        assert_eq!(tokens.len(), 2);
        app.activate_annotation(restore::UNRESOLVED_ID | tokens[1].start as u64, window, cx);
        app.leave_unresolved(window, cx);
        assert_eq!(app.unresolved_tokens(cx).len(), 1, "ORG_5 is left as is");
        assert!(app.pii.restore.as_ref().unwrap().map.identities().len() == 1);
    });
}

/// A Restore tab holds originals: Copy Markdown and the editor's own copies
/// are tagged restored text, until a conversion replaces the tab's text.
#[gpui::test]
fn restore_tab_copies_are_restored_text(cx: &mut gpui::TestAppContext) {
    use crate::restored_clipboard::{TAG, is_restored};
    use mdoc_pii::restore::ExportedIdentity;
    let (app, cx) = crate::document_view::tests::boot(cx);
    app.update_in(cx, |app, window, cx| {
        app.copy_markdown(&CopyMarkdown, window, cx);
        assert!(!is_restored(&cx.read_from_clipboard().unwrap()));
        assert_eq!(app.editor.read(cx).copy_metadata(), None);
    });
    // An ordinary document's before/after shows its own text: normal copies.
    app.update_in(cx, |app, window, cx| {
        app.editor
            .update(cx, |editor, cx| editor.set_text("Anna Petrova signed.", cx));
        app.pii.review.seed(
            &[ExportedIdentity {
                alias: "PERSON_1".into(),
                previous: Vec::new(),
                category: Category::Person,
                custom: false,
                originals: vec!["Anna Petrova".into()],
            }],
            "Anna Petrova signed.",
        );
        app.pii.reviewing = true;
        app.apply_all_pii(&PiiApplyAll, window, cx);
        app.set_view_mode(Some(super::compare::CompareMode::Side), window, cx);
        let compare = app.compare.as_ref().expect("before/after opens");
        for column in compare.columns() {
            assert_eq!(column.read(cx).copy_metadata(), None);
        }
        app.set_view_mode(None, window, cx);
        app.reset_pii(cx);
    });
    let reply = restore::RestoredReply {
        text: "PERSON_1 signed.".into(),
        identities: vec![ExportedIdentity {
            alias: "PERSON_1".into(),
            previous: Vec::new(),
            category: Category::Person,
            custom: false,
            originals: vec!["Anna Petrova".into()],
        }],
        source_name: "contract".into(),
        case: None,
    };
    app.update_in(cx, |app, window, cx| app.start_restored(&reply, window, cx));
    cx.run_until_parked();
    app.update_in(cx, |app, window, cx| {
        app.copy_markdown(&CopyMarkdown, window, cx);
        let item = cx.read_from_clipboard().unwrap();
        assert_eq!(item.text().as_deref(), Some("Anna Petrova signed."));
        assert!(is_restored(&item));
        assert_eq!(app.editor.read(cx).copy_metadata(), Some(TAG));
        // Before/after shows restored originals in both columns.
        app.apply_all_pii(&PiiApplyAll, window, cx);
        app.set_view_mode(Some(super::compare::CompareMode::Side), window, cx);
        let compare = app.compare.as_ref().expect("before/after opens");
        for column in compare.columns() {
            assert_eq!(column.read(cx).copy_metadata(), Some(TAG));
        }
        app.set_view_mode(None, window, cx);
        app.reset_pii(cx);
        assert_eq!(app.editor.read(cx).copy_metadata(), None);
    });
}

#[test]
fn ai_notice_names_aliases_only() {
    let notice = mdoc_pii::handoff::ai_notice(&["PERSON_1", "ORG_2"]);
    assert!(notice.contains("such as PERSON_1 or ORG_2."));
    assert!(notice.ends_with("---\n\n"));
    assert!(mdoc_pii::handoff::ai_notice(&[]).contains("PERSON_1"));
}

#[gpui::test]
fn homonyms_keep_their_own_aliases_through_restore_and_apply(cx: &mut gpui::TestAppContext) {
    use mdoc_pii::restore::ExportedIdentity;
    let (app, cx) = crate::document_view::tests::boot(cx);
    let person = |alias: &str| ExportedIdentity {
        alias: alias.into(),
        previous: Vec::new(),
        category: Category::Person,
        custom: false,
        originals: vec!["John Smith".into()],
    };
    let reply = restore::RestoredReply {
        text: "PERSON_1 met PERSON_2, then PERSON_1 left.".into(),
        identities: vec![person("PERSON_1"), person("PERSON_2")],
        source_name: "contract".into(),
        case: None,
    };
    app.update_in(cx, |app, window, cx| app.start_restored(&reply, window, cx));
    cx.run_until_parked();
    app.update_in(cx, |app, window, cx| {
        assert_eq!(
            app.editor.read(cx).text(),
            "John Smith met John Smith, then John Smith left."
        );
        app.apply_all_pii(&PiiApplyAll, window, cx);
        assert_eq!(
            app.editor.read(cx).text(),
            "PERSON_1 met PERSON_2, then PERSON_1 left."
        );
    });
}

#[gpui::test]
fn scans_skip_categories_the_case_switched_off(cx: &mut gpui::TestAppContext) {
    let (app, cx) = crate::document_view::tests::boot(cx);
    app.update(cx, |app, cx| {
        app.editor
            .update(cx, |editor, cx| editor.set_text("Alice at 1 Main St", cx));
    });
    cx.run_until_parked();
    app.update(cx, |app, cx| {
        app.case.attach(Some(mdoc_store::cases::CaseLink {
            id: "a".into(),
            color: None,
            name: "Acme".into(),
        }));
        let mut skip = app.case.skip().clone();
        skip.insert(Category::Address);
        app.case.set_skip(skip);
        let (generation, identity, revision) = install_scan(app, cx);
        let detection = |range, category| mdoc_pii::Detection {
            range,
            category,
            score: 0.9,
            recognizer: mdoc_pii::Recognizer::Model,
        };
        app.complete_pii_scan(
            generation,
            identity,
            revision,
            Ok(vec![
                detection(0..5, Category::Person),
                detection(9..18, Category::Address),
            ]),
            cx,
        );
        let categories: Vec<_> = app
            .pii
            .review
            .variants()
            .iter()
            .map(|v| v.category)
            .collect();
        assert_eq!(categories, [Category::Person]);
    });
}

#[gpui::test]
fn caseless_scans_follow_the_default_and_a_joined_tab_its_case(cx: &mut gpui::TestAppContext) {
    let (app, cx) = crate::document_view::tests::boot(cx);
    let source = "Alice at 1 Main St";
    app.update(cx, |app, cx| {
        app.editor
            .update(cx, |editor, cx| editor.set_text(source, cx));
        // Settings › Detection: Address is off (ADR 0076).
        let mut prefs = app.preferences.borrow().current.clone().unwrap();
        prefs.categories.set(Category::Address, false);
        app.preferences.borrow_mut().current = Some(prefs);
    });
    cx.run_until_parked();
    let detection = |range, category| mdoc_pii::Detection {
        range,
        category,
        score: 0.9,
        recognizer: mdoc_pii::Recognizer::Model,
    };
    let scan = |app: &mut DocumentView, cx: &mut Context<DocumentView>| {
        let (generation, identity, revision) = install_scan(app, cx);
        app.complete_pii_scan(
            generation,
            identity,
            revision,
            Ok(vec![
                detection(0..5, Category::Person),
                detection(9..18, Category::Address),
            ]),
            cx,
        );
        app.pii
            .review
            .variants()
            .iter()
            .map(|v| v.category)
            .collect::<Vec<_>>()
    };
    app.update(cx, |app, cx| {
        assert_eq!(scan(app, cx), [Category::Person], "no case: the default");
    });
    // Once the tab joins a case, the case's list wins: this one detects all.
    app.update(cx, |app, cx| {
        app.pii.review = Default::default();
        app.case.attach(Some(mdoc_store::cases::CaseLink {
            id: "a".into(),
            color: None,
            name: "Acme".into(),
        }));
        app.case.set_skip(Default::default());
        assert_eq!(scan(app, cx), [Category::Person, Category::Address]);
        // A member ignores Settings changes; its case decides.
        app.default_categories_unticked(&[Category::Person].into(), cx);
        assert_eq!(app.pii.review.variants().len(), 2);
    });
}

#[gpui::test]
fn before_after_follows_replacements_live_and_escape_closes(cx: &mut gpui::TestAppContext) {
    let (app, cx) = crate::document_view::tests::boot(cx);
    app.update(cx, |app, cx| prepare_applied(app, "Anna Bob Anna", cx));
    cx.run_until_parked();
    app.update_in(cx, |app, window, cx| app.toggle_compare(window, cx));
    cx.update(|window, cx| window.draw(cx).clear(cx));
    assert!(cx.debug_bounds("compare-view").is_some());
    assert!(cx.debug_bounds("compare-label-original").is_some());
    assert!(cx.debug_bounds("close-compare").is_some());
    let texts = |app: &DocumentView, cx: &App| app.compare.as_ref().unwrap().texts(cx);
    app.update(cx, |app, cx| {
        assert_eq!(
            texts(app, cx),
            (
                "Anna Bob Anna".into(),
                "PERSON_1 PERSON_2 PERSON_1".into(),
                false
            )
        );
        // Reverting from the panel reaches the open view, with motion.
        let id = app.pii.review.applied()[0].id;
        app.undo_replacements([id].into(), cx);
    });
    cx.update(|window, cx| window.draw(cx).clear(cx));
    app.update(cx, |app, cx| {
        assert_eq!(
            texts(app, cx),
            (
                "Anna Bob Anna".into(),
                "Anna PERSON_2 PERSON_1".into(),
                true
            )
        );
        app.apply_replacements(cx);
    });
    cx.update(|window, cx| window.draw(cx).clear(cx));
    app.update(cx, |app, cx| {
        assert_eq!(texts(app, cx).1, "PERSON_1 PERSON_2 PERSON_1");
    });
    // Esc closes the view, also from inside a column.
    app.update_in(cx, |app, window, cx| {
        let focus = app.compare.as_ref().unwrap().current_focus(cx);
        window.focus(&focus, cx);
    });
    cx.simulate_keystrokes("escape");
    cx.update(|window, cx| window.draw(cx).clear(cx));
    app.update(cx, |app, _| assert!(app.compare.is_none()));
    // Focus handed back to the hidden editor (closing the panel) moves to
    // the view, so Esc still closes it.
    app.update_in(cx, |app, window, cx| {
        app.toggle_compare(window, cx);
        window.focus(&app.editor.read(cx).focus_handle(cx), cx);
    });
    cx.update(|window, cx| window.draw(cx).clear(cx));
    cx.simulate_keystrokes("escape");
    app.update(cx, |app, _| assert!(app.compare.is_none()));
    // With no replacements left, an open view returns to the editor.
    app.update_in(cx, |app, window, cx| app.toggle_compare(window, cx));
    app.update(cx, |app, cx| {
        let ids = app.pii.review.applied().iter().map(|a| a.id).collect();
        app.undo_replacements(ids, cx);
    });
    cx.update(|window, cx| window.draw(cx).clear(cx));
    app.update(cx, |app, _| assert!(app.compare.is_none()));
}

#[gpui::test]
fn view_switcher_picks_edit_original_pseudonymized_or_side_by_side(cx: &mut gpui::TestAppContext) {
    use crate::pii::ui::compare::CompareMode;
    let (app, cx) = crate::document_view::tests::boot(cx);
    cx.simulate_resize(gpui::size(px(1200.), px(800.)));
    let draw = |cx: &mut gpui::VisualTestContext| cx.update(|window, cx| window.draw(cx).clear(cx));
    draw(cx);
    // The pill always offers zoom; the views join once aliases are applied
    // (ADR 0048).
    assert!(cx.debug_bounds("zoom-reset").is_some());
    assert!(
        cx.debug_bounds("view-edit").is_none(),
        "views only with applied replacements"
    );
    app.update(cx, |app, cx| prepare_applied(app, "Anna Bob Anna", cx));
    cx.run_until_parked();
    draw(cx);
    let click = |cx: &mut gpui::VisualTestContext, id: &'static str| {
        draw(cx);
        let at = cx.debug_bounds(id).unwrap().center();
        cx.simulate_click(at, Default::default());
        cx.run_until_parked();
        draw(cx);
    };
    let mode = |app: &Entity<DocumentView>, cx: &mut gpui::VisualTestContext| {
        app.read_with(cx, |app, _| app.compare.as_ref().map(|s| s.mode))
    };
    click(cx, "view-original");
    assert_eq!(mode(&app, cx), Some(CompareMode::Original));
    assert!(cx.debug_bounds("compare-label-original").is_some());
    assert!(cx.debug_bounds("compare-label-current").is_none());
    click(cx, "view-pseudonymized");
    assert_eq!(mode(&app, cx), Some(CompareMode::Pseudonymized));
    assert!(cx.debug_bounds("compare-label-original").is_none());
    assert!(cx.debug_bounds("compare-label-current").is_some());
    click(cx, "view-side");
    assert_eq!(mode(&app, cx), Some(CompareMode::Side));
    assert!(cx.debug_bounds("compare-label-original").is_some());
    assert!(cx.debug_bounds("compare-label-current").is_some());
    click(cx, "view-edit");
    assert_eq!(mode(&app, cx), None);
    assert!(cx.debug_bounds("compare-view").is_none());
    // The toolbar toggle still opens Side by side.
    app.update_in(cx, |app, window, cx| app.toggle_compare(window, cx));
    assert_eq!(mode(&app, cx), Some(CompareMode::Side));
    // The View menu's items reach the same views; Side by Side (⌘\) from
    // another view switches to it, and from itself goes back to Edit.
    use crate::document_view::{
        ShowEditView, ShowOriginalView, ShowPseudonymizedView, ToggleCompare,
    };
    let focus = app.read_with(cx, |app, cx| app.editor.read(cx).focus_handle(cx));
    cx.update(|window, cx| window.focus(&focus, cx));
    cx.dispatch_action(ShowOriginalView);
    assert_eq!(mode(&app, cx), Some(CompareMode::Original));
    cx.dispatch_action(ToggleCompare);
    assert_eq!(mode(&app, cx), Some(CompareMode::Side));
    cx.dispatch_action(ShowPseudonymizedView);
    assert_eq!(mode(&app, cx), Some(CompareMode::Pseudonymized));
    cx.dispatch_action(ShowEditView);
    assert_eq!(mode(&app, cx), None);
    cx.dispatch_action(ToggleCompare);
    assert_eq!(mode(&app, cx), Some(CompareMode::Side));
    cx.dispatch_action(ToggleCompare);
    assert_eq!(mode(&app, cx), None);
}

#[gpui::test]
fn tab_reveals_every_view_and_zoom_control_until_focus_leaves(cx: &mut gpui::TestAppContext) {
    use crate::pii::ui::compare::SWITCHER_IDLE;
    let (app, cx) = crate::document_view::tests::boot(cx);
    cx.update(|_, cx| cx.set_reduce_motion(true));
    cx.simulate_resize(gpui::size(px(1600.), px(900.)));
    app.update(cx, |app, cx| prepare_applied(app, "Anna Bob Anna", cx));
    cx.run_until_parked();
    app.update_in(cx, |app, window, cx| {
        window.focus(&app.editor.read(cx).focus_handle(cx), cx);
    });
    let draw = |cx: &mut gpui::VisualTestContext| {
        cx.update(|window, cx| window.draw(cx).clear(cx));
    };
    draw(cx);
    cx.simulate_keystrokes("shift-f6");
    let mut controls = 0;
    for _ in 0..60 {
        cx.simulate_keystrokes("tab");
        cx.run_until_parked();
        draw(cx);
        let in_pill =
            cx.update(|window, cx| app.read(cx).switcher_region.contains_focused(window, cx));
        if in_pill {
            controls += 1;
            // The idle timer must not hide a control with keyboard focus.
            cx.executor()
                .advance_clock(SWITCHER_IDLE + std::time::Duration::from_millis(50));
            cx.run_until_parked();
            draw(cx);
            let bounds = cx.debug_bounds("view-switcher").unwrap();
            cx.update(|window, _| {
                let scaled = bounds.scale(window.scale_factor());
                assert!(
                    window
                        .painted_quads()
                        .iter()
                        .any(|quad| { quad.bounds == scaled && quad.border_color.a == 1. }),
                    "the focused pill must be painted at full opacity"
                );
            });
        } else if controls > 0 {
            break;
        }
    }
    assert_eq!(controls, 7, "four views and three zoom controls");
    assert!(cx.update(|window, cx| {
        app.read(cx)
            .editor
            .read(cx)
            .focus_handle(cx)
            .is_focused(window)
    }));
    assert!(
        !app.read_with(cx, |app, _| app.switcher.shown()),
        "keyboard focus does not wake the pointer timer"
    );
}

#[gpui::test]
fn the_view_pill_rests_hidden_until_pointer_scroll_or_zoom(cx: &mut gpui::TestAppContext) {
    use crate::pii::ui::compare::SWITCHER_IDLE;
    let (app, cx) = crate::document_view::tests::boot(cx);
    cx.simulate_resize(gpui::size(px(1200.), px(800.)));
    let draw = |cx: &mut gpui::VisualTestContext| cx.update(|window, cx| window.draw(cx).clear(cx));
    let shown = |app: &Entity<DocumentView>, cx: &mut gpui::VisualTestContext| {
        app.read_with(cx, |app, _| app.switcher.shown())
    };
    let idle = |cx: &mut gpui::VisualTestContext| {
        cx.executor()
            .advance_clock(SWITCHER_IDLE + std::time::Duration::from_millis(50));
        cx.run_until_parked();
        draw(cx);
    };
    draw(cx);
    // At rest the pill does not sit over the text (README media decision 9).
    assert!(!shown(&app, cx));
    // Keyboard zoom shows it with the new level, then it fades.
    cx.dispatch_action(crate::document_view::ZoomIn);
    assert!(shown(&app, cx));
    idle(cx);
    assert!(!shown(&app, cx));
    // The pointer near the bottom of the text wakes it; on the pill it stays.
    let pill = cx.debug_bounds("zoom-reset").unwrap().center();
    cx.simulate_mouse_move(pill, None, Default::default());
    draw(cx);
    assert!(shown(&app, cx));
    idle(cx);
    assert!(shown(&app, cx), "the pill stays under the pointer");
    // Away from the bottom it fades after the idle delay.
    cx.simulate_mouse_move(gpui::point(pill.x, px(200.)), None, Default::default());
    draw(cx);
    assert!(shown(&app, cx));
    idle(cx);
    assert!(!shown(&app, cx));
    // Scrolling wakes it too.
    cx.simulate_event(gpui::ScrollWheelEvent {
        position: gpui::point(pill.x, px(300.)),
        delta: gpui::ScrollDelta::Pixels(gpui::point(px(0.), px(-40.))),
        ..Default::default()
    });
    assert!(shown(&app, cx));
}

#[gpui::test]
fn switching_single_column_views_keeps_the_reading_position(cx: &mut gpui::TestAppContext) {
    use crate::pii::ui::compare::CompareMode;
    let (app, cx) = crate::document_view::tests::boot(cx);
    cx.simulate_resize(gpui::size(px(1200.), px(600.)));
    let source = "Anna Bob Anna\n\n".repeat(200);
    app.update(cx, |app, cx| prepare_applied(app, &source, cx));
    cx.run_until_parked();
    let draw = |cx: &mut gpui::VisualTestContext| cx.update(|window, cx| window.draw(cx).clear(cx));
    app.update_in(cx, |app, window, cx| {
        app.set_view_mode(Some(CompareMode::Original), window, cx)
    });
    draw(cx);
    draw(cx);
    app.update(cx, |app, _| {
        let left = &app.compare.as_ref().unwrap().left;
        let mut offset = left.offset();
        offset.y = -(left.max_offset().y / 2.);
        left.set_offset(offset);
    });
    draw(cx);
    app.update_in(cx, |app, window, cx| {
        app.set_view_mode(Some(CompareMode::Pseudonymized), window, cx)
    });
    draw(cx);
    app.read_with(cx, |app, _| {
        let right = &app.compare.as_ref().unwrap().right;
        let fraction = f32::from(-right.offset().y) / f32::from(right.max_offset().y);
        assert!(
            (fraction - 0.5).abs() < 0.05,
            "the pseudonymized column opens where the original was: {fraction}"
        );
    });
}

#[gpui::test]
fn inspector_share_tab_copies_for_ai_after_apply(cx: &mut gpui::TestAppContext) {
    let (app, cx) = crate::document_view::tests::boot(cx);
    cx.simulate_resize(gpui::size(px(1200.), px(800.)));
    app.update(cx, |app, cx| prepare_applied(app, "Anna Bob Anna", cx));
    cx.run_until_parked();
    let draw = |cx: &mut gpui::VisualTestContext| cx.update(|window, cx| window.draw(cx).clear(cx));
    let click = |cx: &mut gpui::VisualTestContext, id: &'static str| {
        draw(cx);
        let at = cx
            .debug_bounds(id)
            .unwrap_or_else(|| panic!("missing {id}"))
            .center();
        cx.simulate_click(at, Default::default());
        cx.run_until_parked();
        draw(cx);
    };
    // Applied: the header offers Share, which opens the Inspector's Share tab
    // (ADR 0048 variant C).
    click(cx, "primary-share");
    app.read_with(cx, |app, _| {
        assert!(app.inspector_share);
        assert!(app.replacements_open());
    });
    assert!(cx.debug_bounds("share-status").is_some());
    click(cx, "share-copy-for-ai");
    let copied = cx.update(|_, cx| cx.read_from_clipboard().unwrap().text().unwrap());
    assert!(copied.contains("PERSON_1"));
    // Review is one click back.
    click(cx, "inspector-review");
    assert!(cx.debug_bounds("replacement-filter").is_some());
}

/// #301: a lone word of a replaced full name, its proposal kept, keeps Share
/// from saying the names are aliases, and the shelf from listing the tab as
/// ready.
#[gpui::test]
fn share_counts_a_lone_first_name_left_after_apply(cx: &mut gpui::TestAppContext) {
    let (app, cx) = crate::document_view::tests::boot(cx);
    app.update(cx, |app, cx| {
        let source = "Павлова Марина Сергеевна подписала. Марина обратилась в суд.";
        app.editor.update(cx, |e, cx| e.set_text(source, cx));
        app.pii.reviewing = true;
        let full = "Павлова Марина Сергеевна".len();
        let detection = mdoc_pii::Detection {
            range: 0..full,
            category: Category::Person,
            score: 0.9,
            recognizer: mdoc_pii::Recognizer::Model,
        };
        app.pii.review.ingest(source, vec![detection]).unwrap();
        // Propagation proposes "Марина" under her alias; keep it.
        let lone = app
            .pii
            .review
            .variants()
            .iter()
            .find(|variant| variant.original.as_ref() == "Марина")
            .expect("propagated first name")
            .id;
        app.pii.review.keep(lone, None);
        app.apply_replacements(cx);
        assert_eq!(app.pii.review.remaining(), 0);
        app.review_changed(cx);
    });
    cx.run_until_parked();
    app.read_with(cx, |app, cx| {
        assert!(app.editor.read(cx).text().contains("Марина обратилась"));
        assert_eq!(app.names_not_replaced(cx), 1);
        assert_eq!(app.share_status(cx), "1 replaced. 1 name not replaced.");
    });
    // Replacing the leftover by hand makes the line true again.
    app.update(cx, |app, cx| {
        let text = app
            .editor
            .read(cx)
            .text()
            .replace("Марина обратилась", "PERSON_1 обратилась");
        app.editor.update(cx, |e, cx| e.set_text(&text, cx));
    });
    cx.run_until_parked();
    app.read_with(cx, |app, cx| {
        assert_eq!(app.names_not_replaced(cx), 0);
        assert!(
            app.share_status(cx)
                .ends_with("Names in the text are aliases now.")
        );
    });
}

#[gpui::test]
fn find_and_replace_never_touches_applied_aliases(cx: &mut gpui::TestAppContext) {
    let (app, cx) = crate::document_view::tests::boot(cx);
    let source = "Bob met PERSON_X and Bob.";
    app.update(cx, |app, cx| {
        app.editor
            .update(cx, |editor, cx| editor.set_text(source, cx));
        let id = app
            .pii
            .review
            .add_manual(source, 0..3, Category::Person)
            .unwrap();
        let _ = id;
        app.apply_replacements(cx);
    });
    cx.run_until_parked();
    let applied = cx.update(|_, cx| app.read(cx).editor.read(cx).text().to_owned());
    assert_ne!(applied, source, "the manual mention is applied");
    let alias = cx.update(|_, cx| app.read(cx).pii.review.applied()[0].step.after.to_string());
    cx.dispatch_action(crate::search_session::FindReplaceMarkdown);
    cx.run_until_parked();
    cx.simulate_input(&alias);
    app.update_in(cx, |app, window, cx| {
        window.focus(&app.markdown_replace.read(cx).focus_handle(cx), cx);
    });
    cx.simulate_input("Carol");
    cx.run_until_parked();
    let matches = cx.update(|_, cx| app.read(cx).search.matches().len());
    assert!(matches >= 1);
    let applied_count = cx.update(|_, cx| app.read(cx).pii.review.applied().len());
    cx.dispatch_action(crate::search_session::ReplaceAllMarkdown);
    cx.run_until_parked();
    assert_eq!(
        cx.update(|_, cx| app.read(cx).editor.read(cx).text().to_owned()),
        applied
    );
    assert_eq!(
        cx.update(|_, cx| app.read(cx).search.status().cloned())
            .as_deref(),
        Some(format!("Replaced 0 · skipped {applied_count} (alias)").as_str())
    );
    cx.dispatch_action(crate::search_session::ReplaceNextMarkdown);
    cx.run_until_parked();
    assert_eq!(
        cx.update(|_, cx| app.read(cx).editor.read(cx).text().to_owned()),
        applied
    );
    assert_eq!(
        cx.update(|_, cx| app.read(cx).search.status().cloned())
            .as_deref(),
        Some("Skipped: alias, change it in Replacements")
    );
}

#[gpui::test]
fn merge_hint_live_panel_merge_undo_and_dismiss(cx: &mut gpui::TestAppContext) {
    let (app, cx) = crate::document_view::tests::boot(cx);
    let source = "Maya Bennett signed. Maya Benett agreed.";
    app.update(cx, |app, cx| {
        app.editor.update(cx, |e, cx| e.set_text(source, cx));
    });
    cx.run_until_parked();
    let hint = app.update(cx, |app, cx| {
        let (generation, identity, revision) = install_scan(app, cx);
        app.complete_pii_scan(
            generation,
            identity,
            revision,
            // A slip the hint offers to merge; a short form of the name
            // would join it by propagation (ADR 0092).
            Ok([0..12, 21..32]
                .map(|range| mdoc_pii::Detection {
                    range,
                    category: Category::Person,
                    score: 0.9,
                    recognizer: mdoc_pii::Recognizer::Model,
                })
                .to_vec()),
            cx,
        );
        app.pii.review.merge_hints().remove(0)
    });
    cx.run_until_parked();
    cx.update(|window, cx| window.draw(cx).clear(cx));
    let merge: &'static str = Box::leak(format!("merge-hint-merge-{}", hint.from).into_boxed_str());
    let dismiss: &'static str =
        Box::leak(format!("merge-hint-dismiss-{}", hint.from).into_boxed_str());
    assert!(cx.debug_bounds(merge).is_some());
    // Merge is a real Tab stop: Enter activates the focused control.
    app.update_in(cx, |app, window, cx| {
        let focus = app.pii.mapping.hint_focus.borrow()[&(hint.from, hint.into)][0].clone();
        window.focus(&focus, cx);
    });
    cx.update(|window, cx| window.draw(cx).clear(cx));
    cx.simulate_keystrokes("enter");
    cx.simulate_event(gpui::KeyUpEvent {
        keystroke: gpui::Keystroke::parse("enter").unwrap(),
    });
    cx.run_until_parked();
    cx.update(|window, cx| window.draw(cx).clear(cx));
    assert!(cx.debug_bounds(merge).is_none());
    app.update(cx, |app, cx| {
        let identities = app.pii.review.export_identities();
        assert_eq!(identities.len(), 1);
        assert_eq!(identities[0].alias, "PERSON_1");
        assert_eq!(identities[0].wording(), "Maya Bennett");
        assert_eq!(
            app.editor.read(cx).text(),
            source,
            "merge keeps proposals unapplied"
        );
    });
    let undo = cx.debug_bounds("replacement-notice-undo").unwrap().center();
    cx.simulate_click(undo, Default::default());
    cx.run_until_parked();
    cx.update(|window, cx| window.draw(cx).clear(cx));
    assert!(
        cx.debug_bounds(merge).is_some(),
        "one Undo restores the hint"
    );
    app.update(cx, |app, _| {
        assert_eq!(app.pii.review.export_identities().len(), 2)
    });
    let button = cx.debug_bounds(dismiss).unwrap().center();
    cx.simulate_click(button, Default::default());
    cx.run_until_parked();
    cx.update(|window, cx| window.draw(cx).clear(cx));
    assert!(cx.debug_bounds(merge).is_none());
    app.update(cx, |app, _| {
        assert_eq!(app.pii.review.export_identities().len(), 2)
    });
    // Dismissals are unordered pairs and live outside undo history.
    app.update(cx, |app, _| {
        assert!(app.pii.mapping.hint_dismissed(&(hint.pair.1, hint.pair.0)))
    });
    app.update_in(cx, |app, window, cx| {
        window.focus(&app.pii.mapping.focus, cx)
    });
    cx.simulate_keystrokes("secondary-shift-z");
    cx.simulate_keystrokes("secondary-z");
    cx.run_until_parked();
    cx.update(|window, cx| window.draw(cx).clear(cx));
    assert!(cx.debug_bounds(merge).is_none());
}

#[gpui::test]
fn merge_hint_applied_alias_keeps_restoring_and_merge_is_one_undo(cx: &mut gpui::TestAppContext) {
    let (app, cx) = crate::document_view::tests::boot(cx);
    let source = "Maya Bennett signed. Maya Benett agreed.";
    app.update(cx, |app, cx| {
        app.editor.update(cx, |e, cx| e.set_text(source, cx))
    });
    cx.run_until_parked();
    let hint = app.update_in(cx, |app, window, cx| {
        app.pii
            .review
            .ingest(
                source,
                [0..12, 21..32]
                    .map(|range| mdoc_pii::Detection {
                        range,
                        category: Category::Person,
                        score: 0.9,
                        recognizer: mdoc_pii::Recognizer::Model,
                    })
                    .to_vec(),
            )
            .unwrap();
        app.pii.reviewing = true;
        app.review_changed(cx);
        app.apply_all_pii(&PiiApplyAll, window, cx);
        assert_eq!(
            app.editor.read(cx).text(),
            "PERSON_1 signed. PERSON_2 agreed."
        );
        if !app.replacements_open() {
            app.toggle_replacements_panel(window, cx);
        }
        app.inspector_share = false;
        cx.notify();
        app.pii.review.merge_hints().remove(0)
    });
    cx.run_until_parked();
    cx.update(|window, cx| window.draw(cx).clear(cx));
    let selector = Box::leak(format!("merge-hint-merge-{}", hint.from).into_boxed_str());
    let merge = cx.debug_bounds(selector).unwrap().center();
    cx.simulate_click(merge, Default::default());
    cx.run_until_parked();
    cx.update(|window, cx| window.draw(cx).clear(cx));
    app.update(cx, |app, cx| {
        assert_eq!(
            app.editor.read(cx).text(),
            "PERSON_1 signed. PERSON_1 agreed."
        );
        let identities = app.pii.review.export_identities();
        assert_eq!(identities.len(), 1);
        assert_eq!(identities[0].previous, ["PERSON_2"]);
        let restored =
            mdoc_pii::restore::RestoreMap::new(identities.clone()).restore("PERSON_1 met PERSON_2");
        assert_eq!(restored.unresolved, 0);
        assert_eq!(
            restored
                .edits
                .iter()
                .map(|(_, s)| s.as_str())
                .collect::<Vec<_>>(),
            ["Maya Bennett", "Maya Bennett"]
        );
        // Case reopen/reseeding must retain the retired alias for later replies.
        let mut reopened = Review::default();
        reopened.seed(&identities, "Maya Bennett");
        let map = mdoc_pii::restore::RestoreMap::new(reopened.export_identities());
        assert_eq!(map.restore("PERSON_2").restored, 1);
        let next = reopened.new_identity(Category::Person);
        assert_eq!(reopened.identity(next).unwrap().alias, "PERSON_3");
    });
    let undo = cx.debug_bounds("replacement-notice-undo").unwrap().center();
    cx.simulate_click(undo, Default::default());
    cx.run_until_parked();
    app.update(cx, |app, cx| {
        assert_eq!(
            app.editor.read(cx).text(),
            "PERSON_1 signed. PERSON_2 agreed."
        );
        assert_eq!(app.pii.review.export_identities().len(), 2);
    });
}

#[gpui::test]
fn merge_hint_retains_target_primary_for_equal_length_wordings(cx: &mut gpui::TestAppContext) {
    let (app, cx) = crate::document_view::tests::boot(cx);
    let source = "Марина Павлова met Павлова Марина";
    app.update(cx, |app, cx| {
        app.editor.update(cx, |e, cx| e.set_text(source, cx))
    });
    cx.run_until_parked();
    let hint = app.update_in(cx, |app, window, cx| {
        let identities = ["Павлова Марина", "Марина Павлова"]
            .into_iter()
            .enumerate()
            .map(|(i, name)| mdoc_pii::restore::ExportedIdentity {
                alias: format!("PERSON_{}", i + 1),
                previous: vec![],
                category: Category::Person,
                custom: false,
                originals: vec![name.into()],
            })
            .collect::<Vec<_>>();
        app.pii.review.seed(&identities, source);
        app.pii.reviewing = true;
        app.review_changed(cx);
        app.apply_all_pii(&PiiApplyAll, window, cx);
        let hint = app.pii.review.merge_hints().remove(0);
        assert_eq!(hint.wording.as_ref(), "Павлова Марина");
        if !app.replacements_open() {
            app.toggle_replacements_panel(window, cx);
        }
        app.inspector_share = false;
        cx.notify();
        hint
    });
    cx.run_until_parked();
    cx.update(|window, cx| window.draw(cx).clear(cx));
    let selector = Box::leak(format!("merge-hint-merge-{}", hint.from).into_boxed_str());
    let merge = cx.debug_bounds(selector).unwrap().center();
    cx.simulate_click(merge, Default::default());
    cx.run_until_parked();
    app.update(cx, |app, _| {
        let exported = app.pii.review.export_identities();
        assert_eq!(exported.len(), 1);
        assert_eq!(exported[0].wording(), "Павлова Марина");
        assert_eq!(exported[0].alias, "PERSON_1");
        let unrelated = app.pii.review.new_identity(Category::Person);
        assert_eq!(
            app.pii.review.identity(unrelated).unwrap().alias,
            "PERSON_3"
        );
    });
}

/// A review of `source` with one Person proposal on its first `proposed`
/// wording; the selection, `select`, is held in the focused editor.
fn split_pill_fixture<'a>(
    cx: &'a mut gpui::TestAppContext,
    source: &'static str,
    proposed: &str,
    select: &str,
) -> (Entity<DocumentView>, &'a mut gpui::VisualTestContext) {
    let (app, cx) = crate::document_view::tests::boot(cx);
    app.update(cx, |app, cx| {
        app.editor
            .update(cx, |editor, cx| editor.set_text(source, cx));
    });
    cx.run_until_parked();
    let at = source.find(proposed).unwrap();
    app.update(cx, |app, cx| {
        let (generation, identity, revision) = install_scan(app, cx);
        app.complete_pii_scan(
            generation,
            identity,
            revision,
            Ok(vec![mdoc_pii::Detection {
                range: at..at + proposed.len(),
                category: Category::Person,
                score: 0.9,
                recognizer: mdoc_pii::Recognizer::Model,
            }]),
            cx,
        );
    });
    cx.run_until_parked();
    let start = source.find(select).unwrap();
    app.update_in(cx, |app, window, cx| {
        window.focus(&app.editor.read(cx).focus_handle(cx), cx);
        app.editor
            .update(cx, |e, cx| e.set_selection(start..start + select.len(), cx));
    });
    cx.run_until_parked();
    (app, cx)
}

/// Enter on the focused control, down and up, as a keyboard press clicks.
fn press_enter(cx: &mut gpui::VisualTestContext) {
    cx.update(|window, cx| {
        use gpui::InputEvent;
        let keystroke = gpui::Keystroke::parse("enter").unwrap();
        window.dispatch_event(
            gpui::KeyDownEvent {
                keystroke: keystroke.clone(),
                is_held: false,
                prefer_character_input: false,
            }
            .to_platform_input(),
            cx,
        );
        window.dispatch_event(gpui::KeyUpEvent { keystroke }.to_platform_input(), cx);
    });
    cx.run_until_parked();
}

fn redraw(cx: &mut gpui::VisualTestContext) {
    cx.update(|window, cx| window.draw(cx).clear(cx));
}

fn added(app: &Entity<DocumentView>, original: &str, cx: &mut gpui::VisualTestContext) -> Category {
    app.read_with(cx, |app, _| {
        app.pii
            .review
            .variants()
            .iter()
            .find(|v| v.original.as_ref() == original)
            .map(|v| v.category)
            .expect("added")
    })
}

#[gpui::test]
fn the_split_pill_adds_in_a_category_picked_with_the_mouse(cx: &mut gpui::TestAppContext) {
    let source = "Alice met Bob Stone today.";
    let (app, cx) = split_pill_fixture(cx, source, "Alice", "Bob Stone");
    // "Replace · Person ▾": the guess, which ⌘⌥P would use.
    let choice = app.read_with(cx, |app, cx| {
        app.editor
            .read(cx)
            .selection_action()
            .cloned()
            .unwrap()
            .choice
    });
    let choice = choice.expect("split pill");
    assert_eq!(choice.label.as_ref(), "Person");
    assert!(!choice.open);
    redraw(cx);
    let part = app.read_with(cx, |app, cx| app.editor.read(cx).selection_choice_bounds());
    let part = part.expect("the pill is painted").center();
    cx.simulate_click(part, Default::default());
    cx.run_until_parked();
    redraw(cx);
    redraw(cx);
    app.read_with(cx, |app, cx| {
        assert!(app.pii.selection_menu.is_some());
        let action = app.editor.read(cx).selection_action().cloned().unwrap();
        assert!(action.choice.unwrap().open, "the pill stays, pressed");
    });
    let menu = cx.debug_bounds("selection-category-menu");
    assert!(menu.is_some(), "the menu opens beside the pill");
    // A second press closes it; a third opens it again.
    cx.simulate_click(part, Default::default());
    cx.run_until_parked();
    assert!(app.read_with(cx, |app, _| app.pii.selection_menu.is_none()));
    cx.simulate_click(part, Default::default());
    cx.run_until_parked();
    redraw(cx);
    redraw(cx);
    let row = cx
        .debug_bounds("selection-category-ORG")
        .expect("Organization row");
    cx.simulate_click(row.center(), Default::default());
    cx.run_until_parked();
    assert_eq!(added(&app, "Bob Stone", cx), Category::Organization);
    app.update_in(cx, |app, window, cx| {
        assert!(app.pii.selection_menu.is_none());
        assert!(app.pii.popup.is_some() && app.pii.enter_applies);
        assert!(app.pii.focus.is_focused(window));
        // A pick is no guess: no category cue.
        assert!(matches!(app.pii.mapping.cue, Some((_, false))));
        let id = app.pii.review.identity_by_alias("ORG_1");
        assert!(id.is_some(), "the alias the menu showed");
        let _ = cx;
    });
}

#[gpui::test]
fn the_split_pill_adds_in_a_category_picked_with_the_keyboard(cx: &mut gpui::TestAppContext) {
    let source = "Alice met Bob Stone today.";
    let (app, cx) = split_pill_fixture(cx, source, "Alice", "Bob Stone");
    // ⇧⌘⌥P opens the menu on the guess; Esc closes it back to the editor.
    cx.simulate_keystrokes("secondary-alt-shift-p");
    cx.run_until_parked();
    app.update_in(cx, |app, window, cx| {
        assert!(app.pii.selection_menu.is_some());
        let guess = app.pii.popup_control_focus("selection-category-PERSON", cx);
        assert!(guess.is_focused(window), "focus is on the guess");
    });
    assert!(cx.debug_bounds("selection-category-menu").is_some());
    cx.simulate_keystrokes("escape");
    cx.run_until_parked();
    app.update_in(cx, |app, window, cx| {
        assert!(app.pii.selection_menu.is_none());
        assert!(app.editor.read(cx).focus_handle(cx).is_focused(window));
        assert!(app.editor.read(cx).selection_action().is_some());
    });
    // ↓ moves to Organization and Enter adds there.
    cx.simulate_keystrokes("secondary-alt-shift-p");
    cx.run_until_parked();
    cx.simulate_keystrokes("down");
    app.update_in(cx, |app, window, cx| {
        let row = app.pii.popup_control_focus("selection-category-ORG", cx);
        assert!(row.is_focused(window));
    });
    press_enter(cx);
    assert_eq!(added(&app, "Bob Stone", cx), Category::Organization);
    app.update_in(cx, |app, window, _| {
        assert!(app.pii.popup.is_some() && app.pii.focus.is_focused(window));
    });
    // Esc closes the popup back to the text, not to the closed menu.
    cx.simulate_keystrokes("escape");
    cx.run_until_parked();
    app.update_in(cx, |app, window, cx| {
        assert!(app.pii.popup.is_none());
        assert!(app.editor.read(cx).focus_handle(cx).is_focused(window));
    });
    // One undo step removes the addition.
    app.update_in(cx, |app, window, cx| {
        window.focus(&app.editor.read(cx).focus_handle(cx), cx)
    });
    cx.dispatch_action(mdoc_editor::Undo);
    cx.run_until_parked();
    app.read_with(cx, |app, _| {
        assert!(
            !app.pii
                .review
                .variants()
                .iter()
                .any(|v| v.original.as_ref() == "Bob Stone" && !v.mentions.is_empty())
        );
    });
}

#[gpui::test]
fn mod_alt_p_still_adds_with_the_guess(cx: &mut gpui::TestAppContext) {
    let source = "Alice met Bob Stone today.";
    let (app, cx) = split_pill_fixture(cx, source, "Alice", "Bob Stone");
    cx.simulate_keystrokes("secondary-alt-p");
    cx.run_until_parked();
    assert_eq!(added(&app, "Bob Stone", cx), Category::Person);
    app.read_with(cx, |app, _| assert!(app.pii.selection_menu.is_none()));
}

#[gpui::test]
fn a_proposed_wording_offers_no_category_menu(cx: &mut gpui::TestAppContext) {
    // The whole proposal is selected: Replace keeps its category, and the
    // popup's category control corrects it after (ADR 0024).
    let source = "Acme signed. Acme paid.";
    let (app, cx) = split_pill_fixture(cx, source, "Acme", "Acme");
    let action = app.read_with(cx, |app, cx| {
        app.editor.read(cx).selection_action().cloned()
    });
    assert!(action.unwrap().choice.is_none(), "no ▾");
    cx.simulate_keystrokes("secondary-alt-shift-p");
    cx.run_until_parked();
    app.update_in(cx, |app, window, cx| {
        assert!(app.pii.selection_menu.is_none());
        app.add_manual_candidate(Some(Category::Organization), window, cx);
        let (_, category) = app.selected_name().unwrap();
        assert_eq!(category, Category::Person, "a pick never recategorizes");
    });
}

#[test]
fn a_long_scan_counts_its_windows_once_it_knows_them() {
    use super::render::scan_stage;
    // Before the source is split, and for one window, stages run on a timer.
    for count in [(0, 0), (0, 1), (1, 1)] {
        let stage = scan_stage(0.5, count);
        assert_eq!(stage.text.as_ref(), "Reading…");
        assert!(stage.fade.is_some());
    }
    assert_eq!(scan_stage(4.0, (0, 0)).text.as_ref(), "Finding names…");
    // Several windows: the one being read, out of all, holding still.
    for (count, text) in [
        ((0, 12), "Finding names… 1 of 12"),
        ((2, 12), "Finding names… 3 of 12"),
        ((12, 12), "Finding names… 12 of 12"),
    ] {
        let stage = scan_stage(4.0, count);
        assert_eq!(stage.text.as_ref(), text);
        assert_eq!(stage.fade, None);
    }
    let count = super::scan::ScanCount::default();
    assert_eq!(count.get(), (0, 0));
    count.set(3, 12);
    assert_eq!(count.get(), (3, 12));
}

#[gpui::test]
fn a_running_scan_shows_skeleton_rows_and_keeps_rescan_off(cx: &mut gpui::TestAppContext) {
    let (app, cx) = crate::document_view::tests::boot(cx);
    app.update(cx, |app, cx| {
        app.editor
            .update(cx, |editor, cx| editor.set_text("Anna met Bob.", cx))
    });
    cx.run_until_parked();
    let generation = app.update(cx, |app, cx| {
        let (generation, ..) = install_scan(app, cx);
        app.pii.mapping.begin_review();
        app.pii.job.as_ref().unwrap().progress.set(2, 12);
        cx.notify();
        generation
    });
    cx.update(|window, cx| window.draw(cx).clear(cx));
    assert!(cx.debug_bounds("replacement-skeleton").is_some());
    assert!(cx.debug_bounds("scan-status").is_some());
    let rescan = cx.debug_bounds("review-rescan").unwrap().center();
    cx.simulate_click(rescan, Default::default());
    cx.run_until_parked();
    app.update(cx, |app, _| {
        let job = app.pii.job.as_ref().expect("the scan keeps running");
        assert_eq!(job.generation, generation);
        assert!(!job.cancel.load(std::sync::atomic::Ordering::Relaxed));
    });
}
