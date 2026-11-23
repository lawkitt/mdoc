//! What a tap on the review screen does, in one core call (refactor
//! grill Q12): `DocumentSession::review` takes the action by mention id
//! and returns the snapshot after it (`save_pending_into_vault` saves it),
//! so the app neither looks the mention up in a snapshot first nor
//! takes a second one. Its refusals are typed (`ReviewError`). Beside it:
//! Copy for AI's hand-off, which refuses while proposals are pending, and
//! what the session holds, read without a snapshot.
use super::{Document, DocumentSession, no_document, review::Scope, span};
use crate::{
    CoreError, ReviewError,
    document_session::{Category, DocumentPhase, ReviewSnapshot},
    text::TextRange,
};

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
    /// Not a name: the mark, or every mark of its identity,
    /// comes off with no trace; replaced text returns first.
    NotAName {
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
    /// Fix the words: the mention takes `words`, a range of its
    /// draft (`DocumentSession::fix_words_draft`, where a replaced mention
    /// shows its original); the new wording replaces the old one
    /// everywhere it fits, keeping the identity and its alias, or joins the
    /// identity whose wording it is already. One undo step.
    FixWords {
        mention: u64,
        words: TextRange,
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
    pub case_id: Option<String>,
    pub scanned: bool,
    pub phase: DocumentPhase,
    pub proposed: u64,
    /// How it began: a new document follows its first line, the sample
    /// restores.
    pub origin: super::DocumentOrigin,
}

/// What Hide name's callout or Fix the words' Hide «…» would hide: the
/// selection snapped to whole words and trimmed.
#[derive(Clone, Debug, PartialEq, uniffi::Record)]
pub struct WordsTarget {
    pub range: TextRange,
    pub wording: String,
    /// Hide as ›: a proposed wording's own category alone, else the core's
    /// guess first, then the others in picker order. Empty for Fix the
    /// words, which keeps the name's identity.
    pub categories: Vec<Category>,
}

/// What Fix the words shows while a name's words are fixed.
#[derive(Clone, Debug, PartialEq, uniffi::Record)]
pub struct FixWordsDraft {
    /// The review as it shows meanwhile: a replaced name has its original
    /// back in place of its alias, as a proposal; ranges index its text.
    pub snapshot: ReviewSnapshot,
    /// The name's words in `snapshot.text`: the handles start here.
    pub wording: TextRange,
    /// How far the handles reach: the paragraph or cell, up to Markdown
    /// formatting or another name's alias.
    pub bounds: TextRange,
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
    /// Apply `action`; the snapshot after. The change waits for
    /// `save_pending_into_vault`, which saves every action since the last
    /// save at once (the save follows the tap).
    pub fn review(&self, action: ReviewAction) -> Result<ReviewSnapshot, ReviewError> {
        let mut guard = self.lock_document();
        let document = guard.as_mut().ok_or_else(no_document)?;
        document.apply(action)?;
        Ok(document.snapshot())
    }

    /// Copy for AI and Share: refused while proposals are pending.
    pub fn hand_off(&self) -> Result<HandOff, ReviewError> {
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
        Ok(hand_off)
    }

    /// Hide name's callout for a selection outside any name: what it
    /// hides and the categories Hide as offers. Refused with the reason
    /// (nothing to hide, a cut through a name, Markdown formatting).
    pub fn hide_target(&self, range: TextRange) -> Result<WordsTarget, ReviewError> {
        let guard = self.lock_document();
        let document = guard.as_ref().ok_or_else(no_document)?;
        let (range, categories) = document.review.hide_target(span(range))?;
        Ok(WordsTarget {
            wording: document.review.text[range.clone()].to_owned(),
            range: TextRange::of(&range),
            categories: categories.into_iter().map(Into::into).collect(),
        })
    }

    /// Fix the words begins on `mention`: the review as it shows while its
    /// words are fixed, where the handles start and how far they reach.
    pub fn fix_words_draft(&self, mention: u64) -> Result<FixWordsDraft, ReviewError> {
        let mut guard = self.lock_document();
        let document = guard.as_mut().ok_or_else(no_document)?;
        let range = document.mention(mention)?;
        let draft = document.review.draft(&range)?;
        let bounds = super::review::ReviewText::words_bounds(&draft);
        let snapshot = super::snapshot::draft(document.snapshot(), &range, &draft);
        Ok(FixWordsDraft {
            snapshot,
            wording: TextRange::of(&draft.wording),
            bounds: TextRange::of(&bounds),
        })
    }

    /// Fix the words' Hide «…» for `words` of `mention`'s draft: the words
    /// it would take, or why it can't.
    pub fn fix_words_target(
        &self,
        mention: u64,
        words: TextRange,
    ) -> Result<WordsTarget, ReviewError> {
        let mut guard = self.lock_document();
        let document = guard.as_mut().ok_or_else(no_document)?;
        let range = document.mention(mention)?;
        let (draft, target) = document.review.words_target(&range, span(words))?;
        Ok(WordsTarget {
            wording: draft.text[target.clone()].to_owned(),
            range: TextRange::of(&target),
            categories: Vec::new(),
        })
    }

    /// The title, Project and phase; `None` before a document is read.
    pub fn info(&self) -> Option<SessionInfo> {
        let guard = self.lock_document();
        let document = guard.as_ref()?;
        let proposed = document.proposed();
        Some(SessionInfo {
            title: document.title.clone(),
            case_id: document.project().map(str::to_owned),
            scanned: document.scanned,
            phase: DocumentPhase::of(document.scanned, proposed),
            proposed,
            origin: document.origin,
        })
    }
}

impl Document {
    /// Pending proposals, as the snapshot counts them.
    fn proposed(&self) -> u64 {
        self.review.counts().proposed
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
            A::NotAName { mention, scope } => {
                let range = self.mention(mention)?;
                self.review.not_a_name(range, scope)?;
            }
            A::ReplaceAll => self.review.replace_all()?,
            A::Mark { range, category } => {
                self.review
                    .add_selection(span(range), category.map(Into::into))?;
            }
            A::FixWords { mention, words } => {
                let range = self.mention(mention)?;
                self.review.fix_words(range, span(words))?;
            }
            A::ReplaceSelection { range, category } => {
                let selection = self
                    .review
                    .add_selection(span(range), Some(category.into()))?;
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
    use crate::{document_session::MentionState, test_support::Scratch, vault::Vault};
    use std::sync::Arc;

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
        let snapshot = session.review(names(&people)).unwrap();
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
    /// and Redo.
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
            .review(ReviewAction::Replace {
                mention: ann,
                scope: Scope::This,
            })
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
            .review(ReviewAction::Keep {
                mention: bob,
                scope: Scope::All,
            })
            .unwrap();
        assert_eq!(id(&kept, "Bob Hart", 0), bob);
        let all = session.review(ReviewAction::ReplaceAll).unwrap();
        assert_eq!(all.text, "PERSON_1 met Bob Hart. PERSON_1 left.");
        assert_eq!(id(&all, "Ann Lee", 1), again);
        let undone = session.review(ReviewAction::Undo).unwrap();
        assert_eq!(ids(&undone), ids(&kept));
        let undone = session.review(ReviewAction::Undo).unwrap();
        assert_eq!(ids(&undone), ids(&replaced));
        session.review(ReviewAction::Redo).unwrap();
        assert_eq!(ids(&session.review(ReviewAction::Redo).unwrap()), ids(&all));
        assert!(matches!(
            session.review(ReviewAction::Keep {
                mention: 999,
                scope: Scope::This
            }),
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
            .review(ReviewAction::Replace {
                mention: ann,
                scope: Scope::All,
            })
            .unwrap();
        let bob = id(&start, "Bob Hart", 0);
        let snapshot = session
            .review(ReviewAction::Keep {
                mention: bob,
                scope: Scope::All,
            })
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
            .review(names(&[("Ann Lee", Category::Person)]))
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
        match session.review(rename(" PERSON_2 ")) {
            Err(ReviewError::AliasTaken { alias, identity }) => {
                assert_eq!((alias.as_str(), identity), ("PERSON_2", bob));
            }
            other => panic!("{other:?}"),
        }
        assert!(matches!(
            session.review(rename("not an alias")),
            Err(ReviewError::Refused { .. })
        ));
        let renamed = session.review(rename("CLIENT")).unwrap();
        assert!(renamed.identities.iter().any(|i| i.alias == "CLIENT"));
        // Its own alias again is no refusal.
        session.review(rename("CLIENT")).unwrap();
    }

    /// Fix the words on a replaced name: the draft shows its original as a
    /// proposal, the handles' reach stops at the line's end, and the action
    /// keeps it replaced under its alias, as one step the toast names.
    #[test]
    fn fix_the_words_runs_from_the_draft() {
        let text = "Mr Ann Lee signed.\nMr Ann Lee paid.";
        let (session, start) = session(text, &["Mr Ann Lee"]);
        let replaced = session.review(ReviewAction::ReplaceAll).unwrap();
        assert_eq!(replaced.text, "PERSON_1 signed.\nPERSON_1 paid.");
        let mention = id(&replaced, "Mr Ann Lee", 0);
        assert_eq!(mention, id(&start, "Mr Ann Lee", 0));
        let draft = session.fix_words_draft(mention).unwrap();
        assert_eq!(draft.snapshot.text, "Mr Ann Lee signed.\nPERSON_1 paid.");
        assert_ne!(draft.snapshot.text_revision, replaced.text_revision);
        let shown = &draft.snapshot.mentions;
        assert_eq!(
            (shown[0].range, shown[0].state),
            (draft.wording, MentionState::Proposed)
        );
        assert_eq!(shown[1].range, TextRange { start: 19, end: 27 });
        assert_eq!(draft.bounds, TextRange { start: 0, end: 18 });
        // Hide «…» names the words the handles hold, trimmed.
        let words = TextRange { start: 2, end: 17 };
        let target = session.fix_words_target(mention, words).unwrap();
        assert_eq!(target.wording, "Ann Lee signed");
        let words = TextRange { start: 3, end: 10 };
        let fixed = session
            .review(ReviewAction::FixWords { mention, words })
            .unwrap();
        assert_eq!(fixed.text, "Mr PERSON_1 signed.\nMr PERSON_1 paid.");
        let step = fixed.undo_step.unwrap();
        assert_eq!(step.label, "Fix the words · Ann Lee");
        assert_eq!(step.range, Some(TextRange { start: 3, end: 11 }));
        assert_eq!(fixed.identities.len(), 1);
        assert_eq!(fixed.identities[0].originals, ["Ann Lee"]);
        let undone = session.review(ReviewAction::Undo).unwrap();
        assert_eq!(undone.text, replaced.text);
    }

    /// Copy for AI refuses while proposals are pending; then it is the
    /// text and file name the separate calls give.
    #[test]
    fn hand_off_waits_for_every_proposal() {
        let (session, _) = session("Ann Lee met Bob Hart.", &["Ann Lee", "Bob Hart"]);
        assert!(matches!(
            session.hand_off(),
            Err(ReviewError::PendingProposals { count: 2 })
        ));
        session.review(ReviewAction::ReplaceAll).unwrap();
        let hand_off = session.hand_off().unwrap();
        assert_eq!(hand_off.text, session.copy_for_ai().unwrap());
        assert_eq!(hand_off.file_name, "Letter.md");
    }

    /// What a review tap costs on a long document (async calls
    /// only past about 16 ms on an iPhone 12 Pro): about 20 pages, 200
    /// names, 1,600 mentions. `cargo test --release -p mdoc-ffi --lib
    /// measure_a_review_tap -- --ignored --nocapture`.
    #[test]
    #[ignore = "a measurement; run it in release"]
    fn measure_a_review_tap() {
        use std::time::Instant;
        let scratch = Scratch::new();
        let vault = Vault::open(scratch.path().display().to_string(), vec![3; 32]).unwrap();
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
            .review(ReviewAction::ProposeNames { names: people })
            .unwrap();
        session.save_into_vault(vault.clone()).unwrap();
        let save = || {
            session.save_pending_into_vault(vault.clone()).unwrap();
        };
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
                .review(ReviewAction::Replace {
                    mention,
                    scope: Scope::This,
                })
                .unwrap();
        });
        time(
            "review: Replace one, then save_pending_into_vault",
            &mut || {
                let mention = mentions.next().unwrap();
                session
                    .review(ReviewAction::Replace {
                        mention,
                        scope: Scope::This,
                    })
                    .unwrap();
                save();
            },
        );
        time("review: Undo, then save_pending_into_vault", &mut || {
            session.review(ReviewAction::Undo).unwrap();
            save();
        });
        // Replace on a replaced mention changes nothing: no save.
        let replaced = start.mentions[0].id;
        time(
            "review: nothing changed, then save_pending_into_vault",
            &mut || {
                let action = ReviewAction::Replace {
                    mention: replaced,
                    scope: Scope::This,
                };
                session.review(action).unwrap();
                save();
            },
        );
    }

    /// Ids stay through the identity tools too: Rename, Category, New
    /// alias and Same as rewrite aliases in the text; a mark adds a mention
    /// with an id of its own.
    #[test]
    fn mention_ids_survive_the_identity_tools() {
        let text = "Ann Lee met Ann Lee and Bob Hart. Mia paid.";
        let (session, _) = session(text, &["Ann Lee", "Bob Hart"]);
        let start = session.review(ReviewAction::ReplaceAll).unwrap();
        let column = |snapshot: &ReviewSnapshot| -> Vec<u64> {
            snapshot.mentions.iter().map(|m| m.id).collect()
        };
        let before = column(&start);
        let (ann, again, bob) = (before[0], before[1], before[2]);
        let bob_identity = start.mentions[2].identity.unwrap();
        let steps = [
            ReviewAction::Rename {
                mention: ann,
                scope: Scope::All,
                alias: "CLIENT".into(),
            },
            ReviewAction::SetCategory {
                mention: bob,
                scope: Scope::All,
                category: Category::Organization,
            },
            ReviewAction::NewAlias { mention: again },
            ReviewAction::SameAs {
                mention: again,
                scope: Scope::This,
                target: bob_identity,
            },
        ];
        let mut texts = vec![start.text.clone()];
        for step in steps {
            let after = session.review(step.clone()).unwrap();
            assert_eq!(column(&after), before, "{step:?}");
            texts.push(after.text);
        }
        texts.dedup();
        assert_eq!(texts.len(), 5, "each tool rewrote the text: {texts:?}");
        let mia = texts[4].find("Mia").unwrap() as u64;
        let marked = session
            .review(ReviewAction::Mark {
                range: TextRange {
                    start: mia,
                    end: mia + 3,
                },
                category: Some(Category::Person),
            })
            .unwrap();
        let ids = column(&marked);
        assert_eq!(ids[..3], before[..]);
        assert!(!before.contains(&ids[3]));
    }

    /// `info` reads the session without a snapshot: the title, the
    /// Project, the proposals left and the phase.
    #[test]
    fn info_reads_the_session_without_a_snapshot() {
        let scratch = Scratch::new();
        let vault = Vault::open(scratch.path().display().to_string(), vec![3; 32]).unwrap();
        let project = vault.create_project("Case".into()).unwrap();
        assert!(DocumentSession::new().info().is_none());
        let (session, start) = session("Ann Lee met Bob Hart.", &["Ann Lee", "Bob Hart"]);
        session
            .join_project(vault.clone(), project.id.clone())
            .unwrap();
        let replaced = session
            .review(ReviewAction::Replace {
                mention: id(&start, "Ann Lee", 0),
                scope: Scope::This,
            })
            .unwrap();
        let info = session.info().unwrap();
        assert_eq!(info.case_id, replaced.case_id);
        assert_eq!(info.case_id, Some(project.id));
        assert_eq!((info.title.as_str(), info.proposed), ("Letter", 1));
        assert_eq!(info.phase, DocumentPhase::Reviewing);
    }

    /// Scanned is kept with the document: a scan that found nothing reopens
    /// as scanned, and a document never scanned as unscanned whatever its
    /// mentions; a rename keeps it.
    #[test]
    fn scanned_is_kept_with_the_document() {
        let scratch = Scratch::new();
        let vault = Vault::open(scratch.path().display().to_string(), vec![3; 32]).unwrap();
        let reopened = |id: &str| {
            DocumentSession::new()
                .open_from_vault(vault.clone(), id.into())
                .unwrap()
        };
        let empty = DocumentSession::new();
        empty
            .import_text("Nothing here.".into(), "Empty".into())
            .unwrap();
        empty
            .review(names(&[("Ann Lee", Category::Person)]))
            .unwrap();
        let empty = empty.save_into_vault(vault.clone()).unwrap().id;
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
            .review(ReviewAction::ReplaceSelection {
                range,
                category: Category::Person,
            })
            .unwrap();
        let marked = marked.save_into_vault(vault.clone()).unwrap().id;
        let again = reopened(&marked);
        assert!(!again.scanned && !again.mentions.is_empty());
        vault
            .rename_document(empty.clone(), "Renamed".into())
            .unwrap();
        assert!(reopened(&empty).scanned);
    }
}
