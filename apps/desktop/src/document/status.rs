//! What a document says about its own state: the conversion card in the
//! Markdown pane, the preview pane's status when it has no page, and the
//! notices above the text.
use super::{DocumentView, ExtractNative, OcrState, RunOcr, view::button};
use crate::document::text;
use crate::{
    document::preview::RetryPreview,
    ui,
    ui::style::{self, Theme},
    workspace,
};
use gpui::{Context, Window, div, prelude::*, px};
use mdoc_import::{ocr, recognition};
use mdoc_store::settings;

impl DocumentView {
    /// The Markdown pane before conversion: one card that changes in place
    /// through conversion, OCR consent, setup and recognition.
    pub(super) fn conversion_card(
        &mut self,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> gpui::AnyElement {
        let theme = self.theme.get();
        let palette = theme.pdf_style();
        let choice = self.recognition_choice();
        let (ocr_model, unreadable) = mdoc_import::ocr::plan(&choice.languages);
        let model = Some(settings::Model::Ocr(ocr_model));
        let shared = self.settings_ocr_model() == Some(ocr_model);
        let ready = self.installed_for(ocr_model, cx).is_some();
        let panel = self.model_panel.read(cx);
        let progress = model.and_then(|m| panel.progress_of(m));
        let cancelling = panel.cancelling();
        let pending = model
            .map(|m| panel.pending[crate::settings::Panel::index(m)])
            .unwrap_or_default();
        let installing =
            progress.is_some() || (shared && matches!(self.ocr_state, OcrState::Installing));
        let card = ui::state_card("conversion-card", theme)
            .when(cfg!(test), |v| {
                v.debug_selector(|| "conversion-card".into())
            })
            .child(
                div()
                    .text_size(px(12.))
                    .text_color(palette.header_muted)
                    .text_ellipsis()
                    .child(self.display_name()),
            );
        if let Some(prompt) = self
            .import
            .password_prompt()
            .filter(|_| !self.import.busy())
        {
            return self.password_card(card, prompt, window, cx);
        }
        let Some(pages) = self.import.ocr_required().map(<[u32]>::to_vec) else {
            let cancelled = self.conversion_cancelled();
            let running = self.import.busy();
            let (title, busy) = if self.import.stopping() {
                (crate::ui::text::CANCELLING, true)
            } else if running {
                (
                    if self.import.recognizing() {
                        text::RECOGNIZING_TEXT
                    } else {
                        text::CONVERTING_TO_MARKDOWN
                    },
                    true,
                )
            } else if self.import.auto_pending() {
                (text::WAITING_TO_CONVERT, true)
            } else if cancelled {
                // "Try again" in the toolbar converts it afresh.
                (text::CONVERSION_CANCELLED, false)
            } else if self.error.is_some() {
                (text::CONVERSION_NOT_COMPLETED, false)
            } else {
                (text::CONVERT_TO_BEGIN, false)
            };
            return card
                .child(if busy {
                    ui::activity("conversion-activity", title, theme)
                        .text_size(px(14.))
                        .into_any_element()
                } else {
                    ui::card_title(title).into_any_element()
                })
                .when_some(
                    self.error.clone().filter(|_| !busy && !cancelled),
                    |v, error| v.child(ui::card_text(error, theme)),
                )
                // A running conversion can be stopped without closing the
                // tab (#950); the supervised worker ends at once.
                .when(running && !self.import.stopping(), |v| {
                    v.child(
                        div().flex().child(
                            ui::control("cancel-conversion", crate::ui::text::CANCEL, theme, true)
                                .when(cfg!(test), |v| {
                                    v.debug_selector(|| "cancel-conversion".into())
                                })
                                .on_click(cx.listener(|this, _, _, cx| {
                                    this.import.stop();
                                    cx.notify();
                                })),
                        ),
                    )
                })
                .into_any_element();
        };
        let count = pages.len();
        let card = card
            .child(ui::card_title(if count == 0 {
                text::RECOGNITION_NEEDS_ATTENTION
            } else {
                self.ocr_pages_title()
            }))
            .when(count > 0, |v| {
                v.child(ui::card_text(
                    if self.import.ocr_all_pages() {
                        text::no_usable_text_all(count)
                    } else {
                        text::no_usable_text(count, ui::page_ranges(&pages, 8))
                    },
                    theme,
                ))
            });
        if !ocr::SUPPORTED {
            return card
                .child(ui::card_text(text::LOCAL_OCR_UNAVAILABLE, theme))
                .child(
                    ui::card_actions().child(
                        ui::primary_button(
                            "extract-native",
                            text::USE_NATIVE_TEXT_ONLY,
                            theme,
                            true,
                        )
                        .on_click(|_, window, cx| {
                            window.dispatch_action(Box::new(ExtractNative), cx)
                        }),
                    ),
                )
                .into_any_element();
        }
        if self.import.recognizing() {
            let stopping = self.import.stopping();
            return card
                .child(ui::activity(
                    "recognition-activity",
                    if stopping {
                        crate::ui::text::CANCELLING
                    } else {
                        text::RECOGNIZING_TEXT
                    },
                    theme,
                ))
                // Recognition stops like a conversion; the card returns here
                // with the language choice kept, so Run OCR retries (#950).
                .when(!stopping, |v| {
                    v.child(
                        div().flex().child(
                            ui::control("cancel-recognition", crate::ui::text::CANCEL, theme, true)
                                .when(cfg!(test), |v| {
                                    v.debug_selector(|| "cancel-recognition".into())
                                })
                                .on_click(cx.listener(|this, _, _, cx| {
                                    this.import.stop();
                                    cx.notify();
                                })),
                        ),
                    )
                })
                .into_any_element();
        }
        if installing {
            return card
                .child(match &progress {
                    Some(state) => {
                        ui::setup_progress("ocr-setup-progress", state, cancelling, theme)
                            .into_any_element()
                    }
                    None => ui::activity("ocr-setup-progress", crate::ui::text::PREPARING, theme)
                        .into_any_element(),
                })
                .child(
                    ui::card_actions().child(
                        crate::settings::control(
                            "cancel-ocr-setup",
                            crate::ui::text::CANCEL,
                            theme,
                            progress.is_some() && !cancelling,
                        )
                        .when(cfg!(test), |v| {
                            v.debug_selector(|| "cancel-ocr-setup".into())
                        })
                        .track_focus(
                            &self
                                .card_focus
                                .clone()
                                .tab_stop(progress.is_some() && !cancelling),
                        )
                        .on_click(cx.listener(|this, _, _, cx| {
                            this.model_panel
                                .update(cx, |panel, cx| panel.cancel_setup(cx))
                        })),
                    ),
                )
                .into_any_element();
        }
        let disabled = self.import.busy() || self.import.slot_busy() || self.ocr_state.busy();
        let failed = match &self.ocr_state {
            OcrState::Failed(error) if shared => Some(error.clone()),
            _ => self
                .import
                .ocr_failure()
                .cloned()
                .or_else(|| self.import.ocr_error().cloned()),
        };
        let primary = if ready && self.import.ocr_error().is_none() {
            text::RUN_OCR
        } else if failed.is_some() {
            text::RETRY
        } else if pending.total() == 0 {
            // Installed but not yet checked: setup only verifies it.
            text::RUN_OCR
        } else {
            text::DOWNLOAD_AND_RECOGNIZE
        };
        if std::mem::take(&mut self.focus_card)
            && (self.focus.contains_focused(window, cx) || window.focused(cx).is_none())
        {
            let focus = self.card_focus.clone();
            window.defer(cx, move |window, cx| window.focus(&focus, cx));
        }
        let name = model
            .map(crate::app::models::short_name)
            .unwrap_or_else(|| text::OCR.into());
        // The engine says what it cannot read.
        let hint = if unreadable.is_empty() {
            choice.reason.clone()
        } else {
            text::cannot_read(
                &choice.reason,
                mdoc_import::ocr::title(ocr_model),
                recognition::label(&unreadable),
            )
        };
        let accent = theme.search_accent();
        let picker = div()
            .id("recognition-languages")
            .role(gpui::Role::RadioGroup)
            .aria_label(text::RECOGNITION_LANGUAGES)
            .flex()
            .flex_wrap()
            .items_center()
            .gap_1()
            .child(
                div()
                    .mr_1()
                    .text_size(px(12.))
                    .text_color(palette.header_muted)
                    .child(text::RECOGNIZE),
            )
            .children(mdoc_import::ocr::offered().into_iter().enumerate().map(
                |(index, languages)| {
                    let selected = choice.languages == languages;
                    let label = recognition::label(languages);
                    let selector = text::recognize_selector(&label);
                    crate::ui::focus_ring(("recognition-language", index), theme)
                        .role(gpui::Role::RadioButton)
                        .aria_label(label.clone())
                        .aria_toggled(if selected {
                            gpui::Toggled::True
                        } else {
                            gpui::Toggled::False
                        })
                        .key_context("UiControl")
                        .tab_index(0)
                        .tab_stop(!disabled)
                        .px_2()
                        .py(px(2.))
                        .rounded_md()
                        .border_1()
                        .border_color(if selected { accent } else { palette.border })
                        .when(selected, |v| v.bg(gpui::Hsla { a: 0.08, ..accent }))
                        .text_size(px(12.))
                        .text_color(if selected { accent } else { palette.header_fg })
                        .when(!disabled, |v| v.cursor_pointer())
                        .when(disabled, |v| v.opacity(0.45))
                        .child(label)
                        .when(cfg!(test), move |v| v.debug_selector(move || selector))
                        .on_click(cx.listener(move |this, _, _, cx| {
                            if !disabled {
                                this.pick_recognition(languages.to_vec(), cx);
                            }
                        }))
                },
            ));
        card.child(picker)
            .child(ui::card_hint(hint, theme).when(cfg!(test), |v| {
                v.debug_selector(|| "ocr-language-hint".into())
            }))
            .child(
                ui::card_hint(text::FOR_THIS_DOCUMENT_ONLY, theme).when(cfg!(test), |v| {
                    v.debug_selector(|| "ocr-document-only".into())
                }),
            )
            .when_some(failed, |v, error| v.child(ui::card_error(error, theme)))
            .when(self.conversion_cancelled(), |v| {
                v.child(ui::card_text(text::RECOGNITION_CANCELLED, theme))
            })
            .child(
                ui::card_actions()
                    .child(
                        ui::primary_button("run-ocr", primary, theme, !disabled)
                            .track_focus(&self.card_focus.clone().tab_stop(!disabled))
                            .when(cfg!(test), move |v| {
                                v.debug_selector(move || primary.into())
                            })
                            .on_click(move |_, window, cx| {
                                if !disabled {
                                    window.dispatch_action(Box::new(RunOcr), cx)
                                }
                            }),
                    )
                    .child(
                        crate::settings::control(
                            "extract-native",
                            text::USE_NATIVE_TEXT_ONLY,
                            theme,
                            !disabled,
                        )
                        .when(cfg!(test), |v| {
                            v.debug_selector(|| text::USE_NATIVE_TEXT_ONLY.into())
                        })
                        .on_click(move |_, window, cx| {
                            if !disabled {
                                window.dispatch_action(Box::new(ExtractNative), cx)
                            }
                        }),
                    )
                    .child(
                        ui::link_button("choose-ocr-model", ui::text::CHOOSE_MODEL, theme, true)
                            .when(cfg!(test), |v| {
                                v.debug_selector(|| "choose-ocr-model".into())
                            })
                            .on_click(cx.listener(move |this, _, window, cx| {
                                if let Some(model) = model {
                                    this.model_panel.update(cx, |panel, cx| {
                                        panel.show_section(model, window, cx)
                                    });
                                }
                            })),
                    ),
            )
            .child(ui::card_note(
                if ready || pending.total() == 0 {
                    text::runs_here(&name)
                } else {
                    text::downloads_once(&name, pending.download_label())
                },
                theme,
            ))
            .when_some(model, |card, model| {
                card.child(ui::model_attribution("ocr-model-attribution", model, theme))
            })
            .into_any_element()
    }

    /// "All pages…" when the whole PDF is scanned, "Some pages…" otherwise.
    pub(super) fn ocr_pages_title(&self) -> &'static str {
        match (
            self.import.ocr_all_pages(),
            self.import.ocr_required().map(<[u32]>::len),
        ) {
            (true, Some(1)) => text::THIS_PAGE_NEEDS_RECOGNITION,
            (true, _) => text::ALL_PAGES_NEED_RECOGNITION,
            (false, _) => text::SOME_PAGES_NEED_RECOGNITION,
        }
    }

