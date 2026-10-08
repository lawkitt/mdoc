# Pseudonymization production readiness — 2026-10-08

Status: first-journey identity review authorized and implemented experimentally
on 2026-10-08, following selected Rust Presidio reuse. The user's subsequent
“Proceed implement everything else” selects Q7–Q11's conservative recommended
workflow, recorded in ADR 0018. Production qualification gates remain open.
Existing accepted behavior remains in force until explicitly amended.

## Purpose

Inspect current pseudonymization/anonymization behavior and agree what evidence
and product changes are required to leave experimental status. The interview
initially mapped the design tree before implementation. The subsequent broad
implementation request authorizes the conservative first-journey workflow; it does
not establish numerical release gates or permit a production-quality claim.

## Current implementation, inspected in this session

- Both modes share the app-owned review/mapping/tracking domain and detector.
  Pseudonymize proposes numbered category placeholders and requires explicit
  acceptance. The default Anonymize action applies shared category markers in
  one undoable batch. Secondary Anonymize review proposes without applying.
- Explicit setup installs pinned size/hash-verified GLiNER2 FP16 or FP32 artifacts.
  Scans run locally with a captured configuration, four CPU inference threads
  and the shared ONNX Runtime. Each scan loads and drops its engine; inference
  and model operations have admission guards.
- Full current Markdown source is scanned, including hidden destinations, image
  paths, code and HTML. Source files and filenames are outside replacement scope.
  Actual schema/text tokenizer bounds constrain overlapping windows to 512 tokens.
  Source size is capped at 2 MiB and checkpoints enforce a cooperative 120-second
  deadline. Native inference/loading cannot be interrupted mid-call.
- Detection combines model output with email and contextual checksum-valid
  INN/SNILS rules. Rules run after model inference, so current automatic scanning
  still requires a functioning installed model; there is no independent rules-only
  fallback. Rule rejection does not veto model output.
- Ingestion validates UTF-8 byte spans, prioritizes rule evidence in conflicts,
  protects Markdown syntax and rejects unsuitable spans. Exact repeated strings
  expand to candidate groups. Different forms/initials are linked manually;
  this does not perform general identity resolution. Group lookup is by exact
  original string rather than a resolved real-world identity.
- Acceptance uses current-source/revision checks and generic editor transactions.
  Late, cancelled, stale, failed or mode-switched scan results cannot auto-apply.
  Keep/exclusions and live mappings survive rescans and tab switches.
- Applied occurrences have explicit before/after provenance, clickable inspection,
  restoration and metadata-aware undo/redo. Marker text alone does not establish
  provenance. Originals/mappings are live-memory state; Save/Copy contain current
  Markdown and do not serialize restoration data. Reopening loses the mapping.

Primary code: `src/pseudonymization.rs`, `src/pseudonymization/{discovery,tracking,syntax}.rs`,
`src/pseudonymization_detector.rs`, `src/pseudonymization_detector/{structured,windows}.rs`,
`src/pseudonymization_ui.rs`, `src/pseudonymization_ui/{scan,render,applied}.rs`.
Existing contracts: ADRs 0002, 0013, 0014 and `CONTEXT.md` glossary.

## Evidence and limits

Existing recorded measurements were inspected; no inference or native acceptance
checks have been rerun in this interview.

- The 2026-10-01 qualification adopted no model. Subsequent implementation was
  explicitly authorized experimentally, without accepting numerical targets.
- The 2026-10-07 selected hybrid research pipeline recorded 87 exact true positives,
  35 false positives and 19 misses against 106 gold mentions across 33 small
  synthetic fixtures: precision 71.3%, recall 82.1%. RU alone: 34 TP / 16 FP /
  13 misses. Organization and address recall were each 50% on very small samples.
  These are detector research results before app syntax filtering and repeat
  expansion, not qualified end-to-end replacement or representative-document scores.
- Structured context looks backward on the same line; table headers and labels
  such as 'ИНН физлица' are outside the supported rule patterns. Damaged identifiers
  can remain sensitive even when their checksums fail.
- Recorded release layout measurements include roughly 3.4 seconds per dense
  wrapped hidden-source frame at 2 MiB/20,000 occurrences, mostly base editor
  cost. Efficient PII tracking does not qualify whole-editor responsiveness.
