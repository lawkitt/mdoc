//! Source Originals through the Vault's public interface (#1091 Decisions
//! 7, 14 and 23; Testing Decisions, seam 1), on a temp folder with an
//! injected key. Only the documented format of `original.enc` (ADR 0125,
//! the crate's `original` module) is touched directly, to damage, cut,
//! reorder or move a file.
use mdoc_vault::{
    Document, DocumentOrigin, Fault, FaultHook, NewDocument, NewProject, ObjectKind, OriginalInfo,
    SourceKind, Stage, Vault, VaultError, WriteStep, source_sha256,
};
use std::{
    fs,
    io::{self, Read},
    path::{Path, PathBuf},
    sync::Arc,
    time::{Duration, SystemTime},
};

const KEY: [u8; 32] = [7; 32];
const MIB: usize = 1 << 20;
const TAG: usize = 16;
/// The file key sealed at the head of `original.enc`: magic, kind byte,
/// nonce, key and tag.
const HEADER: usize = 8 + 1 + 12 + 32 + TAG;
/// A full chunk as stored.
const SEALED_CHUNK: usize = MIB + TAG;

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
    fn document_dir(&self, id: &str) -> PathBuf {
        self.root.join("docs").join(id)
    }
    fn original_file(&self, id: &str) -> PathBuf {
        self.document_dir(id).join("original.enc")
    }
    /// The folders under `docs/`, sorted.
    fn document_dirs(&self) -> Vec<String> {
        let mut dirs: Vec<_> = fs::read_dir(self.root.join("docs"))
            .unwrap()
            .map(|entry| entry.unwrap().file_name().into_string().unwrap())
            .collect();
        dirs.sort();
        dirs
    }
}

/// `len` bytes that differ from chunk to chunk.
fn bytes(len: usize, seed: u64) -> Vec<u8> {
    let mut state = seed.wrapping_mul(0x9e37_79b9_7f4a_7c15) | 1;
    (0..len)
        .map(|_| {
            state ^= state << 13;
            state ^= state >> 7;
            state ^= state << 17;
            state as u8
        })
        .collect()
}

fn new_document(title: &str) -> NewDocument {
    NewDocument {
        project: None,
        title: title.into(),
        source_kind: SourceKind::Pdf,
        origin: DocumentOrigin::Ordinary,
        markdown: "# Brief".into(),
    }
}

fn info(file_name: &str) -> OriginalInfo {
    OriginalInfo {
        file_name: file_name.into(),
        ..OriginalInfo::default()
    }
}

fn import(vault: &Vault, source: &[u8]) -> Document {
    vault
        .import_document(new_document("Brief"), source, info("brief.pdf"))
        .unwrap()
}

fn read_back(vault: &Vault, id: &str) -> Vec<u8> {
    let mut out = Vec::new();
    vault
        .open_original(id)
        .unwrap()
        .read_to_end(&mut out)
        .unwrap();
    out
}

fn ids(documents: Vec<mdoc_vault::DocumentSummary>) -> Vec<String> {
    let mut ids: Vec<_> = documents.into_iter().map(|d| d.id).collect();
    ids.sort();
    ids
}

/// The original neither reads nor copies, and a copy leaves no file.
fn assert_does_not_open(vault: &Vault, id: &str, what: &str) {
    match vault.open_original(id) {
        Err(VaultError::Damaged {
            kind: ObjectKind::Original,
            ..
        }) => {}
        Err(other) => panic!("{what}: {other}"),
        Ok(mut reader) => {
            let error = reader.read_to_end(&mut Vec::new()).expect_err(what);
            assert_eq!(error.kind(), io::ErrorKind::InvalidData, "{what}");
            let again = reader.read(&mut [0; 16]).expect_err(what);
            assert_eq!(again.kind(), io::ErrorKind::InvalidData, "{what}");
        }
    }
    let copies = tempfile::tempdir().unwrap();
    let copied = vault.copy_original_to(id, copies.path(), "view.pdf");
    assert!(
        matches!(
            copied,
            Err(VaultError::Damaged {
                kind: ObjectKind::Original,
                ..
            })
        ),
        "{what}: {copied:?}"
    );
    assert_eq!(
        fs::read_dir(copies.path()).unwrap().count(),
        0,
        "{what}: the copy left a file"
    );
}

