//! The Vault through its public interface, on a temp folder with an
//! injected key (#1091, Testing Decisions, seam 1). Only the documented
//! layout (ADR 0125) is touched directly, to damage or replace a file.
use mdoc_vault::{
    Colour, Document, DocumentOrigin, Fault, FaultHook, ManifestCheck, NewDocument, NewProject,
    ObjectKind, Project, ProjectSummary, Saved, SourceKind, Stage, Vault, VaultError,
};
use std::{
    path::{Path, PathBuf},
    sync::{
        Arc,
        atomic::{AtomicUsize, Ordering},
    },
};

const KEY: [u8; 32] = [7; 32];
const OTHER_KEY: [u8; 32] = [9; 32];

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
    fn manifest(&self) -> PathBuf {
        self.root.join("manifest.enc")
    }
    fn project_file(&self, id: &str) -> PathBuf {
        self.root.join("projects").join(format!("{id}.enc"))
    }
    fn document_file(&self, id: &str) -> PathBuf {
        self.root.join("docs").join(id).join("doc.enc")
    }
}

fn project(vault: &Vault, name: &str) -> Project {
    vault
        .create_project(NewProject {
            name: name.into(),
            ..NewProject::default()
        })
        .unwrap()
}

fn document(vault: &Vault, project: Option<&str>, title: &str, markdown: &str) -> Document {
    vault
        .create_document(NewDocument {
            project: project.map(String::from),
            title: title.into(),
            source_kind: SourceKind::Pdf,
            origin: DocumentOrigin::Ordinary,
            markdown: markdown.into(),
        })
        .unwrap()
}

/// What a caller sees: Project names, and each listed Document's title,
/// text and Project name, each document read from its file.
#[derive(Clone, Debug, PartialEq)]
struct Snapshot {
    projects: Vec<String>,
    documents: Vec<(String, String, Option<String>)>,
}

fn snapshot(vault: &Vault) -> Snapshot {
    assert_eq!(vault.damaged(), vec![]);
    let mut projects: Vec<_> = vault.projects().into_iter().map(|p| p.name).collect();
    projects.sort();
    let mut documents: Vec<_> = vault
        .documents()
        .into_iter()
        .map(|summary| {
            let document = vault.document(&summary.id).unwrap();
            assert_eq!(summary.title, document.title);
            let project = document
                .project
                .as_deref()
                .map(|id| vault.project(id).unwrap().name);
            (document.title, document.markdown, project)
        })
        .collect();
    documents.sort();
    Snapshot {
        projects,
        documents,
    }
}

fn snapshot_of(folder: &Folder) -> Snapshot {
    snapshot(&folder.open())
}

#[test]
fn a_project_and_a_document_survive_a_reopen() {
    let folder = Folder::new();
    let (project, document) = {
        let vault = folder.open();
        let project = vault
            .create_project(NewProject {
                name: "Smith v Jones".into(),
                colour: Some(Colour::Teal),
                pinned: true,
            })
            .unwrap();
        let created = vault
            .create_document(NewDocument {
                project: Some(project.id.clone()),
                title: "Statement of claim".into(),
                source_kind: SourceKind::Spreadsheet,
                origin: DocumentOrigin::Sample,
                markdown: "# Claim\n\nPERSON_1 owes PERSON_2.".into(),
            })
            .unwrap();
        let document = vault
            .update_document(&created.id, |d| {
                d.scanned = true;
                d.exclude_from_ai = true;
                d.ready_fingerprint = Some("ab".repeat(32));
            })
            .unwrap();
        assert_eq!(document.revision, created.revision + 1);
        (project, document)
    };
    let vault = folder.open();
    assert_eq!(vault.opened(), ManifestCheck::Current);
    assert_eq!(vault.project(&project.id).unwrap(), project);
    assert_eq!(vault.document(&document.id).unwrap(), document);
    assert_eq!(vault.projects(), vec![ProjectSummary::of(&project)]);
    let listed = vault.documents_in(Some(&project.id));
    assert_eq!(listed.len(), 1);
    assert_eq!(listed[0].source_kind, SourceKind::Spreadsheet);
    assert!(listed[0].scanned && listed[0].exclude_from_ai);
    assert_eq!(vault.documents_in(None), vec![]);
}

