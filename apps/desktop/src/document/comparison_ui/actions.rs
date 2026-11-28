//! What the Model comparison window carries out: the chosen models run one
//! after another on the captured input, and the report or the prepared
//! Markdown is exported to a file the user picks.
use super::View;
use crate::document::comparison::{self, Config, ResultRecord};
use crate::document::text;
use gpui::{Context, Window};
use mdoc_import::model_work;
use mdoc_store::{
    document,
    settings::{self, Model},
};
use std::{
    path::PathBuf,
    sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
    },
};

impl View {
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
                    .map_err(|_| text::THRESHOLD_NOT_A_NUMBER.to_string())?;
                if !settings::unit_interval(value) {
                    return Err(text::THRESHOLD_OUT_OF_RANGE.into());
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
                self.error = Some(e.to_string());
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
                crate::settings::Status::Ready
            ) {
                self.error = Some(text::model_not_ready(model.name()));
                cx.notify();
                return;
            }
        }
        let permit = match model_work::Permit::acquire_for("model comparison") {
            Ok(p) => p,
            Err(e) => {
                self.error = Some(e.to_string());
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
    pub(super) fn finish(
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
            self.error = Some(text::PARTIAL_RESULTS_DISCARDED.into());
        } else {
            self.records.extend(results);
            self.selected = self.records.len().saturating_sub(1);
            self.update_display();
        }
        cx.notify();
    }
    pub(super) fn update_display(&mut self) {
        self.display = self
            .records
            .get(self.selected)
            .map(|r| r.text(self.raw))
            .unwrap_or_default()
            .into();
    }
    pub(super) fn export(&mut self, markdown: bool, window: &mut Window, cx: &mut Context<Self>) {
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
        let prompt = crate::app::native_picker::new_path(
            cx,
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
            let Ok(Some(path)) = prompt.await else {
                return;
            };
            let task = cx.background_executor().spawn(async move {
                if comparison::export_path_is_source(&path, &sources) {
                    return Err(text::PRESERVE_DOCUMENT_AND_SOURCE.into());
                }
                if let Some(text) = text {
                    document::export(&path, &text).map_err(|e| e.to_string())
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
