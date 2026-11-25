//! The companion window (Milestone 2 item 14), macOS: with "AI connector
//! (preview)" off the menu bar item keeps its shelf popup; on, it toggles
//! the companion, which opens by itself only for a delivery or a review
//! request and never takes focus. Its Stop sharing, its reply's Copy and
//! Save as document, its drop target and its shelf rows run through the
//! window as their other doors do. An AI app's calls go through the
//! handler the connector's listener runs.
#![cfg(target_os = "macos")]

use super::ai_project::{
    Fixture, boot_fixture, call, deliver, document_shown, fixture, review_open, shared,
    shared_lease, window_active,
};
use super::*;
use crate::{companion::CompanionView, menu_bar, shelf_ui::ShelfView};
use mdoc_mcp::wire::{Answer, Call, Done, Kind};
use mdoc_pii::{Category::Person, restore::ExportedIdentity};
use mdoc_store::document_id::VaultId as VaultIdOf;

/// The menu bar item on, as at launch.
fn install_menu_bar(tabs: &Entity<Workspace>, cx: &mut VisualTestContext) {
    let handle = cx.window_handle().downcast::<Workspace>().unwrap();
    let preview = cx.update(|_, cx| tabs.read(cx).ai_preview_setting());
    cx.cx.update(|cx| {
        menu_bar::install(handle, true, true, cx);
        menu_bar::set_companion(preview, cx);
    });
}

/// "AI connector (preview)" on or off, as Settings applying it does, in
/// the workspace's own window: a popup that read the workspace last would
/// otherwise host the update.
fn preview(tabs: &Entity<Workspace>, cx: &mut VisualTestContext, on: bool) {
    cx.update(|window, cx| {
        tabs.update(cx, |tabs, cx| {
            (tabs.preferences.borrow_mut().current.as_mut().unwrap())
                .ai_connector
                .preview = on;
            tabs.refresh_ai_connector(window, cx);
        })
    });
    cx.run_until_parked();
}

fn companion(cx: &mut VisualTestContext) -> Option<gpui::WindowHandle<CompanionView>> {
    cx.run_until_parked();
    let open = cx.update(|_, cx| menu_bar::companion(cx));
    let windows = cx.update(|_, cx| {
        (cx.windows().iter())
            .filter(|window| window.downcast::<CompanionView>().is_some())
            .count()
    });
    assert_eq!(
        windows,
        usize::from(open.is_some()),
        "one companion at most"
    );
    open
}

fn shelf_open(cx: &mut VisualTestContext) -> bool {
    cx.update(|_, cx| (cx.windows().iter()).any(|window| window.downcast::<ShelfView>().is_some()))
}

fn click_item(cx: &mut VisualTestContext) {
    cx.cx.update(menu_bar::item_clicked);
    cx.run_until_parked();
}

fn active_window(cx: &mut VisualTestContext) -> Option<gpui::AnyWindowHandle> {
    cx.update(|_, cx| cx.active_window())
}

/// "Anna Petrova" is PERSON_1 in "Lease", before the window opens, so its
/// reply restores and its document is a shelf row.
fn name_anna(fixture: &Fixture) {
    let vault = mdoc_vault::Vault::open(fixture.dir.path().join("vault"), &[3; 32]).unwrap();
    vault
        .update_project(&fixture.lease, |project| {
            project.mapping.merge(&[ExportedIdentity {
                alias: "PERSON_1".into(),
                previous: vec![],
                category: Person,
                custom: false,
                originals: vec!["Anna Petrova".into()],
            }])
        })
        .unwrap();
    vault.flush().unwrap();
}

fn view<R>(
    companion: gpui::WindowHandle<CompanionView>,
    cx: &mut VisualTestContext,
    f: impl FnOnce(&CompanionView, &gpui::App) -> R,
) -> R {
    companion.read_with(cx, |view, cx| f(view, cx)).unwrap()
}

fn update<R>(
    companion: gpui::WindowHandle<CompanionView>,
    cx: &mut VisualTestContext,
    f: impl FnOnce(&mut CompanionView, &mut Window, &mut gpui::Context<CompanionView>) -> R,
) -> R {
    let result = companion.update(cx, f).unwrap();
    cx.run_until_parked();
    result
}

