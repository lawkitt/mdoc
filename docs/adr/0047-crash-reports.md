# Local crash reports the user sends

Status: Accepted, 2026-10-14. Interview record:
[Anoni comparison](../design/anoni-comparison.md), decision 10.

## Problem

When mdoc crashes, nothing is recorded: the user cannot tell us what happened
and we cannot reproduce it. Anoni offers "See what would be sent" before
sending a crash report, which fits an app whose users handle confidential
documents.

## Decision

1. **A panic hook writes a local report** to `mdoc/crash-report.txt` in app
   data: version, OS and architecture, the panic message and location, and a
   backtrace. Only the most recent report is kept. The hook then runs the
   default hook, so the process still ends as before.
2. **No document content, ever.** The report holds no document text, file
   names or clipboard contents. Before display, the user's home folder is
   replaced by `~`, and any other absolute path by `<path>`. Panic messages
   are masked the same way.
3. **Next launch** shows a bar: "mdoc quit unexpectedly last time." with
   **See what would be sent** and **Dismiss**. The preview is the exact,
   already-masked text. Dismiss deletes the report.
4. **Send** opens a prefilled GitHub issue in the browser
   (`github.com/lawkitt/mdoc/issues/new` with the report in the body, cut to
   fit a URL). **Copy** puts the same text on the clipboard. The report is
   deleted after either. Nothing is sent automatically and there is no server
   endpoint.

## Consequences

The only new network path is the user's browser opening a GitHub page they
chose to open. Crashes that kill the process without a panic (aborts, signals)
are not recorded; that needs platform crash handlers and can come later.

## As implemented

- `src/crash.rs`; the hook is installed in `main` for the app only, not for the
  DOCX worker or self-test modes.
- **No log lines:** mdoc keeps no log file, so the report is version, OS and
  architecture, thread, panic message and location, and the backtrace.
- **Masking.** Absolute paths lose their folders (`<path>/main.rs:12`,
  `~/…/main.rs` under the home folder), since folder names can name clients;
  only Rust source file names are kept, any other becomes `<file>`.
  The panic message itself is never kept. A known standard-library panic
  (unwrap on `None`/`Err`, assertion, index out of bounds, byte index,
  arithmetic overflow) gets a fixed label; any other message is withheld,
  since `assert_eq!` output or `panic!("{}", text)` can carry document text in
  any form, even text that starts like a standard message
  (Codex review). Location and backtrace still place the crash.
  Masking is best effort; the preview is what guarantees the user sees what
  would leave the Mac.
- **Wording:** "mdoc had a problem last time it ran.", because a panic on a
  background thread does not always end the process.
- The bar offers **See what would be sent** and **Dismiss**; the preview adds
  **Send…** (GitHub issue, report cut to about 6,000 characters) and **Copy**.
