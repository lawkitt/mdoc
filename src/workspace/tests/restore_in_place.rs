//! Restore in place (ADR 0078): the Share panel's AI reply box, the Reply
//! view, and the reply kept in the case.
use super::*;
use crate::pii::ui::CompareMode;
use mdoc_pii::{Category, restore::ExportedIdentity};

/// Open the Inspector's Share tab and draw it, so the reply box exists.
fn open_share(view: &Entity<DocumentView>, cx: &mut VisualTestContext) {
    view.update_in(cx, |view, window, cx| {
        if !view.replacements_open() {
            view.toggle_replacements_panel(window, cx);
        }
        view.inspector_share = true;
        cx.notify();
    });
    draw(cx);
    cx.update(|_, cx| assert!(view.read(cx).reply_box().is_some()));
}

fn fill(view: &Entity<DocumentView>, text: &str, cx: &mut VisualTestContext) {
    view.update_in(cx, |view, window, cx| view.fill_reply(text, window, cx));
    cx.run_until_parked();
    draw(cx);
}

fn mode(view: &Entity<DocumentView>, cx: &mut VisualTestContext) -> Option<CompareMode> {
    cx.update(|_, cx| view.read(cx).compare.as_ref().map(|c| c.mode))
}

fn reply_view_text(view: &Entity<DocumentView>, cx: &mut VisualTestContext) -> String {
    cx.update(|_, cx| {
        view.read(cx)
            .compare
            .as_ref()
            .expect("Reply view open")
            .reply_text(cx)
    })
}

fn summary(view: &Entity<DocumentView>, cx: &mut VisualTestContext) -> String {
    cx.update(|_, cx| view.read(cx).reply.summary())
}

/// An `identities.json` of a captured reply (tests/fixtures/restore/README.md).
fn captured_identities(json: &str) -> Vec<ExportedIdentity> {
    let list: serde_json::Value = serde_json::from_str(json).unwrap();
    let strings = |value: &serde_json::Value| -> Vec<String> {
        value
            .as_array()
            .map(|l| l.iter().map(|s| s.as_str().unwrap().to_owned()).collect())
            .unwrap_or_default()
    };
    list.as_array()
        .unwrap()
        .iter()
        .map(|entry| {
            let alias = entry["alias"].as_str().unwrap().to_owned();
            let category = *Category::ALL
                .iter()
                .find(|c| c.token() == entry["category"].as_str().unwrap())
                .unwrap();
            let generated = alias
                .split_once('_')
                .is_some_and(|(p, n)| p == category.token() && n.parse::<u64>().is_ok());
            ExportedIdentity {
                custom: !generated,
                previous: strings(&entry["previous"]),
                originals: strings(&entry["originals"])
                    .iter()
                    .map(|o| Arc::from(o.as_str()))
                    .collect(),
                category,
                alias,
            }
        })
        .collect()
}

/// Every captured ChatGPT and Claude reply (ADR 0056) restores completely in
/// the Reply view, exactly as in the restore tab.
#[gpui::test]
fn captured_replies_restore_completely_in_the_reply_view(cx: &mut TestAppContext) {
    let fixtures = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/restore");
    let mut captures: Vec<_> = std::fs::read_dir(&fixtures)
        .unwrap()
        .map(|e| e.unwrap().path())
        .filter(|p| p.is_dir())
        .collect();
    captures.sort();
    assert!(captures.len() >= 4, "the four captures of ADR 0056");
    let dir = tempfile::tempdir().unwrap();
    let (tabs, cx) = boot_home(cx, Session::default());
    cx.simulate_resize(size(px(1400.), px(850.)));
    for capture in captures {
        let name = capture.file_name().unwrap().to_string_lossy().to_string();
        let read = |file: &str| std::fs::read_to_string(capture.join(file)).unwrap();
        let identities = captured_identities(&read("identities.json"));
        let sent = read("sent.md");
        let (_, sent) = sent.split_once("\n\n---\n\n").unwrap();
        let path = dir.path().join(format!("{name}.md"));
        std::fs::write(&path, sent).unwrap();
        tabs.update_in(cx, |tabs, window, cx| {
            tabs.open_paths(vec![path], window, cx)
        });
        cx.run_until_parked();
        let view = active(&tabs, cx);
        view.update(cx, |view, cx| {
            let text = view.editor.read(cx).text().to_owned();
            let history = view.editor.read(cx).history_id();
            let review = view.pii.review_mut();
            let ids = review.seed(&identities, &text);
            review.relink(&text, &identities, &ids, history);
            view.review_changed(cx);
            assert!(view.has_applied_aliases(), "{name}");
        });
        open_share(&view, cx);
        fill(&view, &read("reply.md"), cx);
        assert_eq!(mode(&view, cx), Some(CompareMode::Reply), "{name}");
        assert_eq!(reply_view_text(&view, cx), read("expected.md"), "{name}");
        let summary = summary(&view, cx);
        assert!(summary.ends_with(" · 0 unrecognised"), "{name}: {summary}");
        assert!(!summary.starts_with("0 "), "{name}: {summary}");
        assert!(
            cx.debug_bounds("share-next-unrecognised").is_none(),
            "{name}"
        );
    }
}

