- **Edit the rendered text.** Edit now works with Show Markdown off too:
  the construct the caret touches shows its markers dimmed and the caret's
  line shows its `#`, `>` or bullet, while everything else stays as drawn;
  the caret steps over hidden markers and tables; Return continues lists,
  tasks, numbers and quotes and ends them on an empty item; ⌫ joins
  paragraphs, deletes an image's line whole and takes an emptied `**` pair
  with its last letter; each keystroke restyles only the paragraph it
  changed ([ADR 0109](docs/adr/0109-mobile-markdown-editor.md)).
