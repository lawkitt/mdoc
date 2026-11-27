//! What the companion window ([`crate::ai::companion`], ADR 0128; Milestone 2
//! item 14) reads from the window, and what its controls ask of it: the AI
//! project and Stop sharing, its documents by AI label and state, the
//! latest reply restored, the latest review request with its proposals and
//! Replace all (item 16), the shelf's rows and the drop target.
//!
//! The companion floats over other apps, so it reads only what the window
//! shows already: a document of the AI project by its AI label (as a
//! notification names it), never by its stored title; the shelf's rows as
//! the shelf shows them. The reply is restored here, in memory, each time
//! the companion reads; nothing restored is kept.
//!
//! Its review is the window's own: the proposals are the document's
//! review as its tab holds it, or as the Vault saved it while no tab has
//! it, and Replace all runs that tab's Replace all, opening the tab
//! behind the others if need be; its save decides Ready, as any save
//! does. Whatever else is left is decided in mdoc.
use super::{
    Workspace,
    ai_project::{Arrived, Replace},
    autosave::Write,
    document_writer::Target,
};
use crate::{document_view::DocumentView, shelf::ShelfDocument};
use gpui::{App, Context, Window};
use mdoc_pii::{Review, restore::RestoreMap};
use mdoc_store::document_id::{DocumentId, VaultId};
use std::time::Duration;

/// How often the companion's Replace all looks at its tab again, and how
/// many times at most: the tab loads, settles, replaces and saves.
const REPLACE_POLL: Duration = Duration::from_millis(100);
const REPLACE_POLLS: usize = 150;

/// Everything the companion shows, read when it opens and again when what
/// it shows changes.
#[derive(Clone, Debug)]
pub(crate) struct CompanionContent {
    /// The connector runs: the preview is on and the Vault open.
    pub(crate) running: bool,
    /// The AI project, while one is shared.
    pub(crate) shared: Option<CompanionProject>,
    /// Its documents, oldest first, or why they could not be listed.
    pub(crate) documents: Result<Vec<CompanionDocument>, String>,
    pub(crate) reply: Option<CompanionReply>,
    pub(crate) review: Option<CompanionReview>,
    /// The shelf's rows, as the shelf would list them.
    pub(crate) rows: Vec<ShelfDocument>,
    /// "Drag a preview into any app" shows until a drag has dropped.
    pub(crate) drag_hint: bool,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct CompanionProject {
    pub(crate) id: String,
    pub(crate) name: String,
    pub(crate) hue: u32,
}

/// A document of the AI project, as AI apps are told it.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct CompanionDocument {
    pub(crate) id: String,
    /// Its AI label: never the stored title.
    pub(crate) label: String,
    pub(crate) ready: bool,
    /// "Ready", or why it needs review.
    pub(crate) state: String,
}

/// The latest delivered reply, restored.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct CompanionReply {
    pub(crate) id: String,
    /// The AI app, by its product's name.
    pub(crate) from: String,
    /// Its text with the names restored, as the Reply page shows it.
    pub(crate) restored: String,
    /// It is a Document already: saved from here, or delivered as one.
    pub(crate) saved: bool,
    /// Save as document is on its way.
    pub(crate) saving: bool,
    /// The last Save as document failed; it may run again.
    pub(crate) failed: bool,
}

/// The latest request for a document that needs review.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct CompanionReview {
    pub(crate) document: String,
    pub(crate) from: String,
    pub(crate) label: String,
    pub(crate) step: ReviewStep,
}

/// What the companion can do with a requested document now.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) enum ReviewStep {
    /// Its pending proposals: Replace all replaces every one.
    Proposals(Vec<CompanionProposal>),
    /// Replace all runs in its tab; these are what it replaces.
    Replacing(Vec<CompanionProposal>),
    /// Replace all made it Ready: the AI app can read it now.
    Ready,
    /// Only the window can go on: its state in words.
    InMdoc(String),
}

/// One wording a review proposes to replace, as the review shows it.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct CompanionProposal {
    /// The text as written: a real name.
    pub(crate) original: String,
    /// What Replace all writes in its place.
    pub(crate) alias: String,
    /// Its pending mentions.
    pub(crate) mentions: usize,
}

/// `review`'s pending proposals, by wording and alias in the order they
/// first appear, each with what Replace all writes for it.
pub(crate) fn proposals_of(review: &Review) -> Vec<CompanionProposal> {
    let mut proposals: Vec<CompanionProposal> = Vec::new();
    for candidate in review.candidates() {
        let Some(variant) = review.variant(candidate.variant) else {
            continue;
        };
        let alias = (review.occurrence_identity(candidate.variant, &candidate.range))
            .and_then(|id| review.identity(id))
            .map(|identity| identity.alias.clone());
        let Some(alias) = alias else {
            continue;
        };
        let original = variant.original.to_string();
        match (proposals.iter_mut()).find(|p| p.original == original && p.alias == alias) {
            Some(proposal) => proposal.mentions += 1,
            None => proposals.push(CompanionProposal {
                original,
                alias,
                mentions: 1,
            }),
        }
    }
    proposals
}

