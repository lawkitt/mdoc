//! ADR 0077: a case overview must not implicitly open documents or keys.
use super::*;
use mdoc_store::cases::{CaseColor, CaseSummary};

#[gpui::test]
fn opening_a_case_is_a_transient_page_without_key_or_member_reads(cx: &mut TestAppContext) {
    let dir = tempfile::tempdir().unwrap();
    let (tabs, cx, _) = applied_document(cx, dir.path(), Session::default());
    let tab = cx.update(|_, cx| tabs.read(cx).tabs.active());
    let id = tabs.update_in(cx, |tabs, window, cx| {
        tabs.create_case(tab, window, cx).unwrap()
    });
    cx.run_until_parked();
    let closed = dir.path().join("closed.md");
    std::fs::write(&closed, "never load this member on page open").unwrap();
    tabs.update(cx, |tabs, _| {
        tabs.case_coordinator.cases_mut()[0].members.push(closed);
    });
    let (before, reads, member_reads) = cx.update(|_, cx| {
        let w = tabs.read(cx);
        (w.snapshot(cx), w.cases.key_reads, w.cases.member_reads)
    });
    tabs.update_in(cx, |tabs, window, cx| tabs.open_case(&id, window, cx));
    cx.run_until_parked();
    cx.update(|_, cx| {
        let w = tabs.read(cx);
        assert_eq!(
            w.snapshot(cx).tabs,
            before.tabs,
            "opening a page must not admit a member"
        );
        assert_eq!(w.cases.key_reads, reads);
        assert_eq!(w.cases.member_reads, member_reads);
        assert_eq!(w.cases.page.as_ref().unwrap().id(), id);
        assert!(!w.showing_home(cx));
        assert!(w.showing_overview(cx));
    });
    draw(cx);
    assert!(cx.debug_bounds("case-page").is_some());
    tabs.update_in(cx, |tabs, window, cx| tabs.activate(tab, window, cx));
    assert!(cx.update(|_, cx| tabs.read(cx).cases.page.is_none()));
}

#[gpui::test]
fn locked_page_and_open_case_chooser_use_only_session_metadata(cx: &mut TestAppContext) {
    let dir = tempfile::tempdir().unwrap();
    let (case, _) = saved_case(dir.path());
    let (tabs, cx) = boot_home(
        cx,
        Session {
            cases: vec![CaseSummary::of(&case)],
            ..Session::default()
        },
    );
    tabs.update_in(cx, |tabs, window, cx| {
        tabs.cases.test_may_ask = true;
        tabs.case_coordinator.lock();
        tabs.show_case_list(window, cx);
    });
    draw(cx);
    let row = cx.debug_bounds("open-case-row").unwrap();
    cx.simulate_click(row.center(), Default::default());
    cx.run_until_parked();
    draw(cx);
    assert!(cx.debug_bounds("case-page-locked").is_some());
    assert!(cx.debug_bounds("case-page-unlock").is_some());
    assert!(cx.debug_bounds("case-page-rename").is_none());
    assert!(cx.debug_bounds("case-page-open-all").is_none());
    assert!(cx.debug_bounds("case-page-delete").is_none());
    cx.update(|_, cx| {
        let w = tabs.read(cx);
        assert_eq!(w.cases.key_reads, 0);
        assert_eq!(w.cases.member_reads, 0);
        assert!(!w.cases.explainer);
        assert_eq!(w.cases.page.as_ref().unwrap().id(), case.id);
    });
    let button = cx.debug_bounds("case-page-unlock").unwrap();
    cx.simulate_click(button.center(), Default::default());
    cx.run_until_parked();
    assert!(cx.update(|_, cx| tabs.read(cx).cases.explainer));
}

