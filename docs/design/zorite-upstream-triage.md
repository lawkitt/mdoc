# Zorite upstream triage since 18 September 2026

Status: accepted, 2026-10-05. The user confirmed shared understanding and authorized
implementation with "agree, implement".

## Review boundary and method

Review upstream `main` changes integrated from 2026-09-18 through the live
2026-10-05 check, using mainline merge dates rather than feature author dates.
The final mainline commit before the cutoff is
`3f14fafba120386b058e3cde24ced55f23dd653e` (2026-09-11).
The reviewed head is `4aad847f57a5ac3e8ec0b6220e25d9db7e5264b4`
(2026-10-01). The interval contains 19 mainline merge commits.

Sources: [upstream history](https://github.com/packetThrower/zorite/commits/main/),
GitHub commit API, and an isolated bare clone. Compared actual patches with mdoc
`daadde29567f2740a30a3f4aa038e4a802b6496b`, its manifests, lockfile and local GPUI source.
The working tree was clean before research. Initial research used source inspection;
implementation checks are recorded in the verification note linked below.

Release 0.12.0 also describes GPUI Kit, Settings, command palette and link previews
already integrated on 11 September in the boundary commit. Those headlines are
outside this interval. Highlights were authored on 11 September but merged on
22 September and therefore belong in this review.

## Applicable candidates

| Change | Evidence in mdoc | Treatment agreed in round 1 |
| --- | --- | --- |
| [Linux glyph offsets](https://github.com/packetThrower/zorite/commit/9bb9ed10d9e8285e39deccc5ccfcb46fdbc4c3ab) | Locked registry `gpui-pre-wgpu` 0.3.2 drops shaped glyph x/y offsets; inspected local source confirms the affected expression. | Port the narrow correction with upstream source/license preserved, excluded vendor directory and explicit removal condition. A GPUI family upgrade requires a separate justification. |
| [AppImage launcher permissions](https://github.com/packetThrower/zorite/commit/f022066792ebac25c91bcd169f1f9736b0ba77e0) | mdoc retains the extraction/repacking step without launcher chmod or executable permission assertion. | Add 0755 launcher permission and packaging assertion. Incorporate the later changelog correction: this does not establish successful firejail operation. |
| [GOLDEN_RATIO Clippy compatibility](https://github.com/packetThrower/zorite/commit/e7bb3ca5f21e7837a975391d3f07b0be54b782a5) | `crates/mdoc-markdown/src/view.rs` retains the same 1.618_034 literal. Local Rust is 1.98.1; upstream reports a new 1.99 lint. | Port constant substitution; rendering value unchanged. |
| [Markdown highlighting/color](https://github.com/packetThrower/zorite/commit/594e8e629aaf179ab585a844f12d21ff775f95ea) | mdoc already handles ordinary `<mark>` but lacks upstream `highlight_markers`, CSS color parsing, styled spans and color-selection actions. | Render existing `==highlights==`, supported styled marks and colored spans. No highlight action, color-selection UI or picker. |
| [lopdf 0.45](https://github.com/packetThrower/zorite/commit/e82ca5965fb842b010ce341146baafe2f3abd5bd) | mdoc's local `gpui-pdf` forms feature still requests 0.44; lockfile also contains 0.45 for another dependency. | Deferred to separate maintenance. |

## Already present or outside mdoc scope

- `rustls` 0.23.45 is already locked. Upstream's lockfile security change needs no repeat.
- Properties: upstream text-field selection/clipboard/navigation, block height and
  click-away caret fixes belong to the journal property form mdoc removed. Do not
  import that subsystem or its otherwise unused editor height getter.
- Journal tab game, slash-menu templates, Obsidian/Logseq importer changes and
  PDF export library updates address removed product paths.
- Crate version releases do not require changing mdoc's private crate versions.
- [Winget fork-sync diagnostics](https://github.com/packetThrower/zorite/commit/fd2e1b32acc4)
  also apply to mdoc's retained `after_release.yml`, which currently swallows sync
  errors. Deferred to separate release maintenance. It does not authorize
  token changes, publishing, or submitting package-manager PRs.
- GitHub Action revisions can be reviewed separately for actions actually retained;
  docs/Nix/package-manager automation and full upstream Cargo.lock are not useful ports.

## Decision tree

Confirmed starting constraint: preserve the stripped-down file-based app and its
current source/lifecycle contracts. This is an upstream reuse review, not an app
redesign. Agreed approach: selected adapted patches, avoiding a full main merge.

Round 1 confirmed by the user's "agree":

1. Adopt all three correctness fixes, including temporary GPUI vendoring.
2. Render existing highlighting/color syntax only. New formatting actions, color
   menus and the picker are excluded. The editing-UI branch is therefore closed.
3. Defer lopdf, retained Action revisions and Winget diagnostics to maintenance.

Round 2 confirmed by "agree, implement":

4. Deliver a review branch and PR, preserving upstream attribution; main merge
   is separate.
5. Run the automated gate and available native checks. Deliver the tested review
   PR with unavailable Linux visual/AppImage acceptance explicitly pending,
   without claiming cross-platform acceptance.

The frontier is closed and implementation is authorized. See
[ADR 0009](../adr/0009-selected-zorite-upstream-fixes.md).

## Proposed verification principles

For chosen changes, run relevant crate tests and the repository gate. Preserve
complete raw Markdown handoff, Unicode/source offsets, undo/caret/selection,
diagnostic remapping and pseudonymization stale-result protections.
Any syntax support must preserve literal comparison text, escaping, code regions,
table behavior, and source bytes until the user explicitly edits.
Headless passing results do not establish native Linux mark placement or real
AppImage launch acceptance; unavailable platform checks must be recorded explicitly.

## Implementation

The accepted fixes and rendering support are implemented on
`port/zorite-rendering-fixes`. See the
[verification record](../../tests/fixtures/zorite-upstream/README.md).
The rendering port includes necessary corrections for decoded source offsets,
reader search, nested styles, code spans and unsupported styling. General
dependencies, action revisions and removed feature paths were not adopted.