/// What Replace all may replace in `review`: its proposals, or `None`
/// when it would also rewrite what no proposal shows (an applied alias
/// corrected since), which only the window can show.
pub(crate) fn replaceable(review: &Review) -> Option<Vec<CompanionProposal>> {
    let corrected = review.applied().iter().any(|applied| {
        (review.identity(applied.step.identity))
            .is_some_and(|identity| applied.step.after.as_ref() != identity.alias)
    });
    (!corrected).then(|| proposals_of(review))
}

/// What the companion last saw of the requested document's tab: its
/// proposals left, and whether its review has settled. A change reads the
/// window again.
pub(crate) type ReviewMark = Option<(usize, bool)>;

/// Where a look at the tab left the companion's Replace all.
enum Look {
    Again,
    Done,
}

/// A listed document's state in words.
fn state(state: mdoc_connector::State) -> (bool, String) {
    use crate::shelf::Readiness as Row;
    use mdoc_connector::Readiness;
    match state {
        mdoc_connector::State::Ready => (true, "Ready".into()),
        mdoc_connector::State::NeedsReview(readiness) => {
            let row = match readiness {
                Readiness::NamesNotReplaced(n) => Row::NamesNotReplaced(n),
                Readiness::ChangedSinceReview => Row::ChangedSinceReview,
                Readiness::Ready | Readiness::NotReviewed => Row::NotReviewed,
            };
            (false, format!("Needs review · {}", row.label()))
        }
    }
}

impl Workspace {
    /// What the companion shows now.
    pub(crate) fn companion_content(&self, cx: &App) -> CompanionContent {
        let running = self.ai_connector.as_ref();
        let shared = self
            .ai_shared()
            .and_then(|id| self.projects.project(&id))
            .map(|project| CompanionProject {
                id: project.id.clone(),
                name: project.name.clone(),
                hue: super::ProjectLink::of(project).hue(),
            });
        let documents = match (running, &shared) {
            (Some(running), Some(_)) => running
                .connector
                .list_documents()
                .map(|listed| {
                    (listed.into_iter())
                        .map(|listed| {
                            let (ready, state) = state(listed.state);
                            CompanionDocument {
                                id: listed.doc_id,
                                label: listed.label,
                                ready,
                                state,
                            }
                        })
                        .collect()
                })
                .map_err(|error| error.to_string()),
            _ => Ok(Vec::new()),
        };
        // A request for a document not listed now is moot, and so is one
        // Ready since, unless the companion's Replace all made it so.
        let review = self.ai.review_ask().and_then(|ask| {
            let listed = (documents.as_ref().ok().into_iter().flatten())
                .find(|document| document.id == ask.document)?;
            let replaceable = || self.companion_proposals(&ask.document, cx);
            let step = match (self.ai.replace_of(&ask.document), listed.ready) {
                (Some(Replace::Running(_)), _) => {
                    ReviewStep::Replacing(replaceable().unwrap_or_default())
                }
                (Some(Replace::Ran), true) => ReviewStep::Ready,
                (None, true) => return None,
                (Some(Replace::Ran), false) => ReviewStep::InMdoc(listed.state.clone()),
                (None, false) => match replaceable() {
                    Some(proposals) if !proposals.is_empty() => ReviewStep::Proposals(proposals),
                    _ => ReviewStep::InMdoc(listed.state.clone()),
                },
            };
            Some(CompanionReview {
                document: ask.document.clone(),
                from: crate::ai::connector::display_name(&ask.app),
                label: ask.label.clone(),
                step,
            })
        });
        CompanionContent {
            running: running.is_some(),
            shared,
            documents,
            reply: self.companion_reply(),
            review,
            rows: self.shelf_documents(cx),
            drag_hint: !self.shelf_drag_hint_shown(),
        }
    }

    /// What Replace all may replace in document `document`
    /// ([`replaceable`]): as the tab that has it loaded holds its review,
    /// else as its review was saved.
    fn companion_proposals(&self, document: &str, cx: &App) -> Option<Vec<CompanionProposal>> {
        let tab = (self.companion_tab(document, cx)).and_then(|tab| self.tab_view(tab));
        if let Some(view) = tab.filter(|view| view.read(cx).lifecycle.ready()) {
            return replaceable(view.read(cx).pii.review());
        }
        let stored = (self.vault.vault()).and_then(|vault| vault.document(document).ok());
        let Some(saved) = stored.as_ref().and_then(|stored| stored.review.clone()) else {
            return Some(Vec::new());
        };
        // A review that no longer fits the text proposes nothing.
        match Review::restore(saved, &stored?.markdown, 0) {
            Ok(review) => replaceable(&review),
            Err(_) => Some(Vec::new()),
        }
    }

