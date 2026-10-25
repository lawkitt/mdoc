# Document-local identity review for external AI analysis

Status: Accepted, 2026-10-08. Superseded in part by
[ADR 0019](0019-simpler-pseudonymization-ui.md) and later UI ADRs (review
surfaces, copy actions) and [ADR 0041](0041-cases-and-ai-response-restoration.md)
(cases persist mappings across documents). Amended by
[ADR 0068](0068-short-and-inflected-name-variants.md) (surname forms are
proposed).

## Decision

- **Identities**: stable document-local identities with neutral category aliases
  (`PERSON_1`, `ORG_1`, `EMAIL_1`). Alias names are independent of identity IDs.
  Every occurrence keeps its exact original and its assignment. Merges and splits
  never renumber unrelated aliases; alias-wide changes affect tracked
  occurrences only.
- **Automatic sharing**: exact values, or values equal after case, spacing and
  typographic-quote normalization, of the same category share an identity,
  preserving original bytes and company legal forms. This is a lexical default:
  identical names can be split by occurrence.
- **Suggestions, never automatic**: surname plus two initials discovers
  candidates; initials and conservative Russian inflections suggest person links
  that always need confirmation. Title + surname, a bare surname and its
  Russian case forms are proposed as variants of their own and offered under
  Same as… (ADR 0068); none of these, OCR resemblance or proximity establishes
  identity or ownership. Organization shorthand, OCR
  variants, affiliation and party-block ownership need evidence first.
- **Corrections**: rename, category correction (also repairing applied
  aliases), assignment, merge, and separating one occurrence or all matching
  originals. Each correction and Keep is one step in the editor's undo history;
  rescans keep confirmed assignments.
- **Ownership**: contact details, addresses and identifiers can have a manually
  confirmed person or organization owner, separate from identity equivalence.
- **Structured identifiers**: labelled checksum-valid ОГРН and ОГРНИП are
  identifiers, not bank numbers.

## Safeguards

Without a case, mappings, originals and relationship policy live only in the tab
and its undo history; saved Markdown and copied output carry none. Existing
source tokens and aliases are reserved; custom alias collisions are rejected.
Pasted lookalikes never gain provenance. A stale revision or invalid span
refuses the operation. Policy-only staging creates a history checkpoint without
a text-change event. Undo journals share immutable policy and originals; typing
never clones the identity index; deliberate changes are copy-on-write. Lists are
virtualized and context is built only for the selected identity.

## Consequences

Realistic fake names are not offered: they are harder to restore and collide
with real names. Recognition and relationship resolution are not qualified for
production by this workflow.

References: [full text before ADR 0052](https://github.com/lawkitt/mdoc/blob/f368e436cd2d3e70789d51879b9f54f379900e1e/docs/adr/0018-document-local-identity-review.md),
[verification](https://github.com/lawkitt/mdoc/blob/f368e436cd2d3e70789d51879b9f54f379900e1e/docs/evidence/pseudonymization/2026-10-08-identities/README.md).
