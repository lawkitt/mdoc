# Local PDF OCR integration decisions

Status: implemented; validation and remaining platform/UI limits are recorded
in the qualification report. User approved the
Cyrillic model replacement and external fork patch after the initial model
failed Russian acceptance.

## Confirmed decisions

### Round 1

- Extend **Import as Markdown…** to recognize scanned PDF content locally,
  producing editable Markdown alongside the unchanged original PDF.
  Writing searchable PDFs is outside this scope.
- Allow explicit first-use setup downloads of required models and runtimes.
  After setup, recognition works offline. Document contents stay on-device.
- Start with printed documents in English and Russian Cyrillic,
  including mixed scanned/native-text PDFs. Handwriting and complex table
  reconstruction are outside initial acceptance requirements.

Rationale: extend the existing file-based import workflow and preserve the
source while adding local recognition for the user's initial language needs.

### Round 2

- After setup, automatically recognize scanned/incomplete pages during PDF
  import. Retain usable native text and combine results in document order;
  do not add a separate OCR button initially.
- Import usable partial results with persistent, page-specific warnings for
  unreadable or low-confidence pages. Keep warnings outside Markdown. If no
  usable content is extracted, leave the current document untouched and fail.
- Validate OCR on the user's Mac first, keeping macOS, Windows, and Linux
  buildable. Enable OCR on each platform only after an actual scanned-PDF
  runtime test passes on that platform.

Rationale: preserve useful content without silent omissions, reuse the import
flow, and distinguish build support from verified runtime support.

### Round 3

- Call pdf-inspector directly for PDFs inside the existing application import
  module; retain AnyDoc for other formats. Keep pdf-inspector external and pin
  a tested revision. Review existing PDF snapshots when upgrading the engine.
- Offer **Set up local OCR** or **Skip OCR** when an import needs missing OCR
  components. Also provide a **Set up OCR** action in the main UI bar so users
  can prepare OCR before importing a document.
- Download verified components into app-managed storage during explicit setup;
  subsequent imports run offline. Skipping setup retains native text with
  warnings; fully scanned PDFs leave the current document untouched. Setup
  failures allow retry and never fall back to cloud processing.
- Keep one background import at a time, with a **Recognizing text…** indicator
  and an editable current document. Preserve unsaved-change protection and
  discard stale results after document changes. Do not promise percentage
  progress or immediate cancellation with the current synchronous library API.
- Clarification: "Latin" means English for initial language acceptance;
  Russian remains in scope. Other Latin-script languages are not initially
  qualified.

Rationale: keep the conversion seam small, allow advance setup, and preserve
the existing import lifecycle without promising unsupported job controls.

### Final confirmation

- Main-bar states are **Set up OCR**, **Setting up OCR…**, disabled **OCR ready**,
  and **Retry OCR setup** with an actionable error after failure.
- Test actual English and Russian recognition on the user's Mac before
  completing integration. If the bundled model fails, report evidence and
  propose a compatible local replacement before expanding dependency work.
- User confirmed the complete design and requested implementation.

## Verification gates

- Verify recognition quality for English and Russian; dictionary coverage
  alone does not establish recognition quality.
- Verify runtime packaging and model compatibility before adoption.

## Initial qualification result

On 2026-09-19, the pinned upstream PP-OCRv6 Small model ran successfully on
Apple Silicon macOS but failed Russian recognition. See
[the qualification report](local-ocr-qualification.md) for fixtures, output,
runtime versions, and the replacement. Application integration resumed after
the user approved that replacement.

## Approved replacement and implementation

- User approved the replacement and fork patch. English and Russian controlled
  scan fixtures both passed with the v5 Cyrillic recognizer and v6 Small detector.
- Pin external fork commit `620afae42eac4b92fffa2437b89e10a912bb93ee` at
  <https://github.com/lawkitt/pdf-inspector>. The lawkitt organization
  owns this fork; the original Firecrawl history is preserved.
- The fork accepts a pinned model manifest and explicit runtime paths. mdoc
  selects one qualified model; no model chooser or process environment mutation.
- Setup uses application-local storage and verifies archives, libraries, and
  models. It downloads about 54 MB once, retains runtime notices, and validates
  actual runtime/model loading before marking OCR ready.
- macOS Apple Silicon is enabled. Other desktop targets stay buildable and
  offer native-text import with explicit OCR omission warnings.

### Publication under lawkitt

Published the same tested commit to `lawkitt/pdf-inspector` on `main` and
`feat/local-ocr-models`, retaining the Firecrawl fork relationship and history.
The local checkout uses lawkitt as `origin` and Firecrawl as `upstream`.
Updated mdoc's dependency URL, lockfile, and documentation without changing the
commit or model artifacts. Cargo check, formatting, strict workspace/all-target
Clippy, and all six import tests passed against the new Git source.
