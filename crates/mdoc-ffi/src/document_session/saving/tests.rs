//! Saving after the tap, through the Vault (`save_pending_into_vault`).
use super::*;
use crate::{
    document_session::{
        Category, MentionState, ProposedName, ReviewAction, ReviewSnapshot, SavedVaultChange, Scope,
    },
    test_support::Scratch,
    vault::{SharedFrom, Vault},
};
use std::sync::Arc;

const LETTER: &str = "Ann Lee met Bob Hart and Cy Moss.";

fn vault(scratch: &Scratch) -> Arc<Vault> {
    Vault::open(
        scratch.path().join("vault").display().to_string(),
        vec![3; 32],
    )
    .unwrap()
}

fn propose(session: &DocumentSession, names: &[&str]) {
    let names = names
        .iter()
        .map(|wording| ProposedName {
            wording: (*wording).into(),
            category: Category::Person,
        })
        .collect();
    session
        .review(ReviewAction::ProposeNames { names })
        .unwrap();
}

/// `text` saved into the Vault, Unfiled or in `project`, with `names`
/// proposed and, if `replace`, replaced: its session and id.
fn saved(
    vault: &Arc<Vault>,
    project: Option<&str>,
    text: &str,
    names: &[&str],
    replace: bool,
) -> (Arc<DocumentSession>, String) {
    let session = DocumentSession::new();
    session.import_text(text.into(), "Document".into()).unwrap();
    if let Some(project) = project {
        session.join_project(vault.clone(), project.into()).unwrap();
    }
    if !names.is_empty() {
        propose(&session, names);
    }
    if replace {
        session.review(ReviewAction::ReplaceAll).unwrap();
    }
    let id = session.save_into_vault(vault.clone()).unwrap().id;
    assert!(!session.has_unsaved_changes());
    (session, id)
}

/// `LETTER` saved Unfiled, its three names proposed: what the taps decide.
fn saved_letter(vault: &Arc<Vault>) -> (Arc<DocumentSession>, String) {
    saved(
        vault,
        None,
        LETTER,
        &["Ann Lee", "Bob Hart", "Cy Moss"],
        false,
    )
}

/// A tap that saves nothing: Replace on the first name still proposed.
fn replace_next(session: &DocumentSession) -> ReviewSnapshot {
    let mention = session
        .snapshot()
        .unwrap()
        .mentions
        .into_iter()
        .find(|m| m.state == MentionState::Proposed)
        .unwrap()
        .id;
    session
        .review(ReviewAction::Replace {
            mention,
            scope: Scope::All,
        })
        .unwrap()
}

fn on_disk(vault: &Vault, id: &str) -> String {
    vault.store().document(id).unwrap().markdown
}

fn save(vault: &Arc<Vault>, session: &DocumentSession) -> SavedVaultChange {
    session
        .save_pending_into_vault(vault.clone())
        .unwrap()
        .unwrap()
}

fn restored(vault: &Vault, id: &str, reply: &str) -> String {
    let from = SharedFrom::Document { id: id.into() };
    vault.restore_text(from, reply.into()).unwrap().text
}

/// Runs `body` with `folder` read-only, so a write into it fails.
#[cfg(unix)]
fn read_only<R>(folder: &std::path::Path, body: impl FnOnce() -> R) -> R {
    use std::os::unix::fs::PermissionsExt;
    let mode =
        |mode| std::fs::set_permissions(folder, std::fs::Permissions::from_mode(mode)).unwrap();
    mode(0o555);
    let result = body();
    mode(0o755);
    result
}

/// Vault document `id`'s folder, which holds its record.
#[cfg(unix)]
fn folder(scratch: &Scratch, id: &str) -> std::path::PathBuf {
    scratch.path().join("vault/docs").join(id)
}

/// Where document `id` keeps its names: its Project's mapping, or its own
/// while Unfiled.
fn mapping(vault: &Vault, id: &str) -> mdoc_vault::Mapping {
    let document = vault.store().document(id).unwrap();
    match &document.project {
        Some(project) => vault.store().project(project).unwrap().mapping,
        None => document.mapping,
    }
}

