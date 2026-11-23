//! The Project page: it opens no document, and its actions reach the Vault.
use super::super::project_page::Segment;
use super::*;
use mdoc_vault::Colour;

/// Applied document `contract.md` moved into a new Project; the tab and
/// the Project's id.
fn in_project<'a>(
    cx: &'a mut TestAppContext,
    dir: &std::path::Path,
) -> (Entity<Workspace>, &'a mut VisualTestContext, u64, String) {
    let (tabs, cx, _) = applied_document(cx, dir, Session::default());
    let tab = cx.update(|_, cx| tabs.read(cx).tabs.active());
    let id = new_project(&tabs, cx, tab);
    tabs.update(cx, |tabs, cx| tabs.dismiss_toast(cx));
    // The recovery key nudge opens the bell over the page's header.
    tabs.update(cx, |tabs, _| tabs.title.notices_open.set(false));
    (tabs, cx, tab, id)
}

/// A closed document in Project `id`; its Vault id.
fn closed_member(
    tabs: &Entity<Workspace>,
    cx: &mut VisualTestContext,
    id: &str,
    text: &str,
) -> String {
    let document = vault(tabs, cx)
        .create_document(mdoc_vault::NewDocument {
            project: Some(id.to_string()),
            title: "closed".into(),
            source_kind: mdoc_vault::SourceKind::Markdown,
            origin: mdoc_vault::DocumentOrigin::Ordinary,
            markdown: text.into(),
        })
        .unwrap();
    document.id
}

fn page_controls(
    tabs: &Entity<Workspace>,
    cx: &mut VisualTestContext,
) -> Vec<(String, gpui::FocusHandle)> {
    cx.update(|_, cx| {
        tabs.read(cx)
            .project_ui
            .page
            .as_ref()
            .unwrap()
            .test_controls()
    })
}

#[gpui::test]
fn opening_a_project_is_a_transient_page_without_member_reads(cx: &mut TestAppContext) {
    let dir = tempfile::tempdir().unwrap();
    let (tabs, cx, tab, id) = in_project(cx, dir.path());
    closed_member(&tabs, cx, &id, "never load this member on page open");
    let (before, member_reads) = cx.update(|_, cx| {
        let w = tabs.read(cx);
        (w.snapshot(cx), w.project_ui.member_reads)
    });
    tabs.update_in(cx, |tabs, window, cx| tabs.open_project(&id, window, cx));
    cx.run_until_parked();
    cx.update(|_, cx| {
        let w = tabs.read(cx);
        assert_eq!(
            w.snapshot(cx).tabs,
            before.tabs,
            "opening a page opens no member"
        );
        assert_eq!(w.project_ui.member_reads, member_reads);
        assert_eq!(w.project_ui.page.as_ref().unwrap().id(), id);
        assert!(!w.showing_home(cx));
        assert!(w.showing_overview(cx));
    });
    draw(cx);
    assert!(cx.debug_bounds("project-page").is_some());
    assert!(cx.debug_bounds("project-document-0").is_some());
    assert!(cx.debug_bounds("project-document-1").is_some());
    tabs.update_in(cx, |tabs, window, cx| tabs.activate(tab, window, cx));
    assert!(cx.update(|_, cx| tabs.read(cx).project_ui.page.is_none()));
}

