# Grill: pseudonymized DOCX export with rdocx (2026-10-31)

Interview record for the Later item "[DOCX] Pseudonymized DOCX export with
rdocx, then rdocx for the preview". ADRs written from it hold the rules; this
file keeps the reasoning and the evidence.

## State at the start

- The item came from the 2026-10-17 grooming (commit f428df4): write the
  applied mapping into the original DOCX, keep formatting, cover headers,
  footers, footnotes, text boxes, comments and authors, tracked-change
  authors, properties and hyperlink targets; flag images and embedded objects.
  Then evaluate rdocx for the Source preview in place of docxide-pdf.
- [ADR 0002](../adr/0002-reviewable-local-pseudonymization.md) puts "PDF/DOCX
  originals" outside replacement scope; an export writes a new file and leaves
  the original alone, so ADR 0002 needs an amendment, not a reversal.
- What the review covers today: AnyDoc's DOCX conversion reads the body,
  tables, footnotes and endnotes, text boxes, content controls, tracked
  insertions, image alt text and hyperlink targets. It skips headers, footers,
  comments (shown only as preview metadata, ADR 0005), deleted revisions,
  revision and comment authors, document properties, document variables and
  custom XML.
- `mdoc_pii::restore::RestoreMap::pseudonymize` already replaces known
  originals with aliases (exact wording, whole words, longest first, no
  detection); Pseudonymize Clipboard uses it.
- The Source preview renders DOCX through docxide-pdf 0.18 in a supervised
  worker (ADR 0005). docxide-pdf pins zip 8, which keeps six advisory ignores
  in `deny.toml`.

## Anoni 0.86.1, hands-on (macOS, 2026-10-31)

Run with computer use on a synthetic probe (`make_probe.py`, kept in the
session scratchpad; no real documents). The probe puts names in the body
(one split across a plain and a bold run), Cyrillic inflections, a
`mailto:` hyperlink, a `HYPERLINK` field code, a comment and its author, a
content control, a table, a floating text box, image alt text, a header, a
footer, a footnote, and core/app properties.

**Implementation.** Tauri shell plus a compiled Python sidecar; DOCX goes
through python-docx and lxml (`doc.py`: `_substitute_docx`, `_redact_docx`,
`_strip_identity_metadata`, `_strip_annotation_authors`,
`_strip_field_hyperlinks`, `_flatten_revisions_and_controls`,
`docx_unread_content`, `_drop_unread_parts`, `split_hidden_spaces`).

