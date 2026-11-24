//! Every way text leaves mdoc for an AI, on desktop, tested against the
//! readiness check (#1091 Decision 19, `Vault::text_for_ai`; Vault &
//! Projects item 29). The cut-over's editor Copy for AI skipped that check
//! (#1150), so this suite lists every exit in [`EXITS`] and runs each
//! against a document that is not Ready.
//!
//! Each scenario starts from a document the check finds Ready, open in a
//! tab or only stored, with the shelf listing it as ready. The scenario
//! then makes it not Ready, and every exit is asked to hand it out. An
//! exit must refuse, and nothing it hands out may hold a name the Project
//! knows as written. A Ready control proves that each exit does hand
//! text out, so a broken exit cannot pass by handing out nothing.
//!
//! Taking the check out of any one exit fails the suite: the `taking_*`
//! tests take each gate's check out (`crate::exits`) and assert that its
//! exits then fail and every other exit still passes.
//!
//! To add an exit (Milestone 2's connector is next), add its row to
//! [`EXITS`] and its [`Via`]. The match in [`hand_out`] then needs its
//! driver, and if it has a check of its own, `crate::exits::Gate` needs its
//! gate.
//!
//! **Not exits.** These hand out text on purpose and are not checked:
//! - Copy Markdown (⋯ menu, File menu, shortcut, palette) is the
//!   deliberate raw copy of the editor's text.
//! - Export Markdown (⋯ menu, File menu, shortcut, palette) is the same
//!   raw copy, written to a file the user picks (Decision 6).
//! - By the maintainer's ruling on item 29, neither is offered under the
//!   Share tab's "Send to an AI", which offers only checked exits.
//! - Export Project is the recovery export (Decision 22): Markdown, notes
//!   and decrypted originals, real names by design. Notes never leave any
//!   other way.
//! - Pseudonymize Clipboard (Edit menu, palette, shelf) only replaces
//!   known names in text that was already on the clipboard. It hands out
//!   no document text.
//! - Restore Clipboard, the reply box's Copy reply and the Reply page's
//!   Copy restore an AI's reply for the user, marked as restored text. The
//!   reply box has no outbound path: Paste AI reply only comes in.
//! - Copying a selection (in the editor, a find field or a DOCX comment
//!   card), the model comparison's export, and the crash report (masked,
//!   with no document text) are the user's own copies.
use super::*;
use crate::{
    exits::{Gate, take_out},
    pii::ui::CopyForAi,
    shelf::{Readiness, ShelfKey},
    shelf_ui::ShelfView,
};
use mdoc_pii::{Category::Person, restore::ExportedIdentity};
use mdoc_store::document_id::VaultId;

/// What the document says: Anna Petrova is its name, Boris Ivanov only a
/// name once a scenario makes him one.
const TEXT: &str = "Anna Petrova wrote to Boris Ivanov about the lease.";

/// One way text leaves mdoc for an AI.
struct Exit {
    name: &'static str,
    /// Where its text is checked.
    gate: Gate,
    via: Via,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Via {
    /// The Share tab's Copy for AI button.
    ShareButton,
    /// The document header's ⋯ menu.
    MoreMenu,
    /// File › Copy for AI: the menu bar dispatches the action.
    FileMenu,
    /// ⌘⇧C / Ctrl+Shift+C.
    Shortcut,
    /// The command palette's Copy for AI.
    Palette,
    /// The menu bar shelf's Copy for AI on a row: its button's click, ⌘C
    /// and ⌘1…⌘8 all run `ShelfView::copy`.
    ShelfCopy(Placement),
    /// A drag from the shelf: its text and its `.md` file, name and all.
    ShelfDrag(Placement),
    // Milestone 2: the connector joins here.
}

/// Where the shelf's row comes from.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Placement {
    /// An open tab.
    Open,
    /// A Vault document no tab shows.
    Stored,
}

