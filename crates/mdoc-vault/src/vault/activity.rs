//! A Project's AI activity (#1164 Decision 25; Milestone 2 item 6): which
//! AI app read which of its documents, and each delivery, with the time.
//! The Project page lists it (item 8).
//!
//! Stored as one sealed file per Project, `activity/<project id>.enc`
//! (the storage the maintainer is asked about on #1091; another answer
//! changes this module only). It sits outside the manifest and is read
//! when asked for. It keeps the newest [`ACTIVITY_KEPT`] entries, goes
//! with its Project when that is deleted, and holds ids, app names and
//! times only: no text, no title, no name. A Vault from before it has no
//! such file, which reads as no activity.
//!
//! An entry a later build adds a kind of, or that does not parse, is
//! passed over when read and kept as it was when the file is rewritten.
use super::{Vault, WriteLock};
use crate::{
    ObjectKind, VaultError,
    manifest::activity_path,
    records::{Project, now},
    seal::{Unseal, read_limited},
};
use serde::{Deserialize, Serialize};
use std::{fs::File, io};

/// The newest entries a Project's activity keeps.
pub const ACTIVITY_KEPT: usize = 1000;

/// The most characters of an app's name kept.
const APP_CHARS: usize = 64;

/// One line of a Project's AI activity.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct AiActivity {
    /// Seconds since the Unix epoch.
    pub at: u64,
    /// The AI app, as it named itself ([`app_name`]); empty: it did not.
    #[serde(default)]
    pub app: String,
    #[serde(flatten)]
    pub event: AiEvent,
}

/// What the AI app did.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AiEvent {
    /// Read the Document `document`: its text, its outline (`part`
    /// `None` for a long one) or its part `part`.
    Read {
        document: String,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        part: Option<usize>,
    },
    /// Delivered the AI Reply `reply`, naming the documents it `read`;
    /// `document`: the Document it landed as, too.
    Delivered {
        reply: String,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        document: Option<String>,
        #[serde(default)]
        read: Vec<String>,
    },
}

/// The file: entries oldest first, each as stored, so one a later build
/// wrote survives a rewrite by this one.
#[derive(Default, Serialize, Deserialize)]
struct ActivityFile {
    #[serde(default)]
    entries: Vec<serde_json::Value>,
}

/// `app` as the activity and a delivered reply keep it: one line, no
/// control characters, at most [`APP_CHARS`] characters, trimmed.
pub(super) fn app_name(app: &str) -> String {
    let line: String = (app.chars())
        .map(|c| if c.is_control() { ' ' } else { c })
        .collect();
    line.trim()
        .chars()
        .take(APP_CHARS)
        .collect::<String>()
        .trim_end()
        .to_string()
}

impl Vault {
    /// Project `project`'s AI activity, newest first; none for a Project
    /// that has none yet, or a Vault from before it.
    pub fn ai_activity(&self, project: &str) -> Result<Vec<AiActivity>, VaultError> {
        if !crate::is_id(project) {
            return Err(VaultError::InvalidId);
        }
        let mut entries: Vec<AiActivity> = (self.activity_file(project)?.entries.into_iter())
            .filter_map(|entry| serde_json::from_value(entry).ok())
            .collect();
        entries.reverse();
        Ok(entries)
    }

    /// Note that `app` did `event` in Project `project` now. `NotFound`
    /// once the Project is gone; nothing is written then.
    pub fn record_ai_activity(
        &self,
        project: &str,
        app: &str,
        event: AiEvent,
    ) -> Result<AiActivity, VaultError> {
        let lock = self.write_lock()?;
        self.load_record::<Project>(project)?;
        let entry = AiActivity {
            at: now(),
            app: app_name(app),
            event,
        };
        self.record_locked(&lock, project, &entry)?;
        Ok(entry)
    }

    /// Add `entry` to Project `project`'s activity, which the caller
    /// checked is there, keeping the newest [`ACTIVITY_KEPT`].
    pub(super) fn record_locked(
        &self,
        _lock: &WriteLock<'_>,
        project: &str,
        entry: &AiActivity,
    ) -> Result<(), VaultError> {
        let mut file = self.activity_file(project)?;
        file.entries
            .push(serde_json::to_value(entry).map_err(io::Error::from)?);
        let over = file.entries.len().saturating_sub(ACTIVITY_KEPT);
        file.entries.drain(..over);
        let json = serde_json::to_vec(&file).map_err(io::Error::from)?;
        let path = activity_path(&self.root, project);
        self.write_object(ObjectKind::Activity, project, &path, &json)?;
        Ok(())
    }

