//! How the companion draws: what is shared with Hide, the latest review
//! request, the AI project's documents, the latest reply, the drop target,
//! the shelf's rows and the footer, with the pointer watch that hides the
//! reply and the proposals again.
use super::{
    BUTTONS, CompanionView, DIVIDER, DOCUMENT_ROW, DROP, FOOTER, HEADER, HEADER_TOTAL, HEADING,
    PADDING, PROJECT_LINE, PROPOSAL_ROW, PROPOSALS_PADDING, REPLY_TEXT, SECTION_GAP,
    proposals_height, review_height,
};
use crate::{
    ai::text,
    ui,
    ui::style,
    workspace::{CompanionProposal, ReviewStep},
};
use gpui::{
    AnyElement, Context, DispatchPhase, ExternalPaths, MouseExitEvent, MouseMoveEvent, Render,
    SharedString, Window, div, prelude::*, px,
};

impl CompanionView {
    /// Hears the pointer leave the window, and move in it again.
    fn pointer_watch(&self, cx: &mut Context<Self>) -> AnyElement {
        let view = cx.entity().downgrade();
        gpui::canvas(
            |_, _, _| {},
            move |_, _, window, _| {
                let left = view.clone();
                window.on_mouse_event(move |_: &MouseExitEvent, phase, _, cx| {
                    if phase == DispatchPhase::Bubble {
                        let _ = left.update(cx, |this, cx| this.pointer_left(cx));
                    }
                });
                let moved = view.clone();
                window.on_mouse_event(move |_: &MouseMoveEvent, phase, _, cx| {
                    if phase == DispatchPhase::Bubble {
                        let _ = moved.update(cx, |this, cx| this.pointer_moved(cx));
                    }
                });
            },
        )
        .absolute()
        .size_0()
        .into_any_element()
    }

    fn heading(&self, text: impl Into<SharedString>) -> gpui::Div {
        let p = self.theme.get().pdf_style();
        div()
            .h(px(HEADING))
            .px(px(4.))
            .flex()
            .items_end()
            .text_size(px(11.))
            .font_weight(gpui::FontWeight::SEMIBOLD)
            .text_color(p.header_muted)
            .child(text.into())
    }

    fn divider(&self) -> AnyElement {
        let p = self.theme.get().pdf_style();
        div()
            .h(px(DIVIDER))
            .flex()
            .items_center()
            .px(px(4.))
            .child(div().w_full().h(px(1.)).bg(p.border))
            .into_any_element()
    }

    /// "SHARED WITH AI APPS" and Hide; then the Project with Stop
    /// sharing, or "No Project shared". Always shown: what is shared is
    /// always named.
    fn shared(&self, cx: &mut Context<Self>) -> AnyElement {
        let theme = self.theme.get();
        let p = theme.pdf_style();
        let header = div()
            .h(px(HEADER))
            .flex()
            .items_center()
            .justify_between()
            .child(
                div()
                    .px(px(4.))
                    .text_size(px(10.))
                    .font_weight(gpui::FontWeight::SEMIBOLD)
                    .text_color(p.header_muted)
                    .child(text::SHARED_WITH_AI_APPS_HEADING),
            )
            .child(
                ui::header_icon("companion-hide", text::HIDE, ui::Icon::Close, theme, false)
                    .track_focus(&self.hide_focus)
                    .size(px(22.))
                    .on_click(cx.listener(|this, _, window, cx| this.hide(window, cx))),
            );
        let line = div()
            .id("companion-project")
            .when(cfg!(test), |v| {
                v.debug_selector(|| "companion-project".into())
            })
            .role(gpui::Role::Group)
            .aria_label(text::SHARED_WITH_AI_APPS)
            .h(px(PROJECT_LINE))
            .px(px(4.))
            .flex()
            .items_center()
            .gap_2();
        let line = match &self.content.shared {
            Some(project) => line
                .child(
                    div()
                        .flex_shrink_0()
                        .size(px(8.))
                        .rounded_full()
                        .bg(style::project_color(project.hue, theme)),
                )
                .child(
                    div()
                        .flex_1()
                        .min_w_0()
                        .truncate()
                        .font_weight(gpui::FontWeight::SEMIBOLD)
                        .child(project.name.clone()),
                )
                .child(
                    ui::link_button("companion-stop-sharing", text::STOP_SHARING, theme, true)
                        .when(cfg!(test), |v| {
                            v.debug_selector(|| "companion-stop-sharing".into())
                        })
                        .flex_shrink_0()
                        .text_size(px(12.))
                        .on_click(cx.listener(|this, _, _, cx| this.stop_sharing(cx))),
                ),
            None => line.child(div().text_color(p.header_muted).child(
                match self.content.running {
                    true => text::NO_PROJECT_SHARED,
                    false => text::NO_PROJECT_SHARED_LOCKED,
                },
            )),
        };
        div()
            .flex()
            .flex_col()
            .child(header)
            .child(line)
            .into_any_element()
    }

