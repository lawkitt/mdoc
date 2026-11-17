# Document preparation glossary

mdoc prepares local documents as editable Markdown for external AI tools.
This file only defines terms; rules live in `docs/specs/` and
[ADRs](docs/adr/). UI strings use US English ("unrecognized", "license");
identifiers and file names keep their spelling.

## Documents and handoff

**Source document**: the original PDF, DOCX or other file from which Markdown
is extracted.

**Converted Markdown**: editable Markdown extracted from a source document.

**Source preview**: the visual reference to the retained original, before
Markdown edits; DOCX rendering is approximate. UI label: **Source**.
"Original" names the view switcher's pre-pseudonymization view.

**Page tone**: how Original pages are painted: *original* paper or *themed*
(paper and ink remapped to the dark theme). Avoid "invert".

**Markdown handoff**: saving or copying prepared Markdown to another tool.
**Copy Markdown** copies the current source exactly.

**Copy for AI**: copying the full current Markdown with an alias-only note
asking the AI to keep placeholders exactly.

**AI analysis handoff**: preparing Markdown for external AI analysis without
bringing the response back.

**AI analysis return journey**: bringing an external AI response back into
mdoc and restoring its aliases from a case
([ADR 0041](docs/adr/0041-cases-and-ai-response-restoration.md)).

**Restore**: replacing aliases in a pasted AI reply with the source's primary
wordings. UI label: **Paste AI Reply** (Title Case in menus).

**Reply box**: the Share panel's "AI reply" field that restores a pasted
reply in place ([spec](docs/specs/cases-and-privacy.md)).

**Reply view**: the view switcher's read-only Reply segment showing the
restored reply.

**Unresolved token**: an alias-shaped token in AI output that matches no case
identity, such as an invented `PERSON_9`.

**Captured reply**: a real ChatGPT or Claude answer to a demo's Copy for AI
output, stored unedited under `tests/fixtures/restore/`
([spec](docs/specs/cases-and-privacy.md)).

**Multi-file opening**: opening several files for individual reading and lazy
conversion.

**Home**: the page shown with no tab open: drop zone, quick actions, cases and
recent files ([spec](docs/specs/desktop.md)).

**Recent files**: paths, never content, of recently opened files, stored
locally.

**Command palette**: the ⌘K search over actions, open tabs, cases and recent
files.

**Keyboard shortcuts panel**: the read-only sheet listing every shortcut by
context ([spec](docs/specs/desktop.md)).

**Focus ring**: the one keyboard-focus indicator, an accent ring outside the
control ([spec](docs/specs/desktop.md)).

**Find and Replace**: the find bar with its Replace row. It searches
**visible text**, the Markdown with syntax hidden; a match that **crosses
formatting** comes from several source ranges ([spec](docs/specs/desktop.md)).

**Confirm dialog**: mdoc's in-app modal question, used instead of the
platform prompt ([spec](docs/specs/desktop.md)).

**Document header**: the title-bar row above a document: breadcrumb, Source,
panel toggle, ⋯ and the primary action.

**Primary action**: the header's one filled button, which follows the
document's state: Pseudonymize, scanning + Cancel, Apply N, Copy for AI.

**Case section**: a collapsible sidebar group of one case's tabs; tabs
without a case sit under "Open".

**Case page**: the transient page in the content card giving a case's
documents, names, color, categories and delete
([spec](docs/specs/cases-and-privacy.md)).

**Case color**: one of eight named swatches stored on a case and shown
wherever the case appears.

**Folder sheet**: the sheet a dropped folder shows before anything opens,
listing its files to tick ([spec](docs/specs/desktop.md)).

**Settings category**: one entry of the Settings overlay's left list
(General, Detection, Recognition, Spelling, Cases, About)
([spec](docs/specs/desktop.md)).

## Models and processing

**OCR**: recognizing text in page images.

**Recognition languages**: the languages OCR is asked to read; each engine
maps them to its models or packs ([spec](docs/specs/recognition.md)). Avoid
"OCR language".

**Language signal**: what suggests a document's recognition languages: its
text layer, its file name, then a keyboard input source
([spec](docs/specs/recognition.md)).

**Qualified language**: a document language with its own OCR model,
spellcheck dictionary and PII qualification; English and Russian today.