#[test]
fn ids_are_random_32_hex_digits() {
    let folder = Folder::new();
    let vault = folder.open();
    let (a, b) = (project(&vault, "A"), project(&vault, "B"));
    let d = document(&vault, None, "D", "");
    for id in [&a.id, &b.id, &d.id] {
        assert_eq!(id.len(), 32);
        assert!(
            id.bytes()
                .all(|c| c.is_ascii_digit() || (b'a'..=b'f').contains(&c))
        );
    }
    assert!(a.id != b.id && b.id != d.id);
}

#[test]
fn records_are_sealed_and_bound_to_their_id_and_kind() {
    let folder = Folder::new();
    let (a, b, d) = {
        let vault = folder.open();
        let a = project(&vault, "Acme merger");
        let b = project(&vault, "Beta lease");
        let d = document(&vault, Some(&a.id), "Heads of terms", "Ann Lee signs.");
        (a, b, d)
    };
    for file in [folder.project_file(&a.id), folder.document_file(&d.id)] {
        let bytes = std::fs::read(file).unwrap();
        let text = String::from_utf8_lossy(&bytes);
        assert!(!text.contains("Acme") && !text.contains("Ann Lee"));
    }
    // A's file under B's id, and a Project file as a Document.
    std::fs::copy(folder.project_file(&a.id), folder.project_file(&b.id)).unwrap();
    let d_file = folder.document_file(&d.id);
    let d_bytes = std::fs::read(&d_file).unwrap();
    std::fs::copy(folder.project_file(&a.id), &d_file).unwrap();
    let vault = folder.open();
    assert!(matches!(
        vault.project(&b.id),
        Err(VaultError::Damaged { .. })
    ));
    assert!(matches!(
        vault.document(&d.id),
        Err(VaultError::Damaged { .. })
    ));
    let mut damaged = vault.damaged();
    damaged.sort_by_key(|(_, id)| id.clone());
    let mut want = vec![
        (ObjectKind::Project, b.id.clone()),
        (ObjectKind::Document, d.id.clone()),
    ];
    want.sort_by_key(|(_, id)| id.clone());
    assert_eq!(damaged, want);
    assert_eq!(vault.projects().len(), 1);
    drop(vault);
    // A flipped byte.
    let mut flipped = d_bytes.clone();
    *flipped.last_mut().unwrap() ^= 1;
    std::fs::write(&d_file, flipped).unwrap();
    let vault = folder.open();
    assert!(matches!(
        vault.document(&d.id),
        Err(VaultError::Damaged { .. })
    ));
    drop(vault);
    // Put right, it opens again.
    std::fs::write(&d_file, d_bytes).unwrap();
    let vault = folder.open();
    assert_eq!(vault.document(&d.id).unwrap().markdown, "Ann Lee signs.");
    assert_eq!(vault.damaged(), vec![(ObjectKind::Project, b.id.clone())]);
}

#[test]
fn a_missing_manifest_is_rebuilt() {
    let folder = Folder::new();
    {
        let vault = folder.open();
        let p = project(&vault, "Alpha");
        document(&vault, Some(&p.id), "Brief", "one");
        document(&vault, None, "Notes", "two");
    }
    let before = snapshot_of(&folder);
    std::fs::remove_file(folder.manifest()).unwrap();
    let vault = folder.open();
    assert_eq!(vault.opened(), ManifestCheck::Missing);
    assert_eq!(snapshot(&vault), before);
    drop(vault);
    assert_eq!(folder.open().opened(), ManifestCheck::Current);
}

