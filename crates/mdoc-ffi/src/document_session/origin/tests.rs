use super::*;
use crate::{
    document_session::{DocumentPhase, MentionState, ReviewAction, Scope},
    library::TextRange,
    test_support::Scratch,
};

fn library(scratch: &Scratch) -> Arc<Library> {
    Library::new(scratch.path().display().to_string(), vec![3; 32]).unwrap()
}

fn title(text: &str) -> Option<String> {
    first_line_title(text.into())
}

fn end(session: &DocumentSession) -> TextRange {
    let len = session.snapshot().unwrap().text.len() as u64;
    TextRange {
        start: len,
        end: len,
    }
}

/// A new document as the apps make one: begun empty in a case of its own
/// named after it, then saved, as the import's save does.
fn new_document(library: &Arc<Library>, name: &str) -> Arc<DocumentSession> {
    let case = library.create_case(name.into()).unwrap();
    let session = DocumentSession::new();
    session
        .import_text_as(String::new(), name.into(), DocumentOrigin::New)
        .unwrap();
    session.join_case(library.clone(), case.id).unwrap();
    session.save(library.clone()).unwrap();
    session
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

/// A new document's title, its file and its case follow the first line
/// as it is typed, and keep following after it reopens.
#[test]
fn a_new_document_follows_its_first_line_until_renamed() {
    let scratch = Scratch::new();
    let library = library(&scratch);
    let session = new_document(&library, "New document");
    assert_eq!(session.info().unwrap().origin, DocumentOrigin::New);
    assert_eq!(session.follow_first_line(library.clone()).unwrap(), None);

    typed(&session, "# Lease");
    let renamed = session.follow_first_line(library.clone()).unwrap().unwrap();
    assert_eq!(renamed.title, "Lease");
    assert_eq!(renamed.case.unwrap().name, "Lease");
    assert!(renamed.path.unwrap().ends_with("Lease.md"));
    // The line as it was: nothing to rename.
    assert_eq!(session.follow_first_line(library.clone()).unwrap(), None);
    typed(&session, " agreement\n\nThe tenant pays.");
    assert_eq!(
        session
            .follow_first_line(library.clone())
            .unwrap()
            .unwrap()
            .title,
        "Lease agreement"
    );
    let saved = session.save_pending(library.clone()).unwrap().unwrap();
    let summary = library.document_summaries().unwrap();
    assert_eq!(summary[0].title, "Lease agreement");
    assert_eq!(summary[0].origin, DocumentOrigin::New);

    // Reopened, it is still a new document, and its line still names it.
    let reopened = DocumentSession::new();
    reopened.open_document(library.clone(), saved.path).unwrap();
    assert_eq!(reopened.info().unwrap().origin, DocumentOrigin::New);
    assert_eq!(reopened.follow_first_line(library.clone()).unwrap(), None);
    reopened
        .type_text(TextRange { start: 2, end: 7 }, "Rental".into())
        .unwrap();
    assert_eq!(
        reopened
            .follow_first_line(library.clone())
            .unwrap()
            .unwrap()
            .title,
        "Rental agreement"
    );

    // Renamed by the user, it stops following, also when reopened.
    let renamed = reopened
        .rename_document(library.clone(), "Flat 3".into())
        .unwrap();
    assert_eq!(reopened.info().unwrap().origin, DocumentOrigin::Ordinary);
    typed(&reopened, "\nMore");
    reopened
        .type_text(TextRange { start: 2, end: 8 }, "Office".into())
        .unwrap();
    assert_eq!(reopened.follow_first_line(library.clone()).unwrap(), None);
    reopened.save_pending(library.clone()).unwrap();
    let again = DocumentSession::new();
    again
        .open_document(library.clone(), renamed.path.unwrap())
        .unwrap();
    assert_eq!(again.info().unwrap().origin, DocumentOrigin::Ordinary);
    assert_eq!(again.snapshot().unwrap().title, "Flat 3");
}

/// A first line another document's title has gives a numbered title once,
/// not a rename at every pause.
#[test]
fn a_taken_title_is_numbered_once() {
    let scratch = Scratch::new();
    let library = library(&scratch);
    let other = DocumentSession::new();
    other.import_text("Taken".into(), "Lease".into()).unwrap();
    other.save(library.clone()).unwrap();
    let session = new_document(&library, "New document");
    typed(&session, "Lease");
    let renamed = session.follow_first_line(library.clone()).unwrap().unwrap();
    assert_eq!(renamed.title, "Lease 2");
    assert_eq!(session.follow_first_line(library.clone()).unwrap(), None);
}

/// Before its first save only the title and its empty case change.
#[test]
fn an_unsaved_new_document_follows_too() {
    let scratch = Scratch::new();
    let library = library(&scratch);
    let case = library.create_case("New document".into()).unwrap();
    let session = DocumentSession::new();
    session
        .import_text_as(String::new(), "New document".into(), DocumentOrigin::New)
        .unwrap();
    session.join_case(library.clone(), case.id.clone()).unwrap();
    typed(&session, "Notes");
    let renamed = session.follow_first_line(library.clone()).unwrap().unwrap();
    assert_eq!((renamed.path, renamed.title.as_str()), (None, "Notes"));
    assert_eq!(library.case_info(case.id).unwrap().name, "Notes");
    let saved = session.save(library.clone()).unwrap();
    assert!(saved.path.ends_with("Notes.md"));
}

/// Left empty, a new document goes with its own case; with text, renamed,
/// or not new, it stays.
#[test]
fn an_empty_new_document_is_discarded_with_its_case() {
    let scratch = Scratch::new();
    let library = library(&scratch);
    let session = new_document(&library, "New document");
    // Typed and deleted again: empty is empty.
    typed(&session, "x\n");
    session
        .type_text(TextRange { start: 0, end: 2 }, String::new())
        .unwrap();
    assert!(session.discard_if_empty(library.clone()).unwrap());
    assert!(library.document_summaries().unwrap().is_empty());
    assert!(library.cases().cases.is_empty());
    assert!(session.snapshot().is_err());
    assert!(!session.discard_if_empty(library.clone()).unwrap());

    let kept = new_document(&library, "New document");
    typed(&kept, "Words");
    assert!(!kept.discard_if_empty(library.clone()).unwrap());

    let renamed = new_document(&library, "Second");
    renamed
        .rename_document(library.clone(), "Named".into())
        .unwrap();
    assert!(!renamed.discard_if_empty(library.clone()).unwrap());

    let ordinary = DocumentSession::new();
    ordinary.import_text(String::new(), "Empty".into()).unwrap();
    ordinary.save(library.clone()).unwrap();
    assert!(!ordinary.discard_if_empty(library.clone()).unwrap());
    assert_eq!(library.document_summaries().unwrap().len(), 3);
}

/// A case that holds something else stays when the new document goes.
#[test]
fn a_case_with_other_documents_stays() {
    let scratch = Scratch::new();
    let library = library(&scratch);
    let case = library.create_case("Riverside".into()).unwrap();
    let member = DocumentSession::new();
    member.import_text("Kept".into(), "Kept".into()).unwrap();
    member.join_case(library.clone(), case.id.clone()).unwrap();
    member.save(library.clone()).unwrap();
    let session = DocumentSession::new();
    session
        .import_text_as(String::new(), "New document".into(), DocumentOrigin::New)
        .unwrap();
    session.join_case(library.clone(), case.id.clone()).unwrap();
    session.save(library.clone()).unwrap();
    assert!(session.discard_if_empty(library.clone()).unwrap());
    let info = library.case_info(case.id).unwrap();
    assert_eq!(info.documents.len(), 1);
}

/// The sample is told apart in the library's list, so Try a sample finds
/// it whatever it is called now.
#[test]
fn the_sample_is_known_in_the_list() {
    let scratch = Scratch::new();
    let library = library(&scratch);
    let session = DocumentSession::new();
    session
        .import_text_as(
            "Ann Lee signed.".into(),
            "Sample".into(),
            DocumentOrigin::Sample,
        )
        .unwrap();
    let saved = session.save(library.clone()).unwrap();
    library
        .rename_document(saved.path, "Mine now".into())
        .unwrap();
    let summaries = library.document_summaries().unwrap();
    assert_eq!(summaries[0].title, "Mine now");
    assert_eq!(summaries[0].origin, DocumentOrigin::Sample);
    // Its follow does nothing: only a new document follows its line.
    assert_eq!(session.follow_first_line(library.clone()).unwrap(), None);
}

/// Find names again (ADR 0109): a second scan over the edited text
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
        .review(
            ReviewAction::Replace {
                mention: mention_id(&first, mention),
                scope: Scope::All,
            },
            None,
        )
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
