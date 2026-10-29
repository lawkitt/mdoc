//! The model comparison window.
use super::{Driver, HEIGHT, Result, WIDTH};
use crate::{comparison, comparison_ui, document_view::DocumentView};
use gpui::{Entity, Focusable, prelude::*, px, size};
use std::sync::Arc;

/// The comparison uses a second TestWindow, never a native window.
pub(super) fn scenes(d: &mut Driver, view: &Entity<DocumentView>) -> Result<()> {
    let (owner, panel, theme, defaults, input) = d.change(|w, _, cx| {
        (
            cx.entity().downgrade(),
            w.settings.clone(),
            w.theme.clone(),
            w.preferences
                .borrow()
                .snapshot()
                .expect("temporary preferences"),
            comparison::Input::Markdown(Arc::new(view.read(cx).editor.read(cx).text().to_owned())),
        )
    })?;
    let hash = input.hash();
    let comparison =
        d.cx.open_window(size(px(WIDTH), px(HEIGHT)), |window, cx| {
            let view = cx.new(|cx| {
                comparison_ui::View::new((input, hash), defaults, theme, owner, panel, vec![], cx)
            });
            window.focus(&view.read(cx).focus_handle(cx), cx);
            view
        })
        .map_err(|e| e.to_string())?;
    let workspace_window = d.window;
    d.window = comparison.into();
    d.capture("model-comparison")?;
    d.cx.update_window(d.window, |_, window, _| window.remove_window())
        .map_err(|e| e.to_string())?;
    d.window = workspace_window;
    Ok(())
}