    /// The latest review request: who asked for which document, then its
    /// proposals with Replace all, that it is Ready, or Open in mdoc for
    /// what only mdoc decides.
    fn review_card(&self, cx: &mut Context<Self>) -> Option<AnyElement> {
        let review = self.content.review.as_ref()?;
        let theme = self.theme.get();
        let p = theme.pdf_style();
        let (title, line) = match &review.step {
            ReviewStep::Ready => (
                text::review_ready(&review.label),
                text::can_read_it_now(&review.from),
            ),
            ReviewStep::InMdoc(state) => (
                text::asked_for(&review.from, &review.label),
                text::finish_in_mdoc(state.strip_prefix(text::NEEDS_REVIEW)),
            ),
            ReviewStep::Proposals(_) | ReviewStep::Replacing(_) => (
                text::asked_for(&review.from, &review.label),
                text::NEEDS_REVIEW_BEFORE.to_string(),
            ),
        };
        let open = |primary: bool, cx: &mut Context<Self>| {
            let id = "companion-review-open";
            let button = match primary {
                true => ui::primary_button(id, text::OPEN_IN_MDOC, theme, true),
                false => ui::action_control(id, text::OPEN_IN_MDOC, theme, true, false),
            };
            button
                .when(cfg!(test), |v| {
                    v.debug_selector(|| "companion-review-open".into())
                })
                .on_click(cx.listener(|this, _, _, cx| this.open_in_mdoc(cx)))
        };
        let buttons = div().flex().justify_end().items_center().gap_2();
        let (proposals, buttons) = match &review.step {
            ReviewStep::Proposals(proposals) => (
                Some(self.proposals(proposals, cx)),
                Some(
                    buttons.child(open(false, cx)).child(
                        ui::primary_button("companion-replace-all", text::REPLACE_ALL, theme, true)
                            .when(cfg!(test), |v| {
                                v.debug_selector(|| "companion-replace-all".into())
                            })
                            .on_click(cx.listener(|this, _, _, cx| this.replace_all(cx))),
                    ),
                ),
            ),
            ReviewStep::Replacing(proposals) => (
                Some(self.proposals(proposals, cx)),
                Some(buttons.child(open(false, cx)).child(ui::primary_button(
                    "companion-replace-all",
                    text::REPLACING,
                    theme,
                    false,
                ))),
            ),
            ReviewStep::InMdoc(_) => (None, Some(buttons.child(open(true, cx)))),
            ReviewStep::Ready => (None, None),
        };
        Some(
            div()
                .id("companion-review")
                .when(cfg!(test), |v| {
                    v.debug_selector(|| "companion-review".into())
                })
                .role(gpui::Role::Group)
                .aria_label(title.clone())
                .h(px(review_height(&review.step)))
                .p_2()
                .flex()
                .flex_col()
                .gap_1()
                .rounded_md()
                .border_1()
                .border_color(p.border)
                .bg(p.bg)
                .child(
                    div()
                        .truncate()
                        .font_weight(gpui::FontWeight::SEMIBOLD)
                        .child(title),
                )
                .child(
                    div()
                        .truncate()
                        .text_size(px(12.))
                        .text_color(p.header_muted)
                        .child(line),
                )
                .children(proposals)
                .when_some(buttons, |card, buttons| {
                    card.child(div().flex_1()).child(buttons)
                })
                .into_any_element(),
        )
    }