/// The summary counts what came back, Next unrecognised walks the unknown
/// tokens in the Reply view, the view follows the mapping live, and Open as
/// tab makes the tab Paste AI Response makes.
#[gpui::test]
fn the_reply_box_restores_at_once_and_opens_as_the_restore_tab(cx: &mut TestAppContext) {
    let dir = tempfile::tempdir().unwrap();
    let (tabs, cx, _) = applied_document(cx, dir.path(), Session::default());
    cx.update(|_, cx| crate::pii::ui::bind_keys(cx));
    let source = active(&tabs, cx);
    open_share(&source, cx);
    cx.update(|window, cx| window.focus(&source.read(cx).replacements_focus(), cx));
    let box_focused = |cx: &mut VisualTestContext| {
        cx.update(|window, cx| {
            source
                .read(cx)
                .reply_box()
                .unwrap()
                .read(cx)
                .focus_handle(cx)
                .is_focused(window)
        })
    };
    for _ in 0..20 {
        cx.simulate_keystrokes("tab");
        draw(cx);
        if box_focused(cx) {
            break;
        }
    }
    assert!(box_focused(cx), "the reply box is reachable by Tab");
    cx.simulate_keystrokes("tab");
    draw(cx);
    assert!(
        !box_focused(cx),
        "Tab leaves the reply box: {:?}",
        cx.update(|window, cx| (
            format!("{:?}", window.context_stack()),
            source
                .read(cx)
                .reply_box()
                .unwrap()
                .read(cx)
                .text()
                .to_owned()
        ))
    );
    cx.simulate_keystrokes("shift-tab");
    draw(cx);
    assert!(box_focused(cx), "Shift-Tab returns to the reply box");
    cx.update(|_, cx| assert_eq!(source.read(cx).reply_box().unwrap().read(cx).text(), ""));
    // Before a reply: no Reply segment, actions off.
    source.update_in(cx, |view, window, cx| {
        view.set_view_mode(Some(CompareMode::Side), window, cx)
    });
    draw(cx);
    assert!(cx.debug_bounds("view-reply").is_none());
    source.update_in(cx, |view, window, cx| view.set_view_mode(None, window, cx));

    let reply = "**PERSON_1** owes ORG_1; person 1 and PERSON_7 disagree with PERSON_8.";
    // A real paste into the box, as people do.
    cx.write_to_clipboard(gpui::ClipboardItem::new_string(reply.into()));
    cx.update(|window, cx| {
        let editor = source.read(cx).reply_box().unwrap().clone();
        window.focus(&editor.read(cx).focus_handle(cx), cx);
    });
    cx.dispatch_action(mdoc_editor::Paste);
    cx.run_until_parked();
    draw(cx);
    assert_eq!(
        mode(&source, cx),
        Some(CompareMode::Reply),
        "filling switches"
    );
    cx.update(|window, cx| {
        let editor = source.read(cx).reply_box().unwrap().clone();
        assert!(
            editor.read(cx).focus_handle(cx).is_focused(window),
            "typing goes on in the box"
        );
    });
    assert!(cx.debug_bounds("view-reply").is_some());
    assert!(cx.debug_bounds("reply-view").is_some());
    let restored = "**Anna Petrova** owes Northwind Traders Ltd; Anna Petrova and PERSON_7 \
                    disagree with PERSON_8.";
    assert_eq!(reply_view_text(&source, cx), restored);
    // Three mentions of two identities: two values.
    assert_eq!(summary(&source, cx), "2 values restored · 2 unrecognised");
    // Unresolved tokens are marked in the view, nothing guessed.
    cx.update(|_, cx| {
        let shown = source.read(cx).reply.restored().clone();
        let left: Vec<_> = shown
            .unresolved
            .iter()
            .map(|r| &restored[r.clone()])
            .collect();
        assert_eq!(left, ["PERSON_7", "PERSON_8"]);
    });

    // Next unrecognised steps through them, also from the editor.
    source.update_in(cx, |view, window, cx| view.set_view_mode(None, window, cx));
    click_toolbar(cx, "share-next-unrecognised");
    assert_eq!(mode(&source, cx), Some(CompareMode::Reply));
    let active_token =
        |cx: &mut VisualTestContext| cx.update(|_, cx| source.read(cx).reply.current_unresolved());
    let first = active_token(cx);
    click_toolbar(cx, "share-next-unrecognised");
    let second = active_token(cx);
    assert!(first.is_some() && second.is_some() && first != second);
    click_toolbar(cx, "share-next-unrecognised");
    assert_eq!(active_token(cx), first, "wraps after the last");

    // Copy reply copies the restored text.
    click_toolbar(cx, "share-copy-reply");
    cx.update(|_, cx| assert_eq!(cx.read_from_clipboard().unwrap().text().unwrap(), restored));

    // Open as tab is today's restore tab.
    let source_id = cx.update(|_, cx| tabs.read(cx).tabs.active());
    click_toolbar(cx, "share-open-reply");
    let from_box = active(&tabs, cx);
    let (box_text, box_name, box_remaining, box_unresolved) = cx.update(|_, cx| {
        assert_ne!(tabs.read(cx).tabs.active(), source_id);
        let view = from_box.read(cx);
        (
            view.editor.read(cx).text().to_owned(),
            view.display_name(),
            view.pii.review().remaining(),
            view.unresolved_tokens(cx).len(),
        )
    });
    tabs.update_in(cx, |tabs, window, cx| tabs.activate(source_id, window, cx));
    cx.run_until_parked();
    cx.write_to_clipboard(gpui::ClipboardItem::new_string(reply.into()));
    source.update_in(cx, |view, window, cx| {
        view.paste_ai_response(&PasteAiResponse, window, cx)
    });
    cx.run_until_parked();
    let pasted = active(&tabs, cx);
    cx.update(|_, cx| {
        let view = pasted.read(cx);
        assert_ne!(pasted.entity_id(), from_box.entity_id());
        assert_eq!(view.editor.read(cx).text(), box_text);
        assert_eq!(box_text, restored);
        assert_eq!(view.display_name(), box_name);
        assert_eq!(view.pii.review().remaining(), box_remaining);
        assert_eq!(view.unresolved_tokens(cx).len(), box_unresolved);
    });

    // Live: the view follows the mapping. A forgotten identity's alias is
    // unresolved now, as in the restore tab; nothing is guessed.
    tabs.update_in(cx, |tabs, window, cx| tabs.activate(source_id, window, cx));
    cx.run_until_parked();
    source.update_in(cx, |view, window, cx| {
        view.set_view_mode(Some(CompareMode::Reply), window, cx);
        view.forget_case_identity(&["PERSON_1"], &["Anna Petrova".into()], &[], cx);
    });
    draw(cx);
    assert_eq!(
        reply_view_text(&source, cx),
        "**PERSON_1** owes Northwind Traders Ltd; person 1 and PERSON_7 disagree with PERSON_8."
    );
    assert_eq!(summary(&source, cx), "1 value restored · 3 unrecognised");

    // Reply is independent of the document's before/after views. Removing
    // the last applied identity must leave the reply readable and editable.
    source.update(cx, |view, cx| {
        view.forget_case_identity(&["ORG_1"], &["Northwind Traders Ltd".into()], &[], cx);
    });
    draw(cx);
    assert_eq!(mode(&source, cx), Some(CompareMode::Reply));
    assert!(cx.debug_bounds("view-reply").is_some());
    assert!(cx.debug_bounds("share-reply-box").is_some());
    source.update_in(cx, |view, window, cx| view.set_view_mode(None, window, cx));
    source.update_in(cx, |view, window, cx| {
        view.set_view_mode(Some(CompareMode::Reply), window, cx)
    });
    draw(cx);
    assert_eq!(mode(&source, cx), Some(CompareMode::Reply));
    assert_eq!(reply_view_text(&source, cx), reply);

    // Clearing the box removes the reply and leaves the Reply view.
    fill(&source, "", cx);
    assert_eq!(mode(&source, cx), None);
    assert!(cx.debug_bounds("view-reply").is_none());
    cx.update(|_, cx| assert!(!source.read(cx).reply.exists()));
}

