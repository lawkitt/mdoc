//! ADR 0077: a case overview must not implicitly open documents or keys.
use super::*;
use mdoc_store::cases::{CaseColor, CaseSummary};

#[gpui::test]
fn dropped_document_join_uses_the_open_tabs_identity_and_live_mentions(cx: &mut TestAppContext) {
    let dir = tempfile::tempdir().unwrap();
    std::fs::create_dir(dir.path().join("incoming")).unwrap();
    // Force the same raw/canonical mismatch as /var vs /private/var on macOS.
    let path = dir
        .path()
        .join("incoming")
        .join("..")
        .join("Services Agreement.md");
    let source = "Anna met Anna";
    std::fs::write(&path, source).unwrap();
    let (tabs, cx) = boot_home(cx, Session::default());
    tabs.update_in(cx, |tabs, window, cx| {
        tabs.use_case_store(mdoc_store::cases::Store::with_key(
            dir.path().join("cases"),
            &[3; 32],
        ));
        tabs.case_coordinator
            .finish_load(vec![mdoc_store::cases::Case::new(
                "Public fixture".into(),
                &[],
                None,
            )]);
        tabs.drop_files(
            &gpui::ExternalPaths(vec![path.clone()].into()),
            false,
            window,
            cx,
        );
    });
    cx.run_until_parked();
    let view = active(&tabs, cx);
    view.update_in(cx, |view, window, cx| {
        view.pii
            .review_mut()
            .add_manual(source, 0..4, mdoc_pii::Category::Person)
            .unwrap();
        view.apply_all_pii(&PiiApplyAll, window, cx);
    });
    cx.run_until_parked();
    let (case_id, tab) = tabs.update_in(cx, |tabs, window, cx| {
        let id = tabs.case_coordinator.cases()[0].id.clone();
        let tab = tabs.tabs.active();
        tabs.join_case(tab, &id, window, cx);
        tabs.open_case(&id, window, cx);
        (id, tab)
    });
    cx.run_until_parked();
    let (member, tab_count) = cx.update(|_, cx| {
        let w = tabs.read(cx);
        let case = w.case_coordinator.case(&case_id).unwrap();
        let member = case.members[0].clone();
        assert_ne!(member, path);
        assert_eq!(member, session_store::identity(&path));
        assert_eq!(w.tabs.get(tab).unwrap().identity(), Some(&member));
        assert_eq!(
            w.find_path(&member, None, cx),
            Some(tab),
            "page must show Open"
        );
        let review = view.read(cx).pii.review();
        let identity = review.identity_by_alias(&case.exported()[0].alias).unwrap();
        assert_eq!(
            review.identity_count(identity),
            2,
            "page must show live mentions"
        );
        (member, w.tabs.len())
    });
    tabs.update_in(cx, |tabs, window, cx| {
        tabs.open_case_member(member, window, cx)
    });
    cx.update(|_, cx| {
        let w = tabs.read(cx);
        assert_eq!(w.tabs.active(), tab);
        assert_eq!(
            w.tabs.len(),
            tab_count,
            "activate the open member without duplicating it"
        );
    });
}
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
    let add = cx.update(|_, cx| tabs.read(cx).cases.page.as_ref().unwrap().test_inputs().1);
    assert!(
        cx.update(|window, cx| add.read(cx).focus_handle(cx).is_focused(window)),
        "forget returns focus to Add a name"
    );
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
fn the_case_page_scrolls_with_the_wheel(cx: &mut TestAppContext) {
    case_page_scrolls(cx, true);
}

#[gpui::test]
fn the_case_page_scrolls_with_the_sidebar_collapsed(cx: &mut TestAppContext) {
    case_page_scrolls(cx, false);
}

