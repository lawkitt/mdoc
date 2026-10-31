//! The DOCX Source pane of the Russian demo: its ruble signs draw as ₽, not
//! as empty boxes, where the substituted fonts lack them (#305).
use super::{Driver, Result};
use std::{path::PathBuf, sync::Arc};

pub(super) fn scenes(d: &mut Driver) -> Result<()> {
    let source = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("docs/product-media/Соглашение об урегулировании.docx");
    let preview = mdoc_import::docx_preview::render_in_process(
        &source,
        Arc::new(std::sync::atomic::AtomicBool::new(false)),
    )
    .map_err(|e| e.to_string())?;
    d.change(|w, window, cx| {
        w.new_tab(window, cx);
        let view = w.active_view().unwrap();
        view.update(cx, |v, cx| {
            v.begin_preview(source);
            v.load_preview(preview.pdf_path.clone(), Some(preview), window, cx);
        });
    })?;
    d.capture("docx-source")
}
