//! The menu bar shelf's list and handoff: what it lists from the
//! window's tabs and the Vault, and that nothing leaves it unless the
//! Vault's outbound check finds it ready.
use super::*;
use crate::shelf::{Readiness, ShelfExports, ShelfKey};
use mdoc_pii::Category::{Organization, Person};
use mdoc_store::document_id::VaultId;

const ORIGINALS: [&str; 5] = [
    "Anna Petrova",
    "Northwind Ltd",
    "Boris Ivanov",
    "Ivan Ivanov",
    "Clara Moss",
];

/// Pending autosaves, and the Project writes they start, land.
fn written(cx: &mut VisualTestContext) {
    wait(cx, 800);
    wait(cx, 800);
}

/// Apply every proposal in the active tab: a moved document's Project names.
fn apply_all(tabs: &Entity<Workspace>, cx: &mut VisualTestContext) {
    let view = active(tabs, cx);
    view.update_in(cx, |view, window, cx| {
        view.apply_all_pii(&PiiApplyAll, window, cx)
    });
    cx.run_until_parked();
}

fn close(tabs: &Entity<Workspace>, id: u64, cx: &mut VisualTestContext) {
    tabs.update_in(cx, |tabs, window, cx| tabs.close_tab(id, window, cx));
    cx.run_until_parked();
}

