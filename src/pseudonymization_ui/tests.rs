use super::*;
use crate::{CopyMarkdown, document::Document, style::Theme};
use gpui::InputEvent;
fn install_automatic_scan(app: &mut Workspace, cx: &mut Context<Workspace>) -> (u64, u64, u64) {
    let revision = app.editor.read(cx).revision();
    let generation = app.pseudonymization.generation;
    let identity = app.session.generation;
    app.pseudonymization.review.open = true;
    app.pseudonymization.job = Some(ScanJob {
        cancel: Arc::new(AtomicBool::new(false)),
        revision,
        generation,
        config: settings::PiiConfig::default(),
        intent: ScanIntent::Anonymize,
    });
    (generation, identity, revision)
}
#[gpui::test]
fn anonymization_review_scan_proposes_without_applying(cx: &mut gpui::TestAppContext) {
    let (app, cx) = crate::ui_tests::boot(cx);
    app.update(cx, |app, cx| {
        app.editor
            .update(cx, |editor, cx| editor.set_text("Alice", cx));
    });
    cx.run_until_parked();
    app.update(cx, |app, cx| {
        app.pseudonymization.manual_review = true;
        let (generation, identity, revision) = install_automatic_scan(app, cx);
        app.pseudonymization.job.as_mut().unwrap().intent = ScanIntent::Review(Mode::Anonymize);
        app.complete_pseudonym_scan(
            generation,
            identity,
            revision,
            Ok(vec![pseudonymization::Detection {
                range: 0..5,
                category: Category::Person,
                score: 0.9,
                recognizer: crate::pseudonymization::Recognizer::Model,
            }]),
            cx,
        );
        assert_eq!(app.editor.read(cx).text(), "Alice");
        assert_eq!(app.editor.read(cx).revision(), revision);
        assert_eq!(app.pseudonymization.review.remaining(), 1);
        assert!(app.pseudonymization.completion.is_none());
    });
    cx.run_until_parked();
    app.update_in(cx, |app, window, cx| {
        let id = app.pseudonymization.review.groups[0].id;
        let range = app.pseudonymization.review.group(id).unwrap().mentions[0].clone();
        let annotation = app
            .pseudonymization
            .review
            .annotation_id(id, &range)
            .unwrap();
        app.activate_annotation(annotation, window, cx);
        app.accept_pseudonym(&AcceptPseudonymCandidate, window, cx);
        assert_eq!(app.editor.read(cx).text(), "PERSON");
        assert_eq!(app.pseudonymization.review.remaining(), 0);
    });
    cx.dispatch_action(mdoc_editor::Undo);
    cx.run_until_parked();
    app.update(cx, |app, cx| {
        assert_eq!(app.editor.read(cx).text(), "Alice");
        assert_eq!(app.pseudonymization.review.remaining(), 1);
    });
}

