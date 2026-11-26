//! New Project (item 31): the Project menu, the palette and the tree's
//! Projects header ask for a name, make an empty Project and open its page.
use super::*;
use crate::workspace::NewProject;

/// Home, with every key bound and room for the sidebar.
fn home(cx: &mut TestAppContext) -> (Entity<Workspace>, &mut VisualTestContext) {
    cx.update(crate::bind_all_keys);
    let (tabs, cx) = boot_home(cx, Session::default());
    cx.simulate_resize(size(px(1100.), px(760.)));
    draw(cx);
    (tabs, cx)
}

fn sheet_shows(cx: &mut VisualTestContext) -> bool {
    draw(cx);
    cx.debug_bounds("new-project-sheet").is_some()
}

/// Type `name` into the sheet's field, as a keyboard does, and press Enter.
fn name_it(cx: &mut VisualTestContext, name: &str) {
    assert!(sheet_shows(cx), "New Project asks for a name first");
    cx.simulate_input(name);
    key(cx, "enter");
}

/// The Vault's Projects, by name.
fn stored_names(tabs: &Entity<Workspace>, cx: &mut VisualTestContext) -> Vec<String> {
    let mut names: Vec<_> = (vault(tabs, cx).projects().into_iter())
        .map(|p| p.name)
        .collect();
    names.sort();
    names
}

/// The page shows Project `name`, which the Vault holds empty.
fn shows_empty_project(tabs: &Entity<Workspace>, cx: &mut VisualTestContext, name: &str) {
    assert!(!sheet_shows(cx), "the sheet closed");
    let vault = vault(tabs, cx);
    let project = (vault.projects().into_iter())
        .find(|p| p.name == name)
        .unwrap_or_else(|| panic!("no Project “{name}” in {:?}", vault.projects()));
    assert!(vault.documents_in(Some(&project.id)).is_empty());
    let page = cx.update(|_, cx| {
        let w = tabs.read(cx);
        (w.project_ui.page.as_ref()).map(|page| page.id().to_string())
    });
    assert_eq!(
        page.as_deref(),
        Some(project.id.as_str()),
        "its page opened"
    );
    assert!(cx.debug_bounds("project-page").is_some());
}

#[gpui::test]
fn new_project_from_the_project_menu_makes_an_empty_project_and_opens_its_page(
    cx: &mut TestAppContext,
) {
    let (tabs, cx) = home(cx);
    // The menu bar's own item, as a click on it dispatches it.
    let (_, action) = (crate::menus::items().into_iter())
        .find(|(name, _)| name.as_ref() == "New Project…")
        .expect("Project › New Project…");
    cx.update(|window, cx| window.dispatch_action(action, cx));
    cx.run_until_parked();
    // Trimmed, as on the phones.
    name_it(cx, "  Smith v Jones  ");
    shows_empty_project(&tabs, cx, "Smith v Jones");
    assert_eq!(stored_names(&tabs, cx), ["Smith v Jones"]);
    // The window holds it too: the tree lists it.
    let id = (vault(&tabs, cx).projects().into_iter().next()).unwrap().id;
    let row: &'static str = Box::leak(format!("tree-project-{id}").into_boxed_str());
    expand_sidebar(&tabs, cx);
    assert!(cx.debug_bounds(row).is_some());
}

#[gpui::test]
fn new_project_from_the_palette_makes_an_empty_project_and_opens_its_page(cx: &mut TestAppContext) {
    let (tabs, cx) = home(cx);
    cx.simulate_keystrokes("secondary-k");
    draw(cx);
    cx.simulate_input("new project");
    draw(cx);
    key(cx, "enter");
    assert!(cx.debug_bounds("command-palette").is_none());
    name_it(cx, "Lease review");
    shows_empty_project(&tabs, cx, "Lease review");
}

#[gpui::test]
fn new_project_from_the_trees_projects_header_makes_an_empty_project_and_opens_its_page(
    cx: &mut TestAppContext,
) {
    let (tabs, cx) = home(cx);
    expand_sidebar(&tabs, cx);
    let plus = cx
        .debug_bounds("tree-new-project")
        .expect("the Projects header has a +");
    cx.simulate_click(plus.center(), Default::default());
    assert!(sheet_shows(cx));
    cx.simulate_input("Estate of Novak");
    draw(cx);
    let create = cx.debug_bounds("new-project-create").unwrap();
    cx.simulate_click(create.center(), Default::default());
    cx.run_until_parked();
    shows_empty_project(&tabs, cx, "Estate of Novak");
}

