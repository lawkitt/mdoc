# PII restoration implementation verification — 2026-10-07

Implements [ADR 0014](../../../adr/0014-highlighted-pii-replacements.md).
The measured email/contextual INN/SNILS subset is included experimentally; phone/bank
rule expansion and full anonymizer engines remain deferred.

## Behavior and ownership

Applied fields remain clickable in Anonymize and Pseudonymize after Close review,
mode changes, tab switches and Save. The popup shows the immediate before value,
current marker and category, with Restore this occurrence as the default and an
explicit Restore all matching originals action. Equal category markers never imply
equal originals. Restoration creates occurrence Keep decisions respected on rescan.

Text and restoration metadata travel together through Undo/Redo. Unrelated exact
edits shift spans; insertion at the start shifts, at the end preserves, and edits
inside/deleting a marker invalidate provenance. Undo recovers it. Pasted tokens
acquire no provenance. Anna -> PERSON_1 -> PERSON retains predecessor steps. Originals
and tracking are memory only; closing/reopening loses them and Save writes Markdown.

The app owns source-sorted candidates, shared immutable before/after steps, compact
reversible metadata deltas and ID lookup. Adjacent coalesced typing/IME edits compact
when no editor undo state references the intermediate identity; touched-field deltas
remain reversible. The editor retains its existing text snapshots, selection/caret,
IME semantics and undo limits, exposing generic source transactions/history identity.
No duplicate PII text-history snapshot store was added.

Syntax, discovery and provenance have cohesive domain modules; scan orchestration,
background candidate refresh, applied popup and lazy chooser have private controller
modules. Tokenizer-bounded detector windows and selected structured rules are separate.
The source matcher uses cached aho-corasick contiguous NFA; ordinary typing refreshes
known originals in the background with revision/version/identity/cancellation guards,
one running task and one replaceable latest request. It never reruns the model.

Visible visual rows plus 64px overscan bound annotation geometry, including a long
logical line. Hidden fields share counted indicators on their containing visual row;
a virtualized chooser and keyboard navigation reach every field. Cached glyph-position
indexes invalidate on shaped layout identity, wrapping and width; bidi uses the existing
geometry fallback. Search still owns a separate highlight channel.

## Correctness and native checks

`cargo test --workspace --locked -q`: **414 passed, 15 ignored**, no failures.
`cargo clippy --workspace --all-targets --locked -- -D warnings` passed. Formatting,
diff validation, debug build and the final serial release build also passed.

Regression coverage includes exact/coalesced edit identity, original editor IME behavior,
atomic UTF-8 batches, annotation/search coexistence, hidden chooser keyboard actions,
restoration scope, Keep/rescan, source edits/paste, predecessor chains, undo/redo branches,
active-provenance retention and history pruning. A wrapped-hidden paragraph regression
checks that scrolled indicators follow visible visual rows rather than the logical start.
The selected recognizers match the frozen isolated rule ranges/categories on all 33
fixtures; invalid contextual/checksum results never veto model candidates.

Native macOS debug app, synthetic three-email document: automatic replacement left three
visible highlights; clicking showed the exact before/current values; Restore all restored
only the two equal originals; Undo/Redo preserved provenance; a fresh scan replaced zero
restored occurrences. Close review retained the third highlight. Save retained its original
inspection; Alt-Down opened it and Enter restored it. Undo + Save + quit/reopen left the
saved marker without highlight/provenance. The previous session manifest was backed up
and restored after the isolated synthetic check. No user document was modified.

The current FP16 integration also passed a deny-network scan/cancellation probe: 8,640
bytes, 397 spans, 18.61s in the debug runtime; cancellation returned after 5.89s with no
partial result accepted. This is integration evidence, not accuracy qualification.

## Policy CPU baseline

Apple M4, macOS 15.7.7, Rust 1.98.1. Exactly 2,097,152 bytes and 20,000 occurrences.
Pre-refactor baseline is pinned commit `68960389525c8f7ae4eb7fc62eb7a242dd9b648c`,
compiled with `rustc -O`; current exact app policy/transaction types use an isolated
Cargo release harness without GPUI/model loading. Baseline was one call per operation;
current refresh/planning report seven-call medians, tracking 101 calls. These small
host samples are regression evidence, not cross-platform performance guarantees.

Initial comparison (ms), captured before concurrent release linking:

| Groups | Baseline ingest | Current ingest | Baseline refresh | Current refresh | Baseline plan | Current plan |
| ---: | ---: | ---: | ---: | ---: | ---: | ---: |
| 1 | 80.604 | 12.009 | 2.133 | 5.510 | 0.472 | 0.790 |
| 1,000 | 316.674 | 16.462 | 189.977 | 9.390 | 38.116 | 1.300 |
| 20,000 | 4,119.168 | 27.775 | 2,408.540 | 11.699 | 852.348 | 1.511 |

Initial provenance commit cost was 0.813/0.929/2.507ms; exact 20,000-field shifts
were 0.200/0.204/0.201ms. Stable occurrence reconciliation adds cost for one repeated
original (refresh 2.1 -> 5.5ms); unique-group passes remove the old per-group full-source
and per-group planning work. Discovery runs off the typing path. `policy-current.txt`,
`policy-baseline.txt` and `policy-provenance.json` retain the final reproducible rerun.

