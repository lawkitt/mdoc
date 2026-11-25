//! Deliveries and AI activity on a temp Vault with an injected key, with
//! no socket and no MCP (#1164 Decisions 8, 25-27; Milestone 2 item 6):
//! a delivery names documents read under the current share or is
//! refused, a reply restores by identity, a delivered Document links back
//! to its reply, and every read and delivery is recorded with its app.
use mdoc_connector::{
    AiEvent, Connector, ConnectorError, Delivered, Delivery, DeliveryKind, LONG, Read,
};
use mdoc_pii::{Category, restore::ExportedIdentity};
use mdoc_store::settings::AiConnectorConfig;
use mdoc_vault::{
    DocumentOrigin, NewDocument, NewProject, ProjectDocuments, Readiness, ReplyOwner, SourceKind,
    Vault,
};
use std::sync::Arc;

const KEY: [u8; 32] = [7; 32];
const ON: AiConnectorConfig = AiConnectorConfig {
    preview: true,
    open_deliveries: false,
};
const CLAUDE: &str = "Claude";

struct Fixture {
    temp: tempfile::TempDir,
    vault: Arc<Vault>,
    connector: Connector,
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

fn fixture() -> Fixture {
    let temp = tempfile::tempdir().unwrap();
    let vault = Arc::new(Vault::open(temp.path().join("vault"), &KEY).unwrap());
    let connector = Connector::start(vault.clone(), &ON).unwrap();
    Fixture {
        temp,
        vault,
        connector,
    }
}

/// A reply delivery of `markdown`, naming `read`.
fn reply(read: &[&str], markdown: &str) -> Delivery {
    Delivery {
        kind: DeliveryKind::Reply,
        title: None,
        read: read.iter().map(|id| id.to_string()).collect(),
        markdown: markdown.into(),
    }
}

impl Fixture {
    /// A Project that knows Anna Petrova as PERSON_1.
    fn project(&self, name: &str) -> String {
        let project = (self.vault)
            .create_project(NewProject {
                name: name.into(),
                ..NewProject::default()
            })
            .unwrap();
        (self.vault)
            .update_project(&project.id, |p| {
                p.mapping.merge(&[person("PERSON_1", "Anna Petrova")])
            })
            .unwrap()
            .id
    }

    /// A document saved as the review saves a Ready text.
    fn ready(&self, project: Option<&str>, title: &str, markdown: &str) -> String {
        let id = self.draft(project, title, markdown);
        (self.vault)
            .update_document(&id, |d| {
                d.scanned = true;
                d.set_ready(true);
            })
            .unwrap();
        id
    }

