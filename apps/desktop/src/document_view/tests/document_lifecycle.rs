//! Loading and closing a document, and its own dialogs
//! (`DocumentLifecycle`).
use super::*;

#[gpui::test]
fn closing_an_edited_tab_asks_nothing_and_keeps_its_text(cx: &mut TestAppContext) {
    let (app, cx) = boot(cx);
    cx.simulate_input("keep me");
    let _ = close_document(&app, cx);
    cx.run_until_parked();
    assert!(!cx.has_pending_prompt());
    cx.update(|window, _| assert!(!window.has_active_prompt()));
    let next = active_document(&app, cx);
    assert_ne!(next, app);
    let vault = cx.update(|window, cx| {
        let workspace = window.root::<workspace::Workspace>().flatten().unwrap();
        workspace.read(cx).opened_vault().unwrap()
    });
    let documents = vault.documents();
    assert_eq!(documents.len(), 1);
    assert_eq!(
        vault.document(&documents[0].id).unwrap().markdown,
        "keep me"
    );
}

#[gpui::test]
fn failed_open_preserves_current_document(cx: &mut TestAppContext) {
    let dir = tempfile::tempdir().unwrap();
    let (app, cx) = boot(cx);
    app.update_in(cx, |app, window, cx| {
        app.open_path(dir.path().join("missing.md"), window, cx)
    });
    cx.run_until_parked();
    // Nothing to import: no tab opens, and the notice names the file.
    assert_eq!(active_document(&app, cx), app);
    cx.update(|window, cx| {
        let tabs = window.root::<workspace::Workspace>().flatten().unwrap();
        let notice = tabs.read(cx).notice_text().unwrap();
        assert!(notice.contains("missing.md"), "{notice}");
    });
}

#[gpui::test]
fn opening_and_saving_preserves_markdown_bytes(cx: &mut TestAppContext) {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("source.md");
    let source = "\u{feff}# Title\r\n\r\nWords $$x + y$$ more words.\r\n\tIndented\r\n";
    std::fs::write(&path, source).unwrap();
    let (app, cx) = boot(cx);
    app.update_in(cx, |app, window, cx| {
        app.open_path(path.clone(), window, cx)
    });
    assert!(!cx.update(|_, cx| app.read(cx).unsaved(cx)));
    cx.dispatch_action(Save);
    cx.run_until_parked();
    assert_eq!(std::fs::read_to_string(path).unwrap(), source);
}

/// mdoc's confirm dialog asking a save question over a focused draft; the
/// answer lands in the cell.
fn ask_to_close(
    cx: &mut TestAppContext,
) -> (
    Entity<DocumentView>,
    &mut VisualTestContext,
    std::rc::Rc<std::cell::Cell<Option<usize>>>,
) {
    let (app, cx) = boot(cx);
    cx.update(|_, cx| {
        crate::ui::confirm_dialog::bind_keys(cx);
        crate::ui::confirm_dialog::install(
            std::rc::Rc::new(std::cell::Cell::new(style::Theme::Light)),
            cx,
        )
    });
    let answer = std::rc::Rc::new(std::cell::Cell::new(None));
    let cell = answer.clone();
    app.update_in(cx, |app, window, cx| {
        app.editor.update(cx, |editor, cx| {
            editor.set_text("draft", cx);
            editor.focus(window, cx);
        });
        let prompt = window.prompt(
            gpui::PromptLevel::Warning,
            "Save changes?",
            None,
            &[
                gpui::PromptButton::new("Save"),
                gpui::PromptButton::new("Don't Save"),
                gpui::PromptButton::cancel("Cancel"),
            ],
            cx,
        );
        cx.spawn(async move |_, _| cell.set(prompt.await.ok()))
            .detach();
    });
    cx.run_until_parked();
    assert!(!cx.has_pending_prompt(), "not the platform prompt");
    assert!(cx.update(|window, _| window.has_active_prompt()));
    cx.update(|window, cx| window.draw(cx).clear(cx));
    assert!(cx.debug_bounds("confirm-dialog").is_some());
    (app, cx, answer)
}

#[gpui::test]
fn the_confirm_dialog_esc_cancels_and_focus_returns(cx: &mut TestAppContext) {
    let (app, cx, answer) = ask_to_close(cx);
    assert!(cx.update(|window, cx| {
        !app.read(cx)
            .editor
            .read(cx)
            .focus_handle(cx)
            .is_focused(window)
    }));
    cx.simulate_keystrokes("escape");
    cx.run_until_parked();
    assert!(cx.update(|window, _| !window.has_active_prompt()));
    assert_eq!(answer.get(), Some(2));
    assert!(cx.update(|window, cx| {
        app.read(cx)
            .editor
            .read(cx)
            .focus_handle(cx)
            .is_focused(window)
    }));
}

