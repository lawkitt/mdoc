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
    let project = vault.create_project("Lease".into()).unwrap();
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
/// The mobile core imports converted text: its first save leaves nothing
/// for the desktop to convert again, even when that text is empty.
#[test]
fn a_saved_import_is_never_pending_conversion() {
    let scratch = Scratch::new();
    let vault = vault(&scratch);
    let docx = fixture("../../docs/product-media/Settlement agreement.docx");
    let id = imported(&vault, &docx, ImportOptions::default());
    let stored = vault.store().document(&id).unwrap();
    assert!(!stored.markdown.is_empty());
    assert!(!stored.unconverted && !stored.pending_conversion());

    let session = DocumentSession::new();
    let text = (session.import_file(docx.display().to_string(), ImportOptions::default(), None))
        .unwrap()
        .text;
    let all = crate::text::TextRange {
        start: 0,
        end: text.len() as u64,
    };
    session.type_text(all, String::new()).unwrap();
    let emptied = session.save_into_vault(vault.clone()).unwrap().id;
    let stored = vault.store().document(&emptied).unwrap();
    assert!(stored.markdown.is_empty() && stored.original.is_some());
    assert!(!stored.unconverted && !stored.pending_conversion());
}

/// Review of #1126: a record whose import stopped before its first save
/// opens on the phone as it is; a save from there leaves it waiting for
/// its conversion.
#[test]
fn an_unfinished_import_saved_from_the_phone_stays_pending() {
    let scratch = Scratch::new();
    let vault = vault(&scratch);
    let new = mdoc_vault::NewDocument {
        project: None,
        title: "Brief".into(),
        source_kind: mdoc_vault::SourceKind::Pdf,
        origin: mdoc_vault::DocumentOrigin::Ordinary,
        markdown: String::new(),
    };
    let info = mdoc_vault::OriginalInfo {
        file_name: "brief.pdf".into(),
        ..mdoc_vault::OriginalInfo::default()
    };
    let store = vault.store();
    let id = store
        .import_document(new, &b"%PDF-1.4"[..], info)
        .unwrap()
        .id;
    let session = DocumentSession::new();
    session.open_from_vault(vault.clone(), id.clone()).unwrap();
    session.save_into_vault(vault.clone()).unwrap();
    let stored = vault.store().document(&id).unwrap();
    assert!(stored.revision > 1, "the phone saved it");
    assert!(stored.unconverted && stored.pending_conversion());
}

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
/// A PDF's pages, counted by their page objects: enough for the image
/// builder's PDFs, which write each page once.
fn pdf_pages(pdf: &[u8]) -> usize {
    let needle = b"/Type /Page";
    (pdf.windows(needle.len() + 1))
        .filter(|w| &w[..needle.len()] == needle && w[needle.len()] != b's')
        .count()
}

/// Pages picked together keep the PDF the scan became, one page each,
/// named after the scan; `copy_original` and `original_bytes` return it.
/// The import itself needs OCR (see the end-to-end test below), so this
/// keeps the image builder's PDF of three pages as the import does.
#[test]
fn a_three_page_scan_keeps_its_pdf() {
    let scratch = Scratch::new();
    let vault = vault(&scratch);
    let photo = std::fs::read(fixture("images/gps-rotated.jpg")).unwrap();
    let built = mdoc_import::image_pdf::build(
        &[&photo, &photo, &photo],
        mdoc_import::image_pdf::DEFAULT_DPI,
    )
    .unwrap();
    assert_eq!(built.pages, 3);
    assert_eq!(pdf_pages(&built.pdf), 3);

    let session = DocumentSession::new();
    session
        .import_text_as(String::new(), "Lease".into(), DocumentOrigin::Ordinary)
        .unwrap();
    let shared = mdoc_import::import::Source::new("Lease.pdf".into(), None, built.pdf.clone())
        .shared_bytes();
    session.keep_as_original(crate::document_session::Source::scan(shared, "Lease"));
    let id = session.save_into_vault(vault.clone()).unwrap().id;
    let original = vault.original(id.clone()).unwrap().unwrap();
    assert_eq!(original.file_name, "Lease.pdf");
    assert_eq!(original.length, built.pdf.len() as u64);
    assert!(!original.password_protected);
    let bytes = vault.original_bytes(id.clone()).unwrap();
    assert_eq!(bytes, built.pdf);
    assert_eq!(pdf_pages(&bytes), 3);
    let view = scratch.path().join("view");
    let copy = vault.copy_original(id, view.display().to_string()).unwrap();
    assert_eq!(PathBuf::from(&copy), view.join("Lease.pdf"));
    assert_eq!(std::fs::read(copy).unwrap(), built.pdf);
}

