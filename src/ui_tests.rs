use super::*;
use gpui::{TestAppContext, VisualTestContext};

fn boot(cx: &mut TestAppContext) -> (Entity<Workspace>, &mut VisualTestContext) {
    cx.update(mdoc_editor::bind_keys);
    let (app, cx) = cx.add_window_view(Workspace::new);
    cx.run_until_parked();
    (app, cx)
}

#[gpui::test]
fn editing_save_and_new_roundtrip(cx: &mut TestAppContext) {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("note.md");
    let (app, cx) = boot(cx);
    cx.simulate_input("# Hello");
    assert!(cx.update(|_, cx| app.read(cx).dirty(cx)));
    cx.dispatch_action(Save);
    cx.run_until_parked();
    assert!(cx.did_prompt_for_new_path());
    cx.simulate_new_path_selection(|_| Some(path.clone()));
    cx.run_until_parked();
    assert_eq!(std::fs::read_to_string(&path).unwrap(), "# Hello");
    assert!(!cx.update(|_, cx| app.read(cx).dirty(cx)));
    cx.dispatch_action(New);
    cx.run_until_parked();
    assert!(cx.update(|_, cx| app.read(cx).editor.read(cx).text().is_empty()));
    cx.dispatch_action(Open);
    cx.run_until_parked();
    cx.simulate_path_prompt_response(|_| Some(vec![path]));
    cx.run_until_parked();
    assert_eq!(
        cx.update(|_, cx| app.read(cx).editor.read(cx).text().to_owned()),
        "# Hello"
    );
}

#[gpui::test]
fn new_cancel_and_discard_protect_edits(cx: &mut TestAppContext) {
    let (app, cx) = boot(cx);
    cx.simulate_input("keep me");
    cx.dispatch_action(New);
    cx.run_until_parked();
    assert!(cx.has_pending_prompt());
    cx.simulate_prompt_answer("Cancel");
    cx.run_until_parked();
    assert_eq!(
        cx.update(|_, cx| app.read(cx).editor.read(cx).text().to_owned()),
        "keep me"
    );
    cx.dispatch_action(New);
    cx.run_until_parked();
    cx.simulate_prompt_answer("Discard");
    cx.run_until_parked();
    assert!(cx.update(|_, cx| app.read(cx).editor.read(cx).text().is_empty()));
}

#[gpui::test]
fn cancelling_save_as_does_not_continue_new(cx: &mut TestAppContext) {
    let (app, cx) = boot(cx);
    cx.simulate_input("keep me");
    cx.dispatch_action(New);
    cx.run_until_parked();
    cx.simulate_prompt_answer("Save");
    cx.run_until_parked();
    cx.simulate_new_path_selection(|_| None);
    cx.run_until_parked();
    assert!(cx.update(|_, cx| app.read(cx).dirty(cx)));
    assert_eq!(
        cx.update(|_, cx| app.read(cx).editor.read(cx).text().to_owned()),
        "keep me"
    );
}

#[gpui::test]
fn pdf_open_keeps_markdown_and_close_pdf_restores_editor(cx: &mut TestAppContext) {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("invalid.pdf");
    std::fs::write(&path, b"not a PDF").unwrap();
    let (app, cx) = boot(cx);
    cx.simulate_input("keep me");
    app.update_in(cx, |app, window, cx| app.open_path(path, window, cx));
    cx.run_until_parked();
    assert!(cx.update(|_, cx| { app.read(cx).pdf.is_none() && app.read(cx).preview_retryable }));
    assert_eq!(
        cx.update(|_, cx| app.read(cx).editor.read(cx).text().to_owned()),
        "keep me"
    );
    cx.dispatch_action(ClosePdf);
    cx.run_until_parked();
    assert!(cx.update(|_, cx| app.read(cx).pdf.is_none()));
}

#[gpui::test]
fn failed_open_preserves_current_document(cx: &mut TestAppContext) {
    let dir = tempfile::tempdir().unwrap();
    let (app, cx) = boot(cx);
    app.update_in(cx, |app, window, cx| {
        app.open_path(dir.path().join("missing.md"), window, cx)
    });
    assert!(cx.update(|_, cx| app.read(cx).error.is_some()));
    assert!(cx.update(|_, cx| app.read(cx).document.path.is_none()));
}

