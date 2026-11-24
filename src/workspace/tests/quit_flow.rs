//! Quitting: documents are written, then the session, then the window
//! closes (`QuitFlow`).
use super::*;
use std::cell::RefCell;

#[gpui::test]
fn ocr_quit_during_setup_download(cx: &mut TestAppContext) {
    quit_during_setup_download(cx, settings::Model::Ocr(settings::OcrModel::V6Small));
}

#[gpui::test]
fn pii_quit_during_setup_download(cx: &mut TestAppContext) {
    quit_during_setup_download(cx, settings::Model::Pii(settings::PiiModel::Fp16));
}

#[gpui::test]
fn a_quit_writes_every_draft_and_asks_nothing(cx: &mut TestAppContext) {
    let (tabs, cx) = boot(cx, Session::default());
    let vault = cx.update(|_, cx| tabs.read(cx).opened_vault().unwrap());
    cx.simulate_input("first draft");
    cx.dispatch_action(New);
    cx.run_until_parked();
    cx.simulate_input("second draft");
    cx.dispatch_action(Quit);
    cx.run_until_parked();
    assert!(!cx.has_pending_prompt());
    assert!(cx.cx.read(|cx| cx.windows().is_empty()));
    let mut texts: Vec<_> = vault
        .documents()
        .iter()
        .map(|summary| vault.document(&summary.id).unwrap().markdown)
        .collect();
    texts.sort();
    assert_eq!(texts, ["first draft", "second draft"]);
}

/// Review of #1145 (P2-4): a keystroke accepted while a quit's last
/// writes run is written too before the window closes: in the text, and
/// (round 2, P2) in Notes and the reply box.
#[gpui::test]
fn a_keystroke_during_the_quits_last_write_is_written(cx: &mut TestAppContext) {
    let (tabs, cx) = boot(cx, Session::default());
    let vault = cx.update(|_, cx| tabs.read(cx).opened_vault().unwrap());
    cx.simulate_input("first");
    wait(cx, 1000);
    assert_eq!(vault.documents().len(), 1, "created by its first save");
    cx.simulate_input(" second");
    tabs.update_in(cx, |tabs, window, cx| {
        tabs.quit(window, cx);
        // The quit's write of " second" is queued, not run yet.
        let view = tabs.active_view().unwrap();
        view.update(cx, |view, cx| {
            view.editor.update(cx, |editor, cx| {
                let more = format!("{} third", editor.text());
                editor.set_text(more, cx);
            })
        });
        // Notes wait for their pause, and the box's reply for its own.
        view.update(cx, |view, cx| {
            view.type_notes("Call back.", window, cx);
            view.fill_reply("A late reply.", window, cx);
        });
    });
    cx.run_until_parked();
    assert!(cx.cx.read(|cx| cx.windows().is_empty()));
    let id = &vault.documents()[0].id;
    let document = vault.document(id).unwrap();
    assert_eq!(document.markdown, "first second third");
    assert_eq!(document.notes, "Call back.");
    let replies = vault.replies();
    assert_eq!(replies.len(), 1, "the box's reply is saved");
    assert_eq!(
        vault.reply(&replies[0].id).unwrap().markdown,
        "A late reply."
    );
}

