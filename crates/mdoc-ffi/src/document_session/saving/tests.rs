use super::*;
use crate::{
    document_session::{Category, MentionState, ProposedName, ReviewAction, ReviewSnapshot, Scope},
    test_support::Scratch,
};
use std::sync::atomic::Ordering;

const LETTER: &str = "Ann Lee met Bob Hart and Cy Moss.";

fn library(scratch: &Scratch) -> Arc<Library> {
    Library::new(scratch.path().display().to_string(), vec![3; 32]).unwrap()
}

/// `LETTER` saved, its three names proposed: what the taps decide.
fn saved_letter(library: &Arc<Library>) -> Arc<DocumentSession> {
    let session = DocumentSession::new();
    session.import_text(LETTER.into(), "Letter".into()).unwrap();
    let names = ["Ann Lee", "Bob Hart", "Cy Moss"]
        .map(|wording| ProposedName {
            wording: wording.into(),
            category: Category::Person,
        })
        .to_vec();
    session
        .review(ReviewAction::ProposeNames { names }, Some(library.clone()))
        .unwrap();
    assert!(!session.has_unsaved_changes());
    session
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
        .review(
            ReviewAction::Replace {
                mention,
                scope: Scope::All,
            },
            None,
        )
        .unwrap()
}

fn on_disk(session: &DocumentSession) -> String {
    std::fs::read_to_string(session.info().unwrap().path.unwrap()).unwrap()
}

/// A tap without the library changes the session only; `save_pending`
/// saves it, and lists count it from the record it writes, without
/// reading the document (#686).
#[test]
fn a_tap_waits_for_save_pending() {
    let scratch = Scratch::new();
    let library = library(&scratch);
    let session = saved_letter(&library);
    assert_eq!(session.save_pending(library.clone()).unwrap(), None);
    let tapped = replace_next(&session);
    assert_eq!(tapped.replaced, 1);
    assert!(session.has_unsaved_changes());
    assert_eq!(on_disk(&session), LETTER, "not saved yet");
    let saved = session.save_pending(library.clone()).unwrap().unwrap();
    assert!(!saved.rejoined);
    assert_eq!(saved.path, session.info().unwrap().path.unwrap());
    assert!(!session.has_unsaved_changes());
    assert_eq!(on_disk(&session), "PERSON_1 met Bob Hart and Cy Moss.");
    let opens = library.summary_opens.load(Ordering::Relaxed);
    let summary = library.document_summaries().unwrap().remove(0);
    assert_eq!((summary.proposed, summary.replaced), (2, 1));
    assert_eq!(library.summary_opens.load(Ordering::Relaxed), opens);
    assert_eq!(session.save_pending(library).unwrap(), None, "saved once");
}

/// Three taps before the save make one save, which leaves nothing for
/// the next; the document reopens with all three.
#[test]
fn several_taps_make_one_save() {
    let scratch = Scratch::new();
    let library = library(&scratch);
    let session = saved_letter(&library);
    for _ in 0..3 {
        replace_next(&session);
    }
    session.save_pending(library.clone()).unwrap().unwrap();
    assert_eq!(session.save_pending(library.clone()).unwrap(), None);
    let path = session.info().unwrap().path.unwrap();
    let reopened = DocumentSession::new()
        .open_document(library.clone(), path)
        .unwrap();
    assert_eq!(reopened.text, "PERSON_1 met PERSON_2 and PERSON_3.");
    assert_eq!(reopened.replaced, 3);
    assert_eq!(
        library
            .restore(saved_case(&session), "PERSON_3 left.".into())
            .unwrap()
            .text,
        "Cy Moss left."
    );
}

fn saved_case(session: &DocumentSession) -> String {
    session.info().unwrap().case_id.unwrap()
}

/// Undo and Redo before the save: the save writes where they ended.
#[test]
fn undo_and_redo_before_the_save_save_where_they_end() {
    let scratch = Scratch::new();
    let library = library(&scratch);
    let session = saved_letter(&library);
    replace_next(&session);
    session.review(ReviewAction::Undo, None).unwrap();
    assert!(session.save_pending(library.clone()).unwrap().is_some());
    assert!(!session.has_unsaved_changes());
    assert_eq!(on_disk(&session), LETTER);
    session.review(ReviewAction::Redo, None).unwrap();
    replace_next(&session);
    session.review(ReviewAction::Undo, None).unwrap();
    session.save_pending(library.clone()).unwrap().unwrap();
    assert_eq!(on_disk(&session), "PERSON_1 met Bob Hart and Cy Moss.");
    let snapshot = session.snapshot().unwrap();
    assert!(snapshot.can_redo, "the history stays");
}

