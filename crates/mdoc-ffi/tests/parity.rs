//! Parity (ADR 0085): the same document and candidates give the same bytes
//! through the FFI objects as through the desktop's GUI-free path, import →
//! review → Copy for AI → restore. The goldens under
//! `tests/fixtures/mobile-parity/` are the Swift parity suite's too; with
//! `MDOC_BLESS_PARITY=1` this test rewrites them.
use mdoc_ffi::{
    DocumentSession, Library,
    document_session::{Candidate, Category, ImportOptions, MentionState, ReviewSnapshot, Scope},
    library::TextRange,
};
use mdoc_history::{EditorTransaction, HistoryChange, SourceEdit};
use mdoc_pii::{
    Detection, IdentitySnapshot, Recognizer, Review,
    restore::{ExportedIdentity, RestoreMap},
    tracking::ReplacementPlan,
};
use std::{
    collections::{HashMap, HashSet},
    ops::Range,
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

/// Desktop's editor and document view over one review, GUI-free: the
/// editor's undo stacks and transactions (`crates/mdoc-editor/src/input.rs`)
/// and the view's review calls (`src/pii/ui.rs` `commit_plans`,
/// `src/pii/ui/mapping.rs` `change_mapping` and `prepare_mapping_change`,
/// `src/pii/ui/mapping/popup.rs` `apply_scope` and `keep_mentions_now`).
/// The desktop mapping actions the review bar's calls map to
/// (`MappingAction`).
enum Action<'a> {
    Merge(u64, u64),
    Category(u64, mdoc_pii::Category),
    Owner(u64, Option<u64>),
    Scoped {
        identity: u64,
        ranges: Vec<Range<usize>>,
        alias: Option<&'a str>,
        category: Option<mdoc_pii::Category>,
    },
    AssignCandidate(Range<usize>, Option<u64>),
    AssignApplied(Range<usize>, Option<u64>),
}

struct Desktop {
    review: Review,
    text: String,
    history: u64,
    next: u64,
    revision: u64,
    undo: Vec<(u64, String)>,
    redo: Vec<(u64, String)>,
}