/// Every way text leaves mdoc for an AI, on desktop.
const EXITS: &[Exit] = &[
    Exit {
        name: "Share tab › Copy for AI",
        gate: Gate::EditorCopyForAi,
        via: Via::ShareButton,
    },
    Exit {
        name: "⋯ › Copy for AI",
        gate: Gate::EditorCopyForAi,
        via: Via::MoreMenu,
    },
    Exit {
        name: "File › Copy for AI",
        gate: Gate::EditorCopyForAi,
        via: Via::FileMenu,
    },
    Exit {
        name: "Copy for AI shortcut",
        gate: Gate::EditorCopyForAi,
        via: Via::Shortcut,
    },
    Exit {
        name: "Palette › Copy for AI",
        gate: Gate::EditorCopyForAi,
        via: Via::Palette,
    },
    Exit {
        name: "Shelf › Copy for AI, open document",
        gate: Gate::ShelfCopy,
        via: Via::ShelfCopy(Placement::Open),
    },
    Exit {
        name: "Shelf › Copy for AI, stored document",
        gate: Gate::ShelfCopy,
        via: Via::ShelfCopy(Placement::Stored),
    },
    Exit {
        name: "Shelf › drag, open document",
        gate: Gate::ShelfDrag,
        via: Via::ShelfDrag(Placement::Open),
    },
    Exit {
        name: "Shelf › drag, stored document",
        gate: Gate::ShelfDrag,
        via: Via::ShelfDrag(Placement::Stored),
    },
    // Milestone 2: the connector's row goes here.
];

/// How the document stands when its text is asked for.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Scenario {
    /// Ready: every exit hands out its checked text.
    Ready,
    /// Another document names Boris Ivanov and joins the Project through
    /// the window: this one still holds him as written.
    NameFromAnotherDocument,
    /// Anna Petrova, a name it replaced, is written out again: typed in its
    /// tab, or by another writer in the stored text.
    MappedNameAsWritten,
    /// Boris Ivanov joins the Project's names in the Vault from outside the
    /// window, which never hears of it: only the Vault's check knows.
    NameOnlyTheVaultKnows,
    /// Another writer adds a sentence naming no one the Project knows to
    /// the stored text: changed since its review, with no known name in it,
    /// so only the review's fingerprint catches it.
    ChangedElsewhere,
}

const NOT_READY: [Scenario; 4] = [
    Scenario::NameFromAnotherDocument,
    Scenario::MappedNameAsWritten,
    Scenario::NameOnlyTheVaultKnows,
    Scenario::ChangedElsewhere,
];

impl Scenario {
    /// The names its Project knows once it has happened: none of them may
    /// leave as written.
    fn known_names(self) -> &'static [&'static str] {
        match self {
            Scenario::Ready | Scenario::MappedNameAsWritten | Scenario::ChangedElsewhere => {
                &["Anna Petrova"]
            }
            Scenario::NameFromAnotherDocument | Scenario::NameOnlyTheVaultKnows => {
                &["Anna Petrova", "Boris Ivanov"]
            }
        }
    }
}

impl Via {
    fn placement(self) -> Placement {
        match self {
            Via::ShelfCopy(placement) | Via::ShelfDrag(placement) => placement,
            _ => Placement::Open,
        }
    }

    fn on_shelf(self) -> bool {
        matches!(self, Via::ShelfCopy(_) | Via::ShelfDrag(_))
    }
}

/// A window with the document, as an exit finds it.
struct Fixture<'a> {
    tabs: Entity<Workspace>,
    cx: &'a mut VisualTestContext,
    dir: tempfile::TempDir,
    /// Its tab, while it is open.
    letter: Option<u64>,
    document: VaultId,
    project: String,
    /// The shelf, opened while the row was ready, and the row's index.
    shelf: Option<(gpui::WindowHandle<ShelfView>, usize)>,
    /// The Share tab's click never reached Copy for AI.
    missed_click: bool,
}

/// Pending autosaves, and the Project writes they start, land.
fn written(cx: &mut VisualTestContext) {
    wait(cx, 800);
    wait(cx, 800);
}

fn open_shelf(cx: &mut VisualTestContext) -> gpui::WindowHandle<ShelfView> {
    use crate::shelf_ui::ShelfContent;
    let workspace = cx.window_handle().downcast::<Workspace>().unwrap();
    let shelf = cx.cx.update(|cx| {
        let content = workspace
            .read(cx)
            .map(|workspace| ShelfContent::of(workspace, cx))
            .unwrap();
        cx.open_window(gpui::WindowOptions::default(), |window, cx| {
            cx.new(|cx| ShelfView::new(workspace, content, window, cx))
        })
        .unwrap()
    });
    cx.run_until_parked();
    shelf
}

fn clipboard(cx: &mut VisualTestContext) -> String {
    cx.update(|_, cx| clipboard_of(cx))
}

fn clipboard_of(cx: &gpui::App) -> String {
    cx.read_from_clipboard()
        .and_then(|item| item.text())
        .unwrap_or_default()
}

