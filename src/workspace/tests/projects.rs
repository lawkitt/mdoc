//! Projects across tabs: a move into a new or existing Project, out to
//! Unfiled and back with Undo, members following each other, deleting a
//! Project, and what the Vault and the session keep (`Projects`).
use super::*;

/// The document a tab shows, as the Vault stores it now.
fn stored(tabs: &Entity<Workspace>, cx: &mut VisualTestContext, tab: u64) -> mdoc_vault::Document {
    let id = cx.update(|_, cx| tabs.read(cx).tab_document(tab, cx).unwrap());
    vault(tabs, cx).document(id.as_str()).unwrap()
}

fn text_of(view: &Entity<DocumentView>, cx: &mut VisualTestContext) -> String {
    cx.update(|_, cx| view.read(cx).editor.read(cx).text().to_string())
}

fn view_of(tabs: &Entity<Workspace>, cx: &mut VisualTestContext, tab: u64) -> Entity<DocumentView> {
    cx.update(|_, cx| tabs.read(cx).tabs.get(tab).unwrap().view().unwrap().clone())
}

/// A tab's mapping changes reach its Project once the fold timer fires.
fn settle(cx: &mut VisualTestContext) {
    cx.executor()
        .advance_clock(std::time::Duration::from_secs(1));
    cx.run_until_parked();
}

#[gpui::test]
fn an_unfiled_documents_names_are_kept_with_it_across_a_restart(cx: &mut TestAppContext) {
    let dir = tempfile::tempdir().unwrap();
    let (tabs, cx, _) = applied_document(cx, dir.path(), Session::default());
    let tab = cx.update(|_, cx| tabs.read(cx).tabs.active());
    tabs.update_in(cx, |tabs, window, cx| tabs.autosave(tab, window, cx));
    cx.run_until_parked();
    let document = stored(&tabs, cx, tab);
    assert_eq!(document.project, None);
    assert_eq!(document.markdown, "PERSON_1 met ORG_1.");
    let names: Vec<_> = (document.mapping.exported().into_iter())
        .map(|identity| (identity.alias, identity.originals[0].to_string()))
        .collect();
    assert_eq!(
        names,
        [
            ("PERSON_1".to_string(), "Anna Petrova".to_string()),
            ("ORG_1".to_string(), "Northwind Traders Ltd".to_string())
        ]
    );
    // The session names it by id only.
    let session = cx.update(|_, cx| tabs.read(cx).snapshot(cx));
    let json = serde_json::to_string(&session).unwrap();
    for leak in ["Anna Petrova", "Northwind Traders", "contract"] {
        assert!(!json.contains(leak), "{leak} reached the session");
    }
}

#[gpui::test]
fn move_to_new_project_names_it_after_the_document_and_keeps_its_names(cx: &mut TestAppContext) {
    let dir = tempfile::tempdir().unwrap();
    let (tabs, cx, _) = applied_document(cx, dir.path(), Session::default());
    let tab = cx.update(|_, cx| tabs.read(cx).tabs.active());
    cx.update(|_, cx| assert!(tabs.read(cx).can_move(tab, cx)));
    let id = new_project(&tabs, cx, tab);
    let saved = vault(&tabs, cx).project(&id).unwrap();
    assert_eq!(saved.name, "contract");
    let identity = &saved.mapping.identities[0];
    assert_eq!(identity.alias, "PERSON_1");
    assert_eq!(identity.originals, ["Anna Petrova"]);
    assert_eq!(stored(&tabs, cx, tab).project.as_deref(), Some(id.as_str()));
    assert!(
        stored(&tabs, cx, tab).mapping.is_empty(),
        "the Project holds its names"
    );
    let view = active(&tabs, cx);
    assert_eq!(text_of(&view, cx), "PERSON_1 met ORG_1.");
    cx.update(|_, cx| {
        let view = view.read(cx);
        assert_eq!(
            view.pii.review().applied().len(),
            2,
            "its aliases stay applied"
        );
        let toast = &tabs.read(cx).project_ui.toast.as_ref().unwrap().text;
        assert_eq!(toast, "“contract” moved to “contract”.");
    });
    draw(cx);
    let row: &'static str = Box::leak(format!("tree-project-{id}").into_boxed_str());
    assert!(cx.debug_bounds(row).is_some());
    assert!(cx.debug_bounds("move-toast-undo").is_some());
    // The session names no Project.
    let json = cx.update(|_, cx| serde_json::to_string(&tabs.read(cx).snapshot(cx)).unwrap());
    assert!(!json.contains("contract"), "{json}");

    // Renaming an alias in the tab reaches the Project, keeping the old one.
    view.update(cx, |view, cx| {
        let anna = view.pii.review().applied()[0].step.identity;
        view.pii
            .review_mut()
            .rename_identity(anna, "BUYER")
            .unwrap();
        view.review_changed(cx);
    });
    settle(cx);
    let saved = vault(&tabs, cx).project(&id).unwrap();
    assert_eq!(saved.mapping.identities[0].alias, "BUYER");
    assert_eq!(saved.mapping.identities[0].previous, ["PERSON_1"]);
}

#[gpui::test]
fn shortcuts_work_right_after_share_move_to_new_project(cx: &mut TestAppContext) {
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
    let button = cx.debug_bounds("share-new-project").unwrap();
    cx.simulate_click(button.center(), Default::default());
    cx.run_until_parked();
    draw(cx);
    let view = active(&tabs, cx);
    cx.update(|_, cx| assert!(view.read(cx).project.link().is_some()));
    // #79: Mod+Q reaches the workspace without clicking the editor first.
    cx.simulate_keystrokes(if cfg!(target_os = "macos") {
        "cmd-q"
    } else {
        "ctrl-q"
    });
    cx.run_until_parked();
    assert!(cx.cx.read(|cx| cx.windows().is_empty()));
}

