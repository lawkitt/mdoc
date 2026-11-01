# Document preparation glossary

mdoc prepares local documents as editable Markdown for external AI tools.
Behavior and rationale live in [ADRs](docs/adr/); this file defines shared terms.
UI labels and every UI string use US English ("unrecognized", "license",
"organization"); identifiers and file names keep their spelling.

## Documents and handoff

**Source document**: the original PDF, DOCX or other file from which Markdown is extracted.

**Converted Markdown**: editable Markdown extracted from a source document.

**Source preview**: visual reference to the retained original, before Markdown
edits. DOCX rendering is approximate. UI label: **Source** (was "Original"
until ADR 0048; "Original" now names the pill's pre-pseudonymization view).

**Page tone**: how Original pages are painted: *original* paper or *themed*
(paper and ink remapped to the dark theme). Follows the app theme unless
overridden in the Original header ([ADR 0035](docs/adr/0035-dark-original-pages.md)).
Avoid "invert", which suggests a plain color flip.

**Markdown handoff**: saving or copying prepared Markdown to another tool.
**Copy Markdown** copies the full current source exactly, including unsaved edits
and pending proposals; it does not apply replacements or add a relationship legend.
**Copy for AI** adds the placeholder note.

**AI analysis handoff**: prepare Markdown for external AI analysis without
bringing the response back. This is the first qualification journey ([ADR 0016](docs/adr/0016-pii-production-qualification-scope.md)).

**AI analysis return journey**: bring an external AI response back into mdoc
and restore its aliases from a case ([ADR 0041](docs/adr/0041-cases-and-ai-response-restoration.md)).

**Copy for AI**: copy the full current Markdown with an alias-only note asking
the AI to keep placeholders exactly; shown when aliases are applied.

**Restore**: replace aliases in a pasted AI reply with the source's primary
wordings, in a new tab where restored values are proposals keeping their aliases.
UI label: **Paste AI Reply** (Title Case in menus, "Paste AI reply" elsewhere).

**Reply box**: the Share panel's "AI reply" field that restores a pasted
reply in place; its summary reads "N values restored · M unrecognized"
with Next unrecognized, Copy reply and Open as tab. Plain text: a paste is
kept exactly. The reply, in alias form, is kept encrypted in its case, or
only while the tab is open without a case; its aliases follow the document's
renumbering ([ADR 0078](docs/adr/0078-restore-in-place.md)).

**Reply view**: the view switcher's read-only Reply segment showing the
restored reply; live, like Side by side, and shown while a reply exists.
Filling the reply box switches to it. Open as tab turns it into a
Restore tab.

**Unresolved token**: an alias-shaped token in AI output that matches no case
identity (e.g. an invented `PERSON_9`); highlighted, never guessed.

**Captured reply**: a real ChatGPT or Claude answer to a demo's Copy for AI
output, stored unedited under `tests/fixtures/restore/` with what was sent and
the mapping used; the gate for Restore in a stable release
([ADR 0056](docs/adr/0056-captured-ai-reply-fixtures.md)). Contrast the
synthetic replies in `crates/mdoc-pii/fixtures/restore/`.

**Multi-file opening**: open several files for individual reading and lazy conversion.

**Batch conversion**: process an explicitly selected group without opening each
file individually. This workflow remains deferred; avoid calling multi-file opening batch conversion.

**Home**: the page shown with no tab open: drop zone, quick actions, cases and
recent files ([ADR 0048](docs/adr/0048-workspace-shell-redesign.md)).

**Recent files**: paths (never content) of recently opened files, stored
locally and clearable in Settings.

**Command palette**: ⌘K search over actions, open tabs, cases and recent files.

**Keyboard shortcuts panel**: the read-only sheet listing every shortcut by
context (Everywhere, Editor, Popup, Panel), opened with `?` outside text
fields or ⌘/ / Ctrl+/; built from the one binding table
([ADR 0073](docs/adr/0073-keyboard-shortcuts-panel.md)).

**Focus ring**: the one keyboard-focus indicator, an accent ring outside the
control; never a fill, so it stays distinct from selection, hover and a toggle
that is on ([ADR 0074](docs/adr/0074-one-focus-ring.md)).

**Find and Replace**: the find bar with its Replace row (⌘⌥F / Ctrl+H). It
searches **visible text**, the Markdown with syntax hidden. A match that **crosses formatting** (its
visible text comes from several source ranges, as "foo bar" in
`foo **bar**`) or overlaps an applied alias is never replaced
([ADR 0063](docs/adr/0063-find-and-replace.md)).

**Confirm dialog**: mdoc's in-app modal question (unsaved changes, model
removal, case actions), used instead of the platform prompt
([ADR 0064](docs/adr/0064-dialog-and-role-semantics.md)).

