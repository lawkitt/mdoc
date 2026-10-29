//! The AI return journey (ADR 0041, phase 1): Copy for AI adds a placeholder
//! notice; Paste AI Response opens the reply in a new tab with originals
//! restored as proposals that keep their aliases. Unknown alias-shaped tokens
//! are highlighted and can be assigned to an identity, never guessed.
use crate::{
    document_view::DocumentView,
    pii::ui::{CopyForAi, PiiClosePopup},
    ui,
    workspace::{PasteAiResponse, PseudonymizeClipboard},
};
use gpui::{
    AnyElement, App, Context, Focusable, Hsla, Window, anchored, deferred, div, prelude::*, px,
};
use mdoc_pii::restore::{ExportedIdentity, RestoreMap};
use std::{collections::HashSet, ops::Range, sync::Arc};

/// Unresolved-token annotations: bit 62 plus the token's start offset.
pub(super) const UNRESOLVED_ID: u64 = 1 << 62;

/// A copied AI reply on its way to a new tab.
pub(crate) struct RestoredReply {
    pub text: String,
    pub identities: Vec<ExportedIdentity>,
    pub source_name: String,
    /// The source tab's case: the reply joins it (ADR 0041 #16).
    pub case: Option<mdoc_store::cases::CaseLink>,
}

pub(crate) struct RestoreState {
    pub map: RestoreMap,
    pub restored: usize,
    /// Token texts the user chose to leave as they are.
    ignored: HashSet<String>,
    summary_dismissed: bool,
    /// The token whose popup is open, and the highlighted identity row.
    pub(super) popup: Option<(Range<usize>, usize)>,
    scroll: gpui::UniformListScrollHandle,
}

/// Prepended by Copy for AI. It names aliases only, never original values.
pub(crate) fn ai_notice(examples: &[&str]) -> String {
    let examples = match examples {
        [] => "PERSON_1".to_string(),
        [one] => one.to_string(),
        [first, second, ..] => format!("{first} or {second}"),
    };
    format!(
        "Note: names and other identifying details in this document were replaced \
         with placeholders such as {examples}. Keep every placeholder exactly as \
         written, with the same capitals, underscore and number. Do not invent new \
         placeholders or guess the real values; they are restored locally after \
         your reply.\n\n---\n\n"
    )
}