impl Desktop {
    fn new(text: &str, found: &[(Range<usize>, mdoc_pii::Category)]) -> Self {
        let mut review = Review::default();
        review.refresh(text);
        review
            .ingest(
                text,
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
        Self {
            review,
            text: text.into(),
            history: 0,
            next: 0,
            revision: 0,
            undo: Vec::new(),
            redo: Vec::new(),
        }
    }

    fn send(&mut self, before: u64, edits: Vec<SourceEdit>) {
        let mut retained: Vec<_> = self
            .undo
            .iter()
            .chain(&self.redo)
            .map(|(h, _)| *h)
            .chain([self.history])
            .collect();
        retained.sort_unstable();
        retained.dedup();
        self.revision += 1;
        let transaction = EditorTransaction {
            revision: self.revision,
            changes: vec![HistoryChange {
                before,
                after: self.history,
                edits,
            }],
            retained,
        };
        self.review.on_transaction(&transaction, &self.text);
    }

    /// `replace_ranges`, or `checkpoint_metadata` with no edits: one undo point.
    fn commit(&mut self, edits: &[(Range<usize>, String)]) {
        self.undo.push((self.history, self.text.clone()));
        self.redo.clear();
        let before = self.history;
        self.next += 1;
        self.history = self.next;
        for (range, text) in edits.iter().rev() {
            self.text.replace_range(range.clone(), text);
        }
        let edits = if edits.is_empty() {
            vec![SourceEdit {
                range: 0..0,
                new_len: 0,
            }]
        } else {
            edits
                .iter()
                .map(|(range, text)| SourceEdit {
                    range: range.clone(),
                    new_len: text.len(),
                })
                .collect()
        };
        self.send(before, edits);
    }

    fn travel(&mut self, back: bool) {
        let (from, to) = if back {
            (&mut self.undo, &mut self.redo)
        } else {
            (&mut self.redo, &mut self.undo)
        };
        let (history, text) = from.pop().unwrap();
        to.push((self.history, std::mem::replace(&mut self.text, text)));
        let before = std::mem::replace(&mut self.history, history);
        self.send(before, Vec::new());
        // The view's `Changed` rediscovers proposals.
        let text = self.text.clone();
        self.review.refresh(&text);
    }

    fn commit_plans(&mut self, plans: &[ReplacementPlan], old: Option<IdentitySnapshot>) {
        let before = self.history;
        let staged = old.as_ref().map(|old| {
            let next = self.review.identity_snapshot();
            self.review.restore_identity_snapshot(old.clone());
            next
        });
        let added = self.review.prepare_replacements(plans);
        let edits: Vec<_> = plans
            .iter()
            .map(|p| (p.range.clone(), p.after.to_string()))
            .collect();
        self.commit(&edits);
        if let (Some(old), Some(next)) = (old, staged) {
            self.review.restore_identity_snapshot(next);
            self.review
                .commit_identity_snapshot(before, self.history, old);
        }
        self.review.commit_replacements(self.history, added);
    }

    /// The live mentions of identity `id` (`identity_occurrences`).
    fn occurrences(&self, id: u64) -> Vec<Range<usize>> {
        let review = &self.review;
        let mut ranges: Vec<_> = review
            .candidates()
            .iter()
            .filter(|c| review.occurrence_identity(c.variant, &c.range) == Some(id))
            .map(|c| c.range.clone())
            .chain(
                review
                    .applied()
                    .iter()
                    .filter(|a| a.step.identity == id)
                    .map(|a| a.range.clone()),
            )
            .collect();
        ranges.sort_by_key(|r| r.start);
        ranges
    }

    /// `apply_scope` with the popup on the whole entity.
    fn apply(&mut self, id: u64) {
        let ranges: HashSet<_> = self
            .occurrences(id)
            .into_iter()
            .map(|r| (r.start, r.end))
            .collect();
        let plans: Vec<_> = self
            .review
            .pending_plans(&self.text, &HashMap::new())
            .unwrap()
            .into_iter()
            .filter(|p| ranges.contains(&(p.range.start, p.range.end)))
            .collect();
        self.commit_plans(&plans, None);
        let text = self.text.clone();
        self.review.refresh(&text);
    }

    /// Replace all (`apply_aliases`): one step.
    fn apply_all(&mut self) {
        let mut plans = self.review.alias_corrections(&self.text).unwrap();
        plans.extend(
            self.review
                .pending_plans(&self.text, &HashMap::new())
                .unwrap(),
        );
        plans.sort_by_key(|p| p.range.start);
        self.commit_plans(&plans, None);
        let text = self.text.clone();
        self.review.refresh(&text);
    }

    /// `keep_mentions_now` for one pending mention.
    fn keep(&mut self, range: Range<usize>) {
        let variant = self
            .review
            .candidates()
            .iter()
            .find(|c| c.range == range)
            .unwrap()
            .variant;
        let old = self.review.identity_snapshot();
        let before = self.history;
        self.commit(&[]);
        self.review.keep_mentions([(variant, range)]);
        self.review
            .commit_identity_snapshot(before, self.history, old);
    }

    /// `change_mapping` with `prepare_mapping_change` for the actions the
    /// review bar sends (`src/pii/ui/mapping.rs`), applying nothing.
    fn change_mapping(&mut self, action: Action) {
        let source = self.text.clone();
        let old = self.review.identity_snapshot();
        let mut assignments = Vec::new();
        let mut changed: HashMap<u64, u64> = HashMap::new();
        let mut applied_changed: HashMap<u64, u64> = HashMap::new();
        let review = &mut self.review;
        match action {
            Action::Merge(from, target) => {
                review.merge_identity(from, target).unwrap();
                changed.insert(from, target);
                assignments.extend(
                    review
                        .assignments()
                        .iter()
                        .filter(|a| a.identity == from)
                        .map(|a| (a.range.clone(), target)),
                );
            }
            Action::Category(id, category) => {
                review.recategorize_identity(id, category).unwrap();
                changed.insert(id, id);
            }
            Action::Owner(id, owner) => review.set_owner(id, owner).unwrap(),
            Action::Scoped {
                identity,
                ranges,
                alias,
                category,
            } => {
                let ranges: HashSet<_> = ranges.iter().map(|r| (r.start, r.end)).collect();
                let current = review.identity(identity).unwrap().category;
                let target = if ranges.len() == review.identity_count(identity) && alias.is_some() {
                    identity
                } else {
                    review.new_identity(category.unwrap_or(current))
                };
                if let Some(category) = category {
                    review.recategorize_identity(target, category).unwrap();
                }
                if let Some(alias) = alias {
                    review.rename_identity(target, alias).unwrap();
                }
                for c in review.candidates() {
                    if ranges.contains(&(c.range.start, c.range.end))
                        && review.occurrence_identity(c.variant, &c.range) == Some(identity)
                    {
                        assignments.push((c.range.clone(), target));
                    }
                }
                for a in review.applied() {
                    if ranges.contains(&(a.range.start, a.range.end)) && a.step.identity == identity
                    {
                        applied_changed.insert(a.id, target);
                    }
                }
            }
            Action::AssignCandidate(range, target) => {
                let variant = review
                    .candidates()
                    .iter()
                    .find(|c| c.range == range)
                    .unwrap()
                    .variant;
                let category = review.variant(variant).unwrap().category;
                let target = target.unwrap_or_else(|| review.new_identity(category));
                assignments.push((range, target));
            }
            Action::AssignApplied(range, target) => {
                let applied = review
                    .applied()
                    .iter()
                    .find(|a| a.range == range)
                    .unwrap()
                    .clone();
                let target = target.unwrap_or_else(|| review.new_identity(applied.step.category));
                applied_changed.insert(applied.id, target);
            }
        }
        let review = &self.review;
        let mut plans: Vec<_> = review
            .applied()
            .iter()
            .filter_map(|a| {
                let target = applied_changed
                    .get(&a.id)
                    .or_else(|| changed.get(&a.step.identity))?;
                assert_eq!(source.get(a.range.clone()), Some(a.step.after.as_ref()));
                let identity = review.identity(*target)?;
                Some(ReplacementPlan {
                    range: a.range.clone(),
                    before: a.step.after.clone(),
                    after: identity.alias.as_str().into(),
                    category: identity.category,
                    identity: *target,
                })
            })
            .collect();
        plans.sort_by_key(|p| p.range.start);
        let edits: Vec<_> = plans
            .iter()
            .map(|p| (p.range.clone(), p.after.to_string()))
            .collect();
        self.commit_plans(&plans, Some(old));
        assignments.sort_by_key(|(range, _)| range.start);
        assignments.dedup_by(|a, b| a.0 == b.0);
        let (mut index, mut shift) = (0, 0isize);
        for (range, identity) in assignments {
            while index < edits.len() && edits[index].0.end <= range.start {
                shift += edits[index].1.len() as isize - edits[index].0.len() as isize;
                index += 1;
            }
            let range =
                range.start.saturating_add_signed(shift)..range.end.saturating_add_signed(shift);
            self.review.commit_assignment(self.history, range, identity);
        }
        let text = self.text.clone();
        self.review.refresh(&text);
    }

    /// Active identities as `identity_rows` lists a snapshot's.
    fn identities(&self) -> String {
        let review = &self.review;
        let mut rows: Vec<_> = review
            .active_identities()
            .into_iter()
            .map(|id| {
                let i = review.identity(id).unwrap();
                let owner = i
                    .owner
                    .and_then(|o| review.identity(o))
                    .map_or("-".to_owned(), |o| o.alias.clone());
                format!("{}\t{:?}\t{owner}\n", i.alias, Category::from(i.category))
            })
            .collect();
        rows.sort();
        rows.concat()
    }

    /// The mentions as `mentions` lists a snapshot's.
    fn mentions(&self) -> String {
        let review = &self.review;
        let alias = |id: Option<u64>| {
            id.and_then(|id| review.identity(id))
                .map_or("-".to_owned(), |i| i.alias.clone())
        };
        let mut rows: Vec<(Range<usize>, String)> = Vec::new();
        for c in review.candidates() {
            let identity = review.occurrence_identity(c.variant, &c.range);
            let original = &review.variant(c.variant).unwrap().original;
            rows.push((
                c.range.clone(),
                format!("Proposed\t{}\t{original}", alias(identity)),
            ));
        }
        for a in review.applied() {
            rows.push((
                a.range.clone(),
                format!("Replaced\t{}\t{}", a.step.after, a.step.original()),
            ));
        }
        for (range, original, identity) in review.kept_mentions() {
            rows.push((range, format!("Kept\t{}\t{original}", alias(identity))));
        }
        rows.sort_by_key(|(r, _)| (r.start, r.end));
        rows.into_iter()
            .map(|(r, row)| format!("{}..{}\t{row}\n", r.start, r.end))
            .collect()
    }
}

fn text_range(range: &Range<usize>) -> TextRange {
    TextRange {
        start: range.start as u64,
        end: range.end as u64,
    }
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

/// Active identities as the desktop comparison lists them: alias,
/// category and owner's alias.
fn identity_rows(snapshot: &ReviewSnapshot) -> String {
    let alias = |id: u64| {
        snapshot
            .identities
            .iter()
            .find(|i| i.id == id)
            .map_or("-".to_owned(), |i| i.alias.clone())
    };
    let mut rows: Vec<_> = snapshot
        .identities
        .iter()
        .map(|i| {
            let owner = i.owner.map_or("-".to_owned(), alias);
            format!("{}\t{:?}\t{owner}\n", i.alias, i.category)
        })
        .collect();
    rows.sort();
    rows.concat()
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

    // An AI that answers with the document itself gets every original back;
    // "Maya" alone joined her full name (ADR 0092), so it comes back as that.
    let (_, body) = copied.split_once("\n\n---\n\n").unwrap();
    let reply = format!("Summary of the agreement:\n\n{body}");
    let desktop = RestoreMap::new(review.export_identities()).restore_in_place(&reply);
    let restored = session.restore_reply(reply).unwrap();
    assert_eq!(restored.text, desktop.text);
    assert_eq!(restored.values, desktop.values as u64);
    assert_eq!(
        restored.text,
        format!(
            "Summary of the agreement:\n\n{}",
            markdown.replacen("Maya brought", "Maya Bennett brought", 1)
        )
    );
    golden("settlement-docx/restored.md", &restored.text);
}

/// ADR 0098: Replace, Keep, Same as (a whole-identity merge) and Rename,
/// then Undo and Redo, reach the same text and mentions through the FFI as
/// through desktop's editor and mapping changes, step by step.
#[test]
fn identity_tools_and_undo_match_desktop() {
    let text = "Maya Bennett met Maya Benett. Maya Benett and Ann Lee signed. Ann Lee paid.";
    let found = candidates(
        text,
        &identities(
            r#"[{"alias": "PERSON_1", "category": "PERSON", "originals": ["Maya Bennett"]},
                {"alias": "PERSON_2", "category": "PERSON", "originals": ["Maya Benett"]},
                {"alias": "PERSON_3", "category": "PERSON", "originals": ["Ann Lee"]}]"#,
        ),
    );
    let session = DocumentSession::new();
    session.import_text(text.into(), "Letter".into()).unwrap();
    let mut snapshot = session
        .add_candidates(
            found
                .iter()
                .map(|(range, category)| Candidate {
                    range: text_range(range),
                    category: Category::from(*category),
                    score: 0.9,
                })
                .collect(),
        )
        .unwrap();
    let mut desktop = Desktop::new(text, &found);
    let same = |snapshot: &ReviewSnapshot, desktop: &Desktop, step: &str| {
        assert_eq!(snapshot.text, desktop.text, "{step}: text");
        assert_eq!(mentions(snapshot), desktop.mentions(), "{step}: mentions");
    };
    same(&snapshot, &desktop, "scan");
    let at = |snapshot: &ReviewSnapshot, wording: &str, nth: usize| {
        let start = snapshot.text.match_indices(wording).nth(nth).unwrap().0;
        start..start + wording.len()
    };
    let id = |desktop: &Desktop, alias: &str| desktop.review.identity_by_alias(alias).unwrap();

    // Replace Maya Bennett's mentions; keep Ann Lee's second.
    let maya = at(&snapshot, "Maya Bennett", 0);
    snapshot = session.replace(text_range(&maya), Scope::All).unwrap();
    desktop.apply(id(&desktop, "PERSON_1"));
    same(&snapshot, &desktop, "replace");
    let ann = at(&snapshot, "Ann Lee", 1);
    snapshot = session.keep(text_range(&ann), Scope::This).unwrap();
    desktop.keep(ann);
    same(&snapshot, &desktop, "keep");

    // Same as, All: the slip joins Maya Bennett, who keeps PERSON_1.
    let slip = at(&snapshot, "Maya Benett", 0);
    let (from, into) = (id(&desktop, "PERSON_2"), id(&desktop, "PERSON_1"));
    snapshot = session
        .same_as(text_range(&slip), Scope::All, into)
        .unwrap();
    desktop.change_mapping(Action::Merge(from, into));
    same(&snapshot, &desktop, "same as");

    // Rename, All: every mention of the identity takes CLIENT.
    let maya = at(&snapshot, "PERSON_1", 0);
    snapshot = session
        .rename(text_range(&maya), Scope::All, "CLIENT".into())
        .unwrap();
    let ranges = desktop.occurrences(into);
    desktop.change_mapping(Action::Scoped {
        identity: into,
        ranges,
        alias: Some("CLIENT"),
        category: None,
    });
    same(&snapshot, &desktop, "rename");
    snapshot = session.replace_all().unwrap();
    desktop.apply_all();
    same(&snapshot, &desktop, "replace all");

    // Undo every step, then redo them, comparing each state.
    let mut undone = 0;
    while snapshot.can_undo {
        snapshot = session.undo().unwrap();
        desktop.travel(true);
        undone += 1;
        same(&snapshot, &desktop, &format!("undo {undone}"));
    }
    assert_eq!(snapshot.text, text);
    while snapshot.can_redo {
        snapshot = session.redo().unwrap();
        desktop.travel(false);
        same(&snapshot, &desktop, "redo");
    }
    assert!(desktop.redo.is_empty());
    assert_eq!(
        snapshot.text,
        "CLIENT met CLIENT. CLIENT and PERSON_3 signed. Ann Lee paid."
    );
}

/// ADR 0098's This-one paths and the other identity tools: category (All
/// and This one), New alias, Same as one mention, Belongs to…, each with
/// every undo and redo, reach desktop's text, mentions and identities.
#[test]
fn scoped_identity_tools_and_undo_match_desktop() {
    let text =
        "Acme Ltd hired Ann Lee. Acme Ltd paid Ann Lee at ann@acme.example. Bob Hart signed.";
    let found = candidates(
        text,
        &identities(
            r#"[{"alias": "PERSON_1", "category": "PERSON", "originals": ["Acme Ltd"]},
                {"alias": "PERSON_2", "category": "PERSON", "originals": ["Ann Lee"]},
                {"alias": "EMAIL_1", "category": "EMAIL", "originals": ["ann@acme.example"]},
                {"alias": "PERSON_3", "category": "PERSON", "originals": ["Bob Hart"]}]"#,
        ),
    );
    let session = DocumentSession::new();
    session.import_text(text.into(), "Letter".into()).unwrap();
    let mut snapshot = session
        .add_candidates(
            found
                .iter()
                .map(|(range, category)| Candidate {
                    range: text_range(range),
                    category: Category::from(*category),
                    score: 0.9,
                })
                .collect(),
        )
        .unwrap();
    let mut desktop = Desktop::new(text, &found);
    let same = |snapshot: &ReviewSnapshot, desktop: &Desktop, step: &str| {
        assert_eq!(snapshot.text, desktop.text, "{step}: text");
        assert_eq!(mentions(snapshot), desktop.mentions(), "{step}: mentions");
        assert_eq!(
            identity_rows(snapshot),
            desktop.identities(),
            "{step}: identities"
        );
    };
    same(&snapshot, &desktop, "scan");
    let at = |snapshot: &ReviewSnapshot, wording: &str, nth: usize| {
        let start = snapshot.text.match_indices(wording).nth(nth).unwrap().0;
        start..start + wording.len()
    };
    let id = |desktop: &Desktop, alias: &str| desktop.review.identity_by_alias(alias).unwrap();

