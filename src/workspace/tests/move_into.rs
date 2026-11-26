//! Moving a document into an existing Project from the Share tab, the
//! document's ⋯ menu, the Project menu and Ask's Unfiled card (#1312):
//! each opens the sidebar's move menu, moves only, every Project listed,
//! and moves as the sidebar does, with its toast and Undo. Each place with
//! no Project, one Project and a shared one.
use super::ai_project::{set_preview, shared};
use super::*;
use mdoc_store::settings::AskWith;

/// Which Projects the window holds besides the document's Unfiled one.
#[derive(Clone, Copy, PartialEq)]
enum Projects {
    None,
    One,
    Shared,
}

const ALL: [Projects; 3] = [Projects::None, Projects::One, Projects::Shared];

/// A Project made in the Vault and taken by the window, as New Project
/// leaves it; its id.
fn add_project(tabs: &Entity<Workspace>, cx: &mut VisualTestContext, name: &str) -> String {
    let project = vault(tabs, cx)
        .create_project(mdoc_vault::NewProject {
            name: name.into(),
            colour: None,
            pinned: false,
        })
        .unwrap();
    let id = project.id.clone();
    tabs.update(cx, |tabs, cx| {
        tabs.projects.put(project);
        cx.notify();
    });
    draw(cx);
    id
}

/// The applied Unfiled "contract" with `projects`; the Project's id, if
/// any. With `Shared`, the preview is on and the Project is shared.
fn document_with<'a>(
    cx: &'a mut TestAppContext,
    dir: &std::path::Path,
    projects: Projects,
) -> (Entity<Workspace>, &'a mut VisualTestContext, Option<String>) {
    let (tabs, cx, _) = applied_document(cx, dir, Session::default());
    cx.simulate_resize(size(px(1400.), px(850.)));
    let project = (projects != Projects::None).then(|| add_project(&tabs, cx, "Lease"));
    if projects == Projects::Shared {
        set_preview(&tabs, cx, true);
        let id = project.clone().unwrap();
        assert!(tabs.update(cx, |tabs, cx| tabs.share_with_ai(&id, cx)));
        assert_eq!(shared(&tabs, cx), Some(id));
    }
    draw(cx);
    (tabs, cx, project)
}

fn shown(cx: &mut VisualTestContext, id: &str) -> bool {
    draw(cx);
    let id: &'static str = Box::leak(id.to_string().into_boxed_str());
    cx.debug_bounds(id).is_some()
}

fn open_share(tabs: &Entity<Workspace>, cx: &mut VisualTestContext) {
    let view = active(tabs, cx);
    view.update_in(cx, |view, window, cx| {
        view.inspector = crate::document_view::Inspector::Share;
        if !view.replacements_open() {
            view.toggle_replacements_panel(window, cx);
        }
    });
    draw(cx);
}

/// The move menu shows, moves only: Move to New Project, no Delete, and a
/// row for `project` when there is one.
fn move_menu_shows(cx: &mut VisualTestContext, project: Option<&str>) {
    assert!(shown(cx, "document-context-menu"));
    assert!(shown(cx, "context-new-project"));
    assert!(!shown(cx, "context-delete-document"));
    assert!(!shown(cx, "context-leave-project"));
    if let Some(project) = project {
        assert!(shown(cx, &format!("context-move-{project}")));
    } else {
        assert!(!shown(cx, "context-move-projects"));
    }
}

/// Choose `project` from the open move menu: the document moves there,
/// with the sidebar's toast and its Undo.
fn move_into(tabs: &Entity<Workspace>, cx: &mut VisualTestContext, project: &str) {
    let tab = cx.update(|_, cx| tabs.read(cx).tabs.active());
    let row: &'static str = Box::leak(format!("context-move-{project}").into_boxed_str());
    click_toolbar(cx, row);
    cx.run_until_parked();
    draw(cx);
    assert!(!shown(cx, "document-context-menu"));
    assert_eq!(project_of_tab(tabs, cx, tab).as_deref(), Some(project));
    let toast = cx.update(|_, cx| {
        tabs.read(cx)
            .project_ui
            .toast
            .as_ref()
            .map(|t| t.text.clone())
    });
    assert_eq!(toast.as_deref(), Some("“contract” moved to “Lease”."));
    assert!(shown(cx, "move-toast-undo"));
    // Undo takes it back to Unfiled, as from the sidebar.
    click_toolbar(cx, "move-toast-undo");
    cx.run_until_parked();
    assert_eq!(project_of_tab(tabs, cx, tab), None);
}

