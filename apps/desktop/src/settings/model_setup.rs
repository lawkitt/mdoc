//! The models on the Detection and Recognition pages: whether each is
//! installed, and checking, downloading, repairing and removing it.
use super::{Panel, Status};
use crate::app::models;
use mdoc_import::model_download;
use mdoc_store::settings::Model;
use std::sync::atomic::Ordering;

mod actions;
mod view;

#[cfg(test)]
pub(super) use actions::SetupCompletion;

impl Panel {
    pub fn index(model: Model) -> usize {
        Model::ALL.iter().position(|m| *m == model).unwrap()
    }

    /// Whether `model` must be downloaded before use.
    pub fn needs_setup(&self, model: Model) -> bool {
        let index = Self::index(model);
        self.pending[index].total() > 0
            || matches!(self.statuses[index], Status::Missing | Status::Damaged(_))
    }

    /// Live setup state while `model` is downloading or being checked.
    pub fn progress_of(&self, model: Model) -> Option<model_download::State> {
        self.progress
            .as_ref()
            .filter(|_| self.working == Some(model))
            .map(|p| p.state.lock().unwrap().clone())
    }

    pub fn cancelling(&self) -> bool {
        self.progress
            .as_ref()
            .is_some_and(|p| p.cancel.load(Ordering::Relaxed))
    }

    fn model_files(&self) -> [Vec<std::path::PathBuf>; 4] {
        #[cfg(test)]
        if let Some(files) = &self.model_files {
            return files.clone();
        }
        Model::ALL.map(models::files)
    }

    pub(super) fn row_checking(&self, model: Model) -> bool {
        self.checking && matches!(self.row_status(model), Status::Unknown)
    }

    pub(super) fn row_status(&self, model: Model) -> &Status {
        &self.row_statuses.as_ref().unwrap_or(&self.statuses)[Self::index(model)]
    }
}
