//! The AI project in the window (Milestone 2 item 8): the share prompt,
//! the indicator with Stop sharing, the scan on entry and the Project
//! page's AI activity, all only with "AI connector (preview)" on. An AI
//! app's calls go through the handler the connector's listener runs.
use super::*;
use mdoc_mcp::wire::{self, Answer, Call, Done, Kind, Refusal, Request};

/// Two Projects in a Vault in `dir`, "Lease" (pinned) and "Estate", each
/// with one Ready document: their ids and the documents'.
pub(super) struct Fixture {
    pub(super) dir: tempfile::TempDir,
    pub(super) lease: String,
    pub(super) lease_doc: String,
    pub(super) estate: String,
    pub(super) estate_doc: String,
}

pub(super) fn fixture() -> Fixture {
    let dir = tempfile::tempdir().unwrap();
    let vault = mdoc_vault::Vault::open(dir.path().join("vault"), &[3; 32]).unwrap();
    let mut made = Vec::new();
    // "Lease" is pinned, so it lists first.
    for (name, text, pinned) in [
        ("Estate", "PERSON_1 inherits.", false),
        ("Lease", "PERSON_1 rents.", true),
    ] {
        let project = vault
            .create_project(mdoc_vault::NewProject {
                name: name.into(),
                colour: None,
                pinned,
            })
            .unwrap();
        let document = vault
            .create_document(mdoc_vault::NewDocument {
                project: Some(project.id.clone()),
                title: format!("{name} memo"),
                source_kind: mdoc_vault::SourceKind::Markdown,
                origin: mdoc_vault::DocumentOrigin::Ordinary,
                markdown: text.into(),
            })
            .unwrap();
        vault
            .update_document(&document.id, |d| {
                d.scanned = true;
                d.set_ready(true);
            })
            .unwrap();
        made.push((project.id, document.id));
    }
    vault.flush().unwrap();
    let [(estate, estate_doc), (lease, lease_doc)] = <[_; 2]>::try_from(made).unwrap();
    Fixture {
        dir,
        lease,
        lease_doc,
        estate,
        estate_doc,
    }
}

pub(super) fn boot_fixture<'a>(
    cx: &'a mut TestAppContext,
    fixture: &Fixture,
) -> (Entity<Workspace>, &'a mut VisualTestContext) {
    let (tabs, cx) = boot_on_vault(cx, Session::default(), &fixture.dir.path().join("vault"));
    cx.simulate_resize(size(px(1400.), px(850.)));
    tabs.update(cx, |tabs, cx| tabs.load_projects(cx));
    draw(cx);
    (tabs, cx)
}

/// Turn "AI connector (preview)" on or off, as Settings applying it does.
pub(super) fn set_preview(tabs: &Entity<Workspace>, cx: &mut VisualTestContext, on: bool) {
    tabs.update_in(cx, |tabs, window, cx| {
        (tabs.preferences.borrow_mut().current.as_mut().unwrap())
            .ai_connector
            .preview = on;
        tabs.refresh_ai_connector(window, cx);
    });
    cx.run_until_parked();
    draw(cx);
}

/// AI app "claude-ai" makes `call`, as through the pipe; the window hears
/// of it at its next poll.
pub(super) fn call(tabs: &Entity<Workspace>, cx: &mut VisualTestContext, call: Call) -> Answer {
    let answer = cx.update(|_, cx| {
        let tabs = tabs.read(cx);
        let running = tabs.ai_connector.as_ref().expect("the connector runs");
        running.call(&Request {
            v: wire::VERSION,
            app: "claude-ai".into(),
            call,
        })
    });
    wait(cx, 300);
    answer
}

fn list(tabs: &Entity<Workspace>, cx: &mut VisualTestContext) -> Answer {
    call(tabs, cx, Call::ListDocuments)
}

pub(super) fn shared(tabs: &Entity<Workspace>, cx: &mut VisualTestContext) -> Option<String> {
    cx.update(|_, cx| tabs.read(cx).ai_connector.as_ref()?.connector.ai_project())
}

fn prompt(tabs: &Entity<Workspace>, cx: &mut VisualTestContext) -> Option<Vec<String>> {
    draw(cx);
    let shown = cx.debug_bounds("ai-share-prompt").is_some();
    let prompt = cx.update(|_, cx| tabs.read(cx).ai.prompt().map(|p| p.choices.clone()));
    assert_eq!(
        shown,
        prompt.is_some(),
        "the card shows what the state holds"
    );
    prompt
}

/// The pseudonymization model as Settings found it: set up, or missing.
fn pii_model(tabs: &Entity<Workspace>, cx: &mut VisualTestContext, ready: bool) {
    let (panel, model) = cx.update(|_, cx| {
        let tabs = tabs.read(cx);
        let config = tabs
            .preferences
            .borrow()
            .snapshot()
            .unwrap()
            .pseudonymization;
        (tabs.settings.clone(), settings::Model::Pii(config.model))
    });
    panel.update(cx, |panel, cx| {
        panel.pending.fill(Default::default());
        panel.statuses[settings_ui::Panel::index(model)] = if ready {
            settings_ui::Status::Ready
        } else {
            settings_ui::Status::Missing
        };
        cx.notify();
    });
}

