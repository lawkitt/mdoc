//! Source Originals through the exported objects (#1091, item 3's
//! behaviour reached from the mobile core), on a temp folder with an
//! injected key.
use crate::{
    DocumentSession, Vault,
    document_session::{DocumentOrigin, ImportOptions},
    test_support::Scratch,
};
use std::{path::PathBuf, sync::Arc};

fn fixture(name: &str) -> PathBuf {
    std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../tests/fixtures")
        .join(name)
}

fn vault(scratch: &Scratch) -> Arc<Vault> {
    Vault::open(
        scratch.path().join("vault").display().to_string(),
        vec![9; 32],
    )
    .unwrap()
}

/// Import `path` and save it into the Vault, Unfiled; its id.
fn imported(vault: &Arc<Vault>, path: &std::path::Path, options: ImportOptions) -> String {
    let session = DocumentSession::new();
    session
        .import_file(path.display().to_string(), options, None)
        .unwrap();
    session.save_into_vault(vault.clone()).unwrap().id
}

/// An import keeps its file as the original: listed, described, copied for
/// viewing and read back byte for byte; a repeated import of the same bytes
/// finds it under any file name, and a pasted document keeps none.
#[test]
fn an_import_keeps_its_file_as_the_original() {
    let scratch = Scratch::new();
    let vault = vault(&scratch);
    let docx = fixture("../../docs/product-media/Settlement agreement.docx");
    let bytes = std::fs::read(&docx).unwrap();
    let id = imported(&vault, &docx, ImportOptions::default());

    let original = vault.original(id.clone()).unwrap().unwrap();
    assert_eq!(original.file_name, "Settlement agreement.docx");
    assert_eq!(original.length, bytes.len() as u64);
    assert!(!original.password_protected);
    assert!(vault.document(id.clone()).unwrap().has_original);
    assert_eq!(vault.original_bytes(id.clone()).unwrap(), bytes);

    let view = scratch.path().join("view");
    let copy = vault
        .copy_original(id.clone(), view.display().to_string())
        .unwrap();
    assert_eq!(PathBuf::from(&copy), view.join("Settlement agreement.docx"));
    assert_eq!(std::fs::read(&copy).unwrap(), bytes);
    // Never into the Vault itself.
    let inside = scratch.path().join("vault").join("docs");
    assert!(
        vault
            .copy_original(id.clone(), inside.display().to_string())
            .is_err()
    );

    let renamed = scratch.path().join("Copy of the agreement.docx");
    std::fs::copy(&docx, &renamed).unwrap();
    let found: Vec<_> = (vault.documents_from_source(renamed.display().to_string()))
        .unwrap()
        .into_iter()
        .map(|d| d.id)
        .collect();
    assert_eq!(found, std::slice::from_ref(&id));
    let other = fixture("locked-office/word-open.docx");
    assert!(
        (vault.documents_from_source(other.display().to_string()))
            .unwrap()
            .is_empty()
    );

    let pasted = DocumentSession::new();
    pasted
        .import_text_as("Notes.".into(), "Notes".into(), DocumentOrigin::Ordinary)
        .unwrap();
    let pasted = pasted.save_into_vault(vault.clone()).unwrap().id;
    assert_eq!(vault.original(pasted.clone()).unwrap(), None);
    assert!(!vault.document(pasted.clone()).unwrap().has_original);
    assert!(vault.original_bytes(pasted.clone()).is_err());
    assert!(
        vault
            .copy_original(pasted, view.display().to_string())
            .is_err()
    );

    // A move keeps it; a delete takes it, and the repeat is no longer found.
    let project = vault.create_project("Case".into()).unwrap();
    vault.move_document(id.clone(), Some(project.id)).unwrap();
    assert_eq!(vault.original_bytes(id.clone()).unwrap(), bytes);
    vault.delete_document(id).unwrap();
    assert!(
        (vault.documents_from_source(docx.display().to_string()))
            .unwrap()
            .is_empty()
    );
}

