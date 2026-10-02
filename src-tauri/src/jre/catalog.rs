//! Which JRE each prestarter build installs (ADR 0004, 0005): one table for every target.
//!
//! | Prestarter | API query | Package | Directory key |
//! |---|---|---|---|
//! | windows/x86_64 | `os=windows&arch=x86` | zip | `windows-x64` |
//! | windows/aarch64 with x64 emulation | `os=windows&arch=x86` | zip | `windows-x64` |
//! | windows/aarch64 without it (Windows 10 on ARM) | `os=windows&arch=arm` | zip | `windows-arm64` |
//! | linux/x86_64 | `os=linux&arch=x86` | tar.gz | `linux-x64` |
//! | linux/aarch64 | `os=linux&arch=arm` | tar.gz | `linux-arm64` |
//! | macos/x86_64 (universal slice) | `os=macos&arch=x86` | tar.gz | `macos-x64` |
//! | macos/aarch64 (universal slice) | `os=macos&arch=arm` | tar.gz | `macos-arm64` |
//!
//! Windows on ARM takes the x64 JRE because Liberica's native Windows aarch64 jre-full has no `jfxwebkit.dll` and
//! no media: the launcher's embedded pages would not work. The API calls 64-bit ARM `arm`; `aarch64` is a 400.

use crate::platform::{Arch, Host, Os};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Package {
    Zip,
    TarGz,
}

impl Package {
    pub const fn api_name(self) -> &'static str {
        match self {
            Package::Zip => "zip",
            Package::TarGz => "tar.gz",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct JreTarget {
    /// Key in `fallback.json` and part of the JRE directory name.
    pub key: &'static str,
    pub os: Os,
    pub api_os: &'static str,
    pub api_arch: &'static str,
    pub package: Package,
    /// The native Windows ARM64 JRE without WebKit (only where x64 cannot run).
    pub degraded: bool,
}

const fn target(key: &'static str, os: Os, api_arch: &'static str, package: Package, degraded: bool) -> JreTarget {
    JreTarget { key, os, api_os: os.as_str(), api_arch, package, degraded }
}

/// The JRE for `host`. `x64_emulation` only matters for Windows on ARM64.
pub const fn for_host(host: Host, x64_emulation: bool) -> JreTarget {
    match (host.os, host.arch) {
        (Os::Windows, Arch::X86_64) => target("windows-x64", Os::Windows, "x86", Package::Zip, false),
        (Os::Windows, Arch::Aarch64) if x64_emulation => target("windows-x64", Os::Windows, "x86", Package::Zip, false),
        (Os::Windows, Arch::Aarch64) => target("windows-arm64", Os::Windows, "arm", Package::Zip, true),
        (Os::Linux, Arch::X86_64) => target("linux-x64", Os::Linux, "x86", Package::TarGz, false),
        (Os::Linux, Arch::Aarch64) => target("linux-arm64", Os::Linux, "arm", Package::TarGz, false),
        (Os::MacOs, Arch::X86_64) => target("macos-x64", Os::MacOs, "x86", Package::TarGz, false),
        (Os::MacOs, Arch::Aarch64) => target("macos-arm64", Os::MacOs, "arm", Package::TarGz, false),
    }
}

/// `<store>/jre/<this>`: `liberica-25-windows-x64-25.0.4.1+1`.
pub fn dir_name(target: &JreTarget, feature: u32, version: &str) -> String {
    let safe: String =
        version.chars().map(|c| if c.is_ascii_alphanumeric() || ".+-_".contains(c) { c } else { '_' }).collect();
    format!("liberica-{feature}-{}-{safe}", target.key)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn host(os: Os, arch: Arch) -> Host {
        Host { os, arch }
    }

    #[test]
    fn every_target() {
        let rows = [
            (host(Os::Windows, Arch::X86_64), false, "windows-x64", "windows", "x86", Package::Zip),
            (host(Os::Windows, Arch::Aarch64), true, "windows-x64", "windows", "x86", Package::Zip),
            (host(Os::Windows, Arch::Aarch64), false, "windows-arm64", "windows", "arm", Package::Zip),
            (host(Os::Linux, Arch::X86_64), true, "linux-x64", "linux", "x86", Package::TarGz),
            (host(Os::Linux, Arch::Aarch64), false, "linux-arm64", "linux", "arm", Package::TarGz),
            (host(Os::MacOs, Arch::X86_64), true, "macos-x64", "macos", "x86", Package::TarGz),
            (host(Os::MacOs, Arch::Aarch64), false, "macos-arm64", "macos", "arm", Package::TarGz),
        ];
        for (h, emulation, key, api_os, api_arch, package) in rows {
            let t = for_host(h, emulation);
            assert_eq!((t.key, t.api_os, t.api_arch, t.package), (key, api_os, api_arch, package), "{h}");
            assert_ne!(t.api_arch, "aarch64", "the API answers 400 to arch=aarch64");
        }
    }

    #[test]
    fn only_windows_without_emulation_is_degraded() {
        assert!(for_host(host(Os::Windows, Arch::Aarch64), false).degraded);
        assert!(!for_host(host(Os::Windows, Arch::Aarch64), true).degraded);
        assert!(!for_host(host(Os::Linux, Arch::Aarch64), false).degraded);
    }

    #[test]
    fn directory_names() {
        let t = for_host(host(Os::Windows, Arch::X86_64), true);
        assert_eq!(dir_name(&t, 25, "25.0.4.1+1"), "liberica-25-windows-x64-25.0.4.1+1");
        assert_eq!(dir_name(&t, 25, "../x"), "liberica-25-windows-x64-.._x");
    }
}
