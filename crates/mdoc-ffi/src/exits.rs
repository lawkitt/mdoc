//! Every way text leaves mdoc for an AI from the mobile core, tested
//! against the readiness check (#1091 Decision 19; Vault & Projects item
//! 29), with a switch that takes one gate's check out.
//!
//! iOS and Android compose their Copy for AI and Share (as text or as a
//! `.md` file) from two calls. The refusal and file name come first:
//! `DocumentSession::hand_off` on iOS (`RealCore.copyForAI`, after
//! saving), or the app's own pending check and `Vault::share_file_name` on
//! Android (`Documents.copyForAi`); `hand_off` names the file with
//! `Vault::share_file_name` too, a neutral "Document 3 – 2026-11-25.md"
//! with nothing of the stored title (#1205). The text, which is also the
//! file's contents, always comes from `Vault::text_for_ai`. [`EXITS`]
//! mirrors each app's composition, and the suite runs both against a
//! document that is not Ready, and against Ready ones whose title holds a
//! name only another document gave its Project, or a real name nobody
//! reviewed.
//!
//! What each carries: the text (the clipboard, the share sheet's text, the
//! `.md` file's contents), the file's name, and the share sheet's subject
//! and preview title. Both apps take the subject from the file's name
//! without `.md`. **iOS Share as text: its title** was found unchecked by
//! item 29's review: it was the raw document title. Since #1188,
//! `DocumentViewModel.share(asFile: false)` sends the file's name, and the
//! XCTest `testShareCarriesNoOriginalInItsSubjectPreviewTitleOrFileName`
//! checks the subject, preview title and file name the sheet gets. The
//! mirror below sends the same.
//!
//! **Not exits:** `DocumentSession::copy_for_ai`, `share_text` and
//! `HandOff::text` are the session's unchecked text. The apps never send
//! them, and only the parity tests read them. By the maintainer's ruling on
//! item 29 they stay in the bindings, documented as unchecked.
//!
//! To add an exit (Milestone 2's connector), add its row to [`EXITS`] and,
//! if it has a check of its own, a [`Gate`].
use crate::{
    DocumentSession, ReviewError,
    document_session::{Candidate, SavedVaultDocument},
    test_support::Scratch,
    text::TextRange,
    vault::Vault,
};
use mdoc_pii::{Category, restore::ExportedIdentity};
use std::{cell::Cell, sync::Arc};

/// A place the core checks text before it leaves.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Gate {
    /// `Vault::text_for_ai`: the text every app exit sends.
    TextForAi,
    /// `DocumentSession::hand_off`'s refusal while proposals are pending.
    HandOff,
    /// `Vault::share_file_name`'s neutral name in place of the title: the
    /// file's name, and the subject both apps take from it.
    ShareFileName,
    // Milestone 2: the connector joins here, if it checks anywhere else.
}

thread_local! {
    static TAKEN_OUT: Cell<Option<Gate>> = const { Cell::new(None) };
}

/// Whether the running test took `gate`'s check out.
pub(crate) fn taken_out(gate: Gate) -> bool {
    TAKEN_OUT.with(|taken| taken.get() == Some(gate))
}

/// Take `gate`'s check out until the guard drops.
fn take_out(gate: Gate) -> TakenOut {
    TAKEN_OUT.with(|taken| taken.set(Some(gate)));
    TakenOut
}

struct TakenOut;

impl Drop for TakenOut {
    fn drop(&mut self) {
        TAKEN_OUT.with(|taken| taken.set(None));
    }
}

const KEY: [u8; 32] = [7; 32];
/// Anna Petrova is its name; Boris Ivanov only once a scenario makes him
/// one.
const TEXT: &str = "Anna Petrova wrote to Boris Ivanov about the lease.";

/// One way text leaves for an AI from the apps.
struct Exit {
    name: &'static str,
    gate: Gate,
    /// What it sends for the document, the text first and then the names
    /// it leaves under (file name, subject): `None` when refused.
    send: fn(&Letter) -> Option<Vec<String>>,
}

/// Every way text leaves for an AI through the mobile core.
const EXITS: &[Exit] = &[
    Exit {
        name: "iOS Copy for AI and Share: text, file, file name, Share as text's title",
        gate: Gate::TextForAi,
        send: ios,
    },
    Exit {
        name: "Android Copy for AI and Share: text, file, file name, title",
        gate: Gate::TextForAi,
        send: android,
    },
    // Milestone 2: the connector's row goes here.
];