/// In a case the reply is kept encrypted in the case file, survives closing
/// and reopening the member, and clearing the box removes it from the case.
/// It never reaches the session manifest.
#[gpui::test]
fn a_case_member_keeps_its_reply_encrypted_across_reopening(cx: &mut TestAppContext) {
    let dir = tempfile::tempdir().unwrap();
    let (tabs, cx, path) = applied_document(cx, dir.path(), Session::default());
    let view = active(&tabs, cx);
    view.update(cx, |view, cx| view.write(path.clone(), None, cx));
    cx.run_until_parked();
    let tab = cx.update(|_, cx| tabs.read(cx).tabs.active());
    let case_id = tabs
        .update_in(cx, |tabs, window, cx| tabs.create_case(tab, window, cx))
        .unwrap();
    cx.run_until_parked();
    open_share(&view, cx);
    let reply = "PERSON_1 must pay ORG_1 by Friday, says the reply-only line.";
    fill(&view, reply, cx);
    let store = mdoc_store::cases::Store::with_key(dir.path().join("cases"), &[3; 32]);
    let member = session_store::identity(&path);
    assert_eq!(store.load(&case_id).unwrap().reply(&member), Some(reply));
    let bytes = std::fs::read(dir.path().join("cases").join(format!("{case_id}.case"))).unwrap();
    assert!(!String::from_utf8_lossy(&bytes).contains("reply-only"));

    // The session manifest holds no reply text, pasted or restored.
    let manifest = dir.path().join("session.json");
    cx.update(|_, cx| session_store::save(&manifest, &tabs.read(cx).snapshot(cx)).unwrap());
    let json = std::fs::read_to_string(&manifest).unwrap();
    assert!(!json.contains("reply-only") && !json.contains("Friday"));

    // Close the member, then open it again: the reply comes back with it.
    tabs.update_in(cx, |tabs, window, cx| tabs.close_tab(tab, window, cx));
    cx.run_until_parked();
    assert!(!cx.has_pending_prompt(), "saved document, saved case");
    cx.update(|_, cx| assert!(!tabs.read(cx).tabs.contains(tab)));
    tabs.update_in(cx, |tabs, window, cx| {
        tabs.open_paths(vec![path.clone()], window, cx)
    });
    cx.run_until_parked();
    let reopened = active(&tabs, cx);
    cx.update(|_, cx| {
        let view = reopened.read(cx);
        assert_eq!(
            view.case.link().map(|l| l.id.as_str()),
            Some(case_id.as_str())
        );
        assert_eq!(view.reply.text(), reply);
    });
    assert_eq!(mode(&reopened, cx), None, "reopening does not switch views");
    open_share(&reopened, cx);
    cx.update(|_, cx| {
        let view = reopened.read(cx);
        assert_eq!(view.reply_box().unwrap().read(cx).text(), reply);
    });
    assert_eq!(summary(&reopened, cx), "2 values restored · 0 unrecognised");
    reopened.update_in(cx, |view, window, cx| {
        view.set_view_mode(Some(CompareMode::Reply), window, cx)
    });
    draw(cx);
    assert_eq!(
        reply_view_text(&reopened, cx),
        "Anna Petrova must pay Northwind Traders Ltd by Friday, says the reply-only line."
    );

    // Clearing the box removes it from the case too.
    fill(&reopened, "", cx);
    assert_eq!(store.load(&case_id).unwrap().reply(&member), None);
    assert!(store.load(&case_id).unwrap().replies.is_empty());
}