/// The originals of the identity called `alias` in `mapping(id)`.
fn originals(vault: &Vault, id: &str, alias: &str) -> Vec<String> {
    mapping(vault, id)
        .identities
        .into_iter()
        .find(|identity| identity.names(alias))
        .unwrap()
        .originals
}

/// A tap changes the session only; `save_pending_into_vault` saves it,
/// and lists count it from the record it writes (#686).
#[test]
fn a_tap_waits_for_the_pending_save() {
    let scratch = Scratch::new();
    let vault = vault(&scratch);
    let (session, id) = saved_letter(&vault);
    assert_eq!(
        session.save_pending_into_vault(vault.clone()).unwrap(),
        None
    );
    let tapped = replace_next(&session);
    assert_eq!(tapped.replaced, 1);
    assert!(session.has_unsaved_changes());
    assert_eq!(on_disk(&vault, &id), LETTER, "not saved yet");
    let saved = save(&vault, &session);
    assert!(!saved.rejoined);
    assert_eq!(saved.id, id);
    assert!(!session.has_unsaved_changes());
    assert_eq!(on_disk(&vault, &id), "PERSON_1 met Bob Hart and Cy Moss.");
    let listed = vault.document(id).unwrap();
    assert_eq!((listed.proposed, listed.replaced), (2, 1));
    assert_eq!(
        session.save_pending_into_vault(vault).unwrap(),
        None,
        "saved once"
    );
}

/// Three taps before the save make one save, which leaves nothing for
/// the next; the document reopens with all three.
#[test]
fn several_taps_make_one_save() {
    let scratch = Scratch::new();
    let vault = vault(&scratch);
    let (session, id) = saved_letter(&vault);
    for _ in 0..3 {
        replace_next(&session);
    }
    save(&vault, &session);
    assert_eq!(
        session.save_pending_into_vault(vault.clone()).unwrap(),
        None
    );
    let reopened = DocumentSession::new()
        .open_from_vault(vault.clone(), id.clone())
        .unwrap();
    assert_eq!(reopened.text, "PERSON_1 met PERSON_2 and PERSON_3.");
    assert_eq!(reopened.replaced, 3);
    assert_eq!(restored(&vault, &id, "PERSON_3 left."), "Cy Moss left.");
}

/// Undo and Redo before the save: the save writes where they ended.
#[test]
fn undo_and_redo_before_the_save_save_where_they_end() {
    let scratch = Scratch::new();
    let vault = vault(&scratch);
    let (session, id) = saved_letter(&vault);
    replace_next(&session);
    session.review(ReviewAction::Undo).unwrap();
    assert!(
        session
            .save_pending_into_vault(vault.clone())
            .unwrap()
            .is_some()
    );
    assert!(!session.has_unsaved_changes());
    assert_eq!(on_disk(&vault, &id), LETTER);
    session.review(ReviewAction::Redo).unwrap();
    replace_next(&session);
    session.review(ReviewAction::Undo).unwrap();
    save(&vault, &session);
    assert_eq!(on_disk(&vault, &id), "PERSON_1 met Bob Hart and Cy Moss.");
    let snapshot = session.snapshot().unwrap();
    assert!(snapshot.can_redo, "the history stays");
}

/// A save that cannot write (a full disk, a folder it may not write)
/// reports a storage failure, which the apps show as "could not save
/// this change", and keeps the change for the next save.
#[cfg(unix)]
#[test]
fn a_failed_save_keeps_the_change_and_says_so() {
    let scratch = Scratch::new();
    let vault = vault(&scratch);
    let (session, id) = saved_letter(&vault);
    replace_next(&session);
    let failed = read_only(&folder(&scratch, &id), || {
        session.save_pending_into_vault(vault.clone())
    });
    assert!(
        matches!(failed, Err(crate::ReviewError::Storage { .. })),
        "{failed:?}"
    );
    assert!(session.has_unsaved_changes());
    assert_eq!(on_disk(&vault, &id), LETTER);
    save(&vault, &session);
    assert!(!session.has_unsaved_changes());
    assert_eq!(on_disk(&vault, &id), "PERSON_1 met Bob Hart and Cy Moss.");
}

