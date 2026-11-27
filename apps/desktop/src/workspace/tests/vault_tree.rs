//! The Vault tree is the navigation: rows that stay put whatever is viewed,
//! Projects that expand and remember it, Unfiled's newest seven and Show
//! all, moves by drag, multi-select delete, the recency switcher, Close back
//! to Home and the ninth view's eviction (`VaultTree`, `LiveViews`).
use super::*;
use crate::workspace::{SwitchDocument, saving::live_views::LIMIT, sidebar::tree::RowKey};

/// Drag the row at `from` to `to` with the left button.
fn drag(
    cx: &mut VisualTestContext,
    from: gpui::Point<gpui::Pixels>,
    to: gpui::Point<gpui::Pixels>,
) {
    draw(cx);
    cx.simulate_mouse_down(from, gpui::MouseButton::Left, Default::default());
    for position in [from + gpui::point(px(6.), px(6.)), to, to] {
        cx.simulate_mouse_move(position, Some(gpui::MouseButton::Left), Default::default());
        draw(cx);
    }
    cx.simulate_mouse_up(to, gpui::MouseButton::Left, Default::default());
    cx.run_until_parked();
    draw(cx);
}

fn bounds_of(cx: &mut VisualTestContext, selector: String) -> gpui::Bounds<gpui::Pixels> {
    let selector: &'static str = Box::leak(selector.into_boxed_str());
    cx.debug_bounds(selector)
        .unwrap_or_else(|| panic!("{selector} must be drawn"))
}

fn keys(tabs: &Entity<Workspace>, cx: &mut VisualTestContext) -> Vec<RowKey> {
    cx.update(|_, cx| tabs.read(cx).tree_keys())
}

fn live(tabs: &Entity<Workspace>, cx: &mut VisualTestContext) -> usize {
    cx.update(|_, cx| {
        let w = tabs.read(cx);
        w.tabs
            .iter()
            .filter(|tab| !w.is_placeholder(tab, cx))
            .count()
    })
}

/// Hold ⌃ and press Tab, as the switcher's user does; nothing releases ⌃.
fn ctrl_tab(cx: &mut VisualTestContext) {
    cx.dispatch_action(SwitchDocument);
    cx.run_until_parked();
    draw(cx);
}

fn release_ctrl(cx: &mut VisualTestContext) {
    cx.simulate_modifiers_change(gpui::Modifiers::none());
    cx.run_until_parked();
    draw(cx);
}

#[gpui::test]
fn the_tree_stays_put_whatever_is_viewed(cx: &mut TestAppContext) {
    let dir = tempfile::tempdir().unwrap();
    let (tabs, cx) = boot_home(cx, Session::default());
    cx.simulate_resize(size(px(1200.), px(900.)));
    let ids = open_notes(&tabs, cx, dir.path(), &["a", "b", "c", "d"]);
    let lease = new_project(&tabs, cx, ids[0]);
    move_tab(&tabs, cx, ids[1], Some(&lease));
    tabs.update(cx, |tabs, cx| {
        tabs.dismiss_toast(cx);
        tabs.tree.set_expanded(&lease, true);
    });
    draw(cx);
    let before = keys(&tabs, cx);
    assert_eq!(before.len(), 5, "{before:?}");
    let mut seen = Vec::new();
    // A click on each row, the switcher, Back, Close and the Project page.
    for id in &ids {
        let row = tree_row(&tabs, *id, cx);
        click_toolbar(cx, row);
        seen.push(keys(&tabs, cx));
    }
    ctrl_tab(cx);
    release_ctrl(cx);
    seen.push(keys(&tabs, cx));
    tabs.update_in(cx, |w, window, cx| w.navigate(true, window, cx));
    seen.push(keys(&tabs, cx));
    cx.dispatch_action(Close);
    cx.run_until_parked();
    seen.push(keys(&tabs, cx));
    click_toolbar(
        cx,
        Box::leak(format!("tree-project-{lease}").into_boxed_str()),
    );
    cx.update(|_, cx| assert!(tabs.read(cx).project_ui.page.is_some()));
    seen.push(keys(&tabs, cx));
    for after in seen {
        assert_eq!(after, before);
    }
    // What is viewed is marked, nothing more: a collapsed Project's member
    // marks its Project.
    tabs.update_in(cx, |w, window, cx| {
        w.tree.set_expanded(&lease, false);
        w.activate(ids[0], window, cx);
    });
    cx.update(|_, cx| {
        let w = tabs.read(cx);
        let now = w.tree_now();
        let shown = w.tree_shown(cx);
        assert_eq!(
            w.tree.selected(&now.rows, &shown, &now.documents),
            Some(RowKey::Project(lease.clone()))
        );
    });
}

