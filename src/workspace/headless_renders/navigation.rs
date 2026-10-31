//! Home, the document's first open, Back/Forward (ADR 0081) and the tab menu.
use super::{Driver, Result};
use crate::document_view::DocumentView;
use gpui::{Entity, px};
use std::path::Path;

/// Opens the fixture document and returns its view for later scenes.
pub(super) fn scenes(d: &mut Driver, path: &Path) -> Result<Entity<DocumentView>> {
    d.capture("home")?;
    // Home with no cases and the header's bell: "Open a case…" ends under
    // the local note, and the Tab walk starts at Open files… (U25).
    let cases = d.change(|w, _, cx| {
        let cases = w.case_coordinator.cases().to_vec();
        w.case_coordinator.hide_for_locked_render();
        w.case_coordinator.restore_summaries(Vec::new());
        w.notices.post("Saved elsewhere");
        cx.notify();
        cases
    })?;
    d.settle()?;
    d.change(|w, window, cx| {
        w.title.notices_open.set(false);
        window.focus(&w.focus, cx);
        cx.notify();
    })?;
    d.capture("home-notice")?;
    d.change(|w, _, cx| {
        w.notices.dismiss();
        w.case_coordinator.finish_load(cases);
        cx.notify();
    })?;
    // The local note's explanation, as hovering it shows it (U16).
    d.change(|w, _, cx| {
        w.shell.local_note_hover = true;
        cx.notify();
    })?;
    d.capture("home-local-note")?;
    d.change(|w, _, cx| {
        w.shell.local_note_hover = false;
        cx.notify();
    })?;
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
    tab_menus(d)?;
    Ok(view)
}

/// The sidebar tab menu of the saved fixture and of the untitled tab beside
/// it (UI polish decision 13), opened under each row as a right-click does.
fn tab_menus(d: &mut Driver) -> Result<()> {
    for (name, untitled) in [("tab-menu-saved", false), ("tab-menu-untitled", true)] {
        d.change(|w, window, cx| {
            let rows: Vec<u64> = w
                .tabs
                .iter()
                .filter(|tab| !w.is_placeholder(tab, cx))
                .map(|tab| tab.id())
                .collect();
            let (row, id) = rows
                .iter()
                .enumerate()
                .find(|(_, id)| w.tab_file(**id, cx).is_none() == untitled)
                .map(|(row, id)| (row, *id))
                .ok_or("no such tab")?;
            // Under the row's label: rows are 31 pt apart from 160 pt.
            let position = gpui::point(px(48.), px(176. + 31. * row as f32));
            w.menu_return = window.focused(cx);
            window.focus(&w.menu_focus, cx);
            w.tab_menu = Some((id, position));
            cx.notify();
            Ok::<_, String>(())
        })??;
        d.capture(name)?;
        d.change(|w, window, cx| {
            w.tab_menu = None;
            if let Some(focus) = w.menu_return.take() {
                window.focus(&focus, cx);
            }
            cx.notify();
        })?;
    }
    Ok(())
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
