//! Keeping a tab in step with its case (ADR 0041, phase 3): joining aligns its
//! aliases with the case in one undo step, and alias or category changes made
//! in another member tab arrive as an edit in this tab's own history.
use crate::{
    document_view::DocumentView,
    markdown_search,
    pii::{self, Category},
    ui,
};
use gpui::{AnyElement, AppContext, Context, Focusable, div, prelude::*, px};
use pii::restore::{ExportedIdentity, plan_join};

/// What joining a case changed in this tab, for the notice.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub(crate) struct JoinSummary {
    pub renumbered: usize,
    pub proposed: usize,
}

impl DocumentView {
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
    /// Line this tab up with `case`: names the case knows take its aliases,
    /// colliding ones the next free number, and the case's other names are
    /// proposed wherever they occur.
    pub(crate) fn join_case_identities(
        &mut self,
        case: &[ExportedIdentity],
        cx: &mut Context<Self>,
    ) -> Result<JoinSummary, String> {
        let document = self.pii.review.export_identities_with_ids();
        let plan = plan_join(case, &document);
        self.case_names = case.to_vec();
        self.retag_identities(&plan.changes, cx)?;
        let before = self.pii.review.remaining();
        if !plan.unmatched.is_empty() {
            let text = self.editor.read(cx).text().to_owned();
            self.pii.review.seed(&plan.unmatched, &text);
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
    /// may use to the identity it became; case names this tab lacks are added.
    pub(crate) fn sync_case_identities(
        &mut self,
        renames: &[(String, ExportedIdentity)],
        case: &[ExportedIdentity],
        cx: &mut Context<Self>,
    ) {
        self.case_names = case.to_vec();
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
            self.pii.error = Some(format!("Could not follow a change to the case: {error}"));
        }
        let missing: Vec<_> = case
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
    /// Case names this document doesn't mention, folded under the panel
    /// footer, with a field to add a name to the case (ADR 0041 #11, #24).
    pub(crate) fn case_names_section(&self, cx: &mut Context<Self>) -> Option<AnyElement> {
        let link = self.case.as_ref()?;
        let theme = self.theme.get();
        let palette = theme.pdf_style();
        let absent: Vec<_> = self
            .case_names
            .iter()
            .filter(|c| !self.case_alias_is_present(&c.alias, cx))
            .collect();
        let open = self.case_names_open;
        Some(
            div()
                .flex()
                .flex_col()
                .gap_1()
                .on_action(cx.listener(
                    |this, _: &crate::workspace::ConfirmCaseName, window, cx| {
                        let Some(input) = this.case_term.clone() else {
                            return;
                        };
                        let term = input.read(cx).value().trim().to_string();
                        if !term.is_empty() {
                            cx.emit(crate::workspace::TabEvent::AddCaseTerm(term));
                        }
                        input.update(cx, |input, cx| input.reset(cx));
                        window.focus(&input.read(cx).focus_handle(cx), cx);
                    },
                ))
                .on_action(
                    cx.listener(|this, _: &crate::workspace::CancelCaseName, window, cx| {
                        if let Some(input) = &this.case_term {
                            input.update(cx, |input, cx| input.reset(cx));
                        }
                        window.focus(&this.editor.read(cx).focus_handle(cx), cx);
                    }),
                )
                .child(
                    ui::control(
                        "case-names-toggle",
                        if absent.is_empty() {
                            // Nothing to list: keep only the way to add a name (ADR 0046).
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
                        v.debug_selector(|| "case-names-toggle".into())
                    })
                    .text_size(px(12.))
                    .text_color(palette.header_muted)
                    .on_click(cx.listener(|this, _, window, cx| {
                        this.case_names_open = !this.case_names_open;
                        if this.case_names_open && this.case_term.is_none() {
                            this.case_term = Some(cx.new(|cx| {
                                markdown_search::SearchInput::new(cx)
                                    .with_key_context("CaseName")
                                    .with_placeholder("Add a name to the case…")
                            }));
                        }
                        if let Some(input) = this.case_term.clone().filter(|_| this.case_names_open)
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
                                .id("case-names")
                                .when(cfg!(test), |v| v.debug_selector(|| "case-names".into()))
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
                    .children(self.case_term.clone().map(|input| {
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
