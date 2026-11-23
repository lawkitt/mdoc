//! The menu bar shelf's list and handoff: what it lists from the
//! window's tabs and Projects, and that nothing leaves it unless ready.
use super::*;
use crate::shelf::{Readiness, ShelfExports, ShelfKey};
use mdoc_pii::Category::{Organization, Person};

const ORIGINALS: [&str; 5] = [
    "Anna Petrova",
    "Northwind Ltd",
    "Boris Ivanov",
    "Ivan Ivanov",
    "Clara Moss",
];

/// Done when: ready and not-ready documents are copied and dragged from the
/// shelf, and no export holds an original.
#[gpui::test]
fn the_shelf_hands_off_only_ready_documents_and_never_an_original(cx: &mut TestAppContext) {
    let dir = tempfile::tempdir().unwrap();
    let (tabs, cx) = boot(cx, Session::default());
    cx.simulate_resize(size(px(1400.), px(850.)));

    // Ready and open, in a Project.
    let ready = open_applied(
        &tabs,
        cx,
        dir.path(),
        "Anna Petrova contract.md",
        "Anna Petrova met Northwind Ltd.\n\nThe second line.",
        &[("Anna Petrova", Person), ("Northwind Ltd", Organization)],
    );
    let project = new_project(&tabs, cx, ready);

    // Not ready: open, moved in, with the Project's names still proposed.
    let pending = open_applied(
        &tabs,
        cx,
        dir.path(),
        "Boris Ivanov memo.md",
        "Boris Ivanov wrote to Anna Petrova.",
        &[("Boris Ivanov", Person)],
    );
    move_tab(&tabs, cx, pending, Some(&project));

    let rows = cx.update(|_, cx| tabs.read(cx).shelf_documents(cx));
    let state = |key: &ShelfKey| {
        rows.iter()
            .find(|row| &row.key == key)
            .unwrap_or_else(|| panic!("{key:?} is listed"))
            .readiness
    };
    assert_eq!(
        rows.len(),
        2,
        "only open tabs until the shelf reads the Vault"
    );
    assert_eq!(state(&ShelfKey::Tab(ready)), Readiness::Ready);
    assert_eq!(state(&ShelfKey::Tab(pending)), Readiness::LeftToReview(1));
    assert!(
        rows.iter()
            .all(|row| row.project.as_ref().unwrap().id == project)
    );

    // Copy for AI is the editor's bytes for an open tab.
    let ready_view = cx.update(|_, cx| {
        tabs.read(cx)
            .tabs
            .get(ready)
            .unwrap()
            .view()
            .cloned()
            .unwrap()
    });
    let editor_copy = cx.update(|_, cx| ready_view.read(cx).text_for_ai(cx));
    let ready_row = rows
        .iter()
        .find(|row| row.key == ShelfKey::Tab(ready))
        .unwrap();
    assert_eq!(ready_row.text_for_ai(), Some(editor_copy.as_str()));

    // Copy and drag every row; only ready ones carry anything.
    let exports = ShelfExports::create_in(dir.path()).unwrap();
    let mut handed = Vec::new();
    for row in &rows {
        let copied = row.text_for_ai().map(str::to_owned);
        let dragged = exports.export(row).unwrap();
        assert_eq!(copied.is_some(), row.readiness.is_ready(), "{:?}", row.key);
        assert_eq!(dragged.is_some(), row.readiness.is_ready(), "{:?}", row.key);
        if let Some(path) = dragged {
            let file = std::fs::read_to_string(&path).unwrap();
            assert_eq!(
                Some(&file),
                copied.as_ref(),
                "a drag carries Copy for AI's text"
            );
            assert!(copied.as_ref().unwrap().contains("PERSON_1"));
            handed.push(path.file_name().unwrap().to_string_lossy().into_owned());
            handed.push(file);
        }
        // What the shelf shows is handed to anyone watching the screen.
        handed.push(row.title.clone());
        handed.extend(row.project_label.clone());
        handed.extend(row.preview.iter().map(|line| line.text.clone()));
        // The drag card shows under the pointer, in other apps too.
        let card = row.drag_card();
        assert_eq!(card.is_some(), row.readiness.is_ready(), "{:?}", row.key);
        handed.extend(card.into_iter().flat_map(|(title, line)| [title, line]));
        handed.extend(copied);
    }
    assert!(handed.iter().any(|h| h == "PERSON_1 contract.md"));
    for original in ORIGINALS {
        for text in &handed {
            assert!(
                !text.contains(original),
                "{original} left the shelf in {text:?}"
            );
        }
    }
    exports.clear();
}

