# Document preparation glossary

mdoc prepares local documents as editable Markdown for external AI tools.
A term stays here only if it fixes a word users see, forbids a synonym, or
names a domain idea with no type in the code; rules live in `docs/specs/`.
UI strings use US English ("unrecognized", "license"); identifiers and file
names keep their spelling.

## Documents and handoff

**Source document**: the original PDF, DOCX or other file from which Markdown
is extracted.

**Converted Markdown**: editable Markdown extracted from a source document,
from native text, OCR or both.

**Source preview**: the visual reference to the retained original. UI label:
**Source**.
"Original" names the view switcher's pre-pseudonymization view.

**Page tone**: how Original pages are painted: *original* paper or *themed*
(paper and ink remapped to the dark theme). Avoid "invert".

**Markdown handoff**: saving or copying prepared Markdown to another tool;
**Copy Markdown** copies the source exactly.

**Copy for AI**: copying the full current Markdown with an alias-only note
asking the AI to keep placeholders exactly.

**Restore**: replacing aliases in a pasted AI reply with the primary
wordings of its mapping. UI label: **Paste AI Reply** (Title Case in menus).

**Reply box**: the Share panel's "AI reply" field: a paste restores at once
and is saved as an **AI reply** once it settles.

**Reply view**: the view switcher's read-only Reply segment, restoring the
box's reply live.

**Reply page**: a saved AI reply, read-only, restored afresh each time it
shows.

**Notes**: a Document's private text, never handed off with it.

**Unresolved token**: an alias-shaped token in AI output that matches no
identity of the mapping, such as an invented `PERSON_9`.

**Home**: the page shown with no document: drop zone, quick actions and the
**Continue** strip.

**Command palette**: the ⌘K search over actions, titles and Project names.

**Find and Replace**: the find bar and its Replace row, searching **visible
text** (syntax hidden); a match that **crosses formatting** spans several
source ranges.

**Document header**: the title-bar row above a document.

**Project tree**: the sidebar's stable tree of Projects, each expandable to its documents, and Unfiled; desktop's only document navigation.

## Models and processing

**OCR**: recognizing text in page images.

**Recognition languages**: the languages OCR is asked to read; each engine
maps them to models or packs. Avoid "OCR language".

**Language signal**: what suggests a document's recognition languages: its
text layer, file name, then keyboard input source.

**Qualified language**: a document language with its own OCR model,
spellcheck dictionary and PII qualification; English and Russian today.

**Model bundle**: pinned, compatible model artifacts plus the tokenizer or
dictionary they need.

**Setup consent card**: the inline first-use prompt naming a missing model
and its download size. Avoid "setup wizard".

**State card**: the centered Markdown-pane card of an unconverted document.

**Model comparison**: isolated sequential QA runs on one captured input; each
keeps a **configuration snapshot** of its model and settings.

**Recognition output**: OCR text and evidence before fusion with native
text.

**Scan status**: the spinner and timed stage label beside Pseudonymize.

**Page artifact**: a non-text mark OCR misreads: seal, signature, barcode,
watermark or repeated header.

## Pseudonymization and review

**Pseudonymization**: reviewable replacement of identifying information with
consistent aliases that keep useful relationships. Avoid "anonymization".

**Replacement mapping**: the local association between originals and aliases,
kept apart from the Markdown handed off.

**Vault**: mdoc's encrypted local store of Projects, Documents, AI Replies and Source Originals, under opaque names.

**Unfiled**: a Vault document in no Project, with its own mapping and replies.

**AI reply**: a saved pseudonymized AI response of a Project or Unfiled document, shown restored and promotable to a Document.

**Project**: a named, encrypted workspace sharing one replacement mapping across its Documents and AI Replies. Avoid "case" and "matter".

**Vault key**: the 256-bit OS-held key encrypting the Vault. The **quiet
probe** reads it at launch without dialogs; the **dev key** separates
unsigned/debug macOS builds. The **recovery key** is its text form, behind
Show Recovery Key.

**AI project**: the one Project the desktop MCP connector serves, chosen in mdoc.

**AI activity**: the Project page's Vault record of which AI app read or
delivered what, and when.