/// Done when: with the preview off the item opens the shelf popup as
/// before; on, it toggles the companion in its place, and off again it is
/// the shelf's once more.
#[gpui::test]
fn the_preview_decides_between_the_shelf_and_the_companion(cx: &mut TestAppContext) {
    let fixture = fixture();
    let (tabs, cx) = boot_fixture(cx, &fixture);
    install_menu_bar(&tabs, cx);
    click_item(cx);
    assert!(shelf_open(cx));
    assert!(companion(cx).is_none());
    // On with the shelf open: the shelf goes; the item toggles the
    // companion.
    preview(&tabs, cx, true);
    assert!(!shelf_open(cx));
    click_item(cx);
    assert!(companion(cx).is_some());
    assert!(!shelf_open(cx), "no shelf popup beside it");
    click_item(cx);
    assert!(companion(cx).is_none());
    click_item(cx);
    assert!(companion(cx).is_some());
    // Off with it open: it goes, and the item opens the shelf again.
    preview(&tabs, cx, false);
    assert!(companion(cx).is_none());
    click_item(cx);
    assert!(shelf_open(cx));
    assert!(companion(cx).is_none());
}

/// Done when: the companion's window never asks for focus: a popup kind
/// (a non-activating panel on macOS) opened without focus.
#[test]
fn the_companion_window_never_asks_for_focus() {
    let options =
        crate::companion::window_options(gpui::Bounds::default(), gpui::DisplayId::new(1));
    assert!(!options.focus);
    assert_eq!(options.kind, gpui::WindowKind::PopUp);
    assert!(!options.is_movable);
}

/// Done when: the companion opens by itself for a delivery and for a
/// review request, without taking focus from anything, and not for a
/// read, an ask, or a review request repeated while it is quiet.
#[gpui::test]
fn it_opens_by_itself_only_for_a_delivery_or_a_review_request(cx: &mut TestAppContext) {
    crate::document_view::DocumentView::hold_scans(true);
    let fixture = fixture();
    let (tabs, cx, _notes) = shared_lease(cx, &fixture);
    install_menu_bar(&tabs, cx);
    let before = active_window(cx);
    // A list and a read open nothing.
    call(&tabs, cx, Call::ListDocuments);
    let read = Call::GetDocument {
        doc_id: fixture.lease_doc.clone(),
        part: None,
    };
    call(&tabs, cx, read.clone());
    assert!(companion(cx).is_none());
    // A delivery opens it, in the background.
    let (reply, _) = deliver(&tabs, cx, &fixture, Kind::Reply);
    let open = companion(cx).expect("a delivery opens it");
    assert_eq!(active_window(cx), before, "nothing took focus");
    assert!(!window_active(cx));
    assert!(
        !open
            .update(cx, |_, window, _| window.is_window_active())
            .unwrap()
    );
    assert_eq!(
        view(open, cx, |view, _| view
            .content()
            .reply
            .clone()
            .map(|r| r.id)),
        Some(reply)
    );
    // Hidden, a read leaves it closed; a review request opens it.
    update(open, cx, |view, window, cx| view.hide(window, cx));
    assert!(companion(cx).is_none());
    call(&tabs, cx, read.clone());
    assert!(companion(cx).is_none());
    vault(&tabs, cx)
        .update_document(&fixture.lease_doc, |d| d.set_ready(false))
        .unwrap();
    let Answer::Ok(Done::NeedsReview { label, .. }) = call(&tabs, cx, read.clone()) else {
        panic!("needs review");
    };
    let open = companion(cx).expect("a review request opens it");
    assert_eq!(active_window(cx), before, "nothing took focus");
    let review = view(open, cx, |view, _| view.content().review.clone()).unwrap();
    assert_eq!(review.label, label);
    assert_eq!(review.from, "Claude");
    assert_eq!(review.document, fixture.lease_doc);
    // The AI retrying while it is quiet does not open it again.
    update(open, cx, |view, window, cx| view.hide(window, cx));
    call(&tabs, cx, read.clone());
    assert!(companion(cx).is_none());
    // Its Review in mdoc opens the document for review, in front.
    click_item(cx);
    let open = companion(cx).unwrap();
    update(open, cx, |view, _, cx| view.review(cx));
    assert!(window_active(cx));
    assert_eq!(
        document_shown(&tabs, cx).as_deref(),
        Some(fixture.lease_doc.as_str())
    );
    assert!(review_open(&tabs, cx));
    assert_eq!(
        view(open, cx, |view, _| view.content().review.clone()),
        None
    );
    crate::document_view::DocumentView::hold_scans(false);
}

