# DOCX viewer research for mdoc

**Date:** 2026-09-18

**Scope:** research and implementation direction for opening a `.docx` beside mdoc's Markdown editor, with behavior comparable to the existing PDF pane.

## Executive recommendation

Build the first DOCX viewer as an asynchronous **DOCX-to-PDF adapter** and reuse `gpui-pdf` for the actual pane. Keep the original `.docx` as the source of truth, write the converted PDF to a private temporary directory, and delete that directory when the preview is replaced or closed. This gives mdoc page layout, zoom, navigation, links, and search through code that already works on all three target platforms.

Do not make LibreOffice a required runtime dependency for the shipped app. Use it as a development and qualification oracle, or as an explicitly detected fallback when installed. For a self-contained release, evaluate a native Rust converter behind a small application-owned adapter. The most promising candidates found in this research are `rdocx` and `docxide-pdf`, but both need a real-document fidelity and licensing review before adoption. Neither should be treated as Word-compatible merely because it can render a sample document.

A direct DOCX layout engine embedded in GPUI would eventually provide better semantics and editable text, but it is a separate document-layout product. It would duplicate much of the PDF viewer's pagination, rasterization, virtualization, and text-search work. It should be a later project, only if mdoc needs selectable DOCX text, annotations, live reflow, or editing inside the preview.

## What mdoc has today

The current application already has the important host seams:

- `src/main.rs` owns `Workspace`, file opening, unsaved-document transitions, and the side pane.
- `gpui-pdf::PdfView` loads and parses off-thread, virtualizes pages, rasterizes visible pages off-thread, supports zoom and page navigation, and has optional search/forms features.
- `open_path` currently routes Markdown to the editor and PDF to `open_pdf`; other extensions are rejected.
- `src/import.rs` already converts DOC/DOCX to Markdown through the pinned AnyDoc fork. That path intentionally preserves text and structure, not page geometry. It is therefore useful for **Import as Markdown**, but it cannot serve as a faithful visual preview.
- Existing tests cover source preservation, DOCX import fixtures, PDF-pane lifecycle, and the normal workspace gate. The application is GPL-3.0-or-later; reusable crates are MIT.

This means a viewer should not be added to `mdoc-editor`, `mdoc-markdown`, or `gpui-pdf` initially. It belongs in the application layer, with a narrowly scoped conversion module beside `src/import.rs`.

## What “DOCX viewer” can mean

There are three materially different products:

1. **Fixed-page preview.** Produce pages and show them like a PDF. Layout is stable, scrolling is predictable, and the existing viewer can be reused. Text selection and DOCX-specific semantics may be limited.
2. **Flow/HTML preview.** Convert WordprocessingML into HTML/CSS. It can preserve semantic headings and runs, but CSS pagination is approximate and does not naturally provide the same page virtualization and fidelity as PDF.
3. **Native DOCX document surface.** Parse OOXML, calculate Word-like layout, retain an editable model, hit-test text, and render pages. This is the only route to high-quality native selection/editing, but it is a large new engine.

For the request “features like PDF viewer but for DOCX,” the first definition is the closest product match.

## Candidate approaches

