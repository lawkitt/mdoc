//! Merge hints in the Replacements panel (ADR 0080): one line under the
//! shorter (or later) identity's header, "Looks like … · Merge ×". Nothing
//! links without the click; Merge is one undo step.
use super::{MappingAction, PanelCursor, panel_rows::MENTION_INSET};
use crate::{document_view::DocumentView, ui};
use gpui::{AnyElement, Context, SharedString, Window, div, prelude::*, px};
use mdoc_pii::merge_hints::MergeHint;
use std::sync::Arc;

impl DocumentView {
    /// Link the hinted identity into the fuller one, which keeps its alias
    /// and primary wording; the other alias retires (ADR 0041).
    pub(super) fn merge_from_hint(
        &mut self,
        hint: &MergeHint,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let review = &self.pii.review;
        if review.identity(hint.from).is_none() || review.identity(hint.into).is_none() {
            return;
        }
        let before = self.editor.read(cx).history_id();
        self.change_mapping(MappingAction::Merge(hint.from, hint.into), cx);
        let after = self.editor.read(cx).history_id();
        if after != before {
            self.pii.mapping.record_merged(hint.wording.clone(), after);
            self.flash_notice(cx);
        }
        self.pii.mapping.cursor = Some(PanelCursor::Header(hint.into));
        window.focus(&self.pii.mapping.focus, cx);
        cx.notify();
    }

    /// × hides the hint for this pair: in the case when the document has one,
    /// otherwise for the tab's life.
    pub(super) fn dismiss_merge_hint(
        &mut self,
        hint: &MergeHint,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if self.pii.mapping.dismiss_hint(hint.pair.clone()) && self.case.link().is_some() {
            cx.emit(crate::workspace::TabEvent::Case(
                crate::workspace::CaseEvent::DismissHint(
                    hint.pair.0.to_string(),
                    hint.pair.1.to_string(),
                ),
            ));
        }
        self.pii.mapping.cursor = Some(PanelCursor::Header(hint.from));
        window.focus(&self.pii.mapping.focus, cx);
        cx.notify();
    }

    /// The hints dismissed in this tab, which a case made from it keeps.
    pub(crate) fn dismissed_merge_hints(&self) -> Vec<(String, String)> {
        self.pii.mapping.dismissed_hint_pairs()
    }

    /// Hints the case's members dismissed stay hidden here.
    pub(crate) fn dismiss_case_hints(
        &mut self,
        pairs: &[(String, String)],
        cx: &mut Context<Self>,
    ) {
        for (from, into) in pairs {
            self.pii
                .mapping
                .dismiss_hint((Arc::from(from.as_str()), Arc::from(into.as_str())));
        }
        cx.notify();
    }

    /// The hint line: muted text, then Merge and × as Tab stops with the one
    /// focus ring (ADR 0074).
    pub(super) fn hint_row(
        &self,
        index: usize,
        hint: Arc<MergeHint>,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let theme = self.theme.get();
        let palette = theme.pdf_style();
        let (from, into) = (hint.from, hint.into);
        let [merge_focus, dismiss_focus] = self
            .pii
            .mapping
            .hint_focus
            .borrow_mut()
            .entry((from, into))
            .or_insert_with(|| [cx.focus_handle(), cx.focus_handle()].map(|f| f.tab_stop(true)))
            .clone();
        let wording = hint.wording.to_string();
        let merge = hint.clone();
        let dismiss = hint;
        div()
            .id(SharedString::from(format!("replacement-entry-{index}")))
            .when(cfg!(test), |v| {
                v.debug_selector(move || format!("merge-hint-{from}"))
            })
            .w_full()
            .h(px(super::panel::ROW_HEIGHT))
            .pl(px(MENTION_INSET))
            .pr_2()
            .flex()
            .items_center()
            .gap_1()
            .text_size(px(12.))
            .text_color(palette.header_muted)
            .child(
                div()
                    .min_w_0()
                    .flex_shrink(1.)
                    .truncate()
                    .child(format!("Looks like {wording}")),
            )
            .child(div().flex_shrink_0().child("·"))
            .child(
                ui::link_button(
                    SharedString::from(format!("merge-hint-merge-{from}-{into}")),
                    "Merge",
                    theme,
                    true,
                )
                .track_focus(&merge_focus)
                .when(cfg!(test), |v| {
                    v.debug_selector(move || format!("merge-hint-merge-{from}"))
                })
                .aria_label(format!("Merge with {wording}"))
                .px_1()
                .text_size(px(12.))
                .on_click(cx.listener(move |this, _, window, cx| {
                    cx.stop_propagation();
                    this.merge_from_hint(&merge, window, cx);
                })),
            )
            .child(
                ui::control(
                    SharedString::from(format!("merge-hint-dismiss-{from}-{into}")),
                    "×",
                    theme,
                    true,
                )
                .track_focus(&dismiss_focus)
                .when(cfg!(test), |v| {
                    v.debug_selector(move || format!("merge-hint-dismiss-{from}"))
                })
                .aria_label("Dismiss")
                .px_1()
                .text_size(px(12.))
                .text_color(palette.header_muted)
                .on_click(cx.listener(move |this, _, window, cx| {
                    cx.stop_propagation();
                    this.dismiss_merge_hint(&dismiss, window, cx);
                })),
            )
            .into_any_element()
    }
}
