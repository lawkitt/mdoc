//! The window shell (ADR 0048): theme choice, the sidebar's top and foot,
//! Home with its drop zone, and the ⌘K command palette.
use super::{Tab, Workspace};
use crate::{
    document_view::{CopyMarkdown, DocumentView, RecentFolders, Save, SaveAs, ToggleCompare},
    markdown_search::{SearchInput, SearchInputEvent},
    pii::ui::{CopyForAi, Pseudonymize},
    shortcuts,
    style::{self, Theme},
    ui,
    workspace::{
        Close, New, Open, OpenCase, PasteAiResponse, PseudonymizeClipboard, Settings,
        ToggleSidebar, shortcuts_ui::ShowShortcuts,
    },
};
use gpui::{
    Animation, AnimationExt, AnyElement, App, Context, Entity, Focusable, Hsla, KeyBinding,
    SharedString, Subscription, Window, actions, div, prelude::*, px,
};
use mdoc_import::import;
use mdoc_store::recents::{self, Recents};
use mdoc_store::session_store::ThemeChoice;
use std::{path::PathBuf, time::Duration};

actions!(
    mdoc_shell,
    [
        OpenPalette,
        PaletteUp,
        PalettePrevious,
        PaletteDown,
        PaletteConfirm,
        PaletteClose,
        ShowHome,
        PasteAsNewTab,
        CloseNotices,
    ]
);

pub(crate) const PALETTE: &str = "CommandPalette";

/// The palette, Home, Notices and the shortcuts sheet from the shortcuts
/// table (ADR 0073); moving inside the palette's list stays here. Esc in
/// `Workspace` is the window's own, below every deeper one: Notices opens
/// without focus (ADR 0062), so Esc closes it from a source-only tab or Home.
pub(crate) fn bind_keys(cx: &mut App) {
    cx.bind_keys(crate::shortcuts::bindings(crate::shortcuts::Owner::Shell));
    cx.bind_keys([
        KeyBinding::new("up", PaletteUp, Some(PALETTE)),
        KeyBinding::new("down", PaletteDown, Some(PALETTE)),
        KeyBinding::new("enter", PaletteConfirm, Some(PALETTE)),
    ]);
}

pub(super) struct ShellState {
    pub recents: Recents,
    pub palette: Option<Palette>,
    /// The keyboard shortcuts sheet, while open (ADR 0073).
    pub shortcuts: Option<super::shortcuts_ui::ShortcutsSheet>,
    /// Files dragged over the window while Home shows: how many are droppable.
    pub drop: Option<usize>,
    pub drop_generation: u64,
    home_scroll: gpui::ScrollHandle,
    /// Focus for each Recents row on Home, by position.
    pub recent_focus: std::cell::RefCell<Vec<gpui::FocusHandle>>,
}

impl ShellState {
    pub fn new(cx: &mut App) -> Self {
        let recents = Recents::load(if cfg!(test) {
            None
        } else {
            Recents::default_path()
        });
        share_recents(&recents, cx);
        Self {
            recents,
            palette: None,
            shortcuts: None,
            drop: None,
            drop_generation: 0,
            recent_focus: Default::default(),
            home_scroll: gpui::ScrollHandle::new(),
        }
    }
}

/// Recents changed: Save As's folders (#180) and File › Open Recent follow.
fn share_recents(recents: &Recents, cx: &mut App) {
    RecentFolders::share(recents.folders(), cx);
    let files = recents.files.iter().map(|recent| recent.path.clone());
    crate::menus::set_recent_files(files.collect(), cx);
}

pub(super) struct Palette {
    input: Entity<SearchInput>,
    selected: usize,
    return_focus: Option<gpui::FocusHandle>,
    _changes: Subscription,
}

/// What a palette row does when chosen.
#[derive(Clone)]
enum Command {
    NewTab,
    OpenFiles,
    PasteText,
    Pseudonymize,
    PseudonymizeClipboard,
    RestoreReply,
    CopyForAi,
    CopyMarkdown,
    Save,
    SaveAs,
    CloseTab,
    BeforeAfter,
    /// The active document's What you did (ADR 0079); no key of its own.
    DecisionHistory,
    ExpandIdentities,
    CollapseIdentities,
    ToggleSidebar,
    OpenCase,
    Home,
    Settings,
    SettingsCategory(crate::settings_ui::Category),
    Shortcuts,
    Theme,
    /// The active document's heading whose line starts at this offset.
    Heading(usize),
    Tab(u64),
    Case(String),
    Recent(PathBuf),
}

impl Command {
    /// The action this command runs, when it is one: the palette shows its
    /// key from the shortcuts table (ADR 0073).
    fn action(&self) -> Option<Box<dyn gpui::Action>> {
        Some(match self {
            Command::NewTab => Box::new(New),
            Command::OpenFiles => Box::new(Open),
            Command::Pseudonymize => Box::new(Pseudonymize),
            Command::PseudonymizeClipboard => Box::new(PseudonymizeClipboard),
            Command::RestoreReply => Box::new(PasteAiResponse),
            Command::CopyForAi => Box::new(CopyForAi),
            Command::CopyMarkdown => Box::new(CopyMarkdown),
            Command::Save => Box::new(Save),
            Command::SaveAs => Box::new(SaveAs),
            Command::CloseTab => Box::new(Close),
            Command::BeforeAfter => Box::new(ToggleCompare),
            Command::ToggleSidebar => Box::new(ToggleSidebar),
            Command::OpenCase => Box::new(OpenCase),
            Command::Home => Box::new(ShowHome),
            Command::Settings => Box::new(Settings),
            Command::Shortcuts => Box::new(ShowShortcuts),
            Command::PasteText
            | Command::DecisionHistory
            | Command::ExpandIdentities
            | Command::CollapseIdentities
            | Command::Theme
            | Command::SettingsCategory(_)
            | Command::Heading(_)
            | Command::Tab(_)
            | Command::Case(_)
            | Command::Recent(_) => return None,
        })
    }

    /// The key that runs this command on this platform, if any.
    fn shortcut(&self) -> Option<String> {
        self.action().and_then(|action| shortcuts::label(&*action))
    }
}

struct Row {
    section: &'static str,
    label: String,
    detail: Option<String>,
    command: Command,
}

