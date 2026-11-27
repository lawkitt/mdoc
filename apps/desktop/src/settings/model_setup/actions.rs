//! Checking, downloading, repairing and removing the models, off the main
//! thread, and what each check and setup leaves on the page.
use crate::app::models;
use crate::settings::{Event, Panel, Status, text};
use gpui::{Context, PromptLevel, Window};
use mdoc_import::{model_download::Progress, model_work, ocr};
use mdoc_store::model_state::{Files, ModelState};
use mdoc_store::settings::{Model, OcrConfig, PiiConfig};
use std::{sync::atomic::Ordering, time::Duration};

pub(in crate::settings) struct SetupCompletion {
    pub(in crate::settings) result: Result<Option<ocr::Installed>, String>,
    pub(in crate::settings) status: Status,
    model_state: ModelState,
}

impl SetupCompletion {
    /// Runs on the setup worker, while it still owns the model permit.
    pub(in crate::settings) fn remember(
        model: Model,
        remove: bool,
        progress: &Progress,
        mut result: Result<Option<ocr::Installed>, String>,
        mut model_state: ModelState,
        checked_files: Option<Files>,
        paths: &[std::path::PathBuf],
    ) -> Self {
        model_state.reload();
        let status = match &result {
            Ok(_) if remove => Status::Missing,
            Ok(_) => Status::Ready,
            Err(_) if progress.cancel.load(Ordering::Relaxed) => Status::Unknown,
            Err(e) if progress.state.lock().unwrap().phase == text::CHECKING_RUNTIME => {
                Status::Unavailable(e.clone())
            }
            Err(e) => Status::Damaged(e.clone()),
        };
        let after = Files::read(paths);
        let status = if result.is_ok() && !remove && checked_files.is_none() {
            Status::Unknown
        } else {
            status
        };
        let before = checked_files.unwrap_or_else(|| after.clone());
        let status = model_state.record(model, &before, after, status);
        if result.is_ok() && !remove && status != Status::Ready {
            result = Err(text::MODEL_FILES_CHANGED.into());
        }
        if let Err(error) = model_state.save() {
            eprintln!("{}", text::setup_not_remembered(error));
        }
        Self {
            result,
            status,
            model_state,
        }
    }
}

impl Panel {
    /// Details and Less: a model's details show or go.
    pub(in crate::settings) fn toggle_details(&mut self, model: Model, cx: &mut Context<Self>) {
        self.details = if self.details == Some(model) {
            None
        } else {
            Some(model)
        };
        cx.notify();
    }

    /// Remove…: ask first, then remove the model's files.
    pub(in crate::settings) fn ask_remove_model(
        &mut self,
        model: Model,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let prompt = window.prompt(
            PromptLevel::Warning,
            &text::remove_model_files(model.name()),
            Some(text::REMOVE_MODEL_KEEPS),
            &[text::REMOVE, crate::ui::text::CANCEL],
            cx,
        );
        cx.spawn(async move |this, cx| {
            if prompt.await.ok() == Some(0) {
                let _ = this.update(cx, |this, cx| this.setup(model, true, cx));
            }
        })
        .detach();
    }

    /// Render a running setup without network or model access in GPUI tests.
    #[cfg(test)]
    pub(crate) fn simulate_setup(&mut self, model: Model, cx: &mut Context<Self>) -> Progress {
        let progress = Progress::default();
        progress.phase("Downloading test model");
        self.working = Some(model);
        self.progress = Some(progress.clone());
        cx.notify();
        progress
    }

    pub fn cancel_setup(&mut self, cx: &mut Context<Self>) {
        if let Some(p) = &self.progress {
            p.cancel.store(true, Ordering::Relaxed);
        }
        cx.notify();
    }

    /// Select a model and save the choice at once.
    pub fn select(&mut self, model: Model, cx: &mut Context<Self>) {
        if matches!(self.statuses[Self::index(model)], Status::Unavailable(_)) {
            return;
        }
        match model {
            Model::Ocr(m) => self.draft.ocr.model = m,
            Model::Pii(m) => self.draft.pseudonymization.model = m,
        }
        self.persist(cx);
    }

