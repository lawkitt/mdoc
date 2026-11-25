//! A refused Copy for AI or shelf row names its reason and carries the one
//! step that resolves it (#1209, Vault & Projects item 36): every refusal
//! state on desktop, and one click from each ending in copied text or the
//! open review. What may leave is still the readiness check's
//! (`workspace::tests::exits`).
use super::*;
use crate::{
    document_view::Inspector,
    pii::ui::CopyForAi,
    refusal::{Refusal, Step},
    shelf::{Readiness, ShelfKey},
};
use mdoc_pii::{Category::Person, restore::ExportedIdentity};

const TEXT: &str = "Anna Petrova wrote to Boris Ivanov about the lease.";

/// Pending autosaves, and the Project writes they start, land.
fn written(cx: &mut VisualTestContext) {
    wait(cx, 800);
    wait(cx, 800);
}

fn clipboard(cx: &mut VisualTestContext) -> String {
    cx.update(|_, cx| {
        (cx.read_from_clipboard())
            .and_then(|item| item.text())
            .unwrap_or_default()
    })
}

fn before(cx: &mut VisualTestContext) {
    cx.write_to_clipboard(gpui::ClipboardItem::new_string("before".into()));
}

/// Open `text` as `name` with `names` proposed and not applied.
fn open_proposed(
    tabs: &Entity<Workspace>,
    cx: &mut VisualTestContext,
    dir: &std::path::Path,
    name: &str,
    text: &str,
    names: &[&str],
) -> Entity<DocumentView> {
    let path = dir.join(name);
    std::fs::write(&path, text).unwrap();
    tabs.update_in(cx, |tabs, window, cx| tabs.open_path(path, window, cx));
    cx.run_until_parked();
    let view = active(tabs, cx);
    view.update_in(cx, |view, _, cx| {
        view.pii.set_reviewing(true);
        for original in names {
            let start = text.find(original).unwrap();
            (view.pii.review_mut())
                .add_manual(text, start..start + original.len(), Person)
                .unwrap();
        }
        view.review_changed(cx);
    });
    cx.run_until_parked();
    view
}

/// The Share tab showing in `view`'s Inspector.
fn show_share(view: &Entity<DocumentView>, cx: &mut VisualTestContext) {
    view.update_in(cx, |view, window, cx| {
        if !view.replacements_open() {
            view.toggle_replacements_panel(window, cx);
        }
        view.inspector = Inspector::Share;
        cx.notify();
    });
    draw(cx);
}

/// Copy for AI as the menu and the shortcut run it: what it refused.
fn copy_for_ai(view: &Entity<DocumentView>, cx: &mut VisualTestContext) -> Option<Refusal> {
    view.update_in(cx, |view, window, cx| {
        view.copy_refusal = None;
        view.copy_for_ai(&CopyForAi, window, cx)
    });
    written(cx);
    view.read_with(cx, |view, _| view.copy_refusal.clone())
}

/// The review is open on Review.
fn reviewing(view: &Entity<DocumentView>, cx: &mut VisualTestContext) -> bool {
    view.read_with(cx, |view, _| {
        view.replacements_open() && view.inspector == Inspector::Review
    })
}

fn person(alias: &str, original: &str) -> ExportedIdentity {
    ExportedIdentity {
        alias: alias.into(),
        previous: vec![],
        category: Person,
        custom: false,
        originals: vec![original.into()],
    }
}

