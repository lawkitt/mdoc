//! The Project page's view: the header, the segment row and the three
//! segments, with the ⋯ menu. It reads the owner's methods, never the Vault.

use super::state::{Place, Segment, colour_name, created_date, kind_label, project_counts};
use crate::workspace::ProjectLink;
use crate::workspace::projects::text;
use crate::workspace::{
    Workspace,
    projects::{AddProjectName, CancelProjectName, ClearProjectName, ConfirmProjectName},
};
use crate::{ui, ui::style};
use gpui::{AnyElement, Context, Focusable, SharedString, Window, div, prelude::*, px};
use mdoc_store::document_id::{DocumentId, VaultId};
use mdoc_vault::{Colour, DocumentSummary, Project, ReplySummary, SourceKind};

pub(super) fn section(label: &'static str) -> gpui::Div {
    div()
        .text_size(px(16.))
        .font_weight(gpui::FontWeight::SEMIBOLD)
        .child(label)
}

pub(super) fn empty_line(text: &'static str, p: gpui_pdf::PdfStyle) -> gpui::Div {
    div()
        .py_2()
        .text_size(px(12.))
        .text_color(p.header_muted)
        .child(text)
}

pub(super) fn kind_badge(kind: SourceKind, p: gpui_pdf::PdfStyle) -> gpui::Div {
    div()
        .flex_shrink_0()
        .w(px(40.))
        .flex()
        .justify_center()
        .px_1()
        .rounded_sm()
        .border_1()
        .border_color(p.border)
        .text_size(px(10.))
        .text_color(p.header_muted)
        .child(kind_label(kind))
}

impl Workspace {
    /// The Project ⋯ menu, from the page's header or the sidebar's.
    pub(in crate::workspace) fn project_header_menu(
        &self,
        cx: &mut Context<Self>,
    ) -> Option<AnyElement> {
        let (id, position) = self.project_ui.header_menu.as_ref()?;
        let theme = self.theme.get();
        let mut menu = ui::panel("project-header-actions", theme)
            .track_focus(&self.menu_focus)
            .key_context("UiPanel UiMenu")
            .flex()
            .flex_col()
            .p_2()
            .gap_1()
            .on_action(cx.listener(|this, _: &ui::NextControl, window, cx| {
                ui::cycle(window, cx, Some(&this.menu_focus), false);
                cx.stop_propagation();
            }))
            .on_action(cx.listener(|this, _: &ui::PreviousControl, window, cx| {
                ui::cycle(window, cx, Some(&this.menu_focus), true);
                cx.stop_propagation();
            }))
            .on_action(cx.listener(|this, _: &ui::CloseMenu, window, cx| {
                this.project_ui.header_menu = None;
                if let Some(focus) = this.menu_return.take() {
                    window.focus(&focus, cx);
                }
                cx.notify();
            }))
            .on_mouse_down_out(cx.listener(|this, _, _, cx| {
                this.project_ui.header_menu = None;
                cx.notify();
            }));
        for (key, label, place) in [
            ("project-action-rename", text::RENAME, Place::Rename),
            ("project-action-colour", text::COLOUR, Place::Colour),
            (
                "project-action-categories",
                text::CATEGORIES_TO_DETECT,
                Place::Categories,
            ),
        ] {
            let id = id.clone();
            menu = menu.child(
                ui::control(key, label, theme, true)
                    .when(cfg!(test), move |v| v.debug_selector(move || key.into()))
                    .on_click(cx.listener(move |this, _, window, cx| {
                        this.project_ui.header_menu = None;
                        this.show_project_page(&id, place, window, cx);
                    })),
            );
        }
        // Its Markdown and originals as plain files (#1091 Decision 22).
        let export = id.clone();
        menu = menu.child(
            ui::control("project-action-export", text::EXPORT_PROJECT, theme, true)
                .when(cfg!(test), |v| {
                    v.debug_selector(|| "project-action-export".into())
                })
                .on_click(cx.listener(move |this, _, window, cx| {
                    this.project_ui.header_menu = None;
                    this.export_project(export.clone(), window, cx);
                })),
        );
        let delete = id.clone();
        menu = menu.child(ui::menu_divider(theme)).child(
            ui::control("project-action-delete", text::DELETE_PROJECT, theme, true)
                .text_color(style::markdown_style(theme).popover_danger)
                .when(cfg!(test), |v| {
                    v.debug_selector(|| "project-action-delete".into())
                })
                .on_click(cx.listener(move |this, _, window, cx| {
                    this.project_ui.header_menu = None;
                    this.delete_project(delete.clone(), window, cx);
                })),
        );
        Some(
            gpui::deferred(
                gpui::anchored()
                    .position(*position)
                    .snap_to_window()
                    .child(menu),
            )
            .with_priority(2)
            .into_any_element(),
        )
    }