/// `RealCore.copyForAI`: saved first, then the session's refusal and file
/// name, then the Vault's text; Share as text's subject is the file's name
/// without `.md` (#1188).
fn ios(letter: &Letter) -> Option<Vec<String>> {
    letter
        .session
        .save_pending_into_vault(letter.vault.clone())
        .ok()?;
    let hand_off = letter.session.hand_off(letter.vault.clone()).ok()?;
    let text = letter.vault.text_for_ai(letter.id.clone()).ok()?;
    let subject = subject(&hand_off.file_name);
    Some(vec![text, hand_off.file_name, subject])
}

/// The share sheet's subject and preview title: the file's name without
/// `.md`.
fn subject(file_name: &str) -> String {
    file_name
        .strip_suffix(".md")
        .unwrap_or(file_name)
        .to_owned()
}

/// `Documents.copyForAi`: its own pending check, the file name, then the
/// Vault's text.
fn android(letter: &Letter) -> Option<Vec<String>> {
    if letter.session.snapshot().ok()?.proposed > 0 {
        return None;
    }
    let file_name = letter.vault.share_file_name(letter.id.clone()).ok()?;
    let text = letter.vault.text_for_ai(letter.id.clone()).ok()?;
    // `shareIntent`'s EXTRA_TITLE and chooser title.
    let subject = subject(&file_name);
    Some(vec![text, file_name, subject])
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Scenario {
    /// Ready: each exit sends its checked text.
    Ready,
    /// Another document names Boris Ivanov and is saved into the Project.
    NameFromAnotherDocument,
    /// Anna Petrova is written out again in the saved text, by another
    /// writer.
    MappedNameAsWritten,
    /// Boris Ivanov joins the Project's names in the Vault, and no session
    /// hears of it.
    NameOnlyTheVaultKnows,
    /// Another writer adds a sentence naming no one the Project knows to
    /// the saved text: only the review's fingerprint catches it.
    ChangedElsewhere,
    /// Another document names Dmitri Orlov and is saved into the Project,
    /// and the letter is renamed "Dmitri Orlov lease": its text never
    /// mentions him, so it stays Ready, and nothing of its title may leave
    /// (#1192, #1205).
    ProjectNameInTheTitle,
    /// The letter is renamed "Clara Moss: Brightwater lease", a real name
    /// no review saw and the Project doesn't know: it stays Ready, and
    /// nothing of its title may leave (#1205).
    UnknownNameInTheTitle,
}

const READY: [Scenario; 3] = [
    Scenario::Ready,
    Scenario::ProjectNameInTheTitle,
    Scenario::UnknownNameInTheTitle,
];

/// The Ready scenarios that rename the letter.
const TITLED: [Scenario; 2] = [
    Scenario::ProjectNameInTheTitle,
    Scenario::UnknownNameInTheTitle,
];

const NOT_READY: [Scenario; 4] = [
    Scenario::NameFromAnotherDocument,
    Scenario::MappedNameAsWritten,
    Scenario::NameOnlyTheVaultKnows,
    Scenario::ChangedElsewhere,
];

impl Scenario {
    fn known_names(self) -> &'static [&'static str] {
        match self {
            Scenario::Ready
            | Scenario::MappedNameAsWritten
            | Scenario::ChangedElsewhere
            | Scenario::UnknownNameInTheTitle => &["Anna Petrova"],
            Scenario::NameFromAnotherDocument | Scenario::NameOnlyTheVaultKnows => {
                &["Anna Petrova", "Boris Ivanov"]
            }
            Scenario::ProjectNameInTheTitle => &["Anna Petrova", "Dmitri Orlov"],
        }
    }

    fn is_ready(self) -> bool {
        READY.contains(&self)
    }
}

/// The document, saved Ready into a Project, its session still open.
struct Letter {
    _scratch: Scratch,
    vault: Arc<Vault>,
    session: Arc<DocumentSession>,
    id: String,
    project: String,
}

fn at(text: &str, wording: &str) -> TextRange {
    let start = text.find(wording).unwrap() as u64;
    TextRange {
        start,
        end: start + wording.len() as u64,
    }
}

/// A session of `text` with `names` proposed, as a scan would.
fn proposed(text: &str, title: &str, names: &[&str]) -> Arc<DocumentSession> {
    let session = DocumentSession::new();
    session.import_text(text.into(), title.into()).unwrap();
    let candidates = (names.iter())
        .map(|wording| Candidate {
            range: at(text, wording),
            category: crate::document_session::Category::Person,
            score: 0.9,
        })
        .collect();
    session.add_candidates(candidates).unwrap();
    session
}

