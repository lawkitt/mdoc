# Recognition

Folds: 0006, 0016, 0026, 0065, 0066, 0068, 0080, 0093, 0094, 0108.

What mdoc proposes for replacement, how it recognizes scanned pages, and how
recognition quality is measured. Terms are in [CONTEXT.md](../../CONTEXT.md).

## Scope

- Detection assists the user's review; it never establishes that text is
  anonymous, and no UI or doc says it does.
- The qualified envelope is English and Russian legal text, native or
  OCR-derived, on Apple Silicon macOS and Windows x64; Linux builds, and its
  automatic scanning is stated separately.
- A scan covers everything Copy Markdown transfers, hidden source included
  (link targets, image paths, code, HTML); original files, file names and
  attachments are outside it.
- Code that chooses, detects or reports a language takes a list of languages;
  each engine maps the list to what it can read and names what it cannot.
- Never add an EN/RU boolean or two-way switch; convert an existing one when
  you next touch it.
- mdoc claims a document language only once it has its own OCR model,
  spelling dictionary and PII qualification set.
- The UI language and the document language are independent.

## Models

- Models come from a curated catalog of pinned, checksum-verified bundles;
  never load an arbitrary local model.
- Defaults: OCR PP-OCRv6 Small (English), detection GLiNER2 FP16; Cyrillic OCR
  and FP32 are optional.
- Nothing downloads without consent at the point of use: an inline card says
  what downloads, its total size, and that processing then stays offline.
- Settings never opens itself, and a download started in Settings never
  starts document processing.
- Opening or editing a document never depends on a model, and opening never
  starts recognition.
- Explicit OCR recognizes every selected page.
- Inference runs offline on the CPU through the app's one ONNX Runtime.
- One model job runs at a time; a conflict shows busy and retry, never a
  queue, and cancelling frees the slot only once the native call returns.
- Changing a setting never reruns a document; each result records the
  configuration it used.
- Installed, language support and qualified are shown separately, because an
  installed model is not a qualified one.
- Selecting a model changes only the preference; downloading is a separate
  action.
- Repair and Remove keep shared runtimes and refuse a model in use.
- Cancel keeps verified files and discards partial ones.
- Progress shows the bundle's total bytes, then "Verifying" and "Checking
  runtime"; never invent a percentage.
- A download continues when the user leaves; its resume belongs to the tab
  that asked, and a stale continuation is rejected by its generation.
- Invalid or obsolete stored settings show a notice and need an explicit
  reset; a removed model is never silently substituted.
- Settings persist atomically; a failed save keeps the previous values.
- Each model's last check is stored as file paths relative to the model root,
  sizes, modification times and a status kind, never content or error text.
- Settings shows a stored check at once and re-checks quietly, changing a row
  only on a difference; "Checking…" shows only for a row with no current
  stored result.
- Model compare runs on a captured snapshot, never on the working document,
  never changes defaults, and its exported report never holds mappings.
- Model compare results stay in memory only, until the view closes.
- Closing model compare cancels queued work and rejects late results.
- Advanced settings: OCR at 150, 200 or 300 DPI (default 150); minimum
  recognition confidence 0–1 (default 0); detection threshold 0–1 (default
  0.5).

## OCR languages

- Recognition languages are chosen per document: the OCR card offers the
  engine's lists (English; English & Russian), and the pick holds until that
  document converts.
- A per-document pick is never saved and never becomes the default.
- Once OCR runs, a later Settings change does not move that document's
  languages.
- The default comes from the first signal that says anything: the PDF's text
  layer (three letters of a script or more), Cyrillic letters in the file
  name, a language word in the file name ("ru", "english", "русский"), a
  Russian keyboard input source, else the Settings OCR choice.
- A Latin file name is no signal, because Russian documents are often named
  in transliteration.
- Suggested languages map to the first offered list holding all of them,
  else the one holding most.
