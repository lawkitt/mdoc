//! What a tab carries out for its conversion: it reads Settings, starts
//! the background run and the model setup, and applies each effect the
//! conversion's state returns.
use super::ImportEffect;
use crate::document::{Background, DocumentView, Next, OcrState, SetupOcr, text};
use crate::workspace::{self, Import};
use gpui::{App, Context, Window};
use mdoc_import::{import, model_work, ocr, recognition};
use mdoc_store::settings;
use std::path::PathBuf;

impl DocumentView {
    /// The Markdown came from OCR.
    pub(crate) fn recognized(&self) -> bool {
        self.session.ocr_configuration.is_some()
    }

    /// The OCR model chosen in Settings, whose readiness `ocr_state` shares.
    pub(in crate::document) fn settings_ocr_model(&self) -> Option<settings::OcrModel> {
        self.preferences
            .borrow()
            .snapshot()
            .ok()
            .map(|p| p.ocr.model)
    }

    /// This document's recognition languages: the card's pick, else the
    /// detected default with its reason.
    pub(crate) fn recognition_choice(&self) -> recognition::Choice {
        self.import.choice(self.settings_ocr_model())
    }

    /// The bundle that reads this document's recognition languages.
    pub(crate) fn recognition_model(&self) -> settings::OcrModel {
        self.import.model(self.settings_ocr_model())
    }

    /// Pick recognition languages on the card. The pick holds until the
    /// document converts; picking the detected list again keeps its
    /// explanation, frozen so a later Settings change cannot move it.
    pub(crate) fn pick_recognition(
        &mut self,
        languages: Vec<recognition::Language>,
        cx: &mut Context<Self>,
    ) {
        if self.import.busy() || self.ocr_state.busy() {
            return;
        }
        self.import.pick(languages, self.settings_ocr_model());
        cx.notify();
    }

    /// The shared state for the Settings model from what Settings already
    /// knows. Never starts a check: that would take the model slot from a
    /// run about to resume.
    fn known_settings_ocr_state(&self, cx: &App) -> OcrState {
        known_ocr_state(&self.preferences, self.model_panel.read(cx))
    }

    /// A checked installation of `model`: the shared one for the Settings
    /// model, else one Settings checked or set up. None after a failure of
    /// that model, so Retry verifies the installation again.
    pub(in crate::document) fn installed_for(
        &self,
        model: settings::OcrModel,
        cx: &App,
    ) -> Option<ocr::Installed> {
        if self.settings_ocr_model() == Some(model) {
            match &self.ocr_state {
                OcrState::Ready(installed) if installed.config.model == model => {
                    return Some(installed.clone());
                }
                OcrState::Failed(_) => return None,
                _ => {}
            }
        } else if self.import.ocr_failure().is_some() {
            return None;
        }
        self.model_panel
            .read(cx)
            .ocr_installations
            .iter()
            .find(|i| i.config.model == model)
            .cloned()
    }

    pub(crate) fn import(&mut self, _: &Import, window: &mut Window, cx: &mut Context<Self>) {
        if !self.lifecycle.source_only()
            || self.lifecycle.prompting()
            || self.import.busy()
            || self.ocr_state.busy()
            || self.import.slot_busy()
        {
            return;
        }
        let effect = self.import.convert_by_hand(self.preview.source().cloned());
        self.carry_out_import(effect, window, cx);
    }

    /// Convert `path` as Convert would.
    #[cfg(test)]
    pub(crate) fn start_import(
        &mut self,
        path: PathBuf,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.start_import_with(path, false, false, None, window, cx);
    }

    /// `ready` is the installation a finished setup checked, with the
    /// configuration captured when it started.
    fn start_import_with(
        &mut self,
        path: PathBuf,
        skip_ocr: bool,
        allow_ocr: bool,
        ready: Option<ocr::Installed>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if allow_ocr && self.ocr_state.busy() {
            return;
        }
        // Settings' resolution and confidence, this document's model.
        let config = if allow_ocr && !skip_ocr {
            let snapshot = self.preferences.borrow().snapshot();
            match snapshot {
                Ok(p) => Some(settings::OcrConfig {
                    model: self.recognition_model(),
                    ..p.ocr
                }),
                Err(e) => {
                    self.error = Some(e.to_string());
                    cx.notify();
                    return;
                }
            }
        } else {
            None
        };
        let installed = match ready {
            Some(ready) if allow_ocr && !skip_ocr => Some(ready),
            _ => config.and_then(|config| {
                self.installed_for(config.model, cx).map(|mut installed| {
                    installed.config = config;
                    installed
                })
            }),
        };
        if allow_ocr && installed.is_none() {
            self.error = Some(text::OCR_MODEL_NOT_READY.into());
            cx.notify();
            return;
        }
        let effect = self.import.begin(path, installed, skip_ocr);
        self.carry_out_import(effect, window, cx);
    }

