//! Streaming download into a file, with a size cap, progress, cancellation, and sha1 + sha256 computed on the way
//! (sha1 is what the Liberica API publishes; sha256 is what we record and what the fallback table pins).

use std::fs::{self, File};
use std::io::{self, BufWriter, Read, Write};
use std::path::Path;
use std::sync::atomic::{AtomicBool, Ordering};

use reqwest::Url;
use reqwest::blocking::Client;
use ring::digest;
use thiserror::Error;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Downloaded {
    pub size: u64,
    pub sha1: String,
    pub sha256: String,
}

#[derive(Debug, Error)]
pub enum DownloadError {
    #[error("{url} answered HTTP {status}")]
    Status { url: String, status: u16 },
    #[error("network error for {url}: {source}")]
    Network {
        url: String,
        #[source]
        source: reqwest::Error,
    },
    #[error("the connection to {url} broke: {source}")]
    Read {
        url: String,
        #[source]
        source: io::Error,
    },
    #[error("{0} is larger than the allowed {1} bytes")]
    TooLarge(String, u64),
    #[error("cannot write {path}: {source}")]
    Write {
        path: String,
        #[source]
        source: io::Error,
    },
    #[error("download of {0} was cancelled")]
    Cancelled(String),
    #[error("{url}: refused URL ({reason})")]
    Refused { url: String, reason: &'static str },
}

impl DownloadError {
    /// Worth one automatic retry (a broken connection), unlike a refusal or a full disk.
    pub fn is_transient(&self) -> bool {
        matches!(self, DownloadError::Network { .. } | DownloadError::Read { .. })
            || matches!(self, DownloadError::Status { status, .. } if *status >= 500 || *status == 429)
    }
}

pub fn hex(bytes: &[u8]) -> String {
    bytes.iter().map(|b| format!("{b:02x}")).collect()
}

/// Downloads `url` into `dest` (created or truncated; the caller owns renaming and deleting it).
///
/// `progress(done, total)` is called after every chunk; `total` is the Content-Length when the server sent one.
pub fn download_to(
    client: &Client,
    url: &Url,
    dest: &Path,
    max_bytes: u64,
    cancel: &AtomicBool,
    progress: &mut dyn FnMut(u64, Option<u64>),
) -> Result<Downloaded, DownloadError> {
    if !super::client::is_allowed_scheme(url) {
        return Err(DownloadError::Refused { url: url.to_string(), reason: "https only (http only on loopback)" });
    }
    let network = |source| DownloadError::Network { url: url.to_string(), source };
    let mut response = client.get(url.clone()).send().map_err(network)?;
    let status = response.status();
    if !status.is_success() {
        return Err(DownloadError::Status { url: url.to_string(), status: status.as_u16() });
    }
    let total = response.content_length();
    if total.is_some_and(|t| t > max_bytes) {
        return Err(DownloadError::TooLarge(url.to_string(), max_bytes));
    }
    if let Some(parent) = dest.parent() {
        fs::create_dir_all(parent)
            .map_err(|source| DownloadError::Write { path: parent.display().to_string(), source })?;
    }
    let write_err = |source| DownloadError::Write { path: dest.display().to_string(), source };
    let mut out = BufWriter::with_capacity(1 << 20, File::create(dest).map_err(write_err)?);
    let mut sha1 = digest::Context::new(&digest::SHA1_FOR_LEGACY_USE_ONLY);
    let mut sha256 = digest::Context::new(&digest::SHA256);
    let mut buffer = vec![0u8; 64 * 1024];
    let mut done: u64 = 0;
    loop {
        if cancel.load(Ordering::Relaxed) {
            return Err(DownloadError::Cancelled(url.to_string()));
        }
        let n = match response.read(&mut buffer) {
            Ok(0) => break,
            Ok(n) => n,
            Err(err) if err.kind() == io::ErrorKind::Interrupted => continue,
            Err(source) => return Err(DownloadError::Read { url: url.to_string(), source }),
        };
        done += n as u64;
        if done > max_bytes {
            return Err(DownloadError::TooLarge(url.to_string(), max_bytes));
        }
        let chunk = &buffer[..n];
        sha1.update(chunk);
        sha256.update(chunk);
        out.write_all(chunk).map_err(write_err)?;
        progress(done, total);
    }
    let file = out.into_inner().map_err(|e| write_err(e.into_error()))?;
    file.sync_all().map_err(write_err)?;
    Ok(Downloaded { size: done, sha1: hex(sha1.finish().as_ref()), sha256: hex(sha256.finish().as_ref()) })
}

/// sha256 of a file on disk, as lowercase hex.
pub fn sha256_file(path: &Path) -> io::Result<String> {
    let mut file = File::open(path)?;
    let mut ctx = digest::Context::new(&digest::SHA256);
    let mut buffer = vec![0u8; 64 * 1024];
    loop {
        let n = file.read(&mut buffer)?;
        if n == 0 {
            break;
        }
        ctx.update(&buffer[..n]);
    }
    Ok(hex(ctx.finish().as_ref()))
}

pub fn sha256_bytes(bytes: &[u8]) -> String {
    hex(digest::digest(&digest::SHA256, bytes).as_ref())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn hex_and_digests() {
        assert_eq!(hex(&[0, 0xab, 0xff]), "00abff");
        assert_eq!(sha256_bytes(b"abc"), "ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad");
    }

    #[test]
    fn sha256_of_a_file() {
        let path = std::env::temp_dir().join(format!("asterium-sha-{}", crate::store::atomic::unique_suffix()));
        fs::write(&path, b"abc").unwrap();
        assert_eq!(sha256_file(&path).unwrap(), sha256_bytes(b"abc"));
        fs::remove_file(path).unwrap();
    }

    #[test]
    fn plain_http_to_the_internet_is_refused() {
        let client = super::super::client::liberica(super::super::client::Roots::Platform).unwrap();
        let url = Url::parse("http://example.org/x").unwrap();
        let dest = std::env::temp_dir().join("asterium-never-written");
        let err = download_to(&client, &url, &dest, 10, &AtomicBool::new(false), &mut |_, _| {}).unwrap_err();
        assert!(matches!(err, DownloadError::Refused { .. }));
        assert!(!dest.exists());
    }

    #[test]
    fn transient_errors() {
        assert!(DownloadError::Status { url: String::new(), status: 503 }.is_transient());
        assert!(!DownloadError::Status { url: String::new(), status: 404 }.is_transient());
        assert!(!DownloadError::TooLarge(String::new(), 1).is_transient());
    }
}