/// Done when: names still proposed show "Replace 2 and copy" on the Share
/// tab before the click, and that click replaces them and copies the
/// checked text; Copy for AI from the menu refuses with the same step in
/// its notice, and the notice's click copies.
#[gpui::test]
fn pending_names_offer_replace_and_copy(cx: &mut TestAppContext) {
    let dir = tempfile::tempdir().unwrap();
    let (tabs, cx) = boot(cx, Session::default());
    cx.simulate_resize(size(px(1400.), px(850.)));
    let view = open_proposed(
        &tabs,
        cx,
        dir.path(),
        "lease.md",
        TEXT,
        &["Anna Petrova", "Boris Ivanov"],
    );
    written(cx);
    assert_eq!(
        view.read_with(cx, |view, cx| view.copy_step(cx)),
        Some(Step::ReplaceAndCopy(2))
    );

    // From the menu: refused, with the step in the notice.
    before(cx);
    let refused = copy_for_ai(&view, cx).expect("refused");
    assert_eq!(refused.reason, "2 proposals are left to review.");
    assert_eq!(refused.step, Some(Step::ReplaceAndCopy(2)));
    assert_eq!(clipboard(cx), "before");
    draw(cx);
    assert!(cx.debug_bounds("copy-refusal-notice").is_some());
    // Another name marked since: the shown step replaces nothing and shows
    // the count now.
    view.update_in(cx, |view, _, cx| {
        let start = TEXT.find("lease").unwrap();
        (view.pii.review_mut())
            .add_manual(TEXT, start..start + "lease".len(), Person)
            .unwrap();
        view.review_changed(cx);
    });
    click_toolbar(cx, "copy-refusal-step");
    view.read_with(cx, |view, cx| {
        assert_eq!(view.editor.read(cx).text(), TEXT);
        let refusal = view.copy_refusal.clone().expect("refreshed");
        assert_eq!(refusal.step, Some(Step::ReplaceAndCopy(3)));
    });
    click_toolbar(cx, "copy-refusal-step");
    written(cx);
    let copied = clipboard(cx);
    assert!(
        copied.ends_with("PERSON_1 wrote to PERSON_2 about the PERSON_3."),
        "{copied}"
    );
    let document = cx.update(|_, cx| tabs.read(cx).tab_document(tabs.read(cx).tabs.active(), cx));
    let document = document.unwrap();
    assert_eq!(
        vault(&tabs, cx).text_for_ai(document.as_str()).unwrap(),
        copied
    );
    view.read_with(cx, |view, _| assert!(view.copy_refusal.is_none()));

    // On the Share tab the button says it before the click.
    let other = open_proposed(
        &tabs,
        cx,
        dir.path(),
        "memo.md",
        "Clara Moss signed.",
        &["Clara Moss"],
    );
    written(cx);
    show_share(&other, cx);
    before(cx);
    assert_eq!(
        other.read_with(cx, |view, cx| view.copy_step(cx).map(Step::label)),
        Some("Replace 1 and copy".into())
    );
    click_toolbar(cx, "share-copy-for-ai");
    written(cx);
    let copied = clipboard(cx);
    assert!(copied.ends_with("PERSON_1 signed."), "{copied}");
}

/// Done when: a name the tab doesn't know but its Project does (the Vault's
/// check alone sees it) refuses with "Review", and the click opens the
/// review.
#[gpui::test]
fn a_known_name_written_out_offers_review(cx: &mut TestAppContext) {
    let dir = tempfile::tempdir().unwrap();
    let (tabs, cx) = boot(cx, Session::default());
    cx.simulate_resize(size(px(1400.), px(850.)));
    let letter = open_applied(
        &tabs,
        cx,
        dir.path(),
        "letter.md",
        TEXT,
        &[("Anna Petrova", Person)],
    );
    let project = new_project(&tabs, cx, letter);
    written(cx);
    (vault(&tabs, cx).update_project(&project, |project| {
        project.mapping.merge(&[person("PERSON_2", "Boris Ivanov")])
    }))
    .unwrap();
    let view = active(&tabs, cx);
    // The tab can't know it before the click: its button copies.
    assert_eq!(view.read_with(cx, |view, cx| view.copy_step(cx)), None);
    before(cx);
    let refused = copy_for_ai(&view, cx).expect("refused");
    assert!(
        refused
            .reason
            .starts_with("1 known name is still written out"),
        "{refused:?}"
    );
    assert_eq!(refused.step, Some(Step::Review));
    assert_eq!(clipboard(cx), "before");
    click_toolbar(cx, "copy-refusal-step");
    assert!(reviewing(&view, cx));
    view.read_with(cx, |view, _| assert!(view.copy_refusal.is_none()));
}

