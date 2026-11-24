//! Documents saved into and opened from the Vault through the exported
//! objects (#1091, Testing Decisions seam 2: Unfiled intake, Project page
//! data), on a temp folder with an injected key.
use super::super::*;
use crate::{
    document_session::actions::ReviewAction,
    test_support::Scratch,
    text::TextRange,
    vault::{ProjectDocuments, Vault},
};

const KEY: [u8; 32] = [9; 32];

fn vault(scratch: &Scratch) -> Arc<Vault> {
    Vault::open(
        scratch.path().join("vault").display().to_string(),
        KEY.to_vec(),
    )
    .unwrap()
}

fn session(text: &str, title: &str) -> Arc<DocumentSession> {
    let session = DocumentSession::new();
    session.import_text(text.into(), title.into()).unwrap();
    session
}

fn at(text: &str, wording: &str, nth: usize) -> TextRange {
    let start = text.match_indices(wording).nth(nth).unwrap().0 as u64;
    TextRange {
        start,
        end: start + wording.len() as u64,
    }
}

/// Every name in `names` proposed where it first occurs, as a scan would.
fn propose(session: &DocumentSession, names: &[&str]) -> ReviewSnapshot {
    let text = session.snapshot().unwrap().text;
    let candidates = names
        .iter()
        .map(|wording| Candidate {
            range: at(&text, wording, 0),
            category: Category::Person,
            score: 0.9,
        })
        .collect();
    session.add_candidates(candidates).unwrap()
}

/// A document reviewed, every name replaced, and saved into the Vault,
/// Unfiled or into `project`.
fn saved(
    vault: &Arc<Vault>,
    project: Option<&str>,
    text: &str,
    names: &[&str],
    title: &str,
) -> (Arc<DocumentSession>, SavedVaultDocument) {
    let session = session(text, title);
    if let Some(project) = project {
        session.join_project(vault.clone(), project.into()).unwrap();
    }
    propose(&session, names);
    session.replace_all().unwrap();
    let saved = session.save_into_vault(vault.clone()).unwrap();
    (session, saved)
}

/// What a reopened document shows the same: all but the session's own
/// mention ids and undo history.
fn reopened_view(mut snapshot: ReviewSnapshot) -> ReviewSnapshot {
    for mention in &mut snapshot.mentions {
        mention.id = 0;
    }
    snapshot.can_undo = false;
    snapshot.can_redo = false;
    snapshot.undo_step = None;
    snapshot.redo_step = None;
    snapshot
}

fn reopened(vault: &Arc<Vault>, id: &str) -> (Arc<DocumentSession>, ReviewSnapshot) {
    let session = DocumentSession::new();
    let snapshot = session.open_from_vault(vault.clone(), id.into()).unwrap();
    (session, snapshot)
}

/// Unfiled intake: an import saved into the Vault lands in Unfiled, no
/// Project is made, and its list entry has its counts from the manifest.
#[test]
fn an_import_lands_in_unfiled_and_lists_its_counts() {
    let scratch = Scratch::new();
    let vault = vault(&scratch);
    let session = session("Ann Lee met Bob Hart. Ann Lee left.", "Letter");
    propose(&session, &["Ann Lee", "Bob Hart"]);
    let snapshot = session
        .replace(at("Ann Lee met Bob Hart.", "Ann Lee", 0), Scope::All)
        .unwrap();
    let saved = session.save_into_vault(vault.clone()).unwrap();
    assert_eq!(saved.project, None);
    assert_eq!(saved.review.project_id, None);
    assert!(vault.projects().is_empty(), "no import makes a Project");

    let unfiled = vault.unfiled();
    assert_eq!(unfiled.len(), 1);
    let listed = &unfiled[0];
    assert_eq!(listed.id, saved.id);
    assert_eq!(listed.title, "Letter");
    assert_eq!(listed.project_id, None);
    assert_eq!(listed.source_kind, crate::vault::DocumentSourceKind::Text);
    assert_eq!(
        (listed.proposed, listed.replaced, listed.kept),
        (snapshot.proposed, snapshot.replaced, snapshot.kept)
    );
    assert_eq!(listed.phase, DocumentPhase::Reviewing);
    assert_eq!(vault.documents(), unfiled);

    // Read from the manifest by a handle that opens no record to list it.
    drop(vault);
    let vault = self::vault(&scratch);
    assert_eq!(vault.unfiled(), unfiled);
    let stored = vault.store().document(&saved.id).unwrap();
    assert_eq!(stored.markdown, snapshot.text);
    assert!(stored.review.is_some());
    assert_eq!(
        (stored.mapping.identities.iter())
            .map(|i| i.alias.as_str())
            .collect::<Vec<_>>(),
        ["PERSON_1", "PERSON_2"],
        "an Unfiled document keeps its names in its own mapping"
    );
}

