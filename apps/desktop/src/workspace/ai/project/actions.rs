//! The AI project's actions: the connector's start, refresh, poll and stop,
//! an AI app's ask, the share and Stop sharing, a scan on entry and the
//! activity's load. The share lives in the connector, in memory only;
//! nothing here writes the Vault.
use crate::workspace::ai::text;
use crate::workspace::{
    Workspace, ai::project::Asked, projects::project_page::Place, sidebar::navigation::Stop,
};
use gpui::{App, Context, Window};
use mdoc_store::settings::AiConnectorConfig;
use mdoc_vault::ReplyOwner;
use std::{collections::HashMap, time::Duration};

/// How often the window hears from the connector.
const POLL: Duration = Duration::from_millis(250);

/// Whether `MDOC_DEV_AI_PROJECT` was shared already in this process.
#[cfg(debug_assertions)]
static DEV_SHARED: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(false);

impl Workspace {
    /// "AI connector (preview)" as Settings holds it.
    pub(in crate::workspace) fn ai_setting(&self) -> AiConnectorConfig {
        (self.preferences.borrow().current.as_ref())
            .map(|preferences| preferences.ai_connector)
            .unwrap_or_default()
    }

    /// "AI connector (preview)" is on, the Vault open or not.
    pub(crate) fn ai_preview_setting(&self) -> bool {
        self.ai_setting().preview
    }

    /// "AI connector (preview)" on as saved and as Settings shows it: shown
    /// off, its save not landed yet, it is off (#1220, #1273). Whatever
    /// starts the connector or takes the hotkey asks this.
    pub(in crate::workspace) fn ai_preview_wanted(&self, cx: &App) -> bool {
        self.ai_setting().preview && !self.settings.read(cx).ai_connector_shown_off()
    }

    /// The connector runs: the setting is on and the Vault is open.
    pub(in crate::workspace) fn ai_connector_on(&self) -> bool {
        self.ai_connector.is_some()
    }

    /// The AI project, while the window holds it.
    pub(in crate::workspace) fn ai_shared(&self) -> Option<String> {
        let shared = self.ai_connector.as_ref()?.connector.ai_project()?;
        self.projects
            .project(&shared)
            .map(|project| project.id.clone())
    }

    /// With "AI connector (preview)" on, the connector once the Vault is
    /// open, and the window hears from it. Tests and headless renders get
    /// no listener: they keep to their own Vaults and never take the
    /// profile's endpoint, unless a test gives one. Settings showing the
    /// preview off starts nothing, whatever is saved (#1273).
    pub(in crate::workspace) fn start_ai_connector(
        &mut self,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if self.ai_connector.is_some() || !self.ai_preview_wanted(cx) {
            return;
        }
        let Some(vault) = self.vault.vault().cloned() else {
            return;
        };
        let setting = self.ai_setting();
        #[cfg(test)]
        if let Some(endpoint) = self.ai_endpoint.clone() {
            let started = crate::ai::connector::start(vault, &setting, Some(&endpoint));
            self.ai_started(started, window, cx);
            return;
        }
        let started = if cfg!(test) || mdoc_store::app_paths::isolated() {
            Ok(crate::ai::connector::start_unlistened(vault, &setting))
        } else {
            crate::ai::connector::start(vault, &setting, crate::ai::connector::endpoint().as_ref())
        };
        self.ai_started(started, window, cx);
    }

    /// What a start gave: the running connector, or why it couldn't
    /// start, which Settings › AI connector shows (#1220).
    pub(in crate::workspace) fn ai_started(
        &mut self,
        started: Result<Option<crate::ai::connector::Running>, String>,
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
        // The real-model check (`scripts/dev/connector-real-model.sh`) has
        // no one to click Share: a debug build on a dev profile shares the
        // Project this variable names, as the prompt's Share would. Only
        // the first start: turned off and on in Settings, it shares nothing.
        #[cfg(debug_assertions)]
        if mdoc_store::app_paths::dev_profile().is_some()
            && let Ok(project) = std::env::var("MDOC_DEV_AI_PROJECT")
            && !DEV_SHARED.swap(true, std::sync::atomic::Ordering::Relaxed)
        {
            self.share_with_ai(&project, cx);
        }
        cx.notify();
    }