#[gpui::test]
fn project_page_actions_persist_and_delete_returns_home_with_members_open(cx: &mut TestAppContext) {
    cx.update(crate::bind_all_keys);
    let dir = tempfile::tempdir().unwrap();
    let (tabs, cx, tab, id) = in_project(cx, dir.path());
    tabs.update_in(cx, |tabs, window, cx| tabs.open_project(&id, window, cx));
    draw(cx);
    let color = cx.debug_bounds("project-page-color").unwrap();
    cx.simulate_click(color.center(), Default::default());
    draw(cx);
    let swatch = cx.debug_bounds("project-swatch-Teal").unwrap();
    cx.simulate_click(swatch.center(), Default::default());
    cx.run_until_parked();
    cx.update(|_, cx| {
        assert_eq!(project_colour(&tabs, &id, cx), Some(Colour::Teal));
        let view = tabs
            .read(cx)
            .tabs
            .get(tab)
            .unwrap()
            .view()
            .unwrap()
            .read(cx);
        assert_eq!(view.project.link().unwrap().colour, Some(Colour::Teal));
    });
    tabs.update_in(cx, |tabs, window, cx| {
        tabs.start_rename_project(id.clone(), window, cx)
    });
    cx.simulate_input("Petrova v Northwind");
    cx.simulate_keystrokes("enter");
    cx.run_until_parked();
    assert_eq!(project(&tabs, cx, &id).name, "Petrova v Northwind");
    tabs.update(cx, |tabs, cx| {
        tabs.add_project_name(&id, "Project Bluebird", cx)
    });
    cx.run_until_parked();
    let alias = (project(&tabs, cx, &id).mapping.identities.iter())
        .find(|i| i.originals == ["Project Bluebird"])
        .unwrap()
        .alias
        .clone();
    tabs.update_in(cx, |tabs, window, cx| {
        tabs.confirm_forget_project_identity(&id, &alias, window, cx)
    });
    cx.simulate_prompt_answer("Cancel");
    cx.run_until_parked();
    assert!((project(&tabs, cx, &id).mapping.exported().iter()).any(|i| i.alias == alias));
    tabs.update_in(cx, |tabs, window, cx| {
        tabs.confirm_forget_project_identity(&id, &alias, window, cx)
    });
    cx.simulate_prompt_answer("Forget Identity");
    cx.run_until_parked();
    let add = cx.update(|_, cx| {
        tabs.read(cx)
            .project_ui
            .page
            .as_ref()
            .unwrap()
            .test_inputs()
            .1
    });
    assert!(
        cx.update(|window, cx| add.read(cx).focus_handle(cx).is_focused(window)),
        "forget returns focus to Add a name"
    );
    let saved = vault(&tabs, cx).project(&id).unwrap();
    assert_eq!(saved.colour, Some(Colour::Teal));
    assert_eq!(saved.name, "Petrova v Northwind");
    assert!(!saved.mapping.exported().iter().any(|i| i.alias == alias));
    // The member's text never changes for any of it.
    let document = cx.update(|_, cx| tabs.read(cx).tab_document(tab, cx).unwrap());
    let text = vault(&tabs, cx)
        .document(document.as_str())
        .unwrap()
        .markdown;
    assert_eq!(text, "PERSON_1 met ORG_1.");
    tabs.update_in(cx, |tabs, window, cx| {
        tabs.delete_project(id.clone(), window, cx)
    });
    cx.simulate_prompt_answer("Cancel");
    cx.run_until_parked();
    assert!(cx.update(|_, cx| tabs.read(cx).project_ui.page.is_some()));
    tabs.update_in(cx, |tabs, window, cx| {
        tabs.delete_project(id.clone(), window, cx)
    });
    cx.simulate_prompt_answer("Move to Unfiled");
    cx.run_until_parked();
    cx.update(|_, cx| {
        let w = tabs.read(cx);
        assert!(w.showing_home(cx));
        assert!(w.tabs.contains(tab));
        let view = w.tabs.get(tab).unwrap().view().unwrap().read(cx);
        assert!(view.project.link().is_none());
        assert!(!view.pii.review().export_identities().is_empty());
    });
    assert!(vault(&tabs, cx).project(&id).is_err());
    let text = vault(&tabs, cx)
        .document(document.as_str())
        .unwrap()
        .markdown;
    assert_eq!(text, "PERSON_1 met ORG_1.");
}

fn project_colour(tabs: &Entity<Workspace>, id: &str, cx: &App) -> Option<Colour> {
    tabs.read(cx).projects.project(id).unwrap().colour
}

#[gpui::test]
fn page_moves_closed_and_open_members_to_unfiled_without_loading_them(cx: &mut TestAppContext) {
    let dir = tempfile::tempdir().unwrap();
    let (tabs, cx, tab, id) = in_project(cx, dir.path());
    let closed = closed_member(&tabs, cx, &id, "PERSON_1");
    tabs.update_in(cx, |tabs, window, cx| tabs.open_project(&id, window, cx));
    draw(cx);
    let reads = cx.update(|_, cx| tabs.read(cx).project_ui.member_reads);
    let open = cx.update(|_, cx| tabs.read(cx).tab_document(tab, cx).unwrap());
    for document in [
        mdoc_store::document_id::VaultId::parse(&closed).unwrap(),
        open,
    ] {
        tabs.update_in(cx, |tabs, window, cx| {
            tabs.remove_project_member(&id, document, window, cx)
        });
        cx.run_until_parked();
    }
    let vault = vault(&tabs, cx);
    assert!(vault.documents_in(Some(&id)).is_empty());
    assert_eq!(vault.document(&closed).unwrap().project, None);
    cx.update(|_, cx| {
        let w = tabs.read(cx);
        assert_eq!(
            w.project_ui.member_reads,
            reads + 1,
            "only the open member reloads"
        );
        assert!(
            w.find_document(
                &mdoc_store::document_id::DocumentId::Vault(
                    mdoc_store::document_id::VaultId::parse(&closed).unwrap()
                ),
                None,
                cx
            )
            .is_none()
        );
        let view = w.tabs.get(tab).unwrap().view().unwrap().read(cx);
        assert!(view.project.link().is_none());
        assert!(w.project_ui.page.is_some());
    });
}

