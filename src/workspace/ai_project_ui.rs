//! The AI project in the window (ADR 0126; #1164 Decisions 2, 5 and 25;
//! Milestone 2 item 8), all of it only while "AI connector (preview)" is
//! on:
//!
//! - the AI project is always shown: the sidebar's foot (or the rail) and
//!   its Project page name it, with Stop sharing;
//! - an AI app asking while nothing is shared brings a prompt to share
//!   the Project last viewed, or another; only its Share shares, so the
//!   share never follows the view;
//! - a document entering the AI project is scanned in its tab, by the
//!   path Pseudonymize takes: proposals only, never applied;
//! - the Project page lists its AI activity.
//!
//! Notifications, the badge and where a delivery lands are
//! `ai_deliveries.rs` (item 9).
//!
//! The share lives in the connector, in memory only; nothing here writes
//! the Vault.
use super::{Workspace, ai_project::Asked, navigation::Stop, project_page::Place};
use crate::{ai_connector, style, ui};
use gpui::{AnyElement, App, Context, SharedString, Window, div, prelude::*, px};
use mdoc_store::{document_id::VaultId, settings::AiConnectorConfig};
use mdoc_vault::{AiActivity, AiEvent, Project, ReplyOwner};
use std::{collections::HashMap, time::Duration};

/// How often the window hears from the connector.
const POLL: Duration = Duration::from_millis(250);

/// The newest activity lines the Project page lists.
pub(super) const ACTIVITY_SHOWN: usize = 100;

impl Workspace {
    /// "AI connector (preview)" as Settings holds it.
    pub(super) fn ai_setting(&self) -> AiConnectorConfig {
        (self.preferences.borrow().current.as_ref())
            .map(|preferences| preferences.ai_connector)
            .unwrap_or_default()
    }

    /// "AI connector (preview)" is on, the Vault open or not.
    pub(crate) fn ai_preview_setting(&self) -> bool {
        self.ai_setting().preview
    }

    /// The connector runs: the setting is on and the Vault is open.
    pub(super) fn ai_connector_on(&self) -> bool {
        self.ai_connector.is_some()
    }

    /// The AI project, while the window holds it.
    pub(super) fn ai_shared(&self) -> Option<String> {
        let shared = self.ai_connector.as_ref()?.connector.ai_project()?;
        self.projects
            .project(&shared)
            .map(|project| project.id.clone())
    }

    /// With "AI connector (preview)" on, the connector once the Vault is
    /// open, and the window hears from it. Tests and headless renders get
    /// no listener: they keep to their own Vaults and never take the
    /// profile's endpoint.
    pub(super) fn start_ai_connector(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.ai_connector.is_some() {
            return;
        }
        let Some(vault) = self.vault.vault().cloned() else {
            return;
        };
        let setting = self.ai_setting();
        let started = if cfg!(test) || mdoc_store::app_paths::isolated() {
            Ok(ai_connector::start_unlistened(vault, &setting))
        } else {
            ai_connector::start(vault, &setting, ai_connector::endpoint().as_ref())
        };
        self.ai_started(started, window, cx);
    }

    /// What a start gave: the running connector, or why it couldn't
    /// start, which Settings › AI connector shows (#1220).
    pub(super) fn ai_started(
        &mut self,
        started: Result<Option<ai_connector::Running>, String>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let (running, failed) = match started {
            Ok(running) => (running, None),
            Err(why) => (None, Some(why)),
        };
        self.settings
            .update(cx, |panel, cx| panel.ai_connector_failed(failed, cx));
        let Some(running) = running else {
            return;
        };
        self.ai_connector = Some(running);
        self.listen_for_notifications(cx);
        self.ai_poll = Some(cx.spawn_in(window, async move |this, cx| {
            loop {
                cx.background_executor().timer(POLL).await;
                let polled =
                    this.update_in(cx, |this, window, cx| this.poll_ai_connector(window, cx));
                if !polled.unwrap_or(false) {
                    break;
                }
            }
        }));
        self.scan_on_entry(cx);
        cx.notify();
    }

