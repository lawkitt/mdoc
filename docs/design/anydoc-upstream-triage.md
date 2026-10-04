# AnyDoc upstream PR and issue triage

Status: three-PR scope, port policy, delivery, validation and final shared
understanding accepted, 2026-10-04. Implementation authorized.

## Confirmed scope

The user requested a quick grilling of every open upstream PR and issue to decide
what benefits mdoc, with or without modifications. Record decisions before acting.

The user refined the goal to a lightweight PDF/Office-to-Markdown utility and
selected only #17, #177 and #174. All other PRs and issues are outside this update.
The selection prioritizes readable Word forms, retained DOCX option states and
correct final text from tracked revisions in legacy DOC files.

Selected behavior:

- #17: unwrap a single-cell outer table containing a nested table. Keep ordinary
  single-cell data tables and genuine multi-cell nested-table behavior intact.
- #177: retain known Wingdings/Wingdings 2 checkbox states in DOCX, including
  private-use codes, styling and order. Do not invent mappings for unknown symbols.
- #174: omit revision-deleted DOC text while retaining insertions and ordinary
  strikethrough. Preserve field/table structure and omit deleted note content.

See [ADR 0008](../adr/0008-selected-anydoc-fidelity-fixes.md).

## Verified baseline

- GitHub API snapshot: 64 open PRs and 39 open issues, including PR #186.
- Upstream main: `261fc257d17c3eab0f673be31c408fd9fdc2171a`; latest commit dated
  2026-08-28. This confirms the backlog is ahead of the infrequently updated main.
- mdoc pins `lawkitt/anydoc` at `391b6b109b21c051a0edd726755352d78df3853f`.
  The fork's initial snapshot has upstream main's source behavior; its only
  additional functional commit ports #153 (`Ocr::Skip` with named skipped pages).
- Local AnyDoc checkout HEAD is older (`3167ca3`), so analysis used the actual
  pinned revision, fetched from origin. No checkout or source changes there.
- `src/import.rs` detects PDF headers and calls pdf-inspector directly. AnyDoc
  handles other formats and contributes format detection. AnyDoc PDF-only fixes
  do not change the active mdoc PDF import path.
- mdoc's current pdf-inspector pin is separate, with its upstream update recorded
  in ADR 0007. Preserve that work rather than adopting competing dependency bumps.
- Existing upstream source already implements spreadsheet display formats and
  hidden rows/columns/sheets. Open PRs #39/#90/#72 include stale alternatives.
- Reviewed source patches for the fidelity/hardening/structural candidates.
  This is suitability triage, not completed code review, tested adoption, or a
  claim that authors' reported validation establishes mdoc/native acceptance.

## Design tree

Initial frontier, superseded by the user's three-PR selection:

1. Core text fidelity and producer recovery: #179/#177/#176/#174/#171/#180/#46/
   #54/#158/#42. Retain submitted semantics where verified; bound repair narrowly.
2. Parser hardening: #148/#107/#44/#47/#169. Reconcile overlapping RTF bounds,
   retain fatal resource errors, and reject crashes/text loss on controlled inputs.
3. Visible structure: #17/#32/#184; adapt #129 only with evidence distinguishing
   legitimate independent sequences from continuation.
4. New input formats: decide HTML #147 and EML #164 separately; MHTML/passwords
   and less relevant formats are follow-up features.
5. Embedded images/model APIs: choose separate design or defer, because importing
   asset:N markers alone leaves bytes unavailable in saved/copied Markdown.

Final round, accepted with "design confirmed, implement":

1. Port the selected functional behavior and regression coverage substantially
   unchanged, allowing mechanical integration adjustments; a material behavior
   departure needs a new decision.
2. Delivery and validation: fork review PR, exact tested mdoc pin and
   lockfile through an mdoc review PR; fork checks, focused mdoc import regression
   tests and before/after fixture comparisons.
3. Final shared understanding confirmed; proceed with implementation.

New-format, asset-management and other candidate branches are excluded by the
user's explicit scope, not awaiting further decisions in this session.

## Implementation outcome

Ported only #17, #177 (both commits) and #174 with source SHAs/author attribution.
No conflicts or functional adjustments were needed. The new nested-table snapshot
retains current-base header inference. mdoc's exact tested pin is
`97d21d1f46086d84478da072ba1fdfa210461c80`; no transitive changes were retained.