/// The item 2 review, saved into the Vault and opened by id: the same
/// snapshot with no rescan, and both carry on alike.
#[test]
fn a_review_saved_into_the_vault_reopens_as_it_was() {
    let scratch = Scratch::new();
    let vault = vault(&scratch);
    let text = "Ann Lee met Bob Hart. Ann Lee left. Carl Fox stayed. Dana Kim came. Ann Lee rang.";
    let session = session(text, "Letter");
    propose(&session, &["Ann Lee", "Bob Hart", "Carl Fox", "Dana Kim"]);
    session
        .replace(at(text, "Bob Hart", 0), Scope::This)
        .unwrap();
    let bob = session.snapshot().unwrap().mentions[1].range;
    session.rename(bob, Scope::All, "WITNESS".into()).unwrap();
    let text = session.snapshot().unwrap().text;
    session.keep(at(&text, "Carl Fox", 0), Scope::This).unwrap();
    let dana = (session.snapshot().unwrap().mentions.into_iter())
        .find(|mention| mention.original == "Dana Kim")
        .unwrap()
        .id;
    session
        .review(ReviewAction::NotAName {
            mention: dana,
            scope: Scope::This,
        })
        .unwrap();
    session.new_alias(at(&text, "Ann Lee", 1)).unwrap();
    let before = session.snapshot().unwrap();
    let saved = session.save_into_vault(vault.clone()).unwrap();
    assert_eq!(reopened_view(saved.review), reopened_view(before.clone()));

    let (reopened, after) = reopened(&vault, &saved.id);
    assert_eq!(reopened_view(after), reopened_view(before.clone()));
    assert!(!reopened.snapshot().unwrap().can_undo);
    assert_eq!(
        reopened.saved_review().unwrap(),
        session.saved_review().unwrap()
    );
    assert_eq!(
        reopened.save_pending_into_vault(vault.clone()).unwrap(),
        None
    );

    for each in [&session, &reopened] {
        each.keep(before.mentions[1].range, Scope::This).unwrap();
        each.replace_all().unwrap();
    }
    assert_eq!(
        reopened_view(reopened.snapshot().unwrap()),
        reopened_view(session.snapshot().unwrap())
    );
}

/// Intake from a Project page: the import joins the Project, takes its
/// aliases and is proposed its names; the page lists both documents and
/// the names.
#[test]
fn intake_from_a_project_page_joins_it() {
    let scratch = Scratch::new();
    let vault = vault(&scratch);
    let project = vault.create_project("  Lee v Hart ".into()).unwrap();
    assert_eq!(project.name, "Lee v Hart");
    let (_, brief) = saved(
        &vault,
        Some(&project.id),
        "Ann Lee sued.",
        &["Ann Lee"],
        "Brief",
    );
    assert_eq!(brief.project.as_ref().unwrap().id, project.id);
    assert_eq!(
        brief.review.project_id.as_deref(),
        Some(project.id.as_str())
    );

    let memo = session("Bob Hart met Ann Lee.", "Memo");
    let joined = memo
        .join_project(vault.clone(), project.id.clone())
        .unwrap();
    let proposed: Vec<_> = (joined.mentions.iter())
        .map(|m| (m.original.as_str(), m.state))
        .collect();
    assert_eq!(proposed, [("Ann Lee", MentionState::Proposed)]);
    propose(&memo, &["Bob Hart"]);
    memo.replace_all().unwrap();
    let saved = memo.save_into_vault(vault.clone()).unwrap();
    assert_eq!(saved.review.text, "PERSON_2 met PERSON_1.");
    assert_eq!(saved.project.unwrap().documents, 2);

    let page = vault.project_page(project.id.clone()).unwrap();
    assert_eq!(page.project.documents, 2);
    let mut titles: Vec<_> = page.documents.iter().map(|d| d.title.as_str()).collect();
    titles.sort_unstable();
    assert_eq!(titles, ["Brief", "Memo"]);
    assert!(
        page.documents
            .iter()
            .all(|d| d.phase == DocumentPhase::Ready)
    );
    let names: Vec<_> = (page.names.iter())
        .map(|n| (n.alias.as_str(), n.originals.join("|"), n.category))
        .collect();
    assert_eq!(
        names,
        [
            ("PERSON_1", "Ann Lee".to_string(), Category::Person),
            ("PERSON_2", "Bob Hart".to_string(), Category::Person),
        ]
    );
    assert!(vault.unfiled().is_empty());

    // Joining another Project, or one after saving, is refused.
    let other = vault.create_project("Other".into()).unwrap();
    assert!(memo.join_project(vault.clone(), other.id).is_err());
}

