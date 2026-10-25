use crate::{
    bind_app_keys,
    document_view::{
        ClosePdf, CopyMarkdown, DismissImportWarning, DocumentView, Next, OcrState, Save, SaveAs,
        bind_markdown_search_keys, find_counter,
    },
    import, markdown_search, ocr,
    pii::{self, ui::PiiApplyAll},
    preview::{self, RetryPreview},
    search_session::{
        CloseMarkdownSearch, FindMarkdown, FindNextMarkdown, FindPreviousMarkdown,
        FindReplaceMarkdown, ReplaceNextMarkdown, map_edit_offset,
    },
    session_store, settings, settings_ui,
    spelling::{NextMisspelling, PreviousMisspelling, ToggleDocumentSpelling},
    style::{self, Theme},
    ui,
    workspace::{self, Close, Import, New, Open},
};
use gpui::{Entity, Focusable, TestAppContext, VisualTestContext, px, size};
use std::{path::PathBuf, sync::atomic::Ordering};

pub(super) fn boot(cx: &mut TestAppContext) -> (Entity<DocumentView>, &mut VisualTestContext) {
    cx.update(mdoc_editor::bind_keys);
    cx.update(ui::bind_keys);
    cx.update(gpui_pdf::bind_keys);
    cx.update(workspace::bind_app_actions);
    cx.update(markdown_search::bind_keys);
    cx.update(pii::ui::bind_keys);
    cx.update(settings_ui::bind_keys);
    // After the global ⌘↩ (Apply all), as in main, so find fields win.
    cx.update(bind_markdown_search_keys);
    let (tabs, cx) = cx.add_window_view(|window, cx| {
        let mut tabs = workspace::Workspace::empty(window, cx);
        tabs.restore(session_store::Session::default(), window, cx);
        // Start from an Untitled tab rather than Home (ADR 0048).
        tabs.new_tab(window, cx);
        tabs
    });
    cx.run_until_parked();
    let app = cx.update(|_, cx| tabs.read(cx).active_view().unwrap());
    (app, cx)
}

pub(super) fn active_document(
    app: &Entity<DocumentView>,
    cx: &mut VisualTestContext,
) -> Entity<DocumentView> {
    cx.update(|_, cx| {
        let tabs = app.read(cx).owner.1.upgrade().unwrap();
        tabs.read(cx).active_view().unwrap()
    })
}

pub(super) fn open_document(
    app: &Entity<DocumentView>,
    path: PathBuf,
    cx: &mut VisualTestContext,
) -> Entity<DocumentView> {
    app.update(cx, |_, cx| cx.emit(workspace::TabEvent::Open(vec![path])));
    cx.run_until_parked();
    active_document(app, cx)
}

pub(super) fn new_document(
    app: &Entity<DocumentView>,
    cx: &mut VisualTestContext,
) -> Entity<DocumentView> {
    app.update(cx, |_, cx| cx.emit(workspace::TabEvent::New));
    cx.run_until_parked();
    active_document(app, cx)
}

pub(super) fn close_document(
    app: &Entity<DocumentView>,
    cx: &mut VisualTestContext,
) -> Entity<DocumentView> {
    app.update(cx, |_, cx| cx.emit(workspace::TabEvent::CloseRequested));
    cx.run_until_parked();
    if cx.has_pending_prompt() {
        cx.simulate_prompt_answer("Discard");
        cx.run_until_parked();
    }
    active_document(app, cx)
}

#[gpui::test]
fn pseudonymization_group_accept_undo_save_and_identity_reset(cx: &mut TestAppContext) {
    use crate::pii::{Category, Detection};
    let dir = tempfile::tempdir().unwrap();
    let source_path = dir.path().join("legal.md");
    let source = "# Contract\n\nAlice Morgan represents **Alice Morgan**. [contact](https://x.invalid/Alice_Morgan)\n";
    std::fs::write(&source_path, source).unwrap();
    let source_path = session_store::identity(&source_path);
    let (app, cx) = boot(cx);
    let app = open_document(&app, source_path.clone(), cx);
    app.update_in(cx, |app, window, cx| {
        app.session.warning = Some("Review extraction".into());
        app.pii.reviewing = true;
        app.pii.review.ingest(source, vec![Detection { range: 12..24, category: Category::Person, score: 0.9, recognizer: crate::pii::Recognizer::Model }]).unwrap();
        app.review_changed(cx);
        let id = app.pii.review.variants()[0].id;
        let range=app.pii.review.variant(id).unwrap().mentions[0].clone();
        let annotation=app.pii.review.annotation_id(id,&range).unwrap();
        app.activate_annotation(annotation,window,cx);
        app.apply_all_pii(&PiiApplyAll, window, cx);
        assert_eq!(app.editor.read(cx).text(), "# Contract\n\nPERSON_1 represents **PERSON_1**. [contact](https://x.invalid/Alice_Morgan)\n");
        assert_eq!(app.pii.review.remaining(), 0);
        assert_eq!(app.session.document.path.as_ref(), Some(&source_path));
        assert!(app.session.warning.is_some());
        assert!(app.dirty(cx));
        app.copy_markdown(&CopyMarkdown, window, cx);
        assert_eq!(cx.read_from_clipboard().unwrap().text().unwrap(), app.editor.read(cx).text());
        window.focus(&app.editor.read(cx).focus_handle(cx), cx);
    });
    cx.dispatch_action(mdoc_editor::Undo);
    cx.run_until_parked();
    app.update_in(cx, |app, window, cx| {
        assert_eq!(app.editor.read(cx).text(), source);
        assert_eq!(app.pii.review.remaining(), 2);
        assert!(!app.dirty(cx));
        let id = app.pii.review.variants()[0].id;
        let range = app.pii.review.variant(id).unwrap().mentions[0].clone();
        let annotation = app
            .pii
            .review
            .annotation_id(id, &range)
            .unwrap();
        app.activate_annotation(annotation, window, cx);
        app.pii.review.keep(id, Some(range));
        app.apply_all_pii(&PiiApplyAll, window, cx);
        assert_eq!(app.pii.review.remaining(), 0);
        assert_eq!(app.editor.read(cx).text(), "# Contract\n\nAlice Morgan represents **PERSON_1**. [contact](https://x.invalid/Alice_Morgan)\n");
        let text = app.editor.read(cx).text().to_owned();
        app.session
            .save(dir.path().join("prepared.md"), &text)
            .unwrap();
        assert_eq!(
            std::fs::read_to_string(dir.path().join("prepared.md")).unwrap(),
            text
        );
        assert_eq!(std::fs::read_to_string(&source_path).unwrap(), source);
        let generation = app.session.generation;
        app.proceed(
            Next::Import(converted(dir.path().join("replacement.docx"))),
            window,
            cx,
        );
        assert_ne!(app.session.generation, generation);
        assert!(app.pii.review.variants().is_empty());
        assert!(app.pii.popup.is_none());
    });
    let app = close_document(&app, cx);
    cx.update(|_, cx| {
        assert!(app.read(cx).pii.review.variants().is_empty());
        assert!(app.read(cx).pii.popup.is_none());
    });
}

#[gpui::test]
fn pseudonymization_keep_preserves_text_and_popup_edits_are_invalidated(cx: &mut TestAppContext) {
    use crate::pii::{Category, Detection};
    let (app, cx) = boot(cx);
    app.update_in(cx, |app, window, cx| {
        app.editor
            .update(cx, |editor, cx| editor.set_text("Alice Alice", cx));
        app.pii.reviewing = true;
        app.pii
            .review
            .ingest(
                "Alice Alice",
                vec![Detection {
                    range: 0..5,
                    category: Category::Person,
                    score: 0.9,
                    recognizer: crate::pii::Recognizer::Model,
                }],
            )
            .unwrap();
        let id = app.pii.review.variants()[0].id;
        let range = app.pii.review.variant(id).unwrap().mentions[0].clone();
        let annotation = app.pii.review.annotation_id(id, &range).unwrap();
        app.activate_annotation(annotation, window, cx);
        app.pii.review.keep(id, None);
        assert_eq!(app.editor.read(cx).text(), "Alice Alice");
        assert_eq!(app.pii.review.remaining(), 0);
        app.pii
            .review
            .add_manual("Alice Alice", 0..5, Category::Person)
            .unwrap();
        let range = app.pii.review.variant(id).unwrap().mentions[0].clone();
        let annotation = app.pii.review.annotation_id(id, &range).unwrap();
        app.activate_annotation(annotation, window, cx);
        app.editor.update(cx, |editor, cx| {
            let revision = editor.revision();
            editor.replace_ranges(revision, &[(0..5, "Betty".into())], cx);
        });
    });
    cx.run_until_parked();
    app.update(cx, |app, cx| {
        assert!(app.pii.popup.is_none());
        assert_eq!(app.editor.read(cx).text(), "Betty Alice");
        assert_eq!(app.pii.review.remaining(), 1);
    });
}

#[gpui::test]
fn copy_markdown_preserves_source_selection_undo_and_warning(cx: &mut TestAppContext) {
    use gpui::EntityInputHandler;
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("legal.md");
    let saved = "# Договор\r\n\r\n3. **Текст** [link](local.md)\r\n==Важно== <mark style='background:#ff0000'>условие</mark> <span style='color:blue'>сторона</span>\r\n\r\n```rust\r\nlet x = 1;\r\n```\r\n";
    std::fs::write(&path, saved).unwrap();
    let path = session_store::identity(&path);
    let (app, cx) = boot(cx);
    let app = open_document(&app, path.clone(), cx);
    app.update_in(cx, |app, _, cx| {
        app.session.warning = Some("Partial import: page 2 was skipped.".into());
        app.editor.update(cx, |editor, cx| {
            let end = editor.text().len();
            editor.replace_range(end..end, "Unsaved edit\n", cx);
            editor.set_cursor(2, cx);
        });
    });
    cx.dispatch_action(mdoc_editor::SelectRight);
    let selected = app.update_in(cx, |app, window, cx| {
        app.editor.update(cx, |editor, cx| {
            editor.selected_text_range(false, window, cx).unwrap().range
        })
    });
    let generation = cx.update(|_, cx| app.read(cx).session.generation);
    cx.dispatch_action(CopyMarkdown);
    cx.run_until_parked();
    app.update_in(cx, |app, window, cx| {
        let current = format!("{saved}Unsaved edit\n");
        assert_eq!(
            cx.read_from_clipboard().unwrap().text(),
            Some(current.clone())
        );
        assert!(cx.read_from_clipboard().unwrap().metadata().is_none());
        assert_eq!(app.editor.read(cx).text(), current);
        assert!(app.dirty(cx));
        assert_eq!(app.session.document.saved, saved);
        assert_eq!(app.session.document.path.as_ref(), Some(&path));
        assert_eq!(app.session.generation, generation);
        assert!(app.session.warning.as_ref().unwrap().contains("page 2"));
        assert!(app.copy_feedback.is_some());
        app.editor.update(cx, |editor, cx| {
            assert!(editor.focus_handle(cx).is_focused(window));
            assert_eq!(
                editor.selected_text_range(false, window, cx).unwrap().range,
                selected
            );
        });
    });
    assert_eq!(std::fs::read_to_string(path).unwrap(), saved);
    cx.dispatch_action(mdoc_editor::Undo);
    cx.run_until_parked();
    cx.update(|_, cx| {
        assert_eq!(app.read(cx).editor.read(cx).text(), saved);
        assert!(!app.read(cx).dirty(cx));
    });
}

