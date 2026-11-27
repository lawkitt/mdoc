//! The Project page: a header (colour, title, counts, Add document, ⋯) over
//! three segments, `Documents`, `Replies` and `Names`, from the window's
//! Projects and the Vault's summaries; it opens no document. The
//! page's struct is here; its state, view and actions each have their file.
mod actions;
mod state;
mod view;

use crate::document::markdown_search::SearchInput;
use gpui::{Entity, FocusHandle, Subscription};
pub(crate) use state::Segment;
pub(in crate::workspace) use state::{Place, created_date};
use std::{cell::RefCell, collections::HashMap};

/// View state only: never serialized and cleared on any tab activation.
pub(in crate::workspace) struct ProjectPage {
    id: String,
    segment: Segment,
    swatches: bool,
    #[cfg(all(
        debug_assertions,
        feature = "headless-renders",
        any(target_os = "macos", target_os = "windows")
    ))]
    place: Place,
    filter: Entity<SearchInput>,
    add: Entity<SearchInput>,
    scroll: gpui::ScrollHandle,
    color_focus: FocusHandle,
    category_focus: FocusHandle,
    controls: RefCell<HashMap<String, FocusHandle>>,
    _filter_changes: Subscription,
}

/// Controls of the header and the segment row, kept across segments.
fn header_control(id: &str) -> bool {
    id.starts_with("project-segment-")
        || id.starts_with("project-swatch-")
        || matches!(
            id,
            "project-page-rename"
                | "project-page-add-document"
                | "project-page-menu"
                | "project-page-ask"
        )
}

impl ProjectPage {
    pub(in crate::workspace) fn id(&self) -> &str {
        &self.id
    }

    pub(crate) fn segment(&self) -> Segment {
        self.segment
    }

    #[cfg(all(
        debug_assertions,
        feature = "headless-renders",
        any(target_os = "macos", target_os = "windows")
    ))]
    pub(in crate::workspace) fn categories(&self) -> bool {
        matches!(self.place, Place::Categories)
    }

    #[cfg(test)]
    pub(in crate::workspace) fn test_inputs(&self) -> (Entity<SearchInput>, Entity<SearchInput>) {
        (self.filter.clone(), self.add.clone())
    }

    /// The controls the page draws now: the header's and its segment's.
    #[cfg(test)]
    pub(in crate::workspace) fn test_controls(&self) -> Vec<(String, FocusHandle)> {
        let mut controls: Vec<_> = self
            .controls
            .borrow()
            .iter()
            .map(|(id, focus)| (id.clone(), focus.clone()))
            .collect();
        controls.push(("project-page-color".into(), self.color_focus.clone()));
        if self.segment == Segment::Names {
            controls.push(("page-category-PERSON".into(), self.category_focus.clone()));
        }
        controls
    }
    #[cfg(test)]
    pub(in crate::workspace) fn test_scroll_bounds(&self) -> gpui::Bounds<gpui::Pixels> {
        self.scroll.bounds()
    }
    #[cfg(test)]
    pub(in crate::workspace) fn test_scroll_offset(&self) -> gpui::Pixels {
        self.scroll.offset().y
    }
}
