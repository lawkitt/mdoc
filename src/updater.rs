//! Updates from the project's GitHub Releases: detection (ADR 0038), ported
//! from Zorite's `updater.rs`, and in-app installation on macOS (ADR 0040).
//!
//! Checks compare the newest release with `CARGO_PKG_VERSION` and publish
//! [`UpdateState`] for the toolbar dot, the update banner and Settings →
//! Updates. A check is one unauthenticated GET that carries only the app
//! version (User-Agent); failures are silent and offline behaves like "no
//! update". Nothing is downloaded until the user clicks **Download and
//! install**; the downloaded app is verified, staged, and swapped in only
//! while mdoc quits ([`apply_at_quit`]).

mod download;
#[cfg(target_os = "macos")]
mod macos;

use std::{
    path::PathBuf,
    sync::{Arc, atomic::Ordering},
    time::Duration,
};

use serde::{Deserialize, Serialize};

pub use download::Progress;

const REPOSITORY: &str = "lawkitt/mdoc";

/// The latest check's outcome and any install in progress, a gpui global read
/// by render code.
#[derive(Clone, Debug, Default)]
pub struct UpdateState {
    pub checking: bool,
    /// A check has succeeded at least once this session.
    pub checked: bool,
    /// `Some` when a newer release exists.
    pub available: Option<UpdateAvailable>,
    /// `available` came from an automatic check; only those raise the banner.
    pub automatic: bool,
    pub install: Install,
    /// Whether this copy may install updates; `None` until probed at launch.
    pub eligibility: Option<Result<(), Ineligible>>,
    /// The version whose *available* banner was dismissed (remembered).
    pub dismissed: Option<String>,
    /// A banner dismissed for this session.
    pub hidden: Option<Banner>,
    /// mdoc was just updated to this version (one-time notice).
    pub updated_to: Option<String>,
    /// The next quit should relaunch mdoc.
    pub restart_requested: bool,
}
impl gpui::Global for UpdateState {}

#[derive(Clone, Debug, PartialEq)]
pub struct UpdateAvailable {
    /// Bare version, e.g. `0.2.0` or `0.2.0-beta.1`.
    pub version: String,
    /// The release page, opened by "View release".
    pub html_url: String,
    /// Release notes (Markdown); the full text lives at `html_url`.
    pub notes: String,
    /// The macOS disk image, when the release has one.
    pub dmg: Option<Asset>,
    /// The release's `SHA256SUMS`, when it has one.
    pub checksums: Option<String>,
}

#[derive(Clone, Debug, PartialEq)]
pub struct Asset {
    pub url: String,
    pub size: u64,
}

/// The install pipeline's state (ADR 0040).
#[derive(Clone, Debug, Default)]
pub enum Install {
    #[default]
    Idle,
    Downloading {
        version: String,
        received: u64,
        total: u64,
        progress: Progress,
    },
    /// Verified and staged; swapped in when mdoc quits.
    Ready {
        version: String,
        staged: PathBuf,
    },
    Failed {
        version: String,
        message: String,
    },
}

/// Why this copy of mdoc cannot replace itself.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[allow(dead_code, reason = "each platform constructs a subset")]
pub enum Ineligible {
    Unsupported,
    NotABundle,
    Translocated,
    DiskImage,
    NotWritable,
    NotSigned,
}

impl Ineligible {
    pub fn reason(self) -> &'static str {
        match self {
            Self::Unsupported => "On this system, install updates from the release page.",
            Self::NotABundle => "This development build can't update itself.",
            Self::Translocated => {
                "Move mdoc to the Applications folder and reopen it to install updates here."
            }
            Self::DiskImage => {
                "mdoc is running from its disk image. Drag it to Applications and reopen it to install updates here."
            }
            Self::NotWritable => {
                "mdoc can't replace itself in its current folder. Install the update from the release page."
            }
            Self::NotSigned => {
                "This build isn't signed by the mdoc developer, so it can't update itself."
            }
        }
    }
}

/// What the update banner shows.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum BannerKind {
    Available,
    Downloading,
    Ready,
    Failed,
    Updated,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Banner {
    pub kind: BannerKind,
    pub version: String,
}

