# Document preparation glossary

mdoc prepares local documents as editable Markdown for external AI tools.
Behavior and rationale live in [ADRs](docs/adr/); this file defines shared terms.

## Documents and handoff

**Source document**: the original PDF, DOCX or other file from which Markdown is extracted.

**Converted Markdown**: editable Markdown extracted from a source document.

**Source preview**: visual reference to the retained original, before Markdown
edits. DOCX rendering is approximate. UI label: **Original**.

**Page tone**: how Original pages are painted: *original* paper or *themed*
(paper and ink remapped to the dark theme). Follows the app theme unless
overridden in the Original header ([ADR 0035](docs/adr/0035-dark-original-pages.md)).
Avoid "invert", which suggests a plain colour flip.

**Markdown handoff**: saving or copying prepared Markdown to another tool.
**Copy Markdown** copies the full current source exactly, including unsaved edits
and pending proposals; it does not apply replacements or add a relationship legend.

**AI analysis handoff**: prepare Markdown for external AI analysis without
bringing the response back. This is the first qualification journey ([ADR 0016](docs/adr/0016-pii-production-qualification-scope.md)).

**AI analysis return journey**: bring an external AI response back into mdoc.
Response import and identity restoration remain deferred.

**Multi-file opening**: open several files for individual reading and lazy conversion.

**Batch conversion**: process an explicitly selected group without opening each
file individually. This workflow remains deferred; avoid calling multi-file opening batch conversion.

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
separate from the Markdown handed off. Mappings and originals live only in the
open document; Save/Copy/restart do not serialize them.

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
category; exact repeats are proposed together. UI label: **Replace** ([ADR 0024](docs/adr/0024-replace-custom-selection.md)).

**Other category**: manual-only neutral category with a `REDACTED_n` alias;
the detector never emits it.

**Replacement chip**: rounded mention highlight: gold proposed, blue applied.
Motion and scoped popup presentation follow [ADR 0032](docs/adr/0032-replacement-popup-redesign.md).

**Scope outline**: display-only ring around mentions affected by the selected scope.

**Undo replacement**: restore applied text as a proposal retaining its alias.
**Keep original**: restore original text and record a Keep decision. Both follow
text undo/redo ([ADR 0023](docs/adr/0023-pseudonymization-ui-polish.md)).

**Close review**: hide candidate highlights while retaining edits, clickable
applied highlights and live restoration provenance. It does not establish safe sharing.

## Application structure

**Empty page**: immediately writable empty Markdown view with Open files,
supported formats, multi-file hint and shortcut; also a file-drop zone ([ADR 0028](docs/adr/0028-empty-page-and-file-drop.md)).

**Workspace**: one window's shell, sidebar, toolbar, Settings and tabs,
including unopened restored records. Code: `workspace::Workspace`.

**Tab**: sidebar entry owning a document view once initialized; restored tabs
allocate their document view when activated.

**Document view**: one tab's editor, source preview, search, import/OCR and
pseudonymization state. Code: `document_view::DocumentView`. Avoid calling this workspace.