**Flow.** Drop → Check → Export. Check shows one flat text of every story
(body, text box, header, footer, footnote, comment text). Export offers
Pseudonymize (Tokens or Realistic) or Redact, and a "Pseudonymized Word…"
split button (its menu needs full-screen control and wasn't opened). The
save sheet proposes a neutral name (`anonymise-20261031-0018`), not the
source filename. After saving it shows "Saved and checked" with Open and
Show in folder.

**What the exported DOCX holds** (unzipped and read):

| Surface | Result |
| --- | --- |
| Split-run name | Token in the first run's formatting; the bold run left empty. |
| Body, table, text box, footnote | Tokens as `⟦PERSON·1⟧` (brackets, middle dot). |
| Comment | Kept; text tokenized; author and initials blanked; date kept. |
| `mailto:` hyperlink | Relationship deleted; the link text stays styled but goes nowhere. |
| `HYPERLINK` field | Instruction emptied; display text kept. |
| Image alt text | `descr` removed; drawing names renamed to "Image N". |
| Core and app properties | Title, creator, last modified by, keywords, company, manager blanked; created date kept. |
| Content control | **The whole paragraph was dropped** ("Signed for the Client: …"), silently; it also never appeared in Check. |
| Header and footer | **Corrupted**: detection treated "Ivan Petrov" (header) and "Acme Holdings Ltd." (footer) as one ORGANIZATION span across the two parts; the header got `⟦ORGANIZATION·3⟧.` and the footer lost the company name. |
| Watermark | A VML "anoni.dev" watermark added to the header. |
| Package | Rewritten by python-docx; styles unchanged. |

A second probe with tracked changes showed no document view after 20 s and no
message (inconclusive).

**Lessons for mdoc.** Spans must never cross a part or paragraph boundary.
Content mdoc can't reach must be reported, never dropped silently. A
hyperlink whose target holds an original needs a decision (drop, or alias).
The neutral default filename matters: source filenames often hold client
names. A post-save check is cheap reassurance.

## rdocx 0.16.4 (read 2026-10-31)

MIT or Apache-2.0, one author, releases every few days (0.9 in August to
0.16.4 on 2026-10-28). Pure Rust; zip 9, quick-xml 0.41, `ring` only behind
optional features. Claims byte-for-byte preservation of unmodelled XML.

- `replace_text` / `replace_all` / `try_replace_all_expected`: literal
  substring, crosses runs, covers body, tables, headers, footers, notes, text
  boxes and content controls; skips comments and deleted text; refuses a match
  straddling a content control or insertion boundary. No word boundaries.
  Staged: serialized and reopened before commit.
- `redact_text(literal)`: **deletes** the literal from every story, comments,
  deleted and inserted revisions, field instructions, comment and revision
  authors, drawing `name`/`descr`/`title`, document variables, core and custom
  properties, chart caches and embedded workbooks, then scans every inflated
  part (UTF-8 and UTF-16) and fails if the literal survives. Does not cover
  relationship targets (hyperlinks) or extended properties (`app.xml`).
- `accept_all` / `reject_all` for revisions; `to_pdf`, page PNG/SVG; its own
  layout engine with bundled fonts.

### Prototype on the probe

A scratch binary (rdocx 0.16.4, default features off) ran `replace_text` for
each original → alias pair, longest first, then `redact_text` on each original
as a sweep, saved, and rendered a PDF.

- Stories: body, split-run name, table, content control, text box, footnote,
  header and footer all got aliases. The split name took the first run's
  formatting (the bold run was removed). `styles.xml` stayed byte-identical.
- No word boundaries: "Петров" would have matched inside "Петровым" had the
  longer variant not run first; a short variant such as "Ann" would hit
  "Annex".
- Comments: `replace_text` skipped the comment; the sweep **deleted** the name
  ("Check with  before signing."). The author was blanked, the initials "OS"
  kept.
- Alt text and the core title lost the name by deletion ("Signature of ",
  "Agreement with ").
- `app.xml` (Company, Manager) isn't rewritten, so the sweep's own leak scan
  refused every original found there; the refusal is atomic, which also left
  "Petrov" in the core keywords.
- The `mailto:` relationship target and the `HYPERLINK` field instruction were
  untouched (the field's slug `ivan-petrov` is no exact variant anyway).
- Timing (debug build, opt-level 1): replacements 52 ms, save 146 ms in all,
  PDF 11 ms.

## Outcome: deferred

After round 1 was asked, the maintainer deferred the feature (2026-10-31):
the evidence showed more open issues than the roadmap line suggested. The
item moved from Later to Deferred in ROADMAP.md; no ADR was written and none
of the questions below was answered. Recommendations are kept as a starting
point, not as decisions.

Issues the session surfaced:

- Replacing by wording can't separate homonyms or a name kept in one place
  and replaced in another; only Markdown keeps per-mention decisions.
- Headers, footers, comments, deleted revisions and properties never reach
  the review, so the export writes surfaces nobody checked.
- Links and field codes holding a name either break or leak.
- Images, embedded objects and signatures can't be checked at all.
- rdocx lacks whole-word matching, skips comments, deletes rather than
  substitutes in its sweep, and leaves `app.xml` and relationship targets
  alone; filling those gaps means mdoc owning a WordprocessingML pass.
- ADR 0002 keeps originals outside replacement scope; an export amends it.

## Open questions (round 1, unanswered)

Recommendation after each arrow.

1. What is written: mapping into the original DOCX, or a new DOCX from the
   edited Markdown? → Mapping into the original; say other edits are dropped.
2. Which replacements: this document's, or also the case's other names? →
   Also the case's; never wording the user kept.
3. One wording handled two ways? → Homonyms block; kept-once is replaced and
   listed.
4. Matching? → Whole words, exact case, longest first; across runs, never
   across paragraphs or parts; space, NBSP and tab equal.
5. Headers and footers? → Replace known names; say they weren't reviewed.
6. Comments? → Remove them.
7. Tracked changes? → Accept all first.
8. Personal information in the package? → Blank identifying properties,
   neutral authors, drop document variables and custom XML, generic drawing
   names; alt text gets replacements.
9. Links and field codes holding a name? → Drop the target, keep the text.
10. What can't be checked? → Leave untouched and count it; refuse macro and
    encrypted files.
11. After saving? → Reopen and scan every part; block with locations, "Save
    anyway" behind a confirm.
12. Entry point? → Share panel button, File menu, palette; DOCX sources only.
13. Save defaults? → Source name with replacements plus " (pseudonymized)",
    neutral fallback; source folder.
14. Placeholder look? → Plain `PERSON_1` in the replaced text's formatting.
15. Process? → Supervised worker with the preview's limits (ADR 0005).
16. Engine? → rdocx for the package only, mdoc-pii's matcher per paragraph,
    rdocx's leak scan; upstream fixes later.
17. Order? → Export first; the rdocx preview separately, only if fixture
    comparisons beat docxide-pdf.