#[gpui::test]
fn failed_save_does_not_discard_document(cx: &mut TestAppContext) {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("note.md");
    std::fs::write(&path, "original").unwrap();
    let (app, cx) = boot(cx);
    app.update_in(cx, |app, window, cx| {
        app.open_path(path.clone(), window, cx)
    });
    cx.simulate_input("my edits");
    std::fs::write(&path, "external edit").unwrap();
    cx.dispatch_action(New);
    cx.run_until_parked();
    cx.simulate_prompt_answer("Save");
    cx.run_until_parked();
    assert!(cx.update(|_, cx| app.read(cx).dirty(cx)));
    assert!(cx.update(|_, cx| app.read(cx).error.is_some()));
    assert_eq!(std::fs::read_to_string(path).unwrap(), "external edit");
}

#[test]
fn reference_pdf_parses_and_renders() {
    let path =
        std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/reference.pdf");
    let bytes = std::sync::Arc::new(std::fs::read(path).unwrap());
    let pdf = gpui_pdf::parse(bytes).unwrap();
    assert_eq!(gpui_pdf::page_dims(&pdf), vec![(420.0, 595.0)]);
    assert!(gpui_pdf::render_page(&pdf, 0, 1.0).is_ok());
}

#[gpui::test]
fn opening_and_saving_preserves_markdown_bytes(cx: &mut TestAppContext) {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("source.md");
    let source = "\u{feff}# Title\r\n\r\nWords $$x + y$$ more words.\r\n\tIndented\r\n";
    std::fs::write(&path, source).unwrap();
    let (app, cx) = boot(cx);
    app.update_in(cx, |app, window, cx| {
        app.open_path(path.clone(), window, cx)
    });
    assert!(!cx.update(|_, cx| app.read(cx).dirty(cx)));
    cx.dispatch_action(Save);
    cx.run_until_parked();
    assert_eq!(std::fs::read_to_string(path).unwrap(), source);
}

fn converted(source: PathBuf) -> import::Imported {
    import::Imported {
        source,
        markdown: "# Imported\n".into(),
        warning: Some("Partial import: pages 2 of 2 require OCR and were skipped.".into()),
        is_pdf: false,
        is_docx: false,
    }
}

#[gpui::test]
fn import_action_converts_and_saves_without_touching_source(cx: &mut TestAppContext) {
    let dir = tempfile::tempdir().unwrap();
    let source = dir.path().join("table.csv");
    let output = dir.path().join("table.md");
    let original = "Name,Count\nApples,2\n";
    std::fs::write(&source, original).unwrap();
    let (app, cx) = boot(cx);
    cx.dispatch_action(Import);
    cx.run_until_parked();
    cx.simulate_path_prompt_response(|_| Some(vec![source.clone()]));
    cx.run_until_parked();
    let markdown = cx.update(|_, cx| {
        let app = app.read(cx);
        assert!(!app.importing);
        assert!(app.dirty(cx));
        assert!(app.document.path.is_none());
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
        app.importing = true;
        app.pending_import = Some((app.document_generation, Ok(result)));
        app.resume_import(window, cx);
    });
    cx.run_until_parked();
    assert!(cx.has_pending_prompt());
    cx.simulate_prompt_answer("Cancel");
    cx.run_until_parked();
    cx.update(|_, cx| {
        let app = app.read(cx);
        assert_eq!(app.editor.read(cx).text(), "edits made during conversion");
        assert!(app.pdf.is_none());
        assert!(app.import_source.is_none());
        assert!(!app.importing);
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
        assert!(app.document.path.is_none());
    });
}

#[gpui::test]
fn import_waits_for_dialog_and_discards_result_after_document_change(cx: &mut TestAppContext) {
    let dir = tempfile::tempdir().unwrap();
    let (app, cx) = boot(cx);
    cx.simulate_input("before");
    let generation = cx.update(|_, cx| app.read(cx).document_generation);
    cx.dispatch_action(New);
    cx.run_until_parked();
    app.update_in(cx, |app, window, cx| {
        app.importing = true;
        app.pending_import = Some((generation, Ok(converted(dir.path().join("source.docx")))));
        app.resume_import(window, cx);
        assert!(app.pending_import.is_some());
    });
    cx.simulate_prompt_answer("Discard");
    cx.run_until_parked();
    cx.update(|_, cx| {
        let app = app.read(cx);
        assert!(app.editor.read(cx).text().is_empty());
        assert!(app.import_source.is_none());
        assert!(!app.importing);
        assert!(app.pending_import.is_none());
    });
}