/// Done when: a name the review kept as written (a Keep of a name the
/// Project knows) is refused with "Review" before and after the click, never
/// a dead end; the click opens the review. The rule that a Keep refuses is
/// unchanged.
#[gpui::test]
fn a_kept_name_offers_review(cx: &mut TestAppContext) {
    let dir = tempfile::tempdir().unwrap();
    let (tabs, cx) = boot(cx, Session::default());
    cx.simulate_resize(size(px(1400.), px(850.)));
    let text = "Павлова Марина Сергеевна работала аналитиком. Марина обратилась в суд.";
    let path = dir.path().join("claim.md");
    std::fs::write(&path, text).unwrap();
    tabs.update_in(cx, |tabs, window, cx| tabs.open_path(path, window, cx));
    cx.run_until_parked();
    let view = active(&tabs, cx);
    view.update_in(cx, |view, window, cx| {
        let name = "Павлова Марина Сергеевна";
        view.pii.set_reviewing(true);
        (view.pii.review_mut())
            .ingest(
                text,
                vec![mdoc_pii::Detection {
                    range: 0..name.len(),
                    category: Person,
                    score: 1.,
                    recognizer: mdoc_pii::Recognizer::Model,
                }],
            )
            .unwrap();
        // Propagation proposes the lone first name; keep it as written, as
        // the popup's Keep does.
        let lone = (view.pii.review().variants().iter())
            .find(|variant| variant.original.as_ref() == "Марина")
            .expect("propagated first name");
        let mentions: Vec<_> = (lone.mentions.iter())
            .map(|range| (lone.id, range.clone()))
            .collect();
        view.pii.review_mut().keep_mentions(mentions);
        view.review_changed(cx);
        view.apply_all_pii(&PiiApplyAll, window, cx);
    });
    written(cx);
    view.read_with(cx, |view, cx| {
        assert_eq!(view.names_not_replaced(cx), 1);
        assert_eq!(view.copy_step(cx), Some(Step::Review));
    });
    before(cx);
    let refused = copy_for_ai(&view, cx).expect("refused");
    assert!(
        refused
            .reason
            .starts_with("1 known name is still written out"),
        "{refused:?}"
    );
    assert_eq!(refused.step, Some(Step::Review));
    assert_eq!(clipboard(cx), "before");
    // Both clicks open the kept mention itself, where Replace and Not a
    // name are.
    let kept = |cx: &mut VisualTestContext| {
        view.read_with(cx, |view, cx| {
            let text = view.editor.read(cx).text();
            let start = text.find("Марина").expect("kept as written");
            (view.shown_kept_mention(), start..start + "Марина".len())
        })
    };
    click_toolbar(cx, "copy-refusal-step");
    assert!(reviewing(&view, cx));
    let (shown, expected) = kept(cx);
    assert_eq!(shown, Some(expected));
    view.update_in(cx, |view, window, cx| {
        view.close_pii_popup(&crate::pii::ui::PiiClosePopup, window, cx)
    });
    show_share(&view, cx);
    click_toolbar(cx, "share-copy-for-ai");
    assert!(reviewing(&view, cx));
    let (shown, expected) = kept(cx);
    assert_eq!(shown, Some(expected));
    assert_eq!(clipboard(cx), "before");
}

/// Done when: a document changed since its review elsewhere refuses with
/// "Review"; a document excluded from AI says so and offers no step, as
/// desktop has no control for it.
#[gpui::test]
fn changed_since_review_offers_review_and_excluded_offers_nothing(cx: &mut TestAppContext) {
    let dir = tempfile::tempdir().unwrap();
    let (tabs, cx) = boot(cx, Session::default());
    cx.simulate_resize(size(px(1400.), px(850.)));
    let letter = open_applied(
        &tabs,
        cx,
        dir.path(),
        "letter.md",
        TEXT,
        &[("Anna Petrova", Person)],
    );
    new_project(&tabs, cx, letter);
    written(cx);
    let document = cx.update(|_, cx| tabs.read(cx).tab_document(letter, cx).unwrap());
    let vault = vault(&tabs, cx);
    (vault.update_document(document.as_str(), |d| {
        d.markdown.push_str(" Clara Moss signed.")
    }))
    .unwrap();
    let view = active(&tabs, cx);
    let refused = copy_for_ai(&view, cx).expect("refused");
    assert_eq!(
        refused,
        Refusal {
            reason: "The text changed since its review.".into(),
            step: Some(Step::Review),
        }
    );
    click_toolbar(cx, "copy-refusal-step");
    assert!(reviewing(&view, cx));

    (vault.update_document(document.as_str(), |d| d.exclude_from_ai = true)).unwrap();
    let refused = copy_for_ai(&view, cx).expect("refused");
    assert_eq!(refused, Refusal::excluded());
    // With a name proposed, the Share tab offers to replace it; kept from
    // AI, the click refuses and leaves the text as it was.
    view.update_in(cx, |view, _, cx| {
        let text = view.editor.read(cx).text().to_owned();
        let start = text.find("Boris Ivanov").unwrap();
        (view.pii.review_mut())
            .add_manual(&text, start..start + "Boris Ivanov".len(), Person)
            .unwrap();
        view.review_changed(cx);
    });
    let text = view.read_with(cx, |view, cx| view.editor.read(cx).text().to_owned());
    show_share(&view, cx);
    click_toolbar(cx, "share-copy-for-ai");
    written(cx);
    view.read_with(cx, |view, cx| {
        assert_eq!(view.editor.read(cx).text(), text);
        assert_eq!(view.copy_refusal, Some(Refusal::excluded()));
    });
    draw(cx);
    assert!(cx.debug_bounds("copy-refusal-notice").is_some());
    assert!(cx.debug_bounds("copy-refusal-step").is_none());
}

