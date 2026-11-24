//! Saved AI Replies: saved through the Vault, reopened restored by
//! identity on the Reply page, listed on the Project page and deleted.
use super::*;
use crate::workspace::{PasteAiResponse, navigation::Stop, project_page::Segment};
use mdoc_pii::Category::Person;
use mdoc_vault::ReplyOwner;

/// Paste `reply` with Paste AI Reply in the active document; the saved
/// reply's id.
fn paste_reply(tabs: &Entity<Workspace>, cx: &mut VisualTestContext, reply: &str) -> String {
    let view = active(tabs, cx);
    cx.write_to_clipboard(gpui::ClipboardItem::new_string(reply.into()));
    view.update_in(cx, |view, window, cx| {
        view.paste_ai_response(&PasteAiResponse, window, cx)
    });
    cx.run_until_parked();
    shown_reply(tabs, cx).expect("Paste AI Reply shows the saved reply")
}

/// The id of the reply the Reply page shows.
fn shown_reply(tabs: &Entity<Workspace>, cx: &mut VisualTestContext) -> Option<String> {
    cx.update(|_, cx| Some(tabs.read(cx).reply_page.as_ref()?.id().to_string()))
}

/// The Reply page's restored text.
fn page_text(tabs: &Entity<Workspace>, cx: &mut VisualTestContext) -> String {
    cx.update(|_, cx| {
        let page = tabs.read(cx).reply_page.as_ref().expect("the Reply page");
        page.restored().text.clone()
    })
}

/// The Done-when: a reply saved to an Unfiled document follows it into a
/// Project whose names give the same people other aliases. Reopened, after
/// a restart too, it restores the same people, though the reply's own
/// tokens and the document's text now say otherwise.
#[gpui::test]
fn a_reply_reopened_after_its_document_moved_restores_the_same_people(cx: &mut TestAppContext) {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path().join("vault");
    let (tabs, cx) = boot_on_vault(cx, Session::default(), &root);
    cx.simulate_resize(size(px(1400.), px(850.)));
    // The Project knows Anna as PERSON_1 and Boris as PERSON_2.
    let member = open_applied(
        &tabs,
        cx,
        dir.path(),
        "lease.md",
        "Anna Petrova leased to Boris Ivanov.",
        &[("Anna Petrova", Person), ("Boris Ivanov", Person)],
    );
    let project = new_project(&tabs, cx, member);
    tabs.update(cx, |tabs, cx| tabs.dismiss_toast(cx));
    // The Unfiled document knows them the other way round.
    let unfiled = open_applied(
        &tabs,
        cx,
        dir.path(),
        "letter.md",
        "Boris Ivanov wrote to Anna Petrova.",
        &[("Boris Ivanov", Person), ("Anna Petrova", Person)],
    );
    let document = cx.update(|_, cx| tabs.read(cx).tab_document(unfiled, cx).unwrap());
    let reply = "PERSON_2 owes PERSON_1 the deposit.";
    let id = paste_reply(&tabs, cx, reply);
    let restored = "Anna Petrova owes Boris Ivanov the deposit.";
    assert_eq!(page_text(&tabs, cx), restored);
    let saved = vault(&tabs, cx).reply(&id).unwrap();
    assert_eq!(saved.owner, ReplyOwner::Document(document.as_str().into()));
    assert_eq!(saved.markdown, reply);

    tabs.update_in(cx, |tabs, window, cx| tabs.activate(unfiled, window, cx));
    move_tab(&tabs, cx, unfiled, Some(&project));
    cx.update(|_, cx| {
        let view = tabs
            .read(cx)
            .tabs
            .get(unfiled)
            .unwrap()
            .view()
            .unwrap()
            .clone();
        assert_eq!(
            view.read(cx).editor.read(cx).text(),
            "PERSON_2 wrote to PERSON_1.",
            "the move gave both people the Project's aliases"
        );
    });
    tabs.update_in(cx, |tabs, window, cx| tabs.open_reply(&id, window, cx));
    cx.run_until_parked();
    assert_eq!(page_text(&tabs, cx), restored, "the same people");
    assert_eq!(vault(&tabs, cx).reply(&id).unwrap().markdown, reply);
    // The Project lists it now: it followed its document.
    let listed: Vec<_> = vault(&tabs, cx).project_replies(&project);
    assert_eq!(listed.len(), 1);
    assert_eq!(listed[0].id, id);

    // After a restart, opened from the Project page.
    let session = cx.update(|_, cx| tabs.read(cx).snapshot(cx));
    tabs.update_in(cx, |tabs, window, cx| tabs.quit(window, cx));
    cx.run_until_parked();
    let (tabs, cx) = boot_on_vault(&mut cx.cx, session, &root);
    tabs.update_in(cx, |tabs, window, cx| {
        tabs.open_project(&project, window, cx);
        tabs.show_project_segment(Segment::Replies, cx);
    });
    draw(cx);
    click_toolbar(cx, "project-reply-0");
    assert_eq!(shown_reply(&tabs, cx).as_deref(), Some(id.as_str()));
    assert_eq!(page_text(&tabs, cx), restored);
}

