//! The document view: one tab's Markdown editor, source preview, search,
//! import/OCR state and pseudonymization review. The window-level
//! [`crate::workspace::Workspace`] owns these views.
use crate::{
    images, markdown_search, pii,
    pii::ui::{PiiAddCandidate, PiiNextCandidate, PiiPreviousCandidate, PiiReviewCandidate},
    preview::{self, RetryPreview},
    search_session::{
        self, CloseMarkdownSearch, FindNextMarkdown, FindPreviousMarkdown, ReplaceAllMarkdown,
        ReplaceNextMarkdown, SearchRefresh, ToggleMarkdownMatchCase, ToggleMarkdownWholeWord,
    },
    settings_ui, spelling,
    style::{self, Theme},
    ui,
    workspace::{self, Close, Import, New, Open, RetryDocument, ToggleTheme},
};
use gpui::{
    App, Context, Entity, Focusable, PathPromptOptions, PromptLevel, ScrollHandle, Subscription,
    Window, actions, div, prelude::*, px,
};
use mdoc_editor::{EditorEvent, EditorState};
use mdoc_import::{import, model_work, ocr, recognition};
use mdoc_store::{
    document::{self, Document},
    document_session, session_store, settings,
};
use std::{
    cell::Cell,
    path::PathBuf,
    rc::Rc,
    sync::{Arc, atomic::AtomicBool},
};

mod case;
mod case_link;
use case_link::CaseLinkEffect;
mod chrome;
mod document_lifecycle;
use document_lifecycle::LifecycleEffect;
mod import_flow;
use import_flow::ImportEffect;
#[cfg(test)]
pub(crate) mod tests;
actions!(
    mdoc,
    [
        SetupOcr,
        RunOcr,
        ExtractNative,
        DismissImportWarning,
        Save,
        SaveAs,
        CopyMarkdown,
        ToggleCompare,
        ZoomIn,
        ZoomOut,
        ZoomReset,
        ClosePdf,
        TogglePreview,
    ]
);

#[derive(Clone)]
pub(crate) enum Next {
    Import(import::Imported),
    Close,
    /// Saved first so the new case lists the document (#75); `close` closes
    /// the tab once the case is written.
    CreateCase {
        close: bool,
    },
}

#[derive(Clone, Debug)]
pub(crate) enum OcrState {
    Checking,
    Missing,
    Installing,
    Ready(ocr::Installed),
    Failed(String),
    Unsupported,
}

impl OcrState {
    pub(crate) fn busy(&self) -> bool {
        matches!(self, Self::Checking | Self::Installing)
    }
}

pub(crate) struct DocumentView {
    pub(crate) focus: gpui::FocusHandle,
    /// F6 regions of the document (ADR 0062); the editor and the
    /// replacements panel bring their own handles.
    pub(crate) toolbar_region: gpui::FocusHandle,
    /// Controls outside the toolbar that belong to its region: the notices
    /// strip with the pane switch, and the view switcher (ADR 0062).
    pub(crate) controls_region: gpui::FocusHandle,
    pub(crate) switcher_region: gpui::FocusHandle,
    /// The find bar, between the toolbar and the editor in Tab order.
    pub(crate) find_region: gpui::FocusHandle,
    pub(crate) preview_region: gpui::FocusHandle,
    pub(crate) comments_region: gpui::FocusHandle,
    /// The toolbar's More button, which its menu returns focus to.
    pub(crate) more_focus: gpui::FocusHandle,
    /// The open More menu, which holds focus and Tab (ADR 0062).
    pub(crate) more_menu_focus: gpui::FocusHandle,
    /// The More button's bounds, so a press on it is left to its toggle.
    more_bounds: std::rc::Rc<std::cell::Cell<gpui::Bounds<gpui::Pixels>>>,
    /// Whether the last frame drew the Markdown and the original panes: a
    /// narrow window shows one of them (ADR 0062 skips the hidden one).
    panes_drawn: std::cell::Cell<(bool, bool)>,
    /// Whether the window is resolving a close or quit (read-only).
    closing: workspace::CloseWatch,
    /// Imports and DOCX jobs still running, handed to the window if the tab
    /// closes first so it still learns when they end.
    background: Vec<gpui::Task<Background>>,
    pub(crate) active: bool,
    /// Loading, availability, conversion, unsaved changes and dialogs.
    pub(crate) lifecycle: document_lifecycle::DocumentLifecycle,
    /// External files over the empty page: whether any is supported (ADR 0028).
    pub(crate) file_drag: Option<bool>,
    pub(crate) preferences: settings::Shared,
    pub(crate) model_panel: Entity<settings_ui::Panel>,
    pub(crate) theme: Rc<Cell<Theme>>,
    pub(crate) title: style::TitleBar,
    pub(crate) editor: Entity<EditorState>,
    pub(crate) images: images::ImageCache,
    pub(crate) session: document_session::DocumentSession,
    pub(crate) pii: pii::ui::ReviewUi,
    pub(crate) spelling: spelling::SpellState,
    pub(crate) preview: preview::PreviewState,
    pub(crate) scroll: ScrollHandle,
    pub(crate) error: Option<String>,
    pub(crate) copy_feedback: Option<gpui::Task<()>>,
    /// The header's ⋯ menu (ADR 0048).
    pub(crate) more_open: bool,
    /// Text zoom of this document's Markdown, not saved (ADR 0048).
    pub(crate) zoom: f32,
    /// Inspector (ADR 0048 variant C): the Share tab is showing.
    pub(crate) inspector_share: bool,
    /// The running copy feedback belongs to Copy for AI, not Copy Markdown.
    pub(crate) copied_for_ai: bool,
    /// The running copy feedback belongs to Copy reply (ADR 0078).
    pub(crate) copied_reply: bool,
    /// The tab's AI reply and its restoration (ADR 0078).
    pub(crate) reply: pii::ui::ReplyBox,
    /// The Share panel's reply box, made when Share first draws.
    pub(crate) reply_box: Option<(Entity<EditorState>, Subscription)>,
    /// The saved case this tab's mapping belongs to (ADR 0041).
    pub(crate) case: case_link::CaseLink,
    /// The waiting save of the mapping into the case.
    case_save: Option<gpui::Task<()>>,
    pub(crate) case_names_open: bool,
    pub(crate) case_term: Option<Entity<markdown_search::SearchInput>>,
    /// The result of Pseudonymize Clipboard, until dismissed.
    pub(crate) clipboard_notice: Option<String>,
    /// The before/after view replaces the editor while open.
    pub(crate) compare: Option<pii::ui::CompareState>,
    /// The floating view pill fades when idle (README media decision 9).
    pub(crate) switcher: pii::ui::SwitcherRest,
    pub(crate) view_bounds: Rc<Cell<gpui::Bounds<gpui::Pixels>>>,
    pub(crate) split_dragging: bool,
    pub(crate) original_selected: bool,
    /// The card's primary action takes focus once when OCR consent appears.
    /// Stable focus for a setup card's primary action or its in-progress Cancel.
    pub(crate) card_focus: gpui::FocusHandle,
    pub(crate) focus_card: bool,
    pub(crate) ocr_state: OcrState,
    /// Converting the source to Markdown, OCR consent and setup.
    pub(crate) import: import_flow::ImportFlow,
    pub(crate) markdown_search: Entity<markdown_search::SearchInput>,
    /// The Replace row's field; its text stays with the tab (ADR 0063).
    pub(crate) markdown_replace: Entity<markdown_search::SearchInput>,
    pub(crate) search: search_session::SearchSession,
    pub(crate) ocr_setup_subscription: Option<Subscription>,
    pub(crate) _subscription: Subscription,
    pub(crate) _markdown_search_subscription: Subscription,
}

/// The system file dialog could not open. On Linux GPUI uses the
/// xdg-desktop-portal file chooser, which minimal systems and WSL lack.
pub(crate) fn chooser_unavailable(error: &impl std::fmt::Display) -> String {
    let hint = if cfg!(target_os = "linux") {
        " Install xdg-desktop-portal with a backend such as xdg-desktop-portal-gtk, \
         or open files from the command line (mdoc path/to/file) or by dropping them."
    } else {
        ""
    };
    format!("The file dialog could not open: {error}.{hint}")
}

