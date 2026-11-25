//! What the window knows of the AI project (ADR 0126; #1164 Decisions 2
//! and 5; Milestone 2 item 8): the Project the user last viewed, the share
//! prompt, and the AI activity the Project page shows. The share itself is
//! the connector's, in memory only; nothing here changes it. The view
//! carries out what each change returns. GPUI-free.
use mdoc_vault::AiActivity;
use std::time::{Duration, Instant};

/// After "Not now", an ask this soon shows nothing.
pub(super) const QUIET: Duration = Duration::from_secs(10 * 60);

/// At most this many Projects to choose from in the prompt.
pub(super) const CHOICES: usize = 4;

/// The share prompt while it shows.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(super) struct Prompt {
    /// The AI app that asked, as it named itself.
    pub(super) app: String,
    /// The Projects offered, by id: the last viewed first.
    pub(super) choices: Vec<String>,
    /// The one chosen, an index into `choices`.
    pub(super) chosen: usize,
}

/// What an ask asks of the window.
#[derive(Debug, PartialEq, Eq)]
pub(super) enum Asked {
    /// The prompt shows now.
    Prompt,
    /// There is no Project to share: say so, once.
    NoProject(String),
    /// Nothing: it shows already, or the user said "Not now" lately.
    Nothing,
}

#[derive(Default)]
pub(super) struct AiProject {
    /// The Project of the place the user last visited: its page, one of
    /// its documents or replies. Unfiled is no Project.
    last_viewed: Option<String>,
    prompt: Option<Prompt>,
    /// "Not now" was chosen: no prompt before then.
    quiet_until: Option<Instant>,
    /// There was no Project to share, and the window said so: it doesn't
    /// say so again before then. A Project made meanwhile is offered at
    /// once.
    said_no_project_until: Option<Instant>,
    /// A Project's AI activity as last read, newest first, or why it
    /// could not be.
    activity: Option<(String, Result<Vec<AiActivity>, String>)>,
    /// The tabs and their documents whose scan on entering the AI project
    /// started: each scans once in that tab until it leaves the AI
    /// project. A document opened again, in a new tab, scans again.
    entry_scans: std::collections::HashSet<(u64, String)>,
}

impl AiProject {
    /// The user visited a place of Project `project`, or of none.
    pub(super) fn viewed(&mut self, project: Option<String>) {
        if project.is_some() {
            self.last_viewed = project;
        }
    }

    #[cfg(test)]
    pub(super) fn last_viewed(&self) -> Option<&str> {
        self.last_viewed.as_deref()
    }

    pub(super) fn prompt(&self) -> Option<&Prompt> {
        self.prompt.as_ref()
    }

    /// AI app `app` asked while nothing was shared. `projects`: every
    /// Project, pinned first, then the most recently updated.
    pub(super) fn asked(&mut self, app: String, projects: &[String], now: Instant) -> Asked {
        if self.prompt.is_some() || self.quiet_until.is_some_and(|until| now < until) {
            return Asked::Nothing;
        }
        let last = (self.last_viewed.as_ref()).filter(|last| projects.contains(last));
        let choices: Vec<String> = (last.into_iter())
            .chain(projects.iter().filter(|id| Some(*id) != last))
            .take(CHOICES)
            .cloned()
            .collect();
        if choices.is_empty() {
            if self.said_no_project_until.is_some_and(|until| now < until) {
                return Asked::Nothing;
            }
            self.said_no_project_until = Some(now + QUIET);
            return Asked::NoProject(app);
        }
        self.quiet_until = None;
        self.prompt = Some(Prompt {
            app,
            choices,
            chosen: 0,
        });
        Asked::Prompt
    }

    /// Choose the `index`th Project offered.
    pub(super) fn choose(&mut self, index: usize) {
        if let Some(prompt) = self.prompt.as_mut()
            && index < prompt.choices.len()
        {
            prompt.chosen = index;
        }
    }

    /// Share: the prompt closes, and its chosen Project is the one to share.
    pub(super) fn take_choice(&mut self) -> Option<String> {
        let prompt = self.prompt.take()?;
        prompt.choices.into_iter().nth(prompt.chosen)
    }

    /// "Not now": the prompt closes, and asks within [`QUIET`] show nothing.
    pub(super) fn not_now(&mut self, now: Instant) {
        if self.prompt.take().is_some() {
            self.quiet_until = Some(now + QUIET);
        }
    }

    /// Something was shared by other means: no prompt is needed.
    pub(super) fn shared(&mut self) {
        self.prompt = None;
        self.entry_scans.clear();
    }

    /// Project `id` was deleted.
    pub(super) fn forget(&mut self, id: &str) {
        if self.last_viewed.as_deref() == Some(id) {
            self.last_viewed = None;
        }
        if let Some(prompt) = self.prompt.as_mut()
            && let Some(at) = prompt.choices.iter().position(|choice| choice == id)
        {
            prompt.choices.remove(at);
            if prompt.choices.is_empty() {
                self.prompt = None;
            } else if prompt.chosen >= at {
                prompt.chosen = prompt.chosen.saturating_sub(1);
            }
        }
        if self.activity.as_ref().is_some_and(|(of, _)| of == id) {
            self.activity = None;
        }
    }

