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
///
/// Only files that carry the attribute are touched: removing one needs write permission, and a JRE has read-only
/// files (CI saw EACCES on one, which used to stop the whole walk). A read-only file that does carry it gets the
/// owner's write bit for the removal and its mode back afterwards. One failure does not stop the walk; the first
/// error is returned at the end.
#[cfg(target_os = "macos")]
pub fn strip_quarantine(root: &Path) -> std::io::Result<usize> {
    use std::ffi::CString;
    use std::io;
    use std::os::unix::ffi::OsStrExt;
    use std::os::unix::fs::PermissionsExt;

    fn c_string(path: &Path) -> io::Result<CString> {
        CString::new(path.as_os_str().as_bytes()).map_err(|_| io::Error::from(io::ErrorKind::InvalidInput))
    }

    fn has_attribute(path: &CString, name: &CString) -> bool {
        // SAFETY: both strings are NUL-terminated; a null buffer of size 0 only asks for the value's size.
        unsafe { libc::getxattr(path.as_ptr(), name.as_ptr(), std::ptr::null_mut(), 0, 0, libc::XATTR_NOFOLLOW) >= 0 }
    }

    fn remove_attribute(path: &CString, name: &CString) -> io::Result<()> {
        // SAFETY: both strings are NUL-terminated; XATTR_NOFOLLOW acts on a symlink itself.
        let rc = unsafe { libc::removexattr(path.as_ptr(), name.as_ptr(), libc::XATTR_NOFOLLOW) };
        if rc == 0 { Ok(()) } else { Err(io::Error::last_os_error()) }
    }

    fn strip_one(path: &Path, meta: &std::fs::Metadata) -> io::Result<bool> {
        let c_path = c_string(path)?;
        let name = CString::new(QUARANTINE_ATTRIBUTE).expect("constant has no NUL");
        if !has_attribute(&c_path, &name) {
            return Ok(false);
        }
        match remove_attribute(&c_path, &name) {
            Ok(()) => Ok(true),
            Err(err) if err.raw_os_error() == Some(libc::ENOATTR) => Ok(false),
            Err(err) if !meta.file_type().is_symlink() && (meta.permissions().mode() & 0o200) == 0 => {
                let original = meta.permissions();
                std::fs::set_permissions(path, std::fs::Permissions::from_mode(original.mode() | 0o200))
                    .map_err(|_| err)?;
                let removed = remove_attribute(&c_path, &name);
                std::fs::set_permissions(path, original)?;
                removed.map(|()| true)
            }
            Err(err) => Err(err),
        }
    }

    fn keep_first(slot: &mut Option<io::Error>, err: io::Error) {
        if slot.is_none() {
            *slot = Some(err);
        }
    }

    let mut removed = 0;
    let mut first_error: Option<io::Error> = None;
    let mut stack = vec![root.to_path_buf()];
    while let Some(path) = stack.pop() {
        let meta = match std::fs::symlink_metadata(&path) {
            Ok(meta) => meta,
            Err(err) => {
                keep_first(&mut first_error, err);
                continue;
            }
        };
        match strip_one(&path, &meta) {
            Ok(true) => removed += 1,
            Ok(false) => {}
            Err(err) => keep_first(&mut first_error, io::Error::new(err.kind(), format!("{}: {err}", path.display()))),
        }
        if meta.is_dir() {
            match std::fs::read_dir(&path) {
                Ok(entries) => stack.extend(entries.filter_map(|entry| entry.ok().map(|e| e.path()))),
                Err(err) => keep_first(&mut first_error, err),
            }
        }
    }
    match first_error {
        Some(err) => Err(err),
        None => Ok(removed),
    }
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

    /// A JRE has read-only files (CI: EACCES stopped the walk). Without the attribute they are left alone; with it,
    /// the attribute goes and the mode stays.
    #[cfg(target_os = "macos")]
    #[test]
    fn read_only_files_neither_stop_the_walk_nor_lose_their_mode() {
        use std::os::unix::fs::PermissionsExt;
        let dir = std::env::temp_dir().join(format!("asterium-quarantine-ro-{}", std::process::id()));
        std::fs::create_dir_all(dir.join("legal")).unwrap();
        let plain = dir.join("legal/LICENSE");
        let marked = dir.join("legal/ADDITIONAL_LICENSE_INFO");
        std::fs::write(&plain, b"x").unwrap();
        std::fs::write(&marked, b"x").unwrap();
        let status = std::process::Command::new("xattr")
            .args(["-w", QUARANTINE_ATTRIBUTE, "0081;00000000;Test;"])
            .arg(&marked)
            .status()
            .unwrap();
        assert!(status.success());
        for path in [&plain, &marked] {
            std::fs::set_permissions(path, std::fs::Permissions::from_mode(0o444)).unwrap();
        }
        assert_eq!(strip_quarantine(&dir).unwrap(), 1);
        for path in [&plain, &marked] {
            assert_eq!(std::fs::metadata(path).unwrap().permissions().mode() & 0o777, 0o444);
        }
        assert_eq!(strip_quarantine(&dir).unwrap(), 0);
        std::fs::remove_dir_all(&dir).unwrap();
    }
}
