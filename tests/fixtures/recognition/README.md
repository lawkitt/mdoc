# Recognition fixtures

What the detector must propose, one synthetic document per file
([ADR 0093](../../../docs/adr/0093-flaw-hunt-loop.md)); the
[guide](GUIDE.md) says what is gold and where a span starts and ends.

## Layout

`{en,ru}/<kind>/<id>.md`: front matter (`status`, `flaw`, `issue`, `kind`,
`source`, and `reason` for `model-limit`), then the document with
`⟦CATEGORY|text⟧` gold. The folders name the language and the kind. A
fixture with several flaw classes lists them, space-separated, in `flaw`
and their issues in `issue`. Beside each is `<id>.model.json`, its
**snapshot**: the pinned FP16 model's raw entities at threshold 0.5
(document byte offsets, label, score, text), made on the Mac. Files keep LF
line endings (`.gitattributes`): offsets are bytes.

A fixture passes when what the user would be shown (the correction stage,
then the review's conflict rules and repeats) equals its gold exactly: no
miss, no false hit, no boundary or category error. `pass` must pass;
`known-flaw` must fail, and once it passes the gate fails until its status
flips; `model-limit` is listed, never asserted. A `known-flaw` fixture also
records its flaws as `expect:` lines in its front matter, and its flaws
must match them: a change can't add a false hit while the fixture still
fails for its old reasons, and a partial fix records what it fixed. `just
recognize --record-flaws` writes them from the snapshots once a change is
right; the diff shows every flaw added or gone.

## Commands

```sh
just recognize <file.md>                  # proposals, marked like gold (the Mac's model)
just recognize <file.md|folder> --gold    # misses, false hits, boundary errors; status check
just recognize <file.md|folder> --gold --snapshot   # the same from snapshots, any OS
just recognize --scoreboard [<folder>]    # recall and precision per language × category × kind
just recognize --refresh-snapshots [<file.md|folder>]   # the Mac only
just recognize --check [<folder>]         # the detection gate
just recognize --record-flaws [<file.md|folder>]   # known-flaw fixtures' expect: lines
```

The model is the installed pinned FP16 in the app data (read only, never
downloaded; `MDOC_PII_MODEL_ROOT` and `MDOC_ONNX_RUNTIME` point elsewhere).
Folders default to this one; any folder of fixtures works, such as the
holdout: run `just recognize` from inside the repository with the folder's
path (relative to where you are). There the language comes from an `en`/`ru`
folder or a `language` front matter key, a document without `status` is
expected to pass, and one without a snapshot runs the model. `--gold` exits
1 when a document is not as expected; `--scoreboard` prints aggregates only
and exits 0. A plain Markdown file without front matter is annotated too.

`just test` runs every fixture on its snapshot on each OS
(`crates/mdoc-detect/tests/recognition.rs`). `local/macos` also runs `just
detection-gate` when a PR touches recognition or what it depends on: it
reruns the model over every fixture, failing on a stale snapshot, an
unexpected result or a model run over 5 minutes (about 14 s for 48
fixtures on the M4).

## Adding a fixture

Write the document by the guide, then on the Mac
`just recognize --refresh-snapshots <file.md>` and
`just recognize <file.md> --gold` to set its status; for `known-flaw`,
`just recognize --record-flaws <file.md>`. Commit both files.

## Imported

The hybrid corpus (33 cases) and the render corpus (18; its first 13 are
the hybrid's, word for word, so they are imported once) are here re-marked
by the guide; the originals under `tests/fixtures/pseudonymization/` stay
frozen for research reproduction. Re-marked: claim and case numbers after a
label became `CASE` gold (`en-identifiers`, `ru-identifiers`,
`hybrid-snils`, `hybrid-calibration-negative`, two negatives); labelled
invalid identifiers and an invalid IBAN in two negatives became gold (rule
6); and the long settlement's claim number and date of birth became gold. The long fixtures
are the repeats written out. Kinds were assigned by content; mixed-language
cases sit under the language of most of their text.

The demos' spans from #301 are `source: demo`: the whole English and Russian
demos, and one fixture per flaw where a part reproduces it alone (#361,
#362, #363, #364, #365 in English, #367). The Russian "Марина" miss (#365)
shows only in the whole demo, which names it. #366 (one alias per name form)
is about linking, not spans: every form is proposed, so no span fixture
pins it.