#[gpui::test]
fn case_page_actions_persist_and_delete_returns_home_with_members_open(cx: &mut TestAppContext) {
    cx.update(crate::bind_all_keys);
    let dir = tempfile::tempdir().unwrap();
    let (tabs, cx, path) = applied_document(cx, dir.path(), Session::default());
    let tab = cx.update(|_, cx| tabs.read(cx).tabs.active());
    let id = tabs.update_in(cx, |tabs, window, cx| {
        tabs.create_case(tab, window, cx).unwrap()
    });
    cx.run_until_parked();
    tabs.update_in(cx, |tabs, window, cx| tabs.open_case(&id, window, cx));
    draw(cx);
    let color = cx.debug_bounds("case-page-color").unwrap();
    cx.simulate_click(color.center(), Default::default());
    draw(cx);
    let swatch = cx.debug_bounds("case-swatch-Teal").unwrap();
    cx.simulate_click(swatch.center(), Default::default());
    cx.run_until_parked();
    cx.update(|_, cx| {
        assert_eq!(
            tabs.read(cx).case_coordinator.case(&id).unwrap().color,
            Some(CaseColor::Teal)
        );
        assert_eq!(
            tabs.read(cx)
                .active_view()
                .unwrap()
                .read(cx)
                .case
                .link()
                .unwrap()
                .color,
            Some(CaseColor::Teal)
        );
    });
    tabs.update_in(cx, |tabs, window, cx| {
        tabs.start_rename_case(id.clone(), window, cx)
    });
    cx.simulate_input("Petrova v Northwind");
    cx.simulate_keystrokes("enter");
    cx.run_until_parked();
    assert_eq!(
        cx.update(|_, cx| tabs
            .read(cx)
            .case_coordinator
            .case(&id)
            .unwrap()
            .name
            .clone()),
        "Petrova v Northwind"
    );
    tabs.update(cx, |tabs, cx| {
        tabs.add_case_name(&id, "Project Bluebird", cx)
    });
    cx.run_until_parked();
    let alias = cx.update(|_, cx| {
        tabs.read(cx)
            .case_coordinator
            .case(&id)
            .unwrap()
            .identities
            .iter()
            .find(|i| i.originals == ["Project Bluebird"])
            .unwrap()
            .alias
            .clone()
    });
    tabs.update_in(cx, |tabs, window, cx| {
        tabs.confirm_forget_case_identity(&id, &alias, window, cx)
    });
    cx.simulate_prompt_answer("Cancel");
    cx.run_until_parked();
    assert!(cx.update(|_, cx| {
        tabs.read(cx)
            .case_coordinator
            .case(&id)
            .unwrap()
            .exported()
            .iter()
            .any(|i| i.alias == alias)
    }));
    tabs.update_in(cx, |tabs, window, cx| {
        tabs.confirm_forget_case_identity(&id, &alias, window, cx)
    });
    cx.simulate_prompt_answer("Forget Identity");
    cx.run_until_parked();
    let store = mdoc_store::cases::Store::with_key(dir.path().join("cases"), &[3; 32]);
    let saved = store.load(&id).unwrap();
    assert_eq!(saved.color, Some(CaseColor::Teal));
    assert_eq!(saved.name, "Petrova v Northwind");
    assert!(!saved.exported().iter().any(|i| i.alias == alias));
    assert_eq!(saved.members, [session_store::identity(&path)]);
    tabs.update_in(cx, |tabs, window, cx| {
        tabs.delete_case(id.clone(), window, cx)
    });
    cx.simulate_prompt_answer("Cancel");
    cx.run_until_parked();
    assert!(cx.update(|_, cx| tabs.read(cx).cases.page.is_some()));
    tabs.update_in(cx, |tabs, window, cx| {
        tabs.delete_case(id.clone(), window, cx)
    });
    cx.simulate_prompt_answer("Delete Case");
    cx.run_until_parked();
    cx.update(|_, cx| {
        let w = tabs.read(cx);
        assert!(w.showing_home(cx));
        assert!(w.tabs.contains(tab));
        assert!(w.tabs.get(tab).unwrap().record().case.is_none());
        assert!(w.active_view().unwrap().read(cx).case.link().is_none());
        assert!(
            !w.active_view()
                .unwrap()
                .read(cx)
                .pii
                .review()
                .export_identities()
                .is_empty()
        );
    });
    assert!(store.load(&id).is_err());
}

