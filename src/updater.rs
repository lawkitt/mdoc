//! Update check against the project's GitHub Releases (ADR 0038), ported from
//! Zorite's `updater.rs`.
//!
//! **Detection only**: compare the newest release with `CARGO_PKG_VERSION` and
//! publish [`UpdateState`] for the toolbar dot and Settings → Updates. Nothing
//! is downloaded or installed; builds are unsigned until 1.0. The request is
//! one unauthenticated GET that carries only the app version (User-Agent).
//! Failures are silent: offline behaves like "no update".

use std::time::Duration;

use serde::Deserialize;

const REPOSITORY: &str = "lawkitt/mdoc";

/// The latest check's outcome, a gpui global read by render code.
#[derive(Clone, Debug, Default)]
pub struct UpdateState {
    pub checking: bool,
    /// A check has succeeded at least once this session.
    pub checked: bool,
    /// `Some` when a newer release exists.
    pub available: Option<UpdateAvailable>,
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
}

pub fn state(cx: &gpui::App) -> UpdateState {
    cx.try_global::<UpdateState>().cloned().unwrap_or_default()
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

/// Check in the background, then publish the result and repaint. A check
/// already in flight makes this a no-op.
pub fn spawn_check(opted_in: bool, cx: &mut gpui::App) {
    let mut state = state(cx);
    if state.checking {
        return;
    }
    state.checking = true;
    cx.set_global(state);
    cx.refresh_windows();
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
                state.available = newest(releases, &current, include);
            }
            cx.set_global(state);
            cx.refresh_windows();
        });
    })
    .detach();
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
    (version > *current).then(|| UpdateAvailable {
        version: version.to_string(),
        html_url: release.html_url,
        notes: release.body.unwrap_or_default(),
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
        }
    }
    fn newest_of(releases: Vec<Release>, current: &str, include: bool) -> Option<String> {
        newest(releases, &semver::Version::parse(current).unwrap(), include).map(|a| a.version)
    }

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
}
