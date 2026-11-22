//! AI Replies through the Vault's public interface (#1091, Testing
//! Decisions, seam 1: a reply that still restores correctly after a move,
//! and two handles on one folder): a reply keeps its text as received and
//! the identity each alias meant, and restores from those identities'
//! originals now (Decision 18).
use mdoc_pii::{Category, restore::ExportedIdentity};
use mdoc_vault::{
    Binding, DocumentOrigin, Fault, FaultHook, Mapping, NewDocument, NewProject, NewReply,
    ObjectKind, Project, ProjectDocuments, Reply, ReplyOwner, SourceKind, Stage, Vault, VaultError,
};
use std::{path::PathBuf, sync::Arc};

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

fn mapping(exported: &[ExportedIdentity]) -> Mapping {
    let mut mapping = Mapping::default();
    mapping.merge(exported);
    mapping
}

fn new_project(vault: &Vault, name: &str, names: &[ExportedIdentity]) -> Project {
    let project = vault
        .create_project(NewProject {
            name: name.into(),
            ..NewProject::default()
        })
        .unwrap();
    vault
        .update_project(&project.id, |p| p.mapping.merge(names))
        .unwrap()
}

fn new_document(vault: &Vault, project: Option<&str>, markdown: &str) -> String {
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

/// An Unfiled document with its own names.
fn unfiled(vault: &Vault, markdown: &str, names: &[ExportedIdentity]) -> String {
    let document = new_document(vault, None, markdown);
    let local = mapping(names);
    vault
        .update_document(&document, |d| d.mapping = local)
        .unwrap();
    document
}

fn save(vault: &Vault, owner: ReplyOwner, markdown: &str) -> Reply {
    vault
        .save_reply(NewReply {
            owner,
            title: "Reply".into(),
            source_app: Some("Claude".into()),
            markdown: markdown.into(),
        })
        .unwrap()
}

fn restored(vault: &Vault, reply: &str) -> String {
    vault.restore_reply(reply).unwrap().text
}

fn ids(replies: &[mdoc_vault::ReplySummary]) -> Vec<String> {
    replies.iter().map(|r| r.id.clone()).collect()
}

fn files(dir: &std::path::Path) -> Vec<PathBuf> {
    std::fs::read_dir(dir)
        .map(|entries| entries.flatten().map(|e| e.path()).collect())
        .unwrap_or_default()
}

/// A reply keeps its text as received and binds each alias it uses, the
/// reshaped ones too; one naming nobody stays unresolved. It is sealed and
/// survives a reopen.
#[test]
fn a_reply_keeps_its_text_and_the_identity_each_alias_meant() {
    let folder = Folder::new();
    let vault = folder.open();
    let project = new_project(
        &vault,
        "Ivanov",
        &[
            person("PERSON_1", &["Anna Ivanova"]),
            person("PERSON_2", &["Boris Petrov"]),
        ],
    );
    let (anna, boris) = (
        project
            .mapping
            .identity_named("PERSON_1")
            .unwrap()
            .id
            .clone(),
        project
            .mapping
            .identity_named("PERSON_2")
            .unwrap()
            .id
            .clone(),
    );
    let text = "Person 1 wrote to PERSON_2; PERSON_9 and PERSON_2 agree.";
    let reply = save(&vault, ReplyOwner::Project(project.id.clone()), text);
    assert_eq!(reply.markdown, text, "as received");
    assert_eq!(reply.source_app.as_deref(), Some("Claude"));
    let bound = |alias: &str, identity: &str| Binding {
        alias: alias.into(),
        identity: identity.into(),
        custom: false,
    };
    assert_eq!(
        reply.bindings,
        [bound("PERSON_1", &anna), bound("PERSON_2", &boris)]
    );
    let restored = vault.restore_reply(&reply.id).unwrap();
    assert_eq!(
        restored.text,
        "Anna Ivanova wrote to Boris Petrov; PERSON_9 and Boris Petrov agree."
    );
    assert_eq!(restored.unresolved.len(), 1);
    // Sealed: neither the text nor the names are on disk in the clear.
    let replies = files(&folder.root.join("replies"));
    assert_eq!(replies.len(), 1);
    let bytes = std::fs::read(&replies[0]).unwrap();
    for plain in [b"PERSON_9".as_slice(), b"Claude", anna.as_bytes()] {
        assert!(!bytes.windows(plain.len()).any(|w| w == plain));
    }
    drop(vault);
    let vault = folder.open();
    assert_eq!(vault.reply(&reply.id).unwrap(), reply);
    assert_eq!(ids(&vault.replies()), std::slice::from_ref(&reply.id));
    assert_eq!(ids(&vault.project_replies(&project.id)), [reply.id]);
}

/// The item's first Done-when: a reply saved on an Unfiled document follows
/// it into a Project whose aliases mean other people, and on into another
/// Project and back, and restores the same people each time.
#[test]
fn a_reply_restores_the_same_people_after_its_document_moves() {
    let folder = Folder::new();
    let vault = folder.open();
    let note = unfiled(
        &vault,
        "PERSON_1 wrote to PERSON_2.",
        &[
            person("PERSON_1", &["Ann Lee"]),
            person("PERSON_2", &["Bob Hart"]),
        ],
    );
    let reply = save(
        &vault,
        ReplyOwner::Document(note.clone()),
        "PERSON_1 thanks PERSON_2.",
    );
    assert_eq!(restored(&vault, &reply.id), "Ann Lee thanks Bob Hart.");
    assert_eq!(
        ids(&vault.document_replies(&note)),
        std::slice::from_ref(&reply.id)
    );
    // In the Memo, PERSON_1 is Bob and PERSON_2 is Carl.
    let memo = new_project(
        &vault,
        "Memo",
        &[
            person("PERSON_1", &["Bob Hart"]),
            person("PERSON_2", &["Carl Diaz"]),
        ],
    );
    let moved = vault.move_document(&note, Some(&memo.id)).unwrap();
    assert_eq!(moved.document.markdown, "PERSON_3 wrote to PERSON_1.");
    assert_eq!(restored(&vault, &reply.id), "Ann Lee thanks Bob Hart.");
    assert_eq!(
        ids(&vault.project_replies(&memo.id)),
        std::slice::from_ref(&reply.id)
    );
    // On into the Brief, where PERSON_1 is Ann and PERSON_2 is Dan.
    let brief = new_project(
        &vault,
        "Brief",
        &[
            person("PERSON_1", &["Ann Lee"]),
            person("PERSON_2", &["Dan Moss"]),
        ],
    );
    let onward = vault.move_document(&note, Some(&brief.id)).unwrap();
    assert_eq!(onward.document.markdown, "PERSON_1 wrote to PERSON_4.");
    assert_eq!(restored(&vault, &reply.id), "Ann Lee thanks Bob Hart.");
    assert!(vault.project_replies(&memo.id).is_empty());
    assert_eq!(
        ids(&vault.project_replies(&brief.id)),
        std::slice::from_ref(&reply.id)
    );
    // Back to Unfiled, and into the Memo again and undone: the same people
    // still.
    vault.move_document(&note, None).unwrap();
    assert_eq!(restored(&vault, &reply.id), "Ann Lee thanks Bob Hart.");
    let undo = vault.move_document(&note, Some(&memo.id)).unwrap().undo;
    assert_eq!(restored(&vault, &reply.id), "Ann Lee thanks Bob Hart.");
    vault.undo_move(&undo).unwrap();
    assert_eq!(restored(&vault, &reply.id), "Ann Lee thanks Bob Hart.");
    drop(vault);
    assert_eq!(
        restored(&folder.open(), &reply.id),
        "Ann Lee thanks Bob Hart."
    );
}

/// A reply saved for a document while it is in a Project follows it out to
/// Unfiled, and one saved for the Project stays with it.
#[test]
fn a_reply_saved_in_a_project_restores_after_its_document_leaves() {
    let folder = Folder::new();
    let vault = folder.open();
    let memo = new_project(
        &vault,
        "Memo",
        &[
            person("PERSON_1", &["Bob Hart"]),
            person("PERSON_2", &["Carl Diaz"]),
        ],
    );
    let letter = new_document(&vault, Some(&memo.id), "PERSON_1 met PERSON_2.");
    let followed = save(&vault, ReplyOwner::Document(letter.clone()), "PERSON_2.");
    let stays = save(&vault, ReplyOwner::Project(memo.id.clone()), "PERSON_2.");
    let brief = new_project(&vault, "Brief", &[person("PERSON_1", &["Carl Diaz"])]);
    vault.move_document(&letter, Some(&brief.id)).unwrap();
    vault.move_document(&letter, None).unwrap();
    assert_eq!(restored(&vault, &followed.id), "Carl Diaz.");
    assert_eq!(restored(&vault, &stays.id), "Carl Diaz.");
    assert_eq!(ids(&vault.project_replies(&memo.id)), [stays.id]);
    assert_eq!(ids(&vault.document_replies(&letter)), [followed.id]);
}

/// A merge in the Project folds one identity into another; a reply bound
/// to either restores with the one left, and an alias the reply never
/// bound is not read with whoever has it now.
#[test]
fn a_reply_restores_the_same_people_after_a_merge() {
    let folder = Folder::new();
    let vault = folder.open();
    let project = new_project(
        &vault,
        "Bennett",
        &[
            person("PERSON_1", &["Maya Bennett"]),
            person("PERSON_2", &["Ms Bennett"]),
        ],
    );
    let reply = save(
        &vault,
        ReplyOwner::Project(project.id.clone()),
        "PERSON_2 and PERSON_1 and PERSON_3.",
    );
    // A document merged PERSON_2 into PERSON_1.
    let mut merged = person("PERSON_1", &["Maya Bennett", "Ms Bennett"]);
    merged.previous = vec!["PERSON_2".into()];
    let project = vault
        .update_project(&project.id, |p| {
            p.mapping.merge(std::slice::from_ref(&merged));
            p.mapping.merge(&[person("PERSON_3", &["Tom Reed"])]);
        })
        .unwrap();
    assert_eq!(project.mapping.identities.len(), 2);
    assert_eq!(
        restored(&vault, &reply.id),
        "Maya Bennett and Maya Bennett and PERSON_3."
    );
}

/// A forgotten identity stops restoring at once, in a Project and after a
/// move; the reply keeps its text.
#[test]
fn a_forgotten_identity_stops_restoring() {
    let folder = Folder::new();
    let vault = folder.open();
    let project = new_project(
        &vault,
        "Ivanov",
        &[
            person("PERSON_1", &["Anna Ivanova"]),
            person("PERSON_2", &["Boris Petrov"]),
        ],
    );
    let reply = save(
        &vault,
        ReplyOwner::Project(project.id.clone()),
        "PERSON_1 and PERSON_2.",
    );
    vault
        .update_project(&project.id, |p| assert!(p.forget_identity("PERSON_1")))
        .unwrap();
    let restored_now = vault.restore_reply(&reply.id).unwrap();
    assert_eq!(restored_now.text, "PERSON_1 and Boris Petrov.");
    assert_eq!(restored_now.unresolved.len(), 1);
    assert_eq!(vault.reply(&reply.id).unwrap().markdown, reply.markdown);
    // A name moved in from Unfiled and forgotten there.
    let note = unfiled(&vault, "PERSON_1.", &[person("PERSON_1", &["Ann Lee"])]);
    let moved_reply = save(&vault, ReplyOwner::Document(note.clone()), "PERSON_1.");
    let memo = new_project(&vault, "Memo", &[person("PERSON_1", &["Bob Hart"])]);
    let moved = vault.move_document(&note, Some(&memo.id)).unwrap();
    assert_eq!(moved.document.markdown, "PERSON_2.");
    vault
        .update_project(&memo.id, |p| assert!(p.forget_identity("PERSON_2")))
        .unwrap();
    assert_eq!(restored(&vault, &moved_reply.id), "PERSON_1.");
}

/// The app and the Restore extension: a reply one handle saves is picked
/// up by the other through the stamp check, and saving it wrote no
/// manifest (Decision 20).
#[test]
fn a_reply_saved_by_a_second_handle_is_picked_up_by_the_first() {
    let folder = Folder::new();
    let app = folder.open();
    let project = new_project(&app, "Ivanov", &[person("PERSON_1", &["Anna Ivanova"])]);
    app.flush().unwrap();
    let extension = folder.open();
    let manifest = folder.root.join("manifest.enc");
    let before = std::fs::read(&manifest).unwrap();
    let reply = save(
        &extension,
        ReplyOwner::Project(project.id.clone()),
        "PERSON_1 agrees.",
    );
    assert_eq!(
        std::fs::read(&manifest).unwrap(),
        before,
        "no manifest write"
    );
    assert!(app.replies().is_empty());
    assert!(app.refresh().unwrap());
    assert_eq!(
        ids(&app.project_replies(&project.id)),
        std::slice::from_ref(&reply.id)
    );
    assert_eq!(restored(&app, &reply.id), "Anna Ivanova agrees.");
    drop(extension);
    assert_eq!(std::fs::read(&manifest).unwrap(), before, "nor on drop");
    // A fresh handle finds it too.
    drop(app);
    assert_eq!(ids(&folder.open().replies()), [reply.id]);
}

/// A reply needs an owner that exists, and refuses bad ids.
#[test]
fn a_reply_for_a_missing_owner_is_refused() {
    let folder = Folder::new();
    let vault = folder.open();
    let missing = "0".repeat(32);
    for owner in [
        ReplyOwner::Project(missing.clone()),
        ReplyOwner::Document(missing.clone()),
    ] {
        let refused = vault.save_reply(NewReply {
            owner,
            title: "Reply".into(),
            source_app: None,
            markdown: "PERSON_1".into(),
        });
        assert!(matches!(refused, Err(VaultError::NotFound { .. })));
    }
    let refused = vault.save_reply(NewReply {
        owner: ReplyOwner::Project("nope".into()),
        title: "Reply".into(),
        source_app: None,
        markdown: String::new(),
    });
    assert!(matches!(refused, Err(VaultError::InvalidId)));
    assert!(matches!(vault.reply("nope"), Err(VaultError::InvalidId)));
    assert!(matches!(
        vault.reply(&missing),
        Err(VaultError::NotFound {
            kind: ObjectKind::Reply,
            ..
        })
    ));
    assert!(vault.replies().is_empty());
}

/// Deleting a reply's owner deletes it, as a case's replies went with the
/// case and a member's with the member; a Document's replies follow it to
/// Unfiled when its Project is deleted. Replies another handle saved go
/// too.
#[test]
fn deleting_an_owner_deletes_its_replies() {
    let folder = Folder::new();
    let vault = folder.open();
    let other = folder.open();
    let note = unfiled(&vault, "PERSON_1.", &[person("PERSON_1", &["Ann Lee"])]);
    let note_reply = save(&other, ReplyOwner::Document(note.clone()), "PERSON_1.");
    vault.delete_document(&note).unwrap();
    assert!(matches!(
        vault.reply(&note_reply.id),
        Err(VaultError::NotFound { .. })
    ));
    let names = [person("PERSON_1", &["Bob Hart"])];
    for mode in [ProjectDocuments::MoveToUnfiled, ProjectDocuments::Delete] {
        let project = new_project(&vault, "Memo", &names);
        let letter = new_document(&vault, Some(&project.id), "PERSON_1.");
        let own = save(&other, ReplyOwner::Project(project.id.clone()), "PERSON_1.");
        let letters = save(&vault, ReplyOwner::Document(letter.clone()), "PERSON_1.");
        vault.delete_project(&project.id, mode).unwrap();
        assert!(vault.reply(&own.id).is_err());
        match mode {
            ProjectDocuments::MoveToUnfiled => {
                assert_eq!(restored(&vault, &letters.id), "Bob Hart.");
                assert_eq!(ids(&vault.document_replies(&letter)), [letters.id]);
                vault.delete_document(&letter).unwrap();
            }
            ProjectDocuments::Delete => assert!(vault.reply(&letters.id).is_err()),
        }
    }
    assert!(vault.replies().is_empty());
    assert!(files(&folder.root.join("replies")).is_empty());
    vault.flush().unwrap();
    drop((vault, other));
    assert!(folder.open().replies().is_empty());
}

#[test]
fn a_deleted_reply_is_gone() {
    let folder = Folder::new();
    let vault = folder.open();
    let project = new_project(&vault, "Memo", &[]);
    let reply = save(&vault, ReplyOwner::Project(project.id.clone()), "Hello.");
    vault.delete_reply(&reply.id).unwrap();
    assert!(vault.replies().is_empty());
    assert!(vault.reply(&reply.id).is_err());
    assert!(files(&folder.root.join("replies")).is_empty());
}

/// A crash saving a reply leaves it whole or absent, and the next open
/// sweeps what the crash left.
#[test]
fn a_crash_saving_a_reply_leaves_it_whole_or_absent() {
    for stage in [Stage::BeforeRename, Stage::AfterRename] {
        let folder = Folder::new();
        let project = {
            let vault = folder.open();
            let project = new_project(&vault, "Memo", &[person("PERSON_1", &["Bob Hart"])]);
            vault.flush().unwrap();
            project
        };
        let hook: FaultHook = Arc::new(move |step| {
            if step.kind == ObjectKind::Reply && step.stage == stage {
                Fault::Crash
            } else {
                Fault::Continue
            }
        });
        let vault = Vault::open_with_hook(&folder.root, &KEY, hook).unwrap();
        let crashed = vault.save_reply(NewReply {
            owner: ReplyOwner::Project(project.id.clone()),
            title: "Reply".into(),
            source_app: None,
            markdown: "PERSON_1.".into(),
        });
        assert!(matches!(crashed, Err(VaultError::Crashed)));
        drop(vault);
        let vault = folder.open();
        let replies = vault.replies();
        match stage {
            Stage::BeforeRename => assert!(replies.is_empty()),
            Stage::AfterRename => {
                assert_eq!(replies.len(), 1);
                assert_eq!(restored(&vault, &replies[0].id), "Bob Hart.");
            }
        }
        assert_eq!(files(&folder.root.join("replies")).len(), replies.len());
    }
}

/// A reply file placed under another id does not open and is listed as
/// damaged, never restored.
#[test]
fn a_reply_file_under_another_id_does_not_open() {
    let folder = Folder::new();
    let vault = folder.open();
    let project = new_project(&vault, "Memo", &[person("PERSON_1", &["Bob Hart"])]);
    let reply = save(&vault, ReplyOwner::Project(project.id), "PERSON_1.");
    let dir = folder.root.join("replies");
    let other = "f".repeat(32);
    std::fs::copy(
        dir.join(format!("{}.enc", reply.id)),
        dir.join(format!("{other}.enc")),
    )
    .unwrap();
    assert!(vault.refresh().unwrap());
    assert!(matches!(
        vault.reply(&other),
        Err(VaultError::Damaged {
            kind: ObjectKind::Reply,
            ..
        })
    ));
    assert_eq!(vault.damaged(), [(ObjectKind::Reply, other)]);
    assert_eq!(ids(&vault.replies()), [reply.id]);
}