#[gpui::test]
fn projects_start_collapsed_and_their_expansion_survives_a_restart(cx: &mut TestAppContext) {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path().join("vault");
    let (project, document) = saved_project(&mdoc_vault::Vault::open(&root, &[3; 32]).unwrap());
    let (tabs, cx) = boot_on_vault(cx, Session::default(), &root);
    draw(cx);
    assert_eq!(keys(&tabs, cx), [RowKey::Project(project.clone())]);
    let chevron: &'static str = Box::leak(format!("tree-chevron-{project}").into_boxed_str());
    click_toolbar(cx, chevron);
    assert_eq!(
        keys(&tabs, cx),
        [
            RowKey::Project(project.clone()),
            RowKey::Document(document.clone())
        ]
    );
    // The chevron folds; the row itself opens the page and leaves the
    // expansion alone.
    let session = cx.update(|_, cx| tabs.read(cx).snapshot(cx));
    assert_eq!(session.expanded_projects, std::slice::from_ref(&project));
    let row: &'static str = Box::leak(format!("tree-project-{project}").into_boxed_str());
    click_toolbar(cx, row);
    cx.update(|_, cx| {
        let w = tabs.read(cx);
        assert_eq!(
            w.project_ui.page.as_ref().map(|p| p.id()),
            Some(project.as_str())
        );
        assert!(w.tree.expanded(&project));
    });
    tabs.update_in(cx, |tabs, window, cx| tabs.quit(window, cx));
    cx.run_until_parked();
    let (tabs, cx) = boot_on_vault(&mut cx.cx, session, &root);
    draw(cx);
    assert_eq!(
        keys(&tabs, cx),
        [RowKey::Project(project), RowKey::Document(document)]
    );
}

#[gpui::test]
fn unfiled_shows_its_seven_newest_and_show_all(cx: &mut TestAppContext) {
    let dir = tempfile::tempdir().unwrap();
    let (tabs, cx) = boot_home(cx, Session::default());
    cx.simulate_resize(size(px(1200.), px(1400.)));
    let names: Vec<String> = (1..=9).map(|i| format!("note {i}")).collect();
    let names: Vec<&str> = names.iter().map(String::as_str).collect();
    open_notes(&tabs, cx, dir.path(), &names);
    draw(cx);
    assert_eq!(keys(&tabs, cx).len(), 7 + 1);
    assert_eq!(keys(&tabs, cx).last(), Some(&RowKey::ShowAll));
    click_toolbar(cx, "tree-show-all");
    assert_eq!(keys(&tabs, cx).len(), 9 + 1);
    click_toolbar(cx, "tree-show-all");
    assert_eq!(keys(&tabs, cx).len(), 7 + 1);
}

