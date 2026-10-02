//! The launcher jar copy for the AppImage and macOS (ADR 0001): HTTPS to the pinned launcher host (Mozilla roots,
//! same-origin redirects only, see `net::client`), at most 64 MiB, checked as a jar with a `Main-Class`, then moved
//! into place atomically. A failed or partial download never replaces a working copy.

use std::fs;
use std::path::Path;
use std::sync::atomic::AtomicBool;

use chrono::Utc;
use reqwest::Url;
use reqwest::blocking::Client;
use thiserror::Error;

use super::inspect::{JarError, inspect_file};
use crate::net::download::{DownloadError, download_to};
use crate::store::atomic::{self, unique_suffix};
use crate::store::state::JarRecord;

pub const MAX_JAR_BYTES: u64 = 64 * 1024 * 1024;

#[derive(Debug, Error)]
pub enum FetchError {
    #[error(transparent)]
    Download(#[from] DownloadError),
    #[error("the downloaded launcher is not a usable jar: {0}")]
    NotAJar(#[from] JarError),
    #[error(transparent)]
    Io(#[from] std::io::Error),
}

pub fn fetch_copy(
    client: &Client,
    url: &Url,
    dest: &Path,
    cancel: &AtomicBool,
    progress: &mut dyn FnMut(u64, Option<u64>),
) -> Result<JarRecord, FetchError> {
    let part = dest.with_file_name(format!("Asterium.jar.part-{}", unique_suffix()));
    let result = (|| {
        let downloaded = download_to(client, url, &part, MAX_JAR_BYTES, cancel, progress)?;
        let info = inspect_file(&part)?;
        log::info!(
            "launcher jar {url}: {} bytes, Main-Class {}, sha256 {}",
            downloaded.size,
            info.main_class,
            downloaded.sha256
        );
        atomic::replace(&part, dest)?;
        Ok(JarRecord { url: url.to_string(), sha256: downloaded.sha256, size: downloaded.size, fetched_at: Utc::now() })
    })();
    if result.is_err() {
        let _ = fs::remove_file(&part);
    }
    result
}

/// Whether the copy on disk is a usable jar (it may have been updated by Gravit since we fetched it).
pub fn copy_is_valid(dest: &Path) -> bool {
    inspect_file(dest).is_ok_and(|info| info.prefix_len == 0)
}
