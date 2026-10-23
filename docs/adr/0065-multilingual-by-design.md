# Multilingual by design

Status: Accepted, 2026-10-23. Applied to OCR by
[ADR 0066](0066-recognition-language-defaults.md).

## Problem

mdoc is to become a multilingual app, but EN/RU is built in throughout: the
OCR model is offered as "English & Russian", spellcheck bundles EN/RU, and
pseudonymization is qualified for English and Russian legal text only
([ADR 0016](0016-pii-production-qualification-scope.md)). The 0.2.0 should-fix
item that picks the OCR recognition language would add one more EN/RU switch
unless the rule is set first.

## Decision

- **Language lists, not language pairs.** New code that chooses, detects or
  reports a language takes a list of languages. Each engine (PP-OCR today, OS
  engines later) maps that list to what it can read and says what it cannot.
  No new EN/RU booleans or two-way switches.
- **UI strings move to a catalog when localization is promoted** (Later:
  "[L10n] Localized UI, Russian first"). Until then English UI text stays
  inline; no partial translation.
- **Claims follow qualification.** A document language gets an OCR model,
  spellcheck dictionary and PII qualification set of its own before mdoc
  claims to handle it. ADR 0016's EN/RU scope stands until a language passes.
- **No next language is named yet.** One is added to Later when users ask.

## Consequences

- The OCR language pick offers its choices as language lists (English;
  English & Russian), so another model or engine adds entries rather than a
  new control.
- Existing EN/RU code is converted when it is next touched, not in a sweep.
- The UI and document languages are separate: a Russian UI does not imply
  a qualified Russian-only pipeline, and the reverse.