impl DocumentView {
    /// Applied aliases are in the text, so Copy for AI and Paste AI Response apply.
    pub(crate) fn has_applied_aliases(&self) -> bool {
        !self.pii.review.applied().is_empty()
    }
    pub(crate) fn copy_for_ai(&mut self, _: &CopyForAi, _: &mut Window, cx: &mut Context<Self>) {
        if !self.can_copy_markdown() {
            return;
        }
        let mut examples = Vec::new();
        for applied in self.pii.review.applied() {
            let alias = applied.step.after.as_ref();
            if !examples.contains(&alias) {
                examples.push(alias);
            }
            if examples.len() == 2 {
                break;
            }
        }
        let text = self.editor.read(cx).text();
        let copied = if examples.is_empty() {
            text.to_owned()
        } else {
            ai_notice(&examples) + text
        };
        cx.write_to_clipboard(gpui::ClipboardItem::new_string(copied));
        self.copied_for_ai = true;
        self.copied_reply = false;
        self.show_copy_feedback(cx);
    }
    /// Replace the clipboard text with this tab's aliases (ADR 0041 #30):
    /// the case's names, or the tab's own mapping. Exact known names only.
    pub(crate) fn pseudonymize_clipboard(
        &mut self,
        _: &PseudonymizeClipboard,
        _: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let identities = if self.case.link().is_some() {
            self.case.names().to_vec()
        } else {
            self.pii.review.export_identities()
        };
        if identities.is_empty() {
            self.clipboard_notice = Some(
                "This tab has no names to replace yet. Pseudonymize a document or open a case first."
                    .into(),
            );
            cx.emit(crate::workspace::TabEvent::Case(
                crate::workspace::CaseEvent::MappingMissing,
            ));
            cx.notify();
            return;
        }
        let Some(text) = cx.read_from_clipboard().and_then(|item| item.text()) else {
            self.clipboard_notice = Some("The clipboard has no text.".into());
            cx.notify();
            return;
        };
        let (replaced, count) = RestoreMap::new(identities).pseudonymize(&text);
        if count > 0 {
            cx.write_to_clipboard(gpui::ClipboardItem::new_string(replaced));
        }
        let source = self
            .case
            .link()
            .map_or_else(|| "this document".to_string(), |l| format!("“{}”", l.name));
        self.clipboard_notice = Some(match count {
            0 => format!("No names from {source} were found in the clipboard. It is unchanged."),
            n => format!(
                "Replaced {n} {} from {source} in the clipboard. Names mdoc doesn't know yet \
                 were not replaced; check before pasting.",
                if n == 1 { "name" } else { "names" }
            ),
        });
        cx.notify();
    }
    pub(crate) fn paste_ai_response(
        &mut self,
        _: &PasteAiResponse,
        _: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if !self.can_copy_markdown() {
            return;
        }
        let identities = self.pii.review.export_identities();
        if identities.is_empty() {
            self.error = Some(
                "Paste AI Response needs a document whose names were replaced with aliases. \
                 Apply replacements, copy the text for the AI, then paste its reply here."
                    .into(),
            );
            cx.emit(crate::workspace::TabEvent::Case(
                crate::workspace::CaseEvent::MappingMissing,
            ));
            cx.notify();
            return;
        }
        let text = cx
            .read_from_clipboard()
            .and_then(|item| item.text())
            .unwrap_or_default();
        if text.trim().is_empty() {
            self.error = Some("The clipboard has no text. Copy the AI's reply first.".into());
            cx.notify();
            return;
        }
        self.open_restore_tab(text, cx);
    }
    /// A new tab with `text` restored against this tab's mapping: Paste AI
    /// Response, and the reply box's Open as tab (ADR 0078).
    pub(crate) fn open_restore_tab(&mut self, text: String, cx: &mut Context<Self>) {
        let identities = self.pii.review.export_identities();
        let name = self.display_name();
        let stem = name.strip_suffix(".md").unwrap_or(&name).to_string();
        cx.emit(crate::workspace::TabEvent::Tabs(
            crate::workspace::TabsEvent::Restored(Arc::new(RestoredReply {
                text,
                identities,
                source_name: stem,
                case: self.case.link().cloned(),
            })),
        ));
    }
    /// Fill this new tab with `reply`, then restore its aliases as one undo step.
    pub(crate) fn start_restored(
        &mut self,
        reply: &RestoredReply,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.session.name = Some(format!("Restored – {}.md", reply.source_name));
        self.case.attach(reply.case.clone());
        self.lifecycle.keep_blank();
        self.editor
            .update(cx, |editor, cx| editor.set_text(reply.text.clone(), cx));
        let map = RestoreMap::new(reply.identities.clone());
        let restoration = map.restore(&reply.text);
        let ids = self.pii.review.seed(&reply.identities, &reply.text);
        self.pii.reviewing = true;
        self.pii.restore = Some(RestoreState {
            map,
            restored: restoration.restored,
            ignored: HashSet::new(),
            summary_dismissed: false,
            popup: None,
            scroll: gpui::UniformListScrollHandle::new(),
        });
        if !restoration.edits.is_empty() {
            let revision = self.editor.read(cx).revision();
            let committed = self.editor.update(cx, |editor, cx| {
                let committed = editor.replace_ranges(revision, &restoration.edits, cx);
                editor.set_cursor(0, cx);
                committed
            });
            if committed {
                let transaction = self.editor.read(cx).last_transaction().cloned().unwrap();
                self.pii_transaction(&transaction, cx);
                // Pin each restored wording to the identity whose alias it
                // replaced: homonyms share a wording but not an identity.
                let history = self.editor.read(cx).history_id();
                for (range, source) in restoration.restored_ranges() {
                    self.pii
                        .review
                        .commit_assignment(history, range, ids[source]);
                }
            }
        }
        self.review_changed(cx);
        window.focus(&self.editor.read(cx).focus_handle(cx), cx);
        cx.notify();
    }
    /// Unresolved tokens in the current text, minus those left as they are.
    pub(crate) fn unresolved_tokens(&self, cx: &App) -> Vec<Range<usize>> {
        let Some(state) = &self.pii.restore else {
            return Vec::new();
        };
        let text = self.editor.read(cx).text();
        state
            .map
            .unresolved(text)
            .into_iter()
            .filter(|range| !state.ignored.contains(&text[range.clone()]))
            .collect()
    }
    pub(super) fn unresolved_annotations(&self, cx: &App) -> Vec<mdoc_editor::SourceAnnotation> {
        let red = crate::style::markdown_style(self.theme.get()).alert_caution;
        self.unresolved_tokens(cx)
            .into_iter()
            .map(|range| mdoc_editor::SourceAnnotation {
                id: UNRESOLVED_ID | range.start as u64,
                range,
                color: Hsla { a: 0.14, ..red },
                active_color: Hsla { a: 0.3, ..red },
                border: Hsla { a: 0.6, ..red },
                text_color: Some(red),
                underline: None,
                dotted: false,
            })
            .collect()
    }
    /// Open the popup for the unresolved token starting at `start`.
    pub(super) fn activate_unresolved(
        &mut self,
        start: usize,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let Some(range) = self
            .unresolved_tokens(cx)
            .into_iter()
            .find(|r| r.start == start)
        else {
            return;
        };
        let Some(state) = &mut self.pii.restore else {
            return;
        };
        state.popup = Some((range, 0));
        self.pii.dismiss_popup();
        self.editor.update(cx, |e, cx| {
            e.set_active_annotation(Some(UNRESOLVED_ID | start as u64), cx)
        });
        self.pii.popup_previous = window.focused(cx);
        window.focus(&self.pii.focus, cx);
        cx.notify();
    }
    fn close_unresolved(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if let Some(state) = &mut self.pii.restore {
            state.popup = None;
        }
        self.editor
            .update(cx, |e, cx| e.set_active_annotation(None, cx));
        match self.pii.popup_previous.take() {
            Some(focus) => window.focus(&focus, cx),
            None => window.focus(&self.editor.read(cx).focus_handle(cx), cx),
        }
        cx.notify();
    }
    /// Replace the open token with the primary wording of the identity in row
    /// `index`; discovery then proposes it with that identity's alias.
    pub(super) fn assign_unresolved(
        &mut self,
        index: usize,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let text = self.editor.read(cx).text();
        let Some((range, wording)) = self.pii.restore.as_ref().and_then(|state| {
            let (range, _) = state.popup.clone()?;
            let choice = *state.map.choices(text.get(range.clone())?).get(index)?;
            let identity = state.map.identities().get(choice)?;
            Some((range, identity.wording().to_string()))
        }) else {
            return;
        };
        if !self.unresolved_tokens(cx).contains(&range) {
            self.error = Some("The placeholder changed. Select it again.".into());
            self.close_unresolved(window, cx);
            return;
        }
        let revision = self.editor.read(cx).revision();
        self.editor.update(cx, |editor, cx| {
            editor.replace_ranges(revision, &[(range, wording)], cx)
        });
        self.close_unresolved(window, cx);
    }
    pub(super) fn leave_unresolved(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let text = self.editor.read(cx).text().to_owned();
        if let Some(state) = &mut self.pii.restore
            && let Some((range, _)) = &state.popup
            && let Some(token) = text.get(range.clone())
        {
            state.ignored.insert(token.to_string());
        }
        self.close_unresolved(window, cx);
        self.sync_annotations(cx);
    }
    fn step_unresolved_choice(&mut self, backwards: bool, cx: &mut Context<Self>) {
        let Some(state) = &mut self.pii.restore else {
            return;
        };
        let count = state.map.identities().len();
        let Some((_, selected)) = &mut state.popup else {
            return;
        };
        if count == 0 {
            return;
        }
        *selected = if backwards {
            selected.checked_sub(1).unwrap_or(count - 1)
        } else {
            (*selected + 1) % count
        };
        state
            .scroll
            .scroll_to_item(*selected, gpui::ScrollStrategy::Nearest);
        cx.notify();
    }
    /// Move to the next unresolved token after the caret and open its popup.
    fn next_unresolved(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let cursor = self.editor.read(cx).cursor();
        let tokens = self.unresolved_tokens(cx);
        let Some(range) = tokens
            .iter()
            .find(|r| r.start > cursor)
            .or(tokens.first())
            .cloned()
        else {
            return;
        };
        self.editor
            .update(cx, |editor, cx| editor.set_cursor(range.start, cx));
        if let Some(top) = self.editor.read(cx).offset_screen_top(range.start) {
            let viewport = self.scroll.bounds();
            let delta = top - viewport.top() - px(24.);
            let mut scroll = self.scroll.offset();
            scroll.y = (scroll.y - delta).clamp(-self.scroll.max_offset().y, px(0.));
            self.scroll.set_offset(scroll);
        }
        self.activate_unresolved(range.start, window, cx);
    }

