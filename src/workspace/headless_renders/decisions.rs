//! What you did: the decision history and travelling back (ADR 0079).
use super::{Driver, Result};
use crate::{
    document_view::DocumentView,
    pii::ui::{APPLIED_ID, KeepThis, PiiClosePopup},
};
use gpui::Entity;

/// Undo one replacement, then keep the proposal, as two explicit decisions.
pub(super) fn scenes(d: &mut Driver, view: &Entity<DocumentView>) -> Result<()> {
    d.change(|_, window, cx| {
        view.update(cx, |v, cx| {
            let id = v.pii.review().applied()[0].id;
            v.activate_annotation(APPLIED_ID | id, window, cx);
        })
    })?;
    d.settle()?;
    d.change(|_, window, cx| window.dispatch_action(Box::new(KeepThis), cx))?;
    d.settle()?;
    d.change(|_, window, cx| window.dispatch_action(Box::new(KeepThis), cx))?;
    d.settle()?;
    let last = d.change(|_, _, cx| {
        let v = view.read(cx);
        v.pii
            .decisions()
            .rows(&v.editor.read(cx).history_path())
            .count()
            - 1
    })?;
    d.change(|_, window, cx| {
        view.update(cx, |v, cx| {
            v.close_pii_popup(&PiiClosePopup, window, cx);
            v.travel_to_decision(last - 1, window, cx);
            v.open_decision_history(window, cx);
        })
    })?;
    d.capture("decision-history")?;
    d.change(|_, window, cx| {
        let menu = view.read(cx).history.menu.clone();
        window.focus(&menu, cx);
    })?;
    d.settle()?;
    d.key("tab")?;
    let row = d.change(|_, window, cx| window.focused(cx).expect("first decision"))?;
    d.focus_pair("decision-row", row)?;
    d.change(|_, window, cx| {
        view.update(cx, |v, cx| {
            v.travel_to_decision(last, window, cx);
            v.close_decision_history(window, cx);
        })
    })?;
    d.settle()?;
    Ok(())
}
