# Workspace redesign after Anoni — design interview

Status: complete, 2026-10-15. Accepted decisions are in
[ADR 0048](../adr/0048-workspace-shell-redesign.md). Follows the
[Anoni comparison](anoni-comparison.md) (ADR 0046), which adopted small pieces.
This interview reconsiders mdoc's whole shell: sidebar, tab organisation, main
controls, replacements panel and visual language.

## Request

The owner likes Anoni 0.81.2's UI/UX and wants mdoc to take what works and do
it more elegantly. mdoc is early, so sidebar, tabs, controls and the panel may
all be redesigned.

## Findings

### Anoni 0.81.2 (owner screenshots + live app, 2026-10-15)

- **Shell.** Unified title bar: traffic lights, sidebar toggle, breadcrumb
  ("Documents"), right: "● AI active" and a bell. Warm off-white palette; the
  content is an inset rounded card on a slightly darker sidebar ground.
- **Sidebar (expanded, ~280 px).** Workspace header (logo, "Anoni / General",
  ▾ switcher, ✎ new). Search field (⌘K, opens the palette). Home (⌘N).
  Sections: *Workspace* (Documents with count, Cases), *Tools* (Decode,
  Watermark, PDF → Word), *Pro* (Journal, Watched folders). Bottom: Models,
  Settings, Guides & help ↗, and an update card "Installed, next launch ·
  v0.81.2 · relaunch ● →".
- **Sidebar (collapsed).** An icon rail with the same items; no hover reveal.
- **Command palette (⌘K).** "Search or run an action…": Actions (New document
  ⌘N, Paste text, Multiple files, Keyboard shortcuts ?, Connect an AI), Go to
  (every sidebar destination), Recent (files with a type tag: Text, DOCX).
- **Home.** "Hello / What would you like to anonymize?", large dashed drop zone
  with illustration, Choose a document, format line; under it Paste text,
  Multiple files, and "🔒 100% local ?"; a Recent list (name, date, status
  "anonymized"). Update banner across the top: "Anoni 0.81.2 is installed.
  What's new ×".
- **Document page.** Header: ← back, file name, stepper Drop · Check · Export
  (top right). A status line above the text: "● AI active · 46 items found",
  "↻ Re-analyze" on the right. Detected spans have a light category-tinted
  fill with a coloured underline. Floating pill bottom centre: zoom − 125% +;
  on Export it becomes Before · After · Side by side | − 125% +.
- **Check panel ("To mask", a rounded card, right).** Filter field; groups
  "● PERSON 8 (9)" (coloured dot, caps, entities (mentions)); one row per
  entity: checkbox + original, no alias shown. Row hover: ▾ (category) and ↓
  (next mention). Clicking a row scrolls to and outlines the mention. Footer:
  "Re-identification ● Not assessed / Low risk" and a black full-width
  **Continue → ⌘↩**.
- **Click on a highlight** toggles that one occurrence off (struck through) and
  shows a toast "1 occurrence excluded · Undo" bottom right. No popup.
- **Export panel ("How to share?").** Pseudonymize (Tokens | Realistic) or
  Redact; AI reply box; Share the key…; Save… ⌘↩ (split button), Copy for AI,
  Compliance report. Text shows aliases as monospace chips (`PERSON-1`).
- **Weak spots.** Raw Markdown shown as text (`**ДОГОВОР**`, `| --- |`, literal
  `<br>`); detection noise (headings and `_|_|` as PERSON, "FBS" as both
  ORGANIZATION and PLACE, "4" as DATE); no alias visible while checking;
  one document at a time; the stepper hides that edits are possible.

### mdoc beta.9 today

- Native title bar ("Untitled.md — mdoc"); a 40 px icon toolbar: Open, Save,
  Save as, Spelling, Pseudonymize, Paste AI response, Before/after; right:
  theme, Settings.
- Sidebar = Chrome-style vertical tabs (ADR 0027), 200 px, + row, collapsed
  rail with hover reveal. Cases are tab groups in it (ADR 0041).