    // Category, All: Acme becomes an organization (`correct_category`).
    let acme = at(&snapshot, "Acme Ltd", 0);
    snapshot = session
        .set_category(text_range(&acme), Scope::All, Category::Organization)
        .unwrap();
    desktop.change_mapping(Action::Category(
        id(&desktop, "PERSON_1"),
        mdoc_pii::Category::Organization,
    ));
    same(&snapshot, &desktop, "category, all");

    // Replace everything, so the next steps move applied mentions.
    snapshot = session.replace_all().unwrap();
    desktop.apply_all();
    same(&snapshot, &desktop, "replace all");

    // Category, This one: the second Ann Lee splits off as Other.
    let ann = id(&desktop, "PERSON_2");
    let second = at(&snapshot, "PERSON_2", 1);
    snapshot = session
        .set_category(text_range(&second), Scope::This, Category::Other)
        .unwrap();
    desktop.change_mapping(Action::Scoped {
        identity: ann,
        ranges: vec![second],
        alias: None,
        category: Some(mdoc_pii::Category::Other),
    });
    same(&snapshot, &desktop, "category, this one");

    // Same as, This one: it moves back to Ann Lee.
    let other = at(&snapshot, "REDACTED_1", 0);
    snapshot = session
        .same_as(text_range(&other), Scope::This, ann)
        .unwrap();
    desktop.change_mapping(Action::AssignApplied(other, Some(ann)));
    same(&snapshot, &desktop, "same as, this one");

