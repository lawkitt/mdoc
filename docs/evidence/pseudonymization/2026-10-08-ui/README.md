# Replacements UI verification — 2026-10-08

Scope: implement [ADR 0019](../../../adr/0019-simpler-pseudonymization-ui.md).
This record covers the UI simplification on top of the existing identity work;
it does not qualify detector recall or anonymity.

## Automated checks

- `cargo test --locked --workspace`: **435 passed, 16 ignored**, 12 suites.
- `cargo clippy --locked --workspace --all-targets -- -D warnings`: passed.
- `cargo build --locked`: passed.
- `cargo fmt --all -- --check` and `git diff --check`: passed.

Cargo reports existing environment/dependency notices: the configured registry
minimum publish age requires an unstable Cargo flag, and `block v0.1.6` has a
future Rust incompatibility notice. Strict Clippy reports no code diagnostics.

Interactive GPUI tests exercise the actual controls in both themes at wide and
640×480 viewports, including editable alias drafts, disabled Apply until Save,
reachable application controls, long EN/RU transitions, contextual owner picking,
search/Back, highlight-to-item synchronization and focus return. Existing long
candidate-list and dense hidden-field keyboard tests also pass.

State tests cover one-step batch undo/redo; rename, link, split, owner and category
corrections; identity Keep with linked variants and detached homonyms; single
mention Keep; restore-one and matching-original restore; exact raw clipboard
output before/after Apply; shared-marker upgrades without changing pasted tokens;
revision invalidation; stale/cancelled/failed scan rejection; model setup state;
and preservation of the automatic Anonymize flow.

## Native macOS interaction

A temporary QA app bundle ran a local debug build with the synthetic fixture from
`../2026-10-08-identities/synthetic-input.md`. No source document was overwritten.
Observed interactions:

- A review scan proposed eight mentions without applying them.
- Selecting an identity showed its original, editable alias and mentions.
- Typing `CLIENT_1` into the real alias field and choosing Save alias staged the
  proposal without editing source. Selection remained readable in both themes.
- Apply changed all eight tracked mentions. Closing the panel retained clickable
  highlights. Restore this restored only the selected original; its other alias
  occurrence remained applied.
- The docked panel fit at a wide viewport. At 640×480 it became an overlay drawer
  with a reachable footer in both themes. The item view remained scrollable and
  Escape dismissed the drawer, leaving the source visible.

Native screenshot artifacts are in
`/Users/tebriz/.codex/visualizations/2026/10/08/01a11be9-383a-7f91-9443-f933c9eb60a4/mdoc-replacements/`:
`light-item.png`, `light-drawer.png`, `dark-drawer.png`, `dark-item-drawer.png`,
and `dark-docked.png`. Captures precede the final mention-count wording and
shared-string/shared-marker refinements; final rendered geometry and upgrade
behavior are covered by the automated checks above.

The temporary QA process was stopped. Original application session and settings
were restored byte-for-byte and verified. Native input probes with incorrect
coordinates were discarded; accepted checks used a fresh synthetic fixture.

## Verification boundaries

The ignored checks include existing opt-in model/platform/performance exercises;
they were not enabled by this UI change. Native observations complement GPUI
tests and do not establish exhaustive IME or assistive-technology behavior.
Windows/Linux native interaction, release packaging and remote CI were not run.
No new detector quality or production-readiness claim follows from these results.
