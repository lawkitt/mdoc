# Pseudonymization refactoring — design interview

Status: implemented 2026-10-08 on branch `refactor/pseudonymization` after the user
confirmed the shared understanding and requested full implementation. Behavior baseline: ADRs 0013, 0014,
0018, 0019, 0020 and the current workspace test suite.

## Findings (inspection, 2026-10-08)

~10.2k lines across three crate-root module trees in the `mdoc` binary:

| Tree | Lines | Role |
|---|---|---|
| `src/pseudonymization*` (domain) | ~2.6k | `Review` aggregate, identities, tracking journal, syntax protection, discovery |
| `src/pseudonymization_detector*` | ~0.9k | model manifests, scanning, structured rules |
| `src/pseudonymization_ui*` | ~6.7k (2k tests) | every file is `impl Workspace`; ~390 `self.pseudonymization.*` accesses |

Smells observed:

1. **God object.** All UI logic is `impl Workspace`, reaching through
   `self.pseudonymization.mapping.*`. State reset is hand-copied (e.g.
   `begin_review`, `select_identity`, `back_to_replacements`,
   `invalidate_source_edit` each reset overlapping flag subsets).
2. **Three interaction generations coexist.** ADR 0013/0014 candidate/applied
   popups (used when the panel is closed and in Anonymize mode), ADR 0019
   selected-item remnants (`Selection::Identity`, `back_to_replacements`,
   `actions_open`, `choosing_owner/category`, `replacement_commands`), and the
   ADR 0020 direct popup. `pseudonym_popup` dispatches between them on
   `mapping.open`.
3. **Large aggregate.** `Review` owns candidates, groups, identities, tracking,
   matcher, counters and `Mode`; Anonymize/Pseudonymize interplay rides on a
   `conversion` flag (33 references) and 31 `Mode::` branches.
4. **Anonymous tuples.** `ReplacementPlan` (4-tuple), `IdentifiedPlan`,
   `MappingPlans`, `direct_targets` returning a 5-tuple.
5. **Duplicated history plumbing.** snapshot → `history_id` →
   `checkpoint_metadata` → `pii_transaction` → mutate →
   `commit_identity_snapshot` repeated across ~20 sites.
6. **Vocabulary drift.** `pii`, `pseudonym`, `replacement`, `mapping`,
   `identity`, `review` used interchangeably in actions, modules and functions.
7. **Evidence bloat.** 2.3 MB of run evidence under
   `tests/fixtures/pseudonymization/results/` (lockfiles, logs, test dumps).

### Correction after round 1 fact check

Most suspected ADR 0019 remnants are live: `actions_open` is the panel's
"⋯ Scan and model actions" menu; `choosing_owner`/`choosing_category` toggle the
ADR 0020 popup's inline pickers. `Selection::Identity` is still produced after a
mapping change (`selected_after`, mapping.rs:483) and `back_to_replacements` is
called when toggling the panel. Little code is strictly dead; the smell is
three mutually exclusive booleans that should be one state enum.

## Decisions

Round 1 (2026-10-08) — user agreed with all recommendations. See
[ADR 0021](../adr/0021-pseudonymization-refactoring.md).

| # | Decision | Rationale |
|---|---|---|
| Q1 | Strictly behavior-preserving; every step passes the full suite and strict Clippy. Behavior changes need their own ADR. | Tests are the oracle; refactor and redesign don't mix. |
| Q2 | Superseded-design code is deleted, not kept as a fallback, after verifying reachability. | One interaction model in code. |
| Q3 | Keep `impl Workspace` as glue; move state and transitions into plain structs (`MappingUi`, `Popup`) with named transitions. No separate gpui entity. | Bloat is duplicated resets and gpui-free logic; entity extraction would redesign event/undo coordination. |
| Q4 | Move domain (`src/pseudonymization*`) to `crates/mdoc-pii`; detector stays for now (depends on `settings::PiiModel`). | Compiler-enforced "no gpui in domain"; isolated tests. |
| Q5 | ~~Superseded in round 2 by Q18.~~ Anonymize mode is permanent; model it as a rendering policy over the same identities, removing the `conversion` flag and scattered `Mode` branches. | One source of truth. |
| Q6 | Named structs replace plan/target tuples; one helper wraps the identity-history transaction. | Cheap, low-risk. |
| Q7 | Umbrella name `pii` (`pii`, `pii::ui`, `pii::detector`); glossary terms inside; actions renamed to match. | Covers both modes; aligns with CONTEXT.md. |
| Q8 | Keep ADRs/design docs; move run evidence to `docs/evidence/pseudonymization/`, keeping only per-run READMEs. | Evidence isn't test input; git keeps history. |
| Q9 | Incremental green commits: dead code → Q6 → Q3 → Q5 → Q4 → Q7 renames last. | Reviewable diffs. |

