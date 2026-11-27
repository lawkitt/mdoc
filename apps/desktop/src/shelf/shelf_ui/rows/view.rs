//! How the shelf's rows draw: each document's row, the drag hint under
//! them, or the empty line.
use super::{EMPTY, HINT, ROW_GAP, ShelfRows, shows_hint};
use crate::shelf::text;
use gpui::{AnyElement, Context, MouseButton, Render, Window, div, prelude::*, px};

impl ShelfRows {
    fn hint(&self) -> AnyElement {
        let p = self.theme.get().pdf_style();
        div()
            .h(px(HINT))
            .flex()
            .items_center()
            .justify_center()
            .text_size(px(11.))
            .text_color(p.header_muted)
            .child(text::DRAG_HINT)
            .into_any_element()
    }
}

impl Render for ShelfRows {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let p = self.theme.get().pdf_style();
        if self.documents.is_empty() {
            return div()
                .h(px(EMPTY))
                .flex()
                .items_center()
                .justify_center()
                .text_color(p.header_muted)
                .child(text::NO_DOCUMENTS);
        }
        div()
            .flex()
            .flex_col()
            .gap(px(ROW_GAP))
            // A release anywhere ends a press that did not become a drag.
            .on_mouse_up(
                MouseButton::Left,
                cx.listener(|this, _, _, _| this.press = None),
            )
            .on_mouse_up_out(
                MouseButton::Left,
                cx.listener(|this, _, _, _| this.press = None),
            )
            .children((0..self.documents.len()).map(|i| self.document_row(i, cx)))
            .when(shows_hint(&self.documents, self.drag_hint), |v| {
                v.child(self.hint())
            })
    }
}
