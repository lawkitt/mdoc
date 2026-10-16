# Workspace shell redesign

Status: Accepted, 2026-10-15. Interview record:
[workspace redesign after Anoni](../design/anoni-ui-redesign.md). Supersedes the
toolbar of [ADR 0012](0012-document-preparation-ui.md), the 200 px width of
[ADR 0027](0027-chrome-style-vertical-sidebar.md), the popup of
[ADR 0032](0032-replacement-popup-redesign.md), the panel chrome and scope keys
of [ADR 0033](0033-replacements-panel-redesign.md), the empty page of
[ADR 0028](0028-empty-page-and-file-drop.md), and the "no per-category colours"
of [ADR 0046](0046-workspace-polish-from-anoni-review.md).

## Problem

mdoc looks like a developer tool: a native title bar and an icon toolbar
spend about 70 px; nothing says what to do next; the replacements panel is
dense; launch opens an empty Untitled tab. Anoni shows a calmer shell with one
obvious next step, but as a one-document wizard that hides editing.

## Decision

1. **Shell.** A transparent, unified title-bar row holds the traffic lights
   (or drawn caption buttons on Windows; client-side decorations on Linux with
   a system title-bar fallback), the sidebar toggle and the document header.
   The content area is an inset rounded card on the slightly darker sidebar
   ground. mdoc's neutral palette stays.
2. **Sidebar (240 px).** Search ⌘K, Home; then open tabs grouped by case, each
   case a collapsible section (name, count; header hover shows + for a new tab
   in the case, ⋯ for Rename, Categories to detect, Close all, Delete case);
   tabs without a case under "Open". Tabs drag between sections. Foot: an
   update card while an update waits for relaunch, then ⚙ Settings · ◐ theme
   (System / Light / Dark) · ? Help ↗. Collapsed: ADR 0027's rail with hover
   reveal, starting with Search and Home.
3. **Document header.** Breadcrumb (case › file) left; right Source (the
   PDF/DOCX page view, renamed from "Original"), panel toggle, ⋯, and a
   state-driven **primary action**: Pseudonymize → stage + Cancel → Apply N ⌘↩
   → Copy for AI; Copy on a restored reply tab. ⋯ holds Save, Save as, Copy
   Markdown, Copy for AI, Paste AI reply…, Spelling, Rescan, case actions and
   Close tab, all also in the palette and the menu bar. No stepper.
4. **Command palette ⌘K.** Actions, Go to (tabs, cases, Settings), Recent
   files. ⌘F stays find-in-document.
5. **Home.** The page with no tab open and a sidebar item: greeting, drop zone
   (line illustration; a drag anywhere over the window activates it with a
   solid accent border, lifted illustration and "Drop N files"), Open files…
   ⌘O, Paste text, Restore AI reply, Pseudonymize clipboard, the local note,
   Cases and Recent files. Launch shows Home or the restored session, never an
   empty Untitled tab. Recents are paths only, local, clearable in Settings. A
   one-time "What's new" banner follows an update.
6. **Replacements panel.** A rounded card; "Replacements" + total; a filter
   field with All / To replace / Applied / Kept. Category headers carry a
   coloured dot and "entities (mentions)". One row per entity: checkbox
   (ticked = will be replaced, unticked = Keep original, – = mixed), original,
   muted alias; hover ↓ next mention; a chevron expands mention snippets.
   Ticking records intent; text changes only on Apply (one batch, one undo
   step). Unticking an applied entity undoes it and records Keep. Foot: "N to
   replace · M kept", the responsibility line, ↻ Rescan. Hidden before a scan;
   skeleton rows while scanning; "Nothing found" empty state.
7. **Text highlights.** A category-tinted underline; proposed = light tint,
   applied = alias chip. Clicking places the caret, selects the panel row and
   shows the **mention bar** (☑ Replace · alias · category ▾ · scope); Esc or
   typing dismisses it.
8. **Keys.** Panel: ↑/↓, Space toggle, →/← expand, Enter jump, ⌫ Keep, ⌥↑/↓
   keyboard move. ⌘↩ applies everywhere. ADR 0033's ⌘Enter / ⇧⌘Enter scopes go.
9. **Floating pill** always shown: zoom − 100% + (⌘= / ⌘−, per document, not
   saved); Edit · Original · Pseudonymized · Side by side join it once
   replacements are applied.

## Invariants

- Ticking never changes document text; only Apply and undo do.
- A click in the text always places the caret.
- Category colour never encodes state; fill and chip do.
- Recents never store document content.

## Consequences

The toolbar, the native title bar and the replacement popup are removed; their
actions move to the header, ⋯, the palette and the menu bar. Tests that drive
toolbar buttons and popup scopes are rewritten. Two variations are built on
branches for comparison (design doc, decision 18); the chosen one amends this
ADR.

## As implemented

