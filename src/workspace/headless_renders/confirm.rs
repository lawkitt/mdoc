//! The shared confirm component: closing a tab and removing a model. Each
//! is cancelled.
use super::{Driver, Result};
use gpui::PromptLevel;
use mdoc_store::settings;

pub(super) fn scenes(d: &mut Driver) -> Result<()> {
    d.change(|w, window, cx| w.close_tab(w.tabs.active(), window, cx))?;
    d.capture("confirm-keep-mapping")?;
    d.key("escape")?;
    d.change(|w, window, cx| {
        w.case_coordinator.note_asked(w.tabs.active());
        w.close_tab(w.tabs.active(), window, cx);
    })?;
    d.capture("confirm-unsaved")?;
    d.key("escape")?;
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