    /// Delete Project `project`'s activity with it.
    pub(super) fn delete_activity_locked(
        &self,
        _lock: &WriteLock<'_>,
        project: &str,
    ) -> Result<(), VaultError> {
        self.check_alive()?;
        Ok(super::remove_sealed(&activity_path(&self.root, project))?)
    }

    /// The activity file of `project` as stored; an empty one when there
    /// is none.
    fn activity_file(&self, project: &str) -> Result<ActivityFile, VaultError> {
        let damaged = || VaultError::Damaged {
            kind: ObjectKind::Activity,
            id: project.to_string(),
        };
        let mut file = match File::open(activity_path(&self.root, project)) {
            Ok(file) => file,
            Err(e) if e.kind() == io::ErrorKind::NotFound => return Ok(ActivityFile::default()),
            Err(e) => return Err(e.into()),
        };
        let bytes = match read_limited(&mut file) {
            Ok(bytes) => bytes,
            Err(Unseal::Io(e)) => return Err(e.into()),
            Err(_) => return Err(damaged()),
        };
        let plain = (self.sealer)
            .open(ObjectKind::Activity, project, &bytes)
            .map_err(|_| damaged())?;
        serde_json::from_slice(&plain).map_err(|_| damaged())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::NewProject;

    fn vault() -> (tempfile::TempDir, Vault, String) {
        let temp = tempfile::tempdir().unwrap();
        let vault = Vault::open(temp.path().join("vault"), &[7; 32]).unwrap();
        let project = (vault.create_project(NewProject::default()).unwrap()).id;
        (temp, vault, project)
    }

    /// Seal `json` as `project`'s activity file, as another build may have
    /// written it.
    fn write_raw(vault: &Vault, project: &str, json: &str) {
        let lock = vault.write_lock().unwrap();
        let path = activity_path(&vault.root, project);
        (vault.write_object(ObjectKind::Activity, project, &path, json.as_bytes())).unwrap();
        drop(lock);
    }

    /// Entries of a kind a later build adds, or with fields it adds, are
    /// read as far as this build knows them, and a rewrite keeps them.
    #[test]
    fn a_later_builds_entries_load_and_survive_a_rewrite() {
        let (_temp, vault, project) = vault();
        let doc = "0".repeat(32);
        let later = r#"{"at":1,"app":"Claude","asked":{"document":"x"}}"#;
        write_raw(
            &vault,
            &project,
            &format!(
                r#"{{"entries":[{later},{{"at":2,"app":"Claude","read":{{"document":"{doc}"}},"by":"later"}}],"more":true}}"#
            ),
        );
        let read = AiEvent::Read {
            document: doc.clone(),
            part: None,
        };
        assert_eq!(
            vault.ai_activity(&project).unwrap(),
            vec![AiActivity {
                at: 2,
                app: "Claude".into(),
                event: read.clone(),
            }]
        );
        vault.record_ai_activity(&project, "ChatGPT", read).unwrap();
        assert_eq!(vault.ai_activity(&project).unwrap().len(), 2);
        let file = vault.activity_file(&project).unwrap();
        assert_eq!(
            file.entries[0],
            serde_json::from_str::<serde_json::Value>(later).unwrap()
        );
    }

    /// The oldest entries go once there are more than [`ACTIVITY_KEPT`].
    #[test]
    fn only_the_newest_entries_are_kept() {
        let (_temp, vault, project) = vault();
        let entries: Vec<String> = (0..ACTIVITY_KEPT)
            .map(|at| format!(r#"{{"at":{at},"app":"Claude","read":{{"document":"d"}}}}"#))
            .collect();
        write_raw(
            &vault,
            &project,
            &format!(r#"{{"entries":[{}]}}"#, entries.join(",")),
        );
        let event = AiEvent::Read {
            document: "d".into(),
            part: Some(2),
        };
        vault.record_ai_activity(&project, "Claude", event).unwrap();
        let activity = vault.ai_activity(&project).unwrap();
        assert_eq!(activity.len(), ACTIVITY_KEPT);
        assert_eq!(
            activity[0].event,
            AiEvent::Read {
                document: "d".into(),
                part: Some(2),
            }
        );
        assert_eq!(activity.last().unwrap().at, 1);
    }

    #[test]
    fn an_apps_name_is_one_trimmed_line() {
        assert_eq!(app_name("  Claude\nDesktop\t"), "Claude Desktop");
        assert_eq!(app_name(&"x".repeat(100)).chars().count(), APP_CHARS);
        assert_eq!(app_name(" \u{7} "), "");
    }
}
