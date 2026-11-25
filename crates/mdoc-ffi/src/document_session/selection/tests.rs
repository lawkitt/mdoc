use super::*;
use crate::{
    document_session::{Candidate, Category, Scope, StepKind},
    test_support::Scratch,
};

const LETTER: &str = "Ann Lee signed the lease.\n\nBob Hart paid the rent.";

fn at(text: &str, words: &str) -> TextRange {
    let start = text.find(words).unwrap() as u64;
    TextRange {
        start,
        end: start + words.len() as u64,
    }
}

fn vault(scratch: &Scratch) -> Arc<Vault> {
    Vault::open(scratch.path().display().to_string(), vec![3; 32]).unwrap()
}

/// `text` opened and scanned, with `names` found.
fn scanned(text: &str, names: &[&str]) -> Arc<DocumentSession> {
    let session = DocumentSession::new();
    session.import_text(text.into(), "Letter".into()).unwrap();
    let found = (names.iter())
        .map(|name| Candidate {
            range: at(text, name),
            category: Category::Person,
            score: 0.9,
        })
        .collect();
    session.add_candidates(found).unwrap();
    session
}

fn not_clean(proposals: u64, changed_since_scan: bool, known_names: u64) -> SelectionCheck {
    SelectionCheck::NotClean {
        proposals,
        changed_since_scan,
        known_names,
    }
}

/// "Replace N and copy" replaces the selection's proposals alone, as one
/// undo step named with its count, and hands back the selection with the
/// alias whole; that copy is clean. Undo puts both proposals back.
#[test]
fn replace_selection_is_one_undo_step() {
    let scratch = Scratch::new();
    let vault = vault(&scratch);
    let session = scanned(LETTER, &["Ann Lee", "Bob Hart"]);
    let selection = at(LETTER, "Lee signed");
    let check = session.check_selection(vault.clone(), selection);
    assert_eq!(check.unwrap(), not_clean(1, false, 0));

    let done = session.replace_selection(selection).unwrap();
    assert_eq!(done.replaced, 1);
    let text = &done.snapshot.text;
    assert_eq!(
        text,
        "PERSON_1 signed the lease.\n\nBob Hart paid the rent."
    );
    let range = done.range.start as usize..done.range.end as usize;
    assert_eq!(&text[range], "PERSON_1 signed");
    assert_eq!(done.snapshot.proposed, 1);
    let step = done.snapshot.undo_step.unwrap();
    assert_eq!(step.kind, StepKind::ReplaceSelection);
    assert_eq!(step.label, "Replace in selection · 1");
    assert_eq!(step.range, Some(done.range));
    let check = session.check_selection(vault.clone(), done.range);
    assert_eq!(check.unwrap(), SelectionCheck::Clean);

    let undone = session.undo().unwrap();
    assert_eq!(undone.text, LETTER);
    assert_eq!(undone.proposed, 2);
    assert_eq!(session.redo().unwrap().text, *text);

    // Nothing left to replace there: no step.
    let again = session.replace_selection(done.range).unwrap();
    assert_eq!(again.replaced, 0);
    assert_eq!(again.range, done.range);
    assert_eq!(
        again.snapshot.undo_step.unwrap().kind,
        StepKind::ReplaceSelection
    );
}

/// A name the Project knows, kept here, is written out: a selection that
/// holds it is not clean, before the first save (the Project joined) and
/// after it (its stored names), while the paragraph without it is clean.
#[test]
fn a_project_name_kept_here_is_not_clean() {
    let scratch = Scratch::new();
    let vault = vault(&scratch);
    let project = vault.create_project("Lease".into()).unwrap();
    let brief = scanned("Maya Bennett sued.", &["Maya Bennett"]);
    brief
        .join_project(vault.clone(), project.id.clone())
        .unwrap();
    brief.replace_all().unwrap();
    brief.save_into_vault(vault.clone()).unwrap();

    let text = "Maya Bennett signed.\n\nThe term ends in May.";
    let memo = scanned(text, &[]);
    let joined = memo.join_project(vault.clone(), project.id).unwrap();
    assert_eq!(joined.proposed, 1, "the Project's name is proposed");
    memo.keep(at(text, "Maya Bennett"), Scope::All).unwrap();
    let check =
        |memo: &DocumentSession, words| memo.check_selection(vault.clone(), at(text, words));
    assert_eq!(
        check(&memo, "Maya Bennett signed").unwrap(),
        not_clean(0, false, 1)
    );
    assert_eq!(check(&memo, "ya Benn").unwrap(), not_clean(0, false, 1));
    assert_eq!(check(&memo, "The term").unwrap(), SelectionCheck::Clean);
    memo.save_into_vault(vault.clone()).unwrap();
    assert_eq!(
        check(&memo, "Maya Bennett signed").unwrap(),
        not_clean(0, false, 1)
    );
}

/// Never scanned, a selection is "Copy with names" alone; below the floor,
/// names decided by hand count as scanned, as they do for Ready.
#[test]
fn never_scanned_unless_decided_by_hand() {
    let scratch = Scratch::new();
    let vault = vault(&scratch);
    let session = DocumentSession::new();
    session.import_text(LETTER.into(), "Letter".into()).unwrap();
    let all = at(LETTER, LETTER);
    let check = session.check_selection(vault.clone(), all);
    assert_eq!(check.unwrap(), SelectionCheck::NeverScanned);
    session.set_no_name_finder(true);
    let check = session.check_selection(vault.clone(), all);
    assert_eq!(check.unwrap(), SelectionCheck::Clean);
}

/// A selection outside the text, or inside a character, is refused.
#[test]
fn a_selection_outside_the_text_is_refused() {
    let scratch = Scratch::new();
    let vault = vault(&scratch);
    let session = scanned("Марина", &[]);
    let inside = TextRange { start: 1, end: 2 };
    let past = TextRange { start: 0, end: 99 };
    for range in [inside, past] {
        assert!(session.check_selection(vault.clone(), range).is_err());
        assert!(session.replace_selection(range).is_err());
    }
}
