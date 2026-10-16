# Select narrow Zorite correctness and rendering changes

Status: Accepted, 2026-10-05. Linux packaging is moot since
[ADR 0037](0037-ci-and-release-pipeline.md) dropped Linux release jobs.

## Decision

Reuse selected Zorite mainline changes through upstream head
`4aad847f57a5ac3e8ec0b6220e25d9db7e5264b4`:

- Correct Linux shaped glyph positions by applying combining-mark offsets, in a
  temporarily vendored `gpui-pre-wgpu` source (license kept, excluded from
  workspace checks) until an adopted GPUI release includes the fix.
- Make the repacked AppImage launcher executable by every user.
- Use the standard golden-ratio constant for heading/list markers.
- Render existing `==highlights==` and supported coloured `<mark>`/`<span>`
  consistently in the editor and renderer. No formatting action, swatch menu or
  picker.

Port only relevant patches with upstream attribution. Journal, property,
template and import/export features removed from mdoc stay out.

## Invariants

Preserve caret/selection, undo, source offsets, search geometry, diagnostic
remapping and pseudonymization protections. Syntax recognition keeps literal
comparisons, escaping, code and tables. Unsupported styling keeps its source.
Rendering never normalizes, strips or translates formatting on save or Copy
Markdown.

References: [full text before ADR 0052](https://github.com/lawkitt/mdoc/blob/5cad017e5a38efe1f009d47f54d1f0d9e2995547/docs/adr/0009-selected-zorite-upstream-fixes.md),
[inventory](https://github.com/lawkitt/mdoc/blob/dd22e6e98b6635f1e51ced6baf6c47428dd50806/docs/design/zorite-upstream-triage.md),
[verification](../../tests/fixtures/zorite-upstream/README.md).