/// Done when: the companion always names what is shared, and its Stop
/// sharing stops it.
#[gpui::test]
fn its_stop_sharing_stops_the_share(cx: &mut TestAppContext) {
    let fixture = fixture();
    let (tabs, cx, _notes) = shared_lease(cx, &fixture);
    install_menu_bar(&tabs, cx);
    click_item(cx);
    let open = companion(cx).unwrap();
    let project = view(open, cx, |view, _| view.content().shared.clone()).unwrap();
    assert_eq!(project.id, fixture.lease);
    assert_eq!(project.name, "Lease");
    let documents = view(open, cx, |view, _| view.content().documents.clone()).unwrap();
    assert_eq!(documents.len(), 1);
    assert_eq!(documents[0].id, fixture.lease_doc);
    assert!(documents[0].ready);
    assert_eq!(documents[0].state, "Ready");
    // Listed by AI label, never by the stored title.
    assert_ne!(documents[0].label, "Lease memo");
    update(open, cx, |view, _, cx| view.stop_sharing(cx));
    assert_eq!(shared(&tabs, cx), None);
    assert_eq!(
        view(open, cx, |view, _| view.content().shared.clone()),
        None
    );
    let documents = view(open, cx, |view, _| view.content().documents.clone()).unwrap();
    assert!(documents.is_empty());
}

/// Done when: the latest reply shows only while the pointer is over it;
/// its Copy puts the restored text on the clipboard marked as restored,
/// and its Save as document makes one Document of it, once.
#[gpui::test]
fn the_restored_reply_copies_and_saves(cx: &mut TestAppContext) {
    let fixture = fixture();
    name_anna(&fixture);
    let (tabs, cx, _notes) = shared_lease(cx, &fixture);
    install_menu_bar(&tabs, cx);
    let (reply, _) = deliver(&tabs, cx, &fixture, Kind::Reply);
    let open = companion(cx).unwrap();
    let shown = view(open, cx, |view, _| view.content().reply.clone()).unwrap();
    assert_eq!(shown.id, reply);
    assert_eq!(shown.from, "Claude");
    assert_eq!(shown.restored, "Anna Petrova pays on time.");
    // Hidden until the pointer is over it.
    let mut window = VisualTestContext::from_window(open.into(), &cx.cx);
    window.run_until_parked();
    assert!(!view(open, &mut window, |view, _| view.reply_shown()));
    let bounds = window.debug_bounds("companion-reply").expect("the reply");
    window.simulate_mouse_move(bounds.center(), None, gpui::Modifiers::none());
    window.run_until_parked();
    assert!(view(open, &mut window, |view, _| view.reply_shown()));
    window.simulate_mouse_move(gpui::point(px(1.), px(1.)), None, gpui::Modifiers::none());
    window.run_until_parked();
    assert!(!view(open, &mut window, |view, _| view.reply_shown()));
    // Review of #1250: leaving the window straight from the reply hides
    // it, and a redraw at the last place inside does not show it again.
    window.simulate_mouse_move(bounds.center(), None, gpui::Modifiers::none());
    window.run_until_parked();
    assert!(view(open, &mut window, |view, _| view.reply_shown()));
    window.simulate_event(gpui::MouseExitEvent {
        position: bounds.center(),
        pressed_button: None,
        modifiers: gpui::Modifiers::none(),
    });
    window.run_until_parked();
    window.update(|window, _| window.refresh());
    window.run_until_parked();
    assert!(!view(open, &mut window, |view, _| view.reply_shown()));
    window.simulate_mouse_move(bounds.center(), None, gpui::Modifiers::none());
    window.run_until_parked();
    assert!(view(open, &mut window, |view, _| view.reply_shown()));
    // Copy, hidden or not: restored, marked as a restored reply's copy.
    update(open, cx, |view, window, cx| view.copy_reply(window, cx));
    let copied = cx.update(|_, cx| cx.read_from_clipboard()).unwrap();
    assert_eq!(copied.text().as_deref(), Some("Anna Petrova pays on time."));
    assert!(crate::restored_clipboard::is_restored(&copied));
    // Save as document: one new Document in Lease, restored.
    let members = |cx: &mut VisualTestContext| vault(&tabs, cx).documents_in(Some(&fixture.lease));
    let before = members(cx).len();
    update(open, cx, |view, _, cx| view.save_reply(cx));
    wait(cx, 800);
    let after = members(cx);
    assert_eq!(after.len(), before + 1);
    let made = (after.iter())
        .find(|summary| summary.id != fixture.lease_doc)
        .unwrap();
    let made = vault(&tabs, cx).document(&made.id).unwrap();
    assert_eq!(made.from_reply.as_deref(), Some(reply.as_str()));
    assert!(made.markdown.contains("Anna Petrova pays on time."));
    // Once: a second click makes nothing more.
    assert!(
        view(open, cx, |view, _| view.content().reply.clone())
            .unwrap()
            .saved
    );
    update(open, cx, |view, _, cx| view.save_reply(cx));
    wait(cx, 800);
    assert_eq!(members(cx).len(), before + 1);
}