**Model bundle**: pinned, compatible model artifacts plus the tokenizer or
dictionary they need.

**Setup consent card**: the inline first-use prompt naming a missing model
and its download size ([spec](docs/specs/recognition.md)). Avoid "setup
wizard".

**State card**: the centered Markdown-pane state of an unconverted document:
ready, waiting, converting, needs recognition or failed.

**Configuration snapshot**: the model identity and settings captured for one
run.

**Model comparison**: isolated sequential QA runs on one captured input, with
read-only results.

**Recognition output**: OCR text and evidence before fusion with native
text.

**Prepared Markdown**: the converter's final output, from native text, OCR
or both.

**Scan status**: the spinner and timed stage label beside Pseudonymize.

**Page artifact**: a non-text mark OCR misreads: a seal, signature,
barcode, watermark or repeated header.

**Stand-in**: a bracketed line such as `[Seal]` replacing a page artifact
or dropped picture. Avoid "placeholder".

**Vision bundle**: the pinned models that find page artifacts.

## Pseudonymization and review

**Pseudonymization**: reviewable replacement of identifying information with
consistent aliases that keep useful relationships. Avoid "anonymization"
([spec](docs/specs/cases-and-privacy.md)).

**Replacement mapping**: the local association between originals and aliases,
kept apart from the Markdown handed off.

**At-risk tab**: a tab that closing or quitting would lose something from:
unsaved changes, an unsaved mapping, or both.

**Case**: a named, saved mapping shared by a group of tabs. Avoid "matter"
and "project". Made with **Save as Case**
([ADR 0041](docs/adr/0041-cases-and-ai-response-restoration.md)).

**Case key**: the 256-bit key encrypting saved cases, held by the OS (macOS
Keychain, Windows DPAPI, Linux Secret Service)
([ADR 0045](docs/adr/0045-prompt-free-case-key.md)).

**Quiet probe**: the launch read of the case key that never shows a system
dialog.

**Dev key**: the case key of unsigned, ad-hoc and debug builds, under its own
Keychain service.

**Keychain explainer**: the dialog shown before the first case-key access and
after a denial.

**Variant proposal**: a suggested form ("Ms Raman", "Рамана", "Мариша") of a
known identity ([spec](docs/specs/recognition.md)).

**Propagation**: proposing an applied or marked name's forms elsewhere in the
document.

**Rule pack**: the TOML data in `crates/mdoc-detect/rules/` that corrects the
model ([ADR 0092](docs/adr/0092-recognition-correction-pipeline.md)).

**Case number**: the category of a claim or case number after a label (Claim
No, Дело №); aliases `CASE_1`. Code: `Category::CaseNumber`.

**Always and Never lists**: wordings always or never proposed, per case and
user-wide. Code: `mdoc_store::lists::Lists`.

**Web address**: the category of an identifying URL (aliases `URL_1`…), such
as a profile, a private site or a share link.

**Default categories**: Settings › Detection › Categories to detect.

**Flaw**: a miss, false hit or wrong boundary against the
[annotation guide](tests/fixtures/recognition/GUIDE.md). A **flaw class** is
the flaws one fix cures ([spec](docs/specs/recognition.md)).

**Recognition fixture**: one synthetic document with `⟦CATEGORY|text⟧` gold,
a status (`pass`, `known-flaw`, `model-limit`) and a **snapshot**, its raw
model output on the reference platform.

**Detection gate**: the model run over every recognition fixture,
`ci/detection` in shards ([spec](docs/specs/recognition.md)).

**Reference platform**: the one platform whose model output the detection
gate's snapshots record; hosted Windows.

**Hunter, curator, fixer**: the flaw hunt's roles: one writes documents and
reports flaws, one audits gold and lands fixtures, one fixes a flaw class.

**Holdout**: synthetic recognition documents kept in the private
`lawkitt/mdoc-holdout` to catch overfitting.

**Real set**: about 50 real public documents with frozen gold, kept in
`lawkitt/mdoc-probes`; recognition targets are scored on it.

**Probe**: a real public document in `lawkitt/mdoc-probes` run through mdoc
to find flaw classes; never a fixture.

**Judge**: the agent that reads a probe beside mdoc's proposals and lists its
flaws.

**Merge hint**: a dismissible "Looks like Maya Bennett · Merge" line for two
identities that very likely name the same subject
([spec](docs/specs/recognition.md)).

