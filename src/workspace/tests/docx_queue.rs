//! DOCX conversions waiting for the preview worker (`DocxQueue`).
use super::*;

#[gpui::test]
fn queued_docx_jobs_are_serial_and_closed_tabs_cannot_receive_results(cx: &mut TestAppContext) {
    let (tabs, cx) = boot(cx, Session::default());
    let base = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let source = base.join("tests/fixtures/docx-preview/comments.docx");
    let first = active(&tabs, cx);
    let first_id = cx.update(|_, cx| tabs.read(cx).tabs.active());
    // Admit both jobs in one update before the background executor runs.
    tabs.update_in(cx, |tabs, window, cx| {
        first.update(cx, |view, cx| view.open_docx(source.clone(), window, cx));
        tabs.new_tab(window, cx);
        tabs.active_view()
            .unwrap()
            .update(cx, |view, cx| view.open_docx(source, window, cx));
    });
    cx.run_until_parked();
    cx.update(|_, cx| {
        let tabs = tabs.read(cx);
        assert!(tabs.docx_queue.idle());
        assert!(first.read(cx).preview.pdf().is_some());
        assert!(tabs.active_view().unwrap().read(cx).preview.pdf().is_some());
    });
    let (weak, backing) = cx.update(|_, cx| {
        let view = first.read(cx);
        (
            view.preview.pdf().unwrap().downgrade(),
            view.preview.docx().unwrap().pdf_path.clone(),
        )
    });
    drop(first);
    tabs.update_in(cx, |tabs, window, cx| tabs.remove(first_id, window, cx));
    cx.run_until_parked();
    assert!(crate::test_support::removed(&backing));
    cx.update(|_, _| {
        assert!(
            weak.upgrade().is_none(),
            "closed PDF entity must be released"
        )
    });
}

#[gpui::test]
fn a_closed_tab_s_docx_job_ending_frees_the_worker(cx: &mut TestAppContext) {
    // A closed document can't emit; the window waits on its running jobs.
    let (tabs, cx) = boot(cx, Session::default());
    let id = cx.update(|_, cx| tabs.read(cx).tabs.active());
    tabs.update_in(cx, |tabs, window, cx| {
        tabs.new_tab(window, cx);
        tabs.docx_queue.occupy(id, 4);
        let view = tabs
            .tabs
            .get(id)
            .and_then(|tab| tab.view().cloned())
            .unwrap();
        view.update(cx, |view, cx| {
            let docx = cx
                .background_executor()
                .spawn(async { crate::document_view::Background::Docx { generation: 4 } });
            view.keep_background(docx);
        });
        tabs.remove(id, window, cx);
        assert!(!tabs.docx_queue.idle(), "the job has not ended yet");
    });
    cx.run_until_parked();
    cx.update(|_, cx| assert!(tabs.read(cx).docx_queue.idle()));
}

#[gpui::test]
fn closing_queued_conversion_rejects_it_without_affecting_other_tabs(cx: &mut TestAppContext) {
    let (tabs, cx) = boot(cx, Session::default());
    let cancelled = Arc::new(AtomicBool::new(false));
    let id = cx.update(|_, cx| tabs.read(cx).tabs.active());
    tabs.update_in(cx, |tabs, window, cx| {
        tabs.docx_queue.occupy(999, 1);
        let view = tabs.active_view().unwrap();
        view.update(cx, |view, cx| {
            view.open_docx("queued.docx".into(), window, cx)
        });
        tabs.event(
            id,
            &TabEvent::Docx(DocxEvent::Queue {
                path: "queued.docx".into(),
                generation: 1,
                cancel: cancelled.clone(),
            }),
            window,
            cx,
        );
        tabs.remove(id, window, cx);
        tabs.docx_finished(999, 1, window, cx);
        assert!(tabs.docx_queue.idle());
    });
    cx.run_until_parked();
    cx.update(|_, cx| {
        assert_eq!(tabs.read(cx).tabs.len(), 1);
        assert!(
            tabs.read(cx)
                .active_view()
                .unwrap()
                .read(cx)
                .preview
                .source()
                .is_none()
        );
    });
}

#[gpui::test]
fn a_docx_source_notes_its_approximate_preview_without_a_notice(cx: &mut TestAppContext) {
    let (tabs, cx) = boot(cx, Session::default());
    cx.simulate_resize(size(px(1400.), px(800.)));
    let docx =
        PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/docx-preview/comments.docx");
    tabs.update_in(cx, |tabs, window, cx| tabs.open_path(docx, window, cx));
    cx.run_until_parked();
    draw(cx);
    draw(cx);
    // The expected limitation sits by the Source name; the bell stays for
    // real notices (README media decision 9; ADR 0048 names the pane Source).
    cx.update(|_, cx| {
        let view = tabs.read(cx).active_view().unwrap();
        let pdf = view.read(cx).preview.pdf().cloned().unwrap();
        let pdf = pdf.read(cx);
        assert_eq!(pdf.display_name(), Some("Source · comments.docx"));
        assert_eq!(
            pdf.source_note().map(|note| note.as_ref()),
            Some(crate::preview::DOCX_NOTE)
        );
    });
    assert!(cx.debug_bounds("header-notices").is_none());
    cx.update(|_, cx| assert_eq!(tabs.read(cx).title.notices.get(), 0));
}