#[gpui::test]
fn page_document_activation_is_explicit(cx: &mut TestAppContext) {
    let dir = tempfile::tempdir().unwrap();
    let (tabs, cx, _, id) = in_project(cx, dir.path());
    let closed = closed_member(&tabs, cx, &id, "PERSON_1");
    tabs.update_in(cx, |tabs, window, cx| tabs.open_project(&id, window, cx));
    draw(cx);
    let tabs_before = cx.update(|_, cx| tabs.read(cx).tabs.len());
    // Members sort by title: "closed" before "contract".
    let row = cx.debug_bounds("project-document-0").unwrap();
    cx.simulate_click(row.center(), Default::default());
    cx.run_until_parked();
    let closed = mdoc_store::document_id::DocumentId::Vault(
        mdoc_store::document_id::VaultId::parse(&closed).unwrap(),
    );
    let view = cx.update(|_, cx| {
        let w = tabs.read(cx);
        assert!(w.project_ui.page.is_none());
        assert_eq!(w.tabs.len(), tabs_before + 1, "only the clicked one opens");
        let tab = w.find_document(&closed, None, cx).unwrap();
        w.tabs.get(tab).unwrap().view().unwrap().clone()
    });
    // The member opened from the page has its Project's names.
    cx.update(|_, cx| {
        assert_eq!(view.read(cx).project.link().unwrap().id, id);
        assert_eq!(view.read(cx).pii.review().applied().len(), 1);
    });
}

#[gpui::test]
fn names_filter_add_binding_and_every_page_control_are_keyboard_reachable(cx: &mut TestAppContext) {
    cx.update(crate::bind_all_keys);
    let dir = tempfile::tempdir().unwrap();
    let (tabs, cx, _, id) = in_project(cx, dir.path());
    tabs.update(cx, |tabs, cx| {
        for index in 0..9 {
            tabs.add_project_name(&id, &format!("Project Example {index}"), cx);
        }
    });
    tabs.update_in(cx, |tabs, window, cx| tabs.open_project(&id, window, cx));
    tabs.update(cx, |tabs, cx| tabs.show_project_segment(Segment::Names, cx));
    cx.simulate_resize(size(px(720.), px(480.)));
    expand_sidebar(&tabs, cx);
    draw(cx);
    assert!(cx.debug_bounds("project-page-filter").is_some());
    let (filter, add) = cx.update(|_, cx| {
        tabs.read(cx)
            .project_ui
            .page
            .as_ref()
            .unwrap()
            .test_inputs()
    });
    cx.update(|window, cx| window.focus(&filter.read(cx).focus_handle(cx), cx));
    cx.simulate_input("Northwind");
    draw(cx);
    assert!(cx.debug_bounds("project-identity-ORG_1").is_some());
    assert!(cx.debug_bounds("project-identity-PERSON_1").is_none());
    // The page's fields edit like every other text field.
    cx.simulate_keystrokes("backspace left backspace");
    assert_eq!(
        cx.update(|_, cx| filter.read(cx).value().to_string()),
        "Northwn"
    );
    filter.update(cx, |input, cx| input.reset(cx));
    draw(cx);
    cx.update(|window, cx| window.focus(&add.read(cx).focus_handle(cx), cx));
    cx.simulate_input("New name by keyboardx");
    cx.simulate_keystrokes("backspace");
    cx.simulate_keystrokes("enter");
    cx.run_until_parked();
    assert!(
        (project(&tabs, cx, &id).mapping.identities.iter())
            .any(|identity| identity.originals == ["New name by keyboard"])
    );
    assert_eq!(cx.update(|_, cx| add.read(cx).value().to_string()), "");
    draw(cx);
    let targets = page_controls(&tabs, cx);
    let mut seen = Vec::new();
    for _ in 0..100 {
        cx.simulate_keystrokes("tab");
        draw(cx);
        draw(cx); // deferred focus reveal includes the outside ring
        cx.update(|window, cx| {
            if let Some(focus) = window.focused(cx) {
                seen.push(focus);
            }
        });
        if targets.iter().all(|(_, focus)| seen.contains(focus)) {
            break;
        }
    }
    for (name, focus) in targets {
        assert!(seen.contains(&focus), "Tab did not reach {name}");
        cx.update(|window, cx| window.focus(&focus, cx));
        draw(cx);
        draw(cx);
        let bounds = cx
            .debug_bounds(Box::leak(name.clone().into_boxed_str()))
            .unwrap();
        let viewport = cx
            .update(|_, cx| (tabs.read(cx).project_ui.page.as_ref().unwrap()).test_scroll_bounds());
        assert!(
            bounds.top() >= viewport.top() && bounds.bottom() <= viewport.bottom(),
            "{name}: {bounds:?} outside {viewport:?}"
        );
    }
}

#[gpui::test]
fn the_project_page_scrolls_with_the_wheel(cx: &mut TestAppContext) {
    project_page_scrolls(cx, true);
}

#[gpui::test]
fn the_project_page_scrolls_with_the_sidebar_collapsed(cx: &mut TestAppContext) {
    project_page_scrolls(cx, false);
}