**Document header**: the title-bar row above a document: breadcrumb, Source,
panel toggle, ⋯ and the primary action.

**Primary action**: the header's one filled button, which follows the
document's state: Pseudonymize, scanning + Cancel, Apply N, Copy for AI.

**Case section**: a collapsible sidebar group of the tabs of one case; tabs
without a case sit under "Open".

**Case page**: the transient page in the content card, like Home, giving a
case's documents, names, color, categories and delete
([ADR 0077](docs/adr/0077-case-page.md)).

**Case color**: one of eight named swatches stored on a case and shown
wherever the case appears; it never encodes state.

**Folder sheet**: the sheet a dropped folder shows before anything opens: its
files to tick, Open as case or Open files, at most 20
([ADR 0075](docs/adr/0075-folder-sheet.md)).

**Settings category**: one entry of the Settings overlay's left list (General,
Detection, Recognition, Spelling, Cases, About), whose rows show at the right
([ADR 0076](docs/adr/0076-settings-categories.md)).

## Models and processing

**OCR**: recognize text from document images when direct extraction is insufficient.

**Recognition languages**: the list of languages OCR is asked to read; each
engine maps it to the models or packs it has ([ADR 0065](docs/adr/0065-multilingual-by-design.md)).
Picked per document on the OCR card from the engine's offered lists.
Avoid "OCR language" for a single fixed pair.

**Language signal**: what suggests a document's recognition languages: its
text layer, its file name, then a keyboard input source; the first that
says anything sets the default, explained in one line on the card
([ADR 0066](docs/adr/0066-recognition-language-defaults.md)).

**OS OCR engine**: text recognition built into the operating system (Apple
Vision, `Windows.Media.Ocr`), needing no model download. Evaluated in Later,
not yet used.

**Qualified language**: a document language with its own OCR model,
spellcheck dictionary and PII qualification; mdoc claims support only for
these (English and Russian today).

**Model bundle**: pinned, compatible model artifacts and required tokenizer or
dictionary. Installation/runtime compatibility does not establish detection quality.

**Setup consent card**: inline first-use prompt naming the missing model and
its download size. Explicit download installs it and resumes the requested task
([ADR 0026](docs/adr/0026-first-use-model-setup.md)). Avoid automatic setup or setup wizard.

**State card**: centered Markdown-pane state for an unconverted document:
ready, waiting, converting, needs recognition or failed. Hosts OCR setup consent.

**Configuration snapshot**: model identity and settings captured for one run;
later default changes do not change its attribution.

**Model comparison**: isolated sequential QA runs on one captured input with
read-only results; overrides change neither defaults nor the document.

**Recognition output**: OCR text/evidence before native-text fusion and final
Markdown preparation. Confidence is not measured accuracy.

**Prepared Markdown**: final converter output, potentially combining native
text and OCR. Comparisons show it separately without replacing the working document.

**Scan status**: spinner and timed stage label beside Pseudonymize. Stages are
not measured progress ([ADR 0032](docs/adr/0032-replacement-popup-redesign.md)).

## Pseudonymization and review

**Pseudonymization**: reviewable replacement of identifying information with
consistent aliases while retaining useful relationships. Scanning proposes;
explicit Apply edits Markdown. Avoid guaranteed anonymization, Anonymize or
shared category markers ([ADR 0022](docs/adr/0022-remove-anonymization-mode.md)).

**Replacement mapping**: local association between originals and aliases,
separate from the Markdown handed off. Unsaved mappings live only in the open
document; a case stores its mapping encrypted on disk.

**At-risk tab**: a tab that close or quit would lose something from: unsaved
changes, an unsaved mapping, or both; each gets one prompt
([ADR 0071](docs/adr/0071-one-close-and-quit-prompt.md)).

**Case**: a named, saved mapping shared by a group of tabs; shown as a tab group
in the sidebar. One case per document. Avoid "matter" and "project". Making
one: **Save as Case** (Title Case in menus and dialogs, "Save as case" elsewhere;
not "Save mapping as case").