| Approach | What it does | Strengths | Costs and risks | Recommendation |
| --- | --- | --- | --- | --- |
| **LibreOffice headless conversion** | Run `soffice --headless --convert-to pdf:writer_pdf_Export --outdir <private-dir> <file.docx>`, then open the result in `PdfView`. LibreOffice documents `--headless`, `--norestore`, `--convert-to`, and PDF filter parameters in its official help. | Mature DOCX import; broad real-world coverage; good development oracle; simple adapter boundary. | Requires a separately installed and versioned Office runtime; startup latency and process management; output varies with fonts and LibreOffice version; packaging and MPL/LGPL component obligations must be reviewed; an untrusted file is handed to a large external parser. | **Use for development/qualification and optional fallback, not as the mandatory shipped dependency.** |
| **`rdocx` (Rust)** | A high-level Rust crate that opens DOCX and exposes PDF, PNG, HTML, and Markdown output. Its published API includes `Document::open`, `save_pdf`, and deterministic page-image rendering; it advertises no LibreOffice or C dependency. | Fits a single Rust binary; page layout and PDF output are in-process; edition 2024 and a broad document model; potentially easy to call from a background task. | Version is young and fast-moving; official material calls the ecosystem a changing “stable family”; fidelity claims need independent corpus testing; dependency graph is large; license and transitive notices need review; page rendering is not yet a proven drop-in replacement for Word. | **Best first native candidate to spike**, behind an adapter and feature flag until the corpus gate passes. |
| **`docxide-pdf` (Rust)** | A focused DOCX-to-PDF library/CLI. Its README lists text styling, images, headers/footers, footnotes, fields, hyperlinks, columns, shapes, and font handling; it is Apache-2.0. | Narrow purpose; no Office installation; PDF output maps directly to `gpui-pdf`; explicit font and layout work; small conceptual seam. | The project labels itself work in progress; complex scripts, charts, SmartArt, and some wrapping are incomplete; Word-parity is not established; dependency and CPU/memory behavior need measurement. | **Strong alternative spike** if `rdocx` is too broad or its API is unsuitable. |
| **HTML/CSS (`docx-preview` / Mammoth-style)** | Parse DOCX in JavaScript and render DOM/CSS, often in a browser/webview. `docx-preview` explicitly says it is limited by HTML capabilities and that real-time page breaking is not implemented. | Good semantic HTML; mature JavaScript ecosystem; convenient for a web surface. | Adds a browser/webview or JS runtime to a native GPUI app; CSS pagination and font differences; weak fit for current page virtualization; untrusted HTML/resource handling; more packaging complexity. | **Do not choose for the native first release.** |
| **Native document engine (OpenDoc-style)** | Parse OOXML into a normalized model, paginate, and rasterize directly. OpenDoc describes a backend-neutral display list, CPU raster renderer, headers/footers/tables, and an unfinished pre-release status. | Long-term path to selection, semantic search, editing, and native rendering without an Office runtime. | Very large scope; pre-release API; fidelity gaps remain; GPU/native desktop shell is unfinished; would duplicate responsibilities already solved by the PDF pane. | **Long-term investigation only.** |
| **Commercial engine (Aspose.Words, etc.)** | Use a vendor layout engine to render DOCX directly or export PDF. Aspose documents Windows, Linux, and macOS support and says Word is not required. | Broad format support and vendor support; likely strongest compatibility among turnkey engines. | Commercial licensing, distribution terms, native ABI/bindings, binary size, and budget; not aligned with mdoc's current open-source dependency posture. | **Only if product requirements justify a commercial dependency.** |
| **Remote/self-hosted office service (ONLYOFFICE, etc.)** | Send a document to a document server and embed its web viewer. ONLYOFFICE documents a self-hosted Document Server and JavaScript API. | Rich Office compatibility and editing features. | Violates mdoc's local file-first shape unless the user operates a server; network/privacy/auth lifecycle; web embedding; operational burden. | **Out of scope for this desktop viewer.** |