/// The key `action` runs from, as the shortcuts table writes it.
fn key(action: &dyn gpui::Action) -> String {
    shortcuts::label(action).unwrap_or_default()
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
        self.apply_theme(window, cx);
        self.checkpoint(window, cx);
        cx.notify();
    }

    // ---- Home state -------------------------------------------------------

    /// The hidden blank tab under Home: never edited, never listed.
    pub(super) fn is_placeholder(&self, tab: &Tab, cx: &App) -> bool {
        tab.record().blank_disposable
            && tab.view().is_none_or(|view| {
                let view = view.read(cx);
                view.lifecycle.blank_disposable()
                    && view.session.document.path.is_none()
                    && view.preview.source().is_none()
                    && view.editor.read(cx).text().is_empty()
            })
    }

    pub(super) fn showing_home(&self, cx: &App) -> bool {
        self.cases.page.is_none()
            && (self.tabs.home()
                || self
                    .tabs
                    .active_tab()
                    .is_some_and(|tab| self.is_placeholder(tab, cx)))
    }

    pub(super) fn showing_overview(&self, cx: &App) -> bool {
        self.cases.page.is_some() || self.showing_home(cx)
    }

    pub(super) fn show_home(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.show_home_as(super::navigation::Arrival::Visit, window, cx);
    }

    /// Show Home, recording it in Back/Forward as `arrival` says.
    pub(super) fn show_home_as(
        &mut self,
        arrival: super::navigation::Arrival,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if let Some(old) = self.active_view() {
            old.update(cx, |view, _| view.pii.dismiss_popup());
        }
        self.cases.page = None;
        self.end_rename(true, cx);
        self.tabs.show_home();
        self.note_destination(arrival, cx);
        window.focus(&self.focus, cx);
        cx.notify();
    }

    /// Remember files as they open or save (paths only).
    pub(super) fn remember_recent(&mut self, id: u64, cx: &mut App) {
        let Some(view) = self
            .tabs
            .get(id)
            .and_then(|tab| tab.view())
            .map(|view| view.read(cx))
        else {
            return;
        };
        if view.lifecycle.loading() || view.lifecycle.unavailable() {
            return;
        }
        // Saved Markdown first: a conversion saved as .md reopens as that
        // file, not as a fresh conversion of its source (#76).
        let saved = view.session.document.path.as_ref();
        let source = view.preview.attachment().or(view.preview.source());
        let path = saved.or(source);
        let opened = self.tabs.note_recent(id, path);
        if let Some(path) = path {
            // The badge means replacements were applied, not just a scan (#306).
            let pseudonymized = view.has_applied_aliases();
            let new_badge = pseudonymized
                && self
                    .shell
                    .recents
                    .files
                    .iter()
                    .any(|recent| &recent.path == path && !recent.pseudonymized);
            // A saved conversion's source folds into the .md's row (#306).
            let folded = saved.and(source).map(PathBuf::as_path);
            // Cleared or evicted entries return only on a new file event.
            if (opened || new_badge) && self.shell.recents.touch(path, pseudonymized, folded) {
                share_recents(&self.shell.recents, cx);
            }
        }
    }

    /// Settings › Clear and File › Open Recent › Clear Menu.
    pub(super) fn clear_recents(&mut self, cx: &mut Context<Self>) {
        self.shell.recents.clear();
        share_recents(&self.shell.recents, cx);
        cx.notify();
    }

    // ---- Sidebar ------------------------------------------------------------

    /// The sidebar's first row: room for the traffic lights, then the toggle.
    /// It moves the window like a title bar.
    pub(super) fn sidebar_top(&self, toggle: AnyElement) -> AnyElement {
        ui::title_drag(
            div()
                .flex()
                .items_center()
                .flex_shrink_0()
                .h(px(style::CARD_MARGIN + style::HEADER_HEIGHT))
                .pt(px(style::CARD_MARGIN))
                .pl(px(style::TRAFFIC_LIGHT_INSET))
                .pr_2(),
            self.title.armed.clone(),
        )
        .child(ui::no_drag(div().child(toggle)))
        .into_any_element()
    }

    /// Search ⌘K and Home under the top row.
    pub(super) fn sidebar_nav(&self, cx: &mut Context<Self>) -> AnyElement {
        let theme = self.theme.get();
        let p = theme.pdf_style();
        let home = self.showing_home(cx);
        div()
            .flex()
            .flex_col()
            .gap(px(2.))
            .px_2()
            .pb_2()
            .flex_shrink_0()
            .child(
                crate::ui::focus_ring("sidebar-search", theme)
                    .when(cfg!(test), |v| v.debug_selector(|| "sidebar-search".into()))
                    .role(gpui::Role::Button)
                    .aria_label("Search or run an action")
                    .key_context("UiControl")
                    .tab_index(0)
                    .flex()
                    .items_center()
                    .gap_2()
                    .h(px(30.))
                    .px_2()
                    .mb_1()
                    .rounded_md()
                    .border_1()
                    .border_color(p.border)
                    .bg(p.bg)
                    .text_color(p.header_muted)
                    .cursor_pointer()
                    .hover(move |s| s.border_color(p.header_muted))
                    .child(
                        gpui::svg()
                            .data(ui::Icon::Search.data())
                            .size(px(14.))
                            .text_color(p.header_muted),
                    )
                    .child(div().flex_1().child("Search…"))
                    .child(div().text_size(px(11.)).child(key(&OpenPalette)))
                    .on_click(|_, window, cx| window.dispatch_action(Box::new(OpenPalette), cx)),
            )
            .child(
                nav_row("sidebar-home", ui::Icon::Home, "Home", theme, home)
                    .track_focus(&self.panel_home_focus)
                    .child(
                        div()
                            .text_size(px(11.))
                            .text_color(p.header_muted)
                            .child(key(&ShowHome)),
                    )
                    .on_click(cx.listener(|this, _, window, cx| this.show_home(window, cx))),
            )
            .into_any_element()
    }

    /// A small section label, e.g. "Open" over tabs without a case.
    pub(super) fn section_label(&self, label: &'static str) -> AnyElement {
        let p = self.theme.get().pdf_style();
        div()
            .px_2()
            .pt_3()
            .pb_1()
            .text_size(px(11.))
            .text_color(p.header_muted)
            .child(label)
            .into_any_element()
    }

    /// The update card and the Settings · theme · Help row.
    pub(super) fn sidebar_foot(&self, cx: &mut Context<Self>) -> AnyElement {
        let theme = self.theme.get();
        let p = theme.pdf_style();
        let theme_icon = match self.theme_choice {
            ThemeChoice::System => ui::Icon::ThemeSystem,
            ThemeChoice::Light => ui::Icon::Sun,
            ThemeChoice::Dark => ui::Icon::Moon,
        };
        let card = self.update_card(cx);
        // The card carries its own dot; Settings marks an update only without it.
        let dot = card.is_none() && crate::updater::state(cx).marks_settings();
        div()
            .flex()
            .flex_col()
            .gap_1()
            .px_2()
            .pb_2()
            .pt_1()
            .flex_shrink_0()
            .children(card)
            .child(
                div()
                    .flex()
                    .items_center()
                    .gap_1()
                    .child(
                        nav_row("Settings", ui::Icon::Settings, "Settings", theme, false)
                            .track_focus(&self.settings_focus)
                            .flex_1()
                            .when(dot, |v| {
                                v.child(
                                    div()
                                        .when(cfg!(test), |v| {
                                            v.debug_selector(|| "update-dot".into())
                                        })
                                        .size(px(7.))
                                        .rounded_full()
                                        .bg(style::markdown_style(theme).alert_warning),
                                )
                            })
                            .on_click(|_, window, cx| {
                                window.dispatch_action(Box::new(Settings), cx)
                            }),
                    )
                    .child(
                        ui::header_icon_untipped(
                            "theme-toggle",
                            style::theme_label(self.theme_choice),
                            theme_icon,
                            theme,
                            false,
                        )
                        // Clicking changes the choice under the pointer; the
                        // tooltip follows it instead of the value at hover.
                        .tooltip({
                            self.theme_tip.set(self.theme_choice);
                            let (choice, theme) = (self.theme_tip.clone(), self.theme.clone());
                            style::live_tooltip(move || {
                                (style::theme_label(choice.get()).into(), theme.get())
                            })
                        })
                        .when(cfg!(test), |v| v.debug_selector(|| "theme-toggle".into()))
                        .on_click(cx.listener(|this, _, window, cx| this.cycle_theme(window, cx))),
                    )
                    .child(
                        ui::header_icon("sidebar-help", "Help ↗", ui::Icon::Help, theme, false)
                            .on_click(|_, _, cx| cx.open_url(crate::menus::HELP_URL)),
                    ),
            )
            .child(
                div()
                    .h(px(1.))
                    .bg(gpui::transparent_black())
                    .text_color(p.header_muted),
            )
            .into_any_element()
    }

    /// The notices popover under the title row's right end (ADR 0048):
    /// locked cases, a crash report and session messages, with their actions.
    pub(super) fn notices_view(
        &self,
        window: &Window,
        cx: &mut Context<Self>,
    ) -> Option<AnyElement> {
        if !self.title.notices_open.get() {
            return None;
        }
        let theme = self.theme.get();
        let p = theme.pdf_style();
        let cards: Vec<AnyElement> = [
            self.case_key_bar(cx),
            self.crash_bar(cx),
            self.notice_bar(cx),
        ]
        .into_iter()
        .flatten()
        .map(|bar| {
            div()
                .rounded_md()
                .overflow_hidden()
                .border_1()
                .border_color(p.border)
                .child(bar)
                .into_any_element()
        })
        .collect();
        if cards.is_empty() {
            return None;
        }
        let right = window.viewport_size().width - px(style::CARD_MARGIN + 8.);
        Some(
            gpui::deferred(
                gpui::anchored()
                    .position(gpui::point(
                        right,
                        px(style::CARD_MARGIN + style::HEADER_HEIGHT),
                    ))
                    .anchor(gpui::Anchor::TopRight)
                    .snap_to_window_with_margin(px(8.))
                    .child(
                        ui::panel("notices-popover", theme)
                            .when(cfg!(test), |v| {
                                v.debug_selector(|| "notices-popover".into())
                            })
                            .track_focus(&self.notices_focus)
                            .key_context("UiPanel UiMenu")
                            .on_action(cx.listener(|this, _: &ui::CloseMenu, window, cx| {
                                this.close_notices(window, cx);
                                cx.stop_propagation();
                            }))
                            .w(px(400.))
                            .p_2()
                            .rounded_lg()
                            .shadow_lg()
                            .flex()
                            .flex_col()
                            .gap_2()
                            .on_mouse_down_out(cx.listener(
                                |this, event: &gpui::MouseDownEvent, _, cx| {
                                    // A press on the bell is its toggle's to handle.
                                    if this.title.notices_bell.get().contains(&event.position) {
                                        return;
                                    }
                                    this.title.notices_open.set(false);
                                    cx.notify();
                                },
                            ))
                            .child(
                                div()
                                    .flex()
                                    .items_center()
                                    .child(
                                        div()
                                            .flex_1()
                                            .px_1()
                                            .text_size(px(11.))
                                            .font_weight(gpui::FontWeight::SEMIBOLD)
                                            .text_color(p.header_muted)
                                            .child("NOTICES"),
                                    )
                                    .child(
                                        ui::control("close-notices", "×", theme, true)
                                            .when(cfg!(test), |v| {
                                                v.debug_selector(|| "close-notices".into())
                                            })
                                            .aria_label("Close notices")
                                            .on_click(cx.listener(|this, _, window, cx| {
                                                this.close_notices(window, cx);
                                            })),
                                    ),
                            )
                            .children(cards),
                    ),
            )
            .with_priority(2)
            .into_any_element(),
        )
    }

    /// Close the notices popover; focus returns to the document, or to the
    /// window on Home (ADR 0062).
    pub(super) fn close_notices(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.title.notices_open.set(false);
        if self.showing_overview(cx) {
            window.focus(&self.focus, cx);
        } else {
            self.focus_active(window, cx);
        }
        cx.notify();
    }

    /// The collapsed rail's foot: Settings only, an update shown as a dot on
    /// it (ADR 0048). Theme and Help live in the open sidebar.
    pub(super) fn rail_foot(&self, cx: &mut Context<Self>) -> AnyElement {
        let theme = self.theme.get();
        let update = crate::updater::state(cx).marks_settings();
        div()
            .flex()
            .flex_col()
            .items_center()
            .gap(px(2.))
            .pt_1()
            .pb_3()
            .flex_shrink_0()
            .child(
                div()
                    .relative()
                    .child(
                        ui::header_icon(
                            "rail-settings",
                            if update {
                                "Settings — update available"
                            } else {
                                "Settings"
                            },
                            ui::Icon::Settings,
                            theme,
                            false,
                        )
                        .when(cfg!(test), |v| v.debug_selector(|| "rail-settings".into()))
                        .size(px(32.))
                        .on_click(|_, window, cx| window.dispatch_action(Box::new(Settings), cx)),
                    )
                    .when(update, |v| {
                        v.child(
                            div()
                                .when(cfg!(test), |v| {
                                    v.debug_selector(|| "rail-update-dot".into())
                                })
                                .absolute()
                                .top(px(6.))
                                .right(px(6.))
                                .size(px(6.))
                                .rounded_full()
                                .bg(style::markdown_style(theme).alert_warning),
                        )
                    }),
            )
            .into_any_element()
    }

    /// A quiet card while an update is on its way (ADR 0048 decision 9).
    fn update_card(&self, cx: &mut Context<Self>) -> Option<AnyElement> {
        use crate::updater::BannerKind;
        let state = crate::updater::state(cx);
        let banner = state.banner()?;
        if banner.kind == BannerKind::Updated {
            return None;
        }
        let theme = self.theme.get();
        let p = theme.pdf_style();
        let accent = theme.search_accent();
        let version = banner.version.clone();
        let (title, detail) = match banner.kind {
            BannerKind::Available => ("Update available".to_string(), format!("v{version}")),
            BannerKind::Downloading => (
                "Downloading update".to_string(),
                format!("v{version} · {}%", state.percent().unwrap_or(0)),
            ),
            BannerKind::Ready => (
                "Installed, next launch".to_string(),
                format!("v{version} · relaunch"),
            ),
            BannerKind::Failed => ("Update failed".to_string(), format!("v{version} · details")),
            BannerKind::Updated => unreachable!(),
        };
        let ready = banner.kind == BannerKind::Ready;
        Some(
            crate::ui::focus_ring("update-banner", theme)
                .when(cfg!(test), |v| v.debug_selector(|| "update-banner".into()))
                .role(gpui::Role::Button)
                .aria_label(format!("{title}. {detail}"))
                .key_context("UiControl")
                .tab_index(0)
                .flex()
                .items_center()
                .gap_2()
                .px_2()
                .py(px(6.))
                .mb_1()
                .rounded_lg()
                .border_1()
                .border_color(p.border)
                .bg(p.bg)
                .cursor_pointer()
                .hover(move |s| s.border_color(p.header_muted))
                .child(
                    gpui::svg()
                        .data(ui::Icon::Download.data())
                        .size(px(14.))
                        .flex_shrink_0()
                        .text_color(p.header_fg),
                )
                .child(
                    div()
                        .flex_1()
                        .min_w_0()
                        .flex()
                        .flex_col()
                        .child(
                            div()
                                .truncate()
                                .text_size(px(12.))
                                .font_weight(gpui::FontWeight::MEDIUM)
                                .child(title),
                        )
                        .child(
                            div()
                                .truncate()
                                .text_size(px(11.))
                                .text_color(p.header_muted)
                                .child(detail),
                        ),
                )
                .child(
                    div()
                        .size(px(8.))
                        .flex_shrink_0()
                        .rounded_full()
                        .bg(if ready {
                            accent
                        } else {
                            style::markdown_style(theme).alert_warning
                        }),
                )
                .child(
                    div()
                        .id("dismiss-update-banner")
                        .when(cfg!(test), |v| {
                            v.debug_selector(|| "dismiss-update-banner".into())
                        })
                        .aria_label("Dismiss")
                        .px_1()
                        .rounded_sm()
                        .text_color(p.header_muted)
                        .hover(move |s| s.bg(p.placeholder_bg))
                        .child("×")
                        .on_click(|_, _, cx| {
                            cx.stop_propagation();
                            crate::updater::dismiss_banner(cx);
                        }),
                )
                .on_click(cx.listener(move |this, _, window, cx| match banner.kind {
                    BannerKind::Ready => this.restart_to_update(window, cx),
                    _ => {
                        window.activate_window();
                        this.settings
                            .update(cx, |panel, cx| panel.show_updates(window, cx));
                    }
                }))
                .into_any_element(),
        )
    }

    // ---- Home -----------------------------------------------------------------

    pub(super) fn home_view(&self, window: &Window, cx: &mut Context<Self>) -> AnyElement {
        let theme = self.theme.get();
        let p = theme.pdf_style();
        let detached = self.title.detached.get();
        let bar = ui::title_drag(
            div()
                .flex()
                .items_center()
                .flex_shrink_0()
                .h(px(style::HEADER_HEIGHT))
                .pl(px(16. + self.title.inset.get()))
                .pr_2()
                .gap_2()
                .when(detached, |v| {
                    v.h(px(style::CARD_MARGIN + style::HEADER_HEIGHT))
                        .pt(px(style::CARD_MARGIN))
                })
                .when(!detached, |v| v.border_b_1().border_color(p.border))
                .text_size(px(13.)),
            self.title.armed.clone(),
        )
        .child(div().font_weight(gpui::FontWeight::MEDIUM).child("Home"))
        .child(div().flex_1())
        .children(ui::notice_bell(&self.title, theme).map(|bell| ui::no_drag(div().child(bell))));
        let header = ui::title_bar(bar, (!detached).then_some(p.border), window, theme);
        let banner = self.whats_new(cx);
        let body = div()
            .flex_1()
            .min_h_0()
            .flex()
            .flex_col()
            .children(banner)
            .child(
                div()
                    .id("home-scroll")
                    .flex_1()
                    .min_h_0()
                    .overflow_y_scroll()
                    .track_scroll(&self.shell.home_scroll)
                    .child(
                        div()
                            .w_full()
                            .flex()
                            .justify_center()
                            .px_6()
                            .pt(px(56.))
                            .pb(px(48.))
                            .child(
                                div()
                                    .w_full()
                                    .max_w(px(680.))
                                    .flex()
                                    .flex_col()
                                    .gap_4()
                                    .child(
                                        div()
                                            .flex()
                                            .flex_col()
                                            .gap_1()
                                            .child(
                                                div()
                                                    .text_size(px(26.))
                                                    .font_weight(gpui::FontWeight::SEMIBOLD)
                                                    .child("Hello"),
                                            )
                                            .child(
                                                div()
                                                    .text_size(px(14.))
                                                    .text_color(p.header_muted)
                                                    .child(
                                                        "What would you like to prepare for AI?",
                                                    ),
                                            ),
                                    )
                                    .child(self.drop_zone(cx))
                                    .child(self.home_actions(cx))
                                    .children(self.home_cases(cx))
                                    .children(self.home_recents(cx)),
                            ),
                    ),
            );
        // Collapsed sidebar: the header sits in the title row and Home in its
        // own card below (ADR 0048).
        div()
            .id("home")
            .track_focus(self.focus_regions.home())
            .when(cfg!(test), |v| v.debug_selector(|| "home".into()))
            .size_full()
            .flex()
            .flex_col()
            .child(header)
            .map(|v| {
                if detached {
                    v.child(ui::card_surface(body.ml(px(style::CARD_MARGIN)), theme))
                } else {
                    v.child(body)
                }
            })
            .into_any_element()
    }

    /// One-time "What's new" after an update (ADR 0048 decision 9).
    fn whats_new(&self, cx: &mut Context<Self>) -> Option<AnyElement> {
        use crate::updater::BannerKind;
        let banner = crate::updater::state(cx).banner()?;
        if banner.kind != BannerKind::Updated {
            return None;
        }
        let theme = self.theme.get();
        let p = theme.pdf_style();
        let version = banner.version.clone();
        Some(
            div()
                .flex()
                .items_center()
                .gap_3()
                .px_4()
                .py_2()
                .bg(p.placeholder_bg)
                .text_size(px(13.))
                .child(
                    div()
                        .flex_1()
                        .child(format!("mdoc {version} is installed.")),
                )
                .child(
                    ui::link_button("whats-new", "What's new", theme, true).on_click(
                        move |_, _, cx| cx.open_url(&crate::updater::release_url(&version)),
                    ),
                )
                .child(
                    ui::control("dismiss-whats-new", "×", theme, true)
                        .aria_label("Dismiss")
                        .on_click(|_, _, cx| crate::updater::dismiss_banner(cx)),
                )
                .into_any_element(),
        )
    }

    fn drop_zone(&self, _cx: &mut Context<Self>) -> AnyElement {
        let theme = self.theme.get();
        let p = theme.pdf_style();
        let accent = theme.search_accent();
        let active = self.shell.drop.is_some_and(|n| n > 0);
        let refused = self.shell.drop == Some(0);
        let ink = if active { accent } else { p.header_muted };
        let layer = |data: &'static [u8], color: gpui::Hsla| {
            gpui::svg()
                .data(data)
                .absolute()
                .inset_0()
                .size_full()
                .text_color(color)
        };
        let illustration = div()
            .relative()
            .w(px(160.))
            .h(px(112.))
            .child(layer(
                include_bytes!("../../resources/ui/drop-page.svg"),
                ink,
            ))
            .child(layer(
                include_bytes!("../../resources/ui/drop-bars.svg"),
                if active {
                    accent
                } else {
                    Hsla {
                        a: 0.35,
                        ..p.header_muted
                    }
                },
            ))
            .child(layer(
                include_bytes!("../../resources/ui/drop-disc.svg"),
                if active { accent } else { p.header_fg },
            ))
            .child(layer(
                include_bytes!("../../resources/ui/drop-lock.svg"),
                p.bg,
            ));
        let illustration = if active {
            illustration
                .with_animation(
                    ("drop-lift", self.shell.drop_generation),
                    Animation::new(Duration::from_millis(180)).with_easing(gpui::ease_out_quint()),
                    |v, delta| v.mt(px(-6. * delta)).mb(px(6. * delta)),
                )
                .into_any_element()
        } else {
            illustration.into_any_element()
        };
        let title = match self.shell.drop {
            Some(0) => "These files can't be opened".to_string(),
            Some(1) => "Drop to open".to_string(),
            Some(n) => format!("Drop {n} files"),
            None => "Drop documents here".to_string(),
        };
        div()
            .id("home-drop")
            .when(cfg!(test), |v| v.debug_selector(|| "home-drop".into()))
            .flex()
            .flex_col()
            .items_center()
            .gap_2()
            .pt_6()
            .pb_5()
            .rounded_xl()
            .border_1()
            .border_dashed()
            .border_color(Hsla {
                a: 0.45,
                ..p.header_muted
            })
            .when(active, |v| {
                v.border_2()
                    .border_color(accent)
                    .bg(Hsla { a: 0.05, ..accent })
            })
            .when(refused, |v| {
                v.border_color(style::markdown_style(theme).alert_caution)
            })
            .child(illustration)
            .child(
                div()
                    .text_size(px(15.))
                    .font_weight(gpui::FontWeight::MEDIUM)
                    .text_color(if active { accent } else { p.header_fg })
                    .child(title),
            )
            .child(
                ui::filled_button("home-open", "Open files…", theme, true)
                    .when(cfg!(test), |v| v.debug_selector(|| "home-open".into()))
                    .mt_1()
                    .child(ui::key_hint(key(&Open)))
                    .on_click(|_, window, cx| window.dispatch_action(Box::new(Open), cx)),
            )
            .child(
                div()
                    .flex()
                    .flex_col()
                    .items_center()
                    .text_center()
                    .px_4()
                    .text_size(px(12.))
                    .text_color(p.header_muted)
                    .child(mdoc_import::import::supported_formats())
                    .child("Several at once open in their own tabs"),
            )
            .into_any_element()
    }

    fn home_actions(&self, _cx: &mut Context<Self>) -> AnyElement {
        let theme = self.theme.get();
        let p = theme.pdf_style();
        let machine = if cfg!(target_os = "macos") {
            "this Mac"
        } else {
            "this computer"
        };
        let detail = format!(
            "Detection, conversion and saved cases run on {machine}. mdoc goes online only \
             to check for updates and to download models you choose."
        );
        div()
            .flex()
            .flex_wrap()
            .items_center()
            .gap_2()
            .child(
                ui::primary_button("home-paste", "Paste text", theme, true)
                    .when(cfg!(test), |v| v.debug_selector(|| "home-paste".into()))
                    .on_click(|_, window, cx| window.dispatch_action(Box::new(PasteAsNewTab), cx)),
            )
            .child(
                ui::primary_button("home-restore", "Paste AI reply", theme, true)
                    .when(cfg!(test), |v| v.debug_selector(|| "home-restore".into()))
                    .on_click(|_, window, cx| {
                        window.dispatch_action(Box::new(PasteAiResponse), cx)
                    }),
            )
            .child(
                ui::primary_button("home-clipboard", "Pseudonymize clipboard", theme, true)
                    .when(cfg!(test), |v| v.debug_selector(|| "home-clipboard".into()))
                    .on_click(|_, window, cx| {
                        window.dispatch_action(Box::new(PseudonymizeClipboard), cx)
                    }),
            )
            .child(div().flex_1())
            .child(
                div()
                    .id("empty-page-local")
                    .when(cfg!(test), |v| {
                        v.debug_selector(|| "empty-page-local".into())
                    })
                    .flex()
                    .items_center()
                    .gap_1()
                    .text_size(px(12.))
                    .text_color(p.header_muted)
                    .aria_label(format!("Documents stay on {machine}. {detail}"))
                    .tooltip(style::tooltip(detail, theme))
                    .child(
                        gpui::svg()
                            .data(include_bytes!("../../resources/ui/lock.svg"))
                            .size(px(13.))
                            .text_color(p.header_muted),
                    )
                    .child(format!("Documents stay on {machine}")),
            )
            .into_any_element()
    }

    fn home_cases(&self, cx: &mut Context<Self>) -> Option<AnyElement> {
        let theme = self.theme.get();
        let p = theme.pdf_style();
        let cases = self.case_coordinator.summaries();
        let mut list = div().flex().flex_col();
        if cases.is_empty() {
            return Some(
                // The label's top padding moves to the row, so the label and
                // the link share a center line.
                div()
                    .flex()
                    .items_center()
                    .gap_2()
                    .pt_2()
                    .child(home_label("Cases", theme).pt_0())
                    .child(div().flex_1())
                    .child(
                        ui::link_button("home-open-case", "Open a case…", theme, true).on_click(
                            |_, window, cx| window.dispatch_action(Box::new(OpenCase), cx),
                        ),
                    )
                    .into_any_element(),
            );
        }
        for case in cases.iter().take(6) {
            let link = case.link();
            let id = case.id.clone();
            let documents = case.documents;
            list = list.child(
                home_row(
                    SharedString::from(format!("home-case-{}", case.id)),
                    format!(
                        "Case {}, {documents} {}",
                        case.name,
                        if documents == 1 {
                            "document"
                        } else {
                            "documents"
                        }
                    ),
                    theme,
                )
                .child(
                    div()
                        .size(px(8.))
                        .rounded_full()
                        .bg(style::case_color(link.hue(), theme))
                        .flex_shrink_0(),
                )
                .child(div().flex_1().min_w_0().truncate().child(case.name.clone()))
                .child(
                    div()
                        .text_size(px(12.))
                        .text_color(p.header_muted)
                        .child(format!(
                            "{documents} {}",
                            if documents == 1 {
                                "document"
                            } else {
                                "documents"
                            }
                        )),
                )
                .on_click(cx.listener(move |this, _, window, cx| this.open_case(&id, window, cx))),
            );
        }
        Some(
            div()
                .flex()
                .flex_col()
                .gap_1()
                .child(home_label("Cases", theme))
                .child(list)
                .into_any_element(),
        )
    }

    fn home_recents(&self, cx: &mut Context<Self>) -> Option<AnyElement> {
        let theme = self.theme.get();
        let p = theme.pdf_style();
        if self.shell.recents.files.is_empty() {
            return None;
        }
        let rows = self
            .shell
            .recents
            .files
            .iter()
            .take(8)
            .enumerate()
            .map(|(i, recent)| {
                let path = recent.path.clone();
                let name = path
                    .file_name()
                    .map(|n| n.to_string_lossy().into_owned())
                    .unwrap_or_default();
                let folder = path
                    .parent()
                    .and_then(|p| p.file_name())
                    .map(|n| n.to_string_lossy().into_owned())
                    .unwrap_or_default();
                let label = format!(
                    "{name} in {folder}{}, opened {}",
                    if recent.pseudonymized {
                        ", pseudonymized"
                    } else {
                        ""
                    },
                    recents::when(recent.opened).to_lowercase()
                );
                let focus = {
                    let mut handles = self.shell.recent_focus.borrow_mut();
                    while handles.len() <= i {
                        handles.push(cx.focus_handle().tab_stop(true));
                    }
                    handles[i].clone()
                };
                home_row(("home-recent", i), label, theme)
                    .track_focus(&focus)
                    .when(cfg!(test), |v| {
                        v.debug_selector(move || format!("home-recent-{i}"))
                    })
                    .tooltip(style::tooltip(path.to_string_lossy().into_owned(), theme))
                    .child(
                        gpui::svg()
                            .data(ui::Icon::Source.data())
                            .size(px(14.))
                            .flex_shrink_0()
                            .text_color(p.header_muted),
                    )
                    .child(div().min_w_0().truncate().child(name))
                    .child(
                        div()
                            .flex_1()
                            .min_w_0()
                            .truncate()
                            .text_size(px(12.))
                            .text_color(p.header_muted)
                            .child(folder),
                    )
                    .when(recent.pseudonymized, |v| {
                        v.child(
                            div()
                                .px(px(6.))
                                .rounded_sm()
                                .bg(p.placeholder_bg)
                                .text_size(px(11.))
                                .text_color(p.header_muted)
                                .child("pseudonymized"),
                        )
                    })
                    .child(
                        div()
                            .w(px(84.))
                            .flex_shrink_0()
                            .text_right()
                            .text_size(px(12.))
                            .text_color(p.header_muted)
                            .child(recents::when(recent.opened)),
                    )
                    .on_click(cx.listener(move |this, _, window, cx| {
                        this.open_paths(vec![path.clone()], window, cx)
                    }))
            });
        Some(
            div()
                .flex()
                .flex_col()
                .gap_1()
                .child(home_label("Recent", theme))
                .child(div().flex().flex_col().children(rows))
                .into_any_element(),
        )
    }

    /// A file drag anywhere over the window while Home shows (ADR 0048).
    pub(super) fn home_drag(
        &mut self,
        event: &gpui::DragMoveEvent<gpui::ExternalPaths>,
        cx: &mut Context<Self>,
    ) {
        if !self.showing_overview(cx) {
            return;
        }
        let over = event.bounds.contains(&event.event.position);
        let count = over.then(|| {
            event
                .drag(cx)
                .paths()
                .iter()
                .filter(|path| import::droppable(path))
                .count()
        });
        if self.shell.drop != count {
            if self.shell.drop.is_none() {
                self.shell.drop_generation += 1;
            }
            self.shell.drop = count;
            cx.notify();
        }
    }

    pub(super) fn paste_as_new_tab(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let item = cx.read_from_clipboard();
        // A copied file carries its path as text; the editor's Paste skips it.
        let has_files = item.as_ref().is_some_and(|item| {
            item.entries()
                .iter()
                .any(|entry| matches!(entry, gpui::ClipboardEntry::ExternalPaths(_)))
        });
        let Some(text) = item
            .and_then(|item| item.text())
            .filter(|text| !has_files && !text.trim().is_empty())
        else {
            self.notices.post("The clipboard has no text to paste.");
            cx.notify();
            return;
        };
        // Straight into the new buffer: a dispatched Paste would arrive before
        // the new editor holds focus and reach nothing (issue #300).
        let id = self.new_tab(window, cx);
        if let Some(view) = self.tabs.get(id).and_then(|tab| tab.view()).cloned() {
            view.update(cx, |view, cx| {
                view.editor
                    .update(cx, |editor, cx| editor.insert_pasted(text, window, cx))
            });
        }
    }

    /// Home's clipboard actions belong to a document; with none open, say so.
    pub(super) fn clipboard_document(
        &mut self,
        action: &'static str,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Option<Entity<DocumentView>> {
        let real = self
            .tabs
            .active_tab()
            .is_some_and(|tab| !self.is_placeholder(tab, cx));
        if !real {
            // The notice opens even over an earlier one; check quietly whether
            // locked cases belong in it.
            self.load_cases(window, cx);
            let effect = self.notices.post_now(format!(
                "{action} works from a pseudonymized document. Open one, or open a case, first."
            ));
            self.carry_out_notices(effect);
            cx.notify();
            return None;
        }
        // Leave Home so document-level feedback and any error are visible.
        if self.showing_overview(cx) {
            self.activate(self.tabs.active(), window, cx);
        }
        self.active_view()
    }

    // ---- Command palette ------------------------------------------------------

    pub(super) fn open_palette(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.shell.palette.is_some() {
            self.close_palette(window, cx);
            return;
        }
        let input = cx.new(|cx| {
            SearchInput::new(cx)
                .with_theme(self.theme.clone())
                .with_key_context(PALETTE)
                .with_placeholder("Search or run an action…")
        });
        let changes = cx.subscribe(&input, |this, _, _: &SearchInputEvent, cx| {
            if let Some(palette) = &mut this.shell.palette {
                palette.selected = 0;
            }
            cx.notify();
        });
        let return_focus = window.focused(cx);
        window.focus(&input.read(cx).focus_handle(cx), cx);
        self.shell.palette = Some(Palette {
            input,
            selected: 0,
            return_focus,
            _changes: changes,
        });
        cx.notify();
    }

    pub(super) fn close_palette(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if let Some(palette) = self.shell.palette.take() {
            match palette.return_focus {
                Some(focus) => window.focus(&focus, cx),
                None => self.focus_active(window, cx),
            }
        }
        cx.notify();
    }

    fn palette_rows(&self, cx: &App) -> Vec<Row> {
        let query = self
            .shell
            .palette
            .as_ref()
            .map(|p| p.input.read(cx).value().trim().to_lowercase())
            .unwrap_or_default();
        let has_document = !self.showing_overview(cx);
        let mut rows = vec![
            Row {
                section: "Actions",
                label: "New tab".into(),
                detail: None,
                command: Command::NewTab,
            },
            Row {
                section: "Actions",
                label: "Open files…".into(),
                detail: None,
                command: Command::OpenFiles,
            },
            Row {
                section: "Actions",
                label: "Paste text as new tab".into(),
                detail: None,
                command: Command::PasteText,
            },
        ];
        if has_document {
            rows.extend([
                Row {
                    section: "Actions",
                    label: "Pseudonymize".into(),
                    detail: None,
                    command: Command::Pseudonymize,
                },
                Row {
                    section: "Actions",
                    label: "Copy for AI".into(),
                    detail: None,
                    command: Command::CopyForAi,
                },
                Row {
                    section: "Actions",
                    label: "Copy Markdown".into(),
                    detail: None,
                    command: Command::CopyMarkdown,
                },
                Row {
                    section: "Actions",
                    label: "Save".into(),
                    detail: None,
                    command: Command::Save,
                },
                Row {
                    section: "Actions",
                    label: "Save as…".into(),
                    detail: None,
                    command: Command::SaveAs,
                },
                Row {
                    section: "Actions",
                    label: "Side by side".into(),
                    detail: None,
                    command: Command::BeforeAfter,
                },
                Row {
                    section: "Actions",
                    label: "What you did".into(),
                    detail: None,
                    command: Command::DecisionHistory,
                },
            ]);
            let panel = self.active_view().is_some_and(|view| {
                let view = view.read(cx);
                view.replacements_open() && view.can_copy_markdown()
            });
            if panel {
                rows.extend([
                    Row {
                        section: "Actions",
                        label: "Expand all identities".into(),
                        detail: None,
                        command: Command::ExpandIdentities,
                    },
                    Row {
                        section: "Actions",
                        label: "Collapse all identities".into(),
                        detail: None,
                        command: Command::CollapseIdentities,
                    },
                ]);
            }
            rows.extend([Row {
                section: "Actions",
                label: "Close tab".into(),
                detail: None,
                command: Command::CloseTab,
            }]);
        }
        rows.extend([
            Row {
                section: "Actions",
                label: "Paste AI reply".into(),
                detail: None,
                command: Command::RestoreReply,
            },
            Row {
                section: "Actions",
                label: "Pseudonymize clipboard".into(),
                detail: None,
                command: Command::PseudonymizeClipboard,
            },
            Row {
                section: "Actions",
                label: "Open a case…".into(),
                detail: None,
                command: Command::OpenCase,
            },
            Row {
                section: "Actions",
                label: style::theme_switch_label(self.theme_choice.next()).into(),
                detail: None,
                command: Command::Theme,
            },
            Row {
                section: "Actions",
                label: "Toggle sidebar".into(),
                detail: None,
                command: Command::ToggleSidebar,
            },
            Row {
                section: "Actions",
                label: "Keyboard shortcuts".into(),
                detail: None,
                command: Command::Shortcuts,
            },
            Row {
                section: "Go to",
                label: "Home".into(),
                detail: None,
                command: Command::Home,
            },
            Row {
                section: "Go to",
                label: "Settings".into(),
                detail: None,
                command: Command::Settings,
            },
        ]);
        rows.extend(
            crate::settings_ui::Category::ALL
                .into_iter()
                .map(|category| Row {
                    section: "Go to",
                    label: format!("Settings › {}", category.label()),
                    detail: None,
                    command: Command::SettingsCategory(category),
                }),
        );
        // The active document's headings (from MiaoYan's table of contents).
        if let Some(view) = self.active_view().filter(|_| has_document) {
            let view = view.read(cx);
            if !view.lifecycle.source_only()
                && !view.lifecycle.loading()
                && !view.lifecycle.unavailable()
            {
                for heading in mdoc_editor::headings(view.editor.read(cx).text()) {
                    rows.push(Row {
                        section: "Go to heading",
                        label: heading.title,
                        detail: Some(format!("H{}", heading.level)),
                        command: Command::Heading(heading.offset),
                    });
                }
            }
        }
        for tab in &self.tabs {
            if self.is_placeholder(tab, cx) {
                continue;
            }
            let name = super::tab_label(tab, cx).name;
            rows.push(Row {
                section: "Open tabs",
                label: name,
                detail: None,
                command: Command::Tab(tab.id()),
            });
        }
        for case in self.case_coordinator.summaries() {
            rows.push(Row {
                section: "Cases",
                label: case.name.clone(),
                detail: Some(format!("{} documents", case.documents)),
                command: Command::Case(case.id.clone()),
            });
        }
        for recent in &self.shell.recents.files {
            let name = recent
                .path
                .file_name()
                .map(|n| n.to_string_lossy().into_owned())
                .unwrap_or_default();
            let kind = recent
                .path
                .extension()
                .map(|e| e.to_string_lossy().to_uppercase())
                .unwrap_or_default();
            rows.push(Row {
                section: "Recent",
                label: name,
                detail: Some(kind),
                command: Command::Recent(recent.path.clone()),
            });
        }
        if query.is_empty() {
            return rows;
        }
        rows.into_iter()
            .filter(|row| {
                let label = row.label.to_lowercase();
                query.split_whitespace().all(|word| label.contains(word))
            })
            .collect()
    }

    /// Each palette row with a key: its label and the key, for tests.
    #[cfg(test)]
    pub(super) fn palette_shortcuts(&self, cx: &App) -> Vec<(String, String)> {
        self.palette_rows(cx)
            .into_iter()
            .filter_map(|row| Some((row.label, row.command.shortcut()?)))
            .collect()
    }

    /// The palette's labels in one section, for tests.
    #[cfg(test)]
    pub(super) fn palette_section(&self, section: &str, cx: &App) -> Vec<String> {
        self.palette_rows(cx)
            .into_iter()
            .filter(|row| row.section == section)
            .map(|row| row.label)
            .collect()
    }

    pub(super) fn palette_step(&mut self, delta: isize, cx: &mut Context<Self>) {
        let count = self.palette_rows(cx).len();
        if let Some(palette) = &mut self.shell.palette
            && count > 0
        {
            palette.selected =
                (palette.selected as isize + delta).rem_euclid(count as isize) as usize;
            cx.notify();
        }
    }

    pub(super) fn palette_confirm(
        &mut self,
        index: Option<usize>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let rows = self.palette_rows(cx);
        let index = index.or(self.shell.palette.as_ref().map(|p| p.selected));
        let Some(row) = index.and_then(|i| rows.into_iter().nth(i)) else {
            return;
        };
        self.close_palette(window, cx);
        let dispatch =
            |window: &mut Window, cx: &mut Context<Self>, action: Box<dyn gpui::Action>| {
                window.dispatch_action(action, cx)
            };
        match row.command {
            Command::NewTab => dispatch(window, cx, Box::new(New)),
            Command::OpenFiles => dispatch(window, cx, Box::new(Open)),
            Command::PasteText => self.paste_as_new_tab(window, cx),
            Command::Pseudonymize => dispatch(window, cx, Box::new(Pseudonymize)),
            Command::PseudonymizeClipboard => dispatch(window, cx, Box::new(PseudonymizeClipboard)),
            Command::RestoreReply => dispatch(window, cx, Box::new(PasteAiResponse)),
            Command::CopyForAi => dispatch(window, cx, Box::new(CopyForAi)),
            Command::CopyMarkdown => dispatch(window, cx, Box::new(CopyMarkdown)),
            Command::Save => dispatch(window, cx, Box::new(Save)),
            Command::SaveAs => dispatch(window, cx, Box::new(SaveAs)),
            Command::CloseTab => dispatch(window, cx, Box::new(Close)),
            Command::BeforeAfter => dispatch(window, cx, Box::new(ToggleCompare)),
            // Straight to the document: focus may have gone back to the
            // sidebar, outside the view's action handlers.
            Command::DecisionHistory => {
                if let Some(view) = self.active_view() {
                    view.update(cx, |view, cx| view.open_decision_history(window, cx));
                }
            }
            Command::ExpandIdentities | Command::CollapseIdentities => {
                let expand = matches!(row.command, Command::ExpandIdentities);
                if let Some(view) = self.active_view() {
                    view.update(cx, |view, cx| {
                        if view.replacements_open() {
                            view.set_all_identities_expanded(expand, cx)
                        }
                    });
                }
            }
            Command::ToggleSidebar => dispatch(window, cx, Box::new(ToggleSidebar)),
            Command::OpenCase => dispatch(window, cx, Box::new(OpenCase)),
            Command::Home => self.show_home(window, cx),
            Command::Settings => dispatch(window, cx, Box::new(Settings)),
            Command::SettingsCategory(category) => self
                .settings
                .update(cx, |panel, cx| panel.show_category(category, window, cx)),
            Command::Shortcuts => dispatch(window, cx, Box::new(ShowShortcuts)),
            Command::Theme => self.cycle_theme(window, cx),
            Command::Heading(offset) => {
                // The Markdown editor must be the pane shown: not Before/After,
                // nor Source in a narrow window.
                if let Some(view) = self.active_view() {
                    view.update(cx, |view, cx| {
                        if view.compare.is_some() {
                            view.close_compare(window, cx);
                        }
                        view.original_selected = false;
                        view.editor
                            .update(cx, |editor, cx| editor.reveal_offset(offset, cx));
                        cx.notify();
                    });
                }
                self.focus_active(window, cx);
            }
            Command::Tab(id) => self.activate(id, window, cx),
            Command::Case(id) => self.open_case(&id, window, cx),
            Command::Recent(path) => self.open_paths(vec![path], window, cx),
        }
    }

    pub(super) fn palette_view(&self, cx: &mut Context<Self>) -> Option<AnyElement> {
        let palette = self.shell.palette.as_ref()?;
        let theme = self.theme.get();
        let p = theme.pdf_style();
        let rows = self.palette_rows(cx);
        let selected = palette.selected.min(rows.len().saturating_sub(1));
        let mut list = div().flex().flex_col().px_1().pb_1();
        let mut section = "";
        for (i, row) in rows.iter().enumerate() {
            if row.section != section {
                section = row.section;
                list = list.child(
                    div()
                        .px_2()
                        .pt_2()
                        .pb_1()
                        .text_size(px(11.))
                        .text_color(p.header_muted)
                        .child(section),
                );
            }
            let icon = match &row.command {
                Command::NewTab | Command::PasteText => ui::Icon::Plus,
                Command::OpenFiles => ui::Icon::Open,
                Command::Tab(_) | Command::Recent(_) => ui::Icon::Source,
                Command::Heading(_) => ui::Icon::ChevronRight,
                Command::Home => ui::Icon::Home,
                Command::Settings | Command::SettingsCategory(_) => ui::Icon::Settings,
                Command::Shortcuts => ui::Icon::Help,
                Command::Theme => ui::Icon::ThemeSystem,
                Command::RestoreReply => ui::Icon::Reply,
                Command::CopyForAi | Command::CopyMarkdown => ui::Icon::Copy,
                Command::Save | Command::SaveAs => ui::Icon::Save,
                Command::CloseTab => ui::Icon::Close,
                Command::BeforeAfter => ui::Icon::PanelRight,
                Command::DecisionHistory => ui::Icon::Clock,
                Command::ExpandIdentities => ui::Icon::ExpandAll,
                Command::CollapseIdentities => ui::Icon::CollapseAll,
                Command::ToggleSidebar => ui::Icon::Sidebar,
                Command::Pseudonymize | Command::PseudonymizeClipboard => ui::Icon::Anonymous,
                Command::Case(_) | Command::OpenCase => ui::Icon::Sidebar,
            };
            list = list.child(
                div()
                    .id(("palette-row", i))
                    .when(cfg!(test), |v| {
                        v.debug_selector(move || format!("palette-row-{i}"))
                    })
                    .role(gpui::Role::MenuItem)
                    .aria_label(row.label.clone())
                    .flex()
                    .items_center()
                    .gap_2()
                    .h(px(32.))
                    .px_2()
                    .rounded_md()
                    .cursor_pointer()
                    .when(i == selected, |v| v.bg(p.placeholder_bg))
                    .hover(move |s| s.bg(p.placeholder_bg))
                    .child(
                        gpui::svg()
                            .data(icon.data())
                            .size(px(15.))
                            .text_color(p.header_muted),
                    )
                    .child(div().flex_1().min_w_0().truncate().child(row.label.clone()))
                    .when_some(row.detail.clone(), |v, d| {
                        v.child(div().text_size(px(11.)).text_color(p.header_muted).child(d))
                    })
                    .when_some(row.command.shortcut(), |v, s| {
                        v.child(div().text_size(px(11.)).text_color(p.header_muted).child(s))
                    })
                    .on_click(cx.listener(move |this, _, window, cx| {
                        this.palette_confirm(Some(i), window, cx)
                    })),
            );
        }
        if rows.is_empty() {
            list = list.child(
                div()
                    .px_3()
                    .py_4()
                    .text_color(p.header_muted)
                    .child("Nothing matches."),
            );
        }
        Some(
            gpui::deferred(
                div()
                    .absolute()
                    .inset_0()
                    .flex()
                    .justify_center()
                    // The palette hugs its rows instead of stretching down.
                    .items_start()
                    .pt(px(72.))
                    .bg(gpui::hsla(0., 0., 0., 0.18))
                    .occlude()
                    .on_mouse_down(
                        gpui::MouseButton::Left,
                        cx.listener(|this, _, window, cx| this.close_palette(window, cx)),
                    )
                    .child(
                        ui::panel("command-palette", theme)
                            .when(cfg!(test), |v| {
                                v.debug_selector(|| "command-palette".into())
                            })
                            .key_context(PALETTE)
                            .on_mouse_down(gpui::MouseButton::Left, |_, _, cx| {
                                cx.stop_propagation()
                            })
                            .on_action(
                                cx.listener(|this, _: &PaletteUp, _, cx| this.palette_step(-1, cx)),
                            )
                            .on_action(
                                cx.listener(|this, _: &PaletteDown, _, cx| {
                                    this.palette_step(1, cx)
                                }),
                            )
                            .on_action(cx.listener(|this, _: &PaletteConfirm, window, cx| {
                                this.palette_confirm(None, window, cx)
                            }))
                            .on_action(cx.listener(|this, _: &PaletteClose, window, cx| {
                                this.close_palette(window, cx)
                            }))
                            .w(px(560.))
                            .flex()
                            .flex_col()
                            .rounded_xl()
                            .shadow_lg()
                            .overflow_hidden()
                            .child(
                                div()
                                    .flex()
                                    .items_center()
                                    .gap_2()
                                    .px_3()
                                    .h(px(44.))
                                    .flex_shrink_0()
                                    .border_b_1()
                                    .border_color(p.border)
                                    .child(
                                        gpui::svg()
                                            .data(ui::Icon::Search.data())
                                            .size(px(15.))
                                            .text_color(p.header_muted),
                                    )
                                    .child(
                                        div()
                                            .flex_1()
                                            .text_size(px(14.))
                                            .child(palette.input.clone()),
                                    ),
                            )
                            .child(
                                div()
                                    .id("palette-list")
                                    .max_h(px(470.))
                                    .overflow_y_scroll()
                                    .child(list),
                            ),
                    ),
            )
            .with_priority(2)
            .into_any_element(),
        )
    }
}

