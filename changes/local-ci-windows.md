- **Local CI on the Windows PC.** The `local/windows` runner is installed and
  posts statuses. `scripts/local-ci-setup.ps1` now sends the task's output to
  `D:\ci\state\task.log`, because writes to the task's console failed and
  stopped the runner at start-up. It serves results through a localhost
  Python server (the `mdoc local CI web` task) that `tailscale serve`
  proxies, because serving a folder needs a Windows admin. A third lane,
  `windows-lane-3`, is on `D:` (fixes #233,
  [ADR 0082](docs/adr/0082-local-ci.md)).
