# Roadmap

mdoc prepares local documents as editable Markdown for lawyers using AI agents
in other tools. Direction and milestone order are agreed as of 2026-10-01.
The review contract is agreed; inline pseudonymization is implemented experimentally. The
[initial Rust/model qualification](tests/fixtures/pseudonymization/README.md)
has been performed on Apple Silicon macOS; no candidate passed qualification.
The user authorized the pinned GLiNER2 option experimentally despite its blockers.

## Next steps

1. **Documentation and developer workflow — complete** — consolidated into README, CONTEXT,
   selective ADRs, and fixture READMEs. Keep compilation speed and warm rebuilds;
   manual cache cleanup is documented in README. No automatic size limit or
   profile reduction is adopted.
2. **Copy Markdown — complete** — copies the complete current source, including
   unsaved edits and regardless of selection, without wrapper or metadata.
   Shows brief success feedback and keeps extraction warnings near the action.
3. **Pseudonymization qualification — initial baseline complete; adoption blocked** —
   [Rust evaluation tooling](tools/pseudonymization/README.md) compares three pinned
   exports on synthetic EN/RU legal/OCR and full Markdown source. Recorded category
   misses/false positives, Unicode offsets, long/stress behavior, latency, peak
   memory, download size, licenses/revisions and offline OCR-runtime compatibility.
   GLiNER2 passes same-process OCR checks but misses required RU/hidden-source spans;
   gline-rs also conflicts with OCR's Rust binding. Larger holdout and Windows
   qualification, reviewed numerical targets, and a suitable model/export remain
   prerequisites for adoption. See the [measured result](tests/fixtures/pseudonymization/README.md).
4. **Inline pseudonymization — experimental implementation** — the user explicitly
   authorized the pinned GLiNER2 model despite its quality/resource blockers. Uses the
   [accepted review contract](docs/adr/0002-reviewable-local-pseudonymization.md):
   explicit scan, subtle highlights, click/keyboard popup, exact-repeat grouping,
   immediate undoable acceptance, and in-memory document mappings. Explicit verified
   setup, whole-source offline scans, manual candidates/linking, cancellation and
   revision/identity checks are implemented. See [inline verification](tests/fixtures/pseudonymization/inline-review.md).
   Native popup/shortcut/IME acceptance and Windows operation remain unverified;
   experimental authorization does not qualify this checkpoint or accept the
   proposed numerical targets.

A measured responsiveness/resource blocker moves ahead of features. Keep the
app and reusable crates; refactor only where concrete ownership or testing
problems appear. Extraction/OCR algorithm improvements belong in the AnyDoc and
pdf-inspector forks, supported by output comparisons against source documents.

## Qualification and verification

Prioritize English/Russian PDF and DOCX on Apple Silicon macOS and Windows x64;
keep macOS, Windows, and Linux buildable. Model selection is a qualification
milestone, not a decision to make without measured evidence.

Use synthetic or explicitly suitable local fixtures, not distributed private
documents. Qualify mixed/native/scanned PDFs, legal Markdown, repeated/inflected
names, initials, contact details and identifiers, tables, links, code, OCR errors,
and long text. Test offline operation, setup integrity, cancellation/stale
results, tab switching, edit/undo, and source preservation. Native popup layout,
keyboard/IME interaction, and platform operation need native checks in addition
to the automated gate.

Performance measurements use fixed viewports and actual scroll/search positions.
Report build profile, machine, latency and memory; choose numerical budgets after
a baseline. Do not treat debug/headless timings as product guarantees.

## Model/library research — 2026-10-01

The discovery below led to the [measured qualification](tests/fixtures/pseudonymization/README.md).
Exact downloaded exports, runtime checks and results are recorded there; these
discovery descriptions alone do not establish suitability.

| Candidate | Reason to evaluate | Qualification concern |
| --- | --- | --- |
| [gline-rs](https://github.com/fbilhaut/gline-rs) + [GLiNER multi v2.1](https://huggingface.co/urchade/gliner_multi-v2.1) | Apache-2.0 Rust/ONNX inference and a multilingual model with configurable labels. | Exact export/tokenizer, EN/RU legal recall, Unicode offsets and long-text behavior. |
| [Knowledgator GLiNER PII](https://huggingface.co/knowledgator/gliner-pii-base-v1.0) | Apache-2.0 PII model with [published ONNX exports](https://huggingface.co/knowledgator/gliner-pii-base-v1.0/tree/main/onnx): about 197 MB quantized, 333 MB FP16, 665 MB FP32, excluding tokenizer/runtime. | Russian coverage, quantization quality and Rust inference compatibility; published synthetic benchmarks are not acceptance. |
| [gliner2-rs](https://github.com/dariofinardi/gliner2-rs) | Apache-2.0 Rust engine with PII vocabulary, long-text support and removable Hub/network feature. | Larger documented PII exports, model-specific runtime behavior, and actual CPU cost. |

[GLiNER multi PII v1](https://huggingface.co/urchade/gliner_multi_pii-v1/raw/main/README.md)
declares EN/FR/DE/ES/PT/IT, excluding Russian: its name does not establish RU
coverage. [OpenAI Privacy Filter](https://huggingface.co/openai/privacy-filter)
is a comparison candidate with primarily English, fixed categories and no
organization label in its published taxonomy. Presidio/Natasha were researched
but excluded from the shipped shortlist because their integration uses Python.

Reuse detection, but retain app-owned review, replacement mapping, undo, and
stale-result protection. No detector establishes guaranteed anonymization.

## Deferred

- Bulk-conversion redesign: keep current multi-file opening, lazy activation,
  bounded conversion, source preservation and inline OCR consent. Revisit in a
  separate grilling session; no eager processing/output workflow is chosen.
- Online formatting/OCR-text repair: provider, credentials, trust and review
  decisions belong in a future grilling session.
- Cross-document identity mapping, persistent/exported replacement maps, and
  stronger anonymization claims.
- Additional product ideas such as optional source/page references: discuss
  concrete workflows before committing scope.

## Design status

Design interview complete: the user confirmed shared understanding on 2026-10-01.
All active decisions are recorded in the glossary and ADRs. Initial qualification
produced measured blockers and proposed targets; model adoption and final numerical
acceptance targets remain open;
bulk redesign and online processing await their future grilling sessions.
Copy Markdown and experimental inline pseudonymization are implemented.
The GLiNER2 experimental exception was explicitly authorized on 2026-10-01;
qualified adoption still requires the evidence above.
