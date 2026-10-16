# Local crash reports the user sends

Status: Accepted, 2026-10-14.

## Problem

When mdoc crashed, nothing was recorded: the user could not tell us what
happened and we could not reproduce it. Anoni's "See what would be sent" fits an
app whose users handle confidential documents.

## Decision

1. **A panic hook writes a local report** (`src/crash.rs`, app mode only, not
   the DOCX worker or self-test) to `mdoc/crash-report.txt` in app data. Only the
   latest report is kept. The default hook then runs, so the process ends as
   before. mdoc keeps no log file.
2. **Contents**: version, OS and architecture, thread, panic location, the first
   mdoc frame (innermost workspace frame, as function and workspace-relative
   line), the backtrace, a warning when the program file changed after launch
   (backtraces would be scrambled), and a window-state line in words and numbers
   (Home or document kind, size in bytes, zoom, panel tab, proposed and applied
   counts, popup, before/after, find, tabs, sidebar, theme).
3. **No document content, ever.** No text, file names or clipboard. Absolute
   paths lose their folders (`<path>/main.rs:12`, `~/…/main.rs`); only Rust
   source file names stay, others become `<file>`. The panic message is never
   kept: known standard-library panics (unwrap on `None`/`Err`, assertion, index
   out of bounds, byte index, overflow) get a fixed label. String-slicing panics
   are parsed strictly and kept only as numbers (index, the character's byte
   range, size and script, the string's length); anything that does not parse
   exactly is withheld. Masking is best effort; the preview guarantees the user
   sees what would leave the Mac.
4. **Next launch** shows "mdoc had a problem last time it ran." with **See what
   would be sent** and **Dismiss** (deletes the report). The preview shows the
   exact masked text, with **Send…** (a prefilled GitHub issue, cut to about
   6,000 characters) and **Copy**; either deletes the report. Nothing is sent
   automatically; there is no server endpoint.

## Consequences

The only new network path is the user's browser opening a GitHub page they
chose. Crashes without a panic (aborts, signals) are not recorded; that needs
platform crash handlers.

References: [full text before ADR 0052](https://github.com/lawkitt/mdoc/blob/5cad017e5a38efe1f009d47f54d1f0d9e2995547/docs/adr/0047-crash-reports.md),
[interview](https://github.com/lawkitt/mdoc/blob/5cad017e5a38efe1f009d47f54d1f0d9e2995547/docs/design/anoni-comparison.md), decision 10.
