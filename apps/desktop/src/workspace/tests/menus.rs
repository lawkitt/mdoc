//! The menu bar's items reach their handlers and route to the active document.
use super::*;

#[gpui::test]
fn every_menu_item_reaches_a_handler_in_a_document(cx: &mut TestAppContext) {
    let (_tabs, cx) = boot(cx, Session::default());
    cx.update(|_, cx| crate::commands::menus::install(cx));
    cx.simulate_input("alpha beta");
    cx.run_until_parked();
    for (name, action) in crate::commands::menus::items() {
        // In the window's focus path, or app-wide (Hide, Minimize).
        let available = cx.update(|window, cx| {
            window.is_action_available(action.as_ref(), cx)
                || cx.is_action_available(action.as_ref())
        });
        assert!(available, "menu item {name} has no handler");
    }
}

/// Documents live in the Vault, so the File menu lists no recent files:
/// opening a file again is Open…, an import (#1091 Decisions 6 and 7).
#[test]
fn the_file_menu_has_no_open_recent() {
    let names: Vec<_> = crate::commands::menus::items()
        .into_iter()
        .map(|(name, _)| name)
        .collect();
    assert!(names.iter().any(|name| name == "Open…"));
    assert!(!names.iter().any(|name| name == "Clear Menu"), "{names:?}");
}

#[gpui::test]
fn edit_menu_reaches_the_find_field(cx: &mut TestAppContext) {
    let (tabs, cx) = boot(cx, Session::default());
    cx.simulate_input("alpha beta");
    cx.dispatch_action(FindMarkdown);
    cx.run_until_parked();
    cx.simulate_input("alpha");
    cx.write_to_clipboard(gpui::ClipboardItem::new_string("beta".into()));
    cx.dispatch_action(mdoc_editor::SelectAll);
    cx.dispatch_action(mdoc_editor::Paste);
    cx.run_until_parked();
    let view = active(&tabs, cx);
    cx.update(|_, cx| assert_eq!(view.read(cx).markdown_search.read(cx).value(), "beta"));
    cx.dispatch_action(mdoc_editor::SelectAll);
    cx.dispatch_action(mdoc_editor::Cut);
    cx.run_until_parked();
    cx.update(|_, cx| assert_eq!(view.read(cx).markdown_search.read(cx).value(), ""));
    assert_eq!(
        cx.read_from_clipboard()
            .and_then(|item| item.text())
            .as_deref(),
        Some("beta")
    );
    // The document is untouched.
    cx.update(|_, cx| assert_eq!(view.read(cx).editor.read(cx).text(), "alpha beta"));
}

#[gpui::test]
fn menu_routes_save_export_and_open_to_the_active_document(cx: &mut TestAppContext) {
    let dir = tempfile::tempdir().unwrap();
    let copy_path = dir.path().join("copy.md");
    let other_path = dir.path().join("other.md");
    std::fs::write(&other_path, "other document").unwrap();
    let (tabs, cx) = boot(cx, Session::default());
    let vault = cx.update(|_, cx| tabs.read(cx).opened_vault().unwrap());
    let stored = || {
        let summary = vault.documents().into_iter().next()?;
        Some(vault.document(&summary.id).unwrap().markdown)
    };
    let view = active(&tabs, cx);
    view.update(cx, |v, cx| {
        v.editor.update(cx, |e, cx| e.set_text("first version", cx))
    });
    cx.run_until_parked();
    // Save writes at once, without asking where.
    more(cx, "more-save");
    assert_eq!(stored().as_deref(), Some("first version"));
    view.update(cx, |v, cx| {
        v.editor
            .update(cx, |e, cx| e.set_text("updated version", cx))
    });
    cx.run_until_parked();
    more(cx, "more-save");
    assert_eq!(stored().as_deref(), Some("updated version"));
    // Export Markdown writes a copy; the document stays in the Vault.
    more(cx, "more-export");
    cx.simulate_new_path_selection(|_| Some(copy_path.clone()));
    cx.run_until_parked();
    assert_eq!(
        std::fs::read_to_string(&copy_path).unwrap(),
        "updated version"
    );
    cx.update(|_, cx| {
        let view = view.read(cx);
        assert!(view.session.document.vault.is_some());
    });
    // Open has no header button: ⌘O, Home and the palette.
    cx.dispatch_action(Open);
    cx.run_until_parked();
    cx.simulate_path_prompt_response(|_| Some(vec![other_path]));
    cx.run_until_parked();
    cx.update(|_, cx| {
        let opened = tabs.read(cx).active_view().unwrap();
        assert_ne!(opened.entity_id(), view.entity_id());
        assert_eq!(opened.read(cx).editor.read(cx).text(), "other document");
    });
}