pub(super) fn show_page(tabs: &Entity<Workspace>, cx: &mut VisualTestContext, id: &str) {
    let id = id.to_string();
    tabs.update_in(cx, |tabs, window, cx| {
        tabs.show_project_page(
            &id,
            crate::workspace::project_page::Place::Header,
            window,
            cx,
        )
    });
    cx.run_until_parked();
    draw(cx);
}

pub(super) fn open_member(tabs: &Entity<Workspace>, cx: &mut VisualTestContext, id: &str) {
    let id = mdoc_store::document_id::VaultId::parse(id).unwrap();
    tabs.update_in(cx, |tabs, window, cx| {
        tabs.open_vault_document(id, window, cx)
    });
    cx.run_until_parked();
    draw(cx);
}

/// Done when: the prompt appears only when an AI app asks while nothing
/// is shared, and Not now keeps it away for a while.
#[gpui::test]
fn the_prompt_appears_only_when_nothing_is_shared(cx: &mut TestAppContext) {
    let fixture = fixture();
    let (tabs, cx) = boot_fixture(cx, &fixture);
    set_preview(&tabs, cx, true);
    // Nobody asked yet.
    assert_eq!(prompt(&tabs, cx), None);
    // Asked while Settings is open: the card waits for it to close, so it
    // never sits above the dialog.
    cx.dispatch_action(Settings);
    cx.run_until_parked();
    assert_eq!(list(&tabs, cx), Answer::Refused(Refusal::NothingShared));
    draw(cx);
    assert!(cx.update(|_, cx| tabs.read(cx).ai.prompt().is_some()));
    assert!(cx.debug_bounds("ai-share-prompt").is_none());
    tabs.update_in(cx, |tabs, window, cx| {
        (tabs.settings).update(cx, |panel, cx| panel.close(window, cx))
    });
    cx.run_until_parked();
    let choices = prompt(&tabs, cx).expect("asking with nothing shared prompts");
    // Nothing viewed yet: the Projects as the sidebar lists them.
    assert_eq!(choices, [fixture.lease.clone(), fixture.estate.clone()]);
    click_toolbar(cx, "ai-share");
    assert_eq!(shared(&tabs, cx), Some(fixture.lease.clone()));
    assert_eq!(prompt(&tabs, cx), None);
    // Shared: an ask is answered, and nothing prompts.
    assert!(matches!(
        list(&tabs, cx),
        Answer::Ok(Done::Documents { .. })
    ));
    assert_eq!(prompt(&tabs, cx), None);
    // Nothing shared again: the next ask prompts, and Not now hides it
    // through later asks.
    tabs.update(cx, |tabs, cx| tabs.stop_sharing_with_ai(cx));
    list(&tabs, cx);
    assert!(prompt(&tabs, cx).is_some());
    click_toolbar(cx, "ai-share-not-now");
    assert_eq!(prompt(&tabs, cx), None);
    assert_eq!(list(&tabs, cx), Answer::Refused(Refusal::NothingShared));
    assert_eq!(prompt(&tabs, cx), None);
    assert_eq!(shared(&tabs, cx), None);
}

/// Done when: the share never follows the view. The prompt offers the
/// Project last viewed first; only Share shares, and viewing another
/// Project or its documents never changes what is shared.
#[gpui::test]
fn the_share_does_not_follow_the_view(cx: &mut TestAppContext) {
    let fixture = fixture();
    let (tabs, cx) = boot_fixture(cx, &fixture);
    set_preview(&tabs, cx, true);
    show_page(&tabs, cx, &fixture.estate);
    list(&tabs, cx);
    assert_eq!(
        prompt(&tabs, cx).unwrap(),
        [fixture.estate.clone(), fixture.lease.clone()],
        "the last-viewed Project comes first"
    );
    // Viewing another Project while it shows chooses nothing.
    open_member(&tabs, cx, &fixture.lease_doc);
    assert_eq!(shared(&tabs, cx), None);
    click_toolbar(cx, "ai-share");
    assert_eq!(shared(&tabs, cx), Some(fixture.estate.clone()));
    // The view moves on; the share stays.
    show_page(&tabs, cx, &fixture.lease);
    open_member(&tabs, cx, &fixture.lease_doc);
    tabs.update_in(cx, |tabs, window, cx| tabs.show_home(window, cx));
    cx.run_until_parked();
    assert_eq!(shared(&tabs, cx), Some(fixture.estate.clone()));
    let Answer::Ok(Done::Documents { documents }) = list(&tabs, cx) else {
        panic!("the shared Project is listed");
    };
    let ids: Vec<_> = documents.into_iter().map(|row| row.doc_id).collect();
    assert_eq!(ids, std::slice::from_ref(&fixture.estate_doc));
    // After Stop sharing, the Project viewed since is offered first, and
    // the one chosen is shared, not the one in view.
    tabs.update(cx, |tabs, cx| tabs.stop_sharing_with_ai(cx));
    list(&tabs, cx);
    assert_eq!(prompt(&tabs, cx).unwrap()[0], fixture.lease);
    click_toolbar(cx, "ai-share-choice-1");
    click_toolbar(cx, "ai-share");
    assert_eq!(shared(&tabs, cx), Some(fixture.estate.clone()));
    // Closing a tab brings its neighbour to the front, unrecorded in the
    // history: that one is the Project last viewed.
    tabs.update(cx, |tabs, cx| tabs.stop_sharing_with_ai(cx));
    open_member(&tabs, cx, &fixture.estate_doc);
    open_member(&tabs, cx, &fixture.lease_doc);
    tabs.update_in(cx, |tabs, window, cx| {
        tabs.close_tab(tabs.tabs.active(), window, cx)
    });
    cx.run_until_parked();
    list(&tabs, cx);
    assert_eq!(prompt(&tabs, cx).unwrap()[0], fixture.estate);
}

