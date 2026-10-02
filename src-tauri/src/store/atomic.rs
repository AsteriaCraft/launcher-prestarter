//! Writes that are either complete or absent: a temporary file in the same directory, flushed to disk, then renamed
//! over the target (`rename` replaces atomically on Windows too). A crash leaves at most a `*.tmp-*` file behind.

use std::fs::{self, File};
use std::io::{self, Write};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{SystemTime, UNIX_EPOCH};

/// A name suffix that is unique within this process and very unlikely to repeat across processes.
pub fn unique_suffix() -> String {
    static COUNTER: AtomicU64 = AtomicU64::new(0);
    let nanos = SystemTime::now().duration_since(UNIX_EPOCH).map(|d| d.subsec_nanos()).unwrap_or(0);
    format!("{}-{:08x}-{}", std::process::id(), nanos, COUNTER.fetch_add(1, Ordering::Relaxed))
}

/// The temporary sibling used by [`write`] (exposed for tests and cleanup).
pub fn temp_sibling(target: &Path) -> PathBuf {
    let name = target.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default();
    target.with_file_name(format!("{name}.tmp-{}", unique_suffix()))
}

pub fn write(target: &Path, bytes: &[u8]) -> io::Result<()> {
    if let Some(parent) = target.parent() {
        fs::create_dir_all(parent)?;
    }
    let tmp = temp_sibling(target);
    let result = (|| {
        let mut file = File::create(&tmp)?;
        file.write_all(bytes)?;
        file.sync_all()?;
        drop(file);
        fs::rename(&tmp, target)
    })();
    if result.is_err() {
        let _ = fs::remove_file(&tmp);
    }
    result
}

/// Moves a finished file (for example a verified download) over `target`.
pub fn replace(source: &Path, target: &Path) -> io::Result<()> {
    if let Some(parent) = target.parent() {
        fs::create_dir_all(parent)?;
    }
    fs::rename(source, target)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn temp_dir(tag: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!("asterium-atomic-{tag}-{}", unique_suffix()));
        fs::create_dir_all(&dir).unwrap();
        dir
    }

    #[test]
    fn writes_and_replaces() {
        let dir = temp_dir("w");
        let target = dir.join("sub").join("state.json");
        write(&target, b"one").unwrap();
        write(&target, b"two").unwrap();
        assert_eq!(fs::read(&target).unwrap(), b"two");
        let leftovers: Vec<_> = fs::read_dir(target.parent().unwrap()).unwrap().collect();
        assert_eq!(leftovers.len(), 1, "no temporary file is left behind");
        fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn suffixes_are_unique() {
        assert_ne!(unique_suffix(), unique_suffix());
    }
}
