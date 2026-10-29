# Workspace shell redesign

Status: Accepted, 2026-10-15. Supersedes the toolbar of
[ADR 0012](0012-document-preparation-ui.md), the widths and header of
[ADR 0027](0027-chrome-style-vertical-sidebar.md), the empty page of
[ADR 0028](0028-empty-page-and-file-drop.md), the popup layout of
[ADR 0032](0032-replacement-popup-redesign.md), the panel chrome and scope keys
of [ADR 0033](0033-replacements-panel-redesign.md), and the "no per-category
colours" of [ADR 0046](0046-workspace-polish-from-anoni-review.md).
Amended by [ADR 0055](0055-stable-compact-mention-popup.md): popup width,
placement, scope, arrow keys, Enter and Apply all.
Amended by [ADR 0076](0076-settings-categories.md) (Settings categories)
[ADR 0077](0077-case-page.md) (case page from the case header, case
colour), [ADR 0078](0078-restore-in-place.md) (Reply view),
[ADR 0079](0079-decision-history.md) (header clock button) and
[ADR 0081](0081-workspace-back-forward.md) (Back/Forward buttons).

## Problem

mdoc looked like a developer tool: a native title bar and an icon toolbar spent
about 70 px, nothing said what to do next, the panel was dense, and launch
opened an empty Untitled tab. Anoni shows a calmer shell with one obvious next
step, but as a one-document wizard that hides editing.

## Decision

1. **Shell.** A transparent, unified title-bar row holds the traffic lights
   (drawn caption buttons on Windows; client-side decorations on Linux with a
   system title-bar fallback), the sidebar toggle and the document header. The
   content is an inset rounded card on the slightly darker sidebar ground. With
   the sidebar collapsed the header joins the title row beside the traffic
   lights and the content gets its own card, exactly as tall as before.
2. **Sidebar (240 px).** Search ⌘K, Home; open tabs grouped by case, each a
   collapsible section (name, count; header hover shows + for a new tab in the
   case and ⋯ for Rename, Categories to detect, Close all, Delete case); tabs
   without a case under "Open". Tabs drag between sections. Foot: an update card
   while an update waits, then ⚙ Settings · ◐ theme (cycles System / Light / Dark,
   saved in the session; System follows the window live; Settings shows the
   same choice as a segmented control) · ? Help ↗.
   Settings opens as an overlay; a click outside the dialog closes it.
   **Collapsed rail (44 px)**: quiet 16 px icons (toggle, Search, Home), a
   hairline, tabs as document glyphs (active on a soft fill, status as a dot,
   name and format in the tooltip), a plain +, and only Settings pinned to the
   foot (an update as a dot on it). ADR 0027's hover reveal stays. **Toggling**
   resizes the content once; only a clipped sidebar layer slides (150 ms), so
   nothing reflows during the animation.
3. **Document header.** Breadcrumb (case › file) left; right: panel toggle,
   Source (the PDF/DOCX page view, renamed from "Original"), ⋯, and the
   **primary action**: Pseudonymize → stage + Cancel → Apply N ⌘↩; once applied,
   a quiet Share; Copy on a restored reply tab. ⋯ holds Save, Save as, Copy
   Markdown, Copy for AI, Paste AI reply…, Spelling, Rescan, case actions and
   Close tab, all also in the palette and the menu bar. No stepper.