**Companion**: the AI connector preview's floating macOS window, never
focused, toggled from the menu bar in place of the shelf's popup.

**Keychain explainer**: the dialog shown before the first Vault key access
and after a denial.

**Variant proposal**: a suggested form ("Ms Raman", "Рамана", "Мариша") of a
known identity.

**Propagation**: proposing an applied or marked name's forms elsewhere in the
document.

**Case number**: the category of a claim or case number after a label (Claim
No, Дело №), aliased `CASE_1`; not a **Project**.

**Always and Never lists**: wordings always or never proposed, per Project
and user-wide.

**Not a name**: unmarking with no trace.

**Web address**: the UI label of the URL category (`Category::Url`, aliases
`URL_1`): an identifying profile, private site or share link.

**Default categories**: Settings › Detection › Categories to detect.

**Flaw**: a miss, false hit or wrong boundary against the
[annotation guide](tests/fixtures/recognition/GUIDE.md). A **flaw class** is
the flaws one fix cures.

**Reference platform**: the one platform, hosted Windows, whose raw model
output the recognition snapshots record. The **detection gate**
runs the model over every fixture.

**Hunter, curator, fixer**: the flaw hunt's roles: writing documents and
reporting flaws, auditing gold and landing fixtures, fixing a flaw class.

**Holdout**: synthetic recognition documents kept in the private
`lawkitt/mdoc-holdout` to catch overfitting.

**Real set**: about 50 real public documents with frozen gold, kept in
`lawkitt/mdoc-probes`; recognition targets are scored on it.

**Probe**: a real public document in `lawkitt/mdoc-probes` run through mdoc
to find flaw classes; never a fixture. The **judge** is the agent that lists
its flaws beside mdoc's proposals.

**Outside set**: a public document set from outside mdoc, pinned by a
manifest with a role: tune, held-out or regression
([checklist](tools/sets/README.md)).

**Decision history**: the document's list of pseudonymization decisions with
times. UI label: **What you did**.

**Pseudonymize Clipboard**: replacing known Project names in the clipboard
with their aliases, without detection. **Restore Clipboard**: the reverse, to
each identity's primary wording.

**Shelf**: the macOS menu bar item's panel of pseudonymized documents and
clipboard actions. Avoid "tray".

