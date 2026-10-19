# Native platform acceptance

Status: Proposed, 2026-10-18. Applies [ADR 0051](0051-roadmap-structure-and-stable-scope.md)
(native acceptance in stable scope).

## Problem

Headless tests do not establish native appearance, shortcuts, IME, assistive
technology, installers or GPU presentation (docs/development.md). The only
native evidence so far is the 2026-10-17 performance check on macOS. The
ROADMAP must-fix item asks for a checklist per shipped target but leaves the
build, machines, scope, layout and failure handling open.

## Decision

- **Build.** Acceptance checks `0.1.0-rc.1`, installed from the CI release
  artifacts (`.dmg`, NSIS `.exe`) on each machine, so signing, notarization,
  SmartScreen and the installer are part of it. It is cut after the Cases
  fixtures (ADR 0056) and the pre-RC fixes land (2026-10-19): the dry-run
  findings that would fail A1, A2 or K7 under the triage below (screen-reader
  labels, the preview find field's text input) and a physical-keyboard check
  of ⌘⇧S, ⌘⇧H and ⌘↩. Fixing them first avoids a certain rc.2.
- **Dry run.** Before the RC, Claude runs the macOS template on the latest
  beta; results stay out of the tree and failures become fixes or roadmap
  items at once. Only the RC run is recorded as evidence. The beta.13 dry run
  (2026-10-18) is the dry run; changes landed after it (menus, ADR 0058; the
  Keychain explainer) are covered by the RC run, not a second dry run.
- **Machines.** The M4 reference Mac (macOS 15.7.x) and the maintainer's
  Windows 11 x64 PC; exact OS build and GPU are recorded. Windows 10 is listed
  as not checked.
- **Runners.** Claude runs the macOS checklist through computer use (VoiceOver
  through its caption panel) and marks rows it could not observe for the
  maintainer to confirm. Computer use injects keys without a keyboard layout
  and cannot drive VoiceOver speech or the Keychain password dialog, so the
  keyboard/IME rows, spoken output and Keychain rows are the maintainer's.
  The maintainer runs the Windows checklist, and runs the Russian rows but not
  the Japanese ones.
- **Scope.** Light, Dark and System themes (including a live OS appearance
  switch, the Original page tone and popup/dialog contrast); IME; clipboard;
  VoiceOver/Narrator basics; the replacement popup, first-use setup and
  file-drop flows; install and first launch (Gatekeeper/notarization;
  SmartScreen and the VC++ Redistributable); platform shortcuts; display
  scaling (Windows 125 % and 150 %, Retina and a 1× display, Unavailable if no
  such monitor is at hand); case key storage (Keychain, DPAPI); open, save and
  drop of files with Cyrillic names and spaces in Cyrillic folders. A
  non-ASCII Windows username, in-app update (`just smoke-update`) and
  multi-monitor are out of scope.
- **Clipboard.** Copy for AI pasted into ChatGPT and Claude in a browser
  arrives as plain text with the note; Copy Markdown pasted into
  TextEdit/Notepad is byte-identical, Cyrillic and CJK included; Paste AI reply
  from a browser selection with HTML and plain text uses the plain text;
  in-app Pseudonymize Clipboard round-trips; copy and paste between editor
  tabs.
- **Shortcuts.** Every bound shortcut and menu item, listed from the code, is a
  row on each platform.
- **Screen readers.** "Basics" means controls: toolbar, sidebar, panels,
  popup buttons and dialogs announce name and role and are reachable without
  a mouse through the screen reader (VoiceOver, Narrator) or macOS Full
  Keyboard Access. Plain Tab cycles inside panels, the popup and dialogs but
  does not reach the toolbar or sidebar from Home or the editor; that is an
  After 0.1.0 item, not a 0.1.0 blocker (2026-10-19, rc.1 row A4). The editor's text is not exposed to assistive technology; this is
  a recorded known limitation, and exposing it is an After 0.1.0 item.
- **Setup.** Starts from a wiped app-data directory: online model setup
  completes; an offline first launch shows the setup error/retry state
  without crashing.
- **IME.** Russian is typed in the editor and the find fields, plus the
  spellcheck underline on a misspelled RU word. Japanese (built-in IME) rows
  check marked-text underline, candidate window at the caret, commit, Esc
  cancels and one undo removes the committed text; the maintainer does not
  run them (2026-10-19), so they are Unavailable on both platforms unless
  Claude observes them on macOS by switching to the Japanese input source and
  pressing romaji keys through computer use. Headless tests cover the
  composition calls (marked text, then commit) reaching each find field.
- **Linux.** Buildable means the existing CI `lint (ubuntu)` job passes; no
  runtime checks. The README says so.
- **Layout.** A reusable template, `docs/platform-acceptance.md`. Each run is
  `docs/evidence/platform/<date>/`, named for the day the macOS run starts
  (`windows.md` records its own date in its header), with `README.md` (summary, environment,
  build, unavailable checks), `macos.md` and `windows.md`. Every row is Pass,
  Fail or Unavailable with a note. The maintainer fills and commits
  `windows.md` from the template. Screenshots only for failures, of public
  fixtures only.
- **Failures.** A failure that loses data, blocks a listed flow, or leaves a
  control unreachable by screen reader (or by Tab where Tab already cycles:
  panels, the popup, dialogs) is must-fix: fix, cut a
  new RC, rerun the affected rows. Cosmetic or rare failures become After
  0.1.0 items. The item is done when no Fail row lacks a linked roadmap item;
  this ADR then becomes Accepted.

## Consequences

- Stable ships with recorded native evidence for both shipped targets, and
  the template makes later releases repeatable.
- Screen-reader users can operate controls but cannot read the document in
  0.1.0.
- Keyboard-only users without a screen reader reach the toolbar and sidebar
  through macOS Full Keyboard Access, shortcuts, menus and the ⌘K palette;
  Windows has no Full Keyboard Access, so there Narrator, shortcuts and the
  palette carry it until plain Tab reaches every control.
