//! Home is a launcher (drop zone, quick actions, the Continue strip) with no
//! Projects or recents; the palette searches the Vault's titles and Project
//! names; Pseudonymize clipboard leaves the Vault as it was.
use super::*;
use std::time::SystemTime;

/// What the Vault holds: its summaries, and every file under its root with
/// its length and modified time.
#[derive(Debug, PartialEq)]
struct VaultState {
    documents: Vec<mdoc_vault::DocumentSummary>,
    projects: Vec<mdoc_vault::ProjectSummary>,
    replies: Vec<mdoc_vault::ReplySummary>,
    files: Vec<(PathBuf, u64, SystemTime)>,
}

fn vault_state(tabs: &Entity<Workspace>, cx: &mut VisualTestContext) -> VaultState {
    fn walk(dir: &std::path::Path, files: &mut Vec<(PathBuf, u64, SystemTime)>) {
        for entry in std::fs::read_dir(dir).unwrap() {
            let entry = entry.unwrap();
            let meta = entry.metadata().unwrap();
            if meta.is_dir() {
                walk(&entry.path(), files);
            } else {
                files.push((entry.path(), meta.len(), meta.modified().unwrap()));
            }
        }
    }
    let vault = vault(tabs, cx);
    let mut files = Vec::new();
    walk(vault.root(), &mut files);
    files.sort();
    VaultState {
        documents: vault.documents(),
        projects: vault.projects(),
        replies: vault.replies(),
        files,
    }
}

/// Let every autosave and the manifest's write finish.
fn settle(cx: &mut VisualTestContext) {
    for _ in 0..3 {
        wait(cx, 1_000);
    }
    draw(cx);
}

/// The document ids the Continue strip shows, in order.
fn strip_documents(tabs: &Entity<Workspace>, cx: &mut VisualTestContext) -> Vec<String> {
    cx.update(|_, cx| {
        let (strip, _) = tabs.read(cx).home_strip_now(cx).expect("the Vault is open");
        (strip.cards.iter())
            .filter_map(|card| match card {
                crate::workspace::home::Card::Document { summary, .. } => Some(summary.id.clone()),
                crate::workspace::home::Card::Reply(_) => None,
            })
            .collect()
    })
}

#[gpui::test]
fn home_is_a_launcher_without_projects_or_recents(cx: &mut TestAppContext) {
    let dir = tempfile::tempdir().unwrap();
    let (tabs, cx) = boot_home(cx, Session::default());
    cx.simulate_resize(size(px(1100.), px(900.)));
    draw(cx);
    for control in [
        "home-drop",
        "home-open",
        "home-new",
        "home-paste",
        "home-restore",
        "home-clipboard",
    ] {
        assert!(cx.debug_bounds(control).is_some(), "{control}");
    }
    // An empty Vault has nothing to continue.
    assert!(cx.debug_bounds("home-strip").is_none());

    // A Project and an opened file are not listed on Home.
    let ids = open_notes(&tabs, cx, dir.path(), &["lease", "notes"]);
    let project = new_project(&tabs, cx, ids[0]);
    tabs.update(cx, |tabs, cx| tabs.dismiss_toast(cx));
    click_toolbar(cx, "sidebar-home");
    assert!(cx.debug_bounds("home-strip").is_some());
    assert!(cx.debug_bounds("home-recent-0").is_none());
    let selector: &'static str = Box::leak(format!("home-project-{project}").into_boxed_str());
    assert!(cx.debug_bounds(selector).is_none());
    assert!(cx.debug_bounds("home-open-project").is_none());

    // New document leaves Home for a new live view.
    click_toolbar(cx, "home-new");
    cx.update(|_, cx| assert!(!tabs.read(cx).showing_home(cx)));
}