#[gpui::test]
fn a_long_project_name_keeps_categories_page_controls_inside_the_card(cx: &mut TestAppContext) {
    let dir = tempfile::tempdir().unwrap();
    let (tabs, cx, _) = applied_document(cx, dir.path(), Session::default());
    let tab = cx.update(|_, cx| tabs.read(cx).tabs.active());
    let id = new_project(&tabs, cx, tab);
    tabs.update_in(cx, |tabs, window, cx| {
        tabs.projects.edit(&id, |project| {
            project.name = "Заявка_на_приемку_первой_партии_10000_единиц_и_ещё_немного".into();
            Some(())
        });
        tabs.show_project_page(
            &id,
            super::super::project_page::Place::Categories,
            window,
            cx,
        );
    });
    draw(cx);
    draw(cx);
    draw(cx);
    let panel = cx.debug_bounds("project-page").unwrap();
    let control = cx.debug_bounds("page-category-PERSON").unwrap();
    assert!(
        control.right() <= panel.right() && control.left() >= panel.left(),
        "{control:?} outside {panel:?}"
    );
}

#[gpui::test]
fn reopening_a_project_document_restores_its_mapping(cx: &mut TestAppContext) {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path().join("vault");
    let (project, document) = saved_project(&mdoc_vault::Vault::open(&root, &[3; 32]).unwrap());
    let (tabs, cx) = boot_on_vault(cx, session_of(&document), &root);
    let view = active(&tabs, cx);
    cx.update(|_, cx| {
        let view = view.read(cx);
        assert_eq!(
            view.project.link().map(|l| l.name.as_str()),
            Some("Petrova")
        );
        let applied: Vec<_> = (view.pii.review().applied().iter())
            .map(|a| a.step.before.to_string())
            .collect();
        assert_eq!(applied, ["Anna Petrova"]);
        assert!(
            view.has_applied_aliases(),
            "Copy for AI and Paste AI Reply apply"
        );
    });
    // Its review saves with it, and the next open restores that instead.
    let tab = cx.update(|_, cx| tabs.read(cx).tabs.active());
    view.update(cx, |view, cx| view.review_changed(cx));
    tabs.update_in(cx, |tabs, window, cx| tabs.autosave(tab, window, cx));
    cx.run_until_parked();
    assert!(stored(&tabs, cx, tab).review.is_some());
    assert_eq!(
        stored(&tabs, cx, tab).project.as_deref(),
        Some(project.as_str())
    );
}

#[gpui::test]
fn a_reply_pasted_in_a_member_is_the_projects(cx: &mut TestAppContext) {
    let dir = tempfile::tempdir().unwrap();
    let (tabs, cx, _) = applied_document(cx, dir.path(), Session::default());
    let source = cx.update(|_, cx| tabs.read(cx).tabs.active());
    let project = new_project(&tabs, cx, source);
    let view = active(&tabs, cx);

    // A reply pasted from a Project's document is saved as the Project's
    // and restores with its names; no tab opens for it.
    let count = cx.update(|_, cx| tabs.read(cx).tabs.len());
    cx.write_to_clipboard(gpui::ClipboardItem::new_string("PERSON_1 agrees.".into()));
    view.update_in(cx, |view, window, cx| {
        view.paste_ai_response(&PasteAiResponse, window, cx)
    });
    cx.run_until_parked();
    let replies = vault(&tabs, cx).project_replies(&project);
    assert_eq!(replies.len(), 1);
    assert_eq!(
        replies[0].owner,
        mdoc_vault::ReplyOwner::Project(project.clone())
    );
    cx.update(|_, cx| {
        let tabs = tabs.read(cx);
        assert_eq!(tabs.tabs.len(), count);
        let page = tabs.reply_page.as_ref().unwrap();
        assert_eq!(page.restored().text, "Anna Petrova agrees.");
    });
}

#[gpui::test]
fn a_move_renumbers_proposes_follows_renames_and_undoes(cx: &mut TestAppContext) {
    use mdoc_pii::Category::{Organization, Person};
    let dir = tempfile::tempdir().unwrap();
    let (tabs, cx) = boot(cx, Session::default());
    cx.simulate_resize(size(px(1400.), px(850.)));
    let first = open_applied(
        &tabs,
        cx,
        dir.path(),
        "a.md",
        "Anna Petrova met Northwind Ltd.",
        &[("Anna Petrova", Person), ("Northwind Ltd", Organization)],
    );
    let project = new_project(&tabs, cx, first);
    let second = open_applied(
        &tabs,
        cx,
        dir.path(),
        "b.md",
        "Boris Ivanov wrote to Anna Petrova.",
        &[("Boris Ivanov", Person)],
    );
    let second_view = active(&tabs, cx);
    assert_eq!(text_of(&second_view, cx), "PERSON_1 wrote to Anna Petrova.");

    move_tab(&tabs, cx, second, Some(&project));
    let second_view = view_of(&tabs, cx, second);
    assert_eq!(
        text_of(&second_view, cx),
        "PERSON_2 wrote to Anna Petrova.",
        "Boris takes the next free number"
    );
    assert_eq!(
        stored(&tabs, cx, second).markdown,
        "PERSON_2 wrote to Anna Petrova.",
        "the Vault holds the rewritten text"
    );
    cx.update(|_, cx| {
        let view = second_view.read(cx);
        assert_eq!(view.project.link().unwrap().id, project);
        assert_eq!(view.pii.review().remaining(), 1, "Anna is proposed");
        let tabs = tabs.read(cx);
        let toast = &tabs.project_ui.toast.as_ref().unwrap().text;
        assert_eq!(toast, "“b” moved to “a”. 1 alias renumbered to fit.");
        let project = tabs.projects.project(&project).unwrap();
        assert!(
            project
                .mapping
                .identities
                .iter()
                .any(|i| i.alias == "PERSON_2")
        );
    });
    second_view.update_in(cx, |view, window, cx| {
        view.apply_all_pii(&PiiApplyAll, window, cx)
    });
    cx.run_until_parked();
    assert_eq!(text_of(&second_view, cx), "PERSON_2 wrote to PERSON_1.");

    // A rename in the first document reaches the second as its own edit.
    let first_view = view_of(&tabs, cx, first);
    first_view.update(cx, |view, cx| {
        let anna = view.pii.review().identity_by_alias("PERSON_1").unwrap();
        view.retag_identities(&[(anna, "BUYER".into(), Person, true)], cx)
            .unwrap();
    });
    settle(cx);
    assert_eq!(text_of(&first_view, cx), "BUYER met ORG_1.");
    assert_eq!(text_of(&second_view, cx), "PERSON_2 wrote to BUYER.");
    cx.update(|window, cx| {
        window.focus(&second_view.read(cx).editor.read(cx).focus_handle(cx), cx)
    });
    cx.dispatch_action(mdoc_editor::Undo);
    cx.run_until_parked();
    assert_eq!(text_of(&second_view, cx), "PERSON_2 wrote to PERSON_1.");

    // Expanding the Project is remembered in the session.
    tabs.update(cx, |tabs, _| {
        tabs.tree.set_expanded(&project, true);
    });
    cx.update(|_, cx| {
        assert_eq!(
            tabs.read(cx).snapshot(cx).expanded_projects,
            std::slice::from_ref(&project)
        )
    });
}

