# Inspect and select local model settings

Status: implemented, 2026-10-02. The user authorized implementation after the
completed design interview. [Verification](../../tests/fixtures/model-settings/README.md)
records offline operation of all four bundles on Apple Silicon macOS and the
remaining human/native and Windows checks.

Users need to inspect the current OCR and pseudonymization configuration and
compare model quality during QA. Current runtime integration pins one bundle
for each feature and exposes setup through feature-specific controls.

Provide one application-wide **Settings** dialog with OCR and Pseudonymization
sections, available without an open document. The action and contextual links
are named **Settings**, without an ellipsis. Application defaults apply to future
runs; results identify the actual configuration used rather than merely showing
the latest default.

Offer a curated list of supported, pinned model bundles with verified explicit
downloads. Keep installation state, language support and qualification status
separate. An installed or compatible model is not automatically qualified.
Arbitrary local-model loading is outside the first version.

Support isolated, sequential QA comparisons on identical unchanged input,
retaining separate results with configuration provenance and timing. This keeps
the comparison independent of edits or replacements accepted during another
run. Automated annotated-corpus scoring remains in existing developer tooling
initially.

Existing offline inference, explicit setup/OCR consent, source preservation,
reviewable pseudonymization and experimental-model limitations continue to apply.
OCR algorithms remain in dependency forks, with settings and QA orchestration
owned by the app.

## Initial catalog and controls

Use the existing Cyrillic OCR bundle and original PP-OCRv6 Small, with the
latter's known Russian failure visible. For pseudonymization, retain GLiNER2
FP16 and add the pinned FP32 export only after actual compatibility and resource
checks. Both remain experimental; FP32 is another precision of the same detector.
Keep current model defaults. Distinct detector engines are deferred.

Advanced settings expose OCR resolution/minimum recognition confidence and
pseudonymization threshold. OCR routing is fixed to Force, with no user mode
selector: explicit OCR recognizes every selected page. This changes the app's
current Auto routing; it does not remove explicit OCR consent or make document
opening invoke recognition. CPU execution and resource limits are read-only.
Provide Reset to defaults. Category/schema editing and GPU selection are deferred.

OCR resolution is 150/200/300 DPI (default 150). Minimum recognition confidence
is a finite value in 0-1 (default 0); pseudonymization threshold is a finite value
in 0-1 (default 0.5). Reset restores these values and the Cyrillic/FP16 model
defaults. Higher thresholds discard predictions; they are not accuracy scores.

## Change and result lifecycle

Use a compact dialog with two sections, a menu item, standard settings shortcut,
contextual links, Apply and Close. Persist applied defaults across restarts.
Capture configuration at the start of each job; running jobs and existing
results retain that configuration. Settings changes never rerun a document.

Compare models opens a separate view for a suitable active document. Capture PDF
bytes for OCR or the complete current Markdown for pseudonymization once. Run
configurations sequentially on that snapshot. Start with two runs and allow
another run against the same snapshot. Switchable read-only outputs show text or
detected spans, configuration, warnings and elapsed time. QA overrides change
neither application defaults nor the working document.

OCR comparison defaults to All pages, with an optional QA page range; scan full
Markdown for pseudonymization. Freeze page selection with the input. New comparison
captures fresh input. Switching documents does not change the snapshot. Closing
the comparison cancels queued/pending work and releases retained results/input;
an active native call may retain its input until the next checkpoint. Reject late
completions without reopening the view.

Expose Recognition output separately from Prepared Markdown because native/OCR
fusion can conceal recognition differences. Raw OCR output support belongs in
the converter fork, with page warnings/timings. Pseudonymization results expose
detected text/category/source location/confidence. Keep accuracy scoring in the
annotated-fixture harness; counts and elapsed time are the app's measurements.

Retain comparison results in memory until close. Explicit Export report includes
revisions/configuration, input hash, timings, warnings and recognition output or
predictions, without replacement mappings. No restart restoration or automatic
report saving. Allow separate Markdown export of OCR output.

Close/Escape discards unapplied preferences and restores focus. Apply persists
atomically; a save failure leaves previous active defaults and reports the error.
Missing settings use defaults. Invalid/obsolete settings require a notice and
explicit reset/supported selection instead of an unnoticed model substitution.

## Artifact management

Model selection changes preference only. Download is an explicit separate action.
Missing preferred models offer setup rather than silent fallback. Show model
name, language evidence, qualification status, installation state and download
size. Details reveals revision, precision, runtime, licenses and storage location.
Offer Repair and Remove for model files, preserving shared runtimes and preventing
removal while a model is in use.

Setup continues independently of Settings visibility; reopening shows its state.
Cancel download preserves completed verified files and discards incomplete files.
Use measurable byte progress, followed by Verifying/Checking runtime states rather
than invented percentages. Setup from Settings never initiates document processing.

## Admission and model availability

One comparison job executes at a time. QA inference does not overlap document
OCR or pseudonymization inference. Conflicting requests show busy/retry without
automatic queues; explicitly requested comparison runs execute sequentially.
Editing, viewing and saving stay available. Cancellation releases the execution
slot only once the underlying call stops.

An alternative failing runtime loading or existing execution limits remains
visible but disabled on the affected platform, with the measured reason. Complete
the infrastructure and working alternatives without silently changing the selected
model or relaxing limits. Runtime availability and experimental/quality status
remain distinct; known language failures stay visible.

## Acceptance

Require the full repository gate, focused settings/lifecycle tests, converter
tests for raw recognition output and actual offline runs of every enabled model.
Record model/runtime revisions, configuration, timings and peak memory with
machine/build details. Separately check native macOS layout, focus, shortcuts and
numeric input. Explicitly record outstanding Windows/native checks; headless
results do not establish native acceptance or platform runtime qualification.

The [complete design record](../design/local-model-settings.md) contains the
confirmed decision tree, defaults, implementation guidance and explicit deferrals.