/// A write records the Ready text's fingerprint in the Vault document, and
/// a write while not Ready forgets it; the outbound check then refuses it.
#[gpui::test]
fn a_write_records_whether_the_document_is_ready(cx: &mut TestAppContext) {
    let dir = tempfile::tempdir().unwrap();
    let (tabs, cx) = boot(cx, Session::default());
    let id = open_applied(
        &tabs,
        cx,
        dir.path(),
        "a.md",
        "Anna Petrova met Northwind Ltd.",
        &[("Anna Petrova", Person), ("Northwind Ltd", Organization)],
    );
    new_project(&tabs, cx, id);
    let document = cx.update(|_, cx| tabs.read(cx).tab_document(id, cx).unwrap());
    tabs.update_in(cx, |tabs, window, cx| tabs.autosave(id, window, cx));
    cx.run_until_parked();
    let vault = vault(&tabs, cx);
    let stored = vault.document(document.as_str()).unwrap();
    assert!(stored.scanned && stored.ready_fingerprint.is_some());
    assert!(vault.readiness(document.as_str()).unwrap().is_ready());

    // A new proposal makes it not ready; its next write forgets it.
    let view = active(&tabs, cx);
    view.update(cx, |view, cx| {
        let text = view.editor.read(cx).text().to_owned();
        let start = text.find("met").unwrap();
        view.pii
            .review_mut()
            .add_manual(&text, start..start + 3, Person)
            .unwrap();
        view.editor.update(cx, |editor, cx| {
            let end = editor.text().len();
            editor.replace_range(end..end, " Later.", cx);
        });
    });
    cx.run_until_parked();
    tabs.update_in(cx, |tabs, window, cx| tabs.autosave(id, window, cx));
    cx.run_until_parked();
    assert_eq!(
        vault.document(document.as_str()).unwrap().ready_fingerprint,
        None
    );
    assert!(!vault.readiness(document.as_str()).unwrap().is_ready());
}

/// #304: the row's "N replaced" is the editor's Share tab count, mentions
/// rather than names.
#[gpui::test]
fn the_shelf_counts_replaced_mentions_as_the_share_tab_does(cx: &mut TestAppContext) {
    let dir = tempfile::tempdir().unwrap();
    let (tabs, cx) = boot(cx, Session::default());
    let id = open_applied(
        &tabs,
        cx,
        dir.path(),
        "Notes.md",
        "Anna Petrova met Northwind Ltd. Anna Petrova signed for Northwind Ltd.",
        &[("Anna Petrova", Person), ("Northwind Ltd", Organization)],
    );
    let view = active(&tabs, cx);
    let applied = cx.update(|_, cx| view.read(cx).pii.review().applied().len());
    let row = row_of(&tabs, id, cx);
    assert_eq!(applied, 4);
    assert_eq!(row.replaced_count(), applied);
    assert_eq!(row.names_count(), 2);
}