#[gpui::test]
fn page_removes_closed_missing_and_open_members_without_loading_them(cx: &mut TestAppContext) {
    let dir = tempfile::tempdir().unwrap();
    let (tabs, cx, path) = applied_document(cx, dir.path(), Session::default());
    let tab = cx.update(|_, cx| tabs.read(cx).tabs.active());
    let id = tabs.update_in(cx, |tabs, window, cx| {
        tabs.create_case(tab, window, cx).unwrap()
    });
    cx.run_until_parked();
    let closed = dir.path().join("closed.md");
    let missing = dir.path().join("missing.md");
    std::fs::write(&closed, "PERSON_1").unwrap();
    tabs.update(cx, |tabs, _| {
        tabs.case_coordinator.cases_mut()[0]
            .members
            .extend([closed.clone(), missing.clone()])
    });
    tabs.update_in(cx, |tabs, window, cx| tabs.open_case(&id, window, cx));
    draw(cx);
    assert!(cx.debug_bounds("case-document-0").is_some());
    assert!(cx.debug_bounds("case-document-1").is_some());
    assert!(
        cx.debug_bounds("case-document-2").is_none(),
        "missing files have only Remove"
    );
    let reads = cx.update(|_, cx| tabs.read(cx).cases.member_reads);
    for path in [missing, closed, session_store::identity(&path)] {
        tabs.update_in(cx, |tabs, window, cx| {
            tabs.remove_case_member(&id, &path, window, cx)
        });
        cx.run_until_parked();
    }
    cx.update(|_, cx| {
        let w = tabs.read(cx);
        assert_eq!(w.cases.member_reads, reads);
        assert!(w.case_coordinator.case(&id).unwrap().members.is_empty());
        assert!(
            w.tabs
                .get(tab)
                .unwrap()
                .view()
                .unwrap()
                .read(cx)
                .case
                .link()
                .is_none()
        );
        assert!(w.cases.page.is_some());
    });
}

#[gpui::test]
fn page_document_activation_and_open_all_are_explicit(cx: &mut TestAppContext) {
    let dir = tempfile::tempdir().unwrap();
    let (tabs, cx, _) = applied_document(cx, dir.path(), Session::default());
    let tab = cx.update(|_, cx| tabs.read(cx).tabs.active());
    let id = tabs.update_in(cx, |tabs, window, cx| {
        tabs.create_case(tab, window, cx).unwrap()
    });
    cx.run_until_parked();
    // Hosted temp roots can be aliases. Exercise normalization locally too.
    std::fs::create_dir(dir.path().join("members")).unwrap();
    let closed = dir.path().join("members").join("..").join("closed.md");
    std::fs::write(&closed, "PERSON_1").unwrap();
    let closed = session_store::identity(&closed);
    tabs.update(cx, |tabs, _| {
        tabs.case_coordinator.cases_mut()[0]
            .members
            .push(closed.clone())
    });
    tabs.update_in(cx, |tabs, window, cx| tabs.open_case(&id, window, cx));
    draw(cx);
    let row = cx.debug_bounds("case-document-0").unwrap();
    cx.simulate_click(row.center(), Default::default());
    cx.run_until_parked();
    assert!(cx.update(|_, cx| tabs.read(cx).cases.page.is_none()));
    tabs.update_in(cx, |tabs, window, cx| {
        tabs.open_case(&id, window, cx);
        tabs.open_all_case_members(&id, window, cx);
    });
    cx.run_until_parked();
    cx.update(|_, cx| {
        let w = tabs.read(cx);
        assert!(w.cases.page.is_none());
        assert!(w.find_path(&closed, None, cx).is_some());
        assert_eq!(w.tabs.len(), 2);
    });
}

