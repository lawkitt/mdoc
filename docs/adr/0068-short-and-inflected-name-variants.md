# Proposed short and inflected forms of a known person

Status: Accepted, 2026-10-16. Amends [ADR 0018](0018-document-local-identity-review.md)
(surname alone); applies [ADR 0051](0051-roadmap-structure-and-stable-scope.md).
Numbered 0053 until 2026-10-25, when that number proved shared ([ADR 0069](0069-app-architecture-and-code-ownership.md)).

## Problem

A scan proposed "Maya Bennett" but not "Ms Bennett", "Ms Bennett's" or, in
Russian text, "Павловой" for "Павлова Марина Сергеевна". ADR 0018 let a surname
alone establish nothing, so those mentions stayed in the text after Apply and
reached the external AI tool.

## Decision

- **Discovery.** After a scan, every full person name that is not kept yields
  its surname: the last word, except in Russian names, where the patronymic
  places it ("Павлова Марина Сергеевна", "Марина Сергеевна Павлова") and in a
  two-word name the surname ending decides ("Петров Иван"). Ambiguous pairs
  ("Ирина Пушкина"), single words and initials yield none.
- **Forms.** The surname, with an optional title (Mr, Mrs, Ms, Miss, Mx, Dr,
  Prof in any case, with or without a period; Miss only capitalized,
  since lowercase "miss" is a verb), and for a Cyrillic surname its conservative
  case forms: -ов/-ин/consonant (+а, +у, +ым/+ом, +е), -ова/-ина (+ой, +у,
  and the masculine forms), -ский/-ская (-ого, -ому, -им, -ом; -ой, -ую).
  A possessive ("Ms Bennett's") is a mention of the form before the
  apostrophe.
- **Proposed, never linked.** Each wording found in the text outside an
  existing mention, protected syntax and applied replacements becomes a variant
  with an identity of its own. Same as… offers every full name it is a form of,
  first; nothing joins an identity without a user action. Unreviewed, Apply
  gives it a separate alias, so the wording no longer stays in the text.
- **First names.** A first name alone is never proposed. A surname that is also
  a given name of another person in the document is not proposed either
  ("Paul Smith" beside "James Paul").
- Manual additions and Same as… containment are unchanged (ADR 0024, ADR 0051).

## Consequences

- Short forms shared by relatives ("Ms Bennett" for Maya and Daniel Bennett,
  "Петрова" for Петров and Петрова) are offered for both; the user chooses.
- A capitalized common word that equals a surname ("Price") is proposed; Keep
  rejects it.
- Only scans discover these forms; a full name added by hand does not.
- Inflection is a fixed suffix table, not morphology: irregular or foreign
  declensions are missed and stay manual additions.
