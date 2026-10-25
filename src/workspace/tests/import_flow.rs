//! Conversions, OCR consent, recognition languages and setup, seen from the window (`ImportFlow`).
use super::*;

#[gpui::test]
fn ocr_failure_requires_explicit_native_fallback_and_releases_conversion_slot(
    cx: &mut TestAppContext,
) {
    let dir = tempfile::tempdir().unwrap();
    let csv = dir.path().join("other.csv");
    std::fs::write(&csv, "name,count\npears,3\n").unwrap();
    let source = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("tests/fixtures/import/handmade-partly-scanned.pdf");
    let (tabs, cx) = boot(cx, Session::default());
    tabs.update_in(cx, |tabs, window, cx| {
        tabs.open_paths(vec![source, csv], window, cx)
    });
    cx.run_until_parked();
    let first = active(&tabs, cx);
    let first_id = cx.update(|_, cx| tabs.read(cx).tabs.active());
    first.update(cx, |view, _| {
        view.ocr_state = OcrState::Ready(ocr::Installed {
            config: mdoc_store::settings::OcrConfig::default(),
            models: "missing-models".into(),
            pdfium: "missing-pdfium".into(),
            onnx: "missing-onnx".into(),
        })
    });
    first.update_in(cx, |view, window, cx| view.ocr_action(false, window, cx));
    cx.run_until_parked();
    cx.update(|_, cx| {
        let view = first.read(cx);
        assert!(!view.lifecycle.prompting());
        assert!(!view.import.busy());
        assert!(view.import.ocr_required().is_some());
        assert!(view.lifecycle.source_only());
        assert!(view.editor.read(cx).text().is_empty());
        // The OCR runtime is bogus, so recognition fails. Its message varies:
        // a parallel test may hold the process-wide model-work permit, which
        // fails the same way before recognition starts.
        assert!(view.error.is_some(), "OCR failure is reported");
    });
    tabs.update_in(cx, |tabs, window, cx| tabs.cycle(1, window, cx));
    cx.run_until_parked();
    let second = active(&tabs, cx);
    second.update_in(cx, |view, window, cx| view.import(&Import, window, cx));
    cx.run_until_parked();
    cx.update(|_, cx| assert!(!second.read(cx).lifecycle.source_only()));
    first.update_in(cx, |view, window, cx| view.ocr_action(true, window, cx));
    cx.run_until_parked();
    cx.update(|_, cx| {
        assert_ne!(tabs.read(cx).tabs.active(), first_id);
        assert!(!first.read(cx).lifecycle.source_only());
        assert!(
            first
                .read(cx)
                .session
                .warning
                .as_ref()
                .unwrap()
                .contains("2, 5")
        );
        assert!(!first.read(cx).import.slot_busy());
    });
    second.update_in(cx, |view, window, cx| view.import(&Import, window, cx));
    cx.run_until_parked();
    cx.update(|_, cx| assert!(second.read(cx).editor.read(cx).text().contains("pears")));
}

#[gpui::test]
fn ocr_download_retains_card_focus(cx: &mut TestAppContext) {
    download_retains_card_focus(cx, settings::Model::Ocr(settings::OcrModel::V6Small));
}