    /// Settings applied: the connector starts or stops with the setting.
    /// Stopping it shares nothing.
    pub(super) fn refresh_ai_connector(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        // With the preview on, the menu bar item toggles the companion.
        crate::menu_bar::set_companion(self.ai_setting().preview, cx);
        if self.ai_setting().preview {
            self.start_ai_connector(window, cx);
        } else {
            self.stop_ai_connector(cx);
        }
    }

    /// The switch shows off, saved or not yet (#1220): nothing more
    /// reaches an AI app, and nothing is shared.
    pub(super) fn stop_ai_connector(&mut self, cx: &mut Context<Self>) {
        self.settings
            .update(cx, |panel, cx| panel.ai_connector_failed(None, cx));
        if self.ai_connector.take().is_some() {
            self.ai_poll = None;
            self.ai.stopped();
            self.ai_ask_cancelled();
            if (self.project_ui.page.as_ref())
                .is_some_and(|page| page.segment() == super::project_page::Segment::Activity)
            {
                self.show_project_segment(super::project_page::Segment::Documents, cx);
            }
            cx.notify();
        }
    }

    /// What the connector said since the last poll; `false` once it has
    /// stopped.
    pub(super) fn poll_ai_connector(
        &mut self,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> bool {
        let Some(running) = &self.ai_connector else {
            return false;
        };
        let mut activity = false;
        for event in running.events() {
            match event {
                ai_connector::Event::Asked { app } => self.ai_app_asked(app, window, cx),
                ai_connector::Event::Activity => activity = true,
                ai_connector::Event::Delivered {
                    app,
                    reply,
                    document,
                } => {
                    activity = true;
                    self.ai_delivered(&app, reply, document, window, cx);
                }
                ai_connector::Event::NeedsReview {
                    app,
                    document,
                    label,
                } => self.ai_review_requested(&app, document, &label, cx),
            }
        }
        if activity {
            // A delivered Document is a new member: it scans once it opens.
            if let Some(project) = self.ai.activity_of().map(str::to_string) {
                self.load_ai_activity(&project);
            }
            cx.notify();
        }
        true
    }

    /// AI app `app` asked while nothing was shared: ask the user, without
    /// taking focus or bringing the window forward.
    fn ai_app_asked(&mut self, app: String, _: &mut Window, cx: &mut Context<Self>) {
        if self.ai_shared().is_some() {
            return;
        }
        let projects: Vec<String> = (self.projects.summaries().into_iter())
            .map(|project| project.id)
            .collect();
        match self.ai.asked(app, &projects, std::time::Instant::now()) {
            Asked::Prompt => cx.notify(),
            Asked::NoProject(app) => {
                self.notices.post(format!(
                    "{} asked for documents. Make a Project to share it with AI apps.",
                    ai_connector::display_name(&app)
                ));
                cx.notify();
            }
            Asked::Nothing => {}
        }
    }

    /// The prompt's Share, and Ask's: Project `project` is the AI project
    /// from now on, in place of any other, and its members not yet scanned
    /// scan. `false` when it could not be shared, which a notice says.
    pub(super) fn share_with_ai(&mut self, project: &str, cx: &mut Context<Self>) -> bool {
        let Some(running) = &self.ai_connector else {
            return false;
        };
        let shared = match running.connector.share(project) {
            Ok(()) => {
                self.ai.shared();
                self.scan_on_entry(cx);
                crate::menu_bar::companion_changed(cx);
                true
            }
            Err(error) => {
                self.notices
                    .post(format!("Could not share the Project with AI apps. {error}"));
                false
            }
        };
        cx.notify();
        shared
    }

    /// Stop sharing: AI apps read nothing until a Project is shared again.
    pub(super) fn stop_sharing_with_ai(&mut self, cx: &mut Context<Self>) {
        if let Some(running) = &self.ai_connector {
            running.connector.stop_sharing();
        }
        self.ai.sharing_stopped();
        // A later Stop wins over an Ask still waiting for its move.
        self.ai_ask_cancelled();
        crate::menu_bar::companion_changed(cx);
        cx.notify();
    }

    /// Project `id` was deleted: it is shared no more, or offered.
    pub(super) fn ai_project_gone(&mut self, id: &str, cx: &mut App) {
        if let Some(running) = &self.ai_connector
            && running.connector.ai_project().as_deref() == Some(id)
        {
            running.connector.stop_sharing();
            crate::menu_bar::companion_changed(cx);
        }
        self.ai.forget(id);
    }

    /// The user arrived at a place, recorded in the history or not (a
    /// closed tab's neighbour comes to the front unrecorded): its Project
    /// is the one last viewed.
    pub(super) fn note_ai_viewed(&mut self, stop: &Stop) {
        let project = match stop {
            Stop::Project(id) => Some(id.clone()),
            Stop::Document(document) => self.project_of(document.as_str()),
            Stop::Reply(id) => (self.vault.vault())
                .and_then(|vault| vault.reply(id).ok())
                .and_then(|reply| match reply.owner {
                    ReplyOwner::Project(project) => Some(project),
                    ReplyOwner::Document(document) => self.project_of(&document),
                }),
            Stop::Home | Stop::Tab(_) => None,
        };
        self.ai.viewed(project);
        match stop {
            Stop::Reply(id) => self.ai.seen(Some(id), None),
            Stop::Document(document) => self.ai.seen(None, Some(document.as_str())),
            Stop::Project(_) | Stop::Home | Stop::Tab(_) => {}
        }
    }

    /// Scan the open members of the AI project that entered it (#1164
    /// Decision 5): each with no saved review and no scan in its tab yet
    /// scans once, as Pseudonymize would; nothing is applied. A member
    /// that can't scan yet (no text, no model) tries again on the next
    /// load or share.
    pub(super) fn scan_on_entry(&mut self, cx: &mut Context<Self>) {
        let Some(shared) = self.ai_shared() else {
            return;
        };
        let excluded: std::collections::HashSet<String> = (self.vault.vault().into_iter())
            .flat_map(|vault| vault.documents_in(Some(&shared)))
            .filter(|summary| summary.exclude_from_ai)
            .map(|summary| summary.id)
            .collect();
        let views: Vec<_> = (self.tabs.iter())
            .filter_map(|tab| Some((tab.id(), tab.view()?.clone())))
            .collect();
        for (tab, view) in views {
            let (document, project, restored) = {
                let view = view.read(cx);
                (
                    view.session.document.vault.clone(),
                    view.project.link().map(|link| link.id.clone()),
                    view.session.document.review.is_some(),
                )
            };
            let Some(document) = document else {
                continue;
            };
            if project.as_deref() != Some(shared.as_str()) {
                self.ai.left(tab, document.as_str());
                continue;
            }
            if restored || excluded.contains(document.as_str()) {
                continue;
            }
            if !self.ai.entering(tab, document.as_str()) {
                continue;
            }
            if !view.update(cx, |view, cx| view.scan_on_entry(cx)) {
                self.ai.left(tab, document.as_str());
            }
        }
    }

    /// Read Project `project`'s AI activity for its page.
    pub(super) fn load_ai_activity(&mut self, project: &str) {
        let Some(vault) = self.vault.vault() else {
            return;
        };
        let read = vault
            .ai_activity(project)
            .map_err(|error| error.to_string());
        self.ai.set_activity(project.to_string(), read);
    }

    // ---- What the window shows --------------------------------------------

    /// The sidebar's foot: the AI project, with Stop sharing.
    pub(super) fn ai_indicator(&self, cx: &mut Context<Self>) -> Option<AnyElement> {
        self.ai_connector.as_ref()?;
        let theme = self.theme.get();
        let p = theme.pdf_style();
        let shared = self
            .ai_shared()
            .and_then(|id| self.projects.project(&id).cloned());
        let card = div()
            .id("ai-project")
            .when(cfg!(test), |v| v.debug_selector(|| "ai-project".into()))
            .role(gpui::Role::Group)
            .aria_label("Shared with AI apps")
            .flex()
            .flex_col()
            .gap(px(2.))
            .px_1()
            .py_1()
            .rounded_md()
            .border_1()
            .border_color(p.border)
            .child(
                div()
                    .flex()
                    .items_center()
                    .justify_between()
                    .gap_1()
                    .child(
                        div()
                            .px_1()
                            .text_size(px(10.))
                            .font_weight(gpui::FontWeight::SEMIBOLD)
                            .text_color(p.header_muted)
                            .child("SHARED WITH AI APPS"),
                    )
                    .children(self.ai_badge(cx)),
            );
        let card = match shared {
            Some(project) => {
                let id = project.id.clone();
                let color = style::project_color(super::ProjectLink::of(&project).hue(), theme);
                card.child(
                    div()
                        .flex()
                        .items_center()
                        .gap_1()
                        .child(
                            ui::control("ai-project-open", "", theme, true)
                                .when(cfg!(test), |v| {
                                    v.debug_selector(|| "ai-project-open".into())
                                })
                                .aria_label(format!("Open {}", project.name))
                                .flex()
                                .items_center()
                                .gap_2()
                                .flex_1()
                                .min_w_0()
                                .child(div().flex_shrink_0().size(px(8.)).rounded_full().bg(color))
                                .child(div().min_w_0().truncate().child(project.name.clone()))
                                .on_click(cx.listener(move |this, _, window, cx| {
                                    this.show_project_page(&id, Place::Header, window, cx)
                                })),
                        )
                        .child(
                            ui::link_button("ai-stop-sharing", "Stop sharing", theme, true)
                                .when(cfg!(test), |v| {
                                    v.debug_selector(|| "ai-stop-sharing".into())
                                })
                                .flex_shrink_0()
                                .text_size(px(12.))
                                .on_click(
                                    cx.listener(|this, _, _, cx| this.stop_sharing_with_ai(cx)),
                                ),
                        ),
                )
            }
            None => card.child(
                div()
                    .px_1()
                    .pb_1()
                    .text_size(px(12.))
                    .text_color(p.header_muted)
                    .child("No Project shared"),
            ),
        };
        Some(card.into_any_element())
    }

    /// The collapsed rail's mark of the AI project: its colour, its name
    /// in the tooltip, its page on a click.
    pub(super) fn ai_rail_icon(&self, cx: &mut Context<Self>) -> Option<AnyElement> {
        self.ai_connector.as_ref()?;
        let theme = self.theme.get();
        let shared = self
            .ai_shared()
            .and_then(|id| self.projects.project(&id).cloned());
        let label = match &shared {
            Some(project) => format!("Shared with AI apps: {}", project.name),
            None => "No Project shared with AI apps".to_string(),
        };
        let open = shared.as_ref().map(|project| project.id.clone());
        let badge = self.ai_rail_badge();
        let label = match self.ai.unseen().len() {
            0 => label,
            count => format!("{label}. {count} new, open the newest"),
        };
        let counts = badge.is_some();
        Some(
            div()
                .relative()
                .child(
                    ui::header_icon("rail-ai-project", label, ui::Icon::Reply, theme, false)
                        .when(cfg!(test), |v| {
                            v.debug_selector(|| "rail-ai-project".into())
                        })
                        .size(px(32.))
                        .on_click(cx.listener(move |this, _, window, cx| {
                            if counts {
                                this.open_newest(window, cx);
                            } else if let Some(id) = &open {
                                this.show_project_page(id, Place::Header, window, cx);
                            }
                        })),
                )
                .children(badge)
                .when_some(shared.filter(|_| !counts), |v, project| {
                    v.child(
                        div()
                            .absolute()
                            .top(px(6.))
                            .right(px(6.))
                            .size(px(7.))
                            .rounded_full()
                            .bg(style::project_color(
                                super::ProjectLink::of(&project).hue(),
                                theme,
                            )),
                    )
                })
                .into_any_element(),
        )
    }

    /// The share prompt: a card at the window's bottom left, over the
    /// sidebar's foot. It takes no focus. While a dialog or sheet is up it
    /// waits, and shows once that closes: it is drawn above the window's
    /// own layers.
    pub(super) fn ai_prompt_view(
        &self,
        window: &Window,
        cx: &mut Context<Self>,
    ) -> Option<AnyElement> {
        self.ai_connector.as_ref()?;
        let prompt = self.ai.prompt()?;
        if self.modal_up(window, cx) {
            return None;
        }
        let theme = self.theme.get();
        let p = theme.pdf_style();
        let title = format!(
            "{} asked for documents",
            ai_connector::display_name(&prompt.app)
        );
        let rows = (prompt.choices.iter().enumerate()).filter_map(|(index, id)| {
            let project = self.projects.project(id)?;
            let selected = index == prompt.chosen;
            let color = style::project_color(super::ProjectLink::of(project).hue(), theme);
            Some(
                ui::control(
                    SharedString::from(format!("ai-share-choice-{index}")),
                    "",
                    theme,
                    true,
                )
                .when(cfg!(test), move |v| {
                    v.debug_selector(move || format!("ai-share-choice-{index}"))
                })
                .role(gpui::Role::RadioButton)
                .aria_label(project.name.clone())
                .aria_selected(selected)
                .flex()
                .items_center()
                .gap_2()
                .child(
                    div()
                        .flex_shrink_0()
                        .size(px(14.))
                        .rounded_full()
                        .border_1()
                        .border_color(if selected { p.header_fg } else { p.border })
                        .flex()
                        .items_center()
                        .justify_center()
                        .when(selected, |v| {
                            v.child(div().size(px(6.)).rounded_full().bg(p.header_fg))
                        }),
                )
                .child(div().flex_shrink_0().size(px(8.)).rounded_full().bg(color))
                .child(div().min_w_0().truncate().child(project.name.clone()))
                .on_click(cx.listener(move |this, _, _, cx| {
                    this.ai.choose(index);
                    cx.notify();
                })),
            )
        });
        let card = ui::panel("ai-share-prompt", theme)
            .when(cfg!(test), |v| {
                v.debug_selector(|| "ai-share-prompt".into())
            })
            .role(gpui::Role::Group)
            .aria_label(title.clone())
            .w(px(320.))
            .max_w_full()
            .p_4()
            .rounded_lg()
            .shadow_lg()
            .flex()
            .flex_col()
            .gap_2()
            .child(
                div()
                    .text_size(px(14.))
                    .font_weight(gpui::FontWeight::SEMIBOLD)
                    .child(title),
            )
            .child(ui::card_text(
                "Share a Project with AI apps? They read only its Ready documents, \
                 pseudonymized.",
                theme,
            ))
            .child(
                div()
                    .id("ai-share-choices")
                    .role(gpui::Role::RadioGroup)
                    .aria_label("Project to share")
                    .flex()
                    .flex_col()
                    .gap(px(2.))
                    .children(rows),
            )
            .child(
                ui::card_actions()
                    .justify_end()
                    .child(
                        ui::action_control("ai-share-not-now", "Not now", theme, true, false)
                            .when(cfg!(test), |v| {
                                v.debug_selector(|| "ai-share-not-now".into())
                            })
                            .on_click(cx.listener(|this, _, _, cx| {
                                this.ai.not_now(std::time::Instant::now());
                                cx.notify();
                            })),
                    )
                    .child(
                        ui::primary_button("ai-share", "Share", theme, true)
                            .when(cfg!(test), |v| v.debug_selector(|| "ai-share".into()))
                            .on_click(cx.listener(|this, _, _, cx| {
                                if let Some(project) = this.ai.take_choice() {
                                    this.share_with_ai(&project, cx);
                                }
                            })),
                    ),
            );
        Some(
            gpui::deferred(div().absolute().bottom(px(16.)).left(px(16.)).child(card))
                .with_priority(1)
                .into_any_element(),
        )
    }

    /// Under the Project page's counts, while it is the AI project.
    pub(super) fn ai_project_chip(
        &self,
        project: &Project,
        cx: &mut Context<Self>,
    ) -> Option<AnyElement> {
        if self.ai_shared().as_deref() != Some(project.id.as_str()) {
            return None;
        }
        let theme = self.theme.get();
        let accent = theme.search_accent();
        Some(
            div()
                .id("project-page-ai")
                .when(cfg!(test), |v| {
                    v.debug_selector(|| "project-page-ai".into())
                })
                .flex()
                .items_center()
                .gap_2()
                .text_size(px(12.))
                .child(
                    div()
                        .px_2()
                        .py(px(1.))
                        .rounded_full()
                        .border_1()
                        .border_color(accent)
                        .text_color(accent)
                        .child("Shared with AI apps"),
                )
                .child(
                    self.page_control("project-page-stop-sharing", "Stop sharing", cx)
                        .text_color(accent)
                        .on_click(cx.listener(|this, _, _, cx| this.stop_sharing_with_ai(cx))),
                )
                .into_any_element(),
        )
    }

    /// The Project page's AI activity: which app read which document, and
    /// each delivery, newest first.
    pub(super) fn ai_activity_segment(
        &self,
        project: &Project,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let p = self.theme.get().pdf_style();
        let mut list = div()
            .id("project-activity")
            .when(cfg!(test), |v| {
                v.debug_selector(|| "project-activity".into())
            })
            .flex()
            .flex_col()
            .gap_1();
        let entries = match self.ai.activity(&project.id) {
            Some(Ok(entries)) => entries.as_slice(),
            Some(Err(error)) => {
                return list
                    .child(ui::card_error(
                        format!("The AI activity can't be read. {error}"),
                        self.theme.get(),
                    ))
                    .into_any_element();
            }
            None => &[],
        };
        if entries.is_empty() {
            return list
                .child(
                    div()
                        .py_2()
                        .text_size(px(12.))
                        .text_color(p.header_muted)
                        .child("No AI app has read this Project yet."),
                )
                .into_any_element();
        }
        let vault = self.vault.vault();
        let documents: HashMap<String, String> = (vault.into_iter())
            .flat_map(|vault| vault.documents())
            .map(|summary| (summary.id, summary.title))
            .collect();
        let replies: HashMap<String, String> = (vault.into_iter())
            .flat_map(|vault| vault.project_replies(&project.id))
            .map(|reply| (reply.id, reply.title))
            .collect();
        let now = super::home::now();
        for (index, entry) in entries.iter().take(ACTIVITY_SHOWN).enumerate() {
            // Numbered from the oldest, so a row keeps its control (and
            // focus) as newer lines arrive above it.
            let number = entries.len() - index;
            let line = activity_line(entry, &documents, &replies, now);
            let target = line.opens.clone();
            list = list.child(
                div()
                    .flex()
                    .flex_col()
                    .py_1()
                    .child(
                        self.page_control(
                            SharedString::from(format!("project-activity-{number}")),
                            line.text,
                            cx,
                        )
                        .truncate()
                        .when(target.is_none(), |v| v.cursor_default())
                        .on_click(cx.listener(
                            move |this, _, window, cx| match &target {
                                Some(Opens::Document(id)) => {
                                    if let Some(id) = VaultId::parse(id) {
                                        this.open_project_member(id, window, cx);
                                    }
                                }
                                Some(Opens::Reply(id)) => this.open_reply(id, window, cx),
                                None => {}
                            },
                        )),
                    )
                    .child(
                        div()
                            .px_2()
                            .text_size(px(11.))
                            .text_color(p.header_muted)
                            .truncate()
                            .child(line.detail),
                    ),
            );
        }
        if entries.len() > ACTIVITY_SHOWN {
            list = list.child(
                div()
                    .py_2()
                    .text_size(px(12.))
                    .text_color(p.header_muted)
                    .child(format!(
                        "Showing the newest {ACTIVITY_SHOWN} of {}.",
                        entries.len()
                    )),
            );
        }
        list.into_any_element()
    }
}

/// What a row of the AI activity opens.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(super) enum Opens {
    Document(String),
    Reply(String),
}

