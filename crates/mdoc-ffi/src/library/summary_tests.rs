//! `Library::document_summaries`: counts from the library's record while
//! each file is as recorded, and from the file otherwise.
use super::*;
use crate::{
    DocumentSession,
    document_session::{
        Category, DocumentPhase, ProposedName, ReviewAction, ReviewSnapshot, Scope,
    },
    library::TextRange,
    test_support::Scratch,
};
use std::sync::atomic::Ordering;

fn library(scratch: &Scratch) -> Arc<Library> {
    Library::new(scratch.path().display().to_string(), vec![3; 32]).unwrap()
}

/// `text` saved as `title`, with `names` proposed (a scan) unless `None`,
/// and the first `replace` of its mentions replaced.
fn saved(
    library: &Arc<Library>,
    text: &str,
    title: &str,
    names: Option<&[&str]>,
    replace: usize,
) -> (String, ReviewSnapshot) {
    let session = DocumentSession::new();
    session.import_text(text.into(), title.into()).unwrap();
    let mut snapshot = session.snapshot().unwrap();
    if let Some(names) = names {
        let names = names
            .iter()
            .map(|wording| ProposedName {
                wording: (*wording).into(),
                category: Category::Person,
            })
            .collect();
        snapshot = session
            .review(ReviewAction::ProposeNames { names }, None)
            .unwrap();
    }
    for mention in snapshot.mentions.clone().iter().take(replace) {
        let action = ReviewAction::Replace {
            mention: mention.id,
            scope: Scope::This,
        };
        session.review(action, None).unwrap();
    }
    let saved = session.save(library.clone()).unwrap();
    (saved.path, saved.review)
}

fn opens(library: &Library) -> usize {
    library.summary_opens.load(Ordering::Relaxed)
}

fn summary<'a>(summaries: &'a [DocumentSummary], path: &str) -> &'a DocumentSummary {
    summaries.iter().find(|s| s.path == path).unwrap()
}

/// What opening the document gives, which a summary must agree with.
fn reopened(library: &Arc<Library>, path: &str) -> (u64, u64, u64, DocumentPhase) {
    let snapshot = DocumentSession::new()
        .open_document(library.clone(), path.into())
        .unwrap();
    (
        snapshot.proposed,
        snapshot.replaced,
        snapshot.kept,
        snapshot.phase,
    )
}

fn counts(summary: &DocumentSummary) -> (u64, u64, u64, DocumentPhase) {
    (
        summary.proposed,
        summary.replaced,
        summary.kept,
        summary.phase,
    )
}

/// Documents as saved list from their records: nothing is opened.
#[test]
fn fresh_documents_list_without_opening_any() {
    let scratch = Scratch::new();
    let library = library(&scratch);
    let (letter, review) = saved(
        &library,
        "Ann Lee met Bob Hart.",
        "Letter",
        Some(&["Ann Lee", "Bob Hart"]),
        1,
    );
    let (empty, _) = saved(&library, "Nothing here.", "Empty", Some(&["Ann Lee"]), 0);
    let (unscanned, _) = saved(&library, "Not scanned.", "Draft", None, 0);
    for _ in 0..2 {
        let summaries = library.document_summaries().unwrap();
        assert_eq!(opens(&library), 0);
        assert_eq!(summaries.len(), 3);
        let letter = summary(&summaries, &letter);
        assert_eq!(letter.title, "Letter");
        assert_eq!(letter.case_id, review.case_id);
        assert_eq!(counts(letter), (1, 1, 0, DocumentPhase::Reviewing));
        assert!(letter.scanned && letter.opened.is_some());
        let empty = summary(&summaries, &empty);
        assert_eq!(counts(empty), (0, 0, 0, DocumentPhase::Ready));
        assert_eq!(
            summary(&summaries, &unscanned).phase,
            DocumentPhase::Unscanned
        );
    }
    let summaries = library.document_summaries().unwrap();
    for path in [&letter, &empty, &unscanned] {
        assert_eq!(counts(summary(&summaries, path)), reopened(&library, path));
    }
    // Paths and order as `documents` lists them.
    let listed: Vec<_> = library
        .documents()
        .unwrap()
        .into_iter()
        .map(|d| d.path)
        .collect();
    let summarized: Vec<_> = summaries.into_iter().map(|s| s.path).collect();
    assert_eq!(listed, summarized);
}

