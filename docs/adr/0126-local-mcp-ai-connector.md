# Local MCP AI connector for desktop

Status: Accepted, 2026-11-22; amended 2026-11-24 ([#1164](https://github.com/lawkitt/mdoc/issues/1164)).

## Why

Copying between mdoc and Claude Desktop or ChatGPT's desktop app breaks flow, and both run local stdio MCP servers. An `mdoc-mcp` shim, shipped in the app, forwards stdio MCP to mdoc over an owner-only socket or pipe. The Vault key never leaves mdoc. The shim may start mdoc: nothing leaves before the user approves a share.

Aliases are per Project, and one server process serves every chat. So the connector serves one **AI project**, chosen and shown in mdoc, plus allowed Personal documents, whose aliases are a separate series ([ADR 0129](0129-personal-documents-restore-only-aliases.md)). Tools take no project id, and `deliver_reply` names a document it read, so a reply after a switch is refused. Only Ready documents leave, re-checked on each read; Unfiled never.

The model shows its answer in the chat, with aliases, and delivers it as a reply or a new document. mdoc alone restores.

## Rejected

- **Restored text in responses:** real names would enter the AI's context.
- **Tools spanning Projects:** a reply about one restores with another's names.
- **An AI project that follows the Project in view:** it exposes text silently.
- **Holding a call while the user reviews:** one host allows 60 seconds.
- **A bearer token or local HTTP:** neither stops a same-user process.
- **An `.mcpb` carrying its own shim:** it drifts from the app; a launcher alone does not.

## Revisit when

A desktop AI app offers a native local extension API.