/// Done when: Stop sharing, from the sidebar or the Project page, clears
/// the share: AI apps read nothing, and the indicator says so.
#[gpui::test]
fn stop_sharing_clears_the_share(cx: &mut TestAppContext) {
    let fixture = fixture();
    let (tabs, cx) = boot_fixture(cx, &fixture);
    set_preview(&tabs, cx, true);
    expand_sidebar(&tabs, cx);
    assert!(cx.debug_bounds("ai-project").is_some());
    assert!(
        cx.debug_bounds("ai-stop-sharing").is_none(),
        "nothing shared"
    );
    tabs.update(cx, |tabs, cx| {
        tabs.share_with_ai(&fixture.lease.clone(), cx)
    });
    draw(cx);
    assert!(cx.debug_bounds("ai-project-open").is_some());
    click_toolbar(cx, "ai-stop-sharing");
    assert_eq!(shared(&tabs, cx), None);
    assert!(cx.debug_bounds("ai-stop-sharing").is_none());
    let read = Call::GetDocument {
        doc_id: fixture.lease_doc.clone(),
        part: None,
    };
    assert_eq!(
        call(&tabs, cx, read.clone()),
        Answer::Refused(Refusal::NothingShared)
    );
    // The Project page's own Stop sharing, only on the AI project's page.
    tabs.update(cx, |tabs, cx| {
        tabs.ai.not_now(std::time::Instant::now());
        tabs.share_with_ai(&fixture.lease.clone(), cx)
    });
    show_page(&tabs, cx, &fixture.estate);
    assert!(cx.debug_bounds("project-page-ai").is_none());
    show_page(&tabs, cx, &fixture.lease);
    assert!(cx.debug_bounds("project-page-ai").is_some());
    click_toolbar(cx, "project-page-stop-sharing");
    assert_eq!(shared(&tabs, cx), None);
    assert!(cx.debug_bounds("project-page-ai").is_none());
    assert_eq!(
        call(&tabs, cx, read),
        Answer::Refused(Refusal::NothingShared)
    );
}

