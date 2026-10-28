# Changelog fragments

Status: Accepted, 2026-10-28. Amends [ADR 0059](0059-roadmap-after-0-1-0.md)
("Done means deleted": where the CHANGELOG entry goes).

## Problem

Every change added its entry at the top of `## Unreleased` in CHANGELOG.md.
PRs that are open at the same time, like the 0.2.1 fixes (#184–#191),
conflict on those same lines, and each merge forces the next PR to rebase
for a text-only conflict.

## Decision

- **One fragment per change.** The commit that finishes a change adds
  `changes/<slug>.md` instead of editing CHANGELOG.md. The slug is short and
  unique (the branch name or the issue, such as `close-prompt` or
  `issue-171`). The file holds that change's CHANGELOG entry as it will
  read there: one `- **Title.** text` bullet, links relative to the
  repository root. `changes/README.md` describes the format and is never a
  fragment.
- **`just release` joins them.** Every release, pre-releases included, puts
  the fragments at the top of `## Unreleased`, newest merge first like the
  hand-written sections, separated by blank lines, deletes them, and commits
  the result in `release: v…`. A stable release then names the section as
  before ([ADR 0037](0037-ci-and-release-pipeline.md)). Merge order is the
  order in which commits on `main`'s first-parent history added each file; a
  fragment no such commit added goes below the others, by name.
- **A stable release needs an entry:** a fragment or text already under
  Unreleased.
- **Hand edits stay allowed.** Fixing an entry after its release, or
  rewording Unreleased before one, edits CHANGELOG.md directly.

## Consequences

- Parallel PRs touch different files and no longer conflict on the
  CHANGELOG.
- Between releases, Unreleased lives in `changes/`; CHANGELOG.md shows only
  what the last release joined.
- `scripts/test-release.sh` covers the join order, deletion and the stable
  entry check.