#[test]
fn a_damaged_manifest_is_rebuilt() {
    let folder = Folder::new();
    {
        let vault = folder.open();
        let p = project(&vault, "Alpha");
        document(&vault, Some(&p.id), "Brief", "one");
    }
    let before = snapshot_of(&folder);
    let good = std::fs::read(folder.manifest()).unwrap();
    let mut flipped = good.clone();
    flipped[20] ^= 0x40;
    let damages: [Vec<u8>; 4] = [
        flipped,
        good[..good.len() / 2].to_vec(),
        Vec::new(),
        b"not a manifest".to_vec(),
    ];
    for damage in damages {
        std::fs::write(folder.manifest(), damage).unwrap();
        let vault = folder.open();
        assert_eq!(vault.opened(), ManifestCheck::Damaged);
        assert_eq!(snapshot(&vault), before);
    }
    // A manifest sealed under another key is damaged too.
    std::fs::remove_file(folder.manifest()).unwrap();
    {
        let other = Folder::new();
        drop(Vault::open(&other.root, &OTHER_KEY).unwrap());
        std::fs::copy(other.manifest(), folder.manifest()).unwrap();
    }
    let vault = folder.open();
    assert_eq!(vault.opened(), ManifestCheck::Damaged);
    assert_eq!(snapshot(&vault), before);
}

#[test]
fn a_stale_manifest_is_rebuilt() {
    let folder = Folder::new();
    let (alpha, brief) = {
        let vault = folder.open();
        let alpha = project(&vault, "Alpha");
        let brief = document(&vault, Some(&alpha.id), "Brief", "one");
        document(&vault, None, "Kept", "same");
        (alpha, brief)
    };
    let old = std::fs::read(folder.manifest()).unwrap();
    {
        let vault = folder.open();
        vault
            .update_project(&alpha.id, |p| p.name = "Alpha LLP".into())
            .unwrap();
        vault.delete_document(&brief.id).unwrap();
        document(&vault, Some(&alpha.id), "Reply", "three");
    }
    let after = snapshot_of(&folder);
    assert_eq!(after.projects, vec!["Alpha LLP".to_string()]);
    std::fs::write(folder.manifest(), old).unwrap();
    let vault = folder.open();
    assert_eq!(vault.opened(), ManifestCheck::Stale);
    assert_eq!(snapshot(&vault), after);
    drop(vault);
    assert_eq!(folder.open().opened(), ManifestCheck::Current);
}

/// The scripted changes the crash test interrupts; ids are found by name,
/// so the script runs the same in every folder.
fn run_step(vault: &Vault, step: usize) -> Result<(), VaultError> {
    let find_project = |name: &str| {
        vault
            .projects()
            .into_iter()
            .find(|p| p.name == name)
            .unwrap()
            .id
    };
    let find_document = |title: &str| {
        vault
            .documents()
            .into_iter()
            .find(|d| d.title == title)
            .unwrap()
            .id
    };
    let new = |project: Option<String>, title: &str, markdown: &str| NewDocument {
        project,
        title: title.into(),
        source_kind: SourceKind::Text,
        origin: DocumentOrigin::Ordinary,
        markdown: markdown.into(),
    };
    match step {
        0 => {
            vault.create_project(NewProject {
                name: "Alpha".into(),
                ..NewProject::default()
            })?;
        }
        1 => {
            vault.create_document(new(Some(find_project("Alpha")), "Brief", "one"))?;
        }
        2 => {
            vault.update_document(&find_document("Brief"), |d| d.markdown = "two".into())?;
        }
        3 => {
            vault.update_project(&find_project("Alpha"), |p| p.name = "Alpha LLP".into())?;
        }
        4 => {
            vault.create_document(new(None, "Notes", "three"))?;
        }
        5 => vault.delete_document(&find_document("Brief"))?,
        _ => unreachable!(),
    }
    vault.flush()
}
const STEPS: usize = 6;

