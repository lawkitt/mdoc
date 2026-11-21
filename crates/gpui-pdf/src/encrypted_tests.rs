//! Encrypted PDFs opened with their password: the fixtures
//! `tests/fixtures/pdf-source/comments-aes128.pdf` and `comments-aes256.pdf`
//! (user password [`PASSWORD`]) against their open twin `comments.pdf`, and a
//! form built in code, encrypted here.
//!
//! The fixtures are written by [`write_encrypted_fixtures`]:
//! `cargo test -p gpui-pdf -- --ignored write_encrypted_fixtures`.

use std::collections::BTreeMap;
use std::rc::Rc;
use std::sync::Arc;

use gpui::hsla;
use lopdf::encryption::crypt_filters::{Aes128CryptFilter, Aes256CryptFilter, CryptFilter};
use lopdf::{Document as LoDocument, EncryptionState, EncryptionVersion, Object, Permissions};

use crate::{
    FormField, LoadError, PageTone, PdfSource, PdfStyle, PdfView, ViewOptions, open_document,
    render_page,
};

/// The published test password of every encrypted fixture. It protects
/// nothing real (the same value as the locked Office fixtures).
const PASSWORD: &str = "Petrov-2026";
const OWNER_PASSWORD: &str = "Petrov-2026-owner";

// Embedded, not read at run time: cached test binaries can outlive their
// checkout.
const OPEN: &[u8] = include_bytes!("../../../tests/fixtures/pdf-source/comments.pdf");
const AES128: &[u8] = include_bytes!("../../../tests/fixtures/pdf-source/comments-aes128.pdf");
const AES256: &[u8] = include_bytes!("../../../tests/fixtures/pdf-source/comments-aes256.pdf");

#[derive(Clone, Copy, Debug)]
enum Cipher {
    Aes128,
    Aes256,
}

/// `bytes` encrypted with `cipher` under [`PASSWORD`] (user) and
/// [`OWNER_PASSWORD`], every permission granted.
fn encrypt(bytes: &[u8], cipher: Cipher) -> Vec<u8> {
    let mut doc = LoDocument::load_mem(bytes).unwrap();
    // AES-128 keys from the first /ID; a generated file has none.
    if doc.trailer.get(b"ID").is_err() {
        let id = Object::string_literal(b"mdoc-fixture-id1".to_vec());
        doc.trailer.set("ID", vec![id.clone(), id]);
    }
    let state = match cipher {
        Cipher::Aes128 => {
            let filter: Arc<dyn CryptFilter> = Arc::new(Aes128CryptFilter);
            EncryptionState::try_from(EncryptionVersion::V4 {
                document: &doc,
                encrypt_metadata: true,
                crypt_filters: BTreeMap::from([(b"StdCF".to_vec(), filter)]),
                stream_filter: b"StdCF".to_vec(),
                string_filter: b"StdCF".to_vec(),
                owner_password: OWNER_PASSWORD,
                user_password: PASSWORD,
                permissions: Permissions::all(),
            })
        }
        Cipher::Aes256 => {
            let filter: Arc<dyn CryptFilter> = Arc::new(Aes256CryptFilter);
            // A fixed key: the fixture protects nothing.
            let key: Vec<u8> = (0u8..32).map(|i| i.wrapping_mul(37) ^ 0x5a).collect();
            EncryptionState::try_from(EncryptionVersion::V5 {
                encrypt_metadata: true,
                crypt_filters: BTreeMap::from([(b"StdCF".to_vec(), filter)]),
                file_encryption_key: &key,
                stream_filter: b"StdCF".to_vec(),
                string_filter: b"StdCF".to_vec(),
                owner_password: OWNER_PASSWORD,
                user_password: PASSWORD,
                permissions: Permissions::all(),
            })
        }
    }
    .unwrap();
    doc.encrypt(&state).unwrap();
    let mut out = Vec::new();
    doc.save_to(&mut out).unwrap();
    out
}

/// Regenerate the encrypted fixtures from `comments.pdf`. Ignored: run it
/// on purpose, then commit the two files (random salts make every run's
/// bytes differ).
#[test]
#[ignore = "writes the fixtures; run on purpose"]
fn write_encrypted_fixtures() {
    let dir =
        std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../tests/fixtures/pdf-source");
    let open = std::fs::read(dir.join("comments.pdf")).unwrap();
    for (cipher, name) in [
        (Cipher::Aes128, "comments-aes128.pdf"),
        (Cipher::Aes256, "comments-aes256.pdf"),
    ] {
        std::fs::write(dir.join(name), encrypt(&open, cipher)).unwrap();
    }
}

