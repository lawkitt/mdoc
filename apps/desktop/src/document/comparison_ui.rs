//! The Model comparison window, opened from Settings: the chosen models run one after
//! another on one captured input, and their results are shown and exported.
use crate::document::text;
use crate::{
    document::comparison::{Config, Input, ResultRecord},
    document::markdown_search,
    ui::style::Theme,
    workspace,
};
use gpui::{App, Context, Entity, FocusHandle, Focusable, actions, prelude::*};
use mdoc_store::settings::{OcrConfig, OcrModel, PiiConfig, PiiModel, Preferences};
use std::{
    cell::Cell,
    path::PathBuf,
    rc::Rc,
    sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
    },
};

mod actions;
mod view;

actions!(model_comparison, [CloseComparison]);

pub fn bind_keys(cx: &mut App) {
    let bindings =
        crate::commands::shortcuts::bindings(crate::commands::shortcuts::Owner::Comparison, cx);
    cx.bind_keys(bindings);
}

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
    owner: gpui::WeakEntity<workspace::Workspace>,
    panel: Entity<crate::settings::Panel>,
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
        owner: gpui::WeakEntity<workspace::Workspace>,
        panel: Entity<crate::settings::Panel>,
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
                        .with_theme(theme.clone())
                        .with_key_context("SettingsInput")
                        .with_placeholder(text::UNIT_INTERVAL);
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
                    .with_theme(theme.clone())
                    .with_key_context("SettingsInput")
                    .with_placeholder(text::ALL_PAGES)
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
    use mdoc_store::settings::Store;

    #[gpui::test]
    fn comparison_results_keep_captured_config_and_ignore_cancelled_or_late_completion(
        cx: &mut TestAppContext,
    ) {
        let (owner, cx) = cx.add_window_view(workspace::Workspace::new);
        cx.run_until_parked();
        let shared = Store::in_memory();
        let panel = cx.new(|cx| {
            crate::settings::Panel::new(shared.clone(), Rc::new(Cell::new(Theme::default())), cx)
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
