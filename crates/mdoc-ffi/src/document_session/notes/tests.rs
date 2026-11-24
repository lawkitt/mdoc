//! Notes saved with a Vault document and a promoted reply opened as one,
//! through the exported objects, on a temp folder with an injected key.
use super::super::*;
use crate::{
    test_support::Scratch,
    text::TextRange,
    vault::{SharedFrom, Vault},
};

const KEY: [u8; 32] = [7; 32];
const NOTES: &str = "Call Ann Lee's lawyer before Friday.";

fn vault(scratch: &Scratch) -> Arc<Vault> {
    Vault::open(
        scratch.path().join("vault").display().to_string(),
        KEY.to_vec(),
    )
    .unwrap()
}

/// "Ann Lee wrote." with Ann Lee replaced, saved Unfiled or into
/// `project`.
fn reviewed(vault: &Arc<Vault>, project: Option<&str>) -> (Arc<DocumentSession>, String) {
    let session = DocumentSession::new();
    session
        .import_text("Ann Lee wrote.".into(), "Letter".into())
        .unwrap();
    if let Some(project) = project {
        session.join_project(vault.clone(), project.into()).unwrap();
    }
    session
        .add_candidates(vec![Candidate {
            range: TextRange { start: 0, end: 7 },
            category: Category::Person,
            score: 0.9,
        }])
        .unwrap();
    session.replace_all().unwrap();
    session.set_no_name_finder(true);
    let id = session.save_into_vault(vault.clone()).unwrap().id;
    (session, id)
}

/// The alias Ann Lee has in `session`.
fn alias(session: &DocumentSession) -> String {
    let snapshot = session.snapshot().unwrap();
    snapshot.mentions[0].alias.clone().unwrap()
}

fn reopened(vault: &Arc<Vault>, id: &str) -> Arc<DocumentSession> {
    let session = DocumentSession::new();
    session.open_from_vault(vault.clone(), id.into()).unwrap();
    session
}

/// Notes are saved by the save that follows a change, sealed in the
/// record with the review, and opened with the document again.
#[test]
fn notes_save_with_the_document_and_reopen() {
    let scratch = Scratch::new();
    let vault = vault(&scratch);
    let (session, id) = reviewed(&vault, None);
    assert_eq!(session.notes(), "");
    assert_eq!(
        session.save_pending_into_vault(vault.clone()).unwrap(),
        None
    );

    session.set_notes(NOTES.into()).unwrap();
    assert!(session.has_unsaved_changes(), "notes alone are a change");
    let saved = session.save_pending_into_vault(vault.clone()).unwrap();
    assert_eq!(saved.map(|saved| saved.id), Some(id.clone()));
    assert!(!session.has_unsaved_changes());
    assert_eq!(vault.store().document(&id).unwrap().notes, NOTES);

    // The same notes again are no change.
    session.set_notes(NOTES.into()).unwrap();
    assert!(!session.has_unsaved_changes());

    // A review action and a notes change in one save; the document's
    // later saves keep the notes.
    session.undo().unwrap();
    session.set_notes(format!("{NOTES} Done.")).unwrap();
    session.save_pending_into_vault(vault.clone()).unwrap();
    session.redo().unwrap();
    session.save_pending_into_vault(vault.clone()).unwrap();
    assert_eq!(reopened(&vault, &id).notes(), format!("{NOTES} Done."));

    let sealed = std::fs::read_dir(scratch.path().join("vault"))
        .unwrap()
        .flatten()
        .flat_map(|entry| walk(&entry.path()))
        .any(|bytes| bytes.windows(10).any(|w| w == &NOTES.as_bytes()[..10]));
    assert!(!sealed, "the notes are sealed in the record");
}

fn walk(path: &std::path::Path) -> Vec<Vec<u8>> {
    if path.is_dir() {
        (std::fs::read_dir(path).unwrap().flatten())
            .flat_map(|entry| walk(&entry.path()))
            .collect()
    } else {
        vec![std::fs::read(path).unwrap()]
    }
}