#[gpui::test]
fn copy_markdown_feedback_restarts_expires_and_stays_with_its_tab(cx: &mut TestAppContext) {
    use std::time::Duration;
    let (app, cx) = boot(cx);
    cx.dispatch_action(CopyMarkdown);
    cx.run_until_parked();
    cx.update(|_, cx| {
        let item = cx.read_from_clipboard().unwrap();
        let [gpui::ClipboardEntry::String(text)] = item.entries() else {
            panic!("expected a single plain text entry");
        };
        assert!(text.text().is_empty());
        assert!(item.metadata().is_none());
    });
    cx.executor().advance_clock(Duration::from_secs(1));
    cx.dispatch_action(CopyMarkdown);
    cx.run_until_parked();
    cx.executor().advance_clock(Duration::from_secs(1));
    cx.run_until_parked();
    assert!(cx.update(|_, cx| app.read(cx).copy_feedback.is_some()));
    cx.executor().advance_clock(Duration::from_secs(1));
    cx.run_until_parked();
    assert!(cx.update(|_, cx| app.read(cx).copy_feedback.is_none()));
    cx.dispatch_action(CopyMarkdown);
    cx.run_until_parked();
    cx.simulate_input("x");
    cx.run_until_parked();
    assert!(cx.update(|_, cx| app.read(cx).copy_feedback.is_none()));
    let app = new_document(&app, cx);
    cx.dispatch_action(CopyMarkdown);
    cx.run_until_parked();
    cx.dispatch_action(New);
    cx.run_until_parked();
    let next = active_document(&app, cx);
    assert!(cx.update(|_, cx| next.read(cx).copy_feedback.is_none()));
    assert!(
        cx.update(|_, cx| app.read(cx).copy_feedback.is_some()),
        "the previous tab retains its own feedback"
    );
}

#[gpui::test]
fn copy_markdown_refuses_loading_unavailable_and_source_only_views(cx: &mut TestAppContext) {
    let (app, cx) = boot(cx);
    for state in 0..3 {
        app.update_in(cx, |app, window, cx| {
            app.loading = state == 0;
            app.unavailable = state == 1;
            app.source_only = state == 2;
            cx.write_to_clipboard(gpui::ClipboardItem::new_string("keep clipboard".into()));
            app.copy_markdown(&CopyMarkdown, window, cx);
            assert_eq!(
                cx.read_from_clipboard().unwrap().text().as_deref(),
                Some("keep clipboard")
            );
            assert!(app.copy_feedback.is_none());
        });
    }
}

#[gpui::test]
fn search_wraps_and_preserves_document_and_selection(cx: &mut TestAppContext) {
    let (app, cx) = boot(cx);
    app.update_in(cx, |app, window, cx| {
        app.session.document.saved = "alpha **alpha**".into();
        app.editor.update(cx, |editor, cx| {
            editor.set_text("alpha **alpha**", cx);
            editor.set_cursor(3, cx);
            editor.focus(window, cx);
        });
    });
    cx.dispatch_action(FindMarkdown);
    cx.run_until_parked();
    cx.simulate_input("alpha");
    cx.run_until_parked();
    assert_eq!(cx.update(|_, cx| app.read(cx).search.active), Some(1));
    cx.dispatch_action(FindNextMarkdown);
    cx.run_until_parked();
    assert_eq!(cx.update(|_, cx| app.read(cx).search.active), Some(0));
    cx.dispatch_action(FindPreviousMarkdown);
    cx.run_until_parked();
    assert_eq!(cx.update(|_, cx| app.read(cx).search.active), Some(1));
    assert_eq!(cx.update(|_, cx| app.read(cx).editor.read(cx).cursor()), 3);
    assert!(!cx.update(|_, cx| app.read(cx).dirty(cx)));
    assert_eq!(
        cx.update(|_, cx| app.read(cx).editor.read(cx).text().to_owned()),
        "alpha **alpha**"
    );
}

#[gpui::test]
fn search_reveals_last_wrapped_occurrence(cx: &mut TestAppContext) {
    let (app, cx) = boot(cx);
    app.update_in(cx, |app, _, cx| {
        let source = format!("{}needle", "a long paragraph with spaces ".repeat(1000));
        app.editor
            .update(cx, |editor, cx| editor.set_text(source, cx));
    });
    cx.dispatch_action(FindMarkdown);
    cx.run_until_parked();
    cx.simulate_input("needle");
    cx.run_until_parked();
    for _ in 0..3 {
        cx.update(|window, cx| {
            window.simulate_next_frame(cx);
        });
        cx.run_until_parked();
    }
    cx.update(|_, cx| {
        let app = app.read(cx);
        let bounds = app
            .editor
            .read(cx)
            .search_match_bounds(0)
            .expect("painted match");
        assert!(
            app.scroll.offset().y < px(0.),
            "bounds={bounds:?} viewport={:?} max={:?}",
            app.scroll.bounds(),
            app.scroll.max_offset()
        );
        assert!(bounds.top() >= app.scroll.bounds().top(), "{bounds:?}");
        assert!(
            bounds.bottom() <= app.scroll.bounds().bottom(),
            "{bounds:?}"
        );
    });
}

#[gpui::test]
fn automatic_table_cells_keep_readable_wrapping_when_original_is_toggled(cx: &mut TestAppContext) {
    let (app, cx) = boot(cx);
    cx.simulate_resize(gpui::size(px(2600.), px(1000.)));
    let left = "Contractor: ORG_1 General director ______________________________ PERSON_4, authorized representative of the company.";
    let right = "Заказчик: индивидуальный предприниматель PERSON_5 ______________________________ PERSON_6, уполномоченный представитель.";
    let source = format!(
        "<!-- table:grid cols=70,260,260 -->\n| Clause | Contractor | Customer |\n| --- | --- | --- |\n| 4.5.2 | {} | {} |\n\n| | |\n| --- | --- |\n| {left} | {right} |",
        "Payment after services. ".repeat(6),
        "Оплата после оказания услуг. ".repeat(6)
    );
    let first_start = source.find("Payment").unwrap();
    let first_end = first_start + "Payment after services. ".repeat(6).len() - 2;
    let left_start = source.find(left).unwrap();
    let right_start = source.find(right).unwrap();
    app.update(cx, |app, cx| {
        app.session.document.saved = source.clone();
        app.preview.source = Some(PathBuf::from("retained-original.docx"));
        app.preview.loading = true;
        app.preview.visible = true;
        app.editor.update(cx, |e, cx| {
            e.set_text(&source, cx);
            e.set_search(
                vec![
                    left_start..left_start + 1,
                    left_start + left.len() - 1..left_start + left.len(),
                    right_start..right_start + right.chars().next().unwrap().len_utf8(),
                    right_start + right.len() - 1..right_start + right.len(),
                    first_start..first_start + 1,
                    first_end..first_end + 1,
                ],
                None,
                cx,
            );
        });
        cx.notify();
    });
    cx.run_until_parked();
    let revision = cx.update(|_, cx| app.read(cx).editor.read(cx).revision());
    let mut saved_geometry = None;
    for visible in [true, false, true, false, true] {
        app.update_in(cx, |app, window, cx| {
            if app.preview.visible != visible {
                app.toggle_preview(window, cx);
            }
        });
        cx.run_until_parked();
        cx.update(|window, cx| {
            window.refresh();
            window.draw(cx).clear(cx);
        });
        cx.update(|_, cx| {
            let app = app.read(cx);
            let editor = app.editor.read(cx);
            assert_eq!(editor.text(), source);
            assert_eq!(editor.revision(), revision);
            assert!(!app.dirty(cx));
            for index in [0, 2] {
                let first = editor.search_match_bounds(index).expect("first cell glyph");
                let last = editor.search_match_bounds(index + 1).expect("last cell glyph");
                assert!(last.top() > first.top(), "automatic cells should retain readable wrapping with Original visible={visible}");
            }
            // A manually sized table retains its own text geometry in either pane width.
            let first = editor.search_match_bounds(4).unwrap();
            let last = editor.search_match_bounds(5).unwrap();
            let geometry = (last.left() - first.left(), last.top() - first.top());
            if let Some(saved) = saved_geometry {
                assert_eq!(geometry, saved);
            } else {
                saved_geometry = Some(geometry);
            }
        });
    }
}

#[gpui::test]
fn wrapped_table_search_uses_painted_cell_geometry(cx: &mut TestAppContext) {
    let (app, cx) = boot(cx);
    app.update_in(cx, |app, _, cx| {
        // Narrow explicit widths force the long cell to wrap inside the test
        // window. Without `cols=` the content-measured column is ~1000 chars
        // wide, so the match lies outside the visible band and highlight
        // clipping yields no bounds (pre-existing baseline failure).
        let source = format!(
            "<!-- table:grid cols=200,100 -->\n| heading | other |\n| --- | --- |\n| {}**needle** | end |",
            "long cell ".repeat(100)
        );
        app.editor
            .update(cx, |editor, cx| editor.set_text(source, cx));
    });
    cx.dispatch_action(FindMarkdown);
    cx.run_until_parked();
    cx.simulate_input("needle");
    cx.run_until_parked();
    for _ in 0..3 {
        cx.update(|window, cx| {
            window.simulate_next_frame(cx);
        });
        cx.run_until_parked();
    }
    cx.update(|_, cx| {
        let app = app.read(cx);
        let bounds = app
            .editor
            .read(cx)
            .search_match_bounds(0)
            .expect("table glyph bounds");
        assert!(bounds.size.width > px(0.));
        assert!(bounds.top() >= app.scroll.bounds().top());
        assert!(bounds.bottom() <= app.scroll.bounds().bottom());
    });
}