    pub(crate) fn resume_import(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.closing.resolving() {
            return;
        }
        let generation = self.session.generation;
        let settings_model = self.settings_ocr_model();
        let Some(effects) = self
            .import
            .resume(generation, self.lifecycle.prompting(), |import| {
                settings_model == Some(import.model(settings_model))
            })
        else {
            return;
        };
        self.carry_out_import(effects, window, cx);
        // The run's password opened the locked PDF the Original asks for,
        // though the import stopped short (OCR consent, a failed
        // recognition): the Original opens with it, asking no more.
        if self.preview.password().is_some()
            && let Some(path) = self.preview.source().cloned()
            && self.pdf_password(&path).is_some()
        {
            self.open_pdf(path, window, cx);
        }
        // A stale completion also clears the busy indicator.
        cx.notify();
    }

    fn carry_out_import(
        &mut self,
        effects: impl IntoIterator<Item = ImportEffect>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        for effect in effects {
            match effect {
                ImportEffect::Convert {
                    path,
                    skip_ocr,
                    allow_ocr,
                } => self.start_import_with(path, skip_ocr, allow_ocr, None, window, cx),
                ImportEffect::ConvertWith { path, installed } => {
                    self.start_import_with(path, false, true, Some(installed), window, cx);
                }
                ImportEffect::Run {
                    path,
                    mut unlock,
                    installed,
                    skip_ocr,
                    permit,
                    cancel,
                } => {
                    cx.emit(workspace::TabEvent::Import(workspace::ImportEvent::State));
                    self.error = None;
                    let generation = self.session.generation;
                    let task = cx.background_executor().spawn(async move {
                        // The run takes the tab's unlock (or a typed
                        // password) and hands back what opens the source
                        // now; a closed tab's drops with the task.
                        let result = import::prepare_unlocking(
                            &path,
                            &mut unlock,
                            installed.as_ref(),
                            skip_ocr,
                            &cancel,
                        );
                        drop(permit);
                        (result, unlock)
                    });
                    let task = cx.spawn_in(window, async move |this, cx| {
                        let (result, unlock) = task.await;
                        let _ = this.update_in(cx, |this, window, cx| {
                            this.import.complete(generation, result, unlock);
                            this.resume_import(window, cx);
                            cx.emit(workspace::TabEvent::Import(
                                workspace::ImportEvent::Finished,
                            ));
                        });
                        Background::Import
                    });
                    self.keep_background(task);
                    cx.notify();
                }
                ImportEffect::SetUp => self.begin_ocr_setup(window, cx),
                ImportEffect::Request(imported) => {
                    self.request(Next::Import(Box::new(imported)), window, cx);
                }
                ImportEffect::SharedOcrReady(installed) => {
                    self.ocr_state = OcrState::Ready(installed);
                }
                ImportEffect::SharedOcrFailed(error) => {
                    self.ocr_state = OcrState::Failed(error);
                }
                ImportEffect::SharedOcrKnown => {
                    self.ocr_state = self.known_settings_ocr_state(cx);
                }
                ImportEffect::SharedOcrChanged => {
                    cx.emit(workspace::TabEvent::Import(workspace::ImportEvent::Ocr(
                        self.ocr_state.clone(),
                    )));
                }
                ImportEffect::Error(error) => self.error = Some(error),
                ImportEffect::FocusCard => self.focus_card = true,
                ImportEffect::State => {
                    cx.emit(workspace::TabEvent::Import(workspace::ImportEvent::State));
                }
            }
        }
    }

    pub(in crate::document) fn setup_ocr(
        &mut self,
        _: &SetupOcr,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if self.lifecycle.prompting()
            || self.import.busy()
            || self.ocr_state.busy()
            || self.import.slot_busy()
            || !ocr::SUPPORTED
        {
            return;
        }
        if self.installed_for(self.recognition_model(), cx).is_some() {
            return;
        }
        self.begin_ocr_setup(window, cx);
    }