    /// Settings applied, or a model check finished: the connector starts
    /// or stops with the setting as saved and shown (#1273). Stopping it
    /// shares nothing.
    pub(in crate::workspace) fn refresh_ai_connector(
        &mut self,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        // With the preview on, the menu bar item toggles the companion.
        crate::shelf::menu_bar::set_companion(self.ai_setting().preview, cx);
        self.refresh_hotkey(cx);
        if self.ai_preview_wanted(cx) {
            self.start_ai_connector(window, cx);
        } else {
            self.stop_ai_connector(cx);
        }
    }

    /// The companion's hotkey follows the saved setting: registered only
    /// while the companion can open (the preview and the menu bar item
    /// on), and Settings shows why it couldn't be. The preview shown off,
    /// its save not landed yet (#1220), counts as off.
    pub(crate) fn refresh_hotkey(&mut self, cx: &mut Context<Self>) {
        let setting = self.ai_setting();
        let item = self.menu_bar_setting() && crate::shelf::menu_bar::keeps_running(cx);
        let wanted =
            (self.ai_preview_wanted(cx) && !setting.hotkey_off && item).then_some(setting.hotkey);
        let failed = crate::ai::companion::hotkey::set(wanted, cx);
        self.settings
            .update(cx, |panel, cx| panel.hotkey_failed(failed, cx));
    }

    /// The switch shows off, saved or not yet (#1220): nothing more
    /// reaches an AI app, and nothing is shared.
    pub(in crate::workspace) fn stop_ai_connector(&mut self, cx: &mut Context<Self>) {
        self.settings
            .update(cx, |panel, cx| panel.ai_connector_failed(None, cx));
        crate::ai::companion::hotkey::set(None, cx);
        self.settings
            .update(cx, |panel, cx| panel.hotkey_failed(None, cx));
        if self.ai_connector.take().is_some() {
            self.ai_poll = None;
            self.ai.stopped();
            self.ai_ask_cancelled();
            if (self.project_ui.page.as_ref()).is_some_and(|page| {
                page.segment() == crate::workspace::projects::project_page::Segment::Activity
            }) {
                self.show_project_segment(
                    crate::workspace::projects::project_page::Segment::Documents,
                    cx,
                );
            }
            cx.notify();
        }
    }