#[gpui::test]
fn names_filter_add_binding_and_every_page_control_are_keyboard_reachable(cx: &mut TestAppContext) {
    cx.update(crate::bind_all_keys);
    let dir = tempfile::tempdir().unwrap();
    let (tabs, cx, _) = applied_document(cx, dir.path(), Session::default());
    let tab = cx.update(|_, cx| tabs.read(cx).tabs.active());
    let id = tabs.update_in(cx, |tabs, window, cx| {
        tabs.create_case(tab, window, cx).unwrap()
    });
    cx.run_until_parked();
    tabs.update(cx, |tabs, cx| {
        for index in 0..9 {
            tabs.add_case_name(&id, &format!("Project Example {index}"), cx);
        }
    });
    tabs.update_in(cx, |tabs, window, cx| tabs.open_case(&id, window, cx));
    cx.simulate_resize(size(px(720.), px(480.)));
    draw(cx);
    assert!(cx.debug_bounds("case-page-filter").is_some());
    let (filter, add) = cx.update(|_, cx| tabs.read(cx).cases.page.as_ref().unwrap().test_inputs());
    cx.update(|window, cx| window.focus(&filter.read(cx).focus_handle(cx), cx));
    cx.simulate_input("Northwind");
    draw(cx);
    assert!(cx.debug_bounds("case-identity-ORG_1").is_some());
    assert!(cx.debug_bounds("case-identity-PERSON_1").is_none());
    filter.update(cx, |input, cx| input.reset(cx));
    draw(cx);
    cx.update(|window, cx| window.focus(&add.read(cx).focus_handle(cx), cx));
    cx.simulate_input("New name by keyboard");
    cx.simulate_keystrokes("enter");
    cx.run_until_parked();
    assert!(cx.update(|_, cx| {
        tabs.read(cx)
            .case_coordinator
            .case(&id)
            .unwrap()
            .identities
            .iter()
            .any(|identity| identity.originals == ["New name by keyboard"])
    }));
    assert_eq!(cx.update(|_, cx| add.read(cx).value().to_string()), "");
    draw(cx);
    let targets = cx.update(|_, cx| tabs.read(cx).cases.page.as_ref().unwrap().test_controls());
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
        let viewport = cx.update(|_, cx| {
            tabs.read(cx)
                .cases
                .page
                .as_ref()
                .unwrap()
                .test_scroll_bounds()
        });
        assert!(
            bounds.top() >= viewport.top() && bounds.bottom() <= viewport.bottom(),
            "{name}: {bounds:?} outside {viewport:?}"
        );
    }
}

#[gpui::test]
fn an_old_empty_manifest_can_discover_cases_through_explicit_unlock(cx: &mut TestAppContext) {
    let dir = tempfile::tempdir().unwrap();
    let (case, _) = saved_case(dir.path());
    let (tabs, cx) = boot_home(cx, Session::default());
    tabs.update_in(cx, |tabs, window, cx| {
        tabs.stage_case_key(Ok(mdoc_store::cases::Store::with_key(
            dir.path().join("cases"),
            &[3; 32],
        )));
        tabs.cases.test_may_ask = false;
        tabs.show_case_list(window, cx);
    });
    draw(cx);
    assert!(cx.debug_bounds("open-case-row").is_none());
    let unlock = cx.debug_bounds("case-list-unlock").unwrap();
    cx.simulate_click(unlock.center(), Default::default());
    cx.run_until_parked();
    draw(cx);
    assert!(cx.debug_bounds("open-case-row").is_some());
    cx.update(|_, cx| {
        let w = tabs.read(cx);
        assert!(w.case_coordinator.case(&case.id).is_some());
        assert_eq!(w.cases.member_reads, 0);
    });
}

#[gpui::test]
fn cached_page_unlocks_from_home_and_a_missing_case_returns_home(cx: &mut TestAppContext) {
    for missing in [false, true] {
        let dir = tempfile::tempdir().unwrap();
        let (case, _) = saved_case(dir.path());
        if missing {
            std::fs::remove_file(dir.path().join("cases").join(format!("{}.case", case.id)))
                .unwrap();
        }
        let (tabs, cx) = boot_home(
            cx,
            Session {
                cases: vec![CaseSummary::of(&case)],
                ..Session::default()
            },
        );
        tabs.update_in(cx, |tabs, window, cx| {
            tabs.stage_case_key(Ok(mdoc_store::cases::Store::with_key(
                dir.path().join("cases"),
                &[3; 32],
            )));
            tabs.cases.test_may_ask = false;
            tabs.open_case(&case.id, window, cx);
        });
        draw(cx);
        let unlock = cx.debug_bounds("case-page-unlock").unwrap();
        cx.simulate_click(unlock.center(), Default::default());
        cx.run_until_parked();
        draw(cx);
        cx.update(|_, cx| {
            let w = tabs.read(cx);
            if missing {
                assert!(w.showing_home(cx));
                assert!(w.cases.page.is_none());
            } else {
                assert!(w.cases.page.is_some());
                assert!(w.case_coordinator.case(&case.id).is_some());
            }
            assert_eq!(w.cases.member_reads, 0);
        });
    }
}

