//! Java for the launcher: which JRE, from where, checked how, installed where (ADR 0004, 0005).

pub mod api;
pub mod catalog;
pub mod extract;
pub mod fallback;
pub mod install;
pub mod layout;
pub mod version;

use chrono::{DateTime, Duration, Utc};

use crate::store::state::JreRecord;
use version::JavaVersion;

/// The Java feature version compiled in (the signed policy can change it for the copy mode, ADR 0001).
pub const DEFAULT_FEATURE: u32 = 25;
/// How often the prestarter asks the API whether a newer JRE exists.
pub const UPDATE_CHECK_INTERVAL_DAYS: i64 = 7;

/// Whether the weekly JRE update check is due.
pub fn update_check_due(checked_at: Option<DateTime<Utc>>, now: DateTime<Utc>) -> bool {
    match checked_at {
        None => true,
        Some(at) => now.signed_duration_since(at) >= Duration::days(UPDATE_CHECK_INTERVAL_DAYS) || at > now,
    }
}

/// Whether `candidate` should replace the installed JRE: a newer version of the same feature, or another feature
/// (the policy moved to a new Java), or an installed record that cannot be parsed.
pub fn is_upgrade(installed: &JreRecord, candidate: &JavaVersion) -> bool {
    match JavaVersion::parse(&installed.version) {
        Some(current) => current.feature() != candidate.feature() || *candidate > current,
        None => true,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::store::state::JreSource;

    fn record(version: &str) -> JreRecord {
        JreRecord {
            dir: "d".into(),
            version: version.into(),
            feature: 25,
            target: "linux-x64".into(),
            source: JreSource::Api,
            archive_sha256: String::new(),
            installed_at: Utc::now(),
        }
    }

    #[test]
    fn weekly_check() {
        let now: DateTime<Utc> = "2026-10-10T12:00:00Z".parse().unwrap();
        assert!(update_check_due(None, now));
        assert!(!update_check_due(Some("2026-10-04T12:00:01Z".parse().unwrap()), now));
        assert!(update_check_due(Some("2026-10-03T12:00:00Z".parse().unwrap()), now));
        assert!(update_check_due(Some("2027-01-01T00:00:00Z".parse().unwrap()), now), "a clock that went back");
    }

    #[test]
    fn upgrades() {
        let v = |t| JavaVersion::parse(t).unwrap();
        assert!(is_upgrade(&record("25.0.4+9"), &v("25.0.4.1+1")));
        assert!(!is_upgrade(&record("25.0.4.1+1"), &v("25.0.4.1+1")));
        assert!(!is_upgrade(&record("25.0.4.1+1"), &v("25.0.4+9")));
        assert!(is_upgrade(&record("25.0.4.1+1"), &v("26+1")));
        assert!(is_upgrade(&record("garbage"), &v("25+1")));
    }
}
