# Inspect and select local model settings

Status: Accepted, 2026-10-02. Amended by
[ADR 0026](0026-first-use-model-setup.md) (defaults, first-use setup, Settings
rows and footer).

## Problem

Users need to inspect the current OCR and pseudonymization configuration and
compare models during QA. Each feature pinned one bundle with feature-specific
setup controls.

## Decision

- **One Settings dialog** with OCR and Pseudonymization sections, available
  without an open document. Defaults apply to future runs; each result records
  the configuration it actually used. Running jobs and existing results keep
  their configuration; changing settings never reruns a document.
- **Curated catalog** of pinned bundles with verified downloads: OCR English
  PP-OCRv6 Small and Cyrillic; pseudonymization GLiNER2 FP16 and FP32. No
  arbitrary local models. Installation state, language support and
  qualification status are shown separately; installed does not mean qualified.
  Known language failures stay visible.
- **Advanced settings**: OCR resolution 150/200/300 DPI (default 150), minimum
  recognition confidence 0–1 (default 0), pseudonymization threshold 0–1
  (default 0.5). Higher thresholds discard predictions; they are not accuracy
  scores. CPU execution and resource limits are read-only. Reset to defaults.
- **OCR routing is Force**: explicit OCR recognizes every selected page. Opening
  a document still never starts recognition.
- **Model compare** runs sequentially on one captured snapshot (PDF bytes or the
  full current Markdown), never on the working document and never changing
  defaults. Results show recognition output (separate from prepared Markdown) or
  detected spans, configuration, warnings and elapsed time, kept in memory until
  the view closes. Closing cancels queued work and rejects late results. Export
  report includes configuration, input hash, timings and outputs, never mappings.
  Accuracy scoring stays in the developer tooling.
- **Artifacts**: selecting a model changes only the preference; downloading is
  separate. Repair and Remove keep shared runtimes and refuse a model in use.
  Cancel keeps verified files and discards partial ones. Progress is byte-based,
  then Verifying / Checking runtime; no invented percentages.
- **Admission**: one model job at a time; QA never overlaps document inference;
  conflicts show busy/retry without queues. Cancellation frees the slot only
  when the call stops.
- **Persistence**: applied settings persist atomically; a failed save keeps the
  previous defaults. Invalid or obsolete stored settings show a notice and need
  an explicit reset; a removed model is never silently substituted.

## Consequences

OCR algorithms stay in the forks (ADR 0001); settings and QA orchestration are
the app's. Distinct detector engines, category editing and GPU selection are
deferred.

References: [full text before ADR 0052](https://github.com/lawkitt/mdoc/blob/f368e436cd2d3e70789d51879b9f54f379900e1e/docs/adr/0006-local-model-settings.md),
[verification](../../tests/fixtures/model-settings/README.md).