    /// "AI reply restored" and what the restored names are, above a restored reply.
    pub(crate) fn restore_summary(&self, cx: &mut Context<Self>) -> Option<AnyElement> {
        let state = self.pii.restore.as_ref()?;
        if state.summary_dismissed {
            return None;
        }
        let theme = self.theme.get();
        let palette = theme.pdf_style();
        let unresolved = self.unresolved_tokens(cx).len();
        let (headline, detail) = summary_text(state.restored, unresolved);
        let accent = if unresolved > 0 {
            crate::style::markdown_style(theme).alert_caution
        } else {
            theme.applied()
        };
        Some(
            div()
                .id("restore-summary")
                .when(cfg!(test), |v| {
                    v.debug_selector(|| "restore-summary".into())
                })
                .flex()
                .items_center()
                .gap_2()
                .flex_shrink_0()
                .px_3()
                .py_1()
                .text_size(px(12.))
                .bg(Hsla { a: 0.07, ..accent })
                .border_b_1()
                .border_color(palette.border)
                .child(div().text_color(accent).child("ⓘ"))
                .child(
                    div()
                        .flex_shrink_0()
                        .text_color(palette.header_fg)
                        .child(headline),
                )
                .child(
                    div()
                        .flex_1()
                        .min_w_0()
                        .text_color(palette.header_muted)
                        .child(detail),
                )
                .when(unresolved > 0, |v| {
                    v.child(
                        ui::control("next-unresolved", "Next unrecognized", theme, true)
                            .when(cfg!(test), |v| {
                                v.debug_selector(|| "next-unresolved".into())
                            })
                            .on_click(
                                cx.listener(|this, _, window, cx| this.next_unresolved(window, cx)),
                            ),
                    )
                })
                .child(
                    ui::control("dismiss-restore-summary", "×", theme, true)
                        .aria_label("Dismiss notice")
                        .on_click(cx.listener(|this, _, _, cx| {
                            if let Some(state) = &mut this.pii.restore {
                                state.summary_dismissed = true;
                            }
                            cx.notify();
                        })),
                )
                .into_any_element(),
        )
    }