    /// What the connector said since the last poll; `false` once it has
    /// stopped.
    pub(in crate::workspace) fn poll_ai_connector(
        &mut self,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> bool {
        // A picker that ended since the last poll may have been up when a
        // delivery in this batch arrived: that one waits on the badge too.
        let ended = crate::app::native_picker::ended(cx);
        let picker_was_up = ended != self.pickers_ended;
        self.pickers_ended = ended;
        let Some(running) = &self.ai_connector else {
            return false;
        };
        let mut activity = false;
        for event in running.events() {
            match event {
                crate::ai::connector::Event::Asked { app } => self.ai_app_asked(app, window, cx),
                crate::ai::connector::Event::Activity { document } => {
                    activity = true;
                    // Ready since, it is read: the companion's request is
                    // answered.
                    if (self.ai.review_ask()).is_some_and(|ask| ask.document == document) {
                        self.ai.read(&document);
                        crate::shelf::menu_bar::companion_changed(cx);
                    }
                }
                crate::ai::connector::Event::Delivered {
                    app,
                    reply,
                    document,
                } => {
                    activity = true;
                    self.ai_delivered(&app, reply, document, picker_was_up, window, cx);
                }
                crate::ai::connector::Event::NeedsReview {
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
                self.notices.post(text::asked_with_no_project(
                    crate::ai::connector::display_name(&app),
                ));
                cx.notify();
            }
            Asked::Nothing => {}
        }
    }

    /// The prompt's Share, and Ask's: Project `project` is the AI project
    /// from now on, in place of any other, and its members not yet scanned
    /// scan. `false` when it could not be shared, which a notice says.
    pub(in crate::workspace) fn share_with_ai(
        &mut self,
        project: &str,
        cx: &mut Context<Self>,
    ) -> bool {
        let Some(running) = &self.ai_connector else {
            return false;
        };
        let shared = match running.connector.share(project) {
            Ok(()) => {
                self.ai.shared();
                self.scan_on_entry(cx);
                crate::shelf::menu_bar::companion_changed(cx);
                true
            }
            Err(error) => {
                self.notices.post(text::could_not_share(error));
                false
            }
        };
        cx.notify();
        shared
    }

    /// Stop sharing: AI apps read nothing until a Project is shared again.
    pub(in crate::workspace) fn stop_sharing_with_ai(&mut self, cx: &mut Context<Self>) {
        if let Some(running) = &self.ai_connector {
            running.connector.stop_sharing();
        }
        self.ai.sharing_stopped();
        // A later Stop wins over an Ask still waiting for its move.
        self.ai_ask_cancelled();
        crate::shelf::menu_bar::companion_changed(cx);
        cx.notify();
    }

    /// Project `id` was deleted: it is shared no more, or offered.
    pub(in crate::workspace) fn ai_project_gone(&mut self, id: &str, cx: &mut App) {
        if let Some(running) = &self.ai_connector
            && running.connector.ai_project().as_deref() == Some(id)
        {
            running.connector.stop_sharing();
            crate::shelf::menu_bar::companion_changed(cx);
        }
        self.ai.forget(id);
    }

    /// The user arrived at a place, recorded in the history or not (a
    /// closed tab's neighbour comes to the front unrecorded): its Project
    /// is the one last viewed.
    pub(in crate::workspace) fn note_ai_viewed(&mut self, stop: &Stop) {
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
    pub(in crate::workspace) fn scan_on_entry(&mut self, cx: &mut Context<Self>) {
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
    pub(in crate::workspace) fn load_ai_activity(&mut self, project: &str) {
        let Some(vault) = self.vault.vault() else {
            return;
        };
        let read = vault
            .ai_activity(project)
            .map_err(|error| error.to_string());
        self.ai.set_activity(project.to_string(), read);
    }

    /// The titles Project `project`'s AI activity names, by id: every
    /// document's, and the Project's replies'.
    pub(in crate::workspace) fn activity_titles(
        &self,
        project: &str,
    ) -> (HashMap<String, String>, HashMap<String, String>) {
        let vault = self.vault.vault();
        let documents: HashMap<String, String> = (vault.into_iter())
            .flat_map(|vault| vault.documents())
            .map(|summary| (summary.id, summary.title))
            .collect();
        let replies: HashMap<String, String> = (vault.into_iter())
            .flat_map(|vault| vault.project_replies(project))
            .map(|reply| (reply.id, reply.title))
            .collect();
        (documents, replies)
    }

    /// The rail's AI icon: the newest arrival while any waits (`counts`),
    /// else the shared Project's page (`open`).
    pub(in crate::workspace) fn ai_rail_clicked(
        &mut self,
        counts: bool,
        open: Option<&str>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if counts {
            self.open_newest(window, cx);
        } else if let Some(id) = open {
            self.show_project_page(id, Place::Header, window, cx);
        }
    }

    /// The share prompt's row `index` is chosen.
    pub(in crate::workspace) fn choose_ai_share(&mut self, index: usize, cx: &mut Context<Self>) {
        self.ai.choose(index);
        cx.notify();
    }

    /// The share prompt's Not now.
    pub(in crate::workspace) fn ai_share_not_now(&mut self, cx: &mut Context<Self>) {
        self.ai.not_now(std::time::Instant::now());
        cx.notify();
    }

    /// The share prompt's Share: the chosen Project, if any.
    pub(in crate::workspace) fn share_ai_choice(&mut self, cx: &mut Context<Self>) {
        if let Some(project) = self.ai.take_choice() {
            self.share_with_ai(&project, cx);
        }
    }
}