- Recorded macOS restoration/native checks and automated safeguards are valuable
  but do not establish full IME/accessibility or Windows/Linux runtime acceptance.

Evidence: `tests/fixtures/pseudonymization/README.md`,
`tests/fixtures/pseudonymization/results/2026-10-07-hybrid/measurements.md`,
`tests/fixtures/pseudonymization/results/2026-10-07-restoration/README.md`.

## Design tree before the broad implementation authorization

Q1–Q4 were confirmed with "Agree". The user then requested product brainstorming
and described anonymization as PII removal without restoration for AI handoff.
Resolve the product-purpose branch and clarify that restoration distinction
before selecting technical release gates. Unconfirmed recommendations remain proposals.

1. Q1: Production product promise and intended handoff risk — confirmed.
   - Q5: Confirmed two AI-analysis journeys; focus initially on external AI analysis
     without returning to mdoc. A return journey is future scope, not selected drafting.
   - Q7: Which identity associations may happen automatically, and which require
     confirmation? Open.
   - Q8: Whether contact/identifier ownership is first-release relationship scope.
     Open; ownership must not be mistaken for identity equivalence.
   - Q9: Neutral category aliases versus role labels as default output. Open.
   - Q10: Identity panel plus inline assignment versus popup-only correction. Open.
   - Q11: Proposed-map review before replacement versus immediate application. Open.
   - Detection vs end-to-end success metrics and critical failure definitions.
   - Human review obligations, automatic Anonymize policy and completion wording.
   - Release gates and the evidence required to remove experimental status.
2. Q2: Initial qualification envelope: languages, documents and platforms — confirmed.
   - Representative corpus, independently reviewed holdout and per-cohort reporting.
   - Required identifiers, OCR damage, names/variants and negative examples.
   - Model/rule strategy, runtime isolation and measured resource budgets.
   - Native UI, accessibility, lifecycle and platform acceptance matrix.
3. Q3: Sanitization boundary: current Markdown versus other outbound artifacts — confirmed.
   - Q6: Anonymize correction/restoration versus deliberate forgetting of originals.
     Open; user expressed a product understanding, not an implementation instruction.
   - Hidden source, source preservation, export/copy behavior and metadata handling.
   - Residual-risk presentation and scan/review staleness after edits.
4. Q4: Identity consistency scope: live document, reopened document or matter — confirmed.
   - Variant linking, ambiguous identical strings and relationship fidelity.
   - Mapping/restoration lifetime, persistence, storage protection and recovery.

After root answers, recompute the frontier. Independent branches can be asked
together; no dependent decision is silently fixed in the same round as its
unsettled prerequisite. Record confirmed decisions and rationale each round;
create an ADR when durable choices are confirmed and update the shared glossary
when terminology is agreed. Final shared-understanding confirmation is required.

## Round 1 proposals

- Q1 recommendation: qualified local preparation assistant with measurable
  identification coverage, reliable replacement/restoration and explicit final
  user review; no claim that the subject cannot be identified from context.
- Q2 recommendation: EN/RU legal PDF/DOCX-derived and directly opened Markdown,
  native and OCR text; Apple Silicon macOS and Windows x64 required for the first
  qualified release. Linux remains buildable with automatic-scanning availability
  stated separately. Confirm before choosing numerical targets or support claims.
- Q3 recommendation: current full Markdown handoff only, with clear treatment of
  hidden source; original PDFs/DOCX, filenames, attached files and their metadata
  remain outside sanitization scope. Wider artifact cleaning needs an explicit
  product scope decision.
- Q4 recommendation: stable identities within one live document, with explicit
  variant linking; retain current in-memory mapping lifecycle for the first
  qualified release. Matter-wide and persistent mappings require separate choices.

## Confirmed decisions

Round 1 response: "Agree. But let's brainstorm more."

- Q1: Qualify a local preparation assistant with measured identification coverage,
  reliable replacement behavior and explicit final review. Do not promise that
  remaining context cannot identify subjects. Restoration behavior is being
  clarified separately after the user's new anonymization framing.
- Q2: First qualified release covers EN/RU native and OCR-derived legal text,
  PDF/DOCX-derived and directly opened Markdown, on Apple Silicon macOS and
  Windows x64. Linux remains buildable; automatic-scanning support is stated
  separately. This is a target, not evidence that the platforms are qualified.
