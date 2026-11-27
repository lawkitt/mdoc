//! Keeping a tab in step with its Project: an undo that brings back an alias
//! the Project retired lines the tab up with it again, and alias or category
//! changes made in another member tab arrive as an edit in this tab's own
//! history.
use crate::{document::DocumentView, document::markdown_search, ui};
use gpui::{AnyElement, AppContext, Context, Focusable, div, prelude::*, px};
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
            return Err("The document changed. Try again.".into());
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
            self.pii.error = Some(format!("Could not follow a change to the Project: {error}"));
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
}

impl DocumentView {
    /// Project names this document doesn't mention, folded under the panel
    /// footer, with a field to add a name to the Project.
    pub(crate) fn project_names_section(&self, cx: &mut Context<Self>) -> Option<AnyElement> {
        let link = self.project.link()?;
        let theme = self.theme.get();
        let palette = theme.pdf_style();
        let absent: Vec<_> = self
            .project
            .names()
            .iter()
            .filter(|c| !self.project_alias_is_present(&c.alias, cx))
            .collect();
        let open = self.project_names_open;
        Some(
            div()
                .flex()
                .flex_col()
                .gap_1()
                .on_action(cx.listener(
                    |this, _: &crate::workspace::ConfirmProjectName, window, cx| {
                        let Some(input) = this.project_term.clone() else {
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
                    },
                ))
                .on_action(cx.listener(
                    |this, _: &crate::workspace::CancelProjectName, window, cx| {
                        if let Some(input) = &this.project_term {
                            input.update(cx, |input, cx| input.reset(cx));
                        }
                        window.focus(&this.editor.read(cx).focus_handle(cx), cx);
                    },
                ))
                .child(
                    ui::control(
                        "project-names-toggle",
                        if absent.is_empty() {
                            // Nothing to list: keep only the way to add a name.
                            format!(
                                "{} Add a name to “{}”",
                                if open { "▾" } else { "▸" },
                                link.name
                            )
                        } else {
                            format!(
                                "{} In “{}”, not in this document ({})",
                                if open { "▾" } else { "▸" },
                                link.name,
                                absent.len()
                            )
                        },
                        theme,
                        true,
                    )
                    .when(cfg!(test), |v| {
                        v.debug_selector(|| "project-names-toggle".into())
                    })
                    .text_size(px(12.))
                    .text_color(palette.header_muted)
                    .on_click(cx.listener(|this, _, window, cx| {
                        this.project_names_open = !this.project_names_open;
                        if this.project_names_open && this.project_term.is_none() {
                            this.project_term = Some(cx.new(|cx| {
                                markdown_search::SearchInput::new(cx)
                                    .with_theme(this.theme.clone())
                                    .with_key_context("ProjectName")
                                    .with_placeholder("Add a name to the Project…")
                            }));
                        }
                        if let Some(input) = this
                            .project_term
                            .clone()
                            .filter(|_| this.project_names_open)
                        {
                            window.focus(&input.read(cx).focus_handle(cx), cx);
                        }
                        cx.notify();
                    })),
                )
                .when(open, |v| {
                    v.when(!absent.is_empty(), |v| {
                        v.child(
                            div()
                                .id("project-names")
                                .when(cfg!(test), |v| v.debug_selector(|| "project-names".into()))
                                .max_h(px(160.))
                                .overflow_y_scroll()
                                .flex()
                                .flex_col()
                                .children(absent.iter().map(|c| {
                                    div()
                                        .flex()
                                        .gap_2()
                                        .px_2()
                                        .text_size(px(12.))
                                        .child(
                                            div()
                                                .flex_1()
                                                .min_w_0()
                                                .truncate()
                                                .child(c.wording().to_string()),
                                        )
                                        .child(
                                            div()
                                                .flex_shrink_0()
                                                .text_color(theme.applied())
                                                .child(c.alias.clone()),
                                        )
                                })),
                        )
                    })
                    .children(self.project_term.clone().map(|input| {
                        div()
                            .mx_2()
                            .px_1()
                            .rounded_sm()
                            .border_1()
                            .border_color(palette.border)
                            .child(input)
                    }))
                })
                .into_any_element(),
        )
    }
}
