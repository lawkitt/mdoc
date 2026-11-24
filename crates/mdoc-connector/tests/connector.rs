//! The connector service's rules on a temp Vault with an injected key, with
//! no socket and no MCP (#1164 Testing Decisions, the core seam; Milestone
//! 2 item 5): nothing outside the AI project, Ready re-checked on each
//! read, labels never from a stored title, the outline for long
//! documents, and nothing shared or started by default.
use mdoc_connector::{Connector, ConnectorError, LONG, PART_MAX, Read, Readiness, State};
use mdoc_pii::{Category, restore::ExportedIdentity};
use mdoc_store::settings::AiConnectorConfig;
use mdoc_vault::{DocumentOrigin, NewDocument, NewProject, SourceKind, Vault};
use std::sync::Arc;

const KEY: [u8; 32] = [7; 32];
const ON: AiConnectorConfig = AiConnectorConfig { preview: true };

struct Fixture {
    _temp: tempfile::TempDir,
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
        _temp: temp,
        vault,
        connector,
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

    /// The row of document `id`.
    fn row(&self, id: &str) -> (String, String, State) {
        (self.listed().into_iter().find(|row| row.0 == id)).expect("listed")
    }

    fn listed(&self) -> Vec<(String, String, State)> {
        (self.connector.list_documents().unwrap().into_iter())
            .map(|listed| (listed.doc_id, listed.label, listed.state))
            .collect()
    }

    fn text(&self, id: &str) -> String {
        match self.connector.get_document(id, None).unwrap() {
            Read::Text { text, .. } => text,
            other => panic!("not a text: {other:?}"),
        }
    }
}

/// Done when: no read reaches another Project or Unfiled. Neither is
/// listed, both are refused alike by id, and a document moved out of the
/// AI project is refused from then on.
#[test]
fn no_read_reaches_another_project_or_unfiled() {
    let f = fixture();
    let shared = f.project("Shared");
    let other = f.project("Other");
    let letter = f.ready(Some(&shared), "Letter", "PERSON_1 signed.");
    let elsewhere = f.ready(Some(&other), "Memo", "PERSON_1 wrote.");
    let unfiled = f.ready(None, "Note", "PERSON_1 called.");
    assert!(f.vault.text_for_ai(&elsewhere).is_ok());
    assert!(f.vault.text_for_ai(&unfiled).is_ok());
    f.connector.share(&shared).unwrap();

    let listed: Vec<_> = f.listed().into_iter().map(|row| row.0).collect();
    assert_eq!(listed, std::slice::from_ref(&letter));
    for id in [&elsewhere, &unfiled, &"0".repeat(32)] {
        for part in [None, Some(1)] {
            assert!(
                matches!(
                    f.connector.get_document(id, part),
                    Err(ConnectorError::NotShared)
                ),
                "{id} {part:?}"
            );
        }
    }

    f.vault.move_document(&letter, Some(&other)).unwrap();
    assert!(f.listed().is_empty());
    assert!(matches!(
        f.connector.get_document(&letter, None),
        Err(ConnectorError::NotShared)
    ));
    f.vault.move_document(&elsewhere, None).unwrap();
    f.connector.share(&other).unwrap();
    assert!(matches!(
        f.connector.get_document(&elsewhere, None),
        Err(ConnectorError::NotShared)
    ));
}

/// Done when: each read re-checks the Project's names. A name another
/// document adds to the Project, or the Project's names gain in the Vault
/// alone, makes a Ready document need review on the next list and read,
/// at once and with no text.
#[test]
fn each_read_rechecks_the_projects_names() {
    let f = fixture();
    let project = f.project("Case");
    let letter = f.ready(
        Some(&project),
        "Letter",
        "# Lease\n\nPERSON_1 met Boris Ivanov.",
    );
    f.connector.share(&project).unwrap();
    assert_eq!(f.row(&letter).2, State::Ready);
    let text = f.text(&letter);
    assert_eq!(text, f.vault.text_for_ai(&letter).unwrap());

    // Another document of the Project names Boris.
    let memo = f.ready(Some(&project), "Memo", "PERSON_2 signed.");
    (f.vault)
        .update_project(&project, |p| {
            p.mapping
                .merge_from(&[person("PERSON_2", "Boris Ivanov")], Some(&memo))
        })
        .unwrap();

    let not_ready = State::NeedsReview(Readiness::NamesNotReplaced(1));
    let row = f.row(&letter);
    let number = 1 + (f.listed().iter().position(|row| row.0 == letter)).unwrap();
    let label = format!("Document {number}");
    assert_eq!(row, (letter.clone(), label.clone(), not_ready));
    for part in [None, Some(1)] {
        assert_eq!(
            f.connector.get_document(&letter, part).unwrap(),
            Read::NeedsReview {
                doc_id: letter.clone(),
                label: label.clone(),
                readiness: Readiness::NamesNotReplaced(1),
            }
        );
    }

    // Reviewed again, it is Ready again.
    (f.vault)
        .update_document(&letter, |d| {
            d.markdown = "# Lease\n\nPERSON_1 met PERSON_2.".into();
            d.set_ready(true);
        })
        .unwrap();
    assert!(f.text(&letter).contains("PERSON_1 met PERSON_2."));
}