    pub(in crate::workspace) fn project_page(
        &mut self,
        window: &Window,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let page = self.project_ui.page.as_ref().unwrap();
        let Some(project) = self.projects.project(&page.id).cloned() else {
            let gone = crate::workspace::sidebar::navigation::Stop::Project(page.id.clone());
            self.project_ui.page = None;
            self.tabs.show_home();
            self.note_destination(crate::workspace::sidebar::navigation::Arrival::Replace, cx);
            self.navigation.forget(&gone);
            return self.home_view(window, cx);
        };
        let theme = self.theme.get();
        let p = theme.pdf_style();
        let detached = self.title.detached.get();
        let bar = ui::title_drag(
            div()
                .flex()
                .items_center()
                .flex_shrink_0()
                .h(px(style::HEADER_HEIGHT))
                .pl(px(16. + self.title.inset.get()))
                .pr_2()
                .gap_2()
                .when(detached, |v| {
                    v.h(px(style::CARD_MARGIN + style::HEADER_HEIGHT))
                        .pt(px(style::CARD_MARGIN))
                })
                .text_size(px(13.)),
            self.title.armed.clone(),
        )
        .child(
            div()
                .size(px(8.))
                .rounded_full()
                .bg(style::project_color(ProjectLink::of(&project).hue(), theme)),
        )
        .child(text::PROJECT)
        .child(div().flex_1())
        .children(ui::notice_bell(&self.title, theme).map(|bell| ui::no_drag(div().child(bell))));
        let header = ui::title_bar(bar, (!detached).then_some(p.border), window, theme);
        let content = self.unlocked_project_page(&project, cx);
        let body = div()
            .id("project-page-scroll")
            .flex_1()
            .min_h_0()
            .overflow_y_scroll()
            .track_scroll(&page.scroll)
            .child(div().w_full().flex().justify_center().p_6().child(content));
        let drop_into = project.id.clone();
        div()
            .id("project-page")
            .when(cfg!(test), |v| v.debug_selector(|| "project-page".into()))
            .track_focus(self.focus_regions.home())
            .size_full()
            .flex()
            .flex_col()
            // Files dropped anywhere on the page are imported into it.
            .on_drop(
                cx.listener(move |this, paths: &gpui::ExternalPaths, window, cx| {
                    cx.stop_propagation();
                    this.home.drop = None;
                    this.drop_on_project(paths, &drop_into, window, cx);
                }),
            )
            .child(header)
            .child(if detached {
                // The card clips its corners; it wraps the scroll area so its
                // overflow_hidden does not replace the area's scrolling.
                ui::card_surface(
                    div()
                        .flex_1()
                        .min_h_0()
                        .flex()
                        .flex_col()
                        .ml(px(style::CARD_MARGIN))
                        .child(body),
                    theme,
                )
                .into_any_element()
            } else {
                body.into_any_element()
            })
            .into_any_element()
    }

