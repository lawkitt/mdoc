# Handoff: run the 2026-10-22 UX review

A brief for a fresh Claude Code session in `/Users/tebriz/Developer/lawkitt/mdoc`.
The plan was settled with the user in a grilling session; do not re-grill, start
the run. Start the new session with:

> Read docs/evidence/ux-review/2026-10-22/HANDOFF.md and README.md, then run the review.

## Goal

Explore the installed **mdoc 0.1.1** (`/Applications/mdoc.app`) and **Anoni
0.86.0** (`/Applications/Anoni.app`) with computer use, for mdoc bugs, UX
improvements and feature ideas, small and big. The last Anoni comparison was of
0.81.1 ([ADR 0046](../../../adr/0046-workspace-polish-from-anoni-review.md));
look closely at what Anoni added since.

The agreed ground rules are in [README.md](README.md) and are binding. Summary:

- **Routing ([ADR 0059](../../../adr/0059-roadmap-after-0-1-0.md),
  [Roadmap upkeep](../../../development.md#roadmap-upkeep)):** bugs become GitHub
  issues, filed directly with `gh` (pre-approved). Patch-rule bugs (wrong
  output, data loss or silent failure, *and* no design decision) get label
  `bug` + milestone `0.1.2`; other bugs get `bug`, no milestone. Ideas get no
  issue: one line each at the end of ROADMAP.md *Later*, no reordering. Only
  findings serious enough for 0.2.0 get a full item (title, `[Theme]` tag,
  source line "*From the 2026-10-22 UX review.*", 1–4 lines, `*Done when:*`).
  Duplicates of an open issue (#78) or of a roadmap point (e.g. 0.2.0
  should-fix *Small polish*) get no new issue; add a comment only with new facts.
- **Issues stand alone:** fixture repro steps, expected vs actual, platform and
  version. No links to this folder until it is on `main`.
- **Anoni ideas that conflict with mdoc's Not planned list** go only into the
  README, under their own heading.
- **Delivery:** one local commit on branch `ux-review-2026-10-22`, no push, no
  PR, no new ADRs (write "ADR first" on items that need a decision).

## Data

Synthetic fixtures (screenshots of these may be committed):

- `tests/fixtures/import/`: text/tables/nested/checkboxes DOCX, `text.pdf`,
  `handmade-scanned.pdf`, `handmade-partly-scanned.pdf`, `handmade-mixed.pdf`,
  `.doc`, `.xls(x)`, and the broken `encrypted--errors.odt`,
  `truncated--errors.docx`.
- `tests/fixtures/docx-preview/`: `legal-opinion.docx`, `comments.docx`, `coverage.docx`.
- `tests/fixtures/pseudonymization/`: `inline-review.md`, `hybrid/`.
- `tests/fixtures/restore/`: ChatGPT/Claude EN/RU replies for AI-reply restoration.

Real files from `~/Downloads`, and **only these four** (names given to the
session in chat, not recorded here): a 14-page and a 37-page legal opinion
(DOCX), a Russian court PDF and a law (DOCX).

Never open lab results, bills, books or other personal files. **Never commit or
put in an issue a screenshot of a real file**; keep those in the session
scratchpad, describe the finding in words, and reproduce it with a fixture
where possible. In Anoni, open only these files and fixtures; do not open or
capture its existing history or projects.

## Side effects

- No model downloads over ~200 MB.
- Save/export only to the scratchpad.
- Cases only as `zz-ux-review-*`; delete them at the end.
- Leave existing cases and settings in both apps as found (restore anything changed).

## Tour (both apps, ~1–2 hours)

Home and first run; opening/dropping files and conversion; DOCX/PDF preview;
editor (typing, formatting, Enter/Shift+Enter, undo, find, long documents);
PII detection and review (popup, scopes, Replacements panel, categories);
cases and AI-reply restoration; Copy for AI / export; OCR on scanned PDFs;
Settings and models; keyboard-only use and macOS shortcuts; dark/light; narrow
and wide windows; errors on broken files. Compare each flow with Anoni and note
what Anoni does better, worse or not at all.

## Output

1. `docs/evidence/ux-review/2026-10-22/README.md`: add *Findings* (mdoc bugs,
   mdoc UX, Anoni ideas worth taking, Not planned conflicts), each with the
   issue number or roadmap line it became; fixture screenshots in `mdoc/` and
   `anoni/` subfolders, referenced from the README.
2. GitHub issues on `lawkitt/mdoc` per the routing above.
3. ROADMAP.md: Later one-liners appended; full items only for 0.2.0-worthy findings.
4. Commit on `ux-review-2026-10-22`; report what was filed and changed, with links.

Notes: computer use needs `request_access` for mdoc, Anoni and Finder (file
dialogs); load all computer-use tools in one ToolSearch (`computer-use`, 30
results). Agent guidance lives in `docs/development.md`; there is no AGENTS.md
or CLAUDE.md by design.
