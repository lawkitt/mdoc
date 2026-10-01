# Document preparation

mdoc prepares local documents as editable Markdown for lawyers to use with AI
agents in other tools.

## Language

**Source document**:
The original document from which Markdown is extracted, such as a PDF or DOCX.

**Converted Markdown**:
The Markdown produced from a source document, which the lawyer can review and edit.

**OCR**:
Recognition of text from document images, used when usable text cannot be
extracted directly.

**Markdown handoff**:
The transfer of prepared Markdown to another tool, by saving a file or copying
the full text.

**Pseudonymization**:
Reviewable replacement of selected identifying information with consistent
placeholders while retaining useful relationships in the text. It does not
guarantee that the document's subjects cannot be identified.
_Avoid_: Guaranteed anonymization

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
