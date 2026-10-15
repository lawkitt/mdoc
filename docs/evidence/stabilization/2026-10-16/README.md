# Maintainability and performance review, 2026-10-16

Review point: `fd3c685` (`v0.1.0-beta.10`), clean `main` before this note.
Status: pre-change audit evidence. Decisions are recorded in
[ADR 0049](../../../adr/0049-measured-stabilization-and-architecture-review.md);
implementation results will be appended separately.

## Assessment

The earlier ownership work remains useful: document identity, preview lifetime,
import admission, search scheduling, editor history and the GUI-free PII model
have explicit owners. This review does not justify replacing that architecture.
Since the October 9 cleanup, spelling, encrypted cases and synchronization,
AI-response restoration, before/after views, updates and the new shell have
expanded the host integration substantially.

The concrete problems are repeated computation, refresh methods with unrelated
effects, and persistence error handling. Splitting files by length would leave
these problems intact. Keep ADR 0029's app-first cleanup and test-removal rules;
do not reopen superseded reusable-library requirements from ADRs 0010/0011.

Inspection covered the application shell/document lifecycle and integration,
PII review/panel/case synchronization, editor layout and spelling, persistence,
performance harnesses and CI. This is a targeted codebase audit, not an exhaustive
security, model-quality or dependency-fork review.

## Findings

### F1 — P1: Quit discards case persistence failures

`src/workspace_cases.rs:896` folds member state into cases, drains pending writes
and calls `store.save` synchronously. Store-open errors and write errors are
discarded (`let _ = store.save(&case)`, line 913). `finish_quit` in
`src/workspace.rs:1481` then releases document views and removes the window.
If persistence fails, latest case aliases/originals can be lost without a retry
or an explicit choice. The synchronous path also waits on the store mutex and
filesystem flush on the UI thread (`src/cases.rs:238`).

This is a source-confirmed error path, not a reproduced disk-failure scenario.
Session-save failure already has an explicit quit/cancel path; cases need an
equivalent contract. Extract a small persistence owner only if it makes pending,
in-flight, failed and acknowledged revisions explicit. Keep encryption, serialized
writes, stale-write protection and deletion semantics.

### F2 — P2: Layout scans every diagnostic for every logical line

`crates/mdoc-editor/src/element.rs:3449` filters the entire diagnostics vector
inside the document-line loop. This happens before the offscreen plain-row skip
at line 3747. With L lines and D spelling diagnostics, this part costs O(L × D)
even on a warm redraw. OCR output with many misspellings is the relevant workload.
Index or walk sorted diagnostics by intersecting line ranges; preserve source
offsets, table styling, painted geometry and caret/selection behavior.

The current tab benchmark also demonstrates expensive unchanged document redraws
without establishing which individual layout stage dominates. Existing structural,
run, height and table caches should be profiled and preserved; do not describe
the editor as having no caching or no offscreen culling.

### F3 — P2: Repeated lines defeat the spelling cache

`crates/mdoc-spell/src/lib.rs:212` removes a cached entry from `previous` on its
first use. Subsequent identical lines in the same document re-run `check_line`;
the current pass's cache is not consulted. Output remains correct, but unchanged
repeated paragraphs/table rows get almost no dictionary-work reuse.

A temporary probe linked the current debug `mdoc-spell` library, loaded dictionaries
once, and checked unchanged text five times. Median warm timings:

| Lines | Repeated source | Unique source with numeric line suffix |
| --- | ---: | ---: |
| 2,000 | 45.987 ms / 98,000 bytes | 2.107 ms / 106,890 bytes |
| 10,000 | 224.741 ms / 490,000 bytes | 9.888 ms / 538,890 bytes |

The repeated line was `The parties agree to the terms of this contract.` followed
by a newline. Unique lines added their index before the full stop; both workloads
had zero diagnostics. This is a dictionary/cache microbenchmark, excluding
Markdown exclusions, host scheduling and painting. The probe executable used
`rustc -O`; the linked dependency was the workspace's debug library. It is not a
release-app latency claim.

### F4 — P2: A visual refresh schedules unrelated expensive work

`src/pii/ui.rs:270` builds annotations, then unconditionally schedules spelling
and case persistence at lines 345–347. Its callers include theme refresh,
transactions and review edits. In `src/document_view.rs:146`, an editor
Transaction refreshes annotations; Changed can refresh them again through
`pii_edited`, then explicitly schedules spelling a third time. The overlapping
paths are the maintainability problem, regardless of whether earlier tasks are
cancelled before executing.

