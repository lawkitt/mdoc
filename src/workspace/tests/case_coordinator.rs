//! Saved cases across tabs: saving, joining, the Keychain and quitting (`CaseCoordinator`).
use super::*;
use crate::document_view::save_prompt;

#[gpui::test]
fn unsaved_mapping_is_absent_after_session_restore(cx: &mut TestAppContext) {
    let dir = tempfile::tempdir().unwrap();
    let (tabs, cx, path) = applied_document(cx, dir.path(), Session::default());
    let view = active(&tabs, cx);
    // Saving Markdown retains alias tokens, but does not save their originals.
    view.update(cx, |view, cx| view.write(path, None, cx));
    cx.run_until_parked();
    let manifest = dir.path().join("session.json");
    cx.update(|_, cx| session_store::save(&manifest, &tabs.read(cx).snapshot(cx)).unwrap());
    let json = std::fs::read_to_string(&manifest).unwrap();
    assert!(!json.contains("Anna Petrova"));
    assert!(!json.contains("Northwind Traders"));
    let saved = session_store::load(&manifest).unwrap();
    assert!(saved.tabs[0].case.is_none());
    let restored = cx.update(|window, cx| {
        cx.new(|cx| {
            let mut restored = Workspace::empty(window, cx);
            restored.use_case_store(mdoc_store::cases::Store::with_key(
                dir.path().join("cases"),
                &[3; 32],
            ));
            restored.restore(saved, window, cx);
            restored
        })
    });
    cx.run_until_parked();
    let reopened = active(&restored, cx);
    cx.update(|_, cx| {
        assert_eq!(
            reopened.read(cx).editor.read(cx).text(),
            "PERSON_1 met ORG_1."
        );
        assert!(reopened.read(cx).case.link().is_none());
        assert!(
            reopened
                .read(cx)
                .pii
                .review()
                .active_identities()
                .is_empty()
        );
    });
}

#[gpui::test]
fn a_saved_case_is_encrypted_named_and_listed_in_the_sidebar(cx: &mut TestAppContext) {
    let dir = tempfile::tempdir().unwrap();
    let (tabs, cx, path) = applied_document(cx, dir.path(), Session::default());
    let tab = cx.update(|_, cx| tabs.read(cx).tabs.active());
    cx.update(|_, cx| assert!(tabs.read(cx).can_create_case(tab, cx)));
    tabs.update_in(cx, |tabs, window, cx| tabs.create_case(tab, window, cx));
    cx.run_until_parked();
    let store = mdoc_store::cases::Store::with_key(dir.path().join("cases"), &[3; 32]);
    let (cases, _) = store.list();
    assert_eq!(cases.len(), 1);
    let case = &cases[0];
    assert_eq!(case.name, "contract");
    assert_eq!(case.members, [session_store::identity(&path)]);
    assert_eq!(case.identities[0].alias, "PERSON_1");
    assert_eq!(case.identities[0].originals, ["Anna Petrova"]);
    draw(cx);
    assert!(cx.debug_bounds("case-header").is_some());
    cx.update(|_, cx| {
        let session = tabs.read(cx).snapshot(cx);
        assert_eq!(session.tabs[0].case.as_deref(), Some(case.id.as_str()));
        assert!(!tabs.read(cx).can_create_case(tab, cx));
    });

    // Renaming an alias in the tab reaches the case, keeping the old alias.
    let view = active(&tabs, cx);
    view.update(cx, |view, cx| {
        let anna = view.pii.review().applied()[0].step.identity;
        view.pii
            .review_mut()
            .rename_identity(anna, "BUYER")
            .unwrap();
        view.review_changed(cx);
    });
    cx.executor()
        .advance_clock(std::time::Duration::from_secs(1));
    cx.run_until_parked();
    let saved = store.load(&case.id).unwrap();
    assert_eq!(saved.identities[0].alias, "BUYER");
    assert_eq!(saved.identities[0].previous, ["PERSON_1"]);
}

#[gpui::test]
fn saving_an_unsaved_document_as_a_case_saves_the_markdown_first(cx: &mut TestAppContext) {
    let dir = tempfile::tempdir().unwrap();
    let (tabs, cx, _) = applied_document(cx, dir.path(), Session::default());
    let tab = cx.update(|_, cx| tabs.read(cx).tabs.active());
    // As a fresh conversion: Markdown never saved (#75).
    active(&tabs, cx).update(cx, |view, _| view.session.document.path = None);
    let store = || mdoc_store::cases::Store::with_key(dir.path().join("cases"), &[3; 32]);
    // A plain Save As gives no reason.
    cx.dispatch_action(SaveAs);
    cx.run_until_parked();
    assert_eq!(save_prompt::take_last_reason(), Some(None));
    cx.simulate_new_path_selection(|_| None);
    cx.run_until_parked();
    // Cancelling the save creates no case; its Save As says why it opened.
    tabs.update_in(cx, |tabs, window, cx| {
        assert!(tabs.create_case(tab, window, cx).is_none())
    });
    cx.run_until_parked();
    assert_eq!(
        save_prompt::take_last_reason(),
        Some(Some(save_prompt::CASE_REASON))
    );
    cx.simulate_new_path_selection(|_| None);
    cx.run_until_parked();
    assert!(store().list().0.is_empty());
    // Saving it lists the saved file in the new case.
    let saved = dir.path().join("converted.md");
    tabs.update_in(cx, |tabs, window, cx| tabs.create_case(tab, window, cx));
    cx.run_until_parked();
    cx.simulate_new_path_selection(|_| Some(saved.clone()));
    cx.run_until_parked();
    let (cases, _) = store().list();
    assert_eq!(cases.len(), 1);
    assert_eq!(cases[0].members, [session_store::identity(&saved)]);
    assert!(saved.exists());
}

#[gpui::test]
fn shortcuts_work_right_after_share_save_as_case(cx: &mut TestAppContext) {
    let dir = tempfile::tempdir().unwrap();
    let (tabs, cx, _) = applied_document(cx, dir.path(), Session::default());
    cx.update(|_, cx| crate::bind_app_keys(cx));
    let view = active(&tabs, cx);
    view.update_in(cx, |view, window, cx| {
        view.inspector_share = true;
        if !view.replacements_open() {
            view.toggle_replacements_panel(window, cx);
        }
    });
    draw(cx);
    let button = cx.debug_bounds("share-save-case").unwrap();
    cx.simulate_click(button.center(), Default::default());
    cx.run_until_parked();
    draw(cx);
    cx.update(|_, cx| assert!(view.read(cx).case.link().is_some()));
    // #79: Mod+Q reaches the workspace without clicking the editor first.
    cx.simulate_keystrokes(if cfg!(target_os = "macos") {
        "cmd-q"
    } else {
        "ctrl-q"
    });
    cx.run_until_parked();
    cx.update(|_, cx| {
        let tabs = tabs.read(cx);
        assert!(tabs.quit_flow.quitting() || tabs.quit_flow.finishing());
    });
}

#[gpui::test]
fn a_long_case_name_keeps_categories_page_controls_inside_the_card(cx: &mut TestAppContext) {
    let dir = tempfile::tempdir().unwrap();
    let (tabs, cx, _) = applied_document(cx, dir.path(), Session::default());
    let tab = cx.update(|_, cx| tabs.read(cx).tabs.active());
    tabs.update_in(cx, |tabs, window, cx| tabs.create_case(tab, window, cx));
    cx.run_until_parked();
    tabs.update_in(cx, |tabs, window, cx| {
        let case = &mut tabs.case_coordinator.cases_mut()[0];
        case.name = "Заявка_на_приемку_первой_партии_10000_единиц_и_ещё_немного".into();
        let id = case.id.clone();
        tabs.show_case_page(&id, super::super::case_page::Place::Categories, window, cx);
    });
    draw(cx);
    draw(cx);
    draw(cx);
    let panel = cx.debug_bounds("case-page").unwrap();
    let control = cx.debug_bounds("page-category-PERSON").unwrap();
    assert!(
        control.right() <= panel.right() && control.left() >= panel.left(),
        "{control:?} outside {panel:?}"
    );
}

#[gpui::test]
fn reopening_a_case_document_restores_its_mapping(cx: &mut TestAppContext) {
    let dir = tempfile::tempdir().unwrap();
    let store = mdoc_store::cases::Store::with_key(dir.path().join("cases"), &[3; 32]);
    let path = dir.path().join("contract.md");
    std::fs::write(&path, "PERSON_1 met ORG_1; PERSON_9 did not.").unwrap();
    let case = mdoc_store::cases::Case::new(
        "Petrova".into(),
        &[
            mdoc_pii::restore::ExportedIdentity {
                alias: "PERSON_1".into(),
                previous: vec![],
                category: mdoc_pii::Category::Person,
                custom: false,
                originals: vec!["Anna Petrova".into()],
            },
            mdoc_pii::restore::ExportedIdentity {
                alias: "ORG_1".into(),
                previous: vec![],
                category: mdoc_pii::Category::Organization,
                custom: false,
                originals: vec!["Northwind Traders Ltd".into()],
            },
        ],
        Some(session_store::identity(&path)),
    );
    store.save(&case).unwrap();
    let session = Session {
        tabs: vec![TabRecord {
            markdown: session_store::identity(&path),
            case: Some(case.id.clone()),
            ..TabRecord::default()
        }],
        ..Session::default()
    };
    let (tabs, cx) = boot(cx, Session::default());
    tabs.update_in(cx, |tabs, window, cx| {
        tabs.use_case_store(mdoc_store::cases::Store::with_key(
            dir.path().join("cases"),
            &[3; 32],
        ));
        tabs.restore(session, window, cx);
        let restored = tabs.tabs.last().unwrap().id();
        tabs.activate(restored, window, cx);
    });
    cx.run_until_parked();
    let view = active(&tabs, cx);
    cx.update(|_, cx| {
        let view = view.read(cx);
        assert_eq!(view.case.link().map(|l| l.name.as_str()), Some("Petrova"));
        let applied: Vec<_> = view
            .pii
            .review()
            .applied()
            .iter()
            .map(|a| a.step.before.to_string())
            .collect();
        assert_eq!(applied, ["Anna Petrova", "Northwind Traders Ltd"]);
        assert!(
            view.has_applied_aliases(),
            "Copy for AI and Paste AI Reply apply"
        );
    });

    // Deleting the case wipes it; the tab keeps its live mapping.
    tabs.update_in(cx, |tabs, window, cx| {
        tabs.forget_case(&case.id, window, cx)
    });
    cx.run_until_parked();
    assert!(store.load(&case.id).is_err());
    cx.update(|_, cx| {
        assert!(view.read(cx).case.link().is_none());
        assert_eq!(view.read(cx).pii.review().applied().len(), 2);
        assert!(
            tabs.read(cx)
                .snapshot(cx)
                .tabs
                .iter()
                .all(|t| t.case.is_none())
        );
    });
}