/// Review of #1250: a reply delivered as a Document is saved already, and
/// a Save as document that fails says so and may run again.
#[gpui::test]
fn save_as_document_counts_only_what_was_saved(cx: &mut TestAppContext) {
    let fixture = fixture();
    name_anna(&fixture);
    let (tabs, cx, _notes) = shared_lease(cx, &fixture);
    install_menu_bar(&tabs, cx);
    let members = |cx: &mut VisualTestContext| vault(&tabs, cx).documents_in(Some(&fixture.lease));
    let (_, document) = deliver(&tabs, cx, &fixture, Kind::Document);
    assert!(document.is_some());
    let open = companion(cx).unwrap();
    let reply = view(open, cx, |view, _| view.content().reply.clone()).unwrap();
    assert!(reply.saved, "delivered as a Document");
    let before = members(cx).len();
    update(open, cx, |view, _, cx| view.save_reply(cx));
    wait(cx, 800);
    assert_eq!(members(cx).len(), before, "no second Document");
    // A reply whose promotion fails (deleted meanwhile): not saved, and
    // not marked so.
    let (reply, _) = deliver(&tabs, cx, &fixture, Kind::Reply);
    vault(&tabs, cx).delete_reply(&reply).unwrap();
    cx.update(|window, cx| {
        tabs.update(cx, |tabs, cx| tabs.companion_save_reply(&reply, window, cx))
    });
    wait(cx, 800);
    let latest = cx.update(|_, cx| tabs.read(cx).ai.latest_reply().cloned());
    let latest = latest.unwrap();
    assert!(
        latest.failed && !latest.saved && !latest.saving,
        "{latest:?}"
    );
}

/// Review of #1250: opening the requested document from mdoc's sidebar
/// takes the review card off the companion at once.
#[gpui::test]
fn the_review_card_goes_once_the_document_opens(cx: &mut TestAppContext) {
    crate::document_view::DocumentView::hold_scans(true);
    let fixture = fixture();
    let (tabs, cx, _notes) = shared_lease(cx, &fixture);
    install_menu_bar(&tabs, cx);
    vault(&tabs, cx)
        .update_document(&fixture.lease_doc, |d| d.set_ready(false))
        .unwrap();
    call(
        &tabs,
        cx,
        Call::GetDocument {
            doc_id: fixture.lease_doc.clone(),
            part: None,
        },
    );
    let open = companion(cx).unwrap();
    assert!(view(open, cx, |view, _| view.content().review.clone()).is_some());
    let id = VaultIdOf::parse(&fixture.lease_doc).unwrap();
    cx.update(|window, cx| tabs.update(cx, |tabs, cx| tabs.open_vault_document(id, window, cx)));
    cx.run_until_parked();
    assert_eq!(
        view(open, cx, |view, _| view.content().review.clone()),
        None
    );
    crate::document_view::DocumentView::hold_scans(false);
}

