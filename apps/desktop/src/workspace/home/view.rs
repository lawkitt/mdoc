//! Home's view: the header, the drop zone, the quick actions and the
//! Continue strip. It reads the owner's methods, never the Vault.

use super::PasteAsNewTab;
use super::text;
use crate::ui;
use crate::ui::style;
use crate::ui::style::Theme;
use crate::workspace::New;
use crate::workspace::Open;
use crate::workspace::PasteAiResponse;
use crate::workspace::PseudonymizeClipboard;
use crate::workspace::Workspace;
use crate::workspace::home;
use crate::workspace::palette::key;
use gpui::Animation;
use gpui::AnimationExt;
use gpui::AnyElement;
use gpui::Context;
use gpui::Hsla;
use gpui::Window;
use gpui::div;
use gpui::prelude::*;
use gpui::px;
use mdoc_store::document_id::VaultId;
use mdoc_vault::ProjectSummary;
use mdoc_vault::ReplyOwner;
use std::collections::HashMap;
use std::time::Duration;

pub(super) fn home_label(label: &'static str, theme: Theme) -> gpui::Div {
    div()
        .pt_2()
        .text_size(px(12.))
        .font_weight(gpui::FontWeight::MEDIUM)
        .text_color(theme.pdf_style().header_muted)
        .child(label)
}

