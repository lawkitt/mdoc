# UI polish overnight grill, 2026-11-01

Picks desktop UI polish items from the 2026-10-31 UI/UX survey
([evidence](../evidence/ui-ux-survey/2026-10-31/README.md), ROADMAP Later)
for one unattended hub run of 4–5 hours while the maintainer sleeps.

## Round 1 (agreed)

1. **Items and lanes.** Items sharing hot files run in sequence on one lane.
   - **A** (menus, palette, `src/shortcuts.rs`, `src/menus.rs`): U14 one name
     per action → U15 menus → U16 tooltips.
   - **B**: U17 Settings states.
   - **C**: U22 dialogs, then U20 native panels follow the app theme (macOS).
   - **D**: U25 small alignment items, then U31 fuller tab menu.
   - **E**: U21 narrow windows; first to cut if time runs short.
   - Left out: U13 chip spacing (editor layout and hit-testing risk
     unattended), U18, U19, U26 (flow and wording choices), U24 (whether the
     view bar always shows is open), U23 (after shelf acceptance), ideas
     U27–U30.
2. **Filler.** A lane done early takes survey bugs #303, #308, #310, #311, in
   that order; no other issues.
3. **Merge authority.** The hub merges on the fleet gate (local CI green,
   Codex review answered or skipped when Codex is limited, headless renders
   checked in both themes, macOS GUI rows run by the hub); squash, delete the
   branch. Held for the maintainer: anything touching vendored dependencies,
   the installer, signing, updates or release scripts. Windows GUI rows are
   Pending (maintainer).
4. **GUI checks.** The hub may take full-screen control of the Mac for GUI
   rows on `mdoc-dev.app`; profile backed up first, `session.json` and
   `recents.json` restored after each check.
5. **Machines.** More than two Mac builders when memory and CPU allow
   (maintainer). Measured at grill time: M4, 10 cores, 24 GB, 68% memory
   free; disk is the limit at 52 GiB free, about 10–15 GB per lane
   `target/`. Windows lanes run as T3 Claude threads (Opus 5.5 high); the
   hub checks which are free at preflight (Android #314, #322 may hold some).
6. **Records and release.** This record; each PR amends the ADR it changes
   and removes its own ROADMAP line. No new ADR unless a lane decides real
   behavior. No tags or release candidates overnight.

## Round 2 (agreed)

7. **Placement.** Three Mac Desktop sessions (Opus 5.5 high): A, C (has the
   macOS-only U20), E (needs a feel check on the Mac). Two Windows T3 Claude
   lanes: B, D. Filler bugs go to the first lane free. The hub cleans a
   lane's `target/` after its last merge. The seven stale worktrees found at
   grill time stay; the hub deletes nothing it did not create.
8. **Spelling.** US English in every UI string and in CONTEXT.md's UI
   labels, with one CONTEXT.md line saying so. Identifiers and file names
   unchanged.
9. **One name per action.** **Paste AI Reply** (replaces Restore AI reply,
   Paste AI Response, Paste AI reply…), **Save as Case** (replaces Save
   mapping as case), **Side by Side** (replaces Before / After; CONTEXT.md's
   "Before / after view" is renamed), **Switch to Dark Theme** / **Switch to
   Light Theme** (names the target). Casing follows each surface's current
   convention (menus Title Case); no app-wide casing pass.
10. **Menus.** File › Open Recent (up to 10 from recents, Clear Menu); Edit ›
    Restore Clipboard under Pseudonymize Clipboard; View › Home ⇧⌘H, Search
    ⌘K, Edit / Original / Pseudonymized / Side by Side, Show Replacements;
    Help › mdoc Help (what the sidebar's ? opens); Check for Updates… in the
    mdoc menu on macOS, in Help on Windows. Copy for AI ⇧⌘C and Copy
    Markdown ⌥⇧⌘C (Ctrl+Shift+C, Ctrl+Alt+Shift+C); on a clash with the
    keymap or text-field defaults the lane takes the next free chord and
    says so in the PR.
11. **Dialogs.** Platform order. macOS: Don't Save at the left, Cancel and
    the filled default at the right (mapping prompt: Don't Save … Cancel ·
    Save · **Save as Case**). Windows: default first, then Save, Don't Save,
    Cancel. A scrim in both themes. The Quit summary names up to five
    documents, then "and N more". Amends ADRs 0071 and 0064.
12. **Narrow windows.** The sidebar drags between 160 and 320 pt, width
    saved in settings; it collapses by itself below a 900 pt window width and
    returns when the window widens, unless the user collapsed it. Names
    truncate in the middle and keep the extension.
13. **Tab menu.** Reveal in Finder (Show in Explorer on Windows) and Copy
    Path, both hidden for untitled tabs; Close Others asks once per at-risk
    tab in sidebar order with ADR 0071's prompt, and Cancel stops the rest.

## Hub goal

Run as hub on macbook-air until lanes A–E are merged or the window ends
(about 5 hours), following the fleet skill's hub rules and the decisions
above. Preflight: commit this record to `main`, back up the mdoc profile,
check disk, usage (Claude, Codex) and free Windows lanes, Amphetamine and
Remote Control on. Dispatch A, C, E as Mac Desktop sessions and B, D as
Windows T3 Claude threads, each brief carrying its items, the matching
decisions, its Done-when (renders in both themes, tests, ROADMAP line
removed in the PR, ADR amended where noted) and "keep long waits in the
foreground" for T3. Merge on the gate in decision 3; hold what it lists.
Keep a run log in a `hub-ui-polish-overnight` memory. At the end: ROADMAP
and issues tidied, a morning summary (merged, held, Pending (maintainer)
rows, what was cut), no release.

## Changes at hub start (2026-10-31 18:10Z)

- A second hub (the open-issues fix run,
  [record](issue-fixes-grill-2026-11-01.md)) started at the same time and
  owns #303, #308, #310 and #311, so this run has **no filler**.
- The two hubs share the Mac's disk (104 GiB free after that run's cleanup;
  a new builder needs 40 GiB, then ~30 GiB per `target/`), so decision 7's
  placement changes: **Mac** Desktop sessions run A and C (C keeps the
  macOS-only U20); **Windows** T3 Claude lanes run B (`windows-lane-1`), D
  (`windows-lane-2`) and E (`windows-lane-3`); `windows-lane-4` stays free
  for the other hub. The hub GUI-checks E on the Mac.
- The nine items are promoted from ROADMAP Later into a numbered "UI polish"
  section (one item per PR) so lane briefs come from `fleet-brief`.
- Codex weekly is at 0%, so reviews are skipped (fleet rule: skip, never
  queue).
