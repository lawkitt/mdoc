//! Readiness checked when text leaves, through the Vault's public interface
//! (#1091 Decision 19; Testing Decisions, seam 1: the outbound readiness
//! check, including a name added by another document). Ready is the
//! fingerprint of the text saved while Ready; every outbound read checks
//! the Document against its Project's names as stored then, and stores no
//! AI text.
use mdoc_pii::{Category, handoff, restore::ExportedIdentity};
use mdoc_vault::{
    Document, DocumentOrigin, Mapping, NewDocument, NewProject, Readiness, SourceKind, Vault,
    VaultError,
};
use std::{collections::BTreeMap, path::PathBuf, sync::Arc};

const KEY: [u8; 32] = [7; 32];

struct Folder {
    _temp: tempfile::TempDir,
    root: PathBuf,
}

impl Folder {
    fn new() -> Self {
        let temp = tempfile::tempdir().unwrap();
        let root = temp.path().join("vault");
        Self { _temp: temp, root }
    }
    fn open(&self) -> Vault {
        Vault::open(&self.root, &KEY).unwrap()
    }
    /// Every file in the folder, by path, with its bytes.
    fn files(&self) -> BTreeMap<PathBuf, Vec<u8>> {
        fn walk(dir: &std::path::Path, out: &mut BTreeMap<PathBuf, Vec<u8>>) {
            for entry in std::fs::read_dir(dir).unwrap().flatten() {
                let path = entry.path();
                if path.is_dir() {
                    walk(&path, out);
                } else {
                    out.insert(path.clone(), std::fs::read(&path).unwrap());
                }
            }
        }
        let mut out = BTreeMap::new();
        walk(&self.root, &mut out);
        out
    }
}

fn person(alias: &str, originals: &[&str]) -> ExportedIdentity {
    ExportedIdentity {
        alias: alias.into(),
        previous: Vec::new(),
        category: Category::Person,
        custom: false,
        originals: originals.iter().map(|o| Arc::from(*o)).collect(),
    }
}

fn project_with(vault: &Vault, names: &[ExportedIdentity]) -> String {
    let project = vault
        .create_project(NewProject {
            name: "Matter".into(),
            ..NewProject::default()
        })
        .unwrap();
    vault
        .update_project(&project.id, |p| p.mapping.merge(names))
        .unwrap()
        .id
}

fn document(vault: &Vault, project: Option<&str>, markdown: &str) -> String {
    vault
        .create_document(NewDocument {
            project: project.map(String::from),
            title: "Letter".into(),
            source_kind: SourceKind::Text,
            origin: DocumentOrigin::Ordinary,
            markdown: markdown.into(),
        })
        .unwrap()
        .id
}

/// Saved as the review saves a Ready text: scanned, no proposals left.
fn save_ready(vault: &Vault, id: &str) -> Document {
    vault
        .update_document(id, |d| {
            d.scanned = true;
            d.set_ready(true);
        })
        .unwrap()
}

#[test]
fn a_ready_document_leaves_with_the_text_copy_for_ai_builds() {
    let folder = Folder::new();
    let vault = folder.open();
    let project = project_with(
        &vault,
        &[
            person("PERSON_1", &["Anna Petrova"]),
            person("PERSON_2", &["Boris Ivanov"]),
        ],
    );
    let text = "PERSON_2 wrote to PERSON_1 and PERSON_2 again.";
    let letter = document(&vault, Some(&project), text);
    save_ready(&vault, &letter);

    assert_eq!(vault.readiness(&letter).unwrap(), Readiness::Ready);
    let sent = vault.text_for_ai(&letter).unwrap();
    assert_eq!(sent, handoff::for_ai(text, ["PERSON_2", "PERSON_1"]));
    assert!(sent.contains("such as PERSON_2 or PERSON_1."), "{sent}");
}