fn project_page_scrolls(cx: &mut TestAppContext, sidebar_visible: bool) {
    cx.update(crate::bind_all_keys);
    let dir = tempfile::tempdir().unwrap();
    let (tabs, cx, _, id) = in_project(cx, dir.path());
    if !sidebar_visible {
        draw(cx);
        click_toolbar(cx, "sidebar-toggle");
    }
    assert_eq!(
        cx.update(|_, cx| tabs.read(cx).sidebar.visible()),
        sidebar_visible
    );
    tabs.update(cx, |tabs, cx| {
        for index in 0..20 {
            tabs.add_project_name(&id, &format!("Project Example {index}"), cx);
        }
    });
    tabs.update_in(cx, |tabs, window, cx| tabs.open_project(&id, window, cx));
    tabs.update(cx, |tabs, cx| tabs.show_project_segment(Segment::Names, cx));
    cx.simulate_resize(size(px(720.), px(480.)));
    draw(cx);
    let offset = |cx: &mut VisualTestContext| {
        cx.update(|_, cx| (tabs.read(cx).project_ui.page.as_ref().unwrap()).test_scroll_offset())
    };
    assert_eq!(offset(cx), px(0.));
    // A focused field near the top must not pull the page back each frame.
    let (filter, _) = cx.update(|_, cx| {
        tabs.read(cx)
            .project_ui
            .page
            .as_ref()
            .unwrap()
            .test_inputs()
    });
    cx.update(|window, cx| window.focus(&filter.read(cx).focus_handle(cx), cx));
    draw(cx);
    draw(cx);
    let viewport = cx.debug_bounds("project-page").unwrap();
    cx.simulate_event(gpui::ScrollEvent {
        position: viewport.center(),
        delta: gpui::ScrollDelta::Pixels(gpui::point(px(0.), px(-400.))),
        ..Default::default()
    });
    draw(cx);
    draw(cx);
    draw(cx);
    assert!(
        offset(cx) < px(-300.),
        "the page did not stay scrolled: {:?}",
        offset(cx)
    );
    assert!(
        cx.update(|window, cx| filter.read(cx).focus_handle(cx).is_focused(window)),
        "scrolling keeps focus in the field"
    );
    // The wheel reaches the last control on the page, inside the window.
    for _ in 0..20 {
        cx.simulate_event(gpui::ScrollEvent {
            position: viewport.center(),
            delta: gpui::ScrollDelta::Pixels(gpui::point(px(0.), px(-400.))),
            ..Default::default()
        });
        draw(cx);
    }
    let window = cx.update(|window, _| window.viewport_size());
    let scroll =
        cx.update(|_, cx| (tabs.read(cx).project_ui.page.as_ref().unwrap()).test_scroll_bounds());
    let last = mdoc_pii::Category::ALL
        .into_iter()
        .rfind(|category| *category != mdoc_pii::Category::Other)
        .unwrap();
    let last: &'static str = Box::leak(format!("page-category-{}", last.token()).into_boxed_str());
    let delete = cx.debug_bounds(last).unwrap();
    assert!(
        scroll.bottom() <= window.height,
        "the scroll area {scroll:?} runs past the window {window:?}"
    );
    assert!(
        delete.bottom() <= scroll.bottom(),
        "the last control {delete:?} stays below the scroll area {scroll:?}"
    );
}

#[gpui::test]
fn projects_load_with_the_vault_once_it_is_unlocked(cx: &mut TestAppContext) {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path().join("vault");
    let (project, _) = saved_project(&mdoc_vault::Vault::open(&root, &[3; 32]).unwrap());
    let (tabs, cx) = boot_home_with(cx, Session::default(), {
        let root = root.clone();
        move |tabs, _, _| tabs.stage_vault_key(Ok(mdoc_vault::Vault::open(root, &[3; 32]).unwrap()))
    });
    tabs.update_in(cx, |tabs, window, cx| tabs.show_project_list(window, cx));
    draw(cx);
    assert!(cx.debug_bounds("open-project-row").is_none());
    assert!(cx.debug_bounds("project-list-locked").is_some());
    let unlock = cx.debug_bounds("project-list-unlock").unwrap();
    cx.simulate_click(unlock.center(), Default::default());
    cx.run_until_parked();
    tabs.update_in(cx, |tabs, window, cx| tabs.continue_unlock(window, cx));
    cx.run_until_parked();
    tabs.update_in(cx, |tabs, window, cx| tabs.show_project_list(window, cx));
    draw(cx);
    assert!(cx.debug_bounds("open-project-row").is_some());
    cx.update(|_, cx| {
        let w = tabs.read(cx);
        assert!(w.projects.project(&project).is_some());
        assert_eq!(w.project_ui.member_reads, 0);
    });
}

#[gpui::test]
fn a_missing_project_page_returns_home(cx: &mut TestAppContext) {
    let dir = tempfile::tempdir().unwrap();
    let (tabs, cx, _, id) = in_project(cx, dir.path());
    tabs.update_in(cx, |tabs, window, cx| tabs.open_project(&id, window, cx));
    draw(cx);
    tabs.update(cx, |tabs, cx| {
        tabs.projects.remove(&id);
        cx.notify();
    });
    draw(cx);
    cx.update(|_, cx| {
        let w = tabs.read(cx);
        assert!(w.showing_home(cx));
        assert!(w.project_ui.page.is_none());
    });
}

