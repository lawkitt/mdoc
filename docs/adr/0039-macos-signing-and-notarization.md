# macOS signing and notarization

Status: Accepted, 2026-10-11.

## Problem

Unsigned macOS builds are blocked on first launch. From macOS 15,
right-click → Open no longer bypasses Gatekeeper: users must find **System
Settings → Privacy & Security → Open Anyway**, where non-technical lawyers
give up. The owner has an individual Apple Developer Program membership
(team `F33DLM8K53`) and a *Developer ID Application* certificate.

## Decision

- **Developer ID**: tag builds, and test builds run with **sign**, re-sign
  `mdoc.app` with that certificate, the hardened runtime, a secure timestamp
  and `resources/entitlements.plist` (the library-validation exception for
  the downloaded PDFium and ONNX Runtime stays).
- **Notarization twice**: `scripts/desktop/macos-sign.sh` notarizes and
  staples the `.app`, so a copy dragged out of the disk image opens offline,
  then signs, notarizes and staples the `.dmg`.
- **The `.dmg` is built with `hdiutil`**, not cargo-packager, whose image
  would repack and sign the app ad hoc. It holds `mdoc.app` and an
  `/Applications` link.
- **Credentials**: notarization uses the owner's Apple ID with an
  app-specific password. The `release` environment, which admits only `main`
  and `v*` tags, holds `APPLE_CERTIFICATE`, `APPLE_CERTIFICATE_PASSWORD`,
  `APPLE_ID`, `APPLE_PASSWORD` and the variables `APPLE_SIGNING_IDENTITY`,
  `APPLE_TEAM_ID`; CI imports the certificate into a throwaway keychain.
- **Gate**: signed builds set `MDOC_REQUIRE_SIGNING=1`, so packaging fails
  without the identity, and `just smoke-package` checks the stapled tickets
  and `spctl` acceptance before the OCR self-test.
- Windows signing is deferred: Microsoft's Trusted Signing limits individual
  eligibility, and SmartScreen's **Run anyway** is one click.

## Consequences

- The certificate expires on 2027-02-01 and the membership renews yearly;
  replace the certificate secrets before expiry. A lapse blocks new builds,
  not published ones, whose timestamps and tickets stay valid.
- Signed builds enable in-app updates on macOS
  ([ADR 0040](0040-in-app-updates-on-macos.md)).
- Setup and local signing: [specs/release.md](../specs/release.md).
