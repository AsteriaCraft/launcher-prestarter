//! Writes that are either complete or absent: a temporary file in the same directory, flushed to disk, then renamed
//! over the target (`rename` replaces atomically on Windows too). A crash leaves at most a `*.tmp-*` file behind.
//! Plus a rename that waits out programs holding files open for a moment ([`rename_patiently`]).

use std::fs::{self, File};
use std::io::{self, Write};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

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

/// Renames `from` to `to`, waiting out another program that has a file inside open for a moment. On Windows a
/// directory cannot be renamed while a file in it is open without delete sharing, and virus scanners and the x64
/// emulator's translation cache on ARM64 do that right after Java's files are written or first run (CI on
/// windows-11-arm: "Access is denied" for a fresh JRE). Access-denied and sharing-violation errors are retried with
/// growing pauses until `patience` is spent; other errors, and every error outside Windows, return at once. The
/// error names both paths.
pub fn rename_patiently(from: &Path, to: &Path, patience: Duration) -> io::Result<()> {
    const FIRST_PAUSE: Duration = Duration::from_millis(50);
    const LONGEST_PAUSE: Duration = Duration::from_secs(2);
    let started = Instant::now();
    let mut pause = FIRST_PAUSE;
    loop {
        match fs::rename(from, to) {
            Ok(()) => {
                if pause > FIRST_PAUSE {
                    log::info!("moved {} after waiting {} ms", from.display(), started.elapsed().as_millis());
                }
                return Ok(());
            }
            Err(err) if cfg!(windows) && is_busy(&err) && started.elapsed() + pause <= patience => {
                if pause == FIRST_PAUSE {
                    log::warn!("{} is in use by another program ({err}); waiting up to {patience:?}", from.display());
                }
                std::thread::sleep(pause);
                pause = (pause * 2).min(LONGEST_PAUSE);
            }
            Err(err) => {
                return Err(io::Error::new(
                    err.kind(),
                    format!("cannot move {} to {}: {err}", from.display(), to.display()),
                ));
            }
        }
    }
}

/// `ERROR_ACCESS_DENIED` (5) or `ERROR_SHARING_VIOLATION` (32): another process has a file open.
fn is_busy(err: &io::Error) -> bool {
    matches!(err.raw_os_error(), Some(5 | 32))
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
    fn a_free_directory_is_moved_at_once() {
        let dir = temp_dir("r");
        fs::create_dir_all(dir.join("staging/bin")).unwrap();
        fs::write(dir.join("staging/bin/java"), b"x").unwrap();
        let started = Instant::now();
        rename_patiently(&dir.join("staging"), &dir.join("home"), Duration::from_secs(5)).unwrap();
        assert!(started.elapsed() < Duration::from_secs(1));
        assert!(dir.join("home/bin/java").is_file());
        fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn a_missing_source_fails_at_once_and_names_both_paths() {
        let dir = temp_dir("m");
        let started = Instant::now();
        let err = rename_patiently(&dir.join("absent"), &dir.join("home"), Duration::from_secs(5)).unwrap_err();
        assert_eq!(err.kind(), io::ErrorKind::NotFound);
        assert!(started.elapsed() < Duration::from_secs(1));
        assert!(err.to_string().contains("absent") && err.to_string().contains("home"), "{err}");
        fs::remove_dir_all(dir).unwrap();
    }

    /// Windows: a file open without delete sharing (what a scanner does) blocks the directory's rename until it is
    /// closed; the rename waits for it.
    #[cfg(windows)]
    #[test]
    fn waits_for_a_program_that_holds_a_file_open() {
        use std::os::windows::fs::OpenOptionsExt;
        const FILE_SHARE_READ: u32 = 0x1;
        let dir = temp_dir("busy");
        fs::create_dir_all(dir.join("staging/bin")).unwrap();
        fs::write(dir.join("staging/bin/java.exe"), b"x").unwrap();
        let held = fs::OpenOptions::new().read(true).share_mode(FILE_SHARE_READ).open(dir.join("staging/bin/java.exe"));
        let held = held.unwrap();
        assert!(fs::rename(dir.join("staging"), dir.join("probe")).is_err(), "the open file must block the rename");
        let releaser = std::thread::spawn(move || {
            std::thread::sleep(Duration::from_millis(700));
            drop(held);
        });
        let started = Instant::now();
        rename_patiently(&dir.join("staging"), &dir.join("home"), Duration::from_secs(10)).unwrap();
        assert!(started.elapsed() >= Duration::from_millis(500));
        assert!(dir.join("home/bin/java.exe").is_file());
        releaser.join().unwrap();
        fs::remove_dir_all(dir).unwrap();
    }

    #[cfg(windows)]
    #[test]
    fn gives_up_when_patience_runs_out() {
        use std::os::windows::fs::OpenOptionsExt;
        let dir = temp_dir("stuck");
        fs::create_dir_all(dir.join("staging")).unwrap();
        fs::write(dir.join("staging/release"), b"x").unwrap();
        let held = fs::OpenOptions::new().read(true).share_mode(0x1).open(dir.join("staging/release")).unwrap();
        let err = rename_patiently(&dir.join("staging"), &dir.join("home"), Duration::from_millis(300)).unwrap_err();
        assert_eq!(err.kind(), io::ErrorKind::PermissionDenied);
        assert!(err.to_string().starts_with("cannot move "), "{err}");
        drop(held);
        fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn suffixes_are_unique() {
        assert_ne!(unique_suffix(), unique_suffix());
    }
}
