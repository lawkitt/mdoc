//! Converting a source to Markdown: the import job, OCR consent, recognition
//! languages and setup (`ImportFlow`).
use super::*;

#[gpui::test]
fn inline_ocr_skip_imports_native_content_and_keeps_page_warning(cx: &mut TestAppContext) {
    let (app, cx) = boot(cx);
    let source = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("tests/fixtures/import/handmade-partly-scanned.pdf");
    app.update_in(cx, |app, window, cx| {
        app.start_import(source.clone(), window, cx)
    });
    cx.run_until_parked();
    assert!(cx.update(|_, cx| app.read(cx).import.ocr_required().is_some()));
    assert!(!cx.has_pending_prompt());
    app.update_in(cx, |app, window, cx| app.ocr_action(true, window, cx));
    cx.run_until_parked();
    app.update_in(cx, |app, _, cx| {
        assert!(!app.import.busy());
        assert!(!app.lifecycle.prompting());
        assert!(app.editor.read(cx).text().contains("Readable page three"));
        assert!(app.session.warning.as_ref().unwrap().contains("2, 5"));
        assert!(app.session.document.path.is_none());
        assert_eq!(app.session.source.as_ref(), Some(&source));
    });
}

#[gpui::test]
fn inline_ocr_wait_preserves_current_edits(cx: &mut TestAppContext) {
    let (app, cx) = boot(cx);
    app.update_in(cx, |app, window, cx| {
        app.editor
            .update(cx, |editor, cx| editor.set_text("keep edits", cx));
        app.import.job_mut().complete(
            app.session.generation,
            Err(import::ImportError::NeedsOcr(
                "scan.pdf".into(),
                vec![1],
                Vec::new(),
                1,
            )),
        );
        app.resume_import(window, cx);
    });
    assert!(!cx.has_pending_prompt());
    cx.run_until_parked();
    app.update_in(cx, |app, _, cx| {
        assert_eq!(app.editor.read(cx).text(), "keep edits");
        assert!(!app.import.busy());
        assert!(!app.lifecycle.prompting());
        assert!(app.session.source.is_none());
    });
}

#[gpui::test]
fn ocr_setup_failure_is_retryable_and_stale_success_does_not_import(cx: &mut TestAppContext) {
    let (app, cx) = boot(cx);
    app.update_in(cx, |app, window, cx| {
        app.ocr_state = OcrState::Installing;
        app.import.job_mut()
            .defer_for_setup(app.session.generation, "scan.pdf".into());
        app.finish_ocr_setup(Err("download interrupted".into()), window, cx);
        assert!(matches!(&app.ocr_state, OcrState::Failed(error) if error.contains("download interrupted")));
        assert!(!app.import.busy());
        assert!(app.import.ocr_required().is_some());
        assert!(!app.lifecycle.prompting());
    });
    assert!(!cx.has_pending_prompt());
    cx.run_until_parked();
    app.update_in(cx, |app, window, cx| {
        assert!(!app.import.busy());
        assert!(!app.import.slot_busy());
        app.editor
            .update(cx, |editor, cx| editor.set_text("keep edits", cx));
        app.import
            .job_mut()
            .defer_for_setup(app.session.generation, "scan.pdf".into());
        app.session.generation += 1;
        app.finish_ocr_setup(
            Ok(ocr::Installed {
                config: mdoc_store::settings::OcrConfig::default(),
                models: "models".into(),
                pdfium: "pdfium".into(),
                onnx: "onnx".into(),
            }),
            window,
            cx,
        );
        assert!(!app.import.busy());
        assert!(!app.import.job_mut().has_ocr_continuation());
        assert_eq!(app.editor.read(cx).text(), "keep edits");
    });
}

fn set_default_ocr(app: &DocumentView, model: mdoc_store::settings::OcrModel) {
    let mut store = app.preferences.borrow_mut();
    store.current.as_mut().unwrap().ocr.model = model;
}

