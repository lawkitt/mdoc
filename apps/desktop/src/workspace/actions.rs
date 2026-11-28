//! The window's actions: its documents' events carried out, the theme, Settings
//! and the model comparison's window.
use super::{
    DocxEvent, ImportEvent, ProjectEvent, ReplyEvent, SaveEvent, TabEvent, TabsEvent, Workspace,
    saving::docx_queue::DocxJob, text,
};
use crate::document::{OcrState, comparison, comparison_ui};
use crate::ui::style;
use gpui::{
    App, Bounds, Context, Focusable, Window, WindowBounds, WindowOptions, prelude::*, px, size,
};
use mdoc_import::{model_work, ocr};
use mdoc_store::session_store::ThemeChoice;
use mdoc_store::settings;
use std::sync::Arc;

impl Workspace {
    pub(super) fn refresh_model_settings(&mut self, cx: &mut Context<Self>) {
        let config = match self.preferences.borrow().snapshot() {
            Ok(p) => p.ocr,
            Err(e) => {
                self.notices.post(e.to_string());
                cx.notify();
                return;
            }
        };
        let model = settings::Model::Ocr(config.model);
        let status = self.settings.read(cx).statuses[settings::Model::ALL
            .iter()
            .position(|m| *m == model)
            .unwrap()]
        .clone();
        match status {
            crate::settings::Status::Missing => {
                self.set_ocr(OcrState::Missing, cx);
                return;
            }
            crate::settings::Status::Damaged(e) | crate::settings::Status::Unavailable(e) => {
                self.set_ocr(
                    if ocr::SUPPORTED {
                        OcrState::Failed(e)
                    } else {
                        OcrState::Unsupported
                    },
                    cx,
                );
                return;
            }
            _ => {}
        }
        // Parameter-only changes reuse the already checked model; setup publishes
        // its checked paths directly, so it cannot race a consent continuation.
        if let OcrState::Ready(installed) = &self.ocr_state
            && installed.config.model == config.model
        {
            let mut installed = installed.clone();
            installed.config = config;
            self.set_ocr(OcrState::Ready(installed), cx);
            return;
        }
        let cached = self
            .settings
            .read(cx)
            .ocr_installations
            .iter()
            .find(|i| i.config.model == config.model)
            .cloned();
        if let Some(mut installed) = cached {
            installed.config = config;
            self.set_ocr(OcrState::Ready(installed), cx);
            return;
        }
        if model_work::busy() {
            return;
        }
        let captured = config.clone();
        let task = cx
            .background_executor()
            .spawn(async move { check_ocr(&config) });
        cx.spawn(async move |this, cx| {
            let result = task.await;
            let _ = this.update(cx, |this, cx| {
                if this
                    .preferences
                    .borrow()
                    .snapshot()
                    .is_ok_and(|p| p.ocr != captured)
                {
                    return;
                }
                let state = match result {
                    Ok(Some(i)) => OcrState::Ready(i),
                    Ok(None) if ocr::SUPPORTED => OcrState::Missing,
                    Ok(None) => OcrState::Unsupported,
                    Err(e) => OcrState::Failed(e),
                };
                this.set_ocr(state, cx);
            });
        })
        .detach();
    }
    pub(crate) fn show_settings(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        window.activate_window();
        self.settings.update(cx, |panel, cx| panel.show(window, cx));
    }
    pub(crate) fn open_comparison(
        &mut self,
        model: settings::Model,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let Some(view) = self.active_view() else {
            self.notices.post(text::OPEN_BEFORE_COMPARING);
            cx.notify();
            return;
        };
        let defaults = match self.preferences.borrow().snapshot() {
            Ok(p) => p,
            Err(e) => {
                self.notices.post(e.to_string());
                cx.notify();
                return;
            }
        };
        let mut protected: Vec<_> = [
            view.read(cx).session.source.clone(),
            view.read(cx).preview.attachment().cloned(),
        ]
        .into_iter()
        .flatten()
        .collect();
        let (source, input) = match model {
            settings::Model::Ocr(_) => {
                let source = view
                    .read(cx)
                    .preview
                    .attachment()
                    .cloned()
                    .or_else(|| view.read(cx).session.source.clone());
                let Some(source) = source else {
                    self.notices.post(text::OCR_COMPARISON_NEEDS_PDF);
                    cx.notify();
                    return;
                };
                (Some(source), None)
            }
            settings::Model::Pii(_) => {
                if !view.read(cx).can_copy_markdown() {
                    self.notices.post(text::CONVERT_BEFORE_COMPARING);
                    cx.notify();
                    return;
                }
                (
                    None,
                    Some(comparison::Input::Markdown(Arc::new(
                        view.read(cx).editor.read(cx).text().to_string(),
                    ))),
                )
            }
        };
        self.settings
            .update(cx, |panel, cx| panel.close(window, cx));
        let owner = cx.entity().downgrade();
        let theme = self.theme.clone();
        let panel = self.settings.clone();
        let source_for_worker = source.clone();
        if let Some(source) = source {
            protected.insert(0, source);
        }
        let task = cx.background_executor().spawn(async move {
            let input = match input {
                Some(i) => i,
                None => comparison::Input::pdf(source_for_worker.as_ref().unwrap())?,
            };
            let hash = input.hash();
            Ok::<_, String>((input, hash))
        });
        let bounds = Bounds::centered(None, size(px(1000.), px(750.)), cx);
        cx.spawn(async move |this, cx| match task.await {
            Ok((input, hash)) => {
                if this.upgrade().is_none() {
                    return;
                }
                let _ = cx.open_window(
                    WindowOptions {
                        window_bounds: Some(WindowBounds::Windowed(bounds)),
                        ..Default::default()
                    },
                    move |window, cx| {
                        window.set_window_title(if matches!(input, comparison::Input::Pdf(_)) {
                            text::COMPARE_OCR_TITLE
                        } else {
                            text::COMPARE_PSEUDONYMIZATION_TITLE
                        });
                        let view = cx.new(|cx| {
                            comparison_ui::View::new(
                                (input, hash),
                                defaults,
                                theme,
                                owner,
                                panel,
                                protected,
                                cx,
                            )
                        });
                        window.focus(&view.read(cx).focus_handle(cx), cx);
                        view
                    },
                );
            }
            Err(e) => {
                let _ = this.update(cx, |this, cx| {
                    this.notices.post(e);
                    cx.notify();
                });
            }
        })
        .detach();
    }
    pub(super) fn event(
        &mut self,
        id: u64,
        event: &TabEvent,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if !self.tabs.contains(id) {
            return;
        }
        match event {
            TabEvent::Tabs(TabsEvent::New) if !self.quit_flow.quitting() => {
                if !self.intake_refused(window, cx) {
                    self.new_tab(window, cx);
                }
            }
            TabEvent::Tabs(TabsEvent::Open(paths)) => self.open_paths(paths.clone(), window, cx),
            TabEvent::Reply(event) if !self.quit_flow.quitting() => match event {
                ReplyEvent::Settled { open } => self.queue_box_save(id, *open, window, cx),
                ReplyEvent::Pasted(text) => self.queue_pasted_reply(id, text.clone(), window, cx),
                ReplyEvent::Open(reply) => self.open_reply(reply, window, cx),
            },
            TabEvent::Import(ImportEvent::State) => {
                self.refresh_import(window, cx);
                // A converted document of a Project gets its names now.
                self.attach_project(id, cx);
            }
            TabEvent::Import(ImportEvent::Ocr(state)) => self.set_ocr(state.clone(), cx),
            TabEvent::Project(ProjectEvent::Changed) => self.project_changed(id, cx),
            TabEvent::Project(ProjectEvent::OpenPage) => {
                let project = (self.tab_view(id))
                    .and_then(|view| view.read(cx).project.link().map(|link| link.id.clone()));
                if let Some(project) = project {
                    self.open_project(&project, window, cx);
                }
            }
            TabEvent::Project(ProjectEvent::MappingMissing) => {
                if self.vault_banner().is_some() {
                    self.carry_out_notices(self.notices.show_locked_projects());
                    cx.notify();
                }
            }
            TabEvent::Project(ProjectEvent::AddTerm(term)) => self.add_project_term(id, term, cx),
            TabEvent::Project(ProjectEvent::DismissHint(from, into)) => {
                self.dismiss_project_hint(id, from, into, cx)
            }
            TabEvent::Project(ProjectEvent::MoveToNew) => self.move_to_new_project(id, window, cx),
            TabEvent::Project(ProjectEvent::FinishRename { save }) => {
                self.finish_rename(*save, window, cx)
            }
            TabEvent::Project(ProjectEvent::MoveMenu(at)) => {
                self.open_move_menu(id, *at, window, cx)
            }
            TabEvent::CopyForAi => self.copy_tab_for_ai(id, window, cx),
            TabEvent::Ask {
                with,
                move_to_new,
                into,
            } => {
                let into = into.clone();
                self.ask_from_tab(id, *with, *move_to_new, into, window, cx)
            }
            TabEvent::NamesFound => self.copy_for_ai_after_write(cx),
            TabEvent::CopyStep(step) => {
                self.run_tab_step(id, *step, window, cx);
            }
            TabEvent::ToggleTheme => self.cycle_theme(window, cx),
            TabEvent::SourceDisplay(display) => {
                let display = *display;
                // Kept for this window when the save fails; the error is logged.
                let _ = self.update_general(
                    move |general| {
                        general.show_comments = display.comments;
                        general.show_hidden_text = display.hidden_text;
                    },
                    cx,
                );
                let views: Vec<_> = self
                    .tabs
                    .iter()
                    .filter_map(|tab| tab.view().cloned())
                    .collect();
                for view in views {
                    view.update(cx, |view, cx| view.source_display_changed(window, cx));
                }
            }
            TabEvent::Save(SaveEvent::Changed) => self.autosave(id, window, cx),
            TabEvent::Import(ImportEvent::Finished) => {
                self.refresh_import(window, cx);
                // A converted member of the AI project has text to scan now.
                self.scan_on_entry(cx);
            }
            TabEvent::Docx(DocxEvent::Finished { generation }) => {
                self.docx_finished(id, *generation, window, cx);
            }
            TabEvent::Docx(DocxEvent::Queue {
                path,
                generation,
                cancel,
            }) => {
                self.docx_queue.push(DocxJob {
                    id,
                    path: path.clone(),
                    generation: *generation,
                    cancel: cancel.clone(),
                });
                self.pump_docx(window, cx);
            }
            _ => {}
        }
        cx.notify();
    }
}