/// Done when (#1312): the Share tab offers Move to Project… once a Project
/// exists; its menu moves into it, shared or not.
#[gpui::test]
fn share_tab_moves_into_an_existing_project(cx: &mut TestAppContext) {
    for projects in ALL {
        let dir = tempfile::tempdir().unwrap();
        let (tabs, cx, project) = document_with(cx, dir.path(), projects);
        open_share(&tabs, cx);
        assert!(shown(cx, "share-new-project"));
        assert_eq!(shown(cx, "share-move-to-project"), project.is_some());
        let Some(project) = project else {
            continue;
        };
        click_toolbar(cx, "share-move-to-project");
        move_menu_shows(cx, Some(&project));
        move_into(&tabs, cx, &project);
    }
}

/// Done when (#1312): the ⋯ menu's Move to Project… opens the same menu
/// under ⋯; with no Project it still holds Move to New Project.
#[gpui::test]
fn more_menu_moves_into_an_existing_project(cx: &mut TestAppContext) {
    for projects in ALL {
        let dir = tempfile::tempdir().unwrap();
        let (tabs, cx, project) = document_with(cx, dir.path(), projects);
        more(cx, "more-move");
        move_menu_shows(cx, project.as_deref());
        // Under the ⋯ button, on the window's right.
        let menu = cx.debug_bounds("document-context-menu").unwrap();
        let button = cx.debug_bounds("header-more").unwrap();
        assert!(
            menu.top() >= button.bottom() - px(1.),
            "{menu:?} {button:?}"
        );
        match project {
            Some(project) => move_into(&tabs, cx, &project),
            None => {
                let tab = cx.update(|_, cx| tabs.read(cx).tabs.active());
                click_toolbar(cx, "context-new-project");
                cx.run_until_parked();
                assert!(project_of_tab(&tabs, cx, tab).is_some());
            }
        }
    }
}

/// Done when (#1312): the Project menu's Move to Project… and Move to New
/// Project act on the document shown.
#[gpui::test]
fn project_menu_moves_the_document_shown_with_no_project(cx: &mut TestAppContext) {
    project_menu_moves(cx, Projects::None);
}

#[gpui::test]
fn project_menu_moves_the_document_shown_with_one_project(cx: &mut TestAppContext) {
    project_menu_moves(cx, Projects::One);
}

#[gpui::test]
fn project_menu_moves_the_document_shown_with_a_shared_project(cx: &mut TestAppContext) {
    project_menu_moves(cx, Projects::Shared);
}

/// The menu's commands reach the window's one workspace, so each state
/// gets its own app.
fn project_menu_moves(cx: &mut TestAppContext, projects: Projects) {
    {
        let dir = tempfile::tempdir().unwrap();
        let (tabs, cx, project) = document_with(cx, dir.path(), projects);
        cx.update(|_, cx| crate::menus::install(cx));
        let names: Vec<_> = (crate::menus::items().into_iter())
            .map(|(name, _)| name.to_string())
            .collect();
        assert!(names.iter().any(|n| n == "Move to Project…"), "{names:?}");
        assert!(
            names.iter().any(|n| n == "Move to New Project"),
            "{names:?}"
        );
        cx.dispatch_action(MoveToProject);
        cx.run_until_parked();
        move_menu_shows(cx, project.as_deref());
        match project {
            Some(project) => move_into(&tabs, cx, &project),
            None => {
                cx.dispatch_action(ui::CloseMenu);
                let tab = cx.update(|_, cx| tabs.read(cx).tabs.active());
                cx.dispatch_action(MoveToNewProject);
                cx.run_until_parked();
                assert!(project_of_tab(&tabs, cx, tab).is_some());
            }
        }
        // Home shows no document: nothing opens.
        tabs.update_in(cx, |tabs, window, cx| tabs.show_home(window, cx));
        cx.dispatch_action(MoveToProject);
        cx.run_until_parked();
        assert!(!shown(cx, "document-context-menu"));
    }
}