**Decision history**: the document's list of pseudonymization decisions with
times. UI label: **What you did** ([spec](docs/specs/desktop.md)).

**Pseudonymize Clipboard**: replacing the clipboard text's known case names
with their aliases, without detection.

**Restore Clipboard**: replacing aliases in the clipboard text with a case's
primary wordings.

**Shelf**: the macOS menu bar item's panel of pseudonymized documents and
clipboard actions ([spec](docs/specs/desktop.md)). Avoid "tray".

**Ready document**: one with at least one applied alias and no pending
proposals.

**Shelf export**: the temporary `.md` file a shelf drag carries.

**Side by side view**: the read-only, line-paired view of original wording
beside pseudonymized text. Avoid "diff".

**View switcher**: the floating Edit · Original · Pseudonymized · Side by side
(· Reply) control.

**Crash report**: a local, masked panic record the user may preview and send
([spec](docs/specs/cases-and-privacy.md)).

**Primary wording**: the variant an identity restores to.

**Placeholder / alias**: a stable replacement token such as `PERSON_1`;
generated by mdoc or a **custom alias** the user chose.

**Identity**: a document-local entity owning a stable neutral alias and
original variants.

**Variant**: one exact original wording whose mentions belong to an identity.
Code: `mdoc_pii::Variant`.

**Mention**: one occurrence of identifying information in the Markdown source.

**Occurrence assignment**: an identity override for one mention, separating
homonyms.

**Sameness**: mentions refer to one identity. **Ownership**: a contact,
address or identifier belongs to a person or organization. **Affiliation**: a
person represents an organization.

**Proposed mapping**: staged identity and alias changes until Apply
([spec](docs/specs/cases-and-privacy.md)).

**Review candidate**: a detected span proposed for replacement.

**Replacements panel**: the searchable overview beside the editor, grouped by
entity ([spec](docs/specs/desktop.md)).

**Category header**: a non-interactive panel line above a category's group
headers, "Person 3 (7)".

**Group header**: an entity's panel row: original, alias, count.

**Mention row**: one mention under an expanded group header, with a snippet.

**Inline controls**: the one-line Apply to · Apply · Keep original under a
mention row selected in the panel.

**Keyboard move**: ⌥↑/↓ stepping a mention or group through drop targets.

**Triage focus**: focus moving to the next undecided mention after a
decision.

**Scope**: This mention, Same wording or Entire entity; the reach of one
alias, category, Apply, Undo or Keep action.

**Manual addition**: selected missed text made a proposal. UI label:
**Replace · Person ▾**, a split pill ([spec](docs/specs/desktop.md)).

**Other category**: the manual-only neutral category with `REDACTED_n`
aliases.

**Replacement chip**: a mention highlight with a category-tinted underline.

**Entity checkbox**: the panel's tick per entity: ticked will be replaced,
unticked kept, – mixed.

**Mention bar**: the slim bar above a clicked mention: Replace checkbox,
alias, category and This one | All scope ([spec](docs/specs/desktop.md)).

**Kept marker**: the muted dotted underline on a kept mention.

**Scope outline**: the display-only ring around mentions the selected scope
affects.

**Undo replacement**: restoring applied text as a proposal that keeps its
alias. **Keep original**: restoring the original text and recording a Keep
decision.

**Close review**: hiding candidate highlights while keeping edits; it does not
establish safe sharing.

## Spelling

**Spellcheck**: offline EN/RU misspelling flags in Converted Markdown, mainly
for conversion and OCR errors
([ADR 0036](docs/adr/0036-bundled-spellcheck.md)).

**Misspelling**: a flagged word span. Code: `mdoc_editor::Diagnostic`.

**Spell exclusion**: a byte range never dictionary-checked, supplied by the
host.

**Skip rule**: a word-level filter inside `mdoc-spell`.

**Mixed-script word**: a letters-only word combining Latin and Cyrillic
letters.

**User dictionary**: words added with "Add to dictionary", in
`spelling-words.txt` beside the settings. Code: `settings::UserWords`.

**Ignored word**: a word skipped in one document for the session.

**Legal word list**: the curated supplement to the base EN/RU dictionaries
holding legal vocabulary, never names.

## Application structure