/// A tap while a save is writing (on the phone's utility queue) returns at
/// once: the save holds the document only to take what to write. The tap
/// is not in that save and stays unsaved for the next one.
#[test]
fn a_tap_during_a_slow_save_returns_at_once() {
    use std::{
        sync::mpsc,
        time::{Duration, Instant},
    };
    let scratch = Scratch::new();
    let vault = vault(&scratch);
    let (session, id) = saved_letter(&vault);
    replace_next(&session);
    let (entered, on_enter) = mpsc::channel();
    let (release, on_release) = mpsc::channel::<()>();
    *vault.during_save.lock().unwrap() = Some(Box::new(move || {
        entered.send(()).unwrap();
        on_release.recv().unwrap();
    }));
    let save_thread = {
        let (session, vault) = (session.clone(), vault.clone());
        std::thread::spawn(move || session.save_pending_into_vault(vault))
    };
    on_enter.recv_timeout(Duration::from_secs(10)).unwrap();
    // The tap runs on its own thread, so a tap that waits fails the test
    // instead of hanging it.
    let (tapped, on_tap) = mpsc::channel();
    let tap = {
        let session = session.clone();
        std::thread::spawn(move || {
            let began = Instant::now();
            let snapshot = replace_next(&session);
            tapped.send(began.elapsed()).unwrap();
            snapshot
        })
    };
    let took = on_tap.recv_timeout(Duration::from_secs(2));
    release.send(()).unwrap();
    let took = took.expect("the tap waited for the save");
    assert!(took < Duration::from_millis(500), "{took:?}");
    assert_eq!(tap.join().unwrap().replaced, 2);
    save_thread.join().unwrap().unwrap().unwrap();
    assert_eq!(on_disk(&vault, &id), "PERSON_1 met Bob Hart and Cy Moss.");
    assert!(
        session.has_unsaved_changes(),
        "the tap after the save began"
    );
    save(&vault, &session);
    assert_eq!(on_disk(&vault, &id), "PERSON_1 met PERSON_2 and Cy Moss.");
}

/// A rejoin whose write then fails is still told with the save that
/// lands: the app shows the review joined again.
#[cfg(unix)]
#[test]
fn a_rejoin_is_told_after_a_failed_write() {
    let scratch = Scratch::new();
    let vault = vault(&scratch);
    let project = vault.create_project("Case".into()).unwrap();
    let sessions: Vec<_> = [
        ("Ann Lee signed.", "Ann Lee"),
        ("Bob Hart signed.", "Bob Hart"),
    ]
    .into_iter()
    .map(|(text, name)| {
        let (session, id) = saved(&vault, Some(&project.id), text, &[], false);
        propose(&session, &[name]);
        session.review(ReviewAction::ReplaceAll).unwrap();
        (session, id)
    })
    .collect();
    save(&vault, &sessions[0].0);
    let (bob, bob_id) = &sessions[1];
    let failed = read_only(&folder(&scratch, bob_id), || {
        bob.save_pending_into_vault(vault.clone())
    });
    assert!(failed.is_err(), "{failed:?}");
    assert_eq!(bob.snapshot().unwrap().text, "PERSON_2 signed.");
    let saved = save(&vault, bob);
    assert!(saved.rejoined);
    bob.review(ReviewAction::Undo).unwrap();
    bob.review(ReviewAction::Redo).unwrap();
    let again = save(&vault, bob);
    assert!(!again.rejoined, "told once");
}

/// A document never saved is left to `save_into_vault`, its import's or
/// scan's first save, which joins the Project chosen for it.
#[test]
fn a_document_never_saved_is_left_to_its_first_save() {
    let scratch = Scratch::new();
    let vault = vault(&scratch);
    let session = DocumentSession::new();
    session.import_text(LETTER.into(), "Letter".into()).unwrap();
    assert!(session.has_unsaved_changes());
    assert_eq!(
        session.save_pending_into_vault(vault.clone()).unwrap(),
        None
    );
    assert!(vault.documents().is_empty());
}