/// One row of the AI activity as the page shows it.
#[derive(Debug, PartialEq, Eq)]
pub(super) struct ActivityLine {
    pub(super) text: String,
    pub(super) detail: String,
    pub(super) opens: Option<Opens>,
}

/// `entry` in words: the app by its product's name, the documents and
/// replies by their titles in mdoc (`documents`, `replies`), and how long
/// ago at `now`.
pub(super) fn activity_line(
    entry: &AiActivity,
    documents: &HashMap<String, String>,
    replies: &HashMap<String, String>,
    now: u64,
) -> ActivityLine {
    let app = ai_connector::display_name(&entry.app);
    let quoted = |title: &String| format!("\u{201c}{title}\u{201d}");
    let document = |id: &str| documents.get(id).map(quoted);
    let age = super::home::age(entry.at, now);
    match &entry.event {
        AiEvent::Read { document: id, part } => {
            let title = document(id);
            let part = part
                .map(|part| format!(" · part {part}"))
                .unwrap_or_default();
            ActivityLine {
                text: format!(
                    "{app} read {}{part}",
                    title.clone().unwrap_or_else(|| "a deleted document".into())
                ),
                detail: age,
                opens: title.map(|_| Opens::Document(id.clone())),
            }
        }
        AiEvent::Delivered {
            reply,
            document: delivered,
            read,
        } => {
            let (text, opens) = match delivered {
                Some(id) => match document(id) {
                    Some(title) => (
                        format!("{app} delivered {title} as a document"),
                        Some(Opens::Document(id.clone())),
                    ),
                    None => (format!("{app} delivered a deleted document"), None::<Opens>),
                },
                None => match replies.get(reply) {
                    Some(title) => (
                        format!("{app} delivered a reply {}", quoted(title)),
                        Some(Opens::Reply(reply.clone())),
                    ),
                    None => (format!("{app} delivered a deleted reply"), None),
                },
            };
            let from = match read.as_slice() {
                [] => None,
                [first, rest @ ..] => {
                    let first = document(first).unwrap_or_else(|| "a deleted document".into());
                    Some(match rest.len() {
                        0 => format!("from {first}"),
                        more => format!("from {first} and {more} more"),
                    })
                }
            };
            ActivityLine {
                text,
                detail: [Some(age), from]
                    .into_iter()
                    .flatten()
                    .collect::<Vec<_>>()
                    .join(" · "),
                opens,
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn titles(pairs: &[(&str, &str)]) -> HashMap<String, String> {
        (pairs.iter())
            .map(|(id, title)| (id.to_string(), title.to_string()))
            .collect()
    }

    #[test]
    fn activity_reads_as_who_did_what_and_when() {
        let documents = titles(&[("a", "Lease"), ("b", "Notice"), ("d", "Summary")]);
        let replies = titles(&[("r", "Reply to Lease")]);
        let now = 10_000;
        let read = |document: &str, part, app: &str| AiActivity {
            at: now - 300,
            app: app.into(),
            event: AiEvent::Read {
                document: document.into(),
                part,
            },
        };
        assert_eq!(
            activity_line(&read("a", None, "claude-ai"), &documents, &replies, now),
            ActivityLine {
                text: "Claude read “Lease”".into(),
                detail: "5 min ago".into(),
                opens: Some(Opens::Document("a".into())),
            }
        );
        let part = activity_line(&read("a", Some(2), "codex"), &documents, &replies, now);
        assert_eq!(part.text, "Codex read “Lease” · part 2");
        let gone = activity_line(&read("x", None, ""), &documents, &replies, now);
        assert_eq!(gone.text, "An AI app read a deleted document");
        assert_eq!(gone.opens, None);
        let delivered = |document: Option<&str>, read: &[&str]| AiActivity {
            at: now,
            app: "claude-ai".into(),
            event: AiEvent::Delivered {
                reply: "r".into(),
                document: document.map(str::to_string),
                read: read.iter().map(|id| id.to_string()).collect(),
            },
        };
        assert_eq!(
            activity_line(&delivered(None, &["a"]), &documents, &replies, now),
            ActivityLine {
                text: "Claude delivered a reply “Reply to Lease”".into(),
                detail: "just now · from “Lease”".into(),
                opens: Some(Opens::Reply("r".into())),
            }
        );
        assert_eq!(
            activity_line(
                &delivered(Some("d"), &["a", "b", "x"]),
                &documents,
                &replies,
                now
            ),
            ActivityLine {
                text: "Claude delivered “Summary” as a document".into(),
                detail: "just now · from “Lease” and 2 more".into(),
                opens: Some(Opens::Document("d".into())),
            }
        );
    }
}