#[test]
fn a_crash_between_two_writes_loses_nothing() {
    // What each step leaves, from a run with no crash.
    let mut expected = Vec::new();
    {
        let folder = Folder::new();
        let vault = folder.open();
        expected.push(snapshot(&vault));
        for step in 0..STEPS {
            run_step(&vault, step).unwrap();
            expected.push(snapshot(&vault));
        }
    }
    // Crash at the 1st write step, then the 2nd, and so on, until a run
    // ends with no crash.
    let (mut before, mut after) = (0, 0);
    for crash_at in 1.. {
        let folder = Folder::new();
        let calls = Arc::new(AtomicUsize::new(0));
        let hook: FaultHook = {
            let calls = calls.clone();
            Arc::new(move |_step| {
                if calls.fetch_add(1, Ordering::SeqCst) + 1 == crash_at {
                    Fault::Crash
                } else {
                    Fault::Continue
                }
            })
        };
        let in_flight = match Vault::open_with_hook(&folder.root, &KEY, hook) {
            Err(VaultError::Crashed) => Some(0),
            Err(other) => panic!("{other}"),
            Ok(vault) => {
                let mut crashed = None;
                for step in 0..STEPS {
                    match run_step(&vault, step) {
                        Ok(()) => {}
                        Err(VaultError::Crashed) => {
                            crashed = Some(step);
                            break;
                        }
                        Err(other) => panic!("{other}"),
                    }
                }
                assert!(
                    matches!(
                        vault.create_project(NewProject::default()),
                        Err(VaultError::Crashed)
                    ) || crashed.is_none()
                );
                crashed
            }
        };
        let Some(step) = in_flight else {
            assert!(crash_at > STEPS * 2, "every step writes twice");
            break;
        };
        let vault = folder.open();
        let now = snapshot(&vault);
        if now == expected[step] {
            before += 1;
        } else {
            assert_eq!(
                now,
                expected[step + 1],
                "crash at write step {crash_at}, in step {step}"
            );
            after += 1;
        }
        drop(vault);
        let reopened = folder.open();
        assert_eq!(reopened.opened(), ManifestCheck::Current);
        assert_eq!(snapshot(&reopened), now);
    }
    // Crashes landed both before and after a step's record was written.
    assert!(
        before >= STEPS && after >= STEPS,
        "{before} before, {after} after"
    );
}

#[test]
fn a_crash_before_a_rename_keeps_the_last_revision() {
    let folder = Folder::new();
    let id = {
        let vault = folder.open();
        document(&vault, None, "Brief", "kept").id
    };
    let hook: FaultHook = Arc::new(|step| {
        if step.kind == ObjectKind::Document && step.stage == Stage::BeforeRename {
            Fault::Crash
        } else {
            Fault::Continue
        }
    });
    let vault = Vault::open_with_hook(&folder.root, &KEY, hook).unwrap();
    let result = vault.update_document(&id, |d| d.markdown = "lost".into());
    assert!(matches!(result, Err(VaultError::Crashed)));
    assert!(matches!(vault.flush(), Err(VaultError::Crashed)));
    drop(vault);
    let folder_files = |dir: &Path| std::fs::read_dir(dir).unwrap().count();
    let doc_dir = folder.root.join("docs").join(&id);
    assert_eq!(
        folder_files(&doc_dir),
        2,
        "the record and the dead write's temp file"
    );
    let vault = folder.open();
    assert_eq!(vault.document(&id).unwrap().markdown, "kept");
    assert_eq!(folder_files(&doc_dir), 1, "the temp file is swept at open");
}

#[test]
fn a_second_handle_sees_the_first_handles_writes() {
    let folder = Folder::new();
    let first = folder.open();
    let second = folder.open();
    let p = project(&first, "Alpha");
    let d = document(&first, Some(&p.id), "Brief", "one");
    // Records are read from their files.
    assert_eq!(second.document(&d.id).unwrap(), d);
    // Lists come from the cache until a refresh checks it.
    assert!(second.refresh().unwrap());
    assert_eq!(snapshot(&second), snapshot(&first));
    assert!(!second.refresh().unwrap());
    let changed = second
        .update_document(&d.id, |d| d.markdown = "two".into())
        .unwrap();
    assert_eq!(changed.revision, d.revision + 1);
    let q = project(&second, "Beta");
    assert!(first.refresh().unwrap());
    assert_eq!(first.document(&d.id).unwrap().markdown, "two");
    assert!(first.projects().iter().any(|p| p.id == q.id));
    // Each handle's manifest takes in the other's writes.
    second.flush().unwrap();
    drop(second);
    drop(first);
    let vault = folder.open();
    assert_eq!(vault.opened(), ManifestCheck::Current);
    assert_eq!(vault.projects().len(), 2);
}