impl Workspace {
    // ---- Theme ------------------------------------------------------------

    /// Resolve the theme choice against the OS appearance and repaint every
    /// open document when the result changes.
    pub(super) fn apply_theme(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let theme = style::resolve(self.theme_choice, window.appearance());
        self.settings
            .update(cx, |panel, cx| panel.sync_theme(self.theme_choice, cx));
        if theme == self.theme.get() {
            return;
        }
        self.theme.set(theme);
        for tab in &self.tabs {
            if let Some(view) = tab.view() {
                view.update(cx, |view, cx| {
                    view.editor.update(cx, |editor, cx| {
                        editor.set_markdown_style(style::markdown_style(theme), cx)
                    });
                    view.sync_pii_theme(cx);
                    cx.notify();
                });
            }
        }
        cx.notify();
    }
    /// macOS: the Save panel, its alerts and the menus follow the choice
    /// too. Called when the choice changes, not from the
    /// appearance observer: setting the appearance notifies it.
    pub(super) fn sync_app_appearance(&self, cx: &App) {
        #[cfg(target_os = "macos")]
        cx.set_window_appearance(style::app_appearance(self.theme_choice));
        #[cfg(not(target_os = "macos"))]
        let _ = cx;
    }
    pub(super) fn cycle_theme(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.select_theme(self.theme_choice.next(), window, cx);
    }
    pub(super) fn select_theme(
        &mut self,
        choice: ThemeChoice,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.theme_choice = choice;
        self.sync_app_appearance(cx);
        self.apply_theme(window, cx);
        self.checkpoint(window, cx);
        cx.notify();
    }
}

impl Workspace {
    /// What the notices count from the Vault: whether it is locked or
    /// unavailable, and whether that opens the bell. A Vault sealed with
    /// another key doesn't: Home asks for its recovery key.
    pub(super) fn vault_notice(&self) -> (bool, bool) {
        let locked = self.vault_banner().is_some();
        (locked, locked && self.vault.wrong_key_root().is_none())
    }
}

/// The background OCR install check. Tests never read the real profile's
/// installed models.
pub(super) fn check_ocr(
    config: &mdoc_store::settings::OcrConfig,
) -> Result<Option<mdoc_import::ocr::Installed>, String> {
    if cfg!(test) {
        Ok(None)
    } else {
        ocr::check_config(config).map_err(|e| e.to_string())
    }
}
