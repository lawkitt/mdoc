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
    let cards: Vec<_> = cx.update(|_, cx| tabs.read(cx).shell.card_focus.borrow()[..3].to_vec());
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
    let expected = strip_documents(&tabs, cx)[2].clone();
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
    assert_eq!(section(&tabs, cx, "Projects"), [name.clone()]);
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

    // From the document, and with nothing to replace.
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
