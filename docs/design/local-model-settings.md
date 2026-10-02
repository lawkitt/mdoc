# OCR and pseudonymization settings

Status: implemented, 2026-10-02, after explicit user authorization. All four
rounds and shared understanding are confirmed. The implementation and measured
platform limits are recorded in [verification](../../tests/fixtures/model-settings/README.md).
Human native acceptance and new Windows runtime checks remain outstanding.

## Goal

Let users inspect current OCR and pseudonymization settings, identify the model
used, and select between supported models. Support QA when comparing recognition
and detection quality through isolated comparisons on unchanged input.

Use the existing term **pseudonymization**: reviewable replacement with consistent
placeholders, without a guarantee that a document is anonymous.

## Verified starting point

- There is no general settings action/dialog. OCR setup is exposed in the
  workspace; experimental model setup is exposed in pseudonymization review.
- OCR uses one pinned bundle: PP-OCRv6 Small detection plus PP-OCRv5 Cyrillic
  recognition and its matching dictionary. `src/ocr.rs` passes its manifest into
  the pdf-inspector fork. The app does not currently select another bundle.
- Pseudonymization uses one pinned GLiNER2 privacy PII FP16 export at revision
  `e594898629d452e8311796f5f329c7edbeda907c`, CPU inference and threshold 0.5.
  It remains experimental and did not pass model qualification.
- Both features require explicit setup of verified artifacts and run inference
  offline. They share the pinned ONNX Runtime. Enabled automatic inference targets
  are Apple Silicon macOS and Windows x64; pseudonymization operation on Windows
  remains unverified.
- `tools/pseudonymization/` evaluates three pinned candidates outside the main
  workspace. Two use gline-rs with an ONNX Rust binding version incompatible with
  the app's dependency graph. Those candidates cannot simply become app dropdown
  entries. QA threshold experiments already exist in the separate harness.
- OCR recognition algorithms remain in dependency forks; review policy stays in
  the app and generic editor extensions remain model-independent.

Sources: [OCR ADR](../adr/0004-explicit-offline-ocr-with-qualified-models.md),
[pseudonymization ADR](../adr/0002-reviewable-local-pseudonymization.md),
[qualification tooling](../../tools/pseudonymization/README.md),
[OCR evidence](../../tests/fixtures/ocr-qualification/README.md), and current
`src/ocr.rs`, `src/pseudonymization_detector.rs`, `src/main.rs`.

## Decision tree

Round 1 roots (confirmed):

1. Application-wide Settings dialog and contextual links; defaults for future runs.
   - Then: entry points, layout, persistence, focus/keyboard behavior.
2. Curated, supported pinned model bundles, without arbitrary local-model loading.
   - Then: initial candidates, runtime compatibility, qualification labels,
     model information, setup/repair/removal, offline and unsupported states.
3. Isolated same-input comparisons, with automated corpus scoring kept in tooling.
   - Then: immutable input/configuration snapshots, output destinations,
     provenance, metrics/export, and separation from current document edits.

Round 2 branches (confirmed):

- Q4: initial catalog, including whether alternative precision is sufficient for
  first-version pseudonymization QA or distinct detector integration is required.
- Q5: editable parameters versus fixed/read-only runtime limits.
- Q6: dialog layout, entry points, persistence and change application lifecycle.
- Q7: QA input capture, configuration overrides and result presentation.
- Q8: model details, selection/install distinction, setup/repair/removal behavior.

Round 3 branches (confirmed):

- Q9: exact parameter choices, bounds and reset defaults.
- Q10: comparison page scope, snapshot refresh and cancellation ownership.
- Q11: raw model output versus prepared output, and QA measurement semantics.
- Q12: comparison retention and explicit export contents.
- Q13: download/repair lifetime and cancellation feedback.
- Q14: unapplied changes, persistence failures and unavailable-model behavior.

Round 4 branches (confirmed):

- Q15: model-work admission between QA and document inference.
- Q16: failed alternative-model validation and platform availability policy.
- Q17: completion evidence and native/platform acceptance boundaries.

Frontier: empty. No product decisions remain open for this scope. Runtime and
platform checks are implementation evidence to collect, not assumed results.

## Confirmed decisions

The user answered Q1 yes (with the exact label **Settings**, without dots), Q2
yes and Q3 yes to the recommendations on 2026-10-02:

- One application-wide **Settings** dialog with **OCR** and **Pseudonymization**
  sections, reachable with no document open. Contextual links use **Settings**.
  Defaults affect future runs, and results identify the configuration used.