#[gpui::test]
fn ocr_card_asks_once_offers_retry_and_settings_downloads_do_not_convert(cx: &mut TestAppContext) {
    let source = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("tests/fixtures/import/handmade-partly-scanned.pdf");
    let (tabs, cx) = boot(cx, Session::default());
    cx.simulate_resize(size(px(1100.), px(760.)));
    tabs.update_in(cx, |tabs, window, cx| {
        tabs.open_paths(vec![source], window, cx)
    });
    cx.run_until_parked();
    let view = active(&tabs, cx);
    let panel = cx.update(|_, cx| tabs.read(cx).settings.clone());
    let draw = |cx: &mut VisualTestContext| {
        cx.update(|window, cx| {
            window.refresh();
            window.draw(cx).clear(cx);
        })
    };
    view.update(cx, |view, cx| {
        view.ocr_state = OcrState::Missing;
        cx.notify();
    });
    draw(cx);
    cx.update(|_, cx| assert!(view.read(cx).import.ocr_required().is_some()));
    assert_eq!(
        mdoc_import::ocr::title(settings::OcrModel::V6Small),
        "English"
    );
    for selector in [
        "ocr-language-hint",
        "conversion-card",
        "Download & recognize",
        "Use native text only",
        "choose-ocr-model",
    ] {
        assert!(cx.debug_bounds(selector).is_some(), "{selector}");
    }
    for model in settings::OcrModel::ALL {
        view.update(cx, |view, cx| {
            view.pick_recognition(mdoc_import::ocr::languages(model).to_vec(), cx);
        });
        draw(cx);
        let model = settings::Model::Ocr(model);
        let label = format!(
            "{}: {}",
            model.name(),
            crate::models::attribution_label(model)
        );
        let bounds = cx
            .debug_bounds(Box::leak(label.into_boxed_str()))
            .expect("OCR source and license");
        let card = cx.debug_bounds("conversion-card").unwrap();
        assert!(bounds.top() >= card.top() && bounds.bottom() <= card.bottom());
    }
    view.update(cx, |view, cx| {
        view.pick_recognition(
            mdoc_import::ocr::languages(settings::OcrModel::V6Small).to_vec(),
            cx,
        );
    });
    // Setup progress replaces the actions with Cancel.
    view.update(cx, |view, cx| {
        view.ocr_state = OcrState::Installing;
        cx.notify();
    });
    draw(cx);
    assert!(cx.debug_bounds("cancel-ocr-setup").is_some());
    assert!(cx.debug_bounds("Download & recognize").is_none());
    // Cancel or failure returns to the card with Retry.
    view.update(cx, |view, cx| {
        view.ocr_state = OcrState::Failed("Download cancelled.".into());
        cx.notify();
    });
    draw(cx);
    assert!(cx.debug_bounds("Retry").is_some());
    // A download finished from Settings only enables one-click Run OCR.
    panel.update(cx, |_, cx| {
        cx.emit(settings_ui::Event::Finished(
            settings::Model::Ocr(settings::OcrModel::V6Small),
            Ok(Some(ocr::Installed {
                config: settings::OcrConfig::default(),
                models: "models".into(),
                pdfium: "pdfium".into(),
                onnx: "onnx".into(),
            })),
        ))
    });
    cx.run_until_parked();
    draw(cx);
    cx.update(|_, cx| {
        let view = view.read(cx);
        assert!(matches!(view.ocr_state, OcrState::Ready(_)));
        assert!(!view.import.busy());
        assert!(view.lifecycle.source_only());
        assert!(view.editor.read(cx).text().is_empty());
        assert!(!panel.read(cx).open);
    });
    assert!(cx.debug_bounds("Run OCR").is_some());
    click_toolbar(cx, "choose-ocr-model");
    cx.update(|_, cx| assert!(panel.read(cx).open));
}

/// ROADMAP 0.2.0 should-fix 1: the Russian OCR fixture gets English &
/// Russian by default and the English one stays English, with the reason
/// on the card, even on a computer with a Russian keyboard.
#[gpui::test]
fn ocr_fixtures_default_to_their_recognition_languages(cx: &mut TestAppContext) {
    use recognition::{ENGLISH, RUSSIAN};
    let fixtures =
        PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/ocr-qualification");
    let (tabs, cx) = boot(cx, Session::default());
    cx.simulate_resize(size(px(1100.), px(760.)));
    tabs.update(cx, |tabs, _| {
        tabs.input_languages = || vec![ENGLISH, RUSSIAN]
    });
    let russian = open_scan(&tabs, fixtures.join("russian.pdf"), cx);
    assert_eq!(
        recognition(&russian, cx),
        (
            vec![ENGLISH, RUSSIAN],
            "English & Russian: the file name mentions Russian".into(),
            settings::OcrModel::Cyrillic
        )
    );
    for selector in [
        "ocr-language-hint",
        "recognize English",
        "recognize English & Russian",
        "Download & recognize",
    ] {
        assert!(cx.debug_bounds(selector).is_some(), "{selector}");
    }
    let english = open_scan(&tabs, fixtures.join("english.pdf"), cx);
    assert_ne!(english, russian);
    assert_eq!(
        recognition(&english, cx),
        (
            vec![ENGLISH],
            "English: the file name mentions English".into(),
            settings::OcrModel::V6Small
        )
    );
    assert!(cx.debug_bounds("ocr-language-hint").is_some());
}

