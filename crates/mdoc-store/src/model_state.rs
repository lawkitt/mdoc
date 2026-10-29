//! Disposable model-check results beside settings. Only model file metadata is
//! read: this cache never opens a model or a document for its contents.
use crate::settings::Model;
use serde::{Deserialize, Serialize};
use std::{
    io::{self, Read, Write},
    path::{Path, PathBuf},
    time::SystemTime,
};

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum Status {
    Unknown,
    Missing,
    Ready,
    Damaged(String),
    Unavailable(String),
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
struct File {
    path: PathBuf,
    metadata: Option<(u64, SystemTime)>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Files(Vec<File>);

impl Files {
    pub fn read(paths: &[PathBuf]) -> Self {
        Self(
            paths
                .iter()
                .map(|path| File {
                    path: path.clone(),
                    metadata: std::fs::metadata(path)
                        .ok()
                        .filter(|m| m.is_file())
                        .and_then(|m| Some((m.len(), m.modified().ok()?))),
                })
                .collect(),
        )
    }

    fn complete(&self) -> bool {
        !self.0.is_empty() && self.0.iter().all(|file| file.metadata.is_some())
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
struct Entry {
    model: Model,
    files: Files,
    status: Status,
}

#[derive(Clone, Default, Serialize, Deserialize)]
pub struct ModelState {
    entries: Vec<Entry>,
    #[serde(skip)]
    path: Option<PathBuf>,
}

impl ModelState {
    pub fn contains(&self, model: Model) -> bool {
        self.entries.iter().any(|entry| entry.model == model)
    }

    pub fn open(settings: Option<&Path>) -> Self {
        Self::read(settings.map(|p| p.with_file_name("model-state.json")))
    }

    /// Call under model admission before recording one model, retaining any
    /// catalog result another window finished since this owner was created.
    pub fn reload(&mut self) {
        if self.path.is_some() {
            *self = Self::read(self.path.clone());
        }
    }

    fn read(path: Option<PathBuf>) -> Self {
        let mut state = path
            .as_ref()
            .and_then(|path| {
                let mut bytes = Vec::new();
                std::fs::File::open(path)
                    .ok()?
                    .take(65_537)
                    .read_to_end(&mut bytes)
                    .ok()?;
                (bytes.len() <= 65_536)
                    .then(|| serde_json::from_slice::<Self>(&bytes).ok())
                    .flatten()
            })
            .unwrap_or_default();
        state.path = path;
        state
    }

    /// None means no previous check. A changed or incomplete installation may
    /// never borrow an old Installed label, even while the model slot is busy.
    pub fn status(&self, model: Model, files: &Files) -> Option<Status> {
        let entry = self.entries.iter().find(|entry| entry.model == model)?;
        Some(
            if entry.files != *files || (entry.status == Status::Ready && !files.complete()) {
                Status::Unknown
            } else {
                entry.status.clone()
            },
        )
    }

    /// Metadata must stay stable across verification. Never attach a verified
    /// result to files replaced while the check was in flight.
    pub fn record(&mut self, model: Model, before: &Files, after: Files, status: Status) -> Status {
        let status = if *before != after || (status == Status::Ready && !after.complete()) {
            Status::Unknown
        } else {
            status
        };
        self.entries.retain(|entry| entry.model != model);
        self.entries.push(Entry {
            model,
            files: after,
            status: status.clone(),
        });
        status
    }

    pub fn save(&self) -> io::Result<()> {
        let Some(path) = &self.path else {
            return Ok(());
        };
        let parent = path
            .parent()
            .ok_or_else(|| io::Error::other("model state has no parent"))?;
        std::fs::create_dir_all(parent)?;
        let bytes = serde_json::to_vec(self).map_err(io::Error::other)?;
        if bytes.len() > 65_536 {
            return Err(io::Error::other("model state is too large"));
        }
        let mut file = tempfile::NamedTempFile::new_in(parent)?;
        file.write_all(&bytes)?;
        file.as_file().sync_all()?;
        file.persist(path).map_err(|e| e.error)?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::settings::OcrModel;
    const MODEL: Model = Model::Ocr(OcrModel::V6Small);

    #[test]
    fn results_survive_reopening_without_reading_file_contents() {
        let dir = tempfile::tempdir().unwrap();
        let settings = dir.path().join("settings.json");
        let model = dir.path().join("model.onnx");
        let content = "private document content must never be stored";
        std::fs::write(&model, content).unwrap();
        let files = Files::read(&[model]);
        let mut state = ModelState::open(Some(&settings));
        assert_eq!(state.status(MODEL, &files), None);
        state.record(MODEL, &files, files.clone(), Status::Ready);
        state.save().unwrap();
        assert_eq!(
            ModelState::open(Some(&settings)).status(MODEL, &files),
            Some(Status::Ready)
        );
        let json = std::fs::read_to_string(dir.path().join("model-state.json")).unwrap();
        assert!(!json.contains(content));
        assert!(json.len() < 2048);
    }

    #[test]
    fn removed_resized_retimed_and_replaced_files_invalidate_installed() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("model");
        std::fs::write(&path, b"model").unwrap();
        let set_modified = |seconds| {
            std::fs::File::options()
                .write(true)
                .open(&path)
                .unwrap()
                .set_modified(SystemTime::UNIX_EPOCH + std::time::Duration::from_secs(seconds))
                .unwrap();
        };
        set_modified(123);
        let paths = [path.clone()];
        let before = Files::read(&paths);
        let mut state = ModelState::default();
        state.record(MODEL, &before, before.clone(), Status::Ready);
        set_modified(456);
        assert_eq!(
            state.status(MODEL, &Files::read(&paths)),
            Some(Status::Unknown)
        );
        std::fs::write(&path, b"resized model").unwrap();
        // Isolate the size signal from modification time.
        set_modified(123);
        assert_eq!(
            state.status(MODEL, &Files::read(&paths)),
            Some(Status::Unknown)
        );
        std::fs::remove_file(&path).unwrap();
        let gone = Files::read(&paths);
        assert_eq!(state.status(MODEL, &gone), Some(Status::Unknown));
        state.record(MODEL, &gone, gone.clone(), Status::Missing);
        assert_eq!(state.status(MODEL, &gone), Some(Status::Missing));
        std::fs::write(&path, b"model").unwrap();
        // Windows can retain a deleted file's timestamps when its name is
        // reused quickly. The replacement must have a known metadata change.
        set_modified(789);
        let after = Files::read(&paths);
        assert_eq!(
            state.record(MODEL, &before, after, Status::Ready),
            Status::Unknown
        );
    }

    #[test]
    fn installed_requires_a_nonempty_complete_file_list_even_in_stored_json() {
        let dir = tempfile::tempdir().unwrap();
        let settings = dir.path().join("settings.json");
        for files in [Files::read(&[]), Files::read(&[dir.path().join("gone")])] {
            let mut state = ModelState::open(Some(&settings));
            // Simulate a malformed or obsolete cache, bypassing record's guard.
            state.entries = vec![Entry {
                model: MODEL,
                files: files.clone(),
                status: Status::Ready,
            }];
            state.save().unwrap();
            assert_eq!(
                ModelState::open(Some(&settings)).status(MODEL, &files),
                Some(Status::Unknown)
            );
            assert_eq!(
                state.record(MODEL, &files, files.clone(), Status::Ready),
                Status::Unknown
            );
        }
    }
}