#[gpui::test]
fn ocr_card_explains_that_a_pick_never_changes_the_default(cx: &mut TestAppContext) {
    use mdoc_import::recognition::{ENGLISH, RUSSIAN};
    for theme in [Theme::Light, Theme::Dark] {
        let (app, cx) = boot(cx);
        app.update_in(cx, |app, window, cx| {
            app.theme.set(theme);
            app.lifecycle.show_source();
            app.import.job_mut().complete(
                app.session.generation,
                Err(import::ImportError::NeedsOcr(
                    "scan.pdf".into(),
                    vec![1],
                    Vec::new(),
                    1,
                )),
            );
            app.resume_import(window, cx);
            app.ocr_state = OcrState::Missing;
        });
        for languages in [vec![ENGLISH], vec![ENGLISH, RUSSIAN]] {
            app.update(cx, |app, cx| app.pick_recognition(languages, cx));
            cx.update(|window, cx| {
                window.refresh();
                window.draw(cx).clear(cx);
            });
            let reason = cx.debug_bounds("ocr-language-hint").unwrap();
            let note = cx.debug_bounds("ocr-document-only").unwrap();
            assert!(note.top() >= reason.bottom());
            assert!(note.bottom() <= px(760.));
            cx.update(|_, cx| {
                assert_eq!(
                    app.read(cx)
                        .preferences
                        .borrow()
                        .snapshot()
                        .unwrap()
                        .ocr
                        .model,
                    mdoc_store::settings::OcrModel::V6Small
                )
            });
        }
    }
}

/// A pick, a run's choice and a setup's captured model all survive a
/// Settings change, and setup never leaves the shared state Installing.
#[gpui::test]
fn recognition_choice_and_setup_survive_a_settings_change(cx: &mut TestAppContext) {
    use mdoc_import::recognition::{ENGLISH, RUSSIAN};
    use mdoc_store::settings::OcrModel;
    let (app, cx) = boot(cx);
    app.update_in(cx, |app, window, cx| {
        app.import.job_mut().complete(
            app.session.generation,
            Err(import::ImportError::NeedsOcr(
                "scan.pdf".into(),
                vec![1],
                Vec::new(),
                1,
            )),
        );
        app.resume_import(window, cx);
        assert_eq!(app.recognition_choice().languages, [ENGLISH]);
        // Picking the default again still holds it.
        app.pick_recognition(vec![ENGLISH, RUSSIAN], cx);
        app.pick_recognition(vec![ENGLISH], cx);
        set_default_ocr(app, OcrModel::Cyrillic);
        assert_eq!(
            app.recognition_choice().reason,
            "English: no sign of Russian in the text, file name or keyboard"
        );
        assert_eq!(app.recognition_model(), OcrModel::V6Small);
        // Without a pick, running OCR keeps the choice it ran with. The
        // bundle counts as checked, so nothing is set up or downloaded.
        app.import.set_pick(None);
        assert_eq!(app.recognition_model(), OcrModel::Cyrillic);
        app.model_panel.update(cx, |panel, _| {
            panel.ocr_installations.push(ocr::Installed {
                config: mdoc_store::settings::OcrConfig {
                    model: OcrModel::Cyrillic,
                    ..Default::default()
                },
                models: "models".into(),
                pdfium: "pdfium".into(),
                onnx: "onnx".into(),
            })
        });
        app.ocr_action(false, window, cx);
        assert!(app.import.recognizing());
        set_default_ocr(app, OcrModel::V6Small);
        assert_eq!(app.recognition_model(), OcrModel::Cyrillic);
    });
    cx.run_until_parked();
    // English setup claimed the shared state; Settings moves to Cyrillic
    // before it finishes. The tab resumes with English and clears Installing.
    app.update_in(cx, |app, window, cx| {
        assert!(!app.import.busy());
        app.import.set_pick(None);
        app.ocr_state = OcrState::Installing;
        app.import.set_setup(Some((OcrModel::V6Small, true)));
        app.import
            .job_mut()
            .defer_for_setup(app.session.generation, "scan.pdf".into());
        set_default_ocr(app, OcrModel::Cyrillic);
        app.finish_ocr_setup(
            Ok(ocr::Installed {
                config: mdoc_store::settings::OcrConfig::default(),
                models: "models".into(),
                pdfium: "pdfium".into(),
                onnx: "onnx".into(),
            }),
            window,
            cx,
        );
        // The new Settings model's state comes from what Settings knows.
        assert!(matches!(
            &app.ocr_state,
            OcrState::Ready(i) if i.config.model == OcrModel::Cyrillic
        ));
        assert!(app.import.recognizing(), "resumed with the captured model");
    });
    cx.run_until_parked();
    app.update_in(cx, |app, _, _| {
        assert!(!app.import.busy());
        assert!(!app.ocr_state.busy());
    });
}

