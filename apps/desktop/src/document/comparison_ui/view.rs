//! How the Model comparison window draws: the models to compare and their
//! settings, the run and export controls, and the results.
use super::{CloseComparison, View};
use crate::{
    document::{
        comparison::{Config, Input},
        text,
    },
    settings::control,
    ui,
    ui::style,
    workspace::{self, Close, Quit, Settings},
};
use gpui::{Context, Window, div, prelude::*, px};
use mdoc_store::settings::{Model, OcrModel, PiiModel};
use std::sync::atomic::Ordering;

impl Render for View {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let theme = self.theme.get();
        let p = theme.pdf_style();
        let is_ocr = matches!(self.input, Input::Pdf(_));
        div()
            .id("model-comparison")
            .size_full()
            .flex()
            .flex_col()
            .gap_3()
            .p_4()
            .bg(p.bg)
            .text_color(p.header_fg)
            .track_focus(&self.focus)
            .key_context("ModelComparison")
            .on_action(cx.listener(|this, _: &CloseComparison, window, _| {
                this.cancel.store(true, Ordering::Relaxed);
                window.remove_window();
            }))
            .on_action(cx.listener(|this, _: &Close, window, _| {
                this.cancel.store(true, Ordering::Relaxed);
                window.remove_window();
            }))
            .on_action(cx.listener(|_, _: &Settings, _, cx| {
                if let Some(handle) = cx
                    .windows()
                    .into_iter()
                    .find_map(|w| w.downcast::<workspace::Workspace>())
                {
                    cx.defer(move |cx| {
                        let _ =
                            handle.update(cx, |tabs, window, cx| tabs.show_settings(window, cx));
                    });
                }
            }))
            .on_action(cx.listener(|_, _: &Quit, _, cx| {
                if let Some(handle) = cx
                    .windows()
                    .into_iter()
                    .find_map(|w| w.downcast::<workspace::Workspace>())
                {
                    cx.defer(move |cx| {
                        let _ = handle.update(cx, |tabs, window, cx| tabs.quit(window, cx));
                    });
                }
            }))
            .child(
                div()
                    .flex()
                    .items_center()
                    .gap_2()
                    .child(div().flex_1().text_size(px(18.)).child(if is_ocr {
                        text::COMPARE_OCR_MODELS
                    } else {
                        text::COMPARE_PII_MODELS
                    }))
                    .child(
                        control("new-comparison", text::NEW_COMPARISON, theme, !self.running)
                            .on_click(cx.listener(move |this, _, window, cx| {
                                if this.running {
                                    return;
                                }
                                let owner = this.owner.clone();
                                let kind = if is_ocr {
                                    Model::Ocr(OcrModel::Cyrillic)
                                } else {
                                    Model::Pii(PiiModel::Fp16)
                                };
                                let _ = owner.update(cx, |owner, cx| {
                                    owner.open_comparison(kind, window, cx)
                                });
                            })),
                    )
                    .child(
                        control("close-comparison", crate::ui::text::CLOSE, theme, true).on_click(
                            cx.listener(|this, _, window, _| {
                                this.cancel.store(true, Ordering::Relaxed);
                                window.remove_window();
                            }),
                        ),
                    ),
            )
            .child(div().text_size(px(11.)).child(text::captured_input(
                &self.input_hash,
                if is_ocr {
                    text::PDF_SNAPSHOT
                } else {
                    text::MARKDOWN_SNAPSHOT
                },
            )))
            .child(
                div()
                    .flex()
                    .flex_col()
                    .gap_2()
                    .id("qa-configurations")
                    .max_h(px(280.))
                    .min_h_0()
                    .overflow_y_scroll()
                    .children(self.configs.iter().enumerate().map(|(index, config)| {
                        let config = config.clone();
                        let models: Vec<_> = match config {
                            Config::Ocr(_) => OcrModel::ALL.map(Model::Ocr).to_vec(),
                            Config::Pii(_) => PiiModel::ALL.map(Model::Pii).to_vec(),
                        };
                        div()
                            .flex()
                            .flex_wrap()
                            .gap_2()
                            .items_center()
                            .child(text::run_number(index + 1))
                            .children(models.into_iter().map(|model| {
                                let selected = match (&config, model) {
                                    (Config::Ocr(c), Model::Ocr(m)) => c.model == m,
                                    (Config::Pii(c), Model::Pii(m)) => c.model == m,
                                    _ => false,
                                };
                                let available = !matches!(
                                    self.panel.read(cx).statuses
                                        [Model::ALL.iter().position(|m| *m == model).unwrap()],
                                    crate::settings::Status::Unavailable(_)
                                );
                                control(
                                    (
                                        "qa-model",
                                        index * Model::ALL.len()
                                            + Model::ALL.iter().position(|m| *m == model).unwrap(),
                                    ),
                                    format!("{}{}", if selected { "● " } else { "" }, model.name()),
                                    theme,
                                    !self.running && available,
                                )
                                .on_click(cx.listener(
                                    move |this, _, _, cx| {
                                        if this.running || !available {
                                            return;
                                        }
                                        match (&mut this.configs[index], model) {
                                            (Config::Ocr(c), Model::Ocr(m)) => c.model = m,
                                            (Config::Pii(c), Model::Pii(m)) => c.model = m,
                                            _ => {}
                                        };
                                        cx.notify();
                                    },
                                ))
                            }))
                            .child(div().text_size(px(11.)).child(if is_ocr {
                                text::MINIMUM_CONFIDENCE
                            } else {
                                text::THRESHOLD
                            }))
                            .child(
                                div()
                                    .w(px(65.))
                                    .border_1()
                                    .border_color(p.border)
                                    .px_2()
                                    .child(self.values[index].clone()),
                            )
                            .when(is_ocr, |v| {
                                v.children([150, 200, 300].map(|dpi| {
                                    let selected =
                                        matches!(&self.configs[index],Config::Ocr(c) if c.dpi==dpi);
                                    control(
                                        ("qa-dpi", index * 10 + (dpi / 50) as usize),
                                        text::dpi(if selected { "● " } else { "" }, dpi),
                                        theme,
                                        !self.running,
                                    )
                                    .on_click(cx.listener(
                                        move |this, _, _, cx| {
                                            if !this.running {
                                                if let Config::Ocr(c) = &mut this.configs[index] {
                                                    c.dpi = dpi;
                                                }
                                                cx.notify();
                                            }
                                        },
                                    ))
                                }))
                            })
                            .child(
                                control(("qa-run", index), text::RUN, theme, !self.running)
                                    .on_click(
                                        cx.listener(move |this, _, _, cx| {
                                            this.run(vec![index], cx)
                                        }),
                                    ),
                            )
                    })),
            )
            .when(is_ocr, |v| {
                v.child(
                    div().flex().gap_2().child(text::PAGES).child(
                        div()
                            .w(px(170.))
                            .border_1()
                            .border_color(p.border)
                            .px_2()
                            .when(self.frozen_pages.is_none(), |v| v.child(self.pages.clone()))
                            .when_some(self.frozen_pages.clone(), |v, pages| {
                                v.child(
                                    pages
                                        .map(|p| {
                                            p.iter()
                                                .map(u32::to_string)
                                                .collect::<Vec<_>>()
                                                .join(", ")
                                        })
                                        .unwrap_or_else(|| text::ALL_PAGES_FROZEN.into()),
                                )
                            }),
                    ),
                )
            })
            .child(
                div()
                    .flex()
                    .gap_2()
                    .child(
                        control("run-both", text::RUN_BOTH, theme, !self.running)
                            .on_click(cx.listener(|this, _, _, cx| this.run(vec![0, 1], cx))),
                    )
                    .child(
                        control(
                            "cancel-comparison",
                            crate::ui::text::CANCEL,
                            theme,
                            self.running,
                        )
                        .on_click(cx.listener(|this, _, _, cx| {
                            if this.running {
                                this.cancel.store(true, Ordering::Relaxed);
                                cx.notify();
                            }
                        })),
                    )
                    .child(
                        control(
                            "export-comparison",
                            text::EXPORT_REPORT,
                            theme,
                            !self.records.is_empty(),
                        )
                        .on_click(cx.listener(|this, _, w, cx| this.export(false, w, cx))),
                    )
                    .when(is_ocr, |v| {
                        v.child(
                            control(
                                "export-ocr-markdown",
                                text::SAVE_MARKDOWN,
                                theme,
                                !self.records.is_empty(),
                            )
                            .on_click(cx.listener(|this, _, w, cx| this.export(true, w, cx))),
                        )
                    })
                    .child(div().flex_1())
                    .when(self.running, |v| {
                        v.child(ui::activity(
                            "comparison-activity",
                            if self.cancel.load(Ordering::Relaxed) {
                                text::CANCELLING_AFTER_STEP
                            } else {
                                text::COMPARING_MODELS
                            },
                            theme,
                        ))
                    }),
            )
            .when_some(self.error.clone(), |v, e| {
                v.child(
                    div()
                        .text_color(style::markdown_style(theme).alert_warning)
                        .child(e),
                )
            })
            .child(
                div()
                    .flex()
                    .gap_2()
                    .flex_wrap()
                    .children(self.records.iter().enumerate().map(|(index, record)| {
                        control(
                            ("qa-result", index),
                            format!(
                                "{} · {:.2}s",
                                record.configuration.name(),
                                record.elapsed_ms as f64 / 1000.
                            ),
                            theme,
                            true,
                        )
                        .when(index == self.selected, |v| v.bg(p.placeholder_bg))
                        .on_click(cx.listener(move |this, _, _, cx| {
                            this.selected = index;
                            this.update_display();
                            cx.notify();
                        }))
                    })),
            )
            .when(is_ocr, |v| {
                v.child(
                    div()
                        .flex()
                        .gap_2()
                        .child(
                            control("show-raw-ocr", text::RECOGNITION_OUTPUT, theme, true)
                                .on_click(cx.listener(|this, _, _, cx| {
                                    this.raw = true;
                                    this.update_display();
                                    cx.notify();
                                })),
                        )
                        .child(
                            control("show-prepared-ocr", text::PREPARED_MARKDOWN, theme, true)
                                .on_click(cx.listener(|this, _, _, cx| {
                                    this.raw = false;
                                    this.update_display();
                                    cx.notify();
                                })),
                        ),
                )
            })
            .when_some(self.records.get(self.selected), |v, record| {
                v.child(div().text_size(px(11.)).child(format!(
                    "{} · {} · {}",
                    record.configuration.summary(),
                    record.identity["revision"].as_str().unwrap_or_default(),
                    record.warnings()
                )))
                .child(
                    div()
                        .id("qa-page-details")
                        .max_h(px(90.))
                        .overflow_y_scroll()
                        .text_size(px(11.))
                        .child(record.measurements()),
                )
                .child(
                    div()
                        .id("qa-output")
                        .flex_1()
                        .min_h_0()
                        .overflow_y_scroll()
                        .p_3()
                        .border_1()
                        .border_color(p.border)
                        .font_family("monospace")
                        .text_size(px(13.))
                        .child(self.display.clone()),
                )
            })
            .when(self.records.is_empty(), |v| {
                v.child(div().flex_1().child(text::RESULTS_READ_ONLY))
            })
    }
}