    /// A document never reviewed.
    fn draft(&self, project: Option<&str>, title: &str, markdown: &str) -> String {
        (self.vault)
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

    /// Read document `id` as `app`, which must hand its text out.
    fn read(&self, app: &str, id: &str) {
        match self.connector.get_document(app, id, None).unwrap() {
            Read::Text { .. } => {}
            other => panic!("not a text: {other:?}"),
        }
    }

    fn deliver(&self, delivery: Delivery) -> Result<Delivered, ConnectorError> {
        self.connector.deliver(CLAUDE, delivery)
    }

    /// A Project shared with a Ready letter read from it.
    fn shared_with_a_read_letter(&self) -> (String, String) {
        let shared = self.project("Shared");
        let letter = self.ready(Some(&shared), "Letter", "PERSON_1 signed.");
        self.connector.share(&shared).unwrap();
        self.read(CLAUDE, &letter);
        (shared, letter)
    }
}

/// Done when: one that names a document outside the AI project is
/// refused. Another Project's, Unfiled, unknown and none at all are
/// refused before anything is written, and so is a document read and then
/// moved out of the AI project.
#[test]
fn a_delivery_naming_a_document_outside_the_ai_project_is_refused() {
    let f = fixture();
    let (_, letter) = f.shared_with_a_read_letter();
    let other = f.project("Other");
    let elsewhere = f.ready(Some(&other), "Memo", "PERSON_1 wrote.");
    let unfiled = f.ready(None, "Note", "PERSON_1 called.");
    for named in [
        vec![],
        vec![elsewhere.as_str()],
        vec![unfiled.as_str()],
        vec![letter.as_str(), elsewhere.as_str()],
    ] {
        assert!(
            matches!(
                f.deliver(reply(&named, "PERSON_1 agrees.")),
                Err(ConnectorError::NotRead)
            ),
            "{named:?}"
        );
    }
    assert!(f.vault.replies().is_empty());

    f.vault.move_document(&letter, Some(&other)).unwrap();
    assert!(matches!(
        f.deliver(reply(&[&letter], "PERSON_1 agrees.")),
        Err(ConnectorError::NotShared)
    ));
    assert!(f.vault.replies().is_empty());
    assert!(f.vault.documents_in(Some(&other)).len() == 2);
}

/// Done when: refused outside what was served under the current share. A
/// document read before a switch, before Stop sharing, or before sharing
/// the same Project again, only listed, or answered "needs review", was
/// not read under this share.
#[test]
fn a_delivery_names_only_documents_read_under_the_current_share() {
    let f = fixture();
    let (shared, letter) = f.shared_with_a_read_letter();
    let other = f.project("Other");
    f.connector.share(&other).unwrap();
    f.connector.share(&shared).unwrap();
    assert!(matches!(
        f.deliver(reply(&[&letter], "PERSON_1 agrees.")),
        Err(ConnectorError::NotRead)
    ));
    f.connector.stop_sharing();
    assert!(matches!(
        f.deliver(reply(&[&letter], "PERSON_1 agrees.")),
        Err(ConnectorError::NothingShared)
    ));

    f.connector.share(&shared).unwrap();
    let draft = f.draft(Some(&shared), "Draft", "Anna Petrova wrote.");
    f.connector.list_documents().unwrap();
    assert!(matches!(
        f.connector.get_document(CLAUDE, &draft, None).unwrap(),
        Read::NeedsReview { .. }
    ));
    for named in [&letter, &draft] {
        assert!(matches!(
            f.deliver(reply(&[named], "PERSON_1 agrees.")),
            Err(ConnectorError::NotRead)
        ));
    }
    assert!(f.vault.replies().is_empty());
    f.read(CLAUDE, &letter);
    assert!(f.deliver(reply(&[&letter], "PERSON_1 agrees.")).is_ok());
}

/// Done when: a deleted document is refused. So is one excluded from AI
/// after it was read, and a delivery whose Project was deleted, which
/// shares nothing from then on.
#[test]
fn a_delivery_naming_a_deleted_or_excluded_document_is_refused() {
    let f = fixture();
    let (shared, letter) = f.shared_with_a_read_letter();
    let memo = f.ready(Some(&shared), "Memo", "PERSON_1 wrote.");
    f.read(CLAUDE, &memo);
    f.vault.delete_document(&letter).unwrap();
    assert!(matches!(
        f.deliver(reply(&[&memo, &letter], "PERSON_1 agrees.")),
        Err(ConnectorError::NotShared)
    ));
    (f.vault.update_document(&memo, |d| d.exclude_from_ai = true)).unwrap();
    assert!(matches!(
        f.deliver(reply(&[&memo], "PERSON_1 agrees.")),
        Err(ConnectorError::NotShared)
    ));
    assert!(f.vault.replies().is_empty());

    (f.vault
        .update_document(&memo, |d| d.exclude_from_ai = false))
    .unwrap();
    f.vault
        .delete_project(&shared, ProjectDocuments::MoveToUnfiled)
        .unwrap();
    assert!(matches!(
        f.deliver(reply(&[&memo], "PERSON_1 agrees.")),
        Err(ConnectorError::NothingShared)
    ));
    assert_eq!(f.connector.ai_project(), None);
    assert!(f.vault.replies().is_empty());
}

/// Done when: a reply restoring by identity. It is the AI project's
/// reply, as received, from its app, bound to the identities its aliases
/// mean now: an alias the Project learns later is not read into it. Its
/// title is restored now; without one it is "Reply to" the document read.
/// The answer holds ids and counts and the unknown aliases only.
#[test]
fn a_delivered_reply_restores_by_identity() {
    let f = fixture();
    let (shared, letter) = f.shared_with_a_read_letter();
    let delivered = f
        .deliver(reply(&[&letter], "PERSON_1 and PERSON_2 agree."))
        .unwrap();
    assert_eq!(delivered.document, None);
    assert_eq!(delivered.restored, 1);
    assert_eq!(delivered.unresolved, ["PERSON_2"]);
    assert!(!format!("{delivered:?}").contains("Anna"));

    let saved = f.vault.reply(&delivered.reply).unwrap();
    assert_eq!(saved.owner, ReplyOwner::Project(shared.clone()));
    assert_eq!(saved.markdown, "PERSON_1 and PERSON_2 agree.");
    assert_eq!(saved.source_app.as_deref(), Some(CLAUDE));
    assert_eq!(saved.title, "Reply to Letter");
    let listed: Vec<_> = (f.vault.project_replies(&shared).into_iter())
        .map(|r| r.id)
        .collect();
    assert_eq!(listed, std::slice::from_ref(&delivered.reply));

    (f.vault)
        .update_project(&shared, |p| {
            p.mapping.merge(&[person("PERSON_2", "Tom Reed")])
        })
        .unwrap();
    assert_eq!(
        f.vault.restore_reply(&delivered.reply).unwrap().text,
        "Anna Petrova and PERSON_2 agree."
    );

    let memo = f.ready(Some(&shared), "Memo", "PERSON_2 wrote.");
    f.read(CLAUDE, &memo);
    let titled = Delivery {
        title: Some("  Advice for PERSON_1\nand more".into()),
        ..reply(&[&letter, &memo, &letter], "PERSON_2 agrees.")
    };
    let delivered = f.deliver(titled).unwrap();
    let saved = f.vault.reply(&delivered.reply).unwrap();
    assert_eq!(saved.title, "Advice for Anna Petrova");
    assert_eq!(
        f.vault.restore_reply(&delivered.reply).unwrap().text,
        "Tom Reed agrees."
    );
    let untitled = f.deliver(reply(&[&memo, &letter], "Done.")).unwrap();
    let saved = f.vault.reply(&untitled.reply).unwrap();
    assert_eq!(saved.title, "Reply to Memo and 1 more");
}

/// Done when: a delivered Document linking back. It lands in the AI
/// project as Markdown with its names restored, unreviewed, named by the
/// reply it was saved as too; the connector answers "needs review" for it
/// until it is reviewed. A Document needs a title, and any delivery text.
#[test]
fn a_delivered_document_links_back_to_its_reply() {
    let f = fixture();
    let (shared, letter) = f.shared_with_a_read_letter();
    let untitled = Delivery {
        kind: DeliveryKind::Document,
        title: Some("  ".into()),
        ..reply(&[&letter], "PERSON_1 agrees.")
    };
    assert!(matches!(
        f.deliver(untitled),
        Err(ConnectorError::TitleNeeded)
    ));
    assert!(matches!(
        f.deliver(reply(&[&letter], " \n")),
        Err(ConnectorError::Empty)
    ));
    assert!(f.vault.replies().is_empty());

    let delivery = Delivery {
        kind: DeliveryKind::Document,
        title: Some("Draft for PERSON_1".into()),
        ..reply(&[&letter], "# Draft\n\nPERSON_1 agrees.")
    };
    let delivered = f.deliver(delivery).unwrap();
    assert!(!format!("{delivered:?}").contains("Anna"));
    let id = delivered.document.expect("a document");
    let document = f.vault.document(&id).unwrap();
    assert_eq!(document.project.as_deref(), Some(shared.as_str()));
    assert_eq!(
        document.from_reply.as_deref(),
        Some(delivered.reply.as_str())
    );
    assert_eq!(document.markdown, "# Draft\n\nAnna Petrova agrees.");
    assert_eq!(document.title, "Draft for Anna Petrova");
    assert_eq!(document.source_kind, SourceKind::Markdown);
    assert_eq!(f.vault.readiness(&id).unwrap(), Readiness::NotReviewed);
    let reply = f.vault.reply(&delivered.reply).unwrap();
    assert_eq!(reply.markdown, "# Draft\n\nPERSON_1 agrees.");
    assert_eq!(reply.title, "Draft for Anna Petrova");
    assert!(matches!(
        f.connector.get_document(CLAUDE, &id, None).unwrap(),
        Read::NeedsReview { .. }
    ));
}

/// Done when: the record. Each read that hands text out, its outline or a
/// part, and each delivery, is a line of the Project's AI activity with
/// its app and time, newest first. A listing, a "needs review" answer and
/// a refused delivery are not.
#[test]
fn each_read_and_delivery_is_recorded_with_its_app() {
    let f = fixture();
    let (shared, letter) = f.shared_with_a_read_letter();
    let long = f.ready(Some(&shared), "Long", &"word ".repeat(LONG / 4));
    let draft = f.draft(Some(&shared), "Draft", "Anna Petrova wrote.");
    f.connector.list_documents().unwrap();
    f.connector.get_document(CLAUDE, &draft, None).unwrap();
    f.read("ChatGPT", &letter);
    f.connector.get_document(CLAUDE, &long, None).unwrap();
    f.connector.get_document(CLAUDE, &long, Some(2)).unwrap();
    assert!(f.deliver(reply(&[&draft], "No.")).is_err());
    let delivered = f.deliver(reply(&[&letter, &long], "Yes.")).unwrap();

    let activity = f.vault.ai_activity(&shared).unwrap();
    assert_eq!(activity[0], delivered.activity);
    let lines: Vec<_> = (activity.iter())
        .map(|line| (line.app.as_str(), line.event.clone()))
        .collect();
    let read = |document: &str, part| AiEvent::Read {
        document: document.to_string(),
        part,
    };
    assert_eq!(
        lines,
        [
            (
                CLAUDE,
                AiEvent::Delivered {
                    reply: delivered.reply.clone(),
                    document: None,
                    read: vec![letter.clone(), long.clone()],
                }
            ),
            (CLAUDE, read(&long, Some(2))),
            (CLAUDE, read(&long, None)),
            ("ChatGPT", read(&letter, None)),
            (CLAUDE, read(&letter, None)),
        ]
    );
    assert!(activity.iter().all(|line| line.at > 0));
}

/// A read whose activity line can't be written hands nothing out, and so
/// counts as no read for a delivery.
#[test]
fn a_read_that_cannot_be_recorded_hands_nothing_out() {
    let f = fixture();
    let shared = f.project("Shared");
    let letter = f.ready(Some(&shared), "Letter", "PERSON_1 signed.");
    f.connector.share(&shared).unwrap();
    let file = f
        .temp
        .path()
        .join("vault/activity")
        .join(format!("{shared}.enc"));
    std::fs::create_dir_all(&file).unwrap();
    assert!(matches!(
        f.connector.get_document(CLAUDE, &letter, None),
        Err(ConnectorError::Vault(_))
    ));
    assert!(matches!(
        f.deliver(reply(&[&letter], "PERSON_1 agrees.")),
        Err(ConnectorError::NotRead)
    ));
    std::fs::remove_dir(&file).unwrap();
    f.read(CLAUDE, &letter);
    assert_eq!(f.vault.ai_activity(&shared).unwrap().len(), 1);
}

/// Old Vaults load: one written before AI activity has none, opens as it
/// did, and gains the file on its first read; the activity reads back
/// after a reopen and goes with its Project.
#[test]
fn an_old_vault_has_no_activity_and_gains_it() {
    let f = fixture();
    let shared = f.project("Shared");
    let letter = f.ready(Some(&shared), "Letter", "PERSON_1 signed.");
    let root = f.temp.path().join("vault");
    assert!(!root.join("activity").exists());
    let Fixture {
        temp,
        vault,
        connector,
    } = f;
    drop(connector);
    drop(vault);
    let vault = Arc::new(Vault::open(&root, &KEY).unwrap());
    assert!(vault.ai_activity(&shared).unwrap().is_empty());
    assert_eq!(vault.document(&letter).unwrap().from_reply, None);
    let connector = Connector::start(vault.clone(), &ON).unwrap();
    connector.share(&shared).unwrap();
    assert!(matches!(
        connector.get_document(CLAUDE, &letter, None).unwrap(),
        Read::Text { .. }
    ));
    drop(connector);
    drop(vault);
    let vault = Vault::open(&root, &KEY).unwrap();
    assert_eq!(vault.ai_activity(&shared).unwrap().len(), 1);
    (vault.delete_project(&shared, ProjectDocuments::Delete)).unwrap();
    assert!(!root.join("activity").join(format!("{shared}.enc")).exists());
    drop(temp);
}

/// Review 5: a delivery names documents its own app read. Another app's
/// read of the same document, in the Project shared after a move, does
/// not let an answer about the first Project through.
#[test]
fn an_apps_delivery_names_only_what_that_app_read() {
    let f = fixture();
    let (_, letter) = f.shared_with_a_read_letter();
    let other = f.project("Other");
    f.vault.move_document(&letter, Some(&other)).unwrap();
    (f.vault.update_document(&letter, |d| d.set_ready(true))).unwrap();
    f.connector.share(&other).unwrap();
    f.read("ChatGPT", &letter);
    assert!(matches!(
        f.deliver(reply(&[&letter], "PERSON_1 agrees.")),
        Err(ConnectorError::NotRead)
    ));
    let theirs = f
        .connector
        .deliver("ChatGPT", reply(&[&letter], "PERSON_1 agrees."));
    assert!(theirs.is_ok());
    assert_eq!(f.vault.replies().len(), 1);
}

/// Review 4: a Project deleted before anything was read from it is shared
/// no more once a delivery finds it gone.
#[test]
fn a_delivery_to_a_deleted_project_shares_nothing() {
    let f = fixture();
    let shared = f.project("Shared");
    let letter = f.ready(Some(&shared), "Letter", "PERSON_1 signed.");
    f.connector.share(&shared).unwrap();
    f.vault
        .delete_project(&shared, ProjectDocuments::MoveToUnfiled)
        .unwrap();
    assert!(matches!(
        f.deliver(reply(&[&letter], "PERSON_1 agrees.")),
        Err(ConnectorError::NothingShared)
    ));
    assert_eq!(f.connector.ai_project(), None);
}

/// Review 1: a delivery whose activity line can't be written takes back
/// its reply and Document, so a retry makes no second copy.
#[test]
fn a_delivery_that_cannot_be_recorded_leaves_nothing() {
    let f = fixture();
    let (shared, letter) = f.shared_with_a_read_letter();
    let file = (f.temp.path().join("vault/activity")).join(format!("{shared}.enc"));
    std::fs::remove_file(&file).unwrap();
    std::fs::create_dir_all(&file).unwrap();
    let delivery = Delivery {
        kind: DeliveryKind::Document,
        title: Some("Draft".into()),
        ..reply(&[&letter], "PERSON_1 agrees.")
    };
    assert!(matches!(
        f.deliver(delivery.clone()),
        Err(ConnectorError::Vault(_))
    ));
    assert!(f.vault.replies().is_empty());
    assert_eq!(f.vault.documents_in(Some(&shared)).len(), 1);
    std::fs::remove_dir(&file).unwrap();
    let delivered = f.deliver(delivery).unwrap();
    assert_eq!(f.vault.replies().len(), 1);
    assert!(delivered.document.is_some());
    let reopened = Vault::open(f.temp.path().join("vault"), &KEY).unwrap();
    assert_eq!(reopened.replies().len(), 1);
    assert_eq!(reopened.documents_in(Some(&shared)).len(), 2);
}

/// Review 3: deleting a Project whose activity can't be removed fails
/// before any of its Documents or replies change.
#[test]
fn a_project_whose_activity_cannot_go_stays_whole() {
    let f = fixture();
    let (shared, letter) = f.shared_with_a_read_letter();
    f.deliver(reply(&[&letter], "PERSON_1 agrees.")).unwrap();
    let file = (f.temp.path().join("vault/activity")).join(format!("{shared}.enc"));
    std::fs::remove_file(&file).unwrap();
    std::fs::create_dir_all(file.join("held")).unwrap();
    assert!(
        f.vault
            .delete_project(&shared, ProjectDocuments::Delete)
            .is_err()
    );
    assert!(f.vault.project(&shared).is_ok());
    assert_eq!(f.vault.documents_in(Some(&shared)).len(), 1);
    assert_eq!(f.vault.project_replies(&shared).len(), 1);
}
