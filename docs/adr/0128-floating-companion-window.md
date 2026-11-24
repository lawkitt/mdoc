# A floating companion window replaces the shelf popup

Status: Accepted, 2026-11-24 ([#1164](https://github.com/lawkitt/mdoc/issues/1164)).

## Why

With the connector ([ADR 0126](0126-local-mcp-ai-connector.md)) the user works inside the AI app, and no AI app lets a local connector show anything by itself. Restored text must never render there, and switching to mdoc for every reply breaks the flow.

So mdoc floats one small window above other apps, toggled by a system-wide hotkey and the menu bar icon. It holds a fixed set: the AI project with a stop button, its documents with their state, the latest reply restored, Personal, search and a drop target. It opens by itself only for a delivered reply or a review request, and never takes focus.

It shows real names over other apps, so it is excluded from screen capture where the OS allows, hides restored text until hover by default, and always names what is shared.

It replaces the menu bar shelf's popup, which closed when another app became active, and takes over its copy and drag of Ready documents. The hotkey adds the `global-hotkey` dependency.

## Rejected

- **A notification only:** every reply then needs a switch to mdoc.
- **A panel per reply:** it cannot hold review, drop or what is shared.
- **The main window coming forward:** it steals focus mid-sentence.
- **A card inside the chat:** it would send restored text to the AI app.

## Revisit when

GPUI cannot float a non-activating window on Windows: Windows then ships notifications and the Reply page until it can.