/// A source gone by the end of its import (the app moved or deleted it
/// while the import ran) leaves the import working and the document
/// without an original.
#[test]
fn a_source_gone_at_the_end_of_its_import_keeps_no_original() {
    let scratch = Scratch::new();
    let vault = vault(&scratch);
    let path = scratch.path().join("Letter.md");
    std::fs::write(&path, "Ann Lee wrote.").unwrap();
    let gone = path.clone();
    crate::document_session::BEFORE_KEEPING.with(|hook| {
        *hook.borrow_mut() = Some(Box::new(move || std::fs::remove_file(gone).unwrap()));
    });
    let session = DocumentSession::new();
    let opened = session
        .import_file(path.display().to_string(), ImportOptions::default(), None)
        .unwrap();
    assert_eq!(opened.text, "Ann Lee wrote.");
    assert!(!path.exists());
    let id = session.save_into_vault(vault.clone()).unwrap().id;
    assert_eq!(vault.original(id.clone()).unwrap(), None);
    assert!(!vault.document(id).unwrap().has_original);
}

/// With the OCR runtime: three camera pages picked together import as one
/// document whose original is the three-page PDF they became.
#[test]
#[ignore = "downloads OCR runtimes/models; requires Apple Silicon macOS or Windows x64"]
#[cfg(any(
    all(target_os = "macos", target_arch = "aarch64"),
    all(target_os = "windows", target_arch = "x86_64")
))]
fn a_three_page_scan_import_keeps_its_pdf_end_to_end() {
    let scratch = Scratch::new();
    let vault = vault(&scratch);
    let kept = scratch.path().join("ocr").display().to_string();
    let models = scratch.path().join("models");
    let installed = mdoc_import::ocr::install_config_in(
        &models,
        &mdoc_store::settings::OcrConfig {
            model: mdoc_store::settings::OcrModel::Cyrillic,
            ..Default::default()
        },
        &mdoc_import::model_download::Progress::default(),
    )
    .unwrap();
    let options = ImportOptions {
        ocr: Some(crate::setup::OcrPaths {
            root: models.display().to_string(),
            model: crate::setup::ModelId::OcrEnglishRussian,
            pdfium: installed.pdfium.display().to_string(),
            onnx: installed.onnx.display().to_string(),
        }),
        ..ImportOptions::default()
    };
    let pages: Vec<String> = (1..=3)
        .map(|n| {
            let page = scratch.path().join(format!("page-{n}.png"));
            // A picture with words, which OCR reads: the photo fixture has none.
            std::fs::copy(
                fixture("../../apps/ios/mdocTests/Snapshots/documents-rows-light.png"),
                &page,
            )
            .unwrap();
            page.display().to_string()
        })
        .collect();
    let session = DocumentSession::new();
    session
        .import_images_resuming(kept, pages, "Lease".into(), options, None)
        .unwrap();
    let id = session.save_into_vault(vault.clone()).unwrap().id;
    let original = vault.original(id.clone()).unwrap().unwrap();
    assert_eq!(original.file_name, "Lease.pdf");
    let bytes = vault.original_bytes(id.clone()).unwrap();
    assert!(bytes.starts_with(b"%PDF"));
    assert_eq!(pdf_pages(&bytes), 3);
    let view = scratch.path().join("view");
    let copy = vault.copy_original(id, view.display().to_string()).unwrap();
    assert_eq!(std::fs::read(copy).unwrap(), bytes);
}