#[gpui::test]
fn import_completes_after_open_picker_cancel_and_refuses_second_job(cx: &mut TestAppContext) {
    let dir = tempfile::tempdir().unwrap();
    let (app, cx) = boot(cx);
    cx.dispatch_action(Open);
    cx.run_until_parked();
    app.update_in(cx, |app, window, cx| {
        app.importing = true;
        app.pending_import = Some((
            app.document_generation,
            Ok(converted(dir.path().join("source.docx"))),
        ));
        app.start_import(dir.path().join("missing.pdf"), window, cx);
        app.resume_import(window, cx);
        assert!(app.pending_import.is_some());
    });
    cx.simulate_path_prompt_response(|_| None);
    cx.run_until_parked();
    cx.update(|_, cx| {
        let app = app.read(cx);
        assert_eq!(app.editor.read(cx).text(), "# Imported\n");
        assert!(!app.importing);
        assert!(app.error.is_none());
    });
}

#[gpui::test]
fn imported_warning_survives_save_and_clears_on_dismiss_or_new(cx: &mut TestAppContext) {
    let dir = tempfile::tempdir().unwrap();
    let (app, cx) = boot(cx);
    app.update_in(cx, |app, window, cx| {
        app.proceed(
            Next::Import(converted(dir.path().join("source.docx"))),
            window,
            cx,
        );
        app.write(dir.path().join("output.md"), None, window, cx);
        assert!(app.import_warning.is_some());
    });
    cx.dispatch_action(DismissImportWarning);
    cx.run_until_parked();
    assert!(cx.update(|_, cx| app.read(cx).import_warning.is_none()));
    app.update_in(cx, |app, window, cx| {
        app.import_warning = Some("warning".into());
        app.request(Next::New, window, cx);
        assert!(app.import_warning.is_none());
        assert!(app.import_source.is_none());
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
        app.write(source.clone(), None, window, cx);
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
        app.importing = true;
        app.pending_import = Some((app.document_generation, Err("requires OCR".into())));
        app.resume_import(window, cx);
        assert_eq!(app.editor.read(cx).text(), "keep edits");
        assert_eq!(app.error.as_deref(), Some("requires OCR"));
        let generation = app.document_generation;
        app.proceed(Next::Open(path), window, cx);
        app.importing = true;
        app.pending_import = Some((generation, Ok(converted(dir.path().join("source.docx")))));
        app.resume_import(window, cx);
        assert_eq!(app.editor.read(cx).text(), "other");
        assert!(app.error.is_none());
        assert!(!app.importing);
    });
}

#[gpui::test]
fn accepted_pdf_import_opens_source_pane(cx: &mut TestAppContext) {
    let source =
        std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/reference.pdf");
    let (app, cx) = boot(cx);
    app.update_in(cx, |app, window, cx| {
        let mut imported = converted(source);
        imported.is_pdf = true;
        app.proceed(Next::Import(imported), window, cx);
        assert!(app.preview_loading);
        assert!(app.dirty(cx));
    });
    cx.run_until_parked();
}

#[gpui::test]
fn accepted_docx_import_opens_source_pane(cx: &mut TestAppContext) {
    let source =
        std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/import/text.docx");
    let (app, cx) = boot(cx);
    app.update_in(cx, |app, window, cx| {
        let mut imported = converted(source);
        imported.is_docx = true;
        app.proceed(Next::Import(imported), window, cx);
        assert!(app.dirty(cx));
    });
    cx.run_until_parked();
    assert!(cx.update(|_, cx| {
        let app = app.read(cx);
        app.pdf.is_some() && app.docx_preview.is_some()
    }));
}

#[gpui::test]
fn failed_docx_preview_keeps_retry_source(cx: &mut TestAppContext) {
    let dir = tempfile::tempdir().unwrap();
    let source = dir.path().join("missing.docx");
    let (app, cx) = boot(cx);
    app.update_in(cx, |app, window, cx| {
        app.open_path(source.clone(), window, cx);
    });
    cx.run_until_parked();
    assert!(cx.update(|_, cx| {
        let app = app.read(cx);
        app.pdf.is_none()
            && app.preview_retryable
            && app.preview_source.as_deref() == Some(source.as_path())
    }));
    cx.dispatch_action(RetryPreview);
    cx.run_until_parked();
    assert!(cx.update(|_, cx| app.read(cx).preview_retryable));
}