    pub fn refresh(&mut self, cx: &mut Context<Self>) {
        self.model_state.reload();
        let paths = self.model_files();
        if self.working.is_none() {
            self.row_statuses = Some(
                Model::ALL
                    .into_iter()
                    .enumerate()
                    .map(|(index, model)| {
                        let files = Files::read(&paths[index]);
                        self.model_state
                            .status(model, &files)
                            .unwrap_or_else(|| self.statuses[index].clone())
                    })
                    .collect(),
            );
        }
        // Unit checks substitute the engines below and isolate admission too:
        // concurrent test windows must not take each other's process-wide slot.
        #[cfg(test)]
        let busy = self.check_blocked;
        #[cfg(not(test))]
        let busy = model_work::busy();
        if self.working.is_some() || busy {
            self.check_pending = true;
            return;
        }
        #[cfg(test)]
        let permit: Option<model_work::Permit> = None;
        #[cfg(not(test))]
        let permit = match model_work::Permit::acquire_for(text::CHECKING_INSTALLED_MODELS) {
            Ok(permit) => Some(permit),
            Err(_) => {
                self.check_pending = true;
                return;
            }
        };
        self.checks += 1;
        self.check_pending = false;
        self.checking = true;
        let generation = self.checks;
        let mut model_state = self.model_state.clone();
        let task = cx.background_executor().spawn(async move {
            let _permit = permit;
            let mut installed = Vec::new();
            let pending = Model::ALL.map(models::pending).to_vec();
            let statuses = Model::ALL
                .into_iter()
                .enumerate()
                .map(|(index, model)| {
                    let paths = &paths[index];
                    // An install from before #905 converts before its files
                    // are read, so the check that follows can remember it.
                    if let Model::Pii(m) = model
                        && !cfg!(test)
                    {
                        mdoc_detect::upgrade_reserved(m);
                    }
                    let before = Files::read(paths);
                    let result = match model {
                        // Tests never read the real profile's installed models.
                        Model::Ocr(_) if cfg!(test) => Ok(false),
                        Model::Ocr(m) => ocr::check_reserved(&OcrConfig {
                            model: m,
                            ..Default::default()
                        })
                        .map(|v| {
                            let ready = v.is_some();
                            if let Some(v) = v {
                                installed.push(v);
                            }
                            ready
                        })
                        .map_err(|e| e.to_string()),
                        Model::Pii(_) if cfg!(test) => Ok(false),
                        Model::Pii(m) => mdoc_detect::check_reserved(m).map_err(|e| e.to_string()),
                    };
                    let status = models::check_status(result);
                    model_state.record(model, &before, Files::read(paths), status)
                })
                .collect::<Vec<_>>();
            if let Err(error) = model_state.save() {
                eprintln!("{}", text::checks_not_remembered(error));
            }
            (statuses, installed, pending, model_state)
        });
        cx.spawn(async move |this, cx| {
            let (mut statuses, mut installed, pending, model_state) = task.await;
            let _ = this.update(cx, |this, cx| {
                if this.checks == generation && this.working.is_none() {
                    // A different model's runtime check may take time. Recheck
                    // metadata at publication too, so an earlier result cannot
                    // label a file removed while the rest of the catalog ran.
                    let paths = this.model_files();
                    for (index, model) in Model::ALL.into_iter().enumerate() {
                        statuses[index] = model_state
                            .status(model, &Files::read(&paths[index]))
                            .unwrap_or(Status::Unknown);
                    }
                    installed.retain(|i| {
                        statuses[Self::index(Model::Ocr(i.config.model))] == Status::Ready
                    });
                    this.checking = false;
                    if this.statuses != statuses {
                        this.statuses = statuses;
                    }
                    this.row_statuses = None;
                    this.ocr_installations = installed;
                    this.pending = pending;
                    this.model_state = model_state;
                    cx.emit(Event::Checked);
                    cx.notify();
                }
            });
        })
        .detach();
    }