/// Review findings on #263: a kept original and a scan still finding
/// proposals after an edit each keep an open tab from leaving the shelf,
/// and a kept name stays masked in the title.
#[gpui::test]
fn kept_names_and_scans_in_flight_keep_an_open_tab_on_the_shelf(cx: &mut TestAppContext) {
    let dir = tempfile::tempdir().unwrap();
    let (tabs, cx) = boot(cx, Session::default());
    let path = dir.path().join("Clara Moss.md");
    let source = "Anna Petrova met Clara Moss.";
    std::fs::write(&path, source).unwrap();
    tabs.update_in(cx, |tabs, window, cx| {
        tabs.open_paths(vec![path], window, cx)
    });
    cx.run_until_parked();
    let view = active(&tabs, cx);
    let id = cx.update(|_, cx| tabs.read(cx).tabs.active());
    view.update_in(cx, |view, window, cx| {
        view.pii.set_reviewing(true);
        view.pii
            .review_mut()
            .add_manual(source, 0..12, Person)
            .unwrap();
        let clara = view
            .pii
            .review_mut()
            .add_manual(source, 17..27, Person)
            .unwrap();
        view.pii.review_mut().keep(clara, None);
        view.apply_all_pii(&PiiApplyAll, window, cx);
    });
    cx.run_until_parked();
    let kept = row_of(&tabs, id, cx);
    assert_eq!(
        cx.update(|_, cx| view.read(cx).editor.read(cx).text().to_owned()),
        "PERSON_1 met Clara Moss."
    );
    assert_eq!(kept.readiness, Readiness::NamesNotReplaced(1));
    assert_eq!(kept.text_for_ai(), None);
    assert!(!kept.title.contains("Clara Moss"), "{}", kept.title);

    // A ready document gets a new proposal, then an unrelated edit: until
    // the scan finds the proposal again, it is not ready.
    let id = open_applied(
        &tabs,
        cx,
        dir.path(),
        "b.md",
        "Anna Petrova met Boris.",
        &[("Anna Petrova", Person)],
    );
    let view = active(&tabs, cx);
    assert_eq!(row_of(&tabs, id, cx).readiness, Readiness::Ready);
    view.update(cx, |view, cx| {
        let text = view.editor.read(cx).text().to_owned();
        let start = text.find("Boris").unwrap();
        view.pii
            .review_mut()
            .add_manual(&text, start..start + 5, Person)
            .unwrap();
        view.editor.update(cx, |editor, cx| {
            let end = editor.text().len();
            editor.replace_range(end..end, " Later.", cx);
        });
    });
    let checking = row_of(&tabs, id, cx).readiness;
    assert_eq!(checking, Readiness::Checking);
    cx.run_until_parked();
    assert_eq!(row_of(&tabs, id, cx).readiness, Readiness::LeftToReview(1));
}

fn row_of(
    tabs: &Entity<Workspace>,
    id: u64,
    cx: &mut VisualTestContext,
) -> crate::shelf::ShelfDocument {
    cx.update(|_, cx| {
        tabs.read(cx)
            .shelf_documents(cx)
            .into_iter()
            .find(|row| row.key == ShelfKey::Tab(id))
            .unwrap()
    })
}

fn open_shelf(cx: &mut VisualTestContext) -> gpui::WindowHandle<crate::shelf_ui::ShelfView> {
    use crate::shelf_ui::{ShelfContent, ShelfView};
    let workspace = cx.window_handle().downcast::<Workspace>().unwrap();
    let shelf = cx.cx.update(|cx| {
        let content = workspace
            .read(cx)
            .map(|workspace| ShelfContent::of(workspace, cx))
            .unwrap();
        cx.open_window(gpui::WindowOptions::default(), |window, cx| {
            cx.new(|cx| ShelfView::new(workspace, content, window, cx))
        })
        .unwrap()
    });
    cx.run_until_parked();
    shelf
}