#[gpui::test]
fn ocr_runtime_failure_enables_setup_retry(cx: &mut TestAppContext) {
    let (app, cx) = boot(cx);
    app.update_in(cx, |app, window, cx| {
        app.import.job_mut().complete(
            app.session.generation,
            Err(import::ImportError::OcrFailed(import::OcrFailure::Engine(
                "runtime missing".into(),
            ))),
        );
        app.resume_import(window, cx);
        assert!(
            matches!(&app.ocr_state, OcrState::Failed(error) if error.contains("runtime missing"))
        );
        assert!(app.error.as_ref().unwrap().contains("runtime missing"));
        assert!(!app.import.busy());
    });
}

#[gpui::test]
fn ocr_without_text_stays_with_its_document(cx: &mut TestAppContext) {
    let (app, cx) = boot(cx);
    app.update_in(cx, |app, window, cx| {
        app.ocr_state = OcrState::Ready(mdoc_import::ocr::Installed {
            config: mdoc_store::settings::OcrConfig::default(),
            models: "models".into(),
            pdfium: "pdfium".into(),
            onnx: "onnx".into(),
        });
        app.import.set_source(Some("scan.pdf".into()));
        app.import
            .job_mut()
            .complete(app.session.generation, Err(import::ImportError::OcrNoText));
        app.resume_import(window, cx);
        assert!(matches!(app.ocr_state, OcrState::Ready(_)));
        assert!(app.import.ocr_error().unwrap().contains("no usable text"));
        assert!(app.error.as_ref().unwrap().contains("no usable text"));
        assert!(app.import.ocr_required().is_some());
        assert!(!app.import.busy());
    });
}

#[gpui::test]
fn import_action_converts_and_saves_without_touching_source(cx: &mut TestAppContext) {
    let dir = tempfile::tempdir().unwrap();
    let source = dir.path().join("table.csv");
    let output = dir.path().join("table.md");
    let original = "Name,Count\nApples,2\n";
    std::fs::write(&source, original).unwrap();
    let (app, cx) = boot(cx);
    app.update_in(cx, |app, window, cx| {
        app.open_source(source.clone(), window, cx)
    });
    cx.run_until_parked();
    cx.dispatch_action(Import);
    cx.run_until_parked();
    let markdown = cx.update(|_, cx| {
        let app = app.read(cx);
        assert!(!app.import.busy());
        assert!(app.dirty(cx));
        assert!(app.session.document.path.is_none());
        assert_eq!(app.save_directory(), dir.path());
        app.editor.read(cx).text().to_owned()
    });
    assert!(markdown.contains("| Apples | 2 |"));
    cx.dispatch_action(Save);
    cx.run_until_parked();
    cx.simulate_new_path_selection(|_| Some(output.clone()));
    cx.run_until_parked();
    assert_eq!(std::fs::read_to_string(output).unwrap(), markdown);
    assert_eq!(std::fs::read_to_string(source).unwrap(), original);
    assert!(!cx.update(|_, cx| app.read(cx).dirty(cx)));
}