#[gpui::test]
fn recognition_defaults_follow_text_file_name_then_keyboard(cx: &mut TestAppContext) {
    use recognition::{ENGLISH, RUSSIAN};
    let fixtures = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures");
    let dir = tempfile::tempdir().unwrap();
    let raster = fixtures.join("ocr-qualification/english.pdf");
    for name in ["договор.pdf", "scan.pdf", "page.pdf"] {
        std::fs::copy(&raster, dir.path().join(name)).unwrap();
    }
    let (tabs, cx) = boot(cx, Session::default());
    cx.simulate_resize(size(px(1100.), px(760.)));
    let cyrillic = open_scan(&tabs, dir.path().join("договор.pdf"), cx);
    assert_eq!(
        recognition(&cyrillic, cx).1,
        "English & Russian: the file name is in Russian"
    );
    // No keyboard, text or file-name sign: the Settings choice (English).
    let plain = open_scan(&tabs, dir.path().join("page.pdf"), cx);
    assert_eq!(
        recognition(&plain, cx).1,
        "English: no sign of Russian in the text, file name or keyboard"
    );
    tabs.update(cx, |tabs, _| {
        tabs.input_languages = || vec![ENGLISH, RUSSIAN]
    });
    let keyboard = open_scan(&tabs, dir.path().join("scan.pdf"), cx);
    assert_eq!(
        recognition(&keyboard, cx),
        (
            vec![ENGLISH, RUSSIAN],
            "English & Russian: this computer has a Russian keyboard".into(),
            settings::OcrModel::Cyrillic
        )
    );
    // English text on the pages that need no OCR outranks the keyboard.
    let partly = open_scan(
        &tabs,
        fixtures.join("import/handmade-partly-scanned.pdf"),
        cx,
    );
    assert_eq!(
        recognition(&partly, cx),
        (
            vec![ENGLISH],
            "English: the text layer has no Russian text".into(),
            settings::OcrModel::V6Small
        )
    );
}

#[gpui::test]
fn picking_recognition_languages_changes_only_this_document(cx: &mut TestAppContext) {
    use recognition::ENGLISH;
    let fixtures =
        PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/ocr-qualification");
    let (tabs, cx) = boot(cx, Session::default());
    cx.simulate_resize(size(px(1100.), px(760.)));
    let english = open_scan(&tabs, fixtures.join("english.pdf"), cx);
    let view = open_scan(&tabs, fixtures.join("russian.pdf"), cx);
    let panel = cx.update(|_, cx| tabs.read(cx).settings.clone());
    let installed = |model| ocr::Installed {
        config: settings::OcrConfig {
            model,
            ..Default::default()
        },
        models: "missing-models".into(),
        pdfium: "missing-pdfium".into(),
        onnx: "missing-onnx".into(),
    };
    // English (the Settings model) is ready; English & Russian is not.
    tabs.update(cx, |tabs, cx| {
        tabs.set_ocr(OcrState::Ready(installed(settings::OcrModel::V6Small)), cx)
    });
    draw(cx);
    assert!(cx.debug_bounds("Download & recognize").is_some());
    click_toolbar(cx, "recognize English");
    assert_eq!(
        recognition(&view, cx),
        (
            vec![ENGLISH],
            "English: picked for this document".into(),
            settings::OcrModel::V6Small
        )
    );
    draw(cx);
    assert!(cx.debug_bounds("Run OCR").is_some());
    // Picking the detected list again restores its reason.
    click_toolbar(cx, "recognize English & Russian");
    assert_eq!(
        recognition(&view, cx).1,
        "English & Russian: the file name mentions Russian"
    );
    cx.update(|_, cx| {
        assert_eq!(
            english.read(cx).recognition_model(),
            settings::OcrModel::V6Small
        );
        assert_eq!(
            settings::Preferences::default().ocr.model,
            settings::OcrModel::V6Small
        );
    });
    // A checked English & Russian installation enables Run OCR. Its run
    // fails here (no runtime), and that failure stays with this document.
    panel.update(cx, |panel, _| {
        panel
            .ocr_installations
            .push(installed(settings::OcrModel::Cyrillic))
    });
    draw(cx);
    assert!(cx.debug_bounds("Run OCR").is_some());
    view.update_in(cx, |view, window, cx| view.ocr_action(false, window, cx));
    cx.run_until_parked();
    draw(cx);
    cx.update(|_, cx| {
        let view = view.read(cx);
        assert!(view.import.ocr_failure().is_some());
        assert!(matches!(view.ocr_state, OcrState::Ready(_)));
        assert!(matches!(english.read(cx).ocr_state, OcrState::Ready(_)));
        assert!(english.read(cx).import.ocr_failure().is_none());
    });
    assert!(cx.debug_bounds("Retry").is_some());
}

