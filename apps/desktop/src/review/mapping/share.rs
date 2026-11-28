//! The Inspector's Share tab: everything that takes the
//! pseudonymized document out, and an AI reply box that restores in place
//! with Copy reply and Open as tab.
use super::Selection;
use crate::review::text::{self, review_tab_label, share_status};
use crate::{
    document::{DocumentView, Inspector},
    review::refusal::Step,
    review::{APPLIED_ID, KEPT_ID},
    ui,
    ui::style,
};
use gpui::{AnyElement, App, Context, Focusable, Window, div, prelude::*, px};

impl DocumentView {
    /// Review | Share | Notes, the Inspector's tabs.
    pub(crate) fn inspector_tabs(&self, entities: usize, cx: &mut Context<Self>) -> AnyElement {
        let theme = self.theme.get();
        let p = theme.pdf_style();
        let tab = |id: &'static str, label: String, which: Inspector| {
            let on = self.inspector == which;
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
                    this.inspector = which;
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
                        review_tab_label(entities),
                        Inspector::Review,
                    ))
                    .child(tab("inspector-share", text::SHARE.into(), Inspector::Share))
                    .child(tab("inspector-notes", text::NOTES.into(), Inspector::Notes)),
            )
            .into_any_element()
    }

    pub(super) fn share_panel(&self, window: &Window, cx: &mut Context<Self>) -> AnyElement {
        let theme = self.theme.get();
        let p = theme.pdf_style();
        let aliases = self.has_applied_aliases();
        // A document scanned with nothing to replace may leave too.
        let copy_for_ai = self.copy_for_ai_reason().is_none();
        // What Copy for AI needs first shows on the button, the count with
        // it, before the click.
        let step = self.copy_step(cx);
        let copy_enabled = copy_for_ai || step.is_some();
        let copied = self.copy_feedback.is_some();
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
        let link = self.project.link().cloned();
        // Only a document in the Vault can move into a Project.
        let movable = self.session.document.vault.is_some();
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
                    .child(section(text::SEND_TO_AN_AI))
                    .child(
                        div()
                            .when(cfg!(test), |v| v.debug_selector(|| "share-status".into()))
                            .text_size(px(13.))
                            .child(status),
                    )
                    .child(
                        ui::filled_button(
                            "share-copy-for-ai",
                            self.share_copy_label(step, copied, cx),
                            theme,
                            copy_enabled,
                        )
                        .when(cfg!(test), |v| {
                            v.debug_selector(|| "share-copy-for-ai".into())
                        })
                        .h(px(32.))
                        .on_click(cx.listener(
                            move |this, _, window, cx| {
                                this.share_copy(step, copy_for_ai, window, cx)
                            },
                        )),
                    )
                    .child(note(text::ADDS_A_SHORT_NOTE.into()))
                    // Only checked exits: Copy Markdown and Export Markdown
                    // hand the text out as it is, so they stay in the
                    // header's ⋯ menu, the File menu, their shortcuts and
                    // the palette (Vault & Projects item 29).
                    .when(aliases || self.reply.exists(), |v| {
                        v.children(self.reply_section(cx))
                    })
                    .child(section(text::PROJECT))
                    .child(match &link {
                        Some(link) => ui::control("share-project-page", "", theme, true)
                            .when(cfg!(test), |v| {
                                v.debug_selector(|| "share-project-page".into())
                            })
                            .aria_label(text::open_project(&link.name))
                            .on_click(cx.listener(|_, _, _, cx| {
                                cx.emit(crate::workspace::TabEvent::Project(
                                    crate::workspace::ProjectEvent::OpenPage,
                                ))
                            }))
                            .flex()
                            .items_center()
                            .gap_2()
                            .child(
                                div()
                                    .size(px(8.))
                                    .rounded_full()
                                    .bg(style::project_color(link.hue(), theme)),
                            )
                            .child(div().flex_1().truncate().child(link.name.clone()))
                            .into_any_element(),
                        None if !movable => {
                            note(text::KEPT_WITH_THE_DOCUMENT.into()).into_any_element()
                        }
                        None => note(text::UNFILED_NAMES_KEPT.into()).into_any_element(),
                    })
                    // A row, so the buttons keep their label width like Copy
                    // Markdown instead of stretching into a field. Move to
                    // Project… shows once another Project exists (#1312).
                    .when(movable, |v| {
                        let others = self
                            .projects
                            .get()
                            .saturating_sub(usize::from(link.is_some()));
                        let bounds = self.share_move_bounds.clone();
                        v.child(
                            div()
                                .flex()
                                .flex_wrap()
                                .gap_2()
                                .when(others > 0, |row| {
                                    row.child(
                                        div()
                                            .relative()
                                            .child(
                                                gpui::canvas(
                                                    move |b, _, _| bounds.set(b),
                                                    |_, _, _, _| {},
                                                )
                                                .absolute()
                                                .inset_0(),
                                            )
                                            .child(
                                                ui::action_control(
                                                    "share-move-to-project",
                                                    text::MOVE_TO_PROJECT,
                                                    theme,
                                                    true,
                                                    false,
                                                )
                                                .when(cfg!(test), |v| {
                                                    v.debug_selector(|| {
                                                        "share-move-to-project".into()
                                                    })
                                                })
                                                .rounded_lg()
                                                .on_click(cx.listener(|this, _, window, cx| {
                                                    window.focus(&this.pii.mapping.focus, cx);
                                                    let at =
                                                        this.share_move_bounds.get().bottom_left();
                                                    cx.emit(crate::workspace::TabEvent::Project(
                                                        crate::workspace::ProjectEvent::MoveMenu(
                                                            at,
                                                        ),
                                                    ));
                                                })),
                                            ),
                                    )
                                })
                                .child(
                                    ui::primary_button(
                                        "share-new-project",
                                        text::MOVE_TO_NEW_PROJECT,
                                        theme,
                                        true,
                                    )
                                    .when(cfg!(test), |v| {
                                        v.debug_selector(|| "share-new-project".into())
                                    })
                                    .on_click(cx.listener(
                                        move |this, _, window, cx| {
                                            // The button leaves with the click; the panel
                                            // stays in every view, so shortcuts keep a
                                            // focused target (#79).
                                            window.focus(&this.pii.mapping.focus, cx);
                                            cx.emit(crate::workspace::TabEvent::Project(
                                                crate::workspace::ProjectEvent::MoveToNew,
                                            ));
                                        },
                                    )),
                                ),
                        )
                    }),
            )
            .into_any_element()
    }
}