#[gpui::test]
fn undo_moves_a_document_back_with_its_own_names_across_a_restart(cx: &mut TestAppContext) {
    use mdoc_pii::Category::Person;
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path().join("vault");
    let (tabs, cx) = boot_on_vault(cx, Session::default(), &root);
    let first = open_applied(
        &tabs,
        cx,
        dir.path(),
        "a.md",
        "Anna Petrova signed.",
        &[("Anna Petrova", Person)],
    );
    let project = new_project(&tabs, cx, first);
    let second = open_applied(
        &tabs,
        cx,
        dir.path(),
        "b.md",
        "Boris Ivanov signed.",
        &[("Boris Ivanov", Person)],
    );
    move_tab(&tabs, cx, second, Some(&project));
    assert_eq!(stored(&tabs, cx, second).markdown, "PERSON_2 signed.");
    // Undo from the toast: back in Unfiled, with its own alias again.
    draw(cx);
    let undo = cx.debug_bounds("move-toast-undo").unwrap().center();
    cx.simulate_click(undo, Default::default());
    cx.run_until_parked();
    let document = stored(&tabs, cx, second);
    assert_eq!(document.project, None);
    assert_eq!(document.markdown, "PERSON_1 signed.");
    assert_eq!(
        document.mapping.exported()[0].originals[0].as_ref(),
        "Boris Ivanov"
    );
    let view = view_of(&tabs, cx, second);
    assert_eq!(text_of(&view, cx), "PERSON_1 signed.");
    cx.update(|_, cx| {
        assert!(view.read(cx).project.link().is_none());
        assert_eq!(view.read(cx).pii.review().applied().len(), 1);
        let toast = &tabs.read(cx).project_ui.toast.as_ref().unwrap();
        assert_eq!(toast.text, "“b” moved back to Unfiled.");
        let project = tabs.read(cx).projects.project(&project).unwrap();
        assert!(
            !project
                .mapping
                .identities
                .iter()
                .any(|i| i.alias == "PERSON_2"),
            "the name it brought leaves with it"
        );
    });
    draw(cx);
    assert!(
        cx.debug_bounds("move-toast-undo").is_none(),
        "an undo is not undone"
    );

    // A restart restores it Unfiled, its names applied.
    let session = cx.update(|_, cx| tabs.read(cx).snapshot(cx));
    tabs.update_in(cx, |tabs, window, cx| tabs.quit(window, cx));
    cx.run_until_parked();
    let (tabs, cx) = boot_on_vault(&mut cx.cx, session, &root);
    let restored = cx.update(|_, cx| {
        (tabs.read(cx).tabs.iter())
            .find(|tab| {
                tab.record()
                    .document
                    .as_ref()
                    .is_some_and(|id| id.as_str() == document.id)
            })
            .map(|tab| tab.id())
            .unwrap()
    });
    tabs.update_in(cx, |tabs, window, cx| tabs.activate(restored, window, cx));
    cx.run_until_parked();
    let view = active(&tabs, cx);
    assert_eq!(text_of(&view, cx), "PERSON_1 signed.");
    cx.update(|_, cx| {
        assert!(view.read(cx).project.link().is_none());
        assert_eq!(view.read(cx).pii.review().applied().len(), 1);
    });
}

#[gpui::test]
fn the_move_toast_times_out(cx: &mut TestAppContext) {
    let dir = tempfile::tempdir().unwrap();
    let (tabs, cx, _) = applied_document(cx, dir.path(), Session::default());
    let tab = cx.update(|_, cx| tabs.read(cx).tabs.active());
    new_project(&tabs, cx, tab);
    draw(cx);
    assert!(cx.debug_bounds("move-toast").is_some());
    cx.executor()
        .advance_clock(std::time::Duration::from_secs(11));
    cx.run_until_parked();
    draw(cx);
    assert!(cx.debug_bounds("move-toast").is_none());
}

#[gpui::test]
fn move_to_unfiled_keeps_the_names_its_text_uses(cx: &mut TestAppContext) {
    use mdoc_pii::Category::{Organization, Person};
    let dir = tempfile::tempdir().unwrap();
    let (tabs, cx) = boot(cx, Session::default());
    let first = open_applied(
        &tabs,
        cx,
        dir.path(),
        "a.md",
        "Anna Petrova met Northwind Ltd.",
        &[("Anna Petrova", Person), ("Northwind Ltd", Organization)],
    );
    let project = new_project(&tabs, cx, first);
    move_tab(&tabs, cx, first, None);
    let document = stored(&tabs, cx, first);
    assert_eq!(document.project, None);
    assert_eq!(document.markdown, "PERSON_1 met ORG_1.");
    assert_eq!(document.mapping.exported().len(), 2);
    let view = view_of(&tabs, cx, first);
    cx.update(|_, cx| {
        assert!(view.read(cx).project.link().is_none());
        assert_eq!(view.read(cx).pii.review().applied().len(), 2);
        let toast = &tabs.read(cx).project_ui.toast.as_ref().unwrap().text;
        assert_eq!(toast, "“a” moved to Unfiled.");
    });
    // The Project stays, with the names it had.
    assert_eq!(
        vault(&tabs, cx)
            .project(&project)
            .unwrap()
            .mapping
            .exported()
            .len(),
        2
    );
    assert!(vault(&tabs, cx).documents_in(Some(&project)).is_empty());
}