/// A locked DOCX keeps its original locked, byte for byte, marked so.
#[test]
fn a_locked_source_stays_locked() {
    let scratch = Scratch::new();
    let vault = vault(&scratch);
    let locked = fixture("locked-office/word-locked.docx");
    let id = imported(
        &vault,
        &locked,
        ImportOptions {
            password: Some("Petrov-2026".into()),
            ..ImportOptions::default()
        },
    );
    let original = vault.original(id.clone()).unwrap().unwrap();
    assert!(original.password_protected);
    assert_eq!(
        vault.original_bytes(id).unwrap(),
        std::fs::read(&locked).unwrap()
    );
}

/// A photo's original is the photo without its metadata, as the phones
/// keep it (its import needs OCR, which tests do not install, so this
/// reads what an import would keep).
#[test]
fn a_photo_keeps_no_metadata_in_its_original() {
    let scratch = Scratch::new();
    let vault = vault(&scratch);
    let photo = scratch.path().join("Photo.jpg");
    std::fs::copy(fixture("images/gps-rotated.jpg"), &photo).unwrap();
    let raw = std::fs::read(&photo).unwrap();
    let stripped = mdoc_import::image_pdf::without_metadata(&raw).unwrap();
    assert_ne!(stripped, raw);
    let source = (crate::document_session::Source::of(&photo, false))
        .unwrap()
        .unwrap();
    assert_eq!(source.file_name, "Photo.jpg");
    let mut kept = Vec::new();
    std::io::Read::read_to_end(&mut source.reader().unwrap(), &mut kept).unwrap();
    assert_eq!(kept, stripped);
    assert!(
        (vault.documents_from_source(photo.display().to_string()))
            .unwrap()
            .is_empty()
    );
}

/// A locked PDF keeps its original locked too; an open file given a
/// password anyway is not marked locked (review 4).
#[test]
fn a_locked_pdf_stays_locked_and_an_open_file_is_not_marked() {
    let scratch = Scratch::new();
    let vault = vault(&scratch);
    let password = || ImportOptions {
        password: Some("Petrov-2026".into()),
        ..ImportOptions::default()
    };
    let pdf = fixture("pdf-source/comments-aes256.pdf");
    let id = imported(&vault, &pdf, password());
    assert!(
        vault
            .original(id.clone())
            .unwrap()
            .unwrap()
            .password_protected
    );
    assert_eq!(
        vault.original_bytes(id).unwrap(),
        std::fs::read(&pdf).unwrap()
    );
    let open = imported(&vault, &fixture("locked-office/word-open.docx"), password());
    assert!(!vault.original(open).unwrap().unwrap().password_protected);
}

/// The original keeps the import's notice and left-out line apart: a PDF
/// with comments has a left-out line and no notice (review 3).
#[test]
fn the_original_keeps_the_notice_and_the_left_out_line_apart() {
    let scratch = Scratch::new();
    let vault = vault(&scratch);
    let id = imported(
        &vault,
        &fixture("pdf-source/comments.pdf"),
        ImportOptions::default(),
    );
    let original = vault.original(id).unwrap().unwrap();
    assert_eq!(original.import_warning, None);
    assert!(
        original
            .left_out
            .is_some_and(|line| line.contains("comment"))
    );
}