- Curated pinned bundles with verified explicit downloads. Installation state,
  language support and qualification/experimental status are distinct. Arbitrary
  local-model loading is excluded from the first version.
- QA option B: isolated sequential comparisons on identical unchanged input,
  separate results, configuration provenance and timing. Automated corpus scoring
  stays in existing tooling initially. This avoids comparing against text changed
  by the first run.

These decisions are recorded in [ADR 0006](../adr/0006-local-model-settings.md).

Round 2 answers on 2026-10-02 accepted Q4, Q6, Q7 and Q8. Q5 was accepted
with the explicit amendment to exclude automatic/forced selection and fix OCR
to **Force**:

- First catalog: current Cyrillic OCR bundle plus original PP-OCRv6 Small;
  GLiNER2 FP16 plus FP32 after actual compatibility/resource checks. Keep the
  Cyrillic and FP16 model defaults. Distinct pseudonymization engine integration
  is deferred. Alternative precision does not imply a distinct detector.
- Advanced controls: OCR resolution and minimum recognition confidence;
  pseudonymization detection threshold. No routing-mode control. Explicit OCR
  uses Force on all selected pages; this is a change from the verified current
  Auto implementation, not a claim that Force was the existing default. Native
  extraction on opening remains separate and OCR consent remains explicit.
  CPU execution/resource limits are read-only. Include Reset to defaults;
  category/schema editing and GPU selection are deferred.
- Compact two-section dialog, menu/standard settings shortcut/contextual links,
  Apply and Close. Applied application defaults persist across restart and affect
  the next run. Running jobs and existing results retain captured configuration;
  changes do not rerun documents automatically.
- Separate Compare models view, opened from Settings for a suitable active
  document. Capture PDF bytes for OCR or complete current Markdown for
  pseudonymization once. Sequential runs use that same snapshot. Switchable
  read-only outputs include text/detected spans, configuration, warnings and
  elapsed time. Start with two runs and allow additional runs on the snapshot.
  QA overrides never change defaults or edit the working document.
- Selecting a model only changes the preference; Download is explicit and
  separate. Missing preferred models offer setup instead of silent fallback.
  Summary shows name, language evidence, experimental/qualification status,
  installation state and download size. Details exposes revision, precision,
  runtime, licenses and storage location. Offer Repair and Remove; preserve
  shared runtimes and prevent removal while in use.

Round 3: the user answered **agree** to Q9-Q14 on 2026-10-02:

- OCR resolution choices are 150/200/300 DPI, reset to 150. Minimum recognition
  confidence accepts finite values in 0-1 inclusive, reset to 0. Pseudonymization
  threshold accepts finite values in 0-1 inclusive, reset to 0.5. Higher thresholds
  discard more predictions; confidence is not measured accuracy. Execution limits
  remain fixed/read-only. Reset also restores the agreed model defaults.
- OCR comparisons default to All pages with an optional QA page range. Full
  Markdown is always the pseudonymization input. Input and page selection are
  frozen per comparison; New comparison captures fresh input. Document switching
  does not change a comparison. Closing it cancels queued/pending work and drops
  retained snapshots/results; a running synchronous call may retain its input
  until the next checkpoint. Late results cannot reopen the closed view.
- OCR QA offers Recognition output and Prepared Markdown, page warnings and
  timings. Raw recognition support is implemented in the converter fork.
  Pseudonymization QA shows detected text/category/source location/confidence.
  Counts and elapsed time are measurements; annotated-fixture tooling handles
  accuracy scoring.
- Comparison results are in memory until close, without restart persistence or
  automatic reports. Explicit Export report includes model revisions/configuration,
  input hash, timings, warnings and recognition output/predictions. It excludes
  replacement mappings. OCR output can be saved separately as Markdown.
- Setup/repair continues after Settings closes; reopening shows current state.
  Cancel download retains completed verified artifacts and discards incomplete
  files. Show measurable byte progress, then Verifying/Checking runtime rather
  than fabricated percentages. Settings setup never starts document processing.
- Close/Escape discards unapplied preferences and restores prior focus. Apply
  atomically persists defaults; persistence failure preserves old active defaults
  and shows an error. Missing settings use defaults. Invalid/obsolete stored
  preferences require a visible notice and explicit reset/supported selection,
  without an unnoticed model substitution.

Round 4: the user answered **agree** to Q15-Q17 and confirmed the completed
design on 2026-10-02:

- One comparison job may execute at a time. QA inference never overlaps ordinary
  document OCR or pseudonymization inference. Conflicting requests show busy/retry,
  without automatic admission queues; the explicitly requested runs within one
  comparison execute sequentially. Editing, viewing and saving remain available.
  Cancellation retains the execution slot until the native call actually stops.
- An alternative that fails runtime loading or cannot operate within existing
  limits remains visible but disabled on the affected platform with the measured
  reason. Complete the infrastructure and working alternatives without silently
  changing selected models or relaxing limits. Quality/experimental status is
  distinct from runtime availability; known language misses remain visible.
- Complete the full repository gate, focused settings/lifecycle tests, converter
  raw-output tests and actual offline probes for every enabled model. Record timing
  and peak memory with machine/build details. Native macOS layout/focus/shortcuts
  and numeric input receive separate checks. Report outstanding Windows/native
  checks explicitly rather than treating headless results as acceptance.

## Implementation direction

Implementation guidance derived from the agreed scope:

- Keep a small app-owned settings/catalog module and a versioned local preference
  file containing configuration only. Keep tab restoration and replacement maps
  separate. Share applied defaults across application views; jobs capture values.
- Extend existing setup and detector paths to accept catalog/configuration
  snapshots instead of adding a generic plugin framework or a new inference engine.
  Reuse verified existing default artifacts without re-downloading them.
- Retain per-run model identity and configuration with regular document OCR/review
  results as well as comparisons. Later defaults do not relabel earlier results.
- Keep a bounded app-owned comparison session and read-only result presentation;
  snapshots never invoke editor replacement or overwrite the source file.
- Add optional raw recognition results to the pdf-inspector fork, preserving
  current conversion behavior and avoiding duplicate OCR inference for QA.
- Retain explicit consent and experimental warnings, native-only extraction and
  manual review when automatic inference is unavailable.

## Accepted completion criteria

- Focused logic/lifecycle tests and headless GPUI flow checks cover atomic Apply,
  invalid preferences, per-run snapshots, source preservation, QA isolation,
  cancellation/late completion, installation reuse and guarded model removal.
- The repository full gate passes: fmt, workspace clippy with denied warnings and
  workspace tests. Converter fork tests cover raw output and forced page selection.
- Opt-in offline probes use actual installed artifacts for every enabled model;
  record model/runtime revisions, machine/build, threshold/DPI, elapsed time and
  peak memory. Compare against existing synthetic EN/RU evidence and expose failures.
- Native Settings/comparison layout, focus/shortcuts and input checks remain
  distinct from headless results. Windows runtime operation requires Windows
  evidence. Buildability does not establish model runtime qualification.

## Explicit deferrals

- Arbitrary model paths and user-authored model manifests.
- Distinct pseudonymization engines requiring a different runtime binding.
- Editable entity schemas/categories, GPU selection and configurable execution
  limits or OCR routing mode.
- In-app annotated-corpus accuracy scoring, automatic report saving and restart
  persistence of comparison inputs/results.
- Online inference, cross-document replacement maps and guaranteed anonymization.

## Additional investigation for round 2

- The pinned pdf-inspector fork already exports `PP_OCR_V6_SMALL` alongside
  `PP_OCR_CYRILLIC`. It also exposes Auto/Force routing, minimum recognition
  confidence, render DPI and selected page numbers. Its original PP-OCRv6 bundle
  has a documented Russian failure; availability is not qualification for RU.
- The pinned GLiNER2 export repository contains both `fp16_v2` and `fp32_v2`.
  The installed Rust engine supports the legacy folder layout; FP32 is a plausible
  compatible candidate, not a tested app option or a distinct detector. The FP32
  listed artifacts total about 1.25 GB including tokenizer, excluding runtime.
  No new artifacts were downloaded or inference-qualified in this interview.
- The current app threshold is fixed at 0.5. OCR minimum confidence is 0.0,
  routing is Auto and default rendering is 150 DPI.
- The current pipeline can retain/fuse trustworthy native text even when OCR
  is forced. Comparing only prepared Markdown can therefore conceal recognition
  differences. Raw recognition evidence and prepared output are distinct QA
  choices. The app's public pipeline result currently exposes prepared pages and
  provenance; the agreed raw-output addition belongs in the dependency fork.

FP32 source: [pinned upstream file listing](https://huggingface.co/jugaadsrl/gliner2-privacy-filter-PII-multi-onnx/tree/e594898629d452e8311796f5f329c7edbeda907c/fp32_v2),
verified through the repository tree API on 2026-10-02.

The agreed model/comparison/recognition terms are recorded in
[CONTEXT.md](../../CONTEXT.md).