impl DocumentView {
    /// The Review tab's label as the Share tab shows it, for tests.
    #[cfg(test)]
    pub(in crate::review) fn review_tab_label(&self, cx: &App) -> String {
        review_tab_label(self.replacement_entity_count(cx))
    }

    /// The Share tab's copy button: Copy for AI, or the step it needs
    /// first. "Replace N names and copy" counts names, as the status above
    /// it and the header's Apply do (#1314); the step keeps the places, so
    /// a button drawn for another count still replaces nothing.
    pub(in crate::review) fn share_copy_label(
        &self,
        step: Option<Step>,
        copied: bool,
        cx: &App,
    ) -> String {
        match step {
            Some(Step::ReplaceAndCopy(_)) => {
                text::replace_names_and_copy(self.pending_name_count(cx))
            }
            Some(step) => step.label(),
            None if copied && self.copied_for_ai => crate::ui::text::COPIED.into(),
            None => text::COPY_FOR_AI.into(),
        }
    }

    /// The Share tab's status line. It says the names are aliases only
    /// while readiness finds none left as written (#301).
    pub(in crate::review) fn share_status(&self, cx: &App) -> String {
        let pending = self.pii.review.remaining();
        let applied = self.pii.review.applied().len();
        let left = if pending == 0 && applied > 0 {
            self.names_not_replaced(cx)
        } else {
            0
        };
        let names = if pending > 0 {
            self.pending_name_count(cx)
        } else {
            self.applied_name_count(cx)
        };
        share_status(names, pending, applied, self.kept_mention_count(cx), left)
    }