    /// The tab that has Vault document `document`, if one does.
    pub(super) fn companion_tab(&self, document: &str, cx: &App) -> Option<u64> {
        let id = DocumentId::Vault(VaultId::parse(document)?);
        self.find_document(&id, None, cx)
    }

    /// What the window shares, the document it was last asked to review,
    /// and that document's review in its tab: the companion reads the
    /// window again when any changes.
    pub(crate) fn companion_marks(&self, cx: &App) -> (Option<String>, Option<String>, ReviewMark) {
        let mark = self.companion_review_view(cx).map(|view| {
            let view = view.read(cx);
            (view.pii.review().remaining(), view.review_settled())
        });
        (
            self.ai_shared(),
            self.ai.review_ask().map(|ask| ask.document.clone()),
            mark,
        )
    }

    /// The tab view of the document last asked to review, if a tab has
    /// it: the companion watches its review.
    pub(crate) fn companion_review_view(&self, cx: &App) -> Option<gpui::Entity<DocumentView>> {
        let ask = self.ai.review_ask()?;
        self.tab_view(self.companion_tab(&ask.document, cx)?)
    }

    /// The latest delivered reply, restored now; `None` once deleted.
    fn companion_reply(&self) -> Option<CompanionReply> {
        let latest = self.ai.latest_reply()?;
        Some(CompanionReply {
            id: latest.reply.clone(),
            from: crate::ai::connector::display_name(&latest.app),
            restored: self.companion_reply_text(&latest.reply)?,
            saved: latest.saved,
            saving: latest.saving,
            failed: latest.failed,
        })
    }

    /// Reply `id` restored as the Reply page restores it, read from the
    /// Vault now: what the companion's Copy puts on the clipboard.
    pub(crate) fn companion_reply_text(&self, id: &str) -> Option<String> {
        let reply = self.vault.vault()?.reply(id).ok()?;
        let map = (self.reply_restore_map(&reply))
            // Its owner is gone: nothing restores.
            .unwrap_or_else(|_| RestoreMap::new(Vec::new()));
        Some(map.restore_in_place(&reply.markdown).text)
    }

    /// The companion's Stop sharing.
    pub(crate) fn companion_stop_sharing(&mut self, cx: &mut Context<Self>) {
        self.stop_sharing_with_ai(cx);
    }

    /// The companion's Save as document on reply `id`: Promote to Document,
    /// once; the new document opens in a tab, and mdoc stays where it is.
    /// The reply counts as saved only once the promotion succeeds.
    pub(crate) fn companion_save_reply(
        &mut self,
        id: &str,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let latest = self.ai.latest_reply();
        if self.vault.vault().is_none() || latest.is_none_or(|latest| latest.reply != id) {
            return;
        }
        if latest.is_some_and(|latest| latest.saved || latest.saving) {
            return;
        }
        self.ai.reply_saving(id);
        self.next_op += 1;
        let target = Target::Op(self.next_op);
        self.queue_write(target, Write::Promote(id.to_string()), window, cx);
        crate::shelf::menu_bar::companion_changed(cx);
        cx.notify();
    }

    /// Files dropped on the companion: imported into the AI project, or
    /// opened as a drop on the sidebar opens them while nothing is shared.
    /// mdoc comes forward, as for any drop: an import may ask something.
    pub(crate) fn companion_drop(
        &mut self,
        paths: &gpui::ExternalPaths,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        match self.ai_shared() {
            Some(project) => self.drop_on_project(paths, &project, window, cx),
            None => self.drop_files(paths, window, cx),
        }
    }

    /// The companion's Open in mdoc on a review request: mdoc comes
    /// forward with the document open for review, as the notification's
    /// click opens it.
    pub(crate) fn companion_review(
        &mut self,
        document: &str,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.notification_clicked(&Arrived::Review(document.to_string()), window, cx);
    }