#[test]
fn originals_round_trip_at_chunk_edges() {
    let folder = Folder::new();
    let vault = folder.open();
    for len in [0, 1, MIB - 1, MIB, MIB + 1, 3 * MIB] {
        let source = bytes(len, len as u64);
        let document = import(&vault, &source);
        assert_eq!(read_back(&vault, &document.id), source, "{len}");
        let original = document.original.clone().unwrap();
        assert_eq!(original.len, len as u64);
        assert_eq!(original.sha256, source_sha256(&source[..]).unwrap());
        assert_eq!(original.info, info("brief.pdf"));
        // One chunk per started MiB, and an empty source still has one.
        let chunks = len.div_ceil(MIB).max(1);
        let stored = fs::metadata(folder.original_file(&document.id)).unwrap();
        assert_eq!(stored.len() as usize, HEADER + len + chunks * TAG, "{len}");
        assert_eq!(vault.document(&document.id).unwrap(), document);
    }
}

#[test]
fn an_original_reads_in_small_pieces_across_chunks() {
    let folder = Folder::new();
    let vault = folder.open();
    let source = bytes(2 * MIB + 3, 5);
    let document = import(&vault, &source);
    let mut reader = vault.open_original(&document.id).unwrap();
    let (mut out, mut piece) = (Vec::new(), [0; 4093]);
    loop {
        let n = reader.read(&mut piece).unwrap();
        if n == 0 {
            break;
        }
        out.extend_from_slice(&piece[..n]);
    }
    assert_eq!(out, source);
    assert_eq!(reader.read(&mut piece).unwrap(), 0);
}

#[test]
fn source_hashes_are_sha256_in_lowercase_hex() {
    assert_eq!(
        source_sha256(&b"abc"[..]).unwrap(),
        "ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad"
    );
    assert_eq!(
        source_sha256(io::empty()).unwrap(),
        "e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855"
    );
}

#[test]
fn a_damaged_cut_reordered_or_extended_original_does_not_open() {
    let folder = Folder::new();
    let vault = folder.open();
    // Three full chunks and a short final one.
    let source = bytes(3 * MIB + 5, 3);
    let document = import(&vault, &source);
    let file = folder.original_file(&document.id);
    let sealed = fs::read(&file).unwrap();
    assert_eq!(sealed.len(), HEADER + 3 * SEALED_CHUNK + 5 + TAG);
    let chunk = |i: usize| HEADER + i * SEALED_CHUNK;
    let mut cases: Vec<(&str, Vec<u8>)> = Vec::new();
    for (what, at) in [
        ("a flipped byte in the magic", 0),
        ("a flipped byte in the kind byte", 8),
        ("a flipped byte in the sealed key", HEADER - 20),
        ("a flipped byte in chunk 0", chunk(0) + 10),
        ("a flipped byte in chunk 1's tag", chunk(2) - 1),
        ("a flipped byte in the final chunk", chunk(3) + 2),
        ("a flipped byte in the last tag", sealed.len() - 1),
    ] {
        let mut damaged = sealed.clone();
        damaged[at] ^= 1;
        cases.push((what, damaged));
    }
    cases.push(("the final chunk dropped", sealed[..chunk(3)].to_vec()));
    cases.push(("the last two chunks dropped", sealed[..chunk(2)].to_vec()));
    cases.push(("a cut inside a chunk", sealed[..chunk(1) + 100].to_vec()));
    cases.push((
        "a cut inside the last tag",
        sealed[..sealed.len() - 3].to_vec(),
    ));
    cases.push(("the header only", sealed[..HEADER].to_vec()));
    cases.push(("a cut inside the header", sealed[..HEADER - 1].to_vec()));
    cases.push(("an empty file", Vec::new()));
    cases.push(("a byte appended", [&sealed[..], &[0]].concat()));
    cases.push((
        "a chunk appended",
        [&sealed[..], &sealed[chunk(1)..chunk(2)]].concat(),
    ));
    let mut swapped = sealed.clone();
    swapped[chunk(0)..chunk(1)].copy_from_slice(&sealed[chunk(1)..chunk(2)]);
    swapped[chunk(1)..chunk(2)].copy_from_slice(&sealed[chunk(0)..chunk(1)]);
    cases.push(("chunks 0 and 1 swapped", swapped));
    let mut repeated = sealed.clone();
    repeated[chunk(1)..chunk(2)].copy_from_slice(&sealed[chunk(0)..chunk(1)]);
    cases.push(("chunk 0 in chunk 1's place", repeated));
    for (what, damaged) in cases {
        fs::write(&file, &damaged).unwrap();
        assert_does_not_open(&vault, &document.id, what);
    }
    fs::write(&file, &sealed).unwrap();
    assert_eq!(read_back(&vault, &document.id), source);
}

