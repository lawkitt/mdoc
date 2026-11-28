//! Quitting: each tab queued is asked in turn, then the quit is the
//! writer's last job (ADR 0130): documents, Projects and the session are
//! written and the window closes in one turn. The workspace view asks,
//! writes and closes the window; this owner decides what comes next. Its state and actions each have their file.
mod actions;
mod state;

pub(crate) use state::CloseWatch;
pub(in crate::workspace) use state::{QuitEffect, QuitFlow};