#[gpui::test]
fn removing_a_lazy_member_clears_its_record_before_activation(cx: &mut TestAppContext) {
    let dir = tempfile::tempdir().unwrap();
    let (case, path) = saved_case(dir.path());
    std::fs::create_dir(dir.path().join("members")).unwrap();
    let path = session_store::identity(
        &dir.path()
            .join("members")
            .join("..")
            .join(path.file_name().unwrap()),
    );
    let other = dir.path().join("other.md");
    std::fs::write(&other, "# Other").unwrap();
    let (tabs, cx) = boot_home(
        cx,
        Session {
            tabs: vec![
                TabRecord {
                    markdown: session_store::identity(&other),
                    ..TabRecord::default()
                },
                TabRecord {
                    markdown: path.clone(),
                    case: Some(case.id.clone()),
                    ..TabRecord::default()
                },
            ],
            cases: vec![CaseSummary::of(&case)],
            ..Session::default()
        },
    );
    let lazy = cx.update(|_, cx| tabs.read(cx).tabs.last().unwrap().id());
    tabs.update(cx, |tabs, _| {
        tabs.use_case_store(mdoc_store::cases::Store::with_key(
            dir.path().join("cases"),
            &[3; 32],
        ));
        tabs.case_coordinator.finish_load(vec![case.clone()]);
        assert!(tabs.tabs.get(lazy).unwrap().view().is_none());
    });
    tabs.update_in(cx, |tabs, window, cx| {
        tabs.open_case(&case.id, window, cx);
        tabs.remove_case_member(&case.id, &path, window, cx);
        assert!(tabs.tabs.get(lazy).unwrap().record().case.is_none());
        tabs.activate(lazy, window, cx);
    });
    cx.run_until_parked();
    cx.update(|_, cx| {
        let w = tabs.read(cx);
        assert!(
            !w.case_coordinator
                .case(&case.id)
                .unwrap()
                .members
                .contains(&path)
        );
        assert!(
            w.tabs
                .get(lazy)
                .unwrap()
                .view()
                .unwrap()
                .read(cx)
                .case
                .link()
                .is_none()
        );
    });
}

#[gpui::test]
fn forgetting_an_unused_seed_cannot_reappear_when_typed_later(cx: &mut TestAppContext) {
    let dir = tempfile::tempdir().unwrap();
    let (tabs, cx, _) = applied_document(cx, dir.path(), Session::default());
    let tab = cx.update(|_, cx| tabs.read(cx).tabs.active());
    let id = tabs.update_in(cx, |tabs, window, cx| {
        tabs.create_case(tab, window, cx).unwrap()
    });
    tabs.update(cx, |tabs, cx| {
        tabs.add_case_name(&id, "Forgotten Bluebird", cx)
    });
    let alias = cx.update(|_, cx| {
        tabs.read(cx)
            .case_coordinator
            .case(&id)
            .unwrap()
            .identities
            .last()
            .unwrap()
            .alias
            .clone()
    });
    tabs.update(cx, |tabs, cx| tabs.forget_case_identity(&id, &alias, cx));
    let view = active(&tabs, cx);
    view.update(cx, |view, cx| {
        let source = format!("{} Forgotten Bluebird", view.editor.read(cx).text());
        view.editor
            .update(cx, |editor, cx| editor.set_text(source.clone(), cx));
        view.pii.review_mut().refresh(&source);
        assert!(
            !view
                .pii
                .review()
                .export_identities()
                .iter()
                .any(|identity| identity.alias == alias)
        );
    });
}

