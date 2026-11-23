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

/// Import `names`, Markdown files in `dir`, in order; returns their tab
/// ids.
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

/// A document in the Vault has no file of its own to reveal or copy, and
/// neither has an untitled tab; both keep the close rows.
#[gpui::test]
fn a_vault_document_and_an_untitled_tab_offer_no_file_rows(cx: &mut TestAppContext) {
    let dir = tempfile::tempdir().unwrap();
    let (tabs, cx) = boot_home(cx, Session::default());
    cx.simulate_resize(size(px(1100.), px(760.)));
    let (actions, revealed) = crate::workspace::tab_menu::FileActions::recording();
    tabs.update(cx, |tabs, _| tabs.file_actions = actions);
    let imported = open_saved(&tabs, cx, dir.path(), &["contract.md"])[0];
    let untitled = new_dirty(&tabs, cx, "draft");
    for tab in [imported, untitled] {
        cx.update(|_, cx| assert_eq!(tabs.read(cx).tab_file(tab, cx), None));
        open_menu(cx, tab);
        assert!(cx.debug_bounds("context-reveal").is_none());
        assert!(cx.debug_bounds("context-copy-path").is_none());
        assert!(cx.debug_bounds("context-close-tab").is_some());
        assert!(cx.debug_bounds("context-close-others").is_some());
        cx.update(|_, cx| tabs.update(cx, |tabs, _| tabs.tab_menu = None));
    }
    assert!(revealed.borrow().is_empty());
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

/// Other tabs close at once: every document keeps its names.
#[gpui::test]
fn close_others_closes_every_other_tab_without_a_question(cx: &mut TestAppContext) {
    let dir = tempfile::tempdir().unwrap();
    let (tabs, cx, _) = applied_document(cx, dir.path(), Session::default());
    let mapped = active_id(&tabs, cx);
    let saved = open_saved(&tabs, cx, dir.path(), &["keep.md", "clean.md"]);
    let (keep, clean) = (saved[0], saved[1]);
    let draft = new_dirty(&tabs, cx, "draft");
    assert_eq!(ids(&tabs, cx), [mapped, keep, clean, draft]);

    open_menu(cx, keep);
    click(cx, "context-close-others");
    cx.run_until_parked();
    assert!(!cx.has_pending_prompt());
    assert_eq!(ids(&tabs, cx), [keep]);
    assert_eq!(active_id(&tabs, cx), keep);
}
