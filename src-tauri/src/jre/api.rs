//! The Liberica API (`api.bell-sw.com`): which JRE to install, with the size and sha1 to check it against.
//!
//! Only GA, JavaFX, `jre-full` records of the requested OS, CPU and package are taken; of those, the highest version
//! wins (the API currently lists both a CSPU `25.0.4.1+1` and a PSU `25.0.4+9`).

use std::time::Duration;

use reqwest::Url;
use reqwest::blocking::Client;
use serde::Deserialize;
use thiserror::Error;

use super::catalog::JreTarget;
use super::version::JavaVersion;
use crate::store::state::JreSource;

pub const DEFAULT_API: &str = "https://api.bell-sw.com/v1/liberica/releases";
/// The answer is a few kilobytes; anything near this is not the API.
const MAX_RESPONSE_BYTES: usize = 1024 * 1024;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Checksum {
    Sha1(String),
    Sha256(String),
}

/// A JRE archive to install, from the API or from the fallback table.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct JreRelease {
    pub version: JavaVersion,
    pub version_text: String,
    pub url: Url,
    pub filename: String,
    pub size: u64,
    pub checksum: Checksum,
    pub source: JreSource,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
struct ApiRecord {
    download_url: String,
    filename: String,
    size: u64,
    sha1: String,
    version: String,
    feature_version: u32,
    #[serde(rename = "GA")]
    ga: bool,
    #[serde(rename = "FX")]
    fx: bool,
    bundle_type: String,
    package_type: String,
    os: String,
    architecture: String,
    bitness: u32,
}