#[gpui::test]
fn a_project_write_waiting_behind_a_move_keeps_the_moves_names(cx: &mut TestAppContext) {
    use mdoc_pii::Category::Person;
    let dir = tempfile::tempdir().unwrap();
    let (tabs, cx) = boot(cx, Session::default());
    let first = open_applied(
        &tabs,
        cx,
        dir.path(),
        "a.md",
        "Anna Petrova signed.",
        &[("Anna Petrova", Person)],
    );
    let project = new_project(&tabs, cx, first);
    let second = open_applied(
        &tabs,
        cx,
        dir.path(),
        "b.md",
        "Boris Ivanov signed.",
        &[("Boris Ivanov", Person)],
    );
    // A Project edit and a move queued together: the edit, written after
    // the move, keeps the name the move brought.
    tabs.update_in(cx, |tabs, window, cx| {
        tabs.toggle_project_category(&project, mdoc_pii::Category::Address, cx);
        tabs.move_tab(second, Some(project.clone()), window, cx);
        tabs.add_project_name(&project, "Clara Diaz", cx);
    });
    cx.run_until_parked();
    let saved = vault(&tabs, cx).project(&project).unwrap();
    let mut originals: Vec<_> = (saved.mapping.identities.iter())
        .map(|i| i.originals[0].clone())
        .collect();
    originals.sort();
    assert_eq!(originals, ["Anna Petrova", "Boris Ivanov", "Clara Diaz"]);
    assert_eq!(saved.skip, ["ADDRESS"]);
}

#[gpui::test]
fn deleting_a_project_moves_its_documents_to_unfiled_or_deletes_them(cx: &mut TestAppContext) {
    use mdoc_pii::Category::Person;
    let dir = tempfile::tempdir().unwrap();
    let (tabs, cx) = boot(cx, Session::default());
    let first = open_applied(
        &tabs,
        cx,
        dir.path(),
        "a.md",
        "Anna Petrova signed.",
        &[("Anna Petrova", Person)],
    );
    let project = new_project(&tabs, cx, first);
    let vault = vault(&tabs, cx);
    let reply = vault
        .save_reply(mdoc_vault::NewReply {
            owner: mdoc_vault::ReplyOwner::Project(project.clone()),
            title: "Reply".into(),
            source_app: None,
            markdown: "PERSON_1 agreed.".into(),
        })
        .unwrap();
    // A non-empty Project asks: Move to Unfiled.
    tabs.update_in(cx, |tabs, window, cx| {
        tabs.delete_project(project.clone(), window, cx)
    });
    cx.run_until_parked();
    cx.simulate_prompt_answer("Move to Unfiled");
    cx.run_until_parked();
    assert!(vault.project(&project).is_err());
    assert!(
        vault.reply(&reply.id).is_err(),
        "its own replies go with it"
    );
    let document = stored(&tabs, cx, first);
    assert_eq!(document.project, None);
    assert_eq!(document.markdown, "PERSON_1 signed.");
    let view = view_of(&tabs, cx, first);
    cx.update(|_, cx| {
        assert!(view.read(cx).project.link().is_none());
        assert_eq!(view.read(cx).pii.review().applied().len(), 1);
        assert!(tabs.read(cx).projects.project(&project).is_none());
    });

    // Delete Documents: the document goes, and its tab closes unwritten.
    let project = new_project(&tabs, cx, first);
    let id = stored(&tabs, cx, first).id;
    tabs.update_in(cx, |tabs, window, cx| {
        tabs.delete_project(project.clone(), window, cx)
    });
    cx.run_until_parked();
    cx.simulate_prompt_answer("Delete Documents");
    cx.run_until_parked();
    assert!(vault.project(&project).is_err());
    assert!(vault.document(&id).is_err());
    cx.update(|_, cx| assert!(!tabs.read(cx).tabs.contains(first)));

    // An empty Project asks only to confirm.
    let other = open_applied(
        &tabs,
        cx,
        dir.path(),
        "c.md",
        "Boris Ivanov signed.",
        &[("Boris Ivanov", Person)],
    );
    let project = new_project(&tabs, cx, other);
    move_tab(&tabs, cx, other, None);
    tabs.update_in(cx, |tabs, window, cx| {
        tabs.delete_project(project.clone(), window, cx)
    });
    cx.run_until_parked();
    cx.simulate_prompt_answer("Delete");
    cx.run_until_parked();
    assert!(vault.project(&project).is_err());
    assert_eq!(stored(&tabs, cx, other).project, None);
}

#[gpui::test]
fn a_term_added_to_the_project_is_proposed_in_every_member(cx: &mut TestAppContext) {
    use mdoc_pii::Category::Person;
    let dir = tempfile::tempdir().unwrap();
    let (tabs, cx) = boot(cx, Session::default());
    let first = open_applied(
        &tabs,
        cx,
        dir.path(),
        "a.md",
        "Anna Petrova hired Project Bluebird.",
        &[("Anna Petrova", Person)],
    );
    let project = new_project(&tabs, cx, first);
    tabs.update(cx, |tabs, cx| {
        tabs.add_project_term(first, "Project Bluebird", cx)
    });
    cx.run_until_parked();
    let view = active(&tabs, cx);
    cx.update(|_, cx| {
        let project = tabs.read(cx).projects.project(&project).unwrap();
        assert!((project.mapping.identities.iter()).any(|i| i.originals == ["Project Bluebird"]));
        let view = view.read(cx);
        assert_eq!(view.pii.review().remaining(), 1, "the new term is proposed");
        assert_eq!(view.project.names().len(), 2);
    });
    let saved = vault(&tabs, cx).project(&project).unwrap();
    assert_eq!(saved.mapping.identities.len(), 2);
}

#[gpui::test]
fn clipboard_names_and_project_categories_follow_the_tab(cx: &mut TestAppContext) {
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
        // Aliases only: a normal copy.
        assert!(!crate::restored_clipboard::is_restored(
            &cx.read_from_clipboard().unwrap()
        ));
        assert!((view.read(cx).clipboard_notice.text().unwrap()).contains("Replaced 2 names"));
    });

    let tab = cx.update(|_, cx| tabs.read(cx).tabs.active());
    let project = new_project(&tabs, cx, tab);
    let view = active(&tabs, cx);
    tabs.update(cx, |tabs, cx| {
        tabs.toggle_project_category(&project, mdoc_pii::Category::Address, cx)
    });
    cx.run_until_parked();
    cx.update(|_, cx| {
        assert!(
            view.read(cx)
                .project
                .skip()
                .contains(&mdoc_pii::Category::Address)
        );
        assert_eq!(
            tabs.read(cx).projects.project(&project).unwrap().skip,
            ["ADDRESS"]
        );
    });
    assert_eq!(
        vault(&tabs, cx).project(&project).unwrap().skip,
        ["ADDRESS"]
    );
    // Leaving the Project ends its category switches for this tab.
    move_tab(&tabs, cx, tab, None);
    let view = view_of(&tabs, cx, tab);
    cx.update(|_, cx| assert!(view.read(cx).project.skip().is_empty()));
    // Before/after shows originals beside aliases.
    view.update_in(cx, |view, window, cx| view.toggle_compare(window, cx));
    draw(cx);
    assert!(cx.debug_bounds("compare-view").is_some());
}