/// Done when: ready, not-ready and changed-since-review documents, open
/// and stored, are copied and dragged from the shelf; only ready ones carry
/// anything, the Vault's outbound text; no export holds an original.
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

    // Ready and stored: moved in, its Project's names applied, written and
    // closed.
    let closed = open_applied(
        &tabs,
        cx,
        dir.path(),
        "Clara Moss notes.md",
        "Clara Moss paid Anna Petrova.",
        &[("Clara Moss", Person)],
    );
    move_tab(&tabs, cx, closed, Some(&project));
    apply_all(&tabs, cx);
    written(cx);
    let closed_document = cx.update(|_, cx| tabs.read(cx).tab_document(closed, cx).unwrap());
    close(&tabs, closed, cx);

    // Changed since review: written ready and closed, then its stored text
    // changed by another writer.
    let changed = open_applied(
        &tabs,
        cx,
        dir.path(),
        "Ivan Ivanov settlement.md",
        "Ivan Ivanov owes Northwind Ltd.",
        &[("Ivan Ivanov", Person)],
    );
    move_tab(&tabs, cx, changed, Some(&project));
    // Its Project's names applied, so it is written ready: left proposed,
    // it was never reviewed, which the shelf once read as changed (#1209).
    apply_all(&tabs, cx);
    written(cx);
    let changed_document = cx.update(|_, cx| tabs.read(cx).tab_document(changed, cx).unwrap());
    close(&tabs, changed, cx);
    let vault = vault(&tabs, cx);
    vault
        .update_document(changed_document.as_str(), |document| {
            document
                .markdown
                .push_str("\n\nA paragraph added elsewhere.")
        })
        .unwrap();

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
    written(cx);

    let rows = cx.update(|_, cx| tabs.read(cx).shelf_documents(cx));
    let state = |key: &ShelfKey| {
        rows.iter()
            .find(|row| &row.key == key)
            .unwrap_or_else(|| panic!("{key:?} is listed"))
            .readiness
    };
    let stored = |document: &VaultId| ShelfKey::Document(document.clone());
    assert_eq!(rows.len(), 4);
    // Open tabs first, then documents no tab shows.
    assert!(rows[..2].iter().all(|r| matches!(r.key, ShelfKey::Tab(_))));
    assert!(
        rows[2..]
            .iter()
            .all(|r| matches!(r.key, ShelfKey::Document(_)))
    );
    assert_eq!(state(&ShelfKey::Tab(ready)), Readiness::Ready);
    assert_eq!(state(&ShelfKey::Tab(pending)), Readiness::LeftToReview(1));
    assert_eq!(state(&stored(&closed_document)), Readiness::Ready);
    assert_eq!(
        state(&stored(&changed_document)),
        Readiness::ChangedSinceReview
    );
    assert!(
        rows.iter()
            .all(|row| row.project.as_ref().unwrap().id == project)
    );

    // Copy for AI is the editor's bytes for an open tab, as the Vault's
    // outbound check gives them, and the check's text for a stored one.
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
    let row = |key: &ShelfKey| rows.iter().find(|row| &row.key == key).unwrap();
    assert_eq!(
        row(&ShelfKey::Tab(ready)).text_for_ai(),
        Some(editor_copy.as_str())
    );
    let outbound = vault.text_for_ai(closed_document.as_str()).unwrap();
    assert_eq!(
        row(&stored(&closed_document)).text_for_ai(),
        Some(outbound.as_str())
    );

    // Copy and drag every row; only ready ones carry anything.
    let exports = ShelfExports::create_in(dir.path()).unwrap();
    let mut handed = Vec::new();
    for row in &rows {
        let copied = row.text_for_ai().map(str::to_owned);
        // Named as a drag names it, as it leaves.
        let mut row = row.clone();
        let name = cx.update(|_, cx| tabs.read(cx).shelf_share_name(&row.key, cx));
        row.set_share_name(name);
        let row = &row;
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
    // Exports leave under the Vault's neutral names, nothing of a title
    // (#1208).
    let ready_document = cx.update(|_, cx| tabs.read(cx).tab_document(ready, cx).unwrap());
    for document in [&ready_document, &closed_document] {
        let name = format!("{}.md", vault.share_name(document.as_str()).unwrap());
        assert!(handed.contains(&name), "{name} in {handed:?}");
    }
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

/// Done when: a name another document adds to the Project takes a row off
/// ready without touching its file, and a shelf opened before hands out
/// nothing for it.
/// Done when (#1292): a Ready document moved out of a deleted Project
/// leaves the shelf's Ready rows and reads "Names not looked for yet",
/// as Android's document does: the move cleared its review.
#[gpui::test]
fn a_document_moved_out_of_a_deleted_project_owes_a_scan(cx: &mut TestAppContext) {
    let dir = tempfile::tempdir().unwrap();
    let (tabs, cx) = boot(cx, Session::default());
    let letter = open_applied(
        &tabs,
        cx,
        dir.path(),
        "letter.md",
        "Anna Petrova wrote about the lease.",
        &[("Anna Petrova", Person)],
    );
    let project = new_project(&tabs, cx, letter);
    written(cx);
    let document = cx.update(|_, cx| tabs.read(cx).tab_document(letter, cx).unwrap());
    close(&tabs, letter, cx);
    let key = ShelfKey::Document(document.clone());
    let listed = |cx: &mut VisualTestContext| {
        cx.update(|_, cx| {
            tabs.read(cx)
                .shelf_documents(cx)
                .into_iter()
                .find(|row| row.key == key)
                .unwrap()
        })
    };
    assert_eq!(listed(cx).readiness, Readiness::Ready);
    vault(&tabs, cx)
        .delete_project(&project, mdoc_vault::ProjectDocuments::MoveToUnfiled)
        .unwrap();
    let moved = listed(cx);
    assert_eq!(moved.readiness, Readiness::NotReviewed);
    assert_eq!(moved.state(), "Names not looked for yet");
    assert_eq!(moved.step(), Some(crate::refusal::Step::FindNames));
    assert_eq!(moved.text_for_ai(), None);
}

#[gpui::test]
fn a_name_another_document_adds_takes_a_row_off_ready(cx: &mut TestAppContext) {
    let dir = tempfile::tempdir().unwrap();
    let (tabs, cx) = boot(cx, Session::default());
    let letter = open_applied(
        &tabs,
        cx,
        dir.path(),
        "letter.md",
        "Anna Petrova wrote to Boris Ivanov about the lease.",
        &[("Anna Petrova", Person)],
    );
    let project = new_project(&tabs, cx, letter);
    written(cx);
    let document = cx.update(|_, cx| tabs.read(cx).tab_document(letter, cx).unwrap());
    close(&tabs, letter, cx);
    let vault = vault(&tabs, cx);
    let file = vault
        .root()
        .join("docs")
        .join(document.as_str())
        .join("doc.enc");
    let before = std::fs::read(&file).unwrap();
    let key = ShelfKey::Document(document.clone());
    let listed = |cx: &mut VisualTestContext| {
        cx.update(|_, cx| {
            tabs.read(cx)
                .shelf_documents(cx)
                .into_iter()
                .find(|row| row.key == key)
                .unwrap()
        })
    };
    assert_eq!(listed(cx).readiness, Readiness::Ready);
    let shelf = open_shelf(cx);
    let index = shelf
        .read_with(cx, |view, cx| {
            let documents = &view.content(cx).documents;
            documents.iter().position(|d| d.key == key).unwrap()
        })
        .unwrap();

    // Another document names Boris Ivanov and joins the Project.
    let memo = open_applied(
        &tabs,
        cx,
        dir.path(),
        "memo.md",
        "Boris Ivanov signed.",
        &[("Boris Ivanov", Person)],
    );
    move_tab(&tabs, cx, memo, Some(&project));
    written(cx);
    let names = vault.project(&project).unwrap().mapping.exported();
    assert!(
        names
            .iter()
            .any(|n| n.originals.iter().any(|o| &**o == "Boris Ivanov"))
    );

    let row = listed(cx);
    assert_eq!(row.readiness, Readiness::NamesNotReplaced(1));
    assert_eq!(row.text_for_ai(), None);
    assert!(row.preview.is_none());
    assert_eq!(
        vault.readiness(document.as_str()).unwrap(),
        mdoc_vault::Readiness::NamesNotReplaced(1)
    );
    assert_eq!(
        std::fs::read(&file).unwrap(),
        before,
        "its file is untouched"
    );

    // The shelf listed it ready before: Copy for AI and a drag check again
    // and hand out nothing.
    cx.write_to_clipboard(gpui::ClipboardItem::new_string("before".into()));
    shelf
        .update(cx, |view, window, cx| {
            view.copy(index, window, cx);
            view.drag(index, window, cx);
            assert!(view.rows().read(cx).dragged.is_none());
            assert_eq!(view.copied(cx), None);
            assert_eq!(
                view.content(cx).documents[index].readiness,
                Readiness::NamesNotReplaced(1)
            );
        })
        .unwrap();
    assert_eq!(cx.read_from_clipboard().unwrap().text().unwrap(), "before");
}

/// Review of #1145 (P1): the editor's Copy for AI is checked as the shelf
/// is, against the names its Project holds now. A name another document
/// adds takes it off ready: the Share tab, the menu and the shortcut copy
/// nothing and say why; Copy Markdown is still the deliberate raw copy.
#[gpui::test]
fn copy_for_ai_in_the_editor_is_checked_against_the_projects_names(cx: &mut TestAppContext) {
    let dir = tempfile::tempdir().unwrap();
    let (tabs, cx) = boot(cx, Session::default());
    let text = "Anna Petrova wrote to Boris Ivanov about the lease.";
    let letter = open_applied(
        &tabs,
        cx,
        dir.path(),
        "letter.md",
        text,
        &[("Anna Petrova", Person)],
    );
    let project = new_project(&tabs, cx, letter);
    written(cx);
    let view = active(&tabs, cx);
    let copy = |cx: &mut VisualTestContext| {
        view.update_in(cx, |view, window, cx| {
            view.error = None;
            view.copy_for_ai(&crate::pii::ui::CopyForAi, window, cx)
        });
        cx.run_until_parked();
    };
    // Ready: the Vault's outbound text, the shelf's bytes.
    copy(cx);
    let copied = cx.update(|_, cx| cx.read_from_clipboard().unwrap().text().unwrap());
    assert!(copied.ends_with("PERSON_1 wrote to Boris Ivanov about the lease."));
    assert!(!copied.contains("Anna"));

    // Another document names Boris Ivanov and joins the Project.
    let memo = open_applied(
        &tabs,
        cx,
        dir.path(),
        "memo.md",
        "Boris Ivanov signed.",
        &[("Boris Ivanov", Person)],
    );
    move_tab(&tabs, cx, memo, Some(&project));
    written(cx);
    tabs.update_in(cx, |tabs, window, cx| tabs.activate(letter, window, cx));
    cx.run_until_parked();
    cx.write_to_clipboard(gpui::ClipboardItem::new_string("before".into()));
    copy(cx);
    assert_eq!(cx.read_from_clipboard().unwrap().text().unwrap(), "before");
    view.read_with(cx, |view, _| {
        let why = view.copy_refusal.as_ref().expect("a refusal");
        // Its review proposes the Project's new name, as the shelf's row
        // says, and the notice offers to replace it.
        assert_eq!(why.reason, "1 proposal is left to review.");
        assert_eq!(why.step, Some(crate::refusal::Step::ReplaceAndCopy(1)));
    });
    // The shortcut and the menu dispatch the same action.
    cx.update(|window, cx| window.focus(&view.read(cx).editor.read(cx).focus_handle(cx), cx));
    cx.dispatch_action(crate::pii::ui::CopyForAi);
    cx.run_until_parked();
    assert_eq!(cx.read_from_clipboard().unwrap().text().unwrap(), "before");
    // Copy Markdown copies the text as it is, on purpose.
    view.update_in(cx, |view, window, cx| {
        view.copy_markdown(&crate::document_view::CopyMarkdown, window, cx)
    });
    let raw = cx.update(|_, cx| cx.read_from_clipboard().unwrap().text().unwrap());
    assert_eq!(raw, "PERSON_1 wrote to Boris Ivanov about the lease.");
}

/// Review of #1150: a Copy for AI waiting for its write never replaces a
/// copy made since.
#[gpui::test]
fn a_waiting_copy_for_ai_never_replaces_a_newer_copy(cx: &mut TestAppContext) {
    let dir = tempfile::tempdir().unwrap();
    let (tabs, cx) = boot(cx, Session::default());
    open_applied(
        &tabs,
        cx,
        dir.path(),
        "letter.md",
        "Anna Petrova wrote.",
        &[("Anna Petrova", Person)],
    );
    let view = active(&tabs, cx);
    // Unsaved: the copy waits for its write.
    view.update_in(cx, |view, window, cx| {
        view.copy_for_ai(&crate::pii::ui::CopyForAi, window, cx)
    });
    let waiting = cx.update(|_, cx| tabs.read(cx).copy_for_ai_waiting.is_some());
    assert!(waiting, "the copy waits for its write");
    cx.write_to_clipboard(gpui::ClipboardItem::new_string("newer".into()));
    cx.run_until_parked();
    assert_eq!(cx.read_from_clipboard().unwrap().text().unwrap(), "newer");
    // Asked again with nothing newer, it copies.
    view.update_in(cx, |view, window, cx| {
        view.copy_for_ai(&crate::pii::ui::CopyForAi, window, cx)
    });
    cx.run_until_parked();
    let copied = cx.update(|_, cx| cx.read_from_clipboard().unwrap().text().unwrap());
    assert!(copied.ends_with("PERSON_1 wrote."), "{copied}");
}

/// A stored document that is not ready opens in a tab at its first
/// proposal: the name its Project added since.
#[gpui::test]
fn a_stored_document_that_is_not_ready_opens_at_its_first_proposal(cx: &mut TestAppContext) {
    let dir = tempfile::tempdir().unwrap();
    let (tabs, cx) = boot(cx, Session::default());
    let letter = open_applied(
        &tabs,
        cx,
        dir.path(),
        "letter.md",
        "Northwind Ltd wrote. Copy to Anna Petrova.",
        &[("Northwind Ltd", Organization)],
    );
    let project = new_project(&tabs, cx, letter);
    written(cx);
    let document = cx.update(|_, cx| tabs.read(cx).tab_document(letter, cx).unwrap());
    close(&tabs, letter, cx);
    let memo = open_applied(
        &tabs,
        cx,
        dir.path(),
        "memo.md",
        "Anna Petrova signed.",
        &[("Anna Petrova", Person)],
    );
    move_tab(&tabs, cx, memo, Some(&project));
    written(cx);
    let key = ShelfKey::Document(document.clone());
    let row = cx
        .update(|_, cx| tabs.read(cx).shelf_documents(cx))
        .into_iter()
        .find(|row| row.key == key)
        .unwrap();
    assert_eq!(row.readiness, Readiness::NamesNotReplaced(1));
    tabs.update_in(cx, |tabs, window, cx| {
        tabs.open_from_shelf(&row.key, false, None, window, cx)
    });
    for _ in 0..20 {
        wait(cx, 100);
    }
    let shown = cx.update(|_, cx| tabs.read(cx).tab_document(tabs.read(cx).tabs.active(), cx));
    assert_eq!(shown, Some(document));
    assert_first_proposal_shown(&tabs, cx);
}

/// Review of #1127: a name applied in an open member reaches the Vault
/// only once the Project is written; until then a stored member of that
/// Project is not checked against the old names, and does not leave.
#[gpui::test]
fn a_stored_row_waits_for_its_projects_unwritten_names(cx: &mut TestAppContext) {
    let dir = tempfile::tempdir().unwrap();
    let (tabs, cx) = boot(cx, Session::default());
    let letter = open_applied(
        &tabs,
        cx,
        dir.path(),
        "letter.md",
        "Anna Petrova wrote to Boris Ivanov about the lease.",
        &[("Anna Petrova", Person)],
    );
    let project = new_project(&tabs, cx, letter);
    written(cx);
    let document = cx.update(|_, cx| tabs.read(cx).tab_document(letter, cx).unwrap());
    close(&tabs, letter, cx);
    let memo = open_applied(
        &tabs,
        cx,
        dir.path(),
        "memo.md",
        "Boris Ivanov signed for Anna Petrova.",
        &[],
    );
    move_tab(&tabs, cx, memo, Some(&project));
    apply_all(&tabs, cx);
    written(cx);
    let key = ShelfKey::Document(document);
    let state = |cx: &mut VisualTestContext| {
        cx.update(|_, cx| {
            tabs.read(cx)
                .shelf_documents(cx)
                .into_iter()
                .find(|row| row.key == key)
                .unwrap()
                .readiness
        })
    };
    assert_eq!(state(cx), Readiness::Ready);

    // The memo names Boris Ivanov: its mapping change waits to be folded
    // into the Project, then the Project's write.
    let view = active(&tabs, cx);
    view.update_in(cx, |view, window, cx| {
        let text = view.editor.read(cx).text().to_owned();
        let start = text.find("Boris Ivanov").unwrap();
        view.pii
            .review_mut()
            .add_manual(&text, start..start + 12, Person)
            .unwrap();
        view.apply_all_pii(&PiiApplyAll, window, cx);
    });
    cx.run_until_parked();
    assert_eq!(state(cx), Readiness::Checking);
    written(cx);
    assert_eq!(state(cx), Readiness::NamesNotReplaced(1));
}

/// Review of #1127: a save made while an edit's names were still being
/// found recorded the review as not ready; once they are found it is
/// written again, so the row does not stay on "Checking names".
#[gpui::test]
fn a_save_during_a_scan_is_written_again_once_the_scan_ends(cx: &mut TestAppContext) {
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
    written(cx);
    assert_eq!(row_of(&tabs, id, cx).readiness, Readiness::Ready);
    let view = active(&tabs, cx);
    view.update(cx, |view, cx| {
        view.editor.update(cx, |editor, cx| {
            let end = editor.text().len();
            editor.replace_range(end..end, " Later.", cx);
        });
    });
    // ⌘S at once: written while the edit's scan still runs.
    view.update(cx, |view, cx| {
        assert!(view.pii.review().awaiting_discovery());
        view.save_now(cx);
    });
    cx.run_until_parked();
    written(cx);
    // The edited paragraph's scan follows the typing pause.
    wait(cx, 1600);
    written(cx);
    assert_eq!(row_of(&tabs, id, cx).readiness, Readiness::Ready);
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
    written(cx);
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

/// The shelf's own controls: Copy for AI and a drag carry the
/// editor's bytes for a ready row and nothing for one that is not; Restore
/// Clipboard uses the current Project; a click brings the document forward.
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
    written(cx);
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
            .read_with(cx, |view, cx| {
                view.content(cx)
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
        .read_with(cx, |view, cx| {
            crate::shelf_ui::row_label(&view.content(cx).documents[pending_row])
        })
        .unwrap();
    // The Project is named after its first document: masked too.
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
            assert!(view.rows().read(cx).dragged.is_none());
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
        .read_with(cx, |view, cx| view.rows().read(cx).dragged.clone().unwrap())
        .unwrap();
    assert_eq!(dragged, editor_copy);
    assert_eq!(std::fs::read_to_string(&file).unwrap(), editor_copy);
    // Its file leaves under the Vault's neutral name, nothing of its title
    // (#1208).
    let document = cx.update(|_, cx| tabs.read(cx).tab_document(ready, cx).unwrap());
    let share_name = vault(&tabs, cx).share_name(document.as_str()).unwrap();
    assert!(share_name.starts_with("Document "), "{share_name}");
    let name = file.file_name().unwrap().to_string_lossy().to_lowercase();
    for word in ["anna", "petrova", "contract"] {
        assert!(!name.contains(word), "{word} of the title left in {name}");
    }
    assert_eq!(
        file.file_name().unwrap().to_string_lossy(),
        format!("{share_name}.md")
    );

    // Restore Clipboard with the Project of the document used last.
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
    // Two sources: a Project, and a document without one through its mapping.
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
    written(cx);
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
            .read_with(&shelf_cx, |view, cx| view.content(cx).source.is_some())
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
    written(cx);
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
        .read_with(cx, |view, cx| {
            let content = view.content(cx);
            assert!(content.documents.iter().all(|d| d.preview.is_some()));
            (
                content.documents[1].text_for_ai().unwrap().to_string(),
                content.drag_hint,
            )
        })
        .unwrap();
    assert!(hint, "the hint shows before the first drag");
    // A document without a Project is named without `.md` (GUI run of #280).
    let sources = shelf
        .read_with(cx, |view, cx| view.content(cx).sources.clone())
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
        shelf
            .read_with(&shelf_cx, |view, cx| view.copied(cx))
            .unwrap(),
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
        shelf
            .read_with(&shelf_cx, |view, cx| view.copied(cx))
            .unwrap(),
        None
    );
    let hint = |cx: &mut VisualTestContext| {
        shelf
            .read_with(cx, |view, cx| view.content(cx).drag_hint)
            .unwrap()
    };
    shelf
        .update(cx, |view, window, cx| {
            view.drag(0, window, cx);
            let card = view.rows().read(cx).dragged_card.clone().unwrap();
            assert_eq!(Some(card), view.content(cx).documents[0].drag_card());
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
            view.dropped_on_shelf(cx);
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
            .read_with(cx, |view, cx| view.content(cx).drag_hint)
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
    written(cx);
    let shelf = open_shelf(cx);
    let estimate = shelf
        .update(cx, |view, _, cx| {
            let estimate = view.content(cx).height();
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