**Case key**: the 256-bit key encrypting saved cases, held by the OS (macOS
Keychain, Windows DPAPI, Linux Secret Service). Read only for an action that
needs it ([ADR 0043](docs/adr/0043-deferred-keychain-access.md)). Signed
macOS builds move it to the data protection keychain, which never asks
([ADR 0045](docs/adr/0045-prompt-free-case-key.md)).

**Variant proposal**: a suggested link between a shorter or inflected form
("Ms Raman", "Raman's", "Рамана") and a known identity. Always reviewed, never
linked automatically; a first name alone is not proposed
([ADR 0068](docs/adr/0068-short-and-inflected-name-variants.md)).

**Merge hint**: a dismissible line under an identity in the Replacements
panel, "Looks like Maya Bennett · Merge", for two identities that very likely
name the same subject. Never applied automatically; Merge is one undo step;
no score is shown ([ADR 0080](docs/adr/0080-merge-hint.md)).

**Decision history**: the document's list of pseudonymization decisions with
times, from the header clock button; a click travels undo to that entry.
Memory only. UI label: **What you did** ([ADR 0079](docs/adr/0079-decision-history.md)).

**Default categories**: Settings › Detection › Categories to detect; filters
scans of documents without a case and seeds a new case's list (ADR 0076).

**Quiet probe**: the launch read of the case key with per-query "no UI"; it
either opens cases or leaves them locked, and never shows a system dialog.

**Dev key**: the case key of unsigned, ad-hoc and debug builds, under its own
Keychain service so it never collides with the release key.

**Keychain explainer**: the dialog shown before the first case-key access (and
after a denial), with an animated preview of the macOS password dialog.

**Pseudonymize Clipboard**: replace the clipboard text's known case names with
their aliases, without detection. Unknown names stay; it is not a scan.

**Restore Clipboard**: replace aliases in the clipboard text with a case's
primary wordings, from the shelf; the clipboard counterpart of Restore
([ADR 0084](docs/adr/0084-menu-bar-shelf.md)).

**Shelf**: the menu bar item's panel: pseudonymized documents as small
previews to open, drag out or Copy for AI, and the clipboard actions. On by
default on macOS, which keeps mdoc running without a window; a one-time
**first-close note** says so (ADR 0084). Avoid "tray"
for the macOS item.

**Ready document**: one with at least one applied alias and no pending
proposals; only a ready document can be copied or dragged from the shelf
(ADR 0084).

**Shelf export**: the temporary `.md` file a shelf drag carries, written from
the current text into a private folder cleared at quit; its name is the title
with known originals pseudonymized (ADR 0084).

**Side by side view**: read-only, line-paired view of original wording beside
pseudonymized text. Live: it follows every applied-replacement change
(ADR 0042). Avoid "diff", which suggests arbitrary edits.

**View switcher**: the floating Edit · Original · Pseudonymized · Side by side
(· Reply) control shown while replacements are applied ([ADR 0046](docs/adr/0046-workspace-polish-from-anoni-review.md)).

**Crash report**: a local, masked panic record the user may preview and send
on the next launch; never sent automatically ([ADR 0047](docs/adr/0047-crash-reports.md)).

**Primary wording**: the variant an identity restores to; defaults to the
longest full form and can be changed.

**Placeholder / alias**: stable replacement token such as `PERSON_1`.
Generated aliases come from mdoc; custom aliases retain deliberate user names
through category corrections.

**Identity**: document-local entity owning a stable neutral alias and original variants.

**Variant**: one exact original wording whose mentions belong to an identity.
Linking variants shares an alias. Code: `mdoc_pii::Variant`.

**Mention**: one occurrence of identifying information in the Markdown source.

**Occurrence assignment**: identity override for a particular mention, allowing homonyms to be separated.

**Sameness**: mentions refer to one identity. **Ownership**: a contact, address
or identifier belongs to a person/organization. **Affiliation**: a person
represents an organization. These are separate relationships, not proximity rules.

**Proposed mapping**: staged identity/alias changes against originals until
Apply; tracked replacements remain correctable afterward ([ADR 0018](docs/adr/0018-document-local-identity-review.md)).

**Review candidate**: detected span proposed for replacement.

**Replacements panel**: searchable overview beside the editor, grouped by entity.
Selecting a mention row reveals and outlines it in the editor and shows inline
controls; editor selections use the word popup ([ADR 0020](docs/adr/0020-direct-replacement-workspace.md),
[ADR 0033](docs/adr/0033-replacements-panel-redesign.md)).