/// A forget in the Project shows at once in a reply open on its page; the
/// saved text never changes.
#[gpui::test]
fn a_forgotten_name_leaves_an_open_reply_at_once(cx: &mut TestAppContext) {
    let dir = tempfile::tempdir().unwrap();
    let (tabs, cx, _) = applied_document(cx, dir.path(), Session::default());
    let tab = cx.update(|_, cx| tabs.read(cx).tabs.active());
    let project = new_project(&tabs, cx, tab);
    tabs.update(cx, |tabs, cx| tabs.dismiss_toast(cx));
    let id = paste_reply(&tabs, cx, "PERSON_1 thanks ORG_1.");
    assert_eq!(
        vault(&tabs, cx).reply(&id).unwrap().owner,
        ReplyOwner::Project(project.clone()),
        "a Project's document gives the Project's reply"
    );
    assert_eq!(
        page_text(&tabs, cx),
        "Anna Petrova thanks Northwind Traders Ltd."
    );
    tabs.update(cx, |tabs, cx| {
        tabs.forget_project_identity(&project, "PERSON_1", cx)
    });
    cx.run_until_parked();
    assert_eq!(
        page_text(&tabs, cx),
        "PERSON_1 thanks Northwind Traders Ltd."
    );
    cx.update(|_, cx| {
        let page = tabs.read(cx).reply_page.as_ref().unwrap();
        assert_eq!(page.summary(), "1 value restored · 1 unrecognized");
    });
    assert_eq!(
        vault(&tabs, cx).reply(&id).unwrap().markdown,
        "PERSON_1 thanks ORG_1."
    );
}

/// Delete reply asks; Cancel keeps it, Delete removes it from the Vault and
/// the page, and Back no longer stops there.
#[gpui::test]
fn delete_reply_asks_then_returns_home_and_back_skips_it(cx: &mut TestAppContext) {
    let dir = tempfile::tempdir().unwrap();
    let (tabs, cx, _) = applied_document(cx, dir.path(), Session::default());
    let id = paste_reply(&tabs, cx, "PERSON_1 replied.");
    draw(cx);
    click_toolbar(cx, "reply-page-delete");
    cx.simulate_prompt_answer("Cancel");
    cx.run_until_parked();
    assert!(vault(&tabs, cx).reply(&id).is_ok());
    assert_eq!(shown_reply(&tabs, cx).as_deref(), Some(id.as_str()));
    click_toolbar(cx, "reply-page-delete");
    cx.simulate_prompt_answer("Delete");
    cx.run_until_parked();
    assert!(vault(&tabs, cx).reply(&id).is_err());
    cx.update(|_, cx| {
        let w = tabs.read(cx);
        assert!(w.reply_page.is_none());
        assert!(w.showing_home(cx));
        assert!(!matches!(
            w.navigation_target(true, cx),
            Some(Stop::Reply(_))
        ));
    });
}