#[derive(Debug, Error)]
pub enum ApiError {
    #[error("Liberica API request failed: {0}")]
    Network(#[from] reqwest::Error),
    #[error("Liberica API answered HTTP {0}")]
    Status(u16),
    #[error("Liberica API answer is not the expected JSON: {0}")]
    Json(#[from] serde_json::Error),
    #[error("Liberica API answer is too large")]
    TooLarge,
    #[error("Liberica API lists no GA jre-full with JavaFX for {0}")]
    NoMatch(String),
}

pub fn query_url(base: &Url, target: &JreTarget, feature: u32) -> Url {
    let mut url = base.clone();
    url.query_pairs_mut()
        .append_pair("version-feature", &feature.to_string())
        .append_pair("version-modifier", "latest")
        .append_pair("bitness", "64")
        .append_pair("os", target.api_os)
        .append_pair("arch", target.api_arch)
        .append_pair("package-type", target.package.api_name())
        .append_pair("bundle-type", "jre-full");
    url
}

fn is_safe_filename(name: &str) -> bool {
    !name.is_empty()
        && name.len() <= 200
        && !name.starts_with('.')
        && name.chars().all(|c| c.is_ascii_alphanumeric() || "._+-".contains(c))
}

/// Picks the record to install from the API's answer (pure: tested on the recorded answers in `tests/fixtures`).
pub fn select(body: &[u8], target: &JreTarget, feature: u32) -> Result<JreRelease, ApiError> {
    let records: Vec<ApiRecord> = serde_json::from_slice(body)?;
    records
        .into_iter()
        .filter(|r| {
            r.ga && r.fx
                && r.bundle_type == "jre-full"
                && r.os == target.api_os
                && r.architecture == target.api_arch
                && r.package_type == target.package.api_name()
                && r.bitness == 64
                && r.feature_version == feature
                && r.sha1.len() == 40
                && r.sha1.bytes().all(|b| b.is_ascii_hexdigit())
                && r.size > 0
                && is_safe_filename(&r.filename)
        })
        .filter_map(|r| {
            let version = JavaVersion::parse(&r.version)?;
            let url = Url::parse(&r.download_url).ok()?;
            (version.feature() == feature && super::super::net::client::is_allowed_scheme(&url)).then(|| JreRelease {
                version,
                version_text: r.version.clone(),
                url,
                filename: r.filename.clone(),
                size: r.size,
                checksum: Checksum::Sha1(r.sha1.to_ascii_lowercase()),
                source: JreSource::Api,
            })
        })
        .max_by(|a, b| a.version.cmp(&b.version))
        .ok_or_else(|| ApiError::NoMatch(target.key.to_owned()))
}

/// Asks the API; `timeout` bounds the whole request (3 s for the background update check).
pub fn fetch(
    client: &Client,
    base: &Url,
    target: &JreTarget,
    feature: u32,
    timeout: Option<Duration>,
) -> Result<JreRelease, ApiError> {
    let mut request = client.get(query_url(base, target, feature));
    if let Some(timeout) = timeout {
        request = request.timeout(timeout);
    }
    let response = request.send()?;
    if !response.status().is_success() {
        return Err(ApiError::Status(response.status().as_u16()));
    }
    if response.content_length().is_some_and(|len| len as usize > MAX_RESPONSE_BYTES) {
        return Err(ApiError::TooLarge);
    }
    let body = response.bytes()?;
    if body.len() > MAX_RESPONSE_BYTES {
        return Err(ApiError::TooLarge);
    }
    select(&body, target, feature)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::jre::catalog::for_host;
    use crate::platform::{Arch, Host, Os};

    const WINDOWS_X86: &[u8] = include_bytes!("../../tests/fixtures/liberica/windows-x86.json");
    const LINUX_ARM: &[u8] = include_bytes!("../../tests/fixtures/liberica/linux-arm.json");
    const WINDOWS_ARM: &[u8] = include_bytes!("../../tests/fixtures/liberica/windows-arm.json");

    fn target(os: Os, arch: Arch, emulation: bool) -> JreTarget {
        for_host(Host { os, arch }, emulation)
    }

    #[test]
    fn picks_the_cspu_over_the_psu() {
        let r = select(WINDOWS_X86, &target(Os::Windows, Arch::X86_64, true), 25).unwrap();
        assert_eq!(r.version_text, "25.0.4.1+1");
        assert_eq!(r.filename, "bellsoft-jre25.0.4.1+1-windows-amd64-full.zip");
        assert_eq!(r.size, 120_037_237);
        assert_eq!(r.checksum, Checksum::Sha1("f42fb6d8ca801d7b9abb8d86d52031247d46c41b".into()));
        assert_eq!(r.url.host_str(), Some("github.com"));
    }

    #[test]
    fn linux_arm_uses_arch_arm() {
        let r = select(LINUX_ARM, &target(Os::Linux, Arch::Aarch64, false), 25).unwrap();
        assert_eq!(r.filename, "bellsoft-jre25.0.4.1+1-linux-aarch64-full.tar.gz");
    }

    #[test]
    fn a_record_of_another_target_is_never_taken() {
        // The Windows ARM answer for an x64 target: nothing matches.
        let err = select(WINDOWS_ARM, &target(Os::Windows, Arch::X86_64, true), 25).unwrap_err();
        assert!(matches!(err, ApiError::NoMatch(_)));
        // And the wrong feature version.
        assert!(select(WINDOWS_X86, &target(Os::Windows, Arch::X86_64, true), 26).is_err());
    }

    #[test]
    fn filters_non_ga_non_fx_and_bad_fields() {
        let body = br#"[
          {"downloadUrl":"https://x/a.zip","filename":"a.zip","size":1,"sha1":"0000000000000000000000000000000000000000","version":"25.0.9+1","featureVersion":25,"GA":false,"FX":true,"bundleType":"jre-full","packageType":"zip","os":"windows","architecture":"x86","bitness":64},
          {"downloadUrl":"https://x/b.zip","filename":"b.zip","size":1,"sha1":"0000000000000000000000000000000000000000","version":"25.0.8+1","featureVersion":25,"GA":true,"FX":false,"bundleType":"jre-full","packageType":"zip","os":"windows","architecture":"x86","bitness":64},
          {"downloadUrl":"https://x/c.zip","filename":"c.zip","size":1,"sha1":"0000000000000000000000000000000000000000","version":"25.0.7+1","featureVersion":25,"GA":true,"FX":true,"bundleType":"jre","packageType":"zip","os":"windows","architecture":"x86","bitness":64},
          {"downloadUrl":"http://x/d.zip","filename":"d.zip","size":1,"sha1":"0000000000000000000000000000000000000000","version":"25.0.6+1","featureVersion":25,"GA":true,"FX":true,"bundleType":"jre-full","packageType":"zip","os":"windows","architecture":"x86","bitness":64},
          {"downloadUrl":"https://x/e.zip","filename":"../e.zip","size":1,"sha1":"0000000000000000000000000000000000000000","version":"25.0.5+1","featureVersion":25,"GA":true,"FX":true,"bundleType":"jre-full","packageType":"zip","os":"windows","architecture":"x86","bitness":64},
          {"downloadUrl":"https://x/f.zip","filename":"f.zip","size":1,"sha1":"short","version":"25.0.4+1","featureVersion":25,"GA":true,"FX":true,"bundleType":"jre-full","packageType":"zip","os":"windows","architecture":"x86","bitness":64},
          {"downloadUrl":"https://x/g.zip","filename":"g.zip","size":7,"sha1":"ABCDEF0000000000000000000000000000000000","version":"25.0.1+1","featureVersion":25,"GA":true,"FX":true,"bundleType":"jre-full","packageType":"zip","os":"windows","architecture":"x86","bitness":64}
        ]"#;
        let r = select(body, &target(Os::Windows, Arch::X86_64, true), 25).unwrap();
        assert_eq!(r.filename, "g.zip");
        assert_eq!(r.checksum, Checksum::Sha1("abcdef0000000000000000000000000000000000".into()));
    }

    #[test]
    fn not_json_is_an_error() {
        assert!(matches!(select(b"<html>", &target(Os::Linux, Arch::X86_64, true), 25), Err(ApiError::Json(_))));
    }

    #[test]
    fn query() {
        let base = Url::parse(DEFAULT_API).unwrap();
        let url = query_url(&base, &target(Os::Linux, Arch::Aarch64, false), 25);
        assert_eq!(
            url.as_str(),
            "https://api.bell-sw.com/v1/liberica/releases?version-feature=25&version-modifier=latest&bitness=64&os=linux&arch=arm&package-type=tar.gz&bundle-type=jre-full"
        );
    }
}