#[gpui::test]
fn closing_an_unsaved_mapping_offers_to_save_it_once(cx: &mut TestAppContext) {
    let dir = tempfile::tempdir().unwrap();
    let (tabs, cx, path) = applied_document(cx, dir.path(), Session::default());
    let tab = cx.update(|_, cx| tabs.read(cx).tabs.active());
    tabs.update_in(cx, |tabs, window, cx| tabs.close_tab(tab, window, cx));
    cx.run_until_parked();
    // Edited and pseudonymized: one question covers both (ADR 0071).
    assert_eq!(
        cx.pending_prompt().unwrap().0,
        "Save “contract.md” and keep its mapping?"
    );
    cx.simulate_prompt_answer("Save as Case");
    cx.run_until_parked();
    assert!(!cx.has_pending_prompt());
    cx.update(|_, cx| assert!(!tabs.read(cx).tabs.contains(tab)));
    assert_eq!(
        std::fs::read_to_string(path).unwrap(),
        "PERSON_1 met ORG_1."
    );
    let store = mdoc_store::cases::Store::with_key(dir.path().join("cases"), &[3; 32]);
    assert_eq!(store.list().0.len(), 1);
}

#[gpui::test]
fn the_close_prompt_can_save_only_the_document_or_neither(cx: &mut TestAppContext) {
    for (answer, written) in [("Save", true), ("Don't Save", false)] {
        let dir = tempfile::tempdir().unwrap();
        let (tabs, cx, path) = applied_document(cx, dir.path(), Session::default());
        let tab = cx.update(|_, cx| tabs.read(cx).tabs.active());
        tabs.update_in(cx, |tabs, window, cx| tabs.close_tab(tab, window, cx));
        cx.run_until_parked();
        cx.simulate_prompt_answer(answer);
        cx.run_until_parked();
        assert!(!cx.has_pending_prompt(), "{answer} asks nothing more");
        cx.update(|_, cx| assert!(!tabs.read(cx).tabs.contains(tab)));
        assert_eq!(
            std::fs::read_to_string(&path).unwrap() == "PERSON_1 met ORG_1.",
            written,
            "{answer}"
        );
        let store = mdoc_store::cases::Store::with_key(dir.path().join("cases"), &[3; 32]);
        assert!(store.list().0.is_empty(), "{answer} forgets the mapping");
    }
}

#[gpui::test]
fn cancelling_the_close_prompt_keeps_the_tab_and_asks_again(cx: &mut TestAppContext) {
    let dir = tempfile::tempdir().unwrap();
    let (tabs, cx, _) = applied_document(cx, dir.path(), Session::default());
    let tab = cx.update(|_, cx| tabs.read(cx).tabs.active());
    for _ in 0..2 {
        tabs.update_in(cx, |tabs, window, cx| tabs.close_tab(tab, window, cx));
        cx.run_until_parked();
        assert_eq!(
            cx.pending_prompt().unwrap().0,
            "Save “contract.md” and keep its mapping?"
        );
        cx.simulate_prompt_answer("Cancel");
        cx.run_until_parked();
        cx.update(|_, cx| assert!(tabs.read(cx).tabs.contains(tab)));
    }
}

#[gpui::test]
fn quitting_asks_about_an_unsaved_mapping(cx: &mut TestAppContext) {
    for (answer, cases) in [("Close", 0), ("Save as Case", 1)] {
        let dir = tempfile::tempdir().unwrap();
        let (tabs, cx, path) = applied_document(cx, dir.path(), Session::default());
        // Saved, so only the mapping is at risk.
        active(&tabs, cx).update(cx, |view, cx| view.write(path, None, cx));
        cx.run_until_parked();
        tabs.update_in(cx, |tabs, window, cx| tabs.quit(window, cx));
        cx.run_until_parked();
        assert_eq!(
            cx.pending_prompt().unwrap().0,
            "Keep the replacement mapping?"
        );
        cx.simulate_prompt_answer(answer);
        cx.run_until_parked();
        assert!(!cx.has_pending_prompt(), "{answer} asks nothing more");
        assert!(cx.cx.read(|cx| cx.windows().is_empty()), "{answer} quits");
        let store = mdoc_store::cases::Store::with_key(dir.path().join("cases"), &[3; 32]);
        assert_eq!(store.list().0.len(), cases, "{answer}");
    }
}

#[gpui::test]
fn quitting_with_the_question_turned_off_drops_the_mapping(cx: &mut TestAppContext) {
    let dir = tempfile::tempdir().unwrap();
    let (tabs, cx, path) = applied_document(cx, dir.path(), Session::default());
    active(&tabs, cx).update(cx, |view, cx| view.write(path, None, cx));
    cx.run_until_parked();
    cx.update(|_, cx| {
        let prefs = tabs.read(cx).preferences.clone();
        prefs
            .borrow_mut()
            .current
            .as_mut()
            .unwrap()
            .cases
            .ask_before_closing = false;
    });
    tabs.update_in(cx, |tabs, window, cx| tabs.quit(window, cx));
    cx.run_until_parked();
    assert!(!cx.has_pending_prompt());
    assert!(cx.cx.read(|cx| cx.windows().is_empty()));
}

#[gpui::test]
fn quitting_with_two_at_risk_tabs_summarizes_first(cx: &mut TestAppContext) {
    let dir = tempfile::tempdir().unwrap();
    let (tabs, cx, path) = applied_document(cx, dir.path(), Session::default());
    let summary = "2 documents have unsaved changes or mappings";
    tabs.update_in(cx, |tabs, window, cx| tabs.new_tab(window, cx));
    cx.simulate_input("draft");
    tabs.update_in(cx, |tabs, window, cx| tabs.quit(window, cx));
    cx.run_until_parked();
    assert_eq!(cx.pending_prompt().unwrap().0, summary);
    cx.simulate_prompt_answer("Cancel");
    cx.run_until_parked();
    assert!(!cx.has_pending_prompt());
    cx.update(|_, cx| {
        assert_eq!(tabs.read(cx).tabs.len(), 2);
        assert!(!tabs.read(cx).quit_flow.quitting());
    });
    // Review asks each tab in sidebar order: the pseudonymized one, then the
    // draft.
    tabs.update_in(cx, |tabs, window, cx| tabs.quit(window, cx));
    cx.run_until_parked();
    assert_eq!(cx.pending_prompt().unwrap().0, summary);
    cx.simulate_prompt_answer("Review…");
    cx.run_until_parked();
    assert_eq!(
        cx.pending_prompt().unwrap().0,
        "Save “contract.md” and keep its mapping?"
    );
    cx.simulate_prompt_answer("Save as Case");
    cx.run_until_parked();
    assert!(
        cx.pending_prompt()
            .unwrap()
            .0
            .starts_with("Save changes to")
    );
    cx.simulate_prompt_answer("Discard");
    cx.run_until_parked();
    assert!(cx.cx.read(|cx| cx.windows().is_empty()));
    assert_eq!(
        std::fs::read_to_string(path).unwrap(),
        "PERSON_1 met ORG_1."
    );
    let store = mdoc_store::cases::Store::with_key(dir.path().join("cases"), &[3; 32]);
    assert_eq!(store.list().0.len(), 1);
}

#[gpui::test]
fn a_quit_waits_for_an_open_close_prompt(cx: &mut TestAppContext) {
    let dir = tempfile::tempdir().unwrap();
    let (tabs, cx, _) = applied_document(cx, dir.path(), Session::default());
    let tab = cx.update(|_, cx| tabs.read(cx).tabs.active());
    tabs.update_in(cx, |tabs, window, cx| tabs.close_tab(tab, window, cx));
    cx.run_until_parked();
    // The window's close button quits; it must not replace the question.
    tabs.update_in(cx, |tabs, window, cx| tabs.quit(window, cx));
    cx.run_until_parked();
    cx.update(|_, cx| assert!(!tabs.read(cx).quit_flow.quitting()));
    assert_eq!(
        cx.pending_prompt().unwrap().0,
        "Save “contract.md” and keep its mapping?"
    );
    cx.simulate_prompt_answer("Cancel");
    cx.run_until_parked();
    assert!(!cx.has_pending_prompt());
}

#[gpui::test]
fn a_quit_saves_the_case_after_the_keychain_explainer(cx: &mut TestAppContext) {
    quit_through_the_keychain_explainer(cx, true);
}

#[gpui::test]
fn not_now_in_the_keychain_explainer_cancels_the_quit(cx: &mut TestAppContext) {
    quit_through_the_keychain_explainer(cx, false);
}

fn quit_through_the_keychain_explainer(cx: &mut TestAppContext, quits: bool) {
    let dir = tempfile::tempdir().unwrap();
    let (tabs, cx, path) = applied_document(cx, dir.path(), Session::default());
    active(&tabs, cx).update(cx, |view, cx| view.write(path, None, cx));
    cx.run_until_parked();
    let store = || mdoc_store::cases::Store::with_key(dir.path().join("cases"), &[3; 32]);
    tabs.update_in(cx, |tabs, window, cx| {
        tabs.stage_case_key(Ok(store()));
        tabs.quit(window, cx);
    });
    cx.run_until_parked();
    cx.simulate_prompt_answer("Save as Case");
    cx.run_until_parked();
    draw(cx);
    // The quit waits for the explainer instead of closing past it.
    assert!(cx.debug_bounds("keychain-explainer").is_some());
    cx.update(|_, cx| assert!(tabs.read(cx).quit_flow.quitting()));
    if quits {
        let button = cx.debug_bounds("keychain-continue").unwrap();
        cx.simulate_click(button.center(), Default::default());
    } else {
        cx.simulate_keystrokes("escape");
    }
    cx.run_until_parked();
    assert_eq!(cx.cx.read(|cx| cx.windows().is_empty()), quits);
    assert_eq!(store().list().0.len(), usize::from(quits));
    if !quits {
        cx.update(|_, cx| assert!(!tabs.read(cx).quit_flow.quitting()));
    }
}