    pub(in crate::workspace) fn page_control(
        &self,
        id: impl Into<SharedString>,
        label: impl Into<SharedString>,
        cx: &mut Context<Self>,
    ) -> ui::Control {
        let page = self.project_ui.page.as_ref().unwrap();
        let id = id.into();
        let focus = match id.as_ref() {
            "project-page-color" => page.color_focus.clone(),
            "page-category-PERSON" => page.category_focus.clone(),
            _ => page
                .controls
                .borrow_mut()
                .entry(id.to_string())
                .or_insert_with(|| cx.focus_handle().tab_index(0).tab_stop(true))
                .clone(),
        };
        ui::reveal_focus(
            ui::control(id.clone(), label, self.theme.get(), true)
                .track_focus(&focus)
                .when(cfg!(test), |v| v.debug_selector(move || id.to_string())),
            focus,
            page.scroll.clone(),
        )
    }

    pub(super) fn unlocked_project_page(
        &self,
        project: &Project,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let page = self.project_ui.page.as_ref().unwrap();
        let members = self.member_summaries(&project.id);
        let replies = self.project_replies(&project.id);
        let heading = self.project_heading(project, members.len(), replies.len(), cx);
        let segments = div()
            .id("project-segments")
            .role(gpui::Role::TabList)
            .aria_label(text::PROJECT_VIEW)
            .flex()
            .items_center()
            .gap_1()
            .p_1()
            .rounded_lg()
            .border_1()
            .border_color(self.theme.get().pdf_style().border)
            .children(
                (Segment::ALL.into_iter())
                    .filter(|segment| *segment != Segment::Activity || self.ai_connector_on())
                    .map(|segment| {
                        let p = self.theme.get().pdf_style();
                        let selected = page.segment == segment;
                        self.page_control(segment.control(), segment.label(), cx)
                            .role(gpui::Role::Tab)
                            .aria_selected(selected)
                            .px_3()
                            .when(selected, |v| v.bg(p.placeholder_bg).text_color(p.header_fg))
                            .when(!selected, |v| v.text_color(p.header_muted))
                            .on_click(cx.listener(move |this, _, _, cx| {
                                this.show_project_segment(segment, cx)
                            }))
                    }),
            );
        let body = match page.segment {
            Segment::Documents => self.documents_segment(project, &members, cx),
            Segment::Replies => self.replies_segment(project, &replies, cx),
            Segment::Names => self.names_segment(project, &members, cx),
            Segment::Activity if self.ai_connector_on() => self.ai_activity_segment(project, cx),
            Segment::Activity => self.documents_segment(project, &members, cx),
        };
        div()
            .w_full()
            .max_w(px(680.))
            .flex()
            .flex_col()
            .gap_5()
            .text_size(px(13.))
            .child(heading)
            .child(div().flex().child(segments))
            .child(body)
            .into_any_element()
    }

