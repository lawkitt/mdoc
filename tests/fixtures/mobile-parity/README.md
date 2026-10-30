# Mobile parity goldens

What the mobile core (`crates/mdoc-ffi`) gives for the demo documents, checked
against the desktop's GUI-free path by `crates/mdoc-ffi/tests/parity.rs`
([ADR 0085](../../../docs/adr/0085-mobile-apps.md), Parity tests). The Swift
parity suite reads the same files.

The captured Claude round trip in [`../restore/claude-en`](../restore/claude-en)
is the first check: the Settlement agreement demo joins a case holding its
`identities.json`, and Copy for AI must equal `sent.md`, which desktop put on
the clipboard, and the reply must restore to `expected.md`.

## `settlement-docx/`

[`Settlement agreement.docx`](../../../docs/product-media/Settlement%20agreement.docx)
converted in-process, reviewed with injected candidates (every whole
occurrence of each `claude-en` original, score 0.9; no scan), all replaced.

| File | Contents |
| --- | --- |
| `markdown.md` | The converted Markdown |
| `candidates.tsv` | Mentions after the candidates are ingested: byte range, state, alias, original |
| `replaced.tsv` | The same after Replace all |
| `copy-for-ai.md` | Copy for AI's text |
| `restored.md` | A reply quoting the pseudonymized document, restored |

Regenerate with `MDOC_BLESS_PARITY=1 cargo nextest run -p mdoc-ffi --test parity`
after checking why they changed.