#[test]
fn a_name_another_document_adds_makes_a_closed_ready_document_not_ready() {
    let folder = Folder::new();
    let app = folder.open();
    let project = project_with(&app, &[person("PERSON_1", &["Anna Petrova"])]);
    // Boris was not found in the letter, so its review was Ready with him
    // left as written.
    let letter = document(&app, Some(&project), "PERSON_1 met Boris Ivanov.");
    let saved = save_ready(&app, &letter);
    assert!(app.readiness(&letter).unwrap().is_ready());
    let before = folder.files();

    // Another document of the Project, saved through a second handle (the
    // Restore extension or another window), names Boris.
    let other = folder.open();
    let memo = document(&other, Some(&project), "PERSON_2 signed.");
    other
        .update_project(&project, |p| {
            p.mapping
                .merge_from(&[person("PERSON_2", &["Boris Ivanov"])], Some(&memo));
        })
        .unwrap();
    drop(other);

    assert_eq!(
        app.readiness(&letter).unwrap(),
        Readiness::NamesNotReplaced(1)
    );
    assert!(matches!(
        app.text_for_ai(&letter),
        Err(VaultError::NotReady(Readiness::NamesNotReplaced(1)))
    ));
    // The letter's file was not touched: only its check changed.
    let letter_file = |files: &BTreeMap<PathBuf, Vec<u8>>| {
        files
            .iter()
            .find(|(path, _)| path.to_string_lossy().contains(&letter))
            .map(|(_, bytes)| bytes.clone())
            .unwrap()
    };
    assert_eq!(letter_file(&folder.files()), letter_file(&before));
    assert_eq!(app.document(&letter).unwrap(), saved);

    // Replacing the name and saving Ready again lets it leave.
    app.update_document(&letter, |d| {
        d.markdown = "PERSON_1 met PERSON_2.".into();
        d.set_ready(true);
    })
    .unwrap();
    assert_eq!(app.readiness(&letter).unwrap(), Readiness::Ready);
}

#[test]
fn an_outbound_read_stores_no_copy_of_the_ai_text() {
    let folder = Folder::new();
    let vault = folder.open();
    let project = project_with(&vault, &[person("PERSON_1", &["Anna Petrova"])]);
    let letter = document(&vault, Some(&project), "PERSON_1 signed.");
    save_ready(&vault, &letter);
    vault.flush().unwrap();
    let before = folder.files();

    let sent = vault.text_for_ai(&letter).unwrap();
    vault.readiness(&letter).unwrap();
    vault.flush().unwrap();
    drop(vault);

    assert_eq!(folder.files(), before, "an outbound read writes nothing");
    let stored = folder.open().document(&letter).unwrap();
    assert_eq!(stored.markdown, "PERSON_1 signed.");
    let record = serde_json::to_string(&stored).unwrap();
    assert!(!record.contains("Keep every placeholder"), "{record}");
    assert!(!record.contains(&sent), "{record}");
}

#[test]
fn ready_is_the_fingerprint_of_the_text_saved_while_ready() {
    let folder = Folder::new();
    let vault = folder.open();
    let project = project_with(&vault, &[person("PERSON_1", &["Anna Petrova"])]);
    let letter = document(&vault, Some(&project), "PERSON_1 signed.");

    // Never scanned, or saved with proposals left.
    assert_eq!(vault.readiness(&letter).unwrap(), Readiness::NotReviewed);
    vault
        .update_document(&letter, |d| d.set_ready(true))
        .unwrap();
    assert_eq!(
        vault.readiness(&letter).unwrap(),
        Readiness::NotReviewed,
        "not scanned"
    );
    vault
        .update_document(&letter, |d| {
            d.scanned = true;
            d.set_ready(false);
        })
        .unwrap();
    assert_eq!(vault.readiness(&letter).unwrap(), Readiness::NotReviewed);

    save_ready(&vault, &letter);
    assert_eq!(vault.readiness(&letter).unwrap(), Readiness::Ready);

    // A save that does not say it is Ready keeps the old fingerprint, which
    // the new text does not match.
    vault
        .update_document(&letter, |d| d.markdown = "PERSON_1 signed twice.".into())
        .unwrap();
    assert_eq!(
        vault.readiness(&letter).unwrap(),
        Readiness::ChangedSinceReview
    );
    assert!(matches!(
        vault.text_for_ai(&letter),
        Err(VaultError::NotReady(Readiness::ChangedSinceReview))
    ));
    save_ready(&vault, &letter);
    assert_eq!(vault.readiness(&letter).unwrap(), Readiness::Ready);

    // An edit that writes a known name back is caught before the
    // fingerprint.
    vault
        .update_document(&letter, |d| d.markdown = "Anna Petrova signed.".into())
        .unwrap();
    assert_eq!(
        vault.readiness(&letter).unwrap(),
        Readiness::NamesNotReplaced(1)
    );
}