- Q3: Sanitization covers the complete current Markdown handoff. Original source
  files, filenames, attached files and their metadata remain outside this scope.
- Q4: Stable identity scope is one live document, with explicit variant linking
  and the existing live-memory mapping lifetime. Persistent or matter-wide maps
  remain deferred.

Rationale: bounded claims, whole-handoff coverage and one-document mapping allow
meaningful qualification without silently adding artifact cleaning or sensitive
mapping persistence. Durable direction is recorded in ADR 0016.

## Product brainstorming — after Round 1

Pseudonymization's core value is retaining distinctions and references while
hiding identifying values: different parties get different aliases, and mentions
of the same party use the same alias. Restoration is a separate workflow choice.
The current exact-string grouping/manual linking is a starting implementation,
not a complete identity-resolution guarantee.

Candidate workflows, not yet selected:

1. Analysis handoff: review parties/aliases, copy pseudonymized Markdown to the
   external AI tool, then interpret answers expressed using those aliases.
2. Drafting return path: paste AI-generated clauses back, review alias occurrences
   and restore selected real values locally. Current restoration handles tracked
   original-document occurrences only; pasted/generated text has no provenance.
   A return path needs explicit token/ambiguity/import rules and release evidence.
3. Matter-wide continuity: reuse aliases across documents or sessions. Deferred
   by Q4; keep as a future product branch rather than first-release scope.

Q5 recommendation: select analysis handoff as the first production job. Establish
useful identity consistency and review before expanding to AI response restoration.

Q6 recommendation: interpret "without restoration" as no restoration step in the
ordinary anonymization journey, while keeping correction/undo during preparation.
If deliberate forgetting is desired, define its effect on original documents,
editor undo, mappings and process memory in a later dependent round; deleting a
mapping alone cannot establish local erasure.

## Confirmed journey focus — Round 2

The user selected: (1) prepare a document for external AI analysis without bringing
it back to mdoc, and (2) prepare a document for AI analysis with bringing it back.
Focus initially on journey (1). This confirms Q5's initial priority, while retaining
the return journey as later scope. It does not settle Q6's deliberate-forgetting
question or automatically select an AI-drafting workflow.

Rationale: identity relationships must survive the outgoing Markdown for useful
external analysis. A return pipeline is unnecessary to deliver that initial job.
The shared glossary and ADR 0016 record the two journey definitions.

## Identity and correction UX brainstorming — Round 3 proposals

Current code evidence:

- Workspace resets to Anonymize, which produces shared PERSON/ORG markers.
  Pseudonymize already proposes PERSON_1/PERSON_2, but is a secondary mode.
- Review stores one group per exact original string. Explicit linking currently
  assigns the same replacement string to separate groups, without creating an
  identity aggregate. `mappings()` deduplicates by alias and displays only one
  original per alias. Current provenance retains exact originals per occurrence.
- The candidate popup has a hidden list of existing placeholders; selecting one
  edits the replacement draft. The applied popup offers restoration, without a
  direct reassignment, identity merge, split or alias-wide rename control.
- Exact-repeat discovery is lexical. Equal surface forms cannot distinguish
  homonyms, and inflection/initials/OCR variants require manual linking today.

Recommended conceptual model, unconfirmed:

- Stable document-local identity owns an alias and name variants; every source
  occurrence has a separate assignment and original text. Alias renaming changes
  display/output, not identity. Keep occurrence provenance and editor history.
- Distinguish sameness (two mentions refer to one person) from ownership (a phone
  or email belongs to that person) and affiliation (a person represents a company).
  They are different relationships, with separate evidence and corrections.
- Protect user-confirmed grouping/splitting on rescan. Uncertainty remains visible;
  lower-confidence evidence must not overwrite confirmed assignments.
- Stable aliases never renumber after a merge/split. Alias-wide actions update only
  assigned tracked occurrences; pasted lookalikes do not acquire provenance.

Automation hypotheses to qualify, not guarantees:

- Group unambiguous full exact repeats; normalize spacing/case/typographic quote
  differences for comparison, preserving original source bytes and company legal
  form distinctions. Equal text remains splittable by occurrence/context.
- Russian inflections, initials, shortened names and OCR variants produce ranked
  association suggestions using context and contradiction checks. Weak character
  similarity or a matching surname alone is insufficient for automatic merging.