**Category header**: a non-interactive panel line above a category's group
headers, "Person 3 (7)" (entities, mentions) ([ADR 0046](docs/adr/0046-workspace-polish-from-anoni-review.md)).

**Group header**: an entity's panel row (original, alias, count). Expands
independently of selection; its alias and category edit the whole entity.

**Mention row**: one mention under an expanded group header, with a snippet.

**Inline controls**: one-line Apply to · Apply · Keep original under a mention
row selected in the panel; the panel's counterpart to the word popup.

**Keyboard move**: ⌥↑/↓ steps a mention or group through drop targets; releasing
⌥ moves the mention (or merges the entity), Esc cancels.

**Triage focus**: after a keyboard or middle-click decision, focus moves to the
next undecided mention.

**Scope**: This mention, Same wording or Entire entity. Same wording includes
matching originals assigned to the selected entity; Entire entity includes its
other variants. One scope governs alias, category, Apply, Undo and Keep actions.

**Manual addition**: selected missed text becomes a proposal with a correctable
category; exact repeats are proposed together, except inside a longer
mention, which stays whole. UI label: **Replace · Person ▾**, the split
pill: Replace adds with the guess, ▾ picks the category first
([ADR 0024](docs/adr/0024-replace-custom-selection.md)).

**Other category**: manual-only neutral category with a `REDACTED_n` alias;
the detector never emits it.

**Replacement chip**: mention highlight with a category-tinted underline;
light tint while proposed, alias chip once applied (ADR 0048).

**Entity checkbox**: the panel's tick per entity: ticked = will be replaced,
unticked = Keep original, – = mixed mentions. Ticking records intent; text
changes only on Apply.

**Mention bar**: slim bar above a clicked mention: Replace checkbox, alias,
category, This one | All scope. Enter applies in the shown scope and moves to
the next proposal. Replaced the replacement popup (ADR 0048);
stable, compact and reversible per [ADR 0055](docs/adr/0055-stable-compact-mention-popup.md).
Motion and scoped popup presentation follow [ADR 0032](docs/adr/0032-replacement-popup-redesign.md).

**Kept marker**: muted dotted underline on a kept mention while reviewing;
clicking it reopens the mention bar with Replace unticked (ADR 0055).

**Scope outline**: display-only ring around mentions affected by the selected scope.

**Undo replacement**: restore applied text as a proposal retaining its alias.
**Keep original**: restore original text and record a Keep decision. Both follow
text undo/redo ([ADR 0023](docs/adr/0023-pseudonymization-ui-polish.md)).

**Close review**: hide candidate highlights while retaining edits, clickable
applied highlights and live restoration provenance. It does not establish safe sharing.

## Spelling

**Spellcheck**: offline EN/RU misspelling flags in Converted Markdown, mainly
to surface conversion/OCR errors ([ADR 0036](docs/adr/0036-bundled-spellcheck.md)).
Results are identical on every platform; it never uses the OS checker.

**Misspelling**: a flagged word span, shown as a squiggle with lazy suggestions.
Code: `mdoc_editor::Diagnostic`.

**Spell exclusion**: a byte range never dictionary-checked: code, link targets,
raw HTML, aliases and PII entity spans. Supplied by the host, not `mdoc-spell`.

**Skip rule**: word-level filter inside `mdoc-spell`. ALL-CAPS and digit-containing
words are skipped by default with Settings toggles; e-mail and bare URLs always.

**Mixed-script word**: a letters-only word combining Latin and Cyrillic letters;
always flagged, even inside entity spans, as a likely OCR homoglyph error.

**User dictionary**: words the user added with "Add to dictionary"; exact
spellings in `spelling-words.txt` beside the settings, shared by all documents
and managed in Settings → Spelling → Manage…. Code: `settings::UserWords`.

**Ignored word**: a word skipped in one document for the session with "Ignore
in this document"; never saved.

**Legal word list**: curated, non-user-editable supplement to the base EN/RU
dictionaries holding real legal vocabulary, never names.

## Application structure

**Empty page**: immediately writable empty Markdown view with Open files,
supported formats, multi-file hint and shortcut; also a file-drop zone ([ADR 0028](docs/adr/0028-empty-page-and-file-drop.md)).

**Workspace**: one window's shell, sidebar, toolbar, Settings and tabs,
including unopened restored records. Code: `workspace::Workspace`.

**Tab**: sidebar entry owning a document view once initialized; restored tabs
allocate their document view when activated.

