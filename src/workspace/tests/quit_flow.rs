//! Closing and quitting with unsaved changes (`QuitFlow`).
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
fn quit_cancel_keeps_all_tabs_and_save_as_collision_never_overwrites(cx: &mut TestAppContext) {
    let dir = tempfile::tempdir().unwrap();
    let note = dir.path().join("saved.md");
    std::fs::write(&note, "disk original").unwrap();
    let (tabs, cx) = boot(
        cx,
        Session {
            tabs: vec![TabRecord {
                markdown: note.clone(),
                ..TabRecord::default()
            }],
            ..Session::default()
        },
    );
    cx.dispatch_action(New);
    cx.run_until_parked();
    cx.simulate_input("unsaved");
    cx.dispatch_action(SaveAs);
    cx.run_until_parked();
    cx.simulate_new_path_selection(|_| Some(note.clone()));
    cx.run_until_parked();
    cx.simulate_prompt_answer("Cancel");
    cx.run_until_parked();
    assert_eq!(std::fs::read_to_string(note).unwrap(), "disk original");
    cx.dispatch_action(New);
    cx.run_until_parked();
    cx.simulate_input("other unsaved");
    cx.dispatch_action(Quit);
    cx.run_until_parked();
    cx.simulate_prompt_answer("Review…");
    cx.run_until_parked();
    cx.simulate_prompt_answer("Don't Save");
    cx.run_until_parked();
    cx.simulate_prompt_answer("Cancel");
    cx.run_until_parked();
    cx.update(|_, cx| {
        let tabs = tabs.read(cx);
        assert_eq!(tabs.tabs.len(), 3);
        assert!(!tabs.quit_flow.quitting());
        assert_eq!(
            tabs.tabs
                .iter()
                .filter(|tab| tab.view().unwrap().read(cx).dirty(cx))
                .count(),
            2
        );
    });
}

#[gpui::test]
fn production_shell_queues_startup_opens_and_close_hook_keeps_dirty_tabs(cx: &mut TestAppContext) {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("startup.md");
    std::fs::write(&path, "startup").unwrap();
    cx.update(mdoc_editor::bind_keys);
    cx.update(bind_app_actions);
    let (tabs, cx) = cx.add_window_view(|window, cx| {
        let mut tabs = Workspace::new(window, cx);
        tabs.open_path(path, window, cx);
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
    cx.simulate_prompt_answer("Cancel");
    cx.run_until_parked();
    cx.update(|_, cx| assert!(view.read(cx).dirty(cx)));
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
fn system_quit_asks_like_quit_and_answers_appkit(cx: &mut TestAppContext) {
    use crate::system_quit::Answer;
    let (tabs, cx) = boot(cx, Session::default());
    cx.simulate_input("unsaved");
    let (gate, replies) = system_quit_gate(cx);
    // The Dock's Quit: AppKit waits, and Quit's question comes up.
    assert_eq!(
        gate.should_terminate(Some(system_quit_target(cx))),
        Answer::Later
    );
    cx.run_until_parked();
    assert!(cx.update(|_, cx| tabs.read(cx).quit_flow.quitting()));
    assert!(gate.waiting());
    // Cancel refuses the terminate and keeps the edit.
    cx.simulate_prompt_answer("Cancel");
    cx.run_until_parked();
    assert_eq!(*replies.borrow(), [false]);
    let view = active(&tabs, cx);
    cx.update(|_, cx| {
        assert!(!tabs.read(cx).quit_flow.quitting());
        assert!(view.read(cx).dirty(cx), "Cancel keeps the edit");
    });
    assert!(!cx.cx.read(|cx| cx.windows().is_empty()));
    // Again, and Discard: the quit flow closes the window, which answers yes.
    assert_eq!(
        gate.should_terminate(Some(system_quit_target(cx))),
        Answer::Later
    );
    cx.run_until_parked();
    cx.simulate_prompt_answer("Don't Save");
    cx.run_until_parked();
    assert!(cx.cx.read(|cx| cx.windows().is_empty()));
    assert_eq!(*replies.borrow(), [false, true]);
    assert!(!gate.waiting());
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