    pub fn setup(&mut self, model: Model, remove: bool, cx: &mut Context<Self>) {
        if self.working.is_some() || model_work::busy() {
            self.error = Some(text::ANOTHER_MODEL_JOB.into());
            cx.notify();
            return;
        }
        let permit = match model_work::Permit::acquire_for(models::setup_label(model, remove)) {
            Ok(permit) => permit,
            Err(error) => {
                self.error = Some(error.to_string());
                cx.notify();
                return;
            }
        };
        self.checks += 1;
        self.checking = false;
        self.working = Some(model);
        self.error = None;
        let progress = Progress::default();
        progress.state.lock().unwrap().planned = if remove {
            0
        } else {
            self.pending[Self::index(model)].total()
        };
        self.progress = Some(progress.clone());
        let model_state = self.model_state.clone();
        let paths = self.model_files()[Self::index(model)].clone();
        let task = cx.background_executor().spawn({
            let progress = progress.clone();
            async move {
                let _permit = permit;
                let result = if remove {
                    match model {
                        Model::Ocr(m) => ocr::remove_reserved(m).map_err(|e| e.to_string()),
                        Model::Pii(m) => mdoc_detect::remove_reserved(m).map_err(|e| e.to_string()),
                    }
                    .map(|_| (None, None))
                } else {
                    match model {
                        Model::Ocr(m) => ocr::install_reserved(
                            &OcrConfig {
                                model: m,
                                ..Default::default()
                            },
                            &progress,
                        )
                        .map(|(installed, files)| (Some(installed), Some(files)))
                        .map_err(|e| e.to_string()),
                        Model::Pii(m) => mdoc_detect::setup_reserved(
                            &PiiConfig {
                                model: m,
                                ..Default::default()
                            },
                            &progress,
                        )
                        .map(|files| (None, Some(files)))
                        .map_err(|e| e.to_string()),
                    }
                };
                let (result, checked_files) = match result {
                    Ok((installed, files)) => (Ok(installed), files),
                    Err(error) => (Err(error), None),
                };
                SetupCompletion::remember(
                    model,
                    remove,
                    &progress,
                    result,
                    model_state,
                    checked_files,
                    &paths,
                )
            }
        });
        cx.spawn(async move |this, cx| {
            let completion = task.await;
            let _ = this.update(cx, |this, cx| {
                this.finish_setup(model, completion, cx);
            });
        })
        .detach();
        cx.spawn(async move |this, cx| {
            loop {
                cx.background_executor()
                    .timer(Duration::from_millis(250))
                    .await;
                if this
                    .update(cx, |this, cx| {
                        if this.working.is_some() {
                            cx.notify();
                            true
                        } else {
                            false
                        }
                    })
                    .unwrap_or(false)
                {
                    continue;
                }
                break;
            }
        })
        .detach();
        cx.notify();
    }

    pub(in crate::settings) fn finish_setup(
        &mut self,
        model: Model,
        completion: SetupCompletion,
        cx: &mut Context<Self>,
    ) {
        let SetupCompletion {
            mut result,
            mut status,
            model_state,
        } = completion;
        let index = Self::index(model);
        if status == Status::Ready
            && model_state.status(model, &Files::read(&self.model_files()[index]))
                != Some(Status::Ready)
        {
            status = Status::Unknown;
            result = Err(text::MODEL_FILES_CHANGED.into());
        }
        // Runtimes are shared, so any setup can change every model's size.
        self.pending = Model::ALL.map(models::pending).to_vec();
        self.statuses[index] = status.clone();
        if let Some(rows) = &mut self.row_statuses {
            rows[index] = status;
        }
        self.model_state = model_state;
        if let Model::Ocr(m) = model {
            self.ocr_installations.retain(|i| i.config.model != m);
            if let Ok(Some(i)) = &result {
                self.ocr_installations.push(i.clone());
            }
        }
        cx.emit(Event::Finished(model, result.clone()));
        self.error = result.err();
        self.working = None;
        self.progress = None;
        cx.notify();
    }
}
