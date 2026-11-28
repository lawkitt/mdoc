//! Intake: a file opened, dropped or chosen in a folder sheet becomes a
//! Vault document, Unfiled or in the Project it was dropped on, when it is
//! admitted, before any conversion, with
//! its bytes kept as its Source Original. A file the Vault already holds
//! asks once, for every such file in the open, whether to open the existing
//! document or import it again. Its state and actions each have their file.
mod actions;
mod state;

pub(in crate::workspace) use state::{Staged, needs_conversion};