/// Edit › Restore Clipboard puts the tab's names back in copied text, as the
/// shelf's Restore does; ⇧⌘C and ⇧⌘M copy for AI and copy the
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
    let restored = |cx: &mut VisualTestContext| {
        cx.update(|_, cx| {
            crate::restored_clipboard::is_restored(&cx.read_from_clipboard().unwrap())
        })
    };
    assert!(restored(cx));
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
    assert!(!restored(cx), "Copy for AI stays a normal copy");
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

/// Open the replacements panel of `view` and draw it.
fn show_panel(view: &Entity<DocumentView>, cx: &mut VisualTestContext) {
    view.update_in(cx, |view, window, cx| {
        view.inspector_share = false;
        if !view.replacements_open() {
            view.toggle_replacements_panel(window, cx);
        }
    });
    draw(cx);
}

fn hint_button(
    view: &Entity<DocumentView>,
    cx: &mut VisualTestContext,
    kind: &str,
) -> &'static str {
    let from = cx.update(|_, cx| view.read(cx).pii.review().merge_hints()[0].from);
    Box::leak(format!("merge-hint-{kind}-{from}").into_boxed_str())
}

#[gpui::test]
fn merge_hint_dismissal_survives_project_reopen(cx: &mut TestAppContext) {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path().join("vault");
    let store = mdoc_vault::Vault::open(&root, &[3; 32]).unwrap();
    let person = |alias: &str, name: &str| mdoc_pii::restore::ExportedIdentity {
        alias: alias.into(),
        previous: vec![],
        category: mdoc_pii::Category::Person,
        custom: false,
        originals: vec![name.into()],
    };
    let project = store
        .create_project(mdoc_vault::NewProject {
            name: "Bennett".into(),
            colour: None,
            pinned: false,
        })
        .unwrap();
    store
        .update_project(&project.id, |project| {
            project.mapping.merge(&[
                person("PERSON_1", "Maya Bennett"),
                person("PERSON_2", "Ms Bennett"),
            ])
        })
        .unwrap();
    let document = store
        .create_document(mdoc_vault::NewDocument {
            project: Some(project.id.clone()),
            title: "contract".into(),
            source_kind: mdoc_vault::SourceKind::Markdown,
            origin: mdoc_vault::DocumentOrigin::Ordinary,
            markdown: "Maya Bennett signed. Ms Bennett agreed.".into(),
        })
        .unwrap();
    drop(store);
    let (tabs, cx) = boot_on_vault(cx, session_of(&document.id), &root);
    let view = active(&tabs, cx);
    show_panel(&view, cx);
    let selector = hint_button(&view, cx, "dismiss");
    let button = cx
        .debug_bounds(selector)
        .expect("live Project hint")
        .center();
    cx.simulate_click(button, Default::default());
    cx.run_until_parked();
    draw(cx);
    assert!(cx.debug_bounds(selector).is_none());
    assert_eq!(
        vault(&tabs, cx)
            .project(&project.id)
            .unwrap()
            .dismissed_hints
            .len(),
        1
    );
    let session = cx.update(|_, cx| tabs.read(cx).snapshot(cx));
    tabs.update_in(cx, |tabs, window, cx| tabs.quit(window, cx));
    cx.run_until_parked();
    let (tabs, cx) = boot_on_vault(&mut cx.cx, session, &root);
    let reopened = active(&tabs, cx);
    show_panel(&reopened, cx);
    cx.update(|_, cx| assert_eq!(reopened.read(cx).pii.review().merge_hints().len(), 1));
    assert!(
        {
            let selector = hint_button(&reopened, cx, "dismiss");
            cx.debug_bounds(selector)
        }
        .is_none()
    );
}

#[gpui::test]
fn merge_hint_dismissed_before_move_to_new_project_stays_dismissed(cx: &mut TestAppContext) {
    use mdoc_pii::Category;
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("contract.md");
    let source = "Maya Bennett signed. Maya Benett agreed.";
    std::fs::write(&path, source).unwrap();
    let (tabs, cx) = boot(cx, Session::default());
    tabs.update_in(cx, |tabs, window, cx| {
        tabs.open_paths(vec![path.clone()], window, cx)
    });
    cx.run_until_parked();
    let view = active(&tabs, cx);
    view.update_in(cx, |view, _, cx| {
        view.pii.set_reviewing(true);
        for name in ["Maya Bennett", "Maya Benett"] {
            let start = source.find(name).unwrap();
            view.pii
                .review_mut()
                .add_manual(source, start..start + name.len(), Category::Person)
                .unwrap();
        }
        cx.notify();
    });
    show_panel(&view, cx);
    let selector = hint_button(&view, cx, "dismiss");
    let button = cx
        .debug_bounds(selector)
        .expect("hint before the Project")
        .center();
    cx.simulate_click(button, Default::default());
    cx.run_until_parked();
    let tab = cx.update(|_, cx| tabs.read(cx).tabs.active());
    let id = new_project(&tabs, cx, tab);
    assert_eq!(
        vault(&tabs, cx).project(&id).unwrap().dismissed_hints,
        [("Maya Benett".to_string(), "Maya Bennett".to_string())]
    );
}

