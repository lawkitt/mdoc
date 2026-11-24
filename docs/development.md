# Development

## Build from source

All dependencies, including the pinned AnyDoc converter, are public:
`git clone https://github.com/lawkitt/mdoc.git`, `cd mdoc`, then `cargo run`
(or `cargo run -- path/to/document.pdf`). Development commands are
[just](https://just.systems) recipes, and CI runs the same ones; `just
--list` describes each. `just setup` installs the pinned tools and `just
doctor` checks them.

**The repository gate.** Before a PR is ready, run `just lint` and the tests
of what it changes: `cargo nextest run -p <crate>`, the app's with
`-p mdoc --features headless-renders`, a script's suite with
`bash scripts/<area>/test-<suite>.sh`, and `just test` for a change to
`Cargo.lock`, the toolchain or `.config/nextest.toml`. How the PR then
merges is in [specs/ci.md](specs/ci.md). A slow test is a bug to fix, not a
limit to raise; tests never retry.

## Docs

The code is the record of what mdoc does. Read this file and your area's
spec: [desktop](specs/desktop.md), [mobile](specs/mobile.md),
[originals](specs/originals.md), [recognition](specs/recognition.md),
[cases and privacy](specs/cases-and-privacy.md), [CI](specs/ci.md) or
[release](specs/release.md). [ROADMAP.md](../ROADMAP.md) holds open work,
[Issues](https://github.com/lawkitt/mdoc/issues) bugs,
[CONTEXT.md](../CONTEXT.md) terms, and [adr/](adr/) the few choices that are
hard to reverse. [privacy.md](privacy.md) is for users.

### Writing docs

- **A doc line says what the code can't**: intent, a promise to users, a
  rejected option, or why code that looks wrong is right. A line that
  restates code, tests, lint, config or `just --list` is a *cache*: it goes
  stale, so delete it. History lives in git, PRs and issues.
- **Specs: test or delete.** One rule per bullet, with at most one "because"
  clause, paths only where an agent must go, and no narration. A rule about
  privacy, data loss or the case key that a test can pin becomes that test
  and leaves the spec; a behaviour the code already shows leaves.
- **ADRs** record a choice that is hard to reverse, surprising without
  context and the result of a real trade-off. Template: title;
  `Status: Accepted, YYYY-MM-DD.`; Why; Rejected; Revisit when (one
  paragraph is enough when the last two add nothing). A change rewrites the
  ADR in place; numbering follows [adr/README.md](adr/README.md).
- **Comments** give a non-obvious why in their own words. Decision numbers
  and the story of how code got here belong in git.
- **CONTEXT.md** keeps a term only if it fixes a word users see, forbids a
  synonym, or names a domain idea with no type in the code.
- A grill's record goes in its PR or an issue; its output is the spec or
  code change. Documentation of one script or module lives beside it, in its
  header or a folder README. Agent guidance lives here: no `AGENTS.md` or
  `CLAUDE.md`.
- Survey or before-and-after PNGs go in the PR. Evidence stays only while a
  rerun script reads it; acceptance results go in the release's issue.
- A PR that trims rules lists each one it dropped, with a reason, for the
  cross-provider reviewer and the maintainer.
- `just lint` runs `scripts/dev/docs-lint.sh`: word ceilings (its header
  lists them), no design or dated evidence folders, ADR numbers and
  changelog fragments. The maintainer runs the `codebase-doctor` skill to
  clear sediment the lint can't see.

### Roadmap upkeep

- Bugs are GitHub issues, grouped by milestone and closed by `fixes #n`.
  `ROADMAP.md` holds the plan: milestone scope, priority order, Done-when
  lines, Later, Deferred and Not planned. Features need no issue.
- A bug enters the patch milestone when it gives wrong output, loses data or
  makes an action fail silently, and needs no design decision.
- Between groomings, a new bug becomes an issue and a new idea goes to the
  end of Later; don't reorder.
- Item shape: title, `[Theme]` tag, a one-line source, 1–4 lines of what and
  why, `*Done when:*`; about 6 lines. Diagnosis goes in the issue, linked.
- Only the patch lane and the next minor carry full items; Later is one line
  each, written out when promoted.
- Should-fix items never hold a release; a minor is scope-boxed at about
  five must-fix items.
- Finishing an item: the same commit deletes it from `ROADMAP.md` and adds
  its CHANGELOG entry as `changes/<slug>.md` (`changes/ios/` or
  `changes/android/` for one app, `changes/mobile/` for both; format in
  [changes/README.md](../changes/README.md)).
- Groom after each release and acceptance run: close the finished milestone
  into Current status, triage issues, pick the next minor's must-fix items,
  promote from Later.

## Code layout

[ADR 0069](adr/0069-app-architecture-and-code-ownership.md) sets ownership.
Conversion and OCR algorithm changes belong in the dependency forks
([ADR 0001](adr/0001-conversion-and-ocr-in-dependency-forks.md)). Each
module's header says what it owns.

**Where code goes:**

- New state goes into an owner with private fields, never onto `Workspace`
  or `DocumentView` unless it is view state (focus handles, scroll, hover).
- A state machine is a plain struct whose transitions are synchronous,
  GPUI-free and return that owner's effect enum; the view carries effects
  out. No trait hierarchy or service container.
- Logic that needs no GPUI goes into a GUI-free crate (`mdoc-store`,
  `mdoc-import`, `mdoc-detect`, `mdoc-pii`, …), with typed errors.
- A document reaches the window only through `TabEvent`. Actions live with
  the feature that handles them. Boundary easements were declined
  ([#895](https://github.com/lawkitt/mdoc/issues/895)).
- A user-facing key binding is a row in `src/shortcuts.rs`; plumbing keys
  (Tab in a control group, moving in a menu, caret basics) stay with the
  feature.
- mdoc is an application: code the binary cannot reach is dead unless a
  named consumer exists. Crates are internal (`publish = false`).
- A refactor preserves behaviour; anything a user can observe needs its own
  decision. Judge it by clearer ownership, explicit effects and measured
  cost; file size alone never justifies a split.
- A test goes when it exercises removed code, duplicates another, asserts
  implementation detail or cannot fail. UI tests boot through the real tab
  shell with production dependencies and deterministic fixtures, never real
  downloads, and cover only what the real UI shows.
- A closed tab or stale completion never changes another tab or frees a
  running worker's slot.

**Dependencies.** `Cargo.toml` is the only place to change a revision:
`just lint` fails when `Cargo.lock` isn't what cargo resolves from it. An
exact pin carries its reason in a comment. Bump `dirs`, `aes-gcm` and
`docxide-pdf` deliberately, one PR each, checking their paths, old cases
and the preview corpus. If Cargo's Git fetch cannot authenticate, use
`CARGO_NET_GIT_FETCH_WITH_CLI=true`; never change global Cargo
configuration. Never distribute local user-provided fixtures.

**Native checks.** Native appearance, shortcuts, IME, accessibility, GPU
presentation and installers need a native run; headless tests do not
establish them.

**Tests and OS services.** A test never asks the OS's security or identity
services (the code signature, the Vault key, the build identity built on
them): tests open a Vault with an injected key and take the build label as
`Dev`. `mdoc-store`'s `deny-os-services` feature makes those calls
panic in every test build; a new call into such a service goes behind the
same guard.

**Shell scripts.** Recipes stay one command; anything longer is a Bash
script in [scripts/](../scripts/README.md) that sources `lib.sh`, no
`xtask`. Scripts run in Git Bash on Windows; one whose header says so stays
Bash 3.2 compatible for macOS's `/bin/bash`.

## Build cache

- A dependency gets its own dev `opt-level` (2) only when a profile shows it
  hot and the cold test build grows no more than about 30% on both macOS and
  Windows.
- Never override profiles or `RUSTFLAGS` through environment variables, or
  share a `CARGO_TARGET_DIR` between worktrees: both invalidate artifacts,
  and a shared one tested stale code.
- Cargo never deletes superseded artifacts: after a toolchain bump or a
  large dependency update, run `cargo clean --profile dev`; `just disk` and
  `just retire-worktree <path>` manage worktrees' `target/`.

## Fleet

- Other machines: the maintainer's `fleet` skill.
- `wsl` builds only the platform-neutral crates. A lane stops and reports
  before adding a dependency, vendoring code, patching `vendor/` or a GPUI
  platform crate, or editing outside its item.
- A dev build uses the real profile: copy `~/Library/Application Support/mdoc`
  aside before a GUI check and restore it after.
- An integration branch keeps `main`'s required checks. After every two or
  three items merged into it, one cross-provider review of the whole branch
  runs, limited to the privacy and data-loss rules. It never blocks a merge
  into the branch; its findings go to a running fix list
  ([#1153](https://github.com/lawkitt/mdoc/issues/1153)).
- A test that takes two unrelated PRs out of the merge queue is quarantined,
  with an issue; a hub does this without asking.

**Profile.** The fleet tools in the maintainer's config repository read
this block; change it with the rules above.

```toml fleet-profile
version = 1
base = "main"
roadmap = "ROADMAP.md"
read_first = [
  "`docs/development.md` (Fleet, Code layout, repository gate)",
  "`CONTEXT.md` (the terms the item uses)",
]
# A fast subset (Build from source): the app is `-p mdoc --features
# headless-renders`, a script's suite `bash scripts/<area>/test-<suite>.sh`, and a
# change to Cargo.lock, the toolchain or .config/nextest.toml runs `just test`.
gate = ["just lint", "cargo nextest run -p <each changed crate>"]
merge_queue = true
checks = ["lint (ubuntu)", "merge-gate"]
rules = [
  "Never run the app against the real profile (`~/Library/Application Support/mdoc`, `%LOCALAPPDATA%\\mdoc`) or touch the real Keychain key; tests use injected stores.",
  "Stop and report to the hub before adding a dependency, vendoring code, patching `vendor/` or a GPUI platform crate, or editing outside the item's area.",
]

[disk]
per_worktree_gib = 30
first_build_gib = 40
warm_gib = 15

[renders]
produce = "just headless-renders <new-directory>"
fetch = "dispatch `ci-desktop.yml` on the branch with `-f os=macos -f screens=<globs>` and `gh run download <run> --name renders-macos-screens`"
done_when = "headless renders of {what}, light and dark (the hosted desktop job uploads them)"

[bookkeeping]
roadmap_item = "remove item {number} from `{roadmap}` {milestone} (delete only its own lines; do not renumber)"
issue = "the commit that finishes it says `fixes #{number}`"
fragment = "add `changes/{slug}.md`, or `changes/ios/{slug}.md` / `changes/android/{slug}.md` for one app, `changes/mobile/{slug}.md` for both (format in `changes/README.md`)"

[ci]
scarce = { macos = { slots = 5, labels = ["macos-26", "xcode-27"] } }
shadow = "they pass a shadow run on `ci-shadow/main/<name>` before the hub enqueues them (`docs/specs/ci.md`, Changing CI). A change that adds or alters a red path also needs a deliberately broken twin that must fail. Delete both branches afterwards."
targets = "`scripts/ci/ci-stats.sh` (the nightly's CI targets): a core change's queue median 30 minutes or less; missing it reopens a self-hosted Mac (#1041)."
notes = [
  "`docs/specs/ci.md` has a 500-word ceiling (docs-lint; count with `LC_ALL=en_US.UTF-8`): make room by tightening lines that restate the code, never by raising it.",
  "On the Mac, reproduce a hosted iOS failure with Xcode 27.2 (ADR 0124): a queue or branch failure on the iOS 26.2 Simulator runtime (the default), a nightly one with `MDOC_XCODE=builds` (iOS 27.2).",
]

[machines.macbook-air]
setup = "Never set `CARGO_TARGET_DIR`: this worktree builds into its own `target/` (a shared one gives stale builds). First build is cold."
builds = ["rust", "macos", "ios", "android"]

[machines.windows]
disk_check = "`Get-PSDrive E` (not `just disk`, which fails while another lane builds)"
lanes = { windows-lane-1 = 'E:\t3\worktrees\mdoc\windows-lane-1', windows-lane-2 = 'E:\t3\worktrees\mdoc\windows-lane-2', windows-lane-3 = 'E:\t3\worktrees\mdoc\windows-lane-3', windows-lane-4 = 'E:\t3\worktrees\mdoc\windows-lane-4' }
notes = "Every lane, the root checkout and the queue runners' homes (`E:\\ci`) are on `E:`, a ReFS Dev Drive (a VHDX on `D:`); `CARGO_HOME` and `RUSTUP_HOME` stay on `C:`."
builds = ["rust", "windows"]

[machines.wsl]
disk_check = "`df -h ~`"
lanes = { wsl-lane-1 = "~/.t3/worktrees/mdoc/wsl-lane-1" }
gate = ["cargo test -p mdoc-pii -p mdoc-editor -p mdoc-history"]
notes = "Only the platform-neutral crates build here; a Linux build of the app is not a shipped target."
builds = ["rust-neutral"]
```