    /// A review's proposals, each wording with the alias Replace all
    /// writes for it: a placeholder until the pointer is over them.
    fn proposals(&self, proposals: &[CompanionProposal], cx: &mut Context<Self>) -> AnyElement {
        let p = self.theme.get().pdf_style();
        let shown = self.shows_proposals();
        let names = text::names_to_replace(proposals.len());
        let list = div()
            .id("companion-proposals")
            .when(cfg!(test), |v| {
                v.debug_selector(|| "companion-proposals".into())
            })
            .role(gpui::Role::List)
            .aria_label(match shown {
                true => text::PROPOSALS.to_string(),
                false => text::proposals_hidden(&names),
            })
            .h(px(proposals_height(proposals)))
            .p(px(PROPOSALS_PADDING))
            .rounded_md()
            .border_1()
            .border_color(p.border)
            .text_size(px(12.))
            .overflow_y_scroll()
            .on_hover(
                cx.listener(|this, hovered: &bool, _, cx| this.set_proposals_hovered(*hovered, cx)),
            );
        let list = match shown {
            // The names are in the window only while shown.
            true => list.children(proposals.iter().enumerate().map(|(i, proposal)| {
                div()
                    .id(("companion-proposal", i))
                    .role(gpui::Role::ListItem)
                    .aria_label(text::proposal(
                        &proposal.original,
                        &proposal.alias,
                        proposal.mentions,
                    ))
                    .h(px(PROPOSAL_ROW))
                    .flex()
                    .items_center()
                    .gap_1()
                    .child(div().min_w_0().truncate().child(proposal.original.clone()))
                    .child(
                        div()
                            .flex_shrink_0()
                            .text_color(p.header_muted)
                            .child("\u{2192}"),
                    )
                    .child(
                        div()
                            .flex_1()
                            .min_w_0()
                            .truncate()
                            .text_color(p.header_muted)
                            .child(proposal.alias.clone()),
                    )
                    .when(proposal.mentions > 1, |row| {
                        row.child(
                            div()
                                .flex_shrink_0()
                                .text_color(p.header_muted)
                                .child(format!("\u{d7}{}", proposal.mentions)),
                        )
                    })
            })),
            false => list.child(
                div()
                    .size_full()
                    .flex()
                    .items_center()
                    .justify_center()
                    .text_color(p.header_muted)
                    .child(text::point_to_see(&names)),
            ),
        };
        #[cfg(all(
            debug_assertions,
            feature = "headless-renders",
            any(target_os = "macos", target_os = "windows")
        ))]
        let list = {
            let bounds = self.proposals_bounds.clone();
            list.child(
                gpui::canvas(move |b, _, _| bounds.set(Some(b)), |_, _, _, _| {})
                    .absolute()
                    .top_0()
                    .left_0()
                    .size_full(),
            )
        };
        div().relative().child(list).into_any_element()
    }

    /// The AI project's documents, by AI label, with their state.
    fn documents(&self) -> Option<AnyElement> {
        self.content.shared.as_ref()?;
        let p = self.theme.get().pdf_style();
        let line = |text: String| {
            div()
                .h(px(DOCUMENT_ROW))
                .px(px(4.))
                .flex()
                .items_center()
                .text_size(px(12.))
                .text_color(p.header_muted)
                .child(text)
        };
        let list = div()
            .id("companion-documents")
            .when(cfg!(test), |v| {
                v.debug_selector(|| "companion-documents".into())
            })
            .role(gpui::Role::List)
            .aria_label(text::DOCUMENTS_AI_APPS_CAN_ASK_FOR)
            .flex()
            .flex_col();
        let list = match &self.content.documents {
            Err(error) => list.child(line(text::documents_not_listed(error))),
            Ok(documents) if documents.is_empty() => list.child(line(text::NO_DOCUMENTS.into())),
            Ok(documents) => list.children(documents.iter().enumerate().map(|(i, document)| {
                div()
                    .id(("companion-document", i))
                    .h(px(DOCUMENT_ROW))
                    .px(px(4.))
                    .flex()
                    .items_center()
                    .gap_2()
                    .text_size(px(12.))
                    .role(gpui::Role::ListItem)
                    .aria_label(format!("{}, {}", document.label, document.state))
                    .child(
                        div()
                            .flex_1()
                            .min_w_0()
                            .truncate()
                            .child(document.label.clone()),
                    )
                    .child(
                        div()
                            .flex_shrink_0()
                            .text_color(if document.ready {
                                p.header_fg
                            } else {
                                p.header_muted
                            })
                            .child(document.state.clone()),
                    )
            })),
        };
        Some(
            div()
                .flex()
                .flex_col()
                .child(self.heading(text::DOCUMENTS))
                .child(list)
                .into_any_element(),
        )
    }

    /// The latest reply: a placeholder until the pointer is over it, then
    /// its restored text; Copy and Save as document either way.
    fn reply(&self, cx: &mut Context<Self>) -> Option<AnyElement> {
        let reply = self.content.reply.as_ref()?;
        let theme = self.theme.get();
        let p = theme.pdf_style();
        let shown = self.shows_reply();
        let text = div()
            .id("companion-reply")
            .when(cfg!(test), |v| {
                v.debug_selector(|| "companion-reply".into())
            })
            .role(gpui::Role::Group)
            .aria_label(match shown {
                true => text::REPLY_RESTORED,
                false => text::REPLY_HIDDEN,
            })
            .h(px(REPLY_TEXT))
            .p_2()
            .rounded_md()
            .border_1()
            .border_color(p.border)
            .bg(p.bg)
            .overflow_y_scroll()
            .on_hover(cx.listener(|this, hovered: &bool, _, cx| this.set_reply_shown(*hovered, cx)))
            .child(match shown {
                // The restored text is in the window only while shown.
                true => div()
                    .text_size(px(12.))
                    .child(reply.restored.clone())
                    .into_any_element(),
                false => div()
                    .size_full()
                    .flex()
                    .items_center()
                    .justify_center()
                    .text_size(px(12.))
                    .text_color(p.header_muted)
                    .child(text::POINT_TO_READ)
                    .into_any_element(),
            });
        #[cfg(all(
            debug_assertions,
            feature = "headless-renders",
            any(target_os = "macos", target_os = "windows")
        ))]
        let text = {
            let bounds = self.reply_bounds.clone();
            text.child(
                gpui::canvas(move |b, _, _| bounds.set(Some(b)), |_, _, _, _| {})
                    .absolute()
                    .top_0()
                    .left_0()
                    .size_full(),
            )
        };
        let copied = self.copied.is_some();
        let buttons = div()
            .h(px(BUTTONS))
            .flex()
            .items_center()
            .justify_end()
            .gap_2()
            .child(
                ui::action_control(
                    "companion-reply-copy",
                    ui::copy_label(copied, text::COPY),
                    theme,
                    true,
                    false,
                )
                .on_click(cx.listener(|this, _, window, cx| this.copy_reply(window, cx))),
            )
            .child(
                ui::action_control(
                    "companion-reply-save",
                    match (reply.saved, reply.saving, reply.failed) {
                        (true, _, _) => text::SAVED_AS_DOCUMENT,
                        (false, true, _) => text::SAVING,
                        (false, false, true) => text::NOT_SAVED,
                        (false, false, false) => text::SAVE_AS_DOCUMENT,
                    },
                    theme,
                    !reply.saved && !reply.saving,
                    false,
                )
                .on_click(cx.listener(|this, _, _, cx| this.save_reply(cx))),
            );
        Some(
            div()
                .flex()
                .flex_col()
                .child(self.heading(text::latest_reply(&reply.from)))
                .child(div().relative().child(text))
                .child(buttons)
                .into_any_element(),
        )
    }

    /// Where files dropped are imported: into the AI project, or opened in
    /// mdoc while nothing is shared.
    fn drop_target(&self, cx: &mut Context<Self>) -> AnyElement {
        let theme = self.theme.get();
        let p = theme.pdf_style();
        let label = match &self.content.shared {
            Some(project) => text::drop_to_add(&project.name),
            None => text::DROP_TO_OPEN.into(),
        };
        div()
            .id("companion-drop")
            .when(cfg!(test), |v| v.debug_selector(|| "companion-drop".into()))
            .h(px(DROP))
            .px_2()
            .flex()
            .items_center()
            .justify_center()
            .rounded_md()
            .border_1()
            .border_color(p.border)
            .text_size(px(12.))
            .text_color(p.header_muted)
            .drag_over::<ExternalPaths>(move |v, _, _, _| v.bg(theme.sidebar_selected()))
            .on_drop(cx.listener(|this, paths: &ExternalPaths, _, cx| {
                cx.stop_propagation();
                this.dropped(paths, cx);
            }))
            .child(div().truncate().child(label))
            .into_any_element()
    }

    /// Open mdoc at the left; Settings and Quit at the right.
    fn footer(&self, cx: &mut Context<Self>) -> AnyElement {
        let theme = self.theme.get();
        let row = |id: &'static str, label: &'static str| {
            ui::control(id, label, theme, true)
                .h(px(FOOTER))
                .flex()
                .items_center()
        };
        div()
            .h(px(FOOTER))
            .flex()
            .flex_row()
            .items_center()
            .child(
                row("companion-open-mdoc", text::OPEN_MDOC)
                    .on_click(cx.listener(|this, _, _, cx| this.open_mdoc(cx))),
            )
            .child(div().flex_1())
            .child(
                ui::header_icon(
                    "companion-settings",
                    text::SETTINGS,
                    ui::Icon::Settings,
                    theme,
                    false,
                )
                .on_click(cx.listener(|this, _, _, cx| this.open_settings(cx))),
            )
            .child(
                row("companion-quit", text::QUIT)
                    .on_click(cx.listener(|this, _, window, cx| this.quit(window, cx))),
            )
            .into_any_element()
    }
}