/// Each page of `bytes`, opened with `password` under `options`, rendered
/// in `tone`, as raw pixels.
fn pages(
    bytes: &[u8],
    password: Option<&str>,
    options: ViewOptions,
    tone: PageTone,
) -> Vec<Vec<u8>> {
    let (doc, _) = open_document(Arc::new(bytes.to_vec()), password, options).unwrap();
    (0..doc.pages().len())
        .map(|idx| {
            let image = render_page(&doc, idx, 1.0, tone).unwrap();
            image.as_bytes(0).unwrap().to_vec()
        })
        .collect()
}

/// The light page and the dark theme's toned page.
fn tones() -> [PageTone; 2] {
    [
        PageTone::Original,
        PageTone::themed(hsla(0., 0., 0.12, 1.), hsla(0., 0., 0.9, 1.)),
    ]
}

fn options(hide: bool) -> ViewOptions {
    ViewOptions {
        hide_comment_annotations: hide,
    }
}

#[test]
fn the_fixtures_are_encrypted_with_their_ciphers() {
    for (bytes, v, r) in [(AES128, 4, 4), (AES256, 5, 6)] {
        let doc = LoDocument::load_mem(bytes).unwrap();
        let dict = doc.get_encrypted().expect("encrypted");
        assert_eq!(dict.get(b"V").unwrap().as_i64().unwrap(), v);
        assert_eq!(dict.get(b"R").unwrap().as_i64().unwrap(), r);
        // A user password, not an owner-only lock: no password reads nothing.
        assert!(LoDocument::load_mem(bytes).unwrap().is_encrypted());
    }
}

#[test]
fn an_encrypted_fixture_renders_like_its_open_twin() {
    for (name, bytes) in [("aes128", AES128), ("aes256", AES256)] {
        for hide in [false, true] {
            for tone in tones() {
                let twin = pages(OPEN, None, options(hide), tone);
                let locked = pages(bytes, Some(PASSWORD), options(hide), tone);
                assert_eq!(twin.len(), 2, "{name}");
                assert!(
                    twin == locked,
                    "{name}, comments hidden: {hide}, {tone:?}: pages differ"
                );
            }
        }
    }
}

#[test]
fn comment_annotations_stay_hidden_on_an_encrypted_file() {
    for bytes in [AES128, AES256] {
        let shown = pages(bytes, Some(PASSWORD), options(false), PageTone::Original);
        let hidden = pages(bytes, Some(PASSWORD), options(true), PageTone::Original);
        // Page 1 holds the sticky note, the highlight's comment and the
        // FreeText box: hiding them changes the page.
        assert!(shown[0] != hidden[0]);
    }
}

#[test]
fn a_missing_or_wrong_password_leaves_it_locked() {
    for bytes in [AES128, AES256] {
        for password in [
            None,
            Some(""),
            Some("petrov-2026"),
            Some(OWNER_PASSWORD.trim_end_matches("-owner")),
        ] {
            let result = open_document(Arc::new(bytes.to_vec()), password, ViewOptions::default());
            match (password, result) {
                (Some(PASSWORD), Ok(_)) => {}
                (_, Err(LoadError::Locked)) => {}
                (password, other) => panic!(
                    "{:?}: {:?}",
                    password.map(|_| "***"),
                    other.map(|_| ()).map_err(|e| format!("{e:?}"))
                ),
            }
        }
        // The owner password opens it too, as in other readers (hayro 0.9
        // alone refuses it for AES-128; the fix-up's decrypted bytes cover it).
        assert!(
            open_document(
                Arc::new(bytes.to_vec()),
                Some(OWNER_PASSWORD),
                ViewOptions::default()
            )
            .is_ok()
        );
    }
}

#[test]
fn an_unknown_security_handler_is_unsupported() {
    // `/Standard` swapped for a handler no reader knows, same length so every
    // offset still holds.
    let mut bytes = AES256.to_vec();
    let at = bytes
        .windows(9)
        .position(|w| w == b"/Standard")
        .expect("a standard security handler");
    bytes[at..at + 9].copy_from_slice(b"/Nonstand");
    let result = open_document(Arc::new(bytes), Some(PASSWORD), ViewOptions::default());
    assert!(
        matches!(result, Err(LoadError::Unsupported)),
        "{:?}",
        result.err()
    );
}