/// Two documents of one Project reviewed side by side both give PERSON_1
/// to their own person; the later save moves its person to a free number
/// instead of merging the two people.
#[test]
fn stale_sessions_never_merge_different_people() {
    let scratch = Scratch::new();
    let vault = vault(&scratch);
    let project = vault.create_project("Lease".into()).unwrap();
    let ann = session("Ann Lee signed.", "Ann");
    let bob = session("Bob Hart signed.", "Bob");
    for (session, name) in [(&ann, "Ann Lee"), (&bob, "Bob Hart")] {
        session
            .join_project(vault.clone(), project.id.clone())
            .unwrap();
        propose(session, &[name]);
        session.replace_all().unwrap();
    }
    assert_eq!(bob.snapshot().unwrap().text, "PERSON_1 signed.");
    ann.save_into_vault(vault.clone()).unwrap();
    let saved = bob.save_into_vault(vault.clone()).unwrap();
    assert_eq!(saved.review.text, "PERSON_2 signed.");
    let stored = vault.store().document(&saved.id).unwrap();
    assert_eq!(stored.markdown, "PERSON_2 signed.");
    let mapping = vault.store().project(&project.id).unwrap().mapping;
    let names: Vec<_> = (mapping.identities.iter())
        .map(|i| (i.alias.as_str(), i.originals.join("|")))
        .collect();
    assert_eq!(
        names,
        [
            ("PERSON_1", "Ann Lee".to_string()),
            ("PERSON_2", "Bob Hart".to_string())
        ]
    );
}

/// Save after the tap: actions change the document only, and the next
/// `save_pending_into_vault` saves them all; another document's names
/// since make it join again and say so.
#[test]
fn pending_changes_save_into_the_vault_and_rejoin() {
    let scratch = Scratch::new();
    let vault = vault(&scratch);
    let project = vault.create_project("Lease".into()).unwrap();
    let (ann, first) = saved(
        &vault,
        Some(&project.id),
        "Ann Lee met Carl Fox.",
        &["Ann Lee"],
        "Ann",
    );
    assert_eq!(ann.save_pending_into_vault(vault.clone()).unwrap(), None);
    // Another document gives PERSON_2 to Bob meanwhile.
    saved(
        &vault,
        Some(&project.id),
        "Bob Hart and Ann Lee.",
        &["Bob Hart"],
        "Bob",
    );
    propose(&ann, &["Carl Fox"]);
    ann.replace_all().unwrap();
    assert_eq!(ann.snapshot().unwrap().text, "PERSON_1 met PERSON_2.");
    let change = ann.save_pending_into_vault(vault.clone()).unwrap().unwrap();
    assert_eq!(change.id, first.id);
    assert_eq!(change.project_id.as_deref(), Some(project.id.as_str()));
    assert!(change.rejoined);
    assert_eq!(ann.snapshot().unwrap().text, "PERSON_1 met PERSON_3.");
    let stored = vault.store().document(&first.id).unwrap();
    assert_eq!(stored.markdown, "PERSON_1 met PERSON_3.");
    assert_eq!(ann.save_pending_into_vault(vault.clone()).unwrap(), None);
    let listed = vault.document(first.id).unwrap();
    assert_eq!((listed.replaced, listed.proposed), (2, 0));
}