    /// The "AI reply" box under Copy for AI: pasting restores at
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
                        .child(text::AI_REPLY_HEADING),
                )
                .child(
                    // The editor tracks its own handle; the box only rings it.
                    ui::focus_ring("share-reply-box", theme)
                        .ring_for(&focus)
                        .aria_label(text::AI_REPLY)
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
                        .when(!restorable, |v| {
                            v.child(
                                muted(text::NOTHING_CAN_BE_RESTORED.into()).when(cfg!(test), |v| {
                                    v.debug_selector(|| "share-reply-nothing".into())
                                }),
                            )
                        })
                        .into_any_element()
                } else {
                    muted(text::PASTE_THE_AIS_ANSWER.into()).into_any_element()
                })
                .child(
                    div()
                        .flex()
                        .flex_wrap()
                        .gap_2()
                        .child(
                            ui::primary_button(
                                "share-copy-reply",
                                if copied {
                                    crate::ui::text::COPIED
                                } else {
                                    text::COPY_REPLY
                                },
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
                                text::OPEN_REPLY,
                                theme,
                                restorable,
                            )
                            .when(cfg!(test), |v| {
                                v.debug_selector(|| "share-open-reply".into())
                            })
                            .on_click(cx.listener(
                                move |this, _, _, cx| {
                                    if restorable {
                                        this.open_saved_reply(cx)
                                    }
                                },
                            )),
                        )
                        // Bordered like its neighbors, not a bare text
                        // control beside them (U25).
                        .when(exists && unresolved > 0, |v| {
                            v.child(
                                ui::primary_button(
                                    "share-next-unrecognised",
                                    text::NEXT_UNRECOGNIZED,
                                    theme,
                                    true,
                                )
                                .when(cfg!(test), |v| {
                                    v.debug_selector(|| "share-next-unrecognised".into())
                                })
                                .on_click(cx.listener(
                                    |this, _, window, cx| this.next_unrecognised(window, cx),
                                )),
                            )
                        }),
                )
                // Saved replies: the Project's, or this document's own while
                // Unfiled; each opens on its Reply page.
                .when(!self.saved_replies.is_empty(), |v| {
                    v.child(
                        div()
                            .pt_3()
                            .pb_1()
                            .text_size(px(11.))
                            .font_weight(gpui::FontWeight::SEMIBOLD)
                            .text_color(p.header_muted)
                            .child(text::SAVED_REPLIES_HEADING),
                    )
                    .children(self.saved_replies.iter().enumerate().map(|(index, row)| {
                        let id = row.id.clone();
                        let key = format!("share-saved-reply-{index}");
                        div()
                            .flex()
                            .flex_col()
                            .child(
                                ui::control(
                                    gpui::SharedString::from(key.clone()),
                                    row.title.clone(),
                                    theme,
                                    true,
                                )
                                .when(cfg!(test), move |v| v.debug_selector(move || key.clone()))
                                .truncate()
                                .on_click(cx.listener(
                                    move |_, _, _, cx| {
                                        cx.emit(crate::workspace::TabEvent::Reply(
                                            crate::workspace::ReplyEvent::Open(id.clone()),
                                        ));
                                    },
                                )),
                            )
                            .child(
                                div()
                                    .px_2()
                                    .text_size(px(11.))
                                    .text_color(p.header_muted)
                                    .truncate()
                                    .child(row.detail.clone()),
                            )
                    }))
                })
                .into_any_element(),
        )
    }

    /// Hovering a replacement shows what it is; a click opens the mention
    /// bar.
    pub(crate) fn replacement_tip(&self, window: &Window, cx: &App) -> Option<AnyElement> {
        let annotation = self.editor.read(cx).hovered_annotation()?;
        // While the popup is open a click moves it; tips would only stack
        // over it.
        if self.pii.popup.is_some() {
            return None;
        }
        let review = &self.pii.review;
        let kept = annotation & KEPT_ID != 0;
        let (alias, category, applied) = if kept {
            // A kept original: its entity's name, from the review
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
                                        if applied { text::DOT_APPLIED } else { "" }
                                    ))),
                            )
                            .child(div().opacity(0.7).child(if kept {
                                text::KEPT_CLICK_TO_EDIT
                            } else {
                                text::CLICK_TO_EDIT
                            })),
                    ),
            )
            .with_priority(1)
            .into_any_element(),
        )
    }
}

#[cfg(test)]
mod tests {
    use super::share_status;

    #[test]
    fn the_share_line_says_aliases_only_without_leftovers() {
        assert_eq!(
            share_status(6, 0, 18, 0, 0),
            "6 names replaced. Names in the text are aliases now."
        );
        assert_eq!(
            share_status(1, 0, 2, 0, 1),
            "1 name replaced. 1 name not replaced."
        );
        assert_eq!(
            share_status(6, 0, 18, 2, 3),
            "6 names replaced · 2 kept. 3 names not replaced."
        );
        assert_eq!(
            share_status(3, 4, 18, 0, 1),
            "3 names in 4 places still to replace — apply them first."
        );
        assert_eq!(
            share_status(1, 1, 0, 0, 0),
            "1 name still to replace — apply it first."
        );
        assert_eq!(share_status(0, 0, 0, 0, 0), "Nothing is replaced yet.");
    }
}