#[gpui::test]
fn new_project_numbers_a_taken_name_and_makes_nothing_blank_or_cancelled(cx: &mut TestAppContext) {
    let (tabs, cx) = home(cx);
    cx.dispatch_action(NewProject);
    name_it(cx, "Smith");
    cx.dispatch_action(NewProject);
    name_it(cx, "Smith ");
    shows_empty_project(&tabs, cx, "Smith 2");
    cx.dispatch_action(NewProject);
    name_it(cx, "Smith");
    shows_empty_project(&tabs, cx, "Smith 3");
    assert_eq!(stored_names(&tabs, cx), ["Smith", "Smith 2", "Smith 3"]);

    // A blank name makes nothing, and the sheet waits for one.
    cx.dispatch_action(NewProject);
    name_it(cx, "   ");
    assert!(sheet_shows(cx));
    // Esc cancels: still nothing.
    key(cx, "escape");
    assert!(!sheet_shows(cx));
    assert_eq!(stored_names(&tabs, cx), ["Smith", "Smith 2", "Smith 3"]);
}

/// Two asked before either reaches the head: the second is numbered
/// against the first, as the Vault holds it then.
#[gpui::test]
fn two_new_projects_of_one_name_queued_together_are_numbered_at_the_head(cx: &mut TestAppContext) {
    let (tabs, cx) = home(cx);
    // A background save holds the head while both are asked.
    let dir = tempfile::tempdir().unwrap();
    let ids = open_notes(&tabs, cx, dir.path(), &["Lease"]);
    let view = active(&tabs, cx);
    view.update_in(cx, |view, window, cx| {
        window.focus(&view.editor.read(cx).focus_handle(cx), cx)
    });
    cx.simulate_input(" edited");
    tabs.update_in(cx, |tabs, window, cx| {
        tabs.autosave(ids[0], window, cx);
        for _ in 0..2 {
            tabs.ask_new_project(window, cx);
            tabs.type_new_project_name("Smith", cx);
            tabs.confirm_new_project(window, cx);
        }
        assert!(tabs.document_writer.busy(), "both wait behind the save");
    });
    cx.run_until_parked();
    assert_eq!(stored_names(&tabs, cx), ["Smith", "Smith 2"]);
}

/// ⌘⇧N / Ctrl+Shift+N from the palette: the palette gives way to the sheet,
/// which takes the keyboard.
#[gpui::test]
fn the_shortcut_opens_new_project_in_place_of_the_palette(cx: &mut TestAppContext) {
    let (tabs, cx) = home(cx);
    cx.simulate_keystrokes("secondary-k");
    draw(cx);
    assert!(cx.debug_bounds("command-palette").is_some());
    key(cx, "secondary-shift-n");
    assert!(cx.debug_bounds("command-palette").is_none());
    name_it(cx, "Shortcut matter");
    shows_empty_project(&tabs, cx, "Shortcut matter");
}

/// Undo from the sheet's buttons never reaches the document behind it.
#[gpui::test]
fn undo_on_the_sheets_buttons_leaves_the_document_alone(cx: &mut TestAppContext) {
    let (tabs, cx) = home(cx);
    let dir = tempfile::tempdir().unwrap();
    open_notes(&tabs, cx, dir.path(), &["Lease"]);
    let view = active(&tabs, cx);
    view.update_in(cx, |view, window, cx| {
        window.focus(&view.editor.read(cx).focus_handle(cx), cx)
    });
    cx.simulate_input(" edited");
    let text = |cx: &mut VisualTestContext| {
        cx.update(|_, cx| view.read(cx).editor.read(cx).text().to_string())
    };
    let before = text(cx);
    assert!(before.contains(" edited"), "{before}");
    cx.dispatch_action(NewProject);
    assert!(sheet_shows(cx));
    // Off the field, onto Cancel.
    key(cx, "tab");
    let on_field = cx.update(|window, _| {
        (window.context_stack().iter()).any(|context| context.contains("TextInput"))
    });
    assert!(!on_field, "focus left the field");
    key(cx, "secondary-z");
    assert_eq!(text(cx), before);
    assert!(sheet_shows(cx));
}