#[gpui::test]
fn forgetting_an_unused_seed_cannot_reappear_when_typed_later(cx: &mut TestAppContext) {
    let dir = tempfile::tempdir().unwrap();
    let (tabs, cx, _, id) = in_project(cx, dir.path());
    tabs.update(cx, |tabs, cx| {
        tabs.add_project_name(&id, "Forgotten Bluebird", cx)
    });
    let alias = (project(&tabs, cx, &id)
        .mapping
        .identities
        .last()
        .unwrap()
        .alias)
        .clone();
    tabs.update(cx, |tabs, cx| tabs.forget_project_identity(&id, &alias, cx));
    let view = active(&tabs, cx);
    view.update(cx, |view, cx| {
        let source = format!("{} Forgotten Bluebird", view.editor.read(cx).text());
        view.editor
            .update(cx, |editor, cx| editor.set_text(source.clone(), cx));
        view.pii.review_mut().refresh(&source);
        assert!(
            !(view.pii.review().export_identities().iter()).any(|identity| identity.alias == alias)
        );
    });
}

#[gpui::test]
fn share_and_sidebar_header_open_the_same_page(cx: &mut TestAppContext) {
    let dir = tempfile::tempdir().unwrap();
    let (tabs, cx, tab, id) = in_project(cx, dir.path());
    let view = active(&tabs, cx);
    view.update_in(cx, |view, window, cx| {
        view.inspector_share = true;
        if !view.replacements_open() {
            view.toggle_replacements_panel(window, cx);
        }
    });
    draw(cx);
    // A member can move to a new Project from Share too.
    assert!(cx.debug_bounds("share-new-project").is_some());
    let share = cx.debug_bounds("share-project-page").unwrap();
    cx.simulate_click(share.center(), Default::default());
    cx.run_until_parked();
    assert_eq!(
        cx.update(|_, cx| tabs
            .read(cx)
            .project_ui
            .page
            .as_ref()
            .unwrap()
            .id()
            .to_string()),
        id
    );
    tabs.update_in(cx, |tabs, window, cx| tabs.activate(tab, window, cx));
    draw(cx);
    let row: &'static str = Box::leak(format!("tree-project-{id}").into_boxed_str());
    let header = cx.debug_bounds(row).unwrap();
    cx.simulate_click(header.center(), Default::default());
    cx.run_until_parked();
    assert_eq!(
        cx.update(|_, cx| tabs
            .read(cx)
            .project_ui
            .page
            .as_ref()
            .unwrap()
            .id()
            .to_string()),
        id
    );
}

#[gpui::test]
fn a_long_project_name_stays_inside_the_narrow_page(cx: &mut TestAppContext) {
    let dir = tempfile::tempdir().unwrap();
    let (tabs, cx, _, id) = in_project(cx, dir.path());
    tabs.update(cx, |tabs, _| {
        tabs.projects.edit(&id, |project| {
            project.name = "A long Project name about acceptance of the first shipment with \
                            several parties and exhibits"
                .into();
            Some(())
        });
    });
    tabs.update_in(cx, |tabs, window, cx| tabs.open_project(&id, window, cx));
    cx.simulate_resize(size(px(720.), px(480.)));
    draw(cx);
    let name = cx.debug_bounds("project-page-rename").unwrap();
    let page = cx.debug_bounds("project-page").unwrap();
    assert!(
        name.left() >= page.left() && name.right() <= page.right(),
        "name {name:?} outside page {page:?}"
    );
}

#[gpui::test]
fn forgetting_the_highest_alias_reserves_it_in_new_members(cx: &mut TestAppContext) {
    let dir = tempfile::tempdir().unwrap();
    let (tabs, cx, _, id) = in_project(cx, dir.path());
    tabs.update(cx, |tabs, cx| {
        tabs.add_project_name(&id, "Bob", cx);
        let alias = (tabs
            .projects
            .project(&id)
            .unwrap()
            .mapping
            .identities
            .last()
            .unwrap()
            .alias)
            .clone();
        assert_eq!(alias, "PERSON_2");
        tabs.forget_project_identity(&id, "PERSON_2", cx);
    });
    cx.run_until_parked();
    let member = open_applied(
        &tabs,
        cx,
        dir.path(),
        "second.md",
        "Carol",
        &[("Carol", mdoc_pii::Category::Person)],
    );
    move_tab(&tabs, cx, member, Some(&id));
    let view = active(&tabs, cx);
    cx.update(|_, cx| {
        assert_eq!(view.read(cx).editor.read(cx).text(), "PERSON_3");
        let identities = view.read(cx).pii.review().export_identities();
        assert!(
            (identities.iter())
                .any(|identity| identity.alias == "PERSON_3" && identity.wording() == "Carol")
        );
    });
    assert!(
        (vault(&tabs, cx)
            .project(&id)
            .unwrap()
            .mapping
            .exported()
            .iter())
        .any(|identity| identity.alias == "PERSON_3" && identity.wording() == "Carol")
    );
}

