# Development notes

## Build from source

All dependencies, including the pinned AnyDoc converter, are public.

```sh
git clone https://github.com/lawkitt/mdoc.git
cd mdoc
cargo run
# Or open a document:
cargo run -- path/to/document.pdf
```

Development commands are [just](https://just.systems) recipes; CI runs the
same ones. `just setup` installs the pinned tools, `just doctor` checks them.

```sh
just check      # fmt, cargo-deny, clippy and tests, as CI runs them
just check-all  # plus the OCR and installed-model smoke tests (downloads)
```

The full list is under [Recipes](#recipes).

## Project documentation

- [Roadmap](../ROADMAP.md): status, next steps and deferred work.
- [Glossary](../CONTEXT.md): product and code terminology.
- [Architecture decisions](adr/): accepted behavior and its rationale.
- [Evidence](evidence/): measurements that current work still compares against.
- [Privacy and limitations](privacy.md).

### Writing ADRs

ADRs state the rules in force ([ADR 0052](adr/0052-adrs-are-current-rules.md));
history lives in git.

- Sections: Problem, Decision, Invariants (if any), Consequences, then an
  optional `References:` line for interview records and evidence.
- Status line: `Status: Accepted, YYYY-MM-DD. Amended by ADR X; superseded in
  part by ADR Y.` No conversation narration.
- A later decision edits the older ADR to show only what still holds, with a
  one-line pointer. An ADR with no surviving rule becomes a stub: title,
  `Superseded by ADR NNNN`, and a commit permalink to its full text.
- Never renumber: code cites ADRs by number.
- Design interviews and dated evidence leave the tree once nothing live reads
  them; link them by commit permalink.

## Code layout

Run the [repository gate](#build-from-source) before delivering code changes.

Keep build caches for fast warm rebuilds. For occasional cleanup from the
repository root, use `cargo clean --workspace` to remove workspace artifacts
while retaining dependency artifacts. Use `cargo clean` for a full reset;
the next build will be cold. Cleanup is manual; no automatic size limit applies.
See [Cargo clean](https://doc.rust-lang.org/cargo/commands/cargo-clean.html).

Keep the current app/reusable-crate layout. Extract ownership only for a concrete
problem; conversion and OCR algorithm changes belong in the dependency forks.

| Location | Responsibility |
| --- | --- |
| `src/main.rs` | Application startup, menus, key bindings and window creation |
| `src/workspace.rs` | The window shell: document sidebar, tab identity, admission, scheduling and session persistence |
| `src/document_view.rs`, `src/document_view/chrome.rs` | One tab's document view: editor, source preview, import/OCR state, actions and toolbar chrome |
| `src/document.rs`, `src/document_session.rs` | Atomic saves, external-change checks, accepted document identity and provenance |
| `src/session_store.rs` | Paths and view metadata restoration; no document text |
| `src/import.rs`, `src/import_session.rs`, `src/ocr.rs` | Library integration, pending jobs, explicit verified setup and offline OCR |
| `src/preview.rs`, `src/docx_preview.rs` | Preview ownership and supervised local DOCX worker |
| `src/docx_comments.rs`, `src/comment_panel.rs` | Read-only DOCX comments |
| `src/markdown_search.rs`, `src/search_session.rs` | Markdown find controls and revision-aware search scheduling |
| `src/pii.rs`, `src/pii/detector*`, `src/pii/ui*` | Pseudonymization: offline detection/setup, scan/edit controller, Replacements panel and word popup, UI regression tests |
| `src/images.rs`, `src/style.rs` | Document-relative local images and app styling |
| `src/document_view_tests.rs`, `src/workspace_tests.rs`, `src/perf_tests.rs` | Headless flows, ownership/lifetime checks, and opt-in performance measurements |
| `crates/mdoc-editor` | Host-agnostic WYSIWYG, rendered-text search, and Markdown recognition |
| `crates/mdoc-pii`, `crates/mdoc-history` | GUI-free pseudonymization review model and the plain edit transactions it follows |
| `crates/gpui-pdf`, `crates/gpui-bidi` | Virtualized PDF preview and bidirectional text layout |
| `src/updater.rs` | Detection-only GitHub Releases update check ([ADR 0038](adr/0038-update-check.md)) |
| `src/spelling.rs`, `crates/mdoc-spell` | Spellcheck: background checks, toolbar indicator and navigation; GUI-free EN/RU engine with bundled dictionaries ([ADR 0036](adr/0036-bundled-spellcheck.md)) |

Cargo.toml and Cargo.lock define dependency revisions; `src/ocr.rs` defines
runtime URLs/digests, and the pinned pdf-inspector manifest defines OCR model
artifacts. Avoid copying these into another configuration. Cargo.lock must be
what cargo resolves from Cargo.toml; `just lint` fails otherwise. Change a
dependency's requirement in Cargo.toml, never the lockfile alone
([ADR 0053](adr/0053-cargo-lock-matches-manifest.md)). AnyDoc supplies
non-PDF conversion; pdf-inspector handles PDFs directly.

For fork updates, test the fork first, review lockfile/transitive changes, then
run the [repository gate](#build-from-source) and compare outputs against
the source before changing fixture expectations. AnyDoc's PR 153 port is attributed in its fork; the fork
began as a snapshot with unrelated upstream history, so port reviewed upstream
changes rather than assuming a normal merge. Its transitive pdf-inspector 1.14.2
baseline is distinct from the app's direct OCR-capable fork. If Cargo's Git fetch
cannot authenticate, use `CARGO_NET_GIT_FETCH_WITH_CLI=true` with existing Git
credentials; do not change global Cargo configuration.

Use [import](../tests/fixtures/import/README.md),
[DOCX](../tests/fixtures/docx-preview/README.md), and
[OCR](../tests/fixtures/ocr-qualification/README.md) fixture notes for reproduction
and qualification limits. Native appearance, shortcuts, IME, GPU presentation,
and execution on other platforms require native checks; headless tests do not
establish them. Do not distribute local user-provided fixtures.

Measure runtime hotspots separately from the routine gate with `just perf`,
which runs every ignored performance test serially (set `MDOC_PERF_PREVIEW` to
a local PDF or DOCX to include preview timing). Run it on an idle machine.
`MDOC_PERF_PROFILE=release just perf` uses the release profile, serial builds
and unchanged ceilings. The runner attempts every check before returning failure.
The scroll workload defaults to the public `coverage.docx`; `MD_PERF_FILE` can
select another DOCX under `tests/fixtures/docx-preview`. The PDF check defaults
to public `reference.pdf` plus a generated 64-page PDF; `PDF_PERF_DIR` selects
an explicit local corpus.
Logs include actual editor widths; outer window dimensions alone are insufficient
for comparisons.
`review_host_performance` covers diagnostic density, proposed/applied review,
before/after views and three-member case synchronization without a detector download.
`MDOC_PERF_STRESS=1` makes that review matrix use 2 MiB / 20,000 mentions;
run it explicitly in release mode on an idle host for stress measurements.

Use a fixed viewport, representative legal/OCR Markdown, and actual scroll/search
positions; report profile, machine, latency, peak memory, and lifecycle results.
Historical tab measurements found roughly 292 ms long-document redraws in
debug/headless mode despite a 0.007 ms tab activation handler. Reproduce before
optimizing; those timings are not current results or native presentation latency.

## Recipes

Every development command is a `just` recipe, run by Git Bash on Windows
(install [Git for Windows](https://gitforwindows.org) and `just`). CI calls the
same recipes, and the tool versions in `scripts/tools.env` are shared by CI and
`just setup` ([ADR 0037](adr/0037-ci-and-release-pipeline.md)).

| Recipe | What it does |
| --- | --- |
| `just setup` / `just doctor` | Install / check the pinned toolchain and tools |
| `just check` | `fmt-check`, `deny`, `lint`, `test` — the CI gate |
| `just smoke-ocr` | Downloads the pinned OCR runtime and recognizes the EN/RU fixtures (release gate) |
| `just smoke-models [--download]` | Default OCR and pseudonymization models through setup and offline paths |
| `just check-all` | `check` + both smoke recipes |
| `just perf` | Ignored performance tests, serially |
| `just package` | This machine's installers into `dist/` |
| `just smoke-package` | OCR from the packaged artifact: the `.dmg`'s signed app, or the release `mdoc.exe` (release gate) |
| `just smoke-update [dmg]` | An older Developer ID signed `.dmg` updates itself to the latest release through `--mdoc-self-update` (manual, before releases that change the updater) |
| `just release <version>` | Release commit and tag (below) |
| `just icon`, `just actionlint` | Icon assets; workflow lint |
| `just disk` | Size of each worktree's `target/` |
| `just retire-worktree <path>` | Remove a finished worktree and its `target/` |

Heavy, download and machine-dependent tests run only through these recipes, on
the Apple Silicon Mac and the Windows x64 PC.

## Build cache and disk use

The dev and test profiles keep line tables for workspace crates and no debug
information for dependencies ([ADR 0050](adr/0050-rust-build-cache-growth.md)).
Backtraces (`RUST_BACKTRACE=1`) still show file and line, but a debugger cannot
inspect variables. For lldb or CodeLLDB, build with full debug information in a
separate `target/debugging/`, leaving the everyday cache untouched:

```sh
cargo run --profile debugging
cargo test --profile debugging -p mdoc-editor --no-run
```

Keep one build configuration: don't override profiles or `RUSTFLAGS` through
environment variables, and don't share a `CARGO_TARGET_DIR` between worktrees.
Either one invalidates artifacts and leaves duplicate copies behind.

Each worktree has its own `target/`, and nothing removes it automatically.
`just disk` reports their sizes. When a worktree's branch is merged or
abandoned, `just retire-worktree <path>` deletes its `target/` directories and
then the worktree, keeping the branch. It refuses a worktree with uncommitted,
untracked or other ignored files (such as `dist/` or `.qualification/`), which
`git worktree remove` would otherwise delete silently.

Cargo never deletes superseded artifacts. A toolchain bump, a large dependency
update or a changed flag builds a complete new copy beside the old one, so run
`cargo clean --profile dev` after one (the next build is cold, about two
minutes for `just test`). Edits alone grow the cache only slowly.

## Releasing

Releases ship only for Apple Silicon macOS (`.dmg`) and Windows x64 (NSIS
`.exe`; `.msi` for stable versions). The repository version is the source of
truth: `just release 0.1.0-beta.1` checks that `main` is clean and current, runs
the checks (optionally `check-all`), sets the version in `Cargo.toml`,
`Cargo.lock` and `Info.plist`, names the CHANGELOG section for stable versions,
commits `release: v…`, tags, and pushes after confirmation. The tag runs
`.github/workflows/release.yml`: version check, CI with the OCR smoke test on
both targets, packaging and the packaged-app gate, then a draft GitHub Release
with `SHA256SUMS` and build provenance. Review the draft and publish it by
hand. "Run workflow" on the Release workflow builds an unpublished test build
from any pushed branch; it goes straight to packaging (the branch already ran
CI) unless **checks** is ticked, and signs the macOS app only with **sign**.

### macOS signing

Signed builds are Developer ID signed and notarized
([ADR 0039](adr/0039-macos-signing-and-notarization.md)) from the `release`
environment. To set it up, or to replace an expiring certificate, the account
holder first opens a shell in a directory whose direnv `.envrc` loads the
`CERTIFICATE_OSX_*` variables from the Keychain (`cd` there on its own, so
direnv runs before the next command), then runs the commands below. Values are
piped, never on a command line, and an empty variable stops the loop: an
empty secret would only fail later, at the signing step.

```sh
gh api -X PUT repos/lawkitt/mdoc/environments/release
for pair in APPLE_CERTIFICATE=CERTIFICATE_OSX_P12_DATA APPLE_CERTIFICATE_PASSWORD=CERTIFICATE_OSX_P12_PASSWORD APPLE_ID=CERTIFICATE_OSX_ID APPLE_PASSWORD=CERTIFICATE_OSX_APP_PASSWORD; do
  src=${pair#*=}
  [[ -n ${(P)src} ]] || { echo "$src is empty"; break; }   # zsh
  printf %s "${(P)src}" | gh secret set "${pair%%=*}" --env release -R lawkitt/mdoc
done
gh variable set APPLE_TEAM_ID --env release -R lawkitt/mdoc --body F33DLM8K53
gh variable set APPLE_SIGNING_IDENTITY --env release -R lawkitt/mdoc --body "Developer ID Application: Tebriz Tagiev (F33DLM8K53)"
```

Then run the Release workflow with **sign** ticked to try the signed path
without a tag. To sign locally, import the certificate into the login
keychain, store notarization credentials once with
`xcrun notarytool store-credentials mdoc-notary`, then run
`APPLE_SIGNING_IDENTITY="Developer ID Application: Tebriz Tagiev (F33DLM8K53)"
APPLE_KEYCHAIN_PROFILE=mdoc-notary just package`.

### Case key and Keychain access

With a Developer ID provisioning profile, the case key lives in the data
protection keychain, and macOS never asks for the login password
([ADR 0045](adr/0045-prompt-free-case-key.md)). Until the profile exists,
signed builds use the login keychain. To turn it on, the account holder
registers the App ID `io.github.lawkitt.mdoc` with **Keychain Sharing**,
creates a **Developer ID** provisioning profile for it, and stores it:

```sh
base64 -i mdoc.provisionprofile | gh secret set APPLE_PROVISIONING_PROFILE_BASE64 --env release -R lawkitt/mdoc
```

Locally, set `APPLE_PROVISIONING_PROFILE=<path>` next to the signing variables.
Unsigned, ad-hoc and debug builds always use their own login-keychain key
(`io.github.lawkitt.mdoc.dev`) and `mdoc/cases-dev/`, so they never touch
release cases.

Before publishing a release that changes case-key handling, install it on a
clean macOS user account and run `scripts/keychain-launch-check.sh`. It seeds
a key this build cannot read silently and checks that launch shows no
Keychain dialog and leaves cases locked.

## App icon

The app icon's source is `build/appicon.svg`: a square SVG whose rounded square
fills the canvas. After changing it, run `just icon` on macOS (built-in `sips`
and `iconutil`, Python 3 without packages). It renders every size from the
vector: the macOS ICNS with the shape at 824 of 1024 px on a transparent
canvas, as Apple's icon grid expects, and the Windows ICO and
`resources/icons/icon.png` full-bleed.
