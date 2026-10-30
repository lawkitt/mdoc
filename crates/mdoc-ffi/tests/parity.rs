//! Parity (ADR 0085): the same document and candidates give the same bytes
//! through the FFI objects as through the desktop's GUI-free path, import →
//! review → Copy for AI → restore. The goldens under
//! `tests/fixtures/mobile-parity/` are the Swift parity suite's too; with
//! `MDOC_BLESS_PARITY=1` this test rewrites them.
use mdoc_ffi::{
    DocumentSession, Library,
    document_session::{Candidate, Category, ImportOptions, MentionState, ReviewSnapshot},
    library::TextRange,
};
use mdoc_history::{EditorTransaction, HistoryChange, SourceEdit};
use mdoc_pii::{
    Detection, Recognizer, Review,
    restore::{ExportedIdentity, RestoreMap},
};
use std::{
    collections::HashMap,
    path::{Path, PathBuf},
    sync::{Arc, atomic::AtomicBool},
};

const KEY: [u8; 32] = [42; 32];

fn repo() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../..")
}

fn read(path: &Path) -> String {
    std::fs::read_to_string(path).unwrap_or_else(|e| panic!("{}: {e}", path.display()))
}

/// A scratch folder for one test's library, removed afterwards.
struct Scratch(PathBuf);
impl Scratch {
    fn new(name: &str) -> Self {
        let path =
            std::env::temp_dir().join(format!("mdoc-ffi-parity-{name}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&path);
        std::fs::create_dir_all(&path).unwrap();
        Self(path)
    }
}
impl Drop for Scratch {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

/// A captured reply's `identities.json` (tests/fixtures/restore/README.md).
fn identities(json: &str) -> Vec<ExportedIdentity> {
    let list: serde_json::Value = serde_json::from_str(json).unwrap();
    list.as_array()
        .unwrap()
        .iter()
        .map(|entry| {
            let alias = entry["alias"].as_str().unwrap().to_string();
            let token = entry["category"].as_str().unwrap();
            let category = *mdoc_pii::Category::ALL
                .iter()
                .find(|c| c.token() == token)
                .unwrap();
            ExportedIdentity {
                custom: !alias.starts_with(&format!("{token}_")),
                previous: Vec::new(),
                originals: entry["originals"]
                    .as_array()
                    .unwrap()
                    .iter()
                    .map(|o| Arc::from(o.as_str().unwrap()))
                    .collect(),
                category,
                alias,
            }
        })
        .collect()
}

// ---- The desktop's GUI-free path ------------------------------------------

/// What desktop Apply does to the review and text: pending plans as one
/// editor step, then the review follows the transaction and commits the
/// replacements (`src/pii/ui.rs` `commit_plans`, `apply_aliases`).
fn desktop_apply_all(review: &mut Review, source: &str, history: u64) -> String {
    let mut plans = review.alias_corrections(source).unwrap();
    plans.extend(review.pending_plans(source, &HashMap::new()).unwrap());
    plans.sort_by_key(|p| p.range.start);
    let added = review.prepare_replacements(&plans);
    let mut text = source.to_string();
    for plan in plans.iter().rev() {
        text.replace_range(plan.range.clone(), &plan.after);
    }
    review.on_transaction(
        &EditorTransaction {
            revision: history,
            changes: vec![HistoryChange {
                before: history - 1,
                after: history,
                edits: plans
                    .iter()
                    .map(|p| SourceEdit {
                        range: p.range.clone(),
                        new_len: p.after.len(),
                    })
                    .collect(),
            }],
            retained: vec![history - 1, history],
        },
        &text,
    );
    review.commit_replacements(history, added);
    review.refresh(&text);
    text
}

/// Desktop Copy for AI (`src/pii/ui/restore.rs` `copy_for_ai`): the note
/// naming the first two distinct applied aliases, then the editor text.
fn desktop_copy_for_ai(review: &Review, text: &str) -> String {
    mdoc_pii::handoff::for_ai(text, review.applied().iter().map(|a| a.step.after.as_ref()))
}

// ---- Goldens --------------------------------------------------------------

fn golden(name: &str, actual: &str) {
    let path = repo().join("tests/fixtures/mobile-parity").join(name);
    if std::env::var_os("MDOC_BLESS_PARITY").is_some() {
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        std::fs::write(&path, actual).unwrap();
        return;
    }
    assert!(
        read(&path) == actual,
        "{name} differs from its golden; rerun with MDOC_BLESS_PARITY=1 after checking why"
    );
}

/// Mentions as the goldens list them: range, state, alias, original.
fn mentions(snapshot: &ReviewSnapshot) -> String {
    snapshot
        .mentions
        .iter()
        .map(|m| {
            format!(
                "{}..{}\t{:?}\t{}\t{}\n",
                m.range.start,
                m.range.end,
                m.state,
                m.alias.as_deref().unwrap_or("-"),
                m.original
            )
        })
        .collect()
}

/// Every whole occurrence of each identity's originals, as a scan would
/// report them, in the identity's category.
fn candidates(
    text: &str,
    identities: &[ExportedIdentity],
) -> Vec<(std::ops::Range<usize>, mdoc_pii::Category)> {
    let mut found = Vec::new();
    for identity in identities {
        for original in &identity.originals {
            for (start, _) in text.match_indices(original.as_ref()) {
                found.push((start..start + original.len(), identity.category));
            }
        }
    }
    found.sort_by_key(|(range, _)| (range.start, range.end));
    found
}

// ---- Tests ----------------------------------------------------------------

/// The captured Claude round trip (ADR 0056): the demo joins a case holding
/// the reviewed mapping, Copy for AI gives exactly what desktop put on the
/// clipboard (`sent.md`), and the reply restores to `expected.md` through
/// the session (desktop's reply box) and through the library (the Restore
/// extension).
#[test]
fn the_captured_claude_round_trip_matches_desktop() {
    let scratch = Scratch::new("claude");
    let capture = repo().join("tests/fixtures/restore/claude-en");
    let mapping = identities(&read(&capture.join("identities.json")));
    let case = mdoc_store::cases::Case::new("Bennett".into(), &mapping, None);
    mdoc_store::cases::Store::with_key(scratch.0.join("cases"), &KEY)
        .save(&case)
        .unwrap();
    let library = Library::new(scratch.0.display().to_string(), KEY.to_vec()).unwrap();
    let demo = repo().join("docs/product-media/Settlement agreement.md");

    let session = DocumentSession::new();
    session
        .import_file(demo.display().to_string(), ImportOptions::default(), None)
        .unwrap();
    session.join_case(library.clone(), case.id.clone()).unwrap();
    let snapshot = session.replace_all().unwrap();
    assert_eq!(snapshot.proposed, 0);
    let sent = read(&capture.join("sent.md"));
    let copied = session.copy_for_ai().unwrap();
    assert_eq!(copied, sent, "Copy for AI differs from desktop's capture");
    assert_eq!(session.share_text().unwrap(), sent);

    // The desktop's GUI-free path on the same input.
    let source = read(&demo);
    let mut review = Review::default();
    review.seed(&case.exported(), &source);
    let text = desktop_apply_all(&mut review, &source, 1);
    assert_eq!(desktop_copy_for_ai(&review, &text), copied);

    let reply = read(&capture.join("reply.md"));
    let expected = read(&capture.join("expected.md"));
    let desktop = RestoreMap::new(review.export_identities()).restore_in_place(&reply);
    assert_eq!(desktop.text, expected);
    let in_session = session.restore_reply(reply.clone()).unwrap();
    assert_eq!(in_session.text, expected);
    assert_eq!(in_session.values, desktop.values as u64);
    assert_eq!(in_session.unrecognised, 0);
    let in_extension = library.restore(case.id.clone(), reply.clone()).unwrap();
    assert_eq!(in_extension.text, expected);

    // Saving keeps the mapping in the case and the reply with the document.
    let saved = session.save(library.clone()).unwrap();
    assert_eq!(saved.case.id, case.id);
    assert_eq!(
        read(Path::new(&saved.path)),
        copied.split_once("\n\n---\n\n").unwrap().1
    );
    library
        .save_reply(case.id.clone(), saved.path.clone(), reply.clone())
        .unwrap();
    assert_eq!(
        library.reply(case.id.clone(), saved.path).unwrap(),
        Some(reply.clone())
    );
    assert_eq!(library.restore(case.id, reply).unwrap().text, expected);
}

/// The demo's DOCX, converted in-process, reviewed with injected candidates
/// (no scan), replaced, copied for AI and restored: the FFI objects and the
/// desktop's GUI-free path give the same bytes, and those are the goldens.
#[test]
fn the_demo_docx_with_injected_candidates_matches_desktop_and_goldens() {
    let docx = repo().join("docs/product-media/Settlement agreement.docx");
    let mapping = identities(&read(
        &repo().join("tests/fixtures/restore/claude-en/identities.json"),
    ));

    let session = DocumentSession::new();
    let opened = session
        .import_file(docx.display().to_string(), ImportOptions::default(), None)
        .unwrap();
    let markdown =
        mdoc_import::import::prepare_cancellable(&docx, None, false, &AtomicBool::new(false))
            .unwrap()
            .markdown;
    assert_eq!(opened.text, markdown, "conversion differs from desktop's");
    assert_eq!(opened.title, "Settlement agreement");
    golden("settlement-docx/markdown.md", &markdown);

    let found = candidates(&markdown, &mapping);
    assert!(found.len() > 20, "{} candidates", found.len());
    let snapshot = session
        .add_candidates(
            found
                .iter()
                .map(|(range, category)| Candidate {
                    range: TextRange {
                        start: range.start as u64,
                        end: range.end as u64,
                    },
                    category: Category::from(*category),
                    score: 0.9,
                })
                .collect(),
        )
        .unwrap();
    assert!(
        snapshot
            .mentions
            .iter()
            .all(|m| m.state == MentionState::Proposed)
    );
    golden("settlement-docx/candidates.tsv", &mentions(&snapshot));
    let replaced = session.replace_all().unwrap();
    golden("settlement-docx/replaced.tsv", &mentions(&replaced));
    let copied = session.copy_for_ai().unwrap();
    golden("settlement-docx/copy-for-ai.md", &copied);

    // The desktop's GUI-free path on the same input.
    let mut review = Review::default();
    review.refresh(&markdown);
    review
        .ingest(
            &markdown,
            found
                .iter()
                .map(|(range, category)| Detection {
                    range: range.clone(),
                    category: *category,
                    score: 0.9,
                    recognizer: Recognizer::Model,
                })
                .collect(),
        )
        .unwrap();
    assert_eq!(review.remaining(), snapshot.proposed as usize);
    let text = desktop_apply_all(&mut review, &markdown, 1);
    assert_eq!(text, replaced.text);
    assert_eq!(desktop_copy_for_ai(&review, &text), copied);

    // An AI that answers with the document itself gets every original back.
    let (_, body) = copied.split_once("\n\n---\n\n").unwrap();
    let reply = format!("Summary of the agreement:\n\n{body}");
    let desktop = RestoreMap::new(review.export_identities()).restore_in_place(&reply);
    let restored = session.restore_reply(reply).unwrap();
    assert_eq!(restored.text, desktop.text);
    assert_eq!(restored.values, desktop.values as u64);
    assert_eq!(
        restored.text,
        format!("Summary of the agreement:\n\n{markdown}")
    );
    golden("settlement-docx/restored.md", &restored.text);
}
