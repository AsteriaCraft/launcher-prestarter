//! The signed wrapper policy for the copy mode (ADR 0001): fetch, verify, keep, and decide what it means for
//! this wrapper. Never blocks a launch, except when this wrapper is below the signed minimum version.

pub mod model;
pub mod verify;

use std::time::Duration;

use chrono::{DateTime, Utc};
use reqwest::Url;
use reqwest::blocking::Client;
use semver::Version;
use thiserror::Error;

pub use model::Policy;
pub use verify::{VerifiedPolicy, VerifyError};

use crate::store::state::PolicyRecord;

pub const MANIFEST_FILE: &str = "prestarter-release.json";
pub const SIGNATURE_FILE: &str = "prestarter-release.json.sig";
pub const POLICY_FILE: &str = "prestarter-policy.json";
pub const REFRESH_HOURS: i64 = 24;
pub const FETCH_TIMEOUT: Duration = Duration::from_secs(3);
const MAX_FILE_BYTES: usize = 64 * 1024;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Verdict {
    Current,
    /// A newer wrapper exists: tell the player (at most once a day), launch anyway.
    NewerAvailable {
        latest: Version,
        page: String,
    },
    /// This wrapper is below the signed minimum: do not launch, send the player to the download page.
    TooOld {
        min: Version,
        page: String,
    },
}

pub fn verdict(own: &Version, policy: &Policy) -> Verdict {
    // A release candidate of version X counts as X for the minimum: 0.3.0-rc.1 is not "older than 0.3.0" there,
    // or every rc wrapper would refuse to start under the policy of its own final release.
    let own_core = Version::new(own.major, own.minor, own.patch);
    if let Ok(min) = policy.min_version()
        && own_core < min
    {
        return Verdict::TooOld { min, page: policy.download_page.clone() };
    }
    match policy.latest_version() {
        Ok(Some(latest)) if *own < latest => Verdict::NewerAvailable { latest, page: policy.download_page.clone() },
        _ => Verdict::Current,
    }
}

/// The policy in force: the stored verified one, else the compiled-in one.
pub fn effective(stored: Option<&PolicyRecord>) -> Policy {
    stored.and_then(|r| Policy::parse(&r.policy_json).ok()).unwrap_or_else(Policy::builtin)
}

pub fn refresh_due(checked_at: Option<DateTime<Utc>>, now: DateTime<Utc>) -> bool {
    checked_at.is_none_or(|at| at > now || now.signed_duration_since(at) >= chrono::Duration::hours(REFRESH_HOURS))
}

/// Anti-rollback: a policy from an older release than the stored one is ignored.
pub fn accept(stored: Option<&PolicyRecord>, fresh: &VerifiedPolicy) -> bool {
    match stored.and_then(|r| Version::parse(&r.release_version).ok()) {
        Some(stored_version) => fresh.release_version >= stored_version,
        None => true,
    }
}

/// `https://launcher.asterium.pro/downloads/` for a jar at `https://launcher.asterium.pro/Asterium.jar`: the
/// LaunchServer mirrors the signed files next to its other downloads.
pub fn base_for(jar_url: &Url) -> Url {
    let mut base = jar_url.clone();
    base.set_path("/downloads/");
    base.set_query(None);
    base.set_fragment(None);
    base
}