    /// The preview pane without a page: progress, or one plain message with
    /// Retry only when another attempt can succeed.
    pub(super) fn preview_status(&self, theme: Theme) -> gpui::AnyElement {
        if self.preview.loading() {
            return ui::activity("preview-activity", text::PREPARING_PREVIEW, theme)
                .into_any_element();
        }
        let message = if self.preview.queued() {
            text::WAITING_TO_PREPARE_PREVIEW.to_string()
        } else {
            self.preview
                .message()
                .cloned()
                .unwrap_or_else(|| text::NO_PREVIEW_TITLE.into())
        };
        div()
            .id("preview-status")
            .when(cfg!(test), |v| v.debug_selector(|| "preview-status".into()))
            .max_w(px(360.))
            .px_6()
            .flex()
            .flex_col()
            .items_center()
            .gap_3()
            .text_center()
            .text_size(px(14.))
            .child(message)
            .when(!self.preview.queued() && self.preview.retryable(), |v| {
                v.child(button(text::RETRY, RetryPreview, theme))
            })
            .into_any_element()
    }

    pub(super) fn notice(
        &self,
        id: &'static str,
        title: &'static str,
        detail: String,
        failure: bool,
        cx: &mut Context<Self>,
    ) -> gpui::AnyElement {
        let theme = self.theme.get();
        let palette = theme.pdf_style();
        let accent = if failure {
            style::markdown_style(theme).alert_caution
        } else {
            style::markdown_style(theme).alert_warning
        };
        div()
            .id(id)
            .flex()
            .when(cfg!(test), |v| v.debug_selector(move || id.into()))
            .flex_col()
            .flex_shrink_0()
            .text_size(px(12.))
            .bg(gpui::Hsla { a: 0.07, ..accent })
            .border_b_1()
            .border_color(palette.border)
            .child(
                div()
                    .flex()
                    .items_center()
                    .gap_2()
                    .px_3()
                    .py_1()
                    .child(
                        div()
                            .text_color(accent)
                            .child(if failure { "!" } else { "ⓘ" }),
                    )
                    .child(
                        div()
                            .flex_shrink_0()
                            .text_color(palette.header_fg)
                            .child(title),
                    )
                    .child(
                        div()
                            .id((id, 2usize))
                            .flex_1()
                            .min_w_0()
                            .text_color(palette.header_muted)
                            // Plain text isn't in the accessibility tree; a Label
                            // node lets VoiceOver and Narrator read the notice.
                            .role(gpui::Role::Label)
                            .aria_label(format!("{title}: {detail}"))
                            .child(detail),
                    )
                    .when(id == "preview-notice" && self.preview.retryable(), |v| {
                        v.child(button(text::RETRY, RetryPreview, theme))
                    })
                    .when_some(
                        (self.copy_refusal.as_ref())
                            .filter(|_| id == "copy-refusal-notice")
                            .and_then(|refusal| refusal.step),
                        |v, step| {
                            v.child(
                                ui::filled_button("copy-refusal-step", step.label(), theme, true)
                                    .when(cfg!(test), |v| {
                                        v.debug_selector(|| "copy-refusal-step".into())
                                    })
                                    .on_click(cx.listener(move |_, _, _, cx| {
                                        cx.emit(workspace::TabEvent::CopyStep(step));
                                    })),
                            )
                        },
                    )
                    .when(id == "ocr-notice", |v| {
                        v.child(
                            ui::control("show-markdown", text::SHOW, theme, true).on_click(
                                cx.listener(|this, _, _, cx| {
                                    this.original_selected = false;
                                    cx.notify();
                                }),
                            ),
                        )
                    })
                    .child(
                        crate::ui::focus_ring((id, 1usize), theme)
                            .role(gpui::Role::Button)
                            .key_context("UiControl")
                            .tab_index(0)
                            .aria_label(text::DISMISS_NOTICE)
                            .when(cfg!(test), |v| {
                                v.debug_selector(move || format!("{id}-dismiss"))
                            })
                            .flex_shrink_0()
                            .cursor_pointer()
                            .px_2()
                            .py_1()
                            .rounded_md()
                            .hover(|v| v.bg(palette.placeholder_bg))
                            .child("×")
                            .on_click(cx.listener(move |this, _, _, cx| {
                                match id {
                                    "import-notice" => this.session.warning = None,
                                    "ocr-notice" => this.import.dismiss_ocr_notice(),
                                    "preview-notice" => this.preview.clear_message(),
                                    "setup-notice" => this.import.dismiss_setup_error(),
                                    "clipboard-notice" => this.clipboard_notice.clear(),
                                    "copy-refusal-notice" => this.copy_refusal = None,
                                    _ => this.error = None,
                                }
                                cx.notify();
                            })),
                    ),
            )
            .into_any_element()
    }
}
