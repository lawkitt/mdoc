//! Projects' view: the Open Project list.

use super::text;
use crate::workspace::{Workspace, projects::ProjectLink};
use crate::{ui, ui::style};
use gpui::{AnyElement, Context, div, prelude::*, px};
use std::collections::HashSet;

impl Workspace {
    /// File → Open Project…: every Project, pinned then most recent.
    pub(in crate::workspace) fn project_list(&self, cx: &mut Context<Self>) -> Option<AnyElement> {
        if !self.project_ui.list_open || self.project_ui.explainer {
            return None;
        }
        let theme = self.theme.get();
        let palette = theme.pdf_style();
        let open: HashSet<String> = self
            .tabs
            .iter()
            .filter_map(|t| t.view()?.read(cx).project.link().map(|l| l.id.clone()))
            .collect();
        let rows: Vec<AnyElement> = self
            .projects
            .summaries()
            .into_iter()
            .map(|summary| {
                let link = ProjectLink {
                    id: summary.id.clone(),
                    name: summary.name.clone(),
                    colour: summary.colour,
                };
                let open_id = summary.id.clone();
                let delete_id = summary.id.clone();
                let documents = self.project_document_count(&summary.id);
                let names = (self.projects.project(&summary.id))
                    .map_or(0, |project| project.mapping.exported().len());
                div()
                    .id(gpui::SharedString::from(format!(
                        "project-row-{}",
                        summary.id
                    )))
                    .flex()
                    .items_center()
                    .gap_2()
                    .px_2()
                    .py_1()
                    .rounded_md()
                    .hover(move |v| v.bg(palette.placeholder_bg))
                    .child(
                        div()
                            .size(px(8.))
                            .flex_shrink_0()
                            .rounded_full()
                            .bg(style::project_color(link.hue(), theme)),
                    )
                    .child(
                        crate::ui::focus_ring(
                            gpui::SharedString::from(format!("open-project-{}", summary.id)),
                            theme,
                        )
                        .when(cfg!(test), |v| {
                            v.debug_selector(|| "open-project-row".into())
                        })
                        // Enter or Space opens it, as a click does.
                        .key_context("UiControl")
                        .tab_index(0)
                        .tab_stop(true)
                        .role(gpui::Role::Button)
                        .aria_label(text::open_named(&summary.name))
                        .rounded_sm()
                        .flex_1()
                        .min_w_0()
                        .cursor_pointer()
                        .flex()
                        .flex_col()
                        .child(div().truncate().child(summary.name.clone()))
                        .child(
                            div()
                                .text_size(px(11.))
                                .text_color(palette.header_muted)
                                .child(text::list_row_detail(
                                    names,
                                    documents,
                                    open.contains(&summary.id),
                                )),
                        )
                        .on_click(cx.listener(
                            move |this, _, window, cx| this.open_project(&open_id, window, cx),
                        )),
                    )
                    .child(
                        ui::control(
                            gpui::SharedString::from(format!("delete-project-{}", summary.id)),
                            text::DELETE,
                            theme,
                            true,
                        )
                        .on_click(cx.listener(
                            move |this, _, window, cx| {
                                this.close_project_list(window, cx);
                                this.show_project_page(
                                    &delete_id,
                                    crate::workspace::projects::project_page::Place::Delete,
                                    window,
                                    cx,
                                )
                            },
                        )),
                    )
                    .into_any_element()
            })
            .collect();
        let locked = self.projects_locked();
        let panel = ui::panel("project-list", theme)
            .when(cfg!(test), |v| v.debug_selector(|| "project-list".into()))
            .track_focus(&self.project_list_focus)
            .key_context("UiPanel UiMenu")
            .on_action(cx.listener(|this, _: &ui::NextControl, window, cx| {
                ui::cycle(window, cx, Some(&this.project_list_focus), false);
                cx.stop_propagation();
            }))
            .on_action(cx.listener(|this, _: &ui::PreviousControl, window, cx| {
                ui::cycle(window, cx, Some(&this.project_list_focus), true);
                cx.stop_propagation();
            }))
            .on_action(cx.listener(|this, _: &ui::CloseMenu, window, cx| {
                this.close_project_list(window, cx);
                cx.stop_propagation();
            }))
            .w(px(420.))
            .max_h(px(480.))
            .overflow_y_scroll()
            .p_2()
            .flex()
            .flex_col()
            .gap_1()
            .on_mouse_down_out(cx.listener(|this, _, window, cx| {
                this.project_ui.list_open = false;
                // Unless the click focused something, focus goes back where
                // the list was opened from.
                let list = this.project_list_focus.clone();
                let back = this.project_list_return.take();
                let weak = cx.entity().downgrade();
                window.defer(cx, move |window, cx| {
                    if list.contains_focused(window, cx) {
                        match back {
                            Some(focus) => window.focus(&focus, cx),
                            None => {
                                let _ = weak.update(cx, |this, cx| this.focus_active(window, cx));
                            }
                        }
                    }
                });
                cx.notify();
            }))
            .child(
                div()
                    .flex()
                    .items_center()
                    .px_2()
                    .pb_1()
                    .child(div().flex_1().child(text::OPEN_PROJECT))
                    .child(
                        ui::control("close-project-list", "×", theme, true)
                            .aria_label(text::CLOSE_PROJECT_LIST)
                            .on_click(cx.listener(|this, _, window, cx| {
                                this.close_project_list(window, cx);
                            })),
                    ),
            )
            .when(rows.is_empty() && !locked, |v| {
                v.child(
                    div()
                        .when(cfg!(test), |v| {
                            v.debug_selector(|| "project-list-empty".into())
                        })
                        .px_2()
                        .py_2()
                        .text_size(px(12.))
                        .text_color(palette.header_muted)
                        .child(text::NO_PROJECTS),
                )
            })
            .children(rows)
            .when(locked, |v| {
                v.child(
                    div()
                        .when(cfg!(test), |v| {
                            v.debug_selector(|| "project-list-locked".into())
                        })
                        .px_2()
                        .py_2()
                        .text_size(px(12.))
                        .text_color(palette.header_muted)
                        .child(text::LOCKED),
                )
                .child(
                    ui::control("project-list-unlock", text::UNLOCK, theme, true)
                        .when(cfg!(test), |v| {
                            v.debug_selector(|| "project-list-unlock".into())
                        })
                        .on_click(
                            cx.listener(|this, _, window, cx| this.unlock_projects(window, cx)),
                        ),
                )
            });
        let placed = div()
            .absolute()
            .inset_0()
            .flex()
            .items_start()
            .justify_center()
            .pt(px(80.))
            .child(panel);
        Some(gpui::deferred(placed).with_priority(2).into_any_element())
    }
}
