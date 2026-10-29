//! Read-only DOCX comments beside the Source pane.
use super::{Driver, Result};
use std::{path::PathBuf, sync::Arc};

/// The public DOCX comments fixture supplies the actual Source pane and its
/// full-width comments toggle, including the content card's clip.
pub(super) fn scenes(d: &mut Driver) -> Result<()> {
    let source =
        PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/docx-preview/comments.docx");
    let preview = mdoc_import::docx_preview::render_in_process(
        &source,
        Arc::new(std::sync::atomic::AtomicBool::new(false)),
    )
    .map_err(|e| e.to_string())?;
    let comments_view = d.change(|w, window, cx| {
        w.new_tab(window, cx);
        let view = w.active_view().unwrap();
        view.update(cx, |v, cx| {
            v.begin_preview(source);
            v.load_preview(preview.pdf_path.clone(), Some(preview), window, cx);
        });
        view
    })?;
    d.settle()?;
    d.change(|_, window, cx| {
        let view = comments_view.read(cx);
        assert!(
            view.preview.comment_panel().is_some(),
            "comments fixture loaded"
        );
        let region = view.comments_region.clone();
        window.focus(&region, cx);
    })?;
    d.key("tab")?;
    let toggle = d.change(|_, window, cx| window.focused(cx).expect("comments toggle"))?;
    d.focus_pair("comments-toggle", toggle)?;
    d.capture("comments")
}