/// Review of #1152 (P1): notes typed while a quit's last write runs, whose
/// own write then fails, cancel the quit: the window stays with them.
#[gpui::test]
fn a_failed_late_notes_write_cancels_the_quit(cx: &mut TestAppContext) {
    use std::sync::atomic::AtomicUsize;
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path().join("vault");
    // Armed, the Vault takes one Document write and fails the next.
    let armed = Arc::new(AtomicBool::new(false));
    let writes = Arc::new(AtomicUsize::new(0));
    let hook: mdoc_vault::FaultHook = {
        let (armed, writes) = (armed.clone(), writes.clone());
        Arc::new(move |step| {
            let document = step.kind == mdoc_vault::ObjectKind::Document
                && step.stage == mdoc_vault::Stage::BeforeRename;
            if armed.load(Ordering::SeqCst)
                && document
                && writes.fetch_add(1, Ordering::SeqCst) >= 1
            {
                mdoc_vault::Fault::Crash
            } else {
                mdoc_vault::Fault::Continue
            }
        })
    };
    let (tabs, cx) = boot_home_with(cx, Session::default(), move |tabs, _, _| {
        tabs.use_vault(mdoc_vault::Vault::open_with_hook(root, &[3; 32], hook).unwrap())
    });
    tabs.update_in(cx, |tabs, window, cx| tabs.new_tab(window, cx));
    cx.run_until_parked();
    cx.simulate_input("first");
    wait(cx, 1000);
    cx.simulate_input(" second");
    armed.store(true, Ordering::SeqCst);
    tabs.update_in(cx, |tabs, window, cx| {
        tabs.quit(window, cx);
        // Only the notes change while the quit writes the text.
        let view = tabs.active_view().unwrap();
        view.update(cx, |view, cx| view.type_notes("Call back.", window, cx));
    });
    cx.run_until_parked();
    assert!(
        !cx.cx.read(|cx| cx.windows().is_empty()),
        "the window stays"
    );
    cx.update(|_, cx| {
        let tabs = tabs.read(cx);
        assert!(!tabs.quit_flow.finishing());
        let view = tabs.active_view().unwrap();
        assert_eq!(view.read(cx).notes.text(), "Call back.");
        let notice = tabs.notices.message().unwrap();
        assert!(notice.contains("notes"), "{notice}");
    });
}

/// Review of #1152 (P2), under ADR 0130: a reply filled in the box while a
/// move waits is saved before the window closes, with the aliases the move
/// gave its document: the quit is the last job, after the move and its
/// reload.
#[gpui::test]
fn a_quit_after_a_waiting_move_saves_the_box_reply_in_the_moved_names(cx: &mut TestAppContext) {
    use mdoc_pii::Category::Person;
    let dir = tempfile::tempdir().unwrap();
    let (tabs, cx) = boot(cx, Session::default());
    let other = open_applied(
        &tabs,
        cx,
        dir.path(),
        "b.md",
        "Boris Ivanov signed.",
        &[("Boris Ivanov", Person)],
    );
    let to = new_project(&tabs, cx, other);
    let tab = open_applied(
        &tabs,
        cx,
        dir.path(),
        "a.md",
        "Anna Petrova wrote.",
        &[("Anna Petrova", Person)],
    );
    wait(cx, 1000);
    let vault = cx.update(|_, cx| tabs.read(cx).opened_vault().unwrap());
    tabs.update_in(cx, |tabs, window, cx| {
        let view = tabs.tabs.get(tab).unwrap().view().unwrap().clone();
        // Its own write runs off the main thread; the move waits behind it.
        view.update(cx, |view, cx| {
            view.editor.update(cx, |editor, cx| {
                let later = format!("{} Later.", editor.text());
                editor.set_text(later, cx);
            })
        });
        tabs.autosave(tab, window, cx);
        tabs.move_tab(tab, Some(to.clone()), window, cx);
        // In its Unfiled aliases: PERSON_1 is Anna.
        view.update(cx, |view, cx| {
            view.fill_reply("PERSON_1 replied.", window, cx)
        });
        tabs.quit(window, cx);
    });
    cx.run_until_parked();
    assert!(cx.cx.read(|cx| cx.windows().is_empty()));
    let replies = vault.replies();
    assert_eq!(replies.len(), 1, "the box's reply is saved");
    assert_eq!(
        vault.restore_reply(&replies[0].id).unwrap().text,
        "Anna Petrova replied."
    );
}

