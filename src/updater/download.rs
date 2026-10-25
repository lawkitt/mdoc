//! Fetching a release's disk image and checksum (ADR 0040). Redirects are
//! followed by hand so every hop stays on GitHub; the checksum is compared
//! while the file streams in.

use std::{
    io::{Read, Write},
    path::Path,
    sync::{
        Arc,
        atomic::{AtomicBool, AtomicU64, Ordering},
    },
    time::Duration,
};

use sha2::{Digest, Sha256};

/// The macOS release asset for `version` (ADR 0037 naming).
pub fn dmg_name(version: &str) -> String {
    format!("mdoc_{version}_arm64.dmg")
}

/// The hex digest `SHA256SUMS` lists for `name` (`sha256sum` format).
pub fn checksum_for(sums: &str, name: &str) -> Option<String> {
    sums.lines().find_map(|line| {
        let (hash, file) = line.split_once(char::is_whitespace)?;
        let file = file.trim_start().trim_start_matches('*');
        (file == name && hash.len() == 64 && hash.bytes().all(|b| b.is_ascii_hexdigit()))
            .then(|| hash.to_ascii_lowercase())
    })
}

/// Release downloads start on github.com and redirect to its asset CDN;
/// anything else means the chain left GitHub.
pub fn allowed(url: &str) -> bool {
    url::Url::parse(url).is_ok_and(|u| {
        u.scheme() == "https"
            && u.host_str().is_some_and(|h| {
                h == "github.com" || h == "api.github.com" || h.ends_with(".githubusercontent.com")
            })
    })
}

/// Live progress of one download, shared with the UI.
#[derive(Clone, Debug, Default)]
pub struct Progress {
    pub received: Arc<AtomicU64>,
    pub cancel: Arc<AtomicBool>,
}

/// Why a download stopped.
#[derive(Debug, PartialEq)]
pub enum Stop {
    Cancelled,
    Failed(String),
}

impl From<String> for Stop {
    fn from(message: String) -> Self {
        Stop::Failed(message)
    }
}

/// A small text file such as `SHA256SUMS`.
pub fn text(url: &str) -> Result<String, String> {
    let mut response = get(url)?;
    response
        .body_mut()
        .with_config()
        .limit(1 << 20)
        .read_to_string()
        .map_err(|e| crate::model_download::download_error(url, e))
}

/// Download `url` to `path`, failing unless it is exactly `size` bytes with
/// SHA-256 `sha` (hex).
pub fn file(url: &str, size: u64, sha: &str, path: &Path, progress: &Progress) -> Result<(), Stop> {
    let stopped = || progress.cancel.load(Ordering::Relaxed);
    let mut response = get(url)?;
    let mut input = response.body_mut().as_reader();
    let mut output = std::fs::File::create(path).map_err(|e| e.to_string())?;
    let mut digest = Sha256::new();
    let mut buf = [0; 65536];
    let mut total = 0u64;
    loop {
        if stopped() {
            return Err(Stop::Cancelled);
        }
        let n = input
            .read(&mut buf)
            .map_err(|_| crate::model_download::unreachable(url))?;
        if n == 0 {
            break;
        }
        total += n as u64;
        if total > size {
            return Err(Stop::Failed(
                "The update is larger than the release says.".into(),
            ));
        }
        digest.update(&buf[..n]);
        output.write_all(&buf[..n]).map_err(|e| e.to_string())?;
        progress.received.store(total, Ordering::Relaxed);
    }
    output.sync_all().map_err(|e| e.to_string())?;
    let found: String = digest
        .finalize()
        .iter()
        .map(|b| format!("{b:02x}"))
        .collect();
    if total != size || found != sha {
        return Err(Stop::Failed(
            "The downloaded update doesn't match its published checksum.".into(),
        ));
    }
    Ok(())
}

/// GET with manually followed redirects, each hop checked by [`allowed`].
fn get(url: &str) -> Result<ureq::http::Response<ureq::Body>, String> {
    if mdoc_store::app_paths::isolated() {
        return Err("network is disabled in headless renders".into());
    }
    let agent = ureq::Agent::new_with_config(
        ureq::Agent::config_builder()
            .https_only(true)
            .max_redirects(0)
            .timeout_connect(Some(Duration::from_secs(15)))
            .timeout_recv_body(Some(Duration::from_secs(60)))
            .build(),
    );
    let mut url = url.to_owned();
    for _ in 0..5 {
        if !allowed(&url) {
            return Err(format!("Refused to download the update from {url}."));
        }
        let response = agent
            .get(&url)
            .header(
                "User-Agent",
                concat!(
                    "mdoc/",
                    env!("CARGO_PKG_VERSION"),
                    " (https://github.com/lawkitt/mdoc)"
                ),
            )
            .call()
            .map_err(|e| crate::model_download::download_error(&url, e))?;
        if !response.status().is_redirection() {
            return Ok(response);
        }
        let location = response
            .headers()
            .get("location")
            .and_then(|v| v.to_str().ok())
            .ok_or("The update server sent a redirect without a location.")?;
        url = url::Url::parse(&url)
            .and_then(|base| base.join(location))
            .map_err(|e| e.to_string())?
            .to_string();
    }
    Err("The update download redirected too many times.".into())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reads_sha256sum_lines() {
        let a = "a".repeat(64);
        let b = "B".repeat(64);
        let sums =
            format!("{a}  mdoc_0.2.0_arm64.dmg\n{b} *mdoc_0.2.0_x64-setup.exe\nshort  other\n");
        assert_eq!(checksum_for(&sums, &dmg_name("0.2.0")), Some(a));
        assert_eq!(
            checksum_for(&sums, "mdoc_0.2.0_x64-setup.exe"),
            Some(b.to_ascii_lowercase())
        );
        assert_eq!(checksum_for(&sums, "other"), None);
        assert_eq!(checksum_for(&sums, "mdoc_0.2.0_arm64.dmg.sig"), None);
    }

    #[test]
    fn downloads_stay_on_github() {
        assert!(allowed(
            "https://github.com/lawkitt/mdoc/releases/download/v0.2.0/mdoc_0.2.0_arm64.dmg"
        ));
        assert!(allowed(
            "https://release-assets.githubusercontent.com/github-production-release-asset/1"
        ));
        assert!(!allowed("http://github.com/lawkitt/mdoc"));
        assert!(!allowed("https://github.com.evil.example/x"));
        assert!(!allowed("https://evilgithubusercontent.com/x"));
        assert!(!allowed("https://example.com/mdoc.dmg"));
    }
}