#[test]
fn a_handle_writes_a_manifest_with_the_other_handles_objects() {
    let folder = Folder::new();
    let first = folder.open();
    let second = folder.open();
    let p = project(&first, "Alpha");
    drop(first);
    // The second handle never refreshed, yet its manifest lists Alpha.
    let q = project(&second, "Beta");
    second.flush().unwrap();
    drop(second);
    let vault = folder.open();
    assert_eq!(vault.opened(), ManifestCheck::Current);
    let mut ids: Vec<_> = vault.projects().into_iter().map(|p| p.id).collect();
    ids.sort();
    let mut want = vec![p.id, q.id];
    want.sort();
    assert_eq!(ids, want);
}

#[test]
fn two_handles_writing_at_once_lose_no_change() {
    let folder = Folder::new();
    let id = {
        let vault = folder.open();
        document(&vault, None, "Log", "").id
    };
    let handles: Vec<_> = ['a', 'b']
        .into_iter()
        .map(|letter| {
            let root = folder.root.clone();
            let id = id.clone();
            std::thread::spawn(move || {
                let vault = Vault::open(root, &KEY).unwrap();
                for _ in 0..20 {
                    vault
                        .update_document(&id, |d| d.markdown.push(letter))
                        .unwrap();
                }
            })
        })
        .collect();
    for handle in handles {
        handle.join().unwrap();
    }
    let document = folder.open().document(&id).unwrap();
    assert_eq!(document.markdown.matches('a').count(), 20);
    assert_eq!(document.markdown.matches('b').count(), 20);
    assert_eq!(document.revision, 41);
}

#[test]
fn a_handle_never_writes_an_older_revision() {
    let folder = Folder::new();
    let vault = folder.open();
    let first = document(&vault, None, "Brief", "one");
    let second = vault
        .update_document(&first.id, |d| d.markdown = "two".into())
        .unwrap();
    let mut stale = first.clone();
    stale.markdown = "stale".into();
    assert_eq!(vault.save_document(&stale).unwrap(), Saved::Skipped);
    assert_eq!(vault.document(&first.id).unwrap().markdown, "two");
    let mut current = second.clone();
    current.markdown = "three".into();
    current.revision += 1;
    assert_eq!(vault.save_document(&current).unwrap(), Saved::Written);
    assert_eq!(vault.document(&first.id).unwrap().markdown, "three");
    // Updates go past the last revision written.
    let next = vault.update_document(&first.id, |_| {}).unwrap();
    assert_eq!(next.revision, current.revision + 1);
    let p = project(&vault, "Alpha");
    let renamed = vault
        .update_project(&p.id, |p| p.name = "Beta".into())
        .unwrap();
    assert_eq!(vault.save_project(&p).unwrap(), Saved::Skipped);
    assert_eq!(vault.project(&p.id).unwrap().name, renamed.name);
}

#[test]
fn a_stale_copy_from_another_handle_is_not_saved() {
    let folder = Folder::new();
    let first = folder.open();
    let second = folder.open();
    let d = document(&first, None, "Brief", "one");
    let stale = second.document(&d.id).unwrap();
    first
        .update_document(&d.id, |d| d.markdown = "two".into())
        .unwrap();
    let mut edited = stale.clone();
    edited.markdown = "stale".into();
    assert_eq!(second.save_document(&edited).unwrap(), Saved::Skipped);
    let stored = second.document(&d.id).unwrap();
    assert_eq!((stored.markdown.as_str(), stored.revision), ("two", 2));
}

#[cfg(unix)]
#[test]
fn a_folder_that_cannot_be_listed_fails_the_open_and_keeps_the_manifest() {
    use std::os::unix::fs::PermissionsExt;
    let folder = Folder::new();
    {
        let vault = folder.open();
        project(&vault, "Alpha");
    }
    let manifest = std::fs::read(folder.manifest()).unwrap();
    let projects = folder.root.join("projects");
    std::fs::set_permissions(&projects, std::fs::Permissions::from_mode(0o000)).unwrap();
    let listable = std::fs::read_dir(&projects).is_ok();
    let opened = Vault::open(&folder.root, &KEY);
    std::fs::set_permissions(&projects, std::fs::Permissions::from_mode(0o755)).unwrap();
    if listable {
        return; // Running as root: permissions do not apply.
    }
    assert!(matches!(opened, Err(VaultError::Io(_))));
    assert_eq!(std::fs::read(folder.manifest()).unwrap(), manifest);
    assert_eq!(folder.open().projects().len(), 1);
}

