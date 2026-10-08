# Simplify pseudonymization around review, application and one copy action

Status: accepted and implemented, 2026-10-08, following the user's "implement".
See the
[design tree](../design/pseudonymization-ui-simplification.md).

## Problem

The supplied screenshot distributes pseudonymization across review controls,
identity management and competing copy actions. Routine review exposes detailed
correction tools and repeated status/caution text, consuming document space and
obscuring the main task.

## Confirmed direction

The user agreed to Q1/Q2, chose one copy action for Q3, and agreed to Q4–Q10:

- Review compact original-to-alias proposals before explicitly applying them.
  Provide one prominent batch application action.
- Retain rename, category correction, identity linking/separation and owner
  assignment. Expose them contextually for a selected item.
- Replace the parallel review bar/Identities surface with one compact, simple,
  elegant Replacements panel. Open it after scanning and keep the document
  visible. Closing the panel retains clickable applied highlights.
- Show one original-to-alias row per identity with a mention count. Selection
  exposes variants/occurrences and connects to document highlights.
- Use one Copy Markdown action for now, copying current text exactly. Remove
  the second copy action and automatic relationship appendix option; owner
  assignments remain local. Pending proposals do not block ordinary copying;
  copying never implicitly applies them.
- Apply all remaining proposals as one undo step without mandatory per-row
  approvals. Keep original on an identity row excludes its pending mentions;
  single-mention Keep remains available during occurrence inspection.
- Open selected-item inspection within the same panel, with Back preserving
  list position. Center the transition, editable alias and mention navigation;
  expose detailed correction tools through item-specific actions. Applied
  document popups retain scoped Restore and a route to replacement editing.
- Dock beside the document when readable; use a dismissible overlay drawer
  at narrow widths, preserving document selection/scroll and reachable Apply.
- Use one concise experimental review caution, pending status and no safe-to-share
  claim. Preserve predictable raw clipboard behavior.

The rationale is a clearer routine workflow while retaining correction power.
This direction revises ADR 0018's review surfaces and two-copy-action design.
The existing raw-source clipboard behavior is retained.

## Invariants and final confirmation

The linked interview contains the complete visual, state and acceptance contract.
The design frontier is empty; the user's "implement" confirms the final scope.

Existing experimental status, stable aliases, occurrence provenance, local-only
originals, Keep, undo/redo, syntax protection and stale-result guards remain in
force. No detector changes or quality qualification are selected by this decision.

## Implementation and verification

The Replacements list and selected-item view now share one workspace, with
contextual correction commands, explicit Apply and responsive docking/drawer
presentation. Copy Markdown retains its exact current-source behavior.
Identity-level Keep uses occurrence assignments and batched exclusions; applied
originals share existing provenance strings rather than copying them per render.

The workspace suite passes 435 tests (16 existing ignored checks). Build, strict
Clippy, formatting and diff checks pass. Native macOS interaction covers the
editable alias, application, scoped restoration, both themes and a 640×480
drawer. See the [verification record](../evidence/pseudonymization/2026-10-08-ui/README.md)
for evidence and remaining platform/interaction boundaries.
