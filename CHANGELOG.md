# Changelog

All notable changes to **mdoc** are documented here.

## Unreleased

- **The import note counts pictures left out.** Pictures in a PDF or Word
  document never reach the Markdown; the note now says how many, for
  example "3 pictures not imported.", after any comments. A scanned page's
  image counts even when OCR read its text; a scan stored in strips counts
  once, and charts, diagrams and text boxes, which import as text, don't
  count (#911).

- **The name detector needs about half the memory.** Setup re-saves its
  encoder with the weights in a file beside it, which ONNX Runtime loads
  without keeping a second copy: finding names in a 10,000-character
  document peaked at about 2.5 GB on a Mac and now at about 1.2 GB (the
  optional FP32 model drops similarly). An existing install converts once,
  without a new download (#905).

- **Detection evaluation harness.** `tools/detection-eval/` scores candidate
  detection models and union combos through the app's own correction
  pipeline (`just recognize --scoreboard --snapshot`), exports ONNX builds and
  times them on the CPU, for choosing the detection options (#903). Nothing
  in the app changes.

- **The nightly runs during the maintainer's night.** Its cron moves from
  02:17 to 12:17 UTC: GitHub starts this repo's scheduled runs about 7 hours
  late, so the 02:17 run began around 09:00 UTC, mid-afternoon at GMT+7; now
  it begins about 02:00–03:00 GMT+7 ([scripts/ci/README.md](scripts/ci/README.md)).

- **The collapsed sidebar opens quicker on hover.** The document list now
  starts sliding out after 100 ms of hover instead of 200, and slides in
  120 ms instead of 150. The document beside it isn't redrawn while it
  slides
  ([ADR 0117](https://github.com/lawkitt/mdoc/blob/0aaf8f706e754d8b4c9ad905305b7bac194218a1/docs/adr/0117-gpui-0-3-12-and-a-quicker-sidebar-reveal.md)).

- **macOS 12 or later.** The desktop app moves to GPUI `gpui-pre` 0.3.12
  ([ADR 0117](https://github.com/lawkitt/mdoc/blob/0aaf8f706e754d8b4c9ad905305b7bac194218a1/docs/adr/0117-gpui-0-3-12-and-a-quicker-sidebar-reveal.md)),
  whose Metal shader build targets macOS 12, so macOS 11 is no longer
  supported.

- **The iOS UI shards split by measured time.** `scripts/ios-ui-shards.sh`
  packs the UI test classes into `ci/ios (ui 1)` and `(ui 2)`, longest
  first, by `apps/ios/ui-class-times.tsv`, each class's median time over the
  green UI parts of the last 14 days (a new class counts as the median).
  Each green UI part uploads its class times, and the nightly refreshes the
  table on the branch `nightly/ui-class-times`, never on `main`
  ([ADR 0113](https://github.com/lawkitt/mdoc/blob/0aaf8f706e754d8b4c9ad905305b7bac194218a1/docs/adr/0113-ci-targets-by-commit-and-detection-off-macos.md)).

- **Hosted iOS UI tests retry once.** A failed test in `ci/ios (ui 1)` or
  `(ui 2)` runs once more (`-retry-tests-on-failure -test-iterations 2`);
  a pass on the retry keeps the job green but is named in a warning
  ("Passed on retry in ci/ios (ui 2): <test>") and the job summary.
  `scripts/ci-stats.sh` reads `ci/ios`'s annotations, so a hosted iOS
  failure gets ADR 0112's same-test-elsewhere rule, and lists retried
  passes and the tests with two unrelated failures to quarantine. Unit,
  Rust, Android and script tests still never retry
  ([ADR 0113](https://github.com/lawkitt/mdoc/blob/0aaf8f706e754d8b4c9ad905305b7bac194218a1/docs/adr/0113-ci-targets-by-commit-and-detection-off-macos.md)).

- **The detection gate runs on hosted Windows.** `ci/detection` moved from
  hosted `macos-26` to `windows-latest`, in 12 shards from the job pool
  Windows shares with Linux, so it no longer waits for one of the five macOS
  runners. Hosted Windows is now the reference platform: the 256 recognition
  snapshots were recorded there again (193 had moved past the 1e-4 score
  tolerance from the Mac's), `just detection-gate` runs on Windows, and
  agents on the Mac take refreshed snapshots from CI with `just
  snapshots-fetch`. `just recognize --check` prints each fixture's model time,
  and a manual run of the gate with `record` uploads every changed snapshot
  ([ADR 0113](https://github.com/lawkitt/mdoc/blob/0aaf8f706e754d8b4c9ad905305b7bac194218a1/docs/adr/0113-ci-targets-by-commit-and-detection-off-macos.md)).

- **`mdoc-detect`'s unit tests build the rule pack once.** The `unit` and
  `rust` test tiers run `mdoc-detect`'s lib tests in one process instead of
  one nextest process each, so the rule pack is no longer built for every
  test ([ADR 0113](https://github.com/lawkitt/mdoc/blob/0aaf8f706e754d8b4c9ad905305b7bac194218a1/docs/adr/0113-ci-targets-by-commit-and-detection-off-macos.md)).
  Quarantined tests are skipped by exact name, and failures are named as
  nextest names them.

- **Hosted iOS failures name their failing tests.** A failed `ci/ios`
  part reads its xcresult and names each failed test, as
  `ios:<target>/<class>/<method>`, in its error annotation ("Failed in
  ci/ios (ui 1): <test> and 1 more", as the PC's status words it) and job
  summary, and `ci/ios` repeats them, so ADR 0112's "same test elsewhere"
  rule can read hosted iOS failures
  ([ADR 0113](https://github.com/lawkitt/mdoc/blob/0aaf8f706e754d8b4c9ad905305b7bac194218a1/docs/adr/0113-ci-targets-by-commit-and-detection-off-macos.md)).

- **CI targets by the kind of commit.** `scripts/ci-stats.sh` classifies
  each queue commit as core, desktop-only, iOS-only or other by the jobs it
  ran, and reports ADR 0113's targets: a median of 20 minutes for a core
  commit, 15 for a desktop-only one, 25 for every commit and 15 for the
  slower iOS UI shard. Each target has a start date, so the nightly's issue
  judges only data from after the change it depends on.

- **A desktop-only change skips the detection gate.** App-only files
  (`src/`, `vendor/`, `build/`, `tests/` outside the recognition fixtures)
  no longer select `ci/detection`, so such a queue commit runs `ci/macos`
  and `ci/windows` without detection shards; core, the recognition
  fixtures, `resources/`, the recognition crates and the gate's own script
  and workflow still run it
  ([ADR 0113](https://github.com/lawkitt/mdoc/blob/0aaf8f706e754d8b4c9ad905305b7bac194218a1/docs/adr/0113-ci-targets-by-commit-and-detection-off-macos.md)).

- **CI: the iOS UI tests run in two shards.** `ci/ios (ui 1)` and
  `ci/ios (ui 2)` replace `ci/ios (ui)`, each running half of the UI test
  classes, as `scripts/ios-ui-shards.sh` lists them by their time on the
  hosted runner; a new UI test class fails `lint (ubuntu)` until it is in
  exactly one shard, and a failing `ci/ios` names the part that failed
  (ADR 0112).

- **CI: `main`'s cache builds are no longer cancelled by the next merge.**
  A run cancelled while saving left the iOS core's cache key reserved and
  never written, so every iOS job rebuilt the core; `main-caches.yml` and
  `ci.yml` on `main` now let a running build finish.

- **CI: the queue runner's suite takes about 57 s instead of 96.** Its
  job over the time limit runs on a pass of its own with a 20 s limit,
  instead of waiting out the 60 s the heartbeat job needs. It was the only
  script suite over 60 s on Linux
  ([ADR 0112](https://github.com/lawkitt/mdoc/blob/0aaf8f706e754d8b4c9ad905305b7bac194218a1/docs/adr/0112-ci-speed-after-the-first-fortnight.md)).

- **The nightly checks ADR 0112's CI targets.** `scripts/ci-stats.sh` now
  measures a queue commit's time to a green gate, the Windows job, the iOS UI
  job and the queue drops a PR did not cause, and the nightly opens "CI
  targets missed" after 3 nights that missed one, closing it on a night that
  meets them all
  ([ADR 0112](https://github.com/lawkitt/mdoc/blob/0aaf8f706e754d8b4c9ad905305b7bac194218a1/docs/adr/0112-ci-speed-after-the-first-fortnight.md)).

- **CI: the recognition fixtures check on every core.** Their one test, the
  only Rust test besides the render test over 10 s on hosted macOS (15 to
  17 s), now spreads the fixtures over the machine's cores: about 6 s with
  three and 5 s on the PC instead of 20. The plain unit tests that took 22
  to 31 s on the PC run in under 10 s alone; they were slowed by the script
  suites beside them, which ADR 0112 moves off the Windows job
  ([ADR 0112](https://github.com/lawkitt/mdoc/blob/0aaf8f706e754d8b4c9ad905305b7bac194218a1/docs/adr/0112-ci-speed-after-the-first-fortnight.md)).

- **The iOS test sweep.** ADR 0112's rule applied to the iOS UI tests and
  the slow unit tests: three UI tests a unit test already asserts are gone
  (a new document's title, Find names again, one pane's missing row), four
  pairs that open the same screen share one launch, three keep only what
  the real screen shows, and `ReviewScrollTests` waits for the text to
  settle, laying it out as the screen would, instead of a fixed time (180 s
  to 115 s) ([ADR 0112](https://github.com/lawkitt/mdoc/blob/0aaf8f706e754d8b4c9ad905305b7bac194218a1/docs/adr/0112-ci-speed-after-the-first-fortnight.md)).

- **CI: the iOS job runs in two parts.** `ci/ios (unit)` runs the unit and
  parity tests, then the Release build that `ci/ios-release` ran;
  `ci/ios (ui)` runs the UI tests beside it. The timing tests
  run only in the nightly, and `just ios-test` takes
  `-only-testing` identifiers (ADR 0112).

- **CI: the PC's Windows job runs only the Rust tests.** `ci/windows` now
  runs `just test-tier rust` (every Rust test tier in one nextest run, then
  the doctests) instead of `just test`; the script suites stay on hosted
  Linux and macOS. They took about 25 of the job's 30 to 45 minutes and
  failed most of its runs, so its limit goes back to 30 minutes
  ([ADR 0112](https://github.com/lawkitt/mdoc/blob/0aaf8f706e754d8b4c9ad905305b7bac194218a1/docs/adr/0112-ci-speed-after-the-first-fortnight.md)).
- **CI: a PC status names a failing test without nextest's progress
  counter** ("Failed in just test-tier rust: mdoc-history tests::x", not
  "… ( 958/1944) mdoc-history tests::x").

- **CI: the PC's queue runner starts far fewer processes.** Reading the area
  table no longer starts one per line, and the runner's loop takes the time,
  a commit's short name and its done list from bash itself. On Windows, where
  Git Bash needs about 0.1 s for each, that cuts the script suites' process
  starts by 40 to 60 percent and the idle runner's own load.

- **A person's other name forms are proposed as the person.** A surname
  the model proposed as an organization is a person where the document
  gives a person that surname, writes it after a title ("Senator …",
  "Dr. …") or lists it after a plural title ("Senators …, … and …"); a
  company with a legal form or a body word ("… Holdings Ltd", "…
  Foundation") stays a company. A noun or a post-nominal after a name
  ("the … family", "… Lab", "M.D.", "Ph.D.") and a pleading's line number
  after a title come off; an all-capitals caption keeps the given name
  before a surname; Russian initials glued to a surname («Т.Т.Колобова»),
  after a double surname or a court officer's role are found whole, and a
  verb or a preposition before initials comes off
  ([ADR 0108](https://github.com/lawkitt/mdoc/blob/0aaf8f706e754d8b4c9ad905305b7bac194218a1/docs/adr/0108-recognition-targets.md); part of #503, #489,
  #793 and #387).

- **More English case and identity numbers are proposed.** A Court of
  Appeal (Criminal Division) number keeps its letter-digit suffix
  ("203199912 B2"); a Crown Court indictment's "T" number is a case number
  after "indictment" and in the bracketed list under the court's name; a
  UK tribunal's "Appeal reference" and "Case Number: TC …" are case
  numbers, not bank details; and a New York index number is found with
  its label glued to a running header. An officer's ID or badge number
  after the officer, a notary's "State No." in the notary's block and an
  aircraft's registration mark are identity numbers. Neutral citations, a precedent's docket number, form and
  statute numbers and an aircraft's type alone stay unmarked
  ([ADR 0108](https://github.com/lawkitt/mdoc/blob/0aaf8f706e754d8b4c9ad905305b7bac194218a1/docs/adr/0108-recognition-targets.md); part of #364 and
  #483).

- **Public bodies other than courts are no longer proposed as
  organizations or people.** Agencies, commissions, authorities and
  special districts, a public body's units (a subcommittee, an advisory
  panel, an inspectorate), local government and police, NHS bodies and
  statutory regulators, and the armed forces and their laboratories are
  left unmarked, by name or by a short name the document defines for
  them. So is a generic word alone ("the Agency", "the Commission", "the
  County", "the Crown"), a place cut off its county or police department
  ("Elko" of "Elko County Sheriff's Office"), and in Russian more federal
  services, customs, «МФЦ», regional statistics offices, dumas, election
  commissions, state hospitals and an OCR-glued «Министерствообороны».
  Universities, state-owned companies with a legal form, private bodies
  that share a word, people after a title and a public body's own contact
  details stay marked
  ([ADR 0108](https://github.com/lawkitt/mdoc/blob/0aaf8f706e754d8b4c9ad905305b7bac194218a1/docs/adr/0108-recognition-targets.md); #788).

- **Company and address spans end where the name does.** A company's legal
  form after a comma or in brackets joins its name ("QUILLAN, INC",
  «Банк Кедр (ПАО)»), with the dot of "Inc." or "N.A." left outside; a
  seal or execution phrase, a party's role, a job title or a citation's
  abbreviation before the name, and a year, a flight number or a
  pleading's line number after it come off; and a person's "Estate of …"
  is proposed as the person. A city, state and ZIP after a public body's
  or a company's name no longer take that name's words ("Review Board
  Washington, DC 20599" is "Washington, DC 20599")
  ([ADR 0108](https://github.com/lawkitt/mdoc/blob/0aaf8f706e754d8b4c9ad905305b7bac194218a1/docs/adr/0108-recognition-targets.md); part of #502, #374
  and #793).

- **Foreign street addresses are proposed whole.** Canadian addresses run
  to their province and postal code ("Montréal (Québec) H2Z 9Z9", "Halifax,
  NS B2Z 9Z9"); Swiss and other Continental ones open at a street whose
  type comes first ("Rue du Lac 4"), a German street or a named chalet and
  end at the town after the postcode; Taiwanese, Hong Kong and Malaysian
  ones open at their floor, unit, lot, "No." or building and run over
  their section, road, district or "Jalan" street to the city; Brazilian
  ones run from "Rua …, 412" to the city, state and CEP, and a farm with
  its CEP is an address. The country after the city stays outside
  ([ADR 0108](https://github.com/lawkitt/mdoc/blob/0aaf8f706e754d8b4c9ad905305b7bac194218a1/docs/adr/0108-recognition-targets.md); #551).

- **Defined party nouns are no people either.** A party's generic term
  that the document defines ("(“Distributor”)", "(hereinafter referred to
  as the “Promoter”)", «именуемое в дальнейшем «Поставщик»») is no longer
  proposed as a person, at the definition or at any mention, so it no
  longer spreads to every repeat; its possessive and compound terms
  ("Distributor’s Affiliates", "Disclosing Party", "Indemnified Party") go
  with it. More party nouns count (promoter, manufacturer, artist, venue,
  licensee, hirer and others; «Покупатель», «Заказчик», «Арендатор»). A
  name with a given name, a title or an initial, and a word the document
  never defines, are still proposed (part of #390).

- **Russian addresses keep their ends, and a split full name is one.**
  The city and index after the house now stay in the address ("ул.
  Кедровая, д. 9, г. Энск, 999999", "…, д. 9, Энск, 999999", "…, 52 в г.
  Энске"); a register extract's address in capitals with dotless place
  types ("999999, ЭНСКИЙ КРАЙ, Г ЭНСК, УЛ …", "Х", "ПОС", "С", "СТ-ЦА",
  "Р-Н", «ЗД.») starts at its index; and a house written first ("в доме
  № 7 по улице Садовой г. Энска") is an address from «доме» to the
  settlement. A place or an index alone, and «дом» in prose, are still no
  address. A surname and a given name with a patronymic that the model
  found apart on one line ("Коврижкина" and "Елена Тарасовна") are now
  one name, never across a line, a cell or two full names
  ([ADR 0092](docs/adr/0092-recognition-correction-pipeline.md), #362).

- **More US and UK address lines are proposed whole.** A city, state and
  ZIP with no street ("Ashby Falls, OH 45999", "Washington DC 20017") are
  an address, also with a ZIP+4, in capitals, with the state written out
  or with one OCR letter in the ZIP. An address that opens with a floor,
  suite, unit or building opens there; it runs on to a ZIP after a comma
  and over a pleading's line number, and keeps its house number before a
  stray comma. A corner of two streets with its city and ZIP, and a
  letterhead's street line with a floor or a compass part ("1640 Q St.,
  N.W."), are addresses too
  ([ADR 0108](https://github.com/lawkitt/mdoc/blob/0aaf8f706e754d8b4c9ad905305b7bac194218a1/docs/adr/0108-recognition-targets.md); part of #498).

- **Russian company names keep their spelled-out legal form.** A company
  named after «общества с ограниченной ответственностью», «акционерному
  обществу» or «публичного акционерного общества», in any case form and
  before straight or angle quotes, is proposed whole, form and quotes
  included, as «ООО «…»» already was; a state or municipal enterprise
  spelled out is a public body and stays unmarked (part of #374).
- **More Russian case numbers.** A court's number after «производство №»,
  the first-instance number an appeal cites («1 инстанция №», «номер дела
  в суде первой инстанции»), an appeal header's «Судья … № 33-…/…» and a
  УИД standing alone by its shape are proposed as case numbers (part of
  #364).

- **Names come without their titles and roles.** "Defendant Holloway",
  "Deputy Calloway", "HIS HONOUR JUDGE TALBOT" and «судья Мухин» are
  proposed as the name alone, and a bare "Defendant" elsewhere is no longer
  taken for a name. A pleading's line number inside a name keeps it one
  name, and a stray "Â" left by a mis-decoded space comes off a name's end
  (#382, #489).

- **Editing polish.** "Turn into" applies to every block a selection
  touches, skipping blank lines and closing up new list items, and turns
  them all back into text when each is that kind already; a code block
  wraps the selection in one fence. Italic on a bold selection gives bold
  italic, and Bold on an italic one too. A new table row or column takes
  the caret into its own cell in line with where it was; Return in a
  table's last row adds a row, and on an empty last row removes it and
  leaves the table. Ordered lists show the number written (`9.` shows 9.,
  a nested `3.` shows c.), as Copy carries it
  ([ADR 0110](https://github.com/lawkitt/mdoc/blob/0aaf8f706e754d8b4c9ad905305b7bac194218a1/docs/adr/0110-mobile-editor-polish.md)).

- **CI: the Linux dependency install retries on another mirror.** When
  the runner's Azure Ubuntu mirror stalls, the second attempt uses
  `archive.ubuntu.com`.

- **The editor's rules live in one GUI-free crate.** Enter and ⇧Enter,
  joins on ⌫/⌦, formatting toggles, indent, Turn into, task boxes, table
  commands, paste clean-up, the undo history, search and the reveal rule
  move from `mdoc-editor` into `crates/mdoc-editor-core`, which the mobile
  apps will call too ([ADR 0109](docs/adr/0109-mobile-markdown-editor.md)).
  The desktop editor behaves as before.

- **A table with a blank cell no longer stops recognition.** In a document
  that names a person whose initials appear on their own, a Markdown table
  with a cell of only spaces (or a no-break space) made recognition fail
  with a panic; the blank cell is now read as
  empty ([ADR 0092](docs/adr/0092-recognition-correction-pipeline.md)).

- **The rule pack is split by layer, and its largest patterns by form.** No
  change in what is proposed: the rules' code is one module per layer, and
  the Russian and English address patterns and the cited-case pattern are
  lists of their forms, each compiled alone, so the next address rule stays
  within the regex size limit
  ([ADR 0092](docs/adr/0092-recognition-correction-pipeline.md)).

- **A passport's number in its machine-readable zone is proposed.** On a
  passport's or ID card's MRZ ("7605318428RUS9104123F…", a card's
  "I<NORD231458907<<<…"), the document number is an identity document,
  without the "<" fillers that pad it and its check digit, OCR-damaged
  too, in documents imported from Word, RTF or ODT and inside Markdown;
  its alias restores in the zone. The zone's dates and other check digits
  stay unmarked, and a masked number gives nothing
  ([ADR 0092](docs/adr/0092-recognition-correction-pipeline.md), #495).

- **OCR-damaged words are no longer proposed as names.** In a Russian
  scan, a common word or abbreviation written in Latin or Greek look-alike
  letters («Ha» for «На» before a date, «Bce», «T.π.») is dropped as a
  person or an organization; a real name with a letter of its own
  («Ивaнов», "Hans", «ООО «Abc»») stays. In an English scan, "PagelD"
  ("PageID"), "IntereStee", "LIU" in "(Full name of company)", "LIZ" after
  a company's name, "V.S. agriculture" and "Netberlaads" are dropped, a
  "Depertuent of Agriculture" is a public body, and a garbled token is no
  tax number ("t e43b7") or identity number ("2808888888R") without its
  label ([ADR 0092](docs/adr/0092-recognition-correction-pipeline.md), part
  of #549).

- **A US Social Security number is an identity number.** After its label
  ("SSN", "S.S.N.", "Social Security No."), an SSN is proposed as an
  identity number rather than a tax number, even when it is damaged
  (missing dashes, spaces, an OCR letter for a digit); an EIN, TIN or ITIN
  stays a tax number
  ([ADR 0092](docs/adr/0092-recognition-correction-pipeline.md); part of
  #381).

- **A person's name OCR damaged is no longer proposed as a company.** Where
  the model took "A1ice M0rgan" or «Ко3лов И. П.» for an organization, the
  name is a person once its digits read back as the letters OCR mistook
  them for; companies with digits ("3M", "A1 Group", "M0rgan Stanley Ltd"),
  codes and names with a legal form or a company's word stay organizations
  ([ADR 0092](docs/adr/0092-recognition-correction-pipeline.md), part of #377).

- **A surname with initials spreads to its later mentions.** A person
  named by a surname and initials, in either order and with a comma or
  not ("Penrow, J. L.", "J. L. Penrow", «Иванов И. И.», «И. И. Иванов»),
  or by a surname, first name and middle initial ("VALECROFT, Jorin P.",
  "Ivo D. Penrow"), now has its later bare surname, its Russian case forms
  («Иванова», «Иванову», «Ивановым») and its other initialled wordings
  proposed and joined to that person, as a full name's are. Two people
  with one surname and different initials keep the bare surname apart;
  a surname that is also an institution's or an ordinary word of the
  document («Суд Н. О.», "U.S. Bank" beside "the Bank") spreads only its
  initialled wordings, and initials alone are unchanged (fixes #749).

- **Labelled identifiers are proposed even when invalid.** After its label
  a СНИЛС, ИНН, passport, account, БИК, payment card or personnel number
  («Табельный номер: VK-731628») is proposed with its category whatever its
  checksum, a digit short or long or with an OCR letter for a digit too, as
  is an English passport or card number; an unlabelled number that fails
  its checksum is still left to the model (fixes #381).

- **Email addresses with escaped underscores.** An address whose
  underscores are Markdown-escaped, as a Word import writes them
  (`snake\_case@…`, `\_roman@…`), or read as emphasis or strikethrough
  inside it (`__init__@…`) is proposed whole, in code too, so no part of
  it stays visible; emphasis around an address stays (fixes #746).

- **A surname-first name is one name.** A surname, a comma and the
  initials after it ("Penrow, J. L.", "Draycott,D.P.", «Иванов, И. И.»)
  are proposed as one person, whether the model found the surname alone or
  the surname and initials apart. So is a surname with a given name and a
  middle initial, a second given name or a patronymic after it where a
  name stands alone, on its own line, after a label or in a table
  ("VALECROFT, Jorin P.", «Маркович, Савелий Игоревич»), and with a given
  name alone before a reference's year ("Herder, Mike. 1999."). A judge's
  or a lawyer's post-nominal ("Brown, J.", "Denning, L.J.", "Harlow,
  Q.C."), a place ("Washington, D.C.", "Halifax, Nova Scotia"), two
  surnames ("Witnesses: Harlow, Brennan"), a role after the comma and
  "and" between two people stay apart (fixes #414).

- **A known full name written with decomposed letters joins its person.**
  «Артёма Антоновича Зайцева» typed with «ё» and «й» as a letter plus a
  combining mark reads as the same name, the known «Артём Антонович
  Зайцев» in another case, and is proposed and joined to him; the span
  keeps the document's own characters
  ([ADR 0092](docs/adr/0092-recognition-correction-pipeline.md), part of #418).

- **Dotless initials after a person's role are a person.** In a
  depersonalised Russian act, two or three capitals after a role only a
  person has, such as «потерпевший», «гр.», «ИП» or «свидетель»
  («Потерпевший КЛМ», «ИП ДРС»), are proposed as a person, not an
  organization, and so are their repeats; «гр.» stays outside a person's
  name. After a party's role («истец», «ответчик») letters the model took
  for an organization become the person only where the document names
  them as a person elsewhere. Companies («ООО «ДРС»», «АО БАВ», «(далее –
  КСС)»), public bodies («МЧС»), abbreviations, a country after
  «гражданин» («гражданин РА»), letters before a name they qualify («КЛМ
  Групп») and headings in capitals stay as they were (fixes #491).

- **Email addresses with underscores.** An address with an underscore in
  its local part (`quill_and_ink_77@…`, `<tam_roe@…>` without its angle
  brackets) or starting with one (`_roman@…`) is proposed whole; an
  underscore inside an address is no Markdown emphasis (fixes #412).

- **Company names in nested quotes stay whole.** A Russian company whose
  quoted name holds a second quoted name (ООО «Правовая группа «Кедровый
  щит»», straight quotes too) is proposed as one span, from its legal form
  to the last closing quote, where the detector cut it at the inner quote,
  kept only the inner name or left the outer closing quote out (fixes #557).

- **A surname keeps its particle, and a short name after «гражданин» or
  "Mr" is a name.** A lowercase particle before a proposed surname joins
  it ("де Лаваль", "фон дер Пален", "van der Berg", "de la Cruz",
  "d'Estaing", «аль-Хадид»), with the given name and patronymic or initials
  before it, and its repeats keep it; «да», «фон», "van", "von" and "ten",
  which are also words, join only after an initial, a title or a word like
  «на фамилию». Two short capitalised words, of up to four letters each,
  after «гражданин», «господин» or a courtesy title are proposed as a
  person where nothing else is («гражданина Лю Бо», "Mr Li Na"); a country
  after «гражданин» stays unmarked (fixes #403, fixes #404).

- **A labelled name keeps its value before a full stop or comma, and «ФИО»
  labels a full name.** "Surname: Trustee, Given name: Edda." gives
  "Trustee" and "Edda" without the punctuation, and «Ф.И.О.: Ветлин Нил
  Олегович» is proposed as one person; a lone «ФИО: Истец» stays a form's
  placeholder (fixes #736).

- **An identifier beside a labelled ИНН or ОГРН stays hidden.** Where the
  model reads a tax number and the account, passport or phone after it as
  one value, the labelled number keeps its own span and the next field's
  value gets its own, without its label (fixes #739).

- **A role word after a name part's label stays a surname when a
  neighbouring name part's label holds a name.** "Given name: Edda / Surname:
  Trustee" and «Имя: Лада / Фамилия: Должник» keep their value as a name, as
  a cell under a name-part header already did; a lone "Full name: Claimant"
  stays a form's placeholder (fixes #601).

- **ОГРН, ОГРНИП and ОКПО are tax numbers.** A value after one of these
  labels, a public body's own too, is proposed as a tax number like an ИНН,
  never as an identity document or bank details, and is no longer missed
  beside its label; КБК and ОКТМО codes stay unmarked (fixes #563).

- **More common words are no longer proposed as people.** Template and
  heading words ("Fire", "Location", "Index", "OPERATING MANUAL", "Serious
  Accident Investigation"), group nouns ("Investors", "Librarians",
  "Founding Fathers"), deities ("God", "Jesus Christ"), a pay grade
  ("GS-14"), a plot's or a list's letter, a programme's long title and the
  "Ex p" of a caption are dropped. A person named so stays after a title
  or where the document gives the full name ("Edda Fire", then "Fire")
  ([ADR 0092](docs/adr/0092-recognition-correction-pipeline.md), part of
  #500).

- **A company's short name is no person.** A name the detector took for a
  person is proposed as the company on strong evidence in the document: a
  short name it defines right after the company ("Quillon Freight Corp.
  (“Quillon”)"), the name before its legal form, the first word before a
  business word ("Pellam" of "Pellam Bank"), or the whole name in another
  letter case beside a legal form. One name with "airlines" or "air fares"
  after it, and a team "known as" one, is a company too. Firms named after
  partners, a company named after its owner, «ИП» and anyone with a title
  or a given name stay people (part of #548).

- **Russian settlement types keep an address whole and are no
  initials.** A workers' settlement written «р. п.» or «р.п.» no longer
  cuts an address before it ("431999, Энская область, р. п. Кедровка,
  улица Садовая, дом 8" is one address from its index), a settlement with
  a house and no street is an address ("с. Кедровка, д. 5, кв. 2", "п.
  Кедровый ключ ДОС № 1, кв. 2"), and a settlement's type before a place
  («уроженец а. Новый Хутор», «районного суда г. Ольховск», «проведено в
  г. Ольховск») is no longer proposed as a person's initial. A place
  alone is still no address, and real initials ("Иванов А. Б.", "А.
  Петров") stay names
  ([ADR 0092](docs/adr/0092-recognition-correction-pipeline.md), #482).

- **A column of company abbreviations keeps them as companies.** A
  table cell the model reads as a company stays one when other cells of its
  column are companies too, or the name is quoted («ООО „ЯВР“»), even if
  its letters spell a person's initials (part of #387).

- **Names in a passport's machine-readable zone are proposed.** On a
  passport or ID card's MRZ line ("P<RUSKRENC<<LIDIIA<ARSENEVNA<<<…"), each
  surname and given-name part is a PERSON span without the "<" fillers,
  also in documents imported from Word, RTF or ODT, inside lists, tables
  and quotes, and after a label. Other lines of the zone stay unmarked, and
  text with "<<" that is no MRZ gives nothing (part of #495).

- **Initials alone are proposed as a name.** One initial before a surname
  after a title or connector ("Judge K Ashdale"), and an all-caps given name
  before "X. SURNAME" in a signature line ("DORIAN P. WELK"), join the name.
  Bare initials that spell a person already found ("JB", «ЯВР», "IDP/ms")
  are PERSON in a table column headed by a person label, on a reference
  line, alone after a signature or after an "Initials:" label; acronyms
  and codes stay unmarked (part of #387).

- **More Russian addresses from court acts and register extracts are
  proposed whole.** Flat lists and a second house number ("ул. Сосновая,
  12 кв. 1, 2, 4", "ул. Луговая, 6, 9"), a house number with no "д." or
  no space after the comma ("ул. Еловая,16"), dotless "д", "к" and "кв",
  «им.» and initials in a street's name, a street's type after its name,
  capitals ("РОССИЯ 999999, ЭНСК, УЛ ЦЕНТРАЛЬНАЯ, Д. 14, К. 4, ОФ. 1"),
  districts and settlement types («ч.», «г.о.», «ст.», «пгт»), a house
  letter ("д. 43 Г"), «ГСП-4» in a court's header, a street with no house
  or a box after an index and a place, and the parts around a blank or a
  masked part are now one address, from the index or region to the last
  part, and an index with its places before a masked part is an address
  of its own. An address still stops before a name, a person's initials,
  a public body's name, the next field or a phone or date after a flat;
  one that a found name opens now starts after the name instead of being
  dropped, and a model's address that runs into a cadastral label ends
  before it ([ADR 0092](docs/adr/0092-recognition-correction-pipeline.md),
  #362).

- **More legal reference numbers are no tax, bank or identity values.**
  US codes written without dots or with "Section" ("42 USC Section
  1981"), Federal Register and Nevada statute citations, Civil Procedure
  Rule numbers ("CPR r71.8"), release numbers, agency letters and rulings
  (FDIC FIL, OTS, CEQ, T.D., Rev. Rul.), court running headers ("… Doc
  19-1 Filed …"), years, Russian federal-law numbers («127-фз»), codes
  named in full, letter, regulation and act numbers after «письмо»,
  «закон» or «положение», УПД and waybill numbers and dotted dates are no
  longer proposed as tax, bank or identity values. A court docket the
  model took for one of these is the document's own case number, unless
  it stands in a cited case. A real account, card, БИК, passport or
  phone next to a citation stays marked
  ([ADR 0092](docs/adr/0092-recognition-correction-pipeline.md), #480).

- **More dates of birth are proposed as dates.** A labelled birth date with
  a two-digit year ("DOB 3 Jun 61", "born 3 Sept. 61") and a date with its
  label after it ("his 3 June 1961 date of birth", «01.01.1980 года
  рождения», «14 марта 1988 г.р.») are DATE, with the label outside. Such a
  date repeats only before the same label, a name or address beside it
  stays its own entity, and other dates (issue, death, hearing) stay
  unmarked (part of #363).

- **Macs test on CI's exact iOS Simulator runtime.** `scripts/tools.env`
  pins the runtime's build (iOS 26.2, 23C54) beside its version, which
  `scripts/ios-toolchain.sh` downloads and `just doctor` and
  `just mobile --ios` check; asking for version 26.2 alone fetched the
  23C52 release candidate (#654).

- **The queue runner's test waits for its runners, not the clock.** Every
  wait in `scripts/test-queue-runner.sh` ends once its condition holds,
  within 15 minutes (`QUEUE_TEST_WAIT_S`), so a loaded PC no longer drops
  queue entries on its 5-minute limit; a runner that misses the limit fails
  once, with its log's tail, and the checks that depend on it are skipped
  (fixes #661). Its fake `tailscale` runs once before the runners, so a
  developer Mac's first-run check of a new program no longer reads as /ci
  not served (fixes #658).

- **The merge gate no longer drops a CI change whose group partner merged
  first.** When a queue commit's areas shrink from desktop and CI to CI
  alone (another PR in its group reached `main` first), the gate now
  counts `ci/scripts` as passed when that check never ran but `ci/macos`
  passed on the commit, since `ci/macos` runs the same script suites; the
  description says "scripts (via ci/macos)" (fixes #665,
  [ADR 0100](https://github.com/lawkitt/mdoc/blob/0aaf8f706e754d8b4c9ad905305b7bac194218a1/docs/adr/0100-ci-reliability-after-the-cutover.md)).

- **Short numbers and record numbers are no longer bank, phone or tax
  values.** Segment and pleading line numbers, bracketed short numbers,
  plot fragments, drawing dimensions, a code in a document path's file
  name, a six-digit postcode before a city, and Russian contract, project,
  notice, protocol, order and deposit-certificate numbers stay unmarked.
  Sort codes, SWIFT/BIC codes, IBANs, masked cards, labelled and dotted
  phones, and identity documents stay marked (part of #375).

- **The recognition fixture test fits its time limit again.** Dev and test
  builds optimize the regex crates, where the rule pack spent most of its
  time compiling and matching patterns unoptimized: the test over every
  recognition fixture takes about 3 s instead of 14 on the M4, so a full
  `just test` on the PC queue runner no longer kills it at 2 minutes and
  drops the queue entry (fixes #655).

- **More case numbers are proposed as case numbers.** Numbers after US
  labels ("Civil Action No.", "C.A.", "Civil Docket No.", "Cause No.",
  "Adv. Proc. No.", case-shaped "Docket No."/"Index No.", a Texas
  appellate caption, an EEOC charge) and Russian ones (УИД, an appeal
  proceeding, an administrative-offence protocol) are CASE, with the label
  outside. A repeat of the document's own number in another letter case is
  CASE too, and a date after «Дело №» no longer joins the number. A
  precedent's docket inside its citation stays unmarked (part of #364).

- **More US and UK address shapes are proposed whole.** A city after a
  suite with only a space ("Suite 305 Roseland, New Jersey 07068"),
  "City,ST ZIP" without a space, a ZIP after a state written out,
  compass-plus-number streets ("224 W 238 St."), house numbers in words,
  "#16" units, P.O. Boxes and «а/я» up to their postcode, a court's or
  public body's street address, and a corner with its city and ZIP are
  now one address. An address never takes a name: it stops at Attn, c/o
  or a person or company the model marked (part of #498).

- **Keep legal citation parts outside personal details.** English chapter
  and Roman-numeral section references, report and court abbreviations,
  numbered instruments, practice directions, manual pages and exhibit
  labels no longer appear as names, addresses or identifiers. Declaration,
  affidavit and deposition citations keep the person's name alone; a
  precedent's trailing Texas appeal number stays part of its citation.
  Real names, companies and the current case's number remain marked
  (#499).

- More public bodies are no longer proposed as people or organizations:
  agency offices, research bodies, statutory enterprises and regulators,
  official titles and defined pension-system short names. Fragments cut
  from court names, quoted Russian police units, wrapped ministry names
  and law titles are recognized in context. People, companies, universities,
  colleges, non-state funds and public bodies' contact details and numbers
  remain marked (#378, #484 item 4, #553 item 1).

- **Judgment party codes stay unmarked.** Defined English party labels
  such as C1 and D12, their ranges and possessive trustee phrases are no
  longer proposed as people or organizations. Real names beside the codes
  stay marked, and lowercase list letters come off names (fixes #546;
  company-category consistency remains tracked in #548).

- **Fewer amounts mistaken for identifiers.** Money and its decimal or
  kopecks parts, including joined cells in OCR tables and English amounts
  such as "$350 billion", are no longer proposed as tax numbers, bank
  details or phones. Labelled identifiers and twenty-digit accounts stay.

- **English and Russian roles and job titles are no longer proposed as
  names.** Procedural parties, delegation titles, job titles and Russian
  case forms are filtered when the whole span is a role. Complete job
  phrases come off a name's edges, and «ИП» stays outside an entrepreneur's
  name. A title or full name keeps a surname such as “Leader” or «Должник»
  ([ADR 0092](docs/adr/0092-recognition-correction-pipeline.md), #385,
  #484 item 5).

- **iOS CI stops downloading a Simulator runtime.** The pinned Xcode and
  iOS Simulator are now 26.2, which the hosted `macos-26` image ships, so
  `ci/ios` no longer fetches about 8 GB each run; building or testing the
  iOS app on a Mac needs Xcode 26.2 (fixes #630).

- **The merge gate no longer waits forever on a job GitHub lost.** A queue
  commit's hosted check that is missing once its Queue run has completed
  (a summary job never created), and a job cancelled after it started (an
  internal error, a lost runner), are now infrastructure failures: re-run
  up to twice, then failed as "did not run" or "cancelled"; a job that hit
  its own timeout still fails at once (fixes #635,
  [ADR 0100](https://github.com/lawkitt/mdoc/blob/0aaf8f706e754d8b4c9ad905305b7bac194218a1/docs/adr/0100-ci-reliability-after-the-cutover.md)).

- **iOS CI reuses main's built core.** `main` saves the built iOS core
  (`target/ios`), keyed on a hash of everything its build reads, and both
  `ci/ios` jobs restore it and skip `just ios-core` on an exact hit; a
  miss rebuilds it and fails if the build read a file the key does not hash
  (fixes #631).

- **A hosted Android job, merge-gate's Android fallback.** `ci-android.yml`
  (check `ci/android (hosted)`, 40 minutes) runs `just android` on
  `ubuntu-latest` with KVM; `merge-gate` dispatches it on a queue commit
  when the PC is silent, stale or erring, or the group holds a fork or bot
  PR, so a group that needs `ci/android` no longer fails with the PC off
  ([ADR 0095](docs/adr/0095-ci-on-a-merge-queue.md),
  [ADR 0100](https://github.com/lawkitt/mdoc/blob/0aaf8f706e754d8b4c9ad905305b7bac194218a1/docs/adr/0100-ci-reliability-after-the-cutover.md)).

- **Android scripts on macOS and Linux, and the minified build in CI.**
  `just android-setup`, `just android-emulator`, `just android-core` and
  `just android` run on an Apple Silicon Mac (an arm64-v8a emulator) and on
  Linux x86_64 with KVM as well as on Windows, each archive pinned per
  platform; on macOS and Linux the SDK goes in `mdoc-sdk`, apart from
  Android Studio's. The Android job also builds the minified release-like
  app (R8, `apps/android/app/proguard-rules.pro`) and opens its library on
  the emulator, so a missing keep rule fails it
  ([ADR 0091](https://github.com/lawkitt/mdoc/blob/0aaf8f706e754d8b4c9ad905305b7bac194218a1/docs/adr/0091-mobile-ci-and-releases.md)).

- **A stale snapshot is red on the PR, not in the merge queue.** A PR that
  changes a recognition fixture or the recognition code (`mdoc-detect`,
  `mdoc-pii`) runs the detection gate on its head (`detection-head /
  ci/detection`, 4 shards), and its `merge-gate` waits for it and names
  `just snapshots-fetch <PR>` when it fails; a core or desktop change alone
  still runs it only in the queue. Lanes no longer dispatch the gate by hand
  ([ADR 0100](https://github.com/lawkitt/mdoc/blob/0aaf8f706e754d8b4c9ad905305b7bac194218a1/docs/adr/0100-ci-reliability-after-the-cutover.md)).

- **`scripts/ci-stats.sh` measures CI against its targets.** A queue
  commit's time to a green gate, a PR head's time to the fast tier, and
  infrastructure failures a week, each against ADR 0100's target, over the
  last days or since a change landed
  ([ADR 0100](https://github.com/lawkitt/mdoc/blob/0aaf8f706e754d8b4c9ad905305b7bac194218a1/docs/adr/0100-ci-reliability-after-the-cutover.md)).

- **CI-only changes skip the desktop jobs.** A queue commit that touches only
  the CI machinery (the merge gate, the queue runner, the CI scripts and
  workflows, their suites) runs `ci/scripts`, the script suites on Linux and
  under `/bin/bash` 3.2, beside `lint (ubuntu)`, instead of `ci/macos`,
  `ci/windows` and the detection gate; a change to `ci-ios.yml` now runs
  `ci/ios` ([ADR 0100](https://github.com/lawkitt/mdoc/blob/0aaf8f706e754d8b4c9ad905305b7bac194218a1/docs/adr/0100-ci-reliability-after-the-cutover.md)).

- **The detection gate says how close it is to its budget.** A shard warns
  past 80% of its 5-minute model budget (on 3 shards each took 84%, so the
  queue keeps 4), manual runs default to 4 shards instead of 2, which went
  over the budget, and a stale snapshot's message
  shows both scores to five decimals and how far they moved
  ([ADR 0100](https://github.com/lawkitt/mdoc/blob/0aaf8f706e754d8b4c9ad905305b7bac194218a1/docs/adr/0100-ci-reliability-after-the-cutover.md)).

- **iOS CI builds the Release configuration.** `ci/ios` now passes only
  when `ci/ios (tests)` and the new `ci/ios-release` (`just ios-release`:
  the app unsigned in the Release configuration, as an archive compiles it)
  both pass, so a Release-only compile error fails the merge queue instead
  of the archive. The tests job downloads the Simulator runtime only when
  the image has none of the pinned major.minor, in the background while the
  core builds ([ADR 0100](https://github.com/lawkitt/mdoc/blob/0aaf8f706e754d8b4c9ad905305b7bac194218a1/docs/adr/0100-ci-reliability-after-the-cutover.md)).

- **The merge gate no longer fails on a missing Android fallback.** After
  the gate's reliability change, counting the manual runs of
  `ci-android.yml`, which does not exist yet, failed every pass on a queue
  commit that needs `ci/android`; a 404 there now means none
  ([ADR 0100](https://github.com/lawkitt/mdoc/blob/0aaf8f706e754d8b4c9ad905305b7bac194218a1/docs/adr/0100-ci-reliability-after-the-cutover.md)).

- **Faster desktop CI.** `ci/macos` runs as two parallel jobs, the checks
  and the test tiers, under a summary of that name; the fast tier on PR
  heads runs unit and scripts in parallel under `fast (ubuntu)`. A push to
  `main` runs lint and builds the tests without running them, including the
  iOS and Android targets of `just mobile --ffi`, so the merge queue
  restores them from cache; Linux package installs retry once within 5
  minutes ([ADR 0100](https://github.com/lawkitt/mdoc/blob/0aaf8f706e754d8b4c9ad905305b7bac194218a1/docs/adr/0100-ci-reliability-after-the-cutover.md)).

- **The merge gate survives GitHub errors and missing runners.** Every
  GitHub call in `scripts/merge-gate.sh` is time-limited and retried, and a
  pass that skips a PR or queue commit ends red and runs again 3 minutes
  later (`merge-gate-recheck.yml`); a hosted job that never got a runner is
  re-run up to twice before it fails as "no macOS runner"; a green PR that
  auto-merge leaves out of the queue gets a warning; the gate reads a queue
  commit's own area table; its job is now "merge-gate pass"
  ([ADR 0100](https://github.com/lawkitt/mdoc/blob/0aaf8f706e754d8b4c9ad905305b7bac194218a1/docs/adr/0100-ci-reliability-after-the-cutover.md)).

- **The PC's queue jobs explain their failures and keep a week of logs.** A
  failing Android job's status names the failing test from Gradle's JUnit
  results, a failed job's summary ends with the failing step's last lines,
  a passing Windows job drops its renders so 7 days of logs fit, the
  runner keeps the tailnet log page served and reports when it is not, and
  the Android CI job builds the core for the emulator's x86_64 only
  ([ADR 0100](https://github.com/lawkitt/mdoc/blob/0aaf8f706e754d8b4c9ad905305b7bac194218a1/docs/adr/0100-ci-reliability-after-the-cutover.md), #614).

- **An individual entrepreneur's name is a person, not an organization.**
  The name after "ИП" or "индивидуальный предприниматель" (any case form:
  "ИП Мохов Л. Р.", "индивидуального предпринимателя Мохова Льва
  Романовича") is now proposed as a person, with the status words outside
  it. A trading name in quotes after it ("магазин «Кедрач»") stays an
  organization, and placeholders ("ИП ФИО1") stay unmarked
  ([ADR 0092](docs/adr/0092-recognition-correction-pipeline.md), #562).

- **Defined terms are no organizations; real short names stay.** A generic
  word the document defines as a term ("(the “Distributor”)", "(the
  “Franchised Business”)", «(далее – Положение)», «именуемое в дальнейшем
  «Займодавец»») is no longer proposed as an organization, in any of its
  mentions or case forms, and neither is the abbreviation of a filing
  ("Def.’s Statement of Uncontested Material Fact (“DSUMF”)"). A coined or
  real short name ("Quillon", "Chase", "Fox") and a party's name with its
  legal form are still proposed (part of #390).

- **Names after name-part labels are proposed.** A value right after a
  label such as Surname, Given name, First name, Last name, Middle name,
  Patronymic, Фамилия, Имя or Отчество and a colon is proposed as a person
  when it looks like a name, also when two such pairs stand side by side
  on one line, as in an OCR'd two-column form ("Given name: Alvora
  Given name: Nerian"). A bare "Name:", a label in prose, placeholders,
  values with digits, code and link targets are left alone (#397;
  [ADR 0092](docs/adr/0092-recognition-correction-pipeline.md)).

- **Cited cases' parties are legal references.** Parties in a full citation
  ("Pellam v. Ostrand, 912 F.3d 455", "In re Draycott, 77 B.R. 310",
  "Ex parte Quill, 12 So. 3d 4") followed directly by a reporter, Westlaw,
  neutral or English report citation, or a docket with court and year,
  are no longer proposed. Reporter short cites ("Pellam, 912 F.3d at 460"),
  "Id." and "Ibid." are references too. A precedent's labelled case number
  is omitted. Names in the case's caption, beside roles or titles, or
  outside citations stay marked, including in earlier decisions. A
  caption's own docket with court and year stays, even after a precedent;
  "In re" captions count too. A sentence ending with a suffix or
  abbreviation keeps its name before the next citation
  ([ADR 0092](docs/adr/0092-recognition-correction-pipeline.md), #561).

- **More courts, public bodies and laws are no longer proposed as
  organizations or people.** Russian courts, ministries, administrations,
  state institutions and codes are dropped as people too, unless a title,
  an initial or a surname's ending makes a person ("Судов", "Суд Н. О.");
  the pension and social funds and their offices ("Пенсионного фонда …",
  "Фонда пенсионного и социального страхования", "УПФР", "ОСФР"), "УФК",
  "ЦБ РФ", "ВАС РФ", chambers and the President are dropped; and so is a
  span the model cut from a body's name ("Министерства ⟦финансов …⟧",
  "⟦Sheriff’⟧s Office", "⟦Bureau⟧ of …"). In English, Customs,
  constabularies, county jails, grand juries, divisions, agency short
  names ("NASA", "ICAO", "IMF", "NATO"), treaties and agreements,
  ordinances, rules of procedure, laws whose head is "Law", bills ("SB 9")
  and an abbreviation defined after "hereinafter" go; companies by their
  legal form ("Treaty Oak Capital LLC", "Customs Brokers Inc"),
  universities and colleges, a state one too, and non-state funds stay
  ([ADR 0092](docs/adr/0092-recognition-correction-pipeline.md), #378).

- **Names in table columns are proposed.** A cell under a Markdown table
  header that names a part of a person's name (Фамилия, Имя, Отчество,
  ФИО, Surname, Given name, Middle name, Full name and the like, or Name
  beside one of them) is proposed as a person when it looks like a name,
  so a short surname alone in its cell ("Кий", "Лю") is no longer missed.
  Placeholders, totals rows, role words and a lone "Name" column of files
  or companies are left alone (#397;
  [ADR 0092](docs/adr/0092-recognition-correction-pipeline.md)).

- **`***Bold italic***` shows no stray asterisk.** Three asterisks each
  side are one bold italic run, as a converter writes a bold italic
  heading.

- **Long company names with `L.L.C.` are recognized more often.**
  Detection gives this dotted legal form more room beside the company name,
  preserving the source spelling and punctuation. Synthetic reproductions
  cover the remaining company and union misses (part of #505).

- **Recognition reads through formatting inside names and addresses.**
  Adjacent emphasis runs and inline superscript or subscript tags no longer
  split a value imported from a PDF; replacements preserve the original
  source for exact restoration (fixes #552;
  [ADR 0092](docs/adr/0092-recognition-correction-pipeline.md)).

- **Placeholders and public-law companies are no longer proposed; cadastral,
  registration and personal register numbers are identifiers.** «ФИО1»,
  «ИП ФИО1», «Потерпевший №1», "John Doe", "Jane Does 1-25", "DOE
  SUPERVISORS I-X" and "ROE ENTITIES" stand for no one and are dropped,
  with a role before them in the span too; real initials after one
  («ИП ФИО3 А-Х.О.») are a person on their own, and a real name beside one
  stays. «ППК «Роскадастр»» and «публично-правовая компания «…»» are
  public bodies. Cadastral and right-registration numbers («кадастровый
  номер …», «номер регистрации права …»), an attorney's, notary's or
  arbitration manager's register or certificate number, and bar and
  shield numbers ("State Bar No.", "SBN", "Bar #") are proposed as
  identity values, the label left outside
  ([ADR 0092](docs/adr/0092-recognition-correction-pipeline.md), part of
  #484, part of #483).

- **English programmes, standards, abbreviations and software are no longer
  proposed as organizations.** A name ending in Program, Standard, Plan,
  Initiative, project, trial, Online, Manager, System or portal ("Bright
  Paths Program", "VR services", "Application Manager"), a medical
  condition ("Acute Lung Injury"), a treatise ("4 Brindle on Insolvency"),
  a legal form with no name ("a Wyoming limited liability company"),
  generic words alone ("Crew", "Operations"), common abbreviations ("VAT",
  "EFT", "F.O.B.", "OEM", "LEA", "IEP", "RNG"), an abbreviation defined
  beside any of these ("… Standard (NRAS)"), app platforms ("AppStore",
  "Google Play"), public portals ("USAJOBS", "CM/ECF", "Grants.gov") and a
  field code heading a table's column are dropped. Companies keep their
  names by their legal form or a word of their own ("Northvale Ledger
  Services LLC", "Acme Program Ltd", "Plan International")
  ([ADR 0092](docs/adr/0092-recognition-correction-pipeline.md), #501,
  #481, #488).

- **PDF captions and counsel blocks keep their names whole.** A court
  caption or a two-column counsel block in a PDF now imports with each
  column's lines kept together in one table cell: a name wrapped over two
  lines reads as one ("ACME HEALTH SYSTEMS, INC."), and two attorneys in
  one column stay apart, so recognition no longer splits a party or joins
  two people (pdf-inspector update, fixes #504).

- **Form numbers, codes and patent numbers are no identifiers.** Russian
  form numbers and classifier codes («Форма № Р11001», «Форма по ОКУД
  0401060», «ОКВЭД 47.11», «КБК …»), a tax form's barcode line, standards
  and building codes («ГОСТ Р 7.0.97-2016», «СП 2.13130.2020»), waybill and
  invoice numbers («ТН № 47», «ТТН № 112»), patent and trademark numbers and
  their schedules' rows ("Patent US 99/123,456", "AU  2009241803
  30-Apr-09"), form and industry codes ("Form 10-K", "STD001Y2K.UK", "NAICS
  541511"), comma-grouped numbers, a GUID, a link's tracking token, a page
  number ("-44-") and a toll-free prefix alone ("800") are no longer
  proposed as tax, bank, identity or phone values. Real accounts, БИК,
  ИНН, КПП, passport numbers, IBANs and whole toll-free numbers stay, and
  an account's label («р/с», «к/с», "IBAN") stays outside its span
  ([ADR 0092](docs/adr/0092-recognition-correction-pipeline.md), fixes
  #486).

- **English pronouns, headings, religions and nationalities are no longer
  proposed as people.** "He", "It", the "In" of "In re", a running
  header's "Judgment" or "Others", "Program", "General", "Federal",
  "Christian", "Islam", "Americans", "Maltese lawyer", "United States
  Citizens" and the "York" of "New York" are dropped, and "In re" comes off
  a caption's party. A person named so stays after a title ("Mr He",
  "Dr. General") or where the document gives the full name ("Dawn French",
  then "French"), and a company keeps any other word ("General Motors")
  ([ADR 0092](docs/adr/0092-recognition-correction-pipeline.md), #500,
  #492).

- **A surname split by a line-end hyphen joins its name.** Where a known
  person's name recurs with a word split over lines ("Верес-" then "това
  Дарья Ильинична"), the whole name is proposed as one span in that
  person's identity, not just the first name and patronymic; a capital
  after the hyphen ("Санкт-" then "Петербург"), a dash or a plain line
  break keeps the parts apart (fixes #388;
  [ADR 0092](docs/adr/0092-recognition-correction-pipeline.md)).

- **Web addresses are proposed.** An identifying URL (a profile, a personal
  or company site, a share link, or any address holding a name, email or ID)
  is proposed whole as a Web address, aliased `URL_1`, and wins over a name
  or email inside it. A Markdown link's target is replaced in place
  (`[the claim](URL_1)`); `http`/`https`, a leading `www.` and a trailing
  slash are one address. Courts', public bodies', laws' and legal
  databases' addresses stay, and `mailto:`/`tel:` stay Email and Phone.
  Settings › Detection › Categories to detect and the category menu list
  Web address after Email, on by default; saved mappings and cases load
  unchanged ([ADR 0094](https://github.com/lawkitt/mdoc/blob/0aaf8f706e754d8b4c9ad905305b7bac194218a1/docs/adr/0094-url-category.md)).

- **More US and UK addresses are proposed whole.** An address whose
  "City, ST ZIP" or "Town POSTCODE" line sits under its street ("1800 S
  RIO VERDE AVENUE" then "BRANWICK, CO 81999") is proposed, also when the
  sentence goes on after it (", and to …"), and so are
  suites, floors and buildings ("Bldg 499", "10th Floor", "Harrow House"),
  compass parts ("Ave. N.W."), a state written out ("Georgia 30999") and a
  street without a number before its postcode ("Strand, Slocombe, WC2A
  9ZZ"); the address ends at its postcode, not the country after it, and a
  building, floor or room part is no organization, bank account or
  identity document
  ([ADR 0092](docs/adr/0092-recognition-correction-pipeline.md), fixes
  #498).

- **English courts, public bodies, laws and place names are no longer
  proposed as organizations or people.** Departments, offices, agencies,
  bureaus, commissions, boards, Congress, circuits ("9th Cir."), agencies'
  and tribunals' short names ("HMRC", "EEOC", "EAT"), Acts, amendments,
  codes ("18 U.S.C.") and an abbreviation defined beside one ("… Privacy
  Act (“BIPA”)") are dropped, and so are a country, US state or major city
  alone, "State", "States" and "Federal" alone, and a place after "State
  of" or "Town of"; companies by their legal form or any other word in
  their name ("… LLC", "State Street …", "CMA CGM") stay whole
  ([ADR 0092](docs/adr/0092-recognition-correction-pipeline.md), #378,
  #478).

- **Legal citations are no identifiers or names.** Reporter, Westlaw and
  law-report citations ("424 U.S. at 38", "904 F.2d 275, 277", "2015 WL
  4554844", "[2020] STC 960"), codes and regulations ("26 U.S.C. § 6212",
  "48 CFR 52.212-1", "FAR 52.212-1"), sections, the record ("Tr. 435-36",
  "ECF Doc. #12-3", "n. 14", "123sa", "Id. at 38"), tax years ("2014/15"),
  and Russian law, article and court-act numbers ("№ 127-ФЗ", "ст. 428 ГК
  РФ", "Определение … № 305-ЭС21-1234") are no longer proposed as tax,
  bank, phone, identity or address values. "U.S." and the "S.C." of
  "U.S.C." are no names, "& Anor" and "& Ors" come off a party, and
  "Ostrand Decl." is the person Ostrand when the document names them. A
  real tax number, phone or case number beside a citation stays
  ([ADR 0092](docs/adr/0092-recognition-correction-pipeline.md), fixes
  #480, part of #499).

- **Links into legal databases are not read as text.** A link target of a
  scheme other than web, email or phone (`consultantplus:`, `garant:`,
  `file:`) is a reference into another program: a scan no longer reads its
  hashes and short numbers, so a ConsultantPlus `ref=` hash is not proposed
  as a tax or bank number. A name, an email or a long number (a phone, a tax
  number) in such a target still reads (#494;
  [ADR 0092](docs/adr/0092-recognition-correction-pipeline.md)).

- **Courts, public bodies, laws and place names are no longer proposed as
  organizations.** Russian courts, ministries, services and their
  territorial units, state institutions, and codes and laws are dropped in
  every case form and abbreviation ("Арбитражного суда … округа", "ГУ МВД
  России по …", "ГКУ города … «…»", "Гражданского кодекса Российской
  Федерации", "АПК РФ"), and so is a country, region or city alone or as
  a body's tail ("Именем Российской Федерации", "… области"); companies
  and banks, by their legal form or quoted name, stay
  ([ADR 0092](docs/adr/0092-recognition-correction-pipeline.md), fixes
  #378, #478).

- **Letter-spaced names and footnoted labels are read.** A scan reads a
  letter-spaced name or label, as OCR and spaced-out signature captions
  write them ("D E L V A   R O S K", "И Н Н:"), as its words, and reads a
  label past its footnote reference or note marker ("**ИНН**[^1]:",
  "ИНН¹:"), so the value after it keeps its category; initials,
  one-letter words and enumerations read as before, and the document
  itself is unchanged (fixes #400 and #448;
  [ADR 0092](docs/adr/0092-recognition-correction-pipeline.md)).

- **`just import`.** Prints what mdoc's own import makes of a file, so
  probes are judged on mdoc's text rather than another converter's
  ([tests/fixtures/recognition](tests/fixtures/recognition/README.md)).

- **Identifier columns keep their header.** A scan reads each value in a
  Markdown table column headed as an identifier (Паспорт, ИНН, Счёт,
  Телефон, Tax ID, IBAN, Phone and the like) with its header ("Паспорт:
  46 82 731609"), so a passport number alone in a cell is no longer
  proposed as a phone; name and other columns read as before, and the
  document itself is unchanged (fixes #397 in part;
  [ADR 0092](docs/adr/0092-recognition-correction-pipeline.md)).

- **Date masks, common abbreviations and products are no names.** A blank
  form's date mask ("ДД.ММ.ГГГГ", "ХХ.ХХ.ХХ") is no longer proposed as an
  organization, a person or a date; neither are common abbreviations
  ("ТКО", "ТТН", "АЗС", "МЧД", "КЭП", "ЭДО"), the Internet ("сети
  «Интернет»"), a product after the word for it ("ПО «Сетка»", "АСУ
  «Экспресс»"), a messenger or legal database ("Ватсап", "КонсультантПлюс")
  or a bare site domain. A company with its legal form keeps its name
  ("ООО «АЗС»", "АО «ТТН-Логистик»")
  ([ADR 0092](docs/adr/0092-recognition-correction-pipeline.md), fixes
  #487, #481, #488).

- **A phone on the line after its label keeps its plus.** In "Телефон:"
  followed by "+ 7 (900) 000-84-31", the "+" is the number's own sign,
  not a one-item list, so the whole phone is proposed; a "+" item in a
  real list, and any other list marker, stays Markdown. A value with a
  literal `#` ("LC#713624") can also be marked by hand (fixes #428;
  #423 in part;
  [ADR 0092](docs/adr/0092-recognition-correction-pipeline.md)).

- **Note markers and underscore emphasis hide no names.** A scan reads a
  superscript note marker after a name ("Avelina Durnell¹") as a word of
  its own and reads past underscore emphasis ("_Alva Ruskett_"), and a
  name followed by a note marker counts as a whole word (fixes #442 and
  #443; [ADR 0092](docs/adr/0092-recognition-correction-pipeline.md)).

- **More Russian addresses are proposed whole.** An address written flat
  first and city last ("кв. 9, д. 32, переулок Бумажных лодок, Тверь") is
  proposed, and so is a city without "г." before its street ("Псков, ул.
  Тихих залов, д. 29, пом. 8"); a city alone or a list of cities is still
  no address ([ADR 0092](docs/adr/0092-recognition-correction-pipeline.md),
  fixes #362).

- **Full-width identifiers are found.** A scan reads full-width digits and
  letters as ASCII, so a passport, INN or account number typed in
  full-width digits is proposed (fixes #405;
  [ADR 0092](docs/adr/0092-recognition-correction-pipeline.md)).

- **Dates of birth are proposed.** A date after born, DOB, date of birth,
  дата рождения, д.р. or род. is proposed as a Date; every other date
  (signing, deadlines, hearings) stays, even the same date elsewhere (#466).
  Settings › Detection › Categories to
  detect lists Dates of birth, on by default
  ([ADR 0092](docs/adr/0092-recognition-correction-pipeline.md), fixes #363).

- **A full name is proposed whole, not cut into parts.** Two initials beside
  a surname belong to it ("Ветлин Н. О.", "J. R. Quill"), and a known
  person's whole name in another wording is one proposal in their identity
  ("Лидией Игоревной Брюн" beside "Лидия Игоревна Брюн", "Лея Аркадьевна
  Ли-Вельт" beside "Лея Ли-Вельт"), not a first name and a surname apart;
  two people side by side keep their own
  ([ADR 0092](docs/adr/0092-recognition-correction-pipeline.md), fixes
  #391).

- **A title in «» is no longer proposed as a person.** In Russian text, a
  name in guillemets after a word for a work or a product («повести
  «Кедровый свет»», «проекта «…»») is read as a title; a quoted real name,
  a nickname or a company named after a person is still proposed
  ([ADR 0092](docs/adr/0092-recognition-correction-pipeline.md), part of
  #415).

- **Values over HTML line breaks are found.** A scan reads a `<br>` inside a
  line as the line break it shows, and a value over the lines of a table
  cell ("27 Wisp Lane<br>Norwich NR2 8ZZ") is proposed as one span; a tag
  at a value's edge or any other tag stays syntax (fixes #402 in part;
  [ADR 0092](docs/adr/0092-recognition-correction-pipeline.md)).

- **Home and office addresses are proposed whole.** An English address
  with a house number and street ("Flat 3, 41 Larch Road, Bristol BS6
  9ZY") or a Russian one with a street and house ("197101,
  Санкт-Петербург, Ольховый переулок, д. 41, кв. 3") is proposed from its
  first element to its postcode or city, and a city alone is no longer
  proposed as an address
  ([ADR 0092](docs/adr/0092-recognition-correction-pipeline.md), fixes
  #361, #362, #386).

- **Names in link targets are found.** A scan reads `%20` and percent-
  encoded letters in link and image targets as text and an underscore
  between letters there as a space, and a name it finds is also proposed
  where the document spells it so ("clients/Liora%20Nesket/",
  "Alice_Morgan.png"), as written (fixes #401 and #376;
  [ADR 0092](docs/adr/0092-recognition-correction-pipeline.md)).

- **Roles, category words and courts are not proposed as names.** Scans no
  longer propose "Owner", "Technical Reviewer", "Email", "Address", the
  defined term "Provider" or "Общество", "ГК РФ", or courts and public bodies
  ("County Court", "Ministry of Justice", "Районный суд") as people or
  organizations ([ADR 0092](docs/adr/0092-recognition-correction-pipeline.md),
  fixes #378, #379, #385, #390).
- **Surnames built on a role word are kept.** "Подрядчиков", "Супругов" or
  "Директоров" alone are no longer dropped as the roles подрядчик, супруг,
  директор (#450).

- **Values wrapped over lines are found.** A name split by a line-end
  hyphen ("Brin-" then "sett") reads as one word for the scan and the
  readiness check, an email wrapped after its "@" too, and a name, email,
  phone or account number wrapped over lines is proposed as one span; two
  lines' text, a paragraph break or a hard line break is never joined
  (fixes #396 and #419; #388 in part;
  [ADR 0092](docs/adr/0092-recognition-correction-pipeline.md)).

- **Bank accounts and staff IDs follow their label.** A value after "Bank
  account", "Account number", "Sort code" or "IBAN" is bank details even
  with a slash between sort code and number, and one after "Staff ID" or
  "Employee ID" is an identifier even when it is letters only, never a
  person ([ADR 0092](docs/adr/0092-recognition-correction-pipeline.md)).

- **Invisible characters split no names.** A scan and the readiness check
  read past soft hyphens and zero-width characters, so a name with a soft
  hyphen inside is proposed whole, and read a letter written with
  combining marks as one letter, as Unicode NFC writes it ("е" and a
  diaeresis is "ё"); the document itself is unchanged (fixes #420; #418 in
  part; [ADR 0092](docs/adr/0092-recognition-correction-pipeline.md)).

- **Whole email addresses with unusual local parts.** An address whose
  local part starts with `+`, is quoted or holds an apostrophe
  (`eira.o'venn@…`) is proposed whole instead of cut short
  ([ADR 0092](docs/adr/0092-recognition-correction-pipeline.md)).

- **Headings and teams are no organizations.** English words in sentence
  case that the detector took for an organization ("Distribution
  register", "Lasting power of attorney", "Accounts team") are no longer
  proposed; names in title case or with a legal form still are
  ([ADR 0092](docs/adr/0092-recognition-correction-pipeline.md)).

- **Organization and person spans end where the name does.** A Russian
  organization keeps its legal form and quotes ("ООО «Северный Вектор»",
  not "Северный Вектор"), an English one ends before the sentence's period
  ("Northbridge Legal Ltd"), and a title or post-nominal stays outside a
  person ("Ms Bennett" proposes "Bennett") (fixes #374, #382; the rule
  pack's `span` section, [ADR 0092](docs/adr/0092-recognition-correction-pipeline.md)).

- **A case number's digits stay out of amounts.** After "Номер дела: 7316",
  "7316 рублей" elsewhere is no longer proposed as the case number: a case
  number's repeats count only standing alone, never inside a longer number
  or identifier or as an amount
  ([ADR 0092](docs/adr/0092-recognition-correction-pipeline.md)).
- **Only court cases are case numbers.** An archive, personnel or registry
  file ("архивное дело 58239", "личное дело № 4521") is no longer proposed
  as a case number (fixes #451).

- **Fewer numbers taken for the wrong kind.** Long digit runs and bank codes
  are no phone numbers, a bare ten- or eleven-digit number needs a label to
  be bank details, a rate ("20 percent tax") or a writ series is no tax
  identifier, and an address needs a dotted domain to be an email
  ([ADR 0092](docs/adr/0092-recognition-correction-pipeline.md)).

- **Russian identifiers take their label's category.** A value after
  "ИНН" is a tax identifier and one after "СНИЛС" an identity number, even
  when its checksum fails or OCR misread a digit, and a table column headed
  by such a label counts as the label for its cells
  ([ADR 0092](docs/adr/0092-recognition-correction-pipeline.md)).

- **Your own Always and Never lists.** In a case, keeping a name
  everywhere in a document puts its wordings on the case's Never list, and
  marking a wording yourself puts it on Always, so the case's other documents skip or propose
  it on their next scan; undoing the Keep or the mark takes it off. Matching
  is whole-word and ignores case and spacing, and a person on Always also
  brings their other name forms. The case page lists both, each with
  Remove; Settings › Detection has user-wide lists you add to and remove
  from, used in every document. Both are kept encrypted with your saved
  cases, never in the settings file
  ([ADR 0092](docs/adr/0092-recognition-correction-pipeline.md)).

- **Character references hide no names.** A scan and the readiness check
  read HTML character references as their characters, so "Ann&#97;
  Petrova" is proposed as one name and "ismay&#64;crane-pith.invalid" as
  an email, and a leftover written with references still counts; the
  document itself is unchanged (fixes #367;
  [ADR 0092](docs/adr/0092-recognition-correction-pipeline.md)).

- **Values that look like Markdown are found.** An email in angle brackets
  (`<name@domain>`), a value with a literal `*` or `#` ("Fennel*Orbit Ltd",
  "ООО «Лист#8»") and a number at the start of a line after a label
  ("Паспорт:" then "46. 18 731904") are proposed instead of being hidden
  as source syntax; only whole HTML tags and real list markers stay
  protected, and a value after a list bullet is proposed without it
  (fixes #411; #423 and #428 in part;
  [ADR 0092](docs/adr/0092-recognition-correction-pipeline.md)).

- **Case numbers are proposed.** A claim or case number after its label
  (Claim No, Case number, Номер дела, дело №) is proposed as a Case number,
  aliased `CASE_1`, and restores like any alias; statute and article numbers
  stay. Settings › Detection › Categories to detect and the popup's category
  menu list Case number, on by default
  ([ADR 0092](docs/adr/0092-recognition-correction-pipeline.md), fixes #364).

- **Detection rules move into a rule pack.** The titles, roles, legal forms
  and date, phone and name shapes that decide what a scan keeps and what a
  manual selection is guessed to be now live in
  `crates/mdoc-detect/rules/{shared,en,ru}.toml`, checked by a unit test;
  detection behaves as before ([ADR 0092](docs/adr/0092-recognition-correction-pipeline.md)).

- **Word comments, deleted changes and properties stay out.** A DOCX's
  comments, deleted tracked changes and document properties never reach the
  Markdown (insertions read as accepted), and the import note says how many
  comments were not imported.

- **Restored text stays off other devices and clipboard histories.** Copy
  reply, Restore Clipboard, and copies from a Restore tab or the Reply view
  stay on this computer: macOS keeps them out of Universal Clipboard and marks
  them concealed and transient for clipboard managers; Windows keeps them out
  of clipboard history, cloud sync and clipboard monitors. Copy for AI is
  unchanged.

- **A person's other names join them.** When a full name is proposed,
  applied or marked, its forms elsewhere in the document (surname or first
  name alone, first name and patronymic, surname with initials, Russian case
  forms and common nicknames) are proposed under the same alias when only
  one person fits, so the Russian demo's Павлова gets one alias instead of
  six. Forms two people share keep an alias of their own, and a form that is
  also an ordinary word counts only capitalised mid-sentence
  ([ADR 0092](docs/adr/0092-recognition-correction-pipeline.md)).

- **Detection gate.** Recognition fixtures with `⟦CATEGORY|text⟧` gold
  and the model's raw output as snapshots
  ([tests/fixtures/recognition](tests/fixtures/recognition/README.md)),
  `just recognize` (`--gold`, `--snapshot`, `--scoreboard`), a snapshot
  test on every OS and a model run in `local/macos`, so a change that
  breaks a fixture fails CI; #301's spans are known-flaw fixtures
  ([ADR 0093](https://github.com/lawkitt/mdoc/blob/0aaf8f706e754d8b4c9ad905305b7bac194218a1/docs/adr/0093-flaw-hunt-loop.md)). The scan is split into a
  model stage and a correction stage with no change in what it proposes.

## [0.3.0] - 2026-11-01

- **mdoc's M in the menu bar (macOS).** The menu bar item shows the app
  icon's M monogram instead of a generic document symbol, tinted to suit a
  light or dark menu bar ([ADR 0084](https://github.com/lawkitt/mdoc/blob/0aaf8f706e754d8b4c9ad905305b7bac194218a1/docs/adr/0084-menu-bar-shelf.md)).

- **Android spike: detector settings.** The Android feasibility spike also
  times the name detector's window at 2, 4, 6 and 8 threads, on ONNX
  Runtime's CPU provider and on XNNPACK, for the floor decision
  ([ADR 0089](docs/adr/0089-android-app.md)). The detector's thread count
  can be set with `MDOC_DETECT_THREADS`; the apps keep 4.

- **A fuller tab menu.** The sidebar's tab menu adds Reveal in Finder (Show
  in Explorer on Windows) and Copy path for a tab with a file, and Close
  others, which asks each tab with unsaved changes or an unsaved mapping in
  turn, in sidebar order; Cancel keeps that tab and the rest
  ([ADR 0071](https://github.com/lawkitt/mdoc/blob/0aaf8f706e754d8b4c9ad905305b7bac194218a1/docs/adr/0071-one-close-and-quit-prompt.md)).

- **A long document scans like its parts.** Detection windows hold whole
  paragraphs and each heading starts one, so a section repeated in a longer
  document gets the proposals it gets alone, and a repeated section is
  scanned once. Titles, role words and the names of categories are no longer
  proposed ("Ms", "her husband", "the Employer", "email"), nor a claim number
  as a bank account. A wording found only inside a longer one no longer takes
  an alias, so aliases don't skip a number (ORG_1–4, ORG_6)
  ([#302](https://github.com/lawkitt/mdoc/issues/302),
  [ADR 0002](docs/adr/0002-reviewable-local-pseudonymization.md)).

- **Readiness catches a lone first name.** A word of a known multi-word
  person name left on its own ("Марина" of "Марина Сергеевна") now counts as
  a name not replaced on the shelf and in its export checks, and Share says
  how many names are left instead of "Names in the text are aliases now"
  (refs #301).

- **Small alignment fixes.** Home's "Open a case…" now ends in line with
  "Documents stay on this Mac" above it, and Home's first Tab stop is Open
  files…, with the header's bell after Home's own controls. In the AI reply
  box, Next unrecognized is a bordered button in the row with Copy reply and
  Open as tab. In the Reply view, the Reply label lines up with Close.

- **Narrow windows.** Drag the sidebar's edge to make it 160–320 pt wide;
  the width is saved. Below a 900 pt window width the sidebar collapses to
  the rail by itself and comes back when the window widens, unless you
  collapsed it. Long tab names lose their middle, not their extension, so
  names that share a start still differ
  ([ADR 0027](https://github.com/lawkitt/mdoc/blob/0aaf8f706e754d8b4c9ad905305b7bac194218a1/docs/adr/0027-chrome-style-vertical-sidebar.md)).

- **Background tabs are named as their source.** A file opened with others
  and not yet shown reads as its source in the sidebar and its tooltip, not
  as a `.md` that doesn't exist (fixes #311).

- **Context menus clear sidebar tooltips.** Opening a tab's or a case's
  context menu dismisses the sidebar's tooltip, and none shows while the
  menu stays open (fixes #310).

- **Android polish, round 2.** The Android app has mdoc's icon, with a
  themed variant, and its notification shows it. Paste text and Paste AI
  reply have a one-tap Paste button, a pasted text is titled from its first
  line, and the reply sheet names the case whose names come back, as on
  iOS. On a tablet, Home and the text keep a readable width
  ([ADR 0087](https://github.com/lawkitt/mdoc/blob/0aaf8f706e754d8b4c9ad905305b7bac194218a1/docs/adr/0087-ios-ui-polish.md),
  [ADR 0089](docs/adr/0089-android-app.md)).

- **Android polish, round 3.** In the Android app, tapping an identity
  opens its first mention, and a mention opened from anywhere scrolls into
  view above its sheet. A document without text yet says, in iOS's words,
  whether mdoc is reading it, the import stopped, or recognition is waiting
  or part done. Download while another job runs says so. The long-job
  notification says mdoc keeps working in other apps. A long press deletes
  a document from Home, replaces or keeps all of an identity, and a double
  tap zooms the original page. Back and More options
  have names for TalkBack
  ([ADR 0087](https://github.com/lawkitt/mdoc/blob/0aaf8f706e754d8b4c9ad905305b7bac194218a1/docs/adr/0087-ios-ui-polish.md),
  [ADR 0089](docs/adr/0089-android-app.md)).

- **Home › Recent badges and conversions.** The "pseudonymized" badge shows
  only on documents with applied replacements, and a converted source saved
  as `.md` folds into that file's row instead of a second entry (fixes #306).

- **Open a case names Save as case once.** The empty hint reads "Right-click
  a tab with applied replacements, or open Share, and choose Save as case",
  the label both places show (UI polish decision 9).

- **Settings states are easier to tell apart.** "Remove…" is a bordered
  button and is left out, not dimmed, while it can't run; every switch sits
  in one right-hand column and is grey when off; the selected Appearance and
  Resolution choice is filled, so a focused one no longer looks selected;
  About no longer ends with a Reset that has nothing to reset; and a long
  page shows a scroll thumb and fades where its rows continue
  ([ADR 0076](https://github.com/lawkitt/mdoc/blob/0aaf8f706e754d8b4c9ad905305b7bac194218a1/docs/adr/0076-settings-categories.md)).

- **Long counts keep the Replacements chevron.** An entity row's mention
  count has room for four digits, and the chevron beside it keeps its full
  width at any count (fixes #308).

- **Tooltips name their shortcut.** The sidebar's Collapse sidebar (⌃⌘S),
  Back and Forward (⌘[ ⌘]) and New tab (⌘N) and the toolbar's Pseudonymize
  (⇧⌘P) show their key; the popup's alias field says "Rename alias"; Home's
  "Documents stay on this Mac" explains itself above the note instead of over
  the buttons beside it
  ([ADR 0073](https://github.com/lawkitt/mdoc/blob/0aaf8f706e754d8b4c9ad905305b7bac194218a1/docs/adr/0073-keyboard-shortcuts-panel.md)).

- **A selected alias's ring hugs the alias.** After Apply, the ring around
  a selected alias follows its chip, so a short alias like ORG_1 no longer
  carries a ring the width of its original across the next words, and on a
  list item's wrapped rows the ring stays at the text. A selection across
  wrapped rows no longer sometimes highlights its whole last row
  (fixes #303).

- **Locked cases say how many are locked.** The notice reads "4 saved cases
  are locked…" when the cases folder can be counted, and Open a case… says
  they are locked above Unlock instead of "No saved cases yet". Opened from
  Home, the list sits below its link, and its empty hint also names
  Share › Save as Case (fixes #307).

- **Native panels follow the app's theme (macOS).** With Appearance on Light
  or Dark, the Save and Open panels, their alerts and the menus use it too,
  even when macOS is set the other way; System follows macOS
  ([ADR 0076](https://github.com/lawkitt/mdoc/blob/0aaf8f706e754d8b4c9ad905305b7bac194218a1/docs/adr/0076-settings-categories.md)).

- **The DOCX source preview draws ₽.** On macOS, where the preview draws a
  missing Calibri or Cambria in Arial, the ruble sign and the newer currency
  signs (₸ ₹ ₺ ₼ ₾) were empty boxes; they now come from PT Sans and Rockwell,
  which ship with macOS (fixes #305).

- **Fuller menus and copy keys.** File › Open Recent lists up to 10 recent
  files with Clear Menu; Edit › Restore Clipboard restores the active tab's
  aliases in copied text; View gains Home, Search, Edit, Original,
  Pseudonymized, Side by Side and Show Replacements; Help gains mdoc Help;
  Check for Updates… sits in the mdoc menu (Help on Windows). Copy for AI is
  ⇧⌘C (Ctrl+Shift+C) and Copy Markdown ⇧⌘M (Ctrl+Shift+M). ⌘\ now switches
  to Side by side from Original or Pseudonymized
  ([ADR 0058](https://github.com/lawkitt/mdoc/blob/0aaf8f706e754d8b4c9ad905305b7bac194218a1/docs/adr/0058-standard-macos-menus.md)).

- **Android polish.** The Android app's bottom bars sit clear of the
  gesture bar, and its button bars stack at large text instead of wrapping,
  with full names for TalkBack. Home rows show their status as a symbol, as
  on iOS, and the Add menu has icons. Counts read "1 mention" and "1 value
  restored". Restored replies copied to the clipboard are marked sensitive,
  so Android's clipboard preview hides them
  ([#321](https://github.com/lawkitt/mdoc/issues/321)). Share as Markdown
  file moves to the document's menu. An import interrupted off screen says
  so in the same words as on iOS. The candidate sheet's alias is readable
  in dark mode, and the Identities sheet shows each category's colour
  ([ADR 0087](https://github.com/lawkitt/mdoc/blob/0aaf8f706e754d8b4c9ad905305b7bac194218a1/docs/adr/0087-ios-ui-polish.md),
  [ADR 0089](docs/adr/0089-android-app.md)).

- **The shelf's count matches the editor's.** A shelf row's "N replaced"
  counts mentions, the same number as the Share tab; hovering it lists the
  names those mentions are ("14 names: 8 people · 3 organizations · …")
  (fixes #304).

- **A running scan says so.** While a scan runs the Replacements footer says
  "Scanning…" instead of "Nothing to replace yet", Rescan is greyed out, and
  a long document counts its parts in the toolbar ("Finding names… 3 of 12")
  (fixes #309).

- **Dialogs follow the platform.** Confirm dialogs lay out their buttons as
  macOS and Windows do (on macOS Don't Save at the left, Cancel and the
  default at the right), fill the default button, and dim the window behind
  them in Dark too. "Discard" is now "Don't Save". The Quit summary names up
  to five of the documents it asks about, then "and N more"
  ([ADR 0064](https://github.com/lawkitt/mdoc/blob/0aaf8f706e754d8b4c9ad905305b7bac194218a1/docs/adr/0064-dialog-and-role-semantics.md),
  [ADR 0071](https://github.com/lawkitt/mdoc/blob/0aaf8f706e754d8b4c9ad905305b7bac194218a1/docs/adr/0071-one-close-and-quit-prompt.md)).

- **iOS keeps restored replies on the phone.** Copy reply and Copy in
  Restore with mdoc put the restored text, which holds originals, on this
  device's clipboard only (no Universal Clipboard) for ten minutes; Copy for
  AI is unchanged. Writing Tools no longer acts on the restored reply
  (fixes #321).

- **One name per action.** Paste AI Reply replaces Restore AI reply, Paste AI
  Response and Paste AI reply…; Save as Case replaces Save mapping as case;
  Side by Side replaces Before / After; the palette's theme row says which
  theme it switches to ("Switch to dark theme"). UI text uses US spelling
  ("unrecognized", "App license", "organizations").

- **Android device tests.** `just android-device-test` runs the Android
  app's tests and a long-job test on a phone by its adb serial or on
  Firebase Test Lab phones: the model download, text recognition and Find
  names go on while mdoc is off screen and resume after Android kills its
  process, with no crash or ANR
  ([ADR 0089](docs/adr/0089-android-app.md)).

- **Paste text fills its new tab.** Home's Paste text and the palette's
  "Paste text as new tab" put the clipboard text straight into the new tab
  instead of leaving an empty Untitled tab (fixes #300).

- **Android app scaffold.** `apps/android` is a Jetpack Compose app on the
  Rust core's Kotlin bindings: Home lists stored documents, the case key is
  wrapped by the Android Keystore, nothing goes into backups or device
  transfers, and parity tests check the core against the desktop goldens on
  the JVM. Windows local CI gains the Android job (`just android`)
  ([ADR 0089](docs/adr/0089-android-app.md)).

- **Android: model setup.** A setup card names the download size and
  downloads text recognition (and the name detector, on phones that can run
  it) once, on Wi-Fi; on cellular mdoc asks first, and a download held for
  Wi-Fi says "Waiting for Wi-Fi" with Use cellular. A document that needs a
  model offers "Download & continue" ([ADR 0089](docs/adr/0089-android-app.md)).

- **Android: the Original page.** A PDF's pages or a photo show beside the
  text on a wide screen, or under an Original tab on a phone, zoomable with
  two fingers ([ADR 0089](docs/adr/0089-android-app.md)).

- **Android: Restore with mdoc, Open in mdoc and Settings.** An AI app's
  reply shared to "Restore with mdoc" comes back restored with the case of
  the document shared last (another case from a picker), then Copy, Share or
  Save to document; PDFs, Word files and images shared or opened from other
  apps open in mdoc; Settings shows the models, where documents live and the
  licence ([ADR 0089](docs/adr/0089-android-app.md)).

- **Android: the review flow.** Add a document by Paste, Files, Photos or
  Scan (the system camera) into a new or existing case; read it as a long
  job in a foreground service; review candidates, mark missed names with
  Replace…, see Identities; Share or Copy for AI; paste the AI's reply to
  restore it ([ADR 0089](docs/adr/0089-android-app.md)).

- **Android UI survey.** `just android-survey` takes every screen of the
  Android app in light and dark, and at the largest font, on the phone and
  tablet emulators; the first set is in
  [docs/evidence/android-app](https://github.com/lawkitt/mdoc/blob/1e7096412fda2d27ec5a274e5b53ba00c3b101f1/docs/evidence/android-app/2026-10-31-ui-survey/README.md).
  It found an unlabelled + button and a tablet crash, both fixed
  ([ADR 0089](docs/adr/0089-android-app.md)).

- **Android feasibility spike.** `tools/android-spike` runs the core's spike
  on Android phones in Firebase Test Lab, downloading the models through the
  core's setup as the app will; the first rows are in the
  [report](https://github.com/lawkitt/mdoc/blob/0aaf8f706e754d8b4c9ad905305b7bac194218a1/docs/evidence/mobile/2026-10-31/README.md#android-adr-0089)
  ([ADR 0089](docs/adr/0089-android-app.md)).

- **Android toolchain on Windows.** `just android-setup` installs the pinned
  Android SDK, emulator, JDK 21 and NDK; `just android-emulator` boots an
  API 36 emulator with 16 KB pages; and `just android-core` builds the mobile
  core with ONNX Runtime and PDFium for Android phones and the emulator
  ([ADR 0089](docs/adr/0089-android-app.md)).

- **Mobile core on Android.** The core opens the ONNX Runtime and PDFium
  that ship inside the Android app, reads the process's memory there for the
  feasibility spike, and leaves out the desktop's DOCX preview worker
  ([ADR 0089](docs/adr/0089-android-app.md)).

- **iOS: clearer text, and layouts that hold at the largest text sizes.**
  A category's name and an alias are no longer drawn in the category's
  colour, which failed contrast; the colour stays on the icons. Identities'
  state chips read in the text colour, and its Done button has a full-size
  target. At accessibility text sizes the model rows in Settings and setup,
  the candidate sheet's category and Identities' rows stack instead of
  breaking words over several lines, and the reply sheet's line about the
  case is no longer cut short. Opt-in audits of the main screens help find
  more.

- **iOS device UI tests below the floor.** `just ios-device-test <udid>`
  picks the test by the phone's tier: below the 8 GB detector floor (the
  iPhone 12 Pro) it checks the text-recognition download, a scan recognized
  while the app is left, a .docx import that is whole after it, the note
  where Find names would be and in Settings, and marking a name by selecting
  it; `below-floor` or `detector` picks one by hand
  ([ADR 0088](https://github.com/lawkitt/mdoc/blob/0aaf8f706e754d8b4c9ad905305b7bac194218a1/docs/adr/0088-ios-on-real-devices.md)).

- **iOS: Restore with mdoc leads with Copy.** The restored reply's Copy is
  the prominent button, as most replies go back into a chat or a document;
  Share and Save to document stay beside it.

- **iOS: smaller polish from the second UI survey.** Paste AI reply shows
  a hint in its empty editor; Replace… for a selection is titled by the
  category it becomes, not "Replace" twice; the share sheet's preview has a
  document icon in place of a blank one; and on iPad the setup card shows
  once, in the document that asked for it, not in the sidebar too.

- **iOS device UI tests.** `just ios-device-test <udid>` runs the review
  flow and a long-jobs test on a connected iPhone, through the real
  background task the Simulator refuses: a camera scan downloads text
  recognition on Wi-Fi and is recognized while the app is left, Find names
  runs the detector and keeps running in the background on a 37-page .docx,
  and a text PDF comes in. The recipe copies the fixtures into the app's
  container with `devicectl` and fails on a new crash report
  ([ADR 0088](https://github.com/lawkitt/mdoc/blob/0aaf8f706e754d8b4c9ad905305b7bac194218a1/docs/adr/0088-ios-on-real-devices.md)).

- **Menu bar shelf on by default, and polished (macOS).** "Show mdoc in the
  menu bar" is on unless turned off; the first time the last window closes,
  a note under the icon says mdoc is still there, with **Quit when windows
  close** to turn it off. Previews show the first real prose with Markdown
  hidden in two lines; titles drop `.md` and show their case as a tinted
  label; Copy for AI is a button that shows "Copied" (also ⌘C and ⌘1–⌘8);
  "N replaced" lists its categories on hover; the clipboard actions sit in a
  Clipboard section; the footer is one row (Open mdoc, Settings, Quit); the
  shelf has a blurred background; a drag shows a card with the title and
  first line, and a hint that goes after the first drop
  ([ADR 0084](https://github.com/lawkitt/mdoc/blob/0aaf8f706e754d8b4c9ad905305b7bac194218a1/docs/adr/0084-menu-bar-shelf.md)).

- **iOS device fixes.** Below the detector floor (8 GB) the app no longer
  runs a rules-only scan: where Find names would be, the document says the
  device cannot find names and that selecting text marks one, and Settings
  says the same. A model download asks first on any cellular interface,
  shows "Waiting for Wi-Fi" with Use cellular while it is held, an
  interrupted import reads "Import not finished" with Finish import, and the
  + menu reads Paste, Files, Photos, Scan
  ([ADR 0088](https://github.com/lawkitt/mdoc/blob/0aaf8f706e754d8b4c9ad905305b7bac194218a1/docs/adr/0088-ios-on-real-devices.md)).

- **iOS: the name detector needs 8 GB of RAM.** The iOS app runs the name
  detector (GLiNER2) only on iPhones and iPads with 8 GB or more: iPhone 15
  Pro and every iPhone from the 16 on. On real phones the iPhone 17 Pro met
  every detector budget (a 20-page scan in 35–43 s, peak memory 1.96–1.97
  GiB, load under 1 s), while the 6 GB iPhone 12 Pro took about two minutes
  for a 20-page scan against a 60-second budget
  ([evidence](https://github.com/lawkitt/mdoc/blob/0aaf8f706e754d8b4c9ad905305b7bac194218a1/docs/evidence/mobile/2026-10-31/README.md)).

- **iOS: fewer steps to share and restore.** Paste takes the text, title and
  case in one sheet; Share opens the share sheet in one tap (a long press
  shares a Markdown file) with the document's title as the preview; a
  restored reply is styled like the review text, in the app and in Restore
  with mdoc, and on the Reply tab the bar shares and copies the reply. A scan
  that finds nothing says so ([ADR 0087](https://github.com/lawkitt/mdoc/blob/0aaf8f706e754d8b4c9ad905305b7bac194218a1/docs/adr/0087-ios-ui-polish.md),
  [UI survey](https://github.com/lawkitt/mdoc/blob/1e7096412fda2d27ec5a274e5b53ba00c3b101f1/docs/evidence/ios-app/2026-10-31-ui-survey/README.md)).

- **iOS: long jobs no longer crash the app on a device.** Downloading a
  model, importing a document or finding names starts a background task,
  whose handlers stopped the app on a real iPhone (Swift's main-actor
  check on the scheduler's queue).

- **iOS: calmer review text and Home.** Blank lines take a third of a line,
  tables use a smaller monospaced font, and highlights are stronger in dark
  mode. Home lists documents newest first with their case, and status is a
  symbol and words, never a colour. The document's setup card has Not now,
  Identities has Done and labelled states, and Settings names models by
  what they do ([ADR 0087](https://github.com/lawkitt/mdoc/blob/0aaf8f706e754d8b4c9ad905305b7bac194218a1/docs/adr/0087-ios-ui-polish.md)).

- **iOS: polish defects fixed, and an app icon.** The candidate sheet loses
  its empty band and is titled by category; no value is hyphenated at any
  text size; Share, Copy and Paste reply stay on one line or stack; the setup
  card shows as a card in light mode and promises only what the rules-only
  tier finds. `just icon` now also makes the iOS icon, and `just ios-survey`
  takes light and dark screenshots of every screen
  ([ADR 0087](https://github.com/lawkitt/mdoc/blob/0aaf8f706e754d8b4c9ad905305b7bac194218a1/docs/adr/0087-ios-ui-polish.md)).

- **Backslash escapes dim like other markers.** With the caret on a
  converted list number such as `1\.`, the editor now dims the backslash as
  it dims `**` or `#`; away from the caret it still shows `1.` (ADR 0087).

- **Menu bar shelf (macOS).** Settings › General › "Show mdoc in the menu
  bar" adds a menu bar item listing pseudonymized documents to open, drag
  into another app or Copy for AI, with Pseudonymize Clipboard and Restore
  Clipboard for the current case. Only reviewed documents leave the shelf;
  with it on, mdoc keeps running after its window closes
  ([ADR 0084](https://github.com/lawkitt/mdoc/blob/0aaf8f706e754d8b4c9ad905305b7bac194218a1/docs/adr/0084-menu-bar-shelf.md)).

- **The core builds for iOS.** `mdoc-import`, `mdoc-detect` and `mdoc-store`
  build for iPhone and the Simulator, and the new `crates/mdoc-ffi` exposes
  them to Swift through UniFFI; `just ios-core` assembles
  `MdocCore.xcframework` with ONNX Runtime linked in and PDFium as an embedded
  framework. A throwaway harness measured GLiNER2 and PP-OCR against the
  mobile detector budgets; phone rows await the maintainer
  ([ADR 0085](docs/adr/0085-mobile-apps.md),
  [report](https://github.com/lawkitt/mdoc/blob/0aaf8f706e754d8b4c9ad905305b7bac194218a1/docs/evidence/mobile/2026-10-31/README.md)). Desktop is unchanged.

- **Licence permission for app stores.** mdoc stays GPL-3.0-or-later and adds
  a section 7 permission to ship through app stores
  ([LICENSE-APP-STORES.md](LICENSE-APP-STORES.md),
  [ADR 0086](docs/adr/0086-app-store-licence-permission.md)).

- **Quiet macOS builds.** The vendored `gpui-pre-macos` crate no longer prints
  716 upstream `deprecated` warnings on every build; see
  [its patch notes](vendor/gpui-pre-macos/MDOC-PATCH.md).

- **Quitting from the Dock asks first on macOS.** Quit from the Dock, a quit
  Apple Event or logout now shows the same unsaved-changes and mapping
  prompts as ⌘Q, and a logout or shutdown goes on once they are answered.
  It used to close mdoc at once, losing unsaved edits, unsaved mappings,
  untitled tabs and OCR results
  ([ADR 0071](https://github.com/lawkitt/mdoc/blob/0aaf8f706e754d8b4c9ad905305b7bac194218a1/docs/adr/0071-one-close-and-quit-prompt.md)).

- **UI polish from a dev-build walk-through.** A click in the blank page
  under a document's last line puts the caret at the end instead of taking
  focus from the editor. Numbered list items share one text edge, mention
  snippets in Replacements read without gaps, and the tooltips of More
  actions, What you did, notices and the Replacements filter no longer sit on
  their open menus. Sidebar tab names line up whatever the badge, and a
  restored conversion keeps its name when first opened. The source preview's
  Find and Fit page buttons use line icons, a failed conversion offers Try
  again, the OCR result line names the model as Settings does, and smaller
  alignment and wording fixes land in Home, Share, Settings and Compare.

- **Windows checkouts on a Dev Drive.** On the Windows PC the root checkout,
  lanes `windows-lane-1` to `windows-lane-4` and local CI's home (`E:\ci`) are
  on `E:`, a ReFS Dev Drive, instead of `C:` and `D:`. Re-running
  `scripts/local-ci-setup.ps1` with a new `-Root` now also ends the results
  server left over from the old home, which otherwise kept answering on the
  same port (fleet ADR 0019).

- **⌃↩ opens a sidebar tab's menu.** Ctrl+Enter on a focused sidebar tab
  opens its menu in the expanded sidebar and the collapsed rail, with no fn
  on MacBook keyboards, where F10 is the mute key; ⇧F10 still works, and the
  Menu key on Windows. The shortcuts panel lists it
  ([ADR 0062](https://github.com/lawkitt/mdoc/blob/0aaf8f706e754d8b4c9ad905305b7bac194218a1/docs/adr/0062-keyboard-reach.md)).

- **Replace picks a category.** The pill beside a selection reads
  "Replace · Person ▾": Replace adds with the guessed category, as ⌘⌥P
  does, and ▾ (or ⇧⌘⌥P, Ctrl+Alt+Shift+P on Windows) lists the categories
  with the alias each would give, adding in the one picked
  ([ADR 0024](https://github.com/lawkitt/mdoc/blob/0aaf8f706e754d8b4c9ad905305b7bac194218a1/docs/adr/0024-replace-custom-selection.md)).

- **Local CI is the PR test gate.** Hosted CI runs only `lint (ubuntu)` on
  PRs from branches in the repository; `local/windows` and `local/macos`
  test them on the maintainer's machines. Fork and Dependabot PRs keep the
  full hosted matrix, and `ci.yml` can be run by hand on any branch
  ([ADR 0082](https://github.com/lawkitt/mdoc/blob/0aaf8f706e754d8b4c9ad905305b7bac194218a1/docs/adr/0082-local-ci.md); fixes #252).

- **Save As starts in a folder you use.** Save As for an Untitled tab, pasted
  text or an AI reply starts in the folder of your most recent file, or in
  Documents, instead of mdoc's install folder; a conversion still starts in
  its source's folder (fixes #180).

- **The category menu says what each choice gives.** In the mention bar's
  category menu each row shows the alias choosing that category would
  give, and none where the alias stays (the current category, or a custom
  alias of the whole entity), instead of the current alias on every row
  (fixes #179).

- **Save As says why it opens.** Save mapping as case on an unsaved conversion
  opens Save As with "Save the document first so the case can find it": the
  dialog's message line on macOS, its title on Windows.

- **Windows ships the `.exe` installer only.** Releases no longer include an
  `.msi`, and `just setup` no longer installs WiX. If you installed mdoc from
  an `.msi`, remove it in Settings › Apps before running the `.exe`; your
  cases are kept, because the `.msi` never touches `%LOCALAPPDATA%`
  ([ADR 0083](https://github.com/lawkitt/mdoc/blob/0aaf8f706e754d8b4c9ad905305b7bac194218a1/docs/adr/0083-fewer-release-candidates.md)).

- **F6 lands on the active tab.** F6 into the sidebar now lands on the
  active tab's row (Home's row on Home), in the expanded sidebar and the
  collapsed rail, instead of the control last focused there, often Settings
  ([ADR 0062](https://github.com/lawkitt/mdoc/blob/0aaf8f706e754d8b4c9ad905305b7bac194218a1/docs/adr/0062-keyboard-reach.md)).

- **The clipboard notice goes away by itself.** The result of Pseudonymize
  Clipboard leaves the document after 8 seconds, or sooner when you edit the
  document, copy from it or dismiss it (fixes #181).

- **Faster local CI on Windows.** `local/windows` keeps the CI checkout's
  build cache between jobs (each job had built cold), runs at normal
  priority, builds GPUI's layout and paint crates at `opt-level` 2, and
  `just test` runs the script suites beside the Rust tests (fixes #240,
  [ADR 0082](https://github.com/lawkitt/mdoc/blob/0aaf8f706e754d8b4c9ad905305b7bac194218a1/docs/adr/0082-local-ci.md)).

- **Local CI on the Windows PC.** The `local/windows` runner is installed and
  posts statuses. `scripts/local-ci-setup.ps1` now sends the task's output to
  `D:\ci\state\task.log`, because writes to the task's console failed and
  stopped the runner at start-up. It serves results through a localhost
  Python server (the `mdoc local CI web` task) that `tailscale serve`
  proxies, because serving a folder needs a Windows admin. A third lane,
  `windows-lane-3`, is on `D:` (fixes #233,
  [ADR 0082](https://github.com/lawkitt/mdoc/blob/0aaf8f706e754d8b4c9ad905305b7bac194218a1/docs/adr/0082-local-ci.md)).

- **Local CI runner.** `just local-ci` tests each PR's merge with `main` on
  the maintainer's Mac and Windows PC and posts `local/macos` and
  `local/windows` statuses that link to the log and renders over the tailnet
  ([ADR 0082](https://github.com/lawkitt/mdoc/blob/0aaf8f706e754d8b4c9ad905305b7bac194218a1/docs/adr/0082-local-ci.md), #233).

- **Hosted CI hygiene.** CI jobs time out after 15 (lint), 20 (clippy) and
  30 (test) minutes, `just test` runs the workspace tests with cargo-nextest
  (per-test timeouts, doctests through `cargo test --doc`), `main` deletes
  older generations of its cargo caches, renders artifacts are kept 7 days,
  and PRs on any base branch get CI
  ([ADR 0082](https://github.com/lawkitt/mdoc/blob/0aaf8f706e754d8b4c9ad905305b7bac194218a1/docs/adr/0082-local-ci.md); fixes #231).

- **Faster macOS screen renders, fewer render-file conflicts.** The headless
  render test takes 72 s instead of 234 s on the M4: `gpui-pre-apple`,
  `image` and `fdeflate` build at `opt-level` 2 in the dev profile (cold test
  build +3%), and PNGs are encoded on a background thread as on Windows.
  Scenes live in one module per feature and the test's screen list in
  `tests/headless_screens.txt`, merged by union (fixes #232).

- **Case identity fixes.** Saving a new case no longer retains forgotten names in dismissed merge hints, and merging identities no longer blocks alias updates in other open case members.

- **Restore in place fixes.** Joining a case, a case-wide rename and their
  Undo keep an AI reply restoring to the same people: its aliases follow the
  renumbering, in the case file too. The reply box keeps a paste exactly
  (no Markdown paste rules) and keeps the caret in view in a long reply;
  Next unrecognised scrolls to its token on the click that opens the Reply
  view ([ADR 0078](https://github.com/lawkitt/mdoc/blob/0aaf8f706e754d8b4c9ad905305b7bac194218a1/docs/adr/0078-restore-in-place.md)).

- **What you did fixes.** Re-choosing the current category of an identity
  whose mentions are already replaced no longer adds an entry or drops the
  undone ones; undoing a case join that only proposed names withdraws those
  proposals, so Apply no longer replaces them; Tab and ⇧Tab through a long
  list scroll the focused entry into view.

- **Back and Forward fixes.** Closing the tab you're on or deleting a case from
  its page no longer leaves Back pointing at the page shown, and keeps Forward;
  a close or quit prompt you cancel no longer clears Forward; the buttons and
  shortcuts agree on where they go; navigating closes an open tab or case menu
  ([ADR 0081](https://github.com/lawkitt/mdoc/blob/0aaf8f706e754d8b4c9ad905305b7bac194218a1/docs/adr/0081-workspace-back-forward.md)).

- **Model status stays accurate across windows.** Settings refreshes remembered model state, shows clearer setup progress, preserves runtime errors, and avoids storing absolute paths or detailed errors in the cache.

- **Restore AI replies in place.** Paste an AI reply into Share to read it
  with real names in the live Reply view, see unrecognised placeholders,
  copy the restored reply or open it as a tab. Each case member's reply is
  kept encrypted with its mapping. The box replaces Share's "Paste AI reply"
  button; Paste AI Response in the File menu and ⌥⇧⌘V still open a restore
  tab.

- **Default categories to detect.** Settings › Detection › Categories to
  detect chooses what scans of documents without a case look for, and each
  new case starts from that list; existing cases keep theirs. Turning a
  category off removes its unapplied proposals in open documents without a
  case; applied replacements and names you marked stay
  ([ADR 0076](https://github.com/lawkitt/mdoc/blob/0aaf8f706e754d8b4c9ad905305b7bac194218a1/docs/adr/0076-settings-categories.md)).

- **Merge hint fixes.** Restored replies use an identity’s longest wording
  after Same as… links a fuller name to a saved short one; merging into a later
  case identity no longer leaves two case entries with one alias; relatives
  (“Mr” / “Ms Bennett”, “Петрову” / “Петрова”, a relative named by initials)
  and different numbers (“1.11.2020” / “11.1.2020”) get no hint; hints
  dismissed before Save mapping as case stay dismissed; a failed merge changes
  nothing.

- **What you did.** The document header's clock and the command palette open
  a list of pseudonymization decisions with times. Choose an entry to undo
  back to it or redo up to it; intervening text edits travel with them.
  Decisions older than the undo limit stay listed but can no longer be
  travelled to. The list stays in memory for the tab and is never saved to
  disk or a case.

- **Merge likely duplicate identities.** The Replacements panel offers “Looks like … · Merge” for unambiguous short or inflected names, punctuation and case variants, long-name typos and reversed Russian names. Merge keeps the fuller identity’s alias and takes one Undo; dismissed pairs stay hidden for the tab or its saved case.

- **Settings remembers model checks.** Model rows show the last result at once and re-check quietly; changed or missing files lose their Installed status. Recognition explains its default, and the OCR card says language picks apply only to that document.

- **Navigate back and forward.** Use the title bar buttons, keyboard shortcuts, or mouse buttons to return to recently visited tabs, Home, and case pages.

- **A page for each case.** Open a case to see its documents and names without
  opening files or reading the case key. Rename it, choose one of eight colours,
  manage membership and detection categories, add or forget names, and delete
  the case from one place. Locked cases show their name and document count until
  you choose Unlock ([ADR 0077](https://github.com/lawkitt/mdoc/blob/0aaf8f706e754d8b4c9ad905305b7bac194218a1/docs/adr/0077-case-page.md)).

- **Settings by category.** General, Detection, Recognition, Spelling, Cases and About now have their own rows, switches and category-only Reset. Choices save immediately; numbers commit on Enter or blur with inline validation. The keyboard tab list, command palette and model/update links open each category directly, and the dictionary stays inside Spelling.

- **Folder sheet.** A dropped folder lists its files with checkboxes before
  anything opens: Open as case (or Add to “X” when a loaded case already holds
  its files or it lands on a case header), Open files, or Cancel. Open files
  never touches the case key, one folder opens at most 20 new tabs, and
  several folders get their sheets in turn
  ([ADR 0075](https://github.com/lawkitt/mdoc/blob/0aaf8f706e754d8b4c9ad905305b7bac194218a1/docs/adr/0075-folder-sheet.md)).

- **Choose the theme in Settings.** Appearance offers System, Light and Dark,
  stays in sync with the sidebar theme button, and saves the choice in the
  session. System follows the window's appearance live.

- **Consistent keyboard focus.** A single outside accent ring marks keyboard
  focus in light and dark themes, including panel toggles while their panels
  are open (fixes #194).

- **Keyboard Shortcuts sheet.** `?` outside text fields, ⌘/ (Ctrl+/), the
  command palette or Help › Keyboard Shortcuts lists every shortcut by
  context, with this platform's keys and a filter; the palette's shortcut
  labels come from the same table
  ([ADR 0073](https://github.com/lawkitt/mdoc/blob/0aaf8f706e754d8b4c9ad905305b7bac194218a1/docs/adr/0073-keyboard-shortcuts-panel.md)).

- **Untitled tabs are numbered.** The first is `Untitled.md`; later tabs take
  the lowest free number, which closing a tab makes available again. Empty
  untitled tabs keep their names across restarts, and Save As suggests the
  tab's name.

- **Save as Case retries a failed case save.** Closing a tab whose case
  couldn't be saved and choosing Save as Case now saves the case again and
  closes the tab once it is written (fixes #189).

- **The restore bar says what restored names are.** After Paste AI Response,
  the bar reads "The N restored names are proposals. Apply turns them back
  into aliases before you send the text to the AI again."

- **One close prompt, and quit asks about mappings.** Closing an edited,
  pseudonymized tab asks once, "Save “…” and keep its mapping?" (Save as
  Case, Save, Don't Save, Cancel), instead of two questions in a row. Quit
  now asks about unsaved mappings too, instead of dropping them; with two or
  more documents to lose, a summary comes first (Review…, Discard All,
  Cancel). The Settings switch "Ask before closing an unsaved mapping" covers
  both ([ADR 0071](https://github.com/lawkitt/mdoc/blob/0aaf8f706e754d8b4c9ad905305b7bac194218a1/docs/adr/0071-one-close-and-quit-prompt.md), #119).

## [0.2.1] - 2026-10-27

- **Sidebar tooltips follow file drops.** Inserting or reordering tabs
  dismisses a moved row's old tooltip, so it cannot name the previous tab
  over a newly inserted row (fixes #171).

- **Undo keeps the replacement popup open.** Undoing or redoing a keep that
  also changes applied text now follows the restored mention with its scope
  and focus intact, so the next popup shortcut still acts there (#178).

- **Rescan fits small windows.** The Replacements footer wraps its counts
  and actions at the minimum window size, keeping Rescan and Update aliases
  fully inside the panel (fixes #170).

- **Enter applies after choosing a popup scope.** Clicking This one or All
  no longer leaves Enter pressing that segment again; it applies the shown
  scope and moves to the next proposal (#177).

- **Visible view and zoom controls on Tab.** The floating document controls
  now stay visible while keyboard focus is inside them, so Tab no longer
  crosses seven hidden controls before reaching the editor (#168).

- **OS file opening keeps scanned PDFs.** An Open With event that arrives
  during an app update now waits for that update, then opens the source tab
  and text-recognition card through the same flow as File > Open and a drop
  (fixes #166).

- **Undo after Apply all.** Switching from Review to Share returns focus to
  the panel when its old search field disappears, including after editing a
  popup alias. Undo and Redo keep working without clicking the editor (fixes
  #169).

## [0.2.0] - 2026-10-26

Polish from the 2026-10-22 UX review and the code laid out by owner: plain
error messages, Find and the source preview fit small windows, and screen
readers and the keyboard reach more of the app.

- **Code laid out by owner.** Each piece of window and document state now
  lives in one owner with private fields
  ([ADR 0069](docs/adr/0069-app-architecture-and-code-ownership.md)):
  storage, import and detection moved into their own GUI-free crates, and
  `just lint` rejects glob imports, `#[path]` modules and GPUI under those
  crates. No change in behaviour; the headless renders match before and
  after.

- **Whole words in mention snippets; disabled actions say why.** The
  Replacements panel's snippets now start and end at whole words instead of
  mid-word ("ennett, known to…"). In the ⋯ menu, a disabled Copy for AI or
  Paste AI reply shows why under its name ("Pseudonymize the document
  first.", "Apply replacements first."), and screen readers announce the
  same reason.

- **The locked-cases notice waits until it matters.** Opening a plain
  document no longer pops "Saved cases are locked" over its preview; the
  notice stays behind the Notices bell. It still opens by itself when a tab
  restored from a saved case is open, when Paste AI Response or Pseudonymize
  clipboard finds no names in the tab while cases are locked, and after a
  declined Keychain dialog, so Try again is in view.

- **The source preview fits narrow windows.** Its toolbar stays on one row
  down to the minimum window size; tools without room move into a ⋯ menu
  that works from the keyboard. Fit width now fits the page at any width
  instead of stopping at 50%, and the preview's find bar stays inside the
  pane.

- **Plain error messages.** A damaged DOCX or PDF now says so in one
  sentence ("This DOCX is damaged or incomplete, so it can't be previewed.")
  instead of a technical error, and Retry shows only when trying again can
  help, such as a file that moved or a preview that timed out. A format
  without a preview says so once, and a failed conversion states its reason
  on the card instead of pointing to a separate notice. When every page of a
  PDF needs text recognition, the card says "All pages" instead of "Some
  pages".

- **F6 finds your tab in the collapsed sidebar.** F6 into the sidebar returns
  to the tab that last had focus, in the expanded list or the collapsed rail,
  even after collapsing or expanding the sidebar; before, the rail landed on
  Search.

- **Open hints name every format.** Home and a new tab list each format mdoc
  opens: PDF, Word, Excel, PowerPoint, OpenDocument, RTF, EPUB, CSV and
  Markdown, not just PDF, DOCX and Markdown. The unsupported-format message
  uses the same list.

- **Screen readers hear the counters.** VoiceOver and Narrator read the find
  bar's match counter ("Match 14 of 16") and the mention popup's position
  ("Mention 4 of 6"), and announce each change as you step through.

- **Undo works from the toolbar.** Undo and Redo reach the active document
  after clicking Apply and from other document chrome, including Ctrl+Y on
  Windows. Text fields keep their existing shortcut behavior.

- **Cleaner document conversion.** Converted PDFs and Office documents join
  hyphenated line breaks and normalize lists and blank lines.

- **Recents waits for pseudonymization results.** Opening or cancelling model
  setup no longer adds the "pseudonymized" tag to a never-scanned document,
  on screen or in its screen reader name (fixes #117).

- **Text recognition names both supported languages.** Settings now says it
  reads English and Russian, matching the offered model choices (fixes #120).

- **Find controls fit small windows.** Find and Find and Replace keep usable
  text fields and wrap their controls within the editor column when the
  Replacements panel and Source preview are open at the minimum window size.

- **Know what a model download contains.** Pseudonymization and OCR setup,
  including the model choices in Settings, show the source repository and
  license beside the download size.

- **The caret keeps its row at a soft wrap.** On a row that wrapped inside a
  word (a long URL, unspaced text), ⌘→ now goes after the row's last
  character instead of stopping one short, and the caret stays drawn at the
  end of that row. Up and Down from there keep to the ends of the rows, and a
  click past a row's end puts the caret there too. Up and Down also step
  through the rows of a table cell whose text wraps or has `<br>` lines
  before moving to the next table row.

- **Windows offscreen screen and Tab evidence.** `just headless-renders`
  now renders both themes through DirectWrite and a software DirectX device,
  without creating native windows. The Windows test gate checks the same PNG,
  Tab-walk and timing contract as macOS, using a temporary app profile.

## [0.1.5] - 2026-10-23

- **Tab moves between regions.** Tab past the toolbar now enters the editor
  instead of skipping it, and Tab from the Replacements panel's last control
  moves on to the next region instead of looping inside the panel. ⇧Tab from
  the panel returns to the editor. F6 is unchanged, and the editor still
  takes Tab for Indent.

- **Home's Recents and Cases rows work by keyboard.** Each row is a Tab stop
  with a name for screen readers, and Enter or Space opens it.

- **Category and filter menus work by keyboard.** The mention popup's
  category menu, its Belongs to… menu and the Replacements panel's filter
  menu now take focus when they open: Tab or the arrows move between rows,
  Enter chooses, and Esc returns to the button. Before, focus stayed on the
  button and ↓ stepped to the next mention behind the open menu (fixes #167).

- **Keychain explainer keeps Continue reachable.** Tab and ⇧Tab now move
  between Not now and Continue instead of sticking on Not now. The OCR and
  pseudonymization setup cards' buttons had the same fault and are fixed too.

- **Expand or collapse all identities.** A toggle beside the Replacements
  panel's filter, and two palette commands, open or close every group.

- **Go to heading.** The command palette lists the document's headings;
  choosing one moves the caret there and scrolls it into view.

- **Contrast checks.** Tests hold text to WCAG 4.5:1 and controls to 3:1 in
  both themes. Light muted text, light applied aliases, dark destructive menu
  rows, category-coloured mention text and the notices badge are adjusted to
  pass, and nothing is drawn below 10 px.

## [0.1.4] - 2026-10-23

- **First published release with the 0.1.3 changes.** The 0.1.3 release build
  stopped at a stale OCR smoke-test check and was never published; 0.1.4 ships
  the same changes with the test fixed.

## [0.1.3] - 2026-10-23

- **Dependency security update.** `ctutils` 0.4.5 fixes non-constant-time
  comparisons (RUSTSEC-2026-0385) in the crypto crates behind case encryption.

- **Replacement navigation keeps visible mentions in place.** Stepping through
  mentions or applying a proposal with Enter no longer scrolls away from an
  already-visible target. Off-screen mentions scroll just far enough to show
  the mention and its popup.

- **Quit works during model setup.** Quit and app-level commands stay available
  without keyboard focus. Both the pseudonymization and OCR cards keep focus on
  Cancel when Download becomes progress. Quit still asks about unsaved documents
  and cancels setup after those prompts are resolved (fixes #103).

- **Reliable PDF scrolling tests on Windows.** The tests carry their PDF fixture
  with them, so cached binaries still work after a checkout moves. They wait for
  the PDF to load and report load errors before checking toolbar and scroll bounds.

- **DOCX previews keep words apart.** The local renderer now resolves theme
  fonts correctly, preserving word spacing in bold justified headings with tabs.

- **Offscreen screen evidence on macOS.** `just headless-renders <directory>`
  writes light/dark PNGs for Home, a document, Review, Settings and app dialogs,
  plus actual Tab walks, focus-ring images and frame costs in JSON. Dev-only,
  with a temporary profile and no visible window or activation. Windows's
  headless DirectX adapter remains a follow-up.

- **Pick the recognition languages on the OCR card.** The card offers English
  or English & Russian for this document and picks a default: English &
  Russian when the PDF's text, its file name (Cyrillic letters, or a word
  such as "russian" or "ru") or, failing those, a Russian keyboard suggests
  Russian; otherwise your Settings choice. One line says why, for example
  "English & Russian: the file name is in Russian" (ADR 0066).

- **Small polish.** View › Toggle Sidebar has ⌃⌘S on macOS. Two tabs of the
  same name in one folder (a DOCX's conversion beside an `.md`) show their
  type, not the full path. The DOCX notice about equations, charts and
  SmartArt shows only when the file has them. A search match at the start of
  a wrapped line, or wrapping at a space, no longer paints a block at the end
  of the line above. After Save As the tab's badge says MD; its tooltip names
  the source. The window title is "mdoc" while Home shows. A drop from
  Finder brings mdoc to the front, so a dialog it opens gets the keyboard.
  Unsupported files dragged over the sidebar show "These files can't be
  opened" instead of the insertion line and open nothing; a mixed drop's
  notice names skipped files without their folders, and Esc closes it. The
  source preview's Find button no longer claims ⌘F (that finds in the
  Markdown). The theme button's tooltip follows each click. A setup cancelled
  part-way says "MB left to download".

## [0.1.2] - 2026-10-22

- **Dialogs and choices read correctly with a screen reader.** Settings is
  announced as a dialog; model choices as radio buttons in a named group; the
  narrow window's Markdown / Source switch as a "View" choice; each tab's
  close button names its tab. Every confirmation (unsaved changes, model
  removal, case actions) is now mdoc's own dialog, named by its question
  rather than "Warning": unsaved changes ask "Save changes to “name”?" with
  Save, Discard (⌘D / Alt+D) and Cancel (Esc) (ADR 0064).

- **Find and Replace.** ⌘⌥F / Ctrl+H (or Edit ▸ Find and Replace…) opens find
  with a Replace row. Enter replaces the current match and moves on; ⌘↩ /
  Ctrl+Alt+Enter replaces all of them as one undo step. Whole word joins
  Match case (⌥⌘W / Alt+W and ⌥⌘C / Alt+C). Replace leaves applied aliases
  and matches that cross hidden formatting alone and says how many it skipped
  (ADR 0063).

- **Every control is reachable by keyboard.** F6 and Shift+F6 move between the
  sidebar, the toolbar, the document, the Replacements panel and the original,
  returning to the control you left in each. Tab from Home reaches its
  buttons. In the sidebar, ↑ and ↓ move between documents and Backspace closes
  one; Shift+F10 opens its case actions. Esc in the sidebar or toolbar returns
  to the document. Case category choices and the Keychain explainer keep focus
  inside until closed (ADR 0062).

- **macOS caret keys.** ⌘← and ⌘→ go to the start and end of the line as
  you see it wrapped (after a list bullet first), ⌘↑ and ⌘↓ to the start and
  end of the document, and Shift with each selects. ⌥⌫ and ⌥⌦ delete a word,
  ⌘⌫ deletes to the line start (ADR 0061).

- **Enter in the mention popup follows the scope you see.** With All N
  selected, Enter replaces every mention of the name; with This one, only
  this mention. The choice no longer flips to This one, and the popup moves
  on to the next suggestion, closing when none is left. Apply all (⌘↩ /
  Ctrl+Enter, or the toolbar's Apply) closes the popup (ADR 0055).

- **Enter starts a paragraph.** Lines you separate with Enter stay separate
  in Copy for AI: Enter writes a blank line between paragraphs, and Shift+Enter
  is a line break within one (`\` at the line end, hidden away from the
  caret). Lists, quotes, tables and code keep their own Enter; leaving a list,
  quote or table leaves a blank line. Backspace at a paragraph's start joins
  it to the one above in one press. Existing documents are not changed
  (ADR 0060).
- **An OCR run that finds no text stays with its PDF.** Other PDF tabs, and
  PDFs opened afterwards, offer Run OCR as usual instead of showing that
  file's error and Retry.
- **Esc closes the Replacements panel's right-click menu** first; with the
  mention popup open, a second Esc closes the popup.
- **A failed conversion points to the notice below**, where it is shown,
  instead of "above".

## [0.1.1] - 2026-10-22

- **Shortcuts keep working after Save as case and after undoing in the
  popup.** After Share › Save as case, focus stays in the Share panel, so
  Ctrl+Q / ⌘Q and other shortcuts work without clicking the document first. An undo that
  closes the mention popup returns focus where Esc would, so redo still works.
- **Bold and other formatting stay valid with a trailing space.** Formatting a
  selection that starts or ends with a space keeps the space outside the
  markers (`**delta** `), so Copy for AI keeps the formatting.
- **A converted document saved as Markdown enters Recents** as the saved
  file, so it reopens as your edited Markdown instead of a fresh conversion of
  the source. On macOS, a newly saved file gets the usual permissions instead
  of being readable only by you.
- **Save as case keeps the document.** On a conversion whose Markdown was
  never saved, Save as case (or Save as Case when closing) first asks where to
  save it, then creates the case with that file in it. It no longer creates
  an empty case that can't be opened.
- **One mdoc on Windows.** A second launch or **Open with** hands its files to
  the running mdoc, which opens them and comes to the front, instead of
  starting another copy that also writes the session and Recents.
- **mdoc is offered under Open with on Windows** for Markdown, text, PDF and
  Word files, never as the default app; uninstalling removes exactly those
  entries.
- **Windows word and document moves.** Ctrl+←/→ (with Shift) move by word and
  stop at the next word's start, Ctrl+Backspace/Delete delete a word, and
  Ctrl+Home/End (with Shift) go to the document's ends without revealing
  hidden syntax.
- **The command palette covers what the Windows menu lacks:** Save As,
  Before / After, Close Tab and Toggle Sidebar. Settings › Saved cases now
  points to Home and the palette.

## [0.1.0] - 2026-10-20

- **Windows window buttons work.** Minimize, maximize/restore and close act on
  a click, hovering maximize shows snap layouts, and double-clicking the title
  bar toggles maximize and restore (ADR 0048).
- **Standard macOS menus.** The menu bar now has mdoc, File, Edit, View and
  Window menus: About, Hide (⌘H), Services, menu-bar Undo, Cut, Copy and Paste
  (which also work in Open and Save dialogs), Find, zoom, Minimize (⌘M) and tab
  switching (ADR 0058).
- **The Keychain explainer's picture can't be mistaken for the real dialog.**
  The animated preview of the macOS password dialog now sits below the
  explanation, captioned "After Continue, macOS shows this:". macOS still asks
  only after you press Continue (ADR 0043).
- **The document follows your selection.** Dragging a selection to the top or
  bottom edge scrolls the page, even while the pointer stays still, and
  Shift+arrow selection, arrow keys and typing keep the caret in view with a
  few lines of context (ADR 0054).
- **The zoom and view pill no longer covers text.** It fades after a moment
  and comes back when you move the pointer near the bottom of the document,
  scroll, zoom or switch views. It stays while the pointer is on it, and
  ⌘= / ⌘− still zoom (ADR 0048).
- **Word previews say they are approximate**, with an ⓘ beside the file name
  instead of a notice. The preview header now reads **Source · name**
  (ADR 0048).
- **Crash reports you check first.** If mdoc hits an internal error, the next
  launch says so and offers **See what would be sent**: the report with folder
  names and quoted text masked, and never document text. **Send…** opens a
  prefilled GitHub issue for you to review; **Copy** and **Dismiss** also
  work. Nothing is sent automatically (ADR 0047).
- **View switcher.** While replacements are applied, a floating control at the
  bottom of the document switches between **Edit**, **Original**,
  **Pseudonymized** and **Side by side**. The toolbar button and ⌘\ still
  toggle Side by side (ADR 0046).
- **The empty page says documents stay on this Mac**, with a hover note on the
  only network use: the update check and model downloads you choose. Long
  tooltips now wrap (ADR 0046).
- **Replacements panel groups by category.** Names are listed under a label
  per category with counts, e.g. "Person 3 (7)": names, then mentions. The
  labels are not selectable; arrow keys skip them (ADR 0046).
- **Paste AI Response** with a reply that has no placeholders now says
  "No placeholders found in this reply." instead of "Restored 0 mentions of
  0 values" (ADR 0046).
- **Line breaks in table cells.** `<br>` inside a table cell, as converted
  Word tables produce, now shows as a line break instead of literal text. The
  saved Markdown is unchanged (ADR 0046).
- **Categories to detect** keeps its close button inside the popup for long
  case names; the name is shortened under the title. Long names in
  **Move to case “…”** are shortened too. The panel footer no longer reads
  "not in this document (0)": with nothing to list it offers
  **Add a name to “case”** (ADR 0046).
- **No Keychain dialogs at launch.** When a saved-case key was made by another
  build, launching mdoc could still show two macOS password dialogs: the quiet
  check reported "denied" where macOS meant "would ask", and launch then read the
  key with dialogs allowed. Launch now leaves such cases locked behind
  **Unlock…**. Builds signed with a provisioning profile keep the key in the data
  protection keychain, which never asks for the login password. An existing key
  moves there on the next Unlock, after one last dialog. Development builds keep
  their own key and cases folder (ADR 0045).
- **Keychain access is explained before macOS asks.** mdoc no longer reads the
  saved-case key at launch while doing so could show the macOS password dialog.
  Until it can, saved cases stay locked behind an **Unlock…** bar. Saving a case,
  opening one, or unlocking first shows a short explainer with an animated
  preview of the macOS dialog, recommending **Always Allow**. Declining keeps
  the documents linked to their case and offers **Try again…** (ADR 0043).
- **Replacements panel:** Apply, Keep and Undo leave the selection on the
  mention you acted on, rather than jumping to the next one; after Keep, its
  group header is selected. Applied mentions show a blue bar, dimmed when not
  selected. Group headers lose their ▸/▾ markers and line up with their
  mentions. A mention row repeats the original only when its wording differs.
  A selected row shows one Undo. The panel's close button now matches the
  Original viewer's (ADR 0044).
- **Before / after** follows the Replacements panel live: applying, undoing or
  keeping originals updates both columns with the editor's replacement motion.
  The header row is gone: quiet floating labels name the columns, a floating
  **Close** (or Esc) leaves the view, and originals are marked gold beside blue
  aliases. The toolbar shows a two-column icon that stays pressed while the view
  is open (⌘\\, also in the File menu). Selecting a replacement scrolls both
  columns to it, clicking a name selects its row, and narrow windows show one
  column at a time. Opening the view hides the PDF preview until it closes
  (ADR 0042).
- Dropping a folder opens its files (not subfolders) as one new case named after
  it; each document joins once it has Markdown. **File → Pseudonymize
  Clipboard** (⌥⇧⌘C) replaces names the active tab's case knows in the
  clipboard text, without running detection. **Before / after** shows the
  original wording beside the pseudonymized text, line by line. A case's
  **Categories to detect…** switches categories off for scans in its documents
  (ADR 0041, phase 4).
- Cases with several documents: right-click a tab and choose **Move to case**,
  or drag it onto a case's header. Names the case already knows take its
  aliases, other names that clash get the next free number (one undo step), and
  the case's other names are proposed wherever they appear. Renaming an alias or
  changing its category in one document updates the other open documents of the
  case, each as its own undoable edit. The replacements panel lists the case's
  names this document doesn't mention and adds new names to the case before they
  appear. Case headers fold their documents away (ADR 0041, phase 3).
- Saved cases: right-click a tab with applied replacements and choose **Save
  mapping as case** to keep which original each alias stands for, encrypted on
  this computer (AES-256-GCM; the key is held by the macOS Keychain, Windows
  data protection or, on Linux, the Secret Service keyring such as GNOME
  Keyring or KWallet). The sidebar shows the case above its document; later
  mapping changes save automatically. Reopening the document gives its aliases
  back their originals, so they can be restored and AI replies pasted again. Old
  aliases keep restoring after a rename. **File → Open Case…** lists saved
  cases; a case can be renamed, a document removed from it, or the case deleted
  and its originals forgotten. Closing a tab whose mapping isn't saved asks
  once (Settings → Saved cases turns this off). On Linux without a keyring,
  cases are unavailable (ADR 0041, phase 2).
- AI return journey: after applying replacements, **Copy for AI** copies the
  Markdown with a short note asking the assistant to keep placeholders such as
  `PERSON_1` exactly. **Paste AI Response** (⌥⇧⌘V, the toolbar or the File
  menu) opens the assistant's reply in a new tab with the original names
  restored as proposals that keep their aliases, so Apply turns them back into
  aliases before the next round. Reshaped aliases (`person 1`, `PERSON-1`,
  `**PERSON_1**'s`, `PERSON_1-у`) restore; placeholders the original document
  never had are highlighted in red and can be assigned to a name or left as
  they are. Nothing is saved to disk (ADR 0041, phase 1).
- In-app updates on macOS: Settings → Updates offers **Download and install**
  for a newer release, then **Restart now**. The download is checked against
  the release's `SHA256SUMS` and must carry the mdoc developer's Apple
  signature, pass Gatekeeper and contain the promised version; the installed
  app is replaced only while mdoc quits, and kept if anything fails. A banner
  at the top of the window shows an available update, download progress, a
  ready update and failures, and opens Settings → Updates. Nothing downloads
  without a click; copies that can't replace themselves (not in Applications,
  running from the disk image, unsigned builds) keep **View release**
  (ADR 0040).
- macOS releases are signed with a Developer ID and notarized by Apple, so mdoc
  opens normally on first launch; the release gate checks Gatekeeper accepts the
  `.dmg` and the app (ADR 0039).
- Failed model downloads say plainly that GitHub couldn't be reached, or name
  the proxy or HTTP problem, instead of showing the raw network error.
- Update check: at launch mdoc asks GitHub whether a newer release exists and
  marks the Settings button with a dot when one does. Settings → Updates shows
  the version and notes with **View release**, **Check now**, and switches for
  the launch check and pre-releases (always on in pre-release builds). It only
  sends the app version and never downloads or installs (ADR 0038).
- Releases: Apple Silicon `.dmg` and Windows x64 installer (`.msi` for stable
  versions), with `SHA256SUMS` and build provenance. The Windows installer
  installs the Visual C++ Redistributable that OCR needs when it is missing
  (ADR 0037).

- Dark Original pages: in the dark theme PDF and DOCX pages render on dark
  paper with light ink, keeping the colour of coloured text. The ◐ button in
  the Original header switches a document back to the original paper (ADR 0035).

- Replacements panel: Undo, Redo and Rescan icon buttons in its toolbar, and
  ⌘Z / ⇧⌘Z work while the panel has focus. Scan progress and Cancel live only
  in the main toolbar; the ⋯ menu is gone (Model settings are in Settings).
  After Apply, Keep or Replace a short "… · Undo" notice appears. Pseudonymization
  is no longer labelled experimental (ADRs 0033, 0034).
- Replacements panel groups open independently (a header click only toggles),
  with grey rows, Expand/Collapse all, an All/Proposed/Applied filter, and the
  alias and category editable on the group header.
- Selecting a mention in the panel shows inline Apply/Keep controls instead of
  the popup. Keys in the panel and the popup: Enter / ⌘Enter / ⇧⌘Enter apply
  this one / same text / all; Delete with the same modifiers keeps originals.
  ↑/↓ move through the list and focus follows to the next undecided mention.
- ⌥↑/↓ moves a mention or group to another entity (release ⌥ to drop, Esc to
  cancel). A mouse wheel steps through the list; a middle click keeps an
  original, in the panel and on editor chips.

- Removed interactions that did nothing in mdoc (ADR 0031): the PDF
  toolbar's Highlight (⌘⇧H) and Area highlight tools; the caret now enters
  `$…$` and `$$…$$` math like other Markdown, Backspace/Delete edit it
  character by character, and right-click shows the normal menu; clicking
  an inline image places the caret instead of showing a hand cursor.

- Standard Markdown only: note-taking syntax inherited from Zorite —
  `[[wiki links]]`, `#tags`, `key:: value` property panels, `((block refs))`,
  `^block-ids` and `![[embeds]]` — now displays as ordinary text. Their links
  never opened anything in mdoc. Saved files and Copy Markdown are unchanged
  (ADR 0030).

- Empty page: a blank Markdown tab shows a centered **Open files…** button,
  supported formats, the multi-file hint and ⌘O, and still accepts typing.
  Files dragged from the OS open when dropped on the empty page or on the
  sidebar, at an insertion line between documents; dragging over the collapsed
  rail reveals the list (ADR 0028).

- Chrome-style document sidebar: expanded by default with a + row under the
  last document. When collapsed, hovering or keyboard focus slides the full
  list out over the editor; it slides back shortly after the pointer leaves.
  Compact entries switch documents directly, replacing the filename popup
  (ADR 0027).

- First-use model setup asks in place: a Markdown-pane card for OCR and a
  Replacements-panel card for Pseudonymize show the download size, total-byte
  progress, Cancel and Retry, then continue the task. Settings no longer opens
  by itself; model choices save immediately with plain-language rows.
  English OCR (PP-OCRv6 Small) is the new default (ADR 0026).

- Multi-file opening with lazy retained tabs, paths-only session restoration,
  compact sidebar navigation, and automatic PDF/DOCX conversion when OCR is
  unnecessary; explicit inline OCR choices preserve the original source.
- Slim Markdown and PDF/DOCX overflow scrollbars, horizontal preview scrolling,
  and quieter expandable conversion notices.
- Product direction, roadmap, domain glossary, and consolidated architecture
  decisions; manual build-cache cleanup instructions.
- Read-only side-by-side DOCX preview with a local Rust converter, plus a
  read-only DOCX comment list that never enters editor text.
- Literal Markdown find (Cmd/Ctrl+F, Cmd/Ctrl+G navigation, Match case
  toggle) over rendered visible text.
- Local offline OCR import for scanned PDFs on Apple Silicon macOS and
  Windows x64, with explicit setup and review warnings.
- Search-bar styling pass and Markdown search performance coverage.

## 0.1.0

- Initial standalone mdoc repository, derived from Zorite.
- File-based Markdown WYSIWYG editing with atomic saves and external-change detection.
- Side-by-side PDF viewing and local document import through AnyDoc.
- Light and dark themes, with native file actions on macOS, Windows, and Linux.

For the original project's release history, see
[Zorite releases](https://github.com/packetThrower/zorite/releases).
