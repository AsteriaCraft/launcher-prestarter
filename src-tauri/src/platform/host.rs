//! The operating system and CPU this prestarter binary was built for.
//!
//! A macOS universal binary is two slices; each slice reports its own CPU, so the arm64 slice installs the arm64
//! JRE and the x86_64 slice (Intel Macs, or Rosetta) the x64 one.

use std::fmt;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Os {
    Windows,
    Linux,
    MacOs,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Arch {
    X86_64,
    Aarch64,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct Host {
    pub os: Os,
    pub arch: Arch,
}

#[cfg(not(any(target_os = "windows", target_os = "linux", target_os = "macos")))]
compile_error!("the prestarter supports Windows, Linux and macOS only");

#[cfg(not(any(target_arch = "x86_64", target_arch = "aarch64")))]
compile_error!("the prestarter supports x86_64 and aarch64 only");

impl Host {
    /// The target of this build (`cfg!`), not of the machine: an x64 build under emulation reports x86_64.
    pub const fn current() -> Self {
        let os = if cfg!(target_os = "windows") {
            Os::Windows
        } else if cfg!(target_os = "macos") {
            Os::MacOs
        } else {
            Os::Linux
        };
        let arch = if cfg!(target_arch = "aarch64") { Arch::Aarch64 } else { Arch::X86_64 };
        Self { os, arch }
    }
}

impl Os {
    pub const fn as_str(self) -> &'static str {
        match self {
            Os::Windows => "windows",
            Os::Linux => "linux",
            Os::MacOs => "macos",
        }
    }
}

impl Arch {
    pub const fn as_str(self) -> &'static str {
        match self {
            Arch::X86_64 => "x86_64",
            Arch::Aarch64 => "aarch64",
        }
    }
}

impl fmt::Display for Host {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}-{}", self.os.as_str(), self.arch.as_str())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn current_matches_the_compile_target() {
        let host = Host::current();
        assert_eq!(host.os.as_str(), std::env::consts::OS);
        assert_eq!(host.arch.as_str(), std::env::consts::ARCH);
    }

    #[test]
    fn display_is_os_dash_arch() {
        let host = Host { os: Os::MacOs, arch: Arch::Aarch64 };
        assert_eq!(host.to_string(), "macos-aarch64");
    }
}
