//! Operating-system specifics behind a small API. Nothing here depends on Tauri.

pub mod host;
pub mod macos;
#[cfg(unix)]
pub mod unix;
#[cfg(windows)]
pub mod windows;

use std::io;
use std::path::Path;
use std::process::{Child, Command};
use std::time::Duration;

pub use host::{Arch, Host, Os};

/// Bytes available to this user on the volume that holds `path`.
pub fn free_space(path: &Path) -> io::Result<u64> {
    #[cfg(windows)]
    return windows::free_space(path);
    #[cfg(unix)]
    return unix::free_space(path);
}

/// Windows: the ANSI code page that cannot write `path`, which Java would then receive with `?` in it
/// ([`windows::ansi_cannot_write`]). Elsewhere Java takes the path's bytes as they are: always `None`.
pub fn java_cannot_write(path: &Path) -> Option<u32> {
    #[cfg(windows)]
    return windows::ansi_cannot_write(path.as_os_str());
    #[cfg(not(windows))]
    {
        let _ = path;
        None
    }
}

/// The ANSI code page on Windows (65001 means UTF-8); `None` elsewhere.
pub fn ansi_code_page() -> Option<u32> {
    #[cfg(windows)]
    return Some(windows::ansi_code_page());
    #[cfg(not(windows))]
    return None;
}

/// Starts `command`, retrying for a moment while its executable is busy (`ETXTBSY`): a file is busy while any process
/// holds it open for writing, and a child that another thread forks at that moment (WebKitGTK's helper processes,
/// tests running in parallel) keeps a copy of the descriptor until it execs. CI hit this with `java -version` right
/// after the JRE was unpacked. Pauses double from 20 ms, about 5 s in all; other errors return at once.
pub fn spawn_when_not_busy(command: &mut Command) -> io::Result<Child> {
    let mut pause = Duration::from_millis(20);
    for _ in 0..8 {
        match command.spawn() {
            Err(err) if is_text_busy(&err) => {
                log::warn!("{err} starting {:?}; trying again in {} ms", command.get_program(), pause.as_millis());
                std::thread::sleep(pause);
                pause *= 2;
            }
            other => return other,
        }
    }
    command.spawn()
}

fn is_text_busy(err: &io::Error) -> bool {
    #[cfg(unix)]
    return err.raw_os_error() == Some(libc::ETXTBSY);
    #[cfg(not(unix))]
    {
        let _ = err;
        false
    }
}

/// Whether this machine can run x64 code. Always true for an x64 build; for the Windows ARM64 build it asks the OS
/// (ADR 0004); Linux and macOS ARM builds never install an x64 JRE, so the answer does not matter there.
pub fn x64_emulation_available() -> bool {
    if cfg!(target_arch = "x86_64") {
        return true;
    }
    #[cfg(windows)]
    return windows::x64_emulation_available();
    #[cfg(not(windows))]
    return false;
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Linux refuses to run a file that is open for writing; the spawn waits until the writer closes it.
    #[cfg(target_os = "linux")]
    #[test]
    fn waits_while_the_executable_is_open_for_writing() {
        use std::io::Write;
        use std::os::unix::fs::PermissionsExt;
        let dir = std::env::temp_dir().join(format!("asterium-busy-{}", crate::store::atomic::unique_suffix()));
        std::fs::create_dir_all(&dir).unwrap();
        let script = dir.join("java");
        let mut writer = std::fs::File::create(&script).unwrap();
        writer.write_all(b"#!/bin/sh\nexit 0\n").unwrap();
        writer.flush().unwrap();
        std::fs::set_permissions(&script, std::fs::Permissions::from_mode(0o755)).unwrap();
        let err = Command::new(&script).spawn().unwrap_err();
        assert!(is_text_busy(&err), "{err}");
        let closer = std::thread::spawn(move || {
            std::thread::sleep(Duration::from_millis(200));
            drop(writer);
        });
        let status = spawn_when_not_busy(&mut Command::new(&script)).unwrap().wait().unwrap();
        assert!(status.success());
        closer.join().unwrap();
        std::fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn other_spawn_errors_return_at_once() {
        let started = std::time::Instant::now();
        let err = spawn_when_not_busy(&mut Command::new("asterium-no-such-program")).unwrap_err();
        assert_eq!(err.kind(), io::ErrorKind::NotFound);
        assert!(started.elapsed() < Duration::from_secs(1));
    }
}