#[gpui::test]
fn production_shell_queues_startup_opens_and_a_quit_writes_the_edit(cx: &mut TestAppContext) {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("startup.md");
    std::fs::write(&path, "startup").unwrap();
    cx.update(mdoc_editor::bind_keys);
    cx.update(bind_app_actions);
    let (tabs, cx) = cx.add_window_view(|window, cx| {
        let mut tabs = Workspace::new(window, cx);
        tabs.open_path(path.clone(), window, cx);
        tabs
    });
    cx.run_until_parked();
    let view = active(&tabs, cx);
    cx.update(|_, cx| {
        assert_eq!(view.read(cx).editor.read(cx).text(), "startup");
        assert!(!view.read(cx).ocr_state.busy());
    });
    cx.simulate_input(" edited");
    cx.dispatch_action(Quit);
    cx.run_until_parked();
    assert!(!cx.has_pending_prompt());
    assert!(cx.cx.read(|cx| cx.windows().is_empty()));
}

#[gpui::test]
fn failed_final_checkpoint_can_cancel_quit_without_losing_tabs(cx: &mut TestAppContext) {
    let dir = tempfile::tempdir().unwrap();
    let parent_file = dir.path().join("not-a-directory");
    std::fs::write(&parent_file, "preserve").unwrap();
    let (tabs, cx) = boot(cx, Session::default());
    tabs.update(cx, |tabs, _| {
        tabs.session_writer
            .set_path(Some(parent_file.join("session.json")))
    });
    cx.dispatch_action(Quit);
    cx.run_until_parked();
    cx.simulate_prompt_answer("Cancel");
    cx.run_until_parked();
    cx.update(|_, cx| {
        assert_eq!(tabs.read(cx).tabs.len(), 1);
        assert!(!tabs.read(cx).quit_flow.finishing());
        assert!(tabs.read(cx).notices.message().is_some());
    });
    assert_eq!(std::fs::read_to_string(parent_file).unwrap(), "preserve");
}

/// The workspace window behind `cx`, as the macOS terminate hook sees it.
fn system_quit_target(cx: &mut VisualTestContext) -> crate::system_quit::QuitTarget {
    cx.update(|window, cx| {
        (
            window.window_handle().downcast::<Workspace>().unwrap(),
            cx.to_async(),
        )
    })
}

/// A gate installed as the app's, recording AppKit's deferred answers, and
/// closing the app's last window answers it as main.rs does.
fn system_quit_gate(
    cx: &mut VisualTestContext,
) -> (crate::system_quit::QuitGate, Rc<RefCell<Vec<bool>>>) {
    let replies = Rc::new(RefCell::new(Vec::new()));
    let gate = crate::system_quit::QuitGate::new({
        let replies = replies.clone();
        move |terminate| replies.borrow_mut().push(terminate)
    });
    cx.update(|_, cx| {
        cx.set_global(gate.clone());
        let gate = gate.clone();
        cx.on_window_closed(move |cx, _| {
            if cx.windows().is_empty() {
                gate.last_window_closed();
            }
        })
        .detach();
    });
    (gate, replies)
}

#[gpui::test]
fn system_quit_writes_the_edit_and_answers_appkit(cx: &mut TestAppContext) {
    use crate::system_quit::Answer;
    let (tabs, cx) = boot(cx, Session::default());
    let vault = cx.update(|_, cx| tabs.read(cx).opened_vault().unwrap());
    cx.simulate_input("unsaved");
    let (gate, replies) = system_quit_gate(cx);
    // The Dock's Quit: AppKit waits while the edit is written, then the
    // window closes, which answers yes.
    assert_eq!(
        gate.should_terminate(Some(system_quit_target(cx))),
        Answer::Later
    );
    cx.run_until_parked();
    assert!(!cx.has_pending_prompt());
    assert!(cx.cx.read(|cx| cx.windows().is_empty()));
    assert_eq!(*replies.borrow(), [true]);
    assert!(!gate.waiting());
    let summary = &vault.documents()[0];
    assert_eq!(vault.document(&summary.id).unwrap().markdown, "unsaved");
}