fn nav_row(
    id: &'static str,
    icon: ui::Icon,
    label: &'static str,
    theme: Theme,
    selected: bool,
) -> ui::Control {
    let p = theme.pdf_style();
    crate::ui::focus_ring(id, theme)
        .when(cfg!(test), |v| v.debug_selector(move || id.into()))
        .role(gpui::Role::Button)
        .aria_label(label)
        .key_context("UiControl")
        .tab_index(0)
        .flex()
        .items_center()
        .gap_2()
        .h(px(30.))
        .px_2()
        .rounded_md()
        .cursor_pointer()
        .when(selected, |v| v.bg(theme.sidebar_selected()))
        .hover(move |s| s.bg(p.placeholder_bg))
        .child(
            gpui::svg()
                .data(icon.data())
                .size(px(15.))
                .flex_shrink_0()
                .text_color(if selected {
                    p.header_fg
                } else {
                    p.header_muted
                }),
        )
        .child(div().flex_1().child(label))
}

fn home_label(label: &'static str, theme: Theme) -> gpui::Div {
    div()
        .pt_2()
        .text_size(px(12.))
        .font_weight(gpui::FontWeight::MEDIUM)
        .text_color(theme.pdf_style().header_muted)
        .child(label)
}

/// A Home list row: a Tab stop that Enter or Space opens, read by screen
/// readers as a button named `label` (ADR 0062).
fn home_row(
    id: impl Into<gpui::ElementId>,
    label: impl Into<SharedString>,
    theme: Theme,
) -> ui::Control {
    let p = theme.pdf_style();
    crate::ui::focus_ring(id, theme)
        .role(gpui::Role::Button)
        .aria_label(label.into())
        .key_context("UiControl")
        .tab_index(0)
        .flex()
        .items_center()
        .gap_3()
        .h(px(36.))
        .px_2()
        .rounded_md()
        .border_b_1()
        .border_color(p.border)
        .text_size(px(13.))
        .cursor_pointer()
        .hover(move |s| s.bg(p.placeholder_bg))
}
