- **Local CI runner.** `just local-ci` tests each PR's merge with `main` on
  the maintainer's Mac and Windows PC and posts `local/macos` and
  `local/windows` statuses that link to the log and renders over the tailnet
  ([ADR 0082](docs/adr/0082-local-ci.md); fixes #233).
