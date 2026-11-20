# Changelog fragments

Each change adds one `changes/<slug>.md` here instead of editing
CHANGELOG.md, because PRs open at the same time conflicted on its top lines
and each merge forced the next PR to rebase. The slug
is short and unique, in letters, digits, `.`, `_` and `-` (for example
`close-prompt` or `issue-171`). The file is the CHANGELOG entry as it will
read there, one bullet with links relative to the repository root:

```markdown
- **Sidebar tooltips follow file drops.** Inserting or reordering tabs
  dismisses a moved row's old tooltip (fixes #171).
```

`just release` moves every fragment to the top of `## Unreleased`, newest
merge first, and deletes it. `just lint` (docs-lint) rejects a name or
first line it would reject. This README is not a fragment. Hand edits to
CHANGELOG.md stay allowed for fixing a released entry or rewording
Unreleased.

A change to the iOS or Android app puts its fragment in `changes/ios/` or
`changes/android/` instead, in the same format and with the same
repository-root links; that app's release joins them into
`apps/ios/CHANGELOG.md` or `apps/android/CHANGELOG.md`, and the desktop
release leaves them alone ([releases](../docs/specs/release.md)).
A core change users notice on several platforms gets a fragment in each
folder it affects.