/// A background job a document ran, as the window learns of its end.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Background {
    Import,
    Docx { generation: u64 },
}

pub(crate) struct DocumentViewDependencies {
    pub(crate) closing: workspace::CloseWatch,
    pub(crate) preferences: settings::Shared,
    pub(crate) model_panel: Entity<settings_ui::Panel>,
    pub(crate) theme: Rc<Cell<Theme>>,
    pub(crate) import_busy: Arc<AtomicBool>,
    pub(crate) ocr: OcrState,
    pub(crate) title: style::TitleBar,
    /// Keyboard input source languages (`recognition::input_source_languages`).
    pub(crate) input_languages: fn() -> Vec<recognition::Language>,
}

/// The shared OCR state for the Settings model from what Settings already
/// knows, without starting a check (ADR 0066).
pub(crate) fn known_ocr_state(
    preferences: &settings::Shared,
    panel: &settings_ui::Panel,
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
    match &panel.statuses[settings_ui::Panel::index(settings::Model::Ocr(config.model))] {
        settings_ui::Status::Damaged(e) | settings_ui::Status::Unavailable(e) => {
            OcrState::Failed(e.clone())
        }
        _ => OcrState::Missing,
    }
}

impl DocumentView {
    pub(crate) fn new(
        dependencies: DocumentViewDependencies,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Self {
        let markdown_search = cx
            .new(|cx| markdown_search::SearchInput::new(cx).with_theme(dependencies.theme.clone()));
        let markdown_replace = cx.new(|cx| {
            markdown_search::SearchInput::new(cx)
                .with_theme(dependencies.theme.clone())
                .with_key_context("MarkdownReplace")
                .with_placeholder("Replace with")
        });
        let scroll = ScrollHandle::new();
        let editor = cx.new(|cx| {
            // Ghost text at the caret; the empty page centers the rest (ADR 0028).
            let mut editor = EditorState::new(window, cx).with_placeholder("Type here…");
            editor.set_markdown_style(style::markdown_style(dependencies.theme.get()), cx);
            editor.set_block_chip_provider(|src| {
                gpui_pdf::is_pdf(src).then(|| src.to_owned().into())
            });
            spelling::install_editor_hooks(&mut editor, dependencies.preferences.clone());
            editor.set_autoscroll(scroll.clone());
            editor
        });
        let images = images::install(&editor, Document::default().directory(), cx);
        window.focus(&editor.read(cx).focus_handle(cx), cx);
        let subscription =
            cx.subscribe_in(&editor, window, |this, _, event, window, cx| match event {
                EditorEvent::Transaction(transaction) => this.pii_transaction(transaction, cx),
                EditorEvent::Changed => {
                    this.copy_feedback = None;
                    this.pii_edited(cx);
                    this.schedule_spellcheck(true, cx);
                    this.schedule_case_save(cx);
                    this.lifecycle.edited();
                    let dirty = this.dirty(cx);
                    this.lifecycle.set_dirty(dirty);
                    this.refresh_markdown_search(SearchRefresh::DocumentEdit, false, window, cx);
                    cx.notify();
                }
                EditorEvent::ActivateAnnotations(ids) => {
                    this.choose_annotations(ids.clone(), window, cx)
                }
                EditorEvent::ActivateAnnotation(id) => this.activate_annotation(*id, window, cx),
                EditorEvent::MiddleClickAnnotation(id, modifiers) => {
                    this.middle_click_mention(*id, *modifiers, false, window, cx)
                }
                EditorEvent::SelectionChanged => this.sync_selection_action(cx),
                EditorEvent::DiagnosticAction(action, word) => {
                    this.spelling_action(*action, word.clone(), cx)
                }
                EditorEvent::SelectionAction => {
                    this.add_pii_candidate(&PiiAddCandidate, window, cx)
                }
                // A hovered replacement shows its tooltip (ADR 0048).
                EditorEvent::HoverAnnotation(_) => cx.notify(),
                EditorEvent::OpenLink(src) => {
                    if src.starts_with("https://")
                        || src.starts_with("http://")
                        || src.starts_with("mailto:")
                    {
                        cx.open_url(src);
                    } else if let Some(path) = document::local_path(src, &this.save_directory()) {
                        cx.emit(workspace::TabEvent::Tabs(workspace::TabsEvent::Open(vec![
                            path,
                        ])));
                    }
                }
            });
        let markdown_search_subscription = cx.subscribe_in(
            &markdown_search,
            window,
            |this, _, event, window, cx| match event {
                markdown_search::SearchInputEvent::Changed if this.search.open() => {
                    this.refresh_markdown_search(SearchRefresh::Query, true, window, cx);
                }
                _ => {}
            },
        );
        Self {
            focus: cx.focus_handle(),
            toolbar_region: cx.focus_handle(),
            controls_region: cx.focus_handle(),
            switcher_region: cx.focus_handle(),
            find_region: cx.focus_handle(),
            preview_region: cx.focus_handle(),
            comments_region: cx.focus_handle(),
            more_focus: cx.focus_handle().tab_stop(true),
            more_menu_focus: cx.focus_handle(),
            more_bounds: Default::default(),
            panes_drawn: std::cell::Cell::new((true, false)),
            closing: dependencies.closing,
            background: Vec::new(),
            active: true,
            lifecycle: Default::default(),
            file_drag: None,
            preferences: dependencies.preferences,
            model_panel: dependencies.model_panel,
            theme: dependencies.theme.clone(),
            title: dependencies.title,
            editor,
            images,
            session: document_session::DocumentSession::default(),
            pii: pii::ui::ReviewUi::new(dependencies.theme.clone(), cx),
            spelling: spelling::SpellState::default(),
            preview: preview::PreviewState::default(),
            scroll,
            error: None,
            copy_feedback: None,
            more_open: false,
            zoom: 1.,
            inspector_share: false,
            copied_for_ai: false,
            copied_reply: false,
            reply: Default::default(),
            reply_box: None,
            case: Default::default(),
            case_save: None,
            case_names_open: false,
            case_term: None,
            clipboard_notice: None,
            compare: None,
            switcher: Default::default(),
            view_bounds: Rc::new(Cell::new(gpui::Bounds::default())),
            split_dragging: false,
            original_selected: false,
            card_focus: cx.focus_handle(),
            focus_card: false,
            ocr_state: dependencies.ocr,
            import: import_flow::ImportFlow::new(
                dependencies.import_busy,
                dependencies.input_languages,
            ),
            markdown_search,
            markdown_replace,
            search: search_session::SearchSession::default(),
            ocr_setup_subscription: None,
            _subscription: subscription,
            _markdown_search_subscription: markdown_search_subscription,
        }
    }

    fn toggle_theme(&mut self, _: &ToggleTheme, _: &mut Window, cx: &mut Context<Self>) {
        cx.emit(workspace::TabEvent::ToggleTheme);
    }

    pub(crate) fn replace_images(
        &mut self,
        directory: PathBuf,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.images.release(window, cx);
        self.images = images::install(&self.editor, directory, cx);
    }

    pub(crate) fn dirty(&self, cx: &App) -> bool {
        if self.lifecycle.loading()
            || self.lifecycle.unavailable()
            || self.lifecycle.source_only()
            || self.lifecycle.generated_unedited()
        {
            return false;
        }
        self.session.dirty(self.editor.read(cx).text())
            || (self.session.document.path.is_none() && self.preview.attachment().is_some())
    }

    /// A Markdown tab with no content and no source shows the empty page (ADR 0028).
    fn shows_empty_page(&self, cx: &App) -> bool {
        !self.lifecycle.loading()
            && !self.lifecycle.unavailable()
            && !self.lifecycle.source_only()
            && self.session.source.is_none()
            && self.preview.source().is_none()
            && self.preview.attachment().is_none()
            && self.editor.read(cx).text().is_empty()
    }

    pub(crate) fn can_copy_markdown(&self) -> bool {
        !self.lifecycle.loading() && !self.lifecycle.unavailable() && !self.lifecycle.source_only()
    }

    pub(crate) fn copy_markdown(
        &mut self,
        _: &CopyMarkdown,
        _: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if !self.can_copy_markdown() {
            return;
        }
        cx.write_to_clipboard(gpui::ClipboardItem::new_string(
            self.editor.read(cx).text().to_owned(),
        ));
        self.copied_for_ai = false;
        self.copied_reply = false;
        self.show_copy_feedback(cx);
    }
    pub(crate) fn show_copy_feedback(&mut self, cx: &mut Context<Self>) {
        // Replacing the task restarts feedback; typing and document transitions cancel it.
        self.copy_feedback = Some(cx.spawn(async move |this, cx| {
            cx.background_executor()
                .timer(std::time::Duration::from_secs(2))
                .await;
            let _ = this.update(cx, |this, cx| {
                this.copy_feedback = None;
                cx.notify();
            });
        }));
        cx.notify();
    }

    /// This tab has a completed scan or replacement data, not just an open review.
    pub(crate) fn pseudonymized(&self) -> bool {
        !self.pii.scans().is_empty()
            || !self.pii.review().variants().is_empty()
            || !self.pii.review().applied().is_empty()
    }
    /// The Markdown came from OCR.
    pub(crate) fn recognized(&self) -> bool {
        self.session.ocr_configuration.is_some()
    }

    /// The OCR model chosen in Settings, whose readiness `ocr_state` shares.
    fn settings_ocr_model(&self) -> Option<settings::OcrModel> {
        self.preferences
            .borrow()
            .snapshot()
            .ok()
            .map(|p| p.ocr.model)
    }

    /// This document's recognition languages: the card's pick, else the
    /// detected default with its reason (ADR 0065).
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
    fn installed_for(&self, model: settings::OcrModel, cx: &App) -> Option<ocr::Installed> {
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

    /// Zoom the Markdown text between 50% and 200% in 5% steps.
    pub(crate) fn set_zoom(&mut self, zoom: f32, cx: &mut Context<Self>) {
        let zoom = ((zoom * 20.).round() / 20.).clamp(0.5, 2.);
        // Show the pill so keyboard zoom reports its level.
        self.wake_switcher(cx);
        if zoom != self.zoom {
            self.zoom = zoom;
            cx.notify();
        }
    }

    pub(crate) fn display_name(&self) -> String {
        if self.lifecycle.source_only() {
            self.preview
                .source()
                .and_then(|path| path.file_name())
                .map(|name| name.to_string_lossy().into_owned())
                .unwrap_or_else(|| "Source".into())
        } else {
            self.session.display_name()
        }
    }

    pub(crate) fn open_source(
        &mut self,
        path: PathBuf,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.lifecycle.show_source();
        self.preview.show_source(path.clone());
        self.open_path(path, window, cx);
        if self.active {
            window.focus(&self.focus, cx);
        }
    }

    /// The window title while this tab shows; the workspace sets it.
    pub(crate) fn window_title(&self) -> String {
        format!(
            "{}{} — mdoc",
            if self.lifecycle.dirty() { "• " } else { "" },
            self.display_name()
        )
    }

    pub(crate) fn request(&mut self, next: Next, window: &mut Window, cx: &mut Context<Self>) {
        let dirty = self.dirty(cx);
        match self.lifecycle.request(dirty) {
            Some(LifecycleEffect::Proceed) => {
                self.proceed(next, window, cx);
                return;
            }
            Some(LifecycleEffect::AskToSave) => {}
            _ => return,
        }
        // Save is the default, Discard ⌘D / Alt+D, Cancel Esc (ADR 0064).
        let question = format!("Save changes to “{}”?", self.display_name());
        let answer = window.prompt(
            PromptLevel::Warning,
            &question,
            Some("Unsaved changes will be lost if you discard them."),
            &[
                gpui::PromptButton::new("Save"),
                gpui::PromptButton::new("Discard"),
                gpui::PromptButton::cancel("Cancel"),
            ],
            cx,
        );
        cx.spawn_in(window, async move |this, cx| {
            let answer = answer.await.ok();
            let _ = this.update_in(cx, |this, window, cx| {
                this.lifecycle.end_prompt();
                match answer {
                    Some(0) => this.save(false, Some(next), window, cx),
                    Some(1) => this.proceed(next, window, cx),
                    _ => {
                        cx.emit(workspace::TabEvent::Close(workspace::CloseEvent::Cancelled));
                    }
                }
                this.resume_import(window, cx);
            });
        })
        .detach();
    }

    pub(crate) fn proceed(&mut self, next: Next, window: &mut Window, cx: &mut Context<Self>) {
        self.copy_feedback = None;
        match next {
            Next::Close => {
                cx.emit(workspace::TabEvent::Close(workspace::CloseEvent::Resolved));
                return;
            }
            Next::CreateCase { close } => {
                cx.emit(workspace::TabEvent::Case(workspace::CaseEvent::Create {
                    close,
                }));
                return;
            }
            Next::Import(imported) => {
                let automatic = self.import.imported();
                self.lifecycle.converted(automatic);
                let retain_preview = self.preview.source() == Some(&imported.source)
                    && (self.preview.pdf().is_some() || self.preview.loading());
                self.session
                    .import(imported.source.clone(), imported.warning);
                self.session.ocr_configuration = imported.ocr_configuration;
                self.reset_pii(cx);
                if !retain_preview {
                    self.close_preview(window, cx);
                }
                self.replace_images(self.save_directory(), window, cx);
                self.editor
                    .update(cx, |editor, cx| editor.set_text(imported.markdown, cx));
                let dirty = self.dirty(cx);
                self.lifecycle.set_dirty(dirty);
                self.reset_markdown_search(cx);
                if self.active {
                    window.focus(&self.editor.read(cx).focus_handle(cx), cx);
                }
                self.error = None;
                if !retain_preview {
                    self.preview.clear_message();
                }
                if retain_preview {
                    self.preview.show();
                } else if imported.is_pdf {
                    self.open_pdf(imported.source, window, cx);
                } else if imported.is_docx {
                    self.open_docx(imported.source, window, cx);
                } else {
                    self.preview.no_preview(imported.source);
                }
                self.scroll.set_offset(gpui::point(px(0.), px(0.)));
            }
        }
        cx.notify();
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
            self.error = Some(
                "Selected OCR model is not ready. Open Settings to download or repair it.".into(),
            );
            cx.notify();
            return;
        }
        let effect = self.import.begin(path, installed, skip_ocr);
        self.carry_out_import(effect, window, cx);
    }

    /// Keep a running job's task, dropping those that have ended.
    pub(crate) fn keep_background(&mut self, task: gpui::Task<Background>) {
        self.background.retain(|task| !task.is_ready());
        self.background.push(task);
    }

    /// The tab is closing: its running jobs, for the window to wait on.
    pub(crate) fn take_background(&mut self) -> Vec<gpui::Task<Background>> {
        std::mem::take(&mut self.background)
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
                    installed,
                    skip_ocr,
                    permit,
                    cancel,
                } => {
                    cx.emit(workspace::TabEvent::Import(workspace::ImportEvent::State));
                    self.error = None;
                    let generation = self.session.generation;
                    let task = cx.background_executor().spawn(async move {
                        let result = import::prepare_cancellable(
                            &path,
                            installed.as_ref(),
                            skip_ocr,
                            &cancel,
                        );
                        drop(permit);
                        result
                    });
                    let task = cx.spawn_in(window, async move |this, cx| {
                        let result = task.await;
                        let _ = this.update_in(cx, |this, window, cx| {
                            this.import.complete(generation, result);
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
                    self.request(Next::Import(imported), window, cx);
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

    fn setup_ocr(&mut self, _: &SetupOcr, window: &mut Window, cx: &mut Context<Self>) {
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
                    if let settings_ui::Event::Finished(settings::Model::Ocr(model), result) = event
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
            self.finish_ocr_setup(
                Err("Another model job is running. Retry when it finishes.".into()),
                window,
                cx,
            );
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

    /// The Markdown pane before conversion: one card that changes in place
    /// through conversion, OCR consent, setup and recognition (ADR 0026).
    fn conversion_card(&mut self, window: &mut Window, cx: &mut Context<Self>) -> gpui::AnyElement {
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
            .map(|m| panel.pending[settings_ui::Panel::index(m)])
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
        let Some(pages) = self.import.ocr_required().map(<[u32]>::to_vec) else {
            let (title, busy) = if self.import.busy() {
                (
                    if self.import.recognizing() {
                        "Recognizing text…"
                    } else {
                        "Converting to Markdown…"
                    },
                    true,
                )
            } else if self.import.auto_pending() {
                ("Waiting to convert…", true)
            } else if self.error.is_some() {
                ("Conversion could not be completed", false)
            } else {
                ("Convert this document to begin editing", false)
            };
            return card
                .child(if busy {
                    ui::activity("conversion-activity", title, theme)
                        .text_size(px(14.))
                        .into_any_element()
                } else {
                    ui::card_title(title).into_any_element()
                })
                .when_some(self.error.clone().filter(|_| !busy), |v, error| {
                    v.child(ui::card_text(error, theme))
                })
                .into_any_element();
        };
        let count = pages.len();
        let card = card
            .child(ui::card_title(if count == 0 { "Text recognition needs your attention" } else { self.ocr_pages_title() }))
            .when(count > 0, |v| v.child(ui::card_text(if self.import.ocr_all_pages() {
                format!(
                    "{} usable text. Recognition reads {} on this computer; the original stays in the preview.",
                    if count == 1 { "The page has no" } else { "No page has" },
                    if count == 1 { "it" } else { "them" },
                )
            } else {
                format!(
                    "{count} {} no usable text ({}). Recognition reads {} on this computer; the original stays in the preview.",
                    if count == 1 { "page has" } else { "pages have" },
                    ui::page_ranges(&pages, 8),
                    if count == 1 { "it" } else { "them" },
                )
            }, theme)));
        if !ocr::SUPPORTED {
            return card
                .child(ui::card_text(
                    "Local OCR is unavailable on this platform.",
                    theme,
                ))
                .child(
                    ui::card_actions().child(
                        ui::primary_button("extract-native", "Use native text only", theme, true)
                            .on_click(|_, window, cx| {
                                window.dispatch_action(Box::new(ExtractNative), cx)
                            }),
                    ),
                )
                .into_any_element();
        }
        if self.import.recognizing() {
            return card
                .child(ui::activity(
                    "recognition-activity",
                    "Recognizing text…",
                    theme,
                ))
                .into_any_element();
        }
        if installing {
            return card
                .child(match &progress {
                    Some(state) => {
                        ui::setup_progress("ocr-setup-progress", state, cancelling, theme)
                            .into_any_element()
                    }
                    None => {
                        ui::activity("ocr-setup-progress", "Preparing…", theme).into_any_element()
                    }
                })
                .child(
                    ui::card_actions().child(
                        settings_ui::control(
                            "cancel-ocr-setup",
                            "Cancel",
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
            "Run OCR"
        } else if failed.is_some() {
            "Retry"
        } else if pending.total() == 0 {
            // Installed but not yet checked: setup only verifies it.
            "Run OCR"
        } else {
            "Download & recognize"
        };
        if std::mem::take(&mut self.focus_card)
            && (self.focus.contains_focused(window, cx) || window.focused(cx).is_none())
        {
            let focus = self.card_focus.clone();
            window.defer(cx, move |window, cx| window.focus(&focus, cx));
        }
        let name = model
            .map(crate::models::short_name)
            .unwrap_or_else(|| "OCR".into());
        // The engine says what it cannot read (ADR 0065).
        let hint = if unreadable.is_empty() {
            choice.reason.clone()
        } else {
            format!(
                "{}. {} OCR cannot read {}.",
                choice.reason,
                mdoc_import::ocr::title(ocr_model),
                recognition::label(&unreadable)
            )
        };
        let accent = theme.search_accent();
        let picker = div()
            .id("recognition-languages")
            .role(gpui::Role::RadioGroup)
            .aria_label("Recognition languages")
            .flex()
            .flex_wrap()
            .items_center()
            .gap_1()
            .child(
                div()
                    .mr_1()
                    .text_size(px(12.))
                    .text_color(palette.header_muted)
                    .child("Recognize"),
            )
            .children(mdoc_import::ocr::offered().into_iter().enumerate().map(
                |(index, languages)| {
                    let selected = choice.languages == languages;
                    let label = recognition::label(languages);
                    let selector = format!("recognize {label}");
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
                ui::card_hint(
                    "For this document only. Default: Settings › Recognition.",
                    theme,
                )
                .when(cfg!(test), |v| {
                    v.debug_selector(|| "ocr-document-only".into())
                }),
            )
            .when_some(failed, |v, error| v.child(ui::card_error(error, theme)))
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
                        settings_ui::control(
                            "extract-native",
                            "Use native text only",
                            theme,
                            !disabled,
                        )
                        .when(cfg!(test), |v| {
                            v.debug_selector(|| "Use native text only".into())
                        })
                        .on_click(move |_, window, cx| {
                            if !disabled {
                                window.dispatch_action(Box::new(ExtractNative), cx)
                            }
                        }),
                    )
                    .child(
                        ui::link_button("choose-ocr-model", "Choose model…", theme, true)
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
                    format!("{name} · runs on this computer")
                } else {
                    format!(
                        "{name} · {}, once · then runs on this computer",
                        pending.download_label()
                    )
                },
                theme,
            ))
            .when_some(model, |card, model| {
                card.child(ui::model_attribution("ocr-model-attribution", model, theme))
            })
            .into_any_element()
    }

    /// "All pages…" when the whole PDF is scanned, "Some pages…" otherwise.
    fn ocr_pages_title(&self) -> &'static str {
        match (
            self.import.ocr_all_pages(),
            self.import.ocr_required().map(<[u32]>::len),
        ) {
            (true, Some(1)) => "This page needs text recognition",
            (true, _) => "All pages need text recognition",
            (false, _) => "Some pages need text recognition",
        }
    }

    /// The preview pane without a page: progress, or one plain message with
    /// Retry only when another attempt can succeed.
    fn preview_status(&self, theme: Theme) -> gpui::AnyElement {
        if self.preview.loading() {
            return ui::activity("preview-activity", "Preparing preview…", theme)
                .into_any_element();
        }
        let message = if self.preview.queued() {
            "Waiting to prepare preview…".to_string()
        } else {
            self.preview
                .message()
                .cloned()
                .unwrap_or_else(|| "No preview".into())
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
                v.child(button("Retry", RetryPreview, theme))
            })
            .into_any_element()
    }

    fn notice(
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
                            .flex_1()
                            .min_w_0()
                            .text_color(palette.header_muted)
                            .child(detail),
                    )
                    .when(id == "preview-notice" && self.preview.retryable(), |v| {
                        v.child(button("Retry", RetryPreview, theme))
                    })
                    .when(id == "ocr-notice", |v| {
                        v.child(ui::control("show-markdown", "Show", theme, true).on_click(
                            cx.listener(|this, _, _, cx| {
                                this.original_selected = false;
                                cx.notify();
                            }),
                        ))
                    })
                    .child(
                        crate::ui::focus_ring((id, 1usize), theme)
                            .role(gpui::Role::Button)
                            .key_context("UiControl")
                            .tab_index(0)
                            .aria_label("Dismiss notice")
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
                                    "clipboard-notice" => this.clipboard_notice = None,
                                    _ => this.error = None,
                                }
                                cx.notify();
                            })),
                    ),
            )
            .into_any_element()
    }

    pub(crate) fn save_directory(&self) -> PathBuf {
        self.session.directory()
    }

    pub(crate) fn open_path(&mut self, path: PathBuf, window: &mut Window, cx: &mut Context<Self>) {
        if path
            .extension()
            .is_some_and(|s| s.eq_ignore_ascii_case("pdf"))
        {
            self.open_pdf(path, window, cx);
            self.error = None;
            cx.notify();
        } else if path
            .extension()
            .is_some_and(|s| s.eq_ignore_ascii_case("docx"))
        {
            self.open_docx(path, window, cx);
            cx.notify();
        } else if document::is_markdown(&path) {
            cx.emit(workspace::TabEvent::Tabs(workspace::TabsEvent::Open(vec![
                path,
            ])));
        } else if self.lifecycle.source_only() {
            let generation = self.begin_preview(path.clone());
            let task = cx.background_executor().spawn(async move {
                std::fs::File::open(path)
                    .and_then(|file| file.metadata())
                    .map(|_| ())
            });
            cx.spawn_in(window, async move |this, cx| {
                let result = task.await;
                let _ = this.update(cx, |this, cx| {
                    if this.preview.checked(generation, result.is_ok()) {
                        cx.notify();
                    }
                });
            })
            .detach();
            cx.notify();
        } else {
            self.error = Some("Choose a Markdown or supported source document.".into());
            cx.notify();
        }
    }

    pub(crate) fn open(&mut self, _: &Open, window: &mut Window, cx: &mut Context<Self>) {
        let Some(LifecycleEffect::AskForFiles) = self.lifecycle.open_files() else {
            return;
        };
        let paths = cx.prompt_for_paths(PathPromptOptions {
            files: true,
            directories: false,
            multiple: true,
            prompt: Some("Open one or more files".into()),
        });
        cx.spawn_in(window, async move |this, cx| {
            let result = paths.await;
            let _ = this.update_in(cx, |this, window, cx| {
                this.lifecycle.end_prompt();
                match result {
                    Ok(Ok(Some(paths))) => {
                        cx.emit(workspace::TabEvent::Tabs(workspace::TabsEvent::Open(paths)))
                    }
                    Ok(Err(error)) => {
                        this.error = Some(chooser_unavailable(&error));
                        cx.notify();
                    }
                    _ => {}
                }
                this.resume_import(window, cx);
            });
        })
        .detach();
    }

    pub(crate) fn save(
        &mut self,
        save_as: bool,
        next: Option<Next>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        match self
            .lifecycle
            .save(save_as, self.session.document.path.is_some())
        {
            LifecycleEffect::CancelClose => {
                cx.emit(workspace::TabEvent::Close(workspace::CloseEvent::Cancelled));
                return;
            }
            LifecycleEffect::Write => {
                if let Some(path) = self.session.document.path.clone() {
                    self.write(path, next, cx);
                }
                return;
            }
            _ => {}
        }
        let name = self.session.suggested_name();
        let path = cx.prompt_for_new_path(&self.save_directory(), Some(&name));
        cx.spawn_in(window, async move |this, cx| {
            let result = path.await;
            let _ = this.update_in(cx, |this, window, cx| {
                this.lifecycle.end_prompt();
                if let Ok(Ok(Some(mut path))) = result {
                    if path.extension().is_none() {
                        path.set_extension("md");
                    }
                    if !document::is_markdown(&path) {
                        this.error = Some(
                            "Save Markdown with a .md, .markdown, .mdown, or .txt extension."
                                .into(),
                        );
                        cx.emit(workspace::TabEvent::Close(workspace::CloseEvent::Cancelled));
                        cx.notify();
                        this.resume_import(window, cx);
                        return;
                    }
                    this.write(path, next, cx);
                } else {
                    if let Ok(Err(error)) = &result {
                        this.error = Some(chooser_unavailable(error));
                        cx.notify();
                    }
                    cx.emit(workspace::TabEvent::Close(workspace::CloseEvent::Cancelled));
                }
                this.resume_import(window, cx);
            });
        })
        .detach();
    }

    pub(crate) fn write(&mut self, path: PathBuf, next: Option<Next>, cx: &mut Context<Self>) {
        // The window checks that no other tab has the file open, then calls
        // `save_to`.
        let path = session_store::identity(&path);
        cx.emit(workspace::TabEvent::Save(workspace::SaveEvent::Write {
            path,
            next,
        }));
    }

    /// Save to `path`, which no other tab has open, then go on with `next`.
    pub(crate) fn save_to(
        &mut self,
        path: PathBuf,
        next: Option<Next>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let old_directory = self.save_directory();
        match self.session.save(path, self.editor.read(cx).text()) {
            Ok(()) => {
                self.lifecycle.saved();
                cx.emit(workspace::TabEvent::Save(workspace::SaveEvent::Saved));
                if old_directory != self.session.document.directory() {
                    self.replace_images(self.session.document.directory(), window, cx);
                }
                self.error = None;
                if let Some(next) = next {
                    self.proceed(next, window, cx);
                }
            }
            Err(error) => {
                self.error = Some(format!("Could not save: {error}"));
                cx.emit(workspace::TabEvent::Close(workspace::CloseEvent::Cancelled));
            }
        }
        cx.notify();
    }
}

pub(crate) fn button(label: &'static str, action: impl gpui::Action, theme: Theme) -> ui::Control {
    ui::control(label, label, theme, true)
        .when(cfg!(test), |view| view.debug_selector(move || label.into()))
        .on_click(move |_, window, cx| window.dispatch_action(action.boxed_clone(), cx))
}

/// A find-bar control: a focusable button named for assistive technology,
/// whose tooltip shows its shortcut.
fn search_bar_button(
    id: &'static str,
    content: impl IntoElement,
    label: &'static str,
    shortcut: &'static str,
    theme: Theme,
) -> ui::Control {
    let palette = theme.pdf_style();
    crate::ui::focus_ring(id, theme)
        .when(cfg!(test), |v| v.debug_selector(move || id.into()))
        .role(gpui::Role::Button)
        .key_context("UiControl")
        .tab_index(0)
        .aria_label(label)
        .aria_keyshortcuts(shortcut)
        .tooltip(style::tooltip(format!("{label}  {shortcut}"), theme))
        .flex_shrink_0()
        .rounded_md()
        .px_2()
        .h(px(28.))
        .flex()
        .items_center()
        .cursor_pointer()
        .text_color(palette.header_muted)
        .hover(|view| view.bg(palette.placeholder_bg))
        .child(content)
}

/// The macOS or the Windows/Linux spelling of a shortcut.
const fn chord(mac: &'static str, other: &'static str) -> &'static str {
    if cfg!(target_os = "macos") {
        mac
    } else {
        other
    }
}