#[gpui::test]
fn the_strip_lists_documents_that_need_review_and_opens_them(cx: &mut TestAppContext) {
    use mdoc_pii::Category::Person;
    let dir = tempfile::tempdir().unwrap();
    let (tabs, cx) = boot_home(cx, Session::default());
    cx.simulate_resize(size(px(1100.), px(900.)));
    // Never scanned: both need review.
    let ids = open_notes(&tabs, cx, dir.path(), &["first", "second"]);
    let first = document_of(&tabs, ids[0], cx);
    let second = document_of(&tabs, ids[1], cx);
    // Applied and Ready: saved scanned, so it needs no review.
    let ready = open_applied(
        &tabs,
        cx,
        dir.path(),
        "ready.md",
        "Anna Petrova signed.",
        &[("Anna Petrova", Person)],
    );
    let ready_document = document_of(&tabs, ready, cx);
    settle(cx);
    click_toolbar(cx, "sidebar-home");
    let mut shown = strip_documents(&tabs, cx);
    shown.sort();
    let mut expected = vec![first.clone(), second.clone()];
    expected.sort();
    assert_eq!(shown, expected);
    assert!(cx.debug_bounds("home-strip-more").is_none());

    // A name proposed in the Ready document's view puts it back at once,
    // with its count.
    let view = cx.update(|_, cx| {
        tabs.read(cx)
            .tabs
            .get(ready)
            .unwrap()
            .view()
            .cloned()
            .unwrap()
    });
    view.update(cx, |view, cx| {
        let text = view.editor.read(cx).text().to_string();
        let start = text.find("signed").unwrap();
        view.pii
            .review_mut()
            .add_manual(&text, start..start + 6, Person)
            .unwrap();
        view.review_changed(cx);
    });
    draw(cx);
    cx.update(|_, cx| {
        let (strip, _) = tabs.read(cx).home_strip_now(cx).unwrap();
        assert!(strip.cards.iter().any(|card| matches!(
            card,
            crate::workspace::home::Card::Document { summary, proposed: Some(1) } if summary.id == ready_document
        )));
        assert_eq!(strip.need_review, 3);
    });

    // A click opens the card's document.
    click_toolbar(cx, "home-card-0");
    let opened = cx.update(|_, cx| {
        let w = tabs.read(cx);
        assert!(!w.showing_home(cx));
        let tab = w.tabs.active_tab().unwrap();
        w.live_document(tab, cx).unwrap().as_str().to_string()
    });
    assert!([first, second, ready_document].contains(&opened));
}

/// Each card is a Tab stop, and Enter opens it.
#[gpui::test]
fn strip_cards_are_reachable_by_tab_and_open_with_enter(cx: &mut TestAppContext) {
    let dir = tempfile::tempdir().unwrap();
    let (tabs, cx) = boot_home(cx, Session::default());
    cx.simulate_resize(size(px(1100.), px(900.)));
    open_notes(&tabs, cx, dir.path(), &["a", "b", "c", "d"]);
    click_toolbar(cx, "sidebar-home");
    tabs.update_in(cx, |tabs, window, cx| window.focus(&tabs.focus, cx));
    draw(cx);
    assert!(
        cx.debug_bounds("home-strip-more").is_some(),
        "4 need review, 3 shown"
    );
    let shown = strip_documents(&tabs, cx);
    let cards: Vec<_> = cx.update(|_, cx| {
        let focus = tabs.read(cx).shell.card_focus.borrow();
        (shown.iter()).map(|id| focus[id].clone()).collect()
    });
    let mut reached = [false; 3];
    for _ in 0..40 {
        key(cx, "tab");
        for (i, card) in cards.iter().enumerate() {
            reached[i] |= cx.update(|window, _| card.is_focused(window));
        }
        if reached.iter().all(|r| *r) {
            break;
        }
    }
    assert_eq!(reached, [true; 3], "Tab reaches every card");
    assert!(cx.update(|window, _| cards[2].is_focused(window)));
    let expected = shown[2].clone();
    activate_control(cx);
    cx.run_until_parked();
    cx.update(|_, cx| {
        let w = tabs.read(cx);
        assert!(!w.showing_home(cx));
        let tab = w.tabs.active_tab().unwrap();
        assert_eq!(w.live_document(tab, cx).unwrap().as_str(), expected);
    });
}

#[gpui::test]
fn the_strip_says_when_nothing_needs_review(cx: &mut TestAppContext) {
    use mdoc_pii::Category::Person;
    let dir = tempfile::tempdir().unwrap();
    let (tabs, cx) = boot_home(cx, Session::default());
    cx.simulate_resize(size(px(1100.), px(900.)));
    open_applied(
        &tabs,
        cx,
        dir.path(),
        "ready.md",
        "Anna Petrova signed.",
        &[("Anna Petrova", Person)],
    );
    settle(cx);
    click_toolbar(cx, "sidebar-home");
    assert!(cx.debug_bounds("home-strip-empty").is_some());
    assert!(cx.debug_bounds("home-card-0").is_none());
}

