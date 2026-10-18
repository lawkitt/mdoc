# Captured AI replies

Real ChatGPT and Claude answers to the fictional settlement demos, the gate for
Restore in a stable release ([ADR 0056](../../../docs/adr/0056-captured-ai-reply-fixtures.md)).
`captured_replies_restore_completely` in `crates/mdoc-pii/src/restore_tests.rs`
checks every directory here. The synthetic replies in
`crates/mdoc-pii/fixtures/restore/` stay as unit tests for edge shapes.

## Captures

| Directory | Assistant | Model | Date | Demo |
| --- | --- | --- | --- | --- |
| `chatgpt-en` | ChatGPT | | | [Settlement agreement.md](../../../docs/product-media/Settlement%20agreement.md) |
| `chatgpt-ru` | ChatGPT | | | [Соглашение об урегулировании.md](../../../docs/product-media/Соглашение%20об%20урегулировании.md) |
| `claude-en` | Claude | | | [Settlement agreement.md](../../../docs/product-media/Settlement%20agreement.md) |
| `claude-ru` | Claude | | | [Соглашение об урегулировании.md](../../../docs/product-media/Соглашение%20об%20урегулировании.md) |

Shapes seen (fill in after capture): Markdown table, inflected aliases
(`PERSON_1у`), paraphrases, renumbered or invented aliases. A shape that did
not appear after one retry is noted here, not faked.

## Each directory

| File | Contents |
| --- | --- |
| `sent.md` | Exactly what **Copy for AI** put on the clipboard, note included |
| `identities.json` | The mapping used, written by hand from the reviewed panel |
| `reply.md` | The assistant's answer as its Copy button gave it, never edited |
| `expected.md` | `reply.md` with every alias replaced by its primary wording, by hand |

`identities.json` is a list of identities, primary wording first:

```json
[
  { "alias": "PERSON_1", "category": "PERSON", "originals": ["Maya Bennett", "Ms Bennett"] },
  { "alias": "ORG_1", "category": "ORG", "originals": ["Harrow & Finch Ltd"], "previous": [] }
]
```

`category` is the alias token (`PERSON`, `ORG`, `EMAIL`, `PHONE`, `ADDRESS`,
`IDENTITY`, `TAX`, `BANK`, `DATE`, `REDACTED`); `previous` lists earlier
aliases and may be left out. List only originals that were replaced: the test
pseudonymizes the demo with this map and expects `sent.md` without its note.
The data is fictional, so the mapping is stored in plaintext.

## Capturing

1. Open the demo `.md` in the current mdoc release, scan, review and **Apply**.
2. **Copy for AI** and save the clipboard as `sent.md`.
3. Write `identities.json` from the replacements panel.
4. In a new chat with memory off, on the default model the product offers a
   signed-in user, paste `sent.md`, then on a new line the prompt:
   - EN: `Summarise each party's obligations in a table, then list the risks for the Employee.`
   - RU: `Кратко изложи обязанности каждой стороны в таблице, затем перечисли риски для Работника.`
5. Copy the answer with the assistant's own Copy button and save it as
   `reply.md`. Fill in the model and date above.
6. Write `expected.md` by hand, then run
   `cargo test -p mdoc-pii captured_replies`. A difference is either a Restore
   bug or a fixture mistake; decide which before changing anything.

Captures are frozen: never regenerate one. A later capture goes in a new
directory with a date suffix (`claude-en-2027-03-01`).