#[gpui::test]
fn dragging_a_row_onto_a_project_or_unfiled_moves_its_document(cx: &mut TestAppContext) {
    let dir = tempfile::tempdir().unwrap();
    let (tabs, cx, _) = applied_document(cx, dir.path(), Session::default());
    let first = cx.update(|_, cx| tabs.read(cx).tabs.active());
    let project = new_project(&tabs, cx, first);
    tabs.update(cx, |tabs, cx| tabs.dismiss_toast(cx));
    let [second] = open_notes(&tabs, cx, dir.path(), &["loose"])[..] else {
        unreachable!()
    };
    let document = document_of(&tabs, second, cx);
    let from = bounds_of(cx, format!("tree-document-{document}")).center();
    let to = bounds_of(cx, format!("tree-project-{project}")).center();
    drag(cx, from, to);
    assert_eq!(project_of_tab(&tabs, cx, second), Some(project.clone()));
    cx.update(|_, cx| {
        let toast = tabs
            .read(cx)
            .project_ui
            .toast
            .as_ref()
            .map(|t| t.text.clone());
        assert!(
            toast.is_some_and(|text| text.contains("moved to")),
            "the move's toast"
        );
    });
    // Its row follows into the Project; dragged onto Unfiled it comes back.
    tabs.update(cx, |tabs, cx| {
        tabs.dismiss_toast(cx);
        tabs.tree.set_expanded(&project, true);
        cx.notify();
    });
    draw(cx);
    let from = bounds_of(cx, format!("tree-document-{document}")).center();
    let to = bounds_of(cx, "tree-unfiled".into()).center();
    drag(cx, from, to);
    assert_eq!(project_of_tab(&tabs, cx, second), None);
}

#[gpui::test]
fn unfiled_rows_are_deleted_together_after_a_confirm(cx: &mut TestAppContext) {
    let dir = tempfile::tempdir().unwrap();
    let (tabs, cx) = boot_home(cx, Session::default());
    let ids = open_notes(&tabs, cx, dir.path(), &["a", "b", "c"]);
    let documents: Vec<String> = ids.iter().map(|id| document_of(&tabs, *id, cx)).collect();
    // A plain click picks one row; ⌘/Ctrl-click adds another.
    let secondary = if cfg!(target_os = "macos") {
        gpui::Modifiers::command()
    } else {
        gpui::Modifiers::control()
    };
    let a = bounds_of(cx, format!("tree-document-{}", documents[0])).center();
    let c = bounds_of(cx, format!("tree-document-{}", documents[2])).center();
    cx.simulate_click(a, gpui::Modifiers::none());
    cx.run_until_parked();
    cx.simulate_click(c, secondary);
    cx.run_until_parked();
    draw(cx);
    cx.update(|_, cx| {
        assert_eq!(
            tabs.read(cx).tree.selection(),
            [documents[0].clone(), documents[2].clone()]
        )
    });
    // Cancel keeps them.
    click_toolbar(cx, "tree-delete-chosen");
    assert!(cx.has_pending_prompt());
    cx.simulate_prompt_answer("Cancel");
    cx.run_until_parked();
    assert_eq!(
        cx.update(|_, cx| tabs.read(cx).tree_now().documents.len()),
        3
    );
    // Delete removes both, with their live views, and the shown one gives
    // way to Home.
    click_toolbar(cx, "tree-delete-chosen");
    cx.simulate_prompt_answer("Delete");
    cx.run_until_parked();
    draw(cx);
    assert_eq!(keys(&tabs, cx), [RowKey::Document(documents[1].clone())]);
    let vault = vault(&tabs, cx);
    assert!(vault.document(&documents[0]).is_err());
    assert!(vault.document(&documents[2]).is_err());
    cx.update(|_, cx| {
        let w = tabs.read(cx);
        assert!(!w.tabs.contains(ids[0]) && !w.tabs.contains(ids[2]));
        assert!(w.showing_home(cx));
        assert!(w.tree.selection().is_empty());
    });
    // ⌫ on a Project's member does nothing; a Project's document leaves
    // with Move to Unfiled.
    let project = new_project(&tabs, cx, ids[1]);
    tabs.update(cx, |tabs, cx| {
        tabs.dismiss_toast(cx);
        tabs.tree.set_expanded(&project, true);
    });
    let focus = tree_row_focus(&tabs, ids[1], cx);
    cx.update(|window, cx| window.focus(&focus, cx));
    key(cx, "backspace");
    assert!(!cx.has_pending_prompt());
}