/// A tab set up the Settings model and was closed; Settings then chose
/// another model. The setup's end still clears the shared Installing.
#[gpui::test]
fn ocr_setup_ending_after_its_tab_closed_clears_installing(cx: &mut TestAppContext) {
    let fixtures =
        PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/ocr-qualification");
    let (tabs, cx) = boot(cx, Session::default());
    let view = open_scan(&tabs, fixtures.join("english.pdf"), cx);
    tabs.update(cx, |tabs, cx| tabs.set_ocr(OcrState::Installing, cx));
    let id = cx.update(|_, cx| tabs.read(cx).tabs.active());
    tabs.update_in(cx, |tabs, window, cx| tabs.close_tab(id, window, cx));
    cx.run_until_parked();
    drop(view);
    let panel = cx.update(|_, cx| tabs.read(cx).settings.clone());
    cx.update(|_, cx| {
        let prefs = tabs.read(cx).preferences.clone();
        prefs.borrow_mut().current.as_mut().unwrap().ocr.model = settings::OcrModel::Cyrillic;
    });
    panel.update(cx, |_, cx| {
        cx.emit(settings_ui::Event::Finished(
            settings::Model::Ocr(settings::OcrModel::V6Small),
            Err("Download cancelled.".into()),
        ))
    });
    cx.run_until_parked();
    cx.update(|_, cx| assert!(matches!(tabs.read(cx).ocr_state, OcrState::Missing)));
}

#[gpui::test]
fn closing_source_rejects_conversion_even_when_view_is_retained(cx: &mut TestAppContext) {
    let dir = tempfile::tempdir().unwrap();
    let source = dir.path().join("source.csv");
    std::fs::write(&source, "name,count\napples,2\n").unwrap();
    let (tabs, cx) = boot(cx, Session::default());
    tabs.update_in(cx, |tabs, window, cx| tabs.open_path(source, window, cx));
    cx.run_until_parked();
    let view = active(&tabs, cx);
    tabs.update_in(cx, |tabs, window, cx| {
        view.update(cx, |view, cx| view.import(&Import, window, cx));
        assert!(view.read(cx).import.busy());
        tabs.close_tab(tabs.tabs.active(), window, cx);
    });
    cx.run_until_parked();
    cx.update(|_, cx| {
        assert_eq!(tabs.read(cx).tabs.len(), 1);
        assert!(view.read(cx).lifecycle.source_only());
        assert!(view.read(cx).editor.read(cx).text().is_empty());
        assert!(!view.read(cx).import.busy());
        assert!(!tabs.read(cx).import_busy.load(Ordering::Relaxed));
    });
}

#[gpui::test]
fn conversion_stays_in_source_tab_and_ready_ocr_requires_consent(cx: &mut TestAppContext) {
    let source = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("tests/fixtures/import/handmade-partly-scanned.pdf");
    let original = std::fs::read(&source).unwrap();
    let (tabs, cx) = boot_home(cx, Session::default());
    tabs.update_in(cx, |tabs, window, cx| {
        tabs.open_path(source.clone(), window, cx)
    });
    cx.run_until_parked();
    let view = active(&tabs, cx);
    let pdf = cx.update(|_, cx| view.read(cx).preview.pdf().cloned().unwrap());
    view.update(cx, |view, _| {
        view.ocr_state = OcrState::Ready(ocr::Installed {
            config: mdoc_store::settings::OcrConfig::default(),
            models: "must-not-load".into(),
            pdfium: "must-not-load".into(),
            onnx: "must-not-load".into(),
        })
    });
    cx.dispatch_action(Import);
    cx.run_until_parked();
    cx.update(|_, cx| {
        assert!(!view.read(cx).lifecycle.prompting());
        assert_eq!(view.read(cx).import.ocr_required(), Some(&[2, 5][..]));
        assert!(view.read(cx).lifecycle.source_only());
        assert!(!view.read(cx).import.slot_busy());
        assert!(matches!(view.read(cx).ocr_state, OcrState::Ready(_)));
    });
    view.update_in(cx, |view, window, cx| view.ocr_action(true, window, cx));
    cx.run_until_parked();
    cx.update(|_, cx| {
        let view = view.read(cx);
        assert!(!view.lifecycle.source_only());
        assert!(view.dirty(cx));
        assert!(view.editor.read(cx).text().contains("Readable page three"));
        assert!(view.session.warning.as_ref().unwrap().contains("2, 5"));
        assert_eq!(view.preview.pdf(), Some(&pdf));
        assert!(!view.import.slot_busy());
        assert_eq!(tabs.read(cx).tabs.len(), 1);
        assert!(
            tabs.read(cx).snapshot(cx).tabs.is_empty(),
            "unsaved conversion is not persisted as a source-only tab"
        );
    });
    tabs.update_in(cx, |tabs, window, cx| {
        tabs.open_path(source.clone(), window, cx)
    });
    cx.run_until_parked();
    assert_eq!(active(&tabs, cx), view);
    cx.simulate_input("preserve edits");
    let text = cx.update(|_, cx| view.read(cx).editor.read(cx).text().to_owned());
    cx.dispatch_action(Import);
    cx.run_until_parked();
    cx.update(|_, cx| assert_eq!(view.read(cx).editor.read(cx).text(), text));
    assert_eq!(std::fs::read(source).unwrap(), original);
}