#[gpui::test]
fn merge_hint_does_not_rename_an_absorbed_identity_in_another_member(cx: &mut TestAppContext) {
    use mdoc_pii::Category::{Organization, Person};
    let dir = tempfile::tempdir().unwrap();
    let (tabs, cx) = boot(cx, Session::default());
    cx.simulate_resize(size(px(1400.), px(850.)));
    let source = "Maya Bennett signed. Maya Benett agreed. Northwind Ltd witnessed.";
    let names = &[
        ("Maya Bennett", Person),
        ("Maya Benett", Person),
        ("Northwind Ltd", Organization),
    ];
    let first = open_applied(&tabs, cx, dir.path(), "first.md", source, names);
    let project = new_project(&tabs, cx, first);
    // Its own bytes: the same file twice would ask Open Existing.
    let second = open_applied(
        &tabs,
        cx,
        dir.path(),
        "second.md",
        &format!("{source} "),
        names,
    );
    move_tab(&tabs, cx, second, Some(&project));
    let second_view = view_of(&tabs, cx, second);
    let first_view = view_of(&tabs, cx, first);
    tabs.update_in(cx, |tabs, window, cx| tabs.activate(first, window, cx));
    cx.run_until_parked();
    let aliases: Vec<_> = (project_of(&tabs, cx, &project).mapping.identities.iter())
        .map(|i| i.alias.clone())
        .collect();
    assert_eq!(aliases, ["PERSON_1", "PERSON_2", "ORG_1"]);
    show_panel(&first_view, cx);
    let selector = hint_button(&first_view, cx, "merge");
    let button = cx.debug_bounds(selector).unwrap().center();
    cx.simulate_click(button, Default::default());
    cx.run_until_parked();
    tabs.update(cx, |tabs, cx| tabs.project_changed(first, cx));
    cx.run_until_parked();
    cx.update(|_, cx| {
        let other = second_view.read(cx);
        assert_eq!(other.pii.error(), None);
        assert_eq!(
            other.editor.read(cx).text().trim_end(),
            "PERSON_1 signed. PERSON_2 agreed. ORG_1 witnessed."
        );
        assert!(
            other.pii.review().identity_by_alias("PERSON_2").is_some(),
            "merges are not propagated"
        );
    });
    // The unrelated trailing identity must still follow an ordinary rename.
    second_view.update(cx, |view, cx| {
        let org = view.pii.review().identity_by_alias("ORG_1").unwrap();
        view.retag_identities(&[(org, "COMPANY".into(), Organization, true)], cx)
            .unwrap();
    });
    tabs.update(cx, |tabs, cx| tabs.project_changed(second, cx));
    cx.run_until_parked();
    cx.update(|_, cx| {
        assert_eq!(
            second_view.read(cx).editor.read(cx).text().trim_end(),
            "PERSON_1 signed. PERSON_2 agreed. COMPANY witnessed."
        );
        assert_eq!(
            first_view.read(cx).editor.read(cx).text(),
            "PERSON_1 signed. PERSON_1 agreed. COMPANY witnessed."
        );
    });
    let saved = project_of(&tabs, cx, &project);
    assert_eq!(saved.mapping.identities[0].alias, "PERSON_1");
    assert_eq!(saved.mapping.identities[0].originals[0], "Maya Bennett");
}

/// The window's copy of Project `id`.
fn project_of(
    tabs: &Entity<Workspace>,
    cx: &mut VisualTestContext,
    id: &str,
) -> mdoc_vault::Project {
    project(tabs, cx, id)
}

#[gpui::test]
fn a_move_gives_the_projects_open_members_its_names_at_once(cx: &mut TestAppContext) {
    use mdoc_pii::Category::Person;
    let dir = tempfile::tempdir().unwrap();
    let (tabs, cx) = boot(cx, Session::default());
    let first = open_applied(
        &tabs,
        cx,
        dir.path(),
        "a.md",
        "Anna Petrova signed.",
        &[("Anna Petrova", Person)],
    );
    let project = new_project(&tabs, cx, first);
    let second = open_applied(
        &tabs,
        cx,
        dir.path(),
        "b.md",
        "Boris Ivanov signed.",
        &[("Boris Ivanov", Person)],
    );
    move_tab(&tabs, cx, second, Some(&project));
    // The open member knows Boris as PERSON_2 before any fold: it can't
    // give that alias to someone else.
    let first_view = view_of(&tabs, cx, first);
    cx.update(|_, cx| {
        let review = first_view.read(cx).pii.review();
        let boris = review.identity_by_alias("PERSON_2").unwrap();
        assert_eq!(review.identity(boris).unwrap().alias, "PERSON_2");
    });
    first_view.update(cx, |view, cx| {
        let id = view.pii.review_mut().new_identity(Person);
        assert_ne!(view.pii.review().identity(id).unwrap().alias, "PERSON_2");
        let _ = cx;
    });
}

#[gpui::test]
fn text_typed_while_a_move_waits_moves_with_it(cx: &mut TestAppContext) {
    use mdoc_pii::Category::Person;
    let dir = tempfile::tempdir().unwrap();
    let (tabs, cx) = boot(cx, Session::default());
    let tab = open_applied(
        &tabs,
        cx,
        dir.path(),
        "a.md",
        "Anna Petrova signed.",
        &[("Anna Petrova", Person)],
    );
    let view = view_of(&tabs, cx, tab);
    view.update_in(cx, |view, window, cx| {
        window.focus(&view.editor.read(cx).focus_handle(cx), cx)
    });
    // The move queues behind the tab's own write; typing goes on meanwhile.
    tabs.update_in(cx, |tabs, window, cx| {
        tabs.move_to_new_project(tab, window, cx)
    });
    cx.simulate_input(" Later.");
    cx.run_until_parked();
    let text = text_of(&view, cx);
    assert!(text.contains(" Later."), "{text}");
    assert_eq!(
        stored(&tabs, cx, tab).markdown,
        text,
        "the typed text moved"
    );
    assert!(stored(&tabs, cx, tab).project.is_some());
}

/// Review of #1145 (P1-3): Delete Project › Move to Unfiled writes each
/// open member as it shows at the head of the delete, so text typed while
/// the delete waited is what moves; nothing older reloads over it.
#[gpui::test]
fn text_typed_while_a_project_delete_waits_moves_to_unfiled(cx: &mut TestAppContext) {
    use mdoc_pii::Category::Person;
    let dir = tempfile::tempdir().unwrap();
    let (tabs, cx) = boot(cx, Session::default());
    let tab = open_applied(
        &tabs,
        cx,
        dir.path(),
        "a.md",
        "Anna Petrova signed.",
        &[("Anna Petrova", Person)],
    );
    let project = new_project(&tabs, cx, tab);
    cx.run_until_parked();
    let view = view_of(&tabs, cx, tab);
    view.update_in(cx, |view, window, cx| {
        window.focus(&view.editor.read(cx).focus_handle(cx), cx)
    });
    cx.simulate_input(" Now.");
    // The delete queues behind the member's write; typing goes on.
    tabs.update_in(cx, |tabs, window, cx| {
        let members = tabs.project_members(&project);
        let unfiled = mdoc_vault::ProjectDocuments::MoveToUnfiled;
        tabs.forget_project(&project, unfiled, members, window, cx);
    });
    view.update(cx, |view, cx| {
        view.editor.update(cx, |editor, cx| {
            let later = format!("{} Later.", editor.text());
            editor.set_text(later, cx);
        })
    });
    cx.run_until_parked();
    let vault = vault(&tabs, cx);
    assert!(vault.project(&project).is_err());
    let text = text_of(&view, cx);
    assert!(
        text.contains(" Now.") && text.ends_with(" Later."),
        "{text}"
    );
    let document = stored(&tabs, cx, tab);
    assert_eq!(document.project, None);
    assert_eq!(document.markdown, text, "the typed text moved");
}