/// Without a case the reply lives only while the tab is open.
#[gpui::test]
fn without_a_case_the_reply_lives_only_in_the_tab(cx: &mut TestAppContext) {
    let dir = tempfile::tempdir().unwrap();
    let (tabs, cx, path) = applied_document(cx, dir.path(), Session::default());
    let view = active(&tabs, cx);
    view.update(cx, |view, cx| view.write(path.clone(), None, cx));
    cx.run_until_parked();
    open_share(&view, cx);
    fill(&view, "PERSON_1 replied, tab-only.", cx);
    assert!(
        !dir.path().join("cases").exists()
            || std::fs::read_dir(dir.path().join("cases"))
                .unwrap()
                .next()
                .is_none(),
        "no case file is written"
    );
    let manifest = dir.path().join("session.json");
    cx.update(|_, cx| session_store::save(&manifest, &tabs.read(cx).snapshot(cx)).unwrap());
    assert!(
        !std::fs::read_to_string(&manifest)
            .unwrap()
            .contains("tab-only")
    );
    let tab = cx.update(|_, cx| tabs.read(cx).tabs.active());
    // The file is saved; only the memory-only mapping needs a close answer.
    tabs.update_in(cx, |tabs, window, cx| tabs.close_tab(tab, window, cx));
    cx.run_until_parked();
    assert!(cx.has_pending_prompt());
    cx.simulate_prompt_answer("Close");
    cx.run_until_parked();
    cx.update(|_, cx| assert!(!tabs.read(cx).tabs.contains(tab)));
    tabs.update_in(cx, |tabs, window, cx| {
        tabs.open_paths(vec![path], window, cx)
    });
    cx.run_until_parked();
    let reopened = active(&tabs, cx);
    cx.update(|_, cx| assert!(!reopened.read(cx).reply.exists()));
}

#[gpui::test]
fn a_reply_follows_case_creation_member_rename_leave_and_join(cx: &mut TestAppContext) {
    let dir = tempfile::tempdir().unwrap();
    let (tabs, cx, path) = applied_document(cx, dir.path(), Session::default());
    let view = active(&tabs, cx);
    open_share(&view, cx);
    let reply = "PERSON_1 owes ORG_1 a response.";
    fill(&view, reply, cx);
    view.update(cx, |view, cx| view.write(path.clone(), None, cx));
    cx.run_until_parked();
    let tab = cx.update(|_, cx| tabs.read(cx).tabs.active());
    let case = tabs.update_in(cx, |tabs, window, cx| {
        tabs.create_case(tab, window, cx).unwrap()
    });
    cx.run_until_parked();
    let store = mdoc_store::cases::Store::with_key(dir.path().join("cases"), &[3; 32]);
    let old = session_store::identity(&path);
    assert_eq!(store.load(&case).unwrap().reply(&old), Some(reply));

    let renamed = dir.path().join("renamed.md");
    view.update(cx, |view, cx| view.write(renamed.clone(), None, cx));
    cx.run_until_parked();
    let member = session_store::identity(&renamed);
    let saved = store.load(&case).unwrap();
    assert_eq!(saved.reply(&old), None);
    assert_eq!(saved.reply(&member), Some(reply));

    tabs.update_in(cx, |tabs, window, cx| {
        tabs.remove_from_case(tab, window, cx)
    });
    cx.run_until_parked();
    assert!(store.load(&case).unwrap().replies.is_empty());
    cx.update(|_, cx| assert_eq!(view.read(cx).reply.text(), reply));
    tabs.update_in(cx, |tabs, window, cx| {
        tabs.join_case(tab, &case, window, cx)
    });
    cx.run_until_parked();
    assert_eq!(store.load(&case).unwrap().reply(&member), Some(reply));
    fill(&view, " \n  ", cx);
    assert!(store.load(&case).unwrap().replies.is_empty());
}

