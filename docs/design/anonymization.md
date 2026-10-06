# Anonymization interview — 2026-10-06

Status: shared understanding confirmed and implementation authorized by
"ok, implement" on 2026-10-06. Implemented locally; verification is recorded
below. The design frontier is empty.

## Request

Quickly grill on implementing Anonymization alongside existing Pseudonymization.

## Current implementation evidence

- Pseudonymization uses local experimental GLiNER2 detection and inline review.
- Eight categories are supported: person, organization, email, phone, address,
  identity, tax identifier, and bank details.
- Accepted replacements use stable numbered tokens, such as PERSON_1.
- Review retains original strings and mappings in live-document memory; editor
  history supports undo. Save/Copy hand off the current Markdown only.
- Full Markdown source is inspected, including hidden destinations/code/HTML,
  subject to source-syntax protection. Original attachments and filenames remain
  outside replacement scope.
- Model qualification remains incomplete. Changing replacement policy alone
  does not improve detection coverage or remove identifying context.

Evidence: CONTEXT.md, ADR 0002, ROADMAP.md, src/pseudonymization.rs,
src/pseudonymization_ui.rs, and src/pseudonymization_detector.rs.

## Confirmed decisions — Round 1

Q1: use shared category markers, such as PERSON and EMAIL, without identity
numbering. Different people receive the same PERSON marker. The initial scope
is reviewable identifier removal rather than contextual generalization.

Q2: edit the current Markdown and retain undo; handoff uses existing Copy
Markdown and Save. A separate output-copy workflow is not introduced.

Rationale: reuse the current document preparation and review flow, while
providing an alternative to identity-linked numbered pseudonyms.

Undo necessarily retains earlier identifying text in the live editor. This
direction concerns Markdown handoff, not deletion of local originals/history.

## Design tree and current frontier

1. Meaning of Anonymization — settled, Q1.
   - Existing category scope, local detection and review contract — settled, Q3.
   - Fixed shared markers versus custom replacements — settled, Q4.
   - Completion claims: shared markers cannot establish anonymous text.
2. Output target — settled, Q2.
   - Existing accepted pseudonyms and switching modes — settled, Q5.
   - Activation/control placement — settled with user amendments, Q6.
   - Fast-path apply behavior — settled, Q7.
   - Clipboard coupling — settled, Q8.
   - Review state, rescan, source syntax and verification follow the settled
     choices and the implementation invariants below.

## Confirmed decisions — Round 2

The user answered: "Agree, Q6 icon button with anonymous person, default to
Anonymize, it should be much more simple then Pseudonymize to quickly eraze
personal data PII and copy markdown".

Q3: retain eight existing categories, local experimental GLiNER2, whole-source
scan, inline Accept/Keep/manual additions/Accept all and existing safeguards.
Dates, amounts and identifying context receive no new automatic treatment.

Q4: fixed category tokens PERSON, ORG, EMAIL, PHONE, ADDRESS, IDENTITY, TAX and
BANK. Anonymization has no custom identity-linked replacements or variant
linking UI; ordinary Markdown editing remains available.

Q5: switching modes changes pending proposals only. Offer recognized accepted
pseudonyms from the live document's review state as reviewable conversion
candidates; never rewrite accepted text solely on mode selection. Arbitrary
tokens from reopened files need manual selection because there is no persisted
mapping proving their provenance.

Q6, amended by the user: an icon button with an anonymous-person symbol and a
small mode menu; new documents default to Anonymize. Mode remains per live
document rather than an application-wide persisted setting. Provide
action-specific tooltip and accessible naming for the icon.

Product intent: make Anonymize substantially simpler than Pseudonymize, for
quick PII removal and Markdown copying. The existing review tools remain available as secondary controls, following Q7.

## Confirmed decisions — Round 3

Q7: the primary Anonymize action scans and applies all eligible detected PII
and known live-document pseudonyms using fixed shared markers as one undoable
batch. Respect existing Keep exclusions; manual review and additions are
secondary. Run completion must still validate document identity, revision,
mode and cancellation; errors and stale/partial results apply no batch.

Q8: retain Copy Markdown as an explicit separate action, enabled as usual.
Anonymize does not automatically copy or claim all PII was found. The compact
completion message states the actual replacement count and prompts checking
the remaining text. Existing explicit model setup and experimental status
remain accessible without expanding the normal flow.

The user confirmed both recommendations and authorized implementation with
"ok, implement". See [ADR 0013](../adr/0013-reviewable-anonymization.md) and
terminology in CONTEXT.md.

## Implementation invariants and verification criteria

- Share the current detector, model settings and pinned runtime. No additional
  model, dependency, online inference or automatic setup is introduced.
- Default each newly opened/replaced document to Anonymize; retain the selected
  mode on tab switches. The menu changes proposals and cancels an active scan,
  without applying edits. Preserve pseudonym-mode custom tokens and Keep decisions.
