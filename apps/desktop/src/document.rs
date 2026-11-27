//! The document view itself: one tab's view and how it is built. Each of its
//! jobs has its own file under `document/`, its drawing `view.rs`; the
//! window-level [`crate::workspace::Workspace`] owns these views.
use crate::{
    document::search_session::SearchRefresh,
    review,
    review::PiiAddCandidate,
    ui::style::{self, Theme},
    workspace,
};
use gpui::{
    App, Context, Entity, Focusable, ScrollHandle, Subscription, Window, actions, prelude::*,
};
use mdoc_editor::{EditorEvent, EditorState};
use mdoc_import::{import, ocr, recognition};
use mdoc_store::{document, document_session, settings};
use std::{
    cell::Cell,
    path::PathBuf,
    rc::Rc,
    sync::{Arc, atomic::AtomicBool},
};

mod ask;
pub(crate) mod comment_rail;
pub(crate) mod comparison;
pub(crate) mod comparison_ui;
pub(crate) mod images;
pub(crate) mod markdown_search;
pub(crate) mod notes;
pub(crate) mod pdf_comments;
pub(crate) mod preview;
mod project;
mod project_link;
pub(crate) mod search_session;
pub(crate) mod spelling;
use project_link::ProjectEffect;
mod chrome;
mod decision_history;
mod document_lifecycle;
mod import_flow;
#[cfg(test)]
pub(crate) use import_flow::PasswordPrompt;
pub(crate) use import_flow::known_ocr_state;
mod autosave;
mod copy_markdown;
mod files;
#[cfg(test)]
use text::chooser_unavailable;
mod status;
mod text;
pub(crate) use search_session::bind_markdown_search_keys;
#[cfg(test)]
use text::find_counter;
#[cfg(test)]
pub(crate) mod tests;
mod view;
actions!(
    mdoc,
    [
        SetupOcr,
        RunOcr,
        ExtractNative,
        DismissImportWarning,
        Save,
        ExportMarkdown,
        CopyMarkdown,
        ToggleCompare,
        ShowEditView,
        ShowOriginalView,
        ShowPseudonymizedView,
        ShowReplacements,
        ShowNotes,
        OpenSourceReply,
        ZoomIn,
        ZoomOut,
        ZoomReset,
        ClosePdf,
        TogglePreview,
    ]
);

/// The Inspector's tabs.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(crate) enum Inspector {
    #[default]
    Review,
    Share,
    /// The document's private notes.
    Notes,
}

#[derive(Clone)]
pub(crate) enum Next {
    Import(Box<import::Imported>),
}

/// The folder of the last file imported or exported this run, for Export
/// Markdown to start in when a document has no folder of its own (#180).
/// Kept in memory only: nothing about it is written to disk.
#[derive(Default)]
pub(crate) struct LastFolder(Option<PathBuf>);

impl gpui::Global for LastFolder {}

impl LastFolder {
    /// `file` was imported or exported: its folder is the last one.
    pub(crate) fn note(file: &std::path::Path, cx: &mut App) {
        if let Some(folder) = file.parent().filter(|f| !f.as_os_str().is_empty()) {
            cx.set_global(Self(Some(folder.to_path_buf())));
        }
    }