#[gpui::test]
fn try_again_after_a_denied_quit_saves_the_case_without_resolving_a_later_quit(
    cx: &mut TestAppContext,
) {
    let dir = tempfile::tempdir().unwrap();
    let (tabs, cx, path) = applied_document(cx, dir.path(), Session::default());
    active(&tabs, cx).update(cx, |view, cx| view.write(path, None, cx));
    cx.run_until_parked();
    let store = || mdoc_store::cases::Store::with_key(dir.path().join("cases"), &[3; 32]);
    tabs.update_in(cx, |tabs, window, cx| {
        tabs.stage_case_key(Err(mdoc_store::cases::CaseError::KeyDenied));
        tabs.quit(window, cx);
    });
    cx.run_until_parked();
    cx.simulate_prompt_answer("Save as Case");
    cx.run_until_parked();
    draw(cx);
    let button = cx.debug_bounds("keychain-continue").unwrap();
    cx.simulate_click(button.center(), Default::default());
    cx.run_until_parked();
    // Denied: the quit stops; Try again still owes the case.
    cx.update(|_, cx| assert!(!tabs.read(cx).quit_flow.quitting()));
    tabs.update_in(cx, |tabs, window, cx| tabs.new_tab(window, cx));
    cx.simulate_input("draft");
    tabs.update_in(cx, |tabs, window, cx| tabs.quit(window, cx));
    cx.run_until_parked();
    cx.simulate_prompt_answer("Review…");
    cx.run_until_parked();
    cx.simulate_prompt_answer("Close");
    cx.run_until_parked();
    assert!(
        cx.pending_prompt()
            .unwrap()
            .0
            .starts_with("Save changes to")
    );
    tabs.update_in(cx, |tabs, window, cx| {
        tabs.stage_case_key(Ok(store()));
        tabs.continue_unlock(window, cx);
    });
    cx.run_until_parked();
    // The draft's question is still the one to answer.
    cx.update(|_, cx| assert!(tabs.read(cx).quit_flow.quitting()));
    assert!(
        cx.pending_prompt()
            .unwrap()
            .0
            .starts_with("Save changes to")
    );
    assert_eq!(store().list().0.len(), 1);
    cx.simulate_prompt_answer("Discard");
    cx.run_until_parked();
    assert!(cx.cx.read(|cx| cx.windows().is_empty()));
}

#[gpui::test]
fn discard_all_quits_without_asking_each_tab(cx: &mut TestAppContext) {
    let dir = tempfile::tempdir().unwrap();
    let (tabs, cx, path) = applied_document(cx, dir.path(), Session::default());
    tabs.update_in(cx, |tabs, window, cx| tabs.new_tab(window, cx));
    cx.simulate_input("draft");
    tabs.update_in(cx, |tabs, window, cx| tabs.quit(window, cx));
    cx.run_until_parked();
    cx.simulate_prompt_answer("Discard All");
    cx.run_until_parked();
    assert!(!cx.has_pending_prompt());
    assert!(cx.cx.read(|cx| cx.windows().is_empty()));
    assert_eq!(
        std::fs::read_to_string(path).unwrap(),
        "Anna Petrova met Northwind Traders Ltd."
    );
    let store = mdoc_store::cases::Store::with_key(dir.path().join("cases"), &[3; 32]);
    assert!(store.list().0.is_empty());
}

#[gpui::test]
fn closing_an_unsaved_conversion_as_a_case_saves_it_then_closes(cx: &mut TestAppContext) {
    let dir = tempfile::tempdir().unwrap();
    let (tabs, cx, _) = applied_document(cx, dir.path(), Session::default());
    let tab = cx.update(|_, cx| tabs.read(cx).tabs.active());
    active(&tabs, cx).update(cx, |view, _| view.session.document.path = None);
    tabs.update_in(cx, |tabs, window, cx| tabs.close_tab(tab, window, cx));
    cx.run_until_parked();
    cx.simulate_prompt_answer("Save as Case");
    cx.run_until_parked();
    assert_eq!(
        save_prompt::take_last_reason(),
        Some(Some(save_prompt::CASE_REASON))
    );
    let saved = dir.path().join("converted.md");
    cx.simulate_new_path_selection(|_| Some(saved.clone()));
    cx.run_until_parked();
    assert!(!cx.has_pending_prompt());
    cx.update(|_, cx| assert!(!tabs.read(cx).tabs.contains(tab)));
    let store = mdoc_store::cases::Store::with_key(dir.path().join("cases"), &[3; 32]);
    let (cases, _) = store.list();
    assert_eq!(cases.len(), 1);
    assert_eq!(cases[0].members, [session_store::identity(&saved)]);
}

#[gpui::test]
fn replies_join_the_source_case_and_save_as_moves_the_member(cx: &mut TestAppContext) {
    let dir = tempfile::tempdir().unwrap();
    let (tabs, cx, path) = applied_document(cx, dir.path(), Session::default());
    let source = cx.update(|_, cx| tabs.read(cx).tabs.active());
    tabs.update_in(cx, |tabs, window, cx| tabs.create_case(source, window, cx));
    cx.run_until_parked();
    let case = cx.update(|_, cx| tabs.read(cx).case_coordinator.cases()[0].id.clone());

    // Save As: the case now names only the new path.
    let moved = dir.path().join("moved.md");
    std::fs::copy(&path, &moved).unwrap();
    let view = active(&tabs, cx);
    view.update(cx, |view, _| {
        view.session.document.path = Some(session_store::identity(&moved))
    });
    tabs.update_in(cx, |tabs, window, cx| {
        tabs.event(source, &TabEvent::Save(SaveEvent::Saved), window, cx)
    });
    cx.run_until_parked();
    cx.update(|_, cx| {
        assert_eq!(
            tabs.read(cx).case_coordinator.cases()[0].members,
            [session_store::identity(&moved)]
        );
    });

    // A reply pasted from a case tab joins the case, right after its source.
    tabs.update_in(cx, |tabs, window, cx| tabs.new_tab(window, cx));
    tabs.update_in(cx, |tabs, window, cx| tabs.activate(source, window, cx));
    cx.write_to_clipboard(gpui::ClipboardItem::new_string("PERSON_1 agrees.".into()));
    view.update_in(cx, |view, window, cx| {
        view.paste_ai_response(&PasteAiResponse, window, cx)
    });
    cx.run_until_parked();
    let reply = active(&tabs, cx);
    cx.update(|_, cx| {
        let tabs = tabs.read(cx);
        assert_eq!(
            tabs.tabs[1].id(),
            tabs.tabs.active(),
            "the reply follows its source"
        );
        let link = reply.read(cx).case.link().cloned().unwrap();
        assert_eq!(link.id, case);
        assert_eq!(
            reply.read(cx).editor.read(cx).text(),
            "Anna Petrova agrees."
        );
    });
}

#[gpui::test]
fn joining_a_case_renumbers_proposes_and_follows_renames(cx: &mut TestAppContext) {
    use mdoc_pii::Category::{Organization, Person};
    let dir = tempfile::tempdir().unwrap();
    let (tabs, cx) = boot(cx, Session::default());
    cx.simulate_resize(size(px(1400.), px(850.)));
    tabs.update(cx, |tabs, _| {
        tabs.use_case_store(mdoc_store::cases::Store::with_key(
            dir.path().join("cases"),
            &[3; 32],
        ))
    });
    let first = open_applied(
        &tabs,
        cx,
        dir.path(),
        "a.md",
        "Anna Petrova met Northwind Ltd.",
        &[("Anna Petrova", Person), ("Northwind Ltd", Organization)],
    );
    tabs.update_in(cx, |tabs, window, cx| tabs.create_case(first, window, cx));
    cx.run_until_parked();
    let case = cx.update(|_, cx| tabs.read(cx).case_coordinator.cases()[0].id.clone());
    let second = open_applied(
        &tabs,
        cx,
        dir.path(),
        "b.md",
        "Boris Ivanov wrote to Anna Petrova.",
        &[("Boris Ivanov", Person)],
    );
    let second_view = active(&tabs, cx);
    cx.update(|_, cx| {
        assert_eq!(
            second_view.read(cx).editor.read(cx).text(),
            "PERSON_1 wrote to Anna Petrova."
        )
    });

    tabs.update_in(cx, |tabs, window, cx| {
        tabs.join_case(second, &case, window, cx)
    });
    cx.run_until_parked();
    cx.update(|_, cx| {
        let view = second_view.read(cx);
        assert_eq!(view.case.link().unwrap().id, case);
        assert_eq!(
            view.editor.read(cx).text(),
            "PERSON_2 wrote to Anna Petrova.",
            "Boris takes the next free number"
        );
        assert_eq!(view.pii.review().remaining(), 1, "Anna is proposed");
        let tabs = tabs.read(cx);
        assert!(
            tabs.notices
                .message()
                .unwrap()
                .contains("1 alias renumbered")
        );
        let case = &tabs.case_coordinator.cases()[0];
        assert!(case.identities.iter().any(|i| i.alias == "PERSON_2"));
    });
    second_view.update_in(cx, |view, window, cx| {
        view.apply_all_pii(&PiiApplyAll, window, cx)
    });
    cx.run_until_parked();
    cx.update(|_, cx| {
        assert_eq!(
            second_view.read(cx).editor.read(cx).text(),
            "PERSON_2 wrote to PERSON_1."
        )
    });

    // A rename in the first document reaches the second as its own edit.
    let first_view = cx.update(|_, cx| {
        tabs.read(cx)
            .tabs
            .get(first)
            .and_then(|t| t.view().cloned())
            .unwrap()
    });
    first_view.update(cx, |view, cx| {
        let anna = view.pii.review().identity_by_alias("PERSON_1").unwrap();
        view.retag_identities(&[(anna, "BUYER".into(), Person, true)], cx)
            .unwrap();
    });
    cx.executor()
        .advance_clock(std::time::Duration::from_secs(1));
    cx.run_until_parked();
    cx.update(|_, cx| {
        assert_eq!(
            first_view.read(cx).editor.read(cx).text(),
            "BUYER met ORG_1."
        );
        assert_eq!(
            second_view.read(cx).editor.read(cx).text(),
            "PERSON_2 wrote to BUYER."
        );
    });
    // Undo in the second document is its own step.
    cx.update(|window, cx| {
        window.focus(&second_view.read(cx).editor.read(cx).focus_handle(cx), cx)
    });
    cx.dispatch_action(mdoc_editor::Undo);
    cx.run_until_parked();
    cx.update(|_, cx| {
        assert_eq!(
            second_view.read(cx).editor.read(cx).text(),
            "PERSON_2 wrote to PERSON_1."
        )
    });

    // Folding the case is remembered in the session.
    tabs.update(cx, |tabs, _| {
        tabs.cases.collapsed.insert(case.clone());
    });
    cx.update(|_, cx| {
        assert_eq!(
            tabs.read(cx).snapshot(cx).collapsed_cases,
            std::slice::from_ref(&case)
        )
    });
}