**Back/Forward**: per-window history of tab activations, Home and the case
page; ⌘[ / ⌘] ([ADR 0081](docs/adr/0081-workspace-back-forward.md)).

**Region**: a part of the window that F6 / ⇧F6 move focus between: sidebar,
toolbar, editor, replacements panel, source preview, comments, or Home's
content. Tab follows window order; the Replacements panel, popups, menus and
dialogs keep it inside ([ADR 0062](docs/adr/0062-keyboard-reach.md)).

**Document view**: one tab's editor, source preview, search, import/OCR and
pseudonymization state. Code: `document_view::DocumentView`. Avoid calling this workspace.

## Code structure

**Owner**: the one place that holds a piece of state and changes it, with
private fields; other code calls its methods ([ADR 0069](docs/adr/0069-app-architecture-and-code-ownership.md)).
A state machine owner is a plain struct (`QuitFlow`, `SessionWriter`,
`DocxQueue`, `CaseCoordinator`, `DocumentLifecycle`, `ImportFlow` …); a region
that renders and takes focus on its own may be a GPUI entity. Avoid Manager,
Controller or Service in owner names.

**Transition**: a named, synchronous, GPUI-free method on an owner that
changes its state in response to one input.

**Effect**: what a transition asks its GPUI view to do (spawn a task, show a
prompt, emit an event), as a variant of that owner's own effect enum.

**Case coordination**: the window-level owner (`CaseCoordinator`) of case
state, the case key and case saves across tabs. Distinct from a tab's
**case link** (`CaseLink`), its own membership and pending save.

**Document lifecycle**: a document view's loading, generation, availability,
conversion and dirty state (`DocumentLifecycle`). Distinct from file-level
**document session** (accepted identity, provenance, external-change checks).

## Releases

**Release commit**: the commit made by `just release` that sets the version in
`Cargo.toml`, `Cargo.lock` and `Info.plist` (and, for stable versions, names
the CHANGELOG section); its tag must match that version ([ADR 0037](docs/adr/0037-ci-and-release-pipeline.md)).

**Pre-release**: a version with a SemVer suffix such as `-beta.1` or `-rc.2`;
marked pre-release on GitHub, built with the same artifacts as a stable
version (on Windows the NSIS `.exe` only), and offered by the update check to
pre-release builds or when "Include pre-releases" is on.

**Shipped target**: a platform with release artifacts — Apple Silicon macOS
and Windows x64. Other platforms build from source, unsupported.

**Update check**: detection-only comparison of the running version with the
newest GitHub Release; shows Settings → Updates and never installs ([ADR 0038](docs/adr/0038-update-check.md)).

**Local CI**: the pre-merge test gate on the maintainer's Windows PC and Mac:
a runner on each that tests PR heads from branches in the repository, and
new `main` commits when no PR waits, and posts a desktop and a mobile local
status. Hosted CI keeps the Ubuntu lint on PRs, `main` and releases
([ADR 0082](docs/adr/0082-local-ci.md)).

**Local status**: a commit status local CI posts on a PR head or `main`
commit: `local/windows` and `local/macos` (desktop), `local/android` and
`local/ios` (mobile), each from its own job; "Not affected" when the change
is outside its area. With `lint (ubuntu)`, the five checks a code PR needs
green ([ADR 0091](docs/adr/0091-mobile-ci-and-releases.md)).

**CI checkout**: local CI's own clone of the repository on a machine, separate
from lanes and worktrees, with its own `target/`.

**Release gate**: checks a release must pass on every shipped target: the OCR
smoke test before packaging, and the packaged-app self-test (`just
smoke-package`) on the built artifact itself.

**Must-fix / should-fix**: roadmap tiers for a milestone. Stable waits only on
must-fix items; unfinished should-fix items move to the next milestone. Distinct
from a release gate, which is an automated check on every release
([ADR 0051](docs/adr/0051-roadmap-structure-and-stable-scope.md)).

**Patch lane**: the 0.x.y milestone after a release, for items that give wrong
output, lose data or make an action fail silently and need no design decision;
each is a GitHub issue ([ADR 0059](docs/adr/0059-roadmap-after-0-1-0.md)).

**Later**: roadmap items beyond the next milestone, one line each, written out
only when promoted at grooming ([ADR 0059](docs/adr/0059-roadmap-after-0-1-0.md)).

