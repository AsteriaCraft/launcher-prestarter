//! Path checks for archive entries: nothing may land outside the staging directory, neither through `..` or an
//! absolute path, nor through a symlink that points out of the tree.

use std::path::{Component, Path, PathBuf};

/// The entry path as a plain relative path, or `None` if it is absolute, has a drive or root, or climbs with `..`.
pub fn safe_relative(path: &Path) -> Option<PathBuf> {
    let mut out = PathBuf::new();
    for component in path.components() {
        match component {
            Component::Normal(part) => out.push(part),
            Component::CurDir => {}
            Component::ParentDir | Component::RootDir | Component::Prefix(_) => return None,
        }
    }
    (!out.as_os_str().is_empty()).then_some(out)
}

/// Whether a symlink stored at `entry` (relative to the archive root) with target `target` stays inside the root.
/// Only relative targets are allowed; `..` may climb up to, but not above, the root.
pub fn symlink_stays_inside(entry: &Path, target: &Path) -> bool {
    if target.as_os_str().is_empty()
        || target.has_root()
        || target.components().any(|c| matches!(c, Component::Prefix(_)))
    {
        return false;
    }
    let Some(entry) = safe_relative(entry) else { return false };
    let mut depth: Vec<_> = entry.parent().map(|p| p.components().collect()).unwrap_or_default();
    for component in target.components() {
        match component {
            Component::Normal(part) => depth.push(Component::Normal(part)),
            Component::CurDir => {}
            Component::ParentDir => {
                if depth.pop().is_none() {
                    return false;
                }
            }
            Component::RootDir | Component::Prefix(_) => return false,
        }
    }
    true
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn relative_paths() {
        assert_eq!(safe_relative(Path::new("jre/bin/java")), Some(PathBuf::from("jre/bin/java")));
        assert_eq!(safe_relative(Path::new("./jre/./bin")), Some(PathBuf::from("jre/bin")));
        for bad in ["../evil", "jre/../../evil", "/etc/passwd", "", "."] {
            assert_eq!(safe_relative(Path::new(bad)), None, "{bad}");
        }
        if cfg!(windows) {
            assert_eq!(safe_relative(Path::new(r"C:\Windows\evil.dll")), None);
            assert_eq!(safe_relative(Path::new(r"\\server\share\x")), None);
        }
    }

    #[test]
    fn symlinks() {
        let entry = Path::new("jre/legal/java.prefs/LICENSE");
        assert!(symlink_stays_inside(entry, Path::new("../java.base/LICENSE")));
        assert!(symlink_stays_inside(Path::new("jre/lib/libjli.dylib"), Path::new("../Contents/x")));
        assert!(!symlink_stays_inside(entry, Path::new("../../../../etc/passwd")));
        assert!(!symlink_stays_inside(entry, Path::new("/etc/passwd")));
        assert!(!symlink_stays_inside(Path::new("link"), Path::new("..")));
        assert!(!symlink_stays_inside(Path::new("../link"), Path::new("x")));
        assert!(!symlink_stays_inside(entry, Path::new("")));
    }
}
