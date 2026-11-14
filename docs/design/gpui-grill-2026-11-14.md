# Desktop GPUI upgrade grill

Research as of 2026-11-14. Four rounds answered. The outcome is
[ADR 0117](../adr/0117-gpui-0-3-12-and-a-quicker-sidebar-reveal.md) and the
ROADMAP section "Desktop GPUI". The record stays open until the maintainer
confirms a shared understanding.

## Decisions so far

1. **Driver: desktop responsiveness.** The maintainer sees the right sidebar
   (the Replacements panel) as not responsive, smooth or quick enough. That
   symptom is the first target. Staying near upstream is a welcome side effect,
   not the driver.
2. **macOS 11 is not required.** `gpui-pre 0.3.12` is acceptable. mdoc has no
   users yet, so breaking changes to the support floor and packaging are fine.
3. **Scope: GPUI only.** No Iced, Slint or Tauri evaluation. A GPUI Kit
   primitive is considered only for a specific mdoc UI problem.

4. **The slow sidebar is the left one** (the document sidebar), not
   Replacements. The round 2 Replacements facts below remain true but are not
   the target.
5. **Sequence: upgrade to 0.3.12 first**, as one change that keeps behavior on
   both shipped platforms. Profile there. Check wrap, caret and scroll by eye.
6. **App-side fix before Fast.** On 0.3.12, give hot UI its own view boundary
   and cache unaffected views with upstream `.cached()`. Then measure. Run the
   Fast A/B only if the sidebar still misses its target.
7. **macOS floor: 12.0.**
8. **One change covers macOS and Windows.** The Windows windowless renderer
   factory needs the most care.
9. **Keep vendoring** the macOS, Windows and WGPU crates and re-port the
   patches. Upstreaming the Windows headless factory and the Save As reason is
   optional later work.
10. **Reuse GPUI Kit controls** to speed up UI development. This promotes
    "faster UI development" to a secondary objective. Round 3 settles the
    layer, the version coupling and the first controls.

11. **The slow interaction is the hover reveal from the collapsed rail.**
    The other sidebar interactions were not reported.
12. **Resize, if it is ever slow:** move the edge on top and reflow once on
    release, or at most every ~100 ms. Resize was not reported slow, so this
    is a fallback rule, not work.
13. ~~Kit layer: `gpui-base`.~~ Reopened by the maintainer's question in round
    3 ("what's the point of gpui-base?"). See round 4.
14. **Version coupling, if Kit is adopted:** mdoc follows Kit's exact
    `gpui-pre` pin.
15. **First Kit controls, if Kit is adopted:** the sidebar list as part of
    the sidebar fix, plus new UI; then menus and dialogs one at a time; text
    inputs last.
16. **Done criteria:** a native release build shows no hitches over one frame
    (16.7 ms at 60 Hz) during the hover reveal, measured with Instruments'
    Animation Hitches and a `drive.swift`-style driver on a real legal
    document with the preview open. Windows also gets a smoothness check.

17. **A plain `gpui-pre 0.3.12` bump. Kit is deferred** until a specific new
    control would be expensive to build. Decisions 14 and 15 apply only if
    Kit is adopted later.
18. **Both reveal fixes:** `.cached()` on the document view, and timings of
    100 ms intent plus a 120 ms slide (amends ADR 0027). The 300 ms close grace
    stays. The maintainer confirms the timings by eye.
19. **Record:** ADR 0117, plus a ROADMAP "Desktop GPUI" section that
    replaces the Later item on GPUI Fast. Glossary: rail and hover reveal,
    cached view.

## Facts for round 4

