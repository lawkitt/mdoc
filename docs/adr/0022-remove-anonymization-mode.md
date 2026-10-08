# Remove Anonymization mode; Pseudonymization is the default

Status: accepted and implemented, 2026-10-08. The user decided "keep only Pseudonymization and make
it default behavior" during the
[refactoring interview](../design/pseudonymization-refactoring.md). The user confirmed the interview and requested full implementation.

## Problem

Two modes share one review model. Anonymize (toolbar default, Cmd/Ctrl+Shift+A)
scans and applies shared category markers in one step; Pseudonymize proposes
identity-linked aliases for explicit review and Apply. Their interplay requires
`Mode` branches, marker-to-alias conversion groups, a split toolbar button with
a mode menu, a second popup path and dedicated tests.

## Decision

Remove Anonymization mode. Pseudonymization is the only PII replacement behavior
and the toolbar default. This supersedes [ADR 0013](0013-reviewable-anonymization.md)
and the marker-specific parts of [ADR 0014](0014-highlighted-pii-replacements.md).

This is the one deliberate behavior change within the
[ADR 0021](0021-pseudonymization-refactoring.md) refactoring.

## Settled details

- Toolbar: single icon. No review → scan and open the Replacements panel with
  proposals (explicit Apply). Review with panel closed → reopen without rescan.
  Panel open → close. Cmd/Ctrl+Shift+P remains; Cmd/Ctrl+Shift+A and the
  Anonymize menu item are removed.
- One popup: highlight clicks open the panel with the ADR 0020 direct popup;
  legacy candidate/applied popups are removed; the overlap chooser remains.
- Marker→alias conversion is deleted; leftover markers are ordinary text.
- Mode-agnostic anonymization tests are ported to Pseudonymize; marker-specific
  tests are deleted. These are the only assertion changes in ADR 0021's work.
- ADR 0013 is superseded; ADR 0014 marker notes, CONTEXT.md, README and ROADMAP
  are updated in the same change.