**Empty page**: the immediately writable empty Markdown view, also a
file-drop zone.

**Workspace**: one window's shell, sidebar, toolbar, Settings and tabs. Code:
`workspace::Workspace`.

**Tab**: a sidebar entry owning a document view once initialized.

**Rail** and **hover reveal**: the collapsed sidebar's icon strip, and the
full document list that slides out over the editor on hover
([spec](docs/specs/desktop.md)).

**Back/Forward**: per-window history of tab activations, Home and the case
page.

**Region**: a part of the window that F6 / ⇧F6 move focus between
([spec](docs/specs/desktop.md)).

**Document view**: one tab's editor, source preview, search, import, OCR and
pseudonymization state. Code: `document_view::DocumentView`.

## Code structure

**Owner**: the one place that holds a piece of state and changes it, with
private fields
([ADR 0069](docs/adr/0069-app-architecture-and-code-ownership.md)). Avoid
Manager, Controller or Service in owner names.

**Transition**: a named, synchronous, GPUI-free method on an owner that
changes its state in response to one input.

**Effect**: what a transition asks its GPUI view to do, as a variant of that
owner's effect enum.

**Case coordination**: the window-level owner (`CaseCoordinator`) of case
state, the case key and case saves. A tab's **case link** (`CaseLink`) is
its own membership and pending save.

**Cached view**: a GPUI entity rendered through `.cached()`, whose last frame
a window rebuild reuses unless it was notified.

**Document lifecycle**: a document view's loading, generation, availability,
conversion and dirty state (`DocumentLifecycle`). Distinct from the
file-level **document session**.

## Releases and CI

**Release commit**: the commit `just release` makes to set the version, which
its tag names.

**Pre-release**: a version with a SemVer suffix such as `-rc.2`.

**Shipped target**: a platform with release artifacts: Apple Silicon macOS and
Windows x64.

**Update check**: the detection-only comparison of the running version with
the newest GitHub Release ([spec](docs/specs/desktop.md)).

**Release gate**: an automated check a release passes on every shipped
target, such as the packaged-app self-test.

**Release candidate (RC)**: a `-rc.N` pre-release that a minor's platform
acceptance runs on.

**Platform acceptance**: the manual checklist run on an installed release on
each shipped target; each row is *Pass*, *Fail* or *Unavailable*.

**Smoke run**: a short acceptance check on each shipped target: install,
launch, open a document, Copy for AI.

**Must-fix / should-fix**: roadmap tiers for a milestone; stable waits only on
must-fix items.

**Patch lane**: the 0.x.y milestone after a release, for fixes that need no
design decision.

**Later**: roadmap items beyond the next milestone, one line each.

**Ordinary legal document**: the interactive performance target, about 30
pages or 135 KB of Markdown.

**Merge queue**: GitHub's queue on `main`, which tests `main` plus a group of
queued PRs on a **queue commit** (`gh-readonly-queue/main/*`)
([ADR 0095](docs/adr/0095-ci-on-a-merge-queue.md)).

**Fast tier**: what every PR head runs on hosted Ubuntu.

**Test tier**: a named slice of `just test`: `unit`, `app`, `renders`,
`recognition` and `scripts`.

**`merge-gate`**: the one aggregate required status beside `lint (ubuntu)`.

**Infrastructure failure**: a hosted CI job that ended without a result, as
opposed to a test failure.

**Follow-up pass**: a `merge-gate` pass started by `merge-gate-recheck.yml`
after one that left something pending.

**Quarantine list**: `.config/quarantine.txt`, tests CI excludes for a while,
each with an issue and an expiry date.

**Timing test**: a test that measures how fast something runs rather than
whether it works.

**Area**: one of desktop, core, recognition, ios, android and ci, from
`scripts/ci-areas.txt`; a change runs the jobs of the areas it touches.

**Core change**: a change that touches the core area; its queue commit runs every
CI job.

**Android job**: the build of the Android core and app with its JVM and
emulator tests (`ci/android`).

**iOS job**: the build of the iOS core and app with XCTest and XCUITest on the
pinned Simulator (`ci/ios`).

**Mobile release draft**: where an agent-started mobile release stops, before
anything users can install.

**Beta build** (iOS): a TestFlight build tagged `ios-v<x.y.z>-rc.N`; the
plain `ios-v<x.y.z>` tag marks the build sent to App Store review.

