//! The Inspector's Share tab (ADR 0048 variant C): everything that takes the
//! pseudonymized document out, and an AI reply box that restores in place
//! with Copy reply and Open as tab (ADR 0078).
use super::Selection;
use crate::{
    document_view::{CopyMarkdown, DocumentView, SaveAs},
    pii::ui::{APPLIED_ID, CopyForAi, KEPT_ID},
    style, ui,
};
use gpui::{AnyElement, App, Context, Focusable, Window, div, prelude::*, px};

impl DocumentView {
    /// Review | Share, the Inspector's two tabs.
    pub(super) fn inspector_tabs(&self, entities: usize, cx: &mut Context<Self>) -> AnyElement {
        let theme = self.theme.get();
        let p = theme.pdf_style();
        let tab = |id: &'static str, label: String, share: bool| {
            let on = self.inspector_share == share;
            ui::focus_ring(id, theme)
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
        let applied = self.pii.review.applied().len();
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
        let status = self.share_status(cx);
        let case = self.case.link().cloned();
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
                                ui::primary_button("share-copy-markdown", if copied && !self.copied_for_ai && !self.copied_reply { "Copied" } else { "Copy Markdown" }, theme, true)
                                    .when(cfg!(test), |v| v.debug_selector(|| "share-copy-markdown".into()))
                                    .on_click(cx.listener(|this, _, window, cx| this.copy_markdown(&CopyMarkdown, window, cx))),
                            )
                            .child(
                                ui::primary_button("share-save", "Save…", theme, true)
                                    .on_click(|_, window, cx| window.dispatch_action(Box::new(SaveAs), cx)),
                            ),
                    )
                    .when(aliases || self.reply.exists(), |v| v.children(self.reply_section(cx)))
                    .child(section("Case"))
                    .child(match case {
                        Some(link) => ui::control("share-case-page", "", theme, true)
                            .when(cfg!(test), |v| v.debug_selector(|| "share-case-page".into()))
                            .aria_label(format!("Open case {}", link.name))
                            .on_click(cx.listener(|_, _, _, cx| cx.emit(crate::workspace::TabEvent::Case(crate::workspace::CaseEvent::OpenPage))))
                            .flex()
                            .items_center()
                            .gap_2()
                            .child(div().size(px(8.)).rounded_full().bg(style::case_color(link.hue(), theme)))
                            .child(div().flex_1().truncate().child(link.name.clone()))
                            .child(note("saved".into()))
                            .into_any_element(),
                        None => div()
                            .flex()
                            .flex_col()
                            .gap_2()
                            .child(note("Save which original each alias stands for, encrypted on this computer, to restore replies later.".into()))
                            // A row, so the button keeps its label width like
                            // Copy Markdown instead of stretching into a field.
                            .child(
                                div().flex().child(
                                    ui::primary_button("share-save-case", "Save as case", theme, applied > 0)
                                        .when(cfg!(test), |v| v.debug_selector(|| "share-save-case".into()))
                                        .on_click(cx.listener(move |this, _, window, cx| {
                                            if applied > 0 {
                                                // The button leaves with the click; the panel
                                                // stays in every view, so shortcuts keep a
                                                // focused target (#79).
                                                window.focus(&this.pii.mapping.focus, cx);
                                                cx.emit(crate::workspace::TabEvent::Case(crate::workspace::CaseEvent::Create { close: false }));
                                            }
                                        })),
                                ),
                            )
                            .into_any_element(),
                    }),
            )
            .into_any_element()
    }
}

impl DocumentView {
    /// The Share tab's status line. It says the names are aliases only
    /// while readiness finds none left as written (#301).
    pub(in crate::pii::ui) fn share_status(&self, cx: &App) -> String {
        let pending = self.pii.review.remaining();
        let applied = self.pii.review.applied().len();
        let left = if pending == 0 && applied > 0 {
            self.names_not_replaced(cx)
        } else {
            0
        };
        share_status(pending, applied, self.kept_mention_count(cx), left)
    }