#[test]
fn an_original_ending_on_a_chunk_boundary_cannot_be_cut_or_extended() {
    let folder = Folder::new();
    let vault = folder.open();
    // Two full chunks, the second flagged final.
    let source = bytes(2 * MIB, 4);
    let document = import(&vault, &source);
    let file = folder.original_file(&document.id);
    let sealed = fs::read(&file).unwrap();
    assert_eq!(sealed.len(), HEADER + 2 * SEALED_CHUNK);
    for (what, damaged) in [
        (
            "the final chunk dropped",
            sealed[..HEADER + SEALED_CHUNK].to_vec(),
        ),
        ("an empty chunk appended", [&sealed[..], &[0; TAG]].concat()),
        (
            "the final chunk repeated",
            [&sealed[..], &sealed[HEADER + SEALED_CHUNK..]].concat(),
        ),
    ] {
        fs::write(&file, &damaged).unwrap();
        assert_does_not_open(&vault, &document.id, what);
    }
}

#[test]
fn an_original_placed_under_another_document_does_not_open() {
    let folder = Folder::new();
    let vault = folder.open();
    let first = import(&vault, &bytes(MIB + 3, 6));
    let second = import(&vault, &bytes(MIB + 3, 7));
    let without = vault.create_document(new_document("Pasted")).unwrap();
    fs::copy(
        folder.original_file(&first.id),
        folder.original_file(&second.id),
    )
    .unwrap();
    assert_does_not_open(&vault, &second.id, "another document's original");
    fs::copy(
        folder.original_file(&first.id),
        folder.original_file(&without.id),
    )
    .unwrap();
    assert_does_not_open(&vault, &without.id, "an original added to a document");
    assert_eq!(read_back(&vault, &first.id), bytes(MIB + 3, 6));
}

#[test]
fn an_original_is_kept_only_where_there_is_one() {
    let folder = Folder::new();
    let vault = folder.open();
    let pasted = vault.create_document(new_document("Pasted")).unwrap();
    assert_eq!(pasted.original, None);
    assert!(matches!(
        vault.open_original(&pasted.id),
        Err(VaultError::NotFound {
            kind: ObjectKind::Original,
            ..
        })
    ));
    assert!(matches!(
        vault.open_original("not an id"),
        Err(VaultError::InvalidId)
    ));
}

#[test]
fn another_vault_key_does_not_open_an_original() {
    let folder = Folder::new();
    let vault = folder.open();
    let document = import(&vault, &bytes(10, 8));
    drop(vault);
    // Records and originals share the key, so the folder refuses it whole.
    assert!(matches!(
        Vault::open(&folder.root, &[9; 32]),
        Err(VaultError::WrongKey)
    ));
    assert_eq!(read_back(&folder.open(), &document.id), bytes(10, 8));
}

