# Recognition fixtures

What the detector must propose, one synthetic CC0 document per file. The
[recognition spec](../../../docs/specs/recognition.md#fixtures-and-the-gate)
sets the layout and the statuses; the [guide](GUIDE.md) says what is gold
and where a span starts and ends. `just recognize` is the tool: its
modes are in the header of `crates/mdoc-detect/examples/recognize.rs`.

## Files

`{en,ru}/<kind>/<id>.md` holds front matter, then the document with
`⟦CATEGORY|text⟧` gold. A `model-limit` fixture adds `reason`. A fixture
with several flaw classes lists them, space-separated, in `flaw`, and
their issues in `issue`. A `known-flaw` fixture records its flaws as
`expect:` lines and must keep failing for exactly those, so a change can't
add a false hit while the old flaws hide it, and a partial fix records
what it fixed.

`<id>.model.json` beside it is its **snapshot**: the pinned FP16 model's
raw entities at threshold 0.5 (document byte offsets, label, score, text)
from the reference platform, CI's hosted Windows. Files keep LF line
endings because offsets are bytes.

A fixture passes when what the user would be shown (the correction stage,
then the review's conflict rules and repeats) equals its gold exactly: no
miss, no false hit, no boundary or category error.

## Adding a fixture

1. Write the document by the guide.
2. `just recognize --refresh-snapshots <file.md>` makes a draft snapshot on
   any machine with the model installed.
3. `just recognize <file.md> --gold` shows how it compares; set `status`.
   For `known-flaw`, `just recognize --record-flaws <file.md>`.
4. Commit both files and push. The PR's `ci/detection` regenerates the
   snapshot on the reference platform; `just snapshots-fetch <PR>` writes
   it into the tree to commit. If a score near the threshold moved, rerun
   `--gold` and `--record-flaws` on it.

Judge probes on `just import <file> > <id>.import.md`, mdoc's own import,
not on `pdftotext` or `textutil` output: each has flaws the other lacks.

`just recognize` with a folder path works on any folder of fixtures, such
as the holdout; run it from inside the repository. There the language comes
from an `en`/`ru` folder or a `language` front matter key, and a plain
Markdown file without front matter is annotated too.

## Imported

The hybrid corpus (33 cases) and the render corpus (18; its first 13 are
the hybrid's, word for word, so they are imported once) are re-marked here
by the guide. The originals under `tests/fixtures/pseudonymization/` stay
frozen for research reproduction. Re-marked: claim and case numbers after a
label became `CASE` gold (`en-identifiers`, `ru-identifiers`,
`hybrid-snils`, `hybrid-calibration-negative`, two negatives); labelled
invalid identifiers and an invalid IBAN in two negatives became gold (rule
6); and the long settlement's claim number and date of birth became gold.
The long fixtures are the repeats written out. Kinds follow content;
mixed-language cases sit under the language of most of their text.

`source: demo` fixtures come from the English and Russian demos: the
whole demos, and one fixture per flaw where a part reproduces it alone. The
Russian "Марина" miss shows only in the whole demo, which names it. One
alias per name form is about linking, not spans: every form is proposed,
so no span fixture pins it.
