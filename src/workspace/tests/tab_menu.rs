//! The sidebar tab menu's file and close rows (UI polish decision 13).
use super::*;

/// Right-click tab `id`'s sidebar row.
fn open_menu(cx: &mut VisualTestContext, id: u64) {
    draw(cx);
    let row = cx
        .debug_bounds(Box::leak(format!("tab-{id}").into_boxed_str()))
        .unwrap();
    cx.simulate_mouse_down(row.center(), gpui::MouseButton::Right, Default::default());
    cx.simulate_mouse_up(row.center(), gpui::MouseButton::Right, Default::default());
    cx.run_until_parked();
    draw(cx);
}

fn click(cx: &mut VisualTestContext, selector: &'static str) {
    let bounds = cx.debug_bounds(selector).unwrap();
    cx.simulate_click(bounds.center(), Default::default());
    cx.run_until_parked();
    draw(cx);
}

fn active_id(tabs: &Entity<Workspace>, cx: &mut VisualTestContext) -> u64 {
    cx.update(|_, cx| tabs.read(cx).tabs.active())
}

fn active_name(tabs: &Entity<Workspace>, cx: &mut VisualTestContext) -> String {
    let view = active(tabs, cx);
    cx.update(|_, cx| view.read(cx).display_name())
}

/// Open `names` as saved files in `dir`, in order; returns their tab ids.
fn open_saved(
    tabs: &Entity<Workspace>,
    cx: &mut VisualTestContext,
    dir: &std::path::Path,
    names: &[&str],
) -> Vec<u64> {
    names
        .iter()
        .map(|name| {
            let path = dir.join(name);
            std::fs::write(&path, *name).unwrap();
            tabs.update_in(cx, |tabs, window, cx| {
                tabs.open_paths(vec![path], window, cx)
            });
            cx.run_until_parked();
            active_id(tabs, cx)
        })
        .collect()
}

/// An untitled tab with unsaved text; returns its id.
fn new_dirty(tabs: &Entity<Workspace>, cx: &mut VisualTestContext, text: &str) -> u64 {
    tabs.update_in(cx, |tabs, window, cx| tabs.new_tab(window, cx));
    cx.run_until_parked();
    cx.simulate_input(text);
    cx.run_until_parked();
    active_id(tabs, cx)
}

fn ids(tabs: &Entity<Workspace>, cx: &mut VisualTestContext) -> Vec<u64> {
    cx.update(|_, cx| tabs.read(cx).tabs.iter().map(|tab| tab.id()).collect())
}

#[gpui::test]
fn a_saved_tab_reveals_and_copies_its_path_and_an_untitled_one_offers_neither(
    cx: &mut TestAppContext,
) {
    let dir = tempfile::tempdir().unwrap();
    let (tabs, cx) = boot_home(cx, Session::default());
    cx.simulate_resize(size(px(1100.), px(760.)));
    let (actions, revealed) = crate::workspace::tab_menu::FileActions::recording();
    tabs.update(cx, |tabs, _| tabs.file_actions = actions);
    let saved = open_saved(&tabs, cx, dir.path(), &["contract.md"])[0];
    let file = cx.update(|_, cx| tabs.read(cx).tab_file(saved, cx).unwrap());
    assert_eq!(
        std::fs::canonicalize(&file).unwrap(),
        std::fs::canonicalize(dir.path().join("contract.md")).unwrap()
    );
    let untitled = new_dirty(&tabs, cx, "draft");
    cx.update(|_, cx| assert_eq!(tabs.read(cx).tab_file(untitled, cx), None));

    open_menu(cx, saved);
    for row in [
        "context-reveal",
        "context-copy-path",
        "context-close-tab",
        "context-close-others",
    ] {
        assert!(cx.debug_bounds(row).is_some(), "{row}");
    }
    // The file rows sit above the close rows.
    let reveal = cx.debug_bounds("context-reveal").unwrap();
    let copy = cx.debug_bounds("context-copy-path").unwrap();
    let close = cx.debug_bounds("context-close-tab").unwrap();
    assert!(reveal.top() < copy.top() && copy.top() < close.top());
    click(cx, "context-reveal");
    assert_eq!(*revealed.borrow(), std::slice::from_ref(&file));
    cx.update(|_, cx| assert!(tabs.read(cx).tab_menu.is_none()));

    open_menu(cx, saved);
    click(cx, "context-copy-path");
    cx.update(|_, cx| {
        assert_eq!(
            cx.read_from_clipboard().unwrap().text().as_deref(),
            Some(file.to_string_lossy().as_ref())
        );
    });

    open_menu(cx, untitled);
    assert!(cx.debug_bounds("context-reveal").is_none());
    assert!(cx.debug_bounds("context-copy-path").is_none());
    assert!(cx.debug_bounds("context-close-tab").is_some());
    assert!(cx.debug_bounds("context-close-others").is_some());
    assert_eq!(revealed.borrow().len(), 1, "nothing else was revealed");
}