/// Done when: a document never scanned shows "Find names" on the Share tab
/// and in the notice, and the click opens the review on its scan.
#[gpui::test]
fn a_document_never_scanned_offers_find_names(cx: &mut TestAppContext) {
    let dir = tempfile::tempdir().unwrap();
    let (tabs, cx) = boot(cx, Session::default());
    cx.simulate_resize(size(px(1400.), px(850.)));
    let path = dir.path().join("lease.md");
    std::fs::write(&path, TEXT).unwrap();
    tabs.update_in(cx, |tabs, window, cx| tabs.open_path(path, window, cx));
    cx.run_until_parked();
    let view = active(&tabs, cx);
    assert_eq!(
        view.read_with(cx, |view, cx| view.copy_step(cx)),
        Some(Step::FindNames)
    );
    let refused = copy_for_ai(&view, cx).expect("refused");
    assert_eq!(refused.reason, "Its names haven't been found yet.");
    assert_eq!(refused.step, Some(Step::FindNames));
    show_share(&view, cx);
    click_toolbar(cx, "share-copy-for-ai");
    assert!(reviewing(&view, cx));
    assert!(
        view.read_with(cx, |view, _| view.scan_asked()),
        "a scan starts"
    );
}

/// Done when: shelf rows that are not Ready carry their step: "Replace 1
/// and copy" on an open tab ends in copied text, "Review" on a stored
/// document opens its review, "Find names" on one never scanned opens it
/// on its scan; "not reviewed" reads "Not reviewed"; and Copy for AI (a
/// click or ⌘C) on such a row says why and focuses its step.
#[gpui::test]
fn shelf_rows_that_are_not_ready_carry_their_step(cx: &mut TestAppContext) {
    let dir = tempfile::tempdir().unwrap();
    let (tabs, cx) = boot(cx, Session::default());
    cx.simulate_resize(size(px(1400.), px(850.)));
    let letter = open_applied(
        &tabs,
        cx,
        dir.path(),
        "letter.md",
        TEXT,
        &[("Anna Petrova", Person)],
    );
    let project = new_project(&tabs, cx, letter);
    written(cx);
    let letter_document = cx.update(|_, cx| tabs.read(cx).tab_document(letter, cx).unwrap());
    tabs.update_in(cx, |tabs, window, cx| tabs.close_tab(letter, window, cx));
    cx.run_until_parked();
    let vault = vault(&tabs, cx);
    // Stored and never saved Ready: never scanned, and scanned.
    let new = |title: &str| mdoc_vault::NewDocument {
        project: Some(project.clone()),
        title: title.into(),
        source_kind: mdoc_vault::SourceKind::Text,
        origin: mdoc_vault::DocumentOrigin::Ordinary,
        markdown: "PERSON_1 asked for a reply.".into(),
    };
    let unscanned = vault.create_document(new("Unscanned")).unwrap().id;
    let scanned = vault.create_document(new("Scanned")).unwrap().id;
    (vault.update_document(&scanned, |d| d.scanned = true)).unwrap();
    // An open tab moved in, the Project's name proposed; its own name
    // joins the Project, so the stored letter holds it as written.
    let memo = open_applied(
        &tabs,
        cx,
        dir.path(),
        "memo.md",
        "Boris Ivanov signed for Anna Petrova.",
        &[("Boris Ivanov", Person)],
    );
    move_tab(&tabs, cx, memo, Some(&project));
    written(cx);
    let note = memo;

    let shelf = open_shelf(cx);
    let rows = shelf
        .read_with(cx, |view, cx| view.content(cx).documents)
        .unwrap();
    let at = |key: ShelfKey| rows.iter().position(|row| row.key == key).unwrap();
    let id = |id: &str| ShelfKey::Document(mdoc_store::document_id::VaultId::parse(id).unwrap());
    let letter_row = at(ShelfKey::Document(letter_document.clone()));
    let unscanned_row = at(id(&unscanned));
    let scanned_row = at(id(&scanned));
    let note_row = at(ShelfKey::Tab(note));
    assert_eq!(rows[letter_row].readiness, Readiness::NamesNotReplaced(1));
    assert_eq!(rows[letter_row].step(), Some(Step::Review));
    assert_eq!(rows[unscanned_row].readiness.label(), "Not reviewed");
    assert_eq!(rows[unscanned_row].step(), Some(Step::FindNames));
    assert_eq!(rows[scanned_row].readiness.label(), "Not reviewed");
    assert_eq!(rows[scanned_row].step(), Some(Step::Review));
    let left = rows[note_row].readiness;
    assert!(matches!(left, Readiness::LeftToReview(_)), "{left:?}");
    let Some(Step::ReplaceAndCopy(n)) = rows[note_row].step() else {
        panic!("{:?}", rows[note_row].step());
    };

    // Copy for AI on a row that is not Ready says why and hands nothing out.
    before(cx);
    shelf
        .update(cx, |view, window, cx| view.copy(letter_row, window, cx))
        .unwrap();
    shelf
        .read_with(cx, |view, cx| {
            assert_eq!(view.refused(cx), Some(letter_row));
            assert_eq!(view.copied(cx), None);
        })
        .unwrap();
    assert_eq!(clipboard(cx), "before");

    // "Replace N and copy" on the open tab: replaced, then copied.
    shelf
        .update(cx, |view, window, cx| {
            view.run_step(note_row, Step::ReplaceAndCopy(n), window, cx)
        })
        .unwrap();
    written(cx);
    let copied = clipboard(cx);
    let memo_document = cx.update(|_, cx| tabs.read(cx).tab_document(note, cx).unwrap());
    assert_eq!(vault.text_for_ai(memo_document.as_str()).unwrap(), copied);
    assert!(
        !copied.contains("Anna") && !copied.contains("Boris"),
        "{copied}"
    );

    // "Review" on a stored document: it opens with its review showing.
    let shelf = open_shelf(cx);
    let index = shelf
        .read_with(cx, |view, cx| {
            let documents = view.content(cx).documents;
            let key = ShelfKey::Document(letter_document.clone());
            documents.iter().position(|row| row.key == key).unwrap()
        })
        .unwrap();
    shelf
        .update(cx, |view, window, cx| {
            view.run_step(index, Step::Review, window, cx)
        })
        .unwrap();
    written(cx);
    let view = active(&tabs, cx);
    let shown = cx.update(|_, cx| tabs.read(cx).tab_document(tabs.read(cx).tabs.active(), cx));
    assert_eq!(shown.as_ref(), Some(&letter_document));
    assert!(reviewing(&view, cx));

    // "Find names" on one never scanned: it opens on its scan.
    let shelf = open_shelf(cx);
    let index = shelf
        .read_with(cx, |view, cx| {
            let documents = view.content(cx).documents;
            documents
                .iter()
                .position(|row| row.key == id(&unscanned))
                .unwrap()
        })
        .unwrap();
    shelf
        .update(cx, |view, window, cx| {
            view.run_step(index, Step::FindNames, window, cx)
        })
        .unwrap();
    written(cx);
    let view = active(&tabs, cx);
    let shown = cx.update(|_, cx| tabs.read(cx).tab_document(tabs.read(cx).tabs.active(), cx));
    assert_eq!(shown.map(|d| d.as_str().to_owned()), Some(unscanned));
    assert!(reviewing(&view, cx));
    // Its alias relinks as applied on opening: that is no scan.
    assert!(
        view.read_with(cx, |view, _| view.scan_asked()),
        "a scan starts"
    );

    // A row listed ready that is gone by its Copy for AI says so too.
    let shelf = open_shelf(cx);
    let (index, ready) = shelf
        .read_with(cx, |view, cx| {
            let documents = view.content(cx).documents;
            let at = (documents.iter())
                .position(|row| row.key == ShelfKey::Tab(note))
                .unwrap();
            (at, documents[at].readiness)
        })
        .unwrap();
    assert_eq!(ready, Readiness::Ready);
    let memo_document = cx.update(|_, cx| tabs.read(cx).tab_document(note, cx).unwrap());
    (vault.update_document(memo_document.as_str(), |d| d.exclude_from_ai = true)).unwrap();
    before(cx);
    shelf
        .update(cx, |view, window, cx| view.copy(index, window, cx))
        .unwrap();
    shelf
        .read_with(cx, |view, cx| {
            assert_eq!(view.refused(cx), Some(index));
            assert_eq!(view.copied(cx), None);
        })
        .unwrap();
    assert_eq!(clipboard(cx), "before");
}