- The card says in one line why it chose ("English & Russian: the file name
  is in Russian") and that the pick is for this document only.
- Only the Settings model's readiness is shared between tabs; another
  bundle's setup and failures stay with the tab that uses it.
- A setup that marked the shared state as installing always replaces it when
  it ends, even if Settings changed or the tab closed.
- Language detection reads only the text layer's letters, the file name and
  the input-source list; it never reads other content, sends or downloads.
- Tests inject the input-source signal and never read the machine's
  keyboards.

## Detector

- The model is GLiNER2 through gliner2-rs (Hub feature off) with the pinned
  privacy PII export; the threshold and the export change only by maintainer
  decision.
- Setup re-saves the pinned encoder once with its weights in a `.onnx.data`
  file beside it, because ONNX Runtime keeps a second copy of weights stored
  inside a model file.
- The re-saved pair is pinned by digest and stays the unmodified export; an
  older install's upstream encoder converts on its next check or scan,
  without a download.
- A scan reads the source in windows of whole paragraphs, each heading
  starting one, because a section must scan the same wherever it sits.
- A paragraph too long for one window is packed by its lines; only a line
  longer than a window gets overlapping windows.
- Windows are checked against actual token counts; a pathological window is
  rejected, never truncated.
- A window repeated word for word runs once.
- Limits: 2 MiB of source, 512 tokens per window, a two-minute cooperative
  deadline.
- One scan runs at a time, and each scan loads and drops its engine.
- Model load and a native inference call cannot be interrupted mid-call; a
  cancelled scan's late results are rejected.

## Correction pipeline

- At scan time, in `mdoc-detect`: model → `never` and `shape` filters →
  `context` rules → structured identifiers → `span` fixes → the user's lists.
- After a decision, in `mdoc-pii`: propagation.
- Every detection keeps its recognizer (`Model`, a rule, `Propagation`,
  `UserList`).
- The rule pack is TOML compiled in from `crates/mdoc-detect/rules/`
  (`shared`, `en`, `ru`); every entry carries a comment, and a unit test
  validates the pack.
- `never` lists titles, roles, determiners, category words and defined terms,
  which are never a name on their own.
- `context` lets a label propose a category for the value beside it.
- `shape` says what a span of each category must look like; a model span
  without its shape is dropped ("Ms", "husband", "the Employer", "email", a
  bank number with `/`).
- `span` fixes boundaries (trim a title, extend an address to its postcode).
- `variants` holds nicknames for given names, used only for names already in
  the document.
- Manual marks and the model share the pack's legal forms and date and name
  shapes.
- Preprocessing (windowing, table cells read as sentences, OCR hyphen joins,
  HTML entities decoded) runs before detection and readiness.
- The no-model path (`scan_rules`) is not a pipeline target.

## Categories

- Proposed: names, organizations, contact details, addresses, identity, tax
  and bank identifiers, dates of birth, labelled case numbers and identifying
  web addresses.
- Other dates, amounts and legal references stay unless the user marks them.
- A date is proposed only beside born, DOB, date of birth, дата рождения,
  д.р. or род.; dates are never shifted.
- A case number (token `CASE`) is proposed only in a labelled context (Claim
  No, Case No, Дело №, номер дела), because statute and article numbers must
  stay.
- An address is anchored on a postcode shape (UK, US ZIP, Russian six-digit
  index) or a street-type word with a house number, then extended to the
  clause or line boundary.
- A web address (token `URL`) is marked only when it identifies: a person's
  or private organization's page or file, a share link with a token, or any
  URL containing a name, email or ID.
- Courts, public bodies, laws and legal databases are never marked, as
  names or as URLs.
- A URL is one span from scheme to end without trailing punctuation, and it
  wins over a name, email or ID detected inside it.
- A Markdown link's target is replaced in place; its visible text is judged
  on its own, and a target equal to its text shares its alias.
- A bare domain is marked when the text plainly writes a web address.
- Scheme, a leading `www.` and a trailing slash join one identity; any other
  difference is another identity.
- `mailto:` stays EMAIL and `tel:` stays PHONE.
- Date (`DATE_n`) and Other (`REDACTED_n`) can be added by hand; Other is
  never detected.
- A manual mark is guessed as Person only for 1–4 capitalized words (hyphens
  and initials allowed), as Date for date shapes, otherwise as Other.
- A new category reaches the desktop, iOS and Android apps in one change,
  and saved mappings and cases from before load unchanged.
- Categories to detect are all on by default and stored per case; the
  Settings list filters documents without a case and seeds each new case.
- Unticking a category removes its unapplied proposals in the affected open
  documents; applied replacements and manual additions stay.

## Name forms and linking

- A full name that is not kept yields its surname: the last word, except in
  Russian, where the patronymic places it and a two-word name is decided by
  the surname's ending.
- Ambiguous pairs ("Ирина Пушкина"), single words and initials yield no
  surname.
- A surname's forms: with an optional title (Mr, Mrs, Ms, Miss, Mx, Dr, Prof;
  Miss only capitalized), and for a Cyrillic surname its fixed case-suffix
  table.