/// Done when: no label comes from a stored title. Titles hold real names;
/// a Ready document is named by its first heading, else "Document N", and
/// one that needs review by "Document N" whatever its text says.
#[test]
fn no_label_comes_from_a_stored_title() {
    let f = fixture();
    let project = f.project("Case");
    let lease = f.ready(
        Some(&project),
        "Anna Petrova lease.docx",
        "Intro.\n\n# Lease of PERSON_1's flat\n\nSigned.",
    );
    let memo = f.ready(Some(&project), "Boris Ivanov memo.pdf", "PERSON_1 signed.");
    let draft = f.draft(
        Some(&project),
        "Clara Moss draft",
        "# Clara Moss's claim\n\nDraft.",
    );
    let copy = f.ready(
        Some(&project),
        "Anna Petrova copy",
        "# Lease of PERSON_1's flat\n",
    );
    f.connector.share(&project).unwrap();

    // Oldest first; documents made within one second by their ids.
    let mut order: Vec<_> = (f.vault.documents_in(Some(&project)).into_iter())
        .map(|summary| (summary.created, summary.id))
        .collect();
    order.sort();
    let listed = f.listed();
    let ids: Vec<_> = listed.iter().map(|row| row.0.clone()).collect();
    assert_eq!(ids, order.into_iter().map(|(_, id)| id).collect::<Vec<_>>());
    let label_of = |id: &str| (listed.iter().find(|row| row.0 == id)).unwrap().1.clone();
    let number = |id: &str| 1 + ids.iter().position(|listed| listed == id).unwrap();

    let heading = "Lease of PERSON_1's flat";
    let mut leases = [label_of(&lease), label_of(&copy)];
    if ids.iter().position(|id| *id == copy) < ids.iter().position(|id| *id == lease) {
        leases.reverse();
    }
    assert_eq!(leases, [heading.to_string(), format!("{heading} (2)")]);
    assert_eq!(label_of(&memo), format!("Document {}", number(&memo)));
    assert_eq!(label_of(&draft), format!("Document {}", number(&draft)));
    assert_eq!(f.row(&draft).2, State::NeedsReview(Readiness::NotReviewed));
    for id in [&lease, &memo, &draft, &copy] {
        let read = f.connector.get_document(id, None).unwrap();
        let (Read::Text { label: got, .. } | Read::NeedsReview { label: got, .. }) = read else {
            panic!("{read:?}");
        };
        assert_eq!(got, label_of(id));
    }
    for label in f.listed().into_iter().map(|row| row.1) {
        for name in ["Anna", "Boris", "Clara", "Petrova", ".docx", ".pdf"] {
            assert!(!label.contains(name), "{label}");
        }
    }
}

/// Done when: above about 60,000 characters a read is an outline with
/// numbered parts cut at headings; the parts give the text back, each read
/// alone carries the note, and a part past the last is refused. A shorter
/// document is its text, and its part 1 the same.
#[test]
fn a_long_document_is_read_as_an_outline_and_parts() {
    let f = fixture();
    let project = f.project("Case");
    let section = |n: usize| format!("## Clause {n}\n\n{}\n\n", "PERSON_1 agrees. ".repeat(600));
    let body: String = (1..=15).map(section).collect();
    assert!(body.chars().count() > LONG);
    let long = f.ready(Some(&project), "Contract", &body);
    let short = f.ready(Some(&project), "Letter", "PERSON_1 signed.");
    f.connector.share(&project).unwrap();
    let whole = f.vault.text_for_ai(&long).unwrap();
    let note = whole.strip_suffix(body.as_str()).unwrap();
    assert!(note.starts_with("Note: "), "{note}");

    let Read::Outline {
        label,
        note: got,
        size,
        parts,
        ..
    } = f.connector.get_document(&long, None).unwrap()
    else {
        panic!("not an outline");
    };
    assert_eq!(label, "Clause 1");
    assert_eq!(got, note);
    assert_eq!(size.chars, body.chars().count());
    assert!(parts.len() > 1);
    let headings: usize = parts.iter().map(|part| part.headings.len()).sum();
    assert_eq!(headings, 15);

    let mut joined = String::new();
    for info in &parts {
        let Read::Part {
            part,
            of,
            text,
            headings,
            ..
        } = f.connector.get_document(&long, Some(info.part)).unwrap()
        else {
            panic!("not a part");
        };
        assert_eq!((part, of), (info.part, parts.len()));
        assert_eq!(headings, info.headings);
        let text = text.strip_prefix(note).expect("each part carries the note");
        assert!(text.chars().count() <= PART_MAX);
        assert_eq!(text.chars().count(), info.chars);
        assert!(text.starts_with("## Clause "));
        joined.push_str(text);
    }
    assert_eq!(joined, body);
    let past = parts.len() + 1;
    for part in [0, past] {
        assert!(matches!(
            f.connector.get_document(&long, Some(part)),
            Err(ConnectorError::NoSuchPart { of }) if of == parts.len()
        ));
    }

    let text = f.text(&short);
    assert_eq!(text, f.vault.text_for_ai(&short).unwrap());
    assert_eq!(
        f.connector.get_document(&short, Some(1)).unwrap(),
        f.connector.get_document(&short, None).unwrap()
    );
    assert!(matches!(
        f.connector.get_document(&short, Some(2)),
        Err(ConnectorError::NoSuchPart { of: 1 })
    ));
}