/// Done when (#1312): the sidebar's menu, which every place opens, lists
/// every Project, not five, and still offers Delete on an Unfiled row.
#[gpui::test]
fn the_move_menu_lists_every_project(cx: &mut TestAppContext) {
    let dir = tempfile::tempdir().unwrap();
    let (tabs, cx, _) = document_with(cx, dir.path(), Projects::None);
    let ids: Vec<String> = (0..10)
        .map(|i| add_project(&tabs, cx, &format!("Matter {i}")))
        .collect();
    open_share(&tabs, cx);
    click_toolbar(cx, "share-move-to-project");
    assert!(shown(cx, "context-move-projects"));
    for id in &ids {
        let row: &'static str = Box::leak(format!("context-move-{id}").into_boxed_str());
        assert!(cx.debug_bounds(row).is_some(), "{id} is listed");
    }
    // Eight rows show; the rest scroll.
    let list = cx.debug_bounds("context-move-projects").unwrap();
    assert!(list.size.height <= px(8. * 26. + 1.), "{list:?}");
    cx.dispatch_action(ui::CloseMenu);
    draw(cx);
    // The sidebar's own menu still deletes an Unfiled document.
    let document = cx.update(|_, cx| {
        let tabs = tabs.read(cx);
        tabs.tab_document(tabs.tabs.active(), cx).unwrap()
    });
    tabs.update(cx, |tabs, cx| {
        tabs.document_menu = Some((document, gpui::point(px(40.), px(200.))));
        tabs.move_menu = false;
        cx.notify();
    });
    assert!(shown(cx, "context-delete-document"));
}

/// Done when (#1312): Ask on an Unfiled document offers the shared Project
/// first, then a new Project; with none shared the card is as before.
#[gpui::test]
fn ask_on_unfiled_offers_the_shared_project_first(cx: &mut TestAppContext) {
    for projects in ALL {
        let dir = tempfile::tempdir().unwrap();
        let (tabs, cx, project) = document_with(cx, dir.path(), projects);
        set_preview(&tabs, cx, true);
        let tab = cx.update(|_, cx| tabs.read(cx).tabs.active());
        click_toolbar(cx, "header-ask");
        assert!(shown(cx, "ask-unfiled"));
        assert!(shown(cx, "ask-move-and-share"));
        assert_eq!(
            shown(cx, "ask-move-into-shared"),
            projects == Projects::Shared
        );
        if projects != Projects::Shared {
            click_toolbar(cx, "ask-unfiled-cancel");
            continue;
        }
        let project = project.unwrap();
        // The shared Project's button sits after (right of) the new one's.
        let new = cx.debug_bounds("ask-move-and-share").unwrap();
        let into = cx.debug_bounds("ask-move-into-shared").unwrap();
        assert!(
            into.top() >= new.bottom() || into.left() >= new.right(),
            "{new:?} {into:?}"
        );
        click_toolbar(cx, "ask-move-into-shared");
        wait(cx, 300);
        assert_eq!(
            project_of_tab(&tabs, cx, tab).as_deref(),
            Some(project.as_str())
        );
        assert_eq!(shared(&tabs, cx), Some(project));
        assert!(cx.opened_url().is_some(), "the app opens with the prompt");
        let toast = cx.update(|_, cx| {
            tabs.read(cx)
                .project_ui
                .toast
                .as_ref()
                .map(|t| t.text.clone())
        });
        assert_eq!(toast.as_deref(), Some("“contract” moved to “Lease”."));
        assert_eq!(cx.update(|_, cx| tabs.read(cx).ask_last()), AskWith::Claude);
    }
}