/// The shelf's own controls: Copy for AI and a drag carry the
/// editor's bytes for a ready row and nothing for one that is not; Restore
/// Clipboard uses the current case; a click brings the tab forward.
#[gpui::test]
fn the_shelf_copies_drags_restores_and_opens(cx: &mut TestAppContext) {
    let dir = tempfile::tempdir().unwrap();
    let (tabs, cx) = boot(cx, Session::default());
    let ready = open_applied(
        &tabs,
        cx,
        dir.path(),
        "Anna Petrova contract.md",
        "Anna Petrova met Northwind Ltd.",
        &[("Anna Petrova", Person), ("Northwind Ltd", Organization)],
    );
    let project = new_project(&tabs, cx, ready);
    let pending = open_applied(
        &tabs,
        cx,
        dir.path(),
        "memo.md",
        "Boris Ivanov wrote to Anna Petrova.",
        &[("Boris Ivanov", Person)],
    );
    move_tab(&tabs, cx, pending, Some(&project));
    let editor_copy = cx.update(|_, cx| {
        let view = tabs
            .read(cx)
            .tabs
            .get(ready)
            .unwrap()
            .view()
            .cloned()
            .unwrap();
        view.read(cx).text_for_ai(cx)
    });

    let shelf = open_shelf(cx);
    let index = |key: ShelfKey, cx: &mut VisualTestContext| {
        shelf
            .read_with(cx, |view, _| {
                view.content()
                    .documents
                    .iter()
                    .position(|d| d.key == key)
                    .unwrap()
            })
            .unwrap()
    };
    let ready_row = index(ShelfKey::Tab(ready), cx);
    let pending_row = index(ShelfKey::Tab(pending), cx);
    let label = shelf
        .read_with(cx, |view, _| {
            crate::shelf_ui::row_label(&view.content().documents[pending_row])
        })
        .unwrap();
    // The case is named after its first document: masked too.
    assert_eq!(label, "memo, PERSON_1 contract, 1 left to review");
    let names_from = shelf
        .read_with(cx, |view, _| view.source_line().unwrap())
        .unwrap();
    assert!(!names_from.contains("Anna"), "{names_from}");

    // Not ready: no copy, no drag.
    cx.write_to_clipboard(gpui::ClipboardItem::new_string("before".into()));
    shelf
        .update(cx, |view, window, cx| {
            view.copy(pending_row, window, cx);
            view.drag(pending_row, window, cx);
            assert!(view.dragged.is_none());
        })
        .unwrap();
    assert_eq!(cx.read_from_clipboard().unwrap().text().unwrap(), "before");

    // Ready: the editor's Copy for AI bytes, on the clipboard and in a drag.
    shelf
        .update(cx, |view, window, cx| {
            view.copy(ready_row, window, cx);
            view.drag(ready_row, window, cx);
        })
        .unwrap();
    assert_eq!(
        cx.read_from_clipboard().unwrap().text().unwrap(),
        editor_copy
    );
    let (dragged, file) = shelf
        .read_with(cx, |view, _| view.dragged.clone().unwrap())
        .unwrap();
    assert_eq!(dragged, editor_copy);
    assert_eq!(std::fs::read_to_string(&file).unwrap(), editor_copy);
    assert_eq!(file.file_name().unwrap(), "PERSON_1 contract.md");

    // Restore Clipboard with the case of the document used last.
    cx.write_to_clipboard(gpui::ClipboardItem::new_string(
        "PERSON_1 owes ORG_1; PERSON_9 is new.".into(),
    ));
    shelf
        .update(cx, |view, window, cx| {
            view.clipboard(crate::shelf_ui::ClipboardRow::Restore, window, cx)
        })
        .unwrap();
    assert_eq!(
        cx.read_from_clipboard().unwrap().text().unwrap(),
        "Anna Petrova owes Northwind Ltd; PERSON_9 is new."
    );
    assert!(crate::restored_clipboard::is_restored(
        &cx.read_from_clipboard().unwrap()
    ));

    // A click on the not-ready row brings its tab forward at its proposal.
    shelf
        .update(cx, |view, window, cx| view.open(pending_row, window, cx))
        .unwrap();
    cx.run_until_parked();
    assert_eq!(cx.update(|_, cx| tabs.read(cx).tabs.active()), pending);
    assert!(
        cx.update(|_, cx| !cx
            .windows()
            .iter()
            .any(|w| w.window_id() == shelf.window_id())),
        "the shelf closes"
    );
    assert_first_proposal_shown(&tabs, cx);
}

