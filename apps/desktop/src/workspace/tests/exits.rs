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
//! text out, so a broken exit cannot pass by handing out nothing. No file
//! an exit names holds a word of the document's title, which may hold a
//! real name nobody reviewed (#1208).
//!
//! Taking the check out of any one exit fails the suite: the `taking_*`
//! tests take each gate's check out (`crate::exits`) and assert that its
//! exits then fail and every other exit still passes.
//!
//! A refusal's step (#1209) is an exit too: "Replace N and copy" on the
//! Share tab, in the "Not copied" notice and on a shelf row replaces the
//! pending names and then asks Copy for AI. Such an exit `resolves`: in a
//! scenario its step fixes, it may hand out text, but only what the
//! Vault's check gives for the document then.
//!
//! **The companion window** (Milestone 2 item 14) hosts the shelf's rows:
//! its Copy for AI and drag are the shelf's, behind the same gates.
//!
//! To add an exit, add its row to [`EXITS`] and its [`Via`]. The match in
//! [`hand_out`] then needs its driver, and if it has a check of its own,
//! `crate::exits::Gate` needs its gate.
//!
//! **The AI connector** (Milestone 2) joined first, over the window's Vault
//! with the fixture's Project shared: `get_document`, and `list_documents`,
//! whose Ready label is the heading of the text that passed its check. For
//! the list the stored document gets a heading naming PERSON_1, so its label
//! carries document text; a row that needs review hands out nothing but its
//! "Document N".
//!
//! **Not exits.** These hand out text on purpose and are not checked:
//! - Copy Markdown (⋯ menu, File menu, shortcut, palette) is the
//!   deliberate raw copy of the editor's text, a hand copy marked unless
//!   clean.
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
//! - Restore Clipboard, the reply box's Copy reply, the Reply page's Copy
//!   and the companion's Copy of the latest reply restore an AI's reply
//!   for the user, marked as restored text. The reply box has no outbound
//!   path: Paste AI reply only comes in.
//! - Copying a selection (in the editor, a find field or a DOCX comment
//!   card), the model comparison's export, and the crash report (masked,
//!   with no document text) are the user's own copies. A hand copy of
//!   document text is never blocked; one not known to be clean is marked
//!   as restored text is, and "Replace N and Copy" replaces through the
//!   review (#1209, `hand_copies.rs`).
use super::*;
use crate::{
    ai::companion::CompanionView,
    exits::{Gate, take_out},
    review::CopyForAi,
    review::refusal::Step,
    shelf::shelf_ui::ShelfView,
    shelf::{Readiness, ShelfKey},
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
    /// It may make the document Ready first (a refusal's step).
    resolves: bool,
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
    /// and ⌘1…⌘8 all run `ShelfRows::copy`.
    ShelfCopy(Placement),
    /// A drag from the shelf: its text and its `.md` file, name and all.
    ShelfDrag(Placement),
    /// The companion window's shelf rows: Copy for AI on a row.
    CompanionCopy(Placement),
    /// A drag from the companion's shelf rows.
    CompanionDrag(Placement),
    /// Copy for AI refused in the editor, then the "Not copied" notice's
    /// step.
    NoticeStep,
    /// A shelf row's "Replace N and copy", run in its open tab.
    ShelfStep,
    /// The AI connector's `get_document` (Milestone 2), the Project shared.
    Connector(Placement),
    /// The AI connector's `list_documents`: the document's label, from a
    /// heading the stored document is given.
    ConnectorList,
    /// The shim's wire (`mdoc_connector::serve`): `get_document`'s answer
    /// line as it crosses the pipe, label and all, for the stored document
    /// with that heading.
    ConnectorWire,
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
        // Its button shows "Replace N and copy" while names are pending.
        resolves: true,
    },
    Exit {
        name: "⋯ › Copy for AI",
        gate: Gate::EditorCopyForAi,
        via: Via::MoreMenu,
        resolves: false,
    },
    Exit {
        name: "File › Copy for AI",
        gate: Gate::EditorCopyForAi,
        via: Via::FileMenu,
        resolves: false,
    },
    Exit {
        name: "Copy for AI shortcut",
        gate: Gate::EditorCopyForAi,
        via: Via::Shortcut,
        resolves: false,
    },
    Exit {
        name: "Palette › Copy for AI",
        gate: Gate::EditorCopyForAi,
        via: Via::Palette,
        resolves: false,
    },
    Exit {
        name: "Not copied notice › its step",
        gate: Gate::EditorCopyForAi,
        via: Via::NoticeStep,
        resolves: true,
    },
    Exit {
        name: "Shelf › Copy for AI, open document",
        gate: Gate::ShelfCopy,
        via: Via::ShelfCopy(Placement::Open),
        resolves: false,
    },
    Exit {
        name: "Shelf › Copy for AI, stored document",
        gate: Gate::ShelfCopy,
        via: Via::ShelfCopy(Placement::Stored),
        resolves: false,
    },
    Exit {
        name: "Shelf › Replace N and copy, open document",
        gate: Gate::EditorCopyForAi,
        via: Via::ShelfStep,
        resolves: true,
    },
    Exit {
        name: "Shelf › drag, open document",
        gate: Gate::ShelfDrag,
        via: Via::ShelfDrag(Placement::Open),
        resolves: false,
    },
    Exit {
        name: "Shelf › drag, stored document",
        gate: Gate::ShelfDrag,
        via: Via::ShelfDrag(Placement::Stored),
        resolves: false,
    },
    Exit {
        name: "Companion › Copy for AI, open document",
        gate: Gate::ShelfCopy,
        via: Via::CompanionCopy(Placement::Open),
        resolves: false,
    },
    Exit {
        name: "Companion › Copy for AI, stored document",
        gate: Gate::ShelfCopy,
        via: Via::CompanionCopy(Placement::Stored),
        resolves: false,
    },
    Exit {
        name: "Companion › drag, open document",
        gate: Gate::ShelfDrag,
        via: Via::CompanionDrag(Placement::Open),
        resolves: false,
    },
    Exit {
        name: "Companion › drag, stored document",
        gate: Gate::ShelfDrag,
        via: Via::CompanionDrag(Placement::Stored),
        resolves: false,
    },
    Exit {
        name: "Connector › get_document, open document",
        gate: Gate::Connector,
        via: Via::Connector(Placement::Open),
        resolves: false,
    },
    Exit {
        name: "Connector › get_document, stored document",
        gate: Gate::Connector,
        via: Via::Connector(Placement::Stored),
        resolves: false,
    },
    Exit {
        name: "Connector › list_documents, the document's label",
        gate: Gate::Connector,
        via: Via::ConnectorList,
        resolves: false,
    },
    Exit {
        name: "Connector › over the pipe, get_document's whole answer",
        gate: Gate::Connector,
        via: Via::ConnectorWire,
        resolves: false,
    },
];

