# Changelog fragments

Status: Accepted, 2026-10-28.

## Problem

Every change added its entry at the top of `## Unreleased` in CHANGELOG.md.
PRs open at the same time conflicted on those lines, and each merge forced
the next PR to rebase for a text-only conflict.

## Decision

- **One fragment per change.** The commit that finishes a change adds
  `changes/<slug>.md` instead of editing CHANGELOG.md; an iOS or Android app
  change adds `changes/ios/<slug>.md` or `changes/android/<slug>.md`. The
  slug is short and unique (the branch or the issue). The file holds the
  entry as it will read: one `- **Title.** text` bullet, links relative to
  the repository root. `changes/README.md` describes the format and is never
  a fragment.
- **`just release` joins the top-level fragments.** Every desktop release,
  pre-releases included, puts them at the top of `## Unreleased`, newest
  merge first, deletes them and commits the result in `release: v…`. Merge
  order is the order in which commits on `main`'s first-parent history added
  each file; a fragment no such commit added goes last, by name.
- **App fragments join their app's changelog** (`apps/ios/CHANGELOG.md`,
  `apps/android/CHANGELOG.md`) at that app's release, with links rewritten
  for the app's folder. A core change users see on several platforms gets a
  fragment in each folder.
- **A stable release needs an entry:** a fragment or text already under
  Unreleased.
- **Hand edits stay allowed** for fixing a released entry or rewording
  Unreleased.

## Consequences

- Parallel PRs touch different files and no longer conflict on the
  CHANGELOG.
- Between releases, Unreleased lives in `changes/`.
- `scripts/release/test-release.sh` covers the join order, deletion and the
  stable entry check.