#[test]
fn a_viewing_copy_goes_to_the_folder_the_caller_names() {
    let folder = Folder::new();
    let vault = folder.open();
    let source = bytes(MIB + 7, 9);
    let document = import(&vault, &source);
    let cache = folder._temp.path().join("view cache");
    let path = (vault.copy_original_to(&document.id, &cache, "brief.pdf")).unwrap();
    assert_eq!(path, cache.join("brief.pdf"));
    assert_eq!(fs::read(&path).unwrap(), source);
    fs::write(&path, b"stale").unwrap();
    (vault.copy_original_to(&document.id, &cache, "brief.pdf")).unwrap();
    assert_eq!(fs::read(&path).unwrap(), source);
    assert_eq!(fs::read_dir(&cache).unwrap().count(), 1);
    for name in [
        "",
        ".",
        "..",
        "../brief.pdf",
        "sub/brief.pdf",
        "sub\\brief.pdf",
        "C:brief.pdf",
        "brief.pdf:stream",
    ] {
        assert!(
            matches!(
                vault.copy_original_to(&document.id, &cache, name),
                Err(VaultError::InvalidCopyTarget)
            ),
            "{name:?}"
        );
    }
    // Never into the Vault, where it could replace the sealed original or
    // the record, by any path to it.
    let docs = folder.root.join("docs");
    let own = folder.document_dir(&document.id);
    let by_alias = docs.join("..").join("docs").join(&document.id);
    for (dir, name) in [
        (&own, "original.enc"),
        (&own, "doc.enc"),
        (&by_alias, "original.enc"),
        (&folder.root, "brief.pdf"),
        (&docs.join("new"), "brief.pdf"),
    ] {
        assert!(
            matches!(
                vault.copy_original_to(&document.id, dir, name),
                Err(VaultError::InvalidCopyTarget)
            ),
            "{dir:?} {name}"
        );
    }
    assert_eq!(read_back(&vault, &document.id), source);
    assert!(matches!(
        vault.copy_original_to(&"0".repeat(32), &cache, "brief.pdf"),
        Err(VaultError::NotFound {
            kind: ObjectKind::Original,
            ..
        })
    ));
    assert_eq!(fs::read_dir(&cache).unwrap().count(), 1);
}

#[test]
fn a_second_import_of_the_same_bytes_finds_the_first() {
    let folder = Folder::new();
    let vault = folder.open();
    let source = bytes(MIB + 11, 10);
    let hash = source_sha256(&source[..]).unwrap();
    assert_eq!(vault.documents_with_source(&hash), vec![]);
    let first = import(&vault, &source);
    import(&vault, &bytes(MIB + 11, 11));
    vault.create_document(new_document("Pasted")).unwrap();
    assert_eq!(
        ids(vault.documents_with_source(&hash)),
        vec![first.id.clone()]
    );
    assert_eq!(
        ids(vault.documents_with_source(&hash.to_uppercase())),
        vec![first.id.clone()]
    );
    // A second handle, and a manifest rebuilt from the records, find it.
    assert_eq!(
        ids(folder.open().documents_with_source(&hash)),
        vec![first.id.clone()]
    );
    drop(vault);
    fs::remove_file(folder.root.join("manifest.enc")).unwrap();
    let vault = folder.open();
    assert_eq!(
        ids(vault.documents_with_source(&hash)),
        vec![first.id.clone()]
    );
    // Import again: both are found; a deleted one no longer is.
    let second = import(&vault, &source);
    let mut both = vec![first.id.clone(), second.id.clone()];
    both.sort();
    assert_eq!(ids(vault.documents_with_source(&hash)), both);
    vault.delete_document(&first.id).unwrap();
    assert_eq!(ids(vault.documents_with_source(&hash)), vec![second.id]);
    assert!(!folder.document_dir(&first.id).exists());
}

#[test]
fn a_save_or_update_keeps_the_stored_original() {
    let folder = Folder::new();
    let vault = folder.open();
    let document = import(&vault, &bytes(100, 12));
    let mut copy = document.clone();
    copy.original = None;
    copy.revision += 1;
    vault.save_document(&copy).unwrap();
    assert_eq!(
        vault.document(&document.id).unwrap().original,
        document.original
    );
    let updated = vault
        .update_document(&document.id, |d| d.original = None)
        .unwrap();
    assert_eq!(updated.original, document.original);
    let hash = &document.original.unwrap().sha256;
    assert_eq!(ids(vault.documents_with_source(hash)), vec![document.id]);
}