/// How the document stands when its text is asked for.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Scenario {
    /// Ready: every exit hands out its checked text.
    Ready,
    /// Ready, and titled "Clara Moss, Brightwater lease": a real name no
    /// review saw and the Project doesn't know, so nothing masks it.
    UnknownNameInTheTitle,
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
    /// A new name, Dana Rowe, typed into its tab, found by the scan of the
    /// edited paragraph (Vault & Projects item 44); a stored document gets
    /// it from another writer.
    NewNameTyped,
}

const READY: [Scenario; 2] = [Scenario::Ready, Scenario::UnknownNameInTheTitle];

const NOT_READY: [Scenario; 5] = [
    Scenario::NameFromAnotherDocument,
    Scenario::MappedNameAsWritten,
    Scenario::NameOnlyTheVaultKnows,
    Scenario::ChangedElsewhere,
    Scenario::NewNameTyped,
];

/// The scenarios no step resolves: replacing what the tab proposes leaves
/// the document not Ready, so an exit that resolves must still refuse.
const UNRESOLVED: [Scenario; 2] = [Scenario::NameOnlyTheVaultKnows, Scenario::ChangedElsewhere];

impl Scenario {
    /// The names its Project knows once it has happened: none of them may
    /// leave as written.
    fn known_names(self) -> &'static [&'static str] {
        match self {
            Scenario::Ready
            | Scenario::UnknownNameInTheTitle
            | Scenario::MappedNameAsWritten
            | Scenario::ChangedElsewhere => &["Anna Petrova"],
            Scenario::NameFromAnotherDocument | Scenario::NameOnlyTheVaultKnows => {
                &["Anna Petrova", "Boris Ivanov"]
            }
            Scenario::NewNameTyped => &["Anna Petrova", "Dana Rowe"],
        }
    }

    /// The file the document is opened from, which titles it.
    fn file(self) -> &'static str {
        match self {
            Scenario::UnknownNameInTheTitle => "Clara Moss, Brightwater lease.md",
            _ => "Anna Petrova lease.md",
        }
    }
}

