//! The Inspector's Share tab (ADR 0048 variant C): everything that takes the
//! pseudonymized document out, and brings the AI's reply back.
use super::*;
use crate::ui;
use crate::{CopyForAi, CopyMarkdown, PasteAiResponse, SaveAs};
use gpui::{AnyElement, App, Window, div, prelude::*};

impl DocumentView {
    /// Review | Share, the Inspector's two tabs.
    pub(super) fn inspector_tabs(&self, entities: usize, cx: &mut Context<Self>) -> AnyElement {
        let theme = self.theme.get();
        let p = theme.pdf_style();
        let tab = |id: &'static str, label: String, share: bool| {
            let on = self.inspector_share == share;
            div()
                .id(id)
                .when(cfg!(test), move |v| v.debug_selector(move || id.into()))
                .role(gpui::Role::Tab)
                .aria_label(label.clone())
                .key_context("UiControl")
                .tab_index(0)
                .flex_1()
                .flex()
                .justify_center()
                .py(px(5.))
                .rounded_md()
                .text_size(px(12.))
                .cursor_pointer()
                .when(on, |v| {
                    v.bg(p.bg)
                        .shadow_sm()
                        .text_color(p.header_fg)
                        .font_weight(gpui::FontWeight::MEDIUM)
                })
                .when(!on, |v| v.text_color(p.header_muted))
                .child(label)
                .on_click(cx.listener(move |this, _, window, cx| {
                    this.inspector_share = share;
                    window.focus(&this.pii.mapping.focus, cx);
                    cx.notify();
                }))
        };
        div()
            .flex_shrink_0()
            .px_3()
            .pt_3()
            .pb_2()
            .child(
                div()
                    .flex()
                    .p(px(3.))
                    .gap(px(2.))
                    .rounded_lg()
                    .bg(p.placeholder_bg)
                    .child(tab(
                        "inspector-review",
                        format!("Review · {entities}"),
                        false,
                    ))
                    .child(tab("inspector-share", "Share".into(), true)),
            )
            .into_any_element()
    }

    pub(super) fn share_panel(&self, window: &Window, cx: &mut Context<Self>) -> AnyElement {
        let theme = self.theme.get();
        let p = theme.pdf_style();
        let aliases = self.has_applied_aliases();
        let copied = self.copy_feedback.is_some();
        let pending = self.pii.review.remaining();
        let applied = self.pii.review.applied().len();
        let kept = self.kept_mention_count(cx);
        let entities = self.replacement_entity_count(cx);
        let section = |title: &'static str| {
            div()
                .pt_3()
                .pb_1()
                .text_size(px(11.))
                .font_weight(gpui::FontWeight::SEMIBOLD)
                .text_color(p.header_muted)
                .child(title.to_uppercase())
        };
        let note = |text: String| {
            div()
                .text_size(px(12.))
                .text_color(p.header_muted)
                .child(text)
        };
        let status = if pending > 0 {
            format!("{pending} still to replace — apply them first.")
        } else if applied > 0 {
            format!(
                "{applied} replaced{}. Names in the text are aliases now.",
                if kept > 0 {
                    format!(" · {kept} kept")
                } else {
                    String::new()
                }
            )
        } else {
            "Nothing is replaced yet.".to_string()
        };
        let case = self.case.clone();
        ui::panel("identity-panel", theme)
            .when(cfg!(test), |v| v.debug_selector(|| "identity-panel".into()))
            .m_2()
            .rounded_lg()
            .shadow_none()
            .overflow_hidden()
            .key_context("IdentityPanel UiPanel UiMenu")
            .tab_group()
            .tab_stop(false)
            .track_focus(&self.pii.mapping.focus)
            .on_action(cx.listener(|this, _: &ui::CloseMenu, window, cx| {
                this.close_replacements(window, cx);
                cx.stop_propagation();
            }))
            .flex_shrink_0()
            .min_h_0()
            .flex()
            .flex_col()
            .text_size(px(13.))
            .w(self.replacements_panel_width(window) - px(16.))
            .child(self.inspector_tabs(entities, cx))
            .child(div().h(px(1.)).flex_shrink_0().bg(p.border))
            .child(
                div()
                    .id("share-body")
                    .flex_1()
                    .min_h_0()
                    .overflow_y_scroll()
                    .px_3()
                    .pb_3()
                    .flex()
                    .flex_col()
                    .gap_2()
                    .child(section("Send to an AI"))
                    .child(div().when(cfg!(test), |v| v.debug_selector(|| "share-status".into())).text_size(px(13.)).child(status))
                    .child(
                        ui::filled_button(
                            "share-copy-for-ai",
                            if copied && self.copied_for_ai { "Copied" } else { "Copy for AI" },
                            theme,
                            aliases,
                        )
                        .when(cfg!(test), |v| v.debug_selector(|| "share-copy-for-ai".into()))
                        .h(px(32.))
                        .on_click(cx.listener(move |this, _, window, cx| {
                            if aliases {
                                this.copy_for_ai(&CopyForAi, window, cx)
                            }
                        })),
                    )
                    .child(note("Adds a short note asking the AI to keep every placeholder exactly.".into()))
                    .child(
                        div()
                            .flex()
                            .flex_wrap()
                            .gap_2()
                            .child(
                                ui::primary_button("share-copy-markdown", if copied && !self.copied_for_ai { "Copied" } else { "Copy Markdown" }, theme, true)
                                    .when(cfg!(test), |v| v.debug_selector(|| "share-copy-markdown".into()))
                                    .on_click(cx.listener(|this, _, window, cx| this.copy_markdown(&CopyMarkdown, window, cx))),
                            )
                            .child(
                                ui::primary_button("share-save", "Save…", theme, true)
                                    .on_click(|_, window, cx| window.dispatch_action(Box::new(SaveAs), cx)),
                            ),
                    )
                    .child(section("Bring the reply back"))
                    .child(note("Copy the AI's answer, then paste it here: a new tab shows it with the real names restored.".into()))
                    .child(
                        ui::primary_button("share-paste-reply", "Paste AI reply", theme, aliases)
                            .when(cfg!(test), |v| v.debug_selector(|| "share-paste-reply".into()))
                            .on_click(move |_, window, cx| {
                                if aliases {
                                    window.dispatch_action(Box::new(PasteAiResponse), cx)
                                }
                            }),
                    )
                    .child(section("Case"))
                    .child(match case {
                        Some(link) => div()
                            .flex()
                            .items_center()
                            .gap_2()
                            .child(div().size(px(8.)).rounded_full().bg(link.color()))
                            .child(div().flex_1().truncate().child(link.name.clone()))
                            .child(note("saved".into()))
                            .into_any_element(),
                        None => div()
                            .flex()
                            .flex_col()
                            .gap_2()
                            .child(note("Save which original each alias stands for, encrypted on this computer, to restore replies later.".into()))
                            .child(
                                ui::primary_button("share-save-case", "Save as case", theme, applied > 0)
                                    .when(cfg!(test), |v| v.debug_selector(|| "share-save-case".into()))
                                    .on_click(cx.listener(move |_, _, _, cx| {
                                        if applied > 0 {
                                            cx.emit(crate::workspace::TabEvent::CreateCase);
                                        }
                                    })),
                            )
                            .into_any_element(),
                    }),
            )
            .into_any_element()
    }
}

