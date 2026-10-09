# os-spellcheck

Spell-checking through the operating system's own checker: `NSSpellChecker`
on macOS and the Windows Spell Checking API. Returns misspelled byte ranges and
suggestions; no GUI dependencies. Other platforms report nothing. Internal to
the mdoc workspace (`publish = false`).

Not used by the app yet: it is groundwork for a deferred spell-check
experiment (see ROADMAP). The `mdoc-editor` demo
(`cargo run -p mdoc-editor --example demo`) keeps it compiled and wired to the
editor's diagnostics.

MIT-licensed (see [LICENSE](LICENSE)), derived from Zorite.
