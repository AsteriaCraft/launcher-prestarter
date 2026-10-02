//! `prestarter-policy.json` schema 1 (ADR 0001): what a copy-mode wrapper (AppImage, macOS) may not hard-code
//! forever. The repository file `policy/prestarter-policy.json` is compiled in as the default; each release
//! publishes it with `latestWrapperVersion` set to the release's own version, covered by the release signature.

use reqwest::Url;
use semver::Version;
use serde::{Deserialize, Serialize};
use thiserror::Error;

const BUILTIN: &str = include_str!("../../../policy/prestarter-policy.json");

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Policy {
    pub schema: u32,
    /// Where the copy mode fetches the launcher jar (https only).
    pub jar_url: String,
    /// The Java feature version to install.
    pub java_feature: u32,
    /// Wrappers below this refuse to start the launcher and send the player to `download_page`.
    pub min_wrapper_version: String,
    /// The newest wrapper; older ones show a non-blocking notice. Absent in the repository file.
    #[serde(default)]
    pub latest_wrapper_version: Option<String>,
    pub download_page: String,
}

#[derive(Debug, Error, PartialEq, Eq)]
pub enum PolicyError {
    #[error("policy: {0}")]
    Invalid(String),
}

impl Policy {
    pub fn parse(text: &str) -> Result<Self, PolicyError> {
        let policy: Policy = serde_json::from_str(text).map_err(|e| PolicyError::Invalid(e.to_string()))?;
        policy.validate()?;
        Ok(policy)
    }

    pub fn validate(&self) -> Result<(), PolicyError> {
        let invalid = |m: String| Err(PolicyError::Invalid(m));
        if self.schema != 1 {
            return invalid(format!("unsupported schema {}", self.schema));
        }
        match Url::parse(&self.jar_url) {
            Ok(url) if url.scheme() == "https" && url.host_str().is_some() => {}
            _ => return invalid(format!("jarUrl must be an https URL: {}", self.jar_url)),
        }
        match Url::parse(&self.download_page) {
            Ok(url) if url.scheme() == "https" => {}
            _ => return invalid(format!("downloadPage must be an https URL: {}", self.download_page)),
        }
        if !(17..=99).contains(&self.java_feature) {
            return invalid(format!("javaFeature {} is out of range", self.java_feature));
        }
        self.min_version()?;
        self.latest_version()?;
        Ok(())
    }

    pub fn min_version(&self) -> Result<Version, PolicyError> {
        Version::parse(&self.min_wrapper_version).map_err(|e| PolicyError::Invalid(format!("minWrapperVersion: {e}")))
    }

    pub fn latest_version(&self) -> Result<Option<Version>, PolicyError> {
        self.latest_wrapper_version
            .as_deref()
            .map(|v| Version::parse(v).map_err(|e| PolicyError::Invalid(format!("latestWrapperVersion: {e}"))))
            .transpose()
    }

    pub fn jar_url(&self) -> Url {
        Url::parse(&self.jar_url).expect("validated")
    }

    /// The compiled-in policy (the repository file).
    pub fn builtin() -> Self {
        Policy::parse(BUILTIN).expect("policy/prestarter-policy.json is valid (unit-tested)")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_repository_file_is_valid() {
        let policy = Policy::builtin();
        assert_eq!(policy.jar_url, crate::jar::DEFAULT_JAR_URL);
        assert_eq!(policy.java_feature, crate::jre::DEFAULT_FEATURE);
        assert_eq!(policy.latest_wrapper_version, None, "CI adds it per release");
        assert!(policy.min_version().unwrap() <= Version::parse(env!("CARGO_PKG_VERSION")).unwrap());
    }

    #[test]
    fn refuses_bad_values() {
        let good = Policy::builtin();
        let cases: Vec<(&str, Policy)> = vec![
            ("schema", Policy { schema: 2, ..good.clone() }),
            ("http jar", Policy { jar_url: "http://launcher.asterium.pro/Asterium.jar".into(), ..good.clone() }),
            ("not a url", Policy { jar_url: "Asterium.jar".into(), ..good.clone() }),
            ("page", Policy { download_page: "ftp://x".into(), ..good.clone() }),
            ("java", Policy { java_feature: 8, ..good.clone() }),
            ("min", Policy { min_wrapper_version: "0.3".into(), ..good.clone() }),
            ("latest", Policy { latest_wrapper_version: Some("x".into()), ..good.clone() }),
        ];
        for (label, policy) in cases {
            assert!(policy.validate().is_err(), "{label}");
        }
    }

    #[test]
    fn unknown_fields_are_ignored_for_forward_compatibility() {
        let text = r#"{"schema":1,"jarUrl":"https://a.example/A.jar","javaFeature":26,"minWrapperVersion":"0.3.0","latestWrapperVersion":"0.4.0","downloadPage":"https://a.example/","future":true}"#;
        let policy = Policy::parse(text).unwrap();
        assert_eq!(policy.java_feature, 26);
        assert_eq!(policy.latest_version().unwrap(), Some(Version::new(0, 4, 0)));
    }
}
