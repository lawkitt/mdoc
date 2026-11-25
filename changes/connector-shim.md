- **The AI connector's shim and local pipe.** Behind "AI connector
  (preview)", off by default, mdoc listens for the new `mdoc-mcp` shim on a
  pipe only the signed-in user can open (on macOS, a socket in a folder only
  they can enter). The shim speaks MCP to Claude Desktop or ChatGPT's
  desktop app in both protocol eras, with three tools (list the shared
  documents, read one, deliver an answer) and a "restore" prompt, and it
  can start mdoc in the background. It holds no key, reads no Vault file
  and never sees a real name: every call is answered by the connector
  service. With the setting off, nothing listens. Nothing registers it
  with an AI app yet (Milestone 2 item 7,
  [#1164](https://github.com/lawkitt/mdoc/issues/1164)).
