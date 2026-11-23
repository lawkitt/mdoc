//! Export Project… through the crate's public interface (#1091 Decision
//! 22): an exported Project's files match its documents, on temp folders
//! with an injected key.
use mdoc_vault::{
    DocumentOrigin, NewDocument, NewProject, ORIGINALS, OriginalInfo, SourceKind, Vault, VaultError,
};
use std::{
    collections::BTreeMap,
    fs,
    path::{Path, PathBuf},
};

const KEY: [u8; 32] = [5; 32];

struct Folder {
    temp: tempfile::TempDir,
    vault: Vault,
}

impl Folder {
    fn new() -> Self {
        let temp = tempfile::tempdir().unwrap();
        let vault = Vault::open(temp.path().join("vault"), &KEY).unwrap();
        Self { temp, vault }
    }
    fn picked(&self) -> PathBuf {
        self.temp.path().join("Exports")
    }
    fn project(&self, name: &str) -> String {
        (self.vault)
            .create_project(NewProject {
                name: name.into(),
                ..NewProject::default()
            })
            .unwrap()
            .id
    }
    fn new_document(project: Option<&str>, title: &str, markdown: &str) -> NewDocument {
        NewDocument {
            project: project.map(str::to_owned),
            title: title.into(),
            source_kind: SourceKind::Text,
            origin: DocumentOrigin::Ordinary,
            markdown: markdown.into(),
        }
    }
    fn import(&self, project: Option<&str>, title: &str, markdown: &str, file: &str, bytes: &[u8]) {
        let info = OriginalInfo {
            file_name: file.into(),
            ..OriginalInfo::default()
        };
        (self.vault)
            .import_document(Self::new_document(project, title, markdown), bytes, info)
            .unwrap();
    }
    fn paste(&self, project: Option<&str>, title: &str, markdown: &str) {
        (self.vault)
            .create_document(Self::new_document(project, title, markdown))
            .unwrap();
    }
}

/// Every file under `dir`, by its path relative to it, with `/`.
fn files(dir: &Path) -> BTreeMap<String, Vec<u8>> {
    let mut found = BTreeMap::new();
    let mut stack = vec![dir.to_path_buf()];
    while let Some(at) = stack.pop() {
        for entry in fs::read_dir(&at).unwrap() {
            let path = entry.unwrap().path();
            if path.is_dir() {
                stack.push(path);
            } else {
                let name = path.strip_prefix(dir).unwrap().to_string_lossy();
                found.insert(name.replace('\\', "/"), fs::read(&path).unwrap());
            }
        }
    }
    found
}

#[test]
fn an_exported_projects_files_match_its_documents() {
    let f = Folder::new();
    let project = f.project("Smith v. Jones");
    let other = f.project("Other matter");
    let pdf: Vec<u8> = (0..3 * 1024 * 1024 + 7).map(|i| (i % 251) as u8).collect();
    f.import(
        Some(&project),
        "Contract",
        "# Contract\n\nJohn Smith",
        "contract.pdf",
        &pdf,
    );
    // A `.md` import keeps the same text as its original.
    f.import(Some(&project), "Notes", "# Notes", "Notes.md", b"# Notes");
    f.paste(Some(&project), "Pasted email", "Dear Jane");
    // Two documents with one title, and a title Windows refuses.
    f.paste(Some(&project), "Brief", "first");
    f.paste(Some(&project), "brief", "second");
    f.paste(Some(&project), "Q3: costs/fees?", "costs");
    // A conversion that never finished: the original alone.
    f.import(Some(&project), "Scan", "", "scan.png", b"\x89PNG");
    // Not in the Project: never exported.
    f.import(Some(&other), "Elsewhere", "other", "elsewhere.pdf", b"%PDF");
    f.paste(None, "Unfiled", "unfiled");

    let exported = f.vault.export_project(&project, &f.picked()).unwrap();
    assert_eq!(exported.folder, f.picked().join("Smith v. Jones"));
    assert_eq!(exported.documents, 7);
    assert!(exported.failed.is_empty());
    let written = files(&exported.folder);
    let expected: BTreeMap<String, Vec<u8>> = [
        ("Contract.md", b"# Contract\n\nJohn Smith".to_vec()),
        ("Notes.md", b"# Notes".to_vec()),
        ("Pasted email.md", b"Dear Jane".to_vec()),
        ("Brief.md", b"first".to_vec()),
        ("brief 2.md", b"second".to_vec()),
        ("Q3- costs-fees-.md", b"costs".to_vec()),
        (&format!("{ORIGINALS}/contract.pdf"), pdf.clone()),
        (&format!("{ORIGINALS}/Notes.md"), b"# Notes".to_vec()),
        (&format!("{ORIGINALS}/scan.png"), b"\x89PNG".to_vec()),
    ]
    .into_iter()
    .map(|(name, bytes)| (name.to_string(), bytes))
    .collect();
    assert_eq!(
        written.keys().collect::<Vec<_>>(),
        expected.keys().collect::<Vec<_>>()
    );
    assert_eq!(written, expected);
    // Each document's Markdown, read back through the Vault, is its file.
    for summary in f.vault.documents_in(Some(&project)) {
        let document = f.vault.document(&summary.id).unwrap();
        if !document.markdown.is_empty() {
            assert!(
                written
                    .values()
                    .any(|bytes| bytes == document.markdown.as_bytes()),
                "{}",
                document.title
            );
        }
    }
}

#[test]
fn a_second_export_never_overwrites_the_first() {
    let f = Folder::new();
    let project = f.project("Matter");
    f.paste(Some(&project), "Letter", "one");
    let first = f.vault.export_project(&project, &f.picked()).unwrap();
    fs::write(first.folder.join("Letter.md"), "edited outside mdoc").unwrap();
    let second = f.vault.export_project(&project, &f.picked()).unwrap();
    assert_eq!(second.folder, f.picked().join("Matter 2"));
    assert_eq!(
        fs::read_to_string(first.folder.join("Letter.md")).unwrap(),
        "edited outside mdoc"
    );
    assert_eq!(
        fs::read_to_string(second.folder.join("Letter.md")).unwrap(),
        "one"
    );
}

#[test]
fn a_project_named_unsafely_exports_into_a_safe_folder() {
    let f = Folder::new();
    let project = f.project("../CON");
    f.paste(Some(&project), "Letter", "one");
    let exported = f.vault.export_project(&project, &f.picked()).unwrap();
    assert_eq!(exported.folder.parent(), Some(f.picked().as_path()));
    assert_eq!(exported.folder.file_name().unwrap(), "-CON");
    let empty = f.project("   ");
    let exported = f.vault.export_project(&empty, &f.picked()).unwrap();
    assert_eq!(exported.folder.file_name().unwrap(), "Untitled");
    assert_eq!((exported.documents, files(&exported.folder).len()), (0, 0));
}

#[test]
fn nothing_is_exported_into_the_vault() {
    let f = Folder::new();
    let project = f.project("Matter");
    f.paste(Some(&project), "Letter", "one");
    let root = f.vault.root().to_path_buf();
    for inside in [root.clone(), root.join("docs"), root.join("new folder")] {
        assert!(matches!(
            f.vault.export_project(&project, &inside),
            Err(VaultError::ExportInsideVault)
        ));
    }
    assert!(!root.join("Matter").exists() && !root.join("docs").join("Matter").exists());
    assert!(matches!(
        f.vault
            .export_project("0123456789abcdef0123456789abcdef", &f.picked()),
        Err(VaultError::NotFound { .. })
    ));
}
