//! Ask (Milestone 2 item 10), with "AI connector (preview)" on: the
//! document header's Ask menu, its Unfiled card, and the Project page's
//! Ask menu. Nothing is chosen, so nothing is shared and no link opens. It
//! leaves the setting off and the window as it found it.
use super::{Driver, Result};
use crate::workspace::project_page::Place;
use mdoc_store::settings::AskWith;

fn preview(d: &mut Driver, on: bool) -> Result<()> {
    d.change(|w, window, cx| {
        if let Some(preferences) = w.preferences.borrow_mut().current.as_mut() {
            preferences.ai_connector.preview = on;
        }
        w.refresh_ai_connector(window, cx);
    })?;
    d.settle()
}

/// `project`: the fixture Project, which holds the fixture document's tab.
pub(super) fn scenes(d: &mut Driver, project: &str) -> Result<()> {
    let was = d.change(|w, _, cx| (w.project_ui.page.is_some(), w.showing_home(cx)))?;
    preview(d, true)?;
    // The fixture document, in its Project: the header's menu.
    let view = d.change(|w, window, cx| {
        let tab = w.tabs.active();
        w.activate(tab, window, cx);
        w.tab_view(tab)
    })?;
    let view = view.ok_or("the fixture document's tab")?;
    d.settle()?;
    d.change(|_, window, cx| view.update(cx, |view, cx| view.show_ask(None, window, cx)))?;
    d.capture("ask-menu")?;
    // The card Ask shows on an Unfiled document.
    d.change(|_, window, cx| {
        view.update(cx, |view, cx| {
            view.show_ask(Some(AskWith::Claude), window, cx)
        })
    })?;
    d.capture("ask-unfiled")?;
    d.change(|_, window, cx| view.update(cx, |view, cx| view.close_ask(window, cx)))?;
    // The Project page's menu, under its button.
    d.change(|w, window, cx| {
        let width = window.viewport_size().width;
        let at = gpui::point(width / 2. + gpui::px(285.), gpui::px(118.));
        w.open_project_ask_menu(project, at, window, cx);
    })?;
    d.capture("project-ask-menu")?;
    d.key("escape")?;
    d.change(|w, window, cx| match was {
        (true, _) => w.show_project_page(project, Place::Header, window, cx),
        (false, true) => w.show_home(window, cx),
        (false, false) => w.activate(w.tabs.active(), window, cx),
    })?;
    preview(d, false)
}
