# Roadmap upkeep and scope after 0.1.0

Status: Accepted, 2026-10-20. Amends [ADR 0051](0051-roadmap-structure-and-stable-scope.md)
(structure); [ROADMAP.md](../../ROADMAP.md) applies it.

## Problem

0.1.0 shipped on 2026-10-19. The roadmap still led with a finished milestone;
its "After 0.1.0" list mixed bugs users now hit with features and design work;
one item held ~18 unrelated points behind an uncheckable finish line; and the
file changed almost daily (56 commits in a month) because every item, however
far off, carried full detail and finished items stayed with "Done" notes.
GitHub Issues were enabled but unused, though the repository is public and
0.1.0 is a stable release.

## Decision

The process suits one maintainer working with coding agents: the plan stays
one file an agent reads whole, and bug intake and closure use GitHub's own
mechanics.

- **Issues take in bugs; ROADMAP.md holds the plan.** User reports and patch
  items are GitHub issues, grouped by a GitHub milestone and closed by
  `fixes #n`. ROADMAP.md keeps milestone scope, priority order, Done-when
  lines, Later, Deferred and Not planned, and links an item's issue when one
  exists. Features and design work need no issue. No Projects board.
- **Sections:** Current status, *0.1.x patches* (the patch lane), *0.2.0*
  (must-fix, should-fix), *Later*, Ongoing tracks, Engineering, Deferred, Not
  planned. ADR 0051's milestone tiers and "should-fix never holds the release"
  still apply.
- **Patch rule:** an item enters the patch lane when it gives wrong output,
  loses data or makes an action fail silently, *and* needs no design decision
  or ADR. A patch item is done when its issue closes.
- **Item template:** title, theme tag, one-line source note, 1–4 lines of what
  and why, a Done-when line; about 6 lines at most. Root causes, repro steps
  and measurements live in the issue, evidence folder or ADR, linked.
- **Detail only for the next milestone.** The patch lane and the next minor
  carry full items; everything beyond is Later, one line each in rough order,
  promoted and written out at grooming.
- **Done means deleted.** The commit that finishes an item removes it from
  ROADMAP.md and adds the CHANGELOG entry.
- **Grooming happens after each release and each acceptance run.** Between
  groomings, new bugs go to Issues and new ideas to the end of Later, without
  reordering.
- **Minors are scope-boxed** at about five must-fix items. A patch ships when
  its lane holds something worth releasing; 0.1.1 ships when T6 is checked on
  the installed 0.1.0 and the 0-document case and Windows single-instance bugs
  are fixed.
- **Acceptance per release:** a patch gets a *smoke run* on both platforms
  (install, launch, open a document, Copy for AI) plus the rows its fixes
  touch, with no RC. A minor gets an RC and the full ADR 0057 checklist,
  including rows the previous run deferred (S1, A5, D3 and D4 for 0.2.0).
- **0.2.0 order:** what users copy first, then editing, then performance. The
  stress tier moves to Later; editor text for screen readers is the first
  Later item to promote.

## Consequences

- ROADMAP.md lists only open work and changes mainly at grooming; history
  lives in the CHANGELOG, git, issues and evidence.
- Users and agents file bugs as issues; a fix commit closes them.
- Pre-grooming item detail is reachable by a permalink from Later.

References: [ROADMAP.md before this ADR](https://github.com/lawkitt/mdoc/blob/c400853/ROADMAP.md).