/// The active tab shows its earliest pending proposal in the popup.
fn assert_first_proposal_shown(tabs: &Entity<Workspace>, cx: &mut VisualTestContext) {
    cx.update(|_, cx| {
        let view = tabs.read(cx).active_view().unwrap();
        let view = view.read(cx);
        let first = view
            .pii
            .review()
            .candidates()
            .iter()
            .map(|c| c.range.start)
            .min()
            .expect("a pending proposal");
        assert!(view.pii.popup().is_some(), "the proposal's popup is open");
        assert_eq!(view.editor.read(cx).cursor(), first);
    });
}

/// Review of #265: every shelf control is reachable by keyboard, and the
/// picker keeps focus inside the shelf.
#[gpui::test]
fn the_shelf_is_reachable_by_keyboard_and_the_picker_keeps_focus(cx: &mut TestAppContext) {
    let dir = tempfile::tempdir().unwrap();
    let (tabs, cx) = boot(cx, Session::default());
    // Two sources: a case, and a tab without one through its mapping.
    let cased = open_applied(
        &tabs,
        cx,
        dir.path(),
        "a.md",
        "Anna Petrova met Northwind Ltd.",
        &[("Anna Petrova", Person), ("Northwind Ltd", Organization)],
    );
    new_project(&tabs, cx, cased);
    open_applied(
        &tabs,
        cx,
        dir.path(),
        "b.md",
        "Clara Moss wrote.",
        &[("Clara Moss", Person)],
    );
    cx.update(|_, cx| crate::shelf_ui::bind_keys(cx));
    let shelf = open_shelf(cx);
    let mut shelf_cx = VisualTestContext::from_window(shelf.into(), cx);
    let focus = |cx: &mut VisualTestContext| {
        cx.update(|window, cx| {
            assert!(
                window.context_stack().iter().any(|c| c.contains("Shelf")),
                "focus stays in the shelf"
            );
            window.focused(cx)
        })
    };
    // Tab walks every control and comes back round.
    let mut seen = Vec::new();
    for _ in 0..24 {
        shelf_cx.simulate_keystrokes("tab");
        let now = focus(&mut shelf_cx);
        if !seen.contains(&now) {
            seen.push(now);
        }
    }
    // 2 rows, 2 Copy for AI, 2 clipboard buttons, the ▾, Open mdoc,
    // Settings and Quit.
    assert!(seen.len() >= 10, "{} stops", seen.len());

    // The picker takes focus, ↑↓ move in it, Esc gives focus back to the ▾
    // that opened it, which still shows; the shelf keeps its keys.
    let before = focus(&mut shelf_cx);
    shelf
        .update(&mut shelf_cx, |view, window, cx| {
            view.toggle_picker(window, cx)
        })
        .unwrap();
    draw(&mut shelf_cx);
    let first = focus(&mut shelf_cx);
    assert_ne!(first, before);
    shelf_cx.simulate_keystrokes("down");
    let second = focus(&mut shelf_cx);
    assert_ne!(second, first);
    shelf_cx.simulate_keystrokes("escape");
    let back = focus(&mut shelf_cx);
    assert!(back.is_some() && back != first && back != second);
    // Tab still moves on from there, and Esc now closes the shelf.
    shelf_cx.simulate_keystrokes("tab");
    assert_ne!(focus(&mut shelf_cx), back);
    assert!(
        shelf
            .read_with(&shelf_cx, |view, _| view.content().source.is_some())
            .unwrap()
    );
}