#[gpui::test]
#[ignore = "requires installed verified GLiNER2 FP16 and ONNX Runtime; no downloads"]
fn anonymization_installed_model_offline_smoke(cx: &mut gpui::TestAppContext) {
    let (app, cx) = crate::ui_tests::boot(cx);
    let source = "Alice Morgan represents Northbridge Legal Ltd. Contact alice@example.invalid.\n";
    app.update(cx, |app, cx| {
        app.editor
            .update(cx, |editor, cx| editor.set_text(source, cx));
    });
    cx.run_until_parked();
    app.update_in(cx, |app, window, cx| app.anonymize(&Anonymize, window, cx));
    cx.run_until_parked();
    app.update(cx, |app, cx| {
        assert!(
            app.pseudonymization.error.is_none(),
            "{:?}",
            app.pseudonymization.error
        );
        let (count, _) = app
            .pseudonymization
            .completion
            .expect("automatic scan must apply");
        assert!(count > 0);
        let result = app.editor.read(cx).text();
        assert!(result.contains("PERSON"));
        assert!(result.contains("EMAIL"));
        assert!(!result.contains("PERSON_"));
        eprintln!("installed offline anonymization: {count} replacements; {result}");
    });
}
#[gpui::test]
fn anonymization_applies_scan_as_one_undo_step_and_copy_is_explicit(cx: &mut gpui::TestAppContext) {
    let (app, cx) = crate::ui_tests::boot(cx);
    let source = "**Анна** Bob Анна [mail](anna@example.invalid)";
    let dir = tempfile::tempdir().unwrap();
    let original = dir.path().join("original.md");
    std::fs::write(&original, source).unwrap();
    let app = crate::ui_tests::open_document(&app, original.clone(), cx);
    app.update(cx, |app, cx| {
        assert_eq!(app.pseudonymization.review.mode, Mode::Anonymize);
        app.session.warning = Some("Review extraction".into());
        let kept = app
            .pseudonymization
            .review
            .add_manual(source, 2..10, Category::Person)
            .unwrap();
        app.pseudonymization.review.keep(kept, Some(2..10));
        cx.write_to_clipboard(gpui::ClipboardItem::new_string("keep clipboard".into()));
        let (generation, identity, revision) = install_automatic_scan(app, cx);
        let bob = source.find("Bob").unwrap();
        let mail = source.find("anna@example.invalid").unwrap();
        app.complete_pseudonym_scan(
            generation,
            identity,
            revision,
            Ok(vec![
                pseudonymization::Detection {
                    range: 2..10,
                    category: Category::Person,
                    score: 0.9,
                    recognizer: crate::pseudonymization::Recognizer::Model,
                },
                pseudonymization::Detection {
                    range: bob..bob + 3,
                    category: Category::Person,
                    score: 0.9,
                    recognizer: crate::pseudonymization::Recognizer::Model,
                },
                pseudonymization::Detection {
                    range: mail..mail + 20,
                    category: Category::Email,
                    score: 0.9,
                    recognizer: crate::pseudonymization::Recognizer::Model,
                },
            ]),
            cx,
        );
        assert_eq!(
            app.editor.read(cx).text(),
            "**Анна** PERSON PERSON [mail](EMAIL)"
        );
        assert_eq!(app.pseudonymization.completion.unwrap().0, 3);
        assert_eq!(
            cx.read_from_clipboard().unwrap().text().as_deref(),
            Some("keep clipboard")
        );
        assert!(app.dirty(cx));
        assert!(app.session.warning.is_some());
    });
    cx.run_until_parked();
    cx.update(|window, cx| window.draw(cx).clear(cx));
    assert!(cx.debug_bounds("anonymization-status").is_some());
    assert!(cx.debug_bounds("review-menu").is_none());
    assert!(app.read_with(cx, |app, _| app.pseudonymization.completion.is_some()));
    app.update_in(cx, |app, window, cx| {
        app.copy_markdown(&CopyMarkdown, window, cx);
        assert_eq!(
            cx.read_from_clipboard().unwrap().text().as_deref(),
            Some("**Анна** PERSON PERSON [mail](EMAIL)")
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
        assert!(app.pseudonymization.completion.is_none());
    });
    cx.dispatch_action(mdoc_editor::Redo);
    cx.run_until_parked();
    assert_eq!(
        app.read_with(cx, |app, cx| app.editor.read(cx).text().to_owned()),
        "**Анна** PERSON PERSON [mail](EMAIL)"
    );
}
#[gpui::test]
fn anonymization_failed_cancelled_stale_and_switched_scans_do_not_edit(
    cx: &mut gpui::TestAppContext,
) {
    let (app, cx) = crate::ui_tests::boot(cx);
    for case in 0..6 {
        app.update(cx, |app, cx| {
            app.editor
                .update(cx, |editor, cx| editor.set_text("Alice", cx));
            app.select_pii_mode(Mode::Anonymize, cx);
        });
        cx.run_until_parked();
        app.update(cx, |app, cx| {
            let (generation, identity, revision) = install_automatic_scan(app, cx);
            let mut result = Ok(vec![pseudonymization::Detection {
                range: 0..5,
                category: Category::Person,
                score: 0.9,
                recognizer: crate::pseudonymization::Recognizer::Model,
            }]);
            match case {
                0 => result = Err("Partial scan rejected".into()),
                1 => result.as_mut().unwrap()[0].range = 0..99,
                2 => app.pseudonymization.cancel(),
                3 => {
                    app.editor
                        .update(cx, |editor, cx| editor.replace_range(0..5, "Betty", cx));
                }
                4 => app.select_pii_mode(Mode::Pseudonymize, cx),
                _ => app.session.replace(Document::default()),
            }
            app.complete_pseudonym_scan(generation, identity, revision, result, cx);
            assert_eq!(
                app.editor.read(cx).text(),
                if case == 3 { "Betty" } else { "Alice" }
            );
            assert!(app.pseudonymization.completion.is_none());
            app.pseudonymization.cancel();
        });
        cx.run_until_parked();
    }
}
#[gpui::test]
fn anonymization_icon_menu_defaults_and_manual_popup_are_simple(cx: &mut gpui::TestAppContext) {
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
        assert!(cx.debug_bounds("Anonymize").is_some());
        assert!(cx.debug_bounds("Pseudonymize").is_none());
        click(cx, "pii-mode-menu");
        click(cx, "choose-pseudonymize");
        assert_eq!(
            app.read_with(cx, |app, _| app.pseudonymization.review.mode),
            Mode::Pseudonymize
        );
        assert!(cx.debug_bounds("Pseudonymize").is_some());
        click(cx, "pii-mode-menu");
        click(cx, "choose-anonymize");
    }
    // Invalid preferences exercise the actual primary icon without loading
    // a model or downloading anything in the UI test.
    app.update(cx, |app, cx| {
        app.preferences.borrow_mut().current = None;
        cx.notify();
    });
    click(cx, "Anonymize");
    app.update_in(cx, |app, window, cx| {
        assert!(app.editor.read(cx).focus_handle(cx).is_focused(window));
    });
    assert!(app.read_with(cx, |app, _| app.pseudonymization.error.is_some()));
    click(cx, "review-anonymization");
    app.update_in(cx, |app, window, cx| {
        app.editor
            .update(cx, |editor, cx| editor.set_text("Alice", cx));
        let id = app
            .pseudonymization
            .review
            .add_manual("Alice", 0..5, Category::Person)
            .unwrap();
        app.sync_annotations(cx);
        let range = app.pseudonymization.review.group(id).unwrap().mentions[0].clone();
        let annotation = app
            .pseudonymization
            .review
            .annotation_id(id, &range)
            .unwrap();
        app.activate_annotation(annotation, window, cx);
    });
    cx.run_until_parked();
    cx.update(|window, cx| window.draw(cx).clear(cx));
    assert!(cx.debug_bounds("pseudonym-replacement").is_none());
    assert!(cx.debug_bounds("candidate-links").is_none());
    app.update_in(cx, |app, window, cx| {
        app.pseudonymization
            .input
            .update(cx, |input, cx| input.set_value("CUSTOM_1".into(), cx));
        app.accept_pseudonym(&AcceptPseudonymCandidate, window, cx);
        assert_eq!(app.editor.read(cx).text(), "PERSON");
    });
    let next = crate::ui_tests::new_document(&app, cx);
    assert_eq!(
        next.read_with(cx, |app, _| app.pseudonymization.review.mode),
        Mode::Anonymize
    );
}
#[gpui::test]
fn accept_all_button_applies_pending_replacements_as_one_undo_step(cx: &mut gpui::TestAppContext) {
    let (app, cx) = crate::ui_tests::boot(cx);
    app.update(cx, |app, cx| app.select_pii_mode(Mode::Pseudonymize, cx));
    let source = "**Анна** Acme Анна [mail](anna@example.invalid) Bob";
    app.update(cx, |app, cx| {
        app.editor
            .update(cx, |editor, cx| editor.set_text(source, cx));
    });
    cx.run_until_parked();
    app.update_in(cx, |app, window, cx| {
        let review = &mut app.pseudonymization.review;
        review.open = true;
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
        let range = app.pseudonymization.review.group(anna).unwrap().mentions[0].clone();
        let annotation = app
            .pseudonymization
            .review
            .annotation_id(anna, &range)
            .unwrap();
        app.activate_annotation(annotation, window, cx);
        app.pseudonymization
            .input
            .update(cx, |input, cx| input.set_value("PERSON_CUSTOM".into(), cx));
        window.focus(&app.focus, cx);
        cx.notify();
    });
    cx.run_until_parked();
    cx.update(|window, cx| window.draw(cx).clear(cx));
    let more = cx.debug_bounds("review-menu").unwrap();
    cx.simulate_click(more.center(), Default::default());
    cx.run_until_parked();
    cx.update(|window, cx| window.draw(cx).clear(cx));
    let bounds = cx.debug_bounds("accept-all-pseudonyms").unwrap();
    cx.simulate_click(bounds.center(), gpui::Modifiers::none());
    cx.run_until_parked();
    app.update(cx, |app, cx| {
        assert_eq!(
            app.editor.read(cx).text(),
            "**Анна** ORG_1 PERSON_CUSTOM [mail](EMAIL_1) Bob"
        );
        assert_eq!(app.pseudonymization.review.remaining(), 0);
        assert!(app.pseudonymization.popup.is_none());
        assert!(app.dirty(cx));
        assert!(app.pseudonymization.error.is_none());
    });
    // With no pending suggestions, another click must not add an undo step.
    let revision = app.read_with(cx, |app, cx| app.editor.read(cx).revision());
    cx.update(|window, cx| window.draw(cx).clear(cx));
    let more = cx.debug_bounds("review-menu").unwrap();
    cx.simulate_click(more.center(), Default::default());
    cx.run_until_parked();
    cx.update(|window, cx| window.draw(cx).clear(cx));
    let bounds = cx.debug_bounds("accept-all-pseudonyms").unwrap();
    cx.simulate_click(bounds.center(), gpui::Modifiers::none());
    cx.run_until_parked();
    app.update_in(cx, |app, window, cx| {
        assert_eq!(app.editor.read(cx).revision(), revision);
        window.focus(&app.editor.read(cx).focus_handle(cx), cx);
    });
    cx.dispatch_action(mdoc_editor::Undo);
    cx.run_until_parked();
    app.update(cx, |app, cx| {
        assert_eq!(app.editor.read(cx).text(), source);
        assert_eq!(app.pseudonymization.review.remaining(), 3);
    });
    cx.dispatch_action(mdoc_editor::Redo);
    cx.run_until_parked();
    app.update(cx, |app, cx| {
        assert_eq!(
            app.editor.read(cx).text(),
            "**Анна** ORG_1 PERSON_CUSTOM [mail](EMAIL_1) Bob"
        );
        assert_eq!(app.pseudonymization.review.remaining(), 0);
    });
}
#[gpui::test]
fn keyboard_reveals_long_candidate_links_without_editing_source(cx: &mut gpui::TestAppContext) {
    let (app, cx) = crate::ui_tests::boot(cx);
    app.update(cx, |app, cx| app.select_pii_mode(Mode::Pseudonymize, cx));
    cx.simulate_resize(gpui::size(px(640.), px(480.)));
    let source = (0..30)
        .map(|i| format!("Name{i:02}"))
        .collect::<Vec<_>>()
        .join(" ");
    app.update_in(cx, |app, window, cx| {
        app.editor
            .update(cx, |editor, cx| editor.set_text(&source, cx));
        app.pseudonymization.review.open = true;
        for i in 0..30 {
            app.pseudonymization
                .review
                .add_manual(&source, i * 7..i * 7 + 6, Category::Person)
                .unwrap();
        }
        app.sync_pseudonym_theme(cx);
        let id = app.pseudonymization.review.groups[0].id;
        let range = app.pseudonymization.review.group(id).unwrap().mentions[0].clone();
        let annotation = app
            .pseudonymization
            .review
            .annotation_id(id, &range)
            .unwrap();
        app.activate_annotation(annotation, window, cx);
        app.pseudonymization.popup.as_mut().unwrap().links_open = true;
        cx.notify();
    });
    cx.run_until_parked();
    let target = gpui::ElementId::from(gpui::SharedString::from("link-PERSON_30"));
    let mut reached = false;
    for _ in 0..50 {
        cx.simulate_keystrokes("tab");
        for _ in 0..3 {
            cx.update(|window, cx| {
                window.refresh();
                window.draw(cx).clear(cx);
            });
        }
        reached = cx.update(|window, cx| {
            app.read(cx)
                .pseudonymization
                .popup_controls
                .borrow()
                .get(&target)
                .is_some_and(|focus| focus.is_focused(window))
        });
        if reached {
            break;
        }
    }
    assert!(reached, "Tab must reach all disclosed linking choices");
    let last = cx.debug_bounds("last-candidate-link").unwrap();
    cx.update(|window, cx| {
        let app = app.read(cx);
        let viewport = app.pseudonymization.popup_scroll.bounds();
        assert!(last.top() >= viewport.top() && last.bottom() <= viewport.bottom());
        let offset = app.pseudonymization.popup_scroll.offset();
        assert!(offset.y < px(0.), "focus must reveal the lower controls");
        assert!(viewport.bottom() <= window.viewport_size().height);
        assert_eq!(app.editor.read(cx).text(), source);
        assert!(app.pseudonymization.popup.is_some());
    });
    cx.simulate_keystrokes("space");
    cx.update(|window, cx| {
        window.dispatch_event(
            gpui::KeyUpEvent {
                keystroke: gpui::Keystroke::parse("space").unwrap(),
            }
            .to_platform_input(),
            cx,
        );
    });
    cx.run_until_parked();
    app.update(cx, |app, cx| {
        assert_eq!(app.pseudonymization.input.read(cx).value(), "PERSON_30");
        assert_eq!(app.editor.read(cx).text(), source);
    });
    cx.simulate_keystrokes("escape");
    cx.update(|window, cx| {
        window.refresh();
        window.draw(cx).clear(cx);
    });
    let close = cx.debug_bounds("leave-pseudonyms").unwrap();
    cx.simulate_click(close.center(), Default::default());
    app.update_in(cx, |app, window, cx| {
        assert!(!app.pseudonymization.review.open);
        assert!(app.editor.read(cx).focus_handle(cx).is_focused(window));
        assert_eq!(app.pseudonymization.review.mappings().len(), 30);
        assert_eq!(app.editor.read(cx).text(), source);
    });
}

