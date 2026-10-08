# Highlighted anonymizer replacements and restoration — 2026-10-07

Status: confirmed and implemented, 2026-10-07. Q15 settled the final recognizer
selection; all agreed restoration and subsystem-refactoring work is implemented.
See the [verification report](../evidence/pseudonymization/2026-10-07-restoration/README.md).

## Confirmed request

Keep the default local scan-and-replace action. After replacement, leave replaced
fields highlighted so the user can see what changed, click a field to inspect a
popup, and restore its value. Discuss maintainability/refactoring alongside this
behavior. Existing fixed shared category markers remain the starting contract.

## Evidence at interview start

- `Review::refresh` finds pending mentions by their original text. Accepted
  replacements no longer match these spans.
- `Workspace::sync_annotations` exposes pending candidates only while review is
  open and either Pseudonymize or secondary manual review is active.
- `Popup` addresses a candidate group and mention; its actions are Accept/Keep.
- Shared markers cannot identify originals: different people both become PERSON.
  Restoration therefore needs occurrence provenance rather than marker searching.
- `commit_pii_edits` already centralizes revision-checked acceptance and Keep
  rebasing. `ReviewUi` owns UI state, with rendering and tests in private modules.
- Existing document mappings are live-memory state; file handoff is Markdown.

Evidence: `src/pseudonymization.rs`, `src/pseudonymization_ui.rs`,
`src/pseudonymization_ui/render.rs`, ADR 0013 and `docs/design/anonymization.md`.

## Design tree and frontier

1. Default action — confirmed by request: replace and retain clickable highlights.
   - Restore scope and rescan behavior — Q1 confirmed.
   - Original-value/highlight lifetime — Q2 confirmed.
   - Applied behavior in both modes — Q4 confirmed.
   - Editing and undo — Q5 confirmed.
   - Applied-popup behavior and visibility — Q6 confirmed.
   - Existing pseudonym conversion — Q7 confirmed.
2. Maintainability scope — Q3 confirmed with broader user scope.
   - Ownership and occurrence/history tracking — follow chosen scope and behavior.
3. Architecture/performance interview — requested before implementation.
   - Shared domain model and storage — Q8 confirmed.
   - Generic editor change/history contract — Q9 confirmed.
   - Workload envelope and performance priorities — Q10 confirmed.
   - Range update/index strategy — Q11 confirmed.
   - Exact-repeat discovery — Q12 confirmed.
   - Visible geometry and dense hidden-source display — Q13 confirmed.
   - Numerical budgets follow baseline measurement, as confirmed in Q10.
4. Architecture confirmation — received with Q11-Q13 agreement.
5. OSS research — completed; see [source review](pii-oss-research.md).
   - Isolated hybrid detector evaluation — Q14 confirmed and completed.
   - Production recognizer selection — Q15 confirmed: email + contextual INN/SNILS.

## Confirmed decision — Round 6

The user answered "agree" to Q14: evaluate an isolated structured-rule + GLiNER2
prototype before selecting production recognizers. This permits research tools
and synthetic fixtures, not detector activation in the app. The evaluation is
complete with frozen holdout gold, per-rule ablations and retained offline
execution evidence; see the [measurements](../evidence/pseudonymization/2026-10-07-hybrid/measurements.md).