impl UpdateState {
    /// The banner to show, if any (ADR 0040): install progress first, then the
    /// one-time "updated" notice, then an update found by an automatic check.
    pub fn banner(&self) -> Option<Banner> {
        let (kind, version) = match &self.install {
            Install::Downloading { version, .. } => (BannerKind::Downloading, version),
            Install::Ready { version, .. } => (BannerKind::Ready, version),
            Install::Failed { version, .. } => (BannerKind::Failed, version),
            Install::Idle => match &self.updated_to {
                Some(version) => (BannerKind::Updated, version),
                None => {
                    let available = self.available.as_ref().filter(|a| {
                        self.automatic && self.dismissed.as_deref() != Some(a.version.as_str())
                    })?;
                    (BannerKind::Available, &available.version)
                }
            },
        };
        let banner = Banner {
            kind,
            version: version.clone(),
        };
        (self.hidden.as_ref() != Some(&banner)).then_some(banner)
    }

    /// Whether Settings carries the update dot: a newer release exists or an
    /// install needs attention. The one-time "installed" notice doesn't count.
    pub fn marks_settings(&self) -> bool {
        self.available.is_some()
            || self
                .banner()
                .is_some_and(|b| b.kind != BannerKind::Updated)
    }

    /// Download progress in percent while downloading.
    pub fn percent(&self) -> Option<u8> {
        match &self.install {
            Install::Downloading {
                received, total, ..
            } if *total > 0 => Some((received * 100 / total).min(100) as u8),
            _ => None,
        }
    }

    /// `Ok` when the available update can be installed in the app; otherwise
    /// the reason it can't, shown beside **View release**.
    pub fn installable(&self) -> Result<(), &'static str> {
        let Some(update) = &self.available else {
            return Err("");
        };
        match self.eligibility {
            None => return Err("Checking whether mdoc can update itself…"),
            Some(Err(why)) => return Err(why.reason()),
            Some(Ok(())) => {}
        }
        if update.dmg.is_none() || update.checksums.is_none() {
            return Err("This release can't be installed from inside mdoc.");
        }
        Ok(())
    }
}

/// The GitHub Releases fields we read.
#[derive(Debug, Deserialize)]
struct Release {
    #[serde(default)]
    tag_name: String,
    #[serde(default)]
    html_url: String,
    #[serde(default)]
    body: Option<String>,
    #[serde(default)]
    draft: bool,
    #[serde(default)]
    prerelease: bool,
    #[serde(default)]
    assets: Vec<ReleaseAsset>,
}

#[derive(Debug, Deserialize)]
struct ReleaseAsset {
    name: String,
    browser_download_url: String,
    #[serde(default)]
    size: u64,
}

pub fn state(cx: &gpui::App) -> UpdateState {
    cx.try_global::<UpdateState>().cloned().unwrap_or_default()
}

fn publish(state: UpdateState, cx: &mut gpui::App) {
    cx.set_global(state);
    cx.refresh_windows();
}

/// Pre-release builds always consider pre-releases; stable builds only when
/// the user opted in.
pub fn includes_prereleases(opted_in: bool) -> bool {
    opted_in || is_prerelease_build()
}

pub fn is_prerelease_build() -> bool {
    current().is_some_and(|v| !v.pre.is_empty())
}

fn current() -> Option<semver::Version> {
    semver::Version::parse(env!("CARGO_PKG_VERSION")).ok()
}

/// The release page for `version`.
pub fn release_url(version: &str) -> String {
    format!("https://github.com/{REPOSITORY}/releases/tag/v{version}")
}

/// Launch-time setup: restore remembered choices and the outcome of the last
/// quit-time install, clear leftover downloads, and probe whether this copy
/// may update itself.
pub fn init(cx: &mut gpui::App) {
    let mut record = Record::load();
    let mut state = state(cx);
    state.dismissed = record.dismissed.clone();
    if record.installed.as_deref() == Some(env!("CARGO_PKG_VERSION")) {
        state.updated_to = record.installed.clone();
    }
    // The last quit's outcome is shown once.
    if record.installed.take().is_some() | record.failed.is_some() {
        if let Some(failure) = record.failed.take() {
            state.install = Install::Failed {
                version: failure.version,
                message: failure.message,
            };
        }
        record.save();
    }
    publish(state, cx);
    cx.spawn(async move |cx| {
        let eligibility = cx
            .background_executor()
            .spawn(async move {
                if let Some(dir) = updates_dir() {
                    let _ = std::fs::remove_dir_all(dir);
                }
                eligibility().map(|_| ())
            })
            .await;
        cx.update(|cx| {
            let mut state = self::state(cx);
            state.eligibility = Some(eligibility);
            publish(state, cx);
        });
    })
    .detach();
}

