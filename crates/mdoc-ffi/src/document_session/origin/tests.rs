use super::*;
use crate::{
    document_session::{DocumentPhase, DocumentSession, MentionState, ReviewAction, Scope},
    test_support::Scratch,
    text::TextRange,
    vault::Vault,
};
use std::sync::Arc;

fn vault(scratch: &Scratch) -> Arc<Vault> {
    Vault::open(
        scratch.path().join("vault").display().to_string(),
        vec![3; 32],
    )
    .unwrap()
}

fn title(text: &str) -> Option<String> {
    first_line_title(text.into())
}

/// A new document as the apps make one: begun empty, then saved, as the
/// import's save does; its session and id.
fn new_document(vault: &Arc<Vault>, name: &str) -> (Arc<DocumentSession>, String) {
    let session = DocumentSession::new();
    session
        .import_text_as(String::new(), name.into(), DocumentOrigin::New)
        .unwrap();
    let id = session.save_into_vault(vault.clone()).unwrap().id;
    (session, id)
}

fn end(session: &DocumentSession) -> TextRange {
    let len = session.snapshot().unwrap().text.len() as u64;
    TextRange {
        start: len,
        end: len,
    }
}

fn typed(session: &DocumentSession, text: &str) {
    session.type_text(end(session), text.into()).unwrap();
}

#[test]
fn the_first_line_names_the_document_as_it_reads() {
    assert_eq!(title(""), None);
    assert_eq!(title("\n  \n\t\n"), None);
    assert_eq!(
        title("# Lease agreement\nBody"),
        Some("Lease agreement".into())
    );
    assert_eq!(title("## **Bold** _title_"), Some("Bold title".into()));
    assert_eq!(title("\n\nLater line"), Some("Later line".into()));
    assert_eq!(
        title("> [!NOTE]\n> A *quoted* note"),
        Some("A quoted note".into())
    );
    assert_eq!(title("- [ ] Buy milk"), Some("Buy milk".into()));
    assert_eq!(title("  1. First step"), Some("First step".into()));
    assert_eq!(title("---\nAfter a rule"), Some("After a rule".into()));
    assert_eq!(title("```\ncode line\n```"), Some("code line".into()));
    assert_eq!(
        title("[Link text](https://example.com) here"),
        Some("Link text here".into())
    );
    assert_eq!(
        title("Many   spaces\there "),
        Some("Many spaces here".into())
    );
    assert_eq!(
        title("| Item | Amount |\n|---|---|"),
        Some("Item Amount".into())
    );
    assert_eq!(title("Title\r\nmore"), Some("Title".into()));
    assert_eq!(title("# Договор аренды"), Some("Договор аренды".into()));
    assert_eq!(title("\\# Not a heading"), Some("# Not a heading".into()));
    assert_eq!(title("# **  **\nThe words"), Some("The words".into()));
}

#[test]
fn a_long_first_line_is_cut_at_a_word() {
    let line = "The parties agree that the tenant may sublet the flat with the landlord's consent";
    let cut = title(line).unwrap();
    assert_eq!(
        cut,
        "The parties agree that the tenant may sublet the flat with"
    );
    assert!(cut.chars().count() <= MAX_TITLE_CHARS);
    // One long word is cut where it reaches the limit.
    let word = "ж".repeat(80);
    assert_eq!(title(&word).unwrap().chars().count(), MAX_TITLE_CHARS);
}

/// A new document's title follows the first line as it is typed, in its
/// record too, and keeps following after it reopens, until the user
/// renames it.
#[test]
fn a_new_document_follows_its_first_line_until_renamed() {
    let scratch = Scratch::new();
    let vault = vault(&scratch);
    let (session, id) = new_document(&vault, "New document");
    assert_eq!(session.info().unwrap().origin, DocumentOrigin::New);
    let follow = |session: &DocumentSession| session.follow_first_line_in_vault(vault.clone());
    assert_eq!(follow(&session).unwrap(), None);

    typed(&session, "# Lease");
    assert_eq!(follow(&session).unwrap().as_deref(), Some("Lease"));
    // The line as it was: nothing to rename.
    assert_eq!(follow(&session).unwrap(), None);
    typed(&session, " agreement\n\nThe tenant pays.");
    assert_eq!(
        follow(&session).unwrap().as_deref(),
        Some("Lease agreement")
    );
    session
        .save_pending_into_vault(vault.clone())
        .unwrap()
        .unwrap();
    let listed = vault.document(id.clone()).unwrap();
    assert_eq!(listed.title, "Lease agreement");
    assert_eq!(listed.origin, DocumentOrigin::New);

    // Reopened, it is still a new document, and its line still names it.
    let reopened = DocumentSession::new();
    reopened.open_from_vault(vault.clone(), id.clone()).unwrap();
    assert_eq!(reopened.info().unwrap().origin, DocumentOrigin::New);
    assert_eq!(follow(&reopened).unwrap(), None);
    reopened
        .type_text(TextRange { start: 2, end: 7 }, "Rental".into())
        .unwrap();
    assert_eq!(
        follow(&reopened).unwrap().as_deref(),
        Some("Rental agreement")
    );

    // Renamed by the user, it stops following, also when reopened.
    reopened
        .rename_in_vault(vault.clone(), "Flat 3".into())
        .unwrap();
    assert_eq!(reopened.info().unwrap().origin, DocumentOrigin::Ordinary);
    typed(&reopened, "\nMore");
    reopened
        .type_text(TextRange { start: 2, end: 8 }, "Office".into())
        .unwrap();
    assert_eq!(follow(&reopened).unwrap(), None);
    reopened.save_pending_into_vault(vault.clone()).unwrap();
    let again = DocumentSession::new();
    again.open_from_vault(vault.clone(), id).unwrap();
    assert_eq!(again.info().unwrap().origin, DocumentOrigin::Ordinary);
    assert_eq!(again.snapshot().unwrap().title, "Flat 3");
}

