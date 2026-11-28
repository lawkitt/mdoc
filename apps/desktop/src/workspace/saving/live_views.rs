//! The window's live views, most recently viewed first: each one's id, the
//! document it names, its saved record and its document view, which one is
//! active, and whether Home covers it. At most `LIMIT` documents stay live;
//! the least recently viewed beyond them is evicted. Generic over the view,
//! so the recency rules run without a window; the workspace view creates,
//! releases and focuses the documents. Its state and actions each have their file.
mod actions;
mod state;

pub(in crate::workspace) use state::{LIMIT, LiveView, LiveViews, LiveViewsEffect, Weight};
