//! Linux: does the system have the libraries JavaFX needs (ADR 0006)? Checked before launch with `ldd`, under the
//! launcher's (cleaned) environment, so an AppImage's own GTK does not hide a missing system GTK. A missing library
//! becomes a package name for the player's distribution.
//!
//! Measured on a bare Ubuntu 24.04 with Liberica 25 jre-full: `libglass.so` needs libX11; `libglassgtk3.so` needs
//! GTK 3 (gtk, gdk, atk, pango, cairo, gdk-pixbuf, glib) and libXtst; `libprism_es2.so` needs libGL and
//! libXxf86vm; media (`libjfxmedia.so`, `libgstreamer-lite.so`) needs glib and libasound.

use std::ffi::OsString;
use std::path::Path;
use std::process::{Command, Stdio};

/// Without these the launcher window cannot open: the launch is refused with a package hint.
pub const CRITICAL: &[&str] = &["libglass.so", "libglassgtk3.so"];
/// Without these JavaFX falls back to software rendering or has no sound: a warning only.
pub const OPTIONAL: &[&str] = &["libprism_es2.so", "libjfxmedia.so", "libgstreamer-lite.so"];

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Distro {
    Debian,
    Fedora,
    Arch,
    Suse,
    Unknown,
}

#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct LibReport {
    pub critical_missing: Vec<String>,
    pub optional_missing: Vec<String>,
}

impl LibReport {
    pub fn is_ok(&self) -> bool {
        self.critical_missing.is_empty()
    }
}

/// Sonames that `ldd` reports as `not found`.
pub fn parse_ldd(output: &str) -> Vec<String> {
    output
        .lines()
        .filter_map(|line| {
            let (name, rest) = line.trim().split_once("=>")?;
            rest.trim().starts_with("not found").then(|| name.trim().to_owned())
        })
        .collect()
}

/// The distribution family from `/etc/os-release` (`ID`, then `ID_LIKE`).
pub fn distro_from_os_release(text: &str) -> Distro {
    let field = |key: &str| {
        text.lines()
            .find_map(|l| l.strip_prefix(key))
            .map(|v| v.trim().trim_matches('"').to_ascii_lowercase())
            .unwrap_or_default()
    };
    let words = format!("{} {}", field("ID="), field("ID_LIKE="));
    let has = |name: &str| words.split_whitespace().any(|w| w == name);
    if has("debian") || has("ubuntu") {
        Distro::Debian
    } else if has("fedora") || has("rhel") || has("centos") {
        Distro::Fedora
    } else if has("arch") {
        Distro::Arch
    } else if has("suse") || has("opensuse") || words.contains("opensuse") {
        Distro::Suse
    } else {
        Distro::Unknown
    }
}

/// The package that provides `soname` on `distro` (GTK's own dependencies resolve to the GTK package).
pub fn package_for(soname: &str, distro: Distro) -> Option<&'static str> {
    const GTK: [&str; 4] = ["libgtk-3-0", "gtk3", "gtk3", "libgtk-3-0"];
    const GLIB: [&str; 4] = ["libglib2.0-0", "glib2", "glib2", "libglib-2_0-0"];
    let row: [&str; 4] = match soname {
        "libgtk-3.so.0"
        | "libgdk-3.so.0"
        | "libatk-1.0.so.0"
        | "libpangocairo-1.0.so.0"
        | "libpango-1.0.so.0"
        | "libcairo.so.2"
        | "libcairo-gobject.so.2"
        | "libgdk_pixbuf-2.0.so.0" => GTK,
        "libglib-2.0.so.0"
        | "libgobject-2.0.so.0"
        | "libgio-2.0.so.0"
        | "libgthread-2.0.so.0"
        | "libgmodule-2.0.so.0" => GLIB,
        "libXtst.so.6" => ["libxtst6", "libXtst", "libxtst", "libXtst6"],
        "libX11.so.6" => ["libx11-6", "libX11", "libx11", "libX11-6"],
        "libXxf86vm.so.1" => ["libxxf86vm1", "libXxf86vm", "libxxf86vm", "libXxf86vm1"],
        "libGL.so.1" => ["libgl1", "mesa-libGL", "libglvnd", "Mesa-libGL1"],
        "libasound.so.2" => ["libasound2", "alsa-lib", "alsa-lib", "libasound2"],
        _ => return None,
    };
    let index = match distro {
        Distro::Debian => 0,
        Distro::Fedora => 1,
        Distro::Arch => 2,
        Distro::Suse => 3,
        Distro::Unknown => return None,
    };
    Some(row[index])
}

/// Unique packages for `missing`, in a stable order, plus the sonames no package is known for.
pub fn packages(missing: &[String], distro: Distro) -> (Vec<&'static str>, Vec<String>) {
    let mut found: Vec<&'static str> = Vec::new();
    let mut unknown = Vec::new();
    for soname in missing {
        match package_for(soname, distro) {
            Some(package) if !found.contains(&package) => found.push(package),
            Some(_) => {}
            None => unknown.push(soname.clone()),
        }
    }
    // GTK pulls in glib; listing both only lengthens the command.
    if found.iter().any(|p| ["libgtk-3-0", "gtk3"].contains(p)) {
        found.retain(|p| !["libglib2.0-0", "glib2", "libglib-2_0-0"].contains(p));
    }
    (found, unknown)
}

