# Add reviewable anonymization with shared category markers

Status: accepted and implementation authorized with "ok, implement",
2026-10-06. Implemented locally; verification is in the linked design record.

The user agreed to an Anonymization mode alongside Pseudonymization that
replaces identifiers with shared category markers instead of numbered,
identity-linked tokens. For example, both Anna and Boris become PERSON.
This mode edits current Markdown with undo and uses existing Copy Markdown/Save
for handoff. It does not introduce a separate cleaned-copy workflow.

The purpose is identifier removal with less retained identity structure.
Contextual generalization is outside this initial direction. Shared markers do
not establish that a subject cannot be identified from the remaining document.
Undo retains prior text in the live editor; this is not local data erasure.

Round 2 confirmed reuse of the eight existing categories and local experimental
GLiNER2 detection with review safeguards, whole-source scanning, manual
selection and bulk acceptance. Markers are fixed: PERSON, ORG, EMAIL, PHONE,
ADDRESS, IDENTITY, TAX and BANK. Anonymization omits custom replacement and
identity-linking controls.

Mode selection changes pending proposals rather than rewriting accepted text.
Known accepted pseudonyms in the live document can become reviewable conversion
candidates; reopened documents have no persisted mappings and require manual
selection for such tokens.

The user amended the toolbar choice to an anonymous-person icon button and
Anonymize as the default. Keep a compact mode menu and per-document mode state,
with an action-specific tooltip and accessible name. Anonymization should be
substantially simpler than Pseudonymization for quick PII removal and Markdown
copying.

The user confirmed the fast path: the primary Anonymize action performs a local
scan and applies eligible detected PII and known accepted live-document
pseudonyms in one undoable batch. Preserve Keep exclusions and reject failed,
partial, cancelled, stale or mode-switched results. Secondary Review retains
manual additions, Accept/Keep and bulk acceptance. Switching modes alone does
not apply a batch; Pseudonymize keeps its existing explicit review workflow.

Copy Markdown remains a separate explicit action. Completion reports actual
replacement occurrences and reminds the user to check remaining text; no
anonymous-document claim is made. Default new documents to Anonymize without
persisting this mode as an application preference. Restore it to Anonymize on
replacement of document identity, and retain mode across tab switches.

[Design and verification](../design/anonymization.md) records all eight confirmed
decisions, lifecycle/source invariants and validation limits. The frontier is
empty. ADR 0002 continues to govern Pseudonymization; shared detection remains
experimental and its qualification gaps are unchanged.
