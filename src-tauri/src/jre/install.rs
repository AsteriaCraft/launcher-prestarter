//! Atomic JRE installation (ADR 0005): free space → download to `tmp/*.part` with size and hash check (one retry)
//! → unpack into `jre/.staging-*` → layout and `java -version` → rename to `jre/liberica-…` → clean up. A running
//! JVM is never overwritten: every version gets its own directory, and old ones are collected later ([`collect`]).
//! The caller holds the install lock.

use std::fs;
use std::io;
use std::path::{Path, PathBuf};
use std::sync::atomic::AtomicBool;

use chrono::Utc;
use log::{info, warn};
use reqwest::blocking::Client;
use thiserror::Error;

use super::api::{Checksum, JreRelease};
use super::catalog::{JreTarget, dir_name};
use super::extract::{self, ExtractError, JRE_LIMITS};
use super::layout::{self, LayoutError};
use crate::net::download::{DownloadError, download_to};
use crate::platform::{self, Os};
use crate::store::StorePaths;
use crate::store::atomic::unique_suffix;
use crate::store::state::JreRecord;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum InstallStage {
    Download,
    Unpack,
    Verify,
}

pub trait InstallObserver {
    fn stage(&mut self, stage: InstallStage);
    fn progress(&mut self, done: u64, total: u64);
}