/// Move with undo through the exported object: the text takes the
/// Project's aliases, the review goes, the counts are taken again, and
/// undo puts the document back with its own names. A session holding the
/// document never writes over the move.
#[test]
fn a_document_moves_into_a_project_and_back() {
    let scratch = Scratch::new();
    let vault = vault(&scratch);
    let project = vault.create_project("Lease".into()).unwrap();
    saved(
        &vault,
        Some(&project.id),
        "Bob Hart sued.",
        &["Bob Hart"],
        "Brief",
    );
    let (open, memo) = saved(
        &vault,
        None,
        "Ann Lee met Bob Hart.",
        &["Ann Lee", "Bob Hart"],
        "Memo",
    );
    assert_eq!(memo.review.text, "PERSON_1 met PERSON_2.");

    let moved = vault
        .move_document(memo.id.clone(), Some(project.id.clone()))
        .unwrap();
    assert_eq!(moved.project.as_ref().unwrap().documents, 2);
    assert_eq!(moved.left, None);
    assert_eq!(
        moved.document.project_id.as_deref(),
        Some(project.id.as_str())
    );
    let stored = vault.store().document(&memo.id).unwrap();
    assert_eq!(stored.markdown, "PERSON_3 met PERSON_1.");
    assert_eq!(stored.review, None);
    assert!(stored.counts.is_some(), "counted again after the move");
    assert_eq!((moved.document.replaced, moved.document.proposed), (2, 0));
    assert!(vault.unfiled().is_empty());

    // The session from before the move refuses to save over it.
    open.keep(
        at(&open.snapshot().unwrap().text, "PERSON_1", 0),
        Scope::This,
    )
    .unwrap();
    assert!(open.save_pending_into_vault(vault.clone()).is_err());
    assert_eq!(
        vault.store().document(&memo.id).unwrap().markdown,
        "PERSON_3 met PERSON_1."
    );
    // Opened again, it is the moved document in the Project.
    let (_, reopened) = reopened(&vault, &memo.id);
    assert_eq!(reopened.text, "PERSON_3 met PERSON_1.");
    assert_eq!(reopened.project_id.as_deref(), Some(project.id.as_str()));
    assert_eq!((reopened.replaced, reopened.proposed), (2, 0));

    let back = vault.undo_move(moved.undo).unwrap();
    assert_eq!(back.document.project_id, None);
    assert_eq!(back.left.unwrap().documents, 1);
    assert_eq!(
        vault.store().document(&memo.id).unwrap().markdown,
        "PERSON_1 met PERSON_2."
    );
    assert!(vault.undo_move(moved.undo).is_err(), "undone once");
    let names: Vec<_> = (vault.project_page(project.id.clone()).unwrap().names)
        .into_iter()
        .map(|n| n.originals.join("|"))
        .collect();
    assert_eq!(names, ["Bob Hart"], "the name the move added went again");

    // Into a new Project.
    let moved = vault
        .move_document_to_new_project(memo.id.clone(), "Fresh".into())
        .unwrap();
    assert_eq!(moved.project.unwrap().name, "Fresh");
    assert!(
        vault
            .move_document(memo.id, Some(moved.document.project_id.unwrap()))
            .is_err()
    );
}

/// Undo keeps the names a move added where it went when text was shared
/// from there since: a reply to it may name them.
#[test]
fn undoing_a_move_keeps_names_shared_since() {
    let scratch = Scratch::new();
    let vault = vault(&scratch);
    let project = vault.create_project("Lease".into()).unwrap();
    let (_, brief) = saved(
        &vault,
        Some(&project.id),
        "Bob Hart sued.",
        &["Bob Hart"],
        "Brief",
    );
    let (_, memo) = saved(&vault, None, "Ann Lee wrote.", &["Ann Lee"], "Memo");
    let moved = vault
        .move_document(memo.id.clone(), Some(project.id.clone()))
        .unwrap();
    let text = vault.text_for_ai(brief.id.clone()).unwrap();
    assert!(text.ends_with("PERSON_1 sued."), "{text}");
    vault.undo_move(moved.undo).unwrap();
    let names: Vec<_> = (vault.project_page(project.id).unwrap().names)
        .into_iter()
        .map(|n| n.originals.join("|"))
        .collect();
    assert_eq!(names, ["Bob Hart", "Ann Lee"]);
}

/// Renames, in a session and through the Vault object, keep the session
/// saving; deletes take documents and Projects away.
#[test]
fn documents_and_projects_are_renamed_and_deleted() {
    let scratch = Scratch::new();
    let vault = vault(&scratch);
    let (open, letter) = saved(&vault, None, "Ann Lee wrote.", &["Ann Lee"], "Letter");
    assert_eq!(
        open.rename_in_vault(vault.clone(), "  Reply  ".into())
            .unwrap(),
        "Reply"
    );
    assert!(open.rename_in_vault(vault.clone(), " ".into()).is_err());
    assert_eq!(vault.document(letter.id.clone()).unwrap().title, "Reply");
    open.keep(
        at(&open.snapshot().unwrap().text, "PERSON_1", 0),
        Scope::This,
    )
    .unwrap();
    assert!(
        open.save_pending_into_vault(vault.clone())
            .unwrap()
            .is_some()
    );

    let renamed = vault
        .rename_document(letter.id.clone(), "Answer".into())
        .unwrap();
    assert_eq!(renamed.title, "Answer");
    let project = vault.create_project("Lease".into()).unwrap();
    let project = vault.rename_project(project.id, " Matter ".into()).unwrap();
    assert_eq!(project.name, "Matter");
    assert!(vault.rename_project(project.id.clone(), "".into()).is_err());

    // Unfiled's multi-select delete.
    let (_, second) = saved(&vault, None, "Bob Hart.", &["Bob Hart"], "Second");
    let (_, third) = saved(&vault, None, "Carl Fox.", &["Carl Fox"], "Third");
    vault
        .delete_documents(vec![letter.id.clone(), second.id.clone()])
        .unwrap();
    let left: Vec<_> = vault.unfiled().into_iter().map(|d| d.id).collect();
    assert_eq!(left, std::slice::from_ref(&third.id));
    vault.delete_document(third.id).unwrap();
    assert!(vault.documents().is_empty());
    assert!(open.save_into_vault(vault.clone()).is_err(), "deleted");
}