    // New alias: Bob Hart's mention splits off.
    let bob = at(&snapshot, "PERSON_3", 0);
    snapshot = session.new_alias(text_range(&bob)).unwrap();
    desktop.change_mapping(Action::AssignApplied(bob, None));
    same(&snapshot, &desktop, "new alias");

    // Belongs to…: the address is Ann Lee's.
    let email = id(&desktop, "EMAIL_1");
    snapshot = session.set_owner(email, Some(ann)).unwrap();
    desktop.change_mapping(Action::Owner(email, Some(ann)));
    same(&snapshot, &desktop, "owner");

    let mut undone = 0;
    while snapshot.can_undo {
        snapshot = session.undo().unwrap();
        desktop.travel(true);
        undone += 1;
        same(&snapshot, &desktop, &format!("undo {undone}"));
    }
    assert_eq!(undone, 6);
    assert_eq!(snapshot.text, text);
    while snapshot.can_redo {
        snapshot = session.redo().unwrap();
        desktop.travel(false);
        same(&snapshot, &desktop, "redo");
    }
    assert!(desktop.redo.is_empty());

    // A proposed mention splits off too (`AssignCandidate`), and undoes.
    let session = DocumentSession::new();
    session.import_text(text.into(), "Letter".into()).unwrap();
    session
        .add_candidates(
            found
                .iter()
                .map(|(range, category)| Candidate {
                    range: text_range(range),
                    category: Category::from(*category),
                    score: 0.9,
                })
                .collect(),
        )
        .unwrap();
    let mut desktop = Desktop::new(text, &found);
    let ann = at(&session.snapshot().unwrap(), "Ann Lee", 1);
    let snapshot = session.new_alias(text_range(&ann)).unwrap();
    desktop.change_mapping(Action::AssignCandidate(ann, None));
    same(&snapshot, &desktop, "new alias, proposed");
    let snapshot = session.undo().unwrap();
    desktop.travel(true);
    same(&snapshot, &desktop, "new alias, proposed, undone");
}