    /// Colour, title, counts, Add document and ⋯.
    pub(super) fn project_heading(
        &self,
        project: &Project,
        documents: usize,
        replies: usize,
        cx: &mut Context<Self>,
    ) -> gpui::Div {
        let page = self.project_ui.page.as_ref().unwrap();
        let theme = self.theme.get();
        let p = theme.pdf_style();
        let id = project.id.clone();
        let names = project.mapping.exported().len();
        let name = match self
            .project_ui
            .renaming
            .as_ref()
            .filter(|(id, _)| *id == project.id)
        {
            Some((_, input)) => div()
                .flex_1()
                .min_w_0()
                .border_1()
                .border_color(p.border)
                .rounded_sm()
                .p_1()
                .on_action(cx.listener(|this, _: &ConfirmProjectName, window, cx| {
                    this.finish_rename(true, window, cx);
                    cx.stop_propagation();
                }))
                .on_action(cx.listener(|this, _: &CancelProjectName, window, cx| {
                    this.finish_rename(false, window, cx);
                    cx.stop_propagation();
                }))
                // A click anywhere else saves the name, like Enter.
                .on_mouse_down_out(cx.listener(|this, _, window, cx| {
                    this.finish_rename(true, window, cx);
                }))
                .child(input.clone())
                .into_any_element(),
            None => self
                .page_control("project-page-rename", project.name.clone(), cx)
                .aria_label(text::rename(&project.name))
                .flex_1()
                .min_w_0()
                .truncate()
                .text_size(px(26.))
                .on_click(cx.listener({
                    let id = id.clone();
                    move |this, _, window, cx| this.start_rename_project(id.clone(), window, cx)
                }))
                .into_any_element(),
        };
        let color = style::project_color(ProjectLink::of(project).hue(), theme);
        let (add, menu) = (id.clone(), id.clone());
        div()
            .flex()
            .flex_col()
            .gap_2()
            .child(
                div()
                    .flex()
                    .items_center()
                    .gap_3()
                    .child(
                        self.page_control("project-page-color", "", cx)
                            .aria_label(text::PROJECT_COLOUR)
                            .p_2()
                            .child(div().size(px(16.)).rounded_full().bg(color))
                            .on_click(cx.listener(|this, _, _, cx| {
                                if let Some(page) = this.project_ui.page.as_mut() {
                                    page.swatches = !page.swatches;
                                }
                                cx.notify();
                            })),
                    )
                    .child(name)
                    .children(self.project_ask_button(&project.id, cx))
                    .child(
                        self.page_control("project-page-add-document", text::ADD_DOCUMENT, cx)
                            .flex_shrink_0()
                            .border_1()
                            .border_color(p.border)
                            .on_click(cx.listener(move |this, _, window, cx| {
                                this.add_document_to_project(add.clone(), window, cx)
                            })),
                    )
                    .child(
                        self.page_control("project-page-menu", "⋯", cx)
                            .aria_label(text::actions_for(&project.name))
                            .flex_shrink_0()
                            .on_click(cx.listener(
                                move |this, event: &gpui::ClickEvent, window, cx| {
                                    this.open_project_menu(&menu, event.position(), window, cx)
                                },
                            )),
                    ),
            )
            .child(
                div()
                    .id("project-page-counts")
                    .when(cfg!(test), |v| {
                        v.debug_selector(|| "project-page-counts".into())
                    })
                    .text_size(px(12.))
                    .text_color(p.header_muted)
                    .child(project_counts(documents, replies, names, project.created)),
            )
            .children(self.ai_project_chip(project, cx))
            .when(page.swatches, |v| {
                v.child(
                    div()
                        .flex()
                        .flex_wrap()
                        .gap_2()
                        .children(Colour::ALL.into_iter().map(|swatch| {
                            let id = id.clone();
                            let name = colour_name(swatch);
                            self.page_control(
                                SharedString::from(format!("project-swatch-{name}")),
                                name,
                                cx,
                            )
                            .flex()
                            .items_center()
                            .gap_2()
                            .child(
                                div()
                                    .size(px(12.))
                                    .rounded_full()
                                    .bg(style::project_color(swatch.hue(), theme)),
                            )
                            .aria_selected(
                                project
                                    .colour
                                    .unwrap_or_else(|| Colour::for_id(&project.id))
                                    == swatch,
                            )
                            .on_click(cx.listener(
                                move |this, _, window, cx| {
                                    this.set_project_color(&id, swatch, window, cx)
                                },
                            ))
                        })),
                )
            })
    }

