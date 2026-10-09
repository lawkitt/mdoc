# Model setup UX

Design interview started 2026-10-09. Status: confirmed and implemented 2026-10-09. ADR:
[0026](../adr/0026-first-use-model-setup.md).

## Scope

First-use OCR and pseudonymization model setup (approval, progress, resume),
the Settings dialog's model section, and English-first default models.
Russian is no longer a priority; more languages arrive later as extra bundles.

## Facts (current code, 2026-10-09)

- OCR first use: the Markdown pane shows "Text recognition required" with
  **Set up OCR**; one click starts the download (no size, no confirmation) and
  pops the Settings dialog open for progress (`main.rs` `begin_ocr_setup`).
  A generation-scoped continuation resumes conversion after setup.
- Pseudonymize first use: the scan starts and fails with "Set up the
  experimental model before scanning."; the review panel then offers
  **Set up model (N MB)**, which downloads and pops Settings open
  (`pii/ui/mapping/panel.rs`). No scan continuation.
- Settings: per-row "Checking…" next to an enabled Download, plus a global
  "checking installed models" line; selection and installation look alike;
  progress covers the current file only; rows lead with engine names and
  Russian caveats; changes need Apply.
- Catalog: OCR Cyrillic (v6 detector + v5 Cyrillic recognizer, default) and
  PP-OCRv6 Small (~31 MB, exact English transcript, Russian failure);
  GLiNER2 FP16 (~632 MB, ~5.5 s, ~2.0 GiB peak, default) and FP32 (~1.25 GB,
  ~8.4 s, ~2.7 GiB peak, no evidence of better accuracy). Windows adds the
  ~77 MB shared ONNX runtime.
- Preferences store the concrete model, not a "default" marker.

## Round 1 — confirmed 2026-10-09

1. **Inline consent card** at the point of use (Markdown pane for OCR, review
   panel for Pseudonymize): what downloads, total size, offline-after-setup,
   **Download & continue** / **Cancel** / **Choose model…**. Progress is
   inline; the task resumes automatically. Settings no longer pops open on its
   own; **Choose model…** opens it.
2. **Pseudonymize with a missing model** shows the consent card instead of
   starting a scan that will fail, and scans automatically after setup.
3. **OCR default → PP-OCRv6 Small (English).** Cyrillic bundle stays as a
   non-default "English & Russian" option.
4. **Pseudonymization default stays FP16.** "Best available" means best
   measured: FP32 is 2× download, ~50% slower, more memory, no shown gain.
5. **No preference migration.** New defaults apply to fresh installs and
   Reset defaults; stored choices are untouched (ADR 0006: no silent
   substitution).
6. **Re-rank the existing catalog only.** Evaluating stronger English
   candidates is separate work needing qualification evidence; Settings should
   accommodate more bundles/languages later.
7. **Settings rows separate selected from installed**: one status label
   (Not downloaded · size / Downloading % · bytes / Verifying… / Installed /
   Unavailable (reason)); quiet placeholder rows while checking, no Download
   until known; a selected missing model reads "Downloads on first use" with
   Download; plain-language titles, engine names/caveats/revision/runtime in
   Details.
8. **Progress is total bundle bytes** across all files, then Verifying /
   Checking runtime, with Cancel — in the inline card and the Settings row.
   Real byte totals only; no invented percentages.
9. **Model choices save immediately**; Advanced numeric fields keep validated
   inline saving. Footer becomes **Reset defaults** and **Done**.
   Running jobs keep their configuration snapshot.
