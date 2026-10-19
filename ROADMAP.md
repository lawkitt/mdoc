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

1. **[Platform] Pre-RC fixes.** *From the 2026-10-18 macOS dry run; split out of
   After 0.1.0 item 13 because they would fail rows A1, A2 and K7 under the
   ADR 0057 triage and force an rc.2.*
   - The DOCX/PDF preview's find field builds text from key codes, not the
     text-input path: injected text arrives garbled and IME likely fails.
   - Screen-reader labels: Replacements panel checkboxes all read "Will be
     replaced" without the identity; the panel search is not a text field;
     zoom buttons read "+" or nothing; sidebar tabs are buttons, not tabs.
   - Confirm ⌘⇧S, ⌘⇧H and ⌘↩ on a physical keyboard with a `just run` build
     (the maintainer); the dry run saw them ignored three times without a repro.
     A real failure is fixed here.
   *Done when:* each point is fixed or confirmed, a headless test composes and
   commits Japanese in the preview find field, and VoiceOver reads each control
   above with its name and role. The native Japanese attempt needs the Japanese
   input source on the reference Mac, which only the maintainer can enable;
   without it the Japanese rows stay Unavailable.
2. **[Platform] Native and platform acceptance** on Apple Silicon macOS and
   Windows x64; keep Linux buildable. Done last, because it checks the finished UI:
   an installed `0.1.0-rc.1`, cut once item 1 lands, against the template
   `docs/platform-acceptance.md` ([ADR 0057](docs/adr/0057-native-platform-acceptance.md)).
   The macOS dry run on beta.13 (2026-10-18) is done; the RC run covers the
   later menus (H17) and Keychain explainer (section 10).
   *Done when:* `docs/evidence/platform/<date>/` holds a README plus `macos.md`
   and `windows.md` covering themes, IME (RU; Japanese where available), clipboard,
   VoiceOver/Narrator basics, the popup, setup and file-drop flows, install,
   shortcuts, scaling, case key storage and Cyrillic paths; unavailable checks are
   listed explicitly, and no Fail row lacks a linked roadmap item. The
   maintainer runs the Windows checks. The folder is named for the day the
   macOS RC run starts; `windows.md` records its own date.

### Should-fix

None open.

## After 0.1.0

In priority order, after any carried-over should-fix items.

1. **[Perf] Stress tier and numerical budgets.** The 2 MiB / 20,000-mention tier
   has bounded work and memory; set numerical budgets from release/native
   baselines. Ordinary documents are done ([native check](docs/evidence/editor-layout/2026-10-17-native/README.md)):
   left for this tier are case fan-out, process memory, the legal opinion's
   25 ms search burst and two ~20 ms typing passes seen natively. [Restoration measurements](https://github.com/lawkitt/mdoc/blob/f368e436cd2d3e70789d51879b9f54f379900e1e/docs/evidence/pseudonymization/2026-10-07-restoration/README.md)
   are historical, not guarantees.
2. **[Editor] Find and replace.** *From the 2026-10-17 roadmap grooming.* The
   find field gains a replace row: Replace, Replace all, Match case and Whole
   word; no regex or search in selection. Port the logic and the search bar
   (option toggles, "3 of 12" counter, expanding replace row) from Zed's
   GPL-3.0 `search` crate, crediting it in the ported files; its dependencies
   rule out using the crate itself. Replace all is one undo step; matches inside
   applied aliases are skipped; an edit touching a proposal invalidates it, as
   typing does. Shortcut ⌘⌥F (macOS), Ctrl+H (Windows/Linux).
   *Done when:* each control works with keyboard and mouse on a long document
   with applied aliases, and one undo reverts a Replace all.
3. **[A11y] Expose editor text to screen readers.** *From ADR 0057.* Controls
   are labelled, but VoiceOver and Narrator cannot read or navigate the
   document. Give the editor an AccessKit text role with its value, text
   ranges, selection and caret events.
   *Done when:* VoiceOver and Narrator read the document by line and word,
   follow the caret, and announce selection in a long document with applied
   aliases.
4. **[PII] Merge hint.** *From the 2026-10-17 roadmap grooming.* When two
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
5. **[UI] Center and smoothly scroll to the selected mention.** Clicking an
   entity or mention row in the Replacements panel reveals, highlights and focuses
   the mention near the vertical center of the editor, clear of the popup and
   clamped at document edges. Animate briefly; rapid selections head for the
   latest target without queued animations; manual scrolling interrupts; Reduce
   Motion positions immediately.
6. **[UI] Expand/collapse all in the Replacements panel.** A compact icon
   control with tooltip and accessible label, preserving selection and
   replacement decisions.
7. **[UI] Settings redesign.** *From the 2026-10-17 roadmap grooming; reference:
   Anoni Settings.* A wider overlay with a left category list: General
   (appearance, updates, recent files), Detection (model, threshold, comparison),
   Recognition (OCR), Spelling (with the dictionary), Cases, About. Each setting
   is one row: title and description left, control right, hairline dividers.
   Switches and segmented controls replace checkbox toggles; changes save
   immediately, with no Apply; Advanced options move into their categories.
   Layout only: no UI language or profiles.
   *Done when:* every current setting is reachable, saved on change and
   operable by keyboard, in both themes.
8. **[Cases] Case page.** *From the 2026-10-17 roadmap grooming.* The sidebar
   case header truncates long names and repeats the tab title; the Share
   panel's Case row only says "saved"; nothing gives an overview of a case. Add a
   case page, opened from the sidebar header, Home and the Share row, listing the
   case's documents and identities, with rename, recolor and delete. Design it
   and accept its ADR before implementation.
   *Done when:* the ADR is accepted and the page ships with those actions,
   verified in both themes.
9. **[UI] Workspace Back/Forward.** Arrow buttons right of the sidebar toggle,
   available with the sidebar expanded or collapsed. Navigate visited documents
   and Home, restore view state, disable unavailable directions, skip closed
   documents, clear forward history on new navigation.
10. **[OCR] Recover page structure.** *Known issue 1.* On a clean one-column A4
    scan (PP-OCRv6 Small, 150 DPI; 300 DPI does not help) paragraphs and headings
    merge, and table rows split or shift; on the *Brown v. Board* page a body line
    becomes a heading. Fix in the AnyDoc/pdf-inspector forks. Add the scan to the
    [OCR qualification set](tests/fixtures/ocr-qualification/README.md) now.
11. **[DOCX] Pseudonymized DOCX export with rdocx, then the preview.** *From the
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
12. **[Cases] System-wide Pseudonymize Clipboard hotkey** (macOS
    `RegisterEventHotKey`, Windows `RegisterHotKey`). The in-app action, before/after
    view and per-case category switches from the [Anoni review](https://github.com/lawkitt/mdoc/blob/f368e436cd2d3e70789d51879b9f54f379900e1e/docs/design/saved-mappings.md#anoni-0810-hands-on-macos-2026-10-11)
    are done. Its on/off switch goes in Settings › General.
13. **[Platform] Dry-run polish.** *From the 2026-10-18 macOS dry run.*
    The preview find field and screen-reader labels moved to must-fix item 1.
    - Dragging an unsupported file shows an insertion line instead of "No
      supported files"; the Notices popover shows the full path and stays open.
    - The theme button's tooltip keeps the previous value while hovered; the
      setup card's size changes from 665 MB to 662 MB after Cancel.
    *Done when:* each point is fixed or moved to its own item.

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

  Native acceptance is tracked under must-fix item 2; ordinary-document
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
  DOCX (DOCX export is After 0.1.0 item 10); an audit journal and
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
