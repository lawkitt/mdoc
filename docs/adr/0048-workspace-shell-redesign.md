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
