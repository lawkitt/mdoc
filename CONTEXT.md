# Document preparation glossary

mdoc prepares local documents as editable Markdown for external AI tools.
Behavior and rationale live in [ADRs](docs/adr/); this file defines shared terms.

## Documents and handoff

**Source document**: the original PDF, DOCX or other file from which Markdown is extracted.

**Converted Markdown**: editable Markdown extracted from a source document.

**Source preview**: visual reference to the retained original, before Markdown
edits. DOCX rendering is approximate. UI label: **Source** (was "Original"
until ADR 0048; "Original" now names the pill's pre-pseudonymization view).

**Page tone**: how Original pages are painted: *original* paper or *themed*
(paper and ink remapped to the dark theme). Follows the app theme unless
overridden in the Original header ([ADR 0035](docs/adr/0035-dark-original-pages.md)).
Avoid "invert", which suggests a plain colour flip.

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
UI label: **Paste AI Response**.

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

**Document header**: the title-bar row above a document: breadcrumb, Source,
panel toggle, ⋯ and the primary action.

**Primary action**: the header's one filled button, which follows the
document's state: Pseudonymize, scanning + Cancel, Apply N, Copy for AI.

**Case section**: a collapsible sidebar group of the tabs of one case; tabs
without a case sit under "Open".

## Models and processing

**OCR**: recognize text from document images when direct extraction is insufficient.

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

**Case**: a named, saved mapping shared by a group of tabs; shown as a tab group
in the sidebar. One case per document. Avoid "matter" and "project".

**Case key**: the 256-bit key encrypting saved cases, held by the OS (macOS
Keychain, Windows DPAPI, Linux Secret Service). Read only for an action that
needs it ([ADR 0043](docs/adr/0043-deferred-keychain-access.md)). Signed
macOS builds move it to the data protection keychain, which never asks
([ADR 0045](docs/adr/0045-prompt-free-case-key.md)).

**Variant proposal**: a suggested link between a shorter or inflected form
("Ms Raman", "Raman's", "Рамана") and a known identity. Always reviewed, never
linked automatically; a first name alone is not proposed
([ADR 0053](docs/adr/0053-short-and-inflected-name-variants.md)).

**Merge hint**: a dismissible suggestion in the Replacements panel to link two
identities that very likely name the same subject. Never applied automatically;
Merge is one undo step. No score is shown.

**Quiet probe**: the launch read of the case key with per-query "no UI"; it
either opens cases or leaves them locked, and never shows a system dialog.

**Dev key**: the case key of unsigned, ad-hoc and debug builds, under its own
Keychain service so it never collides with the release key.

**Keychain explainer**: the dialog shown before the first case-key access (and
after a denial), with an animated preview of the macOS password dialog.

**Pseudonymize Clipboard**: replace the clipboard text's known case names with
their aliases, without detection. Unknown names stay; it is not a scan.

**Before / after view**: read-only, line-paired view of original wording beside
pseudonymized text. Live: it follows every applied-replacement change
(ADR 0042). Avoid "diff", which suggests arbitrary edits.

**View switcher**: the floating Edit · Original · Pseudonymized · Side by side
control shown while replacements are applied ([ADR 0046](docs/adr/0046-workspace-polish-from-anoni-review.md)).

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
mention, which stays whole. UI label: **Replace** ([ADR 0024](docs/adr/0024-replace-custom-selection.md)).

**Other category**: manual-only neutral category with a `REDACTED_n` alias;
the detector never emits it.

**Replacement chip**: mention highlight with a category-tinted underline;
light tint while proposed, alias chip once applied (ADR 0048).

**Entity checkbox**: the panel's tick per entity: ticked = will be replaced,
unticked = Keep original, – = mixed mentions. Ticking records intent; text
changes only on Apply.

**Mention bar**: slim bar above a clicked mention: Replace checkbox, alias,
category, This one | All scope. Replaced the replacement popup (ADR 0048);
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

**Document view**: one tab's editor, source preview, search, import/OCR and
pseudonymization state. Code: `document_view::DocumentView`. Avoid calling this workspace.

## Releases

**Release commit**: the commit made by `just release` that sets the version in
`Cargo.toml`, `Cargo.lock` and `Info.plist` (and, for stable versions, names
the CHANGELOG section); its tag must match that version ([ADR 0037](docs/adr/0037-ci-and-release-pipeline.md)).

**Pre-release**: a version with a SemVer suffix such as `-beta.1` or `-rc.2`;
marked pre-release on GitHub, built without an `.msi`, and offered by the
update check to pre-release builds or when "Include pre-releases" is on.

**Shipped target**: a platform with release artifacts — Apple Silicon macOS
and Windows x64. Other platforms build from source, unsupported.

**Update check**: detection-only comparison of the running version with the
newest GitHub Release; shows Settings → Updates and never installs ([ADR 0038](docs/adr/0038-update-check.md)).

**Release gate**: checks a release must pass on every shipped target: the OCR
smoke test before packaging, and the packaged-app self-test (`just
smoke-package`) on the built artifact itself.

**Must-fix / should-fix**: roadmap tiers for a milestone. Stable waits only on
must-fix items; unfinished should-fix items move to the next milestone. Distinct
from a release gate, which is an automated check on every release
([ADR 0051](docs/adr/0051-roadmap-structure-and-stable-scope.md)).

**Ordinary legal document**: the interactive performance target, about 30 pages
or 135 KB of Markdown. The 2 MiB / 20,000-mention workload is the separate
*stress tier* ([ADR 0049](docs/adr/0049-measured-stabilization-and-architecture-review.md)).