/// A document an import replaces during the save is not marked saved: the
/// write was of the one before.
#[test]
fn a_document_replaced_during_the_save_is_not_marked_saved() {
    let scratch = Scratch::new();
    let vault = vault(&scratch);
    let (session, _) = saved_letter(&vault);
    replace_next(&session);
    let replacing = session.clone();
    *vault.during_save.lock().unwrap() = Some(Box::new(move || {
        replacing
            .import_text("Other text.".into(), "Other".into())
            .unwrap();
    }));
    assert_eq!(
        session.save_pending_into_vault(vault.clone()).unwrap(),
        None
    );
    assert_eq!(session.info().unwrap().title, "Other");
    assert!(session.has_unsaved_changes());
}

/// The Project written but the record not (its folder unwritable): the
/// next save joins the Project as written and saves the record, its names
/// unrenumbered.
#[cfg(unix)]
#[test]
fn a_save_whose_record_failed_saves_next_time() {
    let scratch = Scratch::new();
    let vault = vault(&scratch);
    let project = vault.create_project("Case".into()).unwrap();
    let (session, id) = saved(
        &vault,
        Some(&project.id),
        LETTER,
        &["Ann Lee", "Bob Hart", "Cy Moss"],
        false,
    );
    let replaced = replace_next(&session);
    // A new alias: the Project changes too.
    let mention = replaced.mentions[0].id;
    session
        .review(ReviewAction::Rename {
            mention,
            scope: Scope::All,
            alias: "CLIENT".into(),
        })
        .unwrap();
    let failed = read_only(&folder(&scratch, &id), || {
        session.save_pending_into_vault(vault.clone())
    });
    assert!(failed.is_err(), "{failed:?}");
    assert_eq!(
        originals(&vault, &id, "CLIENT"),
        ["Ann Lee"],
        "the Project was written"
    );
    assert_eq!(on_disk(&vault, &id), LETTER);
    assert!(session.has_unsaved_changes());
    save(&vault, &session);
    assert!(!session.has_unsaved_changes());
    assert_eq!(on_disk(&vault, &id), "CLIENT met Bob Hart and Cy Moss.");
    assert_eq!(restored(&vault, &id, "CLIENT left."), "Ann Lee left.");
}

/// `save_pending_into_vault` with no document read yet saves nothing.
#[test]
fn nothing_read_is_nothing_to_save() {
    let scratch = Scratch::new();
    let session = DocumentSession::new();
    assert!(!session.has_unsaved_changes());
    assert_eq!(
        session.save_pending_into_vault(vault(&scratch)).unwrap(),
        None
    );
}

/// The mention of `wording` (its original, when replaced).
fn mention_of(session: &DocumentSession, wording: &str) -> u64 {
    session
        .snapshot()
        .unwrap()
        .mentions
        .into_iter()
        .find(|m| m.original == wording)
        .unwrap()
        .id
}

fn not_a_name(session: &DocumentSession, wording: &str) -> ReviewSnapshot {
    let mention = mention_of(session, wording);
    session
        .review(ReviewAction::NotAName {
            mention,
            scope: Scope::All,
        })
        .unwrap()
}

/// Not a name on a saved, replaced name: the save takes its wording out
/// of the mapping, the alias staying reserved; Undo and the next save put
/// it back under its old alias.
#[test]
fn not_a_name_leaves_the_mapping_and_undo_brings_it_back() {
    let scratch = Scratch::new();
    let vault = vault(&scratch);
    let (session, id) = saved_letter(&vault);
    replace_next(&session);
    save(&vault, &session);
    assert_eq!(originals(&vault, &id, "PERSON_1"), ["Ann Lee"]);

    let snapshot = not_a_name(&session, "Ann Lee");
    assert_eq!(snapshot.text, LETTER);
    assert!(snapshot.mentions.iter().all(|m| m.original != "Ann Lee"));
    save(&vault, &session);
    assert_eq!(on_disk(&vault, &id), LETTER);
    assert!(originals(&vault, &id, "PERSON_1").is_empty());
    assert!(
        mapping(&vault, &id)
            .reserved_aliases()
            .contains(&"PERSON_1".to_string())
    );

    session.review(ReviewAction::Undo).unwrap();
    save(&vault, &session);
    assert_eq!(on_disk(&vault, &id), "PERSON_1 met Bob Hart and Cy Moss.");
    assert_eq!(originals(&vault, &id, "PERSON_1"), ["Ann Lee"]);
}