/// Notes never leave with the text: not in Copy for AI, Share or the
/// Vault's text for AI, nor in a snapshot.
#[test]
fn notes_never_leave_with_the_text() {
    let scratch = Scratch::new();
    let vault = vault(&scratch);
    let (session, id) = reviewed(&vault, None);
    session.set_notes(NOTES.into()).unwrap();
    session.save_pending_into_vault(vault.clone()).unwrap();
    let session = reopened(&vault, &id);
    assert_eq!(session.notes(), NOTES);

    let left = [
        session.copy_for_ai().unwrap(),
        session.share_text().unwrap(),
        vault.text_for_ai(id.clone()).unwrap(),
        format!("{:?}", session.snapshot().unwrap()),
        format!("{:?}", vault.document(id).unwrap()),
    ];
    for text in left {
        assert!(!text.contains("lawyer"), "{text}");
    }
}

/// A move keeps the notes: the session saves before it and opens the
/// moved record again.
#[test]
fn notes_follow_a_move() {
    let scratch = Scratch::new();
    let vault = vault(&scratch);
    let (session, id) = reviewed(&vault, None);
    session.set_notes(NOTES.into()).unwrap();
    session.save_pending_into_vault(vault.clone()).unwrap();
    let project = vault.create_project("Lee".into()).unwrap();
    vault.move_document(id.clone(), Some(project.id)).unwrap();
    assert_eq!(reopened(&vault, &id).notes(), NOTES);
}

/// Promote to Document: a reply to a Project's document becomes a new
/// document in that Project, its text restored, not reviewed, naming the
/// reply; the reply stays, and once it is deleted the way back says so.
#[test]
fn a_promoted_reply_opens_as_a_document_with_the_way_back() {
    let scratch = Scratch::new();
    let vault = vault(&scratch);
    let project = vault.create_project("Lee".into()).unwrap();
    let (session, id) = reviewed(&vault, Some(&project.id));
    let alias = alias(&session);
    let reply = vault
        .save_reply(
            SharedFrom::Document { id },
            "Reply to Letter".into(),
            None,
            format!("Tell {alias} to sign."),
        )
        .unwrap();

    let promoted = vault.promote_reply(reply.id.clone()).unwrap();
    assert_eq!(promoted.project_id.as_deref(), Some(project.id.as_str()));
    assert_eq!(promoted.title, "Reply to Letter");
    assert!(!promoted.scanned);
    assert!(!promoted.has_original);
    assert_eq!(vault.project_page(project.id).unwrap().documents.len(), 2);

    let opened = DocumentSession::new();
    let snapshot = (opened.open_from_vault(vault.clone(), promoted.id.clone())).unwrap();
    assert_eq!(snapshot.text, "Tell Ann Lee to sign.");
    assert!(
        snapshot.proposed > 0,
        "the Project's names are proposed again"
    );
    assert_eq!(opened.from_reply(), Some(reply.id.clone()));
    assert_eq!(opened.notes(), "");
    assert_eq!(vault.reply_info(reply.id.clone()), Some(reply.clone()));
    assert!(matches!(
        vault.readiness(promoted.id).unwrap(),
        crate::vault::Readiness::NotReviewed
    ));

    vault.delete_reply(reply.id.clone()).unwrap();
    assert_eq!(vault.reply_info(reply.id.clone()), None);
    assert!(vault.promote_reply(reply.id).is_err());
    assert!(opened.from_reply().is_some(), "the record still names it");
}

/// An Unfiled document's reply promotes into Unfiled, where its review
/// marks the reply's names under the same aliases.
#[test]
fn an_unfiled_reply_promotes_into_unfiled() {
    let scratch = Scratch::new();
    let vault = vault(&scratch);
    let (session, id) = reviewed(&vault, None);
    let alias = alias(&session);
    let reply = vault
        .save_reply(
            SharedFrom::Document { id },
            "Reply".into(),
            None,
            format!("Ask {alias}."),
        )
        .unwrap();
    let promoted = vault.promote_reply(reply.id).unwrap();
    assert_eq!(promoted.project_id, None);
    let opened = DocumentSession::new();
    let snapshot = opened.open_from_vault(vault.clone(), promoted.id).unwrap();
    assert_eq!(snapshot.text, "Ask Ann Lee.");
    let mention = snapshot.mentions.first().expect("the name is marked again");
    assert_eq!(mention.alias.as_deref(), Some(alias.as_str()));
}
