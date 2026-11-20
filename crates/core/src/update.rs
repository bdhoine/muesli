//! Rules about Muesli's versions and where its releases are.

use crate::REPOSITORY;

/// Where the update information of the latest stable release is. GitHub serves the release marked
/// Latest here, never a pre-release or a draft.
pub const LATEST_JSON: &str = "https://github.com/bdhoine/muesli/releases/latest/download/latest.json";

/// How long the update check waits for GitHub before it gives up, in seconds.
pub const CONNECT_TIMEOUT_SECONDS: u64 = 15;

/// Whether `candidate` is a stable release newer than `current`. Pre-releases, versions that do
/// not parse and anything equal or older are not: Muesli never downgrades.
pub fn is_newer(candidate: &str, current: &str) -> bool {
    let parse = |v: &str| semver::Version::parse(v.trim().trim_start_matches('v')).ok();
    match (parse(candidate), parse(current)) {
        (Some(candidate), Some(current)) => candidate.pre.is_empty() && candidate > current,
        _ => false,
    }
}

/// The release notes of a version on GitHub ("What's new").
pub fn release_notes_url(version: &str) -> String {
    format!("{REPOSITORY}/releases/tag/v{}", version.trim_start_matches('v'))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn only_newer_stable_versions_count() {
        assert!(is_newer("0.2.0", "0.1.0"));
        assert!(is_newer("v0.1.1", "0.1.0"));
        assert!(is_newer("1.0.0", "0.9.9"));
        assert!(!is_newer("0.1.0", "0.1.0"));
        assert!(!is_newer("0.0.9", "0.1.0"), "never downgrade");
        assert!(!is_newer("0.2.0-beta.1", "0.1.0"), "pre-releases are ignored");
        assert!(!is_newer("latest", "0.1.0"));
        assert!(!is_newer("0.2.0", "not a version"));
    }

    #[test]
    fn release_notes_point_at_the_tag() {
        assert_eq!(release_notes_url("0.2.0"), "https://github.com/bdhoine/muesli/releases/tag/v0.2.0");
        assert_eq!(release_notes_url("v0.2.0"), "https://github.com/bdhoine/muesli/releases/tag/v0.2.0");
    }
}
