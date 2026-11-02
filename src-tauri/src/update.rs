use std::time::Duration;

use serde::{Deserialize, Serialize};

const RELEASES_LATEST: &str =
    "https://git.nikkosphere.com/api/v1/repos/nicole/AuraMail/releases/latest";

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct UpdateInfo {
    pub version: String,
    pub url: String,
}

#[derive(Debug, Deserialize)]
struct Release {
    tag_name: String,
    html_url: String,
    #[serde(default)]
    draft: bool,
    #[serde(default)]
    prerelease: bool,
}

/// Ask the release server whether a newer stable release exists. Any network
/// or parsing failure counts as "no update": the notice is a convenience, not
/// a reason to surface an error.
pub fn check(current: &str) -> Option<UpdateInfo> {
    let client = reqwest::blocking::Client::builder()
        .timeout(Duration::from_secs(5))
        .user_agent(format!("AuraMail/{current}"))
        .build()
        .ok()?;
    let release: Release = client
        .get(RELEASES_LATEST)
        .send()
        .ok()?
        .error_for_status()
        .ok()?
        .json()
        .ok()?;
    if release.draft || release.prerelease || !is_newer(current, &release.tag_name) {
        return None;
    }
    Some(UpdateInfo {
        version: release.tag_name.trim_start_matches(['v', 'V']).to_string(),
        url: release.html_url,
    })
}

fn parse_version(value: &str) -> Option<(u64, u64, u64)> {
    let trimmed = value.trim().trim_start_matches(['v', 'V']);
    let core = trimmed.split(['-', '+']).next()?;
    let mut parts = core.split('.');
    let major = parts.next()?.parse().ok()?;
    let minor = parts.next().unwrap_or("0").parse().ok()?;
    let patch = parts.next().unwrap_or("0").parse().ok()?;
    Some((major, minor, patch))
}

/// True only for a strictly newer stable release. Pre-release tags are never
/// advertised automatically.
pub fn is_newer(current: &str, candidate: &str) -> bool {
    let candidate = candidate.trim().trim_start_matches(['v', 'V']);
    if candidate.contains('-') {
        return false;
    }
    match (parse_version(current), parse_version(candidate)) {
        (Some(current), Some(candidate)) => candidate > current,
        _ => false,
    }
}

#[cfg(test)]
mod tests {
    use super::is_newer;

    #[test]
    fn compares_each_version_component_numerically() {
        assert!(is_newer("0.6.1", "0.6.2"));
        assert!(is_newer("0.6.9", "0.6.10"));
        assert!(is_newer("0.9.9", "0.10.0"));
        assert!(is_newer("1.0.0", "2.0.0"));
        assert!(!is_newer("0.6.1", "0.6.1"));
        assert!(!is_newer("0.6.2", "0.6.1"));
    }

    #[test]
    fn accepts_a_v_prefix_and_ignores_noise() {
        assert!(is_newer("0.6.1", "v0.6.2"));
        assert!(is_newer("v0.6.1", "0.6.2"));
        assert!(!is_newer("0.6.1", "nightly"));
        assert!(!is_newer("0.6.1", "0.6.2-beta.1"));
    }

    #[test]
    fn omitted_components_count_as_zero() {
        assert!(is_newer("0.6", "0.6.1"));
        assert!(!is_newer("0.6.1", "0.6"));
    }
}
