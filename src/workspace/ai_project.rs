//! What the window knows of the AI project (ADR 0126; #1164 Decisions 2,
//! 4, 5 and 6; Milestone 2 items 8 and 9): the Project the user last
//! viewed, the share prompt, the AI activity the Project page shows, and
//! what arrived from AI apps that the user has not opened yet (the badge
//! and the notifications), and what the companion window shows of them:
//! the latest delivered reply and the latest review request (item 14).
//! The share itself is the connector's, in memory only; nothing here
//! changes it. The view carries out what each change returns. GPUI-free.
use mdoc_vault::AiActivity;
use std::time::{Duration, Instant};

/// After "Not now", an ask this soon shows nothing.
pub(super) const QUIET: Duration = Duration::from_secs(10 * 60);

/// At most this many Projects to choose from in the prompt.
pub(super) const CHOICES: usize = 4;

/// After a review request's notification, the same document raises no
/// other this soon, unless the user opened it meanwhile.
pub(super) const REVIEW_QUIET: Duration = Duration::from_secs(10 * 60);

/// Something an AI app sent or asked for, by id: what a notification and
/// the badge open.
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub(super) enum Arrived {
    /// A delivered AI Reply: its Reply page.
    Reply(String),
    /// A delivered Document: it opens for review.
    Document(String),
    /// A document an AI app asked for that needs review: it opens for
    /// review.
    Review(String),
}

impl Arrived {
    const REPLY: &str = "ai-reply-";
    const DOCUMENT: &str = "ai-document-";
    const REVIEW: &str = "ai-review-";

    /// Its notification's tag: ids only, never a name.
    pub(super) fn tag(&self) -> String {
        match self {
            Self::Reply(id) => format!("{}{id}", Self::REPLY),
            Self::Document(id) => format!("{}{id}", Self::DOCUMENT),
            Self::Review(id) => format!("{}{id}", Self::REVIEW),
        }
    }

    /// What the notification tagged `tag` opens; `None` for another's.
    pub(super) fn from_tag(tag: &str) -> Option<Self> {
        let id = |prefix: &str| {
            (tag.strip_prefix(prefix))
                .filter(|id| !id.is_empty())
                .map(str::to_string)
        };
        (id(Self::REPLY).map(Self::Reply))
            .or_else(|| id(Self::DOCUMENT).map(Self::Document))
            .or_else(|| id(Self::REVIEW).map(Self::Review))
    }
}

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

/// The latest reply an AI app delivered, as the companion shows it: by
/// id, never its text.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(super) struct LatestReply {
    pub(super) reply: String,
    /// The AI app that delivered it, as it named itself.
    pub(super) app: String,
    /// It is a Document already: Save as document ran on it, or it was
    /// delivered as one.
    pub(super) saved: bool,
    /// Save as document is queued and has not ended yet.
    pub(super) saving: bool,
    /// The last Save as document failed (mdoc's notice says why).
    pub(super) failed: bool,
}