/// The find bar's counter: what it shows ("14 / 16") and what a screen reader
/// says ("Match 14 of 16"), ADR 0064.
pub(crate) fn find_counter(
    empty_query: bool,
    searching: bool,
    active: Option<usize>,
    matches: usize,
) -> (String, gpui::SharedString) {
    if empty_query {
        ("0 / 0".into(), "No search".into())
    } else if searching {
        ("Searching…".into(), "Searching…".into())
    } else if matches == 0 {
        ("0 matches".into(), "No matches".into())
    } else {
        let position = active.map_or(0, |index| index + 1);
        let spoken = if position == 0 {
            format!(
                "{matches} {}",
                if matches == 1 { "match" } else { "matches" }
            )
        } else {
            ui::position_announcement("Match", position, matches)
        };
        (format!("{position} / {matches}"), spoken.into())
    }
}

fn toggled(on: bool) -> gpui::Toggled {
    if on {
        gpui::Toggled::True
    } else {
        gpui::Toggled::False
    }
}

/// The find bar's keys, from the shortcuts table (ADR 0063, 0073).
pub(crate) fn bind_markdown_search_keys(cx: &mut App) {
    cx.bind_keys(crate::shortcuts::bindings(crate::shortcuts::Owner::Find));
}

/// One zoom step (ADR 0048).
pub(crate) const ZOOM_STEP: f32 = 0.05;
/// The widest the Markdown reading column gets at 100% (ADR 0048).
const READING_WIDTH: f32 = 760.;

