# Decision history ("What you did")

Status: Accepted, 2026-10-29.

## Problem

Undo is the only way back from a pseudonymization decision, and it is blind:
⌘Z steps through text edits and decisions alike without saying what each step
was. After a review of 50 mentions a user cannot see what they kept, renamed
or merged. Anoni 0.86.1 lists "What you did" per document with times.

## Decision

- **What is listed.** Pseudonymization decisions only: apply, keep original,
  undo replacement, alias rename, category change, merge, split, manual
  addition, restore, case join. Ordinary text edits are not listed. Each entry
  is one line with its time ("Kept “Ms Bennett” · 14:02"); a bulk action is one
  entry ("Applied 24 replacements").
- **Built on the one history.** Entries mark points in the document's linear
  undo history (`mdoc-history`). Clicking an entry undoes back to just after
  it; later entries then show dimmed as "undone" and clicking one redoes up to
  it. A new decision drops the undone entries, as redo does. There is no
  out-of-order undo of a single entry.
- **Where.** A popover from a clock button in the document header, beside ⋯,
  and a palette entry "What you did". No shortcut of its own.
- **Memory only.** The list lives as long as the tab and is never written to
  disk or the case, because entries name originals.

## Invariants

- Every listed entry corresponds to an undo point; travelling to it equals
  pressing ⌘Z / ⇧⌘Z that many times.

## Consequences

Text edits between decisions are undone along with them when travelling back;
the popover says so in its footer.

References: [interview](../design/roadmap-grill-2026-10-29.md).