    /// The Project's documents by title, each opening on a click, with
    /// Move to Unfiled.
    pub(super) fn documents_segment(
        &self,
        project: &Project,
        members: &[DocumentSummary],
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let p = self.theme.get().pdf_style();
        let mut list = div()
            .id("project-documents")
            .when(cfg!(test), |v| {
                v.debug_selector(|| "project-documents".into())
            })
            .flex()
            .flex_col()
            .gap_1();
        if members.is_empty() {
            list = list.child(empty_line(text::NO_DOCUMENTS, p));
        }
        for (index, summary) in members.iter().enumerate() {
            let Some(document) = VaultId::parse(&summary.id) else {
                continue;
            };
            let detail = text::member_detail(
                self.member_needs_review(summary, cx),
                &created_date(summary.updated),
            );
            let (open_document, remove_document) = (document.clone(), document);
            let id = project.id.clone();
            list = list.child(
                div()
                    .flex()
                    .items_center()
                    .gap_2()
                    .py_1()
                    .child(kind_badge(summary.source_kind, p))
                    .child(
                        self.page_control(
                            SharedString::from(format!("project-document-{index}")),
                            summary.title.clone(),
                            cx,
                        )
                        .flex_1()
                        .min_w_0()
                        .truncate()
                        .on_click(cx.listener(
                            move |this, _, window, cx| {
                                this.open_project_member(open_document.clone(), window, cx)
                            },
                        )),
                    )
                    .child(
                        div()
                            .flex_shrink_0()
                            .text_size(px(11.))
                            .text_color(p.header_muted)
                            .child(detail),
                    )
                    .child(
                        self.page_control(
                            SharedString::from(format!("project-remove-{index}")),
                            text::MOVE_TO_UNFILED,
                            cx,
                        )
                        .flex_shrink_0()
                        .aria_label(text::move_to_unfiled(&summary.title))
                        .on_click(cx.listener(
                            move |this, _, window, cx| {
                                this.remove_project_member(&id, remove_document.clone(), window, cx)
                            },
                        )),
                    ),
            );
        }
        list.into_any_element()
    }

    /// The Project's AI Replies, newest first, each opening restored, with
    /// Delete….
    pub(super) fn replies_segment(
        &self,
        _project: &Project,
        replies: &[ReplySummary],
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let p = self.theme.get().pdf_style();
        let titles = self.document_titles();
        let mut list = div()
            .id("project-replies")
            .when(cfg!(test), |v| {
                v.debug_selector(|| "project-replies".into())
            })
            .flex()
            .flex_col()
            .gap_1();
        if replies.is_empty() {
            list = list.child(empty_line(text::NO_REPLIES, p));
        }
        for (index, reply) in replies.iter().enumerate() {
            let detail = crate::workspace::replies::reply_detail(
                &reply.owner,
                reply.source_app.as_deref(),
                reply.created,
                &titles,
            );
            let (open, delete) = (reply.id.clone(), reply.id.clone());
            list = list.child(
                div()
                    .flex()
                    .items_center()
                    .gap_2()
                    .py_1()
                    .child(
                        div()
                            .flex_1()
                            .min_w_0()
                            .flex()
                            .flex_col()
                            .child(
                                self.page_control(
                                    SharedString::from(format!("project-reply-{index}")),
                                    reply.title.clone(),
                                    cx,
                                )
                                .truncate()
                                .on_click(cx.listener(
                                    move |this, _, window, cx| this.open_reply(&open, window, cx),
                                )),
                            )
                            .child(
                                div()
                                    .px_2()
                                    .text_size(px(11.))
                                    .text_color(p.header_muted)
                                    .truncate()
                                    .child(detail),
                            ),
                    )
                    .child(
                        self.page_control(
                            SharedString::from(format!("project-reply-delete-{index}")),
                            text::DELETE,
                            cx,
                        )
                        .flex_shrink_0()
                        .aria_label(text::delete_named(&reply.title))
                        .on_click(cx.listener(
                            move |this, _, window, cx| {
                                this.confirm_delete_reply(&delete, window, cx)
                            },
                        )),
                    ),
            );
        }
        list.into_any_element()
    }

