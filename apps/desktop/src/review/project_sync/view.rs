//! How the Project's names draw under the Replacements panel: the names
//! this document doesn't mention, and the field that adds one.
use crate::review::text;
use crate::{document::DocumentView, ui};
use gpui::{AnyElement, Context, div, prelude::*, px};

impl DocumentView {
    /// Project names this document doesn't mention, folded under the panel
    /// footer, with a field to add a name to the Project.
    pub(crate) fn project_names_section(&self, cx: &mut Context<Self>) -> Option<AnyElement> {
        let link = self.project.link()?;
        let theme = self.theme.get();
        let palette = theme.pdf_style();
        let absent: Vec<_> = self
            .project
            .names()
            .iter()
            .filter(|c| !self.project_alias_is_present(&c.alias, cx))
            .collect();
        let open = self.project_names_open;
        Some(
            div()
                .flex()
                .flex_col()
                .gap_1()
                .on_action(cx.listener(
                    |this, _: &crate::workspace::ConfirmProjectName, window, cx| {
                        this.confirm_project_name(window, cx)
                    },
                ))
                .on_action(cx.listener(
                    |this, _: &crate::workspace::CancelProjectName, window, cx| {
                        this.cancel_project_name(window, cx)
                    },
                ))
                .child(
                    ui::control(
                        "project-names-toggle",
                        if absent.is_empty() {
                            // Nothing to list: keep only the way to add a name.
                            text::add_a_name_to(if open { "▾" } else { "▸" }, &link.name)
                        } else {
                            text::in_project_not_here(
                                if open { "▾" } else { "▸" },
                                &link.name,
                                absent.len(),
                            )
                        },
                        theme,
                        true,
                    )
                    .when(cfg!(test), |v| {
                        v.debug_selector(|| "project-names-toggle".into())
                    })
                    .text_size(px(12.))
                    .text_color(palette.header_muted)
                    .on_click(
                        cx.listener(|this, _, window, cx| this.toggle_project_names(window, cx)),
                    ),
                )
                .when(open, |v| {
                    v.when(!absent.is_empty(), |v| {
                        v.child(
                            div()
                                .id("project-names")
                                .when(cfg!(test), |v| v.debug_selector(|| "project-names".into()))
                                .max_h(px(160.))
                                .overflow_y_scroll()
                                .flex()
                                .flex_col()
                                .children(absent.iter().map(|c| {
                                    div()
                                        .flex()
                                        .gap_2()
                                        .px_2()
                                        .text_size(px(12.))
                                        .child(
                                            div()
                                                .flex_1()
                                                .min_w_0()
                                                .truncate()
                                                .child(c.wording().to_string()),
                                        )
                                        .child(
                                            div()
                                                .flex_shrink_0()
                                                .text_color(theme.applied())
                                                .child(c.alias.clone()),
                                        )
                                })),
                        )
                    })
                    .children(self.project_term.clone().map(|input| {
                        div()
                            .mx_2()
                            .px_1()
                            .rounded_sm()
                            .border_1()
                            .border_color(palette.border)
                            .child(input)
                    }))
                })
                .into_any_element(),
        )
    }
}
