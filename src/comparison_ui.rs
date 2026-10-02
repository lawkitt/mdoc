use crate::{
    comparison::{self, Config, Input, ResultRecord},
    settings::*,
    settings_ui::{self, control},
    *,
};
use gpui::{FocusHandle, actions};
actions!(model_comparison, [CloseComparison]);

pub struct View {
    input_hash: String,
    pub input: Input,
    pub configs: Vec<Config>,
    pub values: Vec<Entity<markdown_search::SearchInput>>,
    display: gpui::SharedString,
    pub records: Vec<ResultRecord>,
    pub selected: usize,
    pub raw: bool,
    pub running: bool,
    pub error: Option<String>,
    pub pages: Entity<markdown_search::SearchInput>,
    frozen_pages: Option<Option<std::collections::BTreeSet<u32>>>,
    cancel: Arc<AtomicBool>,
    generation: u64,
    theme: Rc<Cell<Theme>>,
    focus: FocusHandle,
    owner: gpui::WeakEntity<tabs::Tabs>,
    panel: Entity<settings_ui::Panel>,
    sources: Vec<PathBuf>,
}
impl Drop for View {
    fn drop(&mut self) {
        self.cancel.store(true, Ordering::Relaxed);
    }
}
impl View {
    pub fn new(
        snapshot: (Input, String),
        defaults: Preferences,
        theme: Rc<Cell<Theme>>,
        owner: gpui::WeakEntity<tabs::Tabs>,
        panel: Entity<settings_ui::Panel>,
        sources: Vec<PathBuf>,
        cx: &mut Context<Self>,
    ) -> Self {
        let (input, input_hash) = snapshot;
        let configs = match &input {
            Input::Pdf(_) => OcrModel::ALL
                .map(|model| {
                    Config::Ocr(OcrConfig {
                        model,
                        ..defaults.ocr.clone()
                    })
                })
                .to_vec(),
            Input::Markdown(_) => PiiModel::ALL
                .map(|model| {
                    Config::Pii(PiiConfig {
                        model,
                        ..defaults.pseudonymization.clone()
                    })
                })
                .to_vec(),
        };
        let values = configs
            .iter()
            .map(|c| {
                let value = match c {
                    Config::Ocr(c) => c.minimum_confidence,
                    Config::Pii(c) => c.threshold,
                };
                cx.new(|cx| {
                    let mut i = markdown_search::SearchInput::new(cx)
                        .with_key_context("SettingsInput")
                        .with_placeholder("0–1");
                    i.set_value(value.to_string(), cx);
                    i
                })
            })
            .collect();
        Self {
            values,
            display: "".into(),
            input,
            input_hash,
            configs,
            records: Vec::new(),
            selected: 0,
            raw: true,
            running: false,
            error: None,
            pages: cx.new(|cx| {
                markdown_search::SearchInput::new(cx)
                    .with_key_context("SettingsInput")
                    .with_placeholder("All pages")
            }),
            frozen_pages: None,
            cancel: Arc::new(AtomicBool::new(false)),
            generation: 0,
            theme,
            focus: cx.focus_handle(),
            owner,
            panel,
            sources,
        }
    }
    pub fn run(&mut self, indices: Vec<usize>, cx: &mut Context<Self>) {
        if self.running {
            return;
        }
        let pages = match &self.frozen_pages {
            Some(p) => p.clone(),
            None => match comparison::parse_pages(self.pages.read(cx).value()) {
                Ok(p) => p,
                Err(e) => {
                    self.error = Some(e);
                    cx.notify();
                    return;
                }
            },
        };
        let configs: Result<Vec<_>, String> = indices
            .into_iter()
            .map(|index| {
                let value = self.values[index]
                    .read(cx)
                    .value()
                    .parse::<f32>()
                    .map_err(|_| {
                        "Threshold/confidence must be a number from 0 to 1.".to_string()
                    })?;
                if !settings::unit_interval(value) {
                    return Err("Threshold/confidence must be finite and between 0 and 1.".into());
                }
                let mut config = self.configs[index].clone();
                match &mut config {
                    Config::Ocr(c) => c.minimum_confidence = value,
                    Config::Pii(c) => c.threshold = value,
                };
                Ok(config)
            })
            .collect();
        let configs = match configs {
            Ok(c) => c,
            Err(e) => {
                self.error = Some(e);
                cx.notify();
                return;
            }
        };
        for config in &configs {
            let model = match config {
                Config::Ocr(c) => Model::Ocr(c.model),
                Config::Pii(c) => Model::Pii(c.model),
            };
            let index = Model::ALL.iter().position(|m| *m == model).unwrap();
            if !matches!(
                self.panel.read(cx).statuses[index],
                settings_ui::Status::Ready
            ) {
                self.error = Some(format!(
                    "{} is not ready. Download/repair it in Settings first.",
                    model.name()
                ));
                cx.notify();
                return;
            }
        }
        let permit = match model_work::Permit::acquire() {
            Ok(p) => p,
            Err(e) => {
                self.error = Some(e);
                cx.notify();
                return;
            }
        };
        self.frozen_pages = Some(pages.clone());
        self.error = None;
        self.running = true;
        self.generation += 1;
        let generation = self.generation;
        self.cancel = Arc::new(AtomicBool::new(false));
        let cancel = self.cancel.clone();
        let input = self.input.clone();
        let task = cx.background_executor().spawn(async move {
            let mut results = Vec::new();
            for config in configs {
                if cancel.load(Ordering::Relaxed) {
                    break;
                }
                results.push(comparison::run(&input, config, pages.clone(), &cancel));
            }
            drop(permit);
            (results, cancel.load(Ordering::Relaxed))
        });
        cx.spawn(async move |this, cx| {
            let (results, cancelled) = task.await;
            let _ = this.update(cx, |this, cx| {
                this.finish(generation, results, cancelled, cx);
            });
        })
        .detach();
        cx.notify();
    }
    fn finish(
        &mut self,
        generation: u64,
        results: Vec<ResultRecord>,
        cancelled: bool,
        cx: &mut Context<Self>,
    ) {
        if self.generation != generation {
            return;
        }
        self.running = false;
        if cancelled {
            self.error = Some("Comparison cancelled; partial results discarded.".into());
        } else {
            self.records.extend(results);
            self.selected = self.records.len().saturating_sub(1);
            self.update_display();
        }
        cx.notify();
    }
    fn update_display(&mut self) {
        self.display = self
            .records
            .get(self.selected)
            .map(|r| r.text(self.raw))
            .unwrap_or_default()
            .into();
    }
    fn export(&mut self, markdown: bool, window: &mut Window, cx: &mut Context<Self>) {
        if self.records.is_empty() {
            return;
        }
        let text = if markdown {
            self.records
                .get(self.selected)
                .and_then(|r| r.output.get("prepared_markdown"))
                .and_then(|v| v.as_str())
                .map(str::to_owned)
        } else {
            None
        };
        if markdown && text.is_none() {
            return;
        }
        let directory = self
            .sources
            .first()
            .and_then(|p| p.parent())
            .map(PathBuf::from)
            .unwrap_or_else(|| PathBuf::from("."));
        let prompt = cx.prompt_for_new_path(
            &directory,
            Some(if markdown {
                "ocr-comparison.md"
            } else {
                "model-comparison.json"
            }),
        );
        let records = self.records.clone();
        let sources = self.sources.clone();
        cx.spawn_in(window, async move |this, cx| {
            let Ok(Ok(Some(path))) = prompt.await else {
                return;
            };
            let task = cx.background_executor().spawn(async move {
                if comparison::export_path_is_source(&path, &sources) {
                    return Err(
                        "Choose a different path to preserve the document and its source.".into(),
                    );
                }
                if let Some(text) = text {
                    let mut d = document::Document::default();
                    d.save(path, &text).map_err(|e| e.to_string())
                } else {
                    comparison::save_report(&path, &records)
                }
            });
            let result = task.await;
            let _ = this.update(cx, |this, cx| {
                this.error = result.err();
                cx.notify();
            });
        })
        .detach();
    }
}
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
                    .find_map(|w| w.downcast::<tabs::Tabs>())
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
                    .find_map(|w| w.downcast::<tabs::Tabs>())
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
                        "Compare OCR models"
                    } else {
                        "Compare pseudonymization models"
                    }))
                    .child(
                        control("new-comparison", "New comparison", theme, !self.running).on_click(
                            cx.listener(move |this, _, window, cx| {
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
                            }),
                        ),
                    )
                    .child(control("close-comparison", "Close", theme, true).on_click(
                        cx.listener(|this, _, window, _| {
                            this.cancel.store(true, Ordering::Relaxed);
                            window.remove_window();
                        }),
                    )),
            )
            .child(div().text_size(px(11.)).child(format!(
                "Captured input SHA-256: {} · {}",
                self.input_hash,
                if is_ocr {
                    "PDF snapshot"
                } else {
                    "Complete Markdown snapshot"
                }
            )))
            .child(
                div()
                    .flex()
                    .flex_col()
                    .gap_2()
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
                            .child(format!("Run {}", index + 1))
                            .children(models.into_iter().map(|model| {
                                let selected = match (&config, model) {
                                    (Config::Ocr(c), Model::Ocr(m)) => c.model == m,
                                    (Config::Pii(c), Model::Pii(m)) => c.model == m,
                                    _ => false,
                                };
                                let available = !matches!(
                                    self.panel.read(cx).statuses
                                        [Model::ALL.iter().position(|m| *m == model).unwrap()],
                                    settings_ui::Status::Unavailable(_)
                                );
                                control(
                                    (
                                        "qa-model",
                                        index * 4
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
                                "minimum confidence"
                            } else {
                                "threshold"
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
                                        format!("{}{} DPI", if selected { "● " } else { "" }, dpi),
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
                                control(("qa-run", index), "Run", theme, !self.running).on_click(
                                    cx.listener(move |this, _, _, cx| this.run(vec![index], cx)),
                                ),
                            )
                    })),
            )
            .when(is_ocr, |v| {
                v.child(
                    div().flex().gap_2().child("Pages").child(
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
                                        .unwrap_or_else(|| "All pages · frozen".into()),
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
                        control("run-both", "Run both", theme, !self.running)
                            .on_click(cx.listener(|this, _, _, cx| this.run(vec![0, 1], cx))),
                    )
                    .child(
                        control("cancel-comparison", "Cancel", theme, self.running).on_click(
                            cx.listener(|this, _, _, cx| {
                                if this.running {
                                    this.cancel.store(true, Ordering::Relaxed);
                                    cx.notify();
                                }
                            }),
                        ),
                    )
                    .child(
                        control(
                            "export-comparison",
                            "Export report",
                            theme,
                            !self.records.is_empty(),
                        )
                        .on_click(cx.listener(|this, _, w, cx| this.export(false, w, cx))),
                    )
                    .when(is_ocr, |v| {
                        v.child(
                            control(
                                "export-ocr-markdown",
                                "Save Markdown",
                                theme,
                                !self.records.is_empty(),
                            )
                            .on_click(cx.listener(|this, _, w, cx| this.export(true, w, cx))),
                        )
                    })
                    .child(div().flex_1())
                    .when(self.running, |v| {
                        v.child("Running · cancellation waits for the current native call")
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
                            control("show-raw-ocr", "Recognition output", theme, true).on_click(
                                cx.listener(|this, _, _, cx| {
                                    this.raw = true;
                                    this.update_display();
                                    cx.notify();
                                }),
                            ),
                        )
                        .child(
                            control("show-prepared-ocr", "Prepared Markdown", theme, true)
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
                        record
                            .output
                            .get("warnings")
                            .map(|w| w.to_string())
                            .unwrap_or_default()
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
                v.child(div().flex_1().child(
                    "Results are read-only. QA does not edit the document or change defaults.",
                ))
            })
    }
}

impl Focusable for View {
    fn focus_handle(&self, _: &App) -> FocusHandle {
        self.focus.clone()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use gpui::TestAppContext;

    #[gpui::test]
    fn comparison_results_keep_captured_config_and_ignore_cancelled_or_late_completion(
        cx: &mut TestAppContext,
    ) {
        let (owner, cx) = cx.add_window_view(tabs::Tabs::new);
        cx.run_until_parked();
        let shared = Store::new();
        let panel = cx.new(|cx| {
            settings_ui::Panel::new(shared.clone(), Rc::new(Cell::new(Theme::default())), cx)
        });
        let defaults = shared.borrow().snapshot().unwrap();
        let input = Input::Markdown(Arc::new("Alice@example.invalid".into()));
        let hash = input.hash();
        let weak_owner = owner.downgrade();
        let (view, cx) = cx.add_window_view(|_, cx| {
            View::new(
                (input.clone(), hash.clone()),
                defaults,
                Rc::new(Cell::new(Theme::default())),
                weak_owner,
                panel,
                Vec::new(),
                cx,
            )
        });
        let record = ResultRecord {
            configuration: Config::Pii(PiiConfig::default()),
            identity: Config::Pii(PiiConfig::default()).identity(),
            input_sha256: hash.clone(),
            pages: None,
            elapsed_ms: 1,
            output: serde_json::json!({"predictions":[],"count":0}),
            error: None,
        };
        shared
            .borrow_mut()
            .current
            .as_mut()
            .unwrap()
            .pseudonymization
            .model = PiiModel::Fp32;
        view.update(cx, |view, cx| {
            view.generation = 2;
            view.running = true;
            view.finish(1, vec![record.clone()], false, cx);
            assert!(view.running);
            assert!(view.records.is_empty());
            view.finish(2, vec![record.clone()], true, cx);
            assert!(!view.running);
            assert!(view.records.is_empty());
            view.finish(2, vec![record], false, cx);
            assert_eq!(
                view.records[0].configuration,
                Config::Pii(PiiConfig::default())
            );
            assert_eq!(view.records[0].input_sha256, hash);
            assert_eq!(view.input.hash(), input.hash());
            view.values[0].update(cx, |i, cx| i.set_value("NaN".into(), cx));
            view.run(vec![0], cx);
            assert!(view.error.as_ref().unwrap().contains("finite"));
            assert_eq!(view.records.len(), 1);
        });
        assert_eq!(
            shared.borrow().snapshot().unwrap().pseudonymization.model,
            PiiModel::Fp32
        );
    }
}