/// Another document's saved text still has the alias: the Project keeps
/// the wording, so that document still restores.
#[test]
fn not_a_name_stays_in_the_project_while_another_document_uses_it() {
    let scratch = Scratch::new();
    let vault = vault(&scratch);
    let project = vault.create_project("Case".into()).unwrap();
    let sessions: Vec<_> = ["Ann Lee signed.", "Ann Lee paid."]
        .into_iter()
        .map(|text| saved(&vault, Some(&project.id), text, &["Ann Lee"], true))
        .collect();
    assert_eq!(on_disk(&vault, &sessions[1].1), "PERSON_1 paid.");

    not_a_name(&sessions[0].0, "Ann Lee");
    save(&vault, &sessions[0].0);
    assert_eq!(on_disk(&vault, &sessions[0].1), "Ann Lee signed.");
    assert_eq!(originals(&vault, &sessions[0].1, "PERSON_1"), ["Ann Lee"]);
}

/// Keep instead, then Not a name, saved, then undone: the kept name comes
/// back into the mapping under its alias.
#[test]
fn undoing_not_a_name_after_keep_instead_refills_the_mapping() {
    let scratch = Scratch::new();
    let vault = vault(&scratch);
    let (session, id) = saved_letter(&vault);
    replace_next(&session);
    save(&vault, &session);
    let mention = mention_of(&session, "Ann Lee");
    session
        .review(ReviewAction::Keep {
            mention,
            scope: Scope::All,
        })
        .unwrap();
    save(&vault, &session);
    assert_eq!(originals(&vault, &id, "PERSON_1"), ["Ann Lee"]);
    not_a_name(&session, "Ann Lee");
    save(&vault, &session);
    assert!(originals(&vault, &id, "PERSON_1").is_empty());
    session.review(ReviewAction::Undo).unwrap();
    save(&vault, &session);
    assert_eq!(originals(&vault, &id, "PERSON_1"), ["Ann Lee"]);
}

/// Another document restores the alias as Restore reads it
/// ("PERSON_1-like"), so the Project keeps the wording; the document that
/// took it off still leaves it off when it opens again.
#[test]
fn a_wording_the_project_keeps_stays_off_when_the_document_opens() {
    let scratch = Scratch::new();
    let vault = vault(&scratch);
    let project = vault.create_project("Case".into()).unwrap();
    let sessions: Vec<_> = ["Ann Lee signed.", "Ann Lee paid."]
        .into_iter()
        .map(|text| saved(&vault, Some(&project.id), text, &["Ann Lee"], true))
        .collect();
    vault
        .store()
        .update_document(&sessions[1].1, |record| {
            record.markdown = "PERSON_1-like paid.".into();
        })
        .unwrap();

    not_a_name(&sessions[0].0, "Ann Lee");
    save(&vault, &sessions[0].0);
    assert_eq!(originals(&vault, &sessions[0].1, "PERSON_1"), ["Ann Lee"]);

    let reopened = DocumentSession::new()
        .open_from_vault(vault.clone(), sessions[0].1.clone())
        .unwrap();
    assert_eq!(reopened.text, "Ann Lee signed.");
    assert!(reopened.mentions.iter().all(|m| m.original != "Ann Lee"));
    assert_eq!(reopened.proposed, 0);
}