#[derive(Debug, Error)]
pub enum FetchError {
    #[error("cannot fetch {0}: {1}")]
    Network(String, reqwest::Error),
    #[error("{0} answered HTTP {1}")]
    Status(String, u16),
    #[error("{0} is larger than allowed")]
    TooLarge(String),
    #[error(transparent)]
    Verify(#[from] VerifyError),
}

fn get(client: &Client, url: Url) -> Result<Vec<u8>, FetchError> {
    let shown = url.to_string();
    let response = client.get(url).timeout(FETCH_TIMEOUT).send().map_err(|e| FetchError::Network(shown.clone(), e))?;
    if !response.status().is_success() {
        return Err(FetchError::Status(shown, response.status().as_u16()));
    }
    if response.content_length().is_some_and(|n| n as usize > MAX_FILE_BYTES) {
        return Err(FetchError::TooLarge(shown));
    }
    let bytes = response.bytes().map_err(|e| FetchError::Network(shown.clone(), e))?;
    if bytes.len() > MAX_FILE_BYTES {
        return Err(FetchError::TooLarge(shown));
    }
    Ok(bytes.to_vec())
}

/// Fetches and verifies the three files from `base`.
pub fn fetch(client: &Client, base: &Url, keys: &[[u8; 32]]) -> Result<VerifiedPolicy, FetchError> {
    let join = |name: &str| base.join(name).expect("constant file names join");
    let manifest = get(client, join(MANIFEST_FILE))?;
    let signature = get(client, join(SIGNATURE_FILE))?;
    let policy = get(client, join(POLICY_FILE))?;
    Ok(verify::verify(&manifest, &signature, &policy, keys)?)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn policy(min: &str, latest: Option<&str>) -> Policy {
        Policy { min_wrapper_version: min.into(), latest_wrapper_version: latest.map(Into::into), ..Policy::builtin() }
    }

    fn record(version: &str) -> PolicyRecord {
        PolicyRecord { release_version: version.into(), policy_json: String::new(), fetched_at: Utc::now() }
    }

    #[test]
    fn verdicts() {
        let own = Version::new(0, 3, 0);
        assert_eq!(verdict(&own, &policy("0.3.0", None)), Verdict::Current);
        assert_eq!(verdict(&own, &policy("0.3.0", Some("0.3.0"))), Verdict::Current);
        assert!(matches!(verdict(&own, &policy("0.3.0", Some("0.4.0"))), Verdict::NewerAvailable { .. }));
        assert!(matches!(verdict(&own, &policy("0.3.1", Some("0.4.0"))), Verdict::TooOld { .. }));
        let rc = Version::parse("0.3.0-rc.1").unwrap();
        assert_eq!(verdict(&rc, &policy("0.3.0", None)), Verdict::Current, "an rc meets its own minimum");
        assert!(matches!(verdict(&rc, &policy("0.3.0", Some("0.3.0"))), Verdict::NewerAvailable { .. }));
    }

    #[test]
    fn anti_rollback() {
        let fresh = |v: &str| VerifiedPolicy {
            release_version: Version::parse(v).unwrap(),
            policy: Policy::builtin(),
            policy_json: String::new(),
        };
        assert!(accept(None, &fresh("0.3.0")));
        assert!(accept(Some(&record("0.3.0")), &fresh("0.3.0")));
        assert!(accept(Some(&record("0.3.0")), &fresh("0.3.2")));
        assert!(!accept(Some(&record("0.3.2")), &fresh("0.3.1")));
    }

    #[test]
    fn effective_policy_falls_back_to_builtin() {
        assert_eq!(effective(None), Policy::builtin());
        assert_eq!(effective(Some(&record("0.3.0"))), Policy::builtin(), "an unreadable stored policy is ignored");
        let stored = PolicyRecord {
            release_version: "0.3.1".into(),
            policy_json: String::from_utf8(verify::tests::policy_bytes("0.3.1", "0.3.0")).unwrap(),
            fetched_at: Utc::now(),
        };
        assert_eq!(effective(Some(&stored)).latest_wrapper_version.as_deref(), Some("0.3.1"));
    }

    #[test]
    fn daily_refresh() {
        let now: DateTime<Utc> = "2026-10-10T12:00:00Z".parse().unwrap();
        assert!(refresh_due(None, now));
        assert!(!refresh_due(Some("2026-10-10T00:00:00Z".parse().unwrap()), now));
        assert!(refresh_due(Some("2026-10-09T11:00:00Z".parse().unwrap()), now));
    }

    #[test]
    fn base_url() {
        let jar = Url::parse("https://launcher.asterium.pro/Asterium.jar?x=1").unwrap();
        assert_eq!(base_for(&jar).as_str(), "https://launcher.asterium.pro/downloads/");
        assert_eq!(
            base_for(&jar).join(POLICY_FILE).unwrap().as_str(),
            "https://launcher.asterium.pro/downloads/prestarter-policy.json"
        );
        let local = Url::parse("http://127.0.0.1:8080/Asterium.jar").unwrap();
        assert_eq!(base_for(&local).as_str(), "http://127.0.0.1:8080/downloads/");
    }
}
