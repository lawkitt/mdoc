//! The `mdoc-mcp` shim (ADR 0126; #1164 Decisions 7, 8, 21 and 23;
//! Milestone 2 item 7). An AI app such as Claude Desktop starts it and
//! speaks MCP on its stdin and stdout ([`mcp`], both protocol eras); each
//! tool call goes to the running mdoc of this profile over an owner-only
//! local pipe or socket ([`ipc`], [`wire`]), which answers through its
//! connector service. The shim holds no key, reads no Vault file and knows
//! no real name: it may only start mdoc ([`launch`]), and nothing leaves
//! before the user shares a Project there.
pub mod forward;
pub mod ipc;
pub mod launch;
pub mod mcp;
pub mod wire;
