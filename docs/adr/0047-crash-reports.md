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
