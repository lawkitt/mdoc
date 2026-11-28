//! The sidebar, split by job: its state, its view and its actions, with the
//! tree, navigation, the switcher and the window's F6 regions beside them.
pub(super) mod actions;
pub(super) mod focus_regions;
pub(super) mod navigation;
pub(super) mod navigation_ui;
pub(super) mod state;
pub(super) mod switcher;
pub(super) mod switcher_ui;
mod text;
pub(super) mod tree;
pub(super) mod tree_ui;
pub(super) mod view;

pub(in crate::workspace) use state::Sidebar;
#[cfg(test)]
pub(in crate::workspace) use state::{DEFAULT_WIDTH, MAX_WIDTH, MIN_WIDTH};
pub(in crate::workspace) use view::can_drop_files;
#[cfg(test)]
pub(in crate::workspace) use view::{middle_cut, split_extension};