#[gpui::test]
fn system_quit_without_changes_closes_without_asking(cx: &mut TestAppContext) {
    use crate::system_quit::{Answer, QuitGate};
    let (_tabs, cx) = boot(cx, Session::default());
    let (gate, replies) = system_quit_gate(cx);
    assert_eq!(
        gate.should_terminate(Some(system_quit_target(cx))),
        Answer::Later
    );
    cx.run_until_parked();
    assert!(!cx.has_pending_prompt());
    assert!(cx.cx.read(|cx| cx.windows().is_empty()));
    // A clean session lets a logout or shutdown that asked go on.
    assert_eq!(*replies.borrow(), [true]);
    // Before the window opens there is nothing to lose.
    assert_eq!(QuitGate::new(|_| {}).should_terminate(None), Answer::Now);
}

#[gpui::test]
fn own_quit_opens_the_gate(cx: &mut TestAppContext) {
    use crate::system_quit::{Answer, QuitGate};
    let (_tabs, cx) = boot(cx, Session::default());
    let gate = QuitGate::new(|_| panic!("nothing deferred"));
    // ⌘Q closes the last window: main.rs then quits, and that terminate
    // goes through at once.
    assert!(!gate.last_window_closed());
    assert_eq!(
        gate.should_terminate(Some(system_quit_target(cx))),
        Answer::Now
    );
}

#[gpui::test]
fn system_quit_waits_through_a_failed_final_save(cx: &mut TestAppContext) {
    use crate::system_quit::Answer;
    let dir = tempfile::tempdir().unwrap();
    let parent_file = dir.path().join("not-a-directory");
    std::fs::write(&parent_file, "preserve").unwrap();
    let (tabs, cx) = boot(cx, Session::default());
    tabs.update(cx, |tabs, _| {
        tabs.session_writer
            .set_path(Some(parent_file.join("session.json")))
    });
    let (gate, replies) = system_quit_gate(cx);
    // Nothing to ask about, but the final save fails: its question shows
    // while AppKit still waits, so it comes to the front (confirm_dialog).
    assert_eq!(
        gate.should_terminate(Some(system_quit_target(cx))),
        Answer::Later
    );
    cx.run_until_parked();
    assert!(cx.has_pending_prompt());
    assert!(cx.update(|_, cx| crate::system_quit::waiting(cx)));
    cx.simulate_prompt_answer("Cancel");
    cx.run_until_parked();
    assert_eq!(*replies.borrow(), [false]);
    // Again, and quit with the previous session list: the terminate goes on.
    assert_eq!(
        gate.should_terminate(Some(system_quit_target(cx))),
        Answer::Later
    );
    cx.run_until_parked();
    cx.simulate_prompt_answer("Quit without updating session");
    cx.run_until_parked();
    assert!(cx.cx.read(|cx| cx.windows().is_empty()));
    assert_eq!(*replies.borrow(), [false, true]);
}

/// Review of #1175 (P2): Always and Never lists changed while a quit waits
/// are saved before the window closes: the quit stays the last job.
#[gpui::test]
fn a_list_changed_while_a_quit_waits_is_saved_before_it_closes(cx: &mut TestAppContext) {
    let (tabs, cx) = boot(cx, Session::default());
    let vault = cx.update(|_, cx| tabs.read(cx).opened_vault().unwrap());
    cx.simulate_input("first");
    wait(cx, 1000);
    cx.simulate_input(" second");
    tabs.update_in(cx, |tabs, window, cx| {
        // A save runs off the main thread; the quit waits behind it.
        let tab = tabs.tabs.active();
        tabs.autosave(tab, window, cx);
        tabs.quit(window, cx);
        let mut lists = mdoc_store::lists::Lists::default();
        lists.add_never("Supreme Court");
        tabs.save_user_lists(lists, window, cx);
    });
    cx.run_until_parked();
    assert!(cx.cx.read(|cx| cx.windows().is_empty()));
    assert!(vault.user_lists().unwrap().lists.never_has("Supreme Court"));
}
