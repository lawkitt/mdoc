# Personal documents and restore-only aliases

Status: Accepted, 2026-11-24 ([#1165](https://github.com/lawkitt/mdoc/issues/1165)).

## Why

Everyone holds documents an AI could help with: a passport, a lease, a child's papers. The commonest task is filling in a form. The Vault gets a Personal section, by person and kind. An ID is a card: a scan and fields the user confirmed. Anything else is an ordinary Document.

Personal details get their own alias series (`SURNAME_P1`, `ID_P1`), valid in every Project and chat, so a personal document sits beside any Project without two people sharing an alias. One alias stands for one stored field. mdoc puts it back exactly as stored and computes nothing: where a form wants a date split or an age, the AI says in words what to do by hand.

Each card field is an alias or readable in plain words (by default nationality, title, sex, document type, country, expiry), with a switch. AI access is off until the user allows documents. Real values and scans sit behind device authentication; the pseudonymized version does not.

## Rejected

- **Parts and formats computed by mdoc:** a wrong computed value on a form is worse than none, and "it only restores" is a promise users can check.
- **A preset Project:** one click shares it; it has no fields.
- **Per-Project aliases for personal details:** they collide beside a Project.
- **Closing Personal to the AI:** contracts are what people ask about.

## Revisit when

A form set shows that instructions done by hand fail too often.