#[gpui::test]
fn a_term_added_to_the_case_is_proposed_in_every_member(cx: &mut TestAppContext) {
    use mdoc_pii::Category::Person;
    let dir = tempfile::tempdir().unwrap();
    let (tabs, cx) = boot(cx, Session::default());
    tabs.update(cx, |tabs, _| {
        tabs.use_case_store(mdoc_store::cases::Store::with_key(
            dir.path().join("cases"),
            &[3; 32],
        ))
    });
    let first = open_applied(
        &tabs,
        cx,
        dir.path(),
        "a.md",
        "Anna Petrova hired Project Bluebird.",
        &[("Anna Petrova", Person)],
    );
    tabs.update_in(cx, |tabs, window, cx| tabs.create_case(first, window, cx));
    cx.run_until_parked();
    tabs.update(cx, |tabs, cx| {
        tabs.add_case_term(first, "Project Bluebird", cx)
    });
    cx.run_until_parked();
    let view = active(&tabs, cx);
    cx.update(|_, cx| {
        let tabs = tabs.read(cx);
        let case = &tabs.case_coordinator.cases()[0];
        assert!(
            case.identities
                .iter()
                .any(|i| i.originals == ["Project Bluebird"])
        );
        let view = view.read(cx);
        assert_eq!(view.pii.review().remaining(), 1, "the new term is proposed");
        assert_eq!(view.case.names().len(), 2);
    });
}

#[gpui::test]
fn clipboard_names_and_case_categories_follow_the_tab(cx: &mut TestAppContext) {
    let dir = tempfile::tempdir().unwrap();
    let (tabs, cx, _) = applied_document(cx, dir.path(), Session::default());
    let view = active(&tabs, cx);
    cx.write_to_clipboard(gpui::ClipboardItem::new_string(
        "Call Anna Petrova at Northwind Traders Ltd today.".into(),
    ));
    view.update_in(cx, |view, window, cx| {
        view.pseudonymize_clipboard(&PseudonymizeClipboard, window, cx)
    });
    cx.update(|_, cx| {
        assert_eq!(
            cx.read_from_clipboard().unwrap().text().as_deref(),
            Some("Call PERSON_1 at ORG_1 today.")
        );
        assert!(
            view.read(cx)
                .clipboard_notice
                .text()
                .unwrap()
                .contains("Replaced 2 names")
        );
    });

    let tab = cx.update(|_, cx| tabs.read(cx).tabs.active());
    tabs.update_in(cx, |tabs, window, cx| tabs.create_case(tab, window, cx));
    cx.run_until_parked();
    let case = cx.update(|_, cx| tabs.read(cx).case_coordinator.cases()[0].id.clone());
    tabs.update(cx, |tabs, cx| {
        tabs.toggle_case_category(&case, mdoc_pii::Category::Address, cx)
    });
    cx.run_until_parked();
    cx.update(|_, cx| {
        assert!(
            view.read(cx)
                .case
                .skip()
                .contains(&mdoc_pii::Category::Address)
        );
        assert_eq!(tabs.read(cx).case_coordinator.cases()[0].skip, ["ADDRESS"]);
    });
    // Leaving the case ends its category switches for this tab.
    tabs.update_in(cx, |tabs, window, cx| {
        tabs.remove_from_case(tab, window, cx)
    });
    cx.update(|_, cx| assert!(view.read(cx).case.skip().is_empty()));
    // Before/after shows originals beside aliases.
    view.update_in(cx, |view, window, cx| view.toggle_compare(window, cx));
    draw(cx);
    assert!(cx.debug_bounds("compare-view").is_some());
}

/// Edit › Restore Clipboard puts the tab's names back in copied text, as the
/// shelf's Restore does (ADR 0084); ⇧⌘C and ⇧⌘M copy for AI and copy the
/// Markdown (decision 10 of the UI polish grill).
#[gpui::test]
fn restore_clipboard_and_the_copy_keys_work_from_the_document(cx: &mut TestAppContext) {
    let dir = tempfile::tempdir().unwrap();
    let (tabs, cx, _) = applied_document(cx, dir.path(), Session::default());
    cx.update(|_, cx| crate::bind_app_keys(cx));
    let view = active(&tabs, cx);
    let focus = view.read_with(cx, |view, cx| view.editor.read(cx).focus_handle(cx));
    cx.update(|window, cx| window.focus(&focus, cx));
    let clipboard = |cx: &mut VisualTestContext| {
        cx.read_from_clipboard()
            .and_then(|item| item.text())
            .unwrap_or_default()
    };
    let notice = |cx: &mut VisualTestContext| {
        cx.update(|_, cx| {
            view.read(cx)
                .clipboard_notice
                .text()
                .unwrap_or_default()
                .to_string()
        })
    };

    cx.write_to_clipboard(gpui::ClipboardItem::new_string(
        "PERSON_1 thanks ORG_1 and PERSON_9.".into(),
    ));
    cx.dispatch_action(crate::workspace::RestoreClipboard);
    cx.run_until_parked();
    assert_eq!(
        clipboard(cx),
        "Anna Petrova thanks Northwind Traders Ltd and PERSON_9."
    );
    assert!(
        notice(cx).starts_with("Restored 2 names from this document"),
        "{}",
        notice(cx)
    );
    assert!(
        notice(cx).contains("1 alias unrecognized"),
        "{}",
        notice(cx)
    );

    cx.write_to_clipboard(gpui::ClipboardItem::new_string(
        "Nothing to restore.".into(),
    ));
    cx.dispatch_action(crate::workspace::RestoreClipboard);
    cx.run_until_parked();
    assert_eq!(clipboard(cx), "Nothing to restore.");
    assert!(notice(cx).starts_with("No aliases"), "{}", notice(cx));

    cx.update(|window, cx| window.focus(&focus, cx));
    cx.simulate_keystrokes("secondary-shift-c");
    cx.run_until_parked();
    let for_ai = clipboard(cx);
    assert!(
        for_ai.contains("PERSON_1") && !for_ai.contains("Anna Petrova"),
        "{for_ai}"
    );
    assert_ne!(
        for_ai,
        view.read_with(cx, |view, cx| view.editor.read(cx).text().to_string())
    );
    cx.simulate_keystrokes("secondary-shift-m");
    cx.run_until_parked();
    assert_eq!(
        clipboard(cx),
        view.read_with(cx, |view, cx| view.editor.read(cx).text().to_string())
    );
}

/// The clipboard notice is transient (issue #181): it times out, and an
/// edit, a copy or its × clears it sooner.
#[gpui::test]
fn the_clipboard_notice_times_out_or_clears_on_the_next_action(cx: &mut TestAppContext) {
    use crate::pii::ui::restore::CLIPBOARD_NOTICE_TIMEOUT;
    let dir = tempfile::tempdir().unwrap();
    let (tabs, cx, _) = applied_document(cx, dir.path(), Session::default());
    let view = active(&tabs, cx);
    let pseudonymize = |cx: &mut VisualTestContext| {
        cx.write_to_clipboard(gpui::ClipboardItem::new_string("Call Anna Petrova.".into()));
        view.update_in(cx, |view, window, cx| {
            view.pseudonymize_clipboard(&PseudonymizeClipboard, window, cx)
        });
        draw(cx);
        assert!(cx.debug_bounds("clipboard-notice").is_some());
    };
    let gone = |cx: &mut VisualTestContext| {
        draw(cx);
        assert!(cx.debug_bounds("clipboard-notice").is_none());
        assert!(cx.update(|_, cx| view.read(cx).clipboard_notice.text().is_none()));
    };

    // Shown, then gone once the timeout passes; a second run restarts it.
    pseudonymize(cx);
    cx.executor().advance_clock(CLIPBOARD_NOTICE_TIMEOUT / 2);
    pseudonymize(cx);
    cx.executor()
        .advance_clock(CLIPBOARD_NOTICE_TIMEOUT / 2 + CLIPBOARD_NOTICE_TIMEOUT / 4);
    cx.run_until_parked();
    draw(cx);
    assert!(cx.debug_bounds("clipboard-notice").is_some(), "restarted");
    cx.executor().advance_clock(CLIPBOARD_NOTICE_TIMEOUT);
    cx.run_until_parked();
    gone(cx);

    // Editing the document clears it.
    pseudonymize(cx);
    view.update_in(cx, |view, window, cx| {
        window.focus(&view.editor.read(cx).focus_handle(cx), cx)
    });
    cx.simulate_input(" ");
    cx.run_until_parked();
    gone(cx);

    // So does a copy, which replaces the clipboard the notice describes.
    pseudonymize(cx);
    view.update_in(cx, |view, window, cx| {
        view.copy_markdown(&crate::document_view::CopyMarkdown, window, cx)
    });
    gone(cx);

    // And its ×, as before.
    pseudonymize(cx);
    let dismiss = cx
        .debug_bounds("clipboard-notice-dismiss")
        .unwrap()
        .center();
    cx.simulate_click(dismiss, Default::default());
    cx.run_until_parked();
    gone(cx);
}

#[gpui::test]
fn saving_a_case_explains_the_keychain_before_asking(cx: &mut TestAppContext) {
    let dir = tempfile::tempdir().unwrap();
    let (tabs, cx, _) = applied_document(cx, dir.path(), Session::default());
    let tab = cx.update(|_, cx| tabs.read(cx).tabs.active());
    let store = || mdoc_store::cases::Store::with_key(dir.path().join("cases"), &[3; 32]);
    // Not now cancels: nothing is saved and the key is not read.
    tabs.update_in(cx, |tabs, window, cx| {
        tabs.stage_case_key(Ok(store()));
        assert!(tabs.create_case(tab, window, cx).is_none());
    });
    draw(cx);
    assert!(cx.debug_bounds("keychain-explainer").is_some());
    cx.simulate_keystrokes("escape");
    draw(cx);
    assert!(cx.debug_bounds("keychain-explainer").is_none());
    cx.update(|_, cx| {
        let tabs = tabs.read(cx);
        assert!(tabs.cases.test_open.is_some() && !tabs.cases.explainer);
    });
    assert!(store().list().0.is_empty());
    // Continue reads the key, then saves the case it was opened for.
    tabs.update_in(cx, |tabs, window, cx| {
        tabs.create_case(tab, window, cx);
    });
    draw(cx);
    let button = cx.debug_bounds("keychain-continue").unwrap();
    cx.simulate_click(button.center(), Default::default());
    cx.run_until_parked();
    draw(cx);
    assert!(cx.debug_bounds("keychain-explainer").is_none());
    assert_eq!(store().list().0.len(), 1);
    // The key is open now: later case actions do not explain again.
    cx.update(|_, cx| assert!(tabs.read(cx).case_key_banner().is_none()));
}

