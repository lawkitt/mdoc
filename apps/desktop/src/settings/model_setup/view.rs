//! A model's row on the Detection and Recognition pages: its status, its
//! buttons and its details.
use crate::settings::{Panel, Status, text};
use crate::{app::models, ui};
use gpui::{AnyElement, Context, div, prelude::*, px};
use mdoc_import::{model_download, model_work, ocr};
use mdoc_store::settings::{Model, PiiModel};

impl Panel {
    pub(in crate::settings) fn section_note(&self, text: &str, model: Model) -> String {
        match self.pending[Self::index(model)].runtime {
            0 => text.to_owned(),
            runtime => text::with_runtime(text, model_download::megabytes(runtime)),
        }
    }

    pub(in crate::settings) fn row(&self, model: Model, cx: &mut Context<Self>) -> AnyElement {
        let theme = self.theme.get();
        let palette = theme.pdf_style();
        let accent = theme.search_accent();
        let index = Self::index(model);
        let status = self.row_status(model);
        let pending = self.pending[index];
        let selected = match model {
            Model::Ocr(m) => self.draft.ocr.model == m,
            Model::Pii(m) => self.draft.pseudonymization.model == m,
        };
        let selectable = !matches!(self.statuses[index], Status::Unavailable(_));
        let idle = self.working.is_none() && !model_work::busy();
        let working = self.working == Some(model);
        let checking = self.row_checking(model);
        let missing = matches!(status, Status::Missing)
            || (matches!(status, Status::Unknown) && !checking && pending.total() > 0);
        let removable = idle && !matches!(self.statuses[index], Status::Missing | Status::Unknown);
        let bytes = match model {
            Model::Ocr(m) => ocr::manifest::for_model(m)
                .artifacts
                .iter()
                .map(|a| a.size)
                .sum::<u64>(),
            Model::Pii(m) => mdoc_detect::manifest_for(m)
                .files
                .iter()
                .map(|a| a.bytes)
                .sum::<u64>(),
        };
        // A missing model's size sits on its Download button.
        let status_label = if checking {
            text::CHECKING.to_owned()
        } else {
            text::status_label(status)
        };
        let download_label = text::download(model_download::megabytes(if pending.model > 0 {
            pending.model
        } else {
            bytes
        }));
        let note = (missing && selected && !working).then_some(text::DOWNLOADS_ON_FIRST_USE);
        div()
            .id(("settings-choice", index))
            .flex()
            .flex_col()
            .min_w_0()
            .border_b_1()
            .border_color(palette.border)
            .when(selected, |v| v.bg(gpui::Hsla { a: 0.05, ..accent }))
            .child(
                div()
                    .flex()
                    .flex_wrap()
                    .items_center()
                    .gap_2()
                    .p_2()
                    .child(
                        crate::ui::focus_ring(("select-model", index), theme)
                            .role(gpui::Role::RadioButton)
                            .aria_label(models::title(model))
                            .aria_toggled(if selected {
                                gpui::Toggled::True
                            } else {
                                gpui::Toggled::False
                            })
                            .key_context("UiControl")
                            .tab_index(0)
                            .tab_stop(selectable)
                            .flex()
                            .flex_1()
                            .flex_basis(px(220.))
                            .min_w(px(200.))
                            .items_center()
                            .gap_3()
                            .when(selectable, |v| v.cursor_pointer())
                            .when(!selectable, |v| v.opacity(0.5))
                            .child(
                                div()
                                    .flex()
                                    .flex_col()
                                    .flex_1()
                                    .min_w_0()
                                    .gap_1()
                                    .child(
                                        div()
                                            .flex()
                                            .items_center()
                                            .gap_2()
                                            .min_w_0()
                                            .child(
                                                div()
                                                    .text_size(px(13.))
                                                    .text_ellipsis()
                                                    .child(models::title(model)),
                                            )
                                            .when(model.recommended(), |v| {
                                                v.child(
                                                    div()
                                                        .flex_shrink_0()
                                                        .px_1()
                                                        .rounded_sm()
                                                        .text_size(px(10.))
                                                        .text_color(accent)
                                                        .border_1()
                                                        .border_color(gpui::Hsla {
                                                            a: 0.5,
                                                            ..accent
                                                        })
                                                        .child(text::RECOMMENDED),
                                                )
                                            }),
                                    )
                                    .child(
                                        div()
                                            .text_size(px(11.))
                                            .text_color(palette.header_muted)
                                            .child(text::model_line(
                                                model.summary(),
                                                model_download::megabytes(bytes),
                                                &status_label,
                                                note,
                                            )),
                                    ),
                            )
                            .child(
                                div()
                                    .text_color(if selected {
                                        accent
                                    } else {
                                        palette.header_muted
                                    })
                                    .child(if selected { "●" } else { "○" }),
                            )
                            .when(cfg!(test) && model == Model::Pii(PiiModel::Fp32), |v| {
                                v.debug_selector(|| "settings-select-fp32".into())
                            })
                            .map(|v| self.reveal(("select-model", index), v, selectable, cx))
                            .on_click(cx.listener(move |this, _, _, cx| {
                                if selectable && !selected {
                                    this.select(model, cx);
                                }
                            })),
                    )
                    .child(
                        div()
                            .flex()
                            .flex_shrink_0()
                            .items_center()
                            .gap_2()
                            .when(working, |v| {
                                v.child(
                                    self.scrolled_quiet(
                                        ("cancel-model", index),
                                        crate::ui::text::CANCEL,
                                        theme,
                                        !self.cancelling(),
                                        cx,
                                    )
                                    .on_click(cx.listener(|this, _, _, cx| this.cancel_setup(cx))),
                                )
                            })
                            .when(missing && !working, |v| {
                                v.child(
                                    self.scrolled_control(
                                        ("download-model", index),
                                        download_label,
                                        theme,
                                        idle && ocr::SUPPORTED,
                                        cx,
                                    )
                                    .on_click(cx.listener(
                                        move |this, _, _, cx| {
                                            if idle && ocr::SUPPORTED {
                                                this.setup(model, false, cx);
                                            }
                                        },
                                    )),
                                )
                            })
                            .when(matches!(status, Status::Damaged(_)) && !working, |v| {
                                v.child(
                                    self.scrolled_control(
                                        ("repair-now", index),
                                        text::REPAIR,
                                        theme,
                                        idle && ocr::SUPPORTED,
                                        cx,
                                    )
                                    .on_click(cx.listener(
                                        move |this, _, _, cx| {
                                            if idle && ocr::SUPPORTED {
                                                this.setup(model, false, cx);
                                            }
                                        },
                                    )),
                                )
                            })
                            // Remove… is a bordered button like Download and Repair, and is
                            // absent rather than dimmed while it can't run.
                            .when(!missing && !working && !checking && removable, |v| {
                                v.child(
                                    self.scrolled_control(
                                        ("remove-model", index),
                                        text::REMOVE_ELLIPSIS,
                                        theme,
                                        true,
                                        cx,
                                    )
                                    .when(cfg!(test), move |v| {
                                        v.debug_selector(move || format!("settings-remove-{index}"))
                                    })
                                    .on_click(cx.listener(
                                        move |this, _, window, cx| {
                                            this.ask_remove_model(model, window, cx)
                                        },
                                    )),
                                )
                            })
                            .child(
                                self.scrolled_quiet(
                                    ("model-details", index),
                                    if self.details == Some(model) {
                                        text::LESS
                                    } else {
                                        text::DETAILS
                                    },
                                    theme,
                                    true,
                                    cx,
                                )
                                .when(cfg!(test) && model == Model::Pii(PiiModel::Fp16), |v| {
                                    v.debug_selector(|| "settings-details-fp16".into())
                                })
                                .on_click(
                                    cx.listener(move |this, _, _, cx| {
                                        this.toggle_details(model, cx)
                                    }),
                                ),
                            ),
                    ),
            )
            .child(div().px_2().pb_2().child(ui::model_attribution(
                ("model-attribution", index),
                model,
                theme,
            )))
            .when_some(self.progress_of(model), |v, state| {
                v.child(div().px_2().pb_3().child(ui::setup_progress(
                    ("model-progress", index),
                    &state,
                    self.cancelling(),
                    theme,
                )))
            })
            .when(self.details == Some(model), |v| {
                let (revision, path) = match model {
                    Model::Ocr(m) => (
                        ocr::manifest::for_model(m).revision.to_string(),
                        ocr::root()
                            .map(|root| ocr::model_root(&root, m))
                            .map_err(|e| e.to_string()),
                    ),
                    Model::Pii(m) => (
                        mdoc_detect::manifest_for(m).revision,
                        mdoc_detect::root_for(m).map_err(|e| e.to_string()),
                    ),
                };
                let evidence = match model {
                    Model::Ocr(m) => m.evidence(),
                    Model::Pii(m) => m.evidence(),
                };
                let (_, license) = models::source_and_license(model);
                let mut details = div()
                    .flex()
                    .flex_col()
                    .gap_2()
                    .px_2()
                    .pb_3()
                    .min_w_0()
                    .text_size(px(11.))
                    .text_color(palette.header_muted)
                    .child(div().text_color(palette.header_fg).child(text::model_size(
                        model.name(),
                        model_download::megabytes(bytes),
                    )))
                    .child(evidence)
                    .when_some(
                        match status {
                            Status::Damaged(e) | Status::Unavailable(e) => Some(e.clone()),
                            _ => None,
                        },
                        |v, reason| v.child(reason),
                    );
                if let Model::Pii(m) = model {
                    let manifest = mdoc_detect::manifest_for(m);
                    let export_url = format!(
                        "https://huggingface.co/{}/tree/{}",
                        manifest.repository, manifest.revision
                    );
                    details = details
                        .child(m.description())
                        .child(text::languages(m.languages()))
                        .child(
                            div()
                                .flex()
                                .flex_wrap()
                                .gap_2()
                                .child(
                                    self.scrolled_quiet(
                                        ("settings-hf", index * 2),
                                        text::MODEL_CARD,
                                        theme,
                                        true,
                                        cx,
                                    )
                                    .on_click(move |_, _, cx| cx.open_url(m.hugging_face_url())),
                                )
                                .child(
                                    self.scrolled_quiet(
                                        ("settings-hf", index * 2 + 1),
                                        text::PINNED_FILES,
                                        theme,
                                        true,
                                        cx,
                                    )
                                    .on_click(move |_, _, cx| cx.open_url(&export_url)),
                                ),
                        );
                }
                details = details
                    .child(text::model_facts(
                        &revision,
                        &license,
                        path.map(|p| p.display().to_string()).unwrap_or_else(|e| e),
                    ))
                    .child(match model {
                        Model::Ocr(_) => text::OCR_RUNTIME,
                        Model::Pii(_) => text::PII_RUNTIME,
                    })
                    .child(div().flex().flex_wrap().gap_2().when(!missing, |v| {
                        v.child(
                            self.scrolled_quiet(
                                ("repair-model", index),
                                text::REPAIR,
                                theme,
                                idle && ocr::SUPPORTED,
                                cx,
                            )
                            .on_click(cx.listener(
                                move |this, _, _, cx| {
                                    if idle && ocr::SUPPORTED {
                                        this.setup(model, false, cx);
                                    }
                                },
                            )),
                        )
                    }));
                v.child(details)
            })
            .into_any_element()
    }
}