**Lawyers group**: the one external TestFlight group, and Play's closed
testing track of the same name.

**Never-client-files line**: the sentence that opens every beta description:
"Test with documents you are allowed to share, never client files; check a
screenshot before you send it."

## Mobile apps

**Mobile core**: the GUI-free Rust crates the iOS and Android apps call
through `crates/mdoc-ffi`'s UniFFI bindings
([ADR 0085](docs/adr/0085-mobile-apps.md)).

**Review-first flow**: the mobile journey: scan or import, recognize, review
replacements, share to an AI app, restore the reply.

**Restore with mdoc**: the mobile share-sheet action that opens an AI reply
from another app restored.

**Rendered view**: how mobile apps show Markdown: markers hidden, blocks drawn
over the unchanged source ([spec](docs/specs/mobile.md)).

**Show Markdown**: the mobile document menu's toggle between the rendered
view and the source.

**Document menu** (mobile): the menu under the document's title, the one
place for its commands.

**Adjust Words** (mobile): correcting a found name's span with handles in the
text. Called "Fix the words" in code (`wordsFix`, `fix_words`). Not "rename",
which changes the alias.

**Edit mode** (mobile): the document screen with the keyboard, the editing
toolbar and editable text ([ADR 0109](docs/adr/0109-mobile-markdown-editor.md)).
Not "edit view".

**Editor core**: the GUI-free Rust editing rules that desktop's `mdoc-editor`
and both phones call ([ADR 0109](docs/adr/0109-mobile-markdown-editor.md)).

**Reveal rule**: when hidden Markdown markers show, on the phone and on
desktop.

**Formatting keyboard** (mobile): the panel the editing toolbar's Aa key swaps
in for the system keyboard. Not "format menu".

**Table controls** (mobile): what the editing toolbar shows with the caret in
a table cell.

**Panes row** (mobile): the segmented row under the navigation bar: Text,
Original and Reply on iPhone. Not "tabs".

**In-app clipboard** (mobile): the pasteboard edit mode's Cut and Copy write,
which only mdoc reads.

**Table pass test**: what the rendered view's tables must do inside the one
TextKit 2 text view ([spec](docs/specs/mobile.md)).

**Device floor**: the least physical RAM on which a phone gets the GLiNER2
detector ([spec](docs/specs/mobile.md)).

**Hand-off bar**: the bar under a reviewed document with Share, Copy and Paste
reply.

**Sidebar** (mobile): the panel under the screen on iPhone, the first column
on a wide iPad ([spec](docs/specs/mobile.md)).

**Identities drawer** (mobile): the open document's names in a panel from the
right edge on iPhone, a standing column on iPad.

**Start** (mobile): the main area with no document open.

**Review bar**: the docked bar a phone reviews in: progress line, review card,
then Undo, Keep and Replace ([spec](docs/specs/mobile.md)).

**Review card**: the swipeable part of the review bar: the name under review
and its chips.

**Peek**: the sliver of the previous or next review card at the bar's edges.

**Progress line**: "3 of 15" and a thin track under the review bar's grabber.

**Mentions chip**: "1 of 3 ›" on the review card when an identity has more
than one mention.

**Full view** (mobile review): the review bar dragged up, one row per mention
of the identity under review. Also "expanded bar".

**State mark**: a mention's or identity's state as a round mark: dashed ring
proposed, accent disc replaced, eye kept.

**Hint line**: a one-line tip in the review bar or Identities about a
gesture.

**Drag-to-merge** (mobile Identities): dragging one name's row onto another's
to make them one.

**Focus** (mobile review): stepping limited to one identity or one category.

**Mobile spec**: `docs/specs/mobile.md`, the current rules for both mobile
apps.

**Sample document**: the short contract with made-up names behind "Try a
sample", also the UI tests' fixture.

**UI survey**: light and dark screenshots of every mobile screen, taken by
`SurveyUITests` (`just ios-survey`) and `just android-survey`.

**Device UI tests**: the review flow and long jobs run on a real phone by hand
(`just ios-device-test`, `just android-device-test`).

**Real-device tiers** (Android): the emulator, Firebase Test Lab phones by RAM
class, and a phone by hand ([ADR 0089](docs/adr/0089-android-app.md)).
