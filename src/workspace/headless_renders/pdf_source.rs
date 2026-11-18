//! A PDF's annotations on the comment rail beside its Source page
//! (originals spec, Comments in Source): the generated fixture with a sticky
//! note, a FreeText box, a highlight with a comment and a reply, a resolved
//! note with a reply, and markup, its own drawing of them hidden. The rail
//! with the highlight's thread open, the resolved note open, the turned
//! second page, markers in a 640×480 window, and the plain page with
//! comments off, where the stamp, ink and the uncommented highlight stay.
use super::{Driver, HEIGHT, Result, WIDTH};
use std::path::PathBuf;

pub(super) fn scenes(d: &mut Driver) -> Result<()> {
    d.resize(1500., 900.)?;
    let source =
        PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/pdf-source/comments.pdf");
    let annotations = mdoc_import::import::source_annotations(&source).unwrap_or_default();
    let view = d.change(|w, window, cx| {
        w.new_tab(window, cx);
        let view = w.active_view().unwrap();
        view.update(cx, |v, cx| {
            v.preview.set_split(0.25);
            let stamp = mdoc_import::import::SourceStamp::of(&source);
            v.preview
                .imported(source.clone(), stamp, Vec::new(), annotations);
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