#[test]
fn a_document_excluded_from_ai_never_leaves() {
    let folder = Folder::new();
    let vault = folder.open();
    let project = project_with(&vault, &[person("PERSON_1", &["Anna Petrova"])]);
    let letter = document(&vault, Some(&project), "PERSON_1 signed.");
    save_ready(&vault, &letter);
    vault
        .update_document(&letter, |d| d.exclude_from_ai = true)
        .unwrap();

    assert_eq!(vault.readiness(&letter).unwrap(), Readiness::Ready);
    assert!(matches!(
        vault.text_for_ai(&letter),
        Err(VaultError::ExcludedFromAi)
    ));
}

#[test]
fn an_unfiled_document_is_checked_against_its_own_names() {
    let folder = Folder::new();
    let vault = folder.open();
    // A Project's names say nothing about an Unfiled document.
    project_with(&vault, &[person("PERSON_1", &["Boris Ivanov"])]);
    let note = document(&vault, None, "PERSON_1 met Boris Ivanov.");
    vault
        .update_document(&note, |d| {
            let mut mapping = Mapping::default();
            mapping.merge(&[person("PERSON_1", &["Anna Petrova"])]);
            d.mapping = mapping;
            d.scanned = true;
            d.set_ready(true);
        })
        .unwrap();
    assert_eq!(vault.readiness(&note).unwrap(), Readiness::Ready);
    assert_eq!(
        vault.text_for_ai(&note).unwrap(),
        handoff::for_ai("PERSON_1 met Boris Ivanov.", ["PERSON_1"])
    );

    // Its own mapping gaining a name it holds makes it not Ready.
    vault
        .update_document(&note, |d| {
            d.mapping.merge(&[person("PERSON_2", &["Boris Ivanov"])]);
        })
        .unwrap();
    assert_eq!(
        vault.readiness(&note).unwrap(),
        Readiness::NamesNotReplaced(1)
    );
}

#[test]
fn a_wording_the_document_took_off_as_not_a_name_does_not_block_it() {
    let folder = Folder::new();
    let vault = folder.open();
    let project = project_with(&vault, &[person("PERSON_1", &["Anna Petrova", "Grace"])]);
    // "Grace" is a period of time in the letter, a name in the memo, which
    // still uses it: the Project keeps the wording for the memo's sake.
    let text = "PERSON_1 asked for a Grace period.";
    let letter = document(&vault, Some(&project), text);
    let memo = document(&vault, Some(&project), "Grace signed.");
    vault
        .update_project(&project, |p| {
            let pair = [("PERSON_1".to_string(), "Grace".to_string())];
            p.mapping.drop_wordings(&pair, &letter, |_| true);
        })
        .unwrap();
    save_ready(&vault, &letter);
    save_ready(&vault, &memo);

    assert_eq!(vault.readiness(&letter).unwrap(), Readiness::Ready);
    assert_eq!(
        vault.text_for_ai(&letter).unwrap(),
        handoff::for_ai(text, ["PERSON_1"])
    );
    assert_eq!(
        vault.readiness(&memo).unwrap(),
        Readiness::NamesNotReplaced(1),
        "only the letter took it off"
    );
}

#[test]
fn a_moved_document_is_not_ready_until_saved_ready_again() {
    let folder = Folder::new();
    let vault = folder.open();
    let from = project_with(&vault, &[person("PERSON_1", &["Anna Petrova"])]);
    let to = project_with(&vault, &[person("PERSON_1", &["Boris Ivanov"])]);
    let letter = document(&vault, Some(&from), "PERSON_1 signed.");
    save_ready(&vault, &letter);

    let moved = vault.move_document(&letter, Some(&to)).unwrap();
    assert_eq!(moved.document.markdown, "PERSON_2 signed.");
    assert_eq!(vault.readiness(&letter).unwrap(), Readiness::NotReviewed);
    save_ready(&vault, &letter);
    assert_eq!(
        vault.text_for_ai(&letter).unwrap(),
        handoff::for_ai("PERSON_2 signed.", ["PERSON_2"])
    );
}

#[test]
fn a_project_that_does_not_open_fails_the_check() {
    let folder = Folder::new();
    let vault = folder.open();
    let project = project_with(&vault, &[person("PERSON_1", &["Anna Petrova"])]);
    let letter = document(&vault, Some(&project), "PERSON_1 signed.");
    save_ready(&vault, &letter);

    let path = folder.root.join("projects").join(format!("{project}.enc"));
    std::fs::write(&path, b"not sealed").unwrap();
    assert!(matches!(
        vault.readiness(&letter),
        Err(VaultError::Damaged { .. })
    ));
    std::fs::remove_file(&path).unwrap();
    assert!(matches!(
        vault.text_for_ai(&letter),
        Err(VaultError::NotFound { .. })
    ));
}
