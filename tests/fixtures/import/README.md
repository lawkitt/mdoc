# Import regression corpus

These fixtures and expected Markdown are copied from MIT-licensed
[AnyDoc](https://github.com/firecrawl/anydoc), revision
`76c444fd1df4df5eb824b55fd3ad144347c839a2` (upstream PR 153).
The license is retained in `LICENSE.anydoc`.

Inputs originate in upstream `tests/fixtures/{doc,docx,xls,xlsx,pdf,malformed}`.
Expected Markdown comes from the corresponding `tests/snapshots` files with
Insta metadata removed. The partly-scanned expected output is the `--skip`
snapshot. Upstream `tests/gen_fixtures.py` and `tests/fixture-src` contain their
generators and sources. They are synthetic test documents, not user files.

The corpus checks converter stability, not visual fidelity. Review output
changes when updating the fork; do not regenerate expectations blindly.