#[test]
fn a_save_or_update_keeps_the_stored_project() {
    let folder = Folder::new();
    let vault = folder.open();
    let p = project(&vault, "Alpha");
    let d = document(&vault, None, "Brief", "one");
    let mut moved = d.clone();
    moved.project = Some(p.id.clone());
    assert_eq!(vault.save_document(&moved).unwrap(), Saved::Written);
    let updated = vault
        .update_document(&d.id, |d| d.project = Some(p.id.clone()))
        .unwrap();
    assert_eq!(updated.project, None);
    assert_eq!(vault.document(&d.id).unwrap().project, None);
    assert_eq!(vault.documents_in(Some(&p.id)), vec![]);
}

#[test]
fn a_deleted_document_stays_deleted() {
    let folder = Folder::new();
    let d = {
        let vault = folder.open();
        let d = document(&vault, None, "Brief", "one");
        vault.delete_document(&d.id).unwrap();
        assert!(matches!(
            vault.document(&d.id),
            Err(VaultError::NotFound {
                kind: ObjectKind::Document,
                ..
            })
        ));
        assert_eq!(vault.save_document(&d).unwrap(), Saved::Skipped);
        assert!(matches!(
            vault.update_document(&d.id, |_| {}),
            Err(VaultError::NotFound { .. })
        ));
        assert_eq!(vault.documents(), vec![]);
        d
    };
    let vault = folder.open();
    assert!(matches!(
        vault.document(&d.id),
        Err(VaultError::NotFound { .. })
    ));
    assert!(!folder.root.join("docs").join(&d.id).exists());
}

#[test]
fn the_wrong_key_opens_nothing_and_writes_nothing() {
    let folder = Folder::new();
    let p = {
        let vault = folder.open();
        project(&vault, "Alpha")
    };
    let manifest = std::fs::read(folder.manifest()).unwrap();
    assert!(matches!(
        Vault::open(&folder.root, &OTHER_KEY),
        Err(VaultError::WrongKey)
    ));
    assert_eq!(std::fs::read(folder.manifest()).unwrap(), manifest);
    // Without its manifest, the objects alone tell a wrong key.
    std::fs::remove_file(folder.manifest()).unwrap();
    assert!(matches!(
        Vault::open(&folder.root, &OTHER_KEY),
        Err(VaultError::WrongKey)
    ));
    assert!(!folder.manifest().exists());
    assert_eq!(folder.open().project(&p.id).unwrap().name, "Alpha");
}

#[test]
fn a_document_goes_only_into_an_existing_project() {
    let folder = Folder::new();
    let vault = folder.open();
    let result = vault.create_document(NewDocument {
        project: Some("0".repeat(32)),
        title: "Brief".into(),
        source_kind: SourceKind::Word,
        origin: DocumentOrigin::New,
        markdown: String::new(),
    });
    assert!(matches!(
        result,
        Err(VaultError::NotFound {
            kind: ObjectKind::Project,
            ..
        })
    ));
    assert_eq!(vault.documents(), vec![]);
}

#[test]
fn ids_that_are_not_vault_ids_are_refused() {
    let folder = Folder::new();
    let vault = folder.open();
    let upper = "A".repeat(32);
    for id in ["../manifest", "", upper.as_str()] {
        assert!(matches!(vault.project(id), Err(VaultError::InvalidId)));
        assert!(matches!(vault.document(id), Err(VaultError::InvalidId)));
        assert!(matches!(
            vault.delete_document(id),
            Err(VaultError::InvalidId)
        ));
    }
}

#[test]
fn source_kinds_and_origins_are_stored_as_lowercase_strings() {
    let kinds = [
        (SourceKind::Pdf, "pdf"),
        (SourceKind::Word, "word"),
        (SourceKind::Spreadsheet, "spreadsheet"),
        (SourceKind::Presentation, "presentation"),
        (SourceKind::Ebook, "ebook"),
        (SourceKind::Image, "image"),
        (SourceKind::Scan, "scan"),
        (SourceKind::Markdown, "markdown"),
        (SourceKind::Text, "text"),
    ];
    for (kind, text) in kinds {
        let json = format!("\"{text}\"");
        assert_eq!(serde_json::to_string(&kind).unwrap(), json);
        assert_eq!(serde_json::from_str::<SourceKind>(&json).unwrap(), kind);
    }
    let origins = [
        (DocumentOrigin::Ordinary, "\"ordinary\""),
        (DocumentOrigin::New, "\"new\""),
        (DocumentOrigin::Sample, "\"sample\""),
    ];
    for (origin, json) in origins {
        assert_eq!(serde_json::to_string(&origin).unwrap(), json);
    }
}