#[gpui::test]
fn swatches_and_inline_rename_are_reachable_and_activatable_by_keyboard(cx: &mut TestAppContext) {
    cx.update(crate::bind_all_keys);
    let dir = tempfile::tempdir().unwrap();
    let (tabs, cx, _, id) = in_project(cx, dir.path());
    tabs.update_in(cx, |tabs, window, cx| tabs.open_project(&id, window, cx));
    draw(cx);
    draw(cx);
    let control = |tabs: &Entity<Workspace>, cx: &mut VisualTestContext, name: &str| {
        (page_controls(tabs, cx).into_iter())
            .find(|(id, _)| id == name)
            .unwrap()
            .1
    };
    let color = control(&tabs, cx, "project-page-color");
    cx.update(|window, cx| window.focus(&color, cx));
    draw(cx);
    activate_control(cx);
    let swatches: Vec<_> = (page_controls(&tabs, cx).into_iter())
        .filter(|(id, _)| id.starts_with("project-swatch-"))
        .collect();
    assert_eq!(swatches.len(), 8);
    let mut reached = Vec::new();
    for _ in 0..60 {
        cx.simulate_keystrokes("tab");
        draw(cx);
        cx.update(|window, cx| {
            if let Some(focus) = window.focused(cx) {
                reached.push(focus);
            }
        });
        if swatches.iter().all(|(_, focus)| reached.contains(focus)) {
            break;
        }
    }
    assert!(swatches.iter().all(|(_, focus)| reached.contains(focus)));
    let blue = (swatches.iter())
        .find(|(id, _)| id == "project-swatch-Blue")
        .unwrap()
        .1
        .clone();
    cx.update(|window, cx| window.focus(&blue, cx));
    draw(cx);
    activate_control(cx);
    assert_eq!(
        cx.update(|_, cx| project_colour(&tabs, &id, cx)),
        Some(Colour::Blue)
    );
    let rename = control(&tabs, cx, "project-page-rename");
    cx.update(|window, cx| window.focus(&rename, cx));
    draw(cx);
    activate_control(cx);
    cx.simulate_input("Renamed from keyboard");
    cx.simulate_keystrokes("enter");
    cx.run_until_parked();
    assert_eq!(project(&tabs, cx, &id).name, "Renamed from keyboard");
}

#[gpui::test]
fn leaving_the_rename_field_saves_and_escape_still_cancels(cx: &mut TestAppContext) {
    cx.update(crate::bind_all_keys);
    let dir = tempfile::tempdir().unwrap();
    let (tabs, cx, tab, id) = in_project(cx, dir.path());
    // Focus-out events reach listeners only in an active window.
    cx.update(|window, _| window.activate_window());
    tabs.update_in(cx, |tabs, window, cx| tabs.open_project(&id, window, cx));
    draw(cx);
    let name = |cx: &mut VisualTestContext| project(&tabs, cx, &id).name;
    let original = name(cx);
    let rename = |cx: &mut VisualTestContext, value: &str| {
        tabs.update_in(cx, |tabs, window, cx| {
            tabs.start_rename_project(id.clone(), window, cx)
        });
        let input = cx.update(|_, cx| {
            tabs.read(cx)
                .project_ui
                .renaming
                .as_ref()
                .unwrap()
                .1
                .clone()
        });
        input.update(cx, |input, cx| input.set_value(value.to_string(), cx));
        draw(cx);
    };
    let renaming =
        |cx: &mut VisualTestContext| cx.update(|_, cx| tabs.read(cx).project_ui.renaming.is_some());

    // Escape cancels.
    rename(cx, "Not kept");
    cx.simulate_keystrokes("escape");
    draw(cx);
    assert!(!renaming(cx));
    assert_eq!(name(cx), original);

    // A click on a blank part of the page saves.
    rename(cx, "Saved by a click");
    let page = cx.debug_bounds("project-page").unwrap();
    cx.simulate_click(
        gpui::point(page.left() + px(4.), page.bottom() - px(4.)),
        gpui::Modifiers::none(),
    );
    draw(cx);
    assert!(!renaming(cx));
    assert_eq!(name(cx), "Saved by a click");

    // Moving focus away (Tab) saves.
    rename(cx, "Saved by Tab");
    cx.simulate_keystrokes("tab");
    draw(cx);
    assert!(!renaming(cx));
    assert_eq!(name(cx), "Saved by Tab");

    // Keyboard navigation away from the page saves too: switching to a tab
    // and going Home both end the rename before the page goes.
    rename(cx, "Saved by switching tabs");
    tabs.update_in(cx, |tabs, window, cx| tabs.activate(tab, window, cx));
    draw(cx);
    assert!(!renaming(cx));
    assert_eq!(name(cx), "Saved by switching tabs");
    tabs.update_in(cx, |tabs, window, cx| tabs.open_project(&id, window, cx));
    draw(cx);
    rename(cx, "Saved by going Home");
    tabs.update_in(cx, |tabs, window, cx| tabs.show_home(window, cx));
    draw(cx);
    assert!(!renaming(cx));
    assert_eq!(name(cx), "Saved by going Home");
    cx.run_until_parked();
    assert_eq!(
        vault(&tabs, cx).project(&id).unwrap().name,
        "Saved by going Home"
    );
}

