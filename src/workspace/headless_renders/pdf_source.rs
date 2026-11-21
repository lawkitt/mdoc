//! A PDF's annotations on the comment rail beside its Source page: the
//! generated fixture with a sticky note, a FreeText box, a highlight with a
//! comment and a reply, a resolved note with a reply, and markup, its own
//! drawing of them hidden. The rail with the highlight's thread open, the
//! resolved note open, the turned second page, markers in a 640×480 window,
//! and the plain page with comments off, where the stamp, ink and the
//! uncommented highlight stay. Then the same agreement encrypted with
//! AES-256, opened from its bytes with its password, beside its open twin
//! opened the same way: the same pages, the PDF's own comment drawing hidden.
use super::{Driver, HEIGHT, Result, WIDTH};
use gpui_pdf::PdfSource;
use std::{path::PathBuf, sync::Arc};

pub(super) fn scenes(d: &mut Driver) -> Result<()> {
    d.resize(1500., 900.)?;
    let source =
        PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/pdf-source/comments.pdf");
    let read = mdoc_import::import::Source::read(&source).map_err(|e| e.to_string())?;
    let annotations = mdoc_import::import::source_annotations(&read, None).unwrap_or_default();
    let view = d.change(|w, window, cx| {
        w.new_tab(window, cx);
        let view = w.active_view().unwrap();
        view.update(cx, |v, cx| {
            v.preview.set_split(0.25);
            v.preview.imported(&read, Vec::new(), annotations);
            v.open_pdf(source, window, cx);
        });
        view
    })?;
    d.settle()?;
    let rail = d.change(|_, _, cx| {
        view.read(cx)
            .preview
            .comment_rail()
            .cloned()
            .ok_or_else(|| "the PDF fixture has annotations".to_string())
    })??;
    let pdf = d.change(|_, _, cx| view.read(cx).preview.pdf().cloned())?;
    let pdf = pdf.ok_or("the PDF fixture loads")?;
    // The highlight's thread open, with its reply.
    d.change(|_, _, cx| {
        rail.update(cx, |rail, cx| rail.show_open(Some("2"), false, cx));
    })?;
    d.capture("pdf-comments-rail")?;
    d.change(|_, _, cx| {
        rail.update(cx, |rail, cx| rail.show_open(Some("0"), false, cx));
    })?;
    d.capture("pdf-comments-resolved")?;
    // The second page, turned a quarter, with its highlight open.
    d.change(|_, _, cx| {
        pdf.update(cx, |pdf, cx| pdf.go_to_page(1, cx));
        rail.update(cx, |rail, cx| rail.show_open(Some("8"), false, cx));
    })?;
    d.capture("pdf-comments-rotated")?;
    // A 640×480 window: markers, the highlight's card open under its line.
    d.change(|_, _, cx| {
        pdf.update(cx, |pdf, cx| pdf.go_to_page(0, cx));
    })?;
    d.resize(640., 480.)?;
    d.change(|_, _, cx| {
        view.update(cx, |v, cx| {
            v.original_selected = true;
            cx.notify();
        });
        rail.update(cx, |rail, cx| rail.show_open(Some("2"), false, cx));
    })?;
    d.capture("pdf-comments-markers")?;
    d.change(|_, _, cx| {
        view.update(cx, |v, cx| {
            v.original_selected = false;
            cx.notify();
        })
    })?;
    d.resize(1500., 900.)?;
    // Comments off: the plain page, without the PDF's own comment drawing.
    let show = |d: &mut Driver, comments: bool| {
        d.change(|w, window, cx| {
            let _ = w.update_general(move |general| general.show_comments = comments, cx);
            view.update(cx, |v, cx| v.source_display_changed(window, cx));
        })
    };
    show(d, false)?;
    d.capture("pdf-comments-hidden")?;
    show(d, true)?;
    d.resize(WIDTH, HEIGHT)
}

/// The fixtures' published test password (tests/fixtures/pdf-source).
const PASSWORD: &str = "Petrov-2026";

/// The encrypted agreement and its open twin in the Original, each opened
/// from bytes in memory: comments hidden on the page, as no rail reads an
/// encrypted file's annotations yet.
pub(super) fn encrypted(d: &mut Driver) -> Result<()> {
    let dir = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/pdf-source");
    for (name, file, password) in [
        ("pdf-encrypted-twin", "comments.pdf", None),
        ("pdf-encrypted", "comments-aes256.pdf", Some(PASSWORD)),
    ] {
        let path = dir.join(file);
        let bytes = Arc::new(std::fs::read(&path).map_err(|e| e.to_string())?);
        let source = PdfSource::Bytes {
            bytes: bytes.into(),
            password: password.map(|p| Arc::new(p.to_owned()) as gpui_pdf::Password),
        };
        let view = d.change(|w, window, cx| {
            w.new_tab(window, cx);
            let view = w.active_view().unwrap();
            view.update(cx, |v, cx| {
                v.preview.set_split(0.25);
                v.begin_preview(path.clone());
                v.load_preview_source(path, source, None, window, cx);
            });
            view
        })?;
        d.settle()?;
        d.change(|_, _, cx| {
            let pdf = view.read(cx).preview.pdf().ok_or("the PDF opens")?.read(cx);
            match (
                pdf.is_loaded(),
                pdf.is_locked(),
                pdf.options().hide_comment_annotations,
            ) {
                (true, false, true) => Ok(()),
                state => Err(format!(
                    "{name}: loaded, locked, comments hidden: {state:?}"
                )),
            }
        })??;
        d.capture(name)?;
    }
    d.resize(WIDTH, HEIGHT)
}

