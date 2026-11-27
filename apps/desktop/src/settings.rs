//! Settings: the panel and its sections (General, Models, AI connector, the Vault,
//! categories, the dictionary and the lists), model downloads and their status, and
//! saving each change.
use crate::{app::models, document::markdown_search::SearchInput, ui::style::Theme};
use gpui::{App, Context, Entity, FocusHandle, KeyBinding, Window, actions, prelude::*, px};
use mdoc_import::{
    model_download::{Pending, Progress},
    ocr,
};
use mdoc_store::model_state::ModelState;
pub use mdoc_store::model_state::Status;
use mdoc_store::session_store::ThemeChoice;
use mdoc_store::settings::{Model, Preferences, Shared};
use std::{cell::Cell, rc::Rc};

mod actions;
mod ai_connector;
pub(crate) mod dictionary;
pub(crate) mod lists;
mod model_setup;
mod state;
mod text;
mod vault;
mod view;
pub(crate) use state::Category;
pub(crate) use view::block_document_commands;
pub use view::control;

actions!(
    model_settings,
    [
        CloseSettings,
        NextSettingsField,
        PreviousSettingsField,
        NextCategory,
        PreviousCategory,
        CommitNumber
    ]
);
pub enum Event {
    /// The window owns and persists the theme choice.
    ThemeSelected(ThemeChoice),
    Applied,
    Checked,
    Compare(Model),
    Finished(Model, Result<Option<ocr::Installed>, String>),
    /// Restart now: quit through the workspace, then relaunch.
    RestartToUpdate,
    Shortcuts,
    /// Saved: Settings › Detection stopped detecting these.
    CategoriesUnticked(std::collections::HashSet<mdoc_pii::Category>),
    /// The user-wide lists changed: save them in the Vault.
    UserListsChanged(mdoc_store::lists::Lists),
    /// The lists wait for the locked Vault: unlock it.
    UnlockLists,
    /// Settings › Vault's Show: read the key and hand it back with
    /// [`Panel::show_recovery_key`].
    ShowRecoveryKey,
    /// "AI connector (preview)" as the page shows it, ahead of its save:
    /// off stops the connector at once, and a save that fails turns it
    /// back on (#1220). On starts with [`Event::Applied`].
    AiConnectorShown(bool),
}