impl Workspace {
    pub(in crate::workspace) fn home_view(
        &self,
        window: &Window,
        cx: &mut Context<Self>,
    ) -> AnyElement {
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
                .when(!detached, |v| v.border_b_1().border_color(p.border))
                .text_size(px(13.)),
            self.title.armed.clone(),
        )
        .child(
            div()
                .font_weight(gpui::FontWeight::MEDIUM)
                .child(text::HOME),
        )
        .child(div().flex_1())
        .children(ui::notice_bell(&self.title, theme).map(|bell| ui::no_drag(div().child(bell))));
        let header = ui::title_bar(bar, (!detached).then_some(p.border), window, theme);
        let banner = self.whats_new(cx);
        let body = div()
            .flex_1()
            .min_h_0()
            .flex()
            .flex_col()
            .children(banner)
            .child(
                div()
                    .id("home-scroll")
                    .flex_1()
                    .min_h_0()
                    .overflow_y_scroll()
                    .track_scroll(&self.shell.home_scroll)
                    .child(
                        div()
                            .w_full()
                            .flex()
                            .justify_center()
                            .px_6()
                            .pt(px(56.))
                            .pb(px(48.))
                            .child(
                                div()
                                    .w_full()
                                    .max_w(px(680.))
                                    .flex()
                                    .flex_col()
                                    .gap_4()
                                    .child(
                                        div()
                                            .flex()
                                            .flex_col()
                                            .gap_1()
                                            .child(
                                                div()
                                                    .text_size(px(26.))
                                                    .font_weight(gpui::FontWeight::SEMIBOLD)
                                                    .child(text::HELLO),
                                            )
                                            .child(
                                                div()
                                                    .text_size(px(14.))
                                                    .text_color(p.header_muted)
                                                    .child(text::PREPARE_FOR_AI),
                                            ),
                                    )
                                    .child(self.drop_zone(cx))
                                    .child(self.home_actions(cx))
                                    .children(self.home_strip(cx)),
                            ),
                    ),
            );
        // Collapsed sidebar: the header sits in the title row and Home in its
        // own card below. The header comes after the body in
        // element order, shown on top by the reversed column, so Home's
        // first Tab stop is Open files…, not the header's bell (U25).
        div()
            .id("home")
            .track_focus(self.focus_regions.home())
            .when(cfg!(test), |v| v.debug_selector(|| "home".into()))
            .size_full()
            .flex()
            .flex_col_reverse()
            .map(|v| {
                if detached {
                    v.child(ui::card_surface(body.ml(px(style::CARD_MARGIN)), theme))
                } else {
                    v.child(body)
                }
            })
            .child(header)
            .into_any_element()
    }

    /// One-time "What's new" after an update.
    pub(super) fn whats_new(&self, cx: &mut Context<Self>) -> Option<AnyElement> {
        use crate::app::updater::BannerKind;
        let banner = crate::app::updater::state(cx).banner()?;
        if banner.kind != BannerKind::Updated {
            return None;
        }
        let theme = self.theme.get();
        let p = theme.pdf_style();
        let version = banner.version.clone();
        Some(
            div()
                .flex()
                .items_center()
                .gap_3()
                .px_4()
                .py_2()
                .bg(p.placeholder_bg)
                .text_size(px(13.))
                .child(div().flex_1().child(text::installed(&version)))
                .child(
                    ui::link_button("whats-new", text::WHATS_NEW, theme, true)
                        .on_click(move |_, _, cx| Self::open_release_notes(&version, cx)),
                )
                .child(
                    ui::control("dismiss-whats-new", "×", theme, true)
                        .aria_label(crate::ui::text::DISMISS)
                        .on_click(|_, _, cx| Self::dismiss_whats_new(cx)),
                )
                .into_any_element(),
        )
    }

    pub(super) fn drop_zone(&self, cx: &mut Context<Self>) -> AnyElement {
        let theme = self.theme.get();
        let p = theme.pdf_style();
        let accent = theme.search_accent();
        let active = self.shell.drop.is_some_and(|n| n > 0);
        let refused = self.shell.drop == Some(0);
        let ink = if active { accent } else { p.header_muted };
        let layer = |data: &'static [u8], color: gpui::Hsla| {
            gpui::svg()
                .data(data)
                .absolute()
                .inset_0()
                .size_full()
                .text_color(color)
        };
        let illustration = div()
            .relative()
            .w(px(160.))
            .h(px(112.))
            .child(layer(
                include_bytes!("../../../resources/ui/drop-page.svg"),
                ink,
            ))
            .child(layer(
                include_bytes!("../../../resources/ui/drop-bars.svg"),
                if active {
                    accent
                } else {
                    Hsla {
                        a: 0.35,
                        ..p.header_muted
                    }
                },
            ))
            .child(layer(
                include_bytes!("../../../resources/ui/drop-disc.svg"),
                if active { accent } else { p.header_fg },
            ))
            .child(layer(
                include_bytes!("../../../resources/ui/drop-lock.svg"),
                p.bg,
            ));
        let illustration = if active {
            illustration
                .with_animation(
                    ("drop-lift", self.shell.drop_generation),
                    Animation::new(Duration::from_millis(180)).with_easing(gpui::ease_out_quint()),
                    |v, delta| v.mt(px(-6. * delta)).mb(px(6. * delta)),
                )
                .into_any_element()
        } else {
            illustration.into_any_element()
        };
        // Sealed with another key: the zone asks for the recovery key.
        if self.home_needs_recovery_key() {
            return self.recovery_panel(cx);
        }
        // No Vault this run: nothing can be added, and the zone says why.
        let unavailable = self.home_unavailable();
        let title = match self.shell.drop {
            _ if unavailable.is_some() => text::DOCUMENTS_UNAVAILABLE.to_string(),
            Some(0) => text::CANT_BE_OPENED.to_string(),
            Some(n) => text::drop_files(n),
            None => text::DROP_DOCUMENTS_HERE.to_string(),
        };
        div()
            .id("home-drop")
            .when(cfg!(test), |v| v.debug_selector(|| "home-drop".into()))
            .flex()
            .flex_col()
            .items_center()
            .gap_2()
            .pt_6()
            .pb_5()
            .rounded_xl()
            .border_1()
            .border_dashed()
            .border_color(Hsla {
                a: 0.45,
                ..p.header_muted
            })
            .when(active, |v| {
                v.border_2()
                    .border_color(accent)
                    .bg(Hsla { a: 0.05, ..accent })
            })
            .when(refused, |v| {
                v.border_color(style::markdown_style(theme).alert_caution)
            })
            .child(illustration)
            .child(
                div()
                    .text_size(px(15.))
                    .font_weight(gpui::FontWeight::MEDIUM)
                    .text_color(if active { accent } else { p.header_fg })
                    .child(title),
            )
            .child(
                ui::filled_button("home-open", text::OPEN_FILES, theme, unavailable.is_none())
                    .when(cfg!(test), |v| v.debug_selector(|| "home-open".into()))
                    .mt_1()
                    .child(ui::key_hint(key(&Open)))
                    .on_click(|_, window, cx| window.dispatch_action(Box::new(Open), cx)),
            )
            .child(
                div()
                    .flex()
                    .flex_col()
                    .items_center()
                    .text_center()
                    .px_4()
                    .text_size(px(12.))
                    .text_color(p.header_muted)
                    .map(|v| match unavailable {
                        Some(why) => v.max_w(px(460.)).child(why.to_owned()),
                        None => v
                            .child(mdoc_import::import::supported_formats())
                            .child(text::SEVERAL_AT_ONCE),
                    }),
            )
            .into_any_element()
    }

    pub(super) fn home_actions(&self, cx: &mut Context<Self>) -> AnyElement {
        let theme = self.theme.get();
        let p = theme.pdf_style();
        let machine = if cfg!(target_os = "macos") {
            text::THIS_MAC
        } else {
            text::THIS_COMPUTER
        };
        let detail = text::local_detail(machine);
        let intake = self.home_unavailable().is_none();
        div()
            .flex()
            .flex_wrap()
            .items_center()
            .gap_2()
            .child(
                ui::primary_button("home-new", text::NEW_DOCUMENT, theme, intake)
                    .when(cfg!(test), |v| v.debug_selector(|| "home-new".into()))
                    .on_click(|_, window, cx| window.dispatch_action(Box::new(New), cx)),
            )
            .child(
                ui::primary_button("home-paste", text::PASTE_TEXT, theme, intake)
                    .when(cfg!(test), |v| v.debug_selector(|| "home-paste".into()))
                    .on_click(|_, window, cx| window.dispatch_action(Box::new(PasteAsNewTab), cx)),
            )
            .child(
                ui::primary_button("home-restore", text::PASTE_AI_REPLY, theme, true)
                    .when(cfg!(test), |v| v.debug_selector(|| "home-restore".into()))
                    .on_click(|_, window, cx| {
                        window.dispatch_action(Box::new(PasteAiResponse), cx)
                    }),
            )
            .child(
                ui::primary_button("home-clipboard", text::PSEUDONYMIZE_CLIPBOARD, theme, true)
                    .when(cfg!(test), |v| v.debug_selector(|| "home-clipboard".into()))
                    .on_click(|_, window, cx| {
                        window.dispatch_action(Box::new(PseudonymizeClipboard), cx)
                    }),
            )
            .child(div().flex_1())
            .child(
                div()
                    .id("empty-page-local")
                    .when(cfg!(test), |v| {
                        v.debug_selector(|| "empty-page-local".into())
                    })
                    .flex()
                    .items_center()
                    .gap_1()
                    .text_size(px(12.))
                    .text_color(p.header_muted)
                    .aria_label(text::local_label(machine, &detail))
                    // Above the note, right-aligned with it: a pointer tooltip
                    // at the window's edge would flip over the buttons beside
                    // it (U16).
                    .relative()
                    .on_hover(cx.listener(|this, hovered: &bool, _, cx| {
                        this.hover_local_note(*hovered, cx)
                    }))
                    .child(
                        gpui::svg()
                            .data(include_bytes!("../../../resources/ui/lock.svg"))
                            .size(px(13.))
                            .text_color(p.header_muted),
                    )
                    .child(text::local_note(machine))
                    .when(self.shell.local_note_hover, |v| {
                        v.child(
                            gpui::deferred(
                                div()
                                    .absolute()
                                    .right_0()
                                    .bottom(gpui::relative(1.))
                                    .w(px(300.))
                                    // Clear of the taller buttons in the row.
                                    .pb(px(10.))
                                    .flex()
                                    .justify_end()
                                    .child(style::tip_body(detail, theme)),
                            )
                            .with_priority(1),
                        )
                    }),
            )
            .into_any_element()
    }

    /// Continue: documents that need review and saved replies, newest first.
    pub(super) fn home_strip(&self, cx: &mut Context<Self>) -> Option<AnyElement> {
        let (strip, documents) = self.home_strip_now(cx)?;
        if !strip.shown {
            return None;
        }
        let theme = self.theme.get();
        let p = theme.pdf_style();
        let projects: HashMap<String, ProjectSummary> = (self.projects.summaries().into_iter())
            .map(|project| (project.id.clone(), project))
            .collect();
        let documents_shown = (strip.cards.iter())
            .filter(|card| matches!(card, home::Card::Document { .. }))
            .count();
        let more = match strip.need_review {
            0 if !strip.cards.is_empty() => Some(text::NOTHING_NEEDS_REVIEW.to_string()),
            n if n <= documents_shown => None,
            n => Some(text::need_review(n)),
        };
        let header = div()
            .flex()
            .items_center()
            .gap_2()
            .pt_2()
            .child(home_label(text::CONTINUE, theme).pt_0())
            .child(div().flex_1())
            .children(more.map(|more| {
                div()
                    .when(cfg!(test), |v| {
                        v.debug_selector(|| "home-strip-more".into())
                    })
                    .text_size(px(12.))
                    .text_color(p.header_muted)
                    .child(more)
            }));
        let strip_box = div()
            .id("home-strip")
            .when(cfg!(test), |v| v.debug_selector(|| "home-strip".into()))
            .flex()
            .flex_col()
            .gap_2()
            .child(header);
        if strip.cards.is_empty() {
            return Some(
                strip_box
                    .child(
                        div()
                            .when(cfg!(test), |v| {
                                v.debug_selector(|| "home-strip-empty".into())
                            })
                            .text_size(px(13.))
                            .text_color(p.header_muted)
                            .child(text::NOTHING_NEEDS_REVIEW_SENTENCE),
                    )
                    .into_any_element(),
            );
        }
        // A document's place: its Project's dot and name, or Unfiled.
        let place = |project: Option<&str>| -> (Option<Hsla>, String) {
            match project.and_then(|id| projects.get(id)) {
                Some(project) => {
                    let link = crate::workspace::ProjectLink {
                        id: project.id.clone(),
                        name: project.name.clone(),
                        colour: project.colour,
                    };
                    (
                        Some(style::project_color(link.hue(), theme)),
                        project.name.clone(),
                    )
                }
                None => (None, text::UNFILED.to_string()),
            }
        };
        let now = home::now();
        let shown = strip.cards.len();
        let mut row = div().flex().gap_3();
        for (i, card) in strip.cards.into_iter().enumerate() {
            let age = home::age(card.at(), now);
            let (badge, title, (dot, place), status) = match &card {
                home::Card::Document { summary, proposed } => (
                    crate::workspace::sidebar::tree_ui::kind_label(summary),
                    summary.title.clone(),
                    place(summary.project.as_deref()),
                    match proposed {
                        Some(n) => text::to_review(*n),
                        None => text::NOT_REVIEWED.to_string(),
                    },
                ),
                home::Card::Reply(reply) => {
                    let project = match &reply.owner {
                        ReplyOwner::Project(id) => Some(id.as_str()),
                        ReplyOwner::Document(id) => (documents.iter())
                            .find(|summary| &summary.id == id)
                            .and_then(|summary| summary.project.as_deref()),
                    };
                    (
                        text::REPLY,
                        reply.title.clone(),
                        place(project),
                        text::SAVED_REPLY.to_string(),
                    )
                }
            };
            let label = text::card_label(&title, &place, &status, &age);
            let focus = (self.shell.card_focus.borrow_mut())
                .entry(card.id().to_string())
                .or_insert_with(|| cx.focus_handle().tab_stop(true))
                .clone();
            let view =
                crate::ui::focus_ring(("home-card", i), theme)
                    .when(cfg!(test), |v| {
                        v.debug_selector(move || format!("home-card-{i}"))
                    })
                    .role(gpui::Role::Button)
                    .aria_label(label)
                    .key_context("UiControl")
                    .tab_index(0)
                    .track_focus(&focus)
                    .flex_1()
                    .flex_basis(px(0.))
                    .min_w_0()
                    .flex()
                    .flex_col()
                    .gap_1()
                    .p_3()
                    .rounded_lg()
                    .border_1()
                    .border_color(p.border)
                    .text_size(px(13.))
                    .cursor_pointer()
                    .hover(move |s| s.bg(p.placeholder_bg))
                    .child(
                        div()
                            .flex()
                            .items_center()
                            .gap_2()
                            .child(
                                div()
                                    .flex_shrink_0()
                                    .text_size(px(10.))
                                    .text_color(p.header_muted)
                                    .child(badge),
                            )
                            .child(
                                div()
                                    .flex_1()
                                    .min_w_0()
                                    .truncate()
                                    .font_weight(gpui::FontWeight::MEDIUM)
                                    .child(title),
                            ),
                    )
                    .child(
                        div()
                            .flex()
                            .items_center()
                            .gap_2()
                            .text_size(px(12.))
                            .text_color(p.header_muted)
                            .children(dot.map(|dot| {
                                div().size(px(8.)).rounded_full().bg(dot).flex_shrink_0()
                            }))
                            .child(div().min_w_0().truncate().child(place)),
                    )
                    .child(
                        div()
                            .text_size(px(12.))
                            .text_color(p.header_muted)
                            .truncate()
                            .child(text::card_status(&status, &age)),
                    );
            let view = match card {
                home::Card::Document { summary, .. } => {
                    let id = VaultId::parse(&summary.id);
                    view.on_click(cx.listener(move |this, _, window, cx| {
                        if let Some(id) = id.clone() {
                            this.open_vault_document(id, window, cx);
                        }
                    }))
                }
                home::Card::Reply(reply) => {
                    let id = reply.id;
                    view.on_click(
                        cx.listener(move |this, _, window, cx| this.open_reply(&id, window, cx)),
                    )
                }
            };
            row = row.child(view);
        }
        // Fewer cards keep a card's width.
        for _ in shown..home::STRIP_CARDS {
            row = row.child(div().flex_1().flex_basis(px(0.)));
        }
        Some(strip_box.child(row).into_any_element())
    }
}
