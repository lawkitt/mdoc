# Grill: Dependabot alerts on main (2026-10-31)

Interview record for the 17 open Dependabot alerts GitHub reported on `main`
at v0.3.0-rc.1 (1 critical, 3 high, 4 moderate, 9 low). No ADR: the decisions
apply existing practice (`deny.toml` ignores with reasons). This file keeps
the reasoning and the evidence.

## Evidence

### rustls 0.23.45 — alerts 1–12 (3 moderate, 9 low)

- Only in `tools/pseudonymization/Cargo.lock`, `…/gline/Cargo.lock` and
  `…/gliner2/Cargo.lock`. That tooling is outside mdoc's workspace and
  dependency graph; the app's `Cargo.lock` has no affected rustls.
- Path: `ureq` 3 (and `ureq` 2 via `hf-hub` in gline). The only HTTP call is
  explicit model setup (`tools/pseudonymization/src/lib.rs`, `setup`), from
  fixed URLs, size-bounded and SHA-256 verified.
- Advisories: GHSA-h2v8-394m-p7vv (Brotli cert decompression memory),
  GHSA-c8m5-c952-6cm4 (Session `Debug` prints secrets), GHSA-w44r-93j7-qv28
  (key-share downgrade to X25519), GHSA-vp88-798v-h67f (ECH rejection).
- Fix: 0.23.46, semver-compatible, published 2026-10-14 (passes the 14-day
  `global-min-publish-age`).

### zip 8.6.0 — alerts 13–17 (1 critical, 3 high, 1 moderate)

- mdoc's own code uses zip 9.1.2, which fixes all five. zip 8.6.0 comes in
  through `anydoc` (our fork, rev `cd905101`, `default-features = false`,
  `deflate`) and `docxide-pdf` `=0.18.0` (zip default features).
- Both read DOCX entries in memory through `ZipArchive::by_name`; neither
  calls `extract()` or `ZipStreamReader`.
- Per advisory:
  - GHSA-m665-v7pm-7grj (critical, ≤ 9.0.4): `ZipArchive` (central
    directory) vs `ZipStreamReader` (local headers) differential. Nothing
    here uses `ZipStreamReader`; every reader trusts the central directory.
  - GHSA-hjqx-j4rw-g7xv, GHSA-q7v9-x93c-xqv3 (high): symlink handling in
    `extract()`. Not called on zip 8. OCR runtime extraction
    (`crates/mdoc-import/src/ocr.rs`) uses zip 9 and skips symlinks.
  - GHSA-9hj8-73fh-9963 (high): Shrink decoder, only with the `legacy-zip`
    feature, which no dependency enables.
  - GHSA-8hxf-qr8p-62x4 (moderate): AES HMAC skipped on early EOF. Needs a
    password decrypt; nothing passes one, and `inspect_package` rejects
    encrypted packages before `docxide-pdf` sees them.
- Blocker: `docxide-pdf` 0.20.1 (latest, 2026-10-30) still requires zip `^8`.
  The `deny.toml` comment naming 0.18.4 as latest is stale.
- `cargo deny` stayed green because RustSec has not imported these GHSAs
  yet; it already ignores three older zip-8 `extract()` advisories
  (RUSTSEC-2026-0367/0368/0369) and three zstd ones for the same reason.

## Decisions (round 1, all accepted)

1. **Not a 0.3.0 must-fix.** No alert is reachable in the shipped app, and the
   rustls bump does not touch the shipped build. Fix on `main` normally.
2. **Bump rustls to 0.23.46** in all three tool lockfiles in one small PR.
3. **Dismiss the five zip alerts on GitHub** as "vulnerable code not actually
   used", with reasons matching `deny.toml`. `deny.toml` stays the tracker
   for blocked advisories; a new advisory opens a new alert.
4. **Move off zip 8:** bump our `anydoc` fork to zip 9 now; wait for
   `docxide-pdf` to move on its own (see round 2). No local `docxide-pdf`
   fork (exact pin, rendering review per ADR 0053).
5. **Pre-empt RustSec:** record the five GHSA ids in `deny.toml` with their
   reasons, and fix the stale 0.18.4 comment. *Amended while implementing:*
   the round assumed cargo-deny matches GHSA aliases in `ignore`; 0.20.2 does
   not (a GHSA entry for an imported advisory still fails, and warns
   `unknown-advisory`). So the reasons sit in a comment block, and the
   RUSTSEC id is added when `just deny` first names one.
6. **No Dependabot version updates for `tools/pseudonymization/*`.**
   Evaluation tooling; alerts still arrive; bump those locks on demand.

## Decisions (round 2)

7. **No upstream proposal to `docxide-pdf`.** Neither a PR nor an issue; wait
   for a release that moves to zip 9 by itself. (Recommendation was a PR;
   declined.)
8. **No dedicated `docxide-pdf` bump for zip.** Take zip 9 with the next
   planned `docxide-pdf` upgrade, so it costs one rendering review, not two.
9. **Two PRs.** First `lawkitt/anydoc`: zip 9 (master equals mdoc's pin
   `cd905101`, so nothing else rides along). Then one mdoc PR: rustls in the
   three tool locks, `deny.toml` ignores and comment, the new anydoc pin, and
   this note. Codex reviews each diff before it opens.
10. **The triage rule lives with the config:** `deny.toml` comments, plus a
    `dependabot.yml` comment that the tool lockfiles are left out on purpose.
    No `docs/development.md` section.
11. **Dismiss the five zip alerts after the mdoc PR merges**, in one batch,
    with reasons pointing at `deny.toml`. The rustls alerts close on merge.

## Invariant

zip 8 leaves the build only when both `anydoc` and `docxide-pdf` are on zip 9.
Until then every zip-8 advisory gets a `deny.toml` ignore whose reason names
the unreachable code path, or it is a must-fix.
