//! Saved cases: the case list, the case page (ADR 0077), its locked,
//! rename and categories states, the Keychain explainer and deleting.
use super::{Driver, Result};
use crate::workspace::case_page::Place;
use std::path::Path;

/// Joins the fixture document to the fixture case; returns the case's id.
pub(super) fn scenes(d: &mut Driver, path: &Path) -> Result<String> {
    d.change(|w, window, cx| w.show_case_list(window, cx))?;
    d.capture("open-case")?;
    d.change(|w, window, cx| w.close_case_list(window, cx))?;
    let id = d.change(|w, _, _| w.case_coordinator.cases()[0].id.clone())?;
    d.change(|w, window, cx| {
        w.join_case(w.tabs.active(), &id, window, cx);
        let closed = path.with_file_name("Witness statement.md");
        std::fs::write(&closed, "# Public witness statement\n").expect("temporary fixture");
        let (_, effect) = w
            .case_coordinator
            .edit(&id, |case| {
                case.created = 1791763200;
                case.members
                    .extend([closed, path.with_file_name("Missing exhibit.md")]);
                Some(())
            })
            .unwrap();
        w.carry_out_cases(effect.into_iter().collect(), cx);
        w.open_case(&id, window, cx);
    })?;
    d.capture("case-page")?;
    // Leave only the cached public name/count, exactly as on a locked launch.
    let unlocked = d.change(|w, window, cx| {
        let case = w.case_coordinator.case(&id).unwrap().clone();
        w.case_coordinator
            .restore_summaries(vec![mdoc_store::cases::CaseSummary::of(&case)]);
        w.case_coordinator.hide_for_locked_render();
        w.open_case(&id, window, cx);
        case
    })?;
    d.capture("case-page-locked")?;
    d.change(|w, window, cx| {
        w.case_coordinator.finish_load(vec![unlocked]);
        w.show_home(window, cx);
        w.activate(w.tabs.active(), window, cx);
        w.open_case(&id, window, cx);
    })?;
    d.change(|w, window, cx| w.start_rename_case(id.clone(), window, cx))?;
    d.capture("rename-case")?;
    d.key("escape")?;
    d.change(|w, window, cx| {
        w.show_case_page(&id, Place::Categories, window, cx);
    })?;
    d.capture("case-categories")?;
    d.change(|w, window, cx| w.activate(w.tabs.active(), window, cx))?;
    d.change(|w, _, cx| {
        // Preview only. Continue is never invoked and no OS key is read.
        w.cases.explainer = true;
        w.explainer_focus_pending = true;
        cx.notify();
    })?;
    d.capture("keychain-explainer")?;
    d.change(|w, window, cx| w.cancel_unlock(window, cx))?;
    Ok(id)
}

/// The delete confirmation, cancelled.
pub(super) fn delete(d: &mut Driver, id: String) -> Result<()> {
    d.change(|w, window, cx| w.delete_case(id, window, cx))?;
    d.capture("confirm-delete-case")?;
    d.key("escape")
}

/// Home on a locked launch that remembers no cases (#307): four case files
/// wait for the key behind the bell, and Open a case… says so below its link.
pub(super) fn locked_home(d: &mut Driver) -> Result<()> {
    d.change(|w, window, cx| {
        w.case_coordinator.reopen(None);
        w.case_coordinator.lock();
        w.case_coordinator.hide_for_locked_render();
        w.case_coordinator.restore_summaries(Vec::new());
        w.case_coordinator.count_locked(Some(4));
        w.show_home(window, cx);
        w.title.notices_open.set(true);
        cx.notify();
    })?;
    d.capture("home-locked-notice")?;
    d.change(|w, window, cx| w.close_notices(window, cx))?;
    // Painted Home has placed the link by now.
    d.change(|w, window, cx| {
        let link = w.cases.home_link.get();
        w.show_case_list_at(link, window, cx);
    })?;
    d.capture("home-locked-open-case")?;
    d.change(|w, window, cx| w.close_case_list(window, cx))
}
