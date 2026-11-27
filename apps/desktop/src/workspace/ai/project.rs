//! The AI project in the window (ADR 0126; #1164 Decisions 2, 5 and 25;
//! Milestone 2 item 8), all of it only while "AI connector (preview)" is
//! on:
//!
//! - the AI project is always shown: the sidebar's foot (or the rail) and
//!   its Project page name it, with Stop sharing;
//! - an AI app asking while nothing is shared brings a prompt to share
//!   the Project last viewed, or another; only its Share shares, so the
//!   share never follows the view;
//! - a document entering the AI project is scanned in its tab, by the
//!   path Pseudonymize takes: proposals only, never applied;
//! - the Project page lists its AI activity.
//!
//! Notifications, the badge and where a delivery lands are
//! `deliveries.rs` (item 9).
//!
//! The share lives in the connector, in memory only; nothing here writes
//! the Vault.
//!
//! What the window knows of it, GPUI-free, is `state.rs`; its view and its
//! actions each have their file.
mod actions;
mod state;
pub(in crate::workspace) mod view;

pub(in crate::workspace) use state::*;
