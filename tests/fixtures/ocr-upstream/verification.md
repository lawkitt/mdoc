# Upstream sync verification — 2026-10-04

Target: upstream `ef52f77850b29189048797a48b24862251f54fe7` (1.25.2).
App pin: fork `1baba87892a929e64d08069cb223b4a312f755cf`.

## Passed locally, Apple Silicon macOS

- Fork default and OCR-enabled tests; formatting including WASM; default and
  OCR clippy with warnings denied; release build; package version consistency;
  25 developer-script tests; fork CLI argument-order regression.
- mdoc `cargo fmt --check`, strict workspace/all-target lint and full workspace
  tests: 361 passed, 11 ignored. Existing compiler future-compatibility notice
  for `block 0.1.6` remains; it is not introduced by this update.
- Fresh app `setup_and_offline_english_russian_smoke` passed: downloads pinned
  runtimes/models into throwaway storage, validates setup a second time, performs
  offline EN/RU import, matches known transcripts and verifies source bytes.
  First attempt failed at DNS resolution before downloading; retry passed in
  13.50 seconds after network resolution was verified, without a code change.
- Corpus capture: 14 OCR pages (12 public, two synthetic) and four native
  originals, same harness/model/DPI/confidence/runtime inputs. All 54 prepared,
  recognition and provenance artifacts are byte-identical to baseline.
  Both synthetic transcripts match after recorded normalization, including Ё/ё.
- Retained comparison reruns reproduce `comparison.json`. The scorer checks
  fixture/reference SHA-256 and was independently checked on 961 string pairs.
- Lockfile changes are restricted to the direct fork pin/version and its
  `unicode-bidi` dependency edge, preserving unrelated Windows resolutions.

## Adoption gates and limits

- Fresh Windows x64 setup/offline app smoke is blocked on CI availability. The mdoc Windows x64
  job now explicitly runs the previously opt-in test after workspace tests.
  Passing macOS OCR and compiling Windows are not substitutes for loading the
  actual Windows runtimes. A green runtime smoke must precede adoption.
- GitHub's repository settings report mdoc Actions disabled (`enabled: false`),
  so app PR #7 has no checks. Fork PR #1's jobs remain queued on inherited
  Blacksmith labels; its repository reports zero registered runners. Running
  remote gates requires enabling app Actions and providing supported fork
  runners, or independently executing equivalent Windows/runtime checks.
- Remote PR CI is unverified; source/result equality does not establish native UI,
  keyboard/clipboard, GPU presentation or other platform acceptance.
- The fork's required external `pdf-evals` repository is absent locally and
  inaccessible as `firecrawl/pdf-evals`, so its full snapshot/semantic suite is
  unverified. Available fork integration fixtures and the new corpus were run.
- Public reference transcripts are agent-reviewed, not independently human
  verified. Difficult scan columns and financial sidebars retain baseline
  ordering/recognition limitations. No new model/accuracy qualification is claimed.

No direct main-branch adoption or release is included; both updates are delivered
as review PRs. The app's consent/settings/lifecycle logic is unchanged.