#[gpui::test]
fn the_project_list_hangs_from_homes_link_and_says_when_it_is_empty(cx: &mut TestAppContext) {
    let (tabs, cx) = boot_home(cx, Session::default());
    draw(cx);
    tabs.update_in(cx, |tabs, window, cx| tabs.show_project_list(window, cx));
    draw(cx);
    assert!(cx.debug_bounds("project-list-empty").is_some());
    assert!(cx.debug_bounds("project-list-locked").is_none());
    tabs.update_in(cx, |tabs, window, cx| tabs.close_project_list(window, cx));
    draw(cx);
    let link = cx.debug_bounds("home-open-project").unwrap();
    cx.simulate_click(link.center(), Default::default());
    draw(cx);
    let list = cx.debug_bounds("project-list").unwrap();
    assert!(
        list.top() >= link.bottom() && list.top() <= link.bottom() + px(12.),
        "the list {list:?} opens just below its link {link:?}"
    );
    assert!(
        (list.right() - link.right()).abs() <= px(1.),
        "the list {list:?} lines up with its link {link:?}"
    );
    // From anywhere else it stays centered.
    tabs.update_in(cx, |tabs, window, cx| {
        tabs.close_project_list(window, cx);
        tabs.show_project_list(window, cx);
    });
    draw(cx);
    let centered = cx.debug_bounds("project-list").unwrap();
    let width = cx.update(|window, _| window.viewport_size().width);
    assert!(
        (centered.center().x - width / 2.).abs() <= px(1.),
        "{centered:?} is centered in {width:?}"
    );
}

/// The header: colour, title, counts, Add document and ⋯, whose menu holds
/// Rename, Colour, Categories to detect, Export Project… and Delete
/// Project….
#[gpui::test]
fn the_header_and_its_menu_hold_the_project_actions(cx: &mut TestAppContext) {
    let dir = tempfile::tempdir().unwrap();
    let (tabs, cx, _, id) = in_project(cx, dir.path());
    tabs.update_in(cx, |tabs, window, cx| tabs.open_project(&id, window, cx));
    draw(cx);
    for control in [
        "project-page-color",
        "project-page-rename",
        "project-page-add-document",
        "project-page-menu",
        "project-segment-documents",
        "project-segment-replies",
        "project-segment-names",
        "project-page-counts",
    ] {
        assert!(cx.debug_bounds(control).is_some(), "{control}");
    }
    click_toolbar(cx, "project-page-menu");
    for action in [
        "project-action-rename",
        "project-action-colour",
        "project-action-categories",
        "project-action-export",
        "project-action-delete",
    ] {
        assert!(cx.debug_bounds(action).is_some(), "{action}");
    }
    assert!(cx.debug_bounds("project-action-close-all").is_none());
    // Colour opens the swatches; Categories opens Names at its section.
    click_toolbar(cx, "project-action-colour");
    assert!(cx.debug_bounds("project-swatch-Teal").is_some());
    click_toolbar(cx, "project-page-menu");
    click_toolbar(cx, "project-action-categories");
    cx.update(|_, cx| {
        let page = tabs.read(cx).project_ui.page.as_ref().unwrap();
        assert_eq!(page.segment(), Segment::Names);
    });
    draw(cx);
    assert!(cx.debug_bounds("page-category-PERSON").is_some());
    // Delete Project… asks the delete question at once.
    click_toolbar(cx, "project-page-menu");
    click_toolbar(cx, "project-action-delete");
    assert!(cx.has_pending_prompt());
    cx.simulate_prompt_answer("Cancel");
    cx.run_until_parked();
    assert!(vault(&tabs, cx).project(&id).is_ok());
}

/// The counts line follows what the Project holds.
#[gpui::test]
fn the_counts_follow_documents_replies_and_names(cx: &mut TestAppContext) {
    let dir = tempfile::tempdir().unwrap();
    let (tabs, cx, _, id) = in_project(cx, dir.path());
    tabs.update_in(cx, |tabs, window, cx| tabs.open_project(&id, window, cx));
    let counts = |cx: &mut VisualTestContext| {
        draw(cx);
        cx.update(|_, cx| {
            let w = tabs.read(cx);
            (
                w.members(&id).len(),
                w.project_replies(&id).len(),
                w.projects.project(&id).unwrap().mapping.exported().len(),
            )
        })
    };
    assert_eq!(counts(cx), (1, 0, 2));
    vault(&tabs, cx)
        .save_reply(mdoc_vault::NewReply {
            owner: mdoc_vault::ReplyOwner::Project(id.clone()),
            title: "Reply".into(),
            source_app: None,
            markdown: "PERSON_1".into(),
        })
        .unwrap();
    closed_member(&tabs, cx, &id, "ORG_1");
    assert_eq!(counts(cx), (2, 1, 2));
}

