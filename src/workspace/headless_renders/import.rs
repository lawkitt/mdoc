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
    d.capture("import-hidden-note")?;
    photo(d)
}

/// A photo of a page, stored sideways with EXIF orientation 6 as a phone
/// writes it: the Original shows the image builder's PDF upright, from
/// memory, and the note asks for a check against the image. Then an
/// animated GIF: its first frame, and the note counting the others. No OCR
/// runs here: the Markdown stands in for what it would read.
fn photo(d: &mut Driver) -> Result<()> {
    let photo = sideways_page();
    let animation = animation();
    for (name, file, bytes, warning) in [
        (
            "import-photo",
            "Lease photo.jpg",
            photo,
            Some(
                "OCR review needed: pages 1 of 1 may be incomplete or inaccurate. Compare with the source image.",
            ),
        ),
        ("import-animation", "Signing day.gif", animation, None),
    ] {
        let built = mdoc_import::image_pdf::build(&[&bytes], mdoc_import::image_pdf::DEFAULT_DPI)
            .map_err(|e| e.to_string())?;
        let path = PathBuf::from(file);
        let imported = mdoc_import::import::Imported {
            ocr_configuration: None,
            source: mdoc_import::import::Source::new(path, None, built.pdf),
            markdown: "# Lease of 12 Fictional Street\n\nThe tenant, Ada Quillfeather, \
                       rents the flat from 1 March 2026.\n"
                .into(),
            warning: warning.map(str::to_owned),
            left_out: mdoc_import::import::LeftOut {
                animation_frames: built.frames_left_out,
                ..Default::default()
            },
            comments: Vec::new(),
            annotations: Vec::new(),
            is_pdf: true,
            is_docx: false,
            is_image: true,
        };
        d.change(|w, window, cx| {
            w.new_tab(window, cx);
            w.active_view().unwrap().update(cx, |v, cx| {
                v.preview.set_split(0.5);
                v.proceed(
                    crate::document_view::Next::Import(Box::new(imported)),
                    window,
                    cx,
                );
                cx.notify();
            });
        })?;
        d.settle()?;
        d.capture(name)?;
    }
    fax(d)
}

/// A three-page Group 3 fax (#1024 item 8): its Original shows the pages
/// as the fax coded them, passed through, turned to page 2 of 3, the
/// Russian page. No OCR runs here: the Markdown stands in for what it
/// would read.
fn fax(d: &mut Driver) -> Result<()> {
    let path = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/tiff/fax-g3-1d.tif");
    let bytes = std::fs::read(&path).map_err(|e| e.to_string())?;
    let built = mdoc_import::image_pdf::build(&[&bytes], mdoc_import::image_pdf::DEFAULT_DPI)
        .map_err(|e| e.to_string())?;
    let imported = mdoc_import::import::Imported {
        ocr_configuration: None,
        source: mdoc_import::import::Source::new(
            PathBuf::from("Supply agreement.tif"),
            None,
            built.pdf,
        ),
        markdown: "# SUPPLY AGREEMENT No. 17/26\n\nDate: 3 March 2026\n\n\
                   # ДОГОВОР ПОСТАВКИ № 17/26\n\nг. Казань, 3 марта 2026 г.\n\n\
                   # SIGNATURES\n"
            .into(),
        warning: None,
        left_out: Default::default(),
        comments: Vec::new(),
        annotations: Vec::new(),
        is_pdf: true,
        is_docx: false,
        is_image: true,
    };
    let view = d.change(|w, window, cx| {
        w.new_tab(window, cx);
        let view = w.active_view().unwrap();
        view.update(cx, |v, cx| {
            v.preview.set_split(0.5);
            v.proceed(
                crate::document_view::Next::Import(Box::new(imported)),
                window,
                cx,
            );
            cx.notify();
        });
        view
    })?;
    d.settle()?;
    let pdf = d.change(|_, _, cx| view.read(cx).preview.pdf().cloned())?;
    let pdf = pdf.ok_or("the fax's Original loads")?;
    d.change(|_, _, cx| pdf.update(cx, |pdf, cx| pdf.go_to_page(1, cx)))?;
    d.settle()?;
    d.capture("import-fax")?;
    heic(d)
}

