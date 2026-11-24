//! `Vault::text_for_ai_in` reads the Document and its Project's names at
//! one moment (#1198; the review of #1194): a hook runs between the two
//! reads, where another handle's writes would land without the lock.
use crate::{
    NewDocument, NewProject, Readiness, Vault,
    records::{DocumentOrigin, SourceKind},
};
use mdoc_pii::{Category, restore::ExportedIdentity};
use std::{cell::RefCell, fs::TryLockError, sync::Arc, sync::mpsc};

type Meanwhile = Box<dyn FnOnce()>;

thread_local! {
    /// Run once between the read of the Document and the read of its
    /// Project's names.
    static BETWEEN: RefCell<Option<Meanwhile>> = RefCell::new(None);
}

pub(super) fn between_reads() {
    if let Some(meanwhile) = BETWEEN.with(|slot| slot.borrow_mut().take()) {
        meanwhile();
    }
}

fn person(alias: &str, original: &str) -> ExportedIdentity {
    ExportedIdentity {
        alias: alias.into(),
        previous: Vec::new(),
        category: Category::Person,
        custom: false,
        originals: vec![Arc::from(original)],
    }
}

/// Another handle takes a name out of the text and then forgets its
/// identity while the connector's read is between its two reads. Read at
/// one moment, the document is never Ready: it still holds the name
/// against the old names, and it changed since its review against the new
/// ones. Without the lock, the old text met the new names and left with
/// the name as written.
#[test]
fn a_write_between_the_connectors_two_reads_waits_for_the_check() {
    let temp = tempfile::tempdir().unwrap();
    let root = temp.path().join("vault");
    let vault = Vault::open(&root, &[7; 32]).unwrap();
    let new = NewProject {
        name: "Case".into(),
        ..NewProject::default()
    };
    let project = vault.create_project(new).unwrap().id;
    let anna = [person("PERSON_1", "Anna Petrova")];
    (vault.update_project(&project, |p| p.mapping.merge(&anna))).unwrap();
    let new = NewDocument {
        project: Some(project.clone()),
        title: "Letter".into(),
        source_kind: SourceKind::Text,
        origin: DocumentOrigin::Ordinary,
        markdown: "PERSON_1 met Boris Ivanov.".into(),
    };
    let letter = vault.create_document(new).unwrap().id;
    (vault.update_document(&letter, |d| d.set_ready(true))).unwrap();
    let boris = [person("PERSON_2", "Boris Ivanov")];
    (vault.update_project(&project, |p| p.mapping.merge(&boris))).unwrap();
    assert_eq!(
        vault.readiness(&letter).unwrap(),
        Readiness::NamesNotReplaced(1)
    );

    // The other handle opens now: opening takes the lock too. Between the
    // reads it tries the lock without waiting: free, it writes at once, as
    // any writer would without the lock; held, it writes once the read lets
    // go. Either way the hook returns only once it knows.
    let (go, start) = mpsc::channel::<()>();
    let (tried, outcome) = mpsc::channel::<bool>();
    let (opened, ready) = mpsc::channel::<()>();
    let writer = {
        let (root, letter, project) = (root.clone(), letter.clone(), project.clone());
        std::thread::spawn(move || {
            let other = Vault::open(&root, &[7; 32]).unwrap();
            opened.send(()).unwrap();
            start.recv().unwrap();
            let free = {
                let file = other.lock_file.lock().unwrap();
                match file.try_lock() {
                    Ok(()) => {
                        file.unlock().unwrap();
                        true
                    }
                    Err(TryLockError::WouldBlock) => false,
                    Err(TryLockError::Error(error)) => panic!("{error}"),
                }
            };
            if !free {
                tried.send(false).unwrap();
            }
            let edit = |d: &mut crate::Document| d.markdown = "PERSON_1 met PERSON_2.".into();
            other.update_document(&letter, edit).unwrap();
            let forget = |p: &mut crate::Project| assert!(p.forget_identity("PERSON_2"));
            other.update_project(&project, forget).unwrap();
            if free {
                tried.send(true).unwrap();
            }
        })
    };
    ready.recv().unwrap();
    BETWEEN.with(|slot| {
        *slot.borrow_mut() = Some(Box::new(move || {
            go.send(()).unwrap();
            outcome.recv().unwrap();
        }))
    });
    let read = vault.text_for_ai_in(&project, &letter).unwrap();
    writer.join().unwrap();
    assert!(
        read.checked.is_err(),
        "handed out across two moments: {:?}",
        read.checked
    );
    assert_eq!(
        vault.readiness(&letter).unwrap(),
        Readiness::ChangedSinceReview
    );
}