#[gpui::test]
fn the_palette_finds_documents_by_title_and_project_name(cx: &mut TestAppContext) {
    cx.update(shell::bind_keys);
    let dir = tempfile::tempdir().unwrap();
    let (tabs, cx) = boot_home(cx, Session::default());
    cx.simulate_resize(size(px(1100.), px(900.)));
    let ids = open_notes(&tabs, cx, dir.path(), &["Lease", "Notes", "Other"]);
    let project = new_project(&tabs, cx, ids[0]);
    move_tab(&tabs, cx, ids[1], Some(&project));
    tabs.update(cx, |tabs, cx| tabs.dismiss_toast(cx));
    let notes = document_of(&tabs, ids[1], cx);
    click_toolbar(cx, "sidebar-home");
    let name = project_of_tab(&tabs, cx, ids[0])
        .map(|id| super::project(&tabs, cx, &id).name)
        .unwrap();

    // An empty query lists no documents or Projects.
    cx.simulate_keystrokes("secondary-k");
    draw(cx);
    let section = |tabs: &Entity<Workspace>, cx: &mut VisualTestContext, s: &str| {
        tabs.read_with(cx, |tabs, cx| tabs.palette_section(s, cx))
    };
    assert!(section(&tabs, cx, "Documents").is_empty());
    assert!(section(&tabs, cx, "Projects").is_empty());
    assert!(section(&tabs, cx, "Open tabs").is_empty());
    assert!(section(&tabs, cx, "Recent").is_empty());

    // The Project's name finds its documents after the title match, and
    // the Project itself.
    cx.simulate_input(&name.to_lowercase());
    draw(cx);
    assert_eq!(section(&tabs, cx, "Documents"), ["Lease", "Notes"]);
    assert_eq!(section(&tabs, cx, "Projects"), std::slice::from_ref(&name));
    // Documents come first.
    cx.update(|_, cx| {
        let first = tabs.read(cx).palette_labels(cx);
        assert_eq!(first[0], "Lease", "{first:?}");
    });
    cx.simulate_keystrokes("escape");
    draw(cx);

    // A title opens with Enter.
    cx.simulate_keystrokes("secondary-k");
    cx.simulate_input("notes");
    draw(cx);
    assert_eq!(section(&tabs, cx, "Documents"), ["Notes"]);
    cx.simulate_keystrokes("enter");
    draw(cx);
    cx.update(|_, cx| {
        let w = tabs.read(cx);
        assert!(w.shell.palette.is_none() && !w.showing_home(cx));
        let tab = w.tabs.active_tab().unwrap();
        assert_eq!(w.live_document(tab, cx).unwrap().as_str(), notes);
    });

    // A Project opens its page.
    cx.simulate_keystrokes("secondary-k");
    cx.simulate_input(&format!("{} ", name.to_lowercase()));
    draw(cx);
    let index = cx.update(|_, cx| {
        let labels = tabs.read(cx).palette_labels(cx);
        labels.iter().rposition(|label| *label == name).unwrap()
    });
    tabs.update_in(cx, |tabs, window, cx| {
        tabs.palette_confirm(Some(index), window, cx)
    });
    draw(cx);
    cx.update(|_, cx| {
        let page = tabs
            .read(cx)
            .project_ui
            .page
            .as_ref()
            .map(|p| p.id().to_string());
        assert_eq!(page.as_deref(), Some(project.as_str()));
    });
}

#[gpui::test]
fn pseudonymize_clipboard_leaves_the_vault_unchanged(cx: &mut TestAppContext) {
    let dir = tempfile::tempdir().unwrap();
    let (tabs, cx, _) = applied_document(cx, dir.path(), Session::default());
    let tab = cx.update(|_, cx| tabs.read(cx).tabs.active());
    new_project(&tabs, cx, tab);
    tabs.update(cx, |tabs, cx| tabs.dismiss_toast(cx));
    settle(cx);
    let live = |tabs: &Entity<Workspace>, cx: &mut VisualTestContext| {
        cx.update(|_, cx| tabs.read(cx).tabs.iter().count())
    };
    let before = vault_state(&tabs, cx);
    let views = live(&tabs, cx);
    let documents = before.documents.len();

    // From Home: the document's Project names replace the clipboard's.
    click_toolbar(cx, "sidebar-home");
    cx.write_to_clipboard(gpui::ClipboardItem::new_string(
        "Call Anna Petrova at Northwind Traders Ltd.".into(),
    ));
    click_toolbar(cx, "home-clipboard");
    settle(cx);
    cx.update(|_, cx| {
        assert_eq!(
            cx.read_from_clipboard().unwrap().text().as_deref(),
            Some("Call PERSON_1 at ORG_1.")
        );
    });
    assert_eq!(vault_state(&tabs, cx), before);
    assert_eq!(live(&tabs, cx), views);

    // From the document, with names to replace and with none.
    cx.write_to_clipboard(gpui::ClipboardItem::new_string(
        "Anna Petrova agreed.".into(),
    ));
    cx.dispatch_action(PseudonymizeClipboard);
    settle(cx);
    cx.update(|_, cx| {
        assert!(!tabs.read(cx).showing_home(cx));
        assert_eq!(
            cx.read_from_clipboard().unwrap().text().as_deref(),
            Some("PERSON_1 agreed.")
        );
    });
    assert_eq!(vault_state(&tabs, cx), before);
    cx.write_to_clipboard(gpui::ClipboardItem::new_string("Nobody here.".into()));
    cx.dispatch_action(PseudonymizeClipboard);
    settle(cx);
    assert_eq!(vault_state(&tabs, cx), before);
    assert_eq!(live(&tabs, cx), views);
    assert_eq!(vault(&tabs, cx).documents().len(), documents);
}

