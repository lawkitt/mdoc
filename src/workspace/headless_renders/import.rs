//! Import states shown in a tab: a failed conversion and OCR consent. Both
//! are set directly, so no converter, worker, model download or native
//! runtime runs.
use super::{Driver, Result};
use std::path::PathBuf;

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
    d.capture("ocr-consent")
}