/// Done when: files dropped on the companion are imported into the AI
/// project, and a shelf row's own drag dropped back on it imports nothing.
#[gpui::test]
fn its_drop_target_imports_into_the_ai_project(cx: &mut TestAppContext) {
    let fixture = fixture();
    name_anna(&fixture);
    let (tabs, cx, _notes) = shared_lease(cx, &fixture);
    install_menu_bar(&tabs, cx);
    click_item(cx);
    let open = companion(cx).unwrap();
    let other = tempfile::tempdir().unwrap();
    let note = other.path().join("note.md");
    std::fs::write(&note, "A note").unwrap();
    let paths = gpui::ExternalPaths(vec![note.clone()].into());
    // A row's drag coming back is no drop.
    let rows = view(open, cx, |view, cx| {
        view.shelf_rows().read(cx).documents().to_vec()
    });
    let row = (rows.iter())
        .position(|row| row.readiness.is_ready())
        .expect("Lease's memo is a ready row");
    update(open, cx, |view, window, cx| {
        view.shelf_rows()
            .update(cx, |rows, cx| rows.drag(row, window, cx));
        view.dropped(&paths, cx);
    });
    wait(cx, 300);
    let names = |cx: &mut VisualTestContext| tab_names(&tabs, cx);
    assert!(!names(cx).contains(&"note".to_string()));
    update(open, cx, |view, _, cx| {
        view.shelf_rows()
            .update(cx, |rows, cx| rows.drag_ended(true, cx))
    });
    // A drop from another app: into Lease, with mdoc in front.
    update(open, cx, |view, _, cx| view.dropped(&paths, cx));
    wait(cx, 300);
    let ids: Vec<_> = cx.update(|_, cx| tabs.read(cx).tabs.iter().map(|t| t.id()).collect());
    for id in ids {
        tabs.update_in(cx, |tabs, window, cx| tabs.activate(id, window, cx));
        cx.run_until_parked();
    }
    assert!(names(cx).contains(&"note".to_string()));
    let projects = cx.update(|_, cx| {
        let w = tabs.read(cx);
        (w.tabs.iter())
            .filter(|tab| {
                tab.view()
                    .is_some_and(|v| v.read(cx).display_name() == "note")
            })
            .map(|tab| w.sidebar_project_link(tab, cx).map(|link| link.id))
            .collect::<Vec<_>>()
    });
    assert_eq!(projects, [Some(fixture.lease.clone())]);
    assert!(window_active(cx));
}

/// Done when: the companion's shelf rows copy and drag through the
/// shelf's checked paths: a row that is not Ready hands nothing out.
#[gpui::test]
fn its_shelf_rows_copy_and_drag_only_what_is_ready(cx: &mut TestAppContext) {
    let fixture = fixture();
    name_anna(&fixture);
    let (tabs, cx, _notes) = shared_lease(cx, &fixture);
    install_menu_bar(&tabs, cx);
    click_item(cx);
    let open = companion(cx).unwrap();
    let row = |cx: &mut VisualTestContext| {
        view(open, cx, |view, cx| {
            let rows = view.shelf_rows().read(cx).documents().to_vec();
            (rows.iter()).position(|row| {
                row.key
                    == crate::shelf::ShelfKey::Document(
                        mdoc_store::document_id::VaultId::parse(&fixture.lease_doc).unwrap(),
                    )
            })
        })
        .expect("Lease's memo is a row")
    };
    let index = row(cx);
    let before = gpui::ClipboardItem::new_string("before".into());
    cx.write_to_clipboard(before.clone());
    update(open, cx, |view, window, cx| {
        view.shelf_rows()
            .update(cx, |rows, cx| rows.copy(index, window, cx))
    });
    let copied = cx.update(|_, cx| cx.read_from_clipboard()).unwrap();
    let copied = copied.text().unwrap();
    assert!(copied.ends_with("\n\nPERSON_1 rents."), "{copied}");
    // Not Ready since: the copy and the drag check again and refuse.
    vault(&tabs, cx)
        .update_document(&fixture.lease_doc, |d| {
            d.markdown.push_str(" Anna Petrova pays.")
        })
        .unwrap();
    cx.write_to_clipboard(before);
    let dragged = update(open, cx, |view, window, cx| {
        view.shelf_rows().update(cx, |rows, cx| {
            rows.copy(index, window, cx);
            rows.drag(index, window, cx);
            rows.dragged.take()
        })
    });
    assert_eq!(dragged, None);
    let clipboard = cx.update(|_, cx| cx.read_from_clipboard()).unwrap();
    assert_eq!(clipboard.text().as_deref(), Some("before"));
    assert_eq!(
        view(open, cx, |view, cx| view.shelf_rows().read(cx).refused()),
        Some(index)
    );
}
