# Not a name

Status: Accepted, 2026-11-19.

## Problem

A false hit ("выдачи" in "Дата выдачи") or a name the user wants left
alone can only be kept. Keep leaves traces: a dotted underline, a row in
Identities, a place in the bar's stepping and counts, and, once the name
was replaced and saved, its wording in the case for good. Only a fresh
Hide name mark can go, and only through Undo.

## Decision

1. **Not a name** takes the mark off a proposed, replaced, kept or custom
   name in the scope shown (All N or This one), as one undo step.
   Replaced text returns to its original. The name leaves the text's
   decoration, the bar and Identities.
2. The document remembers the wording, shown nowhere, so Find names again
   doesn't propose it.
3. On the next save a wording the case holds leaves the case, its alias
   reserved, unless another member's saved text still has that alias, so
   that member still restores. Undo brings the name back, and the next save
   returns it under its old alias; only the same document may refill a
   reserved alias.
4. On desktop it adds nothing to the Never list and takes a manual mark's
   wording off Always.
5. iOS offers it last in the card's category menu, the long press and an
   Identities row's menu, unstyled since Undo takes it back. Select's bar
   becomes Replace, Keep and a round ⋯ holding Same as… and Not a name.
   Afterwards the bar moves to the next undecided name with no toast;
   elsewhere a toast reads "Not a name: <wording> · Undo".
6. One core call in `mdoc-ffi` (a range and a scope) serves every platform.
   Desktop and Android follow iOS.

## Consequences

- Keep now means only "send the original"; Not a name means "never found".
- A case can lose a wording it once restored, but never while a saved
  member still needs it.

Terms: **Not a name**, not "delete" or "remove", which read as deleting the
words.

References: ROADMAP iOS item 29; rules in
[docs/specs/mobile.md](../specs/mobile.md),
[docs/specs/cases-and-privacy.md](../specs/cases-and-privacy.md) and
[docs/specs/recognition.md](../specs/recognition.md).
