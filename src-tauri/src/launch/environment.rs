//! The launcher's environment (ADR 0006): a snapshot of the prestarter's environment taken before it changes
//! anything, cleaned of everything an AppImage's AppRun added, plus the macOS Dock options.

use std::ffi::{OsStr, OsString};
use std::path::{Path, PathBuf};

/// Variables AppRun or the AppImage runtime set that must never reach the launcher, Java or Minecraft.
const APPIMAGE_DROP: &[&str] = &[
    "APPDIR",
    "APPIMAGE",
    "ARGV0",
    "OWD",
    "GDK_BACKEND",
    "GTK_THEME",
    "GTK_PATH",
    "GTK_IM_MODULE_FILE",
    "GTK_DATA_PREFIX",
    "GTK_EXE_PREFIX",
    "GDK_PIXBUF_MODULE_FILE",
    "GIO_EXTRA_MODULES",
    "GSETTINGS_SCHEMA_DIR",
    "PYTHONHOME",
    "PYTHONDONTWRITEBYTECODE",
];

/// Search paths AppRun prepends to: only the elements inside `$APPDIR` are removed.
const APPIMAGE_PATH_LISTS: &[&str] = &[
    "PATH",
    "LD_LIBRARY_PATH",
    "XDG_DATA_DIRS",
    "XDG_CONFIG_DIRS",
    "GI_TYPELIB_PATH",
    "PYTHONPATH",
    "PERLLIB",
    "QT_PLUGIN_PATH",
];

/// Prefixes of path-list variables (`GST_PLUGIN_SYSTEM_PATH_1_0` and friends).
const APPIMAGE_PATH_LIST_PREFIXES: &[&str] = &["GST_PLUGIN_PATH", "GST_PLUGIN_SYSTEM_PATH", "GST_PLUGIN_SCANNER"];

const DEFAULT_PATH: &str = "/usr/local/bin:/usr/bin:/bin";

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct EnvSnapshot {
    vars: Vec<(OsString, OsString)>,
}

fn same_name(a: &OsStr, b: &str) -> bool {
    if cfg!(windows) { a.to_string_lossy().eq_ignore_ascii_case(b) } else { a == b }
}

impl EnvSnapshot {
    /// Call first thing in `main`, before any `set_var` and before GTK or WebView2 start.
    pub fn capture() -> Self {
        Self { vars: std::env::vars_os().collect() }
    }

    pub fn from_pairs<K: Into<OsString>, V: Into<OsString>>(pairs: impl IntoIterator<Item = (K, V)>) -> Self {
        Self { vars: pairs.into_iter().map(|(k, v)| (k.into(), v.into())).collect() }
    }

    pub fn get(&self, name: &str) -> Option<&OsStr> {
        self.vars.iter().find(|(k, _)| same_name(k, name)).map(|(_, v)| v.as_os_str())
    }

    pub fn vars(&self) -> &[(OsString, OsString)] {
        &self.vars
    }

    /// `$APPDIR` when the prestarter runs inside an AppImage (both `APPIMAGE` and `APPDIR` are set).
    pub fn appimage_dir(&self) -> Option<PathBuf> {
        self.get("APPIMAGE").filter(|v| !v.is_empty())?;
        self.get("APPDIR").filter(|v| !v.is_empty()).map(PathBuf::from)
    }

    /// The directory the player started the AppImage from (`OWD`), if set.
    pub fn original_dir(&self) -> Option<PathBuf> {
        self.get("OWD").filter(|v| !v.is_empty()).map(PathBuf::from)
    }
}

fn inside(value: &str, appdir: &str) -> bool {
    value == appdir || value.strip_prefix(appdir).is_some_and(|rest| rest.starts_with('/'))
}

fn is_path_list(name: &str) -> bool {
    APPIMAGE_PATH_LISTS.contains(&name) || APPIMAGE_PATH_LIST_PREFIXES.iter().any(|p| name.starts_with(p))
}