/// Done when: Copy for AI by key on a shelf row that is not ready (⌘C on
/// macOS runs this action) copies nothing, focuses the row's step, and
/// Enter on it replaces the names and copies the checked text.
#[gpui::test]
fn the_shelf_keys_reach_a_refused_rows_step(cx: &mut TestAppContext) {
    let dir = tempfile::tempdir().unwrap();
    let (tabs, cx) = boot(cx, Session::default());
    cx.simulate_resize(size(px(1400.), px(850.)));
    cx.update(|_, cx| crate::shelf_ui::bind_keys(cx));
    open_proposed(&tabs, cx, dir.path(), "lease.md", TEXT, &["Anna Petrova"]);
    // Listed: one alias applied, one name still proposed.
    let view = active(&tabs, cx);
    view.update_in(cx, |view, window, cx| {
        view.apply_all_pii(&PiiApplyAll, window, cx)
    });
    view.update_in(cx, |view, _, cx| {
        let start = TEXT.find("Boris Ivanov").unwrap();
        let text = view.editor.read(cx).text().to_owned();
        let start = text.find("Boris Ivanov").unwrap_or(start);
        (view.pii.review_mut())
            .add_manual(&text, start..start + "Boris Ivanov".len(), Person)
            .unwrap();
        view.review_changed(cx);
    });
    written(cx);
    let shelf = open_shelf(cx);
    let mut shelf_cx = VisualTestContext::from_window(shelf.into(), cx);
    let row = shelf
        .read_with(&shelf_cx, |view, cx| view.content(cx).documents[0].clone())
        .unwrap();
    assert_eq!(row.step(), Some(Step::ReplaceAndCopy(1)));
    shelf_cx.write_to_clipboard(gpui::ClipboardItem::new_string("before".into()));
    shelf_cx.dispatch_action(crate::shelf_ui::ShelfCopyForAi);
    let focused = shelf
        .update(&mut shelf_cx, |view, window, cx| {
            assert_eq!(view.refused(cx), Some(0));
            view.rows().read(cx).step_focused(0, window)
        })
        .unwrap();
    assert!(focused, "focus moves to the step");
    assert_eq!(clipboard(&mut shelf_cx), "before");
    // Enter on the focused step; the shelf closes as mdoc runs it.
    shelf_cx.update(|window, cx| {
        use gpui::InputEvent;
        let keystroke = gpui::Keystroke::parse("enter").unwrap();
        let down = gpui::KeyDownEvent {
            keystroke: keystroke.clone(),
            is_held: false,
            prefer_character_input: false,
        };
        window.dispatch_event(down.to_platform_input(), cx);
        window.dispatch_event(gpui::KeyUpEvent { keystroke }.to_platform_input(), cx);
    });
    written(cx);
    let copied = clipboard(cx);
    assert!(
        copied.ends_with("PERSON_1 wrote to PERSON_2 about the lease."),
        "{copied}"
    );
}
