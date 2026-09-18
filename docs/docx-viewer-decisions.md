# DOCX viewer decisions

Companion to `docx-viewer-research.md`. Records the design interview; pending
recommendations are not implementation authorization.

## Confirmed: round 1

1. **Read-only fixed-page preview.** Convert DOCX to PDF and reuse the existing
   pane's navigation, zoom, and search. DOCX editing and text selection are
   outside the first release. Opening a preview must preserve Markdown text,
   path, dirty state, and its unsaved-change workflow. Import as Markdown
   remains a separate action.
2. **Self-contained distribution is required.** The shipped preview must work
   without an Office installation. LibreOffice is a development comparison
   tool only. If native converters fail qualification, revisit scope before
   shipping; do not introduce a runtime Office fallback implicitly.
3. **Practical library-led fidelity (revised in round 4).** Rely primarily on
   the selected Rust OSS library's rendering. Exact pagination, pixel matching,
   and LibreOffice-equivalent layout are not release requirements. Use the
   representative corpus to expose missing text, unreadable output, and library
   limitations, not to create a custom Word-compatible layout engine.

These choices reuse the existing native viewer while keeping deployment
self-contained and making fidelity an explicit qualification gate.

## Confirmed: round 2

4. **Required coverage.** Paragraphs/styles, Latin and Cyrillic, tables including
   merged cells, lists, images, page/section breaks, landscape pages,
   headers/footers, footnotes, and hyperlinks. Defer qualification of equations,
   charts/SmartArt, embedded fonts, CJK, and RTL. Detectable unsupported content
   must produce a warning or rejection; basic corpus success is not a claim of
   universal compatibility.
5. **Preview lifecycle.** Latest preview request wins and cancels superseded
   conversion. Retain the current preview until its replacement successfully
   loads; preserve it on failure. Closing cancels pending work. Markdown changes
   do not cancel standalone preview conversion. Reopening DOCX refreshes it;
   automatic file watching is outside the first release.
6. **Terminable native conversion.** Run the converter in a child process of the
   same executable for cancellation, timeout, and crash containment. This is
   not a security sandbox. Distribution remains self-contained.
7. **Shared PDF link policy.** Allow external HTTPS, HTTP, and mailto links;
   preserve internal page navigation; block other schemes. Apply the policy to
   both generated previews and directly opened PDFs.

## Confirmed: round 3 (paired import)

8. **First-release source previews cover PDF and DOCX.** Import as Markdown
   opens converted Markdown on the left and the original source preview on the
   right. Other supported import formats retain Markdown extraction and show
   an explicit "Source preview unavailable" state. Broader source preview
   coverage is deferred. Source files remain unchanged.
9. **Markdown success survives preview failure.** Accepted imports open as
   unsaved Markdown. Show the source filename, preview error, and Retry if its
   supported preview fails. Clear any unrelated previous preview when import
   is accepted, preventing a misleading pairing. Failed Markdown conversion
   or cancellation of the unsaved-change prompt preserves both existing panes.
   This intentionally overrides standalone preview failure preservation for
   accepted imports only.

## Confirmed: round 4

10. **Relaxed fidelity, performance priority.** The user rejected strict oracle
    parity and chose reliance on the Rust OSS renderer. Keep practical corpus
    checks and disclose known library limitations. The app must remain snappy;
    Rust alone does not establish responsiveness. Conversion, package validation,
    and filesystem cleanup must not block the UI thread.
11. **Initial DOCX preview limits.** 50 MiB input, 10,000 ZIP entries, 250 MiB
    total expanded content, and 30 seconds conversion time. Terminate the worker
    on cancellation/timeout. Reject nested archive expansion. Validate defaults
    during the spike and measure peak memory before selecting an enforceable
    cross-platform memory cap; do not claim an unimplemented cap.
12. **Package/content policy.** Reject encrypted or macro-bearing packages and
    tracked changes. Ignore external resources without fetching them. Warn about
    omitted embedded objects and comments. Ordinary hyperlinks follow decision 7.
    These preview policies do not silently expand or restrict the existing
    Markdown import format contract.

## Code findings informing subsequent rounds

- PDF opening bypasses Markdown unsaved-change prompts, and the preview
  survives Markdown document transitions. Preview jobs need a separate
  generation counter from Markdown imports.
- PDF loading is asynchronous. Preserving the old preview on failure requires
  validating/loading the replacement before committing it.
- Existing PDF replacement and close do not call `PdfView::release`; the
  viewer documents that explicit release is necessary to reclaim textures.
- Existing import runs in a background thread. There is no process termination
  infrastructure; a waiting timeout alone cannot stop native conversion.
- PDF URI links currently go directly to the operating system URL opener.
  A restricted link policy would be an intentional change to shared PDF behavior.

## Confirmed: round 5 and implementation handoff

13. **Converter and fonts.** Start with rdocx and its bundled fonts. Verify
    Cyrillic, search, external-resource handling, and performance. Evaluate
    docxide-pdf only if the spike identifies a concrete blocker.
14. **Responsiveness.** Target loading feedback within 100 ms and continued
    typing, scrolling, and closing responsiveness. Target first visible page
    within two seconds for a typical text-heavy ten-page DOCX on the measured
    development machine, including worker startup. These are measurement
    targets, not established performance claims. Allow one preview conversion
    at a time and terminate obsolete work. Accepted imports display Markdown
    without waiting for their source preview.

The user confirmed shared understanding and the implementation sequence:
converter/performance spike, preview and paired-import integration, full checks
and human visual review. Memory limits remain an evidence-dependent engineering
decision after spike measurements; no enforced memory cap is currently promised.
