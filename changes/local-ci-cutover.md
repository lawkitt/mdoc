- **Local CI is the PR test gate.** Hosted CI runs only `lint (ubuntu)` on
  PRs from branches in the repository; `local/windows` and `local/macos`
  test them on the maintainer's machines. Fork and Dependabot PRs keep the
  full hosted matrix, and `ci.yml` can be run by hand on any branch
  ([ADR 0082](docs/adr/0082-local-ci.md); fixes #252).