/// Deleting a Project asks what becomes of its documents: Unfiled, with
/// the names their text uses, or deleted.
#[test]
fn a_deleted_projects_documents_move_to_unfiled_or_go() {
    let scratch = Scratch::new();
    let vault = vault(&scratch);
    let kept = vault.create_project("Kept".into()).unwrap();
    let (_, brief) = saved(
        &vault,
        Some(&kept.id),
        "Ann Lee sued.",
        &["Ann Lee"],
        "Brief",
    );
    vault
        .delete_project(kept.id.clone(), ProjectDocuments::MoveToUnfiled)
        .unwrap();
    assert!(vault.project(kept.id).is_err());
    let unfiled = vault.unfiled();
    assert_eq!(unfiled.len(), 1);
    assert_eq!(unfiled[0].id, brief.id);
    assert_eq!((unfiled[0].replaced, unfiled[0].proposed), (1, 0));
    let (_, reopened) = reopened(&vault, &brief.id);
    assert_eq!(reopened.text, "PERSON_1 sued.");
    assert_eq!(reopened.identities[0].originals, ["Ann Lee"]);

    let gone = vault.create_project("Gone".into()).unwrap();
    saved(
        &vault,
        Some(&gone.id),
        "Bob Hart sued.",
        &["Bob Hart"],
        "Claim",
    );
    vault
        .delete_project(gone.id, ProjectDocuments::Delete)
        .unwrap();
    assert!(vault.projects().is_empty());
    assert_eq!(vault.documents().len(), 1);
}

/// A new document follows its first line and goes when left empty.
#[test]
fn a_new_document_follows_its_first_line_and_goes_when_empty() {
    let scratch = Scratch::new();
    let vault = vault(&scratch);
    let session = DocumentSession::new();
    session
        .import_text_as(String::new(), "New document".into(), DocumentOrigin::New)
        .unwrap();
    let saved = session.save_into_vault(vault.clone()).unwrap();
    session
        .type_text(
            TextRange { start: 0, end: 0 },
            "# Notes on the visit".into(),
        )
        .unwrap();
    session.end_typing();
    assert_eq!(
        session.follow_first_line_in_vault(vault.clone()).unwrap(),
        Some("Notes on the visit".into())
    );
    assert_eq!(
        session.follow_first_line_in_vault(vault.clone()).unwrap(),
        None
    );
    // Renamed in its record already, and saved with its text.
    assert_eq!(
        vault.document(saved.id.clone()).unwrap().title,
        "Notes on the visit"
    );
    session
        .save_pending_into_vault(vault.clone())
        .unwrap()
        .unwrap();
    let listed = vault.document(saved.id.clone()).unwrap();
    assert_eq!(listed.title, "Notes on the visit");
    assert_eq!(listed.origin, DocumentOrigin::New);
    assert!(!session.discard_if_empty_in_vault(vault.clone()).unwrap());

    let empty = DocumentSession::new();
    empty
        .import_text_as(String::new(), "New document".into(), DocumentOrigin::New)
        .unwrap();
    let saved = empty.save_into_vault(vault.clone()).unwrap();
    assert!(empty.discard_if_empty_in_vault(vault.clone()).unwrap());
    assert!(vault.document(saved.id).is_err());
    assert!(empty.snapshot().is_err());
}

/// A save never overwrites a change another write made to the record since
/// this session read it (a rename elsewhere): it is refused until the
/// document is opened again.
#[test]
fn a_save_never_overwrites_a_change_made_elsewhere() {
    let scratch = Scratch::new();
    let vault = vault(&scratch);
    let (session, saved) = saved(&vault, None, "Ann Lee wrote.", &["Ann Lee"], "Letter");
    vault
        .rename_document(saved.id.clone(), "Renamed elsewhere".into())
        .unwrap();
    let text = session.snapshot().unwrap().text;
    session.keep(at(&text, "PERSON_1", 0), Scope::This).unwrap();
    assert!(matches!(
        session.save_into_vault(vault.clone()),
        Err(CoreError::Project { .. })
    ));
    let stored = vault.store().document(&saved.id).unwrap();
    assert_eq!(stored.title, "Renamed elsewhere");
    assert_eq!(stored.markdown, "PERSON_1 wrote.");
}