- Empty tab: "Type here…", Open files… ⌘O, "Documents stay on this Mac".
- Replacements panel (ADRs 0033, 0044, 0046): toolbar (Undo, Redo, Rescan, ×),
  search, expand/collapse, filter; category headers; expandable entity groups
  with alias, category ▾ and count; mention rows with snippets; inline
  controls; footer "N mentions to apply · Apply replacements".
- Editor-first: the Markdown renders as a document and stays editable; no
  library, files live on disk (ADR 0003); no recent-files store.

## Decisions

### Round 1 (2026-10-15)

1. **Sidebar = hybrid.** Fixed top: Search ⌘K, Home. Then the open documents
   grouped by case: each case a collapsible section (name, count); tabs
   without a case under "Open". Bottom: Models, Settings, Help, update card.
   No Documents library page (files stay on disk, ADR 0003); recents live on
   Home. No Tools section.
2. **Shell.** The content area is an inset rounded card on a slightly darker
   sidebar ground. A unified transparent title bar: traffic lights, sidebar
   toggle and breadcrumb share one row with the document header; no native
   title bar, no toolbar strip. **Not adopted:** Anoni's warm palette and its
   section-label style; mdoc keeps its neutral palette.
3. **Main controls.** The global icon toolbar goes. Each document has a header:
   breadcrumb (case › file) left; right a state-driven primary button and ⋯.
   Primary: Pseudonymize → scanning progress + Cancel → Apply N ⌘↩ → Copy for
   AI. Save, Save as, Spelling, Paste AI reply, Copy Markdown move to ⋯, the
   palette and the menu bar. The **theme toggle stays visible, beside
   Settings** at the foot of the sidebar. No stepper (ADR 0046 stands).
4. **Panel rows = checklist + mdoc's power.** One row per entity: checkbox
   (checked = will be replaced, unchecked = Keep original), original, muted
   alias right. Hover ↓ = next mention; a disclosure chevron expands mention
   snippets on demand; row click jumps to and outlines the mention.