#[gpui::test]
fn offscreen_table_match_is_revealed_with_painted_geometry(cx: &mut TestAppContext) {
    let (app, cx) = boot(cx);
    app.update_in(cx, |app, _, cx| {
        // Offscreen matches get only a row box; the reveal must still land on
        // the cell glyphs once the row is painted.
        let source = format!(
            "{}<!-- table:grid cols=200,100 -->\n| heading | other |\n| --- | --- |\n| {}**needle** | end |",
            "A paragraph that pushes the table below the viewport.\n\n".repeat(200),
            "long cell ".repeat(40)
        );
        app.editor
            .update(cx, |editor, cx| editor.set_text(source, cx));
    });
    cx.dispatch_action(FindMarkdown);
    cx.run_until_parked();
    cx.simulate_input("needle");
    cx.run_until_parked();
    for _ in 0..3 {
        cx.update(|window, cx| {
            window.simulate_next_frame(cx);
        });
        cx.run_until_parked();
    }
    cx.update(|_, cx| {
        let app = app.read(cx);
        let editor = app.editor.read(cx);
        let bounds = editor.search_match_bounds(0).expect("table glyph bounds");
        assert!(app.scroll.offset().y < px(0.), "must scroll to the match");
        assert!(bounds.top() >= app.scroll.bounds().top(), "{bounds:?}");
        assert!(
            bounds.bottom() <= app.scroll.bounds().bottom(),
            "{bounds:?}"
        );
        let width = editor.layout_width().unwrap();
        assert!(
            bounds.size.width > px(0.) && bounds.size.width < width / 2.,
            "painted glyph quad, not the row box: {bounds:?}"
        );
    });
}

#[gpui::test]
fn search_preserves_selection_undo_and_save_as_state(cx: &mut TestAppContext) {
    use gpui::EntityInputHandler;
    let dir = tempfile::tempdir().unwrap();
    let (app, cx) = boot(cx);
    cx.simulate_input("alpha beta");
    cx.dispatch_action(mdoc_editor::SelectAll);
    let selected = app.update_in(cx, |app, window, cx| {
        app.editor.update(cx, |editor, cx| {
            editor.selected_text_range(false, window, cx).unwrap().range
        })
    });
    cx.dispatch_action(FindMarkdown);
    cx.run_until_parked();
    cx.simulate_input("alpha");
    cx.run_until_parked();
    let after = app.update_in(cx, |app, window, cx| {
        app.editor.update(cx, |editor, cx| {
            editor.selected_text_range(false, window, cx).unwrap().range
        })
    });
    assert_eq!(selected, after);
    cx.dispatch_action(SaveAs);
    cx.run_until_parked();
    cx.simulate_new_path_selection(|_| Some(dir.path().join("search.md")));
    cx.run_until_parked();
    assert!(cx.update(|_, cx| app.read(cx).search.open));
    assert_eq!(cx.update(|_, cx| app.read(cx).search.matches.len()), 1);
    cx.dispatch_action(CloseMarkdownSearch);
    cx.run_until_parked();
    cx.dispatch_action(mdoc_editor::Undo);
    cx.run_until_parked();
    assert!(cx.update(|_, cx| app.read(cx).editor.read(cx).text().is_empty()));
}

#[gpui::test]
fn pending_large_search_cannot_survive_tab_close(cx: &mut TestAppContext) {
    let (app, cx) = boot(cx);
    app.update_in(cx, |app, window, cx| {
        app.editor.update(cx, |editor, cx| {
            editor.set_text("alpha ".repeat(20_000), cx)
        });
        app.find_markdown(&FindMarkdown, window, cx);
    });
    let app = close_document(&app, cx);
    cx.run_until_parked();
    assert!(!cx.update(|_, cx| app.read(cx).search.open));
    assert!(cx.update(|_, cx| app.read(cx).search.matches.is_empty()));
    assert!(cx.update(|_, cx| app.read(cx).editor.read(cx).text().is_empty()));
}

#[gpui::test]
fn large_search_publishes_latest_query(cx: &mut TestAppContext) {
    let (app, cx) = boot(cx);
    app.update_in(cx, |app, _, cx| {
        app.editor.update(cx, |editor, cx| {
            editor.set_text("alpha beta\n".repeat(7_000), cx)
        });
    });
    cx.dispatch_action(FindMarkdown);
    cx.run_until_parked();
    // Opening an empty find bar must not build or schedule a large index.
    assert!(cx.update(|_, cx| app.read(cx).search.index.is_none()));
    assert!(cx.update(|_, cx| app.read(cx).search.task.is_none()));
    cx.simulate_input("alpha");
    cx.dispatch_action(FindMarkdown);
    cx.simulate_input("beta");
    cx.run_until_parked();
    assert_eq!(cx.update(|_, cx| app.read(cx).search.matches.len()), 7_000);
    assert!(cx.update(|_, cx| {
        let app = app.read(cx);
        app.search
            .matches
            .iter()
            .all(|m| &app.editor.read(cx).text()[m.source[0].clone()] == "beta")
    }));
}

#[test]
fn search_anchor_moves_after_insertion_at_occurrence_start() {
    assert_eq!(map_edit_offset("alpha beta", "alpha NEW beta", 6), 10);
    assert_eq!(map_edit_offset("я beta", "я 😀beta", 3), 7);
}

#[gpui::test]
fn markdown_search_is_live_and_keeps_the_editor_caret(cx: &mut TestAppContext) {
    let (app, cx) = boot(cx);
    app.update_in(cx, |app, window, cx| {
        app.editor.update(cx, |editor, cx| {
            editor.set_text("before **World** after\nWorld", cx);
            editor.set_cursor(4, cx);
            editor.focus(window, cx);
        });
    });
    cx.dispatch_action(FindMarkdown);
    cx.run_until_parked();
    cx.simulate_input("world");
    cx.run_until_parked();

    assert!(cx.update(|_, cx| app.read(cx).search.open));
    assert_eq!(cx.update(|_, cx| app.read(cx).search.matches.len()), 2);
    assert_eq!(cx.update(|_, cx| app.read(cx).editor.read(cx).cursor()), 4);
    assert_eq!(cx.update(|_, cx| app.read(cx).search.active), Some(0));
}

#[gpui::test]
fn closing_search_clears_highlights_but_retains_query(cx: &mut TestAppContext) {
    let (app, cx) = boot(cx);
    app.update_in(cx, |app, _, cx| {
        app.editor
            .update(cx, |editor, cx| editor.set_text("alpha beta", cx));
    });
    cx.dispatch_action(FindMarkdown);
    cx.run_until_parked();
    cx.simulate_input("beta");
    cx.run_until_parked();
    cx.dispatch_action(CloseMarkdownSearch);
    cx.run_until_parked();

    assert!(!cx.update(|_, cx| app.read(cx).search.open));
    assert_eq!(
        cx.update(|_, cx| app.read(cx).markdown_search.read(cx).value().to_owned()),
        "beta"
    );
    assert!(cx.update(|_, cx| app.read(cx).search.matches.is_empty()));
}

#[gpui::test]
fn accepted_document_transition_resets_search_state(cx: &mut TestAppContext) {
    let (app, cx) = boot(cx);
    app.update_in(cx, |app, _, cx| {
        app.editor
            .update(cx, |editor, cx| editor.set_text("alpha", cx));
    });
    cx.dispatch_action(FindMarkdown);
    cx.run_until_parked();
    cx.simulate_input("alpha");
    cx.run_until_parked();
    cx.dispatch_action(Close);
    cx.run_until_parked();
    cx.simulate_prompt_answer("Discard");
    cx.run_until_parked();

    let app = active_document(&app, cx);
    assert!(cx.update(|_, cx| app.read(cx).markdown_search.read(cx).value().is_empty()));
    assert!(!cx.update(|_, cx| app.read(cx).search.open));
    assert!(cx.update(|_, cx| app.read(cx).search.matches.is_empty()));
}

#[gpui::test]
fn editing_refreshes_search_without_auto_scrolling_or_moving_the_query(cx: &mut TestAppContext) {
    let (app, cx) = boot(cx);
    app.update_in(cx, |app, window, cx| {
        let source = "alpha beta alpha";
        app.session.document.saved = source.into();
        app.editor.update(cx, |editor, cx| {
            editor.set_text(source, cx);
            editor.set_cursor(0, cx);
            editor.focus(window, cx);
        });
    });
    cx.dispatch_action(FindMarkdown);
    cx.run_until_parked();
    cx.simulate_input("alpha");
    cx.run_until_parked();
    cx.dispatch_action(FindNextMarkdown);
    cx.run_until_parked();
    let before = cx.update(|_, cx| app.read(cx).scroll.offset());

    app.update_in(cx, |app, window, cx| {
        app.editor.update(cx, |editor, cx| {
            editor.set_cursor(0, cx);
            editor.focus(window, cx);
        });
    });
    cx.simulate_input("x");
    cx.run_until_parked();

    assert_eq!(cx.update(|_, cx| app.read(cx).search.matches.len()), 2);
    assert_eq!(cx.update(|_, cx| app.read(cx).search.active), Some(1));
    assert_eq!(cx.update(|_, cx| app.read(cx).scroll.offset()), before);
    assert_eq!(
        cx.update(|_, cx| app.read(cx).markdown_search.read(cx).value().to_owned()),
        "alpha"
    );
}

#[gpui::test]
fn cancelled_document_transition_preserves_search_state(cx: &mut TestAppContext) {
    let (app, cx) = boot(cx);
    app.update_in(cx, |app, _, cx| {
        app.editor
            .update(cx, |editor, cx| editor.set_text("alpha", cx));
    });
    cx.dispatch_action(FindMarkdown);
    cx.run_until_parked();
    cx.simulate_input("alpha");
    cx.run_until_parked();
    cx.dispatch_action(Close);
    cx.run_until_parked();
    assert!(cx.update(|_, cx| app.read(cx).search.open));
    cx.simulate_prompt_answer("Cancel");
    cx.run_until_parked();

    assert!(cx.update(|_, cx| app.read(cx).search.open));
    assert_eq!(
        cx.update(|_, cx| app.read(cx).markdown_search.read(cx).value().to_owned()),
        "alpha"
    );
    assert_eq!(
        cx.update(|_, cx| app.read(cx).editor.read(cx).text().to_owned()),
        "alpha"
    );
}