#[gpui::test]
fn failed_replacement_preserves_loaded_preview_and_markdown(cx: &mut TestAppContext) {
    let source = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/reference.pdf");
    let (app, cx) = boot(cx);
    cx.simulate_input("unsaved text");
    app.update_in(cx, |app, window, cx| app.open_path(source, window, cx));
    cx.run_until_parked();
    let old = cx.update(|_, cx| app.read(cx).pdf.clone().unwrap());
    for missing in ["missing.pdf", "missing.docx"] {
        app.update_in(cx, |app, window, cx| {
            app.open_path(PathBuf::from(missing), window, cx)
        });
        cx.run_until_parked();
        cx.update(|_, cx| {
            let app = app.read(cx);
            assert_eq!(app.pdf.as_ref(), Some(&old));
            assert!(app.preview_retryable);
            assert_eq!(app.editor.read(cx).text(), "unsaved text");
            assert!(app.dirty(cx));
        });
    }
}

#[gpui::test]
fn accepted_import_failure_clears_old_preview_and_pdf_retry_works(cx: &mut TestAppContext) {
    let dir = tempfile::tempdir().unwrap();
    let source = dir.path().join("source.pdf");
    let reference = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/reference.pdf");
    let (app, cx) = boot(cx);
    app.update_in(cx, |app, window, cx| {
        app.open_path(reference.clone(), window, cx)
    });
    cx.run_until_parked();
    app.update_in(cx, |app, window, cx| {
        let mut imported = converted(source.clone());
        imported.is_pdf = true;
        app.proceed(Next::Import(imported), window, cx);
        assert!(app.pdf.is_none());
        assert!(app.dirty(cx));
    });
    cx.run_until_parked();
    assert!(cx.update(|_, cx| app.read(cx).preview_retryable));
    std::fs::copy(reference, &source).unwrap();
    cx.dispatch_action(RetryPreview);
    cx.run_until_parked();
    cx.update(|_, cx| {
        let app = app.read(cx);
        assert!(app.pdf.as_ref().unwrap().read(cx).is_loaded());
        assert!(!app.preview_retryable);
        assert!(app.dirty(cx));
    });
}

#[gpui::test]
fn close_and_newer_request_discard_pending_completions(cx: &mut TestAppContext) {
    let base = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let docx = base.join("tests/fixtures/import/text.docx");
    let pdf = base.join("tests/fixtures/reference.pdf");
    let (app, cx) = boot(cx);
    app.update_in(cx, |app, window, cx| {
        app.open_docx(docx.clone(), window, cx);
        app.close_preview(window, cx);
    });
    cx.run_until_parked();
    cx.update(|_, cx| {
        let app = app.read(cx);
        assert!(app.pdf.is_none());
        assert!(app.preview_source.is_none());
        assert!(app.preview_message.is_none());
        assert!(!app.preview_loading);
    });
    app.update_in(cx, |app, window, cx| {
        app.open_docx(docx, window, cx);
        app.open_pdf(pdf.clone(), window, cx);
        app.proceed(Next::New, window, cx);
    });
    cx.run_until_parked();
    cx.update(|_, cx| {
        let app = app.read(cx);
        assert_eq!(app.preview_source.as_ref(), Some(&pdf));
        assert!(app.pdf.as_ref().unwrap().read(cx).is_loaded());
        assert!(app.docx_preview.is_none());
    });
}