    fn begin_ocr_setup(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.ocr_state.busy() {
            return;
        }
        let snapshot = self.preferences.borrow().snapshot();
        let config = match snapshot {
            Ok(p) => settings::OcrConfig {
                model: self.recognition_model(),
                ..p.ocr
            },
            Err(e) => {
                self.error = Some(e.to_string());
                cx.notify();
                return;
            }
        };
        let shared = self.settings_ocr_model() == Some(config.model);
        self.import.begin_setup();
        let panel = self.model_panel.clone();
        // The inline consent continuation remains document-owned. Settings downloads never create one.
        if self.import.setting_up() {
            let config = config.clone();
            let subscription =
                cx.subscribe_in(&panel, window, move |this, _, event, window, cx| {
                    if let crate::settings::Event::Finished(settings::Model::Ocr(model), result) =
                        event
                        && *model == config.model
                        && this.import.setting_up()
                    {
                        match result {
                            Ok(Some(installed)) => {
                                let mut installed = installed.clone();
                                installed.config = config.clone();
                                this.finish_ocr_setup(Ok(installed), window, cx);
                            }
                            Ok(None) => {}
                            Err(e) => this.finish_ocr_setup(Err(e.clone()), window, cx),
                        }
                    }
                });
            self.ocr_setup_subscription = Some(subscription);
        }
        let started = panel.update(cx, |panel, cx| {
            panel.setup(settings::Model::Ocr(config.model), false, cx);
            panel.working == Some(settings::Model::Ocr(config.model))
        });
        if started {
            self.import.setup_started(config.model, shared);
            // Another model's setup shows only in this card's progress.
            if shared {
                self.ocr_state = OcrState::Installing;
                cx.emit(workspace::TabEvent::Import(workspace::ImportEvent::Ocr(
                    self.ocr_state.clone(),
                )));
            }
            cx.notify();
        } else if self.import.setting_up() {
            self.finish_ocr_setup(Err(text::ANOTHER_MODEL_JOB.into()), window, cx);
        }
    }

    pub(crate) fn finish_ocr_setup(
        &mut self,
        result: Result<ocr::Installed, String>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let effects =
            self.import
                .finish_setup(result, self.session.generation, self.settings_ocr_model());
        self.carry_out_import(effects, window, cx);
        cx.notify();
    }

    pub(crate) fn ocr_action(&mut self, skip: bool, window: &mut Window, cx: &mut Context<Self>) {
        // Recognizing freezes the languages the card shows, so this is the
        // model it will use.
        let installed = self.installed_for(self.recognition_model(), cx).is_some();
        let model_busy = model_work::busy() || self.model_panel.read(cx).working.is_some();
        let effect = self.import.consent(
            skip,
            self.ocr_state.busy(),
            self.settings_ocr_model(),
            self.session.generation,
            installed,
            model_busy,
        );
        let error = matches!(effect, Some(ImportEffect::Error(_)));
        self.carry_out_import(effect, window, cx);
        if error {
            cx.notify();
        }
    }

    pub(crate) fn try_auto_convert(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let effect = self.import.auto_convert(self.preview.source().cloned());
        self.carry_out_import(effect, window, cx);
    }

    /// Unlock on the conversion card: the field's text goes to one
    /// conversion as the document's password and is wiped from the field;
    /// what it unlocks stays with the tab.
    pub(super) fn unlock_document(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if !self.import.can_unlock() || self.lifecycle.prompting() {
            return;
        }
        let password = self
            .password_field
            .update(cx, |field, cx| field.take_secret(cx));
        if password.is_empty() {
            return;
        }
        let effect = self.import.try_password(import::Password::from(password));
        self.carry_out_import(effect, window, cx);
    }

    /// The last conversion ended because Cancel was pressed (#950): a state
    /// the user chose, so the tab shows no error mark for it.
    pub(crate) fn conversion_cancelled(&self) -> bool {
        self.error.as_deref() == Some(&*mdoc_import::import::ImportError::Cancelled.to_string())
    }
}

/// The shared OCR state for the Settings model from what Settings already
/// knows, without starting a check.
pub(crate) fn known_ocr_state(
    preferences: &settings::Shared,
    panel: &crate::settings::Panel,
) -> OcrState {
    let Ok(config) = preferences.borrow().snapshot().map(|p| p.ocr) else {
        return OcrState::Missing;
    };
    if let Some(mut installed) = panel
        .ocr_installations
        .iter()
        .find(|i| i.config.model == config.model)
        .cloned()
    {
        installed.config = config;
        return OcrState::Ready(installed);
    }
    match &panel.statuses[crate::settings::Panel::index(settings::Model::Ocr(config.model))] {
        crate::settings::Status::Damaged(e) | crate::settings::Status::Unavailable(e) => {
            OcrState::Failed(e.clone())
        }
        _ => OcrState::Missing,
    }
}
