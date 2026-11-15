# scripts

Every script starts with a header: usage, what it does, and what it needs.
Run them from the repository root; most have a `just` recipe.

| Folder | What lives there |
|---|---|
| top | `lib.sh` (shared helpers, sourced), `tools.env` (pinned tool versions), `setup.sh` (every CI job's first step) and `queue-runner.sh` (the PC's merge-queue runner, whose installed copies update themselves from these paths) |
| `ci/` | The merge gate, the area table (`ci-areas.txt`), the queue runner's setup, the detection gate, the quarantine list, CI stats and nightly issues, and their suites |
| `release/` | `just release`, version stamping and checks, Apple signing credentials for CI |
| `desktop/` | Packaging, signing, the installer's runtime, smoke tests, the icon, performance runs |
| `ios/` | The iOS core build, the iOS CI job and its shards, archives and TestFlight, device tests and surveys |
| `android/` | The Android core build, SDK and NDK installs, the Android CI job, the emulator, device tests and surveys |
| `dev/` | `scripts/dev/test.sh` (`just test`), lint helpers, tool installs, `doctor`, disk and worktree upkeep |

A script sources the helpers with `source "$(dirname "$0")/../lib.sh"`
(`# shellcheck source=../lib.sh`). A suite that copies scripts into a scratch
repository keeps this layout there.

Tool pins stay in `tools.env`, `rust-toolchain.toml` and the install
scripts. mise was weighed and declined
([#892](https://github.com/lawkitt/mdoc/issues/892)): it covers only 6 of
the 10 pins, and CI's `taiki-e/install-action` installs are already
prebuilt and checksummed.
