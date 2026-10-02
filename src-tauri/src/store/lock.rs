//! `.install.lock`: one prestarter at a time installs Java or the launcher copy. A second start (a double click)
//! waits for the first one instead of unpacking into the same directories; the OS releases the lock if the holder
//! dies, so a crash never leaves a stale lock.

use std::fs::{File, OpenOptions, TryLockError};
use std::io;
use std::path::Path;

#[derive(Debug)]
pub struct InstallLock {
    _file: File,
}

impl InstallLock {
    /// Takes the lock; if another process holds it, calls `on_wait` once and then blocks until it is free.
    pub fn acquire(path: &Path, on_wait: impl FnOnce()) -> io::Result<Self> {
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)?;
        }
        let file = OpenOptions::new().read(true).write(true).create(true).truncate(false).open(path)?;
        match file.try_lock() {
            Ok(()) => {}
            Err(TryLockError::WouldBlock) => {
                on_wait();
                file.lock()?;
            }
            Err(TryLockError::Error(err)) => return Err(err),
        }
        Ok(Self { _file: file })
    }

    /// Takes the lock only if it is free right now.
    pub fn try_acquire(path: &Path) -> io::Result<Option<Self>> {
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)?;
        }
        let file = OpenOptions::new().read(true).write(true).create(true).truncate(false).open(path)?;
        match file.try_lock() {
            Ok(()) => Ok(Some(Self { _file: file })),
            Err(TryLockError::WouldBlock) => Ok(None),
            Err(TryLockError::Error(err)) => Err(err),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::store::atomic::unique_suffix;
    use std::sync::mpsc;
    use std::time::Duration;

    #[test]
    fn second_holder_waits_until_the_first_releases() {
        let dir = std::env::temp_dir().join(format!("asterium-lock-{}", unique_suffix()));
        let path = dir.join(".install.lock");
        let first = InstallLock::acquire(&path, || panic!("free lock must not wait")).unwrap();
        assert!(InstallLock::try_acquire(&path).unwrap().is_none());

        let (tx, rx) = mpsc::channel();
        let thread_path = path.clone();
        let waiter = std::thread::spawn(move || {
            let _second = InstallLock::acquire(&thread_path, || tx.send("waiting").unwrap()).unwrap();
            "acquired"
        });
        assert_eq!(rx.recv_timeout(Duration::from_secs(5)).unwrap(), "waiting");
        drop(first);
        assert_eq!(waiter.join().unwrap(), "acquired");
        // Free again once the second holder is gone. CI on macOS once found the lock still taken right after the
        // waiter thread had been joined (the cause was not established; a child process spawned by a parallel test
        // sharing the descriptor for a moment is one candidate), so the check gives the release up to 2 s.
        let free = (0..100).any(|_| {
            let taken = InstallLock::try_acquire(&path).unwrap().is_some();
            if !taken {
                std::thread::sleep(Duration::from_millis(20));
            }
            taken
        });
        assert!(free, "the lock stayed taken after both holders were dropped");
        std::fs::remove_dir_all(dir).unwrap();
    }
}