#[test]
fn a_document_folder_without_its_record_is_ignored() {
    let folder = Folder::new();
    let kept = {
        let vault = folder.open();
        document(&vault, None, "Brief", "one")
    };
    // What a crash between an original's write and its record's leaves.
    let orphan = "f".repeat(32);
    let orphan_dir = folder.root.join("docs").join(&orphan);
    std::fs::create_dir_all(&orphan_dir).unwrap();
    std::fs::write(orphan_dir.join("original.enc"), b"sealed source").unwrap();
    let vault = folder.open();
    assert_eq!(vault.opened(), ManifestCheck::Current);
    assert_eq!(vault.damaged(), vec![]);
    let ids: Vec<_> = vault.documents().into_iter().map(|d| d.id).collect();
    assert_eq!(ids, vec![kept.id]);
    assert!(matches!(
        vault.document(&orphan),
        Err(VaultError::NotFound { .. })
    ));
    drop(vault);
    std::fs::remove_file(folder.manifest()).unwrap();
    let vault = folder.open();
    assert_eq!(vault.opened(), ManifestCheck::Missing);
    assert_eq!(vault.documents().len(), 1);
    assert!(
        orphan_dir.join("original.enc").exists(),
        "an orphan is left for its owner"
    );
}

#[test]
fn projects_list_pinned_first_then_most_recently_updated() {
    let folder = Folder::new();
    let vault = folder.open();
    project(&vault, "A");
    vault
        .create_project(NewProject {
            name: "B".into(),
            pinned: true,
            ..NewProject::default()
        })
        .unwrap();
    project(&vault, "C");
    let listed = vault.projects();
    assert_eq!(listed[0].name, "B");
    // Times are in seconds: equal ones sort by name.
    let rest: Vec<_> = listed[1..]
        .iter()
        .map(|p| (p.updated, p.name.as_str()))
        .collect();
    assert!(
        rest == [(rest[0].0, "A"), (rest[0].0, "C")] || (rest[0].1 == "C" && rest[0].0 > rest[1].0),
        "{rest:?}"
    );
    assert_eq!(Colour::for_id(&"0".repeat(32)), Colour::Red);
}

#[test]
fn a_documents_review_survives_a_reopen_and_restores_over_its_text() {
    use mdoc_pii::{Category, Detection, Recognizer, Review};
    let folder = Folder::new();
    let markdown = "Ann met Bob.";
    let saved = {
        let mut review = Review::default();
        let detections = ["Ann", "Bob"]
            .map(|name| {
                let start = markdown.find(name).unwrap();
                Detection {
                    range: start..start + name.len(),
                    category: Category::Person,
                    score: 0.9,
                    recognizer: Recognizer::Model,
                }
            })
            .to_vec();
        review.ingest(markdown, detections).unwrap();
        review.save()
    };
    let id = {
        let vault = folder.open();
        let created = document(&vault, None, "Memo", markdown);
        assert_eq!(created.review, None);
        vault
            .update_document(&created.id, |d| d.review = Some(saved.clone()))
            .unwrap()
            .id
    };
    let reopened = folder.open().document(&id).unwrap();
    assert_eq!(reopened.review.as_ref(), Some(&saved));
    let restored = Review::restore(reopened.review.unwrap(), &reopened.markdown, 0).unwrap();
    assert_eq!(restored.save(), saved);
}

#[test]
fn a_document_without_a_review_is_stored_without_the_field() {
    let folder = Folder::new();
    let vault = folder.open();
    let created = document(&vault, None, "Memo", "Text.");
    let json = serde_json::to_value(&created).unwrap();
    assert!(json.get("review").is_none());
    let read: Document = serde_json::from_value(json).unwrap();
    assert_eq!(read.review, None);
}
