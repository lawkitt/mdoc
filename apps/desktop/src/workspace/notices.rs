//! The workspace's notices: one session message and the last run's
//! crash report. They wait behind the header's bell with the Vault's
//! banner, which the Vault state owns. The owner's state, view, actions and
//! words each have their file.
mod actions;
mod state;
mod text;
mod view;

pub(super) use state::{Notices, NoticesEffect};

use gpui::actions;

actions!(mdoc_shell, [CloseNotices]);
