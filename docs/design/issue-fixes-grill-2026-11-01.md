# Open issues fix run: grill (2026-11-01)

The maintainer asked Claude to fix every open GitHub issue it can, as one
goal run while they are away. This records what was agreed before the run.

## Scope and the fix for each issue

| Issue | In | Fix |
|---|---|---|
| #300 Paste text opens an empty tab | Yes | The clipboard text goes straight into the new buffer instead of a `Paste` dispatch; a test covers it. |
| #321 iOS restored replies on the shared clipboard | Yes | Both restored-text copies use `.localOnly` and a ten-minute expiry; `RestoredTextView` turns Writing Tools off. A matching desktop flag goes to the roadmap's Later section. |
| #304 Shelf counts identities, editor counts mentions | Yes | The shelf shows mentions ("24 replaced"), the editor's number; the hover reads "14 names: 8 people · 3 organisations · …". |
| #303 Selection ring keeps the original's width | Yes | The ring follows the applied alias's width. |
| #308 Three-digit counts squeeze the chevron | Yes | The count column fits four digits and the chevron keeps its width. |
| #310 Tooltip stays over the context menu | Yes | Opening a context menu dismisses tooltips. |
| #311 Background tab shows a `.md` that doesn't exist | Yes | The source name until a `.md` exists. |
| #305 ₽ as an empty box in the DOCX preview | Yes | A fallback font covering the currency symbols in the preview renderer. |
| #309 Scan footer says "Nothing to replace yet" | Yes | "Scanning…" in the footer, Rescan off during a scan, a chunk count on long documents. |
| #306 Recent badges unapplied files, lists conversions twice | Yes | The badge only with applied replacements; a source folds into the `.md` saved from it. |
| #307 Locked-case messages contradict Home | Partly | "N saved cases are locked"; the popover anchored to its link; the hint names Share › Save as case. The bell rule stays as [ADR 0043](../adr/0043-deferred-keychain-access.md) has it, since a locked case's members can't be known. |
| #301 Copy for AI keeps addresses, dates of birth, a first name | Partly | Readiness flags single words of known multi-word originals ("Марина"); Share stops saying "aliases now" while leftovers remain. Address and date-of-birth detection gets its own grill; the issue stays open (`Refs`). |
| #302 Same text scans differently in a longer document | Yes, last, time-boxed | Find what changes with context and fix it; filter titles and role words; a long fixture joins the qualification set. |
| #254 GUI rows pending (maintainer) | No | Maintainer-only rows (Windows, clipboard menu items computer use refuses). |

## How the run works

- **Where:** this Desktop session is the hub. Items run in parallel where
  their files don't overlap: at most two Mac builders (fleet plan.md), the
  rest as T3 Claude threads on free Windows lanes. One PR per issue.
- **Merging:** the hub merges each PR on the fleet merge gate (local CI
  green, Codex review answered or skipped when Codex is limited, renders for
  UI changes, GUI check done or listed as Pending (maintainer)); squash,
  delete the branch, close the issue. A fix that turns out to need a product
  decision beyond this table is held for the maintainer.
- **GUI checks:** computer use on `mdoc-dev.app`, background first, full
  screen allowed while the maintainer is away; the profile is backed up and
  restored around each check. iOS checks in the Simulator; the Universal
  Clipboard row is the maintainer's.
- **Done:** every in-scope issue is closed by a merged PR, or left open with
  a comment saying why (blocked, time-box, needs a grill). Blocked issues
  are skipped and their questions collected for the end. A Claude limit
  stops the hub after its handoff. Cutting rc.2 is not part of the run.
- **Before the run** (approved): unused build output was removed: local CI
  outputs of closed PRs (~37 GB), seven merged or closed worktrees, the root
  checkout's `target/` (14 GB) and stale incremental caches.