Round 2 (2026-10-08) — user agreed with Q10–Q17 and removed Anonymize mode.

| # | Decision | Rationale |
|---|---|---|
| Q10 | No standalone dead-code step. `Selection::Identity` → `Selection::Entity`; `back_to_replacements` → `MappingUi::clear_selection()`. Q2 still applies to anything proven unreachable. | Both are reachable; deleting would change behavior. |
| Q11 | Popup sub-state flags become `enum Picker { None, Alias, Category, Owner }` if tests show they are mutually exclusive; `actions_open` stays a panel flag. | Removes impossible states. |
| Q12 | One shared popup frame (anchoring, focus, Escape, scroll, one control focus map); popup contents stay separate. | Removes duplicated `popup_controls`/`mapping.controls`. |
| Q13 | `mdoc-pii` splits `Review` into `Candidates`, `IdentityStore`, `Tracking` behind a thin `Review` façade. ~~`GroupKind`/`Mode::placeholder`~~ dropped by Q18. | Separates concerns behind one entry point. |
| Q14 | `mdoc-pii` fields private; read accessors and named mutating commands only. | Makes the crate boundary meaningful. |
| Q15 | gpui integration tests are the oracle: only paths/names change, never assertions (except Q18's removal). Domain tests move with the crate. | Proof of preservation. |
| Q16 | Renames touch Rust identifiers and module paths only; UI labels and serialized settings keys unchanged. | No user-visible or persisted-data churn. |
| Q17 | No line-count target. Done = all steps landed, UI mutates mapping/popup state only through methods, `mdoc-pii` builds without gpui. | Structural, checkable criteria. |
| Q18 | **Behavior change:** remove Anonymize mode entirely; Pseudonymization is the only and default PII behavior. Recorded separately in ADR 0022. | User decision: one mode. |

Round 3 (2026-10-08) — user agreed with all recommendations.

| # | Decision | Rationale |
|---|---|---|
| Q18a | Toolbar click scans and opens the Replacements panel with proposals; explicit Apply. | Review-before-edit is the trust model (ADRs 0019/0020). |
| Q18b | Single icon (existing artwork): no review → scan; review with panel closed → reopen without rescanning; panel open → close. Rescan stays in panel "⋯". Keep Cmd/Ctrl+Shift+P; remove Cmd/Ctrl+Shift+A and the Anonymize menu item. | No mode menu remains to host the toggle. |
| Q18c | Delete marker→alias upgrade code and its two tests; leftover markers become plain text. | Marker provenance is live-tab only and no new markers can arise. |
| Q18d | Port mode-agnostic anonymization tests (stale/failed/cancelled scans, one undo step, offline smoke, syntax protection) to Pseudonymize; delete marker-specific ones. | Keep scan-safety coverage in the oracle. |
| Q18e | **Behavior change:** one popup. Clicking a highlight with the panel closed opens the panel with the ADR 0020 direct popup; legacy candidate/applied popups removed; the overlap chooser stays. Q12's shared frame largely becomes moot. | One interaction model. |
| Q18f | ADR 0022 work lands first, as the only commits that change test assertions. | Shrinks refactor surface; keeps Q15 clean. |
| Q18g | Same commit: ADR 0013 superseded, ADR 0014 marker notes updated, CONTEXT.md drops **Anonymization mode** (_Avoid_: Anonymize under Pseudonymization), README/ROADMAP updated. | Docs match code. |
| Q7′ | Keep `pii` as the umbrella code name; "Pseudonymization" remains the feature/glossary term. | Still covers detector and model settings. |

## Final sequence

1. ADR 0022: remove Anonymize, single popup, toolbar toggle, test port, docs (Q18*).
2. Mechanical: named structs, identity-history helper (Q6).
3. UI state structs, `Selection::Entity`, `clear_selection`, `Picker` enum (Q3, Q10, Q11).
4. Split `Review` into `Candidates`/`IdentityStore`/`Tracking` façade with private fields (Q13, Q14).
5. Move domain to `crates/mdoc-pii` (Q4); move evidence to `docs/evidence/` (Q8).
6. Renames to `pii` vocabulary (Q7, Q16).

Implementation-time checks: verify `Picker` flags are mutually exclusive; check
whether `manual_review` and `ScanIntent` collapse once Anonymize is gone.

## Deferred

Detector extraction into the crate (blocked on `settings::PiiModel`); toolbar
icon artwork.

## Implementation (2026-10-08)

Six commits, each passing `cargo test --workspace` (437 passed, 16 ignored),
strict Clippy and `cargo fmt --check`:

1. ADR 0022: Anonymize removed; single icon toggle; one word popup; docs.
2. `ReplacementPlan` struct, `Review::pending_plans`/`alias_corrections`,
   `checkpoint_review`/`commit_plans` history helpers, `AliasTarget`.
3. `MappingUi`/`ReviewUi` named transitions, `Selection::Entity`, `Pickers`.
4. `Candidates` extracted; `Review` fields private behind accessors/commands.
5. `crates/mdoc-pii` (+ `crates/mdoc-history`); evidence to `docs/evidence/`.
6. `pii` vocabulary: `src/pii/{detector,ui}`, `Workspace::pii`, `Pii*` actions,
   `Variant`/`Candidate` domain types, `panel`/`popup` module names.

Pseudonymization code went from 10,222 to 8,511 lines (tests 2,638 → 2,331),
including the moved 128-line `mdoc-history`.

### Decisions taken during implementation

- **Q11**: the alias, owner and category pickers can be open together today,
  so per Q11 they became one `Pickers` value with three flags, not an enum.
- **Q12**: not implemented. After Q18e only the word popup and the field
  chooser remain; they share no duplicated frame worth extracting.
- **Q13**: `GroupKind`/`Mode::placeholder` became unnecessary once Anonymize was
  removed. `Review` delegates to `Candidates`, `IdentityStore` and `Tracking`.
- **Q17 / Q4 conflict**: `mdoc-pii` needed `EditorTransaction`/`SourceEdit` from
  `mdoc-editor`, which depends on gpui. The gpui-free transaction module was
  lifted into the dependency-free `crates/mdoc-history`; `mdoc-editor`
  re-exports it at the same paths. `cargo tree -p mdoc-pii` shows no gpui.
- **Q8**: "drop raw files" was narrowed to uncited build/test/clippy logs,
  lockfiles and process dumps. Files linked from docs or read by documented
  qualification commands moved with the READMEs. The frozen hybrid
  `comparison.json` is test input and moved to
  `tests/fixtures/pseudonymization/hybrid/`. Historical `provenance.json`
  records keep their original paths and hashes unchanged.
- **Q7 scope**: glossary alignment extended to domain types: `Group` → `Variant`
  (the domain's own error text already said "Variant"), `CandidateOccurrence`
  → `Candidate`. Debug selectors and key-context strings are unchanged.
- Dead code found on the way: `Review.skipped_syntax_spans` (written, never
  read after the anonymization bar), `Review::mappings`, `ui::replacement_transition`,
  the chevron icon, the Alt+K binding (no handler in the word popup).

### Behavior notes beyond ADR 0022

- The toolbar icon stays enabled during a scan so it can hide/show the panel.
- Apply after a mapping change restores the staged identity policy in every
  stale-candidate case; previously one unreachable branch returned without it.
- Test assertions changed only in commit 1 (ported to the Pseudonymize flow;
  two marker-only tests deleted). Later commits changed paths and names only.

### Not verified

No native macOS run of the app was performed for this refactor; coverage is the
headless gpui suite (including drawn-layout and click tests in both themes).