#[gpui::test]
fn the_confirm_dialog_discard_chord_answers_dont_save(cx: &mut TestAppContext) {
    let (_, cx, answer) = ask_to_close(cx);
    cx.simulate_keystrokes(if cfg!(target_os = "macos") {
        "cmd-d"
    } else {
        "alt-d"
    });
    cx.run_until_parked();
    assert!(cx.update(|window, _| !window.has_active_prompt()));
    assert_eq!(answer.get(), Some(1));
}

#[gpui::test]
fn the_confirm_dialog_buttons_follow_tab_and_click(cx: &mut TestAppContext) {
    let (_, cx, answer) = ask_to_close(cx);
    for _ in 0..3 {
        cx.simulate_keystrokes("tab");
    }
    cx.run_until_parked();
    assert!(
        cx.update(|window, _| window.has_active_prompt()),
        "Tab stays inside"
    );
    let cancel = cx.debug_bounds("confirm-Cancel").unwrap().center();
    cx.simulate_click(cancel, gpui::Modifiers::none());
    cx.run_until_parked();
    assert!(cx.update(|window, _| !window.has_active_prompt()));
    assert_eq!(answer.get(), Some(2));
}

#[gpui::test]
fn the_confirm_dialog_enter_answers_the_default_after_a_click_on_the_text(cx: &mut TestAppContext) {
    let (_, cx, answer) = ask_to_close(cx);
    let text = cx.debug_bounds("confirm-dialog").unwrap().origin + gpui::point(px(30.), px(30.));
    cx.simulate_click(text, gpui::Modifiers::none());
    cx.simulate_keystrokes("enter");
    cx.run_until_parked();
    assert!(cx.update(|window, _| !window.has_active_prompt()));
    assert_eq!(answer.get(), Some(0));
}

#[gpui::test]
fn files_opened_during_a_confirm_dialog_wait_for_the_answer(cx: &mut TestAppContext) {
    let (app, cx, _) = ask_to_close(cx);
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("later.md");
    std::fs::write(&path, "later").unwrap();
    app.update(cx, |_, cx| {
        cx.emit(workspace::TabEvent::Tabs(workspace::TabsEvent::Open(vec![
            path,
        ])))
    });
    cx.executor()
        .advance_clock(std::time::Duration::from_millis(300));
    cx.run_until_parked();
    assert!(
        cx.update(|window, _| window.has_active_prompt()),
        "the dialog keeps focus"
    );
    assert_eq!(active_document(&app, cx), app);
    cx.simulate_keystrokes("escape");
    cx.executor()
        .advance_clock(std::time::Duration::from_millis(300));
    cx.run_until_parked();
    let opened = active_document(&app, cx);
    assert_ne!(opened, app);
    assert_eq!(
        cx.update(|_, cx| opened.read(cx).editor.read(cx).text().to_owned()),
        "later"
    );
}

#[gpui::test]
fn an_open_already_resolving_waits_for_a_dialog_raised_meanwhile(cx: &mut TestAppContext) {
    let (app, cx) = boot(cx);
    cx.update(|_, cx| {
        crate::ui::confirm_dialog::install(
            std::rc::Rc::new(std::cell::Cell::new(style::Theme::Light)),
            cx,
        )
    });
    app.update(cx, |app, cx| {
        app.editor
            .update(cx, |editor, cx| editor.set_text("draft", cx))
    });
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("later.md");
    std::fs::write(&path, "later").unwrap();
    // The open starts resolving, then a question comes up before it finishes.
    app.update(cx, |_, cx| {
        cx.emit(workspace::TabEvent::Tabs(workspace::TabsEvent::Open(vec![
            path,
        ])))
    });
    app.update_in(cx, |_, window, cx| {
        drop(window.prompt(
            gpui::PromptLevel::Info,
            "A question",
            None,
            &[gpui::PromptButton::cancel("Cancel")],
            cx,
        ))
    });
    cx.run_until_parked();
    assert!(cx.update(|window, _| window.has_active_prompt()));
    assert_eq!(active_document(&app, cx), app, "the dialog keeps focus");
    cx.simulate_keystrokes("escape");
    cx.executor()
        .advance_clock(std::time::Duration::from_millis(300));
    cx.run_until_parked();
    let opened = active_document(&app, cx);
    assert_eq!(
        cx.update(|_, cx| opened.read(cx).editor.read(cx).text().to_owned()),
        "later"
    );
}