/// The Ready document, open or stored as `via` needs it, with the shelf
/// listing it when `via` is the shelf's.
fn ready(cx: &mut TestAppContext, via: Via) -> Fixture<'_> {
    let dir = tempfile::tempdir().unwrap();
    let (tabs, cx) = boot(cx, Session::default());
    cx.simulate_resize(size(px(1400.), px(850.)));
    cx.update(|_, cx| crate::bind_app_keys(cx));
    cx.update(|_, cx| shell::bind_keys(cx));
    let letter = open_applied(
        &tabs,
        cx,
        dir.path(),
        "Anna Petrova lease.md",
        TEXT,
        &[("Anna Petrova", Person)],
    );
    let project = new_project(&tabs, cx, letter);
    written(cx);
    let document = cx.update(|_, cx| tabs.read(cx).tab_document(letter, cx).unwrap());
    assert!(vault(&tabs, cx).text_for_ai(document.as_str()).is_ok());
    let letter = match via.placement() {
        Placement::Open => Some(letter),
        Placement::Stored => {
            tabs.update_in(cx, |tabs, window, cx| tabs.close_tab(letter, window, cx));
            cx.run_until_parked();
            None
        }
    };
    let mut fixture = Fixture {
        tabs,
        cx,
        dir,
        letter,
        document,
        project,
        shelf: None,
        missed_click: false,
    };
    if via.on_shelf() {
        let key = fixture.key();
        let shelf = open_shelf(fixture.cx);
        let index = shelf
            .read_with(fixture.cx, |view, _| {
                let documents = &view.content().documents;
                let at = documents.iter().position(|row| row.key == key).unwrap();
                assert_eq!(documents[at].readiness, Readiness::Ready);
                at
            })
            .unwrap();
        fixture.shelf = Some((shelf, index));
    }
    fixture
}

impl Fixture<'_> {
    fn key(&self) -> ShelfKey {
        match self.letter {
            Some(tab) => ShelfKey::Tab(tab),
            None => ShelfKey::Document(self.document.clone()),
        }
    }

    /// Make the document not Ready as `scenario` says.
    fn change(&mut self, scenario: Scenario) {
        let tabs = self.tabs.clone();
        let cx = &mut *self.cx;
        match scenario {
            Scenario::Ready => {}
            Scenario::NameFromAnotherDocument => {
                let memo = open_applied(
                    &tabs,
                    cx,
                    self.dir.path(),
                    "memo.md",
                    "Boris Ivanov signed.",
                    &[("Boris Ivanov", Person)],
                );
                move_tab(&tabs, cx, memo, Some(&self.project));
                written(cx);
            }
            Scenario::MappedNameAsWritten => match self.letter {
                Some(letter) => {
                    tabs.update_in(cx, |tabs, window, cx| tabs.activate(letter, window, cx));
                    let view = active(&tabs, cx);
                    view.update_in(cx, |view, window, cx| {
                        let end = view.editor.read(cx).text().len();
                        view.editor
                            .update(cx, |editor, cx| editor.set_cursor(end, cx));
                        window.focus(&view.editor.read(cx).focus_handle(cx), cx);
                    });
                    cx.simulate_input(" Anna Petrova signed.");
                    cx.run_until_parked();
                }
                None => {
                    vault(&tabs, cx)
                        .update_document(self.document.as_str(), |document| {
                            document.markdown.push_str(" Anna Petrova signed.")
                        })
                        .unwrap();
                }
            },
            Scenario::ChangedElsewhere => {
                vault(&tabs, cx)
                    .update_document(self.document.as_str(), |document| {
                        document.markdown.push_str(" Clara Moss signed.")
                    })
                    .unwrap();
                let readiness = vault(&tabs, cx).readiness(self.document.as_str());
                assert_eq!(
                    readiness.unwrap(),
                    mdoc_vault::Readiness::ChangedSinceReview
                );
            }
            Scenario::NameOnlyTheVaultKnows => {
                vault(&tabs, cx)
                    .update_project(&self.project, |project| {
                        project.mapping.merge(&[ExportedIdentity {
                            alias: "PERSON_2".into(),
                            previous: vec![],
                            category: Person,
                            custom: false,
                            originals: vec!["Boris Ivanov".into()],
                        }])
                    })
                    .unwrap();
            }
        }
        // An open tab's change may still be on its way to the Vault.
        if scenario != Scenario::Ready && self.letter.is_none() {
            let readiness = vault(&tabs, cx).readiness(self.document.as_str());
            assert!(!readiness.unwrap().is_ready(), "{scenario:?}");
        }
    }

    /// Bring the document's tab forward with its editor focused.
    fn focus_letter(&mut self) {
        let letter = self.letter.expect("an editor exit needs the open tab");
        let tabs = self.tabs.clone();
        tabs.update_in(self.cx, |tabs, window, cx| {
            tabs.activate(letter, window, cx)
        });
        let view = active(&tabs, self.cx);
        self.cx
            .update(|window, cx| window.focus(&view.read(cx).editor.read(cx).focus_handle(cx), cx));
        draw(self.cx);
    }
}