- The icon runs the selected action. Pseudonymize retains explicit review;
  Anonymize applies its successful scan as one revision-checked batch. Rescan
  within secondary review refreshes candidates without automatically applying.
- Keep conversion provenance only for accepted pseudonyms in the live document.
  Switching exposes conversion candidates; invoking Anonymize accepts eligible
  candidates with the rest of the successful scan. Reopened files do not infer
  provenance from token shapes. Shared category markers remain unchanged on rescan.
- Respect exact-repeat boundaries, group/single-occurrence exclusions and
  existing Markdown protection. Preserve original attachments, filenames,
  document identity, warnings and ordinary editing/undo/dirty behavior.
- Show compact progress/cancellation, actual replacement count and experimental
  coverage reminder. Errors provide Settings access and manual Review; secondary
  review omits replacement editing and variant-linking in Anonymize mode.
- Leave Copy Markdown separate and preserve its current semantics. Neither
  completion nor zero remaining candidates establishes complete PII detection.
- Check shared-marker policy, accepted-token conversion, mode transitions,
  exclusions and syntax, production completion/undo/copy/save, stale/failed
  results, icon/menu/manual-review routing, and new-document defaults. Run fmt,
  workspace Clippy/tests and build. Verify the installed local model on synthetic
  text and native macOS interaction where available; report other platform gaps.

## Verification — 2026-10-06

Formatting, diff checks, workspace Clippy with warnings denied, the development
build and all workspace tests pass: **399 passed, 12 probes ignored**. Existing
pseudonymization regressions remain covered. Five new routine checks cover shared
markers, known accepted-token conversion versus reopened files, Keep/syntax,
mode transitions, one-step undo/redo, explicit copy/save and original preservation,
failed/invalid/cancelled/stale/mode-switched results, icon/menu routing in both
themes at 640×480, simplified manual popup and new-document defaults.

The additional ignored `anonymization_installed_model_offline_smoke` was run
explicitly against installed verified FP16 artifacts and ONNX Runtime, with no
downloads. The production Anonymize flow made three replacements in 5.83 seconds
for synthetic English text, yielding:

```text
PERSON represents ORG Contact EMAIL.
```

This is a runtime/integration smoke check, not PII recall qualification.

Native macOS testing used a temporary bundle of the development build and a
synthetic Markdown file, with the original session backed up. Verified the
anonymous-person icon, real offline scan, compact three-occurrence completion,
visible experimental reminder, and one-step Undo restoring the complete source.
A native focus issue was found and fixed: starting Anonymize now focuses the
editor, so Undo works immediately after the icon action. The final build verified
this behavior, keyboard Tab/Enter selection of Pseudonymize without text edits,
return to Anonymize, secondary Review, fixed PERSON popup without replacement
editing/linking controls, and Escape closing the popup.

The QA app was stopped and removed, the original saved session restored
byte-for-byte, and Settings and the source fixture remained unchanged. Native
Copy/clipboard, IME, narrow/light-theme acceptance, broad accessibility and
Windows/Linux operation remain unverified. Automated checks cover copying and
both themes at narrow size. Detection qualification gaps from ADR 0002 remain
unchanged; retained originals/history and identifying context are outside this
feature's removal scope.

## Maintenance cleanup — 2026-10-06

The user requested a maintainability cleanup after implementing the agreed
feature. Inspection found scan/edit handling interleaved with large rendering
methods, duplicated replacement-commit bookkeeping, and a boolean scan flag
whose meaning depended on a separate mode field.

The controller remains in `src/pseudonymization_ui.rs` with ReviewUi as the sole
state owner. Private `render.rs` holds toolbar/status/popup rendering and
`tests.rs` holds the existing production-shell regression suite. Imports now
name the application's dependencies explicitly. All six moved rendering
function signatures/bodies match the prior staged source as Rust tokens except
optional trailing argument commas and effective crate-level visibility.

`ScanIntent::Review(mode)` versus `ScanIntent::Anonymize` now captures whether a
successful scan merely proposes or applies changes. An automatic pseudonym-mode
scan cannot be represented. Popup and bulk/automatic acceptance share the same
revision-checked text commit, accepted-mapping update and exact Keep-exclusion
rebasing helper. Entry-point focus/notification/cancellation behavior is retained.
No feature, reusable crate, dependency, settings format or document workflow was
removed or changed.

A new regression confirms that a secondary review scan in Anonymize mode changes
no text/history until explicit acceptance, after which one Undo restores the
source and candidate. The focused PII suite passes (25 passed, 2 probes ignored).
Workspace Clippy with warnings denied, build and the full workspace tests pass:
400 passed, 12 probes ignored. Formatting and whitespace/link checks also pass.
The installed-model integration probe was explicitly rerun offline: it passes
with the same three replacements and shared-marker output as before the cleanup.

This cleanup does not change detection quality or establish new native/platform
acceptance. Native evidence and remaining limits in the preceding section remain
applicable. Existing staged feature changes are preserved; this maintenance
follow-up is a separate local diff.
