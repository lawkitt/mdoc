//! The document view itself: one tab's view, how it is built and how it
//! draws. Each of its jobs has its own file under `document/`; the
//! window-level [`crate::workspace::Workspace`] owns these views.
use crate::{
    document::search_session::{
        CloseMarkdownSearch, FindNextMarkdown, FindPreviousMarkdown, ReplaceAllMarkdown,
        ReplaceNextMarkdown, SearchRefresh, ToggleMarkdownMatchCase, ToggleMarkdownWholeWord,
    },
    review,
    review::{PiiAddCandidate, PiiNextCandidate, PiiPreviousCandidate, PiiReviewCandidate},
    ui,
    ui::style::{self, Theme},
    workspace::{self, New, RetryDocument},
};
use gpui::{
    App, Context, Entity, Focusable, ScrollHandle, Subscription, Window, actions, div, prelude::*,
    px,
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
use search_session::{chord, search_bar_button, toggled};
use text::find_counter;
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

pub(crate) fn button(label: &'static str, action: impl gpui::Action, theme: Theme) -> ui::Control {
    ui::control(label, label, theme, true)
        .when(cfg!(test), |view| view.debug_selector(move || label.into()))
        .on_click(move |_, window, cx| window.dispatch_action(action.boxed_clone(), cx))
}

/// One zoom step.
pub(crate) const ZOOM_STEP: f32 = 0.05;
/// The widest the Markdown reading column gets at 100%.
const READING_WIDTH: f32 = 760.;

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

impl Render for DocumentView {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        #[cfg(test)]
        RENDERS.with(|renders| renders.set(renders.get() + 1));
        // The before/after view follows the applied replacements.
        self.refresh_compare(window, cx);
        // The Share panel's reply box restores against the live mapping.
        if self.inspector == Inspector::Share && self.replacements_open() {
            self.ensure_reply_box(window, cx);
            self.refresh_reply();
        }
        if self.inspector == Inspector::Notes && self.replacements_open() {
            self.ensure_notes_editor(window, cx);
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
            text::REPLACE,
            chord("⌥⌘F", "Ctrl+H"),
            theme,
        )
        .w(px(20.))
        .aria_expanded(self.search.replace_open())
        .on_click(cx.listener(|this, _, window, cx| this.toggle_markdown_replace(window, cx)));
        let match_case = search_bar_button(
            "markdown-search-match-case",
            "Aa",
            text::MATCH_CASE,
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
            text::WHOLE_WORD,
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
        // each change is announced.
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
            text::PREVIOUS_MATCH,
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
                text::NEXT_MATCH,
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
            text::CLOSE_MARKDOWN_SEARCH,
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
                        .aria_label(text::REPLACE_WITH)
                        .child(self.markdown_replace.clone()),
                )
                .child(
                    search_bar_button(
                        "markdown-replace-next",
                        text::REPLACE,
                        text::REPLACE,
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
                        text::REPLACE_ALL_BUTTON,
                        text::REPLACE_ALL,
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
                            .aria_label(text::SEARCH_MARKDOWN)
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
        let import_notice =
            self.session.warning.clone().map(|detail| {
                self.notice("import-notice", text::REVIEW_CONVERSION, detail, false, cx)
            });
        let ocr_notice = (self.import.ocr_required().is_some()
            && !self.import.ocr_notice_dismissed())
        .then(|| {
            self.notice(
                "ocr-notice",
                text::TEXT_RECOGNITION_REQUIRED,
                format!("{}.", self.ocr_pages_title()),
                false,
                cx,
            )
        });
        let preview_notice = self.preview.message().cloned().map(|detail| {
            self.notice(
                "preview-notice",
                text::PREVIEW,
                detail,
                self.preview.failed(),
                cx,
            )
        });
        let setup_notice = match &self.ocr_state {
            OcrState::Failed(error) if !self.import.setup_error_dismissed() => {
                Some(self.notice("setup-notice", text::OCR_SETUP, error.clone(), true, cx))
            }
            _ => None,
        };
        let clipboard_notice = self
            .clipboard_notice
            .text()
            .map(str::to_owned)
            .map(|detail| self.notice("clipboard-notice", text::CLIPBOARD, detail, false, cx));
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
            // A cancel is not an error: the card says so, and with the card
            // hidden nothing needs attention (#950 review).
            .filter(|_| !card_shows_error && !setup_says_error && !self.conversion_cancelled())
            .map(|detail| self.notice("error-notice", text::NEEDS_ATTENTION, detail, true, cx));
        let refusal_notice = (self.copy_refusal.as_ref())
            .map(|refusal| refusal.reason.clone())
            .map(|detail| self.notice("copy-refusal-notice", text::NOT_COPIED, detail, false, cx));
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
                ui::activity("document-work-activity", if matches!(self.ocr_state, OcrState::Installing) || self.import.setting_up() { text::SETTING_UP_TEXT_RECOGNITION } else if self.import.recognizing() { text::RECOGNIZING_TEXT } else { text::CONVERTING_TO_MARKDOWN }, theme)
                    .px_3().py_2().border_b_1().border_color(palette.border)))
            .when_some(self.session.ocr_configuration.clone(),|v,config|v.child(div().px_3().py_1().text_size(px(11.)).text_color(palette.header_muted).child(text::recognized_by(crate::app::models::short_name(settings::Model::Ocr(config.model)),config.dpi,config.minimum_confidence))))
            .children(import_notice).children(clipboard_notice).children(ocr_notice.filter(|_| !show_markdown))
            .when(narrow_preview, |v| v.child(self.pane_switch(cx))))
            .child(div().flex().flex_1().min_h_0()
                .when(!self.lifecycle.source_only() && show_markdown, |row| row.child(div().relative().flex().flex_1().min_w_0().h_full().flex().flex_col().text_size(px(16. * self.zoom))
                    .child({ let column = self.switcher.column.clone(); gpui::canvas(move |bounds, _, _| column.set(bounds), |_, _, _, _| {}).absolute().inset_0() })
                    .on_mouse_move(cx.listener(|this, event: &gpui::MouseMoveEvent, _, cx| this.switcher_pointer(event.position, cx)))
                    .when(self.search.open(), |column| column.child(search_bar))
                    .child(if self.lifecycle.loading() || self.lifecycle.unavailable() {
                        div().p_6().child(if self.lifecycle.loading() { ui::activity("markdown-loading", text::LOADING_MARKDOWN, theme).into_any_element() } else { div().child(text::MARKDOWN_UNAVAILABLE).into_any_element() })
                            .when(self.lifecycle.unavailable(), |view| view.child(button(text::RETRY, RetryDocument, theme))).into_any_element()
                    } else if let Some(compare) = self.compare_view(content_width, cx) {
                        compare
                    } else { div().relative().flex_1().min_w_0().min_h_0()
                        // Absolute, so the column's intrinsic sizing never measures the
                        // editor: each probe width (search-bar min-content, max content,
                        // 0 px) shaped it again, and a changing one missed every cache.
                        .child(div().id("document-scroll").absolute().inset_0().overflow_y_scroll().track_scroll(&self.scroll).p_6().pb(px(72.))
                            .on_scroll(cx.listener(|this, _, _, cx| { this.wake_switcher(cx); cx.notify(); }))
                            // A click in the blank page under the last line puts the caret
                            // at the end, as in other editors; it used to take focus away
                            // from the editor, so typing went nowhere.
                            .on_mouse_down(gpui::MouseButton::Left, cx.listener(|this, event: &gpui::MouseDownEvent, window, cx| this.click_below_text(event, window, cx)))
                            // A centred reading column.
                            .child(div().relative().w_full().max_w(px(READING_WIDTH * self.zoom)).mx_auto().child(self.editor.clone())
                                // The mention bar scrolls with its mention.
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
                    .child(match self.preview.password() { Some(password) => self.original_password_card(password, window, cx), None => self.preview_status(theme) })))
                .when_some(self.preview.pdf().cloned().filter(|_| show_original), |row, pdf| row.child(div().track_focus(&self.preview_region).on_action(cx.listener(Self::next_comment)).on_action(cx.listener(Self::previous_comment)).when(!narrow_preview, |v| v.w(px(content_width * (1. - split)))).when(narrow_preview, |v| v.flex_1()).flex_shrink_0().min_w_0().h_full().flex().flex_col().border_l_1().border_color(palette.border)
                    .child(div().flex_1().min_h_0().child(if pdf.read(cx).is_locked() { div().p_6().child(text::PDF_PASSWORD_PROTECTED).into_any_element() } else { pdf.into_any_element() }))
                    .children(preview_notice)))
)
            .children(setup_notice.filter(|_| !(show_markdown && self.lifecycle.source_only() && self.import.ocr_required().is_some()))).children(error_notice).children(refusal_notice)
            // The word popup anchors to the editor, hidden under before/after.
            .children(self.pii_popup(cx).filter(|_| self.compare.is_none()))
            .children(self.replacement_tip(window, cx).filter(|_| self.compare.is_none()));
        // Sidebar collapsed: the header joins the title row and the document
        // sits in its own card below it.
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
            // regions.
            .track_focus(&self.focus)
            .on_action(cx.listener(Self::toggle_theme))
            .on_action(
                cx.listener(|this, _: &crate::review::ExpandAllIdentities, _, cx| {
                    if this.replacements_open() {
                        this.set_all_identities_expanded(true, cx)
                    }
                }),
            )
            .on_action(
                cx.listener(|this, _: &crate::review::CollapseAllIdentities, _, cx| {
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
            .on_action(
                cx.listener(|this, _: &crate::document::ToggleCompare, window, cx| {
                    this.toggle_compare(window, cx)
                }),
            )
            // View › Edit, Original and Pseudonymized: the view switcher's
            // segments.
            .on_action(cx.listener(|this, _: &ShowEditView, window, cx| {
                this.set_view_mode(None, window, cx)
            }))
            .on_action(cx.listener(|this, _: &ShowOriginalView, window, cx| {
                this.set_view_mode(Some(crate::review::CompareMode::Original), window, cx)
            }))
            .on_action(cx.listener(|this, _: &ShowPseudonymizedView, window, cx| {
                this.set_view_mode(Some(crate::review::CompareMode::Pseudonymized), window, cx)
            }))
            .on_action(cx.listener(|this, _: &ShowReplacements, window, cx| {
                this.show_replacements(window, cx)
            }))
            .on_action(cx.listener(|this, _: &ShowNotes, window, cx| {
                if this.can_copy_markdown() && this.notes_available() {
                    this.show_notes(window, cx)
                }
            }))
            .on_action(cx.listener(|this, _: &OpenSourceReply, _, cx| this.open_source_reply(cx)))
            .on_action(cx.listener(Self::pseudonymize_clipboard))
            .on_action(cx.listener(Self::restore_clipboard))
            .on_action(cx.listener(Self::pseudonymize))
            .on_action(cx.listener(Self::add_pii_candidate))
            .on_action(cx.listener(Self::add_pii_candidate_as))
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
            .on_action(cx.listener(|this, _: &Save, _, cx| this.save_now(cx)))
            .on_action(
                cx.listener(|this, _: &ExportMarkdown, window, cx| {
                    this.export_markdown(window, cx)
                }),
            )
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