/// Review of #1145 (P1-3): a member that can't be written stops Delete
/// Project › Move to Unfiled before anything moves; its edit stays in its
/// tab and the error shows.
#[gpui::test]
fn a_project_delete_stops_when_a_member_cannot_be_written(cx: &mut TestAppContext) {
    use mdoc_pii::Category::Person;
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("a.md");
    std::fs::write(&path, "Anna Petrova signed.").unwrap();
    // The Vault stops at the next Document write, as a full disk would.
    let fail = Arc::new(AtomicBool::new(false));
    let root = dir.path().join("vault");
    let hook: mdoc_vault::FaultHook = {
        let fail = fail.clone();
        Arc::new(move |step| {
            if fail.load(Ordering::SeqCst) && step.kind == mdoc_vault::ObjectKind::Document {
                mdoc_vault::Fault::Crash
            } else {
                mdoc_vault::Fault::Continue
            }
        })
    };
    let (tabs, cx) = boot_home_with(cx, Session::default(), move |tabs, _, _| {
        tabs.use_vault(mdoc_vault::Vault::open_with_hook(root, &[3; 32], hook).unwrap())
    });
    tabs.update_in(cx, |tabs, window, cx| tabs.open_path(path, window, cx));
    cx.run_until_parked();
    let tab = cx.update(|_, cx| tabs.read(cx).tabs.active());
    let view = view_of(&tabs, cx, tab);
    view.update_in(cx, |view, window, cx| {
        let text = view.editor.read(cx).text().to_string();
        view.pii.set_reviewing(true);
        view.pii
            .review_mut()
            .add_manual(&text, 0..12, Person)
            .unwrap();
        view.apply_all_pii(&PiiApplyAll, window, cx);
    });
    cx.run_until_parked();
    let project = new_project(&tabs, cx, tab);
    cx.run_until_parked();
    view.update_in(cx, |view, window, cx| {
        window.focus(&view.editor.read(cx).focus_handle(cx), cx)
    });
    fail.store(true, Ordering::SeqCst);
    cx.simulate_input(" My edit.");
    tabs.update_in(cx, |tabs, window, cx| {
        let members = tabs.project_members(&project);
        let unfiled = mdoc_vault::ProjectDocuments::MoveToUnfiled;
        tabs.forget_project(&project, unfiled, members, window, cx);
    });
    cx.run_until_parked();
    let text = text_of(&view, cx);
    assert!(text.contains(" My edit."), "the edit stays: {text}");
    cx.update(|_, cx| {
        let tabs = tabs.read(cx);
        assert!(tabs.projects.project(&project).is_some(), "nothing moved");
        assert!(view.read(cx).project.link().is_some());
        let notice = tabs.notices.message().unwrap();
        assert!(
            notice.starts_with("Could not delete the Project"),
            "{notice}"
        );
    });
}

/// Review of #1150 (P1): a name applied while Delete Project › Move to
/// Unfiled waits joins the Project's names before anything moves, so the
/// member takes it to Unfiled and restores it.
#[gpui::test]
fn a_name_applied_while_a_project_delete_waits_moves_with_its_member(cx: &mut TestAppContext) {
    use mdoc_pii::Category::Person;
    let dir = tempfile::tempdir().unwrap();
    let (tabs, cx) = boot(cx, Session::default());
    let text = "Anna Petrova met Boris Ivanov.";
    let tab = open_applied(
        &tabs,
        cx,
        dir.path(),
        "a.md",
        text,
        &[("Anna Petrova", Person)],
    );
    let project = new_project(&tabs, cx, tab);
    cx.run_until_parked();
    let view = view_of(&tabs, cx, tab);
    view.update(cx, |view, cx| {
        view.editor.update(cx, |editor, cx| {
            let later = format!("{} Later.", editor.text());
            editor.set_text(later, cx);
        })
    });
    tabs.update_in(cx, |tabs, window, cx| {
        let members = tabs.project_members(&project);
        let unfiled = mdoc_vault::ProjectDocuments::MoveToUnfiled;
        // A write runs first, so the delete waits behind it.
        tabs.autosave_all(window, cx);
        tabs.forget_project(&project, unfiled, members, window, cx);
    });
    view.update_in(cx, |view, window, cx| {
        let shown = view.editor.read(cx).text().to_string();
        let start = shown.find("Boris Ivanov").unwrap();
        view.pii
            .review_mut()
            .add_manual(&shown, start..start + "Boris Ivanov".len(), Person)
            .unwrap();
        view.apply_all_pii(&PiiApplyAll, window, cx);
    });
    cx.run_until_parked();
    let vault = vault(&tabs, cx);
    assert!(vault.project(&project).is_err());
    let document = stored(&tabs, cx, tab);
    assert_eq!(document.project, None);
    assert!(
        document.markdown.contains("PERSON_2"),
        "{}",
        document.markdown
    );
    let names = document.mapping.exported();
    assert!(
        names
            .iter()
            .any(|n| n.alias == "PERSON_2" && n.originals.iter().any(|o| &**o == "Boris Ivanov")),
        "{names:?}"
    );
}