**Ready document**: its text matches the fingerprint saved with no proposals
left after a scan (below the phones' floor, every name decided by hand) and
holds no name its Project knows.

**Side by side view**: the read-only, line-paired view of original wording
beside pseudonymized text. Avoid "diff".

**View switcher**: the floating Edit · Original · Pseudonymized · Side by side
(· Reply) control.

**Crash report**: a local, masked panic record the user may preview and send.

**Primary wording**: the variant an identity restores to.

**Placeholder / alias**: a stable replacement token such as `PERSON_1`;
generated by mdoc or a **custom alias** the user chose.

**Identity**: an entity of a mapping, with a stable id, a neutral alias and
original variants. UI label: **Identities** on the phones.

**Occurrence assignment**: an identity override for one mention, separating
homonyms.

**Sameness**: mentions refer to one identity. **Ownership**: a contact,
address or identifier belongs to a person or organization. **Affiliation**: a
person represents an organization.

**Proposed mapping**: staged identity and alias changes until Apply.

**Review candidate**: a detected span proposed for replacement.

**Replacements panel**: the searchable overview beside the editor, grouped by
entity.

**Triage focus**: focus moving to the next undecided mention after a
decision.

**Scope**: This mention, Same wording or Entire entity; the reach of one
alias, category, Apply, Undo or Keep action.

**Manual addition**: selected missed text made a proposal. UI label:
**Replace · Person ▾**, a split pill.

**Other category**: the manual-only neutral category with `REDACTED_n`
aliases.

**Undo replacement**: restoring applied text as a proposal keeping its alias.
**Keep original**: restoring the original text as a Keep decision.

**Close review**: hiding candidate highlights while keeping edits; it does not
establish safe sharing.

## Spelling

**Spellcheck**: offline EN/RU misspelling flags in Converted Markdown, mainly
for conversion and OCR errors.

**Spell exclusion**: a byte range never dictionary-checked, supplied by the
host.

**Mixed-script word**: a letters-only word combining Latin and Cyrillic
letters.

**User dictionary**: words added with "Add to dictionary".

**Ignored word**: a word skipped in one document for the session.

**Legal word list**: curated legal vocabulary beside the base EN/RU
dictionaries, never names.

## Application structure

**Empty page**: the immediately writable empty Markdown view, also a
file-drop zone.

**Rail** and **hover reveal**: the collapsed sidebar's icon strip, and the
Project tree that slides out over the editor on hover.

**Back/Forward**: per-window history of the documents, Home, Project pages
and Reply pages viewed.

**Region**: a part of the window that F6 / ⇧F6 move focus between.

## Code structure

**Owner**: the one place that holds and changes a piece of state, with
private fields. Avoid Manager, Controller or Service in its name.

**Transition**: a named, synchronous, GPUI-free owner method changing its
state for one input; an **effect** is what it asks its GPUI view to do.

## Releases and CI

**Shipped target**: a platform with release artifacts: Apple Silicon macOS and
Windows x64.

**Update check**: the detection-only comparison of the running version with
the newest GitHub Release.

**Release gate**: an automated check a release passes on every shipped
target, such as the packaged-app self-test.

**Release candidate (RC)**: a `-rc.N` pre-release that a minor's platform
acceptance runs on.

**Platform acceptance**: the manual checklist run on an installed release on
each shipped target; each row is *Pass*, *Fail* or *Unavailable*.

**Smoke run**: a short install-and-use acceptance check on each shipped
target.

**Ordinary legal document**: the interactive performance target, about 30
pages or 135 KB of Markdown.

**Fast tier**: what every PR head runs on hosted Ubuntu.

**Infrastructure failure**: a hosted CI job that ended without a result, not a
test failure.

**Timing test**: a test of speed, not of correctness.

**Mobile release draft**: where an agent-started mobile release stops, before
anything users can install.

**Beta build** (iOS): a TestFlight build tagged `ios-v<x.y.z>-rc.N`; the
plain `ios-v<x.y.z>` tag marks the build sent to App Store review.

**Lawyers group**: the one external TestFlight group, and Play's same-named
closed testing track.

**Never-client-files line**: the opening sentence of every beta description:
shareable documents, never client files.

## Mobile apps

**Mobile core**: the GUI-free Rust crates the iOS and Android apps call.

**Review-first flow**: the mobile journey: scan or import, recognize, review
replacements, share to an AI app, restore the reply.

**Restore with mdoc**: the mobile share-sheet action that opens an AI reply
from another app restored.

**Rendered view**: how mobile apps show Markdown: markers hidden, blocks drawn
over the unchanged source. **Show Markdown** toggles to the source.

**Document menu** (mobile): the menu under the document's title, the one
place for its commands.

**Adjust Words** (mobile): correcting a found name's span with handles in the
text; "Fix the words" in code (`wordsFix`, `fix_words`). Not "rename",
which changes the alias.

**Edit mode** (mobile): the document screen with the keyboard, the editing
toolbar and editable text. Not "edit view".

**Formatting keyboard** (mobile): the panel the editing toolbar's Aa key swaps
in for the system keyboard. Not "format menu".

**Table controls** (mobile): what the editing toolbar shows with the caret in
a table cell.

**Panes row** (mobile): the segmented row under the navigation bar: Text,
Original and Reply on iPhone. Not "tabs".

**Device floor**: the least physical RAM on which a phone gets the GLiNER2
detector.

**Hand-off bar**: the bar under a reviewed document with Share, Copy and Paste
reply.

**Identities drawer** (mobile): the open document's names in a panel from the
right edge on iPhone, a standing column on iPad.

**Peek**: the sliver of the previous or next review card at the review bar's
edges.

**Mentions chip**: "1 of 3 ›" on the review card when an identity has more
than one mention.

**Full view** (mobile review): the review bar dragged up, one row per mention
of the identity under review. Also "expanded bar".

**Hint line**: a one-line tip in the review bar or Identities about a
gesture.

**Drag-to-merge** (mobile Identities): dragging one name's row onto another's
to make them one.

**Focus** (mobile review): stepping limited to one identity or one category.