    /// The connector stopped: nothing of it shows.
    pub(super) fn stopped(&mut self) {
        let last_viewed = self.last_viewed.take();
        *self = Self {
            last_viewed,
            ..Self::default()
        };
    }

    /// Project `project`'s activity as last read, if it was.
    pub(super) fn activity(&self, project: &str) -> Option<&Result<Vec<AiActivity>, String>> {
        (self.activity.as_ref())
            .filter(|(of, _)| of == project)
            .map(|(_, activity)| activity)
    }

    /// The Project whose activity is held.
    pub(super) fn activity_of(&self) -> Option<&str> {
        self.activity.as_ref().map(|(of, _)| of.as_str())
    }

    pub(super) fn set_activity(&mut self, project: String, read: Result<Vec<AiActivity>, String>) {
        self.activity = Some((project, read));
    }

    /// Document `id`, in tab `tab`, entered the AI project: `true` the
    /// first time, until it [`left`](Self::left) it.
    pub(super) fn entering(&mut self, tab: u64, id: &str) -> bool {
        self.entry_scans.insert((tab, id.to_string()))
    }

    /// Document `id`, in tab `tab`, is not in the AI project now.
    pub(super) fn left(&mut self, tab: u64, id: &str) {
        self.entry_scans.remove(&(tab, id.to_string()));
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn ids(names: &[&str]) -> Vec<String> {
        names.iter().map(|name| name.to_string()).collect()
    }

    #[test]
    fn the_prompt_offers_the_last_viewed_project_first() {
        let now = Instant::now();
        let mut ai = AiProject::default();
        ai.viewed(Some("b".into()));
        // Unfiled is no Project: the last one viewed stays.
        ai.viewed(None);
        let projects = ids(&["a", "b", "c", "d", "e"]);
        assert_eq!(ai.asked("claude-ai".into(), &projects, now), Asked::Prompt);
        let prompt = ai.prompt().unwrap();
        assert_eq!(prompt.choices, ids(&["b", "a", "c", "d"]));
        assert_eq!(prompt.chosen, 0);
        // A second ask while it shows stacks nothing.
        assert_eq!(ai.asked("chatgpt".into(), &projects, now), Asked::Nothing);
        assert_eq!(ai.prompt().unwrap().app, "claude-ai");
        ai.choose(2);
        ai.choose(9);
        assert_eq!(ai.take_choice().as_deref(), Some("c"));
        assert!(ai.prompt().is_none());
    }

    #[test]
    fn not_now_keeps_quiet_for_a_while() {
        let now = Instant::now();
        let mut ai = AiProject::default();
        let projects = ids(&["a"]);
        assert_eq!(ai.asked("claude-ai".into(), &projects, now), Asked::Prompt);
        ai.not_now(now);
        assert!(ai.prompt().is_none());
        let soon = now + QUIET - Duration::from_secs(1);
        assert_eq!(
            ai.asked("claude-ai".into(), &projects, soon),
            Asked::Nothing
        );
        assert_eq!(
            ai.asked("claude-ai".into(), &projects, now + QUIET),
            Asked::Prompt
        );
    }

    #[test]
    fn with_no_project_it_says_so_once_and_offers_one_made_since() {
        let now = Instant::now();
        let mut ai = AiProject::default();
        assert_eq!(
            ai.asked("claude-ai".into(), &[], now),
            Asked::NoProject("claude-ai".into())
        );
        assert_eq!(ai.asked("claude-ai".into(), &[], now), Asked::Nothing);
        // A Project made after the notice is offered at once.
        assert_eq!(
            ai.asked("claude-ai".into(), &ids(&["new"]), now),
            Asked::Prompt
        );
        ai.not_now(now);
        // A deleted last-viewed Project is not offered.
        ai.viewed(Some("gone".into()));
        assert_eq!(
            ai.asked("claude-ai".into(), &ids(&["a"]), now + QUIET),
            Asked::Prompt
        );
        assert_eq!(ai.prompt().unwrap().choices, ids(&["a"]));
    }

    #[test]
    fn a_deleted_project_leaves_the_prompt() {
        let now = Instant::now();
        let mut ai = AiProject::default();
        ai.asked("claude-ai".into(), &ids(&["a", "b"]), now);
        ai.choose(1);
        ai.forget("a");
        assert_eq!(ai.prompt().unwrap().choices, ids(&["b"]));
        assert_eq!(ai.take_choice().as_deref(), Some("b"));
        ai.asked("claude-ai".into(), &ids(&["a"]), now);
        ai.forget("a");
        assert!(ai.prompt().is_none());
    }

    #[test]
    fn a_document_scans_once_on_entry_until_it_leaves() {
        let mut ai = AiProject::default();
        assert!(ai.entering(1, "d"));
        assert!(!ai.entering(1, "d"));
        // Opened again in a new tab: it scans there.
        assert!(ai.entering(2, "d"));
        ai.left(1, "d");
        assert!(ai.entering(1, "d"));
        // A new share: every member enters again.
        ai.shared();
        assert!(ai.entering(1, "d"));
        ai.viewed(Some("p".into()));
        ai.stopped();
        assert!(ai.entering(1, "d"));
        assert_eq!(ai.last_viewed(), Some("p"));
    }
}
