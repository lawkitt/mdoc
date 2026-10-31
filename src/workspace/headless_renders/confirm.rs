//! The shared confirm component (ADR 0064): the close questions for
//! unsaved changes and a mapping, only a mapping, only changes; the Quit
//! summary over several documents; removing a model. Each is cancelled.
use super::{Driver, Result};
use gpui::{Focusable, PromptLevel};
use mdoc_store::settings;

/// Don't Save in a save prompt (ADR 0073).
const DONT_SAVE: &str = if cfg!(target_os = "macos") {
    "cmd-d"
} else {
    "alt-d"
};

/// Types `key` into the active tab's editor.
fn type_in_active(d: &mut Driver, key: &str) -> Result<()> {
    d.change(|w, window, cx| {
        if let Some(view) = w.active_view() {
            let focus = view.read(cx).editor.focus_handle(cx);
            window.focus(&focus, cx);
        }
    })?;
    d.key(key)
}

pub(super) fn scenes(d: &mut Driver) -> Result<()> {
    // The edited, pseudonymized tab is already in a case, so its mapping
    // questions are asked directly (ADR 0071).
    d.change(|w, window, cx| {
        w.ask_changes_and_mapping(w.tabs.active(), false, window, cx);
    })?;
    d.capture("confirm-save-and-mapping")?;
    d.key("escape")?;
    d.change(|w, window, cx| {
        w.ask_mapping(w.tabs.active(), false, window, cx);
    })?;
    d.capture("confirm-keep-mapping")?;
    d.key("escape")?;
    d.change(|w, window, cx| {
        w.case_coordinator.note_asked(w.tabs.active());
        w.close_tab(w.tabs.active(), window, cx);
    })?;
    d.capture("confirm-unsaved")?;
    d.key("escape")?;
    quit_summary(d)?;
    d.cx.update_window(d.window, |_, window, cx| {
        // Render the shared confirm component without starting a removal job.
        drop(window.prompt(
            PromptLevel::Warning,
            &format!(
                "Remove {} model files?",
                settings::Model::Pii(settings::PiiModel::Fp16).name()
            ),
            Some("Shared runtimes and documents are retained."),
            &["Remove", "Cancel"],
            cx,
        ));
    })
    .map_err(|e| e.to_string())?;
    d.capture("confirm-remove-model")?;
    d.key("escape")
}

/// Six edited drafts beside the pseudonymized tab: the summary names five,
/// then "and 2 more". Each draft is closed again with Don't Save.
fn quit_summary(d: &mut Driver) -> Result<()> {
    let mut drafts = Vec::new();
    for _ in 0..6 {
        drafts.push(d.change(|w, window, cx| w.new_tab(window, cx))?);
        type_in_active(d, "x")?;
    }
    d.change(|w, window, cx| w.quit(window, cx))?;
    d.capture("confirm-quit-summary")?;
    d.key("escape")?;
    for id in drafts {
        d.change(|w, window, cx| w.close_tab(id, window, cx))?;
        d.key(DONT_SAVE)?;
    }
    Ok(())
}