/// Check in the background, then publish the result and repaint. A check
/// already in flight makes this a no-op. `automatic` marks checks the user
/// didn't start by hand; only those raise the *available* banner.
pub fn spawn_check(opted_in: bool, automatic: bool, cx: &mut gpui::App) {
    let mut state = state(cx);
    if state.checking {
        return;
    }
    state.checking = true;
    publish(state, cx);
    let include = includes_prereleases(opted_in);
    cx.spawn(async move |cx| {
        let result = cx
            .background_executor()
            .spawn(async move { fetch(include) })
            .await;
        cx.update(|cx| {
            let mut state = self::state(cx);
            state.checking = false;
            if let (Ok(releases), Some(current)) = (result, current()) {
                state.checked = true;
                let found = newest(releases, &current, include);
                if found.as_ref().map(|f| &f.version)
                    != state.available.as_ref().map(|a| &a.version)
                {
                    state.automatic = false;
                }
                state.automatic |= automatic && found.is_some();
                state.available = found;
                // A newer release than the staged one replaces it (ADR 0040).
                if let (Install::Ready { version, staged }, Some(available)) =
                    (&state.install, &state.available)
                    && *version != available.version
                {
                    discard(staged.clone(), cx);
                    state.install = Install::Idle;
                }
            }
            publish(state, cx);
        });
    })
    .detach();
}

/// Download, verify and stage the available update (ADR 0040). Progress
/// and the outcome are published to [`UpdateState`].
pub fn start_install(cx: &mut gpui::App) {
    let mut state = state(cx);
    if state.installable().is_err()
        || matches!(
            state.install,
            Install::Downloading { .. } | Install::Ready { .. }
        )
    {
        return;
    }
    let Some(update) = state.available.clone() else {
        return;
    };
    let progress = Progress::default();
    state.hidden = None;
    state.install = Install::Downloading {
        version: update.version.clone(),
        received: 0,
        total: update.dmg.as_ref().map_or(0, |d| d.size),
        progress: progress.clone(),
    };
    publish(state, cx);
    let worker = progress.clone();
    let version = update.version.clone();
    cx.spawn(async move |cx| {
        let task = cx
            .background_executor()
            .spawn(async move { prepare(&update, &worker) });
        // Repaint progress until the pipeline finishes.
        let poll = progress.clone();
        let ticker = cx.spawn(async move |cx| {
            loop {
                cx.background_executor()
                    .timer(Duration::from_millis(200))
                    .await;
                cx.update(|cx| {
                    let mut state = self::state(cx);
                    if let Install::Downloading {
                        received, progress, ..
                    } = &mut state.install
                        && Arc::ptr_eq(&progress.received, &poll.received)
                    {
                        *received = poll.received.load(Ordering::Relaxed);
                        publish(state, cx);
                    }
                });
            }
        });
        let result = task.await;
        drop(ticker);
        cx.update(|cx| {
            let mut state = self::state(cx);
            let current = matches!(&state.install,
                Install::Downloading { progress: p, .. } if Arc::ptr_eq(&p.received, &progress.received));
            if !current {
                return;
            }
            state.install = match result {
                Ok(staged) => Install::Ready { version, staged },
                Err(download::Stop::Cancelled) => Install::Idle,
                Err(download::Stop::Failed(message)) => {
                    eprintln!("mdoc: update to {version} failed: {message}");
                    Install::Failed { version, message }
                }
            };
            publish(state, cx);
        });
    })
    .detach();
}

/// Stop a download in progress.
pub fn cancel_install(cx: &mut gpui::App) {
    let mut state = state(cx);
    if let Install::Downloading { progress, .. } = &state.install {
        progress.cancel.store(true, Ordering::Relaxed);
        state.install = Install::Idle;
        publish(state, cx);
    }
}

/// Hide the current banner: *available* for good (per version), the others
/// for this session.
pub fn dismiss_banner(cx: &mut gpui::App) {
    let mut state = state(cx);
    let Some(banner) = state.banner() else {
        return;
    };
    match banner.kind {
        BannerKind::Available => {
            state.dismissed = Some(banner.version.clone());
            let version = banner.version;
            cx.background_executor()
                .spawn(async move {
                    let mut record = Record::load();
                    record.dismissed = Some(version);
                    record.save();
                })
                .detach();
        }
        BannerKind::Updated => state.updated_to = None,
        _ => state.hidden = Some(banner),
    }
    publish(state, cx);
}