/// `text` reviewed, `names` replaced, saved into `project`.
fn saved(
    vault: &Arc<Vault>,
    project: &str,
    text: &str,
    title: &str,
    names: &[&str],
) -> (Arc<DocumentSession>, SavedVaultDocument) {
    let session = DocumentSession::new();
    session.import_text(text.into(), title.into()).unwrap();
    session.join_project(vault.clone(), project.into()).unwrap();
    let candidates = (names.iter())
        .map(|wording| Candidate {
            range: at(text, wording),
            category: crate::document_session::Category::Person,
            score: 0.9,
        })
        .collect();
    session.add_candidates(candidates).unwrap();
    session.replace_all().unwrap();
    let saved = session.save_into_vault(vault.clone()).unwrap();
    (session, saved)
}

/// An empty Vault in a scratch folder, which it lives as long as.
fn vault() -> (Scratch, Arc<Vault>) {
    let scratch = Scratch::new();
    let vault = Vault::open(
        scratch.path().join("vault").display().to_string(),
        KEY.to_vec(),
    )
    .unwrap();
    (scratch, vault)
}

fn ready() -> Letter {
    let (scratch, vault) = vault();
    let project = vault.create_project("Case".into()).unwrap().id;
    let (session, letter) = saved(
        &vault,
        &project,
        TEXT,
        "Anna Petrova lease",
        &["Anna Petrova"],
    );
    assert_eq!(
        vault.readiness(letter.id.clone()).unwrap(),
        crate::vault::Readiness::Ready
    );
    Letter {
        _scratch: scratch,
        vault,
        session,
        id: letter.id,
        project,
    }
}

impl Letter {
    fn change(&self, scenario: Scenario) {
        let store = self.vault.store();
        match scenario {
            Scenario::Ready => {}
            Scenario::NameFromAnotherDocument => {
                saved(
                    &self.vault,
                    &self.project,
                    "Boris Ivanov signed.",
                    "Memo",
                    &["Boris Ivanov"],
                );
            }
            Scenario::MappedNameAsWritten => {
                store
                    .update_document(&self.id, |document| {
                        document.markdown.push_str(" Anna Petrova signed.")
                    })
                    .unwrap();
            }
            Scenario::ChangedElsewhere => {
                store
                    .update_document(&self.id, |document| {
                        document.markdown.push_str(" Clara Moss signed.")
                    })
                    .unwrap();
                assert_eq!(
                    store.readiness(&self.id).unwrap(),
                    mdoc_vault::Readiness::ChangedSinceReview
                );
            }
            Scenario::NameOnlyTheVaultKnows => {
                store
                    .update_project(&self.project, |project| {
                        project.mapping.merge(&[ExportedIdentity {
                            alias: "PERSON_2".into(),
                            previous: vec![],
                            category: Category::Person,
                            custom: false,
                            originals: vec!["Boris Ivanov".into()],
                        }])
                    })
                    .unwrap();
            }
            Scenario::ProjectNameInTheTitle => {
                saved(
                    &self.vault,
                    &self.project,
                    "Dmitri Orlov signed.",
                    "Memo",
                    &["Dmitri Orlov"],
                );
                self.session
                    .rename_in_vault(self.vault.clone(), "Dmitri Orlov lease".into())
                    .unwrap();
                assert!(store.readiness(&self.id).unwrap().is_ready());
            }
            Scenario::UnknownNameInTheTitle => {
                self.session
                    .rename_in_vault(self.vault.clone(), "Clara Moss: Brightwater lease".into())
                    .unwrap();
                assert!(store.readiness(&self.id).unwrap().is_ready());
            }
        }
        if !scenario.is_ready() {
            assert!(
                !store.readiness(&self.id).unwrap().is_ready(),
                "{scenario:?}"
            );
        }
    }
}