/// A file changed since its record (edited elsewhere, moved into another
/// case) is read and counted as opening it would, once.
#[test]
fn a_changed_file_is_counted_again_once() {
    let scratch = Scratch::new();
    let library = library(&scratch);
    let (letter, review) = saved(
        &library,
        "Ann Lee met Bob Hart.",
        "Letter",
        Some(&["Ann Lee", "Bob Hart"]),
        2,
    );
    assert_eq!(review.text, "PERSON_1 met PERSON_2.");
    std::fs::write(&letter, "PERSON_1 met PERSON_2, and Ann Lee again.").unwrap();
    let summaries = library.document_summaries().unwrap();
    assert_eq!(opens(&library), 1);
    let edited = summary(&summaries, &letter);
    assert_eq!(counts(edited), (1, 2, 0, DocumentPhase::Reviewing));
    assert!(edited.scanned, "its record still says scanned");
    library.document_summaries().unwrap();
    assert_eq!(opens(&library), 1, "the record was brought up to date");
    assert_eq!(counts(edited), reopened(&library, &letter));
    // Moved into another case: counted again, and so is that case's other
    // member, whose case changed.
    let (memo_path, memo) = saved(&library, "Bob Hart wrote.", "Memo", Some(&["Bob Hart"]), 1);
    let memo_case = memo.case_id.unwrap();
    library
        .move_document(letter.clone(), memo_case.clone())
        .unwrap();
    let before = opens(&library);
    let summaries = library.document_summaries().unwrap();
    assert_eq!(opens(&library), before + 2);
    let moved = summary(&summaries, &letter);
    assert_eq!(moved.case_id, Some(memo_case));
    assert_eq!(counts(moved), reopened(&library, &letter));
    assert_eq!(
        counts(summary(&summaries, &memo_path)),
        reopened(&library, &memo_path)
    );
}

/// Renamed or removed outside the app: the new file is counted, and the
/// records of files gone are dropped.
#[test]
fn files_renamed_or_removed_outside_the_app() {
    let scratch = Scratch::new();
    let library = library(&scratch);
    let index = || std::fs::read_to_string(scratch.path().join("documents.json")).unwrap();
    let (letter, _) = saved(&library, "Ann Lee wrote.", "Letter", Some(&["Ann Lee"]), 1);
    let (memo, _) = saved(&library, "Bob Hart wrote.", "Memo", Some(&["Bob Hart"]), 0);
    let renamed = library.documents_dir().join("Renamed.md");
    std::fs::rename(&letter, &renamed).unwrap();
    std::fs::remove_file(&memo).unwrap();
    let summaries = library.document_summaries().unwrap();
    assert_eq!(opens(&library), 1);
    assert_eq!(summaries.len(), 1);
    let renamed = &summaries[0];
    assert_eq!(renamed.title, "Renamed");
    // Its case names the old file: in no case now, its alias as text.
    assert_eq!(renamed.case_id, None);
    assert!(!index().contains("Letter") && !index().contains("Memo"));
    assert!(index().contains("Renamed"));
}