#[gpui::test]
fn share_and_sidebar_header_open_the_same_cached_page(cx: &mut TestAppContext) {
    let dir = tempfile::tempdir().unwrap();
    let (tabs, cx, _) = applied_document(cx, dir.path(), Session::default());
    let tab = cx.update(|_, cx| tabs.read(cx).tabs.active());
    let id = tabs.update_in(cx, |tabs, window, cx| {
        tabs.create_case(tab, window, cx).unwrap()
    });
    let view = active(&tabs, cx);
    view.update_in(cx, |view, window, cx| {
        view.inspector_share = true;
        if !view.replacements_open() {
            view.toggle_replacements_panel(window, cx);
        }
    });
    draw(cx);
    let share = cx.debug_bounds("share-case-page").unwrap();
    cx.simulate_click(share.center(), Default::default());
    cx.run_until_parked();
    assert_eq!(
        cx.update(|_, cx| tabs.read(cx).cases.page.as_ref().unwrap().id().to_string()),
        id
    );
    tabs.update_in(cx, |tabs, window, cx| tabs.activate(tab, window, cx));
    draw(cx);
    let header = cx.debug_bounds("case-page-link").unwrap();
    cx.simulate_click(header.center(), Default::default());
    cx.run_until_parked();
    assert_eq!(
        cx.update(|_, cx| tabs.read(cx).cases.page.as_ref().unwrap().id().to_string()),
        id
    );
}

#[gpui::test]
fn a_long_case_name_stays_inside_the_narrow_page(cx: &mut TestAppContext) {
    let dir = tempfile::tempdir().unwrap();
    let (tabs, cx, _) = applied_document(cx, dir.path(), Session::default());
    let tab = cx.update(|_, cx| tabs.read(cx).tabs.active());
    let id = tabs.update_in(cx, |tabs, window, cx| {
        tabs.create_case(tab, window, cx).unwrap()
    });
    tabs.update(cx, |tabs, _| tabs.case_coordinator.cases_mut()[0].name = "A long case name about acceptance of the first shipment with several parties and exhibits".into());
    tabs.update_in(cx, |tabs, window, cx| tabs.open_case(&id, window, cx));
    cx.simulate_resize(size(px(720.), px(480.)));
    draw(cx);
    let name = cx.debug_bounds("case-page-rename").unwrap();
    let page = cx.debug_bounds("case-page").unwrap();
    assert!(
        name.left() >= page.left() && name.right() <= page.right(),
        "name {name:?} outside page {page:?}"
    );
}

#[gpui::test]
fn case_close_all_resolves_dirty_members_in_turn_and_cancel_stops_it(cx: &mut TestAppContext) {
    let dir = tempfile::tempdir().unwrap();
    let (tabs, cx, _) = applied_document(cx, dir.path(), Session::default());
    let first = cx.update(|_, cx| tabs.read(cx).tabs.active());
    let id = tabs.update_in(cx, |tabs, window, cx| {
        tabs.create_case(first, window, cx).unwrap()
    });
    let path = dir.path().join("second.md");
    std::fs::write(&path, "# Second").unwrap();
    tabs.update_in(cx, |tabs, window, cx| {
        tabs.open_paths(vec![path], window, cx)
    });
    cx.run_until_parked();
    let second = cx.update(|_, cx| tabs.read(cx).tabs.active());
    tabs.update_in(cx, |tabs, window, cx| {
        tabs.join_case(second, &id, window, cx)
    });
    active(&tabs, cx).update(cx, |view, cx| {
        view.editor.update(cx, |editor, cx| {
            editor.set_text("Changed second document", cx)
        })
    });
    let close_all = |cx: &mut VisualTestContext| {
        draw(cx);
        let menu = cx.debug_bounds("case-header-menu").unwrap();
        cx.simulate_click(menu.center(), Default::default());
        draw(cx);
        let close = cx.debug_bounds("case-action-close-all").unwrap();
        cx.simulate_click(close.center(), Default::default());
        cx.run_until_parked();
    };
    close_all(cx);
    assert!(cx.has_pending_prompt());
    cx.simulate_prompt_answer("Cancel");
    cx.run_until_parked();
    assert!(cx.update(|_, cx| tabs.read(cx).tabs.contains(first) && tabs.read(cx).tabs.contains(second)));
    close_all(cx);
    cx.simulate_prompt_answer("Discard");
    cx.run_until_parked();
    assert!(
        cx.has_pending_prompt(),
        "the second dirty member must be asked next"
    );
    cx.simulate_prompt_answer("Discard");
    cx.run_until_parked();
    assert!(cx.update(
        |_, cx| !tabs.read(cx).tabs.contains(first) && !tabs.read(cx).tabs.contains(second)
    ));
}
