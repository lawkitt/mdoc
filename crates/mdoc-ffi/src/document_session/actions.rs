//! What a tap on the review screen does, in one core call (ADR 0104, refactor
//! grill Q12): `DocumentSession::review` takes the action by mention id,
//! saves into the library when something changed, and returns the snapshot
//! after it, so the app neither looks the mention up in a snapshot first nor
//! takes a second one. Its refusals are typed (`ReviewError`). Beside it:
//! Copy for AI's hand-off, which refuses while proposals are pending, and
//! what the session holds, read without a snapshot.
use super::{Document, DocumentSession, no_document, review::Scope, span};
use crate::{
    CoreError, ReviewError,
    document_session::{Category, DocumentPhase, ReviewSnapshot},
    library::{Library, TextRange},
};
use std::sync::Arc;

/// One review action. Mentions are named by `Mention::id`, identities by
/// `IdentityRow::id`.
#[derive(Clone, Debug, PartialEq, uniffi::Enum)]
pub enum ReviewAction {
    Replace {
        mention: u64,
        scope: Scope,
    },
    Keep {
        mention: u64,
        scope: Scope,
    },
    /// Every proposal, and applied aliases corrected.
    ReplaceAll,
    /// Selected text as a proposal of every exact repeat, in `category`
    /// unless the wording is proposed already; none: the core's guess.
    Mark {
        range: TextRange,
        category: Option<Category>,
    },
    /// Selected text marked, then every mention of it replaced.
    ReplaceSelection {
        range: TextRange,
        category: Category,
    },
    /// An alias another identity has fails with `ReviewError::AliasTaken`.
    Rename {
        mention: u64,
        scope: Scope,
        alias: String,
    },
    SetCategory {
        mention: u64,
        scope: Scope,
        category: Category,
    },
    SameAs {
        mention: u64,
        scope: Scope,
        target: u64,
    },
    NewAlias {
        mention: u64,
    },
    SetOwner {
        identity: u64,
        owner: Option<u64>,
    },
    /// Same as… from a merge hint, or for a whole identity.
    Link {
        from: u64,
        into: u64,
    },
    DismissHint {
        from: u64,
        into: u64,
    },
    /// Names the app knows (the sample document's), proposed wherever they
    /// occur, longest first, as a scan's are: the document counts as
    /// scanned after.
    ProposeNames {
        names: Vec<ProposedName>,
    },
    Undo,
    Redo,
}

/// A wording to propose, and its category.
#[derive(Clone, Debug, PartialEq, uniffi::Record)]
pub struct ProposedName {
    pub wording: String,
    pub category: Category,
}

/// What Copy for AI and Share send.
#[derive(Clone, Debug, PartialEq, uniffi::Record)]
pub struct HandOff {
    /// The alias-only note, then the text (`copy_for_ai`).
    pub text: String,
    /// The shared `.md` file's name (`share_file_name`).
    pub file_name: String,
}

/// What the session holds, without a snapshot.
#[derive(Clone, Debug, PartialEq, uniffi::Record)]
pub struct SessionInfo {
    pub title: String,
    /// The library file, once saved.
    pub path: Option<String>,
    pub case_id: Option<String>,
    pub scanned: bool,
    pub phase: DocumentPhase,
    pub proposed: u64,
}

/// Every occurrence of `wording` in `text` as UTF-8 byte ranges, left to
/// right without overlaps; NFC and NFD spellings match each other
/// (`mdoc_pii::view::occurrences`).
#[uniffi::export]
pub fn occurrences(text: String, wording: String) -> Vec<TextRange> {
    mdoc_pii::view::occurrences(&text, &wording)
        .iter()
        .map(TextRange::of)
        .collect()
}

#[uniffi::export]
impl DocumentSession {
    /// Apply `action`, then, given `library`, save as `save` does unless
    /// nothing changed since the last save; the snapshot after.
    pub fn review(
        &self,
        action: ReviewAction,
        library: Option<Arc<Library>>,
    ) -> Result<ReviewSnapshot, ReviewError> {
        let mut guard = self.lock_document();
        let document = guard.as_mut().ok_or_else(no_document)?;
        document.apply(action)?;
        match library {
            Some(library) if document.saved != Some(document.change_key()) => {
                Ok(document.save(&library)?.review)
            }
            _ => Ok(document.snapshot()),
        }
    }

