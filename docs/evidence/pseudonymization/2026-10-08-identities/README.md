# Document-local identity review verification — 2026-10-08

Apple Silicon macOS. Implements [ADR 0018](../../../adr/0018-document-local-identity-review.md)
under the user's broad first-journey implementation request. Synthetic regression
and native smoke evidence, not a representative holdout or production qualification.

## Automated checks

- Workspace: 430 passing unit/integration/doc tests; other ignored qualification
  and platform/resource checks remain outside the ordinary run. See workspace-tests.txt.
- Strict workspace Clippy, formatting and diff whitespace checks pass.
- Serial locked release build passes; see release-build.txt.
- Eight new GPUI identity-workflow tests cover stage-without-editing, merging,
  category correction, manually assigned ownership, aliases-only copy legend,
  restoration of exact originals, homonym separation and reassignment, rescan,
  undo/redo, metadata-only Keep, shared-marker upgrade, pasted-token exclusion,
  alias collisions, legacy bulk acceptance and normalization-wide draft renaming.
  Live geometry and actual button clicks exercise dark/light themes at 1500×900
  and 640×480; the owner picker is clicked in both themes, preserving distinct
  person/contact identities. An editor regression verifies stale metadata
  checkpoints refuse changes, preserve source/selection, and emit only transactions.
- Core checks cover normalized spacing/case/quotes without losing original bytes
  or organization legal forms; two people sharing surname/initials remain separate
  suggestions. Labelled checksum-valid ОГРН/ОГРНИП use UTF-8 source byte ranges;
  invalid checksum, all-zero, unlabelled and cross-line examples are declined.
- Dedicated policy-history stress: 20,000 identities, 256 retained shared states,
  one edited override, and original immutable definitions shared rather than
  cloned for typing. See identity-storage.txt. This excludes editor snapshots,
  matcher/layout costs and allocator/hash overhead.
- The 2 MiB/20,000-occurrence matrix passes at 1, 1,000 and 20,000 groups. The
  matcher uses 409 / 23,184 / 453,840 bytes respectively; candidate vector capacity
  is 1,048,576 bytes. Occurrence tracking stores 20,000 spans with shared originals
  and 255 retained reversible deltas. Capacity numbers exclude allocator/hash
  overhead and do not measure whole-process peak memory.
- Cached offline GLiNER2 FP16 smoke: 8,640 bytes, 397 raw spans, 7.98 s in this
  debug run; active cancellation returned in 2.87 s, accepting no partial result.
  Installed-model automatic anonymization smoke also passes. These establish
  execution and cancellation, not detector accuracy. See stress-offline.txt.

## Native interaction

The synthetic input recreates full Russian names, initials in signatures, a
company, email, INN and ОГРНИП. The supplied user's document was not modified.
The app was run in a temporary bundle; prior paths-only session state was backed
up and restored afterward.

Observed in the native app: local staged scan highlighted eight occurrences
without changing source; the searchable Identities panel found the initials and
suggested the full-name identity; confirming the link and applying aliases produced
one PERSON_1 at all four name occurrences, including both signatures. ОГРНИП used
IDENTITY_1. Typing CLIENT_1 in the real alias field and clicking Rename updated all
four occurrences. Both themes and the editable field's selection were inspected.

A final native release run confirmed owner selection in both themes, keeping the
email identity separate from its person owner. Closing the panel and pressing
Undo removed ownership without changing the document or its dirty state. After
linking initials and applying aliases, Copy for AI was pasted into a new local
tab and saved as [native-copied.md](native-copied.md). Its bytes were checked for
four PERSON_1 name occurrences plus the alias-only EMAIL_1 → PERSON_1 legend,
with no original name, email or tax value from this synthetic fixture.

Earlier native automation reported noWindowsAvailable for some picker/scroll
interactions; those attempts are excluded from acceptance evidence. The picker
now binds its rendered action to ownership and validates the selected identity,
rather than reading a mutable menu-purpose flag at click time. Both final native
and GPUI button-click checks pass. Full accessibility/IME acceptance remains
pending; custom search inputs are not fully represented in the native AX tree.

## Boundaries

No numerical release targets or experimental removal are inferred from these
checks. Representative independently reviewed EN/RU legal/OCR holdout, residual
PII, false joins/splits, wrong owners, correction time, full accessibility/IME and
Windows/Linux runtime evidence remain required. The existing dense editor layout
bottleneck is unchanged. Saved/copied Markdown does not persist mappings; originals
and policy remain in live tab/editor undo memory. No return journey or deliberate
local-erasure guarantee is implemented.
