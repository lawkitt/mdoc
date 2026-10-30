- **Quitting from the Dock asks first on macOS.** Quit from the Dock, a quit
  Apple Event or logout now shows the same unsaved-changes and mapping
  prompts as ⌘Q. It used to close mdoc at once, losing unsaved edits,
  unsaved mappings, untitled tabs and OCR results
  ([ADR 0071](docs/adr/0071-one-close-and-quit-prompt.md)).
