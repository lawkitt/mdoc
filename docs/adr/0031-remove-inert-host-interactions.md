# Remove inert host interactions

Status: accepted and implemented, 2026-10-09. See the
[historical codebase cleanup interview](https://github.com/lawkitt/mdoc/blob/faa2c5e22a7eb85d4dc598c8f1f21eb125140044/docs/design/codebase-cleanup.md), checkpoint.

## Problem

Zorite's viewer and editor delegated some interactions to a host that mdoc
never implemented, so they look interactive but do nothing:

- The PDF toolbar's **Highlight** (✎, ⌘⇧H) and **Area highlight** (⬚) tools
  allow a drag selection but create nothing; mdoc has no annotation store.
- Arrowing or clicking into `$$…$$` / `$…$` math asks the host to open a math
  editor (`EditMath`), and right-click asks for a math menu (`MathMenu`); mdoc
  ignores both, so the caret cannot enter math and the clicks do nothing.
- Clicking an inline image asks the host for a preview (`PreviewImage`) under a
  hand cursor; mdoc ignores it, so the click neither previews nor places the
  caret.

## Decision

Remove the highlight tools with highlight rendering, palette and selection
drag. Treat math as ordinary Markdown text: the caret enters it like other
revealed syntax, Backspace/Delete edit it character by character instead of
removing whole formulas, and right-click shows the normal menu. Inline image clicks
behave like clicks on text, without the hand cursor. Image previews would be a
new feature and need their own design.

## Invariants

Saved bytes and Copy Markdown are unchanged. `$$` block detection used by
search, drag ranges, "Turn into" and alignment-marker hiding is unaffected.
PDF search, zoom, navigation, links and form-appearance rendering are unchanged.
