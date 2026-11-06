# Mobile UI system

Status: Accepted, 2026-11-05. Amends [ADR 0087](0087-ios-ui-polish.md)
(Settings and Home) and [ADR 0085](0085-mobile-apps.md) (theme). Written for
iOS first; Android follows the same decisions.

## Problem

Each iOS screen sets its own spacing, colors and type, so a polish request
means a new grill and new sketches, and nothing stops a later change from
undoing it. Agents can only see the UI by walking the whole journey in the
Simulator. The maintainer's device screenshots carry real client data, so
they can't be quoted or committed. Dated screenshot sets fill the tree
(about 38 MB for iOS, 18 MB for Android). Settings shows raw model names and
the phone's RAM; Home repeats the title on its case line; there is no theme
choice, while desktop has System, Light and Dark.

## Decision

### Working on the UI

- **One living guide,** [docs/mobile-ui.md](../mobile-ui.md), for both
  mobile apps: tokens, components, gestures, haptics, motion, wording and
  accessibility, each with a code pointer, and an iOS and an Android column
  where they differ. ADRs keep the reasons. Every UI PR updates the guide;
  agents read it before UI work.
- **Tokens and components in code.** `apps/ios/mdoc/Design/`: `Tokens.swift`
  (spacing 4, 8, 12, 16, 24; radii; type styles; named colors in the asset
  catalog) and one file per component (`Chip`, `Card`, `Toast`,
  `StatusPill`, `IconTile`, and the review bar's parts). Screens compose
  only these. Android mirrors them under `ui/theme` with the same names.
- **Previews instead of sketches.** Every component and screen state has a
  `#Preview` in light, dark, AX5 and iPad, fed by `PreviewFixtures.swift`.
- **Snapshot tests** of components and key states (review bar, Home,
  Settings), in the iOS job, through a small in-repo helper
  (`apps/ios/mdocTests/Snapshot.swift`: `ImageRenderer` and a pixel
  compare) rather than a dependency.
- **Screenshots.** One `current/` UI survey per platform, replaced by each
  UI PR; before and after go in the PR description; dated sets leave the
  tree, linked by permalink. `just ios-survey` defaults to the iPhone 17 Pro.
- **The loop.** A note, or a screenshot of the sample document, becomes a
  `ui` issue (what's wrong, what good looks like). An agent changes the
  components and updates the previews, snapshots and `current/` survey; the
  PR carries before and after; the maintainer checks the TestFlight build.
  A grill is only for a pattern the guide lacks, which the guide then gains.

### Sample document

- A short contract with made-up names, in English and Russian, chosen by
  the phone's language (English otherwise). "Try a sample" sits in Home's
  empty state and in About. It is the same text the UI tests and previews
  use, so screenshots of it are safe to share and reproducible.

### Look

- **Theme:** Settings › Appearance › Theme: Automatic, Light, Dark; applied
  at once to every screen and sheet; stored on the device.
- **Accent:** desktop's teal green (`#336b5c` light, `#82b9a7` dark) fills
  the primary button, tints chips and outlines the active name.
- **Document font:** Settings › Appearance › Document font: System (the
  default) or Serif (New York), at the same Dynamic Type sizes; it changes
  the document text only.

### Settings

- Inset grouped, with colored icon tiles:
  - **Appearance:** Theme, Document font, Show Markdown (also in the ⋯ menu).
  - **Review:** Haptics, Reset tips.
  - **Models:** plain names ("Name finder", "Text recognition (English,
    Russian)") with "Installed · 632 MB"; a tap opens details: the technical
    name, Allow cellular, Remove or Download again, and for the name finder
    "This iPhone: 12 GB · enough to find names" (the Memory row's new home).
  - **Privacy:** "Documents stay on this iPhone", opening the privacy text.
  - **About mdoc:** version, licenses, Try a sample.

### Home

- **Row:** a source icon tile (PDF, photo, scan, pasted text); the title on
  one line; state and time on line 2, colored with an icon ("11 to review ·
  17:24" orange, "Ready to share · Yesterday" green, "Reply restored"
  blue); the case as a small colored tag on line 2.
- **List:** most recently touched first; search over titles and cases from
  8 documents; a case filter in the toolbar from two cases; a long press
  gives a preview and Rename, Move to case, Share, Delete.
- **Adding:** a bottom toolbar with a prominent Scan and a + menu (Paste,
  Files, Photos); Settings stays top left.
- **Empty state:** three large tiles (Scan, Files, Paste) and Try a sample;
  the setup card stays above while models download.

## Invariants

- Screens use tokens and components, never literal spacing or colors.
- Every component and screen state has a preview; key states have
  snapshots.
- A UI change updates the guide, its previews and the `current/` survey in
  the same PR.
- The sample document holds no real person's data.

## Consequences

- A few MB of snapshot reference images, recorded on the pinned Xcode.
- About 50 MB of dated screenshots leave the tree.
- Android gets the same tokens, Settings, Home and theme in its own
  milestone.

References: [ios-review-grill-2026-11-05](../design/ios-review-grill-2026-11-05.md)
(rounds 6 and 7).
