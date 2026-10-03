//! `state.json` schema 1: what is installed and when things were last checked. No personal data: directory names,
//! versions, hashes, URLs of our own servers and timestamps only.

use std::fs;
use std::io;
use std::path::Path;

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

use super::atomic;

pub const SCHEMA: u32 = 1;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum JreSource {
    /// Chosen from the Liberica API and checked against its size and sha1.
    Api,
    /// The built-in table (`jre/fallback.json`), checked against its sha256.
    Fallback,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct JreRecord {
    /// Directory under `<store>/jre`.
    pub dir: String,
    /// Liberica version, for example `25.0.4.1+1`.
    pub version: String,
    pub feature: u32,
    /// Catalog key, for example `windows-x64`.
    pub target: String,
    pub source: JreSource,
    pub archive_sha256: String,
    pub installed_at: DateTime<Utc>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct JarRecord {
    pub url: String,
    pub sha256: String,
    pub size: u64,
    pub fetched_at: DateTime<Utc>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PolicyRecord {
    /// Version of the prestarter release whose signed manifest carried this policy (anti-rollback).
    pub release_version: String,
    /// The verified `prestarter-policy.json`, exactly as it was signed.
    pub policy_json: String,
    pub fetched_at: DateTime<Utc>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct State {
    pub schema: u32,
    #[serde(default)]
    pub jre: Option<JreRecord>,
    /// The JRE that was current before `jre`; kept on disk until the next successful launch with `jre`.
    #[serde(default)]
    pub previous_jre: Option<String>,
    #[serde(default)]
    pub jre_checked_at: Option<DateTime<Utc>>,
    #[serde(default)]
    pub jar: Option<JarRecord>,
    #[serde(default)]
    pub policy: Option<PolicyRecord>,
    #[serde(default)]
    pub policy_checked_at: Option<DateTime<Utc>>,
    /// When the "a newer Asterium is available" notice was last shown (at most once a day).
    #[serde(default)]
    pub wrapper_notice_at: Option<DateTime<Utc>>,
}

impl Default for State {
    fn default() -> Self {
        Self {
            schema: SCHEMA,
            jre: None,
            previous_jre: None,
            jre_checked_at: None,
            jar: None,
            policy: None,
            policy_checked_at: None,
            wrapper_notice_at: None,
        }
    }
}

#[derive(Debug, PartialEq, Eq)]
pub enum Loaded {
    Fresh,
    Read,
    /// The file was unreadable or of another schema; it was moved to `state.json.bad` and a fresh state is used.
    Reset(String),
}

impl State {
    pub fn load(path: &Path) -> io::Result<(State, Loaded)> {
        let bytes = match fs::read(path) {
            Ok(bytes) => bytes,
            Err(err) if err.kind() == io::ErrorKind::NotFound => return Ok((State::default(), Loaded::Fresh)),
            Err(err) => return Err(err),
        };
        let reason = match serde_json::from_slice::<State>(&bytes) {
            Ok(state) if state.schema == SCHEMA => return Ok((state, Loaded::Read)),
            Ok(state) => format!("unsupported schema {}", state.schema),
            Err(err) => format!("unreadable: {err}"),
        };
        let _ = fs::rename(path, path.with_extension("json.bad"));
        Ok((State::default(), Loaded::Reset(reason)))
    }

    pub fn save(&self, path: &Path) -> io::Result<()> {
        let mut json = serde_json::to_vec_pretty(self).map_err(io::Error::other)?;
        json.push(b'\n');
        atomic::write(path, &json)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    fn temp_file(tag: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!("asterium-state-{tag}-{}", atomic::unique_suffix()));
        fs::create_dir_all(&dir).unwrap();
        dir.join("state.json")
    }

    fn sample() -> State {
        State {
            jre: Some(JreRecord {
                dir: "liberica-25-windows-x64-25.0.4.1+1".into(),
                version: "25.0.4.1+1".into(),
                feature: 25,
                target: "windows-x64".into(),
                source: JreSource::Api,
                archive_sha256: "a".repeat(64),
                installed_at: "2026-10-02T10:00:00Z".parse().unwrap(),
            }),
            jre_checked_at: Some("2026-10-02T10:00:00Z".parse().unwrap()),
            ..State::default()
        }
    }

    #[test]
    fn missing_file_is_a_fresh_state() {
        let path = temp_file("missing");
        assert_eq!(State::load(&path).unwrap(), (State::default(), Loaded::Fresh));
    }

    #[test]
    fn round_trips() {
        let path = temp_file("rt");
        sample().save(&path).unwrap();
        assert_eq!(State::load(&path).unwrap(), (sample(), Loaded::Read));
        let text = fs::read_to_string(&path).unwrap();
        assert!(text.contains("\"schema\": 1"));
        assert!(text.contains("\"jreCheckedAt\""));
    }

    #[test]
    fn a_corrupt_file_is_set_aside() {
        let path = temp_file("bad");
        fs::write(&path, b"{not json").unwrap();
        let (state, loaded) = State::load(&path).unwrap();
        assert_eq!(state, State::default());
        assert!(matches!(loaded, Loaded::Reset(_)));
        assert!(path.with_extension("json.bad").exists());
        assert!(!path.exists());
    }

    #[test]
    fn another_schema_is_set_aside() {
        let path = temp_file("schema");
        fs::write(&path, br#"{"schema": 2}"#).unwrap();
        let (_, loaded) = State::load(&path).unwrap();
        assert_eq!(loaded, Loaded::Reset("unsupported schema 2".into()));
    }

    #[test]
    fn older_files_without_optional_fields_load() {
        let path = temp_file("min");
        fs::write(&path, br#"{"schema": 1}"#).unwrap();
        assert_eq!(State::load(&path).unwrap(), (State::default(), Loaded::Read));
    }
}