/// What is wrong with what `exit` sent in `scenario` for the document
/// titled `title` as stored.
fn faults(exit: &Exit, scenario: Scenario, title: &str, sent: Option<&[String]>) -> Vec<String> {
    let mut faults = Vec::new();
    match (scenario, sent) {
        (_, Some(sent))
            if scenario.is_ready() && sent.iter().any(|text| text.contains("PERSON_1")) => {}
        (_, _) if scenario.is_ready() => {
            faults.push(format!(
                "{}: sent nothing Ready ({scenario:?}): {sent:?}",
                exit.name
            ));
        }
        (_, Some(sent)) => faults.push(format!(
            "{}: sent a document not Ready ({scenario:?}): {sent:?}",
            exit.name
        )),
        (_, None) => {}
    }
    for text in sent.unwrap_or_default() {
        for original in scenario.known_names() {
            if text.contains(original) {
                faults.push(format!(
                    "{}: {original} left as written ({scenario:?}) in {text:?}",
                    exit.name
                ));
            }
        }
    }
    // The names it leaves under hold no word of the stored title (#1205).
    let words: Vec<String> = (title.split(|c: char| !c.is_alphanumeric()))
        .filter(|word| word.chars().any(char::is_alphabetic))
        .map(str::to_lowercase)
        .collect();
    for name in sent.unwrap_or_default().iter().skip(1) {
        let lower = name.to_lowercase();
        for word in words.iter().filter(|word| lower.contains(word.as_str())) {
            faults.push(format!(
                "{}: {word:?} of the title {title:?} left ({scenario:?}) in {name:?}",
                exit.name
            ));
        }
    }
    faults
}

fn run(exit: &Exit, scenario: Scenario) -> Vec<String> {
    let letter = ready();
    letter.change(scenario);
    let title = letter.vault.store().document(&letter.id).unwrap().title;
    let sent = (exit.send)(&letter);
    faults(exit, scenario, &title, sent.as_deref())
}

fn suite(scenarios: &[Scenario]) -> Vec<String> {
    let mut found = Vec::new();
    for scenario in scenarios {
        for exit in EXITS {
            found.extend(run(exit, *scenario));
        }
    }
    found
}

/// Done when: each app exit sends the Vault's checked text for a Ready
/// document, with no original in it, and no word of its stored title in
/// its file name and subject: not a name only another document gave the
/// Project, not a real name nobody reviewed (#1205).
#[test]
fn every_exit_sends_a_ready_document_pseudonymized() {
    let faults = suite(&READY);
    assert!(faults.is_empty(), "{faults:#?}");
}

/// Done when: no app exit sends a document that is not Ready: a name
/// another document added, a mapped name as written, a name only the Vault
/// knows.
#[test]
fn no_exit_sends_a_document_that_is_not_ready() {
    let faults = suite(&NOT_READY);
    assert!(faults.is_empty(), "{faults:#?}");
}

/// Done when: `hand_off` refuses while proposals are pending, as iOS asks
/// it before the Vault's text; Android checks the same count itself.
#[test]
fn hand_off_refuses_while_proposals_are_pending() {
    let (_scratch, vault) = vault();
    let session = proposed(TEXT, "Lease", &["Anna Petrova"]);
    session.save_into_vault(vault.clone()).unwrap();
    assert!(matches!(
        session.hand_off(vault),
        Err(ReviewError::PendingProposals { count: 1 })
    ));
    assert_eq!(session.snapshot().unwrap().proposed, 1);
}

/// Done when: taking the check out of any one gate fails the suite: with
/// `Vault::text_for_ai`'s check out, every app exit sends a document that
/// is not Ready; with `Vault::share_file_name`'s neutral name out, every
/// app exit sends its stored title, a Project's name or an unknown real
/// name in it; with `hand_off`'s out, it hands off pending proposals.
#[test]
fn taking_a_gates_check_out_fails_the_suite() {
    {
        let _taken = take_out(Gate::TextForAi);
        for scenario in NOT_READY {
            for exit in EXITS.iter().filter(|exit| exit.gate == Gate::TextForAi) {
                assert!(
                    !run(exit, scenario).is_empty(),
                    "{} passed with the Vault's check taken out ({scenario:?})",
                    exit.name
                );
            }
        }
    }
    {
        let _taken = take_out(Gate::ShareFileName);
        for scenario in TITLED {
            for exit in EXITS {
                assert!(
                    !run(exit, scenario).is_empty(),
                    "{} passed with the neutral name taken out ({scenario:?})",
                    exit.name
                );
            }
        }
    }
    let _taken = take_out(Gate::HandOff);
    let (_scratch, vault) = vault();
    let session = proposed(TEXT, "Lease", &["Anna Petrova"]);
    session.save_into_vault(vault.clone()).unwrap();
    let handed = session.hand_off(vault);
    assert!(
        handed.is_ok_and(|hand_off| hand_off.text.contains("Anna Petrova")),
        "hand_off refused with its check taken out"
    );
}
