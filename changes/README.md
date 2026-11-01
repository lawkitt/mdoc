# Changelog fragments

Each change adds one `changes/<slug>.md` here instead of editing
CHANGELOG.md ([ADR 0072](../docs/adr/0072-changelog-fragments.md)). The slug
is short and unique, in letters, digits, `.`, `_` and `-` (for example
`close-prompt` or `issue-171`). The file is the CHANGELOG entry as it will
read there, one bullet with links relative to the repository root:

```markdown
- **Sidebar tooltips follow file drops.** Inserting or reordering tabs
  dismisses a moved row's old tooltip (fixes #171).
```

`just release` moves every fragment to the top of `## Unreleased`, newest
merge first, and deletes it. This README is not a fragment.

A change to the iOS or Android app puts its fragment in `changes/ios/` or
`changes/android/` instead, in the same format and with the same
repository-root links; that app's release joins them into
`apps/ios/CHANGELOG.md` or `apps/android/CHANGELOG.md`, and the desktop
release leaves them alone ([ADR 0091](../docs/adr/0091-mobile-ci-and-releases.md)).
A core change users notice on several platforms gets a fragment in each
folder it affects.