pub struct Panel {
    pub open: bool,
    pub shared: Shared,
    pub statuses: Vec<Status>,
    /// Persisted labels are never inputs to runtime readiness or admission.
    row_statuses: Option<Vec<Status>>,
    model_state: ModelState,
    #[cfg(test)]
    model_files: Option<[Vec<std::path::PathBuf>; 4]>,
    #[cfg(test)]
    check_blocked: bool,
    pub ocr_installations: Vec<ocr::Installed>,
    /// Bytes each model in `Model::ALL` still needs, measured cheaply by size.
    pub pending: Vec<Pending>,
    pub draft: Preferences,
    pub threshold: Entity<SearchInput>,
    pub confidence: Entity<SearchInput>,
    pub error: Option<String>,
    save_error: Option<String>,
    pub applying: bool,
    pub working: Option<Model>,
    progress: Option<Progress>,
    category: Category,
    category_focus: FocusHandle,
    number_errors: [Option<String>; 2],
    number_subscriptions: Vec<gpui::Subscription>,
    save_pending: bool,
    /// "AI connector (preview)" as the save in flight writes it (#1273).
    saving_preview: Option<bool>,
    reset_theme: Option<u64>,
    theme_revision: u64,
    #[cfg(test)]
    save_gate: Option<gpui::Task<()>>,
    pub details: Option<Model>,
    theme: Rc<Cell<Theme>>,
    /// The window's current choice, projected into this control.
    theme_choice: ThemeChoice,
    #[cfg(all(
        debug_assertions,
        feature = "headless-renders",
        any(target_os = "macos", target_os = "windows")
    ))]
    appearance_bounds: Rc<Cell<Option<[gpui::Bounds<gpui::Pixels>; 3]>>>,
    focus: FocusHandle,
    previous: Option<FocusHandle>,
    checks: u64,
    refresh_watch: Option<gpui::Task<()>>,
    /// Another app job holding the model slot, as `refresh_watch` last saw
    /// it. Read from the process-wide slot only there, so the layout
    /// changes on that notify, never in the middle of a click.
    other_work: Option<&'static str>,
    check_pending: bool,
    checking: bool,
    scroll: gpui::ScrollHandle,
    controls: std::cell::RefCell<std::collections::HashMap<gpui::ElementId, FocusHandle>>,
    /// The user dictionary view, replacing the Settings body while open.
    pub dictionary: Option<dictionary::DictionaryView>,
    /// Settings › Detection's user-wide Always and Never lists.
    user_lists: lists::UserListsUi,
    /// Settings › Vault's recovery key, while it shows.
    recovery: vault::RecoveryUi,
    /// Settings › AI connector's Claude Desktop state and consent card.
    ai: ai_connector::AiConnectorUi,
}
impl gpui::EventEmitter<Event> for Panel {}
pub fn bind_keys(cx: &mut App) {
    let bindings =
        crate::commands::shortcuts::bindings(crate::commands::shortcuts::Owner::Settings, cx);
    cx.bind_keys(bindings);
    cx.bind_keys([
        KeyBinding::new("tab", NextSettingsField, Some("ModelSettings")),
        KeyBinding::new("shift-tab", PreviousSettingsField, Some("ModelSettings")),
        KeyBinding::new("down", NextCategory, Some("SettingsCategories")),
        KeyBinding::new("up", PreviousCategory, Some("SettingsCategories")),
        KeyBinding::new("enter", CommitNumber, Some("SettingsNumber")),
    ]);
    dictionary::bind_keys(cx);
}
fn field(
    label: &'static str,
    value: &str,
    theme: Rc<Cell<Theme>>,
    cx: &mut Context<Panel>,
) -> Entity<SearchInput> {
    cx.new(|cx| {
        let mut input = SearchInput::new(cx)
            .with_theme(theme)
            .with_key_context("SettingsInput")
            .with_placeholder(text::NUMBER_PLACEHOLDER)
            .with_label(label);
        input.set_value(value.into(), cx);
        input
    })
}

impl Panel {
    pub(crate) fn category(&self) -> Category {
        self.category
    }