/// Ask `via` for the document's text: everything it handed out.
const BEFORE: &str = "the clipboard before";

fn hand_out(via: Via, fixture: &mut Fixture<'_>) -> Vec<String> {
    fixture
        .cx
        .write_to_clipboard(gpui::ClipboardItem::new_string(BEFORE.into()));
    let mut out = Vec::new();
    match via {
        Via::ShareButton => {
            fixture.focus_letter();
            let view = active(&fixture.tabs, fixture.cx);
            view.update_in(fixture.cx, |view, window, cx| {
                if !view.replacements_open() {
                    view.toggle_replacements_panel(window, cx);
                }
                view.inspector = crate::document_view::Inspector::Share;
                view.error = None;
                cx.notify();
            });
            click_toolbar(fixture.cx, "share-copy-for-ai");
            written(fixture.cx);
            let reached = fixture.cx.update(|_, cx| {
                let refused =
                    (view.read(cx).error.as_deref()).is_some_and(|e| e.starts_with("Not copied"));
                refused || clipboard_of(cx) != BEFORE
            });
            if !reached {
                // While a proposal is pending, GPUI's test window drops this
                // click (a GUI row checks it in the app). The button's
                // handler is the one the click runs; a Ready document's
                // click must land (`faults`).
                fixture.missed_click = true;
                view.update_in(fixture.cx, |view, window, cx| {
                    view.copy_for_ai(&CopyForAi, window, cx)
                });
            }
        }
        Via::MoreMenu => {
            fixture.focus_letter();
            more(fixture.cx, "more-copy-for-ai");
        }
        Via::FileMenu => {
            fixture.focus_letter();
            fixture.cx.dispatch_action(CopyForAi);
        }
        Via::Shortcut => {
            fixture.focus_letter();
            fixture.cx.simulate_keystrokes("secondary-shift-c");
        }
        Via::Palette => {
            fixture.focus_letter();
            fixture.cx.simulate_keystrokes("secondary-k");
            fixture.cx.simulate_input("Copy for AI");
            draw(fixture.cx);
            let first = fixture
                .cx
                .update(|_, cx| fixture.tabs.read(cx).palette_labels(cx));
            assert_eq!(first.first().map(String::as_str), Some("Copy for AI"));
            fixture.cx.simulate_keystrokes("enter");
        }
        Via::ShelfCopy(_) => {
            let (shelf, index) = fixture.shelf.expect("the shelf is open");
            shelf
                .update(fixture.cx, |view, window, cx| view.copy(index, window, cx))
                .unwrap();
        }
        Via::ShelfDrag(_) => {
            let (shelf, index) = fixture.shelf.expect("the shelf is open");
            let dragged = shelf
                .update(fixture.cx, |view, window, cx| {
                    view.drag(index, window, cx);
                    view.dragged.take()
                })
                .unwrap();
            if let Some((text, file)) = dragged {
                out.push(text);
                out.push(file.file_name().unwrap().to_string_lossy().into_owned());
                out.push(std::fs::read_to_string(&file).unwrap());
            }
        }
    }
    // A Copy for AI waiting for its writes copies once they land.
    written(fixture.cx);
    let copied = clipboard(fixture.cx);
    if copied != BEFORE {
        out.push(copied);
    }
    out
}

/// What is wrong with what `exit` handed out in `scenario`; empty when
/// nothing is.
fn faults(exit: &Exit, scenario: Scenario, out: &[String]) -> Vec<String> {
    let mut faults = Vec::new();
    match scenario {
        Scenario::Ready if !out.iter().any(|text| text.contains("PERSON_1")) => {
            faults.push(format!("{}: handed out nothing Ready: {out:?}", exit.name));
        }
        Scenario::Ready => {}
        _ if !out.is_empty() => {
            faults.push(format!(
                "{}: handed out a document not Ready ({scenario:?}): {out:?}",
                exit.name
            ));
        }
        _ => {}
    }
    for text in out {
        for original in scenario.known_names() {
            if text.contains(original) {
                faults.push(format!(
                    "{}: {original} left as written ({scenario:?}) in {text:?}",
                    exit.name
                ));
            }
        }
    }
    faults
}