/// The box shows while aliases are applied or a reply exists. With the
/// aliases gone, Copy reply and Open as tab are off and say why, and Open as
/// tab has Paste AI Response's guard.
#[gpui::test]
fn without_aliases_the_box_hides_and_a_left_reply_restores_nothing(cx: &mut TestAppContext) {
    let dir = tempfile::tempdir().unwrap();
    let (tabs, cx, _) = applied_document(cx, dir.path(), Session::default());
    let source = active(&tabs, cx);
    let source_id = cx.update(|_, cx| tabs.read(cx).tabs.active());
    let plain = dir.path().join("plain.md");
    std::fs::write(&plain, "Nothing here was replaced.").unwrap();
    tabs.update_in(cx, |tabs, window, cx| {
        tabs.open_paths(vec![plain], window, cx)
    });
    cx.run_until_parked();
    let other = active(&tabs, cx);
    open_share(&other, cx);
    assert!(
        cx.debug_bounds("share-reply-box").is_none(),
        "no aliases, no reply"
    );
    assert!(cx.debug_bounds("share-copy-reply").is_none());

    tabs.update_in(cx, |tabs, window, cx| tabs.activate(source_id, window, cx));
    cx.run_until_parked();
    open_share(&source, cx);
    assert!(cx.debug_bounds("share-reply-box").is_some());
    fill(&source, "PERSON_1 wrote back to ORG_1.", cx);
    assert!(cx.debug_bounds("share-reply-nothing").is_none());
    cx.update(|_, cx| assert!(source.read(cx).reply.restorable()));

    source.update(cx, |view, cx| {
        view.forget_case_identity(&["PERSON_1"], &["Anna Petrova".into()], &[], cx);
        view.forget_case_identity(&["ORG_1"], &["Northwind Traders Ltd".into()], &[], cx);
    });
    draw(cx);
    cx.update(|_, cx| assert!(!source.read(cx).has_applied_aliases()));
    assert!(
        cx.debug_bounds("share-reply-box").is_some(),
        "the reply keeps the box"
    );
    assert!(cx.debug_bounds("share-reply-nothing").is_some());
    cx.update(|_, cx| assert!(!source.read(cx).reply.restorable()));
    let tab_count = |cx: &mut VisualTestContext| cx.update(|_, cx| tabs.read(cx).tabs.len());
    let before = tab_count(cx);
    cx.write_to_clipboard(gpui::ClipboardItem::new_string("untouched".into()));
    click_toolbar(cx, "share-copy-reply");
    cx.update(|_, cx| {
        assert_eq!(
            cx.read_from_clipboard().unwrap().text().unwrap(),
            "untouched"
        )
    });
    click_toolbar(cx, "share-open-reply");
    assert_eq!(tab_count(cx), before, "Open as tab is off");
    source.update(cx, |view, cx| view.open_reply_as_tab(cx));
    cx.run_until_parked();
    assert_eq!(tab_count(cx), before);
    cx.update(|_, cx| {
        let error = source.read(cx).error.clone().unwrap_or_default();
        assert!(
            error.starts_with("Open as tab needs a document whose names were replaced"),
            "{error}"
        );
    });
}

/// The reply editor is one Tab stop: the box only rings it. Tracking the
/// editor's handle on the box too left a duplicate stop that ⇧Tab looped on,
/// which hung the What-you-did popover's Tab containment (ADR 0079).
#[gpui::test]
fn the_reply_box_is_one_tab_stop(cx: &mut TestAppContext) {
    let dir = tempfile::tempdir().unwrap();
    let (tabs, cx, _) = applied_document(cx, dir.path(), Session::default());
    let view = active(&tabs, cx);
    open_share(&view, cx);
    let editor = cx.update(|_, cx| view.read(cx).reply_box().unwrap().focus_handle(cx));
    for backwards in [true, false] {
        cx.update(|window, cx| {
            window.focus(&editor, cx);
            if backwards {
                window.focus_prev(cx);
            } else {
                window.focus_next(cx);
            }
            assert!(!editor.is_focused(window), "backwards: {backwards}");
            if backwards {
                window.focus_next(cx);
            } else {
                window.focus_prev(cx);
            }
            assert!(
                editor.is_focused(window),
                "and back, backwards: {backwards}"
            );
        });
    }
}

/// Space, ⌫ and Enter typed in the box edit the reply; none reaches the
/// Replacements list's bindings (ADR 0048). Share does not mount the list's
/// handlers today; `!ReplyBox` in `LIST` keeps this true if it ever does.
#[gpui::test]
fn typing_in_the_reply_box_stays_out_of_the_replacements_list(cx: &mut TestAppContext) {
    let dir = tempfile::tempdir().unwrap();
    let (tabs, cx, _) = applied_document(cx, dir.path(), Session::default());
    cx.update(|_, cx| crate::pii::ui::bind_keys(cx));
    let view = active(&tabs, cx);
    open_share(&view, cx);
    let state = |cx: &mut VisualTestContext| {
        cx.update(|_, cx| {
            let view = view.read(cx);
            (
                view.pii.review().applied().len(),
                view.pii.review().remaining(),
                view.editor.read(cx).text().to_owned(),
            )
        })
    };
    let before = state(cx);
    cx.update(|window, cx| {
        let editor = view.read(cx).reply_box().unwrap().clone();
        window.focus(&editor.read(cx).focus_handle(cx), cx);
    });
    cx.simulate_keystrokes("P space x backspace enter Q");
    cx.run_until_parked();
    draw(cx);
    cx.update(|_, cx| {
        let view = view.read(cx);
        // Enter types one newline: the box is plain text.
        assert_eq!(view.reply_box().unwrap().read(cx).text(), "P \nQ");
        assert_eq!(view.reply.text(), "P \nQ");
    });
    assert_eq!(state(cx), before, "the list and document are untouched");
}