/// Removes what AppRun added (see the module constants); any other variable whose value mentions `$APPDIR` goes too.
pub fn sanitize_appimage(vars: &[(OsString, OsString)], appdir: &Path) -> Vec<(OsString, OsString)> {
    let appdir = appdir.to_string_lossy().trim_end_matches('/').to_owned();
    let mut out = Vec::with_capacity(vars.len());
    let mut has_path = false;
    for (name, value) in vars {
        let Some(name_text) = name.to_str() else {
            out.push((name.clone(), value.clone()));
            continue;
        };
        if APPIMAGE_DROP.contains(&name_text) || name_text.starts_with("APPIMAGE_") {
            continue;
        }
        let Some(text) = value.to_str() else {
            // Not UTF-8: cannot point into our mount (its path is UTF-8), keep it as the player had it.
            out.push((name.clone(), value.clone()));
            continue;
        };
        if is_path_list(name_text) {
            let kept: Vec<&str> = text.split(':').filter(|e| !e.is_empty() && !inside(e, &appdir)).collect();
            if kept.is_empty() {
                continue;
            }
            if name_text == "PATH" {
                has_path = true;
            }
            out.push((name.clone(), OsString::from(kept.join(":"))));
            continue;
        }
        if text.contains(&appdir) {
            continue;
        }
        out.push((name.clone(), value.clone()));
    }
    if !has_path {
        out.push((OsString::from("PATH"), OsString::from(DEFAULT_PATH)));
    }
    out
}

/// Appends the Dock name and icon to `JDK_JAVA_OPTIONS` (macOS): the Java launcher reads it for every JVM, and
/// Gravit does not clear it, so the launcher's own JVM shows "Asterium" with our icon.
pub fn with_dock_options(mut vars: Vec<(OsString, OsString)>, icon: Option<&Path>) -> Vec<(OsString, OsString)> {
    let mut options = String::from("-Xdock:name=Asterium");
    if let Some(icon) = icon {
        options.push_str(&format!(" \"-Xdock:icon={}\"", icon.display()));
    }
    match vars.iter_mut().find(|(k, _)| k == "JDK_JAVA_OPTIONS") {
        Some((_, value)) => {
            let mut joined = value.clone();
            joined.push(" ");
            joined.push(&options);
            *value = joined;
        }
        None => vars.push((OsString::from("JDK_JAVA_OPTIONS"), OsString::from(options))),
    }
    vars
}

