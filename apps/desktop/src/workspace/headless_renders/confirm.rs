//! The shared confirm component: removing a model, cancelled.
use super::{Driver, Result};
use gpui::PromptLevel;
use mdoc_store::settings;

pub(super) fn scenes(d: &mut Driver) -> Result<()> {
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
