//! Mappings in the Vault through its public interface (#1091, Testing
//! Decisions, seam 1: move and alias reconciliation, Project deletion):
//! the case store's mapping tests (`mdoc_store::cases`) and the mobile
//! core's move tests (`mdoc_ffi::document_session`), with Document ids in
//! place of paths and Unfiled beside Projects.
use mdoc_pii::{Category, restore::ExportedIdentity, restore::RestoreMap};
use mdoc_vault::{
    DocumentOrigin, Fault, FaultHook, Identity, Mapping, MoveUndo, NewDocument, NewProject,
    ObjectKind, Project, ProjectDocuments, Saved, SourceKind, Stage, UserLists, Vault, VaultError,
};
use std::{
    collections::HashSet,
    path::PathBuf,
    sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
    },
};

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

fn identity(alias: &str, category: Category, originals: &[&str]) -> ExportedIdentity {
    ExportedIdentity {
        alias: alias.into(),
        previous: Vec::new(),
        category,
        custom: false,
        originals: originals.iter().map(|o| Arc::from(*o)).collect(),
    }
}

fn person(alias: &str, originals: &[&str]) -> ExportedIdentity {
    identity(alias, Category::Person, originals)
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

fn new_document(vault: &Vault, project: Option<&str>, title: &str, markdown: &str) -> String {
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

/// A reviewed document as a save leaves it: its text in aliases, in a
/// Project of its own that knows its names.
fn saved(
    vault: &Vault,
    markdown: &str,
    names: &[ExportedIdentity],
    title: &str,
) -> (String, String) {
    let project = new_project(vault, title, names);
    let document = new_document(vault, Some(&project.id), title, markdown);
    (project.id, document)
}

fn restored(mapping: &Mapping, reply: &str) -> String {
    RestoreMap::new(mapping.exported())
        .restore_in_place(reply)
        .text
}

fn restored_in(vault: &Vault, project: &str, reply: &str) -> String {
    restored(&vault.project(project).unwrap().mapping, reply)
}

fn text(vault: &Vault, document: &str) -> String {
    vault.document(document).unwrap().markdown
}

fn aliases(mapping: &Mapping, ids: &[String]) -> Vec<String> {
    ids.iter()
        .map(|id| mapping.identity(id).unwrap().alias.clone())
        .collect()
}

fn ids_in(vault: &Vault, project: Option<&str>) -> Vec<String> {
    let mut ids: Vec<_> = vault
        .documents_in(project)
        .into_iter()
        .map(|d| d.id)
        .collect();
    ids.sort();
    ids
}

/// Every alias, current or earlier, names one identity, and every id one.
fn assert_aliases_unique(mapping: &Mapping) {
    let (mut aliases, mut ids) = (HashSet::new(), HashSet::new());
    for identity in &mapping.identities {
        assert!(ids.insert(identity.id.clone()), "{} twice", identity.id);
        for alias in std::iter::once(&identity.alias).chain(&identity.previous) {
            assert!(aliases.insert(alias.clone()), "{alias} names two entries");
        }
    }
}

fn is_id(id: &str) -> bool {
    id.len() == 32 && id.bytes().all(|b| matches!(b, b'0'..=b'9' | b'a'..=b'f'))
}

// The case store's mapping rules.

#[test]
fn identities_have_stable_random_ids() {
    let mut mapping = mapping(&[
        person("PERSON_1", &["Anna Petrova"]),
        identity("ORG_1", Category::Organization, &["Northwind Ltd"]),
    ]);
    let ids: Vec<_> = mapping.identities.iter().map(|i| i.id.clone()).collect();
    assert!(ids.iter().all(|id| is_id(id)), "{ids:?}");
    assert_ne!(ids[0], ids[1]);
    // A rename, a new variant and a category change keep the id.
    let mut renamed = person("BUYER", &["Anna Petrova", "A. Petrova"]);
    renamed.previous.push("PERSON_1".into());
    mapping.merge(&[renamed, person("ORG_1", &["Northwind Ltd"])]);
    assert_eq!(mapping.identity(&ids[0]).unwrap().alias, "BUYER");
    assert_eq!(mapping.identity(&ids[1]).unwrap().category, "PERSON");
    assert_eq!(mapping.identity_named("PERSON_1").unwrap().id, ids[0]);
}

/// Web address only added a token: every token saved before it reads back
/// as the same category, and a URL identity round trips.
#[test]
fn category_tokens_load_unchanged() {
    for token in [
        "PERSON", "ORG", "EMAIL", "PHONE", "ADDRESS", "IDENTITY", "TAX", "BANK", "CASE", "DATE",
        "REDACTED", "URL",
    ] {
        let stored: Identity = serde_json::from_value(serde_json::json!({
            "id": "0".repeat(32),
            "alias": format!("{token}_1"),
            "category": token,
            "originals": ["x"],
        }))
        .unwrap();
        let exported = stored.exported().unwrap();
        assert_eq!(exported.category.token(), token);
        let mut again = mapping(&[exported]);
        again.identities[0].id = stored.id.clone();
        assert_eq!(again.identities[0], stored);
    }
}

#[test]
fn a_projects_mapping_round_trips_sealed_and_never_in_plaintext() {
    let folder = Folder::new();
    let project = {
        let vault = folder.open();
        let project = new_project(&vault, "Matter", &[person("PERSON_1", &["Anna Petrova"])]);
        vault
            .update_project(&project.id, |p| {
                p.dismissed_hints
                    .push(("Ms Petrova".into(), "Anna Petrova".into()));
                p.lists.add_always("Acme Holdings", Category::Organization);
                p.skip.push("DATE".into());
            })
            .unwrap()
    };
    let reopened = folder.open().project(&project.id).unwrap();
    assert_eq!(reopened, project);
    assert_eq!(reopened.skipped(), HashSet::from([Category::Date]));
    assert_eq!(reopened.mapping.exported()[0].wording(), "Anna Petrova");
    let path = folder
        .root
        .join("projects")
        .join(format!("{}.enc", project.id));
    let bytes = String::from_utf8_lossy(&std::fs::read(path).unwrap()).into_owned();
    for secret in [
        "Anna",
        "Petrova",
        "PERSON_1",
        "Acme",
        &project.mapping.identities[0].id,
    ] {
        assert!(!bytes.contains(secret), "{secret} is stored in plaintext");
    }
}

#[test]
fn a_project_saved_before_mappings_opens_empty() {
    let mut json = serde_json::to_value(Project {
        id: "0".repeat(32),
        name: "Old".into(),
        colour: None,
        pinned: false,
        created: 1,
        updated: 1,
        revision: 1,
        mapping: Mapping::default(),
        lists: Default::default(),
        skip: Vec::new(),
        dismissed_hints: Vec::new(),
    })
    .unwrap();
    let object = json.as_object_mut().unwrap();
    for field in ["mapping", "lists", "skip", "dismissed_hints"] {
        object.remove(field).unwrap();
    }
    let old: Project = serde_json::from_value(json).unwrap();
    assert!(old.mapping.is_empty() && old.lists.is_empty() && old.dismissed_hints.is_empty());
}

#[test]
fn renames_exclude_absorbed_entries_and_follow_the_trailing_identity() {
    let before = mapping(&[
        person("PERSON_1", &["Maya Bennett"]),
        person("PERSON_2", &["Ms Bennett"]),
        identity("ORG_1", Category::Organization, &["Northwind Ltd"]),
    ]);
    let mut merged = person("PERSON_1", &["Maya Bennett", "Ms Bennett"]);
    merged.previous.push("PERSON_2".into());
    let mut after = before.clone();
    after.merge(&[merged]);
    assert_eq!(after.identities.len(), 2);
    assert!(after.renames_since(&before).is_empty());
    let mut org = identity("COMPANY", Category::Organization, &["Northwind Ltd"]);
    org.previous.push("ORG_1".into());
    after.merge(std::slice::from_ref(&org));
    after.identities.reverse();
    assert_eq!(after.renames_since(&before), [("ORG_1".into(), org)]);
}

#[test]
fn unmerged_documents_cannot_reverse_a_merge() {
    let mut document = vec![
        person("PERSON_1", &["Maya Bennett"]),
        person("PERSON_2", &["Ms Bennett"]),
    ];
    let mut mapping = mapping(&document);
    let mut merged = person("PERSON_1", &["Maya Bennett", "Ms Bennett"]);
    merged.previous.push("PERSON_2".into());
    mapping.merge(&[merged]);
    let expected = mapping.clone();
    for _ in 0..2 {
        mapping.merge(&document);
        assert_eq!(mapping, expected);
        assert!(mapping.renames_since(&expected).is_empty());
        document.reverse();
    }
}

#[test]
fn unmerged_document_renames_follow_the_merged_identity_in_either_order() {
    let mut merged = person("PERSON_1", &["Maya Bennett", "Ms Bennett"]);
    merged.previous.push("PERSON_2".into());
    let base = mapping(&[merged]);
    let id = base.identities[0].id.clone();
    // The unmerged document renames PERSON_1 forward: whichever entry it
    // exports first, the merged identity takes the new alias.
    let mut client = person("CLIENT", &["Maya Bennett"]);
    client.previous.push("PERSON_1".into());
    let short = person("PERSON_2", &["Ms Bennett"]);
    let mut mapping = base.clone();
    for document in [
        [client.clone(), short.clone()],
        [short.clone(), client.clone()],
    ] {
        mapping = base.clone();
        mapping.merge(&document);
        assert_eq!(mapping.identities.len(), 1);
        assert_eq!(mapping.identities[0].id, id);
        assert_eq!(mapping.identities[0].alias, "CLIENT");
        assert_eq!(mapping.identities[0].originals[0], "Maya Bennett");
        assert_eq!(mapping.renames_since(&base)[0].0, "PERSON_1");
    }
    // Undo in that document restores history that no longer remembers
    // CLIENT. The wording order decides which entry is the merged
    // identity, so the Undo propagates as PERSON_1 in either order.
    let renamed = mapping.clone();
    let undone = person("PERSON_1", &["Maya Bennett"]);
    for document in [
        [undone.clone(), short.clone()],
        [short.clone(), undone.clone()],
    ] {
        mapping = renamed.clone();
        mapping.merge(&document);
        assert_eq!(mapping.identities.len(), 1);
        let identity = &mapping.identities[0];
        assert_eq!(identity.alias, "PERSON_1");
        assert_eq!(identity.originals[0], "Maya Bennett");
        assert!(identity.names("PERSON_2") && identity.names("CLIENT"));
        let renames = mapping.renames_since(&renamed);
        assert_eq!(renames.len(), 1);
        assert_eq!(renames[0].0, "CLIENT");
        assert_eq!(renames[0].1.alias, "PERSON_1");
    }
}

#[test]
fn rename_correspondence_prefers_aliases_then_previous_then_originals() {
    // Homonyms kept apart by alias, built one at a time so the shared
    // original cannot fold them before the test starts.
    let mut before = mapping(&[person("PERSON_1", &["Alex Smith"])]);
    let other = mapping(&[person("PERSON_2", &["Alex Smith"])]);
    before.identities.extend(other.identities);
    let mut after = before.clone();
    after.identities.reverse();
    assert!(
        after.renames_since(&before).is_empty(),
        "same alias wins over a shared original"
    );
    let mut split = before.clone();
    let mut document = before.exported();
    document.push(person("PERSON_3", &["Alex Smith"]));
    split.merge(&document);
    assert!(
        split.renames_since(&before).is_empty(),
        "splits are not renames"
    );
    let new = &mut after.identities[0];
    new.previous
        .push(std::mem::replace(&mut new.alias, "CLIENT".into()));
    new.originals = vec!["Alexander Smith".into()];
    assert_eq!(
        after.renames_since(&before)[0].0,
        "PERSON_2",
        "earlier alias wins even without a shared original"
    );
    let before = mapping(&[person("PERSON_1", &["Northwind"])]);
    let after = mapping(&[identity("ORG_1", Category::Organization, &["Northwind"])]);
    assert_eq!(
        after.renames_since(&before),
        [("PERSON_1".into(), after.exported()[0].clone())]
    );
}

#[test]
fn forgotten_originals_cannot_return_from_a_stale_document() {
    let folder = Folder::new();
    let vault = folder.open();
    let project = new_project(
        &vault,
        "Petrova v Northwind",
        &[
            person("PERSON_1", &["Anna Petrova", "A. Petrova"]),
            identity("ORG_1", Category::Organization, &["Northwind Ltd"]),
        ],
    );
    let old = project.mapping.exported();
    let alias = old[0].alias.clone();
    let wording = old[0].wording().to_string();
    let mut forgot = false;
    let project = vault
        .update_project(&project.id, |p| {
            p.dismissed_hints = vec![
                ("Short form".into(), wording.clone()),
                (wording.clone(), "Other variant".into()),
                ("Unrelated short".into(), "Unrelated full".into()),
            ];
            forgot = p.forget_identity(&alias);
        })
        .unwrap();
    assert!(forgot);
    assert_eq!(
        project.dismissed_hints,
        [("Unrelated short".into(), "Unrelated full".into())]
    );
    let mut stale = project.clone();
    assert!(!stale.forget_identity(&alias));
    let project = vault
        .update_project(&project.id, |p| p.mapping.merge(&old))
        .unwrap();
    assert!(!project.mapping.exported().iter().any(|e| e.alias == alias));
    assert!(
        project
            .mapping
            .identity_named(&alias)
            .unwrap()
            .originals
            .is_empty()
    );
    assert!(project.mapping.reserved_aliases().contains(&alias));
    assert_ne!(project.mapping.next_alias(old[0].category), alias);
    drop(vault);
    assert_eq!(folder.open().project(&project.id).unwrap(), project);
}

#[test]
fn a_merge_into_a_later_identity_folds_the_earlier_one() {
    let folder = Folder::new();
    let vault = folder.open();
    let project = new_project(
        &vault,
        "Bennett",
        &[
            person("PERSON_1", &["Ms Bennett"]),
            identity("ORG_1", Category::Organization, &["Northwind Ltd"]),
            person("PERSON_2", &["Maya Bennett"]),
        ],
    );
    let kept = project
        .mapping
        .identity_named("PERSON_2")
        .unwrap()
        .id
        .clone();
    // The document merged PERSON_1 into PERSON_2, which keeps its alias.
    let mut merged = person("PERSON_2", &["Maya Bennett", "Ms Bennett"]);
    merged.previous = vec!["PERSON_1".into()];
    let project = vault
        .update_project(&project.id, |p| {
            p.mapping.merge(std::slice::from_ref(&merged))
        })
        .unwrap();
    let mapping = &project.mapping;
    assert_aliases_unique(mapping);
    let people: Vec<_> = (mapping.identities.iter())
        .filter(|i| i.category == "PERSON")
        .collect();
    assert_eq!(people.len(), 1, "{:?}", mapping.identities);
    assert_eq!(people[0].id, kept);
    assert_eq!(people[0].alias, "PERSON_2");
    assert_eq!(people[0].previous, ["PERSON_1"]);
    assert_eq!(people[0].originals, ["Maya Bennett", "Ms Bennett"]);
    assert_eq!(mapping.identities[0].alias, "ORG_1", "order otherwise kept");
    // Reopened, it keeps one entry; merging again changes nothing.
    drop(vault);
    let mut reopened = folder.open().project(&project.id).unwrap();
    assert_eq!(reopened, project);
    reopened.mapping.merge(&[merged]);
    assert_eq!(reopened.mapping, project.mapping);
    assert_eq!(reopened.mapping.next_alias(Category::Person), "PERSON_3");
    let exported = reopened.mapping.exported();
    assert_eq!(exported.len(), 2);
    assert!(
        exported
            .iter()
            .any(|e| e.alias == "PERSON_2" && e.previous == ["PERSON_1"])
    );
}

#[test]
fn a_merge_into_an_earlier_identity_keeps_one_entry() {
    let mut mapping = mapping(&[
        person("PERSON_1", &["Maya Bennett"]),
        person("PERSON_2", &["Ms Bennett"]),
    ]);
    let kept = mapping.identities[0].id.clone();
    let mut merged = person("PERSON_1", &["Maya Bennett", "Ms Bennett"]);
    merged.previous = vec!["PERSON_2".into()];
    mapping.merge(&[merged]);
    assert_aliases_unique(&mapping);
    assert_eq!(mapping.identities.len(), 1);
    assert_eq!(mapping.identities[0].id, kept);
    assert_eq!(mapping.identities[0].alias, "PERSON_1");
    assert_eq!(mapping.identities[0].previous, ["PERSON_2"]);
}

#[test]
fn merging_keeps_unused_identities_and_renamed_aliases() {
    let mut mapping = mapping(&[
        person("PERSON_1", &["Anna Petrova"]),
        identity("ORG_1", Category::Organization, &["Northwind Ltd"]),
    ]);
    // The document renamed PERSON_1, learned a variant and dropped ORG_1.
    let mut renamed = person("BUYER", &["Anna Petrova", "A. Petrova"]);
    renamed.custom = true;
    mapping.merge(&[renamed, person("PERSON_2", &["Ivan Ivanov"])]);
    assert_eq!(mapping.identities.len(), 3);
    let anna = &mapping.identities[0];
    assert_eq!(anna.alias, "BUYER");
    assert_eq!(anna.previous, ["PERSON_1"]);
    assert_eq!(anna.originals, ["Anna Petrova", "A. Petrova"]);
    assert_eq!(mapping.identities[1].alias, "ORG_1");
    assert_eq!(mapping.identities[2].alias, "PERSON_2");
    // Renaming back moves the alias, never duplicating it.
    mapping.merge(&[person("PERSON_1", &["Anna Petrova"])]);
    let anna = &mapping.identities[0];
    assert_eq!(
        (anna.alias.as_str(), anna.previous.as_slice()),
        ("PERSON_1", &["BUYER".to_string()][..])
    );
}

/// Not a name drops a wording from the identity its alias names only,
/// never a homonym's; the Document's undo refills it, and its own
/// reservation is not reserved for it.
#[test]
fn not_a_name_drops_one_identitys_wording_and_its_undo_refills_it() {
    let mut mapping = mapping(&[person("PERSON_1", &["Alex Kim"])]);
    let homonym = self::mapping(&[person("PERSON_2", &["Alex Kim"])]);
    mapping.identities.extend(homonym.identities);
    let (document, other) = ("a".repeat(32), "b".repeat(32));
    let pair = [("PERSON_1".to_string(), "Alex Kim".to_string())];
    mapping.drop_wordings(&pair, &document, |_| false);
    assert!(mapping.identities[0].originals.is_empty());
    assert_eq!(
        mapping.identities[0].not_a_name_by.as_deref(),
        Some(document.as_str())
    );
    assert_eq!(mapping.identities[1].originals, ["Alex Kim"]);
    assert!(mapping.reserved_aliases().contains(&"PERSON_1".to_string()));
    assert!(mapping.reserved_aliases_for(&document).is_empty());
    assert!(mapping.not_names.is_empty());

    mapping.refill(&pair, &other);
    assert!(
        mapping.identities[0].originals.is_empty(),
        "another document can't refill it"
    );
    mapping.refill(&pair, &document);
    assert_eq!(mapping.identities[0].originals, ["Alex Kim"]);

    // Two wordings of one identity come back together.
    mapping.identities[0].originals = vec!["Alex Kim".into(), "A. Kim".into()];
    let both = [
        ("PERSON_1".to_string(), "Alex Kim".to_string()),
        ("PERSON_1".to_string(), "A. Kim".to_string()),
    ];
    mapping.drop_wordings(&both, &document, |_| false);
    assert!(mapping.identities[0].originals.is_empty());
    mapping.refill(&both, &document);
    assert_eq!(mapping.identities[0].originals, ["Alex Kim", "A. Kim"]);
    mapping.identities[0].originals = vec!["Alex Kim".into()];

    // Kept for another Document's sake: noted for this one instead.
    mapping.drop_wordings(&pair, &document, |_| true);
    assert_eq!(mapping.identities[0].originals, ["Alex Kim"]);
    assert_eq!(
        mapping.not_names_of(&document),
        [("PERSON_1".to_string(), "Alex Kim".to_string())]
    );
    mapping.drop_wordings(&[], &document, |_| true);
    assert!(
        mapping.not_names.is_empty(),
        "an undone Not a name is no longer noted"
    );
}

/// A forgotten identity stays forgotten even for the Document whose Not a
/// name emptied part of it: its undo gives nothing back.
#[test]
fn a_forgotten_identity_is_not_refilled_by_a_not_a_name_undo() {
    let folder = Folder::new();
    let vault = folder.open();
    let names = [person("PERSON_1", &["Alex Kim", "A. Kim"])];
    let (project, document) = saved(&vault, "PERSON_1 wrote.", &names, "Case");
    let dropped = [("PERSON_1".to_string(), "A. Kim".to_string())];
    let mut forgot = false;
    let project = vault
        .update_project(&project, |p| {
            p.mapping.drop_wordings(&dropped, &document, |_| false);
            forgot = p.forget_identity("PERSON_1");
        })
        .unwrap();
    assert!(forgot);
    let mut mapping = project.mapping.clone();
    mapping.merge_from(&names, Some(&document));
    mapping.refill(&dropped, &document);
    assert!(mapping.identities[0].originals.is_empty());
    assert!((mapping.reserved_aliases_for(&document)).contains(&"PERSON_1".to_string()));
}

/// A note kept for another Document follows a rename of its identity; the
/// Document is named by its id, which no rename or move changes.
#[test]
fn not_a_name_notes_follow_renames() {
    let folder = Folder::new();
    let vault = folder.open();
    let (project, document) = saved(
        &vault,
        "PERSON_1 wrote.",
        &[person("PERSON_1", &["Alex Kim"])],
        "Case",
    );
    let pair = [("PERSON_1".to_string(), "Alex Kim".to_string())];
    let project = vault
        .update_project(&project, |p| {
            p.mapping.drop_wordings(&pair, &document, |_| true);
            let identity = &mut p.mapping.identities[0];
            identity.previous.push("PERSON_1".into());
            identity.alias = "Seller".into();
        })
        .unwrap();
    assert_eq!(
        project.mapping.not_names_of(&document),
        [("Seller".to_string(), "Alex Kim".to_string())]
    );
    let project = vault
        .update_project(&project.id, |p| {
            p.mapping.drop_wordings(&pair, &document, |_| false);
        })
        .unwrap();
    drop(vault);
    let reopened = folder.open().project(&project.id).unwrap();
    assert!(reopened.mapping.identities[0].originals.is_empty());
    assert_eq!(
        reopened.mapping.identities[0].not_a_name_by.as_deref(),
        Some(document.as_str())
    );
    assert!(reopened.mapping.reserved_aliases_for(&document).is_empty());
}

#[test]
fn user_lists_are_sealed_and_survive_a_reopen() {
    let folder = Folder::new();
    assert_eq!(folder.open().user_lists().unwrap(), UserLists::default());
    let saved = folder
        .open()
        .update_user_lists(|lists| {
            lists.add_never("Supreme Court");
            lists.add_always("Acme Holdings", Category::Organization);
        })
        .unwrap();
    assert_eq!(saved.revision, 1);
    // A second handle, or the next launch, reads them.
    assert_eq!(folder.open().user_lists().unwrap(), saved);
    let bytes = std::fs::read(folder.root.join("user-lists.enc")).unwrap();
    assert!(!String::from_utf8_lossy(&bytes).contains("Acme"));
    // They are no Project or Document.
    let vault = folder.open();
    assert!(vault.projects().is_empty() && vault.documents().is_empty());
    // With only the lists in it, another key opens nothing and writes
    // nothing.
    let manifest = std::fs::read(folder.root.join("manifest.enc")).unwrap();
    assert!(matches!(
        Vault::open(&folder.root, &[8; 32]),
        Err(VaultError::WrongKey)
    ));
    assert_eq!(
        std::fs::read(folder.root.join("manifest.enc")).unwrap(),
        manifest
    );
    // Lists sealed under another key in a Vault that opens are damaged,
    // and never overwritten.
    let foreign = Folder::new();
    Vault::open(&foreign.root, &[8; 32])
        .unwrap()
        .update_user_lists(|lists| {
            lists.add_never("Elsewhere");
        })
        .unwrap();
    let other = Folder::new();
    new_project(&other.open(), "Matter", &[]);
    let copied = std::fs::read(foreign.root.join("user-lists.enc")).unwrap();
    std::fs::write(other.root.join("user-lists.enc"), &copied).unwrap();
    let other_vault = other.open();
    assert!(matches!(
        other_vault.user_lists(),
        Err(VaultError::Damaged {
            kind: ObjectKind::UserLists,
            ..
        })
    ));
    assert!(other_vault.update_user_lists(|_| {}).is_err());
    assert_eq!(
        std::fs::read(other.root.join("user-lists.enc")).unwrap(),
        copied,
        "never overwritten"
    );
    // An older revision never overwrites a newer one.
    let mut newer = saved.clone();
    newer.revision = 3;
    newer.lists.add_never("Riverside");
    assert_eq!(vault.save_user_lists(&newer).unwrap(), Saved::Written);
    assert_eq!(vault.save_user_lists(&saved).unwrap(), Saved::Skipped);
    assert_eq!(folder.open().user_lists().unwrap(), newer);
    assert_eq!(vault.update_user_lists(|_| {}).unwrap().revision, 4);
}

#[test]
fn a_project_keeps_its_own_lists() {
    let folder = Folder::new();
    let vault = folder.open();
    let a = new_project(&vault, "A", &[]);
    let b = new_project(&vault, "B", &[]);
    vault
        .update_project(&a.id, |p| {
            p.lists.add_never("Maya Bennett");
        })
        .unwrap();
    drop(vault);
    let vault = folder.open();
    assert!(
        vault
            .project(&a.id)
            .unwrap()
            .lists
            .never_has("maya bennett")
    );
    assert!(vault.project(&b.id).unwrap().lists.is_empty());
}

/// A Document in a Project shares its mapping and keeps none of its own.
#[test]
fn a_document_in_a_project_keeps_no_mapping_of_its_own() {
    let folder = Folder::new();
    let vault = folder.open();
    let (_, filed) = saved(
        &vault,
        "PERSON_1 wrote.",
        &[person("PERSON_1", &["Ann Lee"])],
        "A",
    );
    let names = [person("PERSON_1", &["Ann Lee"])];
    let filed = vault
        .update_document(&filed, |d| d.mapping.merge(&names))
        .unwrap();
    assert!(filed.mapping.is_empty());
    let unfiled = new_document(&vault, None, "Note", "PERSON_1 wrote.");
    let unfiled = vault
        .update_document(&unfiled, |d| d.mapping.merge(&names))
        .unwrap();
    assert_eq!(unfiled.mapping.exported(), names);
    drop(vault);
    assert_eq!(folder.open().document(&unfiled.id).unwrap(), unfiled);
}

// The mobile core's move.

/// Moving re-aliases the document for the Project it joins (`plan_join`):
/// a shared name takes the Project's alias, a clashing one the next free
/// number, and the Project's other names are proposed. The Project it left
/// keeps its names. Moving back gives the names their old aliases and
/// takes out of the Project it leaves the names the move added there.
#[test]
fn a_moved_document_takes_the_projects_aliases() {
    let folder = Folder::new();
    let vault = folder.open();
    let (letter_project, letter) = saved(
        &vault,
        "PERSON_1 wrote to PERSON_2 about Carl Diaz.",
        &[
            person("PERSON_1", &["Ann Lee"]),
            person("PERSON_2", &["Bob Hart"]),
        ],
        "Letter",
    );
    vault
        .update_document(&letter, |d| d.ready_fingerprint = Some("ab12".into()))
        .unwrap();
    let (memo_project, memo) = saved(
        &vault,
        "PERSON_1 met PERSON_2.",
        &[
            person("PERSON_1", &["Bob Hart"]),
            person("PERSON_2", &["Carl Diaz"]),
        ],
        "Memo",
    );
    let bob = (vault.project(&memo_project).unwrap().mapping)
        .identity_named("PERSON_1")
        .unwrap()
        .id
        .clone();
    let stale = vault.document(&letter).unwrap();
    let moved = vault.move_document(&letter, Some(&memo_project)).unwrap();
    let joined = moved.project.clone().unwrap();
    assert_eq!(joined.id, memo_project);
    assert_eq!(
        moved.document.project.as_deref(),
        Some(memo_project.as_str())
    );
    assert!(moved.document.mapping.is_empty());
    assert_eq!(
        moved.document.ready_fingerprint, None,
        "readiness leaves with it"
    );
    assert_eq!(ids_in(&vault, Some(&memo_project)), {
        let mut both = vec![memo.clone(), letter.clone()];
        both.sort();
        both
    });
    assert_eq!(
        aliases(&joined.mapping, &moved.undo.added),
        ["PERSON_3"],
        "Ann; Bob and Carl were known"
    );
    assert_eq!(joined.mapping.identity(&bob).unwrap().alias, "PERSON_1");
    assert_eq!(joined.mapping.identities.len(), 3);
    assert_aliases_unique(&joined.mapping);
    let left = moved.left.clone().unwrap();
    assert_eq!(left.id, letter_project);
    assert!(ids_in(&vault, Some(&letter_project)).is_empty());
    assert_eq!(
        text(&vault, &letter),
        "PERSON_3 wrote to PERSON_1 about Carl Diaz."
    );
    assert_eq!(
        restored_in(&vault, &memo_project, "PERSON_3, PERSON_1, PERSON_2."),
        "Ann Lee, Bob Hart, Carl Diaz."
    );
    // The Project left restores replies to what was sent before.
    assert_eq!(
        restored_in(&vault, &letter_project, "PERSON_1 thanks PERSON_2."),
        "Ann Lee thanks Bob Hart."
    );
    // A copy from before the move cannot undo it.
    assert_eq!(vault.save_document(&stale).unwrap(), Saved::Skipped);
    assert_eq!(
        text(&vault, &letter),
        "PERSON_3 wrote to PERSON_1 about Carl Diaz."
    );
    // Undo: moving back gives the names their old aliases, and Memo's
    // Project forgets Ann.
    let back = vault.undo_move(&moved.undo).unwrap();
    assert_eq!(back.project.unwrap().id, letter_project);
    assert_eq!(back.left.clone().unwrap().id, memo_project);
    assert_eq!(
        ids_in(&vault, Some(&memo_project)),
        std::slice::from_ref(&memo)
    );
    assert_eq!(
        text(&vault, &letter),
        "PERSON_1 wrote to PERSON_2 about Carl Diaz."
    );
    assert_eq!(
        restored_in(&vault, &letter_project, "PERSON_1, PERSON_2."),
        "Ann Lee, Bob Hart."
    );
    let memo_mapping = vault.project(&memo_project).unwrap().mapping;
    assert!(
        memo_mapping
            .identities
            .iter()
            .all(|i| i.alias != "PERSON_3")
    );
    assert_eq!(memo_mapping.identities.len(), 2);
    assert_eq!(back.left.unwrap().mapping, memo_mapping);
}

/// The Project a document leaves stays, even empty (a Project is deleted
/// only on its own). Moving back keeps the names the move added where
/// something still uses them: another document of that Project, or a
/// reply to text shared from it, which the caller knows of.
#[test]
fn a_project_left_stays_and_moving_back_keeps_names_in_use() {
    let folder = Folder::new();
    let vault = folder.open();
    let (letter_project, letter) = saved(
        &vault,
        "PERSON_1 wrote.",
        &[person("PERSON_1", &["Ann Lee"])],
        "Letter",
    );
    let (memo_project, _) = saved(
        &vault,
        "PERSON_1 wrote.",
        &[person("PERSON_1", &["Bob Hart"])],
        "Memo",
    );
    let moved = vault.move_document(&letter, Some(&memo_project)).unwrap();
    let memo_mapping = &moved.project.as_ref().unwrap().mapping;
    assert_eq!(aliases(memo_mapping, &moved.undo.added), ["PERSON_2"]);
    assert!(ids_in(&vault, Some(&letter_project)).is_empty());
    assert_eq!(vault.projects().len(), 2);
    assert_eq!(restored_in(&vault, &letter_project, "PERSON_1"), "Ann Lee");
    assert_eq!(text(&vault, &letter), "PERSON_2 wrote.");
    // Another document of Memo's Project names Ann now.
    new_document(&vault, Some(&memo_project), "Note", "PERSON_2 called.");
    let back = vault.undo_move(&moved.undo).unwrap();
    assert_eq!(back.project.unwrap().id, letter_project);
    assert_eq!(text(&vault, &letter), "PERSON_1 wrote.");
    assert_eq!(restored_in(&vault, &memo_project, "PERSON_2"), "Ann Lee");
    // Moved into a Project shared from since: a reply may name Ann there,
    // so the caller moves it back without the names it added.
    let (other_project, _) = saved(
        &vault,
        "PERSON_1 wrote.",
        &[person("PERSON_1", &["Dan Roe"])],
        "Other",
    );
    let moved = vault.move_document(&letter, Some(&other_project)).unwrap();
    let other_mapping = &moved.project.as_ref().unwrap().mapping;
    assert_eq!(aliases(other_mapping, &moved.undo.added), ["PERSON_2"]);
    let shared = MoveUndo {
        added: Vec::new(),
        ..moved.undo.clone()
    };
    vault.undo_move(&shared).unwrap();
    assert_eq!(restored_in(&vault, &other_project, "PERSON_2"), "Ann Lee");
    // Moving back into the Project it is in is refused, as a move is.
    assert!(matches!(
        vault.undo_move(&moved.undo),
        Err(VaultError::AlreadyThere)
    ));
}

#[test]
fn a_move_into_where_it_is_or_a_missing_project_is_refused() {
    let folder = Folder::new();
    let vault = folder.open();
    let (project, letter) = saved(
        &vault,
        "PERSON_1 wrote.",
        &[person("PERSON_1", &["Ann Lee"])],
        "Letter",
    );
    assert!(matches!(
        vault.move_document(&letter, Some(&project)),
        Err(VaultError::AlreadyThere)
    ));
    assert!(matches!(
        vault.move_document(&letter, Some(&"0".repeat(32))),
        Err(VaultError::NotFound {
            kind: ObjectKind::Project,
            ..
        })
    ));
    assert!(matches!(
        vault.move_document(&letter, Some("not an id")),
        Err(VaultError::InvalidId)
    ));
    assert!(matches!(
        vault.move_document("not an id", None),
        Err(VaultError::InvalidId)
    ));
    let unfiled = new_document(&vault, None, "Note", "Ann Lee");
    assert!(matches!(
        vault.move_document(&unfiled, None),
        Err(VaultError::AlreadyThere)
    ));
    assert_eq!(text(&vault, &letter), "PERSON_1 wrote.");
    assert_eq!(ids_in(&vault, Some(&project)), [letter]);
    assert_eq!(vault.projects().len(), 1);
}

/// New Project…: the document's names keep their aliases in a Project of
/// their own; the Project it left stays.
#[test]
fn a_document_moves_into_a_new_project() {
    let folder = Folder::new();
    let vault = folder.open();
    let (project, letter) = saved(
        &vault,
        "PERSON_1 wrote.",
        &[person("PERSON_1", &["Ann Lee"])],
        "Letter",
    );
    let ann = vault.project(&project).unwrap().mapping.identities[0]
        .id
        .clone();
    let moved = vault
        .move_document_to_new_project(
            &letter,
            NewProject {
                name: "Ivanov".into(),
                ..NewProject::default()
            },
        )
        .unwrap();
    let joined = moved.project.unwrap();
    assert_eq!(joined.name, "Ivanov");
    assert_eq!(joined.mapping.exported().len(), 1);
    assert_eq!(aliases(&joined.mapping, &moved.undo.added), ["PERSON_1"]);
    assert_eq!(moved.undo.added, [ann], "the name keeps its id");
    assert_eq!(text(&vault, &letter), "PERSON_1 wrote.");
    assert_eq!(restored_in(&vault, &joined.id, "PERSON_1"), "Ann Lee");
    assert_eq!(vault.projects().len(), 2);
    assert!(ids_in(&vault, Some(&project)).is_empty());
}

/// A document that cannot be written leaves it where it was, and the
/// Project it was joining as it was; a new Project made for it goes.
#[test]
#[cfg(unix)]
fn a_move_whose_document_cannot_be_written_changes_nothing() {
    use std::os::unix::fs::PermissionsExt;
    let folder = Folder::new();
    let vault = folder.open();
    let (letter_project, letter) = saved(
        &vault,
        "PERSON_1 wrote.",
        &[person("PERSON_1", &["Ann Lee"])],
        "Letter",
    );
    let (memo_project, _) = saved(
        &vault,
        "PERSON_1 wrote.",
        &[person("PERSON_1", &["Bob Hart"])],
        "Memo",
    );
    let before = vault.project(&memo_project).unwrap();
    let dir = folder.root.join("docs").join(&letter);
    let mode =
        |mode| std::fs::set_permissions(&dir, std::fs::Permissions::from_mode(mode)).unwrap();
    mode(0o555);
    if std::fs::File::create(dir.join("probe")).is_ok() {
        mode(0o755);
        return; // Running as root: permissions do not apply.
    }
    let moved = vault.move_document(&letter, Some(&memo_project));
    let to_new = vault.move_document_to_new_project(
        &letter,
        NewProject {
            name: "New".into(),
            ..NewProject::default()
        },
    );
    mode(0o755);
    assert!(matches!(moved, Err(VaultError::Io(_))), "{moved:?}");
    assert!(matches!(to_new, Err(VaultError::Io(_))), "{to_new:?}");
    assert_eq!(text(&vault, &letter), "PERSON_1 wrote.");
    assert_eq!(
        vault.project(&memo_project).unwrap().mapping,
        before.mapping
    );
    assert_eq!(ids_in(&vault, Some(&letter_project)), [letter]);
    assert_eq!(vault.projects().len(), 2, "no new Project is left");
    drop(vault);
    assert_eq!(folder.open().projects().len(), 2);
}

/// A crash between the move's writes loses nothing: the document stays
/// where it was, in its old aliases, and moving it again finishes.
#[test]
fn a_crash_during_a_move_loses_nothing() {
    let folder = Folder::new();
    let (letter_project, letter, memo_project) = {
        let vault = folder.open();
        let (letter_project, letter) = saved(
            &vault,
            "PERSON_1 wrote to PERSON_2.",
            &[
                person("PERSON_1", &["Ann Lee"]),
                person("PERSON_2", &["Bob Hart"]),
            ],
            "Letter",
        );
        let (memo_project, _) = saved(
            &vault,
            "PERSON_1 met.",
            &[person("PERSON_1", &["Bob Hart"])],
            "Memo",
        );
        (letter_project, letter, memo_project)
    };
    // Before or after the Project joined is written, or before the
    // Document is: after the Document is written, the move has happened.
    let crashes = [
        (
            ObjectKind::Project,
            memo_project.clone(),
            Stage::BeforeRename,
        ),
        (
            ObjectKind::Project,
            memo_project.clone(),
            Stage::AfterRename,
        ),
        (ObjectKind::Document, letter.clone(), Stage::BeforeRename),
    ];
    for (kind, id, stage) in crashes {
        let armed = Arc::new(AtomicBool::new(true));
        let hook: FaultHook = Arc::new(move |step| {
            if (step.kind, step.id, step.stage) == (kind, id.as_str(), stage)
                && armed.swap(false, Ordering::SeqCst)
            {
                return Fault::Crash;
            }
            Fault::Continue
        });
        let vault = Vault::open_with_hook(&folder.root, &KEY, hook).unwrap();
        assert!(matches!(
            vault.move_document(&letter, Some(&memo_project)),
            Err(VaultError::Crashed)
        ));
        drop(vault);
        let vault = folder.open();
        assert_eq!(text(&vault, &letter), "PERSON_1 wrote to PERSON_2.");
        assert_eq!(
            ids_in(&vault, Some(&letter_project)),
            std::slice::from_ref(&letter)
        );
    }
    let vault = folder.open();
    let moved = vault.move_document(&letter, Some(&memo_project)).unwrap();
    assert_eq!(moved.document.markdown, "PERSON_3 wrote to PERSON_1.");
    assert_eq!(
        restored_in(&vault, &memo_project, "PERSON_3, PERSON_1."),
        "Ann Lee, Bob Hart."
    );
    assert_aliases_unique(&vault.project(&memo_project).unwrap().mapping);
}

// Unfiled.

/// An Unfiled document's own mapping follows it into a Project: its names
/// keep their ids there, and moving back gives it its mapping again.
#[test]
fn an_unfiled_documents_mapping_follows_it_into_a_project() {
    let folder = Folder::new();
    let vault = folder.open();
    let note = new_document(&vault, None, "Note", "PERSON_1 wrote to PERSON_2.");
    let local = mapping(&[
        person("PERSON_1", &["Ann Lee"]),
        person("PERSON_2", &["Bob Hart"]),
    ]);
    let note_doc = vault
        .update_document(&note, |d| d.mapping = local.clone())
        .unwrap();
    assert_eq!(note_doc.mapping, local);
    let (memo_project, _) = saved(
        &vault,
        "PERSON_1 met PERSON_2.",
        &[
            person("PERSON_1", &["Bob Hart"]),
            person("PERSON_2", &["Carl Diaz"]),
        ],
        "Memo",
    );
    let ann = local.identity_named("PERSON_1").unwrap().id.clone();
    let moved = vault.move_document(&note, Some(&memo_project)).unwrap();
    assert_eq!(moved.left, None);
    assert_eq!(moved.document.markdown, "PERSON_3 wrote to PERSON_1.");
    assert!(moved.document.mapping.is_empty());
    let joined = moved.project.clone().unwrap();
    assert_eq!(moved.undo.added, std::slice::from_ref(&ann));
    assert_eq!(joined.mapping.identity(&ann).unwrap().alias, "PERSON_3");
    assert_eq!(moved.undo.mapping, local);
    assert!(ids_in(&vault, None).is_empty());
    let back = vault.undo_move(&moved.undo).unwrap();
    assert_eq!(back.project, None);
    assert_eq!(back.document.markdown, "PERSON_1 wrote to PERSON_2.");
    // Its names, aliases and ids; Bob also answers to the Memo's Bob, so a
    // reply saved for it there still finds him.
    let memo_bob = joined
        .mapping
        .identity_named("PERSON_1")
        .unwrap()
        .id
        .clone();
    let mut expected = local.clone();
    expected.identities[1].folded = vec![memo_bob];
    assert_eq!(back.document.mapping, expected);
    assert_eq!(ids_in(&vault, None), std::slice::from_ref(&note));
    let memo_mapping = vault.project(&memo_project).unwrap().mapping;
    assert!(memo_mapping.identity(&ann).is_none(), "Ann goes with it");
    assert_eq!(back.left.unwrap().mapping, memo_mapping);
    drop(vault);
    assert_eq!(folder.open().document(&note).unwrap().mapping, expected);
}

/// A document moved out of a Project keeps, as its own mapping, the names
/// its text uses, with their ids; the Project keeps them all.
#[test]
fn a_document_moved_to_unfiled_keeps_the_names_it_uses() {
    let folder = Folder::new();
    let vault = folder.open();
    let (project, memo) = saved(
        &vault,
        "PERSON_1 met PERSON_2 and Carl Diaz.",
        &[
            person("PERSON_1", &["Bob Hart"]),
            person("PERSON_2", &["Ann Lee"]),
            person("PERSON_3", &["Carl Diaz"]),
            person("PERSON_4", &["Dan Roe"]),
        ],
        "Memo",
    );
    let before = vault.project(&project).unwrap().mapping;
    let moved = vault.move_document(&memo, None).unwrap();
    assert_eq!(moved.project, None);
    assert_eq!(moved.document.project, None);
    assert_eq!(
        moved.document.markdown,
        "PERSON_1 met PERSON_2 and Carl Diaz."
    );
    let local = &moved.document.mapping;
    let kept: Vec<_> = (local.identities.iter())
        .map(|i| (i.alias.as_str(), i.id.as_str()))
        .collect();
    let id = |alias| before.identity_named(alias).unwrap().id.as_str();
    assert_eq!(
        kept,
        [
            ("PERSON_1", id("PERSON_1")),
            ("PERSON_2", id("PERSON_2")),
            ("PERSON_3", id("PERSON_3")),
        ],
        "the names it uses, proposed ones too; no Dan"
    );
    assert!(local.identities.iter().all(|i| i.previous.is_empty()));
    assert_eq!(
        restored(local, "PERSON_2 and PERSON_1"),
        "Ann Lee and Bob Hart"
    );
    assert_eq!(
        moved.left.unwrap().mapping,
        before,
        "the Project keeps them"
    );
    assert_eq!(ids_in(&vault, None), std::slice::from_ref(&memo));
    let back = vault.undo_move(&moved.undo).unwrap();
    assert_eq!(back.document.project.as_deref(), Some(project.as_str()));
    assert!(back.document.mapping.is_empty());
    assert_eq!(
        back.document.markdown,
        "PERSON_1 met PERSON_2 and Carl Diaz."
    );
    assert_eq!(vault.project(&project).unwrap().mapping, before);
}

/// A wording a Document took off as Not a name, kept in the Project for
/// another Document, does not come back as a name when it moves.
#[test]
fn a_moved_document_leaves_its_not_a_name_wordings_off() {
    let folder = Folder::new();
    let vault = folder.open();
    let (project, _) = saved(
        &vault,
        "PERSON_1 wrote.",
        &[person("PERSON_1", &["Alex Kim"])],
        "Case",
    );
    let street = new_document(&vault, Some(&project), "Street", "Alex Kim Street is long.");
    let pair = [("PERSON_1".to_string(), "Alex Kim".to_string())];
    vault
        .update_project(&project, |p| {
            p.mapping.drop_wordings(&pair, &street, |_| true)
        })
        .unwrap();
    let moved = vault.move_document(&street, None).unwrap();
    assert!(
        moved.document.mapping.is_empty(),
        "{:?}",
        moved.document.mapping
    );
    assert_eq!(moved.document.markdown, "Alex Kim Street is long.");
}

/// A Document that does not open may be in the Project, so moving back
/// keeps the names the move added.
#[test]
fn moving_back_keeps_added_names_while_a_document_does_not_open() {
    let folder = Folder::new();
    let vault = folder.open();
    let (_, letter) = saved(
        &vault,
        "PERSON_1 wrote.",
        &[person("PERSON_1", &["Ann Lee"])],
        "Letter",
    );
    let (memo_project, _) = saved(
        &vault,
        "PERSON_1 wrote.",
        &[person("PERSON_1", &["Bob Hart"])],
        "Memo",
    );
    let moved = vault.move_document(&letter, Some(&memo_project)).unwrap();
    let note = new_document(&vault, Some(&memo_project), "Note", "PERSON_2 called.");
    let file = folder.root.join("docs").join(&note).join("doc.enc");
    let mut bytes = std::fs::read(&file).unwrap();
    let last = bytes.len() - 1;
    bytes[last] ^= 1;
    std::fs::write(&file, bytes).unwrap();
    vault.undo_move(&moved.undo).unwrap();
    assert_eq!(restored_in(&vault, &memo_project, "PERSON_2"), "Ann Lee");
}

/// A crash part way through moving into a new Project leaves the document
/// where it was; at most the new Project stays, empty.
#[test]
fn a_crash_moving_into_a_new_project_loses_nothing() {
    let folder = Folder::new();
    let (project, letter) = {
        let vault = folder.open();
        saved(
            &vault,
            "PERSON_1 wrote.",
            &[person("PERSON_1", &["Ann Lee"])],
            "Letter",
        )
    };
    let hook: FaultHook = {
        let letter = letter.clone();
        Arc::new(move |step| {
            if step.kind == ObjectKind::Document && step.id == letter {
                return Fault::Crash;
            }
            Fault::Continue
        })
    };
    let vault = Vault::open_with_hook(&folder.root, &KEY, hook).unwrap();
    let new = NewProject {
        name: "New".into(),
        ..NewProject::default()
    };
    assert!(matches!(
        vault.move_document_to_new_project(&letter, new),
        Err(VaultError::Crashed)
    ));
    drop(vault);
    let vault = folder.open();
    assert_eq!(text(&vault, &letter), "PERSON_1 wrote.");
    assert_eq!(
        ids_in(&vault, Some(&project)),
        std::slice::from_ref(&letter)
    );
    for other in vault.projects().into_iter().filter(|p| p.id != project) {
        assert_eq!(other.name, "New");
        assert!(ids_in(&vault, Some(&other.id)).is_empty());
    }
}

// Deleting a Project.

/// A crash part way through deleting a Project keeps it with the
/// Documents not yet moved; deleting again finishes.
#[test]
fn a_crash_deleting_a_project_is_finished_by_deleting_again() {
    let folder = Folder::new();
    let (project, documents) = {
        let vault = folder.open();
        let names = [person("PERSON_1", &["Ann Lee"])];
        let (project, first) = saved(&vault, "PERSON_1 wrote.", &names, "Case");
        let second = new_document(&vault, Some(&project), "Second", "PERSON_1 called.");
        let mut documents = vec![first, second];
        documents.sort();
        (project, documents)
    };
    let writes = Arc::new(std::sync::atomic::AtomicUsize::new(0));
    let hook: FaultHook = {
        let writes = writes.clone();
        Arc::new(move |step| {
            let document = step.kind == ObjectKind::Document && step.stage == Stage::BeforeRename;
            if document && writes.fetch_add(1, Ordering::SeqCst) == 1 {
                return Fault::Crash;
            }
            Fault::Continue
        })
    };
    let vault = Vault::open_with_hook(&folder.root, &KEY, hook).unwrap();
    assert!(matches!(
        vault.delete_project(&project, ProjectDocuments::MoveToUnfiled),
        Err(VaultError::Crashed)
    ));
    drop(vault);
    let vault = folder.open();
    assert!(vault.project(&project).is_ok());
    assert_eq!(ids_in(&vault, Some(&project)).len(), 1);
    assert_eq!(ids_in(&vault, None).len(), 1);
    vault
        .delete_project(&project, ProjectDocuments::MoveToUnfiled)
        .unwrap();
    assert!(vault.projects().is_empty());
    assert_eq!(ids_in(&vault, None), documents);
    for document in &documents {
        let document = vault.document(document).unwrap();
        assert_eq!(restored(&document.mapping, "PERSON_1"), "Ann Lee");
    }
}

/// A Document that cannot be read for now may be in the Project: the
/// delete is refused and nothing goes.
#[test]
#[cfg(unix)]
fn a_document_unreadable_for_now_refuses_a_project_delete() {
    use std::os::unix::fs::PermissionsExt;
    let folder = Folder::new();
    let vault = folder.open();
    let (project, document) = saved(
        &vault,
        "PERSON_1 wrote.",
        &[person("PERSON_1", &["Ann Lee"])],
        "Case",
    );
    let file = folder.root.join("docs").join(&document).join("doc.enc");
    let mode =
        |mode| std::fs::set_permissions(&file, std::fs::Permissions::from_mode(mode)).unwrap();
    mode(0o000);
    if std::fs::read(&file).is_ok() {
        mode(0o644);
        return; // Running as root: permissions do not apply.
    }
    let deleted = vault.delete_project(&project, ProjectDocuments::Delete);
    mode(0o644);
    assert!(matches!(deleted, Err(VaultError::Io(_))), "{deleted:?}");
    drop(vault);
    let vault = folder.open();
    assert!(vault.project(&project).is_ok());
    assert_eq!(ids_in(&vault, Some(&project)), [document]);
}

#[test]
fn deleting_a_project_moves_its_documents_to_unfiled_or_deletes_them() {
    let folder = Folder::new();
    let vault = folder.open();
    let (kept_project, brief) = saved(
        &vault,
        "PERSON_1 wrote.",
        &[
            person("PERSON_1", &["Ann Lee"]),
            person("PERSON_2", &["Bob Hart"]),
        ],
        "Kept",
    );
    let notes = new_document(&vault, Some(&kept_project), "Notes", "No names.");
    let stale = vault.project(&kept_project).unwrap();
    vault
        .delete_project(&kept_project, ProjectDocuments::MoveToUnfiled)
        .unwrap();
    assert!(vault.projects().is_empty());
    assert!(matches!(
        vault.project(&kept_project),
        Err(VaultError::NotFound { .. })
    ));
    assert_eq!(vault.save_project(&stale).unwrap(), Saved::Skipped);
    let mut both = vec![brief.clone(), notes.clone()];
    both.sort();
    assert_eq!(ids_in(&vault, None), both);
    let brief_doc = vault.document(&brief).unwrap();
    assert_eq!(brief_doc.markdown, "PERSON_1 wrote.");
    assert_eq!(restored(&brief_doc.mapping, "PERSON_1"), "Ann Lee");
    assert_eq!(
        brief_doc.mapping.identities.len(),
        1,
        "only the names it uses"
    );
    assert!(vault.document(&notes).unwrap().mapping.is_empty());

    let (gone_project, gone) = saved(
        &vault,
        "PERSON_1 wrote.",
        &[person("PERSON_1", &["Ann Lee"])],
        "Gone",
    );
    vault
        .delete_project(&gone_project, ProjectDocuments::Delete)
        .unwrap();
    assert!(matches!(
        vault.document(&gone),
        Err(VaultError::NotFound { .. })
    ));
    assert_eq!(ids_in(&vault, None), both);
    assert!(matches!(
        vault.delete_project(&gone_project, ProjectDocuments::Delete),
        Err(VaultError::NotFound { .. })
    ));
    drop(vault);
    let vault = folder.open();
    assert!(vault.projects().is_empty());
    assert_eq!(ids_in(&vault, None), both);
}

#[test]
fn a_move_clears_the_documents_saved_review() {
    let folder = Folder::new();
    let vault = folder.open();
    let review = Some(mdoc_pii::Review::default().save());
    let counts = Some(mdoc_vault::ReviewCounts {
        proposed: 1,
        replaced: 2,
        kept: 0,
    });
    let (project, _) = saved(
        &vault,
        "PERSON_1 wrote.",
        &[person("PERSON_1", &["Ann Lee"])],
        "Brief",
    );
    let memo = new_document(&vault, None, "Memo", "Ann met Bob.");
    let summary = |vault: &Vault| {
        (vault.documents().into_iter())
            .find(|d| d.id == memo)
            .unwrap()
            .counts
    };
    vault
        .update_document(&memo, |d| {
            d.review = review.clone();
            d.counts = counts;
        })
        .unwrap();
    assert_eq!(summary(&vault), counts);
    let moved = vault.move_document(&memo, Some(&project)).unwrap();
    assert_eq!(moved.document.review, None);
    assert_eq!(vault.document(&memo).unwrap().review, None);
    // The counts were of that review.
    assert_eq!(vault.document(&memo).unwrap().counts, None);
    assert_eq!(summary(&vault), None);

    vault
        .update_document(&memo, |d| {
            d.review = review.clone();
            d.counts = counts;
        })
        .unwrap();
    vault
        .delete_project(&project, ProjectDocuments::MoveToUnfiled)
        .unwrap();
    assert_eq!(vault.document(&memo).unwrap().review, None);
    assert_eq!(vault.document(&memo).unwrap().counts, None);
    drop(vault);
    assert_eq!(summary(&folder.open()), None);
}