/// An untitled restore tab in a case keeps its reply in memory until its
/// first Save As, which stores it in the case under the new path.
#[gpui::test]
fn an_untitled_case_tab_stores_its_reply_on_first_save_as(cx: &mut TestAppContext) {
    let dir = tempfile::tempdir().unwrap();
    let (tabs, cx, path) = applied_document(cx, dir.path(), Session::default());
    let member = active(&tabs, cx);
    member.update(cx, |view, cx| view.write(path.clone(), None, cx));
    cx.run_until_parked();
    let tab = cx.update(|_, cx| tabs.read(cx).tabs.active());
    let case = tabs
        .update_in(cx, |tabs, window, cx| tabs.create_case(tab, window, cx))
        .unwrap();
    cx.run_until_parked();
    open_share(&member, cx);
    let first = "PERSON_1 owes ORG_1 the member's reply.";
    fill(&member, first, cx);
    click_toolbar(cx, "share-open-reply");
    let restored = active(&tabs, cx);
    cx.update(|_, cx| {
        let view = restored.read(cx);
        assert_ne!(restored.entity_id(), member.entity_id());
        assert!(view.session.document.path.is_none(), "an untitled tab");
        assert_eq!(view.case.link().map(|l| l.id.as_str()), Some(case.as_str()));
    });
    let second = "PERSON_1 answered in the untitled tab.";
    restored.update_in(cx, |view, window, cx| view.fill_reply(second, window, cx));
    cx.run_until_parked();
    let store = mdoc_store::cases::Store::with_key(dir.path().join("cases"), &[3; 32]);
    let saved = store.load(&case).unwrap();
    assert_eq!(saved.replies.len(), 1, "only in memory until saved");
    assert_eq!(saved.reply(&session_store::identity(&path)), Some(first));

    let new_path = dir.path().join("restored.md");
    restored.update(cx, |view, cx| view.write(new_path.clone(), None, cx));
    cx.run_until_parked();
    let saved = store.load(&case).unwrap();
    assert_eq!(
        saved.reply(&session_store::identity(&new_path)),
        Some(second)
    );
    assert_eq!(saved.reply(&session_store::identity(&path)), Some(first));
}

/// Two saved members whose people hold opposite aliases: the case's `a.md`
/// has Boris as PERSON_1 and Anna as PERSON_2; `b.md`, not in the case yet,
/// has Anna as PERSON_1 and Boris as PERSON_2, and a reply naming both.
struct Swapped<'a> {
    tabs: Entity<Workspace>,
    cx: &'a mut VisualTestContext,
    first: u64,
    second: u64,
    view: Entity<DocumentView>,
    path: PathBuf,
    case: String,
    store: mdoc_store::cases::Store,
}

const SWAPPED_REPLY: &str = "PERSON_1 owes PERSON_2, says person 1.";
const SWAPPED_RESTORED: &str = "Anna Petrova owes Boris Ivanov, says Anna Petrova.";

fn swapped<'a>(cx: &'a mut TestAppContext, dir: &std::path::Path) -> Swapped<'a> {
    use mdoc_pii::Category::Person;
    let (tabs, cx) = boot(cx, Session::default());
    cx.simulate_resize(size(px(1400.), px(850.)));
    tabs.update(cx, |tabs, _| {
        tabs.use_case_store(mdoc_store::cases::Store::with_key(
            dir.join("cases"),
            &[3; 32],
        ))
    });
    let first = open_applied(
        &tabs,
        cx,
        dir,
        "a.md",
        "Boris Ivanov met Anna Petrova.",
        &[("Boris Ivanov", Person), ("Anna Petrova", Person)],
    );
    let first_view = active(&tabs, cx);
    first_view.update(cx, |view, cx| view.write(dir.join("a.md"), None, cx));
    cx.run_until_parked();
    let case = tabs
        .update_in(cx, |tabs, window, cx| tabs.create_case(first, window, cx))
        .unwrap();
    cx.run_until_parked();
    let second = open_applied(
        &tabs,
        cx,
        dir,
        "b.md",
        "Anna Petrova wrote to Boris Ivanov.",
        &[("Anna Petrova", Person), ("Boris Ivanov", Person)],
    );
    let view = active(&tabs, cx);
    let path = dir.join("b.md");
    view.update(cx, |view, cx| view.write(path.clone(), None, cx));
    cx.run_until_parked();
    cx.update(|_, cx| {
        assert_eq!(
            first_view.read(cx).editor.read(cx).text(),
            "PERSON_1 met PERSON_2."
        );
        assert_eq!(
            view.read(cx).editor.read(cx).text(),
            "PERSON_1 wrote to PERSON_2."
        );
    });
    open_share(&view, cx);
    fill(&view, SWAPPED_REPLY, cx);
    assert_eq!(restored_reply(&view, cx), SWAPPED_RESTORED);
    Swapped {
        tabs,
        cx,
        first,
        second,
        view,
        path,
        case,
        store: mdoc_store::cases::Store::with_key(dir.join("cases"), &[3; 32]),
    }
}

