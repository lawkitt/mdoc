//! Loading, saving and closing a document, and its own dialogs
//! (`DocumentLifecycle`).
use super::*;

#[gpui::test]
fn editing_save_and_new_roundtrip(cx: &mut TestAppContext) {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("note.md");
    let (app, cx) = boot(cx);
    cx.simulate_input("# Hello");
    assert!(cx.update(|_, cx| app.read(cx).dirty(cx)));
    cx.dispatch_action(Save);
    cx.run_until_parked();
    assert!(cx.did_prompt_for_new_path());
    cx.simulate_new_path_selection(|_| Some(path.clone()));
    cx.run_until_parked();
    assert_eq!(std::fs::read_to_string(&path).unwrap(), "# Hello");
    assert!(!cx.update(|_, cx| app.read(cx).dirty(cx)));
    cx.dispatch_action(New);
    cx.run_until_parked();
    let blank = active_document(&app, cx);
    assert!(cx.update(|_, cx| blank.read(cx).editor.read(cx).text().is_empty()));
    cx.dispatch_action(Open);
    cx.run_until_parked();
    cx.simulate_path_prompt_response(|_| Some(vec![path]));
    cx.run_until_parked();
    let reopened = active_document(&app, cx);
    assert_eq!(
        reopened, app,
        "opening a retained file activates its original tab"
    );
    assert_eq!(
        cx.update(|_, cx| reopened.read(cx).editor.read(cx).text().to_owned()),
        "# Hello"
    );
}

#[gpui::test]
fn close_cancel_and_discard_protect_edits(cx: &mut TestAppContext) {
    let (app, cx) = boot(cx);
    cx.simulate_input("keep me");
    cx.dispatch_action(Close);
    cx.run_until_parked();
    assert!(cx.has_pending_prompt());
    cx.simulate_prompt_answer("Cancel");
    cx.run_until_parked();
    assert_eq!(
        cx.update(|_, cx| app.read(cx).editor.read(cx).text().to_owned()),
        "keep me"
    );
    cx.dispatch_action(Close);
    cx.run_until_parked();
    cx.simulate_prompt_answer("Discard");
    cx.run_until_parked();
    let next = active_document(&app, cx);
    assert!(cx.update(|_, cx| next.read(cx).editor.read(cx).text().is_empty()));
}

#[gpui::test]
fn cancelling_save_as_does_not_continue_close(cx: &mut TestAppContext) {
    let (app, cx) = boot(cx);
    cx.simulate_input("keep me");
    cx.dispatch_action(Close);
    cx.run_until_parked();
    cx.simulate_prompt_answer("Save");
    cx.run_until_parked();
    cx.simulate_new_path_selection(|_| None);
    cx.run_until_parked();
    assert!(cx.update(|_, cx| app.read(cx).dirty(cx)));
    assert_eq!(
        cx.update(|_, cx| app.read(cx).editor.read(cx).text().to_owned()),
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
    let failed = active_document(&app, cx);
    assert!(cx.update(|_, cx| failed.read(cx).error.is_some()));
    assert!(cx.update(|_, cx| failed.read(cx).lifecycle.unavailable()));
    assert_ne!(failed, app);
    assert!(cx.update(|_, cx| app.read(cx).session.document.path.is_none()));
}

#[gpui::test]
fn failed_save_does_not_discard_document(cx: &mut TestAppContext) {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("note.md");
    std::fs::write(&path, "original").unwrap();
    let (app, cx) = boot(cx);
    let app = open_document(&app, path.clone(), cx);
    cx.simulate_input("my edits");
    std::fs::write(&path, "external edit").unwrap();
    cx.dispatch_action(Close);
    cx.run_until_parked();
    cx.simulate_prompt_answer("Save");
    cx.run_until_parked();
    assert!(cx.update(|_, cx| app.read(cx).dirty(cx)));
    assert!(cx.update(|_, cx| app.read(cx).error.is_some()));
    assert_eq!(std::fs::read_to_string(path).unwrap(), "external edit");
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
    assert!(!cx.update(|_, cx| app.read(cx).dirty(cx)));
    cx.dispatch_action(Save);
    cx.run_until_parked();
    assert_eq!(std::fs::read_to_string(path).unwrap(), source);
}

/// Close a dirty tab with mdoc's confirm dialog installed (ADR 0064).
fn ask_to_close(cx: &mut TestAppContext) -> (Entity<DocumentView>, &mut VisualTestContext) {
    let (app, cx) = boot(cx);
    cx.update(|_, cx| {
        crate::confirm_dialog::bind_keys(cx);
        crate::confirm_dialog::install(
            std::rc::Rc::new(std::cell::Cell::new(style::Theme::Light)),
            cx,
        )
    });
    app.update_in(cx, |app, window, cx| {
        app.editor.update(cx, |editor, cx| {
            editor.set_text("draft", cx);
            editor.focus(window, cx);
        });
    });
    cx.dispatch_action(Close);
    cx.run_until_parked();
    assert!(!cx.has_pending_prompt(), "not the platform prompt");
    assert!(cx.update(|window, _| window.has_active_prompt()));
    cx.update(|window, cx| window.draw(cx).clear(cx));
    assert!(cx.debug_bounds("confirm-dialog").is_some());
    (app, cx)
}

#[gpui::test]
fn unsaved_changes_esc_cancels_and_focus_returns(cx: &mut TestAppContext) {
    let (app, cx) = ask_to_close(cx);
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
    assert_eq!(active_document(&app, cx), app, "the tab stays open");
    assert!(cx.update(|window, cx| {
        app.read(cx)
            .editor
            .read(cx)
            .focus_handle(cx)
            .is_focused(window)
    }));
}

#[gpui::test]
fn unsaved_changes_discard_chord_closes_without_saving(cx: &mut TestAppContext) {
    let (app, cx) = ask_to_close(cx);
    cx.simulate_keystrokes(if cfg!(target_os = "macos") {
        "cmd-d"
    } else {
        "alt-d"
    });
    cx.run_until_parked();
    assert!(cx.update(|window, _| !window.has_active_prompt()));
    assert_ne!(active_document(&app, cx), app, "the dirty tab closed");
}

#[gpui::test]
fn unsaved_changes_buttons_follow_tab_and_click(cx: &mut TestAppContext) {
    let (app, cx) = ask_to_close(cx);
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
    assert_eq!(active_document(&app, cx), app);
}

#[gpui::test]
fn unsaved_changes_enter_saves_after_a_click_on_the_text(cx: &mut TestAppContext) {
    let (_, cx) = ask_to_close(cx);
    let text = cx.debug_bounds("confirm-dialog").unwrap().origin + gpui::point(px(30.), px(30.));
    cx.simulate_click(text, gpui::Modifiers::none());
    cx.simulate_keystrokes("enter");
    cx.run_until_parked();
    // Save on an untitled tab asks where to save: the dialog answered Save.
    assert!(cx.update(|window, _| !window.has_active_prompt()));
    assert!(cx.did_prompt_for_new_path());
}

#[gpui::test]
fn files_opened_during_a_confirm_dialog_wait_for_the_answer(cx: &mut TestAppContext) {
    let (app, cx) = ask_to_close(cx);
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
        crate::confirm_dialog::install(
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
    // The open starts resolving, then the close asks before it finishes.
    app.update(cx, |_, cx| {
        cx.emit(workspace::TabEvent::Tabs(workspace::TabsEvent::Open(vec![
            path,
        ])))
    });
    app.update(cx, |_, cx| {
        cx.emit(workspace::TabEvent::Close(workspace::CloseEvent::Requested))
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