/// Done when: a document entering the AI project is scanned in its tab,
/// as Pseudonymize scans, and nothing is replaced by itself.
#[gpui::test]
fn a_document_entering_the_ai_project_is_scanned_not_replaced(cx: &mut TestAppContext) {
    crate::document_view::DocumentView::hold_scans(true);
    let fixture = fixture();
    let (tabs, cx) = boot_fixture(cx, &fixture);
    set_preview(&tabs, cx, true);
    tabs.update(cx, |tabs, cx| {
        tabs.share_with_ai(&fixture.lease.clone(), cx)
    });
    let opened = std::cell::Cell::new(0);
    let open = |cx: &mut VisualTestContext| {
        opened.set(opened.get() + 1);
        let path = fixture
            .dir
            .path()
            .join(format!("signing {}.md", opened.get()));
        // Each its own source, or Open finds the first again.
        let text = format!("Maria Lopez signs for Bob, copy {}.", opened.get());
        std::fs::write(&path, text).unwrap();
        tabs.update_in(cx, |tabs, window, cx| tabs.open_path(path, window, cx));
        cx.run_until_parked();
        cx.update(|_, cx| tabs.read(cx).tabs.active())
    };
    let scanning = |cx: &mut VisualTestContext, id: u64| {
        let view = cx.update(|_, cx| tabs.read(cx).tab_view(id).unwrap());
        cx.update(|_, cx| view.read(cx).pii.scanning())
    };
    // Unfiled, and then into a Project that isn't shared: no scan.
    pii_model(&tabs, cx, true);
    let elsewhere = open(cx);
    assert!(!scanning(cx, elsewhere));
    move_tab(&tabs, cx, elsewhere, Some(&fixture.estate));
    assert_eq!(
        project_of_tab(&tabs, cx, elsewhere),
        Some(fixture.estate.clone())
    );
    assert!(!scanning(cx, elsewhere));
    // Into the AI project while the model isn't set up: no scan, and no
    // setup card by itself.
    pii_model(&tabs, cx, false);
    let waiting = open(cx);
    move_tab(&tabs, cx, waiting, Some(&fixture.lease));
    assert_eq!(
        project_of_tab(&tabs, cx, waiting),
        Some(fixture.lease.clone())
    );
    assert!(!scanning(cx, waiting));
    draw(cx);
    assert!(cx.debug_bounds("pii-setup-primary").is_none());
    // Once Settings has set the model up, the waiting one scans.
    pii_model(&tabs, cx, true);
    let (panel, model) = cx.update(|_, cx| {
        let tabs = tabs.read(cx);
        let config = tabs
            .preferences
            .borrow()
            .snapshot()
            .unwrap()
            .pseudonymization;
        (tabs.settings.clone(), settings::Model::Pii(config.model))
    });
    panel.update(cx, |_, cx| {
        cx.emit(settings_ui::Event::Finished(model, Ok(None)))
    });
    cx.run_until_parked();
    assert!(
        scanning(cx, waiting),
        "a member that waited for the model scans"
    );
    // Its scan fails: later loads and shares don't start it again.
    let view = cx.update(|_, cx| tabs.read(cx).tab_view(waiting).unwrap());
    view.update(cx, |view, cx| {
        view.finish_scan(Err("The model failed.".into()), cx)
    });
    tabs.update(cx, |tabs, cx| tabs.scan_on_entry(cx));
    assert!(
        !scanning(cx, waiting),
        "a failed entry scan does not repeat"
    );
    // Closed and opened again, in a new tab, it scans there.
    let document = document_of(&tabs, waiting, cx);
    tabs.update_in(cx, |tabs, window, cx| tabs.close_tab(waiting, window, cx));
    cx.run_until_parked();
    open_member(&tabs, cx, &document);
    let reopened = cx.update(|_, cx| tabs.read(cx).tabs.active());
    assert_ne!(reopened, waiting);
    assert!(
        scanning(cx, reopened),
        "a reopened member scans in its new tab"
    );
    // Into the AI project: its tab scans.
    let entering = open(cx);
    move_tab(&tabs, cx, entering, Some(&fixture.lease));
    assert_eq!(
        project_of_tab(&tabs, cx, entering),
        Some(fixture.lease.clone())
    );
    assert!(
        scanning(cx, entering),
        "a document entering the AI project scans"
    );
    let view = active(&tabs, cx);
    view.update(cx, |view, cx| {
        let source = view.editor.read(cx).text().to_owned();
        let start = source.find("Maria Lopez").unwrap();
        view.finish_scan(
            Ok(vec![mdoc_pii::Detection {
                range: start..start + "Maria Lopez".len(),
                category: mdoc_pii::Category::Person,
                score: 0.9,
                recognizer: mdoc_pii::Recognizer::Model,
            }]),
            cx,
        );
    });
    cx.run_until_parked();
    cx.update(|_, cx| {
        let view = view.read(cx);
        assert_eq!(view.pii.review().remaining(), 1, "proposed");
        assert!(view.pii.review().applied().is_empty(), "never replaced");
        assert!(view.editor.read(cx).text().contains("Maria Lopez"));
    });
    // It scans once: a later load or share starts nothing.
    tabs.update(cx, |tabs, cx| tabs.scan_on_entry(cx));
    assert!(!scanning(cx, entering));
    // The other Project shared now: its open member enters, and scans.
    tabs.update(cx, |tabs, cx| {
        tabs.share_with_ai(&fixture.estate.clone(), cx)
    });
    assert!(scanning(cx, elsewhere));
    crate::document_view::DocumentView::hold_scans(false);
}

/// Done when: the Project page lists AI activity, reads and deliveries,
/// with the app by its product's name, newest first, and keeps up as
/// calls come in.
#[gpui::test]
fn the_project_page_lists_ai_activity(cx: &mut TestAppContext) {
    let fixture = fixture();
    let (tabs, cx) = boot_fixture(cx, &fixture);
    set_preview(&tabs, cx, true);
    tabs.update(cx, |tabs, cx| {
        tabs.share_with_ai(&fixture.lease.clone(), cx)
    });
    let read = Call::GetDocument {
        doc_id: fixture.lease_doc.clone(),
        part: None,
    };
    assert!(matches!(
        call(&tabs, cx, read.clone()),
        Answer::Ok(Done::Text { .. })
    ));
    show_page(&tabs, cx, &fixture.lease);
    assert!(cx.debug_bounds("project-activity").is_none());
    click_toolbar(cx, "project-segment-activity");
    // Rows are numbered from the oldest.
    assert!(cx.debug_bounds("project-activity-1").is_some());
    assert!(cx.debug_bounds("project-activity-2").is_none());
    // A delivery while the page shows it joins the list at the top.
    let delivered = call(
        &tabs,
        cx,
        Call::Deliver {
            kind: Kind::Reply,
            title: Some("Rent check".into()),
            read: vec![fixture.lease_doc.clone()],
            markdown: "PERSON_1 pays on time.".into(),
        },
    );
    assert!(matches!(delivered, Answer::Ok(Done::Delivered { .. })));
    draw(cx);
    assert!(cx.debug_bounds("project-activity-2").is_some());
    let lines: Vec<String> = cx.update(|_, cx| {
        let tabs = tabs.read(cx);
        let Some(Ok(entries)) = tabs.ai.activity(&fixture.lease) else {
            panic!("the activity is read");
        };
        let vault = tabs.opened_vault().unwrap();
        let documents = (vault.documents().into_iter())
            .map(|summary| (summary.id, summary.title))
            .collect();
        let replies = (vault.project_replies(&fixture.lease).into_iter())
            .map(|reply| (reply.id, reply.title))
            .collect();
        (entries.iter())
            .map(|entry| {
                crate::workspace::ai_project_ui::activity_line(entry, &documents, &replies, 0).text
            })
            .collect()
    });
    assert_eq!(
        lines,
        [
            "Claude delivered a reply “Rent check”",
            "Claude read “Lease memo”",
        ]
    );
    // #1220: the delivered reply keeps the id the app gave itself; its
    // pages name the product.
    let detail = cx.update(|_, cx| {
        let tabs = tabs.read(cx);
        let vault = tabs.opened_vault().unwrap();
        let reply = vault.project_replies(&fixture.lease).pop().unwrap();
        assert_eq!(reply.source_app.as_deref(), Some("claude-ai"));
        crate::workspace::saved_replies::reply_detail(
            &reply.owner,
            reply.source_app.as_deref(),
            reply.created,
            &Default::default(),
        )
    });
    assert!(
        detail.starts_with("From the Project · Claude · "),
        "{detail}"
    );
    // Another Project's page lists only its own.
    show_page(&tabs, cx, &fixture.estate);
    click_toolbar(cx, "project-segment-activity");
    assert!(cx.debug_bounds("project-activity-1").is_none());
}