    pub fn new(shared: Shared, theme: Rc<Cell<Theme>>, cx: &mut Context<Self>) -> Self {
        let user_lists = lists::UserListsUi::new(theme.clone(), cx);
        let draft = shared.borrow().current.clone().unwrap_or_default();
        let model_state = ModelState::open(shared.borrow().path.as_deref());
        Self {
            open: false,
            shared,
            statuses: vec![Status::Unknown; Model::ALL.len()],
            row_statuses: None,
            model_state,
            #[cfg(test)]
            model_files: None,
            #[cfg(test)]
            check_blocked: false,
            ocr_installations: Vec::new(),
            pending: Model::ALL.map(models::pending).to_vec(),
            threshold: field(
                text::DETECTION_THRESHOLD,
                &draft.pseudonymization.threshold.to_string(),
                theme.clone(),
                cx,
            ),
            confidence: field(
                text::OCR_MINIMUM_CONFIDENCE,
                &draft.ocr.minimum_confidence.to_string(),
                theme.clone(),
                cx,
            ),
            draft,
            error: None,
            save_error: None,
            applying: false,
            working: None,
            progress: None,
            category: Category::General,
            category_focus: cx.focus_handle().tab_stop(true),
            number_errors: [None, None],
            number_subscriptions: Vec::new(),
            save_pending: false,
            saving_preview: None,
            reset_theme: None,
            theme_revision: 0,
            #[cfg(test)]
            save_gate: None,
            details: None,
            theme,
            theme_choice: ThemeChoice::default(),
            #[cfg(all(
                debug_assertions,
                feature = "headless-renders",
                any(target_os = "macos", target_os = "windows")
            ))]
            appearance_bounds: Rc::default(),
            focus: cx.focus_handle(),
            previous: None,
            checks: 0,
            refresh_watch: None,
            other_work: None,
            check_pending: false,
            checking: false,
            scroll: gpui::ScrollHandle::new(),
            controls: Default::default(),
            dictionary: None,
            user_lists,
            recovery: vault::RecoveryUi::default(),
            ai: ai_connector::AiConnectorUi::default(),
        }
    }
    #[cfg(all(
        debug_assertions,
        feature = "headless-renders",
        any(target_os = "macos", target_os = "windows")
    ))]
    pub(crate) fn selected_theme(&self) -> ThemeChoice {
        self.theme_choice
    }
    /// Show some Categories to detect off, unsaved, with keyboard focus on
    /// the chip `token`, for the Detection render.
    #[cfg(all(
        debug_assertions,
        feature = "headless-renders",
        any(target_os = "macos", target_os = "windows")
    ))]
    pub(crate) fn render_categories(
        &mut self,
        off: &[mdoc_pii::Category],
        token: &str,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.draft.categories = self
            .shared
            .borrow()
            .current
            .as_ref()
            .map(|p| p.categories.clone())
            .unwrap_or_default();
        for &category in off {
            self.draft.categories.set(category, false);
        }
        let id =
            gpui::ElementId::from(gpui::SharedString::from(format!("settings-detect-{token}")));
        if let Some(focus) = self.controls.borrow().get(&id) {
            window.focus(focus, cx);
        }
        cx.notify();
    }
    /// Show these model statuses, as if a check had found them, for the
    /// installed-model renders; returns the previous ones to restore.
    #[cfg(all(
        debug_assertions,
        feature = "headless-renders",
        any(target_os = "macos", target_os = "windows")
    ))]
    pub(crate) fn render_statuses(
        &mut self,
        statuses: Vec<Status>,
        cx: &mut Context<Self>,
    ) -> Vec<Status> {
        self.row_statuses = None;
        cx.notify();
        std::mem::replace(&mut self.statuses, statuses)
    }
    /// Show the saved preferences with `change` applied, unsaved, for the
    /// switch renders; `|_| {}` shows the saved ones again.
    #[cfg(all(
        debug_assertions,
        feature = "headless-renders",
        any(target_os = "macos", target_os = "windows")
    ))]
    pub(crate) fn render_draft(
        &mut self,
        change: impl FnOnce(&mut Preferences),
        cx: &mut Context<Self>,
    ) {
        self.draft = self.shared.borrow().current.clone().unwrap_or_default();
        change(&mut self.draft);
        cx.notify();
    }
    /// Scroll the page to its end, for the long-page render.
    #[cfg(all(
        debug_assertions,
        feature = "headless-renders",
        any(target_os = "macos", target_os = "windows")
    ))]
    pub(crate) fn render_scroll_to_end(&mut self, cx: &mut Context<Self>) {
        let end = self.scroll.max_offset().y;
        self.scroll.set_offset(gpui::point(px(0.), -end));
        cx.notify();
    }
    /// The focus handle of the Settings control `id`, once it has drawn.
    #[cfg(all(
        debug_assertions,
        feature = "headless-renders",
        any(target_os = "macos", target_os = "windows")
    ))]
    pub(crate) fn control_focus(&self, id: &'static str) -> Option<FocusHandle> {
        self.controls
            .borrow()
            .get(&gpui::ElementId::from(id))
            .cloned()
    }
    /// Geometry from the three actual segments, cleared before each render:
    /// an omitted section or a choice below the fold cannot pass the contract.
    #[cfg(all(
        debug_assertions,
        feature = "headless-renders",
        any(target_os = "macos", target_os = "windows")
    ))]
    pub(crate) fn appearance_in_view(&self) -> bool {
        self.open
            && self.dictionary.is_none()
            && self.appearance_bounds.get().is_some_and(|segments| {
                segments.iter().all(|bounds| {
                    bounds.size.width > px(0.)
                        && bounds.size.height > px(0.)
                        && bounds.is_contained_within(&self.scroll.bounds())
                })
            })
    }
}
impl gpui::Focusable for Panel {
    fn focus_handle(&self, _: &App) -> FocusHandle {
        self.focus.clone()
    }
}

impl Panel {}

impl Render for Panel {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        self.category_render(window, cx)
    }
}

#[cfg(test)]
mod tests;