The final idle rerun after metadata compaction measured 20,000 distinct groups at
27.093ms ingest, 10.808ms refresh, 1.384ms planning, 2.355ms provenance commit and
0.193ms tracking shifts. Reproduced baseline was 4,182.517/2,475.309/1,054.244ms for
those first three operations. One-group refresh remains slower (2.215 -> 6.559ms).

## Geometry, storage and gates

Debug headless GPUI, 1332x750 viewport, 2 MiB/20,000 annotations, seven warm draws per
state. First three-workload run built 33 prose bounds, 182 long-line bounds and 33 hidden
bounds at the top; middle-scroll bounds stayed below 500. Added median annotation cost
was 1.097ms prose, -17.435ms long-line (measurement noise), 5.520ms hidden. Whole draws
were 1.3-1.7 seconds even without annotations: this captures an existing base-editor
stress bottleneck and does **not** prove responsive end-to-end large-document editing.
RSS 411-597 MiB was cumulative process RSS across cases, including the editor/text
shaper, not retained PII metadata. Release geometry evidence is retained separately.

The final serial release matrix also passed, including UTF-8 tables and dense hidden
links on one wrapped logical line. Seven warm draws per state, same viewport/source size:

| Layout | No annotations median ms | Annotated median ms | Difference ms | Top visible bounds |
| --- | ---: | ---: | ---: | ---: |
| Prose | 128.779 | 128.958 | 0.179 | 33 |
| Long line | 66.240 | 66.555 | 0.315 | 182 |
| Hidden, separate rows | 125.703 | 124.018 | -1.685 | 33 |
| UTF-8 table | 279.621 | 280.690 | 1.069 | 20 |
| Dense hidden, wrapped | 3,365.080 | 3,383.023 | 17.944 | 198 |

Negative differences and the dense-case increment should be treated cautiously: these
are sequential whole-frame samples, not instrumented annotation-only self time. Dense
hidden wrapping remains unresponsive even before annotations, and its 17.944ms observed
increment exceeds the initial 8ms review ceiling used for simpler layouts. Bounded
geometry is verified; end-to-end responsiveness for that extreme layout is **not
qualified**. Further editor layout/profiling work is needed, not a lower document cap.

The UTF-8 2 MiB/20,000-occurrence matrix verifies shared source/matcher references;
matcher memory was 409 bytes (1 group), 23,184 bytes (1,000), 453,840 bytes (20,000).
Candidate vector capacity occupied 1,048,576 bytes. After 300 distinct history units,
20,000 applied fields shared one step: occurrence vector 640,000 bytes and 255 compact
deltas 38,760 bytes, excluding hash buckets/allocator overhead. Invalidated provenance
and its original references were released after their last undo unit was pruned.

Local regression review ceilings derived from these observations (rounded headroom,
not product limits): <=1ms median exact rebasing; <=25ms warm discovery; <=60ms initial
ingest; <=5ms batch planning; <=10ms provenance commit; <=1 MiB matcher and <=3 MiB
candidate+applied vector capacities at 20,000 occurrences/groups. Geometry must retain
fewer than 500 bounds for these fixed workloads and avoid a warmed median annotation
increase above 8ms on this debug host. Timing gates are reviewed on an idle matching
host rather than flaky universal wall-clock assertions; geometry correctness/counts,
sharing, history bounds and offsets are asserted. The newly measured dense wrapped
hidden layout exceeds the initial incremental-time ceiling and remains an explicit
performance limitation. Base-editor responsiveness remains
a separate blocker rather than changing the existing source-size cap.

## Reproduction

```sh
python3 tools/pseudonymization/benchmark_policy.py
cargo test --locked utf8_matcher_storage_matrix -- --ignored --nocapture --test-threads=1
cargo test --locked metadata_history_stress -- --ignored --nocapture --test-threads=1
cargo test --release --locked -j 1 --bin mdoc pii_annotation_geometry_matrix -- --ignored --nocapture --test-threads=1
sandbox-exec -p '(version 1) (allow default) (deny network*)' cargo test --locked experimental_model_offline_scan -- --ignored --nocapture --test-threads=1
cargo test --workspace --locked -q
cargo clippy --workspace --all-targets --locked -- -D warnings
cargo fmt --all -- --check
cargo build --locked
cargo build --release --locked -j 1
```

The model probe requires the existing verified qualification cache/native runtime;
it performs no downloads. No production model pin or threshold changed. The initial
additional release-test build was SIGKILLed during parallel LTO of app/test targets;
serial recovery passed for both the final release app and the five-workload geometry
test. Compiler output still notes the pre-existing `block 0.1.6` future-compatibility
warning; there are no strict-Clippy diagnostics.

Detector quality remains experimental, including known EN/RU misses and false positives.
This synthetic/native macOS checkpoint does not qualify Linux/Windows input/GPU behavior,
native IME on every input method, representative contract layouts or independent recall.
