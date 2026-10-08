# Keep applied PII replacements inspectable and restorable

Status: accepted and implemented, 2026-10-07. The final Q15 agreement authorized
email plus contextual checksum-valid INN/SNILS alongside the restoration/refactor.

> Since [ADR 0022](0022-remove-anonymization-mode.md) (2026-10-08) there is no
> Anonymize action or shared category marker; references to them below are
> historical. Inspection, scoped restoration and history travel apply to aliases.

## Confirmed direction

The default Anonymize action continues to scan locally and replace eligible
PII. Applied fields remain highlighted and clickable, exposing a popup that can
restore the replaced value.

Restore defaults to the clicked occurrence. An explicit action can restore all
applied occurrences of the same original value, rather than every field sharing
a category marker. Restoration creates Keep decisions at the chosen scope so
subsequent scans respect it.

Keep originals and occurrence provenance in the live document across tab
switches and Save. Do not serialize originals or restoration metadata into
Markdown, a sidecar, or application session persistence. Closing/reopening ends
restoration availability for those occurrences.

Review and refactor the full anonymizer/pseudonymizer subsystem to maintain
clear responsibilities and ownership, including policy, grouping, provenance,
scan orchestration, detector integration and UI. Preserve unrelated product
behavior and the local-first detection/editor boundary.

## Rationale

Different originals share tokens such as PERSON. Token matching cannot safely
recover which original belongs to a particular occurrence. Explicit provenance
is necessary, with edit/history safeguards to prevent stale restoration.

Live-memory originals retain the existing separation between review state and
Markdown handoff. Refactoring both PII modes avoids separate competing lifecycle
implementations for the same editor and scan machinery.

## Additional confirmed behavior

Both modes retain inspectable/restorable applied replacements; Pseudonymize
continues to require explicit acceptance. Applied highlights remain after closing
review and switching modes, with styling distinct from pending candidates.
Click/keyboard popup shows the replaced value, marker and restoration scope.

Unrelated edits shift tracked occurrences. Editing/deleting a marker invalidates
its restoration action. Undo/redo restores text, provenance and restoration Keep
decisions together. Each Restore action is one undo step. Copy/pasted markers
gain no original-value provenance.

Restore returns the immediate pre-replacement text. A known live conversion
Anna -> PERSON_1 -> PERSON can be restored to PERSON_1, then Anna, while keeping
each operation explicit and preserving prior provenance.

## Confirmed architecture

Use one shared app-owned PII domain model with stable group/occurrence/provenance
IDs, source-sorted spans and shared immutable before/after text. The generic
editor owns annotation geometry and text history; PII policy stays in the app.

Extend the generic editor change contract with exact transactions and undo/redo
identity. Align compact reversible PII metadata changes with editor undo units,
including coalescence and branching; avoid a duplicate document snapshot history.
Prune history-only provenance with editor history while keeping active provenance.

Benchmark 2 MiB documents with 20,000 occurrences, repeated and unique originals,
UTF-8, dense/hidden/table/long-line content and history chains. Prioritize passive
interaction and retained memory; measure independently of detector runtime and
derive numerical gates from baseline evidence. Workload sizes are not new caps.

Source-sorted occurrence storage plus ID lookup uses one ordered rebasing pass
per edit batch. Prefer this simple layout until measurements justify a more
complex offset tree. Use cached multi-pattern matching for exact originals,
with background discovery and current-revision acceptance. Paint visible visual
rows plus overscan; aggregate dense hidden fields behind a counted gutter marker
with on-demand selection and full keyboard navigation.

## OSS research checkpoint

Inspect Rust PII detection/anonymization projects for reusable ideas and narrow
components before implementing. Detector policy changes or additional product
dependencies require an explicit scope decision informed by source inspection
and mdoc's EN/RU, Markdown, runtime and lifecycle constraints. See the
[design tree](../design/anonymizer-restoration.md).

Q14 authorized an isolated hybrid-rule evaluation, now completed. Email plus
contextual checksum-valid INN/SNILS match the complete hybrid's measured gains
on the small synthetic corpus; broader phone/bank additions show no incremental
exact hits. Q15 selected this narrow subset for experimental production use;
phone/bank expansion and full engines remain deferred. The
[source review and proposal](../design/pii-oss-research.md) and
[measurements](../../tests/fixtures/pseudonymization/results/2026-10-07-hybrid/measurements.md)
preserve this distinction; detector quality remains experimental.


## Implementation and validation

The app owns `pseudonymization/{tracking,discovery,syntax}` and the review controller's
scan, discovery, applied-popup and chooser modules. The editor exposes generic exact
source transactions/history identities and visible-row annotation geometry. Shared
immutable before/after steps and reversible metadata deltas follow editor coalescence,
undo/redo, branches and pruning. Originals are never serialized.

See the [implementation report](../../tests/fixtures/pseudonymization/results/2026-10-07-restoration/README.md)
for regressions, native macOS checks, CPU/storage measurements and remaining limits.