/// The latest request for a document that needs review, as the
/// companion shows it.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(super) struct ReviewAsk {
    pub(super) document: String,
    /// Its AI label, as the AI app was told it.
    pub(super) label: String,
    pub(super) app: String,
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
    /// What arrived and the user has not opened yet, oldest first: the
    /// badge counts it.
    unseen: Vec<Arrived>,
    /// Review requests notified lately, by document: no other before then.
    reviews_quiet: std::collections::HashMap<String, Instant>,
    /// Documents opened for review whose Review panel opens once loaded.
    to_review: std::collections::HashSet<String>,
    /// The companion's latest reply and review request.
    latest_reply: Option<LatestReply>,
    review_ask: Option<ReviewAsk>,
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
        self.review_ask = None;
    }

    /// `item` arrived at `now`: the badge counts it, newest last. `true`
    /// when it is to raise a notification: always for a delivery; for a
    /// review request, unless the same document raised one within
    /// [`REVIEW_QUIET`] and was not opened since.
    pub(super) fn arrived(&mut self, item: Arrived, now: Instant) -> bool {
        self.unseen.retain(|unseen| *unseen != item);
        let notify = match &item {
            Arrived::Review(id) => {
                let quiet = (self.reviews_quiet.get(id)).is_some_and(|until| now < *until);
                if !quiet {
                    self.reviews_quiet.insert(id.clone(), now + REVIEW_QUIET);
                }
                !quiet
            }
            Arrived::Reply(_) | Arrived::Document(_) => true,
        };
        self.unseen.push(item);
        notify
    }

    /// AI app `app` delivered reply `reply`: the companion's latest.
    /// `as_document`: it landed as a Document too, so it is saved already.
    pub(super) fn delivered(&mut self, reply: String, app: String, as_document: bool) {
        self.latest_reply = Some(LatestReply {
            reply,
            app,
            saved: as_document,
            saving: false,
            failed: false,
        });
    }

    /// AI app `app` asked for `document`, named to it as `label`, which
    /// needs review: the companion's latest request.
    pub(super) fn review_asked(&mut self, document: String, label: String, app: String) {
        self.review_ask = Some(ReviewAsk {
            document,
            label,
            app,
        });
    }

    pub(super) fn latest_reply(&self) -> Option<&LatestReply> {
        self.latest_reply.as_ref()
    }

    pub(super) fn review_ask(&self) -> Option<&ReviewAsk> {
        self.review_ask.as_ref()
    }

    /// The latest reply, if it is `reply`.
    fn latest_mut(&mut self, reply: &str) -> Option<&mut LatestReply> {
        (self.latest_reply.as_mut()).filter(|latest| latest.reply == reply)
    }

    /// Save as document was queued for reply `reply`.
    pub(super) fn reply_saving(&mut self, reply: &str) {
        if let Some(latest) = self.latest_mut(reply) {
            latest.saving = true;
            latest.failed = false;
        }
    }

    /// The promotion of reply `reply` ended without a Document.
    pub(super) fn reply_not_saved(&mut self, reply: &str) {
        if let Some(latest) = self.latest_mut(reply).filter(|latest| latest.saving) {
            latest.saving = false;
            latest.failed = true;
        }
    }

    /// Reply `reply` was promoted to a Document: it is not saved twice.
    pub(super) fn reply_saved(&mut self, reply: &str) {
        if let Some(latest) = self.latest_mut(reply) {
            latest.saved = true;
            latest.saving = false;
            latest.failed = false;
        }
    }

    /// What arrived and is not opened yet, oldest first.
    pub(super) fn unseen(&self) -> &[Arrived] {
        &self.unseen
    }

    /// The user arrived at reply `reply`, or at document `document`: what
    /// arrived for it is seen, and the document may raise a review
    /// request again.
    pub(super) fn seen(&mut self, reply: Option<&str>, document: Option<&str>) {
        self.unseen.retain(|item| match item {
            Arrived::Reply(id) => Some(id.as_str()) != reply,
            Arrived::Document(id) | Arrived::Review(id) => Some(id.as_str()) != document,
        });
        if let Some(document) = document {
            self.reviews_quiet.remove(document);
            if self
                .review_ask
                .as_ref()
                .is_some_and(|ask| ask.document == document)
            {
                self.review_ask = None;
            }
        }
    }

    /// Document `id` opens for review: its Review panel opens once its tab
    /// has loaded ([`Self::take_review`]).
    pub(super) fn review_on_load(&mut self, id: &str) {
        self.to_review.insert(id.to_string());
    }

    /// `true` once for a document opened for review.
    pub(super) fn take_review(&mut self, id: &str) -> bool {
        self.to_review.remove(id)
    }

    /// Sharing stopped: review requests are moot, deliveries stay.
    pub(super) fn sharing_stopped(&mut self) {
        self.unseen
            .retain(|item| !matches!(item, Arrived::Review(_)));
        self.reviews_quiet.clear();
        self.review_ask = None;
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

    #[test]
    fn tags_hold_ids_and_read_back() {
        for item in [
            Arrived::Reply("r1".into()),
            Arrived::Document("d1".into()),
            Arrived::Review("d2".into()),
        ] {
            assert_eq!(Arrived::from_tag(&item.tag()), Some(item));
        }
        assert_eq!(Arrived::Reply("r1".into()).tag(), "ai-reply-r1");
        assert_eq!(Arrived::from_tag("ai-reply-"), None);
        assert_eq!(Arrived::from_tag("thread-1"), None);
    }

    #[test]
    fn what_arrived_waits_until_it_is_opened() {
        let now = Instant::now();
        let mut ai = AiProject::default();
        assert!(ai.arrived(Arrived::Reply("r".into()), now));
        assert!(ai.arrived(Arrived::Document("d".into()), now));
        assert!(ai.arrived(Arrived::Review("x".into()), now));
        // The same again moves to the newest; a review request keeps quiet.
        assert!(ai.arrived(Arrived::Reply("r".into()), now));
        assert!(!ai.arrived(Arrived::Review("x".into()), now));
        assert_eq!(
            ai.unseen(),
            [
                Arrived::Document("d".into()),
                Arrived::Reply("r".into()),
                Arrived::Review("x".into()),
            ]
        );
        ai.seen(Some("r"), None);
        ai.seen(None, Some("d"));
        assert_eq!(ai.unseen(), [Arrived::Review("x".into())]);
        // Once quiet is over, or once opened, it notifies again.
        assert!(ai.arrived(Arrived::Review("x".into()), now + REVIEW_QUIET));
        assert!(!ai.arrived(Arrived::Review("x".into()), now + REVIEW_QUIET));
        ai.seen(None, Some("x"));
        assert!(ai.unseen().is_empty());
        assert!(ai.arrived(Arrived::Review("x".into()), now + REVIEW_QUIET));
        // Stopping the share drops review requests, never deliveries.
        ai.arrived(Arrived::Reply("r".into()), now);
        ai.sharing_stopped();
        assert_eq!(ai.unseen(), [Arrived::Reply("r".into())]);
        assert!(ai.arrived(Arrived::Review("x".into()), now));
        ai.stopped();
        assert!(ai.unseen().is_empty());
    }

    /// The companion's latest reply stays until another arrives; its
    /// review request goes once opened or moot.
    #[test]
    fn the_companion_keeps_the_latest_reply_and_review_request() {
        let mut ai = AiProject::default();
        assert!(ai.latest_reply().is_none() && ai.review_ask().is_none());
        ai.delivered("r1".into(), "claude-ai".into(), false);
        ai.delivered("r2".into(), "chatgpt".into(), false);
        assert_eq!(ai.latest_reply().unwrap().reply, "r2");
        ai.reply_saved("r1");
        assert!(!ai.latest_reply().unwrap().saved, "not the latest");
        // Saved only once the promotion succeeds; a failure can retry.
        ai.reply_saving("r2");
        assert!(ai.latest_reply().unwrap().saving);
        ai.reply_not_saved("r2");
        let latest = ai.latest_reply().unwrap();
        assert!(!latest.saved && !latest.saving && latest.failed);
        ai.reply_saving("r2");
        ai.reply_saved("r2");
        let latest = ai.latest_reply().unwrap();
        assert!(latest.saved && !latest.saving && !latest.failed);
        ai.reply_not_saved("r2");
        assert!(
            ai.latest_reply().unwrap().saved,
            "a later end changes nothing"
        );
        // A new delivery can be saved again; one delivered as a Document
        // is saved already.
        ai.delivered("r3".into(), "claude-ai".into(), false);
        assert!(!ai.latest_reply().unwrap().saved);
        ai.delivered("r5".into(), "claude-ai".into(), true);
        assert!(ai.latest_reply().unwrap().saved);

        ai.review_asked("d".into(), "Document 1".into(), "claude-ai".into());
        assert_eq!(ai.review_ask().unwrap().label, "Document 1");
        ai.seen(None, Some("other"));
        assert!(ai.review_ask().is_some());
        ai.seen(None, Some("d"));
        assert!(ai.review_ask().is_none());
        ai.review_asked("d".into(), "Document 1".into(), "claude-ai".into());
        ai.sharing_stopped();
        assert!(ai.review_ask().is_none());
        ai.review_asked("d".into(), "Document 1".into(), "claude-ai".into());
        ai.shared();
        assert!(ai.review_ask().is_none(), "another share makes it moot");
        // Deliveries outlive a stop of the share, not of the connector.
        ai.delivered("r4".into(), "claude-ai".into(), false);
        ai.sharing_stopped();
        assert!(ai.latest_reply().is_some());
        ai.stopped();
        assert!(ai.latest_reply().is_none());
    }

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