/// The reply restored against the tab's mapping now.
fn restored_reply(view: &Entity<DocumentView>, cx: &mut VisualTestContext) -> String {
    view.update(cx, |view, _| {
        view.refresh_reply();
        view.reply.restored().text.clone()
    })
}

/// Joining a case that holds the same people under each other's aliases
/// renumbers the document, and the reply's tokens follow: it still restores
/// to the same people, in the box, Copy reply and the encrypted case, across
/// Undo, Redo and reopening the member.
#[gpui::test]
fn a_join_that_swaps_aliases_keeps_the_reply_naming_the_same_people(cx: &mut TestAppContext) {
    let dir = tempfile::tempdir().unwrap();
    let Swapped {
        tabs,
        cx,
        second,
        view,
        path,
        case,
        store,
        ..
    } = swapped(cx, dir.path());
    tabs.update_in(cx, |tabs, window, cx| {
        tabs.join_case(second, &case, window, cx)
    });
    cx.run_until_parked();
    draw(cx);
    let member = session_store::identity(&path);
    let joined = "PERSON_2 owes PERSON_1, says person 2.";
    cx.update(|_, cx| {
        let view = view.read(cx);
        assert_eq!(view.editor.read(cx).text(), "PERSON_2 wrote to PERSON_1.");
        assert_eq!(view.reply.text(), joined, "the tokens follow their people");
        assert_eq!(view.reply_box().unwrap().read(cx).text(), joined);
    });
    assert_eq!(restored_reply(&view, cx), SWAPPED_RESTORED);
    assert_eq!(store.load(&case).unwrap().reply(&member), Some(joined));
    click_toolbar(cx, "share-copy-reply");
    cx.update(|_, cx| {
        assert_eq!(
            cx.read_from_clipboard().unwrap().text().unwrap(),
            SWAPPED_RESTORED
        )
    });

    // Undo in the document reverts the renumbering, and the reply with it.
    cx.update(|window, cx| window.focus(&view.read(cx).editor.read(cx).focus_handle(cx), cx));
    cx.dispatch_action(mdoc_editor::Undo);
    cx.run_until_parked();
    cx.update(|_, cx| {
        let view = view.read(cx);
        assert_eq!(view.editor.read(cx).text(), "PERSON_1 wrote to PERSON_2.");
        assert_eq!(view.reply.text(), SWAPPED_REPLY);
    });
    assert_eq!(restored_reply(&view, cx), SWAPPED_RESTORED);
    cx.dispatch_action(mdoc_editor::Redo);
    cx.run_until_parked();
    cx.update(|_, cx| assert_eq!(view.read(cx).reply.text(), joined));
    assert_eq!(restored_reply(&view, cx), SWAPPED_RESTORED);

    // Close the saved member and open it again from the encrypted case.
    view.update(cx, |view, cx| view.write(path.clone(), None, cx));
    cx.executor()
        .advance_clock(std::time::Duration::from_secs(1));
    cx.run_until_parked();
    assert_eq!(store.load(&case).unwrap().reply(&member), Some(joined));
    let bytes = std::fs::read(dir.path().join("cases").join(format!("{case}.case"))).unwrap();
    assert!(!String::from_utf8_lossy(&bytes).contains("owes"));
    tabs.update_in(cx, |tabs, window, cx| tabs.close_tab(second, window, cx));
    cx.run_until_parked();
    assert!(!cx.has_pending_prompt(), "saved document, saved case");
    tabs.update_in(cx, |tabs, window, cx| {
        tabs.open_paths(vec![path.clone()], window, cx)
    });
    cx.run_until_parked();
    let reopened = active(&tabs, cx);
    cx.update(|_, cx| {
        let view = reopened.read(cx);
        assert_eq!(view.case.link().map(|l| l.id.as_str()), Some(case.as_str()));
        assert_eq!(view.reply.text(), joined);
    });
    assert_eq!(restored_reply(&reopened, cx), SWAPPED_RESTORED);
}

/// A rename in one member reaches another as its own edit; that member's
/// reply follows the renamed identity and the case keeps it so.
#[gpui::test]
fn a_member_synced_after_another_member_renames_keeps_its_reply(cx: &mut TestAppContext) {
    use mdoc_pii::Category::Person;
    let dir = tempfile::tempdir().unwrap();
    let Swapped {
        tabs,
        cx,
        first,
        second,
        view,
        path,
        case,
        store,
    } = swapped(cx, dir.path());
    tabs.update_in(cx, |tabs, window, cx| {
        tabs.join_case(second, &case, window, cx)
    });
    cx.run_until_parked();
    let first_view = cx.update(|_, cx| {
        tabs.read(cx)
            .tabs
            .get(first)
            .and_then(|t| t.view().cloned())
            .unwrap()
    });
    first_view.update(cx, |view, cx| {
        let anna = view.pii.review().identity_by_alias("PERSON_2").unwrap();
        view.retag_identities(&[(anna, "BUYER".into(), Person, true)], cx)
            .unwrap();
    });
    cx.executor()
        .advance_clock(std::time::Duration::from_secs(1));
    cx.run_until_parked();
    let renamed = "BUYER owes PERSON_1, says BUYER.";
    cx.update(|_, cx| {
        let view = view.read(cx);
        assert_eq!(view.editor.read(cx).text(), "BUYER wrote to PERSON_1.");
        assert_eq!(view.reply.text(), renamed);
    });
    assert_eq!(restored_reply(&view, cx), SWAPPED_RESTORED);
    let member = session_store::identity(&path);
    assert_eq!(store.load(&case).unwrap().reply(&member), Some(renamed));
}

