//! The tab list's saves to `session.json`: one write at a time,
//! the latest snapshot waiting behind it, and none when nothing changed
//! since the last save. Generic over the snapshot, so the queue rules run
//! without a window or a disk; the workspace view takes snapshots, runs the
//! writes in the background and reports how they ended. Its state and actions each have their file.
mod actions;
mod state;

pub(in crate::workspace) use state::{SessionEffect, SessionWriter};