/// A document opened from the Vault takes the names its Project changed
/// since it was saved: a renamed alias rewrites its text, which the next
/// save stores, and Undo starts after that.
#[test]
fn a_reopened_document_takes_names_renamed_since() {
    let scratch = Scratch::new();
    let vault = vault(&scratch);
    let project = vault.create_project("Lease".into()).unwrap();
    let (_, letter) = saved(
        &vault,
        Some(&project.id),
        "Ann Lee met Bob Hart.",
        &["Ann Lee", "Bob Hart"],
        "Letter",
    );
    let (claim, _) = saved(&vault, Some(&project.id), "Ann Lee sued.", &[], "Claim");
    let ann = claim.snapshot().unwrap().mentions[0].range;
    claim.rename(ann, Scope::All, "CLIENT".into()).unwrap();
    claim
        .save_pending_into_vault(vault.clone())
        .unwrap()
        .unwrap();

    let (session, opened) = reopened(&vault, &letter.id);
    assert_eq!(opened.text, "CLIENT met PERSON_2.");
    assert!(!opened.can_undo, "Undo starts after lining the names up");
    assert_eq!(opened.proposed, 0);
    let change = session.save_pending_into_vault(vault.clone()).unwrap();
    assert!(change.is_some(), "the rewritten text is saved");
    assert_eq!(
        vault.store().document(&letter.id).unwrap().markdown,
        "CLIENT met PERSON_2."
    );
}

/// Readiness is checked when text leaves: a name another document adds to
/// the Project makes a Ready document not Ready until it is replaced.
#[test]
fn text_leaves_the_vault_only_when_ready() {
    let scratch = Scratch::new();
    let vault = vault(&scratch);
    let project = vault.create_project("Lease".into()).unwrap();
    let (letter, saved_letter) = saved(
        &vault,
        Some(&project.id),
        "Ann Lee met Bob Hart.",
        &["Ann Lee"],
        "Letter",
    );
    let id = saved_letter.id;
    assert_eq!(
        vault.readiness(id.clone()).unwrap(),
        crate::vault::Readiness::Ready
    );
    assert_eq!(
        vault.text_for_ai(id.clone()).unwrap(),
        letter.copy_for_ai().unwrap()
    );
    saved(
        &vault,
        Some(&project.id),
        "Bob Hart sued.",
        &["Bob Hart"],
        "Claim",
    );
    assert_eq!(
        vault.readiness(id.clone()).unwrap(),
        crate::vault::Readiness::NamesNotReplaced { count: 1 }
    );
    assert!(vault.text_for_ai(id.clone()).is_err());
    // Opened again, Bob is proposed; replaced and saved, it is Ready.
    let (session, opened) = reopened(&vault, &id);
    assert_eq!(opened.proposed, 1);
    assert!(!opened.can_undo);
    assert_eq!(
        vault.document(id.clone()).unwrap().phase,
        DocumentPhase::Ready
    );
    session.replace_all().unwrap();
    session
        .save_pending_into_vault(vault.clone())
        .unwrap()
        .unwrap();
    assert_eq!(
        vault.readiness(id.clone()).unwrap(),
        crate::vault::Readiness::Ready
    );
}

/// An alias a document's own Not a name retired stays retired when it is
/// opened again, Unfiled or in a Project: Undo starts afresh, so nothing
/// can give it back, and another name taking it would restore old replies
/// as someone else (review 1).
#[test]
fn an_alias_retired_by_not_a_name_stays_retired_when_reopened() {
    let scratch = Scratch::new();
    let vault = vault(&scratch);
    let project = vault.create_project("Lease".into()).unwrap();
    for place in [None, Some(project.id.as_str())] {
        let (session, saved) = saved(
            &vault,
            place,
            "Ann Lee met Bob Hart.",
            &["Ann Lee", "Bob Hart"],
            "Letter",
        );
        let ann = (session.snapshot().unwrap().mentions.into_iter())
            .find(|m| m.original == "Ann Lee")
            .unwrap();
        assert_eq!(ann.alias.as_deref(), Some("PERSON_1"));
        session
            .review(ReviewAction::NotAName {
                mention: ann.id,
                scope: Scope::All,
            })
            .unwrap();
        session
            .save_pending_into_vault(vault.clone())
            .unwrap()
            .unwrap();

        let (reopened, opened) = reopened(&vault, &saved.id);
        let bob = (opened.mentions.iter())
            .find(|m| m.original == "Bob Hart")
            .unwrap()
            .range;
        assert!(
            reopened.rename(bob, Scope::All, "PERSON_1".into()).is_err(),
            "{place:?}"
        );
        vault.delete_document(saved.id).unwrap();
    }
}

