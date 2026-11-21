//! Import states shown in a tab: a failed conversion, OCR consent, and the
//! notes for a DOCX's comments and an RTF's hidden text left out. The first
//! two are set directly; the notes come from converting public fixtures
//! in-process, so no worker, model download or native runtime runs.
use super::{Driver, Result};
use std::{path::PathBuf, sync::Arc, sync::atomic::AtomicBool};

pub(super) fn scenes(d: &mut Driver) -> Result<()> {
    // A damaged DOCX: the conversion and preview failures as their panes
    // state them.
    d.change(|w, window, cx| {
        w.new_tab(window, cx);
        w.active_view().unwrap().update(cx, |v, cx| {
            let source = PathBuf::from("Damaged agreement.docx");
            v.lifecycle.show_source();
            v.preview.show_failure(
                source,
                crate::preview::docx_message(&mdoc_import::docx_preview::PreviewError::Package(
                    "invalid ZIP".into(),
                ))
                .to_string(),
            );
            v.error = Some(
                "This document could not be converted because its contents are damaged or incomplete."
                    .into(),
            );
            cx.notify();
        });
    })?;
    d.capture("conversion-failed")?;
    // Cancel pressed on a running conversion (#950): the card says so and
    // the toolbar offers Try again.
    d.change(|w, window, cx| {
        w.new_tab(window, cx);
        w.active_view().unwrap().update(cx, |v, cx| {
            v.lifecycle.show_source();
            v.preview
                .no_preview(PathBuf::from("Quarterly figures.xlsx"));
            v.error = Some(mdoc_import::import::ImportError::Cancelled.to_string());
            cx.notify();
        });
    })?;
    d.capture("conversion-cancelled")?;
    // The consent card only.
    d.change(|w, window, cx| {
        w.new_tab(window, cx);
        w.active_view().unwrap().update(cx, |v, cx| {
            v.lifecycle.show_source();
            v.import.show_ocr_fixture(v.session.generation);
            v.resume_import(window, cx);
            v.ocr_state = crate::document_view::OcrState::Missing;
            cx.notify();
        });
    })?;
    d.capture("ocr-consent")?;
    // A locked document's card asks for its password, then again after a
    // wrong one (#1007). The field holds a typed password: only bullets
    // show.
    for (name, file, wrong) in [
        ("import-password", "Settlement terms.docx", false),
        ("import-password-wrong", "Board minutes.docx", true),
    ] {
        d.change(move |w, window, cx| {
            w.new_tab(window, cx);
            w.active_view().unwrap().update(cx, |v, cx| {
                let source = PathBuf::from(file);
                v.lifecycle.show_source();
                // The Original points to the conversion card's field.
                v.preview.show_password(source.clone(), false);
                v.import.show_password_fixture(
                    v.session.generation,
                    source.to_str().unwrap(),
                    wrong,
                );
                v.resume_import(window, cx);
                v.password_field.update(cx, |field, cx| {
                    field.set_value("fictional-pass".into(), cx);
                    field.move_to_end(cx);
                });
                cx.notify();
            });
        })?;
        d.capture(name)?;
    }
    locked_original(d)?;
    comments_note(d)
}

/// Word's locked DOCX (`tests/fixtures/locked-office`): unlocked on the
/// conversion card, its Original renders in memory (#1024 item 3); reopened,
/// its Markdown opens without the password and the Original asks for it,
/// then again after a wrong one (#1024 decision 6).
fn locked_original(d: &mut Driver) -> Result<()> {
    let fixture = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("tests/fixtures/locked-office/word-locked.docx");
    let password = mdoc_import::import::Password::new("Petrov-2026".into());
    let mut imported = mdoc_import::import::prepare_cancellable(
        &fixture,
        Some(&password),
        None,
        true,
        &AtomicBool::new(false),
    )
    .map_err(|e| e.to_string())?;
    let preview = mdoc_import::docx_preview::render_in_process(
        &imported.source,
        Arc::new(AtomicBool::new(false)),
        crate::preview::read_comment_marks,
        Default::default(),
        None,
    )
    .map_err(|e| e.to_string())?;
    let markdown = imported.markdown.clone();
    // No preview worker: the rendering above stands in for its answer.
    imported.is_docx = false;
    d.change(|w, window, cx| {
        w.new_tab(window, cx);
        w.active_view().unwrap().update(cx, |v, cx| {
            v.proceed(
                crate::document_view::Next::Import(Box::new(imported)),
                window,
                cx,
            );
            v.begin_preview(fixture.clone());
            v.load_docx(fixture.clone(), preview, window, cx);
            cx.notify();
        });
    })?;
    d.settle()?;
    d.capture("original-unlocked")?;
    for (name, wrong) in [
        ("original-password", false),
        ("original-password-wrong", true),
    ] {
        let (fixture, markdown) = (fixture.clone(), markdown.clone());
        d.change(move |w, window, cx| {
            w.new_tab(window, cx);
            w.active_view().unwrap().update(cx, |v, cx| {
                v.editor.update(cx, |e, cx| e.set_text(markdown, cx));
                v.preview.show_password(fixture, wrong);
                v.original_password_field.update(cx, |field, cx| {
                    field.set_value("fictional-pass".into(), cx);
                    field.move_to_end(cx);
                });
                cx.notify();
            });
        })?;
        d.capture(name)?;
    }
    Ok(())
}

/// A DOCX whose comments and deleted changes name people: the Markdown
/// holds neither and the note counts the comments left out (recognition
/// grill 2026-11-01, round 3). The preview refuses tracked changes, so the
/// Original pane shows that failure, as the worker would report it.
fn comments_note(d: &mut Driver) -> Result<()> {
    let fixture = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("tests/fixtures/import/handmade-hidden-content.docx");
    let mut imported = mdoc_import::import::prepare_cancellable(
        &fixture,
        None,
        None,
        true,
        &AtomicBool::new(false),
    )
    .map_err(|e| e.to_string())?;
    let Err(failure) = mdoc_import::docx_preview::render_in_process(
        &imported.source,
        Arc::new(AtomicBool::new(false)),
        crate::preview::read_comment_marks,
        Default::default(),
        None,
    ) else {
        return Err("the preview renders tracked changes now; show it instead".into());
    };
    let source = imported.source.path.clone();
    // No preview worker: the failure above stands in for its answer.
    imported.is_docx = false;
    d.change(|w, window, cx| {
        w.new_tab(window, cx);
        w.active_view().unwrap().update(cx, |v, cx| {
            v.proceed(
                crate::document_view::Next::Import(Box::new(imported)),
                window,
                cx,
            );
            v.preview
                .show_failure(source, crate::preview::docx_message(&failure).to_string());
            cx.notify();
        });
    })?;
    d.capture("import-comments-note")?;
    hidden_text_note(d)
}

/// An RTF with hidden-formatted passages: the Markdown holds none of them
/// and the left-out line counts them (#925 decision 9). RTF has no preview, so the
/// Original pane stays empty.
fn hidden_text_note(d: &mut Driver) -> Result<()> {
    let fixture =
        PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/import/handmade-hidden.rtf");
    let imported = mdoc_import::import::prepare_cancellable(
        &fixture,
        None,
        None,
        true,
        &AtomicBool::new(false),
    )
    .map_err(|e| e.to_string())?;
    d.change(|w, window, cx| {
        w.new_tab(window, cx);
        w.active_view().unwrap().update(cx, |v, cx| {
            v.proceed(
                crate::document_view::Next::Import(Box::new(imported)),
                window,
                cx,
            );
            cx.notify();
        });
    })?;
    d.capture("import-hidden-note")
}
