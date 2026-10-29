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
#[serde(from = "StoredEntry", into = "StoredEntry")]
struct Entry {
    model: Model,
    files: Files,
    status: Status,
}

/// Persist only a bounded kind, never an engine's message or a path in it.
/// Legacy `status` fields are ignored; their missing kind defaults to Unknown
/// and is replaced by the next check.
#[derive(Clone, Copy, Default, Serialize, Deserialize)]
enum CheckKind {
    #[default]
    Unknown,
    Missing,
    Ready,
    Damaged,
    Unavailable,
}

#[derive(Serialize, Deserialize)]
struct StoredEntry {
    model: Model,
    files: Files,
    #[serde(default)]
    kind: CheckKind,
}

impl From<Entry> for StoredEntry {
    fn from(entry: Entry) -> Self {
        Self {
            model: entry.model,
            files: entry.files,
            kind: match entry.status {
                Status::Unknown => CheckKind::Unknown,
                Status::Missing => CheckKind::Missing,
                Status::Ready => CheckKind::Ready,
                Status::Damaged(_) => CheckKind::Damaged,
                Status::Unavailable(_) => CheckKind::Unavailable,
            },
        }
    }
}

impl From<StoredEntry> for Entry {
    fn from(entry: StoredEntry) -> Self {
        Self {
            model: entry.model,
            files: entry.files,
            status: match entry.kind {
                CheckKind::Unknown => Status::Unknown,
                CheckKind::Missing => Status::Missing,
                CheckKind::Ready => Status::Ready,
                CheckKind::Damaged => {
                    Status::Damaged("Model files need repair. Retry setup.".into())
                }
                CheckKind::Unavailable => Status::Unavailable(
                    "The last model check failed. Rechecking the runtime.".into(),
                ),
            },
        }
    }
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
        if let Some(root) = state.path.as_deref().and_then(Path::parent) {
            // Old absolute paths are accepted only within this profile. Paths
            // outside it or with traversal cannot become remembered readiness.
            state.entries.retain_mut(|entry| {
                entry.files.0.iter_mut().all(|file| {
                    let relative = file.path.strip_prefix(root).unwrap_or(&file.path);
                    if relative
                        .components()
                        .any(|c| !matches!(c, std::path::Component::Normal(_)))
                    {
                        return false;
                    }
                    file.path = root.join(relative);
                    true
                })
            });
        }
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
        let mut stored = self.clone();
        stored.entries.retain_mut(|entry| {
            entry.files.0.iter_mut().all(|file| {
                let Ok(relative) = file.path.strip_prefix(parent) else {
                    return false;
                };
                if relative
                    .components()
                    .any(|c| !matches!(c, std::path::Component::Normal(_)))
                {
                    return false;
                }
                file.path = relative.to_path_buf();
                true
            })
        });
        let bytes = serde_json::to_vec(&stored).map_err(io::Error::other)?;
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
    fn stored_checks_have_relative_paths_and_kinds_without_error_text() {
        let dir = tempfile::tempdir().unwrap();
        let settings = dir.path().join("settings.json");
        let path = dir.path().join("ocr/v1/model.onnx");
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        std::fs::write(&path, b"model").unwrap();
        let files = Files::read(&[path]);
        for status in [
            Status::Damaged("private engine error".into()),
            Status::Unavailable("private runtime error".into()),
        ] {
            let mut state = ModelState::open(Some(&settings));
            state.record(MODEL, &files, files.clone(), status);
            state.save().unwrap();
            let json = std::fs::read_to_string(dir.path().join("model-state.json")).unwrap();
            let value: serde_json::Value = serde_json::from_str(&json).unwrap();
            assert_eq!(
                Path::new(value["entries"][0]["files"][0]["path"].as_str().unwrap()),
                Path::new("ocr/v1/model.onnx")
            );
            assert!(!json.contains("private"));
            assert!(!json.contains(&dir.path().to_string_lossy().replace('\\', "\\\\")));
            let reopened = ModelState::open(Some(&settings));
            assert!(matches!(
                reopened.status(MODEL, &files),
                Some(Status::Damaged(_) | Status::Unavailable(_))
            ));
        }
    }

    #[test]
    fn legacy_status_defaults_to_unknown_and_is_sanitized_on_save() {
        let dir = tempfile::tempdir().unwrap();
        let settings = dir.path().join("settings.json");
        let files = Files::read(&[dir.path().join("model.onnx")]);
        let legacy = serde_json::json!({"entries": [{"model": MODEL, "files": files, "status": {"Unavailable": "private old error"}}]});
        let path = dir.path().join("model-state.json");
        std::fs::write(&path, legacy.to_string()).unwrap();
        let state = ModelState::open(Some(&settings));
        assert_eq!(state.status(MODEL, &files), Some(Status::Unknown));
        state.save().unwrap();
        let json = std::fs::read_to_string(path).unwrap();
        assert!(!json.contains("private old error"));
        assert!(!json.contains("Unavailable"));
        assert!(json.contains("Unknown"));
    }

    #[test]
    fn files_changed_during_a_quiet_check_are_unknown_even_for_missing() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("model.onnx");
        std::fs::write(&path, b"before").unwrap();
        let paths = [path.clone()];
        let before = Files::read(&paths);
        let mut state = ModelState::default();
        state.record(MODEL, &before, before.clone(), Status::Ready);
        std::fs::remove_file(path).unwrap();
        let after = Files::read(&paths);
        assert_eq!(
            state.record(MODEL, &before, after.clone(), Status::Missing),
            Status::Unknown
        );
        assert_eq!(state.status(MODEL, &after), Some(Status::Unknown));
    }

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