#[gpui::test]
fn accept_all_refuses_pending_scans_and_invalid_popup_tokens(cx: &mut gpui::TestAppContext) {
    let (app, cx) = crate::ui_tests::boot(cx);
    app.update(cx, |app, cx| app.select_pii_mode(Mode::Pseudonymize, cx));
    let source = "Alice Acme";
    app.update(cx, |app, cx| {
        app.editor
            .update(cx, |editor, cx| editor.set_text(source, cx));
    });
    cx.run_until_parked();
    app.update_in(cx, |app, window, cx| {
        app.pseudonymization.review.open = true;
        let alice = app
            .pseudonymization
            .review
            .add_manual(source, 0..5, Category::Person)
            .unwrap();
        app.pseudonymization
            .review
            .add_manual(source, 6..10, Category::Organization)
            .unwrap();
        let revision = app.editor.read(cx).revision();
        app.pseudonymization.job = Some(ScanJob {
            cancel: Arc::new(AtomicBool::new(false)),
            revision,
            generation: 1,
            config: settings::PiiConfig::default(),
            intent: ScanIntent::Review(Mode::Pseudonymize),
        });
        app.accept_all_pseudonyms(&AcceptAllPseudonyms, window, cx);
        assert_eq!(app.editor.read(cx).revision(), revision);
        assert_eq!(app.editor.read(cx).text(), source);
        app.pseudonymization.cancel();
        let range = app.pseudonymization.review.group(alice).unwrap().mentions[0].clone();
        let annotation = app
            .pseudonymization
            .review
            .annotation_id(alice, &range)
            .unwrap();
        app.activate_annotation(annotation, window, cx);
        app.pseudonymization
            .input
            .update(cx, |input, cx| input.set_value("invalid token".into(), cx));
        app.accept_all_pseudonyms(&AcceptAllPseudonyms, window, cx);
        assert_eq!(app.editor.read(cx).revision(), revision);
        assert_eq!(app.editor.read(cx).text(), source);
        assert_eq!(app.pseudonymization.review.remaining(), 2);
        assert!(app.pseudonymization.error.is_some());
    });
}
#[gpui::test]
fn successful_scan_removes_setup_prompt_only_for_the_scanned_model(cx: &mut gpui::TestAppContext) {
    let (app, cx) = crate::ui_tests::boot(cx);
    app.update(cx, |app, cx| app.select_pii_mode(Mode::Pseudonymize, cx));
    let panel = cx.update(|_, cx| app.read(cx).model_panel.clone());
    app.update(cx, |app, cx| {
        app.editor
            .update(cx, |editor, cx| editor.set_text("Alice", cx));
        app.pseudonymization.review.open = true;
        let revision = app.editor.read(cx).revision();
        app.pseudonymization.job = Some(ScanJob {
            cancel: Arc::new(AtomicBool::new(false)),
            revision,
            generation: 7,
            config: settings::PiiConfig::default(),
            intent: ScanIntent::Review(Mode::Pseudonymize),
        });
        app.complete_pseudonym_scan(
            7,
            app.session.generation,
            revision,
            Ok(vec![pseudonymization::Detection {
                range: 0..5,
                category: Category::Person,
                score: 0.9,
                recognizer: crate::pseudonymization::Recognizer::Model,
            }]),
            cx,
        );
    });
    cx.run_until_parked();
    cx.update(|window, cx| window.draw(cx).clear(cx));
    let more = cx.debug_bounds("review-menu").unwrap();
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
    app.update(cx, |app, cx| app.select_pii_mode(Mode::Pseudonymize, cx));
    app.update(cx, |app, cx| {
        app.editor
            .update(cx, |editor, cx| editor.set_text("Alice", cx));
        let result = || {
            Ok(vec![pseudonymization::Detection {
                range: 0..5,
                category: Category::Person,
                score: 0.9,
                recognizer: crate::pseudonymization::Recognizer::Model,
            }])
        };
        let identity = app.session.generation;
        let revision = app.editor.read(cx).revision();
        let install_job = |app: &mut Workspace| {
            app.pseudonymization.job = Some(ScanJob {
                cancel: Arc::new(AtomicBool::new(false)),
                revision,
                generation: 7,
                config: settings::PiiConfig::default(),
                intent: ScanIntent::Review(Mode::Pseudonymize),
            });
        };
        install_job(app);
        app.pseudonymization.cancel();
        app.complete_pseudonym_scan(7, identity, revision, result(), cx);
        assert!(app.pseudonymization.review.groups.is_empty());
        install_job(app);
        app.editor
            .update(cx, |editor, cx| editor.replace_range(0..5, "Betty", cx));
        app.complete_pseudonym_scan(7, identity, revision, result(), cx);
        assert!(app.pseudonymization.review.groups.is_empty());
        app.editor
            .update(cx, |editor, cx| editor.set_text("Alice", cx));
        let revision = app.editor.read(cx).revision();
        app.pseudonymization.job = Some(ScanJob {
            cancel: Arc::new(AtomicBool::new(false)),
            revision,
            generation: 9,
            config: settings::PiiConfig::default(),
            intent: ScanIntent::Review(Mode::Pseudonymize),
        });
        app.session.replace(Document::default());
        app.complete_pseudonym_scan(9, identity, revision, result(), cx);
        assert!(app.pseudonymization.review.groups.is_empty());
        app.pseudonymization.cancel();
    });
}

