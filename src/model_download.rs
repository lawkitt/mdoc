//! Explicit bounded artifact acquisition; progress is per file, never estimated.
use sha2::{Digest, Sha256};
use std::{
    fs::File,
    io::{Read, Write},
    path::Path,
    sync::{
        Arc, Mutex,
        atomic::{AtomicBool, Ordering},
    },
};
#[derive(Clone, Debug, Default)]
pub struct State {
    pub phase: String,
    pub received: u64,
    pub total: u64,
}
#[derive(Clone, Default)]
pub struct Progress {
    pub cancel: Arc<AtomicBool>,
    pub state: Arc<Mutex<State>>,
}
impl Progress {
    pub fn check(&self) -> Result<(), String> {
        if self.cancel.load(Ordering::Relaxed) {
            Err("Download cancelled.".into())
        } else {
            Ok(())
        }
    }
    pub fn phase(&self, phase: &str) {
        self.state.lock().unwrap().phase = phase.into();
    }
}
pub fn verify(path: &Path, bytes: u64, sha: &str) -> Result<(), String> {
    let mut file = File::open(path).map_err(|e| e.to_string())?;
    if file.metadata().map_err(|e| e.to_string())?.len() != bytes {
        return Err("Artifact size mismatch. Repair setup.".into());
    }
    let mut digest = Sha256::new();
    let mut buf = [0; 65536];
    loop {
        let n = file.read(&mut buf).map_err(|e| e.to_string())?;
        if n == 0 {
            break;
        }
        digest.update(&buf[..n]);
    }
    if digest
        .finalize()
        .iter()
        .map(|b| format!("{b:02x}"))
        .collect::<String>()
        != sha
    {
        return Err("Artifact integrity check failed. Repair setup.".into());
    }
    Ok(())
}
pub fn fetch(
    url: &str,
    bytes: u64,
    sha: &str,
    path: &Path,
    progress: &Progress,
) -> Result<(), String> {
    progress.check()?;
    progress.phase("Verifying");
    if verify(path, bytes, sha).is_ok() {
        return Ok(());
    }
    let parent = path.parent().ok_or("Invalid artifact path")?;
    std::fs::create_dir_all(parent).map_err(|e| e.to_string())?;
    *progress.state.lock().unwrap() = State {
        phase: format!(
            "Downloading {}",
            path.file_name().unwrap_or_default().to_string_lossy()
        ),
        received: 0,
        total: bytes,
    };
    let agent = ureq::Agent::new_with_config(
        ureq::Agent::config_builder()
            .https_only(true)
            .timeout_global(Some(std::time::Duration::from_secs(600)))
            .build(),
    );
    let mut response = agent.get(url).call().map_err(|e| e.to_string())?;
    let mut input = response.body_mut().as_reader();
    let mut file = tempfile::NamedTempFile::new_in(parent).map_err(|e| e.to_string())?;
    let mut buf = [0; 65536];
    let mut total = 0;
    loop {
        progress.check()?;
        let n = input.read(&mut buf).map_err(|e| e.to_string())?;
        if n == 0 {
            break;
        }
        total += n as u64;
        if total > bytes {
            return Err("Artifact download exceeded its pinned size.".into());
        }
        file.write_all(&buf[..n]).map_err(|e| e.to_string())?;
        progress.state.lock().unwrap().received = total;
    }
    progress.phase("Verifying");
    file.flush().map_err(|e| e.to_string())?;
    verify(file.path(), bytes, sha)?;
    progress.check()?;
    file.as_file().sync_all().map_err(|e| e.to_string())?;
    file.persist(path).map_err(|e| e.to_string())?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn verified_files_are_reused_offline_and_cancel_keeps_them() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("model");
        std::fs::write(&path, b"abc").unwrap();
        let sha = Sha256::digest(b"abc")
            .iter()
            .map(|b| format!("{b:02x}"))
            .collect::<String>();
        let progress = Progress::default();
        fetch("https://invalid.invalid/model", 3, &sha, &path, &progress).unwrap();
        progress.cancel.store(true, Ordering::Relaxed);
        assert!(fetch("https://invalid.invalid/model", 3, &sha, &path, &progress).is_err());
        assert_eq!(std::fs::read(path).unwrap(), b"abc");
    }
}
