//! The comment rail beside the DOCX Source page: the rail
//! in a wide pane with one card expanded, a resolved thread and "Not on the
//! page", markers in a 640×480 window with a card open, and hidden text
//! shown with its header toggle.
use crate::workspace::headless_renders::{Driver, HEIGHT, Result, WIDTH};
use gpui::Entity;
use mdoc_import::docx_preview::{self, RenderOptions};
use std::{path::PathBuf, sync::Arc};

/// The public comments fixture, with a reply, a resolved thread, a comment
/// with no place and hidden text.
fn render(options: RenderOptions) -> Result<(PathBuf, docx_preview::DocxPreview)> {
    let source = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../../tests/fixtures/docx-preview/comments.docx");
    let preview = docx_preview::render_in_process(
        &docx_preview::read_source(&source).map_err(|e| e.to_string())?,
        Arc::new(std::sync::atomic::AtomicBool::new(false)),
        crate::document::preview::read_comment_marks,
        options,
        None,
    )
    .map_err(|e| e.to_string())?;
    Ok((source, preview))
}

fn rail(
    d: &mut Driver,
    view: &Entity<crate::document::DocumentView>,
) -> Result<Entity<crate::document::comment_rail::CommentRail>> {
    d.change(|_, _, cx| {
        view.read(cx)
            .preview
            .comment_rail()
            .cloned()
            .ok_or_else(|| "the comments fixture has comments".to_string())
    })?
}

pub(in crate::workspace::headless_renders) fn scenes(d: &mut Driver) -> Result<()> {
    // Wide enough for the rail beside the page.
    d.resize(1500., 900.)?;
    let (source, preview) = render(RenderOptions::default())?;
    let view = d.change(|w, window, cx| {
        w.new_tab(window, cx);
        let view = w.active_view().unwrap();
        view.update(cx, |v, cx| {
            v.preview.set_split(0.25);
            v.begin_preview(source.clone());
            v.load_docx(source, preview, window, cx);
        });
        view
    })?;
    d.settle()?;
    let rail = rail(d, &view)?;
    d.change(|_, _, cx| {
        rail.update(cx, |rail, cx| rail.show_open(Some("0"), false, cx));
    })?;
    d.capture("comments-rail")?;
    // The focused card: only the accent ring changes.
    let card = d.change(|_, _, cx| rail.read(cx).card_focus(0))?;
    d.focus_pair("comment-card", card)?;
    d.change(|w, window, cx| window.focus(&w.focus, cx))?;
    d.change(|_, _, cx| {
        rail.update(cx, |rail, cx| rail.show_open(Some("4"), true, cx));
    })?;
    d.capture("comments-resolved")?;
    // A 640×480 window: markers, one card open under its line.
    d.resize(640., 480.)?;
    d.change(|_, _, cx| {
        // The window shows one pane at a time: the Source.
        view.update(cx, |v, cx| {
            v.original_selected = true;
            cx.notify();
        });
        rail.update(cx, |rail, cx| rail.show_open(Some("0"), false, cx));
    })?;
    d.capture("comments-markers")?;
    d.change(|_, _, cx| {
        view.update(cx, |v, cx| {
            v.original_selected = false;
            cx.notify();
        })
    })?;
    d.resize(1500., 900.)?;
    // Hidden text shown: the setting, and the page drawn with it.
    let (source, preview) = render(RenderOptions {
        show_hidden_text: true,
    })?;
    d.change(|w, window, cx| {
        let _ = w.update_general(|general| general.show_hidden_text = true, cx);
        view.update(cx, |v, cx| {
            v.begin_preview(source.clone());
            v.load_docx(source, preview, window, cx);
        });
    })?;
    d.capture("comments-hidden-text")?;
    d.change(|w, _, cx| {
        let _ = w.update_general(|general| general.show_hidden_text = false, cx);
    })?;
    d.resize(WIDTH, HEIGHT)
}