fn prepare_applied(app: &mut Workspace, source: &str, cx: &mut Context<Workspace>) {
    app.editor.update(cx, |e, cx| e.set_text(source, cx));
    app.pseudonymization
        .review
        .set_mode(Mode::Anonymize, source);
    app.pseudonymization.review.open = true;
    let detections: Vec<_> = source
        .match_indices("Anna")
        .chain(source.match_indices("Bob"))
        .map(|(at, text)| pseudonymization::Detection {
            range: at..at + text.len(),
            category: Category::Person,
            score: 0.9,
            recognizer: pseudonymization::Recognizer::Model,
        })
        .collect();
    app.pseudonymization
        .review
        .ingest(source, detections)
        .unwrap();
    app.commit_all_pii(None, cx).unwrap();
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
        app.leave_pseudonyms(cx);
        assert_eq!(app.editor.read(cx).text(), "PERSON PERSON PERSON");
        let id = app.pseudonymization.review.tracking.applied[0].id;
        app.activate_annotation(APPLIED_ID | id, window, cx);
    });
    cx.run_until_parked();
    cx.update(|window, cx| window.draw(cx).clear(cx));
    assert!(cx.debug_bounds("restoration-original").is_some());
    app.update_in(cx, |app, window, cx| {
        app.restore_pii(&RestorePii, window, cx)
    });
    cx.run_until_parked();
    app.update(cx, |app, cx| {
        assert_eq!(app.editor.read(cx).text(), "Anna PERSON PERSON");
        app.pseudonymization
            .review
            .ingest(
                "Anna PERSON PERSON",
                vec![pseudonymization::Detection {
                    range: 0..4,
                    category: Category::Person,
                    score: 0.9,
                    recognizer: pseudonymization::Recognizer::Model,
                }],
            )
            .unwrap();
        assert_eq!(app.pseudonymization.review.remaining(), 0);
    });
    cx.dispatch_action(mdoc_editor::Undo);
    cx.run_until_parked();
    app.update_in(cx, |app, window, cx| {
        assert_eq!(app.editor.read(cx).text(), "PERSON PERSON PERSON");
        let id = app.pseudonymization.review.tracking.applied[0].id;
        app.activate_annotation(APPLIED_ID | id, window, cx);
        app.restore_all_pii(&RestoreAllPii, window, cx);
    });
    cx.run_until_parked();
    app.update(cx, |app, cx| {
        assert_eq!(app.editor.read(cx).text(), "Anna PERSON Anna");
        assert_eq!(app.pseudonymization.review.tracking.applied.len(), 1);
        assert_eq!(
            app.pseudonymization.review.tracking.applied[0]
                .step
                .before
                .as_ref(),
            "Bob"
        );
    });
    cx.dispatch_action(mdoc_editor::Undo);
    cx.run_until_parked();
    cx.dispatch_action(mdoc_editor::Redo);
    cx.run_until_parked();
    app.update(cx, |app, cx| {
        assert_eq!(app.editor.read(cx).text(), "Anna PERSON Anna")
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
                app.pseudonymization.review = Default::default();
                prepare_applied(app, "Anna Bob Anna", cx);
            });
            cx.run_until_parked();
            app.update_in(cx, |app, window, cx| {
                let id = app.pseudonymization.review.tracking.applied[0].id;
                app.activate_annotation(APPLIED_ID | id, window, cx);
            });
            cx.run_until_parked();
            cx.update(|window, cx| window.draw(cx).clear(cx));
            let panel = cx.debug_bounds("applied-pii-popup").unwrap();
            for id in [
                "restoration-original",
                "restoration-token",
                "restore-this",
                "restore-all",
            ] {
                let control = cx
                    .debug_bounds(id)
                    .unwrap_or_else(|| panic!("missing {id}"));
                assert!(control.left() >= panel.left() && control.right() <= panel.right());
                assert!(control.top() >= panel.top() && control.bottom() <= panel.bottom());
            }
            assert!(panel.bottom() <= px(height) && panel.right() <= px(width));
            // Click the actual secondary action, rather than calling its handler.
            let restore_all = cx.debug_bounds("restore-all").unwrap().center();
            cx.simulate_click(restore_all, Default::default());
            cx.run_until_parked();
            app.update(cx, |app, cx| {
                assert_eq!(app.editor.read(cx).text(), "Anna PERSON Anna");
                assert!(app.pseudonymization.popup.is_none());
            });
            app.update_in(cx, |app, window, cx| {
                let id = app.pseudonymization.review.tracking.applied[0].id;
                app.activate_annotation(APPLIED_ID | id, window, cx);
            });
            cx.run_until_parked();
            cx.update(|window, cx| window.draw(cx).clear(cx));
            assert!(
                cx.debug_bounds("restore-all").is_none(),
                "one match needs only Restore this"
            );
            cx.simulate_keystrokes("escape");
            cx.run_until_parked();
            app.update_in(cx, |app, window, cx| {
                assert!(app.pseudonymization.popup.is_none());
                assert!(app.editor.read(cx).focus_handle(cx).is_focused(window));
            });
        }
    }
}

