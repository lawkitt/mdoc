//! Documents save themselves: a tab's text, once typing pauses, goes to the
//! Vault through one writer, and a new document is created by its first
//! save. An Unfiled document keeps its own names with it. A closed tab's
//! last text is still written. Projects, moves, deletes, exports, imports,
//! replies and the quit go through the same writer, by one rule (ADR
//! 0130): a job carries what to do, never what an editor showed. At the
//! head of the queue it reads every editor it writes, names into their
//! Project first and then the text ([`Workspace::capture`]); the Vault
//! checks what the job knew before under its write lock; and the window
//! shows the result once the job has committed it. Its state and actions each have their file.
mod actions;
mod state;

pub(in crate::workspace) use actions::ready_unwritten;
pub(in crate::workspace) use state::{Op, Ran, Write, Writer};