4. **Command palette ⌘K**: actions, Go to (tabs, cases, Settings), Go to
   heading (the active document's ATX headings outside code fences: the caret
   moves to the heading's line and scrolls into view, Undo is untouched),
   Recent files. With the Replacements panel open, Expand / Collapse all
   identities. ⌘F stays find-in-document.
5. **Home**: shown with no tab open and from the sidebar: greeting, drop zone (a
   drag anywhere over the window activates it: solid accent border, lifted
   illustration, "Drop N files"), Open files… ⌘O, Paste text, Restore AI reply,
   Pseudonymize clipboard, the local note (ADR 0046), Cases and Recent files.
   Launch shows Home or the restored session, never an empty Untitled tab. A
   hidden blank placeholder tab sits under Home; ⌘N turns it into Untitled and
   opening files replaces it. While Home shows, focus is on the workspace so
   shortcuts work. Blank tabs are plain editors ("Type here…"). Recents are
   paths only, local, clearable in Settings. A one-time "What's new" banner
   follows an update.
6. **Inspector** beside the document: two tabs, **Review · N | Share**. Review
   is the Replacements panel: a filter field with All / To replace / Applied /
   Kept; category headers with a coloured dot and "entities (mentions)"; one row
   per entity with a checkbox (ticked = will be replaced, unticked = Keep
   original, – = mixed; fill `Theme::checkbox`), original and muted alias; hover
   ↓ next mention; a chevron expands mention snippets. Kept mentions stay listed
   under their entity as unticked rows, in document order; the entity a kept
   wording belonged to is recorded at untick time. Ticking records intent; text
   changes only on Apply (one batch, one undo step). Unticking an applied entity
   undoes it and records Keep; ticking a kept mention proposes it again, alone.
   Right-click on an entity opens the category menu. Foot: "N to replace · M
   kept", the responsibility line, ↻ Rescan. Hidden before a scan, skeleton rows
   while scanning, "Nothing found" when empty.
7. **Text highlights.** Proposed mentions get a light category tint with a 2 px
   category underline; applied ones keep the blue chip (ADR 0032). Hover shows a
   tooltip (alias, category, "Click to edit"); a click places the caret, selects
   the panel row and opens the **popup**, holding the mention bar (☑ Replace ·
   original → alias · category ▾ · scope · ‹ n of N › · ×). Size, placement,
   scopes and stability: ADR 0055.
   Panel entity clicks open the group and the popup on its first mention (a
   second click folds both); mention rows open the popup on that mention.
8. **Keys.** Panel: ↑/↓, Space toggle, →/← expand, Enter jump, ⌫ Keep, ⌥↑/↓
   keyboard move. Popup: Enter applies in the selected scope and moves to the
   next proposal (ADR 0055), Space unticks Replace in scope, ←/→ and ↑/↓ step mentions (ADR 0055), ⌫ / ⌘⌫ / ⇧⌘⌫ keep this / same
   wording / all. ⌘↩ applies everything, everywhere, and closes the popup;
   ⌘Z takes an applied mention back to proposed. ADR 0033's ⌘Enter / ⇧⌘Enter scopes are gone.
9. **Reading column** centred, 760 px at 100%. **Zoom** scales the Markdown
   column's text, 50–200% in 5% steps (⌘= or ⌘+ / ⌘− / ⌘0; Ctrl on Windows
   and Linux; plus is accepted with the Shift a layout needs), per document,
   not saved. The keys step exactly as the pill's buttons do.
10. **View pill**: zoom − 100% + and, once replacements are applied, Edit ·
    Original · Pseudonymized · Side by side. It rests hidden: it fades 1.5 s
    after the last pointer move near the bottom of the text, scroll, zoom or
    view change, and stays while hovered; under Reduce Motion it appears and
    goes without a fade.
11. **Notices** that need attention (case key, crash report, session errors)
    float as toasts over the card's bottom-left, above the view pill, and are
    counted by the bell. The bell opens and closes the notices popover, which
    also closes from its × or an outside click; closing it keeps every notice
    and the bell's count. Expected limitations are not notices: the DOCX
    preview's "approximate" caveat (ADR 0005) is an ⓘ beside the Source name;
    worker warnings stay in the preview pane. The preview header reads
    "Source · name"; its close button is "Close source".

## Invariants

- Ticking never changes document text; only Apply and undo do.
- A click in the text always places the caret.
- Category colour never encodes state; fill and chip do.
- Recents never store document content.
- Nothing rests over the text: floating controls hide or move clear of it.

## Consequences

The toolbar and the native title bar are gone; their actions live in the
header, ⋯, the palette and the menu bar. Drawn caption buttons and client-side
decorations compile on Windows and Linux but were not run there.

References: [full text before ADR 0052](https://github.com/lawkitt/mdoc/blob/f368e436cd2d3e70789d51879b9f54f379900e1e/docs/adr/0048-workspace-shell-redesign.md),
[interview](https://github.com/lawkitt/mdoc/blob/f368e436cd2d3e70789d51879b9f54f379900e1e/docs/design/anoni-ui-redesign.md),
[README media review](https://github.com/lawkitt/mdoc/blob/f368e436cd2d3e70789d51879b9f54f379900e1e/docs/design/readme-media-v2.md) (decisions 9 and 15).
