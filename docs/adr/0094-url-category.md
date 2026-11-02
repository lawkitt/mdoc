# URL category

Status: Accepted, 2026-11-02 (maintainer grill). Amends
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
- **Bare domains** are marked when the text plainly writes a web address (a
  known top-level domain, no space, not part of an email): «сайт
  ivanov-law.ru».
- **Variants.** Scheme (`http`/`https`), a leading `www.` and a trailing
  slash don't make a new identity: those writings join one identity as its
  variants. Any other difference is another identity.
- **Overlaps.** `mailto:` stays EMAIL and `tel:` stays PHONE; a URL inside a
  case number or a legal reference stays unmarked (rule 8); a URL span wins
  over a name, email or ID detected inside it.
- **Detection.** Scans propose URLs from rule-pack shapes (a scheme, `www.`,
  a bare domain with a known top-level domain) minus an exclusion list of
  public and legal domains. On by default in Categories to detect, English
  and Russian alike.
- **Order and saves.** "Web address" comes right after Email in Categories
  to detect and the category menu. Saved mappings and cases from before
  load unchanged; the token is only added.
- **All platforms at once.** The desktop app and the iOS and Android apps
  gain the category in one change (shared core, each app's category list
  and strings).

## Consequences

Categories to detect, the Replacements panel, Copy for AI, saved mappings and
the mobile apps gain one category. The guide gains rule 9 and amends rules
7 and 8; fixtures with names or emails inside identifying URLs are
re-marked, and their snapshots don't change (the model's input is the
same).