- Inflection is a fixed suffix table, never morphology; irregular forms stay
  manual additions.
- A possessive ("Ms Bennett's") is a mention of the form before the
  apostrophe.
- When a full name is proposed, applied or marked, propagation proposes its
  forms elsewhere: surname alone, first name alone, first name + patronymic,
  surname with initials, inflections and `variants` nicknames.
- A form with exactly one candidate full name joins that identity as a
  proposal; with two or more it keeps an identity of its own and gets a merge
  hint.
- First name + patronymic, and a surname whose every initial matches, link
  to a unique candidate; an initial matching a different given name breaks
  the link.
- A form that is also a common word ("Rose", "Мира") is proposed only when
  capitalized mid-sentence.
- A surname that is also another person's given name in the document is not
  proposed.
- Nothing is replaced until the user applies it, and the popup can unlink
  one mention.

## Merge hint

- Signals: a short form with one or more candidates; wordings that differ only in
  case, whitespace or punctuation; an edit distance of 1 between names of 6+
  characters; reversed Russian name order.
- Between two digits a separator difference still counts as a signal, except
  in phone, bank, tax and ID numbers.
- Never signals: a different title
  ("Mr" / "Ms Bennett"), Latin/Cyrillic transliteration.
- The hint is one line under the shorter (or later) identity, "Looks like
  Maya Bennett · Merge ×", with no score.
- Merge links into the fuller identity, which keeps its alias; the other
  alias is retired and still restores; it is one undo step.
- Nothing links without the user's click.
- Dismissing a hint holds for that pair: in the case when there is one,
  otherwise for the tab's life.

## User lists

- Keep on an identity adds its wordings to the case's Never list once none
  of its mentions is still proposed or replaced; undoing the Keep removes
  them.
- A manual mark adds its wording to Always, and a person's Always entry also
  yields propagation's forms.
- Lists match whole words, ignoring case and whitespace.
- A document outside a case uses the user-wide lists; in a case, the case's
  lists win.
- A wording is on one list at a time; a later addition moves it.
- Both lists live in the keychain-protected store, because they can hold
  client names.

## Fixtures and the gate

- A recognition fixture is `tests/fixtures/recognition/{en,ru}/<kind>/<id>.md`:
  front matter (`status`, `flaw`, `issue`, `kind`, `source`) and a body marked
  `⟦CATEGORY|text⟧`, with its raw model output in `<id>.model.json` beside it.
- Gold follows [GUIDE.md](../../tests/fixtures/recognition/GUIDE.md); only the
  maintainer changes its rules.
- Fixtures are synthetic and CC0: no real people and no copied document text;
  one may follow a probe's structure, with every sentence and value invented.
- `pass` must pass; `known-flaw` must fail, strictly, so the gate fails once
  it passes until its status flips; `model-limit` records why no layer can
  fix it.
- A `pass` fixture never moves back, and no fixture is deleted or loosened,
  without the maintainer.
- The pipeline runs on committed snapshots on any platform.
- Hosted `windows-latest` is the reference platform: it records the
  snapshots, and the gate checks them there; the Mac fetches refreshed ones
  from CI's `detection-snapshots-<k>` artifacts with `just snapshots-fetch`.
- The detection gate reruns the model, fails on a stale snapshot and asserts
  every fixture's spans.
- A fix adds no false hit on a negative fixture.
- The scoreboard reports recall and precision per language × category × kind
  and the count per status.
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
- `scripts/ci/hub-gate.sh <PR>` runs the detection crates' tests on the merged
  candidate and the holdout when its private path is set; a holdout fall
  stops fixing and goes to the maintainer.
- Targets on the real set: recall at least 95% overall and 90% in each
  leaking category (PERSON, ADDRESS, IDENTITY, TAX, BANK, PHONE, EMAIL, date
  of birth, CASE), precision at least 85%.
- The holdout stays at or above 97.6% recall and 94.0% precision, and both
  demos stay clean.
- A miss class blocks the release; a false-hit class is fixed only once it
  appears in 3 real-set documents or 5% of its false hits, otherwise it goes
  to the "Recognition backlog" milestone.
- Synthetic documents only reproduce real-set or probe misses as fixtures.