#[gpui::test]
fn background_import_finishes_in_its_own_tab_without_stealing_focus(cx: &mut TestAppContext) {
    let dir = tempfile::tempdir().unwrap();
    let source = dir.path().join("source.csv");
    std::fs::write(&source, "name,value\nalpha,42\n").unwrap();
    let (tabs, cx) = boot(cx, Session::default());
    cx.simulate_input("keep original");
    cx.run_until_parked();
    let original = active(&tabs, cx);
    let id = cx.update(|_, cx| tabs.read(cx).tabs.active());
    tabs.update_in(cx, |tabs, window, cx| {
        tabs.open_path(source, window, cx);
    });
    cx.run_until_parked();
    let imported = active(&tabs, cx);
    imported.update_in(cx, |view, window, cx| view.import(&Import, window, cx));
    tabs.update_in(cx, |tabs, window, cx| {
        tabs.activate(id, window, cx);
    });
    cx.run_until_parked();
    cx.update(|window, cx| {
        assert_eq!(tabs.read(cx).tabs.active(), id);
        assert!(
            original
                .read(cx)
                .editor
                .read(cx)
                .focus_handle(cx)
                .is_focused(window)
        );
        assert_eq!(original.read(cx).editor.read(cx).text(), "keep original");
        let imported = tabs.read(cx).tabs[1].view().unwrap().read(cx);
        assert!(imported.editor.read(cx).text().contains("alpha"));
        assert!(imported.dirty(cx));
        assert!(!imported.import.busy());
    });
}

#[gpui::test]
fn a_closed_tab_s_import_ending_starts_the_conversion_waiting_for_it(cx: &mut TestAppContext) {
    let (tabs, cx) = boot_home(cx, Session::default());
    let base = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/import");
    let permit = cx
        .update(|_, cx| import_session::ImportPermit::acquire(&tabs.read(cx).import_busy).unwrap());
    tabs.update_in(cx, |tabs, window, cx| {
        tabs.open_paths(vec![base.join("text.pdf")], window, cx)
    });
    cx.run_until_parked();
    let waiting = active(&tabs, cx);
    cx.update(|_, cx| assert!(waiting.read(cx).import.auto_pending()));
    // Another tab's import holds the slot; the tab closes before it ends.
    let closed = tabs.update_in(cx, |tabs, window, cx| tabs.new_tab(window, cx));
    tabs.update_in(cx, |tabs, window, cx| {
        let view = tabs
            .tabs
            .get(closed)
            .and_then(|tab| tab.view().cloned())
            .unwrap();
        view.update(cx, |view, cx| {
            let import = cx.background_executor().spawn(async move {
                drop(permit);
                crate::document_view::Background::Import
            });
            view.keep_background(import);
        });
        tabs.remove(closed, window, cx);
    });
    cx.run_until_parked();
    cx.update(|_, cx| {
        assert!(!waiting.read(cx).import.auto_pending());
        assert!(
            !waiting.read(cx).lifecycle.source_only(),
            "the waiting PDF converted"
        );
        assert!(!tabs.read(cx).import_busy.load(Ordering::Relaxed));
    });
}