#[gpui::test]
fn case_members_restored_at_launch_wait_for_unlock(cx: &mut TestAppContext) {
    let dir = tempfile::tempdir().unwrap();
    let (case, path) = saved_case(dir.path());
    let session = Session {
        tabs: vec![TabRecord {
            markdown: session_store::identity(&path),
            case: Some(case.id.clone()),
            ..TabRecord::default()
        }],
        ..Session::default()
    };
    let (tabs, cx) = boot(cx, Session::default());
    tabs.update_in(cx, |tabs, window, cx| {
        // The first attempt is declined, the second allowed.
        tabs.stage_case_key(Err(mdoc_store::cases::CaseError::KeyDenied));
        tabs.restore(session, window, cx);
        let restored = tabs.tabs.last().unwrap().id();
        tabs.activate(restored, window, cx);
    });
    cx.run_until_parked();
    draw(cx);
    // Nothing asked at launch: the member opened without its mapping, kept
    // its case link, and the bar offers Unlock.
    let view = active(&tabs, cx);
    cx.update(|_, cx| {
        assert!(view.read(cx).case.link().is_none());
        let tabs = tabs.read(cx);
        assert!(tabs.case_coordinator.locked() && !tabs.cases.explainer);
        assert!(tabs.cases.test_open.is_some());
        assert_eq!(
            tabs.snapshot(cx).tabs.last().unwrap().case.as_deref(),
            Some(case.id.as_str())
        );
    });
    let unlock = cx.debug_bounds("unlock-cases").unwrap();
    cx.simulate_click(unlock.center(), Default::default());
    draw(cx);
    let button = cx.debug_bounds("keychain-continue").unwrap();
    cx.simulate_click(button.center(), Default::default());
    cx.run_until_parked();
    draw(cx);
    // Declined: still locked, and the bar offers Try again.
    cx.update(|_, cx| {
        let tabs = tabs.read(cx);
        assert!(tabs.case_coordinator.denied());
        assert_eq!(tabs.case_key_banner().map(|b| b.1), Some("Try again…"));
        assert_eq!(
            tabs.snapshot(cx).tabs.last().unwrap().case.as_deref(),
            Some(case.id.as_str())
        );
    });
    tabs.update(cx, |tabs, _| {
        tabs.cases.test_open = Some(Ok(mdoc_store::cases::Store::with_key(
            dir.path().join("cases"),
            &[3; 32],
        )));
    });
    // The declined action waits: the notices popover reopens on Try again.
    assert!(cx.debug_bounds("notices-popover").is_some());
    let unlock = cx.debug_bounds("unlock-cases").unwrap();
    cx.simulate_click(unlock.center(), Default::default());
    draw(cx);
    let button = cx.debug_bounds("keychain-continue").unwrap();
    cx.simulate_click(button.center(), Default::default());
    cx.run_until_parked();
    draw(cx);
    cx.update(|_, cx| {
        assert_eq!(
            view.read(cx).case.link().map(|l| l.name.as_str()),
            Some("Petrova")
        );
        assert!(tabs.read(cx).case_key_banner().is_none());
    });
    assert!(cx.debug_bounds("unlock-cases").is_none());
}

#[gpui::test]
fn try_again_after_a_declined_keychain_dialog_finishes_the_action(cx: &mut TestAppContext) {
    let dir = tempfile::tempdir().unwrap();
    let (tabs, cx, _) = applied_document(cx, dir.path(), Session::default());
    let tab = cx.update(|_, cx| tabs.read(cx).tabs.active());
    let store = || mdoc_store::cases::Store::with_key(dir.path().join("cases"), &[3; 32]);
    tabs.update_in(cx, |tabs, window, cx| {
        tabs.stage_case_key(Err(mdoc_store::cases::CaseError::KeyDenied));
        tabs.create_case(tab, window, cx);
    });
    draw(cx);
    let button = cx.debug_bounds("keychain-continue").unwrap();
    cx.simulate_click(button.center(), Default::default());
    cx.run_until_parked();
    draw(cx);
    assert!(store().list().0.is_empty());
    // Try again → Continue saves the case the user asked for.
    tabs.update(cx, |tabs, _| tabs.cases.test_open = Some(Ok(store())));
    let retry = cx.debug_bounds("unlock-cases").unwrap();
    cx.simulate_click(retry.center(), Default::default());
    draw(cx);
    let button = cx.debug_bounds("keychain-continue").unwrap();
    cx.simulate_click(button.center(), Default::default());
    cx.run_until_parked();
    assert_eq!(store().list().0.len(), 1);
    cx.update(|_, cx| assert!(tabs.read(cx).case_key_banner().is_none()));
}

#[gpui::test]
fn a_second_case_action_waits_with_the_first_for_the_keychain(cx: &mut TestAppContext) {
    let dir = tempfile::tempdir().unwrap();
    let (tabs, cx, _) = applied_document(cx, dir.path(), Session::default());
    let tab = cx.update(|_, cx| tabs.read(cx).tabs.active());
    let store = || mdoc_store::cases::Store::with_key(dir.path().join("cases"), &[3; 32]);
    tabs.update_in(cx, |tabs, window, cx| {
        tabs.stage_case_key(Ok(store()));
        tabs.create_case(tab, window, cx);
        // Open Case… while the save waits, and the save requested again.
        tabs.show_case_list(window, cx);
        tabs.create_case(tab, window, cx);
    });
    draw(cx);
    let button = cx.debug_bounds("keychain-continue").unwrap();
    cx.simulate_click(button.center(), Default::default());
    cx.run_until_parked();
    // Both ran, in order, and the repeated save made no second case.
    assert_eq!(store().list().0.len(), 1);
    cx.update(|_, cx| assert!(tabs.read(cx).cases.list_open));
}

#[gpui::test]
fn the_keychain_explainer_is_modal_and_returns_keyboard_focus(cx: &mut TestAppContext) {
    let dir = tempfile::tempdir().unwrap();
    let (tabs, cx, _) = applied_document(cx, dir.path(), Session::default());
    let tab = cx.update(|_, cx| tabs.read(cx).tabs.active());
    let view = active(&tabs, cx);
    let before = view.update_in(cx, |view, window, cx| {
        let focus = view.editor.read(cx).focus_handle(cx);
        window.focus(&focus, cx);
        focus
    });
    tabs.update_in(cx, |tabs, window, cx| {
        tabs.stage_case_key(Ok(mdoc_store::cases::Store::with_key(
            dir.path().join("cases"),
            &[3; 32],
        )));
        tabs.create_case(tab, window, cx);
    });
    draw(cx);
    // Focus starts on Continue; Tab and Shift-Tab alternate between both
    // buttons, never sticking on Not now (dry-run H18 / A4).
    let continue_focused = |cx: &mut VisualTestContext| {
        cx.update(|window, cx| tabs.read(cx).explainer_focus.is_focused(window))
    };
    assert!(continue_focused(cx), "Continue starts focused");
    for (keys, on_continue) in [
        ("tab", false),
        ("tab", true),
        ("tab", false),
        ("shift-tab", true),
        ("shift-tab", false),
        ("shift-tab", true),
    ] {
        key(cx, keys);
        assert_eq!(continue_focused(cx), on_continue, "{keys}");
    }
    // Tab and Shift-Tab stay on the dialog's buttons.
    for keys in [
        "tab",
        "tab",
        "tab",
        "shift-tab",
        "shift-tab",
        "shift-tab",
        "f6",
        "shift-f6",
    ] {
        cx.simulate_keystrokes(keys);
        draw(cx);
        tabs.update_in(cx, |tabs, window, cx| {
            assert!(tabs.explainer_group.contains_focused(window, cx), "{keys}");
        });
    }
    // Window shortcuts and clicks stop at the dialog: no tab opens or closes.
    let open = cx.update(|_, cx| tabs.read(cx).tabs.len());
    let new_tab = cx
        .debug_bounds("sidebar-new")
        .expect("the sidebar's New tab button");
    cx.simulate_click(new_tab.center(), Default::default());
    cx.dispatch_action(Close);
    cx.dispatch_action(New);
    draw(cx);
    cx.update(|_, cx| {
        let tabs = tabs.read(cx);
        assert_eq!(tabs.tabs.len(), open);
        assert!(tabs.cases.explainer);
    });
    // Esc closes it and hands focus back to the editor.
    cx.simulate_keystrokes("escape");
    draw(cx);
    tabs.update_in(cx, |tabs, window, _| {
        assert!(!tabs.cases.explainer);
        assert!(before.is_focused(window));
    });
}

#[gpui::test]
fn locked_members_load_when_the_key_opens_without_a_dialog_later(cx: &mut TestAppContext) {
    let dir = tempfile::tempdir().unwrap();
    let (case, path) = saved_case(dir.path());
    let session = Session {
        tabs: vec![TabRecord {
            markdown: session_store::identity(&path),
            case: Some(case.id.clone()),
            ..TabRecord::default()
        }],
        ..Session::default()
    };
    let (tabs, cx) = boot(cx, Session::default());
    tabs.update_in(cx, |tabs, window, cx| {
        tabs.stage_case_key(Ok(mdoc_store::cases::Store::with_key(
            dir.path().join("cases"),
            &[3; 32],
        )));
        tabs.restore(session, window, cx);
        let restored = tabs.tabs.last().unwrap().id();
        tabs.activate(restored, window, cx);
    });
    cx.run_until_parked();
    let view = active(&tabs, cx);
    cx.update(|_, cx| {
        assert!(tabs.read(cx).case_coordinator.locked() && view.read(cx).case.link().is_none())
    });
    // The key became readable without a dialog (e.g. allowed elsewhere):
    // Explicit Unlock opens it quietly and the locked member gets its mapping.
    tabs.update_in(cx, |tabs, window, cx| {
        tabs.cases.test_may_ask = false;
        tabs.unlock_cases(window, cx);
    });
    cx.run_until_parked();
    cx.update(|_, cx| {
        let tabs = tabs.read(cx);
        assert!(!tabs.case_coordinator.locked());
        assert_eq!(
            view.read(cx).case.link().map(|l| l.name.as_str()),
            Some("Petrova")
        );
    });
}