/// The form built in `forms`' tests, its fields and its fix-up, encrypted.
#[test]
fn form_fields_and_the_fix_up_read_an_encrypted_form() {
    let open = crate::forms::tests::build_test_pdf();
    let describe = |fields: Vec<FormField>| {
        fields
            .into_iter()
            .map(|f| (f.name, f.value, f.page, f.rect))
            .collect::<Vec<_>>()
    };
    let (_, open_fields) =
        open_document(Arc::new(open.clone()), None, ViewOptions::default()).unwrap();
    assert_eq!(open_fields.len(), 3);
    for cipher in [Cipher::Aes128, Cipher::Aes256] {
        let locked = encrypt(&open, cipher);
        let (_, fields) = open_document(
            Arc::new(locked.clone()),
            Some(PASSWORD),
            ViewOptions::default(),
        )
        .unwrap();
        assert_eq!(
            describe(fields),
            describe(open_fields.clone()),
            "{cipher:?}"
        );
        // The text fields' values are drawn only by the fix-up's appearance
        // streams, so equal pages mean it ran on the encrypted file.
        for tone in tones() {
            assert!(
                pages(&locked, Some(PASSWORD), ViewOptions::default(), tone)
                    == pages(&open, None, ViewOptions::default(), tone),
                "{cipher:?}, {tone:?}"
            );
        }
    }
}

#[test]
fn a_source_never_prints_its_bytes_or_password() {
    let source = PdfSource::Bytes {
        bytes: Arc::new(b"%PDF-1.7 secret body".to_vec()),
        password: Some(PASSWORD.into()),
    };
    let debug = format!("{source:?} {source:#?}");
    assert!(!debug.contains(PASSWORD), "{debug}");
    assert!(!debug.contains("secret"), "{debug}");
    assert!(debug.contains("len"), "{debug}");
}

async fn settled(view: &gpui::Entity<PdfView>, cx: &mut gpui::VisualTestContext) {
    cx.condition(view, |view, _| {
        view.is_loaded() || view.is_locked() || view.load_error().is_some()
    })
    .await;
}

fn bytes_view<'a>(
    bytes: &[u8],
    password: Option<&str>,
    cx: &'a mut gpui::TestAppContext,
) -> (gpui::Entity<PdfView>, &'a mut gpui::VisualTestContext) {
    let source = PdfSource::Bytes {
        bytes: Arc::new(bytes.to_vec()),
        password: password.map(Into::into),
    };
    cx.add_window_view(|_, cx| {
        PdfView::with_options(
            source,
            Rc::new(PdfStyle::default),
            Rc::new(|| 1.),
            options(true),
            cx,
        )
    })
}

#[gpui::test]
async fn a_view_opens_encrypted_bytes_with_their_password(cx: &mut gpui::TestAppContext) {
    let (view, cx) = bytes_view(AES256, Some(PASSWORD), cx);
    settled(&view, cx).await;
    view.read_with(cx, |view, _| {
        assert!(view.is_loaded(), "{:?}", view.load_error());
        assert!(!view.is_locked());
        assert_eq!(view.dims.len(), 2);
        // No path, so no name until the host gives one.
        assert!(view.path.is_none());
    });
}

#[gpui::test]
async fn a_view_without_the_password_is_locked(cx: &mut gpui::TestAppContext) {
    for password in [None, Some("wrong")] {
        let (view, vcx) = bytes_view(AES128, password, cx);
        settled(&view, vcx).await;
        view.read_with(vcx, |view, _| {
            assert!(view.is_locked());
            assert!(!view.is_loaded());
            assert!(view.load_error().is_none());
        });
    }
}

#[gpui::test]
async fn a_view_says_when_no_password_can_open_it(cx: &mut gpui::TestAppContext) {
    let mut bytes = AES128.to_vec();
    let at = bytes.windows(9).position(|w| w == b"/Standard").unwrap();
    bytes[at..at + 9].copy_from_slice(b"/Nonstand");
    let (view, cx) = bytes_view(&bytes, Some(PASSWORD), cx);
    settled(&view, cx).await;
    view.read_with(cx, |view, _| {
        assert!(view.is_unsupported_encryption());
        assert!(!view.is_locked());
        assert!(view.load_error().is_some());
    });
}

#[gpui::test]
async fn a_view_opens_plain_bytes(cx: &mut gpui::TestAppContext) {
    let (view, cx) = bytes_view(OPEN, None, cx);
    settled(&view, cx).await;
    view.read_with(cx, |view, _| assert!(view.is_loaded()));
}
