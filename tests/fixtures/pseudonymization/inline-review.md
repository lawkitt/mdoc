# Experimental inline review verification — 2026-10-02

The user explicitly authorized the pinned GLiNER2 model experimentally despite
the [failed qualification](README.md). This verifies implementation behavior;
it does not qualify detection quality or accept the proposed numerical targets.

## Implementation

The app owns exact-repeat groups, manual additions, explicit variant linking,
Keep exclusions and stable replacement mappings. All decisions remain in the
live document. The editor supplies generic source annotations independent of
find, painted activation geometry and revision-checked atomic multi-range edits.
Accept applies immediately as one undo step; undo restores reviewability. Normal
edits revalidate exact occurrences, and affected single-occurrence exclusions
are conservatively invalidated. Save and copy contain only Markdown. Tab close
releases review state; no mappings or review decisions enter session storage.

Pseudonymize attempts an explicit offline scan. Setup is a separate download
action, verifies sizes/hashes, retains licenses and shares the pinned ONNX
Runtime without installing OCR models or PDFium. Scanning loads one CPU engine
at a time and drops it on completion/error/cancellation. There is no Hub feature
or network call on the inference path. A shared-runtime-only installation does
not mark OCR ready or damaged; OCR still awaits explicit setup.

The scan includes full Markdown source. Adaptive overlapping windows check the
actual schema/text tokenizer length, with limits of 2 MiB source, 512 tokens per
window and a two-minute cooperative deadline. Oversized pathological words or
other errors discard the complete result. Cancel rejects late results and stops
between bounded calls; native load/inference cannot be interrupted mid-call.
A cancelled engine may briefly retain the inference slot while its call ends.

Source replacements cannot cross Markdown delimiters, line breaks, HTML
names/attribute quotes, list prefixes or link outer delimiters. Ordinary
parenthesized phone values remain replaceable. Unsafe detector spans produce a
manual-narrowing notice. Custom tokens start with an ASCII letter, end with a
letter/number and otherwise use letters, numbers, underscores or hyphens (128
bytes maximum); manual source selections are at most 1,024 bytes. Exact repeats
exclude substrings inside longer alphanumeric words. Variants and initial/inflected
forms require a user action to link. These conservative policies intentionally
leave some spans for narrowed manual review.

## Automated checks

The full gate passed: `rtk cargo fmt --check`,
`rtk cargo clippy --workspace --all-targets -- -D warnings`, and
`rtk cargo test --workspace` (341 passed, 9 opt-in tests ignored).
The model-dependent opt-in check below was additionally run explicitly.

Focused tests cover:

- Unicode ranges and complete source fidelity, including destinations, image
  paths, code and HTML values; protected grammar and parenthesized phone spans.
- Exact-repeat acceptance, one-step undo/redo, source-preserving save/copy,
  stable mappings, explicit linking, Keep-all and remapped single exclusions.
- Invalid/stale/overlapping ranges; cancelled jobs, changed editor revisions
  and replaced document identities rejecting late detection results.
- Tab switching retaining mappings and clean tab close releasing its view.
- Annotation/find coexistence, stale painted geometry rejection and actual
  headless click activation of a hidden-source gutter marker.
- Partial/same-size-corrupted model artifacts, deadline/cancellation checkpoints,
  and a shared inference runtime leaving separate OCR setup available.

These are headless and policy checks, not native visual/IME approval.

## Offline native probe

Apple M4, 24 GiB RAM; debug test build (unoptimized + debuginfo), CPU, four
inference threads, pinned GLiNER2 0.9.6 FP16/threshold 0.5 and ONNX Runtime 1.27.0.
Pre-existing verified qualification artifacts were reused; no download or private
fixture was used. The macOS sandbox denied network access to the test process.
The synthetic EN/RU clause was repeated 64 times (8,640 UTF-8 bytes).

| Check | Observed result |
| --- | --- |
| Complete scan | 7.76 s, including validation/load; 333 raw proposals |
| UTF-8/source fidelity | Every proposal verified against original source; detections reached the tail |
| Active cancellation | Flag requested after 2 s; returned at 2.86 s; no partial result accepted |
| Pre-cancelled scan | Rejected |
| Process peak RSS | 1884.3 MiB (includes both scan attempts and test harness) |
| Exit status / offline execution | Passed under network denial |

[Raw probe log](results/2026-10-02/inline-experimental-scan.log) retains process
output and `/usr/bin/time -l` memory statistics. This repeated smoke input does
not measure recall, false positives, a legal holdout, stress/maximum document
memory, idle RSS or native UI latency. The original quality blockers remain.
The adaptive window/token safeguards also mean this is a different inference
policy from the initial qualification; its quality requires fresh evidence.

To rerun with the preverified cache at `.qualification/models/gliner2-pii-fp16`
and an installed shared runtime:

```sh
rtk proxy env GLINER2_DEVICE=cpu cargo test --workspace experimental_model_offline_scan -- --ignored --nocapture --test-threads=1
```

For enforced network denial, first use `rtk cargo test --workspace --no-run`,
then run the reported mdoc test executable with `sandbox-exec -p
'(version 1)(allow default)(deny network*)'` and the same test/ignore flags.

## Native acceptance still outstanding

Verify light/dark highlights, prose wrapping, RTL, tables, hidden-source markers,
long fragments and popup anchoring near viewport edges/after scroll or resize.
Check actual shortcut routing, replacement-field focus, IME composition and
keyboard review, including alongside Markdown find. Headless synthetic keyboard
input does not establish those results. Windows x64 runtime/inference and native
operation remain unmeasured; Linux and Intel macOS keep manual review only.
The checkpoint still has known EN/RU/hidden-source misses and false positives;
experimental use must review the complete Markdown before sharing.
