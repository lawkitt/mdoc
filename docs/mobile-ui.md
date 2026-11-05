# Mobile UI guide

The current look-and-feel rules for the iOS and Android apps
([ADR 0099](adr/0099-mobile-ui-system.md)). ADRs keep the reasons; this
page keeps the rules and points at the code. Read it before UI work, and
change it in the same PR as the UI.

## How to change the UI

1. Start from an issue labelled `ui`: what's wrong, and what good looks
   like. A screenshot of the [sample document](#sample-document) is safe to
   attach; never attach a screenshot of a real document.
2. Change the [tokens](#tokens) or the [components](#components) first;
   a screen only composes them. A screen with a literal spacing or color is
   a bug.
3. Look at the change in Xcode previews. Every component and screen state
   has a `#Preview` fed by `PreviewFixtures` (light, dark, AX5, iPad).
4. Run the snapshot tests. An intended change re-records its references:
   `TEST_RUNNER_MDOC_RECORD_SNAPSHOTS=1` on `xcodebuild test
   -only-testing:mdocTests/ComponentSnapshotTests` (and the screen
   snapshot classes), then commit the new PNGs under
   `apps/ios/mdocTests/Snapshots/`.
5. Refresh the survey with `just ios-survey docs/evidence/ios-app/current`
   (the iPhone 17 Pro by default) and replace the files in `current/`. The
   PR description carries before and after images; dated folders don't go
   in the tree.
6. Update this guide when a rule changes.

A grill is only for a pattern this guide doesn't cover yet; the guide then
gains it.

## Tokens

iOS: [`apps/ios/mdoc/Design/Tokens.swift`](../apps/ios/mdoc/Design/Tokens.swift).
Android: `apps/android/.../ui/theme` under the same names (to come).

| Token | Values | Use |
|---|---|---|
| `Space` | 4, 8, 12, 16, 24 (`xxs`…`l`) | Padding and gaps; nothing off the 4 pt grid |
| `Radius` | chip (capsule), control 12, card 20, tile 8 | Continuous corners |
| `Size` | target 44, bar button 40, chip 30, tile 30 | Hit targets never under 44 pt |
| `Palette` | accent, surface, background, chip fill, separator; to review (orange), ready (green), restored (blue) | Only named colors; system colors adapt to dark mode and Increase Contrast |
| `Typography` | title, body, caption, chip, alias (monospaced), progress | Dynamic Type text styles only |
| `Motion` | snappy spring 0.25 s; fade 0.2 s under Reduce Motion | `Motion.move(reduced:)` picks one |

Category colors and symbols live in
[`Shared/UI/Palette.swift`](../apps/ios/Shared/UI/Palette.swift) (the
same hues as desktop's highlights). A case's color marks the case only,
never a state.

## Components

iOS: [`apps/ios/mdoc/Design/`](../apps/ios/mdoc/Design/).

| Component | Rule |
|---|---|
| `Chip` | Anything tappable that isn't a button is a filled capsule. A chip that opens a menu ends in a chevron; plain text never acts. Neutral fill, or tinted by category or state. At least 44 pt of target. |
| `card()` | A raised surface: the review card, empty-state tiles. |
| `StatusPill` | State as an icon and words in the state's color. |
| `IconTile` | Settings: white symbol on a colored square. Home: tinted fill, colored symbol. |
| `Toast` / `.toast($message)` | A short message after an action, with at most one action (Undo, Redo). Announced to VoiceOver; gone after about 4 s. |
| `Haptics` | `selection` per card, `decision` on Replace, Keep and Same as, `edge` at the end of a list, `success` on All reviewed, `warning` on undo. Off when Settings › Review › Haptics is off. |

## Gestures

Every gesture also has a visible control and a VoiceOver action; no action
exists only as a gesture ([ADR 0098](adr/0098-mobile-review-bar.md)).

| Where | Gesture | Does |
|---|---|---|
| Review card | Swipe sideways, or tap a peek | Previous or next candidate |
| Review bar grabber | Swipe up or down | Expand or collapse; down on the compact bar ends review |
| A name in the text | Long press | Its menu (Replace, Keep, Same as…, Change category) |
| Identities row | Swipe right or left | Replace; Keep and Same as… |
| Anywhere | Shake, three-finger swipe | Undo and redo |

No swipes on the document text.

## Wording

- Short, plain verbs: Replace, Keep, Same as…, New alias, Undo.
- Toasts name what happened: "12 replaced", "Same as Maya Bennett",
  "Undid Replace · Riverside Holdings".
- Plain names for models ("Name finder"); the technical name sits one tap
  deeper.
- US spelling.

## Accessibility

- Every screen works at every Dynamic Type size; bars stop growing at AX2
  and use the large content viewer.
- Every action has a VoiceOver label and, where it's a gesture, an action.
- Motion respects Reduce Motion (`Motion.move(reduced:)`), and colors come
  from the palette, so Increase Contrast and dark mode follow.

## Sample document

[`Shared/Core/SampleDocument.swift`](../apps/ios/Shared/Core/SampleDocument.swift):
a short services agreement with made-up names, in English and Russian,
chosen by the phone's language. It is "Try a sample" in the app and the
fixture for previews and tests, so screenshots of it are safe to share.

## Screenshots

- `docs/evidence/ios-app/current/` and `docs/evidence/android-app/current/`
  hold the latest survey, replaced by each UI PR.
- `just ios-survey <folder> [device]` runs `SurveyUITests` (iPhone 17 Pro
  by default); `just android-survey` on the Windows PC for Android.
- Earlier dated sets are in git history: the
  [2026-10-31 iOS survey](https://github.com/lawkitt/mdoc/tree/1e7096412fda2d27ec5a274e5b53ba00c3b101f1/docs/evidence/ios-app/2026-10-31-ui-survey)
  and later sets beside it.
