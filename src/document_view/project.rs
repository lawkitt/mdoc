//! A tab's link to its Project: mapping changes fold into the Project after
//! a short pause, and a member document loaded without its review gets the
//! Project's mapping back.
use super::{DocumentView, ProjectEffect};
use crate::workspace::{self, ProjectLink};
use gpui::Context;
use mdoc_pii::restore::ExportedIdentity;

impl DocumentView {
    /// Ask the workspace to fold this tab's mapping into its Project, once
    /// changes settle.
    pub(crate) fn schedule_project_save(&mut self, cx: &mut Context<Self>) {
        // Every mapping change passes here, in a Project or not: the AI
        // reply's aliases follow renumbering, and the document saves its
        // review.
        self.follow_reply_aliases_soon(cx);
        self.schedule_autosave(cx);
        let effect = self.project.mapping_changed();
        self.carry_out_project_link(effect, cx);
    }

    pub(crate) fn carry_out_project_link(
        &mut self,
        effect: Option<ProjectEffect>,
        cx: &mut Context<Self>,
    ) {
        match effect {
            None => {}
            Some(ProjectEffect::SaveSoon) => {
                self.project_save = Some(cx.spawn(async move |this, cx| {
                    cx.background_executor()
                        .timer(std::time::Duration::from_millis(700))
                        .await;
                    let _ = this.update(cx, |this, cx| {
                        this.project_save = None;
                        cx.emit(workspace::TabEvent::Project(
                            workspace::ProjectEvent::Changed,
                        ));
                    });
                }));
            }
            Some(ProjectEffect::CancelSave) => self.project_save = None,
            Some(ProjectEffect::Repaint) => cx.notify(),
        }
    }

    /// Whether a mapping change waits to be folded into the Project.
    pub(crate) fn project_save_pending(&self) -> bool {
        self.project_save.is_some()
    }

    /// Give a member document loaded without its review its Project's
    /// identities. Alias tokens in the text become applied replacements
    /// that can be restored.
    pub(crate) fn open_project_mapping(
        &mut self,
        link: ProjectLink,
        identities: &[ExportedIdentity],
        reserved: &[String],
        cx: &mut Context<Self>,
    ) -> usize {
        self.project.attach(Some(link));
        self.project.set_names(identities.to_vec());
        self.open_names(identities, reserved, cx)
    }

    /// Seed the review of a document loaded without one with `identities`:
    /// its Project's, or its own while Unfiled.
    pub(crate) fn open_names(
        &mut self,
        identities: &[ExportedIdentity],
        reserved: &[String],
        cx: &mut Context<Self>,
    ) -> usize {
        let text = self.editor.read(cx).text().to_owned();
        let history = self.editor.read(cx).history_id();
        let review = self.pii.review_mut();
        review.reserve_aliases(reserved);
        let ids = review.seed(identities, &text);
        let linked = review.relink(&text, identities, &ids, history);
        // Saved and restored at once, the review is anchored to this history
        // state, as a reopened document's is, so Not a name and Keep can take
        // its replacements back before any edit.
        match self.saved_review() {
            Some(saved) => self.restore_review(saved, cx),
            None => self.review_changed(cx),
        }
        cx.notify();
        linked
    }
}
