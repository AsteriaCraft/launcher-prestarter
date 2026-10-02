//! Operating-system specifics behind a small API. Nothing here depends on Tauri.

pub mod host;
pub mod macos;
#[cfg(unix)]
pub mod unix;
#[cfg(windows)]
pub mod windows;

use std::io;
use std::path::Path;

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