5. **Category colour (reverses ADR 0046's "not adopted").** A coloured dot on
   category headers and a category-tinted underline in the text; state is
   shown by fill: proposed = light tint, applied = alias chip. Muted palette
   valid in both themes.
6. **Command palette ⌘K** ("Search or run an action…"): Actions (New tab, Open
   files, Paste text as new tab, Pseudonymize clipboard, Restore AI reply, Copy
   for AI, Keyboard shortcuts), Go to (open tabs, cases, Settings), Recent
   files. ⌘F stays find-in-document.
7. **Home.** Sidebar item, and the page shown when no tab is open. Greeting;
   drop zone with Open files… ⌘O, as elegant as Anoni's or better; Paste text,
   Restore AI reply, Pseudonymize clipboard; "Documents stay on this Mac";
   Cases (name, documents, last used); Recent files (name, path hint, date,
   "pseudonymized" badge). Launch shows Home or restores the last session's
   tabs, never an empty Untitled tab. Recents store paths only, locally, with
   Clear recents in Settings.
8. **Floating pill always shown.** Zoom − 100% + (⌘= / ⌘−, per document, not
   saved) alone; Edit · Original · Pseudonymized · Side by side join it once
   replacements are applied.
9. **Update notice.** A sidebar-foot card while an update waits for relaunch;
   a one-time "What's new" banner on Home after it.

### Round 2 (2026-10-15)

10. **Clicking a highlight** places the caret and selects the entity's panel
    row (scrolled into view); a slim floating bar appears above the mention:
    ☑ Replace · alias · category ▾ · scope (This / Same wording / All). Esc or
    typing dismisses it. It replaces the replacement popup; no
    toggle-on-click.
11. **Apply stays an explicit batch.** A checkbox records intent; the text
    stays original until Apply N ⌘↩. Unticking an applied entity undoes it to
    the original and records Keep. Mixed mentions show a tri-state (–) box.
12. **Apply lives in the header only.** The panel foot shows a summary ("12 to
    replace · 3 kept") and the responsibility line. ⌘↩ works everywhere.
13. **Panel chrome.** Rounded card; top row "Replacements" + total; filter
    field with All / To replace / Applied / Kept inside it. Undo/Redo,
    Expand/Collapse all and × leave the panel (⌘Z anywhere; a panel toggle
    beside ⋯ in the header, or Esc, closes it). Rescan moves to ⋯ and to a
    "↻ Rescan" link in the panel's empty/foot area.
14. **Cases in the sidebar.** Drag a tab onto a case section to move it.
    Section ⋯: Rename, Categories to detect, Close all, Delete case. New case
    from a tab's context menu and the palette. Hovering a section header shows
    + (new tab in that case).
15. **Collapsed sidebar** keeps ADR 0027's hover reveal; the rail starts with
    Search and Home, with a thin divider between case groups.
16. **No sidebar header**; the title-bar row holds the toggle and Search is
    first. Fixed width 240 px (was 200).
17. **Drop zone.** Dashed rounded zone with a line illustration (page with
    redaction bars and a small lock). A file dragged anywhere over the window
    activates it: solid accent border, the illustration lifts and its bars
    mask, label "Drop N files". One primary Open files… ⌘O, format line "PDF,
    DOCX, Markdown". Illustration adapts to dark mode.
18. **Three branches.** **A** — the agreed design. **B "Focus"** — the panel
    is a floating card over the right margin and the sidebar auto-collapses
    when a document opens. **C "Inspector"** — the panel is a two-tab
    inspector, Review | Share (Copy for AI, Save, Restore AI reply, case); the
    header primary is only Pseudonymize / Apply. Each variant changes only its
    own axis.

### Round 3 (2026-10-15)

19. **Panel keys.** ↑/↓ rows; Space ticks/unticks (entity, or mention on a
    mention row); →/← expand/collapse snippets; Enter jumps to the mention; ⌫
    unticks (Keep original); ⌘↩ applies everything ticked; ⌥↑/↓ keyboard move
    stays. ADR 0033's ⌘Enter / ⇧⌘Enter scope shortcuts go; scope lives in the
    mention bar, which shares Space and Esc.
20. **Header right side:** Source · Panel · ⋯ · Primary. The PDF/DOCX page view
    is renamed **Source** ("Show the original file"); the pill keeps
    "Original" for pre-pseudonymization wording.
21. **⋯ menu:** Save ⌘S, Save as… ⇧⌘S, Copy Markdown, Copy for AI, Paste AI
    reply…, Spelling ✓, Rescan, Save mapping as case… / Move to case ▸, Close
    tab ⌘W. Every item is also in the palette and the menu bar.
22. **Panel states.** Hidden before a scan; Pseudonymize opens it. Scanning:
    stage + Cancel in the header, quiet skeleton rows; rows appear when the
    scan ends. Nothing found: "Nothing found. Select text to add it by hand ·
    ↻ Rescan".
23. **Restored reply tabs** sit in their case section with a reply icon, keep
    the notice bar, have Copy as the header primary and no replacements panel.
24. **Sidebar foot:** update card (only while one waits), then one row ⚙
    Settings · ◐ theme (System / Light / Dark) · ? Help ↗. No Models item.
25. **Title bar on every platform** is the unified row; Windows draws caption
    buttons at the right; Linux uses client-side decorations with a system
    title-bar fallback.
26. **Records and branches.** ADR 0048 for A; B and C live in this document.
    Local branches `ui/redesign`, `ui/redesign-focus`, `ui/redesign-inspector`
    (B, C cut from A); no PRs or release until the owner picks.

**Delivery:** after the interview, implement the agreed design on one branch
plus 1–2 variations (Claude's choice of axis), three branches in all.

## Variant C "Inspector" (branch `ui/redesign-inspector`)

Built on the agreed design (branch `ui/redesign`), changing one axis:

- The panel is a two-tab Inspector, **Review · N | Share**. Review is the
  agreed checklist. Share holds the handoff: a status line, **Copy for AI**
  (filled), Copy Markdown, Save…, **Paste AI reply** with a one-line
  explanation, and the case (its name, or **Save as case**).
- The header's filled button is only Pseudonymize or Apply N. Once everything
  is applied, a quiet outlined **Share** takes its place and opens the Share
  tab (a deviation from "nothing": without it, the next step was invisible).
- Applying everything (⌘↩ or Apply) turns the Inspector to Share.
