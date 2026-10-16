# Pseudonymization is a supported, assistive feature

Status: Accepted, 2026-10-10. Supersedes the "keep experimental status until
agreed evidence" rule of [ADR 0016](0016-pii-production-qualification-scope.md).

## Decision

mdoc presents pseudonymization as a supported feature: an assistant that
proposes replacements the user reviews, never a guarantee of anonymization.
The "Experimental" label leaves the UI and the README. A permanent line —
"Review for missed identifiers before sharing." — stays beside Apply.

The open qualification gates are ongoing quality work in the ROADMAP, not
release blockers. Measured caveats
(languages, misses, platform availability) remain stated as facts in the README.

## Rationale

The review-and-apply workflow is the product; the label discouraged its use
without adding protection the responsibility line does not already give.
Calling it supported must not imply verified completeness, so the caveats and
the user's review duty remain explicit.