    /// Pick the identity an unrecognized placeholder stands for, or leave it.
    pub(crate) fn unresolved_popup(&self, cx: &mut Context<Self>) -> Option<AnyElement> {
        let state = self.pii.restore.as_ref()?;
        let (range, selected) = state.popup.clone()?;
        let theme = self.theme.get();
        let palette = theme.pdf_style();
        let token = self.editor.read(cx).text().get(range.clone())?.to_string();
        let position = self
            .editor
            .read(cx)
            .annotation_bounds(UNRESOLVED_ID | range.start as u64)
            .map(|b| gpui::point(b.left(), b.bottom() + px(4.)))
            .unwrap_or_else(|| self.scroll.bounds().origin + gpui::point(px(24.), px(24.)));
        let rows: Vec<(String, String)> = state
            .map
            .choices(&token)
            .into_iter()
            .map(|i| &state.map.identities()[i])
            .map(|i| (i.alias.clone(), i.wording().to_string()))
            .collect();
        let count = rows.len();
        let rows = Arc::new(rows);
        Some(
            deferred(
                anchored().position(position).snap_to_window().child(
                    ui::panel("unresolved-popup", theme)
                        .when(cfg!(test), |v| {
                            v.debug_selector(|| "unresolved-popup".into())
                        })
                        .key_context("PiiChooser PseudonymReview UiPanel")
                        .tab_group()
                        .tab_stop(false)
                        .track_focus(&self.pii.focus)
                        .w(px(340.))
                        .max_w_full()
                        .p_2()
                        .flex()
                        .flex_col()
                        .gap_2()
                        .on_action(
                            cx.listener(|this, _: &crate::pii::ui::PiiNextChoice, _, cx| {
                                this.step_unresolved_choice(false, cx)
                            }),
                        )
                        .on_action(cx.listener(
                            |this, _: &crate::pii::ui::PiiPreviousChoice, _, cx| {
                                this.step_unresolved_choice(true, cx)
                            },
                        ))
                        .on_action(cx.listener(
                            |this, _: &crate::pii::ui::PiiOpenChoice, window, cx| {
                                let selected = this
                                    .pii
                                    .restore
                                    .as_ref()
                                    .and_then(|s| s.popup.as_ref())
                                    .map(|(_, s)| *s);
                                if let Some(selected) = selected {
                                    this.assign_unresolved(selected, window, cx);
                                }
                            },
                        ))
                        .on_action(cx.listener(|this, _: &PiiClosePopup, window, cx| {
                            this.close_unresolved(window, cx)
                        }))
                        .on_mouse_down_out(
                            cx.listener(|this, _, window, cx| this.close_unresolved(window, cx)),
                        )
                        .child(
                            div()
                                .flex()
                                .flex_col()
                                .gap_1()
                                .child(
                                    div()
                                        .text_color(palette.header_fg)
                                        .child(format!("Unrecognized placeholder {token}")),
                                )
                                .child(
                                    div()
                                        .text_size(px(12.))
                                        .text_color(palette.header_muted)
                                        .child(
                                            "The original document has no such alias. \
                                             Choose who it stands for, or leave it.",
                                        ),
                                ),
                        )
                        .child(
                            gpui::uniform_list(
                                "unresolved-identities",
                                count,
                                cx.processor(move |this, visible: Range<usize>, _, cx| {
                                    let theme = this.theme.get();
                                    let palette = theme.pdf_style();
                                    visible
                                        .map(|index| {
                                            let (alias, wording) = &rows[index];
                                            ui::control(
                                                gpui::SharedString::from(format!(
                                                    "unresolved-identity-{index}"
                                                )),
                                                "",
                                                theme,
                                                true,
                                            )
                                            .when(cfg!(test), move |v| {
                                                v.debug_selector(move || {
                                                    format!("unresolved-identity-{index}")
                                                })
                                            })
                                            .h(px(30.))
                                            .w_full()
                                            .flex()
                                            .items_center()
                                            .gap_2()
                                            .when(index == selected, |v| {
                                                v.bg(palette.placeholder_bg)
                                            })
                                            .child(
                                                div()
                                                    .flex_1()
                                                    .min_w_0()
                                                    .truncate()
                                                    .child(wording.clone()),
                                            )
                                            .child(
                                                div()
                                                    .flex_shrink_0()
                                                    .text_size(px(12.))
                                                    .text_color(palette.header_muted)
                                                    .child(alias.clone()),
                                            )
                                            .on_click(
                                                cx.listener(move |this, _, window, cx| {
                                                    this.assign_unresolved(index, window, cx)
                                                }),
                                            )
                                        })
                                        .collect()
                                }),
                            )
                            .h(px((count.min(6) as f32 * 30.).max(30.)))
                            .track_scroll(&state.scroll),
                        )
                        .child(
                            div().flex().justify_end().child(
                                ui::control("leave-unresolved", "Leave as is", theme, true)
                                    .when(cfg!(test), |v| {
                                        v.debug_selector(|| "leave-unresolved".into())
                                    })
                                    .on_click(cx.listener(|this, _, window, cx| {
                                        this.leave_unresolved(window, cx)
                                    })),
                            ),
                        ),
                ),
            )
            .with_priority(2)
            .into_any_element(),
        )
    }
}

