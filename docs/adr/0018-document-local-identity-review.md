# Document-local identity review for external AI analysis

Status: authorized and implemented experimentally, 2026-10-08. The user's
“Proceed implement everything else” selects the recommended first-journey product
workflow from the preceding brainstorming, following the selected Presidio reuse.
Production qualification remains open.

## Decision

Prepare Markdown for analysis in an external AI app, without a return pipeline.
Use stable document-local identities with neutral category aliases (PERSON_1,
ORG_1, EMAIL_1, etc.). Alias names are independent of identity IDs. Every occurrence
retains its exact original and its assignment. Merges and splits never renumber
unrelated aliases, and alias-wide changes affect tracked occurrences only.

Automatically share an identity for full exact or case/spacing/typographic-quote
normalized values of the same category, preserving original bytes and company
legal forms. This is a lexical default, not proof of sameness: repeated identical
names can be split by occurrence. Matching surname plus two initials discovers
additional candidates; initials and conservative Russian inflections suggest
possible person links and always need confirmation. A matching surname alone,
OCR resemblance, and proximity to a name do not establish identity or ownership.
Defined organization shorthand, OCR variants, affiliation, and automatic party-
block ownership need separate evidence before automatic association is added.

An optional Identities panel offers search, variants, contextual occurrences,
rename, category correction, assignment, whole-identity merge, and separation of
one occurrence or all matching originals. Inline candidate and applied popups
open these controls directly. Review proposed mappings before applying aliases;
existing shared-marker anonymization can also be upgraded directly from tracked
originals. Application is one undoable batch. Each subsequent correction and Keep
shares the editor's undo/redo history; rescans preserve confirmed assignments.

Contact details, addresses and identifiers can have a manually confirmed person
or organization owner. Ownership is separate from identity equivalence. “Copy for
AI” copies current full Markdown with an optional alias-only relationship legend,
when detected pending mentions are resolved. Ordinary “Copy Markdown” retains its
existing raw-source behavior. Neither action asserts that all sensitive values
were found. The experimental notice and final full-source review remain visible.

## Lifetime and safeguards

Mappings, originals and relationship policy live only in the current tab, including
its editor undo history. Closing/reopening, saved Markdown and copied output do
not persist mappings. Existing source tokens and aliases are reserved; custom
alias collisions are rejected. Pasted lookalikes never acquire occurrence
provenance. A stale revision or invalid span refuses the operation. Policy-only
staging creates an editor history checkpoint without emitting a text-change event.

Undo journals share immutable identity policy and occurrence originals; typing
does not clone a complete identity index. Deliberate mapping changes use copy-on-
write policy. Lists are virtualized and context is built only for the selected
identity. This does not remove the existing dense-source editor layout bottleneck.

## Consequences and qualification

The screenshot's full name and misclassified initials can be assigned to one
PERSON alias without restoring and rescanning the document. A category correction
also repairs already-applied tracked markers. Labelled checksum-valid ОГРН and
ОГРНИП now receive structured identifier evidence, rather than a bank category.
This is a separately authorized expansion beyond ADR 0017's frozen three-rule reuse.

This delivers the first-journey review workflow; it does not qualify automatic
recognition or relationship resolution for production. Required next evidence:
independently reviewed representative EN/RU legal/OCR holdout, false joins/splits,
wrong owners, residual PII and category/boundary misses, correction time, explicit
numerical release gates, accessibility/IME and Windows/Linux runtime acceptance,
and realistic editor/resource budgets. The return journey, persistent matter maps
and deliberate local erasure remain separate future decisions.

Verification and native interaction evidence are recorded in
[identity-review verification](../evidence/pseudonymization/2026-10-08-identities/README.md).