/// Back and Forward stop at a Reply page, named by its title.
#[gpui::test]
fn back_and_forward_stop_at_a_reply_page(cx: &mut TestAppContext) {
    let dir = tempfile::tempdir().unwrap();
    let (tabs, cx, _) = applied_document(cx, dir.path(), Session::default());
    let id = paste_reply(&tabs, cx, "PERSON_1 replied.");
    tabs.update_in(cx, |tabs, window, cx| tabs.show_home(window, cx));
    cx.update(|_, cx| {
        assert_eq!(
            tabs.read(cx).navigation_label(true, cx),
            "Back to “Reply to contract”"
        );
    });
    tabs.update_in(cx, |tabs, window, cx| tabs.navigate(true, window, cx));
    cx.run_until_parked();
    assert_eq!(shown_reply(&tabs, cx).as_deref(), Some(id.as_str()));
    // A tab or another page closes it.
    let tab = cx.update(|_, cx| tabs.read(cx).tabs.active());
    tabs.update_in(cx, |tabs, window, cx| tabs.activate(tab, window, cx));
    assert!(shown_reply(&tabs, cx).is_none());
}

/// Every control on the Reply page is reached by Tab.
#[gpui::test]
fn the_reply_page_controls_are_keyboard_reachable(cx: &mut TestAppContext) {
    cx.update(crate::bind_all_keys);
    let dir = tempfile::tempdir().unwrap();
    let (tabs, cx, _) = applied_document(cx, dir.path(), Session::default());
    paste_reply(&tabs, cx, "PERSON_1 met PERSON_9.");
    draw(cx);
    let targets = cx.update(|_, cx| tabs.read(cx).reply_page.as_ref().unwrap().test_controls());
    assert_eq!(
        targets.len(),
        4,
        "Copy, Next unrecognized, Promote and Delete"
    );
    let mut seen = Vec::new();
    for _ in 0..30 {
        cx.simulate_keystrokes("tab");
        draw(cx);
        cx.update(|window, cx| seen.extend(window.focused(cx)));
        if targets.iter().all(|(_, focus)| seen.contains(focus)) {
            break;
        }
    }
    for (name, focus) in targets {
        assert!(seen.contains(&focus), "Tab did not reach {name}");
    }
}

/// Deleting a Project with Move to Unfiled deletes its own replies, while
/// its documents' replies follow them to Unfiled.
#[gpui::test]
fn a_project_deleted_to_unfiled_keeps_its_documents_replies_only(cx: &mut TestAppContext) {
    let dir = tempfile::tempdir().unwrap();
    let (tabs, cx) = boot(cx, Session::default());
    cx.simulate_resize(size(px(1400.), px(850.)));
    let unfiled = open_applied(
        &tabs,
        cx,
        dir.path(),
        "note.md",
        "Anna Petrova called.",
        &[("Anna Petrova", Person)],
    );
    let documents_reply = paste_reply(&tabs, cx, "PERSON_1 called back.");
    tabs.update_in(cx, |tabs, window, cx| tabs.activate(unfiled, window, cx));
    let project = new_project(&tabs, cx, unfiled);
    tabs.update(cx, |tabs, cx| tabs.dismiss_toast(cx));
    tabs.update_in(cx, |tabs, window, cx| tabs.activate(unfiled, window, cx));
    let projects_reply = paste_reply(&tabs, cx, "PERSON_1 agreed.");
    assert_eq!(vault(&tabs, cx).project_replies(&project).len(), 2);
    tabs.update_in(cx, |tabs, window, cx| {
        tabs.open_project(&project, window, cx)
    });
    tabs.update(cx, |tabs, cx| {
        tabs.show_project_segment(Segment::Replies, cx)
    });
    draw(cx);
    assert!(cx.debug_bounds("project-reply-1").is_some());
    tabs.update_in(cx, |tabs, window, cx| {
        tabs.delete_project(project.clone(), window, cx)
    });
    cx.simulate_prompt_answer("Move to Unfiled");
    cx.run_until_parked();
    let vault = vault(&tabs, cx);
    assert!(vault.reply(&projects_reply).is_err());
    assert!(vault.reply(&documents_reply).is_ok());
}