Sources: [LibreOffice start parameters](https://help.libreoffice.org/latest/en-US/text/shared/guide/start_parameters.html), [LibreOffice PDF filter parameters](https://help.libreoffice.org/latest/en-US/text/shared/guide/pdf_params.html), [`rdocx` documentation](https://docs.rs/crate/rdocx/latest), [`rdocx` repository](https://github.com/tensorbee/rdocx), [`docxide-pdf` repository](https://github.com/sverrejb/docxide-pdf), [`docx-preview` repository](https://github.com/VolodymyrBaydalka/docxjs), [OpenDoc repository](https://github.com/CasualOffice/opendoc), [Aspose.Words system requirements](https://docs.aspose.com/words/cpp/system-requirements/), [ONLYOFFICE self-hosted installation](https://api.onlyoffice.com/docs/docs-api/get-started/installation/self-hosted/).

## Recommended architecture

Keep the conversion boundary small and application-owned:

```text
DOCX path
  -> background conversion adapter
  -> private temporary PDF + conversion metadata
  -> existing gpui-pdf::PdfView
  -> side pane
```

Suggested types (names are illustrative, not an API commitment):

```rust
struct DocxPreview {
    source: PathBuf,
    pdf_path: TempPath,
    converter: ConverterIdentity,
}

fn render_docx_preview(path: &Path) -> Result<DocxPreview, PreviewError>;
```

The adapter should accept a path and return an owned temporary artifact. It should not modify the source, the Markdown document, or the existing `import::convert` contract. The temporary directory must remain alive for the whole `PdfView` lifetime; deleting it immediately after constructing the entity would race the viewer's asynchronous file read.

The workspace state should replace `pdf: Option<Entity<PdfView>>` with either a small enum or paired fields that make ownership explicit:

- no preview;
- loading DOCX preview;
- PDF preview with an optional source path;
- DOCX preview with its temporary-artifact owner;
- preview error.

When a new preview is opened, first release the old `PdfView` textures (`PdfView::release`) before dropping it, then replace the temporary owner. On **Close PDF/Preview**, release textures and drop the owner. This avoids stale GPU atlases and keeps cleanup deterministic.

The preview conversion should follow the existing import lifecycle rules:

- one background conversion at a time;
- the editor remains responsive while conversion runs;
- tag the job with `document_generation` (and preferably a preview generation) so stale completion cannot replace a newer preview;
- failures leave the current editor and preview untouched;
- opening a DOCX as a preview does not create or overwrite a Markdown document;
- the existing **Import as Markdown…** action remains separate and continues to produce an unsaved Markdown document;
- if the same DOCX is imported and previewed, conversion may be deduplicated later, but that is not needed for the first slice.

## User-facing behavior

The smallest coherent feature is:

- **Open** accepts Markdown, PDF, and DOCX.
- Opening a DOCX keeps the Markdown editor as-is and shows a “DOCX Preview” pane.
- The pane uses the same zoom, page navigation, scroll, search, and theme behavior as PDF where those features apply.
- The toolbar shows “Converting…” while the preview is being prepared, then “Close Preview”.
- The source DOCX remains untouched. There is no Save action for the preview.
- A conversion error names the missing converter or the unsupported/corrupt document and tells the user that **Import as Markdown…** is a separate text-extraction path.
- Links inside the generated PDF should follow the existing PDF policy. Do not automatically execute macros, external commands, or embedded objects.

The file association and MIME changes should be made only after the behavior is implemented and tested: add `.docx` and `application/vnd.openxmlformats-officedocument.wordprocessingml.document` to Open's accepted types and the desktop entry. Do not claim `.docm` support until macro-bearing packages have an explicit policy.

## Security and robustness requirements

DOCX is a ZIP/OOXML package containing XML, relationships, images, fonts, and potentially external or active content. The preview path must:

- enforce file-size, ZIP-entry-count, decompressed-size, and nesting limits before conversion;
- reject path traversal and never extract an entry outside a private temporary directory;
- disable or ignore macros, OLE/ActiveX, external template links, remote relationships, and external images unless a future design explicitly allows them;
- use a unique per-job temporary directory with restrictive permissions;
- pass an isolated LibreOffice user profile when the optional converter is used (`-env:UserInstallation=...`), and include `--headless --norestore`;
- avoid displaying or logging document contents in errors;
- clean temporary artifacts on success, failure, replacement, and application close;
- bound conversion time and surface a timeout as a normal preview error;
- treat fonts as an input to fidelity: record whether output used embedded, system, or fallback fonts.

For a native Rust converter, these limits still apply at the package-reader boundary. “Pure Rust” reduces deployment dependencies; it does not by itself make malformed ZIP/XML safe.

## Proposed implementation phases

### Phase 0: corpus and oracle

Create a checked-in or privately sourced qualification corpus: headings and styles, tables with merged cells, nested lists, page/section breaks, headers/footers, footnotes, hyperlinks, images with wrapping, landscape sections, CJK and right-to-left text, equations, comments/track changes, embedded fonts, malformed ZIP/XML, encrypted/password-protected packages, macro-bearing `.docm`, and very large documents.

Use Microsoft Word exports where available and LibreOffice output as a reproducible open-source oracle. Record page count, rendered page images, extracted text order, hyperlinks, and conversion time/memory. Keep confidential documents out of the repository.

### Phase 1: adapter spike

Implement a non-UI `docx_preview` module with the converter behind a compile-time or runtime choice. Start with LibreOffice on developer machines to validate the lifecycle and fixture expectations, then spike `rdocx` and `docxide-pdf` against the same corpus. Measure:

- page-count difference;
- visual difference at 100% and fit-width;
- text/search behavior in the resulting PDF;
- cold and warm conversion latency;
- peak memory and temporary disk use;
- behavior on macOS, Windows, and Linux;
- binary size and startup impact;
- license/notice obligations and dependency health.

Do not integrate a converter into the UI until one candidate passes the minimum corpus gate.

### Phase 2: application integration

Add the preview state machine to `src/main.rs`, keep ownership of the temporary PDF beside the `PdfView`, and reuse the existing pane. Add tests for stale results, replacement, cleanup, errors, and unchanged Markdown. Add human checks for native Open shortcuts and visual fidelity; headless GPUI tests are flow evidence, not visual approval.

### Phase 3: packaging and fallback policy

Choose one of these explicit policies and document it in the README and installers:

1. **Self-contained:** ship a native Rust converter and no Office runtime.
2. **Optional converter:** use native conversion when available, otherwise offer LibreOffice if detected, otherwise explain that preview is unavailable while Markdown import remains available.
3. **LibreOffice required:** only acceptable if the product is willing to add an installation prerequisite and platform-specific packaging/support work.

The recommendation is policy 1 if a native candidate passes the corpus gate; policy 2 is the safest interim distribution policy while that work is evaluated.

## Acceptance matrix

| Area | Minimum acceptance evidence |
| --- | --- |
| Lifecycle | Open DOCX, show progress, replace/close preview, and open another preview without stale content or leaked temporary files. |
| Editor safety | Markdown text, dirty state, source path, and unsaved-change prompts are unchanged by preview conversion. |
| Layout | Page count and representative rendered pages match the selected oracle within an agreed tolerance; tables, sections, headers/footers, images, and breaks are covered. |
| Text | PDF search finds expected text in reading order; non-Latin scripts and fallback fonts are checked. |
| Performance | Conversion is off the UI thread; latency, memory, and disk bounds are recorded for small, medium, and large documents. |
| Errors | Missing converter, corrupt package, encrypted package, unsupported features, timeout, and permission failure produce actionable errors and preserve the current state. |
| Security | Package limits, traversal rejection, no macro/OLE/remote fetch execution, isolated converter profile, and cleanup are tested. |
| Platforms | macOS, Windows, and Linux build and run the same adapter contract; platform-specific converter discovery is documented. |
| Licensing | Direct and transitive licenses are reviewed; notices and shipped binaries comply with the chosen policy. |
| Existing gates | `cargo fmt --check`, `cargo clippy --workspace --all-targets -- -D warnings`, `cargo test --workspace`, and `git diff --check` pass. |

## Decisions to make before implementation

The implementation should wait for an explicit decision on three points:

1. Is the required first release a **read-only fixed-page preview**, or must it support selectable/editable DOCX content?
2. Is a self-contained binary a hard requirement, or is an optional LibreOffice installation acceptable during the transition?
3. What fidelity target is acceptable: “legible and structurally close,” “LibreOffice-equivalent,” or “Word-export-equivalent” for a defined corpus?

Given mdoc's file-first, cross-platform, no-native-runtime posture, the research supports a read-only fixed-page preview with a native converter behind a narrow adapter, using LibreOffice only as an oracle or explicit fallback.