/// Before its first save only the title changes; the save keeps it.
#[test]
fn an_unsaved_new_document_follows_too() {
    let scratch = Scratch::new();
    let vault = vault(&scratch);
    let session = DocumentSession::new();
    session
        .import_text_as(String::new(), "New document".into(), DocumentOrigin::New)
        .unwrap();
    typed(&session, "Notes");
    assert_eq!(
        session.follow_first_line_in_vault(vault.clone()).unwrap(),
        Some("Notes".into())
    );
    assert!(vault.documents().is_empty());
    let saved = session.save_into_vault(vault.clone()).unwrap();
    assert_eq!(vault.document(saved.id).unwrap().title, "Notes");
}

/// Left empty, a new document goes; with text, renamed, or not new, it
/// stays.
#[test]
fn an_empty_new_document_is_discarded() {
    let scratch = Scratch::new();
    let vault = vault(&scratch);
    let discard = |session: &DocumentSession| session.discard_if_empty_in_vault(vault.clone());
    let (session, _) = new_document(&vault, "New document");
    // Typed and deleted again: empty is empty.
    typed(&session, "x\n");
    session
        .type_text(TextRange { start: 0, end: 2 }, String::new())
        .unwrap();
    assert!(discard(&session).unwrap());
    assert!(vault.documents().is_empty());
    assert!(session.snapshot().is_err());
    assert!(!discard(&session).unwrap());

    let (kept, _) = new_document(&vault, "New document");
    typed(&kept, "Words");
    assert!(!discard(&kept).unwrap());

    let (renamed, _) = new_document(&vault, "Second");
    renamed
        .rename_in_vault(vault.clone(), "Named".into())
        .unwrap();
    assert!(!discard(&renamed).unwrap());

    let ordinary = DocumentSession::new();
    ordinary.import_text(String::new(), "Empty".into()).unwrap();
    ordinary.save_into_vault(vault.clone()).unwrap();
    assert!(!discard(&ordinary).unwrap());
    assert_eq!(vault.documents().len(), 3);
}

/// The sample is told apart in the Vault's list, so Try a sample finds it
/// whatever it is called now.
#[test]
fn the_sample_is_known_in_the_list() {
    let scratch = Scratch::new();
    let vault = vault(&scratch);
    let session = DocumentSession::new();
    session
        .import_text_as(
            "Ann Lee signed.".into(),
            "Sample".into(),
            DocumentOrigin::Sample,
        )
        .unwrap();
    let saved = session.save_into_vault(vault.clone()).unwrap();
    vault
        .rename_document(saved.id.clone(), "Mine now".into())
        .unwrap();
    let listed = vault.document(saved.id).unwrap();
    assert_eq!(listed.title, "Mine now");
    assert_eq!(listed.origin, DocumentOrigin::Sample);
    // Its follow does nothing: only a new document follows its line.
    assert_eq!(
        session.follow_first_line_in_vault(vault.clone()).unwrap(),
        None
    );
}

/// Find names again: a second scan over the edited text
/// proposes what was typed and keeps every decision made.
#[test]
fn finding_names_again_proposes_typed_names_and_keeps_decisions() {
    let session = DocumentSession::new();
    session
        .import_text("Write to ann@example.com today.".into(), "Letter".into())
        .unwrap();
    let first = session.scan(None, None).unwrap();
    assert_eq!(first.proposed, 1);
    let mention = first.mentions[0].range;
    session
        .review(ReviewAction::Replace {
            mention: mention_id(&first, mention),
            scope: Scope::All,
        })
        .unwrap();
    typed(&session, " Copy bob@example.com.");
    let again = session.scan(None, None).unwrap();
    assert_eq!(again.phase, DocumentPhase::Reviewing);
    let states: Vec<_> = again
        .mentions
        .iter()
        .map(|m| (m.original.clone(), m.state))
        .collect();
    assert!(states.contains(&("ann@example.com".into(), MentionState::Replaced)));
    assert!(states.contains(&("bob@example.com".into(), MentionState::Proposed)));
}

fn mention_id(snapshot: &crate::document_session::ReviewSnapshot, range: TextRange) -> u64 {
    snapshot
        .mentions
        .iter()
        .find(|m| m.range == range)
        .unwrap()
        .id
}