/// Ask the next quit to relaunch mdoc (Restart now).
pub fn request_restart(cx: &mut gpui::App) {
    let mut state = state(cx);
    state.restart_requested = true;
    cx.set_global(state);
}

/// The user cancelled the quit that Restart now started.
pub fn cancel_restart(cx: &mut gpui::App) {
    let mut state = state(cx);
    if state.restart_requested {
        state.restart_requested = false;
        cx.set_global(state);
    }
}

/// Whether the quit in progress should relaunch mdoc.
pub fn restart_requested(cx: &gpui::App) -> bool {
    cx.try_global::<UpdateState>()
        .is_some_and(|s| s.restart_requested && matches!(s.install, Install::Ready { .. }))
}

/// Replace the installed app with a staged update. Blocking: runs in the
/// synchronous part of the quit handler, after the windows have closed and
/// before GPUI relaunches (ADR 0040). Failures are kept for the next launch,
/// never retried silently.
pub fn apply_at_quit(cx: &gpui::App) {
    let Some(Install::Ready { version, staged }) =
        cx.try_global::<UpdateState>().map(|s| s.install.clone())
    else {
        return;
    };
    let result = install(&staged, &version);
    let mut record = Record::load();
    match result {
        Ok(()) => record.installed = Some(version),
        Err(message) => {
            eprintln!("mdoc: update to {version} failed at quit: {message}");
            record.failed = Some(Failure { version, message });
        }
    }
    record.save();
    if let Some(dir) = updates_dir() {
        let _ = std::fs::remove_dir_all(dir);
    }
}

/// `mdoc --mdoc-self-update`: the whole pipeline without a window, for
/// `just smoke-update` (ADR 0040). Same eligibility and gates as the app.
pub fn self_update() -> Result<String, String> {
    eligibility().map_err(|why| why.reason().to_owned())?;
    let current = current().ok_or("The app version isn't SemVer.")?;
    let include = includes_prereleases(false);
    let Some(update) = newest(fetch(include)?, &current, include) else {
        return Ok(format!("mdoc {current} is the latest version."));
    };
    let staged = prepare(&update, &Progress::default()).map_err(|stop| match stop {
        download::Stop::Cancelled => "Cancelled.".to_owned(),
        download::Stop::Failed(message) => message,
    })?;
    install(&staged, &update.version)?;
    if let Some(dir) = updates_dir() {
        let _ = std::fs::remove_dir_all(dir);
    }
    Ok(format!("Updated mdoc {current} to {}.", update.version))
}

/// The installed bundle this copy may replace.
fn eligibility() -> Result<PathBuf, Ineligible> {
    if cfg!(test) {
        return Err(Ineligible::NotABundle);
    }
    #[cfg(target_os = "macos")]
    {
        let exe = std::env::current_exe().map_err(|_| Ineligible::NotABundle)?;
        macos::eligibility(&exe)
    }
    #[cfg(not(target_os = "macos"))]
    {
        Err(Ineligible::Unsupported)
    }
}

/// Swap the staged bundle over the installed one.
fn install(staged: &std::path::Path, version: &str) -> Result<(), String> {
    let installed = eligibility().map_err(|why| why.reason().to_owned())?;
    #[cfg(target_os = "macos")]
    {
        macos::swap(staged, &installed, version)
    }
    #[cfg(not(target_os = "macos"))]
    {
        let _ = (staged, installed, version);
        Err(Ineligible::Unsupported.reason().into())
    }
}

/// Blocking: download the disk image, check it against `SHA256SUMS`, copy
/// the app out of it and verify that copy. Returns the staged `.app`.
fn prepare(update: &UpdateAvailable, progress: &Progress) -> Result<PathBuf, download::Stop> {
    let (Some(dmg), Some(checksums)) = (&update.dmg, &update.checksums) else {
        return Err("This release can't be installed from inside mdoc."
            .to_owned()
            .into());
    };
    // Each attempt gets its own folder: a cancelled worker may still be
    // verifying while the user retries, and must not share paths with it.
    let root = updates_dir().ok_or("No cache folder for the update.".to_owned())?;
    std::fs::create_dir_all(&root).map_err(|e| e.to_string())?;
    let dir = tempfile::Builder::new()
        .prefix("attempt-")
        .tempdir_in(&root)
        .map_err(|e| e.to_string())?
        .keep();
    let result = stage_into(update, dmg, checksums, &dir, progress);
    if result.is_err() {
        let _ = std::fs::remove_dir_all(&dir);
    }
    result
}