/// A Ready document moved keeps no Ready fingerprint (its text changed);
/// opened again with nothing left to review, its next save makes it Ready
/// again without an edit (review 3).
#[test]
fn a_moved_ready_document_is_ready_again_after_its_next_save() {
    let scratch = Scratch::new();
    let vault = vault(&scratch);
    let project = vault.create_project("Lease".into()).unwrap();
    let (_, memo) = saved(&vault, None, "Ann Lee wrote.", &["Ann Lee"], "Memo");
    let ready = crate::vault::Readiness::Ready;
    assert_eq!(vault.readiness(memo.id.clone()).unwrap(), ready);
    vault
        .move_document(memo.id.clone(), Some(project.id))
        .unwrap();
    assert_ne!(vault.readiness(memo.id.clone()).unwrap(), ready);
    let (session, opened) = reopened(&vault, &memo.id);
    assert_eq!(opened.proposed, 0);
    assert!(
        session
            .save_pending_into_vault(vault.clone())
            .unwrap()
            .is_some()
    );
    assert_eq!(vault.readiness(memo.id.clone()).unwrap(), ready);
    assert!(vault.store().document(&memo.id).unwrap().review.is_some());
}

/// Typing that takes a proposal away leaves the document waiting for
/// discovery: saved then, it is not Ready, as the counts may miss a name
/// (review 4).
#[test]
fn a_document_awaiting_discovery_is_not_saved_ready() {
    let scratch = Scratch::new();
    let vault = vault(&scratch);
    let session = session("Ann Lee met Bob Hart.", "Letter");
    propose(&session, &["Ann Lee", "Bob Hart"]);
    let text = session.snapshot().unwrap().text;
    session
        .replace(at(&text, "Bob Hart", 0), Scope::All)
        .unwrap();
    let text = session.snapshot().unwrap().text;
    session
        .type_text(at(&text, "Lee", 0), "Lea".into())
        .unwrap();
    assert_eq!(session.snapshot().unwrap().proposed, 0);
    let saved = session.save_into_vault(vault.clone()).unwrap();
    assert_eq!(
        vault.readiness(saved.id).unwrap(),
        crate::vault::Readiness::NotReviewed
    );
}

/// Below the name finder's floor, a document reviewed by hand (a name
/// marked and replaced, never scanned) saves Ready once no proposals are
/// left, and leaves as Copy for AI's text, still unscanned; with proposals
/// left, or above the floor, it is not reviewed (#1091 Q4: the
/// maintainer's (b)).
#[test]
fn a_document_reviewed_by_hand_is_ready_where_no_names_are_found() {
    let scratch = Scratch::new();
    let vault = vault(&scratch);
    let marked = |no_name_finder: bool| {
        let session = session("Ann Lee met Bob Hart.", "Letter");
        session.set_no_name_finder(no_name_finder);
        let text = session.snapshot().unwrap().text;
        session
            .add_selection(at(&text, "Ann Lee", 0), Some(Category::Person))
            .unwrap();
        session
    };

    let by_hand = marked(true);
    let saved = by_hand.save_into_vault(vault.clone()).unwrap();
    assert_eq!(
        vault.readiness(saved.id.clone()).unwrap(),
        crate::vault::Readiness::NotReviewed,
        "a proposal is left"
    );
    by_hand.replace_all().unwrap();
    by_hand
        .save_pending_into_vault(vault.clone())
        .unwrap()
        .unwrap();
    assert_eq!(
        vault.readiness(saved.id.clone()).unwrap(),
        crate::vault::Readiness::Ready
    );
    assert_eq!(
        vault.text_for_ai(saved.id.clone()).unwrap(),
        by_hand.hand_off().unwrap().text
    );
    assert_eq!(
        by_hand.snapshot().unwrap().phase,
        DocumentPhase::Unscanned,
        "the session's own phase is unchanged"
    );
    let listed = vault.document(saved.id.clone()).unwrap();
    assert!(!listed.scanned, "scanned stays false");
    assert_eq!(
        listed.phase,
        DocumentPhase::Unscanned,
        "lists keep their label"
    );

    let scanning = marked(false);
    scanning.replace_all().unwrap();
    let saved = scanning.save_into_vault(vault.clone()).unwrap();
    assert_eq!(
        vault.readiness(saved.id).unwrap(),
        crate::vault::Readiness::NotReviewed
    );
}

