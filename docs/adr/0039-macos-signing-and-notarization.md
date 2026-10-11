# macOS signing and notarization

Status: Accepted, 2026-10-11. Implemented in the pipeline; first used by the
first release tagged after the signing secrets are in place.

## Problem

Unsigned macOS builds ([ADR 0037](0037-ci-and-release-pipeline.md)) are
blocked on first launch. From macOS 15, right-click → Open no longer bypasses
Gatekeeper: users must find **System Settings → Privacy & Security → Open
Anyway**. Lawyers who are not technical users give up there. The owner has an
individual Apple Developer Program membership (team `F33DLM8K53`) and a
*Developer ID Application: Tebriz Tagiev (F33DLM8K53)* certificate, valid until
2027-02-01.

## Decision

- **Developer ID**: tag builds, and test builds run with **sign**, re-sign
  `mdoc.app` with that certificate, with the hardened runtime, a secure
  timestamp and `resources/entitlements.plist` (the library-validation
  exception for downloaded PDFium and ONNX Runtime stays).
- **Notarization twice**: `scripts/macos-sign.sh` notarizes and staples the
  `.app` (so a copy dragged out of the disk image opens offline), then signs,
  notarizes and staples the `.dmg`. Rejections print Apple's log.
- **The `.dmg` is built with `hdiutil`**, not cargo-packager, whose disk image
  would repack the ad-hoc `.app` and sign ad hoc. The image holds `mdoc.app`
  and an `/Applications` link.
- **Credentials**: notarization uses the owner's Apple ID with an
  app-specific password (`notarytool --apple-id`), which already exists; an App
  Store Connect API key would add nothing for one maintainer. The GitHub
  environment `release`, used only by signed builds, holds the secrets
  `APPLE_CERTIFICATE` (base64 `.p12`), `APPLE_CERTIFICATE_PASSWORD`,
  `APPLE_ID`, `APPLE_PASSWORD` and the non-secret variables
  `APPLE_SIGNING_IDENTITY`, `APPLE_TEAM_ID`. `scripts/ci-apple-credentials.sh`
  imports the certificate into a throwaway keychain and removes it afterwards.
  Locally, `APPLE_SIGNING_IDENTITY` plus a `notarytool store-credentials`
  profile in `APPLE_KEYCHAIN_PROFILE` makes `just package` produce the same
  signed build.
- **Gate**: signed builds set `MDOC_REQUIRE_SIGNING=1`; packaging fails without
  the identity, and `just smoke-package` checks the stapled tickets and
  `spctl` acceptance of the `.dmg` and `.app` before the OCR self-test. Test
  builds from `workflow_dispatch` stay ad-hoc signed unless **sign** is ticked,
  which lets the whole signed path be tried before a tag.
- Windows signing is deferred: Microsoft's Trusted Signing limits individual
  eligibility, and SmartScreen's **Run anyway** is one click.

## Consequences

- The README and website drop the Open Anyway instructions for macOS once a
  signed release is published; release notes already describe the macOS app
  as signed.
- The certificate expires on 2027-02-01: create a new Developer ID Application
  certificate before then and replace the two certificate secrets. The
  membership renews yearly (next 2026-11-30). Expiry or a lapsed membership
  blocks new builds, not published ones: their timestamps and stapled
  tickets stay valid.
- Signed builds allow reconsidering automatic updates
  ([ADR 0038](0038-update-check.md)) on macOS.