/// Saved before the library kept records: each is counted once, and
/// scanned when it has mentions, as `open_document` tells it.
#[test]
fn documents_saved_before_records_are_counted_once() {
    let scratch = Scratch::new();
    let library = library(&scratch);
    let (letter, _) = saved(
        &library,
        "Ann Lee met Bob Hart.",
        "Letter",
        Some(&["Ann Lee", "Bob Hart"]),
        1,
    );
    let (empty, _) = saved(&library, "Nothing here.", "Empty", Some(&["Ann Lee"]), 0);
    std::fs::remove_file(scratch.path().join("documents.json")).unwrap();
    let summaries = library.document_summaries().unwrap();
    assert_eq!(opens(&library), 2);
    let letter = summary(&summaries, &letter);
    assert_eq!(counts(letter), (1, 1, 0, DocumentPhase::Reviewing));
    assert_eq!((letter.scanned, letter.opened), (true, None));
    assert_eq!(summary(&summaries, &empty).phase, DocumentPhase::Unscanned);
    library.document_summaries().unwrap();
    assert_eq!(opens(&library), 2);
    assert!(library.document_summaries().unwrap().len() == 2);
}

/// A save that lands while a list counts keeps what it wrote: the list's
/// write-back never puts back the entry it read, so scanned stays.
#[test]
fn a_list_racing_a_save_keeps_the_save() {
    let scratch = Scratch::new();
    let library = library(&scratch);
    let session = DocumentSession::new();
    session
        .import_text("Nothing here.".into(), "Empty".into())
        .unwrap();
    let names = vec![ProposedName {
        wording: "Ann Lee".into(),
        category: Category::Person,
    }];
    session
        .review(ReviewAction::ProposeNames { names }, None)
        .unwrap();
    let path = session.save(library.clone()).unwrap().path;
    // No entry: the list counts it, as found then, with no mentions.
    std::fs::remove_file(scratch.path().join("documents.json")).unwrap();
    let (racing, saving) = (session.clone(), library.clone());
    *library.before_write_back.lock().unwrap() = Some(Box::new(move || {
        let range = TextRange { start: 0, end: 7 };
        let mark = ReviewAction::Mark {
            range,
            category: Some(Category::Person),
        };
        racing.review(mark, Some(saving)).unwrap();
    }));
    let raced = library.document_summaries().unwrap();
    assert_eq!(raced[0].phase, DocumentPhase::Unscanned, "as read");
    let state = library.document_state(Path::new(&path)).unwrap();
    assert!(state.scanned, "the save's entry stays");
    let before = opens(&library);
    let after = library.document_summaries().unwrap();
    assert_eq!(opens(&library), before, "the save's entry is fresh");
    assert_eq!(counts(&after[0]), reopened(&library, &path));
    assert_eq!(after[0].phase, DocumentPhase::Reviewing);
}

/// A move that leaves the text as it is still changes what opening the
/// document proposes: the case's names. The list counts it again, and the
/// case's other member too.
#[test]
fn a_move_with_the_text_unchanged_is_counted_again() {
    let scratch = Scratch::new();
    let library = library(&scratch);
    let (letter, review) = saved(&library, "Ann Lee wrote.", "Letter", Some(&["Ann Lee"]), 1);
    let text = "Ann Lee called Bob Hart.";
    let (memo, _) = saved(&library, text, "Memo", Some(&["Bob Hart"]), 0);
    let summaries = library.document_summaries().unwrap();
    assert_eq!(summary(&summaries, &memo).proposed, 1);
    let case = review.case_id.unwrap();
    library.move_document(memo.clone(), case.clone()).unwrap();
    assert_eq!(std::fs::read_to_string(&memo).unwrap(), text);
    let summaries = library.document_summaries().unwrap();
    let moved = summary(&summaries, &memo);
    assert_eq!(moved.case_id, Some(case));
    assert_eq!(moved.proposed, 2, "Ann Lee, from the case, and Bob Hart");
    assert_eq!(counts(moved), reopened(&library, &memo));
    assert_eq!(
        counts(summary(&summaries, &letter)),
        reopened(&library, &letter)
    );
}

