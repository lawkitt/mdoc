# Roadmap structure and 0.1.0 stable scope

Status: accepted, 2026-10-16. The user agreed to the decisions below during
roadmap grooming; [ROADMAP.md](../../ROADMAP.md) applies them.

## Problem

ROADMAP.md kept three parallel lists (Next steps, Known issues from
0.1.0-beta.11, and a UI/UX backlog) with no relative priority, no release
horizon and no finish lines. Engineering-only work sat among user-facing steps,
and a long status recap repeated what the ADRs already record.

## Decision

- **Organized by milestone.** Sections: *0.1.0 stable*, *After 0.1.0*,
  *Ongoing tracks*, *Engineering*, *Deferred*, *Not planned*. Inside a milestone,
  items are ordered by priority and carry a theme tag. Known issues and UI
  backlog items merge into these lists, keeping a one-line provenance note.
- **Every item has a "Done when" line.** It is short and checkable.
- **0.1.0 stable must-fix:** known issues #3 (a partial-name Replace splits an
  existing identity) and #4 (surname-only mentions are not linked); responsiveness
  for ordinary legal documents; native/platform acceptance on Apple Silicon macOS
  and Windows x64 (Linux stays buildable); captured AI-reply fixtures for case
  restoration.
- **0.1.0 stable should-fix:** known issues #2 (highlight across a line wrap) and
  #5 (floating controls cover text); UI backlog 3 (popup flicker), 4 (compact,
  reversible popup), 5 (popup border), 6 (Cmd/Ctrl + + shortcut), 8 (bell
  dismiss) and 9 (Settings click-outside).
- **After 0.1.0:** OCR page structure (#1), though its scan joins the OCR
  qualification set now; UI backlog 1+2 merged (center and smoothly scroll to the
  selected mention), 7 (expand/collapse all) and 10 (Back/Forward); the
  system-wide clipboard hotkey; the 2 MiB stress tier and numerical budgets.
- **Ongoing tracks:** outgoing AI-analysis qualification (former step 2) is a
  continuous quality gate, not a release blocker, consistent with ADR 0034.
- **Engineering section:** the architecture refactoring (ADR 0049) is gated on
  step-1 profiling; the build-cache follow-up (ADR 0050) is a time-boxed
  re-measure; the cleanup charter (ADR 0029) remains standing guidance.
- **Status recap trimmed** to a few lines with links.
- **Should-fix never holds the release.** Stable waits only on must-fix items;
  unfinished should-fix items move to the top of After 0.1.0.

## Finish lines for must-fix items

- **Responsiveness.** On a release build on the M4 reference Mac, the ordinary
  scroll, typing and navigation workloads and the public DOCX/PDF preview
  workloads reach p95 frame time ≤ 16.7 ms, confirmed by a native spot-check on
  a real contract of about 30 pages. An *ordinary legal document* is about 30
  pages, or about 135 KB of Markdown. Debug-build `just perf` ceilings stay
  regression guards rather than the user-facing target.
- **AI-reply fixtures.** Four captured replies from the settlement-agreement demo
  (ChatGPT and Claude, each EN and RU) under `tests/fixtures/`. At least one
  contains Markdown tables and paraphrased or inflected aliases. A fixture passes
  when Restore leaves no alias unrestored.
- **Known issue #3.** A manual Replace inside an existing mention leaves that
  mention intact. Standalone occurrences of the selection receive a *proposed*
  link to the containing identity, which the user confirms through Same as….
  Nothing links automatically.
- **Known issue #4.** For a known identity, mdoc proposes title + surname,
  possessives, surname alone and RU inflections as variants. It does not propose
  a first name alone, which is too ambiguous in legal text. These are proposals
  for review, never automatic links. The change touches ADR 0018 and is recorded
  in its own ADR when implemented.
- **Native/platform acceptance.** One checklist per platform at
  `docs/evidence/platform/<date>/README.md` covers both themes, IME (RU plus a CJK
  smoke test), clipboard, VoiceOver/Narrator basics, and the popup, setup and
  file-drop flows, and lists unavailable checks explicitly. Windows x64 checks are
  run by the maintainer on a Windows machine.

## Ordering

- **Must-fix:** #3, #4 (same code area, back to back), responsiveness, AI-reply
  fixtures, then native/platform acceptance, which checks the finished UI.
- **Should-fix:** one mention-popup item (UI backlog 3, 4, 5 and known issue #5,
  sharing one component and one verification pass); #2; small shell fixes
  (UI backlog 6, 8, 9).
- **After 0.1.0:** carried-over should-fix items, stress tier and budgets,
  center/smooth-scroll, expand/collapse all, Back/Forward, OCR page structure,
  system-wide clipboard hotkey.
- The "extra recognizers / full PII engines" entry is Deferred, not Not planned.