- **Home over a placeholder.** The workspace keeps its one-tab invariant: with
  nothing to restore, a hidden blank placeholder tab is active and Home covers
  it. New tab (⌘N) turns that placeholder into the Untitled tab; opening files
  replaces it. While Home shows, keyboard focus sits on the workspace, not the
  hidden editor, or shortcuts such as ⌘O would reach nothing.
- **Blank tabs** are plain editors ("Type here…"). The old start page shows
  only as drop feedback while files are dragged over an empty tab.
- **Kept rows** come from the review's Keep decisions (`Review::kept_mentions`),
  not from panel state, so undo and redo bring ticks back. Ticking a kept
  entity proposes its wordings again through the manual-addition path, keeping
  the entity's alias.
- **Category menu** opens with a right-click on an entity row; a hover control
  would cover the alias. The mention bar also carries the category.
- **Mention bar** keys: Enter applies this mention, Space unticks ☑ Replace in
  the selected scope, ⌫ / ⌘⌫ / ⇧⌘⌫ keep this / same wording / all, ⌘↩ applies
  everything as everywhere else. The applied-mention "Undo (back to proposed)"
  button is gone; ⌘Z does it.
- **Proposed highlights** are a light category tint with a 2 px category
  underline (`SourceAnnotation::underline`); applied ones keep the blue chip.
- **Zoom** scales the Markdown column's text size, 50–200% in 10% steps,
  ⌘= / ⌘− / ⌘0.
- **Theme choice** is saved in the session; System follows the window's
  appearance live.
- **Not verified on Windows or Linux:** drawn caption buttons and client-side
  decorations compile but were not run there.

## Amendment, 2026-10-15: owner review of the three branches

The agreed design keeps two pieces of the variants: variant C's two-tab
Inspector (**Review · N | Share**, header primary Pseudonymize / Apply, a quiet
Share once applied) and variant B's centred reading column (760 px at 100%).
Variant B's floating panel and folding sidebar are not adopted.

- **Hover and click on a replacement.** Hovering shows a tooltip (alias,
  category, "Click to edit"); clicking opens the popup. The popup has one
  width (up to 720 px), sits just below its word (above only without room),
  and keeps ‹ n of N › and × together at the row's end.
- **Panel clicks open the popup.** An entity row click opens the group and the
  popup on its first mention; a second click folds the group and closes the
  popup. A mention row opens the popup on that mention.
- **Unticked rows stay.** The entity a kept wording belonged to is recorded at
  untick time, so the row stays, unticked, even when the review no longer
  links the wording to its entity.
- **Checkbox fill** is a soft warm grey (`Theme::checkbox`), clearly filled in
  both themes.
- **Zoom** steps by 5%.
- **Workspace notices** (case key, crash, session notices) float as toasts
  over the card's bottom-left, above the view pill, in both sidebar states:
  they never push the header out of the title row or sit off the card.
- **Header order:** panel toggle, Source, ⋯, primary.
- **Collapsed sidebar: the header joins the title row.** As native Mac apps
  move their toolbar into the title bar when the sidebar hides, the document
  (or Home) header sits on the window ground beside the traffic lights and
  the expand toggle, and the content gets its own card below it. The rail
  narrows to 44 px under the title row. The content is exactly as tall as
  with the sidebar open, since the header only moves out of the card.
- **Sliding without reflow.** A toggle resizes the content once, at the
  start; only a clipped sidebar layer animates: expanding reveals the list
  inside its final width, collapsing narrows it over the content in a layer
  drawn above the card. Nothing reflows during the 150 ms slide, so it stays
  smooth in debug builds and with long documents.
- **Collapsed rail, in Anoni's manner:** quiet 16 px icons (toggle, Search,
  Home), a hairline, the open tabs as document glyphs (the active one on a
  soft fill, status as a small dot, name and format in the tooltip), a plain
  +, and only Settings pinned to the foot (an update shown as a dot on it);
  theme and Help live in the open sidebar. No boxed tiles or format captions.
- **Kept mentions stay listed** under their entity as unticked rows, in
  document order. Ticking one proposes that mention again, alone: other kept
  mentions of the same wording stay kept, in the same undo step.

## Amendment, 2026-10-16: nothing rests over the text

From the README media review
([decisions 9 and 15](../design/readme-media-v2.md)): users see these too.

- **The view pill rests hidden.** Decision 9's "always shown" pill covered
  body text. It now fades 1.5 s after the last pointer move near the bottom of
  the text, scroll, zoom (⌘= / ⌘− included) or view change, and stays while
  the pointer is on it. Under Reduce Motion it appears and goes without a
  fade.
- **Expected limitations are not notices.** The bell counts only notices that
  need attention (case key, crash report, session errors). A DOCX preview's
  standing "approximate" caveat (ADR 0005) is an ⓘ beside the Source name.
  Worker warnings about omitted content stay in the preview pane.
- **The Source preview header reads "Source · name"**, as the glossary says;
  its close button is "Close source".