/// Done when: with "AI connector (preview)" off, none of it is shown or
/// reachable; turning it off clears the share, and on again shares
/// nothing.
#[gpui::test]
fn with_the_setting_off_nothing_shows(cx: &mut TestAppContext) {
    let fixture = fixture();
    let (tabs, cx) = boot_fixture(cx, &fixture);
    let hidden = |cx: &mut VisualTestContext| {
        draw(cx);
        cx.debug_bounds("ai-project").is_none()
            && cx.debug_bounds("rail-ai-project").is_none()
            && cx.debug_bounds("ai-share-prompt").is_none()
    };
    expand_sidebar(&tabs, cx);
    assert!(cx.update(|_, cx| tabs.read(cx).ai_connector.is_none()));
    assert!(hidden(cx));
    show_page(&tabs, cx, &fixture.lease);
    assert!(cx.debug_bounds("project-segment-activity").is_none());
    collapse(&tabs, cx);
    assert!(hidden(cx));
    // On: the rail shows it, and the page has its segment.
    set_preview(&tabs, cx, true);
    assert!(cx.debug_bounds("rail-ai-project").is_some());
    assert!(cx.debug_bounds("project-segment-activity").is_some());
    tabs.update(cx, |tabs, cx| {
        tabs.share_with_ai(&fixture.lease.clone(), cx)
    });
    click_toolbar(cx, "project-segment-activity");
    list(&tabs, cx);
    // Off again: everything goes, the page falls back to its documents,
    // and the share with it.
    set_preview(&tabs, cx, false);
    assert!(hidden(cx));
    assert!(cx.debug_bounds("project-segment-activity").is_none());
    assert!(cx.debug_bounds("project-activity").is_none());
    assert!(cx.debug_bounds("project-page-ai").is_none());
    assert!(cx.debug_bounds("project-documents").is_some());
    // On once more: memory only, nothing is shared.
    set_preview(&tabs, cx, true);
    assert_eq!(shared(&tabs, cx), None);
    assert!(cx.debug_bounds("project-page-ai").is_none());
}

// ---- Item 9: notifications and where a reply lands -----------------------

use crate::workspace::ai_deliveries::{Note, Notifier};
use std::{cell::RefCell, rc::Rc};

pub(super) type Notes = Rc<RefCell<Vec<Note>>>;

/// The window's notifications go to a recorder, never the OS.
pub(super) fn record_notes(tabs: &Entity<Workspace>, cx: &mut VisualTestContext) -> Notes {
    let (notifier, notes) = Notifier::recording();
    tabs.update(cx, |tabs, _| tabs.notifier = notifier);
    notes
}

/// The connector on, "Lease" shared, the sidebar open, and the window in
/// the background, as while the user is in the AI app.
fn shared_lease<'a>(
    cx: &'a mut TestAppContext,
    fixture: &Fixture,
) -> (Entity<Workspace>, &'a mut VisualTestContext, Notes) {
    let (tabs, cx) = boot_fixture(cx, fixture);
    let notes = record_notes(&tabs, cx);
    expand_sidebar(&tabs, cx);
    set_preview(&tabs, cx, true);
    tabs.update(cx, |tabs, cx| {
        tabs.share_with_ai(&fixture.lease.clone(), cx)
    });
    cx.deactivate_window();
    draw(cx);
    (tabs, cx, notes)
}

fn deliver(
    tabs: &Entity<Workspace>,
    cx: &mut VisualTestContext,
    fixture: &Fixture,
    kind: Kind,
) -> (String, Option<String>) {
    // A delivery names a document the app read.
    call(
        tabs,
        cx,
        Call::GetDocument {
            doc_id: fixture.lease_doc.clone(),
            part: None,
        },
    );
    let delivered = call(
        tabs,
        cx,
        Call::Deliver {
            kind,
            title: Some("Rent check for Lease".into()),
            read: vec![fixture.lease_doc.clone()],
            markdown: "PERSON_1 pays on time.".into(),
        },
    );
    let Answer::Ok(Done::Delivered {
        reply, document, ..
    }) = delivered
    else {
        panic!("delivered: {delivered:?}");
    };
    (reply, document)
}

