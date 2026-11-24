//! Document notes and Promote to Document through the crate's public
//! interface (#1091 Decision 3), on temp folders with an injected key:
//! notes are sealed with their Document and never leave for an AI, Export
//! Project writes them beside it, and a saved AI Reply becomes a new
//! Document in its place that is reviewed like any other.
use mdoc_pii::{Category, restore::ExportedIdentity};
use mdoc_vault::{
    Document, DocumentOrigin, Fault, FaultHook, Mapping, NewDocument, NewProject, NewReply,
    ObjectKind, Readiness, ReplyOwner, SourceKind, Stage, Vault, VaultError,
};
use std::{path::PathBuf, sync::Arc};

const KEY: [u8; 32] = [9; 32];

struct Folder {
    temp: tempfile::TempDir,
    root: PathBuf,
}

impl Folder {
    fn new() -> Self {
        let temp = tempfile::tempdir().unwrap();
        let root = temp.path().join("vault");
        Self { temp, root }
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

fn project(vault: &Vault, name: &str, names: &[ExportedIdentity]) -> String {
    let project = vault
        .create_project(NewProject {
            name: name.into(),
            ..NewProject::default()
        })
        .unwrap();
    vault
        .update_project(&project.id, |p| p.mapping.merge(names))
        .unwrap()
        .id
}

fn document(vault: &Vault, project: Option<&str>, title: &str, markdown: &str) -> String {
    vault
        .create_document(NewDocument {
            project: project.map(String::from),
            title: title.into(),
            source_kind: SourceKind::Text,
            origin: DocumentOrigin::Ordinary,
            markdown: markdown.into(),
        })
        .unwrap()
        .id
}

fn reply(vault: &Vault, owner: ReplyOwner, markdown: &str) -> String {
    vault
        .save_reply(NewReply {
            owner,
            title: "Reply to Lease".into(),
            source_app: Some("Claude".into()),
            markdown: markdown.into(),
        })
        .unwrap()
        .id
}

/// Saved while Ready, as the app's review says at a save.
fn mark_ready(vault: &Vault, id: &str) {
    vault
        .update_document(id, |d| {
            d.scanned = true;
            d.set_ready(true);
        })
        .unwrap();
}

/// A record an earlier build wrote, without notes or `from_reply`, loads
/// with none, and writes back as it was; a record with them is still read
/// by a build that knows fields it does not (no `deny_unknown_fields`), so
/// an earlier build reads one this build wrote.
#[test]
fn records_without_and_with_the_new_fields_both_load() {
    let old = serde_json::json!({
        "id": "0123456789abcdef0123456789abcdef",
        "project": null,
        "title": "Lease",
        "source_kind": "markdown",
        "created": 1,
        "updated": 2,
        "revision": 3,
        "markdown": "PERSON_1 signed.",
    });
    let document: Document = serde_json::from_value(old.clone()).unwrap();
    assert_eq!(document.notes, "");
    assert_eq!(document.from_reply, None);
    let written = serde_json::to_value(&document).unwrap();
    assert!(written.get("notes").is_none() && written.get("from_reply").is_none());
    let reread: Document = serde_json::from_value(written).unwrap();
    assert_eq!(reread, document);

    let mut newer = serde_json::to_value(Document {
        notes: "Call the landlord.".into(),
        from_reply: Some("fedcba9876543210fedcba9876543210".into()),
        ..document
    })
    .unwrap();
    assert_eq!(newer["notes"], "Call the landlord.");
    // A field from a later build still loads here, as these load in an
    // earlier one.
    newer["a_later_field"] = serde_json::json!({ "any": [1, 2] });
    let read: Document = serde_json::from_value(newer).unwrap();
    assert_eq!(read.notes, "Call the landlord.");
}

/// Notes are sealed in the Document record: they survive a reopen, a text
/// save racing them, and a move, and a later text save never clears them.
#[test]
fn notes_are_kept_through_saves_moves_and_a_reopen() {
    let folder = Folder::new();
    let vault = folder.open();
    let id = document(&vault, None, "Lease", "Anna signed.");
    let noted = vault
        .set_notes(&id, "Ask about the deposit.".into())
        .unwrap();
    assert_eq!(noted.notes, "Ask about the deposit.");
    assert_eq!(noted.revision, 2);
    // A text save, as the app's autosave does it.
    vault
        .update_document(&id, |d| d.markdown = "Anna signed twice.".into())
        .unwrap();
    // A stale copy saved over it leaves the notes too: it is skipped.
    let mut stale = noted.clone();
    stale.notes = String::new();
    stale.revision = 1;
    assert_eq!(
        vault.save_document(&stale).unwrap(),
        mdoc_vault::Saved::Skipped
    );
    let memo = project(&vault, "Memo", &[]);
    vault.move_document(&id, Some(&memo)).unwrap();
    drop(vault);
    let vault = folder.open();
    let stored = vault.document(&id).unwrap();
    assert_eq!(stored.notes, "Ask about the deposit.");
    assert_eq!(stored.markdown, "Anna signed twice.");
    // Not in what lists and search read.
    let summary = serde_json::to_string(&vault.documents()).unwrap();
    assert!(!summary.contains("deposit"));
    // The sealed file never holds them in the clear.
    let sealed = std::fs::read(folder.root.join("docs").join(&id).join("doc.enc")).unwrap();
    assert!(!sealed.windows(7).any(|w| w == b"deposit"));
}

/// Copy for AI, Share, the shelf and MCP read `text_for_ai`: it never
/// carries the notes, and notes never make a Document not Ready.
#[test]
fn text_for_ai_never_reads_the_notes() {
    let folder = Folder::new();
    let vault = folder.open();
    let memo = project(&vault, "Memo", &[person("PERSON_1", &["Anna Petrova"])]);
    let id = document(&vault, Some(&memo), "Lease", "PERSON_1 signed.");
    mark_ready(&vault, &id);
    vault
        .set_notes(&id, "Anna Petrova is the tenant; secret.".into())
        .unwrap();
    assert_eq!(vault.readiness(&id).unwrap(), Readiness::Ready);
    let text = vault.text_for_ai(&id).unwrap();
    assert!(text.contains("PERSON_1 signed."));
    assert!(!text.contains("secret") && !text.contains("Anna Petrova"));
}

/// Export Project writes `<Title>.notes.md` beside `<Title>.md` only when
/// the notes are not empty, paired with the name the Markdown took.
#[test]
fn export_writes_notes_beside_their_document_only_when_there_are_some() {
    let folder = Folder::new();
    let vault = folder.open();
    let memo = project(&vault, "Memo", &[]);
    let first = document(&vault, Some(&memo), "Lease", "One.");
    let second = document(&vault, Some(&memo), "Lease", "Two.");
    let plain = document(&vault, Some(&memo), "Letter", "Three.");
    let blank = document(&vault, Some(&memo), "Brief", "Four.");
    vault.set_notes(&second, "Second's notes.".into()).unwrap();
    vault.set_notes(&first, "First's notes.".into()).unwrap();
    vault.set_notes(&blank, "  \n".into()).unwrap();
    let _ = plain;
    let picked = folder.temp.path().join("Exports");
    let exported = vault.export_project(&memo, &picked).unwrap();
    assert!(exported.failed.is_empty());
    let mut names: Vec<String> = std::fs::read_dir(&exported.folder)
        .unwrap()
        .map(|entry| entry.unwrap().file_name().to_string_lossy().into_owned())
        .collect();
    names.sort();
    assert_eq!(
        names,
        [
            "Brief.md",
            "Lease 2.md",
            "Lease 2.notes.md",
            "Lease.md",
            "Lease.notes.md",
            "Letter.md"
        ]
    );
    let read = |name: &str| std::fs::read_to_string(exported.folder.join(name)).unwrap();
    assert_eq!(read("Lease.md"), "One.");
    assert_eq!(read("Lease.notes.md"), "First's notes.");
    assert_eq!(read("Lease 2.notes.md"), "Second's notes.");
}

/// A Project's Reply promotes into the Project: its text restored with
/// real names, Markdown with no original, not reviewed, naming the Reply,
/// which stays. A review then finds the Project's names in it again.
#[test]
fn a_project_reply_promotes_into_its_project_and_is_kept() {
    let folder = Folder::new();
    let vault = folder.open();
    let memo = project(
        &vault,
        "Memo",
        &[
            person("PERSON_1", &["Anna Petrova"]),
            person("PERSON_2", &["Boris Ivanov"]),
        ],
    );
    let id = reply(
        &vault,
        ReplyOwner::Project(memo.clone()),
        "PERSON_2 owes PERSON_1 the deposit; PERSON_9 too.",
    );
    let promoted = vault.promote_reply(&id).unwrap();
    assert_eq!(promoted.project.as_deref(), Some(memo.as_str()));
    assert_eq!(
        promoted.markdown,
        "Boris Ivanov owes Anna Petrova the deposit; PERSON_9 too."
    );
    assert_eq!(promoted.markdown, vault.restore_reply(&id).unwrap().text);
    assert_eq!(promoted.title, "Reply to Lease");
    assert_eq!(promoted.source_kind, SourceKind::Markdown);
    assert_eq!(promoted.origin, DocumentOrigin::Ordinary);
    assert_eq!(promoted.from_reply.as_deref(), Some(id.as_str()));
    assert!(promoted.original.is_none() && promoted.review.is_none());
    assert!(!promoted.scanned && promoted.mapping.is_empty());
    assert_eq!(
        vault.readiness(&promoted.id).unwrap(),
        Readiness::NotReviewed
    );
    assert!(matches!(
        vault.text_for_ai(&promoted.id),
        Err(VaultError::NotReady(Readiness::NotReviewed))
    ));
    // The Reply is kept as received.
    let kept = vault.reply(&id).unwrap();
    assert_eq!(
        kept.markdown,
        "PERSON_2 owes PERSON_1 the deposit; PERSON_9 too."
    );
    // Its names are the Project's, so a review marks them again.
    let names = (vault.project(&memo).unwrap().mapping.exported())
        .into_iter()
        .flat_map(|identity| identity.originals)
        .filter(|original| promoted.markdown.contains(original.as_ref()))
        .count();
    assert_eq!(names, 2);
    // A text save keeps where it came from.
    vault
        .update_document(&promoted.id, |d| d.from_reply = None)
        .unwrap();
    drop(vault);
    let vault = folder.open();
    let stored = vault.document(&promoted.id).unwrap();
    assert_eq!(stored.from_reply.as_deref(), Some(id.as_str()));
    // A Document's Reply goes where its Document is now.
    let letter = document(&vault, Some(&memo), "Letter", "PERSON_1 wrote.");
    let followed = reply(&vault, ReplyOwner::Document(letter), "PERSON_1 agreed.");
    let promoted = vault.promote_reply(&followed).unwrap();
    assert_eq!(promoted.project.as_deref(), Some(memo.as_str()));
    assert_eq!(promoted.markdown, "Anna Petrova agreed.");
}

/// An Unfiled Document's Reply promotes into Unfiled, with the identities
/// the Reply is bound to as its own names, under the same aliases, so a
/// review marks them again; names the Reply does not use stay behind.
#[test]
fn an_unfiled_reply_promotes_into_unfiled_with_its_names() {
    let folder = Folder::new();
    let vault = folder.open();
    let note = document(&vault, None, "Note", "PERSON_1 met PERSON_2 and PERSON_3.");
    let mut local = Mapping::default();
    local.merge(&[
        person("PERSON_1", &["Anna Petrova"]),
        person("PERSON_2", &["Boris Ivanov"]),
        person("PERSON_3", &["Carl Diaz"]),
    ]);
    let source = local.clone();
    vault.update_document(&note, |d| d.mapping = local).unwrap();
    let id = reply(
        &vault,
        ReplyOwner::Document(note.clone()),
        "PERSON_2 thanks PERSON_1.",
    );
    let promoted = vault.promote_reply(&id).unwrap();
    assert_eq!(promoted.project, None);
    assert_eq!(promoted.markdown, "Boris Ivanov thanks Anna Petrova.");
    let own: Vec<(String, String, Vec<String>)> = (promoted.mapping.identities.iter())
        .map(|i| (i.id.clone(), i.alias.clone(), i.originals.clone()))
        .collect();
    let expected: Vec<_> = ["PERSON_2", "PERSON_1"]
        .into_iter()
        .map(|alias| {
            let i = source.identity_named(alias).unwrap();
            (i.id.clone(), i.alias.clone(), i.originals.clone())
        })
        .collect();
    assert_eq!(own, expected, "the bound identities only, as they are");
    // Its own mapping is saved with it, and later saves keep it.
    let stored = vault.document(&promoted.id).unwrap();
    assert_eq!(stored.mapping, promoted.mapping);
    assert_eq!(vault.documents_in(None).len(), 2);
    assert!(vault.reply(&id).is_ok());
}

/// Nothing is written when the Reply or its owner is gone, and a crash at
/// the record's write leaves no Document.
#[test]
fn a_failed_promotion_leaves_no_document() {
    let folder = Folder::new();
    let vault = folder.open();
    let missing = "0123456789abcdef0123456789abcdef";
    assert!(matches!(
        vault.promote_reply(missing),
        Err(VaultError::NotFound {
            kind: ObjectKind::Reply,
            ..
        })
    ));
    let note = document(&vault, None, "Note", "PERSON_1.");
    let orphan = reply(&vault, ReplyOwner::Document(note.clone()), "PERSON_1.");
    // Its Document deleted under it by another handle, the reply's file
    // left as a stopped delete would.
    let reply_file = folder.root.join("replies").join(format!("{orphan}.enc"));
    let kept = std::fs::read(&reply_file).unwrap();
    vault.delete_document(&note).unwrap();
    std::fs::write(&reply_file, kept).unwrap();
    assert!(matches!(
        vault.promote_reply(&orphan),
        Err(VaultError::NotFound {
            kind: ObjectKind::Document,
            ..
        })
    ));
    assert!(vault.documents().is_empty());
    let memo = project(&vault, "Memo", &[person("PERSON_1", &["Anna"])]);
    let id = reply(&vault, ReplyOwner::Project(memo), "PERSON_1.");
    drop(vault);

    {
        let stage = Stage::BeforeRename;
        let hook: FaultHook = Arc::new(move |step| {
            if step.kind == ObjectKind::Document && step.stage == stage {
                Fault::Crash
            } else {
                Fault::Continue
            }
        });
        let crashing = Vault::open_with_hook(&folder.root, &KEY, hook).unwrap();
        assert!(matches!(
            crashing.promote_reply(&id),
            Err(VaultError::Crashed)
        ));
        drop(crashing);
        let vault = folder.open();
        assert!(vault.documents().is_empty(), "no half-made Document");
        assert!(vault.reply(&id).is_ok());
        // The next try makes one.
        assert!(vault.promote_reply(&id).is_ok());
        assert_eq!(vault.documents().len(), 1);
    }
}
