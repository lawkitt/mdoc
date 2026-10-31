//! Narrow windows (ADR 0027): long names sharing their start, truncated in
//! the middle; a sidebar dragged wide; and the minimum window, where the
//! sidebar collapses by itself until the user expands it.
use super::{Driver, HEIGHT, Result, WIDTH};
use crate::document_view::DocumentView;
use gpui::Entity;
use std::path::Path;

const NAMES: [&str; 3] = [
    "Settlement agreement Smith v Jones draft 2.md",
    "Settlement agreement Smith v Jones draft 3.md",
    "Settlement agreement Smith v Jones final.md",
];

pub(super) fn scenes(d: &mut Driver, view: &Entity<DocumentView>, folder: &Path) -> Result<()> {
    let mut paths = Vec::new();
    for name in NAMES {
        let path = folder.join(name);
        std::fs::write(
            &path,
            "# Settlement agreement\n\nThe parties agree as follows.\n",
        )
        .map_err(|e| e.to_string())?;
        paths.push(path);
    }
    d.change(|w, window, cx| {
        w.drop_files(&gpui::ExternalPaths(paths.into()), false, window, cx);
    })?;
    d.settle()?;
    d.resize(1000., 700.)?;
    d.change(|w, window, cx| {
        w.sidebar.restore(true, Some(true));
        window.focus(&w.focus, cx);
        cx.notify();
    })?;
    d.capture("narrow-sidebar-names")?;
    // As if the edge were dragged to its widest; dragging is a GUI check.
    d.change(|w, _, cx| {
        w.sidebar.restore_width(Some(320));
        cx.notify();
    })?;
    d.capture("sidebar-wide")?;
    d.change(|w, _, cx| {
        w.sidebar.restore_width(None);
        cx.notify();
    })?;
    // The minimum window with Replacements open: the sidebar collapses by
    // itself and gives the editor its width.
    d.change(|w, window, cx| {
        let id = w
            .tabs
            .iter()
            .find(|tab| tab.view() == Some(view))
            .map(|tab| tab.id())
            .expect("the fixture tab");
        w.activate(id, window, cx);
        view.update(cx, |v, cx| {
            if !v.replacements_open() {
                v.toggle_replacements_panel(window, cx);
            }
        });
        window.focus(&w.focus, cx);
    })?;
    d.resize(720., 480.)?;
    d.capture("narrow-minimum")?;
    // Expanded by the user at that size, it stays open.
    d.change(|_, window, cx| window.dispatch_action(Box::new(super::super::ToggleSidebar), cx))?;
    d.capture("narrow-minimum-sidebar")?;
    d.resize(WIDTH, HEIGHT)?;
    d.change(|w, _, cx| {
        w.sidebar.restore(true, Some(true));
        cx.notify();
    })?;
    d.settle()
}