fn window_active(cx: &mut VisualTestContext) -> bool {
    cx.update(|window, _| window.is_window_active())
}

fn reply_shown(tabs: &Entity<Workspace>, cx: &mut VisualTestContext) -> Option<String> {
    cx.update(|_, cx| Some(tabs.read(cx).reply_page.as_ref()?.id().to_string()))
}

fn document_shown(tabs: &Entity<Workspace>, cx: &mut VisualTestContext) -> Option<String> {
    cx.update(|_, cx| {
        let tabs = tabs.read(cx);
        if tabs.reply_page.is_some() || tabs.project_ui.page.is_some() {
            return None;
        }
        let view = tabs.active_view()?;
        let document = view.read(cx).session.document.vault.clone()?;
        Some(document.as_str().to_string())
    })
}

/// The document in front shows its Review panel, as Pseudonymize opens
/// it.
fn review_open(tabs: &Entity<Workspace>, cx: &mut VisualTestContext) -> bool {
    cx.update(|_, cx| {
        let view = tabs.read(cx).active_view();
        view.is_some_and(|view| {
            let view = view.read(cx);
            view.replacements_open() && view.inspector == crate::document_view::Inspector::Review
        })
    })
}

/// The user clicks the notification tagged `tag`, as the OS reports it.
fn click_note(cx: &mut VisualTestContext, tag: &str) {
    cx.simulate_system_notification_response(gpui::SystemNotificationResponse {
        tag: tag.to_string().into(),
        action_id: None,
    });
    cx.run_until_parked();
    draw(cx);
}

/// A notification never holds a title, a restored name or the text.
fn assert_private(notes: &[Note]) {
    for note in notes {
        let all = format!("{} {} {}", note.tag, note.title, note.body);
        for secret in ["Lease", "Rent", "PERSON", "pays", "memo"] {
            assert!(!all.contains(secret), "{secret} in {note:?}");
        }
    }
}

/// Done when: a delivered reply raises one notification and the badge,
/// takes no focus and moves nothing; clicking the notification brings
/// the window forward on its Reply page, and the badge clears.
#[gpui::test]
fn a_delivered_reply_notifies_and_its_click_opens_the_reply_page(cx: &mut TestAppContext) {
    let fixture = fixture();
    let (tabs, cx, notes) = shared_lease(cx, &fixture);
    assert!(cx.debug_bounds("ai-badge").is_none());
    let (reply, document) = deliver(&tabs, cx, &fixture, Kind::Reply);
    assert_eq!(document, None);
    assert_eq!(
        *notes.borrow(),
        [Note {
            tag: format!("ai-reply-{reply}"),
            title: "Claude sent a reply".into(),
            body: "Open it in mdoc to read it with the names restored.".into(),
        }]
    );
    assert_private(&notes.borrow());
    // Nothing moved and nothing took focus; the badge counts it.
    assert!(!window_active(cx));
    assert_eq!(reply_shown(&tabs, cx), None);
    assert!(cx.debug_bounds("ai-badge").is_some());
    collapse(&tabs, cx);
    assert!(cx.debug_bounds("rail-ai-badge").is_some());
    expand_sidebar(&tabs, cx);
    // The click: in front, on the Reply page.
    click_note(cx, &format!("ai-reply-{reply}"));
    assert!(window_active(cx));
    assert_eq!(reply_shown(&tabs, cx).as_deref(), Some(reply.as_str()));
    assert!(cx.debug_bounds("ai-badge").is_none());
    // Another app's notification opens nothing.
    click_note(cx, "thread-1");
    assert_eq!(reply_shown(&tabs, cx).as_deref(), Some(reply.as_str()));
    assert_eq!(notes.borrow().len(), 1);
}

/// Done when: a delivered Document notifies as a document and opens for
/// review, from the badge as from the notification.
#[gpui::test]
fn a_delivered_document_opens_for_review(cx: &mut TestAppContext) {
    crate::document_view::DocumentView::hold_scans(true);
    let fixture = fixture();
    let (tabs, cx, notes) = shared_lease(cx, &fixture);
    let (_, document) = deliver(&tabs, cx, &fixture, Kind::Document);
    let document = document.expect("landed as a Document");
    assert_eq!(notes.borrow().len(), 1);
    let note = notes.borrow()[0].clone();
    assert_eq!(note.tag, format!("ai-document-{document}"));
    assert_eq!(note.title, "Claude sent a document");
    assert_eq!(note.body, "Open it in mdoc to review it.");
    assert_private(&notes.borrow());
    assert_eq!(document_shown(&tabs, cx), None);
    // The badge opens the newest: the Document, in its tab.
    click_toolbar(cx, "ai-badge");
    draw(cx);
    assert_eq!(
        document_shown(&tabs, cx).as_deref(),
        Some(document.as_str())
    );
    assert!(review_open(&tabs, cx), "it opens for review");
    assert!(cx.debug_bounds("ai-badge").is_none());
    // Closed, with sharing stopped, its notification still opens it for
    // review: the scan on entry no longer would.
    click_toolbar(cx, "ai-stop-sharing");
    let tab = cx.update(|_, cx| tabs.read(cx).tabs.active());
    tabs.update_in(cx, |tabs, window, cx| tabs.close_tab(tab, window, cx));
    cx.run_until_parked();
    show_page(&tabs, cx, &fixture.estate);
    click_note(cx, &note.tag);
    assert_eq!(
        document_shown(&tabs, cx).as_deref(),
        Some(document.as_str())
    );
    assert!(review_open(&tabs, cx), "it opens for review once loaded");
    crate::document_view::DocumentView::hold_scans(false);
}

