//! The shared confirm component: the close question for a mapping, and
//! removing a model. Each is cancelled.
use super::{Driver, Result};
use gpui::PromptLevel;
use mdoc_store::settings;

pub(super) fn scenes(d: &mut Driver) -> Result<()> {
    // The pseudonymized tab is already in a case, so its mapping question is
    // asked directly.
    d.change(|w, window, cx| {
        w.ask_mapping(w.tabs.active(), false, window, cx);
    })?;
    d.capture("confirm-keep-mapping")?;
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
