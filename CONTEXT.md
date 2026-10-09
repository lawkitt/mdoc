# Document preparation

mdoc prepares local documents as editable Markdown for lawyers to use with AI
agents in other tools.

## Language

**Source document**:
The original document from which Markdown is extracted, such as a PDF or DOCX.

**Converted Markdown**:
The Markdown produced from a source document, which the lawyer can review and edit.

**Source preview**:
A visual reference to the retained source document, before Markdown edits.
DOCX preview is an approximate rendering and may have fidelity limitations.
_UI label_: Original

**OCR**:
Recognition of text from document images, used when usable text cannot be
extracted directly.

**Markdown handoff**:
The transfer of prepared Markdown to another tool, by saving a file or copying
the full text.

**AI analysis handoff**:
Prepare Markdown in mdoc for analysis in an external AI tool, without bringing
the response back into mdoc. This is the initial pseudonymization production
journey selected on 2026-10-08.

**AI analysis return journey**:
Prepare Markdown for analysis in an external AI tool and bring its response
back into mdoc. This is a later product journey; response import and identity
restoration behavior have not yet been designed or implemented.

**Pseudonymization**:
Reviewable replacement of selected identifying information with consistent
placeholders while retaining useful relationships in the text. It does not
guarantee that the document's subjects cannot be identified. It is the only PII
replacement behavior ([ADR 0022](docs/adr/0022-remove-anonymization-mode.md)):
scanning proposes, explicit Apply replaces as one undo step. Applied fields stay
highlighted (teal; proposals are amber) and clickable. Apply also works per
popup scope. **Undo replacement** returns applied text to a proposal with the
same alias; **Keep original** reverts and records a Keep decision. Both travel
with text undo/redo ([ADR 0023](docs/adr/0023-pseudonymization-ui-polish.md)). Originals live only in the open document; Save does not serialize them.
_Avoid_: Guaranteed anonymization, Anonymize, shared category markers

**Model bundle**:
A pinned set of mutually compatible model artifacts, including the required
dictionary or tokenizer, offered as one selectable configuration. Availability
and installation do not establish recognition or detection quality.

**Setup consent card**:
The inline prompt shown where OCR or Pseudonymize is first needed while its
selected model bundle is missing. It names the bundle and total download size;
**Download & continue** installs it and resumes the task
([ADR 0026](docs/adr/0026-first-use-model-setup.md)).
_Avoid_: automatic setup, setup wizard

**State card**:
The centred card in the Markdown pane for a document not yet converted
(ready, waiting, converting, needs recognition, failed). It changes in place
and hosts the setup consent card for OCR ([ADR 0026](docs/adr/0026-first-use-model-setup.md)).

**Configuration snapshot**:
The model identity and settings captured for one processing run. Later changes
to application defaults do not change the configuration attributed to that run.

**Model comparison**:
Isolated, sequential QA runs against the same captured input with separate
read-only results. Comparison overrides do not edit the document or change
application defaults.

**Recognition output**:
Text and recognition evidence produced by the OCR model before native-text fusion
and final Markdown preparation. Model confidence is not measured accuracy.

**Prepared Markdown**:
The converter's final Markdown output, which may combine native extraction and
OCR recognition. In model comparisons it is shown separately from recognition
output and does not replace the working document.

**Replacement mapping**:
The local association between original identifying information and its
placeholders, separate from the Markdown being handed off.

**Batch conversion**:
Conversion of an explicitly selected group of source documents without requiring
the lawyer to open each one individually for processing.
_Avoid_: Bulk Open, multi-file opening

**Review candidate**:
A detected span of identifying information proposed for replacement, which the
lawyer can accept or decline.

**Close review**:
Leave pseudonymization review and hide candidate highlights while retaining
accepted Markdown edits, clickable applied highlights and the live document's
restoration provenance. It does
not establish that the document is safe to share.

**Manual addition**:
Text the lawyer selects and turns into a proposed replacement after
Pseudonymize, when detection missed it. All exact repeats are proposed together
with a guessed, correctable category
([ADR 0024](docs/adr/0024-replace-custom-selection.md)). Code: `Review::add_manual`.
_UI label_: Replace

**Scope outline**:
A thin outline in the editor around the mentions a selected panel group or
popup scope chip would affect; display only
([ADR 0025](docs/adr/0025-review-refinements.md)).

**Other category**:
The neutral category for manually added text with no recognisable shape;
placeholder `REDACTED_n`. Manual additions only; the detector never emits it.

**Mention**:
One occurrence of identifying information in the Markdown source. Exact repeated
mentions can be reviewed together; variants are linked by the lawyer.

**Variant**:
One exact original wording, such as "Павлова М.С.", whose pending mentions are
found together. Each variant belongs to an identity; linking variants makes
them share one alias. Code: `mdoc_pii::Variant`.

**Placeholder**:
A stable replacement token, such as PERSON_1, associated with selected identifying
information while preserving useful references within a document.

**Multi-file opening**:
Opening several selected files together for individual reading and conversion.
It does not imply processing and saving the entire group automatically.
_Avoid_: Batch conversion

Document-local identity review (ADR 0018): an **identity** owns a stable neutral
alias and original variants; an **occurrence assignment** can override a variant's
identity to separate homonyms. **Sameness** joins mentions into one identity;
**ownership** links a contact/address/identifier to a person or organization.
**Proposed mapping** changes are staged against originals until Apply replacements;
tracked replacements remain correctable afterward. The **Replacements panel**
(ADRs 0019 and [0020](docs/adr/0020-direct-replacement-workspace.md)) is the searchable
overview; selecting a row reveals the exact highlighted occurrence and its shared
word popup. The popup provides direct alias choices and scoped sameness/category
corrections, with separate ownership and Keep/Restore actions. One **Copy Markdown**
action copies current full source exactly, including with pending proposals;
it adds no relationship legend and never implicitly applies replacements.
Owner assignments remain local. The panel stays vertical directly right of the
Markdown editor (before Original, ADR 0023) with compact rows and full
document-area height. One popup **scope** (This mention / Same wording / Entire
entity) governs alias, category, Apply, Undo and Keep actions. Its width adapts to the window; the
Original preview switches panes when the remaining document area is narrow,
keeping the word popup available beside the panel.
**Same wording** affects matching originals still assigned to the selected entity;
**Entire entity** also includes its other variants. **Generated aliases** are
allocated by mdoc; **custom aliases** preserve deliberate user names on category
correction, regardless of their spelling.
These mappings and originals are live-tab memory, not saved restoration metadata.

**Empty page**:
The centered start view of a Markdown tab with empty content: Open files…,
supported formats, the multi-file hint and ⌘O. Non-blocking — typing starts
writing. Also the whole-window file drop zone while shown (ADR 0028).

**Workspace**:
The window shell: the document sidebar, toolbar, settings and the tabs it owns,
including unopened restored records. One per window. In code `Workspace`
(currently `tabs::Tabs`; renamed per [ADR 0029](docs/adr/0029-codebase-cleanup-charter.md)).

**Tab**:
One sidebar entry. An initialized tab owns one document view; an unopened
restored tab allocates nothing until activated.

**Document view**:
Everything shown for one tab's document: Markdown editor, source preview,
search, import/OCR state and pseudonymization review. In code `DocumentView`
(currently `Workspace` in `src/main.rs`; renamed per ADR 0029).
_Avoid_: calling the per-document view a workspace