- Defined company short names and repeated details in clearly scoped party blocks
  can provide stronger document evidence. This needs measured EN/RU legal fixtures;
  a better span detector alone does not prove correct identity association.

Proposed UX:

- Compact optional Identities panel alongside Markdown, with alias, representative
  local original, variant count, mention count and an attention indicator.
- Expanded identity shows all variants and contextual occurrence snippets; selecting
  a snippet navigates/highlights it in the editor. Keep original values local.
- Inline selection offers Assign to existing identity (searchable), New identity,
  and Keep original. Applied occurrences can be reassigned directly without restore
  followed by another scan.
- Panel actions: Merge identities, Separate selected mentions/variant, Rename alias.
  Show affected counts, preserve confirmed decisions and make each action undoable.
- Review attention focuses on contradictory/ambiguous associations and possible
  missed variants. No attention count implies full detection coverage.

Current frontier, recommendations only:

- Q7: Conservative automatic association with ambiguous variants shown as suggestions.
- Q8: Include locally reviewed contact/identifier ownership; infer only from clear
  document evidence. Defer outbound owner-encoded alias/legend format until this
  relationship scope is selected. Never assign attributes by nearest name alone.
- Q9: Default neutral PERSON_1/ORG_1 aliases, with user renaming; do not infer legal
  roles as alias names automatically.
- Q10: Optional identity panel plus inline assignment, with keyboard navigation and
  narrow-window handling to follow after surface selection.
- Q11: Stage the proposed identity map against original text, allow bulk application
  after review, then retain the same correction tools for applied aliases.

Qualification must measure both mistaken joins and mistaken splits, wrong-owner
assignments if selected, boundary/category misses and user correction time. Detection
precision/recall alone cannot qualify relationship-preserving output.

External research checked on 2026-10-08:
[Presidio's mapping example](https://presidio.dataprivacystack.org/samples/python/pseudonymization/)
keeps a mapping per entity type/source value. It supports separating mapping from
replacement but does not establish inflection/homonym resolution. This is source
research, not approval to adopt Python/Presidio or change the app's model/runtime.

The user subsequently requested Rust Presidio implementation research before
answering Q7–Q11. The [source and compatibility investigation](rust-presidio-reuse.md)
confirms reusable rules/validators but no qualified identity/ownership resolver
in the inspected Presidio-style libraries. It identifies a separate Rust CorPipe
port with model-license/resource barriers. The user then approved the recommended
selected-rule reuse and requested implementation. The narrow structured adapter
is implemented experimentally under [ADR 0017](../adr/0017-selected-rust-presidio-reuse.md),
with exact selected-rule corpus regression and workspace/release/offline checks
recorded in the research note. The subsequent broad implementation request selects
Q7–Q11's conservative recommended defaults; see [ADR 0018](../adr/0018-document-local-identity-review.md).

## Implemented first journey — 2026-10-08

Stable identities, lexical normalization, person-variant suggestions, staging,
inline/panel assignment, merge/split/rename/category correction, manually reviewed
ownership and optional alias-only copy legend are implemented. The existing
Anonymize default is preserved; Pseudonymize stages originals, and Apply aliases
can upgrade existing tracked shared markers. See the ADR for exact automation,
lifetime and undo boundaries. Defined company shorthand, OCR linking and automatic
ownership remain evidence-dependent proposals. Q6 deliberate forgetting and the
numerical production gates are not selected by this implementation request.

The historical open questions and proposals above record the interview chronology;
ADR 0018 is the current implemented first-journey decision. No experiment has yet
established production detection or identity-association quality.

Current selected product decisions:

| Question | Implemented choice |
| --- | --- |
| Q7 | Same-category exact/normalized lexical defaults; ambiguous initials/inflections require confirmation and remain splittable. |
| Q8 | Explicitly reviewed contact/identifier ownership, separate from sameness; optional alias-only outgoing legend. |
| Q9 | Stable neutral category aliases with user renaming. |
| Q10 | Searchable optional identity panel with variants/context and direct inline entry. |
| Q11 | Stage identity proposals; apply aliases in one undoable batch; correct tracked replacements afterward. |

Production release gates remain open. The first-journey implementation and its
verification are not evidence of qualified automatic identity association.