See [verification](../../tests/fixtures/import/anydoc-fidelity-verification.md)
for fixture comparisons and checks, and the
[fork PR](https://github.com/lawkitt/anydoc/pull/1) for the ported changes.

## Open PR inventory

The inventory preserves the initial research recommendations. Only #17, #177 and
#174 are selected; every other item is outside the confirmed update scope. PR head
SHAs freeze the inspected snapshot; recheck changes before implementation.

| PR | Title | Proposed disposition | Head SHA |
| --- | --- | --- | --- |
| [#186](https://github.com/firecrawl/anydoc/pull/186) | feat(model): render and build documents outside the crate) | Defer embedded assets until model rendering, retention, save and handoff design settles | `fcd14aaeb54cdfd845903e64ff5ceb25d620a7ed` |
| [#185](https://github.com/firecrawl/anydoc/pull/185) | fix: expand typographic ligatures instead of dropping them (#172) | Investigate ligature issue in active pdf-inspector path; AnyDoc PDF patch cannot fix mdoc PDF path | `312b6c65e48a2f9014deab9e0bdadf2303b77e60` |
| [#184](https://github.com/firecrawl/anydoc/pull/184) | fix: keep bare numbered levels as ordered markers | Candidate: narrow structural improvement | `32dc831724448e87036b36c482d30ca0247cc71e` |
| [#183](https://github.com/firecrawl/anydoc/pull/183) | feat(wasm): send a pdf that needs ocr to firecrawl parse | Out of this update: OCR infrastructure already provided locally by mdoc; no hosted processing | `c32c4fe5c82afae216c276b0d64d35b05dba20e0` |
| [#181](https://github.com/firecrawl/anydoc/pull/181) | fix(deps): update pdf-inspector for RTL text extraction fix | Defer AnyDoc-only PDF updates: mdoc routes PDFs directly to pdf-inspector | `79c453c3ed8c8363be57a1bda4b263ebbd5011d3` |
| [#180](https://github.com/firecrawl/anydoc/pull/180) | fix(markdown): slug a heading line break as the space it renders | Candidate: fidelity; preserve submitted behavior, subject to review/tests | `f959218d43e888ee4e80f41b75a74fa761c3df15` |
| [#179](https://github.com/firecrawl/anydoc/pull/179) | fix(docx): keep non-breaking hyphens | Candidate: fidelity; preserve submitted behavior, subject to review/tests | `21eecd9eceb67f459c29a79a090d64a5dc8e962b` |
| [#177](https://github.com/firecrawl/anydoc/pull/177) | fix(docx): preserve symbol checkbox states | Candidate: fidelity; preserve submitted behavior, subject to review/tests | `2f0907d3bb8920e4b44e39443d575db4ce2eb59a` |
| [#176](https://github.com/firecrawl/anydoc/pull/176) | fix(doc): preserve symbol checkbox states | Candidate: fidelity; preserve submitted behavior, subject to review/tests | `6b6bd4333a9369235e1df7f4575ce674f5979bce` |
| [#175](https://github.com/firecrawl/anydoc/pull/175) | fix(pdf): bump pdf-inspector to 1.20.0 so RTL text extracts in logical order | Defer AnyDoc-only PDF updates: mdoc routes PDFs directly to pdf-inspector | `3863d520c10c8889759e5a6ccddd159a6b4e035e` |
| [#174](https://github.com/firecrawl/anydoc/pull/174) | fix(doc): omit deleted revision text | Candidate: fidelity; preserve submitted behavior, subject to review/tests | `96d59988d6e9f0b946d7849c1d64d80b61680502` |
| [#171](https://github.com/firecrawl/anydoc/pull/171) | fix(markdown): preserve link and image destinations | Candidate: fidelity; preserve submitted behavior, subject to review/tests | `72d699b923caf439cde235328ed976386f4b4fe4` |
| [#169](https://github.com/firecrawl/anydoc/pull/169) | fix(package): classify recoverable allocation failures as resource limits | Candidate: hardening; reconcile overlapping bounds and retain recovery policy | `3e22d20efab60f65a901cf92292bca49eb3809c6` |
| [#168](https://github.com/firecrawl/anydoc/pull/168) | feat(python): ship the anydoc CLI as a console script | Out of mdoc library adoption scope: binding/distribution/CLI/docs changes | `62519d15db6e9f53c34b1e537394e514998b5305` |
| [#166](https://github.com/firecrawl/anydoc/pull/166) | fix(pdf): do not discard a text-based document over one confirmed OCR page | Defer AnyDoc-only PDF updates: mdoc routes PDFs directly to pdf-inspector | `10610207e256747b45afc71ade3b79ac9a0fc33b` |
| [#164](https://github.com/firecrawl/anydoc/pull/164) | feat(eml): read RFC 5322 email messages | Separate feature decision: MIME/email frontend; security and output policy review required | `af00979796307ee5059b003927a3fde1f4139d00` |
| [#163](https://github.com/firecrawl/anydoc/pull/163) | feat(sheet): preserve spreadsheet provenance | Defer structured-model-only metadata; mdoc currently consumes Markdown | `d252d163181c1867d8dcf6a0c22ec9b8f8ba1cb7` |
| [#161](https://github.com/firecrawl/anydoc/pull/161) | feat: OCR scanned PDFs with a vision model via LiteLLM | Out of this update: OCR infrastructure already provided locally by mdoc; no hosted processing | `fdc7f3ff3c372b024ce83ce7ddc23a13ea6c1756` |
| [#158](https://github.com/firecrawl/anydoc/pull/158) | fix(xml): keep a part whose text carries a bare ampersand | Candidate: fidelity; preserve submitted behavior, subject to review/tests | `ca9fdc82cd0286a589040e9c6c031d293b11f44d` |
| [#154](https://github.com/firecrawl/anydoc/pull/154) | api: mark Format as #[non_exhaustive] | Defer API future-proofing: no immediate user-facing improvement | `7a5c8bab003fd58e6f38dcb6c93cc787c05cc086` |
| [#153](https://github.com/firecrawl/anydoc/pull/153) | feat(pdf): convert the readable pages when others need ocr | Already present: Ocr::Skip conversion port is fork commit 391b6b1 | `76c444fd1df4df5eb824b55fd3ad144347c839a2` |
| [#152](https://github.com/firecrawl/anydoc/pull/152) | docs: fix WebAssembly test link | Out of mdoc library adoption scope: binding/distribution/CLI/docs changes | `a6f3aba17dbc5ba54a644b9f14bf067f55d4caae` |
| [#151](https://github.com/firecrawl/anydoc/pull/151) | fix(xlsx): retain embedded worksheet images | Defer embedded assets until model rendering, retention, save and handoff design settles | `11c8c141d62a0924e9fed41f264457baf30a80d2` |
| [#150](https://github.com/firecrawl/anydoc/pull/150) | docs: add AnyDocSwift community binding to README | Out of mdoc library adoption scope: binding/distribution/CLI/docs changes | `69a8139088b22c7aaf1f2912afcf7a443c47691f` |
| [#149](https://github.com/firecrawl/anydoc/pull/149) | feat: add MHTML support | Defer MHTML until HTML and local embedded-assets decisions settle | `cfd9b80aee474082cddeb2158e87c2d98cf42218` |
| [#148](https://github.com/firecrawl/anydoc/pull/148) | fix: bound parser paths reachable from a crafted document | Candidate: hardening; reconcile overlapping bounds and retain recovery policy | `f8c87cc73ab6651b8e1a9355e8332902c93e182a` |
| [#147](https://github.com/firecrawl/anydoc/pull/147) | feat: add standalone HTML support | Separate format decision: prefer tolerant HTML frontend; includes added parser dependencies | `3f1bb626cfc214ab9ad9eedb74e32b2c3211aefa` |
| [#145](https://github.com/firecrawl/anydoc/pull/145) | docs: fix Node OCR README example | Out of mdoc library adoption scope: binding/distribution/CLI/docs changes | `c52b46bd757e00d73e23c919715dc9eea767a70e` |
| [#130](https://github.com/firecrawl/anydoc/pull/130) | feat: decrypt password-protected OOXML when a password is supplied (#102) | Defer password/decryption API and app password lifecycle; compare alternatives | `5e0f81efbf25e3b6df40d963e5dc396982be3135` |
| [#129](https://github.com/firecrawl/anydoc/pull/129) | fix(docx): continue counters across numIds that share an abstract (#96) | Adapt only after Word/reference evidence; shared abstractNum does not alone prove one logical sequence | `52dfa5ebb8082a3343735793f409af24ae914abe` |
| [#126](https://github.com/firecrawl/anydoc/pull/126) | feat(pptx): expose slide boundaries as per-slide anchors"  | Defer structured-model-only metadata; mdoc currently consumes Markdown | `ab040907f6a4da5cd45364cee1c811267011211e` |
| [#107](https://github.com/firecrawl/anydoc/pull/107) | fix: bound CSV and RTF materialization | Candidate: hardening; reconcile overlapping bounds and retain recovery policy | `b5cf3a1988730c4610d3cd7eece296d864f18f3b` |
| [#103](https://github.com/firecrawl/anydoc/pull/103) | feat: accept passwords for encrypted OOXML files | Defer password/decryption API and app password lifecycle; compare alternatives | `d3e6b4e53f120b0b901c3ecc86ab43c71dd09b3c` |
| [#98](https://github.com/firecrawl/anydoc/pull/98) | docs: list community Java bindings | Out of mdoc library adoption scope: binding/distribution/CLI/docs changes | `6847b768cf3e8bfe80a3a36f992674c1963ea72d` |
| [#95](https://github.com/firecrawl/anydoc/pull/95) | Expose slide boundary anchors for presentations | Defer structured-model-only metadata; mdoc currently consumes Markdown | `cb5a21e7daa659ff0ff15581bb282f0b23dfaba7` |
| [#91](https://github.com/firecrawl/anydoc/pull/91) | feat(pdf): add to_markdown_pages for per-page extraction | Defer AnyDoc-only PDF updates: mdoc routes PDFs directly to pdf-inspector | `5d0c50fac35f7f128b288b3ce8fb29a0d2c4dce0` |
| [#90](https://github.com/firecrawl/anydoc/pull/90) | fix(sheet): omit hidden rows and columns in OOXML xlsx | Already covered semantically by current upstream spreadsheet reader; do not import stale alternative implementations | `b6fe9a58a364299ebc9b1dd209d8e0bfeb18e0a5` |
| [#89](https://github.com/firecrawl/anydoc/pull/89) | feat(cli): add --batch directory conversion (#77) | Out of mdoc library adoption scope: binding/distribution/CLI/docs changes | `7b46c9f1a734598f4f70ce35b54b1c8b50f86d4d` |
| [#88](https://github.com/firecrawl/anydoc/pull/88) | docs: add SUPPORT.md and clarify known conversion limits | Out of mdoc library adoption scope: binding/distribution/CLI/docs changes | `5ebae8e5ea6223c10a8cca93ad66cbf267942426` |
| [#83](https://github.com/firecrawl/anydoc/pull/83) | feat(pdf): return positioned image files with Markdown | Defer embedded assets until model rendering, retention, save and handoff design settles | `2dcd50ddf83defa93f90f03a5fc3b3edffb43ea8` |
| [#75](https://github.com/firecrawl/anydoc/pull/75) | Add a Nix flake (anydoc as the default package) | Out of mdoc library adoption scope: binding/distribution/CLI/docs changes | `206f079d5e1e93b9db4ef372fef5102d574f66e6` |
| [#72](https://github.com/firecrawl/anydoc/pull/72) | fix(sheet): render xlsx number formats (percent, currency, thousands) | Already covered semantically by current upstream spreadsheet reader; do not import stale alternative implementations | `242337fa7b6ed774eabc521fbfc7ba38e7cbe258` |
| [#70](https://github.com/firecrawl/anydoc/pull/70) | feat(markdown): emit asset:N hrefs for embedded images | Defer embedded assets until model rendering, retention, save and handoff design settles | `dcdaa8fbbcc44a452be0375dbf57344404ef4d48` |
| [#69](https://github.com/firecrawl/anydoc/pull/69) | feat(sheet): expose worksheet name and used-range origin on Table | Defer structured-model-only metadata; mdoc currently consumes Markdown | `48736ad2bb3b9daad25df8e1eeeb7e7adb476688` |
| [#66](https://github.com/firecrawl/anydoc/pull/66) | feat: add reStructuredText (.rst) support | Defer new formats with lower immediate fit | `f67d4240fb5e4bcdb656874bda7365477ab9f884` |
| [#61](https://github.com/firecrawl/anydoc/pull/61) | WIP: feat(ocr): optional local OCR for scanned PDFs and image documents | Out of this update: OCR infrastructure already provided locally by mdoc; no hosted processing | `edbfbde2549c3259f369b176cf06abc4584daef2` |
| [#55](https://github.com/firecrawl/anydoc/pull/55) | Prevent CLI output from overwriting input | Out of mdoc library adoption scope: binding/distribution/CLI/docs changes | `196f1b21b11927df80ffb84d5d85ec3b78c7717a` |
| [#54](https://github.com/firecrawl/anydoc/pull/54) | Fix invalid EPUB spine references | Candidate: fidelity; preserve submitted behavior, subject to review/tests | `7dbb3e06e3afdaa5cab149d85dc8b3121fa08f07` |
| [#53](https://github.com/firecrawl/anydoc/pull/53) | feat: support standalone HTML documents | Defer alternative HTML frontend; XML leniency is less complete than HTML5 parsing | `b43c39b92cbd3339a5cd62c2c3be54839a5e97fe` |
| [#48](https://github.com/firecrawl/anydoc/pull/48) | feat: add support for .wps and .et | Defer new formats with lower immediate fit | `7dc319dec235da6e70238d2a4383836c7df3a7f6` |
| [#47](https://github.com/firecrawl/anydoc/pull/47) | rtf: single-pass prelude scan; cap group nesting depth | Candidate: hardening; reconcile overlapping bounds and retain recovery policy | `f197ed27a4e4cf1479bb22e550efbaccc678a986` |
| [#46](https://github.com/firecrawl/anydoc/pull/46) | Keep delimiters from pairing across runs in one paragraph | Candidate: fidelity; preserve submitted behavior, subject to review/tests | `75c4313a191169c9aa8877dda70898dc8b909c71` |
| [#44](https://github.com/firecrawl/anydoc/pull/44) | Parse each package part once, and bound the total | Candidate: hardening; reconcile overlapping bounds and retain recovery policy | `6c958c2539f500b19a796060064fcd2277e9c4e5` |
| [#42](https://github.com/firecrawl/anydoc/pull/42) | fix(ole): repair the zero-filled MiniFAT Apple's Cocoa exporter emits | Candidate: bounded producer-specific repair; verify Apple fixture and reject unrelated corrupt files | `fff0296ef42795efd7bdc5a87ff813225bf874e6` |
| [#40](https://github.com/firecrawl/anydoc/pull/40) | feat(docker): add multi-stage CLI image and CI smoke test | Out of mdoc library adoption scope: binding/distribution/CLI/docs changes | `990fd035bd8107c908911f320a774d398a543173` |
| [#39](https://github.com/firecrawl/anydoc/pull/39) | fix(sheet): skip hidden and very-hidden worksheets | Already covered semantically by current upstream spreadsheet reader; do not import stale alternative implementations | `b2208c3da7f2322c58756de825978e7c33b5d414` |
| [#32](https://github.com/firecrawl/anydoc/pull/32) | fix(presentations): separate slides with a thematic break | Candidate: narrow structural improvement | `4b0edae357470e7a663306adc02d451af8dbbfa5` |
| [#30](https://github.com/firecrawl/anydoc/pull/30) | feat: add Go bindings | Out of mdoc library adoption scope: binding/distribution/CLI/docs changes | `1a7a6c04ff8f2689bf8ebbd1e0d9d61d2d109164` |
| [#29](https://github.com/firecrawl/anydoc/pull/29) | feat(cli): add a Rust anydoc binary for cargo install | Out of mdoc library adoption scope: binding/distribution/CLI/docs changes | `87bbb883940936278ac5ef132bee9f86e112c728` |
| [#19](https://github.com/firecrawl/anydoc/pull/19) | feat(outline): preserve headings across document formats | Defer expanded outline/model API and title heuristics; declared-heading subset may be useful later | `e4f65cb9fba905c978bbab1a15c24968f6c2765d` |
| [#17](https://github.com/firecrawl/anydoc/pull/17) | fix(render): unwrap single-cell tables that wrap a nested table  | Candidate: narrow structural improvement | `35f278e516471cb96841af7e8b0a0b8cdf19649b` |
| [#16](https://github.com/firecrawl/anydoc/pull/16) | fix(sheet): preserve merged-cell spans that extend past the used range | Defer structured-model-only metadata; mdoc currently consumes Markdown | `067b07ddd8543081805473cd8ebd85484a833d9f` |
| [#7](https://github.com/firecrawl/anydoc/pull/7) | feat(markdown): resolve embedded asset URLs | Defer embedded assets until model rendering, retention, save and handoff design settles | `894d3747566c3a8b48295d2e031cd8dcd84a10ca` |
| [#4](https://github.com/firecrawl/anydoc/pull/4) | feat(epub): metadata front matter and figure caption dedupe | Defer EPUB metadata/frontmatter behavior; caption-dedup subset could be reviewed separately | `8ca81e9f6d65d6c4a440a4f50836c087f1e76f02` |

## Open issue inventory

| Issue | Title | Proposed handling |
| --- | --- | --- |
| [#182](https://github.com/firecrawl/anydoc/issues/182) | PPTX: images silently dropped in two mc:AlternateContent structures (oleObj preview not walked; first_descendant walks through AlternateContent) | Asset loss: investigate together with embedded-assets work; no open linked implementation selected. |
| [#178](https://github.com/firecrawl/anydoc/issues/178) | mobi support via libmobi | Defer MOBI/new native library. |
| [#173](https://github.com/firecrawl/anydoc/issues/173) | Multi-column tables collapse into unreadable single-column blobs during PDF conversion | Reproduce PDF table layout against mdoc current pdf-inspector; not an AnyDoc-frontend fix. |
| [#172](https://github.com/firecrawl/anydoc/issues/172) | Ligature characters (fi/fl/ffi) are dropped instead of expanded when extracting PDF text | Investigate active PDF path; related PR #185. |
| [#170](https://github.com/firecrawl/anydoc/issues/170) | anydoc-wasm still ships pdf-inspector 1.14.2 — the RTL extraction fix (pdf-inspector#440) isn't included | AnyDoc wasm dependency issue; mdoc PDF path independent. |
| [#167](https://github.com/firecrawl/anydoc/issues/167) | OOXML: recoverable allocation failure in Package::part is classified as malformed | Related hardening PR #169. |
| [#162](https://github.com/firecrawl/anydoc/issues/162) | 0.2.4's needsOcr gate refuses born-digital PDFs that 0.2.3 converts correctly | Related PR #166; active mdoc PDF path already separate. |
| [#157](https://github.com/firecrawl/anydoc/issues/157) | Feature request: local OCR provider hook (--ocr command) - delegate OCR to a user-supplied engine | Local OCR need covered by mdoc direct integration; do not add command hook. |
| [#156](https://github.com/firecrawl/anydoc/issues/156) | xlsx: resource limits hit on real workbooks with excess whitespace | Keep limits; investigate sparse XLSX used-range behavior separately, no blanket cap increase. |
| [#155](https://github.com/firecrawl/anydoc/issues/155) | Node binding retains large XLSX RSS high-water per libuv worker | Node worker RSS issue; outside native Rust integration. |
| [#146](https://github.com/firecrawl/anydoc/issues/146) | Inbuilt Image Parser with Local OCR (Unlimited OCR, PaddleOCR etc.) | Local PDF OCR already provided; standalone image OCR separate feature. |
| [#144](https://github.com/firecrawl/anydoc/issues/144) | 0.2.4: one image-only page makes the whole PDF convert to nothing, including the pages that do have text | Mixed PDFs handled by fork #153 port and active mdoc PDF path. |
| [#139](https://github.com/firecrawl/anydoc/issues/139) | XLSX embedded images are missing from Document.assets | Related PR #151; asset lifecycle prerequisite. |
| [#138](https://github.com/firecrawl/anydoc/issues/138) | benchmark with apache tika | Benchmark discussion; not a merge candidate. |
| [#128](https://github.com/firecrawl/anydoc/issues/128) | Support email file formats: `.eml` and `.msg` | PR #164 covers EML only; MSG still independent, no open #160 in this snapshot. |
| [#127](https://github.com/firecrawl/anydoc/issues/127) | EPUB conversion missing title page | Related XML recovery PR #158. |
| [#104](https://github.com/firecrawl/anydoc/issues/104) | CSV/RTF: bound cell/text materialization to prevent memory exhaustion | Related materialization-limit PR #107. |
| [#102](https://github.com/firecrawl/anydoc/issues/102) | Accept a password for encrypted OOXML files instead of terminating at `Encrypted` | Alternative password PRs #103/#130; app/API lifecycle decision required. |
| [#96](https://github.com/firecrawl/anydoc/issues/96) | DOCM to MD Conversion - Headings to not match expected numbering from document. | Related PR #129; numbering needs reference validation. |
| [#94](https://github.com/firecrawl/anydoc/issues/94) | Expose slide boundaries for PPTX/PPT (the anchor already exists, it is just gated) | Model-only slide anchors #95/#126; #32 affects Markdown visibly. |
| [#87](https://github.com/firecrawl/anydoc/issues/87) | Suggestion: include Xberg (formerly Kreuzberg) in the benchmark | Benchmark discussion; not a merge candidate. |
| [#86](https://github.com/firecrawl/anydoc/issues/86) | mm: unsupported input | Unsupported format report needs reproducible fixture; no implementation selected. |
| [#82](https://github.com/firecrawl/anydoc/issues/82) | Would you accept a PR adding PHP bindings? | PHP binding; out of scope. |
| [#81](https://github.com/firecrawl/anydoc/issues/81) | Bug: duplicate numbering prefix in DOCX ordered lists from programmatically generated files | Duplicate numbering report references closed #80; reproduce before considering stripping literal prefixes. |
| [#77](https://github.com/firecrawl/anydoc/issues/77) | Batch mode for bulk directory level processing | CLI batch processing; mdoc batch conversion is separate. |
| [#74](https://github.com/firecrawl/anydoc/issues/74) | JVM Bindings | JVM bindings; out of scope. |
| [#71](https://github.com/firecrawl/anydoc/issues/71) | Pure-Go bindings via wasm32-wasip1 + wazero (CGO_ENABLED=0) | Go/wasm bindings; out of scope. |
| [#63](https://github.com/firecrawl/anydoc/issues/63) | Expose embedded image assets in markdown output | Asset rendering PRs #7/#70; lifecycle prerequisite. |
| [#62](https://github.com/firecrawl/anydoc/issues/62) | PDF support via to_document / per-page extraction | AnyDoc PDF model/page extraction; mdoc uses pdf-inspector directly. |
| [#52](https://github.com/firecrawl/anydoc/issues/52) | Feature request: Support for HTML / MHTML files (e.g. Jira exports) | HTML/MHTML alternatives #53/#147/#149. |
| [#41](https://github.com/firecrawl/anydoc/issues/41) | docx to md puts a backslash between number and period in ordered lists | Literal/composite list labels may be intentional; reproduce before rewriting numbering. |
| [#38](https://github.com/firecrawl/anydoc/issues/38) | feat proposal: Restructured Text | RST PR #66; defer. |
| [#37](https://github.com/firecrawl/anydoc/issues/37) | doc: Apple textutil-authored .doc is rejected at open — cfb reports "Malformed MiniFAT (mini sector 0 pointed to twice)" | Bounded Apple OLE repair PR #42. |
| [#31](https://github.com/firecrawl/anydoc/issues/31) | Presentation slides are concatenated with no boundary, so untitled slides merge into the previous one | Visible slide separators PR #32; model anchors alone do not change Markdown. |
| [#24](https://github.com/firecrawl/anydoc/issues/24) | Rust CLI binary | Rust CLI PR #29; not required for app library. |
| [#23](https://github.com/firecrawl/anydoc/issues/23) | Compare with Apache Tika? | Benchmark discussion; not a merge candidate. |
| [#14](https://github.com/firecrawl/anydoc/issues/14) | Nested tables are flattened into a single cell, losing the inner grid | Narrow nested-wrapper solution PR #17; multi-cell nested grids remain flattened. |
| [#11](https://github.com/firecrawl/anydoc/issues/11) | Docker support please | Docker PR #40; out of scope. |
| [#10](https://github.com/firecrawl/anydoc/issues/10) | Expose worksheet identity and source coordinates in to_document() | Model provenance PRs #69/#163; no current Markdown consumer benefit. |

## Glossary

- Adopt substantially unchanged: retain the PR's functional behavior after source
  review and tests; mechanical conflict resolution does not establish suitability.
- Adapt: retain the useful idea while changing behavior, scope or integration.
- Covered: the current pinned source already provides the relevant behavior; this
  need not imply the open PR itself was merged.
- Defer: retain a candidate for a distinct decision; it is not approved for adoption.