impl Via {
    fn placement(self) -> Placement {
        match self {
            Via::ShelfCopy(placement)
            | Via::ShelfDrag(placement)
            | Via::CompanionCopy(placement)
            | Via::CompanionDrag(placement)
            | Via::Connector(placement) => placement,
            Via::ConnectorList | Via::ConnectorWire => Placement::Stored,
            _ => Placement::Open,
        }
    }

    fn on_shelf(self) -> bool {
        matches!(self, Via::ShelfCopy(_) | Via::ShelfDrag(_) | Via::ShelfStep)
    }

    fn on_companion(self) -> bool {
        matches!(self, Via::CompanionCopy(_) | Via::CompanionDrag(_))
    }

    /// A drag that carries a `.md` file.
    fn drags(self) -> bool {
        matches!(self, Via::ShelfDrag(_) | Via::CompanionDrag(_))
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
    /// The companion, opened so, and the row's index in its shelf rows.
    companion: Option<(gpui::WindowHandle<CompanionView>, usize)>,
    /// The Share tab's click never reached Copy for AI.
    missed_click: bool,
}

/// Pending autosaves, and the Project writes they start, land.
fn written(cx: &mut VisualTestContext) {
    wait(cx, 800);
    wait(cx, 800);
}

fn clipboard(cx: &mut VisualTestContext) -> String {
    cx.update(|_, cx| clipboard_of(cx))
}

fn clipboard_of(cx: &gpui::App) -> String {
    cx.read_from_clipboard()
        .and_then(|item| item.text())
        .unwrap_or_default()
}

/// The Ready document, titled as `scenario` has it, open or stored as `via`
/// needs it, with the shelf listing it when `via` is the shelf's.
fn ready(cx: &mut TestAppContext, via: Via, scenario: Scenario) -> Fixture<'_> {
    let dir = tempfile::tempdir().unwrap();
    let (tabs, cx) = boot(cx, Session::default());
    cx.simulate_resize(size(px(1400.), px(850.)));
    cx.update(|_, cx| crate::bind_app_keys(cx));
    cx.update(|_, cx| crate::workspace::bind_keys(cx));
    let letter = open_applied(
        &tabs,
        cx,
        dir.path(),
        scenario.file(),
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
    if matches!(via, Via::ConnectorList | Via::ConnectorWire) {
        // A heading, reviewed with the rest, names the document to an AI.
        let store = vault(&tabs, cx);
        (store.update_document(document.as_str(), |document| {
            document.markdown = format!(
                "# {LIST_HEADING}

{}",
                document.markdown
            );
            document.set_ready(true);
        }))
        .unwrap();
        assert!(store.readiness(document.as_str()).unwrap().is_ready());
    }
    let mut fixture = Fixture {
        tabs,
        cx,
        dir,
        letter,
        document,
        project,
        shelf: None,
        companion: None,
        missed_click: false,
    };
    if via.on_shelf() {
        let key = fixture.key();
        let shelf = open_shelf(fixture.cx);
        let index = shelf
            .read_with(fixture.cx, |view, cx| {
                let documents = &view.content(cx).documents;
                let at = documents.iter().position(|row| row.key == key).unwrap();
                assert_eq!(documents[at].readiness, Readiness::Ready);
                at
            })
            .unwrap();
        fixture.shelf = Some((shelf, index));
    }
    if via.on_companion() {
        let key = fixture.key();
        let companion = open_companion(fixture.cx);
        let index = companion
            .read_with(fixture.cx, |view, cx| {
                let rows = view.shelf_rows().read(cx).documents().to_vec();
                let at = rows.iter().position(|row| row.key == key).unwrap();
                assert_eq!(rows[at].readiness, Readiness::Ready);
                at
            })
            .unwrap();
        fixture.companion = Some((companion, index));
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
            Scenario::Ready | Scenario::UnknownNameInTheTitle => {}
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
            Scenario::NewNameTyped => {
                *crate::review::TEST_SCAN.lock().unwrap() = Some(dana);
                append(&tabs, cx, self.letter, &self.document, " Dana Rowe signed.");
            }
            Scenario::MappedNameAsWritten => {
                append(
                    &tabs,
                    cx,
                    self.letter,
                    &self.document,
                    " Anna Petrova signed.",
                );
            }
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
        if !READY.contains(&scenario) && self.letter.is_none() {
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

    /// The document's titles: as stored, and as its tab shows it.
    fn titles(&mut self) -> Vec<String> {
        let tabs = self.tabs.clone();
        let stored = vault(&tabs, self.cx).document(self.document.as_str());
        let mut titles = vec![stored.unwrap().title];
        if let Some(letter) = self.letter {
            titles.push(self.cx.update(|_, cx| {
                let view = tabs.read(cx).tabs.get(letter).unwrap().view().unwrap();
                view.read(cx).display_name()
            }));
        }
        titles
    }
}

/// What an exit handed out: text, and the names of the files it carried.
#[derive(Debug, Default)]
struct Handed {
    texts: Vec<String>,
    file_names: Vec<String>,
}

/// Type `text` at the end of the open tab `letter`, or, stored, have
/// another writer append it.
fn append(
    tabs: &Entity<Workspace>,
    cx: &mut VisualTestContext,
    letter: Option<u64>,
    document: &VaultId,
    text: &str,
) {
    match letter {
        Some(letter) => {
            tabs.update_in(cx, |tabs, window, cx| tabs.activate(letter, window, cx));
            let view = active(tabs, cx);
            view.update_in(cx, |view, window, cx| {
                let end = view.editor.read(cx).text().len();
                view.editor
                    .update(cx, |editor, cx| editor.set_cursor(end, cx));
                window.focus(&view.editor.read(cx).focus_handle(cx), cx);
            });
            cx.simulate_input(text);
            cx.run_until_parked();
        }
        None => {
            vault(tabs, cx)
                .update_document(document.as_str(), |document| {
                    document.markdown.push_str(text)
                })
                .unwrap();
        }
    }
}

/// What the name finder finds in edited text: Dana Rowe.
fn dana(text: &str) -> Vec<mdoc_pii::Detection> {
    text.match_indices("Dana Rowe")
        .map(|(start, name)| mdoc_pii::Detection {
            range: start..start + name.len(),
            category: Person,
            score: 0.9,
            recognizer: mdoc_pii::Recognizer::Model,
        })
        .collect()
}

/// The heading [`Via::ConnectorList`]'s document is given: its label once
/// Ready.
const LIST_HEADING: &str = "PERSON_1's lease";

/// A connector over the window's Vault, sharing the fixture's Project.
fn connector(fixture: &mut Fixture<'_>) -> mdoc_connector::Connector {
    let setting = mdoc_store::settings::AiConnectorConfig {
        preview: true,
        ..Default::default()
    };
    let vault = vault(&fixture.tabs, fixture.cx);
    let connector = mdoc_connector::Connector::start(vault, &setting).unwrap();
    connector.share(&fixture.project).unwrap();
    connector
}

/// Ask `via` for the document's text: everything it handed out.
const BEFORE: &str = "the clipboard before";

fn hand_out(via: Via, fixture: &mut Fixture<'_>) -> Handed {
    fixture
        .cx
        .write_to_clipboard(gpui::ClipboardItem::new_string(BEFORE.into()));
    let mut out = Handed::default();
    match via {
        Via::ShareButton => {
            fixture.focus_letter();
            let view = active(&fixture.tabs, fixture.cx);
            view.update_in(fixture.cx, |view, window, cx| {
                if !view.replacements_open() {
                    view.toggle_replacements_panel(window, cx);
                }
                view.inspector = crate::document::Inspector::Share;
                view.copy_refusal = None;
                cx.notify();
            });
            click_toolbar(fixture.cx, "share-copy-for-ai");
            written(fixture.cx);
            let reached = fixture.cx.update(|_, cx| {
                let view = view.read(cx);
                // A step that opens the review hands nothing out.
                let reviewing = view.replacements_open()
                    && view.inspector == crate::document::Inspector::Review;
                view.copy_refusal.is_some() || reviewing || clipboard_of(cx) != BEFORE
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
        Via::NoticeStep => {
            fixture.focus_letter();
            fixture.cx.dispatch_action(CopyForAi);
            written(fixture.cx);
            let view = active(&fixture.tabs, fixture.cx);
            let step = view.read_with(fixture.cx, |view, _| {
                view.copy_refusal.as_ref().and_then(|refusal| refusal.step)
            });
            if step.is_some() {
                click_toolbar(fixture.cx, "copy-refusal-step");
            }
        }
        Via::ShelfStep => {
            // The row as the shelf lists it now, its "Replace N and copy"
            // run as its button runs it. It is driven in every scenario, so
            // the path to Copy for AI is asked even where the row offers
            // Review.
            let key = fixture.key();
            let shelf = open_shelf(fixture.cx);
            let index = shelf
                .read_with(fixture.cx, |view, cx| {
                    (view.content(cx).documents.iter()).position(|row| row.key == key)
                })
                .unwrap()
                .expect("the shelf lists it");
            let view = active(&fixture.tabs, fixture.cx);
            let pending = view.read_with(fixture.cx, |view, _| view.pii.review().remaining());
            shelf
                .update(fixture.cx, |view, window, cx| {
                    view.run_step(index, Step::ReplaceAndCopy(pending), window, cx)
                })
                .unwrap();
            fixture.cx.run_until_parked();
        }
        Via::ShelfDrag(_) => {
            let (shelf, index) = fixture.shelf.expect("the shelf is open");
            let dragged = shelf
                .update(fixture.cx, |view, window, cx| {
                    view.drag(index, window, cx);
                    view.rows().update(cx, |rows, _| rows.dragged.take())
                })
                .unwrap();
            if let Some((text, file)) = dragged {
                out.texts.push(text);
                let name = file.file_name().unwrap().to_string_lossy();
                out.file_names.push(name.into_owned());
                out.texts.push(std::fs::read_to_string(&file).unwrap());
            }
        }
        Via::CompanionCopy(_) => {
            let (companion, index) = fixture.companion.expect("the companion is open");
            companion
                .update(fixture.cx, |view, window, cx| {
                    view.shelf_rows()
                        .update(cx, |rows, cx| rows.copy(index, window, cx))
                })
                .unwrap();
        }
        Via::CompanionDrag(_) => {
            let (companion, index) = fixture.companion.expect("the companion is open");
            let dragged = companion
                .update(fixture.cx, |view, window, cx| {
                    view.shelf_rows().update(cx, |rows, cx| {
                        rows.drag(index, window, cx);
                        rows.dragged.take()
                    })
                })
                .unwrap();
            if let Some((text, file)) = dragged {
                out.texts.push(text);
                let name = file.file_name().unwrap().to_string_lossy();
                out.file_names.push(name.into_owned());
                out.texts.push(std::fs::read_to_string(&file).unwrap());
            }
        }
        Via::Connector(_) => {
            // The connector reads the Vault: what a tab typed lands first.
            written(fixture.cx);
            let connector = connector(fixture);
            match connector.get_document("Claude", fixture.document.as_str(), None) {
                Ok(mdoc_connector::Read::Text { text, .. }) => out.texts.push(text),
                Ok(mdoc_connector::Read::NeedsReview { .. }) => {}
                read => panic!("the fixture's document is short and shared: {read:?}"),
            }
        }
        Via::ConnectorWire => {
            use mdoc_mcp::wire::{Answer, Call, Done, Request, VERSION};
            let request = Request {
                v: VERSION,
                app: "claude-ai".into(),
                call: Call::GetDocument {
                    doc_id: fixture.document.as_str().into(),
                    part: None,
                },
            };
            let request = serde_json::to_vec(&request).unwrap();
            let line = mdoc_connector::serve::answer_line(&connector(fixture), &request);
            match serde_json::from_slice(&line).unwrap() {
                Answer::Ok(Done::Text { .. }) => out.texts.push(String::from_utf8(line).unwrap()),
                Answer::Ok(Done::NeedsReview { .. }) => {}
                answer => panic!("the fixture's document is short and shared: {answer:?}"),
            }
        }
        Via::ConnectorList => {
            let rows = connector(fixture).list_documents().unwrap();
            let row = rows
                .into_iter()
                .find(|row| row.doc_id == fixture.document.as_str());
            let row = row.expect("the document is listed");
            let fallback = (row.label.strip_prefix("Document "))
                .is_some_and(|n| n.bytes().all(|b| b.is_ascii_digit()));
            if row.state == mdoc_connector::State::Ready || !fallback {
                out.texts.push(row.label);
            }
        }
    }
    // A Copy for AI waiting for its writes copies once they land.
    written(fixture.cx);
    let copied = clipboard(fixture.cx);
    if copied != BEFORE {
        out.texts.push(copied);
    }
    out
}

/// What is wrong with what `exit` handed out in `scenario` for the
/// document titled `titles`; empty when nothing is. `now`: what the
/// Vault's check gives for the document once the exit has run.
fn faults(
    exit: &Exit,
    scenario: Scenario,
    titles: &[String],
    out: &Handed,
    now: Option<&str>,
) -> Vec<String> {
    let mut faults = Vec::new();
    let ready = READY.contains(&scenario);
    match scenario {
        _ if ready && !out.texts.iter().any(|text| text.contains("PERSON_1")) => {
            faults.push(format!("{}: handed out nothing Ready: {out:?}", exit.name));
        }
        _ if ready => {}
        // Its step made the document Ready: what it handed out is the
        // check's text now, with no file.
        _ if exit.resolves
            && !out.texts.is_empty()
            && out.file_names.is_empty()
            && out.texts.iter().all(|text| Some(text.as_str()) == now) => {}
        _ if !out.texts.is_empty() || !out.file_names.is_empty() => {
            faults.push(format!(
                "{}: handed out a document not Ready ({scenario:?}): {out:?}",
                exit.name
            ));
        }
        _ => {}
    }
    for text in out.texts.iter().chain(&out.file_names) {
        for original in scenario.known_names() {
            if text.contains(original) {
                faults.push(format!(
                    "{}: {original} left as written ({scenario:?}) in {text:?}",
                    exit.name
                ));
            }
        }
    }
    // A file's name holds no word of the title, masked or not (#1208).
    for name in &out.file_names {
        let lower = name.to_lowercase();
        for title in titles {
            let title = title.to_lowercase();
            let title = title.strip_suffix(".md").unwrap_or(&title);
            let words = (title.split(|c: char| !c.is_alphanumeric()))
                .filter(|word| word.chars().any(char::is_alphabetic));
            for word in words.filter(|word| lower.contains(word)) {
                faults.push(format!(
                    "{}: {word:?} of the title {title:?} left ({scenario:?}) in {name:?}",
                    exit.name
                ));
            }
        }
    }
    faults
}

/// Run `exit` in `scenario` in a window of its own: its faults.
fn run(cx: &mut TestAppContext, exit: &Exit, scenario: Scenario) -> Vec<String> {
    let mut fixture = ready(cx, exit.via, scenario);
    fixture.change(scenario);
    let titles = fixture.titles();
    let out = hand_out(exit.via, &mut fixture);
    let now = (vault(&fixture.tabs, fixture.cx))
        .text_for_ai(fixture.document.as_str())
        .ok();
    let mut faults = faults(exit, scenario, &titles, &out, now.as_deref());
    if fixture.missed_click && READY.contains(&scenario) {
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
/// with no original in it, and no word of its title in a file's name, not
/// even a real name nobody reviewed (#1208). The control for the scenarios
/// below.
#[gpui::test]
fn every_exit_hands_out_a_ready_document_pseudonymized(cx: &mut TestAppContext) {
    let faults = suite(cx, &READY);
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

/// Done when: no exit hands out a new name typed into a Ready document as
/// typed (Vault & Projects item 44).
#[gpui::test]
fn no_exit_hands_out_a_typed_new_name(cx: &mut TestAppContext) {
    let faults = suite(cx, &[Scenario::NewNameTyped]);
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
        let scenarios: &[Scenario] = if exit.gate == gate && exit.resolves {
            &UNRESOLVED
        } else if exit.gate == gate {
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

#[gpui::test]
fn taking_the_connectors_check_out_fails_it(cx: &mut TestAppContext) {
    taken_out_fails_only(cx, Gate::Connector);
}

/// Done when (#1208): with the drag's neutral file name taken out, every
/// drag fails for a Ready document, its file named after its title, and
/// every other exit still passes.
#[gpui::test]
fn taking_the_shelf_drags_neutral_name_out_fails_it(cx: &mut TestAppContext) {
    let _taken = take_out(Gate::ShelfFileName);
    for exit in EXITS {
        for scenario in READY {
            let faults = run(cx, exit, scenario);
            if exit.via.drags() {
                assert!(
                    !faults.is_empty(),
                    "{} passed with its neutral name taken out ({scenario:?})",
                    exit.name
                );
            } else {
                assert!(faults.is_empty(), "{faults:#?}");
            }
        }
    }
}