/// CPU frame budget through the real workspace, including its editor host.
#[gpui::test]
#[ignore]
fn large_markdown_scroll_budget(cx: &mut TestAppContext) {
    let base = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let file = std::env::var("MD_PERF_FILE")
        .unwrap_or_else(|_| "adcourt-legal-opinion-meruna-trading-tax-and-14-pages.docx".into());
    let source = base.join("tests/fixtures/docx-preview").join(file);
    let imported = import::convert(&source).unwrap();
    let (app, cx) = boot(cx);
    cx.simulate_resize(gpui::size(px(1100.), px(750.)));
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("large.md");
    std::fs::write(&path, &imported.markdown).unwrap();
    app.update_in(cx, |app, window, cx| {
        app.proceed(Next::Open(path), window, cx)
    });
    cx.run_until_parked();
    let mut failures = Vec::new();
    for state in ["markdown_only", "preview_open", "preview_closed"] {
        if state == "preview_open" {
            app.update_in(cx, |app, window, cx| {
                app.open_docx(source.clone(), window, cx)
            });
        } else if state == "preview_closed" {
            app.update_in(cx, |app, window, cx| app.close_preview(window, cx));
        }
        cx.run_until_parked();
        cx.update(|_, cx| {
            assert_eq!(app.read(cx).pdf.is_some(), state == "preview_open");
        });
        let mut times = Vec::new();
        let mut furthest = 0.0_f32;
        for frame in 0..60 {
            let started = std::time::Instant::now();
            cx.update(|window, cx| {
                app.update(cx, |app, cx| {
                    let step = if frame < 30 { frame } else { 59 - frame };
                    let offset = app.scroll.max_offset().y.abs() * (step as f32 / 29.);
                    app.scroll.set_offset(gpui::point(px(0.), -offset));
                    cx.notify();
                });
                window.refresh();
                window.draw(cx).clear(cx);
            });
            let ms = started.elapsed().as_secs_f64() * 1000.;
            furthest = furthest.max(cx.update(|_, cx| -f32::from(app.read(cx).scroll.offset().y)));
            cx.run_until_parked();
            if frame >= 5 {
                times.push(ms);
            }
        }
        assert!(furthest > 100., "viewport must actually scroll");
        times.sort_by(f64::total_cmp);
        let p95 = times[times.len() * 95 / 100];
        eprintln!(
            "MD_PERF state={state} bytes={} lines={} median_ms={:.2} p95_ms={p95:.2}",
            imported.markdown.len(),
            imported.markdown.lines().count(),
            times[times.len() / 2]
        );
        if p95 > 16.667 {
            failures.push(state);
        }
        // Measure actual text input followed by layout/paint, not just idle redraw.
        app.update(cx, |app, cx| {
            app.editor.update(cx, |e, cx| e.set_cursor(0, cx))
        });
        let mut typing = Vec::new();
        for _ in 0..12 {
            let started = std::time::Instant::now();
            cx.simulate_input("x");
            cx.update(|window, cx| {
                window.refresh();
                window.draw(cx).clear(cx);
            });
            typing.push(started.elapsed().as_secs_f64() * 1000.);
            cx.run_until_parked();
        }
        typing.sort_by(f64::total_cmp);
        let typing_p95 = typing[typing.len() * 95 / 100];
        eprintln!("MD_PERF state={state} typing_p95_ms={typing_p95:.2}");
        if typing_p95 > 50. {
            failures.push(state);
        }
    }
    cx.update(|_, cx| {
        assert_eq!(
            app.read(cx).editor.read(cx).text(),
            format!("{}{}", "x".repeat(36), imported.markdown)
        );
    });
    assert!(
        failures.is_empty(),
        "editor CPU budget exceeded: {failures:?}"
    );
}

#[gpui::test]
fn comments_follow_preview_lifecycle(cx: &mut TestAppContext) {
    let base = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let (app, cx) = boot(cx);
    cx.simulate_input("Markdown stays unchanged");
    app.update_in(cx, |app, window, cx| {
        app.open_docx(
            base.join("tests/fixtures/docx-preview/comments.docx"),
            window,
            cx,
        )
    });
    cx.run_until_parked();
    let panel = cx.update(|_, cx| {
        let app = app.read(cx);
        assert_eq!(app.docx_preview.as_ref().unwrap().comments.len(), 4);
        app.comment_panel.clone().unwrap()
    });
    app.update_in(cx, |app, window, cx| {
        app.open_docx(base.join("missing.docx"), window, cx)
    });
    cx.run_until_parked();
    cx.update(|_, cx| assert_eq!(app.read(cx).comment_panel.as_ref(), Some(&panel)));
    app.update_in(cx, |app, window, cx| {
        app.open_pdf(base.join("tests/fixtures/reference.pdf"), window, cx)
    });
    cx.run_until_parked();
    cx.update(|_, cx| assert!(app.read(cx).comment_panel.is_none()));
    app.update_in(cx, |app, window, cx| {
        app.open_docx(
            base.join("tests/fixtures/docx-preview/comments.docx"),
            window,
            cx,
        )
    });
    cx.run_until_parked();
    cx.dispatch_action(ClosePdf);
    cx.run_until_parked();
    cx.update(|_, cx| {
        let app = app.read(cx);
        assert!(app.comment_panel.is_none());
        assert!(app.docx_preview.is_none());
        assert_eq!(app.editor.read(cx).text(), "Markdown stays unchanged");
    });
}