/// A reply in the box is saved when its tab closes before it settles.
#[gpui::test]
fn closing_a_tab_saves_the_reply_in_its_box(cx: &mut TestAppContext) {
    let dir = tempfile::tempdir().unwrap();
    let (tabs, cx, _) = applied_document(cx, dir.path(), Session::default());
    let tab = cx.update(|_, cx| tabs.read(cx).tabs.active());
    let view = active(&tabs, cx);
    view.update_in(cx, |view, window, cx| {
        if !view.replacements_open() {
            view.toggle_replacements_panel(window, cx);
        }
        view.inspector = crate::document_view::Inspector::Share;
        cx.notify();
    });
    draw(cx);
    view.update_in(cx, |view, window, cx| {
        view.fill_reply("PERSON_1 wrote back.", window, cx)
    });
    cx.run_until_parked();
    assert!(vault(&tabs, cx).replies().is_empty());
    tabs.update_in(cx, |tabs, window, cx| tabs.close_tab(tab, window, cx));
    cx.run_until_parked();
    let replies = vault(&tabs, cx).replies();
    assert_eq!(replies.len(), 1);
    assert_eq!(
        vault(&tabs, cx).reply(&replies[0].id).unwrap().markdown,
        "PERSON_1 wrote back."
    );
    cx.update(|_, cx| assert!(!tabs.read(cx).tabs.contains(tab)));
}

/// A reply deleted from its page stays deleted: the box that saved it does
/// not save the same text again on the next autosave or close.
#[gpui::test]
fn a_reply_deleted_from_its_page_is_not_saved_again_by_its_box(cx: &mut TestAppContext) {
    let dir = tempfile::tempdir().unwrap();
    let (tabs, cx, _) = applied_document(cx, dir.path(), Session::default());
    let tab = cx.update(|_, cx| tabs.read(cx).tabs.active());
    let view = active(&tabs, cx);
    view.update_in(cx, |view, window, cx| {
        view.inspector = crate::document_view::Inspector::Share;
        if !view.replacements_open() {
            view.toggle_replacements_panel(window, cx);
        }
    });
    draw(cx);
    view.update_in(cx, |view, window, cx| {
        view.fill_reply("PERSON_1 wrote back.", window, cx);
        view.open_saved_reply(cx);
    });
    cx.run_until_parked();
    let id = shown_reply(&tabs, cx).expect("Open reply shows it");
    tabs.update_in(cx, |tabs, window, cx| {
        tabs.delete_saved_reply(&id, window, cx)
    });
    cx.run_until_parked();
    assert!(vault(&tabs, cx).replies().is_empty());
    tabs.update_in(cx, |tabs, window, cx| {
        tabs.autosave(tab, window, cx);
        tabs.close_tab(tab, window, cx);
    });
    cx.run_until_parked();
    assert!(vault(&tabs, cx).replies().is_empty(), "not saved again");
}

/// Deleting a Project deletes its own replies; one open on its page closes.
#[gpui::test]
fn a_project_reply_open_on_its_page_closes_when_the_project_is_deleted(cx: &mut TestAppContext) {
    let dir = tempfile::tempdir().unwrap();
    let (tabs, cx, _) = applied_document(cx, dir.path(), Session::default());
    let tab = cx.update(|_, cx| tabs.read(cx).tabs.active());
    let project = new_project(&tabs, cx, tab);
    tabs.update(cx, |tabs, cx| tabs.dismiss_toast(cx));
    let id = paste_reply(&tabs, cx, "PERSON_1 agreed.");
    tabs.update_in(cx, |tabs, window, cx| {
        tabs.delete_project(project.clone(), window, cx)
    });
    cx.simulate_prompt_answer("Move to Unfiled");
    cx.run_until_parked();
    assert!(vault(&tabs, cx).reply(&id).is_err());
    cx.update(|_, cx| {
        let w = tabs.read(cx);
        assert!(w.reply_page.is_none(), "the page of a deleted reply closes");
        assert!(!matches!(
            w.navigation_target(true, cx),
            Some(Stop::Reply(_))
        ));
    });
}