#[gpui::test]
fn the_switcher_moves_by_recency_and_releasing_ctrl_opens_the_pick(cx: &mut TestAppContext) {
    let dir = tempfile::tempdir().unwrap();
    let (tabs, cx) = boot_home(cx, Session::default());
    let [a, b, c] = open_notes(&tabs, cx, dir.path(), &["a", "b", "c"])[..] else {
        unreachable!()
    };
    let shown = |cx: &mut VisualTestContext| cx.update(|_, cx| tabs.read(cx).tabs.active());
    // A quick ⌃Tab goes back and forth between the last two.
    ctrl_tab(cx);
    assert!(cx.debug_bounds("recency-switcher").is_some());
    release_ctrl(cx);
    assert!(cx.debug_bounds("recency-switcher").is_none());
    assert_eq!(shown(cx), b);
    ctrl_tab(cx);
    release_ctrl(cx);
    assert_eq!(shown(cx), c);
    // Holding ⌃, each Tab moves one further back: c, b, a.
    ctrl_tab(cx);
    ctrl_tab(cx);
    cx.update(|_, cx| {
        let w = tabs.read(cx);
        let switcher = w.switcher.as_ref().unwrap();
        assert_eq!(switcher.views(), [c, b, a]);
        assert_eq!(switcher.picked(), a);
    });
    release_ctrl(cx);
    assert_eq!(shown(cx), a);
    // Esc leaves things as they were.
    ctrl_tab(cx);
    key(cx, "escape");
    assert!(cx.update(|_, cx| tabs.read(cx).switcher.is_none()));
    assert_eq!(shown(cx), a);
    // From Home the pick starts on the most recent.
    cx.dispatch_action(Close);
    cx.run_until_parked();
    ctrl_tab(cx);
    release_ctrl(cx);
    assert_eq!(shown(cx), a);
    cx.update(|_, cx| assert!(!tabs.read(cx).showing_home(cx)));
}

#[gpui::test]
fn close_returns_home_and_keeps_the_view_live(cx: &mut TestAppContext) {
    let dir = tempfile::tempdir().unwrap();
    let (tabs, cx) = boot_home(cx, Session::default());
    let [id] = open_notes(&tabs, cx, dir.path(), &["a"])[..] else {
        unreachable!()
    };
    let view = active(&tabs, cx);
    cx.simulate_input("typed ");
    cx.run_until_parked();
    cx.dispatch_action(Close);
    cx.run_until_parked();
    cx.update(|_, cx| {
        let w = tabs.read(cx);
        assert!(w.showing_home(cx));
        assert!(w.tabs.contains(id));
        assert_eq!(w.tabs.get(id).and_then(|tab| tab.view()), Some(&view));
    });
    // Close on Home keeps the window and changes nothing.
    cx.dispatch_action(Close);
    cx.run_until_parked();
    cx.update(|_, cx| assert!(tabs.read(cx).tabs.contains(id)));
    // Its row brings the same view back, with its undo.
    let row = tree_row(&tabs, id, cx);
    click_toolbar(cx, row);
    assert_eq!(active(&tabs, cx), view);
    cx.dispatch_action(mdoc_editor::Undo);
    cx.run_until_parked();
    cx.update(|_, cx| assert_eq!(view.read(cx).editor.read(cx).text(), "a text"));
}