#[gpui::test]
fn a_member_opened_by_hand_offers_unlock_and_gets_its_mapping(cx: &mut TestAppContext) {
    let dir = tempfile::tempdir().unwrap();
    let (_, path) = saved_case(dir.path());
    let (tabs, cx) = boot(cx, Session::default());
    tabs.update_in(cx, |tabs, window, cx| {
        tabs.stage_case_key(Ok(mdoc_store::cases::Store::with_key(
            dir.path().join("cases"),
            &[3; 32],
        )));
        // Opened from disk: no session record names its case.
        tabs.open_paths(vec![path.clone()], window, cx);
    });
    cx.run_until_parked();
    draw(cx);
    let view = active(&tabs, cx);
    cx.update(|_, cx| assert!(view.read(cx).case.link().is_none()));
    // Nothing marks it as a member yet, so the notice waits behind the bell
    // (ADR 0043); the bell still offers Unlock.
    assert!(cx.debug_bounds("notices-popover").is_none());
    click_toolbar(cx, "header-notices");
    let unlock = cx
        .debug_bounds("unlock-cases")
        .expect("locked cases offer Unlock even without a tagged tab");
    cx.simulate_click(unlock.center(), Default::default());
    draw(cx);
    let button = cx.debug_bounds("keychain-continue").unwrap();
    cx.simulate_click(button.center(), Default::default());
    cx.run_until_parked();
    cx.update(|_, cx| {
        assert_eq!(
            view.read(cx).case.link().map(|l| l.name.as_str()),
            Some("Petrova")
        );
    });
}

#[gpui::test]
fn save_as_case_on_close_retries_a_failed_case_save(cx: &mut TestAppContext) {
    let dir = tempfile::tempdir().unwrap();
    let (tabs, cx, case) = case_quit_fixture(cx, dir.path());
    tabs.update(cx, |tabs, cx| {
        tabs.toggle_case_category(&case, mdoc_pii::Category::Person, cx)
    });
    cx.run_until_parked();
    tabs.read_with(cx, |tabs, _| assert!(tabs.case_coordinator.failed(&case)));
    // The store is repaired; Save as Case retries the failed save, then closes (#189).
    std::fs::remove_file(dir.path().join("blocked-store")).unwrap();
    let tab = cx.update(|_, cx| tabs.read(cx).tabs.active());
    tabs.update_in(cx, |tabs, window, cx| tabs.close_tab(tab, window, cx));
    cx.run_until_parked();
    cx.simulate_prompt_answer("Save as Case");
    cx.run_until_parked();
    assert!(!cx.has_pending_prompt());
    cx.update(|_, cx| assert!(!tabs.read(cx).tabs.contains(tab)));
    tabs.read_with(cx, |tabs, _| assert!(!tabs.case_coordinator.failed(&case)));
    let saved = mdoc_store::cases::Store::with_key(dir.path().join("blocked-store"), &[3; 32])
        .load(&case)
        .unwrap();
    assert!(!saved.skip.is_empty());
}

#[gpui::test]
fn case_quit_failure_can_cancel_without_losing_mappings(cx: &mut TestAppContext) {
    let dir = tempfile::tempdir().unwrap();
    let (tabs, cx, case) = case_quit_fixture(cx, dir.path());
    tabs.update(cx, |tabs, cx| {
        tabs.toggle_case_category(&case, mdoc_pii::Category::Person, cx)
    });
    tabs.update_in(cx, |tabs, window, cx| tabs.quit(window, cx));
    cx.run_until_parked();
    assert!(cx.has_pending_prompt());
    cx.simulate_prompt_answer("Cancel");
    cx.run_until_parked();
    tabs.read_with(cx, |tabs, cx| {
        assert!(!tabs.quit_flow.finishing() && !tabs.quit_flow.quitting());
        assert_eq!(tabs.case_coordinator.cases()[0].id, case);
        assert_eq!(
            tabs.active_view()
                .unwrap()
                .read(cx)
                .pii
                .review()
                .applied()
                .len(),
            2
        );
    });
    assert_eq!(
        std::fs::read_to_string(dir.path().join("blocked-store")).unwrap(),
        "keep this file"
    );
    // Cancelling retains the pending write; the next quit persists it.
    std::fs::remove_file(dir.path().join("blocked-store")).unwrap();
    tabs.update_in(cx, |tabs, window, cx| tabs.quit(window, cx));
    cx.run_until_parked();
    assert!(cx.cx.read(|cx| cx.windows().is_empty()));
    let store = mdoc_store::cases::Store::with_key(dir.path().join("blocked-store"), &[3; 32]);
    assert!(!store.load(&case).unwrap().skip.is_empty());
}

#[gpui::test]
fn case_quit_retry_saves_the_latest_pending_revision(cx: &mut TestAppContext) {
    let dir = tempfile::tempdir().unwrap();
    let (tabs, cx, case) = case_quit_fixture(cx, dir.path());
    tabs.update(cx, |tabs, cx| {
        tabs.toggle_case_category(&case, mdoc_pii::Category::Person, cx)
    });
    tabs.update_in(cx, |tabs, window, cx| tabs.quit(window, cx));
    cx.run_until_parked();
    assert!(cx.has_pending_prompt());
    // A newer mapping policy queued while recovery is open must win.
    tabs.update(cx, |tabs, cx| {
        tabs.toggle_case_category(&case, mdoc_pii::Category::Organization, cx)
    });
    std::fs::remove_file(dir.path().join("blocked-store")).unwrap();
    cx.simulate_prompt_answer("Retry");
    cx.run_until_parked();
    assert!(cx.cx.read(|cx| cx.windows().is_empty()));
    let saved = mdoc_store::cases::Store::with_key(dir.path().join("blocked-store"), &[3; 32])
        .load(&case)
        .unwrap();
    assert_eq!(saved.skip.len(), 2);
}

#[gpui::test]
fn case_quit_can_explicitly_discard_a_failed_final_save(cx: &mut TestAppContext) {
    let dir = tempfile::tempdir().unwrap();
    let (tabs, cx, case) = case_quit_fixture(cx, dir.path());
    tabs.update(cx, |tabs, cx| {
        tabs.toggle_case_category(&case, mdoc_pii::Category::Person, cx)
    });
    tabs.update_in(cx, |tabs, window, cx| tabs.quit(window, cx));
    cx.run_until_parked();
    assert!(cx.has_pending_prompt());
    cx.simulate_prompt_answer("Quit without saving mappings");
    cx.run_until_parked();
    assert!(cx.cx.read(|cx| cx.windows().is_empty()));
    assert_eq!(
        std::fs::read_to_string(dir.path().join("blocked-store")).unwrap(),
        "keep this file"
    );
}

#[gpui::test]
fn case_quit_store_open_failure_retains_work_for_retry(cx: &mut TestAppContext) {
    let dir = tempfile::tempdir().unwrap();
    let (tabs, cx, case) = case_quit_fixture(cx, dir.path());
    tabs.update(cx, |tabs, cx| {
        tabs.stage_case_key(Err(mdoc_store::cases::CaseError::NoKeyStore));
        tabs.toggle_case_category(&case, mdoc_pii::Category::Person, cx);
    });
    tabs.update_in(cx, |tabs, window, cx| tabs.quit(window, cx));
    cx.run_until_parked();
    assert!(cx.has_pending_prompt());
    tabs.update(cx, |tabs, _| {
        tabs.use_case_store(mdoc_store::cases::Store::with_key(
            dir.path().join("retry-store"),
            &[3; 32],
        ))
    });
    cx.simulate_prompt_answer("Retry");
    cx.run_until_parked();
    assert!(cx.cx.read(|cx| cx.windows().is_empty()));
    let saved = mdoc_store::cases::Store::with_key(dir.path().join("retry-store"), &[3; 32])
        .load(&case)
        .unwrap();
    assert_eq!(saved.skip.len(), 1);
}

#[gpui::test]
fn abandoned_quit_does_not_block_later_case_saves(cx: &mut TestAppContext) {
    let dir = tempfile::tempdir().unwrap();
    let (tabs, cx, case) = case_quit_fixture(cx, dir.path());
    let blocked = dir.path().join("blocked-store");
    std::fs::remove_file(&blocked).unwrap();
    let parent_file = dir.path().join("not-a-directory");
    std::fs::write(&parent_file, "preserve").unwrap();
    // Waiting on final case saves when the tab-list write fails; Cancel
    // abandons the quit.
    tabs.update_in(cx, |tabs, window, cx| {
        tabs.session_writer
            .set_path(Some(parent_file.join("session.json")));
        tabs.quit_flow.settle([]);
        tabs.set_case_quit_window(Some(window.window_handle()));
        tabs.checkpoint(window, cx);
    });
    cx.run_until_parked();
    cx.simulate_prompt_answer("Cancel");
    cx.run_until_parked();
    tabs.read_with(cx, |tabs, _| {
        assert!(!tabs.quit_flow.finishing());
        assert!(!tabs.case_quit_pending());
    });
    // An ordinary failure afterwards is a notice, not a quit prompt.
    std::fs::write(&blocked, "keep this file").unwrap();
    tabs.update(cx, |tabs, cx| {
        tabs.toggle_case_category(&case, mdoc_pii::Category::Person, cx)
    });
    cx.run_until_parked();
    assert!(!cx.has_pending_prompt());
    tabs.read_with(cx, |tabs, _| {
        assert!(
            tabs.notices
                .message()
                .is_some_and(|n| n.contains("Could not save the case"))
        );
        assert!(!tabs.case_quit_pending());
    });
    // The queue still drains once the store is writable again.
    std::fs::remove_file(&blocked).unwrap();
    tabs.update(cx, |tabs, cx| {
        tabs.toggle_case_category(&case, mdoc_pii::Category::Organization, cx)
    });
    cx.run_until_parked();
    let saved = mdoc_store::cases::Store::with_key(blocked, &[3; 32])
        .load(&case)
        .unwrap();
    assert_eq!(saved.skip.len(), 2);
}