The next frontier at that checkpoint was Q15, subsequently confirmed in Round 7.
The measured email/INN/SNILS subset is included in the feature/refactor.
Recommendation and limitations are recorded in the
[OSS checkpoint](pii-oss-research.md#evaluation-result-and-next-frontier--2026-10-07).
The rest of the confirmed restoration/history/performance design is unchanged.

## Confirmed decisions — Round 1

User response: "Q1-Q2 agree, Q3 - broader scope - anonymizer and pseudonimizer
codebase - make it clean and maintainable".

Q1: Restore the clicked occurrence by default; offer an explicit Restore all for
the same original value, never all fields sharing a category marker. Treat
restoration as Keep at the selected scope so the next scan respects it.

Q2: Retain originals and clickable highlights for the live document, including
tab switches and Save; do not persist originals or restoration metadata in
Markdown or a sidecar. Closing/reopening loses restoration provenance.

Q3: Inspect and refactor the full anonymizer/pseudonymizer subsystem for clean,
maintainable ownership and boundaries: review/grouping/replacement policy,
occurrence provenance and edit/history lifecycle, scan orchestration, detector
adapter, popup/annotation rendering and regression coverage. This is broader
than the restoration commit path. Preserve existing product behavior except
the confirmed changes; do not expand the work to unrelated OCR/conversion or
application subsystems. Supporting generic editor changes may be needed.

Rationale: restore scope must be explicit and rescans must respect review
decisions. Live-document provenance preserves the current Markdown handoff and
privacy boundaries. The user wants maintainability across both PII modes.

## Additional evidence at interview start

`mdoc-editor` owns generic revision-scoped source annotations and atomic
multi-range edits. Its undo/redo snapshots currently contain text and caret;
`EditorEvent::Changed` does not identify the operation or carry edit ranges.
Reliable replacement provenance must therefore account for history and normal
edits explicitly; a global search for shared marker text is insufficient.

## Confirmed decisions — Round 2

User response: "agree, but before implementation let's quickly grill on the
design of the feature and data structures we use, so we make it performant".
This confirms Q4-Q7 while requesting another interview before implementation.

Q4: Keep accepted replacements clickable/restorable in both Anonymize and
Pseudonymize. Pseudonymize keeps explicit acceptance and replacement/linking.

Q5: Unrelated edits shift tracked occurrences. Direct edits/deletion of a marker
remove its applied highlight and restoration action. Undo/redo restores text,
provenance and restoration-created Keep decisions together. Each replacement
batch and each Restore action remains one undo step. Pasted/copied marker text
does not inherit the source occurrence's original value.

Q6: Subtle applied highlights remain visible after leaving review and switching
modes. Use a distinct appearance for pending candidates. Click/keyboard popup
shows category, replaced value, current marker, Restore this occurrence and
Restore all matching originals. Hover only strengthens highlighting. Reuse
hidden-source annotation/context presentation. Close/Escape dismisses popup.

Q7: Restore the immediate pre-replacement text. Anna -> PERSON restores Anna;
PERSON_1 -> PERSON restores PERSON_1. Conversion retains prior live provenance
where available, so the restored pseudonym remains independently inspectable
and restorable. Never infer an original from the spelling of a marker.

## Working terminology

- Applied replacement: a committed marker occurrence with a known prior value.
- Restoration provenance: the live-document association between a particular
  committed occurrence and the value it replaced; marker spelling is insufficient.
- Restore: replace an applied marker with its known prior value at an explicitly
  selected scope, with an undoable Keep decision at that scope.
- Occurrence ID: stable identity for one tracked source span, independent of its
  byte offset, group membership and replacement spelling (proposed in Q8).
- Provenance node: an immutable replacement step referencing shared before/after
  text and an optional predecessor (proposed in Q8).

## Architecture investigation — current source, 2026-10-07

These are static code findings, not measured timing results:

- `Review::refresh` copies the source when changed, scans it for syntax, then
  calls `match_indices` for each original group. It checks found spans against
  accumulated occupied spans and exclusions with linear searches.
- Detection conflict filtering checks every previously accepted span;
  `add_seed` and group lookup search group vectors linearly. Dense unique-group
  workloads can therefore expose quadratic work in portions of ingestion.
- `plan_all` repeatedly calls `plan`, whose lookup traverses the group vector
  and whose source validation compares document text. Batch planning should
  validate once and traverse chosen occurrences once.
- Annotation prepaint loops over all annotations, including ones outside the
  viewport. Hit testing uses painted bounds; annotation geometry already has
  generic editor ownership.
- Editor undo snapshots clone text and caret, capped at 256 undo entries.
  Adding independent PII document snapshots would duplicate that cost.
- Ordinary changes/undo/redo all emit `Changed` without structured edit/history
  identity. Exact provenance requires a generic editor contract rather than
  guessing edit locations from before/after strings.

## Confirmed architecture decisions — Round 1

User response: "agree". Q8-Q10 are confirmed. This does not lift the hold on
implementation pending the final architecture frontier.

Q8: One app-owned domain model shared by both modes; separate domain policy,
detector adapter, scan controller and view/popup rendering. Stable IDs identify
groups, occurrences and provenance steps. Share immutable before/after strings
rather than cloning originals per occurrence. Store active non-overlapping spans
in source order with ID lookup; use a lightweight predecessor link for chains.
ReviewUi remains the UI owner. No PII policy moves into the generic editor.

Q9: Extend the editor with generic exact edit transactions and undo/redo history
identity. Track compact reversible PII metadata changes aligned with editor undo
units, including typing coalescence and redo branches. Rebase ranges in ordered
passes; record invalidated/touched entries, not full PII/document snapshots per
keystroke. Release history-only provenance when the corresponding editor history
is discarded; retain provenance reachable from active replacements for the live
document. No editor text-engine/history rewrite is included.

Q10: Use the existing 2 MiB detector source ceiling as the stress document size,
with 20,000 tracked occurrences as a benchmark workload, not a new product cap.
Test repeated and unique originals, EN/RU/UTF-8, prose/tables/hidden source, one
long line, typing before/inside/after markers, popup/scroll, bulk operations,
chains and undo/redo. Measure policy/tracking separately from detector runtime
and existing editor cost. Prioritize passive interaction latency and bounded
retained metadata; choose numerical gates after obtaining baseline evidence.

## Confirmed architecture decisions — Round 2

User response: "agree, let's also inspect OSS rust libraries related to PII
Detection and Anonymization" with redact-core and qooba/anonymize-rs references.
Q11-Q13 are confirmed. Research is authorized; new detectors or wholesale
library adoption are not implied by a request to inspect them.

Q11: Start with source-sorted contiguous occurrence storage and hash-based ID
lookup. A sorted edit batch rebases occurrences in one ordered pass, O(R + E),
where R is tracked occurrences and E edit ranges. Validate a batch once rather
than once per group. Use ordered neighbor checks for source interval conflicts
while preserving current winner/Keep/syntax rules. Avoid lazy-offset trees until
measured tracking costs justify them. Stable IDs must survive shifts and sort
changes. Insertion exactly before/after a span shifts/preserves it; insertion
strictly inside or any overlapping deletion/replacement invalidates it. Controlled
replacement/restoration records the intended transition instead of treating it
as an ordinary destructive edit.

Q12: Find all known originals in one cached multi-pattern matcher pass, with
existing exact boundaries, syntax protection, exclusion and conflict rules.
Use the locally locked aho-corasick 1.1.5, declared directly when implementing;
it already exists in Cargo.lock. Choose memory-conscious automaton configuration
and measure build/retained memory. Rebuild only when original seeds change.
Run large discovery/syntax work off the UI thread with generation/revision
checks, cancellation and one replaceable latest request. Applied provenance
rebases immediately; candidate updates may complete asynchronously, but acceptance
requires candidates valid for current text. Ordinary typing never reruns GLiNER2.
Full passes are permitted for explicit discovery/rescan and coalesced background
refresh; per-original whole-document searches are removed. Matcher hits never
create applied provenance.

Q13: Restrict applied/pending annotation geometry and hitboxes to visible visual
rows plus small overscan, using source-sorted range lookup. A single long logical
line must also cull by wrapped visual rows. Reuse current table/UTF-8/hidden-source
geometry; offscreen is not equivalent to hidden source. Dense hidden spans in
one containing row use one gutter indicator with a count and an on-demand list
for choosing a field; keyboard navigation still reaches every occurrence.
Prepare popup content only on activation. Scrolling/hover performs no discovery,
detector work or provenance rebuilding. Geometry changes require focused
painted-position correctness and performance checks.

## Validation and implementation sequence

After final design confirmation: capture the current baseline before changing
production behavior, including dense/repeated/unique workloads. Introduce the
generic editor transaction/history contract and deterministic regressions, then
the shared PII model, restoration and UI. Compile and run focused checks at each
boundary. Complete subsystem cleanup and full workspace fmt/Clippy/tests/build.

Compare tracking latency, UI/headless geometry work, bulk costs, metadata counts
and memory against the baseline, with document size, occurrence/group counts,
viewport and build profile recorded. Derive numerical regression ceilings from
baseline observations; report existing editor bottlenecks separately. Native
visual/keyboard/undo interaction complements CPU measurements; platform and
detector-qualification claims remain limited to actual evidence.

## Existing baseline invariants

Preserve local explicit detection, fixed markers, Markdown syntax, source
attachments, separate Copy Markdown, dirty/history behavior, Keep decisions,
and generation/revision rejection of failed, cancelled or stale scans.
This behavior change does not qualify detector accuracy.

## Final confirmation — Round 7

The user answered "agree" to Q15. Include the measured Unicode email and
contextual checksum-valid INN/SNILS subset alongside experimental GLiNER2.
Defer phone/bank-rule expansion and full-engine adoption. This settles the final
frontier and authorizes the previously confirmed restoration/subsystem refactor.
Checksum rejection never suppresses otherwise eligible model candidates.


## Completed implementation

- `src/pseudonymization` owns source-sorted candidates, cached exact-original
  discovery, syntax protection and live provenance with a reversible metadata journal.
- `src/pseudonymization_ui` owns immutable scan jobs, one running/latest pending
  discovery job, applied-value popup, restoration commands and a virtualized hidden-field
  chooser. Both modes retain applied annotations after leaving review and Save.
- `crates/mdoc-editor/src/transactions.rs` defines exact edit geometry and history
  identities; the existing snapshot history stays the sole text-history owner. Atomic
  batches build text once. The PII journal records touched metadata only.
- Editor annotation geometry uses visible visual rows and overscan, an invalidated
  visible-line glyph-position cache and counted gutter markers on containing visual rows.
- Detector windows are isolated in `pseudonymization_detector/windows.rs`; cached
  email/contextual INN/SNILS rules live in `structured.rs`. Declining a checksum does
  not veto model output. No full anonymizer engine or phone/bank rule expansion shipped.

The [retained report](../evidence/pseudonymization/2026-10-07-restoration/README.md)
records numerical gates, the repeated-original overhead tradeoff, native smoke checks
and the remaining base-editor/accuracy/platform limitations.