/// The restore bar's headline and detail: the first sentence says the restored
/// names are proposals and what Apply does. A reply without any placeholder
/// says so instead (ADR 0046).
fn summary_text(restored: usize, unresolved: usize) -> (&'static str, String) {
    let plural = |n: usize, one: &'static str, many: &'static str| if n == 1 { one } else { many };
    if restored == 0 && unresolved == 0 {
        return (
            "Nothing to restore",
            "No placeholders found in this reply.".into(),
        );
    }
    let mut detail = if restored == 0 {
        "No known placeholders were restored.".to_string()
    } else if restored == 1 {
        "The restored name is a proposal. Apply turns it back into an alias \
         before you send the text to the AI again."
            .to_string()
    } else {
        format!(
            "The {restored} restored names are proposals. Apply turns them back into \
             aliases before you send the text to the AI again."
        )
    };
    if unresolved > 0 {
        detail = format!(
            "{detail} {unresolved} unrecognized {} highlighted in red.",
            plural(unresolved, "placeholder is", "placeholders are"),
        );
    }
    ("AI reply restored", detail)
}

#[cfg(test)]
mod summary_tests {
    use super::summary_text;

    #[test]
    fn a_reply_without_placeholders_says_so() {
        assert_eq!(
            summary_text(0, 0),
            (
                "Nothing to restore",
                "No placeholders found in this reply.".into()
            )
        );
        let (_, detail) = summary_text(0, 2);
        assert_eq!(
            detail,
            "No known placeholders were restored. 2 unrecognized placeholders are highlighted in red."
        );
    }

    #[test]
    fn the_first_sentence_says_names_are_proposals_and_what_apply_does() {
        let (headline, detail) = summary_text(3, 0);
        assert_eq!(headline, "AI reply restored");
        assert_eq!(
            detail,
            "The 3 restored names are proposals. Apply turns them back into aliases \
             before you send the text to the AI again."
        );
        let (_, detail) = summary_text(1, 2);
        assert_eq!(
            detail,
            "The restored name is a proposal. Apply turns it back into an alias \
             before you send the text to the AI again. \
             2 unrecognized placeholders are highlighted in red."
        );
    }
}