/// Polish: ⌘1–⌘8 copy that row for AI, ✓ "Copied" shows and
/// the shelf closes after about 0.8 s; a clipboard result shows on the
/// source line for a few seconds; the first drag retires the drag hint for
/// good, recorded in settings.
#[gpui::test]
fn copied_closes_the_shelf_and_hints_and_results_come_and_go(cx: &mut TestAppContext) {
    let dir = tempfile::tempdir().unwrap();
    let (tabs, cx) = boot(cx, Session::default());
    let cased = open_applied(
        &tabs,
        cx,
        dir.path(),
        "a.md",
        "Anna Petrova met Northwind Ltd about the lease renewal terms.",
        &[("Anna Petrova", Person), ("Northwind Ltd", Organization)],
    );
    new_project(&tabs, cx, cased);
    open_applied(
        &tabs,
        cx,
        dir.path(),
        "b.md",
        "Clara Moss wrote to the landlord about the deposit.",
        &[("Clara Moss", Person)],
    );
    cx.update(|_, cx| crate::shelf_ui::bind_keys(cx));
    let shelf = open_shelf(cx);
    let open = |shelf: gpui::WindowHandle<crate::shelf_ui::ShelfView>,
                cx: &mut VisualTestContext| {
        cx.update(|_, cx| {
            cx.windows()
                .iter()
                .any(|w| w.window_id() == shelf.window_id())
        })
    };
    let (second, hint) = shelf
        .read_with(cx, |view, _| {
            let content = view.content();
            assert!(content.documents.iter().all(|d| d.preview.is_some()));
            (
                content.documents[1].text_for_ai().unwrap().to_string(),
                content.drag_hint,
            )
        })
        .unwrap();
    assert!(hint, "the hint shows before the first drag");
    // A tab without a case is named without `.md` (GUI run of #280).
    let sources = shelf
        .read_with(cx, |view, _| view.content().sources.clone())
        .unwrap();
    assert!(
        sources.iter().any(
            |s| matches!(s, crate::workspace::ClipboardSource::Tab { title, .. } if title == "b")
        ),
        "{sources:?}"
    );

    // ⌘1–⌘8 are bound on macOS, where the shelf exists; elsewhere the
    // action itself runs.
    let copy_row = |row: usize, cx: &mut VisualTestContext| {
        if cfg!(target_os = "macos") {
            cx.simulate_keystrokes(&format!("cmd-{row}"));
        } else {
            cx.dispatch_action(crate::shelf_ui::ShelfCopyRow(row));
        }
    };
    let mut shelf_cx = VisualTestContext::from_window(shelf.into(), cx);
    copy_row(2, &mut shelf_cx);
    assert_eq!(
        shelf_cx.read_from_clipboard().unwrap().text().unwrap(),
        second
    );
    assert_eq!(
        shelf.read_with(&shelf_cx, |view, _| view.copied()).unwrap(),
        Some(1)
    );
    assert!(open(shelf, cx), "Copied shows first");
    cx.executor().advance_clock(crate::shelf_ui::COPIED_FOR);
    cx.run_until_parked();
    assert!(!open(shelf, cx), "then the shelf closes");

    // A clipboard result stays a few seconds, then the source returns.
    let shelf = open_shelf(cx);
    cx.write_to_clipboard(gpui::ClipboardItem::new_string(
        "PERSON_1 called ORG_1.".into(),
    ));
    let source = shelf
        .read_with(cx, |view, _| view.source_line().unwrap())
        .unwrap();
    assert!(source.starts_with("Names from "), "{source}");
    shelf
        .update(cx, |view, window, cx| {
            view.clipboard(crate::shelf_ui::ClipboardRow::Restore, window, cx)
        })
        .unwrap();
    let line = |cx: &mut VisualTestContext| {
        shelf
            .read_with(cx, |view, _| view.source_line().unwrap())
            .unwrap()
    };
    assert!(
        line(cx).ends_with(" restored · 0 unrecognized"),
        "{}",
        line(cx)
    );
    cx.executor().advance_clock(crate::shelf_ui::RESULT_FOR);
    cx.run_until_parked();
    assert_eq!(line(cx), source);

    // ⌘9 is no row. A drag shows the row's card; a cancelled one keeps the
    // hint, and the first drop retires it for good.
    let mut shelf_cx = VisualTestContext::from_window(shelf.into(), cx);
    copy_row(9, &mut shelf_cx);
    assert_eq!(
        shelf.read_with(&shelf_cx, |view, _| view.copied()).unwrap(),
        None
    );
    let hint = |cx: &mut VisualTestContext| {
        shelf
            .read_with(cx, |view, _| view.content().drag_hint)
            .unwrap()
    };
    shelf
        .update(cx, |view, window, cx| {
            view.drag(0, window, cx);
            let card = view.dragged_card.clone().unwrap();
            assert_eq!(Some(card), view.content().documents[0].drag_card());
            view.drag_ended(false, cx);
        })
        .unwrap();
    assert!(hint(cx), "a cancelled drag keeps the hint");
    assert!(!cx.update(|_, cx| tabs.read(cx).shelf_drag_hint_shown()));
    // Released over the shelf itself (GUI run of #280): AppKit reports a
    // drop, but nothing left mdoc. (The shelf's drop handler calls
    // `dropped_on_shelf`; the native event is a GUI check.)
    shelf
        .update(cx, |view, window, cx| {
            view.drag(0, window, cx);
            view.dropped_on_shelf();
            view.drag_ended(true, cx);
        })
        .unwrap();
    assert!(hint(cx), "a drop on the shelf keeps the hint");
    assert!(!cx.update(|_, cx| tabs.read(cx).shelf_drag_hint_shown()));
    shelf
        .update(cx, |view, window, cx| {
            view.drag(0, window, cx);
            view.drag_ended(true, cx);
        })
        .unwrap();
    assert!(!hint(cx));
    assert!(cx.update(|_, cx| tabs.read(cx).shelf_drag_hint_shown()));
    shelf
        .update(cx, |_, window, _| window.remove_window())
        .unwrap();
    let again = open_shelf(cx);
    assert!(
        !again
            .read_with(cx, |view, _| view.content().drag_hint)
            .unwrap()
    );
}