fn stage_into(
    update: &UpdateAvailable,
    dmg: &Asset,
    checksums: &str,
    dir: &std::path::Path,
    progress: &Progress,
) -> Result<PathBuf, download::Stop> {
    let name = download::dmg_name(&update.version);
    let sha = download::checksum_for(&download::text(checksums)?, &name)
        .ok_or_else(|| format!("The release's SHA256SUMS doesn't list {name}."))?;
    let image = dir.join(&name);
    download::file(&dmg.url, dmg.size, &sha, &image, progress)?;
    if progress.cancel.load(Ordering::Relaxed) {
        return Err(download::Stop::Cancelled);
    }
    #[cfg(target_os = "macos")]
    {
        let staged = dir.join("mdoc.app");
        macos::stage(&image, &staged)?;
        let _ = std::fs::remove_file(&image);
        macos::verify(&staged, &update.version, true)?;
        Ok(staged)
    }
    #[cfg(not(target_os = "macos"))]
    {
        Err(Ineligible::Unsupported.reason().to_owned().into())
    }
}

/// Delete a staged update together with its attempt folder.
fn discard(staged: PathBuf, cx: &gpui::App) {
    cx.background_executor()
        .spawn(async move {
            if let Some(attempt) = staged.parent() {
                let _ = std::fs::remove_dir_all(attempt);
            }
        })
        .detach();
}

/// Downloads and the staged app: `~/Library/Caches/io.github.lawkitt.mdoc/updates`.
fn updates_dir() -> Option<PathBuf> {
    if cfg!(test) {
        return None;
    }
    dirs::cache_dir().map(|d| d.join("io.github.lawkitt.mdoc/updates"))
}

/// Update facts that outlive a launch: the dismissed banner version and the
/// outcome of the last quit-time install, read once at the next launch.
#[derive(Debug, Default, Serialize, Deserialize)]
struct Record {
    #[serde(default)]
    dismissed: Option<String>,
    #[serde(default)]
    installed: Option<String>,
    #[serde(default)]
    failed: Option<Failure>,
}

#[derive(Debug, Serialize, Deserialize)]
struct Failure {
    version: String,
    message: String,
}

impl Record {
    fn path() -> Option<PathBuf> {
        if cfg!(test) {
            return None;
        }
        dirs::data_local_dir().map(|d| d.join("mdoc/update.json"))
    }
    fn load() -> Self {
        Self::path()
            .and_then(|p| std::fs::read(p).ok())
            .and_then(|b| serde_json::from_slice(&b).ok())
            .unwrap_or_default()
    }
    fn save(&self) {
        let Some(path) = Self::path() else {
            return;
        };
        if let Some(parent) = path.parent() {
            let _ = std::fs::create_dir_all(parent);
        }
        if let Ok(json) = serde_json::to_vec_pretty(self) {
            let _ = std::fs::write(path, json);
        }
    }
}

/// The newest non-draft release above `current`, by SemVer rather than API
/// order (a backported patch can be newest by date).
fn newest(
    releases: Vec<Release>,
    current: &semver::Version,
    include_prereleases: bool,
) -> Option<UpdateAvailable> {
    let (version, release) = releases
        .into_iter()
        .filter(|r| !r.draft && (include_prereleases || !r.prerelease))
        .filter_map(|r| {
            let tag = r.tag_name.strip_prefix('v').unwrap_or(&r.tag_name);
            Some((semver::Version::parse(tag).ok()?, r))
        })
        .max_by(|(a, _), (b, _)| a.cmp(b))?;
    (version > *current).then(|| {
        let version = version.to_string();
        let asset = |name: &str| release.assets.iter().find(|a| a.name == name);
        UpdateAvailable {
            dmg: asset(&download::dmg_name(&version)).map(|a| Asset {
                url: a.browser_download_url.clone(),
                size: a.size,
            }),
            checksums: asset("SHA256SUMS").map(|a| a.browser_download_url.clone()),
            version,
            html_url: release.html_url,
            notes: release.body.unwrap_or_default(),
        }
    })
}