/// A file added to a case changes nothing in it, but its counts.
#[test]
fn a_document_added_to_a_case_is_counted_again() {
    let scratch = Scratch::new();
    let library = library(&scratch);
    let (_, review) = saved(&library, "Ann Lee wrote.", "Letter", Some(&["Ann Lee"]), 1);
    let note = library.documents_dir().join("Note.md");
    std::fs::write(&note, "Ann Lee phoned.").unwrap();
    let note = note.display().to_string();
    let summaries = library.document_summaries().unwrap();
    assert_eq!(
        counts(summary(&summaries, &note)),
        (0, 0, 0, DocumentPhase::Unscanned)
    );
    let case = review.case_id.unwrap();
    library.add_document(case.clone(), note.clone()).unwrap();
    let summaries = library.document_summaries().unwrap();
    let added = summary(&summaries, &note);
    assert_eq!(added.case_id, Some(case));
    assert_eq!(added.proposed, 1);
    assert_eq!(counts(added), reopened(&library, &note));
}

/// Another document of the case adds a name this one holds: opening this
/// one proposes it now, and so does its summary.
#[test]
fn a_name_another_member_adds_is_counted() {
    let scratch = Scratch::new();
    let library = library(&scratch);
    let (first, review) = saved(
        &library,
        "Bob Hart met Ann Lee.",
        "First",
        Some(&["Bob Hart"]),
        0,
    );
    let summaries = library.document_summaries().unwrap();
    assert_eq!(summary(&summaries, &first).proposed, 1);
    let second = DocumentSession::new();
    second
        .import_text("Ann Lee wrote.".into(), "Second".into())
        .unwrap();
    second
        .join_case(library.clone(), review.case_id.unwrap())
        .unwrap();
    let names = vec![ProposedName {
        wording: "Ann Lee".into(),
        category: Category::Person,
    }];
    second
        .review(ReviewAction::ProposeNames { names }, None)
        .unwrap();
    second
        .review(ReviewAction::ReplaceAll, Some(library.clone()))
        .unwrap();
    let summaries = library.document_summaries().unwrap();
    let first_now = summary(&summaries, &first);
    assert_eq!(first_now.proposed, 2);
    assert_eq!(counts(first_now), reopened(&library, &first));
}

/// A Keep is not saved: the summary of a document with one counts what
/// opening it gives, in a case that knows the kept name (proposed again)
/// and in one that does not (nothing).
#[test]
fn a_kept_name_lists_as_opening_gives_it() {
    let scratch = Scratch::new();
    let library = library(&scratch);
    let (_, letter) = saved(&library, "Ann Lee wrote.", "Letter", Some(&["Ann Lee"]), 1);
    let keep_first = |session: &DocumentSession, snapshot: &ReviewSnapshot| {
        let action = ReviewAction::Keep {
            mention: snapshot.mentions[0].id,
            scope: Scope::All,
        };
        session.review(action, Some(library.clone())).unwrap();
        session.info().unwrap().path.unwrap()
    };
    // In Letter's case, which knows Ann Lee.
    let note = DocumentSession::new();
    note.import_text("Ann Lee phoned.".into(), "Note".into())
        .unwrap();
    note.join_case(library.clone(), letter.case_id.unwrap())
        .unwrap();
    let names = vec![ProposedName {
        wording: "Ann Lee".into(),
        category: Category::Person,
    }];
    let scanned = note
        .review(ReviewAction::ProposeNames { names }, None)
        .unwrap();
    assert_eq!(scanned.proposed, 1);
    let note = keep_first(&note, &scanned);
    // In a case of its own, which never learns Bob Hart.
    let memo = DocumentSession::new();
    memo.import_text("Bob Hart phoned.".into(), "Memo".into())
        .unwrap();
    let names = vec![ProposedName {
        wording: "Bob Hart".into(),
        category: Category::Person,
    }];
    let proposed = memo
        .review(ReviewAction::ProposeNames { names }, None)
        .unwrap();
    let memo = keep_first(&memo, &proposed);
    let summaries = library.document_summaries().unwrap();
    assert_eq!(
        opens(&library),
        1,
        "only Letter, whose case Note's save wrote"
    );
    let listed = counts(summary(&summaries, &note));
    assert_eq!(listed, (1, 0, 0, DocumentPhase::Reviewing));
    assert_eq!(listed, reopened(&library, &note));
    let listed = counts(summary(&summaries, &memo));
    assert_eq!(listed, (0, 0, 0, DocumentPhase::Ready));
    assert_eq!(listed, reopened(&library, &memo));
}