/// A save that cannot write (a full disk, a folder it may not write)
/// reports a storage failure, which the apps show as "could not save
/// this change", and keeps the change for the next save.
#[cfg(unix)]
#[test]
fn a_failed_save_keeps_the_change_and_says_so() {
    use std::os::unix::fs::PermissionsExt;
    let scratch = Scratch::new();
    let library = library(&scratch);
    let session = saved_letter(&library);
    replace_next(&session);
    let documents = library.documents_dir();
    let mode =
        |mode| std::fs::set_permissions(&documents, std::fs::Permissions::from_mode(mode)).unwrap();
    mode(0o555);
    let failed = session.save_pending(library.clone());
    mode(0o755);
    assert!(
        matches!(failed, Err(ReviewError::Storage { .. })),
        "{failed:?}"
    );
    assert!(session.has_unsaved_changes());
    assert_eq!(on_disk(&session), LETTER);
    session.save_pending(library.clone()).unwrap().unwrap();
    assert!(!session.has_unsaved_changes());
    assert_eq!(on_disk(&session), "PERSON_1 met Bob Hart and Cy Moss.");
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
    let library = library(&scratch);
    let session = saved_letter(&library);
    replace_next(&session);
    let (entered, on_enter) = mpsc::channel();
    let (release, on_release) = mpsc::channel::<()>();
    *library.during_save.lock().unwrap() = Some(Box::new(move || {
        entered.send(()).unwrap();
        on_release.recv().unwrap();
    }));
    let save = {
        let (session, library) = (session.clone(), library.clone());
        std::thread::spawn(move || session.save_pending(library))
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
    save.join().unwrap().unwrap().unwrap();
    assert_eq!(on_disk(&session), "PERSON_1 met Bob Hart and Cy Moss.");
    assert!(
        session.has_unsaved_changes(),
        "the tap after the save began"
    );
    session.save_pending(library).unwrap().unwrap();
    assert_eq!(on_disk(&session), "PERSON_1 met PERSON_2 and Cy Moss.");
}

/// Two documents of one case reviewed side by side: the second to save
/// joins the case again first, so its person moves to a free number, and
/// says so for the app to show the review again.
#[test]
fn a_save_after_another_documents_rejoins_the_case() {
    let scratch = Scratch::new();
    let library = library(&scratch);
    let case = library.create_case("Case".into()).unwrap();
    let sessions: Vec<_> = [
        ("Ann Lee signed.", "Ann Lee"),
        ("Bob Hart signed.", "Bob Hart"),
    ]
    .into_iter()
    .map(|(text, name)| {
        let session = DocumentSession::new();
        session.import_text(text.into(), name.into()).unwrap();
        session.join_case(library.clone(), case.id.clone()).unwrap();
        session.save(library.clone()).unwrap();
        session
    })
    .collect();
    for (session, name) in sessions.iter().zip(["Ann Lee", "Bob Hart"]) {
        let names = vec![ProposedName {
            wording: name.into(),
            category: Category::Person,
        }];
        session
            .review(ReviewAction::ProposeNames { names }, None)
            .unwrap();
        session.review(ReviewAction::ReplaceAll, None).unwrap();
    }
    let first = sessions[0].save_pending(library.clone()).unwrap().unwrap();
    assert!(!first.rejoined);
    let second = sessions[1].save_pending(library.clone()).unwrap().unwrap();
    assert!(second.rejoined);
    assert_eq!(sessions[1].snapshot().unwrap().text, "PERSON_2 signed.");
    assert_eq!(on_disk(&sessions[1]), "PERSON_2 signed.");
    assert_eq!(
        library
            .restore(case.id, "PERSON_1 and PERSON_2 agree.".into())
            .unwrap()
            .text,
        "Ann Lee and Bob Hart agree."
    );
}

/// A rejoin whose write then fails is still told with the save that
/// lands: the app shows the review joined again.
#[cfg(unix)]
#[test]
fn a_rejoin_is_told_after_a_failed_write() {
    use std::os::unix::fs::PermissionsExt;
    let scratch = Scratch::new();
    let library = library(&scratch);
    let case = library.create_case("Case".into()).unwrap();
    let sessions: Vec<_> = [
        ("Ann Lee signed.", "Ann Lee"),
        ("Bob Hart signed.", "Bob Hart"),
    ]
    .into_iter()
    .map(|(text, name)| {
        let session = DocumentSession::new();
        session.import_text(text.into(), name.into()).unwrap();
        session.join_case(library.clone(), case.id.clone()).unwrap();
        session.save(library.clone()).unwrap();
        let names = vec![ProposedName {
            wording: name.into(),
            category: Category::Person,
        }];
        session
            .review(ReviewAction::ProposeNames { names }, None)
            .unwrap();
        session.review(ReviewAction::ReplaceAll, None).unwrap();
        session
    })
    .collect();
    sessions[0].save_pending(library.clone()).unwrap().unwrap();
    let documents = library.documents_dir();
    let mode =
        |mode| std::fs::set_permissions(&documents, std::fs::Permissions::from_mode(mode)).unwrap();
    mode(0o555);
    let failed = sessions[1].save_pending(library.clone());
    mode(0o755);
    assert!(failed.is_err(), "{failed:?}");
    assert_eq!(sessions[1].snapshot().unwrap().text, "PERSON_2 signed.");
    let saved = sessions[1].save_pending(library.clone()).unwrap().unwrap();
    assert!(saved.rejoined);
    sessions[1].review(ReviewAction::Undo, None).unwrap();
    sessions[1].review(ReviewAction::Redo, None).unwrap();
    let again = sessions[1].save_pending(library).unwrap().unwrap();
    assert!(!again.rejoined, "told once");
}

/// A document never saved is left to `save`, its import's or scan's first
/// save, which joins the case chosen for it: saving it here would start a
/// case of its own.
#[test]
fn a_document_never_saved_is_left_to_its_first_save() {
    let scratch = Scratch::new();
    let library = library(&scratch);
    let session = DocumentSession::new();
    session.import_text(LETTER.into(), "Letter".into()).unwrap();
    assert!(session.has_unsaved_changes());
    assert_eq!(session.save_pending(library.clone()).unwrap(), None);
    assert_eq!(session.info().unwrap().path, None);
    assert!(library.cases().cases.is_empty());
}

/// A document an import replaces during the save is not marked saved: the
/// write was of the one before.
#[test]
fn a_document_replaced_during_the_save_is_not_marked_saved() {
    let scratch = Scratch::new();
    let library = library(&scratch);
    let session = saved_letter(&library);
    replace_next(&session);
    let replacing = session.clone();
    *library.during_save.lock().unwrap() = Some(Box::new(move || {
        replacing
            .import_text("Other text.".into(), "Other".into())
            .unwrap();
    }));
    assert_eq!(session.save_pending(library.clone()).unwrap(), None);
    let info = session.info().unwrap();
    assert_eq!((info.title.as_str(), info.path), ("Other", None));
    assert!(session.has_unsaved_changes());
}

/// The file written but the case not (its folder unwritable): the next
/// save takes the file as its own, never as a change made elsewhere.
#[cfg(unix)]
#[test]
fn a_save_whose_case_failed_saves_next_time() {
    use std::os::unix::fs::PermissionsExt;
    let scratch = Scratch::new();
    let library = library(&scratch);
    let session = saved_letter(&library);
    let replaced = replace_next(&session);
    // A new alias: the case changes too.
    let mention = replaced.mentions[0].id;
    session
        .review(
            ReviewAction::Rename {
                mention,
                scope: Scope::All,
                alias: "CLIENT".into(),
            },
            None,
        )
        .unwrap();
    let cases = std::path::PathBuf::from(library.cases_folder());
    let mode =
        |mode| std::fs::set_permissions(&cases, std::fs::Permissions::from_mode(mode)).unwrap();
    mode(0o555);
    let failed = session.save_pending(library.clone());
    mode(0o755);
    assert!(failed.is_err(), "{failed:?}");
    assert_eq!(
        on_disk(&session),
        "CLIENT met Bob Hart and Cy Moss.",
        "the file was written"
    );
    assert!(session.has_unsaved_changes());
    session.save_pending(library.clone()).unwrap().unwrap();
    assert!(!session.has_unsaved_changes());
    assert_eq!(
        library
            .restore(saved_case(&session), "CLIENT left.".into())
            .unwrap()
            .text,
        "Ann Lee left."
    );
}

/// `save_pending` with no document read yet saves nothing.
#[test]
fn nothing_read_is_nothing_to_save() {
    let scratch = Scratch::new();
    let session = DocumentSession::new();
    assert!(!session.has_unsaved_changes());
    assert_eq!(session.save_pending(library(&scratch)).unwrap(), None);
}
