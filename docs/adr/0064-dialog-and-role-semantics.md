# Dialog and role semantics

Status: Accepted, 2026-10-22.

## Problem

The RC runs (A3) found controls announced with the wrong role or name:
Settings is not a dialog; model choices read as checkboxes; the narrow
Markdown/Source switch reads as buttons; close buttons say only "Close tab";
the unsaved-changes dialog is titled "Warning" on Windows, because the
platform prompt titles itself from its level and lives in `gpui-pre-platform`,
which mdoc does not vendor. The find bar's match counter ("14/16") and the
mention popup's counter ("4/6") were not in the accessibility tree at all.

## Decision

- **Settings** is a modal dialog named "Settings" that keeps focus.
- **Model choices** are a radio group named after their section; each model
  is a radio button announced checked or not checked.
- **Markdown/Source** in the narrow layout, where one pane shows at a time,
  is a radio group named "View". The wide layout shows both panes; its
  Show / Hide original stays a button.
- **Close buttons** are named "Close <tab title>", using the same
  disambiguated title as the tab.
- **Unsaved changes** use an in-app dialog built like the Keychain explainer
  (`src/workspace_keychain.rs`), not the platform prompt. The role, name and
  focus are then ours and identical on every platform; it is no longer a
  native macOS sheet.
- **Every platform prompt goes.** All `window.prompt` call sites (unsaved
  changes, model removal, case actions) use one shared in-app confirm dialog,
  so no dialog is titled by its level ("Warning", "Information").
- **Unsaved changes:** "Save changes to “<tab title>”?" with Save (default,
  Enter), Discard (⌘D / Alt+D) and Cancel (Esc). Focus starts on Save, stays
  inside, and returns where it was on close. Enter answers Save even after a
  click on the dialog's text.
- **Counters are read and announced.** The find bar's match counter and the
  mention popup's position are text nodes whose value is words, not the
  drawn "14 / 16": "Match 14 of 16", "3 matches", "No matches", "Mention
  4 of 6". Each is a polite live region, so VoiceOver and Narrator read the
  new value when it changes without moving focus. "Searching…" and replace
  results ("Replaced 2 · skipped 1") use the same node until Next or Previous
  brings the position back. One stable element id
  per counter (`ui::live_text`) keeps a change an update, not a new node.
- **A dialog keeps focus.** Files opened from Finder or Explorer while a
  confirm dialog is up open once it is answered.

## Consequences

- Done when VoiceOver and Narrator announce each with its proper role and name,
  and each counter change; the accessibility tree shows the counter's value
  and, on macOS, an `AXAnnouncementRequested` per change.
  Narrator is checked in a later Windows acceptance run.
- Find and replace (ADR 0063) and this ADR ship as separate PRs.
- GPUI 0.3 cannot mark a dialog modal for assistive technology: a screen
  reader can still move to controls behind Settings or a confirm dialog.