/// Run `exit` in `scenario` in a window of its own: its faults.
fn run(cx: &mut TestAppContext, exit: &Exit, scenario: Scenario) -> Vec<String> {
    let mut fixture = ready(cx, exit.via);
    fixture.change(scenario);
    let out = hand_out(exit.via, &mut fixture);
    let mut faults = faults(exit, scenario, &out);
    if fixture.missed_click && scenario == Scenario::Ready {
        faults.push(format!(
            "{}: the click never reached Copy for AI",
            exit.name
        ));
    }
    faults
}

/// Every exit in `scenarios`: the faults found.
fn suite(cx: &mut TestAppContext, scenarios: &[Scenario]) -> Vec<String> {
    let mut found = Vec::new();
    for scenario in scenarios {
        for exit in EXITS {
            found.extend(run(cx, exit, *scenario));
        }
    }
    found
}

/// Done when: each exit hands out its checked text for a Ready document,
/// with no original in it. The control for the scenarios below.
#[gpui::test]
fn every_exit_hands_out_a_ready_document_pseudonymized(cx: &mut TestAppContext) {
    let faults = suite(cx, &[Scenario::Ready]);
    assert!(faults.is_empty(), "{faults:#?}");
}

/// Done when: no exit hands out a document holding a name another
/// document of its Project added.
#[gpui::test]
fn no_exit_hands_out_a_name_another_document_added(cx: &mut TestAppContext) {
    let faults = suite(cx, &[Scenario::NameFromAnotherDocument]);
    assert!(faults.is_empty(), "{faults:#?}");
}

/// Done when: no exit hands out a document with a mapped name written out
/// again.
#[gpui::test]
fn no_exit_hands_out_a_mapped_name_as_written(cx: &mut TestAppContext) {
    let faults = suite(cx, &[Scenario::MappedNameAsWritten]);
    assert!(faults.is_empty(), "{faults:#?}");
}

/// Done when: no exit hands out a document changed since its review, even
/// with no known name in the change.
#[gpui::test]
fn no_exit_hands_out_a_document_changed_since_review(cx: &mut TestAppContext) {
    let faults = suite(cx, &[Scenario::ChangedElsewhere]);
    assert!(faults.is_empty(), "{faults:#?}");
}

/// Done when: no exit hands out a document with a name only the Vault's
/// check knows of, the case Decision 19 checks every outbound read for.
#[gpui::test]
fn no_exit_hands_out_a_name_only_the_vault_knows(cx: &mut TestAppContext) {
    let faults = suite(cx, &[Scenario::NameOnlyTheVaultKnows]);
    assert!(faults.is_empty(), "{faults:#?}");
}

/// Take `gate`'s check out: every exit behind it fails in every scenario
/// that is not Ready, and every other exit still passes.
fn taken_out_fails_only(cx: &mut TestAppContext, gate: Gate) {
    let _taken = take_out(gate);
    for exit in EXITS {
        // The others are run once, in the scenario only the Vault's check
        // catches.
        let scenarios: &[Scenario] = if exit.gate == gate {
            &NOT_READY
        } else {
            &[Scenario::NameOnlyTheVaultKnows]
        };
        for &scenario in scenarios {
            let faults = run(cx, exit, scenario);
            if exit.gate == gate {
                assert!(
                    !faults.is_empty(),
                    "{} passed with {gate:?}'s check taken out ({scenario:?})",
                    exit.name
                );
            } else {
                assert!(faults.is_empty(), "{gate:?} taken out: {faults:#?}");
            }
        }
    }
}

/// Done when: taking the check out of any one exit fails it, one test per
/// gate. Every gate has an exit, so a gate with none fails here.
#[test]
fn every_gate_has_an_exit() {
    for gate in Gate::ALL {
        assert!(EXITS.iter().any(|exit| exit.gate == gate), "{gate:?}");
    }
}

#[gpui::test]
fn taking_the_editors_check_out_fails_its_exits(cx: &mut TestAppContext) {
    taken_out_fails_only(cx, Gate::EditorCopyForAi);
}

#[gpui::test]
fn taking_the_shelf_copys_check_out_fails_it(cx: &mut TestAppContext) {
    taken_out_fails_only(cx, Gate::ShelfCopy);
}

#[gpui::test]
fn taking_the_shelf_drags_check_out_fails_it(cx: &mut TestAppContext) {
    taken_out_fails_only(cx, Gate::ShelfDrag);
}