/// Done when: a request for a document that needs review notifies by its
/// AI label, once until it is opened, and its click opens the review;
/// nothing opens by itself.
#[gpui::test]
fn a_review_request_notifies_once_and_opens_the_review(cx: &mut TestAppContext) {
    crate::document_view::DocumentView::hold_scans(true);
    let fixture = fixture();
    let (tabs, cx, notes) = shared_lease(cx, &fixture);
    cx.update(|_, cx| {
        let vault = tabs.read(cx).opened_vault().unwrap();
        vault
            .update_document(&fixture.lease_doc, |d| d.set_ready(false))
            .unwrap();
    });
    let read = Call::GetDocument {
        doc_id: fixture.lease_doc.clone(),
        part: None,
    };
    let Answer::Ok(Done::NeedsReview { label, .. }) = call(&tabs, cx, read.clone()) else {
        panic!("needs review");
    };
    assert_eq!(
        *notes.borrow(),
        [Note {
            tag: format!("ai-review-{}", fixture.lease_doc),
            title: format!("Claude asked for \u{201c}{label}\u{201d}"),
            body: "It needs review before AI apps can read it. Open it in mdoc to review it."
                .into(),
        }]
    );
    assert_private(&notes.borrow());
    // The AI retrying raises no second one; nothing opened by itself.
    call(&tabs, cx, read.clone());
    assert_eq!(notes.borrow().len(), 1);
    assert!(!window_active(cx));
    assert_eq!(document_shown(&tabs, cx), None);
    assert!(cx.debug_bounds("ai-badge").is_some());
    // The click opens the document for review, in front.
    let tag = notes.borrow()[0].tag.clone();
    click_note(cx, &tag);
    assert!(window_active(cx));
    assert_eq!(
        document_shown(&tabs, cx).as_deref(),
        Some(fixture.lease_doc.as_str())
    );
    assert!(review_open(&tabs, cx), "it opens for review");
    assert!(cx.debug_bounds("ai-badge").is_none());
    // Opened, it may ask again.
    call(&tabs, cx, read.clone());
    assert_eq!(notes.borrow().len(), 2);
    assert!(cx.debug_bounds("ai-badge").is_some());
    // Stop sharing: the request is moot and the badge goes.
    click_toolbar(cx, "ai-stop-sharing");
    draw(cx);
    assert!(cx.debug_bounds("ai-badge").is_none());
    // A request answered before Stop sharing and heard after it raises
    // nothing.
    tabs.update(cx, |tabs, cx| {
        tabs.share_with_ai(&fixture.lease.clone(), cx)
    });
    let answer = cx.update(|_, cx| {
        let running = tabs.read(cx).ai_connector.as_ref().unwrap();
        running.call(&Request {
            v: wire::VERSION,
            app: "claude-ai".into(),
            call: read,
        })
    });
    assert!(matches!(answer, Answer::Ok(Done::NeedsReview { .. })));
    tabs.update(cx, |tabs, cx| tabs.stop_sharing_with_ai(cx));
    wait(cx, 300);
    assert_eq!(notes.borrow().len(), 2);
    assert!(cx.debug_bounds("ai-badge").is_none());
    crate::document_view::DocumentView::hold_scans(false);
}

