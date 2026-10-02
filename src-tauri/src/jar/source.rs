//! Which jar the prestarter starts (ADR 0001). One binary decides at run time: if its own file ends in a jar, the
//! jar is embedded (Windows `Asterium*.exe`, Linux `Asterium_linux*`, built by the LaunchServer); otherwise it works
//! with a copy in the store (AppImage, macOS `.app`, or any raw prestarter started without a jar).

use std::path::{Path, PathBuf};

use super::inspect::{JarError, JarInfo, inspect_file};

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum JarSource {
    /// The prestarter's own file is the jar. Gravit updates it in place, so the prestarter must not keep running.
    Embedded { path: PathBuf, info: JarInfo },
    /// `<store>/launcher/Asterium.jar`, fetched by the prestarter and updated by Gravit as the `JAR` variant.
    Copy { path: PathBuf },
}

impl JarSource {
    pub fn path(&self) -> &Path {
        match self {
            JarSource::Embedded { path, .. } | JarSource::Copy { path } => path,
        }
    }

    pub fn is_embedded(&self) -> bool {
        matches!(self, JarSource::Embedded { .. })
    }
}

/// The path Gravit will see as its code source. On Windows `current_exe()` can come back as a verbatim `\\?\C:\…`
/// path; Gravit derives its own path and the update target from it, so the prefix is removed (`dunce`).
pub fn launcher_visible_path(path: &Path) -> PathBuf {
    dunce::simplified(path).to_path_buf()
}

/// Looks at `self_exe`: an embedded jar, no jar (copy mode), or a broken embedded jar (an error: the player must
/// download the file again; silently switching to a copy would turn an `.exe` into the `JAR` update variant).
pub fn detect(self_exe: &Path, copy_path: PathBuf) -> Result<JarSource, JarError> {
    match inspect_file(self_exe) {
        Ok(info) => Ok(JarSource::Embedded { path: launcher_visible_path(self_exe), info }),
        Err(JarError::NoJar) => Ok(JarSource::Copy { path: copy_path }),
        Err(err) => Err(err),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::jar::inspect::tests::{MANIFEST, jar_bytes};
    use crate::store::atomic::unique_suffix;
    use std::fs;

    fn temp(tag: &str, bytes: &[u8]) -> PathBuf {
        let path = std::env::temp_dir().join(format!("asterium-src-{tag}-{}", unique_suffix()));
        fs::write(&path, bytes).unwrap();
        path
    }

    #[test]
    fn embedded_copy_and_broken() {
        let copy = PathBuf::from("/store/launcher/Asterium.jar");
        let mut exe = vec![0x7fu8, b'E', b'L', b'F'];
        exe.extend(vec![0u8; 4096]);
        let bare = temp("bare", &exe);
        assert_eq!(detect(&bare, copy.clone()).unwrap(), JarSource::Copy { path: copy.clone() });

        exe.extend(jar_bytes(Some(MANIFEST), true));
        let with_jar = temp("jar", &exe);
        let source = detect(&with_jar, copy.clone()).unwrap();
        assert!(source.is_embedded());
        assert_eq!(source.path(), with_jar.as_path());

        let mut broken = vec![0u8; 100];
        broken.extend(jar_bytes(Some("Manifest-Version: 1.0\n"), true));
        let broken = temp("broken", &broken);
        assert!(matches!(detect(&broken, copy), Err(JarError::NoMainClass)));
        for p in [bare, with_jar, broken] {
            fs::remove_file(p).unwrap();
        }
    }

    #[test]
    fn verbatim_prefix_is_removed() {
        if cfg!(windows) {
            assert_eq!(
                launcher_visible_path(Path::new(r"\\?\C:\Ігри\Asterium.exe")),
                PathBuf::from(r"C:\Ігри\Asterium.exe")
            );
        }
        assert_eq!(launcher_visible_path(Path::new("/opt/Asterium_linux")), PathBuf::from("/opt/Asterium_linux"));
    }
}
