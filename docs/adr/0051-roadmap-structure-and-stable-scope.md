# Roadmap structure and 0.1.0 stable scope

Status: Accepted, 2026-10-16. Amends [ADR 0024](0024-replace-custom-selection.md)
(repeats inside a longer mention). [ROADMAP.md](../../ROADMAP.md) applies it.

## Problem

ROADMAP.md kept three parallel lists (next steps, known issues from
0.1.0-beta.11, a UI backlog) with no priority, release horizon or finish lines;
engineering work sat among user-facing steps, and a long status recap repeated
the ADRs.

## Decision

- **By milestone**: *0.1.0 stable* (must-fix, should-fix), *After 0.1.0*,
  *Ongoing tracks*, *Engineering*, *Deferred*, *Not planned*. Items are in
  priority order, carry a theme tag and keep a one-line provenance note.
- **Every item has a short, checkable "Done when" line.**
- **Should-fix never holds the release.** Stable waits only on must-fix items;
  unfinished should-fix items move to the top of After 0.1.0.
- **Stable scope**: must-fix covers PII linking defects, responsiveness for
  ordinary legal documents, captured AI-reply fixtures, and native acceptance on
  Apple Silicon macOS and Windows x64 (Linux stays buildable), done last because
  it checks the finished UI. Outgoing AI-analysis qualification is an ongoing
  track, not a release blocker (ADR 0034).
- **Ordinary legal document**: about 30 pages, or about 135 KB of Markdown.
  Responsiveness is judged on a release build on the M4 reference Mac (p95 frame
  ≤ 16.7 ms); debug `just perf` ceilings stay regression guards.
- **Variants of a known identity** (title + surname, possessives, surname alone,
  RU inflections) are proposed for review, never linked automatically; a first
  name alone is not proposed. Implementing this amends ADR 0018 in a new ADR.
- The status recap stays a few lines with links.

References: [full text before ADR 0052](https://github.com/lawkitt/mdoc/blob/f368e436cd2d3e70789d51879b9f54f379900e1e/docs/adr/0051-roadmap-structure-and-stable-scope.md).