/// A name a phone allows but a copy refuses becomes one it takes; an
/// image named as something else is still told by its bytes and kept
/// without its metadata (reviews 1 and 2).
#[test]
fn names_never_decide_what_is_kept_or_block_a_copy() {
    let scratch = Scratch::new();
    let vault = vault(&scratch);
    let notes = scratch.path().join("Notes 11:23.md");
    std::fs::write(&notes, "Ann Lee wrote.").unwrap();
    let id = imported(&vault, &notes, ImportOptions::default());
    assert_eq!(
        vault.original(id.clone()).unwrap().unwrap().file_name,
        "Notes 11:23.md"
    );
    let view = scratch.path().join("view");
    let copy = vault.copy_original(id, view.display().to_string()).unwrap();
    assert_eq!(PathBuf::from(copy), view.join("Notes 11-23.md"));

    let disguised = scratch.path().join("scan.bin");
    std::fs::copy(fixture("images/gps-rotated.jpg"), &disguised).unwrap();
    let raw = std::fs::read(&disguised).unwrap();
    let source = (crate::document_session::Source::of(&disguised, false))
        .unwrap()
        .unwrap();
    let mut kept = Vec::new();
    std::io::Read::read_to_end(&mut source.reader().unwrap(), &mut kept).unwrap();
    assert_eq!(
        kept,
        mdoc_import::image_pdf::without_metadata(&raw).unwrap()
    );
    let missing = scratch.path().join("gone.jpg");
    assert!(
        vault
            .documents_from_source(missing.display().to_string())
            .is_err()
    );
}

/// Over 16 MiB an original is refused in memory and still copies; a
/// damaged one fails both ways, as damaged, and leaves no copy (review 5).
#[test]
fn a_large_or_damaged_original() {
    let scratch = Scratch::new();
    let vault = vault(&scratch);
    let root = scratch.path().join("vault");
    let large = {
        let store = mdoc_vault::Vault::open(&root, &[9; 32]).unwrap();
        let bytes = vec![b'x'; 16 * 1024 * 1024 + 1];
        store
            .import_document(
                mdoc_vault::NewDocument {
                    project: None,
                    title: "Large".into(),
                    source_kind: mdoc_vault::SourceKind::Text,
                    origin: mdoc_vault::DocumentOrigin::Ordinary,
                    markdown: String::new(),
                },
                bytes.as_slice(),
                mdoc_vault::OriginalInfo {
                    file_name: "large.txt".into(),
                    ..Default::default()
                },
            )
            .unwrap()
            .id
    };
    vault.refresh().unwrap();
    assert!(matches!(
        vault.original_bytes(large.clone()),
        Err(crate::CoreError::Invalid { .. })
    ));
    let view = scratch.path().join("view");
    let copy = vault
        .copy_original(large, view.display().to_string())
        .unwrap();
    assert_eq!(std::fs::metadata(copy).unwrap().len(), 16 * 1024 * 1024 + 1);

    let docx = fixture("../../docs/product-media/Settlement agreement.docx");
    let id = imported(&vault, &docx, ImportOptions::default());
    let sealed = root.join("docs").join(&id).join("original.enc");
    let mut bytes = std::fs::read(&sealed).unwrap();
    *bytes.last_mut().unwrap() ^= 1;
    std::fs::write(&sealed, bytes).unwrap();
    assert!(matches!(
        vault.original_bytes(id.clone()),
        Err(crate::CoreError::Case { .. })
    ));
    let damaged_view = scratch.path().join("damaged");
    assert!(
        vault
            .copy_original(id, damaged_view.display().to_string())
            .is_err()
    );
    let left: Vec<_> = (std::fs::read_dir(&damaged_view).into_iter().flatten())
        .flatten()
        .collect();
    assert!(left.is_empty(), "{left:?}");
}

/// The original is read at the first save: a file gone by then fails the
/// save and leaves nothing in the Vault, rather than a document without
/// the original it was imported from.
#[test]
fn a_source_gone_before_the_first_save_saves_nothing() {
    let scratch = Scratch::new();
    let vault = vault(&scratch);
    let path = scratch.path().join("Letter.md");
    std::fs::write(&path, "Ann Lee wrote.").unwrap();
    let session = DocumentSession::new();
    session
        .import_file(path.display().to_string(), ImportOptions::default(), None)
        .unwrap();
    std::fs::remove_file(&path).unwrap();
    assert!(session.save_into_vault(vault.clone()).is_err());
    assert!(vault.documents().is_empty());
}