`src/spelling.rs:136` copies the full source and PII ranges before its 300 ms
debounce. Superseded requests therefore still pay snapshot/allocation costs.
Generation checks protect publication, but the worker performs synchronous
checking without an in-work cancellation check or SearchSession's explicit
one-running/one-latest-pending admission model.

Give text changes, PII-exclusion changes, presentation changes and case-mapping
changes explicit refresh effects. Capture spellcheck input once after debounce;
keep stale-result guards and bound background work. Avoid introducing a general
event bus, service container or new crate to solve this local coupling.

### F5 — P2: Panel virtualization does not bound data preparation

`src/pii/ui/mapping/panel.rs:492` builds the complete panel projection on every
render before handing it to `uniform_list`. For every expanded entity,
`replacement_entries` calls `identity_occurrences` (`popup.rs:119`), scanning all
pending/applied occurrences and sorting the result. `kept_of` similarly rebuilds
and filters the full kept list. Preparation can reach O(E × M) for E expanded
entities and M mentions. Visible header checkbox queries rescan again.

The Share tab builds the same entries merely to count entities
(`src/pii/ui/mapping/share.rs:73`). Before/after redraw also scans and hashes every
applied replacement before discovering that its content is unchanged
(`src/pii/ui/compare.rs:310`).

Prefer a review-revision-keyed projection with occurrences/counts grouped once by
identity, then separate filter/expansion state. Keep one authoritative review
model; invalidate derived data on text/history, identity, Keep and case changes.
These complexity findings are source-derived; panel and before/after timings
have not been isolated in this review.

### F6 — P2: Performance checks no longer provide a complete usable baseline

The existing host matrix fails twice on the ordinary workload before reaching
its larger workloads. Its fixed outer-width comment also predates the 760 px
reading column and new shell; effective document width must be recorded before
historical comparisons. Do not raise ceilings to make the current tree green.

The 20,000-annotation matrix sets editor annotations directly, bypassing the
review panel, case synchronization and end-to-end review updates. There is no
comparable diagnostic-density or before/after/case fan-out matrix. Furthermore,
`scripts/perf.sh` unconditionally invokes `large_markdown_scroll_budget`, whose
default DOCX fixture is absent in this checkout. Restore a public reproducible
baseline and make optional private-corpus runs explicit. Keep machine-dependent
timing separate from deterministic routine CI invariants.

## Verification on this tree

Apple M4 (`Mac16,13`), 24 GiB RAM, macOS; Cargo debug/test profile. Tests were run
serially across benchmark commands, but this was an ordinary desktop session,
not a controlled idle performance runner. Zed/WindowServer had background load.
No historical speedup/regression attribution or native frame-latency claim is made.

| Check | Result |
| --- | --- |
| `cargo test --workspace --locked --offline` | 547 passed, 18 ignored; no failures |
| `cargo clippy --workspace --all-targets --locked --offline -- -D warnings` | Passed |
| `cargo fmt --all --check` | Passed |
| `host_performance_matrix`, twice | Failed ordinary scroll ceiling: 14.007 / 13.784 ms median vs 12 ms |
| Ordinary typing, those runs | 14.404 / 14.445 ms median; 21.470 / 21.592 ms p95 |
| Ordinary search burst + paint | 168.660 / 169.267 ms vs 140 ms ceiling; run asserts scroll first |
| `tabs_host_performance` | Passed; activation handler median 0.006 ms; 24-tab metadata snapshot median 0.011 ms |
| Long Markdown tab, 135,000 bytes | Frame median 230.709 ms; unchanged-document frame median 235.993 ms, p95 243.771 ms |
| Five repeated DOCX close cycles | Viewer and temporary-file release assertions passed; RSS rose, but this does not prove a leak |
| `mdoc-editor::search_performance_matrix` | Passed; 2,310,144-byte cold index 814.431 ms, cached folded query 20.266 ms, edit rebuild + query 822.778 ms |

Warnings: registry minimum-publish-age setting ignored on this toolchain,
upstream `block` future compatibility, and debug-linker unwind-table size.
No native UI/IME/accessibility, Windows/Linux, real detector/OCR, release-profile
rendering or peak-memory acceptance was performed. The full geometry stress
matrix and missing-fixture scroll test were not run.

Raw logs and the spelling probe are saved in the task's artifact directory:
`/Users/tebriz/.codex/visualizations/2026/10/15/01a14089-5423-7853-b157-0f52a60ddfbf/mdoc-codebase-review-2026-10-16/`.