#[gpui::test]
fn failed_write_of_a_removed_case_keeps_the_queue_moving(cx: &mut TestAppContext) {
    let dir = tempfile::tempdir().unwrap();
    let (tabs, cx, removed) = case_quit_fixture(cx, dir.path());
    let other = mdoc_store::cases::Case::new("Other".into(), &[], None);
    let good = dir.path().join("good-store");
    tabs.update_in(cx, |tabs, window, cx| {
        tabs.case_coordinator.cases_mut().push(other.clone());
        // The removed case's write is in flight against the blocked store;
        // the other case queues behind it and targets a writable one.
        tabs.toggle_case_category(&removed, mdoc_pii::Category::Person, cx);
        tabs.use_case_store(mdoc_store::cases::Store::with_key(good.clone(), &[3; 32]));
        tabs.toggle_case_category(&other.id, mdoc_pii::Category::Person, cx);
        tabs.forget_case(&removed, window, cx);
    });
    cx.run_until_parked();
    let saved = mdoc_store::cases::Store::with_key(good, &[3; 32])
        .load(&other.id)
        .unwrap();
    assert_eq!(saved.skip.len(), 1);
}

#[gpui::test]
fn case_quit_checks_edits_made_while_final_saves_are_pending(cx: &mut TestAppContext) {
    let dir = tempfile::tempdir().unwrap();
    let (tabs, cx, case) = case_quit_fixture(cx, dir.path());
    tabs.update(cx, |tabs, cx| {
        tabs.toggle_case_category(&case, mdoc_pii::Category::Person, cx)
    });
    tabs.update_in(cx, |tabs, window, cx| tabs.quit(window, cx));
    cx.run_until_parked();
    let view = active(&tabs, cx);
    view.update(cx, |view, cx| {
        view.editor.update(cx, |editor, cx| {
            let end = editor.text().len();
            editor.replace_range(end..end, " New edit.", cx);
        });
    });
    cx.run_until_parked();
    std::fs::remove_file(dir.path().join("blocked-store")).unwrap();
    cx.simulate_prompt_answer("Retry");
    cx.run_until_parked();
    assert!(
        cx.has_pending_prompt(),
        "the new Markdown edit needs its own save decision"
    );
    cx.simulate_prompt_answer("Save");
    cx.run_until_parked();
    assert!(cx.cx.read(|cx| cx.windows().is_empty()));
    assert!(
        std::fs::read_to_string(dir.path().join("contract.md"))
            .unwrap()
            .ends_with(" New edit.")
    );
}

#[gpui::test]
fn merge_hint_dismissal_survives_case_reopen(cx: &mut TestAppContext) {
    let dir = tempfile::tempdir().unwrap();
    let store = || mdoc_store::cases::Store::with_key(dir.path().join("cases"), &[3; 32]);
    let path = session_store::identity(&dir.path().join("contract.md"));
    std::fs::write(&path, "Maya Bennett signed. Ms Bennett agreed.").unwrap();
    let case = mdoc_store::cases::Case::new(
        "Bennett".into(),
        &[
            mdoc_pii::restore::ExportedIdentity {
                alias: "PERSON_1".into(),
                previous: vec![],
                category: mdoc_pii::Category::Person,
                custom: false,
                originals: vec!["Maya Bennett".into()],
            },
            mdoc_pii::restore::ExportedIdentity {
                alias: "PERSON_2".into(),
                previous: vec![],
                category: mdoc_pii::Category::Person,
                custom: false,
                originals: vec!["Ms Bennett".into()],
            },
        ],
        Some(path.clone()),
    );
    store().save(&case).unwrap();
    let session = Session {
        tabs: vec![TabRecord {
            markdown: path,
            case: Some(case.id.clone()),
            ..TabRecord::default()
        }],
        ..Session::default()
    };
    let (tabs, cx) = boot(cx, Session::default());
    tabs.update_in(cx, |tabs, window, cx| {
        tabs.use_case_store(store());
        tabs.restore(session.clone(), window, cx);
        tabs.activate(tabs.tabs.last().unwrap().id(), window, cx);
    });
    cx.run_until_parked();
    let view = active(&tabs, cx);
    view.update_in(cx, |view, window, cx| {
        if !view.replacements_open() {
            view.toggle_replacements_panel(window, cx);
        }
    });
    draw(cx);
    let from = cx.update(|_, cx| view.read(cx).pii.review().merge_hints()[0].from);
    let selector: &'static str = Box::leak(format!("merge-hint-dismiss-{from}").into_boxed_str());
    let button = cx.debug_bounds(selector).expect("live case hint").center();
    cx.simulate_click(button, Default::default());
    cx.run_until_parked();
    draw(cx);
    assert!(cx.debug_bounds(selector).is_none());
    assert_eq!(store().load(&case.id).unwrap().dismissed_hints.len(), 1);
    // Reload the encrypted case through a fresh coordinator, not shared tab state.
    let restored = cx.update(|window, cx| {
        window.replace_root(cx, |window, cx| {
            let mut tabs = Workspace::empty(window, cx);
            tabs.use_case_store(store());
            tabs.restore(session, window, cx);
            tabs.activate(tabs.tabs.last().unwrap().id(), window, cx);
            tabs
        })
    });
    cx.run_until_parked();
    let reopened = active(&restored, cx);
    reopened.update_in(cx, |view, window, cx| {
        if !view.replacements_open() {
            view.toggle_replacements_panel(window, cx);
        }
    });
    draw(cx);
    cx.update(|_, cx| {
        assert_ne!(view.entity_id(), reopened.entity_id());
        assert_eq!(reopened.read(cx).pii.review().merge_hints().len(), 1);
    });
    let from = cx.update(|_, cx| reopened.read(cx).pii.review().merge_hints()[0].from);
    assert!(
        cx.debug_bounds(Box::leak(
            format!("merge-hint-dismiss-{from}").into_boxed_str()
        ))
        .is_none()
    );

    // Paste AI Response creates a different case member through start_restored.
    restored.update_in(cx, |tabs, window, cx| {
        let reply = crate::pii::ui::RestoredReply {
            text: "PERSON_1 met PERSON_2".into(),
            identities: case.exported(),
            source_name: "contract".into(),
            case: reopened.read(cx).case.link().cloned(),
        };
        tabs.event(
            tabs.tabs.active(),
            &TabEvent::Tabs(TabsEvent::Restored(reply.into())),
            window,
            cx,
        );
    });
    cx.run_until_parked();
    let reply = active(&restored, cx);
    reply.update_in(cx, |view, window, cx| {
        if !view.replacements_open() {
            view.toggle_replacements_panel(window, cx);
        }
    });
    draw(cx);
    let from = cx.update(|_, cx| {
        assert_ne!(reply.entity_id(), reopened.entity_id());
        assert!(reply.read(cx).case.is(&case.id));
        reply.read(cx).pii.review().merge_hints()[0].from
    });
    assert!(
        cx.debug_bounds(Box::leak(
            format!("merge-hint-dismiss-{from}").into_boxed_str()
        ))
        .is_none()
    );
}

#[gpui::test]
fn merge_hint_dismissed_before_save_as_case_stays_dismissed(cx: &mut TestAppContext) {
    use mdoc_pii::Category;
    let dir = tempfile::tempdir().unwrap();
    let store = || mdoc_store::cases::Store::with_key(dir.path().join("cases"), &[3; 32]);
    let path = dir.path().join("contract.md");
    let source = "Maya Bennett signed. Ms Bennett agreed.";
    std::fs::write(&path, source).unwrap();
    let (tabs, cx) = boot(cx, Session::default());
    tabs.update_in(cx, |tabs, window, cx| {
        tabs.use_case_store(store());
        tabs.open_paths(vec![path.clone()], window, cx)
    });
    cx.run_until_parked();
    let view = active(&tabs, cx);
    view.update_in(cx, |view, window, cx| {
        view.pii.set_reviewing(true);
        for name in ["Maya Bennett", "Ms Bennett"] {
            let start = source.find(name).unwrap();
            view.pii
                .review_mut()
                .add_manual(source, start..start + name.len(), Category::Person)
                .unwrap();
        }
        if !view.replacements_open() {
            view.toggle_replacements_panel(window, cx);
        }
        cx.notify();
    });
    draw(cx);
    let from = cx.update(|_, cx| view.read(cx).pii.review().merge_hints()[0].from);
    let selector: &'static str = Box::leak(format!("merge-hint-dismiss-{from}").into_boxed_str());
    let button = cx
        .debug_bounds(selector)
        .expect("hint before the case")
        .center();
    cx.simulate_click(button, Default::default());
    cx.run_until_parked();
    let tab = cx.update(|_, cx| tabs.read(cx).tabs.active());
    let id = tabs.update_in(cx, |tabs, window, cx| {
        tabs.create_case(tab, window, cx).unwrap()
    });
    cx.run_until_parked();
    assert_eq!(
        store().load(&id).unwrap().dismissed_hints,
        [("Maya Bennett".to_string(), "Ms Bennett".to_string())]
    );
    // Reopen the case's member from the encrypted file.
    let session = Session {
        tabs: vec![TabRecord {
            markdown: session_store::identity(&path),
            case: Some(id.clone()),
            ..TabRecord::default()
        }],
        ..Session::default()
    };
    let restored = cx.update(|window, cx| {
        window.replace_root(cx, |window, cx| {
            let mut tabs = Workspace::empty(window, cx);
            tabs.use_case_store(store());
            tabs.restore(session, window, cx);
            tabs.activate(tabs.tabs.last().unwrap().id(), window, cx);
            tabs
        })
    });
    cx.run_until_parked();
    let reopened = active(&restored, cx);
    reopened.update_in(cx, |view, window, cx| {
        if !view.replacements_open() {
            view.toggle_replacements_panel(window, cx);
        }
    });
    draw(cx);
    let from = cx.update(|_, cx| {
        assert_ne!(view.entity_id(), reopened.entity_id());
        assert!(reopened.read(cx).case.is(&id));
        let hints = reopened.read(cx).pii.review().merge_hints();
        assert_eq!(hints.len(), 1, "the pair still matches");
        hints[0].from
    });
    assert!(
        cx.debug_bounds(Box::leak(
            format!("merge-hint-dismiss-{from}").into_boxed_str()
        ))
        .is_none()
    );
}