/// Paste AI Reply is saved even when its tab closes before the save runs.
#[gpui::test]
fn a_pasted_reply_is_saved_after_its_tab_closes(cx: &mut TestAppContext) {
    let dir = tempfile::tempdir().unwrap();
    let (tabs, cx, _) = applied_document(cx, dir.path(), Session::default());
    let tab = cx.update(|_, cx| tabs.read(cx).tabs.active());
    let view = active(&tabs, cx);
    // An edit waiting to be written puts the paste behind it in the queue.
    view.update(cx, |view, cx| {
        view.editor.update(cx, |editor, cx| {
            editor.set_text("PERSON_1 met ORG_1 again.", cx)
        })
    });
    cx.write_to_clipboard(gpui::ClipboardItem::new_string("PERSON_1 replied.".into()));
    view.update_in(cx, |view, window, cx| {
        view.paste_ai_response(&PasteAiResponse, window, cx)
    });
    tabs.update_in(cx, |tabs, window, cx| tabs.close_tab(tab, window, cx));
    cx.run_until_parked();
    let replies = vault(&tabs, cx).replies();
    assert_eq!(replies.len(), 1);
    assert_eq!(
        vault(&tabs, cx).reply(&replies[0].id).unwrap().markdown,
        "PERSON_1 replied."
    );
}

/// A reply pasted while its document is moving is not bound against the
/// mapping it moves into: it is refused, and says why.
#[gpui::test]
fn a_reply_pasted_during_a_move_is_refused(cx: &mut TestAppContext) {
    let dir = tempfile::tempdir().unwrap();
    let (tabs, cx, _) = applied_document(cx, dir.path(), Session::default());
    let tab = cx.update(|_, cx| tabs.read(cx).tabs.active());
    let view = active(&tabs, cx);
    cx.write_to_clipboard(gpui::ClipboardItem::new_string("PERSON_1 replied.".into()));
    tabs.update_in(cx, |tabs, window, cx| {
        tabs.move_to_new_project(tab, window, cx);
        view.update(cx, |view, cx| {
            view.paste_ai_response(&PasteAiResponse, window, cx)
        });
    });
    cx.run_until_parked();
    assert!(vault(&tabs, cx).replies().is_empty());
    let notice = cx
        .update(|_, cx| tabs.read(cx).notice_text())
        .unwrap_or_default();
    assert!(notice.contains("is moving"), "{notice}");
}

/// Review of #1152 (P2): a reply open on its page shows the box's edit once
/// it is saved in place, so Promote takes the text the page shows.
#[gpui::test]
fn a_reply_page_shows_its_reply_edited_in_the_box(cx: &mut TestAppContext) {
    let dir = tempfile::tempdir().unwrap();
    let (tabs, cx, _) = applied_document(cx, dir.path(), Session::default());
    let tab = cx.update(|_, cx| tabs.read(cx).tabs.active());
    let view = active(&tabs, cx);
    view.update_in(cx, |view, window, cx| {
        view.inspector = crate::document_view::Inspector::Share;
        if !view.replacements_open() {
            view.toggle_replacements_panel(window, cx);
        }
    });
    draw(cx);
    view.update_in(cx, |view, window, cx| {
        view.fill_reply("PERSON_1 wrote bakc.", window, cx);
        view.open_saved_reply(cx);
    });
    cx.run_until_parked();
    let id = shown_reply(&tabs, cx).expect("Open reply shows it");
    assert!(page_text(&tabs, cx).contains("wrote bakc."));
    view.update_in(cx, |view, window, cx| {
        view.fill_reply("PERSON_1 wrote back.", window, cx)
    });
    tabs.update_in(cx, |tabs, window, cx| tabs.autosave(tab, window, cx));
    cx.run_until_parked();
    assert_eq!(shown_reply(&tabs, cx).as_deref(), Some(id.as_str()));
    let page = page_text(&tabs, cx);
    assert!(page.contains("wrote back."), "{page}");
}