#[derive(Debug, Error)]
pub enum InstallError {
    #[error("not enough disk space in {path}: {needed} bytes needed, {available} available")]
    NoSpace { needed: u64, available: u64, path: PathBuf },
    #[error(transparent)]
    Download(#[from] DownloadError),
    #[error("downloaded Java does not match: {0}")]
    Integrity(String),
    #[error(transparent)]
    Extract(#[from] ExtractError),
    #[error(transparent)]
    Layout(#[from] LayoutError),
    #[error("installation was cancelled")]
    Cancelled,
    #[error(transparent)]
    Io(#[from] io::Error),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct InstalledJre {
    pub home: PathBuf,
    pub record: JreRecord,
}

/// Archive + unpacked JRE + slack: the archive is ~120-150 MB and unpacks to ~3x that.
pub fn space_needed(archive_size: u64) -> u64 {
    archive_size.saturating_mul(4)
}

/// Removes its paths when dropped (the staging directory and the partial download), whatever happened.
struct Cleanup(Vec<PathBuf>);

impl Drop for Cleanup {
    fn drop(&mut self) {
        for path in &self.0 {
            let result = if path.is_dir() { fs::remove_dir_all(path) } else { fs::remove_file(path) };
            if let Err(err) = result
                && err.kind() != io::ErrorKind::NotFound
            {
                warn!("cannot remove {}: {err}", path.display());
            }
        }
    }
}

fn verify(release: &JreRelease, size: u64, sha1: &str, sha256: &str) -> Result<(), InstallError> {
    if size != release.size {
        return Err(InstallError::Integrity(format!("{size} bytes, expected {}", release.size)));
    }
    let (kind, expected, actual) = match &release.checksum {
        Checksum::Sha1(expected) => ("sha1", expected, sha1),
        Checksum::Sha256(expected) => ("sha256", expected, sha256),
    };
    if !expected.eq_ignore_ascii_case(actual) {
        return Err(InstallError::Integrity(format!("{kind} {actual}, expected {expected}")));
    }
    Ok(())
}

#[allow(clippy::too_many_arguments)]
pub fn install(
    paths: &StorePaths,
    client: &Client,
    release: &JreRelease,
    target: &JreTarget,
    feature: u32,
    os: Os,
    cancel: &AtomicBool,
    observer: &mut dyn InstallObserver,
) -> Result<InstalledJre, InstallError> {
    let needed = space_needed(release.size);
    fs::create_dir_all(paths.jre_dir())?;
    fs::create_dir_all(paths.tmp_dir())?;
    let available = platform::free_space(paths.root())?;
    if available < needed {
        return Err(InstallError::NoSpace { needed, available, path: paths.root().to_path_buf() });
    }

    let part = paths.tmp_dir().join(format!("{}.part", release.filename));
    let staging = paths.jre_dir().join(format!(".staging-{}", unique_suffix()));
    let _cleanup = Cleanup(vec![part.clone(), staging.clone()]);

    observer.stage(InstallStage::Download);
    let mut attempt = 0;
    let downloaded = loop {
        attempt += 1;
        let result = download_to(client, &release.url, &part, release.size, cancel, &mut |done, _| {
            observer.progress(done, release.size)
        })
        .map_err(InstallError::from)
        .and_then(|d| verify(release, d.size, &d.sha1, &d.sha256).map(|()| d));
        match result {
            Ok(downloaded) => break downloaded,
            Err(InstallError::Download(DownloadError::Cancelled(_))) => return Err(InstallError::Cancelled),
            Err(err) if attempt == 1 && retryable(&err) => {
                warn!("JRE download attempt 1 failed, retrying once: {err}");
            }
            Err(err) => return Err(err),
        }
    };
    info!(
        "JRE {} downloaded and verified ({} bytes, sha256 {})",
        release.version_text, downloaded.size, downloaded.sha256
    );

    observer.stage(InstallStage::Unpack);
    let root = extract::extract(&part, target.package, &staging, &JRE_LIMITS, cancel, &mut |done, total| {
        observer.progress(done, total)
    })
    .map_err(|err| match err {
        ExtractError::Cancelled => InstallError::Cancelled,
        other => InstallError::Extract(other),
    })?;

    observer.stage(InstallStage::Verify);
    layout::check_files(&root, os, feature)?;
    let banner = layout::run_java_version(&root, os, layout::JAVA_VERSION_TIMEOUT)?;
    info!("java -version: {banner}");

    let dir = dir_name(target, feature, &release.version_text);
    let home = paths.jre_home(&dir);
    if home.exists() {
        let trash = paths.jre_dir().join(format!(".trash-{}", unique_suffix()));
        fs::rename(&home, &trash)?;
        let _ = fs::remove_dir_all(&trash);
    }
    fs::rename(&root, &home)?;

    #[cfg(target_os = "macos")]
    match crate::platform::macos::strip_quarantine(&home) {
        Ok(0) => info!("no quarantine attribute on the JRE"),
        Ok(n) => info!("removed com.apple.quarantine from {n} JRE files"),
        Err(err) => warn!("cannot strip quarantine from the JRE: {err}"),
    }

    let record = JreRecord {
        dir,
        version: release.version_text.clone(),
        feature,
        target: target.key.to_owned(),
        source: release.source,
        archive_sha256: downloaded.sha256,
        installed_at: Utc::now(),
    };
    Ok(InstalledJre { home, record })
}

fn retryable(err: &InstallError) -> bool {
    match err {
        InstallError::Download(d) => d.is_transient(),
        InstallError::Integrity(_) => true,
        _ => false,
    }
}

/// Removes JRE directories other than `keep`, and leftovers of interrupted installs. A directory still in use
/// (Windows keeps a running JVM's files locked) stays until a later start. Returns how many were removed.
pub fn collect(jre_dir: &Path, keep: &[&str]) -> usize {
    let Ok(entries) = fs::read_dir(jre_dir) else { return 0 };
    let mut removed = 0;
    for entry in entries.flatten() {
        let name = entry.file_name().to_string_lossy().into_owned();
        let stale = name.starts_with(".staging-")
            || name.starts_with(".trash-")
            || (name.starts_with("liberica-") && !keep.contains(&name.as_str()));
        if !stale {
            continue;
        }
        match fs::remove_dir_all(entry.path()) {
            Ok(()) => {
                removed += 1;
                info!("removed old JRE directory {name}");
            }
            Err(err) => warn!("cannot remove {name} yet: {err}"),
        }
    }
    removed
}

/// Deletes everything in `tmp/` (partial downloads of an interrupted run). Call under the install lock.
pub fn clean_tmp(tmp_dir: &Path) {
    if let Ok(entries) = fs::read_dir(tmp_dir) {
        for entry in entries.flatten() {
            let path = entry.path();
            let _ = if path.is_dir() { fs::remove_dir_all(&path) } else { fs::remove_file(&path) };
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::store::state::JreSource;
    use reqwest::Url;

    fn release(checksum: Checksum) -> JreRelease {
        JreRelease {
            version: crate::jre::version::JavaVersion::parse("25.0.4.1+1").unwrap(),
            version_text: "25.0.4.1+1".into(),
            url: Url::parse("https://example.org/a.zip").unwrap(),
            filename: "a.zip".into(),
            size: 3,
            checksum,
            source: JreSource::Api,
        }
    }

    #[test]
    fn verify_size_and_hashes() {
        let sha1 = "a9993e364706816aba3e25717850c26c9cd0d89d";
        let sha256 = crate::net::download::sha256_bytes(b"abc");
        assert!(verify(&release(Checksum::Sha1(sha1.into())), 3, sha1, &sha256).is_ok());
        assert!(verify(&release(Checksum::Sha256(sha256.clone())), 3, sha1, &sha256).is_ok());
        assert!(verify(&release(Checksum::Sha1(sha1.into())), 4, sha1, &sha256).is_err());
        assert!(verify(&release(Checksum::Sha1("0".repeat(40))), 3, sha1, &sha256).is_err());
        assert!(verify(&release(Checksum::Sha256("0".repeat(64))), 3, sha1, &sha256).is_err());
    }

    #[test]
    fn collect_keeps_current_and_previous() {
        let dir = std::env::temp_dir().join(format!("asterium-gc-{}", unique_suffix()));
        for name in [
            "liberica-25-linux-x64-1",
            "liberica-25-linux-x64-2",
            "liberica-25-linux-x64-3",
            ".staging-x",
            ".trash-y",
            "other",
        ] {
            fs::create_dir_all(dir.join(name)).unwrap();
        }
        let removed = collect(&dir, &["liberica-25-linux-x64-2", "liberica-25-linux-x64-3"]);
        assert_eq!(removed, 3);
        let mut left: Vec<String> =
            fs::read_dir(&dir).unwrap().map(|e| e.unwrap().file_name().to_string_lossy().into_owned()).collect();
        left.sort();
        assert_eq!(left, ["liberica-25-linux-x64-2", "liberica-25-linux-x64-3", "other"]);
        fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn space() {
        assert_eq!(space_needed(120_000_000), 480_000_000);
    }
}
