//! The Reply page's view: its title bar, the reply restored with its place
//! and where it came from, and its controls. It calls the owner's methods,
//! never the Vault.
use crate::workspace::Workspace;
use crate::workspace::replies::text;
use crate::{ui, ui::style};
use gpui::{AnyElement, Context, FocusHandle, SharedString, Window, div, prelude::*, px};
use mdoc_vault::{Reply, ReplyOwner};

impl Workspace {
    pub(in crate::workspace) fn reply_page_view(
        &mut self,
        window: &Window,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let page = self.reply_page.as_ref().unwrap();
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
        .child(text::AI_REPLY)
        .child(div().flex_1())
        .children(ui::notice_bell(&self.title, theme).map(|bell| ui::no_drag(div().child(bell))));
        let header = ui::title_bar(bar, (!detached).then_some(p.border), window, theme);
        let content = match &page.loaded {
            Ok(reply) => self.reply_content(reply, cx),
            Err(error) => div()
                .p_6()
                .text_color(p.header_muted)
                .child(text::cant_be_opened(error))
                .into_any_element(),
        };
        let body = div()
            .id("reply-page-scroll")
            .flex_1()
            .min_h_0()
            .overflow_y_scroll()
            .track_scroll(&page.scroll)
            .child(div().w_full().flex().justify_center().p_6().child(content));
        div()
            .id("reply-page")
            .when(cfg!(test), |v| v.debug_selector(|| "reply-page".into()))
            .track_focus(self.focus_regions.home())
            .size_full()
            .flex()
            .flex_col()
            .child(header)
            .child(if detached {
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

    fn reply_content(&self, reply: &Reply, cx: &mut Context<Self>) -> AnyElement {
        let page = self.reply_page.as_ref().unwrap();
        let theme = self.theme.get();
        let p = theme.pdf_style();
        let (titles, place) = self.reply_place(reply);
        let meta = [
            place.map(|place| match &reply.owner {
                ReplyOwner::Project(_) => text::in_project(place),
                ReplyOwner::Document(_) if place == text::UNFILED => place,
                ReplyOwner::Document(_) => text::in_project(place),
            }),
            Some({
                let detail = reply_detail(
                    &reply.owner,
                    reply.source_app.as_deref(),
                    reply.created,
                    &titles,
                );
                // The place already names the Project.
                (detail.strip_prefix(text::FROM_THE_PROJECT))
                    .and_then(|rest| rest.strip_prefix(text::SEPARATOR))
                    .map_or(detail.clone(), str::to_string)
            }),
        ]
        .into_iter()
        .flatten()
        .collect::<Vec<_>>()
        .join(text::SEPARATOR);
        let control = |id: &'static str, label: SharedString, focus: &FocusHandle| {
            ui::reveal_focus(
                ui::primary_button(id, label, theme, true)
                    .track_focus(focus)
                    .when(cfg!(test), move |v| v.debug_selector(move || id.into())),
                focus.clone(),
                page.scroll.clone(),
            )
        };
        let id = page.id.clone();
        let unresolved = !page.restored.unresolved.is_empty();
        div()
            .w_full()
            .max_w(px(680.))
            .flex()
            .flex_col()
            .gap_3()
            .text_size(px(13.))
            .child(
                div()
                    .text_size(px(26.))
                    .font_weight(gpui::FontWeight::SEMIBOLD)
                    .child(reply.title.clone()),
            )
            .child(
                div()
                    .text_size(px(12.))
                    .text_color(p.header_muted)
                    .child(meta),
            )
            .child(
                div()
                    .id("reply-page-summary")
                    .when(cfg!(test), |v| {
                        v.debug_selector(|| "reply-page-summary".into())
                    })
                    .text_size(px(12.))
                    .text_color(p.header_muted)
                    .child(page.summary()),
            )
            .child(
                div()
                    .flex()
                    .flex_wrap()
                    .gap_2()
                    .child(
                        control(
                            "reply-page-copy",
                            if page.copied {
                                crate::ui::text::COPIED
                            } else {
                                text::COPY_REPLY
                            }
                            .into(),
                            &page.copy_focus,
                        )
                        .on_click(cx.listener(|this, _, _, cx| this.copy_saved_reply(cx))),
                    )
                    .when(unresolved, |v| {
                        v.child(
                            control(
                                "reply-page-next",
                                text::NEXT_UNRECOGNIZED.into(),
                                &page.next_focus,
                            )
                            .on_click(
                                cx.listener(|this, _, _, cx| this.next_saved_unrecognised(cx)),
                            ),
                        )
                    })
                    .child(
                        control(
                            "reply-page-promote",
                            text::PROMOTE_TO_DOCUMENT.into(),
                            &page.promote_focus,
                        )
                        .on_click(
                            cx.listener(|this, _, window, cx| this.promote_saved_reply(window, cx)),
                        ),
                    )
                    .child(
                        control(
                            "reply-page-delete",
                            text::DELETE_REPLY.into(),
                            &page.delete_focus,
                        )
                        .text_color(style::markdown_style(theme).popover_danger)
                        .on_click(cx.listener(
                            move |this, _, window, cx| this.confirm_delete_reply(&id, window, cx),
                        )),
                    ),
            )
            .child(
                div()
                    .id("reply-page-text")
                    .when(cfg!(test), |v| {
                        v.debug_selector(|| "reply-page-text".into())
                    })
                    .mt_2()
                    .p_4()
                    .rounded_md()
                    .border_1()
                    .border_color(p.border)
                    .child(page.editor.clone()),
            )
            .into_any_element()
    }
}

/// "From “Lease.pdf” · Claude · 23 Nov": the app by its product's name,
/// as a delivered reply keeps the id the app gave itself ("claude-ai").
pub(in crate::workspace) fn reply_detail(
    owner: &ReplyOwner,
    source_app: Option<&str>,
    created: u64,
    titles: &std::collections::HashMap<String, String>,
) -> String {
    let from = match owner {
        ReplyOwner::Document(document) => titles
            .get(document)
            .map_or_else(|| text::FROM_A_DOCUMENT.to_string(), text::from_document),
        ReplyOwner::Project(_) => text::FROM_THE_PROJECT.to_string(),
    };
    [
        Some(from),
        source_app.map(crate::ai::connector::display_name),
        Some(crate::workspace::projects::project_page::created_date(
            created,
        )),
    ]
    .into_iter()
    .flatten()
    .collect::<Vec<_>>()
    .join(text::SEPARATOR)
}