    /// The "AI reply" box under Copy for AI (ADR 0078): pasting restores at
    /// once into the Reply view; the summary counts what came back.
    fn reply_section(&self, cx: &mut Context<Self>) -> Option<AnyElement> {
        let editor = self.reply_box()?.clone();
        let theme = self.theme.get();
        let p = theme.pdf_style();
        let focus = editor.read(cx).focus_handle(cx);
        let exists = self.reply.exists();
        let restorable = self.reply.restorable();
        let unresolved = self.reply.restored().unresolved.len();
        let copied = self.copy_feedback.is_some() && self.copied_reply;
        let muted = |text: String| {
            div()
                .text_size(px(12.))
                .text_color(p.header_muted)
                .child(text)
        };
        Some(
            div()
                .flex()
                .flex_col()
                .gap_2()
                .child(
                    div()
                        .pt_3()
                        .pb_1()
                        .text_size(px(11.))
                        .font_weight(gpui::FontWeight::SEMIBOLD)
                        .text_color(p.header_muted)
                        .child("AI REPLY"),
                )
                .child(
                    // The editor tracks its own handle; the box only rings it.
                    ui::focus_ring("share-reply-box", theme)
                        .ring_for(&focus)
                        .aria_label("AI reply")
                        .when(cfg!(test), |v| {
                            v.debug_selector(|| "share-reply-box".into())
                        })
                        .key_context("ReplyBox")
                        .min_h(px(72.))
                        .max_h(px(180.))
                        .overflow_y_scroll()
                        .track_scroll(&self.reply_scroll)
                        .px_2()
                        .py_1()
                        .rounded_md()
                        .border_1()
                        .border_color(p.border)
                        .bg(p.bg)
                        .text_size(px(13.))
                        .cursor_text()
                        .on_click(
                            cx.listener(|this, _, window, cx| this.focus_reply_box(window, cx)),
                        )
                        .child(editor),
                )
                .child(if exists {
                    div()
                        .flex()
                        .flex_col()
                        .items_start()
                        .gap_1()
                        .child(muted(self.reply.summary()).when(cfg!(test), |v| {
                            v.debug_selector(|| "share-reply-summary".into())
                        }))
                        .when(unresolved > 0, |v| {
                            v.child(
                                ui::control(
                                    "share-next-unrecognised",
                                    "Next unrecognized",
                                    theme,
                                    true,
                                )
                                .when(cfg!(test), |v| {
                                    v.debug_selector(|| "share-next-unrecognised".into())
                                })
                                .text_size(px(12.))
                                .on_click(cx.listener(
                                    |this, _, window, cx| this.next_unrecognised(window, cx),
                                )),
                            )
                        })
                        .when(!restorable, |v| {
                            v.child(
                                muted("No aliases are applied, so nothing can be restored.".into())
                                    .when(cfg!(test), |v| {
                                        v.debug_selector(|| "share-reply-nothing".into())
                                    }),
                            )
                        })
                        .into_any_element()
                } else {
                    muted("Paste the AI's answer: the real names come back here at once.".into())
                        .into_any_element()
                })
                .child(
                    div()
                        .flex()
                        .flex_wrap()
                        .gap_2()
                        .child(
                            ui::primary_button(
                                "share-copy-reply",
                                if copied { "Copied" } else { "Copy reply" },
                                theme,
                                restorable,
                            )
                            .when(cfg!(test), |v| {
                                v.debug_selector(|| "share-copy-reply".into())
                            })
                            .on_click(cx.listener(
                                move |this, _, _, cx| {
                                    if restorable {
                                        this.copy_reply(cx)
                                    }
                                },
                            )),
                        )
                        .child(
                            ui::primary_button(
                                "share-open-reply",
                                "Open as tab",
                                theme,
                                restorable,
                            )
                            .when(cfg!(test), |v| {
                                v.debug_selector(|| "share-open-reply".into())
                            })
                            .on_click(cx.listener(
                                move |this, _, _, cx| {
                                    if restorable {
                                        this.open_reply_as_tab(cx)
                                    }
                                },
                            )),
                        ),
                )
                .into_any_element(),
        )
    }

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

/// The Share status for `pending` proposals, `applied` replacements, `kept`
/// mentions and `left` known names still in the text as written.
fn share_status(pending: usize, applied: usize, kept: usize, left: usize) -> String {
    if pending > 0 {
        return format!("{pending} still to replace — apply them first.");
    }
    if applied == 0 {
        return "Nothing is replaced yet.".to_string();
    }
    let kept = if kept > 0 {
        format!(" · {kept} kept")
    } else {
        String::new()
    };
    match left {
        0 => format!("{applied} replaced{kept}. Names in the text are aliases now."),
        1 => format!("{applied} replaced{kept}. 1 name not replaced."),
        n => format!("{applied} replaced{kept}. {n} names not replaced."),
    }
}

#[cfg(test)]
mod tests {
    use super::share_status;

    #[test]
    fn the_share_line_says_aliases_only_without_leftovers() {
        assert_eq!(
            share_status(0, 18, 0, 0),
            "18 replaced. Names in the text are aliases now."
        );
        assert_eq!(
            share_status(0, 18, 0, 1),
            "18 replaced. 1 name not replaced."
        );
        assert_eq!(
            share_status(0, 18, 2, 3),
            "18 replaced · 2 kept. 3 names not replaced."
        );
        assert_eq!(
            share_status(4, 18, 0, 1),
            "4 still to replace — apply them first."
        );
        assert_eq!(share_status(0, 0, 0, 0), "Nothing is replaced yet.");
    }
}
