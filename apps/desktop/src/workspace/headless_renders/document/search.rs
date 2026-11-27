//! Markdown find and replace.
use crate::workspace::headless_renders::{Driver, Result};
use crate::{document::DocumentView, document::search_session::CloseMarkdownSearch};
use gpui::Entity;

pub(in crate::workspace::headless_renders) fn scenes(
    d: &mut Driver,
    view: &Entity<DocumentView>,
) -> Result<()> {
    d.change(|_, window, cx| view.update(cx, |v, cx| v.open_markdown_search(true, window, cx)))?;
    d.capture("find-replace")?;
    d.change(|_, window, cx| {
        view.update(cx, |v, cx| {
            v.close_markdown_search(&CloseMarkdownSearch, window, cx)
        })
    })?;
    Ok(())
}
