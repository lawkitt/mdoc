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
