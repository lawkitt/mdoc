//! Keeping a tab in step with its Project: an undo that brings back an alias
//! the Project retired lines the tab up with it again, and alias or category
//! changes made in another member tab arrive as an edit in this tab's own
//! history.
mod view;
use super::text;
use crate::{document::DocumentView, document::markdown_search};
use gpui::{AppContext, Context, Focusable, Window};
use mdoc_pii::Category;
use mdoc_pii::restore::{ExportedIdentity, plan_join};

/// What lining a tab up with its Project changed.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub(crate) struct JoinSummary {
    pub renumbered: usize,
    pub proposed: usize,
}

impl DocumentView {
    /// Forget this Project identity without touching a different identity whose
    /// alias collided before it joined. An older scan may not re-seed it.
    pub(crate) fn forget_project_identity(
        &mut self,
        aliases: &[&str],
        originals: &[String],
        identities: &[ExportedIdentity],
        cx: &mut Context<Self>,
    ) {
        self.pii.cancel();
        self.pii.review.forget_project_identity(aliases, originals);
        self.pii.mapping.forget_project_originals(originals);
        self.pii.review.reserve_aliases(
            &aliases
                .iter()
                .map(|alias| alias.to_string())
                .collect::<Vec<_>>(),
        );
        self.project.set_names(identities.to_vec());
        self.review_changed(cx);
        cx.notify();
    }
    /// Retag identities and rewrite their applied aliases as one undo step.
    pub(crate) fn retag_identities(
        &mut self,
        changes: &[(u64, String, Category, bool)],
        cx: &mut Context<Self>,
    ) -> Result<(), String> {
        if changes.is_empty() {
            return Ok(());
        }
        let old = self.pii.review.identity_snapshot();
        self.pii.review.reassign_aliases(changes)?;
        let text = self.editor.read(cx).text().to_owned();
        let plans = match self.pii.review.alias_corrections(&text) {
            Ok(plans) => plans,
            Err(error) => {
                self.pii.review.restore_identity_snapshot(old);
                return Err(error);
            }
        };
        let revision = self.editor.read(cx).revision();
        if self
            .commit_plans(revision, &plans, Some(old.clone()), cx)
            .is_none()
        {
            self.pii.review.restore_identity_snapshot(old);
            return Err(text::DOCUMENT_CHANGED_TRY_AGAIN.into());
        }
        Ok(())
    }
    /// Line this tab up with `Project`: names the Project knows take its aliases,
    /// colliding ones the next free number, and the Project's other names are
    /// proposed wherever they occur.
    pub(crate) fn join_project_identities(
        &mut self,
        identities: &[ExportedIdentity],
        reserved: &[String],
        cx: &mut Context<Self>,
    ) -> Result<JoinSummary, String> {
        let document = self.pii.review.export_identities_with_ids();
        let plan = plan_join(identities, &document, reserved);
        self.pii.review.reserve_aliases(reserved);
        self.project.set_names(identities.to_vec());
        let history = self.editor.read(cx).history_id();
        let old = self.pii.review.identity_snapshot();
        self.retag_identities(&plan.changes, cx)?;
        let before = self.pii.review.remaining();
        if !plan.unmatched.is_empty() {
            let text = self.editor.read(cx).text().to_owned();
            // A join that only proposes still makes its own undo point, so
            // What you did lists it.
            if plan.changes.is_empty() {
                self.checkpoint_review(cx, |_| ());
            }
            let after = self.editor.read(cx).history_id();
            if after == history {
                self.pii.review.seed(&plan.unmatched, &text);
            } else {
                // The proposals belong to the join's step: undoing it
                // withdraws them, and redo brings them back.
                self.pii.review.seed_on_step(&plan.unmatched, &text);
                self.pii
                    .review
                    .commit_identity_snapshot(history, after, old);
            }
        }
        let summary = JoinSummary {
            renumbered: plan.changes.len(),
            proposed: self.pii.review.remaining().saturating_sub(before),
        };
        if summary.proposed > 0 {
            self.pii.reviewing = true;
        }
        self.review_changed(cx);
        cx.notify();
        Ok(summary)
    }
    /// Follow changes another member made: `renames` maps an alias this tab
    /// may use to the identity it became; Project names this tab lacks are added.
    pub(crate) fn sync_project_identities(
        &mut self,
        renames: &[(String, ExportedIdentity)],
        identities: &[ExportedIdentity],
        cx: &mut Context<Self>,
    ) {
        self.project.set_names(identities.to_vec());
        let review = &self.pii.review;
        let changes: Vec<_> = renames
            .iter()
            .filter_map(|(old, new)| {
                let id = review.identity_by_alias(old)?;
                let identity = review.identity(id)?;
                (identity.alias != new.alias || identity.category != new.category)
                    .then(|| (id, new.alias.clone(), new.category, new.custom))
            })
            .collect();
        if let Err(error) = self.retag_identities(&changes, cx) {
            self.pii.error = Some(text::could_not_follow(error));
        }
        let missing: Vec<_> = identities
            .iter()
            .filter(|c| self.pii.review.identity_by_alias(&c.alias).is_none())
            .cloned()
            .collect();
        if !missing.is_empty() {
            let text = self.editor.read(cx).text().to_owned();
            self.pii.review.seed(&missing, &text);
            self.review_changed(cx);
        }
        cx.notify();
    }

    /// Enter in the Project-name field: add its name to the Project and empty it.
    fn confirm_project_name(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let Some(input) = self.project_term.clone() else {
            return;
        };
        let term = input.read(cx).value().trim().to_string();
        if !term.is_empty() {
            cx.emit(crate::workspace::TabEvent::Project(
                crate::workspace::ProjectEvent::AddTerm(term),
            ));
        }
        input.update(cx, |input, cx| input.reset(cx));
        window.focus(&input.read(cx).focus_handle(cx), cx);
    }

    /// Esc in the Project-name field: empty it and return to the text.
    fn cancel_project_name(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if let Some(input) = &self.project_term {
            input.update(cx, |input, cx| input.reset(cx));
        }
        window.focus(&self.editor.read(cx).focus_handle(cx), cx);
    }

    /// The names section's toggle: open it with its field focused, or fold it.
    fn toggle_project_names(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.project_names_open = !self.project_names_open;
        if self.project_names_open && self.project_term.is_none() {
            self.project_term = Some(cx.new(|cx| {
                markdown_search::SearchInput::new(cx)
                    .with_theme(self.theme.clone())
                    .with_key_context("ProjectName")
                    .with_placeholder(text::ADD_A_NAME_TO_THE_PROJECT)
            }));
        }
        if let Some(input) = self
            .project_term
            .clone()
            .filter(|_| self.project_names_open)
        {
            window.focus(&input.read(cx).focus_handle(cx), cx);
        }
        cx.notify();
    }
}
