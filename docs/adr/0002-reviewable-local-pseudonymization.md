# Use reviewable local pseudonymization

Status: accepted direction and review contract, 2026-10-01. Exact library/model
selection and numerical acceptance targets await qualification evidence.

mdoc will assist replacement of identifying information locally using consistent
placeholders and user review. Any reversible mapping stays local and separate
from the Markdown being handed off; automatic detection does not establish that
the text is anonymous. Evaluate existing open-source libraries and models before
choosing an implementation, so the app does not grow its own detection engine.
The shipped integration must use Rust rather than a bundled Python worker or
Docker. Review is inline: highlighted candidates offer accept/decline popups;
exact model selection requires qualification.

## Accepted review contract

- Start from explicit Pseudonymize, scan in the background, then subtly highlight
  candidates in the editor. One compact popup opens by click or keyboard
  activation. Hover strengthens the highlight but does not open a popup.
- Match the existing palette/typography; support normal editing and Markdown find
  alongside review. Generic annotations/painted geometry belong in mdoc-editor;
  detection, review policy, grouping, and mapping belong in the app.
- Review one document's names, organizations, contact details, addresses, and
  identity/tax/bank identifiers. Preserve ordinary dates, amounts, and legal
  references unless explicitly selected. Allow manual additions and exclusions.
- Show a repeated candidate's occurrence count. Accept/Keep applies to all exact
  repeats by default, with an explicit single-occurrence option. Initials,
  inflected names, and other variants stay separate until the user links them.
- Accept applies one group immediately as one undoable edit. Keep changes no text
  and clears the candidate's highlight. Normal edits invalidate or revalidate
  affected candidates; stale revisions/offsets cannot replace the wrong text.
- Keep mappings and review decisions in memory per live document, including tab
  switches. Save only edited Markdown; closing the tab ends the mapping. No
  automatic mapping-file or restart persistence in v1.
- Setup explicitly downloads pinned, verified models into app-local storage.
  Scan offline, load when review starts, and release when idle. Ordinary document
  opening/editing does not depend on model availability. Qualify CPU use first.
- Use stable placeholders such as PERSON_1, ORG_1, and ADDRESS_1. The popup shows
  the original, proposed replacement, occurrence count, and Accept / Keep /
  Edit replacement. A selected span can be added manually or linked to an
  existing placeholder; linking variants always requires a user action.
- Scan everything Copy Markdown would transfer, including identifiers in link
  destinations, image paths, code, and HTML. Show hidden-source candidates beside
  their containing element and expose the relevant source fragment in the popup.
  Preserve Markdown syntax. Source files, filenames, and PDF/DOCX originals are
  outside replacement scope.
- Undo restores an accepted group and makes it reviewable again. Rescan refreshes
  candidates while retaining valid mappings and explicit exclusions. Leaving
  review hides highlights but preserves accepted edits and in-memory decisions.
- Cancel stops pending detection and rejects late results; it does not revert
  accepted edits. Show the remaining candidate count without blocking copy/save
  or claiming the document is fully anonymous.

## Qualification before implementation

Model/runtime selection and numerical quality/performance targets follow measured
English/Russian legal/OCR evidence, including per-category misses, false positives,
Unicode/source fidelity, long text, CPU latency, memory, download size, offline
behavior, and compatibility with the OCR runtime. Qualification provides evidence;
no library/model is selected by the design record. The agreed behavior
above is the product contract for that evaluation and subsequent implementation.

The [2026-10-01 qualification](../../tests/fixtures/pseudonymization/README.md)
implements a reproducible Rust harness and records an initial macOS comparison.
No candidate is adopted: required RU/hidden-source coverage remains insufficient,
and gline-rs has an OCR binding conflict. GLiNER2 passes same-process runtime
coexistence. Proposed numerical targets and unmeasured platform/lifecycle checks
remain separate from this accepted review contract.

## Experimental implementation exception — 2026-10-01

When implementing the inline milestone, the user explicitly chose “Use the pinned
GLiNER2 model experimentally despite its documented quality/resource blockers.”
This authorizes gliner2-rs 0.9.6 (Hub feature disabled) and the unmodified FP16
privacy PII export at `e594898629d452e8311796f5f329c7edbeda907c`, threshold 0.5,
CPU only, as experimental review assistance. The failed qualification and
proposed quality/performance targets remain unchanged. Windows operation and
native visual/shortcut/IME acceptance are still outstanding.

Setup is a separate explicit download action. Inference shares the existing
pinned ONNX Runtime without requiring OCR models or PDFium. One scan runs at a
time; each scan loads and drops its engine. Scans inspect full Markdown source,
use adaptive overlapping windows checked against actual schema/text token counts,
and reject rather than silently truncate pathological windows. Limits are 2 MiB
source, 512 tokens per inference window and a two-minute cooperative deadline.
Cancel rejects results immediately and stops at the next bounded checkpoint;
model load/native inference cannot be interrupted mid-call.

Review annotations and atomic revision-checked multi-range edits are generic
editor extensions, separate from find. The app owns review groups, exact-repeat
and single-occurrence exclusions, manual additions/linking, and stable mappings.
Conservative source revalidation prevents stale replacements. Tokens start with
a letter, end with a letter/number and use ASCII letters, numbers, underscores
and hyphens. HTML tag/attribute names, attribute quotes, list prefixes and link
delimiters are protected; spans crossing syntax or line boundaries require
manual narrowing. Original files and filenames
remain outside replacement scope. Mappings are never serialized.

[Inline verification](../../tests/fixtures/pseudonymization/inline-review.md)
records implementation checks and their limits. This exception permits
experimental use; it does not establish anonymization or qualified model adoption.
