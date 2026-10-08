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
guarantee that the document's subjects cannot be identified.
_Avoid_: Guaranteed anonymization

**Anonymization mode**:
Identifier replacement using shared category
markers, such as PERSON, instead of identity-linked numbered placeholders.
The default anonymous-person icon action scans locally and applies replacements
to current Markdown as one undoable batch; manual review remains available.
Copy Markdown is a separate action. This removes identity distinctions
from replaced mentions but does not establish that the subjects cannot be
identified from remaining context. Accepted behavior is recorded in
[ADR 0013](docs/adr/0013-reviewable-anonymization.md). Applied fields stay
highlighted and clickable in both modes; the popup restores this occurrence or
all occurrences of the same immediate prior value. Restore creates Keep decisions
and travels with text undo/redo. Originals live only in the open document; Save
does not serialize them. See [ADR 0014](docs/adr/0014-highlighted-pii-replacements.md).

**Model bundle**:
A pinned set of mutually compatible model artifacts, including the required
dictionary or tokenizer, offered as one selectable configuration. Availability
and installation do not establish recognition or detection quality.

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

**Mention**:
One occurrence of identifying information in the Markdown source. Exact repeated
mentions can be reviewed together; variants are linked by the lawyer.

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
Owner assignments remain local. The panel stays vertical at the right edge with
compact rows and full document-area height. Its width adapts to the window; the
Original preview switches panes when the remaining document area is narrow,
keeping the word popup available beside the panel.
**Same wording** affects matching originals still assigned to the selected entity;
**Entire entity** also includes its other variants. **Generated aliases** are
allocated by mdoc; **custom aliases** preserve deliberate user names on category
correction, regardless of their spelling.
These mappings and originals are live-tab memory, not saved restoration metadata.