#[test]
fn a_password_protected_source_stays_locked() {
    let fixture = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../tests/fixtures/locked-office/libreoffice-locked.docx");
    let locked = fs::read(fixture).unwrap();
    let folder = Folder::new();
    let vault = folder.open();
    let given = OriginalInfo {
        file_name: "libreoffice-locked.docx".into(),
        password_protected: true,
        import_warning: Some("This file is password-protected.".into()),
        left_out: Some("Left out: 2 comments.".into()),
    };
    let new = NewDocument {
        source_kind: SourceKind::Word,
        ..new_document("Locked")
    };
    let document = vault
        .import_document(new, &locked[..], given.clone())
        .unwrap();
    // Byte for byte as imported: still the encrypted compound file.
    let back = read_back(&folder.open(), &document.id);
    assert_eq!(back, locked);
    assert!(back.starts_with(&[0xd0, 0xcf, 0x11, 0xe0]));
    let reopened = folder.open().document(&document.id).unwrap();
    assert_eq!(reopened.original.unwrap().info, given);
}

#[test]
fn an_import_goes_only_into_an_existing_project() {
    let folder = Folder::new();
    let vault = folder.open();
    let project = vault
        .create_project(NewProject {
            name: "Lease".into(),
            ..NewProject::default()
        })
        .unwrap();
    let new = NewDocument {
        project: Some(project.id.clone()),
        ..new_document("Brief")
    };
    let filed = (vault.import_document(new, &b"source"[..], info("a.pdf"))).unwrap();
    assert_eq!(filed.project, Some(project.id));
    let missing = NewDocument {
        project: Some("0".repeat(32)),
        ..new_document("Brief")
    };
    assert!(matches!(
        vault.import_document(missing, &b"source"[..], info("a.pdf")),
        Err(VaultError::NotFound {
            kind: ObjectKind::Project,
            ..
        })
    ));
    // The refused import left nothing behind.
    assert_eq!(folder.document_dirs(), vec![filed.id]);
}

/// A source that fails after `left` bytes.
struct Unplugged {
    left: usize,
}

impl Read for Unplugged {
    fn read(&mut self, buf: &mut [u8]) -> io::Result<usize> {
        if self.left == 0 {
            return Err(io::Error::other("the source went away"));
        }
        let n = buf.len().min(self.left);
        buf[..n].fill(1);
        self.left -= n;
        Ok(n)
    }
}

#[test]
fn a_source_that_fails_part_way_leaves_nothing() {
    let folder = Folder::new();
    let vault = folder.open();
    let kept = import(&vault, b"kept");
    let failed = vault.import_document(
        new_document("Broken"),
        Unplugged {
            left: MIB + MIB / 2,
        },
        info("broken.pdf"),
    );
    assert!(matches!(failed, Err(VaultError::Io(_))), "{failed:?}");
    assert_eq!(folder.document_dirs(), vec![kept.id.clone()]);
    assert_eq!(ids(vault.documents()), vec![kept.id]);
}

#[test]
fn a_crash_during_an_import_leaves_no_document() {
    for (kind, stage) in [
        (ObjectKind::Original, Stage::BeforeRename),
        (ObjectKind::Original, Stage::AfterRename),
        (ObjectKind::Document, Stage::BeforeRename),
    ] {
        let folder = Folder::new();
        let source = bytes(MIB + 1, 13);
        let kept = import(&folder.open(), b"kept");
        let hook: FaultHook = Arc::new(move |step: &WriteStep<'_>| {
            if step.kind == kind && step.stage == stage {
                Fault::Crash
            } else {
                Fault::Continue
            }
        });
        let vault = Vault::open_with_hook(&folder.root, &KEY, hook).unwrap();
        let crashed = vault.import_document(new_document("Brief"), &source[..], info("b.pdf"));
        assert!(
            matches!(crashed, Err(VaultError::Crashed)),
            "{kind} {stage:?}"
        );
        drop(vault);
        let vault = folder.open();
        assert_eq!(
            ids(vault.documents()),
            vec![kept.id.clone()],
            "{kind} {stage:?}"
        );
        assert_eq!(vault.damaged(), vec![]);
        let hash = source_sha256(&source[..]).unwrap();
        assert_eq!(vault.documents_with_source(&hash), vec![]);
        // Its folder stays while fresh: an import may still be writing it.
        let dirs = folder.document_dirs();
        assert_eq!(dirs.len(), 2, "{kind} {stage:?}");
        assert_eq!(read_back(&vault, &kept.id), b"kept");
        // An original whose record was never written does not open.
        let orphan = dirs.into_iter().find(|id| *id != kept.id).unwrap();
        let not_found = |result: Result<(), VaultError>| {
            matches!(
                result,
                Err(VaultError::NotFound {
                    kind: ObjectKind::Original,
                    ..
                })
            )
        };
        assert!(not_found(vault.open_original(&orphan).map(drop)));
        let copies = tempfile::tempdir().unwrap();
        let copied = vault.copy_original_to(&orphan, copies.path(), "b.pdf");
        assert!(not_found(copied.map(drop)), "{kind} {stage:?}");
    }
}

