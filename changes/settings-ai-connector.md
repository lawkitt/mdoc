- **Settings › AI connector.** A new page holds the "AI connector (preview)"
  switch, off by default; with it on, mdoc can add itself to Claude Desktop.
  Add shows the file and the exact `mcpServers` entry it will write, and writes
  it only on a yes, keeping everything else in the file and the old file as
  `claude_desktop_config.json.mdoc-backup`; Remove takes the entry out again,
  and a file mdoc can't read is left alone. Test connection opens a new Claude
  chat. Add works for ChatGPT's desktop app too, through `~/.codex/config.toml`,
  which Codex also reads; only ChatGPT's Work threads call it. The installer and
  the macOS app now ship the `mdoc-mcp` shim beside mdoc
  ([ADR 0126](docs/adr/0126-local-mcp-ai-connector.md),
  [#1164](https://github.com/lawkitt/mdoc/issues/1164)).