/// Add document asks for files and imports them into the Project; a
/// cancelled picker imports nothing.
#[gpui::test]
fn add_document_imports_chosen_files_into_the_project(cx: &mut TestAppContext) {
    let dir = tempfile::tempdir().unwrap();
    let (tabs, cx, _, id) = in_project(cx, dir.path());
    tabs.update_in(cx, |tabs, window, cx| tabs.open_project(&id, window, cx));
    draw(cx);
    click_toolbar(cx, "project-page-add-document");
    assert!(cx.did_prompt_for_paths());
    cx.simulate_path_prompt_response(|options| {
        assert!(options.files && options.multiple && !options.directories);
        None
    });
    cx.run_until_parked();
    assert_eq!(vault(&tabs, cx).documents_in(Some(&id)).len(), 1);
    let path = dir.path().join("added.md");
    std::fs::write(&path, "# Added").unwrap();
    tabs.update_in(cx, |tabs, window, cx| tabs.open_project(&id, window, cx));
    click_toolbar(cx, "project-page-add-document");
    cx.simulate_path_prompt_response(move |_| Some(vec![path]));
    cx.run_until_parked();
    assert_eq!(vault(&tabs, cx).documents_in(Some(&id)).len(), 2);
}

/// Documents: every document of the Project; Replies: its replies newest
/// first, each opening restored and deleted after a question; Names:
/// searchable, grouped by category, with Always and Never and Categories to
/// detect.
#[gpui::test]
fn the_three_segments_show_documents_replies_and_names(cx: &mut TestAppContext) {
    let dir = tempfile::tempdir().unwrap();
    let (tabs, cx, _, id) = in_project(cx, dir.path());
    closed_member(&tabs, cx, &id, "PERSON_1");
    tabs.update_in(cx, |tabs, window, cx| tabs.open_project(&id, window, cx));
    draw(cx);
    // Documents (the default).
    assert!(cx.debug_bounds("project-documents").is_some());
    assert!(cx.debug_bounds("project-document-1").is_some());
    assert!(cx.debug_bounds("project-replies").is_none());
    assert!(cx.debug_bounds("project-page-filter").is_none());

    // Replies: empty, then two.
    click_toolbar(cx, "project-segment-replies");
    assert!(cx.debug_bounds("project-replies").is_some());
    assert!(cx.debug_bounds("project-reply-0").is_none());
    let vault = vault(&tabs, cx);
    for (title, markdown) in [("Older", "PERSON_1 wrote."), ("Newer", "ORG_1 wrote.")] {
        vault
            .save_reply(mdoc_vault::NewReply {
                owner: mdoc_vault::ReplyOwner::Project(id.clone()),
                title: title.into(),
                source_app: Some("Claude".into()),
                markdown: markdown.into(),
            })
            .unwrap();
    }
    let listed = vault.project_replies(&id);
    assert_eq!(listed.len(), 2);
    let first = listed[0].id.clone();
    let expected = match listed[0].title.as_str() {
        "Older" => "Anna Petrova wrote.",
        _ => "Northwind Traders Ltd wrote.",
    };
    draw(cx);
    assert!(cx.debug_bounds("project-reply-1").is_some());
    click_toolbar(cx, "project-reply-0");
    cx.update(|_, cx| {
        let page = tabs.read(cx).reply_page.as_ref().unwrap();
        assert_eq!(page.id(), first);
        assert_eq!(page.restored().text, expected);
    });
    // Delete… from the list asks, then the reply goes.
    tabs.update_in(cx, |tabs, window, cx| {
        tabs.open_project(&id, window, cx);
        tabs.show_project_segment(Segment::Replies, cx);
    });
    draw(cx);
    click_toolbar(cx, "project-reply-delete-0");
    cx.simulate_prompt_answer("Delete");
    cx.run_until_parked();
    assert_eq!(vault.project_replies(&id).len(), 1);
    assert!(vault.reply(&first).is_err());

    // Names: grouped by category, searchable, with the lists and categories.
    click_toolbar(cx, "project-segment-names");
    assert!(cx.debug_bounds("project-page-filter").is_some());
    assert!(cx.debug_bounds("project-identity-PERSON_1").is_some());
    assert!(cx.debug_bounds("project-identity-ORG_1").is_some());
    assert!(cx.debug_bounds("page-category-PERSON").is_some());
    assert!(cx.debug_bounds("project-always-remove-0").is_none());
    assert!(cx.debug_bounds("project-documents").is_none());
    let (filter, _) = cx.update(|_, cx| {
        tabs.read(cx)
            .project_ui
            .page
            .as_ref()
            .unwrap()
            .test_inputs()
    });
    filter.update(cx, |input, cx| input.set_value("anna".into(), cx));
    draw(cx);
    assert!(cx.debug_bounds("project-identity-PERSON_1").is_some());
    assert!(cx.debug_bounds("project-identity-ORG_1").is_none());
    filter.update(cx, |input, cx| input.set_value("nobody".into(), cx));
    draw(cx);
    assert!(cx.debug_bounds("project-identity-PERSON_1").is_none());
}