impl DocumentView {
    /// Focus handles outside the toolbar whose controls count as the
    /// toolbar's region (ADR 0062).
    pub(crate) fn toolbar_members(&self) -> [&gpui::FocusHandle; 3] {
        [
            &self.controls_region,
            &self.find_region,
            &self.switcher_region,
        ]
    }

    /// The document's F6 regions in order, each with whether it is a single
    /// focus target (the editor) rather than a group of controls (ADR 0062).
    pub(crate) fn focus_regions(&self, cx: &App) -> Vec<(gpui::FocusHandle, bool)> {
        let (markdown, original) = self.panes_drawn.get();
        let mut regions = vec![(self.toolbar_region.clone(), false)];
        if markdown
            && !self.lifecycle.source_only()
            && !self.lifecycle.loading()
            && !self.lifecycle.unavailable()
            && self.compare.is_none()
        {
            regions.push((self.editor.read(cx).focus_handle(cx), true));
        }
        if self.replacements_open() && self.can_copy_markdown() {
            regions.push((self.replacements_focus(), false));
        }
        if self.preview.visible() && original {
            regions.push((self.preview_region.clone(), false));
            if self.preview.comment_panel().is_some() {
                regions.push((self.comments_region.clone(), false));
            }
        }
        regions
    }
}