    /// The Project's names, searchable and grouped by category, with Add a
    /// name, the Always and Never lists and Categories to detect.
    pub(super) fn names_segment(
        &self,
        project: &Project,
        members: &[DocumentSummary],
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let page = self.project_ui.page.as_ref().unwrap();
        let theme = self.theme.get();
        let p = theme.pdf_style();
        let id = project.id.clone();
        let filter = page.filter.read(cx).value().to_lowercase();
        let mut names = div().flex().flex_col().gap_2().child(ui::reveal_focus(
            div()
                .id("project-page-filter")
                .when(cfg!(test), |v| {
                    v.debug_selector(|| "project-page-filter".into())
                })
                .border_1()
                .border_color(p.border)
                .rounded_sm()
                .p_1()
                .child(page.filter.clone()),
            page.filter.read(cx).focus_handle(cx),
            page.scroll.clone(),
        ));
        // Mentions in each live document of the Project, by alias.
        let open_members: Vec<_> = (members.iter())
            .filter_map(|summary| {
                let document = DocumentId::Vault(VaultId::parse(&summary.id)?);
                let tab = self.find_document(&document, None, cx)?;
                Some((self.tabs.get(tab)?.view()?.clone(), summary.title.clone()))
            })
            .collect();
        let mut shown = 0;
        for category in mdoc_pii::Category::ALL {
            let identities: Vec<_> = (project.mapping.identities.iter())
                .filter(|identity| {
                    !identity.originals.is_empty()
                        && identity.category == category.token()
                        && (filter.is_empty()
                            || format!(
                                "{} {} {}",
                                identity.alias,
                                identity.originals.join(" "),
                                category.label()
                            )
                            .to_lowercase()
                            .contains(&filter))
                })
                .collect();
            if identities.is_empty() {
                continue;
            }
            shown += identities.len();
            names = names.child(
                div()
                    .pt_2()
                    .text_size(px(12.))
                    .text_color(p.header_muted)
                    .child(category.label()),
            );
            for identity in identities {
                let alias = identity.alias.clone();
                let id = id.clone();
                let counts = (open_members.iter())
                    .map(|(view, title)| {
                        let review = view.read(cx).pii.review();
                        let count = review
                            .identity_by_alias(&identity.alias)
                            .map_or(0, |id| review.identity_count(id));
                        text::open_count(title, count)
                    })
                    .collect::<Vec<_>>()
                    .join(text::DOT);
                names = names.child(
                    div()
                        .id(SharedString::from(format!("project-identity-{alias}")))
                        .when(cfg!(test), {
                            let alias = alias.clone();
                            move |v| v.debug_selector(move || format!("project-identity-{alias}"))
                        })
                        .flex()
                        .items_start()
                        .gap_2()
                        .py_1()
                        .child(
                            div()
                                .flex_1()
                                .min_w_0()
                                .flex()
                                .flex_col()
                                .gap_1()
                                .child(div().text_size(px(13.)).child(text::identity_line(
                                    &identity.alias,
                                    identity.originals.first().map_or("", String::as_str),
                                )))
                                .child(
                                    div()
                                        .text_size(px(11.))
                                        .text_color(p.header_muted)
                                        .child(text::variants(identity.originals.len(), &counts)),
                                ),
                        )
                        .child(
                            self.page_control(
                                SharedString::from(format!("project-forget-{alias}")),
                                text::FORGET_IDENTITY_MENU,
                                cx,
                            )
                            .on_click(cx.listener(
                                move |this, _, window, cx| {
                                    this.confirm_forget_project_identity(&id, &alias, window, cx)
                                },
                            )),
                        ),
                );
            }
        }
        if shown == 0 {
            names = names.child(empty_line(
                if filter.is_empty() {
                    text::NO_NAMES
                } else {
                    text::NO_NAMES_MATCH
                },
                p,
            ));
        }
        names = names.child(
            div()
                .flex()
                .items_center()
                .gap_2()
                .pt_2()
                .on_action(cx.listener(|this, _: &AddProjectName, _, cx| {
                    this.add_page_name(cx);
                    cx.stop_propagation();
                }))
                .on_action(cx.listener(|this, _: &ClearProjectName, _, cx| {
                    if let Some(page) = &this.project_ui.page {
                        page.add.update(cx, |input, cx| input.reset(cx));
                    }
                    cx.stop_propagation();
                }))
                .child(ui::reveal_focus(
                    div()
                        .flex_1()
                        .border_1()
                        .border_color(p.border)
                        .rounded_sm()
                        .p_1()
                        .child(page.add.clone()),
                    page.add.read(cx).focus_handle(cx),
                    page.scroll.clone(),
                ))
                .child(
                    self.page_control("project-page-add", text::ADD_A_NAME, cx)
                        .on_click(cx.listener(|this, _, _, cx| this.add_page_name(cx))),
                ),
        );
        let skipped = project.skipped();
        let categories = div()
            .flex()
            .flex_col()
            .gap_2()
            .child(section(text::CATEGORIES_TO_DETECT))
            .child(
                div()
                    .text_size(px(12.))
                    .text_color(p.header_muted)
                    .child(text::CATEGORIES_NOTE),
            )
            .child(
                div().flex().flex_wrap().gap_2().children(
                    mdoc_pii::Category::ALL
                        .into_iter()
                        .filter(|category| *category != mdoc_pii::Category::Other)
                        .map(|category| {
                            let id = id.clone();
                            let on = !skipped.contains(&category);
                            self.page_control(
                                SharedString::from(format!("page-category-{}", category.token())),
                                text::category_toggle(on, category.detection_label()),
                                cx,
                            )
                            .role(gpui::Role::CheckBox)
                            .aria_toggled(if on {
                                gpui::Toggled::True
                            } else {
                                gpui::Toggled::False
                            })
                            .on_click(cx.listener(
                                move |this, _, _, cx| {
                                    this.toggle_project_category(&id, category, cx)
                                },
                            ))
                        }),
                ),
            );
        let lists = self.project_lists(project, cx);
        div()
            .flex()
            .flex_col()
            .gap_6()
            .child(names)
            .child(lists)
            .child(categories)
            .into_any_element()
    }