/// Done when: with the AI project off, nothing is listed or read: no
/// connector while the setting is off, nothing shared when it starts, and
/// nothing again after Stop sharing.
#[test]
fn with_the_ai_project_off_nothing_is_listed() {
    let temp = tempfile::tempdir().unwrap();
    let vault = Arc::new(Vault::open(temp.path().join("vault"), &KEY).unwrap());
    let setting = mdoc_store::settings::Preferences::default().ai_connector;
    assert!(Connector::start(vault, &setting).is_none());

    let f = fixture();
    let project = f.project("Case");
    let letter = f.ready(Some(&project), "Letter", "PERSON_1 signed.");
    assert_eq!(f.connector.ai_project(), None);
    let nothing = |f: &Fixture| {
        assert!(matches!(
            f.connector.list_documents(),
            Err(ConnectorError::NothingShared)
        ));
        assert!(matches!(
            f.connector.get_document(&letter, None),
            Err(ConnectorError::NothingShared)
        ));
    };
    nothing(&f);

    f.connector.share(&project).unwrap();
    assert_eq!(f.connector.ai_project(), Some(project.clone()));
    assert_eq!(f.listed().len(), 1);
    f.connector.stop_sharing();
    nothing(&f);
    assert!(f.connector.share(&"0".repeat(32)).is_err());
    nothing(&f);
}

/// A document excluded from AI is never offered: not listed, and refused
/// as one outside the share.
#[test]
fn a_document_excluded_from_ai_is_not_offered() {
    let f = fixture();
    let project = f.project("Case");
    let letter = f.ready(Some(&project), "Letter", "PERSON_1 signed.");
    let kept = f.ready(Some(&project), "Kept", "PERSON_1 kept.");
    (f.vault)
        .update_document(&kept, |d| d.exclude_from_ai = true)
        .unwrap();
    f.connector.share(&project).unwrap();
    let listed: Vec<_> = f.listed().into_iter().map(|row| (row.0, row.1)).collect();
    assert_eq!(listed, [(letter, "Document 1".to_string())]);
    assert!(matches!(
        f.connector.get_document(&kept, None),
        Err(ConnectorError::NotShared)
    ));
}

/// A listed row's size counts its Markdown, without the note, Ready or not.
#[test]
fn sizes_count_the_markdown() {
    let f = fixture();
    let project = f.project("Case");
    let letter = f.ready(Some(&project), "Letter", "PERSON_1 signed it.");
    let draft = f.draft(Some(&project), "Draft", "Two words");
    f.connector.share(&project).unwrap();
    let listed = f.connector.list_documents().unwrap();
    let size = |id: &str| {
        let row = listed.iter().find(|row| row.doc_id == id).unwrap();
        (row.size.chars, row.size.words)
    };
    assert_eq!((size(&letter), size(&draft)), ((19, 3), (9, 2)));
}

/// A document whose own text begins like the note keeps all of it: the
/// note comes from the Vault apart from the text, never parsed out of it.
#[test]
fn a_document_that_begins_like_the_note_keeps_its_text() {
    let f = fixture();
    let project = f.project("Case");
    let text = "Note: names and other identifying details in this document are                 listed below.

---

# Schedule

No one is named.";
    let id = f.ready(Some(&project), "Schedule", text);
    f.connector.share(&project).unwrap();
    assert_eq!(f.text(&id), text);
    let listed = f.connector.list_documents().unwrap();
    assert_eq!(listed[0].size.chars, text.chars().count());
    assert_eq!(listed[0].label, "Schedule");
}