#[gpui::test]
fn pdf_preview_does_not_change_markdown_search(cx: &mut TestAppContext) {
    let dir = tempfile::tempdir().unwrap();
    let pdf_path = dir.path().join("invalid.pdf");
    std::fs::write(&pdf_path, b"not a PDF").unwrap();
    let (app, cx) = boot(cx);
    app.update_in(cx, |app, _, cx| {
        app.session.document.saved = "alpha".into();
        app.editor
            .update(cx, |editor, cx| editor.set_text("alpha", cx));
    });
    cx.dispatch_action(FindMarkdown);
    cx.run_until_parked();
    cx.simulate_input("alpha");
    cx.run_until_parked();
    app.update_in(cx, |app, window, cx| app.open_path(pdf_path, window, cx));
    cx.run_until_parked();

    assert!(cx.update(|_, cx| app.read(cx).search.open));
    assert_eq!(cx.update(|_, cx| app.read(cx).search.matches.len()), 1);
    assert!(cx.update(
        |_, cx| app.read(cx).preview.pdf.is_some() || app.read(cx).preview.message.is_some()
    ));
}

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
fn pdf_open_keeps_markdown_and_close_pdf_restores_editor(cx: &mut TestAppContext) {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("invalid.pdf");
    std::fs::write(&path, b"not a PDF").unwrap();
    let (app, cx) = boot(cx);
    cx.simulate_input("keep me");
    app.update_in(cx, |app, window, cx| app.open_path(path, window, cx));
    cx.run_until_parked();
    assert!(cx.update(|_, cx| {
        let preview = &app.read(cx).preview;
        preview.pdf.is_none() && !preview.retryable && preview.message.is_some()
    }));
    assert_eq!(
        cx.update(|_, cx| app.read(cx).editor.read(cx).text().to_owned()),
        "keep me"
    );
    cx.dispatch_action(ClosePdf);
    cx.run_until_parked();
    assert!(cx.update(|_, cx| app.read(cx).preview.pdf.is_none()));
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
    assert!(cx.update(|_, cx| failed.read(cx).unavailable));
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

#[test]
fn reference_pdf_parses_and_renders() {
    let path =
        std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/reference.pdf");
    let bytes = std::sync::Arc::new(std::fs::read(path).unwrap());
    let pdf = gpui_pdf::parse(bytes).unwrap();
    assert_eq!(gpui_pdf::page_dims(&pdf), vec![(420.0, 595.0)]);
    assert!(gpui_pdf::render_page(&pdf, 0, 1.0, gpui_pdf::PageTone::Original).is_ok());
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

fn converted(source: PathBuf) -> import::Imported {
    import::Imported {
        ocr_configuration: None,
        source,
        markdown: "# Imported\n".into(),
        warning: Some("Partial import: pages 2 of 2 require OCR and were skipped.".into()),
        is_pdf: false,
        is_docx: false,
    }
}

#[gpui::test]
fn inline_ocr_skip_imports_native_content_and_keeps_page_warning(cx: &mut TestAppContext) {
    let (app, cx) = boot(cx);
    let source = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("tests/fixtures/import/handmade-partly-scanned.pdf");
    app.update_in(cx, |app, window, cx| {
        app.start_import(source.clone(), window, cx)
    });
    cx.run_until_parked();
    assert!(cx.update(|_, cx| app.read(cx).ocr_required.is_some()));
    assert!(!cx.has_pending_prompt());
    app.update_in(cx, |app, window, cx| app.ocr_action(true, window, cx));
    cx.run_until_parked();
    app.update_in(cx, |app, _, cx| {
        assert!(!app.job.busy());
        assert!(!app.prompting);
        assert!(app.editor.read(cx).text().contains("Readable page three"));
        assert!(app.session.warning.as_ref().unwrap().contains("2, 5"));
        assert!(app.session.document.path.is_none());
        assert_eq!(app.session.source.as_ref(), Some(&source));
    });
}

#[gpui::test]
fn inline_ocr_wait_preserves_current_edits(cx: &mut TestAppContext) {
    let (app, cx) = boot(cx);
    app.update_in(cx, |app, window, cx| {
        app.editor
            .update(cx, |editor, cx| editor.set_text("keep edits", cx));
        app.job.complete(
            app.session.generation,
            Err(import::ImportError::NeedsOcr(
                "scan.pdf".into(),
                vec![1],
                Vec::new(),
                1,
            )),
        );
        app.resume_import(window, cx);
    });
    assert!(!cx.has_pending_prompt());
    cx.run_until_parked();
    app.update_in(cx, |app, _, cx| {
        assert_eq!(app.editor.read(cx).text(), "keep edits");
        assert!(!app.job.busy());
        assert!(!app.prompting);
        assert!(app.session.source.is_none());
    });
}

#[gpui::test]
fn ocr_setup_failure_is_retryable_and_stale_success_does_not_import(cx: &mut TestAppContext) {
    let (app, cx) = boot(cx);
    app.update_in(cx, |app, window, cx| {
        app.ocr_state = OcrState::Installing;
        app.job
            .defer_for_setup(app.session.generation, "scan.pdf".into());
        app.finish_ocr_setup(Err("download interrupted".into()), window, cx);
        assert!(matches!(&app.ocr_state, OcrState::Failed(error) if error.contains("download interrupted")));
        assert!(!app.job.busy());
        assert!(app.ocr_required.is_some());
        assert!(!app.prompting);
    });
    assert!(!cx.has_pending_prompt());
    cx.run_until_parked();
    app.update_in(cx, |app, window, cx| {
        assert!(!app.job.busy());
        assert!(!app.import_busy.load(Ordering::Relaxed));
        app.editor
            .update(cx, |editor, cx| editor.set_text("keep edits", cx));
        app.job
            .defer_for_setup(app.session.generation, "scan.pdf".into());
        app.session.generation += 1;
        app.finish_ocr_setup(
            Ok(ocr::Installed {
                config: crate::settings::OcrConfig::default(),
                models: "models".into(),
                pdfium: "pdfium".into(),
                onnx: "onnx".into(),
            }),
            window,
            cx,
        );
        assert!(!app.job.busy());
        assert!(!app.job.has_ocr_continuation());
        assert_eq!(app.editor.read(cx).text(), "keep edits");
    });
}

fn set_default_ocr(app: &DocumentView, model: crate::settings::OcrModel) {
    let mut store = app.preferences.borrow_mut();
    store.current.as_mut().unwrap().ocr.model = model;
}

/// A pick, a run's choice and a setup's captured model all survive a
/// Settings change, and setup never leaves the shared state Installing.
#[gpui::test]
fn recognition_choice_and_setup_survive_a_settings_change(cx: &mut TestAppContext) {
    use crate::recognition::{ENGLISH, RUSSIAN};
    use crate::settings::OcrModel;
    let (app, cx) = boot(cx);
    app.update_in(cx, |app, window, cx| {
        app.job.complete(
            app.session.generation,
            Err(import::ImportError::NeedsOcr(
                "scan.pdf".into(),
                vec![1],
                Vec::new(),
                1,
            )),
        );
        app.resume_import(window, cx);
        assert_eq!(app.recognition_choice().languages, [ENGLISH]);
        // Picking the default again still holds it.
        app.pick_recognition(vec![ENGLISH, RUSSIAN], cx);
        app.pick_recognition(vec![ENGLISH], cx);
        set_default_ocr(app, OcrModel::Cyrillic);
        assert_eq!(
            app.recognition_choice().reason,
            "English: no sign of Russian in the text, file name or keyboard"
        );
        assert_eq!(app.recognition_model(), OcrModel::V6Small);
        // Without a pick, running OCR keeps the choice it ran with. The
        // bundle counts as checked, so nothing is set up or downloaded.
        app.recognition_pick = None;
        assert_eq!(app.recognition_model(), OcrModel::Cyrillic);
        app.model_panel.update(cx, |panel, _| {
            panel.ocr_installations.push(ocr::Installed {
                config: crate::settings::OcrConfig {
                    model: OcrModel::Cyrillic,
                    ..Default::default()
                },
                models: "models".into(),
                pdfium: "pdfium".into(),
                onnx: "onnx".into(),
            })
        });
        app.ocr_action(false, window, cx);
        assert!(app.job.recognizing());
        set_default_ocr(app, OcrModel::V6Small);
        assert_eq!(app.recognition_model(), OcrModel::Cyrillic);
    });
    cx.run_until_parked();
    // English setup claimed the shared state; Settings moves to Cyrillic
    // before it finishes. The tab resumes with English and clears Installing.
    app.update_in(cx, |app, window, cx| {
        assert!(!app.job.busy());
        app.recognition_pick = None;
        app.ocr_state = OcrState::Installing;
        app.ocr_setup = Some((OcrModel::V6Small, true));
        app.job
            .defer_for_setup(app.session.generation, "scan.pdf".into());
        set_default_ocr(app, OcrModel::Cyrillic);
        app.finish_ocr_setup(
            Ok(ocr::Installed {
                config: crate::settings::OcrConfig::default(),
                models: "models".into(),
                pdfium: "pdfium".into(),
                onnx: "onnx".into(),
            }),
            window,
            cx,
        );
        // The new Settings model's state comes from what Settings knows.
        assert!(matches!(
            &app.ocr_state,
            OcrState::Ready(i) if i.config.model == OcrModel::Cyrillic
        ));
        assert!(app.job.recognizing(), "resumed with the captured model");
    });
    cx.run_until_parked();
    app.update_in(cx, |app, _, _| {
        assert!(!app.job.busy());
        assert!(!app.ocr_state.busy());
    });
}

#[gpui::test]
fn ocr_runtime_failure_enables_setup_retry(cx: &mut TestAppContext) {
    let (app, cx) = boot(cx);
    app.update_in(cx, |app, window, cx| {
        app.job.complete(
            app.session.generation,
            Err(import::ImportError::OcrFailed("runtime missing".into())),
        );
        app.resume_import(window, cx);
        assert!(
            matches!(&app.ocr_state, OcrState::Failed(error) if error.contains("runtime missing"))
        );
        assert!(app.error.as_ref().unwrap().contains("runtime missing"));
        assert!(!app.job.busy());
    });
}

#[gpui::test]
fn ocr_without_text_stays_with_its_document(cx: &mut TestAppContext) {
    let (app, cx) = boot(cx);
    app.update_in(cx, |app, window, cx| {
        app.ocr_state = OcrState::Ready(crate::ocr::Installed {
            config: crate::settings::OcrConfig::default(),
            models: "models".into(),
            pdfium: "pdfium".into(),
            onnx: "onnx".into(),
        });
        app.conversion_source = Some("scan.pdf".into());
        app.job.complete(
            app.session.generation,
            Err(import::ImportError::OcrNoText("no usable text".into())),
        );
        app.resume_import(window, cx);
        assert!(matches!(app.ocr_state, OcrState::Ready(_)));
        assert!(app.ocr_error.as_ref().unwrap().contains("no usable text"));
        assert!(app.error.as_ref().unwrap().contains("no usable text"));
        assert!(app.ocr_required.is_some());
        assert!(!app.job.busy());
    });
}

#[gpui::test]
fn import_action_converts_and_saves_without_touching_source(cx: &mut TestAppContext) {
    let dir = tempfile::tempdir().unwrap();
    let source = dir.path().join("table.csv");
    let output = dir.path().join("table.md");
    let original = "Name,Count\nApples,2\n";
    std::fs::write(&source, original).unwrap();
    let (app, cx) = boot(cx);
    app.update_in(cx, |app, window, cx| {
        app.open_source(source.clone(), window, cx)
    });
    cx.run_until_parked();
    cx.dispatch_action(Import);
    cx.run_until_parked();
    let markdown = cx.update(|_, cx| {
        let app = app.read(cx);
        assert!(!app.job.busy());
        assert!(app.dirty(cx));
        assert!(app.session.document.path.is_none());
        assert_eq!(app.save_directory(), dir.path());
        app.editor.read(cx).text().to_owned()
    });
    assert!(markdown.contains("| Apples | 2 |"));
    cx.dispatch_action(Save);
    cx.run_until_parked();
    cx.simulate_new_path_selection(|_| Some(output.clone()));
    cx.run_until_parked();
    assert_eq!(std::fs::read_to_string(output).unwrap(), markdown);
    assert_eq!(std::fs::read_to_string(source).unwrap(), original);
    assert!(!cx.update(|_, cx| app.read(cx).dirty(cx)));
}

#[gpui::test]
fn import_completion_protects_edits_and_cancel_preserves_pdf(cx: &mut TestAppContext) {
    let dir = tempfile::tempdir().unwrap();
    let source = dir.path().join("source.pdf");
    std::fs::write(&source, b"placeholder").unwrap();
    let (app, cx) = boot(cx);
    cx.simulate_input("edits made during conversion");
    app.update_in(cx, |app, window, cx| {
        let mut result = converted(source.clone());
        result.is_pdf = true;
        assert!(app.job.begin(false));
        app.job.complete(app.session.generation, Ok(result));
        app.resume_import(window, cx);
    });
    cx.run_until_parked();
    assert!(cx.has_pending_prompt());
    cx.simulate_prompt_answer("Cancel");
    cx.run_until_parked();
    cx.update(|_, cx| {
        let app = app.read(cx);
        assert_eq!(app.editor.read(cx).text(), "edits made during conversion");
        assert!(app.preview.pdf.is_none());
        assert!(app.session.source.is_none());
        assert!(!app.job.busy());
    });
}

#[gpui::test]
fn import_save_then_replace_preserves_original_edits(cx: &mut TestAppContext) {
    let dir = tempfile::tempdir().unwrap();
    let saved = dir.path().join("before.md");
    let (app, cx) = boot(cx);
    cx.simulate_input("save these edits");
    app.update_in(cx, |app, window, cx| {
        app.request(
            Next::Import(converted(dir.path().join("source.docx"))),
            window,
            cx,
        );
    });
    cx.run_until_parked();
    cx.simulate_prompt_answer("Save");
    cx.run_until_parked();
    cx.simulate_new_path_selection(|_| Some(saved.clone()));
    cx.run_until_parked();
    assert_eq!(std::fs::read_to_string(saved).unwrap(), "save these edits");
    cx.update(|_, cx| {
        let app = app.read(cx);
        assert_eq!(app.editor.read(cx).text(), "# Imported\n");
        assert!(app.dirty(cx));
        assert!(app.session.document.path.is_none());
    });
}

#[gpui::test]
fn import_waits_for_dialog_and_discards_result_after_document_change(cx: &mut TestAppContext) {
    let dir = tempfile::tempdir().unwrap();
    let (app, cx) = boot(cx);
    cx.simulate_input("before");
    let generation = cx.update(|_, cx| app.read(cx).session.generation);
    cx.dispatch_action(Close);
    cx.run_until_parked();
    app.update_in(cx, |app, window, cx| {
        assert!(app.job.begin(false));
        app.job
            .complete(generation, Ok(converted(dir.path().join("source.docx"))));
        app.resume_import(window, cx);
        assert!(app.job.has_pending());
    });
    cx.simulate_prompt_answer("Discard");
    cx.run_until_parked();
    // Deliver the delayed completion after the shell has released the tab.
    // The retained test handle lets us verify the old generation is rejected.
    app.update_in(cx, |app, window, cx| app.resume_import(window, cx));
    cx.run_until_parked();
    cx.update(|_, cx| {
        let app = app.read(cx);
        assert_eq!(app.editor.read(cx).text(), "before");
        assert!(app.session.source.is_none());
        assert!(!app.job.busy());
        assert!(!app.job.has_pending());
    });
    let next = active_document(&app, cx);
    assert!(cx.update(|_, cx| next.read(cx).editor.read(cx).text().is_empty()));
}

#[gpui::test]
fn import_completes_after_open_picker_cancel_and_refuses_second_job(cx: &mut TestAppContext) {
    let dir = tempfile::tempdir().unwrap();
    let (app, cx) = boot(cx);
    cx.dispatch_action(Open);
    cx.run_until_parked();
    app.update_in(cx, |app, window, cx| {
        assert!(app.job.begin(false));
        app.job.complete(
            app.session.generation,
            Ok(converted(dir.path().join("source.docx"))),
        );
        app.start_import(dir.path().join("missing.pdf"), window, cx);
        app.resume_import(window, cx);
        assert!(app.job.has_pending());
    });
    cx.simulate_path_prompt_response(|_| None);
    cx.run_until_parked();
    cx.update(|_, cx| {
        let app = app.read(cx);
        assert_eq!(app.editor.read(cx).text(), "# Imported\n");
        assert!(!app.job.busy());
        assert!(app.error.is_none());
    });
}

#[gpui::test]
fn imported_warning_survives_save_and_stays_with_its_tab_until_dismissed(cx: &mut TestAppContext) {
    let dir = tempfile::tempdir().unwrap();
    let (app, cx) = boot(cx);
    app.update_in(cx, |app, window, cx| {
        app.proceed(
            Next::Import(converted(dir.path().join("source.docx"))),
            window,
            cx,
        );
        app.write(dir.path().join("output.md"), None, window, cx);
        assert!(app.session.warning.is_some());
    });
    cx.dispatch_action(DismissImportWarning);
    cx.run_until_parked();
    assert!(cx.update(|_, cx| app.read(cx).session.warning.is_none()));
    app.update(cx, |app, _| {
        app.session.warning = Some("warning".into());
    });
    let next = new_document(&app, cx);
    cx.update(|_, cx| {
        assert!(next.read(cx).session.warning.is_none());
        assert!(next.read(cx).session.source.is_none());
        assert_eq!(app.read(cx).session.warning.as_deref(), Some("warning"));
    });
}

#[gpui::test]
fn empty_import_is_unsaved_and_source_cannot_be_overwritten(cx: &mut TestAppContext) {
    let dir = tempfile::tempdir().unwrap();
    let source = dir.path().join("disguised.md");
    std::fs::write(&source, "source bytes").unwrap();
    let (app, cx) = boot(cx);
    app.update_in(cx, |app, window, cx| {
        let mut result = converted(source.clone());
        result.markdown.clear();
        app.proceed(Next::Import(result), window, cx);
        assert!(app.dirty(cx));
        app.write(source.clone(), None, window, cx);
        assert!(
            app.error
                .as_ref()
                .unwrap()
                .contains("preserve the imported source")
        );
        assert!(app.dirty(cx));
    });
    assert_eq!(std::fs::read_to_string(source).unwrap(), "source bytes");
}

#[gpui::test]
fn import_failure_and_stale_completion_keep_current_document(cx: &mut TestAppContext) {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("other.md");
    std::fs::write(&path, "other").unwrap();
    let (app, cx) = boot(cx);
    cx.simulate_input("keep edits");
    app.update_in(cx, |app, window, cx| {
        assert!(app.job.begin(false));
        app.job
            .complete(app.session.generation, Err("requires OCR".into()));
        app.resume_import(window, cx);
        assert_eq!(app.editor.read(cx).text(), "keep edits");
        assert_eq!(app.error.as_deref(), Some("requires OCR"));
        let generation = app.session.generation;
        let mut replacement = converted(path);
        replacement.markdown = "other".into();
        app.proceed(Next::Import(replacement), window, cx);
        assert!(app.job.begin(false));
        app.job
            .complete(generation, Ok(converted(dir.path().join("source.docx"))));
        app.resume_import(window, cx);
        assert_eq!(app.editor.read(cx).text(), "other");
        assert!(app.error.is_none());
        assert!(!app.job.busy());
    });
}

#[gpui::test]
fn accepted_pdf_import_opens_source_pane(cx: &mut TestAppContext) {
    let source =
        std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/reference.pdf");
    let (app, cx) = boot(cx);
    app.update_in(cx, |app, window, cx| {
        let mut imported = converted(source);
        imported.is_pdf = true;
        app.proceed(Next::Import(imported), window, cx);
        assert!(app.preview.loading);
        assert!(app.dirty(cx));
    });
    cx.run_until_parked();
    cx.update(|_, cx| {
        let app = app.read(cx);
        assert!(!app.preview.loading);
        assert!(app.preview.pdf.is_some(), "the source pane opened");
        assert!(app.preview.docx.is_none());
    });
}

#[gpui::test]
fn accepted_docx_import_opens_source_pane(cx: &mut TestAppContext) {
    let source =
        std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/import/text.docx");
    let (app, cx) = boot(cx);
    app.update_in(cx, |app, window, cx| {
        let mut imported = converted(source);
        imported.is_docx = true;
        app.proceed(Next::Import(imported), window, cx);
        assert!(app.dirty(cx));
    });
    cx.run_until_parked();
    assert!(cx.update(|_, cx| {
        let app = app.read(cx);
        app.preview.pdf.is_some() && app.preview.docx.is_some()
    }));
}

#[gpui::test]
fn failed_docx_preview_keeps_retry_source(cx: &mut TestAppContext) {
    let dir = tempfile::tempdir().unwrap();
    let source = dir.path().join("missing.docx");
    let (app, cx) = boot(cx);
    app.update_in(cx, |app, window, cx| {
        app.open_path(source.clone(), window, cx);
    });
    cx.run_until_parked();
    assert!(cx.update(|_, cx| {
        let app = app.read(cx);
        app.preview.pdf.is_none()
            && app.preview.retryable
            && app.preview.source.as_deref() == Some(source.as_path())
    }));
    cx.dispatch_action(RetryPreview);
    cx.run_until_parked();
    assert!(cx.update(|_, cx| app.read(cx).preview.retryable));
}

#[gpui::test]
fn damaged_or_unpreviewable_sources_show_one_message_without_retry(cx: &mut TestAppContext) {
    let fixtures = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/import");
    let dir = tempfile::tempdir().unwrap();
    let damaged_pdf = dir.path().join("damaged.pdf");
    std::fs::write(&damaged_pdf, b"%PDF-1.7 not really").unwrap();
    let (app, cx) = boot(cx);
    for (source, message) in [
        (
            fixtures.join("truncated--errors.docx"),
            "This DOCX is damaged or incomplete, so it can't be previewed.",
        ),
        (
            damaged_pdf,
            "This PDF is damaged or incomplete, so it can't be previewed.",
        ),
    ] {
        app.update_in(cx, |app, window, cx| app.open_path(source, window, cx));
        cx.run_until_parked();
        cx.update(|_, cx| {
            let app = app.read(cx);
            assert!(app.preview.pdf.is_none());
            assert!(!app.preview.retryable, "{message}");
            assert!(app.preview.failed, "{message}");
            assert_eq!(app.preview.message.as_deref(), Some(message));
        });
    }
    app.update_in(cx, |app, window, cx| {
        app.open_source(fixtures.join("sheet.xlsx"), window, cx)
    });
    cx.run_until_parked();
    cx.update(|_, cx| {
        let app = app.read(cx);
        assert!(!app.preview.retryable && !app.preview.failed);
        assert_eq!(app.preview.message.as_deref(), Some(preview::NO_PREVIEW));
    });
}

#[gpui::test]
fn failed_replacement_preserves_loaded_preview_and_markdown(cx: &mut TestAppContext) {
    let source = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/reference.pdf");
    let (app, cx) = boot(cx);
    cx.simulate_input("unsaved text");
    app.update_in(cx, |app, window, cx| app.open_path(source, window, cx));
    cx.run_until_parked();
    let old = cx.update(|_, cx| app.read(cx).preview.pdf.clone().unwrap());
    for missing in ["missing.pdf", "missing.docx"] {
        app.update_in(cx, |app, window, cx| {
            app.open_path(PathBuf::from(missing), window, cx)
        });
        cx.run_until_parked();
        cx.update(|_, cx| {
            let app = app.read(cx);
            assert_eq!(app.preview.pdf.as_ref(), Some(&old));
            assert!(app.preview.retryable);
            assert_eq!(app.editor.read(cx).text(), "unsaved text");
            assert!(app.dirty(cx));
        });
    }
}

#[gpui::test]
fn accepted_import_failure_clears_old_preview_and_pdf_retry_works(cx: &mut TestAppContext) {
    let dir = tempfile::tempdir().unwrap();
    let source = dir.path().join("source.pdf");
    let reference = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/reference.pdf");
    let (app, cx) = boot(cx);
    app.update_in(cx, |app, window, cx| {
        app.open_path(reference.clone(), window, cx)
    });
    cx.run_until_parked();
    app.update_in(cx, |app, window, cx| {
        let mut imported = converted(source.clone());
        imported.is_pdf = true;
        app.proceed(Next::Import(imported), window, cx);
        assert!(app.preview.pdf.is_none());
        assert!(app.dirty(cx));
    });
    cx.run_until_parked();
    assert!(cx.update(|_, cx| app.read(cx).preview.retryable));
    std::fs::copy(reference, &source).unwrap();
    cx.dispatch_action(RetryPreview);
    cx.run_until_parked();
    cx.update(|_, cx| {
        let app = app.read(cx);
        assert!(app.preview.pdf.as_ref().unwrap().read(cx).is_loaded());
        assert!(!app.preview.retryable);
        assert!(app.dirty(cx));
    });
}

#[gpui::test]
fn close_and_newer_request_discard_pending_completions(cx: &mut TestAppContext) {
    let base = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let docx = base.join("tests/fixtures/import/text.docx");
    let pdf = base.join("tests/fixtures/reference.pdf");
    let (app, cx) = boot(cx);
    app.update_in(cx, |app, window, cx| {
        app.open_docx(docx.clone(), window, cx);
        app.close_preview(window, cx);
    });
    cx.run_until_parked();
    cx.update(|_, cx| {
        let app = app.read(cx);
        assert!(app.preview.pdf.is_none());
        assert!(app.preview.source.is_none());
        assert!(app.preview.message.is_none());
        assert!(!app.preview.loading);
    });
    app.update_in(cx, |app, window, cx| {
        app.open_docx(docx, window, cx);
        app.open_pdf(pdf.clone(), window, cx);
    });
    cx.run_until_parked();
    cx.update(|_, cx| {
        let app = app.read(cx);
        assert_eq!(app.preview.source.as_ref(), Some(&pdf));
        assert!(app.preview.pdf.as_ref().unwrap().read(cx).is_loaded());
        assert!(app.preview.docx.is_none());
    });
}

/// CPU frame budget through the real workspace, including its editor host.
#[gpui::test]
#[ignore]
fn large_markdown_scroll_budget(cx: &mut TestAppContext) {
    let base = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    // The public coverage DOCX and the scrubbed 37-page legal opinion.
    let files = std::env::var("MD_PERF_FILE").map_or_else(
        |_| vec!["coverage.docx".to_owned(), "legal-opinion.docx".to_owned()],
        |file| vec![file],
    );
    let (mut app, cx) = boot(cx);
    // Fixed outer window.
    cx.simulate_resize(gpui::size(px(1332.), px(750.)));
    let dir = tempfile::tempdir().unwrap();
    let mut failures = Vec::new();
    for file in files {
        let source = base.join("tests/fixtures/docx-preview").join(&file);
        let imported = import::convert(&source).unwrap();
        let path = dir
            .path()
            .join(std::path::Path::new(&file).with_extension("md"));
        std::fs::write(&path, &imported.markdown).unwrap();
        app = open_document(&app, path, cx);
        cx.run_until_parked();
        for state in ["markdown_only", "preview_open", "preview_closed"] {
            if state == "preview_open" {
                app.update_in(cx, |app, window, cx| {
                    app.open_docx(source.clone(), window, cx)
                });
            } else if state == "preview_closed" {
                app.update_in(cx, |app, window, cx| app.close_preview(window, cx));
            }
            cx.run_until_parked();
            cx.update(|_, cx| {
                assert_eq!(app.read(cx).preview.pdf.is_some(), state == "preview_open");
            });
            let mut times = Vec::new();
            let mut furthest = 0.0_f32;
            for frame in 0..60 {
                let started = std::time::Instant::now();
                cx.update(|window, cx| {
                    app.update(cx, |app, cx| {
                        let step = if frame < 30 { frame } else { 59 - frame };
                        let offset = app.scroll.max_offset().y.abs() * (step as f32 / 29.);
                        app.scroll.set_offset(gpui::point(px(0.), -offset));
                        cx.notify();
                    });
                    window.refresh();
                    window.draw(cx).clear(cx);
                });
                let ms = started.elapsed().as_secs_f64() * 1000.;
                furthest =
                    furthest.max(cx.update(|_, cx| -f32::from(app.read(cx).scroll.offset().y)));
                cx.run_until_parked();
                if frame >= 5 {
                    times.push(ms);
                }
            }
            assert!(furthest > 100., "viewport must actually scroll");
            times.sort_by(f64::total_cmp);
            let p95 = times[times.len() * 95 / 100];
            eprintln!(
                "MD_PERF file={file} state={state} bytes={} lines={} median_ms={:.2} p95_ms={p95:.2} editor_width={:?}",
                imported.markdown.len(),
                imported.markdown.lines().count(),
                times[times.len() / 2],
                app.read_with(cx, |v, cx| v.editor.read(cx).layout_width().map(f32::from)),
            );
            if p95 > 16.667 {
                failures.push(format!("{file} {state} scroll"));
            }
            // Measure actual text input followed by layout/paint, not just idle redraw.
            app.update(cx, |app, cx| {
                app.editor.update(cx, |e, cx| e.set_cursor(0, cx))
            });
            let mut typing = Vec::new();
            for _ in 0..12 {
                let started = std::time::Instant::now();
                cx.simulate_input("x");
                cx.update(|window, cx| {
                    window.refresh();
                    window.draw(cx).clear(cx);
                });
                typing.push(started.elapsed().as_secs_f64() * 1000.);
                cx.run_until_parked();
            }
            typing.sort_by(f64::total_cmp);
            let typing_p95 = typing[typing.len() * 95 / 100];
            eprintln!("MD_PERF file={file} state={state} typing_p95_ms={typing_p95:.2}");
            if typing_p95 > 50. {
                failures.push(format!("{file} {state} typing"));
            }
        }
        cx.update(|_, cx| {
            assert_eq!(
                app.read(cx).editor.read(cx).text(),
                format!("{}{}", "x".repeat(36), imported.markdown)
            );
        });
        app = close_document(&app, cx);
    }
    assert!(
        failures.is_empty(),
        "editor CPU budget exceeded: {failures:?}"
    );
}

#[gpui::test]
fn comments_follow_preview_lifecycle(cx: &mut TestAppContext) {
    let base = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let (app, cx) = boot(cx);
    cx.simulate_input("Markdown stays unchanged");
    app.update_in(cx, |app, window, cx| {
        app.open_docx(
            base.join("tests/fixtures/docx-preview/comments.docx"),
            window,
            cx,
        )
    });
    cx.run_until_parked();
    let panel = cx.update(|_, cx| {
        let app = app.read(cx);
        assert_eq!(app.preview.docx.as_ref().unwrap().comments.len(), 4);
        app.preview.comment_panel.clone().unwrap()
    });
    app.update_in(cx, |app, window, cx| {
        app.open_docx(base.join("missing.docx"), window, cx)
    });
    cx.run_until_parked();
    cx.update(|_, cx| assert_eq!(app.read(cx).preview.comment_panel.as_ref(), Some(&panel)));
    app.update_in(cx, |app, window, cx| {
        app.open_pdf(base.join("tests/fixtures/reference.pdf"), window, cx)
    });
    cx.run_until_parked();
    cx.update(|_, cx| assert!(app.read(cx).preview.comment_panel.is_none()));
    app.update_in(cx, |app, window, cx| {
        app.open_docx(
            base.join("tests/fixtures/docx-preview/comments.docx"),
            window,
            cx,
        )
    });
    cx.run_until_parked();
    cx.dispatch_action(ClosePdf);
    cx.run_until_parked();
    cx.update(|_, cx| {
        let app = app.read(cx);
        assert!(!app.preview.visible);
        assert!(app.preview.comment_panel.is_some());
        assert!(app.preview.docx.is_some());
        assert_eq!(app.editor.read(cx).text(), "Markdown stays unchanged");
    });
}

#[gpui::test]
fn repeated_document_switches_release_preview_entities_and_backing_files(cx: &mut TestAppContext) {
    let (mut app, cx) = boot(cx);
    let base = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    for _ in 0..3 {
        app.update_in(cx, |app, window, cx| {
            app.open_docx(
                base.join("tests/fixtures/docx-preview/comments.docx"),
                window,
                cx,
            );
        });
        cx.run_until_parked();
        let (pdf, comments, backing) = cx.update(|_, cx| {
            let preview = &app.read(cx).preview;
            (
                preview.pdf.as_ref().unwrap().downgrade(),
                preview.comment_panel.as_ref().unwrap().downgrade(),
                preview.docx.as_ref().unwrap().pdf_path.clone(),
            )
        });
        assert!(backing.exists());
        app.update_in(cx, |app, window, cx| {
            app.open_pdf(base.join("tests/fixtures/reference.pdf"), window, cx);
        });
        cx.run_until_parked();
        cx.update(|window, cx| {
            window.refresh();
            window.draw(cx).clear(cx);
        });
        assert!(pdf.upgrade().is_none());
        assert!(comments.upgrade().is_none());
        assert!(crate::docx_preview::removed(&backing));
        let pdf = cx.update(|_, cx| app.read(cx).preview.pdf.as_ref().unwrap().downgrade());
        app.update_in(cx, |app, window, cx| {
            app.close_preview(window, cx);
        });
        app = close_document(&app, cx);
        cx.run_until_parked();
        cx.update(|window, cx| {
            window.refresh();
            window.draw(cx).clear(cx);
        });
        assert!(pdf.upgrade().is_none());
    }
}

/// The flagged words in the active editor, in source order.
fn misspelled(app: &Entity<DocumentView>, cx: &mut VisualTestContext) -> Vec<String> {
    cx.update(|_, cx| {
        let editor = app.read(cx).editor.read(cx);
        editor
            .diagnostics()
            .iter()
            .map(|d| editor.text()[d.range.clone()].to_owned())
            .collect()
    })
}

fn selected_text(app: &Entity<DocumentView>, cx: &mut VisualTestContext) -> String {
    cx.update(|_, cx| {
        let editor = app.read(cx).editor.read(cx);
        editor.text()[editor.selection()].to_owned()
    })
}

#[gpui::test]
fn spellcheck_flags_prose_and_skips_markdown_and_entities(cx: &mut TestAppContext) {
    use crate::pii::{Category, Detection};
    let source =
        "The partys agre.\n`fooo` [link](https://exampel.com/pathh) Mr Kowalczyk and Mr Ivanоv\n";
    let (app, cx) = boot(cx);
    app.update(cx, |app, cx| {
        app.editor
            .update(cx, |editor, cx| editor.set_text(source, cx));
    });
    cx.run_until_parked();
    // Before PII review, names are ordinary words.
    assert_eq!(
        misspelled(&app, cx),
        ["partys", "agre", "Kowalczyk", "Ivanоv"]
    );
    app.update(cx, |app, cx| {
        let person = |name: &str| {
            let start = source.find(name).unwrap();
            Detection {
                range: start..start + name.len(),
                category: Category::Person,
                score: 0.9,
                recognizer: crate::pii::Recognizer::Model,
            }
        };
        app.pii.reviewing = true;
        app.pii
            .review
            .ingest(source, vec![person("Kowalczyk"), person("Ivanоv")])
            .unwrap();
        app.review_changed(cx);
    });
    cx.run_until_parked();
    // Entities skip the dictionary; the Cyrillic о in "Ivanоv" still shows.
    assert_eq!(misspelled(&app, cx), ["partys", "agre", "Ivanоv"]);
}

#[gpui::test]
fn spellcheck_navigation_document_switch_and_edits(cx: &mut TestAppContext) {
    let (app, cx) = boot(cx);
    app.update(cx, |app, cx| {
        app.editor.update(cx, |editor, cx| {
            editor.set_text("Frist and secnd words.", cx)
        });
    });
    cx.run_until_parked();
    assert_eq!(misspelled(&app, cx), ["Frist", "secnd"]);
    for (backwards, expected) in [
        (false, "Frist"),
        (false, "secnd"),
        (false, "Frist"),
        (true, "secnd"),
    ] {
        app.update_in(cx, |app, window, cx| {
            if backwards {
                app.previous_misspelling(&PreviousMisspelling, window, cx)
            } else {
                app.next_misspelling(&NextMisspelling, window, cx)
            }
        });
        assert_eq!(selected_text(&app, cx), expected);
    }

    // The per-document switch clears and restores the squiggles.
    app.update_in(cx, |app, window, cx| {
        app.toggle_document_spelling(&ToggleDocumentSpelling, window, cx)
    });
    cx.run_until_parked();
    assert!(misspelled(&app, cx).is_empty());
    app.update_in(cx, |app, window, cx| {
        app.toggle_document_spelling(&ToggleDocumentSpelling, window, cx)
    });
    cx.run_until_parked();
    assert_eq!(misspelled(&app, cx), ["Frist", "secnd"]);

    // An edit rechecks after a short pause; until then ranges follow the edit.
    app.update(cx, |app, cx| {
        app.editor
            .update(cx, |editor, cx| editor.replace_range(0..5, "First", cx));
    });
    cx.run_until_parked();
    assert_eq!(misspelled(&app, cx), ["secnd"]);
    app.update(cx, |app, cx| {
        let end = app.editor.read(cx).text().len();
        app.editor
            .update(cx, |editor, cx| editor.replace_range(end..end, " Wrng", cx));
    });
    cx.run_until_parked();
    assert_eq!(misspelled(&app, cx), ["secnd"]);
    cx.executor()
        .advance_clock(std::time::Duration::from_secs(1));
    cx.run_until_parked();
    assert_eq!(misspelled(&app, cx), ["secnd", "Wrng"]);
}

#[gpui::test]
fn spellcheck_follows_settings(cx: &mut TestAppContext) {
    let (app, cx) = boot(cx);
    app.update(cx, |app, cx| {
        app.editor.update(cx, |editor, cx| {
            editor.set_text("THE AGREMENT, обязуютса.", cx)
        });
    });
    cx.run_until_parked();
    assert_eq!(misspelled(&app, cx), ["обязуютса"]);
    let set = |app: &Entity<DocumentView>,
               cx: &mut VisualTestContext,
               f: fn(&mut settings::SpellingConfig)| {
        app.update(cx, |app, cx| {
            let mut store = app.preferences.borrow_mut();
            f(&mut store.current.as_mut().unwrap().spelling);
            drop(store);
            cx.notify();
        });
        cx.run_until_parked();
    };
    set(&app, cx, |s| s.check_all_caps = true);
    assert_eq!(misspelled(&app, cx), ["AGREMENT", "обязуютса"]);
    set(&app, cx, |s| s.russian = false);
    assert_eq!(misspelled(&app, cx), ["AGREMENT"]);
    set(&app, cx, |s| s.enabled = false);
    assert!(misspelled(&app, cx).is_empty());
    cx.update(|_, cx| assert!(app.read(cx).spelling_control_off()));
}

#[gpui::test]
fn ignore_is_per_document_and_dictionary_words_apply_everywhere(cx: &mut TestAppContext) {
    let (first, cx) = boot(cx);
    first.update(cx, |app, cx| {
        app.editor.update(cx, |editor, cx| {
            editor.set_text("Kowalczyk met Nowakowskiego. Kowalczyk left.", cx)
        });
    });
    cx.run_until_parked();
    assert_eq!(
        misspelled(&first, cx),
        ["Kowalczyk", "Nowakowskiego", "Kowalczyk"]
    );

    // Ignore drops every instance in this document only.
    first.update(cx, |app, cx| app.spelling_action(0, "Kowalczyk".into(), cx));
    assert_eq!(misspelled(&first, cx), ["Nowakowskiego"]);
    cx.run_until_parked();
    assert_eq!(misspelled(&first, cx), ["Nowakowskiego"]);
    let second = new_document(&first, cx);
    second.update(cx, |app, cx| {
        app.editor.update(cx, |editor, cx| {
            editor.set_text("Kowalczyk and Nowakowskiego.", cx)
        });
    });
    cx.run_until_parked();
    assert_eq!(misspelled(&second, cx), ["Kowalczyk", "Nowakowskiego"]);

    // Add to dictionary applies to every document.
    second.update(cx, |app, cx| {
        app.spelling_action(1, "Nowakowskiego".into(), cx)
    });
    cx.run_until_parked();
    assert_eq!(misspelled(&second, cx), ["Kowalczyk"]);
    cx.update(|_, cx| {
        let store = second.read(cx).preferences.borrow();
        assert!(store.words.words.contains("Nowakowskiego"));
    });

    // Removing it in Settings brings the flags back.
    let panel = cx.update(|_, cx| second.read(cx).model_panel.clone());
    panel.update(cx, |panel, cx| panel.remove_word("Nowakowskiego", cx));
    cx.run_until_parked();
    assert_eq!(misspelled(&second, cx), ["Kowalczyk", "Nowakowskiego"]);
}

#[test]
fn a_missing_file_dialog_is_explained_not_silent() {
    let message = crate::document_view::chooser_unavailable(&"portal not found");
    assert!(message.starts_with("The file dialog could not open: portal not found."));
    assert_eq!(
        message.contains("xdg-desktop-portal"),
        cfg!(target_os = "linux")
    );
}

fn open_replace(
    app: &Entity<DocumentView>,
    text: &str,
    query: &str,
    with: &str,
    cx: &mut VisualTestContext,
) {
    app.update_in(cx, |app, window, cx| {
        app.editor.update(cx, |editor, cx| {
            editor.set_text(text, cx);
            editor.set_cursor(0, cx);
            editor.focus(window, cx);
        });
    });
    cx.simulate_keystrokes(if cfg!(target_os = "macos") {
        "cmd-alt-f"
    } else {
        "ctrl-h"
    });
    cx.run_until_parked();
    cx.simulate_input(query);
    app.update_in(cx, |app, window, cx| {
        window.focus(&app.markdown_replace.read(cx).focus_handle(cx), cx);
    });
    cx.simulate_input(with);
    cx.run_until_parked();
}

fn replace_all_keys() -> &'static str {
    if cfg!(target_os = "macos") {
        "cmd-enter"
    } else {
        "ctrl-alt-enter"
    }
}

#[gpui::test]
fn find_and_replace_fit_beside_replacements_at_minimum_window_size(cx: &mut TestAppContext) {
    let (app, cx) = boot(cx);
    for theme in [Theme::Dark, Theme::Light] {
        for width in [720., 1100., 1500.] {
            for replace in [false, true] {
                cx.simulate_resize(size(px(width), px(480.)));
                open_replace(&app, "the cat and the dog", "the", "a", cx);
                app.update_in(cx, |app, window, cx| {
                    app.theme.set(theme);
                    app.pii.reviewing = true;
                    if !app.replacements_open() {
                        app.toggle_replacements_panel(window, cx);
                    }
                    app.preview.visible = true;
                    app.original_selected = false;
                    app.search.replace_open = replace;
                    if replace {
                        // Replacement results are wider than the ordinary match count.
                        app.search.status = Some("Replaced 2 · skipped 1 (formatting)".into());
                    }
                    cx.notify();
                });
                // The document measures its column after the workspace/sidebar layout.
                cx.run_until_parked();
                for _ in 0..3 {
                    cx.update(|window, cx| {
                        window.refresh();
                        window.draw(cx).clear(cx);
                    });
                    cx.run_until_parked();
                }
                let panel = cx.debug_bounds("identity-panel").unwrap();
                let mut controls = vec![
                    "markdown-search-replace-toggle",
                    "markdown-search-input",
                    "markdown-search-match-case",
                    "markdown-search-whole-word",
                    "markdown-search-count",
                    "markdown-search-previous",
                    "markdown-search-next",
                    "markdown-search-close",
                ];
                if replace {
                    controls.extend([
                        "markdown-replace-input",
                        "markdown-replace-next",
                        "markdown-replace-all",
                    ]);
                }
                let mut boxes = Vec::new();
                for id in controls {
                    let bounds = cx.debug_bounds(id).unwrap();
                    assert!(
                        bounds.left() >= px(0.)
                            && bounds.right() <= panel.left()
                            && bounds.top() >= px(0.)
                            && bounds.bottom() <= px(480.),
                        "{id} {bounds:?} must fit before panel {panel:?} at {width}"
                    );
                    if id.ends_with("-input") {
                        assert!(bounds.size.width >= px(96.), "usable {id}: {bounds:?}");
                    }
                    for (other_id, other) in &boxes {
                        assert!(!bounds.intersects(other), "{id} overlaps {other_id}");
                    }
                    boxes.push((id, bounds));
                }
                for id in ["markdown-search-input", "markdown-replace-input"] {
                    if id == "markdown-replace-input" && !replace {
                        continue;
                    }
                    let field = cx.debug_bounds(id).unwrap();
                    cx.simulate_click(field.center(), Default::default());
                    cx.run_until_parked();
                    assert!(cx.update(|window, cx| {
                        let app = app.read(cx);
                        let input = if id == "markdown-search-input" {
                            &app.markdown_search
                        } else {
                            &app.markdown_replace
                        };
                        input.read(cx).focus_handle(cx).is_focused(window)
                    }));
                }
                let next = cx.debug_bounds("markdown-search-next").unwrap();
                cx.simulate_click(next.center(), Default::default());
                cx.run_until_parked();
                assert_eq!(app.read_with(cx, |app, _| app.search.active), Some(1));
                let close = cx.debug_bounds("markdown-search-close").unwrap();
                cx.simulate_click(close.center(), Default::default());
                cx.run_until_parked();
                assert!(!app.read_with(cx, |app, _| app.search.open));
            }
        }
    }
}

#[gpui::test]
fn replace_all_skips_formatting_and_one_undo_reverts_it(cx: &mut TestAppContext) {
    let (app, cx) = boot(cx);
    // The chrome fallback must not override Find/Replace's explicit binding.
    cx.update(|_, cx| bind_app_keys(cx));
    let source = "hi **there**. hi there. Hi there.";
    open_replace(&app, source, "hi there", "yo", cx);
    assert!(cx.update(|_, cx| app.read(cx).search.replace_open));
    assert_eq!(cx.update(|_, cx| app.read(cx).search.matches.len()), 3);

    cx.simulate_keystrokes(replace_all_keys());
    cx.run_until_parked();
    let text = |cx: &mut VisualTestContext| {
        cx.update(|_, cx| app.read(cx).editor.read(cx).text().to_owned())
    };
    assert_eq!(text(cx), "hi **there**. yo. yo.");
    assert_eq!(
        cx.update(|_, cx| app.read(cx).search.status.clone())
            .as_deref(),
        Some("Replaced 2 · skipped 1 (formatting)")
    );

    // Straight from the Replace field, without returning to the document.
    cx.simulate_keystrokes(if cfg!(target_os = "macos") {
        "cmd-z"
    } else {
        "ctrl-z"
    });
    cx.run_until_parked();
    assert_eq!(text(cx), source, "one undo reverts Replace all");
    assert!(
        cx.update(|_, cx| app.read(cx).search.status.is_none()),
        "the result goes with the undo"
    );
}

#[gpui::test]
fn replace_moves_past_the_replacement_and_skips_formatting(cx: &mut TestAppContext) {
    let (app, cx) = boot(cx);
    open_replace(&app, "cat **c**at cat", "cat", "cats", cx);
    cx.simulate_keystrokes("enter");
    cx.run_until_parked();
    let state = |cx: &mut VisualTestContext| {
        cx.update(|_, cx| {
            let app = app.read(cx);
            (
                app.editor.read(cx).text().to_owned(),
                app.search.active,
                app.search.status.clone(),
            )
        })
    };
    let (text, active, _) = state(cx);
    assert_eq!(text, "cats **c**at cat");
    assert_eq!(active, Some(1), "the next match, not cat inside cats");
    cx.simulate_keystrokes("enter");
    cx.run_until_parked();
    let (text, active, status) = state(cx);
    assert_eq!(text, "cats **c**at cat", "formatting is passed over");
    assert_eq!(active, Some(2));
    assert_eq!(status.as_deref(), Some("Skipped: crosses formatting"));
    cx.simulate_keystrokes("enter");
    cx.run_until_parked();
    assert_eq!(state(cx).0, "cats **c**at cats");
}

#[gpui::test]
fn stepping_after_a_skip_brings_the_counter_back(cx: &mut TestAppContext) {
    // ADR 0064: the live counter, not a stale replace result, follows Next.
    let (app, cx) = boot(cx);
    open_replace(&app, "**c**at cat cat", "cat", "cats", cx);
    cx.simulate_keystrokes("enter");
    cx.run_until_parked();
    let status = |cx: &mut VisualTestContext| cx.update(|_, cx| app.read(cx).search.status.clone());
    assert_eq!(status(cx).as_deref(), Some("Skipped: crosses formatting"));
    let before = cx.update(|_, cx| app.read(cx).search.active);
    app.update_in(cx, |app, window, cx| {
        app.find_next_markdown(&FindNextMarkdown, window, cx)
    });
    cx.run_until_parked();
    assert_eq!(status(cx), None);
    assert_ne!(cx.update(|_, cx| app.read(cx).search.active), before);
}

#[gpui::test]
fn whole_word_and_match_case_toggle_from_the_keyboard(cx: &mut TestAppContext) {
    let (app, cx) = boot(cx);
    open_replace(&app, "Cat cat category", "cat", "", cx);
    let count = |cx: &mut VisualTestContext| cx.update(|_, cx| app.read(cx).search.matches.len());
    assert_eq!(count(cx), 3);
    let (word, case) = if cfg!(target_os = "macos") {
        ("cmd-alt-w", "cmd-alt-c")
    } else {
        ("alt-w", "alt-c")
    };
    cx.simulate_keystrokes(word);
    cx.run_until_parked();
    assert_eq!(count(cx), 2);
    cx.simulate_keystrokes(case);
    cx.run_until_parked();
    assert_eq!(count(cx), 1);
    cx.simulate_keystrokes(replace_all_keys());
    cx.run_until_parked();
    assert_eq!(
        cx.update(|_, cx| app.read(cx).editor.read(cx).text().to_owned()),
        "Cat  category"
    );
}

#[gpui::test]
fn find_hides_the_replace_row_and_the_toggle_shows_it(cx: &mut TestAppContext) {
    let (app, cx) = boot(cx);
    cx.dispatch_action(FindMarkdown);
    cx.run_until_parked();
    assert!(!cx.update(|_, cx| app.read(cx).search.replace_open));
    cx.update(|window, cx| window.draw(cx).clear(cx));
    assert!(cx.debug_bounds("markdown-replace-row").is_none());
    app.update_in(cx, |app, window, cx| {
        app.toggle_markdown_replace(window, cx)
    });
    cx.run_until_parked();
    assert!(cx.update(|window, cx| {
        app.read(cx)
            .markdown_replace
            .read(cx)
            .focus_handle(cx)
            .is_focused(window)
    }));
}

#[gpui::test]
fn replace_twice_before_a_background_search_settles(cx: &mut TestAppContext) {
    let (app, cx) = boot(cx);
    // Past the 64 KiB background threshold.
    let text = format!("prefix cat cat\n\n{}", "padding ".repeat(10_000));
    open_replace(&app, &text, "cat", "cats", cx);
    app.update_in(cx, |app, window, cx| {
        app.replace_next_markdown(&ReplaceNextMarkdown, window, cx);
        app.replace_next_markdown(&ReplaceNextMarkdown, window, cx);
    });
    cx.run_until_parked();
    let edited = cx.update(|_, cx| app.read(cx).editor.read(cx).text().to_owned());
    assert!(
        edited.starts_with("prefix cats cats\n"),
        "{}",
        &edited[..20]
    );
}

#[gpui::test]
fn replace_after_a_skip_takes_the_highlighted_match_while_a_search_runs(cx: &mut TestAppContext) {
    let (app, cx) = boot(cx);
    let text = format!("**c**at cat cat\n\n{}", "padding ".repeat(10_000));
    open_replace(&app, &text, "cat", "dog", cx);
    // An edit at the end queues a background search, left running below.
    app.update(cx, |app, cx| {
        app.editor.update(cx, |editor, cx| {
            let end = editor.text().len();
            editor.replace_range(end..end, "!", cx);
            editor.set_cursor(0, cx);
        });
    });
    assert!(cx.update(|_, cx| app.read(cx).search.task.is_some()));
    app.update_in(cx, |app, window, cx| {
        app.replace_next_markdown(&ReplaceNextMarkdown, window, cx);
        assert_eq!(
            app.search.status.as_deref(),
            Some("Skipped: crosses formatting")
        );
        app.replace_next_markdown(&ReplaceNextMarkdown, window, cx);
        app.replace_next_markdown(&ReplaceNextMarkdown, window, cx);
    });
    cx.run_until_parked();
    let edited = cx.update(|_, cx| app.read(cx).editor.read(cx).text().to_owned());
    assert!(edited.starts_with("**c**at dog dog\n"), "{}", &edited[..20]);
}

#[gpui::test]
fn find_shows_the_markdown_pane_in_a_narrow_window(cx: &mut TestAppContext) {
    let (app, cx) = boot(cx);
    app.update(cx, |app, _| app.original_selected = true);
    cx.dispatch_action(FindReplaceMarkdown);
    cx.run_until_parked();
    assert!(!cx.update(|_, cx| app.read(cx).original_selected));
}

/// Close a dirty tab with mdoc's confirm dialog installed (ADR 0064).
fn ask_to_close(cx: &mut TestAppContext) -> (Entity<DocumentView>, &mut VisualTestContext) {
    let (app, cx) = boot(cx);
    cx.update(|_, cx| {
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
    app.update(cx, |_, cx| cx.emit(workspace::TabEvent::Open(vec![path])));
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
    app.update(cx, |_, cx| cx.emit(workspace::TabEvent::Open(vec![path])));
    app.update(cx, |_, cx| cx.emit(workspace::TabEvent::CloseRequested));
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

#[test]
fn find_counter_speaks_its_position() {
    // ADR 0064: the counter shows "14 / 16" and screen readers read words.
    let spoken = |empty, searching, active, matches| {
        let (shown, spoken) = find_counter(empty, searching, active, matches);
        (shown, spoken.to_string())
    };
    assert_eq!(
        spoken(false, false, Some(13), 16),
        ("14 / 16".into(), "Match 14 of 16".into())
    );
    assert_eq!(
        spoken(false, false, None, 1),
        ("0 / 1".into(), "1 match".into())
    );
    assert_eq!(
        spoken(false, false, None, 3),
        ("0 / 3".into(), "3 matches".into())
    );
    assert_eq!(
        spoken(false, false, None, 0),
        ("0 matches".into(), "No matches".into())
    );
    assert_eq!(spoken(false, true, None, 0).1, "Searching…");
    assert_eq!(
        spoken(true, false, None, 0),
        ("0 / 0".into(), "No search".into())
    );
}