    /// The Project's Always and Never lists: what its members' manual marks
    /// and Keeps put there, each with Remove. Entries are added only by
    /// marking and keeping in a member.
    pub(super) fn project_lists(&self, project: &Project, cx: &mut Context<Self>) -> gpui::Div {
        let p = self.theme.get().pdf_style();
        let mut lists = div()
            .flex()
            .flex_col()
            .gap_2()
            .child(section(text::ALWAYS_AND_NEVER))
            .child(
                div()
                    .text_size(px(12.))
                    .text_color(p.header_muted)
                    .child(text::LISTS_NOTE),
            );
        // Each list's wordings, with the category of an Always one.
        type Entries = Vec<(String, Option<&'static str>)>;
        let entries: [(bool, Entries); 2] = [
            (
                true,
                project
                    .lists
                    .always
                    .iter()
                    .map(|e| (e.wording.clone(), Some(e.category().label())))
                    .collect(),
            ),
            (
                false,
                project
                    .lists
                    .never
                    .iter()
                    .map(|w| (w.clone(), None))
                    .collect(),
            ),
        ];
        for (always, entries) in entries {
            let list = if always { "always" } else { "never" };
            lists = lists.child(
                div()
                    .pt_2()
                    .text_size(px(12.))
                    .text_color(p.header_muted)
                    .child(if always {
                        text::ALWAYS_REPLACE
                    } else {
                        text::NEVER_REPLACE
                    }),
            );
            if entries.is_empty() {
                lists = lists.child(div().text_size(px(12.)).text_color(p.header_muted).child(
                    if always {
                        text::NO_ALWAYS
                    } else {
                        text::NO_NEVER
                    },
                ));
            }
            for (index, (wording, category)) in entries.into_iter().enumerate() {
                let control = format!("project-{list}-remove-{index}");
                let (id, remove) = (project.id.clone(), wording.clone());
                lists = lists.child(
                    div()
                        .flex()
                        .items_center()
                        .gap_2()
                        .py_1()
                        .child(div().flex_1().min_w_0().truncate().child(wording.clone()))
                        .children(category.map(|label| {
                            div()
                                .text_size(px(11.))
                                .text_color(p.header_muted)
                                .child(label)
                        }))
                        .child(
                            self.page_control(SharedString::from(control), text::REMOVE, cx)
                                .aria_label(text::remove_named(&wording))
                                .on_click(cx.listener(move |this, _, _, cx| {
                                    this.remove_project_list_entry(&id, always, &remove, cx)
                                })),
                        ),
                );
            }
        }
        lists
    }
}