- The reveal waits `REVEAL_DELAY` (200 ms) of hover intent, then slides for
  `SLIDE` (150 ms). The list is fully visible about 350 ms after the
  pointer arrives. It closes `REVEAL_GRACE` (300 ms) after the pointer
  leaves ([workspace.rs](../../src/workspace.rs#L3080)). These timings come
  from [ADR 0027](../adr/0027-chrome-style-vertical-sidebar.md), so changing
  them amends that ADR.
- The reveal is a `deferred` overlay with `shadow_lg`, rounded corners and a
  150 ms width animation. The list inside it has a fixed width and is
  clipped, so the list never reflows. Each of the ~9 frames at 60 Hz still
  rebuilds the whole window, including the active document's editor and the
  PDF preview, because neither is `.cached()`.
- So "not quick" has two separate causes: the deliberate 350 ms before the list
  is fully shown, and per-frame cost that can make the slide drop frames.
  Only the second is a GPUI question.
- The [ROADMAP Later item](../../ROADMAP.md#later) on GPUI Fast is the
  earlier form of this grill. The outcome replaces it.

## Facts for round 3

- The left sidebar is rendered in `Workspace::sidebar`
  ([workspace.rs](../../src/workspace.rs#L2319)), inside the root view's
  render. The rows are plain children of a scrolling `div`, not a virtual list.
  Collapse, expand and rail reveal are 150 ms width animations
  ([sidebar.rs](../../src/workspace/sidebar.rs#L11)). Hover intent, drag-over
  and edge resize call `cx.notify()` on `Workspace`
  ([workspace.rs](../../src/workspace.rs#L2978)).
- Every animation frame, hover-style change and resize step therefore
  rebuilds the whole window, including the active `DocumentView`, its editor
  and the PDF preview. None of them is `.cached()`.
- Expand and collapse already avoid reflowing the content on each frame. The
  list is clipped at its final width
  ([workspace.rs](../../src/workspace.rs#L2799)). An edge resize does not: each
  drag step changes the editor's width, so the visible band is shaped again.
  The [native editor-layout evidence](../evidence/editor-layout/2026-10-17-native/README.md)
  found that layout measurement dominates main-thread time, and that GPUI's line
  cache keeps only the lines used in the previous frame. A new width misses
  every line.
- GPUI Fast and `.cached()` can skip the document view on hover and animation
  frames. Neither helps a live resize, because the content's width really does
  change.
- GPUI Kit has these layers: `gpui-base` (unstyled behavior, state and
  AccessKit semantics), `gpui-component` (styled in shadcn/ui's look, with
  themes) and the `gpui-kit` facade that re-exports them with `gpui-pre
  =0.3.12`. Styled components need `gpui_kit::init`, and Kit windows mount a
  Base `Root`. Apache-2.0. [Repository](https://github.com/longbridge/gpui-kit).
- mdoc has its own control set in [ui.rs](../../src/ui.rs): buttons, menu
  rows, cards, focus rings and cycling, title bar and notices. It also has a
  confirm dialog ([confirm_dialog.rs](../../src/confirm_dialog.rs)) and a
  search text input ([markdown_search.rs](../../src/markdown_search.rs)).
  The look is governed by [ADR 0048](../adr/0048-workspace-shell-redesign.md).

## Facts for round 2

- The Replacements panel is not its own view. `DocumentView::replacements_panel`
  ([panel.rs](../../src/pii/ui/mapping/panel.rs#L686)) builds it inside the
  document view's render, next to the editor and the PDF preview
  ([document_view.rs](../../src/document_view.rs#L2197)). The panel's rows
  are a `uniform_list`, and the projection and entries are cached by revision.
- Row hover, drag-over, filter and expand handlers call `cx.notify()` on
  `DocumentView` ([panel_rows.rs](../../src/pii/ui/mapping/panel_rows.rs#L633)).
  In upstream GPUI, each notification rebuilds the window's element tree, so a
  hovered row in the panel can rebuild the editor and preview elements too.
  The editor (`Entity<EditorState>`) and PDF viewer are separate entities.
- `gpui-pre 0.3.2` already has upstream view caching:
  `Entity::cached(StyleRefinement)` skips a view that has not been notified.
  mdoc doesn't use it anywhere. GPUI Fast generalizes that retention.
- crates.io on 2026-11-14 shows the latest releases are still `gpui-pre`
  0.3.12, `gpui-fast` 0.1.15, `gpui-kit` 0.9.1 and `gpui-ce` 0.3.0.

## mdoc's starting point

- `Cargo.lock` resolves the `gpui-pre` family to 0.3.2, a snapshot of Zed
  `801c087`. `Cargo.toml` allows `0.3` for the core and platform facade,
  pins the macOS and Windows crates to `=0.3.2`, and overrides the macOS,
  Windows and WGPU packages with local copies. The app, editor, PDF viewer and
  BiDi adapter use the same core crate. The GUI-free editor core and mobile
  frontends are outside this dependency change.
- The app advertises macOS 11 on Apple Silicon and sets 11.0 in the
  [packager configuration](../../Cargo.toml#L248) and
  [Info.plist](../../resources/Info.plist#L42). The shipped desktop targets are
  Apple Silicon macOS and Windows x64. Linux is source-only.
- GPUI is part of the WYSIWYG editor's layout, painting, hit-testing and IME,
  as well as PDF preview, actions, focus and accessibility. The current
  editor and PDF viewer also depend on the capture phase of scroll dispatch.
  In 0.3.12, `ScrollEvent` replaces `ScrollWheelEvent` and carries a
  `ScrollSource`. `HeadlessAppContext::with_platform` takes a fallible renderer
  factory. The text wrapper accepts an indent adjustment, upstream added
  UAX #14 line breaking, and Taffy changes from 0.13 to 0.15. Check wrap,
  caret and scroll behavior even where the app compiles without a change.
  [Scroll source](https://github.com/zed-industries/zed/blob/d2a4fb6ded7c470be25d1541281da3882495b65b/crates/gpui/src/interactive.rs),
  [0.3.12 crate](https://docs.rs/crate/gpui-pre/0.3.12),
  [upstream sync notes](https://github.com/longbridge/gpui-fast/releases/tag/v0.1.13).

## Candidate versions

| Option | What mdoc gets | Cost or limitation |
| --- | --- | --- |
| Keep `gpui-pre 0.3.2` | Existing integration and packaging floor. A baseline for app-side profiling and future comparisons. | Keeps three vendored crates and leaves the app on an older upstream snapshot. |
| [gpui-pre 0.3.12](https://docs.rs/crate/gpui-pre/0.3.12) | Same registry family on Zed `d2a4fb6`. First candidate for evaluating upstream input, text and renderer changes. | Re-port the platform hooks and migrate affected APIs. The Apple shader build targets macOS 12.0. [0.3.10's shader build](https://github.com/zed-industries/zed/blob/08cbb88/crates/gpui_apple/build.rs) predates that change and is a comparison point if macOS 11 matters. macOS 11 runtime support requires separate verification. [0.3.12 shader build](https://github.com/zed-industries/zed/blob/d2a4fb6ded7c470be25d1541281da3882495b65b/crates/gpui_apple/build.rs). |
| [GPUI Fast 0.1.15](https://github.com/longbridge/gpui-fast/releases/tag/v0.1.15) | Same Zed baseline as `gpui-pre 0.3.12`, with retained views and layout plus cached scroll layers. This makes the two suitable for an A/B comparison. | Experimental renderer with its own package family. Cached views can expose missing invalidation when state changes outside tracked GPUI state. Re-port the hooks and measure stale pixels, caret and popup placement, input, accessibility and memory along with speed. The [upstream revision](https://raw.githubusercontent.com/longbridge/gpui-fast/v0.1.15/UPSTREAM), [retention rules](https://github.com/longbridge/gpui-fast/blob/v0.1.15/docs/retained-mode.md) and [scroll layer rules](https://github.com/longbridge/gpui-fast/blob/v0.1.15/docs/scroll-layers.md) describe what the fork can reuse. |
| [GPUI Kit 0.9.1](https://github.com/longbridge/gpui-kit/releases/tag/v0.9.1) | Facade for `gpui-pre =0.3.12`, `gpui-base` primitives and optional styled controls. The optional Fast backend selects Fast types for Kit and its layers. Accessible controls, overlays and focus handling are plausible reuse targets. | Adds another API and dependency layer. Use a single GPUI type family across all mdoc crates that exchange entities, windows or elements. Kit keeps upstream dependencies in the build with Fast enabled. Reusing a control requires a specific use case and behavior checks. Its editor and Markdown display would require a separate evaluation against mdoc's source mapping, history, annotation geometry and editing semantics. [Published dependency graph](https://docs.rs/crate/gpui-kit/0.9.1). |
| [GPUI CE 0.3.0](https://docs.rs/crate/gpui-ce/0.3.0) | Apache-licensed community fork, standalone crates, platform and rendering work under independent governance. | CE documents growing API divergence. The Windows headless factory still returns `None` in this release. Upstream contribution policy affects the path to landing fixes developed with AI assistance. [Project and contribution rules](https://github.com/gpui-ce/gpui-ce), [published platform source](https://docs.rs/crate/gpui_ce_platform/0.3.0/source/src/gpui_platform.rs). |
| Pinned Zed Git or [`gpui-unofficial` 1.28.1](https://crates.io/crates/gpui-unofficial/1.28.1) | A Zed Git pin can include an upstream fix as soon as it lands. The registry mirror publishes from Zed release tags and documents its packaging transforms. | A Git pin adds monorepo fetch and dependency-policy work. The mirror adds a different publisher and crate identity. Both still need a platform-hook strategy. Zed is working on official release automation and the official `gpui` package is still at 0.2.2. [Zed maintainer update](https://github.com/zed-industries/zed/issues/66275), [publishing discussion](https://github.com/zed-industries/zed/discussions/55271), [mirror](https://github.com/gpui-archipelago/gpui-unofficial). |

[GPUI Box](https://github.com/fran0220/gpui-box) is another independently
packaged core and component stack. [Bite GPUI](https://github.com/bite-gpui/bite-gpui)
splits GPUI into separately consumable engine layers. Their strongest cases
would be a specific need for their packaging, renderer or extension points.
Outside GPUI, [Iced](https://github.com/iced-rs/iced),
[Slint](https://github.com/slint-ui/slint), and a webview app using
[Tauri](https://github.com/tauri-apps/tauri) are desktop frontend migrations.
They would need equivalents for editor layout and input, preview, accessibility,
window and lifecycle behavior, and the offscreen harness. The Rust domain core
can remain shared. A frontend migration needs its own product objective.

## Patch and qualification findings

| Local patch | Source findings |
| --- | --- |
| [macOS](../../vendor/gpui-pre-macos/MDOC-PATCH.md) | Preserve the Save As reason, deferred AppKit quit, the shelf's status item, Dock presence and popup sizing, text-plus-file drag with its completion callback, and the concealed local clipboard for restored text. The newer published macOS crates still require a port for these behaviors. |
| [Windows](../../vendor/gpui-pre-windows/MDOC-PATCH.md) | Preserve the Save As title, the restored-text clipboard formats, and windowless DirectWrite plus D3D11 WARP with neither an HWND nor a swap chain. `gpui-pre 0.3.12` and `gpui-fast 0.1.15` still provide headless renderer factories only for macOS and Linux. Windows WARP references elsewhere in the source do not expose mdoc's factory. [Upstream platform source](https://github.com/zed-industries/zed/blob/d2a4fb6ded7c470be25d1541281da3882495b65b/crates/gpui_platform/src/gpui_platform.rs). |
| [Linux WGPU](../../vendor/gpui-pre-wgpu/ZORITE-PATCH.md) | Zed merged the same glyph-offset correction on November 11. The downloaded `gpui-pre 0.3.12`, `gpui-fast 0.1.15` and `gpui-unofficial 1.28.1` WGPU sources still use unadjusted positions. Drop the override once the chosen package contains the correction and the combining-mark check passes. [Upstream fix and regression](https://github.com/zed-industries/zed/pull/66799). |

The source audit establishes the package versions, known API changes and
patch candidates. Compile cost, packaged deployment targets, native behavior
and mdoc runtime speed remain unmeasured. The Metal build argument is a
compatibility signal. mdoc currently requests `runtime_shaders`, so verify the
actual packaged binary and runtime path before making a support claim.

If performance drives the decision, use mdoc's public legal-document, review
and preview fixtures at identical editor widths. Compare the current pin,
updated upstream and Fast on the same Zed baseline. Attribute time to editor
and PDF work, GPUI frame work and presentation. Compare sustained scroll,
typing, PII navigation and memory, with native checks on both shipped
platforms before adoption. Longbridge reports 37% lower mean scroll CPU in its
component gallery using `gpui-pre 0.3.9` and `gpui-fast 0.1.7`. mdoc's editor
and preview have their own work per frame, so measure them directly. Keep the
existing offscreen and temporary-profile contract. [Longbridge's measurements](https://github.com/longbridge/gpui-fast#performance),
[ADR 0049](../adr/0049-measured-stabilization-and-architecture-review.md),
[headless and performance commands](../development.md#headless-screen-renders-macos).

## Context for round 1

- No open issue reports desktop jank, slow typing or slow scrolling. The open
  speed issues are iOS (#773) and CI (#760: the Windows headless-renders test
  took 723 s on the queue runner).
- [ADR 0049](../adr/0049-measured-stabilization-and-architecture-review.md)
  already rules that retained editor layout is investigated only if
  release-build cost justifies its complexity and geometry and editing risks,
  and that budgets come from release/native baselines, not debug or headless
  timings. GPUI Fast is retained rendering at the framework level, so the same
  condition applies to it.

## Round 4 questions

17. **Kit or a plain bump.** Is a plain `gpui-pre 0.3.12` bump enough, with
    Kit deferred until a specific new control would be costly to build
    (recommended)?
18. **Reveal timing.** Shorten the intent delay and slide (amending ADR 0027),
    or keep the timings and fix only per-frame cost?
19. **Record.** One new ADR plus a ROADMAP section that replaces the Later
    item on GPUI Fast (recommended)?

## Round 3 questions

11. **Which left-sidebar interactions** are slow: collapse/expand slide, rail
    hover reveal, row hover, scrolling a long tab list, edge resize,
    file drag-over, or switching tabs?
12. **Resize behavior.** Keep the live reflow and try to make it cheap, or
    reflow the content once when the drag ends while the sidebar edge moves
    on top (recommended if resize is slow)?
13. **Kit layer.** `gpui-base` primitives with mdoc's own styling
    (recommended), or `gpui-component`'s styled look?
14. **Version coupling.** Should mdoc's GPUI version follow Kit's exact pin
    (recommended), or would `gpui-base` be adopted only when its pin
    matches mdoc's choice?
15. **First Kit controls.** Which mdoc controls are replaced first?
16. **Done criteria.** Is the work done when native release builds show no
    hitches over one frame in the slow interactions, on a real legal document,
    on both platforms (recommended)?

## Round 2 questions

4. **Symptom.** Which sidebar interactions feel slow: scrolling, row hover,
   opening/closing or resizing, filter/search typing, expand/collapse, drag,
   or jumping to a mention?
5. **Sequence.** Upgrade to 0.3.12 first and profile there (recommended), or
   profile on 0.3.2 first?
6. **App-side fix first.** Before an A/B with Fast, try moving the panel into
   its own view and caching the editor and PDF views with upstream
   `.cached()`? Recommended.
7. **macOS floor.** Is the new floor 12.0, which 0.3.12 needs, or higher?
   Recommend 12.0.
8. **Windows in the same change.** Should one upgrade cover both shipped
   platforms? Recommended.
9. **Patch carrying.** Keep vendoring and re-port the three crates
   (recommended), or move to a patched fork repository?

## Round 1 questions

1. **Reason to move.** Is the first objective desktop responsiveness, lower
   dependency and patch maintenance, or faster UI development? Recommend
   starting with responsiveness and keeping the current pin as the control.
2. **Minimum macOS version.** Is macOS 11 a firm shipping commitment, or can
   a candidate justify raising it to macOS 12? Recommend keeping 11 as the
   requirement unless the user explicitly accepts changing it based on a
   demonstrated benefit.
3. **Scope.** Is this a GPUI upgrade and fork evaluation, or also a search for
   a replacement desktop frontend? Recommend evaluating GPUI families and
   considering a Kit primitive only against a concrete mdoc UI need.
