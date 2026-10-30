//! Closing and quitting with unsaved changes (`QuitFlow`).
use super::*;

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
    cx.simulate_prompt_answer("Discard");
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
fn system_quit_target(
    cx: &mut VisualTestContext,
) -> (gpui::WindowHandle<Workspace>, gpui::AsyncApp) {
    cx.update(|window, cx| {
        (
            window.window_handle().downcast::<Workspace>().unwrap(),
            cx.to_async(),
        )
    })
}

#[gpui::test]
fn system_quit_asks_like_quit_and_terminates_only_after(cx: &mut TestAppContext) {
    use crate::system_quit::QuitGate;
    let (tabs, cx) = boot(cx, Session::default());
    cx.simulate_input("unsaved");
    let gate = QuitGate::default();
    // The Dock's Quit: AppKit is told no, and Quit's question comes up.
    assert!(!gate.should_terminate(Some(system_quit_target(cx))));
    cx.run_until_parked();
    assert!(cx.update(|_, cx| tabs.read(cx).quit_flow.quitting()));
    cx.simulate_prompt_answer("Cancel");
    cx.run_until_parked();
    let view = active(&tabs, cx);
    cx.update(|_, cx| {
        assert!(!tabs.read(cx).quit_flow.quitting());
        assert!(view.read(cx).dirty(cx), "Cancel keeps the edit");
    });
    assert!(!cx.cx.read(|cx| cx.windows().is_empty()));
    // Again, and Discard: the quit flow closes the window.
    assert!(!gate.should_terminate(Some(system_quit_target(cx))));
    cx.run_until_parked();
    cx.simulate_prompt_answer("Discard");
    cx.run_until_parked();
    assert!(cx.cx.read(|cx| cx.windows().is_empty()));
    // mdoc's own quit then opens the gate, so its terminate goes through.
    gate.open();
    assert!(gate.should_terminate(None));
}

#[gpui::test]
fn system_quit_without_changes_closes_without_asking(cx: &mut TestAppContext) {
    let (_tabs, cx) = boot(cx, Session::default());
    let gate = crate::system_quit::QuitGate::default();
    assert!(!gate.should_terminate(Some(system_quit_target(cx))));
    cx.run_until_parked();
    assert!(!cx.has_pending_prompt());
    assert!(cx.cx.read(|cx| cx.windows().is_empty()));
    // Before the window opens there is nothing to lose.
    assert!(crate::system_quit::QuitGate::default().should_terminate(None));
}