/// Blocking; call from a background task. `/releases/latest` already excludes
/// drafts and pre-releases; the list endpoint is needed when they count.
fn fetch(include_prereleases: bool) -> Result<Vec<Release>, String> {
    if cfg!(test) {
        return Err("no network in tests".into());
    }
    let url = if include_prereleases {
        format!("https://api.github.com/repos/{REPOSITORY}/releases?per_page=30")
    } else {
        format!("https://api.github.com/repos/{REPOSITORY}/releases/latest")
    };
    let agent = ureq::Agent::new_with_config(
        ureq::Agent::config_builder()
            .https_only(true)
            .timeout_global(Some(Duration::from_secs(10)))
            .build(),
    );
    let body = agent
        .get(&url)
        .header("Accept", "application/vnd.github+json")
        .header(
            "User-Agent",
            concat!(
                "mdoc/",
                env!("CARGO_PKG_VERSION"),
                " (https://github.com/lawkitt/mdoc)"
            ),
        )
        .call()
        .map_err(|e| e.to_string())?
        .body_mut()
        .read_to_string()
        .map_err(|e| e.to_string())?;
    if include_prereleases {
        serde_json::from_str(&body).map_err(|e| e.to_string())
    } else {
        serde_json::from_str(&body)
            .map(|r| vec![r])
            .map_err(|e| e.to_string())
    }
}

