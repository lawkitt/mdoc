# Live before / after view

Status: Accepted, 2026-10-13; implemented the same day. Supersedes the
"snapshot taken when opened" sentence of
[ADR 0041](0041-cases-and-ai-response-restoration.md) for the before / after view.

## Problem

The before / after view copied the text into two read-only editors once, when
opened. The Replacements panel stays usable beside it, so applying, undoing or
reverting a replacement left the view stale (reported against 0.1.0-beta.5).
A full-width header row ("Original", "Pseudonymized", Close) cost vertical
space and did not match the editor's floating chrome. The toolbar's text
button flipped its label between "Before / after" and "Edit", naming the next
mode rather than the current one, and was the only text control among icons.

## Decision

1. **Live.** The view rebuilds whenever the applied replacements change. The
   rebuild hooks the single point every change goes through
   (`sync_annotations`: apply, undo/redo, revert, rescan). Scroll position is
   kept. When no replacements remain, the view closes back to the editor.
2. **No header row.** Each column carries a small floating label in its
   top-left corner ("Original", "Pseudonymized"), quiet like Copy, faded while
   scrolling and restored on hover.
3. **Floating Close** in the right column's top-right corner, styled like
   `ui::floating_button` (Copy). Esc also closes.
4. **Toolbar toggle is an icon** (two columns) beside Paste AI response, with
   an active (pressed) state while the view is open, a tooltip and a keyboard
   shortcut. No label swap.
5. **Opening the view hides the PDF preview** ("Show original") and closing
   it restores the previous state. The Replacements panel stays.
6. **Gold originals, blue aliases.** The original column marks the original
   wording in gold and the pseudonymized column marks aliases in blue, the
   two colours of the editor's review (ADR 0032), so each side reads as what
   it is. A new replacement settles in as in the editor: its alias eases from
   gold into blue and rolls from the original words; its original glows gold.
7. **Panel → view navigation.** Selecting a Replacements row scrolls both
   columns to that identity's first occurrence and briefly highlights it.
8. **⌘\\ (Ctrl+\\) toggles** the view; it is also in the File menu.
9. **Esc closes innermost first**: a popup, then search, then the panel's own
   Esc; the view closes only when nothing else is open, including from inside
   its columns. Focus returns to the editor.
10. **View → panel navigation.** Clicking a name in either column selects its
    Replacements row.
11. **Narrow windows** (under 720 px for the text) show one column at a time;
    the floating label switches between them.
12. **⌘F closes the view** and searches the editor. Searching both columns can
    come later.
13. The word popup is not shown while the view is open: it anchors to the
    hidden editor, and the panel offers the same decisions.

## Invariants

- The view never shows a state that differs from the editor's text and the
  applied replacements.
- The view stays read-only; every change still goes through the editor or the
  Replacements panel.