#[gpui::test]
fn the_ninth_view_evicts_the_least_recent_after_its_autosave(cx: &mut TestAppContext) {
    let dir = tempfile::tempdir().unwrap();
    let (tabs, cx) = boot_home(cx, Session::default());
    cx.simulate_resize(size(px(1200.), px(1400.)));
    let [first] = open_notes(&tabs, cx, dir.path(), &["first"])[..] else {
        unreachable!()
    };
    let document = document_of(&tabs, first, cx);
    cx.simulate_input("edited ");
    cx.run_until_parked();
    let names: Vec<String> = (2..=LIMIT).map(|i| format!("n{i}")).collect();
    let names: Vec<&str> = names.iter().map(String::as_str).collect();
    open_notes(&tabs, cx, dir.path(), &names);
    assert_eq!(live(&tabs, cx), LIMIT);
    cx.update(|_, cx| assert!(tabs.read(cx).tabs.contains(first), "eight stay"));
    draw(cx);
    let listed = |cx: &mut VisualTestContext| -> Vec<u64> {
        window_menu_views(cx)
            .into_iter()
            .map(|(_, id)| id)
            .collect()
    };
    assert_eq!(listed(cx).len(), LIMIT);
    assert!(
        listed(cx).contains(&first),
        "Window lists each of the eight"
    );
    // The ninth: the first, viewed least recently, goes once written.
    open_notes(&tabs, cx, dir.path(), &["ninth"]);
    assert_eq!(live(&tabs, cx), LIMIT);
    cx.update(|_, cx| assert!(!tabs.read(cx).tabs.contains(first)));
    // Window drops it, and its row, chosen from a menu opened before,
    // does nothing.
    draw(cx);
    assert_eq!(listed(cx).len(), LIMIT);
    assert!(!listed(cx).contains(&first));
    let shown = cx.update(|_, cx| tabs.read(cx).tabs.active());
    cx.dispatch_action(crate::commands::menus::ShowLiveView(first));
    cx.run_until_parked();
    assert_eq!(cx.update(|_, cx| tabs.read(cx).tabs.active()), shown);
    let saved = vault(&tabs, cx).document(&document).unwrap().markdown;
    assert_eq!(saved, "edited first text");
    // Its row, the oldest import, shows with Show all and opens it again
    // from the Vault.
    click_toolbar(cx, "tree-show-all");
    let row: &'static str = Box::leak(format!("tree-document-{document}").into_boxed_str());
    click_toolbar(cx, row);
    cx.update(|_, cx| {
        let view = tabs.read(cx).active_view().unwrap();
        assert_eq!(view.read(cx).editor.read(cx).text(), "edited first text");
    });
    assert_eq!(live(&tabs, cx), LIMIT);
}

#[gpui::test]
fn back_opens_an_evicted_document_again(cx: &mut TestAppContext) {
    let dir = tempfile::tempdir().unwrap();
    let (tabs, cx) = boot_home(cx, Session::default());
    let [first] = open_notes(&tabs, cx, dir.path(), &["first"])[..] else {
        unreachable!()
    };
    let document = document_of(&tabs, first, cx);
    let names: Vec<String> = (2..=LIMIT + 1).map(|i| format!("n{i}")).collect();
    let names: Vec<&str> = names.iter().map(String::as_str).collect();
    open_notes(&tabs, cx, dir.path(), &names);
    cx.update(|_, cx| assert!(!tabs.read(cx).tabs.contains(first)));
    for _ in 0..LIMIT {
        tabs.update_in(cx, |w, window, cx| w.navigate(true, window, cx));
        cx.run_until_parked();
    }
    let shown = cx.update(|_, cx| {
        let w = tabs.read(cx);
        w.tabs
            .active_tab()
            .and_then(|tab| w.live_document(tab, cx))
            .map(|id| id.as_str().to_string())
    });
    assert_eq!(shown, Some(document));
}

#[gpui::test]
fn an_empty_new_document_is_dropped_when_left(cx: &mut TestAppContext) {
    let dir = tempfile::tempdir().unwrap();
    let (tabs, cx) = boot_home(cx, Session::default());
    let [note] = open_notes(&tabs, cx, dir.path(), &["a"])[..] else {
        unreachable!()
    };
    let blank = tabs.update_in(cx, |w, window, cx| w.new_tab(window, cx));
    cx.run_until_parked();
    tabs.update_in(cx, |w, window, cx| w.activate(note, window, cx));
    cx.run_until_parked();
    cx.update(|_, cx| assert!(!tabs.read(cx).tabs.contains(blank)));
    // Typed into, it is a document and stays.
    let typed = tabs.update_in(cx, |w, window, cx| w.new_tab(window, cx));
    cx.run_until_parked();
    cx.simulate_input("words");
    cx.run_until_parked();
    cx.dispatch_action(Close);
    cx.run_until_parked();
    cx.update(|_, cx| assert!(tabs.read(cx).tabs.contains(typed)));
}