/// Another document saves between this document's saved Not a name and
/// its undo: joining the Project again keeps this document's reserved
/// alias for it, so the undo's save takes it back unrenumbered.
#[test]
fn undo_after_another_documents_save_keeps_the_alias() {
    let scratch = Scratch::new();
    let vault = vault(&scratch);
    let project = vault.create_project("Case".into()).unwrap();
    let (ann, ann_id) = saved(&vault, Some(&project.id), "Ann Lee signed.", &[], false);
    let (bob, bob_id) = saved(&vault, Some(&project.id), "Bob Hart paid.", &[], false);
    propose(&ann, &["Ann Lee"]);
    ann.review(ReviewAction::ReplaceAll).unwrap();
    save(&vault, &ann);
    not_a_name(&ann, "Ann Lee");
    save(&vault, &ann);

    propose(&bob, &["Bob Hart"]);
    bob.review(ReviewAction::ReplaceAll).unwrap();
    save(&vault, &bob);
    assert_eq!(
        on_disk(&vault, &bob_id),
        "PERSON_2 paid.",
        "PERSON_1 stays reserved"
    );

    ann.review(ReviewAction::Undo).unwrap();
    let saved = save(&vault, &ann);
    assert!(saved.rejoined);
    assert_eq!(on_disk(&vault, &ann_id), "PERSON_1 signed.");
    assert_eq!(originals(&vault, &ann_id, "PERSON_1"), ["Ann Lee"]);
}

/// Another document's saved text has only the longer custom alias
/// ("Buyer-Lead"): the shorter one's wording still leaves the Project.
#[test]
fn a_longer_alias_elsewhere_does_not_keep_a_shorter_ones_wording() {
    let scratch = Scratch::new();
    let vault = vault(&scratch);
    let project = vault.create_project("Case".into()).unwrap();
    let (session, id) = saved(
        &vault,
        Some(&project.id),
        LETTER,
        &["Ann Lee", "Bob Hart", "Cy Moss"],
        false,
    );
    let rename = |wording: &str, alias: &str| {
        let mention = mention_of(&session, wording);
        session
            .review(ReviewAction::Replace {
                mention,
                scope: Scope::All,
            })
            .unwrap();
        let mention = mention_of(&session, wording);
        session
            .review(ReviewAction::Rename {
                mention,
                scope: Scope::All,
                alias: alias.into(),
            })
            .unwrap();
    };
    rename("Ann Lee", "Buyer");
    rename("Bob Hart", "Buyer-Lead");
    save(&vault, &session);

    let (_, other) = saved(&vault, Some(&project.id), "Paid.", &[], false);
    vault
        .store()
        .update_document(&other, |record| {
            record.markdown = "Buyer-Lead paid.".into();
        })
        .unwrap();

    not_a_name(&session, "Ann Lee");
    save(&vault, &session);
    assert!(originals(&vault, &id, "Buyer").is_empty());
    assert_eq!(originals(&vault, &id, "Buyer-Lead"), ["Bob Hart"]);
}

/// Undo after another document's save: joining the Project again leaves
/// the live document's own decisions, so the undone proposal shows.
#[test]
fn undo_survives_joining_the_project_again() {
    let scratch = Scratch::new();
    let vault = vault(&scratch);
    let project = vault.create_project("Case".into()).unwrap();
    let b = DocumentSession::new();
    b.import_text("Ann Lee paid Bob Hart.".into(), "B".into())
        .unwrap();
    b.join_project(vault.clone(), project.id.clone()).unwrap();
    propose(&b, &["Ann Lee", "Bob Hart"]);
    let mention = mention_of(&b, "Ann Lee");
    b.review(ReviewAction::Replace {
        mention,
        scope: Scope::All,
    })
    .unwrap();
    b.save_into_vault(vault.clone()).unwrap();

    let (a, _) = saved(&vault, Some(&project.id), "Ann Lee signed.", &[], false);
    assert!(
        a.snapshot()
            .unwrap()
            .mentions
            .iter()
            .any(|m| m.original == "Ann Lee")
    );
    not_a_name(&a, "Ann Lee");
    save(&vault, &a);

    let mention = mention_of(&b, "Bob Hart");
    b.review(ReviewAction::Rename {
        mention,
        scope: Scope::All,
        alias: "Payee".into(),
    })
    .unwrap();
    save(&vault, &b);

    a.review(ReviewAction::Undo).unwrap();
    let saved = save(&vault, &a);
    assert!(saved.rejoined);
    assert!(
        a.snapshot()
            .unwrap()
            .mentions
            .iter()
            .any(|m| m.original == "Ann Lee")
    );
}
