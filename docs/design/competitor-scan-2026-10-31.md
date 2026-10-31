# Competitor scan, 2026-10-31

A desk scan of products that, like mdoc, hide client data before an external
AI sees it and put it back afterwards. Sources are vendor pages, store
listings, directories and two papers, read on 2026-10-31; claims are the
vendors' own and unverified, and nothing was installed. Earlier hands-on
notes: Anoni 0.86.1 in the
[2026-10-29 grill](roadmap-grill-2026-10-29.md). The ideas this scan raised
sit at the end of `## Later` in [ROADMAP.md](../../ROADMAP.md), each marked
for a grill before promotion.

## The field

| Product | Form | Detection and formats | Restore | Notable | Price |
| --- | --- | --- | --- | --- | --- |
| [Anoni](https://anoni.dev) (FR) | Desktop: macOS, Windows, Linux | CamemBERT plus rules; PDF, DOCX, Excel, e-mail, TXT, MD, CSV, images; local OCR; dates and "secrets" | Paste reply at Export; vault AES-256-GCM, shareable with a passphrase | Re-identification risk indicator; anonymization report with no personal data; permanent PDF redaction; watermark; watched folders; templates; journal; local MCP connector; per-profession pages | Free download, Pro tier |
| [Lexanon](https://lexanon.fr/en/articles/local-pseudonymisation/) (FR) | Desktop, local engine "Banco" | Names, orgs, places, identifiers by document type | Decodes the model's answer locally | Same code across a batch of documents; redact or code; built-in AI queries with the firm's own key (BYOK) | €450/yr or €45/mo |
| [Occlira](https://directory.lawnext.com/products/occlira/) (CY) | Desktop: Windows, Mac | 40+ types, 7 languages, tuned for DE/AT/CH; DOCX, PDF | Yes, local mapping | Word add-in with review; Claude Desktop connector; browser extension; custom term lists; every finding approved | $159 once |
| [Velum Privacy](https://slashdot.org/software/comparison/Occlira-vs-Velum-Privacy/) (ES) | macOS app, browser extension, Jira/Confluence app | Patterns, checksums for IBAN and national IDs, local NER in 9 languages | Yes; extension unmasks in page | Mapping ledger, audit log, MCP server for coding agents; unmasking never licence-locked | $149/yr, free tier |
| [anonym.plus](https://anonym.plus/blog/how-to-anonymize-before-ai.html) | Web app, desktop, Local API/MCP (Enterprise) | 340+ types, 46 languages, confidence per hit; PDF, DOCX, XLSX, CSV, JSON, XML, images | "Encrypt" mode tokens restore; Deanonymize tab | Custom regex; EU preset at 0.85 | Tiers, not listed |
| [Privatiser](https://privatiser.net/) | Browser extensions, VS Code, PyPI | Pattern-based, 13 languages; secrets, IDs, DOB | Deanonymize tab; mapping JSON export | Always-redact words, never-redact allowlist, custom regex, presets; open source | Free |
| [Amaze](https://heyamaze.com/blog/anonymize-data-before-chatgpt-claude-gemini/) (PL) | On-device app, beta | 18 categories, Polish IDs, Polish names across cases | Yes, private key | Air-gapped mode | Not listed |
| [Hush PII Guard](https://github.com/gabi200/hush-pii-guard) | Chrome/Firefox extension, open source | 15 MB on-device model (WebGPU/WASM), pattern fallback | Restores on screen only, green tint, hover shows the placeholder, eye toggle | Scans attachments (DOCX incl. comments and deleted tracked changes, PDF text); per-file "attach redacted / original / cancel"; never sends if redaction fails | Free |
| [Caviard](https://chromewebstore.google.com/detail/caviard-%E2%80%93-redact-pii-in-c/ncdaondhpapnhbeadnbcgpfmknhcoafh) | Chrome extension | Real-time patterns | Alt+R toggles masked/original | Realistic placeholders; ChatGPT, Claude, Gemini, DeepSeek | Not listed |
| [LawRider Redact](https://github.com/overthelex/lawrider-redact) | Chrome extension, open source, v0.1 | Names, emails, phones | Restores in a reply panel | Mapping in session storage | Free |
| [PII Shield](https://lawve.ai/en/mcp-servers/pii-shield) | MCP server for Claude Desktop | Placeholders `<PERSON_1>`, `<IBAN_1>` | Via the server | Open source | Free |
| [Limina (ex Private AI)](https://docs.private-ai.com/configuration-and-operations/advanced-features/llms) | Containers/API, on-prem | Text, PDF, Office, images, audio | Prompt "re-hydration" | Synthetic replacements that fit context | Enterprise |
| [Tonic Textual](https://tonic.ai/textual) | Platform/SDK | Text, PDF, images, tabular | Tokenize or synthesize per category | Realistic synthesis per entity type | Enterprise |
| Legora Word Actions, [LEGALFLY](https://www.legalfly.com/platform/ms-word-add-in) | Word add-ins inside legal-AI suites | Addresses, companies, dates, places | Not described | Anonymize without leaving Word | Suite pricing |
| [Harvey + Intapp](https://legaltechnology.com/2026/02/23/harvey-partners-with-intapp-for-ethical-walls-enforcement/) | Legal-AI platform | n/a | n/a | Ethical-wall enforcement on every AI interaction (permissioned, auditable) | Enterprise |
| [ДоксДодыр](https://urfaq.com/news/tpost/vm11l7udj1-doksdodir-besplatnaya-programma-dlya-obe) (RU) | Desktop: Windows, macOS, ~900 MB | DOCX only; ФИО, ИНН, ОГРН, КПП | No (removes) | Strips metadata and hidden revision history | Free |
| [Doczilla Pro](https://doczilla.pro/ru/blog/anonimajzer) (RU) | Module in a document platform | Local LLM for names in all cases; masks for sums, dates, %, СНИЛС, cadastral and procurement numbers | Yes, local code table | "Transit" mode the firm can make mandatory, or review-first mode | Enterprise |
| [Маска](https://github.com/alexandr-komarov/maska), [pii-guard-legal](https://github.com/marizariamor/pii-guard-legal) (RU) | Single HTML file / open-source projects | Contracts, acts, 1C exports | Yes, via labels | Leak-eval gate in CI ("0 leaks") | Free |

## What stands out

- **mdoc's edge:** review-first editing of the whole document as Markdown,
  OCR with the page beside it, cases that restore later replies, a free GPL
  app, and EN/RU with a Russian morphology story. Most rivals are paste
  boxes, extensions or batch converters without an editor.
- **Where rivals are ahead:**
  - *Where the work happens:* browser extensions restoring in the chat page
    (Hush, Caviard, Velum, Privatiser), a Word add-in (Occlira, Legora,
    LEGALFLY) and MCP connectors (Anoni, Occlira, Velum, anonym.plus, PII
    Shield). mdoc's agent access is Deferred.
  - *What counts as sensitive:* dates (Anoni, Doczilla, Privatiser's DOB),
    amounts and deal terms (Doczilla), "secrets" (Anoni), always/never word
    lists (Privatiser, Occlira).
  - *Inputs:* e-mail and images (Anoni, anonym.plus); hidden DOCX content
    checked (Hush) or stripped (ДоксДодыр).
  - *Firm controls:* mandatory mode (Doczilla), ethical walls (Harvey),
    audit logs (Velum, Anoni's report).
  - *Platforms:* Anoni ships Linux.
- **Pricing seen:** €450/yr (Lexanon), $159 once (Occlira), $149/yr
  (Velum), free plus Pro (Anoni), free/open source (Privatiser, Hush, PII
  Shield).

## Research on alias style

- Deußer et al., "On the Impact of Anonymization on the Performance of Large
  Language Models" ([arXiv 2609.11335](https://arxiv.org/abs/2609.11335), v2
  15 Sep 2026): five models, eleven benchmarks. Category-unique placeholders
  (pseudonymization) cost little (MuSR 0.73 → 0.70); generalization (0.58)
  and redaction (0.52) cost most; retrieval-augmented tasks dropped 0.22–0.47
  and stronger models lost more; telling the model its input was anonymized
  gave no consistent gain. No legal documents were tested. This supports
  ADR 0002's unique aliases over one shared label.
- [HAQQ](https://www.haqq.ai/blog/redaction-tools-for-lawyers) argues from it
  that role labels (`[LENDER-1]`, `[LENDER-2]`) keep who-owes-whom readable
  in contracts; untested on legal text.
- Yermilov et al. ([arXiv 2306.05561](https://arxiv.org/abs/2306.05561),
  2023): LLM-rewritten surrogates kept context more consistent than plain
  placeholders on downstream tasks; realistic names stay Not planned in mdoc
  (ADR 0018).
