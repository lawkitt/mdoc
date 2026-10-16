# Product captures · October 16, 2026

Actual, unretouched window captures of the installed `/Applications/mdoc.app`,
version **0.1.0-beta.10**, on macOS. Each screenshot is a 2200 × 1522 JPEG, preserving the capture tool’s original bytes. Both themes
were selected inside mdoc; dark source pages use the app's themed paper mode.
The window sidebar is collapsed. Only fictional demonstration material is visible.

| View | Light | Dark |
| --- | --- | --- |
| Word conversion and source preview | [Light](../images/source-light.jpg) | [Dark](../images/source-dark.jpg) |
| Proposed identities and aliases | [Light](../images/review-light.jpg) | [Dark](../images/review-dark.jpg) |
| Original / pseudonymized comparison | [Light](../images/compare-light.jpg) | [Dark](../images/compare-dark.jpg) |
| Copy for AI and reply restoration | [Light](../images/share-light.jpg) | [Dark](../images/share-dark.jpg) |

The README automatically chooses a theme through `<picture>`. It shows conversion,
review and comparison. The website offers those three views and explicit theme
controls, and three short films made from the review, comparison and Share captures.

## Reproduce the content

[Services agreement.md](Services%20agreement.md) is the synthetic review input.
[Services agreement.docx](Services%20agreement.docx) is the Word conversion input.
Rebuild the DOCX with `python3 make-demo-docx.py` (requires `python-docx`).
Names and organizations are fictional, and the email uses the reserved example.com
domain. These are demonstrations, not detector-quality or performance benchmarks.

Open the Markdown, run Pseudonymize, and review the three identities / seven mentions.
Capture the Review panel at 95% document zoom; apply aliases to capture Side by side
with the panel hidden, and the Share panel with the editor visible. Open the DOCX
separately for its converted text and source preview, at 85% editor zoom and Fit width
for the source. The DOCX preview is approximate; dismissing its notice for composition
does not change that limitation. Keep the source at the top and move the pointer out
of the content before capture. No UI or document text was composited or retouched.

## Website films

Editable Rust/fframes sources and capture copies live in the website repository at
`videos/mdoc/`. Its `README.md` describes review, rendering and preview commands.
The 21-second combined film and three seven-second clips are intentionally silent.
Motion and theme changes are editorial transitions between still captures, not a
recording of processing time or live interaction. Review the whole document before
sharing: detection can miss identifiers.