    pub(crate) fn get(cx: &App) -> Option<&std::path::Path> {
        cx.try_global::<Self>().and_then(|last| last.0.as_deref())
    }
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
    /// F6 regions of the document; the editor and the
    /// replacements panel bring their own handles.
    pub(crate) toolbar_region: gpui::FocusHandle,
    /// Controls outside the toolbar that belong to its region: the notices
    /// strip with the pane switch, and the view switcher.
    pub(crate) controls_region: gpui::FocusHandle,
    pub(crate) switcher_region: gpui::FocusHandle,
    /// The find bar, between the toolbar and the editor in Tab order.
    pub(crate) find_region: gpui::FocusHandle,
    pub(crate) preview_region: gpui::FocusHandle,
    pub(crate) comments_region: gpui::FocusHandle,
    /// The toolbar's More button, which its menu returns focus to.
    pub(crate) more_focus: gpui::FocusHandle,
    /// The open More menu, which holds focus and Tab.
    pub(crate) more_menu_focus: gpui::FocusHandle,
    /// The More button's bounds, so a press on it is left to its toggle.
    pub(crate) more_bounds: std::rc::Rc<std::cell::Cell<gpui::Bounds<gpui::Pixels>>>,
    /// Share's Move to Project… button, which its menu opens under.
    pub(crate) share_move_bounds: std::rc::Rc<std::cell::Cell<gpui::Bounds<gpui::Pixels>>>,
    /// Whether the last frame drew the Markdown and the original panes: a
    /// narrow window shows one of them (F6 skips the hidden one).
    panes_drawn: std::cell::Cell<(bool, bool)>,
    /// Whether the window is resolving a close or quit (read-only).
    closing: workspace::CloseWatch,
    /// Imports and DOCX jobs still running, handed to the window if the tab
    /// closes first so it still learns when they end.
    background: Vec<gpui::Task<Background>>,
    pub(crate) active: bool,
    /// Loading, availability, conversion, unsaved changes and dialogs.
    pub(crate) lifecycle: document_lifecycle::DocumentLifecycle,
    /// External files over the empty page: whether any is supported.
    pub(crate) file_drag: Option<bool>,
    pub(crate) preferences: settings::Shared,
    pub(crate) model_panel: Entity<crate::settings::Panel>,
    pub(crate) theme: Rc<Cell<Theme>>,
    pub(crate) title: style::TitleBar,
    pub(crate) editor: Entity<EditorState>,
    pub(crate) images: images::ImageCache,
    pub(crate) session: document_session::DocumentSession,
    pub(crate) pii: review::ReviewUi,
    pub(crate) spelling: spelling::SpellState,
    pub(crate) preview: preview::PreviewState,
    pub(crate) scroll: ScrollHandle,
    pub(crate) error: Option<String>,
    pub(crate) copy_feedback: Option<gpui::Task<()>>,
    /// The header's ⋯ menu.
    pub(crate) more_open: bool,
    /// The header's Ask menu and its Unfiled card (Milestone 2 item 10).
    pub(crate) ask: ask::AskHeader,
    /// The header's What you did popover.
    pub(crate) history: decision_history::HistoryPopover,
    /// Text zoom of this document's Markdown, not saved.
    pub(crate) zoom: f32,
    /// The Inspector's tab showing.
    pub(crate) inspector: Inspector,
    /// The document's private notes and their autosave.
    pub(crate) notes: notes::NotesState,
    /// The AI Reply it was promoted from, when it was.
    pub(crate) from_reply: Option<notes::FromReply>,
    /// The running copy feedback belongs to Copy for AI, not Copy Markdown.
    pub(crate) copied_for_ai: bool,
    /// Why the last Copy for AI copied nothing, and its step: the "Not
    /// copied" notice, until dismissed, a copy or its step.
    pub(crate) copy_refusal: Option<crate::review::refusal::Refusal>,
    /// The running copy feedback belongs to Copy reply.
    pub(crate) copied_reply: bool,
    /// The tab's AI reply and its restoration.
    pub(crate) reply: review::ReplyBox,
    /// The Share panel's reply box, made when Share first draws.
    pub(crate) reply_box: Option<(Entity<EditorState>, Subscription)>,
    /// The reply box's scroll, which keeps its caret in view.
    pub(crate) reply_scroll: gpui::ScrollHandle,
    /// Saves the box's reply once it settles.
    pub(crate) reply_save: Option<gpui::Task<()>>,
    /// The saved replies Share lists: the Project's, or this document's own
    /// while Unfiled; the workspace keeps them current.
    pub(crate) saved_replies: Vec<review::SavedReplyRow>,
    /// The Project this tab's mapping belongs to.
    pub(crate) project: project_link::ProjectState,
    /// The waiting fold of the mapping into the Project.
    project_save: Option<gpui::Task<()>>,
    /// The waiting autosave of the document.
    autosave: Option<gpui::Task<()>>,
    pub(crate) project_names_open: bool,
    pub(crate) project_term: Option<Entity<markdown_search::SearchInput>>,
    /// The Project page's name field, shown in the header's breadcrumb
    /// right after Move to New Project (#1313); the workspace owns it.
    pub(crate) project_name_field: Option<Entity<markdown_search::SearchInput>>,
    /// How many Projects the window holds: Share offers Move to Project…
    /// once one besides this document's own exists (#1312).
    pub(crate) projects: Rc<Cell<usize>>,
    /// The result of Pseudonymize Clipboard, for a few seconds.
    pub(crate) clipboard_notice: review::restore::ClipboardNotice,
    /// The before/after view replaces the editor while open.
    pub(crate) compare: Option<review::CompareState>,
    /// The floating view pill fades when idle (README media decision 9).
    pub(crate) switcher: review::SwitcherRest,
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
    /// The conversion card's field for a locked document's password: what
    /// it holds goes to one conversion on Unlock and is wiped.
    pub(crate) password_field: Entity<markdown_search::SearchInput>,
    /// The Original's field for a reopened locked DOCX's password: what it
    /// holds unlocks the tab's source on Unlock and is wiped.
    pub(crate) original_password_field: Entity<markdown_search::SearchInput>,
    /// The Original's password field takes focus once, after a wrong
    /// password.
    pub(crate) focus_original_card: bool,
    /// The Replace row's field; its text stays with the tab.
    pub(crate) markdown_replace: Entity<markdown_search::SearchInput>,
    pub(crate) search: search_session::SearchSession,
    pub(crate) ocr_setup_subscription: Option<Subscription>,
    pub(crate) _subscription: Subscription,
    pub(crate) _markdown_search_subscription: Subscription,
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
    pub(crate) model_panel: Entity<crate::settings::Panel>,
    pub(crate) theme: Rc<Cell<Theme>>,
    pub(crate) import_busy: Arc<AtomicBool>,
    pub(crate) ocr: OcrState,
    pub(crate) title: style::TitleBar,
    /// Keyboard input source languages (`recognition::input_source_languages`).
    pub(crate) input_languages: fn() -> Vec<recognition::Language>,
    /// How many Projects the window holds (`Projects::count`).
    pub(crate) projects: Rc<Cell<usize>>,
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
                .with_placeholder(text::REPLACE_WITH)
        });
        let password_field = cx.new(|cx| {
            markdown_search::SearchInput::new(cx)
                .with_theme(dependencies.theme.clone())
                .with_key_context("DocumentPassword")
                .with_placeholder(text::PASSWORD)
                .secure()
        });
        let original_password_field = cx.new(|cx| {
            markdown_search::SearchInput::new(cx)
                .with_theme(dependencies.theme.clone())
                .with_key_context("DocumentPassword")
                .with_placeholder(text::PASSWORD)
                .secure()
        });
        let scroll = ScrollHandle::new();
        let editor = cx.new(|cx| {
            // Ghost text at the caret; the empty page centers the rest.
            let mut editor = EditorState::new(window, cx).with_placeholder(text::TYPE_HERE);
            editor.set_markdown_style(style::markdown_style(dependencies.theme.get()), cx);
            editor.set_block_chip_provider(|src| {
                gpui_pdf::is_pdf(src).then(|| src.to_owned().into())
            });
            spelling::install_editor_hooks(&mut editor, dependencies.preferences.clone());
            editor.set_autoscroll(scroll.clone());
            editor
        });
        let images = images::install(
            &editor,
            document_session::DocumentSession::default().directory(),
            cx,
        );
        let view = cx.weak_entity();
        editor.update(cx, |editor, _| {
            crate::review::check_hand_copies(editor, view, crate::review::CopyFrom::Editor)
        });
        window.focus(&editor.read(cx).focus_handle(cx), cx);
        let subscription =
            cx.subscribe_in(&editor, window, |this, _, event, window, cx| match event {
                EditorEvent::Transaction(transaction) => this.pii_transaction(transaction, cx),
                EditorEvent::Changed => {
                    this.copy_feedback = None;
                    this.clipboard_notice.clear();
                    this.pii_edited(cx);
                    this.schedule_spellcheck(true, cx);
                    this.schedule_project_save(cx);
                    this.lifecycle.edited();
                    this.schedule_autosave(cx);
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
                EditorEvent::SelectionActionChoice => this.toggle_selection_menu(window, cx),
                EditorEvent::CopyAction { range, label } => {
                    let editor = this.editor.clone();
                    let from = crate::review::CopyFrom::Editor;
                    this.copy_action(from, &editor, range.clone(), label, cx)
                }
                // A hovered replacement shows its tooltip.
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
            share_move_bounds: Default::default(),
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
            pii: review::ReviewUi::new(dependencies.theme.clone(), cx),
            spelling: spelling::SpellState::default(),
            preview: preview::PreviewState::default(),
            scroll,
            error: None,
            copy_feedback: None,
            more_open: false,
            ask: ask::AskHeader::new(cx),
            history: decision_history::HistoryPopover::new(cx),
            zoom: 1.,
            inspector: Inspector::Review,
            notes: Default::default(),
            from_reply: None,
            copied_for_ai: false,
            copy_refusal: None,
            copied_reply: false,
            reply: Default::default(),
            reply_box: None,
            reply_scroll: gpui::ScrollHandle::new(),
            reply_save: None,
            saved_replies: Vec::new(),
            project: Default::default(),
            project_save: None,
            autosave: None,
            project_names_open: false,
            project_term: None,
            project_name_field: None,
            projects: dependencies.projects.clone(),
            clipboard_notice: Default::default(),
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
            password_field,
            original_password_field,
            focus_original_card: false,
            markdown_replace,
            search: search_session::SearchSession::default(),
            ocr_setup_subscription: None,
            _subscription: subscription,
            _markdown_search_subscription: markdown_search_subscription,
        }
    }

    /// This tab has a completed scan or replacement data, not just an open review.
    pub(crate) fn pseudonymized(&self) -> bool {
        !self.pii.scans().is_empty()
            || !self.pii.review().variants().is_empty()
            || !self.pii.review().applied().is_empty()
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
}

/// How long typing pauses before a document saves itself.
const AUTOSAVE_DELAY: std::time::Duration = std::time::Duration::from_millis(700);

/// One zoom step.
pub(crate) const ZOOM_STEP: f32 = 0.05;

/// Jobs still running when a document goes without closing its tab (the
/// window closing) keep running to their end, as before the one channel.
impl Drop for DocumentView {
    fn drop(&mut self) {
        for task in self.background.drain(..) {
            task.detach();
        }
    }
}

#[cfg(test)]
thread_local! {
    /// How many times a document view rendered on this thread: tests use it
    /// to see the workspace reuse a cached frame.
    pub(crate) static RENDERS: Cell<usize> = const { Cell::new(0) };
}