    /// The companion's Replace all on the requested `document`, whose
    /// card listed `listed`: its tab, opened behind the one in front if
    /// none has it, runs its own Replace all once its review has settled,
    /// then saves, and that save decides Ready. mdoc stays where it is.
    /// When the review would replace anything but `listed` by then,
    /// nothing is replaced and the card shows what it would now. Once
    /// clicked, it runs whatever the AI apps ask meanwhile, until Stop
    /// sharing or another share.
    pub(crate) fn companion_replace_all(
        &mut self,
        document: &str,
        listed: Vec<CompanionProposal>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let asked = (self.ai.review_ask()).is_some_and(|ask| ask.document == document);
        if !self.ai_connector_on() || !asked || listed.is_empty() {
            return;
        }
        let Some(id) = VaultId::parse(document) else {
            return;
        };
        if (self.vault.vault()).is_none_or(|vault| vault.document(document).is_err()) {
            return;
        }
        let tab = match self.companion_tab(document, cx) {
            Some(tab) => tab,
            None => {
                let mut record = mdoc_store::session_store::TabRecord::default();
                record.set_document(DocumentId::Vault(id));
                let tab = self.push(record);
                self.name_from_vault(tab);
                tab
            }
        };
        if !self.ai.replace_started(document, tab) {
            return;
        }
        // Loaded behind the tab in front, which stays; evicted last but
        // one, so Undo there can take the step back.
        if tab != self.tabs.active() {
            self.tabs.behind_active(tab);
        }
        self.initialize(tab, window, cx);
        crate::shelf::menu_bar::companion_changed(cx);
        cx.notify();
        let document = document.to_string();
        cx.spawn_in(window, async move |this, cx| {
            let mut replaced = false;
            for _ in 0..REPLACE_POLLS {
                let look = this.update_in(cx, |this, window, cx| {
                    this.companion_replace_look(tab, &document, &listed, &mut replaced, window, cx)
                });
                match look {
                    Ok(Look::Again) => cx.background_executor().timer(REPLACE_POLL).await,
                    Ok(Look::Done) | Err(_) => return,
                }
            }
            // It never settled: what is left is the window's.
            let _ = this.update(cx, |this, cx| {
                this.companion_replace_ended(&document, true, cx)
            });
        })
        .detach();
    }

    /// One look at tab `tab` for the companion's Replace all.
    fn companion_replace_look(
        &mut self,
        tab: u64,
        document: &str,
        listed: &[CompanionProposal],
        replaced: &mut bool,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Look {
        // It ended (Stop sharing, another share), the document left the
        // AI project, or the tab closed or holds another document now:
        // nothing more runs.
        let running = self.ai.replace_of(document) == Some(Replace::Running(tab));
        let shared = self
            .ai_shared()
            .is_some_and(|shared| self.project_of(document).as_deref() == Some(shared.as_str()));
        let same = self.companion_tab(document, cx) == Some(tab);
        let view = (self.tab_view(tab)).filter(|_| running && shared && same);
        let Some(view) = view else {
            self.companion_replace_ended(document, *replaced, cx);
            return Look::Done;
        };
        if !*replaced {
            if view.read(cx).lifecycle.unavailable() {
                self.companion_replace_ended(document, true, cx);
                return Look::Done;
            }
            if !view.read(cx).review_settled() {
                return Look::Again;
            }
            match replaceable(view.read(cx).pii.review()) {
                Some(now) if now == listed => {}
                // A correction no proposal shows: the window's to decide.
                None => {
                    self.companion_replace_ended(document, true, cx);
                    return Look::Done;
                }
                Some(_) => {
                    self.companion_replace_ended(document, false, cx);
                    return Look::Done;
                }
            }
            if !view.update(cx, |view, cx| view.replace_all_from_companion(window, cx)) {
                self.companion_replace_ended(document, true, cx);
                return Look::Done;
            }
            *replaced = true;
            self.autosave(tab, window, cx);
            return Look::Again;
        }
        if self.write_pending_for(tab, cx) || super::autosave::ready_unwritten(view.read(cx), cx) {
            return Look::Again;
        }
        self.companion_replace_ended(document, true, cx);
        Look::Done
    }

    fn companion_replace_ended(&mut self, document: &str, ran: bool, cx: &mut Context<Self>) {
        self.ai.replace_ended(document, ran);
        crate::shelf::menu_bar::companion_changed(cx);
        cx.notify();
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use mdoc_connector::{Readiness, State};

    #[test]
    fn a_documents_state_reads_as_the_shelf_reads_it() {
        assert_eq!(state(State::Ready), (true, "Ready".into()));
        assert_eq!(
            state(State::NeedsReview(Readiness::NamesNotReplaced(2))),
            (false, "Needs review · 2 names not replaced".into())
        );
        assert_eq!(
            state(State::NeedsReview(Readiness::ChangedSinceReview)),
            (false, "Needs review · Changed since review".into())
        );
        assert_eq!(
            state(State::NeedsReview(Readiness::NotReviewed)),
            (false, "Needs review · Not reviewed".into())
        );
    }
}