**Smoke run**: a patch release's acceptance check on each shipped target:
install, launch, open a document, Copy for AI, plus the platform-acceptance
rows its fixes touch; no release candidate ([ADR 0059](docs/adr/0059-roadmap-after-0-1-0.md)).
A minor's stable gets one too, after its single RC ([ADR 0083](docs/adr/0083-fewer-release-candidates.md)).

**Release candidate (RC)**: a `-rc.N` pre-release a minor's full platform
acceptance runs on. One per minor by default; another only when a must-fix fix
touches the installer, signing, updates or the keychain. Other fixes are
re-checked on a local unsigned build ([ADR 0083](docs/adr/0083-fewer-release-candidates.md)).

**Platform acceptance**: a manual checklist run on an installed release
candidate on each shipped target, recorded under `docs/evidence/platform/`;
each row is *Pass*, *Fail* or *Unavailable* (could not be observed, with the
reason). Distinct from a release gate, which is automated ([ADR 0057](docs/adr/0057-native-platform-acceptance.md)).

**Ordinary legal document**: the interactive performance target, about 30 pages
or 135 KB of Markdown. The 2 MiB / 20,000-mention workload is the separate
*stress tier* ([ADR 0049](docs/adr/0049-measured-stabilization-and-architecture-review.md)).

## Mobile apps

**Mobile core**: the GUI-free Rust crates the iOS and Android apps call
through `crates/mdoc-ffi`'s UniFFI bindings ([ADR 0085](docs/adr/0085-mobile-apps.md)).

**Review-first flow**: the mobile apps' one journey: scan or import,
recognize, review replacements, share to an AI app, restore the reply. No
text editing.

**Restore with mdoc**: the mobile share-sheet action that takes an AI reply
from another app and opens it restored.

**Device floor**: the least physical RAM on which a phone gets the GLiNER2
detector, measured on real phones ([ADR 0088](docs/adr/0088-ios-on-real-devices.md));
below it the app imports, recognizes text and lets you mark names by hand,
and says it cannot find names. There is no rules-only scan.

**Feasibility spike**: the device measurements of the mobile core (load,
memory, scan time, heat, OCR output) that come before mobile UI work.

**Hand-off bar**: the bar under a reviewed document on mobile with Share,
Copy and Paste reply; it replaces the review actions once nothing is left
to decide.

**UI survey**: light and dark screenshots of every mobile screen, taken by
`ScreenshotUITests` on iOS and `just android-survey` on Android, and kept
under `docs/evidence/ios-app` and `docs/evidence/android-app` as before and
after sets.

**Device UI tests**: the review flow and long jobs run on a real phone
(`just ios-device-test`, `just android-device-test`), by hand before each
release after the first, to catch what simulators and emulators miss.

**Real-device tiers** (Android): the emulator daily, Firebase Test Lab
phones by RAM class, and a friend's phone weekly by hand ([ADR 0089](docs/adr/0089-android-app.md)).

**Android job**: the run that builds the Android core and app and runs the
JVM parity tests and the emulator tests, when a change touches the core or
the android area (`just android`); posted as `local/android` from the PC,
and later the Mac by takeover or hosted Linux
([ADR 0089](docs/adr/0089-android-app.md), [ADR 0091](docs/adr/0091-mobile-ci-and-releases.md)).

**iOS job**: the Mac's build of the iOS core and app with XCTest and
XCUITest on the pinned Simulator, when a change touches the core or the ios
area (`just mobile --ios`); posted as `local/ios` ([ADR 0091](docs/adr/0091-mobile-ci-and-releases.md)).

**Area**: one of desktop, core, ios and android, from the one path table
`scripts/ci-areas.txt`; a change runs the jobs of the areas it touches, a
core change runs all of them, and files in no area (docs) run none
([ADR 0091](docs/adr/0091-mobile-ci-and-releases.md)).

**Takeover**: the Mac's local CI running a `local/android` job the PC left
stuck for 15 minutes, before the hosted fallback
([ADR 0091](docs/adr/0091-mobile-ci-and-releases.md)).

**Hosted mobile jobs**: the iOS job on a hosted `macos-26` runner and the
Android job on hosted `ubuntu-latest` with KVM; they run only as the
fallback, on Dependabot and fork PRs, and in release runs
([ADR 0091](docs/adr/0091-mobile-ci-and-releases.md)).

**Mobile release draft**: where an agent-started mobile release stops: the
build on TestFlight's internal testers, Play's internal track and a draft
GitHub Release. Everything users can install is the maintainer's step
([ADR 0091](docs/adr/0091-mobile-ci-and-releases.md)).
