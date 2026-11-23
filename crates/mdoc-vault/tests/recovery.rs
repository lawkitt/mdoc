//! The recovery key through the crate's public interface (#1091 Decision
//! 22; Testing Decisions, seam 1: "opening a copied folder with a
//! recovery key"), on temp folders with injected keys.
use mdoc_vault::{
    DocumentOrigin, NewDocument, NewProject, OriginalInfo, SourceKind, Vault, VaultError,
    recovery::{self, RecoveryKeyError},
};
use std::{fs, path::Path};

/// A key with every byte different, so a shifted bit shows.
fn key(seed: u8) -> [u8; 32] {
    std::array::from_fn(|i| {
        seed.wrapping_mul(31)
            .wrapping_add((i as u8).wrapping_mul(37))
    })
}

fn copy_dir(from: &Path, to: &Path) {
    fs::create_dir_all(to).unwrap();
    for entry in fs::read_dir(from).unwrap() {
        let entry = entry.unwrap();
        let target = to.join(entry.file_name());
        if entry.file_type().unwrap().is_dir() {
            copy_dir(&entry.path(), &target);
        } else {
            fs::copy(entry.path(), target).unwrap();
        }
    }
}

#[test]
fn a_recovery_key_is_52_characters_in_13_groups_and_reads_back() {
    for seed in 0..=255u8 {
        let key = key(seed);
        let text = recovery::format(&key);
        let groups: Vec<&str> = text.split(' ').collect();
        assert_eq!(groups.len(), 13, "{text}");
        assert!(groups.iter().all(|g| g.len() == recovery::GROUP));
        assert!(
            text.chars()
                .all(|c| c == ' ' || c.is_ascii_uppercase() || ('2'..='7').contains(&c))
        );
        assert_eq!(text.replace(' ', "").len(), recovery::LEN);
        assert_eq!(recovery::parse(&text), Ok(key));
    }
}

#[test]
fn reading_forgives_the_form_a_person_writes_it_in() {
    let key = key(9);
    let text = recovery::format(&key);
    let plain = text.replace(' ', "");
    for written in [
        plain.clone(),
        plain.to_lowercase(),
        text.replace(' ', "-"),
        format!("  {}\n", text.replace(' ', "\n")),
        text.replace(' ', " – "),
        // O, I and B written as the digits people mistake them for.
        plain.replace('O', "0").replace('I', "1").replace('B', "8"),
    ] {
        assert_eq!(recovery::parse(&written), Ok(key), "{written:?}");
    }
}

#[test]
fn what_is_not_a_recovery_key_is_refused_and_says_why() {
    let plain = recovery::format(&key(3)).replace(' ', "");
    assert_eq!(
        recovery::parse(&plain[..51]),
        Err(RecoveryKeyError::Length(51))
    );
    assert_eq!(
        recovery::parse(&format!("{plain}A")),
        Err(RecoveryKeyError::Length(53))
    );
    assert_eq!(recovery::parse(""), Err(RecoveryKeyError::Length(0)));
    let mut wrong = plain.clone();
    wrong.replace_range(10..11, "9");
    assert_eq!(
        recovery::parse(&wrong),
        Err(RecoveryKeyError::Character('9'))
    );
    wrong.replace_range(10..11, "+");
    assert_eq!(
        recovery::parse(&wrong),
        Err(RecoveryKeyError::Character('+'))
    );
    // The last character carries one bit and four that must be zero.
    let mut last = plain.clone();
    last.replace_range(51..52, "B");
    assert_eq!(recovery::parse(&last), Err(RecoveryKeyError::Trailing));
    assert!(
        RecoveryKeyError::Length(51)
            .to_string()
            .contains("52 characters")
    );
}

#[test]
fn a_vault_copied_to_a_fresh_profile_opens_with_its_recovery_key() {
    let old = tempfile::tempdir().unwrap();
    let fresh = tempfile::tempdir().unwrap();
    let old_root = old.path().join("vault");
    let sealed_with = key(1);
    let (project, document) = {
        let vault = Vault::open(&old_root, &sealed_with).unwrap();
        let project = vault
            .create_project(NewProject {
                name: "Smith v. Jones".into(),
                ..NewProject::default()
            })
            .unwrap();
        let document = vault
            .import_document(
                NewDocument {
                    project: Some(project.id.clone()),
                    title: "Engagement letter".into(),
                    source_kind: SourceKind::Markdown,
                    origin: DocumentOrigin::Ordinary,
                    markdown: "# Engagement letter\n\nJohn Smith".into(),
                },
                &b"# Engagement letter\n\nJohn Smith"[..],
                OriginalInfo {
                    file_name: "Engagement letter.md".into(),
                    ..OriginalInfo::default()
                },
            )
            .unwrap();
        vault.flush().unwrap();
        (project, document)
    };
    let written = recovery::format(&sealed_with);
    // The fresh profile's own key is another; the copy doesn't open with it.
    let root = fresh.path().join("vault");
    copy_dir(&old_root, &root);
    assert!(matches!(
        Vault::open(&root, &key(2)),
        Err(VaultError::WrongKey)
    ));
    // A key mistyped by one character is a different key.
    let mut typo = written.replace(' ', "");
    let swapped = if typo.starts_with('A') { "C" } else { "A" };
    typo.replace_range(0..1, swapped);
    let typo = recovery::parse(&typo).unwrap();
    assert!(matches!(
        Vault::open(&root, &typo),
        Err(VaultError::WrongKey)
    ));
    // Typed back in lower case, it opens everything as it was.
    let typed = recovery::parse(&written.to_lowercase()).unwrap();
    let vault = Vault::open(&root, &typed).unwrap();
    assert_eq!(vault.projects().len(), 1);
    assert_eq!(vault.project(&project.id).unwrap().name, "Smith v. Jones");
    let back = vault.document(&document.id).unwrap();
    assert_eq!(back.markdown, "# Engagement letter\n\nJohn Smith");
    let mut original = Vec::new();
    std::io::Read::read_to_end(
        &mut vault.open_original(&document.id).unwrap(),
        &mut original,
    )
    .unwrap();
    assert_eq!(original, b"# Engagement letter\n\nJohn Smith");
}

#[test]
fn a_vault_knows_its_own_key_and_keeps_no_other() {
    let dir = tempfile::tempdir().unwrap();
    let vault = Vault::open(dir.path().join("vault"), &key(5)).unwrap();
    assert!(vault.opens_with(&key(5)));
    assert!(!vault.opens_with(&key(6)));
    let parsed = recovery::parse(&recovery::format(&key(5))).unwrap();
    assert!(vault.opens_with(&parsed));
}
