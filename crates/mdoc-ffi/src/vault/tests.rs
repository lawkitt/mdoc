//! A Project page's Always and Never lists through the exported `Vault`
//! (#1091, Testing Decisions seam 2), on a temp folder with an injected
//! key.
use super::*;
use crate::test_support::Scratch;
use mdoc_pii::Category as PiiCategory;

const KEY: [u8; 32] = [7; 32];

fn vault(scratch: &Scratch) -> Arc<Vault> {
    Vault::open(
        scratch.path().join("vault").display().to_string(),
        KEY.to_vec(),
    )
    .unwrap()
}

/// A Project whose lists hold "Riverside Ltd" (Always, an organization),
/// "Maya Ellison" (Always, a person) and "Plaintiff" (Never).
fn listed(vault: &Vault) -> String {
    let project = vault.create_project("Riverside".into()).unwrap();
    vault
        .store()
        .update_project(&project.id, |project| {
            project
                .lists
                .add_always("Riverside Ltd", PiiCategory::Organization);
            project
                .lists
                .add_always("Maya Ellison", PiiCategory::Person);
            project.lists.add_never("Plaintiff");
        })
        .unwrap();
    project.id
}

fn always(page: &ProjectPage) -> Vec<(&str, Category)> {
    (page.always.iter())
        .map(|entry| (entry.wording.as_str(), entry.category))
        .collect()
}

#[test]
fn a_project_page_shows_its_always_and_never_lists() {
    let scratch = Scratch::new();
    let vault = vault(&scratch);
    let id = listed(&vault);
    let page = vault.project_page(id).unwrap();
    assert_eq!(
        always(&page),
        [
            ("Riverside Ltd", Category::Organization),
            ("Maya Ellison", Category::Person)
        ]
    );
    assert_eq!(page.never, ["Plaintiff"]);

    // A new Project's lists are empty.
    let empty = vault.create_project("Empty".into()).unwrap();
    let page = vault.project_page(empty.id).unwrap();
    assert!(page.always.is_empty() && page.never.is_empty());
}

#[test]
fn remove_listed_takes_a_wording_off_one_list_only() {
    let scratch = Scratch::new();
    let vault = vault(&scratch);
    let id = listed(&vault);

    // Compared as the lists compare: case and spacing aside.
    vault
        .remove_listed(id.clone(), " maya   ELLISON ".into(), true)
        .unwrap();
    let page = vault.project_page(id.clone()).unwrap();
    assert_eq!(always(&page), [("Riverside Ltd", Category::Organization)]);
    assert_eq!(page.never, ["Plaintiff"], "Never is untouched");

    // A Never wording is not on Always, so removing it there does nothing.
    vault
        .remove_listed(id.clone(), "Plaintiff".into(), true)
        .unwrap();
    assert_eq!(vault.project_page(id.clone()).unwrap().never, ["Plaintiff"]);

    vault
        .remove_listed(id.clone(), "plaintiff".into(), false)
        .unwrap();
    let page = vault.project_page(id.clone()).unwrap();
    assert!(page.never.is_empty());
    assert_eq!(always(&page), [("Riverside Ltd", Category::Organization)]);

    // Kept for a fresh handle: the change is in the Vault.
    drop(vault);
    let reopened = self::vault(&scratch);
    let page = reopened.project_page(id).unwrap();
    assert_eq!(always(&page), [("Riverside Ltd", Category::Organization)]);
    assert!(page.never.is_empty());
}

#[test]
fn removing_a_wording_not_listed_writes_nothing() {
    let scratch = Scratch::new();
    let vault = vault(&scratch);
    let id = listed(&vault);
    let before = vault.store().project(&id).unwrap().revision;
    vault
        .remove_listed(id.clone(), "Nobody".into(), true)
        .unwrap();
    vault
        .remove_listed(id.clone(), "Nobody".into(), false)
        .unwrap();
    assert_eq!(vault.store().project(&id).unwrap().revision, before);

    assert!(
        vault
            .remove_listed("0".repeat(32), "Plaintiff".into(), false)
            .is_err(),
        "a Project that is not there is refused"
    );
}
