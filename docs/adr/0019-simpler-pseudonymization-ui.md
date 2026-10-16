# Simplify pseudonymization around review, application and one copy action

Status: Accepted, 2026-10-08. Superseded in part by
[ADR 0020](0020-direct-replacement-workspace.md) (selected-item editor),
[ADR 0041](0041-cases-and-ai-response-restoration.md) (Copy for AI returns) and
[ADR 0048](0048-workspace-shell-redesign.md) (panel layout).

## Decision

- **One Replacements panel** replaces the review bar and Identities surface. It
  opens after scanning, keeps the document visible, and shows one
  original → alias row per identity with a mention count. Closing it keeps
  applied highlights clickable.
- **Explicit Apply**: one prominent action applies all remaining proposals as
  one undo step, with no mandatory per-row approval. Keep original on an
  identity excludes its pending mentions; single-mention Keep stays available.
- **Corrections stay** (rename, category, linking/separation, owner) but are
  contextual to the selected item, not routine.
- **Copy Markdown** copies the current text exactly. Pending proposals never
  block copying, and copying never applies them. Owner assignments stay local.
- **One concise caution** and pending status; never a safe-to-share claim.

## Invariants

Stable aliases, occurrence provenance, Keep, undo/redo, syntax protection and
stale-result guards are unchanged.

References: [full text before ADR 0052](https://github.com/lawkitt/mdoc/blob/f368e436cd2d3e70789d51879b9f54f379900e1e/docs/adr/0019-simpler-pseudonymization-ui.md),
[design tree](https://github.com/lawkitt/mdoc/blob/faa2c5e22a7eb85d4dc598c8f1f21eb125140044/docs/design/pseudonymization-ui-simplification.md),
[verification](https://github.com/lawkitt/mdoc/blob/f368e436cd2d3e70789d51879b9f54f379900e1e/docs/evidence/pseudonymization/2026-10-08-ui/README.md).