/// The rules alone, before the name finder is downloaded (#1153): an
/// email address is proposed, and the document stays unscanned, so a
/// save with nothing left to decide is not Ready on a phone that can find
/// names. A later scan finds the same mention once.
#[test]
fn the_rules_alone_mark_an_email_address_and_leave_the_document_unscanned() {
    let scratch = Scratch::new();
    let vault = vault(&scratch);
    let session = session("Ann Lee wrote from ann.lee@example.com today.", "Letter");
    let marked = session.propose_rules().unwrap();
    let emails: Vec<_> = marked
        .mentions
        .iter()
        .filter(|m| m.category == Category::Email)
        .collect();
    assert_eq!(emails.len(), 1);
    assert_eq!(emails[0].original, "ann.lee@example.com");
    assert_eq!(emails[0].state, MentionState::Proposed);
    assert!(
        marked
            .mentions
            .iter()
            .all(|m| m.category != Category::Person),
        "names need the detector"
    );
    assert!(!marked.scanned);
    assert_eq!(marked.phase, DocumentPhase::Unscanned);

    session.replace_all().unwrap();
    let saved = session.save_into_vault(vault.clone()).unwrap();
    assert_eq!(
        vault.readiness(saved.id).unwrap(),
        crate::vault::Readiness::NotReviewed,
        "no names were looked for"
    );
    let scanned = session.scan(None, None).unwrap();
    assert!(scanned.scanned);
    assert_eq!(
        scanned
            .mentions
            .iter()
            .filter(|m| m.category == Category::Email)
            .count(),
        1
    );
}

/// A document saved above the floor, unscanned, is saved Ready by its next
/// save once the phone falls below it (timing the detector can put it
/// there), with no change of its own; opened again, the same (review 3).
#[test]
fn a_document_saved_above_the_floor_is_ready_once_the_phone_falls_below_it() {
    let scratch = Scratch::new();
    let vault = vault(&scratch);
    let session = session("Ann Lee met Bob Hart.", "Letter");
    let saved = session.save_into_vault(vault.clone()).unwrap();
    let readiness = || vault.readiness(saved.id.clone()).unwrap();
    assert_eq!(readiness(), crate::vault::Readiness::NotReviewed);
    assert!(
        session
            .save_pending_into_vault(vault.clone())
            .unwrap()
            .is_none(),
        "nothing owed above the floor"
    );

    session.set_no_name_finder(true);
    assert!(
        session
            .save_pending_into_vault(vault.clone())
            .unwrap()
            .is_some()
    );
    assert_eq!(readiness(), crate::vault::Readiness::Ready);
    assert!(
        session
            .save_pending_into_vault(vault.clone())
            .unwrap()
            .is_none(),
        "saved once"
    );

    let (below, _) = reopened(&vault, &saved.id);
    below.set_no_name_finder(true);
    assert!(
        below
            .save_pending_into_vault(vault.clone())
            .unwrap()
            .is_none(),
        "opened Ready: nothing owed"
    );
    let (above, _) = reopened(&vault, &saved.id);
    assert!(
        above
            .save_pending_into_vault(vault.clone())
            .unwrap()
            .is_some(),
        "above the floor again, the fingerprint goes"
    );
    assert_eq!(readiness(), crate::vault::Readiness::NotReviewed);
}

/// The key is the app's: 32 bytes, and another key's Vault never opens.
#[test]
fn a_vault_opens_only_with_its_key() {
    let scratch = Scratch::new();
    let root = scratch.path().join("vault").display().to_string();
    assert!(Vault::open(root.clone(), vec![1; 31]).is_err());
    let vault = vault(&scratch);
    saved(&vault, None, "Ann Lee.", &["Ann Lee"], "Letter");
    drop(vault);
    assert!(Vault::open(root.clone(), vec![1; 32]).is_err());
    assert_eq!(
        Vault::open(root, KEY.to_vec()).unwrap().documents().len(),
        1
    );
}

/// The file family an import records, and a camera page marked as one.
#[test]
fn an_import_records_its_source_kind() {
    let scratch = Scratch::new();
    let vault = vault(&scratch);
    let docx = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../docs/product-media/Settlement agreement.docx");
    let session = DocumentSession::new();
    session
        .import_file(docx.display().to_string(), ImportOptions::default(), None)
        .unwrap();
    let saved = session.save_into_vault(vault.clone()).unwrap();
    assert_eq!(
        vault.document(saved.id).unwrap().source_kind,
        crate::vault::DocumentSourceKind::Word
    );
    let scan = self::session("Page one.", "Scan");
    scan.set_source_kind(crate::vault::DocumentSourceKind::Scan)
        .unwrap();
    let saved = scan.save_into_vault(vault.clone()).unwrap();
    assert_eq!(
        vault.document(saved.id).unwrap().source_kind,
        crate::vault::DocumentSourceKind::Scan
    );
}