    /// Copy for AI and Share: refused while proposals are pending. Given
    /// `library`, the document's case becomes the current one, as sharing
    /// makes it (`Library::note_shared`).
    pub fn hand_off(&self, library: Option<Arc<Library>>) -> Result<HandOff, ReviewError> {
        let guard = self.lock_document();
        let document = guard.as_ref().ok_or_else(no_document)?;
        let count = document.proposed();
        if count > 0 {
            return Err(ReviewError::PendingProposals { count });
        }
        let hand_off = HandOff {
            text: document.review.copy_for_ai(),
            file_name: document.share_file_name(),
        };
        if let (Some(library), Some(case)) = (library, &document.case) {
            library.note_shared(case.clone())?;
        }
        Ok(hand_off)
    }

    /// The title, file, case and phase; `None` before a document is read.
    pub fn info(&self) -> Option<SessionInfo> {
        let guard = self.lock_document();
        let document = guard.as_ref()?;
        let proposed = document.proposed();
        Some(SessionInfo {
            title: document.title.clone(),
            path: document.path.as_ref().map(|p| p.display().to_string()),
            case_id: document.case.clone(),
            scanned: document.scanned,
            phase: DocumentPhase::of(document.scanned, proposed),
            proposed,
        })
    }
}

impl Document {
    /// Pending proposals, as the snapshot counts them.
    fn proposed(&self) -> u64 {
        let review = &self.review.review;
        review
            .candidates()
            .iter()
            .filter(|c| review.variant(c.variant).is_some())
            .count() as u64
    }

    /// The mention with id `id` now.
    fn mention(&mut self, id: u64) -> Result<std::ops::Range<usize>, ReviewError> {
        self.review.mention_range(id).ok_or(ReviewError::NotFound)
    }

    fn identity(&self, id: u64) -> Result<u64, ReviewError> {
        match self.review.review.identity(id) {
            Some(_) => Ok(id),
            None => Err(ReviewError::NotFound),
        }
    }

    fn apply(&mut self, action: ReviewAction) -> Result<(), ReviewError> {
        use ReviewAction as A;
        match action {
            A::Replace { mention, scope } => {
                let range = self.mention(mention)?;
                self.review.replace(range, scope)?;
            }
            A::Keep { mention, scope } => {
                let range = self.mention(mention)?;
                self.review.keep(range, scope)?;
            }
            A::ReplaceAll => self.review.replace_all()?,
            A::Mark { range, category } => {
                self.review
                    .add_selection(span(range), category.map(Into::into))?;
            }
            A::ReplaceSelection { range, category } => {
                let selection = span(range);
                self.review
                    .add_selection(selection.clone(), Some(category.into()))?;
                let covering = self
                    .review
                    .mention_ranges()
                    .into_iter()
                    .find(|r| r.start <= selection.start && selection.end <= r.end);
                if let Some(mention) = covering {
                    self.review.replace(mention, Scope::All)?;
                }
            }
            A::Rename {
                mention,
                scope,
                alias,
            } => {
                let range = self.mention(mention)?;
                let alias = alias.trim();
                let own = self
                    .review
                    .at(&range)
                    .and_then(|at| self.review.entity(&at));
                if let Some(identity) = self
                    .review
                    .identity_with_alias(alias)
                    .filter(|other| Some(*other) != own)
                {
                    return Err(ReviewError::AliasTaken {
                        alias: alias.to_owned(),
                        identity,
                    });
                }
                self.review.rename(range, scope, alias)?;
            }
            A::SetCategory {
                mention,
                scope,
                category,
            } => {
                let range = self.mention(mention)?;
                self.review.set_category(range, scope, category.into())?;
            }
            A::SameAs {
                mention,
                scope,
                target,
            } => {
                let range = self.mention(mention)?;
                let target = self.identity(target)?;
                self.review.same_as(range, scope, target)?;
            }
            A::NewAlias { mention } => {
                let range = self.mention(mention)?;
                self.review.new_alias(range)?;
            }
            A::SetOwner { identity, owner } => {
                let identity = self.identity(identity)?;
                let owner = owner.map(|id| self.identity(id)).transpose()?;
                self.review.set_owner(identity, owner)?;
            }
            A::Link { from, into } => {
                let (from, into) = (self.identity(from)?, self.identity(into)?);
                self.review.link(from, into)?;
            }
            A::DismissHint { from, into } => self.review.dismiss_hint(from, into)?,
            A::ProposeNames { names } => self.propose(names)?,
            A::Undo => {
                self.review.undo();
            }
            A::Redo => {
                self.review.redo();
            }
        }
        Ok(())
    }

