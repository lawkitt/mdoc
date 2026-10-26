# Captured AI-reply fixtures for Restore

Status: Accepted, 2026-10-19. Amends [ADR 0041](0041-cases-and-ai-response-restoration.md)
(stable-release invariant).

## Problem

Restore ([ADR 0041](0041-cases-and-ai-response-restoration.md)) is tested only
against synthetic replies in `crates/mdoc-pii/fixtures/restore/`, written
against a hand-made identity map. Nobody has checked that replies from real
assistants restore, so Restore cannot ship in a stable release (ROADMAP
must-fix).

## Decision

- **Four captures.** ChatGPT and Claude, each answering in English and in
  Russian, to the fictional settlement-agreement demo.
- **Russian demo.** `docs/product-media/Соглашение об урегулировании.md`, a
  fictional counterpart of `Settlement agreement.md` with the same structure,
  table and fictional-data footer, and a DOCX built by `make-demo-docx.py`:
  employer ООО «Северный двор» (Санкт-Петербург), employee Павлова Марина
  Сергеевна (also "Марина" and "Мариша"), her husband Павлов Дмитрий Андреевич
  (a shared surname, so inflected short forms per ADR 0068 occur), adviser
  Шахова Ирина Олеговна of «Елагин и партнёры», an email at the reserved
  `example.com` (not `example.ru`, which is not reserved), an invented phone
  (`+7 900 000-00-00`), a claim before a районный суд. The RU captures use it;
  the EN captures use the English demo.
- **Shipping flow.** Each demo is opened in mdoc, scanned, reviewed and applied;
  the input is exactly what **Copy for AI** produces, placeholder note included.
  The note is English-only today and stays so for the RU captures; localizing it
  becomes a roadmap item only if the RU replies show a problem.
- **Models.** The default model each product shows a signed-in user on the
  capture day, recorded in the README.
- **Capture.** The maintainer pastes the input into a new chat with memory off
  and copies the answer with the assistant's own Copy button, which is what a
  user pastes into Paste AI reply (plain text). Model name and date are
  recorded.
- **Prompt.** One fixed task prompt per language invites a Markdown table and
  running text about the parties (EN: "Summarise each party's obligations in a
  table, then list the risks for the Employee."; RU: the same in Russian, where
  aliases inflect). Replies are never hand-edited. A shape the Done-when asks for
  that does not appear after one retry is recorded in the README, not faked.
- **Layout.** `tests/fixtures/restore/<chatgpt|claude>-<en|ru>/` holds
  `sent.md` (Copy for AI output), `identities.json` (the mapping used; the data
  is fictional, so plaintext is fine), `reply.md` (as copied) and `expected.md`
  (reviewed). A shared `README.md` records model, date, prompt and capture
  steps. The synthetic fixtures stay as unit tests for edge shapes.
- **Test.** In `mdoc-pii`, reading the directory through
  `CARGO_MANIFEST_DIR/../../tests/fixtures/restore`. For every capture:
  `restore().unresolved == 0`; no current or previous alias of the map
  survives in the restored text; the restored text equals `expected.md` byte
  for byte. The test also checks that pseudonymizing the demo with
  `identities.json` gives `sent.md` without its note, so the map is the one
  that was used.
- **Mapping.** `identities.json` is written by hand from the reviewed panel
  (alias, category, originals with the primary wording first). An original the
  user left in the text is omitted, so the check above stays exact.
- **Expected text.** Each `expected.md` is written by hand from `reply.md`,
  every alias replaced by its primary wording, independently of Restore; each
  difference from Restore's output is decided as a Restore bug or a fixture
  mistake.
- **Frozen.** Captures are never regenerated. Later captures are added beside
  them with a date suffix.
- **Scope of "alias".** A token Restore recognizes (generated or custom). A
  paraphrase that carries no token ("the Employee", "Ms B.") is not an alias
  and is out of scope.
- **Misses.** A shape Restore does not recognize in a captured reply is decided
  case by case, leaning towards widening the matcher only for shapes that can
  only be an mdoc alias. Invented or renumbered aliases stay unresolved for the
  user to assign; Restore never guesses (ADR 0041).

## Consequences

- Restore's stable gate rests on four real replies, captured 2026-10-19;
  models change, so this shows the shapes of one point in time, not a
  guarantee. All four used tables and none inflected an alias, so inflected
  aliases are still covered only by the synthetic fixtures.
- The fixtures contain third-party model output about fictional data only.