/// Jobs still running when a document goes without closing its tab (the
/// window closing) keep running to their end, as before the one channel.
impl Drop for DocumentView {
    fn drop(&mut self) {
        for task in self.background.drain(..) {
            task.detach();
        }
    }
}

impl Render for DocumentView {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        // The before/after view follows the applied replacements (ADR 0042).
        self.refresh_compare(window, cx);
        // The Share panel's reply box restores against the live mapping (ADR 0078).
        if self.inspector_share && self.replacements_open() {
            self.ensure_reply_box(window, cx);
            self.refresh_reply();
        }
        self.release_closed_popup_focus(window, cx);
        // Derived display state; set_outlines only notifies on change.
        self.sync_scope_outlines(cx);
        // Loads and Settings changes reach spellcheck here (no-op otherwise).
        self.ensure_spellcheck(cx);
        // A finished or abandoned OS drag leaves no feedback behind.
        if !cx.has_active_drag() {
            self.file_drag = None;
        }
        let empty_page = self.shows_empty_page(cx);
        let theme = self.theme.get();
        let palette = theme.pdf_style();
        let search_focused = self
            .markdown_search
            .read(cx)
            .focus_handle(cx)
            .is_focused(window);
        let query = self.markdown_search.read(cx).value().to_owned();
        let (count, spoken_count) = find_counter(
            query.is_empty(),
            self.search.running(),
            self.search.active(),
            self.search.matches().len(),
        );
        let navigation_disabled = self.search.matches().is_empty();
        let replace_focused = self
            .markdown_replace
            .read(cx)
            .focus_handle(cx)
            .is_focused(window);
        let field = |id: &'static str, focused: bool| {
            div()
                .id(id)
                .when(cfg!(test), |v| v.debug_selector(move || id.into()))
                .flex()
                .flex_1()
                // Keep room for text after padding, even beside the inspector.
                .min_w(px(96.))
                .h(px(28.))
                .px_2()
                .items_center()
                .bg(palette.bg)
                .border_1()
                .border_color(if focused {
                    theme.search_accent()
                } else {
                    palette.border
                })
                .rounded_md()
        };
        let replace_toggle = search_bar_button(
            "markdown-search-replace-toggle",
            if self.search.replace_open() {
                "⌄"
            } else {
                "›"
            },
            "Replace",
            chord("⌥⌘F", "Ctrl+H"),
            theme,
        )
        .w(px(20.))
        .aria_expanded(self.search.replace_open())
        .on_click(cx.listener(|this, _, window, cx| this.toggle_markdown_replace(window, cx)));
        let match_case = search_bar_button(
            "markdown-search-match-case",
            "Aa",
            "Match case",
            chord("⌥⌘C", "Alt+C"),
            theme,
        )
        .aria_toggled(toggled(self.search.match_case()))
        .when(self.search.match_case(), |view| {
            view.bg(palette.placeholder_bg)
                .text_color(theme.search_accent())
        })
        .on_click(cx.listener(|this, _, window, cx| {
            this.toggle_markdown_match_case(&ToggleMarkdownMatchCase, window, cx)
        }));
        let whole_word = search_bar_button(
            "markdown-search-whole-word",
            div().underline().child("ab"),
            "Whole word",
            chord("⌥⌘W", "Alt+W"),
            theme,
        )
        .aria_toggled(toggled(self.search.whole_word()))
        .when(self.search.whole_word(), |view| {
            view.bg(palette.placeholder_bg)
                .text_color(theme.search_accent())
        })
        .on_click(cx.listener(|this, _, window, cx| {
            this.toggle_markdown_whole_word(&ToggleMarkdownWholeWord, window, cx)
        }));
        // One live node for the count, "Searching…" and replace results, so
        // each change is announced (ADR 0063, ADR 0064).
        let status = self.search.status().cloned();
        let spoken_count = match (&status, self.search.running()) {
            (Some(status), false) => status.clone(),
            _ => spoken_count,
        };
        let count = ui::live_text("markdown-search-count", spoken_count)
            .when(cfg!(test), |v| {
                v.debug_selector(|| "markdown-search-count".into())
            })
            .flex_shrink_0()
            .max_w_full()
            .px_2()
            .text_size(px(12.))
            .text_color(palette.header_muted)
            .child(if self.search.running() {
                div()
                    .flex()
                    .items_center()
                    .gap_2()
                    .min_w_0()
                    .child(ui::spinner("search-activity", theme))
                    .child(div().min_w_0().child(count))
                    .into_any_element()
            } else if let Some(status) = status {
                div().child(status).into_any_element()
            } else {
                div().child(count).into_any_element()
            });
        let previous = search_bar_button(
            "markdown-search-previous",
            "↑",
            "Previous match",
            chord("⇧⌘G", "Shift+Ctrl+G"),
            theme,
        )
        .w(px(28.))
        .justify_center()
        .when(navigation_disabled, |view| {
            view.text_color(palette.header_muted)
        })
        .on_click(cx.listener(|this, _, window, cx| {
            this.find_previous_markdown(&FindPreviousMarkdown, window, cx)
        }));
        let next =
            search_bar_button(
                "markdown-search-next",
                "↓",
                "Next match",
                chord("⌘G", "Ctrl+G"),
                theme,
            )
            .w(px(28.))
            .justify_center()
            .when(navigation_disabled, |view| {
                view.text_color(palette.header_muted)
            })
            .on_click(cx.listener(|this, _, window, cx| {
                this.find_next_markdown(&FindNextMarkdown, window, cx)
            }));
        let close = search_bar_button(
            "markdown-search-close",
            "×",
            "Close Markdown search",
            "Esc",
            theme,
        )
        .w(px(28.))
        .justify_center()
        .on_click(cx.listener(|this, _, window, cx| {
            this.close_markdown_search(&CloseMarkdownSearch, window, cx)
        }));
        let replace_row = self.search.replace_open().then(|| {
            div()
                .id("markdown-replace-row")
                .flex()
                .flex_wrap()
                .w_full()
                .min_w_0()
                .items_center()
                .gap_1()
                .pl(px(24.))
                .child(
                    field("markdown-replace-input", replace_focused)
                        .aria_label("Replace with")
                        .child(self.markdown_replace.clone()),
                )
                .child(
                    search_bar_button(
                        "markdown-replace-next",
                        "Replace",
                        "Replace",
                        "Enter",
                        theme,
                    )
                    .when(navigation_disabled, |view| {
                        view.text_color(palette.header_muted)
                    })
                    .on_click(cx.listener(|this, _, window, cx| {
                        this.replace_next_markdown(&ReplaceNextMarkdown, window, cx)
                    })),
                )
                .child(
                    search_bar_button(
                        "markdown-replace-all",
                        "Replace All",
                        "Replace all",
                        chord("⌘Enter", "Ctrl+Alt+Enter"),
                        theme,
                    )
                    .when(navigation_disabled, |view| {
                        view.text_color(palette.header_muted)
                    })
                    .on_click(cx.listener(|this, _, window, cx| {
                        this.replace_all_markdown(&ReplaceAllMarkdown, window, cx)
                    })),
                )
        });
        let search_bar = div()
            .id("markdown-search-bar")
            .track_focus(&self.find_region)
            .on_action(cx.listener(|this, action: &mdoc_editor::Undo, window, cx| {
                this.editor
                    .update(cx, |editor, cx| editor.undo(action, window, cx))
            }))
            .on_action(cx.listener(|this, action: &mdoc_editor::Redo, window, cx| {
                this.editor
                    .update(cx, |editor, cx| editor.redo(action, window, cx))
            }))
            .flex()
            .flex_col()
            .gap_1()
            .px_2()
            .py_1()
            .text_size(px(13.))
            .flex_shrink_0()
            .border_b_1()
            .border_color(palette.border)
            .bg(palette.bg)
            .child(
                div()
                    .flex()
                    .flex_wrap()
                    .w_full()
                    .min_w_0()
                    .items_center()
                    .gap_1()
                    .child(replace_toggle)
                    .child(
                        field("markdown-search-input", search_focused)
                            .aria_label("Search Markdown")
                            .child(self.markdown_search.clone()),
                    )
                    .child(match_case)
                    .child(whole_word)
                    .child(count)
                    .child(previous)
                    .child(next)
                    .child(close),
            )
            .children(replace_row);
        let markdown_scrollbar = gpui_pdf::scrollbar::overlay_scrollbar(
            "markdown-scrollbar",
            &self.scroll,
            false,
            palette.header_muted,
            cx,
        );
        let import_notice = self
            .session
            .warning
            .clone()
            .map(|detail| self.notice("import-notice", "Review conversion", detail, false, cx));
        let ocr_notice = (self.import.ocr_required().is_some()
            && !self.import.ocr_notice_dismissed())
        .then(|| {
            self.notice(
                "ocr-notice",
                "Text recognition required",
                format!("{}.", self.ocr_pages_title()),
                false,
                cx,
            )
        });
        let preview_notice = self.preview.message().cloned().map(|detail| {
            self.notice(
                "preview-notice",
                "Preview",
                detail,
                self.preview.failed(),
                cx,
            )
        });
        let setup_notice = match &self.ocr_state {
            OcrState::Failed(error) if !self.import.setup_error_dismissed() => {
                Some(self.notice("setup-notice", "OCR setup", error.clone(), true, cx))
            }
            _ => None,
        };
        let clipboard_notice = self
            .clipboard_notice
            .clone()
            .map(|detail| self.notice("clipboard-notice", "Clipboard", detail, false, cx));
        let width = self.chrome_width(window);
        let content_width = (width - f32::from(self.replacements_panel_width(window))).max(1.);
        let narrow_preview = self.preview.visible() && content_width < 620.;
        let show_original = self.preview.visible() && (!narrow_preview || self.original_selected);
        let show_markdown = !narrow_preview || !self.original_selected;
        self.panes_drawn.set((show_markdown, show_original));
        // The conversion card states a conversion error itself, and the OCR
        // card its recognition error; a notice would only repeat it.
        let card_shows_error = self.lifecycle.source_only()
            && show_markdown
            && !self.import.busy()
            && !self.import.auto_pending()
            && (self.import.ocr_required().is_none()
                || ocr::SUPPORTED
                    && self.error.as_ref().is_some_and(|error| {
                        [self.import.ocr_failure(), self.import.ocr_error()].contains(&Some(error))
                            || matches!(&self.ocr_state, OcrState::Failed(e) if e == error)
                    }));
        // The OCR setup notice may already say the same thing.
        let setup_says_error = !self.import.setup_error_dismissed()
            && !(show_markdown
                && self.lifecycle.source_only()
                && self.import.ocr_required().is_some())
            && matches!(&self.ocr_state, OcrState::Failed(e) if Some(e) == self.error.as_ref());
        let error_notice = self
            .error
            .clone()
            .filter(|_| !card_shows_error && !setup_says_error)
            .map(|detail| self.notice("error-notice", "Needs attention", detail, true, cx));
        let minimum = (280. / content_width.max(620.)).min(0.5);
        let split = self
            .preview
            .split_ratio()
            .unwrap_or(0.5)
            .clamp(minimum, 1. - minimum);
        let measured_bounds = self.view_bounds.clone();
        let measured_owner = cx.entity().downgrade();
        let detached = self.title.detached.get();
        let body = div().relative().size_full().flex().flex_col().bg(palette.bg).text_color(palette.header_fg).text_size(px(16.))
            .child(gpui::canvas(move |bounds, _, cx| { if measured_bounds.replace(bounds) != bounds { let _ = measured_owner.update(cx, |_, cx| cx.notify()); } }, |_, _, _, _| {}).absolute().inset_0())
            .on_mouse_move(cx.listener(|this, event: &gpui::MouseMoveEvent, window, cx| {
                // Repaint so a replacement tooltip goes when the pointer leaves it.
                if this.editor.read(cx).hovered_annotation().is_some() { cx.notify(); }
                if event.pressed_button != Some(gpui::MouseButton::Left) { this.split_dragging = false; }
                if this.split_dragging {
                    let bounds = this.view_bounds.get();
                    let panel = this.replacements_panel_width(window);
                    let width = f32::from(bounds.size.width - panel).max(1.);
                    let ratio = f32::from(event.position.x - bounds.left() - panel) / width;
                    this.preview.set_split(ratio);
                    cx.notify();
                }
            }))
            .on_mouse_up(gpui::MouseButton::Left, cx.listener(|this, _, _, cx| { this.split_dragging = false; cx.notify(); }))
            // While the empty page shows, the whole document view accepts OS file drops.
            .when(empty_page, |v| v
                .on_drag_move(cx.listener(|this, event: &gpui::DragMoveEvent<gpui::ExternalPaths>, _, cx| {
                    let over = event.bounds.contains(&event.event.position);
                    let state = over.then(|| event.drag(cx).paths().iter().any(|path| import::droppable(path)));
                    if this.file_drag != state { this.file_drag = state; cx.notify(); }
                }))
                .can_drop(|drag, _, _| drag.downcast_ref::<gpui::ExternalPaths>().is_some_and(|drag| drag.paths().iter().any(|path| import::droppable(path))))
                .on_drop(cx.listener(|this, drag: &gpui::ExternalPaths, window, cx| {
                    this.file_drag = None;
                    workspace::activate_for_drop(window, cx);
                    cx.emit(workspace::TabEvent::Tabs(workspace::TabsEvent::Open(drag.paths().to_vec())));
                    cx.notify();
                })))
            .when(!detached, |v| v.child(div().track_focus(&self.toolbar_region).key_context("ToolbarRegion").child(self.header(window, cx))))
            .child(div().track_focus(&self.controls_region).key_context("ToolbarRegion").flex().flex_col().flex_shrink_0()
            .when(!self.lifecycle.source_only() && self.import.busy(), |v| v.child(
                ui::activity("document-work-activity", if matches!(self.ocr_state, OcrState::Installing) || self.import.setting_up() { "Setting up text recognition…" } else if self.import.recognizing() { "Recognizing text…" } else { "Converting to Markdown…" }, theme)
                    .px_3().py_2().border_b_1().border_color(palette.border)))
            .when_some(self.session.ocr_configuration.clone(),|v,config|v.child(div().px_3().py_1().text_size(px(11.)).text_color(palette.header_muted).child(format!("OCR result: {} · {} DPI · minimum confidence {} · Force",config.model.name(),config.dpi,config.minimum_confidence))))
            .children(import_notice).children(clipboard_notice).children(self.restore_summary(cx)).children(ocr_notice.filter(|_| !show_markdown))
            .when(narrow_preview, |v| v.child(self.pane_switch(cx))))
            .child(div().flex().flex_1().min_h_0()
                .when(!self.lifecycle.source_only() && show_markdown, |row| row.child(div().relative().flex().flex_1().min_w_0().h_full().flex().flex_col().text_size(px(16. * self.zoom))
                    .child({ let column = self.switcher.column.clone(); gpui::canvas(move |bounds, _, _| column.set(bounds), |_, _, _, _| {}).absolute().inset_0() })
                    .on_mouse_move(cx.listener(|this, event: &gpui::MouseMoveEvent, _, cx| this.switcher_pointer(event.position, cx)))
                    .when(self.search.open(), |column| column.child(search_bar))
                    .child(if self.lifecycle.loading() || self.lifecycle.unavailable() {
                        div().p_6().child(if self.lifecycle.loading() { ui::activity("markdown-loading", "Loading Markdown…", theme).into_any_element() } else { div().child("Markdown unavailable").into_any_element() })
                            .when(self.lifecycle.unavailable(), |view| view.child(button("Retry", RetryDocument, theme))).into_any_element()
                    } else if let Some(compare) = self.compare_view(content_width, cx) {
                        compare
                    } else { div().relative().flex_1().min_w_0().min_h_0()
                        // Absolute, so the column's intrinsic sizing never measures the
                        // editor: each probe width (search-bar min-content, max content,
                        // 0 px) shaped it again, and a changing one missed every cache.
                        .child(div().id("document-scroll").absolute().inset_0().overflow_y_scroll().track_scroll(&self.scroll).p_6().pb(px(72.))
                            .on_scroll_wheel(cx.listener(|this, _, _, cx| { this.wake_switcher(cx); cx.notify(); }))
                            // A centred reading column (ADR 0048, from variant B).
                            .child(div().relative().w_full().max_w(px(READING_WIDTH * self.zoom)).mx_auto().child(self.editor.clone())
                                // The mention bar scrolls with its mention (ADR 0055).
                                .children(self.mention_popup_layer(window, cx))))
                        .child(markdown_scrollbar).children((empty_page && self.file_drag.is_some()).then(|| self.empty_page(cx))).into_any_element() })
                    .children(self.view_switcher(window, cx).filter(|_| !self.lifecycle.loading() && !self.lifecycle.unavailable()))))
                // Replacements sit beside the text they describe, before Original.
                .children(self.replacements_panel(window, cx))
                .when(self.lifecycle.source_only() && show_markdown, |row| row.child(div().id("conversion-pane").flex_1().min_w_0().p_6().flex().items_center().justify_center().overflow_y_scroll()
                    .child(self.conversion_card(window, cx))))
                .when(self.preview.visible() && !narrow_preview, |row| row.child(div().id("preview-divider").when(cfg!(test), |v| v.debug_selector(|| "preview-divider".into())).w(px(6.)).h_full().flex_shrink_0().cursor(gpui::CursorStyle::ResizeLeftRight).bg(palette.border)
                    .on_mouse_down(gpui::MouseButton::Left, cx.listener(|this, _, _, cx| { this.split_dragging = true; cx.notify(); }))))
                .when(show_original && self.preview.pdf().is_none(), |row| row.child(div().track_focus(&self.preview_region).when(!narrow_preview, |v| v.w(px(content_width * (1. - split)))).when(narrow_preview, |v| v.flex_1()).h_full().border_l_1().border_color(palette.border)
                    .flex().flex_col().items_center().justify_center().text_color(palette.header_muted)
                    .child(self.preview_status(theme))))
                .when_some(self.preview.pdf().cloned().filter(|_| show_original), |row, pdf| row.child(div().track_focus(&self.preview_region).when(!narrow_preview, |v| v.w(px(content_width * (1. - split)))).when(narrow_preview, |v| v.flex_1()).flex_shrink_0().min_w_0().h_full().flex().flex_col().border_l_1().border_color(palette.border)
                    .child(div().flex_1().min_h_0().child(if pdf.read(cx).is_locked() { div().p_6().child("This PDF is password-protected. Open an unlocked copy to view it here.").into_any_element() } else { pdf.into_any_element() }))
                    .when_some(self.preview.comment_panel().cloned(), |pane, comments| pane.child(div().track_focus(&self.comments_region).child(comments)))
                    .children(preview_notice)))
)
            .children(setup_notice.filter(|_| !(show_markdown && self.lifecycle.source_only() && self.import.ocr_required().is_some()))).children(error_notice)
            // The word popup anchors to the editor, hidden under before/after.
            .children(self.pii_popup(cx).filter(|_| self.compare.is_none()))
            .children(self.replacement_tip(window, cx).filter(|_| self.compare.is_none()));
        // Sidebar collapsed: the header joins the title row and the document
        // sits in its own card below it (ADR 0048).
        let content = if detached {
            div()
                .size_full()
                .flex()
                .flex_col()
                .text_color(palette.header_fg)
                .child(
                    div()
                        .track_focus(&self.toolbar_region)
                        .key_context("ToolbarRegion")
                        .child(self.header(window, cx)),
                )
                .child(
                    ui::card_surface(div().flex_1().min_h_0().ml(px(style::CARD_MARGIN)), theme)
                        .child(body),
                )
                .into_any_element()
        } else {
            body.into_any_element()
        };
        // Both header layouts share the document's focus and action scope.
        div()
            .size_full()
            .flex()
            .flex_col()
            // Tab between controls is the workspace's: it knows the F6
            // regions (ADR 0062).
            .track_focus(&self.focus)
            .on_action(cx.listener(Self::toggle_theme))
            .on_action(
                cx.listener(|this, _: &crate::pii::ui::ExpandAllIdentities, _, cx| {
                    if this.replacements_open() {
                        this.set_all_identities_expanded(true, cx)
                    }
                }),
            )
            .on_action(
                cx.listener(|this, _: &crate::pii::ui::CollapseAllIdentities, _, cx| {
                    if this.replacements_open() {
                        this.set_all_identities_expanded(false, cx)
                    }
                }),
            )
            .on_action(cx.listener(|this, _: &TogglePreview, window, cx| {
                if this.compare.is_none() {
                    this.toggle_preview(window, cx)
                }
            }))
            .on_action(cx.listener(Self::open))
            .on_action(cx.listener(Self::import))
            .on_action(cx.listener(Self::copy_markdown))
            .on_action(cx.listener(Self::copy_for_ai))
            .on_action(cx.listener(Self::paste_ai_response))
            .on_action(cx.listener(
                |this, _: &crate::document_view::ToggleCompare, window, cx| {
                    this.toggle_compare(window, cx)
                },
            ))
            .on_action(cx.listener(Self::pseudonymize_clipboard))
            .on_action(cx.listener(Self::pseudonymize))
            .on_action(cx.listener(Self::add_pii_candidate))
            .on_action(cx.listener(Self::apply_all_pii))
            .on_action(cx.listener(|this, _: &PiiReviewCandidate, window, cx| {
                this.step_pii_highlight(false, true, window, cx)
            }))
            .on_action(cx.listener(|this, _: &PiiNextCandidate, window, cx| {
                this.step_pii_highlight(false, false, window, cx)
            }))
            .on_action(cx.listener(|this, _: &PiiPreviousCandidate, window, cx| {
                this.step_pii_highlight(true, false, window, cx)
            }))
            .on_action(cx.listener(Self::setup_ocr))
            .on_action(
                cx.listener(|this, _: &RunOcr, window, cx| this.ocr_action(false, window, cx)),
            )
            .on_action(
                cx.listener(|this, _: &ExtractNative, window, cx| {
                    this.ocr_action(true, window, cx)
                }),
            )
            .on_action(cx.listener(|this, _: &DismissImportWarning, _, cx| {
                this.session.warning = None;
                cx.notify();
            }))
            .on_action(cx.listener(|_, _: &New, _, cx| {
                cx.emit(workspace::TabEvent::Tabs(workspace::TabsEvent::New))
            }))
            .on_action(cx.listener(|this, _: &Save, window, cx| this.save(false, None, window, cx)))
            .on_action(
                cx.listener(|this, _: &SaveAs, window, cx| this.save(true, None, window, cx)),
            )
            .on_action(cx.listener(|_, _: &Close, _, cx| {
                cx.emit(workspace::TabEvent::Close(workspace::CloseEvent::Requested))
            }))
            .on_action(cx.listener(|this, _: &ClosePdf, window, cx| {
                this.toggle_preview(window, cx);
                if this.lifecycle.source_only() {
                    window.focus(&this.focus, cx);
                } else {
                    window.focus(&this.editor.read(cx).focus_handle(cx), cx);
                }
                cx.notify();
            }))
            .on_action(cx.listener(Self::find_markdown))
            .on_action(cx.listener(Self::find_replace_markdown))
            .on_action(cx.listener(Self::replace_next_markdown))
            .on_action(cx.listener(Self::replace_all_markdown))
            .on_action(cx.listener(Self::toggle_markdown_match_case))
            .on_action(cx.listener(Self::toggle_markdown_whole_word))
            .on_action(cx.listener(Self::next_misspelling))
            .on_action(cx.listener(Self::previous_misspelling))
            .on_action(cx.listener(Self::toggle_document_spelling))
            .on_action(cx.listener(Self::find_next_markdown))
            .on_action(cx.listener(Self::find_previous_markdown))
            .on_action(cx.listener(Self::close_markdown_search))
            .on_action(cx.listener(Self::retry_preview))
            .on_action(
                cx.listener(|this, _: &ZoomIn, _, cx| this.set_zoom(this.zoom + ZOOM_STEP, cx)),
            )
            .on_action(
                cx.listener(|this, _: &ZoomOut, _, cx| this.set_zoom(this.zoom - ZOOM_STEP, cx)),
            )
            .on_action(cx.listener(|this, _: &ZoomReset, _, cx| this.set_zoom(1., cx)))
            .child(content)
    }
}