#[gpui::test]
fn long_transition_wraps_and_keyboard_replacement_remains_undoable(cx: &mut gpui::TestAppContext) {
    let (app, cx) = crate::ui_tests::boot(cx);
    cx.simulate_resize(gpui::size(px(640.), px(480.)));
    let original = "Индивидуальный предприниматель Анна Александровна ".repeat(5);
    for theme in [Theme::Dark, Theme::Light] {
        app.update_in(cx, |app, window, cx| {
            app.theme.set(theme);
            app.editor
                .update(cx, |editor, cx| editor.set_text(&original, cx));
            app.pseudonymization
                .review
                .set_mode(Mode::Pseudonymize, &original);
            app.pseudonymization.review.open = true;
            let id = app
                .pseudonymization
                .review
                .add_manual(&original, 0..original.len(), Category::Person)
                .unwrap();
            app.sync_annotations(cx);
            let annotation = app
                .pseudonymization
                .review
                .annotation_id(id, &(0..original.len()))
                .unwrap();
            app.activate_annotation(annotation, window, cx);
            app.pseudonymization
                .input
                .update(cx, |input, cx| input.set_value("CLIENT_1".into(), cx));
        });
        cx.run_until_parked();
        cx.update(|window, cx| window.draw(cx).clear(cx));
        let panel = cx.debug_bounds("pseudonym-popup").unwrap();
        let text = cx.debug_bounds("pseudonym-original").unwrap();
        assert!(
            text.size.height > px(26.),
            "long original must wrap, not truncate"
        );
        assert!(text.left() >= panel.left() && text.right() <= panel.right());
        assert!(panel.bottom() <= px(480.));
        // The current popup initially focuses its token field. Tab navigation
        // must reveal Replace even when the long transition makes it scroll.
        let target = gpui::ElementId::from("Accept");
        let mut reached = false;
        for _ in 0..8 {
            cx.simulate_keystrokes("tab");
            cx.update(|window, cx| {
                window.refresh();
                window.draw(cx).clear(cx);
            });
            reached = cx.update(|window, cx| {
                app.read(cx)
                    .pseudonymization
                    .popup_controls
                    .borrow()
                    .get(&target)
                    .is_some_and(|focus| focus.is_focused(window))
            });
            if reached {
                break;
            }
        }
        assert!(reached);
        cx.simulate_keystrokes("enter");
        cx.update(|window, cx| {
            window.dispatch_event(
                gpui::KeyUpEvent {
                    keystroke: gpui::Keystroke::parse("enter").unwrap(),
                }
                .to_platform_input(),
                cx,
            );
        });
        cx.run_until_parked();
        assert_eq!(
            app.read_with(cx, |app, cx| app.editor.read(cx).text().to_owned()),
            "CLIENT_1"
        );
        cx.dispatch_action(mdoc_editor::Undo);
        cx.run_until_parked();
        assert_eq!(
            app.read_with(cx, |app, cx| app.editor.read(cx).text().to_owned()),
            original
        );
    }
}
#[gpui::test]
fn editing_marker_invalidates_provenance_undo_recovers_it_and_paste_creates_none(
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
    let id = app.read_with(cx, |app, _| {
        app.pseudonymization.review.tracking.applied[0].id
    });
    app.update(cx, |app, cx| {
        assert_eq!(
            app.pseudonymization.review.tracking.get(id).unwrap().range,
            5..11
        );
        app.editor
            .update(cx, |e, cx| e.replace_range(6..6, "X", cx));
    });
    cx.run_until_parked();
    assert!(app.read_with(cx, |app, _| {
        app.pseudonymization.review.tracking.applied.is_empty()
    }));
    cx.dispatch_action(mdoc_editor::Undo);
    cx.run_until_parked();
    app.update(cx, |app, cx| {
        assert_eq!(
            app.pseudonymization.review.tracking.get(id).unwrap().range,
            5..11
        );
        app.editor
            .update(cx, |e, cx| e.replace_range(11..11, " PERSON", cx));
    });
    cx.run_until_parked();
    app.update(cx, |app, cx| {
        assert_eq!(app.editor.read(cx).text(), "😀 PERSON PERSON");
        assert_eq!(app.pseudonymization.review.tracking.applied.len(), 1);
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
        let applied = &app.pseudonymization.review.tracking.applied;
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
        app.restore_pii(&RestorePii, window, cx)
    });
    cx.run_until_parked();
    app.update(cx, |app, cx| {
        assert_eq!(
            app.editor.read(cx).text(),
            "intro\n\n[link](https://x.invalid/PERSON/Bob)"
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
