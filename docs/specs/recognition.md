# Recognition

What mdoc proposes for replacement, how it recognizes scanned pages, and how
recognition quality is measured. Terms are in [CONTEXT.md](../../CONTEXT.md).

## Scope

- Detection assists the user's review; it never establishes that text is
  anonymous, and no UI or doc says it does.
- Keep means "send the original"; Not a name, "never found", though a case
  keeps a wording another member uses.
- The qualified envelope is English and Russian legal text, native or
  OCR-derived, on Apple Silicon macOS and Windows x64; Linux builds, and its
  automatic scanning is stated separately.
- A scan covers everything Copy Markdown transfers, hidden source included
  (link targets, image paths, code, HTML); original files, file names and
  attachments are outside it. A window too long to scan is rejected, never
  truncated.
- Code that chooses, detects or reports a language takes a list of
  languages; each engine maps the list to what it can read and names what it
  cannot. Never add an EN/RU boolean or two-way switch; convert an existing
  one when you next touch it.
- mdoc claims a document language only once it has its own OCR model,
  spelling dictionary and PII qualification set.
- The UI language and the document language are independent.

## Models

- Models come from a curated catalog of pinned, checksum-verified bundles;
  never load an arbitrary local model.
- Nothing downloads without consent at the point of use: an inline card says
  what downloads, its total size, and that processing then stays offline.
- Nothing processes a document on its own: Settings never opens itself, a
  download started there never starts processing, and opening a document
  never starts recognition. Opening or editing never depends on a model.
- Inference runs offline, on the CPU.
- Installed, language support and qualified are shown separately, because an
  installed model is not a qualified one.
- Repair and Remove keep shared runtimes and refuse a model in use.
- Model compare runs on a captured snapshot, never on the working document,
  never changes defaults, and keeps its results in memory only, until the
  view closes.

## OCR languages

- Recognition languages are picked per document on the OCR card; the pick
  is never saved, never becomes the default, and a later Settings change
  doesn't move it.
- A Latin file name is no language signal, because Russian documents are
  often named in transliteration.

## Repeated headers and footers

- A running header or footer keeps its first copy on a page whose text
  reaches the Markdown, so a page left for OCR that doesn't run neither
  keeps nor loses a row.
- Numbers count, because "Case No. 2-123" and "Case No. 2-124" are
  different cases; only a page number may differ, never a number after an
  identifier label.

## Detector

- Setup re-saves the pinned encoder once with its weights in a `.onnx.data`
  file beside it, because ONNX Runtime keeps a second copy of weights stored
  inside a model file.
- A scan reads the source in windows of whole paragraphs, each heading
  starting one, because a section must scan the same wherever it sits.
- The no-model path (`scan_rules`) is not a pipeline target.

## Categories

- Proposed: names, organizations, contact details, addresses, identity, tax
  and bank identifiers, dates of birth, labelled case numbers and web
  addresses.
- Other dates, amounts and legal references stay unless the user marks them;
  dates are never shifted.
- A case number is proposed only after a label (Claim No, Case No, Дело №,
  номер дела), because statute and article numbers must stay.
- Courts, public bodies, laws and legal databases are never marked, as names
  or as web addresses; every other web address is.
- Date and Other can be added by hand; Other is never detected.
- A new category reaches the desktop, iOS and Android apps in one change,
  and saved mappings and cases from before load unchanged.

## Name forms and linking

- Inflection is a fixed suffix table, never morphology; irregular forms stay
  manual additions.
- Nothing is replaced until the user applies it, and nothing links without
  the user's click.

## Fixtures and the gate

- Fixture statuses: `pass` must pass; `known-flaw` must fail, strictly, so
  the gate fails once it passes until its status flips; `model-limit`
  records why no layer can fix it.
- A `pass` fixture never moves back, and no fixture is deleted or loosened,
  without the maintainer.
- One flaw class is one issue (label `flaw`), closed when its fixtures pass
  or are `model-limit`.
- Agents decide rule-pack data, pipeline code within this spec, fixtures and
  the guide's examples on their own.
- A new category, new UI, the guide's rules, the threshold, the model or its
  export go to the maintainer as a `decision` issue.

## Private sets and targets

- Probes are real public documents kept only in the private
  `lawkitt/mdoc-probes` with their source and licence, never as fixtures and
  never beside mdoc's output.
- Briefs, issues and PRs quote at most the span in question from a probe.
- The holdout (private `lawkitt/mdoc-holdout`) and the real set (about 25 EN
  and 25 RU probe documents, gold frozen) never appear in a fixer's brief;
  only their aggregates are posted.
- A holdout fall in `scripts/ci/hub-gate.sh` stops fixing and goes to the
  maintainer; both demos stay clean.
- Targets on the real set: recall at least 95% overall and 90% in each
  leaking category (PERSON, ADDRESS, IDENTITY, TAX, BANK, PHONE, EMAIL, date
  of birth, CASE), precision at least 85%.
- A miss class blocks the release; a false-hit class is fixed only once it
  appears in 3 real-set documents or 5% of its false hits, otherwise it goes
  to the "Recognition backlog" milestone.
- Synthetic documents only reproduce real-set or probe misses as fixtures.

## Outside sets

- A held-out outside set turns `tune` only with the maintainer, never back
  ([checklist](../../tools/sets/README.md)).
- Outside fixtures keep their licence, aren't recognition fixtures and don't
  gate detection.
