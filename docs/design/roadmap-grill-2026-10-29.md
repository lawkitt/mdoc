# Grill: seven Later items into 0.3.0 (2026-10-29)

Interview record for promoting seven Later items. ADRs written from it hold
the rules; this file keeps the reasoning and what Anoni 0.86.1 does.

## Anoni 0.86.1, seen 2026-10-29 (macOS, synthetic text)

- **Selection bar:** "Anonymize | + Dictionary" over a selection in the Check
  step. Anonymize adds the text as category Custom; no category picker.
- **AI reply:** a box under "AI reply" in the Export panel. Typing restores
  live, switches the view switcher (Before · After · Side by side · Reply) to
  Reply, and shows "✓ 6 values restored" with Copy the reply and Save the
  reply…. The reply is kept with the document. An unknown `PERSON-9` stayed
  plain, with no highlight or count.
- **What you did:** popover from a window-corner icon; entries such as
  "1 occurrence excluded · 1:59 PM" each with Undo; "These actions belong to
  the open document. The list starts over with the next one."
- **Categories:** Settings › Detection › Categories to anonymize (15/15),
  chips, "Uncheck what should never be anonymized"; per profile.
- Anoni linked "Maya" and "Ms Bennett" to one alias by itself; mdoc never
  links without a user action (ADR 0068).

## Round 1 decisions

1. **Scope.** All seven items join 0.3.0 before rc.1.
2. **Merge hint.** The 2026-10-17 spec: signals are an ADR 0068 short or
   inflected form with exactly one candidate full name; wordings differing only
   in case, whitespace or punctuation; OCR/typo near-duplicates (edit distance
   1, names of 6+ characters); and, added now, reversed Russian name order
   ("Павлова Марина" / "Марина Павлова"). Initials and relatives sharing a
   surname are not signals; no Latin/Cyrillic transliteration. "Looks like
   Maya Bennett · Merge"; no score; Merge is one undo step with an Undo toast.
   A dismissal is stored in the case when there is one, else for the tab's life.
3. **Back/Forward.** Stops: tab activations, Home, case page. Not Settings,
   scroll or mention selection; closed tabs are skipped. macOS ⌘[ / ⌘];
   Windows Alt+←/→ outside text (Alt+← in the editor stays Previous word);
   mouse buttons 4/5; ‹ › buttons beside the sidebar toggle.
4. **Selection bar.** The Replace pill becomes a split pill "Replace · Person ▾":
   the main part adds with the guessed category, ▾ picks it first. ⌘⌥P stays
   the guess path.
5. **Before the first Pseudonymize:** not offered (ADR 0024 unchanged).
6. **Restore in place.** A reply box in Share restores live into a read-only
   Reply view of the same document; "Open as tab" gives today's restore tab
   (proposals keeping aliases). Paste AI Response and ⌥⇧⌘V stay.
7. **Reply kept.** One reply per document, encrypted in the case when there is
   one, else for the tab's life; clearing the box clears it from the case.
8. **What you did.** Pseudonymization decisions only (apply, keep, alias
   rename, category change, merge, split, manual add, restore, case join) with
   time, not text edits; a click undoes back to that entry through the linear
   history; a popover from a document-header button; memory only, never on
   disk (entries hold originals).
9. **Default categories.** Settings › Detection › Categories to detect filters
   scans of documents without a case and seeds a new case's list; existing
   cases keep theirs, and a case's list wins once a document joins.
10. **Models.** Settings stores the last check (per model file size and
    modification time), shows it at once and re-checks quietly, changing only
    on a difference; "Checking…" only with nothing stored. The OCR card's pick
    never becomes the default (ADR 0066); the card says "For this document
    only · default in Settings › Recognition", and Settings says so too.

## Round 2 decisions

11. **Order in 0.3.0:** restore in place, merge hint, model state and card
    note, default categories, selection bar, What you did, Back/Forward.
12. **Merge keeps the fuller identity's alias** and primary wording; the other
    alias retires and keeps restoring. The hint sits under the shorter
    identity's group header; every hint shows.
13. **Reply view:** a fifth Reply segment while a reply exists; unresolved
    tokens highlighted; "N values restored · M unrecognised" with Next
    unrecognised; Copy reply and Open as tab; no save-to-file.
14. **Undone entries** in What you did show dimmed, click to redo, and drop on
    a new decision. Clock button beside ⋯; a palette entry; no shortcut.
15. **Unticking a default category** removes that category's unapplied
    proposals in open caseless documents; Reset ticks all; Other not listed.
16. **ADRs:** new 0078 (restore in place), 0079 (decision history), 0080
    (merge hint), 0081 (Back/Forward); the selection bar amends 0024, default
    categories and model state amend 0076 (with 0041, 0066, 0006).

## Defaults written into the ADRs without a question

- Back/Forward history is per window, about 50 stops, not restored at launch.
- The case/whitespace/punctuation merge signal applies to every category.
- The Reply view follows mapping changes live, like Before / after.
- Remembered model state holds file sizes and times only, never content.