/// A real paste into the box is kept exactly: no Markdown paste rules split
/// `$$` math onto its own lines or flatten a paste inside a table row.
#[gpui::test]
fn a_paste_into_the_reply_box_is_kept_exactly(cx: &mut TestAppContext) {
    let dir = tempfile::tempdir().unwrap();
    let (tabs, cx, _) = applied_document(cx, dir.path(), Session::default());
    let view = active(&tabs, cx);
    open_share(&view, cx);
    let editor = cx.update(|_, cx| view.read(cx).reply_box().unwrap().clone());
    cx.update(|window, cx| window.focus(&editor.read(cx).focus_handle(cx), cx));
    let paste = |text: &str, cx: &mut VisualTestContext| {
        cx.write_to_clipboard(gpui::ClipboardItem::new_string(text.into()));
        cx.dispatch_action(mdoc_editor::Paste);
        cx.run_until_parked();
        draw(cx);
    };
    let table =
        "PERSON_1 calculated $$x$$ today.\n\n| who | owes |\n|---|---|\n| PERSON_1 | ORG_1 |\n";
    paste(table, cx);
    cx.update(|_, cx| {
        assert_eq!(editor.read(cx).text(), table);
        assert_eq!(view.read(cx).reply.text(), table);
    });
    // The caret inside the table's last row, then a paste with lines and pipes.
    let row = table.rfind("ORG_1").unwrap();
    editor.update(cx, |editor, cx| editor.set_cursor(row, cx));
    draw(cx);
    paste("a | b\r\nc ", cx);
    let expected = table.replacen("ORG_1", "a | b\nc ORG_1", 1);
    cx.update(|_, cx| assert_eq!(view.read(cx).reply.text(), expected));
    click_toolbar(cx, "share-copy-reply");
    cx.update(|_, cx| {
        assert_eq!(
            cx.read_from_clipboard().unwrap().text().unwrap(),
            expected
                .replace("PERSON_1", "Anna Petrova")
                .replace("ORG_1", "Northwind Traders Ltd")
        )
    });
}

/// The click on Next unrecognised that opens the Reply view scrolls a token
/// far below into view, though the view had not been laid out yet.
#[gpui::test]
fn next_unrecognised_reveals_its_first_token_when_it_opens_the_reply(cx: &mut TestAppContext) {
    let dir = tempfile::tempdir().unwrap();
    let (tabs, cx, _) = applied_document(cx, dir.path(), Session::default());
    let view = active(&tabs, cx);
    open_share(&view, cx);
    let filler: String = (0..120)
        .map(|i| format!("Line {i}: PERSON_1 wrote.\n\n"))
        .collect();
    fill(&view, &format!("{filler}PERSON_9 is unknown."), cx);
    view.update_in(cx, |view, window, cx| view.set_view_mode(None, window, cx));
    draw(cx);
    assert_eq!(mode(&view, cx), None);
    click_toolbar(cx, "share-next-unrecognised");
    assert_eq!(mode(&view, cx), Some(CompareMode::Reply));
    draw(cx);
    draw(cx);
    cx.update(|_, cx| {
        let view = view.read(cx);
        let token = view.reply.current_unresolved().unwrap();
        assert_eq!(&view.reply.restored().text[token.clone()], "PERSON_9");
        assert!(
            view.compare.as_ref().unwrap().reply_shows(token.start, cx),
            "the first click's token is in view"
        );
    });
}

/// Typing below the box's visible lines keeps the caret in view.
#[gpui::test]
fn typing_a_long_reply_keeps_the_caret_in_the_box(cx: &mut TestAppContext) {
    let dir = tempfile::tempdir().unwrap();
    let (tabs, cx, _) = applied_document(cx, dir.path(), Session::default());
    let view = active(&tabs, cx);
    open_share(&view, cx);
    let editor = cx.update(|_, cx| view.read(cx).reply_box().unwrap().clone());
    cx.update(|window, cx| window.focus(&editor.read(cx).focus_handle(cx), cx));
    let scroll = cx.update(|_, cx| view.read(cx).reply_scroll.clone());
    for _ in 0..20 {
        cx.simulate_keystrokes("P enter");
        draw(cx);
    }
    cx.simulate_keystrokes("Q");
    draw(cx);
    draw(cx);
    cx.update(|_, cx| {
        let caret = editor.read(cx).caret_bounds().unwrap();
        let viewport = scroll.bounds();
        assert!(scroll.offset().y < px(0.), "the box scrolled");
        assert!(
            caret.top() >= viewport.top() && caret.bottom() <= viewport.bottom(),
            "caret {caret:?} outside {viewport:?}"
        );
    });
}