/// Everything the launcher process gets: the snapshot, cleaned under an AppImage, with the Dock options on macOS.
pub fn child_environment(snapshot: &EnvSnapshot, macos_icon: Option<Option<&Path>>) -> Vec<(OsString, OsString)> {
    let vars = match snapshot.appimage_dir() {
        Some(appdir) => sanitize_appimage(snapshot.vars(), &appdir),
        None => snapshot.vars().to_vec(),
    };
    match macos_icon {
        Some(icon) => with_dock_options(vars, icon),
        None => vars,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const APPRUN: &str = include_str!("../../tests/fixtures/appimage/apprun-env.txt");

    fn fixture() -> EnvSnapshot {
        EnvSnapshot::from_pairs(APPRUN.lines().filter(|l| !l.is_empty()).map(|line| {
            let (k, v) = line.split_once('=').unwrap();
            (k.to_owned(), v.to_owned())
        }))
    }

    fn lookup<'a>(vars: &'a [(OsString, OsString)], name: &str) -> Option<&'a str> {
        vars.iter().find(|(k, _)| k == name).map(|(_, v)| v.to_str().unwrap())
    }

    #[test]
    fn detects_an_appimage() {
        let snapshot = fixture();
        assert_eq!(snapshot.appimage_dir(), Some(PathBuf::from("/tmp/.mount_AsteriA1b2C3")));
        assert_eq!(snapshot.original_dir(), Some(PathBuf::from("/home/player/Downloads")));
        let plain = EnvSnapshot::from_pairs([("APPDIR", "/x")]);
        assert_eq!(plain.appimage_dir(), None, "APPDIR alone is not an AppImage");
    }

    #[test]
    fn nothing_of_the_appimage_reaches_the_launcher() {
        let snapshot = fixture();
        let clean = child_environment(&snapshot, None);
        for (name, value) in &clean {
            let value = value.to_string_lossy();
            assert!(!value.contains("/tmp/.mount_"), "{name:?}={value} still points into the mount");
            assert!(!APPIMAGE_DROP.contains(&name.to_str().unwrap()), "{name:?} must be removed");
        }
        assert_eq!(lookup(&clean, "PATH"), Some("/usr/local/sbin:/usr/local/bin:/usr/sbin:/usr/bin:/sbin:/bin"));
        assert_eq!(lookup(&clean, "XDG_DATA_DIRS"), Some("/usr/share"));
        assert_eq!(lookup(&clean, "LD_LIBRARY_PATH"), None);
        assert_eq!(lookup(&clean, "GST_PLUGIN_SYSTEM_PATH_1_0"), None);
        assert_eq!(lookup(&clean, "PYTHONPATH"), None);
        assert_eq!(lookup(&clean, "PWD"), None);
        // The player's own variables stay.
        for (name, value) in [
            ("HOME", "/home/player"),
            ("DISPLAY", ":0"),
            ("WAYLAND_DISPLAY", "wayland-0"),
            ("LANG", "uk_UA.UTF-8"),
            ("XDG_RUNTIME_DIR", "/run/user/1000"),
        ] {
            assert_eq!(lookup(&clean, name), Some(value), "{name}");
        }
    }

    #[test]
    fn an_empty_path_gets_a_default() {
        let snapshot =
            EnvSnapshot::from_pairs([("APPIMAGE", "/a.AppImage"), ("APPDIR", "/tmp/m"), ("PATH", "/tmp/m/usr/bin")]);
        let clean = child_environment(&snapshot, None);
        assert_eq!(lookup(&clean, "PATH"), Some(DEFAULT_PATH));
    }

    #[test]
    fn a_similar_prefix_is_not_inside() {
        let vars = vec![(OsString::from("PATH"), OsString::from("/tmp/m2/bin:/tmp/m/bin"))];
        let clean = sanitize_appimage(&vars, Path::new("/tmp/m"));
        assert_eq!(lookup(&clean, "PATH"), Some("/tmp/m2/bin"));
    }

    #[test]
    fn outside_an_appimage_the_snapshot_is_passed_as_is() {
        let snapshot = EnvSnapshot::from_pairs([
            ("PATH", "/usr/bin"),
            ("LD_LIBRARY_PATH", "/opt/lib"),
            ("GDK_BACKEND", "wayland"),
        ]);
        assert_eq!(child_environment(&snapshot, None), snapshot.vars().to_vec());
    }

    #[test]
    fn variables_set_after_the_snapshot_do_not_leak() {
        let snapshot = EnvSnapshot::from_pairs([("PATH", "/usr/bin")]);
        // What the prestarter sets for WebKitGTK after taking the snapshot:
        let later = [("__GL_THREADED_OPTIMIZATIONS", "0"), ("__NV_DISABLE_EXPLICIT_SYNC", "1")];
        let clean = child_environment(&snapshot, None);
        for (name, _) in later {
            assert_eq!(lookup(&clean, name), None);
        }
    }

    #[test]
    fn dock_options_on_macos() {
        let snapshot = EnvSnapshot::from_pairs([("HOME", "/Users/p")]);
        let icon = Path::new("/Applications/My Games/Asterium.app/Contents/Resources/icon.icns");
        let env = child_environment(&snapshot, Some(Some(icon)));
        assert_eq!(
            lookup(&env, "JDK_JAVA_OPTIONS"),
            Some(
                "-Xdock:name=Asterium \"-Xdock:icon=/Applications/My Games/Asterium.app/Contents/Resources/icon.icns\""
            )
        );
        let with_own = EnvSnapshot::from_pairs([("JDK_JAVA_OPTIONS", "-Xss2m")]);
        let env = child_environment(&with_own, Some(None));
        assert_eq!(lookup(&env, "JDK_JAVA_OPTIONS"), Some("-Xss2m -Xdock:name=Asterium"));
    }

    #[test]
    fn lookup_is_case_insensitive_only_on_windows() {
        let snapshot = EnvSnapshot::from_pairs([("Path", "x")]);
        assert_eq!(snapshot.get("PATH").is_some(), cfg!(windows));
    }
}
