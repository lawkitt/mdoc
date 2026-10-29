//! Markdown find and replace.
use super::{Driver, Result};
use crate::{document_view::DocumentView, search_session::CloseMarkdownSearch};
use gpui::Entity;

pub(super) fn scenes(d: &mut Driver, view: &Entity<DocumentView>) -> Result<()> {
    d.change(|_, window, cx| view.update(cx, |v, cx| v.open_markdown_search(true, window, cx)))?;
    d.capture("find-replace")?;
    d.change(|_, window, cx| {
        view.update(cx, |v, cx| {
            v.close_markdown_search(&CloseMarkdownSearch, window, cx)
        })
    })?;
    Ok(())
}
