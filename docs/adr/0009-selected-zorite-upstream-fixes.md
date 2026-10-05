# Select narrow Zorite correctness and rendering changes

Status: accepted, 2026-10-05. The user confirmed shared understanding and authorized
implementation with "agree, implement".

## Decision

Reuse selected Zorite mainline changes integrated since 2026-09-18, through
reviewed upstream head `4aad847f57a5ac3e8ec0b6220e25d9db7e5264b4`:

- Correct Linux shaped glyph positions by applying combining-mark offsets.
  Temporarily vendor the affected `gpui-pre-wgpu` source, preserve its license,
  exclude it from workspace checks, and document removal once an adopted GPUI
  release includes the correction.
- Ensure the repacked AppImage launcher is executable by every user and fail
  packaging when an executable remains owner-only. Do not claim firejail success.
- Replace the existing heading/list marker golden-ratio literal with the standard
  constant for Clippy compatibility.
- Render existing `==highlights==`, supported colored `<mark>` and `<span>` syntax
  consistently in the editor and Markdown renderer. Preserve source bytes and
  complete raw-source Markdown handoff. Add no formatting action, swatch menu,
  picker or UI toolkit migration.

Port only relevant patches, retaining upstream attribution and mdoc's current
source/lifecycle behavior. General dependency updates, GitHub Action updates and
Winget diagnostics are deferred. Journal/property/template/game/import/export
features removed from mdoc remain outside scope.

## Rationale and invariants

These ports repair retained code or display formatting already present in input
documents without expanding mdoc's product workflow. A full merge would mix
unrelated journal features and release machinery into the simplified app.

Preserve caret/selection, undo, source offsets, search geometry, diagnostic
remapping and pseudonymization protections. Syntax recognition must preserve
literal comparisons, escaping, code and table behavior. Unsupported styling must
retain the source rather than rewrite it. Rendering does not normalize, strip or
translate formatting on save or Copy Markdown.

## Delivery and acceptance

Deliver a review branch and PR, relevant rendering/source regressions, repository
gate and available native checks. Unavailable Linux visual
or actual AppImage launch checks remain explicitly pending, without inferring
platform acceptance from headless tests. No release or main merge is authorized
by this decision.

See [the upstream inventory and decision tree](../design/zorite-upstream-triage.md).

## Implementation evidence

The fixes and rendering support are implemented with upstream attribution.
mdoc-specific regressions required source-aware marker offsets after escaped or
entity-decoded text, visible reader search/table measurement, restoration of nested
HTML styles, and literal handling of unsupported tags and code-span contents.
No formatting action or color picker was added.

The [verification record](../../tests/fixtures/zorite-upstream/README.md) reports
374 passing workspace tests, 11 ignored tests, formatting/strict Clippy,
dependency/license/source checks, workflow lint and permission fixtures.
A native macOS standalone-editor smoke check confirmed the new formats paint;
Linux glyph placement and actual AppImage launch remain pending.

Review delivery: [mdoc PR #10](https://github.com/lawkitt/mdoc/pull/10).
The initial implementation commit is
`7a895e3b8580e6604881d869ebe56f1d21a6b59c`; later documentation records delivery.
No main branch merge or release was performed.