#[gpui::test]
fn forgotten_merge_hint_names_cannot_be_saved_as_a_new_case(cx: &mut TestAppContext) {
    use mdoc_pii::Category::Person;
    let dir = tempfile::tempdir().unwrap();
    let store = || mdoc_store::cases::Store::with_key(dir.path().join("cases"), &[3; 32]);
    let (tabs, cx) = boot(cx, Session::default());
    cx.simulate_resize(size(px(1400.), px(850.)));
    tabs.update(cx, |tabs, _| tabs.use_case_store(store()));
    let source = "Maya Bennett signed. Ms Bennett agreed.";
    let names = &[("Maya Bennett", Person), ("Ms Bennett", Person)];
    let first = open_applied(&tabs, cx, dir.path(), "first.md", source, names);
    let case = tabs.update_in(cx, |tabs, window, cx| {
        tabs.create_case(first, window, cx).unwrap()
    });
    let second = open_applied(&tabs, cx, dir.path(), "second.md", source, names);
    tabs.update_in(cx, |tabs, window, cx| {
        tabs.join_case(second, &case, window, cx)
    });
    cx.run_until_parked();
    let view = active(&tabs, cx);
    view.update_in(cx, |view, window, cx| {
        view.inspector_share = false;
        if !view.replacements_open() {
            view.toggle_replacements_panel(window, cx);
        }
    });
    draw(cx);
    let from = cx.update(|_, cx| view.read(cx).pii.review().merge_hints()[0].from);
    let selector: &'static str = Box::leak(format!("merge-hint-dismiss-{from}").into_boxed_str());
    let button = cx.debug_bounds(selector).unwrap().center();
    cx.simulate_click(button, Default::default());
    cx.run_until_parked();
    assert_eq!(store().load(&case).unwrap().dismissed_hints.len(), 1);
    tabs.update_in(cx, |tabs, window, cx| {
        tabs.open_case(&case, window, cx);
        tabs.confirm_forget_case_identity(&case, "PERSON_1", window, cx);
    });
    cx.simulate_prompt_answer("Forget Identity");
    cx.run_until_parked();
    // Both the dismissing tab and the member that received the dismissal
    // must be safe to detach and save, using the real encrypted store.
    for tab in [second, first] {
        let new_case = tabs.update_in(cx, |tabs, window, cx| {
            tabs.activate(tab, window, cx);
            tabs.remove_from_case(tab, window, cx);
            tabs.create_case(tab, window, cx).unwrap()
        });
        cx.run_until_parked();
        assert_ne!(case, new_case);
        let decrypted = serde_json::to_string(&store().load(&new_case).unwrap()).unwrap();
        assert!(!decrypted.contains("Maya Bennett"), "{decrypted}");
        assert!(decrypted.contains("Ms Bennett"), "the other identity stays");
    }
}

#[gpui::test]
fn merge_hint_does_not_rename_an_absorbed_identity_in_another_member(cx: &mut TestAppContext) {
    use mdoc_pii::Category::{Organization, Person};
    let dir = tempfile::tempdir().unwrap();
    let store = || mdoc_store::cases::Store::with_key(dir.path().join("cases"), &[3; 32]);
    let (tabs, cx) = boot(cx, Session::default());
    cx.simulate_resize(size(px(1400.), px(850.)));
    tabs.update(cx, |tabs, _| tabs.use_case_store(store()));
    let source = "Maya Bennett signed. Ms Bennett agreed. Northwind Ltd witnessed.";
    let names = &[
        ("Maya Bennett", Person),
        ("Ms Bennett", Person),
        ("Northwind Ltd", Organization),
    ];
    let first = open_applied(&tabs, cx, dir.path(), "first.md", source, names);
    let case = tabs.update_in(cx, |tabs, window, cx| {
        tabs.create_case(first, window, cx).unwrap()
    });
    let first_view = active(&tabs, cx);
    let second = open_applied(&tabs, cx, dir.path(), "second.md", source, names);
    let second_view = active(&tabs, cx);
    tabs.update_in(cx, |tabs, window, cx| {
        tabs.join_case(second, &case, window, cx);
        tabs.activate(first, window, cx);
    });
    cx.run_until_parked();
    assert_eq!(
        store()
            .load(&case)
            .unwrap()
            .identities
            .iter()
            .map(|i| i.alias.as_str())
            .collect::<Vec<_>>(),
        ["PERSON_1", "PERSON_2", "ORG_1"]
    );
    first_view.update_in(cx, |view, window, cx| {
        view.inspector_share = false;
        if !view.replacements_open() {
            view.toggle_replacements_panel(window, cx);
        }
    });
    draw(cx);
    let from = cx.update(|_, cx| first_view.read(cx).pii.review().merge_hints()[0].from);
    let selector: &'static str = Box::leak(format!("merge-hint-merge-{from}").into_boxed_str());
    let button = cx.debug_bounds(selector).unwrap().center();
    cx.simulate_click(button, Default::default());
    cx.run_until_parked();
    tabs.update(cx, |tabs, cx| tabs.case_changed(first, cx));
    cx.run_until_parked();
    cx.update(|_, cx| {
        let other = second_view.read(cx);
        assert_eq!(other.pii.error(), None);
        assert_eq!(
            other.editor.read(cx).text(),
            "PERSON_1 signed. PERSON_2 agreed. ORG_1 witnessed."
        );
        assert!(
            other.pii.review().identity_by_alias("PERSON_2").is_some(),
            "merges are not propagated"
        );
    });
    assert_eq!(store().load(&case).unwrap().identities.len(), 2);
    // The unrelated trailing identity must still follow an ordinary rename.
    second_view.update(cx, |view, cx| {
        let org = view.pii.review().identity_by_alias("ORG_1").unwrap();
        view.retag_identities(&[(org, "COMPANY".into(), Organization, true)], cx)
            .unwrap();
    });
    tabs.update(cx, |tabs, cx| tabs.case_changed(second, cx));
    cx.run_until_parked();
    cx.update(|_, cx| {
        let other = second_view.read(cx);
        assert_eq!(other.pii.error(), None);
        assert_eq!(
            other.editor.read(cx).text(),
            "PERSON_1 signed. PERSON_2 agreed. COMPANY witnessed."
        );
        let merged = first_view.read(cx);
        assert_eq!(merged.pii.error(), None);
        assert_eq!(
            merged.editor.read(cx).text(),
            "PERSON_1 signed. PERSON_1 agreed. COMPANY witnessed."
        );
    });
    let saved = store().load(&case).unwrap();
    assert_eq!(saved.identities[0].alias, "PERSON_1");
    assert_eq!(saved.identities[0].originals[0], "Maya Bennett");
    // Undo the followed organization rename, then the merge. Neither split
    // may become an alias change in the other member (ADR 0041).
    for _ in 0..2 {
        first_view.update_in(cx, |view, window, cx| {
            view.editor
                .update(cx, |editor, cx| editor.undo_step(window, cx));
        });
        cx.run_until_parked();
        tabs.update(cx, |tabs, cx| tabs.case_changed(first, cx));
        cx.run_until_parked();
    }
    cx.update(|_, cx| {
        for view in [&first_view, &second_view] {
            let view = view.read(cx);
            assert_eq!(view.pii.error(), None);
            assert_eq!(
                view.editor.read(cx).text(),
                "PERSON_1 signed. PERSON_2 agreed. ORG_1 witnessed."
            );
        }
    });
}

#[gpui::test]
fn merge_hint_into_a_later_case_identity_keeps_one_case_entry(cx: &mut TestAppContext) {
    let dir = tempfile::tempdir().unwrap();
    let store = || mdoc_store::cases::Store::with_key(dir.path().join("cases"), &[3; 32]);
    let path = session_store::identity(&dir.path().join("contract.md"));
    std::fs::write(&path, "Ms Bennett agreed. Maya Bennett signed.").unwrap();
    let person = |alias: &str, name: &str| mdoc_pii::restore::ExportedIdentity {
        alias: alias.into(),
        previous: vec![],
        category: mdoc_pii::Category::Person,
        custom: false,
        originals: vec![name.into()],
    };
    let case = mdoc_store::cases::Case::new(
        "Bennett".into(),
        &[
            person("PERSON_1", "Ms Bennett"),
            person("PERSON_2", "Maya Bennett"),
        ],
        Some(path.clone()),
    );
    store().save(&case).unwrap();
    let session = Session {
        tabs: vec![TabRecord {
            markdown: path,
            case: Some(case.id.clone()),
            ..TabRecord::default()
        }],
        ..Session::default()
    };
    let (tabs, cx) = boot(cx, Session::default());
    tabs.update_in(cx, |tabs, window, cx| {
        tabs.use_case_store(store());
        tabs.restore(session.clone(), window, cx);
        tabs.activate(tabs.tabs.last().unwrap().id(), window, cx);
    });
    cx.run_until_parked();
    let view = active(&tabs, cx);
    view.update_in(cx, |view, window, cx| {
        if !view.replacements_open() {
            view.toggle_replacements_panel(window, cx);
        }
    });
    draw(cx);
    let hint = cx.update(|_, cx| view.read(cx).pii.review().merge_hints().remove(0));
    assert_eq!(hint.wording.as_ref(), "Maya Bennett");
    let selector: &'static str =
        Box::leak(format!("merge-hint-merge-{}", hint.from).into_boxed_str());
    let button = cx.debug_bounds(selector).expect("case hint").center();
    cx.simulate_click(button, Default::default());
    cx.run_until_parked();
    cx.executor()
        .advance_clock(std::time::Duration::from_secs(1));
    cx.run_until_parked();
    let saved = store().load(&case.id).unwrap();
    let people: Vec<_> = saved
        .identities
        .iter()
        .filter(|i| i.category == "PERSON")
        .collect();
    assert_eq!(people.len(), 1, "{:?}", saved.identities);
    assert_eq!(people[0].alias, "PERSON_2");
    assert_eq!(people[0].previous, ["PERSON_1"]);
    assert_eq!(people[0].originals[0], "Maya Bennett");
    // Reopened, both aliases restore the full name and no alias is reused.
    let restored = cx.update(|window, cx| {
        window.replace_root(cx, |window, cx| {
            let mut tabs = Workspace::empty(window, cx);
            tabs.use_case_store(store());
            tabs.restore(session, window, cx);
            tabs.activate(tabs.tabs.last().unwrap().id(), window, cx);
            tabs
        })
    });
    cx.run_until_parked();
    let reopened = active(&restored, cx);
    reopened.update(cx, |view, _| {
        let exported = view.pii.review().export_identities();
        assert_eq!(exported.len(), 1);
        assert_eq!(exported[0].alias, "PERSON_2");
        assert_eq!(exported[0].wording(), "Maya Bennett");
        let restore = mdoc_pii::restore::RestoreMap::new(exported).restore("PERSON_1 and PERSON_2");
        assert_eq!(restore.restored, 2);
        assert!(restore.edits.iter().all(|(_, s)| s == "Maya Bennett"));
        let next = view
            .pii
            .review_mut()
            .new_identity(mdoc_pii::Category::Person);
        assert_eq!(view.pii.review().identity(next).unwrap().alias, "PERSON_3");
    });
}