#[gpui::test]
fn import_completion_protects_edits_and_cancel_preserves_pdf(cx: &mut TestAppContext) {
    let dir = tempfile::tempdir().unwrap();
    let source = dir.path().join("source.pdf");
    std::fs::write(&source, b"placeholder").unwrap();
    let (app, cx) = boot(cx);
    cx.simulate_input("edits made during conversion");
    app.update_in(cx, |app, window, cx| {
        let mut result = converted(source.clone());
        result.is_pdf = true;
        assert!(app.import.job_mut().begin(false));
        app.import
            .job_mut()
            .complete(app.session.generation, Ok(result));
        app.resume_import(window, cx);
    });
    cx.run_until_parked();
    assert!(cx.has_pending_prompt());
    cx.simulate_prompt_answer("Cancel");
    cx.run_until_parked();
    cx.update(|_, cx| {
        let app = app.read(cx);
        assert_eq!(app.editor.read(cx).text(), "edits made during conversion");
        assert!(app.preview.pdf().is_none());
        assert!(app.session.source.is_none());
        assert!(!app.import.busy());
    });
}

#[gpui::test]
fn import_save_then_replace_preserves_original_edits(cx: &mut TestAppContext) {
    let dir = tempfile::tempdir().unwrap();
    let saved = dir.path().join("before.md");
    let (app, cx) = boot(cx);
    cx.simulate_input("save these edits");
    app.update_in(cx, |app, window, cx| {
        app.request(
            Next::Import(converted(dir.path().join("source.docx"))),
            window,
            cx,
        );
    });
    cx.run_until_parked();
    cx.simulate_prompt_answer("Save");
    cx.run_until_parked();
    cx.simulate_new_path_selection(|_| Some(saved.clone()));
    cx.run_until_parked();
    assert_eq!(std::fs::read_to_string(saved).unwrap(), "save these edits");
    cx.update(|_, cx| {
        let app = app.read(cx);
        assert_eq!(app.editor.read(cx).text(), "# Imported\n");
        assert!(app.dirty(cx));
        assert!(app.session.document.path.is_none());
    });
}

#[gpui::test]
fn import_waits_for_dialog_and_discards_result_after_document_change(cx: &mut TestAppContext) {
    let dir = tempfile::tempdir().unwrap();
    let (app, cx) = boot(cx);
    cx.simulate_input("before");
    let generation = cx.update(|_, cx| app.read(cx).session.generation);
    cx.dispatch_action(Close);
    cx.run_until_parked();
    app.update_in(cx, |app, window, cx| {
        assert!(app.import.job_mut().begin(false));
        app.import
            .job_mut()
            .complete(generation, Ok(converted(dir.path().join("source.docx"))));
        app.resume_import(window, cx);
        assert!(app.import.job_mut().has_pending());
    });
    cx.simulate_prompt_answer("Discard");
    cx.run_until_parked();
    // Deliver the delayed completion after the shell has released the tab.
    // The retained test handle lets us verify the old generation is rejected.
    app.update_in(cx, |app, window, cx| app.resume_import(window, cx));
    cx.run_until_parked();
    cx.update(|_, cx| {
        let app = app.read(cx);
        assert_eq!(app.editor.read(cx).text(), "before");
        assert!(app.session.source.is_none());
        assert!(!app.import.busy());
        assert!(!app.import.job().has_pending());
    });
    let next = active_document(&app, cx);
    assert!(cx.update(|_, cx| next.read(cx).editor.read(cx).text().is_empty()));
}

#[gpui::test]
fn import_completes_after_open_picker_cancel_and_refuses_second_job(cx: &mut TestAppContext) {
    let dir = tempfile::tempdir().unwrap();
    let (app, cx) = boot(cx);
    cx.dispatch_action(Open);
    cx.run_until_parked();
    app.update_in(cx, |app, window, cx| {
        assert!(app.import.job_mut().begin(false));
        app.import.job_mut().complete(
            app.session.generation,
            Ok(converted(dir.path().join("source.docx"))),
        );
        app.start_import(dir.path().join("missing.pdf"), window, cx);
        app.resume_import(window, cx);
        assert!(app.import.job_mut().has_pending());
    });
    cx.simulate_path_prompt_response(|_| None);
    cx.run_until_parked();
    cx.update(|_, cx| {
        let app = app.read(cx);
        assert_eq!(app.editor.read(cx).text(), "# Imported\n");
        assert!(!app.import.busy());
        assert!(app.error.is_none());
    });
}