#[gpui::test]
fn pseudonymize_clipboard_with_no_document_creates_none(cx: &mut TestAppContext) {
    let (tabs, cx) = boot_home(cx, Session::default());
    cx.simulate_resize(size(px(1100.), px(900.)));
    draw(cx);
    let before = vault_state(&tabs, cx);
    cx.write_to_clipboard(gpui::ClipboardItem::new_string("Call Anna Petrova.".into()));
    click_toolbar(cx, "home-clipboard");
    settle(cx);
    cx.update(|_, cx| {
        let w = tabs.read(cx);
        assert!(w.showing_home(cx));
        assert!(w.tabs.iter().all(|tab| w.is_placeholder(tab, cx)));
        assert_eq!(
            cx.read_from_clipboard().unwrap().text().as_deref(),
            Some("Call Anna Petrova.")
        );
    });
    assert_eq!(vault_state(&tabs, cx), before);
    assert!(before.documents.is_empty());
}

#[gpui::test]
fn a_saved_reply_on_the_strip_opens_its_reply_page(cx: &mut TestAppContext) {
    let dir = tempfile::tempdir().unwrap();
    let (tabs, cx) = boot_home(cx, Session::default());
    cx.simulate_resize(size(px(1100.), px(900.)));
    let ids = open_notes(&tabs, cx, dir.path(), &["lease"]);
    let document = document_of(&tabs, ids[0], cx);
    let reply = vault(&tabs, cx)
        .save_reply(mdoc_vault::NewReply {
            owner: mdoc_vault::ReplyOwner::Document(document),
            title: "Reply to lease".into(),
            source_app: None,
            markdown: "Thanks.\n".into(),
        })
        .unwrap();
    click_toolbar(cx, "sidebar-home");
    // The document still needs review: the header counts nothing extra.
    assert!(cx.debug_bounds("home-strip-more").is_none());
    let index = cx.update(|_, cx| {
        let (strip, _) = tabs.read(cx).home_strip_now(cx).unwrap();
        (strip.cards.iter())
            .position(
                |card| matches!(card, crate::workspace::home::Card::Reply(r) if r.id == reply.id),
            )
            .expect("the reply is on the strip")
    });
    let card: &'static str = Box::leak(format!("home-card-{index}").into_boxed_str());
    click_toolbar(cx, card);
    cx.update(|_, cx| {
        let page = tabs
            .read(cx)
            .reply_page
            .as_ref()
            .map(|page| page.id().to_string());
        assert_eq!(page.as_deref(), Some(reply.id.as_str()));
    });
}

#[gpui::test]
fn the_strip_says_nothing_needs_review_beside_reply_cards(cx: &mut TestAppContext) {
    use mdoc_pii::Category::Person;
    let dir = tempfile::tempdir().unwrap();
    let (tabs, cx) = boot_home(cx, Session::default());
    cx.simulate_resize(size(px(1100.), px(900.)));
    let ready = open_applied(
        &tabs,
        cx,
        dir.path(),
        "ready.md",
        "Anna Petrova signed.",
        &[("Anna Petrova", Person)],
    );
    let document = document_of(&tabs, ready, cx);
    vault(&tabs, cx)
        .save_reply(mdoc_vault::NewReply {
            owner: mdoc_vault::ReplyOwner::Document(document),
            title: "Reply to ready".into(),
            source_app: None,
            markdown: "Thanks.
"
            .into(),
        })
        .unwrap();
    settle(cx);
    click_toolbar(cx, "sidebar-home");
    assert!(cx.debug_bounds("home-card-0").is_some());
    assert!(cx.debug_bounds("home-strip-more").is_some());
    cx.update(|_, cx| {
        let (strip, _) = tabs.read(cx).home_strip_now(cx).unwrap();
        assert_eq!((strip.cards.len(), strip.need_review), (1, 0));
    });
}

