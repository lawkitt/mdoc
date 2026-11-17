# Docs are area specs; ADRs are rare

Status: Accepted, 2026-11-15. Supersedes
[ADR 0052](https://github.com/lawkitt/mdoc/blob/0aaf8f706e754d8b4c9ad905305b7bac194218a1/docs/adr/0052-adrs-are-current-rules.md).

## Problem

`docs/` held about 154k words: 117 ADRs, 39 design records, and guides of
2–12k words each. ADR 0052's cleanup didn't last: design records came back,
each decision edited several older ADRs, iOS ADRs turned into fix logs, the
same facts sat in ADRs, guides and code, and the guides were merge-conflict
hotspots at 300–400 PRs a week.

## Decision

- **Agents first.** A doc stays only if an agent would get something wrong
  without it and code, tests or lint don't already say so. History lives in
  git and PRs.
- **Living area specs are the source of truth**:
  `docs/specs/{desktop,mobile,originals,recognition,cases-and-privacy,ci,release}.md`.
  A change edits one place. Specs have one rule per bullet, no ADR numbers
  and a `Folds:` line naming the ADRs they absorbed.
- **ADRs are rare**: only choices whose change needs a migration or a
  rewrite, or that pick a dependency or platform. Nineteen are kept, each
  under 400 words with no amendment chain; no new ADR for product
  behaviour.
- **ADR numbers stay resolvable**: `docs/adr/README.md`, generated from the
  `Folds:` lines, maps each folded number to its spec and a permalink to its
  last full text. New code doesn't cite ADRs.
- **No design records in the tree**; a grill's record goes in its PR or an
  issue.
- **Colocation**: a script's or module's documentation lives beside it.
- **Acceptance** keeps 10 desktop rows and at most 8 iOS device rows in
  `release.md`; results go in a GitHub issue per release.
- **Fleet orchestration** lives in the maintainer's `fleet` skill; the
  repository keeps the `fleet-profile` block and a few lines of needs.
- **Budgets**, checked by `docs-lint` in `just lint`: `development.md`
  2,500 words, each spec 3,000, `ci.md` 800, each ADR 400, `CONTEXT.md`
  3,000, folder READMEs 1,500. The lint also fails on a `docs/design/`
  folder, a dated or `pr-*` evidence folder, and an ADR number that is
  neither a file nor claimed by exactly one `Folds:` line.
- **Fold safety**: a PR that folds rules lists each dropped rule with a
  reason; a cross-provider reviewer checks it and the maintainer reviews
  the list.

## Consequences

- An agent reads `development.md` and one spec, about 5k words instead of
  20k or more.
- ADR amendment chains end; old citations resolve through the index.
- An agent runs a doctor pass monthly.

References: maintainer grill, 2026-11-15.