/// Review of #1150 (P1): an open member with no edit of its own is not
/// written again at the head of Delete Project, so a newer revision the
/// Vault holds is what moves to Unfiled.
#[gpui::test]
fn a_project_delete_never_writes_an_unchanged_member_over_a_newer_revision(
    cx: &mut TestAppContext,
) {
    use mdoc_pii::Category::Person;
    let dir = tempfile::tempdir().unwrap();
    let (tabs, cx) = boot(cx, Session::default());
    let tab = open_applied(
        &tabs,
        cx,
        dir.path(),
        "a.md",
        "Anna Petrova signed.",
        &[("Anna Petrova", Person)],
    );
    let project = new_project(&tabs, cx, tab);
    wait(cx, 1000);
    // Another writer saves a newer revision; the tab shows the older one.
    let id = cx.update(|_, cx| tabs.read(cx).tab_document(tab, cx).unwrap());
    let vault = vault(&tabs, cx);
    vault
        .update_document(id.as_str(), |document| {
            document.markdown = "PERSON_1 signed, newer.".into()
        })
        .unwrap();
    tabs.update_in(cx, |tabs, window, cx| {
        let members = tabs.project_members(&project);
        let unfiled = mdoc_vault::ProjectDocuments::MoveToUnfiled;
        tabs.forget_project(&project, unfiled, members, window, cx);
    });
    cx.run_until_parked();
    let document = stored(&tabs, cx, tab);
    assert_eq!(document.project, None);
    assert_eq!(document.markdown, "PERSON_1 signed, newer.");
}

#[gpui::test]
fn a_delete_stops_when_a_move_changed_the_projects_documents(cx: &mut TestAppContext) {
    use mdoc_pii::Category::Person;
    let dir = tempfile::tempdir().unwrap();
    let (tabs, cx) = boot(cx, Session::default());
    let first = open_applied(
        &tabs,
        cx,
        dir.path(),
        "a.md",
        "Anna Petrova signed.",
        &[("Anna Petrova", Person)],
    );
    let project = new_project(&tabs, cx, first);
    let second = open_applied(
        &tabs,
        cx,
        dir.path(),
        "b.md",
        "Boris Ivanov signed.",
        &[("Boris Ivanov", Person)],
    );
    let id = cx.update(|_, cx| tabs.read(cx).tab_document(second, cx).unwrap());
    // Confirmed with one document; a move queued first brings a second.
    tabs.update_in(cx, |tabs, window, cx| {
        let asked = tabs.project_members(&project);
        tabs.move_tab(second, Some(project.clone()), window, cx);
        let delete = mdoc_vault::ProjectDocuments::Delete;
        tabs.forget_project(&project, delete, asked, window, cx);
    });
    cx.run_until_parked();
    // Asked again, about both.
    assert!(cx.has_pending_prompt());
    cx.simulate_prompt_answer("Cancel");
    cx.run_until_parked();
    let vault = vault(&tabs, cx);
    assert!(vault.project(&project).is_ok(), "nothing was deleted");
    assert!(vault.document(id.as_str()).is_ok());
    cx.update(|_, cx| {
        assert!(tabs.read(cx).tabs.contains(first) && tabs.read(cx).tabs.contains(second));
        let notice = tabs.read(cx).notices.message().unwrap();
        assert!(
            notice.starts_with("The Project's documents changed"),
            "{notice}"
        );
    });
}

/// Review of #1145 (P1-2): a document imported into the Project while
/// Delete Project's question is up is never deleted unasked: nothing
/// goes, and the question comes again for the documents it holds now.
#[gpui::test]
fn delete_documents_never_takes_a_document_imported_while_it_asked(cx: &mut TestAppContext) {
    use mdoc_pii::Category::Person;
    let dir = tempfile::tempdir().unwrap();
    let (tabs, cx) = boot(cx, Session::default());
    let first = open_applied(
        &tabs,
        cx,
        dir.path(),
        "a.md",
        "Anna Petrova signed.",
        &[("Anna Petrova", Person)],
    );
    let project = new_project(&tabs, cx, first);
    let vault = vault(&tabs, cx);
    tabs.update_in(cx, |tabs, window, cx| {
        tabs.delete_project(project.clone(), window, cx)
    });
    cx.run_until_parked();
    // A background import into the Project ends while the question shows.
    let late = vault
        .create_document(mdoc_vault::NewDocument {
            project: Some(project.clone()),
            title: "Late".into(),
            source_kind: mdoc_vault::SourceKind::Text,
            origin: mdoc_vault::DocumentOrigin::Ordinary,
            markdown: "Imported meanwhile.".into(),
        })
        .unwrap();
    cx.simulate_prompt_answer("Delete Documents");
    cx.run_until_parked();
    assert!(vault.project(&project).is_ok(), "nothing was deleted");
    assert!(vault.document(&late.id).is_ok(), "the late import stays");
    cx.update(|_, cx| {
        assert!(tabs.read(cx).tabs.contains(first));
        let notice = tabs.read(cx).notices.message().unwrap();
        assert!(notice.contains("Choose again"), "{notice}");
    });
    // Asked again, now about both: they go.
    assert!(cx.has_pending_prompt());
    cx.simulate_prompt_answer("Delete Documents");
    cx.run_until_parked();
    assert!(vault.project(&project).is_err());
    assert!(vault.document(&late.id).is_err());
}

#[gpui::test]
fn the_project_page_moves_a_document_whose_tab_never_loaded(cx: &mut TestAppContext) {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path().join("vault");
    let (project, document) = saved_project(&mdoc_vault::Vault::open(&root, &[3; 32]).unwrap());
    // Its tab is restored behind another and never shown.
    let mut session = session_of(&document);
    session.tabs.insert(
        0,
        TabRecord {
            blank: true,
            name: Some("Untitled.md".into()),
            ..TabRecord::default()
        },
    );
    session.active = 0;
    let (tabs, cx) = boot_on_vault(cx, session, &root);
    let lazy = cx.update(|_, cx| tabs.read(cx).tabs.last().unwrap().id());
    cx.update(|_, cx| assert!(tabs.read(cx).tabs.get(lazy).unwrap().view().is_none()));
    let vault_id = mdoc_store::document_id::VaultId::parse(&document).unwrap();
    tabs.update_in(cx, |tabs, window, cx| {
        tabs.open_project(&project, window, cx);
        tabs.remove_project_member(&project, vault_id, window, cx);
    });
    cx.run_until_parked();
    assert_eq!(vault(&tabs, cx).document(&document).unwrap().project, None);
    tabs.update_in(cx, |tabs, window, cx| tabs.activate(lazy, window, cx));
    cx.run_until_parked();
    let view = active(&tabs, cx);
    cx.update(|_, cx| {
        assert!(view.read(cx).project.link().is_none());
        assert_eq!(view.read(cx).pii.review().applied().len(), 1);
    });
}