/// A locked PDF its tab unlocked (#1024 item 4): the Original opens the
/// file with the tab's password, in memory, and the rail reads its
/// annotations with it, so the page leaves them to the rail.
pub(super) fn unlocked(d: &mut Driver) -> Result<()> {
    d.resize(1500., 900.)?;
    let path = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("tests/fixtures/pdf-source/comments-aes256.pdf");
    let view = d.change(|w, window, cx| {
        w.new_tab(window, cx);
        let view = w.active_view().unwrap();
        view.update(cx, |v, cx| {
            v.preview.set_split(0.4);
            v.import
                .show_unlock_fixture(mdoc_import::import::Unlock::Password {
                    path: path.clone(),
                    password: mdoc_import::import::Password::new(PASSWORD.into()),
                });
            v.open_pdf(path, window, cx);
        });
        view
    })?;
    d.settle()?;
    d.change(|_, _, cx| {
        let view = view.read(cx);
        let pdf = view.preview.pdf().ok_or("the PDF opens")?.read(cx);
        match (
            pdf.is_loaded(),
            pdf.is_locked(),
            view.preview.comment_rail().is_some(),
        ) {
            (true, false, true) => Ok(()),
            state => Err(format!("pdf-unlocked: loaded, locked, rail: {state:?}")),
        }
    })??;
    d.capture("pdf-unlocked")?;
    d.resize(WIDTH, HEIGHT)
}

/// A locked PDF's password cards (#1024 item 4), each with a typed password
/// that shows only as bullets: the conversion card asking and after a wrong
/// password, the Original pointing to it; then a reopened tab's Original
/// asking on its own card, and after a wrong password.
pub(super) fn locked(d: &mut Driver) -> Result<()> {
    let path = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("tests/fixtures/pdf-source/comments-aes256.pdf");
    for (name, wrong) in [
        ("import-password-pdf", false),
        ("import-password-pdf-wrong", true),
    ] {
        let path = path.clone();
        d.change(move |w, window, cx| {
            w.new_tab(window, cx);
            w.active_view().unwrap().update(cx, |v, cx| {
                v.lifecycle.show_source();
                v.preview.set_split(0.5);
                v.import
                    .show_password_fixture(v.session.generation, path.to_str().unwrap(), wrong);
                v.resume_import(window, cx);
                v.open_pdf(path, window, cx);
                v.password_field.update(cx, |field, cx| {
                    field.set_value("fictional-pass".into(), cx);
                    field.move_to_end(cx);
                });
                cx.notify();
            });
        })?;
        d.settle()?;
        d.capture(name)?;
    }
    for (name, typed) in [
        ("pdf-original-password", None),
        ("pdf-original-password-wrong", Some("fictional-pass")),
    ] {
        let view = d.change(|w, window, cx| {
            w.new_tab(window, cx);
            let view = w.active_view().unwrap();
            view.update(cx, |v, cx| {
                v.preview.set_split(0.5);
                v.open_pdf(path.clone(), window, cx);
            });
            view
        })?;
        d.settle()?;
        if let Some(typed) = typed {
            d.change(|_, window, cx| {
                view.update(cx, |v, cx| {
                    v.password_field
                        .update(cx, |field, cx| field.set_value(typed.into(), cx));
                    v.unlock_original(window, cx);
                })
            })?;
            d.settle()?;
        }
        d.change(|_, _, cx| {
            view.update(cx, |v, cx| {
                v.password_field.update(cx, |field, cx| {
                    field.set_value("fictional-pass".into(), cx);
                    field.move_to_end(cx);
                });
            })
        })?;
        let expected = if typed.is_some() {
            crate::document_view::PasswordPrompt::Wrong
        } else {
            crate::document_view::PasswordPrompt::Asking
        };
        d.change(|_, _, cx| match view.read(cx).preview.locked() {
            Some(prompt) if prompt == expected => Ok(()),
            state => Err(format!("{name}: the Original asks: {state:?}")),
        })??;
        d.capture(name)?;
    }
    d.resize(WIDTH, HEIGHT)
}