#[test]
fn a_record_too_large_to_read_back_is_not_written() {
    let folder = Folder::new();
    let vault = folder.open();
    let kept = import(&vault, b"kept");
    // Sealed records are read back up to 64 MiB; this one's JSON is larger.
    let huge = NewDocument {
        markdown: "a".repeat(64 * MIB),
        ..new_document("Huge")
    };
    let too_large = |result: Result<Document, VaultError>| {
        matches!(
            result,
            Err(VaultError::TooLarge {
                kind: ObjectKind::Document,
                ..
            })
        )
    };
    assert!(too_large(vault.import_document(
        huge.clone(),
        &b"source"[..],
        info("huge.md")
    )));
    assert!(too_large(vault.create_document(huge)));
    // The refused import left nothing, and no match.
    assert_eq!(folder.document_dirs(), vec![kept.id.clone()]);
    let hash = source_sha256(&b"source"[..]).unwrap();
    assert_eq!(folder.open().documents_with_source(&hash), vec![]);
    assert_eq!(ids(vault.documents()), vec![kept.id]);
}

fn age(path: &Path, by: Duration) {
    let file = fs::File::options().write(true).open(path).unwrap();
    file.set_modified(SystemTime::now() - by).unwrap();
}

#[test]
fn an_open_removes_a_dead_imports_folder_after_a_day() {
    const HOUR: Duration = Duration::from_secs(60 * 60);
    let folder = Folder::new();
    let kept = import(&folder.open(), b"kept");
    // A Document's own folder is never removed, however old.
    age(&folder.original_file(&kept.id), 48 * HOUR);
    let orphan = |id: &str, files: &[&str], by: Duration| {
        let dir = folder.document_dir(id);
        fs::create_dir_all(&dir).unwrap();
        for name in files {
            fs::write(dir.join(name), b"sealed source").unwrap();
            age(&dir.join(name), by);
        }
    };
    let (old, partial, fresh, recent, stalled) = (
        "a".repeat(32),
        "b".repeat(32),
        "c".repeat(32),
        "d".repeat(32),
        "e".repeat(32),
    );
    orphan(&old, &["original.enc"], 48 * HOUR);
    orphan(&partial, &[".import-x1"], 25 * HOUR);
    orphan(&fresh, &["original.enc"], Duration::ZERO);
    orphan(&recent, &["original.enc", ".import-x2"], 23 * HOUR);
    // One old file beside a newer one: still being written.
    age(&folder.document_dir(&recent).join(".import-x2"), 48 * HOUR);
    // An import whose source stalled for two days still holds its lock.
    orphan(&stalled, &[".import-x3"], 48 * HOUR);
    let held = fs::File::options()
        .read(true)
        .write(true)
        .open(folder.document_dir(&stalled).join(".import-x3"))
        .unwrap();
    held.lock().unwrap();
    let vault = folder.open();
    let mut expected = vec![kept.id.clone(), fresh.clone(), recent.clone(), stalled];
    expected.sort();
    assert_eq!(folder.document_dirs(), expected);
    assert_eq!(read_back(&vault, &kept.id), b"kept");
    // Once its lock is gone, it is a dead import's too.
    drop(held);
    drop(folder.open());
    let mut expected = vec![kept.id.clone(), fresh, recent];
    expected.sort();
    assert_eq!(folder.document_dirs(), expected);
}
