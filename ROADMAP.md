# Roadmap

mdoc prepares local documents as editable Markdown for external AI tools.
This roadmap tracks work remaining as of 2026-10-17 (latest: 0.1.0-beta.13);
accepted behavior lives in [ADRs](docs/adr/), with terminology in
[CONTEXT.md](CONTEXT.md). Structure and stable scope:
[ADR 0051](docs/adr/0051-roadmap-structure-and-stable-scope.md).

## Current status

Local conversion with PDF/DOCX previews, offline OCR, assistive pseudonymization
([ADR 0034](docs/adr/0034-pseudonymization-supported-feature.md)), encrypted
cases with AI-reply restoration ([ADR 0041](docs/adr/0041-cases-and-ai-response-restoration.md)),
EN/RU spellcheck and the redesigned workspace shell ([ADR 0048](docs/adr/0048-workspace-shell-redesign.md))
ship in betas. Implementation does not establish qualification: native/platform
acceptance is partial and no PII model passed the [initial qualification](tests/fixtures/pseudonymization/README.md).

## 0.1.0 stable

Stable waits only on **must-fix** items. Should-fix items ship if ready;
otherwise they move to the top of [After 0.1.0](#after-010). Items are in
priority order.

### Must-fix

1. **[Platform] Native and platform acceptance** on Apple Silicon macOS and
   Windows x64; keep Linux buildable. Done last, because it checks the finished UI:
   an installed `0.1.0-rc.1` (published 2026-10-19) against the template
   `docs/platform-acceptance.md` ([ADR 0057](docs/adr/0057-native-platform-acceptance.md)).
   macOS is done (2026-10-19): rc.1 with A2 and H3 rerun on rc.2, the
   maintainer's rows included; offline setup (S1) and spoken VoiceOver output
   (A5) were not run for 0.1.0. Claude's Windows pass on rc.2 and the
   maintainer's Windows rows are done (2026-10-19), except the deferred
   scaling (D3, D4), offline setup (S1) and Narrator speech (A5) rows; the T6
   retest waits for the next RC.
   *Done when:* `docs/evidence/platform/<date>/` holds a README plus `macos.md`
   and `windows.md` covering themes, IME (RU; Japanese where available), clipboard,
   VoiceOver/Narrator basics, the popup, setup and file-drop flows, install,
   shortcuts, scaling, case key storage and Cyrillic paths; unavailable checks are
   listed explicitly, and no Fail row lacks a linked roadmap item. Claude
   runs both checklists through computer use (on Windows from Claude Code on
   the maintainer's PC); the maintainer runs the rows that need a physical
   keyboard, system settings or spoken output. The folder is named for the day the
   macOS RC run starts; `windows.md` records its own date.

### Should-fix

1. **[Platform] Windows caption buttons.** *From the 2026-10-19 Windows rc.2
   run ([T6](docs/evidence/platform/2026-10-19/windows.md)).* Minimize,
   maximize/restore and close draw and highlight but do nothing when clicked,
   and no snap layouts flyout appears: the buttons stop the mouse-down
   (`caption_buttons` in `src/ui.rs`), so GPUI never records the pressed
   caption button. A title-bar double-click calls `zoom_window`, which on
   Windows only maximizes, so a maximized window returns only by dragging the
   title bar or Win+↓.
   *Done when:* on Windows the three buttons act on a physical click, hovering
   maximize shows snap layouts, a title-bar double-click toggles maximize and
   restore, and the T6 row passes on the next RC.

## After 0.1.0

In priority order, after any carried-over should-fix items.

1. **[Perf] Stress tier and numerical budgets.** The 2 MiB / 20,000-mention tier
   has bounded work and memory; set numerical budgets from release/native
   baselines. Ordinary documents are done ([native check](docs/evidence/editor-layout/2026-10-17-native/README.md)):
   left for this tier are case fan-out, process memory, the legal opinion's
   25 ms search burst and two ~20 ms typing passes seen natively. [Restoration measurements](https://github.com/lawkitt/mdoc/blob/f368e436cd2d3e70789d51879b9f54f379900e1e/docs/evidence/pseudonymization/2026-10-07-restoration/README.md)
   are historical, not guarantees.
2. **[Editor] Enter starts a paragraph.** *From the 2026-10-19 rc.1 run.* Enter
    in a paragraph inserts one `\n`: the editor shows a new line, but Markdown
    reads it as the same paragraph, so Copy for AI and other renderers join the
    lines. Decide Enter (new paragraph, `\n\n`) versus ⇧Enter (line break) and
    how lists, tables and headings keep their own Enter.
    *Done when:* what the editor shows after Enter is what a CommonMark renderer
    shows for the copied Markdown, with tests on paragraphs, lists and tables.
3. **[Pseudonymization] Popup Enter follows the scope.** *From the 2026-10-19
    rc.1 run.* The popup shows All N as its scope, but Enter applies only this
    mention (`ApplyThis`) and quietly switches the scope to This one. Decide
    whether Enter applies the shown scope or the scope defaults change, and
    amend ADR 0048/0055 to match.
    *Done when:* Enter, the scope segments and the Apply result agree, and a
    test covers Enter in each scope.
4. **[Editor] Find and replace.** *From the 2026-10-17 roadmap grooming.* The
   find field gains a replace row: Replace, Replace all, Match case and Whole
   word; no regex or search in selection. Port the logic and the search bar
   (option toggles, "3 of 12" counter, expanding replace row) from Zed's
   GPL-3.0 `search` crate, crediting it in the ported files; its dependencies
   rule out using the crate itself. Replace all is one undo step; matches inside
   applied aliases are skipped; an edit touching a proposal invalidates it, as
   typing does. Shortcut ⌘⌥F (macOS), Ctrl+H (Windows/Linux).
   *Done when:* each control works with keyboard and mouse on a long document
   with applied aliases, and one undo reverts a Replace all.
5. **[A11y] Expose editor text to screen readers.** *From ADR 0057.* Controls
   are labelled, but VoiceOver and Narrator cannot read or navigate the
   document. Give the editor an AccessKit text role with its value, text
   ranges, selection and caret events.
   *Done when:* VoiceOver and Narrator read the document by line and word,
   follow the caret, and announce selection in a long document with applied
   aliases.
6. **[PII] Merge hint.** *From the 2026-10-17 roadmap grooming.* When two
   identities very likely name the same subject, the Replacements panel shows a
   dismissible hint on the identity ("Looks like Maya Bennett · Merge"). Signals:
   an ADR 0053 short or inflected form with exactly one candidate full name;
   wordings differing only in case, whitespace or punctuation; OCR or typo
   near-duplicates (edit distance 1, names of 6+ characters). Initials are not a
   signal. No score is shown; nothing links without the click (ADR 0053). Merge
   is one undo step with an Undo toast; a dismissed hint stays dismissed for
   that pair.
   *Done when:* tests cover each signal and a non-signal (relatives sharing a
   surname, initials), and Merge, Undo and dismissal work in the live panel.
7. **[UI] Center and smoothly scroll to the selected mention.** Clicking an
   entity or mention row in the Replacements panel reveals, highlights and focuses
   the mention near the vertical center of the editor, clear of the popup and
   clamped at document edges. Animate briefly; rapid selections head for the
   latest target without queued animations; manual scrolling interrupts; Reduce
   Motion positions immediately.
8. **[UI] Expand/collapse all in the Replacements panel.** A compact icon
   control with tooltip and accessible label, preserving selection and
   replacement decisions.
9. **[UI] Settings redesign.** *From the 2026-10-17 roadmap grooming; reference:
   Anoni Settings.* A wider overlay with a left category list: General
   (appearance, updates, recent files), Detection (model, threshold, comparison),
   Recognition (OCR), Spelling (with the dictionary), Cases, About. Each setting
   is one row: title and description left, control right, hairline dividers.
   Switches and segmented controls replace checkbox toggles; changes save
   immediately, with no Apply; Advanced options move into their categories.
   Layout only: no UI language or profiles.
   *Done when:* every current setting is reachable, saved on change and
   operable by keyboard, in both themes.
10. **[Cases] Case page.** *From the 2026-10-17 roadmap grooming.* The sidebar
    case header truncates long names and repeats the tab title; the Share
    panel's Case row only says "saved"; nothing gives an overview of a case. Add a
    case page, opened from the sidebar header, Home and the Share row, listing the
    case's documents and identities, with rename, recolor and delete. Design it
    and accept its ADR before implementation.
    *Done when:* the ADR is accepted and the page ships with those actions,
    verified in both themes.
11. **[UI] Workspace Back/Forward.** Arrow buttons right of the sidebar toggle,
    available with the sidebar expanded or collapsed. Navigate visited documents
    and Home, restore view state, disable unavailable directions, skip closed
    documents, clear forward history on new navigation.
12. **[OCR] Recover page structure.** *Known issue 1.* On a clean one-column A4
    scan (PP-OCRv6 Small, 150 DPI; 300 DPI does not help) paragraphs and headings
    merge, and table rows split or shift; on the *Brown v. Board* page a body line
    becomes a heading. Fix in the AnyDoc/pdf-inspector forks. Add the scan to the
    [OCR qualification set](tests/fixtures/ocr-qualification/README.md) now.
13. **[DOCX] Pseudonymized DOCX export with rdocx, then the preview.** *From the
    2026-10-17 roadmap grooming.* Evaluate [rdocx](https://github.com/tensorbee/rdocx)
    (MIT/Apache, young: 0.16) to write the applied case mapping into the
    original DOCX, keeping its formatting. Other Markdown edits are not carried
    over, and the save dialog says so. Cover headers, footers, footnotes, text
    boxes, comments and their authors, tracked-change authors, document
    properties and hyperlink targets; flag images and embedded objects, which
    are not cleaned. UI follows Anoni's "Pseudonymized Word…" split button, to
    be designed separately. Then evaluate rdocx in place of docxide-pdf for the
    Source preview (fidelity, speed, dependencies; it would lift the zip 9
    advisory ignore in `deny.toml`). Conversion stays in AnyDoc.
    *Done when:* fixture DOCX exports, reconverted and scanned, contain no
    original wording and open in Word with formatting intact; the preview
    decision is recorded with fixture comparisons.
14. **[Cases] System-wide Pseudonymize Clipboard hotkey** (macOS
    `RegisterEventHotKey`, Windows `RegisterHotKey`). The in-app action, before/after
    view and per-case category switches from the [Anoni review](https://github.com/lawkitt/mdoc/blob/f368e436cd2d3e70789d51879b9f54f379900e1e/docs/design/saved-mappings.md#anoni-0810-hands-on-macos-2026-10-11)
    are done. Its on/off switch goes in Settings › General.
15. **[Platform] Dry-run polish.** *From the 2026-10-18 macOS dry run.*
    The preview find field and screen-reader labels were fixed before rc.1.
    - Dragging an unsupported file shows an insertion line instead of "No
      supported files"; the Notices popover shows the full path and stays open.
    - The preview's Find button says "Find (⌘F)", but ⌘F opens the Markdown
      find from anywhere in the window; the preview find opens only by click.
    - The theme button's tooltip keeps the previous value while hovered; the
      setup card's size changes from 665 MB to 662 MB after Cancel.
    *Done when:* each point is fixed or moved to its own item.
16. **[Platform] RC acceptance follow-ups.** *From the 0.1.0-rc.1 macOS run
    ([evidence](docs/evidence/platform/2026-10-19/macos.md)) and the rc.2
    Windows run ([evidence](docs/evidence/platform/2026-10-19/windows.md)).*
    - Save As never adds the saved file to Recents: the row stays the source
      DOCX, which reopens as a fresh conversion (P2). The new file is also
      created with mode 0600.
    - Settings is not exposed as a dialog: its title is not announced and the
      document's controls stay in the accessibility tree behind it (A3); model
      choices read as checkboxes, not radio buttons.
    - The editor lacks the platform caret moves: ⌘←/⌘→ and ⌘↑/⌘↓ (with ⇧) on
      macOS; Ctrl+←/→ word moves (with ⇧), Ctrl+⌫/Ctrl+Del and Ctrl+Home/End
      on Windows, where only Alt+←/→ moves by word.
    - View › Toggle Sidebar has no shortcut (⌃⌘S is the macOS convention).
    - Plain Tab cannot reach the toolbar or sidebar: Tab cycles only once
      focus is in a control, Home keeps focus on the window and the editor
      uses Tab to indent (A4, H18). Screen readers and macOS Full Keyboard
      Access reach every control ([ADR 0057](docs/adr/0057-native-platform-acceptance.md)).
      Likely F6 / ⇧F6 between sidebar, toolbar and editor, and Tab from Home
      into the controls.
    - Dropping from Finder does not activate mdoc, so a modal the drop opens
      (the Keychain explainer) does not get Esc or other keys.
    - Tabs with the same name in the same folder (a DOCX's conversion and an
      `.md`) fall back to the full absolute path, home folder included;
      disambiguate by source type instead.
    - The DOCX notice "Equations, charts, SmartArt… may render incompletely"
      shows on every DOCX; show it only when the file has such content.
    - After ⌘↩ Apply all the popup stays open on a mention that is now applied.
    - In a narrow window Markdown/Source are buttons, not tabs, for screen
      readers.
    - Windows has no single-instance launch: every Start menu launch or
      "Open with" starts another process and window that restores the same
      session, and both write the session and Recents. The installer
      registers no file associations, so mdoc is not offered under Open with.
    - Windows has no menu bar, but the palette lacks Before / After, Toggle
      Sidebar, Save As and Close Tab; Settings › Saved cases still says
      "File → Open Case…".
    - In Settings, Tab focus is not reported to UI Automation (the window
      stays focused), so Narrator may not follow it.
    - Formatting a selection that ends in a space puts the closing marker
      after the space (`**delta **`), which CommonMark does not render.
    - After some button clicks (Share › Save as case) and after an undo that
      closes the popup, focus leaves the workspace: Ctrl+Q and the popup's
      redo do nothing until the editor is clicked.
    - Save as case on an unsaved conversion saves a case with 0 documents
      that cannot be reopened; ask to save the document first.
    - A search match that wraps also paints a block at the end of the
      previous line; after Save As the tab keeps its DOCX badge; the window
      title names the last document while Home shows; every tab's close
      button reads only "Close tab"; the unsaved-changes dialog is titled
      "Warning".
    *Done when:* each point is fixed or moved to its own item.
17. **[Workspace] Folder drop flow.** *From the 2026-10-19 rc.1 run.* Dropping a
    folder opens every document in it (README.md included) and asks for the
    Keychain before anything was chosen. Decide what a folder drop offers (a
    file list, Open as case, a size limit) and when the case key is read.
    *Done when:* a folder drop shows what will open before it opens, asks for
    the key only when a case is created or unlocked, and a large folder cannot
    flood the sidebar.
18. **[OCR] Pick the recognition language.** *From the 2026-10-19 rc.1 run.* The
    OCR card defaults to the English model for a Cyrillic scan and only warns.
    Choose English & Russian when the text layer, the file name or an installed
    Russian input source suggests it; the user can still change it.
    *Done when:* the Russian OCR fixture gets English & Russian by default and
    the English one stays on English, with the choice explained on the card.
19. **[Platform] Deferred macOS acceptance rows.** *From the 2026-10-19 macOS
    run ([evidence](docs/evidence/platform/2026-10-19/macos.md)).* The
    maintainer did not run S1 (offline first setup: the consent card, then
    Download & continue failing cleanly with Retry) or A5 (VoiceOver speaks
    scan status and toasts) for 0.1.0.
    *Done when:* both rows pass on the next release's macOS run, or fail with
    a linked item.
## Ongoing tracks

- **Outgoing AI-analysis qualification.** A continuous quality gate, not a
  release blocker. Scope: EN/RU legal text from native/OCR PDF, DOCX and opened
  Markdown on Apple Silicon macOS and Windows x64; the handoff is the complete
  current Markdown, including hidden source ([ADR 0016](docs/adr/0016-pii-production-qualification-scope.md)).
  No scan count establishes complete coverage, and remaining context can still
  identify subjects.
  *Done when:*
  - numerical release targets are agreed from measured baselines on a larger
    holdout;
  - category, boundary and hidden-source misses, false positives, mistaken
    identity joins/splits, wrong ownership and correction time are measured
    (detector precision/recall alone does not qualify relationship-preserving output);
  - native/OCR EN/RU legal documents, ambiguous names/initials/inflections,
    tables, links/code/HTML, damaged identifiers and long documents are covered;
  - source integrity, explicit setup/offline operation, cancellation, stale
    results, edits/undo, tab switching, copy/save and provenance lifetime are
    verified.

  Native acceptance is tracked under must-fix item 1; ordinary-document
  [responsiveness](docs/evidence/editor-layout/2026-10-17-native/README.md) is done.
  Historical results: [initial qualification](tests/fixtures/pseudonymization/README.md)
  (no model passed) and the [last gate review](https://github.com/lawkitt/mdoc/blob/f368e436cd2d3e70789d51879b9f54f379900e1e/docs/design/pseudonymization-production-readiness.md).

## Engineering

- **Architecture refactoring** ([ADR 0049](docs/adr/0049-measured-stabilization-and-architecture-review.md)).
  The [layout profiling](docs/evidence/editor-layout/2026-10-16/README.md) and
  [native check](docs/evidence/editor-layout/2026-10-17-native/README.md) found
  local whole-document costs, now fixed; they do not call for a migration.
  Deliver case coordination and document lifecycle as separate, incremental
  projects: explicit owners for case persistence and synchronization, and
  explicit document transitions across the shell/view. GPUI keeps
  presentation, prompts and scheduling; editor/history, import and preview
  owners keep their contracts. Define each boundary before implementation.
  Retained editor layout is not committed work: ordinary
  documents meet the release target without it; reconsider only for the stress
  tier.
- **Build-cache check** ([ADR 0050](docs/adr/0050-rust-build-cache-growth.md),
  [measurements](docs/evidence/build-cache/2026-10-16/README.md)). Re-measure
  with `just disk` around 2026-10-23. Evaluate `sccache` only if `target/`
  exceeds about 30 GiB, accounting for its own disk use. Keep release settings;
  no automatic eviction or cache caps.
- **Cleanup charter** ([ADR 0029](docs/adr/0029-codebase-cleanup-charter.md)).
  Concrete ownership/test improvements. Conversion and OCR algorithm changes
  belong in the AnyDoc/pdf-inspector forks, backed by source/output comparisons.

## Deferred

- An optional case passphrase on top of the OS keyring
  ([ADR 0045](docs/adr/0045-prompt-free-case-key.md)).
- Exporting or sharing a case key (encrypted file plus passphrase), a
  cross-case dictionary, deliberate forgetting of originals, and stronger
  anonymization claims.
- Format-preserving pseudonymized PDF export and restoring an AI reply into
  DOCX (DOCX export is the After 0.1.0 item "Pseudonymized DOCX export"); an audit journal and
  compliance report of replacements; a re-identification risk hint.
- Bulk conversion beyond current lazy multi-file opening; design separately.
- Online formatting/OCR repair; provider, credentials, trust and review need decisions.
- Extra recognizers/full PII engines and optional source/page references; qualify
  a concrete workflow before expanding scope.

## Not planned

- Realistic fake-name pseudonyms: harder to restore and may collide with real
  names ([ADR 0018](docs/adr/0018-document-local-identity-review.md)).
- An irreversible redaction mode ([ADR 0022](docs/adr/0022-remove-anonymization-mode.md)).
- Watermarking and watched folders (outside AI preparation).
- [xerj](https://github.com/xerj-org/xerj) (local search server for agents): no
  product fit for a single-document editor; a local server and its optional
  off-machine reranker conflict with offline operation.
- Replacing AnyDoc with rdocx for DOCX-to-Markdown conversion: conversion
  changes belong in the AnyDoc fork ([ADR 0029](docs/adr/0029-codebase-cleanup-charter.md)).
