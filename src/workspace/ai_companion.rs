//! What the companion window ([`crate::companion`], ADR 0128; Milestone 2
//! item 14) reads from the window, and what its controls ask of it: the AI
//! project and Stop sharing, its documents by AI label and state, the
//! latest reply restored, the latest review request, the shelf's rows and
//! the drop target.
//!
//! The companion floats over other apps, so it reads only what the window
//! shows already: a document of the AI project by its AI label (as a
//! notification names it), never by its stored title; the shelf's rows as
//! the shelf shows them. The reply is restored here, in memory, each time
//! the companion reads; nothing restored is kept.
use super::{Workspace, ai_project::Arrived, autosave::Write, document_writer::Target};
use crate::{ai_connector, shelf::ShelfDocument};
use gpui::{App, Context, Window};
use mdoc_pii::restore::RestoreMap;

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
    /// Save as document ran on it.
    pub(crate) saved: bool,
}

/// The latest request for a document that needs review.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct CompanionReview {
    pub(crate) document: String,
    pub(crate) from: String,
    pub(crate) label: String,
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
        // A request for a document that is Ready since, or not listed now,
        // is moot.
        let review = (self.ai.review_ask())
            .filter(|ask| {
                (documents.as_ref().ok().into_iter().flatten())
                    .any(|document| document.id == ask.document && !document.ready)
            })
            .map(|ask| CompanionReview {
                document: ask.document.clone(),
                from: ai_connector::display_name(&ask.app),
                label: ask.label.clone(),
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

    /// The latest delivered reply, restored now; `None` once deleted.
    fn companion_reply(&self) -> Option<CompanionReply> {
        let latest = self.ai.latest_reply()?;
        Some(CompanionReply {
            id: latest.reply.clone(),
            from: ai_connector::display_name(&latest.app),
            restored: self.companion_reply_text(&latest.reply)?,
            saved: latest.saved,
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
        if latest.is_some_and(|latest| latest.saved) {
            return;
        }
        self.ai.reply_saved(id);
        self.next_op += 1;
        let target = Target::Op(self.next_op);
        self.queue_write(target, Write::Promote(id.to_string()), window, cx);
        crate::menu_bar::companion_changed(cx);
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

    /// The companion's Review in mdoc on a review request: mdoc comes
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