#[gpui::test]
fn automatic_conversion_is_disposable_until_edited_but_can_be_saved(cx: &mut TestAppContext) {
    let (tabs, cx) = boot_home(cx, Session::default());
    let source = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/import/text.docx");
    tabs.update_in(cx, |tabs, window, cx| {
        tabs.open_path(source.clone(), window, cx)
    });
    cx.run_until_parked();
    let view = active(&tabs, cx);
    cx.update(|_, cx| {
        assert!(!view.read(cx).lifecycle.source_only());
        assert!(view.read(cx).lifecycle.generated_unedited());
        assert!(!view.read(cx).dirty(cx));
        assert!(!view.read(cx).editor.read(cx).text().is_empty());
        assert!(view.read(cx).preview.pdf().is_some());
        let snapshot = tabs.read(cx).snapshot(cx);
        assert!(snapshot.tabs[0].source_only);
        assert_eq!(snapshot.tabs[0].attachment.as_ref(), Some(&source));
    });
    cx.dispatch_action(Close);
    cx.run_until_parked();
    assert!(!cx.has_pending_prompt());
    tabs.update_in(cx, |tabs, window, cx| tabs.open_path(source, window, cx));
    cx.run_until_parked();
    let view = active(&tabs, cx);
    cx.simulate_input("edited ");
    cx.run_until_parked();
    cx.dispatch_action(Close);
    cx.run_until_parked();
    assert!(cx.has_pending_prompt());
    cx.simulate_prompt_answer("Cancel");
    cx.run_until_parked();
    let dir = tempfile::tempdir().unwrap();
    let saved = dir.path().join("converted.md");
    cx.dispatch_action(Save);
    cx.run_until_parked();
    cx.simulate_new_path_selection(|_| Some(saved.clone()));
    cx.run_until_parked();
    cx.update(|_, cx| {
        assert!(!view.read(cx).dirty(cx));
        assert_eq!(
            std::fs::read_to_string(&saved).unwrap(),
            view.read(cx).editor.read(cx).text()
        );
        // Recents now leads with the saved file (#76).
        let recents = &tabs.read(cx).shell.recents.files;
        assert_eq!(recents[0].path, session_store::identity(&saved));
    });
}

#[gpui::test]
fn automatic_conversions_queue_on_activation_and_closed_waiters_do_not_run(
    cx: &mut TestAppContext,
) {
    let (tabs, cx) = boot_home(cx, Session::default());
    let base = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/import");
    let permit = cx
        .update(|_, cx| import_session::ImportPermit::acquire(&tabs.read(cx).import_busy).unwrap());
    tabs.update_in(cx, |tabs, window, cx| {
        tabs.open_paths(
            vec![
                base.join("text.pdf"),
                base.join("text.docx"),
                base.join("handmade-partly-scanned.pdf"),
            ],
            window,
            cx,
        )
    });
    cx.run_until_parked();
    let first = active(&tabs, cx);
    cx.update(|_, cx| {
        assert!(first.read(cx).import.auto_pending());
        assert!(tabs.read(cx).tabs[1].view().is_none());
    });
    tabs.update_in(cx, |tabs, window, cx| tabs.cycle(1, window, cx));
    cx.run_until_parked();
    let closed = active(&tabs, cx);
    tabs.update_in(cx, |tabs, window, cx| {
        tabs.close_tab(tabs.tabs.active(), window, cx)
    });
    cx.run_until_parked();
    let selected = cx.update(|_, cx| tabs.read(cx).tabs.active());
    drop(permit);
    tabs.update_in(cx, |tabs, window, cx| tabs.refresh_import(window, cx));
    cx.run_until_parked();
    cx.update(|_, cx| {
        assert_eq!(tabs.read(cx).tabs.active(), selected);
        assert!(!first.read(cx).lifecycle.source_only());
        assert!(closed.read(cx).editor.read(cx).text().is_empty());
        assert!(closed.read(cx).import.cancelled());
        let view = tabs.read(cx).active_view().unwrap();
        assert_eq!(view.read(cx).import.ocr_required(), Some(&[2, 5][..]));
        assert!(!tabs.read(cx).import_busy.load(Ordering::Relaxed));
    });
    assert!(!cx.has_pending_prompt());
}