impl Render for CompanionView {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let theme = self.theme.get();
        let p = theme.pdf_style();
        // What is shared stays in view while the rest scrolls.
        let header = div()
            .flex_shrink_0()
            .px(px(PADDING))
            .pt(px(PADDING))
            .child(self.shared(cx));
        let body = div()
            .relative()
            .flex_shrink_0()
            .flex()
            .flex_col()
            .gap(px(SECTION_GAP))
            .px(px(PADDING))
            .pt(px(SECTION_GAP))
            .pb(px(PADDING))
            .children(self.review_card(cx))
            .children(self.documents())
            .children(self.reply(cx))
            .child(self.drop_target(cx))
            .child(
                div()
                    .flex()
                    .flex_col()
                    .child(self.divider())
                    .child(self.rows.clone())
                    .child(self.divider())
                    .child(self.footer(cx)),
            )
            .when_some(self.fit.clone(), |body, fit| {
                body.child(
                    gpui::canvas(
                        move |bounds, window, cx| {
                            let height = HEADER_TOTAL + bounds.size.height.as_f32();
                            fit.measured(height, window, cx)
                        },
                        |_, _, _, _| {},
                    )
                    .absolute()
                    .top_0()
                    .left_0()
                    .size_full(),
                )
            });
        div()
            .id("companion")
            .when(cfg!(test), |v| v.debug_selector(|| "companion".into()))
            .key_context("Companion")
            // Tab reaches every control, as in the shelf.
            .on_action(
                cx.listener(|_, _: &ui::NextControl, window, cx| {
                    ui::cycle(window, cx, None, false)
                }),
            )
            .on_action(cx.listener(|_, _: &ui::PreviousControl, window, cx| {
                ui::cycle(window, cx, None, true)
            }))
            // A row's drag released over the companion is no drop.
            .on_drop(cx.listener(|this, _: &ExternalPaths, _, cx| {
                this.dropped_on_companion(cx);
            }))
            // Coming back to it, the pointer finds it up to date.
            .on_hover(cx.listener(|this, hovered: &bool, window, cx| {
                if *hovered {
                    this.refresh(window, cx);
                }
            }))
            .size_full()
            .flex()
            .flex_col()
            .bg(crate::shelf::shelf_ui::surface(theme))
            .text_color(p.header_fg)
            .text_size(px(13.))
            .child(self.pointer_watch(cx))
            .child(header)
            .child(
                div()
                    .id("companion-scroll")
                    .flex_1()
                    .min_h_0()
                    .overflow_y_scroll()
                    .child(body),
            )
    }
}
