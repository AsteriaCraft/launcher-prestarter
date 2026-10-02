//! macOS: quarantine on our own files, App Translocation, and the bundle icon for the Dock (ADR 0003).
//!
//! The path logic is plain string and path work, so it is compiled and tested on every OS; only the extended
//! attribute call is macOS-specific.

use std::path::{Path, PathBuf};

pub const QUARANTINE_ATTRIBUTE: &str = "com.apple.quarantine";

/// Where the running `.app` lives, if the executable is `<bundle>.app/Contents/MacOS/<name>`.
pub fn bundle_of(executable: &Path) -> Option<PathBuf> {
    let macos_dir = executable.parent()?;
    let contents = macos_dir.parent()?;
    let bundle = contents.parent()?;
    let is_bundle = macos_dir.file_name()? == "MacOS"
        && contents.file_name()? == "Contents"
        && bundle.extension().is_some_and(|ext| ext == "app");
    is_bundle.then(|| bundle.to_path_buf())
}

/// The `.icns` the Dock should show for the launcher: `icon.icns` if present, otherwise the first `.icns`.
pub fn bundle_icon(bundle: &Path) -> Option<PathBuf> {
    let resources = bundle.join("Contents").join("Resources");
    let preferred = resources.join("icon.icns");
    if preferred.is_file() {
        return Some(preferred);
    }
    let mut icons: Vec<PathBuf> = std::fs::read_dir(&resources)
        .ok()?
        .filter_map(|entry| entry.ok().map(|e| e.path()))
        .filter(|p| p.extension().is_some_and(|ext| ext == "icns"))
        .collect();
    icons.sort();
    icons.into_iter().next()
}

/// The app runs from a read-only place it was not installed to: a mounted DMG or a translocated copy. Nothing breaks
/// (we never write into the bundle), but the player is told to drag Asterium to Applications.
pub fn is_translocated_or_on_dmg(executable: &Path) -> bool {
    let text = executable.to_string_lossy();
    text.contains("/AppTranslocation/") || text.starts_with("/Volumes/")
}

/// Removes `com.apple.quarantine` from `root` and everything below it (symlinks themselves, never their targets).
/// Returns how many files carried the attribute. Missing attributes are not errors.
#[cfg(target_os = "macos")]
pub fn strip_quarantine(root: &Path) -> std::io::Result<usize> {
    use std::ffi::CString;
    use std::os::unix::ffi::OsStrExt;

    fn strip_one(path: &Path) -> std::io::Result<bool> {
        let c_path = CString::new(path.as_os_str().as_bytes())
            .map_err(|_| std::io::Error::from(std::io::ErrorKind::InvalidInput))?;
        let name = CString::new(QUARANTINE_ATTRIBUTE).expect("constant has no NUL");
        // SAFETY: both strings are NUL-terminated; XATTR_NOFOLLOW acts on a symlink itself.
        let rc = unsafe { libc::removexattr(c_path.as_ptr(), name.as_ptr(), libc::XATTR_NOFOLLOW) };
        if rc == 0 {
            return Ok(true);
        }
        let err = std::io::Error::last_os_error();
        if err.raw_os_error() == Some(libc::ENOATTR) { Ok(false) } else { Err(err) }
    }

    let mut removed = 0;
    let mut stack = vec![root.to_path_buf()];
    while let Some(path) = stack.pop() {
        if strip_one(&path)? {
            removed += 1;
        }
        let meta = std::fs::symlink_metadata(&path)?;
        if meta.is_dir() {
            for entry in std::fs::read_dir(&path)? {
                stack.push(entry?.path());
            }
        }
    }
    Ok(removed)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn finds_the_bundle_of_a_bundled_executable() {
        let exe = Path::new("/Applications/Asterium.app/Contents/MacOS/Prestarter");
        assert_eq!(bundle_of(exe), Some(PathBuf::from("/Applications/Asterium.app")));
    }

    #[test]
    fn a_bare_executable_has_no_bundle() {
        assert_eq!(bundle_of(Path::new("/usr/local/bin/Prestarter")), None);
        assert_eq!(bundle_of(Path::new("/tmp/Asterium/Contents/MacOS/Prestarter")), None);
    }

    #[test]
    fn detects_translocation_and_dmg() {
        assert!(is_translocated_or_on_dmg(Path::new(
            "/private/var/folders/xy/T/AppTranslocation/1234/d/Asterium.app/Contents/MacOS/Prestarter"
        )));
        assert!(is_translocated_or_on_dmg(Path::new("/Volumes/Asterium/Asterium.app/Contents/MacOS/Prestarter")));
        assert!(!is_translocated_or_on_dmg(Path::new("/Applications/Asterium.app/Contents/MacOS/Prestarter")));
    }

    #[test]
    fn icon_prefers_icon_icns_then_the_first_icns() {
        let dir = std::env::temp_dir().join(format!("asterium-icns-{}", std::process::id()));
        let resources = dir.join("A.app/Contents/Resources");
        std::fs::create_dir_all(&resources).unwrap();
        std::fs::write(resources.join("b.icns"), b"x").unwrap();
        std::fs::write(resources.join("a.icns"), b"x").unwrap();
        assert_eq!(bundle_icon(&dir.join("A.app")), Some(resources.join("a.icns")));
        std::fs::write(resources.join("icon.icns"), b"x").unwrap();
        assert_eq!(bundle_icon(&dir.join("A.app")), Some(resources.join("icon.icns")));
        std::fs::remove_dir_all(&dir).unwrap();
    }

    #[cfg(target_os = "macos")]
    #[test]
    fn strips_quarantine_recursively() {
        let dir = std::env::temp_dir().join(format!("asterium-quarantine-{}", std::process::id()));
        std::fs::create_dir_all(dir.join("bin")).unwrap();
        std::fs::write(dir.join("bin/java"), b"x").unwrap();
        for path in [dir.clone(), dir.join("bin/java")] {
            let status = std::process::Command::new("xattr")
                .args(["-w", QUARANTINE_ATTRIBUTE, "0081;00000000;Test;"])
                .arg(&path)
                .status()
                .unwrap();
            assert!(status.success());
        }
        assert_eq!(strip_quarantine(&dir).unwrap(), 2);
        assert_eq!(strip_quarantine(&dir).unwrap(), 0);
        std::fs::remove_dir_all(&dir).unwrap();
    }
}
