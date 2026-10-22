# Patch smoke run, 0.1.1 (2026-10-22)

The smoke run for the 0.1.1 patch ([ADR 0059](../../../adr/0059-roadmap-after-0-1-0.md)):
install, launch, open a document and Copy for AI on both platforms, plus the
rows the patch's fixes touch. Rows use the IDs of the
[checklist](../../../platform-acceptance.md).

## Before the release

| ID | Target | Check | Result | Notes |
| --- | --- | --- | --- | --- |
| T6 | Windows x64 | Caption buttons draw, work and snap layouts appear on hover | Pass | The maintainer, on the installed 0.1.0 with a physical mouse, 2026-10-22: minimize, maximize/restore and close act on a click, hovering maximize shows snap layouts, and a title-bar double-click toggles maximize and restore. Closes [#74](https://github.com/lawkitt/mdoc/issues/74); the 0.1.0 run's Fail is in [windows.md](../2026-10-19/windows.md). |

## On the 0.1.1 build

Pending: install, launch, open a document and Copy for AI on Apple Silicon
macOS and Windows x64, plus the fixed flows: Save as case on an unsaved
conversion (#75), Save As and Recents (#76), formatting a selection that ends
in a space (#77), and shortcuts after Save as case and after a popup undo
(#79). On Windows, also check single-instance launch and Open with (#80).
