# AnyDoc integration decisions

Current PDF integration (2026-09-19): PDFs now use the separately pinned
OCR-capable pdf-inspector fork directly through `src/import.rs`; AnyDoc still
handles other formats. The AnyDoc transitive PDF baseline remains pinned in
Cargo.lock. See [local OCR decisions](local-ocr-decisions.md) and
[qualification](local-ocr-qualification.md). The decisions and validation below
describe the original AnyDoc integration; their OCR exclusions are superseded.

Status: design confirmed by the user; implemented. Validation is recorded below.

## Confirmed decisions

### Round 1

- Add a dedicated **Import as Markdown…** action. Preserve the current Open
  behavior, including PDFs opening in the side pane. Import produces a new
  editable Markdown document and respects unsaved-change protection.
- Initial conversion scope is text and structure: headings, lists, and tables.
  Embedded-image preservation and OCR are outside the initial scope. Explain
  OCR-required failures to the user.
- Include upstream PR <https://github.com/firecrawl/anydoc/pull/153>, subject to
  inspecting its exact changes and recording the selected revision.
- Maintain our own AnyDoc fork with a small patch set, so upstream updates can
  be merged regularly.

Rationale: keep importing explicit, preserve existing editor flows, and keep
conversion scope small while allowing focused converter improvements.

### Round 2

- Fork patch scope starts with PR 153 only, preserving upstream history. Keep
  future patches small and host-agnostic; merge upstream regularly, removing
  or reconciling our patch once upstream incorporates equivalent behavior.
- Use Ocr::Skip for mixed PDFs. Show a visible warning listing omitted pages;
  fully scanned PDFs fail without changing the current document.
- Open successful imports as unsaved Markdown, suggesting source-name.md on
  Save. Never overwrite the source. For PDFs, show the source alongside the
  imported Markdown after the document transition is accepted.
- Allow one background conversion at a time, with an indeterminate Converting
  indicator. The current editor remains editable. After successful conversion,
  protect then-current edits with the normal save/discard/cancel flow before
  replacing the document. Failures leave the current document untouched.
- Closing the window discards the pending result. Do not promise hard
  cancellation or percentage progress: AnyDoc exposes neither.

Rationale: keep the initial fork delta minimal, avoid silent partial imports,
and reuse the existing file lifecycle without blocking the UI.

### Integration structure

- Keep AnyDoc external to the mdoc workspace, pinned to a fork commit through
  Cargo with Cargo.lock committed.
- A small src/import.rs module owns conversion and maps errors into app-facing
  errors. GPUI owns background scheduling and document transitions. AnyDoc
  remains host-agnostic; mdoc-editor does not depend on it.
- Avoid a converter trait, plugin system, or additional wrapper crate.

## PR 153 inspection

Checked live on 2026-09-18: open and unmerged. Head commit
76c444fd1df4df5eb824b55fd3ad144347c839a2; base main commit
261fc257d17c3eab0f673be31c408fd9fdc2171a matches the local AnyDoc checkout.

The PR introduces Options, Ocr::Reject/Skip, and conversion results containing
Markdown, pages_needing_ocr, and page_count. Skip permits partial conversion of
mixed PDFs and reports omitted pages. Fully scanned PDFs still fail with
NeedsOcr. It adds no OCR engine or dependencies and preserves default behavior.
Omitted-page numbers are 1-based. The Markdown contains no omission notice;
mdoc retains metadata and presents the warning itself.

## Final confirmed decisions

- Use the fork at <https://github.com/lawkitt/anydoc>, branch `master`.
- If the user switches to another Markdown document or creates a new one
  during conversion, discard the pending result. Edits to the same document
  remain covered by the completion-time unsaved-change prompt.
- Retain partial-conversion warnings until dismissed or the document changes;
  saving does not dismiss them. Say which original PDF pages were skipped,
  without inserting warnings into Markdown.
- Offer the formats supported by AnyDoc, without per-format settings or batch
  conversion. Verify PDF, DOC/DOCX, and XLS/XLSX with representative fixtures;
  other supported extensions are accepted but not claimed equally qualified.
- Adoption and fork updates check complete/mixed/scanned PDFs, office output,
  unsupported/corrupt/encrypted input, document lifecycle and warning behavior,
  plus the existing workspace fmt/clippy/test gate. Use human checks for native
  shortcuts and visual acceptance; headless tests do not provide either.

## Implementation and maintenance

