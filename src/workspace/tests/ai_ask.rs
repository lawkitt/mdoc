//! Ask (Milestone 2 item 10): the document header's and the Project
//! page's Ask Claude, Ask in a ChatGPT thread and Copy for AI, only with "AI
//! connector (preview)" on. Ask shares the Project as part of the click
//! and opens the app with a prompt naming the document by its AI label;
//! Unfiled moves to a new Project first. No app launches: GPUI's test
//! platform only records the link it was asked to open.
use super::ai_project::{
    Fixture, boot_fixture, call, fixture, open_member, record_notes, set_preview, shared, show_page,
};
use super::*;
use crate::ai_ask;
use mdoc_mcp::wire::{Answer, Call, Done, Kind};
use mdoc_store::settings::AskWith;

/// The fixture, plus in "Lease" a Ready document whose title holds a real
/// name and whose heading doesn't, and an Unfiled Ready document: their
/// ids.
struct More {
    headed: String,
    unfiled: String,
}

fn more_documents(fixture: &Fixture) -> More {
    let vault = mdoc_vault::Vault::open(fixture.dir.path().join("vault"), &[3; 32]).unwrap();
    let mut made = Vec::new();
    for (project, title, text) in [
        (
            Some(fixture.lease.clone()),
            "Maria Lopez lease",
            "# Lease of PERSON_1's flat\n\nPERSON_1 rents.",
        ),
        (None, "Loose note", "PERSON_1 called."),
    ] {
        let document = vault
            .create_document(mdoc_vault::NewDocument {
                project,
                title: title.into(),
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
        made.push(document.id);
    }
    vault.flush().unwrap();
    let [headed, unfiled] = <[_; 2]>::try_from(made).unwrap();
    More { headed, unfiled }
}

/// Whether `id` is drawn now.
fn shown(cx: &mut VisualTestContext, id: &'static str) -> bool {
    draw(cx);
    cx.debug_bounds(id).is_some()
}

/// Close the notices' popover, which a notice opens over the header.
fn close_notices(tabs: &Entity<Workspace>, cx: &mut VisualTestContext) {
    tabs.update(cx, |tabs, cx| {
        tabs.title.notices_open.set(false);
        cx.notify();
    });
    draw(cx);
}

fn opened(cx: &mut VisualTestContext) -> Option<String> {
    cx.opened_url()
}

fn last(tabs: &Entity<Workspace>, cx: &mut VisualTestContext) -> AskWith {
    cx.update(|_, cx| tabs.read(cx).ask_last())
}

fn notice(tabs: &Entity<Workspace>, cx: &mut VisualTestContext) -> Option<String> {
    cx.update(|_, cx| tabs.read(cx).notices.message().map(str::to_string))
}

/// The link opened last is `with`'s app with the document prompt for
/// `doc_id`, under whatever label the connector gave it at the click (an
/// entry scan may change it since).
fn asked_about(cx: &mut VisualTestContext, with: AskWith, doc_id: &str) {
    let url = opened(cx).expect("the app opens");
    let start = ai_ask::link(with, "Read \u{201c}").unwrap();
    let end = crate::ai_links::percent_encoded(&format!(
        "\u{201d} in mdoc (doc_id {doc_id}). My question: "
    ));
    assert!(url.starts_with(&start) && url.ends_with(&end), "{url}");
}

/// Done when: Ask Claude on a document shares its Project, in place of
/// the one shared before, and opens Claude with a prompt naming the
/// document by its AI label and id, never its title or text.
#[gpui::test]
fn ask_claude_shares_the_documents_project_and_names_it_by_label(cx: &mut TestAppContext) {
    let fixture = fixture();
    let more = more_documents(&fixture);
    let (tabs, cx) = boot_fixture(cx, &fixture);
    set_preview(&tabs, cx, true);
    tabs.update(cx, |tabs, cx| {
        tabs.share_with_ai(&fixture.estate.clone(), cx)
    });
    open_member(&tabs, cx, &more.headed);
    assert_eq!(last(&tabs, cx), AskWith::Claude);
    click_toolbar(cx, "header-ask");
    assert_eq!(shared(&tabs, cx), Some(fixture.lease.clone()));
    // Ready at the click: its reviewed heading names it.
    let prompt = format!(
        "Read “Lease of PERSON_1's flat” in mdoc (doc_id {}). My question: ",
        more.headed
    );
    let url = opened(cx).expect("Claude opens");
    assert_eq!(url, crate::ai_links::claude(&prompt));
    for private in ["Maria", "Lopez", "rents"] {
        assert!(!url.contains(private), "{url}");
    }
    assert_eq!(
        notice(&tabs, cx).as_deref(),
        Some(
            "“Lease” is shared with AI apps. Claude opened a new chat with the prompt: add your \
             question and send it."
        )
    );
    // The default stays unwritten.
    let written = cx.update(|_, cx| {
        serde_json::to_value(tabs.read(cx).preferences.borrow().current.clone().unwrap()).unwrap()
    });
    assert!(written["ai_connector"].get("ask").is_none(), "{written}");
}

/// Done when: the menu's Ask in a ChatGPT thread opens ChatGPT with the
/// same prompt,
/// and the button and the Project page repeat it from then on.
#[gpui::test]
fn the_last_choice_is_remembered(cx: &mut TestAppContext) {
    let fixture = fixture();
    let (tabs, cx) = boot_fixture(cx, &fixture);
    set_preview(&tabs, cx, true);
    open_member(&tabs, cx, &fixture.lease_doc);
    click_toolbar(cx, "header-ask-more");
    assert!(shown(cx, "ask-menu"));
    click_toolbar(cx, "ask-chatgpt");
    assert!(!shown(cx, "ask-menu"));
    assert_eq!(last(&tabs, cx), AskWith::ChatGpt);
    asked_about(cx, AskWith::ChatGpt, &fixture.lease_doc);
    assert!(notice(&tabs, cx).unwrap().ends_with(
        "a new local Work thread with the prompt: add your question and send it. \
                        Its ordinary chats can't read mdoc."
    ));
    // The Settings draft keeps it, so applying Settings doesn't undo it.
    let draft = cx.update(|_, cx| tabs.read(cx).settings.read(cx).draft.ai_connector.ask);
    assert_eq!(draft, AskWith::ChatGpt);
    // The button now repeats ChatGPT, for another document too.
    open_member(&tabs, cx, &fixture.estate_doc);
    click_toolbar(cx, "header-ask");
    assert_eq!(shared(&tabs, cx), Some(fixture.estate.clone()));
    asked_about(cx, AskWith::ChatGpt, &fixture.estate_doc);
    // So does the Project page's button.
    show_page(&tabs, cx, &fixture.lease);
    click_toolbar(cx, "project-page-ask");
    assert_eq!(shared(&tabs, cx), Some(fixture.lease.clone()));
    assert_eq!(
        opened(cx),
        Some(crate::ai_links::chatgpt(ai_ask::PROJECT_PROMPT))
    );
}

/// Done when: Copy for AI from Ask copies the document as today's Copy
/// for AI does and shares nothing, Unfiled too; the Project page, which
/// can't copy, repeats Claude after it.
#[gpui::test]
fn copy_for_ai_from_ask_copies_and_shares_nothing(cx: &mut TestAppContext) {
    let fixture = fixture();
    let (tabs, cx) = boot_fixture(cx, &fixture);
    set_preview(&tabs, cx, true);
    let note = open_applied(
        &tabs,
        cx,
        fixture.dir.path(),
        "Anna Petrova note.md",
        "Anna Petrova called.",
        &[("Anna Petrova", mdoc_pii::Category::Person)],
    );
    wait(cx, 800);
    assert_eq!(cx.update(|_, cx| tabs.read(cx).tabs.active()), note);
    click_toolbar(cx, "header-ask-more");
    click_toolbar(cx, "ask-copy-for-ai");
    wait(cx, 800);
    assert!(!shown(cx, "ask-unfiled"), "Copy for AI needs no Project");
    assert_eq!(last(&tabs, cx), AskWith::CopyForAi);
    assert_eq!(shared(&tabs, cx), None);
    assert_eq!(opened(cx), None);
    let copied = cx.read_from_clipboard().and_then(|item| item.text());
    assert!(
        copied
            .as_deref()
            .is_some_and(|text| text.contains("PERSON_1 called.") && !text.contains("Anna")),
        "{copied:?}"
    );
    // The button repeats it.
    cx.write_to_clipboard(gpui::ClipboardItem::new_string(String::new()));
    click_toolbar(cx, "header-ask");
    wait(cx, 800);
    let copied = cx.read_from_clipboard().and_then(|item| item.text());
    assert!(
        copied
            .as_deref()
            .is_some_and(|text| text.contains("PERSON_1 called.")),
        "{copied:?}"
    );
    // The Project page offers the apps only, Claude first.
    show_page(&tabs, cx, &fixture.estate);
    click_toolbar(cx, "project-page-ask-more");
    assert!(shown(cx, "project-ask-menu"));
    assert!(shown(cx, "project-ask-claude"));
    assert!(shown(cx, "project-ask-chatgpt"));
    click_toolbar(cx, "project-ask-claude");
    assert!(!shown(cx, "project-ask-menu"));
    assert_eq!(shared(&tabs, cx), Some(fixture.estate.clone()));
    assert_eq!(
        opened(cx),
        Some(crate::ai_links::claude(ai_ask::PROJECT_PROMPT))
    );
    assert_eq!(last(&tabs, cx), AskWith::Claude);
}

/// Done when: Ask on an Unfiled document shares nothing and asks first;
/// "Move to a new Project and share" moves it, shares that Project and
/// opens the app, in one click.
#[gpui::test]
fn ask_on_unfiled_moves_to_a_new_project_and_shares_it(cx: &mut TestAppContext) {
    let fixture = fixture();
    let more = more_documents(&fixture);
    let (tabs, cx) = boot_fixture(cx, &fixture);
    set_preview(&tabs, cx, true);
    open_member(&tabs, cx, &more.unfiled);
    click_toolbar(cx, "header-ask");
    assert!(shown(cx, "ask-unfiled"));
    assert_eq!(shared(&tabs, cx), None);
    assert_eq!(opened(cx), None);
    // Cancel leaves it as it was.
    click_toolbar(cx, "ask-unfiled-cancel");
    assert!(!shown(cx, "ask-unfiled"));
    assert_eq!(shared(&tabs, cx), None);
    click_toolbar(cx, "header-ask-more");
    click_toolbar(cx, "ask-chatgpt");
    assert!(shown(cx, "ask-unfiled"));
    click_toolbar(cx, "ask-move-and-share");
    wait(cx, 300);
    let project = cx
        .update(|_, cx| tabs.read(cx).project_of(&more.unfiled))
        .expect("moved to a new Project");
    assert_ne!(project, fixture.lease);
    assert_ne!(project, fixture.estate);
    assert_eq!(shared(&tabs, cx), Some(project));
    asked_about(cx, AskWith::ChatGpt, &more.unfiled);
    assert_eq!(last(&tabs, cx), AskWith::ChatGpt);
}

/// Done when: a document excluded from AI is never shared by Ask.
#[gpui::test]
fn ask_on_an_excluded_document_shares_nothing(cx: &mut TestAppContext) {
    let fixture = fixture();
    let more = more_documents(&fixture);
    {
        let vault = mdoc_vault::Vault::open(fixture.dir.path().join("vault"), &[3; 32]).unwrap();
        for id in [&fixture.lease_doc, &more.unfiled] {
            vault
                .update_document(id, |d| d.exclude_from_ai = true)
                .unwrap();
        }
        vault.flush().unwrap();
    }
    let (tabs, cx) = boot_fixture(cx, &fixture);
    set_preview(&tabs, cx, true);
    open_member(&tabs, cx, &fixture.lease_doc);
    click_toolbar(cx, "header-ask");
    assert_eq!(shared(&tabs, cx), None);
    assert_eq!(opened(cx), None);
    assert_eq!(notice(&tabs, cx).as_deref(), Some(ai_ask::EXCLUDED));
    // Unfiled and excluded: no offer to move it either (review of #1237).
    tabs.update(cx, |tabs, _| tabs.notices.dismiss());
    open_member(&tabs, cx, &more.unfiled);
    click_toolbar(cx, "header-ask");
    assert!(!shown(cx, "ask-unfiled"));
    assert_eq!(notice(&tabs, cx).as_deref(), Some(ai_ask::EXCLUDED));
    assert_eq!(shared(&tabs, cx), None);
}

/// Done when: Ask on a document that needs review still asks, naming it
/// "Document N" by the connector's own rule; the app's read then raises
/// the review request's notification (Decision 4, item 9).
#[gpui::test]
fn ask_on_a_document_that_needs_review_leads_to_the_review_request(cx: &mut TestAppContext) {
    let fixture = fixture();
    {
        let vault = mdoc_vault::Vault::open(fixture.dir.path().join("vault"), &[3; 32]).unwrap();
        // Never scanned, so opening it doesn't make it Ready (item 39's
        // nothing-to-replace rule needs a scan).
        vault
            .update_document(&fixture.lease_doc, |d| {
                d.scanned = false;
                d.set_ready(false);
            })
            .unwrap();
        vault.flush().unwrap();
    }
    let (tabs, cx) = boot_fixture(cx, &fixture);
    let notes = record_notes(&tabs, cx);
    set_preview(&tabs, cx, true);
    open_member(&tabs, cx, &fixture.lease_doc);
    click_toolbar(cx, "header-ask");
    assert_eq!(shared(&tabs, cx), Some(fixture.lease.clone()));
    let prompt = format!(
        "Read “Document 1” in mdoc (doc_id {}). My question: ",
        fixture.lease_doc
    );
    assert_eq!(opened(cx), Some(crate::ai_links::claude(&prompt)));
    let read = call(
        &tabs,
        cx,
        Call::GetDocument {
            doc_id: fixture.lease_doc.clone(),
            part: None,
        },
    );
    assert!(
        matches!(read, Answer::Ok(Done::NeedsReview { .. })),
        "{read:?}"
    );
    let notes = notes.borrow();
    assert_eq!(notes.len(), 1, "{notes:?}");
    assert!(notes[0].tag.starts_with("ai-review-"), "{notes:?}");
}

/// Review of #1237: asking again in the Project already shared keeps what
/// each app read, so a reply under way still lands.
#[gpui::test]
fn asking_in_the_shared_project_keeps_a_reply_under_way(cx: &mut TestAppContext) {
    let fixture = fixture();
    let (tabs, cx) = boot_fixture(cx, &fixture);
    set_preview(&tabs, cx, true);
    // From the Project page: no open member scans on entry meanwhile.
    show_page(&tabs, cx, &fixture.lease);
    click_toolbar(cx, "project-page-ask");
    assert_eq!(shared(&tabs, cx), Some(fixture.lease.clone()));
    let read = call(
        &tabs,
        cx,
        Call::GetDocument {
            doc_id: fixture.lease_doc.clone(),
            part: None,
        },
    );
    assert!(matches!(read, Answer::Ok(Done::Text { .. })), "{read:?}");
    // Ask ChatGPT in the same Project, then Claude delivers.
    // The notice's popover sits over the button: closed first.
    close_notices(&tabs, cx);
    click_toolbar(cx, "project-page-ask-more");
    click_toolbar(cx, "project-ask-chatgpt");
    assert_eq!(
        opened(cx),
        Some(crate::ai_links::chatgpt(ai_ask::PROJECT_PROMPT))
    );
    let delivered = call(
        &tabs,
        cx,
        Call::Deliver {
            kind: Kind::Reply,
            title: None,
            read: vec![fixture.lease_doc.clone()],
            markdown: "PERSON_1 pays on time.".into(),
        },
    );
    assert!(
        matches!(delivered, Answer::Ok(Done::Delivered { .. })),
        "{delivered:?}"
    );
}

/// Review of #1237: Stop sharing, or a menu left over a page since left,
/// shares nothing later.
#[gpui::test]
fn a_later_stop_or_another_page_shares_nothing(cx: &mut TestAppContext) {
    let fixture = fixture();
    let more = more_documents(&fixture);
    let (tabs, cx) = boot_fixture(cx, &fixture);
    set_preview(&tabs, cx, true);
    // An Ask waiting for its move, then Stop sharing: the move lands and
    // shares nothing.
    open_member(&tabs, cx, &more.unfiled);
    let tab = cx.update(|_, cx| tabs.read(cx).tabs.active());
    tabs.update(cx, |tabs, cx| {
        tabs.ai_ask.after_move(&more.unfiled, AskWith::Claude);
        tabs.stop_sharing_with_ai(cx);
    });
    tabs.update_in(cx, |tabs, window, cx| {
        tabs.move_to_new_project(tab, window, cx)
    });
    wait(cx, 300);
    assert!(
        cx.update(|_, cx| tabs.read(cx).project_of(&more.unfiled))
            .is_some()
    );
    assert_eq!(shared(&tabs, cx), None);
    assert_eq!(opened(cx), None);
    // The Project page's menu goes with the page.
    tabs.update(cx, |tabs, cx| tabs.dismiss_toast(cx));
    show_page(&tabs, cx, &fixture.lease);
    close_notices(&tabs, cx);
    click_toolbar(cx, "project-page-ask-more");
    assert!(shown(cx, "project-ask-menu"));
    tabs.update_in(cx, |tabs, window, cx| tabs.show_home(window, cx));
    cx.run_until_parked();
    assert!(!shown(cx, "project-ask-menu"));
    tabs.update(cx, |tabs, cx| {
        tabs.ask_about_project(&fixture.lease.clone(), AskWith::Claude, cx)
    });
    assert_eq!(shared(&tabs, cx), None);
    assert_eq!(opened(cx), None);
}

/// Done when: with the setting off, Ask shows nowhere.
#[gpui::test]
fn with_the_preview_off_ask_shows_nowhere(cx: &mut TestAppContext) {
    let fixture = fixture();
    let (tabs, cx) = boot_fixture(cx, &fixture);
    open_member(&tabs, cx, &fixture.lease_doc);
    assert!(!shown(cx, "header-ask"));
    assert!(shown(cx, "header-more"));
    show_page(&tabs, cx, &fixture.lease);
    assert!(!shown(cx, "project-page-ask"));
    set_preview(&tabs, cx, true);
    assert!(shown(cx, "project-page-ask"));
    open_member(&tabs, cx, &fixture.lease_doc);
    assert!(shown(cx, "header-ask"));
}