10. **Pseudonymization caveat** becomes English-first ("Experimental. Detection
    can miss names and identifiers — review the whole document before
    sharing."); language detail moves to Details.

## Round 2 — confirmed 2026-10-09

11. **OCR card replaces Set up OCR** inside the existing recognition choice:
    pages needing OCR, one-time download size, **Download & recognize**,
    **Use native text only**, **Choose model…**; installed → **Run OCR**.
    The user also asked for the Markdown pane to be polished (round 3).
12. **Cancel/failure** returns to the same card with an inline error and
    **Retry**; verified files are kept, so retry fetches only the rest. The
    Windows VC++ guidance stays, with Retry.
13. **Leaving during setup**: the download continues globally; the resume
    belongs to the requesting tab and is dropped when it closes or its
    document changes. Other tabs needing the model show its progress instead
    of their own card.
14. **Settings downloads never process a waiting document**; the card updates
    to one-click **Run OCR** / **Pseudonymize**.
15. **Choose model…** opens Settings scrolled to that feature; on close the
    card reflects the newly chosen model and size, or Run if installed.
16. **Row labels**: OCR "English — Recommended" (31 MB), "English & Russian"
    (18 MB); pseudonymization "Standard — Recommended" (632 MB), "Full
    precision" (1.25 GB, slower, more memory, not shown more accurate). A
    missing shared runtime is a separate "+77 MB shared runtime, once" line.
17. **Failed preference save** reverts the selection with an inline section
    error.
18. **Removing the selected model** is allowed (not while in use); its row
    reads "Not downloaded · Downloads on first use" and next use shows the card.

## Round 3 — Markdown pane polish, confirmed 2026-10-09

19. **One centred state card** (~440 px) for every not-yet-converted state:
    muted filename, clear title, one sentence, actions, quiet footnote
    ("English OCR · 31 MB, once · runs on this computer"). Converting,
    waiting, failed and ready-to-convert reuse it. Shared `ui::state_card`;
    the Pseudonymize card in the review panel reuses it at panel width.
20. **Button hierarchy**: one filled accent primary (Download & recognize /
    Run OCR), a quiet secondary (Use native text only), a text-link tertiary
    (Choose model…). The primary takes focus; Return activates it. Add a
    reusable primary style to `ui.rs`.
21. **The card changes in place**: idle → downloading (bar, "212 / 631 MB",
    Cancel) → Verifying / Checking runtime → recognizing (indeterminate,
    "Pages 3, 5") → failed (inline error, Retry, Use native text only).
22. **Page lists** compress to ranges with a count ("6 of 40 pages need
    recognition (3–7, 12)"), truncated with "…" when long.
23. **No duplicate top notice** while the Markdown pane is visible; keep it
    only when a narrow window shows Original alone, with **Show** switching
    to the Markdown pane.
24. **Visual touches**: a muted 24 px monochrome document glyph (new SVG),
    `sidebar_bg` card with subtle border and rounded corners, no animation
    beyond the existing spinner, both themes.

## Round 4 — confirmed 2026-10-09

25. **No first-launch onboarding**: setup is asked just in time; Settings
    remains the place to prepare ahead.
26. **Pseudonymize card copy**: "Set up pseudonymization" / "Detects names,
    organizations and identifiers on this computer. Experimental — review the
    whole document before sharing." / "Standard model · 632 MB, once" /
    **Download & scan**, **Cancel**, **Choose model…**.
27. **Acceptance**: focused gpui tests (missing model shows the card and never
    scans; Download & continue resumes and drops on tab close; cancel/failure
    shows Retry; Settings download does not run the waiting document;
    immediate save and revert on failure; Reset restores v6 Small/FP16 and
    stored preferences are untouched), the full repository gate, native macOS
    check of card and Settings in both themes at normal and narrow widths;
    the Windows native check is recorded as outstanding.

## Invariants

- No network access without an explicit user action naming size.
- Settings downloads never start document processing (ADR 0006); only the
  inline card's **Download & continue** carries a continuation.
- One model job at a time; continuations stay generation-scoped.

## Implementation notes — 2026-10-09

- Layout adjustments to fit one row per model at 600 px: a missing model's
  size is on its button (**Download · 31 MB**) instead of a "Not downloaded ·
  size" label, and the missing shared runtime is one sentence in the section
  description instead of a per-row line. "Downloads on first use" follows the
  selected row's summary.
- `model_download::Pending` measures missing bytes by file size (digests stay
  in setup/use); `State::overall` reports bundle bytes against that plan.
  Tests always report models as missing, so no test scans or downloads.
- `ui::state_card`, `primary_button`, `link_button`, `setup_progress` and
  `page_ranges` are shared by the OCR card and the Pseudonymize card.
- Verified: `cargo fmt --check`, `cargo clippy --workspace --all-targets -D
  warnings`, `cargo test --workspace`; native macOS (Apple Silicon) check of
  the OCR card, Settings (after model checks) and the Pseudonymize card in
  dark and light themes. Not yet checked: a live download through the cards,
  the narrow-window notice, and Windows native behaviour (outstanding).
