use super::*;
use crate::{CopyMarkdown, document::Document, style::Theme};
fn install_scan(app: &mut Workspace, cx: &mut Context<Workspace>) -> (u64, u64, u64) {
    let revision = app.editor.read(cx).revision();
    let generation = app.pii.generation;
    let identity = app.session.generation;
    app.pii.reviewing = true;
    app.pii.job = Some(ScanJob {
        cancel: Arc::new(AtomicBool::new(false)),
        revision,
        generation,
        config: settings::PiiConfig::default(),
    });
    (generation, identity, revision)
}
#[gpui::test]
fn scan_proposes_without_applying_until_explicit_apply(cx: &mut gpui::TestAppContext) {
    let (app, cx) = crate::ui_tests::boot(cx);
    app.update(cx, |app, cx| {
        app.editor
            .update(cx, |editor, cx| editor.set_text("Alice", cx));
    });
    cx.run_until_parked();
    app.update(cx, |app, cx| {
        let (generation, identity, revision) = install_scan(app, cx);
        app.complete_pii_scan(
            generation,
            identity,
            revision,
            Ok(vec![pii::Detection {
                range: 0..5,
                category: Category::Person,
                score: 0.9,
                recognizer: crate::pii::Recognizer::Model,
            }]),
            cx,
        );
        assert_eq!(app.editor.read(cx).text(), "Alice");
        assert_eq!(app.editor.read(cx).revision(), revision);
        assert_eq!(app.pii.review.remaining(), 1);
        assert!(app.pii.mapping.open);
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
#[ignore = "requires installed verified GLiNER2 FP16 and ONNX Runtime; no downloads"]
fn installed_model_offline_smoke(cx: &mut gpui::TestAppContext) {
    let (app, cx) = crate::ui_tests::boot(cx);
    let source = "Alice Morgan represents Northbridge Legal Ltd. Contact alice@example.invalid.\n";
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
    let (app, cx) = crate::ui_tests::boot(cx);
    let source = "**Анна** Bob Анна [mail](anna@example.invalid)";
    let applied = "**Анна** PERSON_2 PERSON_1 [mail](EMAIL_1)";
    let dir = tempfile::tempdir().unwrap();
    let original = dir.path().join("original.md");
    std::fs::write(&original, source).unwrap();
    let app = crate::ui_tests::open_document(&app, original.clone(), cx);
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
                pii::Detection {
                    range: 2..10,
                    category: Category::Person,
                    score: 0.9,
                    recognizer: crate::pii::Recognizer::Model,
                },
                pii::Detection {
                    range: bob..bob + 3,
                    category: Category::Person,
                    score: 0.9,
                    recognizer: crate::pii::Recognizer::Model,
                },
                pii::Detection {
                    range: mail..mail + 20,
                    category: Category::Email,
                    score: 0.9,
                    recognizer: crate::pii::Recognizer::Model,
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
    let (app, cx) = crate::ui_tests::boot(cx);
    for case in 0..6 {
        app.update(cx, |app, cx| {
            app.editor
                .update(cx, |editor, cx| editor.set_text("Alice", cx));
        });
        cx.run_until_parked();
        app.update(cx, |app, cx| {
            let (generation, identity, revision) = install_scan(app, cx);
            let mut result = Ok(vec![pii::Detection {
                range: 0..5,
                category: Category::Person,
                score: 0.9,
                recognizer: crate::pii::Recognizer::Model,
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
            app.pii.cancel();
        });
        cx.run_until_parked();
    }
}
#[gpui::test]
fn toolbar_icon_scans_once_then_toggles_the_review(cx: &mut gpui::TestAppContext) {
    let (app, cx) = crate::ui_tests::boot(cx);
    cx.simulate_resize(gpui::size(px(640.), px(480.)));
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
    click(cx, "Pseudonymize");
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
        app.sync_annotations(cx);
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
    let next = crate::ui_tests::new_document(&app, cx);
    next.read_with(cx, |app, _| {
        assert!(!app.pii.reviewing);
        assert!(!app.pii.mapping.open);
    });
}
#[gpui::test]
fn accept_all_button_applies_pending_replacements_as_one_undo_step(cx: &mut gpui::TestAppContext) {
    let (app, cx) = crate::ui_tests::boot(cx);
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
        app.sync_annotations(cx);
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
    let bounds = cx.debug_bounds("apply-identity-map").unwrap();
    cx.simulate_click(bounds.center(), gpui::Modifiers::none());
    cx.run_until_parked();
    app.update(cx, |app, cx| {
        assert_eq!(
            app.editor.read(cx).text(),
            "**Анна** ORG_1 PERSON_CUSTOM [mail](EMAIL_1) Bob"
        );
        assert_eq!(app.pii.review.remaining(), 0);
        assert!(app.pii.popup.is_some());
        assert!(app.dirty(cx));
        assert!(app.pii.error.is_none());
    });
    // With no pending suggestions, another click must not add an undo step.
    let revision = app.read_with(cx, |app, cx| app.editor.read(cx).revision());
    cx.update(|window, cx| window.draw(cx).clear(cx));
    assert!(cx.debug_bounds("apply-identity-map").is_none());
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
    let (app, cx) = crate::ui_tests::boot(cx);
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
        app.sync_annotations(cx);
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
    let popup = cx.debug_bounds("pseudonym-popup").unwrap();
    assert!(
        last.top() >= popup.top() && last.bottom() <= popup.bottom(),
        "target {last:?} popup {popup:?}"
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
    let (app, cx) = crate::ui_tests::boot(cx);
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
fn successful_scan_removes_setup_prompt_only_for_the_scanned_model(cx: &mut gpui::TestAppContext) {
    let (app, cx) = crate::ui_tests::boot(cx);
    let panel = cx.update(|_, cx| app.read(cx).model_panel.clone());
    app.update(cx, |app, cx| {
        app.editor
            .update(cx, |editor, cx| editor.set_text("Alice", cx));
        app.pii.reviewing = true;
        let revision = app.editor.read(cx).revision();
        app.pii.job = Some(ScanJob {
            cancel: Arc::new(AtomicBool::new(false)),
            revision,
            generation: 7,
            config: settings::PiiConfig::default(),
        });
        app.complete_pii_scan(
            7,
            app.session.generation,
            revision,
            Ok(vec![pii::Detection {
                range: 0..5,
                category: Category::Person,
                score: 0.9,
                recognizer: crate::pii::Recognizer::Model,
            }]),
            cx,
        );
    });
    cx.run_until_parked();
    cx.update(|window, cx| window.draw(cx).clear(cx));
    let more = cx.debug_bounds("replacement-commands").unwrap();
    cx.simulate_click(more.center(), Default::default());
    cx.run_until_parked();
    cx.update(|window, cx| window.draw(cx).clear(cx));
    assert!(cx.debug_bounds("review-download-model").is_none());
    cx.update(|_, cx| {
        let index = settings::Model::ALL
            .iter()
            .position(|m| *m == settings::Model::Pii(settings::PiiModel::Fp16))
            .unwrap();
        assert!(matches!(
            panel.read(cx).statuses[index],
            settings_ui::Status::Ready
        ));
    });
    app.update(cx, |app, cx| {
        app.preferences
            .borrow_mut()
            .current
            .as_mut()
            .unwrap()
            .pseudonymization
            .model = settings::PiiModel::Fp32;
        cx.notify();
    });
    cx.run_until_parked();
    cx.update(|window, cx| window.draw(cx).clear(cx));
    assert!(cx.debug_bounds("review-download-model").is_some());
    panel.update(cx, |panel, _| {
        let index = settings::Model::ALL
            .iter()
            .position(|m| *m == settings::Model::Pii(settings::PiiModel::Fp16))
            .unwrap();
        panel.statuses[index] = settings_ui::Status::Missing;
    });
    app.update(cx, |app, cx| {
        app.preferences
            .borrow_mut()
            .current
            .as_mut()
            .unwrap()
            .pseudonymization
            .model = settings::PiiModel::Fp16;
        cx.notify();
    });
    cx.run_until_parked();
    cx.update(|window, cx| window.draw(cx).clear(cx));
    assert!(cx.debug_bounds("review-download-model").is_some());
}
#[gpui::test]
fn cancelled_edited_and_replaced_document_results_are_rejected(cx: &mut gpui::TestAppContext) {
    let (app, cx) = crate::ui_tests::boot(cx);
    app.update(cx, |app, cx| {
        app.editor
            .update(cx, |editor, cx| editor.set_text("Alice", cx));
        let result = || {
            Ok(vec![pii::Detection {
                range: 0..5,
                category: Category::Person,
                score: 0.9,
                recognizer: crate::pii::Recognizer::Model,
            }])
        };
        let identity = app.session.generation;
        let revision = app.editor.read(cx).revision();
        let install_job = |app: &mut Workspace| {
            app.pii.job = Some(ScanJob {
                cancel: Arc::new(AtomicBool::new(false)),
                revision,
                generation: 7,
                config: settings::PiiConfig::default(),
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
        });
        app.session.replace(Document::default());
        app.complete_pii_scan(9, identity, revision, result(), cx);
        assert!(app.pii.review.variants().is_empty());
        app.pii.cancel();
    });
}

fn prepare_applied(app: &mut Workspace, source: &str, cx: &mut Context<Workspace>) {
    app.editor.update(cx, |e, cx| e.set_text(source, cx));
    app.pii.reviewing = true;
    let detections: Vec<_> = source
        .match_indices("Anna")
        .chain(source.match_indices("Bob"))
        .map(|(at, text)| pii::Detection {
            range: at..at + text.len(),
            category: Category::Person,
            score: 0.9,
            recognizer: pii::Recognizer::Model,
        })
        .collect();
    app.pii.review.ingest(source, detections).unwrap();
    app.apply_replacements(cx);
    assert_eq!(app.pii.review.remaining(), 0);
    app.sync_annotations(cx);
}
#[gpui::test]
fn applied_highlights_restore_one_or_matching_originals_and_keep_survives_rescan(
    cx: &mut gpui::TestAppContext,
) {
    let (app, cx) = crate::ui_tests::boot(cx);
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
                vec![pii::Detection {
                    range: 0..4,
                    category: Category::Person,
                    score: 0.9,
                    recognizer: pii::Recognizer::Model,
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
    let (app, cx) = crate::ui_tests::boot(cx);
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
            for id in [
                "pseudonym-original",
                "direct-alias",
                "direct-undo",
                "direct-keep",
            ] {
                let control = cx
                    .debug_bounds(id)
                    .unwrap_or_else(|| panic!("missing {id}"));
                assert!(control.left() >= panel.left() && control.right() <= panel.right());
                assert!(control.top() >= panel.top() && control.bottom() <= panel.bottom());
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
                assert!(app.pii.popup.is_none());
            });
            app.update_in(cx, |app, window, cx| {
                let id = app.pii.review.applied()[0].id;
                app.activate_annotation(APPLIED_ID | id, window, cx);
            });
            cx.run_until_parked();
            cx.update(|window, cx| window.draw(cx).clear(cx));
            assert!(cx.debug_bounds("direct-applied").is_some());
            assert!(cx.debug_bounds("direct-undo").is_some());
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
fn long_original_and_neutral_enter_preserve_source_until_explicit_apply(
    cx: &mut gpui::TestAppContext,
) {
    let (app, cx) = crate::ui_tests::boot(cx);
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
            app.sync_annotations(cx);
            app.activate_annotation(app.pii.review.candidates()[0].id, window, cx);
        });
        cx.run_until_parked();
        cx.simulate_keystrokes("enter");
        cx.run_until_parked();
        assert_eq!(
            app.read_with(cx, |app, cx| app.editor.read(cx).text().to_owned()),
            original
        );
        app.update_in(cx, |app, window, cx| {
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
        let apply = cx.debug_bounds("apply-identity-map").unwrap();
        cx.simulate_click(apply.center(), Default::default());
        cx.run_until_parked();
        app.update_in(cx, |app, window, cx| {
            assert_eq!(app.editor.read(cx).text(), "CLIENT_1");
            assert!(app.pii.popup.is_some());
            window.focus(&app.pii.focus, cx);
        });
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
    let (app, cx) = crate::ui_tests::boot(cx);
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
    let (app, cx) = crate::ui_tests::boot(cx);
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
    let (app, cx) = crate::ui_tests::boot(cx);
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
    let (app, cx) = crate::ui_tests::boot(cx);
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
            Ok(vec![pii::Detection {
                range: first..first + "Иван".len(),
                category: Category::Person,
                score: 0.9,
                recognizer: crate::pii::Recognizer::Model,
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
        let text = app.editor.read(cx).text().to_owned();
        assert!(text.starts_with("Contact PERSON_2 today."), "{text}");
        assert!(text.ends_with("Иван agreed."), "{text}");
        assert_eq!(app.pii.review.remaining(), 1);
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
    let (app, cx) = crate::ui_tests::boot(cx);
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
            Ok(vec![pii::Detection {
                range: anna..anna + "Анна".len(),
                category: Category::Person,
                score: 0.9,
                recognizer: crate::pii::Recognizer::Model,
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