fn case_page_scrolls(cx: &mut TestAppContext, sidebar_visible: bool) {
    cx.update(crate::bind_all_keys);
    let dir = tempfile::tempdir().unwrap();
    let (tabs, cx, _) = applied_document(cx, dir.path(), Session::default());
    if !sidebar_visible {
        draw(cx);
        click_toolbar(cx, "sidebar-toggle");
    }
    assert_eq!(
        cx.update(|_, cx| tabs.read(cx).sidebar.visible()),
        sidebar_visible
    );
    let tab = cx.update(|_, cx| tabs.read(cx).tabs.active());
    let id = tabs.update_in(cx, |tabs, window, cx| {
        tabs.create_case(tab, window, cx).unwrap()
    });
    cx.run_until_parked();
    tabs.update(cx, |tabs, cx| {
        for index in 0..20 {
            tabs.add_case_name(&id, &format!("Project Example {index}"), cx);
        }
    });
    tabs.update_in(cx, |tabs, window, cx| tabs.open_case(&id, window, cx));
    cx.simulate_resize(size(px(720.), px(480.)));
    draw(cx);
    let offset = |cx: &mut VisualTestContext| {
        cx.update(|_, cx| {
            tabs.read(cx)
                .cases
                .page
                .as_ref()
                .unwrap()
                .test_scroll_offset()
        })
    };
    assert_eq!(offset(cx), px(0.));
    // A focused field near the top must not pull the page back each frame.
    let (filter, _) = cx.update(|_, cx| tabs.read(cx).cases.page.as_ref().unwrap().test_inputs());
    cx.update(|window, cx| window.focus(&filter.read(cx).focus_handle(cx), cx));
    draw(cx);
    draw(cx);
    let viewport = cx.debug_bounds("case-page").unwrap();
    cx.simulate_event(gpui::ScrollWheelEvent {
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
        cx.simulate_event(gpui::ScrollWheelEvent {
            position: viewport.center(),
            delta: gpui::ScrollDelta::Pixels(gpui::point(px(0.), px(-400.))),
            ..Default::default()
        });
        draw(cx);
    }
    let window = cx.update(|window, _| window.viewport_size());
    let scroll = cx.update(|_, cx| {
        tabs.read(cx)
            .cases
            .page
            .as_ref()
            .unwrap()
            .test_scroll_bounds()
    });
    let delete = cx.debug_bounds("case-page-delete").unwrap();
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

#[gpui::test]
fn forgetting_the_highest_alias_reserves_it_in_new_members_and_survives_reopen(
    cx: &mut TestAppContext,
) {
    let dir = tempfile::tempdir().unwrap();
    let (tabs, cx, _) = applied_document(cx, dir.path(), Session::default());
    let first = cx.update(|_, cx| tabs.read(cx).tabs.active());
    let id = tabs.update_in(cx, |tabs, window, cx| {
        tabs.create_case(first, window, cx).unwrap()
    });
    let second = dir.path().join("second.md");
    std::fs::write(&second, "Carol").unwrap();
    let second = session_store::identity(&second);
    tabs.update(cx, |tabs, cx| {
        let (_, effect) = tabs
            .case_coordinator
            .edit(&id, |case| {
                case.identities.push(mdoc_store::cases::StoredIdentity {
                    alias: "PERSON_2".into(),
                    previous: vec![],
                    category: "PERSON".into(),
                    custom: false,
                    originals: vec!["Bob".into()],
                });
                case.members.push(second.clone());
                Some(())
            })
            .unwrap();
        tabs.carry_out_cases(effect.into_iter().collect(), cx);
        tabs.forget_case_identity(&id, "PERSON_2", cx);
    });
    tabs.update_in(cx, |tabs, window, cx| {
        tabs.open_case_member(second.clone(), window, cx)
    });
    cx.run_until_parked();
    let member = cx.update(|_, cx| tabs.read(cx).tabs.active());
    let view = active(&tabs, cx);
    view.update_in(cx, |view, window, cx| {
        view.pii.set_reviewing(true);
        view.pii
            .review_mut()
            .add_manual("Carol", 0..5, mdoc_pii::Category::Person)
            .unwrap();
        view.apply_all_pii(&PiiApplyAll, window, cx);
        assert_eq!(view.editor.read(cx).text(), "PERSON_3");
        view.write(second.clone(), None, cx);
    });
    cx.run_until_parked();
    tabs.update_in(cx, |tabs, window, cx| {
        tabs.case_changed(member, cx);
        tabs.close_tab(member, window, cx);
    });
    cx.run_until_parked();
    tabs.update_in(cx, |tabs, window, cx| {
        tabs.open_case_member(second.clone(), window, cx)
    });
    cx.run_until_parked();
    let reopened = active(&tabs, cx);
    cx.update(|_, cx| {
        let identities = reopened.read(cx).pii.review().export_identities();
        assert!(
            identities
                .iter()
                .any(|identity| identity.alias == "PERSON_3" && identity.wording() == "Carol")
        );
        assert!(
            !identities
                .iter()
                .any(|identity| identity.alias == "PERSON_2")
        );
    });
    let store = mdoc_store::cases::Store::with_key(dir.path().join("cases"), &[3; 32]);
    assert!(
        store
            .load(&id)
            .unwrap()
            .exported()
            .iter()
            .any(|identity| identity.alias == "PERSON_3" && identity.wording() == "Carol")
    );
}

#[gpui::test]
fn detached_documents_keep_their_mapping_and_explicit_rejoin_uses_a_fresh_alias(
    cx: &mut TestAppContext,
) {
    let dir = tempfile::tempdir().unwrap();
    let (tabs, cx, _) = applied_document(cx, dir.path(), Session::default());
    let tab = cx.update(|_, cx| tabs.read(cx).tabs.active());
    let id = tabs.update_in(cx, |tabs, window, cx| {
        tabs.create_case(tab, window, cx).unwrap()
    });
    tabs.update_in(cx, |tabs, window, cx| {
        tabs.remove_from_case(tab, window, cx)
    });
    tabs.update(cx, |tabs, cx| {
        tabs.forget_case_identity(&id, "PERSON_1", cx)
    });
    let view = active(&tabs, cx);
    cx.update(|_, cx| {
        assert!(
            view.read(cx)
                .pii
                .review()
                .export_identities()
                .iter()
                .any(|identity| identity.alias == "PERSON_1")
        )
    });
    tabs.update_in(cx, |tabs, window, cx| tabs.join_case(tab, &id, window, cx));
    cx.run_until_parked();
    cx.update(|_, cx| {
        assert!(
            view.read(cx)
                .pii
                .review()
                .export_identities()
                .iter()
                .all(|identity| identity.alias != "PERSON_1")
        );
        assert!(
            tabs.read(cx)
                .case_coordinator
                .case(&id)
                .unwrap()
                .exported()
                .iter()
                .any(
                    |identity| identity.alias == "PERSON_2" && identity.wording() == "Anna Petrova"
                )
        );
    });
}

#[gpui::test]
fn swatches_and_inline_rename_are_reachable_and_activatable_by_keyboard(cx: &mut TestAppContext) {
    cx.update(crate::bind_all_keys);
    let dir = tempfile::tempdir().unwrap();
    let (tabs, cx, _) = applied_document(cx, dir.path(), Session::default());
    let tab = cx.update(|_, cx| tabs.read(cx).tabs.active());
    let id = tabs.update_in(cx, |tabs, window, cx| {
        tabs.create_case(tab, window, cx).unwrap()
    });
    tabs.update_in(cx, |tabs, window, cx| tabs.open_case(&id, window, cx));
    draw(cx);
    draw(cx);
    let color = cx.update(|_, cx| {
        tabs.read(cx)
            .cases
            .page
            .as_ref()
            .unwrap()
            .test_controls()
            .into_iter()
            .find(|(id, _)| id == "case-page-color")
            .unwrap()
            .1
    });
    cx.update(|window, cx| window.focus(&color, cx));
    draw(cx);
    activate_control(cx);
    let swatches = cx.update(|_, cx| {
        tabs.read(cx)
            .cases
            .page
            .as_ref()
            .unwrap()
            .test_controls()
            .into_iter()
            .filter(|(id, _)| id.starts_with("case-swatch-"))
            .collect::<Vec<_>>()
    });
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
    let blue = swatches
        .iter()
        .find(|(id, _)| id == "case-swatch-Blue")
        .unwrap()
        .1
        .clone();
    cx.update(|window, cx| window.focus(&blue, cx));
    draw(cx);
    activate_control(cx);
    assert_eq!(
        cx.update(|_, cx| tabs.read(cx).case_coordinator.case(&id).unwrap().color),
        Some(CaseColor::Blue)
    );
    let rename = cx.update(|_, cx| {
        tabs.read(cx)
            .cases
            .page
            .as_ref()
            .unwrap()
            .test_controls()
            .into_iter()
            .find(|(id, _)| id == "case-page-rename")
            .unwrap()
            .1
    });
    cx.update(|window, cx| window.focus(&rename, cx));
    draw(cx);
    activate_control(cx);
    cx.simulate_input("Renamed from keyboard");
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
        "Renamed from keyboard"
    );
}

#[gpui::test]
fn forgetting_a_case_alias_preserves_a_joined_identity_renumbered_from_it(cx: &mut TestAppContext) {
    let dir = tempfile::tempdir().unwrap();
    let (tabs, cx, _) = applied_document(cx, dir.path(), Session::default());
    let first = cx.update(|_, cx| tabs.read(cx).tabs.active());
    let case_id = tabs.update_in(cx, |tabs, window, cx| {
        tabs.create_case(first, window, cx).unwrap()
    });
    tabs.update(cx, |tabs, cx| {
        let (_, effect) = tabs
            .case_coordinator
            .edit(&case_id, |case| {
                case.identities.push(mdoc_store::cases::StoredIdentity {
                    alias: "PERSON_2".into(),
                    previous: vec![],
                    category: "PERSON".into(),
                    custom: false,
                    originals: vec!["Other Anna".into()],
                });
                Some(())
            })
            .unwrap();
        tabs.carry_out_cases(effect.into_iter().collect(), cx);
    });
    let joined = open_applied(
        &tabs,
        cx,
        dir.path(),
        "joined.md",
        "Mark met Bob",
        &[
            ("Mark", mdoc_pii::Category::Person),
            ("Bob", mdoc_pii::Category::Person),
        ],
    );
    tabs.update_in(cx, |tabs, window, cx| {
        tabs.join_case(joined, &case_id, window, cx)
    });
    cx.run_until_parked();
    let view = active(&tabs, cx);
    let (before, bob_alias) = cx.update(|_, cx| {
        let view = view.read(cx);
        let bob = view
            .pii
            .review()
            .export_identities()
            .into_iter()
            .find(|identity| identity.wording() == "Bob")
            .unwrap();
        assert_ne!(bob.alias, "PERSON_2");
        (view.editor.read(cx).text().to_string(), bob.alias)
    });
    tabs.update(cx, |tabs, cx| {
        tabs.forget_case_identity(&case_id, "PERSON_2", cx)
    });
    cx.update(|_, cx| {
        let view = view.read(cx);
        assert_eq!(view.editor.read(cx).text(), before);
        assert!(
            view.pii
                .review()
                .export_identities()
                .iter()
                .any(|identity| identity.alias == bob_alias && identity.wording() == "Bob"),
            "a joined identity's creation alias must not select it for forgetting"
        );
        assert!(
            view.pii
                .review()
                .applied()
                .iter()
                .any(|applied| applied.step.after.as_ref() == bob_alias)
        );
    });
}

#[gpui::test]
fn leaving_the_rename_field_saves_and_escape_still_cancels(cx: &mut TestAppContext) {
    cx.update(crate::bind_all_keys);
    let dir = tempfile::tempdir().unwrap();
    let (tabs, cx, _) = applied_document(cx, dir.path(), Session::default());
    let tab = cx.update(|_, cx| tabs.read(cx).tabs.active());
    let id = tabs.update_in(cx, |tabs, window, cx| {
        tabs.create_case(tab, window, cx).unwrap()
    });
    // Focus-out events reach listeners only in an active window.
    cx.update(|window, _| window.activate_window());
    tabs.update_in(cx, |tabs, window, cx| tabs.open_case(&id, window, cx));
    draw(cx);
    let name = |cx: &mut VisualTestContext| {
        cx.update(|_, cx| {
            tabs.read(cx)
                .case_coordinator
                .case(&id)
                .unwrap()
                .name
                .clone()
        })
    };
    let original = name(cx);
    let rename = |cx: &mut VisualTestContext, value: &str| {
        tabs.update_in(cx, |tabs, window, cx| {
            tabs.start_rename_case(id.clone(), window, cx)
        });
        let input = cx.update(|_, cx| tabs.read(cx).cases.renaming.as_ref().unwrap().1.clone());
        input.update(cx, |input, cx| input.set_value(value.to_string(), cx));
        draw(cx);
    };
    let renaming =
        |cx: &mut VisualTestContext| cx.update(|_, cx| tabs.read(cx).cases.renaming.is_some());

    // Escape cancels.
    rename(cx, "Not kept");
    cx.simulate_keystrokes("escape");
    draw(cx);
    assert!(!renaming(cx));
    assert_eq!(name(cx), original);

    // A click on a blank part of the page saves.
    rename(cx, "Saved by a click");
    let page = cx.debug_bounds("case-page").unwrap();
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
}