/// A HEIC photo of a page stored sideways (EXIF orientation 6): the
/// Original shows it upright. On the hosted Mac ImageIO decodes the HEIC,
/// as an import does. Windows runners have neither HEIF nor HEVC
/// extension, so the Windows render shows the upright JPEG a WIC import
/// stores for the same photo (`page-orientation-6-decoded.jpg`, see the
/// fixtures' README). Then the card a Windows PC without the extensions
/// gets. No OCR runs here: the Markdown stands in.
fn heic(d: &mut Driver) -> Result<()> {
    let name = if cfg!(target_os = "macos") {
        "page-orientation-6.heic"
    } else {
        "page-orientation-6-decoded.jpg"
    };
    let path = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("tests/fixtures/heic")
        .join(name);
    let bytes = std::fs::read(&path).map_err(|e| e.to_string())?;
    let built = mdoc_import::image_pdf::build(&[&bytes], mdoc_import::image_pdf::DEFAULT_DPI)
        .map_err(|e| e.to_string())?;
    let imported = mdoc_import::import::Imported {
        ocr_configuration: None,
        source: mdoc_import::import::Source::new(PathBuf::from("Lease page.heic"), None, built.pdf),
        markdown: "# Lease of 12 Fictional Street\n\nThe tenant, Ada Quillfeather, \
                   rents the flat from 1 March 2026.\n"
            .into(),
        warning: None,
        left_out: Default::default(),
        comments: Vec::new(),
        annotations: Vec::new(),
        is_pdf: true,
        is_docx: false,
        is_image: true,
        decrypted: false,
    };
    d.change(|w, window, cx| {
        w.new_tab(window, cx);
        w.active_view().unwrap().update(cx, |v, cx| {
            v.preview.set_split(0.5);
            v.proceed(
                crate::document_view::Next::Import(Box::new(imported)),
                window,
                cx,
            );
            cx.notify();
        });
    })?;
    d.settle()?;
    d.capture("import-heic")?;
    // The extensions missing: the import's own error, as the card shows it.
    d.change(|w, window, cx| {
        w.new_tab(window, cx);
        w.active_view().unwrap().update(cx, |v, cx| {
            v.lifecycle.show_source();
            v.preview.no_preview(PathBuf::from("Lease page.heic"));
            v.error = Some(
                mdoc_import::import::ImportError::Image(
                    mdoc_import::image_pdf::ImageError::HeicCodecMissing(
                        mdoc_import::image_pdf::heic::MissingCodec::HeifExtension,
                    ),
                )
                .to_string(),
            );
            cx.notify();
        });
    })?;
    d.capture("import-heic-missing")
}

/// A page photographed sideways: a white sheet with a heading bar and
/// lines of "text" on a grey desk, stored turned a quarter anticlockwise
/// with EXIF orientation 6, so only the orientation turns it upright.
fn sideways_page() -> Vec<u8> {
    let (width, height) = (900u32, 1200u32);
    let upright = image::RgbImage::from_fn(width, height, |x, y| {
        let sheet = (90..810).contains(&x) && (80..1120).contains(&y);
        let heading = sheet && (150..750).contains(&x) && (150..200).contains(&y);
        let line =
            sheet && (150..750).contains(&x) && (260..1050).contains(&y) && (y - 260) % 60 < 14;
        image::Rgb(if heading || line {
            [40, 40, 48]
        } else if sheet {
            [250, 249, 245]
        } else {
            [150, 140, 128]
        })
    });
    // Stored a quarter anticlockwise, which orientation 6 undoes.
    let stored = image::imageops::rotate270(&upright);
    let mut jpeg = Vec::new();
    image::DynamicImage::ImageRgb8(stored)
        .write_with_encoder(image::codecs::jpeg::JpegEncoder::new_with_quality(
            &mut jpeg, 90,
        ))
        .expect("encoding in memory");
    let mut exif = b"Exif\x00\x00II*\x00\x08\x00\x00\x00\x01\x00".to_vec();
    for field in [0x0112u16, 3] {
        exif.extend_from_slice(&field.to_le_bytes());
    }
    exif.extend_from_slice(&1u32.to_le_bytes());
    exif.extend_from_slice(&6u32.to_le_bytes());
    exif.extend_from_slice(&0u32.to_le_bytes());
    let mut segment = vec![0xff, 0xe1];
    segment.extend_from_slice(&(exif.len() as u16 + 2).to_be_bytes());
    segment.extend_from_slice(&exif);
    [&jpeg[..2], &segment, &jpeg[2..]].concat()
}

/// A three-frame GIF: a teal, an amber and a violet card.
fn animation() -> Vec<u8> {
    let mut gif = Vec::new();
    {
        let mut encoder = image::codecs::gif::GifEncoder::new(&mut gif);
        let frames = [
            [20, 140, 140, 255],
            [220, 150, 30, 255],
            [120, 60, 170, 255],
        ]
        .map(|color| {
            image::Frame::new(image::RgbaImage::from_fn(480, 320, |x, y| {
                let card = (60..420).contains(&x) && (60..260).contains(&y);
                image::Rgba(if card { color } else { [245, 245, 245, 255] })
            }))
        });
        encoder.encode_frames(frames).expect("encoding in memory");
    }
    gif
}