/// Two windows on one document: a save in one while the other opens it
/// keeps what the save wrote; the open records only when it opened.
#[test]
fn an_open_racing_a_save_keeps_the_save() {
    let scratch = Scratch::new();
    let library = library(&scratch);
    let (path, _) = saved(&library, "Ann Lee wrote.", "Letter", None, 0);
    assert!(!library.document_state(Path::new(&path)).unwrap().scanned);
    let other = DocumentSession::new();
    other.open_document(library.clone(), path.clone()).unwrap();
    let (saving, names) = (
        library.clone(),
        vec![ProposedName {
            wording: "Ann Lee".into(),
            category: Category::Person,
        }],
    );
    *library.before_record.lock().unwrap() = Some(Box::new(move || {
        other
            .review(ReviewAction::ProposeNames { names }, Some(saving))
            .unwrap();
    }));
    let opened = DocumentSession::new()
        .open_document(library.clone(), path.clone())
        .unwrap();
    assert!(!opened.scanned, "as read before the save");
    let state = library.document_state(Path::new(&path)).unwrap();
    assert!(state.scanned, "the save's scanned stays");
    let summaries = library.document_summaries().unwrap();
    assert_eq!(opens(&library), 0, "the save's counts stay, and are fresh");
    assert_eq!(counts(&summaries[0]), (1, 0, 0, DocumentPhase::Reviewing));
    assert_eq!(counts(&summaries[0]), reopened(&library, &path));
}

/// The library list for 50 documents of about 10 KB and 20 names each:
/// today's path (`documents`, then `open_document` for each, as the iOS
/// app's list does) against `document_summaries` with every record fresh,
/// and with none (the first list after this change). `cargo test --release
/// -p mdoc-ffi --lib measure_the_library_list -- --ignored --nocapture`.
#[test]
#[ignore = "a measurement; run it in release"]
fn measure_the_library_list() {
    use std::time::Instant;
    let scratch = Scratch::new();
    let library = library(&scratch);
    let names: Vec<String> = (0..20).map(|n| format!("Name{n} Family{n}")).collect();
    let names: Vec<&str> = names.iter().map(String::as_str).collect();
    for d in 0..50 {
        let mut text = String::new();
        for i in 0..160 {
            text += &format!("Clause {i}: {} agrees with the terms. ", names[i % 20]);
        }
        saved(&library, &text, &format!("Document {d}"), Some(&names), 30);
    }
    let time = |label: &str, run: &mut dyn FnMut()| {
        let runs = 5;
        let began = Instant::now();
        for _ in 0..runs {
            run();
        }
        eprintln!(
            "{label}: {:.1} ms",
            began.elapsed().as_secs_f64() * 1e3 / runs as f64
        );
    };
    time("today: documents + open_document each", &mut || {
        for document in library.documents().unwrap() {
            DocumentSession::new()
                .open_document(library.clone(), document.path)
                .unwrap();
        }
    });
    time("document_summaries, records fresh", &mut || {
        assert_eq!(library.document_summaries().unwrap().len(), 50);
    });
    let index = scratch.path().join("documents.json");
    let kept = std::fs::read(&index).unwrap();
    time("document_summaries, no records", &mut || {
        std::fs::remove_file(&index).unwrap();
        library.document_summaries().unwrap();
    });
    std::fs::write(&index, kept).unwrap();
}