#[gpui::test]
fn an_empty_new_document_is_dropped_when_a_project_page_covers_it(cx: &mut TestAppContext) {
    let dir = tempfile::tempdir().unwrap();
    let (tabs, cx) = boot_home(cx, Session::default());
    let [note] = open_notes(&tabs, cx, dir.path(), &["a"])[..] else {
        unreachable!()
    };
    let project = new_project(&tabs, cx, note);
    tabs.update(cx, |tabs, cx| tabs.dismiss_toast(cx));
    let blank = tabs.update_in(cx, |w, window, cx| w.new_tab(window, cx));
    cx.run_until_parked();
    tabs.update_in(cx, |w, window, cx| w.open_project(&project, window, cx));
    cx.run_until_parked();
    cx.update(|_, cx| {
        let w = tabs.read(cx);
        assert!(!w.tabs.contains(blank));
        assert!(w.project_ui.page.is_some(), "the page stays");
    });
}

#[gpui::test]
fn views_kept_while_they_load_are_evicted_once_loaded(cx: &mut TestAppContext) {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path().join("vault");
    let ids: Vec<String> = {
        let vault = mdoc_vault::Vault::open(&root, &[3; 32]).unwrap();
        (0..=LIMIT)
            .map(|i| {
                vault
                    .create_document(mdoc_vault::NewDocument {
                        project: None,
                        title: format!("Note {i}"),
                        source_kind: mdoc_vault::SourceKind::Markdown,
                        origin: mdoc_vault::DocumentOrigin::Ordinary,
                        markdown: format!("note {i}"),
                    })
                    .unwrap()
                    .id
            })
            .collect()
    };
    let (tabs, cx) = boot_on_vault(cx, Session::default(), &root);
    // Nine opened in one go, as a Project's Open all does: each is loading,
    // so none can go yet.
    tabs.update_in(cx, |w, window, cx| {
        for id in &ids {
            let id = mdoc_store::document_id::VaultId::parse(id).unwrap();
            w.open_vault_document(id, window, cx);
        }
    });
    cx.run_until_parked();
    assert_eq!(live(&tabs, cx), LIMIT);
}

#[gpui::test]
fn back_reaches_a_new_document_by_its_id_once_evicted(cx: &mut TestAppContext) {
    let dir = tempfile::tempdir().unwrap();
    let (tabs, cx) = boot_home(cx, Session::default());
    tabs.update_in(cx, |w, window, cx| {
        w.new_tab(window, cx);
    });
    cx.run_until_parked();
    cx.simulate_input("fresh words");
    // The autosave's pause, then its write: the New document has an id.
    cx.executor()
        .advance_clock(std::time::Duration::from_millis(800));
    cx.run_until_parked();
    let fresh = cx.update(|_, cx| tabs.read(cx).tabs.active());
    let document = document_of(&tabs, fresh, cx);
    let names: Vec<String> = (1..=LIMIT).map(|i| format!("n{i}")).collect();
    let names: Vec<&str> = names.iter().map(String::as_str).collect();
    open_notes(&tabs, cx, dir.path(), &names);
    cx.update(|_, cx| assert!(!tabs.read(cx).tabs.contains(fresh), "evicted"));
    for _ in 0..LIMIT {
        tabs.update_in(cx, |w, window, cx| w.navigate(true, window, cx));
        cx.run_until_parked();
    }
    let shown = cx.update(|_, cx| tabs.read(cx).tabs.active());
    assert_eq!(document_of(&tabs, shown, cx), document);
}