The fork's `master` branch contains PR 153 cherry-picked with attribution as
`391b6b109b21c051a0edd726755352d78df3853f`. The source commit is
`76c444fd1df4df5eb824b55fd3ad144347c839a2`; the new repository's initial commit
had the same base code but did not retain upstream Git history. The port keeps
the existing fork history and produces the same code as the original patch
(the fork also contains an unrelated `.DS_Store` file). There are no
mdoc-specific changes inside AnyDoc. Cargo.toml pins the tested port revision.
The editor crates have no AnyDoc dependency; only the application import module
knows its types and options.

The application holds one background conversion and a document generation.
Successful New/Open transitions invalidate its result. Completion while a native
dialog is open waits for that dialog to resolve, then checks the generation.
Only an accepted import resets the editor and opens its PDF source. Imported
empty output is still unsaved. Warning metadata survives Save and is never
written into the Markdown. Local links in unsaved imports resolve from the
source directory; after saving they resolve from the Markdown directory.

The lockfile deliberately retains `pdf-inspector` 1.14.2, the version in
AnyDoc's upstream lockfile at the selected commit. Initial resolution selected
1.20.0, whose PDF output differed from upstream snapshots. This difference was
caught before adoption; the snapshots were not rewritten to hide it.

For an upstream update:

1. Fetch firecrawl/anydoc in a separate AnyDoc checkout and port reviewed upstream
   changes onto the fork's `master` branch. The new fork starts from a snapshot
   with unrelated history, so a normal upstream merge is not yet available;
   preserve this history rather than force-pushing a replacement. Keep application
   logic out of that checkout.
2. Reconcile PR 153 if upstream has incorporated it, including a squashed or
   equivalent implementation. Preserve the import metadata interface or adapt
   only src/import.rs when upstream changes it.
3. Run `cargo test -p anydoc --locked` in the fork. Push the tested fork commit,
   then change mdoc's Cargo.toml revision and update Cargo.lock. Review
   transitive changes explicitly; use
   `cargo update -p pdf-inspector --precise 1.14.2` to retain the current baseline
   if no PDF-engine update is intended.
4. Run mdoc's fmt/clippy/workspace-test gate, including its checked-in import
   corpus and GPUI flow tests. Review changed Markdown against the source before
   updating expectations. Native shortcuts and appearance need a human check.

## Validation

- Conversion corpus: DOC, DOCX (including tables), XLS, XLSX, text PDF, mixed PDF
  with readable content after scanned pages, fully scanned PDF, encrypted ODT,
  corrupt DOCX, unknown content, missing files, and CSV extension fallback.
- GPUI flows: import/save/source preservation, completion-time edit protection,
  save-before-replace, stale results, dialog deferral, one-job restriction,
  warning lifetime, empty imports, errors, and the source PDF pane.
- On macOS, `cargo fmt --check`,
  `cargo clippy --workspace --all-targets -- -D warnings`, and
  `cargo test --workspace --locked` passed (186 tests). `git diff --check` passed.
- The exact AnyDoc fork revision passed `cargo test -p anydoc --locked` in an
  isolated checkout: 301 passed, one optional local-corpus test ignored.
- Cargo reports an existing future-compatibility notice for block 0.1.6 and
  ignores the configured registry minimum publish age without its nightly flag.
  Neither prevented the gates from passing.
- Windows/Linux builds were not run locally. Native Cmd/Ctrl+Shift+I and visual
  acceptance remain human checks; GPUI tests above are headless flow evidence.

### Repository migration (2026-09-18)

- Pushed the attributed PR 153 port to `lawkitt/anydoc` on `master` at
  `391b6b109b21c051a0edd726755352d78df3853f` and verified the remote revision.
- The port passed `cargo fmt --check` and `cargo test -p anydoc --locked`:
  301 passed, one optional test ignored. The original PDF fixture's required
  cross-reference padding triggers Git's whitespace check; the fixture was
  preserved byte-for-byte.
- mdoc passed formatting, strict workspace/all-targets Clippy, all 186
  workspace tests with `--locked`, and `git diff --check` on macOS.
- Cargo's built-in Git fetch could not authenticate to the new repository here.
  Fetching with `CARGO_NET_GIT_FETCH_WITH_CLI=true` used the existing Git
  credentials successfully. Fresh checkouts need repository access and may need
  that environment variable; no machine-wide Cargo settings were changed.
- Only AnyDoc's Git source/revision changed in Cargo.lock. In particular,
  `pdf-inspector` remains at 1.14.2 and all existing import snapshots passed.
