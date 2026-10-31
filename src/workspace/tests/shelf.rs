//! The menu bar shelf's list and handoff (ADR 0084): what it lists from the
//! window's tabs and cases, and that nothing leaves it unless ready.
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

fn save(tabs: &Entity<Workspace>, id: u64, cx: &mut VisualTestContext) {
    let view = cx.update(|_, cx| tabs.read(cx).tabs.get(id).unwrap().view().cloned().unwrap());
    let path = cx.update(|_, cx| view.read(cx).session.document.path.clone().unwrap());
    view.update(cx, |view, cx| view.write(path, None, cx));
    cx.run_until_parked();
}

fn join_and_apply(tabs: &Entity<Workspace>, id: u64, case: &str, cx: &mut VisualTestContext) {
    tabs.update_in(cx, |tabs, window, cx| tabs.join_case(id, case, window, cx));
    cx.run_until_parked();
    let view = active(tabs, cx);
    view.update_in(cx, |view, window, cx| {
        view.apply_all_pii(&PiiApplyAll, window, cx)
    });
    cx.run_until_parked();
}

/// Done when (ADR 0084): ready, not-ready and changed-since-review
/// documents are copied and dragged from the shelf, and no export holds an
/// original.
#[gpui::test]
fn the_shelf_hands_off_only_ready_documents_and_never_an_original(cx: &mut TestAppContext) {
    let dir = tempfile::tempdir().unwrap();
    let (tabs, cx) = boot(cx, Session::default());
    cx.simulate_resize(size(px(1400.), px(850.)));
    tabs.update(cx, |tabs, _| {
        tabs.use_case_store(mdoc_store::cases::Store::with_key(
            dir.path().join("cases"),
            &[3; 32],
        ))
    });

    // Ready and open: saved while ready, then made a case.
    let ready = open_applied(
        &tabs,
        cx,
        dir.path(),
        "Anna Petrova contract.md",
        "Anna Petrova met Northwind Ltd.\n\nThe second line.",
        &[("Anna Petrova", Person), ("Northwind Ltd", Organization)],
    );
    save(&tabs, ready, cx);
    tabs.update_in(cx, |tabs, window, cx| tabs.create_case(ready, window, cx));
    cx.run_until_parked();
    let case = cx.update(|_, cx| tabs.read(cx).case_coordinator.cases()[0].id.clone());

    // Ready and closed, untouched since its save.
    let closed = open_applied(
        &tabs,
        cx,
        dir.path(),
        "Clara Moss notes.md",
        "Clara Moss paid Anna Petrova.",
        &[("Clara Moss", Person)],
    );
    join_and_apply(&tabs, closed, &case, cx);
    save(&tabs, closed, cx);
    tabs.update_in(cx, |tabs, window, cx| tabs.close_tab(closed, window, cx));
    cx.run_until_parked();

    // Changed since review: saved while ready, closed, then edited elsewhere.
    let changed = open_applied(
        &tabs,
        cx,
        dir.path(),
        "Ivan Ivanov settlement.md",
        "Ivan Ivanov owes Northwind Ltd.",
        &[("Ivan Ivanov", Person)],
    );
    join_and_apply(&tabs, changed, &case, cx);
    save(&tabs, changed, cx);
    let changed_path = cx.update(|_, cx| {
        tabs.read(cx)
            .tabs
            .get(changed)
            .unwrap()
            .view()
            .unwrap()
            .read(cx)
            .session
            .document
            .path
            .clone()
            .unwrap()
    });
    tabs.update_in(cx, |tabs, window, cx| tabs.close_tab(changed, window, cx));
    cx.run_until_parked();
    let mut text = std::fs::read_to_string(&changed_path).unwrap();
    text.push_str("\n\nA paragraph added in another editor.");
    std::fs::write(&changed_path, &text).unwrap();

    // Names not replaced: a closed member that gained an original.
    let missed = open_applied(
        &tabs,
        cx,
        dir.path(),
        "letter.md",
        "Northwind Ltd wrote again.",
        &[("Northwind Ltd", Organization)],
    );
    join_and_apply(&tabs, missed, &case, cx);
    save(&tabs, missed, cx);
    let missed_path = dir.path().join("letter.md");
    tabs.update_in(cx, |tabs, window, cx| tabs.close_tab(missed, window, cx));
    cx.run_until_parked();
    let mut text = std::fs::read_to_string(&missed_path).unwrap();
    text.push_str("\n\nCopy to Anna Petrova.");
    std::fs::write(&missed_path, &text).unwrap();

    // Not ready: open, joined, with the case's names still proposed.
    let pending = open_applied(
        &tabs,
        cx,
        dir.path(),
        "Boris Ivanov memo.md",
        "Boris Ivanov wrote to Anna Petrova.",
        &[("Boris Ivanov", Person)],
    );
    tabs.update_in(cx, |tabs, window, cx| {
        tabs.join_case(pending, &case, window, cx)
    });
    cx.run_until_parked();

    let rows = cx.update(|_, cx| tabs.read(cx).shelf_documents(cx));
    let state = |key: &ShelfKey| {
        rows.iter()
            .find(|row| &row.key == key)
            .unwrap_or_else(|| panic!("{key:?} is listed"))
            .readiness
    };
    let member = |name: &str| ShelfKey::Member {
        case: case.clone(),
        path: session_store::identity(&dir.path().join(name)),
    };
    assert_eq!(rows.len(), 5);
    // Open tabs first, most recently edited first; then closed members.
    assert_eq!(rows[0].key, ShelfKey::Tab(pending));
    assert_eq!(rows[1].key, ShelfKey::Tab(ready));
    assert!(
        rows[2..]
            .iter()
            .all(|r| matches!(r.key, ShelfKey::Member { .. }))
    );
    assert_eq!(state(&ShelfKey::Tab(ready)), Readiness::Ready);
    assert_eq!(state(&ShelfKey::Tab(pending)), Readiness::LeftToReview(1));
    assert_eq!(state(&member("Clara Moss notes.md")), Readiness::Ready);
    assert_eq!(
        state(&member("Ivan Ivanov settlement.md")),
        Readiness::ChangedSinceReview
    );
    assert_eq!(state(&member("letter.md")), Readiness::NamesNotReplaced(1));

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
    assert_eq!(rows[1].text_for_ai(), Some(editor_copy.as_str()));

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
        handed.extend(row.preview.iter().map(|line| line.text.clone()));
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

/// Saving records the ready text's fingerprint in the case, and saving
/// while not ready forgets it; the case keeps no text.
#[gpui::test]
fn saving_records_the_ready_fingerprint_in_the_case(cx: &mut TestAppContext) {
    let dir = tempfile::tempdir().unwrap();
    let (tabs, cx) = boot(cx, Session::default());
    tabs.update(cx, |tabs, _| {
        tabs.use_case_store(mdoc_store::cases::Store::with_key(
            dir.path().join("cases"),
            &[3; 32],
        ))
    });
    let id = open_applied(
        &tabs,
        cx,
        dir.path(),
        "a.md",
        "Anna Petrova met Northwind Ltd.",
        &[("Anna Petrova", Person), ("Northwind Ltd", Organization)],
    );
    save(&tabs, id, cx);
    tabs.update_in(cx, |tabs, window, cx| tabs.create_case(id, window, cx));
    cx.run_until_parked();
    let member = session_store::identity(&dir.path().join("a.md"));
    let reviewed = |cx: &mut VisualTestContext| {
        cx.update(|_, cx| {
            tabs.read(cx).case_coordinator.cases()[0]
                .reviewed(&member)
                .map(str::to_owned)
        })
    };
    let saved = std::fs::read_to_string(&member).unwrap();
    assert_eq!(reviewed(cx), Some(crate::shelf::fingerprint(&saved)));

    // A new proposal makes it not ready; saving now forgets the fingerprint.
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
    save(&tabs, id, cx);
    assert_eq!(reviewed(cx), None);
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

/// The shelf's own controls (ADR 0084): Copy for AI and a drag carry the
/// editor's bytes for a ready row and nothing for one that is not; Restore
/// Clipboard uses the current case; a click brings the tab forward.
#[gpui::test]
fn the_shelf_copies_drags_restores_and_opens(cx: &mut TestAppContext) {
    let dir = tempfile::tempdir().unwrap();
    let (tabs, cx) = boot(cx, Session::default());
    tabs.update(cx, |tabs, _| {
        tabs.use_case_store(mdoc_store::cases::Store::with_key(
            dir.path().join("cases"),
            &[3; 32],
        ))
    });
    let ready = open_applied(
        &tabs,
        cx,
        dir.path(),
        "Anna Petrova contract.md",
        "Anna Petrova met Northwind Ltd.",
        &[("Anna Petrova", Person), ("Northwind Ltd", Organization)],
    );
    save(&tabs, ready, cx);
    tabs.update_in(cx, |tabs, window, cx| tabs.create_case(ready, window, cx));
    cx.run_until_parked();
    let pending = open_applied(
        &tabs,
        cx,
        dir.path(),
        "memo.md",
        "Boris Ivanov wrote to Anna Petrova.",
        &[("Boris Ivanov", Person)],
    );
    let case = cx.update(|_, cx| tabs.read(cx).case_coordinator.cases()[0].id.clone());
    tabs.update_in(cx, |tabs, window, cx| {
        tabs.join_case(pending, &case, window, cx)
    });
    cx.run_until_parked();
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
    assert_eq!(label, "memo, Anna Petrova contract, 1 left to review");

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

/// Review of #265: a closed member that is not ready opens as a tab and,
/// once loaded, shows its first pending proposal.
#[gpui::test]
fn a_closed_member_that_is_not_ready_opens_at_its_first_proposal(cx: &mut TestAppContext) {
    let dir = tempfile::tempdir().unwrap();
    let (tabs, cx) = boot(cx, Session::default());
    tabs.update(cx, |tabs, _| {
        tabs.use_case_store(mdoc_store::cases::Store::with_key(
            dir.path().join("cases"),
            &[3; 32],
        ))
    });
    let id = open_applied(
        &tabs,
        cx,
        dir.path(),
        "letter.md",
        "Northwind Ltd wrote to PERSON_9.",
        &[("Northwind Ltd", Organization)],
    );
    save(&tabs, id, cx);
    tabs.update_in(cx, |tabs, window, cx| tabs.create_case(id, window, cx));
    cx.run_until_parked();
    tabs.update_in(cx, |tabs, window, cx| tabs.close_tab(id, window, cx));
    cx.run_until_parked();
    let path = session_store::identity(&dir.path().join("letter.md"));
    std::fs::write(&path, "ORG_1 wrote again. Copy to Northwind Ltd.").unwrap();
    let row = cx
        .update(|_, cx| tabs.read(cx).shelf_documents(cx))
        .into_iter()
        .find(|row| matches!(&row.key, ShelfKey::Member { path: p, .. } if *p == path))
        .unwrap();
    assert_eq!(row.readiness, Readiness::NamesNotReplaced(1));
    tabs.update_in(cx, |tabs, window, cx| {
        tabs.open_from_shelf(&row.key, false, window, cx)
    });
    for _ in 0..20 {
        wait(cx, 100);
        cx.run_until_parked();
    }
    assert!(
        cx.update(|_, cx| tabs.read(cx).find_path(&path, None, cx))
            .is_some()
    );
    assert_first_proposal_shown(&tabs, cx);
}

/// Review of #265: every shelf control is reachable by keyboard, and the
/// picker keeps focus inside the shelf.
#[gpui::test]
fn the_shelf_is_reachable_by_keyboard_and_the_picker_keeps_focus(cx: &mut TestAppContext) {
    let dir = tempfile::tempdir().unwrap();
    let (tabs, cx) = boot(cx, Session::default());
    tabs.update(cx, |tabs, _| {
        tabs.use_case_store(mdoc_store::cases::Store::with_key(
            dir.path().join("cases"),
            &[3; 32],
        ))
    });
    // Two sources: a case, and a tab without one through its mapping.
    let cased = open_applied(
        &tabs,
        cx,
        dir.path(),
        "a.md",
        "Anna Petrova met Northwind Ltd.",
        &[("Anna Petrova", Person), ("Northwind Ltd", Organization)],
    );
    save(&tabs, cased, cx);
    tabs.update_in(cx, |tabs, window, cx| tabs.create_case(cased, window, cx));
    cx.run_until_parked();
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

/// Polish (ADR 0084): ⌘1–⌘8 copy that row for AI, ✓ "Copied" shows and
/// the shelf closes after about 0.8 s; a clipboard result shows on the
/// source line for a few seconds; the first drag retires the drag hint for
/// good, recorded in settings.
#[gpui::test]
fn copied_closes_the_shelf_and_hints_and_results_come_and_go(cx: &mut TestAppContext) {
    let dir = tempfile::tempdir().unwrap();
    let (tabs, cx) = boot(cx, Session::default());
    tabs.update(cx, |tabs, _| {
        tabs.use_case_store(mdoc_store::cases::Store::with_key(
            dir.path().join("cases"),
            &[3; 32],
        ))
    });
    let cased = open_applied(
        &tabs,
        cx,
        dir.path(),
        "a.md",
        "Anna Petrova met Northwind Ltd about the lease renewal terms.",
        &[("Anna Petrova", Person), ("Northwind Ltd", Organization)],
    );
    save(&tabs, cased, cx);
    tabs.update_in(cx, |tabs, window, cx| tabs.create_case(cased, window, cx));
    cx.run_until_parked();
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
        line(cx).ends_with(" restored · 0 unrecognised"),
        "{}",
        line(cx)
    );
    cx.executor().advance_clock(crate::shelf_ui::RESULT_FOR);
    cx.run_until_parked();
    assert_eq!(line(cx), source);

    // ⌘9 is no row; the first drag retires the hint for good.
    let mut shelf_cx = VisualTestContext::from_window(shelf.into(), cx);
    copy_row(9, &mut shelf_cx);
    assert_eq!(
        shelf.read_with(&shelf_cx, |view, _| view.copied()).unwrap(),
        None
    );
    shelf
        .update(cx, |view, window, cx| view.drag(0, window, cx))
        .unwrap();
    assert!(
        !shelf
            .read_with(cx, |view, _| view.content().drag_hint)
            .unwrap()
    );
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
