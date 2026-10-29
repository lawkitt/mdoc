# Decision history ("What you did")

Status: Accepted, 2026-10-29.

## Problem

Undo is the only way back from a pseudonymization decision, and it is blind:
⌘Z steps through text edits and decisions alike without saying what each step
was. After a review of 50 mentions a user cannot see what they kept, renamed
or merged. Anoni 0.86.1 lists "What you did" per document with times.

## Decision

- **What is listed.** Pseudonymization decisions only: apply, keep original,
  re-propose (a kept mention proposed again), undo replacement, alias rename,
  category change, merge, split, manual addition, restore, case join. A
  category change to part of an entity moves it to a new identity and is
  listed as a split ("Separated “Maya Bennett” as Organization"). Ordinary
  text edits are not listed. Each entry is one line with its time ("Kept “Ms
  Bennett” · 14:02"); a bulk action is one entry ("Applied 24 replacements").
- **One undo point, one entry.** Every listed decision is an undo point, a
  case join that only proposes names included. When one action makes several
  decisions on one step, its entry joins their labels in order ("Renamed
  “Maya Bennett” to CLIENT · applied 4 replacements"). A choice that changes
  nothing, such as re-choosing the current category, is no undo point and is
  not listed.
- **Built on the one history.** Entries mark points in the document's linear
  undo history (`mdoc-history`). Clicking an entry undoes back to just after
  it; later entries then show dimmed as "undone" and clicking one redoes up to
  it. A new decision drops the undone entries, as redo does. There is no
  out-of-order undo of a single entry.
- **Past the undo limit.** The list keeps decisions whose undo point the
  editor's undo limit (256 steps) has dropped. They show disabled as "Older
  than the undo limit"; travel reaches only entries still in the history.
- **Where.** A popover from a clock button in the document header, beside ⋯,
  and a palette entry "What you did". No shortcut of its own.
- **Memory only.** The list lives as long as the tab and is never written to
  disk or the case, because entries name originals. Development renders record
  only how many entries are done, undone and past the limit.

## Invariants

- Every listed entry within the undo limit corresponds to one undo point;
  travelling to it equals pressing ⌘Z / ⇧⌘Z that many times.

## Consequences

Text edits between decisions are undone along with them when travelling back;
the popover says so in its footer. Typing consumes undo steps too, so after a
long session the oldest decisions remain readable but can no longer be
travelled to.

References: [interview](../design/roadmap-grill-2026-10-29.md).
