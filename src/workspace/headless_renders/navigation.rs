//! Home, the document's first open and Back/Forward (ADR 0081).
use super::{Driver, Result};
use crate::document_view::DocumentView;
use gpui::Entity;
use std::path::Path;

/// Opens the fixture document and returns its view for later scenes.
pub(super) fn scenes(d: &mut Driver, path: &Path) -> Result<Entity<DocumentView>> {
    d.capture("home")?;
    d.change(|w, _, cx| {
        w.sidebar.restore(false, Some(false));
        cx.notify();
    })?;
    d.capture("navigation-disabled")?;
    d.change(|w, _, cx| {
        w.sidebar.restore(true, Some(true));
        cx.notify();
    })?;
    let view = document(d, path)?;
    d.capture("document")?;
    d.change(|w, window, cx| {
        w.sidebar.restore(false, Some(false));
        window.focus(&w.focus, cx);
        cx.notify();
    })?;
    d.capture("navigation-enabled")?;
    // The document is in the middle: Home behind it and a second tab ahead.
    d.change(|w, window, cx| {
        w.new_tab(window, cx);
        w.navigate(true, window, cx);
        window.focus(&w.focus, cx);
    })?;
    d.capture("navigation-middle")?;
    let (back, forward) = d.change(|w, _, _| (w.back_focus.clone(), w.forward_focus.clone()))?;
    d.focus_pair("navigation-back", back)?;
    d.focus_pair("navigation-forward", forward)?;
    d.change(|w, _, cx| {
        w.sidebar.restore(true, Some(true));
        cx.notify();
    })?;
    Ok(view)
}

fn document(d: &mut Driver, path: &Path) -> Result<Entity<DocumentView>> {
    // Use the same admission and identity normalization as an OS file drop.
    // Injecting Document::open into a blank tab bypasses its identity and can
    // leave /var and /private/var spellings on opposite sides of a case join.
    d.change(|w, window, cx| {
        w.drop_files(
            &gpui::ExternalPaths(vec![path.to_path_buf()].into()),
            false,
            window,
            cx,
        );
    })?;
    d.settle()?;
    d.change(|w, window, cx| {
        window.focus(&w.focus, cx);
        w.active_view().filter(|view| {
            let view = view.read(cx);
            !view.lifecycle.loading() && view.session.document.path.is_some()
        })
    })?
    .ok_or_else(|| "fixture drop did not open its document".into())
}
