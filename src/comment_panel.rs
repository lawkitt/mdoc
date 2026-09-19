//! A virtualized read-only list; comment text never enters the Markdown editor.
use crate::{docx_preview::DocxComment, style::Theme};
use gpui::{Context, ListAlignment, ListState, Window, div, list, prelude::*, px};
use std::{cell::Cell, rc::Rc, sync::Arc};

pub struct CommentPanel {
    comments: Arc<Vec<DocxComment>>,
    list: ListState,
    expanded: bool,
    theme: Rc<Cell<Theme>>,
}
impl CommentPanel {
    pub fn new(comments: Arc<Vec<DocxComment>>, theme: Rc<Cell<Theme>>) -> Self {
        Self {
            list: ListState::new(comments.len(), ListAlignment::Top, px(200.)),
            comments,
            expanded: true,
            theme,
        }
    }
}
impl Render for CommentPanel {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let palette = self.theme.get().pdf_style();
        let comments = self.comments.clone();
        div()
            .flex()
            .flex_col()
            .min_h_0()
            .border_t_1()
            .border_color(palette.border)
            .child(
                div()
                    .id("toggle-comments")
                    .p_2()
                    .cursor_pointer()
                    .child(format!(
                        "{} Comments ({})",
                        if self.expanded { "▾" } else { "▸" },
                        comments.len()
                    ))
                    .on_click(cx.listener(|this, _, _, cx| {
                        this.expanded = !this.expanded;
                        cx.notify();
                    })),
            )
            .when(self.expanded, |panel| {
                panel.child(
                    list(self.list.clone(), move |i, _, _| {
                        let c = &comments[i];
                        div()
                            .p_3()
                            .flex()
                            .flex_col()
                            .gap_2()
                            .border_b_1()
                            .border_color(palette.border)
                            .text_size(px(13.))
                            .child(div().font_weight(gpui::FontWeight::BOLD).child(format!(
                                "{}. {}",
                                i + 1,
                                if c.author.is_empty() {
                                    "Unknown author"
                                } else {
                                    &c.author
                                }
                            )))
                            .when(!c.date.is_empty(), |row| {
                                row.child(div().text_size(px(11.)).child(c.date.clone()))
                            })
                            .when(!c.quote.is_empty(), |row| {
                                row.child(
                                    div()
                                        .border_l_2()
                                        .border_color(palette.border)
                                        .pl_2()
                                        .child(c.quote.clone()),
                                )
                            })
                            .child(if c.text.is_empty() {
                                "Empty comment".to_owned()
                            } else {
                                c.text.clone()
                            })
                            .into_any_element()
                    })
                    .h(px(220.))
                    .w_full(),
                )
            })
    }
}