/// Moving through a long result list keeps the selected row in view.
#[gpui::test]
fn the_palette_scrolls_to_the_selected_row(cx: &mut TestAppContext) {
    cx.update(shell::bind_keys);
    let (tabs, cx) = boot_home(cx, Session::default());
    cx.simulate_resize(size(px(1100.), px(900.)));
    let vault = vault(&tabs, cx);
    for i in 0..30 {
        vault
            .create_document(mdoc_vault::NewDocument {
                project: None,
                title: format!("Lease {i}"),
                source_kind: mdoc_vault::SourceKind::Markdown,
                origin: mdoc_vault::DocumentOrigin::Ordinary,
                markdown: format!("Lease {i}"),
            })
            .unwrap();
    }
    cx.simulate_keystrokes("secondary-k");
    cx.simulate_input("lease");
    draw(cx);
    assert_eq!(tabs.read_with(cx, |w, _| w.palette_offset()), px(0.));
    // Up from the first row selects the last.
    cx.simulate_keystrokes("up");
    draw(cx);
    draw(cx);
    assert!(tabs.read_with(cx, |w, _| w.palette_offset()) < px(0.));
    // A new query starts at the top again.
    cx.simulate_input(" 1");
    draw(cx);
    assert_eq!(tabs.read_with(cx, |w, _| w.palette_offset()), px(0.));
}

/// Review of #1145, round 2 (P2): a document saved Ready once, then left
/// with a proposal, still needs review on Home and its Project page after
/// a restart: its save counted the proposal.
#[gpui::test]
fn a_pending_proposal_needs_review_after_a_restart(cx: &mut TestAppContext) {
    use mdoc_pii::Category::Person;
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path().join("vault");
    let (tabs, cx) = boot_on_vault(cx, Session::default(), &root);
    let text = "Anna Petrova met Boris Ivanov.";
    let tab = open_applied(
        &tabs,
        cx,
        dir.path(),
        "a.md",
        text,
        &[("Anna Petrova", Person)],
    );
    let project = new_project(&tabs, cx, tab);
    settle(cx);
    let id = cx.update(|_, cx| tabs.read(cx).tab_document(tab, cx).unwrap());
    let vault = vault(&tabs, cx);
    let stored = vault.document(id.as_str()).unwrap();
    assert!(stored.scanned, "saved Ready once");
    // A new proposal, left as it is, and saved.
    let view = active(&tabs, cx);
    view.update(cx, |view, cx| {
        let shown = view.editor.read(cx).text().to_string();
        let start = shown.find("Boris Ivanov").unwrap();
        view.pii
            .review_mut()
            .add_manual(&shown, start..start + "Boris Ivanov".len(), Person)
            .unwrap();
        view.save_now(cx);
    });
    settle(cx);
    assert_eq!(
        vault
            .document(id.as_str())
            .unwrap()
            .counts
            .map(|c| c.proposed),
        Some(1)
    );
    tabs.update_in(cx, |tabs, window, cx| tabs.quit(window, cx));
    cx.run_until_parked();
    drop(vault);
    // Restarted with no tabs: no live view counts it.
    let (tabs, cx) = boot_on_vault(&mut cx.cx, Session::default(), &root);
    draw(cx);
    assert_eq!(strip_documents(&tabs, cx), [id.as_str().to_string()]);
    cx.update(|_, cx| {
        let (strip, _) = tabs.read(cx).home_strip_now(cx).unwrap();
        assert_eq!(strip.need_review, 1);
        let Some(crate::workspace::home::Card::Document { proposed, .. }) = strip.cards.first()
        else {
            panic!("the document's card");
        };
        assert_eq!(*proposed, Some(1));
    });
    // The Project page's row says so too.
    cx.update(|_, cx| {
        let tabs = tabs.read(cx);
        let summary = (tabs.opened_vault().unwrap().documents_in(Some(&project)))
            .into_iter()
            .next()
            .unwrap();
        assert!(tabs.member_needs_review(&summary, cx));
    });
}
