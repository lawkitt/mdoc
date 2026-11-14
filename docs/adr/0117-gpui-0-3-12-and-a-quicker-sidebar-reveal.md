# GPUI 0.3.12 and a quicker sidebar reveal

Status: Accepted, 2026-11-14. Amends
[ADR 0027](0027-chrome-style-vertical-sidebar.md) (the hover reveal's
timings). Replaces the ROADMAP Later item on GPUI Fast.
Record: [gpui-grill-2026-11-14](../design/gpui-grill-2026-11-14.md).

## Problem

The desktop app is on `gpui-pre 0.3.2` (Zed `801c087`). It carries vendored
macOS, Windows and WGPU crates, and its minimum is macOS 11. Newer packages
exist: `gpui-pre 0.3.12`, GPUI Fast 0.1.15 on the same Zed revision, GPUI Kit
0.9.1 and GPUI CE 0.3.0. None has been measured in mdoc.

The maintainer finds the left sidebar's **hover reveal** from the collapsed
rail not responsive, smooth or quick enough. The reveal has two separate costs:

- **Deliberate waiting.** A 200 ms hover-intent delay, then a 150 ms slide,
  so the list is fully shown about 350 ms after the pointer arrives.
- **Per-frame cost.** The sidebar renders inside `Workspace`, the root view.
  Every frame of the slide rebuilds the whole window, including the active
  document's editor and PDF preview. None of them uses GPUI's view cache
  (`.cached()`), which `gpui-pre` has offered since 0.3.2.

## Decision

- **Upgrade to `gpui-pre 0.3.12`**, as one change that keeps behavior on
  macOS and Windows. Migrate `ScrollEvent`/`ScrollSource`, the fallible
  headless renderer factory, the text wrapper and Taffy 0.15. Check wrap,
  caret and scroll by eye even where the code compiles unchanged.
- **The macOS minimum becomes 12.0**, which 0.3.12's Metal shader build
  targets. mdoc has no users yet, so the break needs no transition. Update
  the packager configuration and `Info.plist` together.
- **Keep vendoring.** Re-port the macOS and Windows patches (Save As reason,
  deferred quit, menu bar shelf, text-plus-file drag, restored-text clipboard,
  `MDOC_KEEP_RENDERING`, and Windows' windowless DirectWrite plus WARP
  factory). Keep the WGPU combining-mark override until a published release
  contains Zed #66799 and the check passes. Upstreaming the patches is
  optional later work.
- **Fix the reveal app-side first.** While the sidebar's toggle or reveal
  slides, render the document view through `.cached()`, so animation frames
  reuse the document's last frame. Only while it slides: a cached view
  redraws only when notified itself, and the document reads shared state
  (the theme's cell, notices) that changes without notifying it. Each reveal
  slide ends with a repaint that draws the document uncached, and reduced
  motion never caches. *Amended 2026-11-14 in #880; the decision first said
  a standing cache boundary.* Shorten the reveal to a **100 ms** intent delay and a
  **120 ms** slide. The close grace stays 300 ms. The maintainer confirms the
  new timings by eye in a build.
- **GPUI Fast only behind a gate.** If the reveal still misses the target
  below on 0.3.12 with the cache in place, A/B Fast 0.1.15 on the same Zed
  revision. Check stale pixels, caret and popup placement, input,
  accessibility and memory as well as speed. Otherwise Fast is not adopted.
- **GPUI Kit is deferred.** mdoc already has its own controls, focus cycling
  and accessibility roles. Kit is reconsidered when a specific new control
  would be expensive to build. If Kit is adopted, use `gpui-base` with
  mdoc's styling, follow Kit's exact `gpui-pre` pin, start with the control
  that motivated it, and replace text inputs last.
- **Out of scope:** GPUI CE, a Zed Git pin, `gpui-unofficial`, and frontend
  replacements (Iced, Slint, Tauri).
- **A resize rule, if resizing is ever slow:** the sidebar edge moves on top
  and the content reflows once on release, or at most every ~100 ms. Resize
  was not reported slow.

## Done criteria

A native release build shows no hitch over one frame (16.7 ms at 60 Hz)
during the rail's hover reveal and its close. It is measured with Instruments'
Animation Hitches and a driver like the
[editor-layout evidence's](../evidence/editor-layout/2026-10-17-native/README.md)
`drive.swift`, on a real legal document with the preview open. Windows gets a
smoothness check on the same document. The numbers go under
`docs/evidence/`.

## Invariants

- One GPUI type family across every desktop crate (app, editor, PDF viewer,
  BiDi adapter).
- [ADR 0049](0049-measured-stabilization-and-architecture-review.md) budgets:
  release builds on real hardware. Debug and headless timings expose costs
  but are not the target.
- The offscreen harness and its temporary profile keep working on both
  platforms. Windows renders stay windowless, with neither an HWND nor a
  swap chain.
- ADR 0027's other reveal behavior is unchanged: it closes when the pointer
  leaves, re-entry cancels the close, it stays open during a context menu or
  drag, focus in the rail reveals it at once, and it never changes the saved
  choice.

## Consequences

The vendored crates are re-ported once, at a 10-version jump. The upgrade
also positions a later Fast A/B on the same Zed revision. A cached document
view can hide a missing `cx.notify()` as a stale frame, so the cache is held
only for a slide, and a rendering test counts document renders while the
workspace redraws during a slide, at its end and with reduced motion.