/// Done when: with "Open deliveries when they arrive" on, a delivery opens
/// where it lands and brings the window forward; a review request still
/// opens nothing.
#[gpui::test]
fn the_setting_opens_deliveries_in_front(cx: &mut TestAppContext) {
    let fixture = fixture();
    let (tabs, cx, notes) = shared_lease(cx, &fixture);
    tabs.update(cx, |tabs, _| {
        (tabs.preferences.borrow_mut().current.as_mut().unwrap())
            .ai_connector
            .open_deliveries = true;
    });
    // Settings up: the delivery waits on the badge, and nothing moves.
    tabs.update_in(cx, |tabs, window, cx| tabs.show_settings(window, cx));
    cx.deactivate_window();
    draw(cx);
    let (waiting, _) = deliver(&tabs, cx, &fixture, Kind::Reply);
    assert_eq!(reply_shown(&tabs, cx), None);
    assert!(!window_active(cx));
    // Its notification's click brings the window forward to Settings,
    // which stays.
    click_note(cx, &format!("ai-reply-{waiting}"));
    assert!(window_active(cx));
    assert!(cx.update(|_, cx| tabs.read(cx).settings.read(cx).open));
    assert_eq!(reply_shown(&tabs, cx), None);
    tabs.update_in(cx, |tabs, window, cx| {
        tabs.settings
            .update(cx, |panel, cx| panel.close(window, cx))
    });
    cx.deactivate_window();
    draw(cx);
    notes.borrow_mut().clear();
    let (reply, _) = deliver(&tabs, cx, &fixture, Kind::Reply);
    assert!(window_active(cx));
    assert_eq!(reply_shown(&tabs, cx).as_deref(), Some(reply.as_str()));
    // Opened as it landed, it isn't counted; the one that waited still is.
    let unseen = cx.update(|_, cx| tabs.read(cx).ai.unseen().len());
    assert_eq!(unseen, 1);
    assert_eq!(notes.borrow().len(), 1);
    let (_, document) = deliver(&tabs, cx, &fixture, Kind::Document);
    assert_eq!(document_shown(&tabs, cx), document);
    // A review request is never opened for the user.
    cx.deactivate_window();
    cx.update(|_, cx| {
        let vault = tabs.read(cx).opened_vault().unwrap();
        vault
            .update_document(&fixture.lease_doc, |d| d.set_ready(false))
            .unwrap();
    });
    show_page(&tabs, cx, &fixture.lease);
    let read = Call::GetDocument {
        doc_id: fixture.lease_doc.clone(),
        part: None,
    };
    assert!(matches!(
        call(&tabs, cx, read),
        Answer::Ok(Done::NeedsReview { .. })
    ));
    assert_eq!(notes.borrow().len(), 3);
    assert!(!window_active(cx));
    assert!(cx.update(|_, cx| tabs.read(cx).project_ui.page.is_some()));
}

/// Done when: Settings › AI connector's "Open deliveries when they
/// arrive" is what opens a delivery in front: off, as stored, it waits on
/// the badge; switched on, the next opens; switched off again, the next
/// waits.
#[gpui::test]
fn the_settings_switch_opens_deliveries_only_while_on(cx: &mut TestAppContext) {
    let fixture = fixture();
    let (tabs, cx, _) = shared_lease(cx, &fixture);
    // Tall enough that the whole page is in view.
    cx.simulate_resize(size(px(1100.), px(2400.)));
    let flip = |cx: &mut VisualTestContext| {
        tabs.update_in(cx, |tabs, window, cx| {
            tabs.settings.update(cx, |panel, cx| {
                panel.show_category(settings_ui::Category::AiConnector, window, cx)
            })
        });
        draw(cx);
        let bounds = cx
            .debug_bounds("ai-open-deliveries")
            .expect("the switch shows with the preview on");
        cx.simulate_click(bounds.center(), gpui::Modifiers::none());
        cx.run_until_parked();
        tabs.update_in(cx, |tabs, window, cx| {
            tabs.settings
                .update(cx, |panel, cx| panel.close(window, cx))
        });
        cx.deactivate_window();
        draw(cx);
        cx.update(|_, cx| tabs.read(cx).ai_setting().open_deliveries)
    };
    let (waiting, _) = deliver(&tabs, cx, &fixture, Kind::Reply);
    assert_eq!(reply_shown(&tabs, cx), None);
    assert!(!window_active(cx));
    assert!(cx.debug_bounds("ai-badge").is_some());
    assert!(flip(cx), "the switch saved on");
    let (reply, _) = deliver(&tabs, cx, &fixture, Kind::Reply);
    assert!(window_active(cx));
    assert_eq!(reply_shown(&tabs, cx).as_deref(), Some(reply.as_str()));
    assert_ne!(reply, waiting);
    assert!(!flip(cx), "the switch saved off");
    let (later, _) = deliver(&tabs, cx, &fixture, Kind::Reply);
    assert_ne!(later, reply);
    assert!(!window_active(cx));
    assert_eq!(reply_shown(&tabs, cx).as_deref(), Some(reply.as_str()));
}

/// Done when: with the preview off no notification is raised and the
/// badge is gone; the injected notifier takes no app identity.
#[gpui::test]
fn with_the_preview_off_nothing_notifies(cx: &mut TestAppContext) {
    let fixture = fixture();
    let (tabs, cx) = boot_fixture(cx, &fixture);
    let notes = record_notes(&tabs, cx);
    expand_sidebar(&tabs, cx);
    set_preview(&tabs, cx, true);
    tabs.update(cx, |tabs, cx| {
        tabs.share_with_ai(&fixture.lease.clone(), cx)
    });
    let (reply, _) = deliver(&tabs, cx, &fixture, Kind::Reply);
    assert_eq!(notes.borrow().len(), 1);
    assert!(cx.debug_bounds("ai-badge").is_some());
    set_preview(&tabs, cx, false);
    assert!(cx.debug_bounds("ai-badge").is_none());
    assert!(cx.debug_bounds("ai-project").is_none());
    assert_eq!(notes.borrow().len(), 1);
    // Its notification, left behind, opens nothing now.
    click_note(cx, &format!("ai-reply-{reply}"));
    assert_eq!(reply_shown(&tabs, cx), None);
    assert_eq!(cx.app_identity(), None);
}