/// The command a player can paste.
pub fn install_command(distro: Distro, packages: &[&str]) -> Option<String> {
    if packages.is_empty() {
        return None;
    }
    let list = packages.join(" ");
    Some(match distro {
        Distro::Debian => format!("sudo apt install {list}"),
        Distro::Fedora => format!("sudo dnf install {list}"),
        Distro::Arch => format!("sudo pacman -S --needed {list}"),
        Distro::Suse => format!("sudo zypper install {list}"),
        Distro::Unknown => return None,
    })
}

fn bundled(jre_lib: &Path, soname: &str) -> bool {
    jre_lib.join(soname).exists() || jre_lib.join("server").join(soname).exists()
}

/// Runs `ldd` on the JavaFX libraries of the JRE at `home`. Returns `None` when `ldd` cannot run (then nothing is
/// claimed and the launch goes ahead).
pub fn check(home: &Path, env: &[(OsString, OsString)]) -> Option<LibReport> {
    let lib = home.join("lib");
    let mut report = LibReport::default();
    for (names, critical) in [(CRITICAL, true), (OPTIONAL, false)] {
        for name in names {
            let path = lib.join(name);
            if !path.exists() {
                continue;
            }
            let output = Command::new("ldd")
                .arg(&path)
                .env_clear()
                .envs(env.iter().map(|(k, v)| (k, v)))
                .stdin(Stdio::null())
                .stderr(Stdio::null())
                .output()
                .ok()?;
            let text = String::from_utf8_lossy(&output.stdout);
            let target = if critical { &mut report.critical_missing } else { &mut report.optional_missing };
            for soname in parse_ldd(&text) {
                if !bundled(&lib, &soname) && !target.contains(&soname) {
                    target.push(soname);
                }
            }
        }
    }
    Some(report)
}

#[cfg(test)]
mod tests {
    use super::*;

    const LDD_BARE_UBUNTU: &str = "\tlinux-vdso.so.1 (0x00007ffd)\n\tlibgtk-3.so.0 => not found\n\tlibgdk-3.so.0 => not found\n\
        \tlibXtst.so.6 => not found\n\tlibglib-2.0.so.0 => not found\n\tlibdl.so.2 => /lib/x86_64-linux-gnu/libdl.so.2 (0x7f)\n\
        \tlibjvm.so => not found\n";

    #[test]
    fn parses_not_found_lines() {
        assert_eq!(
            parse_ldd(LDD_BARE_UBUNTU),
            ["libgtk-3.so.0", "libgdk-3.so.0", "libXtst.so.6", "libglib-2.0.so.0", "libjvm.so"]
        );
        assert!(parse_ldd("\tlibc.so.6 => /lib/libc.so.6 (0x1)\n").is_empty());
    }

    #[test]
    fn distributions() {
        let ubuntu = "NAME=\"Ubuntu\"\nID=ubuntu\nID_LIKE=debian\n";
        let mint = "ID=linuxmint\nID_LIKE=\"ubuntu debian\"\n";
        let fedora = "ID=fedora\n";
        let alma = "ID=\"almalinux\"\nID_LIKE=\"rhel centos fedora\"\n";
        let manjaro = "ID=manjaro\nID_LIKE=arch\n";
        let tumbleweed = "ID=\"opensuse-tumbleweed\"\nID_LIKE=\"opensuse suse\"\n";
        assert_eq!(distro_from_os_release(ubuntu), Distro::Debian);
        assert_eq!(distro_from_os_release(mint), Distro::Debian);
        assert_eq!(distro_from_os_release(fedora), Distro::Fedora);
        assert_eq!(distro_from_os_release(alma), Distro::Fedora);
        assert_eq!(distro_from_os_release(manjaro), Distro::Arch);
        assert_eq!(distro_from_os_release(tumbleweed), Distro::Suse);
        assert_eq!(distro_from_os_release("ID=nixos\n"), Distro::Unknown);
    }

    #[test]
    fn packages_per_distro() {
        let missing: Vec<String> = parse_ldd(LDD_BARE_UBUNTU);
        let (debian, unknown) = packages(&missing, Distro::Debian);
        assert_eq!(debian, ["libgtk-3-0", "libxtst6"]);
        assert_eq!(unknown, ["libjvm.so"]);
        assert_eq!(packages(&missing, Distro::Fedora).0, ["gtk3", "libXtst"]);
        assert_eq!(packages(&missing, Distro::Arch).0, ["gtk3", "libxtst"]);
        assert_eq!(install_command(Distro::Debian, &debian).as_deref(), Some("sudo apt install libgtk-3-0 libxtst6"));
        assert_eq!(install_command(Distro::Arch, &["gtk3"]).as_deref(), Some("sudo pacman -S --needed gtk3"));
        assert_eq!(install_command(Distro::Unknown, &["x"]), None);
        assert_eq!(install_command(Distro::Debian, &[]), None);
    }

    #[test]
    fn sound_and_gl_are_optional_packages() {
        let missing = vec!["libasound.so.2".to_owned(), "libGL.so.1".to_owned()];
        assert_eq!(packages(&missing, Distro::Debian).0, ["libasound2", "libgl1"]);
    }

    #[test]
    fn libraries_shipped_inside_the_jre_are_not_missing() {
        let dir = std::env::temp_dir().join(format!("asterium-libs-{}", crate::store::atomic::unique_suffix()));
        std::fs::create_dir_all(dir.join("server")).unwrap();
        std::fs::write(dir.join("server/libjvm.so"), b"").unwrap();
        assert!(bundled(&dir, "libjvm.so"));
        assert!(!bundled(&dir, "libgtk-3.so.0"));
        std::fs::remove_dir_all(dir).unwrap();
    }
}
