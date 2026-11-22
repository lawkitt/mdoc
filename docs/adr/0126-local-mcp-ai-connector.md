# Local MCP AI connector for desktop

Status: Accepted, 2026-11-22.

## Why

Copy-pasting between mdoc, Claude Desktop, and ChatGPT Desktop breaks flow on multi-document projects. Both converge on the Model Context Protocol (MCP). Desktop mdoc hosts an authenticated local IPC listener (`0600` Unix socket on macOS; user-SID named pipe on Windows) paired with a thin `mdoc-mcp` stdio shim—packaged as a `.mcpb` bundle for Claude Desktop and registered locally for ChatGPT Desktop (with optional `127.0.0.1` Streamable-HTTP/SSE transport if needed).

The Vault key never leaves mdoc. Five tools are exposed: `list_projects`, `list_documents`, `get_document`, `get_project_aliases`, and `deliver_reply`. Hard invariants prevent leaks: Unfiled documents stay hidden; Projects require opt-in `Allow AI access` (off by default); only `Ready` documents' cached `ready_snapshot` leaves; titles and project labels are pseudonymized; and `deliver_reply` restores aliases exclusively inside mdoc's UI (alerting without stealing focus) while returning only `{reply_id, restored_count, unresolved_tokens}`. When mdoc is closed or locked, `mdoc-mcp` returns an actionable error rather than launching or touching the Keychain headless.

## Rejected

- **Returning restored text in MCP responses:** would feed real client names straight back into the AI's context window.
- **Direct vault access from `mdoc-mcp`:** would expose the OS key outside the running app and bypass user visibility.
- **Separate proprietary plugins per AI app:** MCP covers Claude Desktop, ChatGPT Desktop, and coding agents with one contract.

## Revisit when

ChatGPT Desktop or Claude Desktop introduces a native local app-IPC extension API that supersedes local MCP transports.