#[gpui::test]
fn the_only_tab_has_no_close_others(cx: &mut TestAppContext) {
    let dir = tempfile::tempdir().unwrap();
    let (tabs, cx) = boot_home(cx, Session::default());
    cx.simulate_resize(size(px(1100.), px(760.)));
    let only = open_saved(&tabs, cx, dir.path(), &["only.md"])[0];
    open_menu(cx, only);
    assert!(cx.debug_bounds("context-close-tab").is_some());
    assert!(cx.debug_bounds("context-close-others").is_none());
}

/// Clean tabs close at once; each at-risk tab asks in sidebar order, the
/// asked tab in front.
#[gpui::test]
fn close_others_asks_each_at_risk_tab_in_sidebar_order(cx: &mut TestAppContext) {
    let dir = tempfile::tempdir().unwrap();
    let (tabs, cx, _) = applied_document(cx, dir.path(), Session::default());
    let mapped = active_id(&tabs, cx);
    let saved = open_saved(&tabs, cx, dir.path(), &["keep.md", "clean.md"]);
    let (keep, clean) = (saved[0], saved[1]);
    let dirty = new_dirty(&tabs, cx, "draft");
    assert_eq!(ids(&tabs, cx), [mapped, keep, clean, dirty]);

    open_menu(cx, keep);
    click(cx, "context-close-others");
    // The first tab in the sidebar first: ADR 0071's one question for its
    // unsaved changes and mapping.
    assert_eq!(active_name(&tabs, cx), "contract.md");
    cx.simulate_prompt_answer("Don't Save");
    cx.run_until_parked();
    // The clean tab went without a question; the dirty one asks next.
    assert_eq!(ids(&tabs, cx), [keep, dirty]);
    assert_eq!(active_id(&tabs, cx), dirty);
    cx.simulate_prompt_answer("Don't Save");
    cx.run_until_parked();
    assert!(!cx.has_pending_prompt());
    assert_eq!(ids(&tabs, cx), [keep]);
    assert_eq!(active_id(&tabs, cx), keep);
}

/// Cancel at a tab's question keeps that tab and every one after it.
#[gpui::test]
fn cancel_stops_close_others(cx: &mut TestAppContext) {
    let dir = tempfile::tempdir().unwrap();
    let (tabs, cx) = boot_home(cx, Session::default());
    cx.simulate_resize(size(px(1100.), px(760.)));
    let first = open_saved(&tabs, cx, dir.path(), &["first.md", "keep.md"]);
    let (clean, keep) = (first[0], first[1]);
    let dirty = new_dirty(&tabs, cx, "draft");
    let after = open_saved(&tabs, cx, dir.path(), &["after.md"])[0];
    assert_eq!(ids(&tabs, cx), [clean, keep, dirty, after]);

    open_menu(cx, keep);
    click(cx, "context-close-others");
    assert_eq!(active_id(&tabs, cx), dirty);
    cx.simulate_prompt_answer("Cancel");
    cx.run_until_parked();
    assert!(!cx.has_pending_prompt());
    assert_eq!(ids(&tabs, cx), [keep, dirty, after]);

    // Closing the asked tab later doesn't resume the stopped run.
    tabs.update_in(cx, |tabs, window, cx| tabs.close_tab(dirty, window, cx));
    cx.run_until_parked();
    cx.simulate_prompt_answer("Don't Save");
    cx.run_until_parked();
    assert_eq!(ids(&tabs, cx), [keep, after]);
}