/// An installable update for UI tests.
#[cfg(test)]
pub fn test_update(version: &str) -> UpdateAvailable {
    UpdateAvailable {
        version: version.into(),
        html_url: release_url(version),
        notes: "Notes".into(),
        dmg: Some(Asset {
            url: format!("https://github.com/{REPOSITORY}/releases/download/v{version}/x.dmg"),
            size: 100,
        }),
        checksums: Some(format!(
            "https://github.com/{REPOSITORY}/releases/download/v{version}/SHA256SUMS"
        )),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn release(tag: &str, prerelease: bool, draft: bool) -> Release {
        Release {
            tag_name: tag.into(),
            html_url: format!("https://github.com/{REPOSITORY}/releases/tag/{tag}"),
            body: Some(format!("notes {tag}")),
            draft,
            prerelease,
            assets: Vec::new(),
        }
    }
    fn newest_of(releases: Vec<Release>, current: &str, include: bool) -> Option<String> {
        newest(releases, &semver::Version::parse(current).unwrap(), include).map(|a| a.version)
    }
    use super::test_update as available;

    #[test]
    fn package_version_is_semver() {
        assert!(current().is_some());
    }

    #[test]
    fn picks_the_highest_newer_version_and_skips_drafts_and_bad_tags() {
        let releases = vec![
            release("v0.3.0", false, true),
            release("v0.2.1", false, false),
            release("v0.2.10", false, false),
            release("nightly", false, false),
        ];
        assert_eq!(
            newest_of(releases, "0.2.1", false).as_deref(),
            Some("0.2.10")
        );
        assert_eq!(
            newest_of(vec![release("v0.2.1", false, false)], "0.2.1", false),
            None
        );
        assert_eq!(
            newest_of(vec![release("v0.1.0", false, false)], "0.2.0", false),
            None
        );
    }

    #[test]
    fn prereleases_count_only_when_included() {
        let releases = || {
            vec![
                release("v0.1.0-beta.2", true, false),
                release("v0.1.0-beta.1", true, false),
            ]
        };
        assert_eq!(newest_of(releases(), "0.1.0-beta.1", false), None);
        assert_eq!(
            newest_of(releases(), "0.1.0-beta.1", true).as_deref(),
            Some("0.1.0-beta.2")
        );
        // The stable release outranks its betas.
        let mut all = releases();
        all.push(release("v0.1.0", false, false));
        assert_eq!(
            newest_of(all, "0.1.0-beta.2", true).as_deref(),
            Some("0.1.0")
        );
    }

    #[test]
    fn prerelease_builds_always_include_prereleases() {
        assert!(includes_prereleases(true));
        assert_eq!(includes_prereleases(false), is_prerelease_build());
    }

    #[test]
    fn finds_the_disk_image_and_checksums_of_the_newest_release() {
        let mut with_assets = release("v0.2.0", false, false);
        with_assets.assets = [
            "mdoc_0.2.0_arm64.dmg",
            "mdoc_0.2.0_x64-setup.exe",
            "SHA256SUMS",
        ]
        .into_iter()
        .map(|name| ReleaseAsset {
            name: name.into(),
            browser_download_url: format!("https://github.com/d/{name}"),
            size: 7,
        })
        .collect();
        let found = newest(
            vec![with_assets],
            &semver::Version::parse("0.1.0").unwrap(),
            false,
        )
        .unwrap();
        assert_eq!(
            found.dmg,
            Some(Asset {
                url: "https://github.com/d/mdoc_0.2.0_arm64.dmg".into(),
                size: 7
            })
        );
        assert_eq!(
            found.checksums.as_deref(),
            Some("https://github.com/d/SHA256SUMS")
        );
        // Without both files the release is detection-only.
        let bare = newest(
            vec![release("v0.2.0", false, false)],
            &semver::Version::parse("0.1.0").unwrap(),
            false,
        )
        .unwrap();
        let state = UpdateState {
            available: Some(bare),
            eligibility: Some(Ok(())),
            ..Default::default()
        };
        assert!(state.installable().is_err());
    }

    #[test]
    fn installable_needs_an_eligible_copy() {
        let mut state = UpdateState {
            available: Some(available("9.0.0")),
            ..Default::default()
        };
        assert!(state.installable().is_err(), "not probed yet");
        state.eligibility = Some(Err(Ineligible::Translocated));
        assert_eq!(state.installable(), Err(Ineligible::Translocated.reason()));
        state.eligibility = Some(Ok(()));
        assert_eq!(state.installable(), Ok(()));
    }

    #[test]
    fn banner_follows_the_install_and_respects_dismissal() {
        let banner = |kind, version: &str| {
            Some(Banner {
                kind,
                version: version.into(),
            })
        };
        let mut state = UpdateState {
            available: Some(available("9.0.0")),
            ..Default::default()
        };
        // Only automatic checks raise the available banner.
        assert_eq!(state.banner(), None);
        state.automatic = true;
        assert_eq!(state.banner(), banner(BannerKind::Available, "9.0.0"));
        state.dismissed = Some("9.0.0".into());
        assert_eq!(state.banner(), None);
        // User-started installs always show.
        state.install = Install::Downloading {
            version: "9.0.0".into(),
            received: 50,
            total: 200,
            progress: Progress::default(),
        };
        assert_eq!(state.banner(), banner(BannerKind::Downloading, "9.0.0"));
        assert_eq!(state.percent(), Some(25));
        state.install = Install::Ready {
            version: "9.0.0".into(),
            staged: PathBuf::new(),
        };
        assert_eq!(state.banner(), banner(BannerKind::Ready, "9.0.0"));
        state.hidden = banner(BannerKind::Ready, "9.0.0");
        assert_eq!(state.banner(), None);
        state.install = Install::Failed {
            version: "9.0.0".into(),
            message: "disk full".into(),
        };
        assert_eq!(state.banner(), banner(BannerKind::Failed, "9.0.0"));
        state.install = Install::Idle;
        state.updated_to = Some("9.0.0".into());
        assert_eq!(state.banner(), banner(BannerKind::Updated, "9.0.0"));
    }

    #[gpui::test]
    fn dismissing_and_restart_requests_update_the_global(cx: &mut gpui::TestAppContext) {
        cx.update(|cx| {
            cx.set_global(UpdateState {
                available: Some(available("9.0.0")),
                automatic: true,
                ..Default::default()
            });
            dismiss_banner(cx);
            assert_eq!(state(cx).dismissed.as_deref(), Some("9.0.0"));
            assert_eq!(state(cx).banner(), None);
            // Restart only matters once an update is staged.
            request_restart(cx);
            assert!(!restart_requested(cx));
            let mut staged = state(cx);
            staged.install = Install::Ready {
                version: "9.0.0".into(),
                staged: PathBuf::new(),
            };
            cx.set_global(staged);
            assert!(restart_requested(cx));
            cancel_restart(cx);
            assert!(!restart_requested(cx));
        });
    }

    #[gpui::test]
    fn cancelling_a_download_returns_to_idle(cx: &mut gpui::TestAppContext) {
        cx.update(|cx| {
            let progress = Progress::default();
            cx.set_global(UpdateState {
                install: Install::Downloading {
                    version: "9.0.0".into(),
                    received: 0,
                    total: 1,
                    progress: progress.clone(),
                },
                ..Default::default()
            });
            cancel_install(cx);
            assert!(progress.cancel.load(Ordering::Relaxed));
            assert!(matches!(state(cx).install, Install::Idle));
        });
    }
}