impl DocumentView {
    /// Hovering a replacement shows what it is; a click opens the mention
    /// bar (ADR 0048).
    pub(crate) fn replacement_tip(&self, window: &Window, cx: &App) -> Option<AnyElement> {
        let annotation = self.editor.read(cx).hovered_annotation()?;
        // While the popup is open a click moves it; tips would only stack
        // over it (ADR 0055).
        if self.pii.popup.is_some() {
            return None;
        }
        let review = &self.pii.review;
        let kept = annotation & KEPT_ID != 0;
        let (alias, category, applied) = if kept {
            // A kept original (ADR 0055): its entity's name, from the review
            // or the record taken when it was unticked.
            let Some(Selection::Kept {
                identity, original, ..
            }) = self.kept_selection((annotation & !KEPT_ID) as usize)
            else {
                return None;
            };
            match review.identity(identity) {
                Some(i) => (i.alias.clone(), i.category, false),
                None => {
                    let name = self.pii.mapping.kept_names.get(&original)?;
                    (name.alias.clone(), name.category, false)
                }
            }
        } else {
            let (identity, applied) = if annotation & APPLIED_ID != 0 {
                (
                    review
                        .applied_occurrence(annotation & !APPLIED_ID)?
                        .step
                        .identity,
                    true,
                )
            } else {
                let candidate = review.candidate(annotation)?;
                (
                    review.occurrence_identity(candidate.variant, &candidate.range)?,
                    false,
                )
            };
            let identity = review.identity(identity)?;
            (identity.alias.clone(), identity.category, applied)
        };
        let bounds = self.editor.read(cx).annotation_bounds(annotation)?;
        // The editor keeps its last hover when the pointer leaves it.
        if !bounds.contains(&window.mouse_position()) {
            return None;
        }
        let theme = self.theme.get();
        let tip = gpui::hsla(0., 0., 0.18, 0.96);
        Some(
            gpui::deferred(
                gpui::anchored()
                    .position(gpui::point(bounds.left(), bounds.top() - px(6.)))
                    .anchor(gpui::Anchor::BottomLeft)
                    .snap_to_window_with_margin(px(8.))
                    .child(
                        div()
                            .when(cfg!(test), |v| {
                                v.debug_selector(|| "replacement-tip".into())
                            })
                            .px_2()
                            .py_1()
                            .rounded_md()
                            .bg(tip)
                            .text_color(gpui::hsla(0., 0., 0.96, 1.))
                            .text_size(px(12.))
                            .flex()
                            .flex_col()
                            .child(
                                div()
                                    .flex()
                                    .items_center()
                                    .gap(px(6.))
                                    .child(
                                        div()
                                            .size(px(7.))
                                            .rounded_full()
                                            .bg(theme.category(category)),
                                    )
                                    .child(div().font_weight(gpui::FontWeight::MEDIUM).child(alias))
                                    .child(div().opacity(0.7).child(format!(
                                        "· {}{}",
                                        category.label(),
                                        if applied { " · applied" } else { "" }
                                    ))),
                            )
                            .child(div().opacity(0.7).child(if kept {
                                "Kept · click to edit"
                            } else {
                                "Click to edit"
                            })),
                    ),
            )
            .with_priority(1)
            .into_any_element(),
        )
    }
}
