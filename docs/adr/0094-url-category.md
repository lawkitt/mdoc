# URL category

Status: Draft (grill in progress), 2026-11-02. Amends
[ADR 0092](0092-recognition-correction-pipeline.md) (categories) and the
recognition guide's rule 7.

## Context

mdoc had no category for web addresses. A URL was never proposed whole;
only a name or email inside it was marked (`https://example.invalid/clients/⟦PERSON|Alice_Morgan⟧`),
so the rest of a personal profile, a private site or a share link with a
token reached the AI unchanged.

## Decision

- **A URL category.** Label "Web address" («Веб-адрес»), token `URL`,
  aliases `URL_1`, `URL_2` … like every other category's.
- **Identifying URLs only** (gold rule): a URL that points to a person's or a
  private organization's page or file is marked: profiles (vk.com/…, t.me/…,
  linkedin.com/in/…), personal and company sites, share links with tokens
  (Google Drive, Yandex Disk, Dropbox), and any URL that contains a name,
  email or ID. Courts, public bodies, laws and legal databases (consultant.ru,
  garant.ru, pravo.gov.ru) are never marked, as rule 8 says for their names.
- **One span.** The whole URL, scheme to end without trailing punctuation, is
  one `URL` span. A name or email inside it is no longer marked separately;
  the same name elsewhere in the text is still PERSON. Fixtures with names in
  link targets are re-marked.
- **Markdown links.** A link target is replaced in place (`[the claim](URL_1)`),
  so the syntax stays valid; the visible text is judged on its own, and a
  target equal to its visible text shares its alias.
- **Detection.** Scans propose URLs from rule-pack shapes (a scheme, `www.`,
  known hosts) minus an exclusion list of public and legal domains. On by
  default in Categories to detect, English and Russian alike.

## Consequences

Categories to detect, the Replacements panel, Copy for AI, saved mappings and
the mobile apps gain one category. Rule 7 changes for URLs.