    /// `ReviewAction::ProposeNames`: longest wording first, so "Maya
    /// Bennett" wins over a shorter wording inside it.
    fn propose(&mut self, mut names: Vec<ProposedName>) -> Result<(), CoreError> {
        names.sort_by_key(|name| std::cmp::Reverse(name.wording.len()));
        let mut taken: Vec<std::ops::Range<usize>> = Vec::new();
        let mut detections = Vec::new();
        for name in &names {
            for range in mdoc_pii::view::occurrences(&self.review.text, &name.wording) {
                if taken
                    .iter()
                    .any(|t| t.start < range.end && range.start < t.end)
                {
                    continue;
                }
                taken.push(range.clone());
                detections.push(mdoc_pii::Detection {
                    range,
                    category: name.category.into(),
                    score: 1.,
                    recognizer: mdoc_pii::Recognizer::Model,
                });
            }
        }
        detections.sort_by_key(|d| d.range.start);
        self.ingest(detections)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{document_session::MentionState, test_support::Scratch};

    fn names(names: &[(&str, Category)]) -> ReviewAction {
        ReviewAction::ProposeNames {
            names: names
                .iter()
                .map(|(wording, category)| ProposedName {
                    wording: (*wording).into(),
                    category: *category,
                })
                .collect(),
        }
    }

    fn session(text: &str, proposed: &[&str]) -> (Arc<DocumentSession>, ReviewSnapshot) {
        let session = DocumentSession::new();
        session.import_text(text.into(), "Letter".into()).unwrap();
        let people: Vec<_> = proposed.iter().map(|w| (*w, Category::Person)).collect();
        let snapshot = session.review(names(&people), None).unwrap();
        (session, snapshot)
    }

    /// The id of the `nth` mention whose original is `wording`.
    fn id(snapshot: &ReviewSnapshot, wording: &str, nth: usize) -> u64 {
        snapshot
            .mentions
            .iter()
            .filter(|m| m.original == wording)
            .nth(nth)
            .unwrap()
            .id
    }

    fn ids(snapshot: &ReviewSnapshot) -> Vec<(u64, MentionState)> {
        snapshot.mentions.iter().map(|m| (m.id, m.state)).collect()
    }

    /// A mention keeps its id through Replace, Keep, edits elsewhere, Undo
    /// and Redo (ADR 0104).
    #[test]
    fn mention_ids_survive_changes() {
        let (session, start) = session(
            "Ann Lee met Bob Hart. Ann Lee left.",
            &["Ann Lee", "Bob Hart"],
        );
        let (ann, bob, again) = (
            id(&start, "Ann Lee", 0),
            id(&start, "Bob Hart", 0),
            id(&start, "Ann Lee", 1),
        );
        let replaced = session
            .review(
                ReviewAction::Replace {
                    mention: ann,
                    scope: Scope::This,
                },
                None,
            )
            .unwrap();
        assert_eq!(replaced.text, "PERSON_1 met Bob Hart. Ann Lee left.");
        assert_eq!(
            ids(&replaced),
            [
                (ann, MentionState::Replaced),
                (bob, MentionState::Proposed),
                (again, MentionState::Proposed)
            ]
        );
        let kept = session
            .review(
                ReviewAction::Keep {
                    mention: bob,
                    scope: Scope::All,
                },
                None,
            )
            .unwrap();
        assert_eq!(id(&kept, "Bob Hart", 0), bob);
        let all = session.review(ReviewAction::ReplaceAll, None).unwrap();
        assert_eq!(all.text, "PERSON_1 met Bob Hart. PERSON_1 left.");
        assert_eq!(id(&all, "Ann Lee", 1), again);
        let undone = session.review(ReviewAction::Undo, None).unwrap();
        assert_eq!(ids(&undone), ids(&kept));
        let undone = session.review(ReviewAction::Undo, None).unwrap();
        assert_eq!(ids(&undone), ids(&replaced));
        session.review(ReviewAction::Redo, None).unwrap();
        assert_eq!(
            ids(&session.review(ReviewAction::Redo, None).unwrap()),
            ids(&all)
        );
        assert!(matches!(
            session.review(
                ReviewAction::Keep {
                    mention: 999,
                    scope: Scope::This
                },
                None
            ),
            Err(ReviewError::NotFound)
        ));
    }

    /// Each identity's state, and a name kept everywhere listed as kept.
    #[test]
    fn identities_carry_their_state_and_kept_names_stay_listed() {
        let (session, start) = session(
            "Ann Lee met Bob Hart. Ann Lee left.",
            &["Ann Lee", "Bob Hart"],
        );
        assert!(
            start
                .identities
                .iter()
                .all(|i| i.state == MentionState::Proposed)
        );
        assert!(start.kept_identities.is_empty());
        let ann = id(&start, "Ann Lee", 0);
        session
            .review(
                ReviewAction::Replace {
                    mention: ann,
                    scope: Scope::All,
                },
                None,
            )
            .unwrap();
        let bob = id(&start, "Bob Hart", 0);
        let snapshot = session
            .review(
                ReviewAction::Keep {
                    mention: bob,
                    scope: Scope::All,
                },
                None,
            )
            .unwrap();
        let row = |alias: &str| {
            snapshot
                .identities
                .iter()
                .chain(&snapshot.kept_identities)
                .find(|i| i.alias == alias)
                .unwrap()
                .clone()
        };
        assert_eq!(row("PERSON_1").state, MentionState::Replaced);
        assert_eq!(row("PERSON_1").mention_count, 2);
        let kept = row("PERSON_2");
        assert_eq!(kept.state, MentionState::Kept);
        assert_eq!(kept.originals, ["Bob Hart"]);
        assert_eq!(kept.mention_count, 1);
        let listed = snapshot.identities.iter().chain(&snapshot.kept_identities);
        assert_eq!(listed.filter(|i| i.alias == "PERSON_2").count(), 1);
    }

    /// Proposing names that occur nowhere still counts as scanned: the
    /// document is ready, not unscanned.
    #[test]
    fn a_scan_that_found_nothing_is_scanned() {
        let session = DocumentSession::new();
        let imported = session
            .import_text("Nothing here.".into(), "Note".into())
            .unwrap();
        assert!(!imported.scanned);
        assert_eq!(imported.phase, DocumentPhase::Unscanned);
        let scanned = session
            .review(names(&[("Ann Lee", Category::Person)]), None)
            .unwrap();
        assert!(scanned.scanned && scanned.mentions.is_empty());
        assert_eq!(scanned.phase, DocumentPhase::Ready);
        let (_, proposed) = self::session("Ann Lee wrote.", &["Ann Lee"]);
        assert_eq!(proposed.phase, DocumentPhase::Reviewing);
        let scanned = DocumentSession::new();
        scanned
            .import_text("Nothing.".into(), "Note".into())
            .unwrap();
        assert!(scanned.scan(None, None).unwrap().scanned);
    }

    /// Sample names: longest first, NFC and NFD spellings alike.
    #[test]
    fn proposed_names_are_found_in_either_normal_form() {
        let text = "Zoe\u{308} Lane met Maya Bennett.";
        let (_, snapshot) = session(text, &["Zo\u{eb} Lane", "Bennett", "Maya Bennett"]);
        let originals: Vec<_> = snapshot
            .mentions
            .iter()
            .map(|m| m.original.as_str())
            .collect();
        assert_eq!(originals, ["Zoe\u{308} Lane", "Maya Bennett"]);
        assert_eq!(
            occurrences(text.into(), "Zo\u{eb}".into()),
            [TextRange { start: 0, end: 5 }]
        );
    }

    #[test]
    fn rename_to_a_taken_alias_names_its_identity() {
        let (session, start) = session("Ann Lee met Bob Hart.", &["Ann Lee", "Bob Hart"]);
        let bob = start
            .identities
            .iter()
            .find(|i| i.alias == "PERSON_2")
            .unwrap()
            .id;
        let rename = |alias: &str| ReviewAction::Rename {
            mention: id(&start, "Ann Lee", 0),
            scope: Scope::All,
            alias: alias.into(),
        };
        match session.review(rename(" PERSON_2 "), None) {
            Err(ReviewError::AliasTaken { alias, identity }) => {
                assert_eq!((alias.as_str(), identity), ("PERSON_2", bob));
            }
            other => panic!("{other:?}"),
        }
        assert!(matches!(
            session.review(rename("not an alias"), None),
            Err(ReviewError::Refused { .. })
        ));
        let renamed = session.review(rename("CLIENT"), None).unwrap();
        assert!(renamed.identities.iter().any(|i| i.alias == "CLIENT"));
        // Its own alias again is no refusal.
        session.review(rename("CLIENT"), None).unwrap();
    }

    /// Copy for AI refuses while proposals are pending; then it is the
    /// text and file name the separate calls give, and its case is current.
    #[test]
    fn hand_off_waits_for_every_proposal() {
        let scratch = Scratch::new();
        let library = Library::new(scratch.path().display().to_string(), vec![3; 32]).unwrap();
        let (session, _) = session("Ann Lee met Bob Hart.", &["Ann Lee", "Bob Hart"]);
        assert!(matches!(
            session.hand_off(None),
            Err(ReviewError::PendingProposals { count: 2 })
        ));
        let saved = session
            .review(ReviewAction::ReplaceAll, Some(library.clone()))
            .unwrap();
        let hand_off = session.hand_off(Some(library.clone())).unwrap();
        assert_eq!(hand_off.text, session.copy_for_ai().unwrap());
        assert_eq!(hand_off.file_name, "Letter.md");
        assert_eq!(library.current_case().map(|c| c.id), saved.case_id);
    }

    /// With a library each change saves; a call that changes nothing
    /// writes nothing (a save would refuse the file changed elsewhere).
    #[test]
    fn review_saves_only_what_changed() {
        let scratch = Scratch::new();
        let library = Library::new(scratch.path().display().to_string(), vec![3; 32]).unwrap();
        let (session, start) = session("Ann Lee met Bob Hart.", &["Ann Lee", "Bob Hart"]);
        assert_eq!(session.info().unwrap().path, None);
        let ann = id(&start, "Ann Lee", 0);
        let replace = ReviewAction::Replace {
            mention: ann,
            scope: Scope::This,
        };
        let replaced = session
            .review(replace.clone(), Some(library.clone()))
            .unwrap();
        let info = session.info().unwrap();
        let path = info.path.clone().unwrap();
        assert_eq!(std::fs::read_to_string(&path).unwrap(), replaced.text);
        assert_eq!(info.case_id, replaced.case_id);
        assert_eq!((info.title.as_str(), info.proposed), ("Letter", 1));
        assert_eq!(info.phase, DocumentPhase::Reviewing);
        std::fs::write(&path, "Edited in Files.").unwrap();
        session.review(replace, Some(library.clone())).unwrap();
        let bob = id(&replaced, "Bob Hart", 0);
        let keep = ReviewAction::Keep {
            mention: bob,
            scope: Scope::This,
        };
        assert!(matches!(
            session.review(keep, Some(library.clone())),
            Err(ReviewError::Storage { .. })
        ));
        assert!(DocumentSession::new().info().is_none());
    }

    /// Scanned is kept with the document: a scan that found nothing reopens
    /// as scanned, and a document never scanned as unscanned whatever its
    /// mentions; a rename keeps it. Documents saved before it was kept
    /// count as scanned when they have mentions.
    #[test]
    fn scanned_is_kept_with_the_document() {
        let scratch = Scratch::new();
        let library = Library::new(scratch.path().display().to_string(), vec![3; 32]).unwrap();
        let reopened = |path: &str| {
            DocumentSession::new()
                .open_document(library.clone(), path.into())
                .unwrap()
        };
        let empty = DocumentSession::new();
        empty
            .import_text("Nothing here.".into(), "Empty".into())
            .unwrap();
        empty
            .review(names(&[("Ann Lee", Category::Person)]), None)
            .unwrap();
        let empty = empty.save(library.clone()).unwrap().path;
        let again = reopened(&empty);
        assert!(again.scanned);
        assert_eq!(again.phase, DocumentPhase::Ready);
        let marked = DocumentSession::new();
        let text = marked
            .import_text("Ann Lee wrote.".into(), "Marked".into())
            .unwrap();
        let range = TextRange { start: 0, end: 7 };
        assert_eq!(&text.text[..7], "Ann Lee");
        marked
            .review(
                ReviewAction::ReplaceSelection {
                    range,
                    category: Category::Person,
                },
                Some(library.clone()),
            )
            .unwrap();
        let marked = marked.info().unwrap().path.unwrap();
        let again = reopened(&marked);
        assert!(!again.scanned && !again.mentions.is_empty());
        let renamed = library.rename_document(empty, "Renamed".into()).unwrap();
        assert!(reopened(&renamed.path.clone().unwrap()).scanned);
        // Saved before the library kept it.
        std::fs::remove_file(scratch.path().join("documents.json")).unwrap();
        assert!(!reopened(&renamed.path.unwrap()).scanned);
        assert!(reopened(&marked).scanned);
    }

    /// What a review tap costs on a long document (ADR 0104: async calls
    /// only past about 16 ms on an iPhone 12 Pro): about 20 pages, 200
    /// names, 1,600 mentions. `cargo test --release -p mdoc-ffi --lib
    /// measure_a_review_tap -- --ignored --nocapture`.
    #[test]
    #[ignore = "a measurement; run it in release"]
    fn measure_a_review_tap() {
        use std::time::Instant;
        let scratch = Scratch::new();
        let library = Library::new(scratch.path().display().to_string(), vec![3; 32]).unwrap();
        let mut text = String::new();
        for page in 0..20 {
            text += &format!("## Page {page}\n\n");
            for i in 0..80 {
                let n = (page * 80 + i) % 200;
                text += &format!("Clause {i}: Name{n} Family{n} agrees with the terms. ");
            }
            text += "\n\n";
        }
        let session = DocumentSession::new();
        session.import_text(text, "Long".into()).unwrap();
        let people: Vec<_> = (0..200)
            .map(|n| ProposedName {
                wording: format!("Name{n} Family{n}"),
                category: Category::Person,
            })
            .collect();
        let start = session
            .review(
                ReviewAction::ProposeNames { names: people },
                Some(library.clone()),
            )
            .unwrap();
        eprintln!(
            "{} bytes, {} mentions",
            start.text.len(),
            start.mentions.len()
        );
        let time = |label: &str, run: &mut dyn FnMut()| {
            let runs = 20;
            let began = Instant::now();
            for _ in 0..runs {
                run();
            }
            eprintln!(
                "{label}: {:.2} ms",
                began.elapsed().as_secs_f64() * 1e3 / runs as f64
            );
        };
        time("snapshot", &mut || {
            session.snapshot().unwrap();
        });
        time("markdown_styling", &mut || {
            crate::markdown::markdown_styling(start.text.clone());
        });
        let mut mentions = start.mentions.iter().map(|m| m.id);
        time("review: Replace one, no save", &mut || {
            let mention = mentions.next().unwrap();
            session
                .review(
                    ReviewAction::Replace {
                        mention,
                        scope: Scope::This,
                    },
                    None,
                )
                .unwrap();
        });
        time("review: Replace one, saved", &mut || {
            let mention = mentions.next().unwrap();
            session
                .review(
                    ReviewAction::Replace {
                        mention,
                        scope: Scope::This,
                    },
                    Some(library.clone()),
                )
                .unwrap();
        });
        time("review: Undo, saved", &mut || {
            session
                .review(ReviewAction::Undo, Some(library.clone()))
                .unwrap();
        });
        // Replace on a replaced mention changes nothing: no save.
        let replaced = start.mentions[0].id;
        time("review: nothing changed, library given", &mut || {
            let action = ReviewAction::Replace {
                mention: replaced,
                scope: Scope::This,
            };
            session.review(action, Some(library.clone())).unwrap();
        });
    }
}
