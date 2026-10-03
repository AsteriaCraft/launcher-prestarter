//! The JRE to install when the Liberica API is down or answers nonsense (ADR 0005): `fallback.json`, compiled in,
//! regenerated weekly by `scripts/update-jre-fallback.sh` (jre-watch.yml) after checking every archive against the
//! API's size and sha1. Here each archive is pinned by sha256, so a fallback install needs no API at all.

use std::collections::BTreeMap;

use reqwest::Url;
use serde::Deserialize;

use super::api::{Checksum, JreRelease};
use super::catalog::JreTarget;
use super::version::JavaVersion;
use crate::store::state::JreSource;

const TABLE: &str = include_str!("fallback.json");

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct Table {
    schema: u32,
    feature_version: u32,
    targets: BTreeMap<String, Entry>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct Entry {
    version: String,
    url: String,
    filename: String,
    size: u64,
    package_type: String,
    sha256: String,
}

fn parse(text: &str) -> Option<Table> {
    serde_json::from_str::<Table>(text).ok().filter(|t| t.schema == 1)
}

/// The pinned archive for `target`, if the table has one for this Java feature version.
pub fn release_for(target: &JreTarget, feature: u32) -> Option<JreRelease> {
    release_from(TABLE, target, feature)
}

fn release_from(text: &str, target: &JreTarget, feature: u32) -> Option<JreRelease> {
    let table = parse(text)?;
    if table.feature_version != feature {
        return None;
    }
    let entry = table.targets.get(target.key)?;
    let version = JavaVersion::parse(&entry.version)?;
    let url = Url::parse(&entry.url).ok()?;
    let valid = url.scheme() == "https"
        && entry.package_type == target.package.api_name()
        && entry.sha256.len() == 64
        && entry.sha256.bytes().all(|b| b.is_ascii_hexdigit())
        && version.feature() == feature
        && url.path().ends_with(&format!("/{}", entry.filename));
    valid.then(|| JreRelease {
        version,
        version_text: entry.version.clone(),
        url,
        filename: entry.filename.clone(),
        size: entry.size,
        checksum: Checksum::Sha256(entry.sha256.to_ascii_lowercase()),
        source: JreSource::Fallback,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::jre::catalog::for_host;
    use crate::platform::{Arch, Host, Os};

    #[test]
    fn the_compiled_table_covers_every_target() {
        let hosts = [
            (Os::Windows, Arch::X86_64, true),
            (Os::Windows, Arch::Aarch64, true),
            (Os::Windows, Arch::Aarch64, false),
            (Os::Linux, Arch::X86_64, true),
            (Os::Linux, Arch::Aarch64, true),
            (Os::MacOs, Arch::X86_64, true),
            (Os::MacOs, Arch::Aarch64, true),
        ];
        for (os, arch, emulation) in hosts {
            let target = for_host(Host { os, arch }, emulation);
            let release = release_for(&target, 25).unwrap_or_else(|| panic!("no fallback for {}", target.key));
            assert!(matches!(release.checksum, Checksum::Sha256(_)));
            assert_eq!(release.source, JreSource::Fallback);
            assert!(release.size > 40_000_000, "{}: {}", target.key, release.size);
            assert!(release.url.as_str().starts_with("https://github.com/bell-sw/Liberica/releases/download/"));
        }
    }

    #[test]
    fn no_fallback_for_another_feature() {
        let target = for_host(Host { os: Os::Linux, arch: Arch::X86_64 }, true);
        assert!(release_for(&target, 26).is_none());
    }

    #[test]
    fn a_broken_entry_is_ignored() {
        let target = for_host(Host { os: Os::Linux, arch: Arch::X86_64 }, true);
        let text = r#"{"schema":1,"featureVersion":25,"targets":{"linux-x64":{"version":"25.0.4+9","url":"http://github.com/x/a.tar.gz","filename":"a.tar.gz","size":1,"packageType":"tar.gz","sha1":"x","sha256":"00"}}}"#;
        assert!(release_from(text, &target, 25).is_none());
        assert!(release_from("{}", &target, 25).is_none());
    }
}
