//! Linux and macOS: free disk space and the new session of the detached launcher (ADR 0006).

use std::ffi::CString;
use std::io;
use std::os::unix::ffi::OsStrExt;
use std::path::Path;

/// Bytes available to an unprivileged user on the file system that holds `path` (the deepest existing ancestor).
pub fn free_space(path: &Path) -> io::Result<u64> {
    let existing = path.ancestors().find(|p| p.exists()).unwrap_or(path);
    let c_path =
        CString::new(existing.as_os_str().as_bytes()).map_err(|_| io::Error::from(io::ErrorKind::InvalidInput))?;
    let mut stat: libc::statvfs = unsafe { std::mem::zeroed() };
    // SAFETY: `c_path` is NUL-terminated and `stat` is a valid, writable statvfs.
    let rc = unsafe { libc::statvfs(c_path.as_ptr(), &mut stat) };
    if rc != 0 {
        return Err(io::Error::last_os_error());
    }
    #[allow(clippy::unnecessary_cast)] // the field types differ between Linux and macOS
    Ok(stat.f_bavail as u64 * stat.f_frsize as u64)
}

/// Runs in the forked child before `exec`: a new session, so closing the terminal that started the prestarter
/// (SIGHUP to its session) does not kill the launcher.
pub fn new_session() -> io::Result<()> {
    // SAFETY: setsid is async-signal-safe, which is what CommandExt::pre_exec requires.
    if unsafe { libc::setsid() } == -1 { Err(io::Error::last_os_error()) } else { Ok(()) }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn free_space_of_the_temp_dir_is_positive() {
        assert!(free_space(&std::env::temp_dir()).unwrap() > 0);
    }

    #[test]
    fn free_space_walks_up_to_an_existing_ancestor() {
        assert!(free_space(&std::env::temp_dir().join("asterium-no-such-dir/deeper")).unwrap() > 0);
    }
}