#[gpui::test]
fn imported_warning_survives_save_and_stays_with_its_tab_until_dismissed(cx: &mut TestAppContext) {
    let dir = tempfile::tempdir().unwrap();
    let (app, cx) = boot(cx);
    app.update_in(cx, |app, window, cx| {
        app.proceed(
            Next::Import(converted(dir.path().join("source.docx"))),
            window,
            cx,
        );
        app.write(dir.path().join("output.md"), None, cx);
    });
    // The window checks for another tab with the file, then saves.
    cx.run_until_parked();
    cx.update(|_, cx| {
        let app = app.read(cx);
        assert_eq!(
            app.session.document.path,
            Some(session_store::identity(&dir.path().join("output.md")))
        );
        assert!(app.session.warning.is_some());
    });
    cx.dispatch_action(DismissImportWarning);
    cx.run_until_parked();
    assert!(cx.update(|_, cx| app.read(cx).session.warning.is_none()));
    app.update(cx, |app, _| {
        app.session.warning = Some("warning".into());
    });
    let next = new_document(&app, cx);
    cx.update(|_, cx| {
        assert!(next.read(cx).session.warning.is_none());
        assert!(next.read(cx).session.source.is_none());
        assert_eq!(app.read(cx).session.warning.as_deref(), Some("warning"));
    });
}

#[gpui::test]
fn empty_import_is_unsaved_and_source_cannot_be_overwritten(cx: &mut TestAppContext) {
    let dir = tempfile::tempdir().unwrap();
    let source = dir.path().join("disguised.md");
    std::fs::write(&source, "source bytes").unwrap();
    let (app, cx) = boot(cx);
    app.update_in(cx, |app, window, cx| {
        let mut result = converted(source.clone());
        result.markdown.clear();
        app.proceed(Next::Import(result), window, cx);
        assert!(app.dirty(cx));
        // The window checks for another tab with the file, then saves.
        app.write(source.clone(), None, cx);
    });
    cx.run_until_parked();
    app.update(cx, |app, cx| {
        assert!(
            app.error
                .as_ref()
                .unwrap()
                .contains("preserve the imported source")
        );
        assert!(app.dirty(cx));
    });
    assert_eq!(std::fs::read_to_string(source).unwrap(), "source bytes");
}

#[gpui::test]
fn import_failure_and_stale_completion_keep_current_document(cx: &mut TestAppContext) {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("other.md");
    std::fs::write(&path, "other").unwrap();
    let (app, cx) = boot(cx);
    cx.simulate_input("keep edits");
    app.update_in(cx, |app, window, cx| {
        assert!(app.import.job_mut().begin(false));
        app.import.job_mut().complete(
            app.session.generation,
            Err(import::ImportError::ConverterNeedsOcr),
        );
        app.resume_import(window, cx);
        assert_eq!(app.editor.read(cx).text(), "keep edits");
        assert_eq!(
            app.error,
            Some(import::ImportError::ConverterNeedsOcr.to_string())
        );
        let generation = app.session.generation;
        let mut replacement = converted(path);
        replacement.markdown = "other".into();
        app.proceed(Next::Import(replacement), window, cx);
        assert!(app.import.job_mut().begin(false));
        app.import
            .job_mut()
            .complete(generation, Ok(converted(dir.path().join("source.docx"))));
        app.resume_import(window, cx);
        assert_eq!(app.editor.read(cx).text(), "other");
        assert!(app.error.is_none());
        assert!(!app.import.busy());
    });
}
