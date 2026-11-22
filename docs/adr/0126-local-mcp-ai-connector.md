# Local MCP AI connector for desktop

Status: Accepted, 2026-11-22.

## Why

Copying between mdoc and Claude Desktop or ChatGPT's desktop app breaks flow, and both run local stdio MCP servers. An `mdoc-mcp` shim, shipped in the app and registered from Settings, forwards stdio MCP to the running app over an owner-only socket or pipe. The Vault key never leaves mdoc; closed or locked, the shim returns an error and launches nothing.

Aliases are per Project (`PERSON_1` is a different person in each), and one server process serves every chat. So the connector serves one **AI project**, chosen and shown in mdoc: its tools take no project id, and `deliver_reply` names a document it read, so a reply after a switch is refused. Only Ready documents leave, re-checked against the Project's names on each read; Unfiled never does. The AI's label comes from the reviewed text, never the file-name title. `deliver_reply` restores only inside mdoc.

## Rejected

- **Restored text in responses:** real names would enter the AI's context.
- **Tools spanning Projects:** a reply about one restores with another's names.
- **A bearer token:** stops no one the socket's permissions do not; any same-user process can read the AI project's pseudonymized text.
- **A `127.0.0.1` HTTP transport:** unneeded; local web pages reach it.
- **An `.mcpb` carrying its own shim:** a second signed artifact that drifts from the app.

## Revisit when

A desktop AI app offers a native local extension API, or users need two Projects in parallel chats.