/// GUI run of #280: the popup follows its content's measured height, the
/// picker and a retired hint included, never past the room under the item.
#[gpui::test]
fn the_shelf_popup_follows_its_content_height(cx: &mut TestAppContext) {
    let dir = tempfile::tempdir().unwrap();
    let (tabs, cx) = boot(cx, Session::default());
    let cased = open_applied(
        &tabs,
        cx,
        dir.path(),
        "a.md",
        "Anna Petrova met Northwind Ltd about the lease renewal terms.",
        &[("Anna Petrova", Person), ("Northwind Ltd", Organization)],
    );
    new_project(&tabs, cx, cased);
    open_applied(
        &tabs,
        cx,
        dir.path(),
        "b.md",
        "Clara Moss wrote to the landlord about the deposit.",
        &[("Clara Moss", Person)],
    );
    let shelf = open_shelf(cx);
    let estimate = shelf
        .update(cx, |view, _, _| {
            let estimate = view.content().height();
            view.fit_to_screen(estimate, 10_000.);
            estimate
        })
        .unwrap();
    let mut shelf_cx = VisualTestContext::from_window(shelf.into(), cx);
    let asked = |cx: &mut VisualTestContext| {
        draw(cx);
        cx.run_until_parked();
        shelf
            .read_with(cx, |view, _| view.fit().unwrap().asked())
            .unwrap()
    };
    let full = asked(&mut shelf_cx);
    assert!(
        (full - estimate).abs() <= 1.,
        "measured {full}, estimated {estimate}"
    );

    // The picker replaces the list; closing it brings the list back.
    shelf
        .update(&mut shelf_cx, |view, window, cx| {
            view.toggle_picker(window, cx)
        })
        .unwrap();
    let picking = asked(&mut shelf_cx);
    assert!(picking < full, "{picking} < {full}");
    shelf
        .update(&mut shelf_cx, |view, window, cx| {
            view.toggle_picker(window, cx)
        })
        .unwrap();
    assert_eq!(asked(&mut shelf_cx), full);

    // A retired hint gives its row back.
    shelf
        .update(&mut shelf_cx, |view, window, cx| {
            view.drag(0, window, cx);
            view.drag_ended(true, cx);
        })
        .unwrap();
    let without_hint = asked(&mut shelf_cx);
    assert!(without_hint < full, "{without_hint} < {full}");

    // Never past the room under the item: the shelf scrolls instead.
    shelf
        .update(&mut shelf_cx, |view, _, _| view.fit_to_screen(full, 120.))
        .unwrap();
    assert_eq!(asked(&mut shelf_cx), 120.);
}
